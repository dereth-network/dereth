//! Where a body's head turns: toward an object it looks at, along the direction the server sends
//! for it, or, for the player with nothing to look at, the way the camera looks.
//!
//! A body has a gaze once something first asks it to look ([`Gaze`]). The head is turned by two of
//! its figure's look sliders, the pitch ([`PITCH`], up positive) and the turn ([`TURN`], left
//! positive), which the figure's look node turns the head and neck bones by; the gaze asks them
//! to move each animation step, before the figure's sliders move.
//!
//! - **Looking at an object** ([`Gaze::look_at`]): the gaze blends from where the sliders stood
//!   toward the object over half a second and holds. Each step the turn is the object's angle from
//!   straight ahead (an object behind is taken as beside), the pitch the rise over the level
//!   distance from the body's top to a point on the object (three quarters up a body that moves
//!   on its own, half way up anything else); the turn blends by the weight, the pitch adds the
//!   rise over the distance times the weight (as the client works it). The player stops looking at
//!   an object that has been behind it or more than 10 m off for more than 2 s.
//! - **Released** ([`Gaze::release`]): the blend runs back out over half a second.
//! - **A direction from the server** ([`Gaze::look_dir`]): another body's head turns to the pitch
//!   and turn the server sent, over half a second.
//! - **The player with nothing to look at**: the head follows the camera: the turn is the way the
//!   camera looks seen from the body (in the body's frame; in mouse look the camera's direction
//!   turned into it), the pitch the camera's height along its offset, each divided by the body's
//!   size and moved to over a tenth of a second (a fifth when more than 0.2 off). With the look
//!   around option off the head looks straight ahead; with the look-at-camera option on, a camera
//!   in front of the body turns the head round toward it.

use auberean_physics::geom::{self, Heading};
use dereth_primitives::num::math;
use dereth_primitives::{Position, Vec3};

/// The look sliders: up and down, the roll, left and right.
pub const PITCH: u32 = 0x4000_0007;
pub const ROLL: u32 = 0x4000_0008;
pub const TURN: u32 = 0x4000_0009;

/// How long a look blends in and out, seconds.
pub const FADE: f32 = 0.5;

/// No object.
const NONE: u64 = 0;

/// Just past a quarter turn either side, where the head turning back toward a camera in front
/// blends from one side to the other.
const QUARTER_LOWER: f32 = 1.470_796_3;
const QUARTER_UPPER: f32 = 1.670_796_4;
const QUARTER: f32 = 1.570_796_4;
/// Two over pi.
const TWO_OVER_PI: f32 = 0.636_619_75;

/// A body's gaze.
#[derive(Clone, Debug, PartialEq)]
pub struct Gaze {
    /// Looking (at an object, or along a direction sent).
    pub on: bool,
    /// How long the look blends in and out, seconds.
    pub fade: f32,
    /// Seconds into the look, and how long it lasts.
    pub time: f32,
    pub period: f32,
    /// How far blended in, 0 to 1, and how fast a second.
    pub weight: f32,
    pub rate: f32,
    /// The pitch and the turn toward the object last looked at.
    pub aim_pitch: f32,
    pub aim_turn: f32,
    /// The look sliders as they stood when the look began: pitch, roll, turn.
    pub start: [f32; 3],
    /// How long the object looked at has been behind or far from the body, seconds.
    pub away: f32,
    /// The look holds once blended in.
    pub hold: bool,
    /// The options: the player looks at the camera in front of it, and looks where the camera
    /// looks.
    pub look_at_camera: bool,
    pub look_around: bool,
    /// The object looked at (0 for none), and the body whose gaze this is.
    pub target: u64,
    pub owner: u64,
}

impl Gaze {
    /// The gaze of body `owner`: looking at nothing, both options on.
    // D2-06875, D2-06077
    #[must_use]
    pub fn new(owner: u64) -> Self {
        Self {
            on: false,
            fade: 0.5,
            time: 0.0,
            period: 1.5,
            weight: 0.0,
            rate: 2.0,
            aim_pitch: 0.0,
            aim_turn: 0.0,
            start: [0.0; 3],
            away: 0.0,
            hold: false,
            look_at_camera: true,
            look_around: true,
            target: NONE,
            owner,
        }
    }

    /// The weight's rate for a blend over `fade` seconds: at once with none.
    fn blend_over(&mut self, fade: f32) {
        if fade == 0.0 {
            self.weight = 1.0;
            self.rate = 0.0;
        } else {
            self.weight = 0.0;
            self.rate = 1.0 / fade;
        }
    }

    /// Look at nothing, blending out over `fade` seconds.
    // D2-06872
    pub fn release(&mut self, fade: f32) {
        self.target = NONE;
        self.fade = fade;
        self.on = false;
        self.hold = false;
        self.blend_over(fade);
    }

    /// The server sent a direction: the next step turns the head to it (over half a second when
    /// looking at nothing).
    // D2-06873
    pub fn look_dir(&mut self) {
        self.on = true;
        if self.target == NONE {
            self.time = 0.0;
            self.period = 0.5;
        }
    }

    /// Look at `target` (none: nothing changes), blending in over `fade` seconds from where the
    /// look sliders stand now (`sliders`: pitch, roll, turn) and holding. A body already looking
    /// at something keeps it when asked to look at itself.
    // D2-06874
    pub fn look_at(&mut self, target: u64, fade: f32, sliders: [f32; 3]) {
        if target == NONE {
            return;
        }
        self.on = true;
        if self.target != NONE && self.owner == target {
            return;
        }
        self.start = sliders;
        self.target = target;
        self.fade = fade;
        self.time = 0.0;
        self.period = 1.0;
        self.hold = true;
        self.blend_over(fade);
    }

    /// The turn and pitch toward `target` from `owner` (steered as the look's weight says, from
    /// where the sliders stood when it began), and how long the target has been behind or far.
    // D2-06877
    fn look_logic(&mut self, dt: f32, owner: &Owner, target: &Target, out: &mut Vec<SliderAsk>) {
        if self.target == self.owner {
            return;
        }
        let mut v = geom::offset(&owner.position, &target.position);
        let up = if target.mobile { 0.75 } else { 0.5 };
        v.z = narrow(f64::from(target.height) * up + f64::from(v.z));
        let mut l = geom::global_to_local(&owner.position.frame, v);
        let level = narrow(f64::from(l.y * l.y + l.x * l.x).sqrt());
        if l.y < 0.0 || level > 10.0 {
            self.away += dt;
        } else {
            self.away = 0.0;
        }
        l.z = narrow(f64::from(l.z) - f64::from(owner.height));
        if l.y < 0.0 {
            l.y = 0.0;
        }
        let turn = narrow(math::atan2(f64::from(l.y), f64::from(l.x)) - f64::from(QUARTER));
        let w = self.weight;
        out.push(SliderAsk {
            slider: TURN,
            value: narrow(
                f64::from(1.0 - w) * f64::from(self.start[2]) + f64::from(turn) * f64::from(w),
            ),
            time: 0.0,
        });
        let rise = if level != 0.0 && w != 0.0 {
            f64::from(l.z) / (f64::from(level) * f64::from(w))
        } else {
            0.0
        };
        out.push(SliderAsk {
            slider: PITCH,
            value: narrow(f64::from(1.0 - w) * f64::from(self.start[0]) + rise),
            time: 0.0,
        });
        self.aim_pitch = if level == 0.0 {
            0.0
        } else {
            narrow(f64::from(l.z) / f64::from(level))
        };
        self.aim_turn = turn;
    }

    /// One animation step of `dt` seconds for `owner`: the look's time and weight move on, and
    /// the look sliders are asked to move toward `target` (the object looked at, when it is in
    /// the world and is not the owner), else, for the player, the way `camera` looks, else along
    /// the direction the server sent. A body that is not the player and is not looking is left
    /// alone.
    // D2-06878
    pub fn update(
        &mut self,
        dt: f32,
        owner: &Owner,
        target: Option<&Target>,
        camera: Option<&Camera>,
    ) -> Turned {
        let mut out = Turned::default();
        if !owner.is_player && !self.on {
            return out;
        }
        self.time += dt;
        let last = self.period - self.fade;
        if self.time > last && self.on {
            if self.hold {
                self.time = last;
            } else {
                self.on = false;
                self.target = NONE;
            }
        }
        self.weight = if self.time < self.fade {
            self.time / self.fade
        } else if self.time > self.fade && last > self.time {
            1.0
        } else if last >= self.time {
            dt * self.rate + self.weight
        } else {
            (self.period - self.time) / self.fade
        };
        self.weight = self.weight.clamp(0.0, 1.0);
        if let Some(t) = target.filter(|_| self.target != self.owner) {
            self.look_logic(dt, owner, t, &mut out.asks);
            if owner.is_player && self.away > 2.0 {
                out.stop_looking = true;
            }
            return out;
        }
        let (mut pitch, mut turn) = (owner.dir.0, owner.dir.1);
        let (mut pitch_time, mut turn_time) = (0.5, 0.5);
        if owner.is_player {
            self.away = 0.0;
            let Some(c) = camera else {
                return out;
            };
            (pitch, turn) = self.follow_camera(owner, c);
            pitch_time = 0.1;
            turn_time = 0.1;
            let s = owner.scale;
            let w = self.weight;
            pitch = narrow(
                f64::from(1.0 - w) / f64::from(s.z) * f64::from(pitch)
                    + f64::from(self.aim_pitch) * f64::from(w),
            );
            turn = narrow(
                f64::from(1.0 - w) / f64::from(s.x) * f64::from(turn)
                    + f64::from(self.aim_turn) * f64::from(w),
            );
            if (owner.sliders[0] - pitch).abs() > 0.2 {
                pitch_time = 0.2;
            }
            if (owner.sliders[2] - turn).abs() > 0.2 {
                turn_time = 0.2;
            }
        }
        out.asks.push(SliderAsk {
            slider: PITCH,
            value: pitch,
            time: pitch_time,
        });
        out.asks.push(SliderAsk {
            slider: TURN,
            value: turn,
            time: turn_time,
        });
        out
    }

    /// The pitch and turn the player's head takes from the camera (before the blend): the way the
    /// camera looks, from behind the body, in the body's frame.
    fn follow_camera(&self, owner: &Owner, camera: &Camera) -> (f32, f32) {
        let offset = if camera.mouse_look {
            geom::global_to_local(&owner.position.frame, camera.offset)
        } else {
            camera.offset
        };
        let (a, b, c) = (-offset.x, -offset.y, -offset.z);
        let level = (f64::from(b) * f64::from(b) + f64::from(a) * f64::from(a)).sqrt();
        let heading = if level < f64::from(geom::EPS) {
            Heading::ZERO
        } else {
            let inv = 1.0 / level;
            let angle = math::atan2(inv * f64::from(b), f64::from(a) * inv);
            Heading::from_radians(narrow(FIVE_QUARTERS - angle))
        };
        let mut turn = -heading.radians_centered();
        let mut pitch = c;
        if !self.look_around {
            return (0.0, 0.0);
        }
        if !self.look_at_camera {
            if !(-QUARTER..=QUARTER).contains(&turn) {
                turn = 0.0;
            }
            return (pitch, turn);
        }
        // A camera in front: the head turns back toward it, blending over just past a quarter
        // turn to the left; to the right it turns back once past a little short of one.
        let blend = |t: f32| {
            narrow((f64::from(t) * f64::from(TWO_OVER_PI) - f64::from(QUARTER)) * f64::from(c))
        };
        if turn > QUARTER_LOWER && turn < QUARTER_UPPER {
            pitch = blend(turn);
        } else if turn > QUARTER_UPPER || turn < -QUARTER_LOWER {
            pitch = -c;
        }
        turn = if turn > QUARTER_LOWER && turn < QUARTER_UPPER {
            narrow(
                (f64::from(turn) * f64::from(TWO_OVER_PI) - f64::from(QUARTER)) * f64::from(turn),
            )
        } else if turn > QUARTER_UPPER {
            narrow(-(core::f64::consts::PI - f64::from(turn)))
        } else if turn < -QUARTER_LOWER {
            turn + core::f32::consts::PI
        } else {
            turn
        };
        (pitch, turn)
    }
}

/// Five quarter turns, radians: a direction's angle from east taken from this is its heading.
const FIVE_QUARTERS: f64 = 7.853_981_633_974_483;

#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// What the gaze needs of the body it belongs to.
#[derive(Clone, Copy, Debug)]
pub struct Owner {
    pub is_player: bool,
    pub position: Position,
    /// Its height (its setup's, by its sizes).
    pub height: f32,
    /// Its size along each axis (its look's times its own).
    pub scale: Vec3,
    /// Its look sliders now: pitch, roll, turn.
    pub sliders: [f32; 3],
    /// The pitch and turn the server last sent for it.
    pub dir: (f32, f32),
}

/// What the gaze needs of the object looked at.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub position: Position,
    pub height: f32,
    /// It moves on its own (a creature, a player).
    pub mobile: bool,
}

/// What the player's gaze needs of the camera: its offset from what it looks at (the glide's
/// share of the way from the old one), and whether it is in mouse look (the offset then in the
/// world's frame, not the player's).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub offset: Vec3,
    pub mouse_look: bool,
}

/// A look slider asked to move to `value` over `time` seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderAsk {
    pub slider: u32,
    pub value: f32,
    pub time: f32,
}

/// What a step of the gaze asks: the slider moves, in order, and whether the player stops
/// looking at what it looked at (its object behind it or far for too long).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turned {
    pub asks: Vec<SliderAsk>,
    pub stop_looking: bool,
}

/// What a body's physics object keeps of where it looks: its gaze once it has one, the object
/// it was last asked to look at (0 for none), and the direction the server last sent for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Looking {
    pub gaze: Option<Gaze>,
    pub target: u64,
    pub dir: (f32, f32),
}

impl Looking {
    /// The gaze of body `owner`, made the first time something asks it to look.
    // D2-06077
    pub fn gaze(&mut self, owner: u64) -> &mut Gaze {
        self.gaze.get_or_insert_with(|| Gaze::new(owner))
    }
}
