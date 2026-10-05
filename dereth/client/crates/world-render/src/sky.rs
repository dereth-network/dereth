//! Sky, fog and the day cycle.
//!
//! The sky description's sky, lighting, world fog and present-day-group accessors, the day
//! group's time-of-day split, and the game sky's tick, frame calculation and draw.
//!
//! Time-of-day interpolation controls ambient light, sunlight and fog together.
//!
//! [`get_lighting`] returns a sun vector whose **length is `dir_bright`**, so brightness is
//! carried in the vector and `calc_lighting`'s `n · sunlight` already includes it. Normalising it
//! flattens the whole day cycle.

use dereth_assets::region::{Region, SkyDayPreset, SkyTime};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, Frame, Vec3};

/// One sky object's state at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelestialPosition {
    /// `INVALID_DID` (0) when the object is not present at this time of day.
    pub gfx_id: DataId,
    pub pes_id: DataId,
    /// The fixed orientation a `SkyObjectReplace` forces, in degrees. 0 = none.
    pub heading: f32,
    /// The animated elevation sweep over the object's own `begin_time..end_time` window, degrees.
    pub rotation: f32,
    pub tex_velocity: (f32, f32),
    /// Percentages 0..100, or `-1` for "unset".
    pub transparent: f32,
    pub luminosity: f32,
    pub max_bright: f32,
    /// bit0 = draw after the landscape, bit1 = hide under a fog override, bit2 = weather /
    /// position-locked.
    ///
    /// This field is not zero for every shipped sky object. Over the region's 20 day groups it
    /// takes 0 (×120), 2 (×20),
    /// 4 (×8), 5 (×8) and 13 (×76), so [`draws_in_pass_0`] and [`SkyPass::After`] are live code.
    /// The bit names describe the roles used by the sky renderer.
    ///
    /// Only the eight **"Rainy"** groups carry a weather layer. The three "Cloudy" ones use `{0, 2}`
    /// like every "Sunny" and "Clear" group, and their overcast is the ordinary cloud deck. Bit 3
    /// (`0x8`) is *not* unread: the sky-position update tests it to
    /// decide whether a weather object is dropped to the fixed height `z = -120`.
    pub properties: u32,
}

impl Default for CelestialPosition {
    fn default() -> Self {
        Self {
            gfx_id: DataId(0),
            pes_id: DataId(0),
            heading: 0.0,
            rotation: 0.0,
            tex_velocity: (0.0, 0.0),
            transparent: -1.0,
            luminosity: -1.0,
            max_bright: -1.0,
            properties: 0,
        }
    }
}

/// What the sky lighting accessor produces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLighting {
    pub ambient_level: f32,
    pub ambient_color: [u8; 3],
    /// **Unnormalised**: `|sun_vec| == dir_bright`.
    pub sun_vec: Vec3,
    pub sun_color: [u8; 3],
}

impl SkyLighting {
    /// [`get_lighting`]'s fallback when the day group has no `sky_time` rows at all:
    /// `ambient_level 0.3`, white, `sun_vec (0.5, 0.0, 0.8)`.
    #[must_use]
    pub fn fallback() -> Self {
        Self {
            ambient_level: 0.3,
            ambient_color: [255, 255, 255],
            sun_vec: Vec3::new(0.5, 0.0, 0.8),
            sun_color: [255, 255, 255],
        }
    }
}

/// What produces, when it produces anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldFog {
    pub min: f32,
    pub max: f32,
    pub color: [u8; 3],
}

/// Select a day group from a **deterministic hash of the in-game date**, so
/// every client sees the same weather.
///
/// ```text
/// v = (int32)((year*dpy + day) * 0x6A42FDB2 - 0x7541E9AE)
/// present_day_group = (int)floor(((uint32)v * 2^-32) * day_groups.count)
/// if present_day_group >= count: present_day_group = 0
/// ```
///
/// `chance_of_occur` exists in the data but is **not** used by this selection.
#[must_use]
pub fn calc_present_day_group(year: u32, days_per_year: u32, day: u32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let v = year
        .wrapping_mul(days_per_year)
        .wrapping_add(day)
        .wrapping_mul(0x6A42_FDB2)
        .wrapping_sub(0x7541_E9AE);
    #[allow(clippy::cast_precision_loss)] // the client converts u32 to float here too
    let f = (v as f32) * 2.328_306_4e-10 * (count as f32);
    let i = dereth_primitives::num::floor_to_i32(f);
    let i = i as u32 as usize;
    if i >= count {
        0
    } else {
        i
    }
}

/// The day group's time-of-day split.
///
/// Returns `(index of a, index of b, blend factor)`. The **last entry wraps to the first**, treating
/// `begin = 1.0` as the end of the day — which is what makes midnight continuous.
#[must_use]
pub fn get_time_of_day(sky_time: &[SkyTime], t: f32) -> Option<(usize, usize, f32)> {
    let n = sky_time.len();
    if n == 0 {
        return None;
    }
    let mut i = 0usize;
    if n > 1 {
        while i < n - 1 && t >= sky_time[i + 1].begin {
            i += 1;
        }
    }
    let a = &sky_time[i];
    if i == n - 1 {
        Some((i, 0, (t - a.begin) / (1.0 - a.begin)))
    } else {
        let b = &sky_time[i + 1];
        Some((i, i + 1, (t - a.begin) / (b.begin - a.begin)))
    }
}

/// Per-channel lerp of two packed `0x00RRGGBB` colours, truncated toward zero.
///
/// Both directional and ambient colors use the same per-channel interpolation factor. Each byte is
/// computed as
/// `a + (b - a) * f`, truncated toward zero, and written back with alpha forced to `0xFF`. There
/// is no endpoint snap, so dawn and dusk fade rather than step.
///
/// The `clamp` below is a no-op for the shipped data (both endpoints are bytes and `f` is in
/// `[0, 1)`); retail has no clamp at all, it simply stores the low byte.
#[must_use]
pub fn lerp_color(a: u32, b: u32, f: f32) -> [u8; 3] {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as u8;
    let mut out = [0u8; 3];
    for (k, s) in [16u32, 8, 0].into_iter().enumerate() {
        let (ca, cb) = (f32::from(ch(a, s)), f32::from(ch(b, s)));
        // LINT-OK: the value is clamped to 0..=255 before the cast.
        out[k] = dereth_primitives::num::to_i32(ca + (cb - ca) * f).clamp(0, 255) as u8;
    }
    out
}

/// The sky lighting accessor.
///
/// ```text
/// ambient_level = a.amb_bright + (b.amb_bright - a.amb_bright) * f
/// bright   = a.dir_bright + (b.dir_bright - a.dir_bright) * f
/// pitch    = lerp(a.dir_pitch,   b.dir_pitch,   f) * π/180
/// heading  = lerp(a.dir_heading, b.dir_heading, f) * π/180
/// sun_vec  = ( bright*sin(heading)*cos(pitch), bright*cos(heading)*cos(pitch), bright*sin(pitch) )
/// ```
///
/// The heading convention matches the client's world axes: heading 0 = +y = north, increasing
/// clockwise toward +x = east. **The vector is not normalised**; its length is `bright`.
///
/// The three sun components are the cosine of the pitch
/// and the sine/cosine of the heading, each multiplied by `bright`, and the two angles are
/// converted with pi/180 held as a **double**. The fallback
/// arm sets `ambient_level = 0x3E99999A` (0.3), both colours 0xFFFFFFFF and
/// `sun_vec = (0x3F000000, 0, 0x3F4CCCCD)` = (0.5, 0, 0.8) — [`SkyLighting::fallback`].
#[must_use]
pub fn get_lighting(group: &SkyDayPreset, t: f32) -> SkyLighting {
    let Some((ia, ib, f)) = get_time_of_day(&group.sky_time, t) else {
        return SkyLighting::fallback();
    };
    let (a, b) = (&group.sky_time[ia], &group.sky_time[ib]);
    let lerp = |x: f32, y: f32| x + (y - x) * f;
    let bright = lerp(a.dir_bright, b.dir_bright);
    let pitch = f64::from(lerp(a.dir_pitch, b.dir_pitch)) * std::f64::consts::PI / 180.0;
    let heading = f64::from(lerp(a.dir_heading, b.dir_heading)) * std::f64::consts::PI / 180.0;
    #[allow(clippy::cast_possible_truncation)]
    let sun_vec = Vec3::new(
        bright * (math::sin(heading) * math::cos(pitch)) as f32,
        bright * (math::cos(heading) * math::cos(pitch)) as f32,
        bright * math::sin(pitch) as f32,
    );
    SkyLighting {
        ambient_level: lerp(a.amb_bright, b.amb_bright),
        ambient_color: lerp_color(a.amb_color, b.amb_color, f),
        sun_vec,
        sun_color: lerp_color(a.dir_color, b.dir_color, f),
    }
}

/// Interpolate fog, returning `None` (leave fog alone) unless **both** bracketing
/// entries have `world_fog != 0`.
///
/// Four guards — `a` present, `a.world_fog != 0`, `b` present,
/// `b.world_fog != 0` — any one of which failing returns nothing. Then
/// `min = lerp(a.min_world_fog, b.min_world_fog, f)`,
/// `max = lerp(a.max_world_fog, b.max_world_fog, f)` and the same three-byte truncating colour
/// lerp as [`lerp_color`] over `world_fog_color`, alpha forced to 0xFF.
///
/// Sky drawing is the one consumer: it marks fixed-function fog user-disabled when the
/// fog-enabled setting is off, and when that setting is on (1 in the shipped data) it enables
/// fixed-function fog and hands this answer (colour, min, max) to
/// `D3DRS_FOGCOLOR`, `D3DRS_FOGSTART`, `D3DRS_FOGEND`. **No draw-distance
/// preference enters the range**; the numbers are the region's alone.
#[must_use]
pub fn get_world_fog(group: &SkyDayPreset, t: f32) -> Option<WorldFog> {
    let (ia, ib, f) = get_time_of_day(&group.sky_time, t)?;
    let (a, b) = (&group.sky_time[ia], &group.sky_time[ib]);
    if a.world_fog == 0 || b.world_fog == 0 {
        return None;
    }
    let lerp = |x: f32, y: f32| x + (y - x) * f;
    Some(WorldFog {
        min: lerp(a.min_world_fog, b.min_world_fog),
        max: lerp(a.max_world_fog, b.max_world_fog),
        color: lerp_color(a.world_fog_color, b.world_fog_color, f),
    })
}

/// The fixed-function fog factor retail's device is configured for: **`D3DFOG_LINEAR` vertex fog
/// over the radial eye distance**, 1 = unfogged, 0 = fully the fog colour.
///
/// Retail's fog render states:
///
/// | state | | value |
/// |---|---|---|
/// | `D3DRS_FOGCOLOR` | 34 (`0x22`) | `0x00AAAAAA` |
/// | `D3DRS_FOGTABLEMODE` | 35 (`0x23`) | 0 = `D3DFOG_NONE` — **no per-pixel fog** |
/// | `D3DRS_FOGSTART` | 36 (`0x24`) | `0x43C80000` = 400.0 |
/// | `D3DRS_FOGEND` | 37 (`0x25`) | `0x44FA0000` = 2000.0 |
/// | `D3DRS_FOGDENSITY` | 38 (`0x26`) | `0x3E4CCCCD` = 0.2, dead with table mode NONE |
/// | `D3DRS_RANGEFOGENABLE` | 48 (`0x30`) | **1** |
/// | `D3DRS_FOGVERTEXMODE` | 140 (`0x8C`) | **3 = `D3DFOG_LINEAR`** |
///
/// `RANGEFOGENABLE` is the one that is easy to miss and visible on a wide view: the fog coordinate
/// is `|P - eye|`, not the view-space `z`, so the corners of the screen fog as much as the centre
/// rather than less.
#[must_use]
pub fn linear_fog_factor(distance: f32, start: f32, end: f32) -> f32 {
    ((end - distance) / (end - start).max(1.0e-6)).clamp(0.0, 1.0)
}

/// Produce one [`CelestialPosition`] per sky object for time `t` in the
/// present day group.
///
/// An object whose `begin_time == end_time` is always present with `rotation = begin_angle`; one
/// **strictly inside** its window gets a linear ramp from `begin_angle` to `end_angle`; otherwise
/// its `gfx_id` is `INVALID_DID` (0) and the rotation is still computed. Then the `a` entry's
/// `SkyObjectReplace` rows override the mesh, the heading and — interpolated against the matching
/// row in `b` — the luminosity, brightness and transparency.
#[must_use]
pub fn get_sky(group: &SkyDayPreset, t: f32) -> Vec<CelestialPosition> {
    let mut out: Vec<CelestialPosition> = group
        .sky_objects
        .iter()
        .map(|so| {
            let (gfx_id, rotation) = if so.begin_time == so.end_time {
                (so.default_gfx_object, so.begin_angle)
            } else if so.begin_time < t && t < so.end_time {
                let r = so.begin_angle
                    + (t - so.begin_time) * (so.end_angle - so.begin_angle)
                        / (so.end_time - so.begin_time);
                (so.default_gfx_object, r)
            } else {
                (DataId(0), so.begin_angle)
            };
            CelestialPosition {
                gfx_id,
                pes_id: so.default_pes_object,
                heading: 0.0,
                rotation,
                tex_velocity: so.tex_velocity,
                properties: so.properties,
                ..CelestialPosition::default()
            }
        })
        .collect();

    let Some((ia, ib, f)) = get_time_of_day(&group.sky_time, t) else {
        return out;
    };
    let (a, b) = (&group.sky_time[ia], &group.sky_time[ib]);
    for r in &a.sky_obj_replace {
        let k = r.object_index as usize;
        let Some(p) = out.get_mut(k) else { continue };
        if r.gfx_obj_id.0 != 0 {
            p.gfx_id = r.gfx_obj_id;
        }
        if r.rotate != 0.0 {
            p.heading = r.rotate;
        }
        if r.luminosity > 0.0 || r.max_bright > 0.0 || r.transparent >= 0.0 {
            let Some(r2) = b
                .sky_obj_replace
                .iter()
                .find(|x| x.object_index == r.object_index)
            else {
                continue;
            };
            let lerp = |x: f32, y: f32| x + (y - x) * f;
            if r.luminosity > 0.0 && r2.luminosity > 0.0 {
                p.luminosity = lerp(r.luminosity, r2.luminosity);
            }
            if r.max_bright > 0.0 && r2.max_bright > 0.0 {
                p.max_bright = lerp(r.max_bright, r2.max_bright);
            }
            if r.transparent >= 0.0 && r2.transparent >= 0.0 {
                p.transparent = lerp(r.transparent, r2.transparent);
            }
        }
    }
    out
}

/// Calculate a celestial object's draw frame from the viewer frame, heading and rotation.
///
/// ```text
/// if heading  != 0: set frame heading in degrees about +z
/// if rotation != 0: rotate frame by (0, -rotation * π/180, 0) about local y
/// ```
///
/// The sun/moon arc is therefore a rotation about **north** by `-rotation` degrees, applied after
/// the heading; `begin_angle`/`end_angle` are the elevation sweep endpoints.
pub fn calc_frame(f: &mut Frame, heading: f32, rotation: f32) {
    if heading != 0.0 {
        dereth_terrain::math::set_heading(f, heading);
    }
    if rotation != 0.0 {
        grotate(
            f,
            Vec3::new(0.0, -rotation * std::f32::consts::PI / 180.0, 0.0),
        );
    }
}

/// Rotate about a **global** axis by `|v|` radians. A no-op when
/// `|v|² < 2e-4 * 2e-4`, which is the guard that keeps a zero omega from producing NaN.
pub fn grotate(f: &mut Frame, v: Vec3) {
    use {dereth_terrain::math::set_rotate, dereth_terrain::math::V3};
    let mag2 = v.mag2();
    if mag2 < dereth_terrain::consts::EPSILON * dereth_terrain::consts::EPSILON {
        return;
    }
    let len = mag2.sqrt();
    let inv = 1.0 / len;
    let half = len * 0.5;
    let s = math::sinf(half);
    let (rw, rx, ry, rz) = (
        math::cosf(half),
        s * v.x * inv,
        inv * s * v.y,
        s * inv * v.z,
    );
    let q = f.rotation;
    set_rotate(
        f,
        ((rw * q.w - rx * q.x) - ry * q.y) - rz * q.z,
        (rx * q.w + ry * q.z + rw * q.x) - rz * q.y,
        ry * q.w + rz * q.x + (rw * q.y - rx * q.z),
        rz * q.w + ((rw * q.z + rx * q.y) - ry * q.x),
    );
}

/// The two sky passes and their submission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyPass {
    /// Before the landscape: every object with `properties & 1` **clear**.
    Before = 0,
    /// After the landscape: the weather layer, the whole `after_sky_cell`.
    After = 1,
}

/// Device state installed for the duration of both passes.
///
/// Depth test **ALWAYS** with depth writes **off** — the sky never occludes anything and is never
/// occluded — and `zfar` multiplied by **4** so sky geometry can sit four times farther out than
/// the world. Fog is disabled unless a fog *override* is active.
pub const SKY_ZFAR_MULTIPLIER: f32 = 4.0;

/// Per-object skip rules for sky pass 0.
///
/// ```text
/// skip if (properties & 1) != 0                            // that one belongs to pass 1
/// skip if !weather_enabled and (properties & 4) != 0
/// skip if override_enabled and fog_enabled and (properties & 2) != 0
/// ```
#[must_use]
pub fn draws_in_pass_0(
    properties: u32,
    weather_enabled: bool,
    override_enabled: bool,
    fog_enabled: bool,
) -> bool {
    if properties & 1 != 0 {
        return false;
    }
    if !weather_enabled && properties & 4 != 0 {
        return false;
    }
    if override_enabled && fog_enabled && properties & 2 != 0 {
        return false;
    }
    true
}

/// The present day group of a region, at a given in-game date.
#[must_use]
pub fn present_day_group(region: &Region, year: u32, day: u32) -> Option<&SkyDayPreset> {
    let sky = region.sky_info.as_ref()?;
    let k = calc_present_day_group(
        year,
        region.game_time.days_per_year,
        day,
        sky.day_groups.len(),
    );
    sky.day_groups.get(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::region::{SkyObject, SkyObjectReplace};

    fn sky_time(begin: f32, amb: f32, dir: f32, fog: u32) -> SkyTime {
        SkyTime {
            begin,
            dir_bright: dir,
            dir_heading: 0.0,
            dir_pitch: 90.0,
            dir_color: 0x00FF_FFFF,
            amb_bright: amb,
            amb_color: 0x0080_8080,
            min_world_fog: 10.0,
            max_world_fog: 100.0,
            world_fog_color: 0x0040_4040,
            world_fog: fog,
            sky_obj_replace: Vec::new(),
        }
    }

    fn group(times: Vec<SkyTime>, objects: Vec<SkyObject>) -> SkyDayPreset {
        SkyDayPreset {
            chance_of_occur: 1.0,
            day_name: "test".into(),
            sky_objects: objects,
            sky_time: times,
        }
    }

    fn sky_object(begin_time: f32, end_time: f32, begin_angle: f32, end_angle: f32) -> SkyObject {
        SkyObject {
            begin_time,
            end_time,
            begin_angle,
            end_angle,
            tex_velocity: (0.0, 0.0),
            default_gfx_object: DataId(0x0100_0001),
            default_pes_object: DataId(0),
            properties: 0,
        }
    }

    /// Oracle: the last entry wraps to the
    /// first and uses `(t - begin) / (1 - begin)`, which is what makes midnight continuous.
    #[test]
    fn time_of_day_brackets_wrap_at_midnight() {
        let g = group(
            vec![
                sky_time(0.0, 0.1, 0.0, 0),
                sky_time(0.5, 0.9, 1.0, 0),
                sky_time(0.8, 0.2, 0.1, 0),
            ],
            Vec::new(),
        );
        assert_eq!(get_time_of_day(&g.sky_time, 0.0), Some((0, 1, 0.0)));
        assert_eq!(get_time_of_day(&g.sky_time, 0.25), Some((0, 1, 0.5)));
        assert_eq!(get_time_of_day(&g.sky_time, 0.5), Some((1, 2, 0.0)));
        // Past the last begin: wraps to entry 0 with the (1 - begin) denominator.
        let (a, b, f) = get_time_of_day(&g.sky_time, 0.9).expect("a bracket");
        assert_eq!((a, b), (2, 0));
        assert!(
            (f - 0.5).abs() < 1e-6,
            "(0.9 - 0.8) / (1 - 0.8) = 0.5, got {f}"
        );
        // No rows at all: no bracket, which sends get_lighting to its fallback.
        assert_eq!(get_time_of_day(&[], 0.5), None);
    }

    /// Oracle: sky lighting — the sun vector's **length is
    /// `dir_bright`**. This is the assertion that fails if someone normalises it.
    #[test]
    fn the_sun_vector_length_is_dir_bright() {
        let g = group(
            vec![sky_time(0.0, 0.4, 2.5, 0), sky_time(0.5, 0.4, 2.5, 0)],
            Vec::new(),
        );
        let l = get_lighting(&g, 0.25);
        assert!(
            (l.sun_vec.magnitude() - 2.5).abs() < 1e-4,
            "{:?}",
            l.sun_vec
        );
        // Halving dir_bright must halve the vector, not re-aim it.
        let g2 = group(
            vec![sky_time(0.0, 0.4, 1.25, 0), sky_time(0.5, 0.4, 1.25, 0)],
            Vec::new(),
        );
        let l2 = get_lighting(&g2, 0.25);
        assert!((l2.sun_vec.magnitude() - 1.25).abs() < 1e-4);
    }

    /// Oracle: the sun-vector formula and the client's world axes — heading 0
    /// = +y = north, increasing clockwise toward +x = east; pitch 90 degrees is straight up.
    #[test]
    fn the_sun_vector_follows_the_clients_heading_convention() {
        let mut t = sky_time(0.0, 0.4, 1.0, 0);
        t.dir_pitch = 0.0;
        t.dir_heading = 0.0;
        let g = group(vec![t.clone(), t.clone()], Vec::new());
        let l = get_lighting(&g, 0.2);
        assert!(
            l.sun_vec.y > 0.99,
            "heading 0 points north: {:?}",
            l.sun_vec
        );
        let mut t90 = t.clone();
        t90.dir_heading = 90.0;
        let g = group(vec![t90.clone(), t90], Vec::new());
        assert!(
            get_lighting(&g, 0.2).sun_vec.x > 0.99,
            "heading 90 points east"
        );
        let mut up = t;
        up.dir_pitch = 90.0;
        let g = group(vec![up.clone(), up], Vec::new());
        assert!(
            get_lighting(&g, 0.2).sun_vec.z > 0.99,
            "pitch 90 points straight up"
        );
    }

    /// Oracle: the default fallback — `ambient_level 0.3`, white,
    /// `sun_vec (0.5, 0, 0.8)` — taken when the day group has no `sky_time` rows.
    #[test]
    fn the_documented_fallback_is_reproduced() {
        let g = group(Vec::new(), Vec::new());
        let l = get_lighting(&g, 0.5);
        assert_eq!(l, SkyLighting::fallback());
        assert_eq!(l.ambient_level, 0.3);
        assert_eq!(l.ambient_color, [255, 255, 255]);
        assert_eq!(l.sun_color, [255, 255, 255]);
        assert_eq!(l.sun_vec, Vec3::new(0.5, 0.0, 0.8));
    }

    /// Oracle: world fog returns `None` unless **both** bracketing
    /// entries have `world_fog != 0`.
    #[test]
    fn fog_needs_both_bracketing_entries_to_want_it() {
        let g = group(
            vec![sky_time(0.0, 0.4, 1.0, 1), sky_time(0.5, 0.4, 1.0, 1)],
            Vec::new(),
        );
        let f = get_world_fog(&g, 0.25).expect("both entries want fog");
        assert_eq!((f.min, f.max), (10.0, 100.0));
        assert_eq!(f.color, [0x40, 0x40, 0x40]);
        let g = group(
            vec![sky_time(0.0, 0.4, 1.0, 1), sky_time(0.5, 0.4, 1.0, 0)],
            Vec::new(),
        );
        assert_eq!(
            get_world_fog(&g, 0.25),
            None,
            "one entry with world_fog 0 turns fog off"
        );
    }

    /// Oracle: sky-object selection has three branches — always-present, strictly inside the
    /// window, and absent (`INVALID_DID`, rotation still computed).
    #[test]
    fn get_sky_selects_the_documented_branch_per_object() {
        let objs = vec![
            sky_object(0.0, 0.0, 30.0, 90.0),   // begin == end: always present
            sky_object(0.25, 0.75, 0.0, 180.0), // a daytime arc
        ];
        let g = group(vec![sky_time(0.0, 0.4, 1.0, 0)], objs);
        let at_noon = get_sky(&g, 0.5);
        assert_eq!(at_noon.len(), 2);
        assert_eq!(at_noon[0].gfx_id, DataId(0x0100_0001));
        assert_eq!(
            at_noon[0].rotation, 30.0,
            "an always-present object uses begin_angle"
        );
        assert_eq!(at_noon[1].gfx_id, DataId(0x0100_0001));
        assert!(
            (at_noon[1].rotation - 90.0).abs() < 1e-4,
            "halfway through 0..180"
        );
        let at_midnight = get_sky(&g, 0.0);
        assert_eq!(
            at_midnight[0].gfx_id,
            DataId(0x0100_0001),
            "still always present"
        );
        assert_eq!(
            at_midnight[1].gfx_id,
            DataId(0),
            "outside its window: INVALID_DID"
        );
        assert_eq!(
            at_midnight[1].rotation, 0.0,
            "the rotation is still computed"
        );
        // The window is strict at both ends.
        assert_eq!(
            get_sky(&g, 0.25)[1].gfx_id,
            DataId(0),
            "begin_time itself is outside"
        );
        assert_eq!(
            get_sky(&g, 0.75)[1].gfx_id,
            DataId(0),
            "end_time itself is outside"
        );
    }

    /// Oracle: [`get_sky`]'s replace pass — a `SkyObjectReplace` overrides the mesh and the
    /// heading, and interpolates luminosity/brightness/transparency against the matching row in
    /// `b`. Values left at their "unset" sentinel (`-1`) must stay unset.
    #[test]
    fn sky_object_replacements_override_and_interpolate() {
        let objs = vec![sky_object(0.0, 0.0, 0.0, 0.0)];
        let r = |lum: f32, tr: f32| SkyObjectReplace {
            object_index: 0,
            gfx_obj_id: DataId(0x0100_0099),
            rotate: 45.0,
            transparent: tr,
            luminosity: lum,
            max_bright: -1.0,
        };
        let mut a = sky_time(0.0, 0.4, 1.0, 0);
        a.sky_obj_replace = vec![r(20.0, 0.0)];
        let mut b = sky_time(0.5, 0.4, 1.0, 0);
        b.sky_obj_replace = vec![r(60.0, 100.0)];
        let g = group(vec![a, b], objs);
        let p = get_sky(&g, 0.25);
        assert_eq!(
            p[0].gfx_id,
            DataId(0x0100_0099),
            "the replacement mesh wins"
        );
        assert_eq!(p[0].heading, 45.0, "rotate != 0 sets the fixed heading");
        assert!(
            (p[0].luminosity - 40.0).abs() < 1e-4,
            "halfway between 20 and 60"
        );
        assert!(
            (p[0].transparent - 50.0).abs() < 1e-4,
            "transparent lerps from 0, not from unset"
        );
        assert_eq!(
            p[0].max_bright, -1.0,
            "max_bright is -1 in both rows and stays unset"
        );
    }

    /// Oracle: the day group is a deterministic hash of
    /// the in-game date and **does not** consult `chance_of_occur`. Two clients on the same date
    /// must see the same weather, and consecutive dates must not all collapse to one group.
    #[test]
    fn the_day_group_is_a_deterministic_hash_of_the_date() {
        let count = 20usize;
        let a = calc_present_day_group(3, 360, 100, count);
        assert_eq!(
            a,
            calc_present_day_group(3, 360, 100, count),
            "same date, same weather"
        );
        assert!(a < count);
        let seen: std::collections::BTreeSet<usize> = (0..200)
            .map(|d| calc_present_day_group(3, 360, d, count))
            .collect();
        assert!(
            seen.len() > 5,
            "200 consecutive days used only {} groups",
            seen.len()
        );
        assert_eq!(
            calc_present_day_group(0, 0, 0, 0),
            0,
            "an empty table is index 0"
        );
    }

    /// Oracle: the heading is about +z and the rotation is a
    /// `grotate` about the frame's local **y** axis by `-rotation` degrees, applied afterwards.
    /// That is what lifts the sun off the horizon.
    #[test]
    fn calc_frame_applies_heading_then_a_rotation_about_north() {
        let mut f = Frame::default();
        calc_frame(&mut f, 0.0, 0.0);
        assert_eq!(f, Frame::default(), "both zero is a no-op");
        let mut f = Frame::default();
        calc_frame(&mut f, 90.0, 0.0);
        assert!((dereth_terrain::math::get_heading(&f) - 90.0).abs() < 0.01);
        // A rotation about local y tilts the local x axis out of the ground plane.
        let mut f = Frame::default();
        calc_frame(&mut f, 0.0, 90.0);
        let m = dereth_terrain::math::l2g(f.rotation).0;
        assert!(m[2].abs() > 0.99, "the local x axis is now vertical: {m:?}");
    }

    /// Oracle: the three skip rules for sky pass 0.
    #[test]
    fn pass_zero_skips_the_documented_objects() {
        assert!(draws_in_pass_0(0, true, false, false));
        assert!(
            !draws_in_pass_0(1, true, false, false),
            "bit 0 belongs to pass 1"
        );
        assert!(
            !draws_in_pass_0(4, false, false, false),
            "bit 2 needs weather enabled"
        );
        assert!(draws_in_pass_0(4, true, false, false));
        assert!(
            !draws_in_pass_0(2, true, true, true),
            "bit 1 hides under a fog override"
        );
        assert!(
            draws_in_pass_0(2, true, true, false),
            "but only when fog is actually on"
        );
        assert_eq!(SKY_ZFAR_MULTIPLIER, 4.0);
    }
}
