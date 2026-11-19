//! The figure before the world, drawn by the shell frame by frame on a clock stepped a sixtieth of
//! a second: character select's and creation's play their idle, and creation's shows the height
//! chosen.

use auberean_horizon::creation::Creation;
use auberean_horizon::ui::pregame::Screen;
use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_shell::interface::ViewScene;
use auberean_world::chargen;

use crate::session::{self, Session};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// The figure's vertices as posed in the next frame, in its own frame: what its animation moved.
fn posed(s: &mut Session) -> Vec<Vec3> {
    s.frame_with(|out| {
        let view = out.views.first().expect("the figure's view");
        view.frame
            .objects
            .iter()
            .filter_map(|o| o.posed.as_ref())
            .flat_map(|p| p.positions().iter().copied())
            .collect()
    })
}

/// Over a second of frames, how many moved the figure from the frame before.
fn frames_that_moved(s: &mut Session, each: impl Fn(&mut Session)) -> usize {
    let mut last = posed(s);
    assert!(!last.is_empty(), "a figure drawn");
    let mut moved = 0;
    for _ in 0..60 {
        each(s);
        let now = posed(s);
        moved += usize::from(now != last);
        last = now;
    }
    moved
}

fn select(s: &mut Session) {
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
}

fn open_creation(s: &mut Session) {
    let records = &session::data().0.records;
    s.horizon().borrow_mut().ui_mut().pregame.creation =
        Some(Creation::new(records, 0, Random::new()));
    s.step();
    assert_eq!(s.horizon().borrow().screen(), Some(Screen::Creation));
}

/// The selected character on character select, and the one being made in creation, stand playing
/// their idle: frame after frame their pose moves on. In creation it goes on while the figure is
/// turned.
///
/// Behaviour: desktop.horizon.the-figures-before-the-world-play-their-idle
#[test]
fn the_figures_on_character_select_and_in_creation_play_their_idle() {
    let mut s = Session::choosing(Some(HORIZON));
    select(&mut s);
    s.step();
    let moved = frames_that_moved(&mut s, |_| {});
    assert!(moved > 30, "select: {moved} of 60 frames moved the figure");
    open_creation(&mut s);
    let moved = frames_that_moved(&mut s, |_| {});
    assert!(
        moved > 30,
        "creation: {moved} of 60 frames moved the figure"
    );
    let moved = frames_that_moved(&mut s, |s| {
        let h = s.horizon();
        let mut h = h.borrow_mut();
        let c = h.ui_mut().pregame.creation.as_mut().expect("open");
        c.turn = (c.turn + 3.0).rem_euclid(360.0);
    });
    assert!(
        moved > 30,
        "creation, turning: {moved} of 60 frames moved the figure"
    );
}

/// The figure creation shows in the next frame.
fn shown(s: &mut Session) -> ViewScene {
    s.frame()
        .scenes
        .first()
        .cloned()
        .expect("the figure's scene")
}

/// Creation's height changes the figure's whole height, and the viewport is framed the same for
/// every height (for the tallest the race may be), so the figure stands taller or shorter in it.
///
/// Behaviour: desktop.horizon.creation-s-height-shows-in-the-figure
#[test]
fn creation_s_height_scales_the_whole_figure_in_a_frame_that_stays_the_same() {
    let mut s = Session::choosing(Some(HORIZON));
    select(&mut s);
    open_creation(&mut s);
    let row = |s: &Session| {
        let h = s.horizon();
        let h = h.borrow();
        let c = h.ui().pregame.creation.as_ref().expect("open");
        c.rows()
            .into_iter()
            .find(|r| r.kind == chargen::HEIGHT)
            .expect("a height to choose")
    };
    let stops = row(&s).stops;
    assert!(stops > 2, "{stops} stops");
    let at = |s: &mut Session, stop: usize| {
        {
            let h = s.horizon();
            let mut h = h.borrow_mut();
            let c = h.ui_mut().pregame.creation.as_mut().expect("open");
            c.set(chargen::HEIGHT, stop);
        }
        s.step();
        let scene = shown(s);
        let top = posed(s).iter().map(|v| v.z).fold(f32::MIN, f32::max);
        let f = &scene.figures[0];
        (
            scene.camera,
            f.look.scale,
            f.origin.z + top * f.look.scale.z,
        )
    };
    let (low_camera, low_scale, low_top) = at(&mut s, 0);
    let (high_camera, high_scale, high_top) = at(&mut s, stops - 1);
    assert_eq!(low_camera, high_camera, "framed the same");
    let ratio = high_scale.z / low_scale.z;
    assert!(ratio > 1.1, "{low_scale:?} to {high_scale:?}");
    // The whole body: across as much as up, the build alike.
    assert!((high_scale.x / low_scale.x - ratio).abs() < 1e-4);
    // The figure's head stands higher in the frame, its feet where they were.
    assert!(high_top > low_top, "{low_top} to {high_top}");
}
