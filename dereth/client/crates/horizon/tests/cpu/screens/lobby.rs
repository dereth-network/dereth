//! Character select logs a character in, deletes or restores one only after the player says yes,
//! and the scripted log-in takes the character `-u` names.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::pregame::CharacterAction;
use dereth_horizon::options::{HorizonOptions, StartScreen};
use dereth_horizon::ui::game::{CharacterEntry, GameState};
use dereth_horizon::ui::input::vk;
use dereth_horizon::ui::pregame::LobbyLayout;
use dereth_primitives::ObjectId;

use crate::Harness;

fn account() -> GameState {
    GameState {
        connected: true,
        connect_phase: dereth_horizon::ui::game::ConnectPhase::Ready,
        host: "127.0.0.1:9000".into(),
        world: Some("Test".into()),
        characters: vec![
            CharacterEntry {
                id: ObjectId(0x5000_0001),
                name: "First".into(),
                delete_seconds: 0,
            },
            CharacterEntry {
                id: ObjectId(0x5000_0002),
                name: "Second".into(),
                delete_seconds: 0,
            },
        ],
        ..GameState::default()
    }
}

fn log_ons(actions: &[CharacterAction]) -> Vec<ObjectId> {
    actions
        .iter()
        .filter_map(|a| match a {
            CharacterAction::LogOn(id) => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn enter_asks_to_log_in_and_only_a_second_enter_sends_the_log_on() {
    let state = account();
    let mut h = Harness::new(HorizonOptions {
        screen: StartScreen::Lobby,
        ..HorizonOptions::default()
    });
    h.frame(&state);
    h.input.keys.push(vk::DOWN);
    h.frame(&state);
    h.input.keys.push(vk::ENTER);
    let asked = h.frame(&state);
    assert!(asked.character_actions.is_empty());
    h.input.keys.push(vk::ENTER);
    let confirmed = h.frame(&state);
    assert_eq!(
        log_ons(&confirmed.character_actions),
        vec![ObjectId(0x5000_0002)]
    );
}

#[test]
fn nothing_logs_in_while_the_server_is_updating_the_data_files() {
    let mut state = account();
    state.connect_phase = dereth_horizon::ui::game::ConnectPhase::Updating;
    let mut h = Harness::new(HorizonOptions {
        screen: StartScreen::Lobby,
        ..HorizonOptions::default()
    });
    h.frame(&state);
    for _ in 0..2 {
        h.input.keys.push(vk::ENTER);
        assert!(h.frame(&state).character_actions.is_empty());
    }
    // Once it has finished, Enter asks and a second Enter logs in.
    state.connect_phase = dereth_horizon::ui::game::ConnectPhase::Ready;
    h.input.keys.push(vk::ENTER);
    h.frame(&state);
    h.input.keys.push(vk::ENTER);
    assert_eq!(log_ons(&h.frame(&state).character_actions).len(), 1);
}

#[test]
fn escape_on_the_confirmation_logs_nothing_in() {
    let state = account();
    let mut h = Harness::new(HorizonOptions {
        screen: StartScreen::Lobby,
        ..HorizonOptions::default()
    });
    h.frame(&state);
    h.input.keys.push(vk::ENTER);
    h.frame(&state);
    h.input.keys.push(vk::ESCAPE);
    let out = h.frame(&state);
    assert!(out.character_actions.is_empty());
    for _ in 0..30 {
        assert!(h.frame(&state).character_actions.is_empty());
    }
}

#[test]
fn the_scripted_log_in_goes_from_character_select_to_the_named_character() {
    let state = account();
    let mut h = Harness::new(HorizonOptions {
        auto_login: true,
        start_character: Some("second".into()),
        ..HorizonOptions::default()
    });
    let mut sent = Vec::new();
    for _ in 0..600 {
        sent.extend(log_ons(&h.frame(&state).character_actions));
    }
    assert_eq!(sent, vec![ObjectId(0x5000_0002)]);
}

/// A click at `at`, pressed and let go: everything both frames asked for.
fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) -> dereth_horizon::ui::Outcome {
    h.move_to(at.0, at.1);
    h.press();
    let mut out = h.frame(state);
    h.release();
    let up = h.frame(state);
    out.character_actions.extend(up.character_actions);
    out.quit |= up.quit;
    out
}

fn layout() -> LobbyLayout {
    LobbyLayout::new((1920.0, 1080.0), 1.0)
}

fn centre(r: dereth_horizon::draw::Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn delete_asks_first_and_a_yes_deletes_the_selected_character() {
    let state = account();
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&state);
    h.input.keys.push(vk::DOWN);
    h.frame(&state);
    let asked = click(&mut h, &state, centre(layout().delete));
    assert!(asked.character_actions.is_empty(), "nothing before the yes");
    h.input.keys.push(vk::ENTER);
    let out = h.frame(&state);
    assert_eq!(
        out.character_actions,
        vec![CharacterAction::Delete(ObjectId(0x5000_0002))]
    );
}

#[test]
fn a_character_being_deleted_is_restored_with_the_same_button() {
    let mut state = account();
    state.characters[0].delete_seconds = 600;
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&state);
    click(&mut h, &state, centre(layout().delete));
    h.input.keys.push(vk::ENTER);
    let out = h.frame(&state);
    assert_eq!(
        out.character_actions,
        vec![CharacterAction::Restore(ObjectId(0x5000_0001))]
    );
    // And it cannot be logged in with while it is being deleted.
    let log_in = click(&mut h, &state, centre(layout().log_in));
    h.input.keys.push(vk::ENTER);
    let mut actions = log_in.character_actions;
    actions.extend(h.frame(&state).character_actions);
    assert!(log_ons(&actions).is_empty(), "{actions:?}");
}

#[test]
fn exit_on_character_select_quits() {
    let state = account();
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&state);
    assert!(click(&mut h, &state, centre(layout().exit)).quit);
}

#[test]
fn the_selected_character_stands_between_the_news_and_the_list_only_when_its_look_is_known() {
    let mut state = account();
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&state);
    assert_eq!(h.ui.pregame.doll, None, "no look is remembered");
    state.known_looks = vec![ObjectId(0x5000_0001)];
    h.frame(&state);
    let (at, _) =
        h.ui.pregame
            .doll
            .expect("the first character's look is known");
    let l = layout();
    assert!(at.x >= l.news.right() && at.right() <= l.list.x, "{at:?}");
    h.input.keys.push(vk::DOWN);
    h.frame(&state);
    assert_eq!(h.ui.pregame.doll, None, "the second's is not");
}
