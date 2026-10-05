//! Level-designer markers are never drawn: a graphics object whose degrade record is itself at a
//! zero-width band followed by the empty terminator draws nothing at any distance, so the red cone
//! placed in the Holtburg training academy's library (cell `0x860201AD`, setup `0x02000C39`, part
//! `0x010028CA`) never appears. These are the dat-side halves of that claim: which graphics objects
//! the guard refuses, which placements in the library and in the whole cell dat they are, and where
//! the marker's colours come from. The rendered frame of the library with the guard on and off is
//! the `gpu` tier's `rendering::degrade_marker_parts`.
//!
//! Fixture: `client_cell_1.dat` and `client_portal.dat` from `$DERETH_TEST_DAT_DIR`, read through
//! the asset decoders. Every fixture is an `expect`, so a missing input fails rather than skips.

use std::collections::BTreeSet;

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo, Surface};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_scene::textures::TextureStore;
use {
    dereth_client_runtime::models::draws_at_near_band, dereth_client_runtime::models::resolve_parts,
};
use {dereth_world_data::env_cells::cell_statics, dereth_world_data::env_cells::EnvCellLoader};

/// The first Holtburg starter area's first instantiation cell's landblock -- the block of the
/// room a new Holtburg character wakes up in, and the room the marker stands in.
const TRAINING_DUNGEON: u16 = 0x8602;
const LIBRARY: u32 = 0x8602_01AD;

/// The library's marker placement: a setup whose one part is the cone.
const MARKER_SETUP: u32 = 0x0200_0C39;
const MARKER_GFXOBJ: u32 = 0x0100_28CA;

/// Every graphics object in `client_portal.dat` whose degrade record selects a level with no
/// geometry at the near band. Read out of the dat, not chosen: the first test re-derives this list
/// from the whole portal dat and fails if it moves.
const MARKERS: [u32; 11] = [
    0x0100_01EC,
    0x0100_08A8,
    0x0100_27BB,
    0x0100_28C6,
    0x0100_28C7,
    0x0100_28CA,
    0x0100_28CB,
    0x0100_28CD,
    0x0100_2A65,
    0x0100_2C11,
    0x0100_4E0B,
];

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

/// Oracle: every graphics object in `client_portal.dat`.
///
/// Eleven of the portal dat's graphics objects (about 15,000, over 4,000 of which carry a degrade
/// record at all) are never drawn, and every one of them carries the *same* two-level record:
/// itself at `min = ideal = max = 0.0`, then the terminator at the largest float naming no
/// geometry. That shape is what distinguishes them from an LOD chain, and it is asserted here so
/// that a change to the predicate cannot quietly widen or narrow the set.
#[test]
fn the_dat_names_eleven_markers_the_client_never_draws() {
    let store = store();
    let ids = store.ids_of(DbType::GfxObj);
    assert!(
        ids.len() > 15_000,
        "only {} graphics-object records in the portal dat",
        ids.len()
    );

    let mut with_degrade = 0usize;
    let mut never: BTreeSet<u32> = BTreeSet::new();
    for id in &ids {
        let bytes = store
            .read_typed(DbType::GfxObj, *id)
            .expect("a graphics-object record the dat lists");
        let obj = GfxObj::decode_payload(*id, &bytes).expect("the graphics-object record decodes");
        if obj.did_degrade.is_some() {
            with_degrade += 1;
        }
        if !draws_at_near_band(&store, *id) {
            never.insert(id.0);
        }
    }
    eprintln!(
        "portal dat: {} graphics objects, {with_degrade} with a GfxObjDegradeInfo, {} never drawn",
        ids.len(),
        never.len()
    );
    assert!(
        with_degrade > 4_000,
        "only {with_degrade} degrade records; the corpus is not the retail one"
    );
    assert_eq!(
        never,
        MARKERS.into_iter().collect::<BTreeSet<u32>>(),
        "the set of never-drawn graphics objects is not the eleven the dat names"
    );

    // Every one of them is the same record, and it is not an LOD chain.
    for id in MARKERS {
        let d = DataId(id);
        let obj = GfxObj::decode_payload(d, &store.read_typed(DbType::GfxObj, d).expect("read"))
            .expect("decode");
        let did = obj.did_degrade.expect("a marker names a degrade record");
        let info = GfxObjDegradeInfo::decode_payload(
            did,
            &store.read_typed(DbType::DegradeInfo, did).expect("read"),
        )
        .expect("decode");
        assert_eq!(info.degrades.len(), 2, "{id:#010X}: not a two-level record");
        let (near, term) = (&info.degrades[0], &info.degrades[1]);
        assert_eq!(
            near.gfxobj_id, d,
            "{id:#010X}: level 0 does not name the object itself"
        );
        assert_eq!(
            (near.min_dist, near.ideal_dist, near.max_dist),
            (0.0, 0.0, 0.0),
            "{id:#010X}: level 0 has a band, so it is an LOD chain and not a marker"
        );
        assert_eq!(
            term.gfxobj_id,
            DataId(0),
            "{id:#010X}: the terminator names geometry"
        );
        assert_eq!(
            term.ideal_dist,
            f32::MAX,
            "{id:#010X}: the terminator is not at the largest float"
        );
    }
}

/// Oracle: `client_cell_1.dat`'s interior cell `0x860201AD` and the setup, graphics-object,
/// surface, texture and palette records it reaches.
///
/// Which object stands there, that it is the one the cell names, and where its colours come
/// from: all three are the dat's own, so hiding it is the draw guard's job and not the
/// placement's.
#[test]
fn the_library_places_the_marker_and_its_colours_are_the_dats_own() {
    let store = store();
    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    assert_eq!(
        loader.stats.undecodable, 0,
        "an interior cell of the training dungeon would not decode"
    );
    let library = cells
        .iter()
        .find(|d| d.id.0 == LIBRARY)
        .expect("the library cell is in the block");

    let statics = cell_statics(library);
    assert_eq!(
        statics.len(),
        30,
        "the library's static list is not the retail one"
    );
    let marker: Vec<_> = statics.iter().filter(|s| s.id.0 == MARKER_SETUP).collect();
    assert_eq!(
        marker.len(),
        1,
        "the library does not place exactly one {MARKER_SETUP:#010X}"
    );
    let at = marker[0].frame.origin;
    assert!(
        (at.x - 12.3199).abs() < 1.0e-3 && (at.y + 28.482).abs() < 1.0e-3 && at.z.abs() < 0.01,
        "the marker is placed at {at:?}, not where the cell puts it"
    );

    // The parts are the setup's own, and exactly one of the 30 placements is refused.
    let parts = resolve_parts(&store, DataId(MARKER_SETUP));
    assert_eq!(parts.len(), 1, "the marker setup is not one part");
    assert_eq!(
        parts[0].gfxobj,
        DataId(MARKER_GFXOBJ),
        "the marker resolves to another graphics-object record"
    );
    // Two of the thirty are markers: the cone (`0x02000C39`) and, on the floor beneath it,
    // `0x02000943`, whose part `0x010001EC` is a 5 cm triangle at translucency 1.0. The other
    // 28 draw.
    let refused: BTreeSet<u32> = statics
        .iter()
        .filter(|s| {
            resolve_parts(&store, s.id)
                .iter()
                .any(|p| !draws_at_near_band(&store, p.gfxobj))
        })
        .map(|s| s.id.0)
        .collect();
    assert_eq!(
        refused,
        [0x0200_0943, MARKER_SETUP]
            .into_iter()
            .collect::<BTreeSet<u32>>(),
        "the guard refuses something other than the library's two markers"
    );

    // Where the red and the green come from: the surfaces are *textured* (`type & 6`), and the
    // textures are 16-bit indexed ones whose own default palettes are a red ramp and a green
    // ramp, so a drawn marker shows exactly these colours.
    let obj = GfxObj::decode_payload(
        DataId(MARKER_GFXOBJ),
        &store
            .read_typed(DbType::GfxObj, DataId(MARKER_GFXOBJ))
            .expect("read"),
    )
    .expect("decode");
    assert_eq!(
        obj.surfaces.len(),
        2,
        "the marker does not carry two surfaces"
    );
    let textures = TextureStore::new(&store);
    let mut means = Vec::new();
    for s in &obj.surfaces {
        let sf = Surface::decode_payload(*s, &store.read_typed(DbType::Surface, *s).expect("read"))
            .expect("decode");
        assert!(
            sf.surface_type & 6 != 0,
            "surface {:#010X} is a flat colour, not a texture",
            s.0
        );
        assert_eq!(
            sf.color_value, None,
            "surface {:#010X} carries a colour word",
            s.0
        );
        let tex = sf
            .orig_texture_id
            .expect("a textured surface record names a texture");
        let data = textures.texture_data(tex).expect("the texture decodes");
        let px = &data.levels[0];
        let n = (px.len() / 4) as u64;
        let (mut b, mut g, mut r) = (0u64, 0u64, 0u64);
        for c in px.as_chunks::<4>().0 {
            b += u64::from(c[0]);
            g += u64::from(c[1]);
            r += u64::from(c[2]);
        }
        means.push((r / n, g / n, b / n));
    }
    eprintln!("marker surface means (r, g, b): {means:?}");
    let (r0, g0, b0) = means[0];
    assert!(
        r0 > 180 && g0 < 40 && b0 < 40,
        "the first marker surface is not red: {:?}",
        means[0]
    );
    let (r1, g1, b1) = means[1];
    assert!(
        g1 > 120 && r1 < 60 && b1 < 60,
        "the second marker surface is not green: {:?}",
        means[1]
    );
}

/// Behaviour: rendering.markers.marker-placements-in-the-dat-are-never-drawn
/// Oracle: `client_cell_1.dat`, every landblock that has static placements.
///
/// What the guard hides over the whole cell dat. A guard that hides an object is
/// indistinguishable from an object that was never there, so the count of what it hides is part
/// of the claim: 152,966 of the cell dat's 593,927 static placements, in 2,130
/// landblocks, are placements of these eleven markers. This test is slow by design -- it reads the
/// whole cell dat -- and it is the only place the workspace-wide number is checked.
#[test]
fn the_guard_hides_exactly_the_marker_placements_of_the_whole_cell_dat() {
    let store = store();
    let mut loader = EnvCellLoader::new();
    let markers: BTreeSet<u32> = MARKERS.into_iter().collect();
    let (mut placements, mut hidden) = (0u64, 0u64);
    let mut blocks: BTreeSet<u16> = BTreeSet::new();
    for b in 0..=0xFFFFu32 {
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: the loop bound is 0xFFFF.
        let block = b as u16;
        for d in &loader.load_block(&store, block) {
            for s in cell_statics(d) {
                placements += 1;
                let parts = resolve_parts(&store, s.id);
                if parts.is_empty() {
                    continue;
                }
                if parts.iter().all(|p| !draws_at_near_band(&store, p.gfxobj)) {
                    hidden += 1;
                    blocks.insert(block);
                    assert!(
                        parts.iter().all(|p| markers.contains(&p.gfxobj.0)),
                        "{:#010X} is hidden but is not one of the eleven markers",
                        s.id.0
                    );
                }
            }
        }
    }
    eprintln!(
        "cell dat: {placements} static placements, {hidden} of them markers, in {} landblocks",
        blocks.len()
    );
    assert_eq!(placements, 593_927, "the cell dat is not the retail one");
    assert_eq!(
        hidden, 152_966,
        "the guard hides a different number of placements than measured"
    );
    assert_eq!(
        blocks.len(),
        2_130,
        "the markers are in a different number of landblocks"
    );
}
