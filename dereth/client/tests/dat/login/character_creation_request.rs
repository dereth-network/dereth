//! What the character-creation request carries and how a scripted run gets one sent: the slot is
//! the selected character's index in the character set as the server sent it, and the scripted
//! creation (`-create <name> --enter-world`) reaches the summary page before it names the character
//! and presses Finish, so the request is actually made.
//! Fixture: the retail dats' screens and character-generation tables; a headless `App` with its UI
//! up and no network, on a character screen holding a written character set.

use crate::common::client_dir;

use dereth_chargen::CgVerification;
use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::present::NullPresentation;
use dereth_primitives::ObjectId;
use dereth_ui::framework::mode;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::chargen::CharGenScreen;
use dereth_ui_screens::screens::charmgmt::CharacterManagementScreen;

/// Three characters in the server's order, which is not the list's alphabetical one: `+Alba`
/// is the server's third and the list's first.
const NAMES: [&str; 3] = ["+Aldis", "+Aldwyne", "+Alba"];

fn character_set() -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| dereth_ui::persist::CharacterIdentity {
                id: ObjectId(0x5000_0001 + u32::try_from(i).unwrap_or(0)),
                name: (*n).to_string(),
                seconds_grace_period: 0,
            })
            .collect(),
        num_allowed_characters: 11,
        account: "ac01".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

/// An offline headless `App` on the character screen. `create` is the `-create` switch.
fn app_on_character_screen(create: &str) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        world: false,
        character: false,
        create_char: create.to_owned(),
        enter_world: !create.is_empty(),
        preferences_file: std::env::temp_dir()
            .join("dereth-creation-request-not-created/prefs.ini"),
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("the headless app starts");
    app.start_shell().expect("the shell comes up");
    let host = app.probe_mut().host_state_mut();
    host.character_set = Some(character_set());
    host.received_set = true;
    host.world_name = Some("ACEmulator".into());
    // The data-patch screen, the intro (skipped, as the player skips it) and the character screen.
    for _ in 0..12 {
        let m = app.ui().and_then(|u| u.flow.current_mode());
        if m == Some(mode::CHARACTER_MANAGEMENT) {
            break;
        }
        if m == Some(mode::INTRO) {
            let root = app
                .ui()
                .and_then(|u| u.flow.current())
                .and_then(|s| s.roots().first().copied());
            if let (Some(root), Some(shell)) = (root, app.ui_mut()) {
                shell.ui.broadcast_element_message(
                    root,
                    dereth_ui_screens::screens::intro::MSG_SKIP,
                    0,
                    0,
                );
            }
        }
        app.frame();
    }
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the flow did not reach the character screen"
    );
    app
}

fn current<T: 'static>(app: &mut App) -> Option<&mut T> {
    let shell = app.ui_mut()?;
    let s = shell.flow.current_mut()?;
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<T>()
}

fn click(app: &mut App, id: u32) {
    let shell = app.ui_mut().expect("the shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("element {id:#010X} is not on the screen"));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

/// Behaviour: chargen.finish.the-request-carries-the-selected-characters-slot-in-server-order
/// With `+Alba` selected (the list's first row, the server's third character), the wizard that
/// Create Character opens builds a request whose slot is 2: the character's index in the set as
/// the server sent it.
///
/// Falsified by a slot nothing writes (always 0) and by the row's display index (0).
#[test]
fn the_creation_request_carries_the_selected_characters_index_in_the_servers_order() {
    let mut app = app_on_character_screen("");
    for _ in 0..2 {
        app.frame();
    }
    let alba = ObjectId(0x5000_0003);
    let row = {
        let s = current::<CharacterManagementScreen>(&mut app).expect("the character screen");
        let row = s
            .rows
            .iter()
            .find(|r| r.id == alba)
            .and_then(|r| r.element)
            .expect("+Alba has a row");
        assert_eq!(
            s.rows.first().map(|r| r.id),
            Some(alba),
            "+Alba is the list's first row"
        );
        row
    };
    app.ui_mut()
        .expect("the shell")
        .ui
        .broadcast_element_message(row, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    app.frame();
    assert_eq!(
        current::<CharacterManagementScreen>(&mut app)
            .expect("the character screen")
            .selected_id,
        alba,
        "the click selected +Alba"
    );

    // Create Character.
    click(&mut app, 0x1000_03A0);
    for _ in 0..3 {
        app.frame();
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::CHAR_GEN) {
            break;
        }
    }
    app.frame();
    let w = current::<CharGenScreen>(&mut app).expect("the wizard");
    assert_eq!(
        w.state.get_char_gen_result().slot,
        2,
        "+Alba is the server's third character"
    );
}

/// Behaviour: chargen.finish.a-scripted-creation-names-the-character-on-the-summary-page
/// `-create Seamtest --enter-world` on a character screen: the scripted drive opens the wizard,
/// picks the first heritage and town, goes to the summary page, names the character there and
/// presses Finish, so the creation request is made (the wizard waits for the server's answer
/// with the name it typed).
///
/// Falsified by a drive that presses Finish before the summary page is up: Finish is ignored on
/// every other page, and the wizard would never ask.
#[test]
fn a_scripted_creation_reaches_the_summary_page_and_sends_the_request() {
    let mut app = app_on_character_screen("Seamtest");
    let mut sent = false;
    for _ in 0..40 {
        app.frame();
        if let Some(w) = current::<CharGenScreen>(&mut app) {
            if w.state.verification == CgVerification::Pending {
                assert_eq!(w.state.name, "Seamtest");
                sent = true;
                break;
            }
        }
    }
    assert!(sent, "the scripted creation never made its request");
}
