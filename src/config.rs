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
    let mut args = env::args().skip(1); // skip program name

    let file_path = args.next().unwrap_or("fake_claims.jsonl".to_string());
    let ingest_rate: u64 = args
        .next()
        .unwrap_or("1".to_string())
        .parse()
        .expect("ingest_rate must be a valid u64");
    // Check for verbose flag in remaining args
    let verbose = args.any(|arg| arg == "verbose" || arg == "v");

    Config {
        file_path,
        ingest_rate,
        verbose,
    }
}
