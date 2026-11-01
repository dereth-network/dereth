//! The inside of a building or a dungeon seen from one of its cells: that cell, the cells it sees,
//! and what stands in them.

use std::sync::Arc;

use auberean_world::color::Color;
use auberean_world::inside::{
    BlockLights, CellVertex, LightKind, Outside, ParticleLight, SceneLights, DRAW_LIGHTS,
};
use auberean_world::math::Mat4;
use auberean_world::place::{entity_parts, Part};

use super::{placed_box, Counts, Cull, Frame, ObjectBatch, PointLights, SceneBuilder, Spot};
use crate::cull::{Beyond, Doorway, Room, Rooms};
use crate::effects::PlacedLook;
use crate::material::variant_for;
use crate::mesh::MeshVertex;
use crate::volume::{shadow_direction, ShadowLight, ShadowLightKind};
use auberean_primitives::Vec3;

/// A layer switch the device takes to mean the light is in the vertices: the vertex color the
/// interior's lights give, the normal map lit from each vertex's direction toward the light.
pub const LIT_IN_VERTICES: u32 = 0x4000_0000;

/// An object standing in a cell, for its lights: its key (its cell and one more than its place in
/// the cell's list), where it stands, and how far about that its lights reach.
type ObjectReach = ((u32, u32), Vec3, f32);

/// A cell's switch: it is open to the sky.
const CELL_SKY: u32 = 0x10;
/// A cell's switch: it is never culled, but drawn with the cells drawn with it.
const CELL_NO_CULL: u32 = 0x8;
/// A portal's switch: no cell lies beyond it, it leads outdoors.
const PORTAL_OUT: u16 = 0x2;

/// What the cells about the camera look out on: the sky, where one of them is open to it, and the
/// land outside, where a doorway leads out of one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outlook {
    pub sky: bool,
    pub land: bool,
}

impl SceneBuilder<'_> {
    /// The light of block `block`'s interior cells, read once.
    #[must_use]
    pub fn block_lights(&mut self, block: (i32, i32)) -> Option<Arc<BlockLights>> {
        let records = self.records;
        self.block_lights
            .entry(block)
            .or_insert_with(|| BlockLights::of(records, block.0, block.1).map(Arc::new))
            .clone()
    }

    /// How far about cell part `part`'s place the lights that light it as a whole may stand: half
    /// again its box's largest half side.
    fn cell_reach(&mut self, part: &Part) -> Option<f32> {
        let (mesh, _) = self.mesh(part.mesh.0)?;
        let b = &mesh.vertices.bounds;
        Some(
            1.5 * 0.5
                * (b.max.x - b.min.x)
                    .max(b.max.y - b.min.y)
                    .max(b.max.z - b.min.z),
        )
    }

    /// Cell `id`'s mesh vertices lit by its block's interior light before the lights it keeps,
    /// `outside` the day outside: the lights its record lists baked once, the sun on top made
    /// again when the day outside changes; each vertex's lean toward them in the surface's frame,
    /// not yet made a unit (the device adds the kept lights' as they shine at the moment).
    fn lit_cell(
        &mut self,
        id: u32,
        part: &Part,
        lights: &BlockLights,
        outside: &Outside,
    ) -> Option<Arc<Vec<MeshVertex>>> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let q = |c: f32| (c * 1024.0) as u32;
        let (s, a, d) = (outside.sun, outside.ambient, outside.direction);
        let key = [
            q(s.r * outside.brightness),
            q(s.g * outside.brightness),
            q(s.b * outside.brightness),
            q(a.r + a.g + a.b),
            q(d.x + 2.0),
            q(d.y + 2.0),
            q(d.z + 2.0),
        ];
        if let Some((k, v)) = self.lit_cells.get(&id) {
            if *k == key {
                return Some(v.clone());
            }
        }
        let (_, verts) = self.mesh(part.mesh.0)?;
        let v3 = |a: [f32; 3]| Vec3::new(a[0], a[1], a[2]);
        let cell: Vec<CellVertex> = verts
            .iter()
            .map(|v| CellVertex {
                position: v3(v.position),
                normal: v3(v.normal),
                tangent_s: v3(v.tangent_u),
                tangent_t: v3(v.tangent_v),
            })
            .collect();
        // The lights the record lists for the cell are baked once, as the cell is first made
        // ready, at their intensity of that moment; the sun on top of them again with each change
        // of the day outside.
        let bake = match self.cell_bakes.get(&id) {
            Some(b) => b.clone(),
            None => {
                let b = Arc::new(lights.cell_bake(
                    id,
                    &part.transform,
                    &cell,
                    Some((self.uptime, &self.dice)),
                ));
                self.cell_bakes.insert(id, b.clone());
                b
            }
        };
        let light = lights.cell_sunlit(&bake, id, &part.transform, &cell, outside);
        let lit: Vec<MeshVertex> = verts
            .iter()
            .zip(&light)
            .map(|(v, l)| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let ch = |x: f32| (x.clamp(0.0, 1.0) * 255.0) as u32;
                let c = l.color;
                MeshVertex {
                    diffuse: 0xFF00_0000 | ch(c[0]) << 16 | ch(c[1]) << 8 | ch(c[2]),
                    to_light: [l.toward.x, l.toward.y, l.toward.z],
                    ..*v
                }
            })
            .collect();
        let lit = Arc::new(lit);
        self.lit_cells.insert(id, (key, lit.clone()));
        Some(lit)
    }

    /// The lights draw `key` (a cell, or an object standing in one) keeps standing at `at` and
    /// reaching `radius` about it: those it kept the first frame that drew it, else the lights
    /// `scene` keeps for it now (which sorts the frame's lights for the draws after it).
    // D2-03517, D2-10629
    fn kept_lights(
        &mut self,
        key: (u32, u32),
        at: Vec3,
        radius: f32,
        lights: &BlockLights,
        scene: &mut SceneLights,
        outside: &Outside,
    ) -> Vec<usize> {
        if let Some(k) = self.kept_lights.get(&key) {
            return k.clone();
        }
        let kept = scene.keep(lights, at, radius, outside);
        self.kept_lights.insert(key, kept.clone());
        kept
    }

    /// How far about an object made of `parts` the lights that light it may stand: the largest
    /// half side of the box about its parts.
    fn object_reach(&mut self, parts: &[Part]) -> f32 {
        let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for p in parts {
            let Some((mesh, _)) = self.mesh(p.mesh.0) else {
                continue;
            };
            let b = &mesh.vertices.bounds;
            for k in 0..8 {
                let c = Vec3::new(
                    if k & 1 == 0 { b.min.x } else { b.max.x },
                    if k & 2 == 0 { b.min.y } else { b.max.y },
                    if k & 4 == 0 { b.min.z } else { b.max.z },
                );
                let w = p.transform.point(c);
                lo = Vec3::new(lo.x.min(w.x), lo.y.min(w.y), lo.z.min(w.z));
                hi = Vec3::new(hi.x.max(w.x), hi.y.max(w.y), hi.z.max(w.z));
            }
        }
        if lo.x > hi.x {
            return 0.0;
        }
        0.5 * (hi.x - lo.x).max(hi.y - lo.y).max(hi.z - lo.z)
    }

    /// What culls an object standing in cell `cell` made of `parts`, placed by `place`: its box
    /// about every part, twice its size about its middle.
    // D2-03518
    fn object_cull(&mut self, cell: u16, parts: &[Part], place: &Mat4) -> Cull {
        let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for p in parts {
            let Some((mesh, _)) = self.mesh(p.mesh.0) else {
                continue;
            };
            let b = mesh.bounding_box();
            let (a, c) = placed_box(
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
                &p.transform.then(place),
            );
            lo = Vec3::new(lo.x.min(a.x), lo.y.min(a.y), lo.z.min(a.z));
            hi = Vec3::new(hi.x.max(c.x), hi.y.max(c.y), hi.z.max(c.z));
        }
        if lo.x > hi.x {
            return Cull::Never;
        }
        let mid = Vec3::new(
            0.5 * (lo.x + hi.x),
            0.5 * (lo.y + hi.y),
            0.5 * (lo.z + hi.z),
        );
        let twice = |m: f32, e: f32| m + 2.0 * (e - m);
        Cull::InCell {
            cell,
            lo: Vec3::new(twice(mid.x, lo.x), twice(mid.y, lo.y), twice(mid.z, lo.z)),
            hi: Vec3::new(twice(mid.x, hi.x), twice(mid.y, hi.y), twice(mid.z, hi.z)),
        }
    }

    /// The interior cell `cell` (its low word) of block `block` and the cells it sees, in the order
    /// they are drawn.
    fn seen_cells(&self, block: (i32, i32), cell: u16) -> Vec<u16> {
        let Some(home) = self.records.cell(block.0, block.1, cell) else {
            return Vec::new();
        };
        let mut lows = vec![cell];
        for &c in home.visible.iter().chain(&home.shared) {
            if !lows.contains(&c) {
                lows.push(c);
            }
        }
        lows
    }

    /// What the interior cell `cell` of block `block` and the cells it sees look out on.
    #[must_use]
    pub fn outlook(&self, block: (i32, i32), cell: u16) -> Outlook {
        let mut out = Outlook::default();
        for low in self.seen_cells(block, cell) {
            let Some(c) = self.records.cell(block.0, block.1, low) else {
                continue;
            };
            out.sky |= c.flags & CELL_SKY != 0;
            out.land |= c.portals.iter().any(|p| p.flags & PORTAL_OUT != 0);
        }
        out
    }

    /// The frame seen from `spot` (whose block frame everything is placed in) with the camera in
    /// the interior cell `cell` of block `block`: that cell, the cells it sees and what stands in
    /// them, lit by the block's interior light; and what they look out on, under the light
    /// outdoors ([`Frame::outside`]): the sky where one is open to it, and where a doorway leads
    /// out the land as a frame outdoors draws it (its ground, its objects, its water, the sky). The
    /// frame's own light is still the day's, for the caller to make the interior's; the day outside
    /// comes back with it.
    pub fn inside_frame(
        &mut self,
        spot: &Spot,
        fov_y: f32,
        block: (i32, i32),
        cell: u16,
    ) -> (Frame, Outside) {
        #[allow(clippy::cast_sign_loss)]
        let high = ((block.0 as u32) << 24) | ((block.1 as u32) << 16);
        let rooms: Vec<u32> = self
            .seen_cells(block, cell)
            .into_iter()
            .map(|low| high | u32::from(low))
            .collect();
        let outlook = self.outlook(block, cell);
        let mut f = self.frame_without(spot, fov_y, &rooms);
        let outside = Outside::of(&f.light);
        let inside = self.interior(spot, block, cell, &outside);
        let placed = self.interior_placed(block, cell);
        if outlook.land {
            for b in &mut f.objects {
                b.outside = true;
            }
        } else {
            f.terrain.clear();
            f.objects.clear();
            f.water.clear();
            f.placed.clear();
            f.shadow = None;
            f.clouds = None;
            f.reflection = None;
        }
        if !outlook.sky && !outlook.land {
            f.sky_before.clear();
            f.sky_after.clear();
            f.flares.clear();
        } else {
            f.outside = Some(f.light);
        }
        let outdoors = std::mem::take(&mut f.objects);
        f.objects = inside;
        f.objects.extend(outdoors);
        f.placed.extend(placed);
        f.rooms = self.rooms(spot, block, cell).map(Arc::new);
        (f, outside)
    }

    /// The doorways of cell mesh `mesh`: each of its portals' polygons in the mesh's own frame.
    fn doorways(&mut self, mesh: u32) -> Arc<Vec<Vec<Vec3>>> {
        if let Some(d) = self.cell_doorways.get(&mesh) {
            return d.clone();
        }
        let records = self.records;
        let at = self.mesh(mesh).map(|(_, v)| v);
        let shapes: Vec<Vec<Vec3>> = records
            .get::<auberean_assets::mesh::CellMesh>(mesh)
            .zip(at)
            .map(|(c, at)| {
                c.portals
                    .iter()
                    .map(|p| {
                        c.mesh
                            .physics_polygons
                            .get(usize::from(p.polygon))
                            .map(|poly| {
                                poly.vertices
                                    .iter()
                                    .filter_map(|&k| at.get(usize::from(k)))
                                    .map(|v| Vec3::new(v.position[0], v.position[1], v.position[2]))
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let shapes = Arc::new(shapes);
        self.cell_doorways.insert(mesh, shapes.clone());
        shapes
    }

    /// The camera's cell `cell` of block `block` and the cells it sees, as the walk through the
    /// doorways takes them, placed in the frame of `spot`'s block. The camera's cell takes its
    /// block's interior fog distance and keeps it; the others keep the one they last took.
    // D2-03604, D2-03601, D2-03607
    fn rooms(&mut self, spot: &Spot, block: (i32, i32), cell: u16) -> Option<Rooms> {
        let records = self.records;
        let (bx, by) = block;
        let home = records.cell(bx, by, cell)?;
        #[allow(clippy::cast_sign_loss)]
        let high = ((bx as u32) << 24) | ((by as u32) << 16);
        let lights = self.block_lights(block);
        self.fog_cuts.insert(
            high | u32::from(cell),
            lights.as_ref().and_then(|l| l.fog_cut),
        );
        let block_len = self.region.land.grid().block_length();
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let place = Mat4::translation(Vec3::new(
            (bx - spot.block.0) as f32 * block_len,
            (by - spot.block.1) as f32 * block_len,
            0.0,
        ));
        let mut order = vec![cell];
        for &c in &home.visible {
            if !order.contains(&c) {
                order.push(c);
            }
        }
        let mut cells = std::collections::HashMap::new();
        for &low in &order {
            let Some(c) = (if low == cell {
                Some(home.clone())
            } else {
                records.cell(bx, by, low)
            }) else {
                continue;
            };
            let Some((mesh, _)) = self.mesh(c.mesh.0) else {
                continue;
            };
            let at = Mat4::from_parts(c.frame.origin, c.frame.rotation, Vec3::new(1.0, 1.0, 1.0))
                .then(&place);
            let b = mesh.bounding_box();
            let (lo, hi) = placed_box(
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
                &at,
            );
            let shapes = self.doorways(c.mesh.0);
            let doorways = c
                .portals
                .iter()
                .filter_map(|p| {
                    let corners = shapes.get(usize::from(p.portal))?;
                    (corners.len() >= 3).then(|| Doorway {
                        corners: corners.clone(),
                        beyond: if p.flags & PORTAL_OUT != 0 {
                            Beyond::Outside
                        } else {
                            Beyond::Cell(p.other_cell)
                        },
                    })
                })
                .collect();
            cells.insert(
                low,
                Room {
                    place: at,
                    lo,
                    hi,
                    no_cull: c.flags & CELL_NO_CULL != 0,
                    sky: c.flags & CELL_SKY != 0,
                    doorways,
                    shared: c.shared.clone(),
                    fog_cut: self
                        .fog_cuts
                        .get(&(high | u32::from(low)))
                        .copied()
                        .flatten(),
                },
            );
        }
        let paint = lights
            .as_ref()
            .map_or(Color::new(1.0, 1.0, 1.0, 1.0), |l| l.fog_paint);
        let mut rooms = Rooms {
            root: cell,
            stab: home.visible.clone(),
            cells,
            group_of: std::collections::HashMap::new(),
            groups: Vec::new(),
            fog_color: [paint.r, paint.g, paint.b, paint.a],
        };
        rooms.make_groups(&order);
        Some(rooms)
    }

    /// The objects standing in the interior cell `cell` of block `block` and in the cells it sees
    /// whose looks play startup effects.
    pub fn interior_placed(&mut self, block: (i32, i32), cell: u16) -> Vec<PlacedLook> {
        let mut out = Vec::new();
        for low in self.seen_cells(block, cell) {
            let Some(c) = self.records.cell(block.0, block.1, low) else {
                continue;
            };
            for (i, e) in c
                .entities
                .iter()
                .flat_map(|g| g.entities.iter())
                .enumerate()
            {
                out.extend(self.placed_look(e, block, low, i));
            }
        }
        out
    }

    /// The batches of the interior cell `cell` (its low word) of block `block` and of the cells it
    /// sees, with the entities placed in each, drawn from `spot` (whose block frame the batches are
    /// placed in); the cells lit by the block's interior light, `outside` the day outside.
    // D2-06604, D2-03517
    pub fn interior(
        &mut self,
        spot: &Spot,
        block: (i32, i32),
        cell: u16,
        outside: &Outside,
    ) -> Vec<ObjectBatch> {
        let records = self.records;
        let (bx, by) = block;
        let Some(home) = records.cell(bx, by, cell) else {
            return Vec::new();
        };
        let mut lows = vec![cell];
        for &c in home.visible.iter().chain(&home.shared) {
            if !lows.contains(&c) {
                lows.push(c);
            }
        }
        let block_len = self.region.land.grid().block_length();
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let place = Mat4::translation(Vec3::new(
            (bx - spot.block.0) as f32 * block_len,
            (by - spot.block.1) as f32 * block_len,
            0.0,
        ));
        let mut counts = Counts::default();
        let lights = self.block_lights(block);
        let mut scene = lights.as_deref().map(BlockLights::scene);
        #[allow(clippy::cast_sign_loss)]
        let high = ((bx as u32) << 24) | ((by as u32) << 16);
        // Each part: the cell it is (for a cell's own mesh), or the object it belongs to (by its
        // cell and place in the cell's list, with where it stands and how far its lights reach),
        // and what culls it.
        let mut parts: Vec<(Option<u32>, Option<ObjectReach>, Part, Cull)> = Vec::new();
        for low in lows {
            let Some(c) = (if low == cell {
                Some(home.clone())
            } else {
                records.cell(bx, by, low)
            }) else {
                continue;
            };
            let id = high | u32::from(low);
            let look = self.cell_look(id, &c);
            parts.push((
                Some(id),
                None,
                Part {
                    mesh: c.mesh,
                    transform: Mat4::from_parts(
                        c.frame.origin,
                        c.frame.rotation,
                        Vec3::new(1.0, 1.0, 1.0),
                    ),
                    detail: None,
                    look,
                },
                Cull::Cell(low),
            ));
            for (k, e) in c
                .entities
                .iter()
                .flat_map(|g| g.entities.iter())
                .enumerate()
            {
                let ps = entity_parts(records, e);
                let Some(first) = ps.first() else {
                    continue;
                };
                let at = e
                    .offset
                    .as_ref()
                    .map_or(first.transform.point(Vec3::ZERO), |o| {
                        let p = o.frame.origin;
                        Vec3::new(p.x, p.y, p.z)
                    });
                let radius = self.object_reach(&ps);
                let key = (id, u32::try_from(k + 1).unwrap_or(u32::MAX));
                let cull = self.object_cull(low, &ps, &place);
                parts.extend(
                    ps.into_iter()
                        .map(|p| (None, Some((key, at, radius)), p, cull)),
                );
            }
        }
        let mut out = Vec::new();
        for (cell_id, object, part, cull) in &parts {
            let at = part.transform.then(&place).point(Vec3::ZERO);
            let d = auberean_world::math::length(auberean_world::math::sub(at, spot.position));
            let mut batches = self.part_batches(part, &place, d, Color::WHITE, &mut counts);
            for b in &mut batches {
                b.cull = *cull;
            }
            // A cell's own mesh carries in its vertices the light its record lists and the sun's;
            // the lights kept about the cell, as many point lights as each layer's program takes of
            // them (no directional light inside), light it as they shine at the moment.
            if let (Some(id), Some(l), Some(scene)) = (cell_id, lights.as_deref(), scene.as_mut()) {
                let radius = self.cell_reach(part).unwrap_or(0.0);
                let at = part.transform.point(Vec3::ZERO);
                let kept = self.kept_lights((*id, 0), at, radius, l, scene, outside);
                let device = kept
                    .iter()
                    .filter(|&&k| l.lights[k].kind != LightKind::Directional)
                    .count();
                for b in &mut batches {
                    let variants = &b.layer.light_variants;
                    let points = variant_for(
                        variants,
                        0,
                        u32::try_from(device.min(DRAW_LIGHTS)).unwrap_or(0),
                    )
                    .map_or(0, |k| variants[k].1);
                    let points = usize::try_from(points).unwrap_or(0);
                    if let Some(lit) = self.lit_cell(*id, part, l, outside) {
                        b.vertices = lit;
                        b.layer.switches |= LIT_IN_VERTICES;
                        b.points =
                            PointLights::inside(&l.device_lights(&kept, outside, points), &place);
                    }
                }
            }
            // An object inside is lit by the lights it keeps where it stands, as many point
            // lights as each layer's program takes, and not by the sun.
            if let (Some((key, at, radius)), Some(l), Some(scene)) =
                (object, lights.as_deref(), scene.as_mut())
            {
                let kept = self.kept_lights(*key, *at, *radius, l, scene, outside);
                let device = kept
                    .iter()
                    .filter(|&&k| l.lights[k].kind != LightKind::Directional)
                    .count();
                for b in &mut batches {
                    let variants = &b.layer.light_variants;
                    let points = variant_for(
                        variants,
                        0,
                        u32::try_from(device.min(DRAW_LIGHTS)).unwrap_or(0),
                    )
                    .map_or(0, |k| variants[k].1);
                    let points = usize::try_from(points).unwrap_or(0);
                    b.points =
                        PointLights::inside(&l.device_lights(&kept, outside, points), &place);
                }
            }
            out.extend(batches);
        }
        out
    }
}

impl SceneBuilder<'_> {
    /// Light `batches`, one body (the player's or one the server makes) standing at `at`, both in
    /// the frame of `spot`'s block, inside block `block`, as an object inside is lit: by the lights
    /// it keeps where it stands, reaching the largest half side of its box, as the frame's first
    /// draw keeps them (bodies move, so nothing is kept from frame to frame), in place of the sun.
    /// Returns the way the lights it keeps throw its shadow (the sun's way when the block has no
    /// lights, or the body has no draw).
    // D2-10629, D2-10630
    pub fn light_inside(
        &mut self,
        spot: &Spot,
        block: (i32, i32),
        batches: &mut [ObjectBatch],
        at: Vec3,
        outside: &Outside,
    ) -> Vec3 {
        let Some(l) = self.block_lights(block) else {
            return outside.direction;
        };
        let block_len = self.region.land.grid().block_length();
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let (dx, dy) = (
            (block.0 - spot.block.0) as f32 * block_len,
            (block.1 - spot.block.1) as f32 * block_len,
        );
        let place = Mat4::translation(Vec3::new(dx, dy, 0.0));
        let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for b in batches.iter() {
            for v in b.vertices.iter() {
                let w = b
                    .transform
                    .point(Vec3::new(v.position[0], v.position[1], v.position[2]));
                lo = Vec3::new(lo.x.min(w.x), lo.y.min(w.y), lo.z.min(w.z));
                hi = Vec3::new(hi.x.max(w.x), hi.y.max(w.y), hi.z.max(w.z));
            }
        }
        if lo.x > hi.x {
            return outside.direction;
        }
        let radius = 0.5 * (hi.x - lo.x).max(hi.y - lo.y).max(hi.z - lo.z);
        let here = Vec3::new(at.x - dx, at.y - dy, at.z);
        let kept = l.scene().keep(&l, here, radius, outside);
        let device = kept
            .iter()
            .filter(|&&k| l.lights[k].kind != LightKind::Directional)
            .count();
        for b in batches.iter_mut() {
            let variants = &b.layer.light_variants;
            let points = variant_for(
                variants,
                0,
                u32::try_from(device.min(DRAW_LIGHTS)).unwrap_or(0),
            )
            .map_or(0, |k| variants[k].1);
            let points = usize::try_from(points).unwrap_or(0);
            b.points = PointLights::inside(&l.device_lights(&kept, outside, points), &place);
        }
        let throws: Vec<ShadowLight> = kept
            .iter()
            .map(|&k| {
                let light = &l.lights[k];
                let d = auberean_world::math::length(auberean_world::math::sub(light.origin, here));
                ShadowLight {
                    kind: match light.kind {
                        LightKind::Point => ShadowLightKind::Point,
                        LightKind::Directional => ShadowLightKind::Directional,
                        LightKind::Sky => ShadowLightKind::Sky,
                    },
                    origin: light.origin,
                    inside: light.cell & 0xFFFF >= 0x100,
                    direction: light.direction,
                    weight: l.weight(light, d, outside),
                }
            })
            .collect();
        shadow_direction(
            &throws,
            here,
            true,
            Vec3::new(0.0, 0.0, 1.0),
            outside.direction,
        )
    }

    /// The lights a lit particle system inside block `block` is lit by, standing at `at` in that
    /// block's frame: those kept where it stands, reaching no farther than the place itself, as
    /// the frame's first draw keeps them, in their order; no sun inside. A block with no lights
    /// gives none (its lit particles are black).
    // D2-10631
    pub fn particle_lights(
        &mut self,
        block: (i32, i32),
        at: Vec3,
        outside: &Outside,
    ) -> Vec<ParticleLight> {
        let Some(l) = self.block_lights(block) else {
            return Vec::new();
        };
        let kept = l.scene().keep(&l, at, 0.0, outside);
        l.particle_lights(&kept)
    }
}
