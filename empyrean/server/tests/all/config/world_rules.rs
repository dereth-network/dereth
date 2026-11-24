//! Divergence: V563
//! Divergence: V565
//! Divergence: V569
//! Divergence: V574
//! A world's own rules come from `world.toml` in its overlay folder; a world without an overlay,
//! or without the file, has none of them.
//! Fixture: a temporary overlay folder per test.

use std::path::{Path, PathBuf};

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_server::dat_directory::{
    world_placements, world_rules, world_weenies, WorldRules, WORLD_PLACEMENTS_FILE,
    WORLD_RULES_FILE, WORLD_WEENIES_FILE,
};

/// A fresh, empty folder of its own for the test `name`.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "empyrean-world-rules-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn base(dir: &Path) -> PathBase {
    PathBase::new(None, dir, None, None)
}

fn serving(overlay: &str) -> MasterConfiguration {
    let mut config = MasterConfiguration::default();
    config.dat_overlay.path = overlay.to_owned();
    config
}

#[test]
fn a_world_without_an_overlay_has_no_rules_of_its_own() {
    let dir = folder("none");
    assert_eq!(
        world_rules(&serving(""), &base(&dir)),
        Ok(WorldRules::default())
    );
    assert!(!WorldRules::default().dungeons_by_cells);
    assert!(!WorldRules::default().own_placements);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_overlay_without_the_file_has_no_rules_of_its_own() {
    let dir = folder("absent");
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    assert_eq!(
        world_rules(&serving("overlay"), &base(&dir)),
        Ok(WorldRules::default())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_overlays_world_file_says_the_land_lies_over_the_dungeons() {
    let dir = folder("set");
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    std::fs::write(
        dir.join("overlay").join(WORLD_RULES_FILE),
        "dungeons_by_cells = true\n",
    )
    .unwrap();
    let rules = world_rules(&serving("overlay"), &base(&dir)).unwrap();
    assert!(rules.dungeons_by_cells);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_world_file_that_is_not_valid_is_refused() {
    let dir = folder("bad");
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    std::fs::write(
        dir.join("overlay").join(WORLD_RULES_FILE),
        "dungeons_by_cells = \"yes\"\n",
    )
    .unwrap();
    let refused = world_rules(&serving("overlay"), &base(&dir)).unwrap_err();
    assert!(refused.contains(WORLD_RULES_FILE), "{refused}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_overlays_world_file_says_the_world_places_its_own_things() {
    let dir = folder("own");
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    std::fs::write(
        dir.join("overlay").join(WORLD_RULES_FILE),
        "dungeons_by_cells = true\nown_placements = true\n",
    )
    .unwrap();
    let rules = world_rules(&serving("overlay"), &base(&dir)).unwrap();
    assert!(rules.own_placements);
    assert!(rules.dungeons_by_cells);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_overlays_placements_file_is_read_beside_its_rules_and_none_is_none() {
    let dir = folder("placements");
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    assert_eq!(world_placements(&serving("overlay"), &base(&dir)), Ok(None));
    assert_eq!(world_placements(&serving(""), &base(&dir)), Ok(None));
    let table = "0x7A9B4F00	24037	0xA9B40100	10	20	30	1	0	0	0
";
    std::fs::write(dir.join("overlay").join(WORLD_PLACEMENTS_FILE), table).unwrap();
    let (file, text) = world_placements(&serving("overlay"), &base(&dir))
        .unwrap()
        .expect("the file");
    assert_eq!(file, dir.join("overlay").join(WORLD_PLACEMENTS_FILE));
    assert_eq!(text, table);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_overlays_weenies_file_is_read_beside_its_placements_and_none_is_none() {
    let dir = folder("weenies");
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    assert_eq!(world_weenies(&serving("overlay"), &base(&dir)), Ok(None));
    assert_eq!(world_weenies(&serving(""), &base(&dir)), Ok(None));
    let table = "15728641	kitdoor	278	did.1=0x0200F000
";
    std::fs::write(dir.join("overlay").join(WORLD_WEENIES_FILE), table).unwrap();
    let (file, text) = world_weenies(&serving("overlay"), &base(&dir))
        .unwrap()
        .expect("the file");
    assert_eq!(file, dir.join("overlay").join(WORLD_WEENIES_FILE));
    assert_eq!(text, table);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_land_that_places_its_own_things_keeps_only_the_placements_of_a_world_made_for_it() {
    use empyrean_common::era::{EraExt, EraId};
    let own = WorldRules {
        own_placements: true,
        ..WorldRules::default()
    };
    // A world database made for AC1's land is hidden on it...
    assert!(own.hides_world_placements(EraId::Eor.rules()));
    assert!(own.hides_world_placements(EraId::Launch.rules()));
    // ...and New Dereth's, made for the converted land, is kept.
    assert!(!own.hides_world_placements(EraId::NewDereth.rules()));
    // A land that places nothing of its own hides nothing.
    for era in EraId::ALL {
        assert!(!WorldRules::default().hides_world_placements(era.rules()));
    }
}

/// A world's rules are its era's, with the rules its pack carries over them and the operator's
/// `[era]` keys over those; a pack without rules leaves the era's.
#[test]
fn a_worlds_rules_are_its_eras_with_its_packs_over_them_and_the_operators_keys_over_those() {
    use empyrean_common::era::{EraConfiguration, EraId, StartPositions};
    use empyrean_common::world_rules::{Start, WorldRules};
    use empyrean_content::{MemContent, WorldContent};
    use empyrean_server::world_pack::world_era;
    let mut era = EraConfiguration {
        profile: EraId::NewDereth,
        ..EraConfiguration::default()
    };
    era.features.set("housing", true);
    let rules = WorldRules {
        max_level: Some(40),
        start: Some(Start::CharGen),
        features: [
            ("housing".to_owned(), false),
            ("aetheria".to_owned(), false),
        ]
        .into(),
        ..WorldRules::default()
    };
    let with = |rules: Option<WorldRules>| {
        MemContent::from_content(WorldContent {
            era: EraId::NewDereth,
            rules,
            ..WorldContent::default()
        })
    };
    let r = world_era(&era, &with(Some(rules.clone()))).expect("rules");
    assert_eq!(
        (r.max_level, r.start_positions),
        (Some(40), StartPositions::FromCharGen)
    );
    assert!(r.features.housing && !r.features.aetheria);
    assert!(std::ptr::eq(
        world_era(&era, &with(None)).unwrap(),
        era.rules()
    ));
}
