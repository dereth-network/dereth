//! The target marker is colourised with retail's integer hue and saturation arithmetic and keeps
//! its alpha, for all twelve shipped target surfaces. Fixture: the retail dats; no device.

use dereth_client::{
    textures::TextureStore,
    ui_draw::{derive, image_key},
};
use dereth_ui::region::SurfaceOp;

/// Behaviour: selection.target-marker.is-colourised-with-retails-integer-hue-and-keeps-its-alpha
#[test]
fn colorize_preserves_alpha_and_value_with_retails_integer_hue_and_saturation() {
    // The original 1..256 integer HSV conversion gives 86, while multiplication gives 85, for
    // this grey/gold pair.
    assert_eq!(
        SurfaceOp::Colorize(0xFFFF_AB00).apply(0x8080_8080),
        0x8080_5600
    );
    assert_ne!(
        SurfaceOp::Colorize(0xFFFF_AB00).apply(0x8080_8080),
        SurfaceOp::Multiply(0xFFFF_AB00).apply(0x8080_8080)
    );
    for (color, white, grey) in [
        (0xFFFF_0000, 0xFFFF_0000, 0x8080_0000),
        (0xFFFF_FF00, 0xFFFF_FF00, 0x8080_8000),
        (0xFF00_FF00, 0xFF00_FF00, 0x8000_8000),
        (0xFF00_FFFF, 0xFF00_FFFF, 0x8000_8080),
        (0xFF00_00FF, 0xFF00_00FF, 0x8000_0080),
        (0xFFFF_00FF, 0xFFFF_00FF, 0x8080_0080),
        (0xFFFF_FFFF, 0xFFFF_FFFF, 0x8080_8080),
    ] {
        assert_eq!(SurfaceOp::Colorize(color).apply(0xFFFF_FFFF), white);
        assert_eq!(SurfaceOp::Colorize(color).apply(0x8080_8080), grey);
        assert_eq!(SurfaceOp::Colorize(color).apply(0x0100_0000), 0x0100_0000);
    }
}

#[test]
fn all_twelve_shipped_target_surfaces_reach_the_real_derivation_without_losing_alpha() {
    let store = dereth_dat::testing::open_store().expect("retail DATs");
    let textures = TextureStore::new(&store);
    let mut distinct = std::collections::HashSet::new();
    for index in 1..=12 {
        let id = dereth_assets::did_by_enum(&store, 0x1000_0009, index).expect("target enum");
        let source = textures.texture_data(id).expect("shipped image decodes");
        assert_eq!(
            source.format,
            dereth_primitives::TextureFormat::Bgra8,
            "derive must not silently return compressed pixels uncolored"
        );
        assert!(source.width > 0 && source.height > 0);
        let gold = derive(source.clone(), SurfaceOp::Colorize(0xFFFF_AB00));
        let white = derive(source.clone(), SurfaceOp::Colorize(0xFFFF_FFFF));
        assert_eq!((gold.width, gold.height), (source.width, source.height));
        let mut visible = 0;
        let mut transparent = 0;
        for ((src, dst), corpse) in source.levels[0]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(gold.levels[0].as_chunks::<4>().0)
            .zip(white.levels[0].as_chunks::<4>().0)
        {
            assert_eq!(src[3], dst[3], "source alpha preserved at image {index}");
            assert_eq!(src[3], corpse[3]);
            if src[3] == 0 {
                transparent += 1;
            } else {
                visible += 1;
            }
            assert_eq!(dst[0], 0, "gold has zero blue saturation endpoint");
            assert_eq!(corpse[0], corpse[1]);
            assert_eq!(corpse[1], corpse[2]);
        }
        assert!(
            visible > 0 && transparent > 0,
            "real bracket/arrow, not an opaque quad"
        );
        assert_ne!(
            gold.levels, white.levels,
            "creature/corpse color choice changes actual pixels"
        );
        for color in [
            0xFF40_A8FF,
            0xFFFF_AB00,
            0xFFFF_FFFF,
            0xFFBF_63FF,
            0xFFFF_4063,
            0xFFFF_A8BF,
            0xFF00_8040,
            0xFFFF_FF80,
            0xFF00_FFFF,
            0xFF00_FF00,
        ] {
            assert!(
                distinct.insert(image_key(id, Some(SurfaceOp::Colorize(color)))),
                "the real renderer texture key must distinguish every target image/palette pair"
            );
        }
    }
}
