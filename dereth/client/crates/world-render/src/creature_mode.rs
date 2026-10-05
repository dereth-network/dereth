//! `CreatureMode` -- the preview space.
//!
//! Oracles: the creature-preview mode's own construction, scene initialisation, camera
//! position, field-of-view and sharp-mode switches, render, camera direction (radians and
//! degrees), light add, light direction, light clear, object add, object remove and object
//! clear, plus the UI viewport element's camera, light and render entry points.
//!
//! # What it is
//!
//! A `CreatureMode` is a **private environment cell with its own camera, lights and
//! render pass**, embedded in every viewport element (engine element type `0x0D`). Two screens
//! in this build want one and neither can draw without it: the
//! teleport tunnel's swirl, and the character-generation turntable. This
//! module is the part that has no GPU in it: the camera frame, the lights and the two global flags
//! the pass takes over. The drawing half is the host's (`dereth_scene::preview`).
//!
//! # The three things `Render` does that are easy to miss
//!
//! 1. **It enables creature mode and forces `degrades_disabled = 1`**, saving
//!    the game's own value in `game_degrades_disabled`. Both are global: object picking already
//!    reads the first ([`crate::objects::draw::check_curr_object`]) and the degrade loop
//!    reads the second. See [`DegradeSave`].
//! 2. **The projection is chosen, not inherited.** Without world-FOV following it is the space's
//!    own field of view (45 degrees); with it set the space follows the world controller -- including
//!    its view-distance mode, which makes the view-distance projection collapse apply to the tunnel
//!    as well as to the world. See the projection selection below.
//! 3. **The pass clears depth only.** The viewport element sets the device
//!    viewport to the element's rectangle and clears with flag 4. That flag maps to the depth
//!    buffer only (bit 1 is the color target, bit 2 the
//!    stencil). The colour already in the rectangle -- the panel art the UI blitted underneath --
//!    **stays**. A preview space that cleared to black would erase its own frame.
//!
//! `Render` also flips the sound listener to the camera for the duration of the object update and
//! puts it back around the `update_position` loop, zeroes
//! both light counts on the way in *and* on the way out with [`crate::lighting::use_sunlight_set`] on / off around
//! each pair, and ends by flushing the alpha list at depth 0.0 so the space's own translucent
//! subsets are flushed inside the viewport rather than with the world's.

use dereth_primitives::{Frame, Quat, Vec3};

use crate::lighting::LightType;
use crate::math::{euler_set_rotate, rotate};

/// The default ambient colour: 0.3 in each channel.
pub const DEFAULT_AMBIENT: [f32; 3] = [0.3, 0.3, 0.3];

/// The preview's field of view, 45 degrees.
///
/// The float the client uses is `0.7853982`, which is π/4 (an eighth of a turn) rounded to `f32`.
#[allow(clippy::approx_constant)]
// LINT-OK: this is the float the client compiled, read as a float, not π/4 recomputed
// at a precision the client never had. Every other constant in this module is written the same way.
pub const DEFAULT_FOV_RADIANS: f32 = 0.785_398_2;

/// The degrees-to-radians constant is
/// the same truncated `0.017453292` the rest of the client uses and **not** a full-precision one.
pub const DEG_TO_RAD: f32 = 0.017_453_292;

/// Sharp-mode LOD bias, applied to every sampler when sharp mode is enabled and
/// the texture-filtering preference is below 2.
pub const SHARP_MODE_LOD_BIAS: f32 = -1.4;

/// The `TextureFiltering` preference at or above which sharp mode does nothing.
pub const SHARP_MODE_FILTERING_LIMIT: u32 = 2;

/// One light in the creature-preview pass.
///
/// `add_light` takes only the type and the intensity; everything else is fixed at construction --
/// white, `FLT_MAX` falloff, a 360-degree cone, an identity offset frame and the direction
/// `(1, 1, 1)` until the direction setter replaces it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewLight {
    /// Light type.
    pub light_type: LightType,
    /// Light intensity.
    pub intensity: f32,
    /// Light direction. Construction leaves it at `(1, 1, 1)`.
    pub direction: Vec3,
    /// Set every color channel to opaque white (`0xFFFFFFFF`).
    pub color: [f32; 3],
    /// The bit pattern `0x7f7fffff`, i.e. `f32::MAX`.
    pub falloff: f32,
    /// 360.0, in degrees as the client writes it.
    pub cone_angle: f32,
}

impl PreviewLight {
    /// One preview light.
    #[must_use]
    pub fn new(light_type: LightType, intensity: f32) -> Self {
        Self {
            light_type,
            intensity,
            direction: Vec3::new(1.0, 1.0, 1.0),
            color: [1.0, 1.0, 1.0],
            falloff: f32::MAX,
            cone_angle: 360.0,
        }
    }
}

/// Which projection the preview pass selects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewProjection {
    /// The space follows the preview container's
    /// view-distance override, which is the view-distance projection collapse.
    ViewDistance(f32),
    /// Use the supplied field of view in radians.
    FovRadians(f32),
}

/// The two degradation flags the preview pass takes over on its first pass of a frame,
/// preserving their previous values.
///
/// ```text
/// if creature_mode == 0 {
///     saved_game_degrades_disabled = degrades_disabled;
///     degrades_disabled = 1;
///     creature_mode = 1;
/// }
/// ```
///
/// The preview pass does not put them back. The world pass restores them, so a frame with a
/// preview space and no world leaves
/// `creature_mode` set -- which is exactly the state a char-gen frame is in, and is why
/// every part of the preview is reported as pickable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DegradeSave {
    /// Whether creature-preview mode was active.
    pub creature_mode: bool,
    /// The game's own `degrades_disabled` value, parked.
    pub game_degrades_disabled: bool,
}

impl DegradeSave {
    /// Enter the preview pass with the renderer's current `degrades_disabled` value.
    /// Returns what `degrades_disabled` must become.
    pub fn enter(&mut self, degrades_disabled: bool) -> bool {
        if !self.creature_mode {
            self.game_degrades_disabled = degrades_disabled;
            self.creature_mode = true;
        }
        true
    }

    /// Leave preview mode: the world pass gets the game's parked value back.
    pub fn leave(&mut self) -> bool {
        let restored = self.game_degrades_disabled;
        self.creature_mode = false;
        restored
    }
}

/// `CreatureMode` -- one preview space.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatureMode {
    /// The preview camera's frame. Identity rotation at the origin until the camera-position and
    /// camera-direction setters move it.
    pub view_frame: Frame,
    /// The preview cell's ambient-light color.
    pub ambient: [f32; 3],
    /// Whether the preview follows the world controller's field of view.
    pub use_world_fov: bool,
    /// Whether the preview uses sharp rendering.
    pub use_sharp_mode: bool,
    /// The preview's field of view in radians.
    pub fov_radians: f32,
    /// The preview space's lights.
    pub lights: Vec<PreviewLight>,
    /// Whether scene initialization has created the preview cell. Adding the object puts the
    /// preview object into that cell, so a space whose scene was never initialised draws nothing.
    /// The scene-initialization path is the only writer.
    pub scene_initialised: bool,
}

impl Default for CreatureMode {
    fn default() -> Self {
        Self::new()
    }
}

impl CreatureMode {
    /// Construct the preview mode.
    #[must_use]
    pub fn new() -> Self {
        Self {
            view_frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
            ambient: DEFAULT_AMBIENT,
            use_world_fov: false,
            use_sharp_mode: false,
            fov_radians: DEFAULT_FOV_RADIANS,
            lights: Vec::new(),
            scene_initialised: false,
        }
    }

    /// Initialise the private environment cell; allocation never fails here.
    pub fn initialize_scene(&mut self) -> bool {
        self.scene_initialised = true;
        true
    }

    /// Set **the origin only**; the rotation is left
    /// exactly as it was.
    pub fn set_camera_position(&mut self, p: Vec3) {
        self.view_frame.origin = p;
    }

    /// Reset the rotation to identity, then rotate by the vector in **radians**.
    pub fn set_camera_direction(&mut self, v: Vec3) {
        euler_set_rotate(&mut self.view_frame, 0.0, 0.0, 0.0);
        rotate(&mut self.view_frame, v);
    }

    /// Set the camera direction in degrees, multiplying each component
    /// multiplied by [`DEG_TO_RAD`] first.
    ///
    /// The teleport tunnel calls this every frame with
    /// `(0, current tunnel rotation angle, 0)`, which is the portal's spin.
    pub fn set_camera_direction_degrees(&mut self, v: Vec3) {
        self.set_camera_direction(Vec3::new(
            v.x * DEG_TO_RAD,
            v.y * DEG_TO_RAD,
            v.z * DEG_TO_RAD,
        ));
    }

    /// One-way: nothing clears it but [`Self::set_fov`].
    pub fn use_world_fov(&mut self) {
        self.use_world_fov = true;
    }

    /// A field of view of the space's own, `radians` high, in place of the world's.
    pub fn set_fov(&mut self, radians: f32) {
        self.use_world_fov = false;
        self.fov_radians = radians;
    }

    /// Switch to sharp mode. Also one-way.
    pub fn use_sharp_mode(&mut self) {
        self.use_sharp_mode = true;
    }

    /// Add a light.
    pub fn add_light(&mut self, light_type: LightType, intensity: f32) {
        self.lights.push(PreviewLight::new(light_type, intensity));
    }

    /// Set one light direction; false when the index is past the end, which
    /// is the client's own bounds test and not an assertion.
    pub fn set_light_direction(&mut self, i: usize, v: Vec3) -> bool {
        match self.lights.get_mut(i) {
            Some(l) => {
                l.direction = v;
                true
            }
            None => false,
        }
    }

    /// Remove every light.
    pub fn remove_all_lights(&mut self) {
        self.lights.clear();
    }

    /// Replace every light with one light of the requested type, intensity, and direction.
    ///
    /// Character generation calls this each time it rebuilds the turntable:
    /// `set_light(DISTANT_LIGHT, 2.0, (0.3, 1.9, 0.65))`. Note the **positive** y, where the
    /// character-preview screen's portal space uses `(0.3, -1.9, 0.65)`.
    pub fn set_light(&mut self, light_type: LightType, intensity: f32, direction: Vec3) {
        self.remove_all_lights();
        self.add_light(light_type, intensity);
        self.set_light_direction(0, direction);
    }

    /// The render's projection arm, verbatim.
    ///
    /// `world_view_distance` is the override when view-distance mode is set and `None`
    /// otherwise; `world_fov` is the game FOV and `viewport_aspect` is the device aspect
    /// ratio. The `- 0.1` is preserved and is the same bias
    /// `dereth_render::camera::fov_y_from_preference` applies to the world.
    #[must_use]
    pub fn projection(
        &self,
        world_view_distance: Option<f32>,
        world_fov: f32,
        viewport_aspect: f32,
    ) -> PreviewProjection {
        if !self.use_world_fov {
            return PreviewProjection::FovRadians(self.fov_radians);
        }
        match world_view_distance {
            Some(d) => PreviewProjection::ViewDistance(d),
            None => PreviewProjection::FovRadians(world_fov / (viewport_aspect - 0.1)),
        }
    }

    /// The mipmap LOD bias this pass installs, if any --
    /// Apply [`SHARP_MODE_LOD_BIAS`] when sharp
    /// mode is on and the filtering preference is under [`SHARP_MODE_FILTERING_LIMIT`].
    #[must_use]
    pub fn lod_bias(&self, texture_filtering: u32) -> Option<f32> {
        (self.use_sharp_mode && texture_filtering < SHARP_MODE_FILTERING_LIMIT)
            .then_some(SHARP_MODE_LOD_BIAS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the preview mode's constructor.
    #[test]
    fn the_constructor_is_the_one_in_the_binary() {
        let c = CreatureMode::new();
        assert_eq!(c.ambient, [0.3, 0.3, 0.3]);
        #[allow(clippy::approx_constant)]
        let expected_fov = 0.785_398_2;
        assert_eq!(c.fov_radians, expected_fov);
        assert!(
            !c.use_world_fov,
            "following the world view's field of view is opt-in"
        );
        assert!(!c.use_sharp_mode);
        assert!(c.lights.is_empty());
        assert_eq!(c.view_frame.origin, Vec3::ZERO);
        assert!(
            !c.scene_initialised,
            "scene initialization belongs to viewport post-initialization"
        );
    }

    // Oracle: adding a light fixes every field except the requested type and intensity.
    #[test]
    fn a_light_is_white_with_a_float_max_falloff_and_a_full_cone() {
        let mut c = CreatureMode::new();
        c.add_light(LightType::Directional, 2.0);
        let l = c.lights[0];
        assert_eq!(l.light_type, LightType::Directional, "DISTANT_LIGHT is 1");
        assert_eq!(l.intensity, 2.0);
        assert_eq!(l.color, [1.0, 1.0, 1.0]);
        assert_eq!(
            l.falloff,
            f32::MAX,
            "the literal is the bit pattern 0x7f7fffff"
        );
        assert_eq!(l.cone_angle, 360.0);
        assert_eq!(
            l.direction,
            Vec3::new(1.0, 1.0, 1.0),
            "the (1,1,1) direction is set before the caller's"
        );
        assert!(
            !c.set_light_direction(1, Vec3::ZERO),
            "the index is bounds-tested, not asserted"
        );
        assert!(c.set_light_direction(0, Vec3::new(0.3, -1.9, 0.65)));
        assert_eq!(c.lights[0].direction, Vec3::new(0.3, -1.9, 0.65));
    }

    // Oracle: writes three floats and nothing else.
    #[test]
    fn setting_the_camera_position_leaves_the_rotation_alone() {
        let mut c = CreatureMode::new();
        c.set_camera_direction_degrees(Vec3::new(0.0, 90.0, 0.0));
        let spun = c.view_frame.rotation;
        c.set_camera_position(Vec3::new(0.24, -2.7, 0.88));
        assert_eq!(c.view_frame.origin, Vec3::new(0.24, -2.7, 0.88));
        assert_eq!(
            c.view_frame.rotation, spun,
            "the position setter does not touch the rotation"
        );
    }

    // Oracle: setting the camera direction resets to identity, then rotates. Two calls
    // in a row must not compose, which is what the `euler_set_rotate(0,0,0)` head is for.
    #[test]
    fn the_camera_spin_is_absolute_and_not_accumulated() {
        let mut a = CreatureMode::new();
        a.set_camera_direction_degrees(Vec3::new(0.0, 90.0, 0.0));
        let once = a.view_frame.rotation;

        let mut b = CreatureMode::new();
        b.set_camera_direction_degrees(Vec3::new(0.0, 45.0, 0.0));
        b.set_camera_direction_degrees(Vec3::new(0.0, 90.0, 0.0));
        let d = (once.w - b.view_frame.rotation.w).abs()
            + (once.x - b.view_frame.rotation.x).abs()
            + (once.y - b.view_frame.rotation.y).abs()
            + (once.z - b.view_frame.rotation.z).abs();
        assert!(d < 1e-6, "45 then 90 must be 90, not 135: {d}");

        // ... and a zero direction is the identity used by the character-generation turntable.
        let mut z = CreatureMode::new();
        z.set_camera_direction(Vec3::ZERO);
        assert_eq!(z.view_frame.rotation, Quat::IDENTITY);
    }

    // Oracle: the preview projection's three-way FOV arm.
    #[test]
    fn the_projection_arm_follows_the_world_view_only_when_asked() {
        let mut c = CreatureMode::new();
        // Without `use_world_fov` the space keeps its own 45 degrees whatever the smart box does.
        assert_eq!(
            c.projection(Some(0.001), 1.5, 1.333),
            PreviewProjection::FovRadians(DEFAULT_FOV_RADIANS)
        );
        c.use_world_fov();
        assert_eq!(
            c.projection(Some(0.001), 1.5, 1.333),
            PreviewProjection::ViewDistance(0.001)
        );
        // With it and no override, the smart box's game FOV over the aspect, less the 0.1 bias.
        let PreviewProjection::FovRadians(f) = c.projection(None, 1.5, 1.333) else {
            panic!("no override means a plain FOV");
        };
        assert!((f - 1.5 / (1.333 - 0.1)).abs() < 1e-6, "{f}");
    }

    // Oracle: preview entry and the world pass's corresponding restore.
    #[test]
    fn the_degrade_flag_is_saved_once_and_handed_back_by_the_world_pass() {
        let mut s = DegradeSave::default();
        // The game had degrades on (disabled == false). The first preview pass of the frame parks
        // that and forces the flag.
        assert!(
            s.enter(false),
            "the pass always forces degrades_disabled = 1"
        );
        assert!(s.creature_mode);
        assert!(!s.game_degrades_disabled);
        // A second space in the same frame must not overwrite the parked value with the forced one.
        assert!(s.enter(true));
        assert!(
            !s.game_degrades_disabled,
            "the save happens once, on the creature_mode edge"
        );
        assert!(!s.leave(), "the world pass gets the game's own value back");
        assert!(!s.creature_mode);
    }

    // Oracle: the render path's sharp-mode branch.
    #[test]
    fn sharp_mode_biases_the_lod_only_below_the_filtering_preference() {
        let mut c = CreatureMode::new();
        assert_eq!(c.lod_bias(0), None, "off by default");
        c.use_sharp_mode();
        assert_eq!(c.lod_bias(0), Some(-1.4));
        assert_eq!(c.lod_bias(1), Some(-1.4));
        assert_eq!(
            c.lod_bias(2),
            None,
            "TextureFiltering >= 2 leaves the samplers alone"
        );
    }

    // Oracle: setting a light removes all existing lights, adds one, and points it.
    #[test]
    fn the_viewport_light_setter_replaces_rather_than_appends() {
        let mut c = CreatureMode::new();
        c.set_light(LightType::Directional, 2.0, Vec3::new(0.3, 1.9, 0.65));
        c.set_light(LightType::Directional, 2.0, Vec3::new(0.3, 1.9, 0.65));
        assert_eq!(
            c.lights.len(),
            1,
            "each rebuild replaces the viewport light"
        );
        assert_eq!(c.lights[0].direction, Vec3::new(0.3, 1.9, 0.65));
    }
}
