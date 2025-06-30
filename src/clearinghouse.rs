use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc::{Receiver, Sender}, Mutex};

use crate::message::{ClaimEnvelope, ClaimMessage, ClaimStatus, PayerMessage, RemittanceMessage};
use crate::remittance::{Remittance, RemittanceRecord};

pub struct Clearinghouse {
    claim_rx: Receiver<ClaimMessage>,
    payer_txs: HashMap<String, Sender<PayerMessage>>,
    remittance_rx: Receiver<RemittanceMessage>,
    biller_txs: Arc<Mutex<HashMap<String, Sender<RemittanceMessage>>>>,
    history: Arc<Mutex<HashMap<String, ClaimStatus>>>,
}

impl Clearinghouse {
    pub fn new(
        claim_rx: Receiver<ClaimMessage>,
        payer_txs: HashMap<String, Sender<PayerMessage>>,
        remittance_rx: Receiver<RemittanceMessage>,
        biller_txs: Arc<Mutex<HashMap<String, Sender<RemittanceMessage>>>>,
        history: Arc<Mutex<HashMap<String, ClaimStatus>>>,
    ) -> Self {
        Self {
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs,
            history,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.claim_rx.recv() => {
                    if let ClaimMessage::NewClaim(envelope) = msg { // redundent unless add more ClaimMessage variants... leaving for extensibility
                        self.handle_claim(envelope).await;
                    }
                }
                Some(msg) = self.remittance_rx.recv() => {
                    if let RemittanceMessage::Processed(remittance) = msg { // redundent unless add more RemittanceMessage variants... leaving for extensibility
                        self.handle_remittance(remittance).await;
                    }
                }
                else => {
                    break;
                }
            }
        }
    }


    async fn handle_claim(&mut self, envelope: ClaimEnvelope) {
        let claim = envelope.claim;
        let response_tx = envelope.response_tx;
        let claim_id = claim.claim_id.clone();
        let payer_id = claim.insurance.payer_id.clone();

        //TODO: any faster way than locks here?
        // Track response channel for later
        self.biller_txs.lock().await.insert(claim_id.clone(), response_tx);

        // Track for AR aging
        self.history.lock().await.insert(claim_id.clone(), ClaimStatus::Submitted {
            claim: claim.clone(),
            submitted_at: Instant::now(),
        });

        // Forward claim to payer
        if let Some(payer_tx) = self.payer_txs.get(&payer_id) {
            if let Err(e) = payer_tx.send(PayerMessage::Adjudicate(claim))
            .await {
                eprintln!("Failed to forward claim {} to payer {}: {}", claim_id, payer_id, e);
            }
        } else {
            eprintln!("Unknown payer ID: {}", payer_id);
        }
    }

    async fn handle_remittance(&mut self, remittance: Remittance) {
        let claim_id = remittance.claim_id.clone();

        if let Some(ClaimStatus::Submitted { claim, submitted_at}) = self.history.lock().await.remove(&claim_id) {
            let record = RemittanceRecord::new(
                claim, 
                remittance.clone(), 
                submitted_at, 
                Instant::now());
            self.history.lock().await.insert(claim_id.clone(), ClaimStatus::Remitted(record));
        }

        // Forward remittance to originating biller
        let tx_opt = self.biller_txs.lock().await.remove(&claim_id);
        match tx_opt {
            Some(tx) => {
                if let Err(e) = tx.send(RemittanceMessage::Processed(remittance)).await {
                    eprintln!("Failed to send remittance for claim {}: {}", claim_id, e);
                }
            }
            None => {
                eprintln!("No return channel found for claim {}", claim_id);
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{remittance::mock_remittance, schema::mock_claim};

    #[tokio::test]
    async fn test_run_clearinghouse() {
        // input channel for claims from biller
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);

        // input channel for remittances from payer
        let (remittance_tx, remittance_rx) = tokio::sync::mpsc::channel(1);

        // output channel for PayerMessage -> Payer
        let (payer_tx, mut payer_rx) = tokio::sync::mpsc::channel(1);

        // Create payer channels map
        let mut payer_txs = HashMap::new();
        payer_txs.insert("medicare".to_string(), payer_tx);

        // Create shared state
        let biller_txs = Arc::new(Mutex::new(HashMap::new()));
        let claim_timestamps = Arc::new(Mutex::new(HashMap::new()));

        // spawn clearinghouse task
        let clearinghouse = Clearinghouse::new(
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs.clone(),
            claim_timestamps.clone(),
        );
        tokio::spawn(async move {
            clearinghouse.run().await;
        });

        // Create a mock claim envelope
        let mock_claim = mock_claim();
        let (response_tx, mut response_rx) = tokio::sync::mpsc::channel(1);
        let envelope = ClaimEnvelope {
            claim: mock_claim,
            response_tx,
        };

        // Send claim envelope to clearinghouse
        claim_tx.send(ClaimMessage::NewClaim(envelope)).await.unwrap();

        // Verify claim was forwarded to payer
        if let Some(PayerMessage::Adjudicate(claim)) = payer_rx.recv().await {
            assert_eq!(claim.claim_id, "abc123");
            assert_eq!(claim.insurance.payer_id, "medicare");
        } else {
            panic!("Expected PayerMessage::Adjudicate");
        }

        // Simulate remittance being returned from payer
        let mock_remittance = mock_remittance();
        remittance_tx.send(RemittanceMessage::Processed(mock_remittance)).await.unwrap();

        // Verify remittance was forwarded to biller
        if let Some(RemittanceMessage::Processed(remittance)) = response_rx.recv().await {
            assert_eq!(remittance.claim_id, "abc123");
        } else {
            panic!("Expected RemittanceMessage::Processed");
        }
    }
}