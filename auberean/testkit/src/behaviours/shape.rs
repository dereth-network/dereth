//! Shape: the detection shapes' tests against one another, held to the September 2013 client's own
//! answers.
//!
//! A row claims what the September 2013 client answers for a shape test.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[behaviour! {
    id: "shape.detect.every-shape-test-answers-as-the-client-does",
    says: "Cones, shells, spheres, upright cylinders, boxes and rays answer every test of one \
           against another as the client does, case for case: on the boundary, at no radius, \
           inside one another, off every angle and with a NaN; a cone's cosine and a shell's \
           heights are stored to the bit, a ray's fraction is given to the bit, and the box \
           around a shape carried into another position's frame has the same corners to the bit.",
    since: RETAIL,
    evidence: Evidence::Private("AC2-EVID-SHAPE-ORACLE"),
    station: "auberean-physics::cpu::detect::every_shape_test_answers_as_the_client_does",
    tier: Tier::Cpu,
}];
