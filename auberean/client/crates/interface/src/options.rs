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
        options::Request::AssistanceSite
        | options::Request::Assistance { .. }
        | options::Request::Preferences(_) => None,
    }
}

/// The player's options for the character page, read again when they may have changed.
#[derive(Debug, Default)]
pub struct Reader {
    /// They may have changed since last read.
    pub stale: bool,
    /// They changed in a way the character page takes up itself, or not until it next reads them:
    /// read again without telling it, so its unapplied rows stay.
    // D2-02831, D2-02137
    pub quiet: bool,
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
        if !(self.stale || self.quiet || said || now.is_none()) {
            return;
        }
        let Some(o) = client.options() else {
            return;
        };
        self.count_read(said || now.is_none());
        *now = Some(Values {
            bits: o.bits,
            radar_mask: o.radar_mask,
            shortcut_height: o.shortcut_height,
            damage_range: f32::from_bits(o.damage_text_range),
            read: self.read,
        });
    }

    /// A reading taken: it tells the page when they were stale or `told`, and not when only
    /// quiet.
    fn count_read(&mut self, told: bool) {
        if self.stale || told {
            self.read += 1;
        }
        self.stale = false;
        self.quiet = false;
    }
}

#[cfg(test)]
mod reader_tests {
    //! Behaviour: none (when the character page is told to read the player's options again)
    use super::Reader;

    #[test]
    fn a_quiet_reading_does_not_tell_the_page() {
        let mut r = Reader {
            stale: true,
            ..Reader::default()
        };
        r.count_read(false);
        assert_eq!(r.read, 1, "stale tells it");
        r.quiet = true;
        r.count_read(false);
        assert_eq!(r.read, 1, "quiet does not");
        assert!(!r.quiet);
        r.quiet = true;
        r.stale = true;
        r.count_read(false);
        assert_eq!(r.read, 2, "stale wins");
        r.count_read(true);
        assert_eq!(r.read, 3, "a notice tells it");
    }
}
