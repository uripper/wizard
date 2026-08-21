#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("wizard-cli-{name}-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn executable(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl AsRef<Path> for TestDirectory {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wizard() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wizard"))
}

#[test]
fn exact_lookup_prints_the_first_executable() {
    let directory = TestDirectory::new("exact");
    let executable = directory.executable("spellcheck");
    let output = wizard()
        .arg("spellcheck")
        .env("PATH", directory.as_ref())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        executable.to_string_lossy()
    );
}

#[test]
fn fuzzy_lookup_prints_the_closest_command() {
    let directory = TestDirectory::new("fuzzy");
    directory.executable("spellcheck");
    directory.executable("shellcheck");
    let output = wizard()
        .args(["spelcheck", "--matches=1", "--threshold=.8"])
        .env("PATH", directory.as_ref())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("spellcheck"));
    assert!(!stdout.contains("shellcheck"));
}

#[test]
fn missing_command_without_close_matches_prints_the_no_match_message() {
    let directory = TestDirectory::new("missing");
    let output = wizard()
        .arg("wizard-parity-definitely-missing")
        .env("PATH", directory.as_ref())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "Command 'wizard-parity-definitely-missing' not found and no close matches."
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn reports_help_version_and_invalid_options() {
    let help = wizard().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Usage: wizard")
    );

    let version = wizard().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.1.4");

    let invalid = wizard().arg("--matches=0").output().unwrap();
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("integer ≥ 1")
    );

    let unrecognized = wizard().arg("--unknown").output().unwrap();
    assert!(!unrecognized.status.success());
    assert_eq!(
        String::from_utf8(unrecognized.stderr).unwrap().trim(),
        "Unrecognized option '--unknown'"
    );
}

#[test]
fn forced_pretty_groups_and_frames_mixed_query_results() {
    let directory = TestDirectory::new("pretty-mixed");
    let spellcheck = directory.executable("spellcheck");
    let output = wizard()
        .args([
            "--format=pretty",
            "--matches=1",
            "--threshold=.8",
            "spellcheck",
            "spelcheck",
            "zzzzzzzzzzzz",
        ])
        .env("PATH", directory.as_ref())
        .env("COLUMNS", "120")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "the missing query must fail the batch"
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();

    let exact = stdout
        .find("╭─ ✓ spellcheck")
        .expect("missing exact-match card");
    let suggested = stdout
        .find("╭─ ≈ spelcheck")
        .expect("missing suggestion card");
    let missing = stdout
        .find("╭─ × zzzzzzzzzzzz")
        .expect("missing not-found card");
    assert!(exact < suggested && suggested < missing);

    assert_eq!(stdout.matches("╭─").count(), 3);
    assert_eq!(stdout.matches('╰').count(), 3);
    assert!(stdout.contains("1 exact match"));
    assert!(stdout.contains("No exact match · 1 suggestion"));
    assert!(stdout.contains("No exact match · no close suggestions"));
    assert!(stdout.matches("│ 1 │").count() >= 2);
    assert!(stdout.contains(&spellcheck.to_string_lossy().into_owned()));
    assert!(stdout.contains("\n\n╭─ ≈ spelcheck"));
    assert!(stdout.contains("\n\n╭─ × zzzzzzzzzzzz"));
}

#[test]
fn forced_plain_keeps_exact_results_path_only() {
    let directory = TestDirectory::new("plain-exact");
    let first = directory.executable("first-command");
    let second = directory.executable("second-command");
    let output = wizard()
        .args(["--format=plain", "first-command", "second-command"])
        .env("PATH", directory.as_ref())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}\n{}\n", first.display(), second.display())
    );
}

#[test]
fn no_color_suppresses_ansi_in_forced_pretty_output() {
    let directory = TestDirectory::new("pretty-no-color");
    directory.executable("spellcheck");
    let output = wizard()
        .args([
            "--format=pretty",
            "--matches=1",
            "--threshold=.8",
            "spelcheck",
        ])
        .env("PATH", directory.as_ref())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("COLUMNS", "120")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("╭─ ≈ spelcheck"));
    assert!(!stdout.contains("\x1b["));
}

#[test]
fn pretty_output_fits_and_truncates_to_the_reported_terminal_width() {
    let directory = TestDirectory::new("pretty-narrow");
    directory.executable("a-very-long-command-name-for-output");
    let output = wizard()
        .args(["--format=pretty", "a-very-long-command-name-for-output"])
        .env("PATH", directory.as_ref())
        .env("COLUMNS", "40")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains('…'));
    assert!(
        stdout.lines().all(|line| line.chars().count() == 40),
        "every card line should fit the terminal width:\n{stdout}"
    );
}

#[test]
fn invalid_output_format_fails() {
    let output = wizard().arg("--format=ornate").output().unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "Invalid format 'ornate'; expected auto, pretty, or plain"
    );
}
