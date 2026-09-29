//! Every font the shipped layouts name rasterises, with its range and texture size stated; the two
//! char-gen wizard fonts that do not fit a 256x256 bake still draw; the name prompt draws without
//! its string-table backslashes; and the FINISH caption's font rasterises. Fixture: the retail dats
//! read directly, and an offline headless `App` on the char-gen wizard (no link, FINISH never
//! pressed).
//!
//! # How UI text is rasterised
//!
//! The client's font-texture setup bakes only printable ASCII into an exact 256x256 ARGB texture
//! and refuses when the range will not fit. Three shipped fonts do not fit (`0x40000013`,
//! `0x40000014` and `0x40000024`, whose max_char_height is 46, 48 and 42), and the wizard names two
//! of them. The bake and its refusal are still asserted here. UI text does not use that atlas: the
//! UI element system blits from the font's own source surfaces, and the atlas serves only the small
//! debug font and Latin-script text outside that source-sheet path. The glyph texture for UI text
//! is therefore the font's foreground sheet, addressed by each character's source rectangle, and no
//! font can overflow anything.
//!
//! | claim | oracle |
//! |---|---|
//! | every retail font rasterises | the portal dat's own 49 `0x40xxxxxx` objects, each one read |
//! | the 256x256 bake still refuses the three tall ones | it refuses past row 255 |
//! | the name prompt loses its escapes | the string-table escape rules |

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_primitives::DataId;
use dereth_render::font::{FontAtlas, GlyphSheet};
use dereth_ui::framework::mode;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::chargen::{EcgProgress, FINISH_BUTTON};

fn require_dats() {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
}

// -------------------------------------------------------------------------------------------
// 1. Every font in the retail dats, and the bake that still refuses three of them
// -------------------------------------------------------------------------------------------

/// The `Font` objects `client_portal.dat` actually ships, ascending.
fn every_retail_font(store: &dereth_dat::RetailDatStore) -> Vec<DataId> {
    let mut ids: Vec<DataId> = store
        .portal()
        .iter_ids()
        .filter(|d| (0x4000_0000..0x4100_0000).contains(&d.0))
        .collect();
    ids.sort_unstable_by_key(|d| d.0);
    assert!(!ids.is_empty(), "the portal dat ships Font objects");
    ids
}

/// Behaviour: ui.fonts.every-font-the-layouts-name-rasterises
///
/// **Measured rather than assumed:** every font the dat ships produces a glyph texture, and every
/// printable ASCII character is addressable in it.
///
/// The three the 256x256 bake refuses are named, so the claim "three do not fit" is a fact this
/// test would notice changing rather than a sentence in a report.
#[test]
fn every_retail_font_rasterises_and_the_range_and_texture_size_are_stated() {
    require_dats();
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats open");
    let ids = every_retail_font(&store);

    let mut overflowing: Vec<u32> = Vec::new();
    let mut texture_bytes = 0usize;
    let mut widest = (DataId(0), (0u32, 0u32));
    for did in &ids {
        let font = dereth_client::ui_draw::load_font(&store, *did)
            .unwrap_or_else(|e| panic!("{did:?} is a Font object: {e}"));
        let atlas = dereth_client::ui_draw::build_font_atlas(&store, *did)
            .unwrap_or_else(|e| panic!("{did:?} rasterises: {e}"));

        // The texture is the font's own foreground sheet, not a 256x256 bake.
        assert_ne!(atlas.texture_size, (0, 0), "{did:?} has a glyph texture");
        assert_eq!(
            atlas.pixels.len(),
            (atlas.texture_size.0 as usize) * (atlas.texture_size.1 as usize) * 4,
            "{did:?}: the texture carries its own pixels"
        );
        texture_bytes += atlas.pixels.len();
        if u64::from(atlas.texture_size.0) * u64::from(atlas.texture_size.1)
            > u64::from(widest.1 .0) * u64::from(widest.1 .1)
        {
            widest = (*did, atlas.texture_size);
        }
        // Its range is the font's own, not a clamped 0x20..=0x7F window.
        let (lo, hi) = font.unicode_range().expect("a shipped font has glyphs");
        assert_eq!(
            atlas.baked_chars().len(),
            font.char_descs.len(),
            "{did:?}: every one of the font's {} records is addressable (range {lo:#06X}..={hi:#06X})",
            font.char_descs.len()
        );
        for ch in 0x20u8..=0x7E {
            assert!(
                atlas.glyph(ch as char).is_some(),
                "{did:?} draws {:?}",
                ch as char
            );
        }

        // ...and the client's own bake, unchanged, on the same font.
        let sheet_id = DataId(font.foreground_surface_data_id);
        let textures = dereth_client::textures::TextureStore::new(&store);
        let sheet = textures
            .bgra8(sheet_id)
            .unwrap_or_else(|e| panic!("{sheet_id:?}: {e}"));
        let flat: Vec<u8> = sheet.pixels.iter().flat_map(|p| *p).collect();
        let baked = FontAtlas::build(
            &font,
            GlyphSheet {
                width: sheet.width,
                height: sheet.height,
                bgra: &flat,
            },
        );
        match baked {
            Ok(b) => assert_eq!(
                b.texture_size,
                (256, 256),
                "{did:?}: a bake is always 256x256"
            ),
            Err(dereth_render::RenderError::FontAtlasOverflow) => overflowing.push(did.0),
            Err(e) => panic!("{did:?}: the bake failed for a reason other than overflow: {e}"),
        }
    }

    // The bake refuses once the pen passes row 255, for exactly the three display fonts whose
    // `max_char_height` is 46, 48 and 42.
    assert_eq!(
        overflowing,
        vec![0x4000_0013, 0x4000_0014, 0x4000_0024],
        "the 256x256 bake still refuses exactly the three tall fonts; the UI path does not use it"
    );
    // What the change costs, stated rather than guessed: a glyph texture is now the font's sheet,
    // so it is bigger than a 256x256 bake and only the fonts a screen names are ever uploaded.
    eprintln!(
        "font_rasterisation: {} retail fonts, all rasterised; 3 exceed the 256x256 bake. \
         All 49 sheets together are {:.1} MiB; the largest is {:?} at {:?}.",
        ids.len(),
        texture_bytes as f64 / (1024.0 * 1024.0),
        widest.0,
        widest.1
    );
}

/// The two the char-gen layouts name, called out on their own because FINISH and the town's name
/// are drawn in them.
#[test]
fn the_two_fonts_the_wizard_names_are_the_ones_that_overflowed() {
    require_dats();
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats open");
    for (did, max_h) in [(0x4000_0024u32, 42u32), (0x4000_0014, 48)] {
        let did = DataId(did);
        let font = dereth_client::ui_draw::load_font(&store, did).expect("the font reads");
        assert_eq!(font.max_char_height, max_h, "{did:?}'s line height");
        let atlas = dereth_client::ui_draw::build_font_atlas(&store, did)
            .unwrap_or_else(|e| panic!("{did:?} rasterises: {e}"));
        // The sheet is far larger than 256 in at least one axis, which is why the bake could never
        // have held it.
        assert!(
            atlas.texture_size.0 > 256 || atlas.texture_size.1 > 256,
            "{did:?}'s sheet is {:?}",
            atlas.texture_size
        );
        for c in "FINISHHoltburgYaraq".chars() {
            assert!(atlas.glyph(c).is_some(), "{did:?} draws {c:?}");
        }
    }
}

// -------------------------------------------------------------------------------------------
// 2. Driving the wizard headless
// -------------------------------------------------------------------------------------------

/// The wizard, offline and headless, at 800x600.
fn app_on_wizard() -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(mode::CHAR_GEN);
    for _ in 0..8 {
        app.frame();
    }
    app
}

/// A button-element message `(element, 1, 7, 0)` — a real button click.
fn click(app: &mut App, id: u32) {
    let shell = app.ui_mut().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("element {id:#010X} is in the shipped layout"));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

// -------------------------------------------------------------------------------------------
// 3. The string table's escapes
// -------------------------------------------------------------------------------------------

/// `ID_CharGen_NamePrompt` carries `\[ Name \]` in the shipped table and retail draws `[ Name ]`.
///
/// The string-table rule is: `n`, `q`, `r` and `t` become the four control characters, each of
/// the ten metacharacters `[]!{}#\|^$` becomes itself, and
/// anything else is not an escape at all, so the backslash stays. Glyph lookup is *not* where the
/// client applies it (glyph lookup handles `<...>` tags and nothing else), which is recorded in
/// `unescape`'s own note.
#[test]
fn the_name_prompt_draws_without_its_backslashes() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_wizard();
    click(
        &mut app,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab")
            .0,
    );
    for _ in 0..4 {
        app.frame();
    }
    let prompt = app
        .ui_draw_list()
        .iter()
        .filter(|c| !c.glyphs.is_empty())
        .map(|c| {
            c.glyphs
                .iter()
                .filter_map(|g| char::from_u32(u32::from(g.ch)))
                .collect::<String>()
        })
        .find(|s| s.contains("Name"))
        .expect("the summary page shows the name prompt");
    assert_eq!(
        prompt, "[ Name ]",
        "the shipped string is `\\[ Name \\]` and retail unescapes it"
    );
    assert!(!prompt.contains('\\'));
    app.shutdown();
}

/// Behaviour: chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names
///
/// The FINISH button's caption font is `0x40000024`, and it rasterises, so the two halves are
/// asserted in one place.
#[test]
fn the_finish_buttons_font_is_the_one_that_used_to_overflow() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_wizard();
    click(
        &mut app,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab")
            .0,
    );
    for _ in 0..4 {
        app.frame();
    }
    let shell = app.ui().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, FINISH_BUTTON)
        .expect("FINISH is in the layout");
    let props = shell.ui.node(h).expect("a live node").merged_properties();
    let font = match props.get(dereth_ui::props::attr::TEXT_FONT_DID) {
        Some(dereth_assets::ui::PropertyValue::Array(a)) => match a.first().map(|e| &e.value) {
            Some(dereth_assets::ui::PropertyValue::DataFile(d)) => Some(*d),
            _ => None,
        },
        _ => None,
    };
    assert_eq!(font, Some(DataId(0x4000_0024)), "the caption's font");
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats open");
    assert!(
        dereth_client::ui_draw::build_font_atlas(&store, DataId(0x4000_0024)).is_ok(),
        "and it rasterises"
    );
    app.shutdown();
}
