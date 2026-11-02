//! How many of a picture list's levels a texture takes.

use auberean_draw::picture::{dropped_levels, Image};

/// Texture detail from lowest to highest leaves out the four, three, two or one largest levels of
/// a picture list, and none at the highest when its high detail is kept, but never every level.
///
/// Behaviour: draw.picture.texture-detail-leaves-out-the-largest-levels
#[test]
fn texture_detail_leaves_out_the_largest_levels() {
    let nine: Vec<usize> = (0..=4).map(|d| dropped_levels(d, true, 9)).collect();
    assert_eq!(nine, [4, 3, 2, 1, 0]);
    assert_eq!(dropped_levels(4, false, 9), 1);
    assert_eq!(dropped_levels(0, true, 3), 2);
    assert_eq!(dropped_levels(0, true, 1), 0);
}

/// A texture at a lower detail starts at a smaller stored level: a 64-texel picture list of seven
/// levels is a 32-texel texture of six at high detail and a 4-texel one of three at very low; a
/// single picture stays whole.
///
/// Behaviour: draw.picture.a-texture-at-a-lower-detail-starts-at-a-smaller-level
#[test]
fn a_texture_at_a_lower_detail_starts_at_a_smaller_level() {
    let level = |side: u32| Image {
        width: side,
        height: side,
        bgra: vec![u8::try_from(side.min(255)).unwrap_or(0); (side * side * 4) as usize],
        smaller: Vec::new(),
    };
    let list = Image {
        smaller: [32, 16, 8, 4, 2, 1].map(level).to_vec(),
        ..level(64)
    };
    assert_eq!(list.levels(), 7);
    let sides = |img: &Image| {
        std::iter::once(img.width)
            .chain(img.smaller.iter().map(|l| l.width))
            .collect::<Vec<_>>()
    };
    assert_eq!(sides(&list.clone().at_detail(4)), [64, 32, 16, 8, 4, 2, 1]);
    let high = list.clone().at_detail(3);
    assert_eq!(sides(&high), [32, 16, 8, 4, 2, 1]);
    assert_eq!(high.bgra[0], 32, "the stored level itself");
    assert_eq!(sides(&list.at_detail(0)), [4, 2, 1]);
    assert_eq!(sides(&level(16).at_detail(0)), [16]);
}
