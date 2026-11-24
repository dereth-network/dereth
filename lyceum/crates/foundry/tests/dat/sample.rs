//! Behaviour: none (tooling: the sample world built, baked and exported over the retail files, and its changes to the game's own tables compiled into its overlay)

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dereth_assets::world::{CellLandblock, LandblockInfo};
use dereth_assets::{Decode, Region, SurfaceTexture};
use dereth_dat::testing::ScratchDir;
use dereth_dat::{DatFile, ModernDat};
use dereth_primitives::DataId;
use lyceum_foundry::export::{self, Era, Importer, Options, Target};
use lyceum_foundry::{bake, compile};

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dest = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// The sample world copied into a scratch folder, its output kept there too.
pub fn sample(tag: &str) -> (ScratchDir, PathBuf) {
    dereth_dat::testing::require_dats();
    let dir = ScratchDir::new(tag).unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample");
    copy_tree(&from.join("holtburg"), &dir.path().join("holtburg"));
    std::fs::copy(from.join("world.lock"), dir.path().join("world.lock")).unwrap();
    let world = std::fs::read_to_string(from.join("world.toml")).unwrap();
    let world: String = world
        .lines()
        .map(|l| {
            if l.starts_with("out ") {
                "out = \"build\""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.path().join("world.toml"), world).unwrap();
    let path = dir.path().join("world.toml");
    (dir, path)
}

#[test]
fn the_sample_world_builds_over_the_retail_files_with_the_ids_its_lockfile_keeps() {
    let (dir, world) = sample("pack-sample-build");
    let dats = dereth_dat::testing::dat_dir();
    let built = compile::build(&world, &dats, None, false).unwrap();
    assert!(
        built.numbered.is_empty(),
        "every id is in the lockfile: {:?}",
        built.numbered
    );
    let committed = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample/world.lock"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("world.lock")).unwrap(),
        committed,
        "the build leaves the lockfile as it is"
    );
    assert!(compile::build(&world, &dats, None, true).unwrap().current);
    let set = lyceum_foundry::dats::DatSet::open(&dats)
        .unwrap()
        .with_overlay(&dir.path().join("build/overlay"), Some("sample-world"))
        .unwrap();
    // The new picture's texture, under the id the lockfile gives it.
    let st = SurfaceTexture::decode_payload(
        DataId(0x0500_3359),
        &set.portal.read(DataId(0x0500_3359)).unwrap(),
    )
    .unwrap();
    assert_eq!(st.source_levels, [DataId(0x0600_7577)]);
    // West of Holtburg: a statics record made, and the terrain flagged for it.
    let info = LandblockInfo::decode_payload(
        DataId(0xA8B4_FFFE),
        &set.cell.read(DataId(0xA8B4_FFFE)).unwrap(),
    )
    .unwrap();
    assert_eq!(info.objects.len(), 3);
    let west = CellLandblock::decode_payload(
        DataId(0xA8B4_FFFF),
        &set.cell.read(DataId(0xA8B4_FFFF)).unwrap(),
    )
    .unwrap();
    assert_eq!(west.lbi_exists, 1);
    // The hill on Holtburg's west edge is in both landblocks.
    let holtburg = CellLandblock::decode_payload(
        DataId(0xA9B4_FFFF),
        &set.cell.read(DataId(0xA9B4_FFFF)).unwrap(),
    )
    .unwrap();
    assert_eq!((holtburg.height[1], holtburg.height[2]), (50, 50));
    assert_eq!((west.height[8 * 9 + 1], west.height[8 * 9 + 2]), (50, 50));
    // Holtburg keeps its 114 statics and gains three. The two placed on terrain vertices (1, 1)
    // and (2, 1) stand at those vertices' heights, as the region's height table gives them.
    let info = LandblockInfo::decode_payload(
        DataId(0xA9B4_FFFE),
        &set.cell.read(DataId(0xA9B4_FFFE)).unwrap(),
    )
    .unwrap();
    assert_eq!(info.objects.len(), 117);
    let region = Region::decode_payload(
        DataId(0x1300_0000),
        &set.portal.read(DataId(0x1300_0000)).unwrap(),
    )
    .unwrap();
    let table = dereth_terrain::land::mesh::height_table(&region);
    for (o, vertex) in [(114, 9 + 1), (115, 2 * 9 + 1)] {
        let at = info.objects[o].frame.origin;
        let want = table[usize::from(holtburg.height[vertex])];
        assert!((at.z - want).abs() < 0.001, "{at:?} stands at {want}");
    }
    // The SQL names the world's ids.
    let spawn = std::fs::read_to_string(
        dir.path()
            .join("build/sql/holtburg/landblocks/A9B4 guide.sql"),
    )
    .unwrap();
    assert!(
        spawn.contains("VALUES (0x7A9B4FFF, 100000, 0xA9B40009"),
        "{spawn}"
    );
}

#[test]
fn the_sample_world_bakes_into_copies_of_the_retail_files() {
    let (dir, world) = sample("pack-sample-bake");
    let dats = dereth_dat::testing::dat_dir();
    compile::build(&world, &dats, None, false).unwrap();
    let to = dir.path().join("baked");
    let baked = bake::bake(&dir.path().join("build/overlay"), &dats, &to, true).unwrap();
    assert_eq!(baked.world_key, "sample-world");
    assert_eq!(baked.written.len(), 2, "the portal and cell files");
    let cell = DatFile::open(&ModernDat::Cell.in_dir(&to)).unwrap();
    let portal = DatFile::open(&ModernDat::Portal.in_dir(&to)).unwrap();
    // The base's iterations and one more.
    assert_eq!(
        cell.iteration_list().unwrap(),
        (1..=983).collect::<Vec<_>>()
    );
    assert_eq!(
        portal.iteration_list().unwrap(),
        (1..=2073).collect::<Vec<_>>()
    );
    assert!(cell.contains(DataId(0xA8B4_FFFE)));
    assert!(portal.contains(DataId(0x0600_7577)));
    assert_eq!(cell.entry(DataId(0xA8B4_FFFE)).unwrap().iteration, 983);
}

/// An import tool that writes what `gdle` writes for the sample and keeps the arguments.
#[derive(Default)]
struct Gdle {
    runs: RefCell<Vec<Vec<String>>>,
}

impl Importer for Gdle {
    fn run(&self, args: &[OsString]) -> Result<String, String> {
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        self.runs.borrow_mut().push(args.clone());
        let out = Path::new(&args[args.iter().position(|a| a == "--out").unwrap() + 1]);
        for (folder, file) in [
            ("weenies", "100000 - sample-gribble.json"),
            ("landblocks", "A9B4.json"),
        ] {
            std::fs::create_dir_all(out.join(folder)).unwrap();
            std::fs::write(out.join(folder).join(file), "{}").unwrap();
        }
        // The server's own list, when it is named, with the sample's quest after it.
        let given = args
            .iter()
            .position(|a| a == "--quests")
            .map(|i| std::fs::read_to_string(&args[i + 1]).unwrap());
        let ours = r#"{"key": "GribbleGreeted", "value": {"fullname": null, "mindelta": 0, "maxsolves": -1}}"#;
        let list = match given {
            Some(text) => format!("{}, {ours}]", text.trim().trim_end_matches(']')),
            None => format!("[{ours}]"),
        };
        std::fs::write(out.join("quests.json"), list).unwrap();
        Ok("wrote".into())
    }
}

#[test]
fn the_sample_world_exported_for_ace_is_baked_files_and_aces_content_folder() {
    let (dir, world) = sample("export-ace");
    let dats = dereth_dat::testing::dat_dir();
    let out = dir.path().join("for-ace");
    let opts = Options {
        target: Target::Ace,
        out: &out,
        era: Era::Eor,
        world_sql: None,
        importer: None,
        gdle_quests: None,
    };
    let done = export::export(&world, &dats, &opts).unwrap();
    assert_eq!(done.out, out);
    // The two files the sample changes, baked; the others are the player's own.
    let baked = out.join("dats");
    let cell = DatFile::open(&ModernDat::Cell.in_dir(&baked)).unwrap();
    let portal = DatFile::open(&ModernDat::Portal.in_dir(&baked)).unwrap();
    assert!(
        cell.contains(DataId(0xA8B4_FFFE)),
        "the statics record made"
    );
    assert!(portal.contains(DataId(0x0600_7577)), "the new picture");
    assert!(!ModernDat::HighRes.in_dir(&baked).exists());
    // The content where ACE's import finds it, its ids filled in.
    let weenie = std::fs::read_to_string(out.join("sql/weenies/100000 guide.sql")).unwrap();
    assert!(weenie.contains("'Gribble the Painted'") && !weenie.contains("{{"));
    let spawn =
        std::fs::read_to_string(out.join("sql/landblocks/A9B4 holtburg guide.sql")).unwrap();
    assert!(
        spawn.contains("0x7A9B4FFF") && spawn.contains("100000"),
        "{spawn}"
    );
    let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
    assert!(readme.contains("/import-sql weenie all"), "{readme}");
    // A second export replaces the first; a folder of other files is refused.
    export::export(&world, &dats, &opts).unwrap();
    let other = dir.path().join("mine");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("notes.txt"), "mine").unwrap();
    let refused = export::export(
        &world,
        &dats,
        &Options {
            out: &other,
            ..opts
        },
    );
    assert!(refused.is_err());
    assert!(other.join("notes.txt").is_file());
}

#[test]
fn the_sample_world_exported_for_gdle_is_baked_files_and_gdles_folders() {
    let (dir, world) = sample("export-gdle");
    let dats = dereth_dat::testing::dat_dir();
    let dump = dir.path().join("ACE-World-Database-v0.9.295.sql");
    std::fs::write(&dump, "-- the world database").unwrap();
    let out = dir.path().join("for-gdle");
    let tool = Gdle::default();
    let opts = Options {
        target: Target::Gdle,
        out: &out,
        era: Era::Eor,
        world_sql: Some(&dump),
        importer: Some(&tool),
        gdle_quests: None,
    };
    export::export(&world, &dats, &opts).unwrap();
    let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
    assert!(
        readme.contains("the quests this world adds or changes"),
        "{readme}"
    );
    // Over the server's own quests.json, the merged list is written and the README says what the
    // world adds; a file that is not there is refused before anything is run or replaced.
    let given = dir.path().join("server-quests.json");
    std::fs::write(
        &given,
        r#"[{"key": "ServerQuest", "value": {"fullname": "Kept", "mindelta": 5, "maxsolves": 1}}]"#,
    )
    .unwrap();
    export::export(
        &world,
        &dats,
        &Options {
            gdle_quests: Some(&given),
            ..opts
        },
    )
    .unwrap();
    let list: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("json/quests.json")).unwrap())
            .unwrap();
    assert_eq!(list.as_array().map(Vec::len), Some(2));
    let merged = std::fs::read_to_string(out.join("README.txt")).unwrap();
    assert!(
        merged.contains("This world adds: GribbleGreeted."),
        "{merged}"
    );
    let missing = dir.path().join("no-such-quests.json");
    let refused = export::export(
        &world,
        &dats,
        &Options {
            gdle_quests: Some(&missing),
            ..opts
        },
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("not a file"), "{refused}");
    assert!(out.join("json/quests.json").is_file(), "the export stands");
    let runs = tool.runs.borrow();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0][0], "gdle");
    assert_eq!(runs[0][2], dump.to_string_lossy());
    assert!(!runs[0].iter().any(|a| a == "--quests"));
    let at = runs[1].iter().position(|a| a == "--quests").unwrap();
    assert_eq!(runs[1][at + 1], given.to_string_lossy());
    assert!(DatFile::open(&ModernDat::Cell.in_dir(&out.join("dats")))
        .unwrap()
        .contains(DataId(0xA8B4_FFFE)));
    // GDLE reads a landblock's spawns from spawnMaps.
    assert!(out.join("json/spawnMaps/A9B4.json").is_file());
    assert!(!out.join("json/landblocks").exists());
    assert!(out
        .join("json/weenies/100000 - sample-gribble.json")
        .is_file());
    let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
    assert!(readme.contains("Data/json"), "{readme}");
    assert!(!out.join(".lyceum-staging").exists());
}

/// The sample world (statics, a hill, a new texture and an NPC wearing it) builds over the
/// February 2005 files with its records in their layout, and the client's store over those files
/// reads them.
#[test]
fn the_sample_world_builds_on_the_february_2005_files() {
    let (scratch, world) = super::pretod::infiltration_sample("classic-sample");
    let built = compile::build(&world, &super::pretod::dir(), None, false).unwrap();
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let od = dereth_dat::overlay::OverlayDir::new(&scratch.path().join("build/overlay")).unwrap();
    assert_eq!(od.base_era(), Some(dereth_dat::ContainerEra::Classic));
    let store = dereth_dat::RetailDatStore::open_classic_dir(&super::pretod::dir())
        .unwrap()
        .with_overlay(&od, Some("sample-world"))
        .unwrap();
    // The new texture holds its picture's pixels, as the older textures do.
    let texture = DataId(0x0500_3359);
    let bytes = store.read_portal(texture).unwrap();
    let rs = dereth_assets::RenderSurface::from_classic_texture(texture, &bytes).unwrap();
    assert!(rs.width > 0 && rs.payload(&bytes).is_some());
    // The statics west of Holtburg are there.
    let id = DataId(0xA8B4_FFFE);
    let info = dereth_assets::world::LandblockInfo::decode_payload_for(
        super::pretod::ERA,
        id,
        &store.read_cell(id).unwrap(),
    )
    .unwrap();
    assert_eq!(info.objects.len(), 3);
}

// ---------------------------------------------------------------------------------------------
// The game's tables a pack changes
// ---------------------------------------------------------------------------------------------

mod tables {
    //! Behaviour: none (tooling: a world's changes to the game's own tables compiled into its overlay, over the retail files)

    use super::sample;
    use dereth_assets::tables::{CharGen, SkillTable, SpellTable, XpTable};
    use dereth_assets::world::CellLandblock;
    use dereth_assets::{Decode, Region};
    use dereth_dat::ModernDat;
    use dereth_primitives::DataId;
    use lyceum_foundry::compile::{self, Compiled};
    use lyceum_foundry::dats::DatSet;
    use lyceum_foundry::tables::{CHARGEN, EXPERIENCE, SKILLS, SPELLS};
    use lyceum_foundry::world::World;

    /// The sample world with `rows` as the sample pack's `records/rules.toml`, and `rival` (when given)
    /// as a second pack's, loaded after it; compiled over the retail files.
    fn compiled(
        tag: &str,
        rows: &str,
        rival: Option<&str>,
        world_extra: &str,
    ) -> Result<Compiled, String> {
        let (dir, world) = sample(tag);
        let root = dir.path();
        std::fs::write(root.join("holtburg/records/rules.toml"), rows).unwrap();
        if let Some(rival) = rival {
            std::fs::create_dir_all(root.join("rival/records")).unwrap();
            std::fs::write(
                root.join("rival/pack.toml"),
                "name = \"rival\"\nformat_version = 1\nversion = \"0.1.0\"\nauthors = [\"Dereth\"]\n",
            )
            .unwrap();
            std::fs::write(root.join("rival/records/rules.toml"), rival).unwrap();
            let text = std::fs::read_to_string(&world).unwrap();
            std::fs::write(
                &world,
                text.replace(
                    "packs = [\"holtburg\"]",
                    "packs = [\"holtburg\", \"rival\"]",
                ) + world_extra,
            )
            .unwrap();
        }
        let w = World::read(&world).map_err(|e| e.to_string())?;
        let packs = w
            .packs
            .iter()
            .map(|p| lyceum_foundry::source::read_pack(p))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let lock = lyceum_foundry::lock::Lock::read(&w.lock_path()).unwrap();
        let base = DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
        compile::compile(&w, &packs, &base, &lock).map_err(|e| e.to_string())
    }

    fn base() -> DatSet {
        DatSet::open(&dereth_dat::testing::dat_dir()).unwrap()
    }

    /// A table as the build compiled it, or `None` when the build left it to the base.
    fn table<T: Decode>(c: &Compiled, id: u32) -> Option<T> {
        let s = c.records.get(&ModernDat::Portal)?.get(&id)?;
        Some(T::decode_payload(DataId(id), &s.bytes).unwrap())
    }

    /// A table as the retail files hold it.
    fn retail<T: Decode>(id: u32) -> T {
        T::decode_payload(DataId(id), &base().read_portal(DataId(id)).unwrap()).unwrap()
    }

    const ROWS: &str = r#"
    [[skill]]
    skill = "melee defense"
    trained_cost = 4
    specialized_cost = 8

    [[skill]]
    id = 1
    like = "Heavy Weapons"
    name = "Axe"
    description = "Helps you fight with axes."
    formula = { attributes = ["Strength", "Coordination"], divisor = 3 }

    [[experience]]
    max_level = 20

    [[level]]
    level = 2
    xp = 900
    credits = 2

    [[spell]]
    id = 6
    name = "Mend Self I"
    mana = 10

    [[starter-area]]
    name = "Holtburg Gate"
    heritages = ["Aluvian"]
    at = [{ cell = 0xA9B40025, at = [100.0, 100.0, "ground"], heading = 90.0 }]

    [[heritage]]
    heritage = "aluvian"
    skill_credits = 60

    [[heritage-skill]]
    heritage = "Aluvian"
    skill = "Axe"
    trained_cost = 6
    specialized_cost = 10

    [[heritage-template]]
    heritage = "Aluvian"
    name = "Axeman"
    strength = 100
    trained = ["Axe", "Melee Defense"]
    specialized = ["Axe"]
    "#;

    /// Each table a pack changes is compiled into the overlay over the player's own, changed only where
    /// the rows say: a skill's costs, a new skill copied from another and named, a shorter curve with one
    /// level changed, a renamed spell, a new starter area standing on the ground for one heritage, a
    /// heritage's credits, a skill's cost to it and a new template.
    #[test]
    fn each_table_a_pack_changes_compiles_over_the_players_own_where_its_rows_say() {
        let c = compiled("tables-each", ROWS, None, "").unwrap();

        let was: SkillTable = retail(SKILLS);
        let mut skills: SkillTable = table(&c, SKILLS).expect("the skill table");
        let md = &skills.skills[&6];
        assert_eq!((md.trained_cost, md.specialized_cost), (4, 8));
        let axe = skills.skills.remove(&1).expect("Axe");
        let heavy = &was.skills[&44];
        assert_eq!(
            (axe.name.as_str(), axe.trained_cost),
            ("Axe", heavy.trained_cost)
        );
        assert_eq!((axe.formula.x, axe.formula.y, axe.formula.z), (1, 1, 3));
        assert_eq!((axe.formula.attr1, axe.formula.attr2), (1, 4));
        for (k, s) in &skills.skills {
            if *k != 6 {
                assert_eq!(s, &was.skills[k], "skill {k}");
            }
        }

        let xp_was: XpTable = retail(EXPERIENCE);
        let xp: XpTable = table(&c, EXPERIENCE).expect("the experience table");
        assert_eq!(xp.level_xp.len(), 21);
        assert_eq!((xp.level_xp[2], xp.level_credits[2]), (900, 2));
        assert_eq!(xp.level_xp[3..], xp_was.level_xp[3..21]);
        assert_eq!(xp.attribute_xp, xp_was.attribute_xp);

        let spells_was: SpellTable = retail(SPELLS);
        let spells: SpellTable = table(&c, SPELLS).expect("the spell table");
        let mend = &spells.spells[&6];
        assert_eq!((mend.name.as_str(), mend.base_mana), ("Mend Self I", 10));
        assert_eq!(
            mend.comps, spells_was.spells[&6].comps,
            "its components kept"
        );
        assert_eq!(spells.spells[&27], spells_was.spells[&27]);

        let cg_was: CharGen = retail(CHARGEN);
        let cg: CharGen = table(&c, CHARGEN).expect("character creation");
        let gate = cg.starter_areas.last().unwrap();
        assert_eq!(
            (gate.name.as_str(), gate.locations.len()),
            ("Holtburg Gate", 1)
        );
        let p = gate.locations[0];
        let land = CellLandblock::decode_payload(
            DataId(0xA9B4_FFFF),
            &base().cell.read(DataId(0xA9B4_FFFF)).unwrap(),
        )
        .unwrap();
        let region: Region = retail(0x1300_0000);
        assert_eq!(
            p.frame.origin.z,
            compile::ground(&land, &region, 0xA9B4, 100.0, 100.0)
        );
        let index = u32::try_from(cg.starter_areas.len() - 1).unwrap();
        for (k, h) in &cg.heritage_groups {
            assert_eq!(
                h.primary_start_areas.contains(&index),
                *k == 1,
                "heritage {k}"
            );
        }
        let aluvian = &cg.heritage_groups[&1];
        assert_eq!(aluvian.skill_credits, 60);
        assert_eq!(
            aluvian.attribute_credits,
            cg_was.heritage_groups[&1].attribute_credits
        );
        assert!(aluvian.skills.contains(&(1, 6, 10)));
        let axeman = aluvian.templates.last().unwrap();
        assert_eq!(
            (axeman.name.as_str(), axeman.attributes[0]),
            ("Axeman", 100)
        );
        assert_eq!(
            (axeman.normal_skills.clone(), axeman.primary_skills.clone()),
            (vec![1, 6], vec![1])
        );
        assert_eq!(cg.heritage_groups[&2], cg_was.heritage_groups[&2]);
    }

    /// A row that sets what the table below already holds changes nothing, and the build leaves the
    /// table to the player's own files.
    #[test]
    fn rows_that_change_nothing_leave_the_table_to_the_base() {
        let was: SkillTable = retail(SKILLS);
        let md = &was.skills[&6];
        let c = compiled(
            "tables-same",
            &format!(
                "[[skill]]\nskill = \"Melee Defense\"\ntrained_cost = {}\n",
                md.trained_cost
            ),
            None,
            "",
        )
        .unwrap();
        assert!(table::<SkillTable>(&c, SKILLS).is_none());
    }

    /// Two packs that set one field of a table differently are a conflict, named by the field; the
    /// world settles it by saying which pack wins, and the winner's value is compiled.
    #[test]
    fn two_packs_setting_a_tables_field_differently_conflict_until_the_world_settles_it() {
        let mine = "[[skill]]\nskill = \"Melee Defense\"\ntrained_cost = 4\n\n[[heritage]]\nheritage = \"Aluvian\"\nskill_credits = 60\n";
        let theirs = "[[skill]]\nskill = \"Melee Defense\"\ntrained_cost = 5\nspecialized_cost = 9\n\n[[heritage]]\nheritage = \"aluvian\"\nskill_credits = 61\n";
        let c = compiled("tables-conflict", mine, Some(theirs), "").unwrap();
        let items: Vec<String> = c.conflicts().iter().map(|x| x.what.clone()).collect();
        assert_eq!(
            items,
            ["skill 6 trained-cost", "heritage aluvian skill-credits"]
        );
        let settled = compiled(
            "tables-settled",
            mine,
            Some(theirs),
            "\n[[override]]\npack = \"holtburg\"\nover = \"rival\"\n",
        )
        .unwrap();
        assert!(settled.conflicts().is_empty());
        let md = &table::<SkillTable>(&settled, SKILLS).unwrap().skills[&6];
        assert_eq!((md.trained_cost, md.specialized_cost), (4, 9));
        let cg: CharGen = table(&settled, CHARGEN).unwrap();
        assert_eq!(cg.heritage_groups[&1].skill_credits, 60);
    }

    /// Rows the tables can't take are refused at the row: a skill the table doesn't have, a level the
    /// cap adds with no experience, a curve that falls, a duration on a spell that doesn't run out, a
    /// spell the table doesn't have, and the first template taken out.
    #[test]
    fn rows_the_tables_cant_take_are_refused_where_they_are() {
        let refused = |tag: &str, rows: &str, says: &str| {
            let e = compiled(tag, rows, None, "").expect_err(rows);
            assert!(e.contains("rules.toml") && e.contains(says), "{e}");
        };
        refused(
            "tables-no-skill",
            "[[skill]]\nskill = \"Basket Weaving\"\nicon = 1\n",
            "no skill \"Basket Weaving\"",
        );
        refused(
            "tables-no-xp",
            "[[experience]]\nmax_level = 300\n",
            "level 276 is new",
        );
        refused(
            "tables-falls",
            "[[level]]\nlevel = 3\nxp = 1\n",
            "takes less experience",
        );
        refused(
            "tables-duration",
            "[[spell]]\nid = 6\nduration = 60.0\n",
            "does not run out",
        );
        refused(
            "tables-no-spell",
            "[[spell]]\nid = 999999\nmana = 1\n",
            "no spell 999999",
        );
        let first = &retail::<CharGen>(CHARGEN).heritage_groups[&1].templates[0].name;
        refused(
            "tables-first-template",
            &format!(
                "[[heritage-template]]\nheritage = \"Aluvian\"\nname = {first:?}\nremove = true\n"
            ),
            "first template",
        );
    }

    /// A heritage a pack takes out is no longer in character creation, so the game no longer offers
    /// it: gone from the table and from its order, with what another row changed of it; the others
    /// are as the files have them. A pack can't take out every heritage.
    #[test]
    fn a_heritage_taken_out_is_no_longer_in_character_creation() {
        let was: CharGen = retail(CHARGEN);
        let lugian = was
            .heritage_groups
            .iter()
            .find(|(_, h)| h.name == "Lugian")
            .map(|(k, _)| *k)
            .expect("Lugian");
        let rows = "[[heritage]]\nheritage = \"Lugian\"\nremove = true\n\n[[heritage-template]]\nheritage = \"Lugian\"\nname = \"Bruiser\"\nstrength = 100\n";
        let c = compiled("tables-heritage-out", rows, None, "").unwrap();
        let cg: CharGen = table(&c, CHARGEN).expect("character creation");
        assert!(!cg.heritage_groups.contains_key(&lugian));
        let kept: Vec<u32> = was
            .heritage_order
            .iter()
            .copied()
            .filter(|k| *k != lugian)
            .collect();
        assert_eq!(
            cg.heritage_order, kept,
            "the others in the record's own order"
        );
        assert_eq!(cg.heritage_groups.len(), was.heritage_groups.len() - 1);
        for (k, h) in &cg.heritage_groups {
            assert_eq!(h, &was.heritage_groups[k], "heritage {k}");
        }
        let all: String = was
            .heritage_groups
            .values()
            .map(|h| format!("[[heritage]]\nheritage = {:?}\nremove = true\n\n", h.name))
            .collect();
        let e = compiled("tables-heritage-none", &all, None, "").expect_err("none left");
        assert!(e.contains("at least one heritage"), "{e}");
    }
}
