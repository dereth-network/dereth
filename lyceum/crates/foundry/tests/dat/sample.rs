//! Behaviour: none (tooling: the sample world built, baked and exported over the retail files)

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
