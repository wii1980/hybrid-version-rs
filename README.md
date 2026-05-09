# hybrid-version

**English** | [简体中文](README_CN.md)

> Hybrid Cargo.toml + Git version generation for Rust `build.rs`.

Generate comprehensive version constants and build fingerprints at compile time by merging **Cargo.toml** version metadata with **Git repository** state.

## Features

- **Hybrid version source** — Reads `major.minor.patch` from `Cargo.toml`, combines with git branch, commit, and timestamp
- **Auto-patch from commit count** — When `patch = 0`, automatically counts commits since the version line was last changed (via git blame)
- **Modified lines detection** — Tracks both staged and unstaged changes, appends `-D`, `-M{N}` suffix
- **Release safety** — `modified_cannot_build_release()` panics in release mode if uncommitted changes exist
- **Rich fingerprint output** — Generates `SOURCES_FINGERPRINT` (version + branch + commit + commit time) and `BUILD_FINGERPRINT` (build time + toolchain)
- **Build log** — Writes timestamped build logs to avoid unnecessary rebuilds via `cargo:rerun-if-changed`
- **Environment export** — Export version string to shell via `.env` file or `setx` (Windows)
- **Fluent API** — Method chaining: `Version::new(path)?.write_version(out)?.set_output_env("VAR")?.write_buildlog(log)?`
- **Git submodule support** — Correctly handles nested git repositories
- **No `unsafe` code** — `#![forbid(unsafe_code)]`

## Quick Start

### 1. Add as build dependency

```toml
[build-dependencies]
hybrid-version = "0.1"
```

### 2. Create `build.rs`

```rust
use hybrid_version::error::VResult;
use hybrid_version::version::Version;
use std::path::PathBuf;

fn main() -> VResult<()> {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Buildlog.txt");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    Version::new(&manifest_dir)?
        .modified_cannot_build_release()
        .write_version(PathBuf::from(&out_dir).join("version.rs"))?;
    Ok(())
}
```

### 3. Use in your code

```rust
include!(concat!(env!("OUT_DIR"), "/version.rs"));

fn main() {
    println!("Version: {}", VERSION);
    println!("Source:  {}", SOURCES_FINGERPRINT);
    println!("Build:   {}", BUILD_FINGERPRINT);
}
```

## Generated Constants

The generated `version.rs` provides these compile-time constants:

| Constant | Example | Description |
|---|---|---|
| `VERSION` | `"0.5.3.beta1-D/M12"` | Full version string with build metadata |
| `VERSION_MAJOR` | `0` | Major version from Cargo.toml |
| `VERSION_MINOR` | `5` | Minor version from Cargo.toml |
| `VERSION_PATCH` | `3` | Patch version (auto-calculated if 0 in Cargo.toml) |
| `BUILD_ID` | `"beta1"` | Optional build identifier (from `BUILD_ID` env var) |
| `SOURCES_FINGERPRINT` | `"v0.5.3-D/M dev-57181d0 2023-08-02T14:05:08+08:00"` | Source-level fingerprint |
| `BUILD_FINGERPRINT` | `"2023-08-02T14:10:09+08:00 debug \[stable-x86_64-unknown-linux-gnu, rustc 1.71.0, cargo 1.71.0\]"` | Build environment fingerprint |

The `VERSION` string suffix indicates:
- `-D` — Debug build
- `-M12` — Release build with 12 modified lines
- `-D/M12` — Debug build with 12 modified lines

## Version Public API

The `Version` struct is the core of hybrid-version. Its public fields and methods form the complete user-facing API.

### Constructor

| Method | Description |
|---|---|
| `Version::new(path)` | Create a Version from a project directory. Reads `Cargo.toml` for version numbers and git state (branch, commit, timestamp, modified lines). |
| `Version::new_for(path, build_id)` | Same as `new`, but accepts an explicit build identifier (e.g. `"beta1"`, `"rc.2"`) appended to the version string. |

### Chainable Methods

All methods consume and return `Self`, enabling fluent chaining:

| Method | Description |
|---|---|
| `.modified_cannot_build_release()` | Release-safety gate. In release (`--release`) builds, **panics** if the working tree has uncommitted modifications. In debug builds, this is a no-op. Call it before `write_version` to prevent accidental release builds from dirty sources. |
| `.write_version(path)` | Generate a `version.rs` file at the given path containing all version constants (see [Generated Constants](#generated-constants)). This is the primary output method. |
| `.set_output_env(var)` | Export the version string to an environment variable. On Unix/macOS, writes to `.{VAR}.env` for `source`-ing. On Windows, uses `setx`. |
| `.write_buildlog(path)` | Append a single-line build record to a log file (see [Buildlog](#buildlog)). Useful with `cargo:rerun-if-changed=Buildlog.txt` to avoid unnecessary rebuilds. |

### Getter Methods

| Method | Return Type | Description |
|---|---|---|
| `major()` | `u32` | Major version directly from `Cargo.toml` |
| `minor()` | `u32` | Minor version directly from `Cargo.toml` |
| `patch()` | `u32` | Patch version. If `0` in `Cargo.toml`, auto-calculated from commit count since last version line change |
| `build_id()` | `Option<&str>` | Optional build identifier, e.g. `Some("beta1")` |
| `branch()` | `&str` | Current git branch name (empty string if detached HEAD) |
| `commit()` | `&str` | Short git commit hash (e.g. `57181d0`) |
| `commit_ts()` | `&DateTime` | The commit's author timestamp (converted to local timezone) |
| `modified()` | `usize` | Total staged + unstaged modified lines (used to compute `-D`/`-M{N}` suffix) |
| `build_ts()` | `&DateTime` | Timestamp when `Version::new()`/`new_for()` was called |

### Usage Example

```rust
use hybrid_version::version::Version;

let ver = Version::new("/path/to/project")?;

println!("{}.{}.{}", ver.major(), ver.minor(), ver.patch()); // 0.5.3
println!("branch: {}, commit: {}", ver.branch(), ver.commit()); // main, 57181d0
println!("modified lines: {}", ver.modified()); // e.g. 12
```

### Buildlog

To avoid unnecessary rebuilds, use `write_buildlog` with `cargo:rerun-if-changed`:

```rust
// build.rs
println!("cargo:rerun-if-changed=Buildlog.txt");
Version::new(&manifest_dir)?
    .write_buildlog("Buildlog.txt")?;
```

Build log example:

| Build Time | Type | Branch | Commit | Commit Time | Target | Compiler | Env |
|---|---|---|---|---|---|---|---|
| 2023-08-02T14:38:52+08:00 | Debug | v0.1.17-D/M | dev-f2098e8 | 2023-08-02T14:26:28+08:00 | stable-x86_64-unknown-linux-gnu | rustc 1.71.0 | cargo 1.71.0 |
| 2023-08-02T14:40:56+08:00 | release | v0.1.18 | dev-848120e | 2023-08-02T14:40:45+08:00 | stable-x86_64-unknown-linux-gnu | rustc 1.71.0 | cargo 1.71.0 |

## Why hybrid-version?

Unlike other git-version crates that derive versions solely from git tags or `git describe`, **hybrid-version** keeps your canonical version in `Cargo.toml` and enriches it with git metadata. This approach:

- Keeps `Cargo.toml` as the single source of truth for semantic version
- Automatically derives patch version from commit count (when patch=0)
- Generates rich fingerprints for debugging and traceability
- Prevents accidental release builds with uncommitted changes

## Comparison

| Feature | hybrid-version | git-version | git2version | vergen |
|---|---|---|---|---|
| Version source | Cargo.toml + Git | Git describe | Git tags | Git + others |
| Auto-patch (commit count) | ✅ | ❌ | ❌ | ❌ |
| Modified lines count | ✅ | Dirty flag only | ✅ | ❌ |
| SOURCES_FINGERPRINT | ✅ | ❌ | ❌ | ❌ |
| BUILD_FINGERPRINT | ✅ | ❌ | ❌ | ✅ |
| Build log | ✅ | ❌ | ❌ | ❌ |
| Release safety check | ✅ | ❌ | ❌ | ❌ |
| Env export (.env/setx) | ✅ | ❌ | ❌ | ❌ |
| Fluent API | ✅ | ❌ | ❌ | Builder |

## License

MIT
