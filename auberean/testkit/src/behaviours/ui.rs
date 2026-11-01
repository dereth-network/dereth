//! The interface: elements built from the layout records, the pointer, the compositor, and the
//! game's panels on them.
//!
//! A row claims what the September 2013 client's interface does.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "ui.shortcuts.an-item-on-the-bar-is-named-and-shown-as-the-pack-shows-it",
        says: "At each recorded log-off, the first items of the player's pack put on an empty slot of the bar are named and shown on the bar as the pack shows them: the name for the item's quantity and the icon its scripts give.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-ITEM-LOOK"),
        station: "auberean-client-runtime::corpus::toolbar::an_item_on_the_bar_is_named_and_shown_as_the_pack_shows_it",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "ui.shortcuts.the-toolbar-from-the-scripts-names-and-shows-each-slot",
        says: "Each time a recorded session's options came again, the toolbar asked of the player's scripts holds the player's shortcuts slot for slot; every skill slot gets the skill's name and an icon, every empty slot the same empty slot's tooltip and no icon; every skill a slot holds is trained; the height is 1 to 5 rows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-TOOLBAR"),
        station: "auberean-client-runtime::corpus::toolbar::the_toolbar_from_the_scripts_names_and_shows_each_slot",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "ui.statusbar.the-effects-from-the-scripts-are-the-players-effects",
        says: "Each time a recorded session's scripts said an effect on the player changed, the effects the player's scripts list for the effects panel each have a number, a name and an icon, no number twice, and each was among the player's effects when the session started or was added by the server since, and was not taken off since.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EFFECT-LIST"),
        station: "auberean-client-runtime::corpus::effects::the_effects_from_the_scripts_are_the_effects_the_server_gave_the_player",
        tier: Tier::Corpus,
    },
];
