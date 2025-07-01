use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time;

use crate::message::ClaimStatus;

/// Periodically generate and display business reports
/// 
/// Runs every 5 seconds to show AR aging and patient financial summaries
/// Uses shared claim history to track processing status
pub async fn run_reporter(history: Arc<Mutex<HashMap<String, ClaimStatus>>>, verbose: bool) {
    if verbose {
        println!("[reporter] Starting reporter task");
    }
    let mut interval = time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;
        let records = history.lock().await;

        print_combined_report(&records);
    }
}

/// Generate and print combined AR aging and patient financial reports
/// 
/// AR Aging: Groups claims by payer and age buckets (0-1m, 1-2m, 2-3m, 3m+)
/// Patient Summary: Totals copay, coinsurance, and deductible by patient
#[derive(Default)]
struct Totals {
    copay: f64,
    coins: f64,
    deduct: f64,
}

fn print_combined_report(records: &HashMap<String, ClaimStatus>) {
    let mut aging_buckets: HashMap<String, [u32; 4]> = HashMap::new();
    let mut patient_summary: HashMap<String, Totals> = HashMap::new();

    for status in records.values() {
        update_aging_buckets(status, &mut aging_buckets);
        update_patient_summary(status, &mut patient_summary);
    }

    println!("\n--- AR Aging Report ---");
    for (payer, [b0, b1, b2, b3]) in aging_buckets {
        println!("{payer}: 0–1m: {b0}, 1–2m: {b1}, 2–3m: {b2}, 3+m: {b3}");
    }
    println!("");

    println!("\n--- Patient Financial Summary ---");
    for (patient, totals) in patient_summary {
        println!(
            "{patient}: Copay: ${:.2}, Coinsurance: ${:.2}, Deductible: ${:.2}",
            totals.copay, totals.coins, totals.deduct
        );
    }
    println!("");
}

fn update_aging_buckets(status: &ClaimStatus, aging_buckets: &mut HashMap<String, [u32; 4]>) {
    if let ClaimStatus::Submitted { claim, submitted_at } = status {
        let payer_id = claim.insurance.payer_id.clone();
        let age_secs = std::time::Instant::now().duration_since(*submitted_at).as_secs();
        let bucket = match age_secs {
            0..=59 => 0,
            60..=119 => 1,
            120..=179 => 2,
            _ => 3,
        };
        aging_buckets.entry(payer_id).or_insert([0; 4])[bucket] += 1;
    }
}

fn update_patient_summary(status: &ClaimStatus, patient_summary: &mut HashMap<String, Totals>) {
    if let ClaimStatus::Remitted(record) = status {
        let entry = patient_summary
            .entry(record.patient_id().to_string())
            .or_default();
        for line in &record.remittance.service_line_remittances {
            entry.copay += line.copay_amount;
            entry.coins += line.coinsurance_amount;
            entry.deduct += line.deductible_amount;
        }
    }
}
