//! World capabilities used by every interface, independent of its artwork.
use dereth_primitives::{DataId, EraFeatures, EraId};

/// Presentation facts of the selected world profile. Stored spell banks remain unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraUiFacts {
    pub void_magic: bool,
    pub spell_level_eight: bool,
    pub spell_favorite_tabs: usize,
    /// Whether a scroll bar with nothing to scroll still draws its thumb.
    pub idle_scroll_thumb: bool,
    /// One panel of the spells in effect on a character, rather than beneficial and harmful apart.
    pub one_effects_panel: bool,
    /// The interface's words are the 1999 client's where a later client changed them.
    pub launch_wording: bool,
    /// The map opens only outdoors.
    pub map_outdoors_only: bool,
    /// The spellbook filters its rows by school and level.
    pub spell_filter_panel: bool,
    /// The combat panel carries the camera's option to keep the target in view.
    pub combat_keep_in_view: bool,
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
                idle_scroll_thumb: true,
                one_effects_panel: false,
                launch_wording: false,
                map_outdoors_only: false,
                spell_filter_panel: true,
                combat_keep_in_view: true,
            },
            EraId::Launch => Self {
                void_magic: false,
                spell_level_eight: false,
                spell_favorite_tabs: 7,
                idle_scroll_thumb: false,
                one_effects_panel: true,
                launch_wording: true,
                map_outdoors_only: true,
                spell_filter_panel: false,
                combat_keep_in_view: false,
            },
            EraId::Infiltration => Self {
                void_magic: false,
                spell_level_eight: false,
                spell_favorite_tabs: 7,
                idle_scroll_thumb: true,
                one_effects_panel: false,
                launch_wording: false,
                map_outdoors_only: false,
                spell_filter_panel: true,
                combat_keep_in_view: false,
            },
        }
    }
    /// The facts for a world of `profile` playing `features`: a world without the spellcasting
    /// panel's tabs (before May 2000) has one bar of favorites.
    #[must_use]
    pub const fn for_world(profile: EraId, features: EraFeatures) -> Self {
        let mut facts = Self::for_profile(profile);
        if !features.spell_tabs {
            facts.spell_favorite_tabs = 1;
        }
        facts
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

/// A window, page, tab or slot that belongs to one system of the world. Every interface asks this
/// one question, so a world without the system loses it the same way in each; each interface maps
/// its own windows to a surface and refuses to open what is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EraSurface {
    /// The window holding the journal and the contracts: offered with either.
    QuestWindow,
    /// The journal's pages.
    Journal,
    /// The contracts held.
    Contracts,
    /// The character titles.
    Titles,
    /// The house, its purchase and its maintenance.
    House,
    /// The chess board.
    Chess,
    /// Secure trade.
    Trade,
    /// The salvage window a tinkering tool opens.
    Salvage,
    /// The Create Spell page.
    SpellResearch,
    /// The paper doll's cloak slot.
    CloakSlot,
    /// The paper doll's trinket slot.
    TrinketSlot,
}

impl EraSurface {
    /// Every surface.
    pub const ALL: [Self; 11] = [
        Self::QuestWindow,
        Self::Journal,
        Self::Contracts,
        Self::Titles,
        Self::House,
        Self::Chess,
        Self::Trade,
        Self::Salvage,
        Self::SpellResearch,
        Self::CloakSlot,
        Self::TrinketSlot,
    ];

    /// Whether a world playing `features` has this surface.
    #[must_use]
    pub const fn offered(self, features: EraFeatures) -> bool {
        match self {
            Self::QuestWindow => quest_page(features, None).is_some(),
            Self::Journal => features.journal,
            Self::Contracts => features.contracts,
            Self::Titles => features.titles,
            Self::House => features.housing,
            Self::Chess => features.chess,
            Self::Trade => features.trade,
            Self::Salvage => features.tinkering,
            Self::SpellResearch => features.spell_research,
            Self::CloakSlot => features.cloaks,
            Self::TrinketSlot => features.trinkets,
        }
    }

    /// Whether a world is known to lack this surface: a world whose era is not known has
    /// everything.
    #[must_use]
    pub const fn refused(self, features: Option<EraFeatures>) -> bool {
        match features {
            Some(features) => !self.offered(features),
            None => false,
        }
    }
}

/// The warning before creating a character with skill credits unspent, on a world where they are
/// lost when the character enters it.
pub const LOST_SKILL_CREDITS_WARNING: &str =
    "You have not spent all your Training Credits. If you \
     don't spend them during character creation, you will lose them. Create this character anyway?";

/// Whether creating a character with `remaining` skill credits unspent loses them.
#[must_use]
pub const fn loses_skill_credits(features: EraFeatures, remaining: i32) -> bool {
    !features.creation_credit_carry_over && remaining > 0
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

    /// Behaviour: presentation.era.endurance-information-follows-the-world-profile
    /// The Endurance information follows the announced systems, not the era's name, and survives
    /// a snapshot.
    #[test]
    fn launch_endurance_information_follows_the_announced_systems_and_survives_snapshots() {
        for profile in EraId::ALL {
            let mut w = world(profile);
            w.era.world_dats = ContainerEra::Modern;
            let on = w.era_features().endurance_benefits;
            assert_eq!(on, profile != EraId::Launch);
            assert_eq!(
                GameSnapshot::from_view(&w)
                    .era_features()
                    .endurance_benefits,
                on
            );
            w.era.announced_features.set("endurance_benefits", !on);
            assert_eq!(w.era_features().endurance_benefits, !on, "{profile}");
            assert_eq!(
                GameSnapshot::from_view(&w)
                    .era_features()
                    .endurance_benefits,
                !on
            );
        }
        assert!(EmptyGameView.era_features().endurance_benefits);
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
                spell_favorite_tabs: 7,
                idle_scroll_thumb: true,
                one_effects_panel: false,
                launch_wording: false,
                map_outdoors_only: false,
                spell_filter_panel: true,
                combat_keep_in_view: false,
            }
        );
        let mut late = world(EraId::Eor);
        late.era.world_dats = ContainerEra::Classic;
        assert_eq!(
            late.era_ui(),
            EraUiFacts {
                void_magic: true,
                spell_level_eight: true,
                spell_favorite_tabs: 8,
                idle_scroll_thumb: true,
                one_effects_panel: false,
                launch_wording: false,
                map_outdoors_only: false,
                spell_filter_panel: true,
                combat_keep_in_view: true,
            }
        );
        late.era.announced_features.set("luminance", false);
        late.era.announced_features.set("spell_research", true);
        assert!(!late.era_features().luminance);
        assert!(late.era_features().spell_research);
        assert_eq!(late.era_ui(), EraUiFacts::for_profile(EraId::Eor));
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    /// A world without the spellcasting panel's tabs keeps one bar of favorites, from its
    /// announced systems: a Launch world that announces the tabs has seven, and a snapshot keeps
    /// the count.
    #[test]
    fn a_world_without_spell_tabs_has_one_bar_of_favorites() {
        let mut w = world(EraId::Launch);
        assert_eq!(w.era_ui().spell_favorite_tabs, 1);
        assert_eq!(GameSnapshot::from_view(&w).era_ui().spell_favorite_tabs, 1);
        w.era.announced_features.set("spell_tabs", true);
        assert_eq!(w.era_ui().spell_favorite_tabs, 7);
        assert_eq!(GameSnapshot::from_view(&w).era_ui().spell_favorite_tabs, 7);
        let mut late = world(EraId::Eor);
        late.era.announced_features.set("spell_tabs", false);
        assert_eq!(late.era_ui().spell_favorite_tabs, 1);
    }

    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn magic_controls_read_independent_facts() {
        let facts = EraUiFacts {
            void_magic: false,
            spell_level_eight: true,
            spell_favorite_tabs: 7,
            idle_scroll_thumb: true,
            one_effects_panel: false,
            launch_wording: false,
            map_outdoors_only: false,
            spell_filter_panel: true,
            combat_keep_in_view: false,
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

    /// Behaviour: presentation.era.a-surface-follows-its-own-system
    /// Each surface follows its own flag alone, and a world whose era is not known has everything.
    #[test]
    fn each_surface_follows_its_own_system_and_an_unknown_world_has_everything() {
        fn lacking(surface: EraSurface) -> EraFeatures {
            let mut f = EraFeatures::ALL;
            match surface {
                EraSurface::QuestWindow => {
                    f.journal = false;
                    f.contracts = false;
                }
                EraSurface::Journal => f.journal = false,
                EraSurface::Contracts => f.contracts = false,
                EraSurface::Titles => f.titles = false,
                EraSurface::House => f.housing = false,
                EraSurface::Chess => f.chess = false,
                EraSurface::Trade => f.trade = false,
                EraSurface::Salvage => f.tinkering = false,
                EraSurface::SpellResearch => f.spell_research = false,
                EraSurface::CloakSlot => f.cloaks = false,
                EraSurface::TrinketSlot => f.trinkets = false,
            }
            f
        }
        for surface in EraSurface::ALL {
            assert!(surface.offered(EraFeatures::ALL), "{surface:?}");
            assert!(!surface.refused(Some(EraFeatures::ALL)), "{surface:?}");
            assert!(!surface.refused(None), "{surface:?}");
            assert!(!surface.offered(lacking(surface)), "{surface:?}");
            assert!(surface.refused(Some(lacking(surface))), "{surface:?}");
            for other in EraSurface::ALL {
                let shares_a_flag = matches!(
                    (surface, other),
                    (EraSurface::QuestWindow, _) | (_, EraSurface::QuestWindow)
                );
                if other != surface && !shares_a_flag {
                    assert!(
                        other.offered(lacking(surface)),
                        "{other:?} without {surface:?}"
                    );
                }
            }
        }
        let mut one = lacking(EraSurface::Journal);
        assert!(EraSurface::QuestWindow.offered(one), "contracts alone");
        one.contracts = false;
        assert!(!EraSurface::QuestWindow.offered(one));
    }

    /// Behaviour: presentation.era.unspent-skill-credits-warn-where-they-are-lost
    #[test]
    fn only_a_world_without_the_carry_over_warns_of_lost_skill_credits() {
        let mut f = EraFeatures::ALL;
        assert!(!loses_skill_credits(f, 3));
        f.creation_credit_carry_over = false;
        assert!(loses_skill_credits(f, 3));
        assert!(!loses_skill_credits(f, 0));
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
                idle_scroll_thumb: true,
                one_effects_panel: false,
                launch_wording: false,
                map_outdoors_only: false,
                spell_filter_panel: true,
                combat_keep_in_view: false,
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
