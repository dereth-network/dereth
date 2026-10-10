//! The Horizon interface as the third interface: chosen live in the world, switched to and from the
//! modern and the classic interface in any order, and refused, with the interface shown staying,
//! while the host has not got its art.
//!
//! Behaviour: none (experimental Horizon interface)

use super::*;
use crate::{
    app::CoreApp,
    platform::{
        host::{Host, NullHost},
        window::HostEvent,
    },
};
use dereth_client_contract::options::{
    interface::{Interface, INTERFACE},
    store,
};
use dereth_client_contract::PrefValue;

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
}

/// The client on host `H` at the character list in the modern interface, its settings kept in
/// `state`.
fn at_the_character_list<H: Host>(state: &std::path::Path) -> (CoreApp<H>, ClientShell<H>) {
    let cfg = dereth_client_runtime::config::Config {
        headless: true,
        connect: false,
        sound: false,
        world: false,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: state.join("prefs.ini"),
        ..Default::default()
    };
    let mut app = CoreApp::<H>::bring_up_with_store(
        cfg,
        None,
        |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
    )
    .expect("real tables and a device-free presentation");
    let mut shell = ClientShell::with_window_events(app.window.raw_handle(), Default::default());
    app.start_shell(&mut shell).expect("real input and UI");
    for _ in 0..2 {
        assert!(app.frame(&mut shell));
    }
    (app, shell)
}

/// The client on host `H` at the gameplay screen in the modern interface.
fn at_the_gameplay_screen<H: Host>(state: &std::path::Path) -> (CoreApp<H>, ClientShell<H>) {
    let (mut app, mut shell) = at_the_character_list::<H>(state);
    shell
        .modern
        .ui
        .as_mut()
        .expect("UI")
        .queue(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..6 {
        assert!(app.frame(&mut shell));
    }
    (app, shell)
}

impl Client {
    /// The client at the gameplay screen in the modern interface.
    fn new(state: &std::path::Path) -> Self {
        let (app, shell) = at_the_gameplay_screen::<NullHost>(state);
        Self { app, shell }
    }

    /// The client at the character list in the modern interface: no interface has reached the
    /// world, so none has registered the character session's input maps.
    fn before_the_world(state: &std::path::Path) -> Self {
        let (app, shell) = at_the_character_list::<NullHost>(state);
        Self { app, shell }
    }

    /// The Horizon interface brought up beside the others and not shown, with no art behind it,
    /// starting on `screen`.
    fn install_horizon(&mut self, screen: dereth_horizon::options::StartScreen) {
        let mut ui = dereth_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(dereth_horizon::art::Art::empty()),
            dereth_horizon::options::HorizonOptions {
                screen,
                ..Default::default()
            },
            None,
        );
        ui.start(&mut self.app.ui_context());
        self.shell.horizon.ui = Some(ui);
    }

    /// The Horizon interface brought up beside the others and not shown, in the world, with its
    /// bodies drawn between keyframes (`smooth`) or at them.
    fn install_horizon_drawing(&mut self, smooth: bool) {
        self.install_horizon_with(dereth_horizon::options::HorizonOptions {
            smooth_animation: smooth,
            ..Default::default()
        });
    }

    /// The Horizon interface brought up beside the others and not shown, in the world, with
    /// `options`.
    fn install_horizon_with(&mut self, options: dereth_horizon::options::HorizonOptions) {
        let mut ui = dereth_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(dereth_horizon::art::Art::empty()),
            dereth_horizon::options::HorizonOptions {
                screen: dereth_horizon::options::StartScreen::Game,
                ..options
            },
            None,
        );
        ui.start(&mut self.app.ui_context());
        self.shell.horizon.ui = Some(ui);
    }

    /// The classic interface brought up beside the others and not shown.
    fn install_classic(&mut self, state: &std::path::Path) {
        struct Fonts;
        impl dereth_classic_dat::fonts::FontSource for Fonts {
            fn rasterize(
                &self,
                _: &dereth_classic_dat::fonts::FontSpec,
            ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
                Ok(Default::default())
            }
        }
        let portal = std::path::PathBuf::from(
            std::env::var_os("DERETH_CLASSIC_PORTAL").expect("classic portal"),
        );
        let art = std::sync::Arc::new(
            dereth_classic_ui::art::ClassicArt::new(
                dereth_classic_dat::ClassicPortal::open(&portal).unwrap(),
                &Fonts,
            )
            .unwrap(),
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
        ui.start(&mut self.app.ui_context())
            .expect("the classic interface starts");
        self.shell.classic.ui = Some(ui);
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    fn choose(&mut self, interface: Interface) {
        store::set_value(INTERFACE, PrefValue::Int(interface.value()));
        self.frame();
    }

    /// Which interface each part of the client says is shown: the shell, the HUD's receivers
    /// and the chat.
    fn shown(&self) -> Interface {
        let shell = self.shell.shown_interface();
        assert_eq!(
            self.app.hud.classic_active,
            shell == Interface::Classic,
            "the HUD's classic receivers follow the shell"
        );
        assert_eq!(
            self.app.hud.horizon_active,
            shell == Interface::Horizon,
            "the HUD's Horizon receivers follow the shell"
        );
        shell
    }

    fn finish(self) {
        let Self { app, mut shell } = self;
        app.shutdown(&mut shell);
    }
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn each_interface_is_shown_live_from_each_other_one() {
    let state = std::env::temp_dir().join(format!("dereth-switch-horizon-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_classic(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Lobby);
    assert_eq!(c.shown(), Interface::Modern);
    for (to, from) in [
        (Interface::Horizon, Interface::Modern),
        (Interface::Classic, Interface::Horizon),
        (Interface::Horizon, Interface::Classic),
        (Interface::Modern, Interface::Horizon),
        (Interface::Classic, Interface::Modern),
        (Interface::Modern, Interface::Classic),
    ] {
        assert_eq!(c.shown(), from);
        c.choose(to);
        assert_eq!(c.shown(), to, "{from:?} -> {to:?}");
        assert_eq!(Interface::chosen(), to);
        for _ in 0..3 {
            c.frame();
        }
        assert_eq!(c.shown(), to, "{to:?} stays shown");
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_draws_its_hud_through_the_shared_overlay_only_while_shown() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-draw-{}", std::process::id()));
    let mut c = Client::new(&state);
    // With no server, the HUD over a sample character.
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..3 {
        c.frame();
    }
    let horizon = c.shell.horizon.ui.as_ref().expect("the Horizon interface");
    assert!(
        !horizon.overlay_items().is_empty(),
        "the shown Horizon interface composes an overlay"
    );
    c.choose(Interface::Modern);
    let drawn = c
        .shell
        .horizon
        .ui
        .as_ref()
        .unwrap()
        .overlay_stats()
        .quads_drawn;
    for _ in 0..3 {
        c.frame();
    }
    assert_eq!(
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .overlay_stats()
            .quads_drawn,
        drawn,
        "the Horizon interface draws nothing while another is shown"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_choice_comes_up_on_its_own_art_with_nothing_to_build_first() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-own-art-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.choose(Interface::Horizon);
    assert_eq!(c.shown(), Interface::Horizon);
    assert_eq!(Interface::chosen(), Interface::Horizon);
    let ui = c.shell.horizon.ui.as_ref().expect("brought up");
    assert!(
        ui.ui.art.has_piece("window.tl"),
        "its own pieces are behind it"
    );
    for _ in 0..3 {
        c.frame();
    }
    assert_eq!(c.shown(), Interface::Horizon);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

thread_local! {
    /// What [`ArtLater`] has of the Horizon art, on this test's thread: nothing yet (loading), the
    /// art, or why it cannot have it.
    static HANDED_ART: std::cell::RefCell<
        Option<Result<std::sync::Arc<dereth_horizon::pieces::Pieces>, String>>,
    > = const { std::cell::RefCell::new(None) };
}

/// The host with nothing under it, whose Horizon art is loading until the test hands it over, as
/// a page's is until it has fetched it.
struct ArtLater;

impl Host for ArtLater {
    const BUILD_ID: &'static str = NullHost::BUILD_ID;
    type Clipboard = <NullHost as Host>::Clipboard;
    fn open_platform(
        cfg: &dereth_client_runtime::config::Config,
        events: crate::platform::window::WindowEvents,
    ) -> Result<dereth_client_runtime::app::Platform, dereth_client_runtime::app::StartupError>
    {
        NullHost::open_platform(cfg, events)
    }
    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        NullHost::local_utc_offset_secs(unix_secs)
    }
    fn launch_uri(url: &str) -> i32 {
        NullHost::launch_uri(url)
    }
    fn install_default_output() {}
    fn clipboard() -> Self::Clipboard {
        NullHost::clipboard()
    }
    fn cursor_images(window: Option<isize>) -> Box<dyn crate::cursor::CursorImages> {
        NullHost::cursor_images(window)
    }
    fn horizon_art() -> crate::platform::host::HorizonArt {
        use crate::platform::host::HorizonArt;
        HANDED_ART.with(|art| match art.borrow().clone() {
            None => HorizonArt::Loading,
            Some(Ok(pieces)) => HorizonArt::Ready(pieces),
            Some(Err(why)) => HorizonArt::Unavailable(why),
        })
    }
}

/// How many of the chat lines the interfaces were handed say `text`.
fn lines_saying(shell: &ClientShell<ArtLater>, text: &str) -> usize {
    shell
        .classic
        .history()
        .filter(|l| l.body.contains(text))
        .count()
}

/// A saved choice of Horizon while its art is loading starts in the modern interface, keeps the
/// choice and says once that the art is loading; the frame after the art is in, Horizon is shown
/// with nothing chosen again.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_choice_waits_in_the_modern_interface_until_its_art_is_in() {
    use dereth_client_contract::options::interface::HORIZON_ART_LOADING;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-art-later-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("prefs.ini"), "[UI]\nInterface=Horizon\n").unwrap();
    let (mut app, mut shell) = at_the_gameplay_screen::<ArtLater>(&state);
    for _ in 0..5 {
        assert!(app.frame(&mut shell));
    }
    assert_eq!(shell.shown_interface(), Interface::Modern, "it waits");
    assert_eq!(Interface::chosen(), Interface::Horizon, "the choice stays");
    assert!(shell.horizon.ui.is_none(), "nothing was brought up");
    assert_eq!(lines_saying(&shell, HORIZON_ART_LOADING), 1, "said once");
    // The page has fetched the art.
    HANDED_ART.with(|art| {
        *art.borrow_mut() = Some(Ok(std::sync::Arc::new(
            dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
        )));
    });
    for _ in 0..3 {
        assert!(app.frame(&mut shell));
    }
    assert_eq!(shell.shown_interface(), Interface::Horizon);
    assert_eq!(Interface::chosen(), Interface::Horizon);
    assert!(
        shell
            .horizon
            .ui
            .as_ref()
            .expect("brought up")
            .ui
            .art
            .has_piece("window.tl"),
        "on the art the host handed over"
    );
    assert_eq!(lines_saying(&shell, HORIZON_ART_LOADING), 1);
    app.shutdown(&mut shell);
    let _ = std::fs::remove_dir_all(&state);
}

/// A choice of Horizon on a host that cannot get its art goes back to the interface shown, and
/// the chat gives the host's reason.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_horizon_choice_is_refused_with_the_host_s_reason_when_its_art_cannot_be_had() {
    let why = "The Horizon interface's art did not load: pkg/horizon/fonts.json: HTTP 404";
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-art-failed-{}",
        std::process::id()
    ));
    HANDED_ART.with(|art| *art.borrow_mut() = Some(Err(why.to_owned())));
    let (mut app, mut shell) = at_the_gameplay_screen::<ArtLater>(&state);
    store::set_value(INTERFACE, PrefValue::Int(Interface::Horizon.value()));
    for _ in 0..3 {
        assert!(app.frame(&mut shell));
    }
    assert_eq!(shell.shown_interface(), Interface::Modern, "refused");
    assert_eq!(
        Interface::chosen(),
        Interface::Modern,
        "the choice goes back"
    );
    assert!(shell.horizon.ui.is_none());
    assert_eq!(lines_saying(&shell, why), 1, "the chat says why");
    app.shutdown(&mut shell);
    let _ = std::fs::remove_dir_all(&state);
}

/// The version the Horizon interface's pre-game screens show is the one the host names the
/// client by, as `@version` prints it.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_reads_the_client_s_version_the_host_names() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-version-{}",
        std::process::id()
    ));
    let mut c = Client::before_the_world(&state);
    c.app.interaction.client_build_id = "dereth-client 9.8.7";
    let read = dereth_horizon::state::snapshot(&c.app.ui_context(), Vec::new());
    assert_eq!(read.client_version, "9.8.7");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_line_the_horizon_interface_sends_reaches_the_game_and_its_answer_reaches_the_chat() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-line-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .outbox
        .emit(dereth_client_contract::UiRequest::ChatLine {
            text: "@version".into(),
            window: 0,
        });
    for _ in 0..3 {
        c.frame();
    }
    assert!(
        c.shell
            .horizon
            .ui
            .as_mut()
            .unwrap()
            .outbox
            .take()
            .is_empty(),
        "every request was handed on"
    );
    let answered = c
        .shell
        .classic
        .history()
        .any(|l| l.body.contains("Client version"));
    assert!(answered, "the game answered the line in the chat");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_spellbook_filter_the_horizon_interface_sets_is_the_game_s_filter() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-filter-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    let mask = dereth_client_contract::spellbook::DEFAULT_SPELL_FILTERS
        & !dereth_client_contract::spellbook::level_mask(1);
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .outbox
        .emit(dereth_client_contract::UiRequest::SetSpellbookFilter { mask });
    for _ in 0..3 {
        c.frame();
    }
    assert_eq!(c.app.objects.world.player_system.spell_filters, mask);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// A key `vk` (scan code `scan`) pressed and released over two frames, typing `text` if any.
fn tap(c: &mut Client, vk: usize, scan: u16, text: Option<&str>) {
    use dereth_input::keys::Key;
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(vk, scan),
        pressed: true,
        text: text.map(str::to_owned),
    });
    c.frame();
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(vk, scan),
        pressed: false,
        text: None,
    });
    c.frame();
}

/// How many actions the device input has handed the game while W is held for a few frames.
fn actions_while_walking(c: &mut Client) -> u64 {
    use dereth_input::keys::Key;
    let before = c.app.actions.stats().submitted;
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(0x57, 0x11),
        pressed: true,
        text: Some("w".into()),
    });
    for _ in 0..5 {
        c.frame();
    }
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(0x57, 0x11),
        pressed: false,
        text: None,
    });
    for _ in 0..2 {
        c.frame();
    }
    c.app.actions.stats().submitted - before
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn after_a_line_is_typed_and_sent_in_the_horizon_chat_the_movement_keys_reach_the_game_again() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-walk-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    let walking = actions_while_walking(&mut c);
    assert!(walking > 0, "W walks before the chat is touched");
    // Enter opens the line, a letter is typed, Enter sends it.
    tap(&mut c, 0x0D, 0x1C, None);
    tap(&mut c, 0x48, 0x23, Some("h"));
    tap(&mut c, 0x49, 0x17, Some("i"));
    tap(&mut c, 0x0D, 0x1C, None);
    for _ in 0..2 {
        c.frame();
    }
    assert!(
        !c.shell.horizon.ui.as_ref().unwrap().ui.text_focus,
        "the chat line gave the keyboard back"
    );
    let after = actions_while_walking(&mut c);
    assert!(after > 0, "W walks after a line was sent");
    // The line opened and typed in, then a click in the world: the keyboard goes back.
    tap(&mut c, 0x0D, 0x1C, None);
    tap(&mut c, 0x48, 0x23, Some("h"));
    assert!(
        c.shell.horizon.ui.as_ref().unwrap().ui.text_focus,
        "the line has the keyboard"
    );
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    for pressed in [true, false] {
        c.shell.queue_window_event(HostEvent::MouseInput {
            button: crate::platform::keys::MouseButton::Left,
            pressed,
        });
        c.frame();
    }
    c.frame();
    assert!(
        !c.shell.horizon.ui.as_ref().unwrap().ui.text_focus,
        "a click in the world let the keyboard go"
    );
    assert!(
        actions_while_walking(&mut c) > 0,
        "W walks after a click away"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_game_screen_takes_the_game_s_keys_when_no_other_interface_reached_the_world() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-keys-{}", std::process::id()));
    let mut c = Client::before_the_world(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    // I, the Horizon layout's inventory key.
    tap(&mut c, 0x49, 0x17, None);
    assert!(
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .ui
            .windows
            .is_open(dereth_horizon::ui::panels::WindowId::Inventory),
        "the inventory key opens the inventory"
    );
    assert!(actions_while_walking(&mut c) > 0, "W walks");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_power_bar_fills_as_an_attack_builds_and_empties_when_it_ends() {
    use dereth_client_model::combat::PowerBarMode;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-power-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let combat = &mut c.app.objects.world.combat;
    combat.begin_power_bar(PowerBarMode::Combat, true, 0);
    combat.set_power_bar_level(0.4);
    c.frame();
    let power = || c.shell.horizon.ui.as_ref().unwrap().power();
    assert_eq!(power(), Some(0.4), "the attack's level fills the bar");
    c.app.objects.world.combat.hide_power_bar();
    c.frame();
    assert_eq!(
        c.shell.horizon.ui.as_ref().unwrap().power(),
        None,
        "the attack's end empties it"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_bare_channel_command_sent_from_the_horizon_chat_switches_the_talk_focus_and_it_stays() {
    use dereth_client_model::chat::TalkFocus;
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-cg-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    // The player is in the General channel.
    c.app
        .objects
        .world
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    for _ in 0..2 {
        c.frame();
    }
    let type_cg = |c: &mut Client, space: bool| {
        tap(c, 0x0D, 0x1C, None);
        tap(c, 0xBF, 0x35, Some("/"));
        tap(c, 0x43, 0x2E, Some("c"));
        tap(c, 0x47, 0x22, Some("g"));
        if space {
            tap(c, 0x20, 0x39, Some(" "));
        }
        tap(c, 0x0D, 0x1C, None);
        for _ in 0..3 {
            c.frame();
        }
    };
    for space in [true, false] {
        c.app.objects.world.chat.talk_focus = TalkFocus::All;
        type_cg(&mut c, space);
        assert_eq!(
            c.app.objects.world.chat.talk_focus,
            TalkFocus::General,
            "/cg{} and Enter",
            if space { " " } else { "" }
        );
        assert_eq!(
            c.app
                .objects
                .world
                .chat
                .entries
                .get(&dereth_client_contract::chat::interface::window::MAIN)
                .map(|e| e.text.as_str()),
            Some(""),
            "the command is not left in the line"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_screens_before_the_world_keep_the_player_s_size_and_the_others_the_login_size() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-size-{}", std::process::id()));
    let mut c = Client::before_the_world(&state);
    assert!(
        c.app.uses_forced_resolution(),
        "the modern character list runs at the login size"
    );
    c.install_horizon(dereth_horizon::options::StartScreen::Lobby);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    assert!(
        !c.app.uses_forced_resolution(),
        "the Horizon character list runs at the player's size"
    );
    c.choose(Interface::Modern);
    for _ in 0..2 {
        c.frame();
    }
    assert!(
        c.app.uses_forced_resolution(),
        "back in the modern interface, the login size again"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn an_item_the_horizon_interface_drops_on_the_world_is_picked_for_where_it_was_let_go() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-drop-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let horizon = c.shell.horizon.ui.as_mut().unwrap();
    horizon.frame_input.mouse = (412.0, 233.0);
    horizon
        .outbox
        .emit(dereth_client_contract::UiRequest::DragDrop {
            item: dereth_primitives::ObjectId(0x8000_0042),
            target: dereth_client_contract::view::DropTarget::World,
        });
    c.frame();
    assert_eq!(c.app.interaction.cursor(), (412, 233));
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn with_nothing_selected_no_interface_lays_a_ring_on_the_ground() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-ring-{}", std::process::id()));
    let mut c = Client::new(&state);
    let marker = |c: &Client| {
        c.app
            .present
            .as_any()
            .downcast_ref::<crate::present::NullPresentation>()
            .expect("the headless presentation")
            .ground_markers()
            .first()
            .copied()
    };
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(
        marker(&c),
        None,
        "the Horizon game screen with no selection"
    );
    c.choose(Interface::Modern);
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(marker(&c), None, "the modern interface");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// Whether `scan` (a key's scan code, unmodified) does `name` in the input map the key pages list
/// it in, in the key map in use.
fn key_does(c: &Client, scan: u16, name: &str) -> bool {
    let input = c.shell.shared.input.as_ref().expect("input");
    let action = dereth_client_contract::actions::names::action_for_enum_name(name).expect(name);
    let map = dereth_input::presentation::rows_of(action)
        .next()
        .expect("a row")
        .input_map();
    let key = dereth_input::scheme::keyboard_key(&input.manager.keymap, scan, 0).expect("a key");
    input
        .keys_for_action(action, map)
        .iter()
        .any(|k| k.is_exactly_equal(&key))
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn bodies_are_drawn_between_ticks_only_while_horizon_is_shown_with_its_box_ticked() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-smooth-movement-{}",
        std::process::id()
    ));
    for ticked in [true, false] {
        let mut c = Client::new(&state);
        c.install_classic(&state);
        c.install_horizon_with(dereth_horizon::options::HorizonOptions {
            smooth_movement: ticked,
            ..Default::default()
        });
        assert!(
            !c.app.smooth_movement,
            "the modern interface draws as the game does"
        );
        for (to, smooth) in [
            (Interface::Horizon, ticked),
            (Interface::Classic, false),
            (Interface::Horizon, ticked),
            (Interface::Modern, false),
        ] {
            c.choose(to);
            c.frame();
            assert_eq!(c.shown(), to);
            assert_eq!(
                c.app.smooth_movement, smooth,
                "{to:?}, the box ticked {ticked}"
            );
            assert!(!c.app.smooth_animation, "{to:?}: the other box is its own");
        }
        c.finish();
    }
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn bodies_are_drawn_between_keyframes_only_while_horizon_is_shown_with_its_box_ticked() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-smooth-{}",
        std::process::id()
    ));
    for ticked in [true, false] {
        let mut c = Client::new(&state);
        c.install_classic(&state);
        c.install_horizon_drawing(ticked);
        assert!(
            !c.app.smooth_animation,
            "the modern interface draws as the game does"
        );
        for (to, smooth) in [
            (Interface::Horizon, ticked),
            (Interface::Classic, false),
            (Interface::Horizon, ticked),
            (Interface::Modern, false),
        ] {
            c.choose(to);
            c.frame();
            assert_eq!(c.shown(), to);
            assert_eq!(
                c.app.smooth_animation, smooth,
                "{to:?}, the box ticked {ticked}"
            );
        }
        c.finish();
    }
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_plays_with_its_own_keys_and_camera_and_gives_the_others_back() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-keys2-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    // The modern keys: Q locks the run, E examines.
    assert!(key_does(&c, 0x10, "MovementRunLock"));
    assert!(key_does(&c, 0x12, "SelectionExamine"));
    assert!(c.app.orbit.is_none(), "the game's camera");
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    // The Horizon layout: Q and E step sideways, R locks the run, I is the inventory, V the front view.
    assert!(key_does(&c, 0x10, "MovementStrafeLeft"));
    assert!(key_does(&c, 0x12, "MovementStrafeRight"));
    assert!(key_does(&c, 0x13, "MovementRunLock"));
    assert!(key_does(&c, 0x17, "ToggleInventoryPanel"));
    assert!(key_does(&c, 0x2F, "LookAtFront"));
    assert!(!key_does(&c, 0x10, "MovementRunLock"));
    assert!(c.app.orbit.is_some(), "the interface's own camera");
    c.choose(Interface::Modern);
    c.frame();
    assert!(
        key_does(&c, 0x10, "MovementRunLock"),
        "the modern keys again"
    );
    assert!(key_does(&c, 0x12, "SelectionExamine"));
    assert!(c.app.orbit.is_none(), "the game's camera again");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn under_camera_based_movement_back_runs_forward_and_under_character_based_it_backs_up() {
    use dereth_client_contract::actions::{movement as a, Action};
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-back-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    // Camera-based: chosen, the keyboard's own default being character-based.
    let movement = |c: &mut Client, m| {
        c.shell
            .horizon
            .ui
            .as_mut()
            .unwrap()
            .ui
            .options
            .orbit
            .movement = m;
    };
    movement(&mut c, dereth_client_runtime::orbit::MovementMode::Camera);
    c.frame();
    assert_eq!(
        c.app.orbit.map(|o| o.movement),
        Some(dereth_client_runtime::orbit::MovementMode::Camera)
    );
    c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
    c.frame();
    assert!(c.app.char_input.forward && !c.app.char_input.back);
    assert!(c.app.orbit_keys.held.away(), "faced away from the camera");
    c.app.inject_action(Action::end(a::MOVE_BACKWARD));
    c.frame();
    assert!(!c.app.char_input.forward);
    // Character-based: the game's own.
    movement(
        &mut c,
        dereth_client_runtime::orbit::MovementMode::Character,
    );
    c.frame();
    c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
    c.frame();
    assert!(c.app.char_input.back && !c.app.char_input.forward);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_slash_key_opens_the_horizon_chat_line_with_one_slash() {
    use dereth_client_model::chat::TalkFocus;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-slash-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.app
        .objects
        .world
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    for _ in 0..2 {
        c.frame();
    }
    // "/" opens the line; "cg " after it is the General channel's command only with one slash.
    tap(&mut c, 0xBF, 0x35, Some("/"));
    tap(&mut c, 0x43, 0x2E, Some("c"));
    tap(&mut c, 0x47, 0x22, Some("g"));
    tap(&mut c, 0x20, 0x39, Some(" "));
    assert_eq!(c.app.objects.world.chat.talk_focus, TalkFocus::General);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_target_the_horizon_interface_gives_ends_the_use_cursor() {
    use dereth_client_runtime::interaction::TargetMode;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-target-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let horizon = c.shell.horizon.ui.as_mut().unwrap();
    horizon
        .outbox
        .emit(dereth_client_contract::UiRequest::SetTargetMode(
            dereth_client_contract::TargetMode::Use,
        ));
    c.frame();
    assert_eq!(c.app.interaction.target_mode(), TargetMode::Use);
    c.shell.horizon.ui.as_mut().unwrap().outbox.emit(
        dereth_client_contract::UiRequest::ExecuteTargetItem(dereth_primitives::ObjectId(
            0x5000_0001,
        )),
    );
    c.frame();
    assert_eq!(c.app.interaction.target_mode(), TargetMode::None);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_keeps_the_game_view_s_clock_running_between_messages() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-clock-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let before = c.app.hud.now.0;
    std::thread::sleep(std::time::Duration::from_millis(30));
    c.frame();
    assert!(
        c.app.hud.now.0 > before,
        "{} then {}",
        before,
        c.app.hud.now.0
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_turns_the_camera_with_the_arrows_and_uses_with_f_and_the_full_stop() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-keys2-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    assert!(key_does(&c, 0xCB, "CameraRotateLeft"));
    assert!(key_does(&c, 0xCD, "CameraRotateRight"));
    assert!(key_does(&c, 0xC8, "CameraRotateUp"));
    assert!(key_does(&c, 0xD0, "CameraRotateDown"));
    assert!(key_does(&c, 0x21, "USE"));
    assert!(key_does(&c, 0x34, "USE"));
    // T and F1 to F9 are the interface's own: the game's map gives them nothing.
    let input = c.shell.shared.input.as_ref().expect("input");
    for scan in [0x14, 0x3B, 0x43] {
        let key =
            dereth_input::scheme::keyboard_key(&input.manager.keymap, scan, 0).expect("a key");
        assert!(
            dereth_input::scheme::keyboard_bindings(&input.manager.keymap)
                .all(|(_, k, _)| !k.is_exactly_equal(&key)),
            "scan {scan:#x} is unbound"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn both_mouse_buttons_held_over_the_world_run_forward_in_the_horizon_interface() {
    use crate::platform::keys::MouseButton;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-steer-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    let press = |c: &mut Client, button, pressed| {
        c.shell
            .queue_window_event(HostEvent::MouseInput { button, pressed });
        c.frame();
        c.frame();
    };
    press(&mut c, MouseButton::Left, true);
    assert!(
        !c.app.char_input.forward,
        "one button turns the camera only"
    );
    press(&mut c, MouseButton::Right, true);
    assert!(c.app.char_input.forward, "both run");
    assert!(c.app.mouse_look, "and still turn the camera");
    press(&mut c, MouseButton::Left, false);
    assert!(!c.app.char_input.forward, "one let go stops the run");
    assert!(c.app.mouse_look, "the other still turns the camera");
    press(&mut c, MouseButton::Right, false);
    assert!(!c.app.mouse_look);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn under_character_based_movement_the_turning_keys_turn_with_the_left_button_and_step_with_the_right(
) {
    use crate::platform::keys::MouseButton;
    use dereth_client_contract::actions::{movement as a, Action};
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-charturn-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .ui
        .options
        .orbit
        .movement = dereth_client_runtime::orbit::MovementMode::Character;
    c.frame();
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    let press = |c: &mut Client, button, pressed| {
        c.shell
            .queue_window_event(HostEvent::MouseInput { button, pressed });
        c.frame();
        c.frame();
    };
    // The left button held: the turning key still turns the player.
    press(&mut c, MouseButton::Left, true);
    c.app.inject_action(Action::begin(a::TURN_LEFT));
    c.frame();
    assert!(c.app.char_input.turn_left && !c.app.char_input.step_left);
    press(&mut c, MouseButton::Left, false);
    // The right button steers: the same key held now steps sideways.
    press(&mut c, MouseButton::Right, true);
    assert!(
        c.app.char_input.step_left && !c.app.char_input.turn_left,
        "a step while the mouse steers"
    );
    press(&mut c, MouseButton::Right, false);
    assert!(
        c.app.char_input.turn_left && !c.app.char_input.step_left,
        "a turn again once it is let go"
    );
    c.app.inject_action(Action::end(a::TURN_LEFT));
    c.frame();
    assert!(!c.app.char_input.turn_left && !c.app.char_input.step_left);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_keeps_the_game_s_spell_and_shortcut_keys_and_inherits_none_of_its_others() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-own-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let input = c.shell.shared.input.as_mut().expect("input");
    let defaults = input.horizon_defaults();
    let actions_of = |scan: u16| -> Vec<u32> {
        let Some(key) = dereth_input::scheme::keyboard_key(&defaults, scan, 0) else {
            return Vec::new();
        };
        dereth_input::scheme::keyboard_bindings(&defaults)
            .filter(|(_, k, _)| k.is_exactly_equal(&key))
            .map(|(_, _, a)| a.0)
            .collect()
    };
    let named = |name: &str| {
        dereth_client_contract::actions::names::action_for_enum_name(name)
            .unwrap()
            .0
    };
    // Kept: the spell keys, the shortcut keys.
    assert!(
        actions_of(0xD2).contains(&named("CombatPrevSpellTab")),
        "Insert"
    );
    assert!(
        actions_of(0xC9).contains(&named("CombatNextSpellTab")),
        "Page Up"
    );
    assert!(actions_of(0x02).contains(&named("UseQuickSlot_1")), "1");
    // The left arrow turns the camera, and still moves the cursor in a text box.
    assert!(
        actions_of(0xCB).contains(&named("CameraRotateLeft")),
        "left arrow"
    );
    assert!(
        actions_of(0xCB).contains(&named("CursorCharLeft")),
        "cursor left"
    );
    // Not inherited: F12 (the game's inventory key, the layout's screenshot), Z (the game's
    // sidestep).
    assert_eq!(actions_of(0x58), [named("CaptureScreenshot")], "F12");
    assert!(actions_of(0x2C).is_empty(), "Z: {:?}", actions_of(0x2C));
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_loses_the_game_s_keys_it_inherited_and_keeps_the_player_s_own() {
    use crate::input::horizon_scheme::drop_inherited;
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-drop-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let game = dereth_input::InputManager::load_over_defaults(None, &shipped, None);
    let key = |scan| dereth_input::scheme::keyboard_key(&template, scan, 0).expect("a key");
    let actions = |map: &dereth_input::MasterInputMap, scan| -> Vec<u32> {
        dereth_input::scheme::keyboard_bindings(map)
            .filter(|(_, k, _)| k.is_exactly_equal(&key(scan)))
            .map(|(_, _, a)| a.0)
            .collect()
    };
    // A map saved when the layout still inherited the game's F12, with the player's own Z.
    let mut saved = defaults.clone();
    for (m, k, a) in dereth_input::scheme::keyboard_bindings(&game)
        .filter(|(_, k, _)| k.is_exactly_equal(&key(0x58)))
        .collect::<Vec<_>>()
    {
        saved.create_input_map(m).add_mapping(k, a);
    }
    let inventory =
        dereth_client_contract::actions::names::action_for_enum_name("ToggleInventoryPanel")
            .expect("an action");
    saved
        .create_input_map(dereth_input::InputMapId(0x1000_0009))
        .add_mapping(key(0x2C), inventory);
    // Saved when the layout's arrows took the text box's cursor keys too.
    for section in &mut saved.sections {
        if section.input_map_id == dereth_input::InputMapId(7) {
            section.unbind_by_key(&key(0xCB));
        }
    }
    let cursor_left =
        dereth_client_contract::actions::names::action_for_enum_name("CursorCharLeft")
            .expect("an action");
    assert!(!actions(&saved, 0xCB).contains(&cursor_left.0));
    assert!(!actions(&saved, 0x58).is_empty());
    drop_inherited(&mut saved, &shipped, &defaults);
    assert!(
        actions(&saved, 0xCB).contains(&cursor_left.0),
        "the cursor key is back"
    );
    assert!(
        actions(&saved, 0x58).is_empty(),
        "the inherited F12 is gone"
    );
    assert_eq!(actions(&saved, 0x2C), [inventory.0], "the player's Z stays");
    assert_eq!(
        actions(&saved, 0xD2),
        actions(&defaults, 0xD2),
        "a kept key of the game's stays"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_ctrl_c_is_not_c() {
    use dereth_horizon::ui::panels::WindowId;
    use dereth_input::keys::Key;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-ctrlc-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    let open = |c: &Client| {
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .ui
            .windows
            .is_open(WindowId::Character)
    };
    let ctrl = |c: &mut Client, pressed: bool| {
        c.shell.queue_window_event(HostEvent::KeyboardInput {
            key: Key::new(0x11, 0x1D),
            pressed,
            text: None,
        });
        c.frame();
    };
    ctrl(&mut c, true);
    tap(&mut c, 0x43, 0x2E, None);
    ctrl(&mut c, false);
    assert!(!open(&c), "Ctrl+C opens nothing");
    tap(&mut c, 0x43, 0x2E, Some("c"));
    assert!(open(&c), "C opens the character window");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_older_than_its_layout_takes_the_layout_s_new_keys_but_keeps_a_rebinding()
{
    use crate::input::horizon_scheme::bring_up_to_date;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-older-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let game = dereth_input::InputManager::load_over_defaults(None, &shipped, None);
    let key = |scan| dereth_input::scheme::keyboard_key(&template, scan, 0).expect("a key");
    let actions = |map: &dereth_input::MasterInputMap, scan| -> Vec<u32> {
        dereth_input::scheme::keyboard_bindings(map)
            .filter(|(_, k, _)| k.is_exactly_equal(&key(scan)))
            .map(|(_, _, a)| a.0)
            .collect()
    };
    let rotate_left =
        dereth_client_contract::actions::names::action_for_enum_name("CameraRotateLeft")
            .expect("an action")
            .0;
    // A map saved before the layout named the arrows: the left arrow as the game binds it, and
    // the player's own choice for Q.
    let mut saved = defaults.clone();
    for scan in [0xCB_u16, 0x10] {
        for section in &mut saved.sections {
            section.unbind_by_key(&key(scan));
        }
    }
    for (m, k, a) in dereth_input::scheme::keyboard_bindings(&game)
        .filter(|(_, k, _)| k.is_exactly_equal(&key(0xCB)))
        .collect::<Vec<_>>()
    {
        saved.create_input_map(m).add_mapping(k, a);
    }
    let jump = dereth_client_contract::actions::names::action_for_enum_name("MovementJump")
        .expect("an action");
    saved
        .create_input_map(dereth_input::InputMapId(0x1000_0001))
        .add_mapping(key(0x10), jump);
    let turn_left =
        dereth_client_contract::actions::names::action_for_enum_name("MovementTurnLeft")
            .expect("an action")
            .0;
    assert!(
        actions(&saved, 0xCB).contains(&turn_left),
        "the old map: the arrow turns the player too"
    );
    bring_up_to_date(&mut saved, &template, &shipped, &defaults);
    assert_eq!(
        actions(&saved, 0xCB),
        actions(&defaults, 0xCB),
        "the arrow turns the camera, not the player"
    );
    assert!(actions(&saved, 0xCB).contains(&rotate_left));
    assert!(!actions(&saved, 0xCB).contains(&turn_left));
    assert_eq!(
        actions(&saved, 0x10),
        [jump.0],
        "the player's own binding stays"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_wheel_over_the_world_zooms_the_horizon_camera() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-wheel-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    // The pointer over the world, clear of every window.
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    let before = c.app.actions.stats().submitted;
    c.shell
        .queue_window_event(HostEvent::MouseWheel { notches: 1.0 });
    for _ in 0..2 {
        c.frame();
    }
    // A notch of zoom: the game's zoom begun and let go.
    assert_eq!(c.app.actions.stats().submitted - before, 2);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_layout_x_stops_and_g_examines_and_nothing_else() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-xg-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    assert_eq!(
        bindings_of(&defaults, &template, 0x2D, 0),
        ["MovementStop"],
        "X"
    );
    assert_eq!(
        bindings_of(&defaults, &template, 0x22, 0),
        ["SelectionExamine"],
        "G"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// What the key `scan` held with `meta` does in `map`: the action in each input map it is bound
/// in, by name, in order.
fn bindings_of(
    map: &dereth_input::MasterInputMap,
    template: &dereth_input::MasterInputMap,
    scan: u16,
    meta: u32,
) -> Vec<String> {
    let key = dereth_input::scheme::keyboard_key(template, scan, meta).expect("a key");
    let mut out: Vec<String> = dereth_input::scheme::keyboard_bindings(map)
        .filter(|(_, k, _)| k.is_exactly_equal(&key))
        .map(|(_, _, a)| dereth_client_contract::actions::names::enum_name_for_action(a))
        .collect();
    out.sort();
    out
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_binds_the_chat_keys_the_last_slots_the_item_keys_and_the_screenshot() {
    use crate::input::horizon_scheme::{CTRL, SHIFT};
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-layout-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let does = |scan, meta| bindings_of(&defaults, &template, scan, meta);
    // Enter and `/` open the chat line as the game's own actions, so the page shows them; Enter
    // still accepts a text box's line.
    assert!(
        does(0x1C, 0).contains(&"EnterChatMode".to_owned()),
        "{:?}",
        does(0x1C, 0)
    );
    assert!(
        does(0x1C, 0).contains(&"AcceptInput".to_owned()),
        "{:?}",
        does(0x1C, 0)
    );
    assert_eq!(does(0x35, 0), ["START_COMMAND"]);
    // 0, - and =: the tenth to twelfth shortcuts, and the spell bar's in a spell stance.
    for (scan, n) in [(0x0B, 10), (0x0C, 11), (0x0D, 12)] {
        assert_eq!(
            does(scan, 0),
            [format!("UseQuickSlot_{n}"), format!("UseSpellSlot_{n}")],
            "{scan:#x}"
        );
        // With Ctrl, the shortcut's, as Ctrl with 1 to 9 is.
        assert_eq!(
            does(scan, CTRL),
            [format!("UseQuickSlot_{n}")],
            "Ctrl {scan:#x}"
        );
    }
    // The layout's Ctrl is the key map's own: the left Ctrl key's bit.
    let left_ctrl =
        dereth_input::spec::ControlCode::new(0, dereth_input::spec::SubControlIndex::None, 0x1D);
    assert_eq!(template.meta_mode_from_key(left_ctrl), CTRL);
    // Alt with 1 to 9 reach no shortcut: not the tenth to twelfth, which 0, - and = are, nor the
    // thirteenth to eighteenth, which keep no key.
    let alt = 0x2000_0000;
    for scan in 0x02..=0x0A {
        assert!(
            !does(scan, alt)
                .iter()
                .any(|a| a.starts_with("UseQuickSlot_1")),
            "Alt {scan:#x}: {:?}",
            does(scan, alt)
        );
    }
    // Backslash, [ and ]: the closest, the previous and the next item.
    assert_eq!(does(0x2B, 0), ["SelectionClosestItem"]);
    assert_eq!(does(0x1A, 0), ["SelectionPreviousItem"]);
    assert_eq!(does(0x1B, 0), ["SelectionNextItem"]);
    // Tab and Shift+Tab: the next and the previous creature.
    assert_eq!(does(0x0F, 0), ["SelectionNextMonster"]);
    assert_eq!(does(0x0F, SHIFT), ["SelectionPreviousMonster"]);
    // Shift+R switches between walking and running.
    assert_eq!(does(0x13, SHIFT), ["MovementWalkMode"]);
    assert_eq!(does(0x58, 0), ["CaptureScreenshot"]);
    // Dropped: the numpad's zoom, Shift+Escape's end of the session.
    assert!(does(0x4A, 0).is_empty(), "numpad -: {:?}", does(0x4A, 0));
    assert!(does(0x4E, 0).is_empty(), "numpad +: {:?}", does(0x4E, 0));
    assert!(
        !does(0x01, 1).contains(&"LOGOUT".to_owned()),
        "Shift+Escape: {:?}",
        does(0x01, 1)
    );
    let nowhere = |name: &str| {
        let action = dereth_client_contract::actions::names::action_for_enum_name(name).unwrap();
        defaults
            .sections
            .iter()
            .all(|s| s.bindings().iter().all(|(_, a)| *a != action))
    };
    for name in [
        "LOGOUT",
        "CreateShortcut",
        "ToggleFriendsPanel",
        "MovementHoldSidestep",
        "UseQuickSlot_13",
        "UseQuickSlot_14",
        "UseQuickSlot_15",
        "UseQuickSlot_16",
        "UseQuickSlot_17",
        "UseQuickSlot_18",
    ] {
        assert!(nowhere(name), "{name} is bound");
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_wheel_is_the_horizon_camera_s_zoom_in_its_key_map() {
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-wheelmap-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let defaults = input.horizon_defaults();
    let camera = dereth_input::InputMapId(5);
    for (name, wheel) in [
        ("CameraMoveToward", 0x0008_0101),
        ("CameraMoveAway", 0x0008_0201),
    ] {
        let action = dereth_client_contract::actions::names::action_for_enum_name(name).unwrap();
        let keys = defaults
            .section(camera)
            .map(|s| s.keys_for_action(action))
            .unwrap_or_default();
        assert_eq!(keys.len(), 1, "{name}: {keys:?}");
        assert_eq!(
            keys[0].control,
            dereth_input::spec::ControlCode(wheel),
            "{name}"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_gives_up_the_earlier_layout_s_x_o_and_numpad_zoom_but_keeps_the_player_s(
) {
    use crate::input::horizon_scheme::{bring_up_to_date, drop_inherited};
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-retired-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let game = dereth_input::InputManager::load_over_defaults(None, &shipped, None);
    let key = |scan| dereth_input::scheme::keyboard_key(&template, scan, 0).expect("a key");
    let named = |n| dereth_client_contract::actions::names::action_for_enum_name(n).unwrap();
    let ui = dereth_input::InputMapId(0x1000_0009);
    let camera = dereth_input::InputMapId(5);
    // Written by an earlier layout: X examined, O opened the friends list, the numpad zoomed (as
    // the game binds it) and the wheel was not in the map; the player put J on the inventory.
    let mut saved = defaults.clone();
    for scan in [0x2D_u16, 0x18, 0x24] {
        for section in &mut saved.sections {
            section.unbind_by_key(&key(scan));
        }
    }
    saved
        .create_input_map(ui)
        .add_mapping(key(0x2D), named("SelectionExamine"));
    saved
        .create_input_map(ui)
        .add_mapping(key(0x18), named("ToggleFriendsPanel"));
    saved
        .create_input_map(ui)
        .add_mapping(key(0x24), named("ToggleInventoryPanel"));
    for (m, k, a) in dereth_input::scheme::keyboard_bindings(&game)
        .filter(|(_, k, _)| k.is_exactly_equal(&key(0x4A)))
        .collect::<Vec<_>>()
    {
        saved.create_input_map(m).add_mapping(k, a);
    }
    let wheel = |s: &dereth_input::MasterInputMap| {
        s.section(camera)
            .map(|s| s.keys_for_action(named("CameraMoveToward")))
            .unwrap_or_default()
            .iter()
            .any(|k| k.control == dereth_input::spec::ControlCode(0x0008_0101))
    };
    if let Some(s) = saved.sections.iter_mut().find(|s| s.input_map_id == camera) {
        s.retain(|k, _| k.control != dereth_input::spec::ControlCode(0x0008_0101));
    }
    assert!(!wheel(&saved));
    // Read back as the client reads a saved file: over the layout, then brought up to date.
    let mut loaded = dereth_input::InputManager::load_over_defaults(
        Some(&saved.to_keymap_text()),
        &[&defaults],
        Some(&input.manager.action_map),
    );
    bring_up_to_date(&mut loaded, &template, &shipped, &defaults);
    drop_inherited(&mut loaded, &shipped, &defaults);
    let does = |scan| bindings_of(&loaded, &template, scan, 0);
    assert_eq!(does(0x2D), ["MovementStop"], "X");
    assert_eq!(does(0x18), ["ToggleSocialPanel"], "O");
    assert!(does(0x4A).is_empty(), "numpad -: {:?}", does(0x4A));
    assert_eq!(does(0x24), ["ToggleInventoryPanel"], "the player's J stays");
    assert!(wheel(&loaded), "the wheel zooms");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_world_the_horizon_interface_listens_to_every_chat_channel_once_its_options_have_come() {
    use dereth_client_model::player::options::option;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-channels-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.app.host_state.in_world = true;
    let channels = [
        option::HEAR_GENERAL_CHAT,
        option::HEAR_TRADE_CHAT,
        option::HEAR_LFG_CHAT,
        option::HEAR_ROLEPLAY_CHAT,
        option::HEAR_SOCIETY_CHAT,
    ];
    let listening = |c: &Client| channels.map(|o| c.app.objects.world.player_system.options.get(o));
    for o in channels {
        c.app.objects.world.player_system.options.set(o, false);
    }
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(listening(&c), [false; 5], "the options have not come yet");
    // The player's description: listening to General only.
    c.app.objects.world.player_system.module = Some(Default::default());
    c.app
        .objects
        .world
        .player_system
        .options
        .set(option::HEAR_GENERAL_CHAT, true);
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(listening(&c), [true; 5]);
    // A channel left stays left while the character is in the world.
    c.app
        .objects
        .world
        .player_system
        .options
        .set(option::HEAR_TRADE_CHAT, false);
    for _ in 0..2 {
        c.frame();
    }
    assert!(!listening(&c)[1], "Trade stays left");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn while_the_hud_is_laid_out_no_drag_wheel_or_key_reaches_the_game_and_escape_closes_the_layout() {
    use crate::platform::keys::MouseButton;
    use dereth_horizon::ui::panels::WindowId;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-laying-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    let laying_out = |c: &Client| {
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .ui
            .windows
            .is_open(WindowId::Layout)
    };
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .ui
        .windows
        .open(WindowId::Layout, 0.0);
    c.frame();
    assert!(laying_out(&c));
    // On the minimap's outline, to drag it.
    let r = c
        .shell
        .horizon
        .ui
        .as_ref()
        .unwrap()
        .ui
        .hud
        .layout
        .outlines
        .iter()
        .find(|(name, _)| *name == "minimap")
        .map(|(_, r)| *r)
        .expect("the minimap's outline");
    let (x, y) = (f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0));
    c.shell.queue_window_event(HostEvent::CursorMoved { x, y });
    c.frame();
    let before = c.app.actions.stats().submitted;
    // Both buttons held and dragged: no camera turn, no run.
    for (button, pressed) in [(MouseButton::Left, true), (MouseButton::Right, true)] {
        c.shell
            .queue_window_event(HostEvent::MouseInput { button, pressed });
        c.frame();
    }
    c.shell.queue_window_event(HostEvent::CursorMoved {
        x: x - 60.0,
        y: y + 40.0,
    });
    c.frame();
    assert!(!c.app.mouse_look, "a drag does not turn the camera");
    assert!(!c.app.char_input.forward, "both buttons do not run");
    for button in [MouseButton::Left, MouseButton::Right] {
        c.shell.queue_window_event(HostEvent::MouseInput {
            button,
            pressed: false,
        });
        c.frame();
    }
    // The wheel: no zoom.
    c.shell
        .queue_window_event(HostEvent::MouseWheel { notches: 1.0 });
    c.frame();
    c.frame();
    assert_eq!(
        c.app.actions.stats().submitted - before,
        0,
        "the buttons and the wheel reach nothing of the game"
    );
    assert_eq!(actions_while_walking(&mut c), 0, "W does not walk");
    tap(&mut c, 0x1B, 0x01, None);
    assert!(!laying_out(&c), "Escape closes the layout");
    assert!(actions_while_walking(&mut c) > 0, "W walks again");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_o_opens_the_social_window() {
    use dereth_horizon::ui::panels::WindowId;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-social-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    tap(&mut c, 0x4F, 0x18, Some("o"));
    assert!(c
        .shell
        .horizon
        .ui
        .as_ref()
        .unwrap()
        .ui
        .windows
        .is_open(WindowId::Social));
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_loses_the_game_s_alt_keys_for_the_last_shortcuts_gains_ctrl_and_keeps_the_player_s_own(
) {
    use crate::input::horizon_scheme::{CTRL, HORIZON_KEYMAP_FILE};
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-alt-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&state);
    let mut c = Client::new(&state);
    let named = |n| dereth_client_contract::actions::names::action_for_enum_name(n).expect(n);
    let alt = 0x2000_0000;
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let key =
        |scan, meta| dereth_input::scheme::keyboard_key(&template, scan, meta).expect("a key");
    // A map saved by the earlier layout: Alt+1, Alt+2 and Alt+4 as the game gave them, no
    // Ctrl+0, and Alt+3 the player's own, on the fifth shortcut.
    let mut saved = input.horizon_defaults();
    // The shortcuts' keys' own map, where the layout binds them.
    let quick = saved
        .sections
        .iter()
        .find(|s| {
            s.bindings()
                .iter()
                .any(|(_, a)| *a == named("UseQuickSlot_10"))
        })
        .expect("the shortcuts' map")
        .input_map_id;
    for section in &mut saved.sections {
        section.unbind_by_key(&key(0x0B, CTRL));
    }
    saved
        .create_input_map(quick)
        .add_mapping(key(0x02, alt), named("UseQuickSlot_10"));
    saved
        .create_input_map(quick)
        .add_mapping(key(0x03, alt), named("UseQuickSlot_11"));
    saved
        .create_input_map(quick)
        .add_mapping(key(0x04, alt), named("UseQuickSlot_5"));
    saved
        .create_input_map(quick)
        .add_mapping(key(0x05, alt), named("UseQuickSlot_13"));
    std::fs::write(state.join(HORIZON_KEYMAP_FILE), saved.to_keymap_text()).expect("written");
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let does = |c: &Client, k: &dereth_input::ControlChord, n| {
        c.shell
            .shared
            .input
            .as_ref()
            .expect("input")
            .keys_for_action(named(n), quick)
            .iter()
            .any(|x| x.is_exactly_equal(k))
    };
    assert!(!does(&c, &key(0x02, alt), "UseQuickSlot_10"));
    assert!(!does(&c, &key(0x03, alt), "UseQuickSlot_11"));
    assert!(!does(&c, &key(0x05, alt), "UseQuickSlot_13"));
    assert!(does(&c, &key(0x0B, CTRL), "UseQuickSlot_10"));
    assert!(
        does(&c, &key(0x04, alt), "UseQuickSlot_5"),
        "the player's own key"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_with_freed_movement_arrows_and_an_earlier_shift_still_turns_the_camera_and_shifts(
) {
    use crate::input::horizon_scheme::{HORIZON_KEYMAP_FILE, SHIFT};
    use dereth_input::binding::DO_NOTHING;
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-mend-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&state);
    let mut c = Client::new(&state);
    let named = |n| dereth_client_contract::actions::names::action_for_enum_name(n).expect(n);
    let home = |n| {
        dereth_input::presentation::rows_of(named(n))
            .next()
            .expect("a row")
            .input_map()
    };
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let key =
        |scan, meta| dereth_input::scheme::keyboard_key(&template, scan, meta).expect("a key");
    // The layout's Shift is the key map's own: the left Shift key's bit.
    let left_shift =
        dereth_input::spec::ControlCode::new(0, dereth_input::spec::SubControlIndex::None, 0x2A);
    assert_eq!(template.meta_mode_from_key(left_shift), SHIFT);
    // The left arrow freed in the movement keys, as a Restore Defaults left it, and Shift+Tab
    // and Shift+R given with the bottom bit an earlier layout took for Shift.
    let mut saved = input.horizon_defaults();
    for section in &mut saved.sections {
        for scan in [0x0F, 0x13] {
            section.unbind_by_key(&key(scan, SHIFT));
        }
    }
    saved
        .create_input_map(home("MovementTurnLeft"))
        .add_mapping(key(0xCB, 0), DO_NOTHING);
    saved
        .create_input_map(home("SelectionPreviousMonster"))
        .add_mapping(key(0x0F, 1), named("SelectionPreviousMonster"));
    saved
        .create_input_map(home("MovementWalkMode"))
        .add_mapping(key(0x13, 1), DO_NOTHING);
    saved
        .create_input_map(home("MovementWalkMode"))
        .add_mapping(key(0x13, SHIFT), named("MovementWalkMode"));
    std::fs::write(state.join(HORIZON_KEYMAP_FILE), saved.to_keymap_text()).expect("written");
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    let does = |c: &Client, k: &dereth_input::ControlChord, n| {
        c.shell
            .shared
            .input
            .as_ref()
            .expect("input")
            .keys_for_action(named(n), home(n))
            .iter()
            .any(|x| x.is_exactly_equal(k))
    };
    assert!(does(&c, &key(0x0F, SHIFT), "SelectionPreviousMonster"));
    assert!(does(&c, &key(0x13, SHIFT), "MovementWalkMode"));
    assert!(!does(&c, &key(0x0F, 1), "SelectionPreviousMonster"));
    // The left arrow, held, turns the camera.
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x25, 0xE04B),
        pressed: true,
        text: None,
    });
    c.frame();
    assert!(c.app.input.look_left, "the left arrow turns the camera");
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x25, 0xE04B),
        pressed: false,
        text: None,
    });
    c.frame();
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_gives_b_and_l_their_windows_and_ctrl_tab_the_radar_and_a_saved_map_takes_them(
) {
    use crate::input::horizon_scheme::{bring_up_to_date, drop_inherited, CTRL, SHIFT};
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-bltab-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let does =
        |map: &dereth_input::MasterInputMap, scan, meta| bindings_of(map, &template, scan, meta);
    assert_eq!(does(&defaults, 0x30, 0), ["ToggleInventoryPanel"], "B");
    assert_eq!(does(&defaults, 0x26, 0), ["ToggleJournalPanel"], "L");
    assert_eq!(
        does(&defaults, 0x0F, CTRL),
        ["SelectionNextCompassItem"],
        "Ctrl+Tab"
    );
    assert_eq!(
        does(&defaults, 0x0F, CTRL | SHIFT),
        ["SelectionPreviousCompassItem"],
        "Ctrl+Shift+Tab"
    );
    assert_eq!(does(&defaults, 0x0F, 0), ["SelectionNextMonster"], "Tab");
    assert_eq!(
        does(&defaults, 0x0F, SHIFT),
        ["SelectionPreviousMonster"],
        "Shift+Tab"
    );
    // Saved before the layout gave these keys, with the player's own L on the map.
    let key =
        |scan, meta| dereth_input::scheme::keyboard_key(&template, scan, meta).expect("a key");
    let mut saved = defaults.clone();
    for (scan, meta) in [(0x30, 0), (0x26, 0), (0x0F, CTRL), (0x0F, CTRL | SHIFT)] {
        for section in &mut saved.sections {
            section.unbind_by_key(&key(scan, meta));
        }
    }
    let map = dereth_client_contract::actions::names::action_for_enum_name("ToggleMapPanel")
        .expect("an action");
    saved
        .create_input_map(dereth_input::InputMapId(0x1000_0009))
        .add_mapping(key(0x26, 0), map);
    let mut loaded = dereth_input::InputManager::load_over_defaults(
        Some(&saved.to_keymap_text()),
        &[&defaults],
        Some(&input.manager.action_map),
    );
    bring_up_to_date(&mut loaded, &template, &shipped, &defaults);
    drop_inherited(&mut loaded, &shipped, &defaults);
    assert_eq!(does(&loaded, 0x30, 0), ["ToggleInventoryPanel"], "B, saved");
    assert_eq!(
        does(&loaded, 0x26, 0),
        ["ToggleMapPanel"],
        "the player's L stays"
    );
    assert_eq!(
        does(&loaded, 0x0F, CTRL),
        ["SelectionNextCompassItem"],
        "Ctrl+Tab, saved"
    );
    assert_eq!(
        does(&loaded, 0x0F, CTRL | SHIFT),
        ["SelectionPreviousCompassItem"],
        "Ctrl+Shift+Tab, saved"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_tab_with_each_of_its_modifiers_does_only_its_own_action() {
    use dereth_input::fire::ControlType;
    let state =
        std::env::temp_dir().join(format!("dereth-switch-horizon-tabs-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    for _ in 0..2 {
        c.frame();
    }
    let input = c.shell.shared.input.as_mut().expect("input");
    let control = |input: &crate::input::InputShell, scan| {
        dereth_input::scheme::keyboard_key(&input.manager.keymap, scan, 0)
            .expect("a key")
            .control
    };
    let mut time = 1000_u32;
    let mut press = |input: &mut crate::input::InputShell, scans: &[u16]| -> Vec<String> {
        let _ = input.manager.take_events();
        for scan in scans {
            time += 300;
            let cs = control(input, *scan);
            input
                .manager
                .fire_input_event(cs, ControlType::Button, 0x80, time);
        }
        let fired: Vec<String> = input
            .manager
            .take_events()
            .iter()
            .filter(|e| e.start)
            .map(|e| dereth_client_contract::actions::names::enum_name_for_action(e.action))
            .collect();
        for scan in scans.iter().rev() {
            time += 300;
            let cs = control(input, *scan);
            input
                .manager
                .fire_input_event(cs, ControlType::Button, 0, time);
        }
        let _ = input.manager.take_events();
        fired
    };
    assert_eq!(press(input, &[0x0F]), ["SelectionNextMonster"], "Tab");
    assert_eq!(
        press(input, &[0x2A, 0x0F]),
        ["SelectionPreviousMonster"],
        "Shift+Tab"
    );
    assert_eq!(
        press(input, &[0x1D, 0x0F]),
        ["SelectionNextCompassItem"],
        "Ctrl+Tab"
    );
    assert_eq!(
        press(input, &[0x1D, 0x2A, 0x0F]),
        ["SelectionPreviousCompassItem"],
        "Ctrl+Shift+Tab"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_s_and_a_back_up_and_step_left_through_a_cast_in_either_movement() {
    use dereth_client_contract::actions::{movement as a, Action};
    use dereth_client_runtime::orbit::MovementMode;
    let state = std::env::temp_dir().join(format!(
        "dereth-switch-horizon-slide-{}",
        std::process::id()
    ));
    let mut c = Client::new(&state);
    c.install_horizon(dereth_horizon::options::StartScreen::Game);
    c.choose(Interface::Horizon);
    c.frame();
    for movement in [MovementMode::Camera, MovementMode::Character] {
        c.shell
            .horizon
            .ui
            .as_mut()
            .unwrap()
            .ui
            .options
            .orbit
            .movement = movement;
        c.frame();
        let input = |c: &Client| {
            let i = c.app.char_input;
            (i.forward, i.back, i.turn_left, i.step_left)
        };
        // Not casting: S and A run the way they point (camera-based) or back up and turn.
        c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
        c.app.inject_action(Action::begin(a::TURN_LEFT));
        c.frame();
        assert!(
            !input(&c).3,
            "{movement:?}: not casting, A does not step: {:?}",
            input(&c)
        );
        c.app.inject_action(Action::end(a::TURN_LEFT));
        c.app.inject_action(Action::end(a::MOVE_BACKWARD));
        c.frame();
        // Casting: they back up and step left, and keep doing so after the cast while held.
        c.app.objects.world.magic.casting = true;
        c.frame();
        c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
        c.app.inject_action(Action::begin(a::TURN_LEFT));
        c.frame();
        assert_eq!(
            input(&c),
            (false, true, false, true),
            "{movement:?}: casting, back and a step left, no run and no turn"
        );
        c.app.objects.world.magic.casting = false;
        c.frame();
        assert_eq!(
            input(&c),
            (false, true, false, true),
            "{movement:?}: the cast over, the keys held go on as they were"
        );
        c.app.inject_action(Action::end(a::TURN_LEFT));
        c.app.inject_action(Action::end(a::MOVE_BACKWARD));
        c.frame();
        assert_eq!(input(&c), (false, false, false, false), "{movement:?}");
        // Casting, S alone backs up.
        c.app.objects.world.magic.casting = true;
        c.frame();
        c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
        c.frame();
        assert_eq!(
            input(&c),
            (false, true, false, false),
            "{movement:?}: S backs up"
        );
        c.app.inject_action(Action::end(a::MOVE_BACKWARD));
        c.app.objects.world.magic.casting = false;
        c.frame();
        c.frame();
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}
