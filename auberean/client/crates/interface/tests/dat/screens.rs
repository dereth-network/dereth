//! From the character list into the world through the retail screens, as a player clicks and
//! types: a server double plays the servers' side (an account with no character, a character made,
//! the world entry into the human training hall); the shell's host carries its datagrams on a clock
//! set by hand, and the test points and clicks at the screens' own buttons.
//!
//! Fixture: the September 2013 install (the layouts, pictures and fonts of the screens; the hall's
//! cells, the human male); a hand-written login.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{
    CharacterCreated, CharacterList, DataQuery, Empty, PlayerObject, PlayerQualities,
};
use auberean_client_sdk::protocol::physics::CreateObject;
use auberean_client_sdk::protocol::table::{Table, SMALL};
use auberean_client_sdk::protocol::text::StringInfo;
use auberean_client_sdk::protocol::text::{NarrowString, WideString};
use auberean_client_sdk::protocol::types::{
    CharacterIdentity, ParentLink, PhysicsDesc, Placement, Qualities, VisualDesc, WeenieDesc,
};
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::runtime::description::{set_bool, set_instance_id, set_string_info};
use auberean_client_sdk::Action;
use auberean_client_sdk::ClientData;
use auberean_interface::{Login, Retail};
use auberean_primitives::{Frame, Quat, Vec3};
use auberean_scene::WorldData;
use auberean_shell::host::InputArea;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::{Game, Interface, Overlay, Phase, Request};
use auberean_shell::{Host, Shell};
use auberean_testkit::double::script::{ConnSpec, Event as Step, Msg, Script};
use auberean_testkit::double::ServerDouble;
use auberean_ui::{DatAssets, Token};

const PLAYER: u64 = 0x2130_0000_0000_0001;
const HALL_CELL: u32 = 0x0298_0109;
const HALL_AT: Vec3 = Vec3 {
    x: 59.0,
    y: 240.0,
    z: -44.6,
};
const HUMAN_MALE: u32 = 0x1F00_0023;
const NAME: &str = "Rowan";
/// The name quality.
const NAME_QUALITY: u32 = 0x100;

type Data = (WorldData, ClientData, Arc<DatAssets>);

/// The world, the client's data and the interface's records, opened once.
pub(crate) fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = auberean_dat::testing::require_target_install();
        let found = install_dats(&dir).expect("list the install");
        let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
            let (_, p) = found.iter().find(|(n, _)| want(n))?;
            DatFile::open(p).ok()
        };
        let portal = open(&|n| *n == DatName::Portal).expect("a portal file");
        let cell = open(&|n| *n == DatName::Cell(1)).expect("a cell file");
        let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
        let ui_ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
        let records = auberean_world::records::WorldRecords::new(
            portal,
            cell,
            open(&|n| *n == DatName::HighRes),
            ctx,
        );
        let world = WorldData::new(records).expect("the region");
        let portal = Arc::new(open(&|n| *n == DatName::Portal).expect("a portal file"));
        let cell = Arc::new(open(&|n| *n == DatName::Cell(1)).expect("a cell file"));
        let data = ClientData {
            files: DataFiles {
                files: vec![portal, cell],
                language: ENGLISH,
            },
            scripts: None,
        };
        let mut dats = vec![open(&|n| *n == DatName::Local("English".into())).expect("English")];
        dats.push(open(&|n| *n == DatName::Portal).expect("a portal file"));
        dats.extend(open(&|n| *n == DatName::HighRes));
        (world, data, Arc::new(DatAssets::new(dats, ui_ctx)))
    })
}

fn conn(n: usize, login: bool, port: u16, t_request: f64) -> ConnSpec {
    ConnSpec {
        n,
        login,
        server_port: port,
        server_id: if login { 0xF1 } else { 0x0B },
        iteration: if login { 5 } else { 2 },
        net_id: u32::try_from(n + 1).expect("small"),
        server_time: 1.25e8,
        t_request,
        t_connect: t_request + 0.05,
        switched: !login,
    }
}

fn s2c(t: f64, conn: usize, queue: u16, blob_id: u64, m: &ServerMessage) -> Step {
    Step::Msg {
        t,
        msg: Msg {
            conn,
            c2s: false,
            queue,
            blob_id,
            mtype: m.message_type(),
            raw: m.encode(),
        },
    }
}

fn c2s(t: f64, conn: usize, queue: u16, mtype: u32) -> Step {
    Step::Msg {
        t,
        msg: Msg {
            conn,
            c2s: true,
            queue,
            blob_id: 0,
            mtype,
            raw: mtype.to_le_bytes().to_vec(),
        },
    }
}

const LOGIN: u64 = 0x2300_0000_0000_0000;
const WORLD: u64 = 0x0300_0000_0000_0000;

/// What the servers say of a player's object, as the 2017 servers sent it: its script package and
/// entity template (whose qualities record says it moves), and its physics type (the low and high
/// words), one the camera's lines of sight pass through.
fn player_weenie() -> WeenieDesc {
    let mut w = WeenieDesc {
        mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY,
        package_id: 0x37F,
        entity_did: 0x4700_0530,
        ..WeenieDesc::default()
    };
    for (bit, word) in [(0x8000, 0x0480_0020), (0x1_0000, 0)] {
        let i = WeenieDesc::NUMBER_BITS
            .iter()
            .position(|b| *b == bit)
            .expect("a physics type word");
        w.mask |= bit;
        w.numbers[i] = word;
    }
    w
}

/// An account with no character; the character made when asked; the world entry into the hall.
fn script() -> Script {
    script_with(Vec::new())
}

/// [`script`], with more of the server's messages on the world's connection.
fn script_with(extra: Vec<Step>) -> Script {
    let list = CharacterList {
        characters: Vec::new(),
        deleted: Vec::new(),
        status: 0,
        slots: 6,
        account: NarrowString::from("tester"),
        uses_chat_service: 1,
        has_expansion: 0,
        unknown: 0,
    };
    let made = CharacterCreated {
        response: CharacterCreated::OK,
        identity: CharacterIdentity {
            id: PLAYER,
            name: WideString(NAME.encode_utf16().collect()),
            seconds_greyed_out: 0,
            visual: VisualDesc::default(),
        },
        result: 0,
    };
    // The 2017 servers name the player in its own description: a written name, its meta
    // letters after it.
    let mut weenie = player_weenie();
    set_string_info(
        &mut weenie,
        NAME_QUALITY,
        StringInfo::literal(&format!("{NAME} [M]")),
    );
    let player = CreateObject {
        id: PLAYER,
        weenie,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 8,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: HALL_AT,
                    rotation: Quat {
                        w: 1.0,
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                },
            },
            ..PhysicsDesc::default()
        },
    };
    let events = vec![
        s2c(
            0.20,
            0,
            5,
            LOGIN,
            &ServerMessage::DataQuery(DataQuery {
                region: 1,
                name_language: 1,
                languages: vec![1],
            }),
        ),
        c2s(0.25, 0, 5, 0x10038),
        s2c(0.30, 0, 5, LOGIN | 1, &ServerMessage::DownloadEnds(Empty)),
        s2c(0.31, 0, 1, LOGIN | 2, &ServerMessage::CharacterList(list)),
        c2s(6.00, 0, 4, 0x1002A),
        s2c(
            6.10,
            0,
            1,
            LOGIN | 3,
            &ServerMessage::CharacterCreated(made),
        ),
        c2s(6.20, 0, 4, 0x1002B),
        Step::Referral {
            t: 6.25,
            from: 0,
            to: Some(1),
        },
        s2c(
            6.40,
            1,
            1,
            WORLD | 4,
            &ServerMessage::PlayerObject(PlayerObject {
                id: PLAYER,
                region: 1,
            }),
        ),
        s2c(6.41, 1, 1, WORLD | 5, &ServerMessage::CreateObject(player)),
        s2c(
            6.42,
            1,
            1,
            WORLD | 6,
            &ServerMessage::PlayerQualities(PlayerQualities(Qualities {
                header: Qualities::INTS,
                ints: Table::from_inserted(SMALL, [(258, 80), (263, 100), (259, 55), (264, 120)]),
                ..Qualities::default()
            })),
        ),
        c2s(8.00, 1, 3, 0x100D1),
    ];
    let mut events = events;
    events.extend(extra);
    events.sort_by(|a, b| a.t().total_cmp(&b.t()));
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 6.3)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

struct DoubleHost {
    time: f64,
    double: ServerDouble,
    inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    /// Where the input method writes, as the shell last told.
    input_method: Option<InputArea>,
}

fn me() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 50_000))
}

impl Host for DoubleHost {
    fn now(&self) -> f64 {
        self.time
    }
    fn unix_time(&self) -> u32 {
        0
    }
    fn random(&mut self, out: &mut [u8]) {
        out.fill(7);
    }
    fn connect(&mut self, _server: SocketAddr) -> Result<(), String> {
        Ok(())
    }
    fn send(&mut self, to: SocketAddr, bytes: &[u8]) {
        let t = LocalTime(1.0 + self.time);
        self.double.datagram(to, me(), bytes, t);
    }
    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.inbox.pop_front()
    }
    fn hold_pointer(&mut self, _held: bool) {}
    fn set_title(&mut self, _title: &str) {}
    fn load_settings(&self) -> Option<String> {
        None
    }
    fn save_settings(&mut self, _text: &str) {}
    fn set_input_method(&mut self, area: Option<InputArea>) {
        self.input_method = area;
    }
}

/// The interface, shared with the test so it can look at the screens while the shell drives it.
struct Shared(Rc<RefCell<Retail>>);

thread_local! {
    /// What the interface asked of the shell, in this test's thread.
    static ASKED: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
}

impl Interface for Shared {
    fn event(&mut self, event: &Event, game: &Game<'_>) -> bool {
        self.0.borrow_mut().event(event, game)
    }
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.borrow_mut().update(game, requests);
        ASKED.with(|a| a.borrow_mut().extend(requests.iter().cloned()));
    }
    fn draw(&mut self, game: &Game<'_>, overlay: &mut Overlay) {
        self.0.borrow_mut().draw(game, overlay);
    }
    fn wants_pointer(&self, game: &Game<'_>) -> bool {
        self.0.borrow().wants_pointer(game)
    }
    fn prepare(&mut self, client: &mut auberean_client_sdk::Client) {
        self.0.borrow_mut().prepare(client);
    }
}

/// A frame of 1/60 s.
fn step(shell: &mut Shell<'_, DoubleHost>) {
    let h = &mut shell.host;
    h.time += 1.0 / 60.0;
    let t = LocalTime(1.0 + h.time);
    for d in h.double.tick(t) {
        h.inbox.push_back((d.from, d.bytes));
    }
    let _ = shell.frame();
}

/// Where an element named `name` is on the screen: its middle.
fn middle(retail: &Rc<RefCell<Retail>>, name: &str) -> (f32, f32) {
    let r = retail.borrow();
    let ui = r.ui();
    let h = ui
        .find(Token::of(name))
        .unwrap_or_else(|| panic!("{name} on the screen"));
    let rect = ui.screen_rect(h).expect("placed");
    #[allow(clippy::cast_precision_loss)]
    let m = (
        (rect.x0 + rect.x1) as f32 / 2.0,
        (rect.y0 + rect.y1) as f32 / 2.0,
    );
    m
}

/// Point at `(x, y)`, press the left button, let it go: a frame between each.
fn click(shell: &mut Shell<'_, DoubleHost>, (x, y): (f32, f32)) {
    shell.event(&Event::Pointer { x, y });
    step(shell);
    for down in [true, false] {
        shell.event(&Event::Button {
            button: Button::Left,
            down,
        });
        step(shell);
    }
}

/// The opening pictures, ended as a player ends them, with a press let go: connecting follows.
fn past_the_intro(shell: &mut Shell<'_, DoubleHost>, retail: &Rc<RefCell<Retail>>) {
    step(shell);
    assert_eq!(
        retail.borrow().screen(),
        "intro",
        "the game opens on its pictures"
    );
    click(shell, (10.0, 10.0));
    step(shell);
}

/// Run frames until `done`, at most `seconds` more of the host's clock.
fn until(
    shell: &mut Shell<'_, DoubleHost>,
    seconds: f64,
    mut done: impl FnMut(&Shell<'_, DoubleHost>) -> bool,
) -> bool {
    let end = shell.host.time + seconds;
    while shell.host.time < end {
        if done(shell) {
            return true;
        }
        step(shell);
    }
    done(shell)
}

/// The flow every test here starts with: an empty list, Create, the man in the hall, Next, the
/// name typed, Enter, until the player is in the world.
fn into_the_world() -> (Shell<'static, DoubleHost>, Rc<RefCell<Retail>>) {
    into_the_world_with(script())
}

/// [`into_the_world`] on a script of the test's own.
fn into_the_world_with(script: Script) -> (Shell<'static, DoubleHost>, Rc<RefCell<Retail>>) {
    let (_, client_data, _) = data();
    into_the_world_running(script, client_data)
}

/// The client's data with the install's scripts linked: the objects' scripts run.
fn data_with_scripts() -> &'static ClientData {
    static SCRIPTED: OnceLock<ClientData> = OnceLock::new();
    SCRIPTED.get_or_init(|| {
        let (_, plain, _) = data();
        let portal = Arc::clone(&plain.files.files[0]);
        let program =
            auberean_client_sdk::runtime::link_scripts(&portal).expect("the library links");
        ClientData {
            files: plain.files.clone(),
            scripts: Some((portal, Arc::new(program))),
        }
    })
}

/// [`into_the_world_with`], the client running on `client_data`.
fn into_the_world_running(
    script: Script,
    client_data: &'static ClientData,
) -> (Shell<'static, DoubleHost>, Rc<RefCell<Retail>>) {
    let (world, _, assets) = data();
    let double = ServerDouble::new(script, std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let server = double.login_addr();
    let records: Arc<dyn auberean_ui::Assets + Send + Sync> = Arc::clone(assets) as _;
    let retail = Rc::new(RefCell::new(Retail::new(
        records,
        (1024, 768),
        Some(Login {
            server,
            account: "tester".into(),
            password: "t".into(),
        }),
    )));
    let host = DoubleHost {
        time: 0.0,
        double,
        inbox: VecDeque::new(),
        input_method: None,
    };
    let mut s = Shell::new(
        host,
        world,
        client_data.clone(),
        Box::new(Shared(Rc::clone(&retail))),
    );
    s.size = (1024, 768);
    let screen = |r: &Rc<RefCell<Retail>>| r.borrow().screen();
    past_the_intro(&mut s, &retail);

    let listed = until(&mut s, 5.0, |s| {
        s.phase() == Phase::Characters
            && s.client
                .as_ref()
                .is_some_and(|c| c.model.characters.is_some())
    });
    assert!(listed, "the character list arrives: {:?}", s.phase());
    assert!(
        until(&mut s, 1.0, |_| screen(&retail) == "select"),
        "select: {}",
        screen(&retail)
    );

    let create = middle(&retail, "CreateButton");
    click(&mut s, create);
    assert!(
        until(&mut s, 1.0, |_| screen(&retail) == "avatar"),
        "Create opens the avatar hall: {}",
        screen(&retail)
    );

    // The man stands in the right front of the hall: the pointer over him a moment (the hall
    // asks its scene what is under the pointer), then pressed.
    for x in [829.0, 830.0] {
        s.event(&Event::Pointer { x, y: 470.0 });
        step(&mut s);
    }
    for _ in 0..4 {
        step(&mut s);
    }
    click(&mut s, (830.0, 470.0));
    assert!(
        until(&mut s, 1.0, |_| screen(&retail) == "appearance"),
        "a human picked: {}",
        screen(&retail)
    );

    // One stop taller.
    let taller = middle(&retail, "HeightRightButton");
    click(&mut s, taller);
    step(&mut s);

    let next = middle(&retail, "NextButton");
    click(&mut s, next);
    assert!(
        until(&mut s, 1.0, |_| screen(&retail) == "name"),
        "Next asks for the name: {}",
        screen(&retail)
    );

    // The entry takes the keyboard when pressed.
    let entry = middle(&retail, "NameEntry");
    click(&mut s, entry);
    s.event(&Event::Text(NAME.into()));
    step(&mut s);
    let enter = middle(&retail, "EnterButton");
    click(&mut s, enter);

    let entered = until(&mut s, 10.0, |s| s.phase() == Phase::World);
    assert!(
        entered,
        "in the world: {:?}, screen {}",
        s.phase(),
        screen(&retail)
    );
    let panels = until(&mut s, 1.0, |_| {
        retail
            .borrow()
            .play()
            .is_some_and(|p| !p.children().is_empty())
    });
    assert!(panels, "the play screen's panels come up");
    (s, retail)
}

/// The game opens on its pictures, which a press ends; connecting logs in and brings up the
/// character list. A player with no character clicks Create there, the human man in the avatar
/// hall (the figure the ray through the pointer meets in the hall's scene), raises the height a
/// stop on the appearance, Next, clicks the name's entry, types a name and clicks Enter: the
/// character is asked of the server as a human man with that name, the player's entity, the human
/// start area, class and implement, and all thirteen physique values (the height the stop chosen,
/// a type a human lacks 0); once made the client enters the world with it; the play screen's
/// panels come up.
///
/// Behaviour: desktop.interface.the-screens-take-a-player-from-the-list-into-the-world
#[test]
fn the_screens_take_a_player_from_the_list_into_the_world() {
    let (s, retail) = into_the_world();
    assert_eq!(
        retail.borrow().screen(),
        "play",
        "the play screen's panels are up"
    );
    let sent: Vec<ClientMessage> = s
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .map(|m| m.message.clone())
        .collect();
    let made = sent
        .iter()
        .find_map(|m| match m {
            ClientMessage::CreateCharacter(c) => Some(c.clone()),
            _ => None,
        })
        .expect("a character asked for");
    assert_eq!(String::from_utf16_lossy(&made.name.0), NAME);
    assert_eq!((made.race, made.sex), (1, 0x1000), "a human man");
    assert_eq!(
        (made.entity_did, made.start_area, made.class, made.implement),
        (0x4700_0530, 0x4000_0001, 2, 4),
        "a player's entity, the human start area, class and implement"
    );
    let order: Vec<u32> = made.physique.entries.iter().map(|(t, _)| *t).collect();
    assert_eq!(
        order,
        vec![0x1, 0x2, 0x4, 0x1000, 0x8, 0x10, 0x40, 0x100, 0x800, 0x20, 0x80, 0x400, 0x200],
        "all thirteen types, laid out as the table holds them"
    );
    let heights = auberean_world::chargen::by_steps(0.95, 1.1);
    assert_eq!(
        made.physique.get(0x2),
        Some(&heights[8]),
        "the height one stop above the middle"
    );
    assert_eq!(
        made.physique.get(0x8),
        Some(&0.0),
        "no skin detail for a human"
    );
    let played = sent
        .iter()
        .any(|m| matches!(m, ClientMessage::PlayCharacter(p) if p.character == PLAYER));
    assert!(played, "entered with the character made");
}

/// The text a text element shows.
fn text_of(retail: &Rc<RefCell<Retail>>, name: &str) -> Option<String> {
    let r = retail.borrow();
    let ui = r.ui();
    let h = ui.find(Token::of(name))?;
    ui.text(h)
}

/// In the world the status bar shows the player's health and vigor from its qualities; the war
/// button asks the player's scripts to attack, its look waiting on their word; Enter starts a chat
/// line that takes the keys typed (the body never moves), and Enter again ends it, the entry
/// emptied and the line handed to the player's command line.
///
/// Behaviour: desktop.interface.the-play-screen-follows-the-player-and-takes-war-and-chat
#[test]
fn the_play_screen_follows_the_player_and_takes_war_and_chat() {
    let (mut s, retail) = into_the_world();
    let shown = until(&mut s, 2.0, |_| retail.borrow().vitals_shown().is_some());
    assert!(shown, "vitals shown");
    assert_eq!(retail.borrow().vitals_shown(), Some([80, 100, 55, 120]));
    assert_eq!(text_of(&retail, "HealthText").as_deref(), Some("80 / 100"));
    assert_eq!(text_of(&retail, "VigorText").as_deref(), Some("55 / 120"));

    assert_eq!(retail.borrow().attacking(), Some(false));
    let war = middle(&retail, "StartAttackButton");
    click(&mut s, war);
    step(&mut s);
    let asked = ASKED.with(|a| {
        a.borrow()
            .contains(&Request::Act(Action::Attack { start: true }))
    });
    assert!(asked, "the war button asks the player's scripts to attack");
    assert_eq!(
        retail.borrow().attacking(),
        Some(false),
        "peace shown until the scripts say the player attacks"
    );

    let before = s.scene.status.origin;
    for key in [Key::Enter, Key::W] {
        s.event(&Event::Key {
            key,
            down: true,
            repeat: false,
        });
        step(&mut s);
    }
    s.event(&Event::Text("w".into()));
    for _ in 0..60 {
        step(&mut s);
    }
    let after = s.scene.status.origin;
    assert!(
        ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt() < 0.01,
        "W typed into the line never moves the body: {before:?} {after:?}"
    );
    assert_eq!(text_of(&retail, "ChatEntry").as_deref(), Some("w"));
    s.event(&Event::Key {
        key: Key::Enter,
        down: true,
        repeat: false,
    });
    step(&mut s);
    step(&mut s);
    {
        let r = retail.borrow();
        let ui = r.ui();
        let e = ui.find(Token::of("ChatEntry")).expect("the entry");
        assert_eq!(ui.focus(), None, "Enter ends the line");
        assert_eq!(ui.text(e).as_deref(), Some(""), "the entry emptied");
    }
    let typed = s.client.as_ref().map(|c| c.shell.lines().to_vec());
    assert_eq!(
        typed,
        Some(vec!["w".to_owned()]),
        "the line went to the player's command line"
    );
}

/// A player in the world whose login the server ends with a boot (its line, from the game's
/// strings, that the character was booted) is taken to the disconnected screen showing that line:
/// the play screen goes, the character is logged off and the world let go; the screen stays, and
/// its button leaves the game.
///
/// Behaviour: desktop.interface.a-boot-ends-the-session-on-the-disconnected-screen-with-its-line
#[test]
fn a_boot_ends_the_session_on_the_disconnected_screen_with_its_line() {
    use auberean_client_sdk::protocol::login::Text;
    use auberean_client_sdk::protocol::text::StringInfo;
    let boot = ServerMessage::AccountBooted(Text(StringInfo {
        string_id: 0xDA7D_EF7F,
        table_id: 0x2500_0012,
        ..StringInfo::default()
    }));
    let (mut s, retail) = into_the_world_with(script_with(vec![s2c(10.0, 0, 1, LOGIN | 9, &boot)]));
    let screen = |r: &Rc<RefCell<Retail>>| r.borrow().screen();
    assert_eq!(screen(&retail), "play");
    assert!(
        until(&mut s, 6.0, |_| screen(&retail) == "disconnected"),
        "the disconnected screen: {}",
        screen(&retail)
    );
    assert_eq!(
        text_of(&retail, "ErrorText").as_deref(),
        Some("Your character has been booted from the world."),
        "the boot's line"
    );
    assert!(retail.borrow().play().is_none(), "the play screen gone");
    let logged_off = s
        .host
        .double
        .report()
        .received
        .iter()
        .flatten()
        .any(|m| m.raw.get(..4) == Some(&0x0001_0027_u32.to_le_bytes()[..]));
    assert!(logged_off, "the character logged off");
    assert_ne!(s.phase(), Phase::World, "out of the world");
    for _ in 0..180 {
        step(&mut s);
    }
    assert_eq!(screen(&retail), "disconnected", "it stays");
    assert_eq!(
        text_of(&retail, "ErrorText").as_deref(),
        Some("Your character has been booted from the world.")
    );
    let quit = middle(&retail, "QuitButton");
    click(&mut s, quit);
    step(&mut s);
    assert!(
        ASKED.with(|a| a.borrow().contains(&Request::Quit)),
        "its button leaves the game"
    );
}

/// A player in the world whose world server goes (here it ends the connection, and the client
/// answers its goodbye) is taken to the disconnected screen saying the connection was lost: the
/// session is over, the play screen gone; the screen stays.
///
/// Behaviour: desktop.interface.a-server-gone-ends-the-session-on-the-disconnected-screen
#[test]
fn a_world_server_gone_ends_the_session_on_the_disconnected_screen() {
    let (mut s, retail) = into_the_world_with(script_with(vec![Step::End {
        t: 10.0,
        conn: 1,
        c2s: false,
    }]));
    let screen = |r: &Rc<RefCell<Retail>>| r.borrow().screen();
    assert_eq!(screen(&retail), "play");
    assert!(
        until(&mut s, 6.0, |_| screen(&retail) == "disconnected"),
        "the disconnected screen: {}",
        screen(&retail)
    );
    assert!(s.client.is_none(), "the session ended");
    assert!(retail.borrow().play().is_none(), "the play screen gone");
    for _ in 0..60 {
        step(&mut s);
    }
    assert_eq!(screen(&retail), "disconnected", "it stays");
    assert_eq!(
        text_of(&retail, "ErrorText").as_deref(),
        Some("The connection to the server has been lost!"),
        "its line"
    );
}

/// An account the server refuses with a boot as it logs in (its line that the account is banned,
/// before the character list) is taken from connecting to the disconnected screen showing that
/// line.
///
/// Behaviour: desktop.interface.a-boot-at-login-shows-its-line-on-the-disconnected-screen
#[test]
fn a_boot_at_login_shows_its_line_on_the_disconnected_screen() {
    use auberean_client_sdk::protocol::login::Text;
    use auberean_client_sdk::protocol::text::StringInfo;
    let boot = ServerMessage::AccountBooted(Text(StringInfo {
        string_id: 0x974F_243D,
        table_id: 0x2500_0012,
        ..StringInfo::default()
    }));
    let script = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0)],
        events: vec![
            s2c(
                0.20,
                0,
                5,
                LOGIN,
                &ServerMessage::DataQuery(DataQuery {
                    region: 1,
                    name_language: 1,
                    languages: vec![1],
                }),
            ),
            c2s(0.25, 0, 5, 0x10038),
            s2c(0.30, 0, 5, LOGIN | 1, &ServerMessage::DownloadEnds(Empty)),
            s2c(0.31, 0, 1, LOGIN | 2, &boot),
        ],
    };
    script.check().expect("a well-formed script");
    let (world, client_data, assets) = data();
    let double = ServerDouble::new(script, std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let server = double.login_addr();
    let records: Arc<dyn auberean_ui::Assets + Send + Sync> = Arc::clone(assets) as _;
    let retail = Rc::new(RefCell::new(Retail::new(
        records,
        (1024, 768),
        Some(Login {
            server,
            account: "tester".into(),
            password: "t".into(),
        }),
    )));
    let host = DoubleHost {
        time: 0.0,
        double,
        inbox: VecDeque::new(),
        input_method: None,
    };
    let mut s = Shell::new(
        host,
        world,
        client_data.clone(),
        Box::new(Shared(Rc::clone(&retail))),
    );
    s.size = (1024, 768);
    past_the_intro(&mut s, &retail);
    let screen = |r: &Rc<RefCell<Retail>>| r.borrow().screen();
    assert!(
        until(&mut s, 5.0, |_| screen(&retail) == "disconnected"),
        "the disconnected screen: {}",
        screen(&retail)
    );
    for _ in 0..60 {
        step(&mut s);
    }
    assert_eq!(screen(&retail), "disconnected", "it stays");
    assert_eq!(
        text_of(&retail, "ErrorText").as_deref(),
        Some("Your account is currently banned."),
        "the boot's line"
    );
}

/// A record the server sends that the client cannot store (here a string table, with no local
/// file to keep it in) stops the client: the session ends there and the disconnected screen
/// says, in the client's own words, that it failed to save data; the screen stays.
///
/// Behaviour: desktop.interface.a-record-not-stored-ends-the-session-with-the-clients-line
#[test]
fn a_record_that_cannot_be_stored_ends_the_session_on_the_disconnected_screen_with_its_line() {
    use auberean_client_sdk::protocol::login::DataRecord;
    let unstored = ServerMessage::DataRecord(DataRecord {
        dat_file: 0,
        kind: 0x4000_0026,
        id: 0x2500_0001,
        size: 0,
        bytes: vec![1, 0, 0x25, 0, 0, 0, 0, 0],
    });
    let script = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0)],
        events: vec![
            s2c(
                0.20,
                0,
                5,
                LOGIN,
                &ServerMessage::DataQuery(DataQuery {
                    region: 1,
                    name_language: 1,
                    languages: vec![1],
                }),
            ),
            c2s(0.25, 0, 5, 0x10038),
            s2c(0.30, 0, 5, LOGIN | 1, &unstored),
        ],
    };
    script.check().expect("a well-formed script");
    let (world, client_data, assets) = data();
    let double = ServerDouble::new(script, std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let server = double.login_addr();
    let records: Arc<dyn auberean_ui::Assets + Send + Sync> = Arc::clone(assets) as _;
    let retail = Rc::new(RefCell::new(Retail::new(
        records,
        (1024, 768),
        Some(Login {
            server,
            account: "tester".into(),
            password: "t".into(),
        }),
    )));
    let host = DoubleHost {
        time: 0.0,
        double,
        inbox: VecDeque::new(),
        input_method: None,
    };
    let mut s = Shell::new(
        host,
        world,
        client_data.clone(),
        Box::new(Shared(Rc::clone(&retail))),
    );
    s.size = (1024, 768);
    past_the_intro(&mut s, &retail);
    let screen = |r: &Rc<RefCell<Retail>>| r.borrow().screen();
    assert!(
        until(&mut s, 5.0, |_| screen(&retail) == "disconnected"),
        "the disconnected screen: {}",
        screen(&retail)
    );
    assert!(s.client.is_none(), "the session ended");
    for _ in 0..60 {
        step(&mut s);
    }
    assert_eq!(screen(&retail), "disconnected", "it stays");
    assert_eq!(
        text_of(&retail, "ErrorText").as_deref(),
        Some("Failed to save data. Are you out of disk space, or are your data files read-only?"),
        "the client's line"
    );
}

/// In a window of other proportions than 4:3 the screens before the world are still laid out on a
/// 1024 by 768 screen, shown as large as the window lets it at those proportions, in its middle: a
/// click lands on the button drawn under it, and one beside the screen on nothing. The pointer shows
/// the data's default cursor. Credits opens the credits, which an interface key
/// (Escape) ends, back to the character list; Options the key map, whose Back returns there.
///
/// Behaviour: desktop.interface.the-screens-before-the-world-keep-1024-by-768-in-any-window
#[test]
fn the_screens_before_the_world_keep_1024_by_768_in_any_window() {
    let (world, client_data, assets) = data();
    let double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let server = double.login_addr();
    let records: Arc<dyn auberean_ui::Assets + Send + Sync> = Arc::clone(assets) as _;
    let window = (1920_u32, 1080_u32);
    let retail = Rc::new(RefCell::new(Retail::new(
        records,
        window,
        Some(Login {
            server,
            account: "tester".into(),
            password: "t".into(),
        }),
    )));
    let host = DoubleHost {
        time: 0.0,
        double,
        inbox: VecDeque::new(),
        input_method: None,
    };
    let mut s = Shell::new(
        host,
        world,
        client_data.clone(),
        Box::new(Shared(Rc::clone(&retail))),
    );
    s.size = window;
    past_the_intro(&mut s, &retail);
    let listed = until(&mut s, 5.0, |s| {
        s.phase() == Phase::Characters
            && s.client
                .as_ref()
                .is_some_and(|c| c.model.characters.is_some())
    });
    assert!(listed, "the character list arrives");
    assert!(
        until(&mut s, 1.0, |_| retail.borrow().screen() == "select"),
        "select: {}",
        retail.borrow().screen()
    );
    assert_eq!(
        retail.borrow().ui().display(),
        (1024, 768),
        "laid out on 1024 by 768"
    );
    let out = s.frame();
    assert_eq!(
        out.overlay.screen,
        Some((1024, 768)),
        "the screen is told to the host"
    );
    assert_eq!(
        auberean_shell::interface::fitted((1024, 768), window),
        [240.0, 0.0, 1440.0, 1080.0],
        "as high as a wide window, in its middle"
    );
    assert_eq!(
        auberean_shell::interface::fitted((1024, 768), (800, 1000)),
        [0.0, 200.0, 800.0, 600.0],
        "as wide as a tall window"
    );
    let cursor = out.overlay.cursor.clone().expect("the interface's cursor");
    assert_eq!(
        cursor.picture, 0x4100_0870,
        "the default cursor of the data"
    );
    assert_eq!(
        cursor.rgba.len(),
        (cursor.width * cursor.height * 4) as usize,
        "a whole picture"
    );
    #[allow(clippy::cast_precision_loss)]
    let scaled = |(x, y): (f32, f32)| (240.0 + x * 1080.0 / 768.0, y * 1080.0 / 768.0);
    let credits = middle(&retail, "CreditsButton");
    click(&mut s, (100.0, scaled(credits).1));
    assert_eq!(
        retail.borrow().screen(),
        "select",
        "a click beside the screen presses nothing"
    );
    click(&mut s, scaled(credits));
    assert!(
        until(&mut s, 1.0, |_| retail.borrow().screen() == "credits"),
        "Credits shows the credits: {}",
        retail.borrow().screen()
    );
    for down in [true, false] {
        s.event(&Event::Key {
            key: Key::Escape,
            down,
            repeat: false,
        });
        step(&mut s);
    }
    assert!(
        until(&mut s, 1.0, |_| retail.borrow().screen() == "select"),
        "Escape ends them: {}",
        retail.borrow().screen()
    );
    step(&mut s);
    // Options: the key map, the client's maps (the Defaults the play screen binds) by action in
    // the game's order; Back returns to the list.
    let options = middle(&retail, "OptionsButton");
    click(&mut s, scaled(options));
    assert!(
        until(&mut s, 1.0, |_| retail.borrow().screen() == "keyboard"),
        "Options shows the key map: {}",
        retail.borrow().screen()
    );
    let rows = retail.borrow().key_map_rows();
    assert_eq!(
        rows[..9],
        [
            "Examine panel.",
            "E",
            "Ctrl - F5",
            "Inventory panel.",
            "I",
            "Ctrl - F6",
            "Map panel.",
            "M",
            "Ctrl - F4"
        ]
    );
    let logout = rows.iter().position(|r| r == "Log out.").expect("log out");
    assert_eq!(rows[logout + 1], "Shift - Esc");
    let back = middle(&retail, "quit_button");
    click(&mut s, scaled(back));
    assert!(
        until(&mut s, 1.0, |_| retail.borrow().screen() == "select"),
        "Back: {}",
        retail.borrow().screen()
    );
    step(&mut s);
    let create = middle(&retail, "CreateButton");
    click(&mut s, scaled(create));
    assert!(
        until(&mut s, 1.0, |_| retail.borrow().screen() == "avatar"),
        "Create, clicked where it is drawn: {}",
        retail.borrow().screen()
    );
}

/// Press and let go a key, a frame after each.
fn press(shell: &mut Shell<'_, DoubleHost>, key: Key) {
    for down in [true, false] {
        shell.event(&Event::Key {
            key,
            down,
            repeat: false,
        });
        step(shell);
    }
}

/// Whether the element named `name` shows.
fn shows(retail: &Rc<RefCell<Retail>>, name: &str) -> bool {
    let r = retail.borrow();
    r.ui()
        .find(Token::of(name))
        .is_some_and(|h| r.ui().shown(h))
}

/// In the world the play screen runs on the frameworks: E shows the examination window; the
/// skills button shows the skills window and its tooltip turns to "Hide"; F12 hides the toolbar and
/// shows it again; the middle button turns mouse look on (the camera follows, the crosshair shows)
/// and off; the up arrow still runs the body forward.
///
/// Behaviour: desktop.interface.the-play-screen-runs-on-the-frameworks
#[test]
fn the_play_screen_runs_on_the_frameworks() {
    let (mut s, retail) = into_the_world();
    assert!(!shows(&retail, "ExaminationField"));
    press(&mut s, Key::E);
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "ExaminationField")),
        "E shows the examination window"
    );
    let skills = middle(&retail, "SkillButton");
    click(&mut s, skills);
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "SkillField")),
        "the skills button shows its window"
    );
    let tip = {
        let r = retail.borrow();
        let b = r.ui().find(Token::of("SkillButton")).expect("the button");
        r.ui().element_tooltip(b).map(str::to_owned)
    };
    assert_eq!(
        tip.as_deref(),
        Some("Hide the Skills Panel (keyboard shortcut: F7).")
    );
    press(&mut s, Key::F12);
    assert!(
        until(&mut s, 1.0, |_| !shows(&retail, "ToolbarField")),
        "F12 hides the toolbar"
    );
    press(&mut s, Key::F12);
    assert!(until(&mut s, 1.0, |_| shows(&retail, "ToolbarField")));
    for down in [true, false] {
        s.event(&Event::Button {
            button: Button::Middle,
            down,
        });
        step(&mut s);
    }
    assert!(
        until(&mut s, 1.0, |s| s.scene.camera.mouse_look),
        "the middle button turns mouse look on"
    );
    assert!(shows(&retail, "MouseLookCursorField"), "the crosshair");
    for down in [true, false] {
        s.event(&Event::Button {
            button: Button::Middle,
            down,
        });
        step(&mut s);
    }
    assert!(until(&mut s, 1.0, |s| !s.scene.camera.mouse_look));
    let before = s.scene.status.origin;
    s.event(&Event::Key {
        key: Key::ArrowUp,
        down: true,
        repeat: false,
    });
    for _ in 0..60 {
        step(&mut s);
    }
    s.event(&Event::Key {
        key: Key::ArrowUp,
        down: false,
        repeat: false,
    });
    let after = s.scene.status.origin;
    assert!(
        ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt() > 0.5,
        "the up arrow runs the body forward: {before:?} {after:?}"
    );
}

/// A man of the hall, standing ahead of the player and to its right.
const MAN: u64 = 0x2130_0000_0000_0002;

/// The script with a man standing in the hall near the player.
fn with_a_man() -> Script {
    let man = CreateObject {
        id: MAN,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 8,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: HALL_AT.x + 1.5,
                        y: HALL_AT.y + 3.0,
                        z: HALL_AT.z,
                    },
                    rotation: Quat {
                        w: 0.0,
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    },
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    // The double sends no object's qualities, so the man is not marked selectable: the player
    // is given the kind bits that let it select anything.
    let anything = ServerMessage::PlayerQualities(PlayerQualities(Qualities {
        header: Qualities::INTS,
        ints: Table::from_inserted(
            SMALL,
            [(258, 80), (263, 100), (259, 55), (264, 120), (20, 0x3F)],
        ),
        ..Qualities::default()
    }));
    script_with(vec![
        s2c(6.43, 1, 1, WORLD | 7, &ServerMessage::CreateObject(man)),
        s2c(6.44, 1, 1, WORLD | 8, &anything),
    ])
}

/// Where a drawn body's chest shows in the window (the middle of its box, three quarters up),
/// once the world drew it.
fn on_screen(s: &Shell<'_, DoubleHost>, id: u64) -> Option<(f32, f32)> {
    let seen = &s.scene.seen;
    let body = seen.body(id)?;
    let (lo, hi) = body.bounds?;
    let mid = Vec3 {
        x: (lo.x + hi.x) / 2.0,
        y: (lo.y + hi.y) / 2.0,
        z: lo.z + (hi.z - lo.z) * 0.75,
    };
    let (x, y, _) = seen.camera?.project(body.to_block(mid), 1024.0, 768.0)?;
    Some((x, y))
}

/// The requests the interface asked of the shell since the test began.
fn asked() -> Vec<Request> {
    ASKED.with(|a| a.borrow().clone())
}

/// The screens before the world ask for their music, each stopping the one before over two
/// seconds and keeping its own across the screens; the play screen stops it as it is made.
///
/// Behaviour: audio.screens.the-screens-play-their-music-and-the-world-stops-it
#[test]
fn the_screens_ask_for_their_music_and_the_play_screen_stops_it() {
    use auberean_shell::interface::{SoundOrder, SoundSlot};
    ASKED.with(|a| a.borrow_mut().clear());
    let (_s, _retail) = into_the_world();
    let sounds: Vec<SoundOrder> = asked()
        .into_iter()
        .filter_map(|r| match r {
            Request::Sound(s) => Some(s),
            _ => None,
        })
        .collect();
    let stop = SoundOrder::Stop {
        slot: SoundSlot::Screens,
        fade: 2.0,
    };
    let played: Vec<u32> = sounds
        .iter()
        .filter_map(|s| match s {
            SoundOrder::Play {
                data,
                slot: Some(SoundSlot::Screens),
            } => Some(data.id),
            _ => None,
        })
        .collect();
    assert!(!played.is_empty(), "the screens play music: {sounds:?}");
    assert!(
        played.iter().all(|id| id >> 24 == 0x2C || id >> 24 == 0x2A),
        "each a sound or music setting: {played:x?}"
    );
    assert_eq!(
        sounds.first(),
        Some(&stop),
        "the intro stops what played first"
    );
    assert_eq!(
        sounds.last(),
        Some(&stop),
        "the play screen stops the music"
    );
}

/// The element the pointer is over at `(x, y)` is the field that lies over the world.
fn over_the_world(retail: &Rc<RefCell<Retail>>, (x, y): (f32, f32)) -> bool {
    let r = retail.borrow();
    let ui = r.ui();
    #[allow(clippy::cast_possible_truncation)]
    let hit = ui.hit(x as i32, y as i32);
    hit.is_some() && hit == ui.find(Token::of("ItemSelectionField"))
}

/// A click on a man standing in the world lands on the field that lies over the world, and at
/// the next tick after the world says what lay under the pointer the man is selected through the
/// player's scripts (the player's kind lets it select anything: the double marks no object
/// selectable); a click on empty ground selects nothing.
///
/// Behaviour: desktop.interface.a-click-in-the-world-selects-what-lies-under-it
#[test]
fn a_click_in_the_world_selects_what_lies_under_it() {
    let (mut s, retail) = into_the_world_with(with_a_man());
    assert!(
        until(&mut s, 3.0, |s| on_screen(s, MAN).is_some()),
        "the man is drawn: {:?}",
        s.scene.seen.bodies.iter().map(|b| b.id).collect::<Vec<_>>()
    );
    // Out of portal space the world's view settles to its own field of view.
    let settled = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .seen
            .camera
            .is_some_and(|c| (c.fov_y - s.scene.fov).abs() < 0.01)
    };
    assert!(until(&mut s, 10.0, settled), "the view settles");
    let at = on_screen(&s, MAN).expect("on the screen");
    assert!(
        over_the_world(&retail, at),
        "the field lies over the world at {at:?}"
    );
    let before = asked().len();
    click(&mut s, at);
    let picked = until(&mut s, 1.0, |_| {
        asked()[before..].contains(&Request::Act(Action::Select { target: MAN }))
    });
    assert!(picked, "the man is selected: {:?}", &asked()[before..]);
    assert!(
        asked()[before..].contains(&Request::PickWorld {
            x: at.0.trunc(),
            y: at.1.trunc()
        }),
        "the world was asked what lay under the pointer"
    );

    // Empty ground, away from the man and every panel.
    let ground = (150.0, 420.0);
    assert!(over_the_world(&retail, ground));
    let before = asked().len();
    click(&mut s, ground);
    let none = until(&mut s, 1.0, |_| {
        asked()[before..].contains(&Request::Act(Action::Select { target: 0 }))
    });
    assert!(none, "nothing is selected: {:?}", &asked()[before..]);
}

/// A man named in his description, selected, then renamed by the server (a visual update of his
/// name quality, stamped next after his creation): the player's scripts tell the panels his radar
/// look changed, the interface asks his name again and the panels hear the notice with the world
/// that has it, so the selection's name over him is the new one.
///
/// Behaviour: desktop.interface.a-renamed-selection-is-named-anew
#[test]
fn a_renamed_selection_is_named_anew() {
    use auberean_client_sdk::protocol::quality::VisualQuality;
    use auberean_client_sdk::protocol::types::ObjectStamp;
    /// The visual messages' ordering type.
    const VISUAL: u64 = 0x0200_0000_0000_0000;
    let mut script = with_a_man();
    for e in &mut script.events {
        if let Step::Msg { msg, .. } = e {
            if msg.mtype == 0x10058 {
                if let Ok(ServerMessage::CreateObject(mut man)) = ServerMessage::decode(&msg.raw) {
                    if man.id == MAN {
                        // A human's scripts, which give his name.
                        man.weenie = player_weenie();
                        set_string_info(
                            &mut man.weenie,
                            NAME_QUALITY,
                            StringInfo::literal("Aldric"),
                        );
                        msg.raw = ServerMessage::CreateObject(man).encode();
                    }
                }
            }
        }
    }
    let rename = ServerMessage::UpdateStringInfoVisual(VisualQuality {
        object: ObjectStamp {
            id: MAN,
            instance_stamp: 1,
            other_stamp: 9,
        },
        quality: NAME_QUALITY,
        value: StringInfo::literal("Corpse of Aldric"),
    });
    script.events.push(s2c(40.0, 1, 1, VISUAL | 9, &rename));
    let (mut s, retail) = into_the_world_running(script, data_with_scripts());
    assert!(
        until(&mut s, 3.0, |s| on_screen(s, MAN).is_some()),
        "the man is drawn"
    );
    let settled = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .seen
            .camera
            .is_some_and(|c| (c.fov_y - s.scene.fov).abs() < 0.01)
    };
    assert!(until(&mut s, 10.0, settled), "the view settles");
    let at = on_screen(&s, MAN).expect("on the screen");
    click(&mut s, at);
    let named = |want: &'static str| {
        let r = Rc::clone(&retail);
        move |_: &Shell<'_, DoubleHost>| text_of(&r, "SelectionName").as_deref() == Some(want)
    };
    assert!(
        until(&mut s, 2.0, named("Aldric")),
        "the selection's name: {:?}",
        text_of(&retail, "SelectionName")
    );
    assert!(
        until(&mut s, 45.0, named("Corpse of Aldric")),
        "the selection's name after the rename: {:?}",
        text_of(&retail, "SelectionName")
    );
}

/// A bow the player holds in its left hand.
const BOW: u64 = 0x8130_0000_0000_0B0B;
/// The look a recorded 2017 player's bow was made from.
const BOW_LOOK: u32 = 0x1F00_02C0;
/// The qualities the bow's description carries: whether it may be selected, what holds it.
const SELECTABLE: u32 = 624;
const CONTAINER: u32 = 261;

/// The script with the player holding a bow in its left hand, as the 2017 servers described a
/// wielded bow: it may be selected, and its holder is the player.
fn with_a_bow_held() -> Script {
    let mut weenie = WeenieDesc::default();
    assert!(set_bool(&mut weenie, SELECTABLE, 1));
    assert!(set_instance_id(&mut weenie, CONTAINER, PLAYER));
    let bow = CreateObject {
        id: BOW,
        visual: VisualDesc {
            bits: VisualDesc::PARENT,
            parent: BOW_LOOK,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::PARENT,
            instance_stamp: 1,
            visual_ordering_stamp: 8,
            parent: ParentLink {
                parent: PLAYER,
                location: 2,
                stamp: 1,
            },
            ..PhysicsDesc::default()
        },
        weenie,
    };
    script_with(vec![s2c(
        6.43,
        1,
        1,
        WORLD | 7,
        &ServerMessage::CreateObject(bow),
    )])
}

/// A point of the window where the world's pick finds object `id` (scanned over the box its
/// drawn body fills, at whole pixels, as a click is), once the world drew it.
fn picks(s: &Shell<'_, DoubleHost>, id: u64) -> Option<(f32, f32)> {
    let seen = &s.scene.seen;
    let body = seen.body(id)?;
    let camera = seen.camera?;
    let mut lo = (f32::MAX, f32::MAX);
    let mut hi = (f32::MIN, f32::MIN);
    for c in body.corners(body.bounds?) {
        let (x, y, _) = camera.project(c, 1024.0, 768.0)?;
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    let mut y = lo.1.max(0.0).floor();
    while y <= hi.1.min(767.0) {
        let mut x = lo.0.max(0.0).floor();
        while x <= hi.0.min(1023.0) {
            if s.scene.pick_world((x, y), (1024, 768)) == Some(id) {
                return Some((x, y));
            }
            x += 2.0;
        }
        y += 2.0;
    }
    None
}

/// Every kind of request the play screen makes reaches what carries it out: being brought back,
/// turning an offer down, getting down from a mount, keeping the options, yes to the server's
/// question, the groove level, the world hidden and shown, what the status bar covers, and the
/// examination panel opened.
///
/// Behaviour: desktop.interface.what-the-play-screen-asks-is-carried-out
#[test]
fn what_the_play_screen_asks_is_carried_out() {
    use auberean_shell::interface::SoundOrder;
    use auberean_ui_game::gameplay::Request as R;
    let (mut s, retail) = into_the_world_with(with_a_man());
    assert!(until(&mut s, 3.0, |s| on_screen(s, MAN).is_some()), "in");
    let carried = |r: R| {
        let mut out = Vec::new();
        retail.borrow_mut().carry_out(vec![r], &mut out);
        out
    };
    for (r, a) in [
        (R::Resurrect, Action::Resurrect),
        (R::DeclineResurrection, Action::DeclineResurrection),
        (R::Dismount, Action::Dismount),
        (R::KeepOptions, Action::KeepOptions),
        (
            R::Confirm {
                objects: [7, 8],
                value: 9,
            },
            Action::ApplyTotem {
                target: 7,
                source: 8,
                effect: 9,
            },
        ),
    ] {
        assert_eq!(carried(r), vec![Request::Act(a)]);
    }
    assert_eq!(
        carried(R::GrooveLevel(3)),
        vec![Request::Sound(SoundOrder::Groove(3))]
    );
    assert_eq!(
        carried(R::ShowWorld(false)),
        vec![Request::ShowWorld(false)]
    );
    assert!(carried(R::PrecludeFromTop(40)).is_empty());
    assert_eq!(retail.borrow().preclude().0, 40);
    assert!(!shows(&retail, "ExaminationField"));
    assert!(carried(R::OpenExaminationPanel).is_empty());
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "ExaminationField")),
        "the examination panel opens"
    );
}

/// What a body holds is seen and picked as itself: a click on the bow in the player's hand
/// selects the bow, and a double press on it, an item the player holds, selects it without
/// using it.
///
/// Behaviour: desktop.interface.what-a-body-holds-is-picked-and-selected-as-itself
#[test]
fn what_a_body_holds_is_picked_and_selected_as_itself() {
    let (mut s, retail) = into_the_world_with(with_a_bow_held());
    let settled = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .seen
            .camera
            .is_some_and(|c| (c.fov_y - s.scene.fov).abs() < 0.01)
    };
    assert!(until(&mut s, 10.0, settled), "the view settles");
    let mut at = None;
    assert!(
        until(&mut s, 3.0, |s| {
            at = picks(s, BOW);
            at.is_some()
        }),
        "the bow is drawn where a click finds it: {:?}",
        s.scene.seen.bodies.iter().map(|b| b.id).collect::<Vec<_>>()
    );
    let at = at.expect("found");
    assert!(over_the_world(&retail, at), "the field lies over the bow");
    let before = asked().len();
    click(&mut s, at);
    click(&mut s, at);
    until(&mut s, 1.0, |_| false);
    let asked_now = asked()[before..].to_vec();
    assert!(
        asked_now.contains(&Request::Act(Action::Select { target: BOW })),
        "the bow is selected: {asked_now:?}"
    );
    assert!(
        !asked_now.contains(&Request::Act(Action::UseSelected)),
        "a double press on what the player holds does not use it: {asked_now:?}"
    );
}

/// Running while mouse look turns it, the player's body holds its place before the camera from
/// frame to frame: as far from the camera, and facing the way the camera faces, though its physics
/// ticks only every sixth frame. (Well clear of the hall's walls; near them the camera moves in.)
///
/// Behaviour: desktop.interface.the-body-holds-its-place-before-the-camera-in-mouse-look
#[test]
fn the_body_holds_its_place_before_the_camera_in_mouse_look() {
    let (mut s, _retail) = into_the_world();
    for _ in 0..120 {
        step(&mut s);
    }
    s.event(&Event::Button {
        button: Button::Right,
        down: true,
    });
    for _ in 0..10 {
        step(&mut s);
    }
    assert!(s.scene.camera.mouse_look, "the right button held looks");
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    for _ in 0..60 {
        step(&mut s);
    }
    let mut frames = Vec::new();
    for _ in 0..70 {
        s.event(&Event::Motion { dx: 6.0, dy: 0.0 });
        step(&mut s);
        let seen = &s.scene.seen;
        let camera = seen.camera.expect("the world is drawn");
        let body = seen.body(PLAYER).expect("the body is drawn");
        let (dx, dy) = (
            body.origin.x - camera.position.x,
            body.origin.y - camera.position.y,
        );
        let away = (dx * dx + dy * dy).sqrt();
        let facing = (camera.heading - body.heading + 540.0).rem_euclid(360.0) - 180.0;
        frames.push((away, facing, body.heading));
    }
    let turned = frames[69].2 - frames[30].2;
    assert!(turned.abs() > 45.0, "the body turned: {turned}");
    for w in frames[30..].windows(2) {
        assert!(
            (w[1].0 - w[0].0).abs() < 0.05,
            "as far from the camera: {:?} then {:?}",
            w[0],
            w[1]
        );
        assert!(w[1].1.abs() < 0.5, "facing with the camera: {:?}", w[1]);
    }
}

/// In mouse look the left button runs the body forward while it is held, though the field that
/// lies over the world takes the press: the world's controls see the button too.
///
/// Behaviour: desktop.interface.the-left-button-runs-forward-in-mouse-look-over-the-world
#[test]
fn the_left_button_runs_forward_in_mouse_look_over_the_world() {
    let (mut s, retail) = into_the_world();
    for down in [true, false] {
        s.event(&Event::Button {
            button: Button::Middle,
            down,
        });
        step(&mut s);
    }
    assert!(
        until(&mut s, 1.0, |s| s.scene.camera.mouse_look),
        "the middle button turns mouse look on"
    );
    let at = (512.0, 300.0);
    s.event(&Event::Pointer { x: at.0, y: at.1 });
    step(&mut s);
    assert!(over_the_world(&retail, at), "the field lies over the world");
    let before = s.scene.status.origin;
    s.event(&Event::Button {
        button: Button::Left,
        down: true,
    });
    for _ in 0..60 {
        step(&mut s);
    }
    let held = s.scene.status.origin;
    s.event(&Event::Button {
        button: Button::Left,
        down: false,
    });
    for _ in 0..30 {
        step(&mut s);
    }
    let rested = s.scene.status.origin;
    for _ in 0..30 {
        step(&mut s);
    }
    let after = s.scene.status.origin;
    let moved = |a: Vec3, b: Vec3| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    assert!(
        moved(before, held) > 0.5,
        "the left button runs the body: {before:?} {held:?}"
    );
    assert!(
        moved(rested, after) < 0.01,
        "let go, the body stops: {rested:?} {after:?}"
    );
}

/// In the world the interface draws every panel in front of the world: the world fills the
/// window behind every element, so a panel's own viewport (the paper doll's, shown with the
/// inventory, deeper than the toolbar and shallower than the chat window and the status bar) puts
/// nothing behind it. A viewport's scene stands at its viewport's depth: the chat window and the
/// status bar are drawn in the picture over the world and under the figure, the toolbar in the
/// picture in front of it.
///
/// Behaviour: desktop.interface.every-panel-is-drawn-in-front-of-the-world
#[test]
fn every_panel_is_drawn_in_front_of_the_world() {
    let (mut s, retail) = into_the_world();
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "ChatWindow1")),
        "the chat window shows"
    );
    press(&mut s, Key::I);
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "PaperDollViewport")),
        "I opens the inventory and its paper doll"
    );
    let (chat_z, viewports) = {
        let r = retail.borrow();
        let ui = r.ui();
        let chat = ui.find(Token::of("ChatWindow1")).expect("the chat window");
        let z = ui.node(chat).map_or(0, |n| n.z);
        let v: Vec<i32> = auberean_ui::draw::viewports(ui)
            .iter()
            .filter_map(|(h, _)| ui.node(*h).map(|n| n.z))
            .collect();
        (z, v)
    };
    assert!(
        viewports.iter().any(|v| *v < chat_z),
        "a panel's viewport shallower than the chat window: {viewports:?}, chat at {chat_z}"
    );
    assert_eq!(
        retail.borrow().scene_depth(),
        None,
        "nothing behind the world"
    );
    let out = s.frame();
    assert!(out.overlay.behind.is_none(), "no picture behind the world");
    let middle = out
        .overlay
        .middle
        .clone()
        .expect("the picture under the figure");
    let front = out.overlay.front.clone().expect("the picture in front");
    let r = retail.borrow();
    let ui = r.ui();
    let drawn_in = |picture: &auberean_shell::interface::Canvas, name: &str| {
        let h = ui.find(Token::of(name)).expect("made");
        let rect = ui.screen_rect(h).expect("placed");
        (rect.y0..=rect.y1).any(|y| {
            (rect.x0..=rect.x1).any(|x| {
                #[allow(clippy::cast_sign_loss)]
                let i = (y as usize * picture.width as usize + x as usize) * 4;
                picture.bgra.get(i + 3).is_some_and(|a| *a != 0)
            })
        })
    };
    for name in ["ChatWindow1", "StatusbarField"] {
        assert!(drawn_in(&middle, name), "{name} is drawn under the figure");
    }
    assert!(
        drawn_in(&front, "ToolbarField"),
        "the toolbar is drawn over the figure"
    );
    assert!(
        drawn_in(&front, "PaperDollViewport"),
        "the inventory's window is drawn over the figure"
    );
}

/// The inventory's paper doll shows the player's figure as the player looks, standing still in
/// its first pose in the paper doll's scene, seen straight on by the scene's camera: the camera
/// sees the figure's drawn height (the box around its parts as read, times its scale; less for the
/// races drawn shorter), the figure's own place half that height below the camera, so its feet
/// meet the viewport's foot; the scene stands in the viewport's rectangle.
///
/// Behaviour: ui.inventory.the-paper-doll-shows-the-player-seen-straight-on
#[test]
fn the_paper_doll_shows_the_player_seen_straight_on() {
    let (mut s, retail) = into_the_world();
    let none = s.frame();
    assert!(
        none.overlay.scenes.is_empty(),
        "no scene before the inventory opens"
    );
    press(&mut s, Key::I);
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "PaperDollViewport")),
        "I opens the inventory"
    );
    let doll = s
        .frame()
        .overlay
        .scenes
        .iter()
        .find(|v| v.scene == 0x2100_0000)
        .cloned()
        .expect("the paper doll's scene");
    let rect = {
        let r = retail.borrow();
        let ui = r.ui();
        let v = ui.find(Token::of("PaperDollViewport")).expect("made");
        ui.screen_rect(v).expect("placed")
    };
    #[allow(clippy::cast_precision_loss)]
    let want = [
        rect.x0 as f32,
        rect.y0 as f32,
        rect.x1 as f32,
        rect.y1 as f32,
    ];
    assert_eq!(doll.rect, want, "in the viewport's rectangle");
    let records = &data().0.records;
    let player = s
        .client
        .as_ref()
        .and_then(|c| c.model.object(PLAYER))
        .expect("the player");
    let look = auberean_interface::doll::look_of(records, &player.visual);
    let (lo, hi) = auberean_scene::view::figure_box(records, &look).expect("a figure");
    let height = hi.z - lo.z;
    assert!(
        (1.5..3.0).contains(&height),
        "the man's figure draws about two metres: {height}"
    );
    let c = doll.camera;
    assert!(c.orthographic, "seen straight on");
    assert!(
        (c.vertical_scale - height).abs() < 1e-4,
        "the camera sees the figure's height: {} against {height}",
        c.vertical_scale
    );
    assert!(
        (c.heading - 180.0).abs() < 1e-3,
        "looking south: {}",
        c.heading
    );
    assert_eq!((c.near, c.far), (-20.0, 20.0), "the scene's own distances");
    assert_eq!(doll.figures.len(), 1, "one figure");
    let f = &doll.figures[0];
    assert_eq!(f.look, look, "as the player looks");
    assert!(f.still, "standing still");
    assert!(
        (f.origin.z + 0.5 * height).abs() < 1e-4,
        "half the height below the camera: {:?}",
        f.origin
    );
}

/// The paper doll's figure answers a search under the pointer with the part it meets: the doll's
/// scene is searched straight along its view from the pointer (taken from the viewport's corner),
/// and the mesh met there names the kind it is. A search on the head meets a mesh the doll keeps
/// for the head's place, one on the middle of the figure a mesh of the torso's, and one beside the
/// figure meets nothing.
///
/// Behaviour: ui.inventory.the-paper-doll-finds-the-part-under-the-pointer
#[test]
fn the_paper_doll_finds_the_part_under_the_pointer() {
    use auberean_ui_game::inventory::paper_doll::mesh_type_to_location;
    let (mut s, retail) = into_the_world();
    press(&mut s, Key::I);
    assert!(
        until(&mut s, 1.0, |_| shows(&retail, "PaperDollViewport")),
        "I opens the inventory"
    );
    let doll = s
        .frame()
        .overlay
        .scenes
        .iter()
        .find(|v| v.scene == 0x2100_0000)
        .cloned()
        .expect("the paper doll's scene");
    let (w, h) = (doll.rect[2] - doll.rect[0], doll.rect[3] - doll.rect[1]);
    let mut search = |x: f32, y: f32| {
        let mut asked = doll.clone();
        asked.pick = Some((x, y));
        let _ = s.scene.view_frames(&[asked], 0.0);
        s.scene.views.picked.first().expect("answered").1
    };
    let head = search(0.5 * w, 0.22 * h).expect("the head is under the pointer");
    let middle = search(0.5 * w, 0.4 * h).expect("the figure is under the pointer");
    let beside = search(2.0, 0.4 * h);
    assert_eq!(head.figure, 0, "the player's figure");
    assert_eq!(
        mesh_type_to_location(head.mesh_type),
        1,
        "the head's place: {:#x}",
        head.mesh_type
    );
    assert_eq!(
        mesh_type_to_location(middle.mesh_type),
        2,
        "the torso's place: {:#x}",
        middle.mesh_type
    );
    assert!(beside.is_none(), "nothing beside the figure: {beside:?}");
}

/// The servers name the player in its own description, as every 2017 player's creation did: its
/// scripts write the player's own name from it (not the name quality's absence).
///
/// Behaviour: desktop.interface.the-players-scripts-write-its-own-name
#[test]
fn the_players_scripts_write_its_own_name() {
    let (mut s, _retail) = into_the_world_running(script(), data_with_scripts());
    let named =
        |s: &mut Shell<'_, DoubleHost>| s.client.as_mut().and_then(|c| c.object_name(PLAYER, 1));
    assert!(
        until(&mut s, 5.0, |s| s
            .client
            .as_ref()
            .is_some_and(|c| c.model.player_object().is_some())),
        "the player stands"
    );
    assert_eq!(named(&mut s).as_deref(), Some(NAME));
}

/// Enter starts a chat line: the host's input method is turned on over the part of the chat entry
/// that shows, in the window's pixels; Enter again ends the line and turns it off.
///
/// Behaviour: desktop.interface.the-input-method-writes-over-the-chat-entry-while-it-takes-the-keys
#[test]
fn the_input_method_writes_over_the_chat_entry_while_it_takes_the_keys() {
    let (mut s, retail) = into_the_world();
    assert!(
        until(&mut s, 2.0, |_| retail.borrow().vitals_shown().is_some()),
        "in the world"
    );
    assert_eq!(s.host.input_method, None, "off before a line is started");
    press(&mut s, Key::Enter);
    step(&mut s);
    let entry = {
        let r = retail.borrow();
        let ui = r.ui();
        let e = ui.find(Token::of("ChatEntry")).expect("the entry");
        assert_eq!(ui.focus(), Some(e), "the entry takes the keys");
        ui.screen_clip_rect(e).expect("the entry shows")
    };
    let area = s
        .host
        .input_method
        .expect("on while the entry takes the keys");
    assert_eq!(
        (area.x, area.y),
        (entry.x0, entry.y0),
        "at the entry's corner: the screens are drawn 1:1 in a 1024 by 768 window"
    );
    assert_eq!(
        (area.width, area.height),
        (
            u32::try_from(entry.x1 - entry.x0).unwrap_or(0),
            u32::try_from(entry.y1 - entry.y0).unwrap_or(0)
        ),
        "over the part of the entry that shows"
    );
    press(&mut s, Key::Enter);
    step(&mut s);
    assert_eq!(s.host.input_method, None, "the line ended turns it off");
}

/// The play screen's kept places go into a table of saved places as the client keeps them (each
/// panel under its layout and its name, its corners and whether it shows) and come back from one:
/// a panel moved after the table was made stands where the table keeps it once put back.
///
/// Behaviour: desktop.interface.the-play-screens-kept-places-go-into-a-table-and-come-back
#[test]
fn the_play_screens_kept_places_go_into_a_table_and_come_back() {
    use auberean_client_sdk::script::ui_locations::UiLocations;
    use auberean_interface::places;
    let (_s, retail) = into_the_world();
    let mut r = retail.borrow_mut();
    let frame = r
        .play()
        .map(auberean_ui_game::gameplay::GamePlay::frame)
        .expect("the play screen");
    let kept = r.ui().frame_kept_places(frame);
    assert!(
        kept.len() >= 3,
        "the play screen's kept panels: {}",
        kept.len()
    );
    let table = UiLocations::read(&places::table(&kept).write()).expect("the table reads");
    for ((layout, name), l) in &kept {
        let want = [
            l.x0.to_bits(),
            l.y0.to_bits(),
            l.x1.to_bits(),
            l.y1.to_bits(),
            u32::from(l.visible),
        ];
        assert_eq!(table.location(*layout, name.raw()), Some(want), "{name:?}");
    }
    let ((_, name), l) = kept[0];
    let ui = r.ui_mut();
    let panel = ui.find(name).expect("the panel");
    let at = ui.screen_origin(panel).expect("placed");
    ui.move_to(panel, at.0 + 37, at.1 + 21);
    assert_ne!(ui.screen_origin(panel), Some(at));
    assert!(ui.frame_apply_locations(frame, &places::kept(&table)));
    // The corner the table keeps, in the display's pixels (clip space read back cuts toward
    // zero, so it may stand a pixel from where it was read).
    let (w, h) = ui.display();
    let want = auberean_ui::location::clip_to_pixel(l.x0, l.y0, w, h);
    assert_eq!(ui.screen_origin(panel), Some(want), "put back");
    assert!(
        (want.0 - at.0).abs() <= 1 && (want.1 - at.1).abs() <= 1,
        "{want:?} {at:?}"
    );
}
