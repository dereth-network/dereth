//! What the classic interface writes from the program and the session: it tells the player why the
//! session ended in sentences, never in the string ids the reason travels as (its era's own words
//! where it had them, the later client's sentence for a network refusal, and a boot's or a ban's
//! own sentence as it came), and it names the running program's own version.

use super::*;
use crate::{app::CoreApp, platform::host::NullHost};
use dereth_client_contract::pregame::DisconnectNotice;

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
}

impl Client {
    /// The client over a world drawn from the files before Throne of Destiny, with the classic
    /// interface up and no server.
    fn new(state: &std::path::Path) -> Self {
        let cfg = dereth_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::both_sets_dir(),
            world_base: Some(dereth_primitives::ContainerEra::Classic),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
        )
        .expect("both dat sets and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        struct Fonts;
        impl dereth_classic_dat::fonts::FontSource for Fonts {
            fn rasterize(
                &self,
                _: &dereth_classic_dat::fonts::FontSpec,
            ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
                Ok(Default::default())
            }
        }
        let portal = dereth_classic_dat::ClassicPortal::of_store(&app.store)
            .expect("the world's own portal is the classic one");
        let art = std::sync::Arc::new(
            dereth_classic_ui::art::ClassicArt::new(portal, &Fonts).expect("the art"),
        );
        let mut ui = dereth_classic_ui::runtime::ClassicUi::new(
            dereth_classic_ui::resources::Resources::new(
                art,
                Err("World creation tables unavailable".into()),
                None,
            ),
            dereth_classic_ui::art::ClassicPaths {
                state: state.join("classic"),
            },
            dereth_classic_ui::panels::factory,
            (800, 600),
        );
        ui.start(&mut app.ui_context())
            .expect("the classic interface starts");
        shell.classic.ui = Some(ui);
        let mut c = Self { app, shell };
        // Shown: the interface the player chose.
        use dereth_client_contract::options::interface::{Interface, INTERFACE};
        dereth_client_contract::options::store::set_value(
            INTERFACE,
            dereth_client_contract::PrefValue::Int(Interface::Classic.value()),
        );
        assert!(c.app.frame(&mut c.shell));
        assert!(c.app.hud.classic_active, "the classic interface is shown");
        c
    }

    /// The session ends for `notice`, carrying `error` as the runtime hands it on, and a frame
    /// passes. Returns the message box's text and every text the screen draws.
    fn ended(&mut self, error: &str, notice: DisconnectNotice) -> (Option<String>, Vec<String>) {
        self.app.host_state.error = Some(error.to_owned());
        self.app.host_state.disconnect = Some(notice);
        assert!(self.app.frame(&mut self.shell));
        let ui = self
            .shell
            .classic
            .ui
            .as_mut()
            .expect("the classic interface");
        let drawn = ui
            .desktop
            .screen()
            .commands
            .into_iter()
            .filter_map(|c| match c {
                dereth_classic_ui::Command::TextBox { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        (ui.classic.disconnect_message.clone(), drawn)
    }
}

fn client(name: &str) -> (dereth_dat::testing::ScratchDir, Client) {
    let state = dereth_dat::testing::ScratchDir::new(name).expect("a scratch directory");
    let c = Client::new(state.path());
    (state, c)
}

/// Behaviour: login.classic.the-reason-a-session-ended-reads-as-a-sentence
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_lost_connection_reads_the_classic_sentence_and_never_its_id() {
    let (_state, mut c) = client("classic-disconnect-lost");
    let (message, drawn) = c.ended(
        dereth_client_contract::disconnect::SERVER_DIED_STRING_ID,
        DisconnectNotice::ServerDied,
    );
    assert_eq!(message.as_deref(), Some("\n\n\nServer connection lost"));
    assert!(
        drawn.iter().any(|t| t.contains("Server connection lost")),
        "the screen shows the sentence: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|t| t.contains("ID_")),
        "no string id is drawn: {drawn:?}"
    );
}

/// Behaviour: login.classic.the-reason-a-session-ended-reads-as-a-sentence
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_character_error_reads_its_own_sentence_even_where_two_codes_share_an_id() {
    let (_state, mut c) = client("classic-disconnect-character");
    let (message, _) = c.ended(
        "ID_CHAR_ERROR_SERVER_CRASH",
        DisconnectNotice::CharacterError(8),
    );
    assert_eq!(
        message.as_deref(),
        Some("\n\nThe account you specified is already in use.")
    );
    let (_state, mut c) = client("classic-disconnect-character-4");
    let (message, _) = c.ended(
        "ID_CHAR_ERROR_SERVER_CRASH",
        DisconnectNotice::CharacterError(4),
    );
    assert_eq!(
        message.as_deref(),
        Some("\nThe server has disconnected. Please try again in a few minutes.")
    );
}

/// Behaviour: login.classic.the-reason-a-session-ended-reads-as-a-sentence
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_network_refusal_reads_the_later_clients_sentence_and_a_boot_its_own() {
    let (_state, mut c) = client("classic-disconnect-net");
    let (message, drawn) = c.ended(
        "ID_ConnectionError_ServerFull",
        DisconnectNotice::Net("ID_ConnectionError_ServerFull".into()),
    );
    assert_eq!(message.as_deref(), Some("Server Full"));
    assert!(!drawn.iter().any(|t| t.contains("ID_")), "{drawn:?}");

    let (_state, mut c) = client("classic-disconnect-booted");
    let text = "You have been booted from Asheron's Call for Code of Conduct Violations.";
    let (message, _) = c.ended(text, DisconnectNotice::Booted(None));
    assert_eq!(message.as_deref(), Some(text));
}

/// Behaviour: options.classic.the-options-window-names-the-running-programs-version
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn the_classic_interface_names_the_running_programs_version() {
    let (_state, mut c) = client("classic-version");
    // The program names itself at start-up; this library's own package version is not it.
    c.app.interaction.client_build_id = "dereth-client 9.8.7";
    assert!(c.app.frame(&mut c.shell));
    let ui = c.shell.classic.ui.as_ref().expect("the classic interface");
    assert_eq!(ui.classic.client_version, "9.8.7");
    // And still after a character session ends.
    c.app.interaction.on_end_character_session();
    assert!(c.app.frame(&mut c.shell));
    let ui = c.shell.classic.ui.as_ref().expect("the classic interface");
    assert_eq!(ui.classic.client_version, "9.8.7");
}
