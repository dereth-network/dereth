//! The fixed form of the log lines a program reads: a program that drives the server through its
//! console (a world builder's playtest, a launcher) finds each command's answer, and each
//! character's entering and leaving, by a tag at the end of the line that does not change when
//! the words before it do.
//!
//! ```text
//! <what was done> [ok <command>]
//! <why nothing was done> [refused <command>]
//! in the world at <cell> ... [entered "<name>" <id>]
//! left the world at <cell> [left "<name>" <id>]
//! listening on ... [listening]
//! binding the ports: ... [ports-taken]
//! ```
//!
//! `<command>` is the command's own name (`stop` and `exit` answer as `stop-now`), `<id>` the
//! character's id in hexadecimal (`0x` and small letters), and `<name>` its name, which never holds
//! a double quote or a bracket. The tag is the line's last ` [`, to the `]` that ends it ([`read`]).
//! Every such line comes from [`TARGET`]. The words before the tag are for people, and are one
//! line.

/// Where every line of this form comes from in the log.
pub const TARGET: &str = "yalain";

/// The text as one line.
fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

/// A command did what `text` says.
#[must_use]
pub fn ok(command: &str, text: &str) -> String {
    format!("{} [ok {command}]", one_line(text))
}

/// A command did nothing, for the reason `text` gives.
#[must_use]
pub fn refused(command: &str, text: &str) -> String {
    format!("{} [refused {command}]", one_line(text))
}

/// A character entered the world: `text` says where.
#[must_use]
pub fn entered(name: &str, id: u64, text: &str) -> String {
    format!("{} [entered \"{name}\" {id:#x}]", one_line(text))
}

/// A character left the world: `text` says where.
#[must_use]
pub fn left(name: &str, id: u64, text: &str) -> String {
    format!("{} [left \"{name}\" {id:#x}]", one_line(text))
}

/// Another program holds a port the server listens on, so it stops before it starts (with its
/// own exit code): `text` says which.
#[must_use]
pub fn ports_taken(text: &str) -> String {
    format!("{} [ports-taken]", one_line(text))
}

/// The server listens for clients: `text` says where.
#[must_use]
pub fn listening(text: &str) -> String {
    format!("{} [listening]", one_line(text))
}

/// A line's tag (between its last ` [` and the `]` that ends it) and the words before it; `None`
/// for a line with no tag.
#[must_use]
pub fn read(line: &str) -> Option<(&str, &str)> {
    let inner = line.trim_end().strip_suffix(']')?;
    let at = inner.rfind(" [")?;
    Some((&inner[at + 2..], &inner[..at]))
}
