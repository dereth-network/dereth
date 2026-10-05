//! A part swap naming a graphics object the dat does not hold is refused and the previous part is
//! kept, and a successful swap carries the new object's degrade record. `DatAnimAssets::gfxobj`
//! answers Present or Absent for every id, and three client consumers use it:
//!
//! | consumer | path |
//! |---|---|
//! | world_scene.rs apply_player_objdesc | the local body's part array |
//! | world_scene.rs sync_objects create arm | a newly constructed server-object MotionDriver |
//! | preview.rs add_object_dressed | a dressed preview model and its baked mesh sources |
//!
//! Fixture: the retail dats. The lookup is calibrated over all 15,318 indexed graphics objects,
//! 4,131 of which name a GfxObjDegradeInfo. The absent witness is the first id found searching
//! downward through 0x01000000..0x0100FFFF with a typed-read error, then checked through the
//! lookup. The mechanism itself is covered in the animation crate's absent-graphics-object tests;
//! these are data, model and bake checks with GPU resources but no pixel or frame-draw oracle.
//! Required fixture failures are fatal.

#![cfg(gpu)]

use super::common::{retail_store, test_gpu};
use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_animation::data::{AnimAssets, GfxObjLookup, NoAssets};
use dereth_animation::parts::ObjDesc as AnimObjDesc;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::objects::{ItemCreateObject, ItemObjDescEvent, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{
    AnimPartChange, ObjDesc as ProtocolObjDesc, PhysicsDesc, PhysicsEventStamp, PositionWire,
    PublicWeenieDesc,
};
use dereth_protocol::{write_body, Opcode};
use dereth_render::device::Gpu;
use dereth_world_data::anim_assets::DatAnimAssets;
use {
    dereth_client_runtime::landblock::load_region, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};
use {dereth_scene::preview::PreviewObject, dereth_scene::preview::PreviewSpace};

/// The Aluvian male body: the setup `Character::new` builds and the one every assertion below
/// dresses. 34 parts.
const BODY: DataId = DataId(0x0200_0001);

/// The outdoor landblock and cell held_objects_draw uses, to load the body without an interior.
const LANDBLOCK: u16 = 0x8602;
const CELL: u32 = 0x8602_01AD;

/// Two shipped reference objects with different maximum degrade distances, so the record-update
/// check cannot pass with two absent records or an unchanged previous record.
const LOD_A: DataId = DataId(0x0100_0001);
const LOD_B: DataId = DataId(0x0100_001C);

/// Search downward within the graphics-object id range for a typed-read failure. The lookup test
/// separately requires Absent for the witness; this helper does not distinguish read-error kinds.
fn absent_id(store: &RetailDatStore) -> DataId {
    for n in (0x0100_0000_u32..0x0100_FFFF).rev() {
        let id = DataId(n);
        if store.read_typed(DbType::GfxObj, id).is_err() {
            return id;
        }
    }
    panic!("every id in 0x01000000..0x0100FFFF is present, which cannot be right");
}

/// The setup's original parts, which a refused initial swap must retain.
fn setup_parts(assets: &DatAnimAssets) -> Vec<DataId> {
    AnimAssets::setup(assets, BODY)
        .expect("setup record 0x02000001 decodes")
        .parts
        .clone()
}

/// One `AnimPartChange` on the wire: part `index` becomes `to`.
fn wire_swap(index: u8, to: DataId) -> ProtocolObjDesc {
    ProtocolObjDesc {
        anim_part_changes: vec![AnimPartChange {
            part_index: index,
            part_id: to.0,
        }],
        ..ProtocolObjDesc::default()
    }
}

/// The same change in the animation crate's shape, for the preview, which takes no wire types.
fn anim_swap(index: u8, to: DataId) -> AnimObjDesc {
    let mut od = AnimObjDesc::default();
    dereth_scene::preview::add_anim_part_change(
        &mut od,
        dereth_animation::parts::AnimPartChange {
            part_index: u32::from(index),
            part_id: to,
        },
    );
    od
}

// ---------------------------------------------------------------------------------------------
// 1. The instrument, calibrated in both directions before anything is believed.
// ---------------------------------------------------------------------------------------------

/// **The dat lookup answers Present for every shipped graphics object, Absent for a missing one,
/// and never Unknown.** Every indexed id must be Present; 4,131 of the 15,318 name a degrade
/// record. A typed-read-failure witness and id zero are Absent, and the index-free `NoAssets`
/// still answers Unknown.
#[test]
fn the_dat_seam_answers_present_and_absent_and_never_unknown() {
    let store = retail_store();
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let ids = store.ids_of(DbType::GfxObj);
    assert_eq!(ids.len(), 15_318, "graphics objects in client_portal.dat");

    let (mut present, mut with_record, mut absent, mut unknown) = (0usize, 0usize, 0usize, 0usize);
    for id in &ids {
        match assets.gfxobj(*id) {
            GfxObjLookup::Present { did_degrade } => {
                present += 1;
                if did_degrade.is_some() {
                    with_record += 1;
                }
            }
            GfxObjLookup::Absent => absent += 1,
            GfxObjLookup::Unknown => unknown += 1,
        }
    }
    assert_eq!(
        unknown,
        0,
        "the client's own asset source answered \"I have not looked\" for {unknown} of {} shipped \
         objects -- the lookup is not wired",
        ids.len()
    );
    assert_eq!(
        present,
        ids.len(),
        "{absent} shipped graphics objects did not decode through the lookup"
    );
    assert_eq!(
        with_record, 4_131,
        "4,131 of the 15,318 graphics objects name a GfxObjDegradeInfo; the lookup counts \
         {with_record}"
    );

    // Select a typed-read-failure witness rather than assuming a particular id is absent.
    let gone = absent_id(&store);
    assert_eq!(assets.gfxobj(gone), GfxObjLookup::Absent, "{gone:?}");
    assert_eq!(
        assets.gfxobj(DataId(0)),
        GfxObjLookup::Absent,
        "id zero is not in the dat"
    );

    // Repeated answers must agree. Caching avoids repeated decoding in production, but equality
    // alone does not measure cache use or exclude an implementation that decodes twice.
    assert_eq!(
        assets.gfxobj(gone),
        GfxObjLookup::Absent,
        "the memo disagreed with itself"
    );
    assert_eq!(
        assets.gfxobj(ids[0]),
        assets.gfxobj(ids[0]),
        "the memo disagreed with itself"
    );

    // And the third state is still a state: a source with no index must not answer `Absent`, or
    // every hand-built fixture in the workspace would start failing its swaps.
    assert_eq!(
        NoAssets.gfxobj(ids[0]),
        GfxObjLookup::Unknown,
        "NoAssets has not looked"
    );

    eprintln!(
        "gfxobj lookup: {present} present / {absent} absent / {unknown} unknown over {} shipped \
         graphics objects; {with_record} name a degrade record; absent witness {gone:#010X}",
        ids.len(),
        gone = gone.0
    );
}

/// **The two reference objects carry different degrade records** (200 and 64), pinned so the
/// degrade assertions below cannot pass by both being absent.
#[test]
fn the_two_reference_objects_carry_different_degrade_records() {
    let store = retail_store();
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let max = |id: DataId| {
        let mut p = dereth_animation::parts::PhysicsPart::new(id);
        assert!(p.set_part(id, &assets), "{id:?} is in the dat");
        p.max_degrade_distance()
    };
    let (a, b) = (max(LOD_A), max(LOD_B));
    assert!(
        (a - 200.0).abs() < f32::EPSILON,
        "graphics object 0x01000001 max degrade distance is {a}"
    );
    assert!(
        (b - 64.0).abs() < f32::EPSILON,
        "graphics object 0x0100001C max degrade distance is {b}"
    );
    assert_ne!(a, b, "the pair must differ or nothing below discriminates");
}

// ---------------------------------------------------------------------------------------------
// 2. Local-body description application in world_scene.rs.
// ---------------------------------------------------------------------------------------------

/// Build a scene with the local body at the selected outdoor cell and return an empty stream.
/// The caller supplies its synthetic player-create and subsequent description events.
fn body_scene(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> (WorldScene, ObjectStream) {
    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        start_cell: Some(dereth_primitives::CellId(CELL)),
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let region = load_region(store).expect("the region decodes");
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    (scene, ObjectStream::new())
}

const PLAYER: ObjectId = ObjectId(0x5000_0001);

/// `0xF745 Item_CreateObject` for the player, carrying `od`.
fn player_create(od: ProtocolObjDesc) -> Vec<SessionEvent> {
    let physicsdesc = PhysicsDesc {
        setup_id: Some(BODY.0),
        bitfield: flags::SETUP,
        ..PhysicsDesc::default()
    };
    let body = write_body(&ItemCreateObject(ObjectCreatePayload {
        id: PLAYER,
        objdesc: od,
        physicsdesc,
        wdesc: PublicWeenieDesc::default(),
    }))
    .expect("encode");
    vec![
        SessionEvent::PlayerCreated(PLAYER),
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        },
    ]
}

/// `0xF625 Item_ObjDescEvent` — the dye, the equip and the unequip.
fn objdesc_event(id: ObjectId, od: ProtocolObjDesc, ts: u16) -> SessionEvent {
    let body = write_body(&ItemObjDescEvent {
        id,
        objdesc: od,
        timestamps: PhysicsEventStamp {
            instance: 0,
            event: ts,
        },
    })
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_OBJ_DESC_EVENT,
        body,
    }
}

/// Behaviour: objects.appearance.a-swap-naming-an-absent-gfxobj-keeps-the-previous-part
///
/// A local-body swap to an absent graphics object keeps the previously accepted part. Three
/// arms rule out both ignoring every description and latching after a failure:
/// 1. A shipped replacement changes the part id with zero failures.
/// 2. The failed replacement keeps the full prior part/LOD array, not merely the setup default,
///    and increments the failure count exactly once.
/// 3. Another shipped replacement changes the id again without another failure.
/// The readings are part ids, LOD data and counters, not a pixel measurement of a limb.
#[test]
fn the_local_body_keeps_the_limb_when_a_swap_names_an_absent_gfxobj() {
    let store = retail_store();
    let mut gpu = test_gpu(320, 320);
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let base = setup_parts(&assets);
    let gone = absent_id(&store);
    let (mut scene, mut stream) = body_scene(&store, &mut gpu);

    // Use part zero and prove its setup id differs from LOD_A. The reference-pair calibration
    // separately proves LOD_A and LOD_B differ; this is not a search for a suitable part index.
    let index: u8 = 0;
    assert_ne!(
        base[usize::from(index)],
        LOD_A,
        "part {index} already is the first reference id"
    );

    let mut ts = 1u16;
    let mut apply = |scene: &mut WorldScene, stream: &mut ObjectStream, ev: Vec<SessionEvent>| {
        for e in &ev {
            stream.apply_event(e, LocalTime(f64::from(ts)));
        }
        ts += 1;
        scene
            .sync_objects(&store, &mut gpu, stream)
            .expect("objects sync");
    };

    // Arm 1: a shipped object.
    apply(
        &mut scene,
        &mut stream,
        player_create(wire_swap(index, LOD_A)),
    );
    assert_eq!(
        scene.draw.stats.objdesc_setup_mismatch, 0,
        "the body would not rebuild from 0x02000001"
    );
    assert!(
        scene.draw.stats.objdescs_applied > 0,
        "not one description was applied"
    );
    assert_eq!(
        scene.draw.stats.objdesc_failures, 0,
        "a swap to a shipped object must not fail"
    );
    let lod = scene.character_part_lod().expect("a local body");
    assert_eq!(
        lod[usize::from(index)].0,
        LOD_A,
        "the shipped swap did not reach the limb"
    );
    let after_arm1 = lod.clone();

    // Arm 2: the object the dat does not hold.
    apply(
        &mut scene,
        &mut stream,
        vec![objdesc_event(PLAYER, wire_swap(index, gone), 2)],
    );
    assert_eq!(
        scene.draw.stats.objdesc_failures, 1,
        "object-description changes accepted a swap naming graphics object {:#010X}, which \
         client_portal.dat does not hold",
        gone.0
    );
    let kept = scene.character_part_lod().expect("a local body");
    assert_eq!(
        kept[usize::from(index)].0,
        LOD_A,
        "the refused swap moved the limb to {:?}; the client keeps the previous one",
        kept[usize::from(index)].0
    );
    assert_eq!(
        kept, after_arm1,
        "the refused swap changed some other part of the body"
    );

    // Arm 3: it was not latched.
    apply(
        &mut scene,
        &mut stream,
        vec![objdesc_event(PLAYER, wire_swap(index, LOD_B), 3)],
    );
    assert_eq!(
        scene.draw.stats.objdesc_failures, 1,
        "the second shipped swap must not fail"
    );
    let moved = scene.character_part_lod().expect("a local body");
    assert_eq!(
        moved[usize::from(index)].0,
        LOD_B,
        "the limb latched at the first shipped id"
    );

    eprintln!(
        "local body: part {index} {:#010X} -> {:#010X} -> refused {:#010X} (kept) -> \
         {:#010X}; objdesc_failures {}",
        base[usize::from(index)].0,
        LOD_A.0,
        gone.0,
        LOD_B.0,
        scene.draw.stats.objdesc_failures
    );
}

/// Behaviour: objects.appearance.a-swapped-part-carries-the-new-objects-degrade-record
///
/// A successful local-body swap replaces the part's degrade record too. The no-record maximum is
/// 100.0; the reference distances 200 and 64 differ from that default and from each other, so a
/// missing record and a stale previous record are both caught.
#[test]
fn a_swapped_part_on_the_local_body_carries_the_new_objects_degrade_record() {
    let store = retail_store();
    let mut gpu = test_gpu(320, 320);
    let (mut scene, mut stream) = body_scene(&store, &mut gpu);

    let index: u8 = 0;
    for e in &player_create(wire_swap(index, LOD_A)) {
        stream.apply_event(e, LocalTime(1.0));
    }
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("objects sync");
    let a = scene.character_part_lod().expect("a local body")[usize::from(index)];
    assert_eq!(
        a,
        (LOD_A, 200.0),
        "the part carries {:?} at max degrade distance {}",
        a.0,
        a.1
    );

    stream.apply_event(
        &objdesc_event(PLAYER, wire_swap(index, LOD_B), 2),
        LocalTime(2.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("objects sync");
    let b = scene.character_part_lod().expect("a local body")[usize::from(index)];
    assert_eq!(
        b,
        (LOD_B, 64.0),
        "the swap left the old object's LOD table on the part"
    );

    eprintln!(
        "degrade record: {:#010X} max 200 -> {:#010X} max 64",
        LOD_A.0, LOD_B.0
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Server-object creation in world_scene.rs sync_objects.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.a-swap-naming-an-absent-gfxobj-keeps-the-previous-part
///
/// A server object's swap naming an absent graphics object keeps the setup's part. Each arm
/// creates a new MotionDriver and applies its description before mesh baking, so the failure arm
/// keeps the setup part while the success arm uses the shipped replacement and its distance.
#[test]
fn a_server_objects_swap_naming_an_absent_gfxobj_keeps_the_setups_part() {
    let store = retail_store();
    let mut gpu = test_gpu(320, 320);
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let base = setup_parts(&assets);
    let gone = absent_id(&store);

    // `--no-character`: with no local body every object the server sends is a `SceneObject`, which
    // is the arm under test.
    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        start_cell: Some(dereth_primitives::CellId(CELL)),
        character: false,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let index: u8 = 0;
    let mut run = |to: DataId| -> Vec<(DataId, f32)> {
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        let mut stream = ObjectStream::new();
        let physicsdesc = PhysicsDesc {
            setup_id: Some(BODY.0),
            bitfield: flags::SETUP | flags::POSITION,
            position: Some(PositionWire {
                objcell_id: CELL,
                frame: dereth_protocol::types::Frame::default(),
            }),
            ..PhysicsDesc::default()
        };
        let body = write_body(&ItemCreateObject(ObjectCreatePayload {
            id: ObjectId(0x6000_0001),
            objdesc: wire_swap(index, to),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        }))
        .expect("encode");
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(1.0),
        );
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync");
        assert_eq!(
            scene.server_object_count(),
            1,
            "the server object was not constructed"
        );
        let lod = scene
            .server_object_part_lod(ObjectId(0x6000_0001))
            .expect("the object has a part array");
        assert_eq!(
            scene.draw.stats.objdesc_failures,
            u64::from(to == gone),
            "objdesc_failures for a swap to {:#010X}",
            to.0
        );
        lod
    };

    let shipped = run(LOD_A);
    assert_eq!(
        shipped[usize::from(index)],
        (LOD_A, 200.0),
        "the shipped swap did not land"
    );
    let refused = run(gone);
    assert_eq!(
        refused[usize::from(index)].0,
        base[usize::from(index)],
        "the refused swap replaced the setup's own part instead of keeping it"
    );
    eprintln!(
        "server object: part {index} {:#010X} on a shipped swap, {:#010X} (the setup record's \
         own) on a swap to absent {:#010X}",
        LOD_A.0,
        refused[usize::from(index)].0 .0,
        gone.0
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Call site three: the preview space (`preview.rs`, `add_object_dressed`).
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.a-swap-naming-an-absent-gfxobj-keeps-the-previous-part
///
/// Preview dressing refuses the missing object and bakes geometry from the kept part.
/// `PreviewObject::dressed` records whether applying the description from defaults succeeded;
/// `PreviewObject::built_from` records mesh source ids. These separate the part model from the
/// geometry selected for baking, without a pixel or frame submission comparison.
#[test]
fn the_preview_refuses_a_swap_naming_an_absent_gfxobj_and_bakes_the_previous_limb() {
    let store = retail_store();
    let mut gpu = test_gpu(320, 320);
    let assets = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let base = setup_parts(&assets);
    let gone = absent_id(&store);
    let mut space = PreviewSpace::new(Arc::clone(&assets));
    let index: u8 = 0;

    let ok = space
        .add_object_dressed(&store, &mut gpu, BODY, Some(&anim_swap(index, LOD_A)))
        .expect("the preview bakes")
        .expect("the setup loads");
    let o = space.object(ok).expect("the object");
    assert_eq!(
        o.dressed,
        Some(true),
        "a swap to a shipped object must succeed"
    );
    // Baking uses the part's level-zero graphics object, which can differ from its model id.
    // LOD_A names itself at level zero; the kept setup part below does not, so the bake is checked
    // against the selected geometry and not the model id.
    let drawn = |o: &PreviewObject, i: u8| -> DataId {
        o.part_array.parts[usize::from(i)]
            .gfxobj_at(0)
            .expect("level 0 draws something")
    };
    assert_eq!(
        o.built_from()[usize::from(index)],
        LOD_A,
        "the mesh was baked from the old id"
    );
    assert_eq!(drawn(o, index), LOD_A, "and LOD_A is its own gfxobj[0]");

    let bad = space
        .add_object_dressed(&store, &mut gpu, BODY, Some(&anim_swap(index, gone)))
        .expect("the preview bakes")
        .expect("the setup loads");
    let o = space.object(bad).expect("the object");
    assert_eq!(
        o.dressed,
        Some(false),
        "default object-description changes accepted a swap naming absent graphics object {:#010X}",
        gone.0
    );
    assert_eq!(
        o.part_array.parts[usize::from(index)].gfxobj_id,
        base[usize::from(index)],
        "the refused swap left the part's model at the setup's own id"
    );
    // And the *geometry*: `gfxobj[0]` of that id, which for part 0 of the body is the high-poly
    // `0x01001787` and not `0x0100004E` itself.
    assert_eq!(
        o.built_from()[usize::from(index)],
        drawn(o, index),
        "the refused swap still reached the bake"
    );
    assert_eq!(
        o.drawn_parts(),
        34,
        "the refused swap left a part with no geometry"
    );

    eprintln!(
        "preview: dressed=Some(true) for {:#010X}, Some(false) for absent {:#010X}, \
         {} parts in the refused-swap preview",
        LOD_A.0,
        gone.0,
        o.drawn_parts()
    );
}
