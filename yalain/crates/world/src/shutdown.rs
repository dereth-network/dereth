//! The world shutting down: the countdown every player in the world is shown before the server
//! stops.
//!
//! As the 2017 worlds did at their shutdowns: five lines of the server's text, thirty seconds
//! apart, from two minutes before ("This world will be shutting down in 2 minutes. Please log
//! out.", then 1 minute and 30 seconds, 1 minute, 30 seconds) to "This world is shutting down
//! NOW! Log out!", each to every player in the world at that moment; the minutes and seconds are
//! the text's variables, not written into it. The world goes on twenty seconds after the last line
//! (2017's last messages came 20 to 27 seconds after it), then the server stops.
//!
//! However the server stops, every character still in the world is kept first, as when its client
//! goes away ([`save_everyone`]).

use auberean_protocol::login::Notice;
use auberean_protocol::text::{StringInfo, Variable, VariableValue};
use auberean_protocol::{queue, ServerMessage};
use yalain_net::{BlobKind, ServerNet};
use yalain_store::Store;

use crate::{Clock, Server, Timer};

/// The kind of server text the countdown is shown as.
pub const KIND: u32 = 4;
/// The table of the countdown's lines.
pub const TABLE: u32 = 0x2500_0626;
/// "This world will be shutting down in N minutes. Please log out."
pub const MINUTES: u32 = 0xA4D9_0AFD;
/// "This world will be shutting down in N minutes and N seconds. Please log out."
pub const MINUTES_AND_SECONDS: u32 = 0x6D96_0423;
/// "This world will be shutting down in N seconds! Please log out!"
pub const SECONDS: u32 = 0x455A_4AFC;
/// "This world is shutting down NOW! Log out!"
pub const NOW: u32 = 0x09E2_96F6;
/// The variables: the minutes, the seconds.
pub const MINUTES_VAR: u32 = 0x67A0_6DFD;
pub const SECONDS_VAR: u32 = 0xA4A6_EDFE;
/// The kind of a variable holding a number.
const NUMBER: u16 = 2;

/// Seconds between the countdown's lines, and before the end its first comes.
pub const STEP: u32 = 30;
pub const LEAD: u32 = 120;
/// Seconds the world goes on after the last line.
pub const AFTER_NOW: f64 = 20.0;

/// Where the world is in shutting down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Shutdown {
    /// Running.
    #[default]
    No,
    /// Counting down.
    Counting,
    /// The last line is sent; the world goes on a little.
    Ending,
    /// The server may stop.
    Done,
}

/// The line shown with this many seconds left.
#[must_use]
pub fn line(left: u32) -> StringInfo {
    let number = |id, n| Variable {
        id,
        kind: NUMBER,
        value: VariableValue::Word(n),
    };
    let (minutes, seconds) = (left / 60, left % 60);
    let (string_id, variables) = match (minutes, seconds) {
        (0, 0) => (NOW, Vec::new()),
        (0, s) => (SECONDS, vec![number(SECONDS_VAR, s)]),
        (m, 0) => (MINUTES, vec![number(MINUTES_VAR, m)]),
        (m, s) => (
            MINUTES_AND_SECONDS,
            vec![number(MINUTES_VAR, m), number(SECONDS_VAR, s)],
        ),
    };
    StringInfo {
        string_id,
        table_id: TABLE,
        variable_count: i16::try_from(variables.len()).unwrap_or(0),
        variables,
        ..StringInfo::default()
    }
}

/// Begin the countdown (nothing if it is already running or done).
pub fn begin<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.shutdown != Shutdown::No {
        return;
    }
    s.shutdown = Shutdown::Counting;
    tracing::info!("the world shuts down in {LEAD} seconds");
    step(s, net, LEAD, clock);
}

/// Show the line for `left` seconds to everyone in the world, and have the next one follow.
pub(crate) fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, left: u32, clock: Clock) {
    let m = ServerMessage::Notice(Notice {
        kind: KIND,
        text: line(left),
    });
    let bytes = m.encode();
    let mut worlds: Vec<_> = s.players.keys().copied().collect();
    worlds.sort();
    for world in worlds {
        net.send(world, queue::EVENT, BlobKind::UNORDERED, bytes.clone());
    }
    if left == 0 {
        s.shutdown = Shutdown::Ending;
        tracing::info!("the world is shutting down");
        s.after(clock, AFTER_NOW, Timer::ShutdownEnds);
        return;
    }
    s.after(
        clock,
        f64::from(STEP),
        Timer::Shutdown {
            left: left.saturating_sub(STEP),
        },
    );
}

/// The world has gone on its last seconds.
pub(crate) fn ends<S: Store>(s: &mut Server<S>) {
    s.shutdown = Shutdown::Done;
}

/// The server is stopping, at the countdown's end or at once: every character still in the world
/// leaves it as one whose client went away does, and is kept, so nothing it did is lost. Returns
/// how many left.
pub fn save_everyone<S: Store>(s: &mut Server<S>, net: &mut ServerNet) -> usize {
    let mut worlds: Vec<_> = s.players.keys().copied().collect();
    worlds.sort();
    for world in &worlds {
        crate::world::leave(s, net, *world);
    }
    worlds.len()
}
