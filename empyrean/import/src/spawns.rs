//! `empyrean-import spawns`: what the world's spawned objects are drawn with, as a table for the
//! art survey (`dereth-art survey --spawns`).
//!
//! ```text
//! empyrean-import spawns [--world <world>] [--sql <dump.sql>] [--out <spawns.tsv>]
//! ```
//!
//! - With no `--sql` it reads the dump of `--world` (`patches`, the default, or `16py`) that
//!   `fetch` keeps in the per-user cache, fetching the pinned release first when it is not there.
//! - It writes `--out` (default `spawns.tsv`): a header line, then one row per setup a spawned
//!   object is drawn with, tab-separated: `setup`, `type`, `spawns`, `landblocks`, `weenies`
//!   (`class:name:type:count`, `;`-separated) and `where` (`XXYY:count`, space-separated, most
//!   first).
//!
//! **What is counted.** Every landblock instance is one object where it stands, and every
//! encounter one object at its terrain cell. An object players never see (one only
//! administrators see, or one never drawn) is left out, though what it generates is not. An object that is a generator also stands for what
//! it generates when it is full: its profiles that put an object in the world (on top, scattered
//! or at a specific place; not in a container, wielded, for sale or as treasure), each at its
//! landblock (a specific place's own, else the generator's). A profile that always spawns
//! (probability -1) fills to its maximum first; the generator's remaining slots are shared among
//! its chance profiles by their probabilities, each capped at its own maximum. A profile that
//! puts its objects elsewhere still takes its share of the slots. A generated
//! generator is followed in turn. The counts are therefore expected values when every generator
//! is full, rounded; a weenie or landblock with any share counts at least one.
//!
//! **The types** are read from the weenie: `creature` (an attackable creature), `npc` (a creature
//! that cannot be attacked, or a vendor), `door`, `portal`, `lifestone`, `chest`, `hook` (a
//! house's hook), `generator` (a generic object that only generates) or `item` (everything
//! else). A setup's type is the type
//! of most of its spawns.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use empyrean_common::world_release::{self, WorldLine, PATCHES};
use empyrean_content::models::world::{Encounter, LandblockInstance, Weenie};
use empyrean_content::WorldContent;
use empyrean_entity::enums::{
    PropertyBool, PropertyDataId, PropertyInt, RegenLocationType, WeenieType,
};

/// The weenie class a generator profile names to hold an empty slot.
const PLACEHOLDER_CLASS: u32 = 3666;

/// How deep generated generators are followed.
const MAX_DEPTH: u32 = 8;

/// `spawns`' options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub world: &'static WorldLine,
    pub sql: Option<PathBuf>,
    pub out: PathBuf,
}

/// Reads `spawns`' options (the arguments after `spawns`).
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut world = &PATCHES;
    let mut sql = None;
    let mut out = PathBuf::from("spawns.tsv");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--world" => {
                let v = value()?;
                world = WorldLine::by_id(&v).ok_or_else(|| {
                    let known: Vec<&str> = world_release::WORLDS.iter().map(|w| w.id).collect();
                    format!("--world wants one of {known:?}, got {v:?}")
                })?;
            }
            "--sql" => sql = Some(PathBuf::from(value()?)),
            "--out" => out = PathBuf::from(value()?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Options { world, sql, out })
}

/// What one weenie class is, as far as the table cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Class {
    kind: &'static str,
    setup: Option<u32>,
}

/// The type a weenie's spawns are listed under.
#[must_use]
pub fn kind_of(w: &Weenie) -> &'static str {
    let t = u32::try_from(w.r#type).map_or(WeenieType::Undef, WeenieType);
    match t {
        WeenieType::Creature | WeenieType::Cow => {
            if w.get_property_bool(PropertyBool::Attackable) == Some(false) {
                "npc"
            } else {
                "creature"
            }
        }
        WeenieType::Vendor => "npc",
        WeenieType::Door => "door",
        WeenieType::Portal | WeenieType::HousePortal => "portal",
        WeenieType::LifeStone => "lifestone",
        WeenieType::Chest => "chest",
        WeenieType::Hook => "hook",
        WeenieType::Generic if !w.weenie_properties_generator.is_empty() => "generator",
        _ => "item",
    }
}

/// Whether players see the object: not one only administrators see, and not one never drawn.
#[must_use]
pub fn drawn(w: &Weenie) -> bool {
    w.get_property_bool(PropertyBool::Visibility) != Some(true)
        && w.get_property_bool(PropertyBool::NoDraw) != Some(true)
}

/// Whether a profile's objects stand in the world rather than in a container, a hand, a shop or
/// a corpse's loot.
fn places_in_world(where_create: u32) -> bool {
    let blocked = RegenLocationType::Contain.0
        | RegenLocationType::Wield.0
        | RegenLocationType::Shop.0
        | RegenLocationType::Treasure.0;
    where_create & blocked == 0
}

/// The expected objects of each weenie class at each landblock: `(class, landblock) -> count`.
pub type Counts = BTreeMap<(u32, u16), f64>;

/// Counts every placed object of `content`, and what its generators generate.
#[must_use]
pub fn count(content: &WorldContent) -> Counts {
    let weenies: BTreeMap<u32, &Weenie> = content.weenies.iter().map(|w| (w.class_id, w)).collect();
    let mut counts = Counts::new();
    for LandblockInstance {
        weenie_class_id,
        obj_cell_id,
        ..
    } in &content.landblock_instances
    {
        let lb = u16::try_from(obj_cell_id >> 16).unwrap_or(u16::MAX);
        place(&weenies, &mut counts, *weenie_class_id, lb, 1.0, 0);
    }
    for Encounter {
        landblock,
        weenie_class_id,
        ..
    } in &content.encounters
    {
        let lb = u16::try_from(*landblock).unwrap_or(u16::MAX);
        place(&weenies, &mut counts, *weenie_class_id, lb, 1.0, 0);
    }
    counts
}

/// Counts `n` objects of `class` at `lb`, and, for a generator, what they generate.
fn place(
    weenies: &BTreeMap<u32, &Weenie>,
    counts: &mut Counts,
    class: u32,
    lb: u16,
    n: f64,
    depth: u32,
) {
    if n <= 0.0 {
        return;
    }
    let Some(w) = weenies.get(&class) else {
        return;
    };
    if drawn(w) {
        *counts.entry((class, lb)).or_default() += n;
    }
    if w.weenie_properties_generator.is_empty() || depth >= MAX_DEPTH {
        return;
    }
    for (child, child_lb, k) in generated(w, lb) {
        place(weenies, counts, child, child_lb, n * k, depth + 1);
    }
}

/// What one full generator `w` at `lb` holds: `(class, landblock, expected count)` per profile
/// that puts an object in the world.
#[must_use]
pub fn generated(w: &Weenie, lb: u16) -> Vec<(u32, u16, f64)> {
    let max = w
        .get_property_int(PropertyInt::MaxGeneratedObjects)
        .unwrap_or(0);
    let init = w
        .get_property_int(PropertyInt::InitGeneratedObjects)
        .unwrap_or(0);
    // A maximum below the initial count is raised to it, as the generator does at start.
    let slots = if (max <= 0 || max < init) && init > 0 {
        init
    } else {
        max
    };
    if slots <= 0 {
        return Vec::new();
    }
    let mut remaining = f64::from(slots);
    let profiles: Vec<_> = w
        .weenie_properties_generator
        .iter()
        .filter(|p| p.weenie_class_id != PLACEHOLDER_CLASS && p.weenie_class_id != 0)
        .collect();
    // Each profile fills to its own maximum; one with no maximum (-1) can take every slot.
    let full = |p: &&empyrean_content::models::world::WeeniePropertiesGenerator| -> f64 {
        if p.max_create > 0 {
            f64::from(p.max_create)
        } else if p.max_create == -1 {
            f64::from(slots)
        } else {
            f64::from(p.init_create.max(1))
        }
    };
    let mut out = Vec::new();
    let mut take = |p: &&empyrean_content::models::world::WeeniePropertiesGenerator, n: f64| {
        if n > 0.0 && places_in_world(p.where_create) {
            let at = p
                .obj_cell_id
                .filter(|c| *c != 0 && p.where_create & RegenLocationType::Specific.0 != 0)
                .map_or(lb, |c| u16::try_from(c >> 16).unwrap_or(lb));
            out.push((p.weenie_class_id, at, n));
        }
    };
    for p in profiles.iter().filter(|p| p.probability == -1.0) {
        let n = full(p).min(remaining);
        remaining -= n;
        take(p, n);
    }
    // The chance profiles' weights: each probability is cumulative over the ones before it,
    // starting over where it drops.
    let mut weights = Vec::new();
    let mut last = 0.0f32;
    for p in profiles.iter().filter(|p| p.probability != -1.0) {
        if last > p.probability {
            last = 0.0;
        }
        weights.push((*p, f64::from((p.probability - last).max(0.0))));
        last = p.probability;
    }
    let total: f64 = weights.iter().map(|(_, x)| x).sum();
    if remaining > 0.0 && total > 0.0 {
        for (p, x) in weights {
            take(&p, (remaining * x / total).min(full(&p)));
        }
    }
    out
}

/// One row of the table, before it is written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetupRow {
    pub by_class: BTreeMap<u32, f64>,
    pub by_landblock: BTreeMap<u16, f64>,
    pub by_kind: BTreeMap<&'static str, f64>,
}

/// The counts gathered per setup. Classes with no setup are left out.
#[must_use]
pub fn by_setup(content: &WorldContent, counts: &Counts) -> BTreeMap<u32, SetupRow> {
    let classes: BTreeMap<u32, Class> = content
        .weenies
        .iter()
        .map(|w| {
            (
                w.class_id,
                Class {
                    kind: kind_of(w),
                    setup: w
                        .get_property_did(PropertyDataId::Setup)
                        .filter(|s| *s != 0),
                },
            )
        })
        .collect();
    let mut rows: BTreeMap<u32, SetupRow> = BTreeMap::new();
    for (&(class, lb), &n) in counts {
        let Some(c) = classes.get(&class) else {
            continue;
        };
        let Some(setup) = c.setup else {
            continue;
        };
        let r = rows.entry(setup).or_default();
        *r.by_class.entry(class).or_default() += n;
        *r.by_landblock.entry(lb).or_default() += n;
        *r.by_kind.entry(c.kind).or_default() += n;
    }
    rows
}

/// A count as the table writes it: rounded, and at least one when there is any.
#[must_use]
pub fn rounded(n: f64) -> u64 {
    if n <= 0.0 {
        0
    } else {
        // LINT-OK: a count of objects, far inside u64 and never negative here.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r = n.round() as u64;
        r.max(1)
    }
}

/// The table's text.
#[must_use]
pub fn table(content: &WorldContent, rows: &BTreeMap<u32, SetupRow>) -> String {
    let names: BTreeMap<u32, (&str, &'static str)> = content
        .weenies
        .iter()
        .map(|w| (w.class_id, (w.class_name.as_str(), kind_of(w))))
        .collect();
    let mut out = String::from("setup\ttype\tspawns\tlandblocks\tweenies\twhere\n");
    for (setup, r) in rows {
        let kind = r
            .by_kind
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))
            .map_or("item", |(k, _)| *k);
        let total: f64 = r.by_class.values().sum();
        let weenies: Vec<String> = r
            .by_class
            .iter()
            .map(|(c, n)| {
                let (name, k) = names.get(c).copied().unwrap_or(("", "item"));
                format!(
                    "{c}:{}:{k}:{}",
                    name.replace([':', ';', '\t'], "_"),
                    rounded(*n)
                )
            })
            .collect();
        let mut places: Vec<(u16, f64)> = r.by_landblock.iter().map(|(l, n)| (*l, *n)).collect();
        places.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let places: Vec<String> = places
            .iter()
            .map(|(l, n)| format!("{l:04X}:{}", rounded(*n)))
            .collect();
        let _ = writeln!(
            out,
            "0x{setup:08X}\t{kind}\t{}\t{}\t{}\t{}",
            rounded(total),
            r.by_landblock.len(),
            weenies.join(";"),
            places.join(" ")
        );
    }
    out
}

/// Reads a dump into world content, without building a pack.
pub fn read_dump(sql: &std::path::Path) -> Result<WorldContent, String> {
    use empyrean_content::import::{mysqldump, world_rows::WorldRows};
    let file = std::fs::File::open(sql).map_err(|e| format!("{}: {e}", sql.display()))?;
    let mut rows = WorldRows::default();
    mysqldump::scan(std::io::BufReader::with_capacity(8 << 20, file), &mut rows)
        .map_err(|e| format!("{}: {e}", sql.display()))?;
    Ok(rows.assemble().0)
}

/// Runs `spawns`.
pub fn run(opts: &Options) -> Result<String, String> {
    let sql = match &opts.sql {
        Some(p) => p.clone(),
        None => crate::fetch::fetch(&crate::fetch::Options {
            world: opts.world,
            version: crate::fetch::Version::Pinned,
            dir: None,
            pack: None,
        })?,
    };
    let content = read_dump(&sql)?;
    let counts = count(&content);
    let rows = by_setup(&content, &counts);
    std::fs::write(&opts.out, table(&content, &rows))
        .map_err(|e| format!("{}: {e}", opts.out.display()))?;
    Ok(format!(
        "{}: {} setups from {} landblock instances and {} encounters in {}",
        opts.out.display(),
        rows.len(),
        content.landblock_instances.len(),
        content.encounters.len(),
        sql.display()
    ))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks repository tools: the spawn table for the art survey)
    use super::*;
    use empyrean_content::models::world::WeeniePropertiesGenerator;

    fn profile(
        class: u32,
        probability: f32,
        init: i32,
        max: i32,
        where_: u32,
    ) -> WeeniePropertiesGenerator {
        WeeniePropertiesGenerator {
            weenie_class_id: class,
            probability,
            init_create: init,
            max_create: max,
            where_create: where_,
            ..Default::default()
        }
    }

    fn generator(class: u32, max: i32, profiles: Vec<WeeniePropertiesGenerator>) -> Weenie {
        let mut w = Weenie::new(class, "gen", WeenieType::Generic)
            .with_int(PropertyInt::MaxGeneratedObjects, max)
            .with_int(PropertyInt::InitGeneratedObjects, max);
        w.weenie_properties_generator = profiles;
        w
    }

    const SCATTER: u32 = 0x2;

    #[test]
    fn a_generator_that_always_spawns_fills_each_profile_to_its_maximum_within_its_slots() {
        let g = generator(
            100,
            5,
            vec![
                profile(1, -1.0, 2, 3, SCATTER),
                profile(2, -1.0, 4, 4, SCATTER),
            ],
        );
        let out = generated(&g, 0xA9B4);
        assert_eq!(out, vec![(1, 0xA9B4, 3.0), (2, 0xA9B4, 2.0)]);
    }

    #[test]
    fn chance_profiles_share_the_remaining_slots_by_their_cumulative_probabilities() {
        let g = generator(
            100,
            4,
            vec![
                profile(1, 0.25, 1, 4, SCATTER),
                profile(2, 1.0, 1, 4, SCATTER),
            ],
        );
        let out = generated(&g, 0x0102);
        assert_eq!(out, vec![(1, 0x0102, 1.0), (2, 0x0102, 3.0)]);
    }

    #[test]
    fn objects_put_in_containers_hands_shops_or_loot_are_not_in_the_world() {
        let g = generator(
            100,
            10,
            vec![
                profile(1, -1.0, 1, 1, RegenLocationType::Contain.0),
                profile(2, -1.0, 1, 1, RegenLocationType::Wield.0),
                profile(3, -1.0, 1, 1, RegenLocationType::Shop.0),
                profile(4, -1.0, 1, 1, RegenLocationType::ScatterTreasure.0),
                profile(PLACEHOLDER_CLASS, -1.0, 1, 1, SCATTER),
                profile(5, -1.0, 1, 1, SCATTER),
            ],
        );
        assert_eq!(generated(&g, 7), vec![(5, 7, 1.0)]);
        // They still take the generator's slots: with four, the four ahead of it fill them all.
        let mut g = g;
        g.weenie_properties_int.clear();
        let g = g
            .with_int(PropertyInt::MaxGeneratedObjects, 4)
            .with_int(PropertyInt::InitGeneratedObjects, 4);
        assert_eq!(generated(&g, 7), vec![]);
    }

    #[test]
    fn a_profile_at_a_specific_place_counts_in_that_places_landblock() {
        let mut p = profile(1, -1.0, 1, 1, RegenLocationType::Specific.0);
        p.obj_cell_id = Some(0xC6A9_0123);
        let g = generator(100, 1, vec![p]);
        assert_eq!(generated(&g, 0xA9B4), vec![(1, 0xC6A9, 1.0)]);
    }

    #[test]
    fn spawned_objects_are_gathered_by_setup_with_their_types_and_landblocks() {
        let drudge = Weenie::new(1, "drudge", WeenieType::Creature)
            .with_did(PropertyDataId::Setup, 0x0200_0001);
        let guard = Weenie::new(2, "guard", WeenieType::Creature)
            .with_did(PropertyDataId::Setup, 0x0200_0001)
            .with_bool(PropertyBool::Attackable, false);
        let door =
            Weenie::new(3, "door", WeenieType::Door).with_did(PropertyDataId::Setup, 0x0200_0002);
        let mut gen = generator(10, 3, vec![profile(1, -1.0, 3, 3, SCATTER)]);
        gen = gen.with_did(PropertyDataId::Setup, 0x0200_0003);
        let hidden = generator(11, 1, vec![profile(3, -1.0, 1, 1, SCATTER)])
            .with_did(PropertyDataId::Setup, 0x0200_0004)
            .with_bool(PropertyBool::Visibility, true);
        let content = WorldContent {
            weenies: vec![drudge, guard, door, gen, hidden],
            landblock_instances: vec![
                LandblockInstance::new(1, 2, 0xA9B4_0010, [0.0; 3]),
                LandblockInstance::new(2, 3, 0xA9B4_0011, [0.0; 3]),
                LandblockInstance::new(3, 3, 0xA9B5_0011, [0.0; 3]),
                LandblockInstance::new(4, 10, 0x0102_0001, [0.0; 3]),
                LandblockInstance::new(5, 11, 0xC6A9_0001, [0.0; 3]),
            ],
            ..Default::default()
        };
        let rows = by_setup(&content, &count(&content));
        let text = table(&content, &rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "setup\ttype\tspawns\tlandblocks\tweenies\twhere");
        assert_eq!(
            lines[1],
            "0x02000001\tcreature\t4\t2\t1:drudge:creature:3;2:guard:npc:1\t0102:3 A9B4:1"
        );
        // The hidden generator is left out; the door it generates is counted.
        assert_eq!(
            lines[2],
            "0x02000002\tdoor\t3\t3\t3:door:door:3\tA9B4:1 A9B5:1 C6A9:1"
        );
        assert_eq!(
            lines[3],
            "0x02000003\tgenerator\t1\t1\t10:gen:generator:1\t0102:1"
        );
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn a_share_of_an_object_counts_as_one() {
        assert_eq!(rounded(0.0), 0);
        assert_eq!(rounded(0.2), 1);
        assert_eq!(rounded(2.5), 3);
        assert_eq!(rounded(7.4), 7);
    }

    #[test]
    fn the_options_name_a_world_a_dump_and_the_table() {
        let o = parse_args(&[
            "--sql".into(),
            "d.sql".into(),
            "--out".into(),
            "s.tsv".into(),
        ])
        .expect("good options");
        assert_eq!(o.sql, Some(PathBuf::from("d.sql")));
        assert_eq!(o.out, PathBuf::from("s.tsv"));
        assert_eq!(o.world.id, PATCHES.id);
        assert!(parse_args(&["--world".into(), "nowhere".into()]).is_err());
        assert!(parse_args(&["--bogus".into()]).is_err());
    }
}
