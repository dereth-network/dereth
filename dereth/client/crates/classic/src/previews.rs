//! The classic interface's two 3D previews: the paper doll in the inventory and the creation
//! wizard model, both assembled from the connected world data.
use crate::int::{i32_from, u32_from};
use crate::panels::{Appearance, Preview, PreviewKind};
use crate::runtime::Cx;
use dereth_animation::parts::ObjDesc;
use dereth_assets::Decode;
use dereth_client_contract::overlay::{PreviewLight, PreviewSpace};
use dereth_client_runtime::movement::to_anim_objdesc;
use dereth_client_runtime::present::Presentation;
use dereth_client_runtime::shell::Shell;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::{math, to_i32, to_i32_f64};
use dereth_primitives::{AssetSource, DataId, Vec3};
use dereth_scene::preview::{self, PaletteSetCache};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaperDollHit {
    pub object_index: usize,
    pub part_index: i32,
    pub equipment_mask: u32,
}
struct PickMesh {
    sphere: (Vec3, f32),
    polygons: Vec<dereth_client_runtime::pick_geometry::PickPolygon>,
}

/// What a preview was built from: the setup, the appearance and the environment setup (0 for
/// none).
type BuiltPreview = (DataId, ObjDesc, u32);

/// The paper doll's camera, looking straight ahead from in front of the doll.
const DOLL_CAMERA: Vec3 = Vec3::new(0.24, -2.7, 0.88);
/// The paper doll's and the creation model's focal lengths.
const DOLL_FOCAL: f32 = 0.075;
const CREATION_FOCAL: f32 = 0.1;

#[derive(Default)]
pub struct Previews {
    built: BTreeMap<PreviewSpace, BuiltPreview>,
    palettes: PaletteSetCache,
    pick_meshes: BTreeMap<DataId, Option<PickMesh>>,
    last_time: Option<f64>,
    chargen_heading: Option<f32>,
    chargen_zoom: Option<Zoom>,
    doll_lighting: DollLighting,
    doll_selected: Option<dereth_primitives::ObjectId>,
    /// The spaces this frame draws, and where.
    shown: Vec<(PreviewSpace, crate::widgets::Rect)>,
}
impl std::fmt::Debug for Previews {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Previews")
            .field("built", &self.built.keys().collect::<Vec<_>>())
            .field("last_time", &self.last_time)
            .finish_non_exhaustive()
    }
}
impl Previews {
    /// Forget what the spaces were built from and every record read for them, after a data
    /// patch: each shown space is built again from the files as they are now.
    pub fn forget_built(&mut self) {
        self.built.clear();
        self.palettes = PaletteSetCache::default();
        self.pick_meshes.clear();
    }
    /// A click on the paper doll: the object and part the ray from the doll's camera hits.
    /// Coordinates are in the same absolute UI space as `view.rect`.
    pub fn pick<P: Presentation + ?Sized>(
        &mut self,
        present: &mut P,
        store: &RetailDatStore,
        view: &Preview,
        x: i32,
        y: i32,
    ) -> Result<Option<PaperDollHit>, String> {
        use dereth_client_runtime::pick_geometry::{
            find_object, selection_ray, PickPart, PickPolygon,
        };
        if view.kind != PreviewKind::PaperDoll || !view.rect.contains(x, y) {
            return Ok(None);
        }
        let space = PreviewSpace::PaperDoll;
        // The doll and what it holds: the objects whose part arrays the space still has.
        let mut objects = Vec::new();
        for index in 0..3 {
            let Some(parts) = present.preview_part_array_mut(space, index) else {
                break;
            };
            let parts: Vec<_> = parts
                .parts
                .iter()
                .map(|p| (p.gfxobj_id, p.pos, p.gfxobj_scale))
                .collect();
            objects.push(parts);
        }
        for parts in &objects {
            for &(did, _, _) in parts {
                if self.pick_meshes.contains_key(&did) {
                    continue;
                }
                let bytes = store.read(did).map_err(|e| e.to_string())?;
                // In the layout of the files the part came from: the early portal's or the
                // world's own.
                let mesh = dereth_assets::geometry::GfxObj::decode_payload_in(
                    store.era_of(did),
                    did,
                    &bytes,
                )
                .map_err(|e| e.to_string())?;
                let pick = dereth_world_data::setup::drawing_sphere(&mesh).map(|sphere| {
                    let polygons = mesh
                        .polygons
                        .iter()
                        .filter_map(|p| {
                            let vertices: Option<Vec<_>> = p
                                .vertex_ids
                                .iter()
                                .map(|id| {
                                    mesh.vertex_array
                                        .vertices
                                        .get(*id as usize)
                                        .map(|v| v.position)
                                })
                                .collect();
                            let vertices = vertices?;
                            if vertices.len() < 3 {
                                return None;
                            }
                            let plane = dereth_physics::geom::Polygon::new(vertices.clone()).plane;
                            Some(PickPolygon {
                                plane,
                                vertices,
                                sides_type: p.sides_type,
                            })
                        })
                        .collect();
                    PickMesh {
                        sphere: (sphere.center, sphere.radius),
                        polygons,
                    }
                });
                self.pick_meshes.insert(did, pick);
            }
        }
        let mut picked = vec![];
        for (index, parts) in objects.iter().enumerate() {
            for (part_index, &(did, pos, scale)) in parts.iter().enumerate() {
                let Some(mesh) = self.pick_meshes.get(&did).and_then(Option::as_ref) else {
                    continue;
                };
                picked.push(PickPart {
                    // The shared world picker reserves id zero; preview indices do not.
                    physobj_id: dereth_primitives::ObjectId(u32_from(index) + 1),
                    physobj_index: i32_from(part_index),
                    frame: pos,
                    gfxobj_scale: scale,
                    drawing_sphere: mesh.sphere,
                    polygons: &mesh.polygons,
                    check_polys: true,
                });
            }
        }
        let camera = dereth_primitives::Frame {
            origin: DOLL_CAMERA,
            ..Default::default()
        };
        let direction = selection_ray(
            &camera,
            (x - view.rect.x) as f32,
            (y - view.rect.y) as f32,
            (view.rect.w as u32, view.rect.h as u32),
            fov_of_focal(DOLL_FOCAL, view.rect.h),
        );
        let (object, part_index) = find_object(camera.origin, direction, &picked).result();
        if object.0 == 0 {
            return Ok(None);
        }
        let object_index = object.0 as usize - 1;
        Ok(Some(PaperDollHit {
            object_index,
            part_index,
            equipment_mask: equipment_mask(object_index, part_index),
        }))
    }

    /// The spaces this frame draws, with their rectangles, for the overlay's preview markers.
    #[must_use]
    pub fn shown(&self) -> &[(PreviewSpace, crate::widgets::Rect)] {
        &self.shown
    }

    /// Build or update every preview `previews` names, outside the frame bracket.
    pub fn prepare<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        previews: &[Preview],
    ) -> Result<(), String> {
        self.shown.clear();
        if previews.is_empty() {
            return Ok(());
        }
        let now = cx.now();
        let dt = self
            .last_time
            .map_or(0., |last| (now - last).clamp(0., 0.25));
        self.last_time = Some(now);
        let world_store = Arc::clone(cx.store());
        let world_assets = Arc::clone(cx.anim_assets());
        for view in previews {
            let Some(id) = space_id(view.kind) else {
                continue;
            };
            self.shown.push((id, view.rect));
            let (store, assets) = ((*world_store).clone(), Arc::clone(&world_assets));
            let descriptor = if let Some(appearance) = &view.appearance {
                Some((
                    appearance.state.get_setup_id(&appearance.tables.chargen),
                    self.appearance(&store, appearance)?,
                    appearance
                        .tables
                        .chargen
                        .heritage_groups
                        .get(&appearance.state.heritage_group)
                        .map_or(0, |h| h.environment_setup.0),
                ))
            } else {
                view.object
                    .and_then(|object| cx.objects().presence(object))
                    .and_then(|p| Some((p.setup_id?, to_anim_objdesc(&p.objdesc), 0)))
            };
            let present = cx.present_mut();
            let Some(descriptor) = descriptor else {
                present.preview_remove_all_objects(id);
                self.built.remove(&id);
                continue;
            };
            present.preview_ensure(id, &assets);
            if self.built.get(&id) != Some(&descriptor) {
                present.preview_remove_all_objects(id);
                self.built.remove(&id);
                let index = present
                    .preview_add_object_dressed(id, &store, descriptor.0, Some(&descriptor.1))
                    .map_err(|e| e.to_string())?;
                if index.is_none() {
                    return Err(format!(
                        "Preview setup {:08X} did not load",
                        descriptor.0 .0
                    ));
                }
                if descriptor.2 != 0 {
                    present
                        .preview_add_object(id, &store, DataId(descriptor.2))
                        .map_err(|e| e.to_string())?;
                }
                let (animation, low, rate) = match view.kind {
                    PreviewKind::PaperDoll => (0x030003c0, 1, 0.),
                    _ => (
                        view.appearance
                            .as_ref()
                            .map_or(0x03000001, |a| a.animation.0),
                        -1,
                        30.,
                    ),
                };
                // The low bound compares unsigned: -1 chooses the final frame.
                let low = if low < 0 {
                    use dereth_animation::data::AnimAssets;
                    assets
                        .animation(DataId(animation))
                        .ok_or("Preview animation missing")?
                        .num_frames
                        .saturating_sub(1) as i32
                } else {
                    low
                };
                if !present.preview_set_sequence_animation(
                    id,
                    0,
                    DataId(animation),
                    true,
                    low,
                    rate,
                ) {
                    return Err(format!("Preview animation {animation:08X} did not load"));
                }
                self.built.insert(id, descriptor);
            }
            // The classic previews have a lens of their own: the doll's and the creation model's
            // focal lengths over the window's height, whatever the world camera's zoom.
            let focal = if view.kind == PreviewKind::PaperDoll {
                DOLL_FOCAL
            } else {
                CREATION_FOCAL
            };
            present.preview_set_fov(id, fov_of_focal(focal, view.rect.h));
            match view.kind {
                PreviewKind::PaperDoll => {
                    present.preview_set_camera_position(id, DOLL_CAMERA);
                    present.preview_set_camera_direction(id, Vec3::ZERO);
                    present.preview_set_heading(id, 0, f32::from_bits(0x433f5e2f));
                    // A distant light of intensity 2 from in front of the doll, on the camera's
                    // side.
                    present.preview_set_light(
                        id,
                        PreviewLight::Directional,
                        2.,
                        Vec3::new(0.3, 1.9, 0.65),
                    );
                }
                _ => {
                    let appearance = view.appearance.as_ref();
                    let initial = appearance.map_or(150., |a| a.heading_degrees);
                    let heading = self.chargen_heading.get_or_insert(initial);
                    let velocity = appearance.map_or(0., |a| a.rotation_velocity);
                    *heading = rotated_heading(
                        *heading,
                        velocity,
                        dt,
                        &dereth_animation::seq::Sequence::default(),
                    );
                    let face = appearance.is_none_or(|a| a.zoom_face);
                    let heritage = appearance.map_or(0, |a| a.state.heritage_group);
                    let zoom = self
                        .chargen_zoom
                        .get_or_insert_with(|| Zoom::new(heritage, face, now));
                    let camera = zoom.update(heritage, face, now);
                    present.preview_set_camera_position(id, camera);
                    present.preview_set_camera_direction_degrees(id, Vec3::new(-5., 0., 0.));
                    present.preview_set_heading(id, 0, *heading);
                    let (intensity, direction) = creation_light();
                    present.preview_set_light(id, PreviewLight::Directional, intensity, direction);
                }
            }
            present.preview_use_time(id, dt);
            if view.kind == PreviewKind::PaperDoll {
                let world = cx.model();
                if self.doll_selected != world.selected {
                    self.doll_selected = world.selected;
                    let mask = world.selected.map_or(0, |selected| {
                        if Some(selected) == world.player {
                            return 0x1ffffff;
                        }
                        let inventory = world.player.and_then(|p| world.tables.inventories.get(p));
                        preview::PaperDollSelectionLighting::selection_mask_from_object(
                            selected,
                            world.player,
                            &|loc| inventory.and_then(|i| i.upper_inv_obj(loc)),
                        )
                    });
                    self.doll_lighting.select(mask, now);
                }
                self.doll_lighting.update(now);
                if let Some(parts) = cx.present_mut().preview_part_array_mut(id, 0) {
                    parts.restore_lighting_internal();
                    if self.doll_lighting.mask != 0 {
                        let (luminosity, diffuse) = if self.doll_lighting.bright {
                            (0.99, 1.)
                        } else {
                            (0., 0.35)
                        };
                        preview::PaperDollSelectionLighting::apply(
                            self.doll_lighting.mask,
                            luminosity,
                            diffuse,
                            parts,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn appearance(
        &mut self,
        store: &RetailDatStore,
        appearance: &Appearance,
    ) -> Result<ObjDesc, String> {
        let setup = appearance.state.get_setup_id(&appearance.tables.chargen);
        Ok(preview::chargen_objdesc(
            &appearance.tables.chargen,
            &appearance.state,
            &appearance.tables.clothing,
            setup,
            &mut |id| self.palettes.palettes(store, id),
        )
        .0)
    }
}

/// The paper doll's turn: the preview's construction, not a selection, starts the flip counter.
#[derive(Default)]
struct DollLighting {
    mask: u32,
    bright: bool,
    time: f64,
    count: u32,
}
impl DollLighting {
    fn select(&mut self, mask: u32, now: f64) {
        let mask = mask & 0x1f07fff;
        if self.mask != mask {
            self.mask = mask;
            self.bright = false;
            self.time = now - 0.2;
        }
    }
    fn update(&mut self, now: f64) {
        if self.mask == 0 || now - self.time < 0.2 {
            return;
        }
        self.count = self.count.wrapping_add(1);
        if self.count < 3 {
            self.bright = !self.bright;
            self.time = now;
        } else {
            self.select(0, now);
        }
    }
}

/// The creation model turns at a fixed rate, quantised to whole animation frames.
fn rotated_heading(
    heading: f32,
    velocity: f32,
    dt: f64,
    sequence: &dereth_animation::seq::Sequence,
) -> f32 {
    if velocity == 0. {
        return heading;
    }
    let mut sequence = sequence.clone();
    let mut frame = dereth_primitives::Frame::default();
    dereth_animation::frame::set_heading(&mut frame, heading);
    sequence.set_omega(Vec3::new(0., 0., velocity));
    sequence.update(dt, Some(&mut frame), &mut vec![]);
    dereth_animation::frame::get_heading(&frame)
}

/// The creation preview's camera moves between views over 0.6 seconds, eased by the interface's
/// fixed-point easing table.
struct Zoom {
    heritage: u32,
    face: bool,
    start: [f64; 3],
    current: [f64; 3],
    began: f64,
}
impl Zoom {
    fn target(heritage: u32, face: bool) -> [f64; 3] {
        dereth_presentation::creation::camera(
            heritage,
            face,
            dereth_presentation::DisplayVariant::Classic,
        )
        .map(f64::from)
    }
    fn new(heritage: u32, face: bool, now: f64) -> Self {
        let p = Self::target(heritage, face);
        Self {
            heritage,
            face,
            start: p,
            current: p,
            began: now,
        }
    }
    // The zoom runs in `f64` and the camera takes `f32`; seconds and metres narrow by rounding.
    #[allow(clippy::cast_possible_truncation)]
    fn update(&mut self, heritage: u32, face: bool, now: f64) -> Vec3 {
        if face != self.face || heritage != self.heritage {
            self.heritage = heritage;
            self.start = self.current;
            self.face = face;
            self.began = now;
        }
        let target = Self::target(heritage, face);
        let level = animation_level(((now - self.began) / 0.6) as f32);
        for ((current, target), start) in self.current.iter_mut().zip(target).zip(self.start) {
            let units = to_i32_f64((target - start) * f64::from(level) * 10000.);
            *current = start + f64::from(units >> 10) * 0.0001;
        }
        Vec3::new(
            self.current[0] as f32,
            self.current[1] as f32,
            self.current[2] as f32,
        )
    }
}
fn animation_level(progress: f32) -> i32 {
    static TABLE: std::sync::OnceLock<[i32; 100]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        // The classic interface's own value of pi, kept so its table matches.
        #[allow(clippy::approx_constant)]
        let raw: [i32; 100] = std::array::from_fn(|i| {
            to_i32_f64(math::sin(i as f64 * 3.141592 * 0.010101010101010102) * 1024.)
        });
        let total: i32 = raw.iter().sum();
        let mut sum = 0;
        std::array::from_fn(|i| {
            sum += raw[i];
            sum * 1024 / total
        })
    });
    table[usize::try_from(to_i32(progress.clamp(0., 1.) * 99.)).unwrap_or(0)]
}

fn creation_light() -> (f32, Vec3) {
    (2.0, Vec3::new(0.3, 1.9, 0.65))
}

fn space_id(kind: PreviewKind) -> Option<PreviewSpace> {
    match kind {
        PreviewKind::CharGen => Some(PreviewSpace::CharGen),
        PreviewKind::PaperDoll => Some(PreviewSpace::PaperDoll),
        PreviewKind::Examine => None,
    }
}

/// The body parts a paper-doll click can hit; held objects occupy their own preview indices.
pub fn equipment_mask(object_index: usize, part_index: i32) -> u32 {
    match object_index {
        1 => dereth_rules::slots::loc::WEAPON_READY_SLOT,
        2 => 0x200000,
        _ => match part_index {
            0 => 0x404,
            1 | 5 => 0x2040,
            2 | 6 => 0x4080,
            3 | 4 | 7 | 8 => 0x100,
            9 => 0x202,
            10 | 13 => 0x808,
            11 | 14 => 0x1010,
            12 | 15 => 0x20,
            16 => 1,
            _ => 0,
        },
    }
}

// Focal length = ((width - 1) / 2 / 4000) / tan(horizontal_fov / 2).
// Convert the equivalent vertical projection to the shared normalized distance.
fn normalized_focal(focal: f32, height: i32) -> f32 {
    8000. * focal / (height - 1).max(1) as f32
}

/// The vertical field of view a focal length gives a window `height` pixels high.
fn fov_of_focal(focal: f32, height: i32) -> f32 {
    2. * math::atanf(1. / normalized_focal(focal, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Behaviour: chargen.classic.preview-light-faces-camera
    #[test]
    fn creation_light_faces_the_preview_camera() {
        let (intensity, direction) = creation_light();
        let camera_normal = Vec3::new(0., -1., 0.);
        let facing_light = -direction.dot(camera_normal) / direction.magnitude();
        assert!(facing_light * intensity > 1.5);
        assert!(direction.z > 0., "the light falls from above");
    }
    #[test]
    fn equipment_blink_follows_the_legacy_counter_without_inventing_a_reset() {
        let mut state = DollLighting::default();
        state.select(0x1ffffff, 0.);
        assert_eq!(state.mask, 0x1f07fff);
        state.update(0.);
        assert!(state.bright);
        assert_eq!(state.count, 1);
        state.update(0.19);
        assert!(state.bright);
        state.update(0.2);
        assert!(!state.bright);
        assert_eq!(state.count, 2);
        state.update(0.4);
        assert_eq!(state.mask, 0);
        assert_eq!(state.count, 3);
        state.select(1, 1.);
        state.update(1.01);
        assert_eq!(state.mask, 0);
        assert_eq!(state.count, 4);
    }
    #[test]
    fn camera_zoom_eases_and_reverses_from_the_current_position() {
        assert_eq!(animation_level(-1.), 0);
        assert_eq!(animation_level(1.), 1024);
        assert!((490..=520).contains(&animation_level(0.5)));
        let mut zoom = Zoom::new(1, true, 0.);
        assert_eq!(zoom.update(1, false, 0.), Vec3::new(0., -0.55, 1.65));
        let middle = zoom.update(1, false, 0.3);
        assert!(middle.y < -1.2 && middle.y > -1.5);
        assert_eq!(zoom.update(1, true, 0.3), middle);
        let face = zoom.update(1, true, 0.9);
        assert!((face.y + 0.55).abs() < 0.00011);
        assert!((face.z - 1.65).abs() < 0.00011);
        assert_eq!(zoom.update(1, true, 2.), face);
    }
    #[test]
    #[allow(clippy::approx_constant)] // the interface's own two pi, as the creation screen turns
    fn stopped_rotation_preserves_heading_and_positive_omega_turns_counterclockwise() {
        let seq = dereth_animation::seq::Sequence::default();
        assert_eq!(rotated_heading(231., 0., 100., &seq), 231.);
        let moved = rotated_heading(150., 6.283_f32 / 2.5, 0.25, &seq);
        assert!((moved - 114.).abs() < 0.01);
        assert!((rotated_heading(moved, -6.283_f32 / 2.5, 0.25, &seq) - 150.).abs() < 0.01);
    }
    /// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
    #[test]
    fn creation_camera_frames_later_bodies_without_changing_ordinary_classic_zoom() {
        let mut zoom = Zoom::new(12, false, 0.);
        assert_eq!(zoom.update(12, false, 0.), Vec3::new(0., -3.8, 1.15));
        zoom.update(13, true, 1.);
        let end = zoom.update(13, true, 2.);
        assert!((end.y + 3.05).abs() < 0.001 && (end.z - 2.75).abs() < 0.001);
        assert_eq!(
            Zoom::target(1, false),
            [0., f64::from(-2.2f32), f64::from(1.1f32)]
        );
    }
    #[test]
    fn classic_focal_distance_includes_pixel_scale_and_viewport_height() {
        assert!((normalized_focal(0.1, 424) - 800. / 423.).abs() < 1e-6);
        assert!((normalized_focal(0.075, 215) - 600. / 214.).abs() < 1e-6);
    }
    #[test]
    fn picked_parts_use_coverage_masks_and_held_object_indices() {
        assert_eq!(equipment_mask(0, 16), 1);
        assert_eq!(equipment_mask(0, 0), 0x404);
        assert_eq!(equipment_mask(0, 12), equipment_mask(0, 15));
        assert_eq!(equipment_mask(1, 16), 0x03500000);
        assert_eq!(equipment_mask(2, 0), 0x200000);
        assert_eq!(equipment_mask(0, -1), 0);
    }
}
