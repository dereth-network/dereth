//! The bindless table holds thousands of textures, each sampled by the index it was given.
//!
//! Behaviour: none (experimental)

use dereth_gfx::{
    ColorAttachment, ColorTarget, Cull, Filter, Format, GraphicsDesc, Load, RasterPass,
    SamplerDesc, TextureDesc, TextureUsage,
};

use crate::shaders::Entry;

/// Texture `k`'s colour: different for every one of the 4,096.
fn color(k: u32) -> [u8; 4] {
    [
        (k & 0xFF) as u8,
        ((k >> 8) & 0xFF) as u8 | 0x80,
        (255 - (k & 0xFF)) as u8,
        255,
    ]
}

/// 4,096 one-texel textures, each its own colour, bound into the table and sampled each by one
/// pixel of a 64 x 64 target, a one-pixel square each, through the index the table gave it: every
/// pixel reads back as its texture's colour.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn four_thousand_ninety_six_bindless_textures_are_each_sampled() {
    const N: u32 = 4096;
    let mut d = crate::hardware();
    assert!(d.caps().bindless_textures >= N, "{:?}", d.caps());
    let (vs, fs) = (
        crate::spirv(Entry::TexturesVsMain),
        crate::spirv(Entry::TexturesFsMain),
    );
    let p = d
        .create_graphics_pipeline(&GraphicsDesc {
            label: "sample",
            vertex: crate::shader(Entry::TexturesVsMain, &vs),
            fragment: Some(crate::shader(Entry::TexturesFsMain, &fs)),
            color: &[ColorTarget {
                format: Format::Rgba8Unorm,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })
        .expect("the pipeline builds");
    let sampler = d
        .create_sampler(&SamplerDesc {
            filter: Filter::Nearest,
            mip: Filter::Nearest,
            ..SamplerDesc::default()
        })
        .expect("a sampler");
    let s = d.bind_sampler(sampler).expect("a sampler index");
    let mut indices = Vec::new();
    for k in 0..N {
        let t = d
            .create_texture(&TextureDesc {
                label: "texel",
                width: 1,
                height: 1,
                mips: 1,
                format: Format::Rgba8Unorm,
                usage: TextureUsage::SAMPLED,
            })
            .expect("a texture");
        d.upload_texture(t, 0, &color(k)).expect("staged");
        indices.push(d.bind_texture(t).expect("an index"));
    }
    // The table hands out its indices in order to a fresh device, so texture `k` is at
    // `base + k`; the test checks that rather than assume it.
    let base = indices[0];
    assert!(
        indices
            .iter()
            .enumerate()
            .all(|(k, i)| *i == base + k as u32),
        "the indices are not consecutive"
    );
    let target = d
        .create_texture(&TextureDesc {
            label: "target",
            width: 64,
            height: 64,
            mips: 1,
            format: Format::Rgba8Unorm,
            usage: TextureUsage::COLOR_TARGET | TextureUsage::TRANSFER_SRC,
        })
        .expect("a target");
    {
        let mut r = d.begin().expect("a recording");
        r.raster(
            &RasterPass {
                name: "sample",
                color: &[ColorAttachment {
                    texture: target,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &[],
                area: None,
            },
            |draw| {
                draw.set_pipeline(p)?;
                // A grid of 64 x 64 one-pixel squares, square k sampling texture `base + k`.
                let push: Vec<u8> = [base, s, 64, 64]
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .collect();
                draw.push(0, &push)?;
                draw.draw(6, N, 0, 0);
                Ok(())
            },
        )
        .expect("the pass records");
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    // The grid fills clip space from its bottom row up, and the target's rows are read from the
    // top: the pixel in row `y`, column `x` is square `(63 - y) * 64 + x`.
    let px = crate::pixels(&d.read_texture(target, 0).expect("reads back"));
    let square = |p: usize| ((63 - p / 64) * 64 + p % 64) as u32;
    let wrong: Vec<(usize, [u8; 4])> = px
        .iter()
        .enumerate()
        .filter(|(p, c)| **c != color(square(*p)))
        .map(|(p, c)| (p, *c))
        .take(8)
        .collect();
    assert!(
        wrong.is_empty(),
        "pixels not their texture's colour: {wrong:?}"
    );
    assert_eq!(d.live_textures(), N as usize + 1);
    crate::no_validation_errors();
}

/// Pictures in blocks of two channels and of four (BC5 and BC7) read back, every level, exactly
/// as they were uploaded.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn block_pictures_of_two_and_four_channels_read_back_as_uploaded() {
    let mut d = crate::hardware();
    for format in [Format::Bc5, Format::Bc7] {
        assert!(format.is_block());
        let t = d
            .create_texture(&TextureDesc {
                label: "blocks",
                width: 16,
                height: 8,
                mips: 2,
                format,
                usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC,
            })
            .expect("a texture");
        let levels: Vec<Vec<u8>> = (0..2u32)
            .map(|l| {
                let n = format.level_bytes(16 >> l, 8 >> l) as usize;
                (0..n).map(|i| (i as u32 * 7 + l * 31) as u8).collect()
            })
            .collect();
        assert_eq!(
            levels[0].len(),
            4 * 2 * 16,
            "16 bytes a block of four by four"
        );
        for (l, data) in levels.iter().enumerate() {
            d.upload_texture(t, l as u32, data).expect("staged");
        }
        for (l, data) in levels.iter().enumerate() {
            let back = d.read_texture(t, l as u32).expect("reads back");
            assert_eq!(&back, data, "{format:?} level {l}");
        }
        d.destroy_texture(t);
    }
    crate::no_validation_errors();
}
