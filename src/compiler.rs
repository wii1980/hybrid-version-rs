//! Build environment detection (toolchain, build type, compiler versions).
//!
//! Provides [`CompilerEnv`] for detecting the current Rust toolchain info
//! and [`BuildModel`] for classifying debug/release builds.

use crate::error::VResult;
use std::env;
use std::fmt;
use std::process::Command;

/// 构建类型（Debug/Release）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildModel {
    /// Build type could not be determined.
    #[default]
    Unknown,
    /// Debug build (`cfg!(debug_assertions)` is true).
    Debug,
    /// Release build (`cfg!(debug_assertions)` is false).
    Release,
}

/// 根据是否开启 `debug_assertions` 返回当前构建类型。
pub fn build_model() -> BuildModel {
    if cfg!(debug_assertions) {
        BuildModel::Debug
    } else {
        BuildModel::Release
    }
}

impl fmt::Display for BuildModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unknown => "unknown",
            Self::Debug => "debug",
            Self::Release => "release",
        })
    }
}

/// 编译环境信息（工具链、rustc/cargo 版本等）。
#[allow(clippy::module_name_repetitions)]
pub struct CompilerEnv {
    /// Current build type (debug/release/unknown).
    pub build_type: BuildModel,
    /// Target triple from `TARGET` environment variable.
    pub rust_toolchain: String,
    /// `rustc` version string (e.g., `"rustc 1.75.0 (82e1608df 2023-12-21)"`).
    pub rust_version: String,
    /// `cargo` version string (e.g., `"cargo 1.75.0 (1d8b05cdd 2023-11-20)"`).
    pub cargo_version: String,
}

impl CompilerEnv {
    /// 检测当前编译环境并返回 `CompilerEnv`。
    pub fn new() -> VResult<Self> {
        let rust_toolchain = env::var("TARGET").unwrap_or_else(|_| "-".to_string());

        let rust_out = Command::new("rustc").arg("-V").output()?;
        let cargo_out = Command::new("cargo").arg("-V").output()?;

        let rust_version = String::from_utf8(rust_out.stdout)?
            .trim()
            .to_string();
        let cargo_version = String::from_utf8(cargo_out.stdout)?
            .trim()
            .to_string();

        Ok(Self {
            build_type: build_model(),
            rust_toolchain,
            rust_version,
            cargo_version,
        })
    }
}

impl fmt::Debug for CompilerEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompilerEnv")
            .field("build_type", &self.build_type)
            .field("rust_toolchain", &self.rust_toolchain)
            .field("rust_version", &self.rust_version)
            .field("cargo_version", &self.cargo_version)
            .finish()
    }
}

impl Default for CompilerEnv {
    fn default() -> Self {
        Self {
            build_type: BuildModel::Unknown,
            rust_toolchain: "-".to_string(),
            rust_version: "-".to_string(),
            cargo_version: "-".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_env_detection() {
        let env = CompilerEnv::new();
        assert!(env.is_ok(), "CompilerEnv::new should succeed in dev environment");
        let env = env.unwrap();
        assert_ne!(env.rust_version, "-");
        assert_ne!(env.cargo_version, "-");
    }

    #[test]
    fn test_build_model_default() {
        assert_eq!(BuildModel::default(), BuildModel::Unknown);
    }

    #[test]
    fn test_build_model_display() {
        assert_eq!(BuildModel::Unknown.to_string(), "unknown");
        assert_eq!(BuildModel::Debug.to_string(), "debug");
        assert_eq!(BuildModel::Release.to_string(), "release");
    }

    #[test]
    fn test_build_model_func() {
        #[cfg(debug_assertions)]
        assert_eq!(build_model(), BuildModel::Debug);
        #[cfg(not(debug_assertions))]
        assert_eq!(build_model(), BuildModel::Release);
    }

    #[test]
    fn test_compiler_env_default() {
        let env = CompilerEnv::default();
        assert_eq!(env.build_type, BuildModel::Unknown);
        assert_eq!(env.rust_toolchain, "-");
    }

    #[test]
    fn test_compiler_env_debug() {
        let debug_str = format!("{:?}", CompilerEnv::default());
        assert!(debug_str.contains("build_type"));
        assert!(debug_str.contains("rust_toolchain"));
    }
}
