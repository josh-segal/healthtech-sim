use crate::remittance::{Remittance, RemittanceRecord};
use crate::schema::PayerClaim;
use std::time::Instant;
use tokio::sync::mpsc::Sender;

/// Struct that wraps a claim with a backward channel to send back remittance response
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
