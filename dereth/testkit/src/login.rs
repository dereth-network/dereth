//! Replaying a recorded login, which is how a scenario gets a client into the world.
//!
//! Both backends replay the same recording out of `fixtures/packet-captures` through the same reader
//! ([`dereth_client_net::client_session::testing::capture`]), and
//! neither opens a socket: the endpoint is [`crate::replay::recorded_endpoint`]'s, and only the
//! recording's server-to-client datagrams are fed. The recording's client-to-server half is read
//! for one field, the connection sequence number the authenticator carries.
//!
//! **The endpoint and the replay loop are [`crate::replay`]'s**, because scenarios other than a
//! login want the same two; what is here is the login goal -- the two states a replay stops at, and the
//! character selection in between -- which is the only part that is about logging in.
//!
//! The two backends' clocks are [`crate::replay`]'s module documentation.

use dereth_client::app::App;
use dereth_client_net::client_session::{SessionEvent, SessionState};
use dereth_primitives::LocalTime;

use crate::client::Model;
use crate::replay::{recorded_endpoint, records};
use dereth_client_net::client_session::testing::capture::peer;

/// The most frames one `App` login may run before it gives up, so a recording that never reaches
/// its goal ends with a panic rather than a hang.
const MAX_LOGIN_FRAMES: u64 = 4_000;

/// Which character the replay selects, and its identity in the shard's own set.
fn choose(
    set: &dereth_protocol::login::LoginCharacterSet,
    want: Option<&str>,
) -> (dereth_primitives::ObjectId, String) {
    let names: Vec<&str> = set.characters.iter().map(|c| c.name.as_str()).collect();
    let c = match want {
        Some(name) => set
            .characters
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| {
                panic!("the recorded character set names no {name:?}; it names {names:?}")
            }),
        None => set
            .characters
            .first()
            .unwrap_or_else(|| panic!("the recorded character set is empty")),
    };
    (c.gid, set.account.clone())
}

/// What a replayed login was offered.
#[derive(Debug, Clone, Default)]
pub(crate) struct LoggedIn {
    pub(crate) account: String,
    pub(crate) characters: Vec<String>,
}

fn offered(set: &dereth_protocol::login::LoginCharacterSet) -> LoggedIn {
    LoggedIn {
        account: set.account.clone(),
        characters: set.characters.iter().map(|c| c.name.clone()).collect(),
    }
}

/// Replay `session` into the model backend.
pub(crate) fn model_login(m: &mut Model, session: &str, character: Option<&str>) -> LoggedIn {
    let recs = records(session);
    let mut net = recorded_endpoint(&recs);
    let goal = if character.is_none() {
        SessionState::CharacterSelect
    } else {
        SessionState::Playable
    };
    let mut entered = false;
    let mut last_t = 0.0;

    for r in &recs {
        let now = LocalTime(r.t);
        last_t = r.t;
        if !r.c2s {
            net.feed(&r.raw, peer(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        let events = m.objects.pump(&mut net, now);
        let _chat = m.hud.apply_events_with_combat_mode_handler(
            &events,
            &mut m.objects.world,
            m.shell.as_mut().map(|s| &mut s.ui.ui.requests),
            &mut |_, _| {},
        );
        dereth_client::interaction::apply_events(&mut m.interaction, &events, &mut m.objects.world);
        for e in &events {
            if let SessionEvent::CharacterSet(set) = e {
                if !entered && character.is_some() {
                    let (gid, account) = choose(set, character);
                    net.enter_world(gid, &account);
                    entered = true;
                }
            }
        }
        if net.session_state() == goal {
            break;
        }
    }

    let state = net.session_state();
    assert_eq!(
        state, goal,
        "the replay of {session} ran the whole recording and reached {state:?}, not {goal:?}"
    );
    m.now = last_t;
    let out = offered(net.characters());
    m.net = Some(net);
    out
}

/// Replay `session` into a real `App`, one frame per datagram.
pub(crate) fn app_login(app: &mut App, session: &str, character: Option<&str>) -> LoggedIn {
    let recs = records(session);
    let net = recorded_endpoint(&recs);
    app.attach_replay_network(net)
        .unwrap_or_else(|_| panic!("this client already has a link; a scenario logs in once"));

    let goal = if character.is_none() {
        SessionState::CharacterSelect
    } else {
        SessionState::Playable
    };
    let mut entered = false;
    let mut frames = 0_u64;

    for r in recs.iter().filter(|r| !r.c2s) {
        // The recorded timestamps are not this client's clock. See the module docs.
        let now = LocalTime(app.clock().local_time);
        let from = peer(r.pair);
        app.replay_network_mut()
            .expect("the replay endpoint was attached above")
            .feed(&r.raw, from, now);
        assert!(
            app.frame(),
            "the client shut itself down during the login replay"
        );
        frames += 1;

        let state = app
            .replay_network_mut()
            .expect("the endpoint is attached")
            .session_state();
        if state == SessionState::CharacterSelect && !entered && character.is_some() {
            let net = app.replay_network_mut().expect("the endpoint is attached");
            let set = net.characters().clone();
            let (gid, account) = choose(&set, character);
            net.enter_world(gid, &account);
            entered = true;
        }
        if state == goal || frames >= MAX_LOGIN_FRAMES {
            break;
        }
    }

    let net = app.replay_network_mut().expect("the endpoint is attached");
    let state = net.session_state();
    let out = offered(net.characters());
    assert_eq!(
        state, goal,
        "the replay of {session} ran {frames} frames and reached {state:?}, not {goal:?}"
    );
    out
}
