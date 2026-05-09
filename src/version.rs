//! Version generation: merges `Cargo.toml` version with Git metadata.
//!
//! This module provides [`Version`] which collects version info from both
//! `Cargo.toml` and the Git repository, and outputs Rust source files with
//! compile-time version constants.

use crate::compiler::CompilerEnv;
use crate::error::{VResult, VersionError};
use crate::git::GitRepo;
use crate::ts::{now_timestamp, DateTime, Format};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Parses a `version = "X.Y.Z"` line from `Cargo.toml`.
///
/// Returns `Some((major, minor, patch))` on success, `None` if the line
/// doesn't match or has invalid format.
fn parse_version_line(line: &str) -> Option<(u32, u32, u32)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("version") {
        return None;
    }
    let eq_pos = trimmed.find('=')?;
    let value = trimmed[eq_pos + 1..].trim();
    let value = value.strip_prefix('"').unwrap_or(value);
    let value = value.strip_suffix('"').unwrap_or(value);
    let mut parts = value.splitn(3, '.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next()?.parse::<u32>().ok()?;
    Some((major, minor, patch))
}

/// Returns the default output path (`$OUT_DIR/version.rs`).
///
/// Falls back to `./version.rs` if `OUT_DIR` is not set.
#[must_use]
pub fn out_file() -> PathBuf {
    PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".to_string()))
        .join("version.rs")
}

/// Version metadata collected from `Cargo.toml` and Git.
///
/// Construct via [`Version::new`] or [`Version::new_for`], then use the
/// fluent API (`write_version`, `set_output_env`, `write_buildlog`) to
/// generate output.
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
    build_id: Option<String>,
    branch: String,
    commit: String,
    commit_ts: DateTime,
    modified: usize,
    build_ts: DateTime,
}

impl Version {
    /// Major version from `Cargo.toml`.
    #[must_use]
    pub fn major(&self) -> u32 {
        self.major
    }

    /// Minor version from `Cargo.toml`.
    #[must_use]
    pub fn minor(&self) -> u32 {
        self.minor
    }

    /// Patch version. If `0` in `Cargo.toml`, auto-calculated from commit count.
    #[must_use]
    pub fn patch(&self) -> u32 {
        self.patch
    }

    /// Optional build identifier, e.g. `"beta1"`.
    #[must_use]
    pub fn build_id(&self) -> Option<&str> {
        self.build_id.as_deref()
    }

    /// Current git branch name (empty string if detached HEAD).
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Short git commit hash.
    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// The commit's author timestamp (converted to local timezone).
    #[must_use]
    pub fn commit_ts(&self) -> &DateTime {
        &self.commit_ts
    }

    /// Total staged + unstaged modified lines.
    #[must_use]
    pub fn modified(&self) -> usize {
        self.modified
    }

    /// Timestamp when this `Version` was constructed.
    #[must_use]
    pub fn build_ts(&self) -> &DateTime {
        &self.build_ts
    }
}

impl Version {
    /// Creates a `Version` by reading `Cargo.toml` and Git state from `path`.
    ///
    /// `path` is typically `CARGO_MANIFEST_DIR`. The version is read from
    /// `Cargo.toml`, and git metadata (branch, commit, modified lines) is
    /// collected from the repository containing `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if `Cargo.toml` cannot be read, the git repository
    /// cannot be discovered, or git commands fail.
    pub fn new<P: AsRef<Path>>(path: P) -> VResult<Version> {
        Self::new_for(path, None)
    }

    /// Creates a `Version` with an optional build identifier.
    ///
    /// The `build_id` appears in the version string (e.g., `"1.0.0.beta1"`)
    /// and as a `BUILD_ID` constant if present.
    ///
    /// # Errors
    ///
    /// Returns an error if `Cargo.toml` cannot be read, the git repository
    /// cannot be discovered, or git commands fail.
    pub fn new_for<P: AsRef<Path>>(path: P, build_id: Option<String>) -> VResult<Version> {
        let git = GitRepo::discover(path.as_ref())?;
        if git.is_bare()? {
            return Err(VersionError::from(
                "cannot report status on bare repository",
            ));
        }

        let modified = git.modified_lines()?;
        let branch = git.branch()?.unwrap_or_default();
        let commit = git.head_short_id()?;
        let (timestamp_secs, offset_minutes) = git.head_commit_time()?;
        let commit_ts = DateTime::timestamp_to_local(timestamp_secs, offset_minutes);

        let work_dir = git.work_dir();
        let canonical_work = work_dir.canonicalize().unwrap_or_else(|_| work_dir.to_path_buf());
        let canonical_input = path.as_ref().canonicalize().unwrap_or_else(|_| path.as_ref().to_path_buf());
        let relative_path = canonical_input.strip_prefix(&canonical_work)?.to_path_buf();
        let mut search_path = relative_path.clone();

        loop {
            if let Ok((major, minor, patch)) = Self::generate_version(&git, &search_path) {
                return Ok(Version {
                    major,
                    minor,
                    patch,
                    build_id,
                    branch,
                    commit,
                    commit_ts,
                    modified,
                    build_ts: now_timestamp(),
                });
            }

            if !search_path.pop() {
                break;
            }
        }

        Err(VersionError::from("Not Found"))
    }

    /// Guards against accidental release builds with uncommitted changes.
    ///
    /// In release mode (`cfg(not(debug_assertions))`), panics if any files
    /// are modified. In debug mode, returns `self` unchanged.
    #[must_use]
    pub fn modified_cannot_build_release(self) -> Self {
        if self.modified > 0 {
            #[cfg(not(debug_assertions))]
            panic!("Code modified cannot build release!");
        }
        self
    }

    /// Build identifier suffix (e.g., `".beta1"`), empty if none.
    fn build_id_suffix(&self) -> String {
        self.build_id
            .as_ref()
            .map(|id| format!(".{id}"))
            .unwrap_or_default()
    }

    /// Full version string (e.g., `"0.5.3.beta1-D/M12"`).
    fn version_string(&self) -> String {
        format!(
            "{}.{}.{}{}{}",
            self.major,
            self.minor,
            self.patch,
            self.build_id_suffix(),
            self.modified_suffix(),
        )
    }

    /// Writes version constants to a Rust source file.
    ///
    /// The generated file contains `VERSION`, `VERSION_MAJOR/MINOR/PATCH`,
    /// `SOURCES_FINGERPRINT`, and `BUILD_FINGERPRINT` constants. Include it
    /// with `include!(concat!(env!("OUT_DIR"), "/version.rs"))`.
    ///
    /// # Errors
    ///
    /// Returns an error if the output file cannot be created or written to,
    /// or if the compiler environment cannot be detected.
    pub fn write_version<P: AsRef<Path>>(self, path: P) -> VResult<Self> {
        let mut file = File::create(path).map_err(VersionError::from)?;

        writeln!(file, "#[allow(dead_code)]")?;
        writeln!(file)?;
        writeln!(
            file,
            "pub const VERSION: &str = \"{}\";",
            self.version_string(),
        )?;
        writeln!(file, "pub const VERSION_MAJOR: u32 = {};", self.major)?;
        writeln!(file, "pub const VERSION_MINOR: u32 = {};", self.minor)?;
        writeln!(file, "pub const VERSION_PATCH: u32 = {};", self.patch)?;
        if let Some(build_id) = self.build_id.as_ref() {
            writeln!(file, "pub const BUILD_ID: &str = \"{build_id}\";")?;
        }

        writeln!(
            file,
            "pub const SOURCES_FINGERPRINT: &str = \"v{} {}-{} {}\";",
            self.version_string(),
            self.branch,
            self.commit,
            self.commit_ts.to_rfc3339(),
        )?;

        let compiler = CompilerEnv::new().unwrap_or_default();
        writeln!(
            file,
            "pub const BUILD_FINGERPRINT: &str = \"{} {} [{}, {}, {}]\";",
            self.build_ts.to_rfc3339(),
            compiler.build_type,
            compiler.rust_toolchain,
            compiler.rust_version,
            compiler.cargo_version,
        )?;

        file.flush()?;
        Ok(self)
    }

    /// Exports the version string to a shell environment variable.
    ///
    /// On Windows, uses `setx`. On macOS/Linux, writes an `.env` file and
    /// prints a `cargo:warning` with the source command.
    ///
    /// # Errors
    ///
    /// Returns an error if the `setx` command fails (Windows) or if the
    /// `.env` file cannot be written.
    pub fn set_output_env(self, name: &str) -> VResult<Self> {
        let value = self.version_string();

        #[cfg(target_os = "windows")]
        std::process::Command::new("setx")
            .args([name, &value])
            .status()
            .map_err(VersionError::from)?;

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let env_file = std::env::current_dir()?.join(format!(".{name}.env"));
            let env_line = format!("export {name}={value}");
            let prefix = format!("export {name}=");
            let contents = if let Ok(prev) = std::fs::read_to_string(&env_file) {
                let mut replaced = false;
                let lines: Vec<String> = prev
                    .lines()
                    .map(|line| {
                        if line.starts_with(&prefix) {
                            replaced = true;
                            env_line.clone()
                        } else {
                            line.to_string()
                        }
                    })
                    .collect();
                let mut out = lines.join("\n");
                if !replaced {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&env_line);
                }
                out
            } else {
                env_line
            };

            std::fs::write(&env_file, contents).map_err(VersionError::from)?;
            println!("cargo:warning=exec \"source {}\"", env_file.display());
        }

        Ok(self)
    }

    /// Appends a Markdown table row to a build log file.
    ///
    /// Each row contains build time, build type, version, branch, commit,
    /// and compiler info. Use with `cargo:rerun-if-changed=<path>` to avoid
    /// unnecessary rebuilds.
    ///
    /// # Errors
    ///
    /// Returns an error if the log file cannot be opened or written to,
    /// or if the compiler environment cannot be detected.
    pub fn write_buildlog<P: AsRef<Path>>(self, path: P) -> VResult<Self> {
        let compiler = CompilerEnv::new().unwrap_or_default();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(VersionError::from)?;

        writeln!(
            file,
            "| {} | {} | v{} | {}-{} | {} | {} | {} | {}|",
            self.build_ts.to_rfc3339(),
            compiler.build_type,
            self.version_string(),
            self.branch,
            self.commit,
            self.commit_ts.to_rfc3339(),
            compiler.rust_toolchain,
            compiler.rust_version,
            compiler.cargo_version,
        )?;
        file.flush()?;
        Ok(self)
    }

    /// Reads `Cargo.toml` blob from git, finds the version line, and auto-calculates patch if `0`.
    fn generate_version(git: &GitRepo, path: &Path) -> VResult<(u32, u32, u32)> {
        let cargo_path = path.join("Cargo.toml");
        let spec = format!("HEAD:{}", cargo_path.display());
        let blob = git.blob_content(&spec)?;
        let reader = BufReader::new(&blob[..]);

        let full_path = git.work_dir().join(&cargo_path);

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if let Some((major, minor, mut patch)) = parse_version_line(&line) {
                let line_num = i + 1;
                let commit_id = git.blame_line_commit(&full_path, line_num)?;
                let count = git.revwalk_count_since(&commit_id)?;

                if patch == 0 {
                    patch = u32::try_from(count).unwrap_or(u32::MAX);
                }
                return Ok((major, minor, patch));
            }
        }

        Err(VersionError::from("Not found"))
    }

    /// Modification suffix: `-D` in debug, `-M{N}` in release, `-D/M{N}` if modified.
    fn modified_suffix(&self) -> String {
        #[cfg(not(debug_assertions))]
        {
            if self.modified > 0 {
                format!("-M{}", self.modified)
            } else {
                String::new()
            }
        }
        #[cfg(debug_assertions)]
        {
            if self.modified > 0 {
                format!("-D/M{}", self.modified)
            } else {
                "-D".to_string()
            }
        }
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Version");
        f.field("major", &self.major)
            .field("minor", &self.minor)
            .field("patch", &self.patch)
            .field("branch", &self.branch)
            .field("commit", &self.commit)
            .field("commit_ts", &self.commit_ts.human_format())
            .field("modified", &self.modified)
            .field("build_ts", &self.build_ts.human_format());
        if let Some(build_id) = self.build_id.as_ref() {
            f.field("build_id", build_id);
        }
        f.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_new_and_write() {
        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new(path);
        assert!(
            version.as_ref().is_ok(),
            "Version::new should succeed in repo: {version:?}",
        );
        version
            .unwrap()
            .modified_cannot_build_release()
            .write_version(out_file())
            .expect("write_version");
    }

    // --- parse_version_line tests ---

    #[test]
    fn test_parse_version_line_standard() {
        assert_eq!(parse_version_line(r#"version = "1.2.3""#), Some((1, 2, 3)));
    }

    #[test]
    fn test_parse_version_line_spaced() {
        assert_eq!(parse_version_line(r#"version="4.5.6""#), Some((4, 5, 6)));
    }

    #[test]
    fn test_parse_version_line_spaces_around_equals() {
        assert_eq!(
            parse_version_line(r#"version   =   "7.8.9""#),
            Some((7, 8, 9))
        );
    }

    #[test]
    fn test_parse_version_line_leading_whitespace() {
        assert_eq!(
            parse_version_line(r#"  version = "1.0.0""#),
            Some((1, 0, 0))
        );
    }

    #[test]
    fn test_parse_version_line_no_match() {
        assert_eq!(parse_version_line(r#"name = "my-crate""#), None);
    }

    #[test]
    fn test_parse_version_line_too_few_parts() {
        assert_eq!(parse_version_line(r#"version = "1.2""#), None);
    }

    #[test]
    fn test_parse_version_line_non_numeric() {
        assert_eq!(parse_version_line(r#"version = "a.b.c""#), None);
    }

    #[test]
    fn test_parse_version_line_empty() {
        assert_eq!(parse_version_line(""), None);
    }

    // --- Version::new / new_for tests ---

    #[test]
    fn test_version_new_with_build_id() {
        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new_for(path, Some("beta1".to_string()));
        assert!(version.is_ok(), "Version::new_for should succeed: {version:?}");
        let v = version.unwrap();
        assert_eq!(v.build_id(), Some("beta1"));
    }

    #[test]
    fn test_version_new_without_build_id() {
        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new(path);
        assert!(version.is_ok(), "Version::new should succeed: {version:?}");
        let v = version.unwrap();
        assert_eq!(v.build_id(), None);
    }

    #[test]
    fn test_version_write_output_format() {
        let temp_path = std::env::temp_dir().join("test_version_output.rs");
        let out_path = &temp_path;

        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new(path).expect("Version::new should succeed");
        version.write_version(out_path).expect("write_version should succeed");

        let content = std::fs::read_to_string(out_path).expect("should read output file");
        assert!(content.contains("pub const VERSION:"), "should contain VERSION");
        assert!(
            content.contains("pub const VERSION_MAJOR:"),
            "should contain VERSION_MAJOR"
        );
        assert!(
            content.contains("pub const VERSION_MINOR:"),
            "should contain VERSION_MINOR"
        );
        assert!(
            content.contains("pub const VERSION_PATCH:"),
            "should contain VERSION_PATCH"
        );
        assert!(
            content.contains("pub const SOURCES_FINGERPRINT:"),
            "should contain SOURCES_FINGERPRINT"
        );
        assert!(
            content.contains("pub const BUILD_FINGERPRINT:"),
            "should contain BUILD_FINGERPRINT"
        );
    }

    #[test]
    fn test_version_debug_format() {
        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new(path).expect("Version::new should succeed");
        let debug = format!("{version:?}");
        assert!(debug.contains("Version"), "debug output should contain 'Version'");
        assert!(debug.contains("major"), "debug output should contain 'major'");
        assert!(debug.contains("branch"), "debug output should contain 'branch'");
    }

    #[test]
    fn test_out_file() {
        let path = out_file();
        assert!(
            path.ends_with("version.rs"),
            "out_file should end with 'version.rs', got: {path:?}",
        );
    }

    #[test]
    fn test_write_buildlog() {
        let log_path = std::env::temp_dir().join("test_buildlog.txt");
        let _ = std::fs::remove_file(&log_path);

        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new(path).expect("Version::new should succeed");
        version.write_buildlog(&log_path).expect("write_buildlog should succeed");

        let content = std::fs::read_to_string(&log_path).expect("should read buildlog");
        assert!(
            content.trim_start().starts_with('|'),
            "buildlog line should start with '|'"
        );
        let has_build_type = content.contains("debug") || content.contains("release");
        assert!(
            has_build_type,
            "buildlog should contain 'debug' or 'release'"
        );
    }

    #[test]
    fn test_modified_cannot_build_release_returns_self() {
        let path = env!("CARGO_MANIFEST_DIR");
        let version = Version::new(path).expect("Version::new should succeed");
        let major = version.major();
        let minor = version.minor();
        let returned = version.modified_cannot_build_release();
        assert_eq!(returned.major(), major, "major should match");
        assert_eq!(returned.minor(), minor, "minor should match");
    }
}
