//! Vectors: local ordered-job, acknowledgement and SQLite commit kill-point cases in this module
//! A child process aborted mid-write leaves every job wholly present or absent, in order, at
//! least the last acknowledged, integrity ok.
//! Fixture: a child process, temporary SQLite files and explicit commit kill points.

#![cfg(feature = "real-sqlite")]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use empyrean_common::clock::SystemClock;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{PropertyInt, PropertyString};
use empyrean_entity::Biota;
use empyrean_store::{ShardDatabase, ShardHandle, SqliteShard};

const SEED: u64 = 0x5EED_0012_0000_0001;
const KILL_POINTS: usize = 50;
const MAX_JOBS: u64 = 400;
const BIOTAS_PER_JOB: u32 = 12;
const IN_FLIGHT: u64 = 8;

const ENV_DB: &str = "SERV_STORE_KILL_DB";
const ENV_TARGET: &str = "SERV_STORE_KILL_TARGET";
const ENV_ACKS: &str = "SERV_STORE_KILL_ACKS";
const ENV_MARKER: &str = "SERV_STORE_KILL_MARKER";

fn biota_id(k: u64, i: u32) -> u32 {
    0x8000_0000 + u32::try_from(k).unwrap() * 1000 + i + 1
}

fn job(k: u64) -> Vec<Biota> {
    (0..BIOTAS_PER_JOB)
        .map(|i| {
            let mut ints = DotNetDict::new();
            ints.insert(PropertyInt(1), i32::try_from(k).unwrap());
            for p in 2..10u16 {
                ints.insert(PropertyInt(p), i32::from(p));
            }
            let mut strings = DotNetDict::new();
            strings.insert(PropertyString(1), format!("job {k} item {i}"));
            Biota {
                id: biota_id(k, i),
                weenie_class_id: 1000 + i,
                properties_int: Some(ints),
                properties_string: Some(strings),
                ..Default::default()
            }
        })
        .collect()
}

/// splitmix64 from a fixed seed: test scaffolding, not simulation randomness.
fn kill_targets() -> Vec<u64> {
    let mut state = SEED;
    (0..KILL_POINTS)
        .map(|_| {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            1 + (z ^ (z >> 31)) % (MAX_JOBS / 2)
        })
        .collect()
}

#[derive(Default)]
struct World {
    acked: Vec<u64>,
}

/// The child: a no-op unless the parent set the environment.
#[test]
fn durability_kill_child() {
    let Ok(db_path) = std::env::var(ENV_DB) else {
        return;
    };
    let target: u64 = std::env::var(ENV_TARGET).unwrap().parse().unwrap();
    let acks_path = PathBuf::from(std::env::var(ENV_ACKS).unwrap());
    let marker_path = PathBuf::from(std::env::var(ENV_MARKER).unwrap());

    let mut handle: ShardHandle<World> = ShardHandle::new(
        Box::new(SqliteShard::open(&db_path).unwrap()),
        Arc::new(SystemClock::new()),
    );
    handle.start();
    let probe = handle.probe();
    let acked = Arc::new(AtomicU64::new(0));

    let killer_acked = Arc::clone(&acked);
    std::thread::spawn(move || loop {
        let n = killer_acked.load(Ordering::SeqCst);
        if n >= target && probe.in_transaction() {
            let mut m = std::fs::File::create(&marker_path).unwrap();
            writeln!(m, "mid-transaction after {n} acks").unwrap();
            m.sync_all().unwrap();
            std::process::abort();
        }
        if n >= MAX_JOBS {
            std::fs::write(&marker_path, "ran out of jobs").unwrap();
            std::process::abort();
        }
        std::thread::yield_now();
    });

    let mut acks = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&acks_path)
        .unwrap();
    let mut world = World::default();
    let mut submitted = 0u64;
    loop {
        while submitted < MAX_JOBS && submitted - acked.load(Ordering::SeqCst) < IN_FLIGHT {
            submitted += 1;
            let k = submitted;
            handle.save_biotas_in_parallel(
                job(k),
                Some(Box::new(move |w: &mut World, ok| {
                    assert!(ok);
                    w.acked.push(k);
                })),
                false,
            );
        }
        handle.drain(&mut world);
        for k in world.acked.drain(..) {
            writeln!(acks, "{k}").unwrap();
            acks.sync_data().unwrap();
            acked.store(k, Ordering::SeqCst);
        }
        std::thread::yield_now();
    }
}

fn check_recovered(db_path: &Path, last_acked: u64) -> (u64, bool) {
    let mut db = SqliteShard::open(db_path).unwrap();
    let integrity: String = db
        .connection()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");

    let mut present = Vec::new();
    for k in 1..=MAX_JOBS {
        let found: Vec<bool> = (0..BIOTAS_PER_JOB)
            .map(|i| match db.get_biota(biota_id(k, i), false) {
                None => false,
                Some(b) => {
                    assert_eq!(
                        b.get_property_int(PropertyInt(1)),
                        Some(i32::try_from(k).unwrap()),
                        "job {k} item {i}"
                    );
                    assert_eq!(
                        b.biota_properties_int.len(),
                        9,
                        "job {k} item {i}: all its rows"
                    );
                    assert_eq!(
                        b.get_property_string(PropertyString(1)),
                        Some(format!("job {k} item {i}").as_str())
                    );
                    true
                }
            })
            .collect();
        assert!(
            found.iter().all(|&f| f) || found.iter().all(|&f| !f),
            "job {k} is partly present: {found:?}"
        );
        if found[0] {
            present.push(k);
        }
    }
    let m = present.len() as u64;
    assert_eq!(
        present,
        (1..=m).collect::<Vec<_>>(),
        "committed jobs are a prefix"
    );
    assert!(
        m >= last_acked,
        "an acknowledged job was lost: {m} present, {last_acked} acknowledged"
    );
    (m, m > last_acked)
}

#[test]
fn durability_kill_points() {
    let exe = std::env::current_exe().unwrap();
    let base = std::env::temp_dir().join(format!("serv-store-durability-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let mut mid_transaction = 0;
    let mut unacked_but_committed = 0;
    for (n, target) in kill_targets().into_iter().enumerate() {
        let dir = base.join(format!("k{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("shard.db");
        let acks = dir.join("acks.txt");
        let marker = dir.join("marker.txt");
        let status = std::process::Command::new(&exe)
            .args([
                "--exact",
                "persistence::kill_point_durability::durability_kill_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(ENV_DB, &db)
            .env(ENV_TARGET, target.to_string())
            .env(ENV_ACKS, &acks)
            .env(ENV_MARKER, &marker)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "kill point {n}: the child must die by abort"
        );
        let marker_text = std::fs::read_to_string(&marker).unwrap_or_default();
        if marker_text.starts_with("mid-transaction") {
            mid_transaction += 1;
        }
        let last_acked = std::fs::read_to_string(&acks)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        assert!(
            last_acked >= target,
            "kill point {n}: {last_acked} acks, target {target}"
        );
        let (_, extra) = check_recovered(&db, last_acked);
        if extra {
            unacked_but_committed += 1;
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    println!("durability: {KILL_POINTS} kill points, {mid_transaction} mid-transaction, {unacked_but_committed} with committed-but-unacknowledged jobs");
    assert_eq!(
        mid_transaction, KILL_POINTS,
        "every kill landed inside a write transaction"
    );
}
