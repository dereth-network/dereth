//! Shell fixtures and scenarios for dialog keys.

use super::*;
// =============================================================================================
// dialog-keys.* -- the two keys the pre-game screens answer
//
// Where the dialog key map is registered is not a row of its own; the census of which shipped
// elements carry an input map of their own is folded into the last scenario here, as the
// denominator that census exists to be.
//
// Every key below is a real key-down / character / key-up trio built by the client's own pump and
// delivered where the window loop delivers one, so which map wins the key is the thing under test
// rather than a fixture standing in for it. Nothing here injects a bound action, except once,
// deliberately, as a control.
// =============================================================================================

/// The two actions the dialog map declares: answer, and back out.
const ACCEPT_INPUT: u32 = 0x25;
const ESCAPE_KEY: u32 = 0x27;

/// The live map stack, in the order the client walks it.
pub(super) fn map_stack(c: &mut HeadlessClient) -> Vec<InputMapEntry> {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the UI shell")
        .manager
        .maps
        .entries()
        .to_vec()
}

/// Which map and action the key **the shipped dialog map binds to `want`** resolves to, against
/// the stack as it stands.
///
/// The control comes out of the shipped keymap rather than out of a number here, so this is
/// exactly the question "does the dialog map still win the key it declares". It drives no frame,
/// so it can be asked before and after a gesture.
fn dialog_key_resolves_to(c: &mut HeadlessClient, want: u32) -> Option<(InputMapId, ActionId)> {
    let entries = map_stack(c);
    let shell = c.app_mut().input_manager_mut().expect("an input shell");
    let km = &shell.manager.keymap;
    let section = km
        .section(dereth_input::MAP_DIALOG_BOXES)
        .expect("the dialog section");
    let (qc, _) = section
        .bindings()
        .iter()
        .find(|(_, a)| a.0 == want)
        .copied()
        .unwrap_or_else(|| panic!("the dialog map binds action {want:#X}"));
    // Presented the way a live press is: the release bit cleared and the live bit set.
    let live = ControlChord::new(
        qc.control,
        qc.meta_mode,
        (qc.activation & !activation::UP) | activation::LIVE,
    );
    let is_keyboard = km.device_type_of(qc.control) == Some(DeviceType::Keyboard);
    assert!(
        is_keyboard,
        "both of the dialog map's controls are keyboard controls"
    );
    walk_input_maps(&entries, &live, is_keyboard, |m| km.section(m))
        .map(|r| (r.input_map, r.action))
}

/// How many entries of the stack are the dialog map at the screens' own priority.
///
/// There can be two -- a screen's own and a focused box's -- and they are different callbacks at
/// the same priority, so only a count tells them apart from outside.
fn dialog_map_entries(c: &mut HeadlessClient) -> usize {
    map_stack(c)
        .iter()
        .filter(|e| e.map == InputMapId(9) && e.priority == 3000)
        .count()
}

/// A key pressed and released with the character the desktop makes of it in between -- which is
/// what a player's finger really sends.
fn key_trio(c: &mut HeadlessClient, hands: &mut Hands, code: KeyCode, ch: char) {
    hands.key(c, key(code), true);
    hands.character(c, ch);
    hands.key(c, key(code), false);
    c.tick(1);
}

/// The key without the character: the road a bound action takes, on its own.
fn key_only(c: &mut HeadlessClient, hands: &mut Hands, code: KeyCode) {
    hands.key(c, key(code), true);
    hands.key(c, key(code), false);
    c.tick(1);
}

fn screen_stats(c: &HeadlessClient) -> (u64, u64, u64) {
    let st = &c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    (st.intro_actions, st.intro_characters, st.mode_switches)
}

fn registered_mode_maps(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .registered_mode_maps()
        .to_vec()
}

/// The maps the character list registers for itself: its own dialog keys, as the retail screen
/// does, and the scrollable controls beside them, so the mouse wheel scrolls what is under the
/// pointer with nothing focused (CD-017).
fn character_screen_maps() -> Vec<u32> {
    vec![
        dereth_input::MAP_DIALOG_BOXES.0,
        dereth_client::ui::CHARACTER_SCREEN_SCROLL_MAP,
    ]
}

/// `maps` as a set, sorted, for a comparison that does not turn on registration order.
fn sorted(mut maps: Vec<u32>) -> Vec<u32> {
    maps.sort_unstable();
    maps
}

/// Three characters, so that deleting one has something to ask about.
pub(super) fn three_characters() -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: ["Zephyr", "Aluvia", "Marbo"]
            .iter()
            .enumerate()
            .map(|(i, n)| dereth_ui::persist::CharacterIdentity {
                id: dereth_primitives::ObjectId(0x5000_0001 + u32::try_from(i).unwrap_or(0)),
                name: (*n).to_string(),
                seconds_grace_period: 0,
            })
            .collect(),
        num_allowed_characters: 11,
        account: "offline".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

pub(super) fn a_client_on_character_select() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::screen(mode::CHARACTER_MANAGEMENT, 4));
    {
        let host = c.app_mut().probe_mut().host_state_mut();
        host.character_set = Some(three_characters());
        host.received_set = true;
        host.world_name = Some("ACEmulator".into());
    }
    c.tick(4);
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the character list is the screen this scenario is about"
    );
    c
}

/// The credit roll, reached with **no** character set: a set arriving re-queues the character
/// list, and the roll would then be cut short by the fixture rather than by the key under test.
fn a_client_on_the_credits() -> HeadlessClient {
    let c = HeadlessClient::new(ClientSpec::screen(mode::CREDITS, 4));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(mode::CREDITS),
        "the credit roll is the screen this scenario is about"
    );
    c
}

fn with_credits<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui_screens::screens::credits::CreditsScreen) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<dereth_ui_screens::screens::credits::CreditsScreen>()
        .expect("the credits screen is current"))
}

pub(super) fn with_charmgmt_screen<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut CharacterManagementScreen) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<CharacterManagementScreen>()
        .expect("the character screen is current"))
}

// ---------------------------------------------------------------------------------------------
// keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once
// ---------------------------------------------------------------------------------------------

/// The map the pre-game screens listen on carries two keys, and no more.
///
/// It is asserted rather than assumed because it is the denominator of every scenario below: a
/// third key appearing there would make each of them stop being exhaustive.
pub(super) fn the_dialog_map_binds_only_escape_and_enter() {
    use std::collections::BTreeMap;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    // The shell, and with it the shipped keymap, without any screen on top of it.
    c.app_mut().start_shell().expect("the UI shell comes up");
    let shell = c.app_mut().input_manager_mut().expect("an input shell");
    let section = shell
        .manager
        .keymap
        .section(dereth_input::MAP_DIALOG_BOXES)
        .expect("the dialog section");
    let mut by_action: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, action) in section.bindings() {
        *by_action.entry(action.0).or_default() += 1;
    }
    let two = by_action.keys().copied().collect::<Vec<u32>>() == vec![ACCEPT_INPUT, ESCAPE_KEY];
    let each_once = by_action.values().all(|n| *n == 1);
    c.shutdown();

    // **The negative control**: in the world, where no screen before it has claimed this map,
    // neither of its two keys resolves in it -- they go to the in-game maps that used to swallow
    // them. Without this, "the pre-game screens win these keys" and "this map always wins them"
    // read alike.
    let mut w = HeadlessClient::new(ClientSpec::gameplay(4));
    let not_in_the_world = dialog_key_resolves_to(&mut w, ACCEPT_INPUT)
        .is_none_or(|(m, _)| m != InputMapId(9))
        && dialog_key_resolves_to(&mut w, ESCAPE_KEY).is_none_or(|(m, _)| m != InputMapId(9));
    // ...and they still reach something, rather than nowhere: a walk that answered nothing at all
    // would satisfy the reading above just as well.
    let they_still_arrive = dialog_key_resolves_to(&mut w, ACCEPT_INPUT).is_some()
        && dialog_key_resolves_to(&mut w, ESCAPE_KEY).is_some();

    w.assert_behaviour(
        "keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once",
        move |_| two && each_once && not_in_the_world && they_still_arrive,
    );
    w.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it
// ---------------------------------------------------------------------------------------------

/// On the opening sequence, enter moves it on and escape leaves it for the character list.
pub(super) fn enter_advances_the_opening_sequence_and_escape_leaves_it() {
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    let maps_registered = registered_mode_maps(&c) == dereth_client::ui::INTRO_INPUT_MAPS;
    // The dialog map outranks the in-game chat map that used to win this key, which is the whole
    // of why the screen never heard it.
    let enter_arrives = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(9), ActionId(ACCEPT_INPUT)));

    let opened_on_the_first = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[0]);
    let mut hands = Hands::new();

    // **The two roads are driven apart**, which is the whole design of this scenario: the opening
    // sequence is the one screen that answers a key twice, once as a bound action and once as a
    // typed character, and a scenario that only ever sent the real trio could not say which of
    // them did the work or notice one of them stopping.
    let (a0, ch0, _) = screen_stats(&c);
    let queued_before = intro_state(&mut c).1.len();
    key_only(&mut c, &mut hands, KeyCode::Enter);
    let (a1, ch1, _) = screen_stats(&c);
    let the_action_road = a1 == a0 + 1
        && ch1 == ch0
        && intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[1])
        && intro_state(&mut c).1.len() == queued_before - 1;

    hands.character(&mut c, '\r');
    c.tick(1);
    let (a2, ch2, _) = screen_stats(&c);
    let the_character_road = a2 == a1
        && ch2 == ch1 + 1
        && intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[2])
        && intro_state(&mut c).1.len() == queued_before - 2;

    let still_on_the_sequence = current_screen(&c) == Some(mode::INTRO);

    // Escape, on the other hand, leaves it -- and the screen going away takes both of its maps
    // with it, leaving only the character list's own.
    let escape_arrives =
        dialog_key_resolves_to(&mut c, ESCAPE_KEY) == Some((InputMapId(9), ActionId(ESCAPE_KEY)));
    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');
    let left_for_the_list = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && registered_mode_maps(&c) == character_screen_maps();
    // The second reading, and the only one that can see the screen's own tidying up: the other
    // map the sequence took has no second owner, so a screen that registered without
    // unregistering would leave that one behind even though its dialog map looked clean.
    let at_the_screens_priority: Vec<u32> = map_stack(&mut c)
        .iter()
        .filter(|e| e.priority == 3000)
        .map(|e| e.map.0)
        .collect();
    let nothing_left_behind = sorted(at_the_screens_priority) == sorted(character_screen_maps());

    c.assert_behaviour(
        "dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it",
        move |_| {
            maps_registered
                && enter_arrives
                && opened_on_the_first
                && the_action_road
                && the_character_road
                && still_on_the_sequence
                && escape_arrives
                && left_for_the_list
                && nothing_left_behind
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it
// ---------------------------------------------------------------------------------------------

/// One press of enter moves the sequence on by two pictures, and that is the client's own doing.
///
/// The opening screen subscribes to both roads a key takes, and both move the sequence on for
/// anything that is not the key that leaves. So a real press advances twice. It is said plainly
/// rather than smoothed over: a printable key, which no map here binds, takes the character road
/// alone and advances once -- without which "two roads" and "one road firing twice" read alike.
pub(super) fn a_real_press_of_enter_advances_twice() {
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    let mut hands = Hands::new();
    let opened_on_the_first = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[0]);

    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');
    let advanced_twice = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[2]);

    let before = intro_state(&mut c).0;
    key_trio(&mut c, &mut hands, KeyCode::KeyA, 'a');
    let a_printable_advances_once =
        intro_state(&mut c).0 != before && intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[3]);

    c.assert_behaviour(
        "dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it",
        move |_| opened_on_the_first && advanced_twice && a_printable_advances_once,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others
// ---------------------------------------------------------------------------------------------

/// The opening screen takes what its own maps carry, and refuses an action arriving on another.
pub(super) fn the_opening_screen_answers_only_the_keys_its_own_maps_carry() {
    let mut c = a_client_on_the_intro();
    let (before, _, _) = screen_stats(&c);

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');
    let one_press_one_action = screen_stats(&c).0 == before + 1;

    // The control: the same action delivered on a map the screen does not own. It is the map the
    // client keeps registered for the whole session, so it is an honest control rather than an
    // invented one -- and the screen must refuse it.
    let mid = screen_stats(&c).0;
    c.app_mut()
        .input_manager_mut()
        .expect("an input manager")
        .inject_action(dereth_input::InputEvent {
            action: ActionId(ESCAPE_KEY),
            input_map: InputMapId(0x1000_0009),
            toggle: dereth_input::ToggleType::OneShot,
            extent: 1.0,
            start: true,
            repeat_delta: 1,
            repeat_total: 0,
            from_key_down: false,
        });
    c.tick(1);
    let another_map_is_refused = screen_stats(&c).0 == mid;

    c.assert_behaviour(
        "dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others",
        move |_| one_press_one_action && another_map_is_refused,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once
// ---------------------------------------------------------------------------------------------

/// Enter ends the credit roll, escape ends it, and each does it exactly once.
pub(super) fn either_key_ends_the_credit_roll_and_ends_it_once() {
    // --- enter ---------------------------------------------------------------------------
    let mut c = a_client_on_the_credits();
    let its_own_map = registered_mode_maps(&c) == vec![dereth_input::MAP_DIALOG_BOXES.0];
    let enter_arrives = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(9), ActionId(ACCEPT_INPUT)));
    // The subject is alive enough to have failed: the roll is really running, on the whole
    // shipped text, and has not ended by itself.
    let running = with_credits(&mut c, |s| {
        !s.finished && s.wait_element.is_none() && s.line_count > 2000
    });
    let (intro_actions, _, switches) = screen_stats(&c);

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');

    // The whole chain runs inside one frame, so by the time this reads, the screen that answered
    // has already been thrown away: the end of the roll is the observable, and it is the one the
    // player sees.
    let enter_ended_it = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && screen_stats(&c).2 == switches + 1
        && screen_stats(&c).0 == intro_actions
        && registered_mode_maps(&c) == character_screen_maps();
    c.shutdown();

    // --- escape --------------------------------------------------------------------------
    let mut c = a_client_on_the_credits();
    let escape_arrives =
        dialog_key_resolves_to(&mut c, ESCAPE_KEY) == Some((InputMapId(9), ActionId(ESCAPE_KEY)));
    let still_running = with_credits(&mut c, |s| !s.finished);
    let (intro_actions, _, switches) = screen_stats(&c);

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');

    // One switch. Two deliveries of the same key would be two, which is the only observable here
    // that can count them.
    let escape_ended_it_once = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && screen_stats(&c).2 == switches + 1
        && screen_stats(&c).0 == intro_actions;

    c.assert_behaviour(
        "dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once",
        move |_| {
            its_own_map
                && enter_arrives
                && running
                && enter_ended_it
                && escape_arrives
                && still_running
                && escape_ended_it_once
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit
// ---------------------------------------------------------------------------------------------

/// On the character list, enter arrives and is declined; escape asks, and only once.
pub(super) fn enter_is_declined_on_the_character_list_and_escape_asks_once() {
    use dereth_ui::framework::Screen as _;

    let mut c = a_client_on_character_select();
    let its_own_map = registered_mode_maps(&c) == character_screen_maps();
    let enter_arrives = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(9), ActionId(ACCEPT_INPUT)));

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');
    // The key really arrives -- which it could not before -- and the screen is unmoved by it.
    // Asserting that something opened would be asserting a behaviour the client does not have.
    let declined = with_charmgmt_screen(&mut c, |s| s.open_dialog.is_none())
        && current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    let escape_arrives =
        dialog_key_resolves_to(&mut c, ESCAPE_KEY) == Some((InputMapId(9), ActionId(ESCAPE_KEY)));
    let roots_before = with_charmgmt_screen(&mut c, |s| s.roots().len());
    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');
    // The count of boxes is what can see a double delivery: the screen's one slot for which box
    // is open cannot.
    let asked_once = with_charmgmt_screen(&mut c, |s| s.open_dialog.is_some())
        && with_charmgmt_screen(&mut c, |s| s.roots().len()) == roots_before + 1;

    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');
    let and_no_second = with_charmgmt_screen(&mut c, |s| s.roots().len()) == roots_before + 1;

    c.assert_behaviour(
        "dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit",
        move |_| {
            its_own_map
                && enter_arrives
                && declined
                && escape_arrives
                && asked_once
                && and_no_second
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box
// ---------------------------------------------------------------------------------------------

/// The two shipped elements that name an input map of their own, and what focusing one does.
pub(super) fn a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys() {
    use dereth_primitives::AssetSource as _;
    use dereth_ui::props::attr;

    let mut c = a_client_on_character_select();

    // **The denominator first.** Whether focusing a box can register a map at all depends on
    // there being a box that names one, and the whole shipped corpus carries exactly two -- both
    // naming this same map, both leaves, neither the top of its own layout. The census is read
    // out of the data files' own directory rather than off a range written here, so a layout
    // outside any range would be counted rather than missed.
    let (layouts, elements, carriers, control) = {
        let store = c.dat_store().expect("a retail client has a store").clone();
        let master_id = dereth_primitives::DataId(0x3900_0001);
        let bytes = store.read(master_id).expect("the master property record");
        let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
            master_id, &bytes,
        )
        .expect("it decodes");
        let types = master.property_types();

        fn walk(
            e: &dereth_ui::desc::ElementDesc,
            path: String,
            top_level: bool,
            want: u32,
            hits: &mut Vec<(String, bool, bool, String)>,
        ) {
            let leaf = e.children.is_empty();
            let mut collections = vec![&e.base.properties];
            collections.extend(e.states.values().map(|s| &s.properties));
            for coll in collections {
                for (id, v) in &coll.0 {
                    if *id == want {
                        hits.push((path.clone(), top_level, leaf, format!("{v:?}")));
                    }
                }
            }
            for child in e.children.values() {
                walk(
                    child,
                    format!("{path}/{:#010X}", child.element_id.0),
                    false,
                    want,
                    hits,
                );
            }
        }

        let ids = store.ids_of(dereth_dat::DbType::UiLayout);
        let mut elements = 0_usize;
        let mut carriers = Vec::new();
        let mut control = Vec::new();
        for did in &ids {
            let raw = store
                .read(*did)
                .expect("a layout the directory lists reads");
            let l = dereth_ui::desc::LayoutDesc::read(*did, &raw, &types)
                .expect("every shipped layout decodes");
            elements += l.element_count();
            for e in l.elements.values() {
                let path = format!("{:#010X}/{:#010X}", did.0, e.element_id.0);
                walk(e, path.clone(), true, attr::INPUT_MAP, &mut carriers);
                walk(e, path, true, attr::INPUT_ACTION, &mut control);
            }
        }
        (ids.len(), elements, carriers, control)
    };
    // Without the denominators, "no element names a map" and "the walk visited nothing" are one
    // reading; and the control is the same walk pointed at a neighbouring attribute that is known
    // to be there in quantity, so a zero above would mean the walk is blind.
    let counted_the_corpus = layouts == 101 && elements == 2162 && control.len() == 41;
    let exactly_two = carriers.len() == 2
        && carriers
            .iter()
            .all(|(_, top, leaf, value)| value == "Enum(9)" && !*top && *leaf);

    // The box is reached the way a player reaches it: pick a character, press delete, and the
    // question that comes up is built out of the shared layout one of those two lives in.
    let mut hands = Hands::new();
    let row = with_charmgmt_screen(&mut c, |s| {
        s.rows
            .iter()
            .find(|r| r.name == "Marbo")
            .and_then(|r| r.element)
            .expect("Marbo's row")
    });
    hands.click_handle(&mut c, row);
    let delete = element(&c, ElementId(0x1000_039F));
    hands.click_handle(&mut c, delete);

    let dialog = with_charmgmt_screen(&mut c, |s| {
        s.dialog_element(dereth_ui_screens::screens::charmgmt::DialogContext::DeleteCharacter)
            .expect("deleting raises the question with a box to type in")
    });
    // The box and the question's own root share an id, and the recursive walk never answers the
    // element it started from, so this is the box.
    let typing_box = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(dialog, ElementId(0x2C))
        .expect("the box below the question's root");
    // Read off the live tree, so this and the census are readings of two different things.
    let it_names_the_map = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(typing_box)
        .expect("live")
        .input_map
        == Some(9);

    let before = dialog_map_entries(&mut c);
    let nothing_focused_yet = before == 1
        && !c
            .app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .focused_input_maps()
            .into_iter()
            .any(|(m, _)| m == 9);

    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .take_focus(typing_box);
    c.tick(1);

    let after = c
        .app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .focused_input_maps();
    let maps: Vec<u32> = after.iter().map(|(m, _)| *m).collect();
    let the_boxs_own_first = maps == vec![9, 0x0A, 1, 7, 8]
        && after.iter().find(|(m, _)| *m == 9).map(|(_, p)| *p) == Some(3000);
    // **Counted, not looked for**: the screen already has this map at this priority under a
    // different owner, so asking whether one is present could never have failed.
    let a_second_entry = dialog_map_entries(&mut c) == before + 1;

    // ...and it did not take the keys from the box. The box's own map carries both of these keys
    // too, and it goes in afterwards, so it walks first: enter in the box is still the box's.
    let the_box_keeps_its_keys = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(7), ActionId(ACCEPT_INPUT)))
        && dialog_key_resolves_to(&mut c, ESCAPE_KEY)
            == Some((InputMapId(7), ActionId(ESCAPE_KEY)));

    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .relinquish_focus(typing_box);
    c.tick(1);
    let taken_back = c
        .app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .focused_input_maps()
        .is_empty()
        && dialog_map_entries(&mut c) == before;

    c.assert_behaviour("dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box", move |_| {
        counted_the_corpus
            && exactly_two
            && it_names_the_map
            && nothing_focused_yet
            && the_boxs_own_first
            && a_second_entry
            && the_box_keeps_its_keys
            && taken_back
    });
    c.shutdown();
}

// =============================================================================================
// dialog-keys.* -- the three screens before the world, and which keys each of them answers
//
// Two rows. What else a pre-game screen does with a key is already booked on the
// `dialog-keys.credits.*`, `dialog-keys.character-select.*` and
// `dialog-keys.a-box-that-takes-typing-*` rows above; the table of which screen registers which
// map, as literals beside the client's own constants, would be a transcription and is not a row;
// and the premise of the last -- that both of these keys are one-shot in the shipped bindings -- is
// folded in below as the arm that says why the edge has to be made up.
// =============================================================================================

/// The two actions the pre-game screens listen for, and the one the camera map gives to a key
/// none of them own.
const PREGAME_ACCEPT: u32 = 0x25;
const PREGAME_ESCAPE: u32 = 0x27;
const CAMERA_TO_DEFAULT: u32 = 0x39;
const CAMERA_MAP: dereth_input::InputMapId = dereth_input::InputMapId(5);

/// How many times each pre-game screen has been asked about an action.
fn times_asked(c: &HeadlessClient) -> (u64, u64, u64) {
    let st = &c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    (st.intro_actions, st.credits_actions, st.charmgmt_actions)
}

/// One action arriving at the client on its way **back up** -- the edge a real finger cannot
/// produce for these keys, declared as made up because there is no producer of one here.
fn a_key_coming_back_up(c: &mut HeadlessClient, action: u32) {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is up")
        .inject_action(dereth_input::InputEvent {
            action: dereth_input::ActionId(action),
            input_map: dereth_input::MAP_DIALOG_BOXES,
            toggle: dereth_input::ToggleType::OneShot,
            extent: 0.0,
            start: false,
            repeat_delta: 0,
            repeat_total: 0,
            from_key_down: false,
        });
    c.tick(1);
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running
// ---------------------------------------------------------------------------------------------

/// Both directions in one scenario: the key is shown to have been delivered, so "the roll survived"
/// cannot be read off a keyboard that had stopped working, and the key the roll does own is
/// pressed on the same screen afterwards as the calibration.
pub(super) fn a_key_the_roll_does_not_own_leaves_it_running() {
    let mut c = a_client_on_the_credits();

    // The key is a real one and it is bound: the camera map gives it an action of its own, and
    // that map is registered for the whole run, the credits included.
    let to_default = dereth_testkit::input_steps::bound_scan_code(
        &mut c,
        dereth_input::ActionId(CAMERA_TO_DEFAULT),
        CAMERA_MAP,
    );
    let it_is_bound = to_default != 0;

    let broadcasts = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .key_presses_broadcast;
    let (_, asked_before, _) = times_asked(&c);

    dereth_testkit::input_steps::tap(&mut c, key(KeyCode::Numpad0));
    c.tick(1);

    let it_was_delivered = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .key_presses_broadcast
        > broadcasts;
    let the_roll_was_not_asked = times_asked(&c).1 == asked_before;
    let it_is_still_running =
        current_screen(&c) == Some(mode::CREDITS) && !with_credits(&mut c, |s| s.finished);

    // The calibration, on the same screen with the same hands: a key the roll does own ends it.
    dereth_testkit::input_steps::tap(&mut c, key(KeyCode::Escape));
    c.tick(2);
    let the_owned_key_still_ends_it = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running",
        move |_| {
            it_is_bound
                && it_was_delivered
                && the_roll_was_not_asked
                && it_is_still_running
                && the_owned_key_still_ends_it
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up
// ---------------------------------------------------------------------------------------------

/// Three screens through one seam, which is what makes this worth asserting: a client that hoisted
/// the opening sequence's own refusal out of its arm and applied it to all three would read
/// nothing-happened three times and pass every other scenario about these keys.
pub(super) fn only_the_opening_sequence_refuses_a_key_on_its_way_back_up() {
    // The premise, read out of the **shipped** bindings rather than written down: both keys these
    // screens listen for are one-shot, so the coming-up edge is never made for them and the made
    // up one below is the only way to see the difference at all.
    let mut c = a_client_on_the_intro();
    let both_are_one_shot = {
        let shell = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell is up");
        [PREGAME_ACCEPT, PREGAME_ESCAPE].into_iter().all(|a| {
            shell
                .manager
                .action_map
                .toggle_type(dereth_input::MAP_DIALOG_BOXES, dereth_input::ActionId(a))
                == dereth_input::ToggleType::OneShot
        })
    } && !dereth_input::ToggleType::OneShot.is_hold();

    // The opening sequence refuses it...
    a_key_coming_back_up(&mut c, PREGAME_ESCAPE);
    let the_sequence_refused_it = times_asked(&c).0 == 0;
    // ...and the same screen answers the press, which is what makes that nothing a reading.
    dereth_testkit::input_steps::tap(&mut c, key(KeyCode::Enter));
    c.tick(1);
    let but_it_answers_a_press = times_asked(&c).0 >= 1;
    c.shutdown();

    // The credit roll answers it, and a coming-up edge alone ends the roll.
    let mut c = a_client_on_the_credits();
    a_key_coming_back_up(&mut c, PREGAME_ESCAPE);
    c.tick(1);
    let the_roll_answered_it =
        times_asked(&c).1 == 1 && current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // So does the character list, and a coming-up edge alone raises the question about leaving.
    let mut c = a_client_on_character_select();
    a_key_coming_back_up(&mut c, PREGAME_ESCAPE);
    c.tick(1);
    let the_list_answered_it =
        times_asked(&c).2 == 1 && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_some();

    c.assert_behaviour(
        "dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up",
        move |_| {
            both_are_one_shot
                && the_sequence_refused_it
                && but_it_answers_a_press
                && the_roll_answered_it
                && the_list_answered_it
        },
    );
    c.shutdown();
}
