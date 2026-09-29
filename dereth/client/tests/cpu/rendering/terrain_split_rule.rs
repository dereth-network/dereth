//! The terrain's diagonal split rule, checked between the renderer's mesh and the physics
//! heightfield. Fixture: none; both crates' own implementations over every global cell. The test
//! lives in the client because the client is the crate that links both.

/// Behaviour: terrain.split.drawn-and-walked-triangles-use-one-diagonal-rule
/// The terrain diagonal split decides both which triangles are drawn and which triangles you stand
/// on. The client uses **one** rule for both, so if rendering and physics ever disagree the player
/// sees ground that is not there — and the disagreement would be invisible to either crate's own
/// test suite, because each is internally consistent.
///
/// This is the check `docs/CORRECTIONS.md` warns about: ACE's mesh code and its physics code
/// compute different expressions, so ACE disagrees with itself on exactly this question.
#[test]
fn the_terrain_split_rule_agrees_between_rendering_and_physics() {
    // 0..255 in both axes covers every landblock; the hash takes global cell coordinates, so sweep
    // a dense grid of them rather than sampling.
    let mut checked = 0u64;
    let mut sw_to_ne = 0u64;
    for x in 0..2040i32 {
        for y in 0..2040i32 {
            let render = dereth_world_render::land::mesh::sw_to_ne_cut(x, y);
            let physics = dereth_physics::land::split_hash(x as u32, y as u32);
            assert_eq!(
                render, physics,
                "split rule disagrees at global cell ({x}, {y}): \
                 rendering says {render}, physics says {physics}"
            );
            checked += 1;
            if render {
                sw_to_ne += 1;
            }
        }
    }
    assert_eq!(checked, 2040 * 2040);
    // Sanity: the hash should be close to balanced. A constant answer would pass the equality check
    // above while being obviously wrong.
    let ratio = sw_to_ne as f64 / checked as f64;
    assert!(
        (0.45..0.55).contains(&ratio),
        "split ratio {ratio} is not plausibly balanced; one side may be stubbed"
    );
}
