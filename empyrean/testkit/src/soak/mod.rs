//! Soak and load runs (`--features soak`; never in the default gate).
//!
//! [`run`] starts a [`TestServer`] on the real content (the retail dats under `DERETH_TEST_DAT_DIR`,
//! `world.pack` at `EMPYREAN_TEST_WORLD_PACK`) and lets N [`bot::Bot`]s play for a stretch of virtual
//! time: they log in over the in-memory wire (staggered), create characters, enter the world and
//! play the playable-loop scenario in pieces (wander, chat, loot, fight, trade, relog). The world
//! runs exactly as in every scenario test: one `UpdateWorld` iteration per 1/60 s of virtual time,
//! so the 5-minute saves, the heartbeats, the generators and the guid recycler all run on their own
//! schedules.
//!
//! At the end every bot logs off, and the run goes on for a tail (landblocks go dormant and unload)
//! before the last sample.
//!
//! What it measures ([`metrics`]): the wall time of each world iteration and the performance
//! monitor's stages (ACE's `ServerPerformanceMonitor`, on the machine clock), resident memory,
//! object and guid counts, the shard's size, queue lengths, the messages the bots received, and
//! the wall time per virtual hour. What it checks ([`invariants`]): no panic, no guid reused while
//! held, every relogged character back deep-equal, the object count back near its baseline
//! once everyone has left, and no queue growing without bound. There are no absolute ceilings:
//! each run is profiled against its host's baseline ([`profile`], [`probe`]) and fails only on
//! sustained tick overruns.
//!
//! [`udp`] is the other half: a real `empyrean-server` process on loopback with bots over real UDP
//! sockets.
//!
//! Nothing here is an ACE port; there are no ACE anchors.

pub mod bot;
pub use crate::soak_invariants as invariants;
pub mod probe;
pub mod udp;
pub use crate::soak_metrics as metrics;
pub use crate::soak_profile as profile;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use dereth_protocol::comms::CommunicationTalk;
use empyrean_common::clock::SystemClock;
use empyrean_content::PackContent;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_entity::ObjectGuid;
use empyrean_net::{NetConfig, ServerNet};
use empyrean_store::shard_database::CharacterQuery;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::server_performance_monitor::{
    self as perf, CumulativeEventHistoryType, MonitorType,
};
use empyrean_world::managers::{landblock_manager, player_manager};

use crate::decode;
use crate::{ClientId, ClientStatus, TestServer};
use bot::{Bot, BotEvent, BotIo, Rng, State, TradeRole};
use invariants::{Checks, PanicCounter};
use metrics::{Hour, Sample};

/// A soak run's shape.
#[derive(Debug, Clone)]
pub struct SoakConfig {
    /// The run's name (the report's file name).
    pub name: String,
    pub bots: usize,
    /// Virtual hours of play (after the logins, before the log-offs).
    pub hours: f64,
    /// Virtual seconds between two logins.
    pub join_every: f64,
    /// Virtual seconds between samples.
    pub sample_every: f64,
    /// Virtual seconds after the last log-off before the last sample.
    pub tail: f64,
    /// The bots' RNG seed (the world's is `TestServer::SEED`).
    pub seed: u64,
    /// `Network.MaximumAllowedSessions` (ACE's default 128; at most 256, see empyrean-net).
    pub max_sessions: u32,
    /// The chargen start areas the bots are spread over (0 is Holtburg).
    pub start_areas: Vec<u32>,
    /// Each bot's step: how often (virtual seconds) it reads its messages and acts.
    pub bot_step: f64,
    /// Decode every received message with dereth-protocol (costly with many bots).
    pub decode_all: bool,
    /// Where the report (`<name>.md`) and the samples (`<name>.csv`) go.
    pub out_dir: PathBuf,
    /// Server properties (`PropertyManager.ModifyLong`) set before any bot connects, as an operator
    /// would with `@modifylong`.
    pub properties_long: Vec<(&'static str, i64)>,
    /// Every `hunter_every`-th bot (from bot 1) hunts the encounter drudges of `hunting_ground`; the
    /// others stay in the Academy. 0: none.
    pub hunter_every: usize,
    /// The hunters' ground: an outdoor cell and a spot in it. The default is landblock `0xAEB4`,
    /// east of Holtburg, whose 11 encounters spawn drudge skulkers and slinkers that do not attack
    /// first (`-noaggro`), each regenerating 300 s after it dies.
    pub hunting_ground: (u32, f32, f32),
    /// Trades started per idle bot per second of virtual time.
    pub trade_rate: f64,
    /// Every bot plays only this behaviour (for looking at one in isolation).
    pub only: Option<bot::Kind>,
    /// Bots whose behaviour changes are printed.
    pub trace: Vec<usize>,
    /// Run once the server exists, before any bot connects (the test passes
    /// `empyrean_command::command_manager::initialize`, `Program.Main`'s, for the fights' `@create`).
    pub init: Option<fn()>,
    /// The host-relative profile, the baseline and the overrun guard.
    pub profile: ProfileOptions,
    /// Run the shard on its own database thread, as the server does
    /// (`SerializedShardDatabase`), instead of the test server's synchronous shard, which runs each
    /// save inline on the world thread (`SOAK_SHARD=threaded`). For measuring what the saves cost
    /// the world tick; the run is then less repeatable.
    pub threaded_shard: bool,
}

/// Where the run's profile is compared and recorded (no absolute ceilings; see
/// [`profile`]).
#[derive(Debug, Clone)]
pub struct ProfileOptions {
    /// The baselines' directory (`target/soak-baselines`); `None`: no comparison, no recording.
    pub baseline_dir: Option<PathBuf>,
    /// Write this run as its host's baseline for its configuration key (after comparing).
    pub record_baseline: bool,
    /// The run is pinned to this many cores (`SOAK_CORES`): its own baseline key.
    pub cores: Option<usize>,
    pub margins: profile::Margins,
    /// The absolute guard: tick interval (ms), window (ticks), largest share over.
    pub overrun_interval_ms: f64,
    pub overrun_window: usize,
    pub overrun_max_fraction: f64,
}

impl Default for ProfileOptions {
    fn default() -> Self {
        Self {
            baseline_dir: None,
            record_baseline: false,
            cores: None,
            margins: profile::Margins::default(),
            overrun_interval_ms: TestServer::TICK.as_secs_f64() * 1000.0,
            overrun_window: profile::OverrunWindow::DEFAULT_WINDOW,
            overrun_max_fraction: profile::OverrunWindow::DEFAULT_MAX_FRACTION,
        }
    }
}

impl SoakConfig {
    /// `bots` bots for `hours` virtual hours, with the defaults.
    #[must_use]
    pub fn new(name: &str, bots: usize, hours: f64, out_dir: PathBuf) -> Self {
        Self {
            name: name.to_owned(),
            bots,
            hours,
            join_every: 0.5,
            sample_every: 300.0,
            tail: 20.0 * 60.0,
            seed: 0x50A6_0007_0003,
            max_sessions: 128,
            start_areas: vec![0],
            bot_step: 0.1,
            decode_all: false,
            out_dir,
            init: None,
            only: None,
            trace: Vec::new(),
            trade_rate: 0.005,
            hunter_every: 2,
            hunting_ground: (0xAEB4_0025, 96.0, 96.0),
            properties_long: Vec::new(),
            profile: ProfileOptions::default(),
            threaded_shard: false,
        }
    }
}

/// What a run found.
#[derive(Debug, Clone, Default)]
pub struct SoakReport {
    pub name: String,
    pub bots: usize,
    pub hours: f64,
    pub wall_secs: f64,
    pub samples: Vec<Sample>,
    pub hours_measured: Vec<Hour>,
    pub panics: u64,
    pub panic_examples: Vec<String>,
    pub checks_summary: String,
    pub failures: Vec<String>,
    /// Invariant violations (empty: the run passed).
    pub violations: Vec<String>,
    /// Objects before the first login, at the peak, and after everyone left.
    pub objects_baseline: usize,
    pub objects_peak: usize,
    pub objects_end: usize,
    pub landblocks_baseline: usize,
    pub landblocks_end: usize,
    pub rss_start: Option<u64>,
    pub rss_peak: Option<u64>,
    pub rss_end: Option<u64>,
    pub undecoded: u64,
    pub undecoded_examples: Vec<String>,
    /// Summed over the bots.
    pub bot_totals: BTreeMap<String, u64>,
    pub abandoned: BTreeMap<String, u64>,
    pub received_by_kind: BTreeMap<u32, u64>,
    /// An FNV-1a digest of every message every bot received, in order (bot index, opcode,
    /// body). Two runs of the same build and seed must agree, on any host.
    pub trace_digest: u64,
    /// The failures the bots were told of (`WeenieError`, `AttackDone`, `UseDone`), by code.
    pub errors: BTreeMap<String, u64>,
    pub not_ported: BTreeMap<&'static str, u64>,
    /// Objects still in `World.objects` at the end, by what they are (the player and dynamic guid
    /// ranges only), with an example guid.
    pub leftovers: BTreeMap<String, (u64, u32)>,
    pub markdown: String,
    /// The run's profile (also written as `<name>.profile.json` beside the report).
    pub profile: serde_json::Value,
    /// Figures past their margins against this host's baseline (warnings; `--soak-strict` fails
    /// on them).
    pub regressions: Vec<String>,
    /// Whether a baseline for this configuration was found on this host.
    pub baseline_found: bool,
}

impl SoakReport {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }
}

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;

/// `SOAK_TRACE=<file>`: the trace behind `trace_digest`, one message per line (virtual time, bot,
/// opcode, body in hex), to find where two hosts' runs part.
static TRACE: std::sync::OnceLock<Option<std::sync::Mutex<std::io::BufWriter<std::fs::File>>>> =
    std::sync::OnceLock::new();

fn trace_file() -> Option<&'static std::sync::Mutex<std::io::BufWriter<std::fs::File>>> {
    TRACE
        .get_or_init(|| {
            let path = std::env::var_os("SOAK_TRACE")?;
            let file = std::fs::File::create(&path)
                .unwrap_or_else(|e| panic!("SOAK_TRACE {}: {e}", PathBuf::from(&path).display()));
            Some(std::sync::Mutex::new(std::io::BufWriter::new(file)))
        })
        .as_ref()
}

fn trace_line(now: f64, bot: u32, opcode: u32, body: &[u8]) {
    use std::io::Write as _;
    if let Some(w) = trace_file() {
        let mut w = w.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = write!(w, "{now:.4} {bot} {opcode:08x} ");
        for b in body {
            let _ = write!(w, "{b:02x}");
        }
        let _ = writeln!(w);
    }
}

fn trace_flush() {
    use std::io::Write as _;
    if let Some(w) = trace_file() {
        let _ = w
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flush();
    }
}

/// FNV-1a over `bytes`, continuing from `h`.
fn fnv1a(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
    }
    h
}

use crate::EmptyShard;

/// The retail dats (`DERETH_TEST_DAT_DIR`).
///
/// # Panics
/// When they cannot be opened.
#[must_use]
pub fn real_dats() -> Arc<DatManager> {
    let dir = dereth_dat::testing::dat_dir();
    let source = RealDats::open(&dir).unwrap_or_else(|e| {
        panic!(
            "the soak needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
            dir.display()
        )
    });
    DatManager::initialize(Arc::new(source)).expect("the retail dats initialize")
}

/// `world.pack` (`EMPYREAN_TEST_WORLD_PACK`, default `world.pack` in the repository).
///
/// # Panics
/// When it cannot be opened.
#[must_use]
pub fn real_pack() -> PackContent {
    let path = empyrean_common::test_paths::world_pack();
    PackContent::open(&path).unwrap_or_else(|e| {
        panic!(
            "the soak needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
            path.display()
        )
    })
}

/// The account of bot `i`.
#[must_use]
pub fn account(i: usize) -> String {
    format!("soak{i:03}")
}

/// A character name for bot `i`: letters only (ACE refuses digits), unique per bot.
#[must_use]
pub fn character_name(i: usize) -> String {
    let mut n = i;
    let mut tag = String::new();
    for _ in 0..3 {
        tag.insert(
            0,
            char::from(b'a' + u8::try_from(n % 26).expect("a letter")),
        );
        n /= 26;
    }
    let mut chars = tag.chars();
    let first = chars.next().expect("three letters").to_ascii_uppercase();
    format!("Soak {first}{}", chars.as_str())
}

/// One bot with its connection.
#[derive(Debug)]
struct Slot {
    bot: Bot,
    io: BotIo,
    client: Option<ClientId>,
    join_at: f64,
    /// Admin elevation to undo at this time.
    admin_until: Option<f64>,
    /// Waiting for the character to go offline after a log-off request.
    awaiting_offline: bool,
    /// Offline snapshot taken; waiting for the object to leave the world.
    awaiting_release: bool,
    /// Compare with the snapshot at this time (after entering).
    compare_at: Option<f64>,
}

/// Runs a soak. See the module docs.
///
/// # Panics
/// When the real content is missing, or the harness itself fails (never on an invariant: those go
/// into the report).
#[allow(clippy::too_many_lines)]
pub fn run(cfg: &SoakConfig) -> SoakReport {
    let panics = PanicCounter::install();
    let wall0 = Instant::now();
    let dats = real_dats();
    let bots = cfg.bots;
    let mut ts = TestServer::with_setup(dats, |w| {
        w.content = Arc::new(real_pack());
        guid_manager::initialize(w, &mut EmptyShard);
        let mut auth = w.auth.lock();
        for i in 0..bots {
            auth.create_account(
                &account(i),
                "pw",
                AccessLevel::Player,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .expect("an account");
        }
    });
    if cfg.threaded_shard {
        let clock: Arc<dyn empyrean_common::clock::Clock> =
            Arc::<empyrean_common::clock::VirtualClock>::clone(&ts.clock);
        let mut shard =
            empyrean_store::ShardHandle::new(Box::new(empyrean_store::MemShard::new()), clock);
        shard.start();
        ts.world.shard = shard;
    }
    if let Some(init) = cfg.init {
        init();
    }
    for (key, value) in &cfg.properties_long {
        assert!(
            empyrean_world::managers::property_manager::modify_long(&ts.world, key, *value),
            "server property {key}"
        );
    }
    // Network.MaximumAllowedSessions: the world's transport is the one the in-memory wire pumps
    ts.world.net = ServerNet::new(
        NetConfig {
            maximum_allowed_sessions: cfg.max_sessions,
            rng_seed: TestServer::SEED,
            ..NetConfig::default()
        },
        empyrean_world::network::game_messages::game_message::transport_messages(),
    );
    // the performance monitor on the machine's clock: it times the world's stages for real
    perf::use_clock(&mut ts.world, Arc::new(SystemClock::new()));
    perf::start(&mut ts.world);
    // and the landblocks' cumulative sections (`serverperformance cumulative start`): the hot spots
    perf::start_cumulative(&mut ts.world);
    // and each message handler's time; `SOAK_HANDLER_TIMING=0` leaves it off
    if std::env::var("SOAK_HANDLER_TIMING").map_or(true, |v| v != "0") {
        ts.world.sessions.inbound.handler_timing = Some(Vec::new());
    }
    let _ = TestServer::take_not_ported();
    // the hunters' @teleloc, onto the terrain
    let hunting_ground = {
        let (cell, x, y) = cfg.hunting_ground;
        let spot =
            empyrean_entity::Position::from_components(cell, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false);
        let z = empyrean_world::entity::position_extensions::get_terrain_z(&ts.world, &spot);
        (
            cell,
            format!("@teleloc 0x{cell:08X} {x:.1} {y:.1} {:.2}", z + 0.05),
        )
    };
    let terrain = {
        let lb = cfg.hunting_ground.0 & 0xFFFF_0000;
        let mut heights = Vec::with_capacity(193 * 193);
        for i in 0..193u16 {
            for j in 0..193u16 {
                let (x, y) = (f32::from(i).min(191.99), f32::from(j).min(191.99));
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let cell = lb | ((x / 24.0).floor() as u32 * 8 + (y / 24.0).floor() as u32 + 1);
                let p = empyrean_entity::Position::from_components(
                    cell, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false,
                );
                heights.push(empyrean_world::entity::position_extensions::get_terrain_z(
                    &ts.world, &p,
                ));
            }
        }
        Arc::new(bot::Terrain {
            landblock: lb >> 16,
            heights,
        })
    };

    let mut report = SoakReport {
        name: cfg.name.clone(),
        bots,
        hours: cfg.hours,
        trace_digest: FNV_OFFSET,
        ..SoakReport::default()
    };
    let mut checks = Checks::default();
    let mut rng = Rng::new(cfg.seed ^ 0xA5A5);
    let mut slots: Vec<Slot> = (0..bots)
        .map(|i| {
            let area = cfg.start_areas[i % cfg.start_areas.len()];
            #[allow(clippy::cast_precision_loss)]
            let join_at = 1.0 + cfg.join_every * i as f64;
            let mut bot = Bot::new(
                i,
                account(i),
                character_name(i),
                area,
                cfg.seed.wrapping_add(i as u64 * 0x9E37),
            );
            bot.only.clone_from(&cfg.only);
            bot.trace = cfg.trace.contains(&i);
            bot.hunter = cfg.hunter_every > 0 && i % cfg.hunter_every == 1 % cfg.hunter_every;
            bot.hunting_ground = Some(hunting_ground.clone());
            bot.terrain = Some(Arc::clone(&terrain));
            Slot {
                bot,
                io: BotIo::default(),
                client: None,
                join_at,
                admin_until: None,
                awaiting_offline: false,
                awaiting_release: false,
                compare_at: None,
            }
        })
        .collect();

    #[allow(clippy::cast_precision_loss)]
    let play_start = 1.0 + cfg.join_every * bots as f64;
    let play_end = play_start + cfg.hours * 3600.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks_per_bot_step = (cfg.bot_step / TestServer::TICK.as_secs_f64())
        .round()
        .max(1.0) as u64;

    // the baseline: the preloaded world before anyone logs in
    ts.advance(1.0);
    let first = sample(&mut ts, &mut checks, &slots, "baseline", wall0, &panics);
    report.objects_baseline = first.objects;
    report.landblocks_baseline = first.landblocks;
    report.rss_start = first.rss;
    report.samples.push(first);

    let mut tick: u64 = 0;
    let mut next_sample = cfg.sample_every;
    let mut next_hour = 3600.0;
    let mut hour = Hour {
        hour: 1,
        ..Hour::default()
    };
    let mut hour_wall = Instant::now();
    let mut hour_messages_mark = 0u64;
    let mut finishing = false;
    let mut all_done_at: Option<f64> = None;
    let mut undecoded_examples = Vec::new();
    // the per-tick probe, the run's server-tick and step histograms, and the guard
    let mut probe = probe::Probe::new();
    let mut run_tick = metrics::Histogram::default();
    let mut run_step = metrics::Histogram::default();
    let po = &cfg.profile;
    let mut overrun = profile::OverrunWindow::new(
        po.overrun_interval_ms,
        po.overrun_window,
        po.overrun_max_fraction,
    );
    let loop_wall = Instant::now();
    let mut play_wall: (Option<f64>, Option<f64>) = (None, None);
    let mut play_messages: (u64, u64) = (0, 0);
    // where the loop's wall time goes (the server tick, the bots, the soak's own work)
    let mut harness = profile::HarnessTime::default();
    let cpu0 = profile::process_cpu_seconds();

    loop {
        let now = ts.seconds();
        if tick.is_multiple_of(ticks_per_bot_step) {
            if !finishing && now >= play_end {
                finishing = true;
                for s in &mut slots {
                    s.bot.finish = true;
                }
            }
            let tb = Instant::now();
            drive_bots(
                &mut ts,
                &mut slots,
                &mut checks,
                now,
                cfg,
                &mut report,
                &mut undecoded_examples,
            );
            if !finishing && tick.is_multiple_of(ticks_per_bot_step * 10) {
                pair_traders(&mut slots, &mut rng, cfg.trade_rate);
                checks.check_held(&ts.world);
            }
            let bots_s = tb.elapsed().as_secs_f64();
            harness.bots_s += bots_s;
            hour.harness_s += bots_s;
            if finishing
                && all_done_at.is_none()
                && slots.iter().all(|s| s.bot.state == State::Done)
            {
                all_done_at = Some(now);
                let s = sample(
                    &mut ts,
                    &mut checks,
                    &slots,
                    "all logged off",
                    wall0,
                    &panics,
                );
                report.samples.push(s);
            }
            if all_done_at.is_some_and(|t| now >= t + cfg.tail)
                || (finishing && now >= play_end + cfg.tail + 600.0)
            {
                break;
            }
        }

        let t0 = Instant::now();
        ts.step();
        let step_ms = t0.elapsed().as_secs_f64() * 1000.0;
        hour.step.add(step_ms);
        run_step.add(step_ms);
        let tp = Instant::now();
        let tick_ms = probe.after_tick(&mut ts.world);
        run_tick.add(tick_ms);
        hour.tick.add(tick_ms);
        overrun.push(tick_ms);
        let probe_s = tp.elapsed().as_secs_f64();
        harness.step_s += step_ms / 1000.0;
        harness.server_tick_s += tick_ms / 1000.0;
        harness.instrumentation_s += probe_s;
        hour.harness_s += probe_s + (step_ms - tick_ms).max(0.0) / 1000.0;
        tick += 1;
        // the play window, for messages per second
        let now_v = ts.seconds();
        if play_wall.0.is_none() && now_v >= play_start {
            play_wall.0 = Some(loop_wall.elapsed().as_secs_f64());
            play_messages.0 = slots.iter().map(|s| s.bot.stats.received_total).sum();
        }
        if play_wall.1.is_none() && now_v >= play_end {
            play_wall.1 = Some(loop_wall.elapsed().as_secs_f64());
            play_messages.1 = slots.iter().map(|s| s.bot.stats.received_total).sum();
        }

        let now = ts.seconds();
        if now >= next_sample {
            next_sample += cfg.sample_every;
            let phase = if now < play_start {
                "joining"
            } else if !finishing {
                "playing"
            } else if all_done_at.is_none() {
                "logging off"
            } else {
                "tail"
            };
            let ts0 = Instant::now();
            let s = sample(&mut ts, &mut checks, &slots, phase, wall0, &panics);
            let sample_s = ts0.elapsed().as_secs_f64();
            harness.instrumentation_s += sample_s;
            hour.harness_s += sample_s;
            eprintln!(
                "[{}] t={:>6.0}s wall={:>7.1}s objects={} reachable={} lb={} online={} rss={}MB msgs={} panics={} [{}]",
                cfg.name,
                s.t,
                s.wall,
                s.objects,
                s.reachable,
                s.landblocks,
                s.online,
                metrics::mb(s.rss),
                s.messages,
                s.panics,
                s.activities
            );
            report.samples.push(s);
        }
        if now >= next_hour {
            next_hour += 3600.0;
            hour.wall = hour_wall.elapsed().as_secs_f64();
            for m in MonitorType::ALL {
                let h = perf::get_event_history_24h(&ts.world, m);
                hour.stages.push((
                    m.name(),
                    h.total_events,
                    h.average_event_duration() * 1000.0,
                    h.longest_event * 1000.0,
                ));
            }
            for c in CumulativeEventHistoryType::ALL {
                let h = perf::get_cumulative_event_history_24h(&ts.world, c);
                hour.stages.push((
                    c.name(),
                    h.total_events,
                    h.average_event_duration() * 1000.0,
                    h.longest_event * 1000.0,
                ));
            }
            perf::reset(&mut ts.world);
            let total: u64 = slots.iter().map(|s| s.bot.stats.received_total).sum();
            hour.messages = total - hour_messages_mark;
            hour_messages_mark = total;
            eprintln!(
                "[{}] virtual hour {} took {:.1} s wall; step mean {:.3} ms max {:.1} ms",
                cfg.name,
                hour.hour,
                hour.wall,
                hour.step.mean_ms(),
                hour.step.max_ms
            );
            let next = hour.hour + 1;
            report.hours_measured.push(std::mem::replace(
                &mut hour,
                Hour {
                    hour: next,
                    ..Hour::default()
                },
            ));
            hour_wall = Instant::now();
        }
    }
    if hour.step.count > 0 {
        hour.wall = hour_wall.elapsed().as_secs_f64();
        for m in MonitorType::ALL {
            let h = perf::get_event_history_24h(&ts.world, m);
            hour.stages.push((
                m.name(),
                h.total_events,
                h.average_event_duration() * 1000.0,
                h.longest_event * 1000.0,
            ));
        }
        for c in CumulativeEventHistoryType::ALL {
            let h = perf::get_cumulative_event_history_24h(&ts.world, c);
            hour.stages.push((
                c.name(),
                h.total_events,
                h.average_event_duration() * 1000.0,
                h.longest_event * 1000.0,
            ));
        }
        let total: u64 = slots.iter().map(|s| s.bot.stats.received_total).sum();
        hour.messages = total - hour_messages_mark;
        report.hours_measured.push(hour);
    }
    report.leftovers = leftovers(&ts);
    let last = sample(&mut ts, &mut checks, &slots, "end", wall0, &panics);
    report.objects_end = last.objects;
    report.landblocks_end = last.landblocks;
    report.rss_end = last.rss;
    report.samples.push(last);
    report.not_ported = TestServer::take_not_ported();
    trace_flush();
    report.wall_secs = wall0.elapsed().as_secs_f64();
    report.panics = panics.count();
    report.panic_examples = panics
        .examples
        .lock()
        .map(|e| e.clone())
        .unwrap_or_default();
    panics.uninstall();
    report.undecoded_examples = undecoded_examples;
    let play = PlayWindow {
        wall: play_wall,
        messages: play_messages,
        loop_wall: loop_wall.elapsed().as_secs_f64(),
        total_messages: slots.iter().map(|s| s.bot.stats.received_total).sum(),
    };
    harness.loop_s = play.loop_wall;
    harness.process_cpu_s = cpu0.zip(profile::process_cpu_seconds()).map(|(a, b)| b - a);
    let timing = RunTiming {
        tick: run_tick,
        step: run_step,
        overrun,
        probe,
        play,
        harness,
    };
    finish_report(&mut report, &checks, &slots, cfg, &timing);
    report
}

/// The play window's wall seconds (start, end) and the bots' message totals at each.
#[derive(Debug)]
struct PlayWindow {
    wall: (Option<f64>, Option<f64>),
    messages: (u64, u64),
    loop_wall: f64,
    total_messages: u64,
}

impl PlayWindow {
    /// Messages the bots received per wall second of play (the whole loop if play never ended).
    fn msgs_per_s(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        match self.wall {
            (Some(a), Some(b)) if b > a => (self.messages.1 - self.messages.0) as f64 / (b - a),
            _ if self.loop_wall > 0.0 => self.total_messages as f64 / self.loop_wall,
            _ => 0.0,
        }
    }
}

/// What the probe timed over the run.
#[derive(Debug)]
struct RunTiming {
    tick: metrics::Histogram,
    step: metrics::Histogram,
    overrun: profile::OverrunWindow,
    probe: probe::Probe,
    play: PlayWindow,
    /// The loop's wall time by part.
    harness: profile::HarnessTime,
}

/// One step of every bot: connect, read, act, send; the admin elevation for `@create`; the relog
/// invariants.
#[allow(clippy::too_many_lines)]
fn drive_bots(
    ts: &mut TestServer,
    slots: &mut [Slot],
    checks: &mut Checks,
    now: f64,
    cfg: &SoakConfig,
    report: &mut SoakReport,
    undecoded: &mut Vec<String>,
) {
    for s in slots.iter_mut() {
        if s.bot.state == State::Done && s.client.is_none() {
            continue;
        }
        // connect (and reconnect after a lost connection)
        let Some(c) = s.client else {
            if now >= s.join_at && !s.bot.finish {
                s.client = Some(ts.add_client(&s.bot.account, "pw"));
            }
            continue;
        };
        let status = ts.client(c).status();
        if s.bot.state == State::Connecting {
            if status == ClientStatus::Connected {
                s.bot.connected(&mut s.io);
            } else if status == ClientStatus::Done {
                // gave up (20 unanswered login requests): try again in 10 s
                s.bot.disconnected();
                s.client = None;
                s.join_at = now + 10.0;
                continue;
            }
        } else if s.bot.state != State::Done
            && ts.world.net.find_by_account(&s.bot.account).is_none()
            && status == ClientStatus::Connected
        {
            // the server dropped the session
            s.bot.disconnected();
            s.client = None;
            s.join_at = now + 10.0;
            continue;
        }

        let inbox = ts.take_received(c);
        for msg in &inbox {
            let index = u32::try_from(s.bot.index).unwrap_or(u32::MAX);
            for part in [
                &index.to_le_bytes()[..],
                &msg.opcode.to_le_bytes()[..],
                &msg.body[..],
            ] {
                report.trace_digest = fnv1a(report.trace_digest, part);
            }
            trace_line(now, index, msg.opcode, &msg.body);
        }
        if cfg.decode_all {
            for d in decode::undecoded(&inbox) {
                // the two known ACE-vs-client disagreements are not counted
                if matches!(
                    (d.kind, d.result.as_ref().err().map(String::as_str)),
                    (0x01B1, Some("AttackerNotification: 4 bytes left over"))
                        | (0x01B2, Some("DefenderNotification: 4 bytes left over"))
                ) {
                    continue;
                }
                report.undecoded += 1;
                if undecoded.len() < 20 {
                    undecoded.push(format!("0x{:04X} {}: {:?}", d.kind, d.name(), d.result));
                }
            }
        }
        s.bot.receive(now, &inbox);
        drop(inbox);
        s.bot.update(now, &mut s.io);

        // an admin for one chat command (as the playable-loop scenario's `admin_command` does):
        // the session's access level and the character's IsAdmin, as an Admin account's login sets
        // them, then back
        if let Some(line) = s.io.admin.take() {
            if let (Some(session), Some(me)) =
                (ts.world.net.find_by_account(&s.bot.account), s.bot.me)
            {
                if let (Some(sd), true) = (
                    ts.world.sessions.get_mut(session),
                    ts.world.objects.contains(ObjectGuid::new(me)),
                ) {
                    sd.access_level = AccessLevel::Admin;
                    if let Some(o) = ts.world.objects.get_mut(ObjectGuid::new(me)) {
                        o.set_is_admin_prop(true);
                    }
                    s.io.action(&CommunicationTalk { message: line });
                    s.admin_until = Some(now + 1.0);
                }
            }
        }
        if s.admin_until.is_some_and(|t| now >= t) {
            s.admin_until = None;
            if let Some(session) = ts.world.net.find_by_account(&s.bot.account) {
                if let Some(sd) = ts.world.sessions.get_mut(session) {
                    sd.access_level = AccessLevel::Player;
                }
            }
            if let Some(o) = s
                .bot
                .me
                .and_then(|me| ts.world.objects.get_mut(ObjectGuid::new(me)))
            {
                o.set_is_admin_prop(false);
            }
        }

        for (queue, bytes) in s.io.out.drain(..) {
            ts.client_mut(c).send(queue, &bytes);
        }
        if s.bot.trace && (now % 30.0) < cfg.bot_step {
            trace_world(ts, &s.bot, now);
        }

        // the relog invariants
        for e in s.bot.events.drain(..) {
            let Some(me) = s.bot.me else { continue };
            match e {
                BotEvent::LoggingOff => s.awaiting_offline = true,
                BotEvent::Entering => checks.on_entering(me),
                // compared once the server has had the LoginComplete (portal space sets
                // IgnoreCollisions until then) and before the bot acts (it idles at least 1 s)
                BotEvent::Entered => {
                    if checks.has_snapshot(me) {
                        s.compare_at = Some(now + 0.5);
                    }
                }
                BotEvent::LoggedOff => {}
            }
        }
        if let Some(me) = s.bot.me {
            let g = ObjectGuid::new(me);
            if s.awaiting_offline && player_manager::get_online_player(&ts.world, me).is_none() {
                s.awaiting_offline = false;
                s.awaiting_release = true;
            }
            if s.awaiting_release {
                if ts.world.objects.contains(g) {
                    checks.track(&ts.world, me);
                } else {
                    s.awaiting_release = false;
                    checks.on_released(&ts.world, me);
                }
            }
            if s.compare_at.is_some_and(|t| now >= t) {
                s.compare_at = None;
                checks.on_entered(&ts.world, me);
            }
        }
        if s.bot.state == State::Done && s.bot.finish {
            // the run is over for this bot: disconnect the transport too
            ts.client_mut(c).log_off();
            s.client = None;
        }
    }
}

/// For a traced bot: the drudges its landblock really holds, and whether the bot knows of them.
fn trace_world(ts: &TestServer, bot: &Bot, now: f64) {
    let Some(me) = bot.me else { return };
    let Some(lb) = ts
        .world
        .objects
        .get(ObjectGuid::new(me))
        .and_then(|o| o.current_landblock)
    else {
        return;
    };
    let Some(l) = ts.world.landblock_manager.landblocks.get(lb) else {
        return;
    };
    let drudges: Vec<String> = l
        .get_all_world_objects_for_diagnostics()
        .into_iter()
        .filter_map(|g| ts.world.objects.get(g).map(|o| (g, o)))
        .filter(|(_, o)| {
            o.get_property(empyrean_entity::enums::PropertyString::Name)
                .is_some_and(|n| n.starts_with("Drudge"))
        })
        .map(|(g, o)| {
            format!(
                "{:08X} at {:?} known {}",
                g.full(),
                o.location().map(|p| (p.position_x, p.position_y)),
                bot.knows(g.full())
            )
        })
        .collect();
    eprintln!(
        "bot {} t={now:.0} world drudges in its landblock: {drudges:?}",
        bot.index
    );
}

/// Now and then (`trade_rate` per idle bot per second) an idle bot trades with another idle bot it
/// can see.
fn pair_traders(slots: &mut [Slot], rng: &mut Rng, trade_rate: f64) {
    let idle: Vec<usize> = slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.bot.is_idle_in_world() && s.bot.me.is_some())
        .map(|(i, _)| i)
        .collect();
    let mut taken = std::collections::HashSet::new();
    for &a in &idle {
        if taken.contains(&a) || rng.unit() >= trade_rate {
            continue;
        }
        let Some(&b) = idle.iter().find(|&&b| {
            b != a && !taken.contains(&b) && slots[a].bot.knows(slots[b].bot.me.unwrap_or(0))
        }) else {
            continue;
        };
        taken.insert(a);
        taken.insert(b);
        let (ga, gb) = (slots[a].bot.me.unwrap_or(0), slots[b].bot.me.unwrap_or(0));
        slots[a].bot.trade_with = Some((gb, TradeRole::Initiator));
        slots[b].bot.trade_with = Some((ga, TradeRole::Responder));
    }
}

/// What is still in `World.objects`: every guid the player and dynamic allocators have handed out
/// is probed (the store has no iteration API by design).
fn leftovers(ts: &TestServer) -> BTreeMap<String, (u64, u32)> {
    let w = &ts.world;
    let mut out: BTreeMap<String, (u64, u32)> = BTreeMap::new();
    let ranges = [
        (
            empyrean_entity::ObjectGuid::PLAYER_MIN,
            guid_manager::player_current(w),
        ),
        (
            empyrean_entity::ObjectGuid::DYNAMIC_MIN,
            w.guid_manager
                .dynamic_alloc
                .as_ref()
                .map_or(0, |d| d.current()),
        ),
    ];
    for (lo, hi) in ranges {
        for g in lo..=hi.max(lo) {
            if let Some(o) = w.objects.get(ObjectGuid::new(g)) {
                let what = format!(
                    "{} (weenie {}{}{})",
                    o.get_property(empyrean_entity::enums::PropertyString::Name)
                        .unwrap_or_default(),
                    o.biota.weenie_class_id,
                    if o.container_id().is_some() {
                        ", in a container"
                    } else {
                        ""
                    },
                    if o.current_landblock.is_some() {
                        ", on a landblock"
                    } else {
                        ""
                    }
                );
                let e = out.entry(what).or_insert((0, g));
                e.0 += 1;
            }
        }
    }
    out
}

/// Samples the world (and scans it for the guid invariant).
fn sample(
    ts: &mut TestServer,
    checks: &mut Checks,
    slots: &[Slot],
    phase: &'static str,
    wall0: Instant,
    panics: &PanicCounter,
) -> Sample {
    let reachable = checks.scan(&ts.world);
    checks.check_held(&ts.world);
    let w = &mut ts.world;
    let loaded = landblock_manager::get_loaded_landblocks(w);
    let mut landblock_actions = 0;
    for lb in &loaded {
        if let Some(l) = w.landblock_manager.landblocks.get_mut(*lb) {
            landblock_actions += l.action_queue_mut().len();
        }
    }
    let (guid_current, guid_recycled_pending) = w
        .guid_manager
        .dynamic_alloc
        .as_ref()
        .map_or((0, 0), |d| (d.current(), d.recycled_guids_total()));
    let (shard_biotas, shard_characters) = {
        let mut db = w.shard.base_database();
        (
            db.count_biotas().unwrap_or(-1),
            db.query_characters(CharacterQuery::NotDeleted)
                .map_or(0, |c| c.len()),
        )
    };
    let mut activities: BTreeMap<&str, usize> = BTreeMap::new();
    for s in slots {
        let a = match s.bot.state {
            State::InWorld => s.bot.activity_name(),
            State::Done => "done",
            State::CharacterSelect { .. } | State::LoggingOff { .. } => "off",
            _ => "login",
        };
        *activities.entry(a).or_insert(0) += 1;
    }
    Sample {
        t: ts.seconds(),
        wall: wall0.elapsed().as_secs_f64(),
        phase,
        objects: ts.world.objects.len(),
        reachable,
        landblocks: loaded.len(),
        online: player_manager::get_all_online(&ts.world).len(),
        sessions: ts.world.net.get_session_count(),
        rss: metrics::resident_bytes(),
        shard_biotas,
        shard_characters,
        guid_current,
        guid_recycled_pending,
        world_actions: ts.world.world_manager.action_queue.len(),
        delays: ts.world.world_manager.delay_manager.len(),
        inbound: ts.world.sessions.inbound.inbound_message_queue.len(),
        landblock_actions,
        shard_queue: ts.world.shard.queue_count(),
        messages: slots.iter().map(|s| s.bot.stats.received_total).sum(),
        bot_known: slots.iter().map(|s| s.bot.known_count()).sum(),
        panics: panics.count(),
        activities: activities
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// A queue series grows without bound if its last third averages more than twice its middle
/// third and by more than 100.
fn grows(series: &[f64]) -> Option<(f64, f64)> {
    let n = series.len();
    if n < 6 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = |s: &[f64]| s.iter().sum::<f64>() / s.len().max(1) as f64;
    let mid = mean(&series[n / 3..2 * n / 3]);
    let last = mean(&series[2 * n / 3..]);
    (last > 2.0 * mid && last - mid > 100.0).then_some((mid, last))
}

#[allow(clippy::too_many_lines)]
fn finish_report(
    r: &mut SoakReport,
    checks: &Checks,
    slots: &[Slot],
    cfg: &SoakConfig,
    timing: &RunTiming,
) {
    // bot totals
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for s in slots {
        let st = &s.bot.stats;
        for (k, v) in [
            ("kills", st.kills),
            ("deaths", st.deaths),
            ("items looted", st.items_looted),
            ("food eaten", st.food_eaten),
            ("trades completed", st.trades_completed),
            ("speech heard", st.speech_heard),
            ("logins", st.logins),
            ("reconnects", st.reconnects),
            ("messages received", st.received_total),
            ("messages sent", st.sent_total),
        ] {
            *totals.entry(k.to_owned()).or_insert(0) += v;
        }
        for (k, v) in &st.started {
            *totals.entry(format!("started {k}")).or_insert(0) += v;
        }
        for (k, v) in &st.completed {
            *totals.entry(format!("completed {k}")).or_insert(0) += v;
        }
        for (k, v) in &st.abandoned {
            *r.abandoned.entry(k.clone()).or_insert(0) += v;
        }
        for (k, v) in &st.received {
            *r.received_by_kind.entry(*k).or_insert(0) += v;
        }
        for (k, v) in &st.weenie_errors {
            *r.errors
                .entry(format!("WeenieError 0x{k:04X}"))
                .or_insert(0) += v;
        }
        for (k, v) in &st.attack_errors {
            *r.errors.entry(format!("AttackDone 0x{k:04X}")).or_insert(0) += v;
        }
        for (k, v) in &st.use_errors {
            *r.errors.entry(format!("UseDone 0x{k:04X}")).or_insert(0) += v;
        }
    }
    r.bot_totals = totals;
    r.failures.clone_from(&checks.failures);
    r.objects_peak = r.samples.iter().map(|s| s.objects).max().unwrap_or(0);
    r.rss_peak = r.samples.iter().filter_map(|s| s.rss).max();

    // the invariants
    if r.panics > 0 {
        r.violations.push(format!("{} panics", r.panics));
    }
    if checks.guid_reuse_while_held > 0 {
        r.violations.push(format!(
            "{} guids reused while held",
            checks.guid_reuse_while_held
        ));
    }
    if checks.saved_mismatch > 0 || checks.reload_mismatch > 0 || checks.shard_orphans > 0 {
        r.violations.push(format!(
            "relog deep-equal: {} log-off saves differ from the world, {} re-entries differ, {} orphaned shard possessions",
            checks.saved_mismatch, checks.reload_mismatch, checks.shard_orphans
        ));
    }
    let stuck: Vec<usize> = slots
        .iter()
        .filter(|s| s.bot.state != State::Done)
        .map(|s| s.bot.index)
        .collect();
    if !stuck.is_empty() {
        r.violations
            .push(format!("bots never logged off: {stuck:?}"));
    }
    let end_online = r.samples.last().map_or(0, |s| s.online);
    if end_online > 0 {
        r.violations
            .push(format!("{end_online} players still online after the tail"));
    }
    #[allow(clippy::cast_precision_loss)]
    let near = (r.objects_baseline as f64 * 1.25).max(r.objects_baseline as f64 + 100.0);
    #[allow(clippy::cast_precision_loss)]
    if r.objects_end as f64 > near {
        r.violations.push(format!(
            "objects did not return near the baseline: {} at start, {} peak, {} after the tail",
            r.objects_baseline, r.objects_peak, r.objects_end
        ));
    }
    let playing: Vec<&Sample> = r.samples.iter().filter(|s| s.phase == "playing").collect();
    #[allow(clippy::cast_precision_loss)]
    for (name, f) in [
        (
            "world action queue",
            (|s: &Sample| s.world_actions as f64) as fn(&Sample) -> f64,
        ),
        ("delay manager", |s: &Sample| s.delays as f64),
        ("inbound message queue", |s: &Sample| s.inbound as f64),
        ("landblock action queues", |s: &Sample| {
            s.landblock_actions as f64
        }),
        ("shard queue", |s: &Sample| f64::from(s.shard_queue)),
        ("sessions", |s: &Sample| s.sessions as f64),
    ] {
        let series: Vec<f64> = playing.iter().map(|s| f(s)).collect();
        if let Some((mid, last)) = grows(&series) {
            r.violations.push(format!(
                "{name} grows without bound: middle third mean {mid:.0}, last third {last:.0}"
            ));
        }
    }

    // the one absolute guard: sustained tick overruns are player-visible on any host
    let o = &timing.overrun;
    if o.breached() {
        r.violations.push(format!(
            "tick overrun: {} of {} consecutive ticks ({:.1}%, limit {:.0}%) took longer than {:.2} ms, the window ending at tick {}",
            o.worst,
            o.window,
            o.worst_fraction() * 100.0,
            o.max_fraction * 100.0,
            o.interval_ms,
            o.worst_end
        ));
    }
    let profile_md = build_profile(r, cfg, timing);

    r.checks_summary = format!(
        "relogs: {} last states in the world equal the shard at release ({} differ, {} missed, {} changed after the driver's last snapshot, {} with a vital moved after the log-off save, {} changed after it by a death or more, an ACE bug; {} with a NaN orientation); {} re-entries deep-equal ({} differ; {} back as at the offline switch, the stale OfflinePlayer copy; {} displaced by the physics placement, {} relocated to the Sanctuary, {} with a spent vitae dispelled at login); {} orphaned shard possessions; guids: {} scans (max {} reachable objects), {} recycled, {} reused while held",
        checks.saved_ok,
        checks.saved_mismatch,
        checks.snapshot_missed,
        checks.changed_after_last_snapshot,
        checks.vitals_after_save,
        checks.changed_after_save,
        checks.nan_orientation,
        checks.reload_ok,
        checks.reload_mismatch,
        checks.reload_stale_offline_biota,
        checks.reload_displaced,
        checks.reload_relocated,
        checks.reload_vitae_dispelled,
        checks.shard_orphans,
        checks.scans,
        checks.scanned_objects_max,
        checks.guid_recycles,
        checks.guid_reuse_while_held
    );

    // Markdown
    let mut m = String::new();
    let _ = writeln!(
        m,
        "# Soak run `{}`: {} bots, {} virtual hours\n",
        r.name, r.bots, r.hours
    );
    let _ = writeln!(
        m,
        "- result: **{}**",
        if r.passed() {
            "invariants held"
        } else {
            "INVARIANTS VIOLATED"
        }
    );
    for v in &r.violations {
        let _ = writeln!(m, "  - {v}");
    }
    if !cfg.properties_long.is_empty() {
        let _ = writeln!(m, "- server properties set: {:?}", cfg.properties_long);
    }
    #[allow(clippy::cast_precision_loss)]
    let _ = writeln!(
        m,
        "- wall time {:.0} s ({:.1} s per virtual hour of play)",
        r.wall_secs,
        r.wall_secs / r.hours.max(0.01)
    );
    let _ = writeln!(
        m,
        "- objects: baseline {}, peak {}, end {}; landblocks {} -> {}",
        r.objects_baseline, r.objects_peak, r.objects_end, r.landblocks_baseline, r.landblocks_end
    );
    let _ = writeln!(
        m,
        "- resident memory: start {} MB, peak {} MB, end {} MB",
        metrics::mb(r.rss_start),
        metrics::mb(r.rss_peak),
        metrics::mb(r.rss_end)
    );
    let _ = writeln!(m, "- panics: {}", r.panics);
    let _ = writeln!(m, "- message trace digest: {:016x}", r.trace_digest);
    for p in r.panic_examples.iter().take(5) {
        let _ = writeln!(m, "  - `{}`", p.replace('\n', " "));
    }
    let _ = writeln!(m, "- {}", r.checks_summary);
    if cfg.decode_all {
        let _ = writeln!(m, "- undecodable messages: {}", r.undecoded);
    }
    let _ = writeln!(
        m,
        "\n## Per virtual hour\n\n{}",
        metrics::hours_table(&r.hours_measured, r.bots)
    );
    let _ = writeln!(
        m,
        "\n## Hot spots (the whole run)\n\n{}",
        metrics::hot_spots_table(&r.hours_measured)
    );
    m.push_str(&profile_md);
    let _ = writeln!(m, "## Bot totals\n");
    for (k, v) in &r.bot_totals {
        let _ = writeln!(m, "- {k}: {v}");
    }
    let _ = writeln!(m, "\n## Behaviours abandoned\n");
    for (k, v) in &r.abandoned {
        let _ = writeln!(m, "- {k}: {v}");
    }
    let _ = writeln!(m, "\n## Failures the bots were told of\n");
    for (k, v) in &r.errors {
        let _ = writeln!(m, "- {k}: {v}");
    }
    let _ = writeln!(m, "\n## Messages received, top kinds\n");
    let mut kinds: Vec<(&u32, &u64)> = r.received_by_kind.iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in kinds.iter().take(15) {
        let name = dereth_protocol::Opcode(**k).info().map_or("?", |i| i.name);
        let _ = writeln!(m, "- 0x{k:04X} {name}: {v}");
    }
    let _ = writeln!(m, "\n## Invariant failures (first {})\n", r.failures.len());
    for f in &r.failures {
        let _ = writeln!(m, "- {f}");
    }
    let _ = writeln!(
        m,
        "\n## Objects left in the world at the end ({} kinds)\n",
        r.leftovers.len()
    );
    let mut left: Vec<_> = r.leftovers.iter().collect();
    left.sort_by_key(|e| std::cmp::Reverse(e.1 .0));
    for (k, (n, g)) in left.iter().take(25) {
        let _ = writeln!(m, "- {n} x {k}, e.g. {g:08X}");
    }
    let _ = writeln!(
        m,
        "\n## Stale OfflinePlayer biota (first {})\n",
        checks.stale_examples.len()
    );
    for f in &checks.stale_examples {
        let _ = writeln!(m, "- {f}");
    }
    let _ = writeln!(
        m,
        "\n## Changed after the log-off save (first {})\n",
        checks.changed_after_save_examples.len()
    );
    for f in &checks.changed_after_save_examples {
        let _ = writeln!(m, "- {f}");
    }
    let _ = writeln!(m, "\n## not_ported! sites reached\n");
    let mut np: Vec<_> = r.not_ported.iter().collect();
    np.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in np {
        let _ = writeln!(m, "- {v} {k}");
    }
    let _ = writeln!(m, "\n## Samples\n\n```\n{}", Sample::CSV_HEADER);
    for s in &r.samples {
        let _ = writeln!(m, "{}", s.csv());
    }
    let _ = writeln!(m, "```");
    r.markdown = m;
    if std::fs::create_dir_all(&cfg.out_dir).is_ok() {
        let _ = std::fs::write(cfg.out_dir.join(format!("{}.md", r.name)), &r.markdown);
        if let Ok(text) = serde_json::to_string_pretty(&r.profile) {
            let _ = std::fs::write(
                cfg.out_dir.join(format!("{}.profile.json", r.name)),
                text + "\n",
            );
        }
    }
}

fn round_to(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

/// A histogram's figures for the profile.
fn histogram_json(h: &metrics::Histogram) -> serde_json::Value {
    serde_json::json!({ "count": h.count, "mean_ms": round_to(h.mean_ms(), 3), "p50_ms": h.quantile_ms(0.5), "p99_ms": h.quantile_ms(0.99), "max_ms": round_to(h.max_ms, 3) })
}

/// Rows of timed systems or landblocks, as a Markdown table.
fn timed_table(title: &str, v: &[profile::Timed]) -> String {
    let mut s = format!(
        "\n| {title} | ticks | total ms | p50 ms | p99 ms | max ms |\n|---|---|---|---|---|---|\n"
    );
    for x in v {
        let _ = writeln!(
            s,
            "| {} | {} | {:.1} | {:.3} | {:.3} | {:.1} |",
            x.name, x.count, x.total_ms, x.p50_ms, x.p99_ms, x.max_ms
        );
    }
    s
}

/// Builds the run's profile into `r.profile`, compares it with this host's baseline
/// (flags go to `r.regressions`), records it when asked, and returns its Markdown section.
#[allow(clippy::too_many_lines)]
fn build_profile(r: &mut SoakReport, cfg: &SoakConfig, t: &RunTiming) -> String {
    use serde_json::{json, Value};
    let po = &cfg.profile;
    let host = profile::Host::detect();
    let build = profile::Build::detect();
    let key = profile::config_key(&r.name, build.profile, po.cores);
    let affinity = profile::process_affinity();
    #[allow(clippy::cast_precision_loss)]
    let mb = |b: Option<u64>| b.map(|b| round_to(b as f64 / 1_048_576.0, 1));
    // resident memory at the end of play (the last "playing" sample), against the start
    let rss_end_play = r
        .samples
        .iter()
        .rev()
        .find(|s| s.phase == "playing")
        .and_then(|s| s.rss)
        .or(r.rss_peak);
    #[allow(clippy::cast_precision_loss)]
    let bot_hours = r.bots as f64 * r.hours;
    let growth = match (mb(r.rss_start), mb(rss_end_play)) {
        (Some(a), Some(b)) if bot_hours > 0.0 => round_to((b - a) / bot_hours, 3),
        _ => 0.0,
    };
    let msgs_per_s = round_to(t.play.msgs_per_s(), 1);
    let (sys_total, sys_p99) = profile::top10(&t.probe.systems());
    let (lb_total, lb_p99) = profile::top10(&t.probe.landblocks());
    let (handler_total, handler_p99) = profile::top10(&t.probe.handlers());
    let o = &t.overrun;
    let mut tick = histogram_json(&t.tick);
    if let Some(obj) = tick.as_object_mut() {
        obj.insert("interval_ms".into(), json!(round_to(o.interval_ms, 3)));
        obj.insert("over_count".into(), json!(o.over));
        obj.insert("over_ms".into(), json!(round_to(o.over_ms, 3)));
        obj.insert("over_fraction".into(), json!(o.over_fraction()));
        obj.insert("window_ticks".into(), json!(o.window));
        obj.insert("window_limit".into(), json!(o.max_fraction));
        obj.insert("worst_window_over".into(), json!(o.worst));
        obj.insert("worst_window_fraction".into(), json!(o.worst_fraction()));
        obj.insert("overrun_breached".into(), json!(o.breached()));
    }
    let list = |v: &[profile::Timed]| v.iter().map(profile::Timed::to_json).collect::<Vec<_>>();
    let landblock_note = if t.probe.landblock_timed {
        "timed (Landblock.Monitor5m)"
    } else {
        "unavailable: the landblocks' Monitor5m read the tick's frozen clock (timing it needs an empyrean-world change)"
    };
    let mut profile_json = json!({
        "format": profile::PROFILE_FORMAT,
        "host": host.to_json(),
        "build": build.to_json(),
        "run": {
            "key": key,
            "name": r.name,
            "bots": r.bots,
            "hours": r.hours,
            "cores": po.cores,
            "affinity": affinity,
            "wall_s": round_to(r.wall_secs, 1),
            "invariants_held": r.violations.is_empty(),
        },
        "tick": tick,
        "step": histogram_json(&t.step),
        "memory": {
            "rss_start_mb": mb(r.rss_start),
            "rss_end_play_mb": mb(rss_end_play),
            "rss_peak_mb": mb(r.rss_peak),
            "rss_end_mb": mb(r.rss_end),
            "growth_mb_per_bot_hour": growth,
        },
        "messages": { "total": t.play.total_messages, "per_s": msgs_per_s },
        "harness": t.harness.to_json(),
        "systems_by_total": list(&sys_total),
        "systems_by_p99": list(&sys_p99),
        "landblocks_by_total": list(&lb_total),
        "landblocks_by_p99": list(&lb_p99),
        "landblock_timing": landblock_note,
        "handlers_by_total": list(&handler_total),
        "handlers_by_p99": list(&handler_p99),
        "handler_timing": t.probe.handler_timed,
    });

    let mut m = String::new();
    let _ = writeln!(m, "\n## Profile (host-relative)\n");
    let _ = writeln!(
        m,
        "- host `{}`: {} {}, {} logical cores{}, {} MB RAM, {}; build {} {}{}; key `{key}`",
        host.machine_hash,
        host.os,
        host.arch,
        host.logical_cores,
        po.cores.map_or_else(String::new, |n| format!(
            " (pinned to {n}: affinity {})",
            affinity.as_deref().unwrap_or("not pinned")
        )),
        host.ram_mb
            .map_or_else(|| "?".to_owned(), |v| v.to_string()),
        host.cpu.as_deref().unwrap_or("CPU ?"),
        build.commit.as_deref().unwrap_or("?"),
        build.profile,
        if build.dirty { " (dirty)" } else { "" }
    );
    let _ = writeln!(
        m,
        "- server tick (the six UpdateWorld stages): p50 {:.3} / p99 {:.3} / max {:.1} ms; {} of {} over {:.2} ms ({:.3}%); worst {}-tick window {:.2}% (limit {:.0}%)",
        t.tick.quantile_ms(0.5),
        t.tick.quantile_ms(0.99),
        t.tick.max_ms,
        o.over,
        o.ticks,
        o.interval_ms,
        o.over_fraction() * 100.0,
        o.window,
        o.worst_fraction() * 100.0,
        o.max_fraction * 100.0
    );
    let _ = writeln!(m, "- memory growth {growth:.2} MB per bot-hour; {msgs_per_s:.0} messages per wall second of play");
    let _ = writeln!(m, "- {}", t.harness.line());

    // the comparison with this host's baseline for this configuration
    let base_path = po
        .baseline_dir
        .as_ref()
        .map(|d| profile::BaselineFile::path(d, &host.machine_hash));
    let mut file = base_path
        .as_ref()
        .and_then(|p| profile::BaselineFile::read(p));
    let base_entry = file.as_ref().and_then(|f| f.entries.get(&key)).cloned();
    let mut comparison = json!({ "baseline": "none" });
    match (
        base_entry.as_ref().and_then(profile::Headline::from_json),
        profile::Headline::from_json(&profile_json),
    ) {
        (Some(base), Some(cur)) => {
            r.baseline_found = true;
            let findings = profile::compare(&base, &cur, &po.margins);
            let _ = writeln!(
                m,
                "\n### Against this host's baseline (commit {})\n\n{}",
                base_entry
                    .as_ref()
                    .and_then(|b| b.pointer("/build/commit"))
                    .and_then(Value::as_str)
                    .unwrap_or("?"),
                profile::findings_table(&findings)
            );
            for f in findings.iter().filter(|f| f.regressed) {
                r.regressions.push(format!(
                    "{}: {:.3} -> {:.3} ({:+.1}%, allowed {:+.0}%)",
                    f.metric,
                    f.baseline,
                    f.current,
                    f.change * 100.0,
                    f.allowed * 100.0
                ));
            }
            if r.regressions.is_empty() {
                let _ = writeln!(m, "- **no regression** against the baseline");
            } else {
                let _ = writeln!(
                    m,
                    "- **REGRESSIONS** (warnings; `--soak-strict` fails the run):"
                );
                for x in &r.regressions {
                    let _ = writeln!(m, "  - {x}");
                }
            }
            // the systems' p99 against the baseline's (for reading; not flagged)
            if let Some(base_sys) = base_entry
                .as_ref()
                .and_then(|b| b.get("systems_by_p99"))
                .and_then(Value::as_array)
            {
                let _ = writeln!(
                    m,
                    "\n| system (by p99) | baseline p99 ms | this run p99 ms |\n|---|---|---|"
                );
                for s in &sys_p99 {
                    let b = base_sys
                        .iter()
                        .find(|b| b.get("name").and_then(Value::as_str) == Some(s.name.as_str()))
                        .and_then(|b| b.get("p99_ms"))
                        .and_then(Value::as_f64);
                    let _ = writeln!(
                        m,
                        "| {} | {} | {:.3} |",
                        s.name,
                        b.map_or_else(|| "-".to_owned(), |b| format!("{b:.3}")),
                        s.p99_ms
                    );
                }
            }
            comparison = json!({
                "baseline": "found",
                "regressions": r.regressions,
                "findings": findings.iter().map(|f| json!({ "metric": f.metric, "baseline": f.baseline, "current": f.current, "change": f.change, "allowed": f.allowed, "regressed": f.regressed })).collect::<Vec<_>>(),
            });
        }
        _ => {
            let _ = writeln!(
                m,
                "- no baseline for `{key}` on this host{}",
                if po.baseline_dir.is_none() {
                    " (no baseline directory)"
                } else {
                    ""
                }
            );
        }
    }
    // a pinned run beside the same host's unpinned baseline: what degrades first (for reading)
    if po.cores.is_some() {
        let unpinned = profile::config_key(&r.name, build.profile, None);
        let entry = file.as_ref().and_then(|f| f.entries.get(&unpinned));
        if let (Some(base), Some(cur)) = (
            entry.and_then(profile::Headline::from_json),
            profile::Headline::from_json(&profile_json),
        ) {
            let _ = writeln!(m, "\n### Beside this host's unpinned baseline `{unpinned}` (for reading; not flagged)\n\n{}", profile::findings_table(&profile::compare(&base, &cur, &po.margins)));
            if let Some(base_sys) = entry
                .and_then(|b| b.get("systems_by_p99"))
                .and_then(Value::as_array)
            {
                let _ = writeln!(
                    m,
                    "| system (by p99) | unpinned p99 ms | pinned p99 ms |\n|---|---|---|"
                );
                for s in &sys_p99 {
                    let b = base_sys
                        .iter()
                        .find(|b| b.get("name").and_then(Value::as_str) == Some(s.name.as_str()))
                        .and_then(|b| b.get("p99_ms"))
                        .and_then(Value::as_f64);
                    let _ = writeln!(
                        m,
                        "| {} | {} | {:.3} |",
                        s.name,
                        b.map_or_else(|| "-".to_owned(), |b| format!("{b:.3}")),
                        s.p99_ms
                    );
                }
            }
        }
    }
    if let Some(obj) = profile_json.as_object_mut() {
        obj.insert("comparison".into(), comparison);
    }
    if po.record_baseline {
        if let Some(path) = &base_path {
            m.push('\n');
            let f = file.get_or_insert_with(profile::BaselineFile::default);
            f.host = host.to_json();
            let mut entry = profile_json.clone();
            if let Some(obj) = entry.as_object_mut() {
                obj.remove("comparison");
            }
            f.entries.insert(key.clone(), entry);
            match f.write(path) {
                Ok(()) => {
                    let _ = writeln!(
                        m,
                        "- recorded as this host's baseline for `{key}`: `{}`",
                        path.display()
                    );
                }
                Err(e) => {
                    let _ = writeln!(
                        m,
                        "- could not record the baseline at `{}`: {e}",
                        path.display()
                    );
                }
            }
        }
    }

    m.push_str(&timed_table("slowest systems by total", &sys_total));
    m.push_str(&timed_table("slowest systems by p99", &sys_p99));
    if t.probe.landblock_timed {
        m.push_str(&timed_table("slowest landblocks by total", &lb_total));
        m.push_str(&timed_table("slowest landblocks by p99", &lb_p99));
    } else {
        let _ = writeln!(m, "\n- landblocks: {landblock_note}");
    }
    if t.probe.handler_timed {
        m.push_str(&timed_table(
            "slowest message handlers by total (a GameAction nests in its GameMessage)",
            &handler_total,
        ));
        m.push_str(&timed_table(
            "slowest message handlers by p99",
            &handler_p99,
        ));
    } else {
        let _ = writeln!(m, "\n- message handlers: not timed (SOAK_HANDLER_TIMING=0)");
    }
    r.profile = profile_json;
    m
}
