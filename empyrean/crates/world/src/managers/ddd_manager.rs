// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/DDDManager.cs
//! Port of `Source/ACE.Server/Managers/DDDManager.cs`: which dat files each iteration brought,
//! their sizes, and the compressed copies sent to a client that is missing iterations.
//!
//! ACE's static state is [`DddManagerState`], a field of `World`. `Initialize`
//! runs before the world exists (`Program.Main`), so it fills a state the host then moves into the
//! world.

use std::collections::{BTreeMap, HashMap};

use empyrean_common::config_manager::ConfigManager;
use empyrean_common::dotnet::{format, DotNetDict, DotNetHashSet, Num};
use empyrean_dat::{DatDatabase, DatDatabaseType, DatManager};
use empyrean_net::SessionId;

use crate::network::structure::c_mostly_consecutive_int_set::CMostlyConsecutiveIntSet;
use crate::World;

/// `Environment.NewLine` on the Windows host ACE runs on.
const NEW_LINE: &str = "\r\n";

// ACE: DDDManager.HiFi_String_As_Int
/// The int representation of a byte array from the string, HiFi, which represents the DatFileType
/// of the HighRes DAT file: `BitConverter.ToInt32(Encoding.UTF8.GetBytes("HiFi"), 0)`.
pub const HI_FI_STRING_AS_INT: i32 = i32::from_le_bytes(*b"HiFi");

/// One `DatFileSizes` value: `(int UncompressedFileSize, int CompressedFileSize)`. A
/// `compressed_file_size` of 0 means the file is sent uncompressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DatFileSize {
    pub uncompressed_file_size: i32,
    pub compressed_file_size: i32,
}

/// The parts of ACE's `DatLoader.DatFile` (a dat directory entry) that DDD reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatFile {
    /// `ObjectId`.
    pub object_id: u32,
    /// `FileSize`. ACE's `DatReader` reads exactly `FileSize` bytes, so this is the payload's
    /// length.
    pub file_size: u32,
    /// `Iteration`: the iteration that introduced this version of the file.
    pub iteration: u32,
}

// ACE: DDDManager
/// The mutable static state of ACE's `DDDManager`.
#[derive(Debug, Default)]
pub struct DddManagerState {
    // ACE: DDDManager.Debug
    pub debug: bool,
    // ACE: DDDManager.Iterations
    /// Per dat, iteration -> the files it brought. ACE's inner `ConcurrentBag` is filled in
    /// `Parallel.ForEach` order and always read through `OrderBy`, so the order here is immaterial.
    pub iterations: HashMap<DatDatabaseType, BTreeMap<u32, Vec<u32>>>,
    // ACE: DDDManager.DatFileSizes
    pub dat_file_sizes: HashMap<DatDatabaseType, HashMap<u32, DatFileSize>>,
    // ACE: DDDManager.CompressedDatFilesCache
    pub compressed_dat_files_cache: HashMap<DatDatabaseType, HashMap<u32, Vec<u8>>>,
}

impl DddManagerState {
    // ACE: DDDManager.DDDManager
    /// `static DDDManager()`: the three dictionaries, empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The missing iterations `GetMissingIterations` reports: per dat, iteration -> its files, both
/// in ascending order (`BeginDDD` enumerates the inner dictionary in insertion order).
pub type MissingIterations = HashMap<DatDatabaseType, DotNetDict<u32, Vec<u32>>>;

// ACE: DDDManager.Initialize
pub fn initialize(state: &mut DddManagerState, dats: &DatManager) {
    init_iterations(state, DatDatabaseType::Portal, dats.portal_dat());
    init_iterations(state, DatDatabaseType::Cell, dats.cell_dat());
    init_iterations(state, DatDatabaseType::Language, dats.language_dat());
    if let Some(high_res_dat) = dats.high_res_dat() {
        init_iterations(state, DatDatabaseType::HighRes, high_res_dat);
    }

    log::debug!("DDDManager Initialized.");
}

// ACE: DDDManager.InitIterations
/// ACE walks `AllFiles` in `Parallel.ForEach`; this walks them in id order. The results are the
/// same: every read of what it builds sorts first.
fn init_iterations(
    state: &mut DddManagerState,
    dat_database_type: DatDatabaseType,
    dat_database: &DatDatabase,
) {
    state.iterations.entry(dat_database_type).or_default();
    state.dat_file_sizes.entry(dat_database_type).or_default();
    state
        .compressed_dat_files_cache
        .entry(dat_database_type)
        .or_default();

    let total = dat_database.iteration();
    let mut i: i32 = 1;
    while i <= total {
        state
            .iterations
            .entry(dat_database_type)
            .or_default()
            .entry(i.cast_unsigned())
            .or_default();
        i += 1;
    }

    let precache_compressed_dat_files = ConfigManager::config().ddd.precache_compressed_dat_files;
    // Not ACE: the base's iterations, under a data overlay.
    let base_total: Option<u32> = dat_database.overlay().map(|l| {
        l.own_iterations()
            .first()
            .map_or(l.manifest().base_iterations, |first| {
                first.saturating_sub(1)
            })
    });

    let mut file_count = 0;
    for file_name in dat_database.all_files() {
        let file_iter = dat_database.file_iteration(file_name).unwrap_or_default();

        state
            .iterations
            .entry(dat_database_type)
            .or_default()
            .entry(file_iter)
            .or_default();

        // Not ACE: a `FakeDats` decoded object has no bytes; it counts as an empty file.
        let dat_file = dat_database
            .get_reader_for_file(file_name)
            .unwrap_or_default();
        let uncompressed_file_size = len_i32(&dat_file);
        // DIVERGE (V437): under a data overlay, a base file (one at or below the base's
        // iterations) is what every client patched to the world already holds; it is not
        // compressed ahead of time, and is sent as it is if it is ever sent.
        if base_total.is_some_and(|b| file_iter <= b) {
            state
                .iterations
                .entry(dat_database_type)
                .or_default()
                .entry(file_iter)
                .or_default()
                .push(file_name);
            file_count += 1;
            state
                .dat_file_sizes
                .entry(dat_database_type)
                .or_default()
                .entry(file_name)
                .or_insert(DatFileSize {
                    uncompressed_file_size,
                    compressed_file_size: 0,
                });
            continue;
        }
        let compressed_dat_file = compress(&dat_file);
        let compressed_file_size = len_i32(&compressed_dat_file);
        let use_compressed_file = compressed_file_size.wrapping_add(4) < uncompressed_file_size;
        let file_size_to_send = if use_compressed_file {
            compressed_file_size
        } else {
            0
        };

        state
            .iterations
            .entry(dat_database_type)
            .or_default()
            .entry(file_iter)
            .or_default()
            .push(file_name);
        file_count += 1;
        state
            .dat_file_sizes
            .entry(dat_database_type)
            .or_default()
            .entry(file_name)
            .or_insert(DatFileSize {
                uncompressed_file_size,
                compressed_file_size: file_size_to_send,
            });

        if use_compressed_file && precache_compressed_dat_files {
            state
                .compressed_dat_files_cache
                .entry(dat_database_type)
                .or_default()
                .entry(file_name)
                .or_insert_with(|| {
                    prepend_uncompressed_file_size(
                        &compressed_dat_file,
                        uncompressed_file_size.cast_unsigned(),
                    )
                });
        }
    }
    log::info!(
        "Iterations for {:?} initialized. Iterations.Count={} | FileCount={} | DatFileSizes.Count={}{}",
        dat_database_type,
        state.iterations[&dat_database_type].len(),
        file_count,
        state.dat_file_sizes[&dat_database_type].len(),
        if precache_compressed_dat_files { " | precached Compressed files" } else { "" }
    );
}

/// `(int)array.Length`.
fn len_i32(bytes: &[u8]) -> i32 {
    i32::try_from(bytes.len()).unwrap_or(i32::MAX)
}

// ACE: DDDManager.Compress
/// Compresses data with ZLib: `ZLibStream` at `CompressionLevel.SmallestSize` (zlib level 9,
/// 15-bit window, memory level 8, default strategy), which .NET 10 runs on zlib-ng. zlib-rs is a
/// port of zlib-ng; the `ddd` vectors hold ACE's own output.
///
/// # Panics
/// If deflate reports an error, which it cannot for a buffer of `compressBound` bytes.
#[must_use]
pub fn compress(data: &[u8]) -> Vec<u8> {
    // `CopyTo` writes nothing for an empty input, and a `ZLibStream` that was never written to
    // closes without emitting anything, not even the zlib header.
    if data.is_empty() {
        return Vec::new();
    }
    let mut output = vec![0u8; zlib_rs::compress_bound(data.len())];
    let (compressed, rc) = zlib_rs::compress_slice(
        &mut output,
        data,
        zlib_rs::DeflateConfig::best_compression(),
    );
    assert!(rc == zlib_rs::ReturnCode::Ok, "zlib deflate failed: {rc:?}");
    let n = compressed.len();
    output.truncate(n);
    output
}

// ACE: DDDManager.PrependUncompressedFileSize
/// Combines the uncompressed file size with the compressed data for transmission to client.
#[must_use]
pub fn prepend_uncompressed_file_size(
    compressed_data: &[u8],
    uncompressed_file_size: u32,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(4 + compressed_data.len());
    output.extend_from_slice(&uncompressed_file_size.to_le_bytes());
    output.extend_from_slice(compressed_data);
    output
}

/// `DatManager.PortalDat` / `CellDat` / `LanguageDat` / `HighResDat` by type.
fn dat_database(dats: &DatManager, dat_database_type: DatDatabaseType) -> Option<&DatDatabase> {
    match dat_database_type {
        DatDatabaseType::Portal => Some(dats.portal_dat()),
        DatDatabaseType::Cell => Some(dats.cell_dat()),
        DatDatabaseType::Language => Some(dats.language_dat()),
        DatDatabaseType::HighRes => dats.high_res_dat(),
    }
}

/// `datDatabase.AllFiles.TryGetValue(datFileId, out datFile)`.
#[must_use]
pub fn try_get_dat_file(dat_database: &DatDatabase, dat_file_id: u32) -> Option<DatFile> {
    let iteration = dat_database.file_iteration(dat_file_id)?;
    let file_size = dat_database
        .get_reader_for_file(dat_file_id)
        .map_or(0, |b| len_i32(&b).cast_unsigned());
    Some(DatFile {
        object_id: dat_file_id,
        file_size,
        iteration,
    })
}

/// What `TryGetDatFileContentsForTransmission` hands back: the contents, the `DatFile` and
/// `isCompressed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatFileContents {
    pub contents: Vec<u8>,
    pub dat_file: DatFile,
    pub is_compressed: bool,
}

// ACE: DDDManager.TryGetDatFileContentsForTransmission
/// Tries to search for and return the file contents for a DatFileId of the specified DAT file,
/// suitable for transmission to client. `None` (ACE's `null`) when the file is not found.
///
/// # Panics
/// Where ACE throws: `HighRes` without a high-res dat (`NullReferenceException`), or a file the
/// manager never initialised (`KeyNotFoundException`, as when `Initialize` did not run).
pub fn try_get_dat_file_contents_for_transmission(
    state: &mut DddManagerState,
    dats: &DatManager,
    dat_file_id: u32,
    dat_database_type: DatDatabaseType,
) -> Option<DatFileContents> {
    let dat_database = dat_database(dats, dat_database_type)
        .expect("NullReferenceException: DatManager.HighResDat is null");

    let dat_file = try_get_dat_file(dat_database, dat_file_id)?;

    let cached_dat_file_sizes = state
        .dat_file_sizes
        .get(&dat_database_type)
        .and_then(|s| s.get(&dat_file_id))
        .copied()
        .expect("KeyNotFoundException: DDDManager.DatFileSizes");

    let compress_dat_file = cached_dat_file_sizes.compressed_file_size > 0;

    if compress_dat_file {
        let cache = state
            .compressed_dat_files_cache
            .get_mut(&dat_database_type)
            .expect("KeyNotFoundException: DDDManager.CompressedDatFilesCache");
        if let Some(compressed_data) = cache.get(&dat_file_id) {
            return Some(DatFileContents {
                contents: compressed_data.clone(),
                dat_file,
                is_compressed: true,
            });
        }

        let buffer = dat_database
            .get_reader_for_file(dat_file_id)
            .unwrap_or_default();
        let compressed_data =
            prepend_uncompressed_file_size(&compress(&buffer), dat_file.file_size);

        cache
            .entry(dat_file_id)
            .or_insert_with(|| compressed_data.clone());

        Some(DatFileContents {
            contents: compressed_data,
            dat_file,
            is_compressed: true,
        })
    } else {
        let buffer = dat_database
            .get_reader_for_file(dat_file_id)
            .unwrap_or_default();
        Some(DatFileContents {
            contents: buffer,
            dat_file,
            is_compressed: false,
        })
    }
}

/// The public `GetMissingIterations`'s return value and its two `out`s.
#[derive(Debug, Clone, Default)]
pub struct MissingIterationsResult {
    pub total_missing_iterations: u32,
    pub total_file_size: u32,
    pub iterations: MissingIterations,
}

// ACE: DDDManager.GetMissingIterations
/// The iterations each dat is missing, their files, and the bytes the download will take.
///
/// # Panics
/// As `GetMissingIterationsFromClient`.
#[must_use]
pub fn get_missing_iterations(
    state: &DddManagerState,
    client_portal_dat_int_set: &CMostlyConsecutiveIntSet,
    client_cell_dat_int_set: &CMostlyConsecutiveIntSet,
    client_language_dat_int_set: &CMostlyConsecutiveIntSet,
    client_high_res_dat_int_set: &CMostlyConsecutiveIntSet,
) -> MissingIterationsResult {
    let mut r = MissingIterationsResult::default();

    get_missing_iterations_for(
        state,
        DatDatabaseType::Portal,
        client_portal_dat_int_set,
        &mut r,
    );
    get_missing_iterations_for(
        state,
        DatDatabaseType::Cell,
        client_cell_dat_int_set,
        &mut r,
    );
    get_missing_iterations_for(
        state,
        DatDatabaseType::Language,
        client_language_dat_int_set,
        &mut r,
    );
    get_missing_iterations_for(
        state,
        DatDatabaseType::HighRes,
        client_high_res_dat_int_set,
        &mut r,
    );

    r
}

/// The private overload of `GetMissingIterations`, for one dat (`ref totalFileSize`,
/// `iterations`, `ref totalMissingIterations` are `r`'s fields).
fn get_missing_iterations_for(
    state: &DddManagerState,
    dat_database_type: DatDatabaseType,
    client_dat_iterations: &CMostlyConsecutiveIntSet,
    r: &mut MissingIterationsResult,
) {
    let Some(server_iterations) = state.iterations.get(&dat_database_type) else {
        return;
    };

    // if either CELL or HIGHRES dat files report 0 for iterations, that's okay. CELL will download on demand and HIGHRES will not be used.
    if (dat_database_type == DatDatabaseType::Cell || dat_database_type == DatDatabaseType::HighRes)
        && client_dat_iterations.iterations == 0
    {
        return;
    }

    // Generate a list of the missing iterations the client may have
    let missing_iterations =
        get_missing_iterations_from_client(state, dat_database_type, client_dat_iterations);

    // If the list is empty, we all good here!
    if missing_iterations.is_empty() {
        return;
    }

    // Get all the files for the iterations we are missing
    let x: Vec<(&u32, &Vec<u32>)> = server_iterations
        .iter()
        .filter(|(k, _)| missing_iterations.contains(k))
        .collect();

    if !x.is_empty() {
        let mut compressed_files: u32 = 0;
        let mut uncompressed_files: u32 = 0;
        let sizes = state.dat_file_sizes.get(&dat_database_type);
        let dat_iterations = r.iterations.entry(dat_database_type).or_default();
        for (&key, value) in x {
            r.total_missing_iterations = r.total_missing_iterations.wrapping_add(1);
            dat_iterations.try_add(key, Vec::new());
            let mut files = value.clone();
            files.sort_unstable();
            for z in files {
                dat_iterations.get_mut(&key).expect("added above").push(z);

                if dat_database_type != DatDatabaseType::Cell {
                    let size = sizes
                        .and_then(|s| s.get(&z))
                        .copied()
                        .expect("KeyNotFoundException: DDDManager.DatFileSizes");
                    if size.compressed_file_size > 0 {
                        compressed_files = compressed_files.wrapping_add(1);
                        r.total_file_size = r
                            .total_file_size
                            .wrapping_add(size.compressed_file_size.cast_unsigned());
                    } else {
                        uncompressed_files = uncompressed_files.wrapping_add(1);
                        r.total_file_size = r
                            .total_file_size
                            .wrapping_add(size.uncompressed_file_size.cast_unsigned());
                    }
                } else {
                    // do nothing, files from Cell DAT are not included in totalFileSize calculations because these files are requested/sent on demand and not part of initial patching.
                }
            }
        }
        r.total_file_size = r
            .total_file_size
            .wrapping_add(compressed_files.wrapping_mul(4));
        //totalFileSize += (uint)uncompressedFiles * 4;
        let _ = uncompressed_files;
    }
}

/// `{value:####}`: the digits, or nothing for zero.
fn hash4(value: impl Into<Num>) -> String {
    format(value, "####")
}

// ACE: DDDManager.GetMissingIterationsFromClient
/// Helper function to parse an Iteration list received from the client on login and return any
/// missing iterations it may have.
///
/// Not ACE's (a fix, V290): the list is read exactly as the client writes it.
/// The client sorts its iterations and writes each run of three or more consecutive ones as the
/// negated length followed by the first iteration (`-n, first` covers `first .. first + n - 1`),
/// and every other iteration on its own as a positive entry; `-total, 1` is the client that has
/// every iteration. ACE instead ended the parse with "nothing missing" at any entry whose
/// absolute value equalled the dat's total, so a lone `total` (the client has the latest
/// iteration but not the ones before it) read as up to date and was sent an empty `BeginDDD`
/// and no files. A run length with no first iteration after it (not something the client
/// writes) is ignored, as ACE ignored it; a zero entry is no iteration.
///
/// # Panics
/// Where ACE throws: a dat with no iterations at all (`Keys.Max()` of an empty set), or an entry of
/// `int.MinValue` (`Math.Abs` overflows).
fn get_missing_iterations_from_client(
    state: &DddManagerState,
    dat_database_type: DatDatabaseType,
    client_iterations: &CMostlyConsecutiveIntSet,
) -> Vec<u32> {
    let mut all_iterations: DotNetHashSet<u32> = DotNetHashSet::new();

    // Highest key will be the total iterations. We already know the datDatabaseType exists from an earlier check.
    let total_iterations = *state.iterations[&dat_database_type]
        .keys()
        .max()
        .expect("InvalidOperationException: Sequence contains no elements");

    // Store all the possible Iterations here
    for i in 1..=total_iterations {
        all_iterations.insert(i);
    }

    let db_file = match dat_database_type {
        DatDatabaseType::Portal => "client_portal.dat",
        DatDatabaseType::Cell => "client_cell_1.dat",
        DatDatabaseType::Language => "client_Local_English.dat",
        DatDatabaseType::HighRes => "client_highres.dat",
    };
    let mut debug_str = format!("{db_file}{NEW_LINE}Completed Iterations:{NEW_LINE}");

    // Going to remove all the Iterations the client has
    let mut iterations: i32 = 0;
    let mut entries = client_iterations.ints.iter().copied();
    while let Some(ints) = entries.next() {
        if ints < 0 {
            // A run: its length, then its first iteration.
            let length = i64::from(math_abs(ints));
            let Some(range_start) = entries.next() else {
                break;
            };
            let range_end = i64::from(range_start) + length;
            let mut i = i64::from(range_start);
            while i < range_end {
                if let Ok(iteration) = u32::try_from(i) {
                    if all_iterations.remove(&iteration) {
                        iterations += 1;
                    }
                }
                i += 1;
            }
            if state.debug {
                debug_str += &format!(
                    "  |    {} - {}{NEW_LINE}",
                    hash4(range_start),
                    hash4(range_end - 1)
                );
            }
            continue;
        }

        if all_iterations.remove(&ints.cast_unsigned()) {
            iterations += 1;

            if state.debug {
                //debugStr += $"  |    {ints:####} - {ints:####}" + Environment.NewLine;
                debug_str += &format!("  |    {}{NEW_LINE}", hash4(ints));
            }
        }
    }

    if state.debug {
        empyrean_common::console_write_line!(
            debug: "{debug_str}{NEW_LINE}Total Completed Iterations: {iterations} of {total_iterations} ({}){NEW_LINE}",
            format(f64::from(iterations) / f64::from(total_iterations), "P2")
        );
    }

    // The missing Iterations are what is left
    let missing: Vec<u32> = all_iterations.iter().copied().collect();

    if state.debug {
        let mut debug_str = format!("{db_file}{NEW_LINE}Missing Iterations:{NEW_LINE}");
        debug_str += &show(&ranges(&missing));
        let count = u32::try_from(all_iterations.len()).unwrap_or(u32::MAX);
        empyrean_common::console_write_line!(
            debug: "{debug_str}{NEW_LINE}{NEW_LINE}Total Missing Iterations: {count} of {total_iterations} ({}){NEW_LINE}",
            format(f64::from(count) / f64::from(total_iterations), "P2")
        );
    }

    missing
}

/// `Math.Abs(int)`, which throws on `int.MinValue`.
fn math_abs(value: i32) -> i32 {
    value.checked_abs().expect(
        "OverflowException: Negating the minimum value of a twos complement number is invalid.",
    )
}

// ACE: DDDManager.Ranges
/// Runs of consecutive numbers as `(begin, end)` with `end` exclusive, in input order.
#[must_use]
pub fn ranges(nums: &[u32]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut e = nums.iter().copied();
    if let Some(first) = e.next() {
        let mut begin = first;
        let mut end = begin.wrapping_add(1);
        for current in e {
            if current != end {
                out.push((begin, end));
                begin = current;
                end = current;
            }
            end = end.wrapping_add(1);
        }
        out.push((begin, end));
    }
    out
}

// ACE: DDDManager.Show
/// One line per range, joined with `Environment.NewLine`.
#[must_use]
pub fn show(ranges: &[(u32, u32)]) -> String {
    //return "[" + string.Join(",", ranges.Select(r => r.end - r.begin == 1 ? $"{r.begin}" : $"{r.begin}-{r.end - 1}")) + "]";
    ranges
        .iter()
        .map(|&(begin, end)| {
            if end.wrapping_sub(begin) == 1 {
                format!("  |    {begin}")
            } else {
                format!("  |    {} - {}", hash4(begin), hash4(end.wrapping_sub(1)))
            }
        })
        .collect::<Vec<_>>()
        .join(NEW_LINE)
}

// ACE: DDDManager.AddToQueue
/// Queues a file for the session's `ProcessDDDQueue`. `false` only when the session has no game
/// half (not ACE: a `Session` there always exists).
pub fn add_to_queue(
    w: &mut World,
    session: SessionId,
    dat_file_id: u32,
    dat_database_type: DatDatabaseType,
) -> bool {
    //dddDataQueue.Enqueue((session, datFileId, datDatabaseType));
    //session.AddToDDDQueue(datFileId, datDatabaseType);

    w.sessions
        .get_mut(session)
        .is_some_and(|s| s.add_to_ddd_queue(dat_file_id, dat_database_type))
}

/// `DatFile.GetFileType(datDatabaseType)` (`ACE.DatLoader`, which the shared dat crate does not
/// expose): the `DatFileType` a file id has in a dat, or `None` (after ACE's console line) for an
/// id it does not know.
#[must_use]
pub fn dat_file_get_file_type(object_id: u32, dat_database_type: DatDatabaseType) -> Option<u32> {
    use dat_file_type as t;
    if dat_database_type == DatDatabaseType::Cell {
        if object_id & 0xFFFF == 0xFFFF {
            return Some(t::LAND_BLOCK);
        }

        if object_id & 0xFFFF == 0xFFFE {
            return Some(t::LAND_BLOCK_INFO);
        }

        return Some(t::ENV_CELL);
    }

    if dat_database_type == DatDatabaseType::Portal {
        let by_high_byte = match object_id >> 24 {
            0x01 => Some(t::GRAPHICS_OBJECT),
            0x02 => Some(t::SETUP),
            0x03 => Some(t::ANIMATION),
            0x04 => Some(t::PALETTE),
            0x05 => Some(t::SURFACE_TEXTURE),
            0x06 => Some(t::TEXTURE),
            0x08 => Some(t::SURFACE),
            0x09 => Some(t::MOTION_TABLE),
            0x0A => Some(t::WAVE),
            0x0D => Some(t::ENVIRONMENT),
            0x0F => Some(t::PALETTE_SET),
            0x10 => Some(t::CLOTHING),
            0x11 => Some(t::DEGRADE_INFO),
            0x12 => Some(t::SCENE),
            0x13 => Some(t::REGION),
            0x14 => Some(t::KEY_MAP),
            0x15 => Some(t::RENDER_TEXTURE),
            0x16 => Some(t::RENDER_MATERIAL),
            0x17 => Some(t::MATERIAL_MODIFIER),
            0x18 => Some(t::MATERIAL_INSTANCE),
            0x20 => Some(t::SOUND_TABLE),
            0x22 => Some(t::ENUM_MAPPER),
            0x25 => Some(t::DID_MAPPER),
            0x26 => Some(t::ACTION_MAP),
            0x27 => Some(t::DUAL_DID_MAPPER),
            0x30 => Some(t::COMBAT_TABLE),
            0x31 => Some(t::STRING),
            0x32 => Some(t::PARTICLE_EMITTER),
            0x33 => Some(t::PHYSICS_SCRIPT),
            0x34 => Some(t::PHYSICS_SCRIPT_TABLE),
            0x39 => Some(t::MASTER_PROPERTY),
            0x40 => Some(t::FONT),
            0x78 => Some(t::DB_PROPERTIES),
            _ => None,
        };
        if by_high_byte.is_some() {
            return by_high_byte;
        }

        match object_id >> 16 {
            0x0E01 => return Some(t::QUALITY_FILTER),
            0x0E02 => return Some(t::MONITORED_PROPERTIES),
            _ => {}
        }

        let by_id = match object_id {
            0x0E00_0002 => Some(t::CHARACTER_GENERATOR),
            0x0E00_0003 => Some(t::SECONDARY_ATTRIBUTE_TABLE),
            0x0E00_0004 => Some(t::SKILL_TABLE),
            0x0E00_0007 => Some(t::CHAT_POSE_TABLE),
            0x0E00_000D => Some(t::OBJECT_HIERARCHY),
            0x0E00_000E => Some(t::SPELL_TABLE),
            0x0E00_000F => Some(t::SPELL_COMPONENT_TABLE),
            0x0E00_0018 => Some(t::XP_TABLE),
            0x0E00_001A => Some(t::BAD_DATA),
            0x0E00_001D => Some(t::CONTRACT_TABLE),
            0x0E00_001E => Some(t::TABOO_TABLE),
            0x0E00_001F => Some(t::FILE_TO_ID),
            0x0E00_0020 => Some(t::NAME_FILTER_TABLE),
            _ => None,
        };
        if by_id.is_some() {
            return by_id;
        }
    }

    if dat_database_type == DatDatabaseType::Language {
        match object_id >> 24 {
            0x21 => return Some(t::UI_LAYOUT),
            0x23 => return Some(t::STRING_TABLE),
            0x41 => return Some(t::STRING_STATE),
            _ => {}
        }
    }

    if dat_database_type == DatDatabaseType::HighRes && object_id >> 24 == 0x06 {
        return Some(t::TEXTURE);
    }

    empyrean_common::console_write_line!("Unknown file type: {object_id:08X}");
    None
}

/// ACE's `DatLoader.DatFileType` values (`uint`).
pub mod dat_file_type {
    macro_rules! dat_file_types {
        ($($name:ident = $value:expr, $text:literal;)*) => {
            $(#[doc = $text] pub const $name: u32 = $value;)*

            /// `DatFileType.ToString()`: the member name, or the number for an undefined value.
            #[must_use]
            pub fn name(value: u32) -> String {
                match value {
                    $($name => $text.to_owned(),)*
                    _ => value.to_string(),
                }
            }
        };
    }

    dat_file_types! {
        LAND_BLOCK = 1, "LandBlock";
        LAND_BLOCK_INFO = 2, "LandBlockInfo";
        ENV_CELL = 3, "EnvCell";
        LAND_BLOCK_OBJECTS = 4, "LandBlockObjects";
        INSTANTIATION = 5, "Instantiation";
        GRAPHICS_OBJECT = 6, "GraphicsObject";
        SETUP = 7, "Setup";
        ANIMATION = 8, "Animation";
        ANIMATION_HOOK = 9, "AnimationHook";
        PALETTE = 10, "Palette";
        SURFACE_TEXTURE = 11, "SurfaceTexture";
        TEXTURE = 12, "Texture";
        SURFACE = 13, "Surface";
        MOTION_TABLE = 14, "MotionTable";
        WAVE = 15, "Wave";
        ENVIRONMENT = 16, "Environment";
        CHAT_POSE_TABLE = 17, "ChatPoseTable";
        OBJECT_HIERARCHY = 18, "ObjectHierarchy";
        BAD_DATA = 19, "BadData";
        TABOO_TABLE = 20, "TabooTable";
        FILE_TO_ID = 21, "FileToId";
        NAME_FILTER_TABLE = 22, "NameFilterTable";
        MONITORED_PROPERTIES = 23, "MonitoredProperties";
        PALETTE_SET = 24, "PaletteSet";
        CLOTHING = 25, "Clothing";
        DEGRADE_INFO = 26, "DegradeInfo";
        SCENE = 27, "Scene";
        REGION = 28, "Region";
        KEY_MAP = 29, "KeyMap";
        RENDER_TEXTURE = 30, "RenderTexture";
        RENDER_MATERIAL = 31, "RenderMaterial";
        MATERIAL_MODIFIER = 32, "MaterialModifier";
        MATERIAL_INSTANCE = 33, "MaterialInstance";
        SOUND_TABLE = 34, "SoundTable";
        UI_LAYOUT = 35, "UiLayout";
        ENUM_MAPPER = 36, "EnumMapper";
        STRING_TABLE = 37, "StringTable";
        DID_MAPPER = 38, "DidMapper";
        ACTION_MAP = 39, "ActionMap";
        DUAL_DID_MAPPER = 40, "DualDidMapper";
        STRING = 41, "String";
        PARTICLE_EMITTER = 42, "ParticleEmitter";
        PHYSICS_SCRIPT = 43, "PhysicsScript";
        PHYSICS_SCRIPT_TABLE = 44, "PhysicsScriptTable";
        MASTER_PROPERTY = 45, "MasterProperty";
        FONT = 46, "Font";
        FONT_LOCAL = 47, "FontLocal";
        STRING_STATE = 48, "StringState";
        DB_PROPERTIES = 49, "DbProperties";
        RENDER_MESH = 67, "RenderMesh";
        WEENIE_DEFAULTS = 0x1000_0001, "WeenieDefaults";
        CHARACTER_GENERATOR = 0x1000_0002, "CharacterGenerator";
        SECONDARY_ATTRIBUTE_TABLE = 0x1000_0003, "SecondaryAttributeTable";
        SKILL_TABLE = 0x1000_0004, "SkillTable";
        SPELL_TABLE = 0x1000_0005, "SpellTable";
        SPELL_COMPONENT_TABLE = 0x1000_0006, "SpellComponentTable";
        TREASURE_TABLE = 0x1000_0007, "TreasureTable";
        CRAFT_TABLE = 0x1000_0008, "CraftTable";
        XP_TABLE = 0x1000_0009, "XpTable";
        QUESTS = 0x1000_000A, "Quests";
        GAME_EVENT_TABLE = 0x1000_000B, "GameEventTable";
        QUALITY_FILTER = 0x1000_000C, "QualityFilter";
        COMBAT_TABLE = 0x1000_000D, "CombatTable";
        ITEM_MUTATION = 0x1000_000E, "ItemMutation";
        CONTRACT_TABLE = 0x1000_0010, "ContractTable";
    }
}
