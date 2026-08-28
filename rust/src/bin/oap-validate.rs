use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use open_agent_profile::validate_path;

#[derive(Parser)]
#[command(version, about = "Validate Open Agent Profile documents")]
struct Arguments {
    #[arg(long)]
    strict: bool,
    #[arg(long)]
    digest: bool,
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

fn main() -> ExitCode {
    let args = Arguments::parse();
    let mut failed = false;
    for path in args.paths {
        let report = validate_path(&path);
        for error in &report.errors {
            eprintln!("error: {}: {}", error.pointer, error.message);
        }
        for warning in &report.warnings {
            eprintln!("warn: {}: {}", warning.pointer, warning.message);
        }
        if !report.ok || (args.strict && !report.warnings.is_empty()) {
            failed = true;
        } else {
            println!("{}: valid {}", path.display(), report.kind);
        }
        if args.digest {
            if let Some(digests) = report.digests {
                println!("  profile {}\n  spec    {}", digests.profile, digests.spec);
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
