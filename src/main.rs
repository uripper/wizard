use std::io;
use std::process::ExitCode;

use wizard::cli::{self, Action};

fn main() -> ExitCode {
    let parsed = match cli::parse_args(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    for warning in parsed.warnings {
        eprintln!("warning: {warning}");
    }

    match parsed.action {
        Action::Help => print!("{}", cli::help_text()),
        Action::Version => println!("{}", cli::version_text()),
        Action::Run {
            command: Some(command),
            options,
        } => {
            if let Err(error) = wizard::run(&command, &options, &mut io::stdout().lock()) {
                eprintln!("wizard: {error}");
                return ExitCode::FAILURE;
            }
        }
        Action::Run { command: None, .. } => println!("{}", cli::short_usage()),
    }
    ExitCode::SUCCESS
}
