//! A whole body: its part groups put together, the behaviors and modes it plays, its sliders,
//! and each frame's pose, bone transforms and skinning palettes.
//!
//! A body's visual description names part groups (the body, its head, a tail...), each a setup with
//! a skeleton and an animation map; a group other than the root hangs from a connection point of
//! its parent group's skeleton. Every group plays the same behaviors at the same times, each
//! through its own animation map. The behavior table says, for the body's element and mode, which
//! animation key and layer each behavior plays; the mode tracker ([`crate::modes`]) keeps what
//! plays and changes modes; the slider tracker ([`crate::sliders`]) keeps the sliders the graph
//! reads.
//!
//! Each frame the sliders move, the trackers advance, and every group blends what plays through
//! its compositor ([`crate::compositor`]): every bone's sum is divided by its weight and its
//! rotation brought back to unit length (a bone nothing moved keeps the setup's default pose). The
//! posed bones give each group's bone transforms, and with the inverse rest pose, the skinning
//! palette. What the behaviors ask of the object (callbacks, effects, the camera) is queued as
//! [`Event`]s, and their effects, hooks and impulses gather in [`Body::triggers`].

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::animation::{AnimMap, BehaviorKey, BehaviorTable, CharacterTemplate, ModeKey};
use auberean_assets::fx::FxScript;
use auberean_assets::setup::{BoneTransform, Setup};
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::chargen::Random;
use auberean_world::math::Mat4;

use crate::clip::Pose;
use crate::compositor::{Compositor, Entry, Player};
use crate::graph::{
    animate, hold_loop, period, triggers, Animations, Eval, GraphState, MIN_WEIGHT,
};
use crate::host::{Completion, Event, Host};
use crate::modes::Modes;
use crate::queries::{TableQueries, TemplateQueries};
use crate::scripts::ScriptTracker;
use crate::skeleton::Skeleton;
use crate::sliders::Sliders;
use crate::track::Params;
use crate::tracker::Behaviors;
use crate::triggers::{script_triggers, FrameTriggers, Gathered};
use crate::turns::Ground;
use auberean_world::look::{Group, Look, LookRecords, PartLook};

/// The behavior a body plays while it moves (its locomotion blend).
pub const MOVE: u32 = crate::tracker::MOVE;
/// The elements: on the ground, in the air.
pub const EARTH: u32 = 0x4000_0001;
pub const AIR: u32 = 0x4000_0002;
/// The behaviors a body plays standing idle.
pub const IDLE: [u32; 2] = [crate::tracker::IDLE, crate::tracker::IDLE_FIDGET];

/// The locomotion sliders: sideways and forward speed, vertical speed, turning.
pub mod slider {
    pub const FORWARD: u32 = 0x4000_0001;
    pub const SIDEWAYS: u32 = 0x4000_0002;
    pub const VERTICAL: u32 = 0x4000_0003;
    pub const TURN: u32 = 0x4000_0004;
    /// How hard the wind blows.
    pub const WIND: u32 = 0x4000_000B;
}

/// The pose kind a bone nothing animates rests in.
pub const DEFAULT_POSE: u32 = 0;

/// The time a body's first animation step covers when it is put together.
pub const FIRST_STEP: f32 = 0.1;

/// The records a body is put together from.
pub trait BodyRecords: Animations + LookRecords {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>>;
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>>;
    // D2-06373
    fn behavior_table(&self, id: DataId) -> Option<Arc<BehaviorTable>>;
    /// The character template, which says what kind of behavior each behavior is.
    fn character_template(&self) -> Option<Arc<CharacterTemplate>> {
        None
    }
    /// An effect script.
    fn fx_script(&self, _id: DataId) -> Option<Arc<FxScript>> {
        None
    }
    /// The client's random numbers, for the bodies made from these records to draw from; none
    /// gives each body a generator of its own.
    fn dice(&self) -> Option<&Random> {
        None
    }
    /// How hard the world's wind blows at the moment, which each body's wind slider follows.
    fn wind(&self) -> f32 {
        0.0
    }
}

/// One part group of a body.
#[derive(Clone, Debug)]
pub struct BodyPart {
    pub key: u32,
    pub setup: Arc<Setup>,
    /// The records its setup and animation map were read from.
    pub setup_id: DataId,
    pub anim_map_id: DataId,
    pub skeleton: Option<Skeleton>,
    /// Animation key to animation.
    pub anim_map: HashMap<u32, DataId>,
    /// The group it hangs from, the bone of that group, and the connection point's transform.
    pub parent: Option<(usize, usize, Mat4)>,
    /// Each bone's pose when nothing animates it.
    pub default_pose: Vec<BoneTransform>,
    /// What the appearance does to the group's parts.
    pub look: PartLook,
    pub compositor: Compositor,
}

impl BodyPart {
    /// The animation the part's map gives an animation key; none when the map lacks the key.
    // D2-14901
    #[must_use]
    pub fn animator(&self, key: u32) -> DataId {
        self.anim_map.get(&key).copied().unwrap_or(DataId::NONE)
    }
}

/// One part group's pose this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PartPose {
    /// The group's frame in the body's frame.
    pub world: Mat4,
    /// Each bone's transform in the group's frame, without the bones' own scales.
    pub bones: Vec<Mat4>,
    /// Each bone's skinning transform: from the rest pose to the posed bone, in the group's frame.
    pub palette: Vec<Mat4>,
}

/// A body.
#[derive(Clone, Debug)]
pub struct Body {
    pub parts: Vec<BodyPart>,
    pub table: Option<Arc<BehaviorTable>>,
    /// The record the behavior table was read from.
    table_id: DataId,
    pub template: Option<Arc<CharacterTemplate>>,
    /// The element the body is in.
    pub element: u32,
    /// The mode the body is in; set it to change mode at once (taken at the next step).
    pub mode: u32,
    /// The kind of ground under the body.
    pub terrain: u32,
    /// Where the body stands (the slope lean reads it); its owner sets it each frame.
    pub ground: Ground,
    // D2-05907, D2-06396
    pub modes: Modes,
    // D2-06397
    pub sliders: Sliders,
    /// The effect scripts it plays on their own.
    pub scripts: ScriptTracker,
    /// The effects, hooks and impulses the animations set off, for the owner to play.
    pub triggers: FrameTriggers,
    /// The client's random numbers (its records' when they have them).
    pub dice: Random,
    /// What the behaviors asked of the object, for the owner.
    pub events: Vec<Event>,
    /// The player's own body (the camera follows its behaviors).
    pub is_player: bool,
    applied_mode: u32,
    poses: Vec<PartPose>,
}

fn fill(t: &BoneTransform, with_scale: bool) -> Mat4 {
    let s = if with_scale {
        t.scale
    } else {
        Vec3::new(1.0, 1.0, 1.0)
    };
    Mat4::from_parts(t.offset, t.rotation, s)
}

/// The part group `g` describes, hung from nothing and with nothing on its parts: its setup,
/// skeleton, animation map, default pose and a compositor of the behavior table's layers; none
/// when its setup is not in the data.
fn make_part(
    records: &dyn BodyRecords,
    g: &Group,
    layers: usize,
    tweaks: &[i32],
) -> Option<BodyPart> {
    let setup = records.setup(g.setup)?;
    let anim_map = records
        .anim_map(g.anim_map)
        .map(|m| m.entries.iter().copied().collect())
        .unwrap_or_default();
    let default_pose = crate::setups::placement_frame(&setup, DEFAULT_POSE)
        .or(setup.poses.first())
        .map(|p| p.bones.clone())
        .unwrap_or_default();
    let mut compositor = Compositor::default();
    compositor.init(layers, tweaks, setup.bones.len());
    Some(BodyPart {
        key: g.key,
        skeleton: Skeleton::from_setup(&setup),
        setup,
        setup_id: g.setup,
        anim_map_id: g.anim_map,
        anim_map,
        parent: None,
        default_pose,
        look: PartLook::default(),
        compositor,
    })
}

/// Where group `g` hangs among `parts`: the part of its parent's key, the bone of that part's
/// connection point `g` names, and the point's transform; none for the root, or when the parent or
/// its point is missing.
fn hang(parts: &[BodyPart], g: &Group) -> Option<(usize, usize, Mat4)> {
    let pi = g
        .parent
        .and_then(|pk| parts.iter().position(|p| p.key == pk))?;
    let cp = crate::setups::connection_point(&parts[pi].setup, g.connection)?;
    Some((
        pi,
        usize::try_from(cp.bone).ok()?,
        fill(&cp.transform, true),
    ))
}

/// What `look` does to the parts of its group `key`.
fn part_look(records: &dyn BodyRecords, look: &Look, key: u32) -> PartLook {
    look.atoms(records, key)
        .map(PartLook::from_atoms)
        .unwrap_or_default()
}

/// The object as the trackers see it: the body's parts and records, and where what they ask for
/// goes.
struct Ctx<'a> {
    parts: &'a [BodyPart],
    records: &'a dyn BodyRecords,
    sliders: &'a Sliders,
    terrain: u32,
    template: Option<&'a CharacterTemplate>,
    table: Option<&'a BehaviorTable>,
    triggers: &'a mut FrameTriggers,
    dice: &'a mut Random,
    events: &'a mut Vec<Event>,
    is_player: bool,
}

impl Ctx<'_> {
    /// The animation a combination plays in each part that has one, in part order.
    fn combo_animations(&self, key: &BehaviorKey) -> Vec<DataId> {
        let Some(table) = self.table else {
            return Vec::new();
        };
        let combo = table.combo(key);
        self.parts
            .iter()
            .map(|p| p.animator(combo.anim_key))
            .filter(|&id| !id.is_none() && self.records.animation(id).is_some())
            .collect()
    }

    fn state(&self, key: &BehaviorKey) -> GraphState<'_> {
        GraphState {
            sliders: self.sliders,
            mode: key.mode,
            element: key.element,
            terrain: self.terrain,
        }
    }
}

impl Host for Ctx<'_> {
    /// The first part whose animation of the combination has a period gives the cycles a second.
    // D2-06569, D2-06672
    fn combo_cycle_velocity(&mut self, key: &BehaviorKey) -> f32 {
        // Every behavior asks this every frame, and the first part with a period answers it.
        let Some(table) = self.table else {
            return 0.0;
        };
        let anim_key = table.combo(key).anim_key;
        let state = self.state(key);
        for part in self.parts.iter() {
            let id = part.animator(anim_key);
            if id.is_none() {
                continue;
            }
            let p = period(self.records, id, &state);
            if p != 0.0 {
                return 1.0 / p;
            }
        }
        0.0
    }

    // D2-06570, D2-06673
    fn hold_loop(&mut self, key: &BehaviorKey) -> Option<(f32, f32)> {
        let found = self
            .combo_animations(key)
            .into_iter()
            .find_map(|id| hold_loop(self.records, id, &self.state(key)))?;
        (found.0 != 0.0 || found.1 != 0.0).then_some(found)
    }

    fn behavior_flags(&self, behavior: u32) -> Option<u64> {
        self.template.and_then(|t| t.behavior(behavior))
    }

    fn is_the_player(&self) -> bool {
        self.is_player
    }

    fn triggers(&mut self) -> Option<&mut FrameTriggers> {
        Some(self.triggers)
    }

    fn script_triggers(&mut self, script: u32, time: f32, g: &mut Gathered) {
        if let Some(s) = self.records.fx_script(DataId(script)) {
            script_triggers(&s, time, g);
        }
    }

    fn dice(&mut self) -> &mut Random {
        self.dice
    }

    fn dice_and_triggers(&mut self) -> (&mut Random, Option<&mut FrameTriggers>) {
        (self.dice, Some(self.triggers))
    }

    fn event(&mut self, e: Event) {
        self.events.push(e);
    }
}

/// Plays a compositor's entries for one part.
struct PartPlayer<'a> {
    records: &'a dyn BodyRecords,
    sliders: &'a mut Sliders,
    dice: &'a mut Random,
    terrain: u32,
    /// Per tracker: its element, its mode, and whether it plays the moving behavior.
    trackers: [(u32, u32, bool); 2],
    parents: &'a [Option<usize>],
    bone_types: &'a [u32],
    ground: Ground,
}

impl Player for PartPlayer<'_> {
    fn play(&mut self, e: &Entry, weight: f32, out: &mut Pose) {
        let (element, mode, _) = self.trackers[e.tracker.min(1)];
        let mut eval = Eval {
            sliders: self.sliders,
            dice: self.dice,
            mode,
            element,
            terrain: self.terrain,
            bone_types: self.bone_types,
            ground: self.ground,
        };
        animate(
            self.records,
            e.animation,
            &mut eval,
            weight,
            e.time,
            self.parents,
            out,
        );
    }

    fn playing_move(&self, tracker: usize) -> bool {
        self.trackers[tracker.min(1)].2
    }
}

impl Body {
    /// The body a stored visual description describes; `None` when the description cannot be put
    /// together or no part group has a setup.
    // D2-06552
    #[must_use]
    pub fn assemble(records: &dyn BodyRecords, visual: DataId) -> Option<Self> {
        let look = Look::of_record(records, visual).ok()?;
        Self::from_look(records, &look)
    }

    /// The body a total description describes: its groups from the root, each before the groups
    /// hung from it, each with its setup, animation map, compositor and what the appearance does to
    /// its parts (a group whose setup is not in the data is left out); its behavior table started
    /// in its initial mode, its sliders at their starting values, and a first animation step.
    // D2-06598, D2-06597, D2-06596, D2-06655, D2-06595, D2-12904, D2-06602, D2-06563
    #[must_use]
    pub fn from_look(records: &dyn BodyRecords, look: &Look) -> Option<Self> {
        let table = records.behavior_table(look.behavior_table);
        let template = records.character_template();
        let (layers, tweaks) = table.as_ref().map_or((1, Vec::new()), |t| {
            (t.layers.len().max(1), t.layer_tweaks.clone())
        });
        let mut parts: Vec<BodyPart> = Vec::new();
        for g in look.tree() {
            let Some(mut p) = make_part(records, g, layers, &tweaks) else {
                continue;
            };
            p.parent = hang(&parts, g);
            p.look = part_look(records, look, g.key);
            parts.push(p);
        }
        if parts.is_empty() {
            return None;
        }
        let (element, mode) = template
            .as_ref()
            .map_or((EARTH, 0), |t| (t.default_element(), t.default_mode()));
        let mut body = Self {
            parts,
            table: table.clone(),
            table_id: look.behavior_table,
            template,
            element,
            mode,
            terrain: 0,
            ground: Ground::default(),
            modes: Modes::new(element, mode),
            sliders: Sliders::new(),
            scripts: ScriptTracker::default(),
            triggers: FrameTriggers::default(),
            dice: records.dice().cloned().unwrap_or_default(),
            events: Vec::new(),
            is_player: false,
            applied_mode: mode,
            poses: Vec::new(),
        };
        let id = look.behavior_table.0;
        body.with_host(records, |modes, host| {
            modes.init(id, table, host);
        });
        body.mirror();
        body.advance(records, FIRST_STEP);
        Some(body)
    }

    /// Brings the body up to date with a changed total description, keeping what it plays. Each
    /// group of the description (the root first, each before the groups hung from it) keeps its
    /// part when the body has one of its key hanging from the same group, with the same setup and
    /// animation map, and that group's part was kept too; else its part is made anew (a part made
    /// anew loses those hung from it, which are made anew in turn). Each hangs from its connection
    /// point again and takes what the description does to its parts; parts of groups the
    /// description no longer has go. A different behavior table starts the trackers over in its
    /// initial mode. False, the body left as it was, when no group's setup is in the data.
    // D2-06599, D2-06600, D2-06594, D2-06590, D2-06591, D2-06533
    pub fn compare_and_modify(&mut self, records: &dyn BodyRecords, look: &Look) -> bool {
        let table = if look.behavior_table == self.table_id {
            self.table.clone()
        } else {
            records.behavior_table(look.behavior_table)
        };
        let (layers, tweaks) = table.as_ref().map_or((1, Vec::new()), |t| {
            (t.layers.len().max(1), t.layer_tweaks.clone())
        });
        let parent_key = |parts: &[BodyPart], p: &BodyPart| {
            p.parent.and_then(|(pi, _, _)| parts.get(pi)).map(|q| q.key)
        };
        let old_parents: Vec<Option<u32>> = self
            .parts
            .iter()
            .map(|p| parent_key(&self.parts, p))
            .collect();
        let mut old: Vec<Option<BodyPart>> = std::mem::take(&mut self.parts)
            .into_iter()
            .map(Some)
            .collect();
        let mut parts: Vec<BodyPart> = Vec::new();
        let mut kept: Vec<u32> = Vec::new();
        let mut all_kept = true;
        for g in look.tree() {
            let parent_kept = g.parent.is_none_or(|pk| kept.contains(&pk));
            let found = old.iter().position(|p| {
                p.as_ref().is_some_and(|p| {
                    p.key == g.key && p.setup_id == g.setup && p.anim_map_id == g.anim_map
                })
            });
            let reuse = found
                .filter(|i| parent_kept && old_parents[*i] == g.parent)
                .and_then(|i| old[i].take());
            let mut p = match reuse {
                Some(p) => {
                    kept.push(g.key);
                    p
                }
                None => {
                    all_kept = false;
                    match make_part(records, g, layers, &tweaks) {
                        Some(p) => p,
                        None => continue,
                    }
                }
            };
            p.parent = hang(&parts, g);
            p.look = part_look(records, look, g.key);
            parts.push(p);
        }
        if parts.is_empty() {
            self.parts = old.into_iter().flatten().collect();
            return false;
        }
        if !all_kept || old.iter().any(Option::is_some) {
            self.poses.clear();
        }
        // Every part's compositor is set up again for the behavior table's layers.
        for p in &mut parts {
            p.compositor.init(layers, &tweaks, p.setup.bones.len());
        }
        self.parts = parts;
        if look.behavior_table != self.table_id {
            self.table_id = look.behavior_table;
            self.table = table.clone();
            let id = look.behavior_table.0;
            self.with_host(records, |modes, host| {
                modes.init(id, table, host);
            });
            self.mirror();
        }
        true
    }

    /// Run `f` with the mode tracker and the object it animates (the trackers are given their
    /// object for each call rather than holding it).
    // D2-15154
    fn with_host<R>(
        &mut self,
        records: &dyn BodyRecords,
        f: impl FnOnce(&mut Modes, &mut dyn Host) -> R,
    ) -> R {
        let table = self.table.clone();
        let template = self.template.clone();
        let mut ctx = Ctx {
            parts: &self.parts,
            records,
            sliders: &self.sliders,
            terrain: self.terrain,
            template: template.as_deref(),
            table: table.as_deref(),
            triggers: &mut self.triggers,
            dice: &mut self.dice,
            events: &mut self.events,
            is_player: self.is_player,
        };
        let r = f(&mut self.modes, &mut ctx);
        for d in self.sliders.done.drain(..) {
            self.events.push(Event::SliderDone { context: d.context });
        }
        r
    }

    /// Keep the public element and mode in step with the tracker.
    fn mirror(&mut self) {
        let s = self.modes.mode_state();
        self.element = s.element;
        self.mode = s.mode;
        self.applied_mode = s.mode;
    }

    /// The animation key and layer the table gives behavior `b` in the body's element and mode.
    // D2-14991
    #[must_use]
    pub fn combo(&self, b: u32) -> Option<(u32, u32)> {
        let t = self.table.as_ref()?;
        let c = t.combo(&BehaviorKey {
            element: self.element,
            mode: self.mode,
            behavior: b,
        });
        c.is_valid().then_some((c.anim_key, c.layer))
    }

    /// Play a behavior.
    // D2-06381
    pub fn do_behavior(&mut self, records: &dyn BodyRecords, params: &Params) -> bool {
        self.with_host(records, |m, h| m.do_behavior(params, h))
    }

    /// Play a behavior nobody waits on.
    // D2-06382
    pub fn do_autonomous_behavior(&mut self, records: &dyn BodyRecords, behavior: u32) -> bool {
        self.with_host(records, |m, h| m.do_autonomous_behavior(behavior, h))
    }

    /// Stop a behavior.
    // D2-06384
    pub fn stop_behavior(&mut self, records: &dyn BodyRecords, behavior: u32) -> u32 {
        self.with_host(records, |m, h| m.stop_behavior(behavior, h))
    }

    /// Let go of a hold, playing `next` after it when it names a behavior.
    // D2-06383
    pub fn release_behavior(
        &mut self,
        records: &dyn BodyRecords,
        behavior: u32,
        next: &Params,
    ) -> bool {
        self.with_host(records, |m, h| m.release_behavior(behavior, next, h))
    }

    /// Stop what holds a layer.
    // D2-06385
    pub fn stop_layer(&mut self, records: &dyn BodyRecords, layer: u32) -> u32 {
        self.with_host(records, |m, h| m.stop_layer(layer, h))
    }

    /// Stop every behavior a move cancels.
    // D2-06387
    pub fn stop_move_to_cancels(&mut self, records: &dyn BodyRecords) {
        self.with_host(records, |m, h| m.stop_move_to_cancels(h));
    }

    /// Stop every behavior the server asked for.
    // D2-06386
    pub fn stop_all_server_behaviors(&mut self, records: &dyn BodyRecords) -> bool {
        self.with_host(records, |m, h| m.stop_all_server_behaviors(h))
    }

    /// Whether a behavior plays.
    // D2-06392
    #[must_use]
    pub fn is_doing_behavior(&self, behavior: u32) -> bool {
        self.modes.is_doing_behavior(behavior)
    }

    /// Go to a mode by the table's mode graph; `context` hears when the body is there.
    // D2-06379
    pub fn do_mode(&mut self, records: &dyn BodyRecords, context: u32, mode: u32) -> bool {
        self.with_host(records, |m, h| m.do_mode(context, mode, h))
    }

    /// Go to an element.
    pub fn do_element(&mut self, records: &dyn BodyRecords, element: u32) -> bool {
        self.with_host(records, |m, h| m.do_element(element, h))
    }

    /// Change mode at once.
    pub fn set_mode(&mut self, records: &dyn BodyRecords, mode: u32) -> bool {
        let r = self.with_host(records, |m, h| m.set_mode(mode, h));
        self.mirror();
        r
    }

    /// Every part group's bones rest in its setup's pose `kind`, or its pose 0 when it has none
    /// of that kind (a setup with neither keeps what it had); whether every group had one.
    pub fn set_placement_frame(&mut self, kind: u32) -> bool {
        let mut all = true;
        for part in &mut self.parts {
            let pose = crate::setups::placement_frame(&part.setup, kind)
                .or_else(|| crate::setups::placement_frame(&part.setup, DEFAULT_POSE));
            match pose {
                Some(p) => part.default_pose = p.bones.clone(),
                None => all = false,
            }
        }
        all
    }

    /// The element and mode the body was asked to be in.
    #[must_use]
    pub fn desired_mode_state(&self) -> ModeKey {
        self.modes.desired_mode_state()
    }

    /// The body is going away: its behaviors are discarded, so the effects they started are
    /// stopped in [`Body::triggers`] for the owner to play; it animates no more.
    pub fn discard(&mut self, records: &dyn BodyRecords) {
        self.with_host(records, |m, h| m.discard(h));
        self.modes.can_animate = false;
    }

    /// Play an effect script on its own; `false` when it is not in the data.
    // D2-06378
    pub fn do_fx_script(&mut self, records: &dyn BodyRecords, script: u32) -> bool {
        self.scripts.do_script(records, script)
    }

    /// What the behaviors asked of the object since last taken.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The waits answered since last taken, in the order they were answered (see
    /// [`crate::host`]); the other events stay queued.
    pub fn take_completions(&mut self) -> Vec<Completion> {
        let mut done = Vec::new();
        self.events.retain(|e| match e.completion() {
            Some(c) => {
                done.push(c);
                false
            }
            None => true,
        });
        done
    }

    /// Steer the locomotion sliders toward the body's motion: `velocity` in the body's own frame
    /// (x to its right, y ahead, z up, meters per second), `turn` its rate of turn, and `scale` its
    /// size along its axes (its look's times its own). The sideways and forward sliders take the
    /// speed over the size along their axis, so a smaller body's legs cycle faster at one speed.
    /// Each slider moves at once (in 0.05 s) from just past standing still toward the same side,
    /// slowly (0.3 s) across a change of more than 0.75 m/s, else in 0.1 s. Sideways, the first of
    /// those that holds wins; forward, a change of more than 0.75 m/s wins over just past still, so
    /// stopping and running again eases in.
    // D2-15327
    pub fn set_motion(&mut self, velocity: Vec3, turn: f32, scale: Vec3) {
        let tiny = |c: f32| if c.abs() < 0.001 { 0.0 } else { c };
        let (mut x, y, z) = (tiny(velocity.x), tiny(velocity.y), tiny(velocity.z));
        x *= (1.0 - (turn * 0.017_455_1).abs()).max(0.0);
        // `now` is where the slider stands, as a speed (times the size).
        let just_past_still = |now: f32, target: f32| {
            (now > 0.0 && now < 0.05 && target > 0.0) || (now < 0.0 && now > -0.05 && target < 0.0)
        };
        let far = |now: f32, target: f32| (now - target).abs() > 0.75;
        let s = &mut self.sliders;
        let now = s.current(slider::SIDEWAYS) * scale.x;
        let time = if just_past_still(now, x) {
            0.05
        } else if far(now, x) {
            0.3
        } else {
            0.1
        };
        s.do_slider(0, slider::SIDEWAYS, x / scale.x, time);
        let now = s.current(slider::FORWARD) * scale.y;
        let time = if far(now, y) {
            0.3
        } else if just_past_still(now, y) {
            0.05
        } else {
            0.1
        };
        s.do_slider(0, slider::FORWARD, y / scale.y, time);
        let v = z.max(-10.0);
        let cur = s.current(slider::VERTICAL);
        s.do_slider(
            0,
            slider::VERTICAL,
            v,
            if cur < 0.0 && v > cur { 0.4 } else { 0.01 },
        );
        let speed = (x * x + y * y).sqrt();
        let t = if speed > 0.5 {
            turn / (2.0 * speed)
        } else {
            turn
        };
        let cur = s.current(slider::TURN);
        s.do_slider(
            0,
            slider::TURN,
            t,
            if (cur - t).abs() > 16.0 { 0.1 } else { 0.01 },
        );
    }

    /// Advance `dt` seconds: the sliders move, the effect scripts and the trackers advance, every
    /// part blends what plays into its pose, and the playing behaviors' triggers are gathered.
    pub fn advance(&mut self, records: &dyn BodyRecords, dt: f32) {
        self.step(records, dt, true);
    }

    /// Advance `dt` seconds for a body nobody sees: as [`Body::advance`], but the parts' poses are
    /// not blended (the last ones are kept).
    pub fn advance_unseen(&mut self, records: &dyn BodyRecords, dt: f32) {
        self.step(records, dt, false);
    }

    fn step(&mut self, records: &dyn BodyRecords, dt: f32, posed: bool) {
        if self.mode != self.applied_mode {
            let m = self.mode;
            self.set_mode(records, m);
        }
        // D2-06487
        let wind = records.wind();
        #[allow(clippy::float_cmp)] // the client compares them exactly
        if self.sliders.current(slider::WIND) != wind {
            self.sliders.do_slider(0, slider::WIND, wind, 1.0);
        }
        self.sliders.update(dt);
        self.scripts
            .update(dt, records, &mut self.triggers, &mut self.dice);
        let ok = self.with_host(records, |m, h| m.update(dt, h));
        if ok {
            self.do_animation(records, posed);
            self.with_host(records, |m, h| m.gather_triggers(h));
        } else {
            self.poses.clear();
        }
        self.mirror();
    }

    /// Blend what the trackers play into every part's pose, gathering each playing animation's
    /// triggers into its behavior as it goes. The mode being left weighs `1 - t` and the one
    /// entered `t` while a change of mode is under way. A body nobody sees (`posed` false) gathers
    /// triggers and fills its compositors but blends no pose.
    // D2-06568, D2-06677, D2-06674, D2-06667, D2-06668
    fn do_animation(&mut self, records: &dyn BodyRecords, posed: bool) {
        let t = self.modes.transition_time;
        let terrain = self.terrain;
        let ground = self.ground;
        let mut poses: Vec<PartPose> = Vec::with_capacity(self.parts.len());
        for k in 0..self.parts.len() {
            let part = &mut self.parts[k];
            part.compositor.clear();
            let modes = &mut self.modes;
            let mut add = |tracker: &mut Behaviors, idx: usize, w: f32, sliders: &Sliders| {
                let state = GraphState {
                    sliders,
                    mode: tracker.mode,
                    element: tracker.element,
                    terrain,
                };
                let keys: Vec<u32> = tracker.tracks.keys().collect();
                for b in keys {
                    let track = tracker.tracks.get_mut(b).expect("listed");
                    let id = part.animator(track.anim_key);
                    if id.is_none() {
                        continue;
                    }
                    if records.animation(id).is_none() {
                        continue;
                    }
                    triggers(records, id, &state, track.time, true, &mut track.gathered);
                    if track.weight > 0.0 && w != 0.0 {
                        part.compositor.add(
                            track.layer,
                            Entry {
                                animation: id,
                                weight: w * track.weight,
                                tracker: idx,
                                time: track.time,
                            },
                        );
                    }
                }
            };
            let mut trackers = [(0, 0, false); 2];
            match (modes.from.as_mut(), modes.to.as_mut()) {
                (Some(from), None) => {
                    add(from, 0, 1.0, &self.sliders);
                    trackers[0] = (from.element, from.mode, from.playing_move);
                }
                (Some(from), Some(to)) => {
                    add(from, 0, 1.0 - t, &self.sliders);
                    add(to, 1, t, &self.sliders);
                    trackers[0] = (from.element, from.mode, from.playing_move);
                    trackers[1] = (to.element, to.mode, to.playing_move);
                }
                _ => {}
            }
            if !posed {
                continue;
            }
            let n = part.setup.bones.len();
            let parents: Vec<Option<usize>> = part
                .skeleton
                .as_ref()
                .map_or_else(|| vec![None; n], |s| s.parents.clone());
            // D2-12875
            let bone_types: Vec<u32> = part.setup.bones.iter().map(|b| b.id).collect();
            let mut acc = Pose::new(n);
            let sliders_now = self.sliders.clone();
            let mut player = PartPlayer {
                records,
                sliders: &mut self.sliders,
                dice: &mut self.dice,
                terrain,
                trackers,
                parents: &parents,
                bone_types: &bone_types,
                ground,
            };
            let slider = |s: u32| sliders_now.current(s);
            part.compositor.animate(&mut acc, &slider, &mut player);
            poses.push(finish_pose(part, &acc, &parents, &poses));
        }
        if posed {
            self.poses = poses;
        }
    }

    /// One bone's transform in the body's current pose, with no copy of a pose already worked out.
    pub(crate) fn pose_of_bone(
        &self,
        records: &dyn BodyRecords,
        group: usize,
        bone: usize,
    ) -> Option<Mat4> {
        if self.poses.len() == self.parts.len() {
            return self.poses.get(group)?.bones.get(bone).copied();
        }
        self.pose(records).get(group)?.bones.get(bone).copied()
    }

    /// Each part group's pose at the behaviors' current times: the one the last step blended, or
    /// for a body that has not stepped, every bone at its default pose.
    #[must_use]
    pub fn pose(&self, _records: &dyn BodyRecords) -> Vec<PartPose> {
        if self.poses.len() == self.parts.len() {
            return self.poses.clone();
        }
        let mut out: Vec<PartPose> = Vec::with_capacity(self.parts.len());
        for part in &self.parts {
            let n = part.setup.bones.len();
            let parents: Vec<Option<usize>> = part
                .skeleton
                .as_ref()
                .map_or_else(|| vec![None; n], |s| s.parents.clone());
            let p = finish_pose(part, &Pose::new(n), &parents, &out);
            out.push(p);
        }
        out
    }
}

/// A part's bone transforms, skinning palette and frame from its blended pose. A bone's transform
/// is its own without its scale, followed by its parent's; what skins is its own with its scale,
/// followed by its parent's with the parent's scale, so a parent's scale reaches the bones under
/// it in the skin and not in the bone transforms. (The client works the scaled chain out only for
/// a setup whose meshes are weighted to bones, the only ones it skins.)
// D2-06604, D2-06577
fn finish_pose(
    part: &BodyPart,
    acc: &Pose,
    parents: &[Option<usize>],
    done: &[PartPose],
) -> PartPose {
    let n = part.setup.bones.len();
    let local: Vec<BoneTransform> = (0..n)
        .map(|b| resolve(acc, b, &part.default_pose))
        .collect();
    let mut bones: Vec<Mat4> = Vec::with_capacity(n);
    let mut deform: Vec<Mat4> = Vec::with_capacity(n);
    for b in 0..n {
        let (no_scale, with_scale) = (fill(&local[b], false), fill(&local[b], true));
        match parents[b] {
            Some(p) => {
                bones.push(no_scale.then(&bones[p]));
                deform.push(with_scale.then(&deform[p]));
            }
            None => {
                bones.push(no_scale);
                deform.push(with_scale);
            }
        }
    }
    let palette = part
        .skeleton
        .as_ref()
        .map_or_else(|| deform.clone(), |s| s.palette(&deform));
    let world = match part.parent {
        Some((pi, bone, cp)) => match done.get(pi) {
            Some(parent) => {
                let pb = parent.bones.get(bone).copied().unwrap_or(Mat4::IDENTITY);
                cp.then(&pb).then(&parent.world)
            }
            None => Mat4::IDENTITY,
        },
        None => Mat4::IDENTITY,
    };
    PartPose {
        world,
        bones,
        palette,
    }
}

/// A bone's transform from its blended sum: the rotation divided by the weight and brought back
/// to unit length when it is not too short, the offset and scale divided by the weight; the
/// default pose when the bone weighs under 0.0002.
// D2-06677
#[allow(clippy::neg_cmp_op_on_partial_ord)] // a weight that is not a number rests too
fn resolve(acc: &Pose, b: usize, default: &[BoneTransform]) -> BoneTransform {
    let w = acc.weights[b];
    if !(w >= MIN_WEIGHT) {
        return default.get(b).copied().unwrap_or(BoneTransform {
            offset: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: Quat::IDENTITY,
        });
    }
    let s = acc.bones[b];
    let mut q = s.rotation;
    if w != 0.0 {
        let k = 1.0 / w;
        q = Quat {
            w: q.w * k,
            x: q.x * k,
            y: q.y * k,
            z: q.z * k,
        };
    }
    let n2 = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
    if n2 > 3.999_999_6e-8 {
        let k = 1.0 / n2.sqrt();
        q = Quat {
            w: q.w * k,
            x: q.x * k,
            y: q.y * k,
            z: q.z * k,
        };
    }
    let k = 1.0 / w;
    BoneTransform {
        offset: Vec3::new(s.offset.x * k, s.offset.y * k, s.offset.z * k),
        scale: Vec3::new(s.scale.x * k, s.scale.y * k, s.scale.z * k),
        rotation: q,
    }
}
