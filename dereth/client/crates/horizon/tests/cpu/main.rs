//! The interface driven frame by frame, with no install (every picture and font missing), no
//! device and no server: what the player's presses and drags ask the game for.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod hud;
mod items;
mod screens;
mod windows;

use std::sync::Arc;

use dereth_horizon::art::Art;
use dereth_horizon::draw::DrawList;
use dereth_horizon::options::HorizonOptions;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::input::InputFrame;
use dereth_horizon::ui::{HorizonUi, Outcome};

/// The interface at 1920x1080 with no art behind it.
#[derive(Debug)]
pub struct Harness {
    pub ui: HorizonUi,
    pub list: DrawList,
    pub input: InputFrame,
}

impl Harness {
    pub fn new(options: HorizonOptions) -> Self {
        Self {
            ui: HorizonUi::new(Arc::new(Art::empty()), options),
            list: DrawList::default(),
            input: InputFrame::default(),
        }
    }

    /// One frame at 60 frames a second.
    pub fn frame(&mut self, state: &GameState) -> Outcome {
        let out = self.ui.frame(
            &mut self.list,
            (1920.0, 1080.0),
            1.0 / 60.0,
            state,
            &mut self.input,
        );
        self.input.next_frame();
        out
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.input.mouse = (x, y);
    }

    pub fn press(&mut self) {
        self.input.down[0] = true;
        self.input.pressed[0] = true;
    }

    pub fn release(&mut self) {
        self.input.down[0] = false;
        self.input.released[0] = true;
    }
}
