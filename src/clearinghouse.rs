use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{
    Mutex,
    mpsc::{Receiver, Sender},
};

use crate::logging::log_claim_event;
use crate::message::{ClaimEnvelope, ClaimMessage, ClaimStatus, PayerMessage, RemittanceMessage};
use crate::remittance::{Remittance, RemittanceRecord};

pub struct Clearinghouse {
    claim_rx: Receiver<ClaimMessage>,
    payer_txs: HashMap<String, Sender<PayerMessage>>,
    remittance_rx: Receiver<RemittanceMessage>,
    biller_txs: Arc<Mutex<HashMap<String, Sender<RemittanceMessage>>>>,
    history: Arc<Mutex<HashMap<String, ClaimStatus>>>,
    verbose: bool,
}

impl Clearinghouse {
    pub fn new(
        claim_rx: Receiver<ClaimMessage>,
        payer_txs: HashMap<String, Sender<PayerMessage>>,
        remittance_rx: Receiver<RemittanceMessage>,
        biller_txs: Arc<Mutex<HashMap<String, Sender<RemittanceMessage>>>>,
        history: Arc<Mutex<HashMap<String, ClaimStatus>>>,
        verbose: bool,
    ) -> Self {
        Self {
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs,
            history,
            verbose,
        }
    }

    pub async fn run(mut self) {
        if self.verbose {
            log_claim_event("clearinghouse", "-", "start", "Starting clearinghouse task");
        }
        loop {
            tokio::select! {
                Some(msg) = self.claim_rx.recv() => {
                    if let ClaimMessage::NewClaim(envelope) = msg { // redundent unless add more ClaimMessage variants... leaving for extensibility
                        if self.verbose {
                            log_claim_event("clearinghouse", &envelope.claim.claim_id, "handle_new_claim", &format!("Handling new claim: {}", &envelope.claim.claim_id));
                        }
                        self.handle_claim(envelope).await;
                    }
                }
                Some(msg) = self.remittance_rx.recv() => {
                    if let RemittanceMessage::Processed(remittance) = msg { // redundent unless add more RemittanceMessage variants... leaving for extensibility
                        if self.verbose {
                            log_claim_event("clearinghouse", &remittance.claim_id, "handle_remittance", &format!("Handling remittance for claim: {}", &remittance.claim_id));
                        }
                        self.handle_remittance(remittance).await;
                    }
                }
                else => {
                    break;
                }
            }
        }
        if self.verbose {
            log_claim_event(
                "clearinghouse",
                "-",
                "shutdown",
                "Shutting down clearinghouse task",
            );
        }
    }

    async fn handle_claim(&mut self, envelope: ClaimEnvelope) {
        let claim = envelope.claim;
        let response_tx = envelope.response_tx;
        let claim_id = claim.claim_id.clone();
        let payer_id = claim.insurance.payer_id.clone();

        //TODO: any faster way than locks here?
        // Track response channel for later
        self.biller_txs
            .lock()
            .await
            .insert(claim_id.clone(), response_tx);

        // Track for AR aging
        self.history.lock().await.insert(
            claim_id.clone(),
            ClaimStatus::Submitted {
                claim: claim.clone(), //TODO: is it okay to clone claims and remittance like this?
                submitted_at: Instant::now(),
            },
        );

        if self.verbose {
            log_claim_event(
                "clearinghouse",
                &claim_id,
                "forward_to_payer",
                &format!("Forwarding claim to payer {}", &payer_id),
            );
        }
        // Forward claim to payer
        if let Some(payer_tx) = self.payer_txs.get(&payer_id) {
            if let Err(e) = payer_tx.send(PayerMessage::Adjudicate(claim)).await {
                eprintln!(
                    "Failed to forward claim {} to payer {}: {}",
                    claim_id, payer_id, e
                );
            }
        } else {
            eprintln!("Unknown payer ID: {}", payer_id);
        }
    }

    async fn handle_remittance(&mut self, remittance: Remittance) {
        // println!("ATTEMPTING TO HANDLE REMITTANCE CLEARINGHOUSE ------");
        let claim_id = remittance.claim_id.clone();

        // lock history and try to remove claim
        let mut history = self.history.lock().await;
        match history.remove(&claim_id) {
            Some(ClaimStatus::Submitted {
                claim,
                submitted_at,
            }) => {
                let record =
                    RemittanceRecord::new(claim, remittance.clone(), submitted_at, Instant::now());
                history.insert(claim_id.clone(), ClaimStatus::Remitted(record));
                if self.verbose {
                    log_claim_event(
                        "clearinghouse",
                        &claim_id,
                        "remittance_recorded",
                        "Remittance recorded in history",
                    );
                }
            }
            Some(status) => {
                eprintln!(
                    "Claim {} found in history but not in Submitted state: {:?}",
                    claim_id, status
                );
                if self.verbose {
                    log_claim_event(
                        "clearinghouse",
                        &claim_id,
                        "remittance_wrong_state",
                        "Claim not in Submitted state",
                    );
                }
                return;
            }
            None => {
                eprintln!("Claim {} not found in history", claim_id);
                if self.verbose {
                    log_claim_event(
                        "clearinghouse",
                        &claim_id,
                        "remittance_not_found",
                        "Claim not found in history",
                    );
                }
                return;
            }
        }
        drop(history); // Explicitly drop the lock before locking biller_txs

        // Forward remittance to originating biller
        let mut biller_txs = self.biller_txs.lock().await;
        match biller_txs.remove(&claim_id) {
            Some(tx) => {
                if let Err(e) = tx.send(RemittanceMessage::Processed(remittance)).await {
                    eprintln!("Failed to send remittance for claim {}: {}", claim_id, e);
                } else if self.verbose {
                    log_claim_event(
                        "clearinghouse",
                        &claim_id,
                        "remittance_sent",
                        "Remittance sent to biller",
                    );
                }
            }
            None => {
                eprintln!("No return channel found for claim {}", claim_id);
                if self.verbose {
                    log_claim_event(
                        "clearinghouse",
                        &claim_id,
                        "remittance_no_channel",
                        "No return channel found for claim",
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{remittance::mock_remittance, schema::mock_claim};

    /// Test that a claim is received, routed to the correct payer, and remittance is returned to the biller.
    /// Expected: Claim is forwarded, remittance is received, and history is updated.
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
            true,
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
        claim_tx
            .send(ClaimMessage::NewClaim(envelope))
            .await
            .unwrap();

        // Verify claim was forwarded to payer
        if let Some(PayerMessage::Adjudicate(claim)) = payer_rx.recv().await {
            assert_eq!(claim.claim_id, "abc123");
            assert_eq!(claim.insurance.payer_id, "medicare");
        } else {
            panic!("Expected PayerMessage::Adjudicate");
        }

        // Simulate remittance being returned from payer
        let mock_remittance = mock_remittance();
        remittance_tx
            .send(RemittanceMessage::Processed(mock_remittance))
            .await
            .unwrap();

        // Verify remittance was forwarded to biller
        if let Some(RemittanceMessage::Processed(remittance)) = response_rx.recv().await {
            assert_eq!(remittance.claim_id, "abc123");
        } else {
            panic!("Expected RemittanceMessage::Processed");
        }
    }

    /// Test that claims with unknown payer IDs are handled gracefully.
    /// Expected: Error is logged, claim is not forwarded.
    #[tokio::test]
    async fn test_handle_claim_unknown_payer() {
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (_remittance_tx, remittance_rx) = tokio::sync::mpsc::channel(1);
        let (payer_tx, mut payer_rx) = tokio::sync::mpsc::channel(1);
        let mut payer_txs = HashMap::new();
        payer_txs.insert("medicare".to_string(), payer_tx);
        let biller_txs = Arc::new(Mutex::new(HashMap::new()));
        let history = Arc::new(Mutex::new(HashMap::new()));
        let clearinghouse = Clearinghouse::new(
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs.clone(),
            history.clone(),
            false,
        );
        tokio::spawn(async move {
            clearinghouse.run().await;
        });
        let mut mock_claim = mock_claim();
        mock_claim.insurance.payer_id = "unknown_payer".to_string();
        let (response_tx, _response_rx) = tokio::sync::mpsc::channel(1);
        let envelope = ClaimEnvelope {
            claim: mock_claim,
            response_tx,
        };
        claim_tx
            .send(ClaimMessage::NewClaim(envelope))
            .await
            .unwrap();
        // Should not receive any message from payer
        assert!(payer_rx.try_recv().is_err());
    }

    /// Test that remittance for unknown claim IDs is handled gracefully.
    /// Expected: Error is logged, no panic.
    #[tokio::test]
    async fn test_handle_remittance_no_history() {
        let (_claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, remittance_rx) = tokio::sync::mpsc::channel(1);
        let (payer_tx, _payer_rx) = tokio::sync::mpsc::channel(1);
        let mut payer_txs = HashMap::new();
        payer_txs.insert("medicare".to_string(), payer_tx);
        let biller_txs = Arc::new(Mutex::new(HashMap::new()));
        let history = Arc::new(Mutex::new(HashMap::new()));
        let clearinghouse = Clearinghouse::new(
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs.clone(),
            history.clone(),
            false,
        );
        tokio::spawn(async move {
            clearinghouse.run().await;
        });
        let mut mock_remittance = mock_remittance();
        mock_remittance.claim_id = "unknown_claim".to_string();
        remittance_tx
            .send(RemittanceMessage::Processed(mock_remittance))
            .await
            .unwrap();
        // Should not panic, just log error
    }

    /// Test that remittance for a claim not in the Submitted state is handled gracefully.
    /// Expected: Error is logged, no panic.
    #[tokio::test]
    async fn test_handle_remittance_wrong_state() {
        let (_claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, remittance_rx) = tokio::sync::mpsc::channel(1);
        let (payer_tx, _payer_rx) = tokio::sync::mpsc::channel(1);
        let mut payer_txs = HashMap::new();
        payer_txs.insert("medicare".to_string(), payer_tx);
        let biller_txs = Arc::new(Mutex::new(HashMap::new()));
        let history = Arc::new(Mutex::new(HashMap::new()));
        let clearinghouse = Clearinghouse::new(
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs.clone(),
            history.clone(),
            false,
        );
        tokio::spawn(async move {
            clearinghouse.run().await;
        });
        let mock_claim = mock_claim();
        let mock_remittance = mock_remittance();
        // Manually insert a remitted claim to simulate wrong state
        {
            let mut history = history.lock().await;
            history.insert(
                mock_claim.claim_id.clone(),
                ClaimStatus::Remitted(RemittanceRecord::new(
                    mock_claim,
                    mock_remittance.clone(),
                    Instant::now(),
                    Instant::now(),
                )),
            );
        }
        remittance_tx
            .send(RemittanceMessage::Processed(mock_remittance))
            .await
            .unwrap();
        // Should not panic, just log error
    }

    /// Test that remittance for a claim with no biller channel is handled gracefully.
    /// Expected: Error is logged, no panic.
    #[tokio::test]
    async fn test_handle_remittance_no_biller_channel() {
        let (_claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);
        let (remittance_tx, remittance_rx) = tokio::sync::mpsc::channel(1);
        let (payer_tx, _payer_rx) = tokio::sync::mpsc::channel(1);
        let mut payer_txs = HashMap::new();
        payer_txs.insert("medicare".to_string(), payer_tx);
        let biller_txs = Arc::new(Mutex::new(HashMap::new()));
        let history = Arc::new(Mutex::new(HashMap::new()));
        let clearinghouse = Clearinghouse::new(
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs.clone(),
            history.clone(),
            false,
        );
        tokio::spawn(async move {
            clearinghouse.run().await;
        });
        let mock_claim = mock_claim();
        let mock_remittance = mock_remittance();
        // Manually insert claim in history but don't add biller channel
        {
            let mut history = history.lock().await;
            history.insert(
                mock_claim.claim_id.clone(),
                ClaimStatus::Submitted {
                    claim: mock_claim,
                    submitted_at: Instant::now(),
                },
            );
        }
        remittance_tx
            .send(RemittanceMessage::Processed(mock_remittance))
            .await
            .unwrap();
        // Should not panic, just log error
    }

    /// Test that the clearinghouse can handle multiple claims and remittances correctly.
    /// Expected: All claims are processed and remittances are returned to correct billers.
    #[tokio::test]
    async fn test_clearinghouse_multiple_claims() {
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(2);
        let (remittance_tx, remittance_rx) = tokio::sync::mpsc::channel(2);
        let (payer_tx, mut payer_rx) = tokio::sync::mpsc::channel(2);
        let mut payer_txs = HashMap::new();
        payer_txs.insert("medicare".to_string(), payer_tx);
        let biller_txs = Arc::new(Mutex::new(HashMap::new()));
        let history = Arc::new(Mutex::new(HashMap::new()));
        let clearinghouse = Clearinghouse::new(
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs.clone(),
            history.clone(),
            false,
        );
        tokio::spawn(async move {
            clearinghouse.run().await;
        });
        let claim1 = mock_claim();
        let mut claim2 = mock_claim();
        claim2.claim_id = "claim2".to_string();
        let (response_tx1, mut response_rx1) = tokio::sync::mpsc::channel(1);
        let (response_tx2, mut response_rx2) = tokio::sync::mpsc::channel(1);
        let envelope1 = ClaimEnvelope {
            claim: claim1.clone(),
            response_tx: response_tx1,
        };
        let envelope2 = ClaimEnvelope {
            claim: claim2.clone(),
            response_tx: response_tx2,
        };
        claim_tx
            .send(ClaimMessage::NewClaim(envelope1))
            .await
            .unwrap();
        claim_tx
            .send(ClaimMessage::NewClaim(envelope2))
            .await
            .unwrap();
        // Verify both claims were sent to payer
        for _ in 0..2 {
            if let Some(PayerMessage::Adjudicate(claim)) = payer_rx.recv().await {
                assert!(claim.claim_id == "abc123" || claim.claim_id == "claim2");
            }
        }
        // Send remittances back
        let remittance1 = mock_remittance();
        let mut remittance2 = mock_remittance();
        remittance2.claim_id = "claim2".to_string();
        remittance_tx
            .send(RemittanceMessage::Processed(remittance1))
            .await
            .unwrap();
        remittance_tx
            .send(RemittanceMessage::Processed(remittance2))
            .await
            .unwrap();
        // Verify remittances were sent to billers
        let response1 = response_rx1.recv().await.expect("Expected remittance 1");
        let response2 = response_rx2.recv().await.expect("Expected remittance 2");
        assert!(matches!(response1, RemittanceMessage::Processed(_)));
        assert!(matches!(response2, RemittanceMessage::Processed(_)));
    }
}
