//! What has no counterpart in this server: each of the original's internals is named in the answer,
//! so an admin knows the command arrived and why nothing came of it.

use yalain_store::Store;

use super::Cx;

/// What a call with nothing behind it reports on, by its function id's low word.
const NAMES: &[(&str, &[u32])] = &[
    (
        "script machine",
        &[0x38, 0x39, 0x33, 0x56, 0x71, 0x82, 0x12E, 0x69, 0x5E],
    ),
    ("data miner", &[0x0E, 0x13, 0x1C, 0x28, 0x34, 0x3E, 0x49]),
    ("action locks", &[0x1D, 0x5B]),
    ("database locks", &[0x123, 0x124]),
    ("persistent chat rooms", &[0x18, 0x72]),
    ("path planner", &[0x12, 0x3A, 0x3C, 0x53, 0x66, 0x80]),
    ("heartbeat", &[0x68]),
    ("server parameters", &[0x2B]),
    ("sliders", &[0x01]),
    ("luck", &[0x35]),
    ("upkeep", &[0x1A]),
    ("housekeeping", &[0x36]),
    ("appearance mutations", &[0x4C]),
    ("entity links", &[0x131]),
    ("pending messages", &[0x45]),
    ("detector lists", &[0x89]),
    ("internal floats", &[0x115, 0x11C, 0x129]),
    ("named test packings", &[0x03, 0x8D]),
    ("combat time scale", &[0x29]),
    ("creature debugging", &[0x06, 0x08, 0x0E]),
    ("animation tests by name", &[0x5C, 0x86]),
    ("move-to tests", &[0x42]),
    ("drudge flag", &[0x136]),
    ("per-skill dumps", &[0x15]),
];

/// A call with nothing behind it: an answer saying so.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32) -> Vec<String> {
    let low = f & 0xFFFF;
    let what = NAMES
        .iter()
        .find(|(_, lows)| lows.contains(&low))
        .map_or("thing like that", |(name, _)| name);
    cx.s.unhandled
        .note(format!("admin call {f:#010x}, which has no place here"));
    vec![format!("This server has no {what}.")]
}
