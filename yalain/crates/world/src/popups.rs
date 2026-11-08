//! Tips: a short text a player is shown once in a box of its own, the first time a moment comes for
//! its character. The world database's `[[popup]]` rows say which effect's description each moment
//! shows; a world with none shows nothing.
//!
//! The moments, as 2017 showed them: a new character's first entry (2 of 2 recorded, as the session
//! starts), its first level gained (2 of 2, after the level's own line), and the first glyph that
//! came into its hands (2 of 2, as it was taken, before the quest it moved). Each comes once: a
//! character keeps a mark of each moment it has passed, tip or none, and one kept before tips were
//! shown has passed them all.

use auberean_script::args::Arg;
use yalain_content::popups::Moment;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{progress, session, Server};

/// The install's class every glyph derives from.
pub const GLYPH_CLASS: &str = "GlyphTemplate";

/// The call that shows a text in a box of its own.
pub use crate::equip::DISPLAY_MESSAGE;

/// A moment has come for a player: the first time it comes for its character, it is marked passed,
/// and the tip the world gives it is shown. A moment the world has no tip for, or whose effect says
/// nothing, shows nothing, and is passed all the same: a tip added later is not shown for it.
pub fn moment<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, m: Moment) {
    let text = s
        .popups
        .effect(m)
        .and_then(|e| s.effect_table.descriptions.get(&e).copied());
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.play.popups_shown & m.mark() != 0 {
        return;
    }
    p.play.popups_shown |= m.mark();
    let Some(text) = text else {
        return;
    };
    let shown = session::call(
        DISPLAY_MESSAGE,
        &[Arg::Int(0), crate::chat::text_arg(progress::text(text))],
    )
    .expect("a string reference packs");
    progress::private(net, world, &shown);
}

/// An item of a template came into a player's hands: the glyph tip, if it is a glyph.
pub fn took<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, template: u32) {
    let glyph = s
        .catalog
        .get(template)
        .is_some_and(|t| s.glyph_packages.contains(&t.description.package));
    if glyph {
        moment(s, net, world, Moment::Glyph);
    }
}
