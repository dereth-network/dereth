//! The replay harness: the client against a server double playing a recorded game login, with the
//! recorded player's own actions played back to the client.
//!
//! The double plays the servers. The harness plays the **player**: each action the recorded client
//! took (entering the world with a character, leaving, making or deleting one, chat, the panels'
//! calls to the server, selecting, turning the head, asking for data, the movement it asked for)
//! is given to the client when the server item recorded before it has gone out and its recorded
//! delay has passed. What the recorded client sent on its own (its data versions, the scene drawn,
//! its position reports, its requests for objects again) the client must send on its own, and the
//! harness compares.
//!
//! Time is virtual: a step is a sixtieth of a second while the body moves, a tenth otherwise.
//! Behaviour: none (test support)

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::runtime::{CallTally, Sent};
use auberean_client_sdk::{Action, Client, ClientData, ClientSettings, Motion, NewCharacter};
use auberean_testkit::double::plan::{Gate, ServerPlan};
use auberean_testkit::double::script::{Event, Msg, Script};
use auberean_testkit::double::ServerDouble;

/// The client's address on the in-memory network.
pub fn client_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 50_000))
}

/// What an action waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trigger {
    Start,
    Connect(usize),
    Released { conn: usize, count: usize },
}

/// One action of the recorded player.
#[derive(Clone, Debug)]
struct Planned {
    t: f64,
    trigger: Trigger,
    delay: f64,
    action: Vec<Action>,
}

/// A motion byte as the asking it carries: 0 to 255 over -1 to 1, 127 none.
fn motion(b: u32) -> f32 {
    let b = b & 0xFF;
    if b == 0x7F {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let v = b as f32 / 127.5 - 1.0;
        v.clamp(-1.0, 1.0)
    }
}

/// The player's action a recorded client message stands for, if it is one.
fn action_of(m: &Msg) -> Option<Vec<Action>> {
    let msg = ClientMessage::decode(&m.raw).ok()?;
    Some(match msg {
        ClientMessage::PlayCharacter(p) => vec![Action::PlayCharacter {
            character: p.character,
        }],
        ClientMessage::LogOff(_) => vec![Action::LogOff],
        ClientMessage::CreateCharacter(c) => vec![Action::CreateCharacter(NewCharacter {
            name: c.name.0,
            entity: c.entity_did,
            start_area: c.start_area,
            race: c.race,
            sex: c.sex,
            class: c.class,
            implement: c.implement,
            physique: c.physique.entries,
            physique_size_class: c.physique.size_class,
        })],
        ClientMessage::DeleteCharacter(d) => vec![Action::DeleteCharacter {
            character: d.character,
        }],
        ClientMessage::RestoreCharacter(r) => vec![Action::RestoreCharacter {
            character: r.character,
            name: String::from_utf16_lossy(&r.name.0),
        }],
        ClientMessage::Chat(c) => vec![Action::Chat(c.0)],
        ClientMessage::ServerCall(c) => vec![Action::ServerCall {
            function: c.function.0,
            args: c.args.0,
        }],
        ClientMessage::LookAt(t) => vec![Action::LookAt { target: t.0 }],
        ClientMessage::TurnHead(h) => vec![Action::TurnHead { x: h.x, y: h.y }],
        ClientMessage::DataRequest(r) => vec![Action::RequestData {
            kind: r.kind,
            id: r.id,
        }],
        ClientMessage::Position(p) => {
            let hm = p.heading_motion;
            #[allow(clippy::cast_precision_loss)]
            let heading = (hm >> 24) as f32 * 360.0 / 255.0;
            let mut v = Vec::new();
            if p.flags & 2 != 0 {
                v.push(Action::Jump {
                    velocity: p.jump_velocity,
                });
            }
            v.push(Action::Move(Motion {
                forward: motion(hm >> 8),
                sideways: motion(hm >> 16),
                heading,
                active: p.flags & 4 != 0,
            }));
            v
        }
        ClientMessage::DataHeld(_)
        | ClientMessage::DownloadEnds(_)
        | ClientMessage::SceneRenderingComplete(_)
        | ClientMessage::ForceCreate(_) => return None,
    })
}

/// The account the recorded client logged in with, from what it sent.
pub fn account_of(script: &Script) -> Vec<u8> {
    for e in &script.events {
        let Event::Msg { msg, .. } = e else { continue };
        let account = match (
            msg.c2s,
            if msg.c2s {
                ClientMessage::decode(&msg.raw).ok()
            } else {
                None
            },
        ) {
            (true, Some(ClientMessage::PlayCharacter(p))) => Some(p.account.0),
            (true, Some(ClientMessage::DeleteCharacter(d))) => Some(d.account.0),
            (true, Some(ClientMessage::RestoreCharacter(r))) => Some(r.account.0),
            (true, Some(ClientMessage::CreateCharacter(c))) => Some(c.account.0),
            _ => None,
        };
        if let Some(a) = account {
            return a;
        }
    }
    for e in &script.events {
        let Event::Msg { msg, .. } = e else { continue };
        if !msg.c2s {
            if let Ok(ServerMessage::CharacterList(l)) = ServerMessage::decode(&msg.raw) {
                return l.account.0;
            }
        }
    }
    b"player".to_vec()
}

/// The recorded player's actions with what each waits for.
fn plan_player(script: &Script, plan: &ServerPlan) -> Vec<Planned> {
    let mut released: Vec<(f64, usize, usize)> = Vec::new();
    for (conn, items) in plan.items.iter().enumerate() {
        for (i, it) in items.iter().enumerate() {
            released.push((it.t, conn, i + 1));
        }
    }
    released.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    for e in &script.events {
        let Event::Msg { t, msg } = e else { continue };
        if !msg.c2s {
            continue;
        }
        let Some(action) = action_of(msg) else {
            continue;
        };
        // The latest item recorded strictly before: an item recorded at the same moment may be
        // one the message itself let out.
        let k = released.partition_point(|r| r.0 < *t);
        let mut trigger = (0.0, Trigger::Start);
        if k > 0 {
            let (tr, conn, count) = released[k - 1];
            trigger = (tr, Trigger::Released { conn, count });
        }
        for c in &script.conns {
            if c.t_connect < *t && c.t_connect > trigger.0 {
                trigger = (c.t_connect, Trigger::Connect(c.n));
            }
        }
        out.push(Planned {
            t: *t,
            trigger: trigger.1,
            delay: (t - trigger.0).max(0.0),
            action,
        });
    }
    out
}

/// The server's call that lists the player's whole inventory (its contents, its containers, the
/// containers' segments), and the one that destroys an item the player holds.
pub const RESET_CONTENTS: u32 = 0x820E_0053;
pub const DESTROY_CONTAINED: u32 = 0x820E_0058;

/// How the model's inventory agreed with what the server said of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct InventoryCheck {
    /// Items and containers the server named as the player's.
    pub named: u64,
    /// Of those, the ones the model held as the player's.
    pub held: u64,
    /// The ones the model held, but not as the player's.
    pub elsewhere: u64,
    /// The ones the model did not have at all.
    pub unknown: u64,
}

/// The 64-bit values of the lists among a call's arguments.
fn listed(args: &[auberean_client_sdk::script::args::Arg]) -> Vec<u64> {
    use auberean_client_sdk::script::args::Arg;
    use auberean_client_sdk::script::graph::Body;
    use auberean_client_sdk::script::native_object::{Item, NativeObject};
    let mut out = Vec::new();
    for a in args {
        match a {
            Arg::LongInt(v) => out.push(*v),
            Arg::Object(g) => {
                for e in &g.entries {
                    if let Body::Object(p) = &e.body {
                        if let Some(NativeObject::Sequence { items, .. }) = &p.native {
                            out.extend(items.iter().filter_map(|i| match i {
                                Item::Long(v) => Some(*v),
                                _ => None,
                            }));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn check_inventory(
    client: &Client,
    function: u32,
    args: Option<&auberean_client_sdk::protocol::script::ScriptArgs>,
    check: &mut InventoryCheck,
) {
    let (Some(scripts), Some(args), Some(player)) =
        (client.scripts.as_ref(), args, client.model.player)
    else {
        return;
    };
    let Ok(values) =
        auberean_client_sdk::script::args::decode(&args.0, &scripts.host.program.library)
    else {
        return;
    };
    let ids: Vec<u64> = match function {
        RESET_CONTENTS => listed(&values[1..]),
        _ => listed(&values),
    };
    for id in ids.into_iter().filter(|id| *id != 0) {
        check.named += 1;
        match client.model.object(id) {
            None => check.unknown += 1,
            Some(_) if client.model.is_held_by(id, player) => check.held += 1,
            Some(_) => check.elsewhere += 1,
        }
    }
}

/// What one replay found.
#[derive(Debug, Default)]
pub struct Outcome {
    pub world_logins_recorded: usize,
    pub world_logins: u32,
    pub connected: Vec<usize>,
    pub referrals: u32,
    pub switches: u32,
    /// Times the login connection was found down after it came up.
    pub login_drops: u32,
    pub login_up: bool,
    pub gates_forced: Vec<Gate>,
    pub unmatched_gates: Vec<(u32, usize)>,
    pub actions: usize,
    pub actions_done: usize,
    pub sent: Vec<Sent>,
    /// The client's messages as the double received them, per recorded connection, with their
    /// blob ids.
    pub received: Vec<Vec<auberean_testkit::double::Sent>>,
    /// The recorded client's messages, with their recorded times.
    pub recorded: Vec<(f64, Msg)>,
    pub calls: CallTally,
    /// Players the recorded server created, and how many of them the model held as its player.
    pub players_created: usize,
    pub players_held: usize,
    /// Per recorded game login: the model's inventory size and surroundings at the end.
    pub inventory: usize,
    pub surroundings: usize,
    pub virtual_seconds: f64,
    pub notices: BTreeMap<u32, u64>,
    pub waited: u32,
    pub dropped: u32,
    /// The replay stopped making progress before its end, and where.
    pub stalled: bool,
    pub stall: String,
    /// When the login connection first went down before the server let go of it.
    pub drop_at: Option<f64>,
    /// The double's log of its connections.
    pub log: Vec<(f64, usize, &'static str)>,
    pub inventory_check: InventoryCheck,
    /// The client's game login as the double received it.
    pub login: Option<Vec<u8>>,
    /// The recorded client's first packet on each connection: flags and size.
    pub first_packets: Vec<(u32, u32)>,
}

/// The ticket the replayed client logs in with (the recorded ones are the session's own).
pub const TICKET: &[u8] = b"ticket";

/// Replay one recorded game login.
pub fn replay(script: &Script, data: &ClientData, seed: u64) -> Outcome {
    let double = ServerDouble::new(script.clone(), Ipv4Addr::LOCALHOST, 1.0, seed);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: account_of(script),
        ticket: TICKET.to_vec(),
        timestamp: 1_485_000_000,
        session_key: [0x5A; 64],
        script_sends: false,
    };
    let actions = plan_player(script, double.plan());
    let end = script.events.last().map_or(0.0, Event::t) + 30.0;
    let mut client = Client::new(settings, data.clone());
    client.observe = vec![RESET_CONTENTS, DESTROY_CONTAINED];
    let mut double = double;
    let mut o = Outcome {
        world_logins_recorded: script.conns.len() - 1,
        actions: actions.len(),
        recorded: script
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Msg { t, msg } if msg.c2s => Some((*t, msg.clone())),
                _ => None,
            })
            .collect(),
        ..Outcome::default()
    };
    let me = client_addr();
    let mut now = 0.0f64;
    let mut next_action = 0usize;
    let mut fired: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut seen = vec![0usize; script.conns.len()];
    let mut connect_at: BTreeMap<usize, f64> = BTreeMap::new();
    let mut was_up = false;
    let mut last_progress = (0usize, 0usize);
    let mut progressed_at = 0.0f64;
    let mut waiting: Option<(Gate, f64)> = None;
    let mut players: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    loop {
        let step = if client.moving() { 1.0 / 60.0 } else { 0.1 };
        now += step;
        let t = LocalTime(1.0 + now);
        client.frame(t);
        while let Some((to, bytes)) = client.poll_transmit() {
            double.datagram(to, me, &bytes, t);
        }
        for d in double.tick(t) {
            client.handle_datagram(d.from, &d.bytes, t);
        }
        // What has gone out, for the actions' triggers.
        for (conn, seen) in seen.iter_mut().enumerate() {
            if double.is_connected(conn) {
                connect_at.entry(conn).or_insert(now);
            }
            let r = double.released(conn);
            for count in *seen + 1..=r {
                fired.insert((conn, count), now);
            }
            *seen = (*seen).max(r);
        }
        while let Some(p) = actions.get(next_action) {
            let since = match p.trigger {
                Trigger::Start => Some(0.0),
                Trigger::Connect(c) => connect_at.get(&c).copied(),
                Trigger::Released { conn, count } => fired.get(&(conn, count)).copied(),
            };
            let Some(since) = since else { break };
            if now < since + p.delay {
                break;
            }
            for a in &p.action {
                client.act(a.clone());
            }
            next_action += 1;
            o.actions_done += 1;
        }
        // The server's word on the player's inventory, against the model's.
        for (_, function, args, _) in std::mem::take(&mut client.observed) {
            check_inventory(&client, function, args.as_ref(), &mut o.inventory_check);
        }
        // The login connection, once up, stays up.
        let up = client.session.login_connected();
        if was_up && !up && !double.said_goodbye(0) {
            o.login_drops += 1;
            o.drop_at.get_or_insert(now);
        }
        was_up = up;
        if let Some(p) = client.model.player_object() {
            players.insert(p.id);
        }
        // A gate the client does not fire holds the double up: give up waiting 30 s after the
        // recorded client fired it.
        match (double.waiting_on(), waiting) {
            (Some(g), Some((w, since))) if g == w => {
                if now - since > double.recorded_wait(g) + 30.0 {
                    double.force_next_gate(t);
                    waiting = None;
                }
            }
            (Some(g), _) => waiting = Some((g, now)),
            (None, _) => waiting = None,
        }
        // The end: every action done and every item out; or nothing moved for ten minutes.
        let progress = (next_action, seen.iter().sum::<usize>());
        // An action whose trigger went out and whose recorded delay is still running is waited
        // for, not stalled on (the recorded player sat idle).
        let action_due = actions.get(next_action).is_some_and(|p| match p.trigger {
            Trigger::Start => true,
            Trigger::Connect(c) => connect_at.contains_key(&c),
            Trigger::Released { conn, count } => fired.contains_key(&(conn, count)),
        });
        if progress != last_progress || double.item_due() || action_due {
            last_progress = progress;
            progressed_at = now;
        }
        if next_action == actions.len() && double.finished() && now > end {
            break;
        }
        if next_action == actions.len() && now - progressed_at > 600.0 {
            // Every action done and nothing more will go out: the rest was recorded on connections
            // the client never had.
            break;
        }
        if now - progressed_at > 600.0 || now > end * 4.0 + 3600.0 {
            o.stalled = true;
            o.stall = format!(
                "at {now:.0} s: next action {:?} of {}, the double waits on {:?}: {}",
                actions.get(next_action).map(|p| (
                    p.t,
                    p.trigger,
                    p.action
                        .first()
                        .map(|a| format!("{a:?}").chars().take(40).collect::<String>())
                )),
                actions.len(),
                double.waiting_on(),
                double.describe_waits()
            );
            break;
        }
    }
    let report = double.report();
    o.world_logins = client.session.stats().world_logins;
    o.connected = report.connected;
    o.log = report.log;
    o.referrals = report.referrals;
    o.switches = report.switches;
    o.login_up = client.session.login_connected();
    o.gates_forced = report.gates_forced;
    o.login = report.login;
    o.unmatched_gates = report.unmatched_gates;
    o.sent = client.sent().to_vec();
    o.received = report.received;
    o.calls = client.calls.clone();
    let created: std::collections::BTreeSet<u64> = script
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Msg { msg, .. } if !msg.c2s && msg.mtype == 0x10060 => {
                match ServerMessage::decode(&msg.raw) {
                    Ok(ServerMessage::PlayerObject(p)) => Some(p.id),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    o.players_created = created.len();
    o.players_held = created.intersection(&players).count();
    o.inventory = client.model.inventory().len();
    o.surroundings = client.model.surroundings().count();
    o.virtual_seconds = now;
    if let Some(s) = client.scripts.as_ref() {
        o.notices = s.host.notices.clone();
    }
    let q = client.queue.stats();
    o.waited = q.waited;
    o.dropped = q.dropped;
    o
}
