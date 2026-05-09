//! Git CLI wrapper for local repository operations.
//!
//! Replaces the `git2` C-binding crate with `std::process::Command` calls
//! to the `git` binary. Since this crate is used in `build.rs`, the `git`
//! command is guaranteed to be available.
//!
//! All methods execute `git -C <work_dir> <args>` and parse the output.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{VersionError, VResult};

/// A local Git repository accessed via the `git` CLI.
///
/// Created by [`GitRepo::discover`], which locates the repository root
/// using `git rev-parse --show-toplevel`.
pub(crate) struct GitRepo {
    work_dir: PathBuf,
}

impl GitRepo {
    /// Discovers the Git repository containing `path`.
    ///
    /// Uses `git -C <path> rev-parse --show-toplevel` to find the working
    /// directory. Supports both regular repos and submodules.
    pub fn discover(path: &Path) -> VResult<Self> {
        let output = run_git(&[
            OsStr::new("-C"),
            path.as_os_str(),
            OsStr::new("rev-parse"),
            OsStr::new("--show-toplevel"),
        ])?;
        Ok(Self {
            work_dir: PathBuf::from(output),
        })
    }

    /// Returns `true` if the repository is bare (no working tree).
    pub fn is_bare(&self) -> VResult<bool> {
        let output = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("rev-parse"),
            OsStr::new("--is-bare-repository"),
        ])?;
        Ok(output == "true")
    }

    /// Returns the current branch name, or `None` for detached HEAD / unborn branch.
    #[allow(clippy::unnecessary_wraps)]
    pub fn branch(&self) -> VResult<Option<String>> {
        let Ok(output) = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("rev-parse"),
            OsStr::new("--abbrev-ref"),
            OsStr::new("HEAD"),
        ]) else {
            return Ok(None);
        };
        if output == "HEAD" || output.starts_with("HEAD detached") {
            Ok(None)
        } else {
            Ok(Some(output))
        }
    }

    /// Returns the abbreviated commit hash of HEAD (typically 7-12 hex chars).
    pub fn head_short_id(&self) -> VResult<String> {
        run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("rev-parse"),
            OsStr::new("--short"),
            OsStr::new("HEAD"),
        ])
    }

    /// Returns `(unix_timestamp_seconds, offset_minutes)`.
    pub fn head_commit_time(&self) -> VResult<(i64, i32)> {
        let ts_str = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("log"),
            OsStr::new("-1"),
            OsStr::new("--format=%ct"),
        ])?;
        let timestamp: i64 = ts_str.parse()?;

        let offset_str = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("log"),
            OsStr::new("-1"),
            OsStr::new("--format=%z"),
        ])?;
        let offset_minutes = parse_tz_offset(&offset_str);

        Ok((timestamp, offset_minutes))
    }

    /// Counts total modified lines (insertions + deletions) in both staged
    /// and unstaged changes.
    pub fn modified_lines(&self) -> VResult<usize> {
        let unstaged = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("diff"),
            OsStr::new("--numstat"),
        ])?;
        let staged = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("diff"),
            OsStr::new("--cached"),
            OsStr::new("--numstat"),
        ])?;

        let total = parse_numstat(&unstaged) + parse_numstat(&staged);
        Ok(total)
    }

    /// Reads a blob from the repository using `git show <spec>`.
    ///
    /// `spec` uses Git revision syntax, e.g., `"HEAD:Cargo.toml"`.
    pub fn blob_content(&self, spec: &str) -> VResult<Vec<u8>> {
        let output = Command::new("git")
            .args([
                OsStr::new("-C"),
                self.work_dir.as_os_str(),
                OsStr::new("show"),
                OsStr::new(spec),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VersionError::GitCommand(format!(
                "git show {spec}: {stderr}"
            )));
        }

        Ok(output.stdout)
    }

    /// Returns the full commit hash that last modified `line_num` of `file_path`.
    ///
    /// Uses `git blame --porcelain -L N,N HEAD -- <file>`.
    pub fn blame_line_commit(&self, file_path: &Path, line_num: usize) -> VResult<String> {
        let line_range = format!("{line_num},{line_num}");
        let file_str = file_path.to_string_lossy();
        let output = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("blame"),
            OsStr::new("--porcelain"),
            OsStr::new("-L"),
            OsStr::new(&line_range),
            OsStr::new("HEAD"),
            OsStr::new("--"),
            OsStr::new(file_str.as_ref()),
        ])?;

        // --porcelain: first line is "<40-hex-hash> <metadata>"
        let hash = output
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().next())
            .ok_or_else(|| VersionError::GitCommand("blame output empty".into()))?;

        Ok(hash.to_string())
    }

    /// Counts commits from `exclude_commit` (exclusive) to HEAD.
    ///
    /// Equivalent to `git rev-list --count <exclude>..HEAD`.
    pub fn revwalk_count_since(&self, exclude_commit: &str) -> VResult<usize> {
        let range = format!("{exclude_commit}..HEAD");
        let output = run_git(&[
            OsStr::new("-C"),
            self.work_dir.as_os_str(),
            OsStr::new("rev-list"),
            OsStr::new("--count"),
            OsStr::new(&range),
        ])?;
        Ok(output.parse()?)
    }

    /// Returns the repository working directory (toplevel).
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }
}

/// Executes `git` with the given arguments and returns trimmed stdout.
///
/// Returns `VersionError::GitCommand` on non-zero exit code.
fn run_git(args: &[&OsStr]) -> VResult<String> {
    let output = match Command::new("git").args(args).output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(VersionError::GitCommand(
                "git command not found: ensure git is installed and on PATH".into(),
            ));
        }
        Err(e) => return Err(VersionError::from(e)),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VersionError::GitCommand(stderr.trim().to_string()));
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout.trim_end().to_string())
}

/// Parses a git timezone offset string (e.g., `"+0800"`, `"-0500"`) into minutes.
///
/// Returns `0` for empty strings or invalid formats.
fn parse_tz_offset(input: &str) -> i32 {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let sign: i32 = if trimmed.starts_with('+') {
        1
    } else if trimmed.starts_with('-') {
        -1
    } else {
        return 0;
    };

    let digits = &trimmed[1..];
    if digits.len() < 4 {
        return 0;
    }

    let hours: i32 = digits[..2].parse().unwrap_or(0);
    let minutes: i32 = digits[2..4].parse().unwrap_or(0);

    sign * (hours * 60 + minutes)
}

/// Parses `git diff --numstat` output into a total line count.
///
/// Binary files (shown as `-\t-\t<file>`) are skipped.
fn parse_numstat(output: &str) -> usize {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let added = parts.next()?;
            let deleted = parts.next()?;
            if added == "-" || deleted == "-" {
                return None;
            }
            Some(added.parse::<usize>().ok()? + deleted.parse::<usize>().ok()?)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::GitRepo;
    use std::path::Path;

    // ---- parse_tz_offset ----

    #[test]
    fn test_parse_tz_offset_positive() {
        assert_eq!(super::parse_tz_offset("+0800"), 480);
    }

    #[test]
    fn test_parse_tz_offset_negative() {
        assert_eq!(super::parse_tz_offset("-0500"), -300);
    }

    #[test]
    fn test_parse_tz_offset_zero() {
        assert_eq!(super::parse_tz_offset("+0000"), 0);
    }

    #[test]
    fn test_parse_tz_offset_empty() {
        assert_eq!(super::parse_tz_offset(""), 0);
    }

    #[test]
    fn test_parse_tz_offset_short() {
        assert_eq!(super::parse_tz_offset("+00"), 0);
    }

    // ---- parse_numstat ----

    #[test]
    fn test_parse_numstat_normal() {
        let input = "3\t2\tfile.rs\n1\t0\tother.rs";
        assert_eq!(super::parse_numstat(input), 6);
    }

    #[test]
    fn test_parse_numstat_binary() {
        let input = "-\t-\tbinary.png\n1\t1\tfile.rs";
        assert_eq!(super::parse_numstat(input), 2);
    }

    #[test]
    fn test_parse_numstat_empty() {
        assert_eq!(super::parse_numstat(""), 0);
    }

    // ---- GitRepo integration tests ----

    fn open_repo() -> GitRepo {
        GitRepo::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("failed to discover git repo")
    }

    #[test]
    fn test_discover_current_repo() {
        let repo = open_repo();
        let work_dir = repo.work_dir();
        assert!(work_dir.is_dir(), "work_dir should be a valid directory");
    }

    #[test]
    fn test_is_bare() {
        let repo = open_repo();
        assert!(!repo.is_bare().expect("is_bare failed"));
    }

    #[test]
    fn test_branch() {
        let repo = open_repo();
        let branch = repo.branch().expect("branch failed");
        assert!(branch.is_some(), "repo should have a branch");
    }

    #[test]
    fn test_head_short_id() {
        let repo = open_repo();
        let id = repo.head_short_id().expect("head_short_id failed");
        assert!(id.len() >= 7, "short id should be at least 7 chars, got: {id}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "short id should be hex: {id}");
    }

    #[test]
    fn test_head_commit_time() {
        let repo = open_repo();
        let (timestamp, offset) = repo.head_commit_time().expect("head_commit_time failed");
        assert!(timestamp > 0, "timestamp should be positive, got: {timestamp}");
        assert!(
            (-720..=840).contains(&offset),
            "offset should be within [-720, 840] minutes, got: {offset}"
        );
    }

    #[test]
    fn test_modified_lines() {
        let repo = open_repo();
        let lines = repo.modified_lines().expect("modified_lines failed");
        assert!(lines < usize::MAX);
    }

    #[test]
    fn test_blob_content_cargo_toml() {
        let repo = open_repo();
        let bytes = repo.blob_content("HEAD:Cargo.toml").expect("blob_content failed");
        let content = String::from_utf8(bytes).expect("Cargo.toml should be utf8");
        assert!(
            content.contains("[package]"),
            "Cargo.toml should contain [package]"
        );
    }

    #[test]
    fn test_blame_line_commit() {
        let repo = open_repo();
        let hash = repo
            .blame_line_commit(Path::new("Cargo.toml"), 1)
            .expect("blame_line_commit failed");
        assert_eq!(hash.len(), 40, "full commit hash should be 40 chars, got: {hash}");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be hex: {hash}"
        );
    }

    #[test]
    fn test_revwalk_count_since() {
        let repo = open_repo();
        let full_hash = super::run_git(&[
            std::ffi::OsStr::new("-C"),
            repo.work_dir().as_os_str(),
            std::ffi::OsStr::new("rev-parse"),
            std::ffi::OsStr::new("HEAD"),
        ])
        .expect("rev-parse HEAD failed");
        let count = repo
            .revwalk_count_since(&full_hash)
            .expect("revwalk_count_since failed");
        assert_eq!(count, 0, "rev-list --count HEAD..HEAD should be 0, got: {count}");
    }
}
