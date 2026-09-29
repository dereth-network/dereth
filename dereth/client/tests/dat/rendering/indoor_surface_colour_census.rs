//! No authored surface can paint the flat grey RGB(208,208,208) (colour word `D0D0D0`) seen in an
//! indoor frame: no surface in `client_portal.dat` carries that colour word (the two that carry
//! `C8C8C8` are the academy's filler surfaces `080000DD` and `08000139`), none of the surfaces the
//! three academy blocks' cell polygons name can paint it, and no academy cell portal leads outdoors.
//! The grey is the sky pass, which the gpu tier's indoor/outdoor pass tests cover. The academy's
//! untextured cell faces are all portal-linked, which is what the environment mesh-draw guard for
//! untextured surfaces relies on.
//! Fixture: the retail dats (a missing install fails).
//!
//! Behaviour: none (a census of the shipped surfaces and cells, not a client behaviour).

use dereth_assets::{Decode, Surface};
use dereth_client::env_cells::EnvCellLoader;
use dereth_client::textures::TextureStore;
use dereth_dat::DbType;
use std::collections::BTreeMap;

/// The indoor grey, as a colour word and as a BGRA texel.
const D0: u32 = 0x00D0_D0D0;

fn is_d0(px: &[u8; 4]) -> bool {
    px[0] == 208 && px[1] == 208 && px[2] == 208
}

/// No surface anywhere in the portal dat is the indoor grey, as a solid colour or as a
/// texture that is mostly one.
#[test]
fn no_authored_surface_is_the_indoor_grey() {
    let store = dereth_dat::testing::open_store_or_fail();
    let textures = TextureStore::new(&store);
    let ids = store.ids_of(DbType::Surface);
    assert!(
        ids.len() > 1000,
        "only {} surfaces decoded -- the dat did not open",
        ids.len()
    );

    let (mut solid_d0, mut uniform_d0, mut mostly_d0, mut contains_d0) = (0u32, 0u32, 0u32, 0u32);
    let mut c8 = Vec::new();
    for id in &ids {
        let Ok(bytes) = store.read_typed(DbType::Surface, *id) else {
            continue;
        };
        let Ok(s) = Surface::decode_payload(*id, &bytes) else {
            continue;
        };
        if let Some(c) = s.color_value {
            if c & 0x00FF_FFFF == D0 {
                solid_d0 += 1;
            }
            if c & 0x00FF_FFFF == 0x00C8_C8C8 {
                c8.push(*id);
            }
        }
        let Ok(img) = textures.bgra8(*id) else {
            continue;
        };
        let n = img.pixels.iter().filter(|p| is_d0(p)).count();
        if n == 0 {
            continue;
        }
        contains_d0 += 1;
        if n == img.pixels.len() {
            uniform_d0 += 1;
        } else if n * 2 > img.pixels.len() {
            mostly_d0 += 1;
        }
    }
    eprintln!(
        "surface census of {} surfaces: solid D0D0D0 {solid_d0}; uniformly D0D0D0 {uniform_d0}; \
         majority D0D0D0 {mostly_d0}; containing any D0D0D0 texel {contains_d0}; \
         solid C8C8C8 {c8:?}",
        ids.len()
    );
    // Non-vacuity in both directions: the grey the old reading named *is* in the dat, and the
    // indoor grey is not.
    assert_eq!(
        c8.len(),
        2,
        "the two C8C8C8 academy fillers are the census's calibration"
    );
    assert_eq!(
        solid_d0, 0,
        "a surface record does carry the indoor grey's colour word after all"
    );
    assert_eq!(
        uniform_d0, 0,
        "a texture is uniformly the indoor grey after all"
    );
    assert_eq!(
        mostly_d0, 0,
        "a texture is mostly the indoor grey after all"
    );
    // Scattered texels exist; a flat 21,591-pixel region is not made of them, and this number is
    // printed rather than asserted at a value so a later dat does not fail the test spuriously.
    assert!(
        contains_d0 > 0,
        "no texture holds the colour at all -- the sampler is wrong"
    );
}

/// Every surface the three complete academy blocks' **cell polygons** name, with what it could
/// put on screen. None of them can paint RGB(208,208,208).
#[test]
fn no_academy_cell_surface_can_paint_the_indoor_grey() {
    let store = dereth_dat::testing::open_store_or_fail();
    let textures = TextureStore::new(&store);
    let mut loader = EnvCellLoader::new();
    let mut faces: BTreeMap<u32, u32> = BTreeMap::new();
    for block in [0x7f03u16, 0x8602, 0x8c04] {
        let cells = loader.load_block(&store, block);
        assert_eq!(
            cells.len(),
            568,
            "complete academy cell set for {block:04x}"
        );
        for cell in cells {
            for p in &cell.structure.polygons {
                if p.num_pts < 3 {
                    continue;
                }
                if let Some(&sid) = cell.cell.surfaces.get(usize::from(p.pos_surface)) {
                    *faces.entry(sid.0).or_insert(0) += 1;
                }
            }
        }
    }
    assert_eq!(loader.stats.missing, 0);
    assert_eq!(loader.stats.undecodable, 0);
    assert_eq!(loader.stats.no_environment, 0);
    assert!(
        faces.len() > 20,
        "only {} distinct cell surfaces -- the census is thin",
        faces.len()
    );

    let mut culprits = Vec::new();
    let mut untextured = 0u32;
    for (id, n) in &faces {
        let did = dereth_primitives::DataId(*id);
        let s = store
            .read_typed(DbType::Surface, did)
            .ok()
            .and_then(|b| Surface::decode_payload(did, &b).ok());
        let solid = s.as_ref().and_then(|s| s.color_value);
        if solid.is_some_and(|c| c & 0x00FF_FFFF == D0) {
            culprits.push(format!("{id:08X} solid"));
        }
        match textures.bgra8(did) {
            Ok(img) => {
                if img.pixels.iter().any(|p| is_d0(p)) {
                    culprits.push(format!("{id:08X} texel"));
                }
            }
            Err(_) => untextured += 1,
        }
        eprintln!(
            "  {id:08X} type={:#06x} colour={:?} faces={n}",
            s.as_ref().map_or(0, |s| s.surface_type),
            solid.map(|c| format!("{c:08X}")),
        );
    }
    eprintln!(
        "academy cell-surface census: {} distinct surfaces, {untextured} with no image; \
         candidates for RGB(208,208,208): {culprits:?}",
        faces.len()
    );
    assert!(
        untextured > 0,
        "no untextured cell surface -- the C8C8C8 fillers went missing"
    );
    assert!(
        culprits.is_empty(),
        "an academy cell surface can paint the indoor grey after all: {culprits:?}"
    );
}

/// How many of the academy's cells have a portal that leads **outdoors** at all — the one thing
/// that can still put landscape drawing in an indoor frame once the indoor gate removes the outdoor pass.
///
/// Zero would mean the gate removes the outdoor pass from the whole academy unconditionally, which
/// is the claim the gpu tier's indoor/outdoor pass test measures at one pose; this counts it over all
/// 1,704 cells so the pose is not doing the work.
#[test]
fn the_academy_has_no_cell_portal_that_leads_outdoors() {
    let store = dereth_dat::testing::open_store_or_fail();
    let mut loader = EnvCellLoader::new();
    let (mut cells_seen, mut portals, mut outdoors) = (0u32, 0u32, 0u32);
    for block in [0x7f03u16, 0x8602, 0x8c04] {
        for cell in loader.load_block(&store, block) {
            cells_seen += 1;
            for p in &cell.cell.portals {
                portals += 1;
                // Cell decoding stores `0xFFFFFFFF` for a portal whose flags bit 2 is set.
                if p.other_cell_id == 0xFFFF_FFFF {
                    outdoors += 1;
                }
            }
        }
    }
    eprintln!(
        "academy portal census: {cells_seen} cells, {portals} cell portals, \
         {outdoors} of them leading outdoors"
    );
    assert_eq!(cells_seen, 1704, "the three complete academy blocks");
    assert!(
        portals > 1000,
        "only {portals} portals -- the census is not reading the dat"
    );
    assert_eq!(
        outdoors, 0,
        "{outdoors} academy cell portals lead outdoors, so `outside_view.view_count` can be \
         non-zero there and the gate alone would not remove the sky"
    );
}

/// Every untextured face of the three academy blocks' cells is portal-linked.
#[test]
fn academy_cells_exercise_retail_untextured_subset_guard() {
    let store = dereth_dat::testing::open_store().expect("required retail DATs");
    let mut loader = EnvCellLoader::new();
    for block in [0x7f03, 0x8602, 0x8c04] {
        let cells = loader.load_block(&store, block);
        assert_eq!(cells.len(), 568, "complete academy cell set");
        let mut solids = BTreeMap::new();
        let mut portal_faces = 0;
        for cell in cells {
            for (index, polygon) in cell.structure.polygons.iter().enumerate() {
                let id = cell.cell.surfaces[usize::from(polygon.pos_surface)];
                let bytes = store.read_typed(DbType::Surface, id).unwrap();
                let surface = Surface::decode_payload(id, &bytes).unwrap();
                if surface.surface_type & 6 == 0 {
                    *solids
                        .entry((id, surface.surface_type, surface.color_value))
                        .or_insert(0) += 1;
                    if cell
                        .cell
                        .portals
                        .iter()
                        .any(|p| usize::from(p.polygon_id) == index)
                    {
                        portal_faces += 1;
                    }
                }
            }
        }
        eprintln!(
            "academy {block:04x}: untextured polygons {solids:?}; portal faces {portal_faces}"
        );
        assert_eq!(
            solids.values().sum::<u32>(),
            1372,
            "complete authored solid-face census"
        );
        assert_eq!(
            portal_faces, 1372,
            "these particular authored solids are all portal-linked"
        );
    }
    assert_eq!(loader.stats.missing, 0);
    assert_eq!(loader.stats.undecodable, 0);
    assert_eq!(loader.stats.no_environment, 0);
}
