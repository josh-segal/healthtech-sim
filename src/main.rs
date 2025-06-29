mod schema;
mod config;
mod remittance;
mod message;

// TODO: runtime execute tasks on multiple threads (each thread using async concurrency) to make use of multiple CPU cores?
// concurrency + parallelism
#[tokio::main]
async fn main() {
    println!("Hello World!");
    let config = config::config();

    println!("File path: {}", config.file_path);
    println!("Ingest rate: {}", config.ingest_rate);
}