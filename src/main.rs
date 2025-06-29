use std::env;
use std::fs;

mod schema;

// TODO: runtime execute tasks on multiple threads (each thread using async concurrency) to make use of multiple CPU cores?
// concurrency + parallelism
#[tokio::main]
async fn main() {
    println!("Hello World!");
}