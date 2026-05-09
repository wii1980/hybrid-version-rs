//! Timestamp types and formatting utilities.
//!
//! Provides [`DateTime`] (a timezone-aware datetime wrapper around `time::OffsetDateTime`)
//! and the [`Format`] trait for human-readable output.
//!
//! Local time is obtained via `tzdb`; falls back to UTC on failure.

use std::error::Error;
use time::format_description::well_known::{Rfc2822, Rfc3339};
use time::{format_description, OffsetDateTime, UtcOffset};

/// 人类可读时间格式。
pub trait Format {
    fn human_format(&self) -> String;
}

/// 日期时间（本地或 UTC）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DateTime {
    /// Local timezone datetime.
    Local(OffsetDateTime),
    /// UTC datetime.
    Utc(OffsetDateTime),
}

/// 当前时间戳（优先本地时区，失败则回退 UTC）。
pub fn now_timestamp() -> DateTime {
    DateTime::now()
}

impl Default for DateTime {
    fn default() -> Self {
        Self::now()
    }
}

impl DateTime {
    /// 当前时间（优先本地，失败则 UTC）。
    pub fn now() -> Self {
        Self::local_now().unwrap_or_else(|_| Self::Utc(OffsetDateTime::now_utc()))
    }

    /// 本地当前时间（使用 tzdb）。
    pub fn local_now() -> Result<Self, Box<dyn Error>> {
        let local_time = tzdb::now::local()?;
        let offset =
            UtcOffset::from_whole_seconds(local_time.local_time_type().ut_offset())?;
        let dt = OffsetDateTime::from_unix_timestamp(local_time.unix_time())?
            .to_offset(offset);
        Ok(Self::Local(dt))
    }

    /// 将 Unix 时间戳转为 UTC 时间。
    pub fn timestamp_to_utc(timestamp: i64) -> Self {
        let time = OffsetDateTime::from_unix_timestamp(timestamp)
            .expect("valid unix timestamp");
        Self::Utc(time)
    }

    /// 将 Unix 时间戳转为指定偏移（分钟）的本地时间。
    pub fn timestamp_to_local(timestamp: i64, offset_minutes: i32) -> Self {
        let time = OffsetDateTime::from_unix_timestamp(timestamp)
            .expect("valid unix timestamp")
            .to_offset(
                UtcOffset::from_whole_seconds(offset_minutes * 60)
                    .expect("valid offset in minutes"),
            );
        Self::Local(time)
    }

    /// RFC 2822 格式字符串。
    pub fn to_rfc2822(self) -> String {
        match self {
            Self::Local(dt) | Self::Utc(dt) => dt.format(&Rfc2822).expect("format rfc2822"),
        }
    }

    /// RFC 3339 格式字符串。
    pub fn to_rfc3339(self) -> String {
        match self {
            Self::Local(dt) | Self::Utc(dt) => dt.format(&Rfc3339).expect("format rfc3339"),
        }
    }
}

impl Format for DateTime {
    fn human_format(&self) -> String {
        match self {
            Self::Local(dt) | Self::Utc(dt) => dt.human_format(),
        }
    }
}

/// Human-readable format: `"YYYY-MM-DD HH:MM:SS +HH:MM"`.
impl Format for OffsetDateTime {
    fn human_format(&self) -> String {
        let fmt = format_description::parse(
            "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour \
         sign:mandatory]:[offset_minute]",
        )
        .expect("valid format description");
        self.format(&fmt).expect("format datetime")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_now_human_format() {
        let time = DateTime::local_now().unwrap().human_format();
        #[cfg(unix)]
        assert!(!std::fs::read("/etc/localtime").unwrap().is_empty());

        assert_eq!(time.len(), 26);
        assert!(time.as_bytes()[4] == b'-');
        assert!(time.as_bytes()[7] == b'-');
        assert!(time.as_bytes()[10] == b' ');
        assert!(time.as_bytes()[13] == b':');
        assert!(time.as_bytes()[16] == b':');
        assert!(time.as_bytes()[19] == b' ');
        let sign = time.as_bytes()[20];
        assert!(sign == b'+' || sign == b'-');
        assert!(time.as_bytes()[23] == b':');

        println!("local now:{time}");
    }

    #[test]
    fn test_timestamp_2_utc() {
        let time = DateTime::timestamp_to_utc(1_689_747_042);

        assert_eq!(time.to_rfc2822(), "Wed, 19 Jul 2023 06:10:42 +0000");
        assert_eq!(time.to_rfc3339(), "2023-07-19T06:10:42Z");
        assert_eq!(time.human_format(), "2023-07-19 06:10:42 +00:00");

        let time = DateTime::timestamp_to_local(1_689_747_042, 480);

        println!("{}", time.to_rfc2822());
        println!("{}", time.to_rfc3339());
        println!("{}", time.human_format());

        assert_eq!(time.to_rfc2822(), "Wed, 19 Jul 2023 14:10:42 +0800");
        assert_eq!(time.to_rfc3339(), "2023-07-19T14:10:42+08:00");
        assert_eq!(time.human_format(), "2023-07-19 14:10:42 +08:00");
    }

    #[test]
    fn test_datetime_default() {
        let dt = DateTime::default();
        assert!(!dt.to_rfc3339().is_empty());
    }

    #[test]
    fn test_datetime_now() {
        let dt = DateTime::now();
        assert!(!dt.to_rfc3339().is_empty());
    }

    #[test]
    fn test_timestamp_to_local_offset() {
        let dt = DateTime::timestamp_to_local(1_689_747_042, 480);
        assert_eq!(dt.to_rfc3339(), "2023-07-19T14:10:42+08:00");
    }

    #[test]
    fn test_timestamp_to_local_negative_offset() {
        let dt = DateTime::timestamp_to_local(1_689_747_042, -300);
        assert!(dt.to_rfc3339().contains("-05:00"));
    }

    #[test]
    fn test_timestamp_to_utc_rfc3339() {
        assert_eq!(DateTime::timestamp_to_utc(0).to_rfc3339(), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_datetime_equality() {
        assert_eq!(DateTime::timestamp_to_utc(100), DateTime::timestamp_to_utc(100));
        assert_ne!(DateTime::timestamp_to_utc(100), DateTime::timestamp_to_utc(200));
    }

    #[test]
    fn test_human_format_utc() {
        assert_eq!(
            DateTime::timestamp_to_utc(1_689_747_042).human_format(),
            "2023-07-19 06:10:42 +00:00"
        );
    }
}
