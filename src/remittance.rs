use serde::Deserialize;
use std::time::Instant;

use crate::schema::PayerClaim;

//TODO: switch from pub fields to pub getts
#[derive(Debug, Deserialize, Clone)]
pub struct ServiceLineRemittance {
    pub service_line_id: String,
    pub payer_paid_amount: f64,
    pub coinsurance_amount: f64,
    pub copay_amount: f64,
    pub deductible_amount: f64,
    pub not_allowed_amount: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Remittance {
    pub claim_id: String,
    pub service_line_remittances: Vec<ServiceLineRemittance>,
}

#[derive(Debug)]
pub struct RemittanceRecord {
    claim: PayerClaim,
    pub remittance: Remittance, //TODO: make field private
    submitted_at: Instant,
    remitted_at: Instant,
}

impl RemittanceRecord {
    pub fn new(
        claim: PayerClaim,
        remittance: Remittance,
        submitted_at: Instant,
        remitted_at: Instant,
    ) -> Self {
        Self {
            claim,
            remittance,
            submitted_at,
            remitted_at,
        }
    }
    pub fn elapsed(&self) -> std::time::Duration {
        self.remitted_at.duration_since(self.submitted_at)
    }

    pub fn patient_id(&self) -> &str {
        &self.claim.insurance.patient_member_id
    }

    pub fn payer_id(&self) -> &str {
        &self.claim.insurance.payer_id
    }
}

impl Remittance {
    /// Generate a remittance from a claim using mock payment logic
    /// 
    /// Calculates payment amounts based on a simple percentage model:
    /// 80% paid, 10% coinsurance, 5% copay, 3% deductible, 2% not allowed
    pub fn from_claim(claim: &PayerClaim) -> Remittance {
        let service_line_remittances: Vec<ServiceLineRemittance> = claim
            .service_lines
            .iter()
            .map(|service_line| {
                // Calculate remittance amounts based on service line
                let total_charge = service_line.unit_charge_amount * service_line.units as f64;

                // TODO: replace with EDI 835 ?
                // Simple mock calculation: 80% paid, 10% coinsurance, 5% copay, 3% deductible, 2% not allowed
                let payer_paid_amount = total_charge * 0.80;
                let coinsurance_amount = total_charge * 0.10;
                let copay_amount = total_charge * 0.05;
                let deductible_amount = total_charge * 0.03;
                let not_allowed_amount = total_charge * 0.02;

                ServiceLineRemittance {
                    service_line_id: service_line.service_line_id.clone(),
                    payer_paid_amount,
                    coinsurance_amount,
                    copay_amount,
                    deductible_amount,
                    not_allowed_amount,
                }
            })
            .collect();

        Remittance {
            claim_id: claim.claim_id.clone(),
            service_line_remittances,
        }
    }

    /// Validate that remittance amounts match the original billed amounts
    /// 
    /// Ensures the sum of all payment components equals the total charge
    /// Returns error if amounts don't balance within rounding tolerance
    pub fn validate_against_claim(&self, claim: &PayerClaim) -> Result<(), String> {
        for (remit, service_line) in self
            .service_line_remittances
            .iter()
            .zip(&claim.service_lines)
        {
            let billed = service_line.unit_charge_amount * service_line.units as f64;
            let sum = remit.payer_paid_amount
                + remit.coinsurance_amount
                + remit.copay_amount
                + remit.deductible_amount
                + remit.not_allowed_amount; // unclear if included in summation or not from instructions

            // Allow for floating point rounding errors
            if (sum - billed).abs() > 1e-2 {
                return Err(format!(
                    "Service line {}: remittance sum {:.2} does not match billed amount {:.2}",
                    remit.service_line_id, sum, billed
                ));
            }
        }
        Ok(())
    }
}

/// Mock remittance for testing
#[cfg(test)]
pub fn mock_remittance() -> Remittance {
    Remittance {
        claim_id: "abc123".to_string(),
        service_line_remittances: vec![
            ServiceLineRemittance {
                service_line_id: "sl1".to_string(),
                payer_paid_amount: 120.0,
                coinsurance_amount: 15.0,
                copay_amount: 10.0,
                deductible_amount: 5.0,
                not_allowed_amount: 0.0,
            },
            ServiceLineRemittance {
                service_line_id: "sl2".to_string(),
                payer_paid_amount: 80.0,
                coinsurance_amount: 20.0,
                copay_amount: 0.0,
                deductible_amount: 0.0,
                not_allowed_amount: 0.0,
            },
        ],
    }
}
