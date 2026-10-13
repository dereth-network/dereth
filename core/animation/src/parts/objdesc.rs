//! Applying an `ObjDesc`: part swaps, texture-map swaps and palette shifts.
//!
//! `ObjDesc` is the server's per-object appearance override; its wire form belongs to
//! the protocol crate. The shape below contains only the fields used to apply those overrides.
//!
//! The failure discipline is the client's and is easy to get wrong: **a bad index or a failed load
//! makes the whole call report failure, but the rest of the list still runs.**

use dereth_primitives::DataId;

use super::PartArray;
use crate::data::{AnimAssets, NoAssets};

/// `AnimPartChange { part_index, part_id }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimPartChange {
    pub part_index: u32,
    pub part_id: DataId,
}

/// `TextureMapChange { part_index, old_tex_id, new_tex_id }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureMapChange {
    pub part_index: u32,
    pub old_texture: DataId,
    pub new_texture: DataId,
}

/// One sub-palette range. Offsets and lengths are in 8-entry units and a length of 0 means 256;
/// resolving that is the client's job, so the raw values travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteRange {
    pub palette_set: DataId,
    pub offset: u32,
    pub length: u32,
}

/// The subset of the server's object description this crate applies.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjDesc {
    pub part_changes: Vec<AnimPartChange>,
    pub texture_changes: Vec<TextureMapChange>,
    pub palette_id: DataId,
    pub subpalettes: Vec<PaletteRange>,
}

impl PartArray {
    /// Apply appearance overrides without a graphics-object lookup.
    ///
    /// Equivalent to [`PartArray::do_obj_desc_changes_with`] passing
    /// [`NoAssets`], i.e. every part swap answers
    /// [`GfxObjLookup::Unknown`](crate::data::GfxObjLookup::Unknown) and so is accepted without
    /// checking whether the dat holds the object, and a swapped part keeps the base palette it
    /// had, so a palette recolors only parts whose palette was read through a lookup.
    /// **Prefer the `_with` form.** This one exists
    /// because the three production call sites live in `dereth-client` (`world.rs` twice and
    /// `preview.rs` once).
    pub fn do_obj_desc_changes(&mut self, od: &ObjDesc) -> bool {
        self.do_obj_desc_changes_with(od, &NoAssets)
    }

    /// Apply the object-description changes to the part array.
    ///
    /// Returns false if any single change failed, having applied every change that could be
    /// applied — which is what the client does, `ok &= …` at each step.
    pub fn do_obj_desc_changes_with(&mut self, od: &ObjDesc, assets: &dyn AnimAssets) -> bool {
        let mut ok = true;
        let mut touched = false;
        if !od.part_changes.is_empty() {
            ok &= self.set_part_list(&od.part_changes, assets);
            touched = true;
        }
        if !od.texture_changes.is_empty() {
            ok &= self.set_texture_map_list(&od.texture_changes);
            touched = true;
        }
        if od.subpalettes.is_empty() {
            if !touched {
                return ok;
            }
        } else {
            ok &= self.set_palette(od.palette_id, &od.subpalettes);
        }
        ok
    }

    /// Restore every part's palette first,
    /// then apply. This is the path taken when the server sends a fresh object description.
    pub fn do_obj_desc_changes_from_default(&mut self, od: &ObjDesc) -> bool {
        self.do_obj_desc_changes_from_default_with(od, &NoAssets)
    }

    /// Apply appearance overrides from the default state with the graphics-object lookup available.
    pub fn do_obj_desc_changes_from_default_with(
        &mut self,
        od: &ObjDesc,
        assets: &dyn AnimAssets,
    ) -> bool {
        for p in &mut self.parts {
            p.restore_palette();
        }
        self.do_obj_desc_changes_with(od, assets)
    }

    /// Walk the change list and apply each part swap.
    ///
    /// The loop is unconditional: the client does **not** compare the new id with the one the part
    /// already has, so a change naming the part the setup already carries still goes through
    /// `PhysicsPart::set_part` and still restores that part's surfaces. 2,017 of the capture
    /// corpus's 2,967 part swaps are exactly that — the id the setup already has — and they are how
    /// an unequip takes the garment's texture back off.
    fn set_part_list(&mut self, changes: &[AnimPartChange], assets: &dyn AnimAssets) -> bool {
        let mut ok = true;
        for c in changes {
            match self
                .parts
                .get_mut(usize::try_from(c.part_index).unwrap_or(usize::MAX))
            {
                Some(p) => ok &= p.set_part(c.part_id, assets),
                None => ok = false,
            }
        }
        ok
    }

    /// Apply a texture-map list.
    fn set_texture_map_list(&mut self, changes: &[TextureMapChange]) -> bool {
        let mut ok = true;
        for c in changes {
            match self
                .parts
                .get_mut(usize::try_from(c.part_index).unwrap_or(usize::MAX))
            {
                Some(p) => p.set_texture_map(c.old_texture, c.new_texture),
                None => ok = false,
            }
        }
        ok
    }

    /// Apply the shift palette and its ranges to the parts whose base palette it is.
    ///
    /// A part drawn with another palette keeps its own colors: a costume's limbs, painted against
    /// their own palette, are not recolored by the skin, hair and eye ranges a body's description
    /// carries over the body's palette. Indexing those limbs' pictures through the body's palette
    /// would draw them as noise. A description naming no palette recolors nothing and fails.
    fn set_palette(&mut self, palette: DataId, ranges: &[PaletteRange]) -> bool {
        if self.parts.is_empty() || palette == DataId(0) {
            return false;
        }
        for p in &mut self.parts {
            if p.original_palette_id == palette {
                p.use_palette(palette, ranges);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::data::{MapAssets, NoAssets, SetupData};
    use crate::parts::PartArray;
    use crate::seq::Sequence;

    fn array() -> PartArray {
        let setup = Arc::new(SetupData {
            parts: vec![DataId(0x0100_0001), DataId(0x0100_0002)],
            ..SetupData::default()
        });
        let mut seq = Sequence::new();
        PartArray::create_setup(setup, true, &mut seq, &NoAssets).expect("setup")
    }

    /// The body's palette, which a description's skin, hair and eye ranges are laid over.
    const BODY: DataId = DataId(0x0400_0001);
    /// The palette a costume's limbs are painted against.
    const COSTUME: DataId = DataId(0x0400_0002);

    /// Graphics objects `0x01000001`, `0x01000002` and `0x01000099` drawn with the body's
    /// palette, `0x010000C1` and `0x010000C2` with the costume's, and `0x010000D0` with none.
    fn assets() -> MapAssets {
        let ids = [
            0x0100_0001,
            0x0100_0002,
            0x0100_0099,
            0x0100_00C1,
            0x0100_00C2,
            0x0100_00D0,
        ];
        MapAssets {
            gfxobjs: Some(ids.into_iter().map(|id| (id, None)).collect()),
            base_palettes: [
                (0x0100_0001, BODY),
                (0x0100_0002, BODY),
                (0x0100_0099, BODY),
                (0x0100_00C1, COSTUME),
                (0x0100_00C2, COSTUME),
            ]
            .into_iter()
            .collect(),
            ..MapAssets::default()
        }
    }

    /// A body of three parts drawn with the body's palette.
    fn body(a: &MapAssets) -> PartArray {
        let setup = Arc::new(SetupData {
            parts: vec![
                DataId(0x0100_0001),
                DataId(0x0100_0002),
                DataId(0x0100_0001),
            ],
            ..SetupData::default()
        });
        let mut seq = Sequence::new();
        PartArray::create_setup(setup, true, &mut seq, a).expect("setup")
    }

    fn skin() -> Vec<PaletteRange> {
        vec![PaletteRange {
            palette_set: DataId(0x0400_0F00),
            offset: 0,
            length: 0x18,
        }]
    }

    fn shift(p: &crate::parts::PhysicsPart) -> Option<DataId> {
        p.surface_overrides.as_ref().and_then(|s| s.shift_palette)
    }

    /// ORACLE: the recovered part-array behavior and the three setters the object-description path
    /// calls.
    #[test]
    fn part_texture_and_palette_changes_all_apply() {
        let a = assets();
        let mut pa = body(&a);
        let od = ObjDesc {
            part_changes: vec![AnimPartChange {
                part_index: 1,
                part_id: DataId(0x0100_0099),
            }],
            texture_changes: vec![TextureMapChange {
                part_index: 0,
                old_texture: DataId(0x0500_0001),
                new_texture: DataId(0x0500_0002),
            }],
            palette_id: DataId(0x0400_0001),
            subpalettes: vec![PaletteRange {
                palette_set: DataId(0x0F00_0001),
                offset: 0,
                length: 4,
            }],
        };
        assert!(pa.do_obj_desc_changes_with(&od, &a));
        assert_eq!(pa.parts[1].gfxobj_id, DataId(0x0100_0099));
        assert_eq!(
            pa.parts[0]
                .surface_overrides
                .as_ref()
                .expect("copied")
                .texture_maps
                .len(),
            1
        );
        // Every part is drawn with the description's palette, so every part was copied.
        for p in &pa.parts {
            assert_eq!(shift(p), Some(BODY));
        }
    }

    /// A description's palette recolors the parts drawn with it and no others: limbs a costume
    /// swaps in, painted against their own palette, keep their colors, and the parts left of the
    /// body take the skin.
    #[test]
    fn a_palette_recolors_only_the_parts_drawn_with_it() {
        let a = assets();
        let mut pa = body(&a);
        let od = ObjDesc {
            part_changes: vec![
                AnimPartChange {
                    part_index: 0,
                    part_id: DataId(0x0100_00C1),
                },
                AnimPartChange {
                    part_index: 1,
                    part_id: DataId(0x0100_00C2),
                },
            ],
            palette_id: BODY,
            subpalettes: skin(),
            ..ObjDesc::default()
        };
        assert!(pa.do_obj_desc_changes_from_default_with(&od, &a));
        assert_eq!(pa.parts[0].original_palette_id, COSTUME);
        assert_eq!(
            shift(&pa.parts[0]),
            None,
            "a costume limb keeps its palette"
        );
        assert_eq!(
            shift(&pa.parts[1]),
            None,
            "a costume limb keeps its palette"
        );
        assert_eq!(
            shift(&pa.parts[2]),
            Some(BODY),
            "the body's part takes the skin"
        );
        assert_eq!(
            pa.parts[2]
                .surface_overrides
                .as_ref()
                .expect("copied")
                .subpalettes,
            skin()
        );

        // The costume's own palette recolors the costume and not the body.
        let od = ObjDesc {
            palette_id: COSTUME,
            subpalettes: skin(),
            ..ObjDesc::default()
        };
        assert!(pa.do_obj_desc_changes_from_default_with(&od, &a));
        assert_eq!(shift(&pa.parts[0]), Some(COSTUME));
        assert_eq!(shift(&pa.parts[1]), Some(COSTUME));
        assert_eq!(shift(&pa.parts[2]), None);
    }

    /// A part whose graphics object has no palette is never recolored, and a description that
    /// names no palette recolors nothing and reports failure.
    #[test]
    fn a_part_without_a_palette_and_a_description_without_one_recolor_nothing() {
        let a = assets();
        let mut pa = body(&a);
        let od = ObjDesc {
            part_changes: vec![AnimPartChange {
                part_index: 0,
                part_id: DataId(0x0100_00D0),
            }],
            palette_id: BODY,
            subpalettes: skin(),
            ..ObjDesc::default()
        };
        assert!(pa.do_obj_desc_changes_from_default_with(&od, &a));
        assert_eq!(pa.parts[0].original_palette_id, DataId(0));
        assert_eq!(shift(&pa.parts[0]), None);
        assert_eq!(shift(&pa.parts[1]), Some(BODY));

        let mut pa = body(&a);
        let od = ObjDesc {
            palette_id: DataId(0),
            subpalettes: skin(),
            ..ObjDesc::default()
        };
        assert!(!pa.do_obj_desc_changes_from_default_with(&od, &a));
        assert!(pa.parts.iter().all(|p| shift(p).is_none()));
    }

    /// A graphics object that names a degrade record is drawn from the record's level 0, and its
    /// part's base palette is that object's, not the palette of the object that names the record.
    #[test]
    fn a_parts_base_palette_is_its_degrade_records_level_zeros() {
        let mut a = assets();
        a.gfxobjs
            .as_mut()
            .expect("an index")
            .insert(0x0100_00E0, Some(DataId(0x1100_00E0)));
        a.base_palettes.insert(0x0100_00E0, BODY);
        a.degrades.insert(
            0x1100_00E0,
            Arc::new(crate::data::DegradeInfo {
                degrades: vec![crate::data::GfxObjInfo {
                    gfxobj_id: DataId(0x0100_00C1),
                    degrade_mode: 1,
                    min_dist: 0.0,
                    ideal_dist: 0.0,
                    max_dist: f32::MAX,
                }],
            }),
        );
        let mut pa = body(&a);
        let od = ObjDesc {
            part_changes: vec![AnimPartChange {
                part_index: 0,
                part_id: DataId(0x0100_00E0),
            }],
            palette_id: BODY,
            subpalettes: skin(),
            ..ObjDesc::default()
        };
        assert!(pa.do_obj_desc_changes_from_default_with(&od, &a));
        assert_eq!(pa.parts[0].original_palette_id, COSTUME);
        assert_eq!(shift(&pa.parts[0]), None);
    }

    /// A bad part index fails the call but the rest of the list still runs.
    #[test]
    fn a_bad_index_fails_the_call_without_skipping_the_rest() {
        let mut pa = array();
        let od = ObjDesc {
            part_changes: vec![
                AnimPartChange {
                    part_index: 99,
                    part_id: DataId(0x0100_00AA),
                },
                AnimPartChange {
                    part_index: 0,
                    part_id: DataId(0x0100_00BB),
                },
            ],
            ..ObjDesc::default()
        };
        assert!(!pa.do_obj_desc_changes(&od));
        assert_eq!(
            pa.parts[0].gfxobj_id,
            DataId(0x0100_00BB),
            "the valid change still applied"
        );
    }

    /// An empty `ObjDesc` changes nothing and reports success, and copy-on-write means no part
    /// gained a surface copy.
    #[test]
    fn an_empty_objdesc_touches_nothing() {
        let mut pa = array();
        assert!(pa.do_obj_desc_changes(&ObjDesc::default()));
        assert!(pa.parts.iter().all(|p| p.surface_overrides.is_none()));
        assert_eq!(pa.parts[0].gfxobj_id, DataId(0x0100_0001));
    }

    /// `DoObjDescChangesFromDefault` restores the palettes first.
    #[test]
    fn from_default_restores_the_palette_before_applying() {
        let mut pa = array();
        pa.parts[0].use_palette(DataId(0x0400_00FF), &[]);
        let od = ObjDesc {
            part_changes: vec![AnimPartChange {
                part_index: 0,
                part_id: DataId(0x0100_00CC),
            }],
            ..ObjDesc::default()
        };
        assert!(pa.do_obj_desc_changes_from_default(&od));
        // The old shift palette is gone and no new one was applied.
        assert!(pa.parts[0].surface_overrides.is_none());
        assert_eq!(pa.parts[0].gfxobj_id, DataId(0x0100_00CC));
    }
}
