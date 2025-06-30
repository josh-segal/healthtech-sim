use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{Duration};

use crate::schema::PayerClaim;
use crate::message::{ClaimMessage, ClaimEnvelope};
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
// TODO: possibly add struct to enable multiple billers in program
pub async fn run_biller(
    config: Config,
    mut rx: Receiver<PayerClaim>,
    tx: Sender<ClaimMessage>,
    #[cfg(test)] test_notify: Option<Sender<String>>, //optional notifcation for remittance
) -> anyhow::Result<()> {
    let interval = Duration::from_secs(config.ingest_rate as u64);
    let mut ticker = tokio::time::interval(interval);

    while let Some(claim) = rx.recv().await {
        // ingest throttle
        ticker.tick().await;
        // Create a one-time channel for this claim
        let (rem_tx, mut rem_rx) = tokio::sync::mpsc::channel(1);

        // clone for logging
        let claim_id = claim.claim_id.clone();
        

        #[cfg(test)]
        let test_notify_opt = test_notify.clone();

        #[cfg(not(test))]
        let test_notify_opt: Option<Sender<String>> = None;
        
        // spawn a task to wait for the remittance for this claim
        tokio::spawn(async move {
            if let Some(response) = rem_rx.recv().await {
                println!("Biller received remittance for claim {}: {:?}", claim_id, response);
                //TODO: update stats AR etc
                if let Some(tx) = test_notify_opt {
                    let _ = tx.send(claim_id).await;
                }
            }
        });
        let envelope = ClaimEnvelope {
            claim,
            response_tx: rem_tx,
        };

        if tx.send(ClaimMessage::NewClaim(envelope)).await.is_err() {
            eprintln!("Clearinghouse dropped");
            return Err(anyhow::anyhow!("Clearinghouse channel dropped"))
        }
    }


    Ok(())

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::RemittanceMessage;
    use crate::{remittance::mock_remittance, schema::mock_claim};

    #[tokio::test]
    async fn test_run_biller() {
        // mock config
        let mock_config = Config{ 
            file_path: "mock_path.json".to_string(), 
            ingest_rate: 1 
        };

        // input channel for claims
        let (claim_tx, claim_rx) = tokio::sync::mpsc::channel(1);

        // output channel for ClaimMessage -> Clearinghouse
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(1);

        // notifcation channel
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(1);

        // spawn biller task
        tokio::spawn(async move {
            let _ = run_biller(mock_config, claim_rx, out_tx, Some(notify_tx)).await;
        });

        // send a mock claim
        let mock_claim = mock_claim();
        claim_tx.send(mock_claim).await.unwrap(); // panic if send fails

        // receive envelope sent to clearinghouse and assert correctness
        if let Some(ClaimMessage::NewClaim(envelope)) = out_rx.recv().await {
            assert_eq!(envelope.claim.claim_id, "abc123");

        // simulate remittance being returned
        let mock_remittance = mock_remittance();
        let _ = envelope.response_tx
            .send(RemittanceMessage::Processed(mock_remittance))
            .await;
        } else {
            panic!("Expected ClaimMessage::NewClaim");
        }

        let notified_id = notify_rx.recv().await.expect("Expected remittance notification");
        assert_eq!(notified_id, "abc123");
    }
}