//! What a session exchanged, by opcode, in words.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::down::Count;
use crate::relay::Finished;

fn section(out: &mut String, title: &str, counts: &BTreeMap<u32, Count>) {
    let messages: u64 = counts.values().map(|c| c.messages).sum();
    let bytes: u64 = counts.values().map(|c| c.bytes).sum();
    let _ = writeln!(out, "{title}: {messages} message(s), {bytes} byte(s)");
    for (opcode, c) in counts {
        let _ = writeln!(out, "  {opcode:#06X} x{} {} B", c.messages, c.bytes);
    }
}

/// The report of one finished session.
#[must_use]
pub fn text(f: &Finished) -> String {
    let mut out = String::new();
    section(&mut out, "from the server, translated", &f.upstream.taken);
    section(
        &mut out,
        "from the server, dropped by policy",
        &f.upstream.dropped,
    );
    section(
        &mut out,
        "from the server, not translated yet",
        &f.upstream.unknown,
    );
    section(
        &mut out,
        "from the server, did not read",
        &f.upstream.malformed,
    );
    section(&mut out, "to Dereth", &f.delivered);
    section(&mut out, "from Dereth, forwarded", &f.forwarded);
    section(&mut out, "from Dereth, refused or dropped", &f.refused);
    out
}
