//! AC2's place names, where AC2's own map writes them: the labels of its closer maps.
//!
//! AC2's region names its detail maps; the first holds the closer maps, each with its labels
//! (`.mapnotes` records): a word in a string table, an icon, and a land cell (20 m cells from
//! AC2's south-west corner). The converted world lies at AC2's true size with AC2's origin on
//! AC1's, so a label's place in AC2's world is its place in the converted one.
//!
//! The icon says what a label is: a town or a city, a point of interest or a quest, an outpost, a
//! vault, a dungeon, a gateway or a ringway (AC2's portals), a faction's shrine. The converted
//! world names its towns and cities as towns, and its points of interest, outposts, vaults and
//! dungeons as landmarks ([`kind_of`]), in a places document ([`document`]) beside the overlay,
//! which Dereth's map reads in place of AC1's own map's notes.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use auberean_assets::region::{DetailMap, MapNotes};
use auberean_dat::locate::DatName;
use auberean_world::records::WorldRecords;

/// AC2's land cell, meters.
const CELL_M: f64 = 20.0;

/// One of AC2's map labels: its icon, its words, and its land cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub icon: u32,
    pub words: String,
    pub cell: (u32, u32),
}

impl Label {
    /// The middle of its land cell, in world meters east and north.
    #[must_use]
    pub fn at(&self) -> (f64, f64) {
        (
            (f64::from(self.cell.0) + 0.5) * CELL_M,
            (f64::from(self.cell.1) + 0.5) * CELL_M,
        )
    }
}

/// AC2's token for a name, as its interface names one: each byte from the 26th bit, rotated
/// right by seven more bits than the byte before it, the results xored.
const fn token(name: &str) -> u32 {
    let bytes = name.as_bytes();
    let (mut hash, mut shift, mut i) = (0u32, 0u32, 0);
    while i < bytes.len() {
        hash ^= ((bytes[i] as u32) << 25).rotate_right(shift);
        shift = (shift + 7) & 31;
        i += 1;
    }
    hash
}

/// The variable a label's words may name its place's map coordinates by.
const MAP_COORDINATES: u32 = token("ID_Navigation_MapCoordinates");
/// What a label's coordinates are written as here, to be taken out of its words.
const COORDINATES: &str = "@@";

/// The icons of AC2's towns and cities: the 2012 relaunch's, then the same pictures as the 2002
/// and 2005 data number them.
const TOWNS: [u32; 4] = [0x4100_405D, 0x4100_4061, 0x4100_512C, 0x4100_5477];
/// The icons of its points of interest and quests, outposts, vaults and dungeons, in both
/// numberings.
const LANDMARKS: [u32; 8] = [
    0x4100_405A,
    0x4100_405E,
    0x4100_405F,
    0x4100_4060,
    0x4100_5129,
    0x4100_512D,
    0x4100_512E,
    0x4100_512F,
];

/// What a label names on the converted world's map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Town,
    Landmark,
}

impl Kind {
    /// The places document's word for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Town => "town",
            Self::Landmark => "landmark",
        }
    }
}

/// What a label with `icon` names, by its icon; `None` for AC2's portals and faction shrines.
#[must_use]
pub fn kind_of(icon: u32) -> Option<Kind> {
    if TOWNS.contains(&icon) {
        Some(Kind::Town)
    } else if LANDMARKS.contains(&icon) {
        Some(Kind::Landmark)
    } else {
        None
    }
}

/// A label's name from its words: the place they write its coordinates taken out, and a town's
/// "Town of" or "City of" before its name.
#[must_use]
pub fn name_of(words: &str, kind: Kind) -> String {
    let bare = words
        .replace(&format!(" ({COORDINATES})"), "")
        .replace(&format!("({COORDINATES})"), "")
        .replace(COORDINATES, "");
    let mut name = bare.split_whitespace().collect::<Vec<_>>().join(" ");
    if kind == Kind::Town {
        for prefix in ["Town of ", "City of "] {
            if let Some(rest) = name.strip_prefix(prefix) {
                name = rest.to_owned();
            }
        }
    }
    name
}

/// AC1's landblock side, meters, and its land cell's.
const AC1_BLOCK: f64 = 192.0;
const AC1_CELL: f64 = 24.0;

/// World point `(x, y)` meters as AC1 names a place outdoors: its land cell's id and the point in
/// its landblock; `None` outside AC1's world.
#[must_use]
pub fn ac1_cell(x: f64, y: f64) -> Option<(u32, f64, f64)> {
    let (bx, by) = ((x / AC1_BLOCK).floor(), (y / AC1_BLOCK).floor());
    if !(0.0..255.0).contains(&bx) || !(0.0..255.0).contains(&by) {
        return None;
    }
    let (lx, ly) = (x - bx * AC1_BLOCK, y - by * AC1_BLOCK);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (bx, by, cx, cy) = (
        bx as u32,
        by as u32,
        ((lx / AC1_CELL).floor() as u32).min(7),
        ((ly / AC1_CELL).floor() as u32).min(7),
    );
    Some(((bx << 24) | (by << 16) | (cx * 8 + cy + 1), lx, ly))
}

/// The places document of the converted world `world`: a `town` or `landmark` line for each
/// label that names one inside AC1's world, in the labels' order, standing on the ground
/// `height_at` gives (world meters); and how many towns and landmarks it names.
#[must_use]
pub fn document(
    labels: &[Label],
    world: &str,
    height_at: impl Fn(f64, f64) -> f64,
) -> (String, usize, usize) {
    let mut out = format!("dereth-places-store\t1\nworld\t{world}\n");
    let (mut towns, mut landmarks) = (0, 0);
    for l in labels {
        let Some(kind) = kind_of(l.icon) else {
            continue;
        };
        let name = name_of(&l.words, kind);
        let (x, y) = l.at();
        let Some((cell, lx, ly)) = ac1_cell(x, y) else {
            continue;
        };
        if name.is_empty() || name.contains(['\t', '\n', '\r', '\\']) {
            continue;
        }
        let _ = writeln!(
            out,
            "place\t{}\tpublished\t-\t{name}\t{cell:#010X} {lx:.3} {ly:.3} {:.3}\t0\t0\t-\t-",
            kind.word(),
            height_at(x, y)
        );
        match kind {
            Kind::Town => towns += 1,
            Kind::Landmark => landmarks += 1,
        }
    }
    (out, towns, landmarks)
}

/// Every label of AC2's closer maps, in the order its records hold them, with its English words
/// (empty where they do not write), its coordinates written as [`COORDINATES`].
///
/// # Errors
/// A file that does not open, or a region that does not read.
pub fn read_labels(dir: &Path) -> Result<Vec<Label>, String> {
    let open = |n: DatName| {
        let p = n.in_dir(dir);
        auberean_dat::DatFile::open(&p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let ctx_portal = open(DatName::Portal)?;
    let ctx = auberean_assets::Context::from_portal(&ctx_portal)
        .map_err(|e| format!("AC2 property list: {e:?}"))?;
    let w = WorldRecords::new(ctx_portal, open(DatName::Cell(1))?, None, ctx);
    let region = w.region().map_err(|e| format!("AC2 region: {e:?}"))?;
    let mut text = auberean_text::DatSource::new(vec![
        Arc::new(open(DatName::Portal)?),
        Arc::new(open(DatName::Local("English".into()))?),
    ]);
    let Some(detail) = region
        .region
        .detail_maps
        .first()
        .and_then(|t| w.get::<DetailMap>(t.record.0))
    else {
        return Ok(Vec::new());
    };
    let mut read: Vec<u32> = Vec::new();
    let mut labels = Vec::new();
    for row in &detail.rows {
        for id in &row.notes {
            if read.contains(id) {
                continue;
            }
            read.push(*id);
            let Some(notes) = w.get::<MapNotes>(*id) else {
                continue;
            };
            for n in &notes.notes {
                let mut info = auberean_text::StringInfo::entry(notes.strings.0, n.text);
                info.add_var(
                    MAP_COORDINATES,
                    auberean_text::Value::Text(Box::new(auberean_text::StringInfo::literal(
                        COORDINATES,
                    ))),
                );
                labels.push(Label {
                    icon: n.icon.0,
                    words: info.text(&mut text).trim().to_owned(),
                    cell: (n.x, n.y),
                });
            }
        }
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's place names)
    use super::*;

    #[test]
    fn the_coordinates_token_is_ac2_s() {
        // The interface's own hash of "ID_Navigation_MapCoordinates", as the AC2 client's crates
        // compute it.
        assert_eq!(MAP_COORDINATES, token("ID_Navigation_MapCoordinates"));
        assert_ne!(MAP_COORDINATES, 0);
    }

    #[test]
    fn a_name_is_its_words_less_its_coordinates_and_a_town_less_town_of() {
        assert_eq!(name_of("Town of Kehan (@@)", Kind::Town), "Kehan");
        assert_eq!(
            name_of("City of Linvak Tukal (@@)", Kind::Town),
            "Linvak Tukal"
        );
        assert_eq!(
            name_of("Caerlin Skyport (@@)", Kind::Town),
            "Caerlin Skyport"
        );
        assert_eq!(
            name_of("Mist Caverns (Dungeon, Level 9-14)(@@)", Kind::Landmark),
            "Mist Caverns (Dungeon, Level 9-14)"
        );
        assert_eq!(
            name_of("Northwest Stoneshadow Outpos (@@)t", Kind::Landmark),
            "Northwest Stoneshadow Outpost"
        );
        assert_eq!(
            name_of("Town of Arwic (@@)", Kind::Landmark),
            "Town of Arwic"
        );
    }

    #[test]
    fn towns_and_cities_are_towns_and_portals_and_shrines_are_left_out() {
        assert_eq!(kind_of(0x4100_405D), Some(Kind::Town));
        assert_eq!(kind_of(0x4100_4061), Some(Kind::Town));
        for icon in [0x4100_405A, 0x4100_405E, 0x4100_405F, 0x4100_4060] {
            assert_eq!(kind_of(icon), Some(Kind::Landmark), "{icon:08X}");
        }
        for icon in [0x4100_405B, 0x4100_405C, 0x4100_4062] {
            assert_eq!(kind_of(icon), None, "{icon:08X}");
        }
    }

    /// The 2002 and 2005 data number the same icons otherwise; their labels are told apart alike.
    #[test]
    fn older_data_s_icons_name_the_same_towns_and_landmarks() {
        assert_eq!(kind_of(0x4100_512C), Some(Kind::Town));
        assert_eq!(kind_of(0x4100_5477), Some(Kind::Town));
        for icon in [0x4100_5129, 0x4100_512D, 0x4100_512E, 0x4100_512F] {
            assert_eq!(kind_of(icon), Some(Kind::Landmark), "{icon:08X}");
        }
        for icon in [0x4100_512A, 0x4100_512B, 0x4100_590F] {
            assert_eq!(kind_of(icon), None, "{icon:08X}");
        }
    }

    /// A label's land cell is placed in AC1's world at the same meters from its south-west
    /// corner, in AC1's land cell numbering (eight a side, x major, from 1).
    #[test]
    fn a_label_lies_in_the_ac1_cell_its_metres_fall_in() {
        let kehan = Label {
            icon: 0x4100_405D,
            words: "Town of Kehan (@@)".into(),
            cell: (1135, 1451),
        };
        // (1135.5 * 20, 1451.5 * 20) = (22710, 29030) m: block (118, 151), 54 m and 38 m in.
        let (cell, lx, ly) = ac1_cell(kehan.at().0, kehan.at().1).expect("inside");
        assert_eq!(cell, (118 << 24) | (151 << 16) | (2 * 8 + 1 + 1));
        assert!(
            (lx - 54.0).abs() < 1e-9 && (ly - 38.0).abs() < 1e-9,
            "{lx} {ly}"
        );
        assert_eq!(ac1_cell(255.0 * 192.0, 5.0), None);
        let (doc, towns, landmarks) = document(&[kehan], "w", |_, _| 12.5);
        assert_eq!((towns, landmarks), (1, 0));
        assert_eq!(
            doc,
            "dereth-places-store\t1\nworld\tw\n\
             place\ttown\tpublished\t-\tKehan\t0x76970012 54.000 38.000 12.500\t0\t0\t-\t-\n"
        );
    }

    /// AC2's closer maps name its towns and cities, among them its five continents' first towns.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_map_names_twenty_seven_towns_among_them_each_continents_first() {
        let dir = auberean_dat::testing::require_target_install();
        let labels = read_labels(&dir).expect("labels");
        let (doc, towns, landmarks) = document(&labels, "w", |_, _| 0.0);
        eprintln!(
            "{} labels: {towns} towns, {landmarks} landmarks",
            labels.len()
        );
        assert_eq!(labels.len(), 384);
        assert_eq!(towns, 27);
        for town in ["Arwic", "Cragstone", "Shoushi", "Linvak Tukal", "Kehan"] {
            assert!(
                doc.contains(&format!("\ttown\tpublished\t-\t{town}\t")),
                "{town}"
            );
        }
    }
}
