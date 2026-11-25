//! The objects drawn with an art pack's body ([`crate::assets::skin`]): each one's parts swapped
//! for its body as the object comes, and its body posed by its own clips each frame.
//!
//! **Swapped as it comes.** An object built on a setup the pack has a body for keeps every part,
//! in the frame's order, so its poses still match it: the body's anchor draws the body's mesh (at
//! every level the part has), each other part the body names draws nothing, and each part it
//! does not name draws as it is. The body's mesh is the object's own, named for it
//! ([`crate::assets::skin::skin_mesh`]), so each object's vertices can be written apart.
//!
//! **Posed each frame:** every vertex blended from its bones, as the clip for the object's
//! motion places them where the object stands, in the anchor's own space, and written over the
//! object's mesh before the frame is drawn, on the device ([`super::skin_pass`]) or here. Every
//! pass that draws the anchor draws the posed body.
//!
//! **Drawn on as a ghost.** An object that leaves while its body still dies is kept drawing until
//! its death ends, and the corpse laid in its place waits under it ([`ghosts`]).

use std::collections::BTreeMap;
use std::sync::Arc;

mod ghosts;
pub use ghosts::{CORPSE_REACH, CORPSE_WITHIN, GHOST_MOST};

use crate::assets::skin::{of_skin_mesh, skin_mesh, SkinModel};
use crate::assets::{
    kind, Appearance, AssetSource, CellPieceKey, Material, MaterialKey, Mesh, Texture, TextureKey,
};
use crate::scene::{BlockId, FrameDelta, ObjectKey, ObjectPose, PartLook, SceneDelta, Terrain};

/// The body for an object built on a setup whose look lays the palettes given, being who it is,
/// when there is one.
pub type SkinFor<'a> = dyn FnMut(u32, &[u32], crate::scene::Who) -> Option<Arc<SkinModel>> + 'a;

/// The body an object is drawn with: the pack's for it while the pack's bodies are drawn
/// (`characters`), else the one the files give its setup. Neither is drawn where a pack's models
/// have taken the parts it is drawn in place of ([`SkinModel::fits`]).
pub fn first_body<S: ?Sized>(
    src: &mut S,
    characters: bool,
    pack: impl FnOnce(&mut S) -> Option<Arc<SkinModel>>,
    files: impl FnOnce(&mut S) -> Option<Arc<SkinModel>>,
) -> Option<Arc<SkinModel>> {
    let found = if characters { pack(src) } else { None };
    found.or_else(|| files(src))
}

/// The objects drawn with a body.
#[derive(Debug)]
pub struct Skins {
    /// Each object drawn with a body: the body, the id of the object's mesh of it, and the setup
    /// the object is built on.
    objects: BTreeMap<ObjectKey, (Arc<SkinModel>, u32, u32)>,
    /// The next object's number in its mesh's id.
    next: u16,
    /// Vertices posed, every frame's summed.
    pub posed: u64,
    /// Each body's vertex rows as the last frame posed them, by its mesh's id.
    last: BTreeMap<u32, Vec<u8>>,
    /// What posing each object carries from frame to frame (what it last played).
    states: BTreeMap<ObjectKey, crate::assets::skin::PoseState>,
    /// Whether the motion pass is given the bodies' last places (on unless an instrument turns
    /// it off to show what it adds).
    pub motion: bool,
    /// Whether the bodies are posed on the device ([`super::skin_pass`]) rather than here (on
    /// unless an instrument turns it off to compare the two).
    pub gpu: bool,
    /// The bodies still dying when their objects left, drawn to the end of their deaths, and
    /// what is kept of each body dying now, should its object leave ([`ghosts`]).
    ghosts: ghosts::Ghosts,
    /// Whether such bodies are drawn on (on unless an instrument turns it off to show what it
    /// adds).
    pub ghosting: bool,
}

impl Default for Skins {
    fn default() -> Self {
        Self {
            objects: BTreeMap::new(),
            next: 0,
            posed: 0,
            last: BTreeMap::new(),
            states: BTreeMap::new(),
            motion: true,
            gpu: true,
            ghosts: ghosts::Ghosts::default(),
            ghosting: true,
        }
    }
}

impl Skins {
    /// `deltas` with every object built on a setup that `skin` has a body for drawn with it; an
    /// object whose parts draw anything other than the body names keeps its parts.
    pub fn swap(&mut self, mut deltas: Vec<SceneDelta>, skin: &mut SkinFor<'_>) -> Vec<SceneDelta> {
        for d in &mut deltas {
            let built = matches!(d, SceneDelta::ObjectBuilt(_));
            match d {
                SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => {
                    // The body for the palettes the object's look lays over its parts, when its
                    // setup has one for them.
                    // Those the description lays, and those its parts carry.
                    let mut palettes: Vec<u32> = o.palettes.clone();
                    for l in o.parts.iter().filter_map(|p| p.look.as_ref()) {
                        for id in l.palette.iter().chain(l.ranges.iter().map(|r| &r.0)) {
                            if !palettes.contains(id) {
                                palettes.push(*id);
                            }
                        }
                    }
                    let found = skin(o.setup, &palettes, o.who);
                    let Some(model) = found else {
                        self.objects.remove(&o.key);
                        continue;
                    };
                    // A body fits the parts that draw what it names; an object whose parts draw
                    // anything else there keeps its parts.
                    let levels: Vec<&[u32]> = o.parts.iter().map(|p| p.levels.as_slice()).collect();
                    if !model.fits(&levels) {
                        self.objects.remove(&o.key);
                        continue;
                    }
                    // Drawn with retail surfaces, the body wears the looks of the parts it names
                    // (their palettes and texture swaps), gathered on its anchor.
                    let look = model.rig.retail.then(|| {
                        let mut look: Option<PartLook> = None;
                        for (k, part) in o.parts.iter().enumerate() {
                            if !model.follows(k) {
                                continue;
                            }
                            let Some(l) = &part.look else {
                                continue;
                            };
                            match &mut look {
                                None => look = Some(l.clone()),
                                Some(all) => {
                                    for t in &l.textures {
                                        if !all.textures.iter().any(|(old, _)| *old == t.0) {
                                            all.textures.push(*t);
                                        }
                                    }
                                }
                            }
                        }
                        look
                    });
                    let id = match self.objects.get(&o.key) {
                        Some((m, id, _)) if m.skin == model.skin => *id,
                        _ => {
                            self.next = self.next.wrapping_add(1);
                            skin_mesh(model.skin, self.next)
                        }
                    };
                    let anchor = usize::from(model.anchor());
                    for (k, part) in o.parts.iter_mut().enumerate() {
                        if k == anchor {
                            part.levels = vec![id; part.levels.len().max(1)];
                            part.look = look.clone().flatten();
                        } else if model.follows(k) {
                            part.levels.clear();
                        }
                    }
                    if built {
                        self.ghosts.built(o.key);
                    }
                    self.objects.insert(o.key, (model, id, o.setup));
                }
                SceneDelta::ObjectRemoved(key) => {
                    if let Some((_, id, _)) = self.objects.remove(key) {
                        self.last.remove(&id);
                    }
                    self.states.remove(key);
                    self.ghosts.gone(*key);
                }
                _ => {}
            }
        }
        deltas
    }

    /// What each object's body last played of its own clips: the slot whose clip it played, how
    /// far through it it was drawn, and whether the speed its motion asks paced it; `None` for
    /// one standing at rest. For an instrument to read.
    #[must_use]
    pub fn playing(&self) -> Vec<(ObjectKey, Option<crate::assets::skin::Slot>, f32, bool)> {
        self.states
            .iter()
            .filter_map(|(key, state)| {
                self.objects.get(key)?;
                let (slot, share, paced) = state.playing.as_ref()?.drawn();
                Some((*key, slot, share, paced))
            })
            .collect()
    }

    /// Each object's body this frame, its mesh's id and its joints, for posing on the device; an
    /// object the frame does not pose is left as it was.
    pub fn joints(&mut self, frame: &FrameDelta) -> Vec<(Arc<SkinModel>, u32, Vec<glam::Mat4>)> {
        let mut out = Vec::with_capacity(self.objects.len());
        for pose in &frame.objects {
            let Some((model, id, joints)) = self.joints_of(pose, frame.clock) else {
                continue;
            };
            self.posed += model.mesh.vertices.len() as u64;
            out.push((model, id, joints));
        }
        out
    }

    /// The body `pose`'s object is drawn with, its mesh's id and its joints this frame.
    fn joints_of(
        &mut self,
        pose: &ObjectPose,
        clock: f64,
    ) -> Option<(Arc<SkinModel>, u32, Vec<glam::Mat4>)> {
        let (model, id, _) = self.objects.get(&pose.key)?;
        let (model, id) = (Arc::clone(model), *id);
        let parts: Vec<glam::Mat4> = pose.parts.iter().map(|p| p.transform).collect();
        let live = crate::assets::skin::Live {
            clock,
            state: self.states.entry(pose.key).or_default(),
            motion: pose.motion,
        };
        let joints = model.joints_live(&parts, Some(live))?;
        Some((model, id, joints))
    }

    /// Nothing an object drawn with a body holds is drawn: each held thing's parts in `frame` are
    /// hidden while its holder is drawn with a body.
    pub fn hide_held(&self, frame: &mut FrameDelta) {
        for o in &mut frame.objects {
            if o.held_by
                .is_some_and(|(holder, _)| self.objects.contains_key(&holder))
            {
                for p in &mut o.parts {
                    p.hidden = true;
                }
            }
        }
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
        self.ghosts = ghosts::Ghosts::default();
        for d in deltas {
            match d {
                SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => {
                    self.objects.remove(&o.key);
                }
                SceneDelta::ObjectRemoved(key) => {
                    if let Some((_, id, _)) = self.objects.remove(key) {
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
            .map(|(m, id, _)| (*id, Arc::clone(m)))
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
            let Some((model, id, joints)) = self.joints_of(pose, frame.clock) else {
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
            out.push((id, rows));
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
    fn splat(&mut self, key: u64) -> Option<crate::assets::terrain::Splat> {
        self.src.splat(key)
    }
    fn ground_colors(&mut self) -> Option<crate::assets::terrain::GroundColors> {
        self.src.ground_colors()
    }
    fn pebble_grounds(&mut self) -> crate::assets::terrain::PebbleGrounds {
        self.src.pebble_grounds()
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
    fn pack_skin(
        &mut self,
        setup: u32,
        palettes: &[u32],
        who: crate::scene::Who,
    ) -> Option<Arc<SkinModel>> {
        self.src.pack_skin(setup, palettes, who)
    }
    fn skinned_body(&mut self, setup: u32) -> Option<Arc<SkinModel>> {
        self.src.skinned_body(setup)
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
            palettes: Vec::new(),
            who: crate::scene::Who::default(),
            parts: vec![
                part(0x0100_0010),
                part(0x0100_0020),
                part(0x0100_0030),
                part(0x0100_0040),
            ],
        }
    }

    #[test]
    fn the_body_is_asked_for_with_who_an_object_is() {
        use crate::scene::Who;
        let mut skins = Skins::default();
        let b = body();
        let being = |key: u32, weenie: Option<u32>, launcher: bool| {
            SceneDelta::ObjectBuilt(Box::new(Object {
                key: ObjectKey::Server(key),
                who: Who { weenie, launcher },
                ..object(0x0200_0001)
            }))
        };
        // A source that keeps weenie 237, and whatever has held a launcher, as they are: those
        // objects keep their own parts.
        let out = skins.swap(
            vec![
                being(7, Some(237), false),
                being(8, Some(238), false),
                being(9, None, false),
                being(10, Some(238), true),
            ],
            &mut |_, _, who| (who.weenie != Some(237) && !who.launcher).then(|| Arc::clone(&b)),
        );
        let drawn = |d: &SceneDelta| match d {
            SceneDelta::ObjectBuilt(o) => {
                o.parts[1].levels.iter().any(|g| of_skin_mesh(*g).is_some())
            }
            _ => panic!("an object"),
        };
        assert_eq!(
            out.iter().map(drawn).collect::<Vec<_>>(),
            [false, true, true, false]
        );
        assert_eq!(skins.objects(), 2);
        // The same object changed once it has held a launcher: its own parts from then on.
        let changed = skins.swap(
            vec![SceneDelta::ObjectChanged(Box::new(Object {
                key: ObjectKey::Server(8),
                who: Who {
                    weenie: Some(238),
                    launcher: true,
                },
                ..object(0x0200_0001)
            }))],
            &mut |_, _, who| (!who.launcher).then(|| Arc::clone(&b)),
        );
        let SceneDelta::ObjectChanged(o) = &changed[0] else {
            panic!("an object");
        };
        assert_eq!(o.parts, object(0x0200_0001).parts);
        assert_eq!(skins.objects(), 1);
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
            &mut |setup, _, _| (setup == 0x0200_0001).then(|| Arc::clone(&b)),
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
            &mut |_, _, _| Some(Arc::clone(&b)),
        );
        let SceneDelta::ObjectChanged(o) = &again[0] else {
            panic!("an object");
        };
        assert_eq!(o.parts[1].levels[0], id);
        skins.swap(
            vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
            &mut |_, _, _| None,
        );
        assert_eq!(skins.objects(), 0);
    }

    #[test]
    fn an_object_s_body_is_asked_for_by_the_palettes_its_parts_looks_lay() {
        let mut skins = Skins::default();
        let b = body();
        let mut dyed = object(0x0200_0001);
        dyed.parts[1].look = Some(PartLook {
            textures: Vec::new(),
            palette: Some(0x0400_0001),
            ranges: vec![(0x0400_0F6F, 0, 8), (0x0400_0001, 8, 4)],
        });
        dyed.parts[2].look = Some(PartLook {
            textures: Vec::new(),
            palette: None,
            ranges: vec![(0x0400_0FC3, 0, 8)],
        });
        let mut asked: Vec<Vec<u32>> = Vec::new();
        skins.swap(
            vec![
                SceneDelta::ObjectBuilt(Box::new(dyed)),
                SceneDelta::ObjectBuilt(Box::new(Object {
                    key: ObjectKey::Server(8),
                    ..object(0x0200_0001)
                })),
            ],
            &mut |_, palettes, _| {
                asked.push(palettes.to_vec());
                Some(Arc::clone(&b))
            },
        );
        assert_eq!(
            asked,
            [vec![0x0400_0001, 0x0400_0F6F, 0x0400_0FC3], Vec::new()],
            "each palette once, in the parts' order; none for an undyed object"
        );
    }

    /// A description's palette that lands on none of the parts (each drawn against another base
    /// palette, as a copper golem's are) still chooses the body: the palettes the description lays
    /// come first, then the parts' own, each once.
    #[test]
    fn an_object_s_body_is_asked_for_by_its_descriptions_palettes_too() {
        let mut skins = Skins::default();
        let b = body();
        let mut copper = object(0x0200_07CA);
        copper.palettes = vec![0x0400_0F44];
        copper.parts[1].look = Some(PartLook {
            textures: vec![(0x0500_0001, 0x0500_0002)],
            palette: None,
            ranges: Vec::new(),
        });
        let mut pyreal = Object {
            key: ObjectKey::Server(8),
            ..object(0x0200_07CA)
        };
        pyreal.palettes = vec![0x0400_0F47, 0x0400_133B];
        pyreal.parts[1].look = Some(PartLook {
            textures: Vec::new(),
            palette: Some(0x0400_0F47),
            ranges: vec![(0x0400_133B, 0, 8)],
        });
        let mut asked: Vec<Vec<u32>> = Vec::new();
        skins.swap(
            vec![
                SceneDelta::ObjectBuilt(Box::new(copper)),
                SceneDelta::ObjectBuilt(Box::new(pyreal)),
            ],
            &mut |_, palettes, _| {
                asked.push(palettes.to_vec());
                Some(Arc::clone(&b))
            },
        );
        assert_eq!(
            asked,
            [vec![0x0400_0F44], vec![0x0400_0F47, 0x0400_133B]],
            "the description's palette though no part carries it; no palette twice"
        );
    }

    #[test]
    fn a_posed_body_is_written_as_its_meshs_vertex_rows() {
        let mut skins = Skins::default();
        let b = body();
        skins.swap(
            vec![SceneDelta::ObjectBuilt(Box::new(object(0x0200_0001)))],
            &mut |_, _, _| Some(Arc::clone(&b)),
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
                held_by: None,
                motion: None,
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
        // In the anchor's space, at rest however far part 2 is lifted: its vertex at 1 and the
        // shared one at a half; the texture coordinates and the white color carried.
        assert!((float(16 + 2) - 1.0).abs() < 1e-5);
        assert!((float(32 + 2) - 0.5).abs() < 1e-5);
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

    #[test]
    fn nothing_an_object_drawn_with_a_body_holds_is_drawn() {
        let pose = |transform| PartPose {
            transform,
            previous: transform,
            level: 0,
            hidden: false,
            material: None,
        };
        let frame = || FrameDelta {
            objects: vec![
                ObjectPose {
                    held_by: None,
                    motion: None,
                    key: ObjectKey::Server(7),
                    cell: 0,
                    parts: vec![pose(Mat4::IDENTITY); 4],
                },
                ObjectPose {
                    held_by: Some((ObjectKey::Server(7), 2)),
                    motion: None,
                    key: ObjectKey::Server(9),
                    cell: 0,
                    parts: vec![pose(Mat4::IDENTITY)],
                },
                ObjectPose {
                    held_by: Some((ObjectKey::Server(8), 2)),
                    motion: None,
                    key: ObjectKey::Server(10),
                    cell: 0,
                    parts: vec![pose(Mat4::IDENTITY)],
                },
            ],
            ..FrameDelta::default()
        };
        let mut skins = Skins::default();
        let b = body();
        skins.swap(
            vec![SceneDelta::ObjectBuilt(Box::new(object(0x0200_0001)))],
            &mut |_, _, _| Some(Arc::clone(&b)),
        );
        let mut f = frame();
        skins.hide_held(&mut f);
        assert!(f.objects[0].parts.iter().all(|p| !p.hidden), "the holder");
        assert!(f.objects[1].parts.iter().all(|p| p.hidden), "what it holds");
        assert!(
            f.objects[2].parts.iter().all(|p| !p.hidden),
            "what an object drawn with its parts holds"
        );
    }

    #[test]
    fn a_body_skips_an_object_whose_parts_draw_otherwise_and_wears_its_parts_looks_on_its_anchor() {
        use crate::assets::skin::Rig;
        use crate::scene::PartLook;
        let b = Arc::new((*body()).clone().with_rig(Rig {
            gfxobjs: vec![(1, 0x0100_0020), (2, 0x0100_0030)],
            retail: true,
            ..Rig::default()
        }));
        let look = |textures: Vec<(u32, u32)>| PartLook {
            textures,
            palette: Some(0x0400_007E),
            ranges: vec![(0x0F00_0013, 0, 24)],
        };
        let mut bare = object(0x0200_0001);
        bare.parts[1].look = Some(look(vec![(0x0500_0001, 0x0500_0002)]));
        bare.parts[2].look = Some(look(vec![(0x0500_0003, 0x0500_0004)]));
        let mut other = Object {
            key: ObjectKey::Server(8),
            ..object(0x0200_0001)
        };
        other.parts[2].levels = vec![0x0100_0099];
        let mut skins = Skins::default();
        let out = skins.swap(
            vec![
                SceneDelta::ObjectBuilt(Box::new(bare)),
                SceneDelta::ObjectBuilt(Box::new(other.clone())),
            ],
            &mut |_, _, _| Some(Arc::clone(&b)),
        );
        let SceneDelta::ObjectBuilt(o) = &out[0] else {
            panic!("an object");
        };
        assert!(of_skin_mesh(o.parts[1].levels[0]).is_some());
        assert_eq!(
            o.parts[1].look,
            Some(look(vec![
                (0x0500_0001, 0x0500_0002),
                (0x0500_0003, 0x0500_0004)
            ])),
            "the anchor wears both parts' texture swaps and the palette"
        );
        let SceneDelta::ObjectBuilt(d) = &out[1] else {
            panic!("an object");
        };
        assert_eq!(
            d.parts, other.parts,
            "an object drawing other parts keeps them"
        );
        assert_eq!(skins.objects(), 1);
    }

    /// Two objects of one setup drawn with the one body each wear their own look on it.
    #[test]
    fn two_objects_sharing_a_body_each_wear_their_own_look() {
        use crate::assets::skin::Rig;
        use crate::scene::PartLook;
        let b = Arc::new((*body()).clone().with_rig(Rig {
            gfxobjs: vec![(1, 0x0100_0020), (2, 0x0100_0030)],
            retail: true,
            by_parts: true,
            ..Rig::default()
        }));
        let look = |new: u32| PartLook {
            textures: vec![(0x0500_0001, new)],
            palette: None,
            ranges: Vec::new(),
        };
        let dressed = |key: u32, new: u32| {
            let mut o = Object {
                key: ObjectKey::Server(key),
                ..object(0x0200_0001)
            };
            o.parts[1].look = Some(look(new));
            o.parts[2].look = Some(look(new));
            SceneDelta::ObjectBuilt(Box::new(o))
        };
        let mut made = 0;
        let mut skins = Skins::default();
        let out = skins.swap(
            vec![dressed(7, 0x0500_0002), dressed(8, 0x0500_0003)],
            &mut |_, _, _| {
                made += 1;
                Some(Arc::clone(&b))
            },
        );
        let worn: Vec<_> = out
            .iter()
            .map(|d| {
                let SceneDelta::ObjectBuilt(o) = d else {
                    panic!("an object");
                };
                let anchor = usize::from(b.anchor());
                o.parts[anchor].look.clone()
            })
            .collect();
        assert_eq!(made, 2, "the body is asked for each object");
        assert_eq!(worn, [Some(look(0x0500_0002)), Some(look(0x0500_0003))]);
    }

    /// A pack's body for a setup is drawn before the one the files give it, while the pack's
    /// bodies are drawn; the files' is drawn otherwise; and where a pack's models have taken the
    /// parts a body is drawn in place of, the object keeps those models.
    #[test]
    fn a_packs_body_comes_first_then_the_files_and_a_packs_models_keep_their_parts() {
        use crate::assets::skin::Rig;
        let pack = body();
        let files = Arc::new((*body()).clone().with_rig(Rig {
            gfxobjs: vec![(1, 0x0100_0020), (2, 0x0100_0030)],
            retail: true,
            by_parts: true,
            ..Rig::default()
        }));
        let chosen = |characters: bool, has_pack: bool| {
            first_body(
                &mut (),
                characters,
                |()| has_pack.then(|| Arc::clone(&pack)),
                |()| Some(Arc::clone(&files)),
            )
            .map(|b| b.rig.by_parts)
        };
        assert_eq!(chosen(true, true), Some(false), "the pack's body first");
        assert_eq!(
            chosen(false, true),
            Some(true),
            "the files' with the pack's bodies off"
        );
        assert_eq!(
            chosen(true, false),
            Some(true),
            "the files' where the pack has none"
        );
        // A pack's model for the GfxObj part 2 draws takes that part: the files' body no
        // longer fits, and the object keeps its parts, the pack's model among them.
        let mut packed = object(0x0200_0001);
        let swapped = crate::assets::pack::swap_object_parts(&mut packed, &mut |g| {
            (g == 0x0100_0030).then(|| crate::assets::pack::PackModel {
                levels: vec![crate::assets::pack::PackLevel {
                    mesh: 0xF000_0001,
                    distance: f32::INFINITY,
                }],
                category: crate::assets::pack::Category::Rock,
                cover: false,
            })
        });
        assert_eq!(swapped, 1);
        let mut skins = Skins::default();
        let out = skins.swap(
            vec![
                SceneDelta::ObjectBuilt(Box::new(packed.clone())),
                SceneDelta::ObjectBuilt(Box::new(Object {
                    key: ObjectKey::Server(8),
                    ..object(0x0200_0001)
                })),
            ],
            &mut |_, _, _| Some(Arc::clone(&files)),
        );
        let SceneDelta::ObjectBuilt(kept) = &out[0] else {
            panic!("an object");
        };
        assert_eq!(kept.parts, packed.parts, "the pack's model is drawn");
        let SceneDelta::ObjectBuilt(bent) = &out[1] else {
            panic!("an object");
        };
        assert!(
            of_skin_mesh(bent.parts[1].levels[0]).is_some(),
            "the files' body"
        );
        assert!(bent.parts[2].levels.is_empty());
    }
}
