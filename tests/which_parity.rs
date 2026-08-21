#![cfg(unix)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const FIRST_COMMAND: &str = "wizard-parity-first-command";
const SECOND_COMMAND: &str = "wizard-parity-second-command";
const MISSING_COMMAND: &str = "wizard-parity-command-that-does-not-exist";

struct Fixture {
    root: PathBuf,
    first_bin: PathBuf,
    second_bin: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wizard-which-parity-{name}-{}-{id}",
            std::process::id()
        ));
        let first_bin = root.join("first-bin");
        let second_bin = root.join("second-bin");
        fs::create_dir_all(&first_bin).unwrap();
        fs::create_dir_all(&second_bin).unwrap();
        Self {
            root,
            first_bin,
            second_bin,
        }
    }

    fn executable(&self, directory: &Path, name: &str) -> PathBuf {
        self.file(directory, name, 0o755)
    }

    fn non_executable(&self, directory: &Path, name: &str) -> PathBuf {
        self.file(directory, name, 0o644)
    }

    fn file(&self, directory: &Path, name: &str, mode: u32) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    fn path(&self) -> OsString {
        std::env::join_paths([&self.first_bin, &self.second_bin]).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn wizard() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_wizard"))
}

fn system_which() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH must be set to locate which");
    std::env::split_paths(&path)
        .map(|directory| directory.join("which"))
        .find(|candidate| {
            fs::metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .expect("these parity tests require an external `which` executable in PATH")
}

fn run(program: &Path, args: &[&str], path: &OsString, current_dir: &Path) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .env("PATH", path)
        .env("LC_ALL", "C")
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", program.display()))
}

fn run_os(program: &Path, args: &[&OsStr], path: &OsString, current_dir: &Path) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .env("PATH", path)
        .env("LC_ALL", "C")
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", program.display()))
}

fn run_with_home(
    program: &Path,
    args: &[&str],
    path: &OsString,
    current_dir: &Path,
    home: &Path,
) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .env("PATH", path)
        .env("HOME", home)
        .env("LC_ALL", "C")
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", program.display()))
}

fn run_without_path(program: &Path, args: &[&str], current_dir: &Path) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .env_remove("PATH")
        .env("LC_ALL", "C")
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", program.display()))
}

fn which_supports(option: &str) -> bool {
    let output = Command::new(system_which())
        .arg("--help")
        .env("LC_ALL", "C")
        .output()
        .expect("failed to inspect which --help");
    String::from_utf8_lossy(&output.stdout).contains(option)
        || String::from_utf8_lossy(&output.stderr).contains(option)
}

fn skip_unless_which_supports(option: &str) -> bool {
    if which_supports(option) {
        false
    } else {
        eprintln!("system which does not advertise {option}; parity case skipped");
        true
    }
}

fn assert_matches_which(args: &[&str], fixture: &Fixture) {
    let path = fixture.path();
    let reference = run(&system_which(), args, &path, &fixture.root);
    let actual = run(&wizard(), args, &path, &fixture.root);

    assert_eq!(
        actual.status.code(),
        reference.status.code(),
        "exit status differs\nwhich stderr: {}\nwizard stderr: {}",
        String::from_utf8_lossy(&reference.stderr),
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(
        actual.stdout,
        reference.stdout,
        "stdout differs\nwhich: {}\nwizard: {}",
        String::from_utf8_lossy(&reference.stdout),
        String::from_utf8_lossy(&actual.stdout)
    );
}

#[test]
fn first_path_match_has_which_parity() {
    let fixture = Fixture::new("first-match");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);

    assert_matches_which(&[FIRST_COMMAND], &fixture);
}

#[test]
fn non_executable_path_entries_have_which_parity() {
    let fixture = Fixture::new("non-executable");
    fixture.non_executable(&fixture.first_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);

    assert_matches_which(&[FIRST_COMMAND], &fixture);
}

#[test]
fn explicit_absolute_paths_have_which_parity() {
    let fixture = Fixture::new("absolute-path");
    let command = fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    let command = command.to_str().expect("temporary path should be UTF-8");

    assert_matches_which(&[command], &fixture);
}

#[test]
fn executable_symlinks_have_which_parity() {
    let fixture = Fixture::new("executable-symlink");
    let target = fixture.executable(&fixture.first_bin, "symlink-target");
    symlink(&target, fixture.first_bin.join(FIRST_COMMAND)).unwrap();

    assert_matches_which(&[FIRST_COMMAND], &fixture);
}

#[test]
fn invalid_earlier_entries_fall_through_with_which_parity() {
    let fixture = Fixture::new("invalid-entries");
    let non_executable = fixture.non_executable(&fixture.first_bin, "not-executable");
    symlink(&non_executable, fixture.first_bin.join(FIRST_COMMAND)).unwrap();
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);

    assert_matches_which(&[FIRST_COMMAND], &fixture);

    fs::remove_file(fixture.first_bin.join(FIRST_COMMAND)).unwrap();
    fs::create_dir(fixture.first_bin.join(FIRST_COMMAND)).unwrap();
    assert_matches_which(&[FIRST_COMMAND], &fixture);

    fs::remove_dir(fixture.first_bin.join(FIRST_COMMAND)).unwrap();
    symlink(
        fixture.root.join("missing-symlink-target"),
        fixture.first_bin.join(FIRST_COMMAND),
    )
    .unwrap();
    assert_matches_which(&[FIRST_COMMAND], &fixture);
}

#[test]
fn nonexistent_path_entries_have_which_parity() {
    let fixture = Fixture::new("nonexistent-path-entry");
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);
    let path =
        std::env::join_paths([fixture.root.join("missing"), fixture.second_bin.clone()]).unwrap();
    let reference = run(&system_which(), &[FIRST_COMMAND], &path, &fixture.root);
    let actual = run(&wizard(), &[FIRST_COMMAND], &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn spaces_and_unicode_in_command_names_have_which_parity() {
    let fixture = Fixture::new("unusual-names");
    for name in [
        "wizard parity command",
        "wizard\tparity-command",
        "wizard-parity-🧙",
    ] {
        fixture.executable(&fixture.first_bin, name);
        assert_matches_which(&[name], &fixture);
    }
}

#[test]
fn spaces_in_path_directories_have_which_parity() {
    let fixture = Fixture::new("path-spaces");
    let directory = fixture.root.join("bin with spaces");
    fs::create_dir(&directory).unwrap();
    fixture.executable(&directory, FIRST_COMMAND);
    let path = std::env::join_paths([directory]).unwrap();
    let reference = run(&system_which(), &[FIRST_COMMAND], &path, &fixture.root);
    let actual = run(&wizard(), &[FIRST_COMMAND], &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn symlinked_path_directories_have_which_parity() {
    let fixture = Fixture::new("symlinked-path");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    let linked_directory = fixture.root.join("linked-bin");
    symlink(&fixture.first_bin, &linked_directory).unwrap();
    let path = std::env::join_paths([linked_directory]).unwrap();
    let reference = run(&system_which(), &[FIRST_COMMAND], &path, &fixture.root);
    let actual = run(&wizard(), &[FIRST_COMMAND], &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn help_and_version_options_have_semantic_which_parity() {
    let fixture = Fixture::new("informational-options");
    for option in ["--help", "--version"] {
        let reference = run(&system_which(), &[option], &fixture.path(), &fixture.root);
        let actual = run(&wizard(), &[option], &fixture.path(), &fixture.root);
        if !reference.status.success() {
            eprintln!("system which does not support {option}; parity case skipped");
            continue;
        }

        assert!(actual.status.success(), "Wizard rejected {option}");
        assert!(
            !actual.stdout.is_empty(),
            "Wizard printed no {option} output"
        );
    }
}

#[test]
fn show_dot_has_which_parity_when_supported() {
    if skip_unless_which_supports("--show-dot") {
        return;
    }
    let fixture = Fixture::new("show-dot");
    fixture.executable(&fixture.root, FIRST_COMMAND);
    let path = OsString::from(".");
    let reference = run(
        &system_which(),
        &["--show-dot", FIRST_COMMAND],
        &path,
        &fixture.root,
    );
    let actual = run(
        &wizard(),
        &["--show-dot", FIRST_COMMAND],
        &path,
        &fixture.root,
    );

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn tty_only_non_tty_lookup_has_which_parity_when_supported() {
    if skip_unless_which_supports("--tty-only") {
        return;
    }
    let fixture = Fixture::new("tty-only");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);

    assert_matches_which(&["--tty-only", FIRST_COMMAND], &fixture);
}

// Exit codes for usage and option errors vary between `which` implementations, so those
// cases compare success/failure and output channels rather than vendor-specific numbers.

#[test]
fn no_command_operands_have_which_parity() {
    let fixture = Fixture::new("no-operands");
    let reference = run(&system_which(), &[], &fixture.path(), &fixture.root);
    let actual = run(&wizard(), &[], &fixture.path(), &fixture.root);

    assert_eq!(actual.status.success(), reference.status.success());
    assert_eq!(actual.stdout.is_empty(), reference.stdout.is_empty());
    assert_eq!(actual.stderr.is_empty(), reference.stderr.is_empty());
}

#[test]
fn unknown_options_have_which_parity() {
    let fixture = Fixture::new("unknown-option");
    let reference = run(
        &system_which(),
        &["--wizard-parity-unknown-option"],
        &fixture.path(),
        &fixture.root,
    );
    let actual = run(
        &wizard(),
        &["--wizard-parity-unknown-option"],
        &fixture.path(),
        &fixture.root,
    );

    assert_eq!(actual.status.success(), reference.status.success());
    assert_eq!(actual.stdout.is_empty(), reference.stdout.is_empty());
    assert_eq!(actual.stderr.is_empty(), reference.stderr.is_empty());
}

#[test]
fn tty_only_stops_later_options_when_not_a_tty() {
    if skip_unless_which_supports("--tty-only") || skip_unless_which_supports("--show-dot") {
        return;
    }
    let fixture = Fixture::new("tty-only-option-order");
    fixture.executable(&fixture.root, FIRST_COMMAND);
    let path = OsString::from(".");
    let args = ["--tty-only", "--show-dot", FIRST_COMMAND];
    let reference = run(&system_which(), &args, &path, &fixture.root);
    let actual = run(&wizard(), &args, &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn short_version_options_have_which_parity() {
    let fixture = Fixture::new("short-version");
    let reference_long = run(
        &system_which(),
        &["--version"],
        &fixture.path(),
        &fixture.root,
    );
    let actual_long = run(&wizard(), &["--version"], &fixture.path(), &fixture.root);

    for option in ["-v", "-V"] {
        let reference = run(&system_which(), &[option], &fixture.path(), &fixture.root);
        if !reference.status.success() || reference.stdout != reference_long.stdout {
            eprintln!("system which does not support {option}; parity case skipped");
            continue;
        }
        let actual = run(&wizard(), &[option], &fixture.path(), &fixture.root);
        assert_eq!(actual.status.code(), actual_long.status.code());
        assert_eq!(actual.stdout, actual_long.stdout);
    }
}

#[test]
fn option_terminator_has_which_parity() {
    let fixture = Fixture::new("option-terminator");
    let command = "--wizard-parity-command";
    fixture.executable(&fixture.first_bin, command);

    assert_matches_which(&["--", command], &fixture);
}

#[test]
fn options_after_command_operands_have_which_parity() {
    let fixture = Fixture::new("option-after-operand");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);
    let args = [FIRST_COMMAND, "--all"];
    let reference = run(&system_which(), &args, &fixture.path(), &fixture.root);
    if !reference.status.success()
        || reference
            .stdout
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            != 2
    {
        eprintln!("system which does not process --all after operands; parity case skipped");
        return;
    }
    let actual = run(&wizard(), &args, &fixture.path(), &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn effective_execute_permissions_have_which_parity() {
    let fixture = Fixture::new("effective-permissions");
    fixture.file(&fixture.first_bin, FIRST_COMMAND, 0o001);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);

    assert_matches_which(&[FIRST_COMMAND], &fixture);
}

#[test]
#[ignore = "known parity gap: Wizard's UTF-8 argument parser rejects non-UTF-8 command names"]
fn non_utf8_command_names_have_which_parity() {
    let fixture = Fixture::new("non-utf8-name");
    let name = OsString::from_vec(b"wizard-parity-\xff".to_vec());
    let executable = fixture.first_bin.join(&name);
    fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let reference = run_os(
        &system_which(),
        &[name.as_os_str()],
        &fixture.path(),
        &fixture.root,
    );
    let actual = run_os(
        &wizard(),
        &[name.as_os_str()],
        &fixture.path(),
        &fixture.root,
    );

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn explicit_relative_paths_have_which_parity() {
    let fixture = Fixture::new("relative-path");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    let command = format!("./first-bin/{FIRST_COMMAND}");

    assert_matches_which(&[&command], &fixture);
}

#[test]
fn relative_path_entries_have_which_parity() {
    let fixture = Fixture::new("relative-path-entry");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    let path = std::env::join_paths([PathBuf::from("first-bin")]).unwrap();
    let reference = run(&system_which(), &[FIRST_COMMAND], &path, &fixture.root);
    let actual = run(&wizard(), &[FIRST_COMMAND], &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn empty_path_entries_have_which_parity() {
    let fixture = Fixture::new("empty-path-entry");
    fixture.executable(&fixture.root, FIRST_COMMAND);
    let path = std::env::join_paths([PathBuf::new(), fixture.second_bin.clone()]).unwrap();
    let reference = run(&system_which(), &[FIRST_COMMAND], &path, &fixture.root);
    let actual = run(&wizard(), &[FIRST_COMMAND], &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
#[ignore = "intentional extension: Wizard explains when PATH is unset and no fuzzy match exists"]
fn unset_path_has_which_parity() {
    let fixture = Fixture::new("unset-path");
    let reference = run_without_path(&system_which(), &[MISSING_COMMAND], &fixture.root);
    let actual = run_without_path(&wizard(), &[MISSING_COMMAND], &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout.is_empty(), reference.stdout.is_empty());
    assert_eq!(actual.stderr.is_empty(), reference.stderr.is_empty());
}

#[test]
fn short_all_matches_have_which_parity() {
    let fixture = Fixture::new("all-matches");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);

    let reference = run(
        &system_which(),
        &["-a", FIRST_COMMAND],
        &fixture.path(),
        &fixture.root,
    );
    if !reference.status.success() {
        eprintln!("system which does not support -a; parity case skipped");
        return;
    }

    assert_matches_which(&["-a", FIRST_COMMAND], &fixture);
}

#[test]
fn long_all_matches_have_which_parity() {
    if skip_unless_which_supports("--all") {
        return;
    }
    let fixture = Fixture::new("long-all-matches");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);

    assert_matches_which(&["--all", FIRST_COMMAND], &fixture);
}

#[test]
fn all_matches_with_duplicate_path_entries_have_which_parity() {
    let fixture = Fixture::new("duplicate-path-all");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    let path = std::env::join_paths([
        fixture.first_bin.clone(),
        fixture.first_bin.clone(),
        fixture.second_bin.clone(),
    ])
    .unwrap();
    let reference = run(
        &system_which(),
        &["-a", FIRST_COMMAND],
        &path,
        &fixture.root,
    );
    if !reference.status.success() {
        eprintln!("system which does not support -a; parity case skipped");
        return;
    }
    let actual = run(&wizard(), &["-a", FIRST_COMMAND], &path, &fixture.root);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn multiple_command_operands_have_which_parity() {
    let fixture = Fixture::new("multiple-operands");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, SECOND_COMMAND);

    assert_matches_which(&[FIRST_COMMAND, SECOND_COMMAND], &fixture);
}

#[test]
#[ignore = "intentional extension: a close fuzzy match makes the missing operand actionable"]
fn mixed_found_and_missing_operands_have_which_parity() {
    let fixture = Fixture::new("mixed-operands");
    fixture.executable(&fixture.first_bin, FIRST_COMMAND);
    let reference = run(
        &system_which(),
        &[FIRST_COMMAND, MISSING_COMMAND],
        &fixture.path(),
        &fixture.root,
    );
    let actual = run(
        &wizard(),
        &[FIRST_COMMAND, MISSING_COMMAND],
        &fixture.path(),
        &fixture.root,
    );

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
    assert_eq!(actual.stderr.is_empty(), reference.stderr.is_empty());
}

#[test]
#[ignore = "intentional extension: Wizard prints a no-close-matches diagnostic"]
fn missing_command_exit_status_has_which_parity() {
    let fixture = Fixture::new("missing-command");

    let reference = run(
        &system_which(),
        &[MISSING_COMMAND],
        &fixture.path(),
        &fixture.root,
    );
    let actual = run(
        &wizard(),
        &[MISSING_COMMAND],
        &fixture.path(),
        &fixture.root,
    );

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout.is_empty(), reference.stdout.is_empty());
    assert_eq!(actual.stderr.is_empty(), reference.stderr.is_empty());
}

#[test]
fn skip_dot_has_which_parity() {
    if skip_unless_which_supports("--skip-dot") {
        return;
    }
    let fixture = Fixture::new("skip-dot");
    fixture.executable(&fixture.root, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);
    let path = std::env::join_paths([PathBuf::from("."), fixture.second_bin.clone()]).unwrap();
    let reference = run(
        &system_which(),
        &["--skip-dot", FIRST_COMMAND],
        &path,
        &fixture.root,
    );
    let actual = run(
        &wizard(),
        &["--skip-dot", FIRST_COMMAND],
        &path,
        &fixture.root,
    );

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn tilde_path_expansion_has_which_parity() {
    if skip_unless_which_supports("--show-tilde") {
        return;
    }
    let fixture = Fixture::new("tilde-expansion");
    let home = fixture.root.join("home");
    let home_bin = home.join("bin");
    fs::create_dir_all(&home_bin).unwrap();
    fixture.executable(&home_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);
    let path = std::env::join_paths([PathBuf::from("~/bin"), fixture.second_bin.clone()]).unwrap();
    let reference = run_with_home(
        &system_which(),
        &[FIRST_COMMAND],
        &path,
        &fixture.root,
        &home,
    );
    let actual = run_with_home(&wizard(), &[FIRST_COMMAND], &path, &fixture.root, &home);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn show_tilde_has_which_parity() {
    if skip_unless_which_supports("--show-tilde") {
        return;
    }
    let fixture = Fixture::new("show-tilde");
    let home = fixture.root.join("home");
    let home_bin = home.join("bin");
    fs::create_dir_all(&home_bin).unwrap();
    fixture.executable(&home_bin, FIRST_COMMAND);
    let path = std::env::join_paths([home_bin]).unwrap();
    let args = ["--show-tilde", FIRST_COMMAND];
    let reference = run_with_home(&system_which(), &args, &path, &fixture.root, &home);
    let actual = run_with_home(&wizard(), &args, &path, &fixture.root, &home);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}

#[test]
fn skip_tilde_has_which_parity() {
    if skip_unless_which_supports("--skip-tilde") {
        return;
    }
    let fixture = Fixture::new("skip-tilde");
    let home = fixture.root.join("home");
    let home_bin = home.join("bin");
    let literal_tilde_bin = fixture.root.join("~").join("bin");
    fs::create_dir_all(&home_bin).unwrap();
    fs::create_dir_all(&literal_tilde_bin).unwrap();
    fixture.executable(&home_bin, FIRST_COMMAND);
    fixture.executable(&literal_tilde_bin, FIRST_COMMAND);
    fixture.executable(&fixture.second_bin, FIRST_COMMAND);
    let path = std::env::join_paths([PathBuf::from("~/bin"), fixture.second_bin.clone()]).unwrap();
    let args = ["--skip-tilde", FIRST_COMMAND];
    let reference = run_with_home(&system_which(), &args, &path, &fixture.root, &home);
    let actual = run_with_home(&wizard(), &args, &path, &fixture.root, &home);

    assert_eq!(actual.status.code(), reference.status.code());
    assert_eq!(actual.stdout, reference.stdout);
}
