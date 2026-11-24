//! A world in its one order, with its ids numbered by rule: rows say nothing of the ids a dump gave
//! them, so a fold numbers them alike every time.
//!
//! Every table is put in its key's order: weenies by class, placements by guid, encounters by
//! landblock and cell, quests and events by name without case (then with it), spells and recipes by id, the
//! treasure tables by type (a wielded type's rows keeping their own order). A weenie's typed
//! properties go by type; a list (a create list, an emote's actions) keeps its order. Then each
//! table's ids run from 1 in that order, and each child names its parent's.

use empyrean_content::pack::{Pack, TableId};
use empyrean_content::{PackError, WorldContent};

/// Put `c` in its one order and number its ids.
#[allow(clippy::too_many_lines)] // one pass per table, in the content's own order
pub fn number(c: &mut WorldContent) {
    c.weenies.sort_by_key(|w| w.class_id);
    let mut n = Counters::default();
    for w in &mut c.weenies {
        let o = w.class_id;
        w.weenie_properties_int.sort_by_key(|p| p.r#type);
        w.weenie_properties_int64.sort_by_key(|p| p.r#type);
        w.weenie_properties_bool.sort_by_key(|p| p.r#type);
        w.weenie_properties_float.sort_by_key(|p| p.r#type);
        w.weenie_properties_string.sort_by_key(|p| p.r#type);
        w.weenie_properties_did.sort_by_key(|p| p.r#type);
        w.weenie_properties_iid.sort_by_key(|p| p.r#type);
        w.weenie_properties_position
            .sort_by_key(|p| p.position_type);
        w.weenie_properties_attribute.sort_by_key(|p| p.r#type);
        w.weenie_properties_attribute_2nd.sort_by_key(|p| p.r#type);
        w.weenie_properties_skill.sort_by_key(|p| p.r#type);
        w.weenie_properties_body_part.sort_by_key(|p| p.key);
        w.weenie_properties_spell_book.sort_by_key(|p| p.spell);
        macro_rules! own {
            ($rows:expr, $counter:ident) => {
                for r in &mut $rows {
                    n.$counter += 1;
                    r.id = n.$counter;
                    r.object_id = o;
                }
            };
        }
        own!(w.weenie_properties_anim_part, anim_part);
        own!(w.weenie_properties_attribute, attribute);
        own!(w.weenie_properties_attribute_2nd, attribute_2nd);
        own!(w.weenie_properties_body_part, body_part);
        own!(w.weenie_properties_book_page_data, book_page);
        own!(w.weenie_properties_bool, bool_);
        own!(w.weenie_properties_create_list, create_list);
        own!(w.weenie_properties_did, did);
        own!(w.weenie_properties_event_filter, event_filter);
        own!(w.weenie_properties_float, float);
        own!(w.weenie_properties_generator, generator);
        own!(w.weenie_properties_iid, iid);
        own!(w.weenie_properties_int, int);
        own!(w.weenie_properties_int64, int64);
        own!(w.weenie_properties_palette, palette);
        own!(w.weenie_properties_position, position);
        own!(w.weenie_properties_skill, skill);
        own!(w.weenie_properties_spell_book, spell_book);
        own!(w.weenie_properties_string, string);
        own!(w.weenie_properties_texture_map, texture_map);
        if let Some(b) = &mut w.weenie_properties_book {
            n.book += 1;
            b.id = n.book;
            b.object_id = o;
        }
        for e in &mut w.weenie_properties_emote {
            n.emote += 1;
            e.id = n.emote;
            e.object_id = o;
            for a in &mut e.weenie_properties_emote_action {
                n.emote_action += 1;
                a.id = n.emote_action;
                a.emote_id = e.id;
            }
        }
    }

    c.landblock_instances.sort_by_key(|i| i.guid);
    for i in &mut c.landblock_instances {
        i.landblock = None;
        for l in &mut i.landblock_instance_link {
            n.link += 1;
            l.id = n.link;
            l.parent_guid = i.guid;
        }
    }
    c.encounters
        .sort_by_key(|e| (e.landblock, e.cell_x, e.cell_y));
    renumber(&mut c.encounters, |e| &mut e.id);
    c.cook_books
        .sort_by_key(|k| (k.source_wcid, k.target_wcid, k.recipe_id));
    renumber(&mut c.cook_books, |k| &mut k.id);
    c.recipes.sort_by_key(|r| r.id);
    for r in &mut c.recipes {
        for m in &mut r.recipe_mod {
            n.recipe_mod += 1;
            m.id = n.recipe_mod;
            m.recipe_id = r.id;
            macro_rules! of_mod {
                ($rows:expr, $counter:ident) => {
                    for x in &mut $rows {
                        n.$counter += 1;
                        x.id = n.$counter;
                        x.recipe_mod_id = m.id;
                    }
                };
            }
            of_mod!(m.recipe_mods_bool, mods_bool);
            of_mod!(m.recipe_mods_did, mods_did);
            of_mod!(m.recipe_mods_float, mods_float);
            of_mod!(m.recipe_mods_iid, mods_iid);
            of_mod!(m.recipe_mods_int, mods_int);
            of_mod!(m.recipe_mods_string, mods_string);
        }
        macro_rules! of_recipe {
            ($rows:expr, $counter:ident) => {
                for x in &mut $rows {
                    n.$counter += 1;
                    x.id = n.$counter;
                    x.recipe_id = r.id;
                }
            };
        }
        of_recipe!(r.recipe_requirements_bool, req_bool);
        of_recipe!(r.recipe_requirements_did, req_did);
        of_recipe!(r.recipe_requirements_float, req_float);
        of_recipe!(r.recipe_requirements_iid, req_iid);
        of_recipe!(r.recipe_requirements_int, req_int);
        of_recipe!(r.recipe_requirements_string, req_string);
    }
    c.recipe_vital_requirements.sort_by_key(|v| v.id);
    c.events
        .sort_by_key(|e| (e.name.to_lowercase(), e.name.clone()));
    renumber(&mut c.events, |e| &mut e.id);
    c.quests
        .sort_by_key(|q| (q.name.to_lowercase(), q.name.clone()));
    renumber(&mut c.quests, |q| &mut q.id);
    c.points_of_interest
        .sort_by_key(|p| (p.name.to_lowercase(), p.name.clone()));
    renumber(&mut c.points_of_interest, |p| &mut p.id);
    c.house_portals.sort_by_key(|h| h.house_id);
    renumber(&mut c.house_portals, |h| &mut h.id);
    c.spells.sort_by_key(|s| s.id);
    c.treasure_death.sort_by_key(|t| t.treasure_type);
    renumber(&mut c.treasure_death, |t| &mut t.id);
    c.grouped_treasure.sort_by_key(|t| t.treasure_type);
    renumber(&mut c.grouped_treasure, |t| &mut t.id);
    renumber(&mut c.loot_policies, |p| &mut p.id);
    // A wielded type's rows are a list: its order is what a set means.
    c.treasure_wielded.sort_by_key(|t| t.treasure_type);
    renumber(&mut c.treasure_wielded, |t| &mut t.id);
    c.treasure_gem_count
        .sort_by_key(|t| (t.gem_code, t.tier, t.count));
    renumber(&mut c.treasure_gem_count, |t| &mut t.id);
    c.treasure_material_base
        .sort_by_key(|t| (t.material_code, t.tier, t.material_id));
    renumber(&mut c.treasure_material_base, |t| &mut t.id);
    c.treasure_material_color
        .sort_by_key(|t| (t.material_id, t.color_code, t.palette_template));
    renumber(&mut c.treasure_material_color, |t| &mut t.id);
    c.treasure_material_groups
        .sort_by_key(|t| (t.material_group, t.tier, t.material_id));
    renumber(&mut c.treasure_material_groups, |t| &mut t.id);
    renumber(&mut c.versions, |v| &mut v.id);
}

fn renumber<T>(rows: &mut [T], id: impl Fn(&mut T) -> &mut u32) {
    for (n, r) in (1u32..).zip(rows.iter_mut()) {
        *id(r) = n;
    }
}

/// Each child table's last id given.
#[derive(Debug, Default)]
struct Counters {
    anim_part: u32,
    attribute: u32,
    attribute_2nd: u32,
    body_part: u32,
    book: u32,
    book_page: u32,
    bool_: u32,
    create_list: u32,
    did: u32,
    emote: u32,
    emote_action: u32,
    event_filter: u32,
    float: u32,
    generator: u32,
    iid: u32,
    int: u32,
    int64: u32,
    palette: u32,
    position: u32,
    skill: u32,
    spell_book: u32,
    string: u32,
    texture_map: u32,
    link: u32,
    recipe_mod: u32,
    mods_bool: u32,
    mods_did: u32,
    mods_float: u32,
    mods_iid: u32,
    mods_int: u32,
    mods_string: u32,
    req_bool: u32,
    req_did: u32,
    req_float: u32,
    req_iid: u32,
    req_int: u32,
    req_string: u32,
}

/// Every table of a pack, as the world it was made from (its derived index left out).
pub fn from_pack(p: &Pack) -> Result<WorldContent, PackError> {
    fn rows<T: empyrean_content::pack::Codec>(p: &Pack, t: TableId) -> Result<Vec<T>, PackError> {
        Ok(p.all::<T>(t)?.into_iter().map(|(_, r)| r).collect())
    }
    fn grouped<T>(p: &Pack, t: TableId) -> Result<Vec<T>, PackError>
    where
        Vec<T>: empyrean_content::pack::Codec,
    {
        Ok(p.all::<Vec<T>>(t)?
            .into_iter()
            .flat_map(|(_, r)| r)
            .collect())
    }
    Ok(WorldContent {
        weenies: rows(p, TableId::WEENIE)?,
        landblock_instances: grouped(p, TableId::LANDBLOCK_INSTANCE)?,
        encounters: grouped(p, TableId::ENCOUNTER)?,
        cook_books: grouped(p, TableId::COOK_BOOK)?,
        recipes: rows(p, TableId::RECIPE)?,
        recipe_vital_requirements: rows(p, TableId::RECIPE_VITAL)?,
        events: rows(p, TableId::EVENT)?,
        house_portals: rows(p, TableId::HOUSE_PORTAL)?,
        points_of_interest: rows(p, TableId::POINTS_OF_INTEREST)?,
        quests: rows(p, TableId::QUEST)?,
        spells: rows(p, TableId::SPELL)?,
        treasure_death: rows(p, TableId::TREASURE_DEATH)?,
        grouped_treasure: rows(p, TableId::GROUPED_TREASURE)?,
        loot_policies: rows(p, TableId::LOOT_POLICY)?,
        treasure_gem_count: rows(p, TableId::TREASURE_GEM_COUNT)?,
        treasure_material_base: rows(p, TableId::TREASURE_MATERIAL_BASE)?,
        treasure_material_color: rows(p, TableId::TREASURE_MATERIAL_COLOR)?,
        treasure_material_groups: rows(p, TableId::TREASURE_MATERIAL_GROUPS)?,
        treasure_wielded: rows(p, TableId::TREASURE_WIELDED)?,
        versions: rows(p, TableId::VERSION)?,
        era: p.era(),
    })
}
