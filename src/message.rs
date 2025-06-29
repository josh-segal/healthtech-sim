use crate::schema::PayerClaim;

#[derive(Debug)]
pub enum ClaimMessage {
    NewClaim(PayerClaim),
}

