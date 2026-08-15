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
}
