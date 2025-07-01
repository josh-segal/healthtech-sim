use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time;

use crate::message::ClaimStatus;
use prettytable::{Table, Row, Cell};
use colored::*;

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

    // AR Aging Report
    println!("{}", "\n--- AR Aging Report ---".bold().blue());
    let mut ar_table = Table::new();
    ar_table.add_row(Row::new(vec![
        Cell::new("Payer").style_spec("bFc"),
        Cell::new("0–1m").style_spec("bFc"),
        Cell::new("1–2m").style_spec("bFc"),
        Cell::new("2–3m").style_spec("bFc"),
        Cell::new("3+m").style_spec("bFc"),
    ]));
    for (payer, [b0, b1, b2, b3]) in &aging_buckets {
        ar_table.add_row(Row::new(vec![
            Cell::new(payer),
            Cell::new(&b0.to_string()),
            Cell::new(&b1.to_string()),
            Cell::new(&b2.to_string()),
            Cell::new(&b3.to_string()),
        ]));
    }
    // Add total outstanding claims row
    let (total_b0, total_b1, total_b2, total_b3): (u32, u32, u32, u32) = aging_buckets.values().fold((0, 0, 0, 0), |acc, buckets| {
        (acc.0 + buckets[0], acc.1 + buckets[1], acc.2 + buckets[2], acc.3 + buckets[3])
    });
    let total_outstanding = total_b0 + total_b1 + total_b2 + total_b3;
    ar_table.add_row(Row::new(vec![
        Cell::new("TOTAL OUTSTANDING").style_spec("bFc"),
        Cell::new(&total_b0.to_string()),
        Cell::new(&total_b1.to_string()),
        Cell::new(&total_b2.to_string()),
        Cell::new(&total_b3.to_string()),
    ]));
    ar_table.add_row(Row::new(vec![
        Cell::new("").style_spec(""),
        Cell::new("").style_spec("") ,
        Cell::new("").style_spec("") ,
        Cell::new("").style_spec("") ,
        Cell::new(&format!("Total Claims: {}", total_outstanding)).style_spec("bFc"),
    ]));
    ar_table.printstd();

    // Patient Financial Summary
    println!("{}", "\n--- Patient Financial Summary ---".bold().blue());
    let mut pf_table = Table::new();
    pf_table.add_row(Row::new(vec![
        Cell::new("Patient").style_spec("bFc"),
        Cell::new("Copay").style_spec("bFc"),
        Cell::new("Coinsurance").style_spec("bFc"),
        Cell::new("Deductible").style_spec("bFc"),
    ]));
    for (patient, totals) in &patient_summary {
        pf_table.add_row(Row::new(vec![
            Cell::new(patient),
            Cell::new(&format!("${:.2}", totals.copay)),
            Cell::new(&format!("${:.2}", totals.coins)),
            Cell::new(&format!("${:.2}", totals.deduct)),
        ]));
    }
    // Add total number of patients row
    let total_patients = patient_summary.len();
    pf_table.add_row(Row::new(vec![
        Cell::new("TOTAL PATIENTS").style_spec("bFc"),
        Cell::new("").style_spec("") ,
        Cell::new("").style_spec("") ,
        Cell::new(&format!("{}", total_patients)).style_spec("bFc"),
    ]));
    pf_table.printstd();
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
