//! AC2's creatures in the land: every family a creature table names (`tools/art/ac2-creatures`),
//! made as Asheron's Call's own records ([`crate::ac2_creature`]), each in a block of ids of its own
//! that the land's other records never reach, so a world's weenies name the same ids whatever else
//! the land holds. Each family is a creature type of its own, named in the creature types' table
//! after AC1's ([`types`]). `creatures.tsv` in the overlay folder says which ids each look's weenie
//! names, and its creature type.

use std::fmt::Write as _;

use dereth_primitives::DataId;

use crate::ac2_creature::make::{self, Made};
use crate::ac2_creature::table::Table;
use crate::ac2_look::source::Ac2;
use crate::ac2_world::art::Records;

/// The table written into the overlay folder.
pub const TABLE: &str = "creatures.tsv";

/// The first family's first low id. The land numbers its other records from `0xC000` up.
const FIRST: u16 = 0xB000;

/// Low ids a family may take of each kind.
const BLOCK: u16 = 0x100;

/// Each family's table, in the order its blocks of ids follow.
const FAMILIES: [(&str, &str); 12] = [
    ("tyrant", include_str!("../../ac2-creatures/tyrant.toml")),
    ("beetle", include_str!("../../ac2-creatures/beetle.toml")),
    ("crone", include_str!("../../ac2-creatures/crone.toml")),
    (
        "darkenfowl",
        include_str!("../../ac2-creatures/darkenfowl.toml"),
    ),
    ("flayer", include_str!("../../ac2-creatures/flayer.toml")),
    (
        "gigurath",
        include_str!("../../ac2-creatures/gigurath.toml"),
    ),
    ("grutt", include_str!("../../ac2-creatures/grutt.toml")),
    ("mimbu", include_str!("../../ac2-creatures/mimbu.toml")),
    ("mucor", include_str!("../../ac2-creatures/mucor.toml")),
    ("scraven", include_str!("../../ac2-creatures/scraven.toml")),
    (
        "thornling",
        include_str!("../../ac2-creatures/thornling.toml"),
    ),
    ("vermin", include_str!("../../ac2-creatures/vermin.toml")),
];

/// A family made: its table, what was made of it, and its creature type (0 until [`types`]
/// numbers it).
pub struct Family {
    pub table: Table,
    pub made: Made,
    pub creature_type: u32,
}

/// The families' drawn icons, each its family's name and its PNG (`ac2-creatures/icons`).
const DRAWN: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// An icon's side, texels: Asheron's Call's own icons'.
pub const ICON_SIDE: u32 = 32;

/// The pixel format and category Asheron's Call's own icons are written in: A8R8G8B8 (bytes blue,
/// green, red, alpha), a picture of the portal's.
const ICON_FORMAT: u32 = 21;
const ICON_CATEGORY: u32 = 6;

/// The `k`th family's drawn icon's id: the last picture id of its block.
fn icon_id(k: usize) -> DataId {
    DataId(0x0600_0000 | u32::from(first_of(k) + BLOCK - 1))
}

/// A drawn icon (a PNG, RGB or RGBA, 8 bits a channel, [`ICON_SIDE`] a side) as the picture record
/// `id`.
///
/// # Errors
/// A PNG that does not read, or is not that size or kind.
pub fn icon_record(id: DataId, png: &[u8]) -> Result<Vec<u8>, String> {
    let mut d = png::Decoder::new(std::io::Cursor::new(png));
    d.set_transformations(png::Transformations::EXPAND);
    let mut r = d.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if (info.width, info.height) != (ICON_SIDE, ICON_SIDE) {
        return Err(format!(
            "it is {} × {}, not {ICON_SIDE} × {ICON_SIDE}",
            info.width, info.height
        ));
    }
    if info.bit_depth != png::BitDepth::Eight {
        return Err("it is not 8 bits a channel".into());
    }
    let px = &buf[..info.buffer_size()];
    let bgra: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px
            .chunks(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect(),
        png::ColorType::Rgb => px
            .chunks(3)
            .flat_map(|p| [p[2], p[1], p[0], 0xFF])
            .collect(),
        other => return Err(format!("it is {other:?}, not RGB or RGBA")),
    };
    let rs = dereth_assets::RenderSurface {
        id,
        data_category: ICON_CATEGORY,
        width: ICON_SIDE,
        height: ICON_SIDE,
        format: ICON_FORMAT,
        image_size: 0,
        data_offset: 0,
        default_palette_id: None,
    };
    Ok(rs.encode(&bgra))
}

/// The `k`th family's (`name`) drawn icon in `drawn`, as its id and record; none when it has none.
fn drawn_icon(
    k: usize,
    name: &str,
    drawn: &[(&str, &[u8])],
) -> Result<Option<(DataId, Vec<u8>)>, String> {
    let Some((_, png)) = drawn.iter().find(|(f, _)| *f == name) else {
        return Ok(None);
    };
    let id = icon_id(k);
    let record = icon_record(id, png).map_err(|e| format!("creature {name}: its icon: {e}"))?;
    Ok(Some((id, record)))
}

/// The creature types' names: the portal's table the client names a creature's type from.
pub const TYPES_ID: u32 = 0x2200_000E;

/// AC1's creature types' table `ac1` with a type of its own for each of `labels` (the families'),
/// named so, numbered on from AC1's highest in their order; and each one's number.
#[must_use]
pub fn types(
    ac1: &dereth_assets::tables::EnumMapper,
    labels: &[&str],
) -> (dereth_assets::tables::EnumMapper, Vec<u32>) {
    let mut out = ac1.clone();
    let first = ac1
        .id_to_string
        .iter()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(0)
        + 1;
    let mut numbers = Vec::new();
    for (k, label) in labels.iter().enumerate() {
        let n = first + u32::try_from(k).unwrap_or(u32::MAX);
        // As the files write a type's name: its spaces as underscores.
        out.id_to_string.push((n, label.trim().replace(' ', "_")));
        numbers.push(n);
    }
    (out, numbers)
}

/// Each family its creature type of [`types`], from AC1's table `ac1`: the table with theirs in.
#[must_use]
pub fn number_types(
    ac1: &dereth_assets::tables::EnumMapper,
    families: &mut [Family],
) -> dereth_assets::tables::EnumMapper {
    let labels: Vec<&str> = families.iter().map(|f| f.table.label.as_str()).collect();
    let (table, numbers) = types(ac1, &labels);
    for (f, n) in families.iter_mut().zip(numbers) {
        f.creature_type = n;
    }
    table
}

/// The first low id of the `k`th family's block.
fn first_of(k: usize) -> u16 {
    // LINT-OK: twelve families.
    #[allow(clippy::cast_possible_truncation)]
    let k = k as u16;
    FIRST + k * BLOCK
}

/// Whether `id` lies in the block of ids starting at low id `first`.
fn in_block(id: DataId, first: u16) -> bool {
    let low = id.raw() & 0xFFFF;
    (u32::from(first)..u32::from(first) + u32::from(BLOCK)).contains(&low)
}

/// Make every family into `out`.
///
/// # Errors
/// A table that does not read, a creature that does not make, a family that outgrows its block
/// of ids, or a record whose id `out` holds already.
pub fn make(ac2: &Ac2, out: &mut Records) -> Result<Vec<Family>, String> {
    let mut families = Vec::new();
    for (k, (name, text)) in FAMILIES.iter().enumerate() {
        let mut table = Table::parse(text).map_err(|e| format!("creature {name}: {e}"))?;
        let first = first_of(k);
        let mut ids = make::Ids::from(first);
        let mut records = Records::default();
        let made = make::make(ac2, &table, &mut ids, &mut records)
            .map_err(|e| format!("creature {name}: {e}"))?;
        // Its drawn icon, when it has one, in place of its placeholder.
        if let Some((id, record)) = drawn_icon(k, name, DRAWN)? {
            if records.0.contains_key(&id) {
                return Err(format!(
                    "creature {name}: its icon's id {id:?} is one of its records"
                ));
            }
            records.add(id, record);
            table.icon = Some(id.raw());
        }
        for (id, bytes) in records.0 {
            if !in_block(id, first) {
                return Err(format!(
                    "creature {name}: {id:?} lies beyond its ids ({first:04X} and {BLOCK:#X} on)"
                ));
            }
            if out.0.contains_key(&id) {
                return Err(format!(
                    "creature {name}: {id:?} is a record the land made already"
                ));
            }
            out.add(id, bytes);
        }
        families.push(Family {
            table,
            made,
            creature_type: 0,
        });
    }
    Ok(families)
}

/// `creatures.tsv`: a line per look, the family's own first, with the ids its weenie names, the
/// texture swaps it lays on the body's parts (`part:old>new`, space separated; none for a
/// family's own look), and its family's creature type and icon.
#[must_use]
pub fn table(families: &[Family]) -> String {
    let mut s = String::from(
        "look\tfamily\tlabel\tsetup\tmotion_table\tcombat_table\ttranslucency\ttexture_swaps\tcreature_type\ticon\n",
    );
    for f in families {
        let m = &f.made;
        let mut line = |look: &str, translucency: Option<f64>, swaps: &[(u16, DataId, DataId)]| {
            let swaps: Vec<String> = swaps
                .iter()
                .map(|(part, old, new)| format!("{part}:{:08X}>{:08X}", old.raw(), new.raw()))
                .collect();
            let _ = writeln!(
                s,
                "{look}\t{}\t{}\t{:08X}\t{:08X}\t{:08X}\t{}\t{}\t{}\t{}",
                f.table.name,
                f.table.label,
                m.setup.raw(),
                m.motion_table.raw(),
                m.combat_table.raw(),
                translucency.map_or(String::new(), |t| format!("{t:.2}")),
                swaps.join(" "),
                f.creature_type,
                f.table.icon.map_or(String::new(), |i| format!("{i:08X}"))
            );
        };
        line(&f.table.name, None, &[]);
        for l in &m.looks {
            line(&l.name, l.translucency, &l.parts);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's creatures)

    use super::*;

    /// Each family's ids start a block on from the last, and all of them end below the land's own.
    #[test]
    fn every_familys_block_lies_below_the_lands_own_ids_and_apart_from_the_others() {
        let blocks: Vec<u16> = (0..FAMILIES.len()).map(first_of).collect();
        assert_eq!(blocks[0], 0xB000);
        assert!(blocks.windows(2).all(|w| w[1] - w[0] == BLOCK));
        let last = u32::from(*blocks.last().unwrap()) + u32::from(BLOCK);
        assert!(last <= 0xC000, "the last block ends at {last:#X}");
    }

    /// Each family is a creature type numbered on from AC1's highest, in the families' order, and
    /// named as it is labeled, a space as the files write one; AC1's own types stay as they were.
    #[test]
    fn each_family_is_a_creature_type_after_ac1s_named_by_its_label() {
        let ac1 = dereth_assets::tables::EnumMapper {
            id: DataId(TYPES_ID),
            base_emp_did: DataId(0),
            bucket_index: 0,
            id_to_string: vec![
                (1, "Olthoi".into()),
                (40, "Unknown".into()),
                (3, "Drudge".into()),
            ],
        };
        let (m, numbers) = types(&ac1, &["Tyrant", "Dark fowl"]);
        assert_eq!(&m.id_to_string[..3], &ac1.id_to_string[..]);
        assert_eq!(
            &m.id_to_string[3..],
            &[(41, "Tyrant".to_owned()), (42, "Dark_fowl".to_owned())]
        );
        assert_eq!(numbers, [41, 42]);
    }

    /// A generated icon as a PNG: `side` a side, RGBA, each texel's red its column and green its
    /// row.
    fn png_of(side: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, side, side);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..side * side)
            .flat_map(|i| {
                let (x, y) = (i % side, i / side);
                [u8::try_from(x).unwrap(), u8::try_from(y).unwrap(), 7, 200]
            })
            .collect();
        e.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    /// A drawn icon is written as Asheron's Call's own icons are (32 × 32, A8R8G8B8, a portal
    /// picture), its texels blue, green, red, alpha; one of another size is refused.
    #[test]
    fn a_drawn_icon_is_a_32_by_32_picture_as_the_clients_icons_are() {
        use dereth_assets::Decode;
        let id = DataId(0x0600_B0FF);
        let record = icon_record(id, &png_of(ICON_SIDE)).unwrap();
        let rs = dereth_assets::RenderSurface::decode_payload(id, &record).unwrap();
        assert_eq!(
            (rs.width, rs.height, rs.format, rs.data_category),
            (32, 32, 21, 6)
        );
        let px = rs.payload(&record).unwrap();
        assert_eq!(px.len(), 32 * 32 * 4);
        // The texel at column 5, row 9: blue 7, green 9, red 5, alpha 200.
        let at = (9 * 32 + 5) * 4;
        assert_eq!(&px[at..at + 4], &[7, 9, 5, 200]);
        let e = icon_record(id, &png_of(16)).unwrap_err();
        assert!(e.contains("16 × 16"), "{e}");
    }

    /// A family's drawn icon takes the last picture id of its own block, and one without a
    /// drawing keeps none (its placeholder stands).
    #[test]
    fn a_familys_drawn_icon_takes_the_last_id_of_its_block() {
        let png = png_of(ICON_SIDE);
        let drawn = [("beetle", png.as_slice())];
        let (id, _) = drawn_icon(1, "beetle", &drawn).unwrap().unwrap();
        assert_eq!(id, DataId(0x0600_B1FF));
        assert!(in_block(id, first_of(1)) && !in_block(id, first_of(2)));
        assert!(drawn_icon(0, "tyrant", &drawn).unwrap().is_none());
        let bad = [("beetle", &b"not a png"[..])];
        assert!(drawn_icon(1, "beetle", &bad)
            .unwrap_err()
            .starts_with("creature beetle: its icon"));
    }

    /// An id is its family's when its low half lies in the family's block.
    #[test]
    fn an_id_is_in_its_familys_block_and_not_the_next() {
        assert!(in_block(DataId(0x0200_B1FF), 0xB100));
        assert!(!in_block(DataId(0x0200_B200), 0xB100));
        assert!(!in_block(DataId(0x0200_B0FF), 0xB100));
    }

    /// Each table the land carries reads, and is the family it is listed as.
    #[test]
    fn every_familys_table_reads_and_names_itself() {
        for (name, text) in FAMILIES {
            let t = Table::parse(text).unwrap();
            assert_eq!(t.name, name);
        }
    }
}
