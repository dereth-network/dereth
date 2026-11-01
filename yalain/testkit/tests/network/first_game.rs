//! A player of the first game: its own client, the runtime's application with no window, device
//! or interface (as its headless client runs it with `--world`), logged on to a world over UDP,
//! acting through the lines it types into the chat bar and showing what its chat windows would.
//! The world is built with no device, so the character has a body: the client shows no speech, a
//! tell included, to a player without one.

use std::path::Path;
use std::time::Instant;

use dereth_client_sdk::contract::pregame::{CharGenAction, CharGenResultData};
use dereth_client_sdk::contract::UiRequest;
use dereth_client_sdk::net::client_session::SessionState;
use dereth_client_sdk::runtime::app::{App, Platform, HEADLESS_STEP};
use dereth_client_sdk::runtime::config::Config;
use dereth_client_sdk::runtime::present::Presentation;
use dereth_client_sdk::runtime::scene::SceneConfig;
use dereth_client_sdk::runtime::shell::NullShell;
use dereth_client_sdk::runtime::sim_present::SimPresentation;

/// The most frames one step runs to catch up with the clock.
const CATCH_UP: u32 = 4;

pub struct Player {
    pub name: &'static str,
    app: Option<App<NullShell>>,
    shell: NullShell,
    start: Instant,
    frames: u64,
    /// Every line its chat windows took, in order.
    pub chat: Vec<String>,
    /// Asked to create its character, and the creation's answers then.
    asked: Option<u32>,
    /// Asked to enter the world.
    entering: bool,
}

impl Player {
    /// The client, logging on to the world at `port` on loopback as an account of the player's
    /// name, its preferences kept in `dir`.
    pub fn connect(name: &'static str, port: u16, dir: &Path) -> Self {
        let cfg = Config {
            headless: true,
            frames: None,
            connect: true,
            sound: false,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: dir.join(format!("{name}.ini")),
            host: "127.0.0.1".to_owned(),
            port: u32::from(port),
            account: name.to_lowercase(),
            account_as_typed: name.to_lowercase(),
            vg_password: "network".to_owned(),
            width: 800,
            height: 600,
            ..Config::default()
        };
        let mut app = App::<NullShell>::bring_up(
            cfg,
            |_| Ok(Platform::headless(800, 600)),
            |_, _, _, _| Ok(Box::new(SimPresentation::new(800, 600)) as Box<dyn Presentation>),
        )
        .expect("the first game's client starts");
        // Built on world entry, at the character's own place.
        app.defer_static_scene(SceneConfig {
            character: true,
            ..SceneConfig::default()
        });
        let mut shell = NullShell;
        app.start_shell(&mut shell).expect("its front end starts");
        Self {
            name,
            app: Some(app),
            shell,
            start: Instant::now(),
            frames: 0,
            chat: Vec::new(),
            asked: None,
            entering: false,
        }
    }

    fn app(&mut self) -> &mut App<NullShell> {
        self.app.as_mut().expect("the client is running")
    }

    /// The frames the clock has come to, a few at most; the chat's new lines kept.
    pub fn step(&mut self) {
        let Some(app) = self.app.as_mut() else {
            return;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let due = (self.start.elapsed().as_secs_f64() / HEADLESS_STEP) as u64;
        let mut n = 0;
        while self.frames < due && n < CATCH_UP {
            // The portal tunnel's time, which a front end's interface would tick: it tells the
            // server the character has arrived.
            app.teleport_use_time(&self.shell);
            assert!(
                app.frame(&mut self.shell),
                "{}'s client shut down: {:?}",
                self.name,
                app.network_mut().map(|n| n.session_state())
            );
            self.frames += 1;
            n += 1;
        }
        for (_, m) in std::mem::take(&mut app.hud.pending_chat) {
            let line = format!("{}{}", m.prefix.unwrap_or_default(), m.body);
            self.chat.push(line.trim_end().to_owned());
        }
    }

    /// Moves its login on: once the character list is up, creates the character (an Aluvian
    /// soldier) if the account has none, then enters the world as it. Whether it stands in the
    /// world.
    pub fn enter(&mut self) -> bool {
        let (name, entering) = (self.name, self.entering);
        let app = self.app.as_mut().expect("the client is running");
        if app
            .network_mut()
            .is_some_and(|n| n.session_state() == SessionState::Playable)
            && app.objects.player().is_some()
        {
            return true;
        }
        if !app.host_state.received_set || entering {
            return false;
        }
        let found = app.network_mut().and_then(|n| {
            n.characters()
                .characters
                .iter()
                .find(|c| c.name.trim_start_matches('+').eq_ignore_ascii_case(name))
                .map(|c| c.gid)
        });
        match (found, self.asked) {
            (Some(gid), _) => {
                self.app()
                    .run_chargen_actions(vec![CharGenAction::LogOn(gid)]);
                self.entering = true;
            }
            (None, None) => {
                let notices = self.app().host_state.chargen_response_notices;
                self.app()
                    .run_chargen_actions(vec![CharGenAction::SendCharGenResult(Box::new(
                        soldier(name),
                    ))]);
                self.asked = Some(notices);
            }
            (None, Some(before)) => {
                let app = self.app();
                // Answered, and still not listed: refused.
                assert!(
                    app.host_state.chargen_response_notices == before,
                    "{name} was not created: {:?}",
                    app.host_state.chargen_response
                );
            }
        }
        false
    }

    /// A line typed into the chat bar.
    pub fn type_line(&mut self, text: &str) {
        self.chat.push(format!("> {text}"));
        self.app().submit_requests(vec![UiRequest::ChatLine {
            text: text.to_owned(),
            window: 0,
        }]);
    }

    /// Whether a chat line since `from` holds `needle`.
    pub fn shown_since(&self, from: usize, needle: &str) -> bool {
        self.chat[from.min(self.chat.len())..]
            .iter()
            .any(|l| !l.starts_with("> ") && l.contains(needle))
    }

    /// Whether its friends list shows `name`, online or not.
    pub fn friend(&self, name: &str) -> Option<bool> {
        let app = self.app.as_ref()?;
        app.objects()
            .world
            .friends()
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .map(|f| f.online)
    }

    /// Logs out and shuts the client down, as a player closing it does.
    pub fn quit(&mut self) {
        if let Some(app) = self.app.take() {
            let _ = app.shutdown(&mut self.shell);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.quit();
    }
}

/// A character to create: an Aluvian soldier, as the headless client makes one.
fn soldier(name: &str) -> CharGenResultData {
    let mut skills = vec![0; 55];
    for skill in [44usize, 6, 21, 22, 24] {
        skills[skill] = 2;
    }
    CharGenResultData {
        heritage_group: 1,
        gender: 1,
        eyes_strip: 0,
        nose_strip: 0,
        mouth_strip: 0,
        hair_color: 0,
        eye_color: 0,
        hair_style: 0,
        headgear_style: 0,
        headgear_color: 3,
        shirt_style: 0,
        shirt_color: 4,
        trousers_style: 0,
        trousers_color: 5,
        footwear_style: 0,
        footwear_color: 6,
        skin_shade: 1.0,
        hair_shade: 0.5,
        headgear_shade: 0.25,
        shirt_shade: 0.125,
        trousers_shade: 0.375,
        footwear_shade: 0.625,
        template_num: 6,
        strength: 100,
        endurance: 60,
        coordination: 100,
        quickness: 50,
        focus: 10,
        self_: 10,
        slot: 0,
        class_id: 1,
        skill_advancement_classes: skills,
        name: name.to_owned(),
        start_area: 0,
        is_admin: 0,
        is_envoy: 0,
    }
}
