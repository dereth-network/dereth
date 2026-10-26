//! Kits: what a playtest character carries besides its gear, as a player of its kind took
//! hunting: healing kits, potions, food and money for everyone; mana stones, charges, scarabs
//! and tapers for a mage; arrows for an archer. Each kit is Lyceum's data (`kits.toml`), its
//! items by server class and level; a recipe takes its archetype's kit unless it names another
//! (or none).

use dereth_progression::order::Item;
use serde::Deserialize;

use crate::characters::Recipe;

/// One item of a kit, for the levels from `from` to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitItem {
    pub class: u32,
    pub count: u32,
    #[serde(default)]
    pub from: u32,
    #[serde(default = "most")]
    pub to: u32,
}

const fn most() -> u32 {
    u32::MAX
}

/// A kit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kit {
    pub id: String,
    pub name: String,
    /// Another kit whose items it carries too.
    #[serde(default)]
    pub with: Option<String>,
    pub items: Vec<KitItem>,
}

#[derive(Deserialize)]
struct File {
    kit: Vec<Kit>,
}

/// The id of the kit of none.
pub const NONE: &str = "none";

/// Every kit, in the order the tab offers them, read from the table once.
///
/// # Panics
/// Never: the table is part of the editor, and a test reads it.
#[must_use]
pub fn builtin() -> &'static [Kit] {
    static ALL: std::sync::OnceLock<Vec<Kit>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        toml::from_str::<File>(include_str!("kits.toml"))
            .expect("the built-in kits")
            .kit
    })
}

/// Kit `id`.
#[must_use]
pub fn named(id: &str) -> Option<&'static Kit> {
    builtin().iter().find(|k| k.id == id)
}

/// The kit of an archetype whose main skill is `main` (a skill as an archetype names it): a
/// mage's for a magic skill, an archer's for a missile weapon, else an adventurer's.
#[must_use]
pub fn of_skill(main: &str) -> &'static str {
    let main = main.to_ascii_lowercase();
    if main.contains("magic") {
        "mage"
    } else if ["missile", "bow", "crossbow", "thrown"]
        .iter()
        .any(|w| main.contains(w))
    {
        "archer"
    } else {
        "adventurer"
    }
}

impl Kit {
    /// What it gives a character of `level`, its `with` kit's first.
    #[must_use]
    pub fn items_at(&self, level: u32) -> Vec<Item> {
        let mut out = self
            .with
            .as_deref()
            .and_then(named)
            .map(|k| k.items_at(level))
            .unwrap_or_default();
        out.extend(
            self.items
                .iter()
                .filter(|i| (i.from..=i.to).contains(&level))
                .map(|i| Item {
                    class: i.class,
                    count: i.count,
                }),
        );
        out
    }
}

impl Recipe {
    /// The id of the kit it carries: its own, else its archetype's.
    #[must_use]
    pub fn kit_id(&self) -> String {
        self.kit.clone().unwrap_or_else(|| {
            let main = self
                .plan()
                .ok()
                .and_then(|p| p.specialized.first().cloned())
                .unwrap_or_default();
            of_skill(&main).to_owned()
        })
    }

    /// What its kit gives it at its level: the items whose class `has` says the world has.
    #[must_use]
    pub fn kit_items(&self, has: impl Fn(u32) -> bool) -> Vec<Item> {
        named(&self.kit_id())
            .map(|k| k.items_at(self.level))
            .unwrap_or_default()
            .into_iter()
            .filter(|i| has(i.class))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character's kits)
    use super::*;

    #[test]
    fn the_kits_read_and_each_carries_healing_food_and_money_at_every_level() {
        let all = builtin();
        let ids: Vec<&str> = all.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(ids, ["adventurer", "mage", "archer"]);
        for k in all {
            for level in [
                1, 14, 15, 29, 30, 49, 50, 80, 99, 100, 125, 126, 150, 179, 180, 275,
            ] {
                let items = k.items_at(level);
                let has = |c: u32| items.iter().any(|i| i.class == c);
                assert!(
                    [629, 631, 632, 9229].iter().filter(|c| has(**c)).count() == 1,
                    "one kind of healing kit: {} at {level}",
                    k.id
                );
                assert!(has(259), "food");
                assert!(
                    [273, 2625, 2627].iter().filter(|c| has(**c)).count() == 1,
                    "money"
                );
            }
        }
        let mage = named("mage").unwrap();
        let scarab = |level| {
            mage.items_at(level)
                .into_iter()
                .find(|i| [686, 687, 690, 8897, 37155].contains(&i.class))
                .map(|i| i.class)
        };
        // The scarab of the spell level the buffs go by.
        assert_eq!(
            [1, 15, 40, 100, 180].map(scarab),
            [Some(686), Some(687), Some(690), Some(8897), Some(37155)]
        );
    }

    #[test]
    fn a_recipe_carries_its_archetypes_kit_unless_it_names_one_and_only_what_the_world_has() {
        let at = |archetype: &str| Recipe {
            archetype: archetype.into(),
            level: 50,
            ..Recipe::default()
        };
        assert_eq!(at("melee-heavy").kit_id(), "adventurer");
        assert_eq!(at("war-mage").kit_id(), "mage");
        assert_eq!(at("archer").kit_id(), "archer");
        let none = Recipe {
            kit: Some(NONE.into()),
            ..at("war-mage")
        };
        assert!(none.kit_items(|_| true).is_empty());
        let archer = at("archer");
        assert!(archer.kit_items(|_| true).iter().any(|i| i.class == 300));
        assert!(
            !archer
                .kit_items(|c| c != 300)
                .iter()
                .any(|i| i.class == 300),
            "a class the world lacks is left out"
        );
    }
}
