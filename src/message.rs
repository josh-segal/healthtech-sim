use crate::remittance::{Remittance, RemittanceRecord};
use crate::schema::PayerClaim;
use std::time::Instant;
use tokio::sync::mpsc::Sender;

/// Wraps a claim with a response channel for remittance processing
/// 
/// Used by biller to track which claim a remittance response belongs to
#[derive(Debug)]
pub struct ClaimEnvelope {
    pub claim: PayerClaim,
    pub response_tx: Sender<RemittanceMessage>,
}

/// Message sent from Biller to Clearinghouse
#[derive(Debug)]
pub enum ClaimMessage {
    NewClaim(ClaimEnvelope),
}

/// Message sent from Clearinghouse to Payer
#[derive(Debug)]
pub enum PayerMessage {
    Adjudicate(PayerClaim),
}

/// Message sent from Payer to Clearinghouse
/// and from Clearinghouse to Biller
#[derive(Debug)]
pub enum RemittanceMessage {
    Processed(Remittance),
}

/// Claim status: submitted or remitted
#[derive(Debug)]
pub enum ClaimStatus {
    Submitted {
        claim: PayerClaim,
        submitted_at: Instant,
    },
    Remitted(RemittanceRecord),
}
