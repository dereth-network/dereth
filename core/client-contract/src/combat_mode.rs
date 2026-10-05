//! `COMBAT_MODE` — the five ids. They live in the contract because `GameView::combat_mode`'s
//! default body is [`NONCOMBAT`].
//!
//! Only the ids are here. `toolbar::combat_mode::BUTTONS` (which pairs each id with an
//! [`ElementId`] of the stance row) and `toolbar::combat_mode::toolbar_active`
//! stay in `dereth-ui-screens`, and that module `pub use`s this one so
//! `dereth_ui_screens::toolbar::combat_mode::MELEE` still resolves.

/// Undefined combat mode.
pub const UNDEF: u32 = 0;
/// See [`UNDEF`].
pub const NONCOMBAT: u32 = 1;
/// See [`UNDEF`].
pub const MELEE: u32 = 2;
/// See [`UNDEF`].
pub const MISSILE: u32 = 4;
/// See [`UNDEF`].
pub const MAGIC: u32 = 8;

// The toolbar's two reads of the same four modes, shared with
// `dereth_ui_screens::toolbar::combat_mode`. `dereth_client_runtime::{hud, interaction}` call both: the HUD
// ghosts the shortcut numerals with `toolbar_active`, and the input path matches a pressed
// element against `BUTTONS`. Element ids and one comparison.

use crate::ids::ElementId;

/// `(mode, element)` in the combat-mode notice handler's own call order.
pub const BUTTONS: [(u32, ElementId); 4] = [
    (NONCOMBAT, ElementId(0x1000_0192)),
    (MELEE, ElementId(0x1000_0193)),
    (MISSILE, ElementId(0x1000_0194)),
    (MAGIC, ElementId(0x1000_0195)),
];

/// Toolbar active = the new mode is not `MAGIC_COMBAT_MODE` — the first line of the handler, and the
/// flag it passes to every slot's shortcut-numeral setter as `!active`: in magic mode the shortcut
/// numerals are drawn **ghosted**, because the number keys cast spells instead.
#[must_use]
pub const fn toolbar_active(mode: u32) -> bool {
    mode != MAGIC
}
