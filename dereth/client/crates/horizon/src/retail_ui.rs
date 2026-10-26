//! The game's own panels a plugin hides or moves (`retail_ui`), as this interface has them: each
//! panel name the plugins use, mapped to the HUD element or the windows here that do what it does
//! in the game's own interface.
//!
//! What a plugin asks is laid over this interface each frame and changes nothing the player keeps:
//! a hidden element or window is not drawn and takes no pointer, a moved one stands at the place
//! asked for, and once the plugin lets go each is back where the player left it.

use crate::ui::panels::WindowId;

/// What a panel name is here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counterpart {
    /// A HUD element, by its layout name.
    Hud(&'static str),
    /// A group of windows: hidden together, and each put at the place asked for.
    Windows(&'static [WindowId]),
}

/// The counterpart here of the game's panel `name`, or `None` for one this interface has nothing
/// like: `side_vitals` (one vitals bar here) and `power_bar` (the power pips are part of the stance
/// gauge, which `combat` is).
#[must_use]
pub fn counterpart(name: &str) -> Option<Counterpart> {
    Some(match name {
        "chat" => Counterpart::Hud("chat"),
        "vitals" => Counterpart::Hud("parameter"),
        "toolbar" => Counterpart::Hud("hotbars"),
        "radar" => Counterpart::Hud("minimap"),
        "combat" => Counterpart::Hud("stance"),
        "panels" => Counterpart::Windows(&[
            WindowId::Character,
            WindowId::Inventory,
            WindowId::Actions,
            WindowId::Social,
            WindowId::Allegiance,
            WindowId::Map,
            WindowId::Options,
            WindowId::Journal,
        ]),
        "environment" => Counterpart::Windows(&[
            WindowId::Loot,
            WindowId::Vendor,
            WindowId::Trade,
            WindowId::Salvage,
            WindowId::Maintenance,
        ]),
        "examination" => Counterpart::Windows(&[WindowId::Examine]),
        _ => return None,
    })
}

/// One plugin's ask of a panel: its name, whether it is hidden (`Some(false)` shows it, which
/// here is no more than not hiding it), and its top-left corner on the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub panel: String,
    pub hidden: Option<bool>,
    pub at: Option<(f32, f32)>,
}

/// Lay `asked` over the HUD and the windows, in place of what was asked last frame.
pub fn follow(ui: &mut crate::ui::HorizonUi, asked: &[Ask]) {
    ui.hud.retail_hidden.clear();
    ui.hud.layout.pinned.clear();
    ui.windows.retail_hidden.clear();
    ui.windows.retail_at.clear();
    for ask in asked {
        match counterpart(&ask.panel) {
            Some(Counterpart::Hud(name)) => {
                if ask.hidden == Some(true) {
                    ui.hud.retail_hidden.insert(name);
                }
                if let Some(at) = ask.at {
                    ui.hud.layout.pinned.insert(name.to_owned(), at);
                }
            }
            Some(Counterpart::Windows(ids)) => {
                for id in ids {
                    if ask.hidden == Some(true) {
                        ui.windows.retail_hidden.insert(*id);
                    }
                    if let Some(at) = ask.at {
                        ui.windows.retail_at.insert(*id, at);
                    }
                }
            }
            None => {}
        }
    }
}
