// Ported from ac1999 (Adam Plumb), AGPL-3.0: DatInspector/patching/closure.py and the dat migrations in DatInspector/patching/migrations/release/ (0003 to 0007 here; 0001, 0002 and 0008 in empyrean-dat's launch_spell_fixes)
//! `empyrean-import ac1999 dat-overlay`: the data overlay ac1999's dat migrations make of the 1999
//! files at one exact commit, for Empyrean to serve to Dereth on a Launch world.
//!
//! ```text
//! empyrean-import ac1999 dat-overlay --revision <sha> --base <dir> --out <dir> --world-key <name>
//!     [--source <repository>] [--with-events --reference <file>...]
//! ```
//!
//! The list of migrations comes from the commit's Git objects, and every file on it is pinned by its
//! SHA-256. Nothing of ac1999's runs: each migration here is a reviewed port, so a commit naming one
//! that is new or changed is refused, saying which. `--base` is the operator's Launch pair, checked
//! by hash: the October 31, 1999 `portal.dat` and the retail CD's `cell.dat`.
//!
//! ac1999 builds on the retail CD's portal, and Launch on the one of October 31, the same file but
//! for a world event's records, so the event's migrations work out differently here:
//!
//! | Migration | What it does | Needs | Here |
//! |---|---|---|---|
//! | 0001 | Enchantments last by their level | the base | the rule of [`launch_spell_fixes`] |
//! | 0002 | Fixes to spell formulas, ranges, power and text | the base | those rules, and the formulas and text |
//! | 0003 | Banderlings gain a casting motion | the base | [`STANCES`] |
//! | 0004 | The event begins, on both files | | the base has its portal records; its cell records go dark |
//! | 0005 | One burning town, in the cell file | | 0006 undoes all of it |
//! | 0006 | The event ends | the retail CD's portal | the eleven records the base has as the event left them |
//! | 0007 | Assets of four holiday objects | the September 12, 2001 portal | the same walk through its records |
//! | 0008 | Imperil Other can be resisted | the base | the rule of [`launch_spell_fixes`] |
//!
//! The migrations numbered 0004 to 0007 ("the events") are left out unless `--with-events` asks for
//! them. They need files beyond the Launch pair, named with `--reference`, and the command refuses
//! without them. The output holds retail records: it belongs on the operator's own machine and in no
//! release.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dereth_dat::overlay::{self, OverlayDir, OverlayWriter};
use dereth_dat::{decompose, DatFile, ModernDat};
use dereth_primitives::DataId;
use empyrean_dat::launch_spell_fixes::{self, TableFixes};
use serde_json::json;

use super::cells::{self, DatIdentity};
use super::source;

const MIGRATIONS: &str = "DatInspector/patching/migrations/release";

/// What each migration does here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Durations,
    SpellFixes,
    BanderlingCasts,
    EventBegins,
    EventTown,
    EventEnds,
    HolidayAssets,
    ResistableImperil,
}

impl Step {
    /// Whether the step is one of the world events', which need more than the Launch pair.
    fn is_event(self) -> bool {
        matches!(
            self,
            Self::EventBegins | Self::EventTown | Self::EventEnds | Self::HolidayAssets
        )
    }
}

/// The migrations reviewed and ported, each as committed.
const REVIEWED: [(&str, &str, Step); 8] = [
    (
        "0001-enchantment-durations.py",
        "dd42a3bb40a5c3a4a14cce15939b45942f01675e9d572df0501de28c8fc71ea8",
        Step::Durations,
    ),
    (
        "0002-it160-spell-fixes.py",
        "9d223eb88de3982c8794b2fda46a82ba95c6810e5a3d7a2e20512caefe67ddf9",
        Step::SpellFixes,
    ),
    (
        "0003-banderling-cast-animation.py",
        "fe3fd355d80fae6909064a4306f04a63ade03fc391ec5ccf9c354ed7d2145bac",
        Step::BanderlingCasts,
    ),
    (
        "0004-fire-in-the-sky.py",
        "11076af331dca7af0c7887e183033fb78658ec6240f37f8c3ec6aec40b20c3d9",
        Step::EventBegins,
    ),
    (
        "0005-glenden-wood-burns.py",
        "80f4c02b4ea7f352c6b6eda5da32563354bce0b8ece58656f4e4cba3649e0269",
        Step::EventTown,
    ),
    (
        "0006-fire-in-the-sky-ends.py",
        "0f6d55b5eb2d362ab3647a7b13b4fbfafcf0a560bfed06913b76474fa9f8cd17",
        Step::EventEnds,
    ),
    (
        "0007-halloween-assets.py",
        "491b8143ff39b3fcc3e6f7c93e871e0e4184b939c3c95579cf3aefec34a9ddee",
        Step::HolidayAssets,
    ),
    (
        "0008-resistable-imperil.py",
        "ec4527ddd7ed70724f594fdd556d3e16e44398ea21d23c8bdcf195c7eb952077",
        Step::ResistableImperil,
    ),
];

/// What gives a migration its meaning beside its own file, as reviewed.
const REVIEWED_TOOLING: [(&str, &str); 3] = [
    (
        "DatInspector/patching/dat_patch.py",
        "f3c985d49c8ccaa0bd4937f53b5a2f47e918605e82d1301e1b1c18a8900206ce",
    ),
    (
        "DatInspector/patching/spell_patch.py",
        "f56d4fe6dcc9b4770dc3bd5d721699dafe4949506a208e8fdac4c54ff178aa49",
    ),
    (
        "DatInspector/patching/closure.py",
        "c84009430cac4700ff4004e6679f0c63bd14fad5f8023a09379803a63685181f",
    ),
];

/// The September 12, 2001 `portal.dat`, which the holiday objects come from.
const PORTAL_2001_09_12: DatIdentity = DatIdentity {
    capture: "2001-09-12",
    iteration: 541,
    sha256: "effc3d12130c2ca123b878306c3fca5934cda819ad956fc6883b307677aff3ca",
};

/// The migrations of a commit, in order, from what it holds: a file name's SHA-256 by its path.
/// Refuses anything not reviewed as committed, and a commit in the middle of the event, which only
/// its end makes sense of here.
fn plan(committed: &BTreeMap<String, String>) -> Result<Vec<Step>, String> {
    for (path, sha256) in REVIEWED_TOOLING {
        if committed.get(path).is_some_and(|found| found != sha256) {
            return Err(format!(
                "{path} isn't the one reviewed: the migrations may mean something else now, so review them against it"
            ));
        }
    }
    let mut held = Vec::new();
    for (path, sha256) in committed {
        let Some(file) = path
            .strip_prefix(MIGRATIONS)
            .and_then(|p| p.strip_prefix('/'))
        else {
            continue;
        };
        match REVIEWED.iter().find(|(name, _, _)| *name == file) {
            None => {
                return Err(format!(
                    "migration {file} hasn't been reviewed: this commit can't be distilled until it is ported"
                ));
            }
            Some((_, reviewed, _)) if reviewed != sha256 => {
                return Err(format!(
                    "migration {file} has changed since it was reviewed (sha256 {sha256})"
                ));
            }
            Some((_, _, step)) => held.push(*step),
        }
    }
    let steps: Vec<Step> = REVIEWED
        .iter()
        .map(|(_, _, step)| *step)
        .filter(|step| held.contains(step))
        .collect();
    let event = [Step::EventBegins, Step::EventTown, Step::EventEnds];
    let of_event = event.iter().filter(|s| steps.contains(s)).count();
    if of_event != 0 && of_event != event.len() {
        return Err(
            "this commit has the event's migrations only in part: only their end was reviewed"
                .into(),
        );
    }
    Ok(steps)
}

const SPELL_TABLE: DataId = DataId(empyrean_dat::file_id::SPELL_TABLE);

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("the record ends before offset {at}"))
}

const BANDERLING_MOTION_TABLE: DataId = DataId(0x0900_0007);
const MOTION_CAST_SPELL: u32 = 0x4000_00D3;
const MOTION_READY: u32 = 0x4100_0003;
/// The link list new casting links go in front of: from a falling motion, in the thrown stance.
const LINKS_AFTER_CAST: u32 = (0x47 << 16) | 0x15;
/// The Banderling table after the casting motion is added.
const BANDERLING_MOTION_TABLE_SHA256: &str =
    "b21d442b510625406f8223ab24d146b490fef97cda83e8dc504902b4106e5842";

/// One animation of a motion: its id, frames and frame rate.
type Anim = (u32, i32, i32, f32);
/// Raises the arms.
const RAISE: Anim = (0x0300_062D, 0, 28, 30.0);
/// Holds them there.
const HOLD: Anim = (0x0300_062D, 29, -1, 0.0);
/// Lets them down.
const LOWER: Anim = (0x0300_062D, 30, -1, 30.0);
/// From the two-handed stances, what comes before raising the arms and after lowering them.
const SETTLE_IN: Anim = (0x0300_056A, 0, -1, -30.0);
const SETTLE_OUT: Anim = (0x0300_056A, 0, -1, 30.0);

/// A stance a Banderling casts from.
struct Stance {
    /// The stance's motion style, in the high half of a table key.
    style: u32,
    /// Where casting goes in the list of what follows standing ready.
    at: usize,
    to_cast: &'static [Anim],
    to_ready: &'static [Anim],
}

const STANCES: [Stance; 4] = [
    Stance {
        style: 0x47,
        at: 5,
        to_cast: &[SETTLE_IN, RAISE],
        to_ready: &[LOWER, SETTLE_OUT],
    },
    Stance {
        style: 0x3E,
        at: 4,
        to_cast: &[SETTLE_IN, RAISE],
        to_ready: &[LOWER, SETTLE_OUT],
    },
    Stance {
        style: 0x3C,
        at: 5,
        to_cast: &[RAISE],
        to_ready: &[LOWER],
    },
    Stance {
        style: 0x3D,
        at: 5,
        to_cast: &[RAISE],
        to_ready: &[LOWER],
    },
];

/// One motion's bytes: its id, the number of animations, two empty flag bytes and a pad, then each.
fn motion_bytes(id: u32, anims: &[Anim]) -> Vec<u8> {
    let mut out = id.to_le_bytes().to_vec();
    out.extend_from_slice(&[u8::try_from(anims.len()).unwrap_or(0), 0, 0, 0]);
    for (anim, low, high, rate) in anims {
        out.extend_from_slice(&anim.to_le_bytes());
        out.extend_from_slice(&low.to_le_bytes());
        out.extend_from_slice(&high.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
    }
    out
}

/// Past a motion at `at`: its id, animations and optional velocity and turning.
fn past_motion(bytes: &[u8], at: usize) -> Result<usize, String> {
    let head = read_u32(bytes, at + 4)?;
    let anims = usize::try_from(head & 0xFF).map_err(|e| e.to_string())?;
    let flags = (head >> 16) & 0xFF;
    let mut end = at + 8 + 16 * anims;
    if flags & 1 != 0 {
        end += 12;
    }
    if flags & 2 != 0 {
        end += 12;
    }
    Ok(end)
}

/// The Banderling motion table `bytes` with the casting motion in every stance it casts from: the
/// cycle holds its arms up, and links reach it from standing ready and go back.
fn patch_banderling_table(bytes: &[u8]) -> Result<Vec<u8>, String> {
    // The defaults, then the cycles.
    let defaults = usize::try_from(read_u32(bytes, 8)?).map_err(|e| e.to_string())?;
    let mut at = 12 + 8 * defaults;
    let mut cycles = BTreeMap::new();
    let count = read_u32(bytes, at)?;
    at += 4;
    for _ in 0..count {
        cycles.insert(read_u32(bytes, at)?, at);
        at = past_motion(bytes, at)?;
    }
    let count = read_u32(bytes, at)?;
    at += 4;
    for _ in 0..count {
        at = past_motion(bytes, at)?;
    }
    // Every link list: where it starts and where each entry of it does.
    let link_count_at = at;
    let link_count = read_u32(bytes, at)?;
    at += 4;
    let mut links = BTreeMap::new();
    let mut lists = Vec::new();
    for _ in 0..link_count {
        let key = read_u32(bytes, at)?;
        let entries = read_u32(bytes, at + 4)?;
        let head = at;
        at += 8;
        let mut starts = Vec::new();
        for _ in 0..entries {
            if read_u32(bytes, at)? == MOTION_CAST_SPELL {
                return Err("the Banderling table already casts".into());
            }
            starts.push(at);
            at = past_motion(bytes, at)?;
        }
        starts.push(at);
        links.insert(key, (head, starts));
        lists.push(key);
    }
    if at != bytes.len() {
        return Err("the Banderling table has bytes after its links".into());
    }

    let mut out = bytes.to_vec();
    let mut inserts: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut new_links = Vec::new();
    for stance in &STANCES {
        let cycle = (stance.style << 16) | (MOTION_CAST_SPELL & 0xFFFF);
        let &cycle_at = cycles
            .get(&cycle)
            .ok_or_else(|| format!("no casting cycle {cycle:#x}"))?;
        out[cycle_at..cycle_at + 24].copy_from_slice(&motion_bytes(cycle, &[HOLD]));
        let ready = (stance.style << 16) | (MOTION_READY & 0xFFFF);
        let (head, starts) = links
            .get(&ready)
            .ok_or_else(|| format!("no links from standing ready {ready:#x}"))?;
        let list = u32::try_from(starts.len()).map_err(|e| e.to_string())?;
        out[head + 4..head + 8].copy_from_slice(&list.to_le_bytes());
        let insert_at = *starts
            .get(stance.at)
            .ok_or_else(|| format!("{ready:#x} has too few links"))?;
        inserts.push((insert_at, motion_bytes(MOTION_CAST_SPELL, stance.to_cast)));
        new_links.extend_from_slice(&cycle.to_le_bytes());
        new_links.extend_from_slice(&1u32.to_le_bytes());
        new_links.extend_from_slice(&motion_bytes(MOTION_READY, stance.to_ready));
    }
    let (before, _) = links
        .get(&LINKS_AFTER_CAST)
        .ok_or("the Banderling table has changed its links")?;
    inserts.push((*before, new_links));
    let total = link_count + u32::try_from(STANCES.len()).map_err(|e| e.to_string())?;
    out[link_count_at..link_count_at + 4].copy_from_slice(&total.to_le_bytes());
    inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, bytes) in inserts {
        out.splice(at..at, bytes);
    }
    Ok(out)
}

/// What each class of record names, by class: the classes its aligned words may refer to.
fn children(class: u32) -> Option<&'static [u32]> {
    match class {
        0x02 => Some(&[0x01, 0x02, 0x03, 0x09, 0x20, 0x33, 0x34]),
        0x03 => Some(&[0x01, 0x02, 0x0A, 0x20, 0x32, 0x33]),
        0x08 => Some(&[0x04, 0x05, 0x06]),
        0x09 => Some(&[0x03]),
        0x0F => Some(&[0x04]),
        0x10 => Some(&[0x01, 0x02, 0x04, 0x05, 0x06, 0x0F]),
        0x20 => Some(&[0x0A]),
        0x32 => Some(&[0x01, 0x02]),
        0x33 => Some(&[0x01, 0x02, 0x0A, 0x32, 0x33]),
        0x34 => Some(&[0x33]),
        _ => None,
    }
}

/// The ids `data`, the record `id`, may name. Models and pictures have a layout to read; for every
/// other class each aligned word that looks like a child's id is taken, which errs wide: an extra
/// record costs space, and a missing one would break the client.
fn references(id: u32, data: &[u8]) -> Result<Vec<u32>, String> {
    let class = id >> 24;
    match class {
        0x01 => {
            let surfaces = usize::try_from(read_u32(data, 8)?).map_err(|e| e.to_string())?;
            (0..surfaces).map(|i| read_u32(data, 12 + 4 * i)).collect()
        }
        0x05 | 0x06 => {
            // A picture with a palette ends in the palette's id.
            let last = data.len().checked_sub(4).map(|at| read_u32(data, at));
            match last {
                Some(value) => {
                    let value = value?;
                    Ok(if value >> 24 == 0x04 {
                        vec![value]
                    } else {
                        vec![]
                    })
                }
                None => Ok(vec![]),
            }
        }
        0x04 | 0x0A | 0x30 => Ok(vec![]),
        _ => {
            let classes =
                children(class).ok_or_else(|| format!("{id:#x}: no reading of its class"))?;
            Ok(data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| u32::from_le_bytes(*w))
                .filter(|v| classes.contains(&(v >> 24)) && *v != id)
                .collect())
        }
    }
}

/// What a walk adds, by id, and the ids it kept as ours.
type Walked = (BTreeMap<u32, Vec<u8>>, Vec<u32>);

/// Every record `roots` need from `reference` that `ours` lacks, walking from each through what it
/// names and stopping at what `ours` holds. A record `ours` holds differently is kept as ours and
/// listed, so a caller can refuse it. A word naming nothing `reference` holds isn't a reference.
fn closure(
    reference: &dyn Fn(u32) -> Option<Vec<u8>>,
    ours: &dyn Fn(u32) -> Option<Vec<u8>>,
    roots: &[u32],
) -> Result<Walked, String> {
    let mut new = BTreeMap::new();
    let mut kept = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut todo = roots.to_vec();
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        let data = reference(id).ok_or_else(|| format!("{id:#010x} isn't in the reference"))?;
        match ours(id) {
            Some(have) => {
                if have != data {
                    kept.push(id);
                }
            }
            None => {
                todo.extend(
                    references(id, &data)?
                        .into_iter()
                        .filter(|named| reference(*named).is_some()),
                );
                new.insert(id, data);
            }
        }
    }
    kept.sort_unstable();
    Ok((new, kept))
}

/// The holiday objects' records as a weenie names them: where the walk starts.
const HOLIDAY_ROOTS: [u32; 16] = [
    // A scarecrow's model, motions, sounds, combat, palette, clothing, icon and effects.
    0x0200_0984,
    0x0900_009D,
    0x2000_0066,
    0x3000_002C,
    0x0400_1007,
    0x1000_0274,
    0x0600_1EA5,
    0x3400_0088,
    // The other scarecrow.
    0x0200_0987,
    // A pumpkin.
    0x0200_0949,
    0x0400_1008,
    0x1000_024B,
    0x0600_1E27,
    // A jack o' lantern.
    0x0200_097A,
    0x1000_024C,
    0x0600_1E2C,
];
/// A different count means a file moved underneath the walk.
const HOLIDAY_RECORDS: usize = 196;
/// What the event changed in the portal file and left changed in the file of October 31.
const EVENT_CHANGED_RECORDS: usize = 11;
/// What the event added to the portal file, which the file of October 31 holds already.
const EVENT_ADDED_RECORDS: usize = 42;

/// The files a commit may read: the base, and the references the events need.
struct Files<'a> {
    portal: &'a DatFile,
    retail_cd_portal: Option<&'a DatFile>,
    september_2001_portal: Option<&'a DatFile>,
}

fn read(file: &DatFile, id: u32) -> Option<Vec<u8>> {
    file.read(DataId(id)).ok()
}

/// What the overlay holds: every record of the portal file `steps` change, by id.
fn distill(steps: &[Step], files: &Files<'_>) -> Result<BTreeMap<u32, Vec<u8>>, String> {
    let mut edits = BTreeMap::new();
    let spell_steps = [Step::Durations, Step::SpellFixes, Step::ResistableImperil];
    if steps.iter().any(|s| spell_steps.contains(s)) {
        let table = read(files.portal, SPELL_TABLE.raw()).ok_or("no spell table")?;
        let fixes = TableFixes {
            durations: steps.contains(&Step::Durations),
            range_and_power: steps.contains(&Step::SpellFixes),
            formulas_and_text: steps.contains(&Step::SpellFixes),
            resistable: steps.contains(&Step::ResistableImperil),
        };
        edits.insert(
            SPELL_TABLE.raw(),
            launch_spell_fixes::patch_table(&table, fixes)?,
        );
    }
    if steps.contains(&Step::BanderlingCasts) {
        let table =
            read(files.portal, BANDERLING_MOTION_TABLE.raw()).ok_or("no Banderling motions")?;
        let patched = patch_banderling_table(&table)?;
        if source::hash(&patched) != BANDERLING_MOTION_TABLE_SHA256 {
            return Err("the Banderling motion table came out other than as reviewed".into());
        }
        edits.insert(BANDERLING_MOTION_TABLE.raw(), patched);
    }
    if steps.contains(&Step::EventEnds) {
        let cd = files
            .retail_cd_portal
            .ok_or("the event's end needs the retail CD's portal")?;
        let event = decompose::diff(cd, files.portal).map_err(|e| e.to_string())?;
        if event.changed.len() != EVENT_CHANGED_RECORDS
            || event.added.len() != EVENT_ADDED_RECORDS
            || !event.removed.is_empty()
        {
            return Err(format!(
                "the event changes {} records and adds {}, not {EVENT_CHANGED_RECORDS} and {EVENT_ADDED_RECORDS}",
                event.changed.len(),
                event.added.len()
            ));
        }
        for id in event.changed {
            let record = read(cd, id).ok_or_else(|| format!("{id:#010x} won't read"))?;
            edits.insert(id, record);
        }
    }
    if steps.contains(&Step::HolidayAssets) {
        let september = files
            .september_2001_portal
            .ok_or("the holiday objects need the September 2001 portal")?;
        let (new, kept) = closure(
            &|id| read(september, id),
            &|id| edits.get(&id).cloned().or_else(|| read(files.portal, id)),
            &HOLIDAY_ROOTS,
        )?;
        if !kept.is_empty() {
            return Err(format!(
                "the September 2001 file has other copies of what the holiday objects use: {kept:x?}"
            ));
        }
        if new.len() != HOLIDAY_RECORDS {
            return Err(format!(
                "the holiday objects need {} records, not {HOLIDAY_RECORDS}",
                new.len()
            ));
        }
        edits.extend(new);
    }
    Ok(edits)
}

#[derive(Debug, Default)]
struct Options {
    revision: String,
    source: Option<PathBuf>,
    base: PathBuf,
    out: PathBuf,
    world_key: String,
    references: Vec<PathBuf>,
    with_events: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--with-events" {
            o.with_events = true;
            continue;
        }
        if !matches!(
            arg.as_str(),
            "--revision" | "--source" | "--base" | "--out" | "--world-key" | "--reference"
        ) {
            return Err(format!("unknown ac1999 dat-overlay option {arg}"));
        }
        let value = it.next().ok_or_else(|| format!("{arg} takes a value"))?;
        match arg.as_str() {
            "--revision" => value.to_ascii_lowercase().clone_into(&mut o.revision),
            "--source" => o.source = Some(PathBuf::from(value)),
            "--base" => o.base = PathBuf::from(value),
            "--out" => o.out = PathBuf::from(value),
            "--world-key" => value.clone_into(&mut o.world_key),
            "--reference" => o.references.push(PathBuf::from(value)),
            _ => unreachable!("checked above"),
        }
    }
    if o.revision.len() != 40 || !o.revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("--revision requires exactly 40 hexadecimal characters".into());
    }
    if o.base.as_os_str().is_empty() || o.out.as_os_str().is_empty() || o.world_key.is_empty() {
        return Err("--base, --out and --world-key are required".into());
    }
    if !o.references.is_empty() && !o.with_events {
        return Err("--reference names the files the events need: add --with-events".into());
    }
    Ok(o)
}

/// The steps taken from the commit's: the events only when asked for. Answers whether any were left
/// out.
fn chosen(mut steps: Vec<Step>, with_events: bool) -> (Vec<Step>, bool) {
    let left_out = !with_events && steps.iter().any(|s| s.is_event());
    if !with_events {
        steps.retain(|s| !s.is_event());
    }
    (steps, left_out)
}

/// A file's SHA-256, as a hash of its whole bytes.
fn file_hash(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(source::hash(&bytes))
}

/// `name` in the folder `dir`, checked to be the file `want`.
fn pinned(dir: &Path, name: &str, want: DatIdentity) -> Result<DatFile, String> {
    let path = crate::dat_overlay::find(dir, name)
        .ok_or_else(|| format!("{} holds no {name}", dir.display()))?;
    let found = file_hash(&path)?;
    if found != want.sha256 {
        return Err(format!(
            "{} isn't the {name} of {}, iteration {} (sha256 {}, this one {found})",
            path.display(),
            want.capture,
            want.iteration,
            want.sha256
        ));
    }
    DatFile::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn missing_reference(what: &str, want: DatIdentity) -> String {
    format!(
        "{what} need the portal.dat of {}, iteration {} (sha256 {}): name it with --reference, or leave out --with-events",
        want.capture, want.iteration, want.sha256
    )
}

/// Refuses `steps` when a file they need wasn't named.
fn check_references(steps: &[Step], retail_cd: bool, september_2001: bool) -> Result<(), String> {
    if steps.contains(&Step::EventEnds) && !retail_cd {
        return Err(missing_reference(
            "the end of the event's records",
            cells::PORTAL_1999_10_09,
        ));
    }
    if steps.contains(&Step::HolidayAssets) && !september_2001 {
        return Err(missing_reference("the holiday objects", PORTAL_2001_09_12));
    }
    Ok(())
}

/// Write `edits` as a fresh overlay of the portal file over `base`, in one revision of its own, and
/// read them back through it. Answers the revision.
fn write_overlay(
    base: &DatFile,
    edits: &BTreeMap<u32, Vec<u8>>,
    dir: &OverlayDir,
    key: &str,
) -> Result<u32, String> {
    let path = dir.container(ModernDat::Portal);
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let base = base.base();
    let revision = decompose::revision_over(&base);
    let name = dereth_dat::ClassicDat::Portal.file_name();
    let mut w =
        OverlayWriter::open_or_create(&path, &base, name, key, 0).map_err(|e| e.to_string())?;
    for (id, bytes) in edits {
        w.put(DataId(*id), bytes, 1, revision, 0)
            .map_err(|e| e.to_string())?;
    }
    w.add_iteration(revision, 0).map_err(|e| e.to_string())?;
    w.flush(0).map_err(|e| e.to_string())?;
    drop(w);
    let opened = DatFile::open(&path).map_err(|e| e.to_string())?;
    let layer = overlay::Layer::over(&base, opened, Some(key)).map_err(|e| e.to_string())?;
    for (id, bytes) in edits {
        if layer.read(DataId(*id)).map_err(|e| e.to_string())? != *bytes {
            return Err(format!("{id:#010x} doesn't read back as written"));
        }
    }
    Ok(revision)
}

fn build(o: &Options) -> Result<ExitCode, String> {
    let paths = [
        MIGRATIONS,
        REVIEWED_TOOLING[0].0,
        REVIEWED_TOOLING[1].0,
        REVIEWED_TOOLING[2].0,
    ];
    let scratch = PathBuf::from(format!("{}.fetch-{}", o.out.display(), std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let committed = source::committed_hashes(o.source.as_deref(), &o.revision, &scratch, &paths);
    let _ = std::fs::remove_dir_all(&scratch);
    let (steps, left_out) = chosen(plan(&committed?)?, o.with_events);
    let portal = pinned(&o.base, "portal.dat", cells::PORTAL_1999_10_31)?;
    pinned(&o.base, "cell.dat", cells::CELL_1999_10_09)?;

    let (mut retail_cd_portal, mut september_2001_portal) = (None, None);
    for path in &o.references {
        let found = file_hash(path)?;
        let file = DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if found == cells::PORTAL_1999_10_09.sha256 {
            retail_cd_portal = Some(file);
        } else if found == PORTAL_2001_09_12.sha256 {
            september_2001_portal = Some(file);
        } else {
            return Err(format!(
                "{} is neither of the files the events need (sha256 {found})",
                path.display()
            ));
        }
    }
    check_references(
        &steps,
        retail_cd_portal.is_some(),
        september_2001_portal.is_some(),
    )?;

    let edits = distill(
        &steps,
        &Files {
            portal: &portal,
            retail_cd_portal: retail_cd_portal.as_ref(),
            september_2001_portal: september_2001_portal.as_ref(),
        },
    )?;
    if edits.is_empty() {
        println!("this commit changes nothing here: no overlay was written");
        return Ok(ExitCode::from(1));
    }
    let dir = OverlayDir::new(&o.out).map_err(|e| e.to_string())?;
    let revision = write_overlay(&portal, &edits, &dir, &o.world_key)?;
    let taken: Vec<&str> = REVIEWED
        .iter()
        .filter(|(_, _, step)| steps.contains(step))
        .map(|(file, _, _)| *file)
        .collect();
    let record = json!({
        "source": source::URL,
        "revision": o.revision,
        "migrations": taken,
        "events_left_out": left_out,
        "base": {
            "portal.dat": cells::PORTAL_1999_10_31.sha256,
            "cell.dat": cells::CELL_1999_10_09.sha256,
        },
        "revision_over_base": revision,
        "records": edits
            .iter()
            .map(|(id, bytes)| (format!("{id:08X}"), json!(source::hash(bytes))))
            .collect::<serde_json::Map<_, _>>(),
    });
    let mut text = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
    text.push(b'\n');
    std::fs::write(o.out.join("ac1999-overlay.json"), text).map_err(|e| e.to_string())?;
    println!(
        "wrote {} record(s) of portal.dat to {} (iteration {revision}), from {} migration(s) of {}{}",
        edits.len(),
        dir.container(ModernDat::Portal).display(),
        taken.len(),
        o.revision,
        if left_out {
            ", leaving out the events (--with-events takes them)"
        } else {
            ""
        }
    );
    println!("set [dat_overlay] path to that folder; this overlay holds retail records, so keep it on this machine");
    Ok(ExitCode::SUCCESS)
}

/// Run the subcommand over its arguments.
pub fn run(args: &[String]) -> ExitCode {
    match parse(args).and_then(|o| build(&o)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("empyrean-import ac1999 dat-overlay: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests;
