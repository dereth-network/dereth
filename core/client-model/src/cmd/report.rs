//! `@report` and `@perfcapture`, this client's own commands for reporting what it draws and how
//! fast (CD-report-commands). Retail has neither.
//!
//! `@report` prints one line saying where the player stands, where the camera is and how the
//! frame is drawn, and copies it, so a picture seen on one machine can be drawn again on another.
//! `@perfcapture` records the frame times for a while and saves them to a file. Neither sends
//! anything. This module holds the commands' words: what a typed line asks for, and the sentences
//! they answer with.

/// The seconds `@perfcapture` records when it is given none.
pub const DEFAULT_CAPTURE_SECONDS: u32 = 10;

/// The most seconds one capture records.
pub const MAX_CAPTURE_SECONDS: u32 = 60;

/// What `@report` takes: the tail of its refusal.
pub const REPORT_USAGE: &str = "@report takes no words.";

/// What `@perfcapture` takes: the tail of every refusal.
pub const CAPTURE_USAGE: &str =
    "@perfcapture takes a number of seconds from 1 to 60, or none for 10.";

/// The answer when there is no world, and so nothing drawn, yet.
pub const NO_WORLD: &str = "A report can be made once the world is loaded.";

/// The line after the report, once it is on the clipboard.
pub const COPIED: &str = "The report is on the clipboard.";

/// The answer to a capture asked for while one is recording.
pub const ALREADY_RECORDING: &str = "A performance capture is already recording.";

/// Read a typed `@report`'s arguments. `Err` is the one line that says what was wrong.
///
/// # Errors
///
/// Any word at all.
pub fn parse_report(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("Too many words after @report. {REPORT_USAGE}"))
    }
}

/// Read a typed `@perfcapture`'s arguments: the seconds to record. `Err` is the one line that
/// says what was wrong.
///
/// # Errors
///
/// More than one word, or a word that is not a whole number of seconds from 1 to
/// [`MAX_CAPTURE_SECONDS`].
pub fn parse_capture(args: &[String]) -> Result<u32, String> {
    let word = match args {
        [] => return Ok(DEFAULT_CAPTURE_SECONDS),
        [word] => word.as_str(),
        _ => {
            return Err(format!(
                "Too many words after @perfcapture. {CAPTURE_USAGE}"
            ))
        }
    };
    match word.parse::<u32>() {
        Ok(seconds) if (1..=MAX_CAPTURE_SECONDS).contains(&seconds) => Ok(seconds),
        _ => Err(format!(
            "\"{word}\" is not a number of seconds. {CAPTURE_USAGE}"
        )),
    }
}

/// The answer to `@perfcapture`, as it starts recording.
#[must_use]
pub fn capture_started(seconds: u32) -> String {
    if seconds == 1 {
        "Recording a performance capture for 1 second.".to_owned()
    } else {
        format!("Recording a performance capture for {seconds} seconds.")
    }
}

/// The answer once a capture is saved: where, and what it found.
#[must_use]
pub fn capture_saved(path: &str, summary: &str) -> String {
    format!("Performance capture saved: {path}\n{summary}")
}

/// The answer when a capture could not be saved.
#[must_use]
pub fn capture_failed(why: &str) -> String {
    format!("The performance capture could not be saved: {why}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        crate::cmd::interp::find_all_words(line)
    }

    /// Behaviour: chat.commands.report-and-perfcapture-are-answered-by-this-client-and-never-sent
    #[test]
    fn perfcapture_takes_whole_seconds_up_to_a_minute_and_report_takes_nothing() {
        assert_eq!(parse_report(&[]), Ok(()));
        let refused = parse_report(&words("now")).unwrap_err();
        assert!(refused.contains(REPORT_USAGE), "{refused}");

        assert_eq!(parse_capture(&[]), Ok(DEFAULT_CAPTURE_SECONDS));
        for (line, seconds) in [("1", 1), ("30", 30), ("60", 60)] {
            assert_eq!(parse_capture(&words(line)), Ok(seconds), "{line}");
        }
        for line in ["0", "61", "-5", "2.5", "ten", "5 10"] {
            let refused = parse_capture(&words(line)).unwrap_err();
            assert!(refused.contains(CAPTURE_USAGE), "{line}: {refused}");
        }
        assert_eq!(
            capture_started(1),
            "Recording a performance capture for 1 second."
        );
        assert_eq!(
            capture_started(10),
            "Recording a performance capture for 10 seconds."
        );
    }
}
