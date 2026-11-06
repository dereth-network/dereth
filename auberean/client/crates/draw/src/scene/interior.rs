//! The inside of a building or a dungeon seen from one of its cells: that cell, the cells it sees,
//! and what stands in them.

use std::sync::Arc;

use auberean_world::color::Color;
use auberean_world::inside::{
    any_carried_in_reach, block_offset, device_lights_kept, keep_carried, particle_lights_kept,
    BlockLights, CellVertex, Kept, Light, LightKind, Outside, ParticleLight, SceneLights,
    DRAW_LIGHTS, PARTICLE_LIGHTS,
};
use auberean_world::math::Mat4;
use auberean_world::place::{entity_parts, Part};

use super::{
    placed_box, Counts, Cull, Frame, KeptBy, ObjectBatch, PlacedCaster, PointLights, SceneBuilder,
    Spot,
};
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
pub(crate) type ObjectReach = ((u32, u32), Vec3, f32);

/// A block and its interior light, read once for a frame's draws.
type BlockLit = ((i32, i32), Option<Arc<BlockLights>>);

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
    // D2-03524, D2-03525
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
    pub(crate) fn object_reach(&mut self, parts: &[Part]) -> f32 {
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
    pub(crate) fn object_cull(&mut self, cell: u16, parts: &[Part], place: &Mat4) -> Cull {
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
            f.look_in = Arc::default();
        }
        f.looked_light = None;
        // The water takes no reflection with the camera in a cell.
        // D2-11030
        f.reflection = None;
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
        let (bx, by) = block;
        let home = self.records.cell(bx, by, cell)?;
        #[allow(clippy::cast_sign_loss)]
        let high = ((bx as u32) << 24) | ((by as u32) << 16);
        let lights = self.block_lights(block);
        self.fog_cuts.insert(
            high | u32::from(cell),
            lights.as_ref().and_then(|l| l.fog_cut),
        );
        let mut order = vec![cell];
        for &c in &home.visible {
            if !order.contains(&c) {
                order.push(c);
            }
        }
        let mut rooms = self.block_rooms(spot, block, &order);
        rooms.stab.clone_from(&home.visible);
        Some(rooms)
    }

    /// Cells `order` of block `block`, the first the walk's, as a walk through their doorways takes
    /// them, placed in the frame of `spot`'s block.
    // D2-03557
    pub(crate) fn block_rooms(&mut self, spot: &Spot, block: (i32, i32), order: &[u16]) -> Rooms {
        let records = self.records;
        let (bx, by) = block;
        #[allow(clippy::cast_sign_loss)]
        let high = ((bx as u32) << 24) | ((by as u32) << 16);
        let lights = self.block_lights(block);
        let block_len = self.region.land.grid().block_length();
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let place = Mat4::translation(Vec3::new(
            (bx - spot.block.0) as f32 * block_len,
            (by - spot.block.1) as f32 * block_len,
            0.0,
        ));
        let mut cells = std::collections::HashMap::new();
        for &low in order {
            let Some(c) = records.cell(bx, by, low) else {
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
                    visible: c.visible.clone(),
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
            block,
            root: order.first().copied().unwrap_or(0),
            stab: Vec::new(),
            cells,
            group_of: std::collections::HashMap::new(),
            groups: Vec::new(),
            fog_color: [paint.r, paint.g, paint.b, paint.a],
        };
        rooms.make_groups(order);
        rooms
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
        // A cell's own mesh takes the lights linked as it is drawn: the sky lights alone.
        let mut sky = lights
            .as_deref()
            .zip(scene.as_ref())
            .map(|(l, s)| s.sky_alone(l));
        #[allow(clippy::cast_sign_loss)]
        let high = ((bx as u32) << 24) | ((by as u32) << 16);
        // Each part: the cell it is (for a cell's own mesh), or the object it belongs to (by its
        // cell and place in the cell's list, with where it stands and how far its lights reach),
        // what culls it, for an object casting a volume of shadow what its draws carry, and
        // whether it is seen through.
        let mut parts: Vec<InteriorPart> = Vec::new();
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
                    played: None,
                },
                Cull::Cell(low),
                None,
                false,
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
                let caster = self.placed_caster(e, &ps, &place, None);
                let seen_through = self.placed_seen_through(e);
                parts.extend(
                    ps.into_iter()
                        .map(|p| (None, Some((key, at, radius)), p, cull, caster, seen_through)),
                );
            }
        }
        let mut out = Vec::new();
        for (cell_id, object, part, cull, caster, seen_through) in &parts {
            let at = part.transform.then(&place).point(Vec3::ZERO);
            let d = auberean_world::math::length(auberean_world::math::sub(at, spot.position));
            let mut batches = self.part_batches(part, &place, d, Color::WHITE, &mut counts);
            // An object's draws go farthest first by where the object stands.
            let anchor = object.map(|(_, at, _)| place.point(at));
            for b in &mut batches {
                b.cull = *cull;
                b.seen_through = *seen_through;
                b.anchor = anchor;
                if let Some(c) = caster {
                    b.casts_volume = true;
                    b.caster = Some(*c);
                }
            }
            // A cell's own mesh carries in its vertices the light its record lists and the sun's;
            // the sky lights kept about the cell, as many point lights as each layer's program
            // takes of them, light it as they shine at the moment.
            if let (Some(id), Some(l), Some(sky)) = (cell_id, lights.as_deref(), sky.as_mut()) {
                let radius = self.cell_reach(part).unwrap_or(0.0);
                let at = part.transform.point(Vec3::ZERO);
                let kept = self.kept_lights((*id, 0), at, radius, l, sky, outside);
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
                        b.kept_by = Some(KeptBy {
                            key: (*id, 0),
                            block,
                            cell: *id,
                            at,
                            radius,
                            took_carried: false,
                            own: true,
                        });
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
                // A placed object casting a volume of shadow throws it the way its lights do.
                // D2-10630
                let toward = caster.is_some().then(|| {
                    let throws: Vec<ShadowLight> = kept
                        .iter()
                        .map(|&i| shadow_light(&l.lights[i], key.0, *at, outside))
                        .collect();
                    shadow_direction(
                        &throws,
                        *at,
                        true,
                        Vec3::new(0.0, 0.0, 1.0),
                        outside.direction,
                    )
                });
                for b in &mut batches {
                    if let Some(c) = b.caster.as_mut() {
                        c.toward = toward;
                    }
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
                    b.kept_by = Some(KeptBy {
                        key: *key,
                        block,
                        cell: key.0,
                        at: *at,
                        radius: *radius,
                        took_carried: false,
                        own: true,
                    });
                }
            }
            out.extend(batches);
        }
        out
    }

    /// The draws standing somewhere (`batches`, those that keep their lights where they stand, see
    /// [`ObjectBatch::kept_by`]) lit again for this frame with the lights objects carry
    /// (`carried`, each at its place in its own cell's block): a draw a carried light reaches
    /// takes it among the lights it keeps, sorted by their weight where it stands, as many point
    /// lights as each layer's program takes of them (outdoors, beside the sun); the others keep
    /// only their own (outdoors, the sun). `spot` is the frame's place, `outside` the day outside.
    /// Draws the frame before lit with carried lights are put back when none is carried now.
    // D2-10630, D2-10619, D2-10628
    pub fn light_carried(
        &mut self,
        spot: &Spot,
        batches: &mut [ObjectBatch],
        carried: &[Light],
        outside: &Outside,
    ) {
        if carried.is_empty() && !self.carried_lit {
            return;
        }
        self.carried_lit = !carried.is_empty();
        let block_len = self.region.land.grid().block_length();
        let mut blocks: Vec<BlockLit> = Vec::new();
        for b in batches.iter_mut() {
            let Some(mut kb) = b.kept_by else {
                continue;
            };
            // A draw no carried light reaches keeps its lights as they are, unless they were taken
            // last time.
            let reached = any_carried_in_reach(carried, kb.cell, kb.at, kb.radius);
            if !reached && !kb.took_carried {
                continue;
            }
            kb.took_carried = reached;
            b.kept_by = Some(kb);
            let inside = kb.cell & 0xFFFF >= 0x100;
            let lights = match blocks.iter().find(|(k, _)| *k == kb.block) {
                _ if !inside => None,
                Some((_, l)) => l.clone(),
                None => {
                    let l = self.block_lights(kb.block);
                    blocks.push((kb.block, l.clone()));
                    l
                }
            };
            let own = if kb.own {
                self.kept_lights.get(&kb.key).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            let kept = keep_carried(
                lights.as_deref(),
                &own,
                carried,
                kb.cell,
                kb.at,
                kb.radius,
                outside,
            );
            #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
            let place = Mat4::translation(Vec3::new(
                (kb.block.0 - spot.block.0) as f32 * block_len,
                (kb.block.1 - spot.block.1) as f32 * block_len,
                0.0,
            ));
            let (points, sun) = if inside {
                (
                    program_points(b, device_count(lights.as_deref(), &kept, carried)),
                    false,
                )
            } else {
                outdoor_points(b, device_count(None, &kept, carried))
            };
            let device =
                device_lights_kept(lights.as_deref(), &kept, carried, kb.cell, outside, points);
            b.points = if sun {
                PointLights::outdoors(&device, &place)
            } else {
                PointLights::inside(&device, &place)
            };
        }
        self.turn_casters(batches, carried, outside, &mut blocks);
    }

    /// The way the placed objects among `batches` that cast a volume of shadow throw it this frame,
    /// each from where its first draw keeps its lights: the lights it keeps there, the carried ones
    /// that reach it among them, as the first of them decides (see [`shadow_direction`]); outdoors,
    /// where it keeps none of its own, the sun's way when no carried light reaches it. Every draw
    /// of the object carries the way.
    // D2-10630
    fn turn_casters(
        &mut self,
        batches: &mut [ObjectBatch],
        carried: &[Light],
        outside: &Outside,
        blocks: &mut Vec<BlockLit>,
    ) {
        let mut k = 0;
        while k < batches.len() {
            let (Some(c), Some(kb)) = (batches[k].caster, batches[k].kept_by) else {
                k += 1;
                continue;
            };
            let start = k;
            while batches
                .get(k)
                .is_some_and(|b| b.caster.is_some_and(|o| o.object == c.object))
            {
                k += 1;
            }
            let inside = kb.cell & 0xFFFF >= 0x100;
            let lights = if inside {
                if let Some((_, l)) = blocks.iter().find(|(b, _)| *b == kb.block) {
                    l.clone()
                } else {
                    let l = self.block_lights(kb.block);
                    blocks.push((kb.block, l.clone()));
                    l
                }
            } else {
                None
            };
            let own = self.kept_lights.get(&kb.key).cloned().unwrap_or_default();
            let kept = keep_carried(
                lights.as_deref(),
                &own,
                carried,
                kb.cell,
                kb.at,
                kb.radius,
                outside,
            );
            let throws: Vec<ShadowLight> = kept
                .iter()
                .filter_map(|k| match *k {
                    Kept::Block(i) => lights
                        .as_deref()
                        .map(|l| shadow_light(&l.lights[i], kb.cell, kb.at, outside)),
                    Kept::Carried(i) => Some(shadow_light(&carried[i], kb.cell, kb.at, outside)),
                })
                .collect();
            let toward = (inside || !throws.is_empty()).then(|| {
                shadow_direction(
                    &throws,
                    kb.at,
                    inside,
                    Vec3::new(0.0, 0.0, 1.0),
                    outside.direction,
                )
            });
            for b in &mut batches[start..k] {
                if let Some(c) = b.caster.as_mut() {
                    c.toward = toward;
                }
            }
        }
    }

    /// Where an object standing outdoors in block `block` keeps its lights (see [`KeptBy`]): only
    /// the lights objects carry reach it there (no light is kept outdoors), from its place and
    /// the largest half side of its box.
    pub(crate) fn outdoor_kept_by(&mut self, block: (i32, i32), part: &Part) -> KeptBy {
        #[allow(clippy::cast_sign_loss)]
        let cell = ((block.0 as u32) << 24) | ((block.1 as u32 & 0xFF) << 16) | 1;
        KeptBy {
            key: (cell, 0),
            block,
            cell,
            at: part.transform.point(Vec3::ZERO),
            radius: self.object_reach(std::slice::from_ref(part)),
            took_carried: false,
            own: false,
        }
    }

    /// Light `batches`, the draws of `part` in block `block`'s interior cell `id` seen from outside
    /// (the cell's own mesh, or with `object` one standing in it: its key, place and reach), as a
    /// cell is lit and not by the sun: the cell's mesh by the light its vertices carry (its record's
    /// lights and the day's sun, baked as for a camera inside) and the sky lights it keeps, what
    /// stands in it by the lights it keeps, each as many point lights as each layer's program
    /// takes, as for a camera inside; both by the lights objects carry inside that reach them, each
    /// frame. A block with no interior light leaves them as they are. `lights` is the block's: the
    /// scene and the sky lights alone that its draws keep lights from.
    // D2-03619, D2-03609, D2-10630
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn light_seen_from_outside(
        &mut self,
        batches: &mut [ObjectBatch],
        part: &Part,
        block: (i32, i32),
        id: u32,
        object: Option<ObjectReach>,
        lights: Option<&mut (SceneLights, SceneLights, Arc<BlockLights>)>,
        outside: &Outside,
    ) {
        let Some((scene, sky, lights)) = lights else {
            return;
        };
        let lights = lights.clone();
        let (key, at, radius) = match object {
            Some(o) => o,
            None => (
                (id, 0),
                part.transform.point(Vec3::ZERO),
                self.cell_reach(part).unwrap_or(0.0),
            ),
        };
        let lit = if object.is_none() {
            let Some(lit) = self.lit_cell(id, part, &lights, outside) else {
                return;
            };
            Some(lit)
        } else {
            None
        };
        let scene = if object.is_none() { sky } else { scene };
        let kept = self.kept_lights(key, at, radius, &lights, scene, outside);
        let device = kept
            .iter()
            .filter(|&&k| lights.lights[k].kind != LightKind::Directional)
            .count();
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let place = batches.first().map_or(Mat4::IDENTITY, |b| {
            let p = b.transform.point(Vec3::ZERO);
            let q = part.transform.point(Vec3::ZERO);
            Mat4::translation(Vec3::new(p.x - q.x, p.y - q.y, 0.0))
        });
        for b in batches.iter_mut() {
            if let Some(lit) = &lit {
                b.vertices = lit.clone();
                b.layer.switches |= LIT_IN_VERTICES;
            }
            let variants = &b.layer.light_variants;
            let points = variant_for(
                variants,
                0,
                u32::try_from(device.min(DRAW_LIGHTS)).unwrap_or(0),
            )
            .map_or(0, |k| variants[k].1);
            let points = usize::try_from(points).unwrap_or(0);
            b.points = PointLights::inside(&lights.device_lights(&kept, outside, points), &place);
            b.kept_by = Some(KeptBy {
                key,
                block,
                cell: id,
                at,
                radius,
                took_carried: false,
                own: true,
            });
        }
    }

    /// Light `batches`, one body standing outdoors at `at` in cell `cell` (both in the frame of
    /// `spot`'s block), with the lights objects carry that reach it (`carried`), by the sun and by
    /// them, as many point lights as each layer's program with the sun takes. Returns the way they
    /// throw its shadow (see [`shadow_direction`]), or none when none reaches it and the sun
    /// throws it.
    // D2-10630, D2-10619
    #[allow(clippy::too_many_arguments)]
    pub fn light_outdoors(
        &mut self,
        spot: &Spot,
        batches: &mut [ObjectBatch],
        at: Vec3,
        cell: u32,
        outside: &Outside,
        carried: &[Light],
    ) -> Option<Vec3> {
        if carried.is_empty() {
            return None;
        }
        let radius = reach_of(batches)?;
        #[allow(clippy::cast_sign_loss)]
        let cell =
            ((spot.block.0 as u32) << 24) | ((spot.block.1 as u32 & 0xFF) << 16) | (cell & 0xFFFF);
        let kept = keep_carried(None, &[], carried, cell, at, radius, outside);
        if kept.is_empty() {
            return None;
        }
        let device = device_count(None, &kept, carried);
        for b in batches.iter_mut() {
            let (points, sun) = outdoor_points(b, device);
            let lights = device_lights_kept(None, &kept, carried, cell, outside, points);
            b.points = if sun {
                PointLights::outdoors(&lights, &Mat4::IDENTITY)
            } else {
                PointLights::inside(&lights, &Mat4::IDENTITY)
            };
        }
        let throws: Vec<ShadowLight> = kept
            .iter()
            .filter_map(|k| match *k {
                Kept::Carried(i) => Some(shadow_light(&carried[i], cell, at, outside)),
                Kept::Block(_) => None,
            })
            .collect();
        Some(shadow_direction(
            &throws,
            at,
            false,
            Vec3::new(0.0, 0.0, 1.0),
            outside.direction,
        ))
    }
}

/// How many point lights the program of draw `b`'s layer takes outdoors with `device` of them
/// beside the sun, and whether the sun lights it too: the program made for the sun and that many
/// (or the nearest it has, see [`variant_for`]); one made for point lights alone (a layer lit per
/// pixel by one light) is lit by its point lights in place of the sun. With none, the sun alone.
fn outdoor_points(b: &ObjectBatch, device: usize) -> (usize, bool) {
    if device == 0 {
        return (0, true);
    }
    let variants = &b.layer.light_variants;
    variant_for(
        variants,
        1,
        u32::try_from(device.min(DRAW_LIGHTS)).unwrap_or(0),
    )
    .map(|k| variants[k])
    .map_or((0, true), |v| (usize::try_from(v.1).unwrap_or(0), v.0 == 1))
}

/// How many of the lights `kept` are not directional: those the device takes as point lights.
fn device_count(lights: Option<&BlockLights>, kept: &[Kept], carried: &[Light]) -> usize {
    kept.iter()
        .filter(|k| match **k {
            Kept::Block(i) => lights.is_some_and(|l| l.lights[i].kind != LightKind::Directional),
            Kept::Carried(i) => carried[i].kind != LightKind::Directional,
        })
        .count()
}

/// How many point lights the program of draw `b`'s layer takes with `device` of them (no
/// directional light inside).
fn program_points(b: &ObjectBatch, device: usize) -> usize {
    let variants = &b.layer.light_variants;
    let points = variant_for(
        variants,
        0,
        u32::try_from(device.min(DRAW_LIGHTS)).unwrap_or(0),
    )
    .map_or(0, |k| variants[k].1);
    usize::try_from(points).unwrap_or(0)
}

/// Light `l` as the shadow of an object at `at` in cell `cell` (its block's frame) sees it, its
/// weight there.
fn shadow_light(l: &Light, cell: u32, at: Vec3, outside: &Outside) -> ShadowLight {
    let origin = auberean_world::math::add(l.origin, block_offset(cell, l.cell));
    let d = auberean_world::math::length(auberean_world::math::sub(origin, at));
    ShadowLight {
        kind: match l.kind {
            LightKind::Point => ShadowLightKind::Point,
            LightKind::Directional => ShadowLightKind::Directional,
            LightKind::Sky => ShadowLightKind::Sky,
        },
        origin,
        inside: l.cell & 0xFFFF >= 0x100,
        direction: l.direction,
        weight: BlockLights::weight_of(l, d, outside),
    }
}

/// One part of what an interior's frame draws: the cell it is (for a cell's own mesh) or the
/// object it belongs to, the part, what culls it, what its draws carry when its object casts a
/// volume of shadow, and whether its object is seen through.
type InteriorPart = (
    Option<u32>,
    Option<ObjectReach>,
    Part,
    Cull,
    Option<PlacedCaster>,
    bool,
);

/// The largest half side of the box about `batches` in the frame's world; none for draws with no
/// vertex.
fn reach_of(batches: &[ObjectBatch]) -> Option<f32> {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    // The layers and fragments of one part share its vertices, in one place.
    let mut done: Vec<(usize, [[f32; 4]; 4])> = Vec::new();
    for b in batches {
        let this = (b.places(), b.transform.0);
        if done.contains(&this) {
            continue;
        }
        done.push(this);
        let at = b.transform.0[3];
        if b.posed.is_some() {
            if let Some((l, h)) = b.turned_box((0.0, 1.0)) {
                super::widen(&mut lo, &mut hi, [l[0] + at[0], l[1] + at[1], l[2] + at[2]]);
                super::widen(&mut lo, &mut hi, [h[0] + at[0], h[1] + at[1], h[2] + at[2]]);
            }
            continue;
        }
        b.each_position(|[x, y, z]| {
            let w = b.transform.point(Vec3::new(x, y, z));
            super::widen(&mut lo, &mut hi, [w.x, w.y, w.z]);
        });
    }
    (lo[0] <= hi[0]).then(|| 0.5 * (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]))
}

/// Light `batches`, one draw of an interface scene standing at `at`, by the lights it keeps there
/// among the scene's: in place of the sun, or beside it (`sun`) when the scene has a directional
/// light of its own.
// D2-10629, D2-10630
pub fn light_view_draw(
    batches: &mut [ObjectBatch],
    at: Vec3,
    lights: &BlockLights,
    order: &mut SceneLights,
    outside: &Outside,
    sun: bool,
) {
    let Some(radius) = reach_of(batches) else {
        return;
    };
    let kept = order.keep(lights, at, radius, outside);
    let device = kept
        .iter()
        .filter(|&&k| lights.lights[k].kind != LightKind::Directional)
        .count();
    for b in batches.iter_mut() {
        let (points, with_sun) = if sun {
            outdoor_points(b, device)
        } else {
            (program_points(b, device), false)
        };
        let device = lights.device_lights(&kept, outside, points);
        b.points = if with_sun {
            PointLights::outdoors(&device, &Mat4::IDENTITY)
        } else {
            PointLights::inside(&device, &Mat4::IDENTITY)
        };
    }
}

impl SceneBuilder<'_> {
    /// Light `batches`, one body (the player's or one the server makes) standing at `at` in cell
    /// `cell`, both in the frame of `spot`'s block, inside block `block`, as an object inside is
    /// lit: by the lights it keeps where it stands, reaching the largest half side of its box, as the
    /// frame's first draw keeps them (bodies move, so nothing is kept from frame to frame), with the
    /// lights objects carry that reach it (`carried`, see [`Self::light_carried`]), in place of the
    /// sun. Returns the way the lights it keeps throw its shadow (the sun's way when it keeps none,
    /// or the body has no draw).
    // D2-10629, D2-10630
    #[allow(clippy::too_many_arguments)]
    pub fn light_inside(
        &mut self,
        spot: &Spot,
        block: (i32, i32),
        batches: &mut [ObjectBatch],
        at: Vec3,
        cell: u32,
        outside: &Outside,
        carried: &[Light],
    ) -> Vec3 {
        let l = self.block_lights(block);
        if l.is_none() && carried.is_empty() {
            return outside.direction;
        }
        let block_len = self.region.land.grid().block_length();
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let (dx, dy) = (
            (block.0 - spot.block.0) as f32 * block_len,
            (block.1 - spot.block.1) as f32 * block_len,
        );
        let place = Mat4::translation(Vec3::new(dx, dy, 0.0));
        let Some(radius) = reach_of(batches) else {
            return outside.direction;
        };
        let here = Vec3::new(at.x - dx, at.y - dy, at.z);
        #[allow(clippy::cast_sign_loss)]
        let cell = ((block.0 as u32) << 24) | ((block.1 as u32 & 0xFF) << 16) | (cell & 0xFFFF);
        let own = l
            .as_deref()
            .map(|l| l.scene().keep(l, here, radius, outside))
            .unwrap_or_default();
        let kept = keep_carried(l.as_deref(), &own, carried, cell, here, radius, outside);
        let device = device_count(l.as_deref(), &kept, carried);
        for b in batches.iter_mut() {
            let points = program_points(b, device);
            b.points = PointLights::inside(
                &device_lights_kept(l.as_deref(), &kept, carried, cell, outside, points),
                &place,
            );
        }
        let throws: Vec<ShadowLight> = kept
            .iter()
            .filter_map(|k| match *k {
                Kept::Block(i) => l
                    .as_deref()
                    .map(|b| shadow_light(&b.lights[i], cell, here, outside)),
                Kept::Carried(i) => Some(shadow_light(&carried[i], cell, here, outside)),
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
    ///
    /// The lights objects carry (`carried`, see [`Self::light_carried`]) that reach the place join
    /// them.
    // D2-10631
    pub fn particle_lights(
        &mut self,
        block: (i32, i32),
        at: Vec3,
        outside: &Outside,
        carried: &[Light],
    ) -> Vec<ParticleLight> {
        let l = self.block_lights(block);
        let own = l
            .as_deref()
            .map(|l| l.scene().keep(l, at, 0.0, outside))
            .unwrap_or_default();
        #[allow(clippy::cast_sign_loss)]
        let cell = ((block.0 as u32) << 24) | ((block.1 as u32 & 0xFF) << 16) | 0x100;
        let kept = keep_carried(l.as_deref(), &own, carried, cell, at, 0.0, outside);
        particle_lights_kept(l.as_deref(), &kept, carried, cell)
    }

    /// The lights a lit particle system standing outdoors in block `block` (at `at` in that
    /// block's frame) is lit by: the lights objects carry that reach the place, strongest there
    /// first, then the sun (`sun`, its color times its brightness, alike everywhere). `None` when no
    /// carried light reaches it: the sun alone lights it.
    // D2-10631
    #[must_use]
    pub fn particle_lights_outdoors(
        block: (i32, i32),
        at: Vec3,
        outside: &Outside,
        carried: &[Light],
        sun: Color,
    ) -> Option<Vec<ParticleLight>> {
        #[allow(clippy::cast_sign_loss)]
        let cell = ((block.0 as u32) << 24) | ((block.1 as u32 & 0xFF) << 16) | 0x1;
        let kept = keep_carried(None, &[], carried, cell, at, 0.0, outside);
        if kept.is_empty() {
            return None;
        }
        let mut lights = particle_lights_kept(None, &kept, carried, cell);
        lights.truncate(PARTICLE_LIGHTS - 1);
        lights.push(ParticleLight {
            origin: Vec3::ZERO,
            color: sun,
            range: None,
            flicker: None,
        });
        Some(lights)
    }
}

/// The light of a frame inside: the block's interior ambient; for what stands in the cells, the
/// lights nearest `at` taken together as one light from their side; the interior fog, or none.
// D2-11007, D2-10629
pub fn inside_light(
    light: &mut auberean_world::sky::DayLight,
    lights: Option<&auberean_world::inside::BlockLights>,
    at: Vec3,
    outside: &auberean_world::inside::Outside,
) {
    use auberean_world::color::Color;
    use auberean_world::math::{length, normalize, sub};
    light.fog_min = 0.0;
    light.fog_max = 0.0;
    let Some(l) = lights else {
        return;
    };
    light.ambient_color = l.ambient;
    light.ambient_brightness = 1.0;
    let mut color = Color::new(0.0, 0.0, 0.0, 1.0);
    let mut from = Vec3::new(0.0, 0.0, 0.0);
    for (origin, shine, range) in l.chosen(at, 1.0, outside) {
        let to = sub(at, origin);
        let d = length(to);
        let k = 1.0 / (1.0 + 10.0 * d * d / (range * range));
        color = Color::new(
            color.r + shine.r * k,
            color.g + shine.g * k,
            color.b + shine.b * k,
            1.0,
        );
        let w = (shine.r + shine.g + shine.b) * k;
        let n = normalize(to);
        from = Vec3::new(from.x + n.x * w, from.y + n.y * w, from.z + n.z * w);
    }
    light.direct_color = Color::new(color.r.min(1.0), color.g.min(1.0), color.b.min(1.0), 1.0);
    light.direct_brightness = 1.0;
    if length(from) > 0.0 {
        light.light_direction = normalize(from);
    }
    if let Some((c, near, far)) = l.fog {
        light.fog_color = c;
        light.fog_min = near;
        light.fog_max = far;
    }
}
