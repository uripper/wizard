pub mod cli;
pub mod output;
pub mod search;
pub mod similarity;

use std::io::{self, Write};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    JaroWinkler,
    Levenshtein,
}

impl std::fmt::Display for Algorithm {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JaroWinkler => formatter.write_str("jaro_winkler"),
            Self::Levenshtein => formatter.write_str("levenshtein"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchOptions {
    pub algorithm: Algorithm,
    pub ignore_patterns: Vec<String>,
    pub ignored_directories: Vec<String>,
    pub include_windows: bool,
    pub num_matches: usize,
    pub sensitivity: f64,
    pub threshold: f64,
    pub verbose: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::JaroWinkler,
            ignore_patterns: Vec::new(),
            ignored_directories: Vec::new(),
            include_windows: false,
            num_matches: 5,
            sensitivity: 1.0,
            threshold: 0.75,
            verbose: false,
        }
    }
}

pub fn run(command: &str, options: &SearchOptions, writer: &mut impl Write) -> io::Result<()> {
    if options.verbose {
        writeln!(writer, "Searching for '{command}' in PATH...")?;
        writeln!(
            writer,
            "Sensitivity: {}, Algorithm: {}, Threshold: {}",
            options.sensitivity, options.algorithm, options.threshold
        )?;
        writeln!(
            writer,
            "Ignoring: {}, Ignored Directories: {}",
            options.ignore_patterns.join(", "),
            options.ignored_directories.join(", ")
        )?;
        if search::windows_mounts_excluded(options) {
            writeln!(
                writer,
                "Windows PATH directories: excluded from fuzzy search (use --include-windows to scan them)"
            )?;
        }
    }

    match search::search(command, options) {
        search::SearchResult::Exact(path) => {
            if options.verbose {
                writeln!(writer, "Exact match found: {}", path.display())?;
            }
            writeln!(writer, "{}", path.display())
        }
        search::SearchResult::Suggestions {
            matches,
            candidate_count,
        } => {
            if options.verbose {
                writeln!(
                    writer,
                    "Exact match not found. Gathering all executables from PATH..."
                )?;
                writeln!(writer, "Total unique executables found: {candidate_count}")?;
            }
            output::print_suggestions(writer, &matches, command, options.verbose)
        }
    }
}
