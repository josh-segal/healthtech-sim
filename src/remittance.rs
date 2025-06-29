use serde::{Deserialize};

#[derive(Debug, Deserialize)]
pub struct Remittance {
    pub claim_id: String,
    pub service_line_remittances: Vec<ServiceLineRemittance>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceLineRemittance {
    pub service_line_id: String,
    pub payer_paid_amount: f64,
    pub coinsurance_amount: f64,
    pub copay_amount: f64,
    pub deductible_amount: f64,
    pub not_allowed_amount: f64,
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
