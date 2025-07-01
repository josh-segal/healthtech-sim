use rand::Rng;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::sleep;

use crate::logging::log_claim_event;
use crate::message::{PayerMessage, RemittanceMessage};
use crate::remittance::Remittance;

/// Simulates an insurance payer for claim adjudication
/// 
/// Processes claims asynchronously with configurable response times
/// Generates remittances with payment breakdowns
pub struct Payer {
    payer_id: String,
    min_response_time_secs: u64,
    max_response_time_secs: u64,
    rx: Receiver<PayerMessage>,
    tx: Sender<RemittanceMessage>,
    verbose: bool,
}

impl Payer {
    /// Create a new payer with specified response time range
    pub fn new(
        payer_id: String,
        min_response_time_secs: u64,
        max_response_time_secs: u64,
        tx: Sender<RemittanceMessage>,
        rx: Receiver<PayerMessage>,
        verbose: bool,
    ) -> Self {
        Self {
            payer_id,
            min_response_time_secs,
            max_response_time_secs,
            tx,
            rx,
            verbose,
        }
    }

    /// Main processing loop for claim adjudication
    /// 
    /// Receives claims, processes them asynchronously with random delays
    /// Generates and validates remittances before sending responses
    pub async fn run(mut self) {
        if self.verbose {
            log_claim_event(
                "payer",
                "-",
                "start",
                &format!("Starting payer task for {}", &self.payer_id),
            );
        }
        while let Some(msg) = self.rx.recv().await {
            if let PayerMessage::Adjudicate(claim) = msg {
                if self.verbose {
                    log_claim_event(
                        "payer",
                        &claim.claim_id,
                        "received_for_adjudication",
                        &format!("Received claim for adjudication: {}", &claim.claim_id),
                    );
                    log_claim_event(
                        "payer",
                        &claim.claim_id,
                        "adjudicating",
                        &format!("Adjudicating claim: {}", &claim.claim_id),
                    );
                }
                let delay = self.random_delay();
                let tx = self.tx.clone();
                let verbose = self.verbose;
                tokio::spawn(async move {
                    sleep(delay).await;
                    let remittance = Remittance::from_claim(&claim);
                    if verbose {
                        log_claim_event(
                            "payer",
                            &claim.claim_id,
                            "finished_adjudication",
                            &format!("Finished adjudication for claim: {}", &claim.claim_id),
                        );
                        log_claim_event(
                            "payer",
                            &claim.claim_id,
                            "sending_remittance",
                            &format!("Sending remittance for claim: {}", &claim.claim_id),
                        );
                    }
                    //assert sum of billed amounts equals billed amount in payer claim
                    match remittance.validate_against_claim(&claim) {
                        Ok(()) => {
                            if verbose {
                                log_claim_event(
                                    "payer",
                                    &claim.claim_id,
                                    "remittance_valid",
                                    "Remittance is valid!",
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Remittance validation error: {}", e);
                        }
                    }
                    let _ = tx.send(RemittanceMessage::Processed(remittance)).await;
                });
            }
        }
        if self.verbose {
            log_claim_event(
                "payer",
                "-",
                "shutdown",
                &format!("Shutting down payer task for {}", &self.payer_id),
            );
        }
    }

    /// Generate a random processing delay within configured range
    fn random_delay(&self) -> Duration {
        let mut rng = rand::rng();
        let secs = rng.random_range(self.min_response_time_secs..=self.max_response_time_secs);
        Duration::from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::mock_claim;
    use tokio::time::timeout;

    /// Test that a claim is adjudicated and remittance is sent.
    /// Expected: Remittance is sent and matches the claim.
    #[tokio::test]
    async fn test_run_payer() {
        // Create channels for communication
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, mut remittance_rx) = tokio::sync::mpsc::channel(1);

        // Create payer with short response times for testing
        let payer = Payer::new(
            "medicare".to_string(),
            1, // min response time 1 second
            2, // max response time 2 seconds
            remittance_tx,
            payer_rx,
            true,
        );

        // Spawn payer task
        tokio::spawn(async move {
            payer.run().await;
        });

        // Create a mock claim
        let mock_claim = mock_claim();

        // Send claim to payer for adjudication
        payer_tx
            .send(PayerMessage::Adjudicate(mock_claim.clone()))
            .await
            .unwrap();

        // Wait for remittance response with timeout
        let timeout_duration = Duration::from_secs(5);
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance))) => {
                // Verify the remittance matches the claim
                assert_eq!(remittance.claim_id, mock_claim.claim_id);
                assert_eq!(
                    remittance.service_line_remittances.len(),
                    mock_claim.service_lines.len()
                );

                // Verify service line remittances correspond to service lines
                for (i, service_line) in mock_claim.service_lines.iter().enumerate() {
                    let remittance_line = &remittance.service_line_remittances[i];
                    assert_eq!(
                        remittance_line.service_line_id,
                        service_line.service_line_id
                    );

                    // Verify amounts are calculated (should be non-zero for our mock calculation)
                    let total_charge = service_line.unit_charge_amount * service_line.units as f64;
                    assert!(remittance_line.payer_paid_amount > 0.0);
                    assert!(remittance_line.coinsurance_amount > 0.0);
                    assert!(remittance_line.copay_amount > 0.0);
                    assert!(remittance_line.deductible_amount > 0.0);
                    assert!(remittance_line.not_allowed_amount > 0.0);

                    // Verify total amounts add up to approximately the total charge
                    let total_remitted = remittance_line.payer_paid_amount
                        + remittance_line.coinsurance_amount
                        + remittance_line.copay_amount
                        + remittance_line.deductible_amount
                        + remittance_line.not_allowed_amount;

                    // Allow for small floating point differences
                    assert!((total_remitted - total_charge).abs() < 0.01);
                }
            }
            Ok(None) => {
                panic!("Expected remittance response but got None");
            }
            Err(_) => {
                panic!("Timeout waiting for remittance response");
            }
        }
    }

    /// Test that multiple claims are processed and remittances are sent for each.
    /// Expected: Remittances are sent for all claims.
    #[tokio::test]
    async fn test_payer_multiple_claims() {
        // Create channels for communication
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(10);
        let (remittance_tx, mut remittance_rx) = tokio::sync::mpsc::channel(10);

        // Create payer with very short response times for testing
        let payer = Payer::new(
            "medicare".to_string(),
            1, // min response time 1 second
            1, // max response time 1 second (fixed for predictable testing)
            remittance_tx,
            payer_rx,
            true,
        );

        // Spawn payer task
        tokio::spawn(async move {
            payer.run().await;
        });

        // Send multiple claims
        let claim1 = mock_claim();
        let claim2 = mock_claim(); // This will have the same ID, but that's okay for testing

        payer_tx
            .send(PayerMessage::Adjudicate(claim1.clone()))
            .await
            .unwrap();
        payer_tx
            .send(PayerMessage::Adjudicate(claim2.clone()))
            .await
            .unwrap();

        // Wait for both remittances
        let timeout_duration = Duration::from_secs(10);

        // First remittance
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance1))) => {
                assert_eq!(remittance1.claim_id, claim1.claim_id);
            }
            _ => panic!("Timeout or error waiting for first remittance"),
        }

        // Second remittance
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance2))) => {
                assert_eq!(remittance2.claim_id, claim2.claim_id);
            }
            _ => panic!("Timeout or error waiting for second remittance"),
        }
    }

    /// Test that invalid claims (e.g., with missing fields or zero amounts) are handled and do not panic.
    /// Expected: Remittance is still sent, but may be zeroed or error is logged.
    #[tokio::test]
    async fn test_payer_invalid_claim() {
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, mut remittance_rx) = tokio::sync::mpsc::channel(1);
        let payer = Payer::new("medicare".to_string(), 1, 1, remittance_tx, payer_rx, false);
        tokio::spawn(async move {
            payer.run().await;
        });
        let mut invalid_claim = mock_claim();
        // Set zero amounts to test edge case
        for service_line in &mut invalid_claim.service_lines {
            service_line.unit_charge_amount = 0.0;
            service_line.units = 0;
        }
        payer_tx
            .send(PayerMessage::Adjudicate(invalid_claim.clone()))
            .await
            .unwrap();
        let timeout_duration = Duration::from_secs(5);
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance))) => {
                assert_eq!(remittance.claim_id, invalid_claim.claim_id);
                // Verify all amounts are zero
                for remittance_line in &remittance.service_line_remittances {
                    assert_eq!(remittance_line.payer_paid_amount, 0.0);
                    assert_eq!(remittance_line.coinsurance_amount, 0.0);
                    assert_eq!(remittance_line.copay_amount, 0.0);
                    assert_eq!(remittance_line.deductible_amount, 0.0);
                    assert_eq!(remittance_line.not_allowed_amount, 0.0);
                }
            }
            _ => panic!("Expected remittance response"),
        }
    }

    /// Test that payer exits gracefully if the channel is closed.
    /// Expected: Task exits without panic.
    #[tokio::test]
    async fn test_payer_channel_closed() {
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, _remittance_rx) = tokio::sync::mpsc::channel(1);
        let payer = Payer::new("medicare".to_string(), 1, 2, remittance_tx, payer_rx, false);
        let payer_handle = tokio::spawn(async move {
            payer.run().await;
        });
        // Drop the sender to close the channel
        drop(payer_tx);
        // Should not panic, just exit gracefully
        let result = payer_handle.await;
        assert!(result.is_ok());
    }

    /// Test that payer handles claims with empty service lines gracefully.
    /// Expected: Remittance is sent with empty service line remittances.
    #[tokio::test]
    async fn test_payer_empty_service_lines() {
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, mut remittance_rx) = tokio::sync::mpsc::channel(1);
        let payer = Payer::new("medicare".to_string(), 1, 1, remittance_tx, payer_rx, false);
        tokio::spawn(async move {
            payer.run().await;
        });
        let mut empty_claim = mock_claim();
        empty_claim.service_lines.clear();
        payer_tx
            .send(PayerMessage::Adjudicate(empty_claim.clone()))
            .await
            .unwrap();
        let timeout_duration = Duration::from_secs(5);
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance))) => {
                assert_eq!(remittance.claim_id, empty_claim.claim_id);
                assert_eq!(remittance.service_line_remittances.len(), 0);
            }
            _ => panic!("Expected remittance response"),
        }
    }

    /// Test that payer handles claims with very large amounts without overflow.
    /// Expected: Remittance is calculated correctly for large amounts.
    #[tokio::test]
    async fn test_payer_large_amounts() {
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, mut remittance_rx) = tokio::sync::mpsc::channel(1);
        let payer = Payer::new("medicare".to_string(), 1, 1, remittance_tx, payer_rx, false);
        tokio::spawn(async move {
            payer.run().await;
        });
        let mut large_claim = mock_claim();
        for service_line in &mut large_claim.service_lines {
            service_line.unit_charge_amount = 1_000_000.0;
            service_line.units = 10;
        }
        payer_tx
            .send(PayerMessage::Adjudicate(large_claim.clone()))
            .await
            .unwrap();
        let timeout_duration = Duration::from_secs(5);
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance))) => {
                assert_eq!(remittance.claim_id, large_claim.claim_id);
                for (i, service_line) in large_claim.service_lines.iter().enumerate() {
                    let remittance_line = &remittance.service_line_remittances[i];
                    let total_charge = service_line.unit_charge_amount * service_line.units as f64;
                    let total_remitted = remittance_line.payer_paid_amount
                        + remittance_line.coinsurance_amount
                        + remittance_line.copay_amount
                        + remittance_line.deductible_amount
                        + remittance_line.not_allowed_amount;
                    assert!((total_remitted - total_charge).abs() < 0.01);
                }
            }
            _ => panic!("Expected remittance response"),
        }
    }

    /// Test that payer respects the configured response time range.
    /// Expected: Response times fall within the configured min/max range.
    #[tokio::test]
    async fn test_payer_response_time_range() {
        let (payer_tx, payer_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, mut remittance_rx) = tokio::sync::mpsc::channel(1);
        let payer = Payer::new(
            "medicare".to_string(),
            1, // min 1 second
            2, // max 2 seconds
            remittance_tx,
            payer_rx,
            false,
        );
        tokio::spawn(async move {
            payer.run().await;
        });
        let claim = mock_claim();
        let start_time = std::time::Instant::now();
        payer_tx
            .send(PayerMessage::Adjudicate(claim))
            .await
            .unwrap();
        let _remittance = remittance_rx.recv().await.expect("Expected remittance");
        let elapsed = start_time.elapsed();
        // Should take at least 1 second (min response time)
        assert!(elapsed >= Duration::from_secs(1));
        // Should not take more than 3 seconds (max + buffer)
        assert!(elapsed <= Duration::from_secs(3));
    }
}
