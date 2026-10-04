use super::*;

// A-F18: actual UI image/font/movie upload owners must not inherit world ImgTex AUTOGEN.
// The draw request is synthetic; pixels and font metrics are unchanged installed retail DATs.

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn texture_minification_keeps_real_ui_font_and_movie_owners_single_level() {
    let store = dereth_dat::testing::open_store().expect("required pristine retail DATs");
    let id = DataId(0x0600_7576); // real character-screen JPEG, with encoded (not DAT-header) extent.
    let font = DataId(0x4000_0001);
    let texture = crate::textures::TextureStore::new(&store)
        .texture_data(id)
        .expect("retail UI JPEG");
    assert!(texture.width > 1 && texture.height > 1);
    let cmd = dereth_ui::UiDrawCmd {
        who: dereth_ui::ElemHandle::for_test(0),
        screen: dereth_ui::Box2D::new(0, 0, 15, 15),
        clip: dereth_ui::Box2D::new(0, 0, 15, 15),
        image: Some(id),
        image_op: None,
        image_source: dereth_ui::ImageSource::Interface,
        blit_mode: Default::default(),
        alpha_blend_mod: 1.0,
        tiling_offset: (0, 0),
        rotation_z_degrees: 0,
        color: 0xFFFF_FFFF,
        glyphs: vec![dereth_ui::text::PlacedGlyph {
            x: 0,
            y: 0,
            ch: u16::from(b'A'),
            color: 0xFFFF_FFFF,
            font,
        }],
        text_outline: Some(0xFF00_0000),
        invert: vec![],
        fills: vec![],
    };
    let mut renderer = Renderer::new(None, 16, 16).expect("required WARP renderer");
    assert!(
        renderer.gpu.imgtex_autogen_supported(),
        "a capable device is the discriminating negative"
    );
    renderer.prepare_ui(&store, std::slice::from_ref(&cmd));
    let (image, size) = renderer.ui_textures[&(id, None, dereth_ui::ImageSource::Interface)]
        .expect("actual UI image upload");
    let image_slot = renderer.overlay_slot(image).expect("the image is resident");
    assert_eq!(size, (texture.width, texture.height));
    assert_eq!(renderer.gpu.texture_mip_levels(image_slot), Some(1));
    assert_eq!(
        renderer
            .gpu
            .capture_texture_level(image_slot, 0)
            .expect("UI bytes")
            .bgra,
        texture.levels[0]
    );
    let (_, sheet, outline) = *renderer.ui_fonts[&font]
        .as_ref()
        .map(|(_, s, o)| ((), *s, *o))
        .as_ref()
        .expect("actual DAT glyph upload");
    let glyph_slot = renderer.overlay_slot(sheet).expect("the sheet is resident");
    assert_eq!(renderer.gpu.texture_mip_levels(glyph_slot), Some(1));
    if let Some(outline) = outline {
        let outline_slot = renderer
            .overlay_slot(outline)
            .expect("the outline is resident");
        assert_eq!(renderer.gpu.texture_mip_levels(outline_slot), Some(1));
    }
    let uploaded = renderer.ui_stats.uploaded;
    renderer.prepare_ui(&store, &[cmd]);
    assert_eq!(
        renderer.ui_stats.uploaded, uploaded,
        "same UI request does not regenerate"
    );
    // The movie setter is the production uncached frame-replacement owner. The small frame is
    // synthetic and explicitly not decoded from a retail movie.
    let movie = dereth_primitives::TextureData {
        width: 8,
        height: 8,
        format: dereth_primitives::TextureFormat::Bgra8,
        levels: vec![[20, 40, 80, 255].repeat(64)],
    };
    renderer.set_movie_frame(id, &movie);
    let (frame, _) = renderer.ui_textures[&(id, None, dereth_ui::ImageSource::Interface)]
        .expect("actual movie frame upload");
    let movie_slot = renderer.overlay_slot(frame).expect("the frame is resident");
    assert_eq!(renderer.gpu.texture_mip_levels(movie_slot), Some(1));
    assert_eq!(
        renderer
            .gpu
            .capture_texture_level(movie_slot, 0)
            .expect("movie bytes")
            .bgra,
        movie.levels[0]
    );
    renderer.release_ui_textures();
    assert_eq!(
        renderer.gpu.texture_mip_levels(movie_slot),
        None,
        "movie owner releases all resources"
    );
}
