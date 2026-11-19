//! Horizon's screens before the world: the game before the world as Horizon follows it, character
//! select, creation, connecting and the disconnected screen, and the view of the world behind
//! character select.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod creation;
mod pad;
mod select;
mod state;
mod vista;

use auberean_horizon::pregame::{Character, Pregame};
use auberean_horizon::ui::pregame::PreOut;
use auberean_shell::interface::Phase;

use crate::Harness;

/// One frame before the world of `pre`; what it asked.
pub fn frame(h: &mut Harness, pre: &Pregame) -> PreOut {
    let mut out = PreOut::default();
    h.ui.pregame.follow(pre, &mut out);
    h.ui.pregame_frame(
        &mut h.list,
        h.screen,
        1.0 / 60.0,
        pre,
        &mut h.input,
        true,
        &[],
        &mut out,
    );
    h.input.next_frame();
    out
}

/// A left press and release at `(x, y)` over two frames before the world; what they asked.
pub fn click(h: &mut Harness, pre: &Pregame, (x, y): (f32, f32)) -> PreOut {
    h.input.mouse = (x, y);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let mut first = frame(h, pre);
    h.input.down[0] = false;
    h.input.released[0] = true;
    let second = frame(h, pre);
    first.acts.extend(second.acts);
    PreOut {
        acts: first.acts,
        connect: first.connect || second.connect,
        quit: first.quit || second.quit,
        create: first.create || second.create,
    }
}

/// A key pressed (a widget kit key number) in the next frame before the world.
pub fn key(h: &mut Harness, pre: &Pregame, key: usize) -> PreOut {
    h.input.keys.push(key);
    frame(h, pre)
}

pub const ROWAN: u64 = 0x2130_0000_0000_0001;
pub const ELM: u64 = 0x2130_0000_0000_0002;

/// The character list up, with Rowan and Elm (Elm being deleted, gone in 90 seconds), room for
/// six.
pub fn listed() -> Pregame {
    let mut p = Pregame::default();
    p.phase = Phase::Characters;
    p.slots = 6;
    p.characters = Some(vec![
        Character {
            id: ROWAN,
            name: "Rowan".into(),
            deleting: 0,
        },
        Character {
            id: ELM,
            name: "Elm".into(),
            deleting: 90,
        },
    ]);
    p
}
