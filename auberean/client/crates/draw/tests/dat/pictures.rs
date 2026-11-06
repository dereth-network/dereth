//! Textures made from the install's picture lists.

use std::collections::BTreeMap;

use auberean_assets::picture::Picture;
use auberean_draw::picture::{expand, load};

use crate::common::{ids_of, install};

/// Every stored picture is in a format the draw reads, and a format that isn't one reads as no
/// picture: no stored picture is in YUY2, or any other format only the original's device spoke.
///
/// Behaviour: draw.picture.every-stored-picture-is-in-a-format-the-draw-reads
#[test]
fn every_stored_picture_is_in_a_format_the_draw_reads() {
    let r = &install().records;
    let (mut counts, mut unread) = (BTreeMap::new(), Vec::new());
    for id in ids_of(0x41) {
        let p: Picture = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        *counts.entry(p.format_name()).or_insert(0u32) += 1;
        if expand(&p).is_none() {
            unread.push((id, p.format_name()));
        }
    }
    assert!(counts.len() > 1, "{counts:?}");
    assert!(!counts.contains_key("YUY2"), "{counts:?}");
    assert!(
        unread.is_empty(),
        "{} of them unread, first {:?}; {counts:?}",
        unread.len(),
        unread.first()
    );
}

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
