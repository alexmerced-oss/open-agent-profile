use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use open_agent_profile::{
    ApplyOptions, OapFormat, apply_delta, load, serialize, validate, write_atomically,
};

#[derive(Parser)]
#[command(version, about = "Safely apply an OAP state delta")]
struct Arguments {
    #[arg(long)]
    approve: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, default_value = "oap-rust")]
    actor: String,
    profile: PathBuf,
    delta: PathBuf,
}

fn main() -> ExitCode {
    let args = Arguments::parse();
    let profile = match load(&args.profile) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let delta = match load(&args.delta) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    for (path, document) in [(&args.profile, &profile), (&args.delta, &delta)] {
        let report = validate(document, Some(path));
        if !report.ok {
            for error in report.errors {
                eprintln!("{}: {}: {}", path.display(), error.pointer, error.message);
            }
            return ExitCode::FAILURE;
        }
    }
    let result = match apply_delta(
        &profile,
        &delta,
        ApplyOptions {
            approved: args.approve,
            actor: Some(&args.actor),
            now: None,
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let format = OapFormat::from_path(&args.profile);
    let encoded = match serialize(&result.profile, format) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    for warning in result.warnings {
        eprintln!("warn: {warning}");
    }
    if !result.pending_proposals.is_empty() {
        eprintln!(
            "{} proposal(s) require human review and were not applied",
            result.pending_proposals.len()
        );
    }
    if args.dry_run {
        print!("{encoded}");
    } else if let Err(error) = write_atomically(&args.profile, encoded.as_bytes()) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    eprintln!(
        "{} revision {}",
        if args.dry_run { "would write" } else { "wrote" },
        result
            .profile
            .get("metadata")
            .and_then(|value| value.get("revision"))
            .and_then(|value| value.as_u64())
            .unwrap_or_default()
    );
    ExitCode::SUCCESS
}
