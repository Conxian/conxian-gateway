use std::{path::PathBuf, process::ExitCode};

use clap::Parser;

use crate::{model::WARNING, runner::run_manifest};

#[derive(Debug, Parser)]
#[command(
    name = "bitvmx-eval",
    version,
    about = "Research / Evaluation Only: bounded external BitVMX-CPU execution",
    after_help = WARNING
)]
struct Args {
    /// Versioned JSON input manifest.
    #[arg(long, value_name = "PATH")]
    manifest: PathBuf,

    /// Versioned JSON report output path.
    #[arg(long, value_name = "PATH")]
    report: PathBuf,
}

pub fn run() -> ExitCode {
    let args = Args::parse();
    eprintln!("{WARNING}");

    match run_manifest(&args.manifest, &args.report) {
        Ok(report) => {
            match serde_json::to_string_pretty(&report) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("failed to serialize report: {error}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
