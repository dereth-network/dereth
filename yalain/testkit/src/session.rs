//! Our own client played through a whole session, step by step: the same steps on the in-memory
//! network with the harness's clock, or on loopback against the server's executable. Each step is
//! one or more of the client's actions, then a wait for what the step should leave in the client's
//! model (or, where the scripts hold it, what the client's scripts answer when asked as the game's
//! panels ask). (Feature `retail-dats`: the client runs the install's script library.)

use std::net::SocketAddr;
use std::sync::Arc;

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::model::Object;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::ClientMessage;
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::runtime::interface::function;
use auberean_client_sdk::runtime::{CallResult, InterfaceArg};
use auberean_client_sdk::{Action, Client, ClientData, ClientSettings, Motion, NewCharacter};

use dereth_primitives::num::math;

use crate::authored::{self, Portal};
use crate::{port_addr, test_rules, test_start, Harness, Peer, STEP};
use yalain_net::Port;
use yalain_store::MemoryStore;
use yalain_world::{Server, ServerConfig};

/// The server's calls a session watches arrive.
pub mod call {
    /// A selection's vitals.
    pub const SELECTION_INFO: u32 = 0x8212_002C;
    /// An examination's profile.
    pub const EXAMINATION_PROFILE: u32 = 0x820A_0009;
    /// A saying, on the speaker.
    pub const SAYING: u32 = 0x8205_0016;
    /// A tell to the player.
    pub const HEAR_TELL: u32 = 0x8205_0017;
    pub const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
    pub const EXIT_PORTAL_SPACE: u32 = 0x8212_001B;
}

/// What the client sends that a session looks for.
pub mod sent {
    pub const SET_SELECTION: u32 = 0x8212_0016;
    pub const QUERY_EXAMINATION: u32 = 0x820A_0007;
    /// The look-at message's type.
    pub const LOOK_AT: u32 = 0x100C7;
}

/// The quality that says a creature is dead.
pub const DEAD: u32 = 279;
/// The tabs the examination panel asks for.
pub const TABS: u32 = 0x3FF;

/// The install's data files in the order the client opens them, and its script library.
///
/// # Panics
/// The install is not found, or its script library does not link.
#[must_use]
pub fn client_data() -> ClientData {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("the install's data files");
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<Arc<DatFile>> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok().map(Arc::new)
    };
    let portal = open(&|n| *n == DatName::Portal).expect("the portal file");
    let mut files = vec![Arc::clone(&portal)];
    files.extend(open(&|n| *n == DatName::Local("English".into())));
    files.extend(open(&|n| *n == DatName::Cell(1)));
    files.extend(open(&|n| *n == DatName::HighRes));
    let program = auberean_client_sdk::runtime::link_scripts(&portal).expect("the library links");
    ClientData {
        files: DataFiles {
            files,
            language: ENGLISH,
        },
        scripts: Some((portal, Arc::new(program))),
    }
}

/// A client for a session against the server at `login`, watching the calls a session waits for.
#[must_use]
pub fn client(login: SocketAddr, account: &str) -> Client {
    let settings = ClientSettings {
        login_server: login,
        account: account.as_bytes().to_vec(),
        ticket: b"ticket".to_vec(),
        timestamp: 1_485_419_089,
        session_key: [7; 64],
        script_sends: true,
    };
    let mut c = Client::new(settings, client_data());
    c.observe = vec![
        call::SELECTION_INFO,
        call::EXAMINATION_PROFILE,
        call::SAYING,
        call::HEAR_TELL,
        call::ENTER_PORTAL_SPACE,
        call::EXIT_PORTAL_SPACE,
    ];
    c
}

/// Our client on the harness's wire.
#[derive(Debug)]
pub struct Ours {
    pub addr: SocketAddr,
    pub client: Client,
}

impl Peer for Ours {
    fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn on_datagram(&mut self, from: SocketAddr, bytes: &[u8], now: LocalTime) {
        self.client.handle_datagram(from, bytes, now);
    }

    fn tick(&mut self, now: LocalTime) -> Vec<(SocketAddr, Vec<u8>)> {
        self.client.frame(now);
        let mut out = Vec::new();
        while let Some(d) = self.client.poll_transmit() {
            out.push(d);
        }
        out
    }
}

/// The server with the authored world (a world database compiled against the install) and our
/// client, on the harness's wire and clock.
#[derive(Debug)]
pub struct InProcess {
    pub h: Harness,
    pub ours: Ours,
}

impl InProcess {
    /// The server and a client logging in to it.
    #[must_use]
    pub fn new() -> Self {
        let p = Portal::open();
        let mut h = Harness::new();
        let t = authored::world(&p);
        let catalog = p.catalog(&t);
        h.server = Server::new(
            ServerConfig::default(),
            MemoryStore::default(),
            test_rules(),
        )
        .expect("the store sets ids aside")
        .with_world(test_start(), t.content)
        .with_play(catalog, t.spawns, t.loot);
        h.server.stores = t.stores;
        h.server.quests = t.quests;
        h.server.talk = t.talk;
        h.server.levels = p.levels();
        let addr = SocketAddr::from(([127, 0, 0, 1], 51_000));
        Self {
            h,
            ours: Ours {
                addr,
                client: client(port_addr(Port::LOGIN_EVEN), "ours"),
            },
        }
    }
}

impl Default for InProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl Drive for InProcess {
    fn client(&mut self) -> &mut Client {
        &mut self.ours.client
    }

    fn client_ref(&self) -> &Client {
        &self.ours.client
    }

    fn step(&mut self) {
        self.h.step_with(STEP, &mut [&mut self.ours]);
    }

    fn now(&self) -> f64 {
        self.h.now.0
    }
}

/// What moves a session on: the client, a step of its world, and the clock.
pub trait Drive {
    fn client(&mut self) -> &mut Client;
    fn client_ref(&self) -> &Client;
    /// One step: the client's frame and whatever it and the server exchange.
    fn step(&mut self);
    /// Seconds on the session's clock.
    fn now(&self) -> f64;
}

/// A step that did not come about in time, with what the client last sent.
fn timed_out(d: &impl Drive, what: &str, seconds: f64) -> String {
    format!(
        "{what}: not within {seconds} s ({})",
        last_sent(d.client_ref())
    )
}

/// What the client asked of its scripts, last sent, and how its calls went.
fn last_sent(c: &Client) -> String {
    let last: Vec<String> = c
        .sent()
        .iter()
        .rev()
        .take(8)
        .rev()
        .map(|s| format!("{:.2} {:x?}", s.at, s.message.message_type()))
        .collect();
    format!(
        "asked {:x?}; last sent {last:?}; calls {:?}",
        c.asked, c.calls
    )
}

/// How long a step may take before the session is taken to hang, in seconds on the session's
/// clock. Every step waits for what it should bring; this only stops a hang, so it is generous
/// enough for a loaded machine running several servers at once.
pub const HANG: f64 = 900.0;

/// Step until `done` holds, at most `seconds`.
///
/// # Errors
/// It did not hold in time.
pub fn until(
    d: &mut impl Drive,
    seconds: f64,
    what: &str,
    done: impl Fn(&Client) -> bool,
) -> Result<(), String> {
    let end = d.now() + seconds;
    while !done(d.client_ref()) {
        if d.now() >= end {
            return Err(timed_out(d, what, seconds));
        }
        d.step();
    }
    Ok(())
}

/// Step for `seconds`.
pub fn wait(d: &mut impl Drive, seconds: f64) {
    let end = d.now() + seconds;
    while d.now() < end {
        d.step();
    }
}

/// The first object of a template around the player.
#[must_use]
pub fn find(c: &Client, template: u32) -> Option<Object> {
    c.model
        .surroundings()
        .find(|o| o.weenie.entity_did == template)
        .cloned()
}

/// The messages the client sent from `from` on.
#[must_use]
pub fn sent_since(c: &Client, from: usize) -> Vec<ClientMessage> {
    c.sent()[from..].iter().map(|s| s.message.clone()).collect()
}

/// The character list arrives.
///
/// # Errors
/// It did not.
pub fn character_list(d: &mut impl Drive) -> Result<(), String> {
    until(d, HANG, "the character list", |c| {
        c.model.characters.is_some()
    })
}

/// A new human man of this name, made through the creation request, joins the list; returns its
/// id.
///
/// # Errors
/// It did not join the list.
pub fn create(d: &mut impl Drive, name: &str) -> Result<u64, String> {
    let before = d
        .client_ref()
        .model
        .characters
        .as_ref()
        .map_or(0, |l| l.characters.len());
    d.client().act(Action::CreateCharacter(NewCharacter {
        name: name.encode_utf16().collect(),
        entity: 0x4700_110F,
        start_area: 0x4000_0001,
        race: 1,
        sex: 0x1000,
        class: 2,
        implement: 1,
        physique: vec![(1, 1.0), (2, 1.0)],
        physique_size_class: 2,
    }));
    until(d, HANG, "the new character in the list", |c| {
        c.model
            .characters
            .as_ref()
            .is_some_and(|l| l.characters.len() == before + 1)
    })?;
    let list = d.client_ref().model.characters.as_ref();
    list.and_then(|l| l.characters.last())
        .map(|i| i.id)
        .ok_or_else(|| "no new character".to_owned())
}

/// Into the world with a character: the player made and placed, the scene reported drawn.
///
/// # Errors
/// The player did not arrive.
pub fn enter(d: &mut impl Drive, character: u64) -> Result<(), String> {
    let from = d.client_ref().sent().len();
    d.client().act(Action::PlayCharacter { character });
    until(d, HANG, "the player in the world", |c| {
        c.model.player == Some(character) && c.model.player_object().is_some() && c.body.placed()
    })?;
    until(d, HANG, "the scene reported drawn", |c| {
        sent_since(c, from)
            .iter()
            .any(|m| matches!(m, ClientMessage::SceneRenderingComplete(_)))
    })
}

/// Objects of these templates around the player.
///
/// # Errors
/// They did not all come.
pub fn see(d: &mut impl Drive, templates: &[u32]) -> Result<(), String> {
    until(d, HANG, "the people and creatures around", |c| {
        let around: Vec<u32> = c
            .model
            .surroundings()
            .map(|o| o.weenie.entity_did)
            .collect();
        templates.iter().all(|t| around.contains(t))
    })
}

/// Run toward a point until within `reach` of it, then stop; returns how many position reports
/// the walk sent.
///
/// # Errors
/// The body did not get there.
pub fn walk_to(d: &mut impl Drive, to: [f32; 3], reach: f32) -> Result<usize, String> {
    let from = d.client_ref().sent().len();
    let end = d.now() + 60.0;
    loop {
        let b = &d.client_ref().body;
        let (dx, dy) = (to[0] - b.origin.x, to[1] - b.origin.y);
        if math::hypotf(dx, dy) <= reach {
            break;
        }
        if d.now() >= end {
            return Err(timed_out(d, &format!("walking to {to:?}"), 60.0));
        }
        let heading = math::atan2f(dx, dy).to_degrees().rem_euclid(360.0);
        d.client().act(Action::Move(Motion {
            forward: 1.0,
            sideways: 0.0,
            heading,
            active: true,
        }));
        d.step();
    }
    let heading = d.client_ref().body.heading;
    d.client().act(Action::Move(Motion {
        forward: 0.0,
        sideways: 0.0,
        heading,
        active: false,
    }));
    wait(d, 0.5);
    Ok(sent_since(d.client_ref(), from)
        .iter()
        .filter(|m| matches!(m, ClientMessage::Position(_)))
        .count())
}

/// Select an object and wait for its vitals and its examination's profile (the examination panel
/// open); returns what the selection sent, by function or message type, in order.
///
/// # Errors
/// They did not come.
pub fn select_and_examine(d: &mut impl Drive, target: u64) -> Result<Vec<u32>, String> {
    let (seen, from) = (d.client_ref().observed.len(), d.client_ref().sent().len());
    d.client().act(Action::Select { target });
    until(d, HANG, "the vitals and the profile", |c| {
        let got = |f| c.observed[seen..].iter().any(|(_, g, _, _)| *g == f);
        got(call::SELECTION_INFO) && got(call::EXAMINATION_PROFILE)
    })?;
    Ok(sent_since(d.client_ref(), from)
        .iter()
        .filter_map(|m| match m {
            ClientMessage::ServerCall(c) => Some(c.function.0),
            ClientMessage::LookAt(_) => Some(sent::LOOK_AT),
            _ => None,
        })
        .collect())
}

/// Wield an item of a template the player holds, by using it as the inventory panel does (the
/// player's scripts send the use); returns the item once it is wielded.
///
/// # Errors
/// The player holds none, or it was not wielded.
pub fn wield(d: &mut impl Drive, template: u32) -> Result<u64, String> {
    let me = d
        .client_ref()
        .model
        .player_object()
        .map(|o| o.id)
        .ok_or("no player")?;
    let item = d
        .client_ref()
        .model
        .objects()
        .find(|o| o.weenie.entity_did == template && o.container == Some(me))
        .map(|o| o.id)
        .ok_or("the player holds no such item")?;
    use_object(d, item);
    until(d, HANG, "the item wielded", |c| {
        c.model
            .object(item)
            .is_some_and(|o| o.parent.is_some_and(|(p, _)| p == me))
    })?;
    Ok(item)
}

/// An object's health and greatest health as the client knows them (a creature's from its
/// selection's vitals, the player's from its qualities).
fn health(c: &Client, id: u64) -> (Option<i32>, Option<i32>) {
    use auberean_client_sdk::runtime::description::int;
    use yalain_world::player::quality::{HEALTH, HEALTH_MAX};
    let Some(o) = c.model.object(id) else {
        return (None, None);
    };
    let q = |k: u32| int(&o.weenie, k).or_else(|| o.qualities.ints.get(&k).copied());
    (q(HEALTH), q(HEALTH_MAX))
}

/// Whether the client knows `target` dead. The client keeps the flag where its qualities answer it
/// first: in the object's description, which holds it.
fn dead(c: &Client, target: u64) -> bool {
    c.model.object(target).is_some_and(|o| {
        auberean_client_sdk::runtime::description::bool(&o.weenie, DEAD)
            .or_else(|| o.qualities.bools.get(&DEAD).copied())
            == Some(1)
    })
}

/// How [`attack_until_dead`]'s error begins when the player died first.
pub const LOST_THE_FIGHT: &str = "the player died before the creature";

/// Attack the selection until it is dead. A fight the player loses ends its attack, so the player's
/// death fails at once.
///
/// # Errors
/// The player died first, or the target did not die.
pub fn attack_until_dead(d: &mut impl Drive, target: u64) -> Result<(), String> {
    d.client().act(Action::Attack { start: true });
    let end = d.now() + HANG;
    loop {
        let c = d.client_ref();
        if dead(c, target) {
            return Ok(());
        }
        let me = c.model.player.unwrap_or(0);
        if health(c, me).0 == Some(0) {
            return Err(format!(
                "{LOST_THE_FIGHT} ({}); {}",
                last_sent(c),
                standing(c, target)
            ));
        }
        if d.now() >= end {
            let e = timed_out(d, "the creature dead", HANG);
            return Err(format!("{e}; {}", standing(d.client_ref(), target)));
        }
        d.step();
    }
}

/// Where the player and `target` stand as the client knows them, how far apart, and their health.
fn standing(c: &Client, target: u64) -> String {
    let b = &c.body;
    let me = c.model.player.unwrap_or(0);
    let me = format!(
        "the player at {:08x} {:?}, health {:?}",
        b.cell,
        b.origin,
        health(c, me)
    );
    match c.model.object(target).and_then(|o| o.place.as_ref()) {
        Some(p) => {
            let (dx, dy) = (p.origin.x - b.origin.x, p.origin.y - b.origin.y);
            format!(
                "{me}; the target at {:08x} {:?}, {:.2} m away on the ground, health {:?}",
                p.cell,
                p.origin,
                math::hypotf(dx, dy),
                health(c, target)
            )
        }
        None => format!("{me}; the target not placed"),
    }
}

/// Select and use an object.
pub fn use_object(d: &mut impl Drive, target: u64) {
    d.client().act(Action::Select { target });
    d.client().act(Action::UseSelected);
}

/// Open the corpse a creature left by using it; returns the corpse and the first item in it.
///
/// # Errors
/// No corpse, or nothing in it.
pub fn open_corpse(d: &mut impl Drive) -> Result<(u64, u64), String> {
    until(d, HANG, "the corpse", |c| {
        find(c, authored::CORPSE).is_some()
    })?;
    let corpse = find(d.client_ref(), authored::CORPSE)
        .map(|o| o.id)
        .ok_or("no corpse")?;
    use_object(d, corpse);
    until(d, HANG, "what the corpse holds", |c| {
        c.model.objects().any(|o| o.container == Some(corpse))
    })?;
    let item = d
        .client_ref()
        .model
        .objects()
        .find(|o| o.container == Some(corpse))
        .map(|o| o.id)
        .ok_or("nothing in the corpse")?;
    Ok((corpse, item))
}

/// Whether the player's scripts hold an item, asked as the inventory panel asks.
pub fn have(d: &mut impl Drive, item: u64) -> bool {
    d.client()
        .interface(function::DO_I_HAVE_ITEM, &[InterfaceArg::Long(item)])
        .is_some_and(|(r, v)| r == CallResult::Completed && v == 1)
}

/// Take an item by using it: it leaves the world for the pack the player's scripts keep.
///
/// # Errors
/// It did not reach the pack.
pub fn take(d: &mut impl Drive, item: u64) -> Result<(), String> {
    use_object(d, item);
    let end = d.now() + 15.0;
    while !have(d, item) {
        if d.now() >= end {
            return Err(timed_out(d, "the item in the pack", 15.0));
        }
        d.step();
    }
    if d.client_ref()
        .model
        .object(item)
        .is_some_and(|o| o.in_world)
    {
        return Err("the taken item is still in the world".into());
    }
    Ok(())
}

/// Use a person, and hear it tell the player something.
///
/// # Errors
/// No tell came.
pub fn talk_to(d: &mut impl Drive, person: u64) -> Result<(), String> {
    let heard = d.client_ref().observed.len();
    use_object(d, person);
    until(d, HANG, "the person's tell", |c| {
        c.observed[heard..]
            .iter()
            .any(|(_, f, _, _)| *f == call::HEAR_TELL)
    })
}

/// Say something, and hear it said.
///
/// # Errors
/// It was not heard.
pub fn say(d: &mut impl Drive, text: &str) -> Result<(), String> {
    let heard = d.client_ref().observed.len();
    d.client().act(Action::Say { text: text.into() });
    until(d, HANG, "the saying heard", |c| {
        c.observed[heard..]
            .iter()
            .any(|(_, f, _, _)| *f == call::SAYING)
    })
}

/// Use a portal: into portal space, out again in the cell it leads to; returns the trip's
/// observed calls' start.
///
/// # Errors
/// The trip did not happen.
pub fn take_portal(d: &mut impl Drive, portal: u64, to: u32) -> Result<(), String> {
    let trip = d.client_ref().observed.len();
    use_object(d, portal);
    until(d, HANG, "the portal's destination", |c| c.body.cell == to)?;
    until(d, HANG, "out of portal space", |c| {
        let n = |f| {
            c.observed[trip..]
                .iter()
                .filter(|(_, g, _, _)| *g == f)
                .count()
        };
        n(call::ENTER_PORTAL_SPACE) == 1 && n(call::EXIT_PORTAL_SPACE) == 1
    })
}

/// Log off: out of the world, at the character list again.
///
/// # Errors
/// The client stayed in the world.
pub fn log_off(d: &mut impl Drive) -> Result<(), String> {
    d.client().act(Action::LogOff);
    until(d, HANG, "out of the world", |c| {
        c.model.player.is_none() && c.model.characters.is_some()
    })
}

/// Distance on the ground from a point to an object's place.
#[must_use]
pub fn ground_distance(a: [f32; 3], o: &Object) -> Option<f32> {
    o.place
        .map(|p| math::hypotf(a[0] - p.origin.x, a[1] - p.origin.y))
}
