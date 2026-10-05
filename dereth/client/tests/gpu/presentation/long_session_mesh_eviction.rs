//! Object mesh slots stay bounded over a long session: the world scene releases each baked
//! appearance and its texture slots once nothing links it, so a full replay of a recorded session
//! completes inside the descriptor heap with its peak occupancy stated.
//!
//! Fixture: `long-solo-play` (and the other recordings) replayed from the raw capture through a
//! socket-free `ClientNetwork` into a `WorldScene` on a software device, with `land_radius: 1`,
//! sampling descriptor-heap occupancy (one texture's `t0`/`t1` pair per slot) every frame.
//!
//! It is a **wall, not a leak**: a cache that only grows reaches the heap's ceiling at a fixed
//! amount of play, and an eviction policy that merely postpones it buys minutes. The claim is
//! therefore a **completed** replay with its peak occupancy stated, not a later failure.
//!
//! # Retail's release edge
//!
//! The original client caches every graphics object, surface, surface texture and palette a part
//! array links, per record. Their release edge is **reference counting**, with a *bounded* free
//! list behind it:
//!
//! | step | what it does |
//! |---|---|
//! | 1 | part destruction releases the shift palette and material, restores surfaces, then releases the graphics-object array |
//! | 2 | the graphics-array release releases the degrade record and **every** graphics object in the level array |
//! | 3 | the cache release decrements the link count **only while it is > 1**, and frees once it is `<= 1` — the cache's own link being the last one standing |
//! | 4 | the object free destroys the object unless retention and free-list eligibility are both enabled |
//! | 5 | free-list insertion stamps the current time, appends at the young end, and destroys the **oldest** once the count exceeds the configured maximum |
//!
//! A link count at or below one is freed without a decrement. A free-list count equal to its
//! configured maximum is retained; trimming begins only after the unsigned count is **strictly
//! greater**, so the list transiently reaches maximum plus one.
//!
//! The sizes are set per type: graphics objects **100 / 200**, surface textures **100 / 400**,
//! render surfaces **100 / 400**, surfaces **50 / 200**, palettes **60 / 100**, setups
//! **25 / 100**, and degradation records **80 / 200** (ideal / maximum, every one with recycling
//! disabled).
//!
//! The combined textures are the one thing with **no** retention at all: they live in the shared
//! and custom texture tables rather than in a per-record object cache, and are removed when their
//! reference count reaches zero. That is what `dereth_render::descriptor::TextureTable`
//! implements, and it is the resource a long session runs out of.
//!
//! # This client's edge, and its declared deviation
//!
//! `WorldScene::release_unlinked_appearances` is **step 3 and step 4's destroy arm**: after the
//! frame's removals, any `object_meshes` entry whose `Arc` the cache alone holds — the analogue
//! of a link count at or below one — is dropped and its texture slots handed back to
//! `Gpu::release_texture`, mirroring the combined texture's own reference count. A departed
//! landblock's bake and the mesh set an appearance change replaces are released the same way,
//! and two groups naming one (palette, texture) pair share one slot.
//!
//! **The free list of step 5 is deliberately not reproduced.** Retail retention is per *dat
//! record*; this client's cache entry is a whole baked *appearance* — a setup record crossed with
//! an `ObjDesc` — which has no retail counterpart and therefore no cap to copy, so the retention
//! is zero: the strictest form of the same edge. A creature that leaves and immediately returns
//! re-bakes; the bound is a property of what is *on screen*. `the_appearance_cache_still_shares`
//! guards that this did not turn into "no cache at all".
//!
//! On the full 1,098.5 s `long-solo-play` replay (8,791 frames) the peak is under the 2,048-pair
//! budget. The streaming edges are asserted by the streaming-release tests, on drives in which
//! only their own population moves; this file stays the corpus-scale measurement.

#![cfg(gpu)]

use std::collections::BTreeMap;
use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_net::client_session::testing::capture;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;
use dereth_render::device::{DescriptorUsage, DeviceConfig, Gpu};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

// ---------------------------------------------------------------------------------------------
// The device.
// ---------------------------------------------------------------------------------------------

fn warp() -> Gpu {
    warp_with(None)
}

/// A software device with a named heap size, in **descriptors** (`None` is the shipped budget),
/// or a failed test: the gpu tier never skips for want of a device.
fn warp_with(srv_descriptors: Option<u32>) -> Gpu {
    let cfg = DeviceConfig {
        width: 320,
        height: 240,
        srv_descriptors,
        ..DeviceConfig::default()
    };
    Gpu::new(None, &cfg).unwrap_or_else(|e| {
        panic!(
            "the gpu tier needs a software device (320x240, {srv_descriptors:?} descriptors): {e}"
        )
    })
}

/// The retail dats; the test fails when they are absent.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

// ---------------------------------------------------------------------------------------------
// The measurement.
// ---------------------------------------------------------------------------------------------

/// What one replay did to the descriptor heap.
///
/// Occupancy is quoted in **descriptor-heap slots**, which is one texture's `t0`/`t1` descriptor
/// pair each: `DescriptorUsage::capacity` slots, half of `SRV_HEAP_SIZE` **descriptors**. Saying
/// "3,000" without saying which of the two is meant is a factor-of-two error, so every number here
/// is a slot and the capacity is printed beside it in both units.
#[derive(Debug)]
struct Replayed {
    /// `Some(t)` when `sync_objects` refused, in capture seconds from the first datagram.
    stopped_at: Option<f64>,
    /// The last error text, if it stopped.
    stopped_with: String,
    /// Capture seconds from the first to the last datagram.
    duration: f64,
    /// Capture seconds actually replayed.
    replayed: f64,
    /// Slots live at the end of the replay.
    final_live: u32,
    /// The largest `live` the allocator ever saw: the stated peak.
    peak_live: u32,
    /// Slots ever taken from fresh heap space: `live + free + pending`.
    frontier: u32,
    /// The budget, in slots.
    capacity: u32,
    /// Slots served from the free list rather than fresh heap space.
    reuses: u64,
    /// Allocation attempts that found the heap full.
    exhaustions: u64,
    /// `object_meshes.len()` at the end.
    final_appearances: usize,
    /// The largest `object_meshes.len()` ever reached.
    peak_appearances: usize,
    /// Distinct appearances **built** over the whole replay, counting a rebuild of an appearance
    /// that had been released as a second one. The denominator for the peak.
    appearance_builds: u64,
    /// Appearances released because nothing linked them any more.
    appearance_releases: u64,
    /// `TextureTable` hits: uploads avoided because a texture was already cached under its key.
    texture_hits: u64,
    /// Releases naming a slot with no live entry. A double free. Must be zero.
    unknown_releases: u64,
    /// Objects live in the scene at the end.
    final_objects: usize,
    /// `Arc::strong_count` of every appearance still resident at the end. One means the cache
    /// alone holds it and the next sweep frees it; more means a `SceneObject` still wears it.
    link_counts: Vec<usize>,
    frames: usize,
    /// `BakeCache::textures_uploaded` — descriptor slots the **shared surface cache** consumed.
    /// This, not `object_meshes`, is what holds the descriptors: an appearance's `PartMesh`es
    /// carry *copies* of a `TextureSlot` the `BakeCache` owns.
    bake_textures: u32,
    /// `BakeCache::surfaces_resolved` — distinct `GroupKey`s ever resolved.
    bake_surfaces: u32,
    /// `SceneStats::texture_key_hits`: resolves served by the combined-texture cache's
    /// link-taking arm rather than by an upload.
    key_hits: u32,
    /// `SceneStats::clipmap_key_conflicts`.
    clipmap_conflicts: u32,
    /// `(t, live slots, appearances, bake textures, appearance releases)` sampled every 500
    /// frames, so the growth can be read rather than inferred from two endpoints.
    trace: Vec<(f64, u32, usize, u32, u64)>,
    /// Appearances released at the **last sample before the capture ended**, i.e. while the world
    /// was still live. This is the number that separates "the edge fires during play" from "the
    /// edge fires once, at the log-off" — and it is the only one of these figures that a mutation
    /// deleting the release call can be seen by. Without it the whole file could be satisfied by a
    /// teardown.
    releases_while_live: u64,
    /// Pairs `WorldScene::release_textures` handed back at teardown.
    released_at_teardown: u32,
    /// Slots still live on the device once the scene has been torn down. Everything above this is
    /// somebody else's (the UI, the fonts) or a leak.
    after_teardown: u32,
}

/// Replay `session` through the real `ObjectStream` and the real `WorldScene::sync_objects`, on
/// **every** frame, and report what the descriptor heap did.
///
/// Nothing is synthesised: the recorded datagrams drive the session, and the sampling is on the
/// allocator.
fn replay(session: &str, store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> Replayed {
    let records = capture::shared_session(session);
    let t0 = records.first().map_or(0.0, |r| r.t);
    let t_end = records.last().map_or(0.0, |r| r.t);
    let csn = recording::connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entered = false;
    let mut scene: Option<WorldScene> = None;
    let mut stopped_at: Option<f64> = None;
    let mut stopped_with = String::new();
    let mut peak_appearances = 0usize;
    let mut frames = 0usize;
    let mut last_t = t0;
    let mut trace: Vec<(f64, u32, usize, u32, u64)> = Vec::new();
    // Sampled on every frame, so the log-off's own teardown (which happens inside the capture)
    // cannot be mistaken for a release that happened while the player was playing.
    let mut releases_while_live = 0u64;

    for r in records {
        let now = LocalTime(r.t);
        last_t = r.t;
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in stream.pump(&mut net, now) {
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
        // **The scene is synchronised on every frame once it exists, including the frames after
        // the capture's own log-off.** A drive gated on `stream.player()`, which is `None` again after
        // `ObjectStream::reset`, would never see the teardown at all — and the teardown is precisely
        // the release edge measured here. The measurement therefore
        // includes the moment the object stops existing, not only the interval while it exists.
        if scene.is_none() {
            let Some(player) = stream.player() else {
                continue;
            };
            let Some(pos) = stream.presence(player).and_then(|p| p.position) else {
                continue;
            };
            let block = pos.cell.landblock();
            let cfg = SceneConfig {
                landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
                character: true,
                land_radius: 1,
                scenery_radius: 0,
                ..SceneConfig::default()
            };
            let mut s = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
            let region =
                dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
            s.attach_character(store, &region, gpu)
                .expect("the body is created");
            scene = Some(s);
        }
        let s = scene.as_mut().expect("built above");
        frames += 1;
        if let Err(e) = s.sync_objects(store, gpu, &mut stream) {
            stopped_at = Some(r.t - t0);
            stopped_with = format!("{e}");
            break;
        }
        peak_appearances = peak_appearances.max(s.draw.stats.server_object_setups);
        // While the player is in the world. `ObjectStream::reset` empties the object model at the
        // log-off and its removals reach the very next `sync_objects`, so the sample stops the
        // moment the player is gone.
        if stream.player().is_some() {
            releases_while_live = s.draw.stats.appearance_releases;
        }
        if frames.is_multiple_of(500) {
            trace.push((
                r.t - t0,
                gpu.descriptor_usage().live,
                s.draw.stats.server_object_setups,
                s.draw.stats.textures_uploaded,
                s.draw.stats.appearance_releases,
            ));
        }
    }

    // **One more frame before reading the census, and the reason is the measurement point.**
    // `SceneStats::server_object_setups` is written at the *end* of `sync_objects`, i.e. after that
    // call's creates and after its sweep. An appearance built by the last call the loop made has
    // therefore never been offered to `release_unlinked_appearances`, and reading the stat there
    // reports it as resident when the next frame would free it. Measured: 9 such entries on a tree
    // where the object path churns, 0 on one where it does not — a number that says more about
    // where the recording stops than about the cache. The extra frame samples after that last
    // appearance has been offered to the release sweep.
    if let Some(s) = scene.as_mut() {
        let _ = s.sync_objects(store, gpu, &mut stream);
    }
    let usage: DescriptorUsage = gpu.descriptor_usage();
    // **The census is taken BEFORE the teardown**, because `release_textures` clears
    // `object_meshes` outright and would make every one of these zero for a reason that has
    // nothing to do with the release edge. The first version of this file read them after, and
    // `final_appearances == 0` was then true of a cleared map rather than of an emptied one --
    // a passing assertion about the wrong moment, which is this project's most-repeated defect.
    let link_counts = scene
        .as_ref()
        .map_or_else(Vec::new, WorldScene::appearance_link_counts);
    let (
        final_appearances,
        final_objects,
        builds,
        releases,
        bake_textures,
        bake_surfaces,
        key_hits,
        clipmap_conflicts,
    ) = scene.as_ref().map_or((0, 0, 0, 0, 0, 0, 0, 0), |s| {
        (
            s.live_appearances(),
            s.draw.stats.server_objects,
            s.draw.stats.appearance_builds,
            s.draw.stats.appearance_releases,
            s.draw.stats.textures_uploaded,
            s.draw.stats.surfaces_resolved,
            s.draw.stats.texture_key_hits,
            s.draw.stats.clipmap_key_conflicts,
        )
    });
    // The scene's own teardown, so a second replay on the same device starts from the same place
    // this one did. `release_textures` is `WorldScene`'s missing `Drop`.
    let released_at_teardown = scene.as_mut().map_or(0, |s| s.release_textures(gpu));
    let after_teardown = gpu.descriptor_usage().live;
    let dstats = gpu.descriptor_stats();
    let tstats = gpu.texture_table_stats();
    Replayed {
        stopped_at,
        stopped_with,
        duration: t_end - t0,
        replayed: last_t - t0,
        final_live: usage.live,
        peak_live: usage.high_water,
        frontier: usage.frontier,
        capacity: usage.capacity,
        reuses: dstats.reuses,
        exhaustions: dstats.exhaustions,
        final_appearances,
        peak_appearances,
        appearance_builds: builds,
        appearance_releases: releases,
        texture_hits: tstats.hits,
        unknown_releases: tstats.unknown_releases,
        final_objects,
        link_counts,
        frames,
        bake_textures,
        bake_surfaces,
        key_hits,
        clipmap_conflicts,
        trace,
        releases_while_live,
        released_at_teardown,
        after_teardown,
    }
}

fn report(session: &str, r: &Replayed) {
    eprintln!(
        "mesh eviction {session}: {} frames, replayed {:.1} s of {:.1} s, stopped_at {:?} {}\n  \
         descriptor SLOTS (one texture's t0/t1 pair each) out of {} slots = {} descriptors: \
         peak(live) {}, final(live) {}, frontier {}, reuses {}, exhaustions {}\n  \
         appearances: peak {}, final {}, built {}, released {} | texture-table hits {}, \
         unknown releases {} | objects at end {}",
        r.frames,
        r.replayed,
        r.duration,
        r.stopped_at,
        r.stopped_with,
        r.capacity,
        r.capacity * 2,
        r.peak_live,
        r.final_live,
        r.frontier,
        r.reuses,
        r.exhaustions,
        r.peak_appearances,
        r.final_appearances,
        r.appearance_builds,
        r.appearance_releases,
        r.texture_hits,
        r.unknown_releases,
        r.final_objects,
    );
    eprintln!(
        "  bake cache: {} texture slots over {} distinct GroupKeys ({} of them served by \
         the combined-texture cache-hit AddRef arm, {} clip-map key conflicts) | teardown handed back \
         {}, leaving {} live on the device | appearances released WHILE THE PLAYER WAS IN THE \
         WORLD: {}",
        r.bake_textures,
        r.bake_surfaces,
        r.key_hits,
        r.clipmap_conflicts,
        r.released_at_teardown,
        r.after_teardown,
        r.releases_while_live
    );
    if !r.link_counts.is_empty() {
        eprintln!(
            "  appearances still resident, by owner count: {:?}",
            r.link_counts
        );
    }
    for (t, live, app, tex, rel) in &r.trace {
        eprintln!(
            "    t={t:7.1} s  live slots {live:5}  appearances {app:4}  bake textures {tex:5}  \
             released {rel:4}"
        );
    }
}

/// Behaviour: presentation.long-session.object-mesh-slots-stay-bounded
///
/// A full `long-solo-play` replay completes, with peak *and* final occupancy
/// stated in slots.
#[test]
fn a_full_long_solo_play_replay_completes_and_its_peak_occupancy_is_bounded() {
    let store = store();
    let mut gpu = warp();
    let r = replay("long-solo-play", &store, &mut gpu);
    report("long-solo-play", &r);

    // 1. **The premise**, so that "it completed" cannot be satisfied by a replay that never
    //    started. A drive that built no appearance at all would finish trivially and prove nothing.
    assert!(
        r.appearance_builds > 0 && r.peak_appearances > 0,
        "the replay built no object geometry at all, so completing says nothing about eviction"
    );
    assert!(
        r.frames > 8_000,
        "long-solo-play is 8,816 datagrams; only {} frames ran",
        r.frames
    );

    // 2. **The wall is gone.** Not "it got further" -- it reached the last datagram.
    assert_eq!(
        r.stopped_at, None,
        "the scene still stops before the capture does, at t = {:?} s of {:.1} s: {}",
        r.stopped_at, r.duration, r.stopped_with
    );
    assert_eq!(
        r.exhaustions, 0,
        "the descriptor heap was exhausted {} time(s); a caller got `None` from `alloc`",
        r.exhaustions
    );
    assert!(
        (r.replayed - r.duration).abs() < 0.001,
        "the replay stopped at {:.1} s of {:.1} s",
        r.replayed,
        r.duration
    );

    // 3. **The peak, bounded.** A ceiling is the honest shape: the measured peak is about 1,474
    //    slots, and the assertion allows a generous band above it so that a *drift* reddens while
    //    a rebalance between shared and private textures does not. It is not a `>= N` floor —
    //    those fail open.
    assert!(
        r.peak_live < 2_048,
        "peak live occupancy {} slots of {} -- about 1,474 with a departed landblock's links, the \
         keyed upload and the body's replaced mesh set all released, so this is a real growth in \
         what one session holds",
        r.peak_live,
        r.capacity
    );
    assert!(
        r.peak_live > 500,
        "peak live occupancy {} slots -- the landscape bake alone is about 490 pairs at this \
         radius, so this replay is not doing the work the numbers were taken over",
        r.peak_live
    );

    // 4. **The teardown returns it all.** This is the release edge asserted as a *quantity* rather
    //    than as "something was released": the whole appearance half of the cache goes back when
    //    the capture's own log-off tears the object model down, and the scene's `release_textures`
    //    takes the rest. Anything left is a leak with a number on it.
    // **The edge fires during play, not only at the log-off.** Without this the whole file is
    // satisfied by a teardown, which is a different (and much weaker) claim: `release_textures`
    // hands the cache back whatever `release_unlinked_appearances` does. Measured at 10 when this
    // landed — small, and honestly so: `long-solo-play`'s objects mostly arrive and stay.
    assert!(
        r.releases_while_live > 0,
        "no appearance was released while the player was in the world, so nothing here observes \
         the release edge at all -- only the log-off teardown"
    );
    assert_eq!(
        r.final_appearances, 0,
        "the capture logs off; every appearance should be unlinked"
    );
    assert_eq!(
        r.appearance_releases, r.appearance_builds,
        "{} appearances were built and {} released -- the difference is stranded geometry",
        r.appearance_builds, r.appearance_releases
    );
    assert!(
        r.after_teardown <= 4,
        "{} slots still live after the scene was torn down",
        r.after_teardown
    );
    assert!(
        r.reuses > 0,
        "no slot was ever served from the free list, so nothing was released and the replay \
         completed for some other reason"
    );

    // 5. **The releases balance.** A double release would hand the same descriptors to two owners
    //    and is tolerated-and-counted rather than fatal, so it has to be asserted.
    assert_eq!(r.unknown_releases, 0, "a texture slot was released twice");
}

/// **The discrimination.** Deleting the cache would also make the replay complete, and would be a
/// different and worse change: every object would re-bake its own geometry and its own textures.
///
/// So this asserts the cache still *shares*: `long-solo-play` holds far more objects at its peak than it
/// holds distinct appearances, and the texture table serves hits.
#[test]
fn the_appearance_cache_still_shares() {
    let store = store();
    let mut gpu = warp();
    let r = replay("long-solo-play", &store, &mut gpu);
    report("long-solo-play (sharing)", &r);
    assert!(
        r.appearance_builds > u64::try_from(r.peak_appearances).expect("fits"),
        "every appearance was built exactly once and never released ({} builds, peak {}), which \
         is a cache with no release edge",
        r.appearance_builds,
        r.peak_appearances
    );
    assert!(
        r.appearance_releases > 0,
        "nothing was ever released, so the replay completed without eviction"
    );
    // Objects outnumber appearances: the cache is doing its job rather than being bypassed.
    assert!(
        r.peak_appearances < 400,
        "peak distinct appearances {} -- if this approaches the object count the cache is not \
         sharing",
        r.peak_appearances
    );
}

/// The whole locked corpus, as the breadth control: every recording completes, and the peak
/// occupancy of each is reported so a later reader can see the shape rather than one number.
#[test]
fn every_recording_in_the_corpus_replays_to_its_last_datagram() {
    let store = store();
    let mut peaks: BTreeMap<String, u32> = BTreeMap::new();
    let mut stopped: Vec<String> = Vec::new();
    for session in [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "short-play-with-training",
    ] {
        // **A device each.** `DescriptorUsage::high_water` is the *device's* high-water mark, not
        // the scene's, so four replays on one device would report the largest of the four for all
        // four -- which is exactly what the first run of this file printed. An instrument that
        // answers about a different subject.
        let mut gpu = warp();
        let r = replay(session, &store, &mut gpu);
        report(session, &r);
        peaks.insert(session.to_owned(), r.peak_live);
        if r.stopped_at.is_some() {
            stopped.push(format!(
                "{session} at {:?} s: {}",
                r.stopped_at, r.stopped_with
            ));
        }
        assert_eq!(
            r.unknown_releases, 0,
            "{session}: a texture slot was released twice"
        );
    }
    eprintln!("mesh eviction peak live slots by session: {peaks:?}");
    assert!(
        stopped.is_empty(),
        "recordings that did not reach their last datagram: {stopped:?}"
    );
    // The premise again: a corpus that built nothing would pass the loop above in silence.
    assert!(
        peaks.values().any(|p| *p > 0),
        "no recording put a single texture on the device"
    );
}

/// **The working-set measurement**, taken on a heap large enough that the budget cannot be the
/// answer. This is what says whether the release edge *bounds* occupancy or merely postpones the
/// wall: a bounded scene's `live` plateaus while the session goes on, an unbounded one keeps
/// climbing to the last datagram.
#[test]
fn the_working_set_over_a_whole_session_is_measured_on_a_heap_that_cannot_be_the_limit() {
    let store = store();
    let mut gpu = warp_with(Some(1 << 17));
    let r = replay("long-solo-play", &store, &mut gpu);
    report("long-solo-play (large heap)", &r);
    assert_eq!(
        r.stopped_at, None,
        "even a 65,536-slot heap did not hold it: {}",
        r.stopped_with
    );
    assert_eq!(r.exhaustions, 0);
}

/// **Two whole sessions on one device peak at the same number — and what that does NOT prove.**
///
/// This was written as *the* discriminating experiment for the release edge and it is not one.
/// The first version of this file named it in the mutation calibration and the harness aborted
/// **MISCALIBRATED**: deleting `release_unlinked_appearances` outright leaves this test green,
/// because [`replay`] calls `WorldScene::release_textures` at the end of each pass and that hands
/// back the whole cache regardless. Both arms of the comparison then start from the same clean
/// device, and the subject is absent from both. A differential test cannot measure a subject that
/// is absent from both arms.
///
/// It is kept, because what it does establish is worth having and nothing else asserts it: a scene
/// is **fully returnable**, so a client that walks through a portal, logs out and back in, or runs
/// two sessions does not pay twice. The eviction edge itself is asserted in
/// [`a_full_long_solo_play_replay_completes_and_its_peak_occupancy_is_bounded`] by
/// `releases_while_live`, which is the number a deleted release call moves.
#[test]
fn replaying_the_same_session_twice_on_one_device_does_not_double_the_peak() {
    let store = store();
    let mut gpu = warp_with(Some(1 << 17));
    let first = replay("long-solo-play", &store, &mut gpu);
    report("long-solo-play pass 1", &first);
    let second = replay("long-solo-play", &store, &mut gpu);
    report("long-solo-play pass 2", &second);

    // The premise: both passes actually did the work. Two passes that both built nothing would
    // satisfy every ratio below.
    assert!(
        first.peak_live > 1_000 && second.peak_live > 1_000,
        "a pass built almost nothing"
    );
    assert_eq!(first.stopped_at, None);
    assert_eq!(second.stopped_at, None);

    // The measurement. A pure leak would put the second pass at first + second worth of slots;
    // a bounded working set puts it within a few percent of one pass.
    let ratio = f64::from(second.peak_live) / f64::from(first.peak_live);
    eprintln!(
        "mesh eviction wall-or-leak: pass 1 peak {} slots, pass 2 peak {} slots, ratio {ratio:.3}",
        first.peak_live, second.peak_live
    );
    assert!(
        ratio < 1.10,
        "the second pass peaked at {} slots against the first's {} ({ratio:.3}x) -- occupancy is \
         still a function of how long the client has run, not of what it is drawing",
        second.peak_live,
        first.peak_live
    );
}

/// **`long-solo-play` fits a 4,096-descriptor heap.**
///
/// A heap that small once stopped the replay at t = 973 s; with every release edge in place the
/// peak is about 1,474 pairs and the final residency about 327.
///
/// It is deliberately not a `< 2048` restatement of the test above: it runs on the **small
/// heap**, so it fails by *exhaustion* rather than by an assertion on a number.
///
/// **`SRV_HEAP_SIZE` stays larger, and that is a decision rather than an oversight.** 1,474 pairs is a `long-solo-play` replay at `land_radius: 1` with no UI on the device;
/// the shipped client runs a 7x7 window, uploads ~129 UI images and bakes a font atlas per face
/// into the same heap. 574 pairs of headroom over one recording is not a budget, and a heap that
/// is 128 KiB of descriptors either way is not worth a ceiling.
#[test]
fn long_solo_play_fits_the_old_four_thousand_ninety_six_descriptor_budget() {
    let store = store();
    let mut gpu = warp_with(Some(4096));
    let r = replay("long-solo-play", &store, &mut gpu);
    report("long-solo-play (old 4,096-descriptor budget)", &r);
    assert_eq!(r.capacity, 2048, "the old budget is 2,048 pairs");
    assert_eq!(
        r.stopped_at, None,
        "the replay still stops on the old budget, at t = {:?} s of {:.1} s: {}",
        r.stopped_at, r.duration, r.stopped_with
    );
    assert_eq!(
        r.exhaustions, 0,
        "the descriptor heap was exhausted {} time(s)",
        r.exhaustions
    );
    assert!(
        r.peak_live < 2048,
        "peak {} slots of the old 2,048 -- it completed only because nothing asked for the slot \
         that was not there",
        r.peak_live
    );
}

/// A witness that the drive really can see the player and really does build objects, kept apart
/// from the acceptance so that a change to either is visible on its own.
#[test]
fn the_drive_reaches_the_world_and_builds_objects() {
    let store = store();
    let mut gpu = warp();
    let r = replay("first-login-walk-jump", &store, &mut gpu);
    report("first-login-walk-jump (witness)", &r);
    assert!(
        r.frames > 0,
        "the drive never reached a frame with a player"
    );
    assert!(r.appearance_builds > 0, "the drive built no appearance");
    assert!(r.peak_live > 0, "the drive put no texture on the device");
}
