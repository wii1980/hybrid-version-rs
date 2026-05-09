//! Error types for version generation operations.
//!
//! Provides [`VersionError`] as the unified error enum and [`VResult`]
//! as a convenience alias for `Result<T, VersionError>`.

/// Convenience alias for `Result<T, VersionError>`.
pub type VResult<T> = Result<T, VersionError>;

/// 版本/构建相关错误的统一类型。
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// I/O error (file read/write failures).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// UTF-8 conversion error.
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),

    /// Environment variable error (missing or invalid `OUT_DIR`, etc.).
    #[error(transparent)]
    Env(#[from] std::env::VarError),

    /// Integer parsing error (version number, timestamp, etc.).
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),

    /// Path prefix stripping error (computing relative paths).
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),

    /// 字符串形式错误信息（如业务逻辑中的错误描述）。
    #[error("{0}")]
    Message(String),

    /// Git CLI command execution failure (non-zero exit code).
    #[error("git command failed: {0}")]
    GitCommand(String),
}

impl VersionError {
    /// 从任意实现了 `Error` 的类型构造为 `Message` 变体。
    pub fn new(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Message(err.to_string())
    }
}

impl From<String> for VersionError {
    fn from(s: String) -> Self {
        Self::Message(s)
    }
}

impl From<&str> for VersionError {
    fn from(s: &str) -> Self {
        Self::Message(s.to_string())
    }
}

impl From<std::convert::Infallible> for VersionError {
    fn from(_: std::convert::Infallible) -> Self {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_string() {
        let err = VersionError::from("test error".to_string());
        assert!(matches!(err, VersionError::Message(ref s) if s == "test error"));
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_from_str() {
        let err: VersionError = "test error".into();
        assert!(matches!(err, VersionError::Message(ref s) if s == "test error"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = VersionError::from(io_err);
        assert!(matches!(err, VersionError::Io(_)));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_git_command_error() {
        let err = VersionError::GitCommand("something failed".to_string());
        assert!(err.to_string().contains("git command failed"));
    }

    #[test]
    fn test_error_chain() {
        let err = VersionError::Message("test".to_string());
        let _: &dyn std::error::Error = &err;
    }
}
