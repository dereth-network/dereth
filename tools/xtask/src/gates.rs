//! The per-area gates: every crate's whole test suite, its data and device tiers included.
//! `cargo xtask gate <name>` runs one (`cargo xtask gate physics`); `cargo xtask gates` runs them
//! all and prints a single pass/fail table. `cargo xtask ci tier1` runs them all without the
//! `gpu` binaries, and `ci tier2` runs those binaries alone.
//!
//! Each gate is named for the area it checks and is at minimum `cargo test -p <crate>` over that
//! area's crates, with the features that build every tier binary. Where an area has a standalone
//! harness in the repository, it is listed as an extra command: a harness is "a script, an
//! assertion, a named oracle, one command", and `xtask gates` is the one command that runs all of
//! them.
//!
//! **A harness that needs the retail dats declares it, and the gate derives the answer.** When
//! they are absent the gate runs every harness that does not want them and reports `NO-ORACLE`
//! naming only the ones it could not run; the run exits non-zero, naming the directory searched
//! and the files missing. It does **not** skip: an oracle that was available to consult and was
//! not consulted certifies nothing: a gate that skipped instead could exit 0 with 81 passing
//! unit tests and the 805,347-record cell oracle never executed. See `util.rs`'s
//! [`crate::util::Outcome`] for why there is no bare `Skip` to return.

use crate::sweep::{self, Out, ShardOpts};
use crate::util::{
    print_table, retail_data_available, retail_dir_searched, retail_shortfall, run, run_captured,
    workspace_root, NotRun, Outcome, Profile, Report,
};

/// One extra harness: a description, the command (a program and its arguments, run from the
/// workspace root), and **whether that harness reads the retail dats**.
///
/// The flag lives here, on the harness, and not on the [`Gate`]. *"Reads the retail
/// dats"* is a property of a script, and a gate can hold several scripts that differ. A per-gate
/// bool cannot express each harness's requirements. With one flag for the whole gate, adding
/// a dat-free harness to the dat gate would stop it running on a dat-less host in silence, and
/// adding a dat-reading harness to any other gate would run it and fail confusingly. Neither is
/// expressible: the condition is a question asked of each harness rather than a list maintained
/// by hand.
///
/// The catalogue currently holds no harness at all (the dat-reading oracles are the `dat` tiers
/// of `dereth-dat` and `dereth-assets`); the mechanism stays for the next one.
pub type Harness = (&'static str, &'static [&'static str], bool);

pub struct Gate {
    /// What `cargo xtask gate <name>` selects: the area the gate checks, in one word.
    pub name: &'static str,
    /// The area, in words, for the table.
    pub title: &'static str,
    /// Crates whose test suites constitute the gate.
    pub crates: &'static [&'static str],
    /// Extra harnesses. See [`Harness`] for why the retail flag lives on each one.
    pub harnesses: &'static [Harness],
}

impl Gate {
    /// The gate's row name: its name and, after it, the area in words.
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.title)
    }

    /// Whether **any** harness of this gate reads the retail dats.
    ///
    /// Derived, never declared, so a gate that declares `needs_retail` while holding no retail
    /// harness is unrepresentable rather than something a test has to pin against.
    pub fn needs_retail(&self) -> bool {
        self.harnesses.iter().any(|(_, _, retail)| *retail)
    }
}

/// The `numerics` gate has no spec of its own; it is gated here anyway because
/// `dereth_primitives::num` is what every other gate's arithmetic is measured against.
pub const GATES: &[Gate] = &[
    Gate {
        name: "dat",
        title: "Dat container + asset decode",
        crates: &["dereth-dat", "dereth-assets"],
        harnesses: &[],
    },
    Gate {
        name: "numerics",
        title: "Numerics, math, core types",
        crates: &["dereth-primitives"],
        harnesses: &[],
    },
    Gate {
        name: "render",
        title: "Device + renderer core",
        crates: &["dereth-render", "dereth-render-cpu"],
        harnesses: &[],
    },
    Gate {
        name: "world-render",
        title: "World rendering",
        crates: &["dereth-world-render"],
        harnesses: &[],
    },
    Gate {
        name: "physics",
        title: "Physics + collision",
        crates: &["dereth-physics"],
        harnesses: &[],
    },
    Gate {
        name: "animation",
        title: "Animation + motion",
        crates: &["dereth-animation"],
        harnesses: &[],
    },
    Gate {
        name: "transport",
        title: "Network transport",
        crates: &["dereth-client-net", "dereth-transport"],
        harnesses: &[],
    },
    Gate {
        name: "protocol",
        title: "Protocol messages + session",
        // The session layer is `dereth_client_net::client_session`; its tests are that crate's
        // `cpu` modules `client_session::*`, which the `transport` gate's `cargo test -p
        // dereth-client-net` already runs, so they are not run a second time here.
        crates: &["dereth-protocol"],
        harnesses: &[],
    },
    Gate {
        name: "object-model",
        title: "Object model + gameplay",
        // The pure rules `dereth-client-model` re-exports live in `dereth-rules`.
        crates: &["dereth-rules", "dereth-client-model"],
        harnesses: &[],
    },
    Gate {
        name: "ui",
        title: "UI framework",
        crates: &["dereth-ui"],
        harnesses: &[],
    },
    Gate {
        name: "ui-screens",
        title: "UI screens + HUD",
        crates: &["dereth-ui-screens"],
        harnesses: &[],
    },
    Gate {
        name: "input",
        title: "Device input",
        crates: &["dereth-input"],
        harnesses: &[],
    },
    Gate {
        name: "audio",
        title: "Audio",
        crates: &["dereth-audio"],
        harnesses: &[],
    },
    Gate {
        name: "tooling",
        title: "Tooling + fixtures + CI",
        // `dereth-corpus`'s consistency test regenerates `fixtures/message-corpus` and
        // `fixtures/packet-captures/index.json` from the recordings and compares them byte for
        // byte, so the committed files are a checked cache; `xtask`'s own tests are the
        // repository-wide gates (line endings, skip claims, corpus claims). `dereth-pcap`'s tests
        // build their captures in memory; its one corpus test is a printed, counted skip when
        // `DERETH_RETAIL_PCAPS` is unset.
        crates: &["dereth-corpus", "dereth-pcap", "xtask"],
        harnesses: &[],
    },
    Gate {
        name: "client",
        title: "The client application",
        // The acceptance gate is `cargo run -p dereth-client -- --headless --frames 1 --capture
        // out.png` producing a byte-identical PNG across three runs, plus `cargo test -p
        // dereth-client`. The three-run comparison is `dereth/client/tests/gpu/presentation/headless_capture_determinism.rs`, which spawns the
        // real binary three times and diffs the bytes, so the crate's own test command *is* the
        // whole gate and no separate harness is needed. It reads the retail dats -- the frame it
        // captures is a texture out of `client_portal.dat` -- and skips with a printed line when
        // they, or a D3D12 device, are absent. `dereth-client-runtime` and `dereth-world-data` have no
        // gate of their own; their dat-reading lib tests run here, under `retail-dats`. Nor have
        // the application's two halves, `dereth-scene` and `dereth-client-shell`: their modules'
        // unit tests moved out of `dereth-client` with them and run here, on the same device.
        crates: &[
            "dereth-client",
            "dereth-scene",
            "dereth-client-shell",
            "dereth-client-runtime",
            "dereth-world-data",
        ],
        harnesses: &[],
    },
    Gate {
        name: "scenarios",
        title: "The behaviour scenarios",
        // `dereth-testkit`'s scenarios drive a real `App` through recorded sessions and the
        // retail dats: its `dat` binary is the end-to-end claim that the behaviour rows hold, so
        // the data tier runs it. Its `cpu` binary and lib tests need no dats.
        crates: &["dereth-testkit"],
        harnesses: &[],
    },
];

fn find(name: &str) -> Option<&'static Gate> {
    GATES.iter().find(|g| g.name.eq_ignore_ascii_case(name))
}

/// Whether a gate run includes the `gpu` test binaries.
///
/// `cargo xtask gate`/`gates` run a crate's whole suite, so they include them. `ci tier1` is the
/// data tier and must run on a host with the dats and no graphics device, so it leaves them out;
/// `ci tier2` runs them on their own ([`gpu_invocations`]). Nothing else changes between the two:
/// the features, and so the build, are the same either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuTier {
    Include,
    Exclude,
}

/// The crates whose `gpu` binary [`GpuTier::Exclude`] leaves out, with the invocation that runs
/// that binary alone: single-threaded, because its tests take a real device one at a time, and
/// for a binary in [`SHARDED_TIERS`] sharded across processes unless `threads` is
/// [`Threads::Serial`].
pub fn gpu_invocations(profile: Profile, threads: Threads) -> Vec<(&'static str, Invocation)> {
    GATES
        .iter()
        .flat_map(|g| g.crates.iter())
        .filter(|krate| serialised_gpu_split(krate).is_some_and(|(_, bins)| bins.contains(&"gpu")))
        .map(|krate| {
            if threads == Threads::Sharded && SHARDED_TIERS.contains(&(*krate, "gpu")) {
                let call = Invocation::Sharded {
                    krate: (*krate).to_owned(),
                    tier: "gpu".to_owned(),
                    debug: profile == Profile::Dev,
                };
                return (*krate, call);
            }
            let mut argv: Vec<String> = vec!["test".into()];
            argv.extend(profile.cargo_args().iter().map(|s| (*s).to_owned()));
            argv.extend(["-p".into(), (*krate).to_owned()]);
            argv.extend(["--features".into(), tier_features(krate).join(",")]);
            argv.extend(["--test", "gpu", "--", "--test-threads=1"].map(str::to_owned));
            (*krate, Invocation::Cargo(argv))
        })
        .collect()
}

/// The features a crate's per-tier test binaries need, for the `cargo test -p <crate>` that **is**
/// the gate.
///
/// Every crate has one test binary per tier, and the `dat` and `gpu` binaries carry
/// `required-features`, and a bare `cargo test -p <crate>`
/// builds neither of them. It does not fail and it does not warn: it runs the `cpu` binary and the
/// lib tests, prints a green summary, and the retail-dat and device tiers are simply absent. That
/// can silently omit a whole suite through `required-features`, and the
/// executed-test count does not fall to zero, so the NOT-STARTED arm below does not catch it
/// either (measured on dereth-ui: 326 tests without the features, 259 with a bare run).
///
/// So every crate names its tiers here. `retail-dats` is the tier switch itself; `vulkan` and
/// `trace` are the features the `gpu` (and, for dereth-physics, the trace-recording) binaries are
/// additionally gated on. A crate whose tests have not been tiered yet gets `&[]` and runs exactly
/// as it did before.
fn tier_features(krate: &str) -> &'static [&'static str] {
    match krate {
        // The two crates with a `gpu` binary.
        "dereth-client" | "dereth-render" => &["retail-dats", "vulkan"],
        // The application's halves: their unit tests hold a device, on the backend they held it
        // on inside `dereth-client`.
        "dereth-scene" | "dereth-client-shell" => &["retail-dats", "vulkan"],
        // dereth-physics's `dat` binary includes the recorded-trace comparison.
        "dereth-physics" => &["retail-dats", "trace"],
        "dereth-animation"
        | "dereth-assets"
        | "dereth-audio"
        | "dereth-clipboard"
        | "dereth-dat"
        | "dereth-client-model"
        | "dereth-input"
        | "dereth-client-net"
        | "dereth-transport"
        | "dereth-protocol"
        | "dereth-ui"
        | "dereth-ui-screens"
        | "dereth-world-render"
        | "xtask"
        | "dereth-client-runtime"
        | "dereth-testkit"
        | "dereth-world-data" => &["retail-dats"],
        _ => &[],
    }
}

/// **The two crates whose `gpu` test binary must not run in parallel with itself**, and their test
/// targets split so the gate can hand that one binary `--test-threads=1`.
///
/// A `gpu` module stands up a real Vulkan device. All of a crate's `gpu` tests share one binary,
/// and libtest runs a binary's tests on as many threads as the host has cores.
/// `dereth/client/tests/gpu/common/gpu.rs` is one process-wide lock for the whole binary, which is what makes a
/// parallel run *correct*; this flag
/// is what makes the gate's run readable and its wall clock honest rather than leaving several
/// hundred device tests queued behind one mutex on N threads.
///
/// **Only the `gpu` binary is serialised.** `cpu` and `dat` keep libtest's default thread count:
/// nothing in them contends for a device, and slowing them down would buy nothing.
///
/// The split is written out rather than derived because cargo has no *"every target except this
/// one"* selector. It is not allowed to drift:
/// `tests::the_split_names_every_test_binary_these_crates_declare` reads the two manifests and
/// fails if a `[[test]]` is added, removed or renamed without this table following -- the same
/// silent-gate shape [`tier_features`] is about, one level up.
///
/// The first half of the pair is the non-`[[test]]` targets, which a bare `cargo test -p <crate>`
/// also builds and runs: `dereth-client` has a lib and a bin, `dereth-render` a lib and the `smoke`
/// example. With the `--doc` invocation and one per test binary, the invocations together cover
/// exactly what the single command covered, so `tests_run` summed over them is the number the
/// single command produced and the NOT-STARTED arithmetic below is unchanged.
fn serialised_gpu_split(krate: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match krate {
        "dereth-client" => Some((&["--lib", "--bins"], &["cpu", "dat", "gpu"])),
        "dereth-render" => Some((&["--lib", "--examples"], &["cpu", "gpu"])),
        // No `gpu` binary: split so its `dat` binary can be sharded like the client's.
        "dereth-testkit" => Some((&["--lib"], &["cpu", "dat"])),
        _ => None,
    }
}

/// **The tier binaries this gate runs as N sharded processes rather than one serial one**, and
/// nothing else in the table is sharded.
///
/// `dereth-client`'s `dat` binary is the one gate invocation that runs `--test-threads=1` for a
/// reason that is not about a device: its heap corruption (see [`test_invocations`]) is a
/// within-process race, so the cap is on threads *inside one process*. A second process shares no
/// heap, so the same 1,288 tests run in N single-threaded processes over disjoint module slices at
/// the same risk and a quarter of the wall clock. The sweep's sharded runner (`cargo xtask sweep
/// --shard-binary`, [`crate::sweep::shard_binary`]) owns the deal, the crash detection and the
/// merge; this table only says which binaries get it.
///
/// `dereth-client`'s `gpu` binary is sharded the same way. Its `--test-threads=1` is the device
/// lock in `dereth/client/tests/gpu/common/gpu.rs`, which is one lock *per process*: each shard opens its own
/// device, and a driver serves several processes' devices at once. The runner picks the process
/// count from the host's cores (`DERETH_TEST_SHARDS` overrides it) and deals modules by their
/// recorded time. `dereth-render`'s `gpu` binary is a handful of tests and stays one process.
///
/// `dereth-testkit`'s `dat` binary is sharded the same way: its scenarios share the client's dat
/// reading, and with it the same within-process heap defect.
const SHARDED_TIERS: &[(&str, &str)] = &[
    ("dereth-client", "dat"),
    ("dereth-client", "gpu"),
    ("dereth-testkit", "dat"),
];

/// How a gate runs the binaries that must not run their tests in parallel with themselves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Threads {
    /// The default: the binaries in [`SHARDED_TIERS`] run as N single-threaded processes.
    Sharded,
    /// `--serial`. One single-threaded process per binary: the unsharded command, and the thing a
    /// sharded run has to be equivalent to.
    Serial,
}

/// One thing a gate runs for one crate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Invocation {
    /// `cargo <argv>`, from the workspace root.
    Cargo(Vec<String>),
    /// The sweep's sharded runner over `<krate>:<tier>`, in this process. It reports one merged
    /// `test result:` line, which is the shape [`tests_run`] counts, and fails if any shard failed
    /// **or died**. It builds in the same profile as the rest of the gate: `debug` is the dev
    /// profile.
    Sharded {
        krate: String,
        tier: String,
        debug: bool,
    },
}

impl Invocation {
    /// Run it, capturing its output. Returns (success, combined output).
    pub fn run_captured(&self) -> (bool, String) {
        match self {
            Invocation::Cargo(argv) => {
                let args: Vec<&str> = argv.iter().map(String::as_str).collect();
                run_captured(&workspace_root(), "cargo", &args)
            }
            Invocation::Sharded { .. } => {
                let mut out = Out::capture();
                let ok = self.run_sharded(&mut out);
                (ok, out.into_text())
            }
        }
    }

    /// Run it, streaming its output. Returns whether it succeeded.
    pub fn run(&self) -> bool {
        match self {
            Invocation::Cargo(argv) => {
                let args: Vec<&str> = argv.iter().map(String::as_str).collect();
                run(&workspace_root(), "cargo", &args)
            }
            Invocation::Sharded { .. } => self.run_sharded(&mut Out::print()),
        }
    }

    /// The sharded runner, in this process, its lines into `out`.
    fn run_sharded(&self, out: &mut Out) -> bool {
        let Invocation::Sharded { krate, tier, debug } = self else {
            return false;
        };
        let profile = if *debug {
            Profile::Dev
        } else {
            Profile::TestRelease
        };
        sweep::shard_binary(krate, tier, &ShardOpts::gate(profile), out) == 0
    }

    /// What this invocation is, for a failure message and for the tests that pin the list.
    pub fn render(&self) -> String {
        match self {
            Invocation::Cargo(argv) => argv.join(" "),
            Invocation::Sharded { krate, tier, debug } => {
                let flag = if *debug { " --debug" } else { "" };
                format!("xtask sweep --shard-binary {krate}:{tier}{flag}")
            }
        }
    }
}

/// Every invocation this gate runs for one crate, in order, each in `profile`.
///
/// One invocation for a crate with no `gpu` binary -- byte for byte the command the gate has
/// always run. For the two that have one, the same work split by target, so that
/// `--test-threads=1` reaches the `gpu` binary and nothing else. See [`serialised_gpu_split`].
/// [`GpuTier::Exclude`] drops the `gpu` binary's invocation and changes nothing else.
fn test_invocations(
    krate: &str,
    features: &str,
    threads: Threads,
    profile: Profile,
    gpu: GpuTier,
) -> Vec<Invocation> {
    let argv = |krate: &str, features: &str, extra: &[&str]| -> Vec<String> {
        let mut out: Vec<String> = vec!["test".into()];
        out.extend(profile.cargo_args().iter().map(|s| (*s).to_owned()));
        out.push("-p".into());
        out.push(krate.into());
        if !features.is_empty() {
            out.push("--features".into());
            out.push(features.into());
        }
        out.extend(extra.iter().map(|s| (*s).to_owned()));
        out
    };
    match serialised_gpu_split(krate) {
        None => vec![Invocation::Cargo(argv(krate, features, &[]))],
        Some((units, binaries)) => {
            let mut calls = vec![
                Invocation::Cargo(argv(krate, features, units)),
                Invocation::Cargo(argv(krate, features, &["--doc"])),
            ];
            for binary in binaries {
                if *binary == "gpu" && gpu == GpuTier::Exclude {
                    continue;
                }
                if threads == Threads::Sharded && SHARDED_TIERS.contains(&(krate, binary)) {
                    calls.push(Invocation::Sharded {
                        krate: krate.to_owned(),
                        tier: (*binary).to_owned(),
                        debug: profile == Profile::Dev,
                    });
                    continue;
                }
                let mut call = argv(krate, features, &["--test", binary]);
                if *binary == "gpu" {
                    call.push("--".into());
                    call.push("--test-threads=1".into());
                } else if *binary == "dat"
                    && (krate == "dereth-client" || krate == "dereth-testkit")
                {
                    // dereth-client's `dat` binary fail-fasts with STATUS_HEAP_CORRUPTION
                    // (0xC0000374) at libtest's default thread count (32 on the reference machine)
                    // and still crashed two runs in three at eight threads under load, so the cap
                    // is serial until the defect (candidate: the per-call WinRT `Calendar`
                    // activation) is found and re-measured.
                    //
                    // This arm is what `--serial` selects. The default is the sharded form above:
                    // same one thread per process, N processes.
                    call.push("--".into());
                    call.push("--test-threads=1".into());
                }
                calls.push(Invocation::Cargo(call));
            }
            calls
        }
    }
}

/// Count the tests a `cargo test` run actually executed, so a stub crate reports as a stub rather
/// than quietly counting as a pass.
fn tests_run(output: &str) -> usize {
    output
        .lines()
        .filter_map(|l| l.trim().strip_prefix("test result: "))
        .filter_map(|l| l.split_once(". "))
        .filter_map(|(_, rest)| rest.split_whitespace().next())
        .filter_map(|n| n.parse::<usize>().ok())
        .sum()
}

/// Split this gate's harnesses into **(runnable here, oracle input absent)**.
///
/// The second half answers the old `oracle_went_unconsulted` question -- *did this gate have an
/// oracle it could have consulted and not consult it?* -- and answers it per harness rather than
/// per gate. Asking one question of the gate (`needs_retail && !dats_present`) and on `true`
/// returning before running *any* harness would leave a dat-free harness sharing a gate with a
/// dat-reading one unrun **and then name it in the failure message as an oracle that did not
/// run** -- false twice over.
///
/// A free function rather than an expression inside `run_gate`, for one reason: `run_gate` cannot
/// be unit-tested without running sixteen crates' test suites, so the decision has to be reachable
/// on its own. The tests call **this** function and not a boolean wrapper over it: `run_gate`
/// needs both halves of the split, so a wrapper would be called only by its own tests, and such a
/// predicate does not verify the production decision.
/// Nothing here is re-derived
/// in the test module either -- a test that re-states the predicate it is checking agrees with
/// itself for ever.
fn harness_split(gate: &Gate, dats_present: bool) -> (Vec<&Harness>, Vec<&Harness>) {
    gate.harnesses
        .iter()
        .partition(|(_, _, retail)| dats_present || !*retail)
}

/// Run one gate, its tests built in `profile`. `threads` decides only how the binaries in
/// [`SHARDED_TIERS`] are launched; it changes nothing about which tests run, which is what makes
/// a sharded run equivalent to a serial one.
pub fn run_gate(gate: &Gate, threads: Threads, profile: Profile, gpu: GpuTier) -> Report {
    let ws = workspace_root();
    let mut total = 0usize;

    for krate in gate.crates {
        let features = tier_features(krate).join(",");
        // One invocation, unless this crate's `gpu` binary has to be run single-threaded; see
        // `test_invocations`. `total` sums every invocation's result lines either way -- and the
        // sharded runner prints exactly one `test result:` line, in libtest's own words, so this
        // arithmetic is the same whichever form ran.
        for call in test_invocations(krate, &features, threads, profile, gpu) {
            let (ok, out) = call.run_captured();
            if !ok {
                print!("{out}");
                return Report::new(
                    gate.label(),
                    Outcome::Fail,
                    format!("{} failed", call.render()),
                );
            }
            total += tests_run(&out);
        }
    }

    // The oracle half, per harness. Run everything this host CAN run first: a
    // harness that ran and failed outranks one that could not run, and reporting NO-ORACLE while a
    // runnable sibling was sitting there red would hide a real failure behind an absence.
    let (runnable, unrunnable) = harness_split(gate, retail_data_available().is_some());

    for (desc, argv, _) in &runnable {
        let (ok, out) = run_captured(&ws, argv[0], &argv[1..]);
        if !ok {
            print!("{out}");
            return Report::new(
                gate.label(),
                Outcome::Fail,
                format!("harness failed: {desc}"),
            );
        }
    }

    // A harness whose oracle input is absent had a question to ask and nobody asked it, so it
    // reports NO-ORACLE and the run exits non-zero.
    //
    // Reporting `Outcome::Skip` and exiting 0 here would let 81 unit tests pass while the
    // 805,347-record cell oracle never ran. The failure message names the directory searched and
    // the harnesses that did not run -- only those, not every harness of the gate.
    if !unrunnable.is_empty() {
        let dir = retail_dir_searched();
        let missing = retail_shortfall(&dir).join(", ");
        let unrun: Vec<&str> = unrunnable.iter().map(|(desc, _, _)| *desc).collect();
        return Report::new(
            gate.label(),
            Outcome::NotRun(NotRun::OracleAbsent),
            format!(
                "{total} unit test(s) and {} harness(es) passed, then {} ORACLE HARNESS(ES) DID \
                 NOT RUN: {}. Missing under {}: {missing}. Set DERETH_TEST_DAT_DIR to the \
                 directory holding them.",
                runnable.len(),
                unrun.len(),
                unrun.join(" | "),
                dir.display(),
            ),
        );
    }

    // A gate with no tests has not started. Reporting that as PASS makes the summary line claim
    // progress that does not exist -- at one point it read "14 passed" when eleven gates were empty
    // stubs. So it is NOT-BUILT, which is never counted as a pass and does not redden CI.
    //
    // KNOWN LIMIT, stated rather than left to be found: this arm cannot distinguish an unstarted
    // gate from a test binary that COMPILED TO NOTHING behind a non-default feature.
    // Both read as `total == 0`. Every gate in `GATES` runs a
    // non-zero number of tests today -- `the_gate_catalogue_is_what_this_unit_measured` pins the
    // catalogue so that a new empty gate is a decision somebody makes on purpose -- so the arm is
    // currently unreachable, and telling the two apart is filed rather than guessed at here.
    if total == 0 {
        return Report::new(
            gate.label(),
            Outcome::NotRun(NotRun::NotBuilt),
            format!(
                "NOT STARTED: `cargo test -p {}` executed 0 tests. If this area HAS tests, they \
                 were compiled to nothing -- check for a non-default feature gate.",
                gate.crates.join(", ")
            ),
        );
    }
    let detail = format!("{total} test(s), {} harness(es)", gate.harnesses.len());
    Report::new(gate.label(), Outcome::Pass, detail)
}

/// The gate names, space-separated, for a usage or an unknown-name message.
pub fn names() -> String {
    GATES.iter().map(|g| g.name).collect::<Vec<_>>().join(" ")
}

pub fn gate(name: &str, threads: Threads, profile: Profile) -> i32 {
    match find(name) {
        Some(g) => print_table(
            &format!("Gate {}", g.label()),
            &[run_gate(g, threads, profile, GpuTier::Include)],
        ),
        None => {
            eprintln!("unknown gate {name:?}; known: {}", names());
            2
        }
    }
}

/// Every gate, one row each. `gpu` says whether the `gpu` binaries run (see [`GpuTier`]).
pub fn gates(threads: Threads, profile: Profile, gpu: GpuTier) -> Vec<Report> {
    GATES
        .iter()
        .map(|g| run_gate(g, threads, profile, gpu))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixed_gate() -> Gate {
        Gate {
            name: "dat",
            title: "mixed fixture",
            crates: &["dereth-dat"],
            harnesses: &[
                (
                    "the retail one (reads client_cell_1.dat)",
                    &["a-harness"],
                    true,
                ),
                ("the dat-free one", &["b-harness"], false),
            ],
        }
    }

    /// The gate catalogue is what this unit measured.
    #[test]
    fn the_gate_catalogue_is_what_this_unit_measured() {
        assert_eq!(GATES.len(), 16, "sixteen areas");
        let harnesses: usize = GATES.iter().map(|g| g.harnesses.len()).sum();
        assert_eq!(harnesses, 0, "every oracle is a crate's own test tier");
        let retail_harnesses: Vec<&str> = GATES
            .iter()
            .flat_map(|g| g.harnesses.iter())
            .filter(|(_, _, r)| *r)
            .map(|(desc, _, _)| *desc)
            .collect();
        assert!(
            retail_harnesses.is_empty(),
            "no HARNESS reads the retail dats since the last cell reader was retired"
        );
        let retail: Vec<&str> = GATES
            .iter()
            .filter(|g| g.needs_retail())
            .map(|g| g.name)
            .collect();
        assert!(retail.is_empty(), "so no gate derives needs_retail");
        let names: Vec<&str> = GATES.iter().map(|g| g.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate gate name in GATES");
        for name in names {
            assert!(
                name.len() > 1 && name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{name:?}: a gate is named for what it checks, in lowercase words"
            );
        }
    }

    /// A retail harness with no dats is an oracle that went unconsulted.
    #[test]
    fn a_retail_harness_with_no_dats_is_an_oracle_that_went_unconsulted() {
        assert!(!harness_split(&mixed_gate(), false).1.is_empty());
        assert!(
            Outcome::NotRun(NotRun::OracleAbsent).is_failure(),
            "and that outcome must redden the run"
        );
    }

    /// A gate that needs no dats is not reddened by their absence.
    #[test]
    fn a_gate_that_needs_no_dats_is_not_reddened_by_their_absence() {
        let g = find("transport").expect("gate");
        assert!(
            harness_split(&mixed_gate(), true).1.is_empty(),
            "dats present: the oracle is consulted"
        );
        assert!(
            harness_split(g, false).1.is_empty(),
            "the transport gate never needed the dats"
        );
        assert!(harness_split(g, true).1.is_empty());
        assert!(!Outcome::NotRun(NotRun::Unhostable).is_failure());
        assert!(!Outcome::NotRun(NotRun::NotBuilt).is_failure());
    }

    /// A mixed gate runs the harness it can and reports only the one it cannot.
    #[test]
    fn a_mixed_gate_runs_the_harness_it_can_and_reports_only_the_one_it_cannot() {
        let mixed = mixed_gate();

        let (runnable, unrunnable) = harness_split(&mixed, false);
        assert_eq!(
            runnable.len(),
            1,
            "the dat-free harness runs on a host with no dats"
        );
        assert_eq!(runnable[0].0, "the dat-free one");
        assert_eq!(
            unrunnable.len(),
            1,
            "and only the retail one is reported unrun"
        );
        assert_eq!(unrunnable[0].0, "the retail one (reads client_cell_1.dat)");
        assert!(
            !harness_split(&mixed, false).1.is_empty(),
            "one unconsulted oracle still reddens"
        );

        // **Negative control**: with the dats present nothing is held back, and every harness is
        // in the runnable half. A split that put a retail harness aside regardless of the dats
        // would pass the calibration above and fail here.
        let (runnable, unrunnable) = harness_split(&mixed, true);
        assert_eq!(runnable.len(), 2);
        assert!(unrunnable.is_empty());
        assert!(harness_split(&mixed, true).1.is_empty());
    }

    /// The present catalogue behaves exactly as it did before the flag moved.
    #[test]
    fn the_present_catalogue_behaves_exactly_as_it_did_before_the_flag_moved() {
        for dats in [false, true] {
            for g in GATES {
                let (runnable, unrunnable) = harness_split(g, dats);
                assert_eq!(
                    runnable.len() + unrunnable.len(),
                    g.harnesses.len(),
                    "gate {} lost a harness in the split",
                    g.name
                );
                // No gate holds a retail harness today, so none can hold anything back.
                let expected_unrun = 0;
                assert_eq!(
                    unrunnable.len(),
                    expected_unrun,
                    "gate {} with dats={dats} changed behaviour",
                    g.name
                );
                assert_eq!(!harness_split(g, dats).1.is_empty(), expected_unrun > 0);
            }
        }
    }

    /// Needs retail is derived from the harnesses.
    #[test]
    fn needs_retail_is_derived_from_the_harnesses() {
        assert!(
            !find("dat").expect("gate").needs_retail(),
            "its last harness was retired"
        );
        assert!(!find("transport").expect("gate").needs_retail());
        assert!(
            !find("numerics").expect("gate").needs_retail(),
            "no harnesses at all"
        );
        assert!(
            mixed_gate().needs_retail(),
            "one retail harness of two is enough"
        );
        for g in GATES {
            assert_eq!(
                g.needs_retail(),
                g.harnesses.iter().any(|(_, _, r)| *r),
                "gate {} derives its flag from something other than its harnesses",
                g.name
            );
        }
    }

    /// The `[[test]]` targets a crate's manifest declares, read rather than restated. A hand-kept
    /// copy of a list is the thing [`serialised_gpu_split`] must not become.
    fn declared_test_binaries(krate: &str) -> Vec<String> {
        let manifest =
            crate::util::crate_dir(&crate::util::workspace_root(), krate).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("{}: {e}", manifest.display()));
        let mut names = Vec::new();
        let mut in_test_table = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_test_table = line == "[[test]]";
            } else if in_test_table {
                if let Some(rest) = line.strip_prefix("name") {
                    let value = rest.trim_start().strip_prefix('=').expect("name = \"...\"");
                    names.push(value.trim().trim_matches('"').to_owned());
                }
            }
        }
        names
    }

    /// The split names every test binary these crates declare.
    #[test]
    fn the_split_names_every_test_binary_these_crates_declare() {
        for krate in ["dereth-client", "dereth-render"] {
            let (units, binaries) = serialised_gpu_split(krate).expect("a crate with a gpu binary");
            let declared = declared_test_binaries(krate);
            assert_eq!(
                declared,
                binaries.iter().map(|b| (*b).to_owned()).collect::<Vec<_>>(),
                "{krate}'s [[test]] targets and the gate's split have diverged"
            );
            assert!(
                binaries.contains(&"gpu"),
                "{krate} is in this table because it has a `gpu`"
            );
            assert!(
                units.contains(&"--lib"),
                "{krate}'s lib tests must still run"
            );
            assert!(
                tier_features(krate).contains(&"vulkan"),
                "{krate}'s gpu binary is gated on `vulkan`, so the gate must pass it"
            );
        }
        // Negative control: no other crate is split, so every other gate runs exactly the one
        // command it always ran.
        for krate in ["dereth-ui", "dereth-physics", "dereth-dat", "xtask"] {
            assert!(
                serialised_gpu_split(krate).is_none(),
                "{krate} must not be split"
            );
        }
    }

    /// **`--test-threads=1` reaches the `gpu` binary and nothing else**, and the split covers the
    /// same targets the single command did.
    ///
    /// The negative half matters as much as the positive one: serialising `cpu` and `dat` as well
    /// would pass a "the gpu binary is single-threaded" assertion and cost the gate its parallel
    /// run of three and a half thousand tests that never touch a device.
    #[test]
    fn only_the_gpu_binary_is_run_single_threaded() {
        let calls = rendered("dereth-client", "retail-dats,vulkan", Threads::Serial);
        assert_eq!(
            calls,
            vec![
                "test --profile test-release -p dereth-client --features retail-dats,vulkan --lib --bins",
                "test --profile test-release -p dereth-client --features retail-dats,vulkan --doc",
                "test --profile test-release -p dereth-client --features retail-dats,vulkan --test cpu",
                "test --profile test-release -p dereth-client --features retail-dats,vulkan --test dat -- --test-threads=1",
                "test --profile test-release -p dereth-client --features retail-dats,vulkan --test gpu -- --test-threads=1",
            ],
            "lib + bins, doc tests and one invocation per tier; `gpu` and dereth-client's `dat` are serialised (R-flake)"
        );
        for call in &calls {
            assert_eq!(
                call.contains("--test-threads=1"),
                call.contains("--test gpu") || call.contains("-p dereth-client --features retail-dats,vulkan --test dat"),
                "{call}: the flag belongs to the gpu binaries and to dereth-client's dat binary (R-flake), and to nothing else"
            );
        }
        let render = rendered("dereth-render", "retail-dats,vulkan", Threads::Serial);
        assert_eq!(
            render,
            vec![
                "test --profile test-release -p dereth-render --features retail-dats,vulkan --lib --examples",
                "test --profile test-release -p dereth-render --features retail-dats,vulkan --doc",
                "test --profile test-release -p dereth-render --features retail-dats,vulkan --test cpu",
                "test --profile test-release -p dereth-render --features retail-dats,vulkan --test gpu -- --test-threads=1",
            ]
        );

        // **Negative control**: a crate with no `gpu` binary still gets the one command the gate
        // has always run, with its tier features and no libtest arguments at all.
        let ui = rendered("dereth-ui", "retail-dats", Threads::Serial);
        assert_eq!(
            ui,
            vec!["test --profile test-release -p dereth-ui --features retail-dats"]
        );
        assert_eq!(declared_test_binaries("dereth-primitives"), ["cpu"]);
        assert_eq!(
            rendered(
                "dereth-primitives",
                &tier_features("dereth-primitives").join(","),
                Threads::Serial,
            ),
            ["test --profile test-release -p dereth-primitives"],
            "the data-free vocabulary has no tier feature"
        );
        let bare = rendered("dereth-console", "", Threads::Serial);
        assert_eq!(
            bare,
            vec!["test --profile test-release -p dereth-console"],
            "no features, no trailing `--features`"
        );
    }

    fn rendered(krate: &str, features: &str, threads: Threads) -> Vec<String> {
        test_invocations(
            krate,
            features,
            threads,
            Profile::TestRelease,
            GpuTier::Include,
        )
        .iter()
        .map(Invocation::render)
        .collect()
    }

    /// **Every invocation is in `test-release` by default, and `--debug` changes the profile and
    /// nothing else.** The sharded runner is told too, so its build is not the one binary left in
    /// the other profile.
    #[test]
    fn debug_changes_the_profile_of_every_invocation_and_nothing_else() {
        for threads in [Threads::Sharded, Threads::Serial] {
            let release = test_invocations(
                "dereth-client",
                "retail-dats,vulkan",
                threads,
                Profile::TestRelease,
                GpuTier::Include,
            );
            let debug = test_invocations(
                "dereth-client",
                "retail-dats,vulkan",
                threads,
                Profile::Dev,
                GpuTier::Include,
            );
            assert_eq!(release.len(), debug.len());
            for (r, d) in release.iter().zip(&debug) {
                let (r, d) = (r.render(), d.render());
                if r.starts_with("xtask sweep") {
                    assert_eq!(format!("{r} --debug"), d, "the sharded runner is told");
                } else {
                    assert!(r.starts_with("test --profile test-release -p "), "{r}");
                    assert_eq!(r.replacen(" --profile test-release", "", 1), d);
                }
            }
        }
    }

    /// **What `--serial` switches, and what it must not switch** (unit R-shard).
    ///
    /// The default replaces exactly two invocations -- `dereth-client`'s `dat` and `gpu` binaries
    /// -- with the sharded runner, and `--serial` puts those two back. Everything else in both
    /// lists is identical, which is the claim worth pinning: a flag that quietly changed how the
    /// `cpu` binary or the lib tests ran would make the two forms incomparable, and the
    /// equivalence proof this unit rests on compares the two forms.
    #[test]
    fn sharding_replaces_the_serialised_client_invocations_and_only_those() {
        let sharded = rendered("dereth-client", "retail-dats,vulkan", Threads::Sharded);
        let serial = rendered("dereth-client", "retail-dats,vulkan", Threads::Serial);
        assert_eq!(
            sharded.len(),
            serial.len(),
            "sharding runs the same number of things"
        );
        let differing: Vec<(&String, &String)> = sharded
            .iter()
            .zip(&serial)
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(
            differing,
            vec![
                (
                    &"xtask sweep --shard-binary dereth-client:dat".to_owned(),
                    &"test --profile test-release -p dereth-client --features retail-dats,vulkan --test dat -- --test-threads=1"
                        .to_owned()
                ),
                (
                    &"xtask sweep --shard-binary dereth-client:gpu".to_owned(),
                    &"test --profile test-release -p dereth-client --features retail-dats,vulkan --test gpu -- --test-threads=1"
                        .to_owned()
                ),
            ],
            "exactly the two serialised client binaries may differ between the two forms"
        );
        assert!(
            rendered("dereth-render", "retail-dats,vulkan", Threads::Sharded)
                .iter()
                .any(|c| c.contains("--test gpu") && c.contains("--test-threads=1")),
            "dereth-render's gpu binary is not sharded and keeps its one-thread run"
        );

        // **Negative control**: the flag is inert everywhere else, so no other gate changes shape.
        for krate in [
            "dereth-render",
            "dereth-ui",
            "dereth-console",
            "dereth-physics",
        ] {
            assert_eq!(
                rendered(krate, "retail-dats", Threads::Sharded),
                rendered(krate, "retail-dats", Threads::Serial),
                "{krate} has no sharded tier and must run identically under both forms"
            );
        }
        // And the table cannot name a binary the split does not run.
        for (krate, tier) in SHARDED_TIERS {
            let (_units, binaries) =
                serialised_gpu_split(krate).expect("a sharded crate must be in the split");
            assert!(
                binaries.contains(tier),
                "{krate}'s split does not run a `{tier}` binary, so sharding it would run nothing"
            );
        }
    }

    /// `tests_run` decides the NOT-BUILT arm, so it gets both directions too. The calibration is
    /// the shape that is hard for it: SEVERAL result lines, which is what a multi-crate gate
    /// produces and what a single-line parser would undercount.
    #[test]
    fn tests_run_sums_every_result_line_and_reads_zero_when_there_is_none() {
        let many = "test result: ok. 12 passed; 0 failed; 0 ignored\n\
                    running 3 tests\n\
                    test result: ok. 3 passed; 0 failed; 1 ignored\n";
        assert_eq!(
            tests_run(many),
            15,
            "every binary's line counts, not just the last"
        );
        assert_eq!(tests_run(""), 0);
        assert_eq!(
            tests_run("error: no test target named `missing_example_target`\n"),
            0,
            "a NO-RUN reads as zero, which routes to NOT-BUILT rather than to PASS"
        );
        // The zero-passed result still reads as zero here, so
        // it can never be mistaken for a pass by the `total == 0` arm below it.
        assert_eq!(
            tests_run("test result: ok. 0 passed; 0 failed; 0 ignored\n"),
            0
        );
    }

    /// **The data tier leaves out the `gpu` binaries and nothing else, and the device tier runs
    /// exactly those.** A data-tier run that still reached a `gpu` binary would need a graphics
    /// device; one that dropped anything more would lose a tier nobody else runs.
    #[test]
    fn the_data_tier_drops_the_gpu_binaries_and_the_device_tier_runs_only_those() {
        for krate in ["dereth-client", "dereth-render"] {
            let features = tier_features(krate).join(",");
            let all = rendered(krate, &features, Threads::Serial);
            let data: Vec<String> = test_invocations(
                krate,
                &features,
                Threads::Serial,
                Profile::TestRelease,
                GpuTier::Exclude,
            )
            .iter()
            .map(Invocation::render)
            .collect();
            let dropped: Vec<&String> = all.iter().filter(|c| !data.contains(c)).collect();
            assert_eq!(dropped.len(), 1, "{krate}: {dropped:?}");
            assert!(dropped[0].contains("--test gpu"), "{krate}: {dropped:?}");
            assert!(
                data.iter().all(|c| all.contains(c)),
                "{krate}: nothing else changes"
            );
        }
        // Negative control: a crate with no `gpu` binary runs the same command either way.
        for krate in ["dereth-ui", "dereth-physics"] {
            let features = tier_features(krate).join(",");
            let data: Vec<String> = test_invocations(
                krate,
                &features,
                Threads::Serial,
                Profile::TestRelease,
                GpuTier::Exclude,
            )
            .iter()
            .map(Invocation::render)
            .collect();
            assert_eq!(data, rendered(krate, &features, Threads::Serial), "{krate}");
        }
        let device = |threads| -> Vec<String> {
            gpu_invocations(Profile::TestRelease, threads)
                .into_iter()
                .map(|(_, call)| call.render())
                .collect()
        };
        assert_eq!(
            device(Threads::Serial),
            vec![
                "test --profile test-release -p dereth-render --features retail-dats,vulkan --test gpu -- --test-threads=1",
                "test --profile test-release -p dereth-client --features retail-dats,vulkan --test gpu -- --test-threads=1",
            ]
        );
        assert_eq!(
            device(Threads::Sharded),
            vec![
                "test --profile test-release -p dereth-render --features retail-dats,vulkan --test gpu -- --test-threads=1",
                "xtask sweep --shard-binary dereth-client:gpu",
            ],
            "tier 2 shards dereth-client's gpu binary as the gates do, and only that one"
        );
    }
}
