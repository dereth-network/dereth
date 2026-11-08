//! The panels' help buttons.

use auberean_ui::{Token, Ui};
use auberean_ui_game::gameplay::GamePlay;

use crate::common::records;

/// There is no help panel, so no panel shows a help button: not while its panel is hidden, and not
/// once the panel shows. A panel made as it opens is checked with the book.
///
/// Behaviour: ui.panels.help-buttons-do-not-show
#[test]
fn the_panels_help_buttons_do_not_show() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut out = Vec::new();
    let mut play = GamePlay::new(&mut ui, &mut out);
    play.login_finished(&mut ui, &mut out);

    for name in [
        "ExaminationHelpButton",
        "InventoryHelpButton",
        "NavigationHelpButton",
        "OptionsHelpButton",
        "QuestHelpButton",
        "SkillHelpButton",
        "SocialHelpButton",
    ] {
        let button = ui
            .element(Token::of(name))
            .unwrap_or_else(|| panic!("{name} is on its panel"));
        let mut up = ui.node(button).and_then(|n| n.parent);
        while let Some(parent) = up {
            ui.set_visible(parent, true);
            up = ui.node(parent).and_then(|n| n.parent);
        }
        assert!(!ui.shown(button), "{name} shows");
        let panel = ui.node(button).and_then(|n| n.parent);
        assert!(panel.is_some_and(|p| ui.shown(p)), "{name}'s panel shows");
    }
}
