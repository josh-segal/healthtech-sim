use healthtechsim::schema::{mock_claim, PayerClaim};
use healthtechsim::message::{ClaimMessage, PayerMessage, RemittanceMessage};
use healthtechsim::biller::run_biller;
use healthtechsim::clearinghouse::Clearinghouse;
use healthtechsim::payer::Payer;
use healthtechsim::config::Config;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::timeout;
use std::time::Duration;

/// Test the full claim lifecycle: reader -> biller -> clearinghouse -> payer -> remittance back to biller.
/// Expected: The remittance matches the original claim, and all modules interact as expected.
#[tokio::test]
async fn test_full_claim_lifecycle_happy_path() {
    // Setup config
    let config = Config {
        file_path: "mock_path.json".to_string(),
        ingest_rate: 1,
        verbose: false,
    };
    // Channels
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(1);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(1);
    let (payer_tx, payer_rx) = tokio::sync::mpsc::channel::<PayerMessage>(1);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(1);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));
    
    // Create a notification channel to track when biller receives remittance
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<String>(1);
    
    // Spawn biller
    tokio::spawn(run_biller(config.clone(), claim_input_rx, claim_tx, Some(notify_tx)));
    
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
    tokio::spawn(async move { clearinghouse.run().await; });
    
    // Spawn payer
    let payer = Payer::new(
        "medicare".to_string(),
        1,
        2,
        remit_tx,
        payer_rx,
        false,
    );
    tokio::spawn(async move { payer.run().await; });
    
    // Send a mock claim
    let claim = mock_claim();
    claim_input_tx.send(claim.clone()).await.unwrap();
    
    // Wait for biller to receive remittance notification
    let received_claim_id = timeout(Duration::from_secs(5), notify_rx.recv())
        .await
        .expect("Timeout waiting for remittance notification")
        .expect("Expected remittance notification");
    
    assert_eq!(received_claim_id, claim.claim_id);
    
    // Verify the claim was processed by checking history
    let history = remittance_history.lock().await;
    assert!(history.contains_key(&claim.claim_id), "Claim should be in history");
}

/// Test multiple claims for different payers, ensuring correct routing and remittance delivery.
/// Expected: Each claim is routed to the correct payer, and remittances are returned to the correct biller.
#[tokio::test]
async fn test_multiple_claims_and_payers() {
    let config = Config {
        file_path: "mock_path.json".to_string(),
        ingest_rate: 1,
        verbose: false,
    };
    let (claim_input_tx, claim_input_rx) = tokio::sync::mpsc::channel::<PayerClaim>(2);
    let (claim_tx, claim_rx) = tokio::sync::mpsc::channel::<ClaimMessage>(2);
    let (payer1_tx, payer1_rx) = tokio::sync::mpsc::channel::<PayerMessage>(2);
    let (payer2_tx, payer2_rx) = tokio::sync::mpsc::channel::<PayerMessage>(2);
    let (remit_tx, remit_rx) = tokio::sync::mpsc::channel::<RemittanceMessage>(2);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));
    
    // Create a notification channel to track when biller receives remittances
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<String>(2);
    
    // Spawn biller
    tokio::spawn(run_biller(config.clone(), claim_input_rx, claim_tx, Some(notify_tx)));
    
    // Spawn clearinghouse
    let mut payer_txs = HashMap::new();
    payer_txs.insert("medicare".to_string(), payer1_tx);
    payer_txs.insert("anthem".to_string(), payer2_tx);
    let clearinghouse = Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        false,
    );
    tokio::spawn(async move { clearinghouse.run().await; });
    
    // Spawn payers
    let payer1 = Payer::new(
        "medicare".to_string(),
        1,
        2,
        remit_tx.clone(),
        payer1_rx,
        false,
    );
    let payer2 = Payer::new(
        "anthem".to_string(),
        1,
        2,
        remit_tx.clone(),
        payer2_rx,
        false,
    );
    tokio::spawn(async move { payer1.run().await; });
    tokio::spawn(async move { payer2.run().await; });
    
    // Send two claims for different payers
    let mut claim1 = mock_claim();
    claim1.insurance.payer_id = "medicare".to_string();
    let mut claim2 = mock_claim();
    claim2.claim_id = "claim2".to_string();
    claim2.insurance.payer_id = "anthem".to_string();
    
    claim_input_tx.send(claim1.clone()).await.unwrap();
    claim_input_tx.send(claim2.clone()).await.unwrap();
    
    // Wait for both remittance notifications
    let received_claim_id1 = timeout(Duration::from_secs(5), notify_rx.recv())
        .await
        .expect("Timeout waiting for remittance notification 1")
        .expect("Expected remittance notification 1");
    
    let received_claim_id2 = timeout(Duration::from_secs(5), notify_rx.recv())
        .await
        .expect("Timeout waiting for remittance notification 2")
        .expect("Expected remittance notification 2");
    
    assert_eq!(received_claim_id1, claim1.claim_id);
    assert_eq!(received_claim_id2, claim2.claim_id);
    
    // Verify both claims were processed by checking history
    let history = remittance_history.lock().await;
    assert!(history.contains_key(&claim1.claim_id), "Claim1 should be in history");
    assert!(history.contains_key(&claim2.claim_id), "Claim2 should be in history");
} 