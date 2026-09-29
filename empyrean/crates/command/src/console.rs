//! Not ACE: the server's console output and its interactive prompt.
//!
//! Every console write goes through one lock here: a command's reply and the prompt (standard
//! output), and each log line (standard error, written by the server's logger through
//! [`write_log_line`]). The lock keeps lines whole and in order across threads.
//!
//! **The line editor.** When `server.interactive_console` is true and standard input and output
//! are both terminals, the console reads keys itself ([`run_editor`]) instead of whole lines, so it
//! always knows what the prompt row shows: the prompt and the half-typed line. A line printed while
//! the prompt is up (a log line from any thread, or a command's reply from the world thread) is
//! written *above* it: the prompt rows are cleared, the line is written, and the prompt and the
//! half-typed line are drawn again with the cursor where it was ([`PromptLine::print_above`]).
//! Otherwise (a pipe, a file, a service) lines are simply written as they come and the prompt, when
//! there is one, is ACE's plain `Console.Write`.
//!
//! **Streams.** Log lines stay on standard error and replies on standard output. While the editor
//! owns the terminal, a log line goes through the editor (to standard output) only when standard
//! error is that terminal too; redirected, it is written to the redirect as usual.
//!
//! **Terminal state.** The editor puts the terminal in raw mode (no echo, keys one at a time; on
//! Windows the console's VT processing is turned on for the escape sequences it writes). [`stop`]
//! gives the terminal back — the server calls it before it exits, however it exits — and a Ctrl-C
//! typed at the prompt, which raw mode delivers as a key rather than a signal, runs the handler
//! set with [`set_interrupt_handler`] (the server's Ctrl-C shutdown).

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;

/// What the console shows: the prompt line while the editor runs.
struct Console {
    editor: Option<PromptLine>,
    /// Whether standard error is the editor's terminal (log lines then go above the prompt).
    stderr_on_terminal: bool,
}

static CONSOLE: Mutex<Console> = Mutex::new(Console {
    editor: None,
    stderr_on_terminal: false,
});

/// Set by [`stop`]: the editor ends and is not started again.
static STOPPED: AtomicBool = AtomicBool::new(false);

type InterruptHandler = Box<dyn Fn() + Send + Sync>;

static INTERRUPT: Mutex<Option<InterruptHandler>> = Mutex::new(None);

fn console() -> MutexGuard<'static, Console> {
    CONSOLE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Writes `text` to standard output and flushes it (errors are ignored: the console is best
/// effort, as `Console.Write` is).
fn write_stdout(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// The terminal's width in columns (80 when it cannot be read).
fn columns() -> usize {
    terminal::size()
        .map_or(80, |(cols, _)| usize::from(cols))
        .max(1)
}

/// A line of console output (a command's reply): above the prompt while the editor runs, else
/// on standard output.
pub fn write_line(text: &str) {
    let console = console();
    match &console.editor {
        Some(editor) => write_stdout(&editor.print_above(text, columns())),
        None => write_stdout(&format!("{text}\n")),
    }
}

/// Console output without a line end (the plain prompt, when the editor is not running).
pub fn write(text: &str) {
    let console = console();
    if console.editor.is_none() {
        write_stdout(text);
    }
}

/// A formatted log line: above the prompt while the editor runs on the terminal standard error
/// shows, else on standard error.
pub fn write_log_line(line: &str) {
    let console = console();
    match &console.editor {
        Some(editor) if console.stderr_on_terminal => {
            write_stdout(&editor.print_above(line, columns()))
        }
        _ => {
            let mut err = std::io::stderr().lock();
            let _ = writeln!(err, "{line}");
        }
    }
}

/// Sets what a Ctrl-C typed at the prompt does (raw mode turns it into a key, so the process's
/// Ctrl-C handler does not see it). Without one, the terminal is given back and the process exits.
pub fn set_interrupt_handler(handler: impl Fn() + Send + Sync + 'static) {
    *INTERRUPT.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(handler));
}

/// Ends the editor and gives the terminal back (the prompt row is cleared). Safe from any thread,
/// more than once; the server calls it before it exits.
pub fn stop() {
    STOPPED.store(true, Ordering::SeqCst);
    let mut console = console();
    if let Some(editor) = console.editor.take() {
        write_stdout(&editor.clear(columns()));
        let _ = terminal::disable_raw_mode();
    }
}

/// Whether the line editor can run: standard input and output are terminals that take the escape
/// sequences it writes (on Windows this turns the console's VT processing on).
#[must_use]
pub fn editor_available() -> bool {
    if STOPPED.load(Ordering::SeqCst)
        || !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal()
    {
        return false;
    }
    #[cfg(windows)]
    {
        crossterm::ansi_support::supports_ansi()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Why [`run_editor`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEnd {
    /// End of input: Ctrl-D, or Ctrl-Z, on an empty line.
    EndOfInput,
    /// [`stop`] was called, or `on_line` returned false.
    Stopped,
    /// The terminal could not be read.
    Failed,
}

/// Reads lines at `prompt` with the line editor until the input ends, [`stop`] is called, or
/// `on_line` (given each entered line) returns false. The caller checks [`editor_available`].
pub fn run_editor(prompt: &str, mut on_line: impl FnMut(String) -> bool) -> EditorEnd {
    if terminal::enable_raw_mode().is_err() {
        return EditorEnd::Failed;
    }
    {
        let mut console = console();
        if STOPPED.load(Ordering::SeqCst) {
            let _ = terminal::disable_raw_mode();
            return EditorEnd::Stopped;
        }
        let editor = PromptLine::new(prompt);
        write_stdout(&editor.draw(columns()));
        console.editor = Some(editor);
        console.stderr_on_terminal = std::io::stderr().is_terminal();
    }

    let end = loop {
        match event::poll(Duration::from_millis(100)) {
            Ok(false) => {
                if STOPPED.load(Ordering::SeqCst) {
                    break EditorEnd::Stopped;
                }
                continue;
            }
            Ok(true) => {}
            Err(_) => break EditorEnd::Failed,
        }
        let key = match event::read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => key,
            Ok(Event::Resize(..)) => {
                let console = console();
                if let Some(editor) = &console.editor {
                    write_stdout(&format!("\r\x1b[J{}", editor.draw(columns())));
                }
                continue;
            }
            Ok(_) => continue,
            Err(_) => break EditorEnd::Failed,
        };

        let action = {
            let mut console = console();
            let Some(editor) = console.editor.as_mut() else {
                break EditorEnd::Stopped;
            };
            let cols = columns();
            let before = editor.clear(cols);
            let typed = editor.line();
            let action = editor.key(key);
            let after = match &action {
                KeyAction::Line(line) => format!("{}\r\n{}", editor.echo(line), editor.draw(cols)),
                KeyAction::Interrupt => {
                    format!("{}^C\r\n{}", editor.echo(&typed), editor.draw(cols))
                }
                KeyAction::EndOfInput => editor.echo(""),
                KeyAction::Edited => editor.draw(cols),
                KeyAction::Ignored => String::new(),
            };
            if !matches!(action, KeyAction::Ignored) {
                write_stdout(&format!("{before}{after}"));
            }
            action
        };
        match action {
            KeyAction::Line(line) => {
                if !on_line(line) {
                    break EditorEnd::Stopped;
                }
            }
            KeyAction::Interrupt => interrupt(),
            KeyAction::EndOfInput => break EditorEnd::EndOfInput,
            KeyAction::Edited | KeyAction::Ignored => {}
        }
    };

    let mut console = console();
    if let Some(editor) = console.editor.take() {
        if end == EditorEnd::EndOfInput {
            write_stdout("\r\n");
        } else {
            write_stdout(&editor.clear(columns()));
        }
        let _ = terminal::disable_raw_mode();
    }
    end
}

/// A Ctrl-C at the prompt: the handler, or (without one) the terminal back and exit.
fn interrupt() {
    let handler = INTERRUPT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(handler) = handler.as_ref() {
        handler();
    } else {
        drop(handler);
        stop();
        std::process::exit(130);
    }
}

/// What a key did to the prompt line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// Enter: the line typed (the prompt line is empty again).
    Line(String),
    /// Ctrl-C (the prompt line is empty again).
    Interrupt,
    /// Ctrl-D or Ctrl-Z on an empty line.
    EndOfInput,
    /// The line or the cursor changed.
    Edited,
    /// Nothing changed.
    Ignored,
}

/// The prompt, the half-typed line and the cursor in it, and the lines entered so far (Up and
/// Down recall them). Every method that draws returns the text to write: the line's characters
/// and the escape sequences `ESC[nA` (up), `ESC[nG` (to a column) and `ESC[J` (clear to the end
/// of the screen), with `\r\n` line ends (raw mode does not add the `\r`).
///
/// The drawing keeps one rule: between writes the cursor is on the terminal at the cursor's place
/// in `prompt + line`, counted from the prompt's first column and wrapped at the terminal's width.
/// A character is one column.
#[derive(Debug, Clone, Default)]
pub struct PromptLine {
    prompt: String,
    line: Vec<char>,
    /// The cursor, as an index into `line`.
    cursor: usize,
    history: Vec<String>,
    /// The history entry shown (Up/Down), and the typed line it replaced.
    recalled: Option<(usize, Vec<char>)>,
}

impl PromptLine {
    /// An empty line at `prompt`.
    #[must_use]
    pub fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.to_owned(),
            ..Self::default()
        }
    }

    /// The half-typed line.
    #[must_use]
    pub fn line(&self) -> String {
        self.line.iter().collect()
    }

    /// The cursor's index in [`Self::line`].
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn prompt_width(&self) -> usize {
        self.prompt.chars().count()
    }

    /// Draws the prompt and the line from the cursor's column 0 on the prompt's first row, and
    /// leaves the cursor at its place.
    #[must_use]
    pub fn draw(&self, cols: usize) -> String {
        let cols = cols.max(1);
        let mut out = String::with_capacity(self.prompt.len() + self.line.len() + 16);
        out.push_str(&self.prompt);
        out.extend(self.line.iter());
        let end = self.prompt_width() + self.line.len();
        // A line that fills its last row leaves the terminal's cursor waiting at the last column;
        // a line end puts it at the start of the next row, where the arithmetic below expects it.
        if end > 0 && end.is_multiple_of(cols) {
            out.push_str("\r\n");
        }
        let at = self.prompt_width() + self.cursor;
        let up = end / cols - at / cols;
        if up > 0 {
            out.push_str(&format!("\x1b[{up}A"));
        }
        out.push_str(&format!("\x1b[{}G", at % cols + 1));
        out
    }

    /// Clears the rows of a prompt drawn at `cols` columns (the cursor is at its place, as
    /// [`Self::draw`] leaves it), leaving the cursor at column 0 of the prompt's first row.
    #[must_use]
    pub fn clear(&self, cols: usize) -> String {
        let cols = cols.max(1);
        let row = (self.prompt_width() + self.cursor) / cols;
        if row > 0 {
            format!("\r\x1b[{row}A\x1b[J")
        } else {
            "\r\x1b[J".to_owned()
        }
    }

    /// Clears the prompt rows, writes `text` (each of its lines ends with `\r\n`), and draws the
    /// prompt and the half-typed line again below it with the cursor where it was.
    #[must_use]
    pub fn print_above(&self, text: &str, cols: usize) -> String {
        let mut out = self.clear(cols);
        for line in text.split('\n') {
            out.push_str(line.strip_suffix('\r').unwrap_or(line));
            out.push_str("\r\n");
        }
        out.push_str(&self.draw(cols));
        out
    }

    /// The prompt and an entered line, as they stay on the screen (drawn after a clear; the caller
    /// ends the row).
    fn echo(&self, line: &str) -> String {
        format!("{}{line}", self.prompt)
    }

    /// Applies a key to the line.
    pub fn key(&mut self, key: KeyEvent) -> KeyAction {
        // Control and Alt together are AltGr on Windows: a character such as `@` on some layouts.
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        let alt = key.modifiers.contains(KeyModifiers::ALT)
            && !key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => {
                let line: String = self.line.drain(..).collect();
                self.cursor = 0;
                self.recalled = None;
                if !line.trim().is_empty() && self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                }
                KeyAction::Line(line)
            }
            KeyCode::Char('c') if ctrl => {
                self.line.clear();
                self.cursor = 0;
                self.recalled = None;
                KeyAction::Interrupt
            }
            KeyCode::Char('d' | 'z') if ctrl && self.line.is_empty() => KeyAction::EndOfInput,
            KeyCode::Char('d') if ctrl => self.delete(),
            KeyCode::Char('a') if ctrl => self.move_to(0),
            KeyCode::Char('e') if ctrl => self.move_to(self.line.len()),
            KeyCode::Char('b') if ctrl => self.move_to(self.cursor.saturating_sub(1)),
            KeyCode::Char('f') if ctrl => self.move_to((self.cursor + 1).min(self.line.len())),
            KeyCode::Char('u') if ctrl => {
                if self.cursor == 0 {
                    return KeyAction::Ignored;
                }
                self.line.drain(..self.cursor);
                self.cursor = 0;
                KeyAction::Edited
            }
            KeyCode::Char('k') if ctrl => {
                if self.cursor == self.line.len() {
                    return KeyAction::Ignored;
                }
                self.line.truncate(self.cursor);
                KeyAction::Edited
            }
            KeyCode::Char('p') if ctrl => self.recall_older(),
            KeyCode::Char('n') if ctrl => self.recall_newer(),
            KeyCode::Char(c) if !ctrl && !alt && !c.is_control() => {
                self.line.insert(self.cursor, c);
                self.cursor += 1;
                KeyAction::Edited
            }
            KeyCode::Backspace => {
                if self.cursor == 0 {
                    return KeyAction::Ignored;
                }
                self.cursor -= 1;
                self.line.remove(self.cursor);
                KeyAction::Edited
            }
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_to(self.cursor.saturating_sub(1)),
            KeyCode::Right => self.move_to((self.cursor + 1).min(self.line.len())),
            KeyCode::Home => self.move_to(0),
            KeyCode::End => self.move_to(self.line.len()),
            KeyCode::Up => self.recall_older(),
            KeyCode::Down => self.recall_newer(),
            KeyCode::Esc => {
                if self.line.is_empty() {
                    return KeyAction::Ignored;
                }
                self.line.clear();
                self.cursor = 0;
                self.recalled = None;
                KeyAction::Edited
            }
            _ => KeyAction::Ignored,
        }
    }

    fn delete(&mut self) -> KeyAction {
        if self.cursor >= self.line.len() {
            return KeyAction::Ignored;
        }
        self.line.remove(self.cursor);
        KeyAction::Edited
    }

    fn move_to(&mut self, cursor: usize) -> KeyAction {
        if cursor == self.cursor {
            return KeyAction::Ignored;
        }
        self.cursor = cursor;
        KeyAction::Edited
    }

    fn show(&mut self, line: Vec<char>) -> KeyAction {
        self.cursor = line.len();
        self.line = line;
        KeyAction::Edited
    }

    fn recall_older(&mut self) -> KeyAction {
        let index = match &self.recalled {
            Some((0, _)) => return KeyAction::Ignored,
            Some((i, _)) => i - 1,
            None if self.history.is_empty() => return KeyAction::Ignored,
            None => {
                self.recalled = Some((self.history.len(), self.line.clone()));
                self.history.len() - 1
            }
        };
        if let Some((i, _)) = self.recalled.as_mut() {
            *i = index;
        }
        let line = self.history[index].chars().collect();
        self.show(line)
    }

    fn recall_newer(&mut self) -> KeyAction {
        let Some((i, typed)) = self.recalled.take() else {
            return KeyAction::Ignored;
        };
        if i + 1 >= self.history.len() {
            return self.show(typed);
        }
        self.recalled = Some((i + 1, typed));
        let line = self.history[i + 1].chars().collect();
        self.show(line)
    }
}
