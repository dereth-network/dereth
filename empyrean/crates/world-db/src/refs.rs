//! What a world names that it doesn't have: a placement's class, a create list's, a generator's,
//! an emote's, a recipe's result, a cook book's recipe, a spell a weenie knows or an emote casts,
//! a placement's linked child, a weenie's death or wielded treasure type, a treasure generator's
//! treasure type.
//!
//! Each is one line, `<what names it>: <what it names> is in no <table>`, in the world's order.
//!
//! A generator row with the Treasure flag (`where_create` 64) names no class: its id is a treasure
//! type, which the server looks up as death treasure and then as wielded treasure. Under the
//! Infiltration era's loot rules a chest, a container or a ground generator rolls a few death
//! treasure profiles as another ([`LootRules::moved_profile`]): the profile it rolls is the one
//! the world must have, since the one it names is never looked up as death treasure.

use std::collections::BTreeSet;

use empyrean_common::era::{EraExt, LootRules};
use empyrean_content::models::world::Weenie;
use empyrean_content::WorldContent;
use empyrean_entity::enums::{PropertyDataId, RegenLocationType, WeenieType};

/// The loot rules `c`'s treasure is rolled under: its era's, or its own rules' over them.
fn loot_rules(c: &WorldContent) -> LootRules {
    let era = c.era.rules();
    c.rules
        .as_ref()
        .and_then(|r| r.apply(era).ok())
        .unwrap_or(era)
        .loot_rules
}

/// The death treasure profile `w` rolls for `id` under `rules`: what a creature, a vendor, a cow
/// or a pet drops is its own, and anything else's may be moved.
fn rolled(rules: LootRules, w: &Weenie, id: u32) -> u32 {
    let creature = u32::try_from(w.r#type).is_ok_and(|t| {
        [
            WeenieType::Creature,
            WeenieType::Vendor,
            WeenieType::Cow,
            WeenieType::Pet,
            WeenieType::CombatPet,
        ]
        .contains(&WeenieType(t))
    });
    if creature {
        id
    } else {
        rules.moved_profile(id)
    }
}

/// Whether a generator row with `where_create` makes treasure: its id is a treasure type.
fn treasure_generator(where_create: u32) -> bool {
    where_create & RegenLocationType::Treasure.0 != 0
}

/// Every reference in `c` to something `c` doesn't have.
#[must_use]
#[allow(clippy::too_many_lines)] // one check per kind of reference, in the content's own order
pub fn dangling(c: &WorldContent) -> Vec<String> {
    let classes: BTreeSet<u32> = c.weenies.iter().map(|w| w.class_id).collect();
    let spells: BTreeSet<u32> = c.spells.iter().map(|s| s.id).collect();
    let recipes: BTreeSet<u32> = c.recipes.iter().map(|r| r.id).collect();
    let guids: BTreeSet<u32> = c.landblock_instances.iter().map(|i| i.guid).collect();
    let mut out = Vec::new();
    let mut class = |by: String, id: u32| {
        if id != 0 && !classes.contains(&id) {
            out.push(format!("{by}: class {id} is in no weenie"));
        }
    };
    for w in &c.weenies {
        let me = format!("weenie {}", w.class_id);
        for (i, r) in w.weenie_properties_create_list.iter().enumerate() {
            class(format!("{me} create_list[{i}]"), r.weenie_class_id);
        }
        for (i, r) in w.weenie_properties_generator.iter().enumerate() {
            // A treasure generator's id is a treasure type, checked with the treasure below.
            if !treasure_generator(r.where_create) {
                class(format!("{me} generator[{i}]"), r.weenie_class_id);
            }
        }
        for (i, e) in w.weenie_properties_emote.iter().enumerate() {
            if let Some(id) = e.weenie_class_id {
                class(format!("{me} emote[{i}]"), id);
            }
            for (j, a) in e.weenie_properties_emote_action.iter().enumerate() {
                if let Some(id) = a.weenie_class_id {
                    class(format!("{me} emote[{i}].action[{j}]"), id);
                }
            }
        }
    }
    for i in &c.landblock_instances {
        class(format!("spawn {:#010X}", i.guid), i.weenie_class_id);
    }
    for e in &c.encounters {
        class(
            format!(
                "encounter {:04X} {},{}",
                e.landblock & 0xFFFF,
                e.cell_x,
                e.cell_y
            ),
            e.weenie_class_id,
        );
    }
    for k in &c.cook_books {
        let me = format!(
            "cook-book {} {} {}",
            k.recipe_id, k.source_wcid, k.target_wcid
        );
        class(me.clone(), k.source_wcid);
        class(me, k.target_wcid);
    }
    for r in &c.recipes {
        let me = format!("recipe {}", r.id);
        class(me.clone(), r.success_wcid);
        class(me, r.fail_wcid);
    }
    for t in &c.treasure_wielded {
        class(
            format!("wielded-treasure {}", t.treasure_type),
            t.weenie_class_id,
        );
    }
    for w in &c.weenies {
        let me = format!("weenie {}", w.class_id);
        for s in &w.weenie_properties_spell_book {
            if let Ok(id) = u32::try_from(s.spell) {
                if !spells.contains(&id) {
                    out.push(format!("{me} spell_book: spell {id} is in no spell"));
                }
            }
        }
        for (i, e) in w.weenie_properties_emote.iter().enumerate() {
            for (j, a) in e.weenie_properties_emote_action.iter().enumerate() {
                if let Some(id) = a
                    .spell_id
                    .and_then(|id| u32::try_from(id).ok())
                    .filter(|&id| id != 0)
                {
                    if !spells.contains(&id) {
                        out.push(format!(
                            "{me} emote[{i}].action[{j}]: spell {id} is in no spell"
                        ));
                    }
                }
            }
        }
    }
    // A death treasure type is either table's: ACE's death treasure or the source's grouped. A
    // treasure generator's is too, or else a wielded treasure type, which the server tries next.
    let treasure: BTreeSet<u32> = c
        .treasure_death
        .iter()
        .map(|t| t.treasure_type)
        .chain(c.grouped_treasure.iter().map(|t| t.treasure_type))
        .collect();
    let wielded: BTreeSet<u32> = c.treasure_wielded.iter().map(|t| t.treasure_type).collect();
    let rules = loot_rules(c);
    // `id` as `w` names it, and as the era's loot rules roll it when they move it.
    let named = |w: &Weenie, id: u32| match rolled(rules, w, id) {
        r if r == id => format!("treasure type {id}"),
        r => format!("treasure type {id} (rolled as {r} under the era's loot rules)"),
    };
    for w in &c.weenies {
        for d in &w.weenie_properties_did {
            if d.value == 0 {
                continue;
            }
            if d.r#type == PropertyDataId::DeathTreasureType.0
                && !treasure.contains(&rolled(rules, w, d.value))
            {
                out.push(format!(
                    "weenie {} did.DeathTreasureType: {} is in no death-treasure or grouped-treasure",
                    w.class_id,
                    named(w, d.value)
                ));
            } else if d.r#type == PropertyDataId::WieldedTreasureType.0
                && !wielded.contains(&d.value)
            {
                out.push(format!(
                    "weenie {} did.WieldedTreasureType: treasure type {} is in no wielded-treasure",
                    w.class_id, d.value
                ));
            }
        }
        for (i, g) in w.weenie_properties_generator.iter().enumerate() {
            let id = g.weenie_class_id;
            if treasure_generator(g.where_create)
                && id != 0
                && !treasure.contains(&rolled(rules, w, id))
                && !wielded.contains(&id)
            {
                out.push(format!(
                    "weenie {} generator[{i}]: {} is in no death-treasure, grouped-treasure or wielded-treasure",
                    w.class_id,
                    named(w, id)
                ));
            }
        }
    }
    for k in &c.cook_books {
        if !recipes.contains(&k.recipe_id) {
            out.push(format!(
                "cook-book {} {} {}: recipe {} is in no recipe",
                k.recipe_id, k.source_wcid, k.target_wcid, k.recipe_id
            ));
        }
    }
    for v in &c.recipe_vital_requirements {
        if !recipes.contains(&v.id) {
            out.push(format!(
                "recipe-vital {}: recipe {} is in no recipe",
                v.id, v.id
            ));
        }
    }
    for i in &c.landblock_instances {
        for l in &i.landblock_instance_link {
            if !guids.contains(&l.child_guid) {
                out.push(format!(
                    "spawn {:#010X}: its link {:#010X} is in no placement",
                    i.guid, l.child_guid
                ));
            }
        }
    }
    out
}
