//! Character Options rows reach their client-side consumers when toggled by pointer: Side By Side
//! Vitals swaps the stacked (`0x100005FA`) and side-by-side (`0x100006D5`) vitals windows; Display
//! Coordinates on Radar hides the read-out; Filter Language censors incoming speech through the
//! taboo table; Disable Most Weather Effects and Disable Distance Fog invert into the world's
//! weather and fog flags; Always Day re-lights the landscape from the regional lighting at 0.5.
//! Each toggle runs both directions from the shipped default words (off/on/off, or on/off/on for
//! coordinates), checking the hit target and checkbox/model agreement; the page's row count and
//! distinct option ordinals are pinned too.
//! Fixture: the retail dats and a headless `App` on the gameplay screen, logged in by a synthetic
//! socket-free peer, with a synthetic `0x0013` carrying the default option words.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;
use dereth_client::world::SceneReads;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::pump::Pump;
use dereth_client::world::SceneConfig;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::CommunicationHearSpeech;
use dereth_protocol::login::{LoginEnterGameServerReady, LoginPlayerDescription, PlayerModule};
use dereth_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::PositionWire;
use dereth_ui::{ElemHandle, UiSystem};
use dereth_ui_screens::options::character::CHARACTER_PAGE_ELEMENT;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use dereth_ui_screens::view::PlayerOption;
use winit::event::MouseButton;

/// Holtburg, the worked example throughout these suites.
const CELL_A: u32 = 0xA9B4_001D;
const POS_A: (f32, f32, f32) = (60.0, 80.0, 42.0);
const PLAYER_1: ObjectId = ObjectId(0x5000_0001);
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

/// The shipped default option words (options and options2) — the words a new character starts
/// from, and the reason four of the five environment and vitals toggles are a **tick**.
const DEFAULT_CHARACTER_OPTION: u32 = 0x50C4_A54A;
const DEFAULT_CHARACTER_OPTIONS2: u32 = 0x0094_8700;

/// Checkbox checked attribute 0x0E, inspected as UI state rather than rendered pixels.
const ATTR_CHECKED: u32 = 0x0E;

/// Night time 0.02 distinguishes the initial region lighting from Always Day's 0.5 query.
const NIGHT: f32 = 0.02;

// =============================================================================================
// The socket-free peer — copied in shape from `panels/radar_coordinates.rs`, which is the established
// way to put this client in the world without a socket.
// =============================================================================================

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "p165", "unused", 0)
            .expect("a socket-free network client");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("a literal address")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
            },
            net,
        )
    }

    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .expect("one fragment fits");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("the envelope serialises");
        app.replay_network_mut()
            .expect("an explicitly socket-free endpoint")
            .session
            .transport
            .feed(&raw, None, dereth_primitives::LocalTime(0.0))
            .expect("the transport accepts its own envelope");
    }
}

fn scene() -> SceneConfig {
    SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        // Night: see [`NIGHT`].
        time_of_day: Some(NIGHT),
        ..SceneConfig::default()
    }
}

fn player_create(id: ObjectId, cell: u32, xyz: (f32, f32, f32)) -> Vec<u8> {
    let mut p = ObjectCreatePayload {
        id,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= flags::SETUP | flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    p.physicsdesc.position = Some(PositionWire {
        objcell_id: cell,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: xyz.0,
                y: xyz.1,
                z: xyz.2,
            },
            ..Default::default()
        },
    });
    dereth_protocol::write_blob(&ItemCreateObject(p)).expect("the create encodes")
}

// =============================================================================================
// The bench: in the world, on the gameplay screen, with the options panel reachable by pointer
// =============================================================================================

struct Bench {
    app: App,
    #[allow(dead_code)]
    peer: Peer,
    pump: Pump,
    stamp: u32,
}

impl Bench {
    fn new() -> Self {
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
            client_dir().display()
        );
        let mut app = App::new(Config {
            headless: true,
            sound: false,
            ui: true,
            preferences_file: std::env::temp_dir()
                .join("dereth-character-options-not-created/prefs.ini"),
            dat_dir: client_dir(),
            ..Config::default()
        })
        .expect("the application comes up headless");
        app.start_shell().expect("the UI shell comes up");

        let (mut peer, net) = Peer::new();
        app.attach_replay_network(net)
            .expect("a headless App with no live link");
        app.defer_static_scene(scene());

        app.replay_network_mut()
            .expect("the endpoint")
            .enter_world(PLAYER_1, "p165");
        Self::run(&mut app, 1);
        peer.send(
            &mut app,
            UI_QUEUE,
            dereth_protocol::write_blob(&LoginEnterGameServerReady).expect("0xF7DF"),
        );
        Self::run(&mut app, 2);
        peer.send(
            &mut app,
            SMARTBOX_QUEUE,
            dereth_protocol::write_blob(&LoginCreatePlayer {
                player_id: PLAYER_1,
            })
            .expect("0xF746"),
        );
        peer.send(
            &mut app,
            SMARTBOX_QUEUE,
            player_create(PLAYER_1, CELL_A, POS_A),
        );
        Self::run(&mut app, 5);
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        Self::run(&mut app, 3);
        assert_eq!(
            app.probe_mut().objects_mut().world.player,
            Some(PLAYER_1),
            "the login put a body in"
        );

        // This synthetic login sequence has not supplied 0x0013. Install default option words
        // explicitly so hud::character_option has a description to read; this is not a claim
        // that the recorded corpus lacks player descriptions.
        app.apply_hud_events(&[SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: PlayerModule {
                    options: DEFAULT_CHARACTER_OPTION,
                    options2: DEFAULT_CHARACTER_OPTIONS2,
                    ..PlayerModule::default()
                },
                ..LoginPlayerDescription::default()
            },
        ))]);
        Self::run(&mut app, 4);

        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            app,
            peer,
            pump,
            stamp: 100_000,
        }
    }

    fn run(app: &mut App, n: u32) {
        for i in 0..n {
            assert!(app.frame(), "frame {i} must not end the client");
        }
    }

    fn frames(&mut self, n: u32) {
        Self::run(&mut self.app, n);
    }

    fn ui(&self) -> &UiSystem {
        &self.app.ui().expect("a UI shell").ui
    }

    fn with_screen<R>(&mut self, f: impl FnOnce(&mut UiSystem, &mut GamePlayScreen) -> R) -> R {
        let shell = self.app.ui_mut().expect("a UI shell");
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is current");
        f(&mut shell.ui, gameplay)
    }

    /// Construct normalized move/down/up messages at the element centre, dispatch them through
    /// Pump and the input manager, and first require the hit test to return that exact element.
    /// This exercises the pointer producer inside the application, not operating-system input.
    fn click(&mut self, h: ElemHandle) {
        let b = self.ui().screen_box(h);
        let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let hit = self.ui().hit_test_screen(x, y);
        assert_eq!(
            hit,
            Some(h),
            "the pointer at ({x}, {y}) must land on the element it aims at"
        );
        self.stamp += 100;
        let messages = [
            self.pump
                .mouse_move_message(f64::from(x), f64::from(y), self.stamp),
            self.pump
                .mouse_button_message(MouseButton::Left, true, self.stamp + 10)
                .unwrap(),
            self.pump
                .mouse_button_message(MouseButton::Left, false, self.stamp + 20)
                .unwrap(),
        ];
        for msg in messages {
            self.pump.dispatch(msg);
            self.app
                .input_manager_mut()
                .expect("real input maps")
                .on_message(msg);
        }
        // Allow three frames for gesture handling, request routing into the option word,
        // UI refresh and App::apply_player_option_effects to observe the changed bits.
        self.frames(3);
    }

    /// Click the toolbar panel and tab containing the Character Options page.
    fn open_character_options_tab(&mut self) {
        let (panel_id, container, page) = self.with_screen(|ui, s| {
            let page = s
                .character_options
                .page
                .expect("the character-settings panel bound its page");
            let info = s
                .panels
                .pages
                .iter()
                .copied()
                .find(|p| {
                    let mut h = Some(page);
                    while let Some(cur) = h {
                        if cur == p.handle {
                            return true;
                        }
                        h = ui.parent(cur);
                    }
                    false
                })
                .expect("the Character Options page sits inside a toolbar panel");
            (info.panel_id, info.handle, page)
        });
        let button = self.with_screen(|_, s| {
            s.toolbar
                .buttons
                .iter()
                .find(|b| b.panel_id == panel_id)
                .unwrap_or_else(|| panic!("the toolbar panel has no button for panel {panel_id}"))
                .handle
        });
        self.click(button);
        let tab = self.with_screen(|ui, _| {
            let tab = ui
                .node(container)
                .and_then(|n| {
                    n.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<dereth_ui::widgets::panel::Panel>()
                })
                .and_then(|p| p.page_to_tab.get(&CHARACTER_PAGE_ELEMENT).copied())
                .expect("the options panel's tab table names the Character Options page");
            ui.get_child_recursive(container, tab)
                .expect("the tab caption element")
        });
        self.click(tab);
        assert!(
            self.ui().is_visible(page),
            "the Character Options tab must be up"
        );
    }

    /// Scroll the named checkbox row into the list's viewport before attempting its centre.
    /// This calls the list's scroll_to_view directly; the subsequent toggle uses pointer input.
    /// A below-viewport row would otherwise test the viewport rather than the intended option.
    fn row(&mut self, option: PlayerOption) -> (usize, ElemHandle) {
        let (i, row_handle, element) = self.with_screen(|_, s| {
            let (i, r) = s
                .character_options
                .rows
                .iter()
                .enumerate()
                .find(|(_, r)| r.option == option)
                .unwrap_or_else(|| panic!("the options page has no {option:?} row"));
            (i, r.row, r.element)
        });
        self.with_screen(|ui, s| {
            let Some(list) = s.character_options.option_box.as_mut() else {
                return;
            };
            let Some(index) = list.items.iter().position(|h| *h == row_handle) else {
                return;
            };
            list.scroll_to_view(ui, index);
        });
        self.frames(2);
        (i, element)
    }

    /// Read checkbox attribute 0x0E; the helper name does not imply pixel readback.
    fn drawn(&self, h: ElemHandle) -> bool {
        dereth_ui_screens::bind::attr_bool(self.ui(), h, ATTR_CHECKED).unwrap_or(false)
    }

    /// Read the current World's option bit using the same public ordinal as the UI row.
    fn model(&self, option: PlayerOption) -> bool {
        self.app
            .objects()
            .world
            .player_system
            .options
            .get(dereth_client::hud::option_ordinal(option))
    }

    /// Toggle through pointer messages, requiring both checkbox attribute and model bit to
    /// move. Return the checkbox handle for later persistence checks.
    fn toggle(&mut self, option: PlayerOption, want: bool) -> ElemHandle {
        let (_, h) = self.row(option);
        assert_eq!(
            self.drawn(h),
            !want,
            "{option:?} must start at {} for this gesture to be a real {}",
            !want,
            if want { "tick" } else { "untick" }
        );
        assert_eq!(
            self.model(option),
            !want,
            "and the option word must agree before the click"
        );
        self.click(h);
        assert_eq!(
            self.drawn(h),
            want,
            "{option:?}: the check box did not follow the click"
        );
        assert_eq!(
            self.model(option),
            want,
            "{option:?}: checkbox-option application did not write the model bit"
        );
        h
    }

    fn visible(&self, id: dereth_ui::ElementId) -> bool {
        let root = self.ui().root();
        self.ui()
            .get_child_recursive(root, id)
            .is_some_and(|h| self.ui().is_visible(h))
    }

    /// Read the main chat text element's glyph characters, rather than the scroll model or pixels.
    fn drawn_chat(&mut self) -> String {
        let log = self
            .ui()
            .get_child_recursive(self.ui().root(), dereth_ui_screens::chat::window::LOG)
            .expect("the shipped gameplay layout has a main chat log");
        self.app
            .ui_mut()
            .expect("a UI shell")
            .ui
            .text_element_mut(log)
            .expect("the chat log is a text element")
            .glyphs
            .glyphs
            .iter()
            .map(|g| char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}'))
            .collect()
    }

    /// Encode synthetic speech and deliver it through the real replay-envelope decoder.
    fn hear_speech(&mut self, text: &str) {
        let blob = dereth_protocol::write_blob(&CommunicationHearSpeech {
            message: text.to_owned(),
            sender_name: "Filter witness".to_owned(),
            sender_id: ObjectId(0x5000_00F1),
            text_type: 1,
        })
        .expect("0x02BB speech encodes");
        self.peer.send(&mut self.app, UI_QUEUE, blob);
        self.frames(3);
    }
}

// =============================================================================================
// 1. Side By Side Vitals
// =============================================================================================

/// Behaviour: options.side-by-side-vitals.swaps-the-vitals-windows
///
/// Side By Side Vitals gives the stacked (0x100005FA) and side-by-side (0x100006D5) windows
/// opposite visibility, keeps it over 16 frames of refreshes, then restores them. Mapping the
/// option to no ordinal leaves `PlayerSettingsView::side_by_side_vitals` false and fails the
/// stacked-down assertions.
#[test]
fn side_by_side_vitals_shows_one_vitals_window_and_hides_the_other() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();
    b.open_character_options_tab();

    // The shipped word has bit 21 clear, so this is the **stacked** strip and a real tick.
    assert!(
        !b.model(PlayerOption::SideBySideVitals),
        "the default option word has bit 21 clear"
    );
    assert!(
        b.visible(window::STACKED_VITALS),
        "the stacked strip is the default"
    );
    assert!(
        !b.visible(window::SIDE_VITALS),
        "and the side-by-side window is down"
    );

    // ---- off -> on ---------------------------------------------------------------------------
    let h = b.toggle(PlayerOption::SideBySideVitals, true);
    assert!(
        !b.visible(window::STACKED_VITALS),
        "the stacked strip must be down once the option is on"
    );
    assert!(
        b.visible(window::SIDE_VITALS),
        "and the side-by-side window up"
    );

    // Hud::applied_key includes this option so later player-module refreshes must not
    // overwrite the selected visibility with an old value.
    b.frames(16);
    assert!(b.drawn(h), "the tick is still drawn 16 frames later");
    assert!(!b.visible(window::STACKED_VITALS));
    assert!(b.visible(window::SIDE_VITALS));

    // ---- and back ---------------------------------------------------------------------------
    b.toggle(PlayerOption::SideBySideVitals, false);
    assert!(
        b.visible(window::STACKED_VITALS),
        "the stacked strip is visible again"
    );
    assert!(
        !b.visible(window::SIDE_VITALS),
        "the side-by-side window is hidden again"
    );
}

// =============================================================================================
// 2. Display Coordinates on Radar
// =============================================================================================

/// Radar coordinates require both the enabled option and available player coordinates.
/// Retail hides four elements when either condition fails (it does not merely blank their text).
/// This test observes the combined strip and coordinate container: up means either handle is
/// visible, while down requires both absent/hidden; it does not assert all four elements.
/// Ordinal 20 is default-on, so initial visibility, off and restored-on states distinguish an
/// omitted gate from reversed polarity. Removing the player_coords filter fails the
/// read-out-down assertion.
#[test]
fn display_coordinates_on_radar_takes_the_read_out_down_and_brings_it_back() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();

    // Observe the combined strip and its coordinate container with the OR predicate below.
    let coords = b.with_screen(|_, s| (s.radar.combined_coords, s.radar.coordinate_container));
    let up = |b: &Bench| {
        coords.0.is_some_and(|h| b.ui().is_visible(h))
            || coords.1.is_some_and(|h| b.ui().is_visible(h))
    };

    assert!(
        b.model(PlayerOption::CoordinatesOnRadar),
        "ordinal 20 is default-on"
    );
    assert!(
        up(&b),
        "the shipped profile shows the read-out, and a wrong-polarity gate hides it"
    );

    b.open_character_options_tab();

    // ---- on -> off ---------------------------------------------------------------------------
    b.toggle(PlayerOption::CoordinatesOnRadar, false);
    assert!(!up(&b), "the read-out must be down with the option off");

    // ---- off -> on, which is the tick -------------------------------------------------------
    b.toggle(PlayerOption::CoordinatesOnRadar, true);
    assert!(up(&b), "and back up when the player ticks it again");
}

// =============================================================================================
// 3. Filter Language: the incoming-chat taboo-table consumer
// =============================================================================================

/// Toggle audience-1 taboo filtering on synthetic 0x02BB incoming speech. The word occurs in
/// shipped table 0x0E00001E and is a space-delimited middle token, matching retail's
/// word-splitting input shape. Read chat glyph characters after replay decoding; this is neither
/// recorded speech nor rendered-pixel inspection. Without the consumer, the model and checkbox
/// change but the log keeps "hello shit there" instead of "hello **** there".
#[test]
fn filter_language_censors_incoming_chat_and_can_be_turned_back_off() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();
    b.open_character_options_tab();

    assert!(
        !b.model(PlayerOption::FilterLanguage),
        "options2 bit 17 is clear by default"
    );
    b.toggle(PlayerOption::FilterLanguage, true);
    b.hear_speech("hello shit there");
    let filtered = b.drawn_chat();
    assert!(
        filtered.contains("hello **** there"),
        "the incoming line must pass through audience 1 of the retail taboo table; log={filtered:?}"
    );
    assert!(
        !filtered.contains("hello shit there"),
        "the original token must not remain"
    );

    b.toggle(PlayerOption::FilterLanguage, false);
    b.hear_speech("again shit again");
    let unfiltered = b.drawn_chat();
    assert!(
        unfiltered.contains("again shit again"),
        "unticking the option must bypass the table; log={unfiltered:?}"
    );
}

// =============================================================================================
// 4. Environment rows: option changes reach their live world-state consumers
// =============================================================================================

/// Disable Most Weather Effects inverts into the world's weather-enabled flag.
/// Sky-object creation tests that flag before constructing objects with properties mask 4.
/// This test checks the flag itself, not every reader or actual weather-object
/// creation/rendering. Returning early from option-effect application leaves Some(true) where
/// this test requires Some(false).
#[test]
fn disable_most_weather_effects_turns_the_weather_layer_off() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();
    b.open_character_options_tab();

    assert_eq!(
        b.app.world_scene().and_then(|w| w.weather_enabled()),
        Some(true),
        "the landscape weather flag is 1 during landscape initialization, and the shipped word has bit 16 clear"
    );
    assert!(
        !b.model(PlayerOption::DisableMostWeatherEffects),
        "so this is a tick"
    );

    b.toggle(PlayerOption::DisableMostWeatherEffects, true);
    assert_eq!(
        b.app.world_scene().and_then(|w| w.weather_enabled()),
        Some(false),
        "the weather layer must be off"
    );

    b.toggle(PlayerOption::DisableMostWeatherEffects, false);
    assert_eq!(
        b.app.world_scene().and_then(|w| w.weather_enabled()),
        Some(true),
        "and back on"
    );
}

/// Behaviour: options.distance-fog.turns-world-fog-off-and-back
///
/// Disable Distance Fog changes the world's fog-enabled state, then restores it.
/// Retail inverts the option into fog enablement and informs the device of user disablement
/// before bypassing world-fog calculation. This test reads world_fog_state.enabled, not device
/// commands or fog pixels. Returning early from option-effect application fails it.
#[test]
fn disable_distance_fog_takes_the_fog_off() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();
    b.open_character_options_tab();

    let fog_on = |b: &Bench| {
        b.app
            .world_scene()
            .map(|w| w.world_fog_state())
            .map(|f| f.enabled)
    };
    assert_eq!(fog_on(&b), Some(true), "distance fog is enabled initially");
    assert!(
        !b.model(PlayerOption::DisableDistanceFog),
        "options2 bit 21 is clear by default"
    );

    b.toggle(PlayerOption::DisableDistanceFog, true);
    assert_eq!(fog_on(&b), Some(false), "the world fog state is disabled");

    b.toggle(PlayerOption::DisableDistanceFog, false);
    assert_eq!(fog_on(&b), Some(true), "and back");
}

/// Always Day uses regional landscape lighting at 0.5 instead of the running clock time.
/// Option handling stores the flag and resets two timers for the next update. The lighting
/// reader replaces all four incoming lighting arguments with the regional noon query; it does
/// not force the whole scene clock, sky dome or weather to noon. The bench starts at 0.02 and
/// requires a different result. Bypassing the 0.5 branch in apply_lighting fails the
/// changed-lighting assertion.
#[test]
fn always_day_puts_the_landscape_on_noon_lighting() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();
    b.open_character_options_tab();

    let lighting = |b: &Bench| b.app.world_scene().map(|w| w.landscape_lighting());
    let colours = |b: &Bench| b.app.world_scene().and_then(|w| w.viewer_block_colours());
    let night = lighting(&b).expect("a landscape");
    let night_colours = colours(&b).expect("the viewer's block is built");
    assert!(
        !b.model(PlayerOption::PersistentAtDay),
        "options2 bit 0 is clear by default"
    );
    assert_eq!(
        b.app.world_scene().map(|w| w.always_daylight()),
        Some(false),
        "the always-daylight setting is 0 in the shipped .data"
    );

    b.toggle(PlayerOption::PersistentAtDay, true);
    let day = lighting(&b).expect("a landscape");
    assert_eq!(
        b.app.world_scene().map(|w| w.always_daylight()),
        Some(true),
        "setting daylight to 1"
    );
    assert_ne!(
        day, night,
        "Always Day must change the landscape lighting: lighting at 0.5 must differ from \
         lighting at {NIGHT}"
    );
    // And the landscape on screen must actually be re-lit with it. The lighting is baked into
    // the blocks' vertex colours, so the switch must re-bake the blocks already resident, not
    // only change the stored values, or the terrain keeps the night's light.
    b.frames(2);
    assert_ne!(
        colours(&b).expect("the viewer's block is built"),
        night_colours,
        "Always Day must re-light the blocks already on screen, not only blocks built later"
    );
    // Pin independently measured region ambient values rather than deriving the expected
    // value from the same selection flag. Noon ambient 0.35 is lower than night 0.3999922:
    // regional lighting also supplies directional sun color/vector, so ambient alone is not
    // total illumination. The retail ambient floor is 0.2. These checks retain the exact
    // 0.5/0.02 measurements and 1e-4 tolerances.
    assert!(
        (day.ambient_level - 0.35).abs() < 1e-4,
        "lighting at 0.5 has ambient_level 0.35 in this region; measured {}",
        day.ambient_level
    );
    assert!(
        (night.ambient_level - 0.399_992_2).abs() < 1e-4,
        "lighting at {NIGHT} has ambient_level 0.3999922; measured {}",
        night.ambient_level
    );

    b.toggle(PlayerOption::PersistentAtDay, false);
    let back = lighting(&b).expect("a landscape");
    assert_eq!(
        b.app.world_scene().map(|w| w.always_daylight()),
        Some(false),
        "setting daylight to 0"
    );
    assert_ne!(back, day, "unticking must leave the noon lighting");
    // Back to the clock's own hour. **Not `assert_eq!(back, night)`**: `GameClock` keeps running
    // while the stations click, so `present_time_of_day` has crept past the `0.02` the scene
    // started at and the ramp has moved a little with it. The tolerance is what says "this is the
    // night ramp again" without pretending the clock stood still.
    assert!(
        (back.ambient_level - night.ambient_level).abs() < 1e-2,
        "the clock's own hour again: {} vs {}",
        back.ambient_level,
        night.ambient_level
    );
}

// =============================================================================================
// 5. The page's row count, pinned
// =============================================================================================

/// Behaviour: options.character-options.the-page-has-fifty-rows-each-editing-its-own-setting
///
/// Check the built page's row count and the uniqueness of its rows' option ordinals. The count
/// is not taken from a constant table, so missing bound rows fail. The test does not click every
/// row or discover all effect consumers; it also pins the defaults of the five environment and
/// vitals rows.
#[test]
fn the_character_options_page_is_fifty_rows_and_each_edits_its_own_bit() {
    let _gpu = gpu_lock();
    let mut b = Bench::new();
    b.open_character_options_tab();

    let options: Vec<PlayerOption> =
        b.with_screen(|_, s| s.character_options.rows.iter().map(|r| r.option).collect());
    assert_eq!(
        options.len(),
        50,
        "the built character-options page contains 50 rows"
    );

    let mut ordinals: Vec<usize> = options
        .iter()
        .map(|o| dereth_client::hud::option_ordinal(*o))
        .collect();
    let n = ordinals.len();
    ordinals.sort_unstable();
    ordinals.dedup();
    assert_eq!(
        ordinals.len(),
        n,
        "no two rows edit the same PlayerOption ordinal"
    );

    // The five initial rows are present with their intended starting polarities: four off,
    // coordinates on. The separate FilterLanguage station checks its own default-off premise.
    for (o, want) in [
        (PlayerOption::SideBySideVitals, false),
        (PlayerOption::DisableMostWeatherEffects, false),
        (PlayerOption::DisableDistanceFog, false),
        (PlayerOption::PersistentAtDay, false),
        (PlayerOption::CoordinatesOnRadar, true),
    ] {
        assert!(options.contains(&o), "{o:?} is on the options page");
        assert_eq!(
            b.model(o),
            want,
            "{o:?} in the default option word -- if this moved, the toggles above changed shape"
        );
    }
}
