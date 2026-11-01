//! The retail layouts built into elements.

use auberean_ui::draw::{self, Surface};
use auberean_ui::{Rect, Token, Ui};

use crate::install::records;

/// Every top-level element of every 1024 by 768 interface layout the install holds builds into an
/// element tree, and draws.
///
/// Behaviour: ui.layout.every-retail-layout-builds
#[test]
fn every_retail_layout_builds() {
    let layouts = [
        0x2200_0001u32,
        0x2200_0002,
        0x2200_0004,
        0x2200_0005,
        0x2200_0006,
        0x2200_0007,
        0x2200_0008,
        0x2200_001D,
        0x2200_0020,
        0x2200_0021,
        0x2200_0022,
        0x2200_002B,
    ];
    let mut built = 0;
    for id in layouts {
        let mut ui = Ui::new(records(), 1024, 768);
        let l = ui
            .load_layout(id)
            .unwrap_or_else(|| panic!("layout {id:08X}"));
        let names: Vec<Token> = {
            use auberean_ui::Assets;
            records()
                .layout(id)
                .unwrap()
                .elements
                .iter()
                .map(|e| Token(e.id))
                .collect()
        };
        for n in names {
            let h = ui
                .create_root(l, n)
                .unwrap_or_else(|| panic!("layout {id:08X} element {n:?}"));
            assert!(ui.node(h).is_some());
            built += 1;
        }
        let mut s = Surface::new(1024, 768);
        draw::draw(&ui, &mut s);
    }
    assert!(built > 700, "{built} top-level elements");
}

/// The character list's buttons stand where the retail screen shows them: the frame's right
/// column, 142 by 35 each, Create at (800, 300).
///
/// Behaviour: ui.layout.the-character-list-buttons-stand-where-retail-draws-them
#[test]
fn the_character_list_buttons_stand_where_retail_draws_them() {
    let mut ui = Ui::new(records(), 1024, 768);
    let l = ui.load_layout(0x2200_0002).unwrap();
    let bg = ui.create_root(l, Token::of("BackgroundField")).unwrap();
    for (name, y) in [
        ("CreateButton", 300),
        ("DeleteButton", 340),
        ("OptionsButton", 380),
        ("CreditsButton", 420),
        ("QuitButton", 460),
    ] {
        let b = ui.child_recursive(bg, Token::of(name)).unwrap();
        assert_eq!(
            ui.screen_rect(b),
            Some(Rect::sized(800, y, 142, 35)),
            "{name}"
        );
    }
}

/// A screen's layout is found by its layout number, as the screens ask for theirs: character
/// selection's (`0x41000006`) is the character selection record, the status bar's
/// (`0x4100000F`) the status bar record.
///
/// Behaviour: ui.layout.a-screen-finds-its-layout-by-number
#[test]
fn a_screen_finds_its_layout_by_number() {
    use auberean_ui::Assets;
    let r = records();
    let mapping = auberean_ui::manager::LAYOUT_MAPPING;
    assert_eq!(r.data_id_by_enum(0x4100_0006, mapping), 0x2200_0002);
    assert_eq!(r.data_id_by_enum(0x4100_000F, mapping), 0x2200_002B);
}

/// A button the layout names by reference, whose own first state is the plain one, is in its
/// normal state (and draws its picture) as soon as it is made, before the pointer has touched it:
/// the flags its template sets bring its state up to date as they are applied.
///
/// Behaviour: ui.button.a-button-shows-its-normal-state-before-the-pointer-touches-it
#[test]
fn a_button_shows_its_normal_state_before_the_pointer_touches_it() {
    let mut ui = Ui::new(records(), 1024, 768);
    let l = ui.load_layout(0x2200_0001).unwrap();
    let field = ui.create_root(l, Token::of("OptionField")).unwrap();
    for name in ["OptionCloseButton", "OptionsHelpButton"] {
        let b = ui.child_recursive(field, Token::of(name)).unwrap();
        let n = ui.node(b).unwrap();
        assert_eq!(n.state, Token::of("normal"), "{name}");
        assert!(n.picture.is_some(), "{name} draws its picture");
    }
}
