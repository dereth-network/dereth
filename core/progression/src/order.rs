//! What a world builder's tool asks a server to make, what the server says it made, and what the
//! tool adds to a character the server exports: the files that pass between them. All are JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::gear::Slot;
use crate::{Extras, Plan};

/// The version of [`Order`] this crate reads and writes.
pub const ORDER_VERSION: u32 = 2;

/// A character to make: a level and a plan, the gear and the items it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub version: u32,
    /// The recipe it was made from, as its maker identifies it: a server echoes it back, so the
    /// maker knows whether the character is still the recipe's.
    pub hash: String,
    pub level: u32,
    pub plan: Plan,
    /// The heritage and gender, by the creation table's ids; the character's own when not given.
    /// A character of another is given a new body, as creation makes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heritage: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<u32>,
    #[serde(default)]
    pub gear: Gear,
    /// The seed the gear is rolled with: the same seed rolls the same gear.
    #[serde(default)]
    pub seed: u64,
    /// Items given as they are, by server class.
    #[serde(default)]
    pub items: Vec<Item>,
    /// The tier the creatures where it plays drop, which gear for its level is never below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_tier: Option<u32>,
    /// The slots rolled again, or filled with a chosen item instead of loot.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slots: BTreeMap<Slot, SlotOrder>,
    /// What it gained beyond its build: augmentations, luminance, titles and quests.
    #[serde(default, skip_serializing_if = "Extras::is_empty")]
    pub extras: Extras,
}

/// One slot as asked for: rolled again `roll` times, or filled with `class` as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotOrder {
    #[serde(default)]
    pub roll: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<u32>,
}

/// What a character wears and wields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gear {
    /// Nothing given; what it has, it keeps.
    None,
    /// What a new character of its skills is given.
    Starter,
    /// Gear like loot of the tier a character of its level hunts at, or the place drops.
    #[default]
    Level,
    /// A tier higher, the best of more rolls, and jewelry, a cloak and a trinket too.
    Better,
    /// Gear like loot of this tier (1 to 8).
    Tier(u32),
}

/// An item given by its class, and how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub class: u32,
    #[serde(default = "one")]
    pub count: u32,
}

const fn one() -> u32 {
    1
}

/// The version of [`Made`] this crate reads and writes.
pub const MADE_VERSION: u32 = 1;

/// What a server made of an order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Made {
    pub version: u32,
    /// The order's hash.
    pub hash: String,
    /// The character's name and object id on that server.
    pub name: String,
    pub id: u32,
    pub level: u32,
    /// The tier the gear was rolled at; 0 for none.
    #[serde(default)]
    pub tier: u32,
    #[serde(default)]
    pub items: Vec<MadeItem>,
    /// What it all weighs, and what the character carries at full speed.
    #[serde(default)]
    pub burden: u64,
    #[serde(default)]
    pub capacity: u64,
}

/// One item a server made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeItem {
    pub name: String,
    pub class: u32,
    /// Where it is worn or wielded; 0 in the pack.
    #[serde(default)]
    pub slot: u32,
    /// What wielding it takes, as a player reads it; empty for nothing.
    #[serde(default)]
    pub wield: String,
    #[serde(default)]
    pub count: u32,
    /// The slot it fills; `None` for what is carried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gear: Option<Slot>,
}

/// The version of [`ExportNote`] this crate reads and writes.
pub const EXPORT_VERSION: u32 = 1;

/// What a world builder's tool tells a server about a character it exports, for the file's own
/// record: the recipe the character was made from, and the symbol each of the world's own
/// weenies has in its packs (a weenie's number is the world's own, its symbol the packs').
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportNote {
    pub version: u32,
    /// The recipe as its file holds it; empty when the character is no longer what it made.
    #[serde(default)]
    pub recipe: String,
    /// The hash of the recipe the character was last made from.
    #[serde(default)]
    pub hash: String,
    /// The world's own weenies by class, each with its symbol.
    #[serde(default)]
    pub symbols: BTreeMap<u32, String>,
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the files between the world builder and its server)
    use super::*;

    #[test]
    fn an_export_note_names_each_weenie_by_its_class_and_reads_without_its_optional_parts() {
        let note = ExportNote {
            version: EXPORT_VERSION,
            recipe: "level = 80\n".into(),
            hash: "abc".into(),
            symbols: BTreeMap::from([(100_001, "mypack:coolsword".to_owned())]),
        };
        let text = serde_json::to_string(&note).unwrap();
        assert!(text.contains(r#""100001":"mypack:coolsword""#), "{text}");
        assert_eq!(serde_json::from_str::<ExportNote>(&text).unwrap(), note);
        let bare: ExportNote = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert!(bare.symbols.is_empty() && bare.recipe.is_empty());
        assert!(serde_json::from_str::<ExportNote>(r#"{"version":1,"ranks":3}"#).is_err());
    }

    #[test]
    fn an_order_names_its_slots_by_word_and_one_written_before_slots_still_reads() {
        let plan = crate::Archetype::named("melee-heavy").unwrap().plan;
        let mut o = Order {
            version: ORDER_VERSION,
            hash: "h".into(),
            level: 80,
            plan,
            heritage: None,
            gender: None,
            gear: Gear::Better,
            seed: 3,
            items: Vec::new(),
            place_tier: Some(5),
            slots: BTreeMap::new(),
            extras: Extras::default(),
        };
        o.slots.insert(
            Slot::UpperArms,
            SlotOrder {
                roll: 2,
                class: None,
            },
        );
        o.slots.insert(
            Slot::Weapon,
            SlotOrder {
                roll: 0,
                class: Some(3001),
            },
        );
        let text = serde_json::to_string(&o).unwrap();
        assert!(text.contains(r#""upper_arms":{"roll":2}"#), "{text}");
        assert!(text.contains(r#""gear":"better""#), "{text}");
        assert_eq!(serde_json::from_str::<Order>(&text).unwrap(), o);
        let old = r#"{"version":2,"hash":"h","level":5,"plan":{"innate":[100,100,100,10,10,10]},"gear":{"tier":3}}"#;
        let read: Order = serde_json::from_str(old).unwrap();
        assert_eq!(
            (read.gear, read.place_tier, read.slots.len()),
            (Gear::Tier(3), None, 0)
        );
    }
}
