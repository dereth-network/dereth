//! What a world must have for each request a player makes of it.
//!
//! A world's systems are its [`EraFeatures`](dereth_primitives::EraFeatures). The client leaves
//! out the commands its world lacks and the server refuses the requests behind them; both ask
//! [`Gate::available`](crate::gates::Gate::available), so the two agree on which systems each
//! request needs.

use dereth_primitives::EraFeatures;

/// A request a world may lack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gate {
    /// `@house recall`: to the character's house. The house recalls came with the apartments.
    HouseRecall,
    /// `@house mansion_recall`: to the allegiance's villa or mansion.
    MansionRecall,
    /// `@marketplace`.
    MarketplaceRecall,
    /// `@pkarena`.
    PkArenaRecall,
    /// `@pklarena`: the arena recalls and PK Lite both.
    PklArenaRecall,
    /// `@lifestone` recalls to the lifestone, and `@die` kills the character. Without it
    /// `@lifestone` kills the character and there is no `@die`.
    LifestoneRecall,
    /// The allegiance's message of the day and its broadcasts.
    AllegianceMessages,
    /// Naming the allegiance.
    AllegianceName,
    /// Appointing and listing the allegiance's officers.
    AllegianceOfficers,
    /// Banning accounts from the allegiance.
    AllegianceBans,
    /// Moving a member out of the allegiance room, which came with the officers.
    AllegianceChatBoot,
    /// The officers' titles: the officers and their titles both.
    AllegianceOfficerTitles,
    /// The allegiance lock and its approved vassal, which came with the titles.
    AllegianceLock,
    /// Gagging a member of the allegiance room, which came with the titles.
    AllegianceChatGag,
    /// A patron or monarch booting a member out of the allegiance.
    AllegianceBoot,
    /// `@allegiance house`: the monarch's villa or mansion opened to the allegiance.
    AllegianceHouse,
    /// The allegiance chat room.
    AllegianceChat,
}

impl Gate {
    /// Whether a world with `features` has this request.
    #[must_use]
    pub fn available(self, features: &EraFeatures) -> bool {
        match self {
            Self::HouseRecall | Self::MansionRecall => features.apartments,
            Self::MarketplaceRecall => features.marketplace_recall,
            Self::PkArenaRecall => features.pk_arena_recalls,
            Self::PklArenaRecall => features.pk_arena_recalls && features.pk_lite,
            Self::LifestoneRecall => features.lifestone_recall,
            Self::AllegianceMessages => features.allegiance_messages,
            Self::AllegianceName => features.allegiance_name,
            Self::AllegianceOfficers | Self::AllegianceBans | Self::AllegianceChatBoot => {
                features.allegiance_officers_and_bans
            }
            Self::AllegianceOfficerTitles | Self::AllegianceLock | Self::AllegianceChatGag => {
                features.allegiance_officers_and_bans && features.allegiance_titles_and_lock
            }
            Self::AllegianceBoot => features.allegiance_boot,
            Self::AllegianceHouse => features.allegiance_house_commands,
            Self::AllegianceChat => features.allegiance_chat,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn without(f: impl FnOnce(&mut EraFeatures)) -> EraFeatures {
        let mut features = EraFeatures::END_OF_RETAIL;
        f(&mut features);
        features
    }

    /// Behaviour: presentation.era.commands-follow-the-world-profile
    /// The recalls need the apartments, not the houses, and the PK Lite arena needs the arena
    /// recalls and PK Lite both.
    #[test]
    fn the_recalls_need_their_own_systems() {
        let all = EraFeatures::END_OF_RETAIL;
        for gate in [
            Gate::HouseRecall,
            Gate::MansionRecall,
            Gate::MarketplaceRecall,
            Gate::PkArenaRecall,
            Gate::PklArenaRecall,
            Gate::LifestoneRecall,
        ] {
            assert!(gate.available(&all), "{gate:?}");
        }

        let houses_only = without(|f| f.apartments = false);
        assert!(houses_only.housing);
        assert!(!Gate::HouseRecall.available(&houses_only));
        assert!(!Gate::MansionRecall.available(&houses_only));

        assert!(!Gate::MarketplaceRecall.available(&without(|f| f.marketplace_recall = false)));
        assert!(!Gate::LifestoneRecall.available(&without(|f| f.lifestone_recall = false)));

        let no_pk_lite = without(|f| f.pk_lite = false);
        assert!(Gate::PkArenaRecall.available(&no_pk_lite));
        assert!(!Gate::PklArenaRecall.available(&no_pk_lite));
        let no_arenas = without(|f| f.pk_arena_recalls = false);
        assert!(!Gate::PkArenaRecall.available(&no_arenas));
        assert!(!Gate::PklArenaRecall.available(&no_arenas));
    }

    /// Behaviour: presentation.era.commands-follow-the-world-profile
    /// The titles, the lock and the gag need the officers as well as their own system, and moving
    /// a member out of the room needs the officers.
    #[test]
    fn the_allegiance_tools_need_the_officers_and_then_their_own_system() {
        let later = [
            Gate::AllegianceOfficerTitles,
            Gate::AllegianceLock,
            Gate::AllegianceChatGag,
        ];
        let officers = [
            Gate::AllegianceOfficers,
            Gate::AllegianceBans,
            Gate::AllegianceChatBoot,
        ];
        let no_titles = without(|f| f.allegiance_titles_and_lock = false);
        assert!(officers.iter().all(|g| g.available(&no_titles)));
        assert!(!later.iter().any(|g| g.available(&no_titles)));
        let no_officers = without(|f| f.allegiance_officers_and_bans = false);
        assert!(!officers.iter().any(|g| g.available(&no_officers)));
        assert!(!later.iter().any(|g| g.available(&no_officers)));
        assert!(later
            .iter()
            .all(|g| g.available(&EraFeatures::END_OF_RETAIL)));

        for (gate, f) in [
            (
                Gate::AllegianceMessages,
                without(|f| f.allegiance_messages = false),
            ),
            (Gate::AllegianceName, without(|f| f.allegiance_name = false)),
            (Gate::AllegianceBoot, without(|f| f.allegiance_boot = false)),
            (
                Gate::AllegianceHouse,
                without(|f| f.allegiance_house_commands = false),
            ),
            (Gate::AllegianceChat, without(|f| f.allegiance_chat = false)),
        ] {
            assert!(gate.available(&EraFeatures::END_OF_RETAIL), "{gate:?}");
            assert!(!gate.available(&f), "{gate:?}");
        }
    }
}
