use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc::{Receiver, Sender}, Mutex};

use crate::message::{ClaimMessage, ClaimEnvelope, PayerMessage, RemittanceMessage};
use crate::remittance::Remittance;

pub struct Clearinghouse {
    claim_rx: Receiver<ClaimMessage>,
    payer_txs: HashMap<String, Sender<PayerMessage>>,
    remittance_rx: Receiver<RemittanceMessage>,
    biller_txs: Arc<Mutex<HashMap<String, Sender<RemittanceMessage>>>>,
    claim_timestamps: Arc<Mutex<HashMap<String, Instant>>>,
}

impl Clearinghouse {
    pub fn new(
        claim_rx: Receiver<ClaimMessage>,
        payer_txs: HashMap<String, Sender<PayerMessage>>,
        remittance_rx: Receiver<RemittanceMessage>,
        biller_txs: Arc<Mutex<HashMap<String, Sender<RemittanceMessage>>>>,
        claim_timestamps: Arc<Mutex<HashMap<String, Instant>>>,
    ) -> Self {
        Self {
            claim_rx,
            payer_txs,
            remittance_rx,
            biller_txs,
            claim_timestamps,
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
        self.claim_timestamps.lock().await.insert(claim_id.clone(), Instant::now());

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


