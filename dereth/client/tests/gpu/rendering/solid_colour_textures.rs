//! Solid-colour surfaces: retail rewrites one device-wide texel before each untextured draw,
//! while this build bakes one 1x1 texture per distinct colour word (a declared, pixel-identical
//! difference, counted by `BakeCache::solid_colour_words`). The difference is bounded: a colour
//! word is a pure function of two fields of the surface record (translucency and colour value),
//! asserted by evaluating every untextured surface in the dat under a spread of contexts, so the
//! whole game can produce no more words than the dat holds (at most 146, against a descriptor
//! heap of 65,536). A session reaches a subset of that and a second lap over the same blocks adds
//! none. `solid_texels_uploaded` counts uploads, not words: a released block's colour is uploaded
//! again when it returns, so the two must differ. Fixture: every surface record in
//! `client_portal.dat`, and a two-lap walk over Holtburg's blocks on a software device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::SceneWrites;
use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_assets::Decode;
use dereth_client::character::CharacterInput;
use dereth_client::world::{block_xy, load_region, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::surface::{Surface as RenderState, SurfaceHandler};
use dereth_render::{PipelineKey, SurfaceContext, VertexFormat};

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The `Surface` `resolve_surface` builds out of a decoded surface record, so that this file
/// evaluates the client's own resolution rather than a second copy of it.
fn render_state(s: &dereth_assets::Surface) -> RenderState {
    RenderState {
        r#type: s.surface_type,
        handler: SurfaceHandler::Database,
        color_value: s.color_value.unwrap_or(0),
        translucency: s.translucency,
        luminosity: s.luminosity,
        diffuse: s.diffuse,
    }
}

/// `resolve_surface`'s own context for an untextured surface, plus the knobs that vary between
/// call sites. The point of the sweep is that **none** of them may change the answer.
fn contexts() -> Vec<SurfaceContext> {
    let base = SurfaceContext {
        vertex_format: VertexFormat::XyzDiffuseTex1,
        texture_is_set: false,
        lighting: false,
        ..SurfaceContext::default()
    };
    let mut out = Vec::new();
    for tiled in [false, true] {
        for two_sided in [false, true] {
            for lighting in [false, true] {
                for material_has_alpha in [None, Some(true), Some(false)] {
                    out.push(SurfaceContext {
                        tiled,
                        two_sided,
                        lighting,
                        material_has_alpha,
                        ..base
                    });
                }
            }
        }
    }
    out
}

/// The census of the space `RetailDatStore::ids_of(DbType::Surface)` covers.
///
/// **The space is named because a ceiling is a claim about a population**, and this one is
/// `0x0800_0000..=0x0800_FFFF` in `client_portal.dat` plus the high-res partition —
/// every id the client can address as a surface. Records that will not read or will not decode
/// are **counted, not skipped**: an unexaminable record is the difference between *"nobody checked"*
/// and *"checking confirmed"*, and a census that swallows them returns a confident ceiling that is
/// too low.
///
/// Returns `(ids seen, untextured, unexaminable, the distinct colour words)`.
fn dat_colour_words(store: &RetailDatStore) -> (usize, usize, usize, BTreeSet<u32>) {
    let ids = store.ids_of(DbType::Surface);
    let ctxs = contexts();
    let mut untextured = 0usize;
    let mut unexaminable = 0usize;
    let mut words = BTreeSet::new();
    for id in &ids {
        let Ok(bytes) = store.read_typed(DbType::Surface, *id) else {
            unexaminable += 1;
            continue;
        };
        let Ok(s) = dereth_assets::Surface::decode_payload(*id, &bytes) else {
            unexaminable += 1;
            continue;
        };
        // `surface_type & 6 != 0` is the textured form; the decoder reports it as
        // `color_value: None`. Only the untextured ones reach the solid-colour arm.
        if s.color_value.is_none() {
            continue;
        }
        untextured += 1;
        let state = render_state(&s);
        let mut answer: Option<u32> = None;
        for c in &ctxs {
            let got = PipelineKey::state_from_surface(&state, *c)
                .solid_color
                .expect("an untextured surface always has a solid colour");
            match answer {
                None => answer = Some(got),
                Some(a) => assert_eq!(
                    a, got,
                    "surface {id:?} produced two different colour words under two contexts \
                     ({a:#010x} and {got:#010x}) -- `solid_color` is not a pure function of the \
                     record after all, and the whole bound in this file is void"
                ),
            }
        }
        words.insert(answer.expect("at least one context"));
    }
    (ids.len(), untextured, unexaminable, words)
}

/// **The ceiling.** How many distinct colour words the shipped dat can produce at all, and the
/// independence claim the bound rests on, asserted rather than read off the source.
///
/// The independence is the load-bearing half. If `solid_color` depended on anything dynamic the
/// count below would be a coincidence of this walk rather than a bound, and whether the
/// difference is bounded or a leak could not be answered from a census of the dat at all.
#[test]
fn the_colour_word_is_a_function_of_the_record_alone_and_the_dat_bounds_it() {
    let store = store();
    let (all, untextured, unexaminable, words) = dat_colour_words(&store);
    eprintln!(
        "solid-colour ceiling: {all} surface-record ids in `client_portal.dat`'s 0x08 range, {untextured} of \
         them untextured and {unexaminable} unexaminable (would not read or would not decode), \
         producing {} distinct colour words under {} different SurfaceContexts",
        words.len(),
        contexts().len()
    );
    // The denominator, so that "N distinct words" cannot be a report on an empty census.
    assert!(
        all > 0,
        "the dat holds no surface records at all -- the census read nothing"
    );
    assert!(
        untextured > 0,
        "not one of the {all} surface records is untextured, so the solid-surface setup arm is \
         unreachable and this whole module is about nothing"
    );
    assert!(!words.is_empty());
    // The bound is *derived from the population* rather than pinned to today's figure: it cannot
    // go stale when the dat changes, and it still
    // says the thing that matters.
    assert!(
        words.len() <= untextured,
        "{} distinct colour words from {untextured} untextured surfaces: a word is a function of \
         one record, so there cannot be more words than records",
        words.len()
    );
    // And it must actually be a memo rather than an identity: if every untextured surface had its
    // own word the difference would cost one texture per surface and the colour-word key would
    // be buying nothing.
    assert!(
        words.len() < untextured,
        "every one of the {untextured} untextured surfaces has a distinct colour word, so the \
         colour-word key shares nothing and this deviation is one texture per surface after all"
    );
    // **The blind spot, reported rather than skipped.** Any record this census could not read is a
    // record whose colour word is unknown, so the ceiling would be a lower bound rather than a
    // ceiling. Asserted at zero because it is zero on the shipped dat; if it ever is not, the
    // failure names the number instead of silently shrinking the population.
    assert_eq!(
        unexaminable,
        0,
        "{unexaminable} of {all} surface records could not be read or decoded, so the {} words \
         above are a LOWER BOUND on the ceiling and not the ceiling",
        words.len()
    );
}

/// Behaviour: rendering.textures.solid-colour-textures-stay-bounded-per-session
/// **What a session actually reaches**, against that ceiling.
///
/// Two drives, because one is not a measurement: a settled 5x5 window, and then a 24-station
/// two-lap walk over the same blocks. The second lap must add **nothing** — a colour word is a
/// function of a shipped record, so a walk that re-visits its own blocks cannot invent one — and
/// that equality is what distinguishes a bounded memo from a count that grows with time played.
#[test]
fn a_session_reaches_a_small_fraction_of_the_ceiling_and_stops_growing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    let (_, untextured, _, ceiling) = dat_colour_words(&store);

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
    let at_load = scene.draw.stats.solid_colour_words;
    let uploads_at_load = scene.draw.stats.solid_texels_uploaded;

    // The lap, twice, so the second is the measurement.
    const LAP: [(i32, i32); 12] = [
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 5),
        (6, 6),
        (5, 5),
        (4, 4),
        (3, 3),
        (2, 2),
        (1, 1),
        (0, 0),
    ];
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    // `(distinct colour words, 1x1 uploads)` at the end of each lap. **Two numbers, because they
    // are two things**: the second counts events and rises with block churn because a departed
    // block's surfaces are freed, and reading it as "distinct colour words" would report a
    // session exceeding a ceiling that cannot be exceeded (218 uploads against a 146-word dat).
    let mut after_lap = [(0usize, 0u32); 2];
    for (lap, slot) in after_lap.iter_mut().enumerate() {
        for &(dx, dy) in &LAP {
            let (bx, by) = (hx + dx, hy + dy);
            assert!((0..=0xFE).contains(&bx) && (0..=0xFE).contains(&by));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: both bounded to 0..=0xFE above. Not a float conversion.
            let block = LandblockId(((bx as u16) << 8) | (by as u16));
            let land = Arc::clone(scene.character.as_ref().expect("a body").land());
            let z = land.ground_height(block, 96.0, 96.0).unwrap_or(0.0) + 1.0;
            scene
                .character
                .as_mut()
                .expect("a body")
                .teleport(Position::new(
                    block.cell(1),
                    Frame::new(Vec3::new(96.0, 96.0, z), Quat::IDENTITY),
                ));
            scene.follow_character_now();
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(0.0),
                0.0,
            );
            scene
                .stream(&store, &mut gpu)
                .expect("the streamed blocks build");
        }
        *slot = (
            scene.draw.stats.solid_colour_words,
            scene.draw.stats.solid_texels_uploaded,
        );
        eprintln!(
            "solid colour after lap {}: {} distinct colour words, {} 1x1 uploads",
            lap + 1,
            slot.0,
            slot.1
        );
    }

    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a percentage for a log line.
    let pct = 100.0 * after_lap[1].0 as f64 / ceiling.len() as f64;
    eprintln!(
        "solid-colour session: {at_load} distinct colour words at the load ({uploads_at_load} uploads), \
         {} after lap 1 ({} uploads), {} after lap 2 ({} uploads), against a whole-dat ceiling of \
         {} words over {untextured} untextured surface records ({pct:.1}% of the ceiling). \
         Retail would have used ONE texture for all of them.",
        after_lap[0].0,
        after_lap[0].1,
        after_lap[1].0,
        after_lap[1].1,
        ceiling.len()
    );

    // The premise: the deviation is reachable at all. A session that never drew an untextured
    // surface would satisfy every bound below for the most boring possible reason.
    assert!(
        at_load > 0,
        "the load window took no solid-colour texel at all, so nothing here is a measurement of \
         this deviation"
    );
    // **The measurement.** A colour word is a function of a shipped surface record, so re-walking the
    // same blocks cannot invent one. This equality is what separates "bounded memo" from "grows
    // with time played", and it is stated on the *words*, not on the uploads.
    assert_eq!(
        after_lap[1].0, after_lap[0].0,
        "lap 2 saw {} distinct colour words against lap 1's {}: the population is growing with \
         time played rather than with the surfaces on screen, which would make this a leak and \
         not a deviation",
        after_lap[1].0, after_lap[0].0
    );
    // And the session is inside the whole-dat ceiling, which is the bound *derived from the
    // population* in the test above rather than a number pinned here.
    assert!(
        after_lap[1].0 <= ceiling.len(),
        "a session produced {} distinct colour words against a whole-dat ceiling of {}",
        after_lap[1].0,
        ceiling.len()
    );
    // **The two counters are not the same number, and the walk proves it rather than the doc
    // comment claiming it.** Uploads exceed words because the release edge frees a colour
    // word whose last group departs and the next block to want it uploads it again. If these were
    // ever equal, one of the two counters would be measuring the other.
    assert!(
        after_lap[1].1 > after_lap[1].0.try_into().expect("fits"),
        "{} uploads for {} distinct words: with a release edge in place, a walk that re-enters \
         its own blocks must re-upload -- and if it does not, `solid_texels_uploaded` is counting \
         distinct words rather than uploads",
        after_lap[1].1,
        after_lap[1].0
    );
    scene.release_textures(&mut gpu);
}
