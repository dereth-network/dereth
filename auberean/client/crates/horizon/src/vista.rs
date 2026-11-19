//! The view behind character select and creation, chosen in the settings ([`Choice`]): one of
//! the views ([`Vista`]), or a different one at random each time character select comes up.
//!
//! - **Cragstone**, the humans' city, seen from the slope above the lawn where players arrive in
//!   the world, toward its stone towers and arches and the mountains past them.
//! - **The Drudge Citadel**, seen from the slope over its vault's door: the drudges' great ribs and
//!   skull across the basin below, and the cliffs round it.
//! - **Arramora**, from over the rise where the Star Beacon stands: the river below and the red
//!   mesas across it.
//! - **Linvak Tukal**, the lugians' city, from the snow north-east of its nexus: its great stepped
//!   walls and the peaks past them.
//! - **Aun Tanua**, from a mountaintop across from its fortress: the fortress on its green peak,
//!   and the sea.
//! - **Ikeras**, the tumeroks' city, from over its huts: the river's islands and the peaks past
//!   them.
//!
//! The camera stands still and turns slowly right and back. The shell draws it from the install,
//! with no server ([`Request::Vista`]).
//!
//! [`Request::Vista`]: auberean_shell::interface::Request::Vista

use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_scene::explore::FreeCamera;
use dereth_primitives::num::math;

/// Where a view's camera stands and how it turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    /// The block the camera stands in.
    pub block: (i32, i32),
    /// Where in the block, meters from its south-west corner.
    pub at: Vec3,
    /// How far the camera looks down, degrees.
    pub pitch: f32,
    /// The heading the camera turns about, degrees clockwise from north, and how far it turns
    /// either way.
    pub middle: f32,
    pub swing: f32,
}

/// Cragstone's block: where players arrive in the world.
pub const BLOCK: (i32, i32) = (0x6F, 0xA9);

/// Where Cragstone's camera stands: 29 meters south of the arrival lawn and 10 meters over it, on
/// the slope looking north.
pub const AT: Vec3 = Vec3::new(45.244, 61.446, 95.046);

/// How far Cragstone's camera looks down, degrees.
pub const PITCH: f32 = -8.0;

/// The heading Cragstone's camera turns about, and how far: from due north, toward the towers, to
/// north-east, over the town.
pub const MIDDLE: f32 = 20.0;
pub const SWING: f32 = 20.0;

/// Cragstone's view.
pub const CRAGSTONE: Spot = Spot {
    block: BLOCK,
    at: AT,
    pitch: PITCH,
    middle: MIDDLE,
    swing: SWING,
};

/// The Drudge Citadel's view: the camera 18 meters over the slope above the citadel's vault, east
/// of the basin, looking west-south-west over the drudges' ribs and skull to the cliffs past them,
/// a little down; it turns 18 degrees either way.
pub const DRUDGE_CITADEL: Spot = Spot {
    block: (0x9D, 0x96),
    at: Vec3::new(15.0, 132.0, 273.7),
    pitch: -13.0,
    middle: 256.0,
    swing: 18.0,
};

/// Arramora's view: the camera 71 meters over the rise where the Star Beacon stands, looking
/// north-east down over its palms and the river to the red mesas; it turns 15 degrees either way.
pub const ARRAMORA: Spot = Spot {
    block: (0x40, 0x8E),
    at: Vec3::new(147.3, 147.6, 160.0),
    pitch: -16.0,
    middle: 45.0,
    swing: 15.0,
};

/// Linvak Tukal's view: the camera 80 meters over the snow 35 meters north of the city's nexus,
/// looking south-west down onto the city's stepped walls and the peaks past them.
pub const LINVAK_TUKAL: Spot = Spot {
    block: (0x58, 0x74),
    at: Vec3::new(156.4, 40.0, 247.1),
    pitch: -18.0,
    middle: 220.0,
    swing: 20.0,
};

/// Aun Tanua's view: the camera 15 meters over the mountaintop at the near end of the unseen
/// bridge, looking west-south-west across to the fortress on its green peak, the land below and
/// the sea.
pub const AUN_TANUA: Spot = Spot {
    block: (0xB6, 0x75),
    at: Vec3::new(124.4, 133.3, 329.8),
    pitch: -4.0,
    middle: 245.0,
    swing: 20.0,
};

/// Ikeras's view: the camera 120 meters over the middle of the block where the city's people
/// stand, looking south-west over its huts, down the river and its islands to the peaks.
pub const IKERAS: Spot = Spot {
    block: (0xCF, 0x8C),
    at: Vec3::new(80.0, 80.0, 215.0),
    pitch: -15.0,
    middle: 235.0,
    swing: 20.0,
};

/// How long a turn there and back takes, in seconds.
pub const PERIOD: f64 = 80.0;

/// The game clock the view is seen at, seconds: a fair day there.
pub const TIME: f64 = 130_248_783.7;

/// The views to choose from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vista {
    Cragstone,
    DrudgeCitadel,
    Arramora,
    LinvakTukal,
    AunTanua,
    Ikeras,
}

impl Vista {
    /// Every view, in the order the settings offer them.
    pub const ALL: [Self; 6] = [
        Self::Cragstone,
        Self::DrudgeCitadel,
        Self::Arramora,
        Self::LinvakTukal,
        Self::AunTanua,
        Self::Ikeras,
    ];

    /// Its name, as the settings show it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cragstone => "Cragstone",
            Self::DrudgeCitadel => "Drudge Citadel",
            Self::Arramora => "Arramora",
            Self::LinvakTukal => "Linvak Tukal",
            Self::AunTanua => "Aun Tanua",
            Self::Ikeras => "Ikeras",
        }
    }

    /// Its word in Horizon's settings file.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Cragstone => "cragstone",
            Self::DrudgeCitadel => "drudge-citadel",
            Self::Arramora => "arramora",
            Self::LinvakTukal => "linvak-tukal",
            Self::AunTanua => "aun-tanua",
            Self::Ikeras => "ikeras",
        }
    }

    /// The view a word of the settings file names.
    #[must_use]
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.word() == word)
    }

    /// Where its camera stands and how it turns.
    #[must_use]
    pub const fn spot(self) -> Spot {
        match self {
            Self::Cragstone => CRAGSTONE,
            Self::DrudgeCitadel => DRUDGE_CITADEL,
            Self::Arramora => ARRAMORA,
            Self::LinvakTukal => LINVAK_TUKAL,
            Self::AunTanua => AUN_TANUA,
            Self::Ikeras => IKERAS,
        }
    }

    /// Its camera at `now` (seconds on the program's clock): at the middle heading at the start of
    /// each turn, right to the far end a quarter of the way, back through the middle at half, and
    /// to the near end at three quarters.
    #[must_use]
    pub fn camera(self, now: f64) -> FreeCamera {
        let s = self.spot();
        #[allow(clippy::cast_possible_truncation)]
        let share = (now / PERIOD).rem_euclid(1.0) as f32;
        FreeCamera {
            block: s.block,
            position: s.at,
            heading: s.middle + s.swing * math::sinf(std::f32::consts::TAU * share),
            pitch: s.pitch,
            find_ground: false,
        }
    }
}

/// The view the settings choose: a different one at random each time character select comes up,
/// or always the one named.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    Random,
    One(Vista),
}

impl Choice {
    /// Every choice, in the order the settings offer them: Random, then each view.
    pub const ALL: [Self; 7] = [
        Self::Random,
        Self::One(Vista::Cragstone),
        Self::One(Vista::DrudgeCitadel),
        Self::One(Vista::Arramora),
        Self::One(Vista::LinvakTukal),
        Self::One(Vista::AunTanua),
        Self::One(Vista::Ikeras),
    ];

    /// Its name, as the settings show it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Random => "Random",
            Self::One(v) => v.name(),
        }
    }

    /// Its word in Horizon's settings file.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::One(v) => v.word(),
        }
    }

    /// The choice a word of the settings file names.
    #[must_use]
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.word() == word)
    }
}

/// Which view character select stands over: picked as character select comes up, and kept until
/// the player leaves it for the world (creation, opened from character select, keeps it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visit {
    shown: Option<Vista>,
}

impl Visit {
    /// The view to show this frame, as `choice` chooses it. One view named is that view. Random,
    /// as character select comes up, picks one of the views with `random` (the client's random
    /// numbers), never `last`, the one shown the time before; `last` becomes the one picked, and
    /// the answer says it was picked. Within a visit the view stays as it came up, whatever the
    /// settings change to, unless they name another.
    pub fn view(
        &mut self,
        choice: Choice,
        last: &mut Option<Vista>,
        random: &Random,
    ) -> (Vista, bool) {
        let (view, picked) = match (choice, self.shown) {
            (Choice::One(v), _) => (v, false),
            (Choice::Random, Some(v)) => (v, false),
            (Choice::Random, None) => (pick(*last, random), true),
        };
        if picked {
            *last = Some(view);
        }
        self.shown = Some(view);
        (view, picked)
    }

    /// Character select is left for the world: the next time it comes up is another visit.
    pub fn leave(&mut self) {
        self.shown = None;
    }
}

/// One of the views picked with `random`, any but `last`.
#[must_use]
pub fn pick(last: Option<Vista>, random: &Random) -> Vista {
    let others: Vec<Vista> = Vista::ALL
        .into_iter()
        .filter(|v| Some(*v) != last)
        .collect();
    let most = i32::try_from(others.len()).unwrap_or(1) - 1;
    let at = usize::try_from(random.roll_int(0, most)).unwrap_or(0);
    others[at.min(others.len() - 1)]
}

/// Cragstone's camera at `now`, as [`Vista::camera`].
#[must_use]
pub fn camera(now: f64) -> FreeCamera {
    Vista::Cragstone.camera(now)
}
