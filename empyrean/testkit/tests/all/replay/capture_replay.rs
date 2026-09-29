//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! (features real-content,captures) Every recording satisfies the invariants and replays against
//! our server; each invariant catches its violation; documented exemptions and unreplayed/RNG-
//! divergent classes; death anchoring.
//! Fixture: recorded packet captures and a virtual-time server with retail content.

use std::collections::BTreeMap;

use empyrean_testkit::capture_replay::compare::{self, Finding, SessionReport};
use empyrean_testkit::capture_replay::invariants::{self, InvariantReport};
use empyrean_testkit::capture_replay::recording;
use empyrean_testkit::capture_replay::replay_session;

fn sessions() -> Vec<String> {
    let names = recording::session_names(&recording::captures_root());
    assert!(names.len() >= 20, "the corpus: {} recordings", names.len());
    // CAPTURE_REPLAY_SESSIONS=a,b narrows a local run; the tier itself runs them all
    match std::env::var("CAPTURE_REPLAY_SESSIONS") {
        Ok(only) if !only.is_empty() => names
            .into_iter()
            .filter(|n| only.split(',').any(|o| o == n))
            .collect(),
        _ => names,
    }
}

fn summary(r: &InvariantReport) -> String {
    let v: Vec<String> = r
        .violation_counts()
        .iter()
        .map(|(k, n)| format!("{k} x{n}"))
        .collect();
    let d: Vec<String> = r
        .documented
        .iter()
        .map(|(k, n)| format!("{k} x{n}"))
        .collect();
    let o: Vec<String> = r
        .observed
        .iter()
        .map(|(k, n)| format!("{k} x{n}"))
        .collect();
    format!(
        "violations [{}] documented [{}] observed [{}]",
        v.join(", "),
        d.join(", "),
        o.join(", ")
    )
}

/// Every invariant holds on every ACE recording, with its documented exemptions.
#[test]
fn the_recordings_satisfy_the_invariants() {
    let root = recording::captures_root();
    let mut total: BTreeMap<&str, usize> = BTreeMap::new();
    let mut bad = Vec::new();
    for name in sessions() {
        let rec = recording::load(&root, &name);
        let r = invariants::check(&rec.msgs);
        println!(
            "{name:<32} {} messages ({} unparsed datagrams): {}",
            rec.msgs.len(),
            rec.unparsed,
            summary(&r)
        );
        for (k, n) in &r.checks {
            *total.entry(k).or_insert(0) += n;
        }
        for v in &r.violations {
            bad.push(format!(
                "{name}: {} at #{} (t {:.2}): {}",
                v.invariant, v.index, v.t, v.detail
            ));
        }
    }
    println!("checks: {total:?}");
    for (id, _) in invariants::INVARIANTS {
        assert!(
            total.get(id).copied().unwrap_or(0) > 0,
            "invariant {id} was never checked"
        );
    }
    assert!(
        bad.is_empty(),
        "ACE violates an invariant as stated:\n{}",
        bad.join("\n")
    );
}

fn print_findings(reports: &[SessionReport]) {
    let mut all: BTreeMap<(String, String), (Finding, Vec<String>)> = BTreeMap::new();
    for r in reports {
        for f in &r.findings {
            let e = all
                .entry((f.opener.clone(), f.label.clone()))
                .or_insert_with(|| {
                    (
                        Finding {
                            opener: f.opener.clone(),
                            label: f.label.clone(),
                            ..Finding::default()
                        },
                        Vec::new(),
                    )
                });
            e.0.missing += f.missing;
            e.0.extra += f.extra;
            e.0.phases += f.phases;
            for (k, v) in &f.not_ported {
                *e.0.not_ported.entry(k).or_insert(0) += v;
            }
            e.1.push(r.name.clone());
        }
    }
    let mut ranked: Vec<(Finding, Vec<String>)> = all.into_values().collect();
    ranked.sort_by_key(|(f, _)| std::cmp::Reverse(f.missing + f.extra));
    println!("\n=== unexplained differences, ranked (opener | label | missing/extra | phases | sessions | not_ported sites) ===");
    for (f, s) in &ranked {
        let np: Vec<String> = f
            .not_ported
            .iter()
            .map(|(k, v)| format!("{k} x{v}"))
            .collect();
        println!(
            "{} | {} | -{} +{} | {} | {} | {}",
            f.opener,
            f.label,
            f.missing,
            f.extra,
            f.phases,
            s.join(" "),
            np.join("; ")
        );
    }
}

static WORLDS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Every recording replayed closed-loop against our server on real content.
#[test]
fn every_recording_replays_against_our_server() {
    let _one_at_a_time = WORLDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let names = sessions();
    assert!(!names.is_empty(), "no recording selected");
    // Program.Main's CommandManager.Initialize, as empyrean-server runs it: the recorded `@` commands
    // reach their handlers (empyrean-command installs its Talk hook; process-wide, first install wins)
    empyrean_command::command_manager::initialize(None);
    let workers = std::env::var("CAPTURE_REPLAY_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4usize);
    let queue = std::sync::Mutex::new(names.clone());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let Some(name) = queue.lock().unwrap().pop() else {
                    break;
                };
                let outcome = replay_session(&name);
                if std::env::var_os("CAPTURE_REPLAY_DUMP").is_some() {
                    // structure only: time, direction, label, subject guid
                    for (side, msgs) in [("ACE", &outcome.recorded.msgs), ("ours", &outcome.ours)] {
                        for m in msgs.iter() {
                            use empyrean_testkit::capture_replay::wire;
                            let subject = wire::subject(&m.payload)
                                .map_or(String::new(), |g| format!("{g:08X}"));
                            // a private update's property key (a number, never a value)
                            let key = if wire::PRIVATE_UPDATES.contains(&m.opcode()) {
                                wire::u32_at(&m.payload, 5)
                                    .map_or(String::new(), |k| format!("key {k}"))
                            } else {
                                String::new()
                            };
                            let class = if m.opcode() == wire::CREATE_OBJECT {
                                empyrean_testkit::capture_replay::replay::create_class(&m.payload)
                                    .map_or(String::new(), |(w, t, b)| {
                                        format!(" wcid {w} type {t:X} flags {b:X}")
                                    })
                            } else {
                                String::new()
                            };
                            let class = if m.opcode() == wire::UPDATE_POSITION {
                                format!(" pf {:X}", wire::u32_at(&m.payload, 8).unwrap_or(0))
                            } else {
                                class
                            };
                            let class = if m.opcode() == wire::UPDATE_MOTION {
                                format!(
                                    " mt {} x {:08X}",
                                    m.payload.get(16).copied().unwrap_or(0xFF),
                                    wire::u32_at(&m.payload, 20).unwrap_or(0)
                                )
                            } else {
                                class
                            };
                            let class = if m.opcode() == wire::UPDATE_MOTION
                                && m.payload.get(16).copied() == Some(0)
                            {
                                let flags = wire::u32_at(&m.payload, 20).unwrap_or(0);
                                let at = if flags & 1 != 0 { 26 } else { 24 };
                                let fc = if flags & 2 != 0 {
                                    wire::u16_at(&m.payload, at).unwrap_or(0)
                                } else {
                                    0
                                };
                                format!("{class} fc {fc:04X}")
                            } else {
                                class
                            };
                            let class =
                                if matches!(wire::event_type(&m.payload), Some(0x028A | 0x028B)) {
                                    format!(
                                        "{class} err {:04X}",
                                        wire::u32_at(&m.payload, 16).unwrap_or(0)
                                    )
                                } else {
                                    class
                                };
                            let subject = format!("{subject}{key}{class}");
                            println!(
                                "DUMP {name} {side} {:9.3} {} {} {subject}",
                                m.t,
                                if m.c2s { ">" } else { "<" },
                                empyrean_testkit::capture_replay::wire::label(&m.payload)
                            );
                        }
                    }
                }
                let report = compare::compare(&outcome);
                if std::env::var_os("CAPTURE_REPLAY_GROUPS").is_some() {
                    for (g, opener, label, v) in &report.unexplained_at {
                        println!("GROUP {name} #{g} {opener} {label} {v:+}");
                    }
                }
                println!("{}", report.line());
                results.lock().unwrap().push(report);
            });
        }
    });
    let mut reports = results.into_inner().unwrap();
    reports.sort_by(|a, b| a.name.cmp(&b.name));

    println!("\n=== per session ===");
    for r in &reports {
        println!("{}", r.line());
    }
    println!("\n=== invariants (ACE recording | ours) ===");
    for r in &reports {
        println!(
            "{:<32} ACE {}\n{:<32} ours {}",
            r.name,
            summary(&r.ace),
            "",
            summary(&r.ours)
        );
    }
    print_findings(&reports);
    println!("\n=== ambient differences (label: ACE, ours) ===");
    for r in &reports {
        let d: Vec<String> = r
            .ambient_diff
            .iter()
            .map(|(k, (a, b))| format!("{k}: {a}/{b}"))
            .collect();
        println!("{:<32} {}", r.name, d.join("; "));
    }
    println!("\n=== not_ported sites hit throughout (per session) ===");
    for r in &reports {
        let b: Vec<String> = r
            .background_sites
            .iter()
            .map(|(k, v)| format!("{k} x{v}"))
            .collect();
        println!("{:<32} {}", r.name, b.join("; "));
    }
    println!("\n=== steering: recorded monster deaths anchored in ours; capped waits ===");
    for r in &reports {
        let n: Vec<String> = r
            .not_steered
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect();
        println!(
            "{:<32} steered {} not steered [{}] capped waits {} unrecorded deaths {}",
            r.name,
            r.steered,
            n.join(", "),
            r.capped_waits,
            r.unrecorded_deaths
        );
    }
    println!("\n=== replay notes ===");
    for r in &reports {
        for n in &r.notes {
            println!("{:<32} {n}", r.name);
        }
    }
    let (m, e, u): (usize, usize, usize) = reports.iter().fold((0, 0, 0), |a, r| {
        (
            a.0 + r.matched,
            a.1 + r.explained_total(),
            a.2 + r.unexplained,
        )
    });
    let (c, g): (usize, usize) = reports.iter().fold((0, 0), |a, r| {
        (
            a.0 + r.category("unreplayed-client"),
            a.1 + r.category("rng-divergent"),
        )
    });
    let (st, d): (usize, usize) = reports
        .iter()
        .fold((0, 0), |a, r| (a.0 + r.steered, a.1 + r.unrecorded_deaths));
    println!(
        "\nTOTAL: {} sessions, matched {m}, explained {e}, unreplayed-client {c}, rng-divergent {g}, unexplained {u}, steered {st}, unrecorded-death {d}",
        reports.len()
    );

    assert_eq!(reports.len(), names.len(), "every recording was replayed");
    // A crashed session counts every message as unexplained, so it must fail the tier, not hide in it
    // (a wrong world.pack once turned all 20 sessions into crashes that still passed).
    let crashed: Vec<String> = reports
        .iter()
        .filter_map(|r| r.crashed.as_ref().map(|why| format!("{}: {why}", r.name)))
        .collect();
    assert!(
        crashed.is_empty(),
        "replays crashed:\n{}",
        crashed.join("\n")
    );
    let violations: Vec<String> = reports
        .iter()
        .flat_map(|r| {
            r.ours.violations.iter().map(move |v| {
                format!(
                    "{}: {} at #{} (t {:.2}): {}",
                    r.name, v.invariant, v.index, v.t, v.detail
                )
            })
        })
        .collect();
    assert!(
        violations.is_empty(),
        "our replays violate invariants:\n{}",
        violations.join("\n")
    );
}

// ---- the invariant checker on synthetic streams (no recording, no content) -----------------------

mod checker {
    use empyrean_testkit::capture_replay::invariants::check;
    use empyrean_testkit::capture_replay::recording::Msg;

    const ME: u32 = 0x5000_0001;
    const ITEM: u32 = 0x8000_0010;

    fn m(t: f64, c2s: bool, parts: &[&[u8]]) -> Msg {
        Msg {
            t,
            c2s,
            queue: 0,
            seq: 0,
            payload: parts.concat(),
        }
    }
    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn enter(t: f64) -> Msg {
        m(t, true, &[&le(0xF657), &le(ME)])
    }
    fn event(t: f64, target: u32, seq: u32, kind: u32, rest: &[u8]) -> Msg {
        m(
            t,
            false,
            &[&le(0xF7B0), &le(target), &le(seq), &le(kind), rest],
        )
    }
    fn create(t: f64, g: u32) -> Msg {
        m(t, false, &[&le(0xF745), &le(g)])
    }
    fn motion(t: f64, g: u32, seq: u16) -> Msg {
        m(
            t,
            false,
            &[
                &le(0xF74C),
                &le(g),
                &1u16.to_le_bytes(),
                &seq.to_le_bytes(),
                &[0, 0, 0, 0],
            ],
        )
    }
    fn action(t: f64, stamp: u32, kind: u32) -> Msg {
        m(t, true, &[&le(0xF7B1), &le(stamp), &le(kind)])
    }
    fn vital(t: f64, seq: u8, key: u32, current: u32) -> Msg {
        m(t, false, &[&le(0x02E9), &[seq], &le(key), &le(current)])
    }
    /// A clean login: enter, PlayerDescription, PlayerCreate, own create, a possession.
    fn login() -> Vec<Msg> {
        vec![
            enter(0.0),
            event(0.1, ME, 1, 0x0013, &[]),
            m(0.1, false, &[&le(0xF746), &le(ME)]),
            create(0.1, ME),
            create(0.1, ITEM),
        ]
    }
    fn violated(msgs: &[Msg]) -> Vec<&'static str> {
        check(msgs).violations.iter().map(|v| v.invariant).collect()
    }

    #[test]
    fn a_clean_stream_satisfies_every_invariant() {
        let mut s = login();
        s.extend([
            motion(1.0, ITEM, 1),
            motion(1.1, ITEM, 2),
            action(2.0, 1, 0x0036),
            event(2.1, ME, 2, 0x01C7, &[]),
            vital(3.0, 1, 2, 50),
            vital(3.1, 2, 2, 60),
        ]);
        assert_eq!(violated(&s), Vec::<&str>::new());
    }

    #[test]
    fn each_invariant_catches_its_violation() {
        let base = login;
        let cases: Vec<(&str, Vec<Msg>)> = vec![
            ("event-sequence", vec![event(1.0, ME, 3, 0x0021, &[])]),
            (
                "event-target",
                vec![event(1.0, 0x5000_0002, 2, 0x0021, &[])],
            ),
            ("known-object", vec![motion(1.0, 0x8000_0099, 1)]),
            (
                "delete-known",
                vec![m(1.0, false, &[&le(0xF747), &le(0x8000_0099)])],
            ),
            (
                "object-sequences",
                vec![motion(1.0, ITEM, 5), motion(1.1, ITEM, 4)],
            ),
            (
                "private-sequences",
                vec![vital(1.0, 5, 2, 50), vital(9.0, 4, 2, 50)],
            ),
            ("use-done", vec![event(1.0, ME, 2, 0x01C7, &[])]),
            (
                "vitals",
                vec![event(
                    1.0,
                    ME,
                    2,
                    0x01C0,
                    &[le(ITEM), 1.5f32.to_le_bytes()].concat(),
                )],
            ),
        ];
        for (id, tail) in cases {
            let mut s = base();
            s.extend(tail);
            assert_eq!(violated(&s), [id], "{id}");
        }
        // a login with no PlayerDescription
        let s = vec![
            enter(0.0),
            m(0.1, false, &[&le(0xF746), &le(ME)]),
            create(0.1, ME),
        ];
        assert_eq!(violated(&s), ["login-shape"]);
    }

    #[test]
    fn aces_documented_exemptions_are_not_violations() {
        // early-announce: an update 1 s before its object's create
        let mut s = login();
        s.extend([motion(1.0, 0x8000_0099, 1), create(2.0, 0x8000_0099)]);
        let r = check(&s);
        assert!(r.violations.is_empty());
        assert_eq!(r.documented.get("early-announce"), Some(&1));
        // ... but not 3 s before it
        let mut s = login();
        s.extend([motion(1.0, 0x8000_0099, 1), create(4.0, 0x8000_0099)]);
        assert_eq!(violated(&s), ["known-object"]);

        // death-vitals: one sequence behind in the same send
        let mut s = login();
        s.extend([vital(1.0, 26, 2, 50), vital(1.0, 25, 2, 50)]);
        let r = check(&s);
        assert!(r.violations.is_empty());
        assert_eq!(r.documented.get("death-vitals"), Some(&1));

        // health-after-logoff: UpdateHealth to 0 after the logoff
        let mut s = login();
        s.extend([
            m(5.0, false, &[&le(0xF653)]),
            event(
                6.0,
                0,
                2,
                0x01C0,
                &[le(ITEM), 0.5f32.to_le_bytes()].concat(),
            ),
        ]);
        let r = check(&s);
        assert!(r.violations.is_empty());
        assert_eq!(r.documented.get("health-after-logoff"), Some(&1));
    }
}

mod categories {
    use std::collections::BTreeMap;

    use empyrean_testkit::capture_replay::compare::compare;
    use empyrean_testkit::capture_replay::recording::{Msg, Recording};
    use empyrean_testkit::capture_replay::replay::ReplayOutcome;

    const ME: u32 = 0x5000_0001;
    const OTHER: u32 = 0x5000_0002;
    const MONSTER: u32 = 0x8000_0010;

    fn m(t: f64, c2s: bool, parts: &[&[u8]]) -> Msg {
        Msg {
            t,
            c2s,
            queue: 0,
            seq: 0,
            payload: parts.concat(),
        }
    }
    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn enter(t: f64) -> Msg {
        m(t, true, &[&le(0xF657), &le(ME)])
    }
    fn action(t: f64, kind: u32, target: u32) -> Msg {
        m(t, true, &[&le(0xF7B1), &le(1), &le(kind), &le(target)])
    }
    fn event(t: f64, kind: u32, rest: &[u8]) -> Msg {
        m(t, false, &[&le(0xF7B0), &le(ME), &le(1), &le(kind), rest])
    }
    fn create(t: f64, g: u32) -> Msg {
        m(t, false, &[&le(0xF745), &le(g)])
    }
    fn xp(t: f64) -> Msg {
        m(t, false, &[&le(0x02CF), &[1], &le(2), &le(100), &le(0)])
    }
    fn logoff(t: f64, c2s: bool) -> Msg {
        m(t, c2s, &[&le(0xF653)])
    }
    fn motion(t: f64) -> Msg {
        m(t, false, &[&le(0xF74C), &le(ME), &le(0)])
    }
    fn login() -> Vec<Msg> {
        vec![enter(0.0), create(0.1, ME)]
    }
    fn outcome(recorded: Vec<Msg>, ours: Vec<Msg>) -> ReplayOutcome {
        ReplayOutcome {
            name: "synthetic".to_owned(),
            recorded: Recording {
                name: "synthetic".to_owned(),
                msgs: recorded,
                unparsed: 0,
            },
            ours,
            not_ported: vec![BTreeMap::new(); 8],
            notes: Vec::new(),
            reconstructed: 0,
            missing_weenies: 0,
            crashed: None,
            steered: Vec::new(),
            not_steered: Vec::new(),
            capped_waits: 0,
            virtual_s: 0.0,
            wall_s: 0.0,
        }
    }

    /// A health update ACE sent about another player's character is `unreplayed-client`; the same
    /// label about a monster is unexplained.
    #[test]
    fn a_message_naming_another_players_character_is_unreplayed_client() {
        let mut rec = login();
        rec.extend([
            create(1.0, OTHER),
            create(1.0, MONSTER),
            action(2.0, 0x01BF, OTHER),
            event(2.1, 0x01C0, &le(OTHER)),
        ]);
        rec.extend([
            action(3.0, 0x01BF, MONSTER),
            event(3.1, 0x01C0, &le(MONSTER)),
        ]);
        let mut ours = login();
        ours.extend([action(2.0, 0x01BF, OTHER), action(3.0, 0x01BF, MONSTER)]);
        let r = compare(&outcome(rec, ours));
        assert_eq!(r.category("unreplayed-client"), 1);
        assert_eq!(
            r.unexplained, 1,
            "the monster's health is not the other client's"
        );
    }

    /// A logoff ACE completed after 20 s (a delayed PK logoff) in a recording with another player:
    /// every difference until the LogOffComplete is `unreplayed-client`. Without another player, or
    /// completed after 6 s, it is not.
    #[test]
    fn a_delayed_pk_logoff_with_another_player_is_unreplayed_client() {
        let run = |other: bool, done: f64| {
            let mut rec = login();
            if other {
                rec.push(create(1.0, OTHER));
            }
            rec.extend([
                logoff(2.0, true),
                motion(3.0),
                motion(3.1),
                motion(3.2),
                logoff(2.0 + done, false),
            ]);
            let mut ours = login();
            ours.extend([logoff(2.0, true), logoff(8.0, false)]);
            let r = compare(&outcome(rec, ours));
            (r.category("unreplayed-client"), r.unexplained)
        };
        assert_eq!(run(true, 22.0), (3, 0));
        assert_eq!(run(false, 22.0), (0, 3));
        assert_eq!(run(true, 6.0), (0, 3));
    }

    /// In a phase whose hit counts differ, the XP that followed is `rng-divergent`; in a phase whose
    /// roll outcomes agree it stays unexplained.
    #[test]
    fn roll_consequences_in_a_phase_whose_outcomes_differ_are_rng_divergent() {
        let hit = |t: f64| event(t, 0x01B1, &le(0));
        let mut rec = login();
        rec.extend([
            create(1.0, MONSTER),
            action(2.0, 0x0008, MONSTER),
            hit(2.1),
            xp(2.2),
            action(4.0, 0x0008, MONSTER),
            hit(4.1),
            xp(4.2),
        ]);
        let mut ours = login();
        ours.extend([
            action(2.0, 0x0008, MONSTER),
            action(4.0, 0x0008, MONSTER),
            hit(4.1),
        ]);
        let r = compare(&outcome(rec, ours));
        assert_eq!(
            r.category("rng-divergent"),
            2,
            "the first attack's missing hit and its XP"
        );
        assert_eq!(
            r.unexplained, 1,
            "the second attack hit on both sides: its missing XP is unexplained"
        );
    }

    /// Our character's death that the recording does not have is counted (never steered); the
    /// recording's own death, which ours also has, is not.
    #[test]
    fn a_death_ours_has_and_the_recording_does_not_is_counted() {
        let died = |t: f64| event(t, 0x01AC, &[]);
        let mut rec = login();
        rec.extend([action(2.0, 0x0008, MONSTER), died(2.1)]);
        let mut ours = login();
        ours.extend([
            action(2.0, 0x0008, MONSTER),
            died(2.1),
            action(3.0, 0x0008, MONSTER),
            died(3.1),
        ]);
        rec.push(action(3.0, 0x0008, MONSTER));
        assert_eq!(compare(&outcome(rec.clone(), ours)).unrecorded_deaths, 1);
        let mut ours = login();
        ours.extend([action(2.0, 0x0008, MONSTER), action(3.0, 0x0008, MONSTER)]);
        assert_eq!(
            compare(&outcome(rec, ours)).unrecorded_deaths,
            0,
            "a death only the recording has"
        );
    }
}

// ---- reconstruction: the enchantment registry from the PlayerDescription -------------------------

mod reconstruction {
    use dereth_protocol::login::LoginPlayerDescription;
    use dereth_protocol::objects::ObjectCreatePayload;
    use dereth_protocol::types::qualities::{Enchantment, EnchantmentRegistry, StatMod};
    use empyrean_content::MemContent;
    use empyrean_testkit::capture_replay::character::{
        reconstruct, ENCHANTMENT_CATEGORY_FROM_SPELL,
    };

    fn enchantment(spell: u32, layer: u32, category: u32, set: Option<u32>) -> Enchantment {
        Enchantment {
            id: layer << 16 | spell,
            category_word: category | if set.is_some() { 1 << 16 } else { 0 },
            power_level: 50,
            start_time: -12.5,
            duration: 1800.0,
            caster: dereth_primitives::ObjectId(0x5000_0001),
            degrade_modifier: 0.0,
            degrade_limit: -666.0,
            last_time_degraded: 0.0,
            smod: StatMod {
                kind: 0x4001,
                key: 6,
                value: 10.0,
            },
            spell_set_id: set,
        }
    }

    /// Each registry list becomes rows as ACE stores them: the spell id and layer split from the
    /// id word, vitae at layer 1 in the Vitae category, a cooldown in the Cooldown category, a spell
    /// marked for its MetaSpellType, and the spell set id only when the category word says so.
    #[test]
    fn the_enchantment_registry_is_rebuilt_from_the_player_description() {
        let mut pd = LoginPlayerDescription::default();
        pd.qualities.enchantments = Some(EnchantmentRegistry {
            flags: EnchantmentRegistry::MULTIPLICATIVE
                | EnchantmentRegistry::COOLDOWN
                | EnchantmentRegistry::VITAE,
            multiplicative: Some(vec![enchantment(1234, 2, 17, Some(13))]),
            additive: None,
            cooldowns: Some(vec![enchantment(0x8005, 1, 0x8000, None)]),
            vitae: Some(enchantment(666, 0, 204, None)),
        });
        let r = reconstruct(
            &MemContent::new(),
            0x5000_0001,
            1,
            "Replay Alpha",
            &pd,
            &ObjectCreatePayload::default(),
            &[],
        );
        let rows = r
            .biota
            .properties_enchantment_registry
            .expect("the registry");
        let got: Vec<(i32, u16, u32, bool, i32)> = rows
            .iter()
            .map(|e| {
                (
                    e.spell_id,
                    e.layer_id,
                    e.enchantment_category,
                    e.has_spell_set_id,
                    e.spell_set_id.0,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                (1234, 2, ENCHANTMENT_CATEGORY_FROM_SPELL, true, 13),
                (0x8005, 1, 0x8, false, 0),
                (666, 1, 0x4, false, 0)
            ]
        );
        let spell = &rows[0];
        assert_eq!(
            (
                spell.spell_category.0,
                spell.power_level,
                spell.start_time,
                spell.duration
            ),
            (17, 50, -12.5, 1800.0)
        );
        assert_eq!(
            (
                spell.stat_mod_type.0,
                spell.stat_mod_key,
                spell.stat_mod_value,
                spell.caster_object_id
            ),
            (0x4001, 6, 10.0, 0x5000_0001)
        );
    }
}

mod steering {
    use std::collections::{HashMap, HashSet};

    use empyrean_entity::enums::{PropertyBool, PropertyInt, PropertyInt64};
    use empyrean_entity::ObjectGuid;
    use empyrean_testkit::capture_replay::steer::{self, Steering};
    use empyrean_testkit::{ClientId, TestServer};
    use empyrean_world::dispatch::Class;
    use empyrean_world::world_objects::monster_combat;
    use empyrean_world::world_objects::world_object::{ProjectileLinks, WorldObject};

    use crate::monster_ai::{arena, MONSTER, PLAYER};

    /// The monster's guid in the recording; the replay bound it to ours (`MONSTER`).
    const RECORDED: u32 = 0x8000_0999;
    const DEAD: u16 = 0x0011;
    const READY: u16 = 0x0003;

    /// An UpdateMotion of `g`: an interpreted motion with a style and a forward command.
    fn motion(g: u32, forward: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend(0xF74Cu32.to_le_bytes());
        p.extend(g.to_le_bytes());
        p.extend(1u16.to_le_bytes()); // instance sequence
        p.extend(1u16.to_le_bytes()); // movement sequence
        p.extend(0u16.to_le_bytes()); // server control sequence
        p.extend([0, 0]); // autonomous, align
        p.extend([0, 0]); // movement type (interpreted), motion flags
        p.extend(0x3Du16.to_le_bytes()); // style
        p.extend(3u32.to_le_bytes()); // state flags: style, forward command
        p.extend(0x3Du16.to_le_bytes());
        p.extend(forward.to_le_bytes());
        p
    }

    fn delete(g: u32) -> Vec<u8> {
        [
            0xF747u32.to_le_bytes().as_slice(),
            &g.to_le_bytes(),
            &1u16.to_le_bytes(),
        ]
        .concat()
    }

    /// The arena's monster, alive, and a replay that has seen `recorded` (the recording's server
    /// messages); run past the grace.
    fn replay(recorded: &[Vec<u8>], bound: bool) -> (TestServer, ClientId, Steering) {
        let _one_at_a_time = super::WORLDS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut ts, client) = arena();
        setup(&mut ts);
        let creatures = HashSet::from([RECORDED]);
        let map = if bound {
            HashMap::from([(RECORDED, MONSTER.full())])
        } else {
            HashMap::new()
        };
        let mut s = Steering::default();
        let now = ts.seconds();
        for p in recorded {
            s.observe(p, &creatures, &map, Some(PLAYER.full()), now, 1);
        }
        run(&mut ts, &mut s, now + steer::GRACE_S + 0.25, &[]);
        (ts, client, s)
    }

    /// The arena, readied for a kill.
    fn setup(ts: &mut TestServer) {
        // the synthetic content has no corpse weenie: the monster leaves none (`Creature.CreateCorpse`)
        ts.world
            .objects
            .get_mut(MONSTER)
            .unwrap()
            .set_property(PropertyBool::NoCorpse, true);
        // the kill grants XP (`Creature.OnDeath_GrantXP`): the arena's player gets a level and XP
        let p = ts.world.objects.get_mut(PLAYER).unwrap();
        p.set_level(Some(1));
        p.set_property(PropertyInt64::TotalExperience, 0);
        p.set_property(PropertyInt64::AvailableExperience, 0);
        p.set_property(PropertyInt::AvailableSkillCredits, 0);
        p.set_property(PropertyInt::TotalSkillCredits, 0);
        assert!(!monster_combat::is_dead(
            ts.world.objects.get(MONSTER).unwrap()
        ));
    }

    fn run(ts: &mut TestServer, s: &mut Steering, until: f64, ours: &[u32]) {
        while ts.seconds() < until {
            ts.step();
            let t = ts.seconds();
            s.apply(&mut ts.world, t, ours.iter().copied());
        }
    }

    /// Our projectile still in flight at the monster: the anchor waits for it (our lethal hit may be
    /// that projectile), then applies once it is gone.
    #[test]
    fn an_anchor_waits_for_our_projectile_in_flight() {
        const ARROW: ObjectGuid = ObjectGuid::new(0x8000_0777);
        let _one_at_a_time = super::WORLDS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut ts, _client) = arena();
        setup(&mut ts);
        let mut arrow = WorldObject::allocate(Class::GenericObject);
        arrow.guid = ARROW;
        arrow.projectile = Some(ProjectileLinks {
            source: Some(PLAYER),
            target: Some(MONSTER),
            ..ProjectileLinks::default()
        });
        ts.world.objects.insert(arrow).expect("fresh guid");
        let mut s = Steering::default();
        let map = HashMap::from([(RECORDED, MONSTER.full())]);
        s.observe(
            &motion(RECORDED, DEAD),
            &HashSet::from([RECORDED]),
            &map,
            Some(PLAYER.full()),
            ts.seconds(),
            1,
        );
        let t = ts.seconds();
        run(&mut ts, &mut s, t + steer::GRACE_S + 1.0, &[ARROW.full()]);
        assert!(!dead(&ts) && s.steered.is_empty(), "waiting for the arrow");
        let _ = ts.world.objects.remove(ARROW);
        let t = ts.seconds();
        run(&mut ts, &mut s, t + 0.1, &[ARROW.full()]);
        assert!(dead(&ts), "the arrow is gone: anchored");
        assert_eq!(s.steered.len(), 1);
    }

    fn dead(ts: &TestServer) -> bool {
        ts.world
            .objects
            .get(MONSTER)
            .is_none_or(monster_combat::is_dead)
    }

    /// `KillerNotification` events our character received.
    fn kills(ts: &TestServer, client: ClientId) -> usize {
        ts.received_raw(client)
            .iter()
            .filter(|m| {
                m.opcode == 0xF7B0 && m.body.get(8..12) == Some(0x01ADu32.to_le_bytes().as_slice())
            })
            .count()
    }

    /// The recording's Dead motion kills our live monster through the server's death path (the
    /// killer's notification comes with it), once, counted as steered.
    #[test]
    fn a_recorded_death_anchors_the_live_monster() {
        let (ts, client, s) = replay(
            &[
                motion(RECORDED, READY),
                motion(RECORDED, DEAD),
                delete(RECORDED),
            ],
            true,
        );
        assert!(dead(&ts), "the monster died");
        assert_eq!(s.steered, [(1, MONSTER.full())], "anchored once");
        assert_eq!(kills(&ts, client), 1, "Creature.OnDeath told the killer");
        // a delete alone is evidence too
        let (ts, _, s) = replay(&[delete(RECORDED)], true);
        assert!(dead(&ts));
        assert_eq!(s.steered.len(), 1);
    }

    /// No recorded death, or a death of a monster the replay never translated: nothing is steered.
    #[test]
    fn without_a_recorded_death_nothing_is_steered() {
        let (ts, client, s) = replay(&[motion(RECORDED, READY)], true);
        assert!(!dead(&ts), "the monster lives");
        assert!(s.steered.is_empty() && s.not_steered.is_empty());
        assert_eq!(kills(&ts, client), 0);
        let (ts, _, s) = replay(&[motion(RECORDED, DEAD)], false);
        assert!(!dead(&ts), "an untranslated monster is not guessed");
        assert!(s.steered.is_empty());
        assert_eq!(s.not_steered, [(1, RECORDED, "untranslated")]);
    }
}
