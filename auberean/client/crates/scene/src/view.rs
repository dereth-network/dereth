//! The scenes the interface shows in its viewports: an interface scene's placed objects and its
//! light, the figures a screen stands in it (each with its look, its place and its mode), seen by a
//! camera; and the figure under the pointer.
//!
//! An interface scene keeps what the screen had drawn underneath (only depth is cleared). With no
//! light of its own it is lit by a dim ambient light and a sun from over the viewer's shoulder;
//! with lights, the sun is off. The figures animate in their mode. A pick casts a ray from the
//! camera through the pointer and takes the nearest figure whose drawn triangles it meets.

use std::collections::BTreeMap;

use auberean_animation::body::Body as Figure;
use auberean_animation::look::{Desc, Look};
use auberean_animation::CachedRecords;
use auberean_assets::property::{PropertyCollection, PropertyValue};
use auberean_assets::ui::UiScene;
use auberean_draw::body::BodyDrawer;
use auberean_draw::effects::ViewEffects;
use auberean_draw::particles::camera_axes;
use auberean_draw::scene::{light_view_draw, Camera, Frame, ObjectBatch, SceneBuilder};
use auberean_primitives::random::Random;
use auberean_primitives::{Quat, Vec3};
use auberean_world::color::Color;
use auberean_world::inside::{BlockLights, Light, Outside};
use auberean_world::math::{self, Mat4};
use auberean_world::place::{entity_parts, Part};
use auberean_world::records::WorldRecords;
use auberean_world::sky::DayLight;
use dereth_primitives::num::math as fm;

/// The light of an appearance scene the screen stands behind the camera, shining along its view.
const CAMERA_LIGHT: &str = "Light_1";
/// The entity kinds of an interface scene: a camera, a light.
pub const CAMERA_KIND: u32 = 0x4000_0005;
pub const LIGHT_KIND: u32 = 0x4000_0006;
/// The group a camera's properties are kept in.
pub const CAMERA_GROUP: u32 = 0x4000_000A;
/// A camera's properties: seen without perspective, vertical field of view (degrees), the height
/// seen without perspective, near and far distances.
pub const ORTHOGRAPHIC: u32 = 0x4000_001E;
pub const FOV: u32 = 0x4000_001F;
pub const VERTICAL_SCALE: u32 = 0x4000_0020;
pub const NEAR: u32 = 0x4000_0021;
pub const FAR: u32 = 0x4000_0022;

/// Where a viewport's camera stands and looks, and how it sees: through a perspective of its field
/// of view, or straight on (orthographic), `vertical_scale` metres from the picture's foot to its
/// top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewCamera {
    pub position: Vec3,
    /// Degrees clockwise from north (+y).
    pub heading: f32,
    /// Degrees above level.
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub orthographic: bool,
    pub vertical_scale: f32,
}

impl Default for ViewCamera {
    /// At the origin looking north, in perspective: 65° high, from 0.1 to 2000 m; a height of 1 m
    /// when seen straight on.
    // D2-10517, D2-10499
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            heading: 0.0,
            pitch: 0.0,
            fov_y: 65.0,
            near: 0.1,
            far: 2000.0,
            orthographic: false,
            vertical_scale: 1.0,
        }
    }
}

impl ViewCamera {
    /// A camera at `eye` looking at `target`, z up.
    #[must_use]
    pub fn looking(eye: Vec3, target: Vec3, fov_y: f32) -> Self {
        let d = math::sub(target, eye);
        let flat = (d.x * d.x + d.y * d.y).sqrt();
        Self {
            position: eye,
            heading: fm::atan2f(d.x, d.y).to_degrees().rem_euclid(360.0),
            pitch: fm::atan2f(d.z, flat).to_degrees(),
            fov_y,
            ..Self::default()
        }
    }

    /// Stands at `frame`'s place, looking along its +y.
    // D2-10501, D2-10520
    pub fn set_frame(&mut self, frame: &auberean_primitives::Frame) {
        let d = forward_of(frame.rotation);
        let at = Self::looking(frame.origin, math::add(frame.origin, d), self.fov_y);
        self.position = at.position;
        self.heading = at.heading;
        self.pitch = at.pitch;
    }

    /// Takes one of its properties; false for another property, or a value of another kind.
    // D2-10512
    pub fn apply_property(&mut self, id: u32, value: &PropertyValue) -> bool {
        match (id, value) {
            (ORTHOGRAPHIC, PropertyValue::Bool(b)) => self.orthographic = *b,
            (FOV, PropertyValue::Float(v)) => self.set_fov(*v),
            (VERTICAL_SCALE, PropertyValue::Float(v)) => self.set_vertical_scale(*v),
            (NEAR, PropertyValue::Float(v)) => self.near = *v,
            (FAR, PropertyValue::Float(v)) => self.far = *v,
            _ => return false,
        }
        true
    }

    /// Takes every property of its group in a collection; false when one was not taken.
    // D2-10518
    pub fn apply_properties(&mut self, c: &PropertyCollection) -> bool {
        let mut all = true;
        for g in c.groups.iter().filter(|g| g.group == CAMERA_GROUP) {
            for p in &g.properties {
                if !self.apply_property(p.id, &p.value) {
                    all = false;
                }
            }
        }
        all
    }

    /// Its vertical field of view, degrees.
    // D2-10514
    pub const fn set_fov(&mut self, fov: f32) {
        self.fov_y = fov;
    }

    /// The height it sees when seen straight on.
    // D2-10515
    pub const fn set_vertical_scale(&mut self, scale: f32) {
        self.vertical_scale = scale;
    }

    /// The camera the frame is drawn through: straight on, or in perspective.
    // D2-10593
    #[must_use]
    pub fn drawn(&self) -> Camera {
        Camera {
            position: self.position,
            heading: self.heading,
            pitch: self.pitch,
            fov_y: self.fov_y,
            near: self.near,
            far: self.far,
            orthographic: self.orthographic.then_some(self.vertical_scale),
        }
    }

    fn axes(&self) -> (Vec3, Vec3, Vec3) {
        let (h, p) = (self.heading.to_radians(), self.pitch.to_radians());
        let f = Vec3::new(
            fm::sinf(h) * fm::cosf(p),
            fm::cosf(h) * fm::cosf(p),
            fm::sinf(p),
        );
        let r = math::normalize(math::cross(f, Vec3::new(0.0, 0.0, 1.0)));
        let u = math::cross(r, f);
        (f, r, u)
    }
}

/// How a viewport's scene is lit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewLight {
    /// As the scene's own lights light it (or the default light when it has none).
    Scene,
    /// One light along the camera's view, tipped down by half: the appearance page's.
    Camera,
}

/// A figure a screen stands in its scene.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewFigure {
    /// Its description: the look it is put together with.
    pub look: Desc,
    pub origin: Vec3,
    /// Degrees clockwise from north.
    pub heading: f32,
    /// The mode it stands in (0: its table's first).
    pub mode: u32,
    /// It stands still in its first pose instead of animating.
    pub still: bool,
}

/// One viewport's scene, as a screen asks for it.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewScene {
    /// The viewport, window pixels: left, top, right, bottom.
    pub rect: [f32; 4],
    /// The interface scene record its placed objects and lights come from (0: none).
    pub scene: u32,
    pub camera: ViewCamera,
    pub light: ViewLight,
    pub figures: Vec<ViewFigure>,
    /// A pick asked at the pointer (window pixels).
    pub pick: Option<(f32, f32)>,
    /// Behaviors figures are to begin this frame: the figure's place in `figures`, the behavior.
    pub behaviors: Vec<(usize, u32)>,
    /// Drawn under the interface's picture behind the scenes, showing where that has no picture.
    pub under: bool,
}

/// A viewport's frame: where it goes in the window, and what it shows.
#[derive(Debug)]
pub struct ViewFrame {
    pub rect: [f32; 4],
    pub frame: Frame,
    /// Drawn under the interface's picture behind the scenes.
    pub under: bool,
}

struct Kept {
    figure: ViewFigure,
    body: Option<Figure>,
    scale: Vec3,
    /// The behavior it was asked to play, while it plays it.
    behavior: Option<u32>,
}

struct KeptScene {
    scene: u32,
    parts: Vec<Part>,
    lights: BlockLights,
    /// Its lights but the one a screen turns into a light from behind the camera.
    other_lights: BlockLights,
    fog: Option<(Color, f32, f32)>,
    effects: ViewEffects,
    figures: Vec<Kept>,
}

/// What a pick on a viewport's scene met: the figure under the pointer, and the kind its mesh
/// names itself there (a head, a torso, a leg...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewHit {
    pub figure: usize,
    pub mesh_type: u32,
}

/// The interface's scenes as they were last asked for, their figures put together and animated.
#[derive(Default)]
pub struct Views {
    kept: BTreeMap<usize, KeptScene>,
    /// The client's one generator of random numbers, which the scenes' effects draw from.
    pub dice: Random,
    /// How long the scenes have been drawn, the clock their effects run on.
    clock: f64,
    /// The figures still playing a behavior they were asked to: the scene, the figure.
    pub behaving: Vec<(usize, usize)>,
    /// What each scene's last pick met (scene index, what it met).
    pub picked: Vec<(usize, Option<ViewHit>)>,
}

impl std::fmt::Debug for Views {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Views")
            .field("scenes", &self.kept.len())
            .finish_non_exhaustive()
    }
}

fn forward_of(q: Quat) -> Vec3 {
    Mat4::from_parts(Vec3::ZERO, q, Vec3::new(1.0, 1.0, 1.0)).direction(Vec3::new(0.0, 1.0, 0.0))
}

/// A scene's camera by name: the entity's place, looking along its frame's +y, seen as its
/// properties say (in perspective, 65° high, from 0.1 to 2000 m, when it sets none).
// D2-10469, D2-10451, D2-10496, D2-10480
#[must_use]
pub fn scene_camera(scene: &UiScene, name: &str) -> Option<ViewCamera> {
    let e = scene
        .entities
        .iter()
        .find(|e| e.kind == Some(CAMERA_KIND) && e.name.as_deref() == Some(name))?;
    let mut c = ViewCamera::default();
    c.set_frame(&e.offset.map(|o| o.frame).unwrap_or_default());
    if let Some(p) = &e.properties {
        c.apply_properties(p);
    }
    Some(c)
}

/// The box around what a figure put together from `desc` draws, in its own frame: its parts'
/// meshes as they were read (no pose), times the figure's scale. `None` when the description puts
/// nothing together.
// D2-06121, D2-06120
#[must_use]
pub fn figure_box(records: &WorldRecords, desc: &Desc) -> Option<(Vec3, Vec3)> {
    let look = Look::of(records, desc).ok()?;
    let mut group = auberean_physics::parts::PartGroup::default();
    if !group.init_look(records, &look) {
        return None;
    }
    group.cache_default_render_box();
    let b = group.array.default_render_bounds;
    if b.min.x > b.max.x || b.min.z > b.max.z {
        return None;
    }
    let s = look.scale;
    Some((
        Vec3::new(b.min.x * s.x, b.min.y * s.y, b.min.z * s.z),
        Vec3::new(b.max.x * s.x, b.max.y * s.y, b.max.z * s.z),
    ))
}

/// The light of a scene with no lights of its own: ambient 0.1 and a white sun at half strength
/// travelling along (0.3, -0.9, -1.0).
// D2-10472
#[must_use]
pub fn default_light() -> DayLight {
    DayLight {
        ambient_brightness: 0.1,
        ambient_color: Color::WHITE,
        direct_brightness: 0.5,
        direct_color: Color::WHITE,
        light_direction: math::normalize(Vec3::new(0.3, -0.9, -1.0)),
        fog_min: 0.0,
        fog_max: 0.0,
        ..DayLight::DEFAULT
    }
}

impl Views {
    /// What viewport `i` draws: its scene `scene`, made afresh (its placed objects and lights)
    /// when the viewport is given another scene.
    // D2-11696
    fn keep(&mut self, records: &WorldRecords, i: usize, scene: u32) -> &mut KeptScene {
        let fresh = self.kept.get(&i).is_none_or(|k| k.scene != scene);
        if fresh {
            let mut parts = Vec::new();
            let mut lights = Vec::new();
            let mut other_lights = Vec::new();
            let mut effects = ViewEffects::default();
            let mut fog = None;
            if let Some(s) = (scene != 0)
                .then(|| records.get::<UiScene>(scene))
                .flatten()
            {
                if s.fog {
                    fog = Some((Color::from_argb(s.fog_color.0), s.fog_near, s.fog_far));
                }
                effects = ViewEffects::of_scene(records, &s.entities, &self.dice);
                for e in &s.entities {
                    if e.kind == Some(LIGHT_KIND) {
                        let at = e.offset.map(|o| o.frame.origin).unwrap_or(Vec3::ZERO);
                        let none = PropertyCollection::default();
                        let properties = e.properties.as_ref().unwrap_or(&none);
                        let (light, _, _) = Light::of_properties(0, at, properties);
                        if e.name.as_deref() != Some(CAMERA_LIGHT) {
                            other_lights.push(light.clone());
                        }
                        lights.push(light);
                    } else {
                        parts.extend(entity_parts(records, e));
                    }
                }
            }
            self.kept.insert(
                i,
                KeptScene {
                    scene,
                    parts,
                    lights: BlockLights::of_scene(lights),
                    other_lights: BlockLights::of_scene(other_lights),
                    fog,
                    effects,
                    figures: Vec::new(),
                },
            );
        }
        self.kept.get_mut(&i).expect("kept above")
    }

    /// The scenes of the viewports no longer shown (all but the first `shown`) let go of.
    // D2-11691
    fn release_gone(&mut self, shown: usize) {
        self.kept.retain(|i, _| *i < shown);
    }

    /// Each scene's frame, its figures put together (again when their look or place changed) and
    /// animated over `dt` seconds; each asked pick is answered in [`Views::picked`].
    // D2-10447, D2-10472, D2-10455, D2-10612
    #[allow(clippy::too_many_arguments)]
    pub fn frames(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        builder: &mut SceneBuilder<'_>,
        drawer: &mut BodyDrawer,
        scenes: &[ViewScene],
        dt: f32,
    ) -> Vec<ViewFrame> {
        self.release_gone(scenes.len());
        self.clock += f64::from(dt);
        let now = self.clock;
        let mut picked = Vec::new();
        let mut behaving = Vec::new();
        let mut out = Vec::new();
        for (i, v) in scenes.iter().enumerate() {
            let kept = self.keep(records, i, v.scene);
            kept.figures.truncate(v.figures.len());
            for (k, f) in v.figures.iter().enumerate() {
                let same = kept.figures.get(k).is_some_and(|x| x.figure == *f);
                if same {
                    continue;
                }
                // Another mode alone: the figure goes over to it by its table's graph.
                if let Some(x) = kept
                    .figures
                    .get_mut(k)
                    .filter(|x| x.figure.mode != f.mode && same_but_mode(&x.figure, f))
                {
                    if let Some(b) = &mut x.body {
                        b.do_mode(anims, 0, f.mode);
                    }
                    x.figure.mode = f.mode;
                    continue;
                }
                let look = Look::of(anims, &f.look).ok();
                let scale = look.as_ref().map_or(Vec3::new(1.0, 1.0, 1.0), |l| l.scale);
                let mut body = look.and_then(|l| Figure::from_look(anims, &l));
                if let Some(b) = &mut body {
                    if f.mode != 0
                        && b.table
                            .as_ref()
                            .is_some_and(|t| t.combos.iter().any(|(c, _)| c.mode == f.mode))
                    {
                        b.mode = f.mode;
                    }
                }
                let kept_figure = Kept {
                    figure: f.clone(),
                    body,
                    scale,
                    behavior: None,
                };
                if k < kept.figures.len() {
                    kept.figures[k] = kept_figure;
                } else {
                    kept.figures.push(kept_figure);
                }
            }
            for &(k, behavior) in &v.behaviors {
                if let Some(f) = kept.figures.get_mut(k) {
                    if let Some(b) = &mut f.body {
                        let params = auberean_animation::track::Params::behavior(behavior);
                        if b.do_behavior(anims, &params) {
                            f.behavior = Some(behavior);
                        }
                    }
                }
            }
            let mut bodies: Vec<(usize, Vec<ObjectBatch>)> = Vec::new();
            let mut stand: Vec<(Vec3, f32)> = Vec::new();
            for (k, f) in kept.figures.iter_mut().enumerate() {
                let Some(b) = &mut f.body else { continue };
                if !f.figure.still {
                    b.advance(anims, dt);
                }
                match f.behavior {
                    Some(id) if b.is_doing_behavior(id) => behaving.push((i, k)),
                    _ => f.behavior = None,
                }
                let rot = auberean_physics::body::heading_rotation(f.figure.heading);
                let place = Mat4::from_parts(
                    f.figure.origin,
                    Quat {
                        w: rot.w,
                        x: rot.x,
                        y: rot.y,
                        z: rot.z,
                    },
                    f.scale,
                );
                let poses = b.pose(anims);
                bodies.push((k, drawer.batches(records, b, &poses, &place)));
                stand.push((f.figure.origin, f.figure.heading));
            }
            if let Some(p) = v.pick {
                let hit = pick(&v.camera, v.rect, p, &bodies, &stand);
                picked.push((i, hit));
            }
            let (light, lit_by) = match v.light {
                ViewLight::Camera => {
                    let (f, _, _) = v.camera.axes();
                    let light = DayLight {
                        ambient_brightness: 0.1,
                        ambient_color: Color::WHITE,
                        direct_brightness: 0.6,
                        direct_color: Color::WHITE,
                        light_direction: math::normalize(math::add(f, Vec3::new(0.0, 0.0, -0.5))),
                        ..with_fog(kept.fog)
                    };
                    let others = !kept.other_lights.lights.is_empty();
                    (light, others.then_some((&kept.other_lights, true)))
                }
                ViewLight::Scene if kept.lights.lights.is_empty() => (default_light(), None),
                ViewLight::Scene => (with_fog(kept.fog), Some((&kept.lights, false))),
            };
            if let Some((lights, sun)) = lit_by {
                let outside = Outside::of(&light);
                let mut order = lights.scene();
                for ((_, batches), (at, _)) in bodies.iter_mut().zip(&stand) {
                    light_view_draw(batches, *at, lights, &mut order, &outside, sun);
                }
            }
            let camera = v.camera.drawn();
            let batches = bodies.into_iter().flat_map(|(_, b)| b).collect();
            let mut frame = builder.parts_frame(&kept.parts, batches, camera, light, lit_by);
            let outside = Outside::of(&light);
            let sun = Color::new(
                light.direct_color.r * light.direct_brightness,
                light.direct_color.g * light.direct_brightness,
                light.direct_color.b * light.direct_brightness,
                1.0,
            );
            frame.objects.extend(kept.effects.batches(
                records,
                now,
                sun,
                lit_by.map(|(lights, _)| (lights, &outside)),
                camera.position,
                camera_axes(camera.heading, camera.pitch),
            ));
            out.push(ViewFrame {
                rect: v.rect,
                frame,
                under: v.under,
            });
        }
        self.picked = picked;
        self.behaving = behaving;
        out
    }
}

/// Whether two figures differ in nothing but the mode they stand in.
fn same_but_mode(a: &ViewFigure, b: &ViewFigure) -> bool {
    a.look == b.look && a.origin == b.origin && a.heading == b.heading && a.still == b.still
}

/// The day of a scene lit by its own lights alone: no sun and no ambient light, its own fog.
fn with_fog(fog: Option<(Color, f32, f32)>) -> DayLight {
    let (fog_color, fog_min, fog_max) = fog.unwrap_or((Color::WHITE, 0.0, 0.0));
    DayLight {
        ambient_brightness: 0.0,
        ambient_color: Color::WHITE,
        direct_brightness: 0.0,
        direct_color: Color::WHITE,
        fog_min,
        fog_max,
        fog_color,
        ..DayLight::DEFAULT
    }
}

/// The figure a press of the pointer falls on: of the figures under it by their boxes, the nearest
/// whose drawn triangles the ray from the camera through the pointer meets, with the kind of the
/// mesh it meets there; failing any, the one whose box reaches least deep, on no mesh (kind 0). The
/// pointer is taken in pixels against the viewport's size: a panel that wants the viewport's own
/// pixels takes its corner off first. `stand` says where each of `bodies` stands and its heading.
// D2-10455, D2-10586, D2-10612, D2-10613, D2-06650, D2-10561
#[must_use]
pub fn pick(
    camera: &ViewCamera,
    rect: [f32; 4],
    pointer: (f32, f32),
    bodies: &[(usize, Vec<ObjectBatch>)],
    stand: &[(Vec3, f32)],
) -> Option<ViewHit> {
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let bounds = crate::seen::selection_bounds((w, h), pointer);
    let (cx, cy) = (bounds.0, bounds.1);
    let (f, r, u) = camera.axes();
    let half = camera.vertical_scale * 0.5;
    let t = fm::tanf((camera.fov_y * 0.5).to_radians());
    let aspect = w / h;
    let (from, dir) = if camera.orthographic {
        // Straight on: every ray runs along the view, from the near plane at the pointer.
        let across = math::add(
            math::scale(r, cx * half * aspect),
            math::scale(u, cy * half),
        );
        let from = math::add(
            camera.position,
            math::add(across, math::scale(f, camera.near)),
        );
        (from, f)
    } else {
        let dir = math::normalize(math::add(
            f,
            math::add(math::scale(r, cx * t * aspect), math::scale(u, cy * t)),
        ));
        (camera.position, dir)
    };
    let project = |p: Vec3| {
        let d = math::sub(p, camera.position);
        let along = math::dot(d, f);
        if camera.orthographic {
            (
                math::dot(d, r) / (half * aspect),
                math::dot(d, u) / half,
                along,
            )
        } else {
            (
                math::dot(d, r) / (along * t * aspect),
                math::dot(d, u) / (along * t),
                along,
            )
        }
    };
    let mut best: Option<(f32, ViewHit)> = None;
    let mut boxed: Option<(f32, usize)> = None;
    for ((k, batches), (origin, heading)) in bodies.iter().zip(stand) {
        let body = crate::seen::SeenBody {
            id: 0,
            origin: *origin,
            heading: *heading,
            bounds: None,
            faded: false,
        };
        let Some(depth) = crate::seen::render_box(&body, batches).and_then(|b| {
            crate::seen::box_under_pointer(body.corners(b), camera.near, project, bounds)
        }) else {
            continue;
        };
        let mut met = false;
        for b in batches {
            // Each batch (a part's material) tried as the client tries a part's triangle lists:
            // from either side, the nearest of all kept.
            if let Some(hit) = ray_intersects_tri_list(from, dir, RAY_LENGTH, triangles(b), true) {
                met = true;
                if best.is_none_or(|(bd, _)| hit.fraction < bd) {
                    let hit_of = ViewHit {
                        figure: *k,
                        mesh_type: b.mesh_type,
                    };
                    best = Some((hit.fraction, hit_of));
                }
            }
        }
        if !met && boxed.is_none_or(|(d, _)| depth < d) {
            boxed = Some((depth, *k));
        }
    }
    best.map(|(_, hit)| hit).or_else(|| {
        boxed.map(|(_, figure)| ViewHit {
            figure,
            mesh_type: 0,
        })
    })
}

/// A batch's triangles, each corner where the batch places it.
pub(crate) fn triangles(b: &ObjectBatch) -> impl Iterator<Item = [Vec3; 3]> + '_ {
    let at = |i: u16| {
        let v = b.position(usize::from(i))?;
        Some(b.transform.point(Vec3::new(v[0], v[1], v[2])))
    };
    b.indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(move |t| Some([at(t[0])?, at(t[1])?, at(t[2])?]))
}

/// How long the rays the picks cast are, meters: the client's own rays' length.
pub const RAY_LENGTH: f32 = 100_000.0;

/// What a ray meets first on a list of triangles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriHit {
    /// How far along the ray, as a share of its length.
    pub fraction: f32,
    /// The point met, its corners weighted as the client weights them (the first by the second
    /// edge's share, the second by the third edge's, the third by the rest).
    pub point: Vec3,
    /// The triangle's normal (its first edge across its second, made a unit).
    pub normal: Vec3,
}

/// Where a point lies from a plane, within 0.0002: in front, behind, or in it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Front,
    Behind,
    In,
}

fn side(d: f32) -> Side {
    if d > 0.0002 {
        Side::Front
    } else if d < -0.0002 {
        Side::Behind
    } else {
        Side::In
    }
}

/// The triangle of `tris` a ray from `from` along the unit `dir`, `length` long, meets nearest, as
/// the client finds it: a triangle whose normal is shorter than 0.0002 is skipped; the ray's two
/// ends are placed against the triangle's plane within 0.0002, and the triangle is tried only when
/// the ray reaches its plane (`two_sided`: from either side; else only from in front, or from in
/// the plane); a ray running within 0.0002 of the triangle's plane misses it; then the ray meets the
/// triangle where its edges' shares are 0 to 1 and sum to 1 or less.
// D2-09887
pub fn ray_intersects_tri_list(
    from: Vec3,
    dir: Vec3,
    length: f32,
    tris: impl IntoIterator<Item = [Vec3; 3]>,
    two_sided: bool,
) -> Option<TriHit> {
    let to = math::add(from, math::scale(dir, length));
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for [a, b, c] in tris {
        let e1 = math::sub(b, a);
        let e2 = math::sub(c, a);
        let n = math::cross(e1, e2);
        let len = math::dot(n, n).sqrt();
        if len < 0.0002 {
            continue;
        }
        let n = math::scale(n, 1.0 / len);
        let d = -math::dot(n, a);
        let (s0, s1) = (side(math::dot(n, from) + d), side(math::dot(n, to) + d));
        let tried = match (s0, s1) {
            (Side::Behind, _) if !two_sided => false,
            (Side::Behind, s) => s != Side::Behind,
            (Side::Front, Side::Front) => false,
            (Side::In, Side::Front) => true,
            (_, _) => true,
        };
        if !tried {
            continue;
        }
        let p = math::cross(dir, e2);
        let det = math::dot(p, e1);
        if det > -0.0002 && det < 0.0002 {
            continue;
        }
        let inv = 1.0 / det;
        let s = math::sub(from, a);
        let u = math::dot(s, p) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = math::cross(s, e1);
        let v = math::dot(q, dir) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = math::dot(q, e2) * inv;
        if best.is_none_or(|(bt, _, _)| t < bt) {
            let w = 1.0 - u - v;
            let point = math::add(
                math::add(math::scale(a, u), math::scale(b, v)),
                math::scale(c, w),
            );
            best = Some((t, point, n));
        }
    }
    best.map(|(t, point, normal)| TriHit {
        fraction: t / length,
        point,
        normal,
    })
}
