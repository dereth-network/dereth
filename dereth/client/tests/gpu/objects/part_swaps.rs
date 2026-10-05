//! Part swaps: the character wears the trousers and the boots the server sent, and takes them off
//! again when it says so.
//!
//! Replacing a part first restores its surfaces: the part's custom surface array is released before
//! the new geometry is installed, so every texture-map substitution on that part is dropped and
//! only the shift palette survives (it is re-applied afterwards). A client that only assigned the
//! new graphics object would leave the boots' texture on the player's feet after he takes them off.
//!
//! Fixture: every recorded session (datagrams between the retail client and a local ACE server),
//! replayed through the client's network and object stream, plus the retail dats. Every part index,
//! part id and texture id asserted here is read out of the recordings or out of the setup record's
//! own placement frames at run time.
//!
//! What is asserted:
//!
//! 1. **The census**: how many of the corpus's objects carry part swaps, how many swaps that is,
//!    over how many setups, and that every one of them names a part the setup has and a graphics
//!    object the dat holds.
//! 2. **Which part is swapped for which.** The human body's feet and lower legs are identified by
//!    **their own placement-frame heights** in setup `0x02000001`, not by a name, and the
//!    recording's player is shown to swap exactly those, to ids that differ from the setup's own
//!    and that carry geometry.
//! 3. **End to end**: the body drawn on a real device carries exactly the `part_index -> part_id`
//!    map the recording sent, its triangle count changes, and the changed pixels are confined to
//!    the body.
//! 4. **The unequip**: replaying the player's own `0xF625 Item_ObjDescEvent` sequence onto one live
//!    part array must land in the same state as applying the last description to a fresh one, at
//!    **every** step. This is the assertion that reddens when part replacement's surface
//!    restoration is removed.

#![cfg(gpu)]

use super::common::{
    addr, connection_sequence_number, corpus_sessions, load, retail_store, test_gpu,
};
use crate::common::recorded_world_sessions;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, PartArray, TextureMapChange};
use dereth_assets::{Decode, Setup};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::objects::physics_state::HIDDEN_PS;
use dereth_protocol::objects::{
    ItemCreateObject, ItemObjDescEvent, ItemSetState, ItemUpdateObject,
};
use dereth_render::device::Gpu;
use dereth_world_data::anim_assets::DatAnimAssets;
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
    dereth_world_data::landblock::load_region,
};

// ---------------------------------------------------------------------------------------------
// The replay: the capture through the client's network and object stream.
// ---------------------------------------------------------------------------------------------

struct Replayed {
    objects: ObjectStream,
    /// The index of the last datagram after which the client still held objects; every recording
    /// ends with a clean logout and the end-of-session teardown empties the object model.
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

fn triangles(store: &RetailDatStore, gfxobj: DataId) -> usize {
    dereth_client_runtime::models::build_gfxobj(store, gfxobj)
        .iter()
        .map(|g| g.vertices.len() / 3)
        .sum()
}

/// The `part_index -> part_id` map an `ObjDesc` ends up asking for. The wire may name an index more
/// than once — 950 of the corpus's 2,967 changes do — and part replacement walks the list in
/// order, so the last entry wins.
fn swap_map(od: &dereth_protocol::types::ObjDesc) -> BTreeMap<u32, DataId> {
    od.anim_part_changes
        .iter()
        .map(|c| (u32::from(c.part_index), DataId(c.part_id)))
        .collect()
}

fn to_anim(od: &dereth_protocol::types::ObjDesc) -> ObjDesc {
    ObjDesc {
        part_changes: od
            .anim_part_changes
            .iter()
            .map(|c| AnimPartChange {
                part_index: u32::from(c.part_index),
                part_id: DataId(c.part_id),
            })
            .collect(),
        texture_changes: od
            .texture_changes
            .iter()
            .map(|c| TextureMapChange {
                part_index: u32::from(c.part_index),
                old_texture: DataId(c.old_tex_id),
                new_texture: DataId(c.new_tex_id),
            })
            .collect(),
        palette_id: DataId(od.palette_id),
        subpalettes: od
            .subpalettes
            .iter()
            .map(|x| PaletteRange {
                palette_set: DataId(x.sub_id),
                offset: u32::from(x.offset),
                length: u32::from(x.num_colors),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The census.
// ---------------------------------------------------------------------------------------------

/// **Every recorded part swap is one the client can wear.** Every in-world recorded session, every
/// object the server created while the client was in world, and every `AnimPartChange` any of them
/// carries.
///
/// The three structural claims are the ones a part swap can silently fail on: an index the setup
/// does not have (part replacement skips it and fails the call), a graphics object the dat does
/// not hold (the client refuses the swap and keeps the old part, which is why the replay resolves
/// against the real asset source instead of `NoAssets`), and a swap to something with no drawing
/// polygons, which is a limb that vanishes.
#[test]
fn the_corpus_part_swaps_all_name_a_part_the_setup_has_and_a_gfxobj_the_dat_holds() {
    let store = retail_store();
    // Ask the client's asset source as well as the store, so this census is the same instrument
    // the running client uses rather than a second opinion beside it.
    let seam = DatAnimAssets::new(Arc::clone(&store));
    let (mut objects, mut dressed, mut swaps, mut identity) = (0usize, 0usize, 0usize, 0usize);
    let mut setups: BTreeSet<u32> = BTreeSet::new();
    let mut indices: BTreeSet<u32> = BTreeSet::new();
    for name in world_sessions() {
        let r = in_world(name);
        for (_, p) in r.objects.presences() {
            objects += 1;
            if p.objdesc.anim_part_changes.is_empty() {
                continue;
            }
            dressed += 1;
            let setup_id = p
                .setup_id
                .expect("an object with an ObjDesc has a setup record");
            setups.insert(setup_id.0);
            let base = setup_of(&store, setup_id).parts;
            for c in &p.objdesc.anim_part_changes {
                swaps += 1;
                let i = u32::from(c.part_index);
                indices.insert(i);
                let slot = usize::try_from(i).expect("a part index fits a usize");
                let was = *base.get(slot).unwrap_or_else(|| {
                    panic!(
                        "{name}: setup record {:#010X} has {} parts and a swap names index {i}",
                        setup_id.0,
                        base.len()
                    )
                });
                let now = DataId(c.part_id);
                assert!(
                    store.read_typed(DbType::GfxObj, now).is_ok(),
                    "{name}: a swap names graphics-object record {now:?}, which is not in the retail dat"
                );
                assert!(
                    matches!(
                        dereth_animation::data::AnimAssets::gfxobj(&seam, now),
                        dereth_animation::data::GfxObjLookup::Present { .. }
                    ),
                    "{name}: the client's own graphics-object decoder refuses {now:?}, so this swap would keep the previous limb in a running client"
                );
                assert!(
                    triangles(&store, now) > 0,
                    "{name}: the swap at index {i} names {now:?}, which has no drawing polygons"
                );
                if was == now {
                    identity += 1;
                }
            }
        }
    }
    assert!(objects > 0, "the corpus created nothing");
    assert!(
        dressed > 0,
        "not one object in the corpus carries a part swap"
    );
    assert!(
        swaps >= dressed,
        "every object carrying a part swap carries at least one"
    );
    assert!(
        identity < swaps,
        "not every swap names the part the setup already has"
    );
    eprintln!(
        "part-swap census: {dressed} of {objects} live objects carry part swaps -- {swaps} swaps over \
         {} distinct setup records, part indices {}..={}, of which {identity} name the part the setup \
         already has (an unequip, which is still a part replacement and still restores that part's \
         surfaces)",
        setups.len(),
        indices.iter().next().expect("indices"),
        indices.iter().next_back().expect("indices")
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Which part is swapped for which.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-body-wears-the-part-swaps-the-server-sent
///
/// **The trousers and the boots, named by the dat rather than by hand.**
///
/// The setup record's placement frames put every part where it belongs in the body's own space, so
/// the two lowest parts of a human setup are the two feet and the two above them are the lower
/// legs. That is how "boots" and "trousers" are identified here: by `placement_frames`' own z,
/// through `models::placement_frames`, which follows the original placement-frame fallback chain.
///
/// Then the capture's own player description is shown to swap exactly those four, to ids that
/// differ from the setup's own and that carry geometry. **Every id printed and compared here comes
/// off the wire or out of the dat.**
#[test]
fn the_capture_swaps_the_two_feet_and_the_two_lower_legs_of_its_players_body() {
    let store = retail_store();
    let r = in_world("first-login-walk-jump");
    let (_, p) = r.player.as_ref().expect("the capture creates a player");
    let setup_id = p.setup_id.expect("the player has a setup record");
    let setup = setup_of(&store, setup_id);
    let frames = dereth_client_runtime::models::placement_frames(
        &setup,
        dereth_client_runtime::models::PLACEMENT_RESTING,
    )
    .expect("the human setup carries a placement");
    assert_eq!(frames.len(), setup.parts.len(), "one frame per part");

    // Rank the parts by height. The feet are the two lowest; the lower legs are the next two.
    let mut by_height: Vec<(usize, f32)> = frames
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f.origin.z))
        .collect();
    by_height.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("no NaN in a placement frame"));
    let feet: Vec<usize> = by_height[..2].iter().map(|(i, _)| *i).collect();
    let shins: Vec<usize> = by_height[2..4].iter().map(|(i, _)| *i).collect();
    // A foot is left/right, so the two must sit on opposite sides of the body's centre line and at
    // the same height -- otherwise "the two lowest parts" is not a pair of feet and the rest of
    // this test is about the wrong parts.
    assert!(
        frames[feet[0]].origin.x * frames[feet[1]].origin.x < 0.0,
        "the two lowest parts are on the same side of the body"
    );
    assert!(
        (frames[feet[0]].origin.z - frames[feet[1]].origin.z).abs() < 1e-3,
        "the two lowest parts are at different heights"
    );
    assert!(
        frames[shins[0]].origin.z > frames[feet[0]].origin.z,
        "the shins are not above the feet"
    );

    let swaps = swap_map(&p.objdesc);
    assert!(
        !swaps.is_empty(),
        "the capture's player carries no part swaps at all"
    );
    for (what, group) in [("boot", &feet), ("trouser leg", &shins)] {
        for &i in group {
            let idx = u32::try_from(i).expect("a part index fits a u32");
            let was = setup.parts[i];
            let now = *swaps.get(&idx).unwrap_or_else(|| {
                panic!(
                    "the capture's player description does not swap part {i}, the {what} at \
                     z={:+.4}",
                    frames[i].origin.z
                )
            });
            assert_ne!(
                now, was,
                "part {i} (the {what}) is swapped to the id the setup already has, so this frame \
                 cannot show a {what} at all"
            );
            assert!(
                triangles(&store, now) > 0,
                "part {i} (the {what}) is swapped to {now:?}, which has no drawing polygons"
            );
            eprintln!(
                "part {i:2} ({what}, z={:+.4}, x={:+.4}): {:#010X} -> {:#010X} ({} triangles)",
                frames[i].origin.z,
                frames[i].origin.x,
                was.0,
                now.0,
                triangles(&store, now)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 3. End to end, on a real device.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-body-wears-the-part-swaps-the-server-sent
///
/// **The body drawn on the device wears what the server sent.**
///
/// The player is the one object with no `SceneObject`: his body is built locally and the server's
/// copy is not drawn, so his description reaches the same part array the renderer walks, and every
/// part's `gfxobj_id` can be compared with the capture's own map.
///
/// The pixel half compares a paired before/after in memory: before is setup construction alone
/// (the body in its loincloth, bare legs, bare feet) and after is the same body with the
/// capture's description applied.
#[test]
fn the_drawn_body_carries_exactly_the_captures_part_swaps() {
    let store = retail_store();
    let mut gpu = test_gpu(640, 640);
    let r = in_world("first-login-walk-jump");
    let (_, p) = r
        .player
        .as_ref()
        .expect("the capture creates a player")
        .clone();
    let pos = p.position.expect("the player has a position");
    let setup_id = p.setup_id.expect("the player has a setup record");
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
    let region = load_region(&store).expect("the region decodes");

    // Two scenes rather than two moments of one, because the *only* difference between the frames
    // has to be the description. A single scene shot before and after `sync_objects` also differs
    // by everything the clock moved in between -- the rain, the sky, the body's own idle cycle --
    // and that was 44 % of the frame, measured. So: identical construction, identical settling,
    // weather off, and the dressed run additionally synchronised against a stream holding **only**
    // the capture's `0xF746` and the player's own `0xF745`, so no other server object is drawn.
    let mut render = |dressed: bool| -> (Frame, usize, Vec<DataId>) {
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        if dressed {
            let mut solo = player_only_stream("first-login-walk-jump");
            scene
                .sync_objects(&store, &mut gpu, &mut solo)
                .expect("objects sync");
            assert_eq!(
                scene.server_object_count(),
                0,
                "the solo stream drew a SceneObject: the player must have no second body"
            );
            let s = scene.draw.stats;
            assert_eq!(
                s.objdesc_setup_mismatch, 0,
                "the local body's setup record does not match the server's, so the description was \
                 refused wholesale"
            );
            assert!(s.objdescs_applied > 0, "not one description was applied");
            assert_eq!(
                s.objdesc_failures, 0,
                "object-description application reported a failure"
            );
        }
        settle(&mut scene, 0);
        let parts: Vec<DataId> = scene
            .character
            .as_ref()
            .expect("a body")
            .driver()
            .part_array
            .parts
            .iter()
            .map(|x| x.gfxobj_id)
            .collect();
        let triangles = scene.draw.stats.character_triangles;
        (shot(&mut scene, &mut gpu), triangles, parts)
    };

    let (before, before_triangles, undressed) = render(false);
    let (after, after_triangles, drawn) = render(true);

    // Undressed, every part is the setup's own -- so the "after" comparison below is against a
    // body that demonstrably started from setup creation alone.
    assert_eq!(
        undressed, base,
        "a fresh body does not use the setup record's own part list"
    );

    // Dressed, every part the description named is the capture's own choice, and every part it did
    // **not** name is still the setup's. Both halves matter: a build that swapped every part to one
    // id would satisfy the first alone.
    assert_eq!(drawn.len(), base.len(), "the local body's part count");
    let (mut swapped, mut kept) = (0usize, 0usize);
    for (i, &got) in drawn.iter().enumerate() {
        let idx = u32::try_from(i).expect("a part index fits a u32");
        match expected.get(&idx) {
            Some(&want) => {
                assert_eq!(
                    got, want,
                    "part {i}: the capture asked for {want:?} and the body draws {got:?}"
                );
                swapped += 1;
            }
            None => {
                assert_eq!(
                    got, base[i],
                    "part {i} was not named by the description and must still be the setup's"
                );
                kept += 1;
            }
        }
    }
    assert_eq!(swapped, expected.len(), "every named part was checked");
    assert_ne!(
        before_triangles, after_triangles,
        "the body has the same {before_triangles} triangles dressed as undressed, so the swapped \
         meshes never reached the device"
    );

    let (n, bbox) = diff(&before, &after);
    let total = (before.1 * before.2) as usize;
    assert!(n > 0, "the description changed no pixel at all");
    assert!(
        n * 8 < total,
        "{n} of {total} pixels changed -- the two frames hold one body and nothing else that \
         can move, so dressing it must not repaint the scene"
    );
    eprintln!(
        "the body's {parts} parts: {swapped} swapped by the capture, {kept} left as the setup's; \
         {before_triangles} triangles undressed, {after_triangles} dressed; {n} of {total} \
         pixels differ, bounding box {bbox:?}",
        parts = swapped + kept
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The unequip — the assertion that reddens without part replacement's surface restoration.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.the-body-wears-the-part-swaps-the-server-sent
///
/// **Taking the boots off again.**
///
/// Resetting description changes to defaults restores every part's *palette* and nothing
/// else, so a texture-map substitution written by an earlier description survives into the next
/// one — in the client too. Only part replacement takes it off: every part the new
/// description names goes through part replacement and geometry-array replacement, which restores
/// surfaces and releases that part's custom
/// surface array. The client does not compare ids first, so a description that re-lists a part with
/// the id it already has still clears it, and that is exactly how an unequip works.
///
/// The invariant that follows, and the one asserted here: applying the whole recorded sequence to
/// **one live part array** must land where applying just the latest description to a **fresh** one
/// lands, at every step, for every part the latest description names.
///
/// Fixture: every in-world recording. `early-inventory-and-casting` and `short-second-connection`
/// send eleven successive descriptions (the create plus eight `0xF625 Item_ObjDescEvent`s and two
/// `0xF749` updates); without surface restoration, steps 5, 6 and 7 of
/// `early-inventory-and-casting` diverge on six parts, four of them the two lower legs and the two
/// feet, each keeping the previous garment's substitution. `long-solo-play`'s player sends **43**
/// descriptions in a row.
#[test]
fn replaying_the_captures_equip_sequence_lands_where_a_fresh_body_lands() {
    let store = retail_store();
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let mut checked = 0usize;
    let mut steps = 0usize;
    let mut longest = 0usize;
    // With the dat asset source in place, every corpus swap still succeeds.
    let mut swaps_replayed = 0usize;
    for session in world_sessions() {
        let r = in_world(session);
        let (player, p) = r.player.as_ref().expect("the capture creates a player");
        let setup_id = p.setup_id.expect("the player has a setup record");
        let setup = assets.setup(setup_id).expect("the setup record decodes");
        let descs = objdesc_history(session, *player);
        assert!(
            !descs.is_empty(),
            "{session}: the player's create carries no description"
        );
        longest = longest.max(descs.len());
        let mut live = fresh_array(&setup, &assets);
        for (n, d) in descs.iter().enumerate() {
            // `…_with(&assets)`, not the `NoAssets` default: against `NoAssets` this replay could
            // not fail a swap however absent the id, so "every corpus swap succeeds" would not be
            // a measurement.
            let live_ok = live.do_obj_desc_changes_from_default_with(d, &assets);
            let mut clean = fresh_array(&setup, &assets);
            let clean_ok = clean.do_obj_desc_changes_from_default_with(d, &assets);
            if d.part_changes.is_empty() {
                // A description with only texture or palette work can still return false for a
                // reason that has nothing to do with a graphics object; only the part-swap arm is
                // this test's claim.
            } else {
                swaps_replayed += d.part_changes.len();
                assert!(
                    live_ok && clean_ok,
                    "{session} step {n}: live or clean object-description application refused the recorded description"
                );
            }
            steps += 1;
            let named: BTreeSet<u32> = d.part_changes.iter().map(|c| c.part_index).collect();
            for (i, (a, b)) in live.parts.iter().zip(clean.parts.iter()).enumerate() {
                let idx = u32::try_from(i).expect("a part index fits a u32");
                assert_eq!(
                    a.gfxobj_id, b.gfxobj_id,
                    "{session} step {n} part {i}: the replayed body draws {:?} where a fresh one \
                     draws {:?}",
                    a.gfxobj_id, b.gfxobj_id
                );
                if !named.contains(&idx) {
                    // The client keeps an unnamed part's substitutions too; nothing to compare.
                    continue;
                }
                checked += 1;
                assert_eq!(
                    a.surface_overrides, b.surface_overrides,
                    "{session} step {n} part {i}: this part is named by description {n}, so \
                     part replacement restored its surfaces and it must carry exactly what a fresh body \
                     carries. The replayed body has {:?}",
                    a.surface_overrides
                );
            }
        }
    }
    // A corpus in which no player ever changed clothes would satisfy every assertion above
    // vacuously, so the sequence itself is asserted to exist.
    assert!(
        longest > 1,
        "no session sends more than one description for its player: there is no equip sequence in \
         the corpus and this test proves nothing"
    );
    assert!(
        swaps_replayed > 0,
        "not one part swap was replayed: the asset-source assertion is vacuous"
    );
    eprintln!(
        "{steps} descriptions replayed across the corpus's in-world sessions (longest {longest}); \
         {checked} (part, step) pairs compared against a fresh application; {swaps_replayed} part \
         swaps resolved through the wired graphics-object decoder, 0 refused"
    );
}

/// Every appearance the server sent for one object, in order: the create, every `0xF749` update and
/// every `0xF625 Item_ObjDescEvent`.
fn objdesc_history(session: &str, want: ObjectId) -> Vec<ObjDesc> {
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
    let mut out = Vec::new();
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
                SessionEvent::WorldObject { opcode, body } => {
                    let od = match *opcode {
                        dereth_protocol::Opcode::ITEM_CREATE_OBJECT => {
                            dereth_protocol::read_body_padded::<ItemCreateObject>(body)
                                .ok()
                                .filter(|m| m.0.id == want)
                                .map(|m| m.0.objdesc)
                        }
                        dereth_protocol::Opcode::ITEM_UPDATE_OBJECT => {
                            dereth_protocol::read_body_padded::<ItemUpdateObject>(body)
                                .ok()
                                .filter(|m| m.0.id == want)
                                .map(|m| m.0.objdesc)
                        }
                        dereth_protocol::Opcode::ITEM_OBJ_DESC_EVENT => {
                            dereth_protocol::read_body_padded::<ItemObjDescEvent>(body)
                                .ok()
                                .filter(|m| m.id == want)
                                .map(|m| m.objdesc)
                        }
                        _ => None,
                    };
                    if let Some(od) = od {
                        out.push(to_anim(&od));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// An [`ObjectStream`] holding the capture's `0xF746 Login_PlayerCreated`, the player's own
/// `0xF745` and the first `0xF74B Item_SetState` that clears the player's hidden bit, and nothing
/// else.
///
/// The `0xF745` creates the player as the login leaves it, in portal space: its physics state
/// (`0x00404410`) carries the hidden bit, and a hidden body draws nothing. The server clears the
/// bit with a later `0xF74B` (`0x00400408`) once the client reports the login complete, so the
/// stream takes the capture's first such message too; without it both frames would show no body.
///
/// All three events are the capture's, taken off the replay and re-applied through the public
/// `apply_event`, the same door `objects::held_objects_draw` uses. The point is a scene whose
/// *only* server object is the player, so that two frames of it differ in the body and in nothing
/// else.
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

fn fresh_array(
    setup: &Arc<dereth_animation::data::SetupData>,
    assets: &DatAnimAssets,
) -> PartArray {
    let mut seq = dereth_animation::seq::Sequence::new();
    PartArray::create_setup(Arc::clone(setup), true, &mut seq, assets).expect("setup creation")
}

// ---------------------------------------------------------------------------------------------
// Frames.
// ---------------------------------------------------------------------------------------------

/// The part-array update has to run before a part frame means anything, and the chase
/// camera has to be snapped onto the body afterwards.
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
