use tokio::fs::File;
use tokio::io::{BufReader, AsyncBufReadExt};
use tokio::sync::mpsc::Sender;

use crate::schema::PayerClaim;
use crate::logging::log_claim_event;

pub async fn stream_claims(
    path: &str,
    tx: Sender<PayerClaim>,
    verbose: bool,
) -> anyhow::Result<()> {
    if verbose {
        log_claim_event("reader", "-", "start", &format!("Starting claim stream from file: {}", path));
    }
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    // let mut line_num = 0;
    while let Some(line) = lines.next_line().await? {
        // line_num += 1;
        // if verbose {
        //     log_claim_event("reader", "-", "reading_line", &format!("Reading line {}", line_num));
        // }
        match serde_json::from_str::<PayerClaim>(&line) {
            Ok(claim) => {
                if verbose {
                    // log_claim_event("reader", &claim.claim_id, "parsed_claim", &format!("Parsed claim: {}", &claim.claim_id));
                    log_claim_event("reader", &claim.claim_id, "sending_claim", &format!("Sending parsed claim: {}", &claim.claim_id));
                }
                if tx.send(claim).await.is_err() {
                    eprintln!("Biller receiver dropped");
                    break;
                }
            }
            Err(err) => eprintln!("Invalid claim skipped: {}", err),
        }
    }
    if verbose {
        log_claim_event("reader", "-", "finished", &format!("Finished streaming claims from file: {}", path));
    }
    Ok(())
}
