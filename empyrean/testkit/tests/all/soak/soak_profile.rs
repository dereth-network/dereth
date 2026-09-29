//! Vectors: synthetic histogram buckets and host profiles defined in this module.
//! Soak profile maths: nearest-rank quantiles, sustained overrun window, margin comparison,
//! baseline round-trip, top10, FNV-1a host hash, harness share.
//! Fixture: synthetic host and frame timing measurements.

use empyrean_testkit::soak_metrics::Histogram;
use empyrean_testkit::soak_profile::{
    compare, config_key, fnv1a64, top10, BaselineFile, HarnessTime, Headline, Margins,
    OverrunWindow, Timed,
};

fn hist(values: impl IntoIterator<Item = f64>) -> Histogram {
    let mut h = Histogram::default();
    for v in values {
        h.add(v);
    }
    h
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn quantiles_are_the_upper_edge_of_the_nearest_rank_bucket() {
    // 1.0 ms .. 100.0 ms: the 50th and 99th values, reported to their bucket's upper edge (the
    // buckets are 64 us wide at 50 ms and 128 us wide at 99 ms: 512 per doubling)
    let h = hist((1..=100).map(f64::from));
    assert!(close(h.quantile_ms(0.5), 50.048), "{}", h.quantile_ms(0.5));
    assert!(
        close(h.quantile_ms(0.99), 99.072),
        "{}",
        h.quantile_ms(0.99)
    );
    // never above the maximum
    assert!(close(h.quantile_ms(1.0), 100.0));
    assert!(close(h.mean_ms(), 50.5));
    assert!(close(h.max_ms, 100.0));
    // 1 us buckets below 1,024 us: 99 ticks of 0.25 ms and one slow one; the p99 is still fast
    let h = hist(std::iter::repeat_n(0.25, 99).chain([40.0]));
    assert!(close(h.quantile_ms(0.99), 0.251), "{}", h.quantile_ms(0.99));
    assert!(close(h.quantile_ms(0.5), 0.251));
    assert!(close(h.quantile_ms(1.0), 40.0));
    // sub-0.1 ms ticks are told apart (the old 0.1 ms buckets read 0.1 for all of these)
    let h = hist((0..100).map(|i| if i < 50 { 0.012 } else { 0.087 }));
    assert!(close(h.quantile_ms(0.5), 0.013), "{}", h.quantile_ms(0.5));
    assert!(close(h.quantile_ms(0.99), 0.087), "{}", h.quantile_ms(0.99));
    // at most 0.2% wide anywhere in the range
    for ms in [1.5, 16.7, 250.0, 9_999.0, 60_000.0] {
        let q = hist([ms]).quantile_ms(0.5);
        assert!(q <= ms && q >= ms * 0.998, "{ms}: {q}");
        let mut h = hist([ms]);
        h.add(ms * 2.0);
        let q = h.quantile_ms(0.5);
        assert!(q > ms && q <= ms * 1.002, "{ms}: {q}");
    }
    // past the range (67 s), the quantile is the maximum
    let h = hist([1.0, 500.0, 100_000.0]);
    assert_eq!(h.over, 1);
    assert!(close(h.quantile_ms(0.99), 100_000.0));
    assert_eq!(h.above(1000.0 / 60.0), 2);
    // an empty histogram
    assert!(Histogram::default().quantile_ms(0.99).abs() < 1e-9);
}

#[test]
fn the_overrun_window_trips_only_on_a_sustained_share() {
    // 10-tick window, limit 20%: two overruns in any ten ticks are allowed, three are not
    let mut w = OverrunWindow::new(16.0, 10, 0.2);
    for i in 0..100 {
        // an overrun every fifth tick: exactly two per window
        w.push(if i % 5 == 0 { 30.0 } else { 1.0 });
    }
    assert_eq!(w.over, 20);
    assert_eq!(w.worst, 2);
    assert!(!w.breached(), "20% is at the limit, not over it");
    assert!((w.over_ms - 20.0 * 14.0).abs() < 1e-9);
    assert!((w.over_fraction() - 0.2).abs() < 1e-9);
    // three within ten ticks: tripped, and the worst window ends at the third
    w.push(30.0);
    w.push(30.0);
    assert!(w.breached());
    assert_eq!(w.worst, 3);
    assert_eq!(w.worst_end, 102);
    // spread-out overruns never trip, however many
    let mut w = OverrunWindow::new(16.0, 10, 0.2);
    for i in 0..10_000 {
        w.push(if i % 10 == 0 { 50.0 } else { 16.0 });
    }
    assert_eq!(w.over, 1000);
    assert_eq!(w.worst, 1);
    assert!(!w.breached());
    // a burst before the first window has filled counts against the whole window
    let mut w = OverrunWindow::new(16.0, 10, 0.2);
    for _ in 0..3 {
        w.push(17.0);
    }
    assert!(w.breached());
    // the default: 5% of 3,600 ticks over 16.67 ms
    let w = OverrunWindow::default();
    assert_eq!(w.window, 3600);
    assert!((w.interval_ms - 1000.0 / 60.0).abs() < 1e-9);
    assert!((w.max_fraction - 0.05).abs() < 1e-9);
}

fn headline(p99: f64, growth: f64, msgs: f64) -> Headline {
    Headline {
        p99_tick_ms: p99,
        growth_mb_per_bot_hour: growth,
        msgs_per_s: msgs,
    }
}

fn regressed(base: Headline, cur: Headline, m: &Margins) -> Vec<&'static str> {
    compare(&base, &cur, m)
        .into_iter()
        .filter(|f| f.regressed)
        .map(|f| f.metric)
        .collect()
}

#[test]
fn the_comparison_flags_only_figures_past_their_margins() {
    let m = Margins::default();
    assert!(
        (m.p99_tick - 0.25).abs() < 1e-9
            && (m.mem_growth - 0.20).abs() < 1e-9
            && (m.msgs - 0.20).abs() < 1e-9
    );
    let base = headline(8.0, 10.0, 5000.0);
    // equal, and within every margin
    assert!(regressed(base, base, &m).is_empty());
    assert!(regressed(base, headline(9.9, 11.9, 4100.0), &m).is_empty());
    // past each margin, one at a time
    assert_eq!(
        regressed(base, headline(10.1, 10.0, 5000.0), &m),
        ["p99 tick ms"]
    );
    assert_eq!(
        regressed(base, headline(8.0, 12.1, 5000.0), &m),
        ["memory growth MB per bot-hour"]
    );
    assert_eq!(
        regressed(base, headline(8.0, 10.0, 3999.0), &m),
        ["messages per s"]
    );
    // improvements are never flagged
    assert!(regressed(base, headline(1.0, -5.0, 9000.0), &m).is_empty());
    // the floors: +67% on a 0.3 ms p99 is noise (0.2 ms < 0.25 ms), +300% is not
    assert!(regressed(headline(0.3, 10.0, 5000.0), headline(0.5, 10.0, 5000.0), &m).is_empty());
    assert_eq!(
        regressed(headline(0.3, 10.0, 5000.0), headline(1.2, 10.0, 5000.0), &m),
        ["p99 tick ms"]
    );
    // memory growth near zero: +0.5 MB per bot-hour is under the floor; +2 is flagged even from 0
    assert!(regressed(headline(8.0, 0.0, 5000.0), headline(8.0, 0.5, 5000.0), &m).is_empty());
    assert_eq!(
        regressed(headline(8.0, 0.0, 5000.0), headline(8.0, 2.0, 5000.0), &m),
        ["memory growth MB per bot-hour"]
    );
    // a negative baseline growth: the margin is on its magnitude
    assert_eq!(
        regressed(
            headline(8.0, -10.0, 5000.0),
            headline(8.0, -6.0, 5000.0),
            &m
        ),
        ["memory growth MB per bot-hour"]
    );
    // configurable margins
    let loose = Margins { p99_tick: 1.0, ..m };
    assert!(regressed(base, headline(15.9, 10.0, 5000.0), &loose).is_empty());
    assert_eq!(
        regressed(base, headline(16.6, 10.0, 5000.0), &loose),
        ["p99 tick ms"]
    );
    // the change is reported signed and relative
    let f = compare(&base, &headline(10.0, 10.0, 4000.0), &m);
    assert!((f[0].change - 0.25).abs() < 1e-9);
    assert!((f[2].change + 0.2).abs() < 1e-9);
    assert!((f[2].allowed + 0.2).abs() < 1e-9);
}

#[test]
fn a_profile_round_trips_through_the_baseline_file() {
    let profile = serde_json::json!({
        "tick": { "p99_ms": 7.5 },
        "memory": { "growth_mb_per_bot_hour": 3.25 },
        "messages": { "per_s": 1234.5 },
    });
    let key = config_key("soak-quick-10x10m", "release", Some(2));
    assert_eq!(key, "soak-quick-10x10m/release/cores2");
    assert_eq!(
        config_key("soak-quick-10x10m", "release", None),
        "soak-quick-10x10m/release"
    );
    let mut file = BaselineFile::default();
    file.entries.insert(key.clone(), profile);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("soak-profile-test");
    let path = BaselineFile::path(&dir, "0123456789abcdef");
    assert!(path.ends_with("0123456789abcdef.json"));
    file.write(&path).expect("written");
    let back = BaselineFile::read(&path).expect("read back");
    assert_eq!(back, file);
    assert_eq!(
        Headline::from_json(&back.entries[&key]),
        Some(headline(7.5, 3.25, 1234.5))
    );
    // not a baseline file
    std::fs::write(&path, "{\"format\":\"something else\",\"entries\":{}}").expect("written");
    assert!(BaselineFile::read(&path).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn top10_ranks_by_total_and_by_p99_and_drops_the_idle() {
    let t = |name: &str, count: u64, total: f64, p99: f64| Timed {
        name: name.to_owned(),
        count,
        total_ms: total,
        p50_ms: 0.0,
        p99_ms: p99,
        max_ms: p99,
    };
    let mut items: Vec<Timed> = (0..12)
        .map(|i| t(&format!("s{i}"), 10, f64::from(i), f64::from(12 - i)))
        .collect();
    items.push(t("idle", 0, 0.0, 0.0));
    let (by_total, by_p99) = top10(&items);
    assert_eq!(by_total.len(), 10);
    assert_eq!(by_total[0].name, "s11");
    assert_eq!(by_p99[0].name, "s0");
    assert!(by_total.iter().chain(&by_p99).all(|x| x.name != "idle"));
}

#[test]
fn the_host_hash_is_stable_fnv1a() {
    // FNV-1a 64's published test values
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
}

/// The harness share is what is not the server tick.
#[test]
fn the_harness_share_is_what_is_not_the_server_tick() {
    let h = HarnessTime {
        loop_s: 100.0,
        server_tick_s: 40.0,
        step_s: 55.0,
        bots_s: 30.0,
        instrumentation_s: 10.0,
        process_cpu_s: Some(101.0),
    };
    assert!(close(h.harness_s(), 60.0));
    assert!(close(h.share(), 0.6));
    assert!(close(h.clients_s(), 15.0));
    assert!(close(h.other_s(), 5.0));
    let j = h.to_json();
    assert_eq!(j["harness_share"].as_f64(), Some(0.6));
    assert_eq!(j["clients_s"].as_f64(), Some(15.0));
    assert!(h.line().starts_with("harness share 60.0% of the loop's 100 s wall (server tick 40.0%; bots 30.0%, clients' transports 15.0%, instrumentation 10.0%, other 5.0%)"), "{}", h.line());
    assert!(close(HarnessTime::default().share(), 0.0));
}
