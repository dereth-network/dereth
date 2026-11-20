//! A session for the tests: a server double with an account holding one character, which enters
//! the human training hall; a host carrying the double's datagrams on a clock set by hand; and the
//! shell over both interfaces, each shared with the test so it can look at them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{
    CharacterList, DataQuery, Empty, PlayerObject, PlayerQualities, Text,
};
use auberean_client_sdk::protocol::physics::CreateObject;
use auberean_client_sdk::protocol::table::{Table, SMALL};
use auberean_client_sdk::protocol::text::{NarrowString, StringInfo, WideString};
use auberean_client_sdk::protocol::types::{
    CharacterIdentity, PhysicsDesc, Placement, Qualities, VisualDesc, WeenieDesc,
};
use auberean_client_sdk::protocol::ServerMessage;
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::runtime::description::set_string_info;
use auberean_client_sdk::ClientData;
use auberean_horizon::{maker, Horizon, Shared, Start};
use auberean_interface::{Login, Retail};
use auberean_primitives::{Frame, Quat, Vec3};
use auberean_scene::WorldData;
use auberean_shell::input::{Button, Event};
use auberean_shell::interface::{Choice, Interface, Phase};
use auberean_shell::{Host, Shell};
use auberean_testkit::double::script::{ConnSpec, Event as Step, Msg, Script};
use auberean_testkit::double::ServerDouble;
use auberean_ui::DatAssets;

pub const PLAYER: u64 = 0x2130_0000_0000_0001;
const HALL_CELL: u32 = 0x0298_0109;
const HALL_AT: Vec3 = Vec3 {
    x: 59.0,
    y: 240.0,
    z: -44.6,
};
const HUMAN_MALE: u32 = 0x1F00_0023;
pub const NAME: &str = "Rowan";
/// The world's name, as the login names it.
pub const WORLD_NAME: &str = "Eastwatch";
const NAME_QUALITY: u32 = 0x100;
pub const LOGIN: u64 = 0x2300_0000_0000_0000;
pub const WORLD: u64 = 0x0300_0000_0000_0000;

type Data = (WorldData, ClientData, Arc<DatAssets>);

/// The world, the client's data with the player's scripts linked, and the interface's records,
/// opened once.
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
        let program =
            auberean_client_sdk::runtime::link_scripts(&portal).expect("the library links");
        let data = ClientData {
            files: DataFiles {
                files: vec![Arc::clone(&portal), cell],
                language: ENGLISH,
            },
            scripts: Some((portal, Arc::new(program))),
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

pub fn s2c(t: f64, conn: usize, queue: u16, blob_id: u64, m: &ServerMessage) -> Step {
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

pub fn c2s(t: f64, conn: usize, queue: u16, mtype: u32) -> Step {
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

/// The player's object as the servers describe it: its script package and entity template, its
/// physics type, and its name.
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
    set_string_info(
        &mut w,
        NAME_QUALITY,
        StringInfo::literal(&format!("{NAME} [M]")),
    );
    w
}

/// The account's list: its one character, a human man of the body creation makes him from.
pub fn character_list() -> CharacterList {
    use auberean_world::chargen;
    let body = chargen::body_entity(chargen::HUMAN, chargen::MALE).expect("a human man's body");
    CharacterList {
        characters: vec![CharacterIdentity {
            id: PLAYER,
            name: WideString(NAME.encode_utf16().collect()),
            seconds_greyed_out: 0,
            visual: VisualDesc {
                bits: VisualDesc::DID | VisualDesc::PARENT,
                did: HUMAN_MALE,
                parent: body,
                ..VisualDesc::default()
            },
        }],
        deleted: Vec::new(),
        status: 0,
        slots: 6,
        account: NarrowString::from("tester"),
        uses_chat_service: 1,
        has_expansion: 0,
        unknown: 0,
    }
}

/// A creature that may be selected, in the hall `ahead` meters before where the player stands,
/// its kind told after it: the steps for [`Session::going_on`] from `t` seconds, its messages
/// numbered from `n`.
#[allow(dead_code)] // the gpu tests', which take this file too
pub fn creature_ahead(id: u64, ahead: f32, t: f64, n: u64) -> Vec<Step> {
    use auberean_client_sdk::protocol::quality::VisualQuality;
    use auberean_client_sdk::protocol::types::ObjectStamp;
    use auberean_client_sdk::runtime::description::set_bool;
    let mut weenie = WeenieDesc::default();
    assert!(set_bool(&mut weenie, 624, 1), "selectable");
    set_string_info(&mut weenie, NAME_QUALITY, StringInfo::literal("Drudge"));
    let creature = CreateObject {
        id,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: HALL_AT.x,
                        y: HALL_AT.y + ahead,
                        z: HALL_AT.z,
                    },
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
        weenie,
    };
    // A creature's kind bits, a quality of its own.
    let kind = ServerMessage::UpdateIntVisual(VisualQuality {
        object: ObjectStamp {
            id,
            instance_stamp: 1,
            other_stamp: 2,
        },
        quality: 20,
        value: 0x4F,
    });
    vec![
        s2c(t, 1, 1, WORLD | n, &ServerMessage::CreateObject(creature)),
        s2c(t + 0.01, 1, 1, WORLD | (n + 1), &kind),
    ]
}

/// An account with one character, which enters the world into the hall, and `others` (their ids
/// and names) beside it, each of the same body.
fn script(more: Vec<Step>, others: &[(u64, &str)]) -> Script {
    let mut list = character_list();
    let first = list.characters[0].clone();
    list.characters
        .extend(others.iter().map(|(id, name)| CharacterIdentity {
            id: *id,
            name: WideString(name.encode_utf16().collect()),
            ..first.clone()
        }));
    let player = CreateObject {
        id: PLAYER,
        weenie: player_weenie(),
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
        s2c(
            0.32,
            0,
            1,
            LOGIN | 3,
            &ServerMessage::WorldName(Text(StringInfo::literal(WORLD_NAME))),
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
    events.extend(more);
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 6.3)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

/// The host: the double's datagrams on a clock set by hand, the preferences file it keeps, the
/// player's folder when the test gives it one, and the pad as the test holds it.
pub struct DoubleHost {
    pub time: f64,
    double: ServerDouble,
    inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    pub settings: Option<String>,
    pub files: std::collections::HashMap<String, Vec<u8>>,
    pub folder: Option<Folder>,
    pub pad: Option<auberean_shell::pad::PadState>,
    /// Each text put on the system's clipboard.
    pub copied: Vec<String>,
}

/// The player's folder for a test: files by name behind a lock the test keeps a handle to.
#[derive(Clone, Default)]
pub struct Folder(pub Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>);

impl auberean_client_sdk::script::local::LocalSystem for Folder {
    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        self.0
            .lock()
            .unwrap()
            .insert(file.to_owned(), bytes.to_vec());
        true
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(file).cloned()
    }
}

fn me() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 50_000))
}

impl Host for DoubleHost {
    fn now(&self) -> f64 {
        self.time
    }
    fn set_clipboard_text(&mut self, text: &str) {
        self.copied.push(text.to_owned());
    }
    fn local_system(
        &self,
    ) -> Option<Box<dyn auberean_client_sdk::script::local::LocalSystem + Send>> {
        self.folder
            .clone()
            .map(|f| Box::new(f) as Box<dyn auberean_client_sdk::script::local::LocalSystem + Send>)
    }
    fn gamepad(&mut self) -> Option<auberean_shell::pad::PadState> {
        self.pad
    }
    fn load_file(&self, name: &str) -> Option<Vec<u8>> {
        self.files.get(name).cloned()
    }
    fn save_file(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.files.insert(name.to_owned(), bytes.to_vec());
        true
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
        self.settings.clone()
    }
    fn save_settings(&mut self, text: &str) {
        self.settings = Some(text.to_owned());
    }
}

/// The shell, and both interfaces as the test sees them.
pub struct Session {
    pub shell: Shell<'static, DoubleHost>,
    pub retail: Rc<RefCell<Retail>>,
    /// Horizon, once it was first chosen.
    pub horizon: Rc<RefCell<Option<Rc<RefCell<Horizon>>>>>,
}

impl Session {
    /// A session over the script, its preferences file holding `settings`, with the game's own
    /// interface going into the world as the character the account holds once the list comes.
    pub fn new(settings: Option<&str>) -> Self {
        Self::build(settings, true, Vec::new(), &[])
    }

    /// A session as [`Session::new`], the server going on with `more` once the player is in the
    /// world (from 8 s on the recording's clock, each step held back by the client message before
    /// it that a server answers).
    pub fn going_on(settings: Option<&str>, more: Vec<Step>) -> Self {
        Self::build(settings, true, more, &[])
    }

    /// A session as [`Session::new`], with neither interface entering the world by itself: the
    /// player chooses the character.
    pub fn choosing(settings: Option<&str>) -> Self {
        Self::build(settings, false, Vec::new(), &[])
    }

    /// A session as [`Session::choosing`], the account also holding `others` (their ids and
    /// names), each of the same body as the first. Only the graphics tests ask for it.
    #[allow(dead_code)]
    pub fn choosing_among(settings: Option<&str>, others: &[(u64, &str)]) -> Self {
        Self::build(settings, false, Vec::new(), others)
    }

    fn build(
        settings: Option<&str>,
        start_character: bool,
        more: Vec<Step>,
        others: &[(u64, &str)],
    ) -> Self {
        let start_name = start_character.then(|| NAME.to_lowercase());
        let (world, client_data, assets) = data();
        let double = ServerDouble::new(script(more, others), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
        let server = double.login_addr();
        let records: Arc<dyn auberean_ui::Assets + Send + Sync> = Arc::clone(assets) as _;
        let retail = Rc::new(RefCell::new(
            Retail::new(
                Arc::clone(&records),
                (1024, 768),
                Some(Login {
                    server,
                    account: "tester".into(),
                    password: "t".into(),
                }),
            )
            .with_start_character(start_name.as_deref(), None),
        ));
        let host = DoubleHost {
            time: 0.0,
            double,
            inbox: VecDeque::new(),
            settings: settings.map(str::to_owned),
            files: std::collections::HashMap::new(),
            folder: None,
            pad: None,
            copied: Vec::new(),
        };
        let horizon = Rc::new(RefCell::new(None));
        let made = Rc::clone(&horizon);
        let start = Start {
            login: Some(Login {
                server,
                account: "tester".into(),
                password: "t".into(),
            }),
            play: start_name.clone(),
            ..Start::default()
        };
        let interfaces = maker(Shared(Rc::clone(&retail)), move || {
            let h = Rc::new(RefCell::new(
                Horizon::built_in(Some(Arc::clone(&records)))?.with_start(start.clone()),
            ));
            *made.borrow_mut() = Some(Rc::clone(&h));
            Ok(Box::new(Shared(h)) as Box<dyn Interface>)
        });
        let mut shell = Shell::with_interfaces(host, world, client_data.clone(), interfaces);
        shell.size = (1920, 1080);
        Self {
            shell,
            retail,
            horizon,
        }
    }

    /// A plugin's window as the shell hands it to the interface.
    pub fn shell_window(
        &self,
        plugin: &str,
        window: &str,
    ) -> Option<auberean_shell::plugins::PluginWindow> {
        let h = self.horizon();
        let found = h
            .borrow()
            .ui()
            .plugins
            .windows
            .iter()
            .find(|w| w.plugin == plugin && w.window == window)
            .cloned();
        found
    }

    /// Horizon, once it was chosen.
    pub fn horizon(&self) -> Rc<RefCell<Horizon>> {
        Rc::clone(self.horizon.borrow().as_ref().expect("Horizon was chosen"))
    }

    /// A frame of 1/60 s; how many textured batches it drew and how many textures the overlay
    /// keeps.
    pub fn step(&mut self) -> (usize, usize) {
        self.advance();
        let out = self.shell.frame();
        (out.overlay.items.len(), out.overlay.textures.len())
    }

    /// A frame of 1/60 s, and what it drew.
    pub fn frame(&mut self) -> auberean_shell::interface::Overlay {
        self.advance();
        self.shell.frame().overlay.clone()
    }

    /// The host's clock 1/60 s on, and the double's datagrams by then waiting: what the shell's
    /// next frame hears.
    pub fn advance(&mut self) {
        let h = &mut self.shell.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
    }

    /// A frame of 1/60 s, handed to `f` as the shell drew it.
    pub fn frame_with<R>(&mut self, f: impl FnOnce(&auberean_shell::Output<'_>) -> R) -> R {
        let h = &mut self.shell.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        f(&self.shell.frame())
    }

    /// A frame of 1/60 s; whether it drew the world.
    pub fn world_drawn(&mut self) -> bool {
        self.frame_with(|out| out.world.is_some())
    }

    /// Frames until `done`, at most `seconds` more of the host's clock.
    pub fn until(&mut self, seconds: f64, mut done: impl FnMut(&Self) -> bool) -> bool {
        let end = self.shell.host.time + seconds;
        while self.shell.host.time < end {
            if done(self) {
                return true;
            }
            self.step();
        }
        done(self)
    }

    /// Point at `(x, y)`, press the left button, let it go: a frame between each.
    pub fn click(&mut self, (x, y): (f32, f32)) {
        self.shell.event(&Event::Pointer { x, y });
        self.step();
        for down in [true, false] {
            self.shell.event(&Event::Button {
                button: Button::Left,
                down,
            });
            self.step();
        }
    }

    /// Past the game's opening pictures (a press ends them) and into the world; with Horizon
    /// shown, through its own login.
    pub fn enter_the_world(&mut self) {
        self.step();
        if self.shell.interface() == Choice::Horizon {
            let entered = self.until(15.0, |s| s.shell.phase() == Phase::World);
            assert!(entered, "in the world: {:?}", self.shell.phase());
            return;
        }
        assert_eq!(
            self.retail.borrow().screen(),
            "intro",
            "the game opens on its pictures"
        );
        self.click((10.0, 10.0));
        let entered = self.until(15.0, |s| s.shell.phase() == Phase::World);
        assert!(entered, "in the world: {:?}", self.shell.phase());
    }
}

/// What a guarded test is doing, as it says: named when it hangs.
pub type Doing = Arc<std::sync::Mutex<&'static str>>;

/// Say what a guarded test is doing now.
pub fn doing(note: &Doing, what: &'static str) {
    *note
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = what;
}

/// Run `test` on a thread of its own and hand back what it gives. It fails, naming what the test
/// said it was doing, when the test has not ended within `minutes`: a guard against a hang only,
/// sized for the slowest machine. A test that fails fails here as it would have.
pub fn guarded<T: Send + 'static>(
    minutes: u64,
    test: impl FnOnce(&Doing) -> T + Send + 'static,
) -> T {
    let note: Doing = Arc::new(std::sync::Mutex::new("starting"));
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = {
        let note = Arc::clone(&note);
        std::thread::Builder::new()
            .name("guarded test".to_owned())
            .spawn(move || {
                let _ = tx.send(test(&note));
            })
            .expect("the test's thread")
    };
    match rx.recv_timeout(std::time::Duration::from_secs(minutes * 60)) {
        Ok(out) => out,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let what = *note
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            panic!("still {what} after {minutes} minutes: hung")
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match handle.join() {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(()) => panic!("the test's thread ended with nothing"),
        },
    }
}
