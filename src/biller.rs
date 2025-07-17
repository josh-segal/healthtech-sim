use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Duration;

use crate::config::Config;
use crate::logging::log_claim_event;
use crate::message::{ClaimEnvelope, ClaimMessage};
use crate::schema::PayerClaim;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Biller task that processes claims received over a PayerClaim channel.
///
/// For each incoming claim:
/// - Creates a one-time channel for receiving the remittance response.
/// - Spawns a listener task to handle the remittance asynchronously.
/// - Wraps the claim and response channel in a `ClaimEnvelope`.
/// - Sends the envelope to the clearinghouse via the `ClaimMessage` channel.
///
/// The ingest rate is controlled by the configured interval.
pub async fn run_biller(
    config: Config,
    mut rx: Receiver<PayerClaim>,
    tx: Sender<ClaimMessage>,
    test_notify: Option<Sender<String>>, //optional notification for remittance
    total_claims: usize,
    shutdown_tx: Sender<()>,
) -> anyhow::Result<()> {
    if config.ingest_rate == 0 {
        return Err(anyhow::anyhow!("Config ingest_rate must be non-zero"));
    }
    let interval = Duration::from_secs(config.ingest_rate);
    let mut ticker = tokio::time::interval(interval);
    let verbose = config.verbose;
    if verbose {
        log_claim_event("biller", "-", "start", "Starting biller task");
    }
    let remittances_received = Arc::new(AtomicUsize::new(0));
    let mut claims_sent = 0;

    while let Some(claim) = rx.recv().await {
        ticker.tick().await;
        claims_sent += 1;
        process_claim(
            claim,
            &tx,
            &test_notify,
            verbose,
            remittances_received.clone(),
            total_claims,
            shutdown_tx.clone(),
        ).await?;
        if claims_sent == total_claims {
            break;
        }
    }
    Ok(())
}

async fn process_claim(
    claim: PayerClaim,
    tx: &Sender<ClaimMessage>,
    test_notify: &Option<Sender<String>>,
    verbose: bool,
    remittances_received: Arc<AtomicUsize>,
    total_claims: usize,
    shutdown_tx: Sender<()>,
) -> anyhow::Result<()> {
    if verbose {
        log_claim_event(
            "biller",
            &claim.claim_id,
            "received_payer_claim",
            &format!("Received PayerClaim: Claim ID: {}", &claim.claim_id),
        );
    }
    let (rem_tx, rem_rx) = tokio::sync::mpsc::channel(1);
    let test_notify_opt = test_notify.clone();
    let claim_id = claim.claim_id.clone();
    tokio::spawn(listen_for_remittance(
        rem_rx,
        claim_id.clone(),
        test_notify_opt,
        verbose,
        remittances_received,
        total_claims,
        shutdown_tx.clone(),
    ));
    let envelope = ClaimEnvelope {
        claim,
        response_tx: rem_tx,
    };
    if verbose {
        log_claim_event(
            "biller",
            &claim_id,
            "sending_claim_envelope",
            &format!("Sending claim envelope to clearinghouse: {}", &claim_id),
        );
    }
    if tx.send(ClaimMessage::NewClaim(envelope)).await.is_err() {
        eprintln!("Clearinghouse dropped");
        return Err(anyhow::anyhow!("Clearinghouse channel dropped"));
    }
    Ok(())
}

async fn listen_for_remittance(
    mut rem_rx: tokio::sync::mpsc::Receiver<crate::message::RemittanceMessage>,
    claim_id: String,
    test_notify_opt: Option<Sender<String>>,
    verbose: bool,
    remittances_received: Arc<AtomicUsize>,
    total_claims: usize,
    shutdown_tx: Sender<()>,
) {
    if let Some(_response) = rem_rx.recv().await {
        if verbose {
            log_claim_event(
                "biller",
                &claim_id,
                "received_remittance",
                &format!("Received remittance for claim: {}", &claim_id),
            );
        }
        if let Some(tx) = test_notify_opt {
            let _ = tx.send(claim_id).await;
        }
        let count = remittances_received.fetch_add(1, Ordering::SeqCst) + 1;
        if count == total_claims {
                let _ = shutdown_tx.send(()).await;
            }
        }
    }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::RemittanceMessage;
    use crate::{remittance::mock_remittance, schema::mock_claim};

    /// Test that the biller task processes a claim, sends it to the clearinghouse, and receives a remittance notification.
    /// Expected: The claim is sent, remittance is received, and notification channel receives the correct claim ID.
    #[tokio::test]
    async fn test_run_biller() {
        // mock config
        let mock_config = Config {
            file_path: "mock_path.json".to_string(),
            ingest_rate: 1,
            verbose: true,
        };

        // input channel for claims
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);

        // output channel for ClaimMessage -> Clearinghouse
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(1);

        // notifcation channel
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(1);

        // spawn biller task
        tokio::spawn(async move {
            let _ = run_biller(mock_config, claim_rx, out_tx, Some(notify_tx), 1, None).await;
        });

        // send a mock claim
        let mock_claim = mock_claim();
        claim_tx.send(mock_claim).await.unwrap(); // panic if send fails

        // receive envelope sent to clearinghouse and assert correctness
        if let Some(ClaimMessage::NewClaim(envelope)) = out_rx.recv().await {
            assert_eq!(envelope.claim.claim_id, "abc123");

            // simulate remittance being returned
            let mock_remittance = mock_remittance();
            let _ = envelope
                .response_tx
                .send(RemittanceMessage::Processed(mock_remittance))
                .await;
        } else {
            panic!("Expected ClaimMessage::NewClaim");
        }

        let notified_id = notify_rx
            .recv()
            .await
            .expect("Expected remittance notification");
        assert_eq!(notified_id, "abc123");
    }

    /// Test that the biller returns an error if the clearinghouse channel is dropped.
    /// Expected: run_biller returns an error and does not panic.
    #[tokio::test]
    async fn test_biller_clearinghouse_channel_dropped() {
        let mock_config = Config {
            file_path: "mock_path.json".to_string(),
            ingest_rate: 1,
            verbose: false,
        };
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (out_tx, _out_rx) = tokio::sync::mpsc::channel(1);
        let (notify_tx, _notify_rx) = tokio::sync::mpsc::channel(1);
        // Spawn biller task, then drop the output channel to simulate clearinghouse down
        let biller_handle = tokio::spawn(async move {
            run_biller(mock_config, claim_rx, out_tx, Some(notify_tx), 1, None).await
        });
        // Drop the output channel after spawning
        // (out_tx is moved into the spawned task, so we can't drop it here)
        // Instead, we drop _out_rx so the channel is closed from the receiver side
        drop(_out_rx);
        let mock_claim = mock_claim();
        claim_tx.send(mock_claim).await.unwrap();
        let result = biller_handle.await.unwrap();
        assert!(
            result.is_err(),
            "Expected error when clearinghouse channel is dropped"
        );
    }

    /// Test that the biller sends a notification when a remittance is received for a claim.
    /// Expected: The notification channel receives the correct claim ID after remittance is sent.
    #[tokio::test]
    async fn test_biller_remittance_notification() {
        let mock_config = Config {
            file_path: "mock_path.json".to_string(),
            ingest_rate: 1,
            verbose: false,
        };
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(1);
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _ = run_biller(mock_config, claim_rx, out_tx, Some(notify_tx), 1, None).await;
        });
        let mock_claim = mock_claim();
        claim_tx.send(mock_claim.clone()).await.unwrap();
        if let Some(ClaimMessage::NewClaim(envelope)) = out_rx.recv().await {
            let mock_remittance = mock_remittance();
            let _ = envelope
                .response_tx
                .send(RemittanceMessage::Processed(mock_remittance))
                .await;
        }
        let notified_id = notify_rx
            .recv()
            .await
            .expect("Expected remittance notification");
        assert_eq!(notified_id, mock_claim.claim_id);
    }

    /// Test that the biller returns an error with a clear message if the config ingest_rate is zero.
    /// Expected: run_biller returns an error containing "ingest_rate must be non-zero".
    #[tokio::test]
    async fn test_biller_invalid_config() {
        let mock_config = Config {
            file_path: "mock_path.json".to_string(),
            ingest_rate: 0, // Invalid: zero interval
            verbose: false,
        };
        let (_claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (out_tx, _out_rx) = tokio::sync::mpsc::channel(1);
        let (notify_tx, _notify_rx) = tokio::sync::mpsc::channel(1);
        let result = run_biller(mock_config, claim_rx, out_tx, Some(notify_tx), 1, None).await;
        assert!(result.is_err(), "Expected error with invalid ingest_rate");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("ingest_rate must be non-zero"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    /// Test that the biller can process two claims with the same claim ID.
    /// Expected: Both claims are processed, and notifications are received for both claim IDs.
    #[tokio::test]
    async fn test_biller_duplicate_claim_ids() {
        let mock_config = Config {
            file_path: "mock_path.json".to_string(),
            ingest_rate: 1,
            verbose: false,
        };
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(2);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(2);
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(2);
        tokio::spawn(async move {
            let _ = run_biller(mock_config, claim_rx, out_tx, Some(notify_tx), 2, None).await;
        });
        let claim1 = mock_claim();
        let mut claim2 = mock_claim();
        claim2.patient.first_name = "Other".to_string(); // Different patient, same claim_id
        claim_tx.send(claim1.clone()).await.unwrap();
        claim_tx.send(claim2.clone()).await.unwrap();
        let mut received_ids = vec![];
        for _ in 0..2 {
            if let Some(ClaimMessage::NewClaim(envelope)) = out_rx.recv().await {
                let mock_remittance = mock_remittance();
                let _ = envelope
                    .response_tx
                    .send(RemittanceMessage::Processed(mock_remittance))
                    .await;
            }
            let notified_id = notify_rx
                .recv()
                .await
                .expect("Expected remittance notification");
            received_ids.push(notified_id);
        }
        assert_eq!(
            received_ids,
            vec![claim1.claim_id.clone(), claim2.claim_id.clone()]
        );
    }

    /// Test that the biller can process a claim with minimal/empty fields.
    /// Expected: The claim is sent, remittance is received, and notification channel receives the claim ID.
    #[tokio::test]
    async fn test_biller_empty_claim() {
        use crate::schema::{Insurance, Organization, Patient, PayerClaim, Provider, ServiceLine};
        let mock_config = Config {
            file_path: "mock_path.json".to_string(),
            ingest_rate: 1,
            verbose: false,
        };
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(1);
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _ = run_biller(mock_config, claim_rx, out_tx, Some(notify_tx), 1, None).await;
        });
        let empty_claim = PayerClaim {
            claim_id: "empty1".to_string(),
            place_of_service_code: 0,
            insurance: Insurance {
                payer_id: "".to_string(),
                patient_member_id: "".to_string(),
            },
            patient: Patient {
                first_name: "".to_string(),
                last_name: "".to_string(),
                gender: "".to_string(),
                dob: "".to_string(),
                email: None,
                address: None,
            },
            organization: Organization {
                name: "".to_string(),
                billing_npi: None,
                ein: None,
                contact: None,
                address: None,
            },
            rendering_provider: Provider {
                first_name: "".to_string(),
                last_name: "".to_string(),
                npi: "".to_string(),
            },
            service_lines: vec![ServiceLine {
                service_line_id: "".to_string(),
                procedure_code: "".to_string(),
                units: 0,
                details: "".to_string(),
                unit_charge_currency: "".to_string(),
                unit_charge_amount: 0.0,
                modifiers: None,
                do_not_bill: None,
            }],
        };
        claim_tx.send(empty_claim.clone()).await.unwrap();
        if let Some(ClaimMessage::NewClaim(envelope)) = out_rx.recv().await {
            let mock_remittance = mock_remittance();
            let _ = envelope
                .response_tx
                .send(RemittanceMessage::Processed(mock_remittance))
                .await;
        }
        let notified_id = notify_rx
            .recv()
            .await
            .expect("Expected remittance notification");
        assert_eq!(notified_id, empty_claim.claim_id);
    }
}
