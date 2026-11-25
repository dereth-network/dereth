//! Skinned bodies in the retail look: an object whose setup has a skinned body
//! ([`dereth_assets::SkinnedBody`]) is drawn as that one mesh, bent by its parts' frames each
//! frame, instead of as its parts' models.
//!
//! Everything else about the object is unchanged: its parts are animated, placed, picked and
//! collided as any object's are, and the body only reads their frames. A setup without the record
//! draws its parts, and so does every setup while [`SceneDraw::set_skinned_bodies`] is off.
//!
//! - A **template** per appearance holds the body's meshes at its bind pose, built through the
//!   scene's surface cache like any part's (so the object's palette and texture changes reach
//!   it), and the links on their pictures. Objects that look alike share it.
//! - Each **object** keeps its own copy of the meshes' vertices, rewritten from its parts' frames
//!   before the object pass collects what it draws ([`SceneDraw::pose_skins`]). The copy is in the
//!   first part's space and is drawn with that part's frame and scale.
//! - **Only what can be seen is posed.** A body outside the view, or past where anything is
//!   drawn, keeps its last pose; one far from the eye is posed every second or fourth frame
//!   ([`pose_every`]); while the remaster draws the world it poses the bodies itself, and none is
//!   posed here.
//! - **Levels of detail.** A body that carries coarser levels draws the one its distance from the
//!   eye chooses ([`SkinnedBody::level_at`]), and only the vertices that level draws are posed.

use super::*;

use dereth_assets::{skinned_body_id, Decode, SkinnedBody, SkinnedVertex};
use glam::{Mat4, Quat as GQuat, Vec3 as GVec3};

/// What posing the skinned bodies took on the processor in a frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkinCosts {
    /// How many bodies were posed.
    pub posed: u32,
    /// How long posing them took, milliseconds.
    pub ms: f32,
}

/// How many frames apart a body `distance` meters from the eye is posed: every frame within 40
/// meters, every second one to 80, every fourth beyond. A far body's limbs move a pixel or two in
/// a frame, and its place goes with its first part's frame, which is every frame's.
fn pose_every(distance: f32) -> u64 {
    if distance > 80.0 {
        4
    } else if distance > 40.0 {
        2
    } else {
        1
    }
}

/// The widest the view reaches from its middle, radians: the field of view's half, widened across
/// the widest picture drawn (a 2.4 to 1 one) to its corners.
fn view_reach(fov: f32) -> f32 {
    let wide = 2.4f32;
    use dereth_primitives::num::math;
    math::atanf(math::tanf(fov * 0.5) * (1.0 + wide * wide).sqrt())
}

/// Whether a sphere at `centre` of `radius` may be in a view from `eye` looking along `forward`
/// (a unit vector) reaching `reach` radians from its middle and `far` meters ahead.
fn may_be_seen(eye: Vec3, forward: Vec3, reach: f32, far: f32, centre: Vec3, radius: f32) -> bool {
    let to = Vec3::new(centre.x - eye.x, centre.y - eye.y, centre.z - eye.z);
    let d = to.mag2().sqrt();
    if d <= radius {
        return true;
    }
    if d - radius > far {
        return false;
    }
    let along = (to.x * forward.x + to.y * forward.y + to.z * forward.z) / d;
    use dereth_primitives::num::math;
    let off = math::acosf(along.clamp(-1.0, 1.0));
    off - math::asinf((radius / d).clamp(-1.0, 1.0)) <= reach
}

/// One level of detail of a skinned body at its bind pose.
#[derive(Debug)]
pub(super) struct SkinLevel {
    /// One mesh a surface group, as [`build_meshes`] makes a part's; they hold the pictures'
    /// links.
    meshes: Vec<PartMesh>,
    /// For each mesh, the body's vertex each of its drawn vertices is.
    sources: Vec<Vec<u32>>,
    /// Every vertex of the body it draws, once each.
    used: Vec<u32>,
}

/// A skinned body's meshes at its bind pose, shared by every object of one appearance.
#[derive(Debug)]
pub(super) struct SkinTemplate {
    /// Each level of detail, the fullest first.
    levels: Vec<SkinLevel>,
    /// How far from the eye each level past the fullest is drawn from ([`SkinnedBody::levels`]).
    from: Vec<f32>,
    vertices: Vec<SkinnedVertex>,
    /// For each part, the inverse of its frame at the bind pose.
    unbind: Vec<Mat4>,
    /// The body's drawing sphere at its bind pose, in the first part's unscaled space.
    rest: Option<(Vec3, f32)>,
}

impl SkinTemplate {
    /// The template's meshes at every level, for the release of their pictures.
    pub(super) fn meshes(&self) -> impl Iterator<Item = &PartMesh> {
        self.levels.iter().flat_map(|l| &l.meshes)
    }
}

/// A mesh's copy, its vertices its own.
fn copied(m: &PartMesh) -> PartMesh {
    PartMesh {
        key: m.key,
        surface_type: m.surface_type,
        key_material_alpha: m.key_material_alpha,
        key_force_alpha: m.key_force_alpha,
        key_detail: m.key_detail,
        alpha_ref: m.alpha_ref,
        texture: m.texture,
        sampler: m.sampler,
        subset_mask: m.subset_mask,
        surface: m.surface,
        luminosity: m.luminosity,
        vertices: m.vertices.clone(),
    }
}

/// One object's skinned body: its own vertices, posed this frame.
#[derive(Debug)]
pub(super) struct ObjectSkin {
    template: Arc<SkinTemplate>,
    /// Each level's meshes, the fullest first; a level not yet drawn has none.
    meshes: Vec<Vec<PartMesh>>,
    /// The level drawn.
    level: usize,
    /// The posed body's drawing sphere in the first part's unscaled space.
    sphere: Option<(Vec3, f32)>,
    /// Whether its pose is this frame's or a recent one's: false until it is first posed, and
    /// once it has been left unposed out of sight, so it is posed as soon as it is seen again.
    fresh: bool,
}

impl ObjectSkin {
    fn new(template: Arc<SkinTemplate>) -> Self {
        // The fullest level's copy now; a coarser one's when it is first drawn.
        let meshes = template
            .levels
            .iter()
            .enumerate()
            .map(|(k, l)| {
                if k == 0 {
                    l.meshes.iter().map(copied).collect()
                } else {
                    Vec::new()
                }
            })
            .collect();
        let sphere = template.rest;
        Self {
            template,
            meshes,
            level: 0,
            sphere,
            fresh: false,
        }
    }

    /// The meshes to draw this frame.
    pub(super) fn meshes(&self) -> &[PartMesh] {
        self.meshes.get(self.level).map_or(&[], Vec::as_slice)
    }

    /// Draw level `level` (the coarsest the body has, past it) from now on; a level not drawn
    /// before is posed before it is drawn.
    fn set_level(&mut self, level: usize) {
        let level = level.min(self.meshes.len().saturating_sub(1));
        if level != self.level {
            if let (Some(mine), Some(l)) =
                (self.meshes.get_mut(level), self.template.levels.get(level))
            {
                if mine.is_empty() {
                    *mine = l.meshes.iter().map(copied).collect();
                }
            }
            self.level = level;
            self.fresh = false;
        }
    }

    /// The posed body's drawing sphere, in the first part's unscaled space.
    pub(super) fn sphere(&self) -> Option<(Vec3, f32)> {
        self.sphere
    }

    /// Bend the body to `parts`' frames: each vertex goes where its parts now carry it from
    /// where their bind frames held it, weighed, then into the first part's space (its scale
    /// taken out, since the draw puts it back).
    fn pose(&mut self, parts: &[dereth_animation::parts::PhysicsPart]) {
        let t = &self.template;
        let Some(anchor) = parts.first() else {
            return;
        };
        let scale = anchor.gfxobj_scale.x.max(1e-6);
        let to_anchor = matrix(&anchor.pos).inverse();
        let carry: Vec<Mat4> = t
            .unbind
            .iter()
            .enumerate()
            .map(|(k, unbind)| {
                let Some(p) = parts.get(k) else {
                    return Mat4::IDENTITY;
                };
                let mut rel = to_anchor * matrix(&p.pos);
                rel.w_axis.x /= scale;
                rel.w_axis.y /= scale;
                rel.w_axis.z /= scale;
                rel * *unbind
            })
            .collect();
        let Some(level) = t.levels.get(self.level) else {
            return;
        };
        // Only the vertices this level draws.
        let mut posed = vec![(GVec3::ZERO, GVec3::ZERO); t.vertices.len()];
        for &i in &level.used {
            let Some(v) = t.vertices.get(i as usize) else {
                continue;
            };
            let (mut p, mut n) = (GVec3::ZERO, GVec3::ZERO);
            let (vp, vn) = (gv(v.position), gv(v.normal));
            for k in 0..4 {
                let w = v.weights[k];
                if w <= 0.0 {
                    continue;
                }
                let m = carry
                    .get(usize::from(v.parts[k]))
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                p += m.transform_point3(vp) * w;
                n += m.transform_vector3(vn) * w;
            }
            posed[i as usize] = (p, n.normalize_or_zero());
        }
        let (mut lo, mut hi) = (GVec3::splat(f32::MAX), GVec3::splat(f32::MIN));
        let Some(meshes) = self.meshes.get_mut(self.level) else {
            return;
        };
        for (mesh, sources) in meshes.iter_mut().zip(&level.sources) {
            for (i, s) in sources.iter().enumerate() {
                let Some((p, n)) = posed.get(*s as usize) else {
                    continue;
                };
                lo = lo.min(*p);
                hi = hi.max(*p);
                let at = i * OBJECT_VERTEX_STRIDE;
                let Some(bytes) = mesh.vertices.get_mut(at..at + 24) else {
                    continue;
                };
                for (j, x) in [p.x, p.y, p.z, n.x, n.y, n.z].iter().enumerate() {
                    bytes[j * 4..j * 4 + 4].copy_from_slice(&x.to_le_bytes());
                }
            }
        }
        self.sphere = (lo.x <= hi.x).then(|| {
            let c = (lo + hi) * 0.5;
            (Vec3::new(c.x, c.y, c.z), (hi - lo).length() * 0.5)
        });
        self.fresh = true;
    }
}

fn gv(v: Vec3) -> GVec3 {
    GVec3::new(v.x, v.y, v.z)
}

/// A frame as a matrix: its turn, then its place.
pub(super) fn matrix(f: &Frame) -> Mat4 {
    let q = GQuat::from_xyzw(f.rotation.x, f.rotation.y, f.rotation.z, f.rotation.w);
    let q = if q.length_squared() > 0.0 {
        q.normalize()
    } else {
        GQuat::IDENTITY
    };
    Mat4::from_rotation_translation(q, gv(f.origin))
}

/// The look a skinned body over the first `count` of `parts` is drawn with: the palettes of the
/// first part that has a look, and the texture swaps of every part it bends, the earlier part's
/// winning where two swap the same texture. A description may lay a swap on any part whose model draws the texture,
/// and the body draws them all.
fn body_look(
    parts: &[dereth_animation::parts::PhysicsPart],
    count: usize,
) -> Option<dereth_animation::parts::SurfaceOverrides> {
    let mut look: Option<dereth_animation::parts::SurfaceOverrides> = None;
    for o in parts
        .iter()
        .take(count.max(1))
        .filter_map(|p| p.surface_overrides.as_ref())
    {
        match &mut look {
            None => look = Some(o.clone()),
            Some(all) => {
                for t in &o.texture_maps {
                    if !all.texture_maps.iter().any(|(old, _)| *old == t.0) {
                        all.texture_maps.push(*t);
                    }
                }
            }
        }
    }
    look
}

impl SceneDraw {
    /// Whether objects whose setups have a skinned body draw it (the default) or their parts.
    /// It takes effect for objects created after the change.
    pub fn set_skinned_bodies(&mut self, on: bool) {
        self.skinned_bodies = on;
    }

    /// The skinned body of an object of `setup` dressed as `parts` say, when its setup has one
    /// and skinned bodies are drawn: the template shared under `key`, built on first use, and
    /// the object's own copy.
    pub(super) fn skin_for(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        setup: DataId,
        key: &AppearanceKey,
        parts: &[dereth_animation::parts::PhysicsPart],
    ) -> Result<Option<ObjectSkin>, WorldError> {
        if !self.skinned_bodies {
            return Ok(None);
        }
        if let Some(t) = self.skins.get(key) {
            return Ok(Some(ObjectSkin::new(Arc::clone(t))));
        }
        let id = skinned_body_id(setup);
        let Ok(bytes) = store.read_typed(SkinnedBody::TYPE, id) else {
            return Ok(None);
        };
        let Ok(body) = SkinnedBody::decode_payload(id, &bytes) else {
            return Ok(None);
        };
        if body.setup != setup || body.bind.len() > parts.len() {
            return Ok(None);
        }
        let textures = TextureStore::with_environment_texture_detail(
            store,
            self.cfg.render.environment_texture_detail,
        );
        let look = body_look(parts, body.bind.len());
        let mut levels = Vec::new();
        for level in 0..=body.levels.len() {
            let mut groups = Vec::new();
            let mut sources = Vec::new();
            for g in body.groups.iter().filter(|g| usize::from(g.level) == level) {
                let mut group = dereth_client_runtime::models::SurfaceGroup {
                    surface: body.surfaces.get(g.surface as usize).copied(),
                    two_sided: g.two_sided,
                    // A body's picture coordinates run past the picture's edges, as a tiled
                    // model's do: its pictures repeat.
                    tiled: true,
                    ..Default::default()
                };
                let mut from = Vec::with_capacity(g.indices.len());
                for &i in &g.indices {
                    let Some(v) = body.vertices.get(i as usize) else {
                        continue;
                    };
                    group.vertices.push((v.position, v.uv[0], v.uv[1]));
                    group.normals.push(v.normal);
                    from.push(i);
                }
                groups.push(group);
                sources.push(from);
            }
            // A level with nothing of its own ends the body's levels.
            if groups.is_empty() {
                break;
            }
            let meshes = build_meshes(
                store,
                &mut self.land.bake,
                &textures,
                gpu,
                &groups,
                look.as_ref(),
                None,
            )
            .map_err(|e| WorldError::Render(e.to_string()))?;
            if meshes.len() != groups.len() {
                return Ok(None);
            }
            let mut used: Vec<u32> = sources.iter().flatten().copied().collect();
            used.sort_unstable();
            used.dedup();
            levels.push(SkinLevel {
                meshes,
                sources,
                used,
            });
        }
        if levels.is_empty() {
            return Ok(None);
        }
        let from: Vec<f32> = body.levels.iter().take(levels.len() - 1).copied().collect();
        // The bind pose is in the object's own space, as the parts' frames are.
        let unbind: Vec<Mat4> = body.bind.iter().map(|f| matrix(f).inverse()).collect();
        let to_first = unbind.first().copied().unwrap_or(Mat4::IDENTITY);
        let (mut lo, mut hi) = (GVec3::splat(f32::MAX), GVec3::splat(f32::MIN));
        for v in &body.vertices {
            let p = to_first.transform_point3(gv(v.position));
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let rest = (lo.x <= hi.x).then(|| {
            let c = (lo + hi) * 0.5;
            (Vec3::new(c.x, c.y, c.z), (hi - lo).length() * 0.5)
        });
        let t = Arc::new(SkinTemplate {
            levels,
            from,
            vertices: body.vertices,
            unbind,
            rest,
        });
        self.skins.insert(key.clone(), Arc::clone(&t));
        Ok(Some(ObjectSkin::new(t)))
    }

    /// Bend every skinned object to its parts' frames this frame. It runs after the object step
    /// has placed the parts, before the object pass collects what it draws.
    pub(super) fn pose_skins(&mut self, ws: &WorldState) {
        let started = web_time::Instant::now();
        let mut posed = 0u32;
        self.skin_frame = self.skin_frame.wrapping_add(1);
        // While the remaster draws the world it poses the bodies itself.
        #[cfg(feature = "remaster")]
        let drawn_here = !self.remaster_on();
        #[cfg(not(feature = "remaster"))]
        let drawn_here = true;
        let eye = ws.camera.position;
        let forward = ws.camera.forward();
        let reach = view_reach(self.cfg.render.game_fov_rad());
        let far = dereth_render::camera::ZFAR;
        for (id, o) in &ws.objects {
            let Some(d) = self.object_draws.get_mut(id) else {
                continue;
            };
            let Some(skin) = d.skin.as_mut() else {
                continue;
            };
            if !drawn_here {
                skin.fresh = false;
                continue;
            }
            let driver = o.sim.driver.borrow();
            let parts = &driver.part_array.parts;
            let Some(first) = parts.first() else {
                continue;
            };
            // Where the body may reach: its last sphere, widened for how far its limbs may
            // have swung since.
            let scale = first.gfxobj_scale.x.max(first.gfxobj_scale.z);
            let (centre, radius) = skin.sphere.unwrap_or((Vec3::ZERO, 1.0));
            let centre = dereth_terrain::math::localtoglobal(
                &first.pos,
                Vec3::new(centre.x * scale, centre.y * scale, centre.z * scale),
            );
            let radius = radius * scale * 1.5 + 1.0;
            if !o.drawn || !may_be_seen(eye, forward, reach, far, centre, radius) {
                skin.fresh = false;
                continue;
            }
            let distance = (centre.sub(eye).mag2().sqrt() - radius).max(0.0);
            skin.set_level(SkinnedBody::level_at(
                &skin.template.from,
                centre.sub(eye).mag2().sqrt(),
                scale,
            ));
            let every = pose_every(distance);
            if skin.fresh && (self.skin_frame.wrapping_add(u64::from(id.0))) % every != 0 {
                continue;
            }
            skin.pose(parts);
            posed += 1;
        }
        self.skin_costs = SkinCosts {
            posed,
            ms: started.elapsed().as_secs_f32() * 1000.0,
        };
    }

    /// What posing the skinned bodies took in the last frame, for an instrument to read.
    #[must_use]
    pub fn skinned_costs(&self) -> SkinCosts {
        self.skin_costs
    }

    /// Release the pictures of every template no object draws any longer; how many were.
    pub(super) fn release_unlinked_skins(&mut self, gpu: &mut Gpu) -> usize {
        // ORDER-OK: the templates whose strong count is 1 are found before any is released.
        let dead: Vec<AppearanceKey> = self
            .skins
            .iter()
            .filter(|(_, t)| Arc::strong_count(t) == 1)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &dead {
            if let Some(t) = self.skins.remove(key) {
                for m in t.meshes() {
                    if let Some(slot) = m.texture {
                        self.release_look_texture(gpu, slot, false);
                    }
                }
            }
        }
        dead.len()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the skinned body's posing arithmetic)
    use super::*;

    fn part(origin: Vec3, rotation: Quat) -> dereth_animation::parts::PhysicsPart {
        let mut p = dereth_animation::parts::PhysicsPart::new(DataId(0x0100_0001));
        p.pos = Frame::new(origin, rotation);
        p.gfxobj_scale = Vec3::new(1.0, 1.0, 1.0);
        p
    }

    fn template(bind: &[Frame], vertices: Vec<SkinnedVertex>) -> Arc<SkinTemplate> {
        let mut bytes = vec![0u8; vertices.len() * OBJECT_VERTEX_STRIDE];
        for (i, v) in vertices.iter().enumerate() {
            let at = i * OBJECT_VERTEX_STRIDE;
            bytes[at..at + 4].copy_from_slice(&v.position.x.to_le_bytes());
        }
        let key = PipelineKey::from_surface(
            &dereth_render::surface::Surface::default(),
            SurfaceContext::default(),
        )
        .0;
        let all: Vec<u32> = (0..u32::try_from(vertices.len()).unwrap()).collect();
        let level = |used: Vec<u32>| SkinLevel {
            meshes: vec![PartMesh {
                key,
                surface_type: 0,
                key_material_alpha: key,
                key_force_alpha: key,
                key_detail: key,
                alpha_ref: 0,
                texture: None,
                sampler: 0,
                subset_mask: 0,
                surface: None,
                luminosity: 0.0,
                vertices: bytes.clone(),
            }],
            sources: vec![used.clone()],
            used,
        };
        // A coarser level that draws (and so poses) only the first vertex, from 50 meters on.
        Arc::new(SkinTemplate {
            levels: vec![level(all.clone()), level(vec![0])],
            from: vec![50.0],
            vertices,
            unbind: bind.iter().map(|f| matrix(f).inverse()).collect(),
            rest: None,
        })
    }

    fn position(skin: &ObjectSkin, i: usize) -> Vec3 {
        let b = &skin.meshes[skin.level][0].vertices[i * OBJECT_VERTEX_STRIDE..];
        let f = |k: usize| f32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap());
        Vec3::new(f(0), f(1), f(2))
    }

    fn vertex(at: Vec3, parts: [u16; 4], weights: [f32; 4]) -> SkinnedVertex {
        SkinnedVertex {
            position: at,
            normal: Vec3::new(0.0, 0.0, 1.0),
            uv: [0.0, 0.0],
            parts,
            weights,
        }
    }

    #[test]
    fn a_vertex_follows_its_part_when_the_part_turns_and_stays_put_when_it_is_the_anchors() {
        // Two parts: the root at the origin and a limb a meter up. A vertex on the limb, a meter
        // out along x from it; another on the root.
        let bind = [
            Frame::default(),
            Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY),
        ];
        let t = template(
            &bind,
            vec![
                vertex(Vec3::new(1.0, 0.0, 1.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
                vertex(Vec3::new(0.5, 0.0, 0.0), [0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
            ],
        );
        let mut skin = ObjectSkin::new(t);
        // The object stands at (10, 0, 0); the limb has turned a quarter about z.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let parts = [
            part(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY),
            part(Vec3::new(10.0, 0.0, 1.0), Quat::new(h, 0.0, 0.0, h)),
        ];
        skin.pose(&parts);
        let limb = position(&skin, 0);
        assert!(
            (limb.x - 0.0).abs() < 1e-5 && (limb.y - 1.0).abs() < 1e-5,
            "{limb:?}"
        );
        assert!((limb.z - 1.0).abs() < 1e-5, "{limb:?}");
        let root = position(&skin, 1);
        assert!(
            (root.x - 0.5).abs() < 1e-5 && root.y.abs() < 1e-5,
            "{root:?}"
        );
        let s = skin.sphere().expect("a posed body has a sphere");
        assert!(s.1 > 0.5, "{s:?}");
    }

    #[test]
    fn a_vertex_shared_by_two_parts_lands_between_where_each_would_take_it() {
        let bind = [
            Frame::default(),
            Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY),
        ];
        let t = template(
            &bind,
            vec![vertex(
                Vec3::new(0.0, 0.0, 1.0),
                [0, 1, 0, 0],
                [0.5, 0.5, 0.0, 0.0],
            )],
        );
        let mut skin = ObjectSkin::new(t);
        // The limb has moved a meter along x; the root has not.
        let parts = [
            part(Vec3::ZERO, Quat::IDENTITY),
            part(Vec3::new(1.0, 0.0, 1.0), Quat::IDENTITY),
        ];
        skin.pose(&parts);
        let p = position(&skin, 0);
        assert!(
            (p.x - 0.5).abs() < 1e-5 && (p.z - 1.0).abs() < 1e-5,
            "{p:?}"
        );
    }

    #[test]
    fn a_scaled_object_is_posed_in_its_unscaled_space() {
        // The draw scales the first part's space; the posed vertices must not be scaled twice.
        let bind = [
            Frame::default(),
            Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY),
        ];
        let t = template(
            &bind,
            vec![vertex(
                Vec3::new(0.0, 0.0, 1.0),
                [1, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
            )],
        );
        let mut skin = ObjectSkin::new(t);
        let mut parts = [
            part(Vec3::ZERO, Quat::IDENTITY),
            part(Vec3::new(0.0, 0.0, 2.0), Quat::IDENTITY),
        ];
        for p in &mut parts {
            p.gfxobj_scale = Vec3::new(2.0, 2.0, 2.0);
        }
        skin.pose(&parts);
        let p = position(&skin, 0);
        assert!((p.z - 1.0).abs() < 1e-5, "{p:?}");
    }

    /// A coarser level poses only the vertices it draws, and the level chosen follows the
    /// distance at the object's size.
    #[test]
    fn a_coarser_level_poses_only_the_vertices_it_draws() {
        let bind = [
            Frame::default(),
            Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY),
        ];
        let t = template(
            &bind,
            vec![
                vertex(Vec3::new(0.0, 0.0, 1.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
                vertex(Vec3::new(1.0, 0.0, 1.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
            ],
        );
        let mut skin = ObjectSkin::new(t);
        let parts = [
            part(Vec3::ZERO, Quat::IDENTITY),
            part(Vec3::new(0.0, 0.0, 3.0), Quat::IDENTITY),
        ];
        skin.set_level(SkinnedBody::level_at(&skin.template.from, 60.0, 1.0));
        assert_eq!(skin.level, 1);
        skin.pose(&parts);
        assert!((position(&skin, 0).z - 3.0).abs() < 1e-5);
        assert_eq!(
            position(&skin, 1),
            Vec3::new(1.0, 0.0, 0.0),
            "the coarser level does not draw it, so it is left as the copy was made"
        );
        skin.set_level(SkinnedBody::level_at(&skin.template.from, 60.0, 2.0));
        assert_eq!(skin.level, 0, "twice the size is the fullest at 60 meters");
        assert!(
            !skin.fresh,
            "a level not drawn before is posed before it is drawn"
        );
    }

    /// A body drawn at a coarser level comes back to its fullest, and draws the fullest's meshes,
    /// as soon as the eye is back nearer than the first switch.
    #[test]
    fn a_body_returns_to_its_fullest_level_when_the_eye_comes_back_in() {
        let bind = [Frame::default()];
        let t = template(
            &bind,
            vec![
                vertex(Vec3::new(0.0, 0.0, 1.0), [0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
                vertex(Vec3::new(1.0, 0.0, 1.0), [0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
            ],
        );
        let mut skin = ObjectSkin::new(t);
        let parts = [part(Vec3::ZERO, Quat::IDENTITY)];
        let fullest = skin.meshes()[0].vertices.len();
        // At 0.45 of its size, the coarser level starts 22.5 meters out.
        for (distance, level) in [(10.0, 0), (80.0, 1), (500.0, 1), (20.0, 0), (5.0, 0)] {
            skin.set_level(SkinnedBody::level_at(&skin.template.from, distance, 0.45));
            assert_eq!(skin.level, level, "at {distance} m");
            skin.pose(&parts);
        }
        assert_eq!(skin.level, 0);
        assert_eq!(skin.meshes()[0].vertices.len(), fullest);
        assert!(skin.fresh, "posed at its fullest");
    }

    /// The skinned body wears the texture swaps of every part it bends, wherever the description
    /// laid them, the earlier part's winning; a part past the body's is not its own.
    #[test]
    fn a_skinned_body_wears_the_swaps_laid_on_any_of_its_parts() {
        use dereth_animation::parts::SurfaceOverrides;
        let swaps = |t: &[(u32, u32)]| SurfaceOverrides {
            texture_maps: t.iter().map(|&(a, b)| (DataId(a), DataId(b))).collect(),
            ..SurfaceOverrides::default()
        };
        let mut parts: Vec<_> = (0..4).map(|_| part(Vec3::ZERO, Quat::IDENTITY)).collect();
        assert_eq!(body_look(&parts, 3), None);
        parts[1].surface_overrides = Some(swaps(&[(0x0500_0001, 0x0500_0011)]));
        parts[2].surface_overrides = Some(swaps(&[
            (0x0500_0001, 0x0500_0099),
            (0x0500_0002, 0x0500_0012),
        ]));
        parts[3].surface_overrides = Some(swaps(&[(0x0500_0003, 0x0500_0013)]));
        assert_eq!(
            body_look(&parts, 3),
            Some(swaps(&[
                (0x0500_0001, 0x0500_0011),
                (0x0500_0002, 0x0500_0012)
            ]))
        );
    }

    /// A body out of the view, or past where anything is drawn, is not posed; one in it is.
    #[test]
    fn a_body_out_of_the_view_or_too_far_is_not_seen() {
        let reach = view_reach(1.0);
        let ahead = Vec3::new(0.0, 1.0, 0.0);
        let seen = |at: Vec3, r: f32| may_be_seen(Vec3::ZERO, ahead, reach, 100.0, at, r);
        assert!(seen(Vec3::new(0.0, 20.0, 0.0), 1.0));
        assert!(!seen(Vec3::new(0.0, -20.0, 0.0), 1.0), "behind");
        assert!(!seen(Vec3::new(0.0, 200.0, 0.0), 1.0), "too far");
        assert!(seen(Vec3::new(0.0, -0.5, 0.0), 1.0), "around the eye");
        assert!(
            seen(Vec3::new(30.0, 1.0, 0.0), 30.0),
            "off to the side but reaching in"
        );
        assert_eq!(pose_every(10.0), 1);
        assert_eq!(pose_every(60.0), 2);
        assert_eq!(pose_every(200.0), 4);
    }
}
