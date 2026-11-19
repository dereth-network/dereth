//! Entering the world under either interface on a clock stepped a sixtieth of a second a frame:
//! portal space shows from the start, goes out to white and comes out into the world on the same
//! timeline, and a long first frame of the world leaves its fade whole.

use auberean_draw::portal::State;
use auberean_horizon::ui::pregame::Screen;

use crate::session::Session;

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// How long the frame that draws the world again takes, as loading the world's pictures can.
const LONG_FRAME: f64 = 0.8;

/// One frame of the entry: portal space's phase, the white over all, whether the world was
/// drawn, and whether Horizon drew a screen of its own over portal space.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Moment {
    at: f64,
    state: State,
    white: f32,
    world: bool,
    covered: bool,
}

/// The frames from the session's start until portal space is over after the world came back, the
/// first frame of coming out taking [`LONG_FRAME`].
fn enter(settings: Option<&str>) -> Vec<Moment> {
    let mut s = Session::new(settings);
    s.step();
    if settings.is_none() {
        // The game's own interface opens on its pictures: a press ends them.
        s.click((10.0, 10.0));
    }
    let mut moments: Vec<Moment> = Vec::new();
    while s.shell.host.time < 20.0 {
        if moments.last().is_some_and(|m| m.state == State::ComingOut)
            && moments
                .iter()
                .filter(|m| m.state == State::ComingOut)
                .count()
                == 1
        {
            s.shell.host.time += LONG_FRAME;
        }
        s.advance();
        let out = s.shell.frame();
        let white = out.veil.map_or(0.0, |c| c[3]);
        let world = out.world.is_some();
        let covered = s.horizon.borrow().as_ref().is_some_and(|h| {
            let h = h.borrow();
            h.screen().is_some_and(|screen| screen != Screen::Passage)
                || (h.screen().is_some() && !h.draw_list().quads.is_empty())
        });
        let state = s.shell.scene.portal.state;
        moments.push(Moment {
            at: s.shell.host.time,
            state,
            white,
            world,
            covered,
        });
        let came_out = moments.iter().any(|m| m.state == State::ComingOut);
        if came_out && state == State::Off {
            return moments;
        }
    }
    panic!("portal space never ended: {:?}", moments.last());
}

/// Entering the world with Horizon shown, portal space shows from the frame after it is first
/// drawn (the frame Horizon hears of it), with nothing of Horizon's over it, as under the game's
/// own interface. Under either it goes out to
/// white over a second and comes out over another into the world, frame for frame the same, and
/// the frame that draws the world again, taking most of a second, leaves the white whole and the
/// fade its full second.
///
/// Behaviour: desktop.horizon.entering-shows-portal-space-and-its-fade-as-the-game-s-own-interface
#[test]
fn entering_under_horizon_shows_portal_space_and_its_fade_as_the_game_s_own_interface() {
    let mut timelines = Vec::new();
    for (name, settings) in [("the game's own", None), ("Horizon", Some(HORIZON))] {
        let moments = enter(settings);
        let shown: Vec<&Moment> = moments
            .iter()
            .filter(|m| m.state != State::Off)
            .skip(1)
            .collect();
        assert!(
            shown.iter().all(|m| !m.covered),
            "{name}: portal space covered at {:?}",
            shown.iter().filter(|m| m.covered).collect::<Vec<_>>()
        );
        let going = moments
            .iter()
            .position(|m| m.state == State::GoingOut)
            .expect("portal space goes out");
        assert!(
            moments[..going].iter().any(|m| m.state == State::In),
            "{name}: portal space in before it goes out"
        );
        let from = &moments[going..];
        let out: Vec<&Moment> = from.iter().filter(|m| m.state == State::GoingOut).collect();
        let back: Vec<&Moment> = from
            .iter()
            .filter(|m| m.state == State::ComingOut)
            .collect();
        // Going out: the white rises from nothing to whole over a second, the world not drawn.
        assert!(
            out.windows(2).all(|w| w[1].white >= w[0].white),
            "{name}: white rises"
        );
        assert!(out[0].white < 0.05, "{name}: {:?}", out[0]);
        assert!(
            out.iter().all(|m| !m.world),
            "{name}: no world while going out"
        );
        let rose = out[out.len() - 1].at - out[0].at;
        assert!(
            (0.95..1.05).contains(&rose),
            "{name}: going out took {rose} s"
        );
        // Coming out: the world drawn under the white, whole after the long first frame, fading
        // over a full second from there.
        assert!(
            back.iter().all(|m| m.world),
            "{name}: the world drawn coming out"
        );
        assert!(
            back[0].white > 0.99 && back[1].white > 0.99,
            "{name}: {:?}",
            &back[..2]
        );
        assert!(
            back[1].at - back[0].at >= LONG_FRAME,
            "{name}: the long frame"
        );
        assert!(
            back.windows(2).all(|w| w[1].white <= w[0].white),
            "{name}: white fades"
        );
        let faded = from.last().expect("over").at - back[1].at;
        assert!(
            (1.0..1.05).contains(&faded),
            "{name}: coming out took {faded} s"
        );
        assert_eq!(from.last().map(|m| (m.white, m.world)), Some((0.0, true)));
        timelines.push(
            from.iter()
                .map(|m| (m.state, (m.white * 1000.0).round()))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(timelines[0], timelines[1], "the same timeline under both");
}
