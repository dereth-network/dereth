//! The landscape-normal pass takes the landscape however the world drew it: unlit from its
//! composite, or blended by the splat.

use dereth_render_hifi::derive::reshade::{MaterialClass, WITHOUT_SUN};
use dereth_render_hifi::reshade;

use crate::common::wgsl_eval::{Evaluator, Value};

/// Behaviour: hifi.reshade.the-landscape-normal-pass-takes-the-composite-and-the-splat
/// As the landscape-normal shader decides it, a pixel the world left with the class of the
/// landscape drawn from its composite or blended by the splat may be the landscape; a pixel of
/// any lit class (lit by the sun or only by the lights near it, an interior cell, a leaf) keeps
/// its own normal.
#[test]
fn the_landscape_normal_pass_takes_the_composite_and_the_splat_and_nothing_lit() {
    let shader = Evaluator::new(&reshade::shaders()[1]);
    let taken = |w: f32| shader.call("drawn_as_landscape", &[Value::F32(w)]).bool();
    for (what, w) in [
        ("the composite's landscape", MaterialClass::Unlit.encoded()),
        ("the splat's landscape", MaterialClass::Terrain.encoded()),
    ] {
        assert!(taken(w), "{what} ({w}) was not taken");
    }
    let foliage = MaterialClass::Foliage.encoded();
    for (what, w) in [
        ("a thing lit by the sun", MaterialClass::Lit.encoded()),
        (
            "a thing lit without the sun",
            MaterialClass::Lit.encoded() + WITHOUT_SUN,
        ),
        ("an interior cell", MaterialClass::Baked.encoded()),
        ("a leaf", foliage),
        ("a leaf drawn in shade", foliage + 0.1),
        ("a leaf drawn in the sun", foliage + 0.4),
    ] {
        assert!(!taken(w), "{what} ({w}) was taken as the landscape");
    }
}
