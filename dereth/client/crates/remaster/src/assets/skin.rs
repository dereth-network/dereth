//! Bodies drawn in place of an object's parts: an art pack's skinned mesh, posed only by its own
//! clips, each played for a motion slot ([`Slot`]) the object's motion fills.
//!
//! **The body stands where the object stands.** Each frame its bones take the pose its clip has
//! for that moment, in the body's bind space, and the bind space stands at the object's own place
//! ([`crate::scene::Motion::frame`]). Nothing of the parts' animation moves a bone: the parts only
//! say which motion plays (a walk, a swing, a death), and the body plays its own clip for it.
//! Nothing about the object but its pixels changes: its animation, its collision, its selection
//! and what the server sees are its parts'.
//!
//! **Drawn by one part.** The body is drawn through one part's instance, its *anchor* (the first
//! of the parts it names): each frame its vertices are written in that part's own space, so every
//! pass that draws the part (the G-buffer, the shadows, the lights) draws the body, lit and
//! shadowed as an object. The parts it names draw nothing of their own; any other part draws as
//! it is. A vertex on a part rather than a bone stays where the bind pose puts it.
//!
//! **Slots and fallbacks.** A body brings a clip for any of a fixed list of slots. An empty slot
//! plays another's ([`Slot::fallbacks`]): a side step the walk, a run the walk, a stance's idle the
//! peace idle, a sleep the dead pose. Each slot plays one way ([`Play`]): an idle loops at its own
//! pace, a walk goes round once for each stride its motion asks, an attack plays once, timed so
//! its blow lands when the parts' does, and a death holds its end.
//!
//! **Names.** A body's mesh is one per object, named past every retail id ([`skin_mesh`]): the
//! body's place in the pack and a number of the object's own. Its surfaces are named by the body's
//! place and the primitive's ([`skin_surface`]).

use glam::{Mat3, Mat4, Quat, Vec3};

use super::Mesh;

/// The first id of the bodies' meshes: `PACK_SKIN_MESH | skin << 16 | object`.
pub const PACK_SKIN_MESH: u32 = 0xF400_0000;
/// The first id of the bodies' surfaces: `PACK_SKIN_SURFACE | skin << 8 | primitive`.
pub const PACK_SKIN_SURFACE: u32 = 0xF500_0000;

/// The id of the body `skin`'s mesh drawn for the object numbered `object`.
#[must_use]
pub fn skin_mesh(skin: u32, object: u16) -> u32 {
    PACK_SKIN_MESH | (skin & 0xFF) << 16 | u32::from(object)
}

/// The body and object a body's mesh id names; `None` for any other id.
#[must_use]
pub fn of_skin_mesh(id: u32) -> Option<(u32, u16)> {
    (id & 0xFF00_0000 == PACK_SKIN_MESH).then_some(((id >> 16) & 0xFF, (id & 0xFFFF) as u16))
}

/// The id of the body `skin`'s primitive `primitive`'s surface.
#[must_use]
pub fn skin_surface(skin: u32, primitive: u8) -> u32 {
    PACK_SKIN_SURFACE | (skin & 0xFFFF) << 8 | u32::from(primitive)
}

/// The body and primitive a body's surface id names; `None` for any other id.
#[must_use]
pub fn of_skin_surface(id: u32) -> Option<(u32, u8)> {
    (id & 0xFF00_0000 == PACK_SKIN_SURFACE).then_some(((id >> 8) & 0xFFFF, (id & 0xFF) as u8))
}

/// The first number a vertex names a bone of the body's own skeleton by: `BONE` + its place in
/// [`Rig::bones`].
pub const BONE: u16 = 0x4000;

/// A motion a body may bring a clip for. The object's motion names one each frame
/// ([`crate::scene::Motion::slot`]); the body plays its clip for it, or for the first of its
/// [`Slot::fallbacks`] it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    /// Standing at peace, and in any still state with no slot of its own.
    Idle,
    /// Standing in a melee, missile or magic stance.
    IdleMelee,
    IdleMissile,
    IdleMagic,
    Walk,
    /// A walk backwards: with no clip of its own, the walk played backwards.
    WalkBack,
    Run,
    /// A side step to the right; one to the left plays it backwards.
    SideStep,
    /// A turn to the right; one to the left plays it backwards.
    Turn,
    /// Swings at a target's head, middle and legs.
    MeleeHigh,
    MeleeMed,
    MeleeLow,
    Special,
    /// Aiming, reloading and shooting.
    Missile,
    /// Casting, and the gestures of spells.
    Cast,
    /// The warning before a cast.
    Telegraph,
    /// Struck: a flinch; and struck hard, a stagger or a fall.
    Hit,
    HitHeavy,
    /// Dying: played once, its end held.
    Die,
    /// Lying dead: a corpse.
    Dead,
    Sleep,
    /// A change of stance.
    Stance,
    /// A fidget or a gesture, and a cheer.
    Emote,
    Cheer,
    /// Falling through the air, and leaving the ground.
    Fall,
    Jump,
}

/// How a slot's clip plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    /// Round and round at its own pace.
    Loop,
    /// Once round for each stride ([`Clip::stride`]) the speed its object's motion asks carries the
    /// body, so its feet hold, sped up at most [`MOST_PACE`] times; at its own pace without a
    /// stride.
    Paced,
    /// Once from when it starts, then held at its end while the motion lasts; a clip with a blow
    /// ([`Clip::hit`]) is timed so the blow lands when the parts' does.
    Once,
    /// Once, then held at its end for as long as the object stays in it.
    Hold,
}

impl Slot {
    /// Every slot, in the order the pack format lists them.
    pub const ALL: [Self; 26] = [
        Self::Idle,
        Self::IdleMelee,
        Self::IdleMissile,
        Self::IdleMagic,
        Self::Walk,
        Self::WalkBack,
        Self::Run,
        Self::SideStep,
        Self::Turn,
        Self::MeleeHigh,
        Self::MeleeMed,
        Self::MeleeLow,
        Self::Special,
        Self::Missile,
        Self::Cast,
        Self::Telegraph,
        Self::Hit,
        Self::HitHeavy,
        Self::Die,
        Self::Dead,
        Self::Sleep,
        Self::Stance,
        Self::Emote,
        Self::Cheer,
        Self::Fall,
        Self::Jump,
    ];

    /// Its name in a pack's `clips` table.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::IdleMelee => "idle_melee",
            Self::IdleMissile => "idle_missile",
            Self::IdleMagic => "idle_magic",
            Self::Walk => "walk",
            Self::WalkBack => "walk_back",
            Self::Run => "run",
            Self::SideStep => "side_step",
            Self::Turn => "turn",
            Self::MeleeHigh => "melee_high",
            Self::MeleeMed => "melee_med",
            Self::MeleeLow => "melee_low",
            Self::Special => "special",
            Self::Missile => "missile",
            Self::Cast => "cast",
            Self::Telegraph => "telegraph",
            Self::Hit => "hit",
            Self::HitHeavy => "hit_heavy",
            Self::Die => "die",
            Self::Dead => "dead",
            Self::Sleep => "sleep",
            Self::Stance => "stance",
            Self::Emote => "emote",
            Self::Cheer => "cheer",
            Self::Fall => "fall",
            Self::Jump => "jump",
        }
    }

    /// The slot a pack's `clips` table names `name`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// How its clip plays.
    #[must_use]
    pub const fn play(self) -> Play {
        match self {
            Self::Idle
            | Self::IdleMelee
            | Self::IdleMissile
            | Self::IdleMagic
            | Self::Turn
            | Self::Dead
            | Self::Sleep
            | Self::Fall => Play::Loop,
            Self::Walk | Self::WalkBack | Self::Run | Self::SideStep => Play::Paced,
            Self::Die => Play::Hold,
            _ => Play::Once,
        }
    }

    /// Whether it is a stance's standing still.
    #[must_use]
    pub const fn is_idle(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::IdleMelee | Self::IdleMissile | Self::IdleMagic
        )
    }

    /// The slots whose clips play in its place on a body with none for it, best first; `Idle`
    /// stands for the object's stance's idle, and then the peace idle. An empty hit names none:
    /// the clip already playing goes on. An empty sleep holds the dead pose, a waking from it plays
    /// the death backwards, and an empty stance change fades from one stance's idle to the
    /// other's ([`SkinModel::joints_live`]).
    #[must_use]
    pub const fn fallbacks(self) -> &'static [Self] {
        match self {
            Self::IdleMelee | Self::IdleMissile | Self::IdleMagic => &[Self::Idle],
            Self::Walk => &[Self::Run],
            Self::WalkBack => &[Self::Walk, Self::Run],
            Self::Run => &[Self::Walk],
            Self::SideStep => &[Self::Walk, Self::Run],
            Self::Turn | Self::Emote | Self::Fall | Self::Jump => &[Self::Idle],
            Self::MeleeHigh => &[Self::MeleeMed, Self::MeleeLow],
            Self::MeleeMed => &[Self::MeleeHigh, Self::MeleeLow],
            Self::MeleeLow => &[Self::MeleeMed, Self::MeleeHigh],
            Self::Special | Self::Missile | Self::Cast => {
                &[Self::MeleeMed, Self::MeleeHigh, Self::MeleeLow]
            }
            Self::Telegraph => &[Self::Cast, Self::MeleeMed, Self::MeleeHigh, Self::MeleeLow],
            Self::HitHeavy => &[Self::Hit],
            Self::Die => &[Self::Dead],
            Self::Cheer => &[Self::Emote, Self::Idle],
            Self::Idle | Self::Hit | Self::Dead | Self::Sleep | Self::Stance => &[],
        }
    }
}

/// A bone of a body's own skeleton: the matrix that takes the bind space to its own, and its turn
/// and place in the bind pose, where it stands while no clip poses it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    pub inverse_bind: Mat4,
    rest: (Quat, Vec3),
}

impl Bone {
    /// The bone whose inverse bind matrix is `inverse_bind`.
    #[must_use]
    pub fn new(inverse_bind: Mat4) -> Self {
        let bind = inverse_bind.inverse();
        Self {
            inverse_bind,
            rest: (turn_of(bind), bind.w_axis.truncate()),
        }
    }
}

/// What posing one object's body carries from frame to frame: what it last played of its own
/// clips.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoseState {
    pub playing: Option<Playing>,
}

/// The clip a body plays, and how: its slot (none for the bind pose), whether backwards, and how
/// its place in it goes on.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pick {
    slot: Option<Slot>,
    reversed: bool,
    how: How,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum How {
    /// As its slot plays it ([`Play`]).
    Plays(Play),
    /// Once over this many seconds (a waking over the parts' link).
    Over(f32),
    /// Held at its end (a corpse's death, standing in for its dead pose).
    AtEnd,
}

/// What a body with clips of its own last played: the slot the motion asked, the clip it picked
/// and when it started, the bones it stood in (in the bind space), and a fade under way from an
/// earlier pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Playing {
    asked: Option<Slot>,
    pick: Pick,
    /// Whether it is waking: the parts on their way out of a sleep.
    waking: bool,
    started: f64,
    /// When a clip's blow is to land, and how fast it plays until then.
    hook: Option<(f64, f32)>,
    /// The motions queued and the seconds to the cycle, when last posed: an action that starts
    /// again is told by them.
    queued: u8,
    until_cycle: f32,
    bones: Vec<(Quat, Vec3)>,
    fade: Option<Fade>,
    /// Where a clip with a stride stands in its round.
    paced: Option<Paced>,
    /// How far through its clip it was drawn, 0 to 1.
    share: f32,
}

impl Playing {
    /// The slot whose clip it plays, how far through it it was drawn, and whether the speed its
    /// object's motion asks paced it ([`Paced`]): for an instrument to read.
    #[must_use]
    pub fn drawn(&self) -> (Option<Slot>, f32, bool) {
        (self.pick.slot, self.share, self.paced.is_some())
    }
}

/// A fade under way: when it began, the bones it fades from, and how long it takes.
type Fade = (f64, Vec<(Quat, Vec3)>, f32);

/// A pose of a body's bones: each bone's turn and place in the bind space, and their scales where
/// any is scaled.
type Pose = (Vec<(Quat, Vec3)>, Option<Vec<f32>>);

/// A clip with a stride ([`Clip::stride`]) played for a walk or a run, moved on by the speed its
/// object's motion asks ([`crate::scene::Motion::velocity`]): one round for each stride that speed
/// carries the body, backwards while it plays backwards. Whether the object gets anywhere does not
/// matter: a creature pressed against a crowd or a wall runs in place, as its parts do. It goes
/// round at most [`MOST_PACE`] times as fast as its own pace. It starts at the parts' place in
/// their cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Paced {
    /// The slot whose clip it is, and how far through its round it is, 0 to 1.
    slot: Slot,
    share: f32,
    /// The clock when last posed.
    clock: f64,
    /// How fast it goes round, in rounds (strides) a second, and whether backwards.
    rate: f32,
    backwards: bool,
}

/// The most a walk's or a run's clip is sped up past its own pace to hold its feet. A clip whose
/// strides are short for the speed its motion asks (a lumbering gait asked to run) plays at this
/// many times its own pace, and its feet slip the rest: sped up further, it reads as a frantic
/// patter rather than the gait it is.
pub const MOST_PACE: f32 = 2.5;

/// The least rate, in strides a second, at which a walk's or a run's clip is kept on while the
/// parts slow from it to a stand: paced by their stopping stride while it carries the object,
/// then slowing over [`STOP_EASE`].
pub const LEAST_STRIDES: f32 = 0.1;

/// How fast a walk's or a run's clip kept on into a stand slows: its rate falls by a factor of e
/// every this many seconds.
const STOP_EASE: f32 = 0.15;

/// How long a body takes to fade from one clip to the next, in seconds.
pub const CLIP_FADE: f32 = 0.2;

/// How long it takes to fade into a walk or a run, in seconds: the object is already on its way.
pub const CLIP_FADE_INTO_CYCLE: f32 = 0.12;

/// The longest a change of stance with no clip of its own fades between the two idles over.
const STANCE_FADE: f32 = 1.0;

/// One of a body's own clips: each bone's turn and place in the bind space at even steps through
/// it, the first step at its start and the last at its end.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Each step's bones, in [`Rig::bones`]' order.
    pub steps: std::sync::Arc<Vec<Vec<(Quat, Vec3)>>>,
    /// Each step's bones' scale against their own at rest, in the same order, where the clip
    /// scales any (a blink, a stretched limb); empty where it scales none.
    pub scales: std::sync::Arc<Vec<Vec<f32>>>,
    /// How long it lasts, in seconds.
    pub seconds: f32,
    /// How far the body travels in one round of it, in meters of its own space, for a walk or a
    /// run: it goes round once for each stride the object's motion asks, so its feet hold.
    pub stride: Option<f32>,
    /// When its blow lands, in seconds from its start, for an attack: it is timed so that moment
    /// comes when the parts' own attack lands.
    pub hit: Option<f32>,
}

impl Clip {
    /// Its bones' scale `share` of the way through it (0 to 1), between the steps either side;
    /// `None` for a clip that scales none.
    #[must_use]
    pub fn scales_at(&self, share: f32) -> Option<Vec<f32>> {
        let last = self.scales.len().checked_sub(1)?;
        #[allow(clippy::cast_precision_loss)] // a clip's steps, far fewer than a float holds
        let f = share.clamp(0.0, 1.0) * last as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to `last`
        let i = (f.floor() as usize).min(last.saturating_sub(1));
        #[allow(clippy::cast_precision_loss)]
        let t = f - i as f32;
        let (a, b) = (&self.scales[i], &self.scales[(i + 1).min(last)]);
        Some(a.iter().zip(b).map(|(a, b)| a + (b - a) * t).collect())
    }

    /// The share of the way through it (0 to 1) of its step nearest `share`.
    #[must_use]
    pub fn nearest_step(&self, share: f32) -> f32 {
        let last = self.steps.len().saturating_sub(1).max(1);
        #[allow(clippy::cast_precision_loss)] // a clip's steps, far fewer than a float holds
        let last = last as f32;
        (share.clamp(0.0, 1.0) * last).round() / last
    }

    /// Its bones `share` of the way through it (0 to 1), between the steps either side; `None`
    /// for a clip of no steps.
    #[must_use]
    pub fn at(&self, share: f32) -> Option<Vec<(Quat, Vec3)>> {
        let last = self.steps.len().checked_sub(1)?;
        #[allow(clippy::cast_precision_loss)] // a clip's steps, far fewer than a float holds
        let f = share.clamp(0.0, 1.0) * last as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to `last`
        let i = (f.floor() as usize).min(last.saturating_sub(1));
        #[allow(clippy::cast_precision_loss)]
        let t = f - i as f32;
        let (a, b) = (&self.steps[i], &self.steps[(i + 1).min(last)]);
        Some(blend_bones(a, b, t))
    }

    /// How far through it (0 to 1) it is `t` seconds after it started, its blow timed by `hook`
    /// (when it lands, and how fast it plays until then): before the blow it plays at that speed
    /// so the blow lands then, after it at its own.
    fn once_at(&self, started: f64, hook: Option<(f64, f32)>, clock: f64) -> f32 {
        let seconds = self.seconds.max(1e-3);
        #[allow(clippy::cast_possible_truncation)] // seconds into a clip
        let at = match (hook, self.hit) {
            (Some((lands, rate)), Some(hit)) if clock < lands => {
                hit - (lands - clock) as f32 * rate
            }
            (Some((lands, _)), Some(hit)) => hit + (clock - lands) as f32,
            _ => (clock - started) as f32,
        };
        (at / seconds).clamp(0.0, 1.0)
    }
}

/// Each bone `t` of the way from `a` to `b`: turned the shorter way, carried in a line.
fn blend_bones(a: &[(Quat, Vec3)], b: &[(Quat, Vec3)], t: f32) -> Vec<(Quat, Vec3)> {
    a.iter()
        .zip(b)
        .map(|(&(qa, pa), &(qb, pb))| {
            let qb = if qa.dot(qb) < 0.0 { -qb } else { qb };
            (qa.slerp(qb, t).normalize(), pa.lerp(pb, t))
        })
        .collect()
}

/// What a frame tells the posing of one object beyond its parts: the frame's clock in seconds, and
/// the object's state.
#[derive(Debug)]
pub struct Live<'a> {
    pub clock: f64,
    pub state: &'a mut PoseState,
    /// What the object's motion plays this frame, and where the object stands.
    pub motion: Option<crate::scene::Motion>,
}

/// What a body brings beyond its vertices' parts and weights.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rig {
    /// Each vertex's share of dual-quaternion blending; empty for none.
    pub dqs: Vec<f32>,
    /// The GfxObj each part must draw for the body to fit it: an object whose parts draw anything
    /// else there keeps its parts.
    pub gfxobjs: Vec<(u16, u32)>,
    /// Whether its surfaces are retail ones, drawn with the object's own look.
    pub retail: bool,
    /// Whether it is bent by the object's own parts: each vertex goes where the parts it names
    /// now carry it from where their bind frames held it, as the object's animation places
    /// them, rather than the body playing clips of its own.
    pub by_parts: bool,
    /// The bones of its own skeleton.
    pub bones: Vec<Bone>,
    /// Its own clips, each for its slot.
    pub clips: Vec<(Slot, Clip)>,
}

impl Rig {
    /// Its clip for `slot`, when it has one.
    #[must_use]
    pub fn clip(&self, slot: Slot) -> Option<&Clip> {
        self.clips.iter().find(|(s, _)| *s == slot).map(|(_, c)| c)
    }
}

/// One body as the remaster draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinModel {
    /// Its place in the pack.
    pub skin: u32,
    /// Its triangles in its bind space, each primitive a subset named by [`skin_surface`], with
    /// its vertices' tangents and colors.
    pub mesh: Mesh,
    /// Each vertex's joints (a part's place in the setup, or [`BONE`] and up), and its weight on
    /// each.
    pub parts: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Each part's inverse bind matrix, by its place in the setup; `None` for a part the body is
    /// not drawn in place of.
    pub inverse_binds: Vec<Option<Mat4>>,
    /// The body's triangles at rest in its anchor's own space, which the tables hold as its mesh
    /// until the first frame poses it, with a sphere wide enough to hold it posed.
    pub rest: Mesh,
    /// Its skeleton, clips and dual-quaternion shares.
    pub rig: Rig,
}

impl SkinModel {
    /// The body `skin` of `mesh`, whose vertices are on the joints `parts` with `weights`, drawn
    /// in place of the parts `inverse_binds` names, each with its inverse bind matrix. `None` when
    /// its influences do not match its vertices, or it names no part.
    #[must_use]
    pub fn new(
        skin: u32,
        mesh: Mesh,
        parts: Vec<[u16; 4]>,
        weights: Vec<[f32; 4]>,
        inverse_binds: &[(u16, Mat4)],
    ) -> Option<Self> {
        if parts.len() != mesh.vertices.len() || weights.len() != mesh.vertices.len() {
            return None;
        }
        let count = inverse_binds
            .iter()
            .map(|(p, _)| usize::from(*p) + 1)
            .max()?;
        let mut mesh = mesh;
        if mesh.tangents.len() != mesh.vertices.len() {
            mesh.tangents = super::tangents(&mesh);
        }
        let mut binds = vec![None; count];
        for (p, m) in inverse_binds {
            binds[usize::from(*p)] = Some(*m);
        }
        let mut model = Self {
            skin,
            mesh,
            parts,
            weights,
            inverse_binds: binds,
            rest: Mesh::default(),
            rig: Rig::default(),
        };
        model.rest = model.rest_mesh();
        Some(model)
    }

    /// The body with `rig`.
    #[must_use]
    pub fn with_rig(mut self, rig: Rig) -> Self {
        self.rig = rig;
        self
    }

    /// Whether an object whose parts draw `levels` (each part's GfxObjs, nearest first) draws what
    /// the body expects: each part the rig names draws the GfxObj it names. A body bent by its
    /// parts also fits a part that draws nothing at all, as a bone that carries skin but has no
    /// model of its own does.
    #[must_use]
    pub fn fits(&self, levels: &[&[u32]]) -> bool {
        self.rig.gfxobjs.iter().all(|(p, g)| {
            levels
                .get(usize::from(*p))
                .is_some_and(|l| l.contains(g) || (self.rig.by_parts && l.is_empty()))
        })
    }

    /// The place in [`Self::joints`] of the joint a vertex names: a part's, a bone's after the
    /// parts'.
    #[must_use]
    pub fn slot(&self, joint: u16) -> usize {
        if joint >= BONE {
            self.inverse_binds.len() + usize::from(joint - BONE)
        } else {
            usize::from(joint)
        }
    }

    /// The part the body is drawn through: the first it is drawn in place of.
    #[must_use]
    pub fn anchor(&self) -> u16 {
        self.inverse_binds
            .iter()
            .position(Option::is_some)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(0)
    }

    /// Whether the body is drawn in place of `part`, so that the part draws nothing of its own.
    #[must_use]
    pub fn follows(&self, part: usize) -> bool {
        self.inverse_binds.get(part).is_some_and(Option::is_some)
    }

    /// Where the body's bind space stands in its anchor's own space with no motion to place it:
    /// as its anchor's inverse bind matrix has it.
    fn bind_in_anchor(&self) -> Mat4 {
        self.inverse_binds
            .get(usize::from(self.anchor()))
            .copied()
            .flatten()
            .unwrap_or(Mat4::IDENTITY)
    }

    /// The body at rest in its anchor's own space.
    fn rest_mesh(&self) -> Mesh {
        let to_anchor = self.bind_in_anchor();
        let normals = Mat3::from_mat4(to_anchor).inverse().transpose();
        let mut rest = self.mesh.clone();
        for v in &mut rest.vertices {
            v.position = to_anchor
                .transform_point3(Vec3::from(v.position))
                .to_array();
            v.normal = (normals * Vec3::from(v.normal))
                .normalize_or_zero()
                .to_array();
        }
        // A sphere that holds the body however its limbs swing: round its middle at rest, a
        // third again as wide and a quarter metre more.
        let (lo, hi) = rest.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), v| (lo.min(v.position.into()), hi.max(v.position.into())),
        );
        let centre = if lo.x <= hi.x {
            (lo + hi) * 0.5
        } else {
            Vec3::ZERO
        };
        let radius = rest
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position).distance(centre))
            .fold(0.0f32, f32::max);
        rest.sphere = Some((centre.to_array(), radius * 1.35 + 0.25));
        rest.sort_center = centre.to_array();
        rest.tangents = Vec::new();
        rest
    }

    /// Each joint's skinning matrix this frame, in the anchor's own space, the body at rest where
    /// its anchor's bind places it. `parts` is every part's mesh transform, in the setup's order;
    /// only the anchor's is read, to draw the body through. `None` when the frame lacks the anchor,
    /// or its transform cannot be inverted.
    #[must_use]
    pub fn joints(&self, parts: &[Mat4]) -> Option<Vec<Mat4>> {
        self.joints_live(parts, None)
    }

    /// [`Self::joints`], with what the frame says of the object ([`Live`]): the body stands where
    /// the object does and plays its clip for the slot its motion names.
    #[must_use]
    pub fn joints_live(&self, parts: &[Mat4], live: Option<Live<'_>>) -> Option<Vec<Mat4>> {
        let anchor = *parts.get(usize::from(self.anchor()))?;
        if anchor.determinant().abs() < 1e-12 {
            return None;
        }
        let from_anchor = anchor.inverse();
        if self.rig.by_parts {
            // Each part's joint takes the bind space to where the part now stands, in the
            // anchor's space; a part the frame lacks stands where the anchor does.
            return Some(
                self.inverse_binds
                    .iter()
                    .enumerate()
                    .map(|(k, ib)| {
                        let at = parts.get(k).copied().unwrap_or(anchor);
                        from_anchor * at * ib.unwrap_or(Mat4::IDENTITY)
                    })
                    .collect(),
            );
        }
        let (frame, pose) = match live {
            Some(live) => match live.motion {
                Some(m) => {
                    let pose = if self.rig.clips.is_empty() {
                        None
                    } else {
                        Some(self.play(&m, live))
                    };
                    (from_anchor * m.frame, pose)
                }
                None => (self.bind_in_anchor(), None),
            },
            None => (self.bind_in_anchor(), None),
        };
        Some(self.compose(frame, pose.as_ref()))
    }

    /// Every joint's skinning matrix with the bind space standing at `frame` (in the anchor's
    /// space): each part's the frame itself, each bone posed as `pose` has it (its turn and place
    /// in the bind space, and its scale), else at rest.
    fn compose(&self, frame: Mat4, pose: Option<&Pose>) -> Vec<Mat4> {
        let mut out = vec![frame; self.inverse_binds.len()];
        out.reserve(self.rig.bones.len());
        let posed = pose.filter(|(b, _)| b.len() == self.rig.bones.len());
        for (i, b) in self.rig.bones.iter().enumerate() {
            let (q, p) = posed.map_or(b.rest, |(bones, _)| bones[i]);
            let s = posed
                .and_then(|(_, sizes)| sizes.as_ref())
                .and_then(|s| s.get(i).copied())
                .unwrap_or(1.0);
            out.push(
                frame
                    * Mat4::from_scale_rotation_translation(Vec3::splat(s), q, p)
                    * b.inverse_bind,
            );
        }
        out
    }

    /// The clip for `slot` on this body: its own, else the first of its fallbacks it has, the
    /// stance's idle `idle` standing in for [`Slot::Idle`] there first.
    #[must_use]
    pub fn resolve(&self, slot: Slot, idle: Slot) -> Option<Slot> {
        if self.rig.clip(slot).is_some() {
            return Some(slot);
        }
        slot.fallbacks().iter().find_map(|&f| {
            let tries: &[Slot] = if f == Slot::Idle {
                &[idle, Slot::Idle]
            } else {
                &[f]
            };
            tries
                .iter()
                .copied()
                .find(|&t| t != slot && self.rig.clip(t).is_some())
        })
    }

    /// How many seconds of its death a body last posed with `state` has still to play: `None`
    /// unless it is playing its death through (not backwards, as a waking does, nor held at its
    /// end in a corpse's place).
    #[must_use]
    pub fn death_left(&self, state: &PoseState) -> Option<f32> {
        let p = state.playing.as_ref()?;
        if p.pick.slot != Some(Slot::Die) || p.pick.reversed || p.pick.how != How::Plays(Play::Hold)
        {
            return None;
        }
        let clip = self.rig.clip(Slot::Die)?;
        Some(clip.seconds * (1.0 - p.share.clamp(0.0, 1.0))).filter(|s| *s > 0.0)
    }

    /// The dead pose: the dead clip, else the death held at its end; `None` with neither.
    fn dead_pose(&self) -> Option<Pick> {
        if self.rig.clip(Slot::Dead).is_some() {
            return Some(Pick {
                slot: Some(Slot::Dead),
                reversed: false,
                how: How::Plays(Play::Loop),
            });
        }
        self.rig.clip(Slot::Die).map(|_| Pick {
            slot: Some(Slot::Die),
            reversed: false,
            how: How::AtEnd,
        })
    }

    /// What plays for `m` after `last`: the clip and how, whether it is waking, and whether it is
    /// what already played, kept on (an empty hit, an action with no slot).
    fn pick(&self, m: &crate::scene::Motion, last: Option<&Playing>) -> (Pick, bool, bool) {
        let plain = |slot: Slot| Pick {
            slot: Some(slot),
            reversed: false,
            how: How::Plays(slot.play()),
        };
        let rest = Pick {
            slot: None,
            reversed: false,
            how: How::Plays(Play::Loop),
        };
        let idle = || self.resolve(m.idle, m.idle).map_or(rest, plain);
        let dying = last.is_some_and(|p| p.pick.slot == Some(Slot::Die) && !p.pick.reversed);
        // Waking: the parts on their way out of a sleep to a stance, the death played backwards
        // over their way.
        let asleep = last.is_some_and(|p| p.asked == Some(Slot::Sleep) || p.waking);
        let to_stand = m.slot.is_some_and(|s| s.is_idle() || s == Slot::Stance);
        let wakes = asleep && to_stand && !m.settled && self.rig.clip(Slot::Sleep).is_none();
        if wakes && self.rig.clip(Slot::Die).is_some() {
            let over = match last.map(|p| (p.waking, p.pick.how)) {
                Some((true, How::Over(s))) => s,
                _ => m.until_cycle,
            };
            let how = if over > 0.05 {
                How::Over(over)
            } else {
                How::Plays(Play::Once)
            };
            let pick = Pick {
                slot: Some(Slot::Die),
                reversed: true,
                how,
            };
            return (pick, true, false);
        }
        let kept = || last.map(|p| (p.pick, p.waking, true));
        let Some(asked) = m.slot else {
            return kept().unwrap_or((idle(), false, false));
        };
        let pick = match asked {
            // A sleep with no clip of its own: dying into it, then lying dead.
            Slot::Sleep if self.rig.clip(Slot::Sleep).is_none() => {
                let falling = !m.settled && self.rig.clip(Slot::Die).is_some() && !asleep;
                if dying || falling {
                    plain(Slot::Die)
                } else {
                    self.dead_pose().unwrap_or_else(idle)
                }
            }
            // Dead: a death already under way plays on to its end and holds it.
            Slot::Dead if dying => plain(Slot::Die),
            Slot::Dead => self.dead_pose().unwrap_or_else(idle),
            Slot::Die => match self.resolve(Slot::Die, m.idle) {
                Some(s) => plain(s),
                None => idle(),
            },
            // A change of stance with no clip of its own: the new stance's idle, faded into.
            Slot::Stance if self.rig.clip(Slot::Stance).is_none() => idle(),
            _ => match self.resolve(asked, m.idle) {
                Some(s) => Pick {
                    slot: Some(s),
                    // Left is right played backwards; a walk stands in for a walk backwards played
                    // backwards.
                    reversed: if s == asked {
                        m.reversed && matches!(s, Slot::SideStep | Slot::Turn)
                    } else {
                        asked == Slot::WalkBack
                    },
                    how: How::Plays(s.play()),
                },
                // An empty hit, an action the body cannot do: what plays goes on.
                None if asked.play() == Play::Once => {
                    return kept().unwrap_or((idle(), false, false));
                }
                None => idle(),
            },
        };
        (pick, false, false)
    }

    /// Pose every bone by the clip that plays for `m` ([`Self::pick`]): each bone's turn and place
    /// in the bind space, and their scales where the clip scales any. A change of clip, or an
    /// action that starts again, fades from the pose before it.
    fn play(&self, m: &crate::scene::Motion, live: Live<'_>) -> Pose {
        let clock = live.clock;
        let last = live.state.playing.take();
        let (mut pick, waking, kept) = self.pick(m, last.as_ref());
        let last_paced = last
            .as_ref()
            .and_then(|p| p.paced.filter(|q| p.pick.slot == Some(q.slot)));
        // The parts slowing from a walk or a run to a stand: its clip goes on, slowing, until it
        // all but stops.
        let stopping = m.slot.is_some_and(Slot::is_idle) && !m.settled;
        let held = last_paced
            .filter(|_| stopping)
            .and_then(|q| self.pace(q.slot, (q.share, q.backwards), m, clock, Some(q), true))
            .filter(|q| q.rate >= LEAST_STRIDES);
        if let (Some(q), Some(p)) = (held, last.as_ref()) {
            pick = p.pick;
            debug_assert_eq!(pick.slot, Some(q.slot));
        }
        let again = |p: &Playing| {
            !kept
                && held.is_none()
                && p.pick == pick
                && pick.how == How::Plays(Play::Once)
                && (p.asked != m.slot
                    || m.queued < p.queued
                    || m.until_cycle > p.until_cycle + 0.05)
        };
        let fresh = match &last {
            Some(p) => p.pick != pick || again(p),
            None => true,
        };
        let started = match &last {
            Some(p) if !fresh => p.started,
            _ => clock,
        };
        let clip = pick.slot.and_then(|s| self.rig.clip(s));
        // An attack's blow timed to land when the parts' does.
        let hook = match &last {
            Some(p) if !fresh => p.hook,
            _ => clip.and_then(|c| c.hit).and_then(|hit| {
                let until = m.until_hit.filter(|u| *u > 0.01)?;
                Some((clock + f64::from(until), (hit / until).clamp(0.25, 4.0)))
            }),
        };
        // A walk or a run with a stride goes by the speed its motion asks.
        let paced = held.or_else(|| {
            let slot = pick.slot.filter(|_| pick.how == How::Plays(Play::Paced))?;
            let from = last_paced.filter(|q| q.slot == slot && !fresh);
            self.pace(slot, (m.phase, pick.reversed), m, clock, from, false)
        });
        #[allow(clippy::cast_possible_truncation)] // seconds into a clip
        let elapsed = (clock - started) as f32;
        let share = clip.map_or(0.0, |c| {
            let s = match (paced, pick.how) {
                (Some(q), _) => q.share,
                (None, How::Plays(Play::Loop | Play::Paced)) => {
                    let t = (elapsed / c.seconds.max(1e-3)).rem_euclid(1.0);
                    if pick.reversed {
                        1.0 - t
                    } else {
                        t
                    }
                }
                (None, How::Plays(Play::Once | Play::Hold)) => {
                    let t = c.once_at(started, hook, clock);
                    if pick.reversed {
                        1.0 - t
                    } else {
                        t
                    }
                }
                (None, How::Over(seconds)) => {
                    let t = (elapsed / seconds.max(1e-3)).clamp(0.0, 1.0);
                    if pick.reversed {
                        1.0 - t
                    } else {
                        t
                    }
                }
                (None, How::AtEnd) => {
                    if pick.reversed {
                        0.0
                    } else {
                        1.0
                    }
                }
            };
            // Drawn at keyframes, it is drawn at its step nearest there.
            if m.keyframed {
                c.nearest_step(s)
            } else {
                s
            }
        });
        let target = clip
            .and_then(|c| c.at(share))
            .filter(|b| b.len() == self.rig.bones.len())
            .unwrap_or_else(|| self.rig.bones.iter().map(|b| b.rest).collect());
        let sizes = clip
            .and_then(|c| c.scales_at(share))
            .filter(|s| s.len() == self.rig.bones.len());
        let fade = match last {
            Some(p) if fresh => {
                let length = if pick.how == How::Plays(Play::Paced) {
                    CLIP_FADE_INTO_CYCLE
                } else if m.slot == Some(Slot::Stance) && self.rig.clip(Slot::Stance).is_none() {
                    m.until_cycle.clamp(CLIP_FADE, STANCE_FADE)
                } else {
                    CLIP_FADE
                };
                Some((clock, p.bones, length))
            }
            Some(p) => p.fade,
            None => None,
        };
        // How far the fade has come: a clip's scales grow in with it from whole.
        let mut grown = 1.0;
        let (bones, fade) = match fade {
            Some((start, from, length)) => {
                #[allow(clippy::cast_possible_truncation)] // seconds into a fade
                let w = ((clock - start) as f32 / length).clamp(0.0, 1.0);
                if w >= 1.0 || from.len() != target.len() {
                    (target, None)
                } else {
                    let w = w * w * (3.0 - 2.0 * w);
                    grown = w;
                    (blend_bones(&from, &target, w), Some((start, from, length)))
                }
            }
            None => (target, None),
        };
        let sizes = sizes.map(|s| s.iter().map(|s| 1.0 + (s - 1.0) * grown).collect());
        live.state.playing = Some(Playing {
            asked: m.slot,
            pick,
            waking,
            started,
            hook,
            queued: m.queued,
            until_cycle: m.until_cycle,
            bones: bones.clone(),
            fade,
            paced,
            share: if clip.is_some() { share } else { 0.0 },
        });
        (bones, sizes)
    }

    /// Where the clip for `slot`, `s` of the way through it at its start and going `backwards` or
    /// not, stands in its round paced by the speed its object's motion asks ([`Paced`]), carried
    /// on from `last`; `None` for a clip with no stride, or no speed asked. `held`: kept on while
    /// the parts slow to a stand ([`LEAST_STRIDES`]), the way it last went, at the pace of their
    /// stopping stride while it carries the object, then slowing.
    fn pace(
        &self,
        slot: Slot,
        (s, backwards): (f32, bool),
        m: &crate::scene::Motion,
        clock: f64,
        last: Option<Paced>,
        held: bool,
    ) -> Option<Paced> {
        let clip = self.rig.clip(slot)?;
        let stride = clip.stride.filter(|s| *s > 1e-3)?;
        let last = last.filter(|p| p.slot == slot);
        #[allow(clippy::cast_possible_truncation)] // a frame's step, in seconds
        let dt = last.map_or(0.0, |p| (clock - p.clock).clamp(0.0, 0.25) as f32);
        let most = MOST_PACE / clip.seconds.max(1e-3);
        let (rate, backwards) = if held {
            let p = last?;
            // The parts' stopping stride still carrying the object: the feet keep up with it.
            let carried = m.velocity.truncate().length();
            let rate = if carried > 1e-3 {
                (carried / stride).min(most)
            } else {
                p.rate * dereth_primitives::num::math::expf(-dt / STOP_EASE)
            };
            (rate, p.backwards)
        } else {
            // In the object's own space before its scale, as the stride is.
            let speed = m.velocity.truncate().length();
            if speed <= 1e-3 {
                return None;
            }
            ((speed / stride).min(most), backwards)
        };
        let share = last.map_or(s, |p| {
            if backwards {
                p.share - rate * dt
            } else {
                p.share + rate * dt
            }
        });
        Some(Paced {
            slot,
            share: share.rem_euclid(1.0),
            clock,
            rate,
            backwards,
        })
    }

    /// The height of the body's highest point while it plays `slot`: the highest it reaches over
    /// every step of its clip for it (or the clip standing in), else at rest. In the space of
    /// `parts`, each part's mesh transform as [`Self::joints`] takes them, with `frame` the
    /// object's own place in that space, its scale applied. `None` when the parts lack the
    /// anchor.
    #[must_use]
    pub fn top_playing(&self, parts: &[Mat4], frame: Mat4, slot: Option<Slot>) -> Option<f32> {
        let anchor = *parts.get(usize::from(self.anchor()))?;
        if anchor.determinant().abs() < 1e-12 {
            return None;
        }
        let at = anchor.inverse() * frame;
        let top = |pose: Option<&Pose>| {
            self.posed(&self.compose(at, pose))
                .iter()
                .map(|(p, _, _)| anchor.transform_point3(Vec3::from(*p)).z)
                .reduce(f32::max)
        };
        let Some(clip) = slot
            .and_then(|s| self.resolve(s, s))
            .and_then(|s| self.rig.clip(s))
        else {
            return top(None);
        };
        let last = clip.steps.len().saturating_sub(1).max(1);
        (0..=last)
            .filter_map(|k| {
                #[allow(clippy::cast_precision_loss)]
                // a clip's steps, far fewer than a float holds
                let share = k as f32 / last as f32;
                let pose = (clip.at(share)?, clip.scales_at(share));
                top(Some(&pose))
            })
            .reduce(f32::max)
    }

    /// Every vertex posed by `joints` ([`Self::joints`]): its position, normal and tangent (with
    /// its handedness), in the anchor's own space.
    #[must_use]
    pub fn posed(&self, joints: &[Mat4]) -> Vec<([f32; 3], [f32; 3], [f32; 4])> {
        // Each joint as a dual quaternion, and its scale, for the vertices that blend so.
        let (duals, sizes): (Vec<(Quat, Quat)>, Vec<f32>) = if self.rig.dqs.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            joints.iter().map(|m| (dual_of(*m), scale_of(*m))).unzip()
        };
        self.mesh
            .vertices
            .iter()
            .enumerate()
            .zip(self.parts.iter().zip(&self.weights))
            .zip(&self.mesh.tangents)
            .map(|(((i, v), (ps, ws)), t)| {
                let mut m = Mat4::ZERO;
                for (p, w) in ps.iter().zip(ws) {
                    if *w > 0.0 {
                        m += *joints.get(self.slot(*p)).unwrap_or(&Mat4::IDENTITY) * *w;
                    }
                }
                let at = Vec3::from(v.position);
                let turn = Mat3::from_mat4(m);
                let normals = turn.inverse().transpose();
                let mut n = (normals * Vec3::from(v.normal)).normalize_or_zero();
                let mut tan = (turn * Vec3::new(t[0], t[1], t[2])).normalize_or_zero();
                let mut p = m.transform_point3(at);
                let share = self.rig.dqs.get(i).copied().unwrap_or(0.0);
                if share > 0.0 && !duals.is_empty() {
                    let (q, d) = blend_duals(&duals, ps, ws, |j| self.slot(j));
                    // A dual quaternion carries no scale: the vertex is scaled by its joints'
                    // blended scale before it is turned and moved, as a matrix scales it.
                    let size = blend_sizes(&sizes, ps, ws, |j| self.slot(j));
                    let dp = q * (at * size) + translation_of(q, d);
                    p = p.lerp(dp, share);
                    n = n.lerp(q * Vec3::from(v.normal), share).normalize_or_zero();
                    tan = tan
                        .lerp(q * Vec3::new(t[0], t[1], t[2]), share)
                        .normalize_or_zero();
                }
                (p.to_array(), n.to_array(), [tan.x, tan.y, tan.z, t[3]])
            })
            .collect()
    }
}

/// The turn of a skinning matrix (its scale, if any, taken out).
fn turn_of(m: Mat4) -> Quat {
    let r = Mat3::from_mat4(m);
    let s = dereth_primitives::num::math::powf(r.determinant().abs(), 1.0 / 3.0).max(1e-12);
    Quat::from_mat3(&(r * (1.0 / s))).normalize()
}

/// A rigid skinning matrix as a unit dual quaternion: its turn, and half its shift times it.
fn dual_of(m: Mat4) -> (Quat, Quat) {
    let q = turn_of(m);
    let t = m.w_axis.truncate();
    let d = Quat::from_xyzw(t.x, t.y, t.z, 0.0) * q;
    (
        q,
        Quat::from_xyzw(d.x * 0.5, d.y * 0.5, d.z * 0.5, d.w * 0.5),
    )
}

/// A vertex's joints' dual quaternions blended by its weights (each turned to the first's side),
/// made unit.
fn blend_duals(
    duals: &[(Quat, Quat)],
    joints: &[u16; 4],
    weights: &[f32; 4],
    slot: impl Fn(u16) -> usize,
) -> (Quat, Quat) {
    let first = duals.get(slot(joints[0])).map_or(Quat::IDENTITY, |d| d.0);
    let mut q = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    let mut d = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    for (j, w) in joints.iter().zip(weights) {
        if *w <= 0.0 {
            continue;
        }
        let Some((qj, dj)) = duals.get(slot(*j)) else {
            continue;
        };
        let w = if qj.dot(first) < 0.0 { -*w } else { *w };
        q += *qj * w;
        d += *dj * w;
    }
    let len = q.length().max(1e-12);
    (q * (1.0 / len), d * (1.0 / len))
}

/// The uniform scale of a skinning matrix: the cube root of its determinant's size.
fn scale_of(m: Mat4) -> f32 {
    dereth_primitives::num::math::powf(Mat3::from_mat4(m).determinant().abs(), 1.0 / 3.0)
}

/// A vertex's joints' scales blended by its weights; 1 for a vertex on none.
fn blend_sizes(
    sizes: &[f32],
    joints: &[u16; 4],
    weights: &[f32; 4],
    slot: impl Fn(u16) -> usize,
) -> f32 {
    let (mut sum, mut total) = (0.0, 0.0);
    for (j, w) in joints.iter().zip(weights) {
        if *w <= 0.0 {
            continue;
        }
        if let Some(s) = sizes.get(slot(*j)) {
            sum += s * w;
            total += w;
        }
    }
    if total > 0.0 {
        sum / total
    } else {
        1.0
    }
}

/// The shift a unit dual quaternion carries: twice its dual part times its turn's conjugate.
fn translation_of(q: Quat, d: Quat) -> Vec3 {
    let t = d * q.conjugate();
    Vec3::new(t.x, t.y, t.z) * 2.0
}

#[cfg(test)]
mod tests;
