//! Shell fixtures and scenarios for wizard.

use super::*;
use dereth_ui_screens::screens::chargen;
// ---------------------------------------------------------------------------------------------
// chargen.exit.*
//
// The wizard's Exit button does something: it asks, in the shipped words, and a yes goes back to
// choosing a character.
// ---------------------------------------------------------------------------------------------

/// A client sitting on the character-creation wizard, offline.
pub(super) fn a_client_on_the_wizard() -> HeadlessClient {
    let c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(dereth_ui::framework::mode::CHAR_GEN),
        "the wizard is the screen these scenarios are about"
    );
    c
}

/// How many roots the wizard owns -- its own, plus one per dialog it has raised.
pub(super) fn wizard_roots(c: &mut HeadlessClient) -> usize {
    use dereth_ui::framework::Screen as _;
    with_wizard(c, |_, w| w.roots().len())
}

/// The wizard itself.
pub(super) fn with_wizard<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut dereth_ui_screens::screens::chargen::CharGenScreen,
    ) -> R,
) -> R {
    use dereth_ui::framework::Screen as _;

    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let wizard = any
        .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
        .expect("the wizard is current");
    f(&mut shell.ui, wizard)
}

/// Press a button of the wizard, the way the element manager delivers a press.
pub(super) fn press_wizard_button(c: &mut HeadlessClient, id: ElementId) {
    let h = element(c, id);
    press_handle(c, h);
}

pub(super) fn press_handle(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(1);
}

/// A child of a dialog the wizard raised.
pub(super) fn dialog_child(
    c: &mut HeadlessClient,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::ElemHandle {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(dialog, id)
        .expect("the dialog carries that child")
}

fn dialog_text(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// The warning as the shipped text table itself gives it, never a string written here.
fn the_shipped_exit_warning(c: &HeadlessClient) -> String {
    use dereth_ui_screens::screens::chargen::{ERROR_STRING_TABLE, EXIT_WARNING_STRING};
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        // The same hash of the same name the client itself looks the row up by.
        .resolve_string(
            ERROR_STRING_TABLE,
            dereth_ui::persist::preferences::token_of(EXIT_WARNING_STRING),
        )
        .expect("the warning is in the shipped text table")
}

/// The exit button raises a modal warning in the shipped words, and only one of it.
pub(super) fn the_exit_button_raises_a_modal_warning() {
    use dereth_ui_screens::screens::chargen::{CharGenDialog, EXIT_BUTTON};

    let mut c = a_client_on_the_wizard();
    let nothing_up = with_wizard(&mut c, |_, w| w.exit_dialog).is_none();

    press_wizard_button(&mut c, EXIT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the exit button raises one");
    let context = with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::Exit);
    let modal = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(dialog)
        .expect("live")
        .region
        .flags
        .block_clicks;

    let want = the_shipped_exit_warning(&c);
    let body = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::TEXT);
    let says_the_right_thing = !want.is_empty() && dialog_text(&mut c, body) == want;

    // A second press builds nothing: the same dialog, and the wizard still owns two roots.
    let roots = wizard_roots(&mut c);
    press_wizard_button(&mut c, EXIT_BUTTON);
    let only_one = with_wizard(&mut c, |_, w| w.exit_dialog) == Some(dialog)
        && wizard_roots(&mut c) == roots
        && roots == 2;

    c.assert_behaviour(
        "chargen.exit.the-exit-button-raises-a-modal-warning-in-the-shipped-words",
        move |_| nothing_up && context && modal && says_the_right_thing && only_one,
    );
    c.shutdown();
}

/// Saying yes goes back to choosing a character; saying no stays on the page.
pub(super) fn yes_leaves_the_wizard_and_no_stays_on_the_page() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::chargen::{EcgProgress, EXIT_BUTTON};

    // Yes.
    let mut c = a_client_on_the_wizard();
    press_wizard_button(&mut c, EXIT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the confirmation");
    let yes = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::BUTTON1);
    press_handle(&mut c, yes);
    // The screen changes at the end of the frame the request drained in.
    c.tick(1);
    let left_for_character_select = c
        .view()
        .expect_app()
        .ui()
        .and_then(|u| u.flow.current_mode())
        == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // No -- on a page that is not the first, so "where it was" is something to see.
    let mut c = a_client_on_the_wizard();
    press_wizard_button(
        &mut c,
        EcgProgress::Town.select_button().expect("the town tab"),
    );
    let on_that_page = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Town;
    press_wizard_button(&mut c, EXIT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the confirmation");
    let no = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::BUTTON2);
    press_handle(&mut c, no);
    c.tick(1);
    let stayed = c.view().expect_app().ui().and_then(|u| u.flow.current_mode())
        == Some(mode::CHAR_GEN)
        && with_wizard(&mut c, |_, w| w.exit_dialog).is_none()
        && with_wizard(&mut c, |_, w| w.open_dialog).is_none()
        // The dialog's root is deleted rather than orphaned.
        && wizard_roots(&mut c) == 1
        && with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Town;

    c.assert_behaviour(
        "chargen.exit.saying-yes-goes-back-to-choosing-a-character-and-saying-no-stays-on-the-page",
        move |_| left_for_character_select && on_that_page && stayed,
    );
    c.shutdown();
}

/// The back arrow is exit on the first page and a step back on any other.
pub(super) fn the_back_arrow_is_exit_only_on_the_first_page() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::chargen::{EcgProgress, LEFT_BUTTON};

    let mut c = a_client_on_the_wizard();
    let on_the_first_page = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage;

    press_wizard_button(&mut c, LEFT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the back arrow raises it too");
    let want = the_shipped_exit_warning(&c);
    let body = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::TEXT);
    let same_warning = dialog_text(&mut c, body) == want;

    let yes = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::BUTTON1);
    press_handle(&mut c, yes);
    c.tick(1);
    let lands_the_same_place = c
        .view()
        .expect_app()
        .ui()
        .and_then(|u| u.flow.current_mode())
        == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // ...and past the first page it steps back and raises nothing.
    let mut c = a_client_on_the_wizard();
    press_wizard_button(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    press_wizard_button(&mut c, LEFT_BUTTON);
    let stepped_back = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Profession
        && with_wizard(&mut c, |_, w| w.exit_dialog).is_none();
    press_wizard_button(&mut c, LEFT_BUTTON);
    let stepped_again = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage
        && with_wizard(&mut c, |_, w| w.exit_dialog).is_none();

    c.assert_behaviour(
        "chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other",
        move |_| {
            on_the_first_page
                && same_warning
                && lands_the_same_place
                && stepped_back
                && stepped_again
        },
    );
    c.shutdown();
}

// =============================================================================================
// chargen.* -- the wizard working, page by page
//
// A character can be created end to end. The left arrow is **not** a row here: what it does is
// already `chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other`, and
// a second scenario asserting the same id would be a second scenario for one claim.
//
// The bring-up is the exit scenarios' `a_client_on_the_wizard` and `press_wizard_button`; what is
// added here is `walk_the_wizard`, a player's walk through the wizard -- a heritage, a profession,
// a town, the summary tab and a name typed into the box.
// =============================================================================================

/// Press a wizard button and run the frame that drains the screen's action queue -- a click.
pub(super) fn click_wizard(c: &mut HeadlessClient, id: ElementId) {
    press_wizard_button(c, id);
}

/// The same press with **no** frame after it, for the two claims whose subject is what the press
/// put on the screen's own queue before anything drained it.
fn press_only(c: &mut HeadlessClient, id: ElementId) {
    let h = element(c, id);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

/// The wizard's own text of an element, as the glyphs it composed.
pub(super) fn wizard_text(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| {
            String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
        })
}

fn wizard_text_by_id(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = element(c, id);
    wizard_text(c, h)
}

fn wizard_state(c: &HeadlessClient, id: ElementId) -> dereth_ui::StateId {
    state_of(c, element(c, id))
}

fn wizard_visible(c: &HeadlessClient, id: ElementId) -> bool {
    let h = element(c, id);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .flags
        .visible
}

/// A word out of the shipped text table, by the same name the client looks it up by.
pub(super) fn shipped_word(c: &HeadlessClient, token: &str) -> String {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string(
            chargen::ERROR_STRING_TABLE,
            dereth_ui::persist::preferences::token_of(token),
        )
        .unwrap_or_else(|| panic!("{token} is in the shipped text table"))
}

/// Every line the summary list shows, in list order, with a row's two columns joined by a tab.
fn summary_lines(c: &mut HeadlessClient) -> Vec<String> {
    let rows = with_wizard(c, |_, w| w.summary_rows.clone());
    let mut out = Vec::new();
    for r in rows {
        let cells: Vec<dereth_ui::ElemHandle> = {
            let shell = c.view().expect_app().ui().expect("the UI shell is up");
            [
                chargen::summary_page::LINE_TEXT,
                chargen::summary_page::HEADER_TEXT,
                chargen::summary_page::PAIR_NAME,
                chargen::summary_page::PAIR_VALUE,
            ]
            .iter()
            .filter_map(|id| shell.ui.get_child_recursive(r, *id))
            .collect()
        };
        let parts: Vec<String> = cells
            .into_iter()
            .map(|h| wizard_text(c, h))
            .filter(|s| !s.is_empty())
            .collect();
        out.push(parts.join("\t"));
    }
    out
}

/// The player's own walk: an Aluvian Soldier out of Holtburg, named, on the summary page.
pub(super) fn walk_the_wizard(c: &mut HeadlessClient) {
    click_wizard(c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click_wizard(c, chargen::PROFESSION_BUTTONS[5].0);
    click_wizard(c, EcgProgress::Town.select_button().expect("the town tab"));
    click_wizard(c, chargen::TOWN_BUTTONS[0].0);
    click_wizard(
        c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let name = element(c, chargen::NAME_FIELD);
    {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell
            .ui
            .text_element_mut(name)
            .expect("the name box is a text element")
            .set_text("Tarinell");
        shell
            .ui
            .broadcast_element_message(name, dereth_ui::MessageId(0x44), 0, 0);
    }
    c.tick(1);
}

/// What the wizard has asked the host to do, drained without an application frame -- which is how
/// a claim about *what the press composed* is made on a client with no shard to send it to.
fn chargen_actions(
    c: &mut HeadlessClient,
    now: f64,
) -> Vec<dereth_ui_screens::screens::chargen::CharGenAction> {
    let host = c.view().expect_app().host_state().clone();
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.frame(
        dereth_primitives::LocalTime(now),
        &host,
        &mut dereth_ui::NullInputPump,
    );
    shell.take_chargen_actions()
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to
// ---------------------------------------------------------------------------------------------

/// The summary list, line by line, and the instruction pane beside it.
pub(super) fn the_summary_page_lists_every_choice_and_what_it_came_to() {
    let mut c = a_client_on_the_wizard();
    walk_the_wizard(&mut c);
    assert_eq!(with_wizard(&mut c, |_, w| w.progress), EcgProgress::Summary);

    let lines = summary_lines(&mut c);
    assert!(!lines.is_empty(), "the summary list has rows at all");
    let choices = lines[0] == "Profession: Soldier"
        && lines[1] == "Gender: Male"
        && lines[2] == "Heritage: Aluvian"
        && lines[3] == "Starting Town: Holtburg"
        && lines[4] == "Attributes";

    // The ten numbered rows. The values are read back off the model, so what this states is the
    // *shape* the page must have; the numbers themselves are pinned by the create-request row.
    let (s, e, co, q, f, sf, credits) = with_wizard(&mut c, |_, w| {
        (
            w.state.get(Attr::Strength),
            w.state.get(Attr::Endurance),
            w.state.get(Attr::Coordination),
            w.state.get(Attr::Quickness),
            w.state.get(Attr::Focus),
            w.state.get(Attr::Self_),
            w.state.remaining_skill_credits,
        )
    });
    let want = [
        format!("Strength\t{s}"),
        format!("Endurance\t{e}"),
        format!("Coordination\t{co}"),
        format!("Quickness\t{q}"),
        format!("Focus\t{f}"),
        format!("Self\t{sf}"),
        format!("Health\t{}", e / 2),
        format!("Stamina\t{e}"),
        format!("Mana\t{sf}"),
        format!("Skill Credits\t{credits}"),
    ];
    let numbers = lines[5..15] == want[..];

    let sections = chargen::SUMMARY_SKILL_SECTIONS
        .iter()
        .all(|(header, _)| lines.iter().any(|l| l == header));
    let spec_at = lines
        .iter()
        .position(|l| l == "Specialized Skills")
        .expect("the header");
    let trained_at = lines
        .iter()
        .position(|l| l == "Trained Skills")
        .expect("the header");
    let specialised = &lines[spec_at + 1..trained_at];
    // Four specialised skills, each with a score -- so none of them is the nothing a missing
    // formula would have printed.
    let skills_scored = specialised.len() >= 4
        && specialised.iter().all(|row| {
            row.split_once('\t')
                .and_then(|(_, score)| score.parse::<i32>().ok())
                .is_some_and(|score| score > 0)
        });

    let how_to = wizard_text_by_id(&mut c, chargen::summary_page::HOW_TO);
    let instructions = how_to.contains("A summary of the choices made so far")
        && how_to.contains("Aluvian")
        && how_to.len() > 600;

    let named = with_wizard(&mut c, |_, w| w.state.name.clone()) == "Tarinell";

    c.assert_behaviour(
        "chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to",
        move |_| choices && numbers && sections && skills_scored && instructions && named,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else
// ---------------------------------------------------------------------------------------------

/// FINISH and the forward arrow swap places on the last page, on every page, and the button is
/// dead everywhere else -- which a layout-only reading would miss.
pub(super) fn finish_replaces_the_forward_arrow_on_the_last_page_alone() {
    let mut c = a_client_on_the_wizard();
    let mut swapped_everywhere = true;
    let mut dead_elsewhere = true;
    let mut refuses_for_its_own_reason = false;

    for page in EcgProgress::PAGES {
        click_wizard(&mut c, page.select_button().expect("a real page has a tab"));
        let last = page == EcgProgress::Summary;
        swapped_everywhere &=
            wizard_visible(&c, FINISH_BUTTON) == last && wizard_visible(&c, RIGHT_BUTTON) == !last;

        press_only(&mut c, FINISH_BUTTON);
        let actions = chargen_actions(&mut c, 1.0);
        let error = with_wizard(&mut c, |_, w| w.error_string_id);
        if last {
            // On the last page it refuses for a reason of its own -- no name has been typed --
            // which is the create path's first line and not the page gate.
            refuses_for_its_own_reason =
                actions.is_empty() && error == Some("ID_CharGen_NoNameWarning");
        } else {
            dead_elsewhere &= actions.is_empty() && error.is_none();
        }
        with_wizard(&mut c, |_, w| w.open_dialog = None);
    }

    c.assert_behaviour(
        "chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else",
        move |_| swapped_everywhere && dead_elsewhere && refuses_for_its_own_reason,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.composes-the-character-the-player-built-and-only-once
// ---------------------------------------------------------------------------------------------

/// What pressing FINISH would send, field by field, and the guard against sending it twice.
pub(super) fn finish_composes_the_character_the_player_built_and_only_once() {
    use dereth_assets::Decode as _;
    use dereth_primitives::{AssetSource as _, DataId};

    let mut c = a_client_on_the_wizard();
    walk_the_wizard(&mut c);

    // The shipped tables the build is checked against, opened from the client's own store.
    let (cg, skills) = {
        let store = c.dat_store().expect("a retail client has a store").clone();
        let cg_bytes = store
            .read(DataId(0x0E00_0002))
            .expect("the character-creation table");
        let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes)
            .expect("it decodes");
        let sk_bytes = store.read(DataId(0x0E00_0004)).expect("the skill table");
        let sk = dereth_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes)
            .expect("it decodes");
        (cg, sk)
    };

    let (heritage, gender, template, start_area, attrs, levels) = with_wizard(&mut c, |_, w| {
        (
            w.state.heritage_group,
            w.state.gender,
            w.state.template,
            w.state.start_area,
            [
                w.state.get(Attr::Strength),
                w.state.get(Attr::Endurance),
                w.state.get(Attr::Coordination),
                w.state.get(Attr::Quickness),
                w.state.get(Attr::Focus),
                w.state.get(Attr::Self_),
            ],
            w.state.skill_levels.clone(),
        )
    });
    let walked_where_it_was_told = heritage == 1 && start_area == 0 && template == 6;

    // The press. Unspent credits raise a warning first, and answering yes re-enters the create
    // path -- which is the client's own route and not a shortcut around it.
    press_only(&mut c, FINISH_BUTTON);
    with_wizard(&mut c, |_, w| {
        if w.open_dialog.is_some() {
            w.close_dialog(true);
        }
    });
    let actions = chargen_actions(&mut c, 2.0);
    let [dereth_ui_screens::screens::chargen::CharGenAction::SendCharGenResult(result)] =
        actions.as_slice()
    else {
        panic!("FINISH did not ask to create anything: {actions:?}");
    };

    let identity = result.name == "Tarinell"
        && result.heritage_group == heritage
        && result.gender == gender
        && result.start_area == start_area
        && result.template_num == template;
    let six = [
        result.strength,
        result.endurance,
        result.coordination,
        result.quickness,
        result.focus,
        result.self_,
    ] == attrs;
    let whole_budget = attrs.iter().sum::<i32>()
        == i32::try_from(cg.heritage_groups[&heritage].attribute_credits).expect("it fits");

    let every_skill = result.skill_advancement_classes.len() == levels.len()
        && levels
            .iter()
            .enumerate()
            .all(|(id, want)| result.skill_advancement_classes[id] == *want as i32);

    let mut used = 0;
    let mut trained = 0;
    let mut every_one_is_real = true;
    for (id, sac) in result.skill_advancement_classes.iter().enumerate() {
        if *sac < 2 {
            continue;
        }
        trained += 1;
        let id = u32::try_from(id).expect("a skill id fits");
        every_one_is_real &= skills.skills.contains_key(&id);
        let (t, s) = dereth_chargen::CharGenState::skill_costs(&cg, &skills, heritage, id);
        used += if *sac == 3 { s } else { t };
    }
    let affordable = trained >= 8
        && every_one_is_real
        && used <= i32::try_from(cg.heritage_groups[&heritage].skill_credits).expect("it fits");

    let sex = &cg.heritage_groups[&heritage].sexes[&gender];
    #[allow(clippy::cast_sign_loss)]
    let looks = result.hair_style >= 0
        && (result.hair_style as usize) < sex.hair_styles.len()
        && result.hair_color >= 0
        && (result.hair_color as usize) < sex.hair_colors.len()
        && result.eye_color >= 0
        && (result.eye_color as usize) < sex.eye_colors.len()
        && result.eyes_strip >= 0
        && result.nose_strip >= 0
        && result.mouth_strip >= 0
        && result.is_admin == 0
        && result.is_envoy == 0;

    // And it survives the trip on to the wire and back, with the client's own checksum.
    let mut msg = dereth_client_runtime::app::chargen_result_to_wire(result);
    msg.checksum_value = msg.checksum();
    let m = dereth_protocol::login::CharacterSendCharGenResult {
        account: "ac01".into(),
        result: msg.clone(),
    };
    let body = dereth_protocol::write_body(&m).expect("the create message encodes");
    let back =
        dereth_protocol::read_body::<dereth_protocol::login::CharacterSendCharGenResult>(&body)
            .expect("and decodes again");
    let round_trips = back.result == msg && back.result.checksum_value == back.result.checksum();

    // A second press creates nothing, which is what keeps a double click from making two
    // characters.
    press_only(&mut c, FINISH_BUTTON);
    let only_once = chargen_actions(&mut c, 3.0).is_empty();

    c.assert_behaviour(
        "chargen.finish.composes-the-character-the-player-built-and-only-once",
        move |_| {
            walked_where_it_was_told
                && identity
                && six
                && whole_budget
                && every_skill
                && affordable
                && looks
                && round_trips
                && only_once
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people
// ---------------------------------------------------------------------------------------------

/// Which heritage bullets are lit, in shipped-layout order.
fn lit_heritages(c: &HeadlessClient) -> Vec<ElementId> {
    chargen::HERITAGE_BUTTONS
        .iter()
        .filter(|(id, _)| wizard_state(c, *id) == chargen::STATE_PROFESSION_ON)
        .map(|(id, _)| *id)
        .collect()
}

/// Exactly one bullet is lit, and the page behind it is that people's.
pub(super) fn choosing_a_heritage_lights_its_bullet_and_describes_that_people() {
    let mut c = a_client_on_the_wizard();

    // The wizard opens on a rolled heritage with that bullet already lit, which is the state a
    // player finds the page in before touching anything.
    let opened_on = with_wizard(&mut c, |_, w| w.state.heritage_group);
    let rolled = opened_on != 0;
    let want = chargen::HERITAGE_BUTTONS
        .iter()
        .find(|(_, h)| *h == opened_on)
        .map(|(id, _)| *id)
        .expect("the rolled heritage has a bullet");
    let opens_lit = lit_heritages(&c) == vec![want];

    let mut one_lit = true;
    let mut picture_follows = true;
    let mut described = true;
    for (button, heritage) in [
        (chargen::HERITAGE_BUTTONS[0].0, 1_u32),
        (chargen::HERITAGE_BUTTONS[2].0, 3),
        (chargen::HERITAGE_BUTTONS[7].0, 8),
    ] {
        click_wizard(&mut c, button);
        one_lit &= with_wizard(&mut c, |_, w| w.state.heritage_group) == heritage
            && lit_heritages(&c) == vec![button];

        let (_, want_state, bonus, desc) = chargen::HERITAGE_PAGE
            .iter()
            .find(|(h, _, _, _)| *h == heritage)
            .copied()
            .expect("every heritage has a row");
        picture_follows &= wizard_state(&c, chargen::heritage_page::BACKGROUND) == want_state;

        let pane = wizard_text_by_id(&mut c, chargen::heritage_page::TEXT);
        described &= !pane.is_empty()
            && [
                chargen::HERITAGE_SKILLS_HEADER,
                chargen::HERITAGE_SKILLS_BODY,
                bonus,
                desc,
            ]
            .iter()
            .all(|token| pane.contains(shipped_word(&c, token).trim_end()));
    }

    c.assert_behaviour(
        "chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people",
        move |_| rolled && opens_lit && one_lit && picture_follows && described,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town
// ---------------------------------------------------------------------------------------------

/// Exactly one map pin is lit, the page is titled with the town's name, and the pane describes it.
pub(super) fn choosing_a_town_lights_its_pin_and_titles_the_page() {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Town.select_button().expect("the town tab"),
    );

    let mut one_pin = true;
    let mut titled = true;
    let mut described = true;
    for (button, area) in [
        (chargen::TOWN_BUTTONS[0].0, 0_usize),
        (chargen::TOWN_BUTTONS[2].0, 2),
        (chargen::TOWN_BUTTONS[1].0, 1),
    ] {
        click_wizard(&mut c, button);
        let lit: Vec<ElementId> = chargen::TOWN_BUTTONS
            .iter()
            .filter(|(id, _)| wizard_state(&c, *id) == chargen::town_page::PIN_ON)
            .map(|(id, _)| *id)
            .collect();
        one_pin &=
            with_wizard(&mut c, |_, w| w.state.start_area) as usize == area && lit == vec![button];

        titled &= wizard_state(&c, chargen::town_page::TITLE)
            == chargen::town_page::TITLE_STATES[area]
            && wizard_text_by_id(&mut c, chargen::town_page::TITLE) == chargen::TOWN_NAMES[area];

        let pane = wizard_text_by_id(&mut c, chargen::town_page::TEXT);
        let body = shipped_word(&c, chargen::town_page::TEXT_TOKENS[area]);
        let frame = shipped_word(&c, chargen::town_page::HOW_TO);
        described &= pane == format!("{body}\n\n{frame}\n");
    }

    c.assert_behaviour(
        "chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town",
        move |_| one_pin && titled && described,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one
// ---------------------------------------------------------------------------------------------

/// One tab is bright, and it is the page the wizard is on -- from every page.
pub(super) fn the_tab_strip_brightens_only_the_page_it_is_on() {
    let mut c = a_client_on_the_wizard();
    let mut only_that_one = true;
    for page in EcgProgress::PAGES {
        click_wizard(&mut c, page.select_button().expect("a real page has a tab"));
        for other in EcgProgress::PAGES {
            let id = other.select_button().expect("a real page has a tab");
            let want = if other == page {
                STATE_TAB_ON
            } else {
                STATE_TAB_OFF
            };
            only_that_one &= wizard_state(&c, id) == want;
        }
    }

    c.assert_behaviour(
        "chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one",
        move |_| only_that_one,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say
// ---------------------------------------------------------------------------------------------

/// Each slider carries its attribute's name, its number, and a thumb at that number.
pub(super) fn the_six_attribute_sliders_are_named_and_sit_at_their_values() {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click_wizard(&mut c, chargen::PROFESSION_BUTTONS[5].0);

    let mut named = true;
    let mut numbered = true;
    let mut thumbs = true;
    for (field, attr) in ATTRIBUTE_SLIDERS {
        let f = element(&c, field);
        let (name, value, bar) = {
            let shell = c.view().expect_app().ui().expect("the UI shell is up");
            (
                shell
                    .ui
                    .get_child_recursive(f, chargen::slider::NAME)
                    .expect("the caption"),
                shell
                    .ui
                    .get_child_recursive(f, chargen::slider::VALUE)
                    .expect("the number"),
                shell
                    .ui
                    .get_child_recursive(f, chargen::slider::SCROLL)
                    .expect("the bar"),
            )
        };
        named &= wizard_text(&mut c, name) == chargen::attribute_name(attr);
        let want = with_wizard(&mut c, |_, w| w.state.get(attr));
        numbered &= wizard_text(&mut c, value) == want.to_string();

        let pos = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(bar)
            .expect("a live node")
            .merged_properties()
            .get(chargen::ATTR_SCROLL_POSITION)
            .and_then(|v| match v {
                dereth_assets::ui::PropertyValue::Float(f) => Some(*f),
                _ => None,
            })
            .expect("the bar carries a position");
        #[allow(clippy::cast_precision_loss)]
        let expected = want as f32 * 0.01;
        thumbs &= (pos - expected).abs() < 1e-5 && pos > 0.0;
    }

    // A Soldier is not six of the same number, so the sliders really do differ from one another.
    let values: Vec<i32> = ATTRIBUTE_SLIDERS
        .iter()
        .map(|(_, a)| with_wizard(&mut c, |_, w| w.state.get(*a)))
        .collect();
    let a_real_build = values.iter().any(|v| *v != values[0]);

    c.assert_behaviour(
        "chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say",
        move |_| named && numbered && thumbs && a_real_build,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt
// ---------------------------------------------------------------------------------------------

/// The name box on the last page prompts, takes the keyboard by itself, and what is typed
/// replaces the prompt rather than being typed on top of it.
pub(super) fn the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt() {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );

    let name = element(&c, chargen::NAME_FIELD);
    let took_the_keyboard = what_holds_the_keyboard(&c) == Some(name);
    let prompt = shipped_word(&c, chargen::NAME_PROMPT);
    let prompts =
        wizard_text(&mut c, name) == prompt && !with_wizard(&mut c, |_, w| w.name_entered);
    // **The whole prompt is selected**, which is what makes the first character typed replace it
    // rather than join it -- and it is the selection the client's own reader answers with, not a
    // pair of endpoints left over from some earlier gesture.
    let selected = {
        let t = c
            .app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .text_element_mut(name)
            .expect("a text element");
        let whole = t.get_selection() == Some((0, prompt.chars().count()));
        // Selected, and not by a press that is still held: those are two different bits.
        whole && t.bits.selecting() && !t.bits.selection_from_press()
    };

    // No click first: the page has already given the box the keyboard, so a player can simply
    // type, and the whole prompt is selected so the first character replaces it.
    {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        for ch in "Tarinell".encode_utf16() {
            shell.ui.character(ch);
        }
        shell
            .ui
            .broadcast_element_message(name, dereth_ui::MessageId(0x44), 0, 0);
    }
    c.tick(1);
    let replaced = with_wizard(&mut c, |_, w| w.name_entered)
        && with_wizard(&mut c, |_, w| w.state.name.clone()) == "Tarinell"
        && wizard_text(&mut c, name) == "Tarinell"
        // ...and the selection went with the prompt it covered.
        && c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .text_element_mut(name)
            .and_then(|t| t.get_selection())
            .is_none();

    // And leaving the page and coming back does not put the prompt over the top of it.
    click_wizard(
        &mut c,
        EcgProgress::Hertage
            .select_button()
            .expect("the heritage tab"),
    );
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let kept = wizard_text(&mut c, name) == "Tarinell";

    c.assert_behaviour("chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt", move |_| {
        took_the_keyboard && prompts && selected && replaced && kept
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names
// ---------------------------------------------------------------------------------------------

/// The FINISH button has a caption, in the font the layout names, and that font really draws it.
pub(super) fn the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names() {
    let mut c = a_client_on_the_wizard();
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let shown = wizard_visible(&c, FINISH_BUTTON);
    let h = element(&c, FINISH_BUTTON);
    let font = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("a live node")
        .merged_properties()
        .get(dereth_ui::props::attr::TEXT_FONT_DID)
        .and_then(|v| match v {
            dereth_assets::ui::PropertyValue::Array(a) => match a.first().map(|e| &e.value) {
                Some(dereth_assets::ui::PropertyValue::DataFile(d)) => Some(*d),
                _ => None,
            },
            _ => None,
        });
    let names_a_font = font.is_some();
    let captioned = !wizard_text(&mut c, h).is_empty();

    // ...and the font draws the letters of the caption. It is the font's own sheet that a caption
    // is drawn from, and a sheet must not be refused for being too tall.
    let store = c.dat_store().expect("a retail client has a store").clone();
    let atlas = dereth_client_shell::ui_draw::build_font_atlas(&store, font.expect("a font"))
        .expect("the caption's font rasterises");
    let legible = "FINISH".chars().all(|ch| atlas.glyph(ch).is_some());

    c.assert_behaviour(
        "chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names",
        move |_| shown && names_a_font && captioned && legible,
    );
    c.shutdown();
}

// =============================================================================================
// pointer.click.* -- a real press works on every screen
//
// A real press reaches the character-creation wizard -- a tab, an arrow, the way out -- just as it
// reaches the character list. The wheel's own map is not a row of its own: its data half is folded
// into the map row here, and its other half would be a constant beside the symbol the client
// carries it through.
//
// The rows are in this subject rather than the one the census names, because everything they are
// about -- the wizard, its tabs, its arrows -- is here.
// =============================================================================================

/// A client on the character list, then in the wizard, reached the way a player reaches it.
fn a_client_in_the_wizard_from_the_list(hands: &mut Hands) -> HeadlessClient {
    let mut c = a_client_on_character_select();
    let create = element(&c, ElementId(0x1000_03A0));
    hands.click_handle(&mut c, create);
    assert_eq!(
        current_screen(&c),
        Some(mode::CHAR_GEN),
        "the press reached the character list and it put the wizard up"
    );
    c.tick(1);
    c
}

/// Press an element at its own middle, without asking the hit test to resolve to it exactly --
/// a tab's middle can land on a caption inside it, which is still the tab being pressed.
fn press_at_middle(c: &mut HeadlessClient, hands: &mut Hands, id: ElementId) {
    let h = element(c, id);
    let (x, y) = middle_of(c, h);
    hands.click_at(c, x, y);
}

// ---------------------------------------------------------------------------------------------
// pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard
// ---------------------------------------------------------------------------------------------

/// The same press, on the screen that answered it and the screen that did not.
pub(super) fn the_same_press_works_on_the_character_list_and_in_the_wizard() {
    let mut hands = Hands::new();
    // The character-list half is the control: if it stops working the press is gone and the
    // wizard half proves nothing.
    let mut c = a_client_in_the_wizard_from_the_list(&mut hands);

    let opens_on_the_first = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage;
    // The profession tab -- one of the three the wizard greys on every page change, and the press
    // that was reported reaching nothing.
    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03F0));
    let the_tab_answered = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Profession;

    // Both presses went through the client's own pointer, which is what makes them real.
    let stats = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    let two_real_presses = stats.mouse_downs == 2 && stats.mouse_ups == 2;

    c.assert_behaviour(
        "pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard",
        move |_| opens_on_the_first && the_tab_answered && two_real_presses,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press
// ---------------------------------------------------------------------------------------------

/// The other two things reported: an arrow, and the way out.
pub(super) fn the_wizards_arrows_and_its_way_out_all_answer_a_press() {
    let mut hands = Hands::new();
    let mut c = a_client_in_the_wizard_from_the_list(&mut hands);

    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03C7));
    let forward = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Profession;
    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03C6));
    let back = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage;

    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03CA));
    let the_way_out = with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::Exit);

    c.assert_behaviour(
        "pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press",
        move |_| forward && back && the_way_out,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys
// ---------------------------------------------------------------------------------------------

/// The defect itself, read off the tree rather than through a press: a button that is greyed must
/// not stop being something a pointer can land on.
pub(super) fn every_button_the_wizard_builds_can_be_pressed() {
    let mut hands = Hands::new();
    let mut c = a_client_in_the_wizard_from_the_list(&mut hands);
    // Change the page, so the greying has been written at least twice.
    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03F2));

    const BUTTON: dereth_ui::ElementType = dereth_ui::ElementType(1);
    let (buttons, dead) = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("the wizard").roots()[0];
        fn walk(
            ui: &dereth_ui::UiSystem,
            h: dereth_ui::ElemHandle,
            buttons: &mut u32,
            dead: &mut Vec<u32>,
        ) {
            if let Some(n) = ui.node(h) {
                if n.desc.ty == BUTTON {
                    *buttons += 1;
                    if !n.is_mouse_visible {
                        dead.push(n.element_id().0);
                    }
                }
            }
            for child in ui.children(h) {
                walk(ui, child, buttons, dead);
            }
        }
        let mut buttons = 0_u32;
        let mut dead = Vec::new();
        walk(&shell.ui, root, &mut buttons, &mut dead);
        (buttons, dead)
    };
    // The denominator, so a sweep that found nothing cannot pass for a sweep that found nothing
    // wrong.
    let the_whole_tree = buttons > 100;
    let none_is_dead = dead.is_empty();

    // ...and the three the wizard greys on every page change, by name -- a sweep that happens to
    // pass says less than one that names its suspects.
    let mut the_three = true;
    for id in [0x1000_03F0_u32, 0x1000_03F1, 0x1000_03F3] {
        let h = element(&c, ElementId(id));
        the_three &= c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("alive")
            .is_mouse_visible;
    }

    c.assert_behaviour(
        "pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys",
        move |_| the_whole_tree && none_is_dead && the_three,
    );
    c.shutdown();
}
