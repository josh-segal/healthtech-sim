use healthtechsim::biller::run_biller;
use healthtechsim::clearinghouse::Clearinghouse;
use healthtechsim::config::Config;
use healthtechsim::message::{ClaimMessage, PayerMessage, RemittanceMessage};
use healthtechsim::payer::Payer;
use healthtechsim::reader::stream_claims;
use healthtechsim::schema::{PayerClaim, mock_claim};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Test that a claim flows correctly through Reader -> Biller -> Clearinghouse -> Payer -> Remittance back to Biller
/// This is the core data flow integrity test
#[tokio::test]
async fn test_core_data_flow_integrity() {
    // Create a temporary file with a claim
    let mut tmpfile = NamedTempFile::new().unwrap();
    let claim = mock_claim();
    let json = serde_json::to_string(&claim).unwrap();
    writeln!(tmpfile, "{}", json).unwrap();

    let config = Config {
        file_path: tmpfile.path().to_str().unwrap().to_string(),
        ingest_rate: 1,
        verbose: false,
    };

    // Set up channels
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(1);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(1);
    let (payer_tx, payer_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(1);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // Notification channel to track when biller receives remittance
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<String>(1);

    // Spawn biller
    tokio::spawn(run_biller(
        config.clone(),
        claim_input_rx,
        claim_tx,
        Some(notify_tx),
    ));

    // Spawn reader
    tokio::spawn(async move {
        let _ = stream_claims(&config.file_path, claim_input_tx, false).await;
    });

    // Spawn clearinghouse
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), payer_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // Spawn payer
    let payer = Payer::new("medicare".to_string(), 1, 2, remit_tx, payer_rx, false);
    tokio::spawn(async move {
        payer.run().await;
    });

    // Wait for biller to receive remittance notification
    let received_claim_id = timeout(Duration::from_secs(10), notify_rx.recv())
        .await
        .expect("Timeout waiting for remittance notification")
        .expect("Expected remittance notification");

    assert_eq!(received_claim_id, claim.claim_id, "Claim ID should match");

    // Verify the claim was processed by checking history
    let history = remittance_history.lock().await;
    assert!(
        history.contains_key(&claim.claim_id),
        "Claim should be in history"
    );

    // Verify claim status transitioned to Remitted
    match history.get(&claim.claim_id) {
        Some(healthtechsim::message::ClaimStatus::Remitted(_)) => {
            // Success - claim was properly remitted
        }
        _ => panic!("Claim should be in Remitted status"),
    }
}

/// Test that remittance amounts match the original claim's billed amounts
#[tokio::test]
async fn test_remittance_amount_validation() {
    let config = Config {
        file_path: "mock_path.json".to_string(),
        ingest_rate: 1,
        verbose: false,
    };

    // Set up channels
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(1);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(1);
    let (payer_tx, payer_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(1);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // Spawn biller
    tokio::spawn(run_biller(config.clone(), claim_input_rx, claim_tx, None));

    // Spawn clearinghouse
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), payer_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // Spawn payer
    let payer = Payer::new("medicare".to_string(), 1, 2, remit_tx, payer_rx, false);
    tokio::spawn(async move {
        payer.run().await;
    });

    // Send a claim with known amounts
    let mut claim = mock_claim();
    claim.service_lines[0].unit_charge_amount = 100.0;
    claim.service_lines[0].units = 2;
    claim_input_tx.send(claim.clone()).await.unwrap();

    // Wait for processing and check history
    tokio::time::sleep(Duration::from_secs(3)).await;

    let history = remittance_history.lock().await;
    match history.get(&claim.claim_id) {
        Some(healthtechsim::message::ClaimStatus::Remitted(record)) => {
            let remittance = &record.remittance;

            // Verify remittance validation passes
            assert!(
                remittance.validate_against_claim(&claim).is_ok(),
                "Remittance should validate against claim"
            );

            // Verify amounts add up to billed amount
            let billed_amount =
                claim.service_lines[0].unit_charge_amount * claim.service_lines[0].units as f64;
            let remittance_line = &remittance.service_line_remittances[0];
            let total_remitted = remittance_line.payer_paid_amount
                + remittance_line.coinsurance_amount
                + remittance_line.copay_amount
                + remittance_line.deductible_amount
                + remittance_line.not_allowed_amount;

            // Allow for small floating point differences
            assert!(
                (total_remitted - billed_amount).abs() < 0.01,
                "Total remitted amount should equal billed amount"
            );
        }
        _ => panic!("Claim should be remitted"),
    }
}

/// Test that claims are routed to the correct payer based on payer_id
#[tokio::test]
async fn test_claim_routing_to_correct_payer() {
    let config = Config {
        file_path: "mock_path.json".to_string(),
        ingest_rate: 1,
        verbose: false,
    };

    // Set up channels
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(2);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(2);
    let (medicare_tx, medicare_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (anthem_tx, anthem_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(2);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // Spawn biller
    tokio::spawn(run_biller(config.clone(), claim_input_rx, claim_tx, None));

    // Spawn clearinghouse with multiple payers
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), medicare_tx);
    payer_txs.insert("anthem".to_string(), anthem_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // Spawn payers
    let medicare_payer = Payer::new(
        "medicare".to_string(),
        1,
        2,
        remit_tx.clone(),
        medicare_rx,
        false,
    );
    let anthem_payer = Payer::new(
        "anthem".to_string(),
        1,
        2,
        remit_tx.clone(),
        anthem_rx,
        false,
    );
    tokio::spawn(async move {
        medicare_payer.run().await;
    });
    tokio::spawn(async move {
        anthem_payer.run().await;
    });

    // Send claims for different payers
    let mut medicare_claim = mock_claim();
    medicare_claim.claim_id = "medicare_claim".to_string();
    medicare_claim.insurance.payer_id = "medicare".to_string();

    let mut anthem_claim = mock_claim();
    anthem_claim.claim_id = "anthem_claim".to_string();
    anthem_claim.insurance.payer_id = "anthem".to_string();

    claim_input_tx.send(medicare_claim.clone()).await.unwrap();
    claim_input_tx.send(anthem_claim.clone()).await.unwrap();

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify both claims were processed
    let history = remittance_history.lock().await;
    assert!(
        history.contains_key(&medicare_claim.claim_id),
        "Medicare claim should be processed"
    );
    assert!(
        history.contains_key(&anthem_claim.claim_id),
        "Anthem claim should be processed"
    );

    // Verify both claims are in Remitted status
    match history.get(&medicare_claim.claim_id) {
        Some(healthtechsim::message::ClaimStatus::Remitted(_)) => {}
        _ => panic!("Medicare claim should be remitted"),
    }

    match history.get(&anthem_claim.claim_id) {
        Some(healthtechsim::message::ClaimStatus::Remitted(_)) => {}
        _ => panic!("Anthem claim should be remitted"),
    }
}

/// Test that invalid JSON claims are skipped without breaking the stream
#[tokio::test]
async fn test_invalid_json_handling() {
    // Create a temporary file with valid and invalid JSON
    let mut tmpfile = NamedTempFile::new().unwrap();
    let claim = mock_claim();
    let json = serde_json::to_string(&claim).unwrap();

    // Write invalid JSON, then valid JSON
    writeln!(tmpfile, "{{ invalid json }}").unwrap();
    writeln!(tmpfile, "not json at all").unwrap();
    writeln!(tmpfile, "{}", json).unwrap();
    writeln!(tmpfile, "{{ another invalid }}").unwrap();

    let config = Config {
        file_path: tmpfile.path().to_str().unwrap().to_string(),
        ingest_rate: 1,
        verbose: false,
    };

    // Set up channels
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(1);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(1);
    let (payer_tx, payer_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(1);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // Notification channel
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<String>(1);

    // Spawn biller
    tokio::spawn(run_biller(
        config.clone(),
        claim_input_rx,
        claim_tx,
        Some(notify_tx),
    ));

    // Spawn reader
    tokio::spawn(async move {
        let _ = stream_claims(&config.file_path, claim_input_tx, false).await;
    });

    // Spawn clearinghouse
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), payer_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // Spawn payer
    let payer = Payer::new("medicare".to_string(), 1, 2, remit_tx, payer_rx, false);
    tokio::spawn(async move {
        payer.run().await;
    });

    // Wait for the valid claim to be processed
    let received_claim_id = timeout(Duration::from_secs(10), notify_rx.recv())
        .await
        .expect("Timeout waiting for remittance notification")
        .expect("Expected remittance notification");

    assert_eq!(
        received_claim_id, claim.claim_id,
        "Only the valid claim should be processed"
    );

    // Verify only the valid claim is in history
    let history = remittance_history.lock().await;
    assert_eq!(history.len(), 1, "Only one claim should be in history");
    assert!(
        history.contains_key(&claim.claim_id),
        "Valid claim should be in history"
    );
}

/// Test that unknown payer IDs are handled gracefully
#[tokio::test]
async fn test_unknown_payer_handling() {
    let config = Config {
        file_path: "mock_path.json".to_string(),
        ingest_rate: 1,
        verbose: false,
    };

    // Set up channels
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(1);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(1);
    let (payer_tx, payer_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(1);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // Spawn biller
    tokio::spawn(run_biller(config.clone(), claim_input_rx, claim_tx, None));

    // Spawn clearinghouse with only medicare payer
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), payer_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // Spawn payer
    let payer = Payer::new("medicare".to_string(), 1, 2, remit_tx, payer_rx, false);
    tokio::spawn(async move {
        payer.run().await;
    });

    // Send a claim with unknown payer
    let mut claim = mock_claim();
    claim.insurance.payer_id = "unknown_payer".to_string();
    claim_input_tx.send(claim.clone()).await.unwrap();

    // Wait a bit for processing
    tokio::time::sleep(Duration::from_secs(2)).await;

    // The claim should be in history but not remitted (since unknown payer)
    let history = remittance_history.lock().await;
    match history.get(&claim.claim_id) {
        Some(healthtechsim::message::ClaimStatus::Submitted { .. }) => {
            // Success - claim was submitted but not remitted due to unknown payer
        }
        _ => panic!("Claim should be in Submitted status due to unknown payer"),
    }
}

/// Test that multiple claims can be processed concurrently
#[tokio::test]
async fn test_concurrent_claim_processing() {
    let config = Config {
        file_path: "mock_path.json".to_string(),
        ingest_rate: 1,
        verbose: false,
    };

    // Set up channels with larger buffers
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(10);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(10);
    let (payer_tx, payer_rx) = tokio::sync::mpsc::channel::<PayerMessage>(10);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(10);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // Notification channel
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<String>(10);

    // Spawn biller
    tokio::spawn(run_biller(
        config.clone(),
        claim_input_rx,
        claim_tx,
        Some(notify_tx),
    ));

    // Spawn clearinghouse
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), payer_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // Spawn payer
    let payer = Payer::new("medicare".to_string(), 1, 2, remit_tx, payer_rx, false);
    tokio::spawn(async move {
        payer.run().await;
    });

    // Send multiple claims concurrently
    let mut claims = Vec::new();
    for i in 0..5 {
        let mut claim = mock_claim();
        claim.claim_id = format!("claim_{}", i);
        claims.push(claim.clone());
        claim_input_tx.send(claim).await.unwrap();
    }

    // Wait for all claims to be processed
    let mut received_claim_ids = Vec::new();
    for _ in 0..5 {
        let claim_id = timeout(Duration::from_secs(15), notify_rx.recv())
            .await
            .expect("Timeout waiting for remittance notification")
            .expect("Expected remittance notification");
        received_claim_ids.push(claim_id);
    }

    // Verify all claims were processed
    assert_eq!(
        received_claim_ids.len(),
        5,
        "All 5 claims should be processed"
    );

    let history = remittance_history.lock().await;
    for claim in &claims {
        assert!(
            history.contains_key(&claim.claim_id),
            "Claim {} should be in history",
            claim.claim_id
        );
    }
}
