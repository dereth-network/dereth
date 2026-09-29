//! The light pools, the 8-light cap and `calc_point_light`.
//!
//! The light insert, the static and dynamic adds, the sunlight-set switch, the object and
//! environment-cell minimisers, the object-light removal, the active-light reset, add and
//! enable steps, the degrade-level setter, the point-light calculation and the conversion of a
//! light into local space.
//!
//! The light caps, attenuation and ordering below are observable rendering behavior.
//!
//! `calc_point_light`'s attenuation is
//! `denom = (d² > 1) ? d²·dist : dist`, i.e. **1/d inside one unit and 1/d³ beyond it** — a sharp
//! falloff, and the opposite of what "inverse cube near the light" would suggest. It also adds a
//! `+ dist·0.5` wrap term so a surface facing away from a nearby light still receives half
//! intensity, and clamps each channel at the light's own colour.

use dereth_primitives::{CellId, Frame, Vec3};

use crate::math::{l2g, V3};
use crate::narrow::{i32_of, u32_of};

/// Light type. Every one of the 608 shipped lights is type 0, so only the
/// point branch is ever exercised — but all three are implemented as documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightType {
    #[default]
    Point = 0,
    Directional = 1,
    Spot = 2,
}

/// A dat-authored light description.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightInfo {
    pub light_type: LightType,
    /// Position plus orientation, relative to the owning setup or cell.
    pub offset: Frame,
    /// Filled by the per-frame light update: the light in the viewer's space.
    pub viewerspace_location: Vec3,
    /// Floats 0..1.
    pub color: [f32; 3],
    pub intensity: f32,
    /// Range in world units.
    pub falloff: f32,
    /// Spot only, radians.
    pub cone_angle: f32,
}

impl Default for LightInfo {
    fn default() -> Self {
        Self {
            light_type: LightType::Point,
            offset: Frame::default(),
            viewerspace_location: Vec3::ZERO,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            falloff: 10.0,
            cone_angle: 0.0,
        }
    }
}

/// One light-pool entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderLight {
    pub info: LightInfo,
    pub cell_id: CellId,
    /// Squared distance **from the player**, not the camera.
    pub distance_sq: f32,
    /// The pool index plus the pool's index base; an identity, not a hardware slot.
    pub d3d_light_index: u32,
    /// The hardware-light record built from the combined,
    /// colour-quantised `info`.
    pub d3d: D3dLight,
}

/// The `1.5x` range multiplier applies in `config_hardware_light`. D3D's linear
/// attenuation reaches zero only at infinity, so the client extends the cut-off 50 % past the
/// authored falloff. The multiplier is a fixed client constant.
pub const D3D_RANGE_MULTIPLIER: f32 = 1.5;

/// Multiplier applied to falloff inside `calc_point_light`.
///
/// Read from the client rather than guessed: the constant is
/// **1.3**, with bit pattern `0x3FA66666`.
pub const CALC_POINT_LIGHT_FALLOFF_MULTIPLIER: f32 = 1.3;

/// Ambient multiplier used while updating lights; fixed at `1.0`.
pub const AMBIENT_MULTIPLIER: f32 = 1.0;

/// The epsilon used by the per-object light reach test.
pub const LIGHT_REACH_EPSILON: f32 = 0.0002;

/// There are exactly eight `D3DLIGHT9` slots.
pub const HARDWARE_LIGHT_SLOTS: usize = 8;

/// Light-pool caps as a function of `deg_mul`:
/// `max_static = int(40 + 20*d)`, `max_dynamic = int(7 + 2*d)` for `d >= 0` and `int(7 + 3*d)` for
/// `d < 0`.
#[must_use]
pub fn pool_caps(deg_mul: f32) -> (usize, usize) {
    let d = deg_mul.clamp(-1.0, 1.0);
    let statics = dereth_primitives::num::to_i32(40.0 + 20.0 * d).max(0);
    let dynamics = if d >= 0.0 {
        dereth_primitives::num::to_i32(7.0 + 2.0 * d)
    } else {
        dereth_primitives::num::to_i32(7.0 + 3.0 * d)
    }
    .max(0);
    (statics as usize, dynamics as usize)
}

/// The two pools, each kept **sorted by squared distance from the player**, ascending.
#[derive(Debug, Default, Clone)]
pub struct LightPools {
    pub statics: Vec<RenderLight>,
    pub dynamics: Vec<RenderLight>,
    pub max_static: usize,
    pub max_dynamic: usize,
}

impl LightPools {
    /// Build empty pools with the caps `deg_mul` implies.
    #[must_use]
    pub fn new(deg_mul: f32) -> Self {
        let (s, d) = pool_caps(deg_mul);
        Self {
            statics: Vec::new(),
            dynamics: Vec::new(),
            max_static: s,
            max_dynamic: d,
        }
    }

    /// Insert into a sorted pool, dropping the light when the
    /// pool is full and the new light is farther than everything already held.
    ///
    /// `distance_sq` is measured from the **player**; only point lights get a distance at all
    /// (a directional light's `d2` stays 0, so it sorts first).
    fn insert(pool: &mut Vec<RenderLight>, cap: usize, l: RenderLight, index_base: u32) {
        let i = pool.partition_point(|e| e.distance_sq <= l.distance_sq);
        if pool.len() == cap && i == cap {
            return; // farther than everything already held
        }
        if pool.len() == cap {
            pool.pop(); // the farthest entry is recycled
        }
        pool.insert(i, l);
        for (k, e) in pool.iter_mut().enumerate() {
            // LINT-OK: index arithmetic; the pool caps are at most 60.
            e.d3d_light_index = u32_of(k) + index_base;
        }
    }

    /// Add a static light with `cap = max_static_lights`,
    /// index base `max_dynamic_lights + 1`.
    pub fn add_static(&mut self, info: LightInfo, cell_id: CellId, distance_sq: f32) {
        // LINT-OK: index arithmetic; the pool caps are at most 60.
        let base = u32_of(self.max_dynamic) + 1;
        let info = quantise_colour(info);
        let d3d = config_hardware_light(&info);
        Self::insert(
            &mut self.statics,
            self.max_static,
            RenderLight {
                info,
                cell_id,
                distance_sq,
                d3d_light_index: 0,
                d3d,
            },
            base,
        );
    }

    /// Add a dynamic light with `cap = max_dynamic_lights`, index base 1.
    /// Index 0 is reserved for the sunlight.
    pub fn add_dynamic(&mut self, info: LightInfo, cell_id: CellId, distance_sq: f32) {
        let info = quantise_colour(info);
        let d3d = config_hardware_light(&info);
        Self::insert(
            &mut self.dynamics,
            self.max_dynamic,
            RenderLight {
                info,
                cell_id,
                distance_sq,
                d3d_light_index: 0,
                d3d,
            },
            1,
        );
    }
}

/// The light insert's colour handling: `info.color` is quantised through an 8-bit RGB round-trip
/// (each channel truncated to a byte, repacked, then divided by 255).
#[must_use]
pub fn quantise_colour(mut info: LightInfo) -> LightInfo {
    for c in &mut info.color {
        // LINT-OK: the value is clamped to 0..=255 before the cast.
        let b = dereth_primitives::num::to_i32(*c * 255.0).clamp(0, 255) as u8;
        *c = f32::from(b) / 255.0;
    }
    info
}

/// One of the eight hardware light slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LightSlot {
    /// The light kept its slot from last frame, so the D3D light need not be re-uploaded.
    pub carry_over: bool,
    /// `0 = sunlight, 1 = static, 2 = dynamic, -1 = empty`.
    pub light_class: i32,
    pub index: i32,
}

/// The eight hardware slots, with the `prev`/`cur` pair [`ActiveLights::add`] matches against.
#[derive(Debug, Clone)]
pub struct ActiveLights {
    pub cur: [LightSlot; HARDWARE_LIGHT_SLOTS],
    pub prev: [LightSlot; HARDWARE_LIGHT_SLOTS],
}

impl Default for ActiveLights {
    fn default() -> Self {
        let empty = LightSlot {
            carry_over: false,
            light_class: -1,
            index: -1,
        };
        Self {
            cur: [empty; HARDWARE_LIGHT_SLOTS],
            prev: [empty; HARDWARE_LIGHT_SLOTS],
        }
    }
}

impl ActiveLights {
    /// Copy `cur` into `prev` and clear `cur`.
    pub fn reset(&mut self) {
        self.prev = self.cur;
        self.cur = [LightSlot {
            carry_over: false,
            light_class: -1,
            index: -1,
        }; HARDWARE_LIGHT_SLOTS];
    }

    /// Add one indexed light of the given class.
    ///
    /// If `(class, index)` held a slot last frame it re-uses that slot with `carry_over = true`, and
    /// whatever had already claimed that slot this frame is displaced to the first free one.
    /// Otherwise the pair goes into the first free slot with `carry_over = false`.
    ///
    /// Returns the slot it landed in, or `None` when all eight are taken — which is the 8-light cap.
    pub fn add(&mut self, index: i32, light_class: i32) -> Option<usize> {
        if let Some(k) = self
            .prev
            .iter()
            .position(|s| s.light_class == light_class && s.index == index)
        {
            let displaced = self.cur[k];
            self.cur[k] = LightSlot {
                carry_over: true,
                light_class,
                index,
            };
            if displaced.light_class != -1 {
                if let Some(free) = self.cur.iter().position(|s| s.light_class == -1) {
                    self.cur[free] = displaced;
                } else {
                    // Nowhere to put it: the displaced light is dropped, which is the cap biting.
                }
            }
            return Some(k);
        }
        let free = self.cur.iter().position(|s| s.light_class == -1)?;
        self.cur[free] = LightSlot {
            carry_over: false,
            light_class,
            index,
        };
        Some(free)
    }

    /// How many slots are in use.
    #[must_use]
    pub fn active(&self) -> usize {
        self.cur.iter().filter(|s| s.light_class != -1).count()
    }
}

/// Enable the sunlight set outdoors and for the outdoor pass seen through
/// portals: reset, add the sun (class 0, index -1), enable. **The sun is the only light.**
///
/// `use_sunlight_set(.., false)` is a reset only: every slot is disabled, which is what runs before
/// indoor cells.
pub fn use_sunlight_set(a: &mut ActiveLights, on: bool) {
    a.reset();
    if on {
        a.add(-1, 0);
    }
}

/// Apply the per-object light reach test.
///
/// Returns **true when the light should be dropped**: a point light whose falloff sphere does not
/// intersect the object's bounding sphere. Directional and spot lights are never dropped.
///
/// ```text
/// |viewerspace_location - object_center|² - (falloff + object_radius)² >= 0.0002
/// ```
#[must_use]
pub fn remove_object_light(info: &LightInfo, object_center: Vec3, object_radius: f32) -> bool {
    if info.light_type != LightType::Point {
        return false;
    }
    let d = info.viewerspace_location.sub(object_center);
    let reach = info.falloff + object_radius;
    d.mag2() - reach * reach >= LIGHT_REACH_EPSILON
}

/// Select indoor lights for one object.
///
/// Reset, then for each **dynamic** light in distance order keep it if [`remove_object_light`] says
/// it reaches the object, up to 8; then the same for the **static** lights with the remaining slots.
/// Dynamics first is the required order and it decides which lights survive the cap.
#[must_use]
pub fn minimize_object_lighting(
    pools: &LightPools,
    object_center: Vec3,
    object_radius: f32,
) -> ActiveLights {
    let mut a = ActiveLights::default();
    a.reset();
    for (i, l) in pools.dynamics.iter().enumerate() {
        if remove_object_light(&l.info, object_center, object_radius) {
            continue;
        }
        // LINT-OK: index arithmetic; the pool caps are at most 60.
        if a.add(i32_of(i), 2).is_none() {
            return a;
        }
    }
    for (i, l) in pools.statics.iter().enumerate() {
        if remove_object_light(&l.info, object_center, object_radius) {
            continue;
        }
        // LINT-OK: index arithmetic; the pool caps are at most 60.
        if a.add(i32_of(i), 1).is_none() {
            return a;
        }
    }
    a
}

/// Select lights for an environment cell: reset, then mark
/// **every** dynamic light active. Static lights are handled by the burned-in vertex colours, so
/// they take no slot.
///
/// The retail client also takes the cell position and the drawing-BSP radius, and ignores both --
/// presumably a dropped distance cull -- so they are absent here.
#[must_use]
pub fn minimize_envcell_lighting(pools: &LightPools) -> ActiveLights {
    let mut a = ActiveLights::default();
    a.reset();
    for i in 0..pools.dynamics.len() {
        // LINT-OK: index arithmetic; the pool caps are at most 60.
        if a.add(i32_of(i), 2).is_none() {
            break;
        }
    }
    a
}

/// Transform a light into a mesh's frame. Point and spot
/// lights get `globaltolocal(frame, offset.origin)`; directional lights get the frame's heading
/// vector transformed instead. Colour, intensity, falloff and cone angle are copied unchanged.
#[must_use]
pub fn convert_to_local(info: &LightInfo, mesh_frame: &Frame) -> LightInfo {
    let m = l2g(mesh_frame.rotation);
    let global_to_local_vec = |v: Vec3| {
        let m = m.0;
        Vec3::new(
            m[0] * v.x + m[1] * v.y + m[2] * v.z,
            m[3] * v.x + m[4] * v.y + m[5] * v.z,
            m[6] * v.x + m[7] * v.y + m[8] * v.z,
        )
    };
    let mut out = *info;
    if info.light_type == LightType::Directional {
        let dir = crate::math::get_vector_heading(&info.offset);
        out.offset.origin = global_to_local_vec(dir);
    } else {
        out.offset.origin = global_to_local_vec(info.offset.origin.sub(mesh_frame.origin));
    }
    out
}

/// Calculate one point light's burned-in vertex contribution.
/// static-light term, exactly.
///
/// ```text
/// d  = light.offset.origin - vertex.position           // both in mesh space
/// d2 = d·d ; dist = sqrt(d2)
/// range = light.falloff * 1.3
/// if dist >= range: return
/// n_dot = (vertex.normal · d + dist * 0.5) * (2.0/3.0)
/// if n_dot <= 0: return
/// denom = (d2 > 1.0) ? d2 * dist : dist                // 1/d inside a unit, 1/d³ beyond it
/// s = (1.0 - dist / range) * light.intensity * (n_dot / denom)
/// r += min(s * light.color.r, light.color.r)          // each channel capped at the light's colour
/// ```
///
/// Both oddities are deliberate and visible: the `+ dist*0.5` term wraps the lambert factor so a
/// surface facing away from a nearby light still receives half intensity, and the `d2 > 1` branch
/// makes the falloff inverse-**cube** beyond one unit.
pub fn calc_point_light(vertex_pos: Vec3, vertex_normal: Vec3, l: &LightInfo, rgb: &mut [f32; 3]) {
    let d = l.offset.origin.sub(vertex_pos);
    let d2 = d.mag2();
    let dist = d2.sqrt();
    let range = l.falloff * CALC_POINT_LIGHT_FALLOFF_MULTIPLIER;
    if dist >= range {
        return;
    }
    let n_dot = (vertex_normal.dot(d) + dist * 0.5) * 0.666_666_7;
    if n_dot <= 0.0 {
        return;
    }
    let denom = if d2 > 1.0 { d2 * dist } else { dist };
    let s = (1.0 - dist / range) * l.intensity * (n_dot / denom);
    for (c, &col) in rgb.iter_mut().zip(l.color.iter()) {
        *c += (s * col).min(col);
    }
}

/// Vertex-lighting inner loop: every
/// **static** point light in sorted order, accumulated, then quantised to a byte diffuse.
#[must_use]
pub fn burn_static_lighting(
    pools: &LightPools,
    mesh_frame: &Frame,
    vertex_pos: Vec3,
    vertex_normal: Vec3,
) -> [u8; 3] {
    let mut c = [0.0f32; 3];
    for l in &pools.statics {
        let li = convert_to_local(&l.info, mesh_frame);
        if li.light_type == LightType::Point {
            calc_point_light(vertex_pos, vertex_normal, &li, &mut c);
        }
    }
    // LINT-OK: the value is clamped to 0..=255 before the cast.
    c.map(|v| dereth_primitives::num::to_i32(v * 255.0).clamp(0, 255) as u8)
}

// ---------------------------------------------------------------------------------------------
// The `D3DLIGHT9`s the fixed-function pipeline is handed, and the equation it runs.
//
// Everything below matches the retail client's values and behaviour.
// ---------------------------------------------------------------------------------------------

/// `D3DLIGHTTYPE::D3DLIGHT_POINT`: the type word is set to 1.
pub const D3DLIGHT_POINT: u32 = 1;
/// `D3DLIGHT_SPOT`: the type word is set to 2.
pub const D3DLIGHT_SPOT: u32 = 2;
/// `D3DLIGHT_DIRECTIONAL`: the type word is set to 3.
pub const D3DLIGHT_DIRECTIONAL: u32 = 3;

/// Viewer-light intensity — **2.25**. The stored initial value is 0.0, but a static initialiser
/// that runs before startup sets it to `0.5 * 4.5 = 2.25`.
///
/// Startup registers it as a viewer-light-intensity registry
/// variable, so only a console write changes it. With 0.0 the viewer light would still be added
/// to the dynamic pool every frame, with no intensity.
pub const VIEWER_LIGHT_INTENSITY: f32 = 2.25;
/// `viewer_light.cone_angle`: the constructor stores 360.0 into it.
/// Unused by a type-0 light; carried because the constructor writes it.
pub const VIEWER_LIGHT_CONE_ANGLE: f32 = 360.0;
/// The viewer light's falloff -- 10.0.
pub const VIEWER_LIGHT_FALLOFF: f32 = 10.0;
/// With a player the viewer light sits `2.0` (`0x40000000`) above the
/// player's origin; with none, at the viewer's.
pub const VIEWER_LIGHT_HEIGHT: f32 = 2.0;
/// Ambient level for the arm where the viewer cell has not
/// `seen_outside` (the flag the outdoor arm tests):
/// the world ambient is set to `level` with colour `0xFFFFFFFF`, and the level the client holds
/// is **0.2**.
pub const INDOOR_AMBIENT_LEVEL: f32 = 0.2;
/// The sunlight validity threshold, 0.0002.
pub const SUNLIGHT_EPSILON: f32 = 0.0002;
/// Byte-to-float factor, the `float` value `1/255`.
pub const BYTE_TO_FLOAT: f32 = 0.003_921_569;

/// The fields of a `D3DLIGHT9` the client writes. `Position` and `Direction` are in D3D order --
/// the client's `(x, y, z)` stored as `(x, z, y)`, which `config_hardware_light` preserves through
/// the three-word copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D3dLight {
    pub light_type: u32,
    /// `Diffuse.rgb`; `Diffuse.a` is only ever written for the sun (1.0).
    pub diffuse: [f32; 3],
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub range: f32,
    pub falloff: f32,
    /// `Attenuation0..2`.
    pub attenuation: [f32; 3],
    pub theta: f32,
    pub phi: f32,
}

impl Default for D3dLight {
    fn default() -> Self {
        Self {
            light_type: D3DLIGHT_POINT,
            diffuse: [0.0; 3],
            position: [0.0; 3],
            direction: [0.0; 3],
            range: 0.0,
            falloff: 0.0,
            attenuation: [0.0; 3],
            theta: 0.0,
            phi: 0.0,
        }
    }
}

/// zero for a point light, the offset frame's heading
/// a direction vector for a directional or spot light.
#[must_use]
pub fn get_direction(info: &LightInfo) -> Vec3 {
    match info.light_type {
        LightType::Directional | LightType::Spot => crate::math::get_vector_heading(&info.offset),
        LightType::Point => Vec3::ZERO,
    }
}

/// for a directional or spot light, normalise the vector
/// (`1 / sqrt(y*y + z*z + x*x)`, the client's own operand order) and the vector-heading setter.
pub fn set_direction(info: &mut LightInfo, v: Vec3) {
    if !matches!(info.light_type, LightType::Directional | LightType::Spot) {
        return;
    }
    let inv = 1.0 / (v.y * v.y + v.z * v.z + v.x * v.x).sqrt();
    crate::math::set_vector_heading(&mut info.offset, Vec3::new(inv * v.x, inv * v.y, inv * v.z));
}

/// The hardware light configuration, verbatim.
///
/// ```text
/// Diffuse = colour * intensity
/// Specular = Ambient = 0
/// type 0: POINT, Falloff = 1, Range = falloff * 1.5, Position = (o.x, o.z, o.y)
/// type 1: DIRECTIONAL, Direction = (d.x, d.z, d.y), return   -- no attenuation written
/// type 2: SPOT, Falloff = 1, Range, Theta = Phi = cone_angle, Position, Direction
/// Attenuation = (0, 1, 0)
/// ```
#[must_use]
pub fn config_hardware_light(info: &LightInfo) -> D3dLight {
    let mut l = D3dLight {
        diffuse: [
            info.color[0] * info.intensity,
            info.color[1] * info.intensity,
            info.color[2] * info.intensity,
        ],
        ..D3dLight::default()
    };
    let o = info.offset.origin;
    match info.light_type {
        LightType::Point => {
            l.light_type = D3DLIGHT_POINT;
            l.falloff = 1.0;
            l.range = info.falloff * D3D_RANGE_MULTIPLIER;
            l.position = [o.x, o.z, o.y];
        }
        LightType::Directional => {
            l.light_type = D3DLIGHT_DIRECTIONAL;
            let d = get_direction(info);
            l.direction = [d.x, d.z, d.y];
            return l;
        }
        LightType::Spot => {
            l.light_type = D3DLIGHT_SPOT;
            l.falloff = 1.0;
            l.range = info.falloff * D3D_RANGE_MULTIPLIER;
            l.theta = info.cone_angle;
            l.phi = info.cone_angle;
            l.position = [o.x, o.z, o.y];
            let d = get_direction(info);
            l.direction = [d.x, d.z, d.y];
        }
    }
    l.attenuation = [0.0, 1.0, 0.0];
    l
}

/// Build the sunlight's fixed-function light record:
///
/// ```text
/// if !sunlight_valid and (|s.x| >= 0.0002 or |s.y| >= 0.0002 or |s.z| >= 0.0002):
///     Diffuse.a = 1 ; sunlight_valid = true
///     mag = sqrt(s.y*s.y + s.z*s.z + s.x*s.x)          -- that operand order
///     Diffuse.rgb = sunlight_color * mag
///     Direction = (-s.x, -s.z, -s.y)
/// ```
///
/// `None` is the gate refusing: the client keeps whatever its sunlight record held, which at
/// start-up is zero -- a light that contributes nothing.
#[must_use]
pub fn sunlight_light(sunlight: Vec3, sunlight_color: [f32; 3]) -> Option<D3dLight> {
    let s = sunlight;
    if s.x.abs() < SUNLIGHT_EPSILON && s.y.abs() < SUNLIGHT_EPSILON && s.z.abs() < SUNLIGHT_EPSILON
    {
        return None;
    }
    let mag = (s.y * s.y + s.z * s.z + s.x * s.x).sqrt();
    Some(D3dLight {
        light_type: D3DLIGHT_DIRECTIONAL,
        diffuse: [
            sunlight_color[0] * mag,
            sunlight_color[1] * mag,
            sunlight_color[2] * mag,
        ],
        direction: [-s.x, -s.z, -s.y],
        ..D3dLight::default()
    })
}

/// `((c >> 16) & 0xFF) * 0.003921569`, and so on.
#[must_use]
pub fn set_color32(c: u32) -> [f32; 3] {
    // LINT-OK: each operand is masked to a byte first. Not a float conversion.
    #[allow(clippy::cast_precision_loss)]
    [
        ((c >> 16) & 0xFF) as f32 * BYTE_TO_FLOAT,
        ((c >> 8) & 0xFF) as f32 * BYTE_TO_FLOAT,
        (c & 0xFF) as f32 * BYTE_TO_FLOAT,
    ]
}

/// Decode `colour` and multiply every ambient channel by `level`.
#[must_use]
pub fn world_ambient(level: f32, color32: u32) -> [f32; 3] {
    let c = set_color32(color32);
    [c[0] * level, c[1] * level, c[2] * level]
}

/// `|sunlight| * 0.2 + ambient_level` -- the ambient the
/// *objects* see outdoors, 20 % of the sun's magnitude brighter than the terrain's.
#[must_use]
pub fn calc_object_light(sunlight: Vec3, ambient_level: f32) -> f32 {
    let s = sunlight;
    (s.x * s.x + s.y * s.y + s.z * s.z).sqrt() * 0.2 + ambient_level
}

/// Ambient render state from the world lights' ambient colour
/// -- each channel times 1.0, then `>= 1 -> 255`, `<= 0 -> 0`, else truncation
/// to the byte, repacked with alpha `0xFF` and quantised a
/// second time, which is idempotent.
#[must_use]
pub fn ambient_render_state(ambient: [f32; 3]) -> u32 {
    let byte = |c: f32| -> u32 {
        let c = c * AMBIENT_MULTIPLIER;
        if c >= 1.0 {
            0xFF
        } else if c <= 0.0 {
            0
        } else {
            // LINT-OK: 0 < c < 1, so the product is inside 0..255. Not a cast on a wild float.
            #[allow(clippy::cast_sign_loss)]
            {
                dereth_primitives::num::to_i32(c * 255.0).clamp(0, 255) as u32
            }
        }
    };
    (0xFF << 24) | (byte(ambient[0]) << 16) | (byte(ambient[1]) << 8) | byte(ambient[2])
}

/// Build and restamp the viewer light every frame as a type-0 light
/// (the flag is cleared), colour **white** (the colour word is -1;
/// the color word is decoded), `intensity` =
/// [`VIEWER_LIGHT_INTENSITY`] (2.25), `falloff` = [`VIEWER_LIGHT_FALLOFF`] (10.0),
/// `cone_angle = 360`, and its offset origin at `(0, 0, 2)` when there is a player.
///
/// It is the only light in the dynamic pool at an ordinary station, so it reaches every interior
/// cell mesh, which enables every dynamic light, and every object, with dynamics first, as a
/// `D3DLIGHT_POINT` of Diffuse
/// `(2.25, 2.25, 2.25)`, Range 15, attenuation `1/d` -- a white fill that follows the player.
/// Without this viewer light, ordinary interiors lose their moving white fill.
#[must_use]
pub fn viewer_light(has_player: bool) -> LightInfo {
    LightInfo {
        light_type: LightType::Point,
        offset: Frame::new(
            Vec3::new(0.0, 0.0, if has_player { VIEWER_LIGHT_HEIGHT } else { 0.0 }),
            dereth_primitives::Quat::IDENTITY,
        ),
        viewerspace_location: Vec3::ZERO,
        color: set_color32(0xFFFF_FFFF),
        intensity: VIEWER_LIGHT_INTENSITY,
        falloff: VIEWER_LIGHT_FALLOFF,
        cone_angle: VIEWER_LIGHT_CONE_ANGLE,
    }
}

impl LightPools {
    /// Add a light from a **light object**: the light's [`LightInfo`] and the
    /// owning object's frame, both expressed in one space (the caller has already folded the
    /// block offset into `frame`, and `player_origin` is
    /// player position in that same space).
    ///
    /// ```text
    /// d2 = type == 0 ? |block_offset + frame.origin - player_pos.origin|^2 : 0
    /// L.info.offset = combine(frame, info.offset)
    /// L.info.color  = 8-bit round trip
    /// L.d3d = config_hardware_light(L.info)
    /// ```
    ///
    /// `viewerspace_location` is set to the combined origin: the reach test
    /// [`remove_object_light`] is a difference of two points in one space, and this build keeps
    /// every point in scene space rather than the viewer's.
    fn add_from(
        &mut self,
        is_static: bool,
        info: &LightInfo,
        cell_id: CellId,
        frame: &Frame,
        player_origin: Vec3,
    ) {
        let d2 = if info.light_type == LightType::Point {
            frame.origin.sub(player_origin).mag2()
        } else {
            0.0
        };
        let mut combined = quantise_colour(*info);
        combined.offset = crate::math::combine(frame, &info.offset);
        combined.viewerspace_location = combined.offset.origin;
        let d3d = config_hardware_light(&combined);
        let (pool, cap, base) = if is_static {
            // LINT-OK: index arithmetic; the pool caps are at most 60.
            (
                &mut self.statics,
                self.max_static,
                u32_of(self.max_dynamic) + 1,
            )
        } else {
            (&mut self.dynamics, self.max_dynamic, 1)
        };
        Self::insert(
            pool,
            cap,
            RenderLight {
                info: combined,
                cell_id,
                distance_sq: d2,
                d3d_light_index: 0,
                d3d,
            },
            base,
        );
    }

    /// Add one static `LIGHTOBJ` with `state & 1`.
    pub fn add_static_from(
        &mut self,
        info: &LightInfo,
        cell_id: CellId,
        frame: &Frame,
        player_origin: Vec3,
    ) {
        self.add_from(true, info, cell_id, frame, player_origin);
    }

    /// Add one dynamic `LIGHTOBJ` without the static bit.
    pub fn add_dynamic_from(
        &mut self,
        info: &LightInfo,
        cell_id: CellId,
        frame: &Frame,
        player_origin: Vec3,
    ) {
        self.add_from(false, info, cell_id, frame, player_origin);
    }

    /// Clear the static-light pool.
    pub fn clear_statics(&mut self) {
        self.statics.clear();
    }

    /// Clear the dynamic-light pool once
    /// per frame before the viewer light and every visible cell's dynamic lights are re-added.
    pub fn clear_dynamics(&mut self) {
        self.dynamics.clear();
    }
}

/// Reduce active lights to what a shader needs: the `D3DLIGHT9` record for
/// every slot whose class is not -1, in slot order -- class 0 the sun (the world's
/// sunlight record), class 1 the sorted static light at `index`, class 2
/// the sorted dynamic light at `index`. A class-0 slot with no valid sun contributes nothing, which is
/// what the zero-initialised sunlight does before the first light update.
#[must_use]
pub fn enabled_lights(
    active: &ActiveLights,
    pools: &LightPools,
    sun: Option<&D3dLight>,
) -> Vec<D3dLight> {
    let mut out = Vec::with_capacity(HARDWARE_LIGHT_SLOTS);
    for s in &active.cur {
        let l = match s.light_class {
            0 => sun.copied(),
            1 => usize::try_from(s.index)
                .ok()
                .and_then(|i| pools.statics.get(i))
                .map(|l| l.d3d),
            2 => usize::try_from(s.index)
                .ok()
                .and_then(|i| pools.dynamics.get(i))
                .map(|l| l.d3d),
            _ => None,
        };
        if let Some(l) = l {
            out.push(l);
        }
    }
    out
}

/// The three material channels the Direct3D 9 vertex pipeline reads, after the
/// `D3DRS_*MATERIALSOURCE` selection has been made by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FfMaterial {
    /// `Md` -- the vertex colour (vertex source) or material diffuse RGB (material source).
    pub diffuse: [f32; 3],
    /// `Ma` -- likewise.
    pub ambient: [f32; 3],
    /// `Me` -- material emissive RGB, or the burned vertex colour (vertex source).
    pub emissive: [f32; 3],
}

/// The Direct3D 9 fixed-function vertex lighting equation, as `legacy.hlsl`'s vertex stage runs
/// it, evaluated on the CPU: the reference the tests hold the shader to.
///
/// ```text
/// N = normalize(normal)                                       D3DRS_NORMALIZENORMALS = 1
/// for each enabled light k:
///     DIRECTIONAL: L = -normalize(Direction), atten = 1
///     else:        d = Position - P, dist = |d|; skip if dist >= Range;
///                  L = d / dist, atten = 1 / (a0 + a1 * dist + a2 * dist^2)
///     sum += Diffuse_k * max(N . L, 0) * atten
/// out = saturate(Me + Ma * D3DRS_AMBIENT + Md * sum)          specular off, light ambient 0
/// ```
///
/// `pos` and `normal` are in the same D3D-ordered space the lights are in.
#[must_use]
pub fn ff_vertex_diffuse(
    pos: [f32; 3],
    normal: [f32; 3],
    lights: &[D3dLight],
    ambient: [f32; 3],
    m: &FfMaterial,
) -> [f32; 3] {
    let n = {
        let mag = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if mag > 0.0 {
            [normal[0] / mag, normal[1] / mag, normal[2] / mag]
        } else {
            normal
        }
    };
    let mut sum = [0.0f32; 3];
    for l in lights {
        let (ldir, atten) = if l.light_type == D3DLIGHT_DIRECTIONAL {
            let d = l.direction;
            let mag = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if mag <= 0.0 {
                continue;
            }
            ([-d[0] / mag, -d[1] / mag, -d[2] / mag], 1.0)
        } else {
            let d = [
                l.position[0] - pos[0],
                l.position[1] - pos[1],
                l.position[2] - pos[2],
            ];
            let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if dist >= l.range || dist <= 0.0 {
                continue;
            }
            let a = l.attenuation;
            (
                [d[0] / dist, d[1] / dist, d[2] / dist],
                1.0 / (a[0] + a[1] * dist + a[2] * dist * dist),
            )
        };
        let ndotl = (n[0] * ldir[0] + n[1] * ldir[1] + n[2] * ldir[2]).max(0.0);
        for (s, d) in sum.iter_mut().zip(l.diffuse) {
            *s += d * ndotl * atten;
        }
    }
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        out[c] =
            (m.emissive[c] + m.ambient[c] * ambient[c] + m.diffuse[c] * sum[c]).clamp(0.0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;

    fn light_at(x: f32, falloff: f32) -> LightInfo {
        LightInfo {
            offset: Frame::new(Vec3::new(x, 0.0, 0.0), dereth_primitives::Quat::IDENTITY),
            color: [1.0, 0.5, 0.25],
            intensity: 1.0,
            falloff,
            ..LightInfo::default()
        }
    }

    /// Oracle: the recovered point-light formula, evaluated by hand.
    /// A light 2 m away along +x, falloff 10 (so range 13), lighting a vertex whose normal points
    /// straight at it:
    ///
    /// `d = (2,0,0)`, `d2 = 4`, `dist = 2`, `n_dot = (2 + 1) * 2/3 = 2`,
    /// `denom = 4*2 = 8` (the `d2 > 1` branch), `s = (1 - 2/13) * 1 * (2/8) = 0.21153846`.
    #[test]
    fn calc_point_light_matches_the_hand_evaluated_arithmetic() {
        let l = light_at(2.0, 10.0);
        let mut rgb = [0.0f32; 3];
        calc_point_light(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), &l, &mut rgb);
        let s = (1.0 - 2.0 / 13.0) * (2.0 / 8.0);
        assert!((rgb[0] - s * 1.0).abs() < 1e-6, "{rgb:?} vs s={s}");
        assert!((rgb[1] - s * 0.5).abs() < 1e-6);
        assert!((rgb[2] - s * 0.25).abs() < 1e-6);
    }

    /// Oracle: trap 6 — the attenuation is 1/d beyond 1 unit but 1/d³ inside
    /// 1 unit because of the `d2 > 1` branch. Crossing `d == 1` must change which denominator is
    /// used, and the test computes both forms independently.
    #[test]
    fn the_falloff_switches_branch_at_one_unit() {
        for (dist, expect_cubed) in [(0.5f32, false), (0.999, false), (1.001, true), (3.0, true)] {
            let l = light_at(dist, 10.0);
            let mut rgb = [0.0f32; 3];
            calc_point_light(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), &l, &mut rgb);
            let d2 = dist * dist;
            let n_dot = (dist + dist * 0.5) * 0.666_666_7;
            let denom = if expect_cubed { d2 * dist } else { dist };
            let s = (1.0 - dist / 13.0) * (n_dot / denom);
            assert!(
                (rgb[0] - s.min(1.0)).abs() < 1e-5,
                "dist={dist} {rgb:?} vs {s}"
            );
        }
    }

    /// Oracle: the `+ dist*0.5` term wraps the Lambert factor so a surface
    /// facing away from a nearby light still receives half intensity. Without it a back-facing
    /// vertex would receive nothing, and env-cell interiors would go black.
    #[test]
    fn the_wrap_term_lights_a_surface_facing_away_from_a_near_light() {
        let l = light_at(0.5, 10.0);
        let mut rgb = [0.0f32; 3];
        // A normal pointing directly away from the light: n·d = -0.5.
        calc_point_light(Vec3::ZERO, Vec3::new(-1.0, 0.0, 0.0), &l, &mut rgb);
        // n_dot = (-0.5 + 0.25) * 2/3 < 0, so this one is rejected...
        assert_eq!(rgb, [0.0; 3]);
        // ..but a normal merely perpendicular still receives light, which a plain lambert would
        // not: n·d = 0, n_dot = (0 + 0.25) * 2/3 > 0.
        let mut rgb = [0.0f32; 3];
        calc_point_light(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), &l, &mut rgb);
        assert!(
            rgb[0] > 0.0,
            "the wrap term must light a perpendicular surface: {rgb:?}"
        );
    }

    /// Oracle: each channel is individually clamped at the light's own colour, so
    /// a single light can never over-brighten, and `dist >= range` returns without contributing.
    #[test]
    fn channels_clamp_at_the_lights_colour_and_range_cuts_off() {
        // A very close, very intense light: s explodes, and each channel must stop at the colour.
        let mut l = light_at(0.01, 10.0);
        l.intensity = 1000.0;
        let mut rgb = [0.0f32; 3];
        calc_point_light(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), &l, &mut rgb);
        assert_eq!(
            rgb,
            [1.0, 0.5, 0.25],
            "each channel caps at the light's own colour"
        );
        // Outside range * 1.3: nothing at all.
        let far = light_at(13.0, 10.0);
        let mut rgb = [0.0f32; 3];
        calc_point_light(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), &far, &mut rgb);
        assert_eq!(rgb, [0.0; 3], "dist >= falloff * 1.3 contributes nothing");
        let just_inside = light_at(12.99, 10.0);
        let mut rgb = [0.0f32; 3];
        calc_point_light(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), &just_inside, &mut rgb);
        assert!(
            rgb[0] > 0.0,
            "just inside the 1.3x range it still contributes"
        );
    }

    /// Oracle: light-pool insertion and contract 11.8 — the pools are sorted by squared
    /// distance **from the player**, ascending, and a full pool drops a light that is farther than
    /// everything it already holds.
    #[test]
    fn pools_stay_sorted_and_drop_the_farthest_when_full() {
        let mut p = LightPools::new(0.0);
        p.max_dynamic = 3;
        for d in [50.0f32, 10.0, 30.0, 20.0] {
            p.add_dynamic(LightInfo::default(), CellId(0), d);
        }
        assert_eq!(p.dynamics.len(), 3);
        assert_eq!(
            p.dynamics.iter().map(|l| l.distance_sq).collect::<Vec<_>>(),
            vec![10.0, 20.0, 30.0],
            "the pool keeps the three nearest, in order"
        );
        // A light farther than everything held is dropped outright.
        p.add_dynamic(LightInfo::default(), CellId(0), 999.0);
        assert_eq!(
            p.dynamics.iter().map(|l| l.distance_sq).collect::<Vec<_>>(),
            vec![10.0, 20.0, 30.0]
        );
        // Index 0 is reserved for the sun, so dynamic indices start at 1.
        assert_eq!(p.dynamics[0].d3d_light_index, 1);
    }

    /// Oracle: the recovered degradation table — `deg_mul` -1/0/+1 gives
    /// (20, 4), (40, 7) and (60, 9).
    #[test]
    fn pool_caps_match_the_degrade_table() {
        assert_eq!(pool_caps(-1.0), (20, 4));
        assert_eq!(pool_caps(0.0), (40, 7));
        assert_eq!(pool_caps(1.0), (60, 9));
    }

    /// Oracle: object-light minimization and contract 11.8 — dynamics first,
    /// then statics, up to eight slots total.
    #[test]
    fn the_eight_light_cap_keeps_dynamics_first() {
        let mut p = LightPools::new(1.0);
        for i in 0..6 {
            let mut info = LightInfo {
                falloff: 1000.0,
                ..LightInfo::default()
            };
            info.viewerspace_location = Vec3::ZERO;
            p.add_dynamic(info, CellId(0), f32::from(i as u16));
        }
        for i in 0..10 {
            let info = LightInfo {
                falloff: 1000.0,
                ..LightInfo::default()
            };
            p.add_static(info, CellId(0), f32::from(i as u16));
        }
        let a = minimize_object_lighting(&p, Vec3::ZERO, 1.0);
        assert_eq!(
            a.active(),
            HARDWARE_LIGHT_SLOTS,
            "all eight slots are taken"
        );
        let classes: Vec<i32> = a.cur.iter().map(|s| s.light_class).collect();
        assert_eq!(
            classes.iter().filter(|&&c| c == 2).count(),
            6,
            "all six dynamics kept"
        );
        assert_eq!(
            classes.iter().filter(|&&c| c == 1).count(),
            2,
            "only two statics fit"
        );
    }

    /// Oracle: the reach test's exact threshold,
    /// `|Δ|² − (falloff + radius)² >= 0.0002`, and that non-point lights are never dropped.
    #[test]
    fn the_reach_test_uses_the_documented_threshold() {
        let mut info = LightInfo {
            falloff: 3.0,
            ..LightInfo::default()
        };
        info.viewerspace_location = Vec3::new(5.0, 0.0, 0.0);
        // radius 2: reach 5, |Δ|² = 25, 25 - 25 = 0 < 0.0002 -> kept.
        assert!(!remove_object_light(&info, Vec3::ZERO, 2.0));
        // radius 1.9: reach 4.9, 25 - 24.01 = 0.99 >= 0.0002 -> dropped.
        assert!(remove_object_light(&info, Vec3::ZERO, 1.9));
        // A directional light is never dropped, however far it is.
        let d = LightInfo {
            light_type: LightType::Directional,
            ..info
        };
        assert!(!remove_object_light(&d, Vec3::new(1000.0, 0.0, 0.0), 0.1));
    }

    /// Oracle: `(1)` makes the sun the only light outdoors,
    /// `(0)` disables every slot indoors.
    #[test]
    fn use_sunlight_set_is_all_or_nothing() {
        let mut a = ActiveLights::default();
        use_sunlight_set(&mut a, true);
        assert_eq!(a.active(), 1);
        assert_eq!(a.cur[0].light_class, 0, "class 0 is the sun");
        assert_eq!(a.cur[0].index, -1);
        use_sunlight_set(&mut a, false);
        assert_eq!(a.active(), 0, "indoors every slot is disabled");
    }

    /// Oracle: a light that held a slot last frame gets the
    /// same slot back with `carry_over` set, so the D3D light need not be re-uploaded.
    #[test]
    fn a_returning_light_keeps_its_slot_and_carries_over() {
        let mut a = ActiveLights::default();
        a.reset();
        a.add(3, 2);
        a.add(7, 1);
        let slot_of_3 = a
            .cur
            .iter()
            .position(|s| s.index == 3)
            .expect("light 3 has a slot");
        a.reset();
        // Claim that slot with someone else first, then bring light 3 back.
        a.add(9, 2);
        a.add(3, 2);
        assert_eq!(a.cur[slot_of_3].index, 3, "light 3 reclaimed its old slot");
        assert!(a.cur[slot_of_3].carry_over);
        assert!(
            a.cur.iter().any(|s| s.index == 9),
            "the displaced light found another slot"
        );
    }

    /// Oracle: the light insert's colour handling — "quantised through an 8-bit RGB round-trip". A
    /// colour that is not a multiple of 1/255 must not survive unchanged.
    #[test]
    fn light_colours_round_trip_through_eight_bits() {
        let info = LightInfo {
            color: [0.5, 0.333, 1.0],
            ..LightInfo::default()
        };
        let q = quantise_colour(info);
        assert_eq!(
            q.color[0],
            127.0 / 255.0,
            "0.5*255 = 127.5 truncates to 127"
        );
        assert_eq!(q.color[1], 84.0 / 255.0);
        assert_eq!(q.color[2], 1.0);
    }
}
