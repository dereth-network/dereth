//! How many of a picture list's levels a texture takes.

use auberean_draw::picture::dropped_levels;

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
