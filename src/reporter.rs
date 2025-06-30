use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time;

use crate::remittance::Remittance;

/// Reporter task that periodically logs AR aging and patient-level summaries.
pub async fn run_reporter(
    remittance_history: Arc<Mutex<HashMap<String, (Instant, Remittance)>>>,
) {
    let mut interval = time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;
        let report = {
            let history = remittance_history.lock().await;
            // generate_report(&history)
        };
        println!("report goes here");
    }
}

// /// Computes AR aging and patient-level summaries.
// fn generate_report(
//     history: &HashMap<String, (Instant, Remittance)>,
// ) -> String {
//     let now = Instant::now();

//     let mut ar_buckets = [0, 0, 0, 0]; // 0–1, 1–2, 2–3, 3+ min
//     let mut patient_totals: HashMap<String, (f64, f64, f64)> = HashMap::new();

//     for (_claim_id, (submitted_at, remittance)) in history {
//         let age_min = elapsed_minutes(*submitted_at, now);

//         match age_min {
//             0 => ar_buckets[0] += 1,
//             1 => ar_buckets[1] += 1,
//             2 => ar_buckets[2] += 1,
//             _ => ar_buckets[3] += 1,
//         }

//         let key = remittance.patient_id.clone();
//         let entry = patient_totals.entry(key).or_insert((0.0, 0.0, 0.0));
//         entry.0 += remittance.total_copay();
//         entry.1 += remittance.total_coinsurance();
//         entry.2 += remittance.total_deductible();
//     }

//     format!(
//         "\n====== AR Aging Report ======\n\
//         0–1 min: {}\n\
//         1–2 min: {}\n\
//         2–3 min: {}\n\
//         3+ min: {}\n\
//         \n====== Patient Summary Stats ======\n{}",
//         ar_buckets[0],
//         ar_buckets[1],
//         ar_buckets[2],
//         ar_buckets[3],
//         format_patient_stats(&patient_totals)
//     )
// }

// /// Pretty-print patient copay/coinsurance/deductible totals.
// fn format_patient_stats(
//     totals: &HashMap<String, (f64, f64, f64)>,
// ) -> String {
//     let mut lines = vec![];
//     for (patient_id, (copay, coinsurance, deductible)) in totals {
//         lines.push(format!(
//             "{} => Copay: ${:.2}, Coinsurance: ${:.2}, Deductible: ${:.2}",
//             patient_id, copay, coinsurance, deductible
//         ));
//     }
//     lines.join("\n")
// }
