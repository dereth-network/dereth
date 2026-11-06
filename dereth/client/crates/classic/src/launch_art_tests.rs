//! Every picture this interface can draw on a Launch world is in the Launch portal, is made from
//! what is there, or belongs to a system that world does not have. Reads the portal named by the
//! Launch fixture, `EMPYREAN_TEST_LAUNCH_FIXTURE`, and nothing else: the later files are never
//! opened. Two walks: every panel's frames on a world in the states that draw the most, and every
//! picture id named in this crate's source, which catches what those states do not reach.
//!
//! Behaviour: none (a check of the interface's art against one data set)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_classic_dat::fonts::{FontAtlas, FontSource, FontSpec};
use dereth_client_contract::view::{FellowEntry, FellowshipView};
use dereth_primitives::{EraFeatures, EraId, ObjectId};

use crate::art::ClassicArt;
use crate::panels::{Context, GameView, PanelFrame};

/// A system's name, whether a world has it, and the pictures only it draws.
type System = (&'static str, fn(&EraFeatures) -> bool, &'static [u32]);

/// Pictures named in the source that are not drawn on a Launch world, with the system that would
/// draw them, and that system must be one Launch lacks.
const SYSTEMS: [System; 9] = [
    (
        "trade",
        |f| f.trade,
        &[
            0x0600_1DAE,
            0x0600_1DBB,
            0x0600_1DBC,
            0x0600_1DBD,
            0x0600_1DBE,
            0x0600_1DBF,
            0x0600_1DC0,
            0x0600_1DC1,
            0x0600_1DC2,
            0x0600_1DC3,
            0x0600_1DC5,
            0x0600_1DC6,
        ],
    ),
    (
        "tinkering",
        |f| f.tinkering,
        &[0x0600_2344, 0x0600_2345, 0x0600_2346],
    ),
    (
        "chess",
        |f| f.chess,
        &[
            0x0600_1FC9,
            0x0600_1FCA,
            0x0600_1FCB,
            0x0600_1FCC,
            0x0600_1FCD,
            0x0600_1FCE,
            0x0600_1FCF,
            0x0600_1FD0,
            0x0600_1FD1,
            0x0600_1FD2,
            0x0600_1FD3,
            0x0600_1FD4,
            0x0600_1FD5,
            0x0600_1FD6,
            0x0600_1FD7,
            0x0600_1FD8,
            0x0600_1FD9,
            0x0600_2817,
        ],
    ),
    (
        "housing",
        |f| f.housing,
        &[
            0x0600_223E,
            0x0600_223F,
            0x0600_2240,
            0x0600_2279,
            0x0600_22BA,
            0x0600_22BC,
            0x0600_22BD,
            0x0600_2347,
            0x0600_2348,
            0x0600_2349,
        ],
    ),
    (
        "spell_tabs",
        |f| f.spell_tabs,
        &[
            0x0600_1CB5,
            0x0600_1CB6,
            0x0600_1CB7,
            0x0600_1CB8,
            0x0600_1CB9,
            0x0600_1CBA,
            0x0600_1CBB,
            0x0600_1CBC,
            0x0600_1CBD,
            0x0600_1CBE,
            0x0600_1CBF,
            0x0600_2622,
            0x0600_2623,
            0x0600_2624,
            0x0600_2625,
        ],
    ),
    (
        "journal",
        |f| f.journal || f.contracts,
        &[0x0600_216B, 0x0600_69AE, 0x0600_69AF],
    ),
    ("cloaks", |f| f.cloaks, &[0x0600_32BF, 0x0600_708F]),
    ("trinkets", |f| f.trinkets, &[0x0600_32C8, 0x0600_6A6C]),
    (
        "aetheria",
        |f| f.aetheria,
        &[0x0600_32D4, 0x0600_6BEF, 0x0600_6BF0, 0x0600_6BF1],
    ),
];

/// Numbers in the source that look like a picture's id and are not one drawn: the base a credit
/// photo's number is added to, the first id of the composed range, and a dragged item's pointer
/// decoration, which nothing calls for.
const NOT_DRAWN: [u32; 3] = [0x0600_0000, 0x06F0_0000, 0x0600_1348];

/// The panels a walk asks for frames from; one the world refuses is left out.
const PANELS: [&str; 39] = [
    "hud",
    "radar",
    "vitals",
    "chat",
    "combat",
    "character-info",
    "attributes",
    "character-stats",
    "skills",
    "spellbook",
    "components",
    "beneficial-effects",
    "harmful-effects",
    "vitae",
    "inventory",
    "external-container",
    "examine",
    "spell-favorites",
    "shortcuts",
    "social",
    "allegiance",
    "fellowship",
    "friends",
    "squelch",
    "vendor",
    "map",
    "link-status",
    "options",
    "character-options",
    "sound-graphics",
    "abuse",
    "urgent-assistance",
    "book",
    "maintenance",
    "game-center",
    "trade",
    "salvage",
    "house",
    "titles",
];

struct NoFonts;
impl FontSource for NoFonts {
    fn rasterize(&self, _: &FontSpec) -> Result<FontAtlas, String> {
        Ok(FontAtlas::default())
    }
}

/// The portal the fixture manifest names.
fn launch_portal() -> PathBuf {
    let manifest = std::env::var_os("EMPYREAN_TEST_LAUNCH_FIXTURE")
        .expect("set EMPYREAN_TEST_LAUNCH_FIXTURE to the hash-pinned fixture manifest");
    let text = std::fs::read_to_string(manifest).expect("the fixture manifest");
    assert!(
        text.contains("\"capture\": \"launch\""),
        "this check requires the launch dats"
    );
    let (_, rest) = text
        .split_once("\"dat_directory\":")
        .expect("a data folder");
    let dir = rest.split('"').nth(1).expect("a data folder");
    dereth_dat::ClassicDat::Portal.in_dir(Path::new(dir))
}

fn art() -> Arc<ClassicArt> {
    let portal = dereth_classic_dat::ClassicPortal::open(&launch_portal()).expect("the portal");
    Arc::new(ClassicArt::new(portal, &NoFonts).expect("no fonts to draw"))
}

/// A Launch world in the states that draw the most: a fellowship, spells on the character of both
/// kinds, a selected item with mana, a vitae penalty.
#[derive(Debug)]
struct Launch {
    era: dereth_client_contract::EraView,
    meters: (Option<f32>, Option<f32>),
    /// A stack is selected, which draws the splitter.
    stack: bool,
}
impl GameView for Launch {
    fn era(&self) -> Option<&dereth_client_contract::EraView> {
        Some(&self.era)
    }
    fn player(&self) -> Option<ObjectId> {
        Some(ObjectId(1))
    }
    fn selected_object(&self) -> Option<ObjectId> {
        self.stack.then_some(ObjectId(2))
    }
    fn split_size(&self) -> i32 {
        4
    }
    fn max_split_size(&self) -> i32 {
        if self.stack {
            10
        } else {
            0
        }
    }
    fn enchantment_counts(&self) -> (u32, u32) {
        (1, 1)
    }
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        self.meters
    }
    fn vitae(&self) -> Option<f32> {
        Some(0.9)
    }
    fn fellowship(&self) -> Option<FellowshipView> {
        Some(FellowshipView {
            leader: ObjectId(1),
            members: vec![FellowEntry {
                id: ObjectId(1),
                max_health: 10,
                current_health: 5,
                max_stamina: 10,
                current_stamina: 5,
                max_mana: 10,
                current_mana: 5,
                ..FellowEntry::default()
            }],
            ..FellowshipView::default()
        })
    }
}

/// Every picture a frame names: its own, its overlay's, and its controls'.
fn pictures(frame: &PanelFrame) -> BTreeSet<u32> {
    let mut names: Vec<String> = frame
        .screen
        .commands
        .iter()
        .chain(&frame.overlay)
        .filter_map(|c| match c {
            crate::Command::Image { did, .. } => Some(did.clone()),
            _ => None,
        })
        .collect();
    for c in &frame.controls {
        names.extend(c.images.iter().flatten().cloned());
        names.extend(c.endcaps.iter().flatten().cloned());
    }
    names
        .iter()
        .filter_map(|n| n.split('+').next())
        .filter_map(|n| u32::from_str_radix(n, 16).ok())
        .filter(|id| id >> 24 == 6)
        .collect()
}

/// Every picture id the crate's source names outside its tests, by reading the source: the hex
/// forms `0x0600_1234` and `0x6001234`, and the quoted form `"06001234"`.
fn named_in_source() -> BTreeSet<u32> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("the source folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut ids = BTreeSet::new();
    for file in files {
        if file
            .file_name()
            .is_some_and(|n| n == "launch_art_tests.rs" || n == "tests.rs")
            || file.to_string_lossy().contains("_tests")
        {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("a source file");
        let text = text.split("#[cfg(test)]").next().unwrap_or("");
        for line in text.lines() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for (at, _) in line.match_indices("0x") {
                let digits: String = line[at + 2..]
                    .chars()
                    .take_while(|c| c.is_ascii_hexdigit() || *c == '_')
                    .filter(|c| *c != '_')
                    .collect();
                if digits.len() >= 7 && digits.len() <= 8 {
                    if let Ok(id) = u32::from_str_radix(&digits, 16) {
                        if id >> 24 == 6 {
                            ids.insert(id);
                        }
                    }
                }
            }
            for quoted in line.split('"').skip(1).step_by(2) {
                let name = quoted.split('+').next().unwrap_or("");
                if name.len() == 8 && name.starts_with("06") {
                    if let Ok(id) = u32::from_str_radix(name, 16) {
                        ids.insert(id);
                    }
                }
            }
        }
    }
    ids
}

/// Every picture in the frames of every panel a Launch world opens is there or made from what is.
#[test]
#[ignore = "reads the Launch fixture's portal: EMPYREAN_TEST_LAUNCH_FIXTURE"]
fn every_panel_a_launch_world_opens_draws_only_what_it_has() {
    let art = art();
    let features = EraId::Launch.features();
    let (p, k, s, classic) = Default::default();
    let resources = crate::resources::Resources::default();
    let mut blank = Vec::new();
    let mut seen = 0;
    for (meters, stack) in [
        ((Some(0.5), None), false),
        ((None, Some(0.5)), false),
        ((None, None), true),
    ] {
        let game = Launch {
            era: dereth_client_contract::EraView {
                era: EraId::Launch,
                era_announced: true,
                ..Default::default()
            },
            meters,
            stack,
        };
        let context = Context {
            resources: &resources,
            layout: crate::panels::Layout::default(),
            now: dereth_primitives::LocalTime(0.0),
            game: &game,
            pregame: &p,
            keyboard: &k,
            settings: &s,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let ids = PANELS
            .iter()
            .chain(crate::panels::pregame::IDS.iter())
            .filter(|id| !crate::panels::era_refuses(id, features));
        for id in ids {
            let Some(mut panel) = crate::panels::factory(id, context.now, &resources) else {
                continue;
            };
            panel.set_layout(context.layout);
            panel.resize(800, 600);
            seen += 1;
            for picture in pictures(&panel.frame(&context)) {
                if art.image(picture).is_none() {
                    blank.push(format!("{id}: {picture:08X}"));
                }
            }
        }
    }
    assert!(seen > 20, "only {seen} panels drawn");
    blank.dedup();
    assert!(blank.is_empty(), "{blank:#?}");
}

/// A picture being in the portal does not make it the right one: the keyed thumb on the stack
/// splitter has to show something over the dark bar past it, and its black has to be see-through.
#[test]
#[ignore = "reads the Launch fixture's portal: EMPYREAN_TEST_LAUNCH_FIXTURE"]
fn the_stack_splitters_thumb_stands_out_from_the_bar_it_runs_along() {
    let art = art();
    let brightness = |id: u32| {
        let image = art.image(id).expect("in the Launch portal");
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| u32::from(p[0].max(p[1]).max(p[2])))
            .max()
            .unwrap_or(0)
    };
    let (thumb, dark) = (brightness(0x0600_0F82), brightness(0x0600_1150));
    assert!(thumb > 2 * dark, "thumb {thumb}, dark bar {dark}");
}

/// Every picture id in the source is in the portal, is made from what is, or is listed with the
/// system Launch lacks that would draw it. A listing goes stale when the portal has the picture.
#[test]
#[ignore = "reads the Launch fixture's portal: EMPYREAN_TEST_LAUNCH_FIXTURE"]
fn every_picture_the_source_names_is_accounted_for_on_a_launch_world() {
    let art = art();
    let features = EraId::Launch.features();
    let mut listed: BTreeSet<u32> = NOT_DRAWN.into_iter().collect();
    for (system, has, ids) in SYSTEMS {
        assert!(!has(&features), "Launch has {system}");
        listed.extend(ids.iter().copied());
    }
    let named = named_in_source();
    let unaccounted: Vec<String> = named
        .iter()
        .filter(|id| art.image(**id).is_none() && !listed.contains(id))
        .map(|id| format!("{id:08X}"))
        .collect();
    assert!(unaccounted.is_empty(), "{unaccounted:#?}");
    let stale: Vec<String> = listed
        .iter()
        .filter(|id| art.image(**id).is_some() || !named.contains(id))
        .map(|id| format!("{id:08X}"))
        .collect();
    assert!(stale.is_empty(), "{stale:#?}");
    // Every stand-in is drawn from what the portal has.
    assert!(named
        .iter()
        .all(|id| { !crate::composed::is_stand_in(*id) || art.image(*id).is_some() }));
}
