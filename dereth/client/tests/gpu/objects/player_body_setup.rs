//! The local player's body is the body the server named.
//!
//! The offline body is built from the Aluvian male setup (`0x02000001`). When the server's
//! physics description names a different setup, the body is rebuilt from that setup before the
//! description is applied: an `AnimPartChange` names a part **index**, and an index into a
//! different setup dresses the wrong limb. The corpus's players use two setups, `0x02000001` and
//! the human female `0x0200004E`; a client that kept the constant would drop the female players'
//! whole description and leave them in the loincloth of default setup creation.
//!
//! In the world path the only authority for the setup id is the physics-description `setup_id`
//! field on the wire. (Character generation computes the id instead: the sex's setup, or the hair
//! style's alternate, with the heritage group's setup, `0x02000054` for all thirteen heritages, as
//! the fallback while heritage or gender is unchosen.)
//!
//! Fixture: every recorded session, replayed through the client's network and object stream, plus
//! the retail dats. Every setup id, part index, part id and count asserted here is read off the
//! wire or out of the dat at run time.
//!
//! What is asserted:
//!
//! 1. **The census**: which setup record each capture's player uses, how many part and texture
//!    changes he carries, and that the corpus is not all one setup.
//! 2. **The drawn body**, per capture: `objdesc_setup_mismatch` is zero, the body's part array *is*
//!    the server's setup, and the `GfxObj` ids the meshes on the device were **baked from** carry
//!    exactly the capture's `part_index -> part_id` map, with every unnamed index still the
//!    server's setup's own. A part array that matches the map at the *model* level alone can pass
//!    while the drawn body stays wrong.
//! 3. **The texture half on the device**: the descriptor-heap slots the body's batches bind, before
//!    and after the description, in **one** scene so the slots are comparable.
//! 4. **The `0x0200004E` characters are dressed**: a paired before/after compared in memory, with
//!    the changed pixels confined to the body.

#![cfg(gpu)]

use super::common::{
    addr, connection_sequence_number, corpus_sessions, load, retail_store, test_gpu,
};
use crate::common::recorded_world_sessions;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::{Decode, Setup};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::objects::physics_state::HIDDEN_PS;
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::landblock::load_region, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

// ---------------------------------------------------------------------------------------------
// The replay: the capture through the client's network and object stream.
// ---------------------------------------------------------------------------------------------

struct Replayed {
    objects: ObjectStream,
    /// The index of the last datagram after which the client still held objects. **Every recording
    /// ends with a clean logout** and the end-of-session teardown empties the object model, so a
    /// census taken at the end of the replay would count nothing; it is taken while the objects
    /// exist.
    last_populated: usize,
    player: Option<(ObjectId, dereth_client_runtime::objects::Presence)>,
}

fn replay_upto(session: &str, limit: usize) -> Replayed {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut last_populated = 0usize;
    let mut player = None;
    for (index, r) in records.iter().enumerate() {
        if index >= limit {
            break;
        }
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        if !objects.is_empty() {
            last_populated = index;
        }
        if player.is_none() {
            if let Some(id) = objects.player() {
                if let Some(p) = objects.presence(id) {
                    player = Some((id, p.clone()));
                }
            }
        }
    }
    Replayed {
        objects,
        last_populated,
        player,
    }
}

/// The whole session up to the point the client is still in world, which is where every appearance
/// the server sent is live.
fn in_world(session: &str) -> Replayed {
    let last = replay_upto(session, usize::MAX).last_populated;
    replay_upto(session, last + 1)
}

/// The captures whose character actually reaches the world, **measured rather than named**: as
/// many as the corpus records a `0x0013` for. A login-only capture (the account authenticates and
/// disconnects without a character entering the world) is left out.
fn world_sessions() -> &'static [String] {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let world: Vec<String> = corpus_sessions()
            .into_iter()
            .filter(|s| replay_upto(s, usize::MAX).last_populated != 0)
            .collect();
        assert_eq!(
            world.len(),
            recorded_world_sessions(),
            "the captures that enter the world, as many as carry a recorded 0x0013"
        );
        world
    })
}

fn setup_of(store: &RetailDatStore, id: DataId) -> Setup {
    let bytes = store
        .read_typed(DbType::Setup, id)
        .unwrap_or_else(|e| panic!("{id:?}: {e}"));
    Setup::decode_payload(id, &bytes).unwrap_or_else(|e| panic!("{id:?}: {e}"))
}

/// **The client's graphics-array loading map.**
///
/// The client does not bake a part's own `gfxobj_id`. It loads that graphics object by id, follows
/// its `did_degrade` to a `GfxObjDegradeInfo`, and then fills
/// `gfxobj[i] = degrades[i].gfxobj_id`, so **`gfxobj[0]` is the record's level 0 and not the id
/// the part carries**, and only a part with no record at all draws its own id. 229 of the 4,131
/// shipped records name a level-0 mesh that is not the object pointing at them; on setup
/// `0x02000001` it is **16 of 34 parts**.
///
/// The assertions in this file that read [`WorldScene::character_built_from`] therefore compare it
/// against the setup's part list mapped through this function, not against the part list itself:
/// the expectation performs the client's own mapping.
///
/// `BakeCache::degrade_record`'s rule is reproduced exactly: a record with a **single** level
/// cannot switch and is treated as no record, so such a part still bakes its own id.
fn baked_ids(store: &RetailDatStore, parts: &[DataId]) -> Vec<DataId> {
    parts
        .iter()
        .map(|id| {
            let level0 = (|| {
                let bytes = store.read_typed(DbType::GfxObj, *id).ok()?;
                let obj = dereth_assets::GfxObj::decode_payload(*id, &bytes).ok()?;
                let did = obj.did_degrade?;
                let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
                let info = dereth_assets::GfxObjDegradeInfo::decode_payload(did, &bytes).ok()?;
                (info.degrades.len() > 1).then(|| info.degrades[0].gfxobj_id)
            })();
            level0.unwrap_or(*id)
        })
        .collect()
}

/// The `part_index -> part_id` map an `ObjDesc` ends up asking for. The wire may name an index more
/// than once and part replacement walks the list in order, so the last entry wins.
fn swap_map(od: &dereth_protocol::types::ObjDesc) -> BTreeMap<u32, DataId> {
    od.anim_part_changes
        .iter()
        .map(|c| (u32::from(c.part_index), DataId(c.part_id)))
        .collect()
}

/// The part indices an `ObjDesc`'s **texture** changes touch. A texture change never moves a part's
/// `GfxObj`, which is exactly why `built_from` cannot see it.
fn texture_indices(od: &dereth_protocol::types::ObjDesc) -> BTreeSet<u32> {
    od.texture_changes
        .iter()
        .map(|c| u32::from(c.part_index))
        .collect()
}

/// A stream holding **only** the `0xF746`, the player's own `0xF745` and the first `0xF74B` that
/// clears the player's hidden bit, so nothing but the body is on screen. The same as
/// `objects::part_swaps`'s, for the same reasons: a scene shot before and after a full
/// `sync_objects` differs by the weather, the sky and seventy other objects and would prove
/// nothing; and the `0xF745` creates the player hidden (state `0x00404410`, as the login leaves it
/// in portal space) until the server's `0xF74B` (`0x00400408`) after the client's login-complete,
/// so without that message the dressed body would never be drawn.
fn player_only_stream(session: &str) -> ObjectStream {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut player: Option<ObjectId> = None;
    let mut kept: Vec<(SessionEvent, LocalTime)> = Vec::new();
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            match &e {
                SessionEvent::CharacterSet(set) => {
                    if !entered {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                }
                SessionEvent::PlayerCreated(id) => {
                    player = Some(*id);
                    kept.push((e.clone(), now));
                }
                SessionEvent::WorldObject { opcode, body }
                    if *opcode == dereth_protocol::Opcode::ITEM_CREATE_OBJECT
                        && dereth_protocol::read_body_padded::<ItemCreateObject>(body)
                            .ok()
                            .is_some_and(|m| Some(m.0.id) == player) =>
                {
                    kept.push((e.clone(), now));
                }
                // The login creates the player hidden, in portal space; the server clears the
                // hidden bit with its first `0xF74B` for the player once the client says the
                // login is complete. Without it the body stays hidden and draws nothing.
                SessionEvent::WorldObject { opcode, body }
                    if kept.len() == 2
                        && *opcode == dereth_protocol::Opcode::ITEM_SET_STATE
                        && dereth_protocol::read_body_padded::<ItemSetState>(body)
                            .ok()
                            .is_some_and(|m| Some(m.id) == player && m.state & HIDDEN_PS == 0) =>
                {
                    kept.push((e.clone(), now));
                }
                _ => {}
            }
        }
        if kept.len() >= 3 {
            break;
        }
    }
    assert_eq!(
        kept.len(),
        3,
        "{session}: expected the 0xF746, the player's own 0xF745 and the 0xF74B that shows it, \
         got {} event(s)",
        kept.len()
    );
    let mut solo = ObjectStream::new();
    for (e, now) in &kept {
        solo.apply_event(e, *now);
    }
    assert!(
        solo.player().is_some(),
        "{session}: the solo stream has no player"
    );
    assert_eq!(
        solo.len(),
        1,
        "{session}: the solo stream holds more than the player"
    );
    solo
}

// ---------------------------------------------------------------------------------------------
// Frames.
// ---------------------------------------------------------------------------------------------

fn settle(scene: &mut WorldScene, from: u32) {
    for i in from..from + 8 {
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(f64::from(i) * 0.05),
            0.05,
        );
    }
    scene.follow_character_now();
}

type Frame = (Vec<u8>, u32, u32);

fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Frame {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let (w, h) = (image.width, image.height);
    (image.to_rgba(), w, h)
}

fn diff(a: &Frame, b: &Frame) -> (usize, (u32, u32, u32, u32)) {
    assert_eq!((a.1, a.2), (b.1, b.2));
    let (w, h) = (a.1, a.2);
    let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a.0[i..i + 4] != b.0[i..i + 4] {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (n, (x0, y0, x1, y1))
}

// ---------------------------------------------------------------------------------------------
// 1. The census.
// ---------------------------------------------------------------------------------------------

/// **Which body is each capture's player?**
///
/// The rest of this module rests on one fact: the corpus's players are **not** all Aluvian males.
/// If they were, a body built from the constant would be indistinguishable from one built from the
/// wire, and a session played on an Aluvian male shows nothing wrong either way.
#[test]
fn the_corpus_players_are_not_all_one_setup() {
    let store = retail_store();
    let mut setups: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for name in world_sessions() {
        let r = in_world(name);
        let (id, p) = r
            .player
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no player"));
        let setup_id = p
            .setup_id
            .unwrap_or_else(|| panic!("{name}: the player has no setup record"));
        let s = setup_of(&store, setup_id);
        let parts = s.parts.len();
        setups.entry(setup_id.0).or_default().push(name.clone());
        eprintln!(
            "  setup record {:#010X}: height {:.4}, radius {:.4}, step up {:.4} / down {:.4}, \
             {} sphere(s), {} cylsphere(s), default motion table {:#010X}, default sound table {:#010X}",
            setup_id.0,
            s.height,
            s.radius,
            s.step_up_height,
            s.step_down_height,
            s.spheres.len(),
            s.cylspheres.len(),
            s.default_mtable_id.0,
            s.default_stable_id.0,
        );
        eprintln!(
            "{name}: player {:#010X} uses setup record {:#010X} ({parts} parts), motion table {}, \
             scale {:.3} -- {} part change(s) over {} distinct index(es), {} texture change(s) \
             over {} index(es), {} sub-palette(s)",
            id.0,
            setup_id.0,
            p.mtable_id
                .map_or_else(|| "none".to_owned(), |m| format!("{:#010X}", m.0)),
            p.scale,
            p.objdesc.anim_part_changes.len(),
            swap_map(&p.objdesc).len(),
            p.objdesc.texture_changes.len(),
            texture_indices(&p.objdesc).len(),
            p.objdesc.subpalettes.len(),
        );
    }
    for (setup, sessions) in &setups {
        eprintln!("  setup record {setup:#010X}: {}", sessions.join(", "));
    }
    assert!(
        setups.len() > 1,
        "every capture's player uses setup record {:#010X}, so this corpus cannot tell a body built from \
         the wire from a body built from a constant",
        setups.keys().next().copied().unwrap_or_default()
    );
    assert!(
        setups
            .keys()
            .any(|s| *s != dereth_client_runtime::character::ALUVIAN_MALE_SETUP.0),
        "no capture's player is anything but the Aluvian male"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Per capture, on the drawn geometry.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-players-body-is-built-from-the-setup-the-server-named
///
/// **The body on the device is the body the server named, dressed as the server described it.**
///
/// Run over **every** in-world capture rather than the one whose player happens to be an Aluvian
/// male. The assertion that carries the test is on [`WorldScene::character_built_from`], the
/// `GfxObj` id each *baked mesh* was built from, recorded inside `build_part_meshes` as it read the
/// array, and **not** on `part_array.parts[i].gfxobj_id`, which is the model: a model assertion
/// can pass while the drawn geometry stays wrong.
///
/// Each session gets its own device: `early-inventory-and-casting` alone uploads 1,174 textures
/// against a 2,048-descriptor heap, and dropping a `WorldScene` returns none of them.
#[test]
fn every_captures_body_is_built_from_the_setup_the_server_named() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let (mut checked, mut rebuilt_sessions) = (0usize, 0usize);
    let (mut total_swaps, mut total_applied) = (0usize, 0usize);
    for name in world_sessions() {
        let mut gpu = test_gpu(640, 640);
        let r = in_world(name);
        let (_, p) = r
            .player
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no player"))
            .clone();
        let pos = p
            .position
            .unwrap_or_else(|| panic!("{name}: the player has no position"));
        let setup_id = p
            .setup_id
            .unwrap_or_else(|| panic!("{name}: the player has no setup record"));
        let expected = swap_map(&p.objdesc);
        let base = setup_of(&store, setup_id).parts;
        let block = pos.cell.landblock();
        let cfg = SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: true,
            land_radius: 0,
            scenery_radius: 0,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");

        // Before: setup creation alone, from the offline constant. The undressed textures are read
        // here, in **this** scene, so the slots below are allocated out of one cache and are
        // therefore comparable.
        let undressed_built_from = scene.character_built_from().to_vec();
        let undressed_textures = scene.character_part_textures();
        let undressed_triangles = scene.draw.stats.character_triangles;
        assert_eq!(
            scene.character.as_ref().expect("a body").setup_id(),
            dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
            "{name}: the offline body is not built from the documented constant"
        );
        assert_eq!(
            undressed_built_from,
            baked_ids(
                &store,
                &setup_of(&store, dereth_client_runtime::character::ALUVIAN_MALE_SETUP).parts
            ),
            "{name}: the offline body's baked parts are not what the graphics-object loading map builds from the \
             Aluvian male's own"
        );
        assert!(
            scene
                .character
                .as_ref()
                .expect("a body")
                .driver()
                .part_array
                .parts
                .iter()
                .all(|x| x.surface_overrides.is_none()),
            "{name}: a fresh part array already carries surface overrides"
        );

        let mut solo = player_only_stream(name);
        scene
            .sync_objects(&store, &mut gpu, &mut solo)
            .expect("objects sync");
        assert_eq!(
            scene.server_object_count(),
            0,
            "{name}: the solo stream drew a SceneObject; the player must have no second body"
        );

        let s = scene.draw.stats;
        // The server's setup was accepted.
        assert_eq!(
            s.objdesc_setup_mismatch, 0,
            "{name}: the server's setup record {:#010X} was refused, so the whole description was \
             dropped",
            setup_id.0
        );
        assert!(
            s.objdescs_applied > 0,
            "{name}: not one description was applied"
        );
        assert_eq!(
            s.objdesc_failures, 0,
            "{name}: object-description application reported a failure"
        );
        assert_eq!(
            s.palette_range_failures, 0,
            "{name}: a sub-palette range was refused"
        );
        assert_eq!(s.palette_missing, 0, "{name}: a shift palette had no base");

        let body = scene.character.as_ref().expect("a body");
        assert_eq!(
            body.setup_id(),
            setup_id,
            "{name}: the body is built from {:#010X} and the server named {:#010X}",
            body.setup_id().0,
            setup_id.0
        );
        // Three states, not two: a rebuild that happened, a rebuild that was not needed because the
        // server named the setup the body already had, and a body that was never asked. The last
        // is indistinguishable from the second unless the counter counts *rebuilds*.
        let want_rebuild =
            u64::from(setup_id != dereth_client_runtime::character::ALUVIAN_MALE_SETUP);
        assert_eq!(
            body.stats.setup_changes, want_rebuild,
            "{name}: {} rebuild(s) for a server setup of {:#010X}",
            body.stats.setup_changes, setup_id.0
        );
        if want_rebuild == 1 {
            rebuilt_sessions += 1;
        }

        // ---- the drawn geometry ------------------------------------------------------------
        let drawn = scene.character_built_from().to_vec();
        assert_eq!(
            drawn.len(),
            base.len(),
            "{name}: the device holds {} baked parts and the setup record {:#010X} has {}",
            drawn.len(),
            setup_id.0,
            base.len()
        );
        let model: Vec<DataId> = scene
            .character
            .as_ref()
            .expect("a body")
            .driver()
            .part_array
            .parts
            .iter()
            .map(|x| x.gfxobj_id)
            .collect();
        // The device holds the *record's* level 0 for a part that has one, so the model's ids go
        // through the graphics-object loading map before they are compared (see [`baked_ids`]):
        // the mapping is asserted as well as the swap.
        assert_eq!(
            drawn,
            baked_ids(&store, &model),
            "{name}: the geometry on the device is not the geometry the part array names -- a \
             description landed after the bake"
        );
        let bake1 = |id: DataId| baked_ids(&store, std::slice::from_ref(&id))[0];
        let (mut swapped, mut kept) = (0usize, 0usize);
        for (i, &got) in drawn.iter().enumerate() {
            let idx = u32::try_from(i).expect("a part index fits a u32");
            match expected.get(&idx) {
                Some(&want) => {
                    assert_eq!(
                        got,
                        bake1(want),
                        "{name}: part {i}: the capture asked for {want:?} and the device drew \
                         {got:?}"
                    );
                    swapped += 1;
                }
                None => {
                    assert_eq!(
                        got,
                        bake1(base[i]),
                        "{name}: part {i} was not named by the description and must still be \
                         the setup record {:#010X}'s own part",
                        setup_id.0
                    );
                    kept += 1;
                }
            }
        }
        assert_eq!(
            swapped,
            expected.len(),
            "{name}: every named part was checked"
        );
        total_swaps += expected.len();
        total_applied += swapped;

        // ---- the texture half, also on the device ------------------------------------------
        let dressed_textures = scene.character_part_textures();
        assert_eq!(
            dressed_textures.len(),
            drawn.len(),
            "{name}: one texture-slot list per baked part"
        );
        let named = texture_indices(&p.objdesc);
        let changed_slots: BTreeSet<u32> = (0..drawn.len())
            .filter(|&i| {
                undressed_textures.get(i).map(Vec::as_slice)
                    != dressed_textures.get(i).map(Vec::as_slice)
            })
            .map(|i| u32::try_from(i).expect("a part index fits a u32"))
            .collect();
        let overrides = scene
            .character
            .as_ref()
            .expect("a body")
            .driver()
            .part_array
            .parts
            .iter()
            .filter(|x| x.surface_overrides.is_some())
            .count();
        assert!(
            overrides > 0,
            "{name}: not one of the body's parts carries a surface override after the description"
        );
        assert!(
            !changed_slots.is_empty(),
            "{name}: every part binds the same texture dressed as undressed, so the {} texture \
             change(s) and {} sub-palette(s) never reached the device",
            p.objdesc.texture_changes.len(),
            p.objdesc.subpalettes.len()
        );
        let dressed_triangles = scene.draw.stats.character_triangles;

        eprintln!(
            "{name}: setup record {:#010X}, {} parts -- {swapped} of {} part swap(s) applied on the \
             drawn geometry, {kept} left as the setup's; {} of {} texture-change index(es) among \
             the {} part(s) whose bound texture changed; {overrides} of {} parts carry a surface \
             override; {undressed_triangles} triangles undressed, {dressed_triangles} dressed",
            setup_id.0,
            base.len(),
            expected.len(),
            named.iter().filter(|i| changed_slots.contains(i)).count(),
            named.len(),
            changed_slots.len(),
            drawn.len(),
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        world_sessions().len(),
        "every in-world capture was checked"
    );
    assert!(
        rebuilt_sessions > 0,
        "no capture required a rebuild, so this test cannot see the behaviour it exists for"
    );
    eprintln!(
        "player body setup: {checked} in-world captures, {rebuilt_sessions} of them needing a body rebuilt \
         from the server's setup record; {total_applied} of {total_swaps} player part swaps land on the \
         drawn geometry"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The picture: the `0x0200004E` characters get dressed.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-players-body-is-built-from-the-setup-the-server-named
///
/// **The visible half: a non-Aluvian player is dressed, not naked.**
///
/// Two scenes rather than two moments of one, as in `objects::part_swaps`: a single scene shot
/// before and after `sync_objects` differs by everything the clock moved in between (the rain, the
/// sky, the body's own idle cycle), and that is 44 % of the frame. Both scenes are built
/// identically, settled for the same eight ticks with the weather off, and the dressed one is
/// synchronised against a stream holding only the `0xF746` and the player's own `0xF745`.
///
/// **The survivor control applies here in reverse**: the "before" body is
/// the Aluvian male standing in the default setup's loincloth and the "after" body is a *different
/// setup entirely*, so a pixel difference is guaranteed and proves little on its own. What is
/// asserted alongside it is that the after-body's baked parts are the female setup's, dressed.
#[test]
fn the_non_aluvian_players_are_dressed_rather_than_left_in_the_loincloth() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let mut sessions = 0usize;
    for name in world_sessions() {
        let r = in_world(name);
        let (_, p) = r
            .player
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no player"))
            .clone();
        let setup_id = p.setup_id.expect("the player has a setup record");
        if setup_id == dereth_client_runtime::character::ALUVIAN_MALE_SETUP {
            continue;
        }
        sessions += 1;
        let mut gpu = test_gpu(640, 640);
        let pos = p.position.expect("the player has a position");
        let block = pos.cell.landblock();
        let cfg = SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: true,
            land_radius: 0,
            scenery_radius: 0,
            ..SceneConfig::default()
        };
        let expected = swap_map(&p.objdesc);
        let female = setup_of(&store, setup_id).parts;
        let male = setup_of(&store, dereth_client_runtime::character::ALUVIAN_MALE_SETUP).parts;

        let mut render = |dressed: bool| -> (
            Frame,
            usize,
            Vec<DataId>,
            (bool, i32, dereth_animation::MotionCommand),
        ) {
            let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
            scene.set_weather_enabled(false);
            scene
                .attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            // **The body is stood on the ground before the server's description arrives.**
            //
            // The player's own `0xF745` movement buffer carries `current_style = NonCombat,
            // forward_command = Ready`, and `apply_player_movement` applies it through the
            // interpreted-movement path, whose **first** branch is
            // `contact_allows_move(forward_command)`: with no ground contact it plays `Falling`
            // instead of the forward command. Synchronised on the frame the body is created,
            // `Character::on_ground()` reads **false straight off the physics object** (nothing
            // has stepped it yet), so the dressed arm would go off into `Falling` and link back out
            // of it while the un-synchronised arm sat in its default-initialization animation:
            // frame 10 against 12, the pose difference the assertion below forbids.
            //
            // Eight ticks first put the body in contact, which is also the only state a real
            // session can deliver a player's `0xF745` in: the body is created at enter-world and
            // the description arrives frames later, on the ground. A motion command on an
            // unchanged state then changes nothing (node list, `curr`, `first_cyclic` and
            // `frame_number` all unchanged), so both arms stay on the same animation. Both arms
            // take the same sixteen ticks.
            settle(&mut scene, 0);
            if dressed {
                let mut solo = player_only_stream(name);
                scene
                    .sync_objects(&store, &mut gpu, &mut solo)
                    .expect("objects sync");
                assert_eq!(
                    scene.draw.stats.objdesc_setup_mismatch, 0,
                    "{name}: the description was refused wholesale"
                );
            }
            settle(&mut scene, 8);
            let parts = scene.character_built_from().to_vec();
            let triangles = scene.draw.stats.character_triangles;
            let motion = scene.character_motion().expect("a body");
            (shot(&mut scene, &mut gpu), triangles, parts, motion)
        };

        let (before, before_triangles, undressed, before_motion) = render(false);
        let (after, after_triangles, drawn, after_motion) = render(true);

        // **The survivor control, applied before the pixels are counted.** Two frames can differ
        // by *pose* rather than by gear: rebuilding the body calls
        // `MotionDriver::set_motion_table`, whose `enter_default_state` restarts the sequence, and
        // two bodies can settle to different frames of the idle (10 against 12) with the arms
        // visibly up in one of them. Both bodies must therefore be at the **same animation
        // frame** before a pixel differential means anything about clothing.
        assert_eq!(
            before_motion, after_motion,
            "{name}: the rebuilt body is at a different point in its animation than the body that \
             was not rebuilt, so a pixel differential between them is about pose, not gear"
        );
        assert!(
            before_motion.0,
            "{name}: no animation is playing, so the body is in the setup record's \
             placement frame rather than its idle"
        );

        // The graphics-object loading map, applied to the expectation. See [`baked_ids`].
        let male_baked = baked_ids(&store, &male);
        assert_eq!(
            undressed, male_baked,
            "{name}: the before-body is not what the graphics-object loading map builds from the Aluvian male's \
             own parts"
        );
        assert_ne!(
            drawn, male_baked,
            "{name}: the after-body still draws the Aluvian male's parts, so the server's setup record \
             never reached the device"
        );
        // How much of the drawn body is the female setup's own, and how much is clothing.
        let dressed_parts = (0..drawn.len())
            .filter(|i| {
                expected
                    .get(&u32::try_from(*i).expect("a part index fits a u32"))
                    .is_some_and(|w| *w != female[*i])
            })
            .count();
        assert!(
            dressed_parts > 0,
            "{name}: every part the description named is an id that setup record {:#010X} already carries, \
             so this character wears nothing and the test is about the wrong thing",
            setup_id.0
        );

        let (n, bbox) = diff(&before, &after);
        let total = (before.1 * before.2) as usize;
        assert!(n > 0, "{name}: the body changed no pixel at all");
        assert!(
            n * 8 < total,
            "{name}: {n} of {total} pixels changed -- the two frames hold one body and nothing \
             else that can move"
        );
        eprintln!(
            "{name}: setup record {:#010X} ({} parts) replaces {:#010X} ({} parts); {dressed_parts} \
             part(s) of the drawn body are clothing rather than the setup's own; \
             {before_triangles} -> {after_triangles} triangles; {n} of {total} pixels differ, \
             bounding box {bbox:?}",
            setup_id.0,
            female.len(),
            dereth_client_runtime::character::ALUVIAN_MALE_SETUP.0,
            male.len(),
        );
    }
    assert!(
        sessions > 0,
        "no capture's player is anything but the Aluvian male, so this test proved nothing"
    );
    eprintln!(
        "player body setup: {sessions} capture(s) whose player needed a different setup record, all dressed"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The guards on the rebuild itself.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-players-body-is-built-from-the-setup-the-server-named
///
/// A second description naming the setup the body already has must **not** rebuild it.
///
/// Description changes from defaults restore palettes only, and the unequip in
/// `objects::part_swaps` depends on the surviving surface state; a rebuild between two descriptions
/// would wipe it and the boots' texture would come off for the wrong reason. The client re-offers
/// the player's id through `created` on every `0xF625 Item_ObjDescEvent`, so this path is walked on
/// every equip.
#[test]
fn re_offering_the_same_setup_does_not_rebuild_the_body() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let name = world_sessions()
        .iter()
        .find(|n| {
            in_world(n)
                .player
                .as_ref()
                .and_then(|(_, p)| p.setup_id)
                .is_some_and(|s| s != dereth_client_runtime::character::ALUVIAN_MALE_SETUP)
        })
        .cloned()
        .expect("a capture whose player is not an Aluvian male");
    let mut gpu = test_gpu(640, 640);
    let r = in_world(&name);
    let (_, p) = r.player.as_ref().expect("a player").clone();
    let pos = p.position.expect("a position");
    let setup_id = p.setup_id.expect("a setup record");
    let block = pos.cell.landblock();
    let cfg = SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let mut solo = player_only_stream(&name);
    scene
        .sync_objects(&store, &mut gpu, &mut solo)
        .expect("first sync");
    let after_first = scene
        .character
        .as_ref()
        .expect("a body")
        .stats
        .setup_changes;
    assert_eq!(
        after_first, 1,
        "{name}: the first description did not rebuild the body"
    );
    let built = scene.character_built_from().to_vec();

    // Re-offer the same player, exactly as a `0xF625` does.
    let mut again = player_only_stream(&name);
    scene
        .sync_objects(&store, &mut gpu, &mut again)
        .expect("second sync");
    let body = scene.character.as_ref().expect("a body");
    assert_eq!(
        body.stats.setup_changes, 1,
        "{name}: a second description naming the same setup record rebuilt the body again"
    );
    assert_eq!(body.setup_id(), setup_id);
    assert_eq!(
        scene.character_built_from(),
        built.as_slice(),
        "{name}: the second description changed the drawn geometry"
    );
    assert_eq!(
        scene.draw.stats.objdesc_setup_mismatch, 0,
        "{name}: a description was refused"
    );
    eprintln!(
        "{name}: two descriptions for setup record {:#010X}, {} rebuild(s), {} baked part(s) unchanged",
        setup_id.0,
        after_first,
        built.len()
    );
}

/// Behaviour: objects.appearance.the-players-body-is-built-from-the-setup-the-server-named
///
/// A setup id the dat does not hold must leave the body **exactly as it was** and be counted,
/// not half-applied.
///
/// This is the third state `objdesc_setup_mismatch` measures: the counter fires only for a setup
/// that genuinely will not build, never for a non-Aluvian player.
#[test]
fn a_setup_the_dat_does_not_hold_leaves_the_body_alone() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let mut gpu = test_gpu(640, 640);
    let cfg = SceneConfig {
        landblock: dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    let before = scene.character_built_from().to_vec();
    assert!(!before.is_empty(), "the body baked no parts at all");

    let missing = DataId(0x0200_FFFF);
    assert!(
        store.read_typed(DbType::Setup, missing).is_err(),
        "{missing:?} is in the retail dat after all -- pick another absent id"
    );
    let body = scene.character.as_mut().expect("a body");
    let err = body
        .set_setup_id(missing, None)
        .expect_err("a missing setup record must not build");
    assert_eq!(
        body.setup_id(),
        dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
        "the body took a setup that does not exist"
    );
    assert_eq!(
        body.stats.setup_changes, 0,
        "a failed rebuild was counted as a rebuild"
    );
    assert_eq!(
        scene.character_built_from(),
        before.as_slice(),
        "the failed rebuild changed the drawn geometry"
    );
    eprintln!("a missing setup record is refused with `{err}` and the body is untouched");
}

// ---------------------------------------------------------------------------------------------
// 5. `objdesc_setup_mismatch` over the whole corpus, on the whole stream.
// ---------------------------------------------------------------------------------------------

/// **No player description is refused, counted over every capture and every description.**
///
/// The other tests drive a *solo* stream — the `0xF746` and the player's own `0xF745` — so that a
/// frame differential has one moving thing in it. That is one description per session. This one
/// replays the whole recording and syncs the **whole** object stream against a body, so every
/// `0xF625 Item_ObjDescEvent` the player received is offered too: `ObjectStream` re-offers the id
/// through `created` on each one, exactly as a recreate does, so the count below is descriptions
/// and not sessions.
///
/// Each session gets its own device: one session can upload over a thousand textures
/// (`early-inventory-and-casting` uploads 1,174) against a 2,048-descriptor heap that nothing
/// reclaims.
#[test]
fn no_player_description_in_the_corpus_is_refused() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let (mut mismatches, mut applied, mut sessions) = (0u64, 0u64, 0usize);
    let mut refused: Vec<(String, DataId, DataId, u64)> = Vec::new();
    for name in world_sessions() {
        let mut gpu = test_gpu(640, 640);
        let mut r = in_world(name);
        let (_, p) = r
            .player
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no player"))
            .clone();
        let pos = p.position.expect("the player has a position");
        let setup_id = p.setup_id.expect("the player has a setup record");
        let block = pos.cell.landblock();
        let cfg = SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: true,
            land_radius: 0,
            scenery_radius: 0,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        scene
            .sync_objects(&store, &mut gpu, &mut r.objects)
            .expect("objects sync");

        let s = scene.draw.stats;
        assert_eq!(
            s.objdesc_failures, 0,
            "{name}: object-description application reported a failure"
        );
        // The body ends as the server's, whatever the last description said.
        let body = scene.character.as_ref().expect("a body");
        assert!(
            body.stats.setup_changes <= 1,
            "{name}: the body was rebuilt {} times for one setup",
            body.stats.setup_changes
        );
        // The drawn geometry still agrees with the model after the *last* description, which is
        // what the incremental path (the unequip in `objects::part_swaps`) can break.
        let model: Vec<DataId> = body
            .driver()
            .part_array
            .parts
            .iter()
            .map(|x| x.gfxobj_id)
            .collect();
        // Through the graphics-object loading map. See [`baked_ids`].
        assert_eq!(
            scene.character_built_from(),
            baked_ids(&store, &model).as_slice(),
            "{name}: the device and the part array disagree after the last description"
        );
        mismatches += s.objdesc_setup_mismatch;
        applied += s.objdescs_applied;
        sessions += 1;
        // **Reported per session and asserted once, at the end.** An `assert_eq!` inside the loop
        // aborts on the first capture, which means a run that is going to fail publishes one
        // number instead of the whole selected set -- and the shape of the failure (all captures,
        // or only the non-Aluvian ones) is the whole diagnosis.
        eprintln!(
            "{name}: setup record {:#010X}, body uses {:#010X}, {} description(s) applied over the whole \
             stream, {} refused, {} rebuild(s)",
            setup_id.0,
            body.setup_id().0,
            s.objdescs_applied,
            s.objdesc_setup_mismatch,
            body.stats.setup_changes,
        );
        refused.push((
            name.clone(),
            setup_id,
            body.setup_id(),
            s.objdesc_setup_mismatch,
        ));
    }
    assert_eq!(
        sessions,
        world_sessions().len(),
        "every in-world capture was measured"
    );
    assert!(
        applied > 0,
        "not one description was applied anywhere in the corpus"
    );
    for (name, want, got, n) in &refused {
        assert_eq!(
            got, want,
            "{name}: the body ended with setup record {:#010X} and the server named {:#010X}",
            got.0, want.0
        );
        assert_eq!(
            *n, 0,
            "{name}: {n} player description(s) refused because the body could not be rebuilt with \
             setup record {:#010X}",
            want.0
        );
    }
    assert_eq!(
        mismatches, 0,
        "the corpus refused {mismatches} description(s)"
    );
    eprintln!(
        "player body setup: objdesc_setup_mismatch = {mismatches} over {sessions} capture(s) and {applied} \
         applied description(s)"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. The collision half, and the motion table, which the corpus cannot witness.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-players-body-is-built-from-the-setup-the-server-named
///
/// **`set_setup_id` replaces the body's collision half too, not only its part array.**
///
/// Setup installation re-caches whether the object has a physics BSP after installing the new
/// array, and every collision reads height and radius from the setup. A rebuild that changed the
/// drawn parts and left the spheres behind would therefore be invisibly wrong.
///
/// **The capture corpus cannot see this and it is important to say so.** The two setup records its
/// players use, `0x02000001` and `0x0200004E`, carry an *identical* collision half — height 1.8350,
/// radius 0.6788, step up 0.6 / down 1.5, two spheres, no cylspheres — and both carry
/// `default_mtable_id = 0` and `default_stable_id = 0`. So the fixture here is the retail dat
/// rather than the wire: the first setup **found by scanning** whose height and radius both differ
/// from the Aluvian male's. Nothing about it is written into this file.
#[test]
fn a_rebuilt_body_takes_the_new_setups_collision_half() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let male = setup_of(&store, dereth_client_runtime::character::ALUVIAN_MALE_SETUP);

    // Scan for a setup that actually differs, so the assertion below can fail.
    let mut found: Option<Setup> = None;
    let mut scanned = 0usize;
    for raw in 1u32..0x0400 {
        let id = DataId(0x0200_0000 | raw);
        let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
            continue;
        };
        let Ok(s) = Setup::decode_payload(id, &bytes) else {
            continue;
        };
        scanned += 1;
        if s.parts.is_empty() {
            continue;
        }
        if (s.height - male.height).abs() > 0.05 && (s.radius - male.radius).abs() > 0.05 {
            found = Some(s);
            break;
        }
    }
    let other = found.unwrap_or_else(|| {
        panic!("scanned {scanned} setup records and none differs from the Aluvian male's collision half")
    });

    let mut gpu = test_gpu(640, 640);
    let cfg = SceneConfig {
        landblock: dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    let body = scene.character.as_mut().expect("a body");
    assert!(
        (body.height() - male.height).abs() < 1e-4 && (body.radius() - male.radius).abs() < 1e-4,
        "the offline body's collision half is not the Aluvian male's"
    );
    assert!(
        body.set_setup_id(other.id, None)
            .expect("the scanned setup record builds"),
        "the scanned setup record is the one the body already had"
    );
    assert!(
        (body.height() - other.height).abs() < 1e-4,
        "the body is {:.4} m tall and the setup record {:#010X} is {:.4} m",
        body.height(),
        other.id.0,
        other.height
    );
    assert!(
        (body.radius() - other.radius).abs() < 1e-4,
        "the body's radius is {:.4} and the setup record {:#010X}'s radius is {:.4}",
        body.radius(),
        other.id.0,
        other.radius
    );
    eprintln!(
        "setup record {:#010X} ({:.4} m, r {:.4}) replaces {:#010X} ({:.4} m, r {:.4}) on the collision \
         half as well as the drawn one; {scanned} setups scanned to find one that differs",
        other.id.0,
        other.height,
        other.radius,
        male.id.0,
        male.height,
        male.radius,
    );
}

/// Behaviour: objects.appearance.the-players-body-is-built-from-the-setup-the-server-named
///
/// A motion table the dat does not hold is refused **before** anything is mutated.
///
/// Description handling sets the motion table immediately after the setup. Setting it destroys
/// and recreates the movement manager, so discovering the table is absent after `set_setup` has
/// run would leave a body with the new parts and no movement state. The recorded corpus's players
/// name `0x09000001`, the table the offline body already carries, so the absent table is made up.
#[test]
fn a_motion_table_the_dat_does_not_hold_leaves_the_body_alone() {
    let store = retail_store();
    let region = load_region(&store).expect("the region decodes");
    let mut gpu = test_gpu(640, 640);
    let cfg = SceneConfig {
        landblock: dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    let before = scene.character_built_from().to_vec();

    // A real setup, so the failure can only be the motion table.
    let female = DataId(0x0200_004E);
    assert!(
        store.read_typed(DbType::Setup, female).is_ok(),
        "{female:?} is in the retail dat"
    );
    let missing = DataId(0x0900_FFFF);
    assert!(
        store.read_typed(DbType::MTable, missing).is_err(),
        "{missing:?} is in the retail dat after all -- pick another absent id"
    );

    let body = scene.character.as_mut().expect("a body");
    let err = body
        .set_setup_id(female, Some(missing))
        .expect_err("a missing motion table must not build");
    assert_eq!(
        body.setup_id(),
        dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
        "the setup was installed even though the motion table was refused"
    );
    assert_eq!(body.stats.setup_changes, 0, "a refused rebuild was counted");
    assert_eq!(
        scene
            .character
            .as_ref()
            .expect("a body")
            .driver()
            .part_array
            .parts
            .len(),
        before.len(),
        "the part array was replaced by a rebuild that failed"
    );
    // And the same call **without** the bad table succeeds, so the refusal above is about the
    // motion table and not about the setup.
    assert!(
        scene
            .character
            .as_mut()
            .expect("a body")
            .set_setup_id(female, None)
            .expect("the setup alone builds"),
        "the setup did not change"
    );
    eprintln!("a missing motion table is refused with `{err}` and the body is untouched");
}
