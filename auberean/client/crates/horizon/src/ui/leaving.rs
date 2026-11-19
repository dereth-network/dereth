//! Logging out. The server holds the wait, as the game's own servers did: the character stays in
//! the world, frozen in the glow it arrived in, for about fifteen seconds, plays its leaving, and
//! nearly four seconds later goes into portal space, the session ending a moment after. Meanwhile
//! the HUD and the windows are put away and a box in the middle says the client is logging out, the
//! world drawn behind it, as the game's own interface shows its wait. Once the session has left
//! the world portal space stays in sight [`PORTAL_SEEN`] longer, nothing over it, before character
//! select, so the trip out is seen.

use dereth_ui_kit::art::Family;
use dereth_ui_kit::colours::Colours;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets as kit;

use crate::art::Art;
use crate::draw::{DrawList, Rect};

/// What the box says.
pub const LINE: &str = "Logging out...";

/// How long portal space stays in sight once the session has left the world, seconds: about the
/// second it takes to come in, and a second whole.
pub const PORTAL_SEEN: f64 = 2.0;

/// The box's size, layout units.
const BOX: (f32, f32) = (340.0, 120.0);

/// Where the box stands on a screen `screen` pixels wide and high at `k` pixels a layout unit: in
/// the middle.
#[must_use]
pub fn layout(screen: (f32, f32), k: f32) -> Rect {
    let (w, h) = (BOX.0 * k, BOX.1 * k);
    Rect::new((screen.0 - w) / 2.0, (screen.1 - h) / 2.0, w, h)
}

/// The frame while logging out: the box alone, over the world.
pub fn frame(list: &mut DrawList, art: &Art, screen: (f32, f32)) {
    list.clear();
    let colours = Colours::default();
    let mut p = Painter {
        list,
        art,
        scale: crate::scale::layout_scale(screen),
        screen,
        fade: 1.0,
    };
    let r = layout(screen, p.scale);
    kit::panel(&mut p, r, 0.9);
    let style = TextStyle::new(Family::Body, 18.0, colours.text()).edge(colours.edge());
    p.text_in(&style, r, Align::Centre, LINE);
}
