//! The terrain compositor blends: a retail corner alpha map carries its mask in the alpha channel
//! (reading any other channel gives a flat mask and a hard replace on every texel, which looks
//! like per-cell patches with no blending), and a palettised `PFID_INDEX16` surface decodes to real
//! colour rather than solid black. Fixture: the retail dats (a missing install fails).

use dereth_client_runtime::landblock::load_region;
use dereth_scene::textures::TextureStore;

/// Behaviour: rendering.terrain.the-alpha-map-mask-blends-between-textures
#[test]
fn a_retail_alpha_map_carries_its_mask_in_the_alpha_channel() {
    // A worktree without the retail dats must fail here, not pass having decoded nothing.
    let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    let region = load_region(&store).expect("the region decodes");
    let tm = region
        .land_surf
        .tex_merge
        .as_ref()
        .expect("retail ships a terrain texture-merge record");
    let t = TextureStore::new(&store);

    let map = tm
        .corner_terrain_maps
        .first()
        .expect("retail ships corner alpha maps");
    let img = t.bgra8(map.tex_gid).expect("the alpha map decodes");

    // Per-channel spread over the whole mask. A real corner mask is a gradient, so the channel
    // carrying it must take more than one value; the channels that carry nothing are flat zero.
    let mut lo = [255u8; 4];
    let mut hi = [0u8; 4];
    for px in &img.pixels {
        for c in 0..4 {
            lo[c] = lo[c].min(px[c]);
            hi[c] = hi[c].max(px[c]);
        }
    }
    let spread = |c: usize| hi[c] - lo[c];
    eprintln!(
        "alpha map {} ({}x{}): B {}..{}  G {}..{}  R {}..{}  A {}..{}",
        map.tex_gid, img.width, img.height, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2], lo[3], hi[3]
    );

    assert!(
        spread(3) > 0,
        "the mask must vary across the alpha channel, or there is nothing to blend with"
    );
    assert_eq!(
        spread(2),
        0,
        "channel 2 (R) is flat, so merge_overlay reading pixels[..][2] gets a constant"
    );
}

/// `PFID_INDEX16` (101) is the second palettised format arm: 16-bit
/// indices straight into the 2048-entry palette. Decoded as a native format instead, every INDEX16
/// surface would be solid black with no error, and every part of a human body is INDEX16.
///
/// Oracle: the retail dats. A palettised body texture is not one colour.
#[test]
fn an_index16_surface_decodes_to_real_colour_and_not_solid_black() {
    // A worktree without the retail dats must fail here, not pass having decoded nothing.
    let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    let t = TextureStore::new(&store);

    let mut checked = 0usize;
    let mut varied = 0usize;
    for id in store.ids_of(dereth_dat::DbType::RenderSurface) {
        let Ok((_, rs, _)) = t.resolve(id) else {
            continue;
        };
        if dereth_render::pixel_format::PixelFormatId::from_raw(rs.format)
            != dereth_render::pixel_format::PixelFormatId::Index16
        {
            continue;
        }
        let Ok(img) = t.bgra8(id) else { continue };
        let distinct = img
            .pixels
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        if distinct > 1 {
            varied += 1;
        }
        checked += 1;
        if checked == 24 {
            break;
        }
    }
    assert!(
        checked > 0,
        "retail ships INDEX16 surfaces; finding none means the scan is broken"
    );
    // Not every INDEX16 surface has to be multi-coloured -- 0x060037A3 is a legitimate 8x8 solid
    // swatch -- but the defect made *every one of them* a single flat black, so a clear majority
    // carrying real palette colour is what separates a working arm from the silhouette bug.
    assert!(
        varied * 2 > checked,
        "only {varied} of {checked} INDEX16 surfaces decoded to more than one colour; the \
         palettised arm is missing again and bodies will render as silhouettes"
    );
    eprintln!("checked {checked} INDEX16 surfaces, {varied} with real palette colour");
}
