//! Behaviour: none (file-format decoders: RGB images, indexed textures, palettes, palette sets)
//!
//! Hand-built art records: the RGB and indexed pixel layouts, palette colour order, object
//! descriptions naming a texture, and the mirrored eye strip.

use dereth_classic_dat::appearance::{self, IndexedTexture};
use dereth_classic_dat::image::decode_rgb;

use crate::build::{indexed, palette, Enc};

#[test]
fn an_rgb_record_decodes_to_opaque_rgba_in_row_order() {
    let mut e = Enc::default();
    e.u32(0x0600_0042).u32(2).u32(1).bytes(&[1, 2, 3, 4, 5, 6]);
    let img = decode_rgb(&e.0, Some(0x0600_0042)).unwrap();
    assert_eq!((img.width, img.height), (2, 1));
    assert_eq!(img.rgba, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    assert!(decode_rgb(&e.0, None).is_ok());
}

#[test]
fn an_rgb_record_with_the_wrong_id_or_size_is_refused() {
    let mut e = Enc::default();
    e.u32(0x0600_0042).u32(2).u32(1).bytes(&[1, 2, 3, 4, 5, 6]);
    assert!(decode_rgb(&e.0, Some(0x0600_0043)).is_err());
    let mut long = e.0.clone();
    long.push(0);
    assert!(decode_rgb(&long, None).is_err());
    let mut wrong_type = e.0.clone();
    wrong_type[3] = 5;
    assert!(decode_rgb(&wrong_type, None).is_err());
    assert!(decode_rgb(&e.0[..8], None).is_err());
    let mut empty = Enc::default();
    empty.u32(0x0600_0001).u32(0).u32(5);
    assert!(decode_rgb(&empty.0, None).is_err());
}

#[test]
fn an_indexed_texture_yields_its_indices_and_default_palette() {
    let data = indexed(0x0500_0010, 3, 1, &[7, 8, 9], 0x0400_0003);
    assert_eq!(data.len(), 24);
    let t = appearance::indexed(&data).unwrap();
    assert_eq!(
        t,
        IndexedTexture {
            width: 3,
            height: 1,
            indices: vec![7, 8, 9],
            palette: 0x0400_0003
        }
    );
    assert!(appearance::indexed(&data[..20]).is_err());
    let mut rgb16 = data.clone();
    rgb16[4] = 3;
    assert!(appearance::indexed(&rgb16).is_err());
}

#[test]
fn a_palette_decodes_each_word_as_red_green_blue_with_opaque_alpha() {
    let data = palette(0x0400_0001);
    let p = appearance::palette(&data).unwrap();
    assert_eq!(p.len(), 256);
    assert_eq!(p[1], [0x01, 0x02, 0x03, 0xFF]);
    assert_eq!(p[2], [0x02, 0x04, 0x06, 0xFF]);
    let mut short = data.clone();
    short[4] = 255;
    assert!(appearance::palette(&short).is_err());
}

#[test]
fn a_palette_set_lists_its_palette_ids() {
    let mut e = Enc::default();
    e.u32(0x0F00_0001).list(&[0x0400_0001, 0x0400_0002]);
    assert_eq!(
        appearance::palette_set(&e.0).unwrap(),
        vec![0x0400_0001, 0x0400_0002]
    );
    e.u32(0);
    assert!(appearance::palette_set(&e.0).is_err());
}

#[test]
fn an_object_description_names_the_new_texture_of_its_first_swap() {
    let mut e = Enc::default();
    e.objdesc(Some(0x1234));
    assert_eq!(appearance::first_texture(&e.0).unwrap(), 0x0500_1234);

    // Palette swaps come first: a base palette, then each swap's palette and a range word. A
    // packed id with its high bit set carries a second half.
    let mut e = Enc::default();
    e.bytes(&[0x11, 1, 1, 0])
        .u16(0x8001)
        .u16(0x0002)
        .u16(0x0033)
        .u16(0x0506)
        .u8(0)
        .u16(0x0044)
        .u16(0x8000)
        .u16(0x0055);
    assert_eq!(appearance::first_texture(&e.0).unwrap(), 0x0500_0055);

    let mut none = Enc::default();
    none.objdesc(None);
    assert_eq!(appearance::first_texture(&none.0).unwrap(), 0);
    assert!(appearance::first_texture(&[0x10, 0, 0, 0]).is_err());
    assert!(appearance::first_texture(&[0x11, 0, 1, 0]).is_err());
}

#[test]
fn a_mirrored_texture_follows_each_row_with_its_reverse() {
    let t = IndexedTexture {
        width: 3,
        height: 2,
        indices: vec![1, 2, 3, 4, 5, 6],
        palette: 9,
    };
    let m = appearance::mirrored(&t);
    assert_eq!((m.width, m.height, m.palette), (6, 2, 9));
    assert_eq!(m.indices, vec![1, 2, 3, 3, 2, 1, 4, 5, 6, 6, 5, 4]);
}

#[test]
fn appearance_admission_keeps_header_errors_before_shape_and_length_errors() {
    for len in 0..8 {
        assert_eq!(
            appearance::palette(&vec![0; len]).unwrap_err(),
            "palette header is truncated"
        );
        assert_eq!(
            appearance::palette_set(&vec![0; len]).unwrap_err(),
            "palette set header is truncated"
        );
    }
    for len in 0..16 {
        assert_eq!(
            appearance::indexed(&vec![0; len]).unwrap_err(),
            "indexed texture header is truncated"
        );
    }
    let mut colors = palette(0x0400_0001);
    colors[15] = 0x12;
    assert_eq!(appearance::palette(&colors).unwrap()[1], [1, 2, 3, 255]);
    for bytes in [
        {
            let mut b = colors.clone();
            b[3] = 5;
            b
        },
        {
            let mut b = colors.clone();
            b[4..8].copy_from_slice(&255u32.to_le_bytes());
            b
        },
        colors[..colors.len() - 1].to_vec(),
        {
            let mut b = colors;
            b.push(0);
            b
        },
    ] {
        assert_eq!(
            appearance::palette(&bytes).unwrap_err(),
            "palette dimensions do not consume payload"
        );
    }
    let mut set = Enc::default();
    set.u32(0x0f00_0001).u32(0);
    assert_eq!(appearance::palette_set(&set.0).unwrap(), Vec::<u32>::new());
    for bytes in [
        {
            let mut b = set.0.clone();
            b[3] = 4;
            b
        },
        {
            let mut b = set.0.clone();
            b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
            b
        },
        {
            let mut b = set.0;
            b.push(0);
            b
        },
    ] {
        assert_eq!(
            appearance::palette_set(&bytes).unwrap_err(),
            "palette set does not consume payload"
        );
    }
    for (id, kind, width, height, error) in [
        (0x0400_0001, 2, 1, 1, "face is not an indexed texture"),
        (0x0500_0001, 3, 1, 1, "face is not an indexed texture"),
        (0x0500_0001, 2, 0, 1, "face is not an indexed texture"),
        (0x0500_0001, 2, 1, 0, "face is not an indexed texture"),
        (
            0x0500_0001,
            2,
            u32::MAX,
            u32::MAX,
            "indexed texture does not consume payload",
        ),
    ] {
        let mut e = Enc::default();
        e.u32(id).u32(kind).u32(width).u32(height);
        assert_eq!(appearance::indexed(&e.0).unwrap_err(), error);
    }
    let image = indexed(0x0500_0001, 1, 1, &[7], 0x0400_0001);
    for bytes in [image[..image.len() - 1].to_vec(), {
        let mut b = image;
        b.push(0);
        b
    }] {
        assert_eq!(
            appearance::indexed(&bytes).unwrap_err(),
            "indexed texture does not consume payload"
        );
    }
}
