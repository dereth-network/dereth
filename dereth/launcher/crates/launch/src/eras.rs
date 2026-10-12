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
}

fn era_label(era: EraId) -> &'static str {
    match era {
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
        })
        .collect()
}

fn needs(era: EraId) -> SetKind {
    match era.container_era() {
        ContainerEra::Classic => SetKind::Classic,
        _ => SetKind::Modern,
    }
}

/// The data set a world's world is drawn from: the Classic set for an era before Throne of
/// Destiny, the Modern set for any other, and for no era (or one this build does not know), which
/// the client plays as the end of retail.
pub fn required_set(era: Option<&str>) -> SetKind {
    era.and_then(EraId::parse).map_or(SetKind::Modern, needs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eras_are_the_clients_table_latest_first_with_the_set_each_needs() {
        let e = eras();
        assert_eq!(
            e.iter().map(|e| e.name).collect::<Vec<_>>(),
            ["eor", "infiltration"]
        );
        assert_eq!(e[0].needs, SetKind::Modern);
        assert_eq!(e[1].needs, SetKind::Classic);
        assert!(e[1].features["trade"]);
        assert!(!e[1].features["aetheria"]);
        assert_eq!(e[0].features.len(), EraFeatures::COUNT);
        assert_eq!(features().len(), EraFeatures::COUNT);
        assert_eq!(features()[1].label, "Consolidated weapon skills");
    }

    #[test]
    fn an_era_in_words_is_the_launchers_label_the_worlds_word_or_the_end_of_retail() {
        assert_eq!(label(None), "End of Retail");
        assert_eq!(label(Some(" ")), "End of Retail");
        assert_eq!(label(Some("eor")), "End of Retail");
        assert_eq!(label(Some("Infiltration")), "Infiltration (February 2005)");
        assert_eq!(label(Some("dark-majesty")), "dark-majesty");
    }

    #[test]
    fn a_pre_tod_era_needs_the_classic_set_and_anything_else_the_modern_one() {
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
