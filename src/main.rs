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
        Action::Run { commands, options } => {
            if commands.is_empty() {
                eprintln!("{}", cli::short_usage());
                return ExitCode::FAILURE;
            }
            let mut resolved_all = true;
            for command in commands {
                match wizard::run(&command, &options, &mut io::stdout().lock()) {
                    Ok(true) => {}
                    Ok(false) => {
                        resolved_all = false;
                    }
                    Err(error) => {
                        eprintln!("wizard: {error}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            if !resolved_all {
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
