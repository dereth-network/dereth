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
    /// Whether a world may be set to play it from now on: a world that plays an era no longer
    /// offered keeps playing it, and the era is still named and described for it.
    pub offered: bool,
    /// For an era drawn from a Classic set, the iterations its pair's files carry, each a range
    /// with both ends included: which Classic sets its worlds are offered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair: Option<PairIterations>,
    /// The era's table: every system, on or off.
    pub features: BTreeMap<&'static str, bool>,
}

/// The iterations an era's Classic pair carries ([`dereth_primitives::ExpectedDats`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PairIterations {
    pub portal: (u32, u32),
    pub cell: (u32, u32),
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
        EraId::Launch => "Launch (1999)",
        EraId::Infiltration => "Infiltration (February 2005)",
        EraId::Eor => "End of Retail",
        EraId::NewDereth => "New Dereth, Emergence",
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

/// The environment variable that offers New Dereth in any build when it is `1`, and hides it in a
/// development build when it is `0`.
pub const NEW_DERETH_VARIABLE: &str = "DERETH_NEW_DERETH";

/// Whether the launcher offers New Dereth: a hosted world may be set to play it, a world a player
/// adds may be said to, the Library builds its data set and Lyceum fetches its world. On in a
/// development build and off in a release, unless [`NEW_DERETH_VARIABLE`] (`variable`, its value
/// when set) says otherwise.
#[must_use]
pub fn offers_new_dereth(development: bool, variable: Option<&str>) -> bool {
    match variable.map(str::trim) {
        Some("1") => true,
        Some("0") => false,
        _ => development,
    }
}

/// Every era a world can play, latest first: the end of retail is what most worlds play. New
/// Dereth is offered only when `new_dereth` says so ([`offers_new_dereth`]); every other era always is.
#[must_use]
pub fn eras(new_dereth: bool) -> Vec<EraInfo> {
    EraId::ALL
        .iter()
        .rev()
        .map(|&e| EraInfo {
            name: e.name(),
            label: era_label(e),
            needs: needs(e),
            offered: new_dereth || e != EraId::NewDereth,
            pair: pair_iterations(e),
            features: e.features().iter().collect(),
        })
        .collect()
}

/// Whether `era` names an era this build knows.
#[must_use]
pub fn known(era: &str) -> bool {
    EraId::parse(era).is_some()
}

/// Whether a world may be set to play `era`: one the launcher offers ([`eras`]), or `kept`, the era
/// it plays already, which it keeps playing whether or not that is still offered. An era this
/// build does not know never is.
#[must_use]
pub fn may_choose(era: &str, kept: Option<&str>, new_dereth: bool) -> bool {
    let Some(id) = EraId::parse(era) else {
        return false;
    };
    let kept = kept.and_then(EraId::parse) == Some(id);
    kept || eras(new_dereth)
        .iter()
        .any(|e| e.offered && e.name == id.name())
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
        ContainerEra::Classic => SetKind::Classic,
        ContainerEra::Modern => SetKind::Modern,
    }
}

fn pair_iterations(era: EraId) -> Option<PairIterations> {
    let d = era.expected_dats();
    if d.container != ContainerEra::Classic {
        return None;
    }
    Some(PairIterations {
        portal: d.portal.unwrap_or((0, u32::MAX)),
        cell: d.cell.unwrap_or((0, u32::MAX)),
    })
}

/// The data set a world's world is drawn from: the Classic set for an era before Throne of Destiny
/// (Launch's the 1999 pair, [`crate::datset::pair_era`]), the Modern set for any other, and for no
/// era (or one this build does not know), which the client plays as the end of retail.
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
        let mut e = eras(true);
        // New Dereth, the latest, first, on the later files.
        let nd = e.remove(0);
        assert_eq!(
            (nd.name, nd.needs, nd.offered),
            ("newdereth", SetKind::Modern, true)
        );
        assert_eq!(
            e.iter().map(|e| e.name).collect::<Vec<_>>(),
            ["eor", "infiltration", "launch"]
        );
        assert_eq!(e[0].needs, SetKind::Modern);
        assert_eq!(e[1].needs, SetKind::Classic);
        assert_eq!(e[2].needs, SetKind::Classic);
        assert_eq!(e[0].pair, None);
        assert_eq!(e[2].pair.map(|p| p.portal), Some((102, 123)));
        assert_eq!(e[1].pair.map(|p| p.cell), Some((1537, u32::MAX)));
        assert!(!e[2].features["trade"]);
        assert!(e[2].features["spell_research"]);
        assert!(e[1].features["trade"]);
        assert!(!e[1].features["aetheria"]);
        assert_eq!(e[0].features.len(), EraFeatures::COUNT);
        assert_eq!(features().len(), EraFeatures::COUNT);
        assert_eq!(features()[1].label, "Consolidated weapon skills");
    }

    /// Hidden, New Dereth is still named and described for a world that plays it, but not offered;
    /// the other eras are offered either way.
    #[test]
    fn new_dereth_is_offered_only_when_the_launcher_offers_it_and_still_described_when_not() {
        let hidden = eras(false);
        assert_eq!(hidden.len(), eras(true).len());
        let nd = hidden
            .iter()
            .find(|e| e.name == "newdereth")
            .expect("described");
        assert!(!nd.offered);
        assert_eq!(nd.label, "New Dereth, Emergence");
        assert_eq!(nd.features.len(), EraFeatures::COUNT);
        assert!(hidden
            .iter()
            .filter(|e| e.name != "newdereth")
            .all(|e| e.offered));
    }

    /// A release offers New Dereth only when the variable is `1`; a development build offers it
    /// unless the variable is `0`.
    #[test]
    fn new_dereth_is_on_in_a_development_build_off_in_a_release_and_the_variable_says_otherwise() {
        assert!(offers_new_dereth(true, None), "development");
        assert!(!offers_new_dereth(false, None), "release");
        assert!(offers_new_dereth(false, Some("1")));
        assert!(offers_new_dereth(false, Some(" 1 ")));
        assert!(!offers_new_dereth(false, Some("yes")), "only 1 turns it on");
        assert!(!offers_new_dereth(true, Some("0")));
        assert!(offers_new_dereth(true, Some("")));
    }

    /// A world may be set to an offered era, and keeps the one it plays even when that is no
    /// longer offered.
    #[test]
    fn a_world_keeps_new_dereth_when_it_is_hidden_but_no_other_world_is_set_to_it() {
        assert!(may_choose("newdereth", None, true));
        assert!(!may_choose("newdereth", None, false));
        assert!(!may_choose("newdereth", Some("eor"), false));
        assert!(may_choose("newdereth", Some("newdereth"), false));
        assert!(may_choose("eor", Some("newdereth"), false));
        assert!(may_choose("launch", None, false));
        assert!(
            !may_choose("gold", Some("gold"), true),
            "no era this build knows"
        );
    }

    #[test]
    fn an_era_in_words_is_the_launchers_label_the_worlds_word_or_the_end_of_retail() {
        assert_eq!(label(Some("launch")), "Launch (1999)");
        assert_eq!(label(None), "End of Retail");
        assert_eq!(label(Some(" ")), "End of Retail");
        assert_eq!(label(Some("eor")), "End of Retail");
        assert_eq!(label(Some("Infiltration")), "Infiltration (February 2005)");
        assert_eq!(label(Some("dark-majesty")), "dark-majesty");
    }

    #[test]
    fn an_era_before_throne_of_destiny_needs_the_classic_set_and_others_the_modern_one() {
        assert_eq!(required_set(Some("launch")), SetKind::Classic);
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
