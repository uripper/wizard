use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rayon::prelude::*;

use crate::SearchOptions;
use crate::similarity::PreparedQuery;

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub name: String,
    pub path: PathBuf,
    pub similarity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchResult {
    Exact(Vec<PathBuf>),
    Suggestions {
        matches: Vec<Suggestion>,
        candidate_count: usize,
    },
}

pub fn path_directories() -> Vec<PathBuf> {
    let Some(path) = env::var_os("PATH") else {
        return Vec::new();
    };
    env::split_paths(&path).collect()
}

pub fn search(command: &str, options: &SearchOptions) -> SearchResult {
    search_in_paths(command, options, &path_directories())
}

pub fn search_in_paths(
    command: &str,
    options: &SearchOptions,
    directories: &[PathBuf],
) -> SearchResult {
    let exact = find_exact(command, directories, options);
    if !exact.is_empty() {
        return SearchResult::Exact(exact);
    }

    let ignored_directories: Vec<String> = options
        .ignored_directories
        .iter()
        .map(|pattern| pattern.to_uppercase())
        .collect();
    let per_directory: Vec<Vec<(String, PathBuf)>> = directories
        .par_iter()
        .map(|directory| {
            if should_skip_directory(directory, options) {
                return Vec::new();
            }
            let directory = resolved_directory(directory);
            if should_scan_fuzzy_directory(&directory, options.include_windows, running_in_wsl()) {
                collect_directory(&directory, &options.ignore_patterns, &ignored_directories)
            } else {
                Vec::new()
            }
        })
        .collect();

    // Rayon preserves indexed iterator order here, so inserting the first
    // executable for each name also preserves PATH precedence.
    let mut seen = HashSet::new();
    let candidates: Vec<(String, PathBuf)> = per_directory
        .into_iter()
        .flatten()
        .filter(|(name, _)| seen.insert(name.clone()))
        .collect();
    let candidate_count = candidates.len();
    let prepared = PreparedQuery::new(command, options.algorithm);
    let mut matches: Vec<Suggestion> = candidates
        .into_par_iter()
        .filter_map(|(name, path)| {
            prepared
                .score_candidate(&name, options.sensitivity, options.threshold)
                .map(|similarity| Suggestion {
                    name,
                    path,
                    similarity,
                })
        })
        .collect();

    matches.sort_by(|left, right| {
        right
            .similarity
            .total_cmp(&left.similarity)
            .then_with(|| left.name.cmp(&right.name))
    });
    matches.truncate(options.num_matches);
    SearchResult::Suggestions {
        matches,
        candidate_count,
    }
}

pub fn windows_mounts_excluded(options: &SearchOptions) -> bool {
    running_in_wsl() && !options.include_windows
}

fn should_scan_fuzzy_directory(directory: &Path, include_windows: bool, is_wsl: bool) -> bool {
    include_windows || !is_wsl || !matches!(directory_kind(directory), DirectoryKind::WslWindows)
}

fn running_in_wsl() -> bool {
    static IS_WSL: OnceLock<bool> = OnceLock::new();
    *IS_WSL.get_or_init(detect_wsl)
}

#[cfg(target_os = "linux")]
fn detect_wsl() -> bool {
    if env::var_os("WSL_INTEROP").is_some() || env::var_os("WSL_DISTRO_NAME").is_some() {
        return true;
    }

    ["/proc/sys/kernel/osrelease", "/proc/version"]
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .any(|release| release.to_ascii_lowercase().contains("microsoft"))
}

#[cfg(not(target_os = "linux"))]
fn detect_wsl() -> bool {
    false
}

fn find_exact(command: &str, directories: &[PathBuf], options: &SearchOptions) -> Vec<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return is_executable(command_path, DirectoryKind::Native)
            .then(|| printable_path(command_path, Some(command_path), options))
            .into_iter()
            .collect();
    }

    let mut matches = Vec::new();
    for directory in directories {
        if should_skip_directory(directory, options) {
            continue;
        }
        let printable_directory = printable_directory(directory);
        let resolved_directory = resolved_directory(directory);
        for candidate in exact_candidates(&resolved_directory, command) {
            if is_executable(&candidate, directory_kind(&resolved_directory)) {
                matches.push(printable_path(
                    &candidate,
                    Some(printable_directory),
                    options,
                ));
                if !options.all {
                    return matches;
                }
            }
        }
    }
    matches
}

fn printable_directory(directory: &Path) -> &Path {
    if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    }
}

fn should_skip_directory(directory: &Path, options: &SearchOptions) -> bool {
    let text = directory.to_string_lossy();
    (options.skip_dot && (text.is_empty() || text.starts_with('.')))
        || (options.skip_tilde && (text == "~" || text.starts_with("~/")))
}

fn resolved_directory(directory: &Path) -> PathBuf {
    let directory = printable_directory(directory);
    let mut components = directory.components();
    let Some(first) = components.next() else {
        return PathBuf::from(".");
    };
    if first.as_os_str() != "~" {
        return directory.to_owned();
    }
    let Some(home) = env::var_os("HOME") else {
        return directory.to_owned();
    };
    let mut expanded = PathBuf::from(home);
    expanded.extend(components);
    expanded
}

fn printable_path(path: &Path, source: Option<&Path>, options: &SearchOptions) -> PathBuf {
    if options.show_dot
        && source.is_some_and(|source| {
            let text = source.to_string_lossy();
            text == "." || text.starts_with("./")
        })
    {
        return path.to_owned();
    }

    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    if options.show_tilde
        && let Some(home) = env::var_os("HOME")
        && let Ok(relative) = absolute.strip_prefix(PathBuf::from(home))
    {
        return Path::new("~").join(relative);
    }
    absolute
}

#[cfg(not(windows))]
fn exact_candidates(directory: &Path, command: &str) -> Vec<PathBuf> {
    vec![directory.join(command)]
}

#[cfg(windows)]
fn exact_candidates(directory: &Path, command: &str) -> Vec<PathBuf> {
    let command = Path::new(command);
    if command.extension().is_some() {
        return vec![directory.join(command)];
    }
    windows_extensions()
        .into_iter()
        .map(|extension| {
            directory
                .join(command)
                .with_extension(extension.trim_start_matches('.'))
        })
        .collect()
}

fn collect_directory(
    directory: &Path,
    ignore_patterns: &[String],
    ignored_directories: &[String],
) -> Vec<(String, PathBuf)> {
    if ignored_directories
        .iter()
        .any(|pattern| directory.to_string_lossy().to_uppercase().contains(pattern))
    {
        return Vec::new();
    }

    let kind = directory_kind(directory);
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if ignore_patterns.iter().any(|pattern| name.contains(pattern))
                || !allowed_command_name(&name, kind)
            {
                return None;
            }
            let path = entry.path();
            is_executable(&path, kind).then_some((name, path))
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum DirectoryKind {
    Native,
    WslWindows,
}

#[cfg(windows)]
fn directory_kind(_: &Path) -> DirectoryKind {
    DirectoryKind::Native
}

#[cfg(not(windows))]
fn directory_kind(directory: &Path) -> DirectoryKind {
    let text = directory.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() >= 6
        && bytes[..5].eq_ignore_ascii_case(b"/mnt/")
        && bytes[5].is_ascii_alphabetic()
        && (bytes.len() == 6 || bytes[6] == b'/')
    {
        DirectoryKind::WslWindows
    } else {
        DirectoryKind::Native
    }
}

fn allowed_command_name(name: &str, kind: DirectoryKind) -> bool {
    #[cfg(windows)]
    {
        let extension = extension(name);
        return windows_extensions()
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(extension));
    }
    #[cfg(not(windows))]
    {
        match kind {
            DirectoryKind::Native => true,
            DirectoryKind::WslWindows => {
                let extension = extension(name);
                extension.is_empty()
                    || [
                        ".bat", ".bash", ".cmd", ".com", ".exe", ".fish", ".js", ".pl", ".py",
                        ".rb", ".sh", ".zsh",
                    ]
                    .contains(&extension.to_ascii_lowercase().as_str())
            }
        }
    }
}

fn extension(name: &str) -> &str {
    name.rfind('.')
        .filter(|index| *index > 0)
        .map_or("", |index| &name[index..])
}

#[cfg(windows)]
fn windows_extensions() -> Vec<String> {
    env::var("PATHEXT")
        .unwrap_or_else(|_| ".BAT;.CMD;.COM;.EXE".into())
        .split(';')
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

#[cfg(unix)]
fn is_executable(path: &Path, kind: DirectoryKind) -> bool {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn access(pathname: *const c_char, mode: c_int) -> c_int;
    }

    const X_OK: c_int = 1;

    if matches!(kind, DirectoryKind::WslWindows) {
        return path.is_file();
    }
    if !path.is_file() {
        return false;
    }
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is NUL-terminated and remains alive for the duration of the call.
    unsafe { access(path.as_ptr(), X_OK) == 0 }
}

#[cfg(windows)]
fn is_executable(path: &Path, _: DirectoryKind) -> bool {
    path.is_file()
}

#[cfg(not(any(unix, windows)))]
fn is_executable(path: &Path, _: DirectoryKind) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::Algorithm;

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("wizard-{name}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn child(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn create_file(directory: &Path, name: &str, executable: bool) -> PathBuf {
        let path = directory.join(name);
        fs::File::create(&path)
            .unwrap()
            .write_all(b"#!/bin/sh\n")
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
            )
            .unwrap();
        }
        path
    }

    #[test]
    fn finds_best_candidate() {
        let root = TestDirectory::new("best");
        let first = root.child("first");
        let second = root.child("second");
        let spellcheck = create_file(&first, "spellcheck", true);
        create_file(&second, "shellcheck", true);
        let options = SearchOptions {
            num_matches: 1,
            threshold: 0.8,
            ..SearchOptions::default()
        };

        let SearchResult::Suggestions { matches, .. } =
            search_in_paths("spelcheck", &options, &[first, second])
        else {
            panic!("expected suggestions");
        };
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "spellcheck");
        assert_eq!(matches[0].path, spellcheck);
    }

    #[test]
    fn skips_non_executables_and_preserves_path_precedence() {
        let root = TestDirectory::new("precedence");
        let first = root.child("first");
        let second = root.child("second");
        create_file(&first, "spelchek", false);
        let preferred = create_file(&first, "spellcheck", true);
        create_file(&second, "spellcheck", true);
        let options = SearchOptions {
            num_matches: 1,
            threshold: 0.8,
            ..SearchOptions::default()
        };

        let SearchResult::Suggestions { matches, .. } =
            search_in_paths("spelchek", &options, &[first, second])
        else {
            panic!("expected suggestions");
        };
        assert_eq!(matches[0].path, preferred);
    }

    #[test]
    fn falls_back_to_later_executable_with_same_name() {
        let root = TestDirectory::new("fallback");
        let first = root.child("first");
        let second = root.child("second");
        create_file(&first, "spellcheck", false);
        let executable = create_file(&second, "spellcheck", true);

        let SearchResult::Suggestions { matches, .. } = search_in_paths(
            "spelchek",
            &SearchOptions {
                threshold: 0.8,
                ..SearchOptions::default()
            },
            &[first, second],
        ) else {
            panic!("expected suggestions");
        };
        assert_eq!(matches[0].path, executable);
    }

    #[test]
    fn honors_filters() {
        let root = TestDirectory::new("filters");
        let bin = root.child("vendor-bin");
        create_file(&bin, "spellcheck.cmd", true);
        let options = SearchOptions {
            algorithm: Algorithm::JaroWinkler,
            ignore_patterns: vec![".cmd".into()],
            ignored_directories: vec![],
            threshold: 0.0,
            ..SearchOptions::default()
        };
        assert_eq!(
            search_in_paths("spelcheck", &options, &[bin]),
            SearchResult::Suggestions {
                matches: vec![],
                candidate_count: 0,
            }
        );
    }

    #[test]
    fn skips_windows_mounts_only_on_wsl_by_default() {
        let windows = Path::new("/mnt/c/Windows/System32");
        let native = Path::new("/usr/bin");

        assert!(!should_scan_fuzzy_directory(windows, false, true));
        assert!(should_scan_fuzzy_directory(windows, true, true));
        assert!(should_scan_fuzzy_directory(windows, false, false));
        assert!(should_scan_fuzzy_directory(native, false, true));
    }
}
