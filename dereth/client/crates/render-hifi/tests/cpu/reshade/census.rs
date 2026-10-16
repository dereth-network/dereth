//! Behaviour: none (experimental Horizon rendering effects: the census view's tints, which tell an
//! art pack's models from the world's own)

use crate::common::wgsl_eval::{Evaluator, Value};

#[test]
fn the_census_tints_an_art_packs_leaves_and_lit_surfaces_apart_from_the_worlds_own() {
    let census = Evaluator::new(&dereth_render_hifi::reshade::shaders()[0]);
    let tint = |class: f32| census.call("class_colour", &[Value::F32(class)]).floats();
    // The world's own leaves, whatever sun they carry, share one tint; an art pack's leaves,
    // whatever they carry, another.
    let world_leaf = tint(4.25);
    assert_eq!(tint(4.0), world_leaf);
    let pack_leaf = tint(5.5);
    assert_ne!(pack_leaf, world_leaf);
    assert_eq!(tint(5.0 + 255.0 / 256.0), pack_leaf);
    // The world's own lit surfaces, with the sun among their lights or not, share one tint; an art
    // pack's lit surfaces, a dielectric or a metal, another.
    let world_lit = tint(1.0);
    assert_eq!(tint(1.25), world_lit);
    let pack_lit = tint(1.1);
    assert_ne!(pack_lit, world_lit);
    assert_eq!(tint(1.4), pack_lit);
    assert_ne!(pack_lit, pack_leaf);
}
