//! The client with no window: logs in to a server on loopback, enters the world and plays a script
//! of actions, on the SDK alone.
//!
//! **Depends on** the SDK alone (`auberean-client-sdk`). **Used by** nothing: it is the
//! `auberean-headless` program.
//!
//! Logs in to a server on this machine, waits for the character list, enters the world with a
//! character (making it first with `--create` when the list has none of that name), stays for a
//! while and logs off, printing what happened. While in the world it can walk back and forth
//! (`--walk`) and say a line now and then (`--say`); it prints the other players it sees as they
//! come and move, and what is said around it. With `--agree` it takes every place in a fellowship,
//! every oath and every trade offered to it, and accepts a trade once the other side has; with
//! `--offer` it offers the first item of its pack when a trade opens; with `--trade-with ID` it
//! asks that player for a trade three seconds after it enters. With `--craft RECIPE:SPINNER` (hex
//! recipe, repeatable) it makes each recipe in turn, three seconds after it enters and eight after
//! the one before: it reads the recipe as the ingredient panel does, fills each slot the scripts
//! ask for with the first item they say fits, and prints the answer and what the slots hold
//! after. It prints an effect put on it or taken off as it comes. Everything it does is an
//! action of the SDK's client; it reads no device.
//!
//! ```text
//! auberean-headless --dats <install folder> [--host 127.0.0.1] [--port 9000] --account NAME --ticket TICKET
//!              [--character ID | --create NAME] [--seconds 30] [--walk] [--say TEXT] [--agree] [--offer]
//!              [--trade-with ID] [--craft RECIPE:SPINNER]...
//! ```
//!
//! The install folder is the September 2013 client's (its data files answer the server's question
//! of which data the client has, and its script library runs the server's calls). The server must
//! be on this machine: the client refuses any other address.
//!
//! **Must never** read a device or reach past the SDK: everything it does is an action of the
//! SDK's client (`cargo xtask seams`).

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use auberean_client_sdk::contract::{CraftAction, SocialAction, TradeAction};
use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::script::args::{decode, Arg};
use auberean_client_sdk::script::graph::Body;
use auberean_client_sdk::script::native_object::NativeObject;
use auberean_client_sdk::{Action, Client, ClientData, ClientSettings, Motion, NewCharacter};

/// The template every player's object is made of.
const PLAYER_TEMPLATE: u32 = 0x4700_0530;
/// How long to wait after an entry is refused (the character still in the world) before asking
/// again.
const RETRY_AFTER: f64 = 2.0;
/// A saying, as the server calls it on the speaker.
const SAYING: u32 = 0x8205_0016;
/// The examination panel's tabs.
const EXAMINATION_TABS: u32 = 0x3FF;
/// The server's calls an agreeable client answers: a place in a fellowship offered, an oath
/// offered, a trade asked for, opened, and accepted by the other side.
const RECRUITMENT_OFFERED: u32 = 0x820C_002E;
const OATH_OFFERED: u32 = 0x8203_002F;
const TRADE_REGISTERED: u32 = 0x8215_0046;
const TRADE_OPENED: u32 = 0x8215_004E;
const TRADE_ACCEPTED: u32 = 0x8215_0050;
const CRAFT_ANSWER: u32 = 0x8206_001C;
/// An item's quantity (a quality the server updates).
const QUANTITY: u32 = 262;
const EFFECT_ADDED: u32 = 0x8208_0017;
const EFFECT_REMOVED: u32 = 0x8208_0016;
/// How long a making is given to end before the next is asked.
const BETWEEN_CRAFTS: f64 = 12.0;

struct Options {
    dats: PathBuf,
    host: Ipv4Addr,
    port: u16,
    account: String,
    ticket: String,
    character: Option<u64>,
    create: Option<String>,
    seconds: f64,
    walk: bool,
    say: Option<String>,
    agree: bool,
    offer: bool,
    trade_with: Option<u64>,
    crafts: Vec<(u32, i32)>,
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut o = Options {
        dats: PathBuf::new(),
        host: Ipv4Addr::LOCALHOST,
        port: 9000,
        account: String::new(),
        ticket: String::new(),
        character: None,
        create: None,
        seconds: 30.0,
        walk: false,
        say: None,
        agree: false,
        offer: false,
        trade_with: None,
        crafts: Vec::new(),
    };
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--dats" => o.dats = PathBuf::from(value()?),
            "--host" => o.host = value()?.parse().map_err(|e| format!("--host: {e}"))?,
            "--port" => o.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--account" => o.account = value()?,
            "--ticket" => o.ticket = value()?,
            "--character" => {
                let v = value()?;
                let v = v.trim_start_matches("0x");
                o.character =
                    Some(u64::from_str_radix(v, 16).map_err(|e| format!("--character: {e}"))?);
            }
            "--create" => o.create = Some(value()?),
            "--seconds" => o.seconds = value()?.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--walk" => o.walk = true,
            "--agree" => o.agree = true,
            "--offer" => o.offer = true,
            "--trade-with" => {
                let v = value()?;
                let v = v.trim_start_matches("0x");
                o.trade_with =
                    Some(u64::from_str_radix(v, 16).map_err(|e| format!("--trade-with: {e}"))?);
            }
            "--say" => o.say = Some(value()?),
            "--craft" => {
                let v = value()?;
                let (recipe, spinner) = v.split_once(':').ok_or("--craft takes RECIPE:SPINNER")?;
                o.crafts.push((
                    u32::from_str_radix(recipe.trim_start_matches("0x"), 16)
                        .map_err(|e| format!("--craft: {e}"))?,
                    spinner.parse().map_err(|e| format!("--craft: {e}"))?,
                ));
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if o.dats.as_os_str().is_empty() || o.account.is_empty() {
        return Err("--dats and --account are needed".into());
    }
    if !o.host.is_loopback() {
        return Err("the server must be on this machine (a loopback address)".into());
    }
    Ok(o)
}

/// The data files in the order the client opens them: portal, local, cell, high-resolution.
fn data(dir: &std::path::Path) -> Result<ClientData, String> {
    let found = install_dats(dir).map_err(|e| e.to_string())?;
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<Arc<DatFile>> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok().map(Arc::new)
    };
    let portal = open(&|n| *n == DatName::Portal).ok_or("no portal file")?;
    let mut files = vec![Arc::clone(&portal)];
    files.extend(open(&|n| *n == DatName::Local("English".into())));
    files.extend(open(&|n| *n == DatName::Cell(1)));
    files.extend(open(&|n| *n == DatName::HighRes));
    let program = auberean_client_sdk::runtime::link_scripts_noting(&portal, |r| {
        eprintln!("auberean-headless: the script library: {r}");
    })?;
    Ok(ClientData {
        files: DataFiles {
            files,
            language: ENGLISH,
        },
        scripts: Some((portal, Arc::new(program))),
    })
}

fn main() {
    if let Err(e) = run() {
        eprintln!("auberean-headless: {e}");
        std::process::exit(1);
    }
}

/// The words of a saying's arguments (the last parameter first: the flags, then the words).
fn saying(client: &Client, args: &[u8]) -> Option<String> {
    let lib = &client.scripts.as_ref()?.host.program.library;
    decode(args, lib).ok()?.iter().find_map(|a| match a {
        Arg::Object(g) => g.entries.iter().find_map(|e| match &e.body {
            Body::Object(o) => match &o.native {
                Some(NativeObject::TextInfo(t)) => {
                    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
                }
                _ => None,
            },
            _ => None,
        }),
        _ => None,
    })
}

/// The instance ids of a call's arguments, in the order of the function's parameters (the values
/// come last parameter first).
fn ids(client: &Client, args: &[u8]) -> Vec<u64> {
    let Some(lib) = client.scripts.as_ref().map(|s| &s.host.program.library) else {
        return Vec::new();
    };
    let mut out: Vec<u64> = decode(args, lib)
        .map(|v| {
            v.iter()
                .filter_map(|a| match a {
                    Arg::LongInt(l) => Some(*l),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    out.reverse();
    out
}

/// What the client does in the world: walking back and forth, saying its line now and then, and
/// telling what it sees of the others.
struct Play {
    walk: bool,
    say: Option<String>,
    agree: bool,
    offer: bool,
    /// The trade's other side has accepted, and this side has answered.
    trade_accepted: bool,
    /// A player to ask for a trade, once.
    trade_with: Option<u64>,
    /// The recipes yet to make, and when the next is due.
    crafts: std::collections::VecDeque<(u32, i32)>,
    next_craft: f64,
    /// The items of the last making, by slot.
    slots: Vec<(u32, u64)>,
    entered: f64,
    next_say: f64,
    leg: u32,
    next_turn: f64,
    heard: usize,
    others: BTreeMap<u64, (u32, [f32; 3])>,
}

impl Play {
    fn new(o: &Options) -> Self {
        Self {
            walk: o.walk,
            say: o.say.clone(),
            agree: o.agree,
            offer: o.offer,
            trade_accepted: false,
            trade_with: o.trade_with,
            crafts: o.crafts.iter().copied().collect(),
            next_craft: 0.0,
            slots: Vec::new(),
            entered: 0.0,
            next_say: 0.0,
            leg: 0,
            next_turn: 0.0,
            heard: 0,
            others: BTreeMap::new(),
        }
    }

    fn entered(&mut self, client: &mut Client, t: f64) {
        self.entered = t;
        self.next_say = t + 3.0;
        self.next_turn = t + 2.0;
        self.next_craft = t + 3.0;
        client.act(Action::Examination {
            tabs: Some(EXAMINATION_TABS),
        });
    }

    fn tick(&mut self, client: &mut Client, t: f64) {
        if client.model.player_object().is_none() {
            return;
        }
        if self.walk && t >= self.next_turn {
            // Two seconds forward, a second standing, then back the other way.
            let base = client.body.heading;
            let (forward, heading, next) = match self.leg % 2 {
                0 => (1.0, base, 2.0),
                _ => (0.0, (base + 180.0).rem_euclid(360.0), 1.0),
            };
            client.act(Action::Move(Motion {
                forward,
                sideways: 0.0,
                heading,
                active: forward != 0.0,
            }));
            println!(
                "{:7.2} walking: {} at {:08X} ({:.1}, {:.1})",
                t - self.entered,
                if forward != 0.0 { "running" } else { "turned" },
                client.body.cell,
                client.body.origin.x,
                client.body.origin.y
            );
            self.leg += 1;
            self.next_turn = t + next;
        }
        if let Some(partner) = self.trade_with {
            if t >= self.entered + 3.0 {
                println!("{:7.2} asks {partner:#018x} for a trade", t - self.entered);
                client.act(Action::Trade(TradeAction::Open(partner)));
                self.trade_with = None;
            }
        }
        if t >= self.next_craft {
            if let Some((recipe, spinner)) = self.crafts.pop_front() {
                self.craft(client, recipe, spinner, t);
                self.next_craft = t + BETWEEN_CRAFTS;
            }
        }
        if let Some(line) = &self.say {
            if t >= self.next_say {
                println!("{:7.2} saying: {line}", t - self.entered);
                client.act(Action::Say { text: line.clone() });
                self.next_say = t + 10.0;
            }
        }
        let me = client.model.player;
        let seen: Vec<(u64, u32, [f32; 3])> = client
            .model
            .surroundings()
            .filter(|o| o.weenie.entity_did == PLAYER_TEMPLATE && Some(o.id) != me)
            .filter_map(|o| {
                o.place
                    .map(|p| (o.id, p.cell, [p.origin.x, p.origin.y, p.origin.z]))
            })
            .collect();
        for (id, cell, at) in seen {
            let moved = self.others.get(&id).is_none_or(|(c, a)| {
                *c != cell || (a[0] - at[0]).powi(2) + (a[1] - at[1]).powi(2) > 0.25
            });
            if moved {
                let first = !self.others.contains_key(&id);
                println!(
                    "{:7.2} {} player {id:#018x} at {cell:08X} ({:.1}, {:.1})",
                    t - self.entered,
                    if first { "sees" } else { "sees moved" },
                    at[0],
                    at[1]
                );
                self.others.insert(id, (cell, at));
            }
        }
        while self.heard < client.observed.len() {
            let (_, f, args, speaker) = client.observed[self.heard].clone();
            self.heard += 1;
            if matches!(f, CRAFT_ANSWER | EFFECT_ADDED | EFFECT_REMOVED) {
                self.told(client, f, args.as_ref().map(|a| a.0.as_slice()), t);
                continue;
            }
            if f != SAYING {
                let who = args.map(|a| ids(client, &a.0)).unwrap_or_default();
                self.offered(client, f, &who, t);
                continue;
            }
            let words = args.and_then(|a| saying(client, &a.0)).unwrap_or_default();
            let who = match speaker {
                Some(s) if Some(s) == me => "itself".to_owned(),
                Some(s) => format!("player {s:#018x}"),
                None => "someone".to_owned(),
            };
            println!("{:7.2} hears {who} say \"{words}\"", t - self.entered);
        }
    }

    /// Make a recipe at a spinner value: its slots filled as the scripts say they fit.
    fn craft(&mut self, client: &mut Client, recipe: u32, spinner: i32, t: f64) {
        let at = t - self.entered;
        self.print_slots(client, "after");
        let Some((read, _)) = client.recipe_at(recipe, Some(spinner)) else {
            println!("{at:7.2} the scripts give no recipe {recipe:08X}");
            return;
        };
        self.slots = read
            .ingredients
            .iter()
            .filter_map(|i| {
                read.items
                    .iter()
                    .find(|item| read.fits.get(&(i.id, **item)) == Some(&true))
                    .map(|item| (i.id, *item))
            })
            .collect();
        println!(
            "{at:7.2} makes {recipe:08X} at {spinner}: asked for slots {:?}, filled {:?}, cost {:?}, difficulty {:?}",
            read.ingredients.iter().map(|i| i.id).collect::<Vec<_>>(),
            self.slots.iter().map(|(o, _)| *o).collect::<Vec<_>>(),
            read.cost,
            read.difficulty,
        );
        for (ordinal, item) in self.slots.clone() {
            client.act(Action::Craft(CraftAction::AddIngredient {
                recipe,
                ordinal,
                item,
                spinner,
            }));
        }
        client.act(Action::Craft(CraftAction::ExecuteCraft {
            recipe,
            spinner,
            ingredients: self.slots.clone(),
        }));
    }

    /// What the slots of the last making hold now.
    fn print_slots(&self, client: &Client, when: &str) {
        for (ordinal, item) in &self.slots {
            let held = client
                .model
                .object(*item)
                .and_then(|o| o.qualities.ints.get(&QUANTITY).copied());
            println!("        slot {ordinal} {when}: {held:?}");
        }
    }

    /// A craft's answer, or an effect put on or taken off.
    fn told(&mut self, client: &Client, f: u32, args: Option<&[u8]>, t: f64) {
        let at = t - self.entered;
        let lib = client.scripts.as_ref().map(|s| &s.host.program.library);
        let words: Vec<u32> = args
            .zip(lib)
            .and_then(|(a, l)| decode(a, l).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|a| match a {
                Arg::Int(w) => Some(*w),
                _ => None,
            })
            .collect();
        match f {
            CRAFT_ANSWER => {
                println!("{at:7.2} the craft is answered {words:?} (shown, why, the recipe)");
            }
            EFFECT_ADDED => println!("{at:7.2} an effect is put on: id {words:?}"),
            _ => println!("{at:7.2} an effect is taken off: id {words:?}"),
        }
    }

    /// An offer from another player, answered as the options say: a place in a fellowship, an
    /// oath, a trade.
    fn offered(&mut self, client: &mut Client, f: u32, ids: &[u64], t: f64) {
        let me = client.model.player;
        let at = t - self.entered;
        match f {
            RECRUITMENT_OFFERED if self.agree => {
                println!("{at:7.2} takes the place offered in a fellowship");
                client.act(Action::Social(SocialAction::AcceptRecruitment));
            }
            OATH_OFFERED if self.agree => {
                println!("{at:7.2} takes the oath offered");
                client.act(Action::Social(SocialAction::TakeOath));
            }
            TRADE_REGISTERED => {
                if let (true, Some(&master)) = (self.agree, ids.first()) {
                    if Some(master) != me {
                        println!("{at:7.2} opens the trade with {master:#018x}");
                        self.trade_accepted = false;
                        client.act(Action::Trade(TradeAction::Open(master)));
                    }
                }
            }
            TRADE_OPENED if self.offer => {
                if let Some(item) = client.model.inventory().first().copied() {
                    println!("{at:7.2} offers {item:#018x}");
                    client.act(Action::Trade(TradeAction::Offer { item, quantity: 1 }));
                }
            }
            TRADE_ACCEPTED
                if self.agree
                    && !self.trade_accepted
                    && ids.first().is_some_and(|src| Some(*src) != me) =>
            {
                println!("{at:7.2} accepts the trade");
                self.trade_accepted = true;
                client.act(Action::Trade(TradeAction::Accept));
            }
            _ => {}
        }
    }
}

fn run() -> Result<(), String> {
    let o = options()?;
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut key = [0u8; 64];
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    #[allow(clippy::cast_possible_truncation)]
    for (i, b) in key.iter_mut().enumerate() {
        let v = (seed >> ((i % 16) * 8)) as u8;
        *b = v ^ (i as u8).wrapping_mul(97);
    }
    let settings = ClientSettings {
        login_server: SocketAddr::new(IpAddr::V4(o.host), o.port),
        account: o.account.clone().into_bytes(),
        ticket: o.ticket.clone().into_bytes(),
        timestamp: u32::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        )
        .unwrap_or(0),
        session_key: key,
        script_sends: true,
    };
    let mut client = Client::new(settings, data(&o.dats)?);
    client.observe = vec![
        SAYING,
        RECRUITMENT_OFFERED,
        OATH_OFFERED,
        TRADE_REGISTERED,
        TRADE_OPENED,
        TRADE_ACCEPTED,
        CRAFT_ANSWER,
        EFFECT_ADDED,
        EFFECT_REMOVED,
    ];
    let mut play = Play::new(&o);
    let start = Instant::now();
    let now = || LocalTime(1.0 + start.elapsed().as_secs_f64());
    let mut buf = vec![0u8; 65536];
    let mut entered_at: Option<f64> = None;
    let mut in_world = false;
    let mut created = false;
    let mut logged_off = false;
    // Entries refused (the character still in the world), and when to ask again after the last.
    let mut refusals = 0;
    let mut retry_at = 0.0;
    loop {
        while let Ok((n, from)) = socket.recv_from(&mut buf) {
            client.handle_datagram(from, &buf[..n], now());
        }
        let t = now();
        client.frame(t);
        while let Some((to, bytes)) = client.poll_transmit() {
            let _ = socket.send_to(&bytes, to);
        }
        if let Some((reason, n)) = client.model.refused {
            if n != refusals {
                refusals = n;
                if !in_world {
                    println!("the entry was refused ({reason}); asking again");
                    entered_at = None;
                    retry_at = t.0 + RETRY_AFTER;
                }
            }
        }
        if entered_at.is_none() && t.0 >= retry_at {
            if let Some(list) = client.model.characters.as_ref() {
                let named = |n: &str| {
                    list.characters
                        .iter()
                        .find(|c| String::from_utf16_lossy(&c.name.0).eq_ignore_ascii_case(n))
                        .map(|c| c.id)
                };
                let pick = match &o.create {
                    Some(name) => named(name),
                    None => o
                        .character
                        .or_else(|| list.characters.first().map(|c| c.id)),
                };
                if let Some(character) = pick {
                    println!("entering the world as {character:#018x}");
                    client.act(Action::PlayCharacter { character });
                    entered_at = Some(t.0);
                } else if let (Some(name), false) = (&o.create, created) {
                    println!("making {name}");
                    client.act(Action::CreateCharacter(NewCharacter {
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
                    created = true;
                }
            }
        } else if let Some(at) = entered_at {
            if !in_world && client.model.player_object().is_some() {
                in_world = true;
                println!("in the world at {:08X}", client.body.cell);
                play.entered(&mut client, t.0);
            }
            if !logged_off {
                play.tick(&mut client, t.0);
            }
            if !logged_off && t.0 - at > o.seconds {
                play.print_slots(&client, "after");
                println!(
                    "logging off: {} objects known, {} in the inventory, {} calls run ({} to the end)",
                    client.model.objects().count(),
                    client.model.inventory().len(),
                    client.calls.calls,
                    client.calls.completed
                );
                client.act(Action::LogOff);
                logged_off = true;
            }
            if logged_off && client.model.player.is_none() && t.0 - at > o.seconds + 1.0 {
                break;
            }
            if t.0 - at > o.seconds + 60.0 {
                return Err("the server did not answer the log-off".into());
            }
        }
        if t.0 > 600.0 && entered_at.is_none() {
            return Err("no character list in ten minutes".into());
        }
        std::thread::sleep(Duration::from_millis(if client.moving() { 5 } else { 10 }));
    }
    // A goodbye on every connection, as the client says when it closes.
    client.disconnect();
    while let Some((to, bytes)) = client.poll_transmit() {
        let _ = socket.send_to(&bytes, to);
    }
    println!("done: {} messages sent", client.sent().len());
    Ok(())
}
