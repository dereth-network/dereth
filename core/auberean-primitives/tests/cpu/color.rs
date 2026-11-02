use auberean_primitives::math::{are_float_equal, is_float_zero, Rgba};

use super::color_cases;
use super::oracle;

/// Colors capped low and high, scaled to their brightest channel, checked for lying within
/// 0 to 1, compared within 0.0002 a channel and packed into one word answer as the September
/// 2013 client's own code does, to the bit; so do its tests of a float near zero and of two
/// floats near each other.
///
/// Behaviour: geometry.color.every-operation-answers-as-the-client-does
#[test]
fn every_color_operation_answers_as_the_client_does() {
    oracle::check(color_cases::CASES, |label, x, inp| {
        let c = |i: usize| Rgba {
            r: x[i],
            g: x[i + 1],
            b: x[i + 2],
            a: x[i + 3],
        };
        let cb = |c: Rgba| vec![c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()];
        let out = match label {
            "cap_low" => {
                let mut a = c(0);
                a.cap_low();
                cb(a)
            }
            "cap_high" => {
                let mut a = c(0);
                a.cap_high();
                cb(a)
            }
            "normalize" => {
                let mut a = c(0);
                a.normalize();
                cb(a)
            }
            "is_valid" => vec![u32::from(c(0).is_valid())],
            "same" => vec![u32::from(c(0).same(&c(4)))],
            "pack" => {
                let mut buf = [0x55_u8; 16];
                let n = c(0).pack_into(&mut buf[..inp[4] as usize]);
                let word = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                let moved = if inp[4] < 4 { 0 } else { 4 };
                vec![u32::try_from(n).unwrap_or(0), word, moved]
            }
            "is_float_zero" => vec![u32::from(is_float_zero(x[0], x[1]))],
            "are_float_equal" => vec![u32::from(are_float_equal(x[0], x[1], x[2]))],
            _ => return None,
        };
        Some(out)
    });
}
