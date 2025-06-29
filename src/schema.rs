use serde::Deserialize;

/// The root struct for a claim
#[derive(Debug, Deserialize)]
pub struct PayerClaim {
    pub claim_id: String,
    pub place_of_service_code: u32,
    pub insurance: Insurance,
    pub patient: Patient,
    pub organization: Organization,
    pub rendering_provider: Provider,
    pub service_lines: Vec<ServiceLine>,
}

#[derive(Debug, Deserialize)]
pub struct Insurance {
    pub payer_id: String,
    pub patient_member_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Patient {
    pub first_name: String,
    pub last_name: String,
    pub gender: String, 
    pub dob: String,  
    pub email: Option<String>,
    pub address: Option<Address>,
}

#[derive(Debug, Deserialize)]
pub struct Organization {
    pub name: String,
    pub billing_npi: Option<String>,
    pub ein: Option<String>,
    pub contact: Option<Contact>,
    pub address: Option<Address>,
}

#[derive(Debug, Deserialize)]
pub struct Provider {
    pub first_name: String,
    pub last_name: String,
    pub npi: String, 
}

#[derive(Debug, Deserialize)]
pub struct ServiceLine {
    pub service_line_id: String,
    pub procedure_code: String,
    pub units: u32,
    pub details: String,
    pub unit_charge_currency: String,
    pub unit_charge_amount: f64,
    pub modifiers: Option<Vec<String>>,
    pub do_not_bill: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Address {
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub zip: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Contact {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;

    #[test]
    fn test_schema() {
        let json = r#"
        {
            "claim_id": "abc123",
            "place_of_service_code": 11,
            "insurance": {
                "payer_id": "medicare",
                "patient_member_id": "pmid456"
            },
            "patient": {
                "first_name": "Jane",
                "last_name": "Doe",
                "gender": "f",
                "dob": "1990-01-01",
                "email": "jane.doe@example.com",
                "address": {
                    "street": "123 Main St",
                    "city": "Metropolis",
                    "state": "NY",
                    "zip": "12345",
                    "country": "USA"
                }
            },
            "organization": {
                "name": "Health Inc",
                "billing_npi": "9876543210",
                "ein": "12-3456789",
                "contact": {
                    "first_name": "Bob",
                    "last_name": "Jones",
                    "phone_number": "555-1234"
                },
                "address": {
                    "street": "456 Health Ave",
                    "city": "Gotham",
                    "state": "CA",
                    "zip": "67890",
                    "country": "USA"
                }
            },
            "rendering_provider": {
                "first_name": "Alice",
                "last_name": "Smith",
                "npi": "1234567890"
            },
            "service_lines": [
                {
                    "service_line_id": "sl1",
                    "procedure_code": "99213",
                    "units": 1,
                    "details": "Office visit",
                    "unit_charge_currency": "USD",
                    "unit_charge_amount": 150.0,
                    "modifiers": ["A1", "B2"],
                    "do_not_bill": true
                }
            ]
        }
        "#;

        let claim: PayerClaim = from_str(json).expect("Failed to parse JSON");
        println!("{:#?}", claim); // use cargo test -- --nocapture to still debug print on success
        assert_eq!(claim.claim_id, "abc123");
        assert_eq!(claim.place_of_service_code, 11);
        assert_eq!(claim.insurance.payer_id, "medicare");
        assert_eq!(claim.insurance.patient_member_id, "pmid456");
        assert_eq!(claim.patient.first_name, "Jane");
        assert_eq!(claim.patient.last_name, "Doe");
        assert_eq!(claim.patient.gender, "f");
        assert_eq!(claim.patient.dob, "1990-01-01");
        assert_eq!(claim.patient.email.as_deref(), Some("jane.doe@example.com"));
        let p_addr = claim.patient.address.as_ref().expect("patient address should exist");
        assert_eq!(p_addr.street.as_deref(), Some("123 Main St"));
        assert_eq!(p_addr.city.as_deref(), Some("Metropolis"));
        assert_eq!(p_addr.state.as_deref(), Some("NY"));
        assert_eq!(p_addr.zip.as_deref(), Some("12345"));
        assert_eq!(p_addr.country.as_deref(), Some("USA"));
        assert_eq!(claim.organization.name, "Health Inc");
        assert_eq!(claim.organization.billing_npi.as_deref(), Some("9876543210"));
        assert_eq!(claim.organization.ein.as_deref(), Some("12-3456789"));
        let org_contact = claim.organization.contact.as_ref().expect("org contact should exist");
        assert_eq!(org_contact.first_name.as_deref(), Some("Bob"));
        assert_eq!(org_contact.last_name.as_deref(), Some("Jones"));
        assert_eq!(org_contact.phone_number.as_deref(), Some("555-1234"));
        let org_addr = claim.organization.address.as_ref().expect("org address should exist");
        assert_eq!(org_addr.street.as_deref(), Some("456 Health Ave"));
        assert_eq!(org_addr.city.as_deref(), Some("Gotham"));
        assert_eq!(org_addr.state.as_deref(), Some("CA"));
        assert_eq!(org_addr.zip.as_deref(), Some("67890"));
        assert_eq!(org_addr.country.as_deref(), Some("USA"));
        assert_eq!(claim.rendering_provider.first_name, "Alice");
        assert_eq!(claim.rendering_provider.last_name, "Smith");
        assert_eq!(claim.rendering_provider.npi, "1234567890");
        assert_eq!(claim.service_lines.len(), 1);
        let sl = &claim.service_lines[0];
        assert_eq!(sl.service_line_id, "sl1");
        assert_eq!(sl.procedure_code, "99213");
        assert_eq!(sl.units, 1);
        assert_eq!(sl.details, "Office visit");
        assert_eq!(sl.unit_charge_currency, "USD");
        assert_eq!(sl.unit_charge_amount, 150.0);
        assert_eq!(sl.modifiers.as_ref().unwrap(), &["A1", "B2"]);
        assert_eq!(sl.do_not_bill, Some(true));
    }
}