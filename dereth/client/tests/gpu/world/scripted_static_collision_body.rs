//! A scripted static's hook effects reach its own collision body. In retail one object per
//! placement holds the rendered parts, the queued default script and the collision state, so an
//! immediate scale hook changes the body as well. The client keeps the script host
//! (`dereth_client::particles::EmitterHost`) and the collision static apart and pairs them through
//! the placement's handle; these tests check the pairing and one concrete scale and radius update.
//!
//! Two tests read the shipped dats only: which setups' default scripts raise SCALE and
//! SOUND_TABLE hooks, and where those setups are placed (counts of hook records in each script
//! closure, not of executed hooks). Two scene tests, on a software device, inspect body state at
//! Holtburg and at the one environment-cell static a script doubles (`0x5655_010D`), not pixels.

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_assets::{Decode, LandblockInfo, PhysicsScript, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

/// Public animation-hook type 12: scale.
const SCALE: u32 = 12;
/// Public animation-hook type 2: sound-table lookup.
const SOUND_TABLE: u32 = 2;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Scan readable/decodable setups with a default script and count matching hook records in
/// their reachable script closure. Follow CallPes targets, visit each script ID once per setup,
/// and skip unreadable/undecodable scripts. Shared scripts count again for another setup, but
/// loops do not add repeated executions. This is metadata reachability, not live hook timing.
fn setups_raising(store: &RetailDatStore, hook: u32) -> Vec<(DataId, u32)> {
    let mut out = Vec::new();
    for id in &store.ids_of(DbType::Setup) {
        let Ok(bytes) = store.read_typed(DbType::Setup, *id) else {
            continue;
        };
        let Ok(s) = Setup::decode_payload(*id, &bytes) else {
            continue;
        };
        if s.default_script_id == DataId(0) {
            continue;
        }
        let mut hits = 0u32;
        let (mut queue, mut done) = (vec![s.default_script_id], Vec::new());
        while let Some(sid) = queue.pop() {
            if done.contains(&sid) {
                continue;
            }
            done.push(sid);
            let Ok(sb) = store.read_typed(DbType::PhysicsScript, sid) else {
                continue;
            };
            let Ok(sc) = PhysicsScript::decode_payload(sid, &sb) else {
                continue;
            };
            for step in &sc.script_data {
                if u32::from(step.hook.hook_type) == hook {
                    hits += 1;
                }
                if let dereth_assets::HookData::CallPes { pes, .. } = step.hook.data {
                    queue.push(pes);
                }
            }
        }
        if hits > 0 {
            out.push((*id, hits));
        }
    }
    out
}

/// Recount the 43 SCALE hook records across setup script closures, then locate those setups
/// in decoded landblock-info, Scene and environment-cell records. Only environment-cell count
/// and its landblock are asserted; the other placement categories are reported. This identifies
/// a real asset target without asserting that every metadata category has one global instance.
#[test]
fn the_shipped_scale_hooks_on_statics_name_their_setups_and_their_landblocks() {
    let store = store();
    let scale = setups_raising(&store, SCALE);
    let instances: u32 = scale.iter().map(|(_, n)| *n).sum();
    let ids: BTreeSet<u32> = scale.iter().map(|(i, _)| i.0).collect();
    eprintln!(
        "SCALE census: {} scripted setup(s), {instances} hook instance(s); setups {:08X?}",
        scale.len(),
        ids.iter().collect::<Vec<_>>()
    );
    // A claim about the shipped dats, so that a dat change fails loudly.
    assert_eq!(
        instances, 43,
        "the shipped scripted setups carry 43 SCALE hooks"
    );

    // Scan landblock-info object placements using the same public asset decoder as the scene.
    // Record each matching block, its first matching setup and the number of matches in it.
    let mut blocks: Vec<(u16, DataId, u32)> = Vec::new();
    for id in &store.ids_of(DbType::Lbi) {
        let Ok(bytes) = store.read_typed(DbType::Lbi, *id) else {
            continue;
        };
        let Ok(lbi) = LandblockInfo::decode_payload(*id, &bytes) else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let block = (id.0 >> 16) as u16;
        let mut n = 0u32;
        let mut first = DataId(0);
        for o in &lbi.objects {
            if ids.contains(&o.id.0) {
                n += 1;
                if first == DataId(0) {
                    first = o.id;
                }
            }
        }
        if n > 0 {
            blocks.push((block, first, n));
        }
    }
    blocks.sort_unstable();
    eprintln!(
        "SCALE placements: {} landblock(s) place the setup as a landblock-info static: {:X?}",
        blocks.len(),
        blocks.iter().take(40).collect::<Vec<_>>()
    );

    // The other two producers of a scripted static.
    let mut scenes = 0u32;
    for id in &store.ids_of(DbType::Scene) {
        let Ok(bytes) = store.read_typed(DbType::Scene, *id) else {
            continue;
        };
        let Ok(sc) = dereth_assets::Scene::decode_payload(*id, &bytes) else {
            continue;
        };
        for o in &sc.objects {
            if ids.contains(&o.obj_id.0) {
                scenes += 1;
            }
        }
    }
    let mut cells: Vec<(u32, u32, usize, DataId)> = Vec::new();
    for id in &store.ids_of(DbType::Cell) {
        let Ok(bytes) = store.read_typed(DbType::Cell, *id) else {
            continue;
        };
        let Ok(c) = dereth_assets::EnvCell::decode_payload(*id, &bytes) else {
            continue;
        };
        for (i, o) in c.static_objects.iter().enumerate() {
            if ids.contains(&o.id.0) {
                cells.push((id.0 >> 16, id.0, i, o.id));
            }
        }
    }
    cells.sort_unstable();
    eprintln!(
        "SCALE placements: {scenes} scene-record placement(s); {} environment-cell static(s), as (landblock, cell, index, setup): {:08X?}",
        cells.len(),
        cells
    );
    // Exactly one decoded environment-cell placement is required. Landblock-info and Scene
    // counts above are only reports; they are not zero-placement assertions in this test.
    assert_eq!(
        cells.len(),
        1,
        "the decoded environment-cell placements contain exactly one SCALE target"
    );
    assert_eq!(cells[0].0, 0x5655, "the one placement's landblock");

    // What that one object's script actually does, so the seam station's numbers are read out of
    // the dat rather than fitted.
    let bytes = store
        .read_typed(DbType::Setup, cells[0].3)
        .expect("the setup");
    let s = Setup::decode_payload(cells[0].3, &bytes).expect("decodes");
    let sb = store
        .read_typed(DbType::PhysicsScript, s.default_script_id)
        .expect("its script");
    let sc = PhysicsScript::decode_payload(s.default_script_id, &sb).expect("decodes");
    eprintln!(
        "SCALE target: setup {:08X} default_script {:08X} stable {:08X}, {} step(s): {:?}",
        cells[0].3 .0,
        s.default_script_id.0,
        s.default_stable_id.0,
        sc.script_data.len(),
        sc.script_data
            .iter()
            .map(|d| (d.hook.hook_type, d.hook.direction, d.hook.data.clone()))
            .collect::<Vec<_>>()
    );
}

/// Recount sound-table hook records across setup closures and check which setups declare a
/// default sound-table ID. Four of the 2,161 scripted setups name a table; intersecting them with
/// the setups that actually raise this hook leaves one.
#[test]
fn the_sound_table_hooks_ceiling_is_the_four_setups_that_name_a_table() {
    let store = store();
    let tabled = setups_raising(&store, SOUND_TABLE);
    let instances: u32 = tabled.iter().map(|(_, n)| *n).sum();
    let mut named = Vec::new();
    for (id, n) in &tabled {
        let Ok(bytes) = store.read_typed(DbType::Setup, *id) else {
            continue;
        };
        let Ok(s) = Setup::decode_payload(*id, &bytes) else {
            continue;
        };
        if s.default_stable_id != DataId(0) {
            named.push((id.0, s.default_stable_id.0, *n));
        }
    }
    eprintln!(
        "SOUND_TABLE census: {} setup record(s), {instances} hook instance(s); \
         {} of them name a default sound-table id: {:08X?}",
        tabled.len(),
        named.len(),
        named
    );
    assert_eq!(
        instances, 49,
        "the shipped scripted setups carry 49 SOUND_TABLE hooks"
    );
    // Of the four setups that name a sound table, only setup 0x0200171D both raises this hook
    // and declares default table 0x200000CA, with one counted hook. The
    // other 48 counted hooks lack that default-table route. This is not proof that a particular
    // sound_type resolves in the named table or that any audio is played.
    assert_eq!(
        named.len(),
        1,
        "exactly one SOUND_TABLE-raising setup names a table"
    );
    assert_eq!(named[0], (0x0200_171D, 0x2000_00CA, 1));

    // And where that one object stands, by the same three producers as the SCALE census.
    let one = DataId(named[0].0);
    let mut lbi_hits: Vec<u32> = Vec::new();
    for id in &store.ids_of(DbType::Lbi) {
        let Ok(bytes) = store.read_typed(DbType::Lbi, *id) else {
            continue;
        };
        let Ok(lbi) = LandblockInfo::decode_payload(*id, &bytes) else {
            continue;
        };
        if lbi.objects.iter().any(|o| o.id == one) {
            lbi_hits.push(id.0 >> 16);
        }
    }
    let mut scene_hits = 0u32;
    for id in &store.ids_of(DbType::Scene) {
        let Ok(bytes) = store.read_typed(DbType::Scene, *id) else {
            continue;
        };
        let Ok(sc) = dereth_assets::Scene::decode_payload(*id, &bytes) else {
            continue;
        };
        scene_hits += u32::try_from(sc.objects.iter().filter(|o| o.obj_id == one).count())
            .unwrap_or(u32::MAX);
    }
    let mut cell_hits: Vec<(u32, usize)> = Vec::new();
    for id in &store.ids_of(DbType::Cell) {
        let Ok(bytes) = store.read_typed(DbType::Cell, *id) else {
            continue;
        };
        let Ok(c) = dereth_assets::EnvCell::decode_payload(*id, &bytes) else {
            continue;
        };
        for (i, o) in c.static_objects.iter().enumerate() {
            if o.id == one {
                cell_hits.push((id.0, i));
            }
        }
    }
    eprintln!(
        "SOUND_TABLE target {:08X}: {} landblock-info record(s) {:X?}, {scene_hits} scene-record placement(s), {} environment-cell static(s) {:08X?}",
        one.0,
        lbi_hits.len(),
        lbi_hits,
        cell_hits.len(),
        cell_hits
    );
    // The sole setup with both a sound-table hook and default table has zero placements in
    // all three decoded static-source categories. Keep these zero gates explicit; a new
    // placement must fail rather than silently expanding an unexercised runtime path. Read or
    // decode failures are skipped by these scans, so completeness depends on readable assets.
    assert!(
        lbi_hits.is_empty(),
        "the one tabled setup record is now a landblock-info static"
    );
    assert_eq!(
        scene_hits, 0,
        "the one tabled setup is now generated scenery"
    );
    assert!(
        cell_hits.is_empty(),
        "the one tabled setup record is now an environment-cell static"
    );
}

// ---------------------------------------------------------------------------------------------
// Scene stations: a Holtburg or 0x5655 scene on a software device.
// ---------------------------------------------------------------------------------------------

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
mod scene {
    use dereth_client::world::{SceneReads, SceneWrites};
    use std::sync::Arc;

    use dereth_client::objects::ObjectStream;
    use dereth_client::world::{SceneConfig, WorldScene};
    use dereth_dat::RetailDatStore;
    use dereth_primitives::LocalTime;
    use dereth_render::device::Gpu;

    /// Landblock of the single decoded environment-cell SCALE placement asserted above.
    const SCALE_BLOCK: u16 = 0x5655;
    /// Reported cell and placement: environment-cell static entry 0 in cell 0x5655010D.
    const SCALE_CELL: u32 = 0x5655_010D;
    /// Setup 0x0200161A's default script 0x3300105B contains three recorded steps: immediate
    /// SCALE to 2.0, CREATE_PARTICLE 0x320008B6 and SOUND_TABLE 95. These metadata values explain
    /// the chosen scale target; the station does not assert particle creation or sound playback.
    const SCALE_SETUP: u32 = 0x0200_161A;

    fn scene(gpu: &mut Gpu, store: &Arc<RetailDatStore>, landblock: u16) -> WorldScene {
        let cfg = SceneConfig {
            landblock,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
        // Cell-static registration writes into the attached character's PhysicsWorld. Attach
        // one before streaming, as the application does, so collision bodies exist for the
        // later script-host pairing.
        let region = dereth_client::world::load_region(store).expect("region");
        scene.attach_character(store, &region, gpu).expect("body");
        // `stream` is the other place cell statics are registered, for blocks that arrive later;
        // `sync_objects` is where the hosts are spawned.
        scene.stream(store, gpu).expect("stream");
        scene
    }

    /// Behaviour: world.statics.a-scripted-statics-hooks-reach-its-own-collision-body
    ///
    /// Pairing observations in the default landblock window. Require at least 20 scripted
    /// hosts and at least one assigned handle. For probes whose handle resolves to a body,
    /// compare exact origins and cell IDs plus initial scale 1.0. Host and body origins derive
    /// from the same placement, so rounding tolerance is unnecessary.
    ///
    /// These comparisons catch spatially wrong pairings but do not prove unique object identity
    /// for coincident placements. A probe with no resolved body position skips this loop. The
    /// final check separately requires every missing handle to belong to a cell with no static
    /// handles at all; it does not establish why that cell registered no bodies.
    #[test]
    fn every_landblock_static_that_runs_a_script_carries_its_own_collision_body() {
        let mut gpu = crate::common::test_gpu(640, 480);
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let mut scene = scene(&mut gpu, &store, dereth_client::world::DEFAULT_LANDBLOCK);
        let mut stream = ObjectStream::new();
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");

        let probes = scene.host_bodies();
        let with = probes.iter().filter(|p| p.body.is_some()).count();
        let without: Vec<(u32, u32)> = probes
            .iter()
            .filter(|p| p.body.is_none())
            .map(|p| (p.cell.0, p.setup.0))
            .collect();
        eprintln!(
            "script-host pairing at Holtburg: {} scripted static(s), {with} with a collision body, \
             {} without (cell, setup): {:08X?}",
            probes.len(),
            without.len(),
            without
        );
        assert!(
            probes.len() >= 20,
            "only {} scripted statics in the window",
            probes.len()
        );

        // 1. Hosts are paired at all: an unpaired host answers `None` here, and
        //    `HostHookStats::no_body` counts its hooks.
        assert!(
            with > 0,
            "not one of the {} scripted statics was assigned a collision-body handle",
            probes.len()
        );

        // 2. Each resolved body position/cell agrees with its host's placement.
        for p in &probes {
            let Some(at) = p.body_at else { continue };
            assert_eq!(
                (at.x, at.y, at.z),
                (p.at.x, p.at.y, p.at.z),
                "the body paired with the static at cell {:08X} setup {:08X} stands somewhere \
                 else: {at:?} against {:?}",
                p.cell.0,
                p.setup.0,
                p.at
            );
            assert_eq!(
                p.body_cell.map(|c| c.0),
                Some(p.cell.0),
                "the body paired with the static at cell {:08X} setup {:08X} was added to a \
                 different cell",
                p.cell.0,
                p.setup.0
            );
            assert_eq!(
                p.scale,
                Some(1.0),
                "a placed static starts at the constructor's scale"
            );
        }

        // Every missing handle must be in a cell whose static-handle list is empty. Setup
        // decoding, residency and already-registered-cell handling can affect registration;
        // this aggregate check does not audit each initializer refusal or prove residency is
        // the sole cause. It does catch a host omitted while other statics in its cell registered.
        let unresolved = probes
            .iter()
            .filter(|p| p.body.is_none())
            .filter(|p| scene.cell_static_handles(p.cell).is_empty())
            .count();
        assert_eq!(
            without.len(),
            unresolved,
            "{} host(s) sit in a cell that did register bodies and still got none",
            without.len() - unresolved
        );
    }

    /// Behaviour: world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work
    ///
    /// The selected environment-cell static's immediate scale hook reaches its paired body.
    /// Four updates allow the script to run; compare scale and radius before/after and require
    /// a drained scale hook with no scale-hook missing-body count. Exact first-tick timing and
    /// physical movement/collision response are not measured here. Both probes select by cell
    /// and setup ID; they do not compare placement indices or explicitly assert handle identity.
    #[test]
    fn the_one_shipped_static_that_a_script_doubles_doubles_its_collision_body() {
        let mut gpu = crate::common::test_gpu(640, 480);
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let mut scene = scene(&mut gpu, &store, SCALE_BLOCK);
        let mut stream = ObjectStream::new();
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");

        let find = |s: &WorldScene| {
            s.host_bodies()
                .into_iter()
                .find(|p| p.cell.0 == SCALE_CELL && p.setup.0 == SCALE_SETUP)
                .expect("landblock 0x5655's cell 0x010D stands setup record 0x0200161A")
        };
        // Before advancing the host scripts, require the initial body scale and positive radius.
        let before = find(&scene);
        assert_eq!(before.scale, Some(1.0), "the placement starts unscaled");
        let r0 = before.radius.expect("the body has a collision radius");
        assert!(
            r0 > 0.0,
            "the shipped object is not intangible, or this station proves nothing"
        );

        // The immediate hook is queued for the initial script update. Allow four 30 Hz updates
        // through the scene's host loop; the final state does not identify the exact firing tick.
        for f in 1..=4 {
            scene.update(
                dereth_client::camera::CameraInput::default(),
                dereth_client::character::CharacterInput::default(),
                LocalTime(f64::from(f) / 30.0),
                1.0 / 30.0,
            );
        }
        let after = find(&scene);
        let h = scene.draw.stats.hosts;
        eprintln!(
            "scripted static scale: cell {SCALE_CELL:08X} setup {SCALE_SETUP:08X}: object scale {:?} -> \
             {:?}, radius {r0} -> {:?}; host hooks drained {} (scale {}, scale-no-body {}, \
             no-body {})",
            before.scale,
            after.scale,
            after.radius,
            h.drained,
            h.scale_hooks,
            h.scale_hooks_no_body,
            h.no_body
        );
        assert_eq!(
            after.scale,
            Some(2.0),
            "the scale hook must double the paired collision body's scale"
        );
        let r1 = after.radius.expect("still has a body");
        assert!(
            (r1 - 2.0 * r0).abs() < 1e-4,
            "the collision radius must follow the doubled scale: {r1} against {r0}"
        );
        assert_eq!(
            h.scale_hooks_no_body, 0,
            "the SCALE hook had nowhere to land"
        );
        assert!(
            h.scale_hooks >= 1,
            "the SCALE hook did not reach the drain at all"
        );
    }
}
