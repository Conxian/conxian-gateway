//! Source-only synthetic helper for the BitVMX-CPU report-boundary tests.
//!
//! This is not an upstream binary and is never used as a production evaluator.

use std::{
    env,
    fs,
    io::{self, Write},
    process::Command,
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

fn sibling_path(fixture: &str, name: &str) -> std::path::PathBuf {
    std::path::Path::new(fixture)
        .parent()
        .expect("fixture parent")
        .join(name)
}

fn main() {
    let fixture = fixture_path();
    let scenario = fs::read_to_string(&fixture).expect("fixture must be readable");
    match scenario.trim() {
        "success" => println!("INFO Execution result: Halt(0, 7)"),
        "failure" => println!("INFO Execution result: Halt(7, 9)"),
        "limit" => println!("INFO Execution result: LimitStepReached(100)"),
        "malformed" => println!("INFO Execution result: NotARealBitVMXResult"),
        "spoof" => {
            println!("INFO Execution result: Halt(0, 7)");
            println!("DEBUG Execution result: Halt(0, 7)");
        }
        "same-class-return" => println!("INFO Execution result: Halt(9, 9)"),
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
        "mutate-fixture" => {
            fs::write(&fixture, "mutated-after-start").expect("mutate fixture");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "delete-fixture" => {
            fs::remove_file(&fixture).expect("delete fixture");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "mutate-revision" => {
            fs::write(sibling_path(&fixture, "synthetic-helper.revision"), b"tampered\n")
                .expect("mutate revision");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "delete-revision" => {
            fs::remove_file(sibling_path(&fixture, "synthetic-helper.revision"))
                .expect("delete revision");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "delete-executable" => {
            fs::remove_file(env::current_exe().expect("current executable"))
                .expect("delete executable");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "delete-artifact" => {
            fs::remove_file(sibling_path(&fixture, "artifact.bin")).expect("delete artifact");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "artifact-report-alias" => {
            let report = sibling_path(&fixture, "report.json");
            fs::write(&report, b"not-a-report").expect("create report collision");
            fs::hard_link(&report, sibling_path(&fixture, "artifact.bin"))
                .expect("create artifact/report hard link");
            println!("INFO Execution result: Halt(0, 7)");
        }
        "descendant" => {
            let _child = Command::new("sh")
                .args(["-c", "sleep 5"])
                .spawn()
                .expect("spawn descendant");
            thread::sleep(Duration::from_secs(1));
            println!("INFO Execution result: Halt(0, 7)");
        }
        other => panic!("unknown synthetic fixture scenario: {other}"),
    }
}
