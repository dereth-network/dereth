//! Horizon's own screens before the world, against the server double: it logs in itself with the
//! account it was started with, and its character select enters the world as the character
//! chosen.

use auberean_client_sdk::protocol::ClientMessage;
use auberean_horizon::ui::pregame::Screen;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::{Choice, Phase};

use crate::session::{Session, NAME, WORLD_NAME};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// The `PlayCharacter` messages the client sent so far.
fn entries(s: &Session) -> usize {
    s.shell.client.as_ref().map_or(0, |c| {
        c.sent()
            .iter()
            .filter(|m| matches!(m.message, ClientMessage::PlayCharacter(_)))
            .count()
    })
}

/// The screen Horizon shows before the world.
fn screen(s: &Session) -> Option<Screen> {
    s.horizon().borrow().screen()
}

/// A client whose preferences name Horizon starts in it and logs in itself with the account it was
/// given, its own connecting screen up until the list comes; started with a character to play, it
/// enters the world as that character, and Horizon shows its HUD there.
///
/// Behaviour: desktop.horizon.before-the-world-its-own-screens-log-in-and-enter
#[test]
fn horizon_logs_in_itself_and_enters_the_world_as_the_character_it_was_started_with() {
    let mut s = Session::new(Some(HORIZON));
    assert_eq!(s.shell.interface(), Choice::Horizon);
    s.step();
    assert_eq!(screen(&s), Some(Screen::Connecting));
    assert!(s.shell.client.is_some(), "Horizon asked for the login");
    let entered = s.until(15.0, |s| s.shell.phase() == Phase::World);
    assert!(entered, "in the world: {:?}", s.shell.phase());
    let (items, _) = s.step();
    assert_eq!(screen(&s), None);
    assert!(items > 0, "Horizon's HUD drawn");
    assert!(s.horizon().borrow().state().in_world);
    assert_eq!(entries(&s), 1, "one entry asked");
}

/// Started with no character to play, Horizon's character select lists the account's characters
/// with the selected one standing in its own scene; Enter asks whether to enter the world as it,
/// and the question's Enter does, once.
///
/// Behaviour: desktop.horizon.character-select-lists-the-characters-and-enters-the-one-chosen
#[test]
fn horizon_s_character_select_lists_the_characters_and_enters_the_one_chosen() {
    let mut s = Session::choosing(Some(HORIZON));
    let listed = s.until(10.0, |s| screen(s) == Some(Screen::Select));
    assert!(listed, "character select up: {:?}", screen(&s));
    let names: Vec<String> = s
        .horizon()
        .borrow()
        .pregame()
        .characters
        .iter()
        .flatten()
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(names, vec![NAME.to_owned()]);
    let drawn = s.frame();
    assert_eq!(drawn.scenes.len(), 1, "the selected character's figure");
    assert!(!drawn.scenes[0].figures.is_empty());
    assert_eq!(entries(&s), 0, "nothing entered by itself");
    for down in [true, false] {
        s.shell.event(&Event::Key {
            key: Key::Enter,
            down,
            repeat: false,
        });
        s.step();
    }
    let yes = s
        .horizon()
        .borrow()
        .ui()
        .pregame
        .confirm_buttons
        .first()
        .copied();
    let yes = yes.expect("the question asked");
    assert_eq!(entries(&s), 0, "asked first");
    s.shell.event(&Event::Pointer {
        x: yes.x + yes.w / 2.0,
        y: yes.y + yes.h / 2.0,
    });
    s.step();
    for down in [true, false] {
        s.shell.event(&Event::Button {
            button: Button::Left,
            down,
        });
        s.step();
    }
    let entered = s.until(15.0, |s| s.shell.phase() == Phase::World);
    assert!(entered, "in the world: {:?}", s.shell.phase());
    assert_eq!(entries(&s), 1);
}

/// Horizon's character select stands over Cragstone, drawn from the install with no help from the
/// server, the camera turning as the view's path says at the frame's time; the world's name is the
/// one the login gave. Once the player asks to enter the world the view is let go of: nothing of
/// the world is drawn until the player is in it.
///
/// Behaviour: desktop.horizon.character-select-stands-over-a-view-of-cragstone-and-names-the-world
#[test]
fn horizon_s_character_select_stands_over_cragstone_and_names_the_world() {
    use auberean_horizon::vista;
    let mut s = Session::choosing(Some(HORIZON));
    let folder = crate::session::Folder::default();
    folder.0.lock().expect("the folder").insert(
        auberean_horizon::own::FILE.to_owned(),
        b"scale auto\nvista cragstone\n".to_vec(),
    );
    s.shell.host.folder = Some(folder);
    let listed = s.until(10.0, |s| screen(s) == Some(Screen::Select));
    assert!(listed, "character select up: {:?}", screen(&s));
    s.step();
    assert_eq!(s.horizon().borrow().pregame().world, WORLD_NAME);
    for _ in 0..30 {
        assert!(s.world_drawn(), "the view drawn behind character select");
        let seen = s.shell.scene.seen.camera.expect("a camera seen");
        let asked = vista::camera(s.shell.host.time);
        assert_eq!(s.shell.scene.seen.block, vista::BLOCK);
        assert_eq!(seen.position, vista::AT);
        assert!((seen.heading - asked.heading).abs() < 1e-4, "{seen:?}");
        assert!((seen.pitch - vista::PITCH).abs() < 1e-4);
    }
    for down in [true, false] {
        s.shell.event(&Event::Key {
            key: Key::Enter,
            down,
            repeat: false,
        });
        s.step();
    }
    let yes = s
        .horizon()
        .borrow()
        .ui()
        .pregame
        .confirm_buttons
        .first()
        .copied()
        .expect("the question asked");
    s.shell.event(&Event::Pointer {
        x: yes.x + yes.w / 2.0,
        y: yes.y + yes.h / 2.0,
    });
    s.step();
    // The press answers the question.
    s.shell.event(&Event::Button {
        button: Button::Left,
        down: true,
    });
    assert!(!s.world_drawn(), "let go of once the entry is asked");
    assert_eq!(entries(&s), 1);
    s.shell.event(&Event::Button {
        button: Button::Left,
        down: false,
    });
    while s.shell.phase() != Phase::World && s.shell.host.time < 30.0 {
        let drawn = s.world_drawn();
        assert!(
            !drawn || s.shell.phase() == Phase::World,
            "nothing drawn on the way in"
        );
    }
    assert_eq!(s.shell.phase(), Phase::World);
}
