//! An explicit, incomplete projection into ACE's schema. Unsupported rows stay in the ledger.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use empyrean_content::error::ImportError;
use empyrean_content::import::mysqldump::{DumpSink, Row, TableDef, Value};
use empyrean_content::import::sql::store::Store;
use empyrean_content::import::{patch, sql};
use serde_json::{json, Value as Json};

// ACE (ACEmulator), AGPL-3.0: Database/Base/WorldBase.sql at this revision.
pub const SCHEMA_REVISION: &str = "47edade3bd3f6044b676d4eb877c4965c7eda62b";
pub const SCHEMA: &[u8] = include_bytes!("../../schema/ace_world.sql");
// ac1999, AGPL-3.0: Server/Database/AC1999-World/1-Base/WorldBase.sql.
pub const SOURCE_SCHEMA: &[u8] = include_bytes!("../../schema/ac1999_world.sql");
pub const SOURCE_SCHEMA_REVISION: &str = "5fe910e7a3e549d955117920a1bd30b4ee646938";

pub fn validate_content(src: &patch::Source) -> Result<(), ImportError> {
    let tokens = sql::lex::tokenize(&src.bytes).map_err(fail)?;
    let stmts = sql::parse::parse_script(&tokens).map_err(fail)?;
    for stmt in stmts {
        if matches!(
            stmt.stmt,
            sql::parse::Stmt::AlterTable { .. }
                | sql::parse::Stmt::CreateTable(_)
                | sql::parse::Stmt::DropTable { .. }
        ) {
            return Err(fail(format!(
                "unreviewed content DDL in {} at line {}",
                src.path.display(),
                stmt.line
            )));
        }
    }
    Ok(())
}

pub fn validate_schema(actual: &[u8]) -> Result<(), String> {
    let tokens = |bytes: &[u8]| -> Result<Vec<sql::lex::Tok>, String> {
        Ok(sql::lex::tokenize(bytes)?
            .into_iter()
            .map(|t| t.kind)
            .collect())
    };
    if tokens(actual)? != tokens(SOURCE_SCHEMA)? {
        return Err(
            "source schema differs from the reviewed definition; review this revision".into(),
        );
    }
    Ok(())
}

pub(super) fn literal(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![b'\''];
    for &b in bytes {
        match b {
            0 => out.extend_from_slice(b"\\0"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            26 => out.extend_from_slice(b"\\Z"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\'' => out.extend_from_slice(b"\\'"),
            _ => out.push(b),
        }
    }
    out.push(b'\'');
    out
}

pub(super) fn fail(what: impl Into<String>) -> ImportError {
    ImportError::Sql {
        line: 0,
        what: what.into(),
    }
}

pub(super) fn number(row: &Row<'_>, name: &str) -> Result<i128, ImportError> {
    let i = row
        .columns()
        .iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .ok_or_else(|| fail(format!("{}.{} missing", row.table, name)))?;
    let b = row.values()[i]
        .bytes()
        .ok_or_else(|| fail("unexpected null key"))?;
    std::str::from_utf8(b)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| fail(format!("{}.{} is not an integer", row.table, name)))
}

#[derive(Debug, Default)]
pub struct Census {
    weenies: BTreeSet<i128>,
    spells: BTreeSet<i128>,
    recipes: BTreeSet<i128>,
    mods: BTreeSet<i128>,
    missing_instances: BTreeSet<i128>,
    pub references: super::references::Index,
    pub placements: super::world::Placements,
}

impl Census {
    pub fn read(store: &Store) -> Result<Self, ImportError> {
        let mut c = Self::default();
        store.emit(&mut c)?;
        if let Ok(t) = store.table_index("landblock_instance") {
            let table = store
                .table(t)
                .ok_or_else(|| fail("missing instance table"))?;
            let wcid = store.column(t, "weenie_Class_Id").map_err(fail)?;
            let guid = store.column(t, "guid").map_err(fail)?;
            for row in table.scan_order().map_err(fail)? {
                let values = store.values(t, row).map_err(fail)?;
                let class = values[wcid]
                    .as_exact_int()
                    .ok_or_else(|| fail("bad spawn class"))?;
                if !c.weenies.contains(&class) {
                    c.missing_instances.insert(
                        values[guid]
                            .as_exact_int()
                            .ok_or_else(|| fail("bad spawn guid"))?,
                    );
                }
            }
        }
        Ok(c)
    }
}

impl DumpSink for Census {
    fn create_table(&mut self, _: &TableDef) -> Result<(), ImportError> {
        Ok(())
    }
    fn row(&mut self, row: &Row<'_>) -> Result<(), ImportError> {
        self.references.row(row)?;
        match row.table {
            "weenie" => {
                let class = number(row, "class_Id")?;
                self.weenies.insert(class);
                self.placements.weenies.insert(class);
            }
            "landblock_instance" => {
                self.placements.instances.insert(
                    number(row, "guid")?,
                    (number(row, "weenie_Class_Id")?, number(row, "obj_Cell_Id")?),
                );
            }
            "landblock_instance_link" => {
                self.placements
                    .links
                    .push((number(row, "parent_GUID")?, number(row, "child_GUID")?));
            }
            "spell" => {
                self.spells.insert(number(row, "id")?);
            }
            "recipe_mod"
                if row
                    .columns()
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case("requires")) =>
            {
                self.recipes.insert(number(row, "recipe_Id")?);
                self.mods.insert(number(row, "id")?);
            }
            _ => {}
        }
        Ok(())
    }
}

pub struct Projection<W: Write> {
    pub output: W,
    pub ledger: Vec<Json>,
    pub counts: BTreeMap<String, u64>,
    schema: BTreeMap<String, BTreeSet<String>>,
    census: Census,
    batch: usize,
    pub cells: super::cells::Adaptation,
    pub world: super::world::Policy,
    pub references: super::references::Settled,
}

impl<W: Write> Projection<W> {
    pub fn new(output: W, canonical: &Store, census: Census) -> Self {
        let schema = canonical
            .table_indices()
            .into_iter()
            .map(|i| {
                let t = canonical.table(i).expect("table index from store");
                (
                    t.name.clone(),
                    t.columns
                        .iter()
                        .map(|c| c.name.to_ascii_lowercase())
                        .collect(),
                )
            })
            .collect();
        Self {
            output,
            ledger: Vec::new(),
            counts: BTreeMap::new(),
            schema,
            census,
            batch: 0,
            cells: super::cells::Adaptation::default(),
            world: super::world::Policy::default(),
            references: super::references::Settled::default(),
        }
    }

    fn bytes(&mut self, b: &[u8]) -> Result<(), ImportError> {
        self.output.write_all(b).map_err(|e| fail(e.to_string()))
    }

    pub fn finish(&mut self) -> Result<(), ImportError> {
        if self.batch != 0 {
            self.bytes(b";\n")?;
            self.batch = 0;
        }
        Ok(())
    }

    fn record(&mut self, row: &Row<'_>, action: &str, reason: &str) {
        let values: BTreeMap<_, _> = row
            .columns()
            .iter()
            .zip(row.values())
            .map(|(k, v)| {
                (
                    k,
                    v.bytes().map(|b| String::from_utf8_lossy(b).into_owned()),
                )
            })
            .collect();
        self.ledger
            .push(json!({"table":row.table,"source_row":row.row_no,
            "action":action,"reason":reason,"values":values,
            "quoted_columns":row.columns().iter().zip(row.values())
                .filter_map(|(column,value)| matches!(value,Value::Quoted(_)).then_some(column))
                .collect::<Vec<_>>()}));
    }

    /// Rows the 1999 world removes: dropped placements and their links, rows that place or create
    /// cut content, and destinations in cells the 1999 files lack.
    fn world_quarantine(&mut self, row: &Row<'_>) -> Result<Option<String>, ImportError> {
        if !self.world.active() {
            return Ok(None);
        }
        let (owner, class) = match row.table {
            "landblock_instance" => {
                return Ok(self.world.guids.get(&number(row, "guid")?).cloned());
            }
            "landblock_instance_link" => {
                let parent = number(row, "parent_GUID")?;
                let child = number(row, "child_GUID")?;
                let dropped =
                    self.world.guids.contains_key(&parent) || self.world.guids.contains_key(&child);
                return Ok(dropped.then(|| "link to a placement the 1999 world drops".to_owned()));
            }
            "weenie_properties_position" => {
                let hit = self.world.destination(
                    number(row, "object_Id")?,
                    number(row, "position_Type")?,
                    number(row, "obj_Cell_Id")?,
                );
                return Ok(hit.then(|| {
                    "destination in an interior cell the 1999 cell.dat lacks".to_owned()
                }));
            }
            "weenie_properties_generator" if number(row, "where_Create")? & 0x40 == 0 => (
                Some(number(row, "object_Id")?),
                number(row, "weenie_Class_Id")?,
            ),
            "weenie_properties_create_list" => (
                Some(number(row, "object_Id")?),
                number(row, "weenie_Class_Id")?,
            ),
            "weenie_properties_emote_action" => {
                let owner = self
                    .references
                    .emote_owner
                    .get(&number(row, "emote_Id")?)
                    .copied();
                let class = row
                    .columns()
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case("weenie_Class_Id"))
                    .and_then(|i| row.values()[i].bytes())
                    .and_then(|b| std::str::from_utf8(b).ok()?.parse().ok());
                let Some(class) = class else {
                    return Ok(None);
                };
                (owner, class)
            }
            "encounter" | "treasure_wielded" => (None, number(row, "weenie_Class_Id")?),
            _ => return Ok(None),
        };
        Ok(self.world.cut_reference(owner, class).map(str::to_owned))
    }

    fn quarantine(&mut self, row: &Row<'_>) -> Result<Option<String>, ImportError> {
        let t = row.table;
        if t == "treasure_death" {
            return Ok(Some(
                "six-group death loot requires a separate generator".to_owned(),
            ));
        }
        let recipe_key = if t == "recipe" {
            Some("id")
        } else if t == "cook_book" || t == "recipe_mod" || t.starts_with("recipe_requirements_") {
            Some("recipe_Id")
        } else {
            None
        };
        if let Some(key) = recipe_key {
            if self.census.recipes.contains(&number(row, key)?) {
                return Ok(Some(
                    "recipe has an unsupported targeted/conditional modification".to_owned(),
                ));
            }
        }
        if t.starts_with("recipe_mods_")
            && self.census.mods.contains(&number(row, "recipe_Mod_Id")?)
        {
            return Ok(Some(
                "child of a quarantined recipe modification".to_owned(),
            ));
        }
        if t == "landblock_instance_link"
            && (self
                .census
                .missing_instances
                .contains(&number(row, "parent_GUID")?)
                || self
                    .census
                    .missing_instances
                    .contains(&number(row, "child_GUID")?))
        {
            return Ok(Some("link to a quarantined placed instance".to_owned()));
        }
        if let Some(reason) = self.references.quarantine(row)? {
            return Ok(Some(reason.to_owned()));
        }
        if let Some(reason) = self.world_quarantine(row)? {
            return Ok(Some(reason));
        }
        let class_column = match t {
            "landblock_instance"
            | "encounter"
            | "weenie_properties_create_list"
            | "treasure_wielded" => Some("weenie_Class_Id"),
            "weenie_properties_generator" if number(row, "where_Create")? & 0x40 == 0 => {
                Some("weenie_Class_Id")
            }
            _ => None,
        };
        if let Some(column) = class_column {
            let key = number(row, column)?;
            if key != 0 && !self.census.weenies.contains(&key) {
                return Ok(Some("referenced weenie is absent".to_owned()));
            }
        }
        if t == "weenie_properties_emote_action" {
            // An emote that gives or creates an item names its class; most actions name none.
            let class = row
                .columns()
                .iter()
                .position(|c| c.eq_ignore_ascii_case("weenie_Class_Id"))
                .and_then(|i| row.values()[i].bytes())
                .and_then(|b| std::str::from_utf8(b).ok()?.parse::<i128>().ok());
            if class.is_some_and(|c| c != 0 && !self.census.weenies.contains(&c)) {
                return Ok(Some("referenced weenie is absent".to_owned()));
            }
        }
        if t == "weenie_properties_spell_book"
            && !self.census.spells.contains(&number(row, "spell")?)
        {
            return Ok(Some("referenced spell is absent".to_owned()));
        }
        Ok(None)
    }
}

impl<W: Write> DumpSink for Projection<W> {
    fn create_table(&mut self, table: &TableDef) -> Result<(), ImportError> {
        self.finish()?;
        let target = self
            .schema
            .get(&table.name)
            .ok_or_else(|| fail(format!("unknown source table {}", table.name)))?;
        if table.name == "treasure_death" {
            let mut expected: BTreeSet<String> = [
                "id",
                "treasure_type",
                "tier",
                "loot_quality_mod",
                "last_modified",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            for group in [
                "weapon",
                "armor",
                "item",
                "magic_item",
                "consumable",
                "valuable",
            ] {
                for suffix in ["chance", "min_amount", "max_amount"] {
                    expected.insert(format!("{group}_{suffix}"));
                }
            }
            let actual = table
                .columns
                .iter()
                .map(|c| c.name.to_ascii_lowercase())
                .collect();
            if expected != actual {
                return Err(fail(
                    "unsupported death treasure schema; review this revision",
                ));
            }
        } else {
            for c in &table.columns {
                let name = c.name.to_ascii_lowercase();
                if !target.contains(&name)
                    && !(table.name == "recipe_mod"
                        && matches!(name.as_str(), "index" | "requires"))
                {
                    return Err(fail(format!(
                        "unsupported source column {}.{}",
                        table.name, c.name
                    )));
                }
            }
        }
        Ok(())
    }

    fn row(&mut self, row: &Row<'_>) -> Result<(), ImportError> {
        let cell = self.cells.cell(row)?;
        if let Some(reason) = self.quarantine(row)? {
            if cell.is_some() {
                return Err(fail("required target instance would be quarantined"));
            }
            self.record(row, "quarantine", &reason);
            return Ok(());
        }
        if let Some(after) = cell {
            self.record(row, "adapt", "named October asset target cell placement");
            let record = self.ledger.last_mut().expect("just recorded adaptation");
            record["target"] = self.cells.target().name().into();
            record["adapter"] = super::cells::ADAPTER.into();
            record["changes"] = json!({"obj_Cell_Id":{
                "before":number(row, "obj_Cell_Id")?, "after":after}});
        }
        let normalize = row.table == "quest" && number(row, "min_Delta")? < 0;
        if normalize {
            if number(row, "min_Delta")? != -1 {
                return Err(fail("unrecognized negative quest cooldown"));
            }
            self.record(
                row,
                "normalize",
                "quest cooldown -1 means no cooldown; written as zero",
            );
        }
        if self.batch == 0 {
            let columns = row
                .columns()
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(",");
            self.bytes(format!("INSERT INTO `{}` ({columns}) VALUES\n", row.table).as_bytes())?;
        } else {
            self.bytes(b",\n")?;
        }
        self.bytes(b"(")?;
        for (i, (column, value)) in row.columns().iter().zip(row.values()).enumerate() {
            if i != 0 {
                self.bytes(b",")?;
            }
            if normalize && column.eq_ignore_ascii_case("min_Delta") {
                self.bytes(b"0")?;
                continue;
            }
            if let Some(cell) = cell.filter(|_| column.eq_ignore_ascii_case("obj_Cell_Id")) {
                self.bytes(cell.to_string().as_bytes())?;
                continue;
            }
            match value {
                Value::Null => self.bytes(b"NULL")?,
                Value::Bare(b) => self.bytes(b)?,
                Value::Quoted(b) => self.bytes(&literal(b))?,
            }
        }
        self.bytes(b")")?;
        self.batch += 1;
        *self.counts.entry(row.table.into()).or_default() += 1;
        if self.batch == 500 {
            self.finish()?;
        }
        Ok(())
    }
}

pub fn write_schema(out: &mut impl Write, canonical: &Store) -> Result<(), String> {
    out.write_all(b"-- ACE-schema projection. Use an empty database. See manifest.json and changes.json.\nSET NAMES utf8mb4;\nSET FOREIGN_KEY_CHECKS=0;\nSET SQL_MODE='STRICT_ALL_TABLES,NO_AUTO_VALUE_ON_ZERO';\n")
        .map_err(|e|e.to_string())?;
    for i in canonical.table_indices() {
        let table = canonical.table(i).ok_or("missing canonical table")?;
        writeln!(out, "{};", table.create_sql.trim_end_matches(';')).map_err(|e| e.to_string())?;
    }
    Ok(())
}
