//! Typed chat commands: every command the client handles reaches the wire (or its local effect)
//! from a line typed in the chat entry, and none of them is answered with "That is not a valid
//! command.".
//!
//! `dereth_client_model::cmd::table::INITIALIZE_COMMANDS` and `TURBINE_CHAT_COMMANDS` together
//! register the client's command words. The families asserted here:
//!
//! ```text
//! character  @lifestone @lif @ls  @marketplace @mar @mp  @hor @hr  @hom @hoa
//!            @pkarena @pka  @pklarena @pla  @pklite @pkl  @age  @birth  @die
//! comms      @chat  @notell  @index  @clist  @on  @off  @afk
//! consent    @consent  @permit
//! speech     @say @s
//! local      @speaker  @endurance  @emotes  @clear  @corpse
//! layout     @saveui @loadui (and the automatic pair)  @lockui  @title
//! squelch    @squelch @unsquelch  @filter @unfilter  @messagetypes
//! other      @fillcomps  @log  @loadfile  @version
//! ```
//!
//! The oracle for each wire body is written longhand here (a `u16` `PString` count, the
//! characters, padding to four; a raw dword otherwise), so the expectation never calls the encoder
//! under test. Refusals are read off the screen: each prints its own sentence on chat type `0x1A`
//! and sends nothing.
//!
//! The typed stations drive the production input path with normalized `Pump` messages: a pointer
//! press on the main chat entry (`0x10000016`) through hit-testing, one ASCII `WM_CHAR` per
//! character, then `VK_RETURN`. A test that hands `UiRequest::ChatLine` straight to the
//! interaction cannot tell a wired command from one whose entry box the player cannot reach.
//!
//! Fixture: a headless App with no link. The primary table compares the request the app routed
//! with its expected value, then feeds that expected value to a `Session` backed by
//! `MockTransport` to check its bytes; no datagram leaves the process. The corpus station reads
//! the three `fellowship-*` recordings; the vendor stations replay `long-solo-play`. The commands
//! that change what is drawn (`@render`, `@day`, `@framerate`) are
//! `rendering::render_and_day_commands`.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::gpu_lock;

use dereth_client::interaction::send_request;
use dereth_client::{
    app::App,
    config::Config,
    pump::{Pump, Win32Message},
};
use dereth_client_model::chat_cmd as cmd;
use dereth_client_model::Request;
use dereth_client_net::client_session::{
    testing::{Corpus, Direction, MockTransport},
    Session, SessionEvent,
};
use dereth_primitives::{LocalTime, NetQueue, ObjectId};
use dereth_ui::{framework::mode, ElemHandle, ElementId};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// The main-chat text entry.
const CHAT_ENTRY: ElementId = ElementId(0x1000_0016);
/// The speech-bubble list, the only surface whose shipped filter accepts chat type `0x1A`.
const SPEW_LIST: ElementId = dereth_ui_screens::hud::speech_bubbles::LIST_BOX;

pub(crate) fn app() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir().join("dereth-chat-commands/preferences.ini"),
        ..Config::default()
    })
    .expect("required retail DATs and headless graphics device");
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..4 {
        assert!(app.frame());
    }
    app
}

/// The glyph text of every live element in the speech-bubble list, in list order; this does not
/// inspect rendered pixels.
pub(crate) fn bubbles(app: &mut App) -> Vec<String> {
    let list = {
        let shell = app.ui().expect("shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
        shell
            .ui
            .get_child_recursive(screen.root().expect("root"), SPEW_LIST)
            .expect("the spew box list is in the shipped layout")
    };
    let shell = app.ui_mut().expect("shell");
    let kids = shell.ui.children(list);
    kids.into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// The main chat window's log glyphs — the visible destination for type-zero local lines.
pub(crate) fn chat_log(app: &mut App) -> String {
    let log = {
        let shell = app.ui().expect("shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
        shell
            .ui
            .get_child_recursive(
                screen.root().expect("root"),
                dereth_ui_screens::chat::window::LOG,
            )
            .expect("the main chat log is in the shipped layout")
    };
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(log)
        .expect("the chat log is a text element")
        .glyphs
        .inq_text(false)
}

fn element_text(app: &mut App, element: ElemHandle) -> String {
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(element)
        .expect("the shipped element is text")
        .glyphs
        .inq_text(false)
}

/// Make the first authored popup reachable without using the command under test. Returns its
/// real entry, caption and log elements plus the native window id.
fn first_popup(app: &mut App) -> (ElemHandle, ElemHandle, ElemHandle, u32) {
    let shell = app.ui_mut().expect("shell");
    let current = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **current;
    let screen = any.downcast_mut::<GamePlayScreen>().expect("gameplay");
    let popup = screen
        .floaty_chat
        .first()
        .expect("four shipped floaty chats");
    let root = popup.root.expect("the first popup root is bound");
    let title = popup.title_text.expect("the popup title is bound");
    let entry = screen
        .chat_windows
        .get(1)
        .and_then(|w| w.entry)
        .expect("popup entry");
    let log = screen
        .chat_windows
        .get(1)
        .and_then(|w| w.log)
        .expect("popup log");
    let window = popup.window_id;
    shell.ui.move_to(root, 120, 90);
    shell.ui.set_visible(root, true);
    assert!(shell.ui.node(root).is_some_and(|n| n.region.flags.visible));
    assert!(shell.ui.node(entry).is_some_and(|n| n.region.flags.visible));
    (entry, title, log, window)
}

fn module_chat_title(
    module: &dereth_protocol::login::PlayerModule,
    window: u32,
) -> Option<dereth_protocol::property::StringInfo> {
    use dereth_protocol::property::BasePropertyValue as V;
    let options = module.gameplay_options.as_ref()?;
    let V::Array(rows) = options.properties.get(0x1000_008C)? else {
        return None;
    };
    let V::Struct(row) = rows.get(window.checked_sub(1)? as usize)?.value.as_ref()? else {
        return None;
    };
    let V::StringInfo(title) = row.get(0x1000_008D)? else {
        return None;
    };
    Some(title.clone())
}

fn stored_chat_title(app: &App, window: u32) -> Option<dereth_protocol::property::StringInfo> {
    module_chat_title(app.objects().world.player_system.module.as_ref()?, window)
}

pub(crate) fn gameplay_element(app: &App, id: ElementId) -> dereth_ui::ElemHandle {
    let shell = app.ui().expect("shell");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
    let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
    shell
        .ui
        .get_child_recursive(screen.root().expect("root"), id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped gameplay tree"))
}

fn screen_rect(app: &App, id: ElementId) -> ((i32, i32), (i32, i32)) {
    let shell = app.ui().expect("shell");
    let h = gameplay_element(app, id);
    let bounds = shell.ui.screen_box(h);
    (shell.ui.screen_origin(h), (bounds.width(), bounds.height()))
}

pub(crate) fn install_player_module(app: &mut App) -> dereth_protocol::login::PlayerModule {
    let module = dereth_protocol::login::PlayerModule::default();
    app.hud_mut().player_module = Some(module.clone());
    app.objects_mut()
        .world
        .player_system
        .apply_player_module(&module);
    module
}

pub(crate) fn forced_saved_player_module(app: &mut App) -> dereth_protocol::login::PlayerModule {
    let mut requests = dereth_client_model::RecordingRequests::default();
    assert!(
        app.objects_mut()
            .world
            .player_system
            .save_to_server(&mut requests, true),
        "the forced save path has an authoritative module"
    );
    let [Request::CharacterOptionsEvent(saved)] = requests.0.as_slice() else {
        panic!("forced save produced {:?}", requests.0)
    };
    saved.module.clone()
}

fn assert_lock_wire(request: &Request, locked: bool) {
    let mut session = Session::new(MockTransport::new());
    assert!(
        send_request(&mut session, request),
        "the production sender owns 0x0005"
    );
    let packet = session
        .transport
        .sent
        .last()
        .expect("one lock option datagram");
    assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
    assert_eq!(
        packet.payload,
        action(1, 0x0005, &[51, 0, 0, 0, u8::from(locked), 0, 0, 0]),
        "LockUI_PlayerOption is ordinal 51 followed by its C++ bool dword"
    );
}

/// Replay long-solo-play through the general merchant's `0x0062`, including the real player module,
/// component inventory, component catalogue installation and live vendor-window open path.
fn open_recorded_component_vendor(app: &mut App) {
    const PLAYER: ObjectId = ObjectId(0x5000_000A);
    const VENDOR_INFO_AT: usize = 4736;
    app.objects_mut().world.player = Some(PLAYER);
    let corpus = Corpus::shared("long-solo-play");
    for row in corpus
        .blobs
        .iter()
        .filter(|row| row.dir == Direction::ServerToClient && row.idx <= VENDOR_INFO_AT)
    {
        let ui_queue = dereth_protocol::Opcode(row.opcode)
            .info()
            .is_some_and(|info| info.recv_queue == Some(NetQueue::UiQueue));
        let event = match row.opcode {
            0xf7b0 if row.payload[12..16] == 0x13_u32.to_le_bytes() => {
                SessionEvent::PlayerDescription(Box::new(
                    dereth_protocol::read_body(&row.payload[16..])
                        .expect("recorded player description"),
                ))
            }
            0xf7b0 => SessionEvent::UiEvent {
                opcode: dereth_protocol::Opcode(u32::from_le_bytes(
                    row.payload[12..16].try_into().expect("UI sub-opcode"),
                )),
                blob: row.payload[12..].to_vec(),
            },
            0xf746 => SessionEvent::PlayerCreated(ObjectId(u32::from_le_bytes(
                row.payload[4..8].try_into().expect("player id"),
            ))),
            _ if ui_queue => SessionEvent::UiEvent {
                opcode: dereth_protocol::Opcode(row.opcode),
                blob: row.payload.clone(),
            },
            _ => SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
        };
        let now = LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64());
        app.objects_mut().apply_event(&event, now);
        app.apply_hud_events(std::slice::from_ref(&event));
        app.apply_interaction_events(std::slice::from_ref(&event));
    }
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(
        app.objects().world.shop.vendor_id,
        Some(ObjectId(0x77F0_3064))
    );
    assert!(
        app.hud().panels.vendor.visible,
        "the recorded 0x0062 opened the live vendor panel"
    );
}

// ---------------------------------------------------------------------------------------------
// The byte oracle, longhand
// ---------------------------------------------------------------------------------------------

/// A packed string — a `u16` count, the characters, zero padding to four.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&u16::try_from(s.len()).expect("short").to_le_bytes());
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

fn dw(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// Ordered-action framing: the `0xF7B1` header, the `OrderedActionHeader` stamp, the opcode, and
/// the body.
fn action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

// ---------------------------------------------------------------------------------------------
// The keyboard and pointer, driven as Windows drives them
// ---------------------------------------------------------------------------------------------

pub(crate) struct Hand {
    pump: Pump,
    time: u32,
}

impl Hand {
    pub(crate) fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time: 400_000,
        }
    }

    pub(crate) fn send(&mut self, app: &mut App, message: Win32Message) {
        self.pump.dispatch(message);
        app.input_manager_mut()
            .expect("the input shell exists in a UI build")
            .on_message(message);
    }

    pub(crate) fn click(&mut self, app: &mut App, id: ElementId) {
        let ((x, y), (width, height)) = screen_rect(app, id);
        self.time += 10;
        let message = self.pump.mouse_move_message(
            f64::from(x + width / 2),
            f64::from(y + height / 2),
            self.time,
        );
        self.send(app, message);
        for down in [true, false] {
            self.time += 10;
            let message = self
                .pump
                .mouse_button_message(winit::event::MouseButton::Left, down, self.time)
                .expect("the left button is in the 0x201 block");
            self.send(app, message);
        }
        assert!(app.frame());
    }

    /// Click the chat entry, type the line one `WM_CHAR` at a time, then `VK_RETURN`.
    pub(crate) fn submit(&mut self, app: &mut App, text: &str) {
        let entry = {
            let shell = app.ui().expect("shell");
            let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
            let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
            shell
                .ui
                .get_child_recursive(screen.root().expect("root"), CHAT_ENTRY)
                .expect("the chat entry is in the shipped layout")
        };
        self.submit_to(app, entry, text);
    }

    /// The same physical input journey through one particular popup's entry.
    pub(crate) fn submit_to(&mut self, app: &mut App, entry: ElemHandle, text: &str) {
        let bounds = app.ui().expect("shell").ui.screen_box(entry);
        self.time += 10;
        let message = self.pump.mouse_move_message(
            f64::from((bounds.x0 + bounds.x1) / 2),
            f64::from((bounds.y0 + bounds.y1) / 2),
            self.time,
        );
        self.send(app, message);
        for down in [true, false] {
            self.time += 10;
            let message = self
                .pump
                .mouse_button_message(winit::event::MouseButton::Left, down, self.time)
                .expect("the left button is in the 0x201 block");
            self.send(app, message);
        }
        assert!(app.frame());
        for byte in text.bytes() {
            assert!(
                byte.is_ascii(),
                "this station is deliberately ASCII: {text:?}"
            );
            self.time += 10;
            self.send(
                app,
                Win32Message::new(
                    dereth_input::win32::msg::WM_CHAR,
                    usize::from(byte),
                    0,
                    self.time,
                ),
            );
        }
        assert!(app.frame());
        for (msg, lp) in [
            (dereth_input::win32::msg::WM_KEYDOWN, 0x001c_0001_u32),
            (dereth_input::win32::msg::WM_KEYUP, 0xc01c_0001_u32),
        ] {
            self.time += 10;
            self.send(app, Win32Message::new(msg, 13, lp as isize, self.time));
        }
        assert!(app.frame());
    }
}

/// The opcode each `Request` this family builds carries, spelled out here rather than read off
/// `dereth_protocol::Message::OPCODE` so the table and the codec cannot drift together.
fn opcode_of(r: &Request) -> u32 {
    match r {
        Request::PlayerOptionChanged(_) => 0x0005,
        Request::SetAfkMode(_) => 0x000F,
        Request::SetAfkMessage(_) => 0x0010,
        Request::TeleToPklArena(_) => 0x0026,
        Request::TeleToPkArena(_) => 0x0027,
        Request::ModifyGlobalSquelch(_) => 0x005B,
        Request::ModifyCharacterSquelch(_) => 0x0058,
        Request::ModifyAccountSquelch(_) => 0x0059,
        Request::TeleToLifestone(_) => 0x0063,
        Request::AddToChannel(_) => 0x0145,
        Request::RemoveFromChannel(_) => 0x0146,
        Request::ChannelList(_) => 0x0148,
        Request::ChannelIndex(_) => 0x0149,
        Request::QueryAge(_) => 0x01C2,
        Request::QueryBirth(_) => 0x01C4,
        Request::ClearPlayerConsentList(_) => 0x0216,
        Request::DisplayPlayerConsentList(_) => 0x0217,
        Request::RemoveFromPlayerConsentList(_) => 0x0218,
        Request::AddPlayerPermission(_) => 0x0219,
        Request::RemovePlayerPermission(_) => 0x021A,
        Request::TeleToHouse(_) => 0x0262,
        Request::TeleToMansion(_) => 0x0278,
        Request::Suicide(_) => 0x0279,
        Request::TeleToMarketplace(_) => 0x028D,
        Request::EnterPkLite(_) => 0x028F,
        other => panic!("unexpected request {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The acceptance — every wired command, typed
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.commands.every-wired-command-reaches-the-wire-from-a-typed-line
///
/// Each typed line puts exactly the expected request in the outbox, byte for byte, and
/// `chat_commands_unimplemented` does not move: that counter is where a line that fell to the
/// catch-all lands, so a regression shows here rather than as a silent change of behaviour.
#[test]
fn every_wired_chat_command_reaches_the_wire_from_a_typed_line() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());

    // (typed line, the request the arm must build, the body bytes on the wire)
    let cases: Vec<(&str, Request, Vec<u8>)> = vec![
        // ---- character teleports: three names each, one opcode each ------------------------
        (
            "@lifestone",
            Request::TeleToLifestone(dereth_protocol::combat::CharacterTeleToLifestone),
            Vec::new(),
        ),
        (
            "@ls",
            Request::TeleToLifestone(dereth_protocol::combat::CharacterTeleToLifestone),
            Vec::new(),
        ),
        (
            "@marketplace",
            Request::TeleToMarketplace(dereth_protocol::combat::CharacterTeleToMarketplace),
            Vec::new(),
        ),
        (
            "@mp",
            Request::TeleToMarketplace(dereth_protocol::combat::CharacterTeleToMarketplace),
            Vec::new(),
        ),
        (
            "@hor",
            Request::TeleToHouse(dereth_protocol::trade::HouseTeleToHouse),
            Vec::new(),
        ),
        (
            "@hr",
            Request::TeleToHouse(dereth_protocol::trade::HouseTeleToHouse),
            Vec::new(),
        ),
        (
            "@hom",
            Request::TeleToMansion(dereth_protocol::trade::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@hoa",
            Request::TeleToMansion(dereth_protocol::trade::HouseTeleToMansion),
            Vec::new(),
        ),
        // With no player object, lookup misses and all three arena aliases reach the same send arm.
        (
            "@pkarena",
            Request::TeleToPkArena(dereth_protocol::combat::CharacterTeleToPkArena),
            Vec::new(),
        ),
        (
            "@pklarena",
            Request::TeleToPklArena(dereth_protocol::combat::CharacterTeleToPklArena),
            Vec::new(),
        ),
        (
            "@pklite",
            Request::EnterPkLite(dereth_protocol::combat::CharacterEnterPkLite),
            Vec::new(),
        ),
        // ---- the two personal queries, both with target 0 ----------------------------------
        (
            "@age",
            Request::QueryAge(dereth_protocol::admin::CharacterQueryAge {
                target: ObjectId(0),
            }),
            dw(0),
        ),
        (
            "@birth",
            Request::QueryBirth(dereth_protocol::admin::CharacterQueryBirth {
                target: ObjectId(0),
            }),
            dw(0),
        ),
        // ---- the two global-squelch toggles ------------------------------------------------
        (
            "@chat on",
            Request::ModifyGlobalSquelch(
                dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 0,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_CHAT,
                },
            ),
            [dw(0), dw(cmd::GLOBAL_SQUELCH_TYPE_CHAT)].concat(),
        ),
        (
            "@chat off",
            Request::ModifyGlobalSquelch(
                dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 1,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_CHAT,
                },
            ),
            [dw(1), dw(cmd::GLOBAL_SQUELCH_TYPE_CHAT)].concat(),
        ),
        (
            "@notell off",
            Request::ModifyGlobalSquelch(
                dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 0,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_TELL,
                },
            ),
            [dw(0), dw(cmd::GLOBAL_SQUELCH_TYPE_TELL)].concat(),
        ),
        // Leave-chat and join-chat consume only the first argument. The general-chat option write
        // reports ordinal 35 to the option-change path, whose auto-save arm emits one 0x0005
        // immediately.
        (
            "@leave General ignored",
            Request::PlayerOptionChanged(
                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: 35,
                    value: 0,
                },
            ),
            [dw(35), dw(0)].concat(),
        ),
        (
            "@join general ignored",
            Request::PlayerOptionChanged(
                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: 35,
                    value: 1,
                },
            ),
            [dw(35), dw(1)].concat(),
        ),
        // The no-tell parser has no unknown-word refusal, unlike the chat-toggle parser:
        // anything but "off" is "on".
        (
            "@notell wibble",
            Request::ModifyGlobalSquelch(
                dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 1,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_TELL,
                },
            ),
            [dw(1), dw(cmd::GLOBAL_SQUELCH_TYPE_TELL)].concat(),
        ),
        // ---- the four channel commands -----------------------------------------------------
        (
            "@index",
            Request::ChannelIndex(dereth_protocol::comms::CommunicationChannelIndexRequest),
            Vec::new(),
        ),
        // The channel-routing command maps `fellowship-one-vassal` to `0x800`, not `0x2000`:
        // `0x2000` is `patron` in this chain, and the `0x2000` the command reference calls
        // "fellowship" belongs to talk-focus routing, which is a different table. The handler
        // reads this one.
        (
            "@clist fellowship",
            Request::ChannelList(dereth_protocol::comms::CommunicationChannelListRequest {
                channel: 0x800,
            }),
            dw(0x800),
        ),
        (
            "@on fellowship",
            Request::AddToChannel(dereth_protocol::comms::CommunicationAddToChannel {
                channel: 0x800,
            }),
            dw(0x800),
        ),
        (
            "@off patron",
            Request::RemoveFromChannel(dereth_protocol::comms::CommunicationRemoveFromChannel {
                channel: 0x2000,
            }),
            dw(0x2000),
        ),
        // ---- @afk: the bare form is the `on` arm, and `msg` truncates and newline-terminates -
        (
            "@afk",
            Request::SetAfkMode(dereth_protocol::comms::CommunicationSetAfkMode { afk: 1 }),
            dw(1),
        ),
        (
            "@afk msg back soon",
            Request::SetAfkMessage(dereth_protocol::comms::CommunicationSetAfkMessage {
                message: "back soon\n".into(),
            }),
            pstr("back soon\n"),
        ),
        // ---- consent and permit ------------------------------------------------------------
        (
            "@consent who",
            Request::DisplayPlayerConsentList(
                dereth_protocol::admin::CharacterDisplayPlayerConsentList,
            ),
            Vec::new(),
        ),
        (
            "@consent clear",
            Request::ClearPlayerConsentList(
                dereth_protocol::admin::CharacterClearPlayerConsentList,
            ),
            Vec::new(),
        ),
        (
            "@consent remove Bob",
            Request::RemoveFromPlayerConsentList(
                dereth_protocol::admin::CharacterRemoveFromPlayerConsentList { name: "Bob".into() },
            ),
            pstr("Bob"),
        ),
        (
            "@permit add Bob",
            Request::AddPlayerPermission(dereth_protocol::admin::CharacterAddPlayerPermission {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        // Name joining keeps the interior run of spaces and strips a leading `+`.
        (
            "@permit remove +Bob Smith",
            Request::RemovePlayerPermission(
                dereth_protocol::admin::CharacterRemovePlayerPermission {
                    name: "Bob Smith".into(),
                },
            ),
            pstr("Bob Smith"),
        ),
    ];

    let mut stamp = 1u32;
    for (line, want, body) in &cases {
        let unimplemented = app.interaction().stats.chat_commands_unimplemented;
        let refused = app.interaction().stats.chat_commands_refused;
        let lines = app.interaction().stats.chat_lines_sent;
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().stats.chat_lines_sent,
            lines + 1,
            "{line}: the keystrokes must reach UiRequest::ChatLine before anything else is judged"
        );
        assert_eq!(
            app.interaction().stats.chat_commands_unimplemented,
            unimplemented,
            "{line}: this is the counter a line that falls to the catch-all lands in"
        );
        assert_eq!(
            app.interaction().stats.chat_commands_refused,
            refused,
            "{line}: a wired command prints no refusal"
        );
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            std::slice::from_ref(want),
            "{line}: exactly this request and nothing else"
        );
        assert!(
            send_request(&mut session, want),
            "{line}: the sender has an arm"
        );
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per request");
        assert_eq!(
            (packet.queue, packet.ordered),
            (NetQueue::Weenie, true),
            "{line}: `dereth_protocol::opcodes` puts every one of these C2S rows on the Weenie queue \
             as an ordered game action"
        );
        assert_eq!(
            packet.payload,
            action(stamp, opcode_of(want), body),
            "{line}: the bytes"
        );
        stamp += 1;
        assert!(app.frame());
        assert!(
            app.interaction().last_sent.is_empty(),
            "{line}: the next frame never repeats it"
        );
    }

    // The census: every distinct opcode the table above sends, plus join/leave's player-option
    // write.
    let mut opcodes: Vec<u32> = session
        .transport
        .sent
        .iter()
        .map(|p| u32::from_le_bytes(p.payload[8..12].try_into().expect("an opcode dword")))
        .collect();
    opcodes.sort_unstable();
    opcodes.dedup();
    assert_eq!(
        opcodes,
        vec![
            0x0005, 0x000F, 0x0010, 0x0026, 0x0027, 0x005B, 0x0063, 0x0145, 0x0146, 0x0148, 0x0149,
            0x01C2, 0x01C4, 0x0216, 0x0217, 0x0218, 0x0219, 0x021A, 0x0262, 0x0278, 0x028D, 0x028F,
        ],
        "twenty-two here; 0x0279 Suicide is behind the @die dialog and has its own test"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The refusals, read off the screen rather than off a counter
// ---------------------------------------------------------------------------------------------

/// Every refusal prints the client's own sentence and puts nothing in the outbox, and none of
/// them is "That is not a valid command.", because every handler here returns `true`.
///
/// The one exception is `@afk wibble`: its fall-through returns `false`, so it is the only line in
/// the family followed by the ordinary `0x26` invalid-command notice.
#[test]
fn the_refusals_print_retails_sentence_and_send_nothing() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();

    let cases: Vec<(&str, &str)> = vec![
        // Shape A: argc must be zero. Each has its own literal and they are not interchangeable.
        ("@lifestone now", cmd::PLEASE_SEE_HELP_LIFESTONE),
        ("@marketplace now", cmd::PLEASE_SEE_HELP_MARKETPLACE),
        ("@pkarena now", cmd::PLEASE_SEE_HELP_PKARENA),
        ("@pklarena now", cmd::PLEASE_SEE_HELP_PKLARENA),
        ("@pklite now", cmd::PLEASE_SEE_HELP_PKLITE),
        // One literal shared by both house recalls, and the one with no trailing full stop.
        ("@hor now", cmd::PLEASE_SEE_HELP_HOUSE),
        ("@hom now", cmd::PLEASE_SEE_HELP_HOUSE),
        ("@die now", cmd::PLEASE_SEE_HELP_DIE),
        // Shape B: argc must be exactly one.
        ("@chat", cmd::SPECIFY_CHAT_ON_OR_OFF),
        ("@chat on off", cmd::SPECIFY_CHAT_ON_OR_OFF),
        ("@chat wibble", cmd::SPECIFY_ON_OR_OFF),
        ("@notell", cmd::SPECIFY_TELLS_ON_OR_OFF),
        ("@clist", cmd::SPECIFY_THE_CHANNEL_NAME),
        ("@on", cmd::SPECIFY_THE_CHANNEL_NAME),
        ("@off", cmd::SPECIFY_THE_CHANNEL_NAME),
        // The channel-not-found notice uses a different literal and window from the two
        // above it, and reached only when the word is present and unknown.
        ("@on nosuchchannel", cmd::THAT_CHANNEL_DOES_NOT_EXIST),
        // Shape C: the sub-command chains.
        ("@consent list", cmd::SPECIFY_VALID_CONSENT_COMMAND),
        (
            "@consent remove",
            cmd::SPECIFY_PERSON_TO_REMOVE_FROM_CONSENT,
        ),
        ("@permit wibble Bob", cmd::SPECIFY_VALID_PERMIT_COMMAND),
        ("@permit add", cmd::SPECIFY_PERSON_FOR_PERMIT),
        // The say parser trims with `whitespace_string` first and then refuses; the emote parser
        // does neither.
        ("@say   ", cmd::YOU_MUST_SPECIFY_TEXT_TO_SAY),
    ];

    for (line, want) in &cases {
        hand.submit(&mut app, line);
        assert!(
            app.interaction().last_sent.is_empty(),
            "{line}: a refusal puts nothing on the wire"
        );
        // Every refusal in this family is chat type `0x1A`, and the speech-bubble strip is the only
        // surface whose shipped filter accepts it, so the observable is the element tree and not a
        // counter. Three frames allow the notice, scroll, HUD-event and element update stages to
        // settle; the station does not assign one stage to each frame.
        for _ in 0..3 {
            assert!(app.frame());
        }
        let drawn = bubbles(&mut app);
        assert!(
            drawn.iter().any(|t| t.trim() == want.trim()),
            "{line}: expected {want:?} in the bubble strip, got {drawn:?}"
        );
        assert!(
            !drawn
                .iter()
                .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim()),
            "{line}: these handlers all return true, so the dispatcher adds nothing"
        );
        assert!(app.frame());
    }

    // The one arm that really is `false`: the AFK parser's fall-through prints nothing of its own.
    let refused = app.interaction().stats.chat_commands_refused;
    hand.submit(&mut app, "@afk wibble");
    assert!(app.interaction().last_sent.is_empty());
    assert_eq!(
        app.interaction().stats.chat_commands_refused,
        refused + 1,
        "the AFK fall-through is the one line here that reaches the ordinary invalid-command failure"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let drawn = bubbles(&mut app);
    assert!(
        drawn
            .iter()
            .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim()),
        "and the dispatcher's own sentence is what it prints, got {drawn:?}"
    );

    // Join/leave take one channel token; missing and unknown tokens return false,
    // so the dispatcher supplies the same generic failure and no option write escapes.
    for line in [
        "@join",
        "@join nosuchchannel",
        "@leave",
        "@leave nosuchchannel",
    ] {
        let refused = app.interaction().stats.chat_commands_refused;
        let unimplemented = app.interaction().stats.chat_commands_unimplemented;
        hand.submit(&mut app, line);
        assert!(
            app.interaction().last_sent.is_empty(),
            "{line}: invalid means no 0x0005"
        );
        assert_eq!(
            app.interaction().stats.chat_commands_refused,
            refused + 1,
            "{line}"
        );
        assert_eq!(
            app.interaction().stats.chat_commands_unimplemented,
            unimplemented,
            "{line}: a false return from a wired handler is not an unimplemented handler"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 3. The four local handlers, and the option write
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard
///
/// `@speaker`, `@endurance`, `@emotes` and `@clear` send nothing and each does its own local
/// thing; `@emotes` prints `emote_list_text`.
#[test]
fn the_local_commands_print_their_literal_and_send_nothing() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();

    let unimplemented = app.interaction().stats.chat_commands_unimplemented;
    for line in ["@speaker", "@endurance", "@emotes", "@clear", "@clear all"] {
        hand.submit(&mut app, line);
        assert!(app.interaction().last_sent.is_empty(), "{line}: local only");
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented,
        "none of the five reaches the catch-all any more"
    );
    assert_eq!(
        app.interaction().stats.chat_command_lines,
        3,
        "@speaker, @endurance and @emotes each print one line; the two @clears print none"
    );
    // Clear the command's own window first, then window 0 for "all".
    assert_eq!(
        app.interaction().chat_clears_pending(),
        &[1, 0],
        "`all` adds window 0 after clearing the command's own window"
    );

    // `@consent on|off` writes the accept-loot-permits option, then reports option ordinal 16.
    // Ordinal 16 is an auto-save option, so it leaves as a `0x0005` immediately.
    let sent = app.interaction().stats.option_changes_sent;
    hand.submit(&mut app, "@consent on");
    assert_eq!(app.interaction().stats.option_changes_sent, sent + 1);
    assert_eq!(
        app.interaction().last_sent.as_slice(),
        &[Request::PlayerOptionChanged(
            dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                option: u32::try_from(cmd::ACCEPT_LOOT_PERMITS_ORDINAL).unwrap(),
                value: 1,
            }
        )],
        "the option write and nothing else"
    );
    assert!(app.frame());
    // The equality guard makes a second `on` a no-op.
    hand.submit(&mut app, "@consent on");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the equality guard means a repeated value sends nothing"
    );
}

/// `@corpse` is local: it asks the player description for position quality `0x0E`,
/// formats only that position's outdoor cell id, and puts no game action on the wire.
///
/// Both local coordinates subtract `0x400`, multiply by `0.1`, add `0.5`, take the absolute
/// value, and print N/S before E/W with `"%.1f%s, %.1f%s"`; both add `0xFFFFFC00`.
#[test]
fn corpse_reads_position_quality_0e_from_a_typed_line_and_sends_nothing() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();

    // With no player description, the local corpse command declines the line and the dispatcher
    // supplies its ordinary failure text.
    let unimplemented = app.interaction().stats.chat_commands_unimplemented;
    hand.submit(&mut app, "@corpse");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the command is local on every path"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented,
        "a wired handler failure is not an unimplemented handler"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|text| text.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim()),
        "the corpse command returns false when the player description cannot be resolved"
    );

    // A real player description without 0x0E gets the apology, not the generic command failure.
    app.objects_mut().world.seed_player_desc(
        ObjectId(0x5000_000A),
        dereth_client_model::Qualities::default(),
    );
    hand.submit(&mut app, "@cor");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the alias is local too"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let log = chat_log(&mut app);
    assert!(
        log.contains("We're sorry, but we have no record of your last outside corpse location."),
        "missing position quality 0x0E prints retail's exact local line; log was {log:?}"
    );

    // 0xA9B4002A -> local coordinates (1357, 1441) -> 42.2N, 33.8E.
    let qualities = app
        .objects_mut()
        .world
        .player_qualities_mut()
        .expect("the seeded player descriptor");
    qualities
        .positions
        .get_or_insert_with(Default::default)
        .insert(
            0x0E,
            dereth_protocol::types::PositionWire {
                objcell_id: 0xA9B4_002A,
                ..Default::default()
            },
        );
    hand.submit(&mut app, "@corpse ignored arguments");
    assert!(app.interaction().last_sent.is_empty());
    for _ in 0..3 {
        assert!(app.frame());
    }
    let log = chat_log(&mut app);
    assert!(
        log.contains("The last time you died outside, your corpse was located at (42.2N, 33.8E)."),
        "`@corpse` ignores argc/argv and formats only the cached cell id; log was {log:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. `@die` asks before it kills
// ---------------------------------------------------------------------------------------------

/// `@die` puts a question on screen and nothing on the wire; only the Yes button sends
/// `0x0279`.
///
/// The dialog is the client's own (no `0x0274` asked it and no `0x0275` answers it), so a No is
/// silence.
#[test]
fn die_asks_first_and_only_yes_sends_the_suicide() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();

    hand.submit(&mut app, "@die");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the question, and not the message"
    );
    assert_eq!(app.interaction().stats.die_confirmations_raised, 1);
    assert!(app.frame());
    let prompt = dialog_prompt(&mut app);
    assert_eq!(
        prompt.as_deref().map(str::trim),
        Some(cmd::DIE_CONFIRMATION),
        "property 0xC5 supplies the expected confirmation prompt"
    );

    // A negative dialog answer reads property `0x92` and returns without sending anything.
    //
    // The observable is the **counter** and not `last_sent`, because `last_sent` is what the last
    // frame handed the wire slot and answering a dialog takes six frames -- a request made on the
    // click frame would be gone by the time the click has settled.
    let sent = app.interaction().stats.chat_command_requests;
    answer_dialog(&mut app, &mut hand, false);
    assert_eq!(
        app.interaction().stats.chat_command_requests,
        sent,
        "a No returns without sending the suicide request"
    );

    hand.submit(&mut app, "@die");
    assert_eq!(app.interaction().stats.die_confirmations_raised, 2);
    assert!(app.frame());
    answer_dialog(&mut app, &mut hand, true);
    assert_eq!(
        app.interaction().stats.chat_command_requests,
        sent + 1,
        "0x0279, and only on Yes"
    );

    // And the wire.
    let mut session = Session::new(MockTransport::new());
    let r = Request::Suicide(dereth_protocol::combat::CharacterSuicide);
    assert!(send_request(&mut session, &r));
    let packet = session.transport.sent.last().expect("one datagram");
    assert_eq!(packet.payload, action(1, 0x0279, &[]));
    assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
}

/// Save and load run through the player's real chat entry and the gameplay screen's `UI-*.txt`
/// serializer. The only shared state is the disposable file:
/// the live window is deliberately changed between Save and Load.
#[test]
fn typed_saveui_and_loadui_round_trip_the_visible_layout_file() {
    let _gpu = gpu_lock();
    let dir = std::env::temp_dir().join("dereth-chat-commands");
    std::fs::create_dir_all(&dir).expect("create this test's disposable directory");
    let mut app = app();
    let mut hand = Hand::new();
    const NAME: &str = "p36layout";
    let path = dir.join(format!("{NAME}.txt"));
    if path.exists() {
        std::fs::remove_file(&path).expect("remove this test's old disposable layout");
    }

    let chat = gameplay_element(&app, dereth_ui::persist::WINDOWS[1].element);
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.resize_to(chat, 321, 173);
        ui.move_to(chat, 37, 51);
    }
    assert!(app.frame());
    assert_eq!(
        screen_rect(&app, dereth_ui::persist::WINDOWS[1].element),
        ((37, 51), (321, 173))
    );

    hand.submit(&mut app, &format!("/saveui {NAME}"));
    let bytes = std::fs::read(&path).expect("the typed command wrote its named layout");
    assert!(
        !bytes.contains(&b'\n') && !bytes.contains(&b'\r'),
        "retail writes one line"
    );
    assert_eq!(
        bytes.last(),
        Some(&b' '),
        "the final row retains its trailing space"
    );
    let text = String::from_utf8(bytes).expect("the native layout format is ASCII");
    assert_eq!(
        text.matches(" X:").count(),
        16,
        "all sixteen native window rows are present"
    );
    assert!(
        text.contains("<CHAT> X:37 Y: 51 W: 321 H: 173 "),
        "the visible rect is serialized"
    );

    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.resize_to(chat, 350, 111);
        ui.move_to(chat, 9, 13);
    }
    assert!(app.frame());
    assert_eq!(
        screen_rect(&app, dereth_ui::persist::WINDOWS[1].element),
        ((9, 13), (350, 111))
    );

    hand.submit(&mut app, &format!("/loadui {NAME}"));
    assert_eq!(
        screen_rect(&app, dereth_ui::persist::WINDOWS[1].element),
        ((37, 51), (321, 173)),
        "loading the screen layout resizes then moves the actual visible chat window"
    );

    std::fs::remove_file(&path).expect("clean up this test's disposable layout");

    // No argv means the empty name and therefore `UI-Default.txt`: the plain `/saveui` and
    // `/loadui`, not just the optional named variant above.
    let default_path = dir.join("UI-Default.txt");
    if default_path.exists() {
        std::fs::remove_file(&default_path)
            .expect("remove this test's old disposable default layout");
    }
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.resize_to(chat, 330, 150);
        ui.move_to(chat, 61, 67);
    }
    assert!(app.frame());
    hand.submit(&mut app, "/saveui");
    assert!(
        default_path.is_file(),
        "an empty name selects the native default path"
    );
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.resize_to(chat, 370, 120);
        ui.move_to(chat, 3, 5);
    }
    assert!(app.frame());
    hand.submit(&mut app, "/loadui");
    assert_eq!(
        screen_rect(&app, dereth_ui::persist::WINDOWS[1].element),
        ((61, 67), (330, 150)),
        "the no-argument pair restores the actual visible window from UI-Default.txt"
    );
    std::fs::remove_file(&default_path).expect("clean up this test's disposable default layout");

    // A missing file is the ordinary first-run result: handled, silent and non-mutating.
    const FIFTEEN: &str = "abcdefghijklmno";
    let missing_path = dir.join(format!("{FIFTEEN}.txt"));
    if missing_path.exists() {
        std::fs::remove_file(&missing_path).expect("remove this test's stale missing-file fixture");
    }
    let before_lines = app.interaction().stats.chat_command_lines;
    let before_refused = app.interaction().stats.chat_commands_refused;
    hand.submit(&mut app, &format!("/loadui {FIFTEEN}"));
    assert_eq!(
        screen_rect(&app, dereth_ui::persist::WINDOWS[1].element),
        ((61, 67), (330, 150)),
        "fifteen visible characters pass the native boundary; a missing file changes nothing"
    );
    assert_eq!(app.interaction().stats.chat_command_lines, before_lines);
    assert_eq!(
        app.interaction().stats.chat_commands_refused,
        before_refused
    );

    // A name which passes the 16-character gate can still be an invalid Windows path. The save
    // notice has no user-facing failure line; failed I/O must not become the generic command
    // error either.
    let saves = app.ui().expect("shell").stats.screen_layout_saves;
    hand.submit(&mut app, "/saveui bad<name");
    assert_eq!(app.ui().expect("shell").stats.screen_layout_saves, saves);
    assert_eq!(app.interaction().stats.chat_command_lines, before_lines);
    assert_eq!(
        app.interaction().stats.chat_commands_refused,
        before_refused
    );

    // Both command spellings reach the handler. Its two validation branches return TRUE after
    // printing their own exact type-0x1A line, so no generic error may follow either one.
    hand.submit(&mut app, "@saveui abcdefghijklmnop");
    hand.submit(&mut app, "/loadui one two");
    hand.submit(&mut app, "@saveautoui unexpected");
    assert_eq!(app.interaction().stats.chat_command_lines, before_lines + 3);
    assert_eq!(
        app.interaction().stats.chat_commands_refused,
        before_refused + 3
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let drawn = bubbles(&mut app);
    for expected in [
        "The file name must be 16 characters or less.",
        "Please use @help loadui for proper usage.",
        "Please use @help saveautoui for proper usage.",
    ] {
        assert!(
            drawn.iter().any(|line| line.trim() == expected),
            "expected {expected:?} in the visible bubble strip, got {drawn:?}"
        );
    }
    assert!(
        !drawn
            .iter()
            .any(|line| line.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND),
        "all three validation paths return TRUE"
    );

    // The automatic siblings are the same two notices with the literal `#auto` name.
    app.host_state_mut().entered_character = Some("Kupo".to_owned());
    app.host_state_mut().world_name = Some("Frostfell".to_owned());
    let auto_path = dir.join("UI-Kupo-Frostfell-600-800.txt");
    if auto_path.exists() {
        std::fs::remove_file(&auto_path).expect("remove this test's old disposable auto layout");
    }
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.resize_to(chat, 300, 160);
        ui.move_to(chat, 45, 57);
    }
    assert!(app.frame());
    hand.submit(&mut app, "@saveautoui");
    assert!(
        auto_path.is_file(),
        "the `#auto` notice resolved the identity-based native path"
    );
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.resize_to(chat, 340, 105);
        ui.move_to(chat, 7, 11);
    }
    assert!(app.frame());
    hand.submit(&mut app, "/loadautoui");
    assert_eq!(
        screen_rect(&app, dereth_ui::persist::WINDOWS[1].element),
        ((45, 57), (300, 160)),
        "the automatic load uses the same real gameplay receiver"
    );
    std::fs::remove_file(&auto_path).expect("clean up this test's disposable auto layout");
}

/// The dialog factory's default queue, the one every callback dialog in the current UI lands on.
const DIALOG_QUEUE: u64 = dereth_ui::dialog::factory::DEFAULT_QUEUE;
/// The shipped Confirmation root's two buttons.
const YES: ElementId = ElementId(0x17);
const NO: ElementId = ElementId(0x19);

fn dialog_root(app: &App) -> Option<dereth_ui::ElemHandle> {
    app.ui()?.ui.dialogs.open_on(DIALOG_QUEUE)?.element
}

/// What the prompt line actually says — the text element's glyphs, not the property we put in.
fn dialog_prompt(app: &mut App) -> Option<String> {
    let root = dialog_root(app)?;
    let ui = &mut app.ui_mut()?.ui;
    let h = ui.get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)?;
    ui.text_element_mut(h).map(|t| t.glyphs.inq_text(false))
}

/// Press the open dialog's Yes or No with a real click, through hit-testing.
fn answer_dialog(app: &mut App, hand: &mut Hand, yes: bool) {
    let root = dialog_root(app).expect("an open confirmation dialog to answer");
    let h = app
        .ui()
        .expect("the shell")
        .ui
        .get_child_recursive(root, if yes { YES } else { NO })
        .expect("the shipped Confirmation root carries both buttons");
    let at = {
        let ui = &app.ui().expect("the shell").ui;
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid(), "a real visible control clip");
        ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
    };
    {
        let ui = &app.ui().expect("the shell").ui;
        assert!(
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
            "the button is the thing under the cursor"
        );
    }
    hand.time += 10;
    let msg = hand
        .pump
        .mouse_move_message(f64::from(at.0), f64::from(at.1), hand.time);
    hand.send(app, msg);
    for down in [true, false] {
        hand.time += 10;
        let msg = hand
            .pump
            .mouse_button_message(winit::event::MouseButton::Left, down, hand.time)
            .expect("a button");
        hand.send(app, msg);
    }
    for _ in 0..6 {
        app.frame();
    }
}

// ---------------------------------------------------------------------------------------------
// 5. The corpus, pointed at the target and reporting an honest zero
// ---------------------------------------------------------------------------------------------

/// None of the typed commands' opcodes appears in the selected recordings, which is why the
/// bodies above are composed longhand rather than taken from a capture.
///
/// An empty result is not a negative result until the instrument is proved to be pointed at the
/// target, so this walks every decodable client-to-server `0xF7B1` in each available named
/// scenario, asserts that their aggregate exceeds one hundred actions, and prints the opcode
/// histogram of what it did find. Missing recordings and payloads shorter than twelve bytes do
/// not enter that aggregate.
#[test]
fn the_corpus_records_none_of_these() {
    let mut total = 0usize;
    let mut hits = 0usize;
    let wanted: Vec<u32> = vec![
        0x000F, 0x0010, 0x0026, 0x0027, 0x005B, 0x0063, 0x0145, 0x0146, 0x0148, 0x0149, 0x01C2,
        0x01C4, 0x0216, 0x0217, 0x0218, 0x0219, 0x021A, 0x0262, 0x0278, 0x0279, 0x028D, 0x028F,
    ];
    let mut seen: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for scenario in [
        "fellowship-one-vassal",
        "fellowship-two-monarch",
        "fellowship-three-vassal",
    ] {
        let corpus = Corpus::shared(scenario);
        for blob in &corpus.blobs {
            if blob.dir != Direction::ClientToServer || blob.opcode != 0xf7b1 {
                continue;
            }
            if blob.payload.len() < 12 {
                continue;
            }
            total += 1;
            let op = u32::from_le_bytes(blob.payload[8..12].try_into().expect("dword"));
            *seen.entry(op).or_default() += 1;
            if wanted.contains(&op) {
                hits += 1;
            }
        }
    }
    assert!(
        total > 100,
        "the instrument must be proved to be looking: only {total} client game actions found"
    );
    println!(
        "{total} client 0xF7B1 game actions across three recordings, {} distinct opcodes",
        seen.len()
    );
    assert_eq!(
        hits, 0,
        "the selected corpus must contain no instances of these twenty-two opcodes"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. The static registry-to-WIRED allowlist comparison
// ---------------------------------------------------------------------------------------------

/// The lock-UI command writes option ordinal 51, then broadcasts the same `0x0D` as the radar
/// padlock. Actual
/// typed commands must reach both the visible cascade and the authoritative option sender.
#[test]
fn typed_lockui_toggles_the_padlock_and_sends_the_authoritative_option() {
    use dereth_ui_screens::mapradar::radar::{child, lock_state};
    const LOCK: u32 = 0x0100_0000;
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let before = install_player_module(&mut app);
    for _ in 0..3 {
        app.frame();
    }
    let lock = gameplay_element(&app, child::LOCK_BUTTON);
    let drag = gameplay_element(&app, child::DRAG_BUTTON);
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;
    let picture = |app: &App| {
        let node = app.ui().expect("shell").ui.node(lock).expect("lock");
        node.region.image.as_ref().map(|image| image.did)
    };
    let open_picture = picture(&app);
    assert!(!app.hud().lock_ui(), "the ordinary fixture starts unlocked");

    for (line, locked) in [("@lockui", true), ("/lockui", false)] {
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            &[Request::PlayerOptionChanged(
                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: 51,
                    value: u32::from(locked),
                },
            )],
            "{line} sends one auto-save option change for the UI-lock state"
        );
        assert_lock_wire(&app.interaction().last_sent[0], locked);
        for _ in 0..3 {
            app.frame();
            assert!(
                app.interaction().last_sent.is_empty(),
                "the equality guard prevents repeats"
            );
        }
        assert_eq!(app.hud().lock_ui(), locked, "{line} reaches the HUD mirror");
        let saved = forced_saved_player_module(&mut app);
        assert_eq!(
            saved.options, before.options,
            "{line} preserves the first option word"
        );
        assert_eq!(
            saved.options2 & !LOCK,
            before.options2 & !LOCK,
            "{line} preserves every neighboring options2 bit"
        );
        assert_eq!(
            saved.options2 & LOCK != 0,
            locked,
            "{line} reaches the authoritative saved module"
        );
        let shell = app.ui().expect("shell");
        assert_eq!(
            shell
                .ui
                .node(drag)
                .expect("drag handle")
                .region
                .flags
                .visible,
            !locked
        );
        let node = shell.ui.node(lock).expect("padlock");
        let expected = if locked {
            lock_state::LOCKED
        } else {
            lock_state::UNLOCKED
        };
        let state = node
            .desc
            .access_state(dereth_ui::StateId(expected))
            .expect("shipped lock state");
        let expected_image = match state.media.first().map(|m| &m.fields) {
            Some(dereth_ui::desc::state_desc::MediaFields::Image { file, .. }) => Some(*file),
            other => panic!("the lock state must have an image: {other:?}"),
        };
        assert_eq!(
            picture(&app),
            expected_image,
            "the real padlock follows the global lock notice"
        );
        if locked {
            assert_ne!(picture(&app), open_picture, "the closed picture differs");
        }
        assert!(
            app.interaction().outbox().is_empty(),
            "the one immediate option change was flushed and no duplicate remains"
        );
    }
    hand.submit(&mut app, "@lockui on");
    assert!(app.interaction().last_sent.is_empty());
    for _ in 0..3 {
        app.frame();
    }
    assert!(
        !app.hud().lock_ui(),
        "extra arguments are refused, not interpreted as a setting"
    );
    assert!(bubbles(&mut app)
        .iter()
        .any(|s| s == "Please use @help lockui for proper usage."));
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    assert!(app.interaction().outbox().is_empty());
    app.shutdown();
}

/// The radar button is the second physical producer of the lock-UI option. Its real pointer
/// path must reach the same ordinal-51 write, wire message, persisted module, and equality guard as
/// the typed command above.
#[test]
fn clicking_the_padlock_sends_one_authoritative_option_change_in_each_direction() {
    use dereth_ui_screens::mapradar::radar::child;
    const LOCK: u32 = 0x0100_0000;
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let before = install_player_module(&mut app);
    for _ in 0..3 {
        app.frame();
    }

    for locked in [true, false] {
        hand.click(&mut app, child::LOCK_BUTTON);
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            &[Request::PlayerOptionChanged(
                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: 51,
                    value: u32::from(locked),
                },
            )],
            "one physical click sends one UI-lock option change"
        );
        assert_lock_wire(&app.interaction().last_sent[0], locked);
        for _ in 0..2 {
            app.frame();
        }
        assert_eq!(
            app.hud().lock_ui(),
            locked,
            "the HUD mirror follows the authoritative bit"
        );
        assert!(
            app.interaction().last_sent.is_empty(),
            "later frames do not repeat the edge"
        );
        let saved = forced_saved_player_module(&mut app);
        assert_eq!(
            saved.options, before.options,
            "the first option word is untouched"
        );
        assert_eq!(saved.options2 & !LOCK, before.options2 & !LOCK);
        assert_eq!(
            saved.options2 & LOCK != 0,
            locked,
            "the authoritative saved module sees the physical toggle"
        );
    }
    app.shutdown();
}

/// Squelch and unsquelch share one argument parser. The request split is preserved: ordinary names use `0x0058`
/// with object zero and a message type, while `-account` uses the shorter `0x0059` body.
#[test]
fn typed_squelch_commands_send_the_native_character_and_account_requests() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;

    let cases = [
        (
            "@squelch Baron Lark",
            Request::ModifyCharacterSquelch(
                dereth_protocol::comms::CommunicationModifyCharacterSquelch {
                    add: 1,
                    character_id: ObjectId(0),
                    character_name: "Baron Lark".into(),
                    msg_type: 1,
                },
            ),
            [dw(1), dw(0), pstr("Baron Lark"), dw(1)].concat(),
        ),
        (
            "/unsquelch -Assessment +Baron Lark",
            Request::ModifyCharacterSquelch(
                dereth_protocol::comms::CommunicationModifyCharacterSquelch {
                    add: 0,
                    character_id: ObjectId(0),
                    character_name: "Baron Lark".into(),
                    msg_type: 16,
                },
            ),
            [dw(0), dw(0), pstr("Baron Lark"), dw(16)].concat(),
        ),
        (
            "@squelch -AcCoUnT Baron Lark",
            Request::ModifyAccountSquelch(
                dereth_protocol::comms::CommunicationModifyAccountSquelch {
                    add: 1,
                    character_name: "Baron Lark".into(),
                },
            ),
            [dw(1), pstr("Baron Lark")].concat(),
        ),
    ];
    for (index, (line, expected, body)) in cases.into_iter().enumerate() {
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            &[expected.clone()],
            "{line}"
        );
        assert!(
            send_request(&mut session, &expected),
            "{line} has a production sender"
        );
        let packet = session.transport.sent.last().expect("one encoded request");
        assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
        assert_eq!(
            packet.payload,
            action(
                u32::try_from(index + 1).expect("three cases"),
                opcode_of(&expected),
                &body
            ),
            "{line}"
        );
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    app.shutdown();
}

/// The native parser has two deliberately asymmetric refusal paths: an unknown dash-word returns
/// false and sends nothing, but a missing `-reply` target prints its warning and still sends the
/// empty-name request.  No argv is not an error at all; it prints the local squelch listing.
#[test]
fn squelch_parser_preserves_native_diagnostics_reply_and_no_argument_query() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;

    hand.submit(&mut app, "@squelch -Wibble Baron Lark");
    assert!(
        app.interaction().last_sent.is_empty(),
        "an unknown category returns false"
    );
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|s| s == "\"Wibble\" is not a valid squelch category."),
        "the exact squelch-argument diagnostic is visible"
    );

    hand.submit(&mut app, "/unsquelch -reply");
    assert_eq!(
        app.interaction().last_sent.as_slice(),
        &[Request::ModifyCharacterSquelch(
            dereth_protocol::comms::CommunicationModifyCharacterSquelch {
                add: 0,
                character_id: ObjectId(0),
                character_name: String::new(),
                msg_type: 1,
            },
        )],
        "retail warns but still sends after -reply has no last teller"
    );
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(bubbles(&mut app).iter().any(|s| {
        s == "A player must @tell you before you can squelch them with this command."
    }));

    app.objects_mut().world.chat.last_teller_name = "Baron Lark".into();
    hand.submit(&mut app, "@squelch -reply -account");
    assert_eq!(
        app.interaction().last_sent.as_slice(),
        &[Request::ModifyAccountSquelch(
            dereth_protocol::comms::CommunicationModifyAccountSquelch {
                add: 1,
                character_name: "Baron Lark".into(),
            },
        )],
        "-reply supplies the native target and later leading flags still apply"
    );

    hand.submit(&mut app, "/squelch");
    assert!(app.interaction().last_sent.is_empty(), "the query is local");
    for _ in 0..3 {
        assert!(app.frame());
    }
    let log = chat_log(&mut app);
    assert!(
        log.contains(
            "(account) denotes a character whose account has also been squelched.\n\
Format: Name : List of squelched message types.\n\
--------\n\
none"
        ),
        "the empty local DB prints the native query header and sentinel; log was {log:?}"
    );

    app.objects_mut().world.chat.squelch.characters.insert(
        ObjectId(10),
        dereth_client_model::chat::SquelchEntry {
            types: [
                dereth_client_model::chat::text_type::SPEECH_DIRECT,
                dereth_client_model::chat::text_type::COMBAT,
            ]
            .into_iter()
            .collect(),
            is_zone_squelch: 1,
            name: "Baron Lark".into(),
        },
    );
    let mut all = dereth_client_model::chat::SquelchEntry {
        name: "Allie".into(),
        ..Default::default()
    };
    all.squelch_everything();
    app.objects_mut()
        .world
        .chat
        .squelch
        .characters
        .insert(ObjectId(20), all);
    app.objects_mut()
        .world
        .chat
        .squelch
        .accounts
        .insert("Account Hash Only".into(), 1);
    hand.submit(&mut app, "@unsquelch");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the populated query is also local"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let log = chat_log(&mut app);
    assert!(
        log.contains("  Name: Baron Lark (account)  Tell, Combat\n"),
        "the name/account prefix and legal-channel order are literal; log was {log:?}"
    );
    assert!(
        log.lines().any(|line| line == "  Name: Allie All message types"),
        "a complete 128-bit mask takes the native All-message-types branch; the scroll trims the final newline; log was {log:?}"
    );
    assert!(
        !log.contains("Account Hash Only"),
        "the squelch query walks the character entries only"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    app.shutdown();
}

/// Filter and unfilter share the global-squelch modifier. They reuse the squelch category mapper but send the
/// global `0x005B` body: one add/remove dword followed by the selected `LogTextType`.
#[test]
fn typed_filter_commands_send_literal_global_squelch_requests_without_predicting_state() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;

    let cases = [
        (
            "@filter -Spellcasting",
            Request::ModifyGlobalSquelch(
                dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 1,
                    msg_type: dereth_client_model::chat::text_type::SPELLCASTING,
                },
            ),
            dereth_client_model::chat::text_type::SPELLCASTING,
        ),
        (
            "/unfilter -Assessment",
            Request::ModifyGlobalSquelch(
                dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 0,
                    msg_type: dereth_client_model::chat::text_type::APPRAISAL,
                },
            ),
            dereth_client_model::chat::text_type::APPRAISAL,
        ),
    ];
    for (index, (line, expected, ty)) in cases.into_iter().enumerate() {
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            &[expected.clone()],
            "{line}"
        );
        assert!(
            app.objects().world.chat.squelch.global.is_empty(),
            "the request waits for authoritative 0x01F4 rather than predicting the filter"
        );
        assert!(send_request(&mut session, &expected));
        let packet = session.transport.sent.last().expect("one encoded request");
        assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
        let add = u32::from(line.starts_with("@filter"));
        assert_eq!(
            packet.payload,
            action(
                u32::try_from(index + 1).expect("two cases"),
                0x005B,
                &[dw(add), dw(ty)].concat()
            ),
            "{line} has the literal global-squelch body"
        );
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    app.shutdown();
}

/// Global filtering accepts only dash-prefixed message types and no target/account operand.
/// No arguments query the retained authoritative global entry rather than changing it.
#[test]
fn filter_parser_prints_native_refusals_and_queries_the_authoritative_global_entry() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;

    hand.submit(&mut app, "@filter Spellcasting");
    assert!(app.interaction().last_sent.is_empty());
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(bubbles(&mut app)
        .iter()
        .any(|line| { line == "You must specify a valid message type prefixed by a dash." }));

    hand.submit(&mut app, "/unfilter -Wibble");
    assert!(app.interaction().last_sent.is_empty());
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(bubbles(&mut app)
        .iter()
        .any(|line| line == "\"Wibble\" is not a valid squelch category."));

    hand.submit(&mut app, "@filter -account");
    assert!(app.interaction().last_sent.is_empty());
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(bubbles(&mut app)
        .iter()
        .any(|line| line == "Incorrect usage, use @help for proper arguements."));

    hand.submit(&mut app, "/filter");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the no-argument query is local"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let log = chat_log(&mut app);
    assert!(log.contains(
        "The following types of messages are currently being filtered globally:\nnone\n\
(For a list of filter options, type @help filter)"
    ));

    app.objects_mut().world.chat.squelch.global.types = [
        dereth_client_model::chat::text_type::SPEECH_DIRECT,
        dereth_client_model::chat::text_type::COMBAT,
    ]
    .into_iter()
    .collect();
    hand.submit(&mut app, "@unfilter");
    assert!(app.interaction().last_sent.is_empty());
    for _ in 0..3 {
        assert!(app.frame());
    }
    let log = chat_log(&mut app);
    assert!(log.contains(
        "The following types of messages are currently being filtered globally:\nTell, Combat\n\
(For a list of filter options, type @help filter)"
    ));
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    app.shutdown();
}

/// `@fillcomps` raises the same fill notice as the Components/Vendor UI seam.
/// This drives the ordinary typed entry into the recorded vendor and checks the live Buying page,
/// then drives the exceptional `(INVALID_DID, -1)` clear request and the visible desired column.
#[test]
fn typed_fillcomps_populates_the_visible_buying_page_and_clear_updates_the_component_rows() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    open_recorded_component_vendor(&mut app);

    let (stock, wcid) = app
        .objects()
        .world
        .shop
        .stock
        .iter()
        .find(|row| row.pwd.name == "Lead Scarab")
        .map(|row| (row.iid, row.pwd.wcid))
        .expect("long-solo-play's general merchant stocks Lead Scarabs");
    assert_eq!(
        app.objects()
            .world
            .magic
            .catalogue
            .determine_component_category(wcid),
        dereth_client_model::magic::component_category::SCARAB,
        "the retail component tables classify the captured stock row"
    );
    let have = app
        .objects()
        .world
        .magic
        .components
        .num_component(&app.objects().world.magic.catalogue, wcid);
    let desired = i32::try_from(have + 6).expect("the captured component count is small");
    app.objects_mut()
        .world
        .player_system
        .set_desired_comp_level(wcid, desired);
    assert!(
        app.objects().world.shop.buy_list.is_empty(),
        "freshly opened buying basket"
    );

    hand.submit(&mut app, "@fillcomps Scarab");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(
        app.objects().world.shop.buy_list,
        vec![(stock, 6)],
        "the typed category command reuses the component-fill shortfall calculation"
    );
    assert!(
        app.interaction().last_sent.is_empty(),
        "filling the basket is local; only a later Buy command purchases it"
    );
    let vendor = &app.hud().panels.vendor;
    assert_eq!(vendor.tab, dereth_ui_screens::panels::vendor::Tab::Buying);
    let slot = vendor
        .buy
        .as_ref()
        .expect("the shipped Buying list is bound")
        .slots
        .iter()
        .find(|slot| slot.item == Some(stock))
        .expect("the live Buying list displays the filled component row");
    assert!(
        app.ui().expect("UI shell").ui.is_visible(slot.handle),
        "the populated buying row is on the visible tab"
    );

    hand.submit(&mut app, "/fillcomps clear");
    assert!(
        app.objects().world.player_system.desired_comps.is_empty(),
        "clearing the desired component list updates the retained PlayerModule mirror"
    );
    let [request @ Request::SetDesiredComponentLevel(message)] =
        app.interaction().last_sent.as_slice()
    else {
        panic!("clear emitted {:?}", app.interaction().last_sent)
    };
    assert_eq!((message.component_did, message.level), (0, -1));
    let mut session = Session::new(MockTransport::new());
    assert!(send_request(&mut session, request));
    let packet = session
        .transport
        .sent
        .last()
        .expect("the clear request is encoded");
    assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
    assert_eq!(
        packet.payload,
        action(1, 0x0224, &[dw(0), dw(u32::MAX)].concat()),
        "INVALID_DID then signed -1, exactly as the desired-component-level action packs it"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        app.hud()
            .panels
            .spell_components
            .rows
            .iter()
            .find(|row| row.wcid == wcid)
            .is_some_and(|row| row.desired == 0),
        "the component panel snapshot was rebuilt with the cleared desired level"
    );
    assert!(bubbles(&mut app)
        .iter()
        .any(|line| line == "Component list cleared."));
    app.shutdown();
}

/// The command accepts no arguments, a category, a price, or category plus price. It rejects only
/// a non-category/non-integer first argument, non-positive prices, and more than two arguments; a
/// valid category deliberately ignores a malformed optional second price and uses the zero limit.
#[test]
fn fillcomps_parser_preserves_retails_category_price_and_refusal_rules() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();

    for line in [
        "@fillcomps nonsense",
        "/fillcomps nonsense second",
        "/fillcomps 0",
        "@fillcomps Herb 10 extra",
    ] {
        hand.submit(&mut app, line);
        for _ in 0..2 {
            assert!(app.frame());
        }
    }
    let lines = bubbles(&mut app);
    assert!(lines
        .iter()
        .any(|line| line == "Invalid component type specified."));
    assert!(lines
        .iter()
        .any(|line| line == "Please specify a value greater than 0."));
    assert!(lines
        .iter()
        .any(|line| line == "Please use @help fillcomps for proper usage."));

    let invalid_before = app.interaction().stats.chat_commands_unimplemented;
    let notices_before = app.objects().world.scroll.added;
    for line in [
        "@fillcomps",
        "/fillcomps Scarabs",
        "@fillcomps 1000",
        "/fillcomps 1000 ignored",
        "/fillcomps Powder 1000",
        "@fillcomps Taper not-a-price",
    ] {
        hand.submit(&mut app, line);
    }
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        invalid_before
    );
    assert_eq!(
        app.objects().world.scroll.added - notices_before,
        6,
        "each valid form reaches the established no-open-vendor refusal consumer once: noargs, \
         plural category, price, two-token price, category+price, and native's \
         category+bad-price fallback"
    );
    // The chat-command arm invokes component filling directly, without a `UiRequest`, so the
    // refusal is pinned verbatim: it proves the real handler ran, and the generic `0x26` line is
    // absent. The component fill's first guard is an open vendor:
    //
    // ```text
    // if no vendor is open
    //     emit "You need an open vendor."
    // ```
    let lines = bubbles(&mut app);
    assert!(
        lines.iter().any(|line| line == dereth_client_model::vendor::NEED_AN_OPEN_VENDOR),
        "an accepted `@fillcomps` form reaches the component-fill no-vendor guard verbatim; saw {lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line == dereth_client_model::cmd::interp::NOT_A_VALID_COMMAND),
        "`@fillcomps` is a wired handler, not one of the unimplemented names; saw {lines:?}"
    );
    app.shutdown();
}

/// Every handler name in the command tables has an arm in `chat_command`: the sorted list of
/// wired handlers covers them all, and any name that leaves it must be deleted deliberately.
#[test]
fn the_handler_names_with_no_arm_are_exactly_these() {
    let mut names: Vec<&'static str> = dereth_client_model::cmd::table::INITIALIZE_COMMANDS
        .iter()
        .chain(dereth_client_model::cmd::table::TURBINE_CHAT_COMMANDS)
        .filter_map(|e| e.handler)
        .collect();
    names.sort_unstable();
    names.dedup();

    const WIRED: &[&str] = &[
        "tell",
        "reply",
        "retell",
        "channel_shortcut",
        "friends",
        "friends_add",
        "friends_remove",
        "guild",
        "general",
        "trade",
        "lfg",
        "roleplay",
        "society",
        "olthoi",
        "emote",
        "loc",
        "allegiance",
        "allegiance_hometown",
        "allegiance_broadcast",
        "motd",
        "lifestone",
        "marketplace",
        "house_recall",
        "mansion_recall",
        "pkarena",
        "pklarena",
        "pklite",
        "age",
        "birth",
        "die",
        "chat",
        "notell",
        "index",
        "clist",
        "on",
        "off",
        "afk",
        "consent",
        "permit",
        "speaker",
        "endurance",
        "emotes",
        "clear",
        "say",
        "corpse",
        "join",
        "leave",
        "house",
        "hslist",
        "help",
        "loadautoui",
        "loadui",
        "saveautoui",
        "saveui",
        "lockui",
        "squelch",
        "unsquelch",
        "filter",
        "unfilter",
        "fillcomps",
        "log",
        "loadfile",
        "framerate",
        "messagetypes",
        "version",
        "title",
        "day",
        "render",
    ];

    let remaining: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| !WIRED.contains(n))
        .collect();
    assert_eq!(
        remaining,
        Vec::<&str>::new()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        "every registered handler in this census is listed in WIRED"
    );
}

/// `@log` owns the real displayed-chat append stream.
///
/// The log command joins all arguments, supplies `.txt` only when the joined name has no
/// extension, and opens in `a+` mode. The displayed-chat append path writes the final
/// timestamp prefix plus filtered/trimmed body for every type except `0x1A`. A second argument-less
/// `@log` closes the file before it prints the screen-only acknowledgement.
#[test]
fn typed_log_appends_displayed_chat_stops_and_preserves_native_path_rules() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let dir = std::env::temp_dir()
        .join("dereth-chat-commands")
        .join(format!("chat-command-log-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
    }
    std::fs::create_dir_all(&dir).expect("create disposable log directory");

    let existing = dir.join("journey output.txt");
    std::fs::write(&existing, b"existing\r\n").expect("seed the append oracle");
    let existing_name = existing.to_str().expect("the test temp path is UTF-8");
    assert!(
        existing_name.is_ascii(),
        "the physical WM_CHAR station is deliberately ASCII"
    );
    hand.submit(&mut app, &format!("@log {existing_name}"));
    for _ in 0..3 {
        assert!(app.frame());
    }
    let success = format!(
        "Copying chat to {existing_name}.  Run command again with no arguments to turn off logging."
    );
    assert!(
        chat_log(&mut app).contains(&success),
        "the typed command did not reach the visible log-start success path"
    );

    hand.submit(&mut app, "@speaker");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        chat_log(&mut app).contains(dereth_client_model::chat_cmd::SPEAKER_RETIRED.trim()),
        "the subsequent line was not displayed by the real chat consumer"
    );

    // The network chat handlers compose their final strings directly in `Hud`, while client
    // notices enter through `Scroll`. Feed an actual encoded 0x02BD UI-queue record so this covers
    // both final-string producers rather than only local command feedback. A 0x5000 id takes the
    // ordinary clickable player-name branch.
    let tell = dereth_protocol::comms::CommunicationHearDirectSpeech {
        message: "log me from the wire".to_owned(),
        sender_name: "Archive Scribe".to_owned(),
        sender_id: ObjectId(0x5000_0101),
        target_id: ObjectId(0x5000_0100),
        text_type: dereth_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    };
    let tell_blob = dereth_protocol::write_blob(&tell).expect("the 0x02BD UI-queue record encodes");
    let expected_tell_log = format!(
        "<Tell:IIDString:{}:{}>{}<\\Tell> tells you, \"{}\"",
        tell.sender_id.0, tell.sender_name, tell.sender_name, tell.message
    );
    let expected_tell_visible = format!("{} tells you, \"{}\"", tell.sender_name, tell.message);
    app.apply_hud_events(&[SessionEvent::PlayerCreated(tell.target_id)]);
    // The full in-world body producer is covered by the speech tests. This station is about the
    // encoded UI-queue record reaching the two final-string consumers, so it states its body
    // premise directly.
    app.hud_mut().player_body = true;
    app.apply_hud_events(&[SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob: tell_blob,
    }]);
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        chat_log(&mut app).contains(&expected_tell_visible),
        "the incoming tell was not displayed by the actual Hud chat route"
    );
    let appended = std::fs::read(&existing).expect("read appended log");
    assert!(
        appended.starts_with(b"existing\r\n"),
        "a+ truncated the existing file"
    );
    assert!(
        String::from_utf8_lossy(&appended).contains(&format!("{success}\r\n")),
        "the success line is emitted after logging starts and must itself be logged"
    );
    assert!(
        String::from_utf8_lossy(&appended).contains(&format!(
            "{}\r\n",
            dereth_client_model::chat_cmd::SPEAKER_RETIRED.trim()
        )),
        "displayed type-zero chat did not reach the open append file"
    );
    assert!(
        String::from_utf8_lossy(&appended).contains(&format!("{expected_tell_log}\r\n")),
        "the displayed incoming tell bypassed the open append file"
    );

    hand.submit(&mut app, "@log");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        chat_log(&mut app).contains("Chat output now directed only to the screen."),
        "the argument-less active command did not visibly stop logging"
    );
    let stopped = std::fs::read(&existing).expect("read stopped log");
    assert!(
        String::from_utf8_lossy(&stopped)
            .contains(&format!("Chat log {existing_name} closed.\r\n")),
        "the final close line was not appended before the file closed"
    );
    hand.submit(&mut app, "@speaker");
    assert_eq!(std::fs::read(&existing).expect("read stopped log"), stopped);

    // Load-file variable substitution belongs to `@loadfile`, not `@log`:
    // `%DATE%` is literal here. This name also has no extension, so `.txt` is appended.
    let literal_stem = dir.join("literal-%DATE%");
    let literal_file = dir.join("literal-%DATE%.txt");
    hand.submit(
        &mut app,
        &format!("@log {}", literal_stem.to_str().expect("ASCII temp path")),
    );
    assert!(
        literal_file.is_file(),
        "the log command substituted %DATE% or omitted the required .txt suffix"
    );

    // Starting a replacement log closes the current handle before trying the replacement. A failed
    // replacement therefore must not silently keep writing the prior file.
    let before_failure = std::fs::read(&literal_file).expect("read literal-name log");
    let bad_stem = dir.join("missing-parent").join("cannot-open");
    let bad_name = bad_stem.to_str().expect("ASCII temp path");
    hand.submit(&mut app, &format!("@log {bad_name}"));
    hand.submit(&mut app, "@speaker");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        chat_log(&mut app).contains(&format!("Failed to redirect to file {bad_name}.txt!")),
        "the open failure did not use the visible log-command refusal"
    );
    let after_failure = std::fs::read(&literal_file).expect("read closed literal-name log");
    assert!(
        after_failure.starts_with(&before_failure),
        "the failed replacement rewrote prior bytes"
    );
    assert_eq!(
        &after_failure[before_failure.len()..],
        format!("Chat log {} closed.\r\n", literal_file.display()).as_bytes(),
        "replacement must append the close line to the old file, then leave it inactive"
    );

    app.shutdown();
    std::fs::remove_dir_all(&dir).expect("remove the disposable log directory");
}

/// `@loadfile` re-enters the ordinary command interpreter synchronously.
///
/// The load-file command passes the joined filename to the file reader unchanged. Each
/// `fgets` chunk is substituted, displayed in the command's source window, and only then handed
/// to the ordinary chat-command dispatcher. This journey starts at the real chat entry and uses
/// only local `@speaker` commands from a disposable file, so a socket or user-owned command file is
/// never involved.
#[test]
fn typed_loadfile_echoes_then_executes_each_local_command_in_file_order() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let dir = std::env::temp_dir()
        .join("dereth-chat-commands")
        .join(format!("chat-command-loadfile-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
    }
    std::fs::create_dir_all(&dir).expect("create disposable command directory");
    let script = dir.join("local commands.txt");
    std::fs::write(&script, b"@speaker\r\n@speaker %DATE%/%DATE%/$name\n")
        .expect("write disposable command file");
    let script_name = script.to_str().expect("the test temp path is UTF-8");
    assert!(
        script_name.is_ascii(),
        "the physical WM_CHAR station is deliberately ASCII"
    );

    hand.submit(&mut app, &format!("@loadfile {script_name}"));
    for _ in 0..3 {
        assert!(app.frame());
    }

    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after epoch")
        .as_secs() as i64;
    let date = dereth_client_model::cmd::loadfile::format_date(
        unix,
        dereth_client::platform::local_utc_offset_secs(unix),
    );
    // UTC/date-boundary cases are independently pinned in `dereth-input`; this covers the current
    // host-local replacement on screen.
    let second_echo = format!("@speaker {date}/{date}/$name");
    let log = chat_log(&mut app);
    let mut tail = log.as_str();
    for step in [
        "@speaker",
        dereth_client_model::chat_cmd::SPEAKER_RETIRED.trim(),
        &second_echo,
        dereth_client_model::chat_cmd::SPEAKER_RETIRED.trim(),
    ] {
        let at = tail.find(step).unwrap_or_else(|| {
            panic!("the loaded command file did not display/execute in native order; missing {step:?} in {log:?}")
        });
        tail = &tail[at + step.len()..];
    }

    hand.submit(&mut app, "@loadfile");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == "You must provide a file name."),
        "the missing-name refusal did not reach the bubble text"
    );

    let extension_stem = dir.join("no implicit extension");
    let extension_file = dir.join("no implicit extension.txt");
    std::fs::write(&extension_file, b"@speaker\n").expect("seed extension oracle");
    let extension_name = extension_stem.to_str().expect("ASCII temp path");
    hand.submit(&mut app, &format!("@loadfile {extension_name}"));
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == &format!("Cannot open file {extension_name}")),
        "the load-file command appended an extension or lost its exact open-failure line"
    );

    app.shutdown();
    std::fs::remove_dir_all(&dir).expect("remove the disposable command directory");
}

/// The string assignment stops each fetched command at its first NUL.
///
/// `fgets` has already consumed the rest of that physical line, so the next fetched line still
/// echoes and executes. The suffix deliberately names another local command: if the Rust string
/// preserves the NUL, the first command no longer matches `@speaker`; if code instead treats NUL
/// as end-of-file, the second `@speaker` never runs.
#[test]
fn typed_loadfile_truncates_each_fetched_chunk_at_nul_without_ending_the_file() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let dir = std::env::temp_dir()
        .join("dereth-chat-commands")
        .join(format!("chat-command-loadfile-nul-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
    }
    std::fs::create_dir_all(&dir).expect("create disposable command directory");
    let script = dir.join("embedded nul.txt");
    std::fs::write(&script, b"@speaker\0@framerate\r\n@speaker\r\n")
        .expect("write immutable embedded-NUL command file");
    let script_name = script.to_str().expect("the test temp path is UTF-8");
    assert!(
        script_name.is_ascii(),
        "the physical WM_CHAR station is deliberately ASCII"
    );

    hand.submit(&mut app, &format!("@loadfile {script_name}"));
    for _ in 0..3 {
        assert!(app.frame());
    }

    let log = chat_log(&mut app);
    assert!(
        !log.contains('\0'),
        "the displayed command contains no NUL character"
    );
    assert!(
        !log.contains("@framerate"),
        "the bytes after NUL in the first fetched chunk reached command dispatch"
    );
    assert_eq!(
        log.matches(dereth_client_model::chat_cmd::SPEAKER_RETIRED.trim())
            .count(),
        2,
        "the truncated prefix and the separately fetched next line must both execute: {log:?}"
    );

    app.shutdown();
    std::fs::remove_dir_all(&dir).expect("remove the disposable command directory");
}

/// `@messagetypes` prints the legal squelch/filter channel list.
///
/// The message-types command ignores `argc`/`argv`, asks the communication system for the legal
/// squelch channels, and adds its exact multi-line result to
/// the current source as timestamped type zero. The formatter starts with all 128 squelch bits
/// set, but emits only entries accepted by the legal-channel predicate, in ascending numeric order.
#[test]
fn typed_messagetypes_prints_the_native_legal_channel_order_without_a_request() {
    const LIST: &str = "Squelch channels are as follows:\n  \
Speech, Tell, Combat, Magic, Emote, Appraisal, Spellcasting, Allegiance, Fellowship, \
Combat_Enemy, Combat_Self, Recall, Craft, Salvaging";

    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let unimplemented = app.interaction().stats.chat_commands_unimplemented;
    let refused = app.interaction().stats.chat_commands_refused;

    hand.submit(&mut app, "/messagetypes");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the listing is process-local"
    );
    for _ in 0..3 {
        assert!(
            app.frame(),
            "deliver the type-zero line through the ordinary HUD drain"
        );
    }
    let first = chat_log(&mut app);
    assert!(
        first.contains(LIST),
        "the native channel table/order was absent: {first:?}"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented
    );

    // Unlike most command handlers, the message-types handler never reads argc/argv. All four
    // registered aliases share that behavior, so an extra token produces another identical list
    // rather than 0x1A.
    hand.submit(&mut app, "@msg_types ignored");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the argument-ignoring alias is local too"
    );
    for _ in 0..3 {
        assert!(
            app.frame(),
            "deliver the alias result through the ordinary HUD drain"
        );
    }
    let second = chat_log(&mut app);
    assert_eq!(
        second.matches(LIST).count(),
        first.matches(LIST).count() + 1
    );
    assert_eq!(app.interaction().stats.chat_commands_refused, refused);
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented
    );
    app.shutdown();
}

/// `@version` reports this executable's truthful build identity.
///
/// The version command reports the running executable's own version resource, not the DAT or
/// login-protocol version. This client's equivalent build-owned provenance is Cargo's package
/// name and version; spelling `dereth-client` in the value ensures this binary does not identify
/// itself as the retail executable.
#[test]
fn typed_version_prints_the_rebuild_identity_and_native_optional_turbine_line() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let identity = format!(
        "Client version {} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    let unimplemented = app.interaction().stats.chat_commands_unimplemented;

    hand.submit(&mut app, "/version");
    assert!(
        app.interaction().last_sent.is_empty(),
        "ordinary @version is local"
    );
    for _ in 0..3 {
        assert!(
            app.frame(),
            "deliver the type-zero version line through the ordinary HUD drain"
        );
    }
    let first = chat_log(&mut app);
    assert!(
        first.lines().any(|line| line.ends_with(&identity)),
        "identity absent: {first:?}"
    );
    assert!(
        !first.contains("Using Turbine Chat."),
        "the disabled-chat branch printed its marker"
    );
    assert!(
        !first.contains("1802"),
        "the login protocol version was mislabelled as this binary"
    );

    app.objects_mut().world.chat.using_turbine_chat = true;
    hand.submit(&mut app, "@version");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the Turbine marker is local too"
    );
    for _ in 0..3 {
        assert!(
            app.frame(),
            "deliver both type-zero lines through the ordinary HUD drain"
        );
    }
    let second = chat_log(&mut app);
    assert!(
        second.contains("Using Turbine Chat."),
        "the native optional marker was absent"
    );
    assert_eq!(
        second.matches(&identity).count(),
        first.matches(&identity).count() + 1
    );

    let refused = app.interaction().stats.chat_commands_refused;
    hand.submit(&mut app, "/version ignored");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the argument refusal is local"
    );
    for _ in 0..3 {
        assert!(
            app.frame(),
            "deliver the 0x1A refusal through the ordinary HUD drain"
        );
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == "Unexpected arguments to @version"),
        "the native argc refusal was absent"
    );
    let third = chat_log(&mut app);
    assert_eq!(
        third.matches(&identity).count(),
        second.matches(&identity).count()
    );
    assert_eq!(app.interaction().stats.chat_commands_refused, refused + 1);
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented
    );
    app.shutdown();
}

/// PSR qualities add one bare F7CC query after the local version line.
#[test]
fn typed_version_queries_the_server_only_for_the_three_native_psr_boolean_qualities() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    app.objects_mut().world.seed_player_desc(
        ObjectId(0x5000_000A),
        dereth_client_model::Qualities::default(),
    );

    let apply_bool = |app: &mut App, sequence, property_id, value| {
        let update = dereth_protocol::qualities::QualitiesPrivateUpdateBool(
            dereth_protocol::qualities::PrivateUpdate {
                sequence,
                property_id,
                value,
            },
        );
        let mut blob = dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_BOOL
            .0
            .to_le_bytes()
            .to_vec();
        blob.extend(dereth_protocol::write_body(&update).expect("private Boolean quality update"));
        let event = SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_BOOL,
            blob,
        };
        app.apply_hud_events(std::slice::from_ref(&event));
        assert_eq!(
            app.objects()
                .world
                .player_qualities()
                .expect("player descriptor")
                .inq_bool(property_id),
            value != 0,
            "the encoded private update reached the production quality store"
        );
    };

    // A PlayerDesc without any of the three bits keeps the ordinary local-only path.
    hand.submit(&mut app, "/version");
    assert!(
        app.interaction().last_sent.is_empty(),
        "an ordinary player sends no query"
    );
    let requests_before = app.interaction().stats.chat_command_requests;

    // The PSR-lead check asks 0x2C then 0x2D; the PSR check asks 0x61 after a false lead.
    for property_id in [0x2C, 0x2D, 0x61] {
        apply_bool(&mut app, 1, property_id, 1);
        hand.submit(&mut app, "/version");
        let [request @ Request::AdminGetServerVersion(
            dereth_protocol::admin::AdminSendAdminGetServerVersion,
        )] = app.interaction().last_sent.as_slice()
        else {
            panic!(
                "Boolean {property_id:#04X} emitted {:?}",
                app.interaction().last_sent
            )
        };

        let mut session = Session::new(MockTransport::new());
        assert!(
            send_request(&mut session, request),
            "the bare F7CC sender is connected"
        );
        let [packet] = session.transport.sent.as_slice() else {
            panic!("one query produced {:?}", session.transport.sent)
        };
        assert_eq!((packet.queue, packet.ordered), (NetQueue::Control, false));
        assert_eq!(
            packet.payload,
            [0xCC, 0xF7, 0, 0],
            "the oracle is independent of the codec"
        );
        assert_eq!(
            session.next_action_stamp(),
            1,
            "a bare Control send consumes no action stamp"
        );

        // Removing this bit leaves none of the three set, so the next typed command is local.
        apply_bool(&mut app, 2, property_id, 0);
        hand.submit(&mut app, "/version");
        assert!(
            app.interaction().last_sent.is_empty(),
            "false {property_id:#04X} is not PSR"
        );
    }
    assert_eq!(
        app.interaction().stats.chat_command_requests,
        requests_before + 3
    );

    // The argument refusal precedes the admin-status check and therefore sends nothing even for a PSR.
    apply_bool(&mut app, 3, 0x61, 1);
    hand.submit(&mut app, "/version ignored");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the argument refusal cannot send F7CC"
    );
    assert_eq!(
        app.interaction().stats.chat_command_requests,
        requests_before + 3
    );
    app.shutdown();
}

/// The title command joins the popup command's arguments into a literal `StringInfo`, raises
/// `SetChatWindowTitle(window, value)`, and that window's receiver both draws and stores it in the
/// retained PlayerModule. The stored-string count includes the NUL: 98 user characters are
/// accepted at count 99 and 99 are refused at count 100.
#[test]
fn typed_popup_title_draws_literal_and_survives_the_player_module_roundtrip() {
    let _gpu = gpu_lock();
    let mut app = app();
    install_player_module(&mut app);
    for _ in 0..3 {
        app.frame();
    }
    assert!(
        !app.objects().world.player_system.is_dirty(),
        "the authored default captions must not manufacture a PlayerModule title write"
    );

    let mut hand = Hand::new();
    let (entry, title, _, window) = first_popup(&mut app);
    assert_eq!(
        window, 2,
        "the first shipped floaty chat carries native source/window id 2"
    );
    let unimplemented = app.interaction().stats.chat_commands_unimplemented;
    let requests = app.interaction().stats.requests_sent;

    // This deliberately resembles a shipped token. The title path uses a literal value, so resolving it
    // through table enum 1 would be observably wrong.
    let literal = "ID_Chat_Chat1_DefaultTitle";
    hand.submit_to(&mut app, entry, &format!("/title {literal}"));
    assert_eq!(
        element_text(&mut app, title),
        literal,
        "the live popup caption is the literal"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented
    );
    assert_eq!(
        app.interaction().stats.requests_sent,
        requests,
        "the title command is local-only"
    );
    let stored = stored_chat_title(&app, window).expect("the popup title reached PlayerModule");
    assert_eq!(stored.over, 1);
    assert_eq!(stored.literal.as_deref(), Some(literal));
    assert_eq!((stored.string_id, stored.table_id), (0, 0));
    assert!(app.objects().world.player_system.is_dirty());

    let packed = app
        .objects()
        .world
        .player_system
        .client_packed_module()
        .expect("retained module");
    let mut writer = dereth_protocol::Writer::new();
    packed.write(&mut writer).expect("pack PlayerModule");
    let bytes = writer.into_inner();
    let reparsed =
        dereth_protocol::login::PlayerModule::read(&mut dereth_protocol::Reader::new(&bytes))
            .expect("reparse PlayerModule");
    assert_eq!(module_chat_title(&reparsed, window), Some(stored));

    // A replacement gameplay tree reads the literal property and restores it through the window's
    // title receiver, which writes the equal property again. The default title must not win.
    let mut save = dereth_client_model::RecordingRequests::default();
    assert!(app
        .objects_mut()
        .world
        .player_system
        .save_to_server(&mut save, false));
    assert!(!app.objects().world.player_system.is_dirty());
    app.queue_ui_mode(mode::GAME_PLAY);
    assert!(app.frame());
    let (_, rebuilt_title, _, rebuilt_window) = first_popup(&mut app);
    assert_eq!(rebuilt_window, window);
    assert_eq!(element_text(&mut app, rebuilt_title), literal);
    assert!(
        app.objects().world.player_system.is_dirty(),
        "restoring the equal title writes it back through the retained module's change path"
    );
    assert_eq!(
        stored_chat_title(&app, window)
            .and_then(|s| s.literal)
            .as_deref(),
        Some(literal)
    );

    let accepted = "a".repeat(98);
    let (entry, title, popup_log, _) = first_popup(&mut app);
    hand.submit_to(&mut app, entry, &format!("/title {accepted}"));
    assert_eq!(element_text(&mut app, title), accepted);
    assert_eq!(
        stored_chat_title(&app, window).and_then(|s| s.literal),
        Some(accepted.clone())
    );

    let refused = "b".repeat(99);
    hand.submit_to(&mut app, entry, &format!("/title {refused}"));
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(
        element_text(&mut app, title),
        accepted,
        "99 characters leave the title alone"
    );
    assert!(
        element_text(&mut app, popup_log)
            .contains("Window title length cannot exceed 100 characters."),
        "the refusal stays in the originating popup"
    );

    hand.submit_to(&mut app, entry, "/title");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        element_text(&mut app, popup_log).contains("You must provide a new title for the window."),
        "empty joined arguments are refused in the popup"
    );

    hand.submit(&mut app, "/title Main Must Stay Main");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        chat_log(&mut app).contains("This command must be issued from a popup chat window."),
        "source 8 is the main-window refusal"
    );
    assert_eq!(element_text(&mut app, title), accepted);
    assert_eq!(app.interaction().stats.requests_sent, requests);
    app.shutdown();
}
