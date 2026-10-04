use super::*;

// ---------------------------------------------------------------------------------------------
// combat.window.* and vitals.row.*
//
// **The vitals row is `ui`'s subject and is kept here**, in `behaviours/combat.rs`, beside the
// combat window it is drawn with.
//
// **One narrowing, stated rather than hidden.** Each option box's caption id is the hash of its
// shipped localisation name (`ID_CombatPanelOption_*`), so the ids are the shipped names'; the
// English words are not asserted, because they are a transcription of the shipped table and the
// table is what resolves them. What is asserted instead is stronger about the client: the words
// the table gives reach the element **and the draw list**, and the three boxes do not all say the
// same thing.
// ---------------------------------------------------------------------------------------------

pub(super) fn the_combat_option_boxes_draw_their_shipped_captions() {
    let (mut ui, mut screen, combat) = panels::a_combat_window();
    panels::show_combat(&mut ui, &mut screen);
    let table =
        dereth_ui_screens::env::did_by_enum(&ui, 4, 0x1000_0003).expect("the shipped text table");
    let back = panels::drawn(&mut ui);

    let mut composed: Vec<String> = Vec::new();
    let mut every_box_holds = true;
    for (i, (caption, help)) in [
        (
            "ID_CombatPanelOption_AutoRepeatAttack",
            "ID_PlayerOption_AutoRepeatAttack_Help",
        ),
        (
            "ID_CombatPanelOption_AutoTarget",
            "ID_PlayerOption_AutoTarget_Help",
        ),
        (
            "ID_CombatPanelOption_ViewCombatTarget",
            "ID_PlayerOption_ViewCombatTarget_Help",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let h = combat.options[i].element;
        let caption_id = dereth_ui::persist::preferences::token_of(caption);
        let resolved = ui
            .resolve_string(table, caption_id)
            .expect("the table resolves it");
        let help_text = ui
            .resolve_string(table, dereth_ui::persist::preferences::token_of(help))
            .expect("and resolves the help line");
        let named = ui
            .node(h)
            .expect("alive")
            .merged_properties()
            .get_string_info(dereth_ui::props::attr::TEXT_STRING)
            .map(|si| (si.table_id, si.string_id));
        every_box_holds &= !resolved.is_empty()
            && named == Some((Some(table), Some(caption_id)))
            && panels::text(&mut ui, h) == resolved
            && panels::drawn_text(&back, h) == resolved
            && !help_text.is_empty()
            && ui.node(h).expect("alive").tooltip_text.as_deref() == Some(help_text.as_str());
        composed.push(resolved);
    }
    let all_different = {
        let mut v = composed.clone();
        v.sort();
        v.dedup();
        v.len() == composed.len()
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.the-option-boxes-draw-their-shipped-captions-and-carry-their-shipped-help",
        move |_| every_box_holds && all_different && composed.len() == 3,
    );
}

pub(super) fn the_attack_height_buttons_follow_the_notice() {
    use dereth_ui_screens::hud::combat_window::height;

    let (mut ui, mut screen, mut combat) = panels::a_combat_window();
    let group = combat.height_group.expect("the shipped height group");
    let ships_medium = ui
        .node(group)
        .expect("alive")
        .merged_properties()
        .get_enum(0xB0)
        == Some(0x1000_0058);
    panels::show_combat(&mut ui, &mut screen);

    // No notice yet: the window opens on the shipped default.
    let opens_on_medium = panels::height_art_holds(&mut ui, &combat, height::MEDIUM)
        && ui
            .node(group)
            .expect("alive")
            .merged_properties()
            .get_enum(0xB1)
            == Some(0x1000_0058);

    // Every change, including the first one repeating the default.
    let mut every_change_holds = true;
    for h in [height::MEDIUM, height::HIGH, height::LOW, height::MEDIUM] {
        every_change_holds &= combat.on_attack_height_changed(&mut ui, h)
            && panels::height_art_holds(&mut ui, &combat, h);
    }
    // And a change to no height at all is refused, leaving the height where it was.
    let refused = !combat.on_attack_height_changed(&mut ui, height::UNDEF)
        && panels::height_art_holds(&mut ui, &combat, height::MEDIUM);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.the-attack-height-buttons-follow-the-notice-and-draw-the-chosen-one",
        move |_| ships_medium && opens_on_medium && every_change_holds && refused,
    );
}

pub(super) fn pressing_the_chosen_height_again_keeps_it_chosen() {
    use dereth_ui::msg::element::id::BUTTON_CLICKED;
    use dereth_ui::widgets::groupbox as gb;
    use dereth_ui::StateId;

    let (mut ui, _, combat) = panels::a_combat_window();
    let group = combat.height_group.expect("the shipped height group");
    let high = combat.height_buttons[0].0;
    let low = combat.height_buttons[2].0;
    let receiver = dereth_ui::ListenerId::External(0x_66D0);

    let allows_repeats = ui
        .node(group)
        .expect("alive")
        .merged_properties()
        .get_bool(gb::ALLOW_RESELECT)
        == Some(true);
    ui.register_for_element_messages(group, receiver);
    ui.drain_outbox();
    let mut heard = |ui: &mut dereth_ui::UiSystem| {
        ui.drain_outbox()
            .into_iter()
            .filter(|d| {
                matches!(d, dereth_ui::Delivery::Element { to, msg }
                    if *to == receiver && msg.id == BUTTON_CLICKED)
            })
            .count()
    };

    ui.broadcast_element_message(high, BUTTON_CLICKED, 7, 0);
    let first = heard(&mut ui) == 1
        && ui.node(high).expect("alive").state == StateId(6)
        && ui
            .node(group)
            .expect("alive")
            .merged_properties()
            .get_enum(gb::SELECTED_BUTTON)
            == Some(0x1000_0057);

    // The press has already turned the button off by the time the panel hears about it, so the
    // panel putting it back is the whole of the claim -- with and without the gate.
    let mut both_gates_hold = true;
    for allow in [false, true] {
        ui.set_attribute_bool(group, gb::ALLOW_RESELECT, allow);
        ui.set_state(high, StateId(1));
        ui.broadcast_element_message(high, BUTTON_CLICKED, 7, 0);
        both_gates_hold &= heard(&mut ui) == usize::from(allow)
            && ui.node(high).expect("alive").state == StateId(6)
            && ui
                .node(high)
                .expect("alive")
                .merged_properties()
                .get_bool(0x0E)
                == Some(true);
    }

    // And the closed gate does not stop a press on a **different** height.
    ui.set_attribute_bool(group, gb::ALLOW_RESELECT, false);
    ui.broadcast_element_message(low, BUTTON_CLICKED, 7, 0);
    let a_different_one = heard(&mut ui) == 1
        && ui.node(low).expect("alive").state == StateId(6)
        && ui.node(high).expect("alive").state == StateId(1);

    // A message that is not a press selects nothing.
    ui.broadcast_element_message(high, dereth_ui::MessageId(0x0A), 0, 0);
    let not_a_press = ui.node(low).expect("alive").state == StateId(6);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.pressing-the-chosen-height-again-keeps-it-chosen",
        move |_| allows_repeats && first && both_gates_hold && a_different_one && not_a_press,
    );
}

pub(super) fn choosing_one_height_leaves_every_other_button_alone() {
    use dereth_ui::widgets::groupbox as gb;
    use dereth_ui::StateId;

    let (mut ui, _, combat) = panels::a_combat_window();
    let group = combat.height_group.expect("the shipped height group");
    let high = combat.height_buttons[0].0;
    let medium = combat.height_buttons[1].0;
    let low = combat.height_buttons[2].0;

    // The chosen one is found however deep it sits, and a button in a state of its own is left
    // exactly where it was.
    ui.set_parent(low, Some(high));
    ui.set_state(high, StateId(3));
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0x1000_0059);
    let nested = ui.node(low).expect("alive").state == StateId(6)
        && ui.node(medium).expect("alive").state == StateId(1)
        && ui.node(high).expect("alive").state == StateId(3);

    // Naming a button the panel does not have clears the old one and chooses nothing.
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0xDEAD_BEEF);
    let missing = ui.node(low).expect("alive").state == StateId(1);
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0);
    let nothing_named = ui.node(high).expect("alive").state == StateId(3);

    // A default of nothing does not overwrite a choice that has been made.
    ui.set_attribute_enum(group, gb::SELECTED_BUTTON, 0x1000_0058);
    ui.set_attribute_enum(group, gb::DEFAULT_BUTTON, 0);
    ui.post_init_tree(group);
    let kept = ui
        .node(group)
        .expect("alive")
        .merged_properties()
        .get_enum(gb::SELECTED_BUTTON)
        == Some(0x1000_0058)
        && ui.node(medium).expect("alive").state == StateId(6);

    // And re-reading the choice with nothing new to say puts the same one back.
    ui.set_state(medium, StateId(1));
    ui.on_set_attribute(group, gb::SELECTED_BUTTON, None);
    let re_applied = ui.node(medium).expect("alive").state == StateId(6);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.window.choosing-one-height-leaves-every-other-button-alone",
        move |_| nested && missing && nothing_named && kept && re_applied,
    );
}

pub(super) fn the_vital_rows_are_drawn_into_the_shipped_template() {
    use dereth_ui_screens::view::Vital;

    let (mut ui, mut screen, _) = panels::a_combat_window();
    let table_id =
        dereth_ui_screens::env::did_by_enum(&ui, 4, 0x1000_0001).expect("the shipped table");
    let row = ui
        .env()
        .map(|e| {
            e.with_assets(|assets| {
                let table =
                    <dereth_assets::ui::StringTable as dereth_assets::Decode>::decode_payload(
                        table_id,
                        &assets.read(table_id).expect("the table reads"),
                    )
                    .expect("and decodes");
                table
                    .strings
                    .into_iter()
                    .find(|(id, _)| *id == 0x0593_85AC)
                    .expect("the vitals row is in it")
                    .1
            })
        })
        .expect("the shipped assets are installed");
    // The row is a template of fragments with two holes in it, not a ready-made sentence.
    let is_a_template = row.strings.len() == 3 && row.variables.len() == 2;

    let mut every_reading_holds = true;
    for values in [
        [(35, 35), (70, 70), (50, 100)],
        [(5, 35), (0, 70), (91, 101)],
    ] {
        every_reading_holds &= screen.update_vitals(&mut ui, &panels::Vitals(values));
        // A frame whose numbers have not moved does not rewrite them.
        every_reading_holds &= !screen.update_vitals(&mut ui, &panels::Vitals(values));
        for (vital, (cur, max)) in Vital::ALL.into_iter().zip(values) {
            let (_, label) = dereth_ui_screens::hud::vitals::fields(vital);
            let want = format!(
                "{}{cur}{}{max}{}",
                row.strings[0], row.strings[1], row.strings[2]
            );
            for h in [
                screen
                    .stacked_vitals
                    .get(label)
                    .expect("the stacked layout's row"),
                screen
                    .side_vitals
                    .get(label)
                    .expect("the side layout's row"),
            ] {
                every_reading_holds &= panels::text(&mut ui, h) == want;
            }
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitals.row.the-numbers-are-drawn-into-the-shipped-template-in-both-layouts",
        move |_| is_a_template && every_reading_holds,
    );
}
