//! The objects drawn with an art pack's body ([`crate::assets::skin`]): each one's parts swapped
//! for its body as the object comes, and its body posed by the parts each frame.
//!
//! **Swapped as it comes.** An object built on a setup the pack has a body for keeps every part,
//! in the frame's order, so its poses still match it: the body's anchor draws the body's mesh (at
//! every level the part has), each other part the body follows draws nothing, and each part it
//! does not follow draws as it is. The body's mesh is the object's own, named for it
//! ([`crate::assets::skin::skin_mesh`]), so each object's vertices can be written apart.
//!
//! **Posed each frame:** every vertex blended from the parts' transforms, in the anchor's own
//! space, and written over the object's mesh before the frame is drawn, on the device
//! ([`super::skin_pass`]) or here. Every pass that draws the anchor draws the posed body.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::assets::skin::{of_skin_mesh, skin_mesh, SkinModel};
use crate::assets::{
    kind, Appearance, AssetSource, CellPieceKey, Material, MaterialKey, Mesh, Texture, TextureKey,
};
use crate::scene::{BlockId, FrameDelta, ObjectKey, PartLook, SceneDelta, Terrain};

/// The objects drawn with a body.
#[derive(Debug)]
pub struct Skins {
    /// Each object drawn with a body: the body, and the id of the object's mesh of it.
    objects: BTreeMap<ObjectKey, (Arc<SkinModel>, u32)>,
    /// The next object's number in its mesh's id.
    next: u16,
    /// Vertices posed, every frame's summed.
    pub posed: u64,
    /// Each body's vertex rows as the last frame posed them, by its mesh's id.
    last: BTreeMap<u32, Vec<u8>>,
    /// Whether the motion pass is given the bodies' last places (on unless an instrument turns
    /// it off to show what it adds).
    pub motion: bool,
    /// Whether the bodies are posed on the device ([`super::skin_pass`]) rather than here (on
    /// unless an instrument turns it off to compare the two).
    pub gpu: bool,
}

impl Default for Skins {
    fn default() -> Self {
        Self {
            objects: BTreeMap::new(),
            next: 0,
            posed: 0,
            last: BTreeMap::new(),
            motion: true,
            gpu: true,
        }
    }
}

impl Skins {
    /// `deltas` with every object built on a setup that `skin` has a body for drawn with it.
    pub fn swap(
        &mut self,
        mut deltas: Vec<SceneDelta>,
        skin: &mut dyn FnMut(u32) -> Option<Arc<SkinModel>>,
    ) -> Vec<SceneDelta> {
        for d in &mut deltas {
            match d {
                SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => {
                    let Some(model) = skin(o.setup) else {
                        self.objects.remove(&o.key);
                        continue;
                    };
                    let id = match self.objects.get(&o.key) {
                        Some((m, id)) if m.skin == model.skin => *id,
                        _ => {
                            self.next = self.next.wrapping_add(1);
                            skin_mesh(model.skin, self.next)
                        }
                    };
                    let anchor = usize::from(model.anchor());
                    for (k, part) in o.parts.iter_mut().enumerate() {
                        if k == anchor {
                            part.levels = vec![id; part.levels.len().max(1)];
                            part.look = None;
                        } else if model.follows(k) {
                            part.levels.clear();
                        }
                    }
                    self.objects.insert(o.key, (model, id));
                }
                SceneDelta::ObjectRemoved(key) => {
                    if let Some((_, id)) = self.objects.remove(key) {
                        self.last.remove(&id);
                    }
                }
                _ => {}
            }
        }
        deltas
    }

    /// Each object's body this frame, its mesh's id and its joints, for posing on the device; an
    /// object the frame does not pose is left as it was.
    pub fn joints(&mut self, frame: &FrameDelta) -> Vec<(Arc<SkinModel>, u32, Vec<glam::Mat4>)> {
        let mut out = Vec::with_capacity(self.objects.len());
        for pose in &frame.objects {
            let Some((model, id)) = self.objects.get(&pose.key) else {
                continue;
            };
            let parts: Vec<glam::Mat4> = pose.parts.iter().map(|p| p.transform).collect();
            let Some(joints) = model.joints(&parts) else {
                continue;
            };
            self.posed += model.mesh.vertices.len() as u64;
            out.push((Arc::clone(model), *id, joints));
        }
        out
    }

    /// The rows the body of mesh `id` was last posed with, `rows` kept as its newest: what the
    /// motion pass reads as each vertex's last place. A body posed for the first time was where it
    /// is.
    pub fn last_rows(&mut self, id: u32, rows: &[u8]) -> Vec<u8> {
        match self.last.insert(id, rows.to_vec()) {
            Some(was) if was.len() == rows.len() => was,
            _ => rows.to_vec(),
        }
    }

    /// Forget the objects `deltas` builds again or removes, while bodies are switched off.
    pub fn forget(&mut self, deltas: &[SceneDelta]) {
        for d in deltas {
            match d {
                SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => {
                    self.objects.remove(&o.key);
                }
                SceneDelta::ObjectRemoved(key) => {
                    if let Some((_, id)) = self.objects.remove(key) {
                        self.last.remove(&id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Each body drawn, by the id of its object's mesh.
    #[must_use]
    pub fn meshes(&self) -> BTreeMap<u32, Arc<SkinModel>> {
        self.objects
            .values()
            .map(|(m, id)| (*id, Arc::clone(m)))
            .collect()
    }

    /// How many objects are drawn with a body.
    #[must_use]
    pub fn objects(&self) -> usize {
        self.objects.len()
    }

    /// Each object's body posed by its parts in `frame`, as the vertex rows of its mesh (as the
    /// scene's vertex pool holds them), by the mesh's id. An object the frame does not pose is
    /// left as it was.
    pub fn pose(&mut self, frame: &FrameDelta) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::with_capacity(self.objects.len());
        for pose in &frame.objects {
            let Some((model, id)) = self.objects.get(&pose.key) else {
                continue;
            };
            let parts: Vec<glam::Mat4> = pose.parts.iter().map(|p| p.transform).collect();
            let Some(joints) = model.joints(&parts) else {
                continue;
            };
            let posed = model.posed(&joints);
            let mut rows = Vec::with_capacity(posed.len() * 64);
            for (k, (p, n, t)) in posed.iter().enumerate() {
                let uv = model.mesh.vertices[k].uv;
                let c = model.mesh.colors.get(k).copied().unwrap_or([1.0; 4]);
                for f in p.iter().chain(n).chain(&uv).chain(t).chain(&c) {
                    rows.extend_from_slice(&f.to_le_bytes());
                }
            }
            self.posed += posed.len() as u64;
            out.push((*id, rows));
        }
        out
    }
}

/// A source whose bodies' meshes, by their objects' ids, are the bodies at rest; everything
/// else is the source it wraps.
pub struct WithSkins<'a> {
    pub src: &'a mut dyn AssetSource,
    pub meshes: BTreeMap<u32, Arc<SkinModel>>,
}

impl std::fmt::Debug for WithSkins<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WithSkins")
            .field("meshes", &self.meshes.len())
            .finish_non_exhaustive()
    }
}

impl AssetSource for WithSkins<'_> {
    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        if of_skin_mesh(id).is_some() {
            return self.meshes.get(&id).map(|m| m.rest.clone());
        }
        self.src.gfxobj(id)
    }
    fn cell_piece(&mut self, key: CellPieceKey) -> Option<Mesh> {
        self.src.cell_piece(key)
    }
    fn appearance(&mut self, surface: u32, look: Option<&PartLook>) -> Appearance {
        self.src.appearance(surface, look)
    }
    fn material(&mut self, key: &MaterialKey, used: kind::Use) -> Material {
        self.src.material(key, used)
    }
    fn texture(&mut self, key: &TextureKey, clip_map: bool) -> Option<Texture> {
        self.src.texture(key, clip_map)
    }
    fn prepare(&mut self, key: &TextureKey, clip_map: bool) -> bool {
        self.src.prepare(key, clip_map)
    }
    fn terrain_cells(
        &mut self,
        block: BlockId,
        terrain: &Terrain,
    ) -> Vec<crate::assets::TerrainCell> {
        self.src.terrain_cells(block, terrain)
    }
    fn splat(&mut self, key: u32) -> Option<crate::assets::terrain::Splat> {
        self.src.splat(key)
    }
    fn ground_colors(&mut self) -> Option<[[f32; 4]; 32]> {
        self.src.ground_colors()
    }
    fn sky_objects(&mut self) -> Vec<u32> {
        self.src.sky_objects()
    }
    fn sky(&mut self, id: u32) -> Option<Mesh> {
        self.src.sky(id)
    }
    fn pack_model(&mut self, id: u32) -> Option<crate::assets::pack::PackModel> {
        self.src.pack_model(id)
    }
    fn pack_skin(&mut self, setup: u32) -> Option<Arc<SkinModel>> {
        self.src.pack_skin(setup)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use crate::assets::{Subset, SurfaceRef, Vertex};
    use crate::scene::{Object, ObjectPart, ObjectPose, PartPose};
    use glam::{Mat4, Vec3};

    fn body() -> Arc<SkinModel> {
        let v = |z: f32| Vertex {
            position: [0.0, 0.0, z],
            normal: [1.0, 0.0, 0.0],
            uv: [0.25, 0.75],
        };
        let mesh = Mesh {
            vertices: vec![v(0.0), v(1.0), v(0.5)],
            indices: vec![0, 1, 2],
            subsets: vec![Subset {
                first_index: 0,
                index_count: 3,
                surface: SurfaceRef::Surface(crate::assets::skin::skin_surface(0, 0)),
                two_sided: false,
                tiled: true,
            }],
            ..Mesh::default()
        };
        Arc::new(
            SkinModel::new(
                0,
                mesh,
                vec![[1, 0, 0, 0], [2, 0, 0, 0], [1, 2, 0, 0]],
                vec![
                    [1.0, 0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0, 0.0],
                    [0.5, 0.5, 0.0, 0.0],
                ],
                &[
                    (1, Mat4::IDENTITY),
                    (2, Mat4::from_translation(Vec3::new(0.0, 0.0, -1.0))),
                ],
            )
            .expect("a body"),
        )
    }

    fn object(setup: u32) -> Object {
        let part = |g: u32| ObjectPart {
            levels: vec![g, g + 1],
            look: None,
        };
        Object {
            key: ObjectKey::Server(7),
            setup,
            parts: vec![
                part(0x0100_0010),
                part(0x0100_0020),
                part(0x0100_0030),
                part(0x0100_0040),
            ],
        }
    }

    #[test]
    fn an_object_with_a_body_keeps_its_parts_the_anchor_drawing_the_body_at_every_level() {
        let mut skins = Skins::default();
        let b = body();
        let out = skins.swap(
            vec![
                SceneDelta::ObjectBuilt(Box::new(object(0x0200_0001))),
                SceneDelta::ObjectBuilt(Box::new(Object {
                    key: ObjectKey::Server(8),
                    ..object(0x0200_0002)
                })),
            ],
            &mut |setup| (setup == 0x0200_0001).then(|| Arc::clone(&b)),
        );
        let SceneDelta::ObjectBuilt(o) = &out[0] else {
            panic!("an object");
        };
        assert_eq!(o.parts.len(), 4, "the frame's poses still match the parts");
        assert_eq!(
            o.parts[0].levels,
            [0x0100_0010, 0x0100_0011],
            "part 0 is not followed"
        );
        let id = o.parts[1].levels[0];
        assert_eq!(o.parts[1].levels, [id, id]);
        assert!(of_skin_mesh(id).is_some());
        assert!(o.parts[2].levels.is_empty());
        assert_eq!(o.parts[3].levels, [0x0100_0040, 0x0100_0041]);
        let SceneDelta::ObjectBuilt(other) = &out[1] else {
            panic!("an object");
        };
        assert_eq!(
            other.parts,
            object(0x0200_0002).parts,
            "no body for its setup"
        );
        assert_eq!(skins.objects(), 1);
        // Built again, it keeps its mesh; removed, it is forgotten.
        let again = skins.swap(
            vec![SceneDelta::ObjectChanged(Box::new(object(0x0200_0001)))],
            &mut |_| Some(Arc::clone(&b)),
        );
        let SceneDelta::ObjectChanged(o) = &again[0] else {
            panic!("an object");
        };
        assert_eq!(o.parts[1].levels[0], id);
        skins.swap(
            vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
            &mut |_| None,
        );
        assert_eq!(skins.objects(), 0);
    }

    #[test]
    fn a_posed_body_is_written_as_its_meshs_vertex_rows() {
        let mut skins = Skins::default();
        let b = body();
        skins.swap(
            vec![SceneDelta::ObjectBuilt(Box::new(object(0x0200_0001)))],
            &mut |_| Some(Arc::clone(&b)),
        );
        let lift = |z: f32| Mat4::from_translation(Vec3::new(0.0, 0.0, z));
        let pose = |transform| PartPose {
            transform,
            previous: transform,
            level: 0,
            hidden: false,
            material: None,
        };
        let frame = FrameDelta {
            objects: vec![ObjectPose {
                key: ObjectKey::Server(7),
                cell: 0,
                parts: vec![
                    pose(lift(0.0)),
                    pose(lift(5.0)),
                    pose(lift(7.0)),
                    pose(lift(0.0)),
                ],
            }],
            ..FrameDelta::default()
        };
        let out = skins.pose(&frame);
        assert_eq!(out.len(), 1);
        let rows = &out[0].1;
        assert_eq!(rows.len(), 3 * 64);
        let float = |at: usize| f32::from_le_bytes(rows[at * 4..at * 4 + 4].try_into().expect("4"));
        // In the anchor's space: part 2 a metre higher than it rests, so its vertex at 2 and the
        // shared one at 1; the texture coordinates and the white color carried.
        assert!((float(16 + 2) - 2.0).abs() < 1e-5);
        assert!((float(32 + 2) - 1.0).abs() < 1e-5);
        assert_eq!([float(6), float(7)], [0.25, 0.75]);
        assert_eq!(float(15), 1.0);
        assert_eq!(skins.posed, 3);
    }

    #[test]
    fn a_bodys_last_rows_are_the_frame_befores_and_its_own_the_first_time() {
        let mut skins = Skins::default();
        assert_eq!(
            skins.last_rows(7, &[1, 2]),
            [1, 2],
            "first posed: where it is"
        );
        assert_eq!(skins.last_rows(7, &[3, 4]), [1, 2]);
        assert_eq!(skins.last_rows(7, &[5, 6]), [3, 4]);
        assert_eq!(skins.last_rows(8, &[9, 9]), [9, 9], "each body its own");
        assert_eq!(
            skins.last_rows(7, &[1, 2, 3]),
            [1, 2, 3],
            "a body whose mesh changed size starts again"
        );
    }
}
