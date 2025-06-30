use std::env;

pub struct Config {
    pub file_path: String,
    pub ingest_rate: u64,
}

// TODO: replace with clap for arg validation and error reporting?
pub fn config() -> Config {
    let mut args = env::args().skip(1); // skip program name

    let file_path = args.next().expect("Usage: program <file_path> [ingest_rate]");
    let ingest_rate: u64 = args.next().unwrap_or("1".to_string()).parse()
        .expect("ingest_rate must be a valid u32");

    Config { file_path, ingest_rate }


}