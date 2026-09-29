//! Every table of ACE's world database, as typed rows straight from the dump, and their assembly
//! into [`WorldContent`] (each child row placed inside its parent).
//!
//! The dump lists every table in primary-key order, and children are appended to their parents in
//! that order: this is the order EF's `context.X.Load()` fixes a navigation collection up in.

use std::collections::{BTreeMap, HashMap};

use crate::error::ImportError;
use crate::import::mysqldump::{DumpSink, Row, TableDef};
use crate::import::WorldContent;
use crate::models::world::*;
use crate::records::FromRow;

macro_rules! world_rows {
    ($($field:ident: $ty:ty,)*) => {
        /// Every row of every world table, as read.
        #[derive(Debug, Default)]
        pub struct WorldRows {
            $( pub $field: Vec<$ty>, )*
            /// Tables the dump declares that are not ACE world tables: `(name, rows)`.
            pub unknown_tables: BTreeMap<String, u64>,
            /// `(table, column)` pairs the dump has and no model reads.
            pub unread_columns: Vec<(String, String)>,
            /// The tables whose `CREATE TABLE` was seen.
            pub declared: Vec<String>,
        }

        /// The dump table names of every ACE world table, in model order.
        pub const WORLD_TABLES: &[&str] = &[$(<$ty as FromRow>::TABLE),*];

        impl WorldRows {
            /// Rows read, per dump table.
            #[must_use]
            pub fn counts(&self) -> Vec<(&'static str, u64)> {
                vec![$( (<$ty as FromRow>::TABLE, self.$field.len() as u64), )*]
            }

            fn columns_of(table: &str) -> Option<&'static [&'static str]> {
                $( if table == <$ty as FromRow>::TABLE { return Some(<$ty as FromRow>::COLUMNS); } )*
                None
            }

            fn push(&mut self, row: &Row<'_>) -> Result<bool, ImportError> {
                $( if row.table == <$ty as FromRow>::TABLE {
                    self.$field.push(<$ty as FromRow>::from_row(row)?);
                    return Ok(true);
                } )*
                Ok(false)
            }
        }
    };
}

world_rows! {
    cook_book: CookBook,
    encounter: Encounter,
    event: Event,
    house_portal: HousePortal,
    landblock_instance: LandblockInstance,
    landblock_instance_link: LandblockInstanceLink,
    points_of_interest: PointsOfInterest,
    quest: Quest,
    recipe: Recipe,
    recipe_mod: RecipeMod,
    recipe_mods_bool: RecipeModsBool,
    recipe_mods_did: RecipeModsDID,
    recipe_mods_float: RecipeModsFloat,
    recipe_mods_iid: RecipeModsIID,
    recipe_mods_int: RecipeModsInt,
    recipe_mods_string: RecipeModsString,
    recipe_requirements_bool: RecipeRequirementsBool,
    recipe_requirements_did: RecipeRequirementsDID,
    recipe_requirements_float: RecipeRequirementsFloat,
    recipe_requirements_iid: RecipeRequirementsIID,
    recipe_requirements_int: RecipeRequirementsInt,
    recipe_requirements_string: RecipeRequirementsString,
    spell: Spell,
    treasure_death: TreasureDeath,
    treasure_gem_count: TreasureGemCount,
    treasure_material_base: TreasureMaterialBase,
    treasure_material_color: TreasureMaterialColor,
    treasure_material_groups: TreasureMaterialGroups,
    treasure_wielded: TreasureWielded,
    version: Version,
    weenie: Weenie,
    weenie_properties_anim_part: WeeniePropertiesAnimPart,
    weenie_properties_attribute: WeeniePropertiesAttribute,
    weenie_properties_attribute_2nd: WeeniePropertiesAttribute2nd,
    weenie_properties_body_part: WeeniePropertiesBodyPart,
    weenie_properties_book: WeeniePropertiesBook,
    weenie_properties_book_page_data: WeeniePropertiesBookPageData,
    weenie_properties_bool: WeeniePropertiesBool,
    weenie_properties_create_list: WeeniePropertiesCreateList,
    weenie_properties_did: WeeniePropertiesDID,
    weenie_properties_emote: WeeniePropertiesEmote,
    weenie_properties_emote_action: WeeniePropertiesEmoteAction,
    weenie_properties_event_filter: WeeniePropertiesEventFilter,
    weenie_properties_float: WeeniePropertiesFloat,
    weenie_properties_generator: WeeniePropertiesGenerator,
    weenie_properties_iid: WeeniePropertiesIID,
    weenie_properties_int: WeeniePropertiesInt,
    weenie_properties_int64: WeeniePropertiesInt64,
    weenie_properties_palette: WeeniePropertiesPalette,
    weenie_properties_position: WeeniePropertiesPosition,
    weenie_properties_skill: WeeniePropertiesSkill,
    weenie_properties_spell_book: WeeniePropertiesSpellBook,
    weenie_properties_string: WeeniePropertiesString,
    weenie_properties_texture_map: WeeniePropertiesTextureMap,
}

impl DumpSink for WorldRows {
    fn create_table(&mut self, table: &TableDef) -> Result<(), ImportError> {
        let Some(wanted) = Self::columns_of(&table.name) else {
            self.unknown_tables.entry(table.name.clone()).or_insert(0);
            return Ok(());
        };
        for &c in wanted {
            if !table.columns.iter().any(|d| d.name == c) {
                return Err(ImportError::SchemaColumn {
                    table: table.name.clone(),
                    column: c.to_owned(),
                });
            }
        }
        for d in &table.columns {
            // `landblock_instance.landblock` is a generated column; the importer computes it.
            let computed = table.name == "landblock_instance" && d.name == "landblock";
            if !wanted.contains(&d.name.as_str()) && !computed {
                self.unread_columns
                    .push((table.name.clone(), d.name.clone()));
            }
        }
        self.declared.push(table.name.clone());
        Ok(())
    }

    fn row(&mut self, row: &Row<'_>) -> Result<(), ImportError> {
        if !self.push(row)? {
            *self.unknown_tables.entry(row.table.to_owned()).or_insert(0) += 1;
        }
        Ok(())
    }
}

/// Children whose parent row does not exist, per child table. The dump's foreign keys make this
/// impossible for a well-formed dump; it is counted rather than assumed.
pub type Orphans = BTreeMap<&'static str, u64>;

/// Move each child into a map from parent id to its children, in input order.
fn group<T>(rows: Vec<T>, parent: impl Fn(&T) -> u32) -> HashMap<u32, Vec<T>> {
    let mut m: HashMap<u32, Vec<T>> = HashMap::new();
    for r in rows {
        m.entry(parent(&r)).or_default().push(r);
    }
    m
}

/// Whatever is left in a grouping after every parent took its children is orphaned.
fn orphans<T>(orphans: &mut Orphans, table: &'static str, left: HashMap<u32, Vec<T>>) {
    let n: u64 = left.values().map(|v| v.len() as u64).sum();
    if n > 0 {
        *orphans.entry(table).or_insert(0) += n;
    }
}

impl WorldRows {
    /// Place every child row inside its parent.
    #[must_use]
    pub fn assemble(self) -> (WorldContent, Orphans) {
        let mut orph = Orphans::new();
        macro_rules! take {
            ($map:ident, $id:expr) => {
                $map.remove(&$id).unwrap_or_default()
            };
        }

        // Weenies.
        let mut actions = group(self.weenie_properties_emote_action, |r| r.emote_id);
        let mut emotes: Vec<WeeniePropertiesEmote> = self.weenie_properties_emote;
        for e in &mut emotes {
            e.weenie_properties_emote_action = take!(actions, e.id);
        }
        orphans(&mut orph, "weenie_properties_emote_action", actions);

        let mut anim = group(self.weenie_properties_anim_part, |r| r.object_id);
        let mut attr = group(self.weenie_properties_attribute, |r| r.object_id);
        let mut attr2 = group(self.weenie_properties_attribute_2nd, |r| r.object_id);
        let mut body = group(self.weenie_properties_body_part, |r| r.object_id);
        let mut book = group(self.weenie_properties_book, |r| r.object_id);
        let mut page = group(self.weenie_properties_book_page_data, |r| r.object_id);
        let mut bools = group(self.weenie_properties_bool, |r| r.object_id);
        let mut create = group(self.weenie_properties_create_list, |r| r.object_id);
        let mut did = group(self.weenie_properties_did, |r| r.object_id);
        let mut emote = group(emotes, |r| r.object_id);
        let mut filter = group(self.weenie_properties_event_filter, |r| r.object_id);
        let mut float = group(self.weenie_properties_float, |r| r.object_id);
        let mut gen = group(self.weenie_properties_generator, |r| r.object_id);
        let mut iid = group(self.weenie_properties_iid, |r| r.object_id);
        let mut int = group(self.weenie_properties_int, |r| r.object_id);
        let mut int64 = group(self.weenie_properties_int64, |r| r.object_id);
        let mut palette = group(self.weenie_properties_palette, |r| r.object_id);
        let mut position = group(self.weenie_properties_position, |r| r.object_id);
        let mut skill = group(self.weenie_properties_skill, |r| r.object_id);
        let mut spell_book = group(self.weenie_properties_spell_book, |r| r.object_id);
        let mut string = group(self.weenie_properties_string, |r| r.object_id);
        let mut texture = group(self.weenie_properties_texture_map, |r| r.object_id);

        let mut weenies = self.weenie;
        for w in &mut weenies {
            let id = w.class_id;
            w.weenie_properties_anim_part = take!(anim, id);
            w.weenie_properties_attribute = take!(attr, id);
            w.weenie_properties_attribute_2nd = take!(attr2, id);
            w.weenie_properties_body_part = take!(body, id);
            // One book row per weenie (unique key `object_Id`); a second one is an orphan.
            let mut books = take!(book, id).into_iter();
            w.weenie_properties_book = books.next();
            let extra = books.count() as u64;
            if extra > 0 {
                *orph.entry("weenie_properties_book").or_insert(0) += extra;
            }
            w.weenie_properties_book_page_data = take!(page, id);
            w.weenie_properties_bool = take!(bools, id);
            w.weenie_properties_create_list = take!(create, id);
            w.weenie_properties_did = take!(did, id);
            w.weenie_properties_emote = take!(emote, id);
            w.weenie_properties_event_filter = take!(filter, id);
            w.weenie_properties_float = take!(float, id);
            w.weenie_properties_generator = take!(gen, id);
            w.weenie_properties_iid = take!(iid, id);
            w.weenie_properties_int = take!(int, id);
            w.weenie_properties_int64 = take!(int64, id);
            w.weenie_properties_palette = take!(palette, id);
            w.weenie_properties_position = take!(position, id);
            w.weenie_properties_skill = take!(skill, id);
            w.weenie_properties_spell_book = take!(spell_book, id);
            w.weenie_properties_string = take!(string, id);
            w.weenie_properties_texture_map = take!(texture, id);
        }
        orphans(&mut orph, "weenie_properties_anim_part", anim);
        orphans(&mut orph, "weenie_properties_attribute", attr);
        orphans(&mut orph, "weenie_properties_attribute_2nd", attr2);
        orphans(&mut orph, "weenie_properties_body_part", body);
        orphans(&mut orph, "weenie_properties_book", book);
        orphans(&mut orph, "weenie_properties_book_page_data", page);
        orphans(&mut orph, "weenie_properties_bool", bools);
        orphans(&mut orph, "weenie_properties_create_list", create);
        orphans(&mut orph, "weenie_properties_d_i_d", did);
        orphans(&mut orph, "weenie_properties_emote", emote);
        orphans(&mut orph, "weenie_properties_event_filter", filter);
        orphans(&mut orph, "weenie_properties_float", float);
        orphans(&mut orph, "weenie_properties_generator", gen);
        orphans(&mut orph, "weenie_properties_i_i_d", iid);
        orphans(&mut orph, "weenie_properties_int", int);
        orphans(&mut orph, "weenie_properties_int64", int64);
        orphans(&mut orph, "weenie_properties_palette", palette);
        orphans(&mut orph, "weenie_properties_position", position);
        orphans(&mut orph, "weenie_properties_skill", skill);
        orphans(&mut orph, "weenie_properties_spell_book", spell_book);
        orphans(&mut orph, "weenie_properties_string", string);
        orphans(&mut orph, "weenie_properties_texture_map", texture);

        // Landblock instances and their links.
        let mut links = group(self.landblock_instance_link, |r| r.parent_guid);
        let mut instances = self.landblock_instance;
        for i in &mut instances {
            i.landblock_instance_link = take!(links, i.guid);
        }
        orphans(&mut orph, "landblock_instance_link", links);

        // Recipes: mods with their six stat tables, then the six requirement tables.
        let mut mb = group(self.recipe_mods_bool, |r| r.recipe_mod_id);
        let mut md = group(self.recipe_mods_did, |r| r.recipe_mod_id);
        let mut mf = group(self.recipe_mods_float, |r| r.recipe_mod_id);
        let mut mi = group(self.recipe_mods_iid, |r| r.recipe_mod_id);
        let mut mn = group(self.recipe_mods_int, |r| r.recipe_mod_id);
        let mut ms = group(self.recipe_mods_string, |r| r.recipe_mod_id);
        let mut mods = self.recipe_mod;
        for m in &mut mods {
            m.recipe_mods_bool = take!(mb, m.id);
            m.recipe_mods_did = take!(md, m.id);
            m.recipe_mods_float = take!(mf, m.id);
            m.recipe_mods_iid = take!(mi, m.id);
            m.recipe_mods_int = take!(mn, m.id);
            m.recipe_mods_string = take!(ms, m.id);
        }
        orphans(&mut orph, "recipe_mods_bool", mb);
        orphans(&mut orph, "recipe_mods_d_i_d", md);
        orphans(&mut orph, "recipe_mods_float", mf);
        orphans(&mut orph, "recipe_mods_i_i_d", mi);
        orphans(&mut orph, "recipe_mods_int", mn);
        orphans(&mut orph, "recipe_mods_string", ms);
        let mut mods = group(mods, |r| r.recipe_id);
        let mut rb = group(self.recipe_requirements_bool, |r| r.recipe_id);
        let mut rd = group(self.recipe_requirements_did, |r| r.recipe_id);
        let mut rf = group(self.recipe_requirements_float, |r| r.recipe_id);
        let mut ri = group(self.recipe_requirements_iid, |r| r.recipe_id);
        let mut rn = group(self.recipe_requirements_int, |r| r.recipe_id);
        let mut rs = group(self.recipe_requirements_string, |r| r.recipe_id);
        let mut recipes = self.recipe;
        for r in &mut recipes {
            r.recipe_mod = take!(mods, r.id);
            r.recipe_requirements_bool = take!(rb, r.id);
            r.recipe_requirements_did = take!(rd, r.id);
            r.recipe_requirements_float = take!(rf, r.id);
            r.recipe_requirements_iid = take!(ri, r.id);
            r.recipe_requirements_int = take!(rn, r.id);
            r.recipe_requirements_string = take!(rs, r.id);
        }
        orphans(&mut orph, "recipe_mod", mods);
        orphans(&mut orph, "recipe_requirements_bool", rb);
        orphans(&mut orph, "recipe_requirements_d_i_d", rd);
        orphans(&mut orph, "recipe_requirements_float", rf);
        orphans(&mut orph, "recipe_requirements_i_i_d", ri);
        orphans(&mut orph, "recipe_requirements_int", rn);
        orphans(&mut orph, "recipe_requirements_string", rs);

        let content = WorldContent {
            weenies,
            landblock_instances: instances,
            encounters: self.encounter,
            cook_books: self.cook_book,
            recipes,
            events: self.event,
            house_portals: self.house_portal,
            points_of_interest: self.points_of_interest,
            quests: self.quest,
            spells: self.spell,
            treasure_death: self.treasure_death,
            treasure_gem_count: self.treasure_gem_count,
            treasure_material_base: self.treasure_material_base,
            treasure_material_color: self.treasure_material_color,
            treasure_material_groups: self.treasure_material_groups,
            treasure_wielded: self.treasure_wielded,
            versions: self.version,
        };
        (content, orph)
    }
}
