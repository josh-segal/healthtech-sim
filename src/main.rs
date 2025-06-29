use std::env;
use std::fs;

fn main() {
    // TODO: add configurable claims per second rate
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        panic!("No file_path provided");
    }
    let file_path = args[1].clone(); // TODO: clone not efficient

    println!("Reading file {}", file_path);

    let content = fs::read_to_string(file_path)
        .expect("Should have been able to read the file");

    println!("With text:\n{content}");
}