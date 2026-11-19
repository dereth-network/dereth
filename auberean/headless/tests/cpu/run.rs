//! Scripts run on the SDK's driver against a server double, on a fixed-step clock: what each
//! command does, and what the run tells.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

use auberean_client_sdk::contract::event::{SocialEvent, TradeEvent};
use auberean_client_sdk::plugins::{MemoryStorage, PluginHook, VmClock};
use auberean_client_sdk::protocol::ClientMessage;
use auberean_client_sdk::runtime::calls::function;
use auberean_client_sdk::{Driver, GameEvent, Phase};
use auberean_headless::plugins::{host, DirSource};

use crate::double::{played, runner, with, FRAME, FRAMES};
use crate::world_entry::{refused_script, script, PLAYER};

/// The entry into the world, and the log-off.
const ENTRY: u32 = 0x1002B;
const LOG_OFF: u32 = 0x10027;

/// The messages of a type the client sent, with the host clock's time of each.
fn sent(d: &Driver, mtype: u32) -> Vec<f64> {
    d.sent()
        .iter()
        .filter(|s| s.message.message_type() == mtype)
        .map(|s| s.at - 1.0)
        .collect()
}

/// The server's functions the client called itself, in order.
fn calls(d: &Driver) -> Vec<u32> {
    d.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::ServerCall(c) => Some(c.function.0),
            _ => None,
        })
        .collect()
}

/// A script logs in, enters the world as the character it names, tells where the player stands
/// and the view as text, dumps the session's events in order, logs off back to the list and
/// stops: the entry and the log-off sent once each.
///
/// Behaviour: session.headless.a-script-logs-in-enters-tells-the-view-and-logs-off
#[test]
fn a_script_logs_in_enters_tells_the_view_dumps_the_events_and_logs_off() {
    let (r, lines, _) = played(
        script(),
        "login\nenter Tester # by name\nposition\nsnapshot\ndump events\nlogoff\nquit\n",
    );
    let first = |text: &str| {
        lines
            .iter()
            .position(|l| l.contains(text))
            .unwrap_or_else(|| panic!("{text:?} in {lines:#?}"))
    };
    let order = [
        "entering the world as 0x2130000000000001",
        "in the world at 02980109",
        "position: 02980109 (0.00, 0.00, 0.00) facing 0",
        "phase: World",
        "player: 0x2130000000000001",
        "event Session(Phase(Connecting))",
        "event Session(Phase(Characters))",
        "event Session(Phase(Entering))",
        "event Session(Phase(World))",
        "logging off: 1 objects around, 0 in the inventory",
        "done",
    ];
    let at: Vec<usize> = order.iter().map(|t| first(t)).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "in order: {lines:#?}");
    assert_eq!(with(&lines, "  0x2130000000000001 Tester").len(), 1);
    assert_eq!(sent(r.driver(), ENTRY).len(), 1, "the entry once");
    assert_eq!(sent(r.driver(), LOG_OFF).len(), 1, "the log-off once");
    assert_eq!(r.phase(), Phase::Characters, "back at the list");
}

/// An entry the server refuses for the character still in the world is told, and asked again two
/// seconds later; the second goes in.
///
/// Behaviour: session.headless.an-entry-refused-is-asked-again-two-seconds-later
#[test]
fn an_entry_the_server_refuses_is_asked_again_two_seconds_later() {
    let (r, lines, _) = played(refused_script(), "enter 0x2130000000000001\nquit\n");
    assert_eq!(
        with(&lines, "the entry was refused (11); asking again").len(),
        1,
        "{lines:#?}"
    );
    assert_eq!(with(&lines, "entering the world as").len(), 2);
    assert_eq!(with(&lines, "in the world at 02980109").len(), 1);
    let entries = sent(r.driver(), ENTRY);
    assert_eq!(entries.len(), 2);
    let gap = entries[1] - entries[0];
    assert!(
        (2.0..2.6).contains(&gap),
        "asked again two seconds after the refusal: {entries:?}"
    );
}

/// `tick` runs its frames, `run` its seconds of the clock, and `at` waits until its seconds after
/// the entry; a command that takes no time runs in the same step as the one before it.
///
/// Behaviour: session.headless.tick-run-and-at-wait-their-frames-and-seconds
#[test]
fn tick_run_and_at_wait_their_frames_and_seconds() {
    let (_, _, clock) = played(script(), "tick 30\n");
    assert!((clock.now() - 30.0 * FRAME).abs() < 1e-9, "{}", clock.now());
    let (_, _, clock) = played(script(), "run 2\nposition\n");
    assert!(
        (2.0..2.0 + FRAME + 1e-9).contains(&clock.now()),
        "{}",
        clock.now()
    );
    let (_, lines, _) = played(script(), "enter\nat 4\nposition\nat 1\nposition\n");
    let told = with(&lines, "position:");
    assert!(told[0].trim_start().starts_with("4.0"), "{told:?}");
    assert!(
        told[1].trim_start().starts_with("4.0"),
        "an `at` already past waits for nothing: {told:?}"
    );
}

/// Without the player's scripts, a script's selections and uses are the client's own: `select
/// self` selects the player (the view says so) and `use` uses it, each with the server's call; a
/// selection by id and `select none` likewise; a line said is told.
///
/// Behaviour: session.headless.a-script-selects-uses-and-says-through-actions
#[test]
fn a_script_selects_uses_and_says_through_actions_alone() {
    let (mut r, mut port, mut clock) = runner(
        script(),
        "enter\nselect self\ntick 1\nuse\nselect none\nuse 0x2130000000000001\nsay hello # all\n\
         tick 1\n",
    );
    let mut selected = Vec::new();
    for _ in 0..FRAMES {
        if !r.step(&mut clock, &mut port).expect("a step") {
            break;
        }
        if let Some(w) = &r.view().world {
            selected.push(w.selection.current);
        }
    }
    assert!(
        selected.contains(&PLAYER),
        "the player selected: {selected:?}"
    );
    let made = calls(r.driver());
    assert_eq!(
        made,
        [
            function::SELECT,
            function::USE,
            function::SELECT,
            function::SELECT,
            function::USE
        ],
        "select, use, select nothing, select the player, use"
    );
    let lines = r.take_lines();
    assert_eq!(with(&lines, "saying: hello # all").len(), 1, "{lines:#?}");
}

/// The standing orders answer offers: with `agree on` a place in a fellowship and an oath are
/// taken, a trade asked by another is opened and accepted once the other side has; with `agree
/// off` nothing is; with `offer on` and nothing in the pack, nothing is offered.
///
/// Behaviour: session.headless.offers-are-answered-as-the-standing-orders-say
#[test]
fn offers_are_answered_as_the_standing_orders_say() {
    const OTHER: u64 = 0x2130_0000_0000_0002;
    let offers = [
        GameEvent::Social(SocialEvent::RecruitmentOffered {
            leader: OTHER,
            leader_name: "Other".into(),
            fellowship: "Ours".into(),
        }),
        GameEvent::Social(SocialEvent::OathOffered {
            vassal: OTHER,
            name: "Other".into(),
        }),
        GameEvent::Trade(TradeEvent::Registered {
            asker: OTHER,
            asked: PLAYER,
            partner: "Other".into(),
        }),
        GameEvent::Trade(TradeEvent::Opened),
        GameEvent::Trade(TradeEvent::Accepted { by: OTHER }),
        GameEvent::Trade(TradeEvent::Accepted { by: OTHER }),
    ];
    let (mut r, _, _) = runner(script(), "");
    for e in &offers {
        r.react(e);
    }
    assert!(r.take_lines().is_empty(), "no orders, no answers");
    r.line("agree on");
    r.line("offer on");
    for e in &offers {
        r.react(e);
    }
    let lines = r.take_lines();
    let answers: Vec<&str> = lines.iter().map(|l| l.trim_start()).collect();
    assert_eq!(
        answers,
        [
            "takes the place offered in a fellowship",
            "takes the oath offered",
            "opens the trade with 0x2130000000000002",
            "accepts the trade",
        ],
        "each answered once, the trade accepted once"
    );
    r.line("agree off");
    for e in &offers {
        r.react(e);
    }
    assert!(r.take_lines().is_empty());
}

/// A clock that stands still: no plugin's entry takes any time.
#[derive(Debug, Default)]
struct Still(Cell<Duration>);

impl VmClock for Still {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// A line typed runs at once when it takes no time (a plugin line, a standing order) and is told
/// so when it takes time; a script's plugin lines load the sample plugin and list it, and a line
/// that is none is told.
///
/// Behaviour: session.headless.typed-lines-run-at-once-and-scripts-carry-plugin-lines
#[test]
fn typed_lines_run_at_once_and_a_scripts_plugin_lines_reach_the_plugins() {
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/effects");
    let (mut r, mut port, mut clock) = runner(
        script(),
        &format!(
            "plugin load {}\ntick 1\nplugin list\nplugin dance\nenter\n",
            sample.display()
        ),
    );
    let h = host(
        Box::<Still>::default(),
        Box::<MemoryStorage>::default(),
        DirSource::default(),
    );
    r.driver_mut().install_hook(Box::new(PluginHook(h)));
    r.line("run 5");
    r.line("not a command");
    r.line("");
    let typed = r.take_lines();
    assert_eq!(typed.len(), 2, "{typed:?}");
    assert!(typed[0].contains("run 5: it takes time"));
    assert!(typed[1].contains("no command \"not\""));
    assert!(r
        .run_to_end(&mut clock, &mut port, FRAMES)
        .expect("the script runs"));
    let lines = r.take_lines();
    assert_eq!(
        with(&lines, "plugins: Effects is loaded.").len(),
        1,
        "{lines:#?}"
    );
    assert_eq!(with(&lines, "example.effects 0.1.0 running").len(), 1);
    assert_eq!(with(&lines, "not a plugin line: plugin dance").len(), 1);
    r.line("plugin unload example.effects");
    assert!(r.take_lines().is_empty());
}
