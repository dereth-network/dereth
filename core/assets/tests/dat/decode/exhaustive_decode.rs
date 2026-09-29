//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every supported asset decodes completely, preserves its wire fields and resolves its dependencies.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::collections::BTreeMap;

use dereth_assets::material::{PFID_CUSTOM_RAW_JPEG, PROGRESSIVE_JPEG_IDS, RETAIL_PALETTE_BYTES};
use dereth_assets::world::LANDBLOCK_BYTES;
use dereth_assets::{
    decode_any, Decode, DecodedAsset, GfxObj, Palette, RenderSurface, Setup, SubDataIds, Wave,
};
use dereth_dat::{DatKind, DbType, RetailDatStore};
use dereth_primitives::{AssetSource, DataId};

fn store() -> RetailDatStore {
    let s = dereth_dat::testing::open_store_or_fail();
    assert!(
        s.grant_highres().expect("client_highres.dat opens"),
        "client_highres.dat is present"
    );
    s
}

/// Decode every object of one type and return `(count, decoded)`. Panics with the offending id and
/// the exact shortfall or overrun, which is the only diagnostic that matters here.
fn decode_all(s: &RetailDatStore, kind: DbType) -> Vec<(DataId, DecodedAsset)> {
    let ids = s.ids_of(kind);
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let bytes = s
            .read_typed(kind, id)
            .unwrap_or_else(|e| panic!("{kind:?} {id}: {e}"));
        let v = decode_any(kind, id, &bytes)
            .unwrap_or_else(|e| panic!("{kind:?} {id} ({} bytes): {e}", bytes.len()));
        out.push((id, v));
    }
    out
}

fn count_only(s: &RetailDatStore, kind: DbType) -> usize {
    decode_all(s, kind).len()
}

/// Portal formats decoded by this suite.
const PORTAL_TYPES: &[DbType] = &[
    DbType::GfxObj,
    DbType::Setup,
    DbType::Anim,
    DbType::Palette,
    DbType::SurfaceTexture,
    DbType::Surface,
    DbType::MTable,
    DbType::Wave,
    DbType::Environment,
    DbType::PalSet,
    DbType::Clothing,
    DbType::DegradeInfo,
    DbType::Scene,
    DbType::Region,
    DbType::RenderTexture,
    DbType::STable,
    DbType::ParticleEmitter,
    DbType::PhysicsScript,
    DbType::PhysicsScriptTable,
];

/// Every object of every supported portal type
/// decodes, with `expect_end()` clean.
#[test]
fn every_portal_object_decodes_with_the_cursor_on_the_end() {
    let s = store();
    let mut total = 0usize;
    for kind in PORTAL_TYPES {
        let ids = s.ids_of(*kind);
        assert!(!ids.is_empty(), "the input exercises {kind:?}");
        let n = count_only(&s, *kind);
        assert_eq!(n, ids.len(), "{kind:?}");
        total += n;
    }
    assert_eq!(
        total,
        PORTAL_TYPES
            .iter()
            .map(|kind| s.ids_of(*kind).len())
            .sum::<usize>()
    );
}

/// Every render surface decodes and the two partitions are disjoint.
#[test]
fn every_render_surface_decodes_and_the_two_partitions_are_disjoint() {
    let s = store();
    let portal_ids: Vec<DataId> = s
        .portal()
        .iter_ids()
        .filter(|i| dereth_dat::divine_type(*i) == Some(DbType::RenderSurface))
        .collect();
    let hi_ids: Vec<DataId> = s
        .highres()
        .unwrap()
        .iter_ids()
        .filter(|i| dereth_dat::divine_type(*i) == Some(DbType::RenderSurface))
        .collect();
    assert!(
        !portal_ids.is_empty(),
        "portal render surfaces are exercised"
    );
    assert!(
        !hi_ids.is_empty(),
        "high-resolution render surfaces are exercised"
    );

    let set: std::collections::BTreeSet<DataId> = portal_ids.iter().copied().collect();
    assert_eq!(hi_ids.iter().filter(|i| set.contains(i)).count(), 0);

    let mut jpeg = Vec::new();
    let mut categories: BTreeMap<(bool, u32), usize> = BTreeMap::new();
    for (from_hi, ids) in [(false, &portal_ids), (true, &hi_ids)] {
        for id in ids {
            let bytes = s.read_portal(*id).unwrap();
            let rs = RenderSurface::decode_payload(*id, &bytes)
                .unwrap_or_else(|e| panic!("{id} ({} bytes): {e}", bytes.len()));
            // The payload must be a real slice of the record at +0x18.
            let p = rs.payload(&bytes).expect("payload in range");
            assert_eq!(p.len(), rs.image_size as usize);
            assert_eq!(rs.data_offset, 0x18);
            *categories.entry((from_hi, rs.data_category)).or_insert(0) += 1;
            if rs.format == PFID_CUSTOM_RAW_JPEG {
                jpeg.push(id.raw());
                // Every raw JPEG payload starts with SOI.
                assert_eq!(&p[..2], &[0xFF, 0xD8], "{id} is not a JPEG");
            }
        }
    }

    assert!(!jpeg.is_empty(), "JPEG surfaces are exercised");
    let mut progressive: Vec<u32> = Vec::new();
    for id in &jpeg {
        let bytes = s.read_portal(DataId(*id)).unwrap();
        let rs = RenderSurface::decode_payload(DataId(*id), &bytes).unwrap();
        let p = rs.payload(&bytes).unwrap();
        if scan_jpeg_sof(p) == Some(0xC2) {
            progressive.push(*id);
        }
    }
    progressive.sort_unstable();
    assert_eq!(
        progressive,
        PROGRESSIVE_JPEG_IDS.to_vec(),
        "the progressive JPEG ids"
    );

    // the portal dat uses category 6, the high-res dat uses others.
    let portal_cats: Vec<u32> = categories
        .keys()
        .filter(|(hi, _)| !hi)
        .map(|(_, c)| *c)
        .collect();
    let hi_cats: Vec<u32> = categories
        .keys()
        .filter(|(hi, _)| *hi)
        .map(|(_, c)| *c)
        .collect();
    assert!(
        portal_cats.contains(&6),
        "portal categories: {portal_cats:?}"
    );
    assert!(!hi_cats.contains(&6), "high-res categories: {hi_cats:?}");
}

/// The SOF marker of a JPEG: `0xC0` baseline, `0xC2` progressive.
fn scan_jpeg_sof(b: &[u8]) -> Option<u8> {
    let mut i = 2usize; // skip SOI
    while i + 3 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
            return Some(m);
        }
        if m == 0xD8 || (0xD0..=0xD9).contains(&m) || m == 0xFF {
            i += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
        i += 2 + len.max(2);
    }
    None
}

/// Every palette has 2048 entries and is 8200 bytes.
#[test]
fn every_palette_has_2048_entries_and_is_8200_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::Palette);
    assert!(!ids.is_empty(), "palettes are exercised");
    for id in ids {
        let bytes = s.read_portal(id).unwrap();
        assert_eq!(bytes.len(), RETAIL_PALETTE_BYTES, "{id}");
        let p = Palette::decode_payload(id, &bytes).unwrap();
        assert_eq!(p.colors_argb.len(), 2048, "{id}");
    }
}

/// Every wave payload follows its header and occupies the rest of the record.
#[test]
fn every_wave_payload_is_bounded_and_uses_a_supported_format() {
    let s = store();
    let ids = s.ids_of(DbType::Wave);
    assert!(!ids.is_empty(), "waves are exercised");
    let input_count = ids.len();
    let mut pcm = 0usize;
    let mut mp3 = 0usize;
    let mut sample_bytes = 0usize;
    let mut record_bytes = 0usize;
    for id in ids {
        let bytes = s.read_portal(id).unwrap();
        record_bytes += bytes.len();
        let w = Wave::decode_payload(id, &bytes).unwrap();
        assert!(w.payload(&bytes).is_some(), "{id} payload out of range");
        assert_eq!(w.data_offset, 12 + w.header_size as usize);
        assert_eq!(w.data_offset + w.data_size as usize, bytes.len());
        assert_eq!(w.header.len(), w.header_size as usize);
        sample_bytes += w.data_size as usize;
        match w.format.map(|f| f.format_tag) {
            Some(dereth_assets::audio::WAVE_FORMAT_PCM) => pcm += 1,
            Some(dereth_assets::audio::WAVE_FORMAT_MPEGLAYER3) => mp3 += 1,
            other => panic!("{id}: unexpected wFormatTag {other:?}"),
        }
    }
    assert!(pcm > 0 && mp3 > 0, "both supported codecs are exercised");
    assert_eq!(pcm + mp3, input_count);
    assert!(sample_bytes > 0 && record_bytes > sample_bytes);
}

/// The region decodes and carries the documented constants.
#[test]
fn the_region_decodes_and_carries_the_documented_constants() {
    let s = store();
    let id = DataId(0x1300_0000);
    let bytes = s.read_portal(id).unwrap();
    let r = dereth_assets::Region::decode_payload(id, &bytes).unwrap();

    let t = &r.land_defs.land_height_table;
    assert_eq!(t.len(), 256);
    for (i, v) in t.iter().enumerate().take(201) {
        assert!(
            (*v - 2.0 * i as f32).abs() < 1e-6,
            "height table[{i}] = {v}, expected {}",
            2.0 * i as f32
        );
    }
    assert!(
        t[255] - 700.0 == 0.0,
        "the table must reach 700.0, got {}",
        t[255]
    );
    assert!(
        (t[202] - 2.0 * 202.0).abs() > 1e-6,
        "the table must be non-linear above index 201"
    );
    assert!(
        t.windows(2).all(|w| w[0] <= w[1]),
        "the table must be monotonic"
    );

    assert!((r.game_time.day_length - 7620.0).abs() < 1e-6);
    assert_eq!(r.game_time.days_per_year, 360);
    assert!((r.game_time.zero_time_of_year - 3600.0).abs() < 1e-9);
    assert_eq!(r.game_time.zero_year, 10);

    // parts_mask bit 0x8 is set but nothing extra is serialised, and the record
    // still ends exactly — which decode_payload has already proved by getting here.
    assert_ne!(
        r.parts_mask & 0x8,
        0,
        "parts_mask should carry the encounter bit"
    );
    assert_eq!(r.land_surf.surf_type, 0);
    assert!(r.land_surf.tex_merge.is_some());
}

/// Every cell record decodes with the cursor on the end.
#[test]
fn every_cell_record_decodes_with_the_cursor_on_the_end() {
    let s = store();
    let mut counts: BTreeMap<DbType, usize> = BTreeMap::new();
    let mut flag_dist: BTreeMap<u32, usize> = BTreeMap::new();
    for id in s.cell().iter_ids() {
        let Some(kind) = dereth_dat::classify_cell_id(id) else {
            assert_eq!(id, dereth_dat::ITERATION_LIST);
            continue;
        };
        let bytes = s.read_cell(id).unwrap();
        if kind == DbType::LandBlock {
            assert_eq!(bytes.len(), LANDBLOCK_BYTES, "{id}");
        }
        let v = decode_any(kind, id, &bytes)
            .unwrap_or_else(|e| panic!("{kind:?} {id} ({} bytes): {e}", bytes.len()));
        if let DecodedAsset::EnvCell(cell) = &v {
            *flag_dist.entry(cell.flags).or_insert(0) += 1;
            assert_eq!(
                cell.flags,
                u32::from_le_bytes(bytes[4..8].try_into().unwrap())
            );
            assert_eq!(cell.flags & !0xb, 0, "only supported cell flags occur");
            assert_eq!(cell.cell_id_repeat, id.raw(), "{id}");
        }
        *counts.entry(kind).or_insert(0) += 1;
    }
    for kind in [DbType::LandBlock, DbType::Lbi, DbType::Cell] {
        let expected = s
            .cell()
            .iter_ids()
            .filter(|id| dereth_dat::classify_cell_id(*id) == Some(kind))
            .count();
        assert!(expected > 0, "the input exercises {kind:?}");
        assert_eq!(counts[&kind], expected);
    }
    assert_eq!(
        counts.values().sum::<usize>(),
        s.cell()
            .iter_ids()
            .filter(|id| *id != dereth_dat::ITERATION_LIST)
            .count()
    );
    assert_eq!(flag_dist.values().sum::<usize>(), counts[&DbType::Cell]);
}

/// The id echo check fires on a corrupted payload.
#[test]
fn the_id_echo_check_fires_on_a_corrupted_payload() {
    let s = store();
    let id = s.ids_of(DbType::GfxObj)[0];
    let mut bytes = s.read_portal(id).unwrap();
    // Untouched, it decodes.
    GfxObj::decode_payload(id, &bytes).unwrap();
    // Flip the leading DataID, as a mis-computed B-tree offset would.
    bytes[0] ^= 0xFF;
    assert!(matches!(
        GfxObj::decode_payload(id, &bytes),
        Err(dereth_assets::AssetError::IdEchoMismatch { .. })
    ));
}

/// The dependency closure is bounded and terminates.
#[test]
fn the_dependency_closure_is_bounded_and_terminates() {
    let s = store();

    // Twenty landblocks, including dungeons (blocks whose LBI reports interior cells).
    let mut dungeon_roots: Vec<DataId> = Vec::new();
    let mut plain_roots: Vec<DataId> = Vec::new();
    for id in s.cell().iter_ids() {
        if id.raw() & 0xFFFF != 0xFFFE {
            continue;
        }
        if dungeon_roots.len() >= 5 && plain_roots.len() >= 15 {
            break;
        }
        let bytes = s.read_cell(id).unwrap();
        let lbi = dereth_assets::LandblockInfo::decode_payload(id, &bytes).unwrap();
        let block = DataId((id.raw() & 0xFFFF_0000) | 0xFFFF);
        if lbi.num_cells > 40 {
            if dungeon_roots.len() < 5 {
                dungeon_roots.push(block);
            }
        } else if plain_roots.len() < 15 {
            plain_roots.push(block);
        }
    }
    let dungeons = dungeon_roots.len();
    let mut roots = dungeon_roots;
    roots.extend(plain_roots);
    assert_eq!(roots.len(), 20);
    assert!(
        dungeons >= 3,
        "wanted at least three dungeon landblocks, got {dungeons}"
    );

    const STACK: usize = if cfg!(unoptimised) {
        512 * 1024
    } else {
        64 * 1024
    };
    let root_count = roots.len();
    let result = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let s = dereth_dat::testing::open_store().unwrap();
            assert!(s.grant_highres().expect("client_highres.dat opens"));
            let ids = dereth_assets::closure(&s, &roots).unwrap();
            assert!(
                roots.iter().all(|id| ids.contains(id)),
                "every root survives the closure"
            );
            let all_exist = ids.iter().all(|i| s.exists(*i));
            (ids.len(), all_exist)
        })
        .unwrap()
        .join()
        .expect("closure must fit in a small, fixed stack");
    let (n, all_exist) = result;
    assert!(n > root_count, "the roots expand to dependencies");
    assert!(
        all_exist,
        "every member of the closure must exist in the store"
    );
}

/// One setup closure contains every member reached by a manual walk.
#[test]
fn one_setup_closure_matches_a_manual_walk() {
    let s = store();
    let setup_id = s.ids_of(DbType::Setup)[0];
    let setup = Setup::decode_payload(setup_id, &s.read_portal(setup_id).unwrap()).unwrap();

    // By hand: the setup's own sub-ids, then each part's sub-ids, one level deep.
    let mut manual: std::collections::BTreeSet<DataId> = std::collections::BTreeSet::new();
    manual.insert(setup_id);
    let mut direct = Vec::new();
    setup.sub_data_ids(&mut direct);
    for id in &direct {
        if s.exists(*id) {
            manual.insert(*id);
        }
    }
    for part in &setup.parts {
        if !s.exists(*part) {
            continue;
        }
        let g = GfxObj::decode_payload(*part, &s.read_portal(*part).unwrap()).unwrap();
        let mut kids = Vec::new();
        g.sub_data_ids(&mut kids);
        for id in kids {
            if s.exists(id) {
                manual.insert(id);
            }
        }
    }

    let full = dereth_assets::closure(&s, &[setup_id]).unwrap();
    let full_set: std::collections::BTreeSet<DataId> = full.iter().copied().collect();
    // The closure is the transitive walk, so it must contain everything the two-level walk found.
    for id in &manual {
        assert!(full_set.contains(id), "closure missed {id}");
    }
    assert!(full_set.len() >= manual.len());
}

/// The seam: `AssetSource` must resolve and hand back the same bytes the typed path does.
#[test]
fn the_asset_source_seam_agrees_with_the_typed_path() {
    let s = store();
    for kind in [
        DbType::GfxObj,
        DbType::Setup,
        DbType::Palette,
        DbType::Surface,
    ] {
        for id in s.ids_of(kind).into_iter().take(50) {
            assert!(AssetSource::exists(&s, id), "{id}");
            let a = AssetSource::read(&s, id).unwrap();
            let b = s.read_typed(kind, id).unwrap();
            assert_eq!(a, b, "{id}");
        }
    }
    // An id inside GFXOBJ's range that no file uses. (`0x0100FFFE` would *not* do: it is also
    // landblock (1,0)'s information-record id, and that record does exist in the cell dat — which is
    // exactly the ambiguity `RetailDatStore::resolve` documents.)
    assert!(!AssetSource::exists(&s, DataId(0x0100_4E5A)));
    assert!(
        s.resolve(DataId(0x0100_FFFE)).is_some(),
        "0x0100FFFE is a real landblock info"
    );
    assert_eq!(dereth_dat::dat_for_type(DbType::GfxObj), DatKind::Portal);
}

/// Every setup stores upward steps before downward steps and cylinder radii before heights.
#[test]
fn setup_field_order_is_confirmed_by_the_shipped_values() {
    let s = store();
    let ids = s.ids_of(DbType::Setup);
    assert!(!ids.is_empty(), "setups are exercised");
    let mut cylinders = 0;
    let mut distinct_steps = 0;
    for id in ids {
        let bytes = s.read_portal(id).unwrap();
        let v = Setup::decode_payload(id, &bytes).unwrap();
        // Read fixed-width fields backwards from the trailer, independently of the decoder cursor.
        let bounds = bytes.len() - 20 - 4 - v.lights.len() * 48 - 32 - 16;
        let float = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(
            v.step_up_height,
            float(bounds + 8),
            "{id}: upward step field"
        );
        assert_eq!(
            v.step_down_height,
            float(bounds + 12),
            "{id}: downward step field"
        );
        if v.step_up_height != v.step_down_height {
            distinct_steps += 1;
        }
        let cylinders_start = bounds - 4 - v.spheres.len() * 16 - v.cylspheres.len() * 20;
        for (i, cylinder) in v.cylspheres.iter().enumerate() {
            let offset = cylinders_start + i * 20;
            assert_eq!(cylinder.radius, float(offset + 12), "{id}: cylinder radius");
            assert_eq!(cylinder.height, float(offset + 16), "{id}: cylinder height");
            cylinders += 1;
        }
    }
    assert!(cylinders > 0, "cylinder fields are exercised");
    assert!(
        distinct_steps > 0,
        "the step fields have a discriminating input"
    );
}

/// Every sound-adjustment hook carries a probability before its priority.
#[test]
fn sound_tweaked_hook_field_order_is_confirmed_by_the_shipped_values() {
    use dereth_assets::{AnimHook, HookData};
    let s = store();
    let mut vals: Vec<(f32, f32)> = Vec::new();
    let collect = |hooks: &[AnimHook], out: &mut Vec<(f32, f32)>| {
        for h in hooks {
            if let HookData::SoundTweaked {
                probability,
                priority,
                ..
            } = h.data
            {
                out.push((probability, priority));
            }
        }
    };
    for id in s.ids_of(DbType::Anim) {
        let a = dereth_assets::Animation::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
        for f in &a.part_frames {
            collect(&f.hooks, &mut vals);
        }
    }
    for id in s.ids_of(DbType::Setup) {
        let v = Setup::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
        for p in v.placement_frames.values() {
            collect(&p.hooks, &mut vals);
        }
    }
    for id in s.ids_of(DbType::PhysicsScript) {
        let p =
            dereth_assets::PhysicsScript::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
        for step in &p.script_data {
            collect(std::slice::from_ref(&step.hook), &mut vals);
        }
    }
    assert!(!vals.is_empty(), "sound-adjustment hooks are exercised");
    let zero_prob = vals.iter().filter(|(p, _)| *p == 0.0).count();
    let zero_prio = vals.iter().filter(|(_, p)| *p == 0.0).count();
    let one_prob = vals.iter().filter(|(p, _)| *p == 1.0).count();
    assert_eq!(
        zero_prob, 0,
        "no hook may have probability 0 — it would never play"
    );
    assert!(zero_prio > 0, "zero priority is exercised");
    assert!(one_prob > 0, "certain playback is exercised");
    assert!(vals.iter().all(|(_, priority)| priority.is_finite()));
    assert!(
        vals.iter().all(|(p, _)| (0.0..=1.0).contains(p)),
        "probability out of range"
    );
}

use dereth_assets::exhaustive_decode;

#[test]
fn every_object_in_all_four_dats_decodes_with_nothing_left_over() {
    let s = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the four retail dats are this gate's oracle and none is under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    assert!(
        s.grant_highres().expect("client_highres.dat opens"),
        "client_highres.dat must sit beside the other three"
    );
    let r = exhaustive_decode(&s).unwrap();

    for (id, kind, why) in r.failures.iter().take(20) {
        eprintln!("FAIL {id} {kind:?}: {why}");
    }
    assert!(
        r.failures.is_empty(),
        "{} decode failures",
        r.failures.len()
    );

    for (kind, n) in &r.no_decoder {
        eprintln!("no decoder for {kind:?} ({n} files)");
    }
    assert!(
        r.no_decoder.is_empty(),
        "{} types have no decoder",
        r.no_decoder.len()
    );

    let containers = [s.portal(), s.cell(), s.local(), s.highres().unwrap()];
    assert_eq!(
        r.entries,
        containers.iter().map(|f| f.len()).sum::<usize>(),
        "every directory entry is examined"
    );
    assert_eq!(
        r.untyped.len(),
        containers
            .iter()
            .filter(|f| f.entry(dereth_dat::ITERATION_LIST).is_some())
            .count()
    );
    assert!(r.untyped.iter().all(|i| *i == dereth_dat::ITERATION_LIST));
    assert_eq!(r.decoded, r.entries - r.untyped.len());
    assert!(r.is_clean());
}
