//! The Asheron's Call 2 interface: elements built from the layout records, drawn with the
//! records' pictures and fonts, the way the September 2013 client builds and draws them.
//!
//! **Depends on** the records (`auberean-assets`, `auberean-dat`), record ids
//! (`auberean-primitives`), the game's text writer (`auberean-text`), the shared picture codecs
//! (`dereth-render-cpu`) and the original's float-to-integer conversion (`dereth-primitives`).
//! **Used by** the game's panels (`auberean-ui-game`) and the client.
//!
//! - [`token`]: the hashed names everything in a layout is known by.
//! - [`desc`]: the layout records, indexed as the client looks them up.
//! - [`source`]: the records the interface draws from ([`Assets`]).
//! - [`system`]: the element tree ([`Ui`]): elements made from descriptions, their states,
//!   attributes, pictures, children, places and depths.
//! - [`manager`]: the player's input and the clock as a whole: the pointer's buttons and moves,
//!   the interface keys, the element with the keyboard, listeners, the cursor.
//! - [`region`]: which elements the pointer is over, and which presses and releases reach them.
//! - [`input`]: what every element does with the pointer and the keyboard, and its place moved.
//! - [`types`]: what each kind of element does in place of that (buttons, drag and resize bars,
//!   scroll bars, lists, meters, fields, text).
//! - [`media`]: what a state plays as the clock runs (pictures, animations, pauses, sounds,
//!   cursors, jumps, messages, state changes) and the fades a top-level element runs.
//! - [`location`]: an element's place as the game keeps it for a character.
//! - [`node`]: elements joined as a tree's parents and children, and the lines between them.
//! - [`draw`]: the interface drawn to a picture, element by element, as the original composes it.
//! - [`text`]: glyphs in the records' fonts, kept in a list, broken into lines and drawn.
//! - [`tooltip`]: the tooltip of the element the pointer rests on.
//!
//! **Must never** know a game panel (those are `auberean-ui-game`'s) or a device: what leaves
//! here is pictures and messages.
//!
//! The look is a separable layer: the tree, states and messages are the interface; [`draw`]
//! paints them with the records' own pictures and fonts, and another painter may paint the same
//! tree another way.

#![forbid(unsafe_code)]

pub mod desc;
pub mod dragdrop;
pub mod draw;
pub mod framework;
pub mod input;
pub mod listener;
pub mod location;
pub mod manager;
pub mod media;
pub mod node;
pub mod region;
pub mod source;
pub mod system;
pub mod text;
pub mod token;
pub mod tooltip;
pub mod types;

pub use desc::{DescIx, LayoutIx, Library};
pub use listener::{Listener, ListenerId, Mail};
pub use manager::Cursor;
pub use source::{Assets, DatAssets, Image};
pub use system::{Handle, Message, Ui, Value};
pub use token::Token;

/// A rectangle with inclusive edges: `x1 - x0 + 1` wide.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    #[must_use]
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// A rectangle `w` by `h` at `(x, y)`.
    #[must_use]
    pub const fn sized(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self::new(x, y, x + w - 1, y + h - 1)
    }

    #[must_use]
    pub const fn width(self) -> i32 {
        self.x1 - self.x0 + 1
    }

    #[must_use]
    pub const fn height(self) -> i32 {
        self.y1 - self.y0 + 1
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    /// The overlap of two rectangles, if any.
    #[must_use]
    pub fn intersect(self, o: Self) -> Option<Self> {
        let r = Self::new(
            self.x0.max(o.x0),
            self.y0.max(o.y0),
            self.x1.min(o.x1),
            self.y1.min(o.y1),
        );
        (!r.is_empty()).then_some(r)
    }

    #[must_use]
    pub const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    /// The rectangle moved by `(dx, dy)`.
    #[must_use]
    pub const fn offset(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x0 + dx, self.y0 + dy, self.x1 + dx, self.y1 + dy)
    }
}
