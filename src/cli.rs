use std::fmt;
use std::io::IsTerminal;

use crate::{Algorithm, OutputFormat, SearchOptions, VERSION};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Run {
        commands: Vec<String>,
        options: SearchOptions,
    },
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArgs {
    pub action: Action,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

pub fn parse_args<I, S>(args: I) -> Result<ParsedArgs, ParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    parse_args_for_terminal(args, std::io::stdout().is_terminal())
}

fn parse_args_for_terminal<I, S>(
    args: I,
    stdout_is_terminal: bool,
) -> Result<ParsedArgs, ParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut options = SearchOptions::default();
    let mut commands = Vec::new();
    let mut warnings = Vec::new();
    let mut help = false;
    let mut version = false;
    let mut tty_only_blocks_display_options = false;
    let mut index = 0;

    while index < args.len() {
        let argument = &args[index];
        match argument.as_str() {
            "--help" => help = true,
            "--version" | "-v" | "-V" => version = true,
            "--all" | "-a" => options.all = true,
            "--show-dot" if !tty_only_blocks_display_options => options.show_dot = true,
            "--show-dot" => {}
            "--skip-dot" => options.skip_dot = true,
            "--show-tilde" if !tty_only_blocks_display_options => options.show_tilde = true,
            "--show-tilde" => {}
            "--skip-tilde" => options.skip_tilde = true,
            "--tty-only" => {
                options.tty_only = true;
                tty_only_blocks_display_options = !stdout_is_terminal;
            }
            "--verbose" => options.verbose = true,
            "--include-windows" => options.include_windows = true,
            "--" => {
                commands.extend(args[index + 1..].iter().cloned());
                break;
            }
            _ if option_value(argument, "sensitivity").is_some() => {
                let value = required_value(&args, &mut index, "sensitivity")?;
                options.sensitivity = parse_loose_float(&value).unwrap_or(1.0);
            }
            _ if option_value(argument, "algorithm").is_some() => {
                let value = required_value(&args, &mut index, "algorithm")?;
                options.algorithm = match value.as_str() {
                    "levenshtein" | "lev" | "l" => Algorithm::Levenshtein,
                    "jaro_winkler" | "jw" | "j" => Algorithm::JaroWinkler,
                    _ => Algorithm::JaroWinkler,
                };
            }
            _ if option_value(argument, "threshold").is_some() => {
                let value = required_value(&args, &mut index, "threshold")?;
                match normalize_float(&value).parse::<f64>() {
                    Ok(threshold) if (0.0..=1.0).contains(&threshold) => {
                        options.threshold = threshold;
                    }
                    _ => warnings.push(format!(
                        "Invalid threshold value: {}. Using default 0.75",
                        normalize_float(&value)
                    )),
                }
            }
            _ if option_value(argument, "matches").is_some() => {
                let value = required_value(&args, &mut index, "matches")?;
                options.num_matches = value
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value >= 1)
                    .ok_or_else(|| ParseError("--matches must be an integer ≥ 1".into()))?;
            }
            _ if option_value(argument, "format").is_some() => {
                let value = required_value(&args, &mut index, "format")?;
                options.output_format = match value.as_str() {
                    "auto" => OutputFormat::Auto,
                    "pretty" => OutputFormat::Pretty,
                    "plain" => OutputFormat::Plain,
                    _ => {
                        return Err(ParseError(format!(
                            "Invalid format '{value}'; expected auto, pretty, or plain"
                        )));
                    }
                };
            }
            _ if option_value(argument, "ignore").is_some() => {
                let value = required_value(&args, &mut index, "ignore")?;
                options.ignore_patterns = parse_list(&value);
            }
            _ if option_value(argument, "ignoredir").is_some() => {
                let value = required_value(&args, &mut index, "ignoredir")?;
                options.ignored_directories = parse_list(&value);
            }
            _ if argument.starts_with('-') => {
                return Err(ParseError(format!("Unrecognized option '{argument}'")));
            }
            _ => commands.push(argument.clone()),
        }
        index += 1;
    }

    let action = if help {
        Action::Help
    } else if version {
        Action::Version
    } else {
        Action::Run { commands, options }
    };

    Ok(ParsedArgs { action, warnings })
}

fn option_value<'a>(argument: &'a str, name: &str) -> Option<Option<&'a str>> {
    let long_name = format!("--{name}");
    if argument == long_name {
        Some(None)
    } else {
        argument.strip_prefix(&(long_name + "=")).map(Some)
    }
}

fn required_value(args: &[String], index: &mut usize, name: &str) -> Result<String, ParseError> {
    if let Some(Some(value)) = option_value(&args[*index], name) {
        return Ok(value.to_owned());
    }

    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| ParseError(format!("--{name} requires a value")))
}

fn normalize_float(value: &str) -> String {
    if value.starts_with('.') {
        format!("0{value}")
    } else {
        value.to_owned()
    }
}

fn parse_loose_float(value: &str) -> Option<f64> {
    let normalized = normalize_float(value);
    if let Ok(number) = normalized.parse::<f64>()
        && number.is_finite()
    {
        return Some(number);
    }

    let end = normalized
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(normalized.len()))
        .rev()
        .find(|end| normalized[..*end].parse::<f64>().is_ok())?;
    normalized[..end]
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn parse_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn help_text() -> String {
    String::from(
        "Usage: wizard [options] command [...]\n\n\
         Options:\n\
           -a, --all        Print all exact matches\n\
           --help           Show this help message\n\
           --verbose        Enable verbose mode\n\
           --sensitivity    Set sensitivity (float), default: 1.0\n\
           --algorithm      levenshtein | jaro_winkler, default: jaro_winkler\n\
           --threshold      Float 0-1, default: 0.75. Lower = more matches\n\
           --ignore         Comma-separated extensions to ignore\n\
           --ignoredir      Comma-separated directories to ignore\n\
           --include-windows  Include Windows PATH directories in fuzzy WSL searches\n\
           --matches        Number of matches to display, default: 5\n\
           --format         auto | pretty | plain, default: auto\n\
           --version, -v, -V  Show version information\n",
    )
}

pub fn short_usage() -> &'static str {
    "Usage: wizard [--verbose] [--threshold=0-1.0] [--algorithm=[\"lev\", \"jw\"]] <command>"
}

pub fn version_text() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults() {
        let parsed = parse_args(["spellcheck"]).unwrap();
        assert_eq!(
            parsed.action,
            Action::Run {
                commands: vec!["spellcheck".into()],
                options: SearchOptions::default(),
            }
        );
    }

    #[test]
    fn parses_fuzzy_options_in_any_position() {
        let parsed = parse_args([
            "spellcheck",
            "--verbose",
            "--sensitivity=.5",
            "--algorithm=lev",
            "--threshold",
            ".8",
            "--matches=3",
            "--ignore=.bat,.cmd",
            "--ignoredir=node_modules, vendor",
        ])
        .unwrap();

        let Action::Run { commands, options } = parsed.action else {
            panic!("expected run action");
        };
        assert_eq!(commands, ["spellcheck"]);
        assert!(options.verbose);
        assert_eq!(options.sensitivity, 0.5);
        assert_eq!(options.algorithm, Algorithm::Levenshtein);
        assert_eq!(options.threshold, 0.8);
        assert_eq!(options.num_matches, 3);
        assert_eq!(options.ignore_patterns, [".bat", ".cmd"]);
        assert_eq!(options.ignored_directories, ["node_modules", "vendor"]);
        assert!(!options.include_windows);
    }

    #[test]
    fn collects_every_command_operand() {
        let parsed = parse_args(["first", "second", "third"]).unwrap();
        let Action::Run { commands, .. } = parsed.action else {
            panic!("expected run action");
        };
        assert_eq!(commands, ["first", "second", "third"]);
    }

    #[test]
    fn option_terminator_makes_every_remaining_argument_an_operand() {
        let parsed = parse_args(["first", "--", "--all", "-x"]).unwrap();
        let Action::Run { commands, options } = parsed.action else {
            panic!("expected run action");
        };
        assert_eq!(commands, ["first", "--all", "-x"]);
        assert!(!options.all);
    }

    #[test]
    fn parses_which_lookup_options() {
        let parsed = parse_args([
            "--all",
            "--show-dot",
            "--skip-dot",
            "--show-tilde",
            "--skip-tilde",
            "--tty-only",
            "command",
        ])
        .unwrap();
        let Action::Run { commands, options } = parsed.action else {
            panic!("expected run action");
        };

        assert_eq!(commands, ["command"]);
        assert!(options.all);
        assert!(options.show_dot);
        assert!(options.skip_dot);
        assert!(options.show_tilde);
        assert!(options.skip_tilde);
        assert!(options.tty_only);
    }

    #[test]
    fn parses_short_which_options() {
        let parsed = parse_args(["-a", "command"]).unwrap();
        let Action::Run { options, .. } = parsed.action else {
            panic!("expected run action");
        };
        assert!(options.all);

        for option in ["-v", "-V"] {
            assert_eq!(parse_args([option]).unwrap().action, Action::Version);
        }
    }

    #[test]
    fn tty_only_blocks_later_display_options_when_stdout_is_not_a_terminal() {
        let parsed = parse_args_for_terminal(
            ["--tty-only", "--show-dot", "--show-tilde", "command"],
            false,
        )
        .unwrap();
        let Action::Run { options, .. } = parsed.action else {
            panic!("expected run action");
        };

        assert!(!options.show_dot);
        assert!(!options.show_tilde);
    }

    #[test]
    fn enables_windows_candidates_explicitly() {
        let parsed = parse_args(["--include-windows", "powershel.exe"]).unwrap();
        let Action::Run { options, .. } = parsed.action else {
            panic!("expected run action");
        };
        assert!(options.include_windows);
    }

    #[test]
    fn rejects_invalid_match_count() {
        assert_eq!(
            parse_args(["--matches=0", "spellcheck"])
                .unwrap_err()
                .to_string(),
            "--matches must be an integer ≥ 1"
        );
    }

    #[test]
    fn parses_output_formats() {
        for (value, expected) in [
            ("auto", OutputFormat::Auto),
            ("pretty", OutputFormat::Pretty),
            ("plain", OutputFormat::Plain),
        ] {
            let parsed = parse_args([format!("--format={value}"), "spellcheck".into()]).unwrap();
            let Action::Run { options, .. } = parsed.action else {
                panic!("expected run action");
            };
            assert_eq!(options.output_format, expected);
        }
    }

    #[test]
    fn rejects_invalid_output_format() {
        assert_eq!(
            parse_args(["--format=ornate", "spellcheck"])
                .unwrap_err()
                .to_string(),
            "Invalid format 'ornate'; expected auto, pretty, or plain"
        );
    }

    #[test]
    fn rejects_unrecognized_options() {
        assert_eq!(
            parse_args(["--unknown"]).unwrap_err().to_string(),
            "Unrecognized option '--unknown'"
        );
    }
}
