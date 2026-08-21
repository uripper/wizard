use std::env;
use std::io::{self, Write};

use unicode_segmentation::UnicodeSegmentation;

use crate::search::{SearchResult, Suggestion};
use crate::{LookupResult, OutputFormat, SearchOptions};

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const MAGENTA: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

const DEFAULT_WIDTH: usize = 100;
const MIN_WIDTH: usize = 40;
const MAX_WIDTH: usize = 120;

#[derive(Clone, Copy)]
enum CellStyle {
    Normal,
    Header,
    Exact,
    Suggestion,
    Dim,
}

pub fn print_results(
    writer: &mut impl Write,
    results: &[LookupResult],
    options: &SearchOptions,
    stdout_is_terminal: bool,
) -> io::Result<()> {
    let pretty = match options.output_format {
        OutputFormat::Auto => stdout_is_terminal && !term_is_dumb(),
        OutputFormat::Pretty => true,
        OutputFormat::Plain => false,
    };

    if pretty {
        print_pretty_results(writer, results, options, stdout_is_terminal)
    } else {
        print_plain_results(writer, results, options)
    }
}

fn print_plain_results(
    writer: &mut impl Write,
    results: &[LookupResult],
    options: &SearchOptions,
) -> io::Result<()> {
    for lookup in results {
        if options.verbose {
            print_verbose_search_context(writer, &lookup.command, options)?;
        }

        match &lookup.result {
            SearchResult::Exact(paths) => {
                if options.verbose {
                    for path in paths {
                        writeln!(writer, "Exact match found: {}", path.display())?;
                    }
                }
                for path in paths {
                    writeln!(writer, "{}", path.display())?;
                }
            }
            SearchResult::Suggestions {
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
                print_suggestions(writer, matches, &lookup.command, options.verbose)?;
            }
        }
    }
    Ok(())
}

fn print_verbose_search_context(
    writer: &mut impl Write,
    command: &str,
    options: &SearchOptions,
) -> io::Result<()> {
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
    if crate::search::windows_mounts_excluded(options) {
        writeln!(
            writer,
            "Windows PATH directories: excluded from fuzzy search (use --include-windows to scan them)"
        )?;
    }
    Ok(())
}

fn print_pretty_results(
    writer: &mut impl Write,
    results: &[LookupResult],
    options: &SearchOptions,
    stdout_is_terminal: bool,
) -> io::Result<()> {
    let width = pretty_width(results, options.verbose);
    let color = stdout_is_terminal && env::var_os("NO_COLOR").is_none() && !term_is_dumb();

    for (index, lookup) in results.iter().enumerate() {
        if index > 0 {
            writeln!(writer)?;
        }
        print_pretty_result(writer, lookup, options, width, color)?;
    }
    Ok(())
}

fn print_pretty_result(
    writer: &mut impl Write,
    lookup: &LookupResult,
    options: &SearchOptions,
    width: usize,
    color: bool,
) -> io::Result<()> {
    match &lookup.result {
        SearchResult::Exact(paths) => {
            print_top_border(writer, width, '✓', GREEN, &lookup.command, color)?;
            let summary = format!(
                "{} exact {}",
                paths.len(),
                plural(paths.len(), "match", "matches")
            );
            print_full_row(writer, width, &summary, CellStyle::Dim, color)?;
            print_exact_table(writer, width, paths, color)
        }
        SearchResult::Suggestions {
            matches,
            candidate_count,
        } if matches.is_empty() => {
            print_top_border(writer, width, '×', RED, &lookup.command, color)?;
            let mut summary = String::from("No exact match · no close suggestions");
            if options.verbose {
                summary.push_str(&format!(
                    " · {} {} scanned",
                    candidate_count,
                    plural(*candidate_count, "candidate", "candidates")
                ));
            }
            print_full_row(writer, width, &summary, CellStyle::Dim, color)?;
            print_full_bottom_border(writer, width)
        }
        SearchResult::Suggestions {
            matches,
            candidate_count,
        } => {
            print_top_border(writer, width, '≈', YELLOW, &lookup.command, color)?;
            let mut summary = format!(
                "No exact match · {} {}",
                matches.len(),
                plural(matches.len(), "suggestion", "suggestions")
            );
            if options.verbose {
                summary.push_str(&format!(
                    " · {} {} scanned",
                    candidate_count,
                    plural(*candidate_count, "candidate", "candidates")
                ));
            }
            print_full_row(writer, width, &summary, CellStyle::Dim, color)?;
            print_suggestion_table(
                writer,
                width,
                &lookup.command,
                matches,
                options.verbose,
                color,
            )
        }
    }
}

fn print_top_border(
    writer: &mut impl Write,
    width: usize,
    symbol: char,
    symbol_color: &str,
    command: &str,
    color: bool,
) -> io::Result<()> {
    let inner_width = width.saturating_sub(2);
    let command = sanitize(command);
    let command = truncate(&command, inner_width.saturating_sub(5));
    let symbol = styled(&symbol.to_string(), symbol_color, color);
    let command = styled(&command, BOLD, color);
    let label = format!("─ {symbol} {command} ");
    let fill = inner_width.saturating_sub(visible_width(&label));
    writeln!(writer, "╭{label}{}╮", "─".repeat(fill))
}

fn print_full_row(
    writer: &mut impl Write,
    width: usize,
    text: &str,
    style: CellStyle,
    color: bool,
) -> io::Result<()> {
    let cell_width = width.saturating_sub(4);
    let text = truncate(&sanitize(text), cell_width);
    write!(writer, "│ ")?;
    write_cell(writer, &text, cell_width, style, color)?;
    writeln!(writer, " │")
}

fn print_exact_table(
    writer: &mut impl Write,
    width: usize,
    paths: &[std::path::PathBuf],
    color: bool,
) -> io::Result<()> {
    let index_width = paths.len().max(1).to_string().len();
    let path_width = width.saturating_sub(7 + index_width);
    let widths = [index_width, path_width];

    print_rule(writer, '├', '┬', '┤', &widths)?;
    print_table_row(
        writer,
        &widths,
        &[
            ('#'.to_string(), CellStyle::Header),
            ("Location".to_string(), CellStyle::Header),
        ],
        color,
    )?;
    print_rule(writer, '├', '┼', '┤', &widths)?;
    for (index, path) in paths.iter().enumerate() {
        let path = sanitize(&path.to_string_lossy());
        print_table_row(
            writer,
            &widths,
            &[
                ((index + 1).to_string(), CellStyle::Exact),
                (truncate_middle(&path, path_width), CellStyle::Dim),
            ],
            color,
        )?;
    }
    print_rule(writer, '╰', '┴', '╯', &widths)
}

fn print_suggestion_table(
    writer: &mut impl Write,
    width: usize,
    command: &str,
    matches: &[Suggestion],
    verbose: bool,
    color: bool,
) -> io::Result<()> {
    let index_width = matches.len().max(1).to_string().len();
    let score_width = usize::from(verbose) * 7;
    let column_count = if verbose { 4 } else { 3 };
    let available = width.saturating_sub(3 * column_count + 1);
    let name_and_path = available.saturating_sub(index_width + score_width);
    let natural_name_width = matches
        .iter()
        .map(|suggestion| visible_width(&sanitize(&suggestion.name)))
        .max()
        .unwrap_or(7)
        .max("Command".len());
    let max_name_width = name_and_path.saturating_sub(10).clamp(7, 24);
    let name_width = natural_name_width.clamp(7, max_name_width);
    let path_width = name_and_path.saturating_sub(name_width);

    let mut widths = vec![index_width, name_width, path_width];
    if verbose {
        widths.push(score_width);
    }

    print_rule(writer, '├', '┬', '┤', &widths)?;
    let mut headers = vec![
        ("#".to_string(), CellStyle::Header),
        ("Command".to_string(), CellStyle::Header),
        ("Location".to_string(), CellStyle::Header),
    ];
    if verbose {
        headers.push(("Match".to_string(), CellStyle::Header));
    }
    print_table_row(writer, &widths, &headers, color)?;
    print_rule(writer, '├', '┼', '┤', &widths)?;

    let command = sanitize(command);
    for (index, suggestion) in matches.iter().enumerate() {
        print_suggestion_row(writer, &widths, index, &command, suggestion, verbose, color)?;
    }
    print_rule(writer, '╰', '┴', '╯', &widths)
}

fn print_suggestion_row(
    writer: &mut impl Write,
    widths: &[usize],
    index: usize,
    command: &str,
    suggestion: &Suggestion,
    verbose: bool,
    color: bool,
) -> io::Result<()> {
    let name = truncate(&sanitize(&suggestion.name), widths[1]);
    let path = sanitize(&suggestion.path.to_string_lossy());
    let path = truncate_middle(&path, widths[2]);

    write!(writer, "│ ")?;
    write_cell(
        writer,
        &(index + 1).to_string(),
        widths[0],
        CellStyle::Suggestion,
        color,
    )?;
    write!(writer, " │ ")?;
    write_difference_cell(writer, command, &name, widths[1], color)?;
    write!(writer, " │ ")?;
    write_cell(writer, &path, widths[2], CellStyle::Dim, color)?;
    if verbose {
        write!(writer, " │ ")?;
        write_cell(
            writer,
            &format!("{:.1}%", suggestion.similarity * 100.0),
            widths[3],
            CellStyle::Normal,
            color,
        )?;
    }
    writeln!(writer, " │")
}

fn write_difference_cell(
    writer: &mut impl Write,
    command: &str,
    suggestion: &str,
    width: usize,
    color: bool,
) -> io::Result<()> {
    if color {
        write!(writer, "{}", highlight_differences(command, suggestion))?;
    } else {
        write!(writer, "{suggestion}")?;
    }
    write!(
        writer,
        "{}",
        " ".repeat(width.saturating_sub(visible_width(suggestion)))
    )
}

fn print_table_row(
    writer: &mut impl Write,
    widths: &[usize],
    cells: &[(String, CellStyle)],
    color: bool,
) -> io::Result<()> {
    write!(writer, "│")?;
    for ((text, style), width) in cells.iter().zip(widths) {
        write!(writer, " ")?;
        let text = truncate(text, *width);
        write_cell(writer, &text, *width, *style, color)?;
        write!(writer, " │")?;
    }
    writeln!(writer)
}

fn write_cell(
    writer: &mut impl Write,
    text: &str,
    width: usize,
    style: CellStyle,
    color: bool,
) -> io::Result<()> {
    let code = match style {
        CellStyle::Normal => "",
        CellStyle::Header => BOLD,
        CellStyle::Exact => GREEN,
        CellStyle::Suggestion => YELLOW,
        CellStyle::Dim => DIM,
    };
    write!(writer, "{}", styled(text, code, color && !code.is_empty()))?;
    write!(
        writer,
        "{}",
        " ".repeat(width.saturating_sub(visible_width(text)))
    )
}

fn print_rule(
    writer: &mut impl Write,
    left: char,
    junction: char,
    right: char,
    widths: &[usize],
) -> io::Result<()> {
    write!(writer, "{left}")?;
    for (index, width) in widths.iter().enumerate() {
        write!(writer, "{}", "─".repeat(width + 2))?;
        if index + 1 < widths.len() {
            write!(writer, "{junction}")?;
        }
    }
    writeln!(writer, "{right}")
}

fn print_full_bottom_border(writer: &mut impl Write, width: usize) -> io::Result<()> {
    writeln!(writer, "╰{}╯", "─".repeat(width.saturating_sub(2)))
}

fn pretty_width(results: &[LookupResult], verbose: bool) -> usize {
    let terminal_width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH);
    let mut preferred = MIN_WIDTH;

    for lookup in results {
        preferred = preferred.max(visible_width(&sanitize(&lookup.command)) + 8);
        match &lookup.result {
            SearchResult::Exact(paths) => {
                if let Some(path_width) = paths
                    .iter()
                    .map(|path| visible_width(&sanitize(&path.to_string_lossy())))
                    .max()
                {
                    preferred = preferred.max(path_width + 10);
                }
            }
            SearchResult::Suggestions {
                matches,
                candidate_count,
            } => {
                let name_width = matches
                    .iter()
                    .map(|suggestion| visible_width(&sanitize(&suggestion.name)))
                    .max()
                    .unwrap_or(7)
                    .min(24);
                let path_width = matches
                    .iter()
                    .map(|suggestion| visible_width(&sanitize(&suggestion.path.to_string_lossy())))
                    .max()
                    .unwrap_or(10);
                preferred = preferred.max(name_width + path_width + if verbose { 23 } else { 13 });
                if verbose {
                    preferred = preferred.max(candidate_count.to_string().len() + 42);
                }
            }
        }
    }

    preferred.min(terminal_width)
}

fn term_is_dumb() -> bool {
    env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"))
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                '�'
            } else {
                character
            }
        })
        .collect()
}

fn truncate(text: &str, width: usize) -> String {
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if graphemes.len() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return String::from("…");
    }
    format!("{}…", graphemes[..width - 1].concat())
}

fn truncate_middle(text: &str, width: usize) -> String {
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if graphemes.len() <= width {
        return text.to_owned();
    }
    if width < 3 {
        return truncate(text, width);
    }

    let left_width = (width - 1) / 2;
    let right_width = width - 1 - left_width;
    format!(
        "{}…{}",
        graphemes[..left_width].concat(),
        graphemes[graphemes.len() - right_width..].concat()
    )
}

fn highlight_differences(input: &str, suggestion: &str) -> String {
    let input: Vec<&str> = input.graphemes(true).collect();
    let suggestion: Vec<&str> = suggestion.graphemes(true).collect();
    let mut result = String::new();

    for (index, grapheme) in suggestion.iter().enumerate() {
        let color = if index >= input.len() {
            MAGENTA
        } else if input[index] == *grapheme {
            GREEN
        } else {
            RED
        };
        result.push_str(color);
        result.push_str(grapheme);
        result.push_str(RESET);
    }
    result
}

fn styled(text: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

fn visible_width(text: &str) -> usize {
    strip_ansi(text).graphemes(true).count()
}

fn pad(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(visible_width(text)))
    )
}

fn strip_ansi(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\x1b' && characters.peek() == Some(&'[') {
            characters.next();
            for code in characters.by_ref() {
                if code == 'm' {
                    break;
                }
            }
        } else {
            result.push(character);
        }
    }
    result
}

pub fn print_suggestions(
    writer: &mut impl Write,
    matches: &[Suggestion],
    command: &str,
    verbose: bool,
) -> io::Result<()> {
    if matches.is_empty() {
        return writeln!(
            writer,
            "\nCommand '{command}' not found and no close matches."
        );
    }

    writeln!(writer, "\nCommand '{command}' not found. Close matches:\n")?;
    print_plain_header(writer, verbose)?;
    for suggestion in matches {
        write!(
            writer,
            "{} | {}",
            pad(&suggestion.name, 20),
            pad(&suggestion.path.to_string_lossy(), 50)
        )?;
        if verbose {
            write!(
                writer,
                " | {}",
                pad(&format!("{:.2}", suggestion.similarity), 10)
            )?;
        }
        writeln!(writer)?;
    }
    Ok(())
}

fn print_plain_header(writer: &mut impl Write, verbose: bool) -> io::Result<()> {
    write!(
        writer,
        "{} | {}",
        pad("Suggested Command", 20),
        pad("Location", 50)
    )?;
    if verbose {
        write!(writer, " | {}", pad("Similarity", 10))?;
    }
    writeln!(writer)?;
    writeln!(
        writer,
        "{}",
        "-".repeat(20 + 50 + 3 + if verbose { 13 } else { 0 })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_ignores_color_codes() {
        let highlighted = format!("{GREEN}spell{RESET}check");
        assert_eq!(strip_ansi(&pad(&highlighted, 20)), "spellcheck          ");
    }

    #[test]
    fn difference_highlighting_marks_matching_changed_and_added_letters() {
        let highlighted = highlight_differences("spelcheck", "spellcheck");

        assert_eq!(strip_ansi(&highlighted), "spellcheck");
        assert_eq!(highlighted.matches(GREEN).count(), 4);
        assert_eq!(highlighted.matches(RED).count(), 5);
        assert_eq!(highlighted.matches(MAGENTA).count(), 1);
    }

    #[test]
    fn truncation_counts_grapheme_clusters() {
        assert_eq!(truncate("a👨‍👩‍👧‍👦bc", 3), "a👨‍👩‍👧‍👦…");
        assert_eq!(truncate_middle("abcdefghij", 7), "abc…hij");
    }
}
