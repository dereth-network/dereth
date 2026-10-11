//! The Dereth Network between worlds: tells, replies, friends, the Global room, channels,
//! squelches, a lost hub, a world that is not on the network, and `/dn verify`'s answer to the
//! hub's challenge.
//!
//! Fixture: several virtual-time `TestServer`s, each a world with its own characters and its
//! network half configured, linked through the hub's own routing (`dereth_network_hub::core`) in
//! the same process. Each step carries every frame a world queued to the hub and every frame the
//! hub sends back, so a test sees exactly what each side's client receives.

mod conformance;
mod friends;
mod link;
mod persons;
mod rooms;
mod squelch;
mod tells;
mod verify;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_network_hub::core::{Channels, DirEntry, Directory, Hub, Out};
use dereth_network_protocol::limits::Limits;
use dereth_network_protocol::{Federation, Message};
use dereth_primitives::{NetQueue, ObjectId};
use dereth_protocol::comms::{
    CommunicationTalk, CommunicationTalkDirect, CommunicationTalkDirectByName,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::login::{LoginCharacterSet, LoginSendEnterWorld, LoginSendEnterWorldRequest};
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, CharacterOption, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, ClientId, ClientStatus, EmptyShard, TestServer};
use empyrean_world::federation::{self, FederationSettings, NetworkStore, RoomSlot};
use empyrean_world::managers::{guid_manager, player_manager};
use empyrean_world::world_objects::player_location;

pub(crate) const HOME: u16 = 0xA9B4;

pub(crate) const GAME_EVENT: u32 = 0xF7B0;
pub(crate) const TEXTBOX: u32 = 0xF7E0;
pub(crate) const TELL: u32 = 0x02BD;
pub(crate) const WEENIE_ERROR: u32 = 0x028A;
pub(crate) const WEENIE_ERROR_STR: u32 = 0x028B;
pub(crate) const FRIENDS_UPDATE: u32 = 0x0021;
pub(crate) const TURBINE_CHAT: u32 = 0xF7DE;

/// What a client is shown: the chat-relevant messages, by kind (a game event's type, else the
/// opcode), with their blobs.
#[derive(Debug, Clone)]
pub(crate) struct Got {
    pub kind: u32,
    pub blob: Vec<u8>,
}

impl Got {
    pub fn decode<M: dereth_protocol::Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }

    pub fn plain<M: dereth_protocol::Message>(&self) -> M {
        dereth_protocol::read_body_padded::<M>(&self.blob[4..])
            .unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }
}

const CHAT_KINDS: [u32; 6] = [
    TEXTBOX,
    TELL,
    WEENIE_ERROR,
    WEENIE_ERROR_STR,
    FRIENDS_UPDATE,
    TURBINE_CHAT,
];

/// A character to seed: account, id, name, and the room options it listens to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seed {
    pub account: &'static str,
    pub guid: u32,
    pub name: &'static str,
    pub listens: &'static [CharacterOption],
    pub access: AccessLevel,
}

pub(crate) fn seed(account: &'static str, guid: u32, name: &'static str) -> Seed {
    Seed {
        account,
        guid,
        name,
        listens: &[CharacterOption::ListenToRoleplayChat],
        access: AccessLevel::Player,
    }
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    let table = SecondaryAttributeTable {
        id: dereth_primitives::DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .build()
        .expect("fake dats")
}

fn outdoor(x: f32, y: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    Position::from_components(
        u32::from(HOME) << 16 | cell,
        x,
        y,
        20.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

fn put(ts: &TestServer, s: &Seed, at: Position) {
    let account_id = ts
        .auth()
        .create_account(s.account, "pw", s.access, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .expect("created")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: s.guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(PropertyDataId::CombatTable, 0x3000_0000);
    biota.set_property(PropertyString::Name, s.name.to_owned());
    biota.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    biota.set_property(PropertyBool::ReportCollisions, true);
    biota.set_property(PropertyBool::IgnoreCollisions, false);
    let attributes = biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    for a in &PropertyAttribute::ALL[1..] {
        attributes.insert(
            *a,
            PropertiesAttribute {
                init_level: 100,
                ..PropertiesAttribute::default()
            },
        );
    }
    let vitals = biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        vitals.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                current_level: 100,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    biota
        .properties_position
        .get_or_insert_with(DotNetDict::new)
        .insert(
            PositionType::Location,
            PropertiesPosition {
                obj_cell_id: at.cell(),
                position_x: at.position_x,
                position_y: at.position_y,
                position_z: at.position_z,
                rotation_w: at.rotation_w,
                rotation_x: at.rotation_x,
                rotation_y: at.rotation_y,
                rotation_z: at.rotation_z,
            },
        );
    let options2 = s
        .listens
        .iter()
        .map(|o| {
            o.character_options2()
                .expect("a CharacterOptions2 option")
                .0
        })
        .fold(0u32, |a, b| a | b)
        .cast_signed();
    let character = Character {
        id: s.guid,
        account_id,
        name: s.name.to_owned(),
        character_options_2: options2,
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
}

/// One world of the network.
pub(crate) struct World {
    pub slug: &'static str,
    pub ts: TestServer,
    pub linked: bool,
    pub clients: BTreeMap<&'static str, ClientId>,
}

/// The registry, as the hub's routing reads it: each world's flag, and the hub's own registry for
/// persons (its worlds on the network too).
#[derive(Debug)]
pub(crate) struct Registry(
    pub BTreeMap<String, Federation>,
    pub dereth_network_hub::registry::Registry,
);

impl Registry {
    fn new() -> Self {
        Self(
            BTreeMap::new(),
            dereth_network_hub::registry::Registry::open(
                ":memory:",
                "dereth.network",
                Box::new(std::time::SystemTime::now),
            )
            .expect("the hub's registry"),
        )
    }

    /// `slug` on the network, in both.
    fn add(&mut self, slug: &str) {
        self.0.insert(slug.to_owned(), Federation::On);
        let reg = dereth_network_hub::registry::Registration::from_json(&serde_json::json!({
            "slug": slug, "name": slug.to_uppercase(), "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration");
        self.1.register(&reg).expect("registered");
        self.1
            .set_federation(slug, Federation::On)
            .expect("on the network");
    }
}

impl Directory for Registry {
    fn lookup(&self, slug: &str) -> Option<DirEntry> {
        self.0.get(slug).map(|&federation| DirEntry {
            name: slug.to_uppercase(),
            federation,
        })
    }
    fn person_code(
        &self,
        who: &dereth_network_protocol::Address,
    ) -> Result<(String, u32), dereth_network_protocol::PersonOutcome> {
        Directory::person_code(&self.1, who)
    }
    fn person_unlink(
        &self,
        who: &dereth_network_protocol::Address,
    ) -> dereth_network_hub::core::PersonAnswer {
        Directory::person_unlink(&self.1, who)
    }
    fn person_alts(
        &self,
        who: &dereth_network_protocol::Address,
        shown: bool,
    ) -> dereth_network_hub::core::PersonAnswer {
        Directory::person_alts(&self.1, who, shown)
    }
    fn person_befriend(
        &self,
        who: &dereth_network_protocol::Address,
        target: &dereth_network_protocol::Address,
    ) -> dereth_network_hub::core::PersonAnswer {
        Directory::person_befriend(&self.1, who, target)
    }
    fn person_unfriend(
        &self,
        who: &dereth_network_protocol::Address,
        target: &dereth_network_protocol::Address,
    ) -> dereth_network_hub::core::PersonAnswer {
        Directory::person_unfriend(&self.1, who, target)
    }
    fn person_carry(
        &self,
        who: &dereth_network_protocol::Address,
        targets: &[dereth_network_protocol::Address],
    ) -> dereth_network_hub::core::PersonAnswer {
        Directory::person_carry(&self.1, who, targets)
    }
    fn returns_sent(&self, who: &dereth_network_protocol::Address) {
        Directory::returns_sent(&self.1, who);
    }
    fn person_characters(
        &self,
        target: &dereth_network_protocol::Address,
    ) -> Vec<dereth_network_protocol::Address> {
        Directory::person_characters(&self.1, target)
    }
    fn person_view(
        &self,
        who: &dereth_network_protocol::Address,
    ) -> dereth_network_hub::core::PersonView {
        Directory::person_view(&self.1, who)
    }
}

/// Every frame each way, by the retail name of what it carried.
#[derive(Debug, Default, Clone)]
pub(crate) struct WireLog(pub Vec<String>);

/// Every message each world sent the hub, in order.
#[derive(Debug, Default, Clone)]
pub(crate) struct SentLog(pub Vec<(&'static str, Message)>);

/// The network: the hub's routing and the worlds.
pub(crate) struct Net {
    pub hub: Hub,
    pub registry: Registry,
    pub worlds: Vec<World>,
    pub wire: WireLog,
    pub sent: SentLog,
    /// Every message the hub handed each world, in order.
    pub handed: Vec<(String, Message)>,
}

impl Net {
    /// Worlds by slug, each with its characters; each world's network half on, carrying the
    /// Global room in Roleplay, and in the registry with its flag on.
    pub fn new(worlds: &[(&'static str, &[Seed])]) -> Self {
        let mut registry = Registry::new();
        let mut out = Vec::new();
        for &(slug, seeds) in worlds {
            registry.add(slug);
            let mut ts = TestServer::with_dats(dats());
            land::use_flat_land_with_test_setup(&mut ts.world, &[HOME], 10);
            ts.world.content = Arc::new(
                MemContent::new().weenie(
                    Weenie::new(1, "human", WeenieType::Creature)
                        .with_did(PropertyDataId::CombatTable, 0x3000_0000),
                ),
            );
            guid_manager::initialize(&mut ts.world, &mut EmptyShard);
            for (i, s) in seeds.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                put(&ts, s, outdoor(60.0 + 8.0 * i as f32, 100.0));
            }
            player_manager::initialize(&mut ts.world);
            empyrean_command::command_manager::initialize(None);
            federation::configure(
                &mut ts.world,
                FederationSettings {
                    enabled: true,
                    slug: slug.to_owned(),
                    global_room: Some(RoomSlot::Roleplay),
                    min_character_age_secs: 0,
                },
                NetworkStore::default(),
            );
            out.push(World {
                slug,
                ts,
                linked: false,
                clients: BTreeMap::new(),
            });
        }
        let mut hub = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        hub.set_link_url(Some("https://site.test/characters/".into()));
        hub.set_person_url(Some("https://site.test/characters/".into()));
        Self {
            hub,
            registry,
            worlds: out,
            wire: WireLog::default(),
            sent: SentLog::default(),
            handed: Vec::new(),
        }
    }

    pub fn world(&mut self, slug: &str) -> &mut World {
        self.worlds
            .iter_mut()
            .find(|w| w.slug == slug)
            .unwrap_or_else(|| panic!("no world {slug}"))
    }

    fn now(&self) -> f64 {
        self.worlds[0].ts.world.now.monotonic.as_secs_f64()
    }

    /// Logs `account` in on `slug` and enters the world with its character.
    pub fn enter(&mut self, slug: &str, s: Seed) -> ClientId {
        let w = self.world(slug);
        let id = w.ts.connect(s.account, "pw");
        assert_eq!(
            w.ts.client(id).status(),
            ClientStatus::Connected,
            "{} logs in",
            s.account
        );
        assert!(
            w.ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
            "the character list arrives"
        );
        w.ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
        w.ts.advance(0.1);
        w.ts.send_message(
            id,
            NetQueue::Logon,
            &LoginSendEnterWorld {
                character: ObjectId(s.guid),
                account: s.account.to_owned(),
            },
        );
        let g = ObjectGuid::new(s.guid);
        assert!(
            w.ts.run_until(1.0, |ts| ts
                .world
                .objects
                .get(g)
                .is_some_and(|o| o.current_landblock.is_some())),
            "{} enters the world",
            s.account
        );
        player_location::on_teleport_complete(&mut w.ts.world, g);
        w.ts.advance(0.3);
        let _ = TestServer::take_not_ported();
        w.clients.insert(s.name, id);
        self.pump();
        id
    }

    /// The link of `slug` comes up and is welcomed.
    pub fn link(&mut self, slug: &str) {
        let name = slug.to_uppercase();
        let outs = self.hub.connect(slug, &name);
        self.route(outs);
        let w = self.world(slug);
        w.linked = true;
        federation::on_link_up(&mut w.ts.world);
        self.pump();
        // What the worlds sent their clients on the way reaches them.
        self.advance(0.3);
    }

    /// The link of `slug` goes down.
    pub fn unlink(&mut self, slug: &str) {
        let outs = self.hub.disconnect(slug);
        let w = self.world(slug);
        w.linked = false;
        federation::on_link_down(&mut w.ts.world);
        self.route(outs);
        self.pump();
        self.advance(0.3);
    }

    fn route(&mut self, outs: Vec<Out>) {
        for o in outs {
            match o {
                Out::Send(slug, m) => {
                    self.wire.0.push(format!("hub -> {slug}: {}", m.kind()));
                    self.handed.push((slug.clone(), m.clone()));
                    let w = self.world(&slug);
                    if w.linked {
                        federation::on_link_frame(&mut w.ts.world, &m.to_json());
                    }
                }
                Out::Close(slug, _) => {
                    let w = self.world(&slug);
                    if w.linked {
                        w.linked = false;
                        federation::on_link_down(&mut w.ts.world);
                    }
                }
                Out::Heartbeat { .. } | Out::ChannelsChanged => {}
            }
        }
    }

    /// `slug` sends `m` to the hub as it is, as a world that writes its own frames would (one
    /// that follows no rule of this world's own), and the network carries the outcome.
    pub fn world_sends(&mut self, slug: &str, m: Message) {
        let now = self.now();
        let outs = self.hub.receive(&self.registry, slug, m, now);
        self.route(outs);
        self.pump();
    }

    /// The hub hands `slug` the message `m`, as a hub that writes its own frames would.
    pub fn hub_hands(&mut self, slug: &str, m: &Message) {
        let w = self.world(slug);
        federation::on_link_frame(&mut w.ts.world, &m.to_json());
        self.pump();
    }

    /// What `slug` has sent the hub since `mark` (a length of [`Net::sent`]).
    pub fn sent_since(&self, slug: &str, mark: usize) -> Vec<Message> {
        self.sent.0[mark..]
            .iter()
            .filter(|(s, _)| *s == slug)
            .map(|(_, m)| m.clone())
            .collect()
    }

    /// Carries frames both ways until nobody has anything to send.
    pub fn pump(&mut self) {
        for _ in 0..50 {
            let now = self.now();
            let mut moved = false;
            for i in 0..self.worlds.len() {
                if !self.worlds[i].linked {
                    continue;
                }
                let slug = self.worlds[i].slug;
                let frames = federation::take_frames(&mut self.worlds[i].ts.world);
                for f in frames {
                    moved = true;
                    let m = Message::from_json(&f).expect("a world sends messages");
                    self.wire.0.push(format!("{slug} -> hub: {}", m.kind()));
                    self.sent.0.push((slug, m.clone()));
                    let outs = self.hub.receive(&self.registry, slug, m, now);
                    self.route(outs);
                }
            }
            if !moved {
                return;
            }
        }
        panic!("the network never settled");
    }

    /// Lets `secs` of virtual time pass on every world, carrying frames as they go.
    pub fn advance(&mut self, secs: f64) {
        let steps = (secs / 0.05).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..steps as u32 {
            for w in &mut self.worlds {
                w.ts.advance(0.05);
                federation::tick(&mut w.ts.world);
            }
            let now = self.now();
            let outs = self.hub.tick(&self.registry, now);
            self.route(outs);
            self.pump();
        }
    }

    /// `line` typed in `who`'s chat box (a command, sent as `Communication_Talk`).
    pub fn say(&mut self, slug: &str, who: &str, line: &str) {
        let w = self.world(slug);
        let id = w.clients[who];
        w.ts.send_game_action(
            id,
            &CommunicationTalk {
                message: line.to_owned(),
            },
        );
        self.advance(0.2);
    }

    /// `@tell <target>, <text>` typed by `who` (`Communication_TalkDirectByName`).
    pub fn tell(&mut self, slug: &str, who: &str, target: &str, text: &str) {
        let w = self.world(slug);
        let id = w.clients[who];
        w.ts.send_game_action(
            id,
            &CommunicationTalkDirectByName {
                message: text.to_owned(),
                target_name: target.to_owned(),
            },
        );
        self.advance(0.3);
    }

    /// A reply by id (`Communication_TalkDirect`).
    pub fn tell_id(&mut self, slug: &str, who: &str, target: u32, text: &str) {
        let w = self.world(slug);
        let id = w.clients[who];
        w.ts.send_game_action(
            id,
            &CommunicationTalkDirect {
                message: text.to_owned(),
                target: ObjectId(target),
            },
        );
        self.advance(0.3);
    }

    /// How many messages `who` has received so far (a mark for [`Net::since`]).
    pub fn mark(&mut self, slug: &str, who: &str) -> usize {
        let w = self.world(slug);
        w.ts.received_raw(w.clients[who]).len()
    }

    /// The chat-relevant messages `who` received after `mark`.
    pub fn since(&mut self, slug: &str, who: &str, mark: usize) -> Vec<Got> {
        let w = self.world(slug);
        w.ts.received_raw(w.clients[who])[mark..]
            .iter()
            .map(|m| {
                let mut blob = m.opcode.to_le_bytes().to_vec();
                blob.extend_from_slice(&m.body);
                let kind = if m.opcode == GAME_EVENT {
                    u32::from_le_bytes(m.body[8..12].try_into().expect("an event header"))
                } else {
                    m.opcode
                };
                Got { kind, blob }
            })
            .filter(|g| CHAT_KINDS.contains(&g.kind))
            .collect()
    }

    /// The text of every system line (`Communication_TextboxString`) among `got`.
    pub fn lines(got: &[Got]) -> Vec<String> {
        got.iter()
            .filter(|g| g.kind == TEXTBOX)
            .map(|g| {
                g.plain::<dereth_protocol::comms::CommunicationTextboxString>()
                    .text
            })
            .collect()
    }

    /// Every frame since `mark` of the wire log, as `from -> to: type`.
    pub fn wire_since(&self, mark: usize) -> Vec<String> {
        self.wire.0[mark..].to_vec()
    }
}

/// Ann on aa, Bob on bb, each on the network, linked, in the world; Ann and Bob are mutual
/// friends when `friends` is set.
pub(crate) fn two_worlds(friends: bool) -> Net {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob])]);
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    net.enter("bb", bob);
    net.say("aa", "Ann", "@dn on");
    net.say("bb", "Bob", "@dn on");
    if friends {
        add_friend(&mut net, "aa", "Ann", "Bob@bb");
        add_friend(&mut net, "bb", "Bob", "Ann@aa");
    }
    net
}

/// `@friends add <name>`: the client sends `Social_AddFriend` (`0x0018`) with the name.
pub(crate) fn add_friend(net: &mut Net, slug: &str, who: &str, name: &str) {
    let w = net.world(slug);
    let id = w.clients[who];
    w.ts.send_game_action(
        id,
        &dereth_protocol::social::SocialAddFriend {
            name: name.to_owned(),
        },
    );
    net.advance(0.3);
}
