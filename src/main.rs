use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use anyhow::Result;

mod biller;
mod clearinghouse;
mod config;
mod message;
mod payer;
mod reader;
mod remittance;
mod reporter;
mod schema;

// TODO: runtime execute tasks on multiple threads (each thread using async concurrency) to make use of multiple CPU cores?
// concurrency + parallelism
#[tokio::main]
async fn main() -> Result<()> {
    // parse CLI args
    let config = config::config();

    // channels
    // input reader -> biller
    let (claim_input_tx, claim_input_rx) = mpsc::channel::<schema::PayerClaim>(100);

    // biller -> clearinghouse
    let (claim_tx, claim_rx) = mpsc::channel::<message::ClaimMessage>(100);

    // clearinghouse -> payer
    //TODO: abstract payer setup to not be manual
    let (payer1_tx, payer1_rx) = mpsc::channel::<message::PayerMessage>(100);
    let (payer2_tx, payer2_rx) = mpsc::channel::<message::PayerMessage>(100);
    let (payer3_tx, payer3_rx) = mpsc::channel::<message::PayerMessage>(100);

    let payer_txs = HashMap::from([
        ("medicare".to_string(), payer1_tx),
        ("united_health_group".to_string(), payer2_tx),
        ("anthem".to_string(), payer3_tx),
    ]);

    // payer -> clearinghouse
    let (remit_tx, remit_rx) = mpsc::channel::<message::RemittanceMessage>(100);

    // shared
    //TODO: check correctness
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let claim_timestamps = Arc::new(Mutex::new(HashMap::new()));

    // spawn tasks
    // biller task
    tokio::spawn(biller::run_biller(
        config.clone(),
        claim_input_rx,
        claim_tx,
        #[cfg(test)]
        None,
    ));

    // clearinghouse task
    let clearinghouse = clearinghouse::Clearinghouse::new(claim_rx, payer_txs, remit_rx, biller_txs.clone(), claim_timestamps.clone());
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    // payer tasks
    let payer1 = payer::Payer::new("medicare".into(), 1, 3, remit_tx.clone(), payer1_rx);
    let payer2 = payer::Payer::new(
        "united_health_group".into(),
        2,
        4,
        remit_tx.clone(),
        payer2_rx,
    );
    let payer3 = payer::Payer::new("anthem".into(), 1, 2, remit_tx.clone(), payer3_rx);

    tokio::spawn(async move { payer1.run().await });
    tokio::spawn(async move { payer2.run().await });
    tokio::spawn(async move { payer3.run().await });

    // reader task
    tokio::spawn(async move {
        if let Err(e) = reader::stream_claims(&config.file_path, claim_input_tx).await {
            eprintln!("Claim stream failed: {:?}", e);
        }
    });

    // shutdown
    tokio::signal::ctrl_c().await?;
    println!("Shutdown signal received.");
    Ok(())

}
