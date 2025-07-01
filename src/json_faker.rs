use crate::schema::PayerClaim;
use chrono::NaiveDate;
use fake::faker::address::en::*;
use fake::faker::boolean::en::*;
use fake::faker::company::en::*;
use fake::faker::internet::en::*;
use fake::faker::lorem::en::Word;
use fake::faker::name::en::*;
use fake::faker::number::en::*;
use fake::{Fake, Faker};
use rand::seq::IndexedRandom;
use std::fs::File;
use std::io::{BufWriter, Write};

/// Generate a realistic fake healthcare claim for testing
/// 
/// Creates claims with random but valid patient, provider, and billing data
/// Uses common payer IDs and realistic procedure codes
pub fn fake_payer_claim() -> PayerClaim {
    use crate::schema::*;
    let mut rng = rand::rng();
    PayerClaim {
        claim_id: Faker.fake(),
        place_of_service_code: NumberWithFormat("##")
            .fake::<String>()
            .parse()
            .unwrap_or(11),
        insurance: Insurance {
            payer_id: ["medicare", "united_health_group", "anthem"]
                .choose(&mut rng)
                .unwrap()
                .to_string(),
            patient_member_id: Faker.fake(),
        },
        patient: Patient {
            first_name: FirstName().fake(),
            last_name: LastName().fake(),
            gender: ["m", "f", "o"].choose(&mut rng).unwrap().to_string(),
            dob: NaiveDate::from_ymd_opt(
                *((1950..=2010).collect::<Vec<_>>().choose(&mut rng).unwrap()),
                *((1..=12).collect::<Vec<_>>().choose(&mut rng).unwrap()),
                *((1..=28).collect::<Vec<_>>().choose(&mut rng).unwrap()),
            )
            .unwrap()
            .to_string(),
            email: Some(FreeEmail().fake()),
            address: Some(Address {
                street: Some(StreetName().fake()),
                city: Some(CityName().fake()),
                state: Some(StateAbbr().fake()),
                zip: Some(PostCode().fake()),
                country: Some("USA".to_string()),
            }),
        },
        organization: Organization {
            name: CompanyName().fake(),
            billing_npi: Some(NumberWithFormat("##########").fake()),
            ein: Some(format!(
                "{}-{}",
                NumberWithFormat("##").fake::<String>(),
                NumberWithFormat("######").fake::<String>()
            )),
            contact: Some(Contact {
                first_name: Some(FirstName().fake()),
                last_name: Some(LastName().fake()),
                phone_number: Some(format!("555-{:04}", (0..10000).fake::<u16>())),
            }),
            address: Some(Address {
                street: Some(StreetName().fake()),
                city: Some(CityName().fake()),
                state: Some(StateAbbr().fake()),
                zip: Some(PostCode().fake()),
                country: Some("USA".to_string()),
            }),
        },
        rendering_provider: Provider {
            first_name: FirstName().fake(),
            last_name: LastName().fake(),
            npi: NumberWithFormat("##########").fake(),
        },
        service_lines: vec![ServiceLine {
            service_line_id: Faker.fake(),
            procedure_code: NumberWithFormat("#####").fake(),
            units: (1..5).fake(),
            details: format!("{} {}", Word().fake::<String>(), Word().fake::<String>()),
            unit_charge_currency: "USD".to_string(),
            unit_charge_amount: (50.0..500.0).fake(),
            modifiers: Some(vec![
                (0..2)
                    .map(|_| Word().fake::<String>())
                    .collect::<Vec<_>>()
                    .join(""),
            ]),
            do_not_bill: Some(Boolean(50).fake()),
        }],
    }
}

/// Write multiple fake claims to a JSONL file for simulation
/// 
/// Creates n claims and writes them as JSON lines to the specified path
/// Used to generate test data for the claim processing simulation
pub fn write_fake_claims_jsonl(path: &str, n: usize) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for _ in 0..n {
        let claim = fake_payer_claim();
        let json = serde_json::to_string(&claim).unwrap();
        writeln!(writer, "{}", json)?;
    }
    Ok(())
}
