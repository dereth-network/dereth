//! Behaviour: none (experimental)
//!
//! The remaster's box on the options window's client page, on the retail layouts: a row whose
//! label and help the data's tables don't hold shows this client's own words for them.

use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, View};
use auberean_ui_game::options::client::{self, section, Listing};
use auberean_ui_game::options::helper::{self, own, Control};
use auberean_ui_game::options::preference::Description;
use auberean_ui_game::options::{OptionWindow, Values};
use auberean_ui_game::toolbar::Toolbar;

use crate::common::records;

fn window(p: &GamePlay) -> &OptionWindow {
    p.child::<Toolbar>()
        .and_then(|t| t.window("OptionField"))
        .and_then(|c| c.child.as_any().downcast_ref::<OptionWindow>())
        .expect("the options window")
}

/// One frame, the client listing `listing`.
fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, listing: &Listing) {
    *t += 0.05;
    ui.use_time(*t);
    let view = View {
        options: Some(Values::default()),
        preferences: Some(listing),
        ..View::default()
    };
    p.tick(ui, &view, &[], &mut Vec::new());
}

/// Point at the middle of an element and click it, the clock moving between.
fn click(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, name: &str, listing: &Listing) {
    let h: Handle = ui
        .element(Token::of(name))
        .unwrap_or_else(|| panic!("{name} made"));
    let r = ui.screen_rect(h).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    tick(ui, p, t, listing);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        tick(ui, p, t, listing);
    }
}

/// The rendering section's remaster row says "Remaster", its help this client's own, beside the
/// retail rows' words from the data.
#[test]
fn the_remaster_row_shows_this_clients_own_words() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut out = Vec::new();
    let mut p = GamePlay::new(&mut ui, &mut out);
    p.login_finished(&mut ui, &mut out);
    let mut t = 0.0;
    let describe = |key: u32, label: Token, help: Token| Description {
        section: section::RENDERING,
        key,
        table: client::PREFERENCE_STRINGS,
        label: label.raw(),
        help: help.raw(),
    };
    let full = client::flag(
        describe(
            0x4000_0003,
            Token::of("ID_Rendering_FullScreen"),
            Token::of("ID_Rendering_FullScreen_Help"),
        ),
        false,
        true,
    );
    let remaster = client::flag(
        describe(0x4100_7E01, own::REMASTER, own::REMASTER_HELP),
        true,
        false,
    );
    let listing = Listing {
        serial: 1,
        sections: vec![(section::RENDERING, vec![full, remaster])],
    };
    click(&mut ui, &mut p, &mut t, "OptionButton", &listing);
    click(&mut ui, &mut p, &mut t, "ClientTab", &listing);
    tick(&mut ui, &mut p, &mut t, &listing);
    let c = window(&p).client.as_ref().expect("the client page");
    let (rows, sections) = c.rows();
    let at = sections
        .iter()
        .find(|(s, _)| *s == section::RENDERING)
        .map(|(_, r)| *r)
        .expect("the section");
    let Control::Section { rows: of, .. } = &rows.all[at].control else {
        panic!("a section");
    };
    let words = |row: usize| {
        let e = rows.all[row].element.expect("made");
        let text = ui
            .child_recursive(e, helper::name::TEXT)
            .and_then(|h| ui.text(h))
            .unwrap_or_default();
        (text, ui.element_tooltip(e).unwrap_or_default().to_owned())
    };
    let (full_label, full_help) = words(of[0]);
    assert!(
        !full_label.is_empty() && !full_help.is_empty(),
        "the data's"
    );
    assert_ne!(full_label, "Remaster");
    assert_eq!(
        words(of[1]),
        (
            "Remaster".to_owned(),
            own::words(own::REMASTER_HELP.raw())
                .expect("its help")
                .to_owned()
        )
    );
    assert!(rows.all[of[1]].flag_now(&ui), "ticked as listed");
}
