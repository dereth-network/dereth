//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! (feature soak) Soak runs (10..200 bots, minutes to 24 h, UDP smoke, corpse limit) hold
//! invariants and do not overrun the tick for a sustained window, compared with a host baseline.
//! Fixture: virtual-time multi-client runs using retail dats and world.pack.

pub(crate) use std::path::PathBuf;

pub(crate) use empyrean_testkit::soak::{self, udp, SoakConfig};

pub(crate) fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("soak")
}

/// `target/soak-baselines` (the target directory is `CARGO_TARGET_TMPDIR`'s parent).
pub(crate) fn baseline_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SOAK_BASELINE_DIR") {
        return PathBuf::from(d);
    }
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    tmp.parent()
        .map_or_else(|| tmp.join("soak-baselines"), |t| t.join("soak-baselines"))
}

pub(crate) fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

pub(crate) fn soak(name: &str, bots: usize, hours: f64, tweak: impl FnOnce(&mut SoakConfig)) {
    let mut cfg = SoakConfig::new(name, bots, hours, out_dir());
    cfg.init = Some(|| empyrean_command::command_manager::initialize(None));
    cfg.profile.baseline_dir = Some(baseline_dir());
    cfg.profile.record_baseline = env_flag("SOAK_RECORD_BASELINE");
    cfg.profile.cores = std::env::var("SOAK_CORES")
        .ok()
        .and_then(|n| n.parse().ok())
        .filter(|n| *n > 0);
    cfg.profile.margins = soak::profile::Margins::from_env();
    tweak(&mut cfg);
    // a shorter run of the same shape, for trying changes: SOAK_HOURS=0.5
    if let Some(h) = std::env::var("SOAK_HOURS")
        .ok()
        .and_then(|h| h.parse().ok())
    {
        cfg.hours = h;
    }
    cfg.threaded_shard = std::env::var("SOAK_SHARD").is_ok_and(|v| v == "threaded");
    // every bot's behaviour changes on stderr: SOAK_TRACE_ALL=1
    if std::env::var_os("SOAK_TRACE_ALL").is_some() {
        cfg.trace = (0..cfg.bots).collect();
    }
    let report = soak::run(&cfg);
    println!("{}", report.markdown);
    assert!(
        report.passed(),
        "{}: invariants violated: {:#?}\nfirst failures: {:#?}",
        name,
        report.violations,
        report.failures
    );
    if !report.regressions.is_empty() {
        eprintln!(
            "[{name}] regressions against this host's baseline (warnings): {:#?}",
            report.regressions
        );
        assert!(
            !env_flag("SOAK_STRICT"),
            "{name}: regressions against this host's baseline (--soak-strict): {:#?}",
            report.regressions
        );
    }
}

/// Soak quick 10 bots 10 minutes.
#[test]
pub(crate) fn soak_quick_10_bots_10_minutes() {
    soak("soak-quick-10x10m", 10, 10.0 / 60.0, |c| {
        c.sample_every = 60.0;
        c.tail = 600.0;
    });
}

/// A short run for trying changes: 4 bots for 20 virtual minutes.
#[test]
pub(crate) fn soak_smoke_4_bots_20_minutes() {
    soak("smoke-4x20m", 4, 1.0 / 3.0, |c| {
        c.sample_every = 60.0;
        c.tail = 600.0;
        c.decode_all = true;
    });
}

#[test]
pub(crate) fn soak_10_bots_6_hours() {
    soak("soak-10x6h", 10, 6.0, |c| c.decode_all = true);
}

#[test]
pub(crate) fn soak_50_bots_6_hours() {
    soak("soak-50x6h", 50, 6.0, |_| {});
}

/// 200 sessions need `MaximumAllowedSessions` above ACE's default 128 (at most 256).
#[test]
pub(crate) fn soak_200_bots_6_hours() {
    soak("soak-200x6h", 200, 6.0, |c| c.max_sessions = 256);
}

/// Long enough for the guid recycler (`recycleTime` is 360 minutes) to hand out recycled guids.
#[test]
pub(crate) fn soak_10_bots_24_hours() {
    soak("soak-10x24h", 10, 24.0, |c| c.sample_every = 900.0);
}

/// Soak default corpse limit.
#[test]
pub(crate) fn soak_default_corpse_limit() {
    soak("default-corpse-limit", 10, 2.0, |_| {});
}

/// One real `empyrean-server` on loopback (a free high port), 20 bots over real UDP sockets for two
/// real minutes. Needs the release binary: `cargo build -p empyrean-server --release` first.
#[test]
pub(crate) fn udp_smoke_20_bots_2_minutes() {
    let dir = out_dir().join("udp");
    let cfg = udp::UdpConfig {
        bots: 20,
        play_secs: 120.0,
        server_bin: udp::default_server_bin(),
        dir,
        seed: u64::from(std::process::id()),
    };
    assert!(
        cfg.server_bin.exists(),
        "build the server first: {} is missing",
        cfg.server_bin.display()
    );
    let report = udp::run(&cfg);
    println!("{}", report.markdown);
    assert!(report.violations.is_empty(), "{:#?}", report.violations);
}
