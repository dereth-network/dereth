//! NPCs and players wear the clothes the server sent: every create's `ObjDesc` is kept, applied to
//! part graphics objects and surfaces, and its shift palette reaches the decoded textures, with
//! the tolerant failure counters at zero.
//!
//! Fixture: every promoted capture, discovered on disk and replayed through `dereth_client_net`
//! transport and session dispatch, `dereth_protocol` description decoding, `dereth_animation` part
//! changes, palette composition and the `dereth_render` GPU device; plus the retail dat palettes,
//! surfaces and textures the descriptions name.
//!
//! 1. **The description arrives and is kept.** Every `Item_CreateObject 0xF745` carries an
//!    `ObjDesc`, and the object stream keeps it.
//! 2. **It is applied.** Part graphics-object ids and surface overrides change, and applying the
//!    descriptions never reports a failure over the whole corpus.
//! 3. **The palette shift reaches the pixels.** A part's `PFID_INDEX16` texture decoded against
//!    the shift palette differs from the same texture against the surface's own default, in
//!    exactly the ranges the sub-palettes name and nowhere else.
//! 4. **The counters are asserted.** `palette_range_failures` and `palette_missing` are the
//!    tolerant paths.
//! 5. **The cost is measured.** Descriptor-heap textures a populated capture consumes, against
//!    `dereth_render`'s 2,048-texture ceiling.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::{
    addr, connection_sequence_number, corpus_sessions, load, retail_store, test_gpu,
};
use crate::common::recorded_world_sessions;
use dereth_client::world::{SceneReads, SceneWrites};

use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{DataId, LocalTime, ObjectId};

// ---------------------------------------------------------------------------------------------
// The replay: the capture through the client's network and object stream.
// ---------------------------------------------------------------------------------------------

/// The captures whose character actually reaches the world, **measured rather than named**: as
/// many as the corpus records a `0x0013` for. A login-only capture (the account authenticates and
/// disconnects without a character entering the world) is left out.
fn world_sessions() -> &'static [String] {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let world: Vec<String> = corpus_sessions()
            .into_iter()
            .filter(|s| last_populated(s) != 0)
            .collect();
        assert_eq!(
            world.len(),
            recorded_world_sessions(),
            "the captures that enter the world, as many as carry a recorded 0x0013"
        );
        world
    })
}

struct Replayed {
    objects: ObjectStream,
    /// The index of the last datagram after which the client still held objects. Every recording
    /// ends with a clean logout, and ending the character session empties the object model.
    last_populated: usize,
    player: Option<(ObjectId, dereth_client::objects::Presence)>,
}

fn replay_upto(session: &str, limit: usize) -> Replayed {
    let records = load(session);
    assert!(!records.is_empty(), "{session} is empty");
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

fn replay(session: &str) -> Replayed {
    replay_upto(session, usize::MAX)
}

/// [`Replayed::last_populated`] of the whole of `session`: a pure function of the recording, so
/// each session is replayed to its end once per process for it and the answer kept.
fn last_populated(session: &str) -> usize {
    static FOUND: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
        std::sync::OnceLock::new();
    let found = FOUND.get_or_init(Default::default);
    if let Some(n) = found
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(session)
    {
        return *n;
    }
    let n = replay(session).last_populated;
    found
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(session.to_owned(), n);
    n
}

// ---------------------------------------------------------------------------------------------
// 1. The description arrives and is kept.
// ---------------------------------------------------------------------------------------------

/// **Every create carries an `ObjDesc` and the object stream keeps it.** The client decodes an
/// `ObjDesc` on every create; at least one live object in the corpus is dressed.
///
/// Oracle: every in-world capture. The counts are what a real server sent a real client.
#[test]
fn every_create_carries_an_objdesc_and_the_stream_keeps_it() {
    let mut total = 0usize;
    let mut dressed = 0usize;
    for name in world_sessions() {
        let in_world = last_populated(name);
        let r = replay_upto(name, in_world + 1);
        for (_, p) in r.objects.presences() {
            total += 1;
            if !p.objdesc.subpalettes.is_empty()
                || !p.objdesc.texture_changes.is_empty()
                || !p.objdesc.anim_part_changes.is_empty()
            {
                dressed += 1;
            }
        }
    }
    assert!(total > 0, "the corpus created nothing");
    assert!(
        dressed > 0,
        "not one of the {total} objects the corpus created carries an ObjDesc; if this is 0 the \
         presence is discarding it again"
    );
    eprintln!("{dressed} of {total} live objects carry an ObjDesc");
}

// ---------------------------------------------------------------------------------------------
// 2. It is applied, and the shift palette reaches the pixels.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.a-subpalette-changes-only-the-ranges-it-names
///
/// **A wire sub-palette changes exactly the ranges it names.** A `Subpalette` replaces a range of
/// the palette a `PFID_INDEX16` surface expands against, and the result differs from the surface's
/// own default palette in exactly the ranges the description names.
///
/// Oracle: the dat palettes and one recorded `ObjDesc`. The palette copy is index-aligned:
/// destination entry i takes source entry i, not source entry i minus the offset. Descriptor
/// offsets and lengths are expressed in units of eight entries.
#[test]
fn a_wire_subpalette_changes_exactly_the_ranges_it_names() {
    let store = retail_store();
    let in_world = last_populated("first-login-walk-jump");
    let r = replay_upto("first-login-walk-jump", in_world + 1);
    let (_, p) = r.player.as_ref().expect("the capture creates a player");
    let od = &p.objdesc;
    assert!(
        !od.subpalettes.is_empty(),
        "the capture's player has no sub-palettes"
    );

    let textures = dereth_client::textures::TextureStore::new(&store);
    let base = textures
        .palette(DataId(od.palette_id))
        .unwrap_or_else(|| panic!("base palette {:#010X} is not in the dat", od.palette_id));
    let mut shifted = base.make_modified();
    // The palette composition rule is spelled out independently here so the test is not
    // simply the implementation called twice.
    let mut touched = vec![false; 2048];
    for s in &od.subpalettes {
        let src = textures
            .palette(DataId(s.sub_id))
            .unwrap_or_else(|| panic!("sub-palette {:#010X} is not in the dat", s.sub_id));
        let start = usize::from(s.offset) * 8;
        let count = if s.num_colors == 0 {
            256
        } else {
            usize::from(s.num_colors)
        } * 8;
        assert!(
            start + count <= 2048,
            "{s:?} runs past the end of a 2048-entry palette"
        );
        assert!(
            shifted.apply_subpalette(
                u16::from(s.offset),
                u16::from(s.num_colors),
                &src.0[start..]
            ),
            "{s:?} was refused"
        );
        for e in &mut touched[start..start + count] {
            *e = true;
        }
    }
    let differs: Vec<usize> = (0..2048).filter(|&i| shifted.0[i] != base.0[i]).collect();
    assert!(!differs.is_empty(), "the shift changed nothing at all");
    for i in differs {
        assert!(touched[i], "entry {i} changed outside every named range");
    }

    // And the shift reaches the decode: a body part's INDEX16 texture is different pixels.
    let setup = p.setup_id.expect("the player has a setup");
    let parts = dereth_client::models::resolve_parts(&store, setup);
    let mut compared = 0usize;
    for part in parts.iter().take(8) {
        for g in dereth_client::models::build_gfxobj(&store, part.gfxobj) {
            let Some(sid) = g.surface else { continue };
            if !textures.is_palettised(sid).unwrap_or(false) {
                continue;
            }
            let plain = textures.texture_data_clipped(sid, false).expect("decodes");
            let dyed = textures
                .texture_data_shifted(sid, false, Some(&shifted))
                .expect("decodes");
            assert_eq!(plain.levels.len(), dyed.levels.len());
            if plain.levels[0] != dyed.levels[0] {
                compared += 1;
            }
        }
    }
    assert!(
        compared > 0,
        "the shift palette changed no body texture; every part of a human body is INDEX16, so a \
         zero here means the substitution never reached the expansion"
    );
    eprintln!("{compared} of the player's surfaces are recoloured by his own ObjDesc");
}

// ---------------------------------------------------------------------------------------------
// 3. The whole path, through the real device.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.a-wire-objdesc-dresses-the-body-it-names
///
/// **NPCs and players wear their clothes.**
///
/// Every object the capture created has its description applied to its geometry: its part swaps
/// land in the meshes, its shift palette lands in the textures, and the frame draws. The two
/// failure counters are asserted to be zero, so a tolerant lookup cannot hide a gap.
#[test]
fn the_captures_objects_wear_what_the_server_sent() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let in_world = last_populated("first-login-walk-jump");
    let mut r = replay_upto("first-login-walk-jump", in_world + 1);

    let (_, p) = r.player.as_ref().expect("the capture creates a player");
    let pos = p.position.expect("the player has a position");
    let block = pos.cell.landblock();
    let landblock = (u16::from(block.x()) << 8) | u16::from(block.y());
    let cfg = SceneConfig {
        landblock,
        character: false,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");

    // The baseline: how many textures the world alone needed, before a single outfit.
    let world_only = scene.draw.stats.textures_uploaded;

    scene
        .sync_objects(&store, &mut gpu, &mut r.objects)
        .expect("objects sync");
    let s = scene.draw.stats;

    assert!(s.server_objects > 0, "nothing to draw");
    assert!(s.objdescs_applied > 0, "not one ObjDesc was applied");
    assert_eq!(
        s.objdesc_failures, 0,
        "part-array appearance updates reported {} failure(s): a part index the setup does not \
         have, or a palette applied to an empty part array",
        s.objdesc_failures
    );
    assert_eq!(
        s.palette_range_failures, 0,
        "{} sub-palette range(s) could not be applied",
        s.palette_range_failures
    );
    assert_eq!(
        s.palette_missing, 0,
        "{} shift palette(s) had no base",
        s.palette_missing
    );

    // Two objects with different descriptions no longer share geometry, and two with the same one
    // still do.
    assert!(
        s.server_object_setups > 1,
        "every object still shares one appearance: the ObjDesc is not in the cache key"
    );
    assert!(
        s.server_object_setups <= s.server_objects,
        "{} appearances for {} objects",
        s.server_object_setups,
        s.server_objects
    );

    // The descriptor heap. `dereth_render`'s SRV heap is 4,096 descriptors and `upload_texture`
    // takes a contiguous pair per texture, so 2,048 is the ceiling and nothing reclaims a slot.
    eprintln!(
        "textures: {} for the world alone, {} after {} objects in {} distinct outfits \
         (ceiling 2048)",
        world_only, s.textures_uploaded, s.server_objects, s.server_object_setups
    );
    assert!(
        s.textures_uploaded < 2048,
        "{} textures uploaded, which is past the descriptor heap's 2048",
        s.textures_uploaded
    );

    scene
        .reserve_upload_arena(&mut gpu)
        .expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(&mut gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let rgba = image.to_rgba();
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8)
        .count();
    assert!(
        lit > (image.width as usize * image.height as usize) / 10,
        "only {lit} lit pixels: the world did not draw"
    );
}

/// Behaviour: objects.appearance.a-wire-objdesc-dresses-the-body-it-names
///
/// **The player's own body wears his `ObjDesc`.** He is the one object with no `SceneObject`: his
/// body is built locally before the server says anything and the server's copy is not drawn, so
/// his description has to reach that local part array.
///
/// Oracle: first-login-walk-jump's player create and `ALUVIAN_MALE_SETUP` from the dat store. The
/// assertion checks each part's surface override, a copy-on-write clone created only by an
/// override, so an `ObjDesc` that was accepted and then dropped cannot satisfy it.
#[test]
fn the_players_own_body_wears_his_objdesc() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let in_world = last_populated("first-login-walk-jump");
    let mut r = replay_upto("first-login-walk-jump", in_world + 1);
    let (_, p) = r.player.as_ref().expect("the capture creates a player");
    let pos = p.position.expect("the player has a position");
    assert!(
        !p.objdesc.subpalettes.is_empty(),
        "the capture's player has no ObjDesc"
    );
    let block = pos.cell.landblock();
    let cfg = SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        // The body, which is the whole point of this test.
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    // Attach the local character after loading the world, as the application does.
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // Before applying the description, setup construction alone gives no part a surface clone.
    {
        let c = scene.character.as_ref().expect("a body");
        let d = c.driver();
        assert!(
            d.part_array
                .parts
                .iter()
                .all(|x| x.surface_overrides.is_none()),
            "a fresh part array already has surface overrides"
        );
    }

    scene
        .sync_objects(&store, &mut gpu, &mut r.objects)
        .expect("objects sync");

    let s = scene.draw.stats;
    assert_eq!(
        s.objdesc_setup_mismatch, 0,
        "the capture's player is an Aluvian male, so the local body's setup matches"
    );
    assert!(s.objdescs_applied > 0);
    assert_eq!(s.objdesc_failures, 0);
    assert_eq!(s.palette_range_failures, 0);
    assert_eq!(s.palette_missing, 0);
    // A `0xF625` for an object the tables do not hold would be silently dropped; the counter says
    // it did not happen.
    assert_eq!(
        r.objects.stats.objdesc_events_unknown, 0,
        "{} Item_ObjDescEvent(s) arrived for an unknown object",
        r.objects.stats.objdesc_events_unknown
    );

    let c = scene.character.as_ref().expect("a body");
    let d = c.driver();
    let dressed = d
        .part_array
        .parts
        .iter()
        .filter(|x| x.surface_overrides.is_some())
        .count();
    assert!(
        dressed > 0,
        "not one of the body's {} parts carries a surface override after the ObjDesc",
        d.part_array.parts.len()
    );
    // Applying the shift palette updates **every** part, so a description with sub-palettes
    // dresses the whole array, not a subset.
    assert_eq!(
        dressed,
        d.part_array.parts.len(),
        "the shift palette reached only {dressed} of {} parts",
        d.part_array.parts.len()
    );
    eprintln!(
        "the player's {} parts all carry the server's palette; {} events, {} descriptions applied",
        dressed, r.objects.stats.objdesc_events, s.objdescs_applied
    );
}

/// Behaviour: objects.appearance.a-wire-objdesc-dresses-the-body-it-names
///
/// **No `ObjDesc` in the corpus fails to apply.** Every other in-world capture, for the counters
/// alone: neither tolerant path may fire on any of them.
///
/// Each session gets its own device. early-inventory-and-casting alone uploads 1,174 textures and
/// the device's descriptor heap holds 2,048; dropping a `WorldScene` returns none of its slots,
/// because reclaiming them takes an explicit `WorldScene::release_textures`, and even that hands
/// back only the bake cache and leaves `dereth_world_render`'s `MergeCache` terrain surfaces
/// leaked. Two of the large sessions on one device would exhaust the heap.
#[test]
fn no_objdesc_in_the_corpus_fails_to_apply() {
    let store = retail_store();
    for name in world_sessions()
        .iter()
        .filter(|s| s.as_str() != "first-login-walk-jump")
    {
        let mut gpu = test_gpu(800, 600);
        let in_world = last_populated(name);
        let mut r = replay_upto(name, in_world + 1);
        let (_, p) = r.player.as_ref().expect("the capture creates a player");
        let pos = p.position.expect("the player has a position");
        let block = pos.cell.landblock();
        let cfg = SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: false,
            land_radius: 0,
            scenery_radius: 0,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene
            .sync_objects(&store, &mut gpu, &mut r.objects)
            .expect("objects sync");
        let s = scene.draw.stats;
        assert!(s.objdescs_applied > 0, "{name}: no ObjDesc applied");
        assert_eq!(
            s.objdesc_failures, 0,
            "{name}: object-description application failed"
        );
        assert_eq!(
            s.palette_range_failures, 0,
            "{name}: a sub-palette range was refused"
        );
        assert_eq!(s.palette_missing, 0, "{name}: a shift palette had no base");
        eprintln!(
            "{name}: {} objects, {} outfits, {} ObjDescs applied, {} textures",
            s.server_objects, s.server_object_setups, s.objdescs_applied, s.textures_uploaded
        );
    }
}
