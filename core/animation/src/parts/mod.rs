//! Part arrays and renderable physics parts.
//!
//! Two rules dominate this module:
//!
//! * **Parts are placed from the animation frame alone** — `part_frames[floor(frame_number)]`, with
//!   **no parent composition**. The setup's parent index is data, not a transform
//!   chain; composing it would double every rotation. The client has no runtime reader of it, and
//!   [`crate::data::SetupData`] stores it and uses it for nothing.
//! * **Copy on write.** A part's material is `None` while it uses the gfxobj's unchanged, and the
//!   clone is released again as soon as the overrides match the gfxobj's own values — so an object
//!   faded out and back in ends up sharing the original material again.
//!
//! Nothing here draws. The gfxobj array, surfaces, and billboard `draw_pos` belong to
//! renderer Tracks C and D; what this crate keeps is the part's placement, its LOD
//! level and the appearance overrides the animation hooks write.

pub mod degrade;
pub mod objdesc;

use std::sync::Arc;

use dereth_primitives::{DataId, Frame, Vec3};

use crate::data::{AnimAssets, CylSphere, DegradeInfo, GfxObjLookup, SetupData, Sphere};
use crate::frame::{combine, V3};
use crate::seq::Sequence;

pub use degrade::{get_degrade, get_max_degrade_distance, DegradeSettings, S_R_DEGRADE_DISTANCE};
pub use objdesc::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};

/// The default appearance values, when the gfxobj has no
/// material of its own.
pub const DEFAULT_TRANSLUCENCY: f32 = 0.0;
pub const DEFAULT_DIFFUSE: f32 = 1.0;
pub const DEFAULT_LUMINOSITY: f32 = 0.0;

/// The selection blink's two lighting settings, `(luminosity, diffuse)`, as both producers hand
/// them to the lighting operation.
///
/// Read out of the client's own read-only data: the world producer
/// pushes `0.99` and `1.0` for the high lighting mode and
/// `0.0` and `0.35` for the low one; the paper-doll producer uses `0.99` and `1.0` for
/// one pair and `0.0` and `0.35` for the other. The same four numbers, twice.
///
/// The first value controls material luminosity (`D3DMATERIAL9.Emissive.rgb`); the second controls
/// diffuse intensity (`Diffuse.rgb`).
pub const SELECTION_HIGH_LIGHTING: (f32, f32) = (0.99, 1.0);
/// See [`SELECTION_HIGH_LIGHTING`].
pub const SELECTION_LOW_LIGHTING: (f32, f32) = (0.0, 0.35);
/// Seconds between flips -- one constant for the world and one for the paper doll, both
/// `f64 0.2`.
pub const SELECTION_FLIP_INTERVAL: f64 = 0.2;

/// `LightingMode` — the smart-box lighting operation's argument, with the
/// retail values (restore = 0, low = 1, high = 2). The
/// doll producer passes the numbers directly; it is shared here because both producers
/// apply the same two settings to the physics body's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightingMode {
    /// Restore the part's lighting to its defaults.
    Restore = 0,
    /// `SetLighting(0.0, 0.35)` — dim.
    Low = 1,
    /// `SetLighting(0.99, 1.0)` — bright.
    High = 2,
}

/// The three per-part appearance overrides the animation hooks write.
///
/// `material == None` in the client means "use the gfxobj's own material"; here the whole struct is
/// `None` for the same reason. The client clones the surface state on the first override and drops
/// the clone when the values return to the defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialOverride {
    pub translucency: f32,
    pub diffuse: f32,
    pub luminosity: f32,
}

/// One renderable physics part.
#[derive(Debug, Clone)]
pub struct PhysicsPart {
    /// The gfxobj this part draws. replaces it.
    pub gfxobj_id: DataId,
    /// The LOD table, or `None`.
    pub degrades: Option<Arc<DegradeInfo>>,
    /// The current LOD index and its billboard mode.
    pub deg_level: u32,
    pub deg_mode: i32,
    /// Bit 0 = NoDraw.
    pub draw_state: i32,
    /// Per-part scale; `(1,1,1)` unless the setup or `SetScaleInternal` says otherwise.
    pub gfxobj_scale: Vec3,
    /// World placement, written every frame by [`PartArray::update_parts`].
    pub pos: Frame,
    /// The current overrides, or `None` while the gfxobj's own material is used.
    pub material: Option<MaterialOverride>,
    /// The surface-override set is allocated only once a texture or palette override exists.
    pub surface_overrides: Option<SurfaceOverrides>,
    pub original_palette_id: DataId,
    /// Distance from the viewer.
    pub viewer_distance: f32,
    pub physobj_index: u32,
}

/// What answered. Three states, never two — see
/// [`GfxObjLookup`].
#[derive(Debug, Clone, PartialEq)]
pub enum GfxObjArrayLoad {
    /// The successful lookup result carries the degradation record that the new object
    /// names, or `None` when it names none.
    Loaded { degrades: Option<Arc<DegradeInfo>> },
    /// The client's `return 0` — the `GfxObj` is not in the dat, or the degrade record's level 0
    /// resolves to NULL. `SetPart` then leaves the part untouched and reports failure.
    Failed,
    /// The asset source carries no `GfxObj` index, so nothing was looked up. Not a failure and
    /// not a success: the swap is accepted and the part's degrade record is left as it was.
    NotAsked,
}

/// The copied surface array: texture-map substitutions and the applied shift palette.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SurfaceOverrides {
    /// Replace `old` with `new` on every surface using that original texture.
    pub texture_maps: Vec<(DataId, DataId)>,
    /// The shift palette, if one was applied.
    pub shift_palette: Option<DataId>,
    /// The sub-palette ranges applied on top of it, in application order.
    pub subpalettes: Vec<PaletteRange>,
}

impl PhysicsPart {
    /// Make one physics part.
    #[must_use]
    pub fn new(gfxobj_id: DataId) -> Self {
        Self {
            gfxobj_id,
            degrades: None,
            deg_level: 0,
            deg_mode: 1,
            draw_state: 0,
            gfxobj_scale: Vec3::new(1.0, 1.0, 1.0),
            pos: Frame::default(),
            material: None,
            surface_overrides: None,
            original_palette_id: DataId(0),
            viewer_distance: 0.0,
            physobj_index: 0,
        }
    }

    /// Set the no-draw flag -- `draw_state` bit 0.
    pub fn set_no_draw(&mut self, on: bool) {
        if on {
            self.draw_state |= 1;
        } else {
            self.draw_state &= !1;
        }
    }

    #[must_use]
    pub const fn no_draw(&self) -> bool {
        self.draw_state & 1 != 0
    }

    fn current(&self) -> MaterialOverride {
        self.material.unwrap_or(MaterialOverride {
            translucency: DEFAULT_TRANSLUCENCY,
            diffuse: DEFAULT_DIFFUSE,
            luminosity: DEFAULT_LUMINOSITY,
        })
    }

    /// Whether the current settings are the defaults.
    fn settings_are_default(m: MaterialOverride) -> bool {
        m.translucency == DEFAULT_TRANSLUCENCY
            && m.diffuse == DEFAULT_DIFFUSE
            && m.luminosity == DEFAULT_LUMINOSITY
    }

    fn store(&mut self, m: MaterialOverride) {
        if Self::settings_are_default(m) {
            // drop the clone and share the gfxobj's again.
            self.material = None;
        } else {
            self.material = Some(m);
        }
    }

    /// Set the part's translucency.
    ///
    /// **1.0 means fully invisible**, not opaque, and it sets NoDraw rather than storing a value.
    /// `locked` is the caller's `physobj->state & 0x100000`.
    pub fn set_translucency(&mut self, t: f32, locked: bool) {
        if locked {
            return;
        }
        if t == 1.0 {
            self.set_no_draw(true);
            return;
        }
        self.set_no_draw(false);
        let mut m = self.current();
        m.translucency = t;
        self.store(m);
    }

    /// Set the part's luminosity.
    pub fn set_luminosity(&mut self, l: f32) {
        let mut m = self.current();
        m.luminosity = l;
        self.store(m);
    }

    /// Set the part's diffusion.
    pub fn set_diffusion(&mut self, d: f32) {
        let mut m = self.current();
        m.diffuse = d;
        self.store(m);
    }

    /// Set the part's lighting -- both at once.
    pub fn set_lighting(&mut self, luminosity: f32, diffuse: f32) {
        let mut m = self.current();
        m.luminosity = luminosity;
        m.diffuse = diffuse;
        self.store(m);
    }

    /// Restore the part's lighting.
    pub fn restore_lighting(&mut self) {
        self.set_lighting(DEFAULT_LUMINOSITY, DEFAULT_DIFFUSE);
    }

    /// Copies the surface array on first use.
    pub fn set_texture_map(&mut self, old: DataId, new: DataId) {
        let s = self
            .surface_overrides
            .get_or_insert_with(SurfaceOverrides::default);
        // A second substitution of the same original replaces the first, as re-calling
        // `UseTextureMap` on the same surface would.
        match s.texture_maps.iter_mut().find(|(o, _)| *o == old) {
            Some(e) => e.1 = new,
            None => s.texture_maps.push((old, new)),
        }
    }

    /// Use a palette on the part.
    ///
    /// The shift palette **replaces** whatever was there: the old palette reference is released,
    /// then the new palette reference is retained. Because the argument is one built palette, a second
    /// call cannot leave the first call's ranges behind. Appending instead would be harmless only
    /// while every caller runs `restore_palette` first, and [`PhysicsPart::set_part`] re-applies
    /// the palette without a restore in front of it.
    pub fn use_palette(&mut self, palette: DataId, ranges: &[PaletteRange]) {
        let s = self
            .surface_overrides
            .get_or_insert_with(SurfaceOverrides::default);
        s.shift_palette = Some(palette);
        s.subpalettes.clear();
        s.subpalettes.extend_from_slice(ranges);
    }

    /// Swap the gfxobj this part draws.
    ///
    /// **The swap is not just an id.** A nonzero id must load a valid object array; degradation
    /// information is optional. The installation order matters:
    ///
    ///
    /// ```text
    /// restore the part's default surfaces
    /// ..
    /// install the loaded object's surface array
    /// ..
    /// if a shift palette exists, apply it again
    /// ```
    ///
    /// Installing the new part releases the part's **custom surface array** and points
    /// `surfaces` back at the gfxobj's own, so **every texture-map substitution on this part is
    /// gone**; the shift palette is a separate field, survives, and is re-applied to the new
    /// surfaces by the tail. Assigning `gfxobj_id` alone would leave the previous garment's in
    /// place on the new mesh.
    ///
    /// That is not academic. In the recorded session, the player's own eight
    /// `0xF625 Item_ObjDescEvent`s include one that takes footwear off, and without the restore
    /// the boots' substitutions stay (`0x05000CC0 -> 0x050003CE` on both lower legs and
    /// `0x050003DC -> 0x050003CE` on both feet) on parts 3, 4, 7 and 8 — the two lower legs and
    /// the two feet, named by their own placement-frame heights in `Setup 0x02000001`.
    ///
    /// `INVALID_DID` is a zero-initialised global that retail never writes,
    /// so it is `0`, and a swap naming it fails and leaves the part alone.
    ///
    /// **The asset lookup's two results.** Both are load-bearing: on
    /// failure the part is left **exactly** as it was and reports
    /// failure through object-description application; on success the **new** object's
    /// `GfxObjDegradeInfo` is installed for render-distance selection to read every frame.
    /// Without the lookup a swap naming an absent object would be accepted and draw nothing where
    /// the client keeps the previous limb, and a swapped part would keep the **old** part's
    /// degrade record forever.
    ///
    /// `assets` answers in three states and all three are distinguished — see
    /// [`GfxObjLookup`]. With a source that has no `GfxObj` index the swap is accepted and the
    /// degrade record is left alone, deliberately, because
    /// "could not look" is not "the dat does not hold it".
    pub fn set_part(&mut self, gfxobj: DataId, assets: &dyn AnimAssets) -> bool {
        if gfxobj == DataId(0) {
            return false;
        }
        match Self::load_gfxobj_array(gfxobj, assets) {
            // Loading the new object's array failed: the array is never replaced, so the id,
            // the surfaces, the palette and the degrade record are all still the old part's.
            GfxObjArrayLoad::Failed => false,
            GfxObjArrayLoad::Loaded { degrades } => {
                self.set_gfxobj_array(gfxobj, Some(degrades));
                true
            }
            GfxObjArrayLoad::NotAsked => {
                self.set_gfxobj_array(gfxobj, None);
                true
            }
        }
    }

    /// Load a graphics-object array, reduced to the part of it this track can hold.
    ///
    /// Retail fetches the `GfxObj` itself and fails if it is absent; then fetches its degrade
    /// record. With no record the array is the one object; with a record it holds one entry per
    /// level, null where the level names `INVALID_DID` and otherwise the fetched object. If entry 0
    /// is null the array is released and the load fails; otherwise it succeeds.
    ///
    /// So there are **two** ways to fail, not one, and the second is easy to miss: a record whose
    /// **level 0** names `INVALID_DID` — or an object the dat does not hold — fails the load even
    /// though the part's own `GfxObj` is present. The client then keeps the old part.
    ///
    /// This crate resolves no geometry, so `array[i]` for `i > 0` is not modelled: its
    /// consumer of the array is the *degrade record*, while rendering loads the meshes.
    #[must_use]
    pub fn load_gfxobj_array(gfxobj: DataId, assets: &dyn AnimAssets) -> GfxObjArrayLoad {
        let did_degrade = match assets.gfxobj(gfxobj) {
            GfxObjLookup::Unknown => return GfxObjArrayLoad::NotAsked,
            // The graphics-object fetch returned NULL at the first test.
            GfxObjLookup::Absent => return GfxObjArrayLoad::Failed,
            GfxObjLookup::Present { did_degrade } => did_degrade,
        };
        let Some(did) = did_degrade else {
            // No `GfxObjDegradeInfo`: the one-element array holds the object itself, which the
            // test above already proved present, so `array[0] != NULL` and the load succeeds.
            return GfxObjArrayLoad::Loaded { degrades: None };
        };
        let Some(info) = assets.degrade_info(did) else {
            // A null degrade record takes the same one-element branch: the record the graphics
            // object names is missing, not the object.
            return GfxObjArrayLoad::Loaded { degrades: None };
        };
        // `array[0]` comes from `degrades[0].gfxobj_id`, and NULL there fails the whole load.
        let level0 = info.degrades.first().map_or(DataId(0), |g| g.gfxobj_id);
        if level0 == DataId(0) || assets.gfxobj(level0) == GfxObjLookup::Absent {
            return GfxObjArrayLoad::Failed;
        }
        GfxObjArrayLoad::Loaded {
            degrades: Some(info),
        }
    }

    /// Set the part's graphics-object array.
    ///
    /// `degrades = None` means "the caller could not look", in which case the part keeps whatever
    /// record it had; `Some(d)` stores the new record after releasing the old array, which is
    /// the line that makes a swapped part carry the **new** object's LOD table.
    ///
    /// `deg_level` and `deg_mode` are deliberately **not** reset: retail's array replacement does
    /// not touch them either, and rewrites both before the next `Draw`.
    fn set_gfxobj_array(&mut self, gfxobj: DataId, degrades: Option<Option<Arc<DegradeInfo>>>) {
        self.gfxobj_id = gfxobj;
        if let Some(d) = degrades {
            self.degrades = d;
        }
        // Restoring the surfaces: the custom array goes, so the texture maps go with it. The shift
        // palette is a separate field, not part of that array, and is re-applied below.
        let shift = self
            .surface_overrides
            .as_ref()
            .and_then(|s| s.shift_palette.map(|p| (p, s.subpalettes.clone())));
        self.surface_overrides = None;
        if let Some((palette, ranges)) = shift {
            self.use_palette(palette, &ranges);
        }
    }

    /// Restore the part's palette.
    pub fn restore_palette(&mut self) {
        if let Some(s) = &mut self.surface_overrides {
            s.shift_palette = None;
            s.subpalettes.clear();
            if s.texture_maps.is_empty() {
                // Free the clone once nothing overrides it.
                self.surface_overrides = None;
            }
        }
    }

    /// Graphics object this part **draws** at `level`.
    ///
    /// Not `gfxobj_id`. Loading fills the array from the
    /// *record*, not from the object that names it:
    ///
    /// For level `i`, loading indexes the record's degradation array at stride `0x14`, reads that
    /// row's graphics-object id, and fetches the named object into the draw array.
    ///
    /// Indeed, 229 of the 4,131 shipped records name a level-0 mesh that is **not** the object
    /// pointing at them. On `Setup 0x02000001` that is 16 of the body's 34 parts: the
    /// setup names the older mesh and the record puts the high-poly one at level 0 with the setup's
    /// own id at level 1.
    ///
    /// The clamp is verbatim from the draw path: no record, or a level past the
    /// end of one, is level 0:
    ///
    /// With no degradation record, or when `deg_level` is past the record's level count, drawing
    /// uses level 0. A valid level whose graphics-object entry is NULL draws nothing.
    ///
    /// `None` is that last line: the level is real and selecting it means *draw nothing*, which is
    /// not the same as falling back to another level.
    #[must_use]
    pub fn gfxobj_at(&self, level: u32) -> Option<DataId> {
        // `LoadGfxObjArray`'s one-element array, whose `array[0]` is the object itself. The
        // `je ` above sends every level to 0 here, so the argument does not matter.
        let Some(d) = &self.degrades else {
            return Some(self.gfxobj_id);
        };
        let i = usize::try_from(level).unwrap_or(0);
        let i = if i < d.degrades.len() { i } else { 0 };
        let id = d.degrades.get(i).map_or(DataId(0), |g| g.gfxobj_id);
        (id != DataId(0)).then_some(id)
    }

    /// **100.0** when there is no degrade info.
    #[must_use]
    pub fn max_degrade_distance(&self) -> f32 {
        self.degrades
            .as_ref()
            .map_or(100.0, |d| get_max_degrade_distance(d))
    }
}

/// A part array — the visible body of a physics object.
#[derive(Debug, Clone)]
pub struct PartArray {
    pub setup: Option<Arc<SetupData>>,
    pub parts: Vec<PhysicsPart>,
    /// Part-array scale, default `(1,1,1)`.
    pub scale: Vec3,
    /// `pa_state & 0x10000` — "some part has a physics BSP".
    pub has_physics_bsp: bool,
}

impl Default for PartArray {
    fn default() -> Self {
        Self {
            setup: None,
            parts: Vec::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
            has_physics_bsp: false,
        }
    }
}

impl PartArray {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the parts from a setup and select the default
    /// placement frame (id **101**).
    ///
    /// `init_parts` is the client's flag; a false value leaves `parts` empty, which is the
    /// particle-emitter path.
    pub fn create_setup(
        setup: Arc<SetupData>,
        init_parts: bool,
        seq: &mut Sequence,
        assets: &dyn AnimAssets,
    ) -> Option<Self> {
        let mut pa = Self {
            has_physics_bsp: setup.has_physics_bsp,
            ..Self::new()
        };
        pa.setup = Some(Arc::clone(&setup));
        if init_parts && !pa.init_parts(assets) {
            return None;
        }
        pa.set_placement_frame(0x65, seq);
        Some(pa)
    }

    /// A missing gfxobj fails the **whole** array.
    ///
    /// The client allocates each physics part and immediately attempts to install its object; a
    /// failed installation deletes that part, and array initialization stops at the first failure.
    /// Each part goes through `set_part`, which is also the only writer of
    /// `PhysicsPart::degrades`: [`PhysicsPart::new`] initialises it to `None`, and without this
    /// path `max_degrade_distance` would answer **100.0** for every part of every object.
    pub fn init_parts(&mut self, assets: &dyn AnimAssets) -> bool {
        let Some(setup) = self.setup.clone() else {
            return false;
        };
        if setup.parts.is_empty() {
            return false;
        }
        let mut parts = Vec::with_capacity(setup.parts.len());
        for (i, id) in setup.parts.iter().enumerate() {
            let mut p = PhysicsPart::new(*id);
            if !p.set_part(*id, assets) {
                // The part could not be made, so fewer parts than `num_parts` were built and the
                // whole array fails. The client leaks nothing here; nor do we.
                return false;
            }
            p.physobj_index = u32::try_from(i).unwrap_or(0);
            parts.push(p);
        }
        self.parts = parts;
        if let Some(scales) = &setup.default_scale {
            for (p, s) in self.parts.iter_mut().zip(scales.iter()) {
                p.gfxobj_scale = *s;
            }
        }
        true
    }

    /// The requested id, then key **0**, then clear.
    pub fn set_placement_frame(&mut self, id: u32, seq: &mut Sequence) -> bool {
        let Some(setup) = &self.setup else {
            seq.set_placement_frame(None, 0);
            return false;
        };
        if let Some(f) = setup.placement_frames.get(&id) {
            seq.set_placement_frame(Some(f.clone()), id);
            return true;
        }
        if let Some(f) = setup.placement_frames.get(&0) {
            seq.set_placement_frame(Some(f.clone()), 0);
            return true;
        }
        seq.set_placement_frame(None, 0);
        false
    }

    /// Update the parts.
    ///
    /// Combining the parent and animation frames multiplies the animation-frame origin
    /// component-wise by the part-array scale, transforms it by the parent's rotation, adds the
    /// parent origin and multiplies the quaternions. **Nothing here interpolates.**
    pub fn update_parts(&mut self, world: &Frame, seq: &Sequence) {
        let Some(af) = seq.get_curr_animframe() else {
            return;
        };
        let n = self.parts.len().min(af.frames.len());
        for i in 0..n {
            let a = af.frames[i];
            let scaled = Frame::new(a.origin.mul_componentwise(self.scale), a.rotation);
            self.parts[i].pos = combine(world, &scaled);
        }
    }

    /// Set the part array's scale.
    pub fn set_scale_internal(&mut self, v: Vec3) {
        self.scale = v;
        let default_scale = self.setup.as_ref().and_then(|s| s.default_scale.clone());
        for (i, p) in self.parts.iter_mut().enumerate() {
            p.gfxobj_scale = match &default_scale {
                Some(d) => d
                    .get(i)
                    .copied()
                    .unwrap_or(Vec3::new(1.0, 1.0, 1.0))
                    .mul_componentwise(v),
                None => v,
            };
        }
    }

    // ---- the geometric queries physics asks for ----------------------------------------------
    //
    // All of them scale by `scale.z` only. That is a deliberate simplification in the original: the
    // object's physical size follows the z component and nothing else. Do not "fix" it with a
    // uniform-scale abstraction.

    /// The part array's radius.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.setup.as_ref().map_or(0.0, |s| s.radius * self.scale.z)
    }

    /// The part array's height.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.setup.as_ref().map_or(0.0, |s| s.height * self.scale.z)
    }

    /// **0.01** when there is no setup.
    #[must_use]
    pub fn step_up_height(&self) -> f32 {
        self.setup
            .as_ref()
            .map_or(0.01, |s| s.step_up_height * self.scale.z)
    }

    /// **0.01** when there is no setup.
    #[must_use]
    pub fn step_down_height(&self) -> f32 {
        self.setup
            .as_ref()
            .map_or(0.01, |s| s.step_down_height * self.scale.z)
    }

    /// Whether the part array allows free heading.
    #[must_use]
    pub fn allows_free_heading(&self) -> bool {
        self.setup.as_ref().is_some_and(|s| s.allow_free_heading)
    }

    /// The setup's collision spheres, unscaled and **not**
    /// following the animation.
    #[must_use]
    pub fn spheres(&self) -> &[Sphere] {
        self.setup.as_ref().map_or(&[], |s| s.spheres.as_slice())
    }

    /// The part array's cylinder sphere.
    #[must_use]
    pub fn cylspheres(&self) -> &[CylSphere] {
        self.setup.as_ref().map_or(&[], |s| s.cylspheres.as_slice())
    }

    /// A zero sphere when there is no setup.
    #[must_use]
    pub fn sorting_sphere(&self) -> Sphere {
        self.setup
            .as_ref()
            .map_or(Sphere::default(), |s| s.sorting_sphere)
    }

    /// The centre scales **component-wise**, the
    /// radius by `scale.z` only.
    #[must_use]
    pub fn selection_sphere(&self) -> Sphere {
        let s = self
            .setup
            .as_ref()
            .map_or(Sphere::default(), |s| s.selection_sphere);
        Sphere {
            center: s.center.mul_componentwise(self.scale),
            radius: s.radius * self.scale.z,
        }
    }

    // ---- the blanket setters the animation hooks target --------------------------------------

    /// All of these are no-ops without a setup.
    pub fn set_no_draw_internal(&mut self, on: bool) {
        if self.setup.is_none() {
            return;
        }
        for p in &mut self.parts {
            p.set_no_draw(on);
        }
    }

    /// Set the part array's translucency.
    pub fn set_translucency_internal(&mut self, t: f32, locked: bool) {
        if self.setup.is_none() {
            return;
        }
        for p in &mut self.parts {
            p.set_translucency(t, locked);
        }
    }

    /// Returns false for a bad index.
    pub fn set_part_translucency_internal(&mut self, i: u32, t: f32, locked: bool) -> bool {
        match self.parts.get_mut(usize::try_from(i).unwrap_or(usize::MAX)) {
            Some(p) => {
                p.set_translucency(t, locked);
                true
            }
            None => false,
        }
    }

    /// Set the part array's luminosity.
    pub fn set_luminosity_internal(&mut self, l: f32) {
        if self.setup.is_none() {
            return;
        }
        for p in &mut self.parts {
            p.set_luminosity(l);
        }
    }

    /// Set one part's luminosity.
    pub fn set_part_luminosity_internal(&mut self, i: u32, l: f32) -> bool {
        match self.parts.get_mut(usize::try_from(i).unwrap_or(usize::MAX)) {
            Some(p) => {
                p.set_luminosity(l);
                true
            }
            None => false,
        }
    }

    /// Set the part array's diffusion.
    pub fn set_diffusion_internal(&mut self, d: f32) {
        if self.setup.is_none() {
            return;
        }
        for p in &mut self.parts {
            p.set_diffusion(d);
        }
    }

    /// Set one part's diffusion.
    pub fn set_part_diffusion_internal(&mut self, i: u32, d: f32) -> bool {
        match self.parts.get_mut(usize::try_from(i).unwrap_or(usize::MAX)) {
            Some(p) => {
                p.set_diffusion(d);
                true
            }
            None => false,
        }
    }

    /// Restore the part array's lighting.
    pub fn restore_lighting_internal(&mut self) {
        for p in &mut self.parts {
            p.restore_lighting();
        }
    }

    /// Set the part array's lighting -- the whole body of the object-level setter once the part
    /// array exists: `SetLighting(luminosity, diffuse)` on
    /// every non-null part, nothing when there is no setup.
    pub fn set_lighting_internal(&mut self, luminosity: f32, diffuse: f32) {
        if self.setup.is_none() {
            return;
        }
        for p in &mut self.parts {
            p.set_lighting(luminosity, diffuse);
        }
    }

    /// Set one part's lighting -- the object-level setter's body: `false` with no setup or an
    /// index past `num_parts`; otherwise it sets the lighting on that part and returns `true`.
    pub fn set_part_lighting_internal(&mut self, i: u32, luminosity: f32, diffuse: f32) -> bool {
        if self.setup.is_none() {
            return false;
        }
        match self.parts.get_mut(usize::try_from(i).unwrap_or(usize::MAX)) {
            Some(p) => {
                p.set_lighting(luminosity, diffuse);
                true
            }
            None => false,
        }
    }

    /// Apply lighting from the physics object down. [`LightingMode`] selects restoration, low
    /// lighting, or high lighting; the latter two use the measured constant pairs above.
    /// The operation reaches every part through the physics object.
    /// This is the shared world and paper-doll behavior.
    pub fn apply_lighting(&mut self, mode: LightingMode) {
        match mode {
            LightingMode::Restore => self.restore_lighting_internal(),
            LightingMode::Low => {
                let (l, d) = SELECTION_LOW_LIGHTING;
                self.set_lighting_internal(l, d);
            }
            LightingMode::High => {
                let (l, d) = SELECTION_HIGH_LIGHTING;
                self.set_lighting_internal(l, d);
            }
        }
    }

    /// Each part's `(luminosity, diffuse)` as the renderer will bind it — the gfxobj's own
    /// defaults where the part has no clone. A probe for tests and logs.
    #[must_use]
    pub fn part_lighting(&self) -> Vec<(f32, f32)> {
        self.parts
            .iter()
            .map(|p| {
                p.material
                    .map_or((DEFAULT_LUMINOSITY, DEFAULT_DIFFUSE), |m| {
                        (m.luminosity, m.diffuse)
                    })
            })
            .collect()
    }

    /// Without gfxobjs to scan, the setup's own flag
    /// is the answer; the client or physics can refine it when the meshes are loaded.
    pub fn cache_has_physics_bsp(&mut self) {
        self.has_physics_bsp = self.setup.as_ref().is_some_and(|s| s.has_physics_bsp);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::data::{AnimFrame, MapAssets, NoAssets};
    use dereth_primitives::Quat;

    fn setup() -> Arc<SetupData> {
        let mut placement = BTreeMap::new();
        placement.insert(
            0x65,
            AnimFrame {
                frames: vec![Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY)],
                hooks: Vec::new(),
            },
        );
        Arc::new(SetupData {
            parts: vec![DataId(0x0100_0001), DataId(0x0100_0002)],
            placement_frames: placement,
            height: 2.0,
            radius: 0.5,
            step_up_height: 0.4,
            step_down_height: 0.6,
            allow_free_heading: true,
            spheres: vec![Sphere {
                center: Vec3::ZERO,
                radius: 0.7,
            }],
            selection_sphere: Sphere {
                center: Vec3::new(1.0, 2.0, 3.0),
                radius: 0.5,
            },
            ..SetupData::default()
        })
    }

    /// ORACLE: the recovered part-array behavior, the geometry
    /// accessor table — every one of them scales by `scale.z` alone.
    #[test]
    fn the_geometry_queries_scale_by_z_only() {
        let mut seq = Sequence::new();
        let mut pa = PartArray::create_setup(setup(), true, &mut seq, &NoAssets).expect("setup");
        assert_eq!(pa.radius(), 0.5);
        assert_eq!(pa.height(), 2.0);
        pa.set_scale_internal(Vec3::new(9.0, 9.0, 2.0));
        assert_eq!(pa.radius(), 1.0, "scale.z, not scale.x");
        assert_eq!(pa.height(), 4.0);
        assert_eq!(pa.step_up_height(), 0.8);
        assert_eq!(pa.step_down_height(), 1.2);
        assert!(pa.allows_free_heading());
        assert_eq!(pa.spheres().len(), 1);
        assert_eq!(
            pa.spheres()[0].radius,
            0.7,
            "collision spheres are unscaled"
        );
        // The selection sphere is the exception: the centre scales component-wise.
        let sel = pa.selection_sphere();
        assert_eq!(sel.center, Vec3::new(9.0, 18.0, 6.0));
        assert_eq!(sel.radius, 1.0);
    }

    /// No setup: the step heights are 0.01, not 0.
    #[test]
    fn the_step_heights_default_to_one_centimetre_without_a_setup() {
        let pa = PartArray::new();
        assert_eq!(pa.step_up_height(), 0.01);
        assert_eq!(pa.step_down_height(), 0.01);
        assert_eq!(pa.radius(), 0.0);
    }

    /// Parts are placed from the animation frame alone — contract 6.5. Two parts with different
    /// animation offsets end up at the world frame composed with their own offsets, and the
    /// setup's `parent_index` changes nothing.
    #[test]
    fn parts_are_placed_from_the_animation_frame_with_no_parent_composition() {
        let mut assets = MapAssets::default();
        let id = DataId(0x0300_0001);
        assets.animations.insert(
            id.0,
            Arc::new(crate::data::AnimationData {
                num_frames: 1,
                num_parts: 2,
                pos_frames: None,
                part_frames: vec![AnimFrame {
                    frames: vec![
                        Frame::new(Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY),
                        Frame::new(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY),
                    ],
                    hooks: Vec::new(),
                }],
                has_hooks: false,
            }),
        );
        let mut seq = Sequence::new();
        // A setup that *claims* part 1 is parented to part 0. It must make no difference.
        let mut s = (*setup()).clone();
        s.parent_index = Some(vec![0, 0]);
        let mut pa = PartArray::create_setup(Arc::new(s), true, &mut seq, &assets).expect("setup");
        seq.append_animation(
            crate::data::AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 0,
                framerate: 30.0,
            },
            &assets,
        );
        let world = Frame::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY);
        pa.update_parts(&world, &seq);
        assert_eq!(pa.parts[0].pos.origin, Vec3::new(11.0, 20.0, 30.0));
        assert_eq!(
            pa.parts[1].pos.origin,
            Vec3::new(10.0, 22.0, 30.0),
            "part 1 is NOT composed onto part 0"
        );
    }

    /// The placement frame is the fallback when no animation is playing, and id 101 is the default
    /// `SetPlacementFrame` asks for.
    #[test]
    fn the_placement_frame_is_used_when_there_is_no_animation() {
        let mut seq = Sequence::new();
        let mut pa = PartArray::create_setup(setup(), true, &mut seq, &NoAssets).expect("setup");
        let world = Frame::new(Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY);
        pa.update_parts(&world, &seq);
        assert_eq!(pa.parts[0].pos.origin, Vec3::new(5.0, 0.0, 1.0));
        // An unknown id falls back to key 0; with no key 0 either, the frame is cleared.
        assert!(!pa.set_placement_frame(0x999, &mut seq));
        assert!(seq.get_curr_animframe().is_none());
    }

    /// Copy-on-write: the material clone appears on the first override and is released again when
    /// the values return to the gfxobj's defaults.
    #[test]
    fn the_material_is_cloned_on_write_and_released_on_restore() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        assert!(p.material.is_none(), "shares the gfxobj's material");
        p.set_luminosity(0.5);
        assert_eq!(p.material.expect("cloned").luminosity, 0.5);
        p.set_luminosity(DEFAULT_LUMINOSITY);
        assert!(p.material.is_none(), "back to sharing");
        p.set_diffusion(0.25);
        assert!(p.material.is_some());
        p.restore_lighting();
        assert!(p.material.is_none());
    }

    /// `SetTranslucency(1.0)` means invisible: it sets NoDraw and stores nothing.
    #[test]
    fn full_translucency_sets_nodraw_rather_than_a_value() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        p.set_translucency(1.0, false);
        assert!(p.no_draw());
        assert!(p.material.is_none());
        p.set_translucency(0.5, false);
        assert!(!p.no_draw());
        assert_eq!(p.material.expect("cloned").translucency, 0.5);
        // A locked object ignores the hook entirely.
        p.set_translucency(0.9, true);
        assert_eq!(p.material.expect("cloned").translucency, 0.5);
    }

    /// The surface array is copied only when a texture or palette override actually exists.
    #[test]
    fn surfaces_are_copied_on_write_too() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        assert!(p.surface_overrides.is_none());
        p.set_texture_map(DataId(0x0500_0001), DataId(0x0500_0002));
        assert_eq!(
            p.surface_overrides
                .as_ref()
                .expect("copied")
                .texture_maps
                .len(),
            1
        );
        // Re-mapping the same original replaces rather than appends.
        p.set_texture_map(DataId(0x0500_0001), DataId(0x0500_0003));
        let s = p.surface_overrides.as_ref().expect("copied");
        assert_eq!(
            s.texture_maps,
            vec![(DataId(0x0500_0001), DataId(0x0500_0003))]
        );
        // A palette restore leaves the texture map in place, so the copy survives.
        p.restore_palette();
        assert!(p.surface_overrides.is_some());
    }

    /// ORACLE: a part swap first restores the part's surfaces and last re-applies the shift
    /// palette, if the part has one.
    /// A part swap therefore **drops that part's texture-map substitutions** and **keeps** its
    /// shift palette.
    #[test]
    fn a_part_swap_drops_the_texture_maps_and_keeps_the_shift_palette() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        p.set_texture_map(DataId(0x0500_0001), DataId(0x0500_0002));
        p.use_palette(
            DataId(0x0400_0001),
            &[PaletteRange {
                palette_set: DataId(0x0F00_0001),
                offset: 1,
                length: 2,
            }],
        );
        assert!(p.set_part(DataId(0x0100_0009), &NoAssets));
        assert_eq!(p.gfxobj_id, DataId(0x0100_0009));
        let s = p
            .surface_overrides
            .as_ref()
            .expect("the shift palette survives the swap");
        assert!(
            s.texture_maps.is_empty(),
            "restoring the surfaces did not drop the substitution"
        );
        assert_eq!(s.shift_palette, Some(DataId(0x0400_0001)));
        assert_eq!(
            s.subpalettes.len(),
            1,
            "and its ranges came with it, exactly once"
        );
    }

    /// A part with nothing overridden gains nothing from a swap: restoring the surfaces
    /// leaves `surfaces` pointing at the gfxobj's own array and there is no shift palette to
    /// re-apply.
    #[test]
    fn a_part_swap_on_an_unmodified_part_leaves_it_unmodified() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        assert!(p.set_part(DataId(0x0100_0009), &NoAssets));
        assert_eq!(p.gfxobj_id, DataId(0x0100_0009));
        assert!(p.surface_overrides.is_none());
    }

    /// `SetPart`'s own guard refuses `INVALID_DID`. `INVALID_DID` is a zero-initialised global that
    /// retail never writes, so it is `0`; a swap naming it
    /// fails and the part is left exactly as it was.
    #[test]
    fn a_part_swap_to_the_invalid_data_id_fails_and_changes_nothing() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        p.set_texture_map(DataId(0x0500_0001), DataId(0x0500_0002));
        assert!(!p.set_part(DataId(0), &NoAssets));
        assert_eq!(p.gfxobj_id, DataId(0x0100_0001));
        assert_eq!(
            p.surface_overrides
                .as_ref()
                .expect("kept")
                .texture_maps
                .len(),
            1,
            "a refused swap must not restore the surfaces either"
        );
    }

    /// A second palette change releases the old shift palette and takes a reference to
    /// the new one, so a second call **replaces**; it cannot leave the first call's ranges behind.
    #[test]
    fn a_second_shift_palette_replaces_the_first_rather_than_adding_to_it() {
        let mut p = PhysicsPart::new(DataId(0x0100_0001));
        let a = PaletteRange {
            palette_set: DataId(0x0F00_0001),
            offset: 0,
            length: 4,
        };
        let b = PaletteRange {
            palette_set: DataId(0x0F00_0002),
            offset: 8,
            length: 4,
        };
        p.use_palette(DataId(0x0400_0001), &[a]);
        p.use_palette(DataId(0x0400_0002), &[b]);
        let s = p.surface_overrides.as_ref().expect("copied");
        assert_eq!(s.shift_palette, Some(DataId(0x0400_0002)));
        assert_eq!(
            s.subpalettes,
            vec![b],
            "the first call's range is still there"
        );
    }

    /// Apply lighting is the three way switch onto every part.
    #[test]
    fn apply_lighting_is_the_three_way_switch_onto_every_part() {
        let mut seq = Sequence::new();
        let mut pa = PartArray::create_setup(setup(), true, &mut seq, &NoAssets).expect("setup");
        assert!(
            pa.parts.iter().all(|p| p.material.is_none()),
            "shared materials to begin with"
        );

        pa.apply_lighting(LightingMode::High);
        assert_eq!(pa.part_lighting(), vec![(0.99, 1.0); 2]);
        assert!(
            pa.parts.iter().all(|p| p.material.is_some()),
            "SetLighting cloned the material"
        );

        pa.apply_lighting(LightingMode::Low);
        assert_eq!(pa.part_lighting(), vec![(0.0, 0.35); 2]);

        pa.apply_lighting(LightingMode::Restore);
        assert_eq!(
            pa.part_lighting(),
            vec![(DEFAULT_LUMINOSITY, DEFAULT_DIFFUSE); 2]
        );
        assert!(
            pa.parts.iter().all(|p| p.material.is_none()),
            " with a default translucency drops the clone "
        );
    }

    /// The lighting setter answers `false` past `num_parts` and touches only the
    /// one part it was asked about — lights
    /// the doll region by region through it.
    #[test]
    fn set_part_lighting_touches_one_part_and_refuses_a_bad_index() {
        let mut seq = Sequence::new();
        let mut pa = PartArray::create_setup(setup(), true, &mut seq, &NoAssets).expect("setup");
        assert!(pa.set_part_lighting_internal(1, 0.99, 1.0));
        assert_eq!(
            pa.part_lighting(),
            vec![(DEFAULT_LUMINOSITY, DEFAULT_DIFFUSE), (0.99, 1.0)]
        );
        assert!(
            !pa.set_part_lighting_internal(2, 0.99, 1.0),
            "index == num_parts is refused"
        );
        assert_eq!(pa.part_lighting().len(), 2);
    }
}
