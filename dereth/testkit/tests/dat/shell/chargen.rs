//! Wizard layout and physical name-entry scenarios.

use crate::ui::{hud_find, who_holds_the_keyboard};
use dereth_testkit::{ClientSpec, HeadlessClient, Player};
use dereth_ui::ElementId;

dereth_testkit::scenarios! {
    scenario_every_wizard_page_lays_out => every_wizard_page_lays_out ["chargen.every-wizard-page-lays-out"],
    scenario_the_name_field_of_the_wizard_takes_a_typed_name => the_name_field_of_the_wizard_takes_a_typed_name ["ui.focus.the-name-field-of-the-wizard-takes-a-typed-name"],
}

// -------------------------------------------------------------------------------------------
// 17. chargen.every-wizard-page-lays-out
// -------------------------------------------------------------------------------------------

/// Every page of the character-creation wizard is reachable and lays out.
pub fn every_wizard_page_lays_out() {
    use dereth_ui_screens::screens::chargen::{CharGenScreen, EcgProgress};

    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(dereth_ui::framework::mode::CHAR_GEN),
        "the wizard is the screen this scenario is about"
    );
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("shell");
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let wizard = any.downcast_mut::<CharGenScreen>().expect("the wizard");
        assert!(
            wizard.tables.is_some(),
            "the host handed the wizard its char-gen tables"
        );
    }

    // Each page in turn, through its own tab, and what the tab produced.
    let mut laid_out: Vec<(String, usize)> = Vec::new();
    for page in EcgProgress::PAGES {
        let tab = page.select_button().expect("a real page has a tab");
        {
            let app = c.app_mut();
            let shell = app.ui().expect("shell");
            let root = shell.flow.current().expect("a screen").roots()[0];
            let h = shell
                .ui
                .get_child_recursive(root, tab)
                .unwrap_or_else(|| panic!("{page:?}'s tab is in the shipped layout"));
            let shell = app.ui_mut().expect("shell");
            shell.ui.broadcast_element_message(
                h,
                dereth_ui::msg::element::id::BUTTON_CLICKED,
                7,
                0,
            );
        }
        c.tick(4);
        let elements = {
            let shell = c.view().expect_app().ui().expect("shell");
            let root = shell.flow.current().expect("a screen").roots()[0];
            let mut all = Vec::new();
            walk(&shell.ui, root, &mut all);
            all.len()
        };
        println!("chargen {page:?}: {elements} elements");
        laid_out.push((format!("{page:?}"), elements));
    }

    c.assert_behaviour("chargen.every-wizard-page-lays-out", move |_v| {
        laid_out.len() == EcgProgress::PAGES.len() && laid_out.iter().all(|(_, n)| *n > 1)
    });
    c.shutdown();
}

fn walk(ui: &dereth_ui::UiSystem, h: dereth_ui::ElemHandle, out: &mut Vec<dereth_ui::ElemHandle>) {
    out.push(h);
    for c in ui.children(h) {
        walk(ui, c, out);
    }
}

// ---------------------------------------------------------------------------------------------
// ui.focus.the-name-field-of-the-wizard-takes-a-typed-name
// ---------------------------------------------------------------------------------------------

/// The wizard's name field takes a typed name. The two page presses are the route and not the
/// claim, so they go the short way; the press into the field and the typing are the claim and are
/// a real pointer and a real keyboard.
pub fn the_name_field_of_the_wizard_takes_a_typed_name() {
    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    for id in [0x1000_03BF_u32, 0x1000_03F4] {
        // A heritage, so the wizard has one, and then the summary page the field is on.
        let h = hud_find(&c, ElementId(id));
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell
            .ui
            .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
        c.tick(1);
    }

    let field = hud_find(&c, dereth_ui_screens::screens::chargen::NAME_FIELD);
    let before = wizard_name(&mut c);
    c.when(Player::Click(
        dereth_ui_screens::screens::chargen::NAME_FIELD.into(),
    ));
    let it_holds_the_keyboard = who_holds_the_keyboard(&c) == Some(field);

    c.when(Player::Type("Tarinell".into()));
    // The page puts its own prompt in the box and selects it, and a press into a box that already
    // holds the keyboard drops a caret rather than replacing the selection -- so what is claimed
    // here is that every typed character reached the wizard's record, not what it starts out
    // holding. Typing with no press at all replaces the whole selection and is a claim of the
    // wizard's own.
    let after = wizard_name(&mut c);
    let every_character_arrived = after.ends_with("Tarinell") && after != before;
    let the_wizard_has_a_name = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let s = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **s;
        any.downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
            .expect("the wizard")
            .name_entered
    };

    c.assert_behaviour(
        "ui.focus.the-name-field-of-the-wizard-takes-a-typed-name",
        move |_| it_holds_the_keyboard && every_character_arrived && the_wizard_has_a_name,
    );
    c.shutdown();
}

/// The name the wizard itself is holding -- the value the finish would carry.
fn wizard_name(c: &mut HeadlessClient) -> String {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let s = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
        .expect("the wizard")
        .state
        .name
        .clone()
}
