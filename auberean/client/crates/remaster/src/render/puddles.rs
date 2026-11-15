//! Which ground gathers puddles in the rain: a table of the region's kinds of terrain by name, each
//! with how readily it takes them, and the roads', and the weather's rows that carry it to the
//! landscape's draws.
//!
//! Roads gather the most; dirt, mud, marsh, dry beds and flat town stone less; grass (the human
//! and Tumerok towns' ground is grass), forest floor, jungle and leaf litter a little; rock, sand,
//! gravel, shore, riverbeds, snow, ice, water and volcanic ground never. The weights follow what
//! each kind's flat ground shows, not its name alone. A kind the table doesn't name gathers none.

use dereth_gfx_passes::frame::Rows;

/// How readily each ground gathers puddles: 3 as much as a road, 2 dirt and the like, 1 a little,
/// 0 none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PuddleTable {
    /// By the kind's name in the region's terrain description; a kind not named takes none.
    pub kinds: &'static [(&'static str, u8)],
    /// A road's.
    pub roads: u8,
}

/// The remaster's own table.
pub const PUDDLES: PuddleTable = PuddleTable {
    kinds: &[
        // Dirt, mud and marsh.
        ("Dirt", 2),
        ("PackedDirt", 2),
        ("PatchyDirt", 2),
        ("grasslandmud", 2),
        ("TropicMud", 2),
        ("Burunswamp", 2),
        ("Marsh", 2),
        // Dry beds drawn with the mud's grounds.
        ("Silent_hum_barrenbed", 2),
        ("Silent_hum_dry_barrenbed", 2),
        ("Silent_tum_dry_barrenbed", 2),
        // The Lugians' towns' flat stone.
        ("TownLugian", 2),
        // Grass, the towns' grass among it, forest floor, jungle and leaf litter.
        ("GrassLand", 1),
        ("PatchyGrassLand", 1),
        ("TownHuman", 1),
        ("TownTumerok", 1),
        ("Autumn", 1),
        ("HighlandTundra", 1),
        ("TumerokCoastal", 1),
        ("CobaltSweep", 1),
        ("HauntedGround", 1),
        ("ForestFloor", 1),
        ("tropicfloor", 1),
        ("TropicBamboo", 1),
        ("hero_Jungle", 1),
        ("hero_Jungle_light", 1),
        ("BurntGround", 1),
        ("OlthoiInfested", 1),
    ],
    roads: 3,
};

/// The steps a weight is counted in: a weight of this many is a road's.
const STEPS: f32 = 3.0;

/// How many kinds of terrain the weather's rows have room for; a kind past them takes none.
pub(super) const KINDS: usize = 128;

/// The weather's row the table starts at (`W_GROUNDS` in `shaders/terrain.slang`): the first after
/// the shared passes' own (`W_SNOW` in their `weather.slang` is the last).
const W_GROUNDS: usize = 9;

/// The landscape vertex's ground word for a road, and for a ground not told (`ROAD_GROUND` and
/// `NO_GROUND` in `shaders/terrain.slang`): a kind of terrain is its own index.
pub(super) const ROAD_GROUND: u32 = 128;
pub(super) const NO_GROUND: u32 = u32::MAX;

/// How readily each kind of terrain the region names (`kinds`, by index) gathers puddles by
/// `table`, 0 none to 1 a road's, and after them (at [`KINDS`]) a road's.
#[must_use]
pub fn weights(kinds: &[String], table: &PuddleTable) -> Vec<f32> {
    let weight = |w: u8| f32::from(w).min(STEPS) / STEPS;
    let mut w = vec![0.0; KINDS + 1];
    for (k, name) in kinds.iter().enumerate().take(KINDS) {
        if let Some((_, takes)) = table.kinds.iter().find(|(n, _)| n == name) {
            w[k] = weight(*takes);
        }
    }
    w[KINDS] = weight(table.roads);
    w
}

/// The weather's rows from where the shared passes' first rows end: nothing up to the table, then
/// the table's weights for the region's kinds `kinds`, four a row.
pub(super) fn rows(rows: &mut Rows, kinds: &[String], table: &PuddleTable) {
    while rows.0.len() < W_GROUNDS {
        rows.f([0.0; 4]);
    }
    for four in weights(kinds, table).chunks(4) {
        let mut row = [0.0; 4];
        row[..four.len()].copy_from_slice(four);
        rows.f(row);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| (*s).to_string()).collect()
    }

    /// A road gathers the most; a named kind takes its weight at its index; a kind the table
    /// doesn't name, or an unnamed index, takes none.
    #[test]
    fn roads_gather_the_most_and_an_unnamed_kind_none() {
        let w = weights(&names(&["Dirt", "GrassLand", "", "HighlandRock"]), &PUDDLES);
        assert_eq!(w.len(), KINDS + 1);
        assert_eq!(w[KINDS], 1.0);
        assert!((w[0] - 2.0 / 3.0).abs() < 1e-6);
        assert!((w[1] - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(w[2], 0.0);
        assert_eq!(w[3], 0.0);
        assert!(w[4..KINDS].iter().all(|&x| x == 0.0));
    }

    /// The rows start where the table starts, after the shared rows, and hold every kind and the
    /// road, four a row.
    #[test]
    fn the_table_starts_after_the_shared_rows() {
        let mut r = Rows::default();
        r.f([1.0; 4]);
        rows(&mut r, &names(&["Dirt"]), &PUDDLES);
        assert_eq!(r.0.len(), W_GROUNDS + (KINDS + 1).div_ceil(4));
        assert_eq!(f32::from_bits(r.0[W_GROUNDS][0]), 2.0 / 3.0);
        let road = KINDS / 4;
        assert_eq!(f32::from_bits(r.0[W_GROUNDS + road][KINDS % 4]), 1.0);
    }

    /// Every kind is named once.
    #[test]
    fn no_kind_is_named_twice() {
        let kinds = PUDDLES.kinds;
        for (i, (a, _)) in kinds.iter().enumerate() {
            assert!(
                kinds[i + 1..].iter().all(|(b, _)| a != b),
                "{a} is named twice"
            );
        }
    }
}
