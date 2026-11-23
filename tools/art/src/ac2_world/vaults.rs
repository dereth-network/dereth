//! AC2's dungeons as AC1's: every cell of an AC2 dungeon block, written as AC1 cells under an AC1
//! block (`--vaults LIST`). Each AC2 cell mesh is an AC1 environment of one cell shape, as AC2's
//! rooms in buildings are ([`rooms`]); each cell keeps its place, its links to its neighbors and
//! the cells it sees.
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

/// A dungeon made: its cells, numbered from 0 within it (renumbered when added to the block,
/// [`rooms::add_cells`]), and each AC2 cell's number among them.
pub struct Made {
    pub block: (usize, usize),
    pub cells: Vec<EnvCell>,
    pub numbers: BTreeMap<u16, u16>,
    /// AC2's portals that lead nowhere in the block, left closed.
    pub closed: usize,
    /// AC2's lights placed, and those left out (directional, or in no cell made).
    pub lights: usize,
    pub lights_left: usize,
}

/// A cell flag: static objects follow.
const WITH_STATICS: u32 = 2;
use dereth_ac2_convert::light::{argb, light_setup};

/// The dungeon `v` names, its cells made.
///
/// # Errors
/// A record that will not encode.
pub fn make(
    v: &Vault,
    wr: &WorldRecords,
    ac2: &Ac2,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Made, String> {
    let mut lows = Vec::new();
    for low in 0x0100..=0xFFFDu16 {
        if wr.cell(v.ac2.0, v.ac2.1, low).is_some() {
            lows.push(low);
        } else if !lows.is_empty() && low > lows[lows.len() - 1] + 0x40 {
            break;
        }
    }
    let mut environments = BTreeMap::new();
    let mut made_statics = BTreeMap::new();
    let mut kept = Vec::new();
    for &low in &lows {
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
                &mut made_statics,
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
    let mut lit: BTreeMap<u16, Vec<ObjectEntry>> = BTreeMap::new();
    let mut setups: BTreeMap<(u32, u32), DataId> = BTreeMap::new();
    let (mut lights, mut lights_left) = (0, 0);
    if let Some(li) = wr.light_info(v.ac2.0, v.ac2.1) {
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
                frame: Frame::new(v.at.add(o), Quat::IDENTITY),
            });
            lights += 1;
        }
    }
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
        block: v.block,
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
}
