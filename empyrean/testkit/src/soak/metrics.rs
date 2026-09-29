//! What the soak measures: samples over the run and per-hour figures.
//!
//! Nothing here is an ACE port; there are no ACE anchors.

use std::fmt::Write as _;

/// The process's resident memory (the working set on Windows, `VmRSS` on Linux, `ps`'s RSS on
/// macOS), in bytes; `None` when it cannot be read.
#[must_use]
pub fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(windows)]
    {
        // `tasklist`'s "Mem Usage" is the working set, as "123,456 K"
        let out = std::process::Command::new("tasklist")
            .args([
                "/FI",
                &format!("PID eq {}", std::process::id()),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let field = text.trim().rsplit("\",\"").next()?.trim_end_matches('"');
        let kb: u64 = field
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?;
        Some(kb * 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        None
    }
}

/// Durations in log-linear buckets: 1 us wide below 1,024 us, then 512 buckets per
/// doubling (at most 0.2% wide) up to 2^26 us (67 s), and a count above. A world tick is often
/// well under 0.1 ms, so 0.1 ms buckets could not tell a p50 from a p99.
#[derive(Debug, Clone)]
pub struct Histogram {
    buckets: Vec<u64>,
    pub over: u64,
    pub count: u64,
    pub sum_ms: f64,
    pub max_ms: f64,
}

/// Buckets below 1,024 us (1 us each); then `SUB` per doubling, for `DOUBLINGS` doublings.
const LINEAR: u64 = 1024;
const SUB: u64 = 512;
const DOUBLINGS: u64 = 16;
#[allow(clippy::cast_possible_truncation)]
const BUCKETS: usize = (LINEAR + SUB * DOUBLINGS) as usize;

/// The bucket of `us` microseconds (`BUCKETS` or more: above the range).
fn bucket_of(us: u64) -> usize {
    if us < LINEAR {
        return usize::try_from(us).unwrap_or(usize::MAX);
    }
    // us in [2^k, 2^(k+1)) with k >= 10: shift so that the mantissa is in [512, 1024)
    let shift = u64::from(63 - us.leading_zeros()) - 9;
    let mantissa = us >> shift;
    usize::try_from(LINEAR + (shift - 1) * SUB + (mantissa - SUB)).unwrap_or(usize::MAX)
}

/// The lowest microsecond value in bucket `i` (`i` may be `BUCKETS`, the range's end).
fn bucket_low_us(i: usize) -> u64 {
    let i = i as u64;
    if i < LINEAR {
        i
    } else {
        let shift = (i - LINEAR) / SUB + 1;
        let mantissa = (i - LINEAR) % SUB + SUB;
        mantissa << shift
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_us(ms: f64) -> u64 {
    (ms.max(0.0) * 1000.0) as u64
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            buckets: vec![0; BUCKETS],
            over: 0,
            count: 0,
            sum_ms: 0.0,
            max_ms: 0.0,
        }
    }
}

impl Histogram {
    pub fn add(&mut self, ms: f64) {
        self.count += 1;
        self.sum_ms += ms;
        self.max_ms = self.max_ms.max(ms);
        match self.buckets.get_mut(bucket_of(to_us(ms))) {
            Some(b) => *b += 1,
            None => self.over += 1,
        }
    }

    #[must_use]
    pub fn mean_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let n = self.count as f64;
            self.sum_ms / n
        }
    }

    /// The `q` quantile (0..1), nearest rank, to its bucket's upper edge (never above the maximum).
    #[must_use]
    pub fn quantile_ms(&self, q: f64) -> f64 {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let want = ((self.count as f64) * q).ceil() as u64;
        let mut seen = 0;
        for (i, b) in self.buckets.iter().enumerate() {
            seen += b;
            if seen >= want && want > 0 {
                #[allow(clippy::cast_precision_loss)]
                let upper = bucket_low_us(i + 1) as f64 / 1000.0;
                return upper.min(self.max_ms);
            }
        }
        self.max_ms
    }

    /// How many were above `ms` (to the bucket: those in `ms`'s own bucket count as above).
    #[must_use]
    pub fn above(&self, ms: f64) -> u64 {
        let from = bucket_of(to_us(ms)).min(self.buckets.len());
        self.buckets[from..].iter().sum::<u64>() + self.over
    }
}

/// One sample of the world.
#[derive(Debug, Clone, Default)]
pub struct Sample {
    /// Virtual seconds since start.
    pub t: f64,
    /// Wall seconds since start.
    pub wall: f64,
    pub phase: &'static str,
    /// `World.objects`.
    pub objects: usize,
    /// Objects reachable from the loaded landblocks (containers' contents included).
    pub reachable: usize,
    pub landblocks: usize,
    pub online: usize,
    pub sessions: usize,
    pub rss: Option<u64>,
    pub shard_biotas: i64,
    pub shard_characters: usize,
    /// The dynamic guid allocator: `current`, recycled guids waiting, sequence-gap ids left.
    pub guid_current: u32,
    pub guid_recycled_pending: usize,
    pub world_actions: usize,
    pub delays: usize,
    pub inbound: usize,
    pub landblock_actions: usize,
    pub shard_queue: i32,
    pub messages: u64,
    /// Objects the bots believe exist, summed (a client forgets an object only on DeleteObject).
    pub bot_known: usize,
    pub panics: u64,
    /// Bots by what they are doing.
    pub activities: String,
}

impl Sample {
    pub const CSV_HEADER: &'static str = "t_s,wall_s,phase,objects,reachable,landblocks,online,sessions,rss_mb,shard_biotas,shard_characters,guid_current,guid_recycled_pending,world_actions,delays,inbound,landblock_actions,shard_queue,messages,bot_known,panics,activities";

    #[must_use]
    pub fn csv(&self) -> String {
        #[allow(clippy::cast_precision_loss)]
        let rss = self
            .rss
            .map_or(String::new(), |b| format!("{:.1}", b as f64 / 1_048_576.0));
        format!(
            "{:.0},{:.1},{},{},{},{},{},{},{},{},{},{:08X},{},{},{},{},{},{},{},{},{},{}",
            self.t,
            self.wall,
            self.phase,
            self.objects,
            self.reachable,
            self.landblocks,
            self.online,
            self.sessions,
            rss,
            self.shard_biotas,
            self.shard_characters,
            self.guid_current,
            self.guid_recycled_pending,
            self.world_actions,
            self.delays,
            self.inbound,
            self.landblock_actions,
            self.shard_queue,
            self.messages,
            self.bot_known,
            self.panics,
            self.activities
        )
    }
}

/// One virtual hour.
#[derive(Debug, Clone, Default)]
pub struct Hour {
    pub hour: u32,
    /// Wall seconds this virtual hour took.
    pub wall: f64,
    /// The wall time of each world iteration (`TestServer::step`: the bots' transports, the world
    /// tick, the delivery), in ms.
    pub step: Histogram,
    /// The performance monitor's stages over the hour: (name, events, mean ms, longest ms).
    pub stages: Vec<(&'static str, i64, f64, f64)>,
    /// Messages all bots received this hour.
    pub messages: u64,
    /// The server tick (the six `WorldManager.UpdateWorld` stages), in ms: what the
    /// overrun guard counts (the step also holds the clients' side).
    pub tick: Histogram,
    /// The hour's wall seconds that were not the server tick (the bots, their transports
    /// and the soak's instrumentation).
    pub harness_s: f64,
}

/// Formats bytes as MB.
#[must_use]
pub fn mb(b: Option<u64>) -> String {
    #[allow(clippy::cast_precision_loss)]
    b.map_or_else(
        || "?".to_owned(),
        |b| format!("{:.0}", b as f64 / 1_048_576.0),
    )
}

/// The per-hour table, as Markdown.
#[must_use]
pub fn hours_table(hours: &[Hour], bots: usize) -> String {
    let mut s = String::from("| hour | wall s | step mean ms | p99 ms | max ms | steps > 16.7 ms | server tick mean / p99 / max ms | ticks > 16.7 ms | harness share | UpdateGameWorld mean/max ms | DoSessionWork mean/max ms | msgs/bot |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for h in hours {
        let stage = |name: &str| {
            h.stages
                .iter()
                .find(|s| s.0 == name)
                .map_or("?".to_owned(), |s| format!("{:.3}/{:.1}", s.2, s.3))
        };
        #[allow(clippy::cast_precision_loss)]
        let per_bot = h.messages as f64 / bots.max(1) as f64;
        let _ = writeln!(
            s,
            "| {} | {:.1} | {:.3} | {:.1} | {:.1} | {} | {:.3} / {:.1} / {:.1} | {} | {:.0}% | {} | {} | {:.0} |",
            h.hour,
            h.wall,
            h.step.mean_ms(),
            h.step.quantile_ms(0.99),
            h.step.max_ms,
            h.step.above(1000.0 / 60.0),
            h.tick.mean_ms(),
            h.tick.quantile_ms(0.99),
            h.tick.max_ms,
            h.tick.above(1000.0 / 60.0),
            if h.wall > 0.0 { h.harness_s / h.wall * 100.0 } else { 0.0 },
            stage("UpdateGameWorld_Entire"),
            stage("NetworkManager_DoSessionWork"),
            per_bot
        );
    }
    s.push_str("\n| hour | Landblock physics mean/max ms | Landblock multi-threaded work mean/max ms | Landblock single-threaded work mean/max ms | PlayerManager tick mean/max ms | DoSessionWork outbound mean/max ms |\n|---|---|---|---|---|---|\n");
    for h in hours {
        let stage = |name: &str| {
            h.stages
                .iter()
                .find(|s| s.0 == name)
                .map_or("?".to_owned(), |s| format!("{:.3}/{:.1}", s.2, s.3))
        };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} |",
            h.hour,
            stage("LandblockManager_TickPhysics"),
            stage("LandblockManager_TickMultiThreadedWork"),
            stage("LandblockManager_TickSingleThreadedWork"),
            stage("PlayerManager_Tick"),
            stage("DoSessionWork_TickOutbound")
        );
    }
    s
}

/// The landblock sections (the monitor's cumulative events: one event per world tick, summed over
/// every loaded landblock), the physics pass and the world loop's other stages, ranked by their
/// total wall time over the run. The share is of `UpdateGameWorld_Entire` (the landblock sections
/// and the physics pass are inside it; the loop's other stages are beside it).
#[must_use]
pub fn hot_spots_table(hours: &[Hour]) -> String {
    #[allow(clippy::cast_precision_loss)]
    let total = |name: &str| -> (f64, f64) {
        hours
            .iter()
            .flat_map(|h| h.stages.iter())
            .filter(|s| s.0 == name)
            .fold((0.0, 0.0f64), |(t, m), s| {
                (t + s.1 as f64 * s.2, m.max(s.3))
            })
    };
    let world = total("UpdateGameWorld_Entire").0;
    // the landblock sections, the physics pass, and the world loop's other stages
    let others = [
        "LandblockManager_TickPhysics",
        "PlayerManager_Tick",
        "actionQueue_RunActions",
        "DelayManager_RunActions",
        "NetworkManager_InboundClientMessageQueueRun",
        "DoSessionWork_TickOutbound",
    ];
    let names: Vec<&str> = hours
        .first()
        .map(|h| {
            h.stages
                .iter()
                .map(|s| s.0)
                .filter(|n| n.starts_with("Landblock_Tick_") || others.contains(n))
                .collect()
        })
        .unwrap_or_default();
    let mut rows: Vec<(&str, f64, f64)> = names
        .into_iter()
        .map(|n| {
            let (t, m) = total(n);
            (n, t, m)
        })
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut s = format!(
        "UpdateGameWorld_Entire in all: {:.1} s\n\n| section | total s | share of UpdateGameWorld | longest tick ms |\n|---|---|---|---|\n",
        world / 1000.0
    );
    for (name, t, m) in rows {
        let share = if world > 0.0 { t / world * 100.0 } else { 0.0 };
        let _ = writeln!(s, "| {name} | {:.1} | {share:.0}% | {m:.1} |", t / 1000.0);
    }
    s
}
