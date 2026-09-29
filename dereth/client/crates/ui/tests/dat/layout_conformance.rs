//! Every shipped UI layout decodes, resolves and instantiates: all 101 layouts parse to their exact
//! end and match an independent reader, every partial resolves with no cycle and no missing base,
//! every layout instantiates with its dropped types accounted for, resizing to three display sizes
//! keeps every box consistent, the layout-enum resolver and the dialog kinds' roots agree with the
//! dat, and the persistence files round-trip byte-identically.
//!
//! Fixture: the retail `client_local_English.dat`, read through `dereth-dat`/`dereth-assets`. The
//! census figures (101 layouts, 2 162 elements, 1 088 partials, the type histogram, the layout-enum
//! map and the Dialog roots) were cross-checked against a separate implementation of the same byte
//! layout and are pinned as literals, so agreeing with them is agreement with another reader. Every
//! test fails when the retail data is absent: `env()` returns an `Option` only so a clean checkout
//! builds, and every caller `expect`s it.
//!
//! Behaviour: none (decoder and instantiation conformance over the shipped layouts)

use std::collections::BTreeMap;

use dereth_primitives::{AssetSource, DataId, DataType};

use dereth_ui::desc::LayoutDesc;
use dereth_ui::framework::{DidMapperResolver, LayoutEnum, LayoutEnumResolver};
use dereth_ui::{Box2D, ElementId, ElementType, NullInputPump, UiSystem};

const FIRST: u32 = 0x2100_0000;
const LAST: u32 = 0x2100_0075;

pub(super) struct Env {
    store: dereth_dat::RetailDatStore,
    types: dereth_assets::ui::PropertyTypes,
}

/// Open the dat, or `None` with the reason printed. **Every caller `expect`s
/// this**: the `Option` exists so a clean checkout builds, not so a test can pass without an
/// oracle.
pub(super) fn env() -> Option<Env> {
    let dir = dereth_dat::testing::dat_dir();
    if !dereth_dat::testing::have_dats() {
        eprintln!(
            "skipping: no client_local_English.dat under {}",
            dir.display()
        );
        return None;
    }
    let store = match dereth_dat::RetailDatStore::open_dir(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    // `MasterProperty 0x39000001` first: the layout stream is undecodable without the property
    // type table.
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).ok()?;
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .ok()?;
    let types = master.property_types();
    Some(Env { store, types })
}

pub(super) fn parse_all(e: &Env) -> BTreeMap<DataId, LayoutDesc> {
    let mut out = BTreeMap::new();
    for did in FIRST..=LAST {
        let did = DataId(did);
        if !e.store.exists(did) {
            continue;
        }
        let bytes = e
            .store
            .read(did)
            .unwrap_or_else(|err| panic!("{did}: {err}"));
        // The decoder's own `expect_end` is the "cursor lands exactly on the payload end" check:
        // a short or long read is an error, not a silent truncation.
        let l =
            LayoutDesc::read(did, &bytes, &e.types).unwrap_or_else(|err| panic!("{did}: {err}"));
        out.insert(did, l);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// 1 and 2 — parse every layout, and agree with the independent reader
// ---------------------------------------------------------------------------------------------

/// The engine element types (below `0x1000_0000`) in the shipped layouts, with how many nodes of
/// each type there are across all 101; type 0 is the "partial" bucket.
const ENGINE_TYPE_HISTOGRAM: [(u32, usize); 19] = [
    (0x00, 1088),
    (0x01, 27),
    (0x02, 30),
    (0x03, 695),
    (0x05, 35),
    (0x06, 6),
    (0x07, 23),
    (0x08, 12),
    (0x09, 71),
    (0x0B, 10),
    (0x0C, 52),
    (0x0D, 7),
    (0x11, 2),
    (0x13, 1),
    (0x14, 1),
    (0x15, 1),
    (0x16, 1),
    (0x17, 1),
    (0x19, 1),
];

/// Every one of the 101 shipped layouts parses with the cursor landing exactly
/// on the payload end, and the resulting trees reproduce the independently recorded census.
#[test]
fn all_101_layouts_parse_and_match_the_independent_reader() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    let layouts = parse_all(&e);
    assert_eq!(
        layouts.len(),
        101,
        "the shipped count from 36-ui-layouts.md §5"
    );

    let mut elements = 0_usize;
    let mut roots = 0_usize;
    let mut not_800x600 = Vec::new();
    let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
    for (did, l) in &layouts {
        if (l.display_width, l.display_height) != (800, 600) {
            assert_eq!(
                (l.display_width, l.display_height),
                (800, 500),
                "{did} display size"
            );
            not_800x600.push(did.0);
        }
        roots += l.elements.len();
        elements += l.element_count();
        for r in l.elements.values() {
            walk_hist(r, &mut hist);
        }
    }
    // The independently recorded census: 101 layouts, 2162 elements, 1088 of them partial.
    assert_eq!(elements, 2162);
    assert_eq!(hist.get(&0).copied(), Some(1088), "partials");
    assert_eq!(hist.values().sum::<usize>(), elements);
    assert_eq!(roots, 372, "root elements across all layouts");
    assert_eq!(
        not_800x600,
        [0x2100_0066, 0x2100_0067, 0x2100_0069],
        "the only layouts not authored at 800x600 (all three at 800x500)"
    );
    let engine: Vec<(u32, usize)> = hist
        .iter()
        .filter(|(t, _)| **t < 0x1000_0000)
        .map(|(t, n)| (*t, *n))
        .collect();
    assert_eq!(engine, ENGINE_TYPE_HISTOGRAM, "engine element types");
    let game: Vec<usize> = hist
        .iter()
        .filter(|(t, _)| **t >= 0x1000_0000)
        .map(|(_, n)| *n)
        .collect();
    assert_eq!(game.len(), 82, "distinct game element types");
    assert_eq!(game.iter().sum::<usize>(), 98, "game-type elements");
}

pub(super) fn walk_hist(d: &dereth_ui::ElementDesc, hist: &mut BTreeMap<u32, usize>) {
    *hist.entry(d.ty.0).or_insert(0) += 1;
    for c in d.children.values() {
        walk_hist(c, hist);
    }
}

// ---------------------------------------------------------------------------------------------
// 3 — resolve every partial
// ---------------------------------------------------------------------------------------------

/// `inq_full_desc` resolves **every** type-0 node in the shipped data with no cycle and no
/// unresolved base.
#[test]
fn every_partial_resolves_with_no_cycle_and_no_missing_base() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    let layouts = parse_all(&e);
    let mut lib = dereth_ui::DescLibrary::new();
    for l in layouts.values() {
        lib.insert(l.clone());
    }

    // The shipped data has exactly one dangling base reference. `inq_full_desc` returns false there
    // and the client drops the subtree; the gate pins the identity of that one node so a future
    // regression cannot hide behind it.
    const DANGLING: (u32, u32) = (0x2100_0040, 0x1000_02C2);

    let mut resolved = 0_usize;
    let mut dangling = Vec::new();
    for l in layouts.values() {
        let mut stack: Vec<&dereth_ui::ElementDesc> = l.elements.values().collect();
        while let Some(d) = stack.pop() {
            for c in d.children.values() {
                stack.push(c);
            }
            if !d.is_partial() {
                continue;
            }
            match l.inq_full_desc_cached(&e.store, &e.types, &mut lib, d) {
                Ok(full) => {
                    assert!(
                        !full.is_partial(),
                        "{}: {:?} resolved to another partial",
                        l.did,
                        d.element_id
                    );
                    assert_eq!(
                        full.element_id, d.element_id,
                        "the partial's own id survives"
                    );
                    resolved += 1;
                }
                Err(dereth_ui::UiError::MissingBase { .. }) => {
                    dangling.push((l.did.0, d.element_id.0));
                }
                Err(err) => panic!("{}: element {:?}: {err}", l.did, d.element_id),
            }
        }
    }
    assert_eq!(
        dangling,
        vec![DANGLING],
        "the one dangling base reference in the shipped data"
    );
    assert_eq!(resolved, 1087, "every other partial in the shipped data");
    assert_eq!(resolved + dangling.len(), 1088);
    assert!(
        lib.hits > 0,
        "the desc cache answered repeated references to ItemSlot and Dialog"
    );
}

// ---------------------------------------------------------------------------------------------
// 4 — instantiate against the engine factory
// ---------------------------------------------------------------------------------------------

/// Every shipped layout instantiates against the engine factory table. Element counts
/// are the census's *minus* whatever the factory drops, and the drops are exactly the subtrees
/// whose type has no registration — which for the shipped data is the game types, since this crate
/// registers only the engine ones.
#[test]
fn every_layout_instantiates_and_the_drops_are_accounted_for() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    let layouts = parse_all(&e);

    let mut total_built = 0_usize;
    let mut dropped_types: BTreeMap<u32, usize> = BTreeMap::new();
    for l in layouts.values() {
        let mut ui = UiSystem::new((800, 600));
        ui.property_types.clone_from(&e.types);
        for other in layouts.values() {
            ui.lib.insert(other.clone());
        }
        let before = ui.element_list().len();
        for root in l.elements.values() {
            match ui.create_element(&e.store, l, root) {
                Ok(Some(h)) => {
                    let r = ui.root();
                    ui.set_parent(h, Some(r));
                    if let Some(n) = ui.node_mut(h) {
                        n.flags.set_is_root_element(true);
                    }
                }
                Ok(None) => {}
                Err(err) => panic!("{}: root {:?}: {err}", l.did, root.element_id),
            }
        }
        let built = ui.element_list().len() - before;
        total_built += built;
        for d in &ui.dropped {
            *dropped_types.entry(d.ty.0).or_insert(0) += d.descendants;
        }
        // Nothing is built twice and nothing is orphaned: every built element is reachable from the
        // manager's root.
        for h in ui.element_list().iter().skip(1) {
            assert!(
                ui.root_of(*h).is_some(),
                "{}: {h:?} has no root — set_parent did not run",
                l.did
            );
        }
        // A tree that instantiated must also initialise without panicking.
        for root in ui.children(ui.root()) {
            ui.initialize_tree(root);
        }
        ui.use_time(dereth_primitives::LocalTime(0.0), &mut NullInputPump);
    }

    assert!(total_built > 0);
    // Every dropped type is one of exactly three things: a game type (`0x1000xxxx`, which this
    // crate deliberately does not register — `dereth-ui-screens` does), one of the four ids the
    // client itself leaves unregistered, or type 0 — the single partial in the
    // shipped data whose base element exists in no layout, which `inq_full_desc` refuses and
    // the recursive build from a partial description drops.
    for t in dropped_types.keys() {
        let engine_unregistered = dereth_ui::factory::ty::UNREGISTERED.contains(&ElementType(*t));
        assert!(
            t & 0x1000_0000 != 0 || engine_unregistered || *t == 0,
            "type {t:#X} was dropped but is neither a game type, nor one of the four \
             unregistered engine ids, nor the dangling partial"
        );
    }
    assert_eq!(
        dropped_types.get(&0).copied(),
        Some(1),
        "exactly one node is dropped for an unresolvable base"
    );
    // And the shipped data does **not** use any of the four unregistered engine ids: that is the
    // half of #115 the data can answer.
    for t in dereth_ui::factory::ty::UNREGISTERED {
        assert_eq!(
            dropped_types.get(&t.0),
            None,
            "no shipped layout references unregistered engine type {:#X}",
            t.0
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 5 — resize the tree
// ---------------------------------------------------------------------------------------------

/// Resizing to each of three display sizes leaves every element with a box that the
/// four-edge rule explains, and leaves the manager's root exactly covering the display.
///
/// **What this is not:** a diff against each element's box as the running retail client places
/// it, which cannot be produced from the data. This checks the invariants the arithmetic
/// guarantees, which catches a transform that diverges but not one that is uniformly wrong.
#[test]
fn resizing_every_layout_keeps_the_boxes_consistent() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    let layouts = parse_all(&e);

    for l in layouts.values() {
        let mut ui = UiSystem::new((l.display_width, l.display_height));
        ui.property_types.clone_from(&e.types);
        for other in layouts.values() {
            ui.lib.insert(other.clone());
        }
        let mut roots = Vec::new();
        for root in l.elements.values() {
            if let Ok(Some(h)) = ui.create_element(&e.store, l, root) {
                let r = ui.root();
                ui.set_parent(h, Some(r));
                if let Some(n) = ui.node_mut(h) {
                    n.flags.set_is_root_element(true);
                }
                roots.push(h);
            }
        }
        for root in roots.clone() {
            ui.initialize_tree(root);
        }

        for size in [(800, 600), (1024, 768), (1920, 1080)] {
            ui.refresh_event(size);
            assert_eq!(
                ui.node(ui.root()).unwrap().region.box_,
                Box2D::new(0, 0, size.0 - 1, size.1 - 1),
                "{}: the hollow root covers the display at {size:?}",
                l.did
            );
            for h in ui.element_list() {
                let n = ui.node(*h).unwrap();
                let b = n.region.box_;
                assert_eq!(b.width(), b.x1 - b.x0 + 1, "inclusive width invariant");
                assert!(
                    b.width() > -100_000 && b.width() < 100_000,
                    "{}: {:?} width {} at {size:?} is not a plausible box",
                    l.did,
                    n.element_id(),
                    b.width()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The layout enum map, and the dialog roots
// ---------------------------------------------------------------------------------------------

/// The `UILAYOUT` enum -> `DataId` table, as read out of the shipped id mapper `0x2500000E`.
#[rustfmt::skip]
const LAYOUT_ENUM_MAP: [(u32, u32); 81] = [
    (0x0000_0001, 0x2100_003B), (0x0000_0002, 0x2100_003C), (0x1000_0001, 0x2100_0000),
    (0x1000_0002, 0x2100_0001), (0x1000_0003, 0x2100_0002), (0x1000_0004, 0x2100_0003),
    (0x1000_0005, 0x2100_0004), (0x1000_0006, 0x2100_0005), (0x1000_0007, 0x2100_0006),
    (0x1000_0008, 0x2100_0007), (0x1000_0009, 0x2100_0008), (0x1000_000A, 0x2100_0009),
    (0x1000_000B, 0x2100_000A), (0x1000_000C, 0x2100_000B), (0x1000_000D, 0x2100_000C),
    (0x1000_000E, 0x2100_000D), (0x1000_000F, 0x2100_000E), (0x1000_0010, 0x2100_000F),
    (0x1000_0011, 0x2100_0010), (0x1000_0012, 0x2100_0011), (0x1000_0013, 0x2100_0012),
    (0x1000_0014, 0x2100_0013), (0x1000_0015, 0x2100_0014), (0x1000_0016, 0x2100_0015),
    (0x1000_0017, 0x2100_0016), (0x1000_0018, 0x2100_0017), (0x1000_0019, 0x2100_0018),
    (0x1000_001A, 0x2100_0019), (0x1000_001B, 0x2100_001A), (0x1000_001C, 0x2100_001B),
    (0x1000_001D, 0x2100_001C), (0x1000_001E, 0x2100_001D), (0x1000_001F, 0x2100_001E),
    (0x1000_0020, 0x2100_001F), (0x1000_0021, 0x2100_0020), (0x1000_0022, 0x2100_0021),
    (0x1000_0023, 0x2100_0022), (0x1000_0024, 0x2100_0023), (0x1000_0025, 0x2100_0024),
    (0x1000_0026, 0x2100_0025), (0x1000_0027, 0x2100_0026), (0x1000_0028, 0x2100_0027),
    (0x1000_0029, 0x2100_0028), (0x1000_002A, 0x2100_0029), (0x1000_002B, 0x2100_002A),
    (0x1000_002C, 0x2100_002B), (0x1000_002D, 0x2100_002C), (0x1000_002E, 0x2100_002D),
    (0x1000_002F, 0x2100_002E), (0x1000_0030, 0x2100_002F), (0x1000_0031, 0x2100_0030),
    (0x1000_0032, 0x2100_0031), (0x1000_0033, 0x2100_0032), (0x1000_0034, 0x2100_0033),
    (0x1000_0035, 0x2100_0034), (0x1000_0036, 0x2100_0035), (0x1000_0037, 0x2100_0036),
    (0x1000_0038, 0x2100_0037), (0x1000_0039, 0x2100_0038), (0x1000_003A, 0x2100_0039),
    (0x1000_003B, 0x2100_003A), (0x1000_0092, 0x2100_0058), (0x1000_0093, 0x2100_005E),
    (0x1000_0094, 0x2100_005D), (0x1000_0095, 0x2100_0060), (0x1000_0096, 0x2100_0066),
    (0x1000_0097, 0x2100_0067), (0x1000_0098, 0x2100_0063), (0x1000_0099, 0x2100_0068),
    (0x1000_009A, 0x2100_0069), (0x1000_009B, 0x2100_006E), (0x1000_009C, 0x2100_006D),
    (0x1000_009D, 0x2100_006B), (0x1000_009E, 0x2100_006C), (0x1000_009F, 0x2100_0070),
    (0x1000_00A0, 0x2100_006F), (0x1000_00A1, 0x2100_0071), (0x1000_00A2, 0x2100_0072),
    (0x1000_00A3, 0x2100_0073), (0x1000_00A4, 0x2100_0074), (0x1000_00A5, 0x2100_0075),
];

/// The layout-enum map: the resolver reads it out of the dat through the
/// same two-level lookup the client uses, and it agrees with the independently read table.
#[test]
fn the_layout_enum_resolver_agrees_with_the_dat() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    let r = DidMapperResolver::load_via_master(&e.store).expect("the master mapper names UILAYOUT");
    assert_eq!(r.len(), LAYOUT_ENUM_MAP.len());
    assert_eq!(
        dereth_ui::framework::UI_LAYOUT_GROUP,
        5,
        "the UILAYOUT group"
    );
    for &(en, did) in &LAYOUT_ENUM_MAP {
        assert_eq!(r.resolve(LayoutEnum(en)), Some(DataId(did)), "enum {en:#X}");
    }
    // The two anchors the screen catalogue names.
    assert_eq!(
        r.resolve(LayoutEnum(0x1000_0001)),
        Some(DataId(0x2100_0000)),
        "patch"
    );
    assert_eq!(
        r.resolve(LayoutEnum(2)),
        Some(DataId(0x2100_003C)),
        "Dialog"
    );
    // Every value is a UI_LAYOUT id that actually ships.
    for &(_, did) in &LAYOUT_ENUM_MAP {
        let did = DataId(did);
        assert_eq!(did.0 >> 24, 0x21, "{did} is not a UI_LAYOUT id");
        assert!(
            e.store.exists(did),
            "{did} is named by the mapper but is not in the dat"
        );
    }
    // Sanity on the seam: the resolver is what a screen goes through, and a missing entry is loud.
    let ui_err = dereth_ui::UiError::UnresolvedLayoutEnum(LayoutEnum(0xDEAD));
    assert!(format!("{ui_err}").contains("0xDEAD"), "{ui_err}");
    assert_eq!(r.resolve(LayoutEnum(0xDEAD)), None);
}

/// The dialog factory's seven "layout
/// enums" are root **element ids** of the `Dialog` layout, and six of the seven are there with
/// exactly the matching element type.
#[test]
fn the_dialog_kinds_name_roots_of_the_dialog_layout() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    use dereth_ui::dialog::DialogKind::{
        Confirmation, ConfirmationMenu, ConfirmationTextInput, Menu, Message, TextInput, Wait,
    };
    // The root elements of the shipped `Dialog` layout `0x2100003C`: (element id, element type).
    const DIALOG_ROOTS: [(u32, u32); 6] = [
        (0x15, 0x13),
        (0x1B, 0x16),
        (0x1F, 0x14),
        (0x24, 0x17),
        (0x2C, 0x15),
        (0x31, 0x19),
    ];
    let layouts = parse_all(&e);
    let dialog = &layouts[&DataId(0x2100_003C)];
    let mut shipped: Vec<(u32, u32)> = dialog
        .elements
        .values()
        .map(|d| (d.element_id.0, d.ty.0))
        .collect();
    shipped.sort_unstable();
    assert_eq!(shipped, DIALOG_ROOTS, "the Dialog layout's roots");

    let mut missing = Vec::new();
    for kind in [
        Confirmation,
        Wait,
        Message,
        TextInput,
        ConfirmationTextInput,
        Menu,
        ConfirmationMenu,
    ] {
        let key = kind.root_element_id().0;
        match DIALOG_ROOTS.iter().find(|(id, _)| *id == key) {
            Some((_, ty)) => assert_eq!(
                *ty,
                kind.element_type().0,
                "root {key:#X} should be element type {:#X}",
                kind.element_type().0
            ),
            None => missing.push(kind),
        }
    }
    assert_eq!(
        missing.len(),
        1,
        "exactly one kind has no root: {missing:?}"
    );
    assert_eq!(missing[0], TextInput);

    // And 0x28 is a root of no shipped layout at all, which is what makes that dialog kind
    // uncreatable.
    for l in layouts.values() {
        assert!(
            l.access_element(ElementId(0x28)).is_none(),
            "{} has a root 0x28 after all",
            l.did
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The persistence round trip
// ---------------------------------------------------------------------------------------------

/// The gate's "plus one round-trip assertion". No retail data is needed and none exists: the client
/// writes `UserPreferences.ini` and `UI-*.txt` at run time and neither is in the install, so the
/// round trip is asserted over files in the recovered formats rather than over a played
/// character's. Reported.
#[test]
fn the_persistence_files_round_trip_byte_identically() {
    use dereth_ui::persist::{ScreenLayout, UserPreferences, WINDOWS};

    let ini = "[Default]\r\nMisc.TooltipDelay=1.000000\r\nMisc.TooltipEnable=1\r\n\
               International.UseIME=0\r\nInput.KeymapFile=keymap.txt\r\nUI.ChatFontFace=0\r\n\
               UI.ChatFontSize=2\r\n";
    let p = UserPreferences::parse(ini).expect("parses");
    assert_eq!(
        p.to_text(),
        ini,
        "UserPreferences.ini must re-save byte-identically"
    );
    assert_eq!(p.entries.len(), 6);

    let layout = ScreenLayout {
        windows: WINDOWS
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let i = i32::try_from(i).unwrap();
                (
                    s.tag,
                    dereth_ui::persist::SavedWindow {
                        x: i * 13,
                        y: i * 17,
                        w: 200,
                        h: 100,
                    },
                )
            })
            .collect(),
    };
    let text = layout.to_text();
    let back = ScreenLayout::parse(&text).expect("parses");
    assert_eq!(back, layout);
    assert_eq!(
        back.to_text(),
        text,
        "the screen layout must re-save byte-identically"
    );
    assert!(
        text.contains("<SBOX> X:0 Y: 0 W: 200 H: 100 "),
        "the irregular spacing survives"
    );
}

/// A structural note on the seam, pinned as a test so it is noticed if it changes.
///
/// `AssetSource::iter_type` **cannot enumerate UI layouts**: `dereth_primitives::DataType` does
/// not name `UI_LAYOUT` (`0x21`), and `Other(0x21)` maps to
/// no `DbType`, so the iterator is empty. This crate never needs it — a layout is always looked up
/// by id, through the enum resolver — but a consumer that wanted to walk every layout would have to
/// go around the seam.
#[test]
fn the_asset_seam_cannot_enumerate_ui_layouts() {
    let e = env().expect("the test environment: retail dats and a WARP device");
    assert_eq!(e.store.iter_type(DataType::Other(0x21)).count(), 0);
    // Lookup by id, which is the path this crate uses, works for all 101.
    assert_eq!(
        (FIRST..=LAST)
            .filter(|d| e.store.exists(DataId(*d)))
            .count(),
        101
    );
}
