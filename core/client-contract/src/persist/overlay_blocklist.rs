//! The worlds whose data the player refuses: `overlay-blocklist.txt`, beside the preferences file,
//! one world's name a line. The client never takes those worlds' data, and a launcher that knows a
//! world's name can say so before it starts the client.

use std::collections::BTreeSet;

/// The blocklist's file name, beside the preferences file.
pub const FILE: &str = "overlay-blocklist.txt";

/// The blocklist's text, read: its lines, trimmed, with blank lines and `#` comments left out.
#[must_use]
pub fn parse(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}
