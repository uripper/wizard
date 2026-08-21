use std::io::{self, IsTerminal};
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
            let results: Vec<_> = commands
                .into_iter()
                .map(|command| wizard::lookup(command, &options))
                .collect();
            let resolved_all = results.iter().all(wizard::LookupResult::resolved);

            let stdout = io::stdout();
            let stdout_is_terminal = stdout.is_terminal();
            if let Err(error) = wizard::output::print_results(
                &mut stdout.lock(),
                &results,
                &options,
                stdout_is_terminal,
            ) {
                eprintln!("wizard: {error}");
                return ExitCode::FAILURE;
            }
            if !resolved_all {
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
