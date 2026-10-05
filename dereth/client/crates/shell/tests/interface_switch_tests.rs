//! Switching interfaces while a mouse button is held leaves the interface switched away from with
//! nothing of that press, so one click still opens a panel however many times the player switched.

use super::*;
use crate::{
    app::CoreApp,
    platform::{host::NullHost, keys::MouseButton, window::HostEvent},
};
use dereth_client_contract::options::{
    interface::{Interface, INTERFACE},
    store,
};
use dereth_client_contract::PrefValue;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
}

impl Client {
    /// The client at the gameplay screen in the modern interface, with the classic one brought
    /// up beside it and not shown.
    fn new(state: &std::path::Path) -> Self {
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
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
        )
        .expect("real tables and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        for _ in 0..2 {
            assert!(app.frame(&mut shell));
        }
        shell
            .modern
            .ui
            .as_mut()
            .expect("UI")
            .queue(dereth_ui::framework::mode::GAME_PLAY);
        for _ in 0..6 {
            assert!(app.frame(&mut shell));
        }
        let mut c = Self { app, shell };
        c.install_classic(state);
        c
    }

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

    fn send(&mut self, event: HostEvent) {
        self.shell.queue_window_event(event);
        self.frame();
    }

    fn choose(&mut self, interface: Interface) {
        store::set_value(INTERFACE, PrefValue::Int(interface.value()));
        self.frame();
        assert_eq!(self.app.hud.classic_active, interface == Interface::Classic);
    }

    fn button(&mut self, pressed: bool) {
        self.send(HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed,
        });
    }

    fn point_at(&mut self, h: dereth_ui::ElemHandle) {
        let r = self.ui().screen_box(h);
        self.send(HostEvent::CursorMoved {
            x: f64::from((r.x0 + r.x1) / 2),
            y: f64::from((r.y0 + r.y1) / 2),
        });
    }

    fn ui(&mut self) -> &mut dereth_ui::UiSystem {
        &mut self.shell.modern.ui.as_mut().expect("UI").ui
    }

    fn screen(&mut self) -> &mut GamePlayScreen {
        let shell = self.shell.modern.ui.as_mut().expect("UI");
        let screen = crate::hud_drive::game_screen(&mut shell.flow).expect("gameplay");
        let any: &mut dyn std::any::Any = screen;
        any.downcast_mut::<GamePlayScreen>().expect("gameplay type")
    }

    /// The toolbar's panel buttons, as (button, panel id).
    fn toolbar(&mut self) -> Vec<(dereth_ui::ElemHandle, u32)> {
        let buttons: Vec<_> = self
            .screen()
            .toolbar
            .buttons
            .iter()
            .map(|b| (b.handle, b.panel_id))
            .collect();
        assert!(buttons.len() >= 2, "the toolbar has its panel buttons");
        buttons
    }

    /// The panel the toolbar's stack shows, by panel id.
    fn current_panel(&mut self) -> Option<u32> {
        let s = self.screen();
        let e = s.panels.current?;
        s.panels
            .pages
            .iter()
            .find(|p| p.element == e)
            .map(|p| p.panel_id)
    }

    fn click(&mut self, h: dereth_ui::ElemHandle) {
        self.point_at(h);
        self.button(true);
        self.button(false);
        for _ in 0..3 {
            self.frame();
        }
    }

    fn finish(self) {
        let Self { app, mut shell } = self;
        app.shutdown(&mut shell);
    }
}

/// Behaviour: presentation.interface.a-switch-mid-press-leaves-no-press-behind
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn one_click_opens_a_panel_after_switching_away_mid_press_three_times() {
    let state = std::env::temp_dir().join(format!("dereth-switch-leak-{}", std::process::id()));
    let mut c = Client::new(&state);
    let toolbar = c.toolbar();
    let (pressed, _) = toolbar[0];
    let (target, panel) = toolbar[1];
    // The calibration: one click opens the panel and a second closes it, before any switch.
    assert_eq!(c.current_panel(), None);
    c.click(target);
    assert_eq!(c.current_panel(), Some(panel), "one click opens the panel");
    c.click(target);
    assert_eq!(c.current_panel(), None, "a second click closes it");
    for switch in 1..=3 {
        // The interface choice is made on a press, as the modern Options page's drop-down makes
        // it: the button is still down when the classic interface is shown, and comes up there.
        c.point_at(pressed);
        c.button(true);
        c.choose(Interface::Classic);
        c.button(false);
        c.choose(Interface::Modern);
        c.click(target);
        assert_eq!(
            c.current_panel(),
            Some(panel),
            "after switch {switch} one click opens the panel"
        );
        c.click(target);
        assert_eq!(
            c.current_panel(),
            None,
            "after switch {switch} one more click closes it"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// Behaviour: presentation.interface.a-switch-mid-press-leaves-no-press-behind
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn the_classic_interface_switched_away_from_mid_press_holds_no_press() {
    let state =
        std::env::temp_dir().join(format!("dereth-switch-leak-classic-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.choose(Interface::Classic);
    let holds = |c: &Client| {
        c.shell
            .classic
            .ui
            .as_ref()
            .expect("classic interface")
            .desktop
            .holds_pointer()
    };
    for switch in 1..=3 {
        c.send(HostEvent::CursorMoved { x: 400.0, y: 300.0 });
        c.button(true);
        assert!(holds(&c), "the press is held by a classic window");
        c.choose(Interface::Modern);
        assert!(
            !holds(&c),
            "after switch {switch} the classic interface holds no press"
        );
        c.button(false);
        c.choose(Interface::Classic);
        assert!(!holds(&c));
    }
    c.choose(Interface::Modern);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}
