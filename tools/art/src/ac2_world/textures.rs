//! AC2's ground pictures as AC1's terrain textures: which AC2 kind draws each AC1 terrain slot and
//! each of its variants, how often its picture repeats, and the region record with each slot's
//! texture set in place and an entry added for each variant.
//!
//! A terrain word's variant (bits 7-10) keeps the AC2 kinds sharing a slot apart. The slot's own
//! entry draws its variant 0; each other variant on the land has an entry of its own, its terrain
//! type the slot with the variant above bit 8 ([`variant_type`]), which a client that reads
//! variants draws it by. A client that does not draws every variant as its slot.
//!
//! AC1 draws a land cell's ground from its corners' slot textures, each repeated `tex_tiling` times
//! across the 24 m cell; on the dense grid each 8 m square draws its own part of the cell's
//! pictures, so they repeat across the cell there too. The converted portal is ours, so each
//! slot's texture can be AC2's own picture, written as an AC1 image texture.

use std::collections::BTreeMap;

use dereth_assets::region::TerrainDesc;

use crate::ac2_world::kinds;

/// The slot the region's terrain textures give the road (`terrain_type` 32).
pub const ROAD_TYPE: u32 = 32;

/// The AC2 kind a slot and variant were made from: the inverse of [`kinds::kinds`].
#[must_use]
pub fn kind_of(slot: u16, variant: u16) -> Option<u32> {
    kinds::kinds()
        .slots
        .iter()
        .position(|&(s, v)| s == slot && v == variant)
        .and_then(|k| u32::try_from(k).ok())
}

/// The terrain type a region names AC2's road `road` (1 to 3, as the terrain word's road bits keep
/// it) by: the road's own for road 1, and the road's with `road - 1` above bit 8 for the others,
/// which a client that reads them draws by; one that does not draws every road as road 1.
#[must_use]
pub const fn road_type(road: u32) -> u32 {
    if road <= 1 {
        ROAD_TYPE
    } else {
        ROAD_TYPE | ((road - 1) << 8)
    }
}

/// The terrain type a region names variant `variant` of slot `slot` by.
#[must_use]
pub const fn variant_type(slot: u16, variant: u16) -> u32 {
    (slot as u32 & 0x1F) | ((variant as u32 & 0xF) << 8)
}

/// Each land slot's own kind and the variant it is: variant 0 where the land has it, else the
/// variant holding most of the slot's vertices (`counts` by slot and variant).
#[must_use]
pub fn slot_kinds(counts: &BTreeMap<(u16, u16), usize>) -> BTreeMap<u16, (u16, u32)> {
    let mut out = BTreeMap::new();
    for (slot, kind) in majority_kinds(counts) {
        let variant = kinds::land_slot(kind).1;
        out.insert(slot, (variant, kind));
    }
    for (&(slot, variant), &n) in counts {
        if variant == 0 && n > 0 {
            if let Some(kind) = kind_of(slot, 0) {
                out.insert(slot, (0, kind));
            }
        }
    }
    out
}

/// The variants on the land that are not their slot's own kind (`own`, from [`slot_kinds`]):
/// each slot, variant and kind, which takes an entry of its own.
#[must_use]
pub fn variant_kinds(
    counts: &BTreeMap<(u16, u16), usize>,
    own: &BTreeMap<u16, (u16, u32)>,
) -> Vec<(u16, u16, u32)> {
    counts
        .iter()
        .filter(|&(&(slot, variant), &n)| {
            n > 0 && own.get(&slot).is_some_and(|&(v, _)| v != variant)
        })
        .filter_map(|(&(slot, variant), _)| kind_of(slot, variant).map(|k| (slot, variant, k)))
        .collect()
}

/// Each land slot's kind: the variant holding the most of the slot's vertices (`counts` by slot
/// and variant), the lower variant on a tie.
#[must_use]
pub fn majority_kinds(counts: &BTreeMap<(u16, u16), usize>) -> BTreeMap<u16, u32> {
    let mut best: BTreeMap<u16, (usize, u16)> = BTreeMap::new();
    for (&(slot, variant), &n) in counts {
        if n == 0 || kind_of(slot, variant).is_none() {
            continue;
        }
        let e = best.entry(slot).or_insert((n, variant));
        if n > e.0 || (n == e.0 && variant < e.1) {
            *e = (n, variant);
        }
    }
    best.into_iter()
        .filter_map(|(slot, (_, v))| kind_of(slot, v).map(|k| (slot, k)))
        .collect()
}

/// How many times a picture covering `tile` meters repeats across a cell `cell` meters a side: the
/// nearest whole number, at least once.
#[must_use]
pub fn tiling(tile: f32, cell: f64) -> u32 {
    let tile = if tile > 0.0 { f64::from(tile) } else { 4.0 };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (cell / tile).round() as u32;
    n.max(1)
}

/// A terrain texture entry as the record holds it: 44 bytes.
fn desc_bytes(d: &TerrainDesc) -> Vec<u8> {
    [
        d.terrain_type,
        d.tex_gid.raw(),
        d.tex_tiling,
        d.max_vert_bright,
        d.min_vert_bright,
        d.max_vert_saturate,
        d.min_vert_saturate,
        d.max_vert_hue,
        d.min_vert_hue,
        d.detail_tex_tiling,
        d.detail_tex_gid.raw(),
    ]
    .iter()
    .flat_map(|w| w.to_le_bytes())
    .collect()
}

/// The region record with `extra` added after its terrain textures (`descs`, the record's own,
/// decoded), and their count raised to match. The entries must lie in the record, after their
/// count, exactly once.
///
/// # Errors
/// The entries not found exactly once, or not after their count.
pub fn append_terrain_textures(
    record: &[u8],
    descs: &[TerrainDesc],
    extra: &[TerrainDesc],
) -> Result<Vec<u8>, String> {
    if extra.is_empty() {
        return Ok(record.to_vec());
    }
    let list: Vec<u8> = descs.iter().flat_map(desc_bytes).collect();
    let hits: Vec<usize> = record
        .windows(list.len().max(1))
        .enumerate()
        .filter(|(_, w)| *w == list.as_slice())
        .map(|(i, _)| i)
        .collect();
    let [at] = hits.as_slice() else {
        return Err(format!(
            "the terrain textures found {} times in the region",
            hits.len()
        ));
    };
    let at = *at;
    let n = u32::try_from(descs.len()).map_err(|e| e.to_string())?;
    if at < 4 || record[at - 4..at] != n.to_le_bytes() {
        return Err("the terrain textures do not follow their count".into());
    }
    let total = u32::try_from(descs.len() + extra.len()).map_err(|e| e.to_string())?;
    let end = at + list.len();
    let mut out = Vec::with_capacity(record.len() + 44 * extra.len());
    out.extend_from_slice(&record[..at - 4]);
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&record[at..end]);
    for d in extra {
        out.extend(desc_bytes(d));
    }
    out.extend_from_slice(&record[end..]);
    Ok(out)
}

/// The region record with the terrain textures `set` (by terrain type: texture and tiling) in
/// place: each entry of `descs` (the record's own, decoded) is found by its 44 bytes, which must
/// occur exactly once, and only its texture and tiling words change.
///
/// # Errors
/// A type `set` names that `descs` lacks, or an entry found other than once.
pub fn patch_terrain_textures(
    record: &[u8],
    descs: &[TerrainDesc],
    set: &BTreeMap<u32, (u32, u32)>,
) -> Result<Vec<u8>, String> {
    let mut out = record.to_vec();
    for (&ty, &(tex, tile)) in set {
        let d = descs
            .iter()
            .find(|d| d.terrain_type == ty)
            .ok_or_else(|| format!("the region has no terrain texture for type {ty}"))?;
        let want = desc_bytes(d);
        let hits: Vec<usize> = record
            .windows(want.len())
            .enumerate()
            .filter(|(_, w)| *w == want.as_slice())
            .map(|(i, _)| i)
            .collect();
        let [at] = hits.as_slice() else {
            return Err(format!(
                "terrain texture {ty} found {} times in the region",
                hits.len()
            ));
        };
        out[at + 4..at + 8].copy_from_slice(&tex.to_le_bytes());
        out[at + 8..at + 12].copy_from_slice(&tile.to_le_bytes());
    }
    Ok(out)
}

/// The region record with its road alpha maps (`maps`, the record's own, decoded) drawn by the
/// textures `set` names by code: the maps' list, after its count, must occur in the record exactly
/// once, and only the textures of the codes `set` names change.
///
/// # Errors
/// The list not found exactly once, not after its count, or a code `set` names that it lacks.
pub fn patch_road_maps(
    record: &[u8],
    maps: &[dereth_assets::region::CodeTexture],
    set: &BTreeMap<u32, u32>,
) -> Result<Vec<u8>, String> {
    let n = u32::try_from(maps.len()).map_err(|e| e.to_string())?;
    let list: Vec<u8> = std::iter::once(n)
        .chain(maps.iter().flat_map(|m| [m.code, m.tex_gid.raw()]))
        .flat_map(u32::to_le_bytes)
        .collect();
    let hits: Vec<usize> = record
        .windows(list.len())
        .enumerate()
        .filter(|(_, w)| *w == list.as_slice())
        .map(|(i, _)| i)
        .collect();
    let [at] = hits.as_slice() else {
        return Err(format!(
            "the road maps found {} times in the region",
            hits.len()
        ));
    };
    let mut out = record.to_vec();
    for (&code, &tex) in set {
        let k = maps
            .iter()
            .position(|m| m.code == code)
            .ok_or_else(|| format!("the region has no road map of code {code}"))?;
        let word = at + 4 + 8 * k + 4;
        out[word..word + 4].copy_from_slice(&tex.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's ground pictures)
    use super::*;
    use dereth_primitives::DataId;

    /// The road maps' textures change in place, each by its code, and nothing else does.
    #[test]
    fn patching_the_road_maps_changes_only_their_textures() {
        use dereth_assets::region::CodeTexture;
        let maps = [
            CodeTexture {
                code: 9,
                tex_gid: DataId(0x0500_168E),
            },
            CodeTexture {
                code: 10,
                tex_gid: DataId(0x0500_168C),
            },
            CodeTexture {
                code: 8,
                tex_gid: DataId(0x0500_168D),
            },
        ];
        let mut record = vec![7u8; 10];
        record.extend(3u32.to_le_bytes());
        for m in &maps {
            record.extend(m.code.to_le_bytes());
            record.extend(m.tex_gid.raw().to_le_bytes());
        }
        record.extend([5u8; 6]);
        let set = BTreeMap::from([(10, 0x0500_C001), (8, 0x0500_C002)]);
        let out = patch_road_maps(&record, &maps, &set).unwrap();
        let mut want = record.clone();
        want[10 + 4 + 8 + 4..10 + 4 + 8 + 8].copy_from_slice(&0x0500_C001u32.to_le_bytes());
        want[10 + 4 + 16 + 4..10 + 4 + 16 + 8].copy_from_slice(&0x0500_C002u32.to_le_bytes());
        assert_eq!(out, want);
        // A code the list lacks, and a list found twice, are refused.
        assert!(patch_road_maps(&record, &maps, &BTreeMap::from([(3, 1)])).is_err());
        let twice = [record.clone(), record].concat();
        assert!(patch_road_maps(&twice, &maps, &set).is_err());
    }

    fn desc(ty: u32, tex: u32) -> TerrainDesc {
        TerrainDesc {
            terrain_type: ty,
            tex_gid: DataId(tex),
            tex_tiling: 2,
            max_vert_bright: 100,
            min_vert_bright: 90,
            max_vert_saturate: 90,
            min_vert_saturate: 70,
            max_vert_hue: 150,
            min_vert_hue: 90,
            detail_tex_tiling: 4,
            detail_tex_gid: DataId(0x0500_1786),
        }
    }

    #[test]
    fn every_kind_is_found_again_from_its_slot_and_variant() {
        for (k, &(s, v)) in kinds::kinds().slots.iter().enumerate() {
            assert_eq!(kind_of(s, v), u32::try_from(k).ok());
        }
        assert_eq!(kind_of(16, 0), None);
    }

    #[test]
    fn a_slot_takes_the_kind_with_most_vertices_and_the_lower_variant_on_a_tie() {
        let counts = BTreeMap::from([((10, 0), 5), ((10, 1), 9), ((12, 0), 3), ((12, 1), 3)]);
        let m = majority_kinds(&counts);
        assert_eq!(m[&10], kind_of(10, 1).unwrap());
        assert_eq!(m[&12], kind_of(12, 0).unwrap());
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn a_slot_draws_its_variant_0_and_each_other_variant_on_the_land_takes_an_entry() {
        // Slot 10: variants 0, 1 and 4 on the land, 1 the most; slot 12: 1 and 3, no variant 0.
        let counts = BTreeMap::from([
            ((10, 0), 2),
            ((10, 1), 9),
            ((10, 4), 1),
            ((12, 1), 3),
            ((12, 3), 5),
            ((15, 2), 0),
        ]);
        let own = slot_kinds(&counts);
        assert_eq!(own[&10], (0, kind_of(10, 0).unwrap()));
        assert_eq!(own[&12], (3, kind_of(12, 3).unwrap()));
        assert_eq!(own.len(), 2);
        let others = variant_kinds(&counts, &own);
        assert_eq!(
            others,
            vec![
                (10, 1, kind_of(10, 1).unwrap()),
                (10, 4, kind_of(10, 4).unwrap()),
                (12, 1, kind_of(12, 1).unwrap()),
            ]
        );
        assert_eq!(variant_type(10, 4), 10 | 4 << 8);
        // Clear of the road's type and of every slot.
        assert!((1..16).all(|v| variant_type(0, v) > ROAD_TYPE));
    }

    #[test]
    fn road_1_is_the_roads_own_entry_and_the_others_their_own_clear_of_every_slot() {
        assert_eq!(road_type(1), ROAD_TYPE);
        assert_eq!(road_type(2), ROAD_TYPE | 1 << 8);
        assert_eq!(road_type(3), ROAD_TYPE | 2 << 8);
        for road in 2..=3 {
            assert_eq!(road_type(road) & 0xFF, ROAD_TYPE);
            assert!((0..32).all(|s| (0..16).all(|v| variant_type(s, v) != road_type(road))));
        }
    }

    #[test]
    fn appending_adds_entries_after_the_terrain_textures_and_raises_their_count() {
        let descs = [desc(0, 0x0500_145C), desc(1, 0x0500_1459)];
        let mut record = vec![0xAAu8; 16];
        record.extend_from_slice(&2u32.to_le_bytes());
        for d in &descs {
            record.extend_from_slice(&desc_bytes(d));
        }
        record.extend_from_slice(&[0x55; 8]);
        let extra = [desc(variant_type(1, 2), 0x0500_C002)];
        let out = append_terrain_textures(&record, &descs, &extra).unwrap();
        assert_eq!(out.len(), record.len() + 44);
        assert_eq!(&out[..16], &record[..16]);
        assert_eq!(&out[16..20], &3u32.to_le_bytes());
        assert_eq!(&out[20..20 + 88], &record[20..20 + 88]);
        assert_eq!(&out[108..152], desc_bytes(&extra[0]).as_slice());
        assert_eq!(&out[152..], &record[108..]);
        // Nothing to add changes nothing; entries not after their count are refused.
        assert_eq!(
            append_terrain_textures(&record, &descs, &[]).unwrap(),
            record
        );
        let mut wrong = record.clone();
        wrong[16] = 7;
        assert!(append_terrain_textures(&wrong, &descs, &extra).is_err());
    }

    #[test]
    fn a_picture_repeats_a_whole_number_of_times_and_at_least_once() {
        assert_eq!(tiling(20.0, 8.0), 1);
        assert_eq!(tiling(4.0, 8.0), 2);
        assert_eq!(tiling(2.5, 8.0), 3);
        assert_eq!(tiling(10.0, 24.0), 2);
        assert_eq!(tiling(0.0, 8.0), 2);
    }

    #[test]
    fn patching_changes_only_the_named_entries_texture_and_tiling() {
        let descs = [desc(0, 0x0500_145C), desc(1, 0x0500_1459)];
        let mut record = vec![0xAAu8; 16];
        for d in &descs {
            record.extend_from_slice(&desc_bytes(d));
        }
        record.extend_from_slice(&[0x55; 8]);
        let out = patch_terrain_textures(&record, &descs, &BTreeMap::from([(1, (0x0500_C001, 1))]))
            .unwrap();
        assert_eq!(out.len(), record.len());
        let mut want = desc(1, 0x0500_C001);
        want.tex_tiling = 1;
        assert_eq!(&out[16 + 44..16 + 88], desc_bytes(&want).as_slice());
        assert_eq!(&out[..16 + 44], &record[..16 + 44]);
        assert_eq!(&out[16 + 88..], &record[16 + 88..]);
    }

    #[test]
    fn an_entry_not_found_exactly_once_is_refused() {
        let descs = [desc(0, 0x0500_145C)];
        let record = desc_bytes(&descs[0]).repeat(2);
        assert!(patch_terrain_textures(&record, &descs, &BTreeMap::from([(0, (1, 1))])).is_err());
        assert!(patch_terrain_textures(&record, &descs, &BTreeMap::from([(5, (1, 1))])).is_err());
    }
}
