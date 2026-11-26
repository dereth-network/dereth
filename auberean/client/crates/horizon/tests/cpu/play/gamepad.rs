//! Gamepad mode: the pad's state, frame by frame and with no device, turned into the game's
//! controls and actions in the world, the cross hotbars over the shortcut bar, and the focus moved
//! over a window.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::control::{movement, Control, ControlId, ControlSource};
use auberean_client_sdk::contract::view as v;
use auberean_client_sdk::contract::{shortcut_kind, SelectKind};
use auberean_client_sdk::Action;
use auberean_horizon::abilities::{Bar, Slot};
use auberean_horizon::gamepad::{camera, cross_slot, PadAsked, PadMode, PAD_DEVICE, ZOOM_EVERY};
use auberean_horizon::pad::CrossSet;
use auberean_horizon::state::{GameState, Target};
use auberean_horizon::ui::cross::{place_rect, Crosses};
use auberean_shell::interface::{CameraRequest, Request};
use auberean_shell::pad::{PadButton as B, PadState};

use crate::play::party::{in_party, CARA, DOV};
use crate::{in_world, Harness, PLAYER};

const FRAME: f64 = 1.0 / 60.0;

fn press(id: ControlId) -> Action {
    Action::Control(Control::begin(id).from(ControlSource {
        device: PAD_DEVICE,
        input: id.0,
    }))
}

fn release(id: ControlId) -> Action {
    Action::Control(Control::end(id).from(ControlSource {
        device: PAD_DEVICE,
        input: id.0,
    }))
}

/// The interface and gamepad mode, switched on by a first press.
struct Pad {
    h: Harness,
    mode: PadMode,
}

impl Pad {
    fn on() -> Self {
        let mut p = Self {
            h: Harness::new(),
            mode: PadMode::default(),
        };
        let woke = p.step(&in_world(), PadState::holding(&[B::Start]));
        assert_eq!(
            woke,
            PadAsked::default(),
            "the waking press does nothing else"
        );
        assert!(p.mode.on);
        p.step(&in_world(), PadState::default());
        p
    }

    /// One frame of the pad, then the interface's own.
    fn step(&mut self, state: &GameState, pad: PadState) -> PadAsked {
        let out = self
            .mode
            .frame(Some(pad), FRAME, &mut self.h.ui, state, &mut self.h.input);
        self.h.frame(state);
        out
    }
}

fn selected() -> GameState {
    let mut s = in_world();
    s.target = Some(Target {
        id: 0x8000_0000_0000_0042,
        name: "Bandit".into(),
        health: None,
    });
    s
}

fn bar_with(slots: &[u32]) -> GameState {
    let mut all = vec![Slot::default(); 100];
    for n in slots {
        all[*n as usize] = Slot {
            shortcut: v::Shortcut {
                kind: shortcut_kind::SKILL,
                data_enum: 7,
                ..v::Shortcut::default()
            },
            name: "Slash".into(),
            ..Slot::default()
        };
    }
    let mut s = in_world();
    s.bar = Some(Bar {
        version: 1,
        slots: all,
        set: 3,
        rows: 1,
        locked: false,
        numbers: true,
    });
    s
}

#[test]
fn out_of_gamepad_mode_the_pad_only_switches_it_on_and_a_key_switches_it_off() {
    let mut mode = PadMode::default();
    let mut h = Harness::new();
    let s = selected();
    let still = mode.frame(
        Some(PadState::default()),
        FRAME,
        &mut h.ui,
        &s,
        &mut h.input,
    );
    assert!(!mode.on && still == PadAsked::default());
    let nudge = PadState {
        left: (0.0, 0.3),
        ..PadState::default()
    };
    mode.frame(Some(nudge), FRAME, &mut h.ui, &s, &mut h.input);
    assert!(!mode.on, "a nudge of a stick does not");
    let a = mode.frame(
        Some(PadState::holding(&[B::South])),
        FRAME,
        &mut h.ui,
        &s,
        &mut h.input,
    );
    assert!(
        mode.on && a.acts.is_empty(),
        "A switched it on and used nothing"
    );
    // Held, a stick moves the player; the key that switches it off lets go of that.
    let ahead = PadState {
        left: (0.0, 1.0),
        ..PadState::default()
    };
    let moved = mode.frame(Some(ahead), FRAME, &mut h.ui, &s, &mut h.input);
    assert_eq!(moved.acts, vec![press(movement::FORWARD)]);
    let mut acts = Vec::new();
    mode.off(&mut acts);
    assert!(!mode.on);
    assert_eq!(acts, vec![release(movement::FORWARD)]);
}

#[test]
fn the_left_stick_holds_the_movement_controls_it_points_along_and_lets_them_go() {
    let mut p = Pad::on();
    let s = in_world();
    let ahead = PadState {
        left: (0.0, 1.0),
        ..PadState::default()
    };
    assert_eq!(p.step(&s, ahead).acts, vec![press(movement::FORWARD)]);
    assert!(p.step(&s, ahead).acts.is_empty(), "held: pressed once");
    let diagonal = PadState {
        left: (0.7, 0.7),
        ..PadState::default()
    };
    assert_eq!(
        p.step(&s, diagonal).acts,
        vec![press(movement::STRAFE_RIGHT)]
    );
    let back_left = PadState {
        left: (-0.7, -0.7),
        ..PadState::default()
    };
    assert_eq!(
        p.step(&s, back_left).acts,
        vec![
            release(movement::FORWARD),
            release(movement::STRAFE_RIGHT),
            press(movement::BACKWARD),
            press(movement::STRAFE_LEFT),
        ]
    );
    let centred = p.step(&s, PadState::default());
    assert_eq!(
        centred.acts,
        vec![release(movement::BACKWARD), release(movement::STRAFE_LEFT)]
    );
}

#[test]
fn the_right_stick_turns_the_camera_and_with_lb_held_zooms_it_a_step_at_a_time() {
    let mut p = Pad::on();
    let s = in_world();
    let right = PadState {
        right: (1.0, 0.0),
        ..PadState::default()
    };
    assert_eq!(p.step(&s, right).acts, vec![press(camera::ROTATE_RIGHT)]);
    let up = PadState {
        right: (0.0, 1.0),
        ..PadState::default()
    };
    assert_eq!(
        p.step(&s, up).acts,
        vec![release(camera::ROTATE_RIGHT), press(camera::TILT_UP)]
    );
    p.step(&s, PadState::default());
    let zooming = PadState {
        right: (0.0, 1.0),
        ..PadState::holding(&[B::LeftBumper])
    };
    let first = p.step(&s, zooming);
    assert!(first.acts.is_empty(), "no turn while LB is held");
    assert_eq!(
        first.requests,
        vec![Request::Camera(CameraRequest::Zoom { inward: true })]
    );
    // Held: a step every so often, not every frame.
    let mut steps = 0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (ZOOM_EVERY / FRAME).ceil() as usize * 3;
    for _ in 0..frames {
        steps += p.step(&s, zooming).requests.len();
    }
    assert!((2..=4).contains(&steps), "{steps} steps in {frames} frames");
    let out = PadState {
        right: (0.0, -1.0),
        ..zooming
    };
    p.step(&s, PadState::holding(&[B::LeftBumper]));
    assert_eq!(
        p.step(&s, out).requests,
        vec![Request::Camera(CameraRequest::Zoom { inward: false })]
    );
}

#[test]
fn a_trigger_held_uses_its_cross_hotbars_places_and_rb_with_a_button_shows_a_set() {
    let mut p = Pad::on();
    // The third set shown: slots 20 to 29, the row over it 30 to 39.
    let s = bar_with(&[20, 36]);
    let lt = PadState {
        left_trigger: 1.0,
        ..PadState::default()
    };
    assert!(p.step(&s, lt).acts.is_empty());
    let up = PadState {
        buttons: PadState::holding(&[B::DPadUp]).buttons,
        ..lt
    };
    assert_eq!(
        p.step(&s, up).acts,
        vec![Action::UseToolbarSlot { slot: 20 }]
    );
    p.step(&s, lt);
    let right_place = PadState {
        buttons: PadState::holding(&[B::DPadRight]).buttons,
        ..lt
    };
    assert!(p.step(&s, right_place).acts.is_empty(), "an empty slot");
    let rt = PadState {
        right_trigger: 1.0,
        ..PadState::default()
    };
    p.step(&s, rt);
    // A is the bottom face button: the cross's seventh place.
    let a = PadState {
        buttons: PadState::holding(&[B::South]).buttons,
        ..rt
    };
    assert_eq!(
        p.step(&s, a).acts,
        vec![Action::UseToolbarSlot { slot: 36 }],
        "A fires its place, nothing else"
    );
    p.step(&s, PadState::default());
    p.step(&s, PadState::holding(&[B::RightBumper]));
    assert_eq!(
        p.step(&s, PadState::holding(&[B::RightBumper, B::West]))
            .acts,
        vec![Action::ToolbarSet { set: 4 }],
        "X the fourth set"
    );
    assert_eq!(
        cross_slot(90, CrossSet::Right, 0),
        None,
        "past the last slot"
    );
    assert_eq!(cross_slot(0, CrossSet::Left, 8), None, "eight places");
}

#[test]
fn the_face_buttons_use_select_stop_jump_and_the_map_and_lb_tapped_runs_on() {
    let mut p = Pad::on();
    let none = in_world();
    assert_eq!(
        p.step(&none, PadState::holding(&[B::South])).acts,
        vec![Action::SelectNearest {
            kind: SelectKind::Creature
        }]
    );
    p.step(&none, PadState::default());
    let s = selected();
    assert_eq!(
        p.step(&s, PadState::holding(&[B::South])).acts,
        vec![Action::UseSelected]
    );
    p.step(&s, PadState::default());
    assert_eq!(
        p.step(&s, PadState::holding(&[B::East])).acts,
        vec![
            Action::Select { target: 0 },
            press(movement::STOP),
            release(movement::STOP)
        ]
    );
    p.step(&s, PadState::default());
    assert_eq!(
        p.step(&s, PadState::holding(&[B::North])).acts,
        vec![press(movement::JUMP)]
    );
    assert_eq!(
        p.step(&s, PadState::default()).acts,
        vec![release(movement::JUMP)],
        "a jump for as long as Y is held"
    );
    p.step(&s, PadState::holding(&[B::LeftBumper]));
    assert_eq!(
        p.step(&s, PadState::default()).acts,
        vec![press(movement::RUN_LOCK), release(movement::RUN_LOCK)]
    );
    assert_eq!(
        p.step(&s, PadState::holding(&[B::LeftStick])).acts,
        vec![Action::Attack { start: true }]
    );
    p.step(&s, PadState::default());
    p.step(&s, PadState::holding(&[B::West]));
    assert!(p.h.ui.map.is_open(), "X opens the map");
}

#[test]
fn the_d_pad_steps_through_the_fellowship_and_cycles_the_creatures() {
    let mut p = Pad::on();
    let alone = in_world();
    assert_eq!(
        p.step(&alone, PadState::holding(&[B::DPadDown])).acts,
        vec![Action::SelectSelf]
    );
    p.step(&alone, PadState::default());
    let party = in_party();
    let mut picked = Vec::new();
    for _ in 0..4 {
        let out = p.step(&party, PadState::holding(&[B::DPadDown]));
        picked.extend(out.acts);
        p.step(&party, PadState::default());
    }
    let ids = |acts: &[Action]| -> Vec<u64> {
        acts.iter()
            .filter_map(|a| match a {
                Action::SetSelection { target } => Some(*target),
                _ => None,
            })
            .collect()
    };
    assert_eq!(ids(&picked), vec![PLAYER, CARA, DOV, PLAYER], "round");
    assert_eq!(
        ids(&p.step(&party, PadState::holding(&[B::DPadUp])).acts),
        vec![DOV]
    );
    p.step(&party, PadState::default());
    assert_eq!(
        p.step(&party, PadState::holding(&[B::DPadLeft])).acts,
        vec![Action::SelectCycle {
            kind: SelectKind::Creature,
            previous: true
        }]
    );
}

#[test]
fn on_a_window_the_d_pad_moves_the_focus_and_the_pointer_a_presses_and_b_closes_it() {
    let mut p = Pad::on();
    let s = in_world();
    let ahead = PadState {
        left: (0.0, 1.0),
        ..PadState::default()
    };
    p.step(&s, ahead);
    // Start opens the settings: the movement the pad held is let go as the window takes it.
    let start = PadState {
        buttons: PadState::holding(&[B::Start]).buttons,
        ..ahead
    };
    p.step(&s, start);
    assert!(p.h.ui.settings.is_open());
    let opened = p.step(&s, ahead);
    assert_eq!(opened.acts, vec![release(movement::FORWARD)]);
    // The focus goes into the window: the pointer is over one of its controls.
    let body = p.h.ui.settings.body().expect("open");
    assert!(body.contains(p.h.input.mouse.0, p.h.input.mouse.1));
    let was = p.h.input.mouse;
    p.step(&s, PadState::holding(&[B::DPadDown]));
    p.step(&s, PadState::default());
    assert_ne!(p.h.input.mouse, was, "the d-pad moved it");
    // Down to Log out, the window's last control, and A on it.
    let c = auberean_horizon::ui::settings::Settings::controls(body, p.h.k());
    let mut logged_out = false;
    for _ in 0..30 {
        if c.log_out.contains(p.h.input.mouse.0, p.h.input.mouse.1)
            || c.layout.contains(p.h.input.mouse.0, p.h.input.mouse.1)
        {
            break;
        }
        p.step(&s, PadState::holding(&[B::DPadDown]));
        p.step(&s, PadState::default());
    }
    if c.layout.contains(p.h.input.mouse.0, p.h.input.mouse.1) {
        p.step(&s, PadState::holding(&[B::DPadRight]));
        p.step(&s, PadState::default());
    }
    assert!(
        c.log_out.contains(p.h.input.mouse.0, p.h.input.mouse.1),
        "reached Log out"
    );
    p.mode.frame(
        Some(PadState::holding(&[B::South])),
        FRAME,
        &mut p.h.ui,
        &s,
        &mut p.h.input,
    );
    logged_out |= p.h.frame(&s).log_out;
    p.mode.frame(
        Some(PadState::default()),
        FRAME,
        &mut p.h.ui,
        &s,
        &mut p.h.input,
    );
    logged_out |= p.h.frame(&s).log_out;
    assert!(!logged_out, "Log out asks first");
    assert!(p.h.ui.log_out.is_up(), "A pressed Log out");
    // B takes the question down, then closes the window.
    p.step(&s, PadState::holding(&[B::East]));
    p.step(&s, PadState::default());
    assert!(!p.h.ui.log_out.is_up());
    assert!(p.h.ui.settings.is_open());
    p.step(&s, PadState::holding(&[B::East]));
    assert!(!p.h.ui.settings.is_open());
}

#[test]
fn in_gamepad_mode_the_cross_hotbars_stand_over_the_hotbar_the_one_held_lit() {
    let mut h = Harness::new();
    let s = bar_with(&[20]);
    h.frame(&s);
    let k = h.k();
    let up = place_rect(h.screen, k, 1, CrossSet::Left, 0);
    let drawn = |h: &Harness| h.list.quads.iter().any(|q| q.dst == up);
    assert!(!drawn(&h), "not out of gamepad mode");
    h.ui.crosses = Some(Crosses {
        held: Some(CrossSet::Left),
    });
    h.frame(&s);
    assert!(drawn(&h));
    let bar_top = auberean_horizon::ui::hotbar::slot_rect(h.screen, k, 0, 0);
    for half in [CrossSet::Left, CrossSet::Right] {
        for place in 0..8 {
            let r = place_rect(h.screen, k, 1, half, place);
            assert!(r.bottom() < bar_top.y, "over the hotbar");
        }
    }
    let left = place_rect(h.screen, k, 1, CrossSet::Left, 1);
    let right = place_rect(h.screen, k, 1, CrossSet::Right, 3);
    assert!(left.right() < right.x, "the two apart");
}
