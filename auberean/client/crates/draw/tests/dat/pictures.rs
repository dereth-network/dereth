//! Textures made from the install's picture lists.

use auberean_draw::picture::load;

use crate::common::install;

/// A picture made for one language is read from the language's file: a signpost's wood and words.
///
/// Behaviour: draw.picture.a-picture-made-for-one-language-is-in-its-file
#[test]
fn a_picture_made_for_one_language_is_in_its_file() {
    let r = &install().records;
    assert!(r.holds(0x4010_0001), "the file holds the signpost's");
    let img = load(r, 0x4010_0001).expect("the signpost's picture");
    assert!(
        img.width > 1 && img.height > 1,
        "{} {}",
        img.width,
        img.height
    );
}

/// A texture's levels of detail are its picture list's levels as stored, each half the one
/// before: the evergreen's branch picture keeps its nine levels from 1024 down to 4 texels a
/// side, and every level's alpha stays either clear or solid, as stored, where averaging the
/// largest level would blur its needles' edges into partial cover.
///
/// Behaviour: draw.picture.a-textures-levels-are-its-stored-levels
#[test]
fn a_textures_levels_are_its_picture_lists_stored_levels() {
    let r = &install().records;
    let img = load(r, 0x4000_04A3).expect("the evergreen's branch picture");
    let sizes: Vec<u32> = std::iter::once(&img)
        .chain(&img.smaller)
        .map(|l| l.width)
        .collect();
    assert_eq!(sizes, [1024, 512, 256, 128, 64, 32, 16, 8, 4]);
    for l in std::iter::once(&img).chain(&img.smaller) {
        assert_eq!(l.width, l.height);
        let partial = l
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|c| c[3] != 0 && c[3] != 0xFF)
            .count();
        assert_eq!(partial, 0, "level {} x {}", l.width, l.height);
        let solid = l
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|c| c[3] == 0xFF)
            .count();
        let share = solid as f64 / (l.bgra.len() / 4) as f64;
        assert!((0.5..0.6).contains(&share), "{} solid: {share}", l.width);
    }
}
