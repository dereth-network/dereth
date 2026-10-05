//! The texture cache keeps each producer's keys in a space of its own, so the UI, the world, the
//! fonts and the solid colours never share an entry even when their sixty-four-bit payloads are
//! equal; and the `BASE1_CLIPMAP` bit stays outside the key, as it does in retail. Fixture: the
//! real key producers, a WARP device, and shipped windows at Holtburg and at `0x8C04`.
//!
//! # The collision
//!
//! Four producers upload through `Gpu::upload_texture_keyed` — the world surfaces, UI images, and
//! both halves of a font atlas — into one table:
//!
//! | producer | high half | low half |
//! |---|---|---|
//! | `world::combined_key` | palette DataID, or 0 | `RenderSurface` DataID |
//! | `world::resolve_surface`'s untextured arm | the packed ARGB colour word | 0 |
//! | `ui_draw::image_key` | an operation hash, or 0 | image DataID |
//! | `Gpu::prepare_ui_fonts` | 0 (glyphs) or 1 (outline) | font DataID |
//!
//! Two of those high halves are unrestricted 32-bit values, so **no bit of the sixty-four is free
//! to tag with**, and the payloads alone are not disjoint. This file demonstrates both overlaps
//! rather than asserting they are possible:
//!
//! 1. `image_key(id, Some(Multiply(c)))` puts `c.rotate_left(3) | 1` in the palette half — a
//!    surjection onto the odd 32-bit words — so for **any** odd palette DataID there is a colour
//!    that reproduces it exactly. One is computed below and the two sixty-four-bit payloads are
//!    asserted equal.
//! 2. A plain UI image keys on `(0, image DID)` and a world texture whose `RenderSurface` carries
//!    no default palette keys on `(0, that RenderSurface DID)`. Both ids are `0x06` surface ids out
//!    of the same dat, so this one needs no arithmetic coincidence at all.
//!
//! # Why the spaces matter here and not in retail
//!
//! Retail shares one table and is right to. Its texture combination takes a render surface and a
//! palette, so two owners that agree on both keys agree on the pixels. A *derived* recolored
//! surface has no DataID: it uses key 0, lands in a private slot, and is shared with nobody, so
//! retail needs no operation hash.
//!
//! This build's UI decode is a **different function** from its world decode
//! (`TextureStore::texture_data` plus `ui_draw::derive`/`composite`, against `texture_data_shifted`
//! with the clip-map flag and a shift palette). So agreement on a key here is not agreement on the
//! pixels, and a hit would show two ways: one surface wearing another's pixels, and — worse, because
//! nothing about it looks like a graphics bug — a use-after-free, `BakeCache::release_group_texture`
//! taking the slot back while `Gpu::ui_textures` still names it.
//!
//! # The spaces, and why they are a proof rather than an argument
//!
//! `dereth_render::TextureKey` carries a `TextureSpace` beside the payload. Two keys from different
//! spaces are unequal **whatever their payloads are**, because `PartialEq` on the struct compares
//! the discriminant first. There is no residual claim about DataID ranges, no bit to run out of,
//! and nothing that a future producer can quietly violate: a new producer cannot reach the table
//! without naming a space. `dereth-render`'s own `descriptor::tests::no_two_spaces_can_produce_equal_keys`
//! is the exhaustive half over payloads; this file is the half that drives the real producers.
//!
//! # The census
//!
//! `TextureTableStats::cross_space_payload_collisions` counts inserts whose payload was already
//! live under a different space. That is the difference between *"the collision is arithmetically
//! possible"* and *"the collision is live in the shipped data"*, which needs a device with both
//! populations on it. It is reported here, not asserted, and its calibration is in
//! `descriptor.rs`.
//!
//! # The clip-map half
//!
//! The clip-map flag is **outside** retail's key and this build leaves it out too: palette-shift
//! restoration passes `(type & BASE1_CLIPMAP)` into combined-texture creation as a **separate
//! argument** and it never reaches the key. So two surfaces sharing a (palette, texture) pair and
//! disagreeing about the bit share one texture in retail as well, and whichever resolved first
//! decides how it was expanded.
//!
//! One of those groups is photographed under both settings. The conflicting groups are sampled
//! with the texture id that produced them and the two settings, and the photograph is taken where
//! the bit actually acts: the palette expansion,
//! `(clip_map && idx <= 7) ? 0x00000000 : palette[idx]`. Decoding the same texture both ways and
//! comparing texel for texel says exactly how many pixels the bit moves, which is a sharper answer
//! than whether a human noticed a difference in a screenshot.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;
use dereth_scene::textures::TextureStore;
use {
    dereth_client_runtime::landblock::load_region, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

use dereth_render::{combined_texture_key, TextureKey, TextureSpace};
use dereth_ui::region::SurfaceOp;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

// ---------------------------------------------------------------------------------------------
// The collision, driven through the real producers
// ---------------------------------------------------------------------------------------------

/// **The witness.** `ui_draw::image_key` and `world::combined_key` really can compute the same
/// sixty-four bits, and the space is what stops them sharing an entry.
///
/// This drives the shipped `image_key` rather than restating its arithmetic, which is the half
/// `dereth-render`'s own test cannot do — that one carries the formula, this one carries the
/// function, and the two agreeing is what says the formula in the other file is still the
/// formula this one uses.
#[test]
fn the_ui_and_world_key_spaces_can_produce_the_same_sixty_four_bits() {
    // A palette DataID out of the `0x04` range, chosen odd because `Multiply`'s hash forces the
    // low bit (deliberately, so that no operation can collide with "no operation").
    let palette_did = 0x0400_0123u32;
    assert_eq!(palette_did & 1, 1, "the witness needs an odd palette id");
    let texture_did = 0x0600_4567u32;
    let world = TextureKey::world(combined_texture_key(palette_did, texture_did));

    // Invert `c.rotate_left(3) | 1 == palette_did`.
    let c = palette_did.rotate_right(3);
    let ui =
        dereth_client_shell::ui_draw::image_key(DataId(texture_did), Some(SurfaceOp::Multiply(c)));

    eprintln!(
        "key spaces witness: world key {:#018x} ({:?}), UI key {:#018x} ({:?})",
        world.raw(),
        world.space(),
        ui.raw(),
        ui.space()
    );
    assert_eq!(
        ui.raw(),
        world.raw(),
        "the two producers' sixty-four-bit payloads must collide, or this file is demonstrating \
         nothing -- `image_key`'s Multiply hash has changed and the witness needs re-deriving"
    );
    assert_ne!(ui, world, "and the tagged keys must not be equal");
    assert_eq!(ui.space(), TextureSpace::Ui);
    assert_eq!(world.space(), TextureSpace::World);

    // The second, arithmetic-free case: a plain image and an unpalettised world texture.
    let plain = dereth_client_shell::ui_draw::image_key(DataId(texture_did), None);
    let unpalettised = TextureKey::world(combined_texture_key(0, texture_did));
    assert_eq!(
        plain.raw(),
        unpalettised.raw(),
        "a plain UI image and a world texture with no default palette key on the same bits"
    );
    assert_ne!(plain, unpalettised);

    // And the font sheets, which are the fourth producer.
    let font = TextureKey::font(0, texture_did);
    assert_eq!(
        font.raw(),
        plain.raw(),
        "a glyph sheet keys on `(0, DID)` too"
    );
    assert_ne!(font, plain);
    assert_ne!(font, unpalettised);
    assert_ne!(
        font,
        TextureKey::font(1, texture_did),
        "glyphs and outline are two textures"
    );
}

/// Behaviour: rendering.textures.ui-and-world-producers-never-share-a-cache-key
/// **Every producer this client has, put in one table, with the payloads deliberately made to
/// collide.**
///
/// The premise is the first assertion and it is the one that matters: within a single space the
/// payload *does* hit, so a green run cannot mean the table has simply stopped caching. Then each
/// of the other three spaces is offered the same payload and must miss.
///
/// The release half is asserted too, because it is the worse of the two failures: taking a shared
/// entry's key out of the table when one owner reaches zero would leave the other owner holding a
/// slot the allocator had handed back.
#[test]
fn one_payload_offered_to_every_producer_takes_four_slots() {
    let mut gpu = crate::common::test_gpu(320, 240);
    let one_texel = |b: u8| dereth_primitives::TextureData {
        width: 1,
        height: 1,
        format: dereth_primitives::TextureFormat::Bgra8,
        levels: vec![vec![b, b, b, 0xFF]],
    };
    let bits = combined_texture_key(0x0400_0123, 0x0600_4567);
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the solid-colour producer keys on a 32-bit colour word by construction.
    let low = bits as u32;
    let keys = [
        TextureKey::world(bits),
        TextureKey::ui(bits),
        TextureKey::font(0, low),
        TextureKey::solid_color(low),
    ];
    let before = gpu.descriptor_usage().live;
    let mut slots = Vec::new();
    for (i, k) in keys.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: four iterations.
        let s = gpu
            .upload_texture_keyed(*k, &one_texel(i as u8))
            .expect("the upload succeeds");
        slots.push(s);
    }
    // The premise: the same key in the same space is a hit and costs nothing.
    let again = gpu
        .upload_texture_keyed(keys[0], &one_texel(0))
        .expect("the second upload of the same key");
    assert_eq!(
        again, slots[0],
        "a second upload in the SAME space must share the texture"
    );
    gpu.release_texture(again);

    assert_eq!(
        gpu.descriptor_usage().live - before,
        4,
        "four producers offered one payload took {} slot(s); they must take four",
        gpu.descriptor_usage().live - before
    );
    for (i, a) in slots.iter().enumerate() {
        for (j, b) in slots.iter().enumerate() {
            assert!(
                i == j || a != b,
                "{:?} and {:?} share a slot",
                keys[i],
                keys[j]
            );
        }
    }
    // **The census saw all of it, and the number is 2 rather than 3 for a reason worth stating.**
    // The four keys above are not four copies of one payload: `World` and `Ui` hold the full
    // sixty-four bits, while `Font(0, low)` and `SolidColor(low)` both hold `low` widened —
    // `combined_texture_key(0, low)` *is* `low as u64`. So there are two payloads, each held by
    // two spaces, and each contributes exactly one collision. The equality pins which pairs
    // actually overlap, which is the thing the counter is for.
    assert_eq!(
        gpu.texture_table_stats().cross_space_payload_collisions,
        2,
        "the cross-space census read {} for two payloads each held by two of the four spaces",
        gpu.texture_table_stats().cross_space_payload_collisions
    );
    // Releasing one to zero must leave the other three alone.
    gpu.release_texture(slots[0]);
    let after = gpu
        .upload_texture_keyed(keys[1], &one_texel(1))
        .expect("still cached");
    assert_eq!(
        after, slots[1],
        "the UI entry went with the world entry's release"
    );
    gpu.release_texture(after);
    for s in slots.iter().skip(1) {
        gpu.release_texture(*s);
    }
    assert_eq!(gpu.descriptor_usage().live, before, "everything came back");
    assert_eq!(gpu.texture_table_stats().unknown_releases, 0);
    assert_eq!(gpu.descriptor_stats().invalid_releases, 0);
}

/// **The census over a shipped scene.** How many payloads a real device actually holds in more
/// than one space.
///
/// Reported rather than asserted: it is a fact about the shipped data and about which populations
/// happen to be on the device at once, and pinning it would be the bound-pinned-to-a-number defect.
/// What *is* asserted is the **denominator**: the per-space counts are
/// printed on the same line as the collision count and the two world-side spaces are required to be
/// non-empty, because a census over a device carrying one space reads zero for the most boring
/// possible reason and that zero looks exactly like a clean bill. The UI and font spaces are
/// asserted **empty** here rather than glossed over — this scene has no UI on it, and saying so is
/// what makes the zero a bounded reading instead of a claim about the client.
#[test]
fn the_cross_space_census_over_a_shipped_window_is_reported_with_its_denominator() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    let cfg = SceneConfig {
        land_radius: 2,
        scenery_radius: 2,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    let region = load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let keys = gpu.texture_keys();
    let mut per_space = [0usize; 4];
    for k in &keys {
        per_space[match k.space() {
            TextureSpace::World => 0,
            TextureSpace::SolidColor => 1,
            TextureSpace::Ui => 2,
            TextureSpace::Font => 3,
        }] += 1;
    }
    let collisions = gpu.texture_table_stats().cross_space_payload_collisions;
    eprintln!(
        "key spaces census over a 5x5 window at Holtburg with a body: {} keyed textures -- World {}, \
         SolidColor {}, Ui {}, Font {} -- and {collisions} payload(s) held in more than one space",
        keys.len(),
        per_space[0],
        per_space[1],
        per_space[2],
        per_space[3]
    );
    // The denominator. Without it, "0 collisions" and "only one space is populated" print alike.
    assert!(
        per_space[0] > 0,
        "the device holds no World textures at all"
    );
    assert!(
        per_space[1] > 0,
        "the device holds no SolidColor texels, so the census covers one space and its answer is \
         about nothing"
    );
    assert_eq!(
        per_space[2] + per_space[3],
        0,
        "the loaded scene alone has no UI on it, which is why the second half of this test exists"
    );

    // **The second half: the collision reproduced on a device, with shipped data.**
    //
    // Every `World` key whose palette half is zero is an unpalettised texture keyed on
    // `(0, RenderSurface DID)`. `ui_draw::image_key(id, None)` computes `(0, image DID)`. Both ids
    // are `0x06`/`0x08` surface ids out of the same dat, so the two are the *same sixty-four bits*
    // whenever the same surface is used in both roles — no arithmetic coincidence required.
    //
    // This uploads those very ids through the **UI** producer's key on the **same device**, which
    // is exactly what `Gpu::prepare_ui` does for a plain image, and counts what a single shared
    // `u64` map would have merged. It is a constructed overlap rather than a census of shipped UI
    // *usage*: it establishes that the two roles collide on real surfaces, not that any particular
    // screen puts one there. Saying which of the two this measures is the point.
    let textures = TextureStore::new(&store);
    let unpalettised: Vec<u32> = keys
        .iter()
        .filter(|k| k.space() == TextureSpace::World && (k.raw() >> 32) == 0)
        .map(|k| {
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the low half of a world key is a 32-bit DataID by construction.
            let id = k.raw() as u32;
            id
        })
        .collect();
    // Capped, because the point is that the overlap exists rather than how many of it there is,
    // and every one of these is a real texture upload on a real device. The full count is
    // reported beside the sample so the two cannot be confused.
    const SAMPLE: usize = 32;
    let sample: Vec<u32> = unpalettised.iter().copied().take(SAMPLE).collect();
    let before = collisions;
    let mut offered = 0usize;
    let mut ui_slots = Vec::new();
    for id in &sample {
        let Ok(data) = textures.texture_data(DataId(*id)) else {
            continue;
        };
        let key = dereth_client_shell::ui_draw::image_key(DataId(*id), None);
        assert_eq!(key.space(), TextureSpace::Ui);
        let Ok(slot) = gpu.upload_texture_keyed(key, &data) else {
            continue;
        };
        ui_slots.push(slot);
        offered += 1;
    }
    let now = gpu.texture_table_stats().cross_space_payload_collisions;
    eprintln!(
        "key spaces constructed overlap: {} of the {} World keys are unpalettised `(0, surface id)`; \
         {} of them were sampled, {offered} decoded and were uploaded again through the UI \
         producer's key on the same device, and the census went {before} -> {now}",
        unpalettised.len(),
        per_space[0],
        sample.len()
    );
    assert!(
        offered > 0,
        "not one unpalettised world surface could be decoded through the UI path, so the overlap \
         was never offered and the reading below is about nothing"
    );
    assert_eq!(
        now - before,
        u64::try_from(offered).expect("fits"),
        "{offered} surfaces were held in both the World and Ui spaces and the census counted \
         {} of them",
        now - before
    );
    for s in ui_slots {
        gpu.release_texture(s);
    }
    scene.release_textures(&mut gpu);
    assert_eq!(gpu.texture_table_stats().unknown_releases, 0);
    assert_eq!(gpu.descriptor_stats().invalid_releases, 0);
}

// ---------------------------------------------------------------------------------------------
// The clip-map photograph
// ---------------------------------------------------------------------------------------------

/// **One `clipmap_key_conflict` group, photographed under both settings.**
///
/// The bit acts in exactly one place — the palette expansion,
/// `(clip_map && idx <= 7) ? 0x00000000 : palette[idx]`.
/// The photograph is therefore taken at that expansion boundary:
/// decode the conflicting group's texture with `clip_map` false and again with it true, and
/// compare texel for texel. That is a sharper instrument than a screen capture, because it
/// measures how many pixels the bit moves rather than whether a human noticed, and because it
/// isolates the bit from every other thing on screen.
///
/// **The premise is asserted first**: the scene must actually produce a conflict, and the decode
/// must actually produce pixels. A comparison of two empty buffers agrees perfectly.
///
/// # Why the station is `0x8C04`
///
/// Objects are built only on full-detail blocks, as in retail: dynamic-object, building and
/// static-object initialization each require the mesh's side-cell count to equal eight for full
/// detail, so a block off the 8-cell ring is bare terrain. At Holtburg the one conflicting pair
/// lives in ring-2 scenery that retail never grows: with `lod_object_guard` lifted the device holds
/// **315** World textures and produces **1** conflict (surface `0x080011F0`), and with the shipped
/// guard on it holds **302** and produces **0**.
///
/// `0x8C04`'s *full-detail core* reaches eight conflicts, `0x080011F0` among them, so the
/// photograph is of the same surface (30,195 of 65,536 texels) and is taken in the configuration
/// the client actually ships.
#[test]
fn a_clipmap_key_conflict_is_photographed_under_both_settings() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    // Holtburg's conflict is ring-2 scenery and retail does not grow it; see the doc comment. `lod_object_guard` stays at its shipped default, so this window is the one the
    // client draws.
    let cfg = SceneConfig {
        landblock: 0x8C04,
        land_radius: 2,
        scenery_radius: 2,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    let region = load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let conflicts: Vec<_> = scene.clipmap_conflicts().to_vec();
    eprintln!(
        "key spaces clip-map: {} conflict(s) counted over this window, {} sampled",
        scene.draw.stats.clipmap_key_conflicts,
        conflicts.len()
    );
    assert!(
        scene.draw.stats.clipmap_key_conflicts > 0,
        "this window produced no BASE1_CLIPMAP key conflict, so there is nothing to photograph -- \
         and every reading below would be about an empty list. If this fires, \
         the first question is whether the conflicting population is still baked at all rather than \
         whether the key spaces changed. A conflict needs two surface records sharing one (palette, \
         texture) pair and disagreeing about `BASE1_CLIPMAP`, and the only things that carry them \
         are scenery, statics and buildings -- which exist only on full-detail blocks. Re-run with \
         `lod_object_guard: false` to see whether the conflict is merely out of the shipped \
         window's reach (as it is at Holtburg), and move the station rather \
         than lifting the guard"
    );
    assert!(
        !conflicts.is_empty(),
        "conflicts were counted but none was sampled"
    );

    let textures = TextureStore::new(&store);
    let mut photographed = 0usize;
    for c in &conflicts {
        assert_ne!(
            c.first_clip_map, c.second_clip_map,
            "a recorded conflict whose two settings agree is not a conflict"
        );
        let Some(id) = c.texture else { continue };
        let (Ok(off), Ok(on)) = (
            textures.texture_data_shifted(id, false, None),
            textures.texture_data_shifted(id, true, None),
        ) else {
            continue;
        };
        assert_eq!(
            (off.width, off.height),
            (on.width, on.height),
            "the bit changed the extent"
        );
        assert!(
            !off.levels.is_empty() && !off.levels[0].is_empty(),
            "the decode produced nothing"
        );
        let a = &off.levels[0];
        let b = &on.levels[0];
        assert_eq!(
            a.len(),
            b.len(),
            "the two decodes produced different byte counts"
        );
        // BGRA8; count differing texels rather than differing bytes, and record the largest
        // per-channel excursion so "different" is quantified rather than merely reported.
        let mut differing = 0usize;
        let mut worst = 0u8;
        #[allow(clippy::chunks_exact_to_as_chunks)]
        // LINT-OK: BGRA8, four bytes a texel; `chunks_exact` reads as the pixel loop it is.
        for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
            if pa != pb {
                differing += 1;
                for (x, y) in pa.iter().zip(pb.iter()) {
                    worst = worst.max(x.abs_diff(*y));
                }
            }
        }
        let texels = a.len() / 4;
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: a percentage for a log line.
        let pct = 100.0 * differing as f64 / texels.max(1) as f64;
        eprintln!(
            "key spaces photograph: surface {:?} texture {id:?} {}x{} ({texels} texels) -- first group \
             expanded with clip_map={}, second wanted {} -- {differing} texel(s) differ \
             ({:.3}%), largest per-channel difference {worst}",
            c.surface, off.width, off.height, c.first_clip_map, c.second_clip_map, pct
        );
        photographed += 1;
    }
    assert!(
        photographed > 0,
        "not one of the {} sampled conflicts could be decoded under both settings, so nothing was \
         photographed and this test proved only that the list is non-empty",
        conflicts.len()
    );
    scene.release_textures(&mut gpu);
}
