//! The gfxobj seam reports present/absent/unknown; a swap naming an absent gfxobj fails and keeps
//! the limb while the rest of the list applies; a swapped part takes the new degrade record;
//! without an index the swap is accepted; shipped setups name held gfxobjs with one measured
//! exception.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use std::sync::Arc;

use dereth_animation::data::{AnimAssets, GfxObjLookup, MapAssets, NoAssets};
use dereth_animation::parts::{
    AnimPartChange, GfxObjArrayLoad, ObjDesc, PaletteRange, PartArray, PhysicsPart,
};
use dereth_animation::seq::Sequence;
use dereth_dat::DbType;
use dereth_primitives::DataId;

fn assets() -> common::DatAssets {
    common::open()
}

/// A `0x01……` id in the `GfxObj` number space that the dat does **not** hold. Asserted, never
/// assumed: `absent_id` walks up from a high value until the store says no.
fn absent_id(a: &common::DatAssets) -> DataId {
    for n in (0x0100_0000_u32..0x0100_FFFF).rev() {
        let id = DataId(n);
        if a.gfxobj(id) == GfxObjLookup::Absent {
            return id;
        }
    }
    panic!("every id in 0x01000000..0x0100FFFF is present, which cannot be right");
}

// ---------------------------------------------------------------------------------------------
// 1. The seam itself, and its calibration
// ---------------------------------------------------------------------------------------------

/// §7.8: an instrument that reports "absent" must first be shown reporting "present".
#[test]
fn the_gfxobj_seam_answers_present_absent_and_unknown_and_each_one_is_calibrated() {
    let a = assets();
    let ids = a.ids_of(DbType::GfxObj);
    assert!(!ids.is_empty(), "the input contains graphics objects");

    // Known positive: the first shipped id must be Present, and some shipped id must name a
    // degrade record (4,131 of them do).
    let first = ids[0];
    assert!(
        matches!(a.gfxobj(first), GfxObjLookup::Present { .. }),
        "{first:?} is in the dat and the seam said otherwise"
    );
    let with_record = ids
        .iter()
        .take(4_000)
        .filter(|id| {
            matches!(
                a.gfxobj(**id),
                GfxObjLookup::Present {
                    did_degrade: Some(_)
                }
            )
        })
        .count();
    assert!(
        with_record > 0,
        "the seam never reported a did_degrade over the shipped objects"
    );

    // Known negative: an id the dat does not hold.
    let gone = absent_id(&a);
    assert_eq!(a.gfxobj(gone), GfxObjLookup::Absent, "{gone:?}");

    // The third state: a source with no index must not answer Absent, or every hand-built
    // fixture in the crate would start failing its swaps.
    assert_eq!(
        a.gfxobj(DataId(0)),
        GfxObjLookup::Absent,
        "id 0 is not in the dat either"
    );
    assert_eq!(
        NoAssets.gfxobj(first),
        GfxObjLookup::Unknown,
        "NoAssets has not looked"
    );
    assert_eq!(
        MapAssets::default().gfxobj(first),
        GfxObjLookup::Unknown,
        "nor has an empty map"
    );
    eprintln!("seam: graphics objects, absent witness {gone:?}, three states distinguished");
}

/// Oracle: graphics-object array loading's **two** failure paths.
#[test]
fn load_gfxobj_array_fails_on_an_absent_object_and_on_a_null_level_zero() {
    let a = assets();
    let gone = absent_id(&a);

    // Path 1: a missing type-6 graphics object returns 0 before any degrade lookup.
    assert_eq!(
        PhysicsPart::load_gfxobj_array(gone, &a),
        GfxObjArrayLoad::Failed
    );

    // A present object loads, and brings its own record with it.
    let real = a
        .ids_of(DbType::GfxObj)
        .into_iter()
        .find(|id| {
            matches!(
                a.gfxobj(*id),
                GfxObjLookup::Present {
                    did_degrade: Some(_)
                }
            )
        })
        .expect("some shipped graphics object names a degrade record");
    match PhysicsPart::load_gfxobj_array(real, &a) {
        GfxObjArrayLoad::Loaded { degrades: Some(d) } => {
            assert!(!d.degrades.is_empty(), "{real:?}: a record with no levels");
        }
        other => panic!("{real:?}: expected Loaded with a record, got {other:?}"),
    }

    // Path 2: `array[0] == NULL`. A hand-built source whose object is present but whose degrade
    // record's level 0 names INVALID_DID -- the client releases the array and returns 0.
    let mut m = MapAssets::default();
    let obj = DataId(0x0100_9001);
    let rec = DataId(0x1100_9001);
    m.gfxobjs = Some([(obj.0, Some(rec))].into_iter().collect());
    m.degrades.insert(
        rec.0,
        Arc::new(dereth_animation::data::DegradeInfo {
            degrades: vec![dereth_animation::data::GfxObjInfo {
                gfxobj_id: DataId(0),
                degrade_mode: 1,
                min_dist: 0.0,
                ideal_dist: 0.0,
                max_dist: 0.0,
            }],
        }),
    );
    assert_eq!(
        PhysicsPart::load_gfxobj_array(obj, &m),
        GfxObjArrayLoad::Failed,
        "level 0 is INVALID_DID, so array[0] is NULL and the load fails"
    );

    // And a source that cannot look answers neither.
    assert_eq!(
        PhysicsPart::load_gfxobj_array(real, &NoAssets),
        GfxObjArrayLoad::NotAsked
    );
}

/// Behaviour: objects.appearance.a-part-swap-naming-an-absent-object-keeps-the-limb
/// Oracle: the failing swap never reaches installation, so
/// **every** field installation would have written is still the old part's: the id, the surface
/// array (surface restoration is the installer's first step, so a refused swap does not restore
/// the surfaces either) and the degrade record.
#[test]
fn a_swap_naming_an_absent_gfxobj_keeps_the_limb_exactly_as_it_was() {
    let a = assets();
    let gone = absent_id(&a);
    let real = a
        .ids_of(DbType::GfxObj)
        .into_iter()
        .find(|id| {
            matches!(
                a.gfxobj(*id),
                GfxObjLookup::Present {
                    did_degrade: Some(_)
                }
            )
        })
        .expect("a shipped object with a degrade record");

    let mut p = PhysicsPart::new(real);
    assert!(p.set_part(real, &a), "the shipped id loads");
    let before = p.clone();
    assert!(before.degrades.is_some(), "and installed its record");
    p.set_texture_map(DataId(0x0500_0001), DataId(0x0500_0002));
    p.use_palette(
        DataId(0x0400_0001),
        &[PaletteRange {
            palette_set: DataId(0x0F00_0001),
            offset: 1,
            length: 2,
        }],
    );
    let dressed = p.clone();

    assert!(
        !p.set_part(gone, &a),
        "SetPart must return 0 for an id the dat does not hold"
    );
    assert_eq!(
        p.gfxobj_id, dressed.gfxobj_id,
        "the limb is still the old object"
    );
    assert_eq!(
        p.surface_overrides, dressed.surface_overrides,
        "the surfaces were not restored"
    );
    assert_eq!(
        p.degrades.as_ref().map(Arc::as_ptr),
        dressed.degrades.as_ref().map(Arc::as_ptr),
        "and it still carries the old object's degrade record"
    );

    // The other arm, on the same part, so the two differ only by the id: a present object is
    // installed and takes the surfaces with it.
    let other = a
        .ids_of(DbType::GfxObj)
        .into_iter()
        .find(|id| *id != real && matches!(a.gfxobj(*id), GfxObjLookup::Present { .. }))
        .expect("a second shipped object");
    assert!(
        p.set_part(other, &a),
        "SetPart must return 1 for an id the dat holds"
    );
    assert_eq!(p.gfxobj_id, other);
    assert!(
        p.surface_overrides
            .as_ref()
            .is_none_or(|s| s.texture_maps.is_empty()),
        "restoring the surfaces dropped the texture map"
    );
}
/// Behaviour: objects.appearance.a-swapped-part-carries-the-new-objects-degrade-record
#[test]
fn a_swapped_part_carries_the_new_objects_degrade_record() {
    let a = assets();
    let ids = a.ids_of(DbType::GfxObj);

    // Two shipped objects whose records give different max-degrade distances.
    let mut pair: Option<(DataId, DataId, f32, f32)> = None;
    let mut seen: Vec<(DataId, f32)> = Vec::new();
    for id in ids {
        let GfxObjLookup::Present {
            did_degrade: Some(_),
        } = a.gfxobj(id)
        else {
            continue;
        };
        let mut p = PhysicsPart::new(id);
        assert!(p.set_part(id, &a), "{id:?} is in the dat");
        let d = p.max_degrade_distance();
        if let Some((first, fd)) = seen.iter().find(|(_, fd)| (*fd - d).abs() > 1.0) {
            pair = Some((*first, id, *fd, d));
            break;
        }
        seen.push((id, d));
        if seen.len() > 500 {
            break;
        }
    }
    let (a_id, b_id, a_max, b_max) = pair.expect("two shipped records with different max distance");

    let mut p = PhysicsPart::new(a_id);
    assert!(p.set_part(a_id, &a));
    assert_eq!(p.max_degrade_distance(), a_max);
    let first_record = Arc::as_ptr(p.degrades.as_ref().expect("record a"));

    assert!(p.set_part(b_id, &a), "the swap succeeds");
    let second_record = Arc::as_ptr(p.degrades.as_ref().expect("record b"));
    assert_ne!(
        first_record, second_record,
        "the part is still holding the old record"
    );
    assert_eq!(
        p.max_degrade_distance(),
        b_max,
        "the max degrade distance must answer from the NEW record ({a_id:?} -> {b_id:?})"
    );
    eprintln!("{a_id:?} max {a_max} -> swap -> {b_id:?} max {b_max}, record replaced");
}

/// Do obj desc changes reports failure and still applies the rest of the list.
#[test]
fn do_obj_desc_changes_reports_failure_and_still_applies_the_rest_of_the_list() {
    let a = assets();
    let gone = absent_id(&a);

    let setup = a
        .setup(DataId(0x0200_0001))
        .expect("the human setup decodes");
    assert!(
        setup.parts.len() > 8,
        "the human body has more than 8 parts"
    );
    let mut seq = Sequence::new();
    let mut pa =
        PartArray::create_setup(Arc::clone(&setup), true, &mut seq, &a).expect("CreateSetup");

    let good = a
        .ids_of(DbType::GfxObj)
        .into_iter()
        .find(|id| {
            !setup.parts.contains(id) && matches!(a.gfxobj(*id), GfxObjLookup::Present { .. })
        })
        .expect("a shipped object the setup does not already use");
    let untouched = pa.parts[5].gfxobj_id;

    let od = ObjDesc {
        part_changes: vec![
            AnimPartChange {
                part_index: 5,
                part_id: gone,
            },
            AnimPartChange {
                part_index: 6,
                part_id: good,
            },
        ],
        ..ObjDesc::default()
    };
    assert!(
        !pa.do_obj_desc_changes_with(&od, &a),
        "the part-change operation must report failure when one swap fails"
    );
    assert_eq!(
        pa.parts[5].gfxobj_id, untouched,
        "the failed swap kept the limb"
    );
    assert_eq!(
        pa.parts[6].gfxobj_id, good,
        "and the rest of the list still ran"
    );

    // Both changes good: the call reports success.
    let od = ObjDesc {
        part_changes: vec![AnimPartChange {
            part_index: 5,
            part_id: good,
        }],
        ..ObjDesc::default()
    };
    assert!(
        pa.do_obj_desc_changes_with(&od, &a),
        "a list of shipped ids succeeds"
    );
    assert_eq!(pa.parts[5].gfxobj_id, good);
}

/// Without a gfxobj index the same swap is accepted and the limb is replaced.
#[test]
fn without_a_gfxobj_index_the_same_swap_is_accepted_and_the_limb_is_replaced() {
    let a = assets();
    let gone = absent_id(&a);
    let setup = a.setup(DataId(0x0200_0001)).expect("the human setup");
    let mut seq = Sequence::new();
    let mut pa = PartArray::create_setup(Arc::clone(&setup), true, &mut seq, &a).expect("setup");

    let od = ObjDesc {
        part_changes: vec![AnimPartChange {
            part_index: 5,
            part_id: gone,
        }],
        ..ObjDesc::default()
    };
    // `do_obj_desc_changes` without assets == `do_obj_desc_changes_with(od, &NoAssets)`.
    assert!(pa.do_obj_desc_changes(&od), "no index means no refusal");
    assert_eq!(
        pa.parts[5].gfxobj_id, gone,
        "the limb has been replaced by nothing drawable"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Does any real server ever send such an id?
// ---------------------------------------------------------------------------------------------

/// The shipped setups name gfxobjs the dat holds with one measured exception.
#[test]
fn the_shipped_setups_name_gfxobjs_the_dat_holds_with_one_measured_exception() {
    let a = assets();
    let setups = a.ids_of(DbType::Setup);
    assert!(!setups.is_empty(), "the input contains setups");
    let expected_parts: usize = setups
        .iter()
        .map(|id| a.setup(*id).expect("setup decodes").parts.len())
        .sum();

    let mut part_ids = 0usize;
    // Three buckets, not one, because the part-swap guard and array loading's two failures are
    // three different reasons and collapsing them would hide which one the dat actually contains.
    let mut invalid_did: Vec<(DataId, DataId)> = Vec::new();
    let mut absent: Vec<(DataId, DataId)> = Vec::new();
    let mut null_level_zero: Vec<(DataId, DataId)> = Vec::new();
    let mut failing_setups = 0usize;
    let mut empty_setups = 0usize;
    for sid in setups {
        let s = a.setup(sid).expect("setup decodes");
        if s.parts.is_empty() {
            empty_setups += 1;
            continue;
        }
        let mut failed = false;
        for pid in &s.parts {
            part_ids += 1;
            if *pid == DataId(0) {
                // `INVALID_DID` — refused before the load is attempted.
                failed = true;
                invalid_did.push((sid, *pid));
                continue;
            }
            match PhysicsPart::load_gfxobj_array(*pid, &a) {
                GfxObjArrayLoad::Loaded { .. } => {}
                GfxObjArrayLoad::NotAsked => panic!("the dat-backed seam answered NotAsked"),
                GfxObjArrayLoad::Failed => {
                    failed = true;
                    if a.gfxobj(*pid) == GfxObjLookup::Absent {
                        absent.push((sid, *pid));
                    } else {
                        null_level_zero.push((sid, *pid));
                    }
                }
            }
        }
        if failed {
            failing_setups += 1;
        }
    }

    let all = || invalid_did.iter().chain(&absent).chain(&null_level_zero);
    let mut bad_setups: Vec<u32> = all().map(|(s, _)| s.0).collect();
    bad_setups.sort_unstable();
    bad_setups.dedup();
    let mut bad_ids: Vec<u32> = all().map(|(_, p)| p.0).collect();
    bad_ids.sort_unstable();
    bad_ids.dedup();

    eprintln!(
        "setup census: input setups ({empty_setups} with no parts), {part_ids} part ids; \
         {failing_setups} setups fail part initialization over {} failing slots -- {} INVALID_DID, {} \
         naming an absent graphics object, {} naming one whose degrade record's level 0 is NULL. \
         Distinct setups: {:08X?}. Distinct ids: {:08X?}",
        invalid_did.len() + absent.len() + null_level_zero.len(),
        invalid_did.len(),
        absent.len(),
        null_level_zero.len(),
        bad_setups,
        bad_ids,
    );

    // A denominator, so that a sweep which looked at nothing cannot report zero failures.
    assert!(part_ids > 0, "the input contains setup parts");
    assert_eq!(part_ids, expected_parts, "every input part is checked");

    assert_eq!(
        null_level_zero.len(),
        0,
        "no shipped setup part has a NULL level 0"
    );
    assert!(
        !invalid_did.is_empty(),
        "the input exercises this refusal arm"
    );
    assert!(!absent.is_empty(), "absent graphics objects are exercised");
    assert_eq!(
        failing_setups,
        bad_setups.len(),
        "each failing setup is identified"
    );
    assert_eq!(
        bad_ids,
        vec![0x0000_0000, 0x0100_4E29],
        "the ids that cannot be loaded"
    );
    assert_eq!(
        bad_setups,
        vec![0x0200_1C4F, 0x0200_1C50],
        "the two setups that name them"
    );

    for sid in [DataId(0x0200_1C4F), DataId(0x0200_1C50)] {
        let s = a.setup(sid).expect("setup decodes");
        assert!(
            s.parts.contains(&DataId(0)),
            "{sid:?}: expected an INVALID_DID part slot"
        );
        assert!(
            s.parts.contains(&DataId(0x0100_4E29)),
            "{sid:?}: expected the absent object"
        );
        for source in [&a as &dyn AnimAssets, &NoAssets] {
            let mut seq = Sequence::new();
            assert!(
                PartArray::create_setup(Arc::clone(&s), true, &mut seq, source).is_none(),
                "{sid:?}: part creation must fail -- the INVALID_DID guard alone is enough"
            );
        }
    }

    // The contrast that isolates the *absent object* arm from the guard: the same setup with its
    // `INVALID_DID` slots replaced by a shipped id builds with no index and is refused with one.
    let s = a.setup(DataId(0x0200_1C4F)).expect("setup decodes");
    let good = a
        .ids_of(DbType::GfxObj)
        .into_iter()
        .find(|id| matches!(a.gfxobj(*id), GfxObjLookup::Present { .. }))
        .expect("a shipped object");
    let mut patched = (*s).clone();
    for p in &mut patched.parts {
        if *p == DataId(0) {
            *p = good;
        }
    }
    let patched = Arc::new(patched);
    let mut seq = Sequence::new();
    assert!(
        PartArray::create_setup(Arc::clone(&patched), true, &mut seq, &NoAssets).is_some(),
        "with no graphics-object index there is nothing left to refuse"
    );
    let mut seq = Sequence::new();
    assert!(
        PartArray::create_setup(patched, true, &mut seq, &a).is_none(),
        "0x01004E29 is not in the dat, so part creation still fails"
    );
}
