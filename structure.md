PayerClaim (root)
├── claim_id: String
├── place_of_service_code: u32
├── insurance: Insurance
│   ├── payer_id: String
│   └── patient_member_id: String
├── patient: Patient
│   ├── first_name: String
│   ├── last_name: String
│   ├── gender: String
│   ├── dob: String
│   ├── email: Option<String>
│   └── address: Option<Address>
│       ├── street: Option<String>
│       ├── city: Option<String>
│       ├── state: Option<String>
│       ├── zip: Option<String>
│       └── country: Option<String>
├── organization: Organization
│   ├── name: String
│   ├── billing_npi: Option<String>
│   ├── ein: Option<String>
│   ├── contact: Option<Contact>
│   │   ├── first_name: Option<String>
│   │   ├── last_name: Option<String>
│   │   └── phone_number: Option<String>
│   └── address: Option<Address>  (same structure as patient.address)
├── rendering_provider: Provider
│   ├── first_name: String
│   ├── last_name: String
│   └── npi: String
└── service_lines: Vec<ServiceLine>
    ├── service_line_id: String
    ├── procedure_code: String
    ├── units: u32
    ├── details: String
    ├── unit_charge_currency: String
    ├── unit_charge_amount: f64
    ├── modifiers: Option<Vec<String>>
    └── do_not_bill: Option<bool>

