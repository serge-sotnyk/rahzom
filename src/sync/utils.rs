//! Shared utilities for sync module

use chrono::{DateTime, Utc};

/// FAT32 filesystem has 2-second mtime precision.
/// We use this tolerance when comparing file modification times.
pub const FAT32_TOLERANCE_SECS: i64 = 2;

/// Checks if two timestamps are equal within FAT32 tolerance
pub fn times_equal_with_tolerance(t1: DateTime<Utc>, t2: DateTime<Utc>) -> bool {
    (t1 - t2).num_seconds().abs() <= FAT32_TOLERANCE_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_times_equal_exact() {
        let now = Utc::now();
        assert!(times_equal_with_tolerance(now, now));
    }

    #[test]
    fn test_times_equal_within_tolerance() {
        let now = Utc::now();
        let later = now + Duration::seconds(2);
        assert!(times_equal_with_tolerance(now, later));
    }

    #[test]
    fn test_times_not_equal_outside_tolerance() {
        let now = Utc::now();
        let later = now + Duration::seconds(3);
        assert!(!times_equal_with_tolerance(now, later));
    }
}
