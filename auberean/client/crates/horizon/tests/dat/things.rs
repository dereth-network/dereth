//! Horizon's windows of the player's things in the shell, against the server double: the
//! inventory window's figure of the player.

use auberean_options::page::{section, Change, Request as PageRequest};
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{Choice, OverlayItem, PreviewSpace};

use crate::session::Session;

/// Horizon chosen on the options window's client page, and kept.
fn choose_horizon(s: &mut Session) {
    let p = &mut s.shell.preferences;
    p.handle(&PageRequest::Attach(vec![section::UI]));
    let mut row = p
        .listing()
        .and_then(|l| l.sections.iter().find(|(at, _)| *at == section::UI))
        .and_then(|(_, prefs)| {
            prefs.iter().find_map(|c| match c {
                Change::Choice(c) => Some(c.clone()),
                _ => None,
            })
        })
        .expect("the interface row");
    assert!(row.set_value(Choice::Horizon.value()));
    p.handle(&PageRequest::Change(Change::Choice(row)));
    p.handle(&PageRequest::Apply(section::UI));
    p.handle(&PageRequest::Detach(vec![section::UI]));
    s.step();
    s.step();
}

/// In the world, the inventory window opens on Horizon's inventory key (I) and shows the player's
/// figure as it looks in a scene of its own over the window: the window's batches drawn before the
/// figure's place among them stand under the scene, and the rest over it.
///
/// Behaviour: desktop.horizon.the-inventory-shows-the-player-s-figure-over-its-window
#[test]
fn horizons_inventory_shows_the_players_figure_over_its_window() {
    let mut s = Session::new(None);
    s.enter_the_world();
    choose_horizon(&mut s);
    assert_eq!(s.shell.interface(), Choice::Horizon);
    let before = s.frame();
    assert!(
        before.scenes.is_empty(),
        "no figure before the window opens"
    );
    for down in [true, false] {
        s.shell.event(&Event::Key {
            key: Key::I,
            down,
            repeat: false,
        });
        s.step();
    }
    let overlay = s.frame();
    let horizon = s.horizon();
    let h = horizon.borrow();
    let (rect, _) = h
        .ui()
        .goods
        .inventory
        .figure()
        .expect("the inventory is open");
    let scene = overlay
        .scenes
        .iter()
        .find(|v| v.figures.len() == 1 && v.figures[0].still)
        .expect("the player's figure");
    assert_eq!(scene.rect, [rect.x, rect.y, rect.right(), rect.bottom()]);
    assert!(scene.camera.orthographic, "seen straight on");
    let at = overlay
        .items
        .iter()
        .position(|i| {
            matches!(
                i,
                OverlayItem::Preview {
                    space: PreviewSpace::PaperDoll,
                    ..
                }
            )
        })
        .expect("the figure's place among the batches");
    assert!(at > 0, "the window stands under the figure");
    assert!(
        overlay.items[at..]
            .iter()
            .any(|i| matches!(i, OverlayItem::Triangles { .. })),
        "what follows stands over it"
    );
}

/// The money is drawn with the game's own picture of a pile of gold coins.
///
/// Behaviour: desktop.horizon.the-money-is-the-game-s-gold-picture
#[test]
fn the_money_is_the_games_gold_picture() {
    let (_, _, assets) = crate::session::data();
    let records: auberean_horizon::art::Records = std::sync::Arc::clone(assets) as _;
    let gold = records
        .image(auberean_horizon::ui::items::GOLD)
        .expect("the game's gold");
    assert_eq!((gold.width, gold.height), (25, 23));
    assert!(
        gold.bgra.chunks(4).any(|px| px[3] == 0xFF),
        "a picture that shows"
    );
}
