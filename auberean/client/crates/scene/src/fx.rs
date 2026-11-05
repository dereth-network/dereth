//! The effects a body plays: its look's startup effects from the first time it stands somewhere
//! after it is put together, and every effect asked of it since (by the server, its scripts, its
//! animations or a story), played or stopped where it stands then, in the order asked; effects
//! whose time has passed stop; the particle systems they keep go with the body.

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_draw::effects::{AppearanceFx, FxSound, FxSpot, FxWho, ObjectEffects, Rig};
use auberean_draw::systems::ParticleSystems;
use auberean_primitives::Vec3;
use auberean_world::look::{Look, ObjectLook};
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;

use crate::others::{block_of_global, place_in};

/// Where on a body its figure holds things and has its bones, as it stands now, and the water's
/// height where the body stands.
struct FigureRig<'a, 'r> {
    figure: Option<&'a Figure>,
    anims: &'a CachedRecords<'r>,
    water: Option<f32>,
}

impl Rig for FigureRig<'_, '_> {
    /// The holding location's own transform (with its scale), not its bone's.
    fn holding_location(&self, kind: u32) -> Option<Mat4> {
        let (_, location, _) = self.figure?.holding_location(self.anims, kind)?;
        let t = location.transform;
        Some(Mat4::from_parts(t.offset, t.rotation, t.scale))
    }
    fn bone(&self, kind: u32) -> Option<Mat4> {
        self.figure?.bone_type_matrix(self.anims, kind)
    }
    fn water_height(&self) -> Option<f32> {
        self.water
    }
    fn holding_bone(&self, kind: u32) -> Option<Mat4> {
        self.figure?.carried_bone(self.anims, kind)
    }
    fn has_figure(&self) -> bool {
        self.figure.is_some()
    }
}

/// One body's effects, and those asked of it not yet played: an effect and its strength, or none
/// to stop it.
pub(crate) struct BodyFx {
    pub fx: ObjectEffects,
    /// The block whose frame its systems stand in, once it has stood somewhere.
    pub home: Option<(i32, i32)>,
    pub asked: Vec<(u32, Option<f32>)>,
    /// The object it is the body of.
    pub id: u64,
    /// The sounds its effects played or stopped, not yet taken.
    pub sounds: Vec<FxSound>,
    /// The sliders its effects moved on what it holds too, not yet passed on.
    pub child_sliders: Vec<auberean_draw::effects::FxSlider>,
}

impl BodyFx {
    /// The effects playing that did not begin with the object.
    pub(crate) fn current_fx(&self) -> Vec<(u32, f32)> {
        self.fx.current_fx()
    }

    /// The effects of a body of look `look`, or none when its look carries no effects; their
    /// random choices roll the client's one generator.
    pub(crate) fn of_look(look: &Look) -> Option<Self> {
        ObjectEffects::of_look(look).map(|fx| Self {
            fx,
            home: None,
            asked: Vec::new(),
            id: 0,
            sounds: Vec::new(),
            child_sliders: Vec::new(),
        })
    }

    /// The effects of object `id`'s body of look `look`.
    pub(crate) fn of(look: &Look, id: u64) -> Option<Self> {
        Self::of_look(look).map(|f| Self { id, ..f })
    }

    /// The body stands at `at` (on the one grid) facing `heading` with `scale` at the client's time
    /// `now`, the water of its cell (when it has some) at height `water`: its startup effects begin
    /// the first time; the effects asked are played (set going on `figure`'s holding locations and
    /// bones, their scripts on `figure`) or stopped; the effects have their time (strengths ramp,
    /// those whose end has come stop); its systems are queued to be drawn lit by `light`. The
    /// appearances its effects laid over the object or took off it change its own descriptions
    /// (`dress`, none for an object whose look is a stored record's); when any did, `figure` is
    /// brought up to date with the total they give, the rest of its startup effects play again
    /// once for each change, and the total is handed back for the rest of the body.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn think(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        at: Vec3,
        heading: f32,
        scale: Vec3,
        light: auberean_world::color::Color,
        now: f64,
        who: FxWho,
        water: Option<f32>,
        mut figure: Option<&mut Figure>,
        dress: Option<&mut ObjectLook>,
    ) -> Option<Look> {
        let home = self.home_at(
            systems,
            records,
            anims,
            (at, heading, scale),
            now,
            water,
            figure.as_deref(),
        );
        let place = place_in(at, heading, home);
        let spot = FxSpot {
            home,
            place,
            scale,
            now,
        };
        self.play_asked(systems, records, anims, &spot, who, water, &mut figure);
        self.fx.use_time(records, now);
        self.fx.let_go_of_spent_lights(now);
        let rig = FigureRig {
            figure: figure.as_deref(),
            anims,
            water,
        };
        self.fx
            .think_on(systems, home, &place, scale, light, now, &rig);
        let asked = self.fx.take_appearance();
        self.sounds.extend(self.fx.take_sounds());
        let sliders = self.fx.take_sliders();
        self.child_sliders
            .extend(sliders.iter().filter(|s| s.children).copied());
        if let Some(f) = figure.as_deref_mut() {
            for s in sliders {
                f.sliders.do_slider(0, s.slider, s.value, s.time);
            }
        }
        let (total, changes) = lay_appearance(anims, dress?, &asked)?;
        tracing::debug!(?asked, changes, "the effects changed a body's look");
        if let Some(f) = figure {
            f.compare_and_modify(anims, &total);
        }
        for _ in 0..changes {
            self.fx
                .object_end(systems, records, home, &place, scale, now);
        }
        self.sounds.extend(self.fx.take_sounds());
        Some(total)
    }

    /// The block whose frame its effects stand in; the first time, the body standing at `stands`
    /// (where, facing, scale), that of its place, and its startup effects begin.
    #[allow(clippy::too_many_arguments)]
    fn home_at(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        stands: (Vec3, f32, Vec3),
        now: f64,
        water: Option<f32>,
        figure: Option<&Figure>,
    ) -> (i32, i32) {
        if let Some(h) = self.home {
            return h;
        }
        let (at, heading, scale) = stands;
        let h = block_of_global(at);
        self.home = Some(h);
        let rig = FigureRig {
            figure,
            anims,
            water,
        };
        self.fx.start_on(
            systems,
            records,
            h,
            &place_in(at, heading, h),
            scale,
            now,
            &rig,
        );
        h
    }

    /// The effects asked are played or stopped, as they are asked: the systems they set going wait
    /// to run until they are next drawn.
    #[allow(clippy::too_many_arguments)]
    fn play_asked(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        spot: &FxSpot,
        who: FxWho,
        water: Option<f32>,
        figure: &mut Option<&mut Figure>,
    ) {
        for (effect, play) in std::mem::take(&mut self.asked) {
            match play {
                Some(intensity) => {
                    let rig = FigureRig {
                        figure: figure.as_deref(),
                        anims,
                        water,
                    };
                    let (_, scripts) = self
                        .fx
                        .do_fx(systems, records, effect, intensity, spot, who, &rig);
                    if let Some(f) = figure.as_deref_mut() {
                        for s in scripts {
                            f.do_fx_script(anims, s);
                        }
                    }
                }
                None => {
                    self.fx.stop_fx(records, effect, false);
                }
            }
        }
    }

    /// A frame the body is not drawn in (portal space's): the effects asked still play, standing
    /// at `at`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn play_undrawn(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        at: Vec3,
        heading: f32,
        scale: Vec3,
        now: f64,
        who: FxWho,
        water: Option<f32>,
        figure: Option<&mut Figure>,
    ) {
        let mut figure = figure;
        let home = self.home_at(
            systems,
            records,
            anims,
            (at, heading, scale),
            now,
            water,
            figure.as_deref(),
        );
        let place = place_in(at, heading, home);
        let spot = FxSpot {
            home,
            place,
            scale,
            now,
        };
        self.play_asked(systems, records, anims, &spot, who, water, &mut figure);
    }

    /// The lights its effects placed on the body, as they stand `now` with the body at `at` (on
    /// the one grid) facing `heading` in cell `cell`, at the body's `opacity`; none before it has
    /// stood anywhere.
    pub(crate) fn lights(
        &self,
        at: Vec3,
        heading: f32,
        cell: u32,
        now: f64,
        opacity: f32,
    ) -> Vec<(auberean_draw::effects::PlacedLight, (i32, i32))> {
        let Some(home) = self.home else {
            return Vec::new();
        };
        let place = place_in(at, heading, home);
        self.fx
            .lights_now(&place, cell, now, opacity)
            .into_iter()
            .map(|l| (l, home))
            .collect()
    }

    /// The effects of object `id` once its look became `look` (the effects it had, `old`): made
    /// again from the look, the effects that were playing (not those it began with) and those
    /// asked of it not yet played asked again, in that order.
    // D2-06548
    pub(crate) fn again(old: Option<Self>, look: &Look, id: u64) -> Option<Self> {
        let mut new = Self::of(look, id)?;
        if let Some(old) = old {
            new.asked.extend(
                old.fx
                    .current_fx()
                    .into_iter()
                    .map(|(effect, strength)| (effect, Some(strength))),
            );
            new.asked.extend(old.asked);
        }
        Some(new)
    }
}

/// The appearances an object's effects laid over it or took off it (`asked`, in order) applied to
/// its own descriptions: a value laid over the whole object, which changes them when it takes; a
/// key taken off, which always does. The total they then give and how many times they changed,
/// when they did and the total has groups to draw.
// D2-05863, D2-06148
fn lay_appearance(
    anims: &CachedRecords<'_>,
    dress: &mut ObjectLook,
    asked: &[AppearanceFx],
) -> Option<(Look, usize)> {
    let mut changes = 0;
    for a in asked {
        match *a {
            AppearanceFx::Apply { table, key } => {
                changes += usize::from(dress.do_appearance_fx(anims, table, key));
            }
            AppearanceFx::Remove { table, key } => {
                dress.remove_appearance_fx(anims, table, key);
                changes += 1;
            }
        }
    }
    if changes == 0 {
        return None;
    }
    let total = dress.total(anims).ok()?;
    total.root?;
    Some((total, changes))
}
