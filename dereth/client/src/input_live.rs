//! Device events read from a file as it grows, for whoever drives an offscreen client by what its
//! captures show.
//!
//! Each line is one step, taken on its own frame: anything `--input-replay` takes after its frame
//! number, `capture <path>` to write the frame to a PNG, or `quit`. A line that doesn't read is
//! logged and passed over, since the run is being driven by hand.

use std::collections::VecDeque;
use std::path::PathBuf;

use dereth_input::host::HostEvent;

/// How often the file is read.
const READ_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// One step of the file.
#[derive(Debug, PartialEq)]
pub enum Step {
    Event(HostEvent),
    Capture(PathBuf),
    Quit,
}

pub struct LiveInput {
    path: PathBuf,
    /// How much of the file has been read.
    read: usize,
    steps: VecDeque<Step>,
    last_read: Option<std::time::Instant>,
}

impl LiveInput {
    pub fn load(argv: &mut Vec<String>) -> Result<Option<Self>, String> {
        let Some(at) = argv.iter().position(|a| a == "--input-live") else {
            return Ok(None);
        };
        argv.remove(at);
        if at >= argv.len() || argv[at].is_empty() || argv[at].starts_with("--") {
            return Err("--input-live needs a file path".into());
        }
        let path = PathBuf::from(argv.remove(at));
        if !argv.iter().any(|a| a == "--headless") {
            return Err("--input-live requires --headless".into());
        }
        Ok(Some(Self {
            path,
            read: 0,
            steps: VecDeque::new(),
            last_read: None,
        }))
    }

    /// The next step, if one has arrived.
    pub fn next_step(&mut self) -> Option<Step> {
        let now = std::time::Instant::now();
        if self.last_read.is_none_or(|t| now - t >= READ_EVERY) {
            self.last_read = Some(now);
            if let Ok(text) = std::fs::read_to_string(&self.path) {
                self.read += self.take_lines(text.get(self.read..).unwrap_or(""));
            }
        }
        self.steps.pop_front()
    }

    /// Queues the whole lines of `text`, and answers how many bytes they took: a line still being
    /// written waits for its end.
    fn take_lines(&mut self, text: &str) -> usize {
        let whole = text.rfind('\n').map_or(0, |end| end + 1);
        for line in text[..whole].lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match parse(line) {
                Ok(step) => self.steps.push_back(step),
                Err(e) => tracing::warn!("--input-live: {line}: {e}"),
            }
        }
        whole
    }
}

fn parse(line: &str) -> Result<Step, &'static str> {
    if line == "quit" {
        return Ok(Step::Quit);
    }
    if let Some(path) = line.strip_prefix("capture ") {
        return Ok(Step::Capture(PathBuf::from(path.trim())));
    }
    let fields: Vec<_> = line.split_whitespace().collect();
    crate::input_replay::parse_event(&fields).map(Step::Event)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a driving tool's file format).
    use super::*;

    fn input() -> LiveInput {
        LiveInput {
            path: PathBuf::new(),
            read: 0,
            steps: VecDeque::new(),
            last_read: None,
        }
    }

    #[test]
    fn only_whole_lines_are_taken_and_a_later_read_finishes_the_rest() {
        let mut live = input();
        assert_eq!(live.take_lines("move 1 2\ncapt"), 9);
        assert_eq!(live.steps.len(), 1);
        assert_eq!(live.take_lines("capt"), 0);
        assert_eq!(live.take_lines("capture a.png\nquit\n"), 19);
        assert_eq!(
            live.steps.pop_front().map(|s| matches!(s, Step::Event(_))),
            Some(true)
        );
        assert_eq!(live.steps.pop_front(), Some(Step::Capture("a.png".into())));
        assert_eq!(live.steps.pop_front(), Some(Step::Quit));
    }

    #[test]
    fn a_line_that_does_not_read_is_passed_over() {
        let mut live = input();
        assert_eq!(live.take_lines("# a note\nnonsense\nquit\n"), 23);
        assert_eq!(live.steps.pop_front(), Some(Step::Quit));
        assert_eq!(live.steps.pop_front(), None);
    }

    #[test]
    fn the_switch_needs_a_path_and_a_headless_run() {
        let args = |a: &[&str]| a.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(LiveInput::load(&mut args(&["--headless"]))
            .unwrap()
            .is_none());
        assert!(LiveInput::load(&mut args(&["--input-live", "a"])).is_err());
        assert!(LiveInput::load(&mut args(&["--headless", "--input-live"])).is_err());
        let mut argv = args(&["--input-live", "a", "--headless"]);
        assert!(LiveInput::load(&mut argv).unwrap().is_some());
        assert_eq!(argv, ["--headless"]);
    }
}
