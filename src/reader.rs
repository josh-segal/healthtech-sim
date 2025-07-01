use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::Sender;

use crate::logging::log_claim_event;
use crate::schema::PayerClaim;

/// Stream claims from a JSONL file and send them to the biller
/// 
/// Reads claims line by line, parses JSON, and forwards valid claims
/// Skips invalid JSON lines and continues processing
pub async fn stream_claims(
    path: &str,
    tx: Sender<PayerClaim>,
    verbose: bool,
) -> anyhow::Result<()> {
    if verbose {
        log_claim_event(
            "reader",
            "-",
            "start",
            &format!("Starting claim stream from file: {}", path),
        );
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
                    log_claim_event(
                        "reader",
                        &claim.claim_id,
                        "sending_claim",
                        &format!("Sending parsed claim: {}", &claim.claim_id),
                    );
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
        log_claim_event(
            "reader",
            "-",
            "finished",
            &format!("Finished streaming claims from file: {}", path),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::mock_claim;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Test that valid claims in a file are parsed and sent to the channel.
    /// Expected: All valid claims are sent; function returns Ok.
    #[tokio::test]
    async fn test_stream_claims_happy_path() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        let claim = mock_claim();
        let json = serde_json::to_string(&claim).unwrap();
        writeln!(tmpfile, "{}", json).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let path = tmpfile.path().to_str().unwrap();
        let result = stream_claims(path, tx, false).await;
        assert!(result.is_ok());
        let received = rx.recv().await.expect("Expected a claim");
        assert_eq!(received.claim_id, claim.claim_id);
    }

    /// Test that invalid JSON lines are skipped and do not panic or break the stream.
    /// Expected: Only valid claims are sent; errors are logged; function returns Ok.
    #[tokio::test]
    async fn test_stream_claims_invalid_json() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        let claim = mock_claim();
        let json = serde_json::to_string(&claim).unwrap();
        writeln!(tmpfile, "not a json").unwrap();
        writeln!(tmpfile, "{}", json).unwrap();
        writeln!(tmpfile, "{{ invalid json }}").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let path = tmpfile.path().to_str().unwrap();
        let result = stream_claims(path, tx, false).await;
        assert!(result.is_ok());
        let received = rx.recv().await.expect("Expected a claim");
        assert_eq!(received.claim_id, claim.claim_id);
        // No more claims should be sent
        assert!(rx.try_recv().is_err());
    }

    /// Test that if the receiver drops, the function exits gracefully.
    /// Expected: Function returns Ok after logging the error.
    #[tokio::test]
    async fn test_stream_claims_biller_channel_dropped() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        let claim = mock_claim();
        let json = serde_json::to_string(&claim).unwrap();
        writeln!(tmpfile, "{}", json).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        drop(_rx); // Drop receiver to simulate biller down
        let path = tmpfile.path().to_str().unwrap();
        let result = stream_claims(path, tx, false).await;
        assert!(result.is_ok());
    }
}
