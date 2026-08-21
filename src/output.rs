use std::io::{self, Write};

use unicode_segmentation::UnicodeSegmentation;

use crate::search::Suggestion;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const MAGENTA: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

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
    print_header(writer, verbose)?;
    for suggestion in matches {
        let highlighted = highlight_differences(command, &suggestion.name);
        write!(
            writer,
            "{} | {}",
            pad(&highlighted, 20),
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

fn print_header(writer: &mut impl Write, verbose: bool) -> io::Result<()> {
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

fn pad(text: &str, width: usize) -> String {
    let visible = strip_ansi(text);
    let length = visible.graphemes(true).count();
    format!("{text}{}", " ".repeat(width.saturating_sub(length)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_ignores_color_codes() {
        let highlighted = highlight_differences("spelcheck", "spellcheck");
        assert_eq!(strip_ansi(&pad(&highlighted, 20)), "spellcheck          ");
    }
}
