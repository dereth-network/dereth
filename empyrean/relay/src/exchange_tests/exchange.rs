//! The whole exchange: the later login, the dat download through the later messages, a character
//! made, the world entered, a step taken, a refused action, and the goodbye.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::time::Duration;

use crate::relay::{Config, Relay, SOFTWARE};
use dereth_primitives::era::{EraFeatureBits, EraFeatures, EraId};
use dereth_primitives::{IncomingMessage, NetQueue, ObjectId};
use dereth_protocol::admin::{
    DddBeginDdd, DddData, DddInterrogation, DddInterrogationResponse, DddOverlayManifest,
    MostlyConsecutiveIntSet, OverlayBase, TaggedIterationList,
};
use dereth_protocol::login::{
    CharGenResult, CharacterSendCharGenResult, LoginCharacterSet, LoginPlayerDescription,
    LoginSendEnterWorld,
};
use dereth_protocol::{read_body, write_blob};
use dereth_transport::status_ping::{self, StatusReply};
use empyrean_net::testing::{ClientStatus, TestClient};
use empyrean_net::{ClockSnapshot, ClockSnapshotExt as _, PortKind};

use super::standin::{
    Standin, CHARACTER, FLAVOR_LINE, KNOWN_SPELL, PORTAL_RECORD, SEEDED_CHARGE, STORM_SCRIPT,
};

const PORT: u16 = 9000;
const TICK: Duration = Duration::from_millis(20);
/// The most virtual time one wait may take: a guard against a hang, never the expected time.
const GUARD: u32 = 30_000;

fn relay_addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn client_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 50_000))
}

fn upstream() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 9110))
}

/// The relay, its two peers, the virtual clock, and a transcript of each message each way.
struct Exchange {
    relay: Relay,
    client: TestClient,
    server: Standin,
    now: ClockSnapshot,
    received: Vec<IncomingMessage>,
    transcript: String,
    seen_by_server: usize,
}

fn config() -> Config {
    Config {
        port: PORT,
        upstream: upstream(),
        world: "Stand-in".to_owned(),
        seed: 7,
    }
}

impl Exchange {
    fn new(account: &str, password: &str) -> Self {
        Self {
            relay: Relay::new(config(), Box::new(|| [9; 32])),
            client: TestClient::new(client_addr(), relay_addr(PORT), account, password),
            server: Standin::new(),
            now: ClockSnapshot::at_seconds(1.0),
            received: Vec::new(),
            transcript: String::new(),
            seen_by_server: 0,
        }
    }

    fn step(&mut self) {
        let t = self.now.monotonic.as_secs_f64();
        self.client.tick(t);
        for (to, bytes) in self.client.take_outgoing() {
            let kind = if to.port() == PORT {
                PortKind::C2S
            } else {
                PortKind::S2C
            };
            self.relay
                .on_later_datagram(kind, client_addr(), &bytes, self.now);
        }
        self.relay.step(self.now);
        for out in self.relay.take_for_dereth() {
            let port = match out.via_port_kind {
                PortKind::C2S => PORT,
                PortKind::S2C => PORT + 1,
            };
            self.client.handle_datagram(relay_addr(port), &out.bytes, t);
        }
        for out in self.relay.take_for_servers() {
            assert_eq!(out.to, upstream());
            self.server.datagram(&out.bytes);
            let answering: Vec<_> = self.relay.sessions().collect();
            while let Some(answer) = self.server.take() {
                self.relay
                    .on_server_datagram(answering[0], upstream(), &answer, self.now);
            }
        }
        for message in &self.server.received[self.seen_by_server..] {
            let opcode = u32::from_le_bytes(message[..4].try_into().unwrap());
            let _ = writeln!(self.transcript, "to-server {opcode:04X} {}", hex(message));
        }
        self.seen_by_server = self.server.received.len();
        while let Some(m) = self.client.poll() {
            let _ = writeln!(
                self.transcript,
                "to-dereth {:04X} {}",
                m.opcode,
                hex(&m.body)
            );
            self.received.push(m);
        }
        self.now = self.now.advanced(TICK);
    }

    /// Steps until a message of `opcode` has arrived; every message so far stays readable.
    fn until(&mut self, opcode: u32) -> IncomingMessage {
        for _ in 0..GUARD {
            if let Some(m) = self.received.iter().find(|m| m.opcode == opcode) {
                return m.clone();
            }
            self.step();
        }
        panic!(
            "no {opcode:#06X}; received {:04X?}; the server has {:?} and {} message(s)",
            self.received.iter().map(|m| m.opcode).collect::<Vec<_>>(),
            self.server.reported,
            self.server.received.len(),
        );
    }

    /// Steps until a game event of `opcode` has arrived, in its ordered envelope.
    fn until_wrapped(&mut self, opcode: u32) -> IncomingMessage {
        for _ in 0..GUARD {
            if let Some(m) = self
                .received
                .iter()
                .find(|m| m.opcode == 0xF7B0 && wrapped(m) == opcode)
            {
                return m.clone();
            }
            self.step();
        }
        panic!("no wrapped {opcode:#06X}");
    }

    /// Steps until the server has what `want` looks for.
    fn until_server(&mut self, want: impl Fn(&Standin) -> bool) {
        for _ in 0..GUARD {
            if want(&self.server) {
                return;
            }
            self.step();
        }
        panic!("the server never got it");
    }

    fn forget(&mut self, opcode: u32) {
        self.received.retain(|m| m.opcode != opcode);
    }

    fn answer_interrogation(&mut self) {
        let list = |t, i, n: i32| TaggedIterationList {
            dat_file_type: t,
            dat_file_id: i,
            iterations: MostlyConsecutiveIntSet {
                iterations: n,
                ints: vec![-n, 1],
            },
        };
        let base = |t, i, b| OverlayBase {
            dat_file_type: t,
            dat_file_id: i,
            fingerprint: [b; 32],
        };
        let answer = write_blob(&DddInterrogationResponse {
            client_language: 1,
            iters_with_keys: vec![list(0, 1, 102), list(1, 2, 67)],
            iters_without_keys: Vec::new(),
            flags: DddInterrogationResponse::FLAG_OVERLAY,
            overlay_bases: vec![base(0, 1, 1), base(1, 2, 2)],
        })
        .unwrap();
        self.client.send(NetQueue::ClientCache, &answer);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The ordered envelope's wrapped message.
fn wrapped(m: &IncomingMessage) -> u32 {
    assert_eq!(m.opcode, 0xF7B0);
    u32::from_le_bytes(m.body[8..12].try_into().unwrap())
}

/// The whole session, ending once the client has said goodbye.
fn session() -> Exchange {
    let mut x = Exchange::new("Probe", "Secret");
    let asked = x.until(0xF7E5);
    let asked = read_body::<DddInterrogation>(&asked.body).unwrap();
    assert_eq!(asked.supported_languages, [0, 1]);
    assert!(
        x.server.reported.is_none(),
        "the upstream login waits for what Dereth holds"
    );
    x.answer_interrogation();

    // The later download, whole, before the character list.
    let manifest = x.until(0xF7EC);
    let manifest = read_body::<DddOverlayManifest>(&manifest.body).unwrap();
    assert_eq!(x.server.reported, Some((102, 67)));
    assert_eq!(manifest.world_key, "Stand-in");
    assert_eq!(manifest.files[0].base_fingerprint, [1; 32]);
    assert_eq!(manifest.files[0].records.len(), 1, "one record held back");
    let begin = x.until(0xF7E7);
    let begin = read_body::<DddBeginDdd>(&begin.body).unwrap();
    assert_eq!(begin.revisions[0].iteration, 105);
    assert_eq!(begin.revisions[0].ids_to_download, [PORTAL_RECORD]);
    let data = x.until(0xF7E2);
    let data = read_body::<DddData>(&data.body).unwrap();
    assert_eq!((data.resource_id, data.iteration), (PORTAL_RECORD, 105));
    assert_eq!(data.data, b"made up data");
    assert!(
        !x.received.iter().any(|m| m.opcode == 0xF658),
        "the character list waits for the download's end"
    );
    x.client
        .send(NetQueue::ClientCache, &0xF7EAu32.to_le_bytes());
    x.until(0xF7EA);

    // The list, with the account the server named, and the world's name.
    let set = x.until(0xF658);
    let set = read_body::<LoginCharacterSet>(&set.body).unwrap();
    assert_eq!(set.account, "probe");
    assert!(set.characters.is_empty());
    x.until(0xF7E1);
    x.forget(0xF658);

    // A character.
    let create = write_blob(&CharacterSendCharGenResult {
        account: set.account.clone(),
        result: CharGenResult {
            heritage_group: 1,
            gender: 1,
            name: "Tester".into(),
            ..CharGenResult::default()
        },
    })
    .unwrap();
    x.client.send(NetQueue::Logon, &create);
    x.until(0xF643);
    let set = x.until(0xF658);
    let set = read_body::<LoginCharacterSet>(&set.body).unwrap();
    assert_eq!(set.characters[0].gid, ObjectId(CHARACTER));

    // The two-phase entry.
    x.client.send(NetQueue::Logon, &0xF7C8u32.to_le_bytes());
    x.until(0xF7DF);
    let enter = write_blob(&LoginSendEnterWorld {
        character: ObjectId(CHARACTER),
        account: set.account.clone(),
    })
    .unwrap();
    x.client.send(NetQueue::Logon, &enter);
    x.until(0xF746);
    x.until(0xF745);
    x.until(0xF7DB);
    for _ in 0..GUARD {
        if x.received
            .iter()
            .any(|m| m.opcode == 0xF7B0 && wrapped(m) == 0x0013)
        {
            break;
        }
        x.step();
    }
    let description = x
        .received
        .iter()
        .find(|m| m.opcode == 0xF7B0 && wrapped(m) == 0x0013)
        .expect("the player description");
    let d = read_body::<LoginPlayerDescription>(&description.body[12..]).unwrap();
    let experience = d.qualities.base.tables.int64s.expect("the later table");
    assert_eq!(experience.entries, [(1, 500)]);
    // The login seeds the meter: the first of the page's three floats is the charge.
    let book = d.qualities.spell_book.expect("the spell book");
    assert_eq!(book.entries[0].0, KNOWN_SPELL);
    assert_eq!(book.entries[0].1.legacy, Some((SEEDED_CHARGE, 0)));

    // A step: forwarded in the 1999 client's envelope, and the server's answer states its `w`.
    let mut step = 0xF7B1u32.to_le_bytes().to_vec();
    step.extend(1u32.to_le_bytes());
    step.extend(0xF61Cu32.to_le_bytes());
    step.extend([1, 2, 3, 4]);
    x.client.send(NetQueue::Weenie, &step);
    let position = x.until(0xF748);
    let position =
        read_body::<dereth_protocol::movement::MovementPositionEvent>(&position.body).unwrap();
    assert_eq!(position.position.orientation.w, 0.0);

    // An action the 1999 client never had stops at the relay, with a line in the chat.
    let before = x.server.received.len();
    let mut trade = 0xF7B1u32.to_le_bytes().to_vec();
    trade.extend(2u32.to_le_bytes());
    trade.extend(0x01F6u32.to_le_bytes());
    x.client.send(NetQueue::Weenie, &trade);
    for _ in 0..GUARD {
        if x.received
            .iter()
            .any(|m| m.opcode == 0xF7E0 || (m.opcode == 0xF7B0 && wrapped(m) == 0xF7E0))
        {
            break;
        }
        x.step();
    }
    assert!(x
        .received
        .iter()
        .any(|m| { m.opcode == 0xF7E0 || (m.opcode == 0xF7B0 && wrapped(m) == 0xF7E0) }));
    assert_eq!(
        x.server.received.len(),
        before,
        "nothing reached the server"
    );

    // A cast: the known spell's update, the magic line, and the script as the server numbered it.
    x.forget(0xF7E0);
    let mut cast = 0xF7B1u32.to_le_bytes().to_vec();
    cast.extend(3u32.to_le_bytes());
    cast.extend(0x004Au32.to_le_bytes());
    cast.extend(KNOWN_SPELL.to_le_bytes());
    x.client.send(NetQueue::Weenie, &cast);
    let update = x.until_wrapped(0x02C1);
    assert_eq!(update.body[12..], KNOWN_SPELL.to_le_bytes());
    let line = x.until(0xF7E0);
    let line = read_body::<dereth_protocol::comms::CommunicationTextboxString>(&line.body).unwrap();
    assert_eq!((line.text.as_str(), line.text_type), (FLAVOR_LINE, 7));
    let script = x.until(0xF755);
    assert_eq!(script.body[4..8], STORM_SCRIPT.to_le_bytes());

    // The goodbye reaches the server.
    x.client.log_off();
    x.until_server(|s| s.goodbye);
    x
}

/// Behaviour: none (the login's account field).
#[test]
fn a_session_logs_in_downloads_creates_enters_steps_and_says_goodbye() {
    let x = session();
    assert_eq!(x.client.status(), ClientStatus::Done);
    assert_eq!(
        &x.server.login_body[20..33],
        b"probe:secret\0",
        "lowercased, the 1999 form"
    );
    let creates: Vec<_> = x
        .server
        .received
        .iter()
        .filter(|m| m[..4] == 0xF656u32.to_le_bytes())
        .collect();
    let create =
        dereth_protocol::read_checked::<crate::codec::CreateCharacter>(creates[0]).unwrap();
    assert_eq!(create.account, "probe");
    assert_eq!(create.character.heritage, 0, "the 1999 list's own place");
}

/// The transcript's expected form. `cargo test -p empyrean-relay -- --ignored` rewrites it.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/synthetic-session.txt"
);

#[test]
fn the_recorded_synthetic_session_is_what_the_relay_sends_today() {
    let want = std::fs::read_to_string(FIXTURE).expect("the recorded session");
    let got = session().transcript;
    assert!(got == want, "the exchange changed; see {FIXTURE}");
}

#[test]
#[ignore = "instrument: records the synthetic session, our own codecs' output only"]
fn record_the_synthetic_session() {
    std::fs::write(FIXTURE, session().transcript).unwrap();
}

#[test]
fn credentials_the_1999_login_cannot_hold_are_refused_with_the_reason() {
    let mut x = Exchange::new("averylongaccountname", "andalongpassword");
    let booted = x.until(LOGIN_ACCOUNT_BOOTED);
    let reason = read_body::<dereth_protocol::login::LoginAccountBooted>(&booted.body)
        .unwrap()
        .reason
        .unwrap();
    assert!(reason.contains("20 characters"), "{reason}");
    assert!(x.server.reported.is_none(), "nothing went upstream");
    assert!(x.relay.take_for_servers().is_empty());
    for _ in 0..GUARD {
        x.step();
        if !x.relay.take_finished().is_empty() {
            return;
        }
    }
    panic!("a refused session never counted as ended, so `--once` would wait for ever");
}

const LOGIN_ACCOUNT_BOOTED: u32 = dereth_protocol::Opcode::LOGIN_ACCOUNT_BOOTED.0;

#[test]
fn the_status_ping_answers_as_a_launch_world_from_the_era_table() {
    let mut relay = Relay::new(config(), Box::new(|| [9; 32]));
    let now = ClockSnapshot::at_seconds(1000.0);
    relay.on_later_datagram(PortKind::C2S, client_addr(), &status_ping::hello(), now);
    relay.step(now);
    let token = relay.take_for_dereth().pop().expect("the token");
    let token = status_ping::parse_token_reply(&token.bytes).unwrap();
    relay.on_later_datagram(PortKind::C2S, client_addr(), &status_ping::ask(token), now);
    relay.step(now);
    let reply = relay.take_for_dereth().pop().expect("the status");
    let status = StatusReply::parse(&reply.bytes).unwrap();
    assert_eq!(status.era, EraId::Launch.name());
    assert_eq!(status.software, SOFTWARE);
    assert_eq!(status.world_name, "Stand-in");
    let launch = EraFeatureBits::of(EraId::Launch.features());
    assert_eq!(status.era_table_version, launch.table_version);
    assert_eq!(status.era_features, launch.bytes);
    assert_eq!(launch.table_version, EraFeatures::TABLE_VERSION);
    assert_ne!(
        status.era_features,
        EraFeatureBits::of(EraId::Eor.features()).bytes
    );
}
