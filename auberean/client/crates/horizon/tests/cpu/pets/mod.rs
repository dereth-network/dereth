//! The pets beside the screen and the Dismount button: shown as the view and the options say,
//! the pets selected, renamed, their modes turned and asked again, the commands, an attack shown
//! a while; the player's mount.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{Notice, PetNotice, Said};
use auberean_client_sdk::contract::pet::mode;
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, GameOptions, View};
use auberean_client_sdk::contract::{Action, PetAction};
use auberean_horizon::state::pets::HIDE_PETS;
use auberean_horizon::state::{Frame, GameState};
use auberean_horizon::ui::pets::{dismount_rect, PetsPanel, ASK_AGAIN, ATTACKED_FOR};

use crate::{centre, Harness, PLAYER};

const WOLF: u64 = 0x2140_0000_0000_0011;
const BEAR: u64 = 0x2140_0000_0000_0012;

fn pet(id: u64, name: &str, mode: u32) -> v::Pet {
    v::Pet {
        id,
        name: Some(TextRef::literal(name)),
        icon: 0,
        mode,
    }
}

fn the_view(pets: Vec<v::Pet>, bits: u32, mounted: bool) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            mounted,
            ..v::Player::default()
        }),
        options: Some(GameOptions {
            bits,
            ..GameOptions::default()
        }),
        pets,
        ..View::default()
    }
}

fn written(t: &TextRef) -> Option<String> {
    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
}

fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &written,
        unknown_command: "",
    });
}

fn with_pets() -> (View, GameState) {
    let v = the_view(
        vec![
            pet(WOLF, "Wolf (Rowan)", mode::AGGRESSIVE),
            pet(BEAR, "Bear", mode::IDLE),
        ],
        0,
        false,
    );
    let mut s = GameState::default();
    follow(&mut s, &v, &[]);
    (v, s)
}

#[test]
fn the_pets_follow_the_view_named_without_their_owner_and_hidden_by_the_options() {
    let (_, s) = with_pets();
    assert!(s.pets.shown());
    let names: Vec<&str> = s.pets.pets.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Wolf ", "Bear"]);
    let mut hidden = GameState::default();
    follow(
        &mut hidden,
        &the_view(vec![pet(WOLF, "Wolf", 0x20)], HIDE_PETS, false),
        &[],
    );
    assert!(!hidden.pets.shown());
    assert!(PetsPanel::layout(&hidden, (1920.0, 1080.0), 1.0).is_none());
    let mut none = GameState::default();
    follow(&mut none, &the_view(Vec::new(), 0, false), &[]);
    assert!(!none.pets.shown(), "no pets, no panel");
}

#[test]
fn a_pets_row_pressed_selects_it_and_the_commands_command_the_pets() {
    let mut h = Harness::new();
    let (_, s) = with_pets();
    h.frame(&s);
    let k = h.k();
    let l = PetsPanel::layout(&s, h.screen, k).expect("shown");
    let row = l.row(1, k);
    let out = h.click(&s, (row.x + 50.0 * k, row.y + 8.0 * k));
    assert_eq!(out.acts, [Action::SetSelection { target: BEAR }]);
    let out = h.click(&s, centre(l.commands[1]));
    assert_eq!(out.acts, [Action::Pet(PetAction::Return)]);
    let out = h.click(&s, centre(l.commands[2]));
    assert_eq!(out.acts, [Action::Pet(PetAction::Buff)]);
    let out = h.click(&s, centre(l.commands[3]));
    assert_eq!(out.acts, [Action::Pet(PetAction::Heal)]);
    let out = h.click(&s, centre(l.commands[0]));
    assert_eq!(
        out.acts,
        [Action::Pet(PetAction::Attack(0))],
        "nothing selected"
    );
}

#[test]
fn the_mode_arrows_ask_at_once_from_the_mode_asked_and_again_until_the_pet_takes_it() {
    let mut h = Harness::new();
    let (v, mut s) = with_pets();
    h.frame(&s);
    let k = h.k();
    let l = PetsPanel::layout(&s, h.screen, k).expect("shown");
    let (back, on) = l.arrows(0, k);
    let out = h.click(&s, centre(on));
    assert_eq!(
        out.acts,
        [Action::Pet(PetAction::Mode {
            pet: WOLF,
            mode: mode::OPPORTUNISTIC
        })]
    );
    let out = h.click(&s, centre(on));
    assert_eq!(
        out.acts,
        [Action::Pet(PetAction::Mode {
            pet: WOLF,
            mode: mode::COURAGEOUS
        })],
        "on from the mode asked"
    );
    assert_eq!(
        h.ui.pets.mode_shown(WOLF, mode::AGGRESSIVE),
        mode::COURAGEOUS
    );
    let out = h.click(&s, centre(back));
    assert_eq!(
        out.acts,
        [Action::Pet(PetAction::Mode {
            pet: WOLF,
            mode: mode::OPPORTUNISTIC
        })]
    );
    // Frames until the pet should have answered: asked again, once.
    let mut again = Vec::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(ASK_AGAIN * 60.0) as usize + 2 {
        again.extend(h.frame(&s).acts);
    }
    assert_eq!(
        again,
        [Action::Pet(PetAction::Mode {
            pet: WOLF,
            mode: mode::OPPORTUNISTIC
        })]
    );
    // It took it: no more asking.
    let mut took = v.clone();
    took.pets[0].mode = mode::OPPORTUNISTIC;
    follow(&mut s, &took, &[]);
    let mut more = Vec::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(ASK_AGAIN * 60.0) as usize + 2 {
        more.extend(h.frame(&s).acts);
    }
    assert!(more.is_empty(), "{more:?}");
}

#[test]
fn a_pet_attacked_shows_so_for_a_while() {
    let mut h = Harness::new();
    let (v, mut s) = with_pets();
    follow(
        &mut s,
        &v,
        &[Notice {
            about: None,
            said: Said::Pet(PetNotice::Attacked { pet: BEAR }),
        }],
    );
    h.frame(&s);
    follow(&mut s, &v, &[]);
    let now = |h: &Harness| h.ui.now();
    assert!(h.ui.pets.attacked(BEAR, now(&h)));
    assert!(!h.ui.pets.attacked(WOLF, now(&h)));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(ATTACKED_FOR * 60.0) as usize + 2 {
        h.frame(&s);
    }
    assert!(!h.ui.pets.attacked(BEAR, now(&h)), "a while later");
}

#[test]
fn a_pet_given_another_name_asks_for_it_unless_only_its_letters_case_differs() {
    let mut h = Harness::new();
    let (_, s) = with_pets();
    h.frame(&s);
    let k = h.k();
    let l = PetsPanel::layout(&s, h.screen, k).expect("shown");
    let row = l.row(1, k);
    let out = h.right_click(&s, (row.x + 50.0 * k, row.y + 8.0 * k));
    assert_eq!(out.acts, [Action::SetSelection { target: BEAR }]);
    assert!(h.ui.typing(), "the name box has the keys");
    for _ in 0..4 {
        h.key(&s, auberean_horizon::ui::vk::BACK);
    }
    h.type_text(&s, "Bruin");
    let out = h.key(&s, auberean_horizon::ui::vk::ENTER);
    assert_eq!(
        out.acts,
        [Action::Pet(PetAction::Name {
            pet: BEAR,
            name: "Bruin".to_owned()
        })]
    );
    assert!(!h.ui.typing());
    h.right_click(&s, (row.x + 50.0 * k, row.y + 8.0 * k));
    for _ in 0..4 {
        h.key(&s, auberean_horizon::ui::vk::BACK);
    }
    h.type_text(&s, "BEAR");
    let out = h.key(&s, auberean_horizon::ui::vk::ENTER);
    assert!(out.acts.is_empty(), "the same name: {out:?}");
}

#[test]
fn the_dismount_button_shows_while_mounted_and_gets_the_player_down() {
    let mut h = Harness::new();
    let mut s = GameState::default();
    follow(&mut s, &the_view(Vec::new(), 0, true), &[]);
    assert!(s.pets.mounted);
    h.frame(&s);
    let r = dismount_rect(h.screen, h.k());
    let out = h.click(&s, centre(r));
    assert_eq!(out.acts, [Action::Dismount]);
    follow(&mut s, &the_view(Vec::new(), 0, false), &[]);
    let out = h.click(&s, centre(r));
    assert!(!out.acts.contains(&Action::Dismount), "on foot");
}

#[test]
fn p_and_shift_p_pick_the_next_and_the_previous_of_the_players_pets() {
    use auberean_client_sdk::contract::control::interface::select as selection;
    use auberean_shell::interface_keys::HORIZON_KEYS;
    let p = auberean_shell::input::Key::P.input_word();
    assert!(HORIZON_KEYS.contains(&(p, 0, selection::NEXT_PET)));
    assert!(HORIZON_KEYS.contains(&(p, 1, selection::PREVIOUS_PET)));
}
