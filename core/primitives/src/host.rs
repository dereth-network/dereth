//! Host services the game logic needs but must not implement: text encoding and a text sink.
//!
//! The game model has no clock, no sockets, no threads and no file system of its own. Where the
//! original client reaches the operating system from inside that logic, the reach goes through one
//! of these traits instead, and the application implements them: [`HostEncoding`] for the ANSI
//! code-page conversions, [`TextSink`] for the chat log file. Tests implement both in memory.

/// The host's text conversion: the ANSI code page the original client narrows and widens through,
/// and the one English `%ws` field its `sprintf` emits.
///
/// Each of these is a `kernel32` NLS call in the retail client —
/// `WideCharToMultiByte(CP_ACP, 0, ...)` when narrowing a wide string, `MultiByteToWideChar`
/// when widening one, and MSVCR70's `wctomb` inside the `char sprintf("%ws")` that composes a
/// chat line. Behind this trait, the chat model depends on no platform crate.
///
/// `None` is the conversion that could not be performed at all — an unsupported code-page selector
/// or a count the platform API cannot take. It is *not* the substitution case: a unit with no byte
/// in the target code page is the implementation's own business (the original escapes the whole
/// string as `<%04x>`), and the caller only ever sees the bytes that resulted.
pub trait HostEncoding: std::fmt::Debug + Send + Sync {
    /// UTF-16 to the host's ANSI bytes, terminator excluded.
    fn narrow(&self, utf16: &[u16]) -> Option<Vec<u8>>;

    /// The host's ANSI bytes back to UTF-16, terminator excluded.
    fn widen(&self, bytes: &[u8]) -> Option<Vec<u16>>;

    /// The code page the two above use. `0` is the host's live `CP_ACP`, as the API spells it.
    fn acp(&self) -> u32;

    /// One `%ws` field of the main client's `char sprintf`, after start-up has run
    /// `setlocale("English")`: MSVCR70's `_output` loop converts unit by unit with `wctomb` and
    /// **stops this field** at the first unit it cannot convert, keeping the bytes before it and
    /// resuming the enclosing format.
    ///
    /// It is a fourth method rather than a use of [`narrow`] because it is neither the same code
    /// page (`English_United States.1252`, never `CP_ACP`) nor the same failure rule (this field
    /// truncates; `narrow` escapes the whole string).
    ///
    /// [`narrow`]: HostEncoding::narrow
    fn english_ws_field(&self, utf16: &[u16]) -> Option<Vec<u8>>;
}

/// Where a line of text the client has already composed is copied to.
///
/// The client appends each scroll line to an open log file with a plain
/// `fprintf("%ls%ls\n", prefix, body)`, on the handle that the "copy output to file" command
/// opens. The host owns the file; the model owns the decision to write a line.
///
/// The line arrives **without a terminator** and as a Rust `str`: the byte encoding on disk and
/// the CRLF that MSVCR70 text mode appends are the sink's, because both are host facts.
pub trait TextSink: std::fmt::Debug + Send {
    fn write_line(&mut self, line: &str);
}
