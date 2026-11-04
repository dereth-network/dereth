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
//! The operator's console, as Empyrean's: the countdown's length is the operator's
//! ([`crate::ServerConfig::shutdown_seconds`], two minutes unless given another, 0 to
//! [`MAX_SECONDS`]); a shutdown may carry the operator's message, shown to every player in the
//! world as it begins; a countdown may be cancelled, and every player in the world is told; and
//! the server may be stopped now. A countdown longer than two minutes shows its first line at
//! once, then one at each of Empyrean's marks (two hours, an hour, 45, 30, 15, 10 and 5 minutes)
//! still ahead, and the 2017 lines from two minutes on.
//!
//! However the server stops, every character still in the world is kept first, as when its client
//! goes away ([`save_everyone`]).

use auberean_protocol::login::{CharacterRefused, Notice};
use auberean_protocol::text::{StringInfo, Variable, VariableValue};
use auberean_protocol::{queue, ServerMessage};
use yalain_net::{BlobKind, LocalTime, ServerNet};
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

/// Seconds between the countdown's lines from two minutes on, and the countdown's length unless
/// the operator gives another.
pub const STEP: u32 = 30;
pub const LEAD: u32 = 120;
/// The longest countdown the operator may give (Empyrean's: five digits).
pub const MAX_SECONDS: u32 = 99_999;
/// The seconds left at which the countdown shows a line, after its first.
pub const MARKS: [u32; 12] = [7200, 3600, 2700, 1800, 900, 600, 300, 120, 90, 60, 30, 0];
/// What every player in the world is told when the countdown is cancelled.
pub const CANCELLED: &str = "This world is no longer shutting down.";
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

/// The seconds left at the countdown's next line after one with `left` seconds left.
#[must_use]
pub fn next_mark(left: u32) -> Option<u32> {
    MARKS.into_iter().find(|m| *m < left)
}

/// Begin the countdown, as long as the operator's setting says, showing the operator's message
/// first if there is one. Nothing if a countdown is already running or done; returns whether it
/// began.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    message: Option<&str>,
) -> bool {
    if s.shutdown != Shutdown::No {
        return false;
    }
    let left = s.cfg.shutdown_seconds.min(MAX_SECONDS);
    count_down(s, net, clock, left, message);
    true
}

/// Begin the countdown an update warns players with, `seconds` long from now on, as an operator who
/// set the interval and began a shutdown would. Nothing if a countdown is already running or done;
/// returns whether it began.
pub fn begin_warning<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    seconds: u32,
) -> bool {
    if s.shutdown != Shutdown::No {
        return false;
    }
    s.cfg.shutdown_seconds = seconds.min(MAX_SECONDS);
    begin(s, net, clock, None)
}

/// Show the operator's message, if there is one, and the countdown's first line, with `left`
/// seconds left.
fn count_down<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    left: u32,
    message: Option<&str>,
) {
    s.shutdown = Shutdown::Counting;
    tracing::info!("the world shuts down in {left} seconds");
    if let Some(text) = message.map(str::trim).filter(|t| !t.is_empty()) {
        tracing::info!("the operator's message: {text}");
        for world in in_world(s) {
            crate::login::notice(net, world, text);
        }
    }
    step(s, net, left, clock);
}

/// Cancel a countdown that is running, and tell every player in the world. Nothing once the last
/// line is shown; returns whether it was cancelled.
pub fn cancel<S: Store>(s: &mut Server<S>, net: &mut ServerNet) -> bool {
    if s.shutdown != Shutdown::Counting {
        return false;
    }
    s.timers
        .retain(|(_, t)| !matches!(t, Timer::Shutdown { .. } | Timer::ShutdownEnds));
    s.shutdown = Shutdown::No;
    tracing::info!("the shutdown is cancelled");
    for world in in_world(s) {
        crate::login::notice(net, world, CANCELLED);
    }
    true
}

/// Stop the server now: every player in the world is shown the operator's message, if there is
/// one, and the last line, and the server may stop at once, whether a countdown was running or
/// not. Nothing once the server may stop; returns whether it was stopped.
pub fn stop_now<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    message: Option<&str>,
) -> bool {
    if s.shutdown == Shutdown::Done {
        return false;
    }
    count_down(s, net, clock, 0, message);
    s.timers
        .retain(|(_, t)| !matches!(t, Timer::Shutdown { .. } | Timer::ShutdownEnds));
    s.shutdown = Shutdown::Done;
    true
}

/// The world connections of the players in the world, in order.
fn in_world<S: Store>(s: &Server<S>) -> Vec<yalain_net::ConnId> {
    let mut worlds: Vec<_> = s.players.keys().copied().collect();
    worlds.sort();
    worlds
}

/// Show the line for `left` seconds to everyone in the world, and have the next one follow.
pub(crate) fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, left: u32, clock: Clock) {
    let m = ServerMessage::Notice(Notice {
        kind: KIND,
        text: line(left),
    });
    let bytes = m.encode();
    for world in in_world(s) {
        net.send(world, queue::EVENT, BlobKind::UNORDERED, bytes.clone());
    }
    let Some(next) = next_mark(left) else {
        s.shutdown = Shutdown::Ending;
        tracing::info!("the world is shutting down");
        s.after(clock, AFTER_NOW, Timer::ShutdownEnds);
        return;
    };
    s.after(
        clock,
        f64::from(left - next),
        Timer::Shutdown { left: next },
    );
}

/// The world has gone on its last seconds.
pub(crate) fn ends<S: Store>(s: &mut Server<S>) {
    s.shutdown = Shutdown::Done;
}

/// The server is stopping, at the countdown's end or at once: every character still in the world
/// leaves it as one whose client went away does, and is kept, so nothing it did is lost. Returns
/// how many left. What the store did not take stays held ([`crate::saves::write_out`] tries again).
pub fn save_everyone<S: Store>(s: &mut Server<S>, net: &mut ServerNet) -> usize {
    if s.panicked.is_some() {
        return 0;
    }
    let worlds = in_world(s);
    for world in &worlds {
        crate::world::leave(s, net, *world);
    }
    let _ = crate::saves::write_out(s);
    worlds.len()
}

/// The server is stopping on its operator's word: everyone is kept ([`save_everyone`]) and let go,
/// each client told it was logged off. Returns how many left the world. A world that panicked has
/// let go of everyone already.
pub fn stopping<S: Store>(s: &mut Server<S>, net: &mut ServerNet, now: LocalTime) -> usize {
    let left = save_everyone(s, net);
    if s.panicked.is_none() {
        crate::login::let_everyone_go(s, net, CharacterRefused::LOGGED_OFF, now);
    }
    left
}
