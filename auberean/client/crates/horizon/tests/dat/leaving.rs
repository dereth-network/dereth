//! Logging out under Horizon on a clock stepped a sixtieth of a second a frame, against a server
//! that lets the character go as the game's own servers did: frozen in its glow at once, its
//! leaving played at 15.2 s, into portal space at 18.9 s and out of the world at 19.0 s.

use auberean_client_sdk::protocol::login::Empty;
use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
use auberean_client_sdk::protocol::script::{Arg, ClientCall, FunctionId, ScriptArgs};
use auberean_client_sdk::protocol::types::{BehaviorParams, ObjectStamp};
use auberean_client_sdk::protocol::ServerMessage;
use auberean_horizon::ui::leaving;
use auberean_horizon::ui::pregame::Screen;
use auberean_shell::interface::Phase;
use auberean_testkit::double::script::Event as Step;

use crate::session::{c2s, character_list, s2c, Session, LOGIN, PLAYER, WORLD};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// When the recording's player logs off, seconds on its clock.
const ASKED: f64 = 10.0;

/// A call of one of the player's functions with whole numbers.
fn call(function: u32, args: &[u32]) -> ServerMessage {
    let args: Vec<Arg> = args.iter().map(|a| Arg::Int(*a)).collect();
    ServerMessage::PlayerCall(ClientCall {
        function: FunctionId(function),
        args: Some(ScriptArgs::from_values(&args).expect("whole numbers pack")),
    })
}

fn on_player<A>(action: A) -> ObjectEvent<A> {
    ObjectEvent {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        action,
    }
}

/// The log-off as the 2017 servers answered it, each step its time after the request.
fn log_off() -> Vec<Step> {
    let leaving = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR,
        behavior: 0x4100_0083,
        ..BehaviorParams::default()
    };
    vec![
        c2s(ASKED, 0, 4, 0x10027),
        s2c(ASKED + 0.10, 1, 1, WORLD | 7, &call(0x8212_0021, &[1])),
        s2c(
            ASKED + 0.12,
            1,
            1,
            WORLD | 8,
            &ServerMessage::DoFx(on_player(Fx {
                fx: 0x4100_0010,
                scalar: 1.0,
            })),
        ),
        s2c(
            ASKED + 15.2,
            1,
            1,
            WORLD | 9,
            &ServerMessage::PlayBehavior(on_player(leaving)),
        ),
        s2c(
            ASKED + 18.9,
            1,
            1,
            WORLD | 10,
            &call(0x8212_0020, &[4, 0, 0]),
        ),
        s2c(
            ASKED + 19.0,
            0,
            1,
            LOGIN | 4,
            &ServerMessage::CharExitGame(Empty),
        ),
        s2c(
            ASKED + 19.01,
            0,
            1,
            LOGIN | 5,
            &ServerMessage::CharacterList(character_list()),
        ),
    ]
}

/// One frame of the leaving.
#[derive(Clone, Copy, Debug)]
struct Moment {
    /// Seconds since Log Out was asked.
    at: f64,
    phase: Phase,
    waiting: bool,
    /// Every quad of Horizon's stands in the wait's box.
    only_the_box: bool,
    /// Horizon drew nothing at all.
    nothing: bool,
    portal: bool,
    world: bool,
    screen: Option<Screen>,
}

/// Logging out under Horizon: from Log Out until the session leaves the world the HUD is gone and
/// the box says the client is logging out, the world drawn behind it, for as long as the server
/// holds the character (19 s here, its leaving played on the way); portal space then shows, and
/// stays in sight with nothing over it for [`leaving::PORTAL_SEEN`] after the session has left the
/// world, before character select comes up over its view.
///
/// Behaviour: desktop.horizon.logging-out-shows-the-wait-then-portal-space-then-character-select
#[test]
fn logging_out_shows_the_wait_then_portal_space_long_enough_to_see_then_character_select() {
    let mut s = Session::going_on(Some(HORIZON), log_off());
    let entered = s.until(20.0, |s| {
        s.shell.phase() == Phase::World && !s.shell.scene.in_portal_space()
    });
    assert!(entered, "in the world: {:?}", s.shell.phase());
    s.until(0.5, |_| false);
    let hud = s.horizon().borrow().draw_list().quads.len();
    s.horizon().borrow_mut().log_out();
    let asked = s.shell.host.time;
    let (sw, sh) = (1920.0, 1080.0);
    let k = auberean_horizon::scale::layout_scale((sw, sh));
    let r = leaving::layout((sw, sh), k);
    let near = 24.0 * k;
    let mut moments: Vec<Moment> = Vec::new();
    while s.shell.host.time < asked + 30.0 {
        s.advance();
        let (world, portal) = {
            let out = s.shell.frame();
            (out.world.is_some(), out.portal.is_some())
        };
        let h = s.horizon();
        let h = h.borrow();
        let quads = &h.draw_list().quads;
        let m = Moment {
            at: s.shell.host.time - asked,
            phase: s.shell.phase(),
            waiting: h.logging_out(),
            only_the_box: quads.iter().all(|q| {
                q.dst.x >= r.x - near
                    && q.dst.y >= r.y - near
                    && q.dst.right() <= r.right() + near
                    && q.dst.bottom() <= r.bottom() + near
            }),
            nothing: quads.is_empty(),
            portal,
            world,
            screen: h.screen(),
        };
        moments.push(m);
        if m.screen == Some(Screen::Select) && !m.portal {
            break;
        }
    }
    let last = moments.last().copied().expect("frames");
    assert_eq!(last.screen, Some(Screen::Select), "{last:?}");
    let left = moments
        .iter()
        .position(|m| m.phase != Phase::World)
        .expect("the session left the world");
    let portal_from = moments
        .iter()
        .position(|m| m.portal)
        .expect("portal space shown");
    let select_from = moments
        .iter()
        .position(|m| m.screen == Some(Screen::Select))
        .expect("character select");
    // The wait: the box alone over the world drawn behind it, the HUD gone, from the frame after
    // Log Out until the session leaves the world, as long as the server holds the character.
    let wait = &moments[1..left];
    assert!(hud > 40, "the HUD drawn before: {hud} quads");
    assert!(
        wait.iter()
            .all(|m| m.waiting && m.only_the_box && !m.nothing),
        "the box alone: {:?}",
        wait.iter()
            .find(|m| !(m.waiting && m.only_the_box && !m.nothing))
    );
    assert!(
        moments[1..portal_from].iter().all(|m| m.world),
        "the world behind the box"
    );
    let held = moments[left].at;
    assert!(
        (18.9..19.3).contains(&held),
        "the server held it {held:.2} s"
    );
    // Portal space from the server's word, before the session leaves the world.
    let into = moments[portal_from].at;
    assert!((18.8..19.0).contains(&into), "portal space at {into:.2} s");
    // Then in sight, nothing of Horizon's over it from the frame Horizon hears the world was left,
    // until it has been seen long enough.
    let gone = moments[portal_from..]
        .iter()
        .position(|m| !m.portal)
        .map(|i| portal_from + i)
        .expect("portal space goes");
    let after = &moments[left + 1..gone];
    assert!(
        after
            .iter()
            .all(|m| m.screen == Some(Screen::Passage) && m.nothing && !m.waiting),
        "nothing over portal space: {:?}",
        after
            .iter()
            .find(|m| !(m.screen == Some(Screen::Passage) && m.nothing))
    );
    let seen_after = moments[gone].at - moments[left].at;
    assert!(
        (leaving::PORTAL_SEEN - 0.05..leaving::PORTAL_SEEN + 0.1).contains(&seen_after),
        "portal space in sight {seen_after:.2} s after the world was left"
    );
    // Character select once it goes, and not before.
    assert!(
        select_from >= gone,
        "select at {select_from}, portal gone at {gone}"
    );
}
