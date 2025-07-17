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
    let claims = 10;
    json_faker::write_fake_claims_jsonl("fake_claims.jsonl", claims)
        .expect("Failed to write fake claims");
    println!("Wrote {} fake claims to fake_claims.jsonl", claims);

    let config = config::config();
    println!("Config settings: file_path={}, ingest_rate={}, verbose={}", config.file_path, config.ingest_rate, config.verbose);

    // channels
    let (claim_input_tx, claim_input_rx) = mpsc::channel::<schema::PayerClaim>(100);
    let (claim_tx, claim_rx) = mpsc::channel::<healthtechsim::message::ClaimMessage>(100);
    let (payer1_tx, payer1_rx) = mpsc::channel::<healthtechsim::message::PayerMessage>(100);
    let (payer2_tx, payer2_rx) = mpsc::channel::<healthtechsim::message::PayerMessage>(100);
    let (payer3_tx, payer3_rx) = mpsc::channel::<healthtechsim::message::PayerMessage>(100);
    let payer_txs = HashMap::from([
        ("medicare".to_string(), payer1_tx),
        ("united_health_group".to_string(), payer2_tx),
        ("anthem".to_string(), payer3_tx),
    ]);
    let (remit_tx, remit_rx) = mpsc::channel::<healthtechsim::message::RemittanceMessage>(100);
    let biller_txs = Arc::new(Mutex::new(HashMap::new()));
    let remittance_history = Arc::new(Mutex::new(HashMap::new()));

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1); //TODO: change into pattern that guarantees only one sender

    // setup and spawn tasks
    setup_biller_task(
        config.clone(),
        claim_input_rx,
        claim_tx.clone(),
        claims,
        shutdown_tx,
    );
    setup_clearinghouse_task(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs.clone(),
        remittance_history.clone(),
        config.verbose,
    );
    setup_reporter_task(remittance_history.clone(), config.verbose);
    setup_payer_tasks(
        remit_tx.clone(),
        payer1_rx,
        payer2_rx,
        payer3_rx,
        config.verbose,
    );
    setup_reader_task(&config.file_path, claim_input_tx, config.verbose);

    // shutdown
    tokio::select! {
        _ = shutdown_rx.recv() => {
            println!("All remittances received. Shutting down.");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("Shutdown signal received.");
        }
    }
    Ok(())
}

fn setup_biller_task(
    config: config::Config,
    claim_input_rx: mpsc::Receiver<schema::PayerClaim>,
    claim_tx: mpsc::Sender<healthtechsim::message::ClaimMessage>,
    total_claims: usize,
    shutdown_tx: mpsc::Sender<()>,
) {
    tokio::spawn(biller::run_biller(config, claim_input_rx, claim_tx, None, total_claims, shutdown_tx));
}

fn setup_clearinghouse_task(
    claim_rx: mpsc::Receiver<healthtechsim::message::ClaimMessage>,
    payer_txs: HashMap<String, mpsc::Sender<healthtechsim::message::PayerMessage>>,
    remit_rx: mpsc::Receiver<healthtechsim::message::RemittanceMessage>,
    biller_txs: Arc<
        Mutex<HashMap<String, mpsc::Sender<healthtechsim::message::RemittanceMessage>>>,
    >,
    remittance_history: Arc<Mutex<HashMap<String, healthtechsim::message::ClaimStatus>>>,
    verbose: bool,
) {
    let clearinghouse = clearinghouse::Clearinghouse::new(
        claim_rx,
        payer_txs,
        remit_rx,
        biller_txs,
        remittance_history,
        verbose,
    );
    tokio::spawn(async move {
        clearinghouse.run().await;
    });
}

fn setup_reporter_task(
    remittance_history: Arc<Mutex<HashMap<String, healthtechsim::message::ClaimStatus>>>,
    verbose: bool,
) {
    tokio::spawn(async move {
        reporter::run_reporter(remittance_history, verbose).await;
    });
}

fn setup_payer_tasks(
    remit_tx: mpsc::Sender<healthtechsim::message::RemittanceMessage>,
    payer1_rx: mpsc::Receiver<healthtechsim::message::PayerMessage>,
    payer2_rx: mpsc::Receiver<healthtechsim::message::PayerMessage>,
    payer3_rx: mpsc::Receiver<healthtechsim::message::PayerMessage>,
    verbose: bool,
) {
    let payer1 = payer::Payer::new(
        "medicare".into(),
        10,
        30,
        remit_tx.clone(),
        payer1_rx,
        verbose,
    );
    let payer2 = payer::Payer::new(
        "united_health_group".into(),
        5,
        6,
        remit_tx.clone(),
        payer2_rx,
        verbose,
    );
    let payer3 = payer::Payer::new(
        "anthem".into(),
         60,
          100,
           remit_tx.clone(), 
           payer3_rx, 
           verbose
    );
    tokio::spawn(async move { payer1.run().await });
    tokio::spawn(async move { payer2.run().await });
    tokio::spawn(async move { payer3.run().await });
}

fn setup_reader_task(
    file_path: &str,
    claim_input_tx: mpsc::Sender<schema::PayerClaim>,
    verbose: bool,
) {
    let file_path = file_path.to_string();
    tokio::spawn(async move {
        if let Err(e) = reader::stream_claims(&file_path, claim_input_tx, verbose).await {
            eprintln!("Claim stream failed: {:?}", e);
        }
    });
}
