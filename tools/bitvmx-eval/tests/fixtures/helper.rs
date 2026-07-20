//! Source-only synthetic helper for the BitVMX-CPU report-boundary tests.
//!
//! This is not an upstream binary and is never used as a production evaluator.

use std::{
    env,
    fs,
    io::{self, Write},
    thread,
    time::Duration,
};

fn fixture_path() -> String {
    let args: Vec<String> = env::args().collect();
    args.windows(2)
        .find(|pair| pair[0] == "--elf")
        .map(|pair| pair[1].clone())
        .expect("--elf fixture argument is required")
}

fn main() {
    let scenario = fs::read_to_string(fixture_path()).expect("fixture must be readable");
    match scenario.trim() {
        "success" => println!("INFO Execution result: Halt(0, 7)"),
        "failure" => println!("INFO Execution result: Halt(7, 9)"),
        "malformed" => println!("INFO Execution result: NotARealBitVMXResult"),
        "nonzero" => {
            println!("INFO Execution result: Halt(0, 7)");
            std::process::exit(23);
        }
        "timeout" => {
            thread::sleep(Duration::from_secs(5));
            println!("INFO Execution result: Halt(0, 7)");
        }
        "rss" => {
            let mut memory = vec![0_u8; 64 * 1024 * 1024];
            for page in memory.chunks_mut(4096) {
                page[0] = 1;
            }
            thread::sleep(Duration::from_secs(5));
            println!("INFO Execution result: Halt(0, 7)");
            io::sink().write_all(&memory[..1]).expect("sink write");
        }
        "output" => {
            let chunk = "x".repeat(8192);
            for _ in 0..512 {
                print!("{chunk}");
            }
            io::stdout().flush().expect("flush output");
        }
        "unexpected" => println!("INFO Execution result: Halt(0, 7)"),
        other => panic!("unknown synthetic fixture scenario: {other}"),
    }
}
