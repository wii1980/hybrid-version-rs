//! # hybrid-version
//!
//! Hybrid Cargo.toml + Git version generation for Rust `build.rs`.
//!
//! Generates comprehensive version constants and build fingerprints at compile time
//! by merging **Cargo.toml** version metadata with **Git repository** state.
//!
//! # Features
//!
//! - **Hybrid version source** — Reads `major.minor.patch` from `Cargo.toml`,
//!   combines with git branch, commit, and timestamp
//! - **Auto-patch from commit count** — When `patch = 0`, automatically counts
//!   commits since the version line was last changed (via `git blame`)
//! - **Modified lines detection** — Tracks both staged and unstaged changes,
//!   appends `-D`, `-M{N}` suffix
//! - **Release safety** — `modified_cannot_build_release()` panics in release
//!   mode if uncommitted changes exist
//! - **Rich fingerprint output** — Generates `SOURCES_FINGERPRINT` and
//!   `BUILD_FINGERPRINT` constants
//! - **Build log** — Writes timestamped build logs for `cargo:rerun-if-changed`
//! - **Fluent API** — Method chaining for `build.rs`
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use hybrid_version::error::VResult;
//! use hybrid_version::version::Version;
//! use std::path::PathBuf;
//!
//! fn main() -> VResult<()> {
//!     let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
//!     let out_dir = std::env::var("OUT_DIR").unwrap();
//!
//!     Version::new(&manifest_dir)?
//!         .modified_cannot_build_release()
//!         .write_version(PathBuf::from(&out_dir).join("version.rs"))?;
//!     Ok(())
//! }
//! ```
//!
//! # Generated Constants
//!
//! The generated `version.rs` file provides these compile-time constants:
//!
//! | Constant | Example | Description |
//! |----------|---------|-------------|
//! | `VERSION` | `"0.5.3.beta1-D/M12"` | Full version string |
//! | `VERSION_MAJOR` | `0` | Major from Cargo.toml |
//! | `VERSION_MINOR` | `5` | Minor from Cargo.toml |
//! | `VERSION_PATCH` | `3` | Patch (auto if `0` in Cargo.toml) |
//! | `BUILD_ID` | `"beta1"` | Optional build identifier |
//! | `SOURCES_FINGERPRINT` | `"v0.5.3-D/M dev-57181d0 2023-08-02T14:05:08+08:00"` | Source fingerprint |
//! | `BUILD_FINGERPRINT` | `"2023-08-02T14:10:09+08:00 debug [...]"` | Build fingerprint |

mod compiler;
mod git;
mod ts;

pub mod error;
pub mod version;
