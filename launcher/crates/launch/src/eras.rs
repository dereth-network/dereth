//! The eras a world can play and the systems each has, as the launcher names them, from the
//! client's own era table (`dereth_primitives::EraId`, `EraFeatures`).
//!
//! A world plays the era it says it plays (Empyrean's status ping or document), and the one the
//! launcher's table of worlds that run a client of their own gives it ([`crate::known`]); any
//! other world plays the end of retail, with the end of retail's systems. The player chooses
//! neither.

use std::collections::BTreeMap;

use dereth_primitives::{ContainerEra, EraFeatures, EraId};
use serde::Serialize;

use crate::datset::SetKind;

/// One era, as the launcher names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EraInfo {
    /// As command lines and status documents spell it.
    pub name: &'static str,
    pub label: &'static str,
    /// Which data set the era's world is drawn from.
    pub needs: SetKind,
    /// The era's table: every system, on or off.
    pub features: BTreeMap<&'static str, bool>,
}

/// One system, as the launcher names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureInfo {
    pub name: &'static str,
    pub label: String,
    /// When the system came to the game, or left it, for one a person would look for by name; the
    /// rest are rules and small commands that follow the era.
    pub dated: Option<&'static str>,
}

/// The systems a person would look for by name, in the order they came to the game, each with when.
/// A world's settings offer these first and fold the other flags away.
pub const NAMED_SYSTEMS: &[(&str, &str)] = &[
    ("trade", "2000"),
    ("housing", "2001"),
    ("cantrips", "2002"),
    ("apartments", "2002"),
    ("spell_research", "until 2002"),
    ("tinkering", "2002"),
    ("chess", "2002"),
    ("skill_alteration", "2002"),
    ("pk_lite", "2003"),
    ("allegiance_officers_and_bans", "2004"),
    ("augmentations", "2005"),
    ("titles", "2006"),
    ("journal", "2009"),
    ("trinkets", "2009"),
    ("ratings", "2009"),
    ("aetheria", "2010"),
    ("luminance", "2010"),
    ("contracts", "2011"),
    ("cloaks", "2011"),
    ("dual_wield", "2012"),
    ("weapon_masteries", "2012"),
];

fn era_label(era: EraId) -> &'static str {
    match era {
        EraId::Launch => "Launch (1999, experimental)",
        EraId::Infiltration => "Infiltration (February 2005)",
        EraId::Eor => "End of Retail",
    }
}

/// The era a world plays, in words: the launcher's name for an era this build knows, the world's
/// own word for one it does not, and the end of retail for a world that names none, as the page
/// words it.
pub fn label(era: Option<&str>) -> String {
    match era.map(str::trim).filter(|e| !e.is_empty()) {
        None => era_label(EraId::Eor).to_owned(),
        Some(e) => EraId::parse(e).map_or_else(|| e.to_owned(), |id| era_label(id).to_owned()),
    }
}

/// Every era, latest first: the end of retail is what most worlds play.
pub fn eras() -> Vec<EraInfo> {
    EraId::ALL
        .iter()
        .rev()
        .map(|&e| EraInfo {
            name: e.name(),
            label: era_label(e),
            needs: needs(e),
            features: e.features().iter().collect(),
        })
        .collect()
}

/// A system's name in words: `consolidated_weapon_skills` is "Consolidated weapon skills".
fn feature_label(name: &str) -> String {
    match name {
        "pre_order_items_and_rares" => "Pre-order items and rares".into(),
        "swear_xp_cost" => "Swearing costs experience".into(),
        "assessed_armor_and_ratings" => "Assessing shows armor and ratings".into(),
        "creation_credit_carry_over" => "Unspent skill credits kept".into(),
        "pk_lite" => "PK Lite".into(),
        "lifestone_recall" => "Lifestone recall and @die".into(),
        "pk_arena_recalls" => "PK arena recalls".into(),
        "player_corpse_hour_minimum" => "Player corpses last an hour".into(),
        "component_burn_skill_discount" => "Skill lowers component burn".into(),
        "war_and_life_projectile_criticals" => "War and Life Magic criticals".into(),
        "war_magic_skill_damage_bonus" => "War Magic skill damage bonus".into(),
        "transfer_overflow_reduces_source" => "Transfers cost less on overflow".into(),
        "item_enchantment_redirection" => "Item spells on worn items".into(),
        "corpse_permissions" => "Corpse permits".into(),
        "corpse_location_notices" => "Corpse location on death".into(),
        "chat_log" => "@log".into(),
        "corpse_command" => "@corpse".into(),
        "allegiance_messages" => "Allegiance MOTD and broadcasts".into(),
        "allegiance_oath_confirmation" => "Oaths need the patron's yes".into(),
        "appraisal_without_skills" => "Appraising needs no skill".into(),
        "allegiance_officers_and_bans" => "Allegiance officers and bans".into(),
        _ => {
            let words = name.replace('_', " ");
            let mut c = words.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        }
    }
}

/// Every system, in the table's order.
pub fn features() -> Vec<FeatureInfo> {
    EraFeatures::NAMES
        .iter()
        .map(|&name| FeatureInfo {
            name,
            label: feature_label(name),
            dated: NAMED_SYSTEMS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, d)| *d),
        })
        .collect()
}

fn needs(era: EraId) -> SetKind {
    match era.container_era() {
        // Launch is drawn from the 1999 pair, the other eras before Throne of Destiny from the
        // later one.
        ContainerEra::Classic if era == EraId::Launch => SetKind::Original,
        ContainerEra::Classic => SetKind::Classic,
        _ => SetKind::Modern,
    }
}

/// The data set a world's world is drawn from: the Original set for Launch, the Classic set for
/// another era before Throne of Destiny, the Modern set for any other, and for no era (or one this
/// build does not know), which the client plays as the end of retail.
pub fn required_set(era: Option<&str>) -> SetKind {
    era.and_then(EraId::parse).map_or(SetKind::Modern, needs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every named system is a flag the table has, once, and the rest are left to the era.
    #[test]
    fn the_named_systems_are_flags_of_the_table_and_the_rest_follow_the_era() {
        let all = features();
        for (name, _) in NAMED_SYSTEMS {
            assert_eq!(
                all.iter().filter(|f| f.name == *name).count(),
                1,
                "{name} is one flag"
            );
        }
        assert_eq!(
            all.iter().filter(|f| f.dated.is_some()).count(),
            NAMED_SYSTEMS.len()
        );
        assert!(all.iter().any(|f| f.dated.is_none()), "rules stay folded");
        assert!(
            all.iter()
                .any(|f| f.name == "creation_skill_bonus_as_experience" && f.dated.is_none()),
            "the creation bonus is a rule, folded with the others"
        );
    }

    #[test]
    fn the_eras_are_the_clients_table_latest_first_with_the_set_each_needs() {
        let e = eras();
        assert_eq!(
            e.iter().map(|e| e.name).collect::<Vec<_>>(),
            ["eor", "infiltration", "launch"]
        );
        assert_eq!(e[0].needs, SetKind::Modern);
        assert_eq!(e[1].needs, SetKind::Classic);
        assert_eq!(e[2].needs, SetKind::Original);
        assert!(!e[2].features["trade"]);
        assert!(e[2].features["spell_research"]);
        assert!(e[1].features["trade"]);
        assert!(!e[1].features["aetheria"]);
        assert_eq!(e[0].features.len(), EraFeatures::COUNT);
        assert_eq!(features().len(), EraFeatures::COUNT);
        assert_eq!(features()[1].label, "Consolidated weapon skills");
    }

    #[test]
    fn an_era_in_words_is_the_launchers_label_the_worlds_word_or_the_end_of_retail() {
        assert_eq!(label(Some("launch")), "Launch (1999, experimental)");
        assert_eq!(label(None), "End of Retail");
        assert_eq!(label(Some(" ")), "End of Retail");
        assert_eq!(label(Some("eor")), "End of Retail");
        assert_eq!(label(Some("Infiltration")), "Infiltration (February 2005)");
        assert_eq!(label(Some("dark-majesty")), "dark-majesty");
    }

    #[test]
    fn launch_needs_the_original_set_a_later_pre_tod_era_the_classic_set_and_others_the_modern_one()
    {
        assert_eq!(required_set(Some("launch")), SetKind::Original);
        assert_eq!(required_set(Some("infiltration")), SetKind::Classic);
        assert_eq!(required_set(Some("Infiltration ")), SetKind::Classic);
        assert_eq!(required_set(Some("eor")), SetKind::Modern);
        assert_eq!(required_set(None), SetKind::Modern);
        assert_eq!(
            required_set(Some("dark-majesty")),
            SetKind::Modern,
            "an era this build does not know"
        );
    }
}
