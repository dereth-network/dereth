//! AC2's dungeons as AC1's: every AC2 block's cells that no building takes as its rooms, written
//! as AC1 cells under an AC1 block ([`every`]). Each AC2 cell mesh is an AC1 environment of one
//! cell shape, as AC2's rooms in buildings are ([`rooms`]); each cell keeps its place, its links to
//! its neighbors and the cells it sees.
//!
//! **Where:** a dungeon the list names (`--vaults LIST`) goes where it says (Arwic's vault under
//! its entrance); every other goes under a free AC1 block, one holding no cells and lying wholly
//! beyond AC2's landmass, under the sea as AC1 keeps its own dungeons off the map. Its AC2 block's
//! corner stands [`CORNER`] meters in from the AC1 block's, so its 160 m lie inside the 192, at
//! AC2's heights lowered as the land's are. Whether a portal leads into it is the world's to say.
//!
//! AC2 lights its dungeons with point lights it lists for the block, baked into its meshes' vertices;
//! AC1 lights a cell from the light-bearing statics in it and near it. Each of AC2's point lights
//! becomes a static in the cell AC2 names, a setup of no parts with one light of AC2's color and
//! reach.
//!
//! The list: a dungeon a line, `AC2BLOCK AC1BLOCK X Y Z` (`00DE 6E8F 69.8 243.9 -11.7`): the AC2
//! block's corner stands at `X Y Z` in the AC1 block; `#` starts a comment.

use std::collections::BTreeMap;

use auberean_world::inside::{Light, LightKind};

use crate::ac2_look::source::Ac2;
use auberean_world::records::WorldRecords;
use dereth_assets::world::{CellPortal, ObjectEntry};
use dereth_assets::EnvCell;
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, Frame, Quat, Vec3};

use crate::ac2_world::art::{Ids, Pictures, Records};
use crate::ac2_world::rooms;

/// An AC2 dungeon block and where its cells go.
#[derive(Clone, Debug, PartialEq)]
pub struct Vault {
    pub ac2: (i32, i32),
    pub block: (usize, usize),
    pub at: Vec3,
}

/// The list's dungeons.
///
/// # Errors
/// A line that does not read.
pub fn parse_list(text: &str) -> Result<Vec<Vault>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let bad = || format!("vaults line {}: `{line}`", n + 1);
        if f.len() != 5 {
            return Err(bad());
        }
        let hex = |s: &str| u16::from_str_radix(s, 16).map_err(|_| bad());
        let num = |s: &str| s.parse::<f32>().map_err(|_| bad());
        let (a, b) = (hex(f[0])?, hex(f[1])?);
        out.push(Vault {
            ac2: (i32::from(a >> 8), i32::from(a & 0xFF)),
            block: (usize::from(b >> 8), usize::from(b & 0xFF)),
            at: Vec3::new(num(f[2])?, num(f[3])?, num(f[4])?),
        });
    }
    Ok(out)
}

/// How far in from a free AC1 block's corner an AC2 dungeon block's corner stands, meters.
pub const CORNER: f32 = 16.0;

/// How many cells AC2 block `ac2`'s cell count names that its file doesn't hold.
#[must_use]
pub fn missing(wr: &WorldRecords, ac2: (i32, i32)) -> usize {
    wr.landblock_info(ac2.0, ac2.1)
        .map_or(0, |info| info.cell_count as usize - lows(wr, ac2).len())
}

/// The AC2 cells of AC2 block `ac2` the world holds, by number.
#[must_use]
pub fn lows(wr: &WorldRecords, ac2: (i32, i32)) -> Vec<u16> {
    let Some(info) = wr.landblock_info(ac2.0, ac2.1) else {
        return Vec::new();
    };
    (0..info.cell_count)
        .filter_map(|i| 0x0100u16.checked_add(u16::try_from(i).ok()?))
        .filter(|&low| wr.cell(ac2.0, ac2.1, low).is_some())
        .collect()
}

/// Every AC2 dungeon and where it goes: each AC2 block's cells less those `taken` (a building's
/// rooms), where the list puts it (`listed`) or under the next of the `free` AC1 blocks, its
/// corner [`CORNER`] in and lowered as the land is.
pub fn every(
    wr: &WorldRecords,
    taken: &BTreeMap<(i32, i32), std::collections::BTreeSet<u16>>,
    listed: &[Vault],
    free: &mut dyn Iterator<Item = (usize, usize)>,
) -> Result<Vec<(Vault, Vec<u16>)>, String> {
    let blocks = (0..255i32)
        .flat_map(|x| (0..255i32).map(move |y| (x, y)))
        .map(|b| (b, lows(wr, b)));
    place(blocks, taken, listed, free)
}

/// Where each AC2 block's cells (`blocks`, in order) go as a dungeon: those `taken` left out, a
/// block with none left skipped, one the list names where it says, the rest each under the next of
/// the `free` AC1 blocks.
fn place(
    blocks: impl Iterator<Item = ((i32, i32), Vec<u16>)>,
    taken: &BTreeMap<(i32, i32), std::collections::BTreeSet<u16>>,
    listed: &[Vault],
    free: &mut dyn Iterator<Item = (usize, usize)>,
) -> Result<Vec<(Vault, Vec<u16>)>, String> {
    let mut out = Vec::new();
    for (b, lows) in blocks {
        let cells: Vec<u16> = lows
            .into_iter()
            .filter(|l| !taken.get(&b).is_some_and(|t| t.contains(l)))
            .collect();
        if cells.is_empty() {
            continue;
        }
        let v = match listed.iter().find(|v| v.ac2 == b) {
            Some(v) => v.clone(),
            None => Vault {
                ac2: b,
                block: free
                    .next()
                    .ok_or("no free AC1 block left for AC2's dungeons")?,
                at: Vec3::new(CORNER, CORNER, -crate::ac2_world::table::SEA),
            },
        };
        out.push((v, cells));
    }
    Ok(out)
}

/// AC2's lights as statics, by the AC2 cell they stand in.
pub type Lit = BTreeMap<u16, Vec<ObjectEntry>>;

/// AC2's lights standing in the cells `numbers` holds (AC2's number to the cell's), as statics by
/// the AC2 cell they stand in, each at `place` of its AC2 block point; and how many were placed and
/// left out (directional, or in no cell made).
///
/// # Errors
/// A record that will not encode.
pub fn lights_in(
    wr: &WorldRecords,
    ac2: (i32, i32),
    numbers: &BTreeMap<u16, u16>,
    place: &dyn Fn(Vec3) -> Vec3,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<(Lit, usize, usize), String> {
    let mut lit: BTreeMap<u16, Vec<ObjectEntry>> = BTreeMap::new();
    let mut setups: BTreeMap<(u32, u32), DataId> = BTreeMap::new();
    let (mut lights, mut lights_left) = (0, 0);
    if let Some(li) = wr.light_info(ac2.0, ac2.1) {
        for l in &li.lights {
            let o = Vec3::new(l.origin.x, l.origin.y, l.origin.z);
            let (light, _, _) = Light::of_properties(
                l.cell.0,
                auberean_primitives::Vec3::new(o.x, o.y, o.z),
                &l.properties,
            );
            let low = u16::try_from(l.cell.0 & 0xFFFF).unwrap_or(0);
            if light.kind == LightKind::Directional || !numbers.contains_key(&low) {
                lights_left += 1;
                continue;
            }
            let color = argb(&light);
            let reach = light.most_range.max(light.range);
            let setup = match setups.get(&(color, reach.to_bits())) {
                Some(&s) => s,
                None => {
                    let s = light_setup(color, reach, ids, out)?;
                    setups.insert((color, reach.to_bits()), s);
                    s
                }
            };
            lit.entry(low).or_default().push(ObjectEntry {
                id: setup,
                frame: Frame::new(place(o), Quat::IDENTITY),
            });
            lights += 1;
        }
    }
    Ok((lit, lights, lights_left))
}

/// What the dungeons made share: each AC2 cell mesh's environment and each material's surface,
/// made once for all of them (AC2's dungeons are built of the same kits).
#[derive(Default)]
pub struct Shared {
    environments: BTreeMap<u32, Option<rooms::Room>>,
    surfaces: BTreeMap<dereth_ac2_convert::surface::MadeKey, dereth_ac2_convert::surface::Made>,
}

impl Shared {
    /// The environments of every cell mesh `dungeons` name that none is made for yet, made at
    /// once: each made in turn but for its cells' trees, the trees built side by side, and each
    /// stored in turn, so they take the ids they would made one by one.
    ///
    /// # Errors
    /// A picture that will not read, or a shape the tree builder can't cut.
    pub fn make_environments(
        &mut self,
        dungeons: &[(Vault, Vec<u16>)],
        wr: &WorldRecords,
        ac2: &Ac2,
        pictures: &mut Pictures,
        ids: &mut Ids,
        out: &mut Records,
    ) -> Result<(), String> {
        let mut meshes: Vec<u32> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (v, lows) in dungeons {
            for &low in lows {
                if let Some(c) = wr.cell(v.ac2.0, v.ac2.1, low) {
                    if !self.environments.contains_key(&c.mesh.0) && seen.insert(c.mesh.0) {
                        meshes.push(c.mesh.0);
                    }
                }
            }
        }
        let mut pending = Vec::with_capacity(meshes.len());
        for &mesh in &meshes {
            pending.push(std::sync::Mutex::new(dereth_ac2_convert::cell::pending(
                wr,
                ac2,
                mesh,
                pictures,
                ids,
                out,
                &mut self.surfaces,
            )?));
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let built: Vec<std::sync::Mutex<Option<Result<dereth_ac2_convert::cell::Built, String>>>> =
            (0..pending.len())
                .map(|_| std::sync::Mutex::new(None))
                .collect();
        let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(slot) = pending.get(k) else { break };
                    if let Some(p) = slot.lock().expect("a pending environment").take() {
                        *built[k].lock().expect("a built environment") = Some(p.build());
                    }
                });
            }
        });
        for (mesh, b) in meshes.into_iter().zip(built) {
            let room = match b.into_inner().expect("a built environment") {
                Some(b) => Some(b?.store(ids, out)),
                None => None,
            };
            self.environments.insert(mesh, room);
        }
        Ok(())
    }
}

/// A dungeon made: its cells, numbered from 0 within it (renumbered when added to the block,
/// [`rooms::add_cells`]), and each AC2 cell's number among them.
pub struct Made {
    pub ac2: (i32, i32),
    pub block: (usize, usize),
    pub at: Vec3,
    pub cells: Vec<EnvCell>,
    pub numbers: BTreeMap<u16, u16>,
    /// AC2's portals that lead nowhere in the block, left closed.
    pub closed: usize,
    /// AC2's lights placed, and those left out (directional, or in no cell made).
    pub lights: usize,
    pub lights_left: usize,
    /// The AC2 cells whose mesh makes no cell, left out.
    pub cells_left: usize,
}

/// A cell flag: static objects follow.
pub(crate) const WITH_STATICS: u32 = 2;
use dereth_ac2_convert::light::{argb, light_setup};

/// The dungeon `v` names, its cells `lows` (AC2's numbers) made.
///
/// # Errors
/// A record that will not encode.
#[allow(clippy::too_many_arguments)]
pub fn make(
    v: &Vault,
    lows: &[u16],
    wr: &WorldRecords,
    ac2: &Ac2,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
    shared: &mut Shared,
) -> Result<Made, String> {
    let Shared {
        environments,
        surfaces: made_statics,
    } = shared;
    let mut kept = Vec::new();
    for &low in lows {
        let c = wr
            .cell(v.ac2.0, v.ac2.1, low)
            .ok_or("a cell went missing")?;
        if let std::collections::btree_map::Entry::Vacant(slot) = environments.entry(c.mesh.0) {
            slot.insert(rooms::environment(
                wr,
                ac2,
                c.mesh.0,
                pictures,
                ids,
                out,
                made_statics,
            )?);
        }
        if environments[&c.mesh.0].is_some() {
            kept.push(low);
        }
    }
    let numbers: BTreeMap<u16, u16> = kept
        .iter()
        .enumerate()
        .map(|(k, &low)| (low, u16::try_from(k).unwrap_or(u16::MAX)))
        .collect();
    // AC2's lights, by the cell they stand in.
    let (mut lit, lights, lights_left) =
        lights_in(wr, v.ac2, &numbers, &|o| v.at.add(o), ids, out)?;
    let mut cells = Vec::with_capacity(kept.len());
    let mut closed = 0;
    for &low in &kept {
        let c = wr
            .cell(v.ac2.0, v.ac2.1, low)
            .ok_or("a cell went missing")?;
        let Some((env, surfaces, portal_polygons, _)) = environments[&c.mesh.0].clone() else {
            continue;
        };
        let mut portals = Vec::with_capacity(c.portals.len());
        for p in &c.portals {
            let Some(&polygon_id) = portal_polygons.get(usize::from(p.portal)) else {
                closed += 1;
                continue;
            };
            let Some(&other) = numbers.get(&p.other_cell) else {
                closed += 1;
                continue;
            };
            portals.push(CellPortal {
                flags: u16::from(p.flags & 1 != 0),
                polygon_id,
                other_cell_id: u32::from(other),
                other_portal_id: i16::try_from(p.other_portal).unwrap_or(-1),
            });
        }
        let r = c.frame.rotation;
        let o = c.frame.origin;
        let k = numbers[&low];
        let statics = lit.remove(&low).unwrap_or_default();
        cells.push(EnvCell {
            id: DataId(u32::from(k)),
            flags: if statics.is_empty() { 0 } else { WITH_STATICS },
            cell_id_repeat: u32::from(k),
            surfaces,
            environment: env,
            cell_struct: 0,
            frame: Frame::new(
                v.at.add(Vec3::new(o.x, o.y, o.z)),
                Quat {
                    w: r.w,
                    x: r.x,
                    y: r.y,
                    z: r.z,
                },
            ),
            portals,
            visible_cells: c
                .visible
                .iter()
                .filter_map(|w| numbers.get(w).copied())
                .collect(),
            static_objects: statics,
            restriction_obj: None,
        });
    }
    Ok(Made {
        cells_left: lows.len() - kept.len(),
        ac2: v.ac2,
        block: v.block,
        at: v.at,
        cells,
        numbers,
        closed,
        lights,
        lights_left,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's dungeons)
    use super::*;

    #[test]
    fn a_vault_line_names_both_blocks_and_where_the_corner_goes() {
        let v = parse_list("# Arwic\n00DE 6E8F 69.8 243.9 -11.7\n").unwrap();
        assert_eq!(
            v,
            [Vault {
                ac2: (0x00, 0xDE),
                block: (0x6E, 0x8F),
                at: Vec3::new(69.8, 243.9, -11.7),
            }]
        );
        assert!(parse_list("00DE 6E8F 1 2").is_err());
    }

    /// Every AC2 block's cells no building takes are a dungeon: one the list names goes where it
    /// says, every other under the next free AC1 block, its corner in and lowered as the land is,
    /// and a block whose cells a building takes every one of is none.
    #[test]
    fn every_ac2_dungeon_goes_where_the_list_says_or_under_the_next_free_block() {
        let arwic = Vault {
            ac2: (0x00, 0xDE),
            block: (0x6E, 0x8F),
            at: Vec3::new(69.8, 135.0, -16.7),
        };
        let blocks = vec![
            ((0x00, 0xDE), vec![0x0100, 0x0101]),
            ((0x06, 0xF2), vec![0x0100, 0x0101, 0x0102]),
            ((0x5C, 0x8D), vec![0x0100, 0x0101]),
            ((0x07, 0x00), vec![0x0100]),
        ];
        let taken = BTreeMap::from([
            ((0x06, 0xF2), std::collections::BTreeSet::from([0x0101])),
            (
                (0x5C, 0x8D),
                std::collections::BTreeSet::from([0x0100, 0x0101]),
            ),
        ]);
        let mut free = [(0xFE, 0x00), (0xFE, 0x01)].into_iter();
        let placed = place(
            blocks.into_iter(),
            &taken,
            std::slice::from_ref(&arwic),
            &mut free,
        )
        .unwrap();
        let lowered = Vec3::new(CORNER, CORNER, -crate::ac2_world::table::SEA);
        assert_eq!(
            placed,
            [
                (arwic, vec![0x0100, 0x0101]),
                (
                    Vault {
                        ac2: (0x06, 0xF2),
                        block: (0xFE, 0x00),
                        at: lowered
                    },
                    vec![0x0100, 0x0102]
                ),
                (
                    Vault {
                        ac2: (0x07, 0x00),
                        block: (0xFE, 0x01),
                        at: lowered
                    },
                    vec![0x0100]
                ),
            ]
        );
        // No free block left for a dungeon is refused.
        let mut none = std::iter::empty();
        assert!(place(
            vec![((0x07, 0x00), vec![0x0100])].into_iter(),
            &BTreeMap::new(),
            &[],
            &mut none
        )
        .is_err());
    }
}
