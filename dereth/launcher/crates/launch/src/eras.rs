//! The eras a world can play and the systems each has, as the launcher offers them, from the
//! client's own era table (`dereth_primitives::EraId`, `EraFeatures`).
//!
//! A world that names its era and systems is taken at its word. For one that does not, the player
//! chooses the era and may turn systems on or off over its table; the launcher keeps the choice per
//! world ([`EraChoice`]) and hands it to the Dereth client as `--era` and `--era-features`.

use std::collections::BTreeMap;

use dereth_primitives::{ContainerEra, EraFeatureOverrides, EraFeatures, EraId};
use serde::{Deserialize, Serialize};

use crate::datset::SetKind;

/// One era, for a drop-down.
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

/// One system, for a check box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureInfo {
    pub name: &'static str,
    pub label: String,
}

fn era_label(era: EraId) -> &'static str {
    match era {
        EraId::Infiltration => "Infiltration (February 2005)",
        EraId::Eor => "End of retail",
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

/// What the player chose for a world that does not say its era or systems.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EraChoice {
    /// The era, as [`EraInfo::name`] spells it. `None`: not chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub era: Option<String>,
    /// The systems turned on or off over the era's table, by name. Only those that differ from
    /// the table are kept.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<String, bool>,
}

impl EraChoice {
    /// The table the choice is made over: the chosen era's, else the end of retail's, which is
    /// what the client plays with no era named.
    fn table(&self) -> EraFeatures {
        self.era
            .as_deref()
            .and_then(EraId::parse)
            .unwrap_or_default()
            .features()
    }

    /// Choose the era. Choosing another one starts its systems from its own table.
    pub fn set_era(&mut self, era: Option<&str>) {
        let era = era.and_then(EraId::parse).map(|e| e.name().to_owned());
        if era != self.era {
            self.era = era;
            self.features.clear();
        }
    }

    /// Turn the system `name` on or off. Answers whether `name` is a system's. A value equal to
    /// the table's is not kept.
    pub fn set_feature(&mut self, name: &str, on: bool) -> bool {
        let Some(default) = self.table().get(name) else {
            return false;
        };
        if on == default {
            self.features.remove(name);
        } else {
            self.features.insert(name.to_owned(), on);
        }
        true
    }

    /// The `--era-features` text: each system that differs from the table. `None` when none does.
    pub fn features_text(&self) -> Option<String> {
        let mut o = EraFeatureOverrides::default();
        for (name, on) in &self.features {
            o.set(name, *on);
        }
        (!o.is_empty()).then(|| o.to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.era.is_none() && self.features.is_empty()
    }
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

    #[test]
    fn a_choice_keeps_only_what_differs_from_its_eras_table() {
        let mut c = EraChoice::default();
        assert!(c.is_empty());
        // No era yet: the end of retail's table, where trade is on.
        assert!(c.set_feature("trade", true));
        assert_eq!(c.features_text(), None, "the table's own value is not kept");
        assert!(c.set_feature("trade", false));
        assert_eq!(c.features_text().as_deref(), Some("trade=false"));
        assert!(!c.set_feature("no_such_system", true));

        c.set_era(Some("infiltration"));
        assert_eq!(c.era.as_deref(), Some("infiltration"));
        assert!(
            c.features.is_empty(),
            "another era starts from its own table"
        );
        assert!(c.set_feature("aetheria", true));
        assert!(c.set_feature("chess", false));
        assert_eq!(
            c.features_text().as_deref(),
            Some("aetheria=true,chess=false")
        );
        c.set_era(Some("infiltration"));
        assert_eq!(c.features.len(), 2, "the same era again changes nothing");
        c.set_era(Some("nonsense"));
        assert_eq!(c.era, None, "an era this build does not know is no choice");
    }

    #[test]
    fn the_text_a_choice_makes_is_what_the_client_reads() {
        let mut c = EraChoice::default();
        c.set_era(Some("infiltration"));
        c.set_feature("aetheria", true);
        let (o, unknown) = EraFeatureOverrides::parse(&c.features_text().unwrap()).unwrap();
        assert!(unknown.is_empty());
        let f = o.apply(EraId::Infiltration.features());
        assert!(f.aetheria && f.trade && !f.ratings);
    }
}
