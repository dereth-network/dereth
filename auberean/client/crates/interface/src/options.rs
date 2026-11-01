//! The options window's requests carried out, and the player's options read for it.
//!
//! The character page names each option by its label; the client sets it by the option's own
//! setter ([`PlayerOption`]). The player's options are read again from the scripts whenever they
//! may have changed: when they say so (the options read again, or the window's change), when the
//! window or the radar set some, and at first; each reading counts, and the character page shows
//! the options again at each.

use auberean_client_sdk::contract::PlayerOption;
use auberean_client_sdk::{Action, Client};
use auberean_ui_game::gameplay;
use auberean_ui_game::options::{self, character::label, Values};

/// The character page's options by label, as the client sets them.
const OPTIONS: &[(u32, PlayerOption)] = &[
    (label::AUTO_TARGET, PlayerOption::AutoTarget),
    (label::ALLEGIANCE, PlayerOption::TakeAllegianceOffers),
    (label::FELLOWSHIP, PlayerOption::TakeFellowshipOffers),
    (label::TRADE, PlayerOption::TakeTradeOffers),
    (label::DAMAGE_NUMBERS, PlayerOption::DamageNumbers),
    (label::OTHERS_DAMAGE, PlayerOption::OthersDamageNumbers),
    (label::PK_DAMAGE, PlayerOption::PlayerKillerDamage),
    (label::SHORTCUT_NUMBERS, PlayerOption::ShortcutNumbers),
    (label::SHORTCUT_LOCKED, PlayerOption::ShortcutLocked),
    (label::INSTANT_SCROLL, PlayerOption::ShortcutInstantScroll),
    (label::GOLD_MELTING, PlayerOption::AskBeforeMelting),
    (label::LATENCY_GRAPH, PlayerOption::LatencyGraph),
    (label::FELLOWS_ON_MAP, PlayerOption::FellowsOnMap),
    (label::PET_PANEL, PlayerOption::PetPanel),
    (label::CLASSIC_EXAMINATION, PlayerOption::ClassicExamination),
    (label::RADAR_MONSTERS, PlayerOption::RadarMonsters),
    (label::RADAR_NPCS, PlayerOption::RadarPeople),
    (label::RADAR_PCS, PlayerOption::RadarPlayers),
    (label::RADAR_PORTALS, PlayerOption::RadarPortals),
    (label::RADAR_LIFESTONES, PlayerOption::RadarLifestones),
    (label::RADAR_ITEMS, PlayerOption::RadarItems),
    (label::RADAR_COORDINATES, PlayerOption::RadarCoordinates),
    (label::OTHERS_DAMAGE_RANGE, PlayerOption::OthersDamageRange),
    (label::SHORTCUT_HEIGHT, PlayerOption::ShortcutHeight),
];

/// The client's option of a label on the character page.
#[must_use]
pub fn option_of(label: u32) -> Option<PlayerOption> {
    OPTIONS.iter().find(|(l, _)| *l == label).map(|(_, o)| *o)
}

/// The action an options request is carried out as, if it has one yet.
#[must_use]
pub fn action(r: &options::Request) -> Option<Action> {
    match r {
        options::Request::Set { changes, tell } => Some(Action::SetOptions {
            changes: changes
                .iter()
                .filter_map(|(l, v)| option_of(*l).map(|o| (o, *v)))
                .collect(),
            tell: *tell,
        }),
        _ => None,
    }
}

/// The player's options for the character page, read again when they may have changed.
#[derive(Debug, Default)]
pub struct Reader {
    /// They may have changed since last read.
    pub stale: bool,
    read: u32,
}

impl Reader {
    /// Read them again when stale (or when a notice of the last frame says they changed), when
    /// none were read yet, or keep them.
    pub fn prepare(
        &mut self,
        client: &mut Client,
        notices: &[gameplay::Notice],
        now: &mut Option<Values>,
    ) {
        let said = notices.iter().any(|n| {
            n.native == gameplay::notice::RELOAD_OPTIONS
                || n.native == options::notice::OPTIONS_CHANGED
        });
        if !(self.stale || said || now.is_none()) {
            return;
        }
        let Some(o) = client.options() else {
            return;
        };
        self.stale = false;
        self.read += 1;
        *now = Some(Values {
            bits: o.bits,
            radar_mask: o.radar_mask,
            shortcut_height: o.shortcut_height,
            damage_range: f32::from_bits(o.damage_text_range),
            read: self.read,
        });
    }
}
