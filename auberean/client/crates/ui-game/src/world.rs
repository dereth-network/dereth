//! The world as the play screen's panels see it each frame: the player, the objects around it
//! with what their qualities and the player's scripts say of them, and the camera the world was
//! last drawn from, to find where a body shows on the screen.
//!
//! Places are in meters. An object's place is given in the player's own frame (x to the
//! player's right, y ahead, z up), as the client's panels measure from the player. A drawn body
//! is given in the drawn frame's block, with the box its drawn shape fills in its own frame.

/// The kind bits of an object (its number quality 20).
pub mod kind {
    /// Items: either pattern.
    pub const ITEM: [u32; 2] = [0x81, 0x103];
    /// Players.
    pub const PLAYER: u32 = 0x1F;
    /// Creatures that are not players.
    pub const CREATURE: u32 = 0x4F;
    /// The bits a player's own kind has for any object to be selectable.
    pub const ALL_SELECTABLE: u32 = 0x3F;
}

/// The radar shape of an object that does not show on the radar.
pub const NO_BLIP: u32 = 0x4000_000B;

/// The camera the world was drawn from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Camera {
    /// Where it stands, in the drawn frame's block.
    pub position: [f32; 3],
    /// Degrees clockwise from north (+y), and above level.
    pub heading: f32,
    pub pitch: f32,
    /// The vertical field of view, degrees.
    pub fov_y: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Camera {
    /// Its forward, right and up axes.
    #[must_use]
    pub fn axes(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        use dereth_primitives::num::math::{cosf, sinf};
        let (h, p) = (self.heading.to_radians(), self.pitch.to_radians());
        let f = [sinf(h) * cosf(p), cosf(h) * cosf(p), sinf(p)];
        // Right: forward across up, made one long.
        let r = [f[1], -f[0], 0.0];
        let n = (r[0] * r[0] + r[1] * r[1]).sqrt().max(1e-9);
        let r = [r[0] / n, r[1] / n, 0.0];
        let u = [
            r[1] * f[2] - r[2] * f[1],
            r[2] * f[0] - r[0] * f[2],
            r[0] * f[1] - r[1] * f[0],
        ];
        (f, r, u)
    }

    /// A point of the block through the camera to the screen's clip square (x and y from -1 to 1
    /// across the screen, y up), on a screen `aspect` wide for each unit high; and whether it is
    /// in front of the camera and on the screen.
    // D2-10533
    #[must_use]
    pub fn clip(&self, p: [f32; 3], aspect: f32) -> ([f32; 2], bool) {
        let t = dereth_primitives::num::math::tanf((self.fov_y * 0.5).to_radians());
        let (f, r, u) = self.axes();
        let d = sub(p, self.position);
        let z = dot(d, f);
        let x = dot(d, r) / (z * t * aspect);
        let y = dot(d, u) / (z * t);
        let on = z >= 0.0 && x.abs() < 1.0 && y.abs() < 1.0;
        ([x, y], on)
    }
}

/// A clip-square point on a screen `w` by `h` pixels, as the client turns one into a pixel.
// D2-10535
#[must_use]
pub fn clip_to_pixel(c: [f32; 2], w: i32, h: i32) -> (i32, i32) {
    let (fw, fh) = (f64::from(w), f64::from(h));
    let x = (1.0 / fw + f64::from(c[0]) + 1.0) * fw * 0.5;
    let y = (1.0 - (1.0 / fh + f64::from(c[1]))) * fh * 0.5;
    (
        dereth_primitives::num::to_i32_f64(x),
        dereth_primitives::num::to_i32_f64(y),
    )
}

/// A body as the world was drawn: where it stands in the block, its heading, and the box its
/// drawn shape filled in its own frame (least and most corners).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub origin: [f32; 3],
    pub heading: f32,
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

impl Body {
    /// A point of the body's own frame in the block's.
    #[must_use]
    pub fn to_block(&self, p: [f32; 3]) -> [f32; 3] {
        use dereth_primitives::num::math::{cosf, sinf};
        let (s, c) = (
            sinf(self.heading.to_radians()),
            cosf(self.heading.to_radians()),
        );
        [
            self.origin[0] + p[0] * c + p[1] * s,
            self.origin[1] - p[0] * s + p[1] * c,
            self.origin[2] + p[2],
        ]
    }
}

/// One object of the world, as the panels see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thing {
    pub id: u64,
    /// It is the player.
    pub is_player: bool,
    /// Where it stands from the player, in the player's frame; and its cell.
    pub from_player: [f32; 3],
    /// It may be selected (its yes-or-no quality 624).
    pub selectable: bool,
    /// Its kind bits (its number quality 20).
    pub kind: u32,
    /// Who owns it (a pet's master: its instance quality 505).
    pub owner: Option<u64>,
    /// What holds it (its instance quality 261).
    pub container: Option<u64>,
    /// It is dead (its yes-or-no quality 279).
    pub dead: bool,
    /// How it shows on the radar, as its scripts say: its color (red, green, blue, alpha) and
    /// shape; `None` when it does not show.
    pub radar: Option<([f32; 4], u32)>,
    /// Its groove level (its number quality 950, a byte).
    pub groove: i8,
    /// Its name as written over its head and in its tooltip (with its quantity).
    pub name: String,
    /// Its name as its scripts give it, as a string reference: the names over heads and the
    /// selection's give it to their text elements, which write it once the string tables it reads
    /// are at hand. `None` when the scripts gave none (`name` stands alone).
    pub name_info: Option<auberean_text::StringInfo>,
    /// It is drawn: its body.
    pub body: Option<Body>,
    /// Its animation has faded it out of sight: no marks are put on it.
    pub faded: bool,
    /// What the player's scripts answer of it: it is in the player's fellowship; the player may
    /// attack it on its own; its owner is in the fellowship.
    pub fellow: bool,
    pub attackable: bool,
    pub owner_fellow: bool,
}

/// The world this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    /// The player's id (0 before it is here) and its kind bits.
    pub player: u64,
    pub player_kind: u32,
    /// The player stands in an interior cell (a building, a dungeon).
    pub inside: bool,
    /// The player's cell, and the world's size in landblocks (long, wide).
    pub cell: u32,
    pub blocks: (u32, u32),
    /// The player's heading, degrees clockwise from north.
    pub heading: f32,
    /// The objects around (the player among them).
    pub things: Vec<Thing>,
    /// The selection and the one before it (0 for none); whether the player targets what attacks
    /// it on its own.
    pub selection: u64,
    pub last_selection: u64,
    pub auto_target: bool,
    /// The player's scripts would pick a target of their own as it starts to attack: it targets
    /// automatically, and what is selected is nothing it could attack.
    pub should_auto_target: bool,
    /// The radar's ranges indoors and outdoors, meters.
    pub ranges: (f32, f32),
    /// The radar's map coordinates shown; names shown over heads (the player's options).
    pub show_coordinates: bool,
    pub show_names: bool,
    /// The player is in portal space.
    pub portal_space: bool,
    /// Portal space is drawn, which lasts past the server's putting the player back.
    pub portal_drawn: bool,
    /// The camera the world was last drawn from, once it was.
    pub camera: Option<Camera>,
    /// The answer to the last pick in the world asked: the object under the pointer, or none;
    /// `None` when none was asked.
    pub picked: Option<Option<u64>>,
    /// The player moves: its body's velocity, up and down too, is more than about 0.014 m/s.
    pub moving: bool,
    /// Each fellow's share of the experience, as the player's scripts last answered.
    pub fellow_xp: Vec<(u64, f32)>,
    /// The last to attack the player with a harmful skill, as its story told the client (0 for
    /// none yet): what the last attacker's key selects.
    pub last_attacker: u64,
}

impl World {
    /// An object by id.
    #[must_use]
    pub fn thing(&self, id: u64) -> Option<&Thing> {
        if id == 0 {
            return None;
        }
        self.things.iter().find(|t| t.id == id)
    }

    /// The radar's range where the player stands.
    #[must_use]
    pub fn range(&self) -> f32 {
        if self.inside {
            self.ranges.0
        } else {
            self.ranges.1
        }
    }
}
