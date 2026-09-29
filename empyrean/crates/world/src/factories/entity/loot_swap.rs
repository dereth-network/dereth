// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Entity/LootSwap.cs
//! Port of `Source/ACE.Server/Factories/Entity/LootSwap.cs`.
//!
//! `/lootswap <folder>`: re-reads the chance tables from a folder of edited `Factories/Tables`
//! sources and swaps them into the running server by reflection. Parsing is ported; the swap is
//! not: the tables are immutable `static`s generated into empyrean-tables, and there is no
//! reflection to find a class's static fields. [`update_table`] and the two [`reflection`]
//! members are honest stubs, as there is no mutable table registry.

use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::Instant;

use empyrean_common::dotnet::DotNetDict;

use super::loot_parser::{self, ParsedTable};

/// A parsed folder: class name to its parsed tables.
pub type ParsedFiles = DotNetDict<String, DotNetDict<String, Option<ParsedTable>>>;

/// Parses every table file under `path` and swaps the tables into the running server.
// ACE: LootSwap.UpdateTables
pub fn update_tables(path: &str) {
    let timer = Instant::now();

    let types = reflection::get_types("ACE.Server.Factories.Tables");

    let mut files = ParsedFiles::new();

    parse_folder(path, &mut files);

    update_tables_with(&types, &files);

    // ~0.2s
    empyrean_common::console_write_line!();
    empyrean_common::console_write_line!(
        "Updated loot tables in {}s",
        timer.elapsed().as_secs_f64()
    );
    empyrean_common::console_write_line!();
}

static EXCLUDE_LIST: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "GemMaterialChance.cs", // todo
        "TreasureItemTypeChances.cs",
    ])
});

/// Parses the `.cs` files of `folder` (except the excluded ones), then its subfolders, in the
/// file system's enumeration order (`DirectoryInfo.GetFiles` / `GetDirectories`).
///
/// # Panics
/// Two files share a class name (`Dictionary.Add` throws), or a file cannot be read.
// ACE: LootSwap.ParseFolder
pub fn parse_folder(folder: &str, files: &mut ParsedFiles) {
    let di = std::path::Path::new(folder);

    if !di.is_dir() {
        empyrean_common::console_write_line!("{folder} not found");
        return;
    }

    let mut dir_files = Vec::new();
    let mut subfolders = Vec::new();
    if let Ok(entries) = std::fs::read_dir(di) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subfolders.push(path);
            } else {
                dir_files.push(path);
            }
        }
    }

    for file in dir_files {
        let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if !name.ends_with(".cs") {
            continue;
        }

        if EXCLUDE_LIST.contains(name) {
            continue;
        }

        let class_name = name.replace(".cs", "");
        let result = loot_parser::parse_file(&file.to_string_lossy())
            .expect("IOException reading a loot table file");
        assert!(
            !files.contains_key(&class_name),
            "ArgumentException: An item with the same key has already been added. Key: {class_name}"
        );
        files.insert(class_name, result);
    }

    for subfolder in subfolders {
        parse_folder(&subfolder.to_string_lossy(), files);
    }
}

/// `UpdateTables(types, files)`: for each table class, each of its chance-table fields that the
/// parsed file has is replaced.
fn update_tables_with(types: &DotNetDict<String, String>, files: &ParsedFiles) {
    for (class_name, r#type) in types.iter() {
        let Some(new_tables) = files.get(class_name) else {
            //Console.WriteLine($"Couldn't find {className} in files");
            continue;
        };

        let fields = reflection::get_fields(r#type);

        empyrean_common::console_write_line!("Updated {class_name}");

        for (_table_type, field) in fields {
            let Some(new_table) = new_tables.get(&field) else {
                //Console.WriteLine($"Couldn't find {field.field.Name} in {className}.cs");
                continue;
            };
            update_table(&field, new_table.as_ref());
        }
    }
}

/// `field.SetValue(null, newTable)`.
// ACE: LootSwap.UpdateTable
fn update_table(_field: &str, _new_table: Option<&ParsedTable>) {
    //Console.WriteLine($"Updating {field.Name}");

    empyrean_common::not_ported!("ACE: LootSwap.UpdateTable");
}

/// ACE `LootSwap.Reflection`: the loot table classes and their chance-table fields, found by
/// reflection over ACE's assembly.
pub mod reflection {
    use empyrean_common::dotnet::DotNetDict;
    use empyrean_tables::enums::TreasureTableType;

    /// Every type whose full name starts with `prefix`, by short name.
    // ACE: LootSwap.Reflection.GetTypes
    #[must_use]
    pub fn get_types(_prefix: &str) -> DotNetDict<String, String> {
        empyrean_common::not_ported!("ACE: LootSwap.Reflection.GetTypes");
        DotNetDict::new()
    }

    /// The private static `ChanceTable<T>` fields of a type, with their table type.
    // ACE: LootSwap.Reflection.GetFields
    #[must_use]
    pub fn get_fields(_type: &str) -> Vec<(TreasureTableType, String)> {
        empyrean_common::not_ported!("ACE: LootSwap.Reflection.GetFields");
        Vec::new()
    }
}
