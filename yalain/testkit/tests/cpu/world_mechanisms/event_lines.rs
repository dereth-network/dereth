//! Lines said as an event starts: a person's broadcast and its saying, and a notice every player
//! in the world reads.

use auberean_protocol::ServerMessage;
use yalain_content::events::{Event, EventTable, LineGate, Window};
use yalain_content::talk::{How, Line, When, Words};
use yalain_testkit::Harness;

use super::common::{object, run_to_unix, OBJECT, START, TEMPLATE};

const TABLE: u32 = 0x2500_28FF;
const BROADCAST_LINE: u32 = 0x6D11_60C7;
const SAYING_LINE: u32 = 0x8CA3_8999;
const NOTICE_LINE: u32 = 0x3CEE_FC01;
const USE_LINE: u32 = 0xBC23_12A7;

fn line(how: How, when: When, string: u32) -> Line {
    Line {
        who: TEMPLATE,
        object: None,
        words: Words::Ref(string, TABLE),
        role: None,
        how,
        when,
        quest: None,
        update: None,
        phase: None,
        item: None,
        order: 0,
        measured: false,
        placed: "authored for a test".into(),
    }
}

/// The object standing beside the players, an event `countdown` on from [`START`], and lines of
/// the object's template gated on it: a broadcast, a saying and a notice said as it starts, and a
/// line said at a use; two players in the world.
fn countdown() -> (Harness, usize, usize) {
    let mut h = Harness::new();
    h.server.content.objects = vec![object()];
    let gate = || LineGate {
        event: "countdown".into(),
        on: true,
    };
    h.server.content.events = EventTable {
        events: vec![Event {
            key: "countdown".into(),
            game_event: None,
            windows: vec![Window::Dated {
                from: START,
                until: START + 60,
            }],
            on_players: Vec::new(),
            failure: None,
            hides: Vec::new(),
            ended_by: Vec::new(),
        }],
        camps: Vec::new(),
        objects: Vec::new(),
        steps: Vec::new(),
        lines: vec![
            (line(How::Broadcast, When::Event, BROADCAST_LINE), gate()),
            (line(How::Say, When::Event, SAYING_LINE), gate()),
            (line(How::Notice, When::Event, NOTICE_LINE), gate()),
            (line(How::Tell, When::Use, USE_LINE), gate()),
        ],
    };
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    (h, a, b)
}

/// The object's calls of a function a client was sent since `from`.
fn calls(h: &Harness, bot: usize, from: usize, function: u32) -> Vec<u32> {
    h.world_messages(bot, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CellObjectCall(c)
                if c.object.id == OBJECT && c.call.function.0 == function =>
            {
                Some(c.call.function.0)
            }
            _ => None,
        })
        .collect()
}

fn notices(h: &Harness, bot: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(bot, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Notice(n) => Some((n.kind, n.text.string_id, n.text.table_id)),
            _ => None,
        })
        .collect()
}

/// As the event starts each player hears the object's broadcast once and its saying once, and
/// reads the notice (kind 256, the line's string alone); nothing is said before it starts or again
/// while it runs, and the line for a use is not said.
/// Behaviour: server.talk.lines-said-as-an-event-starts
#[test]
fn an_events_lines_are_said_as_it_starts() {
    let (mut h, a, b) = countdown();
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    run_to_unix(&mut h, START - 5);
    assert!(
        notices(&h, a, from_a).is_empty(),
        "nothing before it starts"
    );
    run_to_unix(&mut h, START + 30);
    for (bot, from) in [(a, from_a), (b, from_b)] {
        assert_eq!(
            notices(&h, bot, from),
            [(256, NOTICE_LINE, TABLE)],
            "the notice, once"
        );
        assert_eq!(calls(&h, bot, from, 0x8205_001F).len(), 1, "one broadcast");
        assert_eq!(calls(&h, bot, from, 0x8205_0016).len(), 1, "one saying");
    }
}
