//! What a world names that it doesn't have: a placement's class, a create list's, a generator's,
//! an emote's, a recipe's result, a cook book's recipe, a spell a weenie knows or an emote casts,
//! a placement's linked child.
//!
//! Each is one line, `<what names it>: <what it names> is in no <table>`, in the world's order.

use std::collections::BTreeSet;

use empyrean_content::WorldContent;

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
            class(format!("{me} generator[{i}]"), r.weenie_class_id);
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
