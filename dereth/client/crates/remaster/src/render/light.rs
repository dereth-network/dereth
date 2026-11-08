//! The frame's light inputs packed for the shaders, with no device: the frame's constants, the
//! light pools, and the light sets objects indoors are drawn with.
//!
//! The layout is the one `shaders/retail.slang` reads: rows of four 32-bit values.

use glam::{Mat4, Vec3};

use crate::scene::{Camera, FrameDelta, Light, LightKind, RetailLight};

/// How the light is worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightMode {
    /// Per vertex, by retail's rules, as Retail Look works it out.
    #[default]
    Vertex,
    /// Per pixel, by the same rules, from the frame's light inputs alone.
    Pixel,
}

/// Retail's byte-to-float factor, the literal its colors are scaled by.
const BYTE: f32 = 0.003_921_569;

/// How much nearer than its reach a light must be to an object's sphere to light it.
const REACH_EPSILON: f32 = 0.0002;

/// The light slots one draw takes at most.
pub const SLOTS: usize = 8;

/// Rows of four 32-bit values, as the shaders read a buffer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows(pub Vec<[u32; 4]>);

impl Rows {
    fn f(&mut self, v: [f32; 4]) {
        self.0.push(v.map(f32::to_bits));
    }

    fn u(&mut self, v: [u32; 4]) {
        self.0.push(v);
    }

    fn mat(&mut self, m: Mat4) {
        for c in m.to_cols_array_2d() {
            self.f(c);
        }
    }

    /// The rows as bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.0
            .iter()
            .flat_map(|r| r.iter().flat_map(|w| w.to_le_bytes()))
            .collect()
    }
}

/// The frame's constants: its camera, its sun, its two ambients, its fog, the screen ramp, the
/// light mode and the counts of its lights, and the target's extent.
#[must_use]
pub fn frame_rows(frame: &FrameDelta, gamma: f32, mode: LightMode, extent: (u32, u32)) -> Rows {
    frame_rows_seen(frame, frame.camera.projection, gamma, mode, extent)
}

/// How far beyond the world's far plane the sky is drawn: four times as far.
pub const SKY_FAR: f32 = 4.0;

/// The camera's projection with its far plane [`SKY_FAR`] times as far, as the sky is drawn.
#[must_use]
pub fn sky_projection(c: &Camera) -> Mat4 {
    let depth = |far: f32| far / (far - c.near);
    if c.far <= c.near {
        return c.projection;
    }
    let r = depth(c.far * SKY_FAR) / depth(c.far);
    let mut p = c.projection.transpose();
    p.z_axis *= r;
    p.transpose()
}

/// [`frame_rows`] seen through `projection`.
#[must_use]
pub fn frame_rows_seen(
    frame: &FrameDelta,
    projection: Mat4,
    gamma: f32,
    mode: LightMode,
    extent: (u32, u32),
) -> Rows {
    let l = &frame.light;
    let c = &frame.camera;
    let view_proj = projection * c.view;
    let mut r = Rows::default();
    r.mat(view_proj);
    r.mat(c.view);
    r.f([c.eye.x, c.eye.y, c.eye.z, 1.0]);
    r.f([l.sun.x, l.sun.y, l.sun.z, 0.0]);
    let s = l.sun_color.map(|b| f32::from(b) * BYTE);
    r.f([s[0], s[1], s[2], 0.0]);
    let a = l.ambient_color.map(|b| f32::from(b) * BYTE);
    r.f([
        a[0] * l.ambient_level,
        a[1] * l.ambient_level,
        a[2] * l.ambient_level,
        0.0,
    ]);
    r.f([
        l.object_ambient[0],
        l.object_ambient[1],
        l.object_ambient[2],
        0.0,
    ]);
    match l.fog {
        Some(f) => {
            r.f([f.start, f.end, 1.0, 0.0]);
            let fc = f.color.map(|b| f32::from(b) * BYTE);
            r.f([fc[0], fc[1], fc[2], 0.0]);
        }
        None => {
            r.f([0.0, 1.0, 0.0, 0.0]);
            r.f([0.0; 4]);
        }
    }
    #[allow(clippy::cast_precision_loss)] // a few hundred lights at most
    r.f([
        gamma,
        if mode == LightMode::Pixel { 1.0 } else { 0.0 },
        l.lights.len() as f32,
        l.statics.min(l.lights.len()) as f32,
    ]);
    r.mat(view_proj.inverse());
    #[allow(clippy::cast_precision_loss)] // a target's extent
    r.f([extent.0 as f32, extent.1 as f32, 0.0, 0.0]);
    // How the scene's colors are drawn: in display values, as retail draws them.
    r.f([0.0; 4]);
    r.f(lift_row(frame));
    r
}

/// Where the near ground gives way to the far, meters from the eye, and 1 while a far ground is
/// drawn: what stands on the ground is lifted onto it mixed the same way.
#[must_use]
pub fn lift_row(frame: &FrameDelta) -> [f32; 4] {
    frame.drawn.as_ref().map_or([0.0; 4], |g| {
        [
            g.knobs.morph.0,
            g.knobs.morph.1,
            f32::from(u8::from(g.knobs.far.is_some())),
            0.0,
        ]
    })
}

/// How far an instance standing at `origin` (the frame's space) is drawn above where it stands,
/// its `lifts` on the near ground and the far mixed by its distance from the eye: the
/// G-buffer's own arithmetic.
#[must_use]
pub fn lifted(frame: &FrameDelta, lifts: [f32; 2], origin: glam::Vec3) -> f32 {
    if lifts == [0.0; 2] {
        return 0.0;
    }
    let [start, end, far, _] = lift_row(frame);
    let t = if far > 0.5 {
        let d = origin.distance(frame.camera.eye);
        let end = end.max(start + 1e-3);
        let x = ((d - start) / (end - start)).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    } else {
        0.0
    };
    lifts[0] + (lifts[1] - lifts[0]) * t
}

/// The row of the frame's constants that says how the scene's colors are drawn.
pub const FRAME_POST: usize = 21;

/// `rows`, the frame's constants, drawn linear and unclamped when `linear`, in display values
/// otherwise.
#[must_use]
pub fn drawn_linear(mut rows: Rows, linear: bool) -> Rows {
    if let Some(r) = rows.0.get_mut(FRAME_POST) {
        r[0] = if linear { 1.0f32 } else { 0.0 }.to_bits();
    }
    rows
}

/// `rows` saying whether what stands outdoors takes an art pack's light.
#[must_use]
pub fn pack_lit(mut rows: Rows, lit: bool) -> Rows {
    if let Some(r) = rows.0.get_mut(FRAME_POST) {
        r[1] = if lit { 1.0f32 } else { 0.0 }.to_bits();
    }
    rows
}

/// `rows` saying whether the air is drawn over the land in place of retail's fog on the opaque world.
#[must_use]
pub fn air_drawn(mut rows: Rows, air: bool) -> Rows {
    if let Some(r) = rows.0.get_mut(FRAME_POST) {
        r[2] = if air { 1.0f32 } else { 0.0 }.to_bits();
    }
    rows
}

/// The light pools, four rows a light: where it is and its range, its diffuse and its type, its
/// color, and its intensity and falloff for the static burn.
#[must_use]
pub fn light_rows(light: &RetailLight) -> Rows {
    let mut r = Rows::default();
    for l in &light.lights {
        let (p, kind) = match l.kind {
            LightKind::Point => (l.position, 1.0),
            LightKind::Spot => (l.position, 2.0),
            LightKind::Directional => (l.direction, 3.0),
        };
        r.f([p.x, p.y, p.z, l.falloff * 1.5]);
        r.f([
            l.color[0] * l.intensity,
            l.color[1] * l.intensity,
            l.color[2] * l.intensity,
            kind,
        ]);
        r.f([l.color[0], l.color[1], l.color[2], 0.0]);
        r.f([l.intensity, l.falloff, 0.0, 0.0]);
    }
    r
}

/// One light set: at most eight lights, as indices into the pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LightSet {
    pub lights: [u16; SLOTS],
    pub count: u8,
}

impl LightSet {
    fn push(&mut self, i: usize) -> bool {
        if usize::from(self.count) >= SLOTS {
            return false;
        }
        self.lights[usize::from(self.count)] = u16::try_from(i).unwrap_or(u16::MAX);
        self.count += 1;
        true
    }
}

/// The light sets as rows, three a set: its count, then its eight lights four to a row.
#[must_use]
pub fn set_rows(sets: &[LightSet]) -> Rows {
    let mut r = Rows::default();
    for s in sets {
        let l = s.lights.map(u32::from);
        r.u([u32::from(s.count), 0, 0, 0]);
        r.u([l[0], l[1], l[2], l[3]]);
        r.u([l[4], l[5], l[6], l[7]]);
    }
    r
}

/// Whether light `l` reaches an object whose sphere is `centre` and `radius`: a point light whose
/// reach and the sphere meet, and any other light.
#[must_use]
pub fn reaches(l: &Light, centre: Vec3, radius: f32) -> bool {
    if l.kind != LightKind::Point {
        return true;
    }
    let d = l.position - centre;
    let reach = l.falloff + radius;
    d.length_squared() - reach * reach < REACH_EPSILON
}

/// The lights an object indoors is drawn with, as retail picks them: the dynamic lights in the
/// pools' order that reach its sphere, then the static ones, eight at most. `statics` is how many
/// of `lights` are static (they come first).
#[must_use]
pub fn object_light_set(lights: &[Light], statics: usize, centre: Vec3, radius: f32) -> LightSet {
    let mut set = LightSet::default();
    let statics = statics.min(lights.len());
    for (i, l) in lights.iter().enumerate().skip(statics) {
        if reaches(l, centre, radius) && !set.push(i) {
            return set;
        }
    }
    for (i, l) in lights.iter().enumerate().take(statics) {
        if reaches(l, centre, radius) && !set.push(i) {
            return set;
        }
    }
    set
}

/// The set a room's walls are drawn with: every dynamic light, eight at most.
#[must_use]
pub fn room_light_set(lights: &[Light], statics: usize) -> LightSet {
    let mut set = LightSet::default();
    for i in statics.min(lights.len())..lights.len() {
        if !set.push(i) {
            break;
        }
    }
    set
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn point(x: f32, falloff: f32) -> Light {
        Light {
            kind: LightKind::Point,
            cell: 0,
            position: Vec3::new(x, 0.0, 0.0),
            direction: Vec3::ZERO,
            color: [1.0, 0.5, 0.25],
            intensity: 2.0,
            falloff,
            cone_angle: 0.0,
        }
    }

    /// The dynamic lights come first, in order, each kept only when its reach meets the object's
    /// sphere; the static ones fill what is left of the eight slots.
    #[test]
    fn an_object_takes_the_dynamic_lights_that_reach_it_then_the_static_ones() {
        // Statics at x = 1..=5, dynamics at x = 10, 2, 30.
        let mut lights: Vec<Light> = (1..=5).map(|x| point(x as f32, 1.0)).collect();
        lights.extend([point(10.0, 1.0), point(2.0, 1.0), point(30.0, 1.0)]);
        // An object of radius 1 at the origin: a light reaches it within 2 metres.
        let set = object_light_set(&lights, 5, Vec3::ZERO, 1.0);
        assert_eq!(&set.lights[..usize::from(set.count)], &[6, 0, 1]);
        // A sphere big enough for every light takes the dynamics first, then the statics to
        // eight.
        let all = object_light_set(&lights, 5, Vec3::ZERO, 100.0);
        assert_eq!(all.count, 8);
        assert_eq!(&all.lights, &[5, 6, 7, 0, 1, 2, 3, 4]);
        let room = room_light_set(&lights, 5);
        assert_eq!(&room.lights[..usize::from(room.count)], &[5, 6, 7]);
    }

    /// A light set is three rows: its count, then its lights four to a row.
    #[test]
    fn a_light_set_is_three_rows() {
        let set = LightSet {
            lights: [3, 1, 4, 1, 5, 9, 2, 6],
            count: 7,
        };
        let r = set_rows(&[LightSet::default(), set]);
        assert_eq!(r.0.len(), 6);
        assert_eq!(r.0[3], [7, 0, 0, 0]);
        assert_eq!(r.0[4], [3, 1, 4, 1]);
        assert_eq!(r.0[5], [5, 9, 2, 6]);
        assert_eq!(r.bytes().len(), 6 * 16);
    }

    /// What stands on the ground is lifted by its near lift near the eye, by its far lift past
    /// where the ground has given way to the far, mixed between as the ground mixes them, and by
    /// its near lift alone while no far ground is drawn.
    #[test]
    fn a_lift_mixes_from_the_near_ground_to_the_far_with_distance() {
        let mut f = FrameDelta {
            drawn: Some(crate::scene::DrawnGround {
                knobs: crate::scene::DrawnKnobs {
                    morph: (300.0, 500.0),
                    far: Some((1.0, 1.5, 96.0, 30_000.0)),
                    ..crate::scene::DrawnKnobs::default()
                },
                ..crate::scene::DrawnGround::default()
            }),
            ..FrameDelta::default()
        };
        let l = [1.0, 3.0];
        let at = |x: f32| glam::Vec3::new(x, 0.0, 0.0);
        assert!((lifted(&f, l, at(100.0)) - 1.0).abs() < 1e-6);
        assert!((lifted(&f, l, at(600.0)) - 3.0).abs() < 1e-6);
        assert!((lifted(&f, l, at(400.0)) - 2.0).abs() < 1e-5);
        assert_eq!(lift_row(&f), [300.0, 500.0, 1.0, 0.0]);
        if let Some(g) = f.drawn.as_mut() {
            g.knobs.far = None;
        }
        assert!((lifted(&f, l, at(600.0)) - 1.0).abs() < 1e-6);
        assert_eq!(lifted(&f, [0.0; 2], at(600.0)), 0.0);
    }

    /// The frame's constants are 23 rows, with the ground's ambient its color's bytes times its
    /// level, the fog off when the frame has none, the colors in display values unless the frame
    /// is drawn linear, and nothing lifted while no ground is drawn over retail's.
    #[test]
    fn the_frames_constants_are_twenty_three_rows() {
        let mut f = FrameDelta::default();
        f.light.ambient_color = [255, 128, 0];
        f.light.ambient_level = 0.5;
        f.camera.projection = Mat4::IDENTITY;
        f.camera.view = Mat4::IDENTITY;
        let r = frame_rows(&f, 0.0, LightMode::Vertex, (640, 480));
        assert_eq!(r.0.len(), 23);
        assert_eq!(r.0[22], [0; 4], "no ground drawn over retail's");
        let row = |i: usize| r.0[i].map(f32::from_bits);
        assert!((row(11)[0] - 0.5).abs() < 1e-6);
        assert!((row(11)[1] - 128.0 * BYTE * 0.5).abs() < 1e-6);
        assert_eq!(row(13), [0.0, 1.0, 0.0, 0.0], "no fog");
        assert_eq!(row(20)[..2], [640.0, 480.0]);
        assert_eq!(row(FRAME_POST), [0.0; 4], "display values");
        let linear = drawn_linear(r.clone(), true);
        assert_eq!(linear.0[FRAME_POST][0], 1.0f32.to_bits());
        assert_eq!(drawn_linear(linear, false), r);
    }

    /// The sky is seen with the far plane four times as far: a point twice the world's far
    /// distance away is beyond the world's depth range but inside the sky's, and the near plane
    /// stays where it was.
    #[test]
    fn the_sky_sees_four_times_as_far() {
        let c = Camera {
            projection: Mat4::perspective_lh(1.0, 1.5, 0.5, 100.0),
            near: 0.5,
            far: 100.0,
            ..Camera::default()
        };
        let depth = |p: Mat4, z: f32| {
            let q = p * glam::Vec4::new(0.0, 0.0, z, 1.0);
            q.z / q.w
        };
        let sky = sky_projection(&c);
        assert!(depth(c.projection, 200.0) > 1.0);
        assert!(depth(sky, 200.0) < 1.0);
        assert!((depth(sky, 400.0) - 1.0).abs() < 1e-5);
        assert!(depth(sky, 0.5).abs() < 1e-6);
    }
}
