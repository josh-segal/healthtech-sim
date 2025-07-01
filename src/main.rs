use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{Mutex, mpsc};

use healthtechsim::biller;
use healthtechsim::clearinghouse;
use healthtechsim::config;
use healthtechsim::json_faker;
use healthtechsim::payer;
use healthtechsim::reader;
use healthtechsim::reporter;
use healthtechsim::schema;

/// Healthcare claim processing simulation
/// 
/// Orchestrates the complete workflow: reader → biller → clearinghouse → payer → remittance
#[tokio::main]
async fn main() -> Result<()> {
    // parse CLI args

    // for simulation
    let claims = 100;
    json_faker::write_fake_claims_jsonl("fake_claims.jsonl", claims)
        .expect("Failed to write fake claims");
    println!("Wrote {} fake claims to fake_claims.jsonl", claims);

    let config = config::config();

    // channels
    // input reader -> biller
    let (claim_input_tx, claim_input_rx) = mpsc::channel::<schema::PayerClaim>(100);

    // biller -> clearinghouse
    let (claim_tx, claim_rx) = mpsc::channel::<healthtechsim::message::ClaimMessage>(100);

    // clearinghouse -> payer
    //TODO: abstract payer setup to not be manual ?
    let (payer1_tx, payer1_rx) = mpsc::channel::<healthtechsim::message::PayerMessage>(100);
    let (payer2_tx, payer2_rx) = mpsc::channel::<healthtechsim::message::PayerMessage>(100);
    let (payer3_tx, payer3_rx) = mpsc::channel::<healthtechsim::message::PayerMessage>(100);

    let payer_txs = HashMap::from([
        ("medicare".to_string(), payer1_tx),
        ("united_health_group".to_string(), payer2_tx),
        ("anthem".to_string(), payer3_tx),
    ]);

    // payer -> clearinghouse
    let (remit_tx, remit_rx) = mpsc::channel::<healthtechsim::message::RemittanceMessage>(100);

    // shared
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    // spawn tasks
    // biller task
    tokio::spawn(biller::run_biller(
        config.clone(),
        claim_input_rx,
        claim_tx,
        None,
    ));

    // clearinghouse task
    let clearinghouse = clearinghouse::Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        config.verbose,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });

    let reporter_history = remittance_history.clone();
    tokio::spawn(async move {
        reporter::run_reporter(reporter_history, config.verbose).await;
    });

    // payer tasks
    let payer1 = payer::Payer::new(
        "medicare".into(),
        1,
        3,
        remit_tx.clone(),
        payer1_rx,
        config.verbose,
    );
    let payer2 = payer::Payer::new(
        "united_health_group".into(),
        2,
        4,
        remit_tx.clone(),
        payer2_rx,
        config.verbose,
    );
    let payer3 = payer::Payer::new(
        "anthem".into(),
        1,
        2,
        remit_tx.clone(),
        payer3_rx,
        config.verbose,
    );

    tokio::spawn(async move { payer1.run().await });
    tokio::spawn(async move { payer2.run().await });
    tokio::spawn(async move { payer3.run().await });

    // reader task
    tokio::spawn(async move {
        if let Err(e) =
            reader::stream_claims(&config.file_path, claim_input_tx, config.verbose).await
        {
            eprintln!("Claim stream failed: {:?}", e);
        }
    });

    // shutdown
    tokio::signal::ctrl_c().await?;
    println!("Shutdown signal received.");
    Ok(())
}
