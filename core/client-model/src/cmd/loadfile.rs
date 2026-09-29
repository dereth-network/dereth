//! `@loadfile` -- read a file of commands and run them, with variable substitution over each
//! line first.
//!
//! The file is opened by its exact joined argument in CRT text mode. `LoadFile` reads with
//! `fgets(buffer, 0x400, file)`, substitutes every `%DATE%`, displays the resulting chunk, then
//! synchronously hands it back to `on_chat_command`. The command itself sends no traffic, though a
//! command read from the file may.

/// Turn bytes read from a file into the chunks `fgets(buffer, 0x400, file)` would return.
///
/// MSVCR text mode translates CRLF to LF and treats `0x1A` as end of file. The size argument
/// includes the terminator, so at most 1023 translated bytes are returned in one chunk; a long
/// physical line is consequently executed in more than one chunk, just as it is in retail.
#[must_use]
pub fn text_mode_chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut text = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1A {
            break;
        }
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            text.push(b'\n');
            i += 2;
        } else {
            text.push(bytes[i]);
            i += 1;
        }
    }

    let mut chunks = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let cap = (pos + 1023).min(text.len());
        let end = text[pos..cap]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(cap, |newline| pos + newline + 1);
        chunks.push(text[pos..end].to_vec());
        pos = end;
    }
    chunks
}

/// The variable substitution `@loadfile` performs.
///
/// The native function has exactly two substitutions: delete every LF and replace every literal,
/// case-sensitive `%DATE%` with the already-formatted local date. Dollar syntax is not special.
#[must_use]
pub fn make_variable_substitutions(line: &str, date: &str) -> String {
    line.replace('\n', "").replace("%DATE%", date)
}

/// The `strftime("%Y-%m-%d-%a", localtime(...))` spelling used by load-file substitution.
///
/// This build's supported text profile emits English weekday abbreviations. The application
/// supplies the operating system's offset for this instant, so daylight-time boundaries remain
/// local-time boundaries rather than UTC ones.
#[must_use]
pub fn format_date(unix_secs: i64, utc_offset_secs: i32) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let days = (unix_secs + i64::from(utc_offset_secs)).div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = usize::try_from((days + 4).rem_euclid(7)).unwrap_or(0);
    format!("{year:04}-{month:02}-{day:02}-{}", WEEKDAYS[weekday])
}

/// Days since 1970-01-01 to a proleptic-Gregorian date (Howard Hinnant's inverse).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::{format_date, make_variable_substitutions, text_mode_chunks};

    #[test]
    fn substitution_is_only_lf_and_repeated_case_sensitive_percent_date() {
        assert_eq!(
            make_variable_substitutions("@say %DATE%/$name/%date%/%DATE%\n", "2026-09-15-Tue"),
            "@say 2026-09-15-Tue/$name/%date%/2026-09-15-Tue"
        );
    }

    #[test]
    fn date_has_the_native_year_month_day_weekday_shape() {
        assert_eq!(format_date(0, 0), "1970-01-01-Thu");
        assert_eq!(format_date(1_789_490_578, 0), "2026-09-15-Tue");
        assert_eq!(format_date(0, -3600), "1969-12-31-Wed");
    }

    #[test]
    fn text_mode_chunks_translate_crlf_stop_at_ctrl_z_and_cap_fgets_payload() {
        let mut bytes = vec![b'x'; 1024];
        bytes.extend_from_slice(b"\r\nnext\x1aignored\n");
        let chunks = text_mode_chunks(&bytes);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 1023);
        assert_eq!(chunks[1], [b'x', b'\n']);
        assert_eq!(chunks[2], b"next");
    }
}
