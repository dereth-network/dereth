//! Divergence: V447
//! The Dereth Network end to end on loopback: the hub and three `empyrean-server` processes on the
//! real content (two federated worlds, `aa` and `bb`, and `cc`, whose federation flag is off),
//! with a soak bot on each federated world speaking real UDP. It plays the conversation of a
//! cross-world tell and its replies by name and by id, friends coming online and going offline,
//! a Global room line, a channel, a squelch, a world that is not on the network and a hub that
//! goes away, and writes what each client was shown (by retail message name and opcode) and what
//! each link carried (the hub's wire log, message types only).

mod run {
    //! Divergence: V447
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::io::{BufRead, BufReader};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use dereth_primitives::{IncomingMessage, NetQueue, ObjectId};
    use dereth_protocol::comms::{
        CommunicationHearDirectSpeech, CommunicationModifyCharacterSquelch, CommunicationTalk,
        CommunicationTalkDirect, CommunicationTalkDirectByName, CommunicationTextboxString,
        CommunicationWeenieError,
    };
    use dereth_protocol::social::{SocialAddFriend, SocialFriendsUpdate};
    use dereth_protocol::turbine::{decode_incoming, IncomingPayload, SendToRoomById};
    use empyrean_testkit::soak::bot::{self, Bot, BotIo, Kind, State};
    use empyrean_testkit::soak::udp::default_server_bin;
    use empyrean_testkit::{ClientStatus, TestClient};

    static LOGS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn free_udp_pair(from: u16) -> u16 {
        let mut p = from;
        loop {
            let lo = UdpSocket::bind((Ipv4Addr::LOCALHOST, p));
            let hi = UdpSocket::bind((Ipv4Addr::LOCALHOST, p + 1));
            if lo.is_ok() && hi.is_ok() {
                return p;
            }
            p += 2;
            assert!(p < 64_000, "no free port pair");
        }
    }

    fn free_tcp() -> u16 {
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .expect("a free port")
    }

    fn hub_bin() -> PathBuf {
        if let Some(p) = std::env::var_os("DERETH_NETWORK_HUB_BIN") {
            return PathBuf::from(p);
        }
        default_server_bin().with_file_name(format!(
            "dereth-network-hub{}",
            std::env::consts::EXE_SUFFIX
        ))
    }

    fn hub_cmd(dir: &Path, args: &[&str]) -> String {
        let out = Command::new(hub_bin())
            .arg("--db")
            .arg(dir.join("hub.sqlite"))
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", hub_bin().display()));
        assert!(
            out.status.success(),
            "hub {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Registers `slug` with the hub and returns its key.
    fn register(dir: &Path, slug: &str, port: u16, flag: &str) -> String {
        let file = dir.join(format!("{slug}.json"));
        std::fs::write(
            &file,
            serde_json::json!({
                "slug": slug, "name": format!("World {}", slug.to_uppercase()),
                "description": "A loopback test world.", "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "127.0.0.1", "port": port },
                "emulator": { "family": "Empyrean" },
            })
            .to_string(),
        )
        .expect("a registration");
        let printed: serde_json::Value =
            serde_json::from_str(&hub_cmd(dir, &["register", &file.to_string_lossy()]))
                .expect("JSON");
        hub_cmd(dir, &["federation", slug, flag]);
        printed["key"].as_str().expect("a key").to_owned()
    }

    struct Process {
        child: Child,
        log: Arc<Mutex<Vec<String>>>,
    }

    /// The run's folder (the worlds' databases and keys, the hub's registry, the logs): removed when
    /// the run ends, after every process has stopped, unless `NETWORK_E2E_DIR` named it.
    struct RunDir {
        path: PathBuf,
        keep: bool,
    }

    impl Drop for RunDir {
        fn drop(&mut self) {
            if !self.keep {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    /// A process the run started is stopped however the run ends, a failed assertion included.
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Process {
        fn saw(&self, needle: &str) -> bool {
            self.log
                .lock()
                .map(|l| l.iter().any(|x| x.contains(needle)))
                .unwrap_or(false)
        }
        fn count(&self, needle: &str) -> usize {
            self.log
                .lock()
                .map(|l| l.iter().filter(|x| x.contains(needle)).count())
                .unwrap_or(0)
        }
    }

    fn spawn(mut cmd: Command, log_file: PathBuf) -> Process {
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("a process");
        let stderr = child.stderr.take().expect("stderr");
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = Arc::clone(&log);
        std::thread::spawn(move || {
            let mut file = std::fs::File::create(log_file).ok();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(f) = file.as_mut() {
                    use std::io::Write as _;
                    let _ = writeln!(f, "{line}");
                }
                if let Ok(mut l) = l.lock() {
                    l.push(line);
                }
            }
        });
        Process { child, log }
    }

    fn start_hub(dir: &Path, port: u16) -> Process {
        let mut cmd = Command::new(hub_bin());
        cmd.arg("--db")
            .arg(dir.join("hub.sqlite"))
            .args([
                "serve",
                "--listen",
                &format!("127.0.0.1:{port}"),
                "--wire-log",
            ])
            .arg(dir.join("wire.log"));
        let n = LOGS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = spawn(cmd, dir.join(format!("hub-{n}.log")));
        let t = Instant::now();
        while !p.saw("listening on") {
            assert!(
                t.elapsed() < Duration::from_secs(20),
                "the hub never listened"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        p
    }

    fn start_world(dir: &Path, slug: &str, port: u16, hub_port: u16, key: &str) -> Process {
        let wdir = dir.join(slug);
        std::fs::create_dir_all(&wdir).expect("a world folder");
        std::fs::write(wdir.join("network.key"), key).expect("the key file");
        let mut c = empyrean_common::master_configuration::MasterConfiguration::default();
        c.server.world_name = format!("World {}", slug.to_uppercase());
        c.server.network.host = "127.0.0.1".to_owned();
        c.server.network.port = u32::from(port);
        c.server.dat_files_directory = dereth_dat::testing::dat_dir()
            .to_string_lossy()
            .into_owned();
        c.server.world_pack_path = empyrean_common::test_paths::world_pack()
            .to_string_lossy()
            .into_owned();
        c.server.interactive_console = false;
        c.database.shard_db_path = wdir.join("shard.db").to_string_lossy().into_owned();
        c.database.auth_db_path = wdir.join("auth.db").to_string_lossy().into_owned();
        c.federation.enabled = true;
        c.federation.hub_url = format!("ws://127.0.0.1:{hub_port}");
        c.federation.slug = slug.to_owned();
        c.federation.key_file = "network.key".to_owned();
        // New characters listen to General but not Roleplay, so the run carries the Global room
        // in General; a world would usually pick a room it does not otherwise use.
        c.federation.global_room = Some("general".to_owned());
        let config = wdir.join("empyrean.toml");
        std::fs::write(&config, empyrean_common::toml_config::to_toml_string(&c))
            .expect("empyrean.toml");
        let mut cmd = Command::new(default_server_bin());
        cmd.args(["--config", &config.to_string_lossy(), "--run-for", "900"])
            .current_dir(&wdir);
        let p = spawn(cmd, wdir.join("server.log"));
        let t = Instant::now();
        while p.count("Listening on") < 2 {
            assert!(
                t.elapsed() < Duration::from_secs(120),
                "{slug} never listened"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        p
    }

    struct Socket {
        socket: Arc<UdpSocket>,
        rx: Receiver<(SocketAddr, Vec<u8>)>,
    }

    impl Socket {
        fn bind() -> Self {
            let socket = Arc::new(UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a socket"));
            let (tx, rx) = mpsc::channel();
            let s = Arc::clone(&socket);
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                // A receive error is not the end: on Windows a datagram that reached a closed port
                // fails the next receive with a reset, and the socket goes on working.
                loop {
                    if let Ok((n, from)) = s.recv_from(&mut buf) {
                        if tx.send((from, buf[..n].to_vec())).is_err() {
                            return;
                        }
                    } else if Arc::strong_count(&s) == 1 {
                        return;
                    } else {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            });
            Self { socket, rx }
        }
    }

    /// One player: a bot over a real socket, and what it was shown.
    struct Player {
        name: &'static str,
        world: &'static str,
        bot: Bot,
        io: BotIo,
        socket: Socket,
        client: TestClient,
        inbox: Vec<IncomingMessage>,
        /// Every message received, by retail name and opcode.
        wire: Vec<String>,
        /// The chat-relevant ones, as text.
        shown: Vec<String>,
        connected: bool,
    }

    impl Player {
        fn new(i: usize, world: &'static str, name: &'static str, port: u16) -> Self {
            let socket = Socket::bind();
            let addr = socket.socket.local_addr().expect("an address");
            let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
            let account = format!("net{}", name.to_ascii_lowercase());
            let client = TestClient::new(addr, server, &account, "pw");
            let mut bot = Bot::new(i, account, name.to_owned(), 0, 7 + i as u64);
            bot.only = Some(Kind::Idle);
            bot.allow_fight = false;
            bot.allow_relog = false;
            Self {
                name,
                world,
                bot,
                io: BotIo::default(),
                socket,
                client,
                inbox: Vec::new(),
                wire: Vec::new(),
                shown: Vec::new(),
                connected: false,
            }
        }

        fn action<M: dereth_protocol::Message>(&mut self, m: &M, said: &str) {
            self.shown.push(format!("> {said}"));
            self.io.action(m);
        }
    }

    fn describe(p: &mut Player, m: &IncomingMessage) {
        let d = empyrean_testkit::decode::decode(m);
        let kind = d.kind;
        let label = format!("{} 0x{kind:04X}", d.name());
        p.wire.push(label.clone());
        let line = match kind {
            0x02BD => bot::read::<CommunicationHearDirectSpeech>(m).map(|t| {
                format!(
                    "{} tells you, \"{}\"   [{label}, sender id 0x{:08X}]",
                    t.sender_name, t.message, t.sender_id.0
                )
            }),
            0xF7E0 => bot::read::<CommunicationTextboxString>(m)
                .map(|t| format!("{}   [{label}]", t.text.trim_end())),
            0x028A => bot::read::<CommunicationWeenieError>(m).map(|e| {
                let what = if e.error_type == 0x052B {
                    "Character not available"
                } else {
                    "an error"
                };
                format!("{what} (0x{:04X})   [{label}]", e.error_type)
            }),
            0x0021 => bot::read::<SocialFriendsUpdate>(m).map(|u| {
                let rows: Vec<String> = u
                    .friends
                    .iter()
                    .map(|f| {
                        format!(
                            "{} (0x{:08X}) {}",
                            f.name,
                            f.id.0,
                            if f.online != 0 { "online" } else { "offline" }
                        )
                    })
                    .collect();
                format!(
                    "friends {} [{}]   [{label}]",
                    ["full list", "added", "removed", "removed", "status"]
                        [usize::try_from(u.update_type.min(4)).unwrap_or(0)],
                    rows.join(", ")
                )
            }),
            0xF7DE => decode_incoming(&m.body).ok().and_then(|p| match p.payload {
                IncomingPayload::RoomEvent(e) => {
                    let id = e
                        .extra
                        .get(0..4)
                        .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap_or([0; 4])));
                    Some(format!(
                        "[room {}] {} says, \"{}\"   [{label}, sender id 0x{id:08X}]",
                        e.room, e.name, e.text
                    ))
                }
                _ => None,
            }),
            _ => None,
        };
        if let Some(line) = line {
            p.shown.push(line);
        }
    }

    struct Run {
        players: Vec<Player>,
        start: Instant,
        next_step: f64,
    }

    impl Run {
        fn t(&self) -> f64 {
            self.start.elapsed().as_secs_f64()
        }

        fn pump(&mut self) {
            let t = self.t();
            for p in &mut self.players {
                p.client.tick(t);
                for (to, bytes) in p.client.take_outgoing() {
                    let _ = p.socket.socket.send_to(&bytes, to);
                }
                while let Ok((from, bytes)) = p.socket.rx.try_recv() {
                    p.client.handle_datagram(from, &bytes, t);
                }
                while let Some(m) = p.client.poll() {
                    p.inbox.push(m);
                }
            }
            if t >= self.next_step {
                self.next_step = t + 0.1;
                for p in &mut self.players {
                    if !p.connected && p.client.status() == ClientStatus::Connected {
                        p.connected = true;
                        p.bot.connected(&mut p.io);
                    }
                    let inbox = std::mem::take(&mut p.inbox);
                    for m in &inbox {
                        describe(p, m);
                    }
                    p.bot.receive(t, &inbox);
                    p.bot.update(t, &mut p.io);
                    for (q, bytes) in p.io.out.drain(..) {
                        p.client.send(q, &bytes);
                    }
                    p.bot.events.clear();
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        fn wait(&mut self, secs: f64) {
            let until = self.t() + secs;
            while self.t() < until {
                self.pump();
            }
        }

        fn until(&mut self, secs: f64, what: &str, mut f: impl FnMut(&Run) -> bool) {
            let until = self.t() + secs;
            while !f(self) {
                assert!(self.t() < until, "timed out waiting for {what}");
                self.pump();
            }
            self.wait(0.3);
        }

        fn p(&mut self, i: usize) -> &mut Player {
            &mut self.players[i]
        }

        fn shown_since(&self, i: usize, mark: usize, needle: &str) -> bool {
            self.players[i].shown[mark..]
                .iter()
                .any(|l| l.contains(needle))
        }
    }

    fn talk(p: &mut Player, line: &str) {
        p.action(
            &CommunicationTalk {
                message: line.to_owned(),
            },
            line,
        );
    }

    fn tell(p: &mut Player, to: &str, text: &str) {
        p.action(
            &CommunicationTalkDirectByName {
                message: text.to_owned(),
                target_name: to.to_owned(),
            },
            &format!("@tell {to}, {text}   [Communication_TalkDirectByName 0x005D]"),
        );
    }

    fn stand_in_of(p: &Player, sender: &str) -> u32 {
        p.shown
            .iter()
            .rev()
            .find_map(|l| {
                let rest = l.strip_prefix(&format!("{sender} tells you, "))?;
                let hex = rest.split("sender id 0x").nth(1)?.trim_end_matches(']');
                u32::from_str_radix(hex, 16).ok()
            })
            .unwrap_or_else(|| panic!("{} was never told by {sender}", p.name))
    }

    #[test]
    #[ignore = "instrument: runs the hub and three empyrean-server processes on loopback and writes the transcript; build dereth-network-hub and empyrean-server first"]
    #[allow(clippy::too_many_lines)]
    fn network_two_worlds() {
        let dir = std::env::var_os("NETWORK_E2E_DIR").map_or_else(
            || std::env::temp_dir().join(format!("dereth-network-e2e-{}", std::process::id())),
            PathBuf::from,
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the run folder");
        // Declared before the processes, so it is dropped after them.
        let run_dir = RunDir {
            path: dir.clone(),
            keep: std::env::var_os("NETWORK_E2E_DIR").is_some(),
        };
        let hub_port = free_tcp();
        let (pa, pb, pc) = (
            free_udp_pair(41_000),
            free_udp_pair(42_000),
            free_udp_pair(43_000),
        );
        let key_a = register(&dir, "aa", pa, "on");
        let key_b = register(&dir, "bb", pb, "on");
        let key_c = register(&dir, "cc", pc, "off");
        let mut hub = start_hub(&dir, hub_port);
        let mut worlds = BTreeMap::new();
        for (slug, port, key) in [("aa", pa, &key_a), ("bb", pb, &key_b), ("cc", pc, &key_c)] {
            worlds.insert(slug, start_world(&dir, slug, port, hub_port, key));
        }
        let t0 = Instant::now();
        while !(worlds["aa"].saw("Dereth Network: welcomed")
            && worlds["bb"].saw("Dereth Network: welcomed"))
        {
            assert!(
                t0.elapsed() < Duration::from_secs(60),
                "the worlds never linked"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        while !worlds["cc"].saw("the hub refused this world (403") {
            assert!(
                t0.elapsed() < Duration::from_secs(60),
                "cc was never refused"
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        let mut run = Run {
            players: vec![
                Player::new(0, "aa", "Ann", pa),
                Player::new(1, "bb", "Bob", pb),
            ],
            start: Instant::now(),
            next_step: 0.0,
        };
        let (ann, bob) = (0, 1);
        run.until(120.0, "both in the world", |r| {
            r.players.iter().all(|p| p.bot.state == State::InWorld)
        });
        let mut notes = Vec::new();
        notes.push(format!("- worlds aa (udp {pa}), bb (udp {pb}) and cc (udp {pc}, federation off); the hub on 127.0.0.1:{hub_port}"));

        // On the network, and friends.
        talk(run.p(ann), "@dn on");
        talk(run.p(bob), "@dn on");
        run.until(10.0, "opted in", |r| {
            r.shown_since(ann, 0, "You are on the Dereth Network as Ann@aa")
                && r.shown_since(bob, 0, "as Bob@bb")
        });
        let m = run.players[ann].shown.len();
        run.p(ann).action(
            &SocialAddFriend {
                name: "Bob@bb".into(),
            },
            "@friends add Bob@bb   [Social_AddFriend 0x0018]",
        );
        run.until(10.0, "Bob added", |r| {
            r.shown_since(ann, m, "Bob@bb has been added")
        });
        run.p(bob).action(
            &SocialAddFriend {
                name: "Ann@aa".into(),
            },
            "@friends add Ann@aa   [Social_AddFriend 0x0018]",
        );
        // Ann's world knew Bob offline, so his coming into view is news to her: the line. Bob's
        // world never knew Ann's state, so to him it is the first report: the row alone.
        run.until(10.0, "each sees the other online", |r| {
            r.shown_since(ann, m, "Bob@bb<\\Tell> has come online")
                && r.players[bob]
                    .shown
                    .iter()
                    .any(|l| l.contains("friends status [Ann@aa (0x") && l.contains("online]"))
        });

        // The conversation.
        let (ma, mb) = (run.players[ann].shown.len(), run.players[bob].shown.len());
        tell(run.p(ann), "Bob@bb", "long time! still playing?");
        run.until(10.0, "Bob hears Ann", |r| {
            r.shown_since(bob, mb, "Ann@aa tells you, \"long time! still playing?\"")
        });
        assert!(run.shown_since(ann, ma, "You tell Bob@bb, \"long time! still playing?\""));
        // `@r` expands in the client to `@tell Ann@aa, ...`: by name.
        tell(run.p(bob), "Ann@aa", "yes! come say hi Sat");
        run.until(10.0, "Ann hears Bob", |r| {
            r.shown_since(ann, ma, "Bob@bb tells you, \"yes! come say hi Sat\"")
        });
        // A reply by id, to the stand-in id the client stored.
        let bob_id = stand_in_of(&run.players[ann], "Bob@bb");
        run.p(ann).action(
            &CommunicationTalkDirect {
                message: "see you then".into(),
                target: ObjectId(bob_id),
            },
            &format!(
                "@reply see you then   [Communication_TalkDirect 0x0032, target 0x{bob_id:08X}]"
            ),
        );
        run.until(10.0, "Bob hears the reply by id", |r| {
            r.shown_since(bob, mb, "Ann@aa tells you, \"see you then\"")
        });

        // The Global room.
        let mb = run.players[bob].shown.len();
        let ann_guid = run.players[ann].bot.me.expect("Ann's id");
        {
            // A room line is a message on the login queue, not a game action.
            let p = run.p(ann);
            let m = SendToRoomById {
                context: 1,
                room: 2,
                text: "Ballroom dance at the Arwic hall, 8 pm!".into(),
                sender: ann_guid,
                chat_type: 2,
            };
            p.shown.push("> [General] Ballroom dance at the Arwic hall, 8 pm!   [Communication_TurbineChat 0xF7DE]".into());
            p.io.out.push((
                NetQueue::Logon,
                dereth_protocol::write_blob(&m).expect("a room line"),
            ));
        }
        run.until(10.0, "Bob hears the room line", |r| {
            r.shown_since(bob, mb, "Ann@aa says, \"Ballroom dance")
        });

        // A channel.
        talk(run.p(ann), "@dn channel create oldguard");
        run.wait(1.0);
        talk(run.p(ann), "@dn channel invite oldguard Bob@bb");
        run.until(10.0, "Bob is invited", |r| {
            r.shown_since(bob, mb, "invites you to the channel oldguard@channel")
        });
        talk(run.p(bob), "@dn channel join oldguard");
        run.wait(1.0);
        let ma = run.players[ann].shown.len();
        tell(run.p(bob), "oldguard@channel", "anyone around tonight?");
        run.until(10.0, "Ann hears the channel", |r| {
            r.shown_since(
                ann,
                ma,
                "oldguard@channel tells you, \"Bob@bb: anyone around tonight?\"",
            )
        });

        // A squelch.
        run.p(bob).action(
            &CommunicationModifyCharacterSquelch {
                add: 1,
                character_id: ObjectId(0),
                character_name: "Ann@aa".into(),
                msg_type: 1,
            },
            "/squelch Ann@aa   [Communication_ModifyCharacterSquelch 0x0058]",
        );
        run.until(10.0, "the squelch", |r| {
            r.shown_since(bob, mb, "Ann@aa has been squelched.")
        });
        let (ma, mb) = (run.players[ann].shown.len(), run.players[bob].shown.len());
        tell(run.p(ann), "Bob@bb", "are you ignoring me?");
        run.until(10.0, "Ann's echo", |r| {
            r.shown_since(ann, ma, "You tell Bob@bb, \"are you ignoring me?\"")
        });
        run.wait(2.0);
        assert!(
            !run.shown_since(bob, mb, "are you ignoring me?"),
            "a squelched tell was shown"
        );
        run.p(bob).action(
            &CommunicationModifyCharacterSquelch {
                add: 0,
                character_id: ObjectId(0),
                character_name: "Ann@aa".into(),
                msg_type: 1,
            },
            "/unsquelch Ann@aa   [Communication_ModifyCharacterSquelch 0x0058]",
        );
        run.wait(1.0);

        // A world whose federation flag is off.
        let ma = run.players[ann].shown.len();
        tell(run.p(ann), "Cid@cc", "hello?");
        run.until(10.0, "cc is not on the network", |r| {
            r.shown_since(ann, ma, "Character not available")
                && r.shown_since(ann, ma, "cc isn't on the network.")
        });

        // The hub goes away and comes back within the minute.
        hub.child.kill().expect("the hub stops");
        let _ = hub.child.wait();
        notes.push("- the hub was stopped".into());
        let (ma, mb) = (run.players[ann].shown.len(), run.players[bob].shown.len());
        run.until(15.0, "Bob shows offline while the hub is away", |r| {
            r.shown_since(ann, ma, "Bob@bb (0x")
                && r.players[ann].shown[ma..]
                    .iter()
                    .any(|l| l.contains("friends status") && l.contains("offline"))
        });
        tell(run.p(ann), "Bob@bb", "sent while the hub was away");
        run.wait(3.0);
        hub = start_hub(&dir, hub_port);
        notes.push("- the hub was started again on the same port and registry".into());
        run.until(45.0, "the held tell arrives", |r| {
            r.shown_since(bob, mb, "Ann@aa tells you, \"sent while the hub was away\"")
        });
        run.until(15.0, "Bob shows online again", |r| {
            r.players[ann].shown[ma..].iter().any(|l| {
                l.contains("friends status") && l.contains("Bob@bb") && l.contains("online]")
            })
        });

        // A friend logging off.
        let ma = run.players[ann].shown.len();
        run.p(bob).bot.finish = true;
        run.p(bob).client.log_off();
        run.until(30.0, "Ann sees Bob go offline", |r| {
            r.shown_since(ann, ma, "Bob@bb<\\Tell> has gone offline.")
        });

        // The hub goes away for good.
        hub.child.kill().expect("the hub stops");
        let _ = hub.child.wait();
        notes.push("- the hub was stopped for good".into());
        let ma = run.players[ann].shown.len();
        tell(run.p(ann), "Bob@bb", "nobody will carry this");
        run.wait(55.0);
        assert!(
            !run.shown_since(ann, ma, "The Dereth Network is unavailable."),
            "given up too soon"
        );
        run.until(30.0, "the network is unavailable", |r| {
            r.shown_since(ann, ma, "The Dereth Network is unavailable.")
        });

        // The record.
        let mut md = String::new();
        let _ = writeln!(
            md,
            "# The Dereth Network on loopback: two worlds through the hub\n"
        );
        for n in &notes {
            let _ = writeln!(md, "{n}");
        }
        for p in &run.players {
            let _ = writeln!(
                md,
                "\n## What {}@{}'s client was shown\n\n```",
                p.name, p.world
            );
            for l in &p.shown {
                let _ = writeln!(md, "{l}");
            }
            let _ = writeln!(md, "```");
        }
        let wire = std::fs::read_to_string(dir.join("wire.log")).unwrap_or_default();
        let _ = writeln!(
            md,
            "\n## What the links carried (the hub's wire log: types only)\n\n```\n{}```",
            wire
        );
        for p in &run.players {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for w in &p.wire {
                *counts.entry(w.as_str()).or_insert(0) += 1;
            }
            let _ = writeln!(
                md,
                "\n## Every message {}'s client received, by kind\n",
                p.name
            );
            for (k, n) in counts {
                let _ = writeln!(md, "- {k}: {n}");
            }
        }
        std::fs::write(dir.join("transcript.md"), &md).expect("the transcript");
        println!("{md}");
        if run_dir.keep {
            println!("written to {}", dir.display());
        } else {
            println!("(the run folder is removed now; set NETWORK_E2E_DIR to keep it)");
        }
        for (_, mut w) in worlds {
            let _ = w.child.kill();
            let _ = w.child.wait();
        }
    }
}
