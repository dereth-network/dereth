//! Vectors: fixtures/vectors/ddd/
//! DDDManager vs ACE vectors; client iteration list parsed as the client writes runs;
//! DDDDataMessage framing; iteration file sent with client's type key.
//! Divergence: V414
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use std::collections::{BTreeMap, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use dereth_protocol::admin::{DddBeginDdd, DddData};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::config_manager::ConfigManager;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::vectors::{self, u64_of};
use empyrean_dat::{DatDatabaseType, FakeDats};
use empyrean_world::managers::ddd_manager::{self, dat_file_type, DatFileSize, DddManagerState};
use empyrean_world::network::game_messages::messages::game_message_ddd_begin_ddd::game_message_ddd_begin_ddd;
use empyrean_world::network::game_messages::messages::game_message_ddd_data_message::game_message_ddd_data_message;
use empyrean_world::network::structure::c_mostly_consecutive_int_set::CMostlyConsecutiveIntSet;
use empyrean_world::World;
use serde_json::Value;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

fn u32_of(v: &Value) -> u32 {
    u32::try_from(u64_of(v).expect("an integer")).expect("a uint")
}

fn i32_of(v: &Value) -> i32 {
    i32::try_from(v.as_i64().expect("an integer")).expect("an int")
}

/// `DddVectors.Input(kind, len)`.
fn input(kind: u64, len: usize) -> Vec<u8> {
    const ALPHABET: &[u8] = b"abcdefghij klmnop";
    let mut x = u32::try_from(len).expect("small");
    (0..len)
        .map(|i| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match kind {
                0 => 0,
                1 => u8::try_from(x >> 24).expect("a byte"),
                2 => ALPHABET[((x >> 24) % 17) as usize],
                _ => u8::try_from((i % 251) ^ (i / 1000)).expect("a byte"),
            }
        })
        .collect()
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// `DDDManager.Compress` equals .NET 10's `ZLibStream` at `SmallestSize` byte for byte, over
/// zeros, noise, text-like and periodic inputs up to 200,000 bytes (past `CopyTo`'s 81,920-byte
/// chunks); `PrependUncompressedFileSize` leads with the little-endian size.
#[test]
fn compress_matches_ace() {
    let file = vectors::load_named("ddd", "ddd_compress");
    assert_eq!(file.cases.len(), 56);
    for c in &file.cases {
        let kind = u64_of(&c.input["kind"]).expect("kind");
        let len = usize::try_from(u64_of(&c.input["len"]).expect("len")).expect("len");
        let data = input(kind, len);
        let out = ddd_manager::compress(&data);
        assert_eq!(
            out.len() as u64,
            u64_of(&c.output["len"]).expect("len"),
            "Compress(kind {kind}, len {len}).Length"
        );
        assert_eq!(
            fnv1a64(&out),
            u64_of(&c.output["fnv1a64"]).expect("fnv"),
            "Compress(kind {kind}, len {len}) bytes"
        );
        if let Some(h) = c.output.get("hex") {
            assert_eq!(hex(&out), h.as_str().expect("hex"));
            let prepended =
                ddd_manager::prepend_uncompressed_file_size(&out, u32::try_from(len).expect("len"));
            assert_eq!(
                hex(&prepended),
                c.output["prepended"].as_str().expect("hex")
            );
        }
    }
}

/// One dat's missing iterations: the dat, then each iteration with its files.
type DatIterations = (DatDatabaseType, Vec<(u32, Vec<u32>)>);

fn dat_type(name: &str) -> DatDatabaseType {
    match name {
        "Portal" => DatDatabaseType::Portal,
        "Cell" => DatDatabaseType::Cell,
        "Language" => DatDatabaseType::Language,
        "HighRes" => DatDatabaseType::HighRes,
        other => panic!("dat type {other}"),
    }
}

fn int_set(v: &Value) -> CMostlyConsecutiveIntSet {
    CMostlyConsecutiveIntSet {
        iterations: i32_of(&v["iterations"]),
        ints: v["ints"]
            .as_array()
            .expect("ints")
            .iter()
            .map(i32_of)
            .collect(),
    }
}

/// The vector's `in.server`, as `DDDManager`'s state.
fn server_state(v: &Value) -> DddManagerState {
    let mut state = DddManagerState::new();
    for dat in v.as_array().expect("server") {
        let ty = dat_type(dat["type"].as_str().expect("type"));
        let mut iterations = BTreeMap::new();
        for it in dat["iterations"].as_array().expect("iterations") {
            let files: Vec<u32> = it[1]
                .as_array()
                .expect("files")
                .iter()
                .map(u32_of)
                .collect();
            iterations.insert(u32_of(&it[0]), files);
        }
        let mut sizes = HashMap::new();
        for s in dat["sizes"].as_array().expect("sizes") {
            sizes.insert(
                u32_of(&s[0]),
                DatFileSize {
                    uncompressed_file_size: i32_of(&s[1]),
                    compressed_file_size: i32_of(&s[2]),
                },
            );
        }
        state.iterations.insert(ty, iterations);
        state.dat_file_sizes.insert(ty, sizes);
    }
    state
}

/// Each dat's missing iterations, by number only, in Portal/Cell/Language/HighRes order.
fn missing_numbers(r: &ddd_manager::MissingIterationsResult) -> Vec<(DatDatabaseType, Vec<u32>)> {
    [
        DatDatabaseType::Portal,
        DatDatabaseType::Cell,
        DatDatabaseType::Language,
        DatDatabaseType::HighRes,
    ]
    .into_iter()
    .filter_map(|ty| {
        r.iterations
            .get(&ty)
            .map(|d| (ty, d.iter().map(|(k, _)| *k).collect()))
    })
    .collect()
}

/// The vector cases where V290 parts from ACE (a lone entry equal to the dat's total, which ACE
/// read as up to date), with what is missing now.
fn rr82_cases() -> HashMap<usize, Vec<(DatDatabaseType, Vec<u32>)>> {
    use DatDatabaseType::{Cell, HighRes, Language, Portal};
    HashMap::from([
        // portal `5, -3` (a run length with nothing after it), cell `0, -5, 1`, language `3`
        (
            6,
            vec![
                (Portal, vec![1, 2, 3, 4, 6, 7, 8, 9, 10]),
                (Language, vec![1, 2]),
            ],
        ),
        // a lone total in each of portal (10), cell (5) and language (3)
        (
            7,
            vec![
                (Portal, (1..=9).collect()),
                (Cell, vec![1, 2, 3, 4]),
                (Language, vec![1, 2]),
            ],
        ),
        // high-res holds only its total, 4
        (
            11,
            vec![
                (Portal, vec![10]),
                (Cell, vec![5]),
                (Language, vec![3]),
                (HighRes, vec![1, 2, 3]),
            ],
        ),
    ])
}

/// `GetMissingIterations` over ACE's own results: which iterations each dat misses and their
/// files (in ACE's order), the total file size (compressed sizes plus 4 each, cell files not
/// counted), and the `BeginDDD` message those feed, byte for byte; `int.MinValue` throws. The
/// three cases V290 changes are checked against the fixed reading instead.
#[test]
fn get_missing_iterations_and_begin_ddd_match_ace() {
    let file = vectors::load_named("ddd", "ddd_get_missing_iterations");
    assert_eq!(file.cases.len(), 13);
    let rr82 = rr82_cases();
    for (n, c) in file.cases.iter().enumerate() {
        let state = server_state(&c.input["server"]);
        let client: Vec<CMostlyConsecutiveIntSet> = c.input["client"]
            .as_array()
            .expect("client")
            .iter()
            .map(int_set)
            .collect();
        let run = || {
            ddd_manager::get_missing_iterations(
                &state, &client[0], &client[1], &client[2], &client[3],
            )
        };
        if vectors::throws(&c.output).is_some() {
            assert!(
                catch_unwind(AssertUnwindSafe(run)).is_err(),
                "case {n}: ACE throws"
            );
            continue;
        }
        let r = run();
        if let Some(want) = rr82.get(&n) {
            assert_eq!(&missing_numbers(&r), want, "case {n}: V290");
            let count: usize = want.iter().map(|(_, v)| v.len()).sum();
            assert_eq!(
                r.total_missing_iterations as usize, count,
                "case {n}: V290 total"
            );
            let msg = game_message_ddd_begin_ddd(
                r.total_missing_iterations,
                r.total_file_size,
                &r.iterations,
            );
            let begin: DddBeginDdd =
                dereth_protocol::read_body_padded(&msg.data[4..]).expect("BeginDDD decodes");
            assert_eq!(begin.revisions.len(), count, "case {n}");
            continue;
        }
        assert_eq!(
            r.total_missing_iterations,
            u32_of(&c.output["total_missing_iterations"]),
            "case {n}: total"
        );
        assert_eq!(
            r.total_file_size,
            u32_of(&c.output["total_file_size"]),
            "case {n}: size"
        );

        let mut got: Vec<DatIterations> = Vec::new();
        for ty in [
            DatDatabaseType::Portal,
            DatDatabaseType::Cell,
            DatDatabaseType::Language,
            DatDatabaseType::HighRes,
        ] {
            if let Some(d) = r.iterations.get(&ty) {
                let its: Vec<(u32, Vec<u32>)> = d.iter().map(|(k, v)| (*k, v.clone())).collect();
                got.push((ty, its));
            }
        }
        let want: Vec<DatIterations> = c.output["iterations"]
            .as_array()
            .expect("iterations")
            .iter()
            .map(|d| {
                let its = d["iterations"]
                    .as_array()
                    .expect("list")
                    .iter()
                    .map(|it| {
                        (
                            u32_of(&it[0]),
                            it[1]
                                .as_array()
                                .expect("files")
                                .iter()
                                .map(u32_of)
                                .collect(),
                        )
                    })
                    .collect();
                (dat_type(d["type"].as_str().expect("type")), its)
            })
            .collect();
        assert_eq!(got, want, "case {n}: iterations");

        let msg = game_message_ddd_begin_ddd(
            r.total_missing_iterations,
            r.total_file_size,
            &r.iterations,
        );
        assert_eq!(
            hex(&msg.data),
            c.output["begin_ddd"].as_str().expect("hex"),
            "case {n}: BeginDDD"
        );

        // Shared rules: the client's own reader takes the message apart as ACE meant it.
        let begin: DddBeginDdd =
            dereth_protocol::read_body_padded(&msg.data[4..]).expect("BeginDDD decodes");
        assert_eq!(begin.data_expected, r.total_file_size, "case {n}");
        assert_eq!(
            begin.revisions.len(),
            r.total_missing_iterations as usize,
            "case {n}"
        );
    }
}

/// A server whose portal dat is at iteration 10 and language dat at 3, one file per iteration.
fn rr82_state() -> DddManagerState {
    let mut state = DddManagerState::new();
    for (ty, total, base) in [
        (DatDatabaseType::Portal, 10u32, 0x0100_0000u32),
        (DatDatabaseType::Language, 3, 0x2300_0000),
    ] {
        let iterations: BTreeMap<u32, Vec<u32>> =
            (1..=total).map(|i| (i, vec![base + i])).collect();
        let sizes: HashMap<u32, DatFileSize> = (1..=total)
            .map(|i| {
                (
                    base + i,
                    DatFileSize {
                        uncompressed_file_size: 10,
                        compressed_file_size: 0,
                    },
                )
            })
            .collect();
        state.iterations.insert(ty, iterations);
        state.dat_file_sizes.insert(ty, sizes);
    }
    state
}

/// The portal iterations a client with this list is missing (language and the rest current).
fn portal_missing(iterations: i32, ints: &[i32]) -> Vec<u32> {
    let state = rr82_state();
    let portal = CMostlyConsecutiveIntSet {
        iterations,
        ints: ints.to_vec(),
    };
    let language = CMostlyConsecutiveIntSet {
        iterations: 3,
        ints: vec![-3, 1],
    };
    let none = CMostlyConsecutiveIntSet::default();
    let r = ddd_manager::get_missing_iterations(&state, &portal, &none, &language, &none);
    r.iterations
        .get(&DatDatabaseType::Portal)
        .map(|d| d.iter().map(|(k, _)| *k).collect())
        .unwrap_or_default()
}

/// V290: the client's iteration list read as the client writes it (a run of three or more is
/// `-length, first`, anything else one entry per iteration). A lone `total` is a client with only
/// the latest iteration, not one that is up to date.
#[test]
fn the_iteration_list_is_read_as_the_client_writes_it() {
    // the latest iteration alone
    assert_eq!(portal_missing(1, &[10]), (1..=9).collect::<Vec<_>>());
    // every iteration: `-total, 1`
    assert!(portal_missing(10, &[-10, 1]).is_empty());
    // singles only
    assert_eq!(portal_missing(3, &[1, 2, 10]), (3..=9).collect::<Vec<_>>());
    // runs and singles: 1-3, 5, 7-9
    assert_eq!(portal_missing(7, &[-3, 1, 5, -3, 7]), vec![4, 6, 10]);
    // a gap before the latest: 1-4 and 10, the total as a single after a run
    assert_eq!(portal_missing(5, &[-4, 1, 10]), vec![5, 6, 7, 8, 9]);
    // a run that ends on the total: 6-10
    assert_eq!(portal_missing(5, &[-5, 6]), vec![1, 2, 3, 4, 5]);
    // a run as long as the total that does not start at 1 covers only what it covers
    assert_eq!(portal_missing(10, &[-10, 2]), vec![1]);
    // nothing at all
    assert_eq!(portal_missing(0, &[]), (1..=10).collect::<Vec<_>>());

    // the same over the language dat: its total alone
    let state = rr82_state();
    let portal = CMostlyConsecutiveIntSet {
        iterations: 10,
        ints: vec![-10, 1],
    };
    let language = CMostlyConsecutiveIntSet {
        iterations: 1,
        ints: vec![3],
    };
    let none = CMostlyConsecutiveIntSet::default();
    let r = ddd_manager::get_missing_iterations(&state, &portal, &none, &language, &none);
    assert_eq!(
        missing_numbers(&r),
        vec![(DatDatabaseType::Language, vec![1, 2])]
    );
    assert_eq!(r.total_missing_iterations, 2);
    assert_eq!(r.total_file_size, 20);
}

/// `Ranges` and `Show` (the debug listing of missing iterations).
#[test]
fn ranges_and_show_match_ace() {
    let file = vectors::load_named("ddd", "ddd_ranges_show");
    for c in &file.cases {
        let nums: Vec<u32> = c.input["nums"]
            .as_array()
            .expect("nums")
            .iter()
            .map(u32_of)
            .collect();
        let ranges = ddd_manager::ranges(&nums);
        let want: Vec<(u32, u32)> = c.output["ranges"]
            .as_array()
            .expect("ranges")
            .iter()
            .map(|r| (u32_of(&r[0]), u32_of(&r[1])))
            .collect();
        assert_eq!(ranges, want, "Ranges({nums:?})");
        assert_eq!(
            ddd_manager::show(&ranges),
            c.output["show"].as_str().expect("show"),
            "Show({nums:?})"
        );
    }
}

/// `DatFile.GetFileType` for every dat over a grid of ids, and `DatFileType.ToString()`.
#[test]
fn dat_file_types_match_ace() {
    let file = vectors::load_named("ddd", "dat_file_get_file_type");
    assert!(file.cases.len() > 600);
    for c in &file.cases {
        let id = u32_of(&c.input["id"]);
        let ty = dat_type(c.input["dat"].as_str().expect("dat"));
        let got = ddd_manager::dat_file_get_file_type(id, ty);
        let want = if c.output["type"].is_null() {
            None
        } else {
            Some(u32_of(&c.output["type"]))
        };
        assert_eq!(got, want, "GetFileType({id:08X}, {ty:?})");
        if let Some(t) = got {
            assert_eq!(
                dat_file_type::name(t),
                c.output["name"].as_str().expect("name")
            );
        }
    }
    let file = vectors::load_named("ddd", "dat_file_type_names");
    for c in &file.cases {
        let v = u32_of(&c.input["value"]);
        assert_eq!(
            dat_file_type::name(v),
            c.output.as_str().expect("name"),
            "DatFileType {v}"
        );
    }
}

// ---- over fake dats ----------------------------------------------------------------------------

const TEXTURE: u32 = 0x0600_0001;
const SETUP: u32 = 0x0200_0002;

/// Portal iteration 3: a compressible texture brought by iteration 2 and a tiny setup by
/// iteration 3; the iteration file itself is at iteration 1.
fn fake_dats() -> FakeDats {
    FakeDats::new()
        .with_iteration(DatDatabaseType::Portal, 3)
        .with_iteration(DatDatabaseType::Cell, 1)
        .with_iteration(DatDatabaseType::Language, 1)
        .with_raw(DatDatabaseType::Portal, TEXTURE, vec![7u8; 600])
        .with_file_iteration(DatDatabaseType::Portal, TEXTURE, 2)
        .with_raw(DatDatabaseType::Portal, SETUP, vec![1, 2, 3])
        .with_file_iteration(DatDatabaseType::Portal, SETUP, 3)
}

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    ConfigManager::initialize(MasterConfiguration::default());
    let mut w = World::new(now, fake_dats().build().expect("fake dats"));
    let dats = std::sync::Arc::clone(&w.dats);
    ddd_manager::initialize(&mut w.ddd_manager, &dats);
    w
}

/// `InitIterations`: every iteration up to the dat's total has an entry, each file sits under its
/// own iteration, and only a file that compresses to more than 4 bytes smaller is sent
/// compressed.
#[test]
fn initialize_records_iterations_and_sizes() {
    let w = world();
    let portal = &w.ddd_manager.iterations[&DatDatabaseType::Portal];
    assert_eq!(portal.keys().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(portal[&2], vec![TEXTURE]);
    assert_eq!(portal[&3], vec![SETUP]);
    // the iteration file (a decoded object in the fakes) is at iteration 1
    assert_eq!(portal[&1], vec![0xFFFF_0001]);

    let sizes = &w.ddd_manager.dat_file_sizes[&DatDatabaseType::Portal];
    let texture = sizes[&TEXTURE];
    assert_eq!(texture.uncompressed_file_size, 600);
    assert_eq!(
        usize::try_from(texture.compressed_file_size).expect("size"),
        ddd_manager::compress(&[7u8; 600]).len()
    );
    assert_eq!(
        sizes[&SETUP],
        DatFileSize {
            uncompressed_file_size: 3,
            compressed_file_size: 0
        }
    );
    assert!(
        w.ddd_manager.compressed_dat_files_cache[&DatDatabaseType::Portal].is_empty(),
        "precaching is off by default"
    );
    assert!(
        !w.ddd_manager
            .iterations
            .contains_key(&DatDatabaseType::HighRes),
        "no high-res dat"
    );
}

/// `GameMessageDDDDataMessage`: a compressible file goes compressed (with its uncompressed size in
/// front, and cached), a small one raw; the client's reader agrees on every field.
#[test]
fn data_message_sends_the_file() {
    let mut w = world();

    let m = game_message_ddd_data_message(&mut w, TEXTURE, DatDatabaseType::Portal);
    let d: DddData = dereth_protocol::read_body_padded(&m.data[4..]).expect("DataMessage decodes");
    let mut want = 600u32.to_le_bytes().to_vec();
    want.extend(ddd_manager::compress(&[7u8; 600]));
    assert_eq!(
        (
            d.dat_file_type,
            d.dat_file_id,
            d.resource_type,
            d.resource_id,
            d.iteration,
            d.compressed,
            d.version
        ),
        (0, 1, dat_file_type::TEXTURE, TEXTURE, 2, 1, 3)
    );
    assert_eq!(d.data_size as usize, want.len() + 4);
    assert_eq!(d.data, want);
    assert_eq!(
        w.ddd_manager.compressed_dat_files_cache[&DatDatabaseType::Portal][&TEXTURE],
        want,
        "cached on first send"
    );

    let m = game_message_ddd_data_message(&mut w, SETUP, DatDatabaseType::Portal);
    let d: DddData = dereth_protocol::read_body_padded(&m.data[4..]).expect("DataMessage decodes");
    assert_eq!(
        (
            d.resource_type,
            d.iteration,
            d.compressed,
            d.data_size,
            d.data.clone()
        ),
        (dat_file_type::SETUP, 3, 0, 7, vec![1, 2, 3])
    );

    // A file the dat does not have: ACE's `null` branch leaves the bare opcode.
    let m = game_message_ddd_data_message(&mut w, 0x0600_0999, DatDatabaseType::Portal);
    assert_eq!(hex(&m.data), "E2F70000");
}

/// V290: the iteration file has no `DatFileType` (ACE's cast of the null type threw); it goes
/// with the type the client keys it by, 6 in the portal dat and 37 in the language dat.
#[test]
fn the_iteration_file_is_sent_with_the_type_the_client_gives_it() {
    let mut w = world();
    assert_eq!(
        ddd_manager::dat_file_get_file_type(0xFFFF_0001, DatDatabaseType::Portal),
        None
    );
    for (ty, dat_file, resource_type) in [
        (
            DatDatabaseType::Portal,
            (0, 1),
            dat_file_type::GRAPHICS_OBJECT,
        ),
        (
            DatDatabaseType::Language,
            (1, 3),
            dat_file_type::STRING_TABLE,
        ),
    ] {
        let m = game_message_ddd_data_message(&mut w, 0xFFFF_0001, ty);
        let d: DddData =
            dereth_protocol::read_body_padded(&m.data[4..]).expect("DataMessage decodes");
        assert_eq!(
            (
                (d.dat_file_type, d.dat_file_id),
                d.resource_type,
                d.resource_id,
                d.iteration,
                d.version
            ),
            (dat_file, resource_type, 0xFFFF_0001, 1, 3),
            "{ty:?}"
        );
        assert_eq!(d.data_size as usize, d.data.len() + 4);
    }
    assert_eq!(
        (dat_file_type::GRAPHICS_OBJECT, dat_file_type::STRING_TABLE),
        (6, 37)
    );
}

// ---- real content ------------------------------------------------------------------------------

/// The capture check (`--features real-content`): every recorded session in `fixtures/packet-captures`
/// shows the same DDD exchange (the server's interrogation, the client's iteration lists, the
/// server's `EndDDD`, the client's `EndDDD`). The recorded lists are decoded at run time and
/// replayed against a server on the retail dats under `DERETH_TEST_DAT_DIR`; ACE's answer, `EndDDD` and
/// nothing else, must come back, and the client's closing `EndDDD` must go unanswered. Nothing from
/// the capture is written anywhere.
#[cfg(feature = "real-content")]
mod real_content {
    use std::sync::Arc;

    use dereth_primitives::NetQueue;
    use dereth_protocol as proto;
    use dereth_protocol::admin::{DddEndDdd, DddInterrogation, DddInterrogationResponse};
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_entity::enums::AccessLevel;
    use empyrean_testkit::TestServer;

    /// The recording's DDD messages (`0xF7E2`..`0xF7EB`) in arrival order: direction and payload.
    fn recorded_ddd(name: &str) -> Vec<(bool, Vec<u8>)> {
        use dereth_client_net::client_session::testing::{Corpus, Direction};
        Corpus::shared(name)
            .blobs
            .iter()
            .filter(|blob| (0xF7E2..=0xF7EB).contains(&blob.opcode))
            .map(|blob| (blob.dir == Direction::ClientToServer, blob.payload.clone()))
            .collect()
    }

    fn opcode(payload: &[u8]) -> u32 {
        u32::from_le_bytes(payload[0..4].try_into().expect("opcode"))
    }

    #[test]
    fn the_recorded_ddd_exchange_replays_as_ace_answers_it() {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let dats: Arc<DatManager> = DatManager::initialize(Arc::new(source)).expect("retail dats");

        for name in [
            "first-login-walk-jump",
            "early-inventory-and-casting",
            "short-second-connection",
            "long-movement-run",
            "fellowship-one-vassal",
            "house-purchase-refused",
        ] {
            let recorded = recorded_ddd(name);
            let shape: Vec<(bool, u32)> =
                recorded.iter().map(|(c2s, p)| (*c2s, opcode(p))).collect();
            assert_eq!(
                shape,
                vec![
                    (false, 0xF7E5),
                    (true, 0xF7E6),
                    (false, 0xF7EA),
                    (true, 0xF7EA)
                ],
                "{name}"
            );
            let response: DddInterrogationResponse = proto::read_body_padded(&recorded[1].1[4..])
                .expect("the recorded response decodes");

            let mut ts = TestServer::with_dats(Arc::clone(&dats));
            ts.auth()
                .create_account(
                    "acct",
                    "pw",
                    AccessLevel::Player,
                    std::net::Ipv4Addr::LOCALHOST.into(),
                )
                .expect("created");
            let id = ts.connect("acct", "pw");
            assert!(
                ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()),
                "{name}: interrogation"
            );
            ts.send_message(id, NetQueue::ClientCache, &response);
            assert!(
                ts.run_until(1.0, |ts| !ts.received::<DddEndDdd>(id).is_empty()),
                "{name}: EndDDD"
            );
            ts.send_message(id, NetQueue::ClientCache, &DddEndDdd);
            ts.run_until(1.0, |_| false);
            let ours: Vec<u32> = ts
                .received_raw(id)
                .iter()
                .map(|m| m.opcode)
                .filter(|o| (0xF7E2..=0xF7EB).contains(o))
                .collect();
            assert_eq!(
                ours,
                vec![0xF7E5, 0xF7EA],
                "{name}: the server's DDD messages"
            );
            let session = ts
                .world
                .net
                .find_by_account("acct")
                .expect("still connected");
            assert!(ts
                .world
                .sessions
                .get(session)
                .is_some_and(|s| !s.begin_ddd_sent && s.ddd_data_queue.is_none()));
        }
    }

    /// The iteration list a client sends for `f`: its `0xFFFF0001` record, or, for a file from
    /// before Throne of Destiny, one run of its header iteration (`n`, then `-n, 1`).
    fn list_of(f: &dereth_dat::DatFile) -> dereth_protocol::admin::MostlyConsecutiveIntSet {
        use dereth_protocol::admin::MostlyConsecutiveIntSet;
        if let Some(n) = f.header_iteration() {
            let n = i32::try_from(n).expect("an iteration");
            return MostlyConsecutiveIntSet {
                iterations: n,
                ints: vec![-n, 1],
            };
        }
        let raw = f
            .read(dereth_dat::divine::ITERATION_LIST)
            .expect("an iteration list");
        let ints: Vec<i32> = raw
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes(c.try_into().expect("4 bytes")))
            .collect();
        MostlyConsecutiveIntSet {
            iterations: ints[0],
            ints: ints[1..].to_vec(),
        }
    }

    fn response(lists: &[(u32, u32, &dereth_dat::DatFile)]) -> DddInterrogationResponse {
        DddInterrogationResponse {
            client_language: 1,
            iters_with_keys: lists
                .iter()
                .map(|&(dat_file_type, dat_file_id, f)| {
                    dereth_protocol::admin::TaggedIterationList {
                        dat_file_type,
                        dat_file_id,
                        iterations: list_of(f),
                    }
                })
                .collect(),
            iters_without_keys: Vec::new(),
            flags: 0,
            overlay_bases: Vec::new(),
        }
    }

    /// Divergence: V414
    /// A server on the February 2005 dats compares a client's portal and cell iterations with its
    /// own and not the language list (those files have no language file): the Dereth client
    /// drawing that world (its portal and cell from February 2005, its language file the end of
    /// retail's) goes on to `DDD_EndDDD` (`0xF7EA`); a client with the end-of-retail portal and
    /// cell is still booted (`Login_AccountBooted`, `0xF7DC`), as is one that sent only the
    /// end-of-retail language list before, which the server once booted as incomplete.
    #[test]
    fn a_server_on_the_february_2005_dats_admits_a_client_drawing_that_world() {
        let old_dir = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_else(|| {
            panic!(
                "{}",
                dereth_dat::testing::pre_tod_shortfall().unwrap_or_default()
            )
        });
        let old = dereth_dat::RetailDatStore::open_pre_tod_dir(&old_dir).expect("2005 dats");
        let later = dereth_dat::RetailDatStore::open_dir(&dereth_dat::testing::dat_dir())
            .expect("the end-of-retail dats");
        let dats: Arc<DatManager> =
            DatManager::initialize(Arc::new(RealDats::open(&old_dir).expect("2005 dats")))
                .expect("2005 dats");

        let hybrid = response(&[
            (0, 1, old.portal()),
            (1, 2, old.cell()),
            (1, 3, later.local()),
        ]);
        let language_only = response(&[(1, 3, later.local())]);
        let end_of_retail = response(&[
            (0, 1, later.portal()),
            (1, 2, later.cell()),
            (1, 3, later.local()),
        ]);
        for (what, answer, admitted) in [
            ("the February 2005 world", hybrid, true),
            ("the language list alone", language_only, true),
            ("the end-of-retail world", end_of_retail, false),
        ] {
            let mut ts = TestServer::with_dats(Arc::clone(&dats));
            ts.auth()
                .create_account(
                    "acct",
                    "pw",
                    AccessLevel::Player,
                    std::net::Ipv4Addr::LOCALHOST.into(),
                )
                .expect("created");
            let id = ts.connect("acct", "pw");
            assert!(
                ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()),
                "{what}: interrogation"
            );
            ts.send_message(id, NetQueue::ClientCache, &answer);
            ts.run_until(1.0, |_| false);
            let got: Vec<u32> = ts.received_raw(id).iter().map(|m| m.opcode).collect();
            assert_eq!(got.contains(&0xF7EA), admitted, "{what}: EndDDD {got:04X?}");
            assert_eq!(
                got.contains(&0xF7DC),
                !admitted,
                "{what}: booted {got:04X?}"
            );
            if !admitted {
                // The end-of-retail client is told which files the world plays.
                let boot = ts
                    .received_raw(id)
                    .iter()
                    .find(|m| m.opcode == 0xF7DC)
                    .expect("the boot");
                let text = String::from_utf8_lossy(&boot.body).into_owned();
                assert!(text.contains("February 2005"), "{what}: {text:?}");
            }
        }
    }
}
