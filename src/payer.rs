use std::time::Duration;
use rand::Rng;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::sleep;

use crate::remittance::Remittance;
use crate::message::{PayerMessage, RemittanceMessage};

pub struct Payer {
    payer_id: String,
    min_response_time_secs: u64,
    max_response_time_secs: u64,
    rx: Receiver<PayerMessage>,
    tx: Sender<RemittanceMessage>,
}

impl Payer {
    pub fn new(
        payer_id: String,
        min_response_time_secs: u64,
        max_response_time_secs: u64,
        tx: Sender<RemittanceMessage>,
        rx: Receiver<PayerMessage>,
    ) -> Self {
        Self {
            payer_id,
            min_response_time_secs,
            max_response_time_secs,
            tx,
            rx,
        }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            if let PayerMessage::Adjudicate(claim) = msg {
                let delay = self.random_delay();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    sleep(delay).await;
                    let remittance = Remittance::from_claim(&claim);

                    //assert sum of billed amounts equals billed amount in payer claim
                    match remittance.validate_against_claim(&claim) {
                        Ok(()) => {
                            println!("Remittance is valid!");
                        }
                        Err(e) => {
                            eprintln!("Remittance validation error: {}", e);
                            //TODO: panic or no?
                        }
                    }

                    let _ = tx.send(RemittanceMessage::Processed(remittance)).await;
                });
            }
        }
    }

    fn random_delay(&self) -> Duration {
        let mut rng = rand::rng();
        let secs = rng.random_range(self.min_response_time_secs..=self.max_response_time_secs);
        Duration::from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    // TODO: clean up comments
    use super::*;
    use crate::schema::mock_claim;
    use tokio::time::timeout;

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
        );

        // Spawn payer task
        tokio::spawn(async move {
            payer.run().await;
        });

        // Create a mock claim
        let mock_claim = mock_claim();

        // Send claim to payer for adjudication
        payer_tx.send(PayerMessage::Adjudicate(mock_claim.clone())).await.unwrap();

        // Wait for remittance response with timeout
        let timeout_duration = Duration::from_secs(5);
        match timeout(timeout_duration, remittance_rx.recv()).await {
            Ok(Some(RemittanceMessage::Processed(remittance))) => {
                // Verify the remittance matches the claim
                assert_eq!(remittance.claim_id, mock_claim.claim_id);
                assert_eq!(remittance.service_line_remittances.len(), mock_claim.service_lines.len());
                
                // Verify service line remittances correspond to service lines
                for (i, service_line) in mock_claim.service_lines.iter().enumerate() {
                    let remittance_line = &remittance.service_line_remittances[i];
                    assert_eq!(remittance_line.service_line_id, service_line.service_line_id);
                    
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
        );

        // Spawn payer task
        tokio::spawn(async move {
            payer.run().await;
        });

        // Send multiple claims
        let claim1 = mock_claim();
        let claim2 = mock_claim(); // This will have the same ID, but that's okay for testing

        payer_tx.send(PayerMessage::Adjudicate(claim1.clone())).await.unwrap();
        payer_tx.send(PayerMessage::Adjudicate(claim2.clone())).await.unwrap();

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
}


