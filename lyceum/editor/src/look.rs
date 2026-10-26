//! A weenie's look, read from its SQL and the data files: what the weenie editor's preview draws
//! and what its clothing, color and shade pickers offer.
//!
//! The look is worked out as a server works it out ([`dereth_rules::appearance`]): the weenie's
//! setup, its clothing look in its color scheme at its shade, and for a creature, the clothing its
//! create list always wields. The description that gives is the one the client draws, so the
//! preview applies it exactly as the client applies a server's.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use dereth_assets::motion::ClothingTable;
use dereth_assets::tables::CharGen;
use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_rules::appearance::{self, Look, LookFiles, OwnChanges, Properties, Wield};
use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::sql_ids::number;

/// The create list's table.
const CREATE_LIST: &str = "weenie_properties_create_list";

/// A create list row's destination bits: wielded, and chosen by chance.
const WIELD: u32 = 2;
const TREASURE: u32 = 8;

/// A weenie's properties as its SQL writes them.
#[derive(Debug)]
pub struct SqlProps<'a>(pub &'a SqlDoc);

impl SqlProps<'_> {
    fn raw(&self, kind: PropertyKind, id: u32) -> Option<String> {
        self.0.property(kind, id).map(|v| content::bare(&v))
    }
}

impl Properties for SqlProps<'_> {
    fn int(&self, id: u32) -> Option<i32> {
        let v = self.raw(PropertyKind::Int, id)?;
        // Bits written as an unsigned number are the same bits.
        #[allow(clippy::cast_possible_truncation)]
        v.trim().parse::<i64>().ok().map(|n| n as i32)
    }

    fn float(&self, id: u32) -> Option<f64> {
        self.raw(PropertyKind::Float, id)?.trim().parse().ok()
    }

    fn data_id(&self, id: u32) -> Option<u32> {
        number(&self.raw(PropertyKind::DataId, id)?)
    }

    fn bool(&self, id: u32) -> Option<bool> {
        let v = self.raw(PropertyKind::Bool, id)?;
        Some(matches!(
            content::unquote(&v).trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "\u{1}"
        ))
    }

    fn string(&self, id: u32) -> Option<String> {
        self.0
            .property(PropertyKind::String, id)
            .map(|v| content::unquote(&v))
    }
}

/// The weenie type `doc`'s `weenie` row names; 0 when it names none.
#[must_use]
pub fn weenie_type(doc: &SqlDoc) -> u32 {
    doc.rows("weenie")
        .first()
        .and_then(|r| doc.cell(*r, "type"))
        .and_then(|v| number(&v))
        .unwrap_or(0)
}

/// Column `c` of row `r` as a number, its bits kept whatever its sign.
fn cell_bits(doc: &SqlDoc, r: content::RowRef, c: &str) -> Option<u32> {
    let v = doc.cell(r, c)?;
    let v = v.trim();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    number(v).or_else(|| v.parse::<i64>().ok().map(|n| n as u32))
}

/// The part, texture and palette changes `doc` lists of its own.
#[must_use]
pub fn own_changes(doc: &SqlDoc) -> OwnChanges {
    let byte = |v: Option<u32>| u8::try_from(v.unwrap_or(0) & 0xFF).unwrap_or(0);
    let half = |v: Option<u32>| u16::try_from(v.unwrap_or(0) & 0xFFFF).unwrap_or(0);
    OwnChanges {
        parts: doc
            .rows("weenie_properties_anim_part")
            .into_iter()
            .map(|r| {
                (
                    byte(cell_bits(doc, r, "index")),
                    cell_bits(doc, r, "animation_Id").unwrap_or(0),
                )
            })
            .collect(),
        textures: doc
            .rows("weenie_properties_texture_map")
            .into_iter()
            .map(|r| {
                (
                    byte(cell_bits(doc, r, "index")),
                    cell_bits(doc, r, "old_Id").unwrap_or(0),
                    cell_bits(doc, r, "new_Id").unwrap_or(0),
                )
            })
            .collect(),
        palettes: doc
            .rows("weenie_properties_palette")
            .into_iter()
            .map(|r| {
                (
                    cell_bits(doc, r, "sub_Palette_Id").unwrap_or(0),
                    half(cell_bits(doc, r, "offset")),
                    half(cell_bits(doc, r, "length")),
                )
            })
            .collect(),
    }
}

/// The look `doc` writes.
#[must_use]
pub fn look_of(doc: &SqlDoc) -> Look {
    Look::of(weenie_type(doc), &SqlProps(doc), own_changes(doc))
}

/// What `doc`'s create list always wields, as the server makes each piece: each row wielded that
/// is not chosen by chance, its weenie read by `weenie` (from the class as the row writes it),
/// in the list's order. A row whose weenie is not known yet is left out and counted.
pub fn wields_of(
    doc: &SqlDoc,
    weenie: &mut dyn FnMut(&str) -> Option<SqlDoc>,
) -> (Vec<Wield>, usize) {
    let mut out = Vec::new();
    let mut unknown = 0;
    for r in doc.rows(CREATE_LIST) {
        let destination = cell_bits(doc, r, "destination_Type").unwrap_or(0);
        if destination & WIELD == 0 {
            continue;
        }
        let shade: f32 = doc
            .cell(r, "shade")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0.0);
        let treasure = destination & TREASURE != 0;
        // A treasure row with a chance is drawn by chance: the preview shows what is always worn.
        if treasure && shade != 0.0 {
            continue;
        }
        let Some(class) = doc.raw(r, "weenie_Class_Id").map(str::to_owned) else {
            continue;
        };
        if number(&class) == Some(0) {
            continue;
        }
        let Some(item) = weenie(&class) else {
            unknown += 1;
            continue;
        };
        #[allow(clippy::cast_possible_wrap)]
        let palette = cell_bits(doc, r, "palette").map_or(0, |p| p as i32);
        out.push(Wield::of(weenie_type(&item), &SqlProps(&item)).made(palette, shade, treasure));
    }
    (out, unknown)
}

/// A clothing table read, and the color scheme its file lists first.
type Table = Option<(Arc<ClothingTable>, Option<u32>)>;

/// The data files a look is read from: `store`'s clothing tables, palette sets, setups and
/// character creation table, each read once.
pub struct DatFiles {
    store: Arc<RetailDatStore>,
    tables: RefCell<HashMap<u32, Table>>,
    sets: RefCell<HashMap<u32, Vec<u32>>>,
    setups: RefCell<HashMap<u32, Vec<u32>>>,
    palettes: RefCell<HashMap<u32, Option<Arc<Vec<u32>>>>>,
    char_gen: OnceCell<Option<CharGen>>,
}

impl std::fmt::Debug for DatFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatFiles")
            .field("tables", &self.tables.borrow().len())
            .finish_non_exhaustive()
    }
}

/// Record `id` of `store`, decoded in its files' layout.
fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: u32) -> Option<(T, Vec<u8>)> {
    let bytes = store.read_typed(kind, DataId(id)).ok()?;
    let v = T::decode_payload_in(store.era_of(DataId(id)), DataId(id), &bytes).ok()?;
    Some((v, bytes))
}

impl DatFiles {
    #[must_use]
    pub fn new(store: Arc<RetailDatStore>) -> Self {
        Self {
            store,
            tables: RefCell::default(),
            sets: RefCell::default(),
            setups: RefCell::default(),
            palettes: RefCell::default(),
            char_gen: OnceCell::new(),
        }
    }

    /// The files read.
    #[must_use]
    pub fn store(&self) -> &Arc<RetailDatStore> {
        &self.store
    }

    fn table(&self, id: u32) -> Table {
        if let Some(t) = self.tables.borrow().get(&id) {
            return t.clone();
        }
        let t = read::<ClothingTable>(&self.store, DbType::Clothing, id).map(|(t, bytes)| {
            let first =
                ClothingTable::palette_template_order(&bytes).and_then(|o| o.first().copied());
            (Arc::new(t), first)
        });
        self.tables.borrow_mut().insert(id, t.clone());
        t
    }

    /// Palette `id`'s colors, each `0xAARRGGBB`; `None` for one the files do not have.
    #[must_use]
    pub fn palette(&self, id: u32) -> Option<Arc<Vec<u32>>> {
        if let Some(p) = self.palettes.borrow().get(&id) {
            return p.clone();
        }
        let p = read::<dereth_assets::material::Palette>(&self.store, DbType::Palette, id)
            .map(|(p, _)| Arc::new(p.colors_argb));
        self.palettes.borrow_mut().insert(id, p.clone());
        p
    }
}

impl LookFiles for DatFiles {
    fn clothing(&self, id: u32) -> Option<Arc<ClothingTable>> {
        self.table(id).map(|(t, _)| t)
    }

    fn first_palette_template(&self, id: u32) -> Option<u32> {
        self.table(id).and_then(|(_, f)| f)
    }

    fn palette_set(&self, id: u32) -> Vec<u32> {
        if let Some(s) = self.sets.borrow().get(&id) {
            return s.clone();
        }
        let s: Vec<u32> =
            read::<dereth_assets::material::PaletteSet>(&self.store, DbType::PalSet, id)
                .map(|(s, _)| s.palette_ids.iter().map(|p| p.0).collect())
                .unwrap_or_default();
        self.sets.borrow_mut().insert(id, s.clone());
        s
    }

    fn setup_parts(&self, id: u32) -> Vec<u32> {
        if let Some(s) = self.setups.borrow().get(&id) {
            return s.clone();
        }
        let s: Vec<u32> = read::<dereth_assets::Setup>(&self.store, DbType::Setup, id)
            .map(|(s, _)| s.parts.iter().map(|p| p.0).collect())
            .unwrap_or_default();
        self.setups.borrow_mut().insert(id, s.clone());
        s
    }

    fn char_gen(&self) -> Option<&CharGen> {
        self.char_gen
            .get_or_init(|| {
                read::<CharGen>(&self.store, DbType::CharGen, 0x0E00_0002).map(|(c, _)| c)
            })
            .as_ref()
    }
}

/// The colors color scheme `key` of clothing table `table` gives at `shade`: one for each of its
/// palette changes, `[r, g, b]`. A range holds a ramp from dark to light, of which the cloth
/// mostly shows the lighter part, so each is the mean of the lighter half of its ranges' colors.
#[must_use]
pub fn scheme_colors(files: &DatFiles, table: u32, key: u32, shade: f64) -> Vec<[u8; 3]> {
    let Some(t) = files.clothing(table) else {
        return Vec::new();
    };
    let Some(scheme) = t.palette_templates.get(&key) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for effect in &scheme.subpalette_effects {
        let id = appearance::palette_at(&files.palette_set(effect.palette_set.0), shade);
        let Some(colors) = files.palette(id) else {
            continue;
        };
        let rgb = |c: u32| [(c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF];
        // The ranges count a later palette's 2,048 entries; a palette of the files from before
        // Throne of Destiny holds 256, each eight of those.
        let unit = if colors.len() <= 256 { 8 } else { 1 };
        let mut all: Vec<[u32; 3]> = Vec::new();
        for r in &effect.ranges {
            let from = usize::try_from(r.offset / unit).unwrap_or(usize::MAX);
            let to = from.saturating_add(usize::try_from(r.length / unit).unwrap_or(0));
            all.extend(
                colors
                    .get(from..to.min(colors.len()))
                    .unwrap_or(&[])
                    .iter()
                    .map(|c| rgb(*c)),
            );
        }
        if all.is_empty() {
            continue;
        }
        all.sort_by_key(|[r, g, b]| r * 3 + g * 6 + b);
        let light = &all[all.len() / 2..];
        let n = u32::try_from(light.len()).unwrap_or(1).max(1);
        let mut sum = [0u32; 3];
        for c in light {
            for (s, v) in sum.iter_mut().zip(c) {
                *s += v;
            }
        }
        out.push(sum.map(|v| u8::try_from(v / n).unwrap_or(u8::MAX)));
    }
    out
}

/// Which clothing tables dress each setup: every clothing table of the files, read once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClothingIndex {
    pub by_setup: BTreeMap<u32, Vec<u32>>,
}

impl ClothingIndex {
    /// The index of `store`'s clothing tables.
    #[must_use]
    pub fn read(store: &RetailDatStore) -> Self {
        let mut by_setup: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for id in store.ids_of(DbType::Clothing) {
            let Some((t, _)) = read::<ClothingTable>(store, DbType::Clothing, id.0) else {
                continue;
            };
            for setup in t.clothing_bases.keys() {
                by_setup.entry(setup.0).or_default().push(id.0);
            }
        }
        Self { by_setup }
    }

    /// The clothing tables that dress `setup`, in id order.
    #[must_use]
    pub fn dressing(&self, setup: u32) -> &[u32] {
        self.by_setup.get(&setup).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the weenie editor's look)
    use super::*;

    const ROBED: &str = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90001, 'robed', 10, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90001, 3, 14) /* PaletteTemplate */
     , (90001, 9, -2147483648) /* ValidLocations */;
INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES (90001, 12, 0.33) /* Shade */;
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90001, 1, 0x02000001) /* Setup */
     , (90001, 7, 0x10000002) /* ClothingBase */;
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90001, 3, 'Male') /* Sex */;
INSERT INTO `weenie_properties_palette` (`object_Id`, `sub_Palette_Id`, `offset`, `length`)
VALUES (90001, 67109558, 0, 24);
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES (90001, 2, 127, 0, 7, 0.33, False)
     , (90001, 10, 130, 0, 0, 0.5, False)
     , (90001, 10, 132, 0, 0, 0, False)
     , (90001, 4, 136, 0, 0, 0, False)
     , (90001, 2, {{wcid:holtburg:hat}}, 0, 0, 0, False);";

    #[test]
    fn a_weenies_look_is_read_from_its_properties_and_rows() {
        let doc = SqlDoc::parse(ROBED);
        let look = look_of(&doc);
        assert_eq!(look.weenie_type, 10);
        assert_eq!(look.setup, Some(0x0200_0001));
        assert_eq!(look.clothing_base, Some(0x1000_0002));
        assert_eq!(look.palette_template, Some(14));
        assert_eq!(look.shade, Some(0.33));
        assert_eq!(look.sex.as_deref(), Some("Male"));
        assert_eq!(look.own.palettes, [(0x0400_02B6, 0, 24)]);
        assert_eq!(SqlProps(&doc).int(9), Some(i32::MIN), "bits kept");
    }

    #[test]
    fn what_is_always_wielded_is_read_through_its_weenies_and_rows_by_chance_are_left_out() {
        let doc = SqlDoc::parse(ROBED);
        let item = |class: u32, ty: u32| {
            SqlDoc::parse(&format!(
                "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({class}, 'x', {ty}, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({class}, 3, 2) /* PaletteTemplate */;"
            ))
        };
        let mut asked = Vec::new();
        let (wields, unknown) = wields_of(&doc, &mut |c: &str| {
            asked.push(c.to_owned());
            match c {
                "127" => Some(item(127, 2)),
                "132" => Some(item(132, 2)),
                _ => None,
            }
        });
        // The pants (always), the shoes (a treasure row with no chance), not the shirt (by
        // chance) nor the pack (held, not wielded); the hat's weenie is not known.
        assert_eq!(asked, ["127", "132", "{{wcid:holtburg:hat}}"]);
        assert_eq!(unknown, 1);
        assert_eq!(wields.len(), 2);
        assert_eq!(
            (wields[0].palette_template, wields[0].shade),
            (Some(7), Some(f64::from(0.33f32)))
        );
        // The treasure row's shade is its chance, so the shoes keep their own.
        assert_eq!(
            (wields[1].palette_template, wields[1].shade),
            (Some(2), None)
        );
    }
}
