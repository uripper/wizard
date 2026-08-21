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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Auto,
    Pretty,
    Plain,
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
    pub all: bool,
    pub ignore_patterns: Vec<String>,
    pub ignored_directories: Vec<String>,
    pub include_windows: bool,
    pub num_matches: usize,
    pub output_format: OutputFormat,
    pub sensitivity: f64,
    pub show_dot: bool,
    pub show_tilde: bool,
    pub skip_dot: bool,
    pub skip_tilde: bool,
    pub threshold: f64,
    pub tty_only: bool,
    pub verbose: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::JaroWinkler,
            all: false,
            ignore_patterns: Vec::new(),
            ignored_directories: Vec::new(),
            include_windows: false,
            num_matches: 5,
            output_format: OutputFormat::Auto,
            sensitivity: 1.0,
            show_dot: false,
            show_tilde: false,
            skip_dot: false,
            skip_tilde: false,
            threshold: 0.75,
            tty_only: false,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LookupResult {
    pub command: String,
    pub result: search::SearchResult,
}

impl LookupResult {
    pub fn resolved(&self) -> bool {
        match &self.result {
            search::SearchResult::Exact(paths) => !paths.is_empty(),
            search::SearchResult::Suggestions { matches, .. } => !matches.is_empty(),
        }
    }
}

pub fn lookup(command: impl Into<String>, options: &SearchOptions) -> LookupResult {
    let command = command.into();
    let result = search::search(&command, options);
    LookupResult { command, result }
}

pub fn run(command: &str, options: &SearchOptions, writer: &mut impl Write) -> io::Result<bool> {
    let result = lookup(command, options);
    let resolved = result.resolved();
    output::print_results(writer, std::slice::from_ref(&result), options, false)?;
    Ok(resolved)
}
