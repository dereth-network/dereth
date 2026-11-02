//! Where the relay's log lines go: standard error, each line behind the UTC time, so an operator
//! can match a line to the game server's own log; or, in tests, wherever the test listens.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A sink for log lines.
#[derive(Clone)]
pub struct Log(Arc<dyn Fn(&str) + Send + Sync>);

impl std::fmt::Debug for Log {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Log")
    }
}

impl Log {
    /// Lines to standard error, each behind the time.
    #[must_use]
    pub fn stderr() -> Self {
        Self(Arc::new(|line| {
            eprintln!("{} {line}", utc(SystemTime::now()));
        }))
    }

    /// Lines to `sink`, as they are.
    #[must_use]
    pub fn to(sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self(Arc::new(sink))
    }

    /// Writes one line.
    pub fn line(&self, line: &str) {
        (self.0)(line);
    }
}

/// `t` as `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn utc(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days to a civil date (proleptic Gregorian), after Howard Hinnant's algorithm.
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_written_as_utc() {
        assert_eq!(utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            utc(UNIX_EPOCH + Duration::from_secs(1_793_491_199)),
            "2026-10-31T23:59:59Z"
        );
        assert_eq!(
            utc(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "2000-02-29T00:00:00Z"
        );
    }
}
