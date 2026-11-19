//! Horizon driven frame by frame from a game state, with no install, no device and no server: what
//! its HUD shows of the game's view and notices, what its chat entry and settings ask for, the
//! minimap, the game's questions, the party list, the social window, the windows of the player's
//! things, crafting, the pets and the mount, the map, the journal, books and the story's scenes,
//! its key layer, and its screens before the world.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod before;
mod crafting;
mod game;
mod pets;
mod play;
mod things;
mod windows;

use std::sync::Arc;

use auberean_horizon::art::{art, Art, Pieces};
use auberean_horizon::draw::DrawList;
use auberean_horizon::state::GameState;
use auberean_horizon::ui::{HorizonUi, InputFrame, Outcome};
use auberean_shell::interface::Choice;

/// The interface at 1920x1080, with no art behind it unless the test asks for the built-in art.
#[derive(Debug)]
pub struct Harness {
    pub ui: HorizonUi,
    pub list: DrawList,
    pub input: InputFrame,
    pub screen: (f32, f32),
    pub shown: Choice,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    pub fn new() -> Self {
        Self::with(Art::empty())
    }

    /// The interface with the art the client carries, so its pieces and text are drawn.
    pub fn with_built_in_art() -> Self {
        Self::with(art(Pieces::built_in().expect("the pieces built in"), None))
    }

    /// The interface with `art`.
    pub fn with(art: Art) -> Self {
        Self {
            ui: HorizonUi::new(Arc::new(art)),
            list: DrawList::default(),
            input: InputFrame::default(),
            screen: (1920.0, 1080.0),
            shown: Choice::Horizon,
        }
    }

    /// Screen pixels per layout unit.
    pub fn k(&self) -> f32 {
        auberean_horizon::scale::layout_scale(self.screen)
    }

    /// One frame at 60 frames a second.
    pub fn frame(&mut self, state: &GameState) -> Outcome {
        let out = self.ui.frame(
            &mut self.list,
            self.screen,
            1.0 / 60.0,
            state,
            &mut self.input,
            self.shown,
        );
        self.input.next_frame();
        out
    }

    /// A left press and release at `(x, y)` over two frames; what each asked.
    pub fn click(&mut self, state: &GameState, (x, y): (f32, f32)) -> Outcome {
        self.input.mouse = (x, y);
        self.input.down[0] = true;
        self.input.pressed[0] = true;
        let first = self.frame(state);
        self.input.down[0] = false;
        self.input.released[0] = true;
        let second = self.frame(state);
        merge(first, second)
    }

    /// A right press and release at `(x, y)` over two frames; what each asked.
    pub fn right_click(&mut self, state: &GameState, (x, y): (f32, f32)) -> Outcome {
        self.input.mouse = (x, y);
        self.input.down[1] = true;
        self.input.pressed[1] = true;
        let first = self.frame(state);
        self.input.down[1] = false;
        self.input.released[1] = true;
        let second = self.frame(state);
        merge(first, second)
    }

    /// A left press at `from`, the pointer moved to `to` with the button held, and let go of
    /// there, a frame each; what they asked.
    pub fn drag(&mut self, state: &GameState, from: (f32, f32), to: (f32, f32)) -> Outcome {
        self.input.mouse = from;
        self.input.down[0] = true;
        self.input.pressed[0] = true;
        let first = self.frame(state);
        self.input.mouse = to;
        let second = merge(first, self.frame(state));
        self.input.down[0] = false;
        self.input.released[0] = true;
        merge(second, self.frame(state))
    }

    /// A key pressed (a widget kit key number) in the next frame; what it asked.
    pub fn key(&mut self, state: &GameState, key: usize) -> Outcome {
        self.input.keys.push(key);
        self.frame(state)
    }

    /// Text typed in the next frame.
    pub fn type_text(&mut self, state: &GameState, text: &str) -> Outcome {
        self.input.chars.extend(text.chars());
        self.frame(state)
    }
}

/// What two frames asked, the second's where both asked.
fn merge(a: Outcome, b: Outcome) -> Outcome {
    let mut acts = a.acts;
    acts.extend(b.acts);
    let mut requests = a.requests;
    requests.extend(b.requests);
    Outcome {
        acts,
        requests,
        say: b.say.or(a.say),
        recall: b.recall.or(a.recall),
        choose: b.choose.or(a.choose),
        orbit: b.orbit.or(a.orbit),
        log_out: a.log_out || b.log_out,
        quit: a.quit || b.quit,
    }
}

/// What `f` measures with a painter over the interface's art at the harness's screen and the scale
/// drawn there (the step chosen in the settings, or Auto's).
pub fn measured<R>(h: &Harness, f: impl FnOnce(&dereth_ui_kit::paint::Painter<'_>) -> R) -> R {
    let art = Arc::clone(h.ui.art());
    let mut list = DrawList::default();
    let p = dereth_ui_kit::paint::Painter {
        list: &mut list,
        art: &art,
        scale: h.ui.layout_scale(h.screen),
        screen: h.screen,
        fade: 1.0,
    };
    f(&p)
}

/// Where the art's piece `name` was drawn in the last frame; none where the art has no such piece.
pub fn drew_piece(h: &Harness, name: &str) -> Vec<auberean_horizon::draw::Rect> {
    let Some(s) = measured(h, |p| p.piece(name)) else {
        return Vec::new();
    };
    h.list
        .quads
        .iter()
        .filter(|q| q.tex == Some(s.tex) && q.src == s.src)
        .map(|q| q.dst)
        .collect()
}

/// The middle of `r`.
pub fn centre(r: auberean_horizon::draw::Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// A player in the world.
pub fn in_world() -> GameState {
    let mut s = GameState::default();
    s.in_world = true;
    s.player = Some(PLAYER);
    s
}

pub const PLAYER: u64 = 0x2130_0000_0000_0001;
