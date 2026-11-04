//! The world's link to the Dereth Network's hub, when the operator turns federation on
//! (`--federation`, with `--hub`): one WebSocket to the hub's link for the world's slug, with its
//! key, beside the HTTPS heartbeat.
//!
//! The link itself (connecting, its keep-alive, reconnecting with growing waits, a minute after
//! the hub refuses the key or the world) runs on a thread of the network's own
//! (`dereth_network_link`). The world's half of what goes over it is `yalain_federation`'s, on the
//! world's thread: [`Network::pump`] hands it what the link received, at most
//! [`FRAMES_PER_PUMP`] frames each pass, lets its clock run, and takes what it has to send.
//!
//! The players' friends service (`yalain_friends`, the `friends` module) answers what the world's
//! half asks of who is present and shows what it delivers.
//!
//! **The directory.** The world's half decides when a listed character's profile goes; the host
//! composes it from the world ([`profile_of`]) and hands it in: at each login of a character on
//! the network, when the half asks (`@dn on`), every [`LOOK_EVERY_SECS`] while it plays, and at
//! its logout, from what the world kept as it left.
//!
//! **Verifying the world.** The operator's `dn verify <token>` (at the console, or said by an
//! admin in game) goes to the hub through [`crate::verify`]; while it waits, the game port hands
//! in the hub's challenges it heard, whose nonces go back the same way ([`Network::verification`]).
//!
//! Nothing here logs the key.

use std::collections::BTreeMap;

use dereth_network_link::{Event, HubUrl, LinkClient};
use dereth_network_protocol::address::is_network_slug;
use dereth_network_protocol::directory::{
    is_shown_text, Allegiance, AubereanVitals, Vital, MAX_ALLEGIANCE_NAME_CHARS,
};
use dereth_network_protocol::rooms::read_setting;
use dereth_network_protocol::{Address, AubereanProfile};
use yalain_federation::{Federation, Friends, Settings, DEFAULT_ROOM};
use yalain_net::ServerNet;
use yalain_store::Store;
use yalain_world::player::quality;
use yalain_world::Server;

use crate::hub::HubSettings;
use crate::verify::{Answered, Verifier};

/// How often the profiles of the listed characters in the world are looked over for a change, in
/// seconds. A change shows at most this long after its character's five minutes are up.
pub const LOOK_EVERY_SECS: f64 = 30.0;

/// How often the directory's pass looks for characters that entered or left the world, or were put
/// on the network, in seconds.
pub const PASS_EVERY_SECS: f64 = 1.0;

/// The world's record of the keys the directory knows its allegiances by, one per monarch.
pub const ALLEGIANCE_KEYS: &str = "dereth-network-allegiance-keys";

/// The most of what the link received that one pass hands the world; the rest waits for the next,
/// so a hub that sends without pause holds no pass of the world's loop for long.
pub const FRAMES_PER_PUMP: usize = 256;

/// The world's half of the network and its link.
#[derive(Debug)]
pub struct Network {
    pub federation: Federation,
    link: Option<LinkClient>,
    /// The characters in the world at the last pass, by id, with their names.
    in_world: BTreeMap<u64, String>,
    /// When the directory's pass next runs, and when it next looks the listed characters'
    /// profiles over.
    next_pass: f64,
    next_look: f64,
    /// The directory's key for each monarch's allegiance, read from the world's record once.
    keys: Option<BTreeMap<u64, String>>,
    /// The calls to the hub's verification routes; none sends nothing.
    verifier: Option<Verifier>,
}

/// The settings the world's half runs with: on, under the hub's slug, as this program, telling the
/// hub the WebSocket endpoint's URL (`websocket_url`) within the network's rule, and carrying the
/// Global room where `federation.global_room` (`global_room`) says, or in General when it says
/// nothing.
///
/// # Errors
/// The slug cannot be a network address's world half (too long, or reserved), or `global_room` names
/// no room.
pub fn settings(
    hub: &HubSettings,
    websocket_url: Option<&str>,
    global_room: Option<&str>,
) -> Result<Settings, String> {
    if !is_network_slug(&hub.slug) {
        return Err(format!(
            "--federation: the slug {} cannot join the Dereth Network (2 to 12 small letters, \
             digits and inner hyphens, not reserved)",
            hub.slug
        ));
    }
    Ok(Settings {
        enabled: true,
        slug: hub.slug.clone(),
        software: concat!("yalain ", env!("CARGO_PKG_VERSION")).to_owned(),
        websocket_url: websocket_url.and_then(told_to_the_hub),
        global_room: read_setting(global_room, DEFAULT_ROOM)?,
        global_room_named: global_room.is_some(),
    })
}

/// The WebSocket endpoint's URL as the hub is told it: only `wss://`, or `ws://` on this machine.
/// One outside that rule is left out, and the log says why.
fn told_to_the_hub(url: &str) -> Option<String> {
    let ok = dereth_network_protocol::web::is_websocket_url(url);
    if !ok {
        tracing::warn!(
            "Dereth Network: the hub is not told the WebSocket endpoint {url:?}: it must be \
             wss://, or ws:// on this machine (server.websocket.public_url names it)"
        );
    }
    ok.then(|| url.to_owned())
}

/// The hub's link address for the heartbeat's: `wss://` for `https://`, `ws://` (to this machine)
/// for `http://`.
///
/// # Errors
/// An address the link cannot use.
pub fn link_url(hub: &HubSettings) -> Result<HubUrl, String> {
    let url = if let Some(rest) = hub.url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = hub.url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        hub.url.clone()
    };
    HubUrl::parse(&url)
}

impl Network {
    /// The world's half as the store keeps it, linking to the hub.
    ///
    /// # Errors
    /// The settings or the store, or the link's thread would not start.
    pub fn start(
        hub: &HubSettings,
        websocket_url: Option<&str>,
        global_room: Option<&str>,
        unix: i64,
        store: &mut impl Store,
    ) -> Result<Self, String> {
        let url = link_url(hub)?;
        let federation = Federation::load(settings(hub, websocket_url, global_room)?, unix, store)
            .map_err(|e| format!("the network's records: {e}"))?;
        let link = LinkClient::start(&url, &hub.slug, &hub.key)
            .map_err(|e| format!("the link to the hub: {e}"))?;
        tracing::info!(
            "Dereth Network: linking to {} as {}",
            url.link(&hub.slug),
            hub.slug
        );
        let mut n = Self::with(federation, link);
        n.verifier = Some(Verifier::new(
            url.verify(&hub.slug),
            url.answer(&hub.slug),
            hub.key.clone(),
        ));
        Ok(n)
    }

    /// The world's half over a link of the caller's.
    #[must_use]
    pub fn with(federation: Federation, link: LinkClient) -> Self {
        Self {
            federation,
            link: Some(link),
            in_world: BTreeMap::new(),
            next_pass: 0.0,
            next_look: 0.0,
            keys: None,
            verifier: None,
        }
    }

    /// One pass: what the link received goes to the world's half, its clock runs to `t`
    /// (seconds), and what it has to send goes to the link.
    pub fn pump(&mut self, friends: &mut impl Friends, t: f64) {
        let Some(link) = &mut self.link else {
            return;
        };
        let f = &mut self.federation;
        for _ in 0..FRAMES_PER_PUMP {
            let Some(e) = link.try_event() else { break };
            match e {
                Event::Up => f.on_link_up(),
                Event::Frame(text) => f.on_frame(friends, t, &text),
                Event::Down(why) => {
                    tracing::info!("Dereth Network: the link went down: {why}");
                    f.on_link_down(friends, t);
                }
                Event::Unreachable { why, retry_in } => tracing::debug!(
                    "Dereth Network: the hub cannot be reached ({why}); trying again in {} s",
                    retry_in.as_secs()
                ),
                Event::Refused { status, retry_in } => tracing::warn!(
                    "Dereth Network: the hub refused the link ({}); trying again in {} s",
                    if status == 401 {
                        "it does not know the world's key"
                    } else {
                        "the world's federation is not on at the hub"
                    },
                    retry_in.as_secs()
                ),
            }
        }
        f.tick(friends, t);
        for frame in f.take_frames(t) {
            link.send(frame);
        }
    }

    /// The directory's pass, every [`PASS_EVERY_SECS`]: the characters who entered the world on
    /// the network are listed, those who left it send their profile as the world kept it if it
    /// changed, those put on the network are listed, and every [`LOOK_EVERY_SECS`] each listed
    /// character in the world hands in its profile as it is now. What the store keeps of the
    /// directory is written when it changed.
    pub fn directory<S: Store>(&mut self, s: &mut Server<S>, t: f64) {
        if !self.federation.enabled() || t < self.next_pass {
            return;
        }
        self.next_pass = t + PASS_EVERY_SECS;
        let now: BTreeMap<u64, String> = s
            .players
            .values()
            .map(|p| (p.character, p.name.clone()))
            .collect();
        let was = std::mem::replace(&mut self.in_world, now.clone());
        let keys = self.keys.get_or_insert_with(|| read_keys(&mut s.store));
        let f = &mut self.federation;
        for (id, name) in &was {
            if !now.contains_key(id) && f.listed(name) {
                if let Some(p) = profile_of(s, f, keys, *id) {
                    f.listing_left(t, name, p);
                }
            }
        }
        for (id, name) in &now {
            if !was.contains_key(id) && f.is_on(name) {
                if let Some(p) = profile_of(s, f, keys, *id) {
                    f.listing_on(t, name, p);
                }
            }
        }
        for (id, name) in f.profiles_wanted() {
            if let Some(p) = profile_of(s, f, keys, id) {
                f.listing_on(t, &name, p);
            }
        }
        if t >= self.next_look {
            self.next_look = t + LOOK_EVERY_SECS;
            for (id, name) in &now {
                if f.listed(name) {
                    if let Some(p) = profile_of(s, f, keys, *id) {
                        f.listing_changed(t, name, p);
                    }
                }
            }
        }
        if let Err(e) = f.keep(&mut s.store) {
            tracing::warn!("Dereth Network: the directory's records were not kept: {e}");
        }
    }

    /// A `dn` command typed at the console at `t` (its words after `dn`): its lines.
    pub fn console(
        &mut self,
        store: &mut impl Store,
        friends: &mut impl Friends,
        t: f64,
        words: &[&str],
    ) -> Vec<String> {
        self.federation.operator(store, friends, t, words)
    }

    /// The operator's verification, each pass at `t`: the hub's challenges the game port heard go
    /// to the world's half, the verifications typed and the answers to the challenges go to the
    /// hub, and the game port hears challenges on only while a verification waits. The hub's
    /// answers come back, each with whom to show it to (a character by id, or the console).
    pub fn verification(&mut self, net: &mut ServerNet, t: f64) -> Vec<Answered> {
        let f = &mut self.federation;
        f.heard_challenges(t, net.take_hub_challenges());
        let mut answered = Vec::new();
        for r in f.take_verify_requests() {
            match &self.verifier {
                Some(v) => v.send(r),
                None => answered.push((r.who, yalain_federation::NOT_SENT.to_owned())),
            }
        }
        for a in f.take_challenge_answers() {
            if let Some(v) = &self.verifier {
                v.answer(a);
            }
        }
        if let Some(v) = &self.verifier {
            answered.extend(std::iter::from_fn(|| v.try_answered()));
        }
        if !answered.is_empty() {
            f.verify_answered();
        }
        net.hear_hub_challenges(f.hearing_challenges(t));
        answered
    }

    /// Closes the link.
    pub fn stop(&mut self) {
        if let Some(link) = self.link.take() {
            link.stop();
        }
    }
}

/// The directory's keys for the world's allegiances, as its record keeps them: a line each, the
/// monarch's id and the key.
fn read_keys(store: &mut impl Store) -> BTreeMap<u64, String> {
    let record = store.world_record(ALLEGIANCE_KEYS).ok().flatten();
    record
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .into_iter()
        .flat_map(str::lines)
        .filter_map(|l| {
            let (id, key) = l.split_once(' ')?;
            Some((id.parse().ok()?, key.to_owned()))
        })
        .collect()
}

/// The key the directory knows `monarch`'s allegiance by: a random value the world keeps while the
/// monarch is the same, made the first time it is needed. None when the system's random source
/// fails, or the key cannot be kept, so that no key is ever made from anything else.
fn key_of(
    store: &mut impl Store,
    keys: &mut BTreeMap<u64, String>,
    monarch: u64,
) -> Option<String> {
    if let Some(k) = keys.get(&monarch) {
        return Some(k.clone());
    }
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        tracing::warn!("Dereth Network: the system's random source failed; no allegiance key made");
        return None;
    }
    let key: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    keys.insert(monarch, key.clone());
    let record: String = keys.iter().map(|(m, k)| format!("{m} {k}\n")).collect();
    if let Err(e) = store.set_world_record(ALLEGIANCE_KEYS, record.as_bytes()) {
        tracing::warn!("Dereth Network: an allegiance key was not kept: {e}");
        keys.remove(&monarch);
        return None;
    }
    Some(key)
}

/// A character's name, in the world or not.
fn name_of<S: Store>(s: &mut Server<S>, id: u64) -> Option<String> {
    match s.players.values().find(|p| p.character == id) {
        Some(p) => Some(p.name.clone()),
        None => s.store.character(id).ok().flatten().map(|c| c.name),
    }
}

/// `character`'s allegiance as the directory shows it: its key, its name when its monarch gave it
/// one, its size, the character's rank, and its patron and monarch while each is listed. None for
/// a character in no allegiance.
fn allegiance_of<S: Store>(
    s: &mut Server<S>,
    f: &Federation,
    keys: &mut BTreeMap<u64, String>,
    character: u64,
) -> Option<Allegiance> {
    yalain_world::allegiance::ensure(s);
    let a = &s.allegiances;
    if !a.member(character) {
        return None;
    }
    let monarch = a.monarch(character)?;
    let patron = a
        .nodes
        .get(&character)
        .map(|n| n.patron)
        .filter(|&p| p != 0);
    let (rank, members) = (a.rank(character), a.size(monarch));
    // The name the monarch gave it, never one made up for an unnamed allegiance.
    let name = a
        .names
        .get(&monarch)
        .filter(|n| is_shown_text(n, MAX_ALLEGIANCE_NAME_CHARS))
        .cloned();
    let key = key_of(&mut s.store, keys, monarch)?;
    let slug = f.settings().slug.clone();
    let mut named = |other: u64| {
        let n = (other != character).then(|| name_of(s, other)).flatten()?;
        f.listed(&n).then(|| Address::new(&n, &slug).ok()).flatten()
    };
    let al = Allegiance {
        key,
        name,
        members,
        rank,
        patron: patron.and_then(&mut named),
        monarch: named(monarch),
    };
    use dereth_network_protocol::directory::{MAX_MEMBERS, MAX_RANK};
    ((1..=MAX_MEMBERS).contains(&al.members) && (1..=MAX_RANK).contains(&al.rank)).then_some(al)
}

/// A character's profile for the directory, as the world has it now: from the character in the
/// world, or, for one that left, from what the world kept as it left. None for one the world has
/// nothing of. It shows no title: the titles the game shows are its own text, which the world does
/// not hold.
pub fn profile_of<S: Store>(
    s: &mut Server<S>,
    f: &Federation,
    keys: &mut BTreeMap<u64, String>,
    character: u64,
) -> Option<AubereanProfile> {
    let (race, sex, qualities) = match s.players.values().find(|p| p.character == character) {
        Some(p) => (p.race, p.sex, p.qualities.clone()),
        None => {
            let c = s.store.character(character).ok().flatten()?;
            let q = yalain_world::saves::state_of(s, character)
                .ok()
                .flatten()?
                .qualities;
            (c.race, c.sex, q)
        }
    };
    let level = u32::try_from(yalain_world::examine::level_of(&qualities)).ok()?;
    let int = |k: u32| yalain_world::quals::int(&qualities, k).and_then(|v| u32::try_from(v).ok());
    let vitals = match (int(quality::HEALTH_MAX), int(quality::VIGOR_MAX)) {
        (Some(health), Some(vigor)) => Some(AubereanVitals {
            health: Vital { base: health },
            vigor: Vital { base: vigor },
        }),
        _ => None,
    };
    Some(AubereanProfile {
        level,
        race,
        sex,
        title: None,
        vitals,
        allegiance: allegiance_of(s, f, keys, character),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the host's plumbing between the link, the world and the world's half)
    //!
    //! The rules are the world half's own tests', and the executable's link a live station. What
    //! the host composes of a character for the directory is the network's own field list
    //! (`docs/network/02-link-protocol.md` section 7).
    use super::*;

    /// The hub is told the WebSocket endpoint's URL only within the network's rule, Empyrean's:
    /// `wss://` to anywhere, `ws://` to this machine; one outside it is left out.
    /// Behaviour: server.network.websocket-url-told-to-the-hub
    #[test]
    fn the_hub_is_told_a_websocket_url_only_within_the_networks_rule() {
        let hub = HubSettings {
            url: "https://hub.example.org".into(),
            slug: "dawnsong".into(),
            key: String::new(),
        };
        let told = |url: Option<&str>| settings(&hub, url, None).expect("settings").websocket_url;
        assert_eq!(told(None), None);
        for url in ["wss://play.example.org/ws", "ws://127.0.0.1:9180/"] {
            assert_eq!(told(Some(url)).as_deref(), Some(url));
        }
        for url in ["ws://play.example.org/ws", "https://play.example.org/ws"] {
            assert_eq!(told(Some(url)), None, "{url}");
        }
    }
    use dereth_network_protocol::limits::Limits;
    use dereth_network_protocol::wire::PROTOCOL;
    use dereth_network_protocol::{Address, Message, Outcome};
    use yalain_federation::{Present, TellEnd, TellId};
    use yalain_store::MemoryStore;

    /// A friends service with nobody present, for the directory's host tests.
    #[derive(Debug, Default)]
    struct NoFriendsService;

    impl Friends for NoFriendsService {
        fn present(&self, _: &str) -> Option<Present> {
            None
        }

        fn everyone_present(&self) -> Vec<String> {
            Vec::new()
        }

        fn remote_friends(&self, _: &str) -> Vec<Address> {
            Vec::new()
        }

        fn ignores(&self, _: &str, _: &Address) -> bool {
            false
        }

        fn ignores_here(&self, _: &str, _: &str) -> bool {
            false
        }

        fn show_tell(&mut self, _: &str, _: &Address, _: &str) {}

        fn show_presence(&mut self, _: &str, _: &Address, _: bool, _: bool) {}

        fn tell_ended(&mut self, _: TellId, _: TellEnd) {}

        fn in_world(&self, _name: &str) -> bool {
            false
        }

        fn show_line(&mut self, _character: u64, _line: &str) {}

        fn kept_count(&self, _: &str) -> usize {
            0
        }

        fn release_friend(&mut self, _: &str, _: &Address) {}

        fn keep_friend(&mut self, _: &str, _: &Address) {}

        fn befriend_answered(&mut self, _: &str, _: &Address, _: bool) -> bool {
            false
        }

        fn person_view_came(&mut self, _: &str) {}
    }

    #[test]
    fn the_pump_says_hello_on_a_new_link_and_answers_the_hubs_frames_on_it() {
        let hub = HubSettings {
            url: "http://127.0.0.1:9".into(),
            slug: "shardfall".into(),
            key: "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        };
        assert_eq!(
            link_url(&hub).expect("a link address").link("shardfall"),
            "ws://127.0.0.1:9/v1/servers/shardfall/link"
        );
        let mut store = MemoryStore::default();
        let federation =
            Federation::load(settings(&hub, None, None).expect("settings"), 1, &mut store)
                .expect("loaded");
        let (link, mut far) = LinkClient::detached();
        let mut n = Network::with(federation, link);
        let mut friends =
            yalain_friends::Service::new(yalain_friends::Config::new("Shardfall", "shardfall"));
        far.hand(Event::Up);
        n.pump(&mut friends, 1.0);
        let hello = far.sent();
        assert!(
            matches!(Message::from_json(&hello[0]), Ok(Message::Hello { ref software, .. }) if software.starts_with("yalain ")),
            "{hello:?}"
        );
        let welcome = Message::Welcome {
            proto: PROTOCOL,
            world: "shardfall".into(),
            hub: "test".into(),
            worlds: Vec::new(),
            rooms: Vec::new(),
            limits: Limits::DEFAULT,
        };
        far.hand(Event::Frame(welcome.to_json()));
        far.hand(Event::Frame(
            Message::Tell {
                id: "x1".into(),
                from: Address::parse("Bob@coldeve").expect("an address"),
                to: Address::parse("Ann@shardfall").expect("an address"),
                text: "hi".into(),
                friend: false,
            }
            .to_json(),
        ));
        n.pump(&mut friends, 2.0);
        let sent: Vec<Message> = far
            .sent()
            .iter()
            .map(|f| Message::from_json(f).expect("a message"))
            .collect();
        assert_eq!(
            sent,
            vec![Message::TellResult {
                id: "x1".into(),
                outcome: Outcome::Offline,
                message: None,
            }]
        );
        assert_eq!(
            n.console(&mut store, &mut friends, 2.0, &["block", "coldeve"]),
            vec!["coldeve is blocked for everyone on this world.".to_owned()]
        );
        assert_eq!(store.network_blocks().expect("read"), vec!["coldeve"]);
        far.hand(Event::Down("gone".into()));
        n.pump(&mut friends, 3.0);
        assert_eq!(n.federation.link(), yalain_federation::LinkState::Down);
        let long = HubSettings {
            slug: "a-slug-too-long".into(),
            ..hub
        };
        assert!(settings(&long, None, None).is_err());
    }

    /// One request a stand-in hub took, and the connection to answer it on.
    struct Taken {
        line: String,
        authorization: String,
        body: serde_json::Value,
        stream: std::net::TcpStream,
    }

    impl Taken {
        fn answer(mut self, status: &str, body: &str) {
            use std::io::Write as _;
            let _ = write!(
                self.stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
        }
    }

    /// The next request the world sends the stand-in hub, unanswered. The wait only guards
    /// against a world that never sends.
    fn take(listener: &std::net::TcpListener) -> Taken {
        use std::io::{BufRead as _, BufReader, Read as _};
        let listener = listener.try_clone().expect("the listener");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("the world calls");
            let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("the request line");
            let (mut authorization, mut length) = (String::new(), 0);
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).expect("a header");
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(':').expect("a header line");
                match k.trim().to_ascii_lowercase().as_str() {
                    "authorization" => authorization = v.trim().to_owned(),
                    "content-length" => length = v.trim().parse().expect("a length"),
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).expect("the body");
            let _ = tx.send(Taken {
                line: line.trim_end().to_owned(),
                authorization,
                body: serde_json::from_slice(&body).expect("JSON"),
                stream,
            });
        });
        rx.recv_timeout(std::time::Duration::from_secs(120))
            .expect("the world calls the hub")
    }

    /// `dn verify` at the console goes to the hub's verification route with the world's key, the
    /// token and who typed it; while it waits, the game port keeps the hub's challenge for this
    /// world, whose nonce goes to the answer route with the key, and nothing goes back over UDP;
    /// the hub's answer is one line for the console, after which the game port reads challenges
    /// no more.
    /// Behaviour: server.network.verify-challenge-answered-once
    #[test]
    fn dn_verify_posts_the_token_and_the_challenges_nonce_to_the_hub_with_the_worlds_key() {
        use dereth_network_protocol::challenge::{nonce_hex, Challenge, NONCE_LEN};
        use yalain_net::{LocalTime, NetConfig, Port};
        const KEY: &str = "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        const TOKEN: &str = "dnv_abcdefghijklmnopqrstuvwxyzAB-_01";
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a stand-in hub");
        let port = listener.local_addr().expect("its port").port();
        let hub = HubSettings {
            url: format!("http://127.0.0.1:{port}"),
            slug: "shardfall".into(),
            key: KEY.into(),
        };
        let url = link_url(&hub).expect("a link address");
        let mut store = MemoryStore::default();
        let federation =
            Federation::load(settings(&hub, None, None).expect("settings"), 1, &mut store)
                .expect("loaded");
        let (link, _far) = LinkClient::detached();
        let mut n = Network::with(federation, link);
        n.verifier = Some(Verifier::new(
            url.verify("shardfall"),
            url.answer("shardfall"),
            KEY.into(),
        ));
        let mut net = ServerNet::new(
            NetConfig::retail_ids("127.0.0.1:9002".parse().expect("an address")),
            yalain_net::fixed_stream(7),
            0.0,
            LocalTime(1.0),
        );
        let from_hub: std::net::SocketAddr = "203.0.113.9:40000".parse().expect("an address");
        let challenge = |slug: &str, nonce: u8| {
            Challenge {
                slug: slug.into(),
                nonce: [nonce; NONCE_LEN],
            }
            .encode()
            .expect("a slug it carries")
        };

        assert_eq!(
            n.console(&mut store, &mut NoFriendsService, 10.0, &["verify", TOKEN]),
            [yalain_federation::ASKING]
        );
        assert!(n.verification(&mut net, 10.0).is_empty());
        let verify = take(&listener);
        assert_eq!(verify.line, "POST /v1/servers/shardfall/verify HTTP/1.1");
        assert_eq!(verify.authorization, format!("Bearer {KEY}"));
        assert_eq!(
            verify.body,
            serde_json::json!({ "token": TOKEN, "by": "the console" })
        );
        for d in [challenge("coldeve", 1), challenge("shardfall", 2)] {
            net.on_datagram(Port::LOGIN_EVEN, from_hub, &d, LocalTime(11.0));
        }
        assert!(n.verification(&mut net, 11.0).is_empty());
        let answer = take(&listener);
        assert_eq!(
            answer.line,
            "POST /v1/servers/shardfall/verify/answer HTTP/1.1"
        );
        assert_eq!(answer.authorization, format!("Bearer {KEY}"));
        assert_eq!(
            answer.body,
            serde_json::json!({ "nonce": nonce_hex(&[2; NONCE_LEN]) })
        );
        answer.answer("200 OK", r#"{"answered":true}"#);
        net.tick(LocalTime(11.0));
        assert!(net.take_datagrams().is_empty(), "nothing over UDP");
        verify.answer(
            "200 OK",
            r#"{"verified":true,"federation":"on","approval":"approved"}"#,
        );
        assert!(n
            .verifier
            .as_ref()
            .expect("the verifier")
            .wait_for_answer(std::time::Duration::from_secs(120)));
        assert_eq!(
            n.verification(&mut net, 12.0),
            [(
                None,
                "The Dereth Network verified this world. It is on the network now; the link \
                 comes up within a minute."
                    .to_owned()
            )]
        );
        net.on_datagram(
            Port::LOGIN_EVEN,
            from_hub,
            &challenge("shardfall", 3),
            LocalTime(12.0),
        );
        assert!(net.take_hub_challenges().is_empty(), "no longer read");
        assert_eq!(
            n.console(&mut store, &mut NoFriendsService, 13.0, &["verify", TOKEN]),
            [yalain_federation::ASKING],
            "the next may go"
        );
    }

    /// The world's half on, linked over a detached link and welcomed; its far end.
    fn linked(store: &mut MemoryStore) -> (Network, dereth_network_link::Detached) {
        let hub = HubSettings {
            url: "http://127.0.0.1:9".into(),
            slug: "shardfall".into(),
            key: "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        };
        let federation = Federation::load(settings(&hub, None, None).expect("settings"), 1, store)
            .expect("loaded");
        let (link, mut far) = LinkClient::detached();
        let mut n = Network::with(federation, link);
        far.hand(Event::Up);
        n.pump(&mut NoFriendsService, 1.0);
        far.sent();
        let welcome = Message::Welcome {
            proto: PROTOCOL,
            world: "shardfall".into(),
            hub: "test".into(),
            worlds: Vec::new(),
            rooms: Vec::new(),
            limits: Limits::DEFAULT,
        };
        far.hand(Event::Frame(welcome.to_json()));
        n.pump(&mut NoFriendsService, 1.0);
        (n, far)
    }

    /// The listings the far end was sent, read as the hub reads an Asheron's Call 2 world's.
    fn listings(
        far: &mut dereth_network_link::Detached,
    ) -> Vec<(String, bool, Option<AubereanProfile>)> {
        far.sent()
            .iter()
            .filter_map(|f| {
                dereth_network_protocol::DirectoryMessage::from_json_for(
                    dereth_network_protocol::Game::Auberean,
                    f,
                )
                .ok()
            })
            .filter_map(|m| match m {
                dereth_network_protocol::DirectoryMessage::Listing {
                    who, on, profile, ..
                } => Some((
                    who.to_string(),
                    on,
                    profile.map(|p| match *p {
                        dereth_network_protocol::ListingProfile::Auberean(p) => p,
                        dereth_network_protocol::ListingProfile::Dereth(_) => {
                            panic!("an Asheron's Call profile")
                        }
                    }),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_character_on_the_network_is_listed_as_it_enters_and_as_the_world_kept_it_when_it_leaves() {
        let mut h = yalain_testkit::Harness::new();
        let (b, id) = h.player_in_world("one", "Elara");
        let (_, cara) = h.player_in_world("two", "Cara");
        let (mut n, mut far) = linked(&mut h.server.store);
        // On the network before it entered: the friends service sets it, the world's pass finds it.
        n.federation
            .set_on(&mut h.server.store, &NoFriendsService, id, "Elara", true)
            .expect("kept");
        n.directory(&mut h.server, 2.0);
        n.pump(&mut NoFriendsService, 2.0);
        let got = listings(&mut far);
        assert_eq!(got.len(), 1, "{got:?}");
        let (who, on, p) = &got[0];
        let p = p.as_ref().expect("a profile");
        assert_eq!((who.as_str(), *on), ("Elara@shardfall", true));
        assert_eq!((p.race, p.sex, p.level), (1, 0x1000, 1));
        assert!(p.vitals.is_some_and(|v| v.health.base > 0), "{p:?}");
        assert_eq!(p.title, None);
        assert_eq!(p.allegiance, None);
        // Sworn to Cara: the patron is named only while it is listed itself.
        yalain_world::allegiance::ensure(&mut h.server);
        let a = &mut h.server.allegiances;
        a.nodes
            .insert(cara, yalain_world::allegiance::Node::default());
        a.nodes.insert(
            id,
            yalain_world::allegiance::Node {
                patron: cara,
                ..Default::default()
            },
        );
        let mut keys = BTreeMap::new();
        let mine = profile_of(&mut h.server, &n.federation, &mut keys, id).expect("composed");
        let al = mine.allegiance.expect("sworn");
        assert_eq!(
            (al.members, al.rank, al.patron, al.monarch),
            (2, 1, None, None)
        );
        assert_eq!(al.key.len(), 32);
        n.federation
            .set_on(&mut h.server.store, &NoFriendsService, cara, "Cara", true)
            .expect("kept");
        let mine = profile_of(&mut h.server, &n.federation, &mut keys, id).expect("composed");
        let al2 = mine.allegiance.expect("sworn");
        assert_eq!(al2.key, al.key, "one key while its monarch is the same");
        assert_eq!(
            al2.patron.as_ref().map(ToString::to_string).as_deref(),
            Some("Cara@shardfall")
        );
        assert_eq!(al2.monarch, al2.patron);
        assert_eq!(
            read_keys(&mut h.server.store).get(&cara),
            Some(&al.key),
            "kept"
        );
        // It leaves a level higher: what the world kept as it left goes at once.
        let world = h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == id)
            .map(|(w, _)| *w)
            .expect("in the world");
        let p = h.server.players.get_mut(&world).expect("in the world");
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, 7);
        h.bots[b].leave_world();
        let mut until = h.now;
        until.0 += 40.0;
        h.run_to(until);
        assert!(
            !h.server.players.values().any(|p| p.character == id),
            "out of the world"
        );
        n.directory(&mut h.server, 40.0);
        n.pump(&mut NoFriendsService, 40.0);
        let got = listings(&mut far);
        let levels: Vec<u32> = got
            .iter()
            .filter(|(w, ..)| w == "Elara@shardfall")
            .filter_map(|(_, _, p)| p.as_ref().map(|p| p.level))
            .collect();
        assert_eq!(levels, [7], "{got:?}");
        // One on the network before it entered is listed as it enters.
        let d = h.at_character_list("three");
        let dana = h.create_character(d, "three", "Dana");
        h.server.store.set_network_on(dana, true).expect("kept");
        let (mut n, mut far) = linked(&mut h.server.store);
        n.directory(&mut h.server, 50.0);
        h.enter_world(d, "three", dana);
        n.directory(&mut h.server, 51.0);
        n.pump(&mut NoFriendsService, 51.0);
        let got = listings(&mut far);
        assert!(
            got.iter().any(|(w, on, _)| w == "Dana@shardfall" && *on),
            "{got:?}"
        );
    }
}
