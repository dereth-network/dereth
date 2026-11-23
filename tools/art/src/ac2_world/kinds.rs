//! AC2's terrain kinds as AC1 terrain words: each kind's AC1 type slot (bits 2-6) and a variant
//! (bits 7-10) that keeps the kinds sharing a slot apart. The table is the tool's data,
//! `ac2-world/kinds.toml` ([`kinds`]).
//!
//! The slot is the AC1 type nearest by look, so a reader that knows only the slot (the retail
//! pictures, the scenery rules) gets the closest AC1 ground. Variant 0 of each slot is the kind
//! the AC2 look (`dereth/tools/art/ac2-look/terrain.toml`) already draws for that slot, so the
//! look is right for it without knowing variants; the other kinds of the slot follow by share of
//! AC2's land.

/// AC1's water slots.
pub const WATER_RUNNING: u16 = 16;
pub const WATER_STANDING_FRESH: u16 = 17;
pub const WATER_SHALLOW_SEA: u16 = 18;
pub const WATER_SHALLOW_STILL_SEA: u16 = 19;
pub const WATER_DEEP_SEA: u16 = 20;

/// AC2's water this deep or deeper is deep water; shallower, wading water. It is where AC2 starts
/// a human swimming: three quarters of a 2.06 m body.
pub const WADING_DEPTH: f64 = 1.545;

/// The kind table as the tool carries it: `ac2-world/kinds.toml`.
pub const KINDS_TOML: &str = include_str!("../../ac2-world/kinds.toml");

/// The kind table: each of AC2's kinds, by its index in AC2's region, as an AC1 slot and variant;
/// and the kinds whose water runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kinds {
    pub slots: Vec<(u16, u16)>,
    pub rivers: Vec<u32>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct KindsFile {
    version: u32,
    rivers: Vec<u32>,
    kind: Vec<KindRow>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct KindRow {
    index: usize,
    #[allow(dead_code)] // for reading only
    ac2: String,
    slot: u16,
    variant: u16,
    #[serde(rename = "as")]
    #[allow(dead_code)] // for reading only
    ac1: String,
}

impl Kinds {
    /// The table from its file's text.
    ///
    /// # Errors
    /// The text does not read, is not version 1, lists its kinds out of order, or names a slot or
    /// variant past the terrain word's bits, or a running kind it does not list.
    pub fn parse(text: &str) -> Result<Self, String> {
        let f: KindsFile = toml::from_str(text).map_err(|e| format!("the kind table: {e}"))?;
        if f.version != 1 {
            return Err(format!("the kind table is version {}, not 1", f.version));
        }
        let mut slots = Vec::with_capacity(f.kind.len());
        for (k, row) in f.kind.iter().enumerate() {
            if row.index != k {
                return Err(format!(
                    "the kind table's kind {k} says it is {}",
                    row.index
                ));
            }
            if row.slot > 31 || row.variant > 15 {
                return Err(format!(
                    "kind {k}: slot {} variant {} past the word's bits",
                    row.slot, row.variant
                ));
            }
            slots.push((row.slot, row.variant));
        }
        if let Some(r) = f
            .rivers
            .iter()
            .find(|&&r| usize::try_from(r).map_or(true, |r| r >= slots.len()))
        {
            return Err(format!(
                "the kind table's running kind {r} is not one of its kinds"
            ));
        }
        Ok(Self {
            slots,
            rivers: f.rivers,
        })
    }
}

/// The kind table the tool carries, read once.
///
/// # Panics
/// If the carried table does not read, which its test rules out.
#[must_use]
pub fn kinds() -> &'static Kinds {
    static KINDS: std::sync::OnceLock<Kinds> = std::sync::OnceLock::new();
    KINDS.get_or_init(|| Kinds::parse(KINDS_TOML).expect("the carried kind table reads"))
}

/// The AC1 slot and variant of AC2 kind `kind`; an unknown kind is bare rock.
#[must_use]
pub fn land_slot(kind: u32) -> (u16, u16) {
    usize::try_from(kind)
        .ok()
        .and_then(|k| kinds().slots.get(k).copied())
        .unwrap_or((0, 0))
}

/// An AC1 terrain word: road in bits 0-1, type in 2-6, variant in 7-10, scene in 11-15. Each is
/// cut to its width.
#[must_use]
pub const fn pack_word(road: u16, slot: u16, variant: u16, scene: u16) -> u16 {
    (road & 0x3) | ((slot & 0x1F) << 2) | ((variant & 0xF) << 7) | ((scene & 0x1F) << 11)
}

/// AC2's road field (0-15) as AC1's two bits: AC2's land roads are 1-3, kept as they are; a
/// larger value is a road too.
#[must_use]
pub fn road_bits(road: u32) -> u16 {
    u16::try_from(road.min(3)).unwrap_or(3)
}

/// The kinds of AC2's water.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WaterKind {
    River,
    Lake,
    Sea,
}

/// The kind of water over AC2 ground of kind `kind`: a river where the ground is a river kind; a
/// lake under a plane that is not the sea's; else the sea.
#[must_use]
pub fn water_kind(kind: u32, is_sea: bool) -> WaterKind {
    if kinds().rivers.contains(&kind) {
        WaterKind::River
    } else if !is_sea {
        WaterKind::Lake
    } else {
        WaterKind::Sea
    }
}

/// The water slot for a vertex under `depth` meters of AC2's water of kind `kind`. Every kind of
/// water is wading or deep by its depth ([`WADING_DEPTH`]): deep, a river is running water, a lake
/// fresh standing water and the sea deep sea; wading, a river and the sea are shallow sea and a
/// lake shallow still sea. Dereth's physics reads the shallow seas on the finer land grid as
/// wading water, where a body stands as in the water at a shore.
#[must_use]
pub fn water_slot(kind: WaterKind, depth: f64) -> u16 {
    let deep = depth >= WADING_DEPTH;
    match (kind, deep) {
        (WaterKind::River, true) => WATER_RUNNING,
        (WaterKind::Lake, true) => WATER_STANDING_FRESH,
        (WaterKind::Sea, true) => WATER_DEEP_SEA,
        (WaterKind::River | WaterKind::Sea, false) => WATER_SHALLOW_SEA,
        (WaterKind::Lake, false) => WATER_SHALLOW_STILL_SEA,
    }
}

/// Whether water slot `slot` is wading water.
#[must_use]
pub const fn is_wading(slot: u16) -> bool {
    slot == WATER_SHALLOW_SEA || slot == WATER_SHALLOW_STILL_SEA
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's ground kinds)

    /// The kind table the tool carries reads, and holds the kinds the land has always been
    /// written with: every slot, variant and running kind, by their bytes' SHA-256.
    #[test]
    fn the_carried_kind_table_reads_and_holds_todays_kinds() {
        use sha2::{Digest, Sha256};
        let k = Kinds::parse(KINDS_TOML).expect("the carried table reads");
        assert_eq!(k.slots.len(), 61);
        let mut h = Sha256::new();
        for (s, v) in &k.slots {
            h.update(s.to_le_bytes());
            h.update(v.to_le_bytes());
        }
        for r in &k.rivers {
            h.update(r.to_le_bytes());
        }
        let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            got,
            "8e4074844a4e71089ce10a8ef5e58ee0d7d7288b393e39c267c7b0417cb78c99"
        );
        assert_eq!(kinds(), &k);
    }

    /// A kind table out of order, past the word's bits, of another version, or running a kind it
    /// does not list, is refused.
    #[test]
    fn a_kind_table_that_says_something_wrong_is_refused() {
        let row = |i: usize, slot: u16| {
            format!(
                "[[kind]]
index = {i}
ac2 = \"a\"
slot = {slot}
variant = 0
as = \"b\"
"
            )
        };
        let table = |version: u32, rivers: &str, rows: &str| {
            format!(
                "version = {version}
rivers = [{rivers}]
{rows}"
            )
        };
        assert!(Kinds::parse(&table(1, "1", &(row(0, 3) + &row(1, 4)))).is_ok());
        assert!(Kinds::parse(&table(2, "1", &(row(0, 3) + &row(1, 4)))).is_err());
        assert!(Kinds::parse(&table(1, "1", &(row(1, 3) + &row(0, 4)))).is_err());
        assert!(Kinds::parse(&table(1, "1", &(row(0, 32) + &row(1, 4)))).is_err());
        assert!(Kinds::parse(&table(1, "2", &(row(0, 3) + &row(1, 4)))).is_err());
    }
    use super::*;

    #[test]
    fn a_word_holds_road_type_variant_and_scene_in_ac1s_bits() {
        let w = pack_word(2, 21, 5, 17);
        assert_eq!(w & 3, 2);
        assert_eq!((w >> 2) & 0x1F, 21);
        assert_eq!((w >> 7) & 0xF, 5);
        assert_eq!(w >> 11, 17);
        // Each field is cut to its width and never spills into the next.
        assert_eq!(pack_word(7, 0, 0, 0), 3);
        assert_eq!(pack_word(0, 0x3F, 0, 0), 0x1F << 2);
        assert_eq!(pack_word(0, 0, 0x1F, 0), 0xF << 7);
    }

    #[test]
    fn every_land_kind_keeps_a_word_of_its_own_and_none_lands_on_a_water_slot() {
        let mut seen = std::collections::BTreeSet::new();
        for (k, &(slot, variant)) in kinds().slots.iter().enumerate() {
            assert!(!(16..=20).contains(&slot), "kind {k} on water slot {slot}");
            assert!(slot < 32 && variant < 16, "kind {k}");
            assert!(
                seen.insert((slot, variant)),
                "kind {k} shares {slot}/{variant}"
            );
        }
    }

    #[test]
    fn water_is_river_on_river_kinds_lake_under_its_own_plane_else_sea() {
        assert_eq!(water_kind(16, true), WaterKind::River);
        assert_eq!(water_kind(53, false), WaterKind::River);
        assert_eq!(water_kind(1, false), WaterKind::Lake);
        assert_eq!(water_kind(1, true), WaterKind::Sea);
    }

    #[test]
    fn every_kind_of_water_wades_shallower_than_ac2s_swim_depth_and_is_deep_from_it() {
        let under = WADING_DEPTH - 0.01;
        for kind in [WaterKind::River, WaterKind::Lake, WaterKind::Sea] {
            assert!(is_wading(water_slot(kind, 0.1)), "{kind:?}");
            assert!(is_wading(water_slot(kind, under)), "{kind:?}");
            assert!(!is_wading(water_slot(kind, WADING_DEPTH)), "{kind:?}");
            assert!(!is_wading(water_slot(kind, 30.0)), "{kind:?}");
        }
        // Deep, each kind keeps its own water; wading, the still kind stays still.
        assert_eq!(water_slot(WaterKind::River, 30.0), WATER_RUNNING);
        assert_eq!(water_slot(WaterKind::Lake, 30.0), WATER_STANDING_FRESH);
        assert_eq!(water_slot(WaterKind::Sea, 30.0), WATER_DEEP_SEA);
        assert_eq!(water_slot(WaterKind::River, 1.0), WATER_SHALLOW_SEA);
        assert_eq!(water_slot(WaterKind::Lake, 1.0), WATER_SHALLOW_STILL_SEA);
        assert_eq!(water_slot(WaterKind::Sea, 1.0), WATER_SHALLOW_SEA);
        // The sea is deep from AC2's swim depth, not from 5 m.
        assert_eq!(water_slot(WaterKind::Sea, 4.9), WATER_DEEP_SEA);
    }

    #[test]
    fn the_roads_keep_their_value_up_to_ac1s_two_bits() {
        assert_eq!(road_bits(0), 0);
        assert_eq!(road_bits(2), 2);
        assert_eq!(road_bits(3), 3);
        assert_eq!(road_bits(9), 3);
    }
}
