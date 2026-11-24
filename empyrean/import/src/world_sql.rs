//! A world as ACE's world SQL: `world.sql`, ACE's schema (WorldBase.sql at the Launch converter's
//! pin, an emote action's motion unsigned as ACE's world database releases have it) and every row of ACE's tables as ACE's own SQL writers write it; and `empyrean.sql`, what
//! stock ACE has no table for, applied after it: the recipes with current-vital preconditions (with
//! their cook books and the preconditions) and the source loot tables.
//!
//! ACE's writers write ids in hexadecimal, as ACE's content files are, so the importer reads both as
//! content over an empty database: `empyrean-import --sql <an empty file> --patches world.sql
//! --patches empyrean.sql` builds the world's pack back from them.

use std::collections::{BTreeMap, BTreeSet};

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_content::export::sql::{
    CookBookSQLWriter, EncounterSQLWriter, EventSQLWriter, HousePortalSQLWriter,
    LandblockInstanceWriter, QuestSQLWriter, RecipeSQLWriter, SQLWriter, SpellSQLWriter, SqlOut,
    TreasureDeathSQLWriter, TreasureWieldedSQLWriter, WeenieSQLWriter,
};
use empyrean_content::import;
use empyrean_content::WorldContent;

use crate::ac1999::convert::{literal, SCHEMA, SCHEMA_REVISION};

/// The two files' text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSql {
    pub world: String,
    pub empyrean: String,
}

fn date(d: DotNetDateTime) -> String {
    d.format("yyyy-MM-dd HH:mm:ss")
}

fn text(s: &str) -> String {
    String::from_utf8_lossy(&literal(s.as_bytes())).into_owned()
}

/// A float as ACE's writers print one: its shortest form that reads back as itself.
fn float(f: f32) -> String {
    format!("{f}")
}

/// One `INSERT` of `rows` (each its values' text) into `table`'s `columns`.
fn insert(out: &mut String, table: &str, columns: &str, rows: &[String]) {
    if rows.is_empty() {
        return;
    }
    out.push_str(&format!("\nINSERT INTO `{table}` ({columns})\n"));
    for (i, r) in rows.iter().enumerate() {
        out.push_str(if i == 0 { "VALUES (" } else { "     , (" });
        out.push_str(r);
        out.push_str(if i + 1 == rows.len() { ");\n" } else { ")\n" });
    }
}

/// A table's `CREATE TABLE` as `world.sql` declares it: the pinned schema's, but for an emote
/// action's `motion`, which the pin declares a signed `int` and ACE's world database releases
/// (and the motions they hold, past `0x7FFFFFFF`) an unsigned one.
fn create_sql(table: &str, pinned: &str) -> String {
    if table == "weenie_properties_emote_action" {
        pinned.replacen(
            "`motion` int DEFAULT NULL",
            "`motion` int unsigned DEFAULT NULL",
            1,
        )
    } else {
        pinned.to_owned()
    }
}

/// `c` as ACE's world SQL and Empyrean's own.
///
/// # Errors
/// What ACE's writers refuse (a guid placed twice), as a sentence, or a schema that doesn't read.
#[allow(clippy::too_many_lines)] // one table after another, in the schema's order
pub fn write(c: &WorldContent) -> Result<WorldSql, String> {
    let canonical = import::load_store(SCHEMA, std::iter::empty(), import::default_now())
        .map_err(|e| e.to_string())?;
    let mut w = String::from("-- A world folded from its rows, in ACE's schema (WorldBase.sql at ");
    w.push_str(SCHEMA_REVISION);
    w.push_str("). Use an empty database; apply empyrean.sql after it.\nSET NAMES utf8mb4;\nSET FOREIGN_KEY_CHECKS=0;\nSET SQL_MODE='STRICT_ALL_TABLES,NO_AUTO_VALUE_ON_ZERO';\n");
    for i in canonical.store.table_indices() {
        let table = canonical
            .store
            .table(i)
            .ok_or("a schema table is missing")?;
        w.push_str(create_sql(&table.name, &table.create_sql).trim_end_matches(';'));
        w.push_str(";\n");
    }
    let err = |what: &str, e: empyrean_content::export::sql::SqlWriterError| format!("{what}: {e}");
    let mut out = SqlOut::new("\n");
    let vital: BTreeSet<u32> = c.recipe_vital_requirements.iter().map(|v| v.id).collect();

    let weenies = WeenieSQLWriter::default();
    for x in &c.weenies {
        out.write_empty_line();
        weenies
            .create_sql_insert_statement(x, &mut out)
            .map_err(|e| err(&format!("weenie {}", x.class_id), e))?;
    }
    let mut by_block: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for i in &c.landblock_instances {
        by_block
            .entry(i.obj_cell_id >> 16)
            .or_default()
            .push(i.clone());
    }
    let instances = LandblockInstanceWriter::default();
    for (lb, rows) in &by_block {
        out.write_empty_line();
        instances
            .create_sql_insert_statement(rows, &mut out)
            .map_err(|e| err(&format!("landblock {lb:04X}"), e))?;
    }
    let mut by_block: BTreeMap<i32, Vec<_>> = BTreeMap::new();
    for e in &c.encounters {
        by_block.entry(e.landblock).or_default().push(e.clone());
    }
    let encounters = EncounterSQLWriter::default();
    for (lb, rows) in &by_block {
        out.write_empty_line();
        encounters
            .create_sql_insert_statement(rows, &mut out)
            .map_err(|e| err(&format!("encounters of {lb:04X}"), e))?;
    }
    let recipes = RecipeSQLWriter::default();
    for r in c.recipes.iter().filter(|r| !vital.contains(&r.id)) {
        out.write_empty_line();
        recipes
            .create_sql_insert_statement(r, &mut out)
            .map_err(|e| err(&format!("recipe {}", r.id), e))?;
    }
    let mut by_recipe: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for k in &c.cook_books {
        by_recipe.entry(k.recipe_id).or_default().push(k.clone());
    }
    let cook_books = CookBookSQLWriter::default();
    for (id, rows) in by_recipe.iter().filter(|(id, _)| !vital.contains(id)) {
        out.write_empty_line();
        cook_books
            .create_sql_insert_statement(rows, &mut out)
            .map_err(|e| err(&format!("cook books of recipe {id}"), e))?;
    }
    let spells = SpellSQLWriter::default();
    for s in &c.spells {
        out.write_empty_line();
        spells.create_sql_insert_statement(s, &mut out);
    }
    let quests = QuestSQLWriter::default();
    for q in &c.quests {
        out.write_empty_line();
        quests.create_sql_insert_statement(q, &mut out);
    }
    let events = EventSQLWriter::default();
    for e in &c.events {
        out.write_empty_line();
        events.create_sql_insert_statement(e, &mut out);
    }
    let mut by_house: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for h in &c.house_portals {
        by_house.entry(h.house_id).or_default().push(h.clone());
    }
    let portals = HousePortalSQLWriter::default();
    for (house, rows) in &by_house {
        out.write_empty_line();
        portals
            .create_sql_insert_statement(rows, &mut out)
            .map_err(|e| err(&format!("house {house}"), e))?;
    }
    let deaths = TreasureDeathSQLWriter::default();
    for t in &c.treasure_death {
        out.write_empty_line();
        deaths.create_sql_insert_statement(t, &mut out);
    }
    let mut by_type: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for t in &c.treasure_wielded {
        by_type.entry(t.treasure_type).or_default().push(t.clone());
    }
    let wielded = TreasureWieldedSQLWriter::default();
    for (ty, rows) in &by_type {
        out.write_empty_line();
        wielded
            .create_sql_insert_statement(rows, &mut out)
            .map_err(|e| err(&format!("wielded treasure {ty}"), e))?;
    }
    w.push_str(&out.text);

    // The tables ACE writes no file for, as plain rows.
    let q = |s: Option<&str>| SQLWriter::get_sql_string(s).unwrap_or_else(|| "NULL".into());
    insert(
        &mut w,
        "points_of_interest",
        "`name`, `weenie_Class_Id`, `last_Modified`",
        &c.points_of_interest
            .iter()
            .map(|p| {
                format!(
                    "{}, {}, '{}'",
                    q(Some(&p.name)),
                    p.weenie_class_id,
                    date(p.last_modified)
                )
            })
            .collect::<Vec<_>>(),
    );
    insert(
        &mut w,
        "treasure_gem_count",
        "`gem_Code`, `tier`, `count`, `chance`",
        &c.treasure_gem_count
            .iter()
            .map(|t| {
                format!(
                    "{}, {}, {}, {}",
                    t.gem_code,
                    t.tier,
                    t.count,
                    float(t.chance)
                )
            })
            .collect::<Vec<_>>(),
    );
    insert(
        &mut w,
        "treasure_material_base",
        "`material_Code`, `tier`, `probability`, `material_Id`",
        &c.treasure_material_base
            .iter()
            .map(|t| {
                format!(
                    "{}, {}, {}, {}",
                    t.material_code,
                    t.tier,
                    float(t.probability),
                    t.material_id
                )
            })
            .collect::<Vec<_>>(),
    );
    insert(
        &mut w,
        "treasure_material_color",
        "`material_Id`, `color_Code`, `palette_Template`, `probability`",
        &c.treasure_material_color
            .iter()
            .map(|t| {
                format!(
                    "{}, {}, {}, {}",
                    t.material_id,
                    t.color_code,
                    t.palette_template,
                    float(t.probability)
                )
            })
            .collect::<Vec<_>>(),
    );
    insert(
        &mut w,
        "treasure_material_groups",
        "`material_Group`, `tier`, `probability`, `material_Id`",
        &c.treasure_material_groups
            .iter()
            .map(|t| {
                format!(
                    "{}, {}, {}, {}",
                    t.material_group,
                    t.tier,
                    float(t.probability),
                    t.material_id
                )
            })
            .collect::<Vec<_>>(),
    );
    insert(
        &mut w,
        "version",
        "`base_Version`, `patch_Version`, `last_Modified`",
        &c.versions
            .iter()
            .map(|v| {
                format!(
                    "{}, {}, '{}'",
                    q(v.base_version.as_deref()),
                    q(v.patch_version.as_deref()),
                    date(v.last_modified)
                )
            })
            .collect::<Vec<_>>(),
    );
    w.push_str("SET FOREIGN_KEY_CHECKS=1;\n");

    // What stock ACE has no table for.
    let mut e = String::from("-- What this world holds that stock ACE has no table for. Apply after world.sql; it requires Empyrean.\nSET NAMES utf8mb4;\nSET SQL_MODE='STRICT_ALL_TABLES,NO_AUTO_VALUE_ON_ZERO';\nSET FOREIGN_KEY_CHECKS=1;\n");
    if !c.recipe_vital_requirements.is_empty() {
        e.push_str("CREATE TABLE `recipe_requirements_vital` (`id` int unsigned NOT NULL, `health` int unsigned NOT NULL, `stamina` int unsigned NOT NULL, `mana` int unsigned NOT NULL, PRIMARY KEY (`id`), CONSTRAINT `recipe_vital_parent` FOREIGN KEY (`id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE) ENGINE=InnoDB;\n");
        let mut out = SqlOut::new("\n");
        for r in c.recipes.iter().filter(|r| vital.contains(&r.id)) {
            out.write_empty_line();
            recipes
                .create_sql_insert_statement(r, &mut out)
                .map_err(|e| err(&format!("recipe {}", r.id), e))?;
        }
        for (id, rows) in by_recipe.iter().filter(|(id, _)| vital.contains(id)) {
            out.write_empty_line();
            cook_books
                .create_sql_insert_statement(rows, &mut out)
                .map_err(|e| err(&format!("cook books of recipe {id}"), e))?;
        }
        e.push_str(&out.text);
        for v in &c.recipe_vital_requirements {
            e.push_str(&format!(
                "INSERT INTO `recipe_requirements_vital` VALUES ({},{},{},{});\n",
                v.id, v.health, v.stamina, v.mana
            ));
        }
    }
    if !c.grouped_treasure.is_empty() || !c.loot_policies.is_empty() {
        e.push_str("-- Versioned source loot. Requires Empyrean; stock ACE cannot execute these profiles.\nCREATE TABLE `empyrean_grouped_treasure` (`id` int unsigned NOT NULL, `profile` mediumtext NOT NULL, PRIMARY KEY (`id`)) ENGINE=InnoDB;\nCREATE TABLE `empyrean_loot_policy` (`id` int unsigned NOT NULL, `profile` mediumtext NOT NULL, PRIMARY KEY (`id`)) ENGINE=InnoDB;\n");
        for g in &c.grouped_treasure {
            let json = serde_json::to_string(g).map_err(|e| e.to_string())?;
            e.push_str(&format!(
                "INSERT INTO `empyrean_grouped_treasure` (`id`,`profile`) VALUES ({},{});\n",
                g.treasure_type,
                text(&json)
            ));
        }
        for (n, p) in (1u32..).zip(&c.loot_policies) {
            let json = serde_json::to_string(p).map_err(|e| e.to_string())?;
            e.push_str(&format!(
                "INSERT INTO `empyrean_loot_policy` (`id`,`profile`) VALUES ({n},{});\n",
                text(&json)
            ));
        }
    }
    Ok(WorldSql {
        world: w,
        empyrean: e,
    })
}
