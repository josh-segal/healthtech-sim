use std::env;

/// Application configuration for claim processing simulation
#[derive(Clone)]
pub struct Config {
    pub file_path: String,
    pub ingest_rate: u64,
    pub verbose: bool,
}

/// Parse command line arguments to create application configuration
/// 
/// Args: [file_path] [ingest_rate] [verbose_flag]
/// - file_path: JSONL file with claims (default: fake_claims.jsonl)
/// - ingest_rate: seconds between claim processing (default: 1)
/// - verbose: enable detailed logging (default: false)
pub fn config() -> Config {
    let args: Vec<String> = env::args().skip(1).collect();

    let file_path = args.get(0).cloned().unwrap_or_else(|| "fake_claims.jsonl".to_string());

    let ingest_rate = args.get(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1);

    let verbose = args.iter().any(|arg| arg == "verbose" || arg == "v");

    Config {
        file_path,
        ingest_rate,
        verbose,
    }
}
