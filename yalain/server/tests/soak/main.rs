//! The soak and load tier: this server's executable on the September 2013 install and a world
//! database, with players logged in over loopback playing a few scripted behaviors for a stretch of
//! time, the world measured and checked as they play. Feature `soak`; never in the default gate:
//! `cargo xtask gate yalain-soak` runs it, or `cargo test -p yalain --features soak --test soak
//! --profile test-release -- --nocapture`.
//!
//! Behaviour: none (a soak run: it measures the world under load and holds it to its invariants: no panic, every player in, the steps within budget, no leaked objects)
//!
//! Each run starts a fresh server whose players are made by the operator's recipes, spread over the
//! world's most populated places, each with a sword ([`server`]). They log in a tenth of a second
//! apart and play ([`bot`]): idle, wander, chat, fight and loot, take a portal, mixed by a seed. Once all
//! are in and a moment has passed, the world is sampled for the run's length: its step times by
//! stage from the status endpoint, its objects and players, the server's memory and processor time
//! ([`metrics`]). Then every player leaves, the world runs on for a tail, and the run is checked
//! ([`invariants`]) and compared with its host's baseline ([`profile`]).
//!
//! Settings, from the environment:
//! - `YALAIN_SOAK_PLAYERS`: the numbers of players, a run each, a fresh server each (default `1,25`);
//! - `YALAIN_SOAK_SECONDS`: how long each run is sampled once its players are in (default 120);
//! - `YALAIN_SOAK_MIX`: the behaviors' weights (default `idle=1,wander=3,chat=1,fight=4,portal=1`);
//! - `YALAIN_SOAK_SEED`: the players' seed (default `0x5EED`);
//! - `YALAIN_SOAK_TAIL`: seconds the world runs on after everyone has left (default 60);
//! - `YALAIN_SOAK_OVERRUN_SHARE`: the most of the steps that may overrun (default 0.05);
//! - `YALAIN_SOAK_OUT`: the folder the reports go to (default cargo's folder for tests' files);
//! - `YALAIN_SOAK_BASELINES`: the baselines' folder (default `target/soak-baselines`);
//!   `YALAIN_SOAK_RECORD=1` keeps each run as its host's baseline, and `YALAIN_SOAK_STRICT=1` fails
//!   a run worse than its baseline beyond the margins (otherwise a warning in the report);
//! - `YALAIN_TEST_WORLD_DB`: the world database, a checkout; else the release this build pins.
//!
//! Memory and processor time are read from Linux's process status, else from `ps`.

mod bot;
mod invariants;
mod metrics;
mod profile;
mod server;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use yalain_net::LocalTime;

use bot::{Mix, Player, Stage};
use metrics::{Measured, Sample};
use server::Server;

/// Seconds between two players' logins.
const LOGIN_EVERY: f64 = 0.1;

/// Seconds after the last player enters before sampling begins.
const SETTLE: f64 = 15.0;

/// A setting from the environment, or its default.
fn setting(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn number(name: &str, default: &str) -> f64 {
    setting(name, default)
        .parse()
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A folder of its own, emptied.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// A player's name: "Soak" and two letters of its number.
fn name(i: usize) -> String {
    let letter = |n: usize| char::from(b'a' + u8::try_from(n % 26).unwrap_or(0));
    format!("Soak {}{}", letter(i / 26).to_ascii_uppercase(), letter(i))
}

/// What a run is asked to be.
struct Shape {
    players: usize,
    seconds: f64,
    tail: f64,
    mix: Mix,
    seed: u64,
}

/// One run: a fresh server, its players playing, sampled, checked.
#[allow(clippy::too_many_lines)]
fn run(install: &Path, world_db: &str, shape: &Shape) -> Measured {
    let n = shape.players;
    let dir = scratch(&format!("soak-{n}"));
    let world = server::world(install, world_db, &dir);
    let places: Vec<server::Place> = (0..n)
        .map(|i| world.places[i % world.places.len()])
        .collect();
    let recipes: Vec<PathBuf> = places
        .iter()
        .enumerate()
        .map(|(i, p)| server::recipe(&dir, i, &name(i), *p))
        .collect();
    let mut server = Server::start(install, world_db, &dir, &recipes);
    let mut m = Measured {
        players: n,
        seconds: shape.seconds,
        mix: shape.mix.text(),
        seed: shape.seed,
        creatures: world.creatures,
        places: world.places.len().min(n),
        ..Measured::default()
    };
    let objects = |s: &Server| s.status()["world_objects"].as_u64().unwrap_or(0);
    m.objects_before = objects(&server);
    m.rss_before = server.usage().map(|u| u.1);
    let started = Instant::now();
    let now = || LocalTime(started.elapsed().as_secs_f64());
    let templates = Arc::new(world.templates);
    let portals = Arc::new(world.portals);
    #[allow(clippy::cast_precision_loss)]
    let mut players: Vec<Player> = places
        .iter()
        .enumerate()
        .map(|(i, p)| {
            Player::new(
                i,
                server.login,
                *p,
                i as f64 * LOGIN_EVERY,
                shape.seed,
                shape.mix.clone(),
                Arc::clone(&templates),
                Arc::clone(&portals),
            )
        })
        .collect();
    while players.iter().any(|p| p.stage != Stage::InWorld) {
        let t = now();
        for p in &mut players {
            p.turn(t);
        }
        if started.elapsed() > server::START_LIMIT {
            let stuck: Vec<String> = players
                .iter()
                .filter(|p| p.stage != Stage::InWorld)
                .map(|p| format!("{} {:?}", p.account, p.stage))
                .collect();
            panic!(
                "{} of {n} players entered; not: {}",
                n - stuck.len(),
                stuck.join("; ")
            );
        }
        assert!(server.running(), "the server stopped while players entered");
        std::thread::sleep(Duration::from_millis(2));
    }
    m.entered_after = started.elapsed().as_secs_f64();
    let begin = now().0 + SETTLE;
    let end = begin + shape.seconds;
    let every = (shape.seconds / 4.0).clamp(30.0, 60.0);
    let mut next_sample = begin + every;
    let mut next_memory = begin;
    let mut cpu_from: Option<(f64, f64)> = None;
    let mut peak = m.rss_before.unwrap_or(0.0);
    let sample = |server: &Server, at: f64, cpu_from: &mut Option<(f64, f64)>| {
        let status = server.status();
        let usage = server.usage();
        let cpu = usage.and_then(|(c, _)| {
            let share = cpu_from.map(|(c0, t0)| (c - c0) / (at - t0).max(1e-9));
            *cpu_from = Some((c, at));
            share
        });
        Sample {
            at: at - begin,
            step: status["world_step"].clone(),
            objects: status["world_objects"].as_u64().unwrap_or(0),
            players: status["players_online"].as_u64().unwrap_or(0),
            rss_mb: usage.map(|u| u.1),
            cpu,
        }
    };
    while now().0 < end {
        let t = now();
        for p in &mut players {
            p.turn(t);
        }
        if m.first.is_null() && t.0 >= begin {
            m.first = server.status()["world_step"].clone();
            m.objects_in = objects(&server);
            let _ = sample(&server, t.0, &mut cpu_from);
        }
        if t.0 >= next_memory {
            next_memory += 5.0;
            if let Some((_, rss)) = server.usage() {
                peak = peak.max(rss);
            }
        }
        if t.0 >= next_sample {
            next_sample += every;
            m.samples.push(sample(&server, t.0, &mut cpu_from));
        }
        assert!(server.running(), "the server stopped during the run");
        std::thread::sleep(Duration::from_millis(2));
    }
    let t = now().0;
    if m.samples.last().is_none_or(|s| s.at < shape.seconds - 1.0) {
        m.samples.push(sample(&server, t, &mut cpu_from));
    }
    m.last = server.status()["world_step"].clone();
    m.rss_peak = Some(peak);
    m.in_world_at_end = players.iter().filter(|p| p.stage == Stage::InWorld).count();
    for p in &mut players {
        p.close(t);
        p.bot.leave();
        p.turn(now());
    }
    m.players_counts = players.iter().map(|p| p.counts.clone()).collect();
    drop(players);
    // Everyone gone (the logoffs linger), the world runs on.
    let tail_end = Instant::now() + Duration::from_secs_f64(shape.tail);
    while Instant::now() < tail_end {
        assert!(
            server.running(),
            "the server stopped after the players left"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    m.objects_after = objects(&server);
    let log = server.stop();
    m.panicked = log.contains("THE WORLD PANICKED");
    m.warned = log.matches("a world step's work took").count();
    std::fs::write(dir.join("run.json"), m.json().to_string()).expect("the run is written");
    m
}

/// A world with players playing a mix of behaviors keeps its steps within budget, never panics, and
/// holds no more objects once everyone has left than before they came (but for corpses still lying):
/// a run for each number of players asked, each reported, checked and compared with the host's
/// baseline.
#[test]
fn a_world_with_players_playing_keeps_its_steps_in_budget_and_leaks_nothing() {
    let install = auberean_dat::testing::require_target_install();
    let world_db = setting("YALAIN_TEST_WORLD_DB", "pinned");
    let counts: Vec<usize> = setting("YALAIN_SOAK_PLAYERS", "1,25")
        .split(',')
        .map(|n| n.trim().parse().expect("a number of players"))
        .collect();
    let seed = {
        let s = setting("YALAIN_SOAK_SEED", "0x5EED");
        u64::from_str_radix(
            s.trim_start_matches("0x"),
            if s.starts_with("0x") { 16 } else { 10 },
        )
        .expect("a seed")
    };
    let mix = Mix::read(&setting(
        "YALAIN_SOAK_MIX",
        "idle=1,wander=3,chat=1,fight=4,portal=1",
    ));
    let out = std::env::var_os("YALAIN_SOAK_OUT")
        .map_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")), PathBuf::from);
    let baselines = std::env::var_os("YALAIN_SOAK_BASELINES").map_or_else(
        || {
            PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
                .parent()
                .map_or_else(
                    || PathBuf::from("soak-baselines"),
                    |t| t.join("soak-baselines"),
                )
        },
        PathBuf::from,
    );
    let record = setting("YALAIN_SOAK_RECORD", "0") == "1";
    let strict = setting("YALAIN_SOAK_STRICT", "0") == "1";
    let overrun_share = number(
        "YALAIN_SOAK_OVERRUN_SHARE",
        &invariants::OVERRUN_SHARE.to_string(),
    );
    std::fs::create_dir_all(&out).expect("the report's folder");
    let mut written = format!("# Soak: world {world_db}, host {}\n\n", profile::host());
    let mut failed = Vec::new();
    for players in counts {
        let shape = Shape {
            players,
            seconds: number("YALAIN_SOAK_SECONDS", "120"),
            tail: number("YALAIN_SOAK_TAIL", "60"),
            mix: mix.clone(),
            seed,
        };
        let m = run(&install, &world_db, &shape);
        let broken = invariants::check(&m, overrun_share);
        let worse =
            profile::compare_and_record(&m, &baselines, profile::Margins::default(), record);
        let mut text = m.report();
        for b in &broken {
            text.push_str(&format!("FAILED: {b}\n"));
        }
        for w in &worse {
            text.push_str(&format!("WORSE THAN THE BASELINE: {w}\n"));
        }
        text.push('\n');
        println!("{text}");
        written.push_str(&text);
        std::fs::write(out.join("soak-report.md"), &written).expect("the report is written");
        failed.extend(
            broken
                .into_iter()
                .map(|b| format!("{players} players: {b}")),
        );
        if strict {
            failed.extend(worse.into_iter().map(|w| format!("{players} players: {w}")));
        }
    }
    assert!(failed.is_empty(), "the soak failed:\n{}", failed.join("\n"));
}
