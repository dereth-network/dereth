//! Divergence: V378
//! The console prompt redraws a half-typed line under printed output, redraws a full-row line
//! cleanly, edits and recalls history, and a command's reply prints whatever the log level.
//! Fixture: a fake terminal, synthetic key events and captured command output.

use std::sync::{Mutex, OnceLock};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use empyrean_command::command_manager;
use empyrean_command::console::{KeyAction, PromptLine};
use empyrean_command::handlers::command_handler_helper::{write_output_debug, write_output_info};
use empyrean_entity::enums::ChatMessageType;
use empyrean_testkit::TestServer;

/// A terminal `cols` wide with unlimited rows: printable characters (wrapping after the last
/// column, as terminals do, only when the next character comes), `\r`, `\n` (down one row, as in
/// raw mode), `ESC[nA`, `ESC[nG` and `ESC[J`. Anything else fails the test.
struct FakeTerminal {
    cols: usize,
    rows: Vec<Vec<char>>,
    row: usize,
    col: usize,
    pending_wrap: bool,
}

impl FakeTerminal {
    fn new(cols: usize) -> Self {
        Self {
            cols,
            rows: vec![Vec::new()],
            row: 0,
            col: 0,
            pending_wrap: false,
        }
    }

    fn row_mut(&mut self, row: usize) -> &mut Vec<char> {
        while self.rows.len() <= row {
            self.rows.push(Vec::new());
        }
        &mut self.rows[row]
    }

    fn put(&mut self, c: char) {
        if self.pending_wrap {
            self.row += 1;
            self.col = 0;
            self.pending_wrap = false;
        }
        let (row, col) = (self.row, self.col);
        let line = self.row_mut(row);
        while line.len() <= col {
            line.push(' ');
        }
        line[col] = c;
        if self.col + 1 == self.cols {
            self.pending_wrap = true;
        } else {
            self.col += 1;
        }
    }

    fn write(&mut self, text: &str) {
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    self.col = 0;
                    self.pending_wrap = false;
                }
                '\n' => {
                    self.row += 1;
                    self.row_mut(self.row);
                    self.pending_wrap = false;
                }
                '\x1b' => {
                    assert_eq!(chars.next(), Some('['), "{text:?}");
                    let mut n = String::new();
                    let command = loop {
                        let c = chars.next().expect("a complete escape sequence");
                        if c.is_ascii_digit() {
                            n.push(c);
                        } else {
                            break c;
                        }
                    };
                    let n: usize = if n.is_empty() { 1 } else { n.parse().unwrap() };
                    self.pending_wrap = false;
                    match command {
                        'A' => self.row = self.row.checked_sub(n).expect("up past the top"),
                        'G' => {
                            assert!(n >= 1 && n <= self.cols, "column {n} of {}", self.cols);
                            self.col = n - 1;
                        }
                        'J' => {
                            let (row, col) = (self.row, self.col);
                            self.row_mut(row).truncate(col);
                            self.rows.truncate(row + 1);
                        }
                        other => panic!("unexpected escape sequence ESC[{n}{other}"),
                    }
                }
                c if c.is_control() => panic!("unexpected control character {c:?}"),
                c => self.put(c),
            }
        }
    }

    /// The screen's rows, trailing spaces trimmed.
    fn screen(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|r| r.iter().collect::<String>().trim_end().to_owned())
            .collect()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn type_text(line: &mut PromptLine, text: &str) {
    for c in text.chars() {
        assert_eq!(line.key(key(KeyCode::Char(c))), KeyAction::Edited);
    }
}

/// A key at the prompt as the editor writes it: the old prompt rows cleared, the key applied, the
/// new prompt line drawn.
fn press(term: &mut FakeTerminal, line: &mut PromptLine, k: KeyEvent) {
    let cols = term.cols;
    term.write(&line.clear(cols));
    assert_eq!(line.key(k), KeyAction::Edited);
    term.write(&line.draw(cols));
}

/// Log lines printed while a half-typed line wraps under the prompt land above the prompt, and
/// the prompt, the whole typed line and the cursor come back where they were.
#[test]
fn a_line_printed_above_the_prompt_redraws_the_half_typed_line() {
    let mut term = FakeTerminal::new(20);
    term.write("earlier output\r\n");
    let mut line = PromptLine::new("empyrean>> ");
    term.write(&line.draw(20));
    assert_eq!(term.screen(), ["earlier output", "empyrean>>"]);
    assert_eq!((term.row, term.col), (1, 11));

    // 11 + 13 characters wrap onto a second row; five Lefts put the cursor on the '1' of 12.
    for c in "teleloc 12 34".chars() {
        press(&mut term, &mut line, key(KeyCode::Char(c)));
    }
    for _ in 0..5 {
        press(&mut term, &mut line, key(KeyCode::Left));
    }
    assert_eq!(
        term.screen(),
        ["earlier output", "empyrean>> teleloc 1", "2 34"]
    );
    assert_eq!((term.row, term.col), (1, 19));

    // A long log line (it wraps) and a short one.
    term.write(&line.print_above(
        "2026-09-25 12:00:00.000 WARN  [World Manager] a long line",
        20,
    ));
    term.write(&line.print_above("short", 20));
    assert_eq!(
        term.screen(),
        [
            "earlier output",
            "2026-09-25 12:00:00.",
            "000 WARN  [World Man",
            "ager] a long line",
            "short",
            "empyrean>> teleloc 1",
            "2 34",
        ]
    );
    assert_eq!(
        (term.row, term.col),
        (5, 19),
        "the cursor is back on the '1' of 12"
    );

    // With the cursor on the second row, a reply of two lines (its line breaks kept).
    press(&mut term, &mut line, key(KeyCode::End));
    assert_eq!((term.row, term.col), (6, 4));
    term.write(&line.print_above("one\ntwo", 20));
    assert_eq!(
        term.screen()[5..],
        ["one", "two", "empyrean>> teleloc 1", "2 34"]
    );
    assert_eq!((term.row, term.col), (8, 4));

    // Typing goes on where it was.
    press(&mut term, &mut line, key(KeyCode::Char('5')));
    assert_eq!(term.screen()[7..], ["empyrean>> teleloc 1", "2 345"]);
    assert_eq!(line.line(), "teleloc 12 345");
}

/// A prompt and line that exactly fill their row leave the cursor at the start of the next row,
/// and a line printed above them still clears every row they use.
#[test]
fn a_prompt_line_that_fills_its_row_is_redrawn_cleanly() {
    let mut term = FakeTerminal::new(16);
    let mut line = PromptLine::new("empyrean>> ");
    term.write(&line.draw(16));
    for c in "abcde".chars() {
        press(&mut term, &mut line, key(KeyCode::Char(c)));
    }
    assert_eq!(term.screen(), ["empyrean>> abcde", ""]);
    assert_eq!((term.row, term.col), (1, 0));

    term.write(&line.print_above("log", 16));
    assert_eq!(term.screen(), ["log", "empyrean>> abcde", ""]);
    assert_eq!((term.row, term.col), (2, 0));

    press(&mut term, &mut line, key(KeyCode::Home));
    assert_eq!((term.row, term.col), (1, 11));
    term.write(&line.print_above("log two", 16));
    assert_eq!(term.screen(), ["log", "log two", "empyrean>> abcde", ""]);
    assert_eq!((term.row, term.col), (2, 11));
}

/// The keys the prompt line takes: editing, Enter, Ctrl-C, Ctrl-D, and Up/Down through the
/// entered lines.
#[test]
fn the_prompt_line_edits_and_recalls() {
    let mut line = PromptLine::new("> ");
    type_text(&mut line, "popx");
    assert_eq!(line.key(key(KeyCode::Backspace)), KeyAction::Edited);
    assert_eq!(line.key(key(KeyCode::Home)), KeyAction::Edited);
    assert_eq!(line.key(key(KeyCode::Delete)), KeyAction::Edited);
    assert_eq!(line.key(key(KeyCode::Char('p'))), KeyAction::Edited);
    assert_eq!((line.line().as_str(), line.cursor()), ("pop", 1));
    assert_eq!(line.key(key(KeyCode::Home)), KeyAction::Edited);
    assert_eq!(line.key(key(KeyCode::Home)), KeyAction::Ignored);
    assert_eq!(line.key(key(KeyCode::Enter)), KeyAction::Line("pop".into()));
    assert_eq!((line.line().as_str(), line.cursor()), ("", 0));

    type_text(&mut line, "version");
    assert_eq!(
        line.key(key(KeyCode::Enter)),
        KeyAction::Line("version".into())
    );
    type_text(&mut line, "hal");
    assert_eq!(line.key(key(KeyCode::Up)), KeyAction::Edited);
    assert_eq!(line.line(), "version");
    assert_eq!(line.key(key(KeyCode::Up)), KeyAction::Edited);
    assert_eq!(line.line(), "pop");
    assert_eq!(line.key(key(KeyCode::Up)), KeyAction::Ignored);
    assert_eq!(line.key(key(KeyCode::Down)), KeyAction::Edited);
    assert_eq!(line.line(), "version");
    assert_eq!(line.key(key(KeyCode::Down)), KeyAction::Edited);
    assert_eq!(line.line(), "hal", "the half-typed line comes back");

    assert_eq!(line.key(key(KeyCode::Left)), KeyAction::Edited);
    assert_eq!(
        line.key(ctrl('d')),
        KeyAction::Edited,
        "Ctrl-D deletes under the cursor mid-line"
    );
    assert_eq!(line.key(ctrl('a')), KeyAction::Edited);
    assert_eq!(line.key(ctrl('d')), KeyAction::Edited);
    assert_eq!(line.line(), "a");
    assert_eq!(line.key(ctrl('c')), KeyAction::Interrupt);
    assert_eq!(line.line(), "");
    assert_eq!(
        line.key(ctrl('d')),
        KeyAction::EndOfInput,
        "Ctrl-D on an empty line ends the input"
    );
    assert_eq!(line.key(ctrl('z')), KeyAction::EndOfInput);
    assert_eq!(
        line.key(key(KeyCode::Enter)),
        KeyAction::Line(String::new())
    );
    // Control with Alt is AltGr (Windows): the character it makes is typed.
    let alt_gr = KeyEvent::new(
        KeyCode::Char('@'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    assert_eq!(line.key(alt_gr), KeyAction::Edited);
    assert_eq!(line.key(key(KeyCode::Enter)), KeyAction::Line("@".into()));
}

/// Log records seen by the test logger.
static LOGGED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Recorder;

impl log::Log for Recorder {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            LOGGED.lock().unwrap().push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

/// A console command's reply (`CommandHandlerHelper.WriteOutputInfo` with no session: ACE's
/// `log.Info`) is console output whatever the log level: written at `error` and `warn`, and never
/// also a log line. Its developer detail (`WriteOutputDebug`) stays a debug log line.
#[test]
fn a_console_commands_reply_does_not_depend_on_the_log_level() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        log::set_logger(&Recorder).expect("the only logger in this test binary");
    });
    let mut ts = TestServer::new();
    for level in [
        log::LevelFilter::Error,
        log::LevelFilter::Warn,
        log::LevelFilter::Trace,
    ] {
        log::set_max_level(level);
        command_manager::start_console_capture();
        write_output_info(
            &mut ts.world,
            None,
            &format!("console reply at {level}"),
            ChatMessageType::Broadcast,
        );
        write_output_debug(
            &mut ts.world,
            None,
            &format!("console detail at {level}"),
            ChatMessageType::Broadcast,
        );
        let out = command_manager::take_console_output();
        assert_eq!(
            out,
            [
                format!("console reply at {level}"),
                format!("console detail at {level}")
            ]
        );
    }
    log::set_max_level(log::LevelFilter::Off);
    let logged = LOGGED.lock().unwrap().clone();
    assert!(
        !logged.iter().any(|l| l.starts_with("console reply")),
        "the reply is not logged: {logged:?}"
    );
    assert!(
        logged.iter().any(|l| l == "console detail at TRACE"),
        "{logged:?}"
    );
    assert!(
        !logged.iter().any(|l| l == "console detail at WARN"),
        "{logged:?}"
    );
}
