//! World capabilities used by every interface, independent of its artwork.
use dereth_primitives::{DataId, EraFeatures, EraId};

/// Presentation facts of the selected world profile. Stored spell banks remain unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraUiFacts {
    pub void_magic: bool,
    pub spell_level_eight: bool,
    pub spell_favorite_tabs: usize,
}
impl Default for EraUiFacts {
    fn default() -> Self {
        Self::for_profile(EraId::Eor)
    }
}
impl EraUiFacts {
    #[must_use]
    pub const fn for_profile(profile: EraId) -> Self {
        match profile {
            EraId::Eor => Self {
                void_magic: true,
                spell_level_eight: true,
                spell_favorite_tabs: 8,
            },
            EraId::Infiltration => Self {
                void_magic: false,
                spell_level_eight: false,
                spell_favorite_tabs: 7,
            },
        }
    }
    /// Whether an authored spell-filter control is available on this world.
    #[must_use]
    pub const fn spell_filter(self, bit: u32) -> bool {
        match bit {
            0x2000 => self.void_magic,
            0x800 => self.spell_level_eight,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestPage {
    Journal,
    Contracts,
}

/// Retain a valid selection; otherwise prefer the journal, then contracts.
#[must_use]
pub const fn quest_page(features: EraFeatures, current: Option<QuestPage>) -> Option<QuestPage> {
    match current {
        Some(QuestPage::Journal) if features.journal => current,
        Some(QuestPage::Contracts) if features.contracts => current,
        _ if features.journal => Some(QuestPage::Journal),
        _ if features.contracts => Some(QuestPage::Contracts),
        _ => None,
    }
}

/// The unlocked sigil slots, intersected with world availability.
#[must_use]
pub const fn aetheria_slots(features: EraFeatures, unlocks: i32) -> u8 {
    if features.aetheria {
        (unlocks & 7) as u8
    } else {
        0
    }
}

/// A complete localized caption with an English fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caption {
    pub table: DataId,
    pub token: u32,
    pub fallback: &'static str,
}
#[must_use]
pub const fn fellowship_share_caption(features: EraFeatures) -> Caption {
    if features.luminance {
        Caption {
            table: DataId(0x23000003),
            token: 0x0e2897f0,
            fallback: "Share Fellowship Experience and Luminance",
        }
    } else {
        Caption {
            table: DataId(0x23000005),
            token: 0x0e2897f0,
            fallback: "Share Fellowship Experience",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmptyGameView, EraView, GameSnapshot, GameView};
    use dereth_primitives::{ContainerEra, ObjectId};

    #[derive(Debug)]
    struct World {
        era: EraView,
        unlocks: Option<i32>,
    }
    impl GameView for World {
        fn era(&self) -> Option<&EraView> {
            Some(&self.era)
        }
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(1))
        }
        fn int_stat(&self, id: ObjectId, property: u32) -> Option<i32> {
            if id == ObjectId(1) && property == 0x142 {
                self.unlocks
            } else {
                None
            }
        }
        fn spell_tab(&self, tab: usize) -> &[u32] {
            if tab == 7 {
                &[42]
            } else {
                &[]
            }
        }
    }
    fn world(era: EraId) -> World {
        World {
            era: EraView {
                era,
                era_announced: true,
                ..Default::default()
            },
            unlocks: None,
        }
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn unknown_world_uses_the_default_profile_without_enabling_every_system() {
        assert_eq!(EmptyGameView.era_features(), EraFeatures::END_OF_RETAIL);
        assert!(!EmptyGameView.era_features().spell_research);
        assert!(!EmptyGameView.era_features().swear_xp_cost);
        assert_eq!(EmptyGameView.era_ui().spell_favorite_tabs, 8);
        assert_eq!(EmptyGameView.aetheria_slots(), 0);
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn profile_facts_ignore_container_format_and_skill_loading() {
        let mut early = world(EraId::Infiltration);
        early.era.skills = vec![43];
        assert_eq!(
            early.era_ui(),
            EraUiFacts {
                void_magic: false,
                spell_level_eight: false,
                spell_favorite_tabs: 7
            }
        );
        let mut late = world(EraId::Eor);
        late.era.world_dats = ContainerEra::Classic;
        assert_eq!(
            late.era_ui(),
            EraUiFacts {
                void_magic: true,
                spell_level_eight: true,
                spell_favorite_tabs: 8
            }
        );
        late.era.announced_features.set("luminance", false);
        late.era.announced_features.set("spell_research", true);
        assert!(!late.era_features().luminance);
        assert!(late.era_features().spell_research);
        assert_eq!(late.era_ui(), EraUiFacts::for_profile(EraId::Eor));
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn magic_controls_read_independent_facts() {
        let facts = EraUiFacts {
            void_magic: false,
            spell_level_eight: true,
            spell_favorite_tabs: 7,
        };
        assert!(!facts.spell_filter(0x2000));
        assert!(facts.spell_filter(0x800));
        let facts = EraUiFacts {
            void_magic: true,
            spell_level_eight: false,
            ..facts
        };
        assert!(facts.spell_filter(0x2000));
        assert!(!facts.spell_filter(0x800));
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn quest_fallback_preserves_a_valid_page_and_hides_neither_systems_page() {
        for (journal, contracts, fallback) in [
            (false, false, None),
            (true, false, Some(QuestPage::Journal)),
            (false, true, Some(QuestPage::Contracts)),
            (true, true, Some(QuestPage::Journal)),
        ] {
            let features = EraFeatures {
                journal,
                contracts,
                ..Default::default()
            };
            assert_eq!(quest_page(features, None), fallback);
            assert_eq!(quest_page(features, Some(QuestPage::Journal)), fallback);
            assert_eq!(
                quest_page(features, Some(QuestPage::Contracts)),
                if contracts {
                    Some(QuestPage::Contracts)
                } else {
                    fallback
                }
            );
        }
    }

    /// Behaviour: presentation.era.aetheria-slots-follow-character-unlocks
    #[test]
    fn aetheria_slots_use_only_the_three_unlock_bits() {
        for bits in [0, 1, 2, 4, 7, 0x100] {
            let mut w = world(EraId::Eor);
            w.unlocks = Some(bits);
            assert_eq!(w.aetheria_slots(), (bits & 7) as u8);
            w.era.era = EraId::Infiltration;
            assert_eq!(w.aetheria_slots(), 0);
        }
        assert_eq!(world(EraId::Eor).aetheria_slots(), 0);
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn snapshots_preserve_resolved_facts_and_all_eight_favorite_banks() {
        let mut w = world(EraId::Eor);
        w.unlocks = Some(5);
        let snapshot = GameSnapshot::from_view(&w);
        assert_eq!(snapshot.aetheria_slots(), 5);
        assert_eq!(snapshot.era_ui(), w.era_ui());
        assert_eq!(snapshot.era_features(), w.era_features());
        w.unlocks = Some(2);
        assert_eq!(GameSnapshot::from_view(&w).aetheria_slots(), 2);
        assert_eq!(snapshot.aetheria_slots(), 5);
        w.era.era = EraId::Infiltration;
        let snapshot = GameSnapshot::from_view(&w);
        assert_eq!(snapshot.era_ui().spell_favorite_tabs, 7);
        assert_eq!(snapshot.spell_tab(7), &[42]);
        let custom = GameSnapshot {
            resolved_era_ui: Some(EraUiFacts {
                void_magic: false,
                spell_level_eight: true,
                spell_favorite_tabs: 7,
            }),
            ..Default::default()
        };
        assert_eq!(GameSnapshot::from_view(&custom).era_ui(), custom.era_ui());
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn fellowship_caption_selects_complete_table_and_text_variants() {
        let full = fellowship_share_caption(EraFeatures::END_OF_RETAIL);
        let short = fellowship_share_caption(EraFeatures::INFILTRATION);
        assert_eq!(
            (full.table.0, full.token, full.fallback),
            (
                0x23000003,
                0x0e2897f0,
                "Share Fellowship Experience and Luminance"
            )
        );
        assert_eq!(
            (short.table.0, short.token, short.fallback),
            (0x23000005, 0x0e2897f0, "Share Fellowship Experience")
        );
    }
}
