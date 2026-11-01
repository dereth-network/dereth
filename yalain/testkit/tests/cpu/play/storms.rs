//! Portal storms: a crowded landblock is warned every 11 s, brewing, imminent, the storm sending
//! some of its players away, and subsided; a brewing storm the crowd thins out of is called off.
//! Fixture: the test world with its threshold made two players and its least one.

use auberean_protocol::ServerMessage;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::places::function as trip;
use yalain_world::storms::{intensity, TELEPORTED, TICK, WARNING};

const SHOW_TEXT: u32 = 0x8205_0011;

/// Three players: two in the test hall, a third moved to another landblock; the storms' threshold
/// two players, the least sent away one. Returns the harness and the clients.
fn crowd() -> (Harness, [usize; 3]) {
    let mut h = Harness::new();
    h.server.storms.threshold = 2;
    h.server.storms.least = 1;
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Bran");
    let (c, far) = h.player_in_world("three", "Cora");
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == far) {
        p.position.cell = 0x7000_0001;
        p.position.origin = [10.0, 10.0, 50.0];
    }
    h.run(5);
    (h, [a, b, c])
}

/// The warnings a client was sent since `from`: the cell and the intensity.
fn warnings(h: &Harness, b: usize, from: usize) -> Vec<(u32, f32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == WARNING => {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                match v.as_slice() {
                    [Value::Float(i), Value::Int(cell)] => Some((*cell, f32::from_bits(*i))),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// Run the world so many ticks of the storms.
fn ticks(h: &mut Harness, n: u32) {
    for _ in 0..n {
        for _ in 0..220 {
            h.step();
        }
    }
}

fn told_teleported(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from).iter().any(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => c
            .args
            .as_ref()
            .and_then(|x| calls::values(&x.0).ok())
            .is_some_and(|v| {
                v.iter().any(|x| {
                    matches!(x.root(), Some(Node::TextInfo(t)) if (t.string_id, t.table) == TELEPORTED)
                })
            }),
        _ => false,
    })
}

/// A landblock holding its threshold of players: brewing (0.4) once to each, with its own cell;
/// nothing for five ticks; imminent (0.6) at five ticks in a row; at the next the storm sends one
/// of them through portal space, told so; at the next the other is told it has subsided (0.0).
/// A player in another landblock hears nothing.
/// Behaviour: server.places.portal-storm
#[test]
fn a_crowded_landblock_brews_a_portal_storm_that_sends_players_away() {
    assert!((TICK - 11.0).abs() < f64::EPSILON);
    let (mut h, [a, b, c]) = crowd();
    let from = [a, b, c].map(|x| h.bots[x].inbox.len());
    // Until the first warning, then half a tick on: each tick after falls inside the next 220 steps.
    let mut n = 0;
    while warnings(&h, a, from[0]).is_empty() {
        h.step();
        n += 1;
        assert!(n < 1000, "a warning within a tick or two");
    }
    for _ in 0..110 {
        h.step();
    }
    let wa = warnings(&h, a, from[0]);
    assert_eq!(wa.len(), 1, "brewing once: {wa:?}");
    assert_eq!(wa[0].1, intensity::BREWING);
    let own = h
        .server
        .players
        .values()
        .find(|p| p.name == "Elara")
        .map(|p| p.position.cell);
    assert_eq!(Some(wa[0].0), own, "its own cell");
    assert_eq!(warnings(&h, b, from[1]).len(), 1);
    ticks(&mut h, 5);
    assert_eq!(warnings(&h, a, from[0]).len(), 1, "quiet while it brews");
    ticks(&mut h, 5);
    let imminent: Vec<f32> = warnings(&h, a, from[0]).iter().map(|w| w.1).collect();
    assert_eq!(
        imminent,
        [
            intensity::BREWING,
            intensity::IMMINENT,
            intensity::IMMINENT,
            intensity::IMMINENT,
            intensity::IMMINENT,
            intensity::IMMINENT
        ]
    );
    let mark = [a, b].map(|x| h.bots[x].inbox.len());
    ticks(&mut h, 1);
    let sent: Vec<usize> = [a, b]
        .into_iter()
        .zip(mark)
        .filter(|(x, m)| {
            told_teleported(&h, *x, *m)
                && h.world_messages(*x, *m).iter().any(|msg| {
                    matches!(msg, ServerMessage::PlayerCall(c) if c.function.0 == trip::ENTER_PORTAL_SPACE)
                })
        })
        .map(|(x, _)| x)
        .collect();
    assert_eq!(sent.len(), 1, "one sent away");
    let stayed = if sent[0] == a {
        (b, mark[1])
    } else {
        (a, mark[0])
    };
    ticks(&mut h, 1);
    let after: Vec<f32> = warnings(&h, stayed.0, stayed.1)
        .iter()
        .map(|w| w.1)
        .collect();
    assert_eq!(after, [intensity::SUBSIDED], "subsided, to the one left");
    assert!(warnings(&h, c, from[2]).is_empty(), "nothing elsewhere");
}

/// A brewing storm whose landblock falls below the threshold is called off: subsided (0.0) to those
/// left, and no storm after.
/// Behaviour: server.places.portal-storm-called-off
#[test]
fn a_brewing_storm_the_crowd_thins_out_of_is_called_off() {
    let (mut h, [a, b, _]) = crowd();
    let from = h.bots[a].inbox.len();
    while warnings(&h, a, from).is_empty() {
        h.step();
    }
    ticks(&mut h, 2);
    let leaving = h
        .server
        .players
        .values_mut()
        .find(|p| p.name == "Bran")
        .map(|p| p.character)
        .expect("Bran");
    if let Some(p) = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == leaving)
    {
        p.position.cell = 0x7100_0001;
    }
    let left_from = h.bots[b].inbox.len();
    ticks(&mut h, 14);
    let got: Vec<f32> = warnings(&h, a, from).iter().map(|w| w.1).collect();
    assert_eq!(got, [intensity::BREWING, intensity::SUBSIDED]);
    assert!(!told_teleported(&h, a, from), "no storm");
    assert!(
        warnings(&h, b, left_from).is_empty(),
        "the one gone hears nothing"
    );
}
