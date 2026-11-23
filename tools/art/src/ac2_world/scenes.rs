//! The converted world's scenery written as AC1 scenery data: AC1's own generator (the client's
//! and Empyrean's) grows it, from scene records and scene types the converter writes.
//!
//! AC1 grows each land cell corner's scene from its terrain word: the terrain type (bits 2-6)
//! names 32 scene types, the scene index (bits 11-15) picks one, and a hash of the corner picks one
//! of that type's scenes, each a list of objects placed about the corner. The converted world's
//! region keeps AC1's scene types and appends its own, one for each of AC2's cover kinds the land
//! uses ([`Key`]); each terrain type's scene types become the cover kinds found on it
//! ([`Slots`]), and each word's scene index names its own kind's ([`Slots::index`]). The last
//! index of every type names a scene type with no scenes ([`crate::ac2_world::clearing::NO_SCENE`]).

use std::collections::BTreeMap;

use dereth_assets::region::{SceneDesc, TerrainType};
use dereth_assets::world::ObjectDesc;
use dereth_primitives::DataId;

use crate::ac2_world::clearing::NO_SCENE;

/// One of AC2's cover kinds: AC2's scenery kind a vertex grows, and whether the vertex is under
/// AC2's water (AC2's scenes grow other things there).
pub type Key = (u32, bool);

/// An AC1 scene record (`0x12`): its id, then a counted list of objects, each its object's id, its
/// frame (origin, then the rotation's w, x, y, z), frequency, displacements, scales, turn, slopes,
/// alignment, orientation and weenie.
#[must_use]
pub fn encode_scene(id: DataId, objects: &[ObjectDesc]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + objects.len() * 76);
    let mut u = |v: u32| b.extend_from_slice(&v.to_le_bytes());
    u(id.raw());
    u(u32::try_from(objects.len()).unwrap_or(0));
    for o in objects {
        let f = |v: f32| v.to_bits();
        let r = o.base_loc.rotation;
        for v in [
            o.obj_id.raw(),
            f(o.base_loc.origin.x),
            f(o.base_loc.origin.y),
            f(o.base_loc.origin.z),
            f(r.w),
            f(r.x),
            f(r.y),
            f(r.z),
            f(o.freq),
            f(o.displace_x),
            f(o.displace_y),
            f(o.min_scale),
            f(o.max_scale),
            f(o.max_rot),
            f(o.min_slope),
            f(o.max_slope),
        ] {
            u(v);
        }
        for v in [o.align, o.orient, o.weenie_obj] {
            u(u32::from_le_bytes(v.to_le_bytes()));
        }
    }
    b
}

/// The region's scene types as the record holds them: a count, then each its sound table index and
/// its counted scene ids.
#[must_use]
pub fn scene_info_bytes(types: &[SceneDesc]) -> Vec<u8> {
    let mut b = Vec::new();
    let mut u = |v: u32| b.extend_from_slice(&v.to_le_bytes());
    u(u32::try_from(types.len()).unwrap_or(0));
    for t in types {
        u(u32::from_le_bytes(t.stb_index.to_le_bytes()));
        u(u32::try_from(t.scenes.len()).unwrap_or(0));
        for s in &t.scenes {
            u(s.raw());
        }
    }
    b
}

/// The region record with its scene types `old` replaced by `new`, and each terrain type's 32
/// scene types set to `types` (one list for each of the record's terrain types, in its order). The
/// scene types are found by their bytes (once); the terrain types follow them, and only their
/// scene type words change.
///
/// # Errors
/// When `old` is not found exactly once, the terrain types after it are not the record's
/// (`ac1_types`), or a list is not 32 long.
pub fn splice(
    record: &[u8],
    old: &[SceneDesc],
    new: &[SceneDesc],
    ac1_types: &[TerrainType],
    types: &[Vec<i32>],
) -> Result<Vec<u8>, String> {
    let want = scene_info_bytes(old);
    let hits: Vec<usize> = record
        .windows(want.len())
        .enumerate()
        .filter(|(_, w)| *w == want.as_slice())
        .map(|(i, _)| i)
        .collect();
    let [at] = hits.as_slice() else {
        return Err(format!(
            "the region's scene types are found {} times, not once",
            hits.len()
        ));
    };
    if types.len() != ac1_types.len() {
        return Err(format!(
            "{} scene type lists for {} terrain types",
            types.len(),
            ac1_types.len()
        ));
    }
    let mut out = record[..*at].to_vec();
    out.extend(scene_info_bytes(new));
    let tail = out.len();
    out.extend_from_slice(&record[at + want.len()..]);
    // The terrain types: a count, then each a name (a 16-bit length, its bytes, padding to four),
    // a color, and its counted scene types.
    let mut p = tail;
    let word = |out: &[u8], p: usize| -> Result<u32, String> {
        out.get(p..p + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| "the region ends inside its terrain types".to_owned())
    };
    let n = word(&out, p)? as usize;
    if n != ac1_types.len() {
        return Err(format!(
            "{n} terrain types after the scene types, not {}",
            ac1_types.len()
        ));
    }
    p += 4;
    for (k, (t, list)) in ac1_types.iter().zip(types).enumerate() {
        let len = out
            .get(p..p + 2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
            .ok_or("the region ends inside a terrain type's name")?;
        if len == 0xFFFF {
            return Err("a terrain type's name is too long".into());
        }
        p += 2 + len;
        p += (4 - p % 4) % 4;
        p += 4;
        let m = word(&out, p)? as usize;
        if m != t.scene_types.len() || m != list.len() {
            return Err(format!(
                "terrain type {k} has {m} scene types; {} expected, {} given",
                t.scene_types.len(),
                list.len()
            ));
        }
        p += 4;
        for (i, (&was, &v)) in t.scene_types.iter().zip(list).enumerate() {
            let o = p + 4 * i;
            if word(&out, o)? != u32::from_le_bytes(was.to_le_bytes()) {
                return Err(format!(
                    "terrain type {k}'s scene types are not where its name ends"
                ));
            }
            out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        p += 4 * m;
    }
    Ok(out)
}

/// The cover kinds each AC1 terrain type holds, at most [`NO_SCENE`] of them, the commonest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slots {
    pub keys: BTreeMap<u16, Vec<Key>>,
    /// Per terrain type, the corners whose kind was not among its kept ones, taken by its commonest
    /// kept kind of the same wetness.
    pub folded: BTreeMap<u16, usize>,
}

impl Slots {
    /// From `counts`: per terrain type, how many corners hold each cover kind.
    #[must_use]
    pub fn from_counts(counts: &BTreeMap<u16, BTreeMap<Key, usize>>) -> Self {
        let mut s = Self::default();
        for (&slot, by_key) in counts {
            let mut v: Vec<(Key, usize)> = by_key.iter().map(|(k, n)| (*k, *n)).collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let kept: Vec<Key> = v.iter().take(usize::from(NO_SCENE)).map(|p| p.0).collect();
            let folded: usize = v.iter().skip(usize::from(NO_SCENE)).map(|p| p.1).sum();
            if folded > 0 {
                s.folded.insert(slot, folded);
            }
            s.keys.insert(slot, kept);
        }
        s
    }

    /// The scene index of a corner of terrain type `slot` holding `key`: its kind's place among the
    /// type's kept ones, else the commonest kept kind of the same wetness (else the commonest);
    /// [`NO_SCENE`] for a corner growing nothing.
    #[must_use]
    pub fn index(&self, slot: u16, key: Option<Key>) -> u16 {
        let (Some(key), Some(list)) = (key, self.keys.get(&slot)) else {
            return NO_SCENE;
        };
        let at = list
            .iter()
            .position(|k| *k == key)
            .or_else(|| list.iter().position(|k| k.1 == key.1))
            .or(if list.is_empty() { None } else { Some(0) });
        at.and_then(|i| u16::try_from(i).ok()).unwrap_or(NO_SCENE)
    }

    /// Every kept kind, once, in order.
    #[must_use]
    pub fn all_keys(&self) -> Vec<Key> {
        let mut v: Vec<Key> = self.keys.values().flatten().copied().collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Each terrain type's 32 scene types: its kept kinds' scene types (`desc_of`), then `empty`.
    /// A type holding no kind keeps none of AC1's: all 32 are `empty`.
    #[must_use]
    pub fn lists(&self, slots: usize, desc_of: &dyn Fn(Key) -> i32, empty: i32) -> Vec<Vec<i32>> {
        (0..slots)
            .map(|slot| {
                let mut v = vec![empty; usize::from(NO_SCENE) + 1];
                if let Some(list) = u16::try_from(slot).ok().and_then(|s| self.keys.get(&s)) {
                    for (i, k) in list.iter().enumerate() {
                        v[i] = desc_of(*k);
                    }
                }
                v
            })
            .collect()
    }
}

/// A 32-bit position hash for picking sample places in an order that does not follow the land.
#[must_use]
pub const fn spot_hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// Of `candidates` (each a hash and a place, meters), the first `n` by hash that stand at least
/// `apart` meters from every one taken before.
#[must_use]
pub fn pick_apart(candidates: &[(u32, f64, f64)], n: usize, apart: f64) -> Vec<(f64, f64)> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (_, x, y) in sorted {
        if out.len() == n {
            break;
        }
        if out
            .iter()
            .all(|&(a, b)| (a - x).abs() >= apart || (b - y).abs() >= apart)
        {
            out.push((x, y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's cover scenes)
    use super::*;
    use dereth_assets::Decode as _;
    use dereth_primitives::{Frame, Quat, Vec3};

    fn object(id: u32, x: f32) -> ObjectDesc {
        ObjectDesc {
            obj_id: DataId(id),
            base_loc: Frame::new(Vec3::new(x, -3.5, 0.0), Quat::new(0.8, 0.0, 0.0, 0.6)),
            freq: 1.0,
            displace_x: 0.0,
            displace_y: 0.0,
            min_scale: 1.25,
            max_scale: 1.25,
            max_rot: 0.0,
            min_slope: 0.5,
            max_slope: 90.0,
            align: 0,
            orient: 0,
            weenie_obj: 0,
        }
    }

    #[test]
    fn a_written_scene_decodes_as_the_scene_it_was_made_from() {
        let objects = vec![object(0x0200_C001, 4.0), object(0x0200_C002, -11.0)];
        let bytes = encode_scene(DataId(0x1200_C000), &objects);
        let back =
            dereth_assets::world::Scene::decode_payload(DataId(0x1200_C000), &bytes).unwrap();
        assert_eq!(back.id, DataId(0x1200_C000));
        assert_eq!(back.objects, objects);
    }

    fn region(types: &[SceneDesc], names: &[&str], lists: &[Vec<i32>]) -> Vec<u8> {
        let mut r = vec![0xAA; 12];
        r.extend(scene_info_bytes(types));
        r.extend_from_slice(&u32::try_from(names.len()).unwrap().to_le_bytes());
        for (name, list) in names.iter().zip(lists) {
            r.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
            r.extend_from_slice(name.as_bytes());
            while !r.len().is_multiple_of(4) {
                r.push(0);
            }
            r.extend_from_slice(&0x00FF_8800u32.to_le_bytes());
            r.extend_from_slice(&u32::try_from(list.len()).unwrap().to_le_bytes());
            for v in list {
                r.extend_from_slice(&v.to_le_bytes());
            }
        }
        r.extend_from_slice(&[0xBB; 8]);
        r
    }

    fn desc(stb: i32, ids: &[u32]) -> SceneDesc {
        SceneDesc {
            stb_index: stb,
            scenes: ids.iter().map(|&i| DataId(i)).collect(),
        }
    }

    #[test]
    fn splicing_appends_scene_types_and_sets_every_terrain_types_list_alone() {
        let old = vec![desc(0, &[0x1200_0001]), desc(1, &[])];
        // Two terrain types with the same lists, named so their padding differs.
        let a: Vec<i32> = (0..32).map(|i| i % 2).collect();
        let record = region(&old, &["Grassland", "Dirt"], &[a.clone(), a.clone()]);
        let ac1_types: Vec<TerrainType> = ["Grassland", "Dirt"]
            .iter()
            .map(|n| TerrainType {
                terrain_name: (*n).to_owned(),
                terrain_color: 0x00FF_8800,
                scene_types: a.clone(),
            })
            .collect();
        let mut new = old.clone();
        new.push(desc(0, &[0x1200_C000, 0x1200_C001]));
        let lists = vec![vec![2; 32], vec![1; 32]];
        let out = splice(&record, &old, &new, &ac1_types, &lists).unwrap();
        assert_eq!(out, region(&new, &["Grassland", "Dirt"], &lists));
        // Not found, or the wrong number of lists: refused.
        assert!(splice(&record, &new, &new, &ac1_types, &lists).is_err());
        assert!(splice(&record, &old, &new, &ac1_types, &lists[..1]).is_err());
    }

    #[test]
    fn each_terrain_type_keeps_its_commonest_kinds_and_folds_the_rest_by_wetness() {
        let mut counts: BTreeMap<u16, BTreeMap<Key, usize>> = BTreeMap::new();
        let land = counts.entry(3).or_default();
        for k in 0..40u32 {
            land.insert((k, false), 100 + k as usize);
        }
        land.insert((99, true), 1);
        let s = Slots::from_counts(&counts);
        let kept = &s.keys[&3];
        assert_eq!(kept.len(), usize::from(NO_SCENE));
        assert_eq!(kept[0], (39, false));
        // 0 to 8 and the wet kind fold: 9 dry (100..=108) and 1 wet.
        assert_eq!(s.folded[&3], (100..=108).sum::<usize>() + 1);
        assert_eq!(s.index(3, Some((39, false))), 0);
        assert_eq!(s.index(3, Some((0, false))), 0);
        // No kept wet kind: the commonest.
        assert_eq!(s.index(3, Some((99, true))), 0);
        assert_eq!(s.index(3, None), NO_SCENE);
        assert_eq!(s.index(4, Some((1, false))), NO_SCENE);
        let lists = s.lists(5, &|k| 1000 + i32::try_from(k.0).unwrap(), -1);
        assert_eq!(lists[3][0], 1039);
        assert_eq!(lists[3][usize::from(NO_SCENE)], -1);
        assert!(lists[4].iter().all(|&v| v == -1));
    }

    #[test]
    fn sample_places_are_taken_by_hash_and_kept_apart() {
        let c = vec![
            (5, 0.0, 0.0),
            (1, 10.0, 10.0),
            (3, 40.0, 0.0),
            (2, 100.0, 100.0),
            (4, 30.0, 20.0),
        ];
        assert_eq!(
            pick_apart(&c, 4, 24.0),
            vec![(10.0, 10.0), (100.0, 100.0), (40.0, 0.0)]
        );
        assert_eq!(pick_apart(&c, 1, 24.0), vec![(10.0, 10.0)]);
        assert_ne!(spot_hash(1, 2), spot_hash(2, 1));
    }
}
