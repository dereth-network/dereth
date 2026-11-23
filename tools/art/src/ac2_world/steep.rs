//! AC2's steep ground in AC1's terrain words: AC2 draws each vertex with the material group of its
//! kind that its slope falls in, so its slopes draw another picture than its flat ground (grass's
//! slopes rock). AC1's terrain chooses no picture by slope, so a vertex whose group draws another
//! picture than its kind's flat ground takes a variant of its slot of its own (bits 7-10), whose
//! terrain texture is that group's picture.
//!
//! A steep variant is shared by every kind of the slot whose steep group draws the same material,
//! and is numbered from the top (15 down), clear of the variants that keep the slot's kinds apart
//! ([`crate::ac2_world::kinds::kinds`]).

use std::collections::BTreeMap;

use auberean_assets::region::{SurfaceDesc, SurfaceType, TerrainDesc};

use crate::ac2_world::kinds;

/// The surface AC2 draws kind `kind` with: the kind's own, or the default kind's for a kind it
/// does not list.
#[must_use]
pub fn surface_of<'a>(
    terrain: &TerrainDesc,
    surface: &'a SurfaceDesc,
    kind: u32,
) -> Option<&'a SurfaceType> {
    let t = terrain
        .terrains
        .iter()
        .find(|t| t.index == kind)
        .or(terrain.default_terrain.as_ref())?;
    surface.surfaces.iter().find(|s| s.index == t.surface)
}

/// The group of `s` a vertex of slope `pitch` (`n.x² + n.y²` of its unit normal) draws: the first
/// whose range (the sines of its slopes) holds it.
#[must_use]
pub fn group_at(s: &SurfaceType, pitch: f32) -> Option<usize> {
    s.groups
        .iter()
        .position(|g| g.min_pitch * g.min_pitch <= pitch && pitch <= g.max_pitch * g.max_pitch)
}

/// One steep variant: the slot and variant its vertices take, and the AC2 material it draws (its
/// group's first base material) with that material's tile width.
#[derive(Clone, Debug, PartialEq)]
pub struct Steep {
    pub slot: u16,
    pub variant: u16,
    pub material: u32,
    pub tile: f32,
    /// The kinds and groups that draw it, as `kind group`.
    pub from: Vec<(u32, usize)>,
}

/// By AC2 kind and material group, the slot and variant a vertex takes.
pub type ByKind = BTreeMap<(u8, u8), (u16, u16)>;

/// The steep variants: by AC2 kind and group, the slot and variant a vertex of that kind in that
/// group takes, for every group that draws another material than the kind's flat ground; and the
/// variants themselves. A slot with no variant left keeps its kind's own (the flat picture).
#[must_use]
pub fn allocate(terrain: &TerrainDesc, surface: &SurfaceDesc) -> (ByKind, Vec<Steep>) {
    let mut by_kind = BTreeMap::new();
    let mut steep: Vec<Steep> = Vec::new();
    for (kind, &(slot, _)) in kinds::kinds().slots.iter().enumerate() {
        let kind = u32::try_from(kind).unwrap_or(u32::MAX);
        let Some(s) = surface_of(terrain, surface, kind) else {
            continue;
        };
        let flat = group_at(s, 0.0)
            .and_then(|g| s.groups[g].base.first())
            .map(|b| b.material.0);
        for (gi, g) in s.groups.iter().enumerate() {
            let Some(base) = g.base.first() else {
                continue;
            };
            if Some(base.material.0) == flat {
                continue;
            }
            let at = match steep
                .iter()
                .position(|v| v.slot == slot && v.material == base.material.0)
            {
                Some(at) => at,
                None => {
                    let taken = |v: u16| {
                        kinds::kinds()
                            .slots
                            .iter()
                            .any(|&(s, kv)| s == slot && kv == v)
                            || steep.iter().any(|x| x.slot == slot && x.variant == v)
                    };
                    let Some(variant) = (0..16u16).rev().find(|&v| !taken(v)) else {
                        continue;
                    };
                    steep.push(Steep {
                        slot,
                        variant,
                        material: base.material.0,
                        tile: base.tile_width,
                        from: Vec::new(),
                    });
                    steep.len() - 1
                }
            };
            steep[at].from.push((kind, gi));
            if let (Ok(k), Ok(gi8)) = (u8::try_from(kind), u8::try_from(gi)) {
                by_kind.insert((k, gi8), (slot, steep[at].variant));
            }
        }
    }
    (by_kind, steep)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's steep ground)
    use super::*;
    use auberean_assets::region::{MaterialGroup, TerrainType, WeightedMaterial};
    use auberean_primitives::DataId;

    fn group(min: f32, max: f32, material: u32) -> MaterialGroup {
        MaterialGroup {
            min_pitch: min,
            max_pitch: max,
            base: vec![WeightedMaterial {
                material: DataId(material),
                frequency: 1.0,
                tile_width: 20.0,
            }],
            base_frequency: 1.0,
            ..MaterialGroup::default()
        }
    }

    fn surface(index: u32, groups: Vec<MaterialGroup>) -> SurfaceType {
        SurfaceType {
            index,
            groups,
            ..SurfaceType::default()
        }
    }

    fn kind(index: u32, surface: u32) -> TerrainType {
        TerrainType {
            index,
            surface,
            ..TerrainType::default()
        }
    }

    #[test]
    fn a_vertex_draws_the_first_group_its_slope_falls_in() {
        // Flat to sin 35 degrees, then to vertical.
        let s = surface(
            7,
            vec![
                group(0.0, 0.573_576_4, 0x2B00_0033),
                group(0.0, 1.0, 0x2B00_0037),
            ],
        );
        assert_eq!(group_at(&s, 0.0), Some(0));
        assert_eq!(group_at(&s, 0.32), Some(0));
        assert_eq!(group_at(&s, 0.34), Some(1));
        assert_eq!(group_at(&s, 1.0), Some(1));
    }

    #[test]
    fn steep_groups_of_another_material_share_a_variant_per_slot_from_the_top() {
        // Kinds 1 (GrassLand, slot 1) and 22 (TownHuman, slot 7) both go to rock; kind 21
        // (ForestFloor, slot 21) too; kind 28's steep group draws its flat material.
        let t = TerrainDesc {
            terrains: vec![kind(1, 100), kind(22, 101), kind(21, 102), kind(28, 103)],
            ..TerrainDesc::default()
        };
        let rock = 0x2B00_0037;
        let s = SurfaceDesc {
            surfaces: vec![
                surface(
                    100,
                    vec![group(0.0, 0.57, 0x2B00_0033), group(0.0, 1.0, rock)],
                ),
                surface(
                    101,
                    vec![group(0.0, 0.57, 0x2B00_0033), group(0.0, 1.0, rock)],
                ),
                surface(
                    102,
                    vec![group(0.0, 0.57, 0x2B00_0045), group(0.0, 1.0, rock)],
                ),
                surface(
                    103,
                    vec![group(0.0, 1.0, 0x2B00_0049), group(0.0, 1.0, 0x2B00_0049)],
                ),
            ],
            ..SurfaceDesc::default()
        };
        let (by_kind, steep) = allocate(&t, &s);
        assert_eq!(by_kind.get(&(1, 1)), Some(&(1, 15)));
        assert_eq!(by_kind.get(&(22, 1)), Some(&(7, 15)));
        assert_eq!(by_kind.get(&(21, 1)), Some(&(21, 15)));
        assert_eq!(by_kind.get(&(28, 1)), None);
        assert_eq!(by_kind.get(&(1, 0)), None);
        assert_eq!(steep.len(), 3);
        assert!(steep.iter().all(|v| v.material == rock && v.variant == 15));
        // Every steep variant is clear of the kinds' own.
        for v in &steep {
            assert!(!kinds::kinds()
                .slots
                .iter()
                .any(|&(s, kv)| s == v.slot && kv == v.variant));
        }
    }
}
