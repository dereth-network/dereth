//! The modern interface's Client Options page ends, in the desktop client, with the remaster's
//! rows under its heading, as every interface's page does: the art pack's box only while a pack
//! is open, NVIDIA's DLSS named under them only where the client carries its library, and a click
//! on the remaster's box writes its preference, which every interface reads.
//! Fixture: the retail dats' shipped layout in a headless gameplay `App` with no device and no
//! network; what the client carries is set by the test after the app starts.

use crate::common::app::gameplay_screen;
use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client_contract::options::names::fidelity as names;
use dereth_client_contract::options::remaster::{self, Carried};
use dereth_client_contract::options::sheet::{self, PageId};
use dereth_client_contract::options::store;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::present::NullPresentation;
use dereth_ui::{ElemHandle, UiSystem};
use dereth_ui_screens::options::page;
use dereth_ui_screens::PrefValue;

/// A headless app in gameplay whose client carries `carried`, said before the gameplay screen
/// builds its pages.
fn gameplay_carrying(carried: Carried) -> App {
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("the headless app starts");
    remaster::set_carried(carried);
    app.start_shell().expect("the shell comes up");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    app
}

/// The text on the row `h`, or `""`.
fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// The options whose rows are in the Client Options page's own list, in order, by their index
/// among the page's options: the chat font's two, which the page builds into the Chat Options
/// page's list, are not.
fn in_the_list(p: &page::PlayerOptionPage) -> Vec<(usize, &'static str)> {
    let rows = &p.option_box.as_ref().expect("the option box").items;
    p.options
        .iter()
        .enumerate()
        .filter(|(_, o)| rows.contains(&o.row))
        .map(|(i, o)| (i, o.preference))
        .collect()
}

/// The remaster's rows the page builds, in the sheet's order: its box, its own boxes (the art
/// pack's only with `art_pack`) and its three menus.
fn the_remasters_rows(art_pack: bool) -> Vec<&'static str> {
    std::iter::once(names::REMASTER)
        .chain(
            remaster::FEATURES
                .iter()
                .filter(|f| !remaster::drawn_nothing_here(f.name))
                .filter(|f| art_pack || f.name != names::REMASTER_ART_PACK)
                .map(|f| f.name),
        )
        .chain([
            names::REMASTER_SCALE,
            names::REMASTER_GOD_RAYS,
            names::REMASTER_SCENERY,
        ])
        .collect()
}

/// Behaviour: remaster.options.the-boxes-are-on-every-interfaces-page-each-off
#[test]
fn the_modern_client_page_ends_with_the_remasters_boxes_and_a_click_writes_its_box() {
    let mut app = gameplay_carrying(Carried::default());
    let want = the_remasters_rows(false);
    let i = {
        let (ui, s) = gameplay_screen(&mut app);
        let p = &s.config_page;
        let listed = in_the_list(p);
        let on_page: Vec<&str> = listed.iter().map(|(_, n)| *n).collect();
        assert!(on_page.len() > want.len(), "{on_page:?}");
        assert_eq!(on_page[on_page.len() - want.len()..], want[..], "last");
        // Each captioned as the other interfaces caption it.
        for o in p
            .options
            .iter()
            .filter(|o| names::NAMES.contains(&o.preference))
        {
            let row = sheet::row_of_preference(PageId::Client, o.preference).expect("a sheet row");
            assert_eq!(o.label.as_deref(), Some(row.caption), "{}", o.preference);
        }
        // Under the remaster's heading, the last on the page.
        let i = listed[on_page.len() - want.len()].0;
        let rows = p.option_box.as_ref().expect("the option box").items.clone();
        let at = rows
            .iter()
            .position(|h| *h == p.options[i].row)
            .expect("its row");
        let heading = rows[at - 1];
        let last = p.options[listed[on_page.len() - 1].0].row;
        // No library beside the client, no line naming it.
        assert!(p.attribution.is_none());
        assert_eq!(rows.last(), Some(&last));
        assert_eq!(text_of(ui, heading), remaster::HEADING);
        // A click on its box, raised as a player's is.
        assert_eq!(
            store::inq_value(names::REMASTER),
            Some(PrefValue::Bool(false))
        );
        let cb = s.config_page.options[i].element;
        ui.set_attribute_bool(cb, page::ATTR_CHECKED, true);
        ui.broadcast_element_message(cb, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
        i
    };
    // Delivered by the next frame, it writes the remaster's preference.
    app.frame();
    assert_eq!(
        store::inq_value(names::REMASTER),
        Some(PrefValue::Bool(true))
    );
    let (_, s) = gameplay_screen(&mut app);
    assert_eq!(s.config_page.options[i].current, PrefValue::Bool(true));
}

/// With an art pack open and NVIDIA's DLSS library beside the client, the art pack's box is on
/// the page and the library is named under the remaster's rows, the page's last line.
///
/// Behaviour: remaster.options.the-boxes-are-on-every-interfaces-page-each-off
#[test]
fn with_a_pack_open_and_the_dlss_library_the_modern_page_has_the_art_pack_box_and_names_dlss() {
    let mut app = gameplay_carrying(Carried {
        dlss: true,
        art_pack: true,
    });
    let (ui, s) = gameplay_screen(&mut app);
    let p = &s.config_page;
    let on_page: Vec<&str> = in_the_list(p).into_iter().map(|(_, n)| n).collect();
    let want = the_remasters_rows(true);
    assert_eq!(on_page[on_page.len() - want.len()..], want[..]);
    let line = p.attribution.expect("the line naming DLSS");
    let rows = p.option_box.as_ref().expect("the option box").items.clone();
    assert_eq!(rows.last(), Some(&line), "under the remaster's rows");
    assert_eq!(text_of(ui, line), remaster::DLSS_ATTRIBUTION);
    remaster::set_carried(Carried::default());
}
