use tokio::fs::File;
use tokio::io::{BufReader, AsyncBufReadExt};
use tokio::sync::mpsc::Sender;

use crate::schema::PayerClaim;

pub async fn stream_claims(
    path: &str,
    sender: Sender<PayerClaim>,
) -> anyhow::Result<()> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<PayerClaim>(&line) {
            Ok(claim) => {
                if sender.send(claim).await.is_err() {
                    eprintln!("Biller receiver dropped");
                    break;
                }
            }
            Err(err) => eprintln!("Invalid claim skipped: {}", err),
        }
    }

    Ok(())
}
