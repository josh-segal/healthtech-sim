use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration};

use crate::schema::PayerClaim;
use crate::message::{ClaimMessage, ClaimEnvelope, RemittanceMessage};
use crate::config::Config;

/// Biller task that processes claims received over a PayerClaim channel.
///
/// For each incoming claim:
/// - Creates a one-time channel for receiving the remittance response.
/// - Spawns a listener task to handle the remittance asynchronously.
/// - Wraps the claim and response channel in a `ClaimEnvelope`.
/// - Sends the envelope to the clearinghouse via the `ClaimMessage` channel.
///
/// The ingest rate is controlled by the configured interval.
pub async fn run_biller (
    config: Config,
    mut rx: Receiver<PayerClaim>,
    tx: Sender<ClaimMessage>,
) -> anyhow::Result<()> {
    let interval = Duration::from_secs(config.ingest_rate as u64);

    while let Some(claim) = rx.recv().await {
        // Create a one-time channel for this claim
        let (rem_tx, mut rem_rx) = tokio::sync::mpsc::channel(1);

        // clone for logging
        let claim_id = claim.claim_id.clone();
        // spawn a task to wait for the remittance for this claim
        tokio::spawn(async move {
            if let Some(response) = rem_rx.recv().await {
                println!("Biller received remittance for claim {}: {:?}", claim_id, response);
                //TODO: update stats AR etc
            }
        });
        let envelope = ClaimEnvelope {
            claim,
            response_tx: rem_tx,
        };

        if tx.send(ClaimMessage::NewClaim(envelope)).await.is_err() {
            eprintln!("Clearinghouse dropped");
            break;
        }
        sleep(interval).await;
    }


    Ok(())

}