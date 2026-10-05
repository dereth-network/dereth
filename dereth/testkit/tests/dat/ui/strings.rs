//! UI fixtures and scenarios for strings.

use super::*;
// =============================================================================================
// strings.* -- what the shipped text says, and what the player reads
//
// Every pane puts a shipped line through the tidying the client does, so a doubled space in the
// shipped text does not reach the screen. The instrument's own calibration and a guard that an
// element id still exists are folded in below as arms rather than rows.
//
// The reader is the client's own, taken off a whole running client rather than built here, so
// what is asserted is what that client would draw.
// =============================================================================================

/// The table the panes about a death's penalty, allegiance, fellowship and trade all read.
const TEXT_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0001);
/// The table the key-binding page reads.
const KEYS_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0004);
/// The table of titles, and the only home of the line that asks to keep its spaces.
const TITLES_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_000E);
/// The table the key names themselves live on.
const KEY_NAMES_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0007);

/// The id a shipped line is filed under, from the name the client files it under.
fn line_id(token: &str) -> u32 {
    dereth_ui::persist::preferences::token_of(token)
}

/// A whole client, whose own reader is what every scenario below asks.
fn a_client_that_can_read_the_shipped_text() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The calibration, folded in here rather than kept as a scenario of its own: with no text
    // installed every reading below would be an empty answer dressed as a pass.
    {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        assert!(
            ui.resolve_string(TEXT_TABLE, line_id("ID_Vitae_Text_Skills"))
                .is_some(),
            "the shipped text has to be readable or nothing here is a measurement"
        );
        assert_eq!(
            ui.resolve_string(TEXT_TABLE, 0xDEAD_BEEF),
            None,
            "and a line that does not exist still answers with nothing"
        );
    }
    c
}

// ---------------------------------------------------------------------------------------------
// strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves
// ---------------------------------------------------------------------------------------------

/// The premise is asserted as well as the claim: the line really does ship the pair of spaces, so
/// "it was drawn with one" is a collapse and not a line that never had two.
pub(super) fn a_run_of_spaces_in_a_shipped_line_is_drawn_as_one() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let id = line_id("ID_Vitae_Text_Skills");

    let rendered = ui
        .resolve_string_rendered(TEXT_TABLE, id, &[String::from("5")])
        .expect("the shipped line");
    let it_is_collapsed = rendered.contains("by 5%. A reduction") && !rendered.contains("  ");

    // The premise: what ships.
    let pieces = ui
        .resolve_string_variants(TEXT_TABLE, id)
        .expect("the same line, untidied");
    let it_really_ships_two =
        format!("{}5{}", pieces[0], pieces[1]).contains("by 5%.  A reduction");

    // ...and nothing else moved: a line with no run and no markup is what it always was,
    // character for character. Nearly the whole of the shipped text is in this class.
    let the_rest_is_untouched = [
        "ID_Allegiance_MonarchLabel",
        "ID_SecureTrade_TotalItemsLabel",
    ]
    .into_iter()
    .all(|tok| {
        let id = line_id(tok);
        ui.resolve_string(TEXT_TABLE, id) == ui.resolve_string_unrendered(TEXT_TABLE, id)
    });

    c.assert_behaviour(
        "strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves",
        move |_| it_is_collapsed && it_really_ships_two && the_rest_is_untouched,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking
// ---------------------------------------------------------------------------------------------

/// The one line in the whole of the shipped text whose two readings differ by more than a run of
/// spaces.
pub(super) fn the_one_line_that_asks_to_keep_its_spaces_keeps_them() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let shown = ui
        .resolve_string(TITLES_TABLE, 0x00E3_C684)
        .expect("the shipped title line");
    let it_kept_them_and_lost_the_asking = shown == "Lore Master  Quiz Night";

    c.assert_behaviour(
        "strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking",
        move |_| it_kept_them_and_lost_the_asking,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint
// ---------------------------------------------------------------------------------------------

/// Both ends of the same line: given both of its values it reads as it should, and given neither
/// it is left with only what stood between them, which the tidying then takes away too.
pub(super) fn a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let id = line_id("ID_KeyNameWithSubControl");

    let both = ui
        .resolve_string_rendered(KEY_NAMES_TABLE, id, &["A".into(), "B".into()])
        .expect("the shipped three-piece line");
    let with_both = both == "A B";
    let with_neither = ui
        .resolve_string_rendered(KEY_NAMES_TABLE, id, &[])
        .as_deref()
        == Some("");

    c.assert_behaviour(
        "strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint",
        move |_| with_both && with_neither,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible
// ---------------------------------------------------------------------------------------------

/// Four shipped lines, each of which a caller placing values by position would get wrong in a
/// different way, and one negative that a caller placing them by position could never fail.
pub(super) fn a_line_places_the_values_by_name_and_not_by_the_order_given() {
    use dereth_ui_screens::panels::characterinfo::{
        compose, compose_in, query_duration, string, var, DURATION_TABLE,
    };

    let mut c = a_client_that_can_read_the_shipped_text();

    // 1. Two lines that name the same pair the opposite way round, from one named supply.
    let (one_conflict, one_line) = {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        let values = [("ACTION", "Turn Left"), ("KEY", "F7")];
        (
            ui.resolve_string_named(
                KEYS_TABLE,
                line_id("ID_ActionKeyMap_OverwriteExistingBinding"),
                &values,
            ),
            ui.resolve_string_named(KEYS_TABLE, line_id("ID_ActionKeyMap_Binding"), &values),
        )
    };
    let both_ways_round = one_conflict.as_deref()
        == Some("'F7' is currently bound to 'Turn Left'. Do you wish to erase that binding?")
        && one_line.as_deref() == Some("'Turn Left' ('F7')");

    // 2. A line that names the same value twice writes it twice, whichever way it is handed in.
    let (in_order, reversed) = {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        (
            compose(
                ui,
                string::RESISTS,
                &[(var::RESIST, "Hardy"), (var::REGEN, "Poor")],
            ),
            compose(
                ui,
                string::RESISTS,
                &[(var::REGEN, "Poor"), (var::RESIST, "Hardy")],
            ),
        )
    };
    let the_duplicate_is_written_twice = !in_order.starts_with(string::RESISTS)
        && in_order.matches("Hardy").count() == 2
        && in_order.matches("Poor").count() == 1
        && in_order == reversed;

    // 3. The widest line this client composes: seven terms, three of them blank, and the blanks
    // take their own words away with them.
    let (forward, backward) = {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        // A day, an hour, a minute and a second; years, months and weeks are nothing at all.
        let terms = query_duration(90_061);
        let named: Vec<(&str, &str)> = var::DURATION_TERMS
            .iter()
            .copied()
            .zip(terms.iter().map(String::as_str))
            .collect();
        let mut shuffled = named.clone();
        shuffled.reverse();
        (
            compose_in(ui, DURATION_TABLE, string::DURATION_FORMAT, &named),
            compose_in(ui, DURATION_TABLE, string::DURATION_FORMAT, &shuffled),
        )
    };
    let seven_terms_by_name = forward == "1 day 1 hour 1 minute 1 second"
        && forward == backward
        && !forward.contains('{')
        && !forward.contains("#1:");

    // 4. The two lines about the link, and the negative: a name the line does not carry puts
    // nothing on the screen, so the composer falls back to saying nothing but the line's own name.
    let (ping, loss, wrong) = {
        use dereth_ui_screens::panels::linkstatus::{string as link, var as link_var};
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        (
            compose(ui, link::PING, &[(link_var::PING, "42")]),
            compose(ui, link::PACKET_LOSS, &[(link_var::PACKET_LOSS, "1.50")]),
            compose(ui, link::PING, &[("PONG", "42")]),
        )
    };
    let the_link_lines = {
        use dereth_ui_screens::panels::linkstatus::string as link;
        !ping.starts_with(link::PING)
            && ping.contains("42")
            && !loss.starts_with(link::PACKET_LOSS)
            && loss.contains("1.50")
            && wrong == "ID_LinkStatus_Ping[42]"
    };

    c.assert_behaviour(
        "strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible",
        move |_| {
            both_ways_round
                && the_duplicate_is_written_twice
                && seven_terms_by_name
                && the_link_lines
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// strings.the-key-binding-prompt-loses-both-of-its-double-spaces
// ---------------------------------------------------------------------------------------------

/// The second place on the screen where the tidying is the difference between what ships and what
/// is read -- and this line ships two runs rather than one.
pub(super) fn the_key_binding_prompt_loses_both_of_its_double_spaces() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;

    let prompt = dereth_ui_screens::options::keybinding::map_instructions(ui, "Jump");
    let both_runs_gone = prompt.contains("the 'Jump' action. You may")
        && prompt.contains("new mappings. Remapping")
        && !prompt.contains("  ");
    // The premise: the line really does ship one of them.
    let it_really_ships_them = ui
        .resolve_string_variants(KEYS_TABLE, line_id("ID_ActionKeyMap_MapInstructions"))
        .expect("the shipped line")[1]
        .contains("action.  You");

    c.assert_behaviour(
        "strings.the-key-binding-prompt-loses-both-of-its-double-spaces",
        move |_| both_runs_gone && it_really_ships_them,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space
// ---------------------------------------------------------------------------------------------

/// Read off the glyphs the pane lays out, not off what the composer answered -- and the pane it
/// is read from is the one the shipped layout carries, which is the guard folded in here.
pub(super) fn the_pane_draws_the_tidied_line_and_not_the_shipped_double_space() {
    use dereth_rules::enchant::ench_type;

    let (mut c, _peer) = a_described_character();
    // A death's penalty, which is what the pane is about, through the one door one comes in by.
    enchant(
        &mut c,
        an_enchantment(666, 0, ench_type::VITAE | ench_type::MULTIPLICATIVE, 0.95),
    );
    light_the_lamp(&mut c, VITAE_LAMP);
    press_the_strip(&mut c, VITAE_LAMP);
    c.tick(2);

    let panel = hud_find(&c, dereth_ui_screens::panels::vitae::PANEL);
    let it_is_the_shipped_pane = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(panel)
        .expect("alive")
        .element_id()
        == ElementId(0x1000_018A);
    let main = hud_find(&c, dereth_ui_screens::panels::vitae::MAIN_TEXT);

    let drawn: String = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let box_ = shell.ui.screen_box(main);
        let t = shell
            .ui
            .text_element_mut(main)
            .expect("the pane's words are a text element");
        t.compose(box_)
            .into_iter()
            .filter_map(|g| char::from_u32(u32::from(g.ch)))
            .collect()
    };
    let one_space = drawn.contains("5%. A reduction") && !drawn.contains("  ");

    c.assert_behaviour(
        "strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space",
        move |_| it_is_the_shipped_pane && one_space,
    );
    c.shutdown();
}
