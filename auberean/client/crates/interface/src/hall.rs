//! The scenes the screens before the world show in their viewports: the account's characters on
//! their arms around the pillar (character select), every race and sex standing in the avatar
//! hall (creation's first screen), and the avatar being shaped (its second); and the avatars
//! creation keeps, one per race and sex, with the choices each has.

use std::collections::BTreeMap;

use auberean_assets::ui::UiScene;
use auberean_client_sdk::protocol::types::CharacterIdentity;
use auberean_primitives::{DataId, Vec3};
use auberean_scene::view::{scene_camera, ViewCamera, ViewFigure, ViewLight, ViewScene};
use auberean_world::chargen::{self, Avatar, Choice, Random};
use auberean_world::look::{Desc, Look};
use auberean_world::math::Mat4;
use auberean_world::place::PlaceRecords;
use auberean_world::records::WorldRecords;
use dereth_primitives::num::math;

/// Character select's scene: the pillar, under the camera named below.
pub const SELECT_SCENE: u32 = 0x2100_0001;
/// The portal's scene behind the enter button.
pub const PORTAL_SCENE: u32 = 0x2100_0008;
/// The avatar hall's scene.
pub const HALL_SCENE: u32 = 0x2100_000B;
/// The camera every one of these scenes is seen by.
pub const CAMERA: &str = "Camera_1";
/// A character's arm, and the look of one whose own look does not put together.
const ARM: u32 = 0x1F00_0068;
const NO_LOOK: u32 = 0x1F00_0065;
/// Where a figure stands on its arm (the arm's holding place), in the arm's frame.
const ON_ARM: Vec3 = Vec3 {
    x: 0.022,
    y: 2.376,
    z: 0.006,
};
/// The hall's floor, which the figures stand on.
const LANDMESH: &str = "CHARGEN_LANDMESH";
/// The modes the figures stand in: in the hall, the one under the pointer, and being shaped.
const HALL_MODE: u32 = 0x4100_0006;
const HALL_UNDER_MODE: u32 = 0x4100_0007;
const SHAPING_MODE: u32 = 0x4100_0008;

/// Where each race and sex stands in the hall: the floor's holding place.
const HALL_PLACES: [(u32, u32, u32); 10] = [
    (chargen::LUGIAN, chargen::MALE, 0x8100_0001),
    (chargen::HUMAN, chargen::MALE, 0x8100_0002),
    (chargen::TUMEROK, chargen::MALE, 0x8100_0003),
    (chargen::LUGIAN, chargen::FEMALE, 0x8100_0004),
    (chargen::HUMAN, chargen::FEMALE, 0x8100_0005),
    (chargen::TUMEROK, chargen::FEMALE, 0x8100_0006),
    (chargen::EMPYREAN, chargen::MALE, 0x8100_000A),
    (chargen::EMPYREAN, chargen::FEMALE, 0x8100_000B),
    (chargen::DRUDGE, chargen::MALE, 0x8100_000E),
    (chargen::DRUDGE, chargen::FEMALE, 0x8100_000F),
];

/// Each race's scene on the appearance screen, and its viewport's name (hashed).
pub const SHAPING: [(u32, u32, u32); 5] = [
    (chargen::HUMAN, 0x2100_0007, 0xC71C_A572),
    (chargen::LUGIAN, 0x2100_0005, 0x3DFA_A90A),
    (chargen::TUMEROK, 0x2100_0006, 0x9F9D_B571),
    (chargen::DRUDGE, 0x2100_0006, 0xEF26_39DA),
    (chargen::EMPYREAN, 0x2100_000D, 0xCC9C_1728),
];

/// The heading a rotation faces: degrees clockwise from north (+y).
fn heading_of(q: auberean_primitives::Quat) -> f32 {
    let f = Mat4::from_parts(Vec3::ZERO, q, Vec3::new(1.0, 1.0, 1.0))
        .direction(Vec3::new(0.0, 1.0, 0.0));
    math::atan2f(f.x, f.y).to_degrees().rem_euclid(360.0)
}

/// A point turned by `heading` degrees clockwise about z.
fn turned(p: Vec3, heading: f32) -> Vec3 {
    let (s, c) = (
        math::sinf(heading.to_radians()),
        math::cosf(heading.to_radians()),
    );
    Vec3::new(p.x * c + p.y * s, -p.x * s + p.y * c, p.z)
}

fn rect_of(r: auberean_ui::Rect) -> [f32; 4] {
    #[allow(clippy::cast_precision_loss)]
    [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32]
}

fn plain(parent: u32) -> Desc {
    Desc {
        parent: DataId(parent),
        scale: Vec3::new(1.0, 1.0, 1.0),
        child_scale: 1.0,
        particle_scale: 1.0,
        ..Desc::default()
    }
}

/// Creation's avatars: one per race and sex, made once a creation session with random choices
/// (height and build in the middle), kept while the player goes back and forth.
#[derive(Debug, Default)]
pub struct Creation {
    // D2-11869, D2-11870, D2-11871
    pub avatars: BTreeMap<(u32, u32), Avatar>,
    pub choices: BTreeMap<(u32, u32), Vec<Choice>>,
    /// The race and sex chosen in the hall.
    pub chosen: (u32, u32),
    /// The appearance screen's view of the avatar: its heading, degrees (180 sees its front).
    pub turn: f32,
}

impl Creation {
    /// Every race and sex's avatar, made with `random`.
    // D2-01849, D2-01847
    #[must_use]
    pub fn new(records: &WorldRecords, random: &mut Random) -> Self {
        let mut me = Self {
            chosen: (chargen::HUMAN, chargen::MALE),
            turn: 180.0,
            ..Self::default()
        };
        for (race, sex, _) in HALL_PLACES {
            let c = chargen::choices(records, race, sex);
            me.avatars
                .insert((race, sex), Avatar::new(&c, race, sex, random));
            me.choices.insert((race, sex), c);
        }
        me
    }

    /// The chosen avatar and its choices.
    // D2-11861
    #[must_use]
    pub fn chosen(&self) -> Option<(&Avatar, &[Choice])> {
        Some((
            self.avatars.get(&self.chosen)?,
            self.choices.get(&self.chosen)?,
        ))
    }

    /// The look of a race and sex's avatar: its body's description with its choices.
    // D2-11914
    #[must_use]
    pub fn look(&self, records: &WorldRecords, race: u32, sex: u32) -> Option<Desc> {
        let body = records
            .entity(DataId(chargen::body_entity(race, sex)?))?
            .object?;
        let a = self.avatars.get(&(race, sex))?;
        Some(a.look(self.choices.get(&(race, sex))?, DataId(body)))
    }
}

/// Character select's scene: the carousel's arms at their headings (an arm per slot, never fewer
/// than the characters), each character standing still on its own arm with its look. The figures
/// are the characters first, in the list's order, then the arms.
// D2-01815
#[must_use]
pub fn select_scene(
    records: &WorldRecords,
    rect: auberean_ui::Rect,
    characters: &[CharacterIdentity],
    arms: &[f32],
) -> Option<ViewScene> {
    let scene: UiScene = records.get(SELECT_SCENE)?;
    let camera = scene_camera(&scene, CAMERA)?;
    let mut figures = Vec::new();
    for (i, c) in characters.iter().enumerate() {
        let Some(&h) = arms.get(i) else { break };
        let mut look = auberean_scene::desc_of(records, &c.visual);
        if (look.parent.is_none() && look.groups.is_empty()) || Look::of(records, &look).is_err() {
            look = plain(NO_LOOK);
        }
        figures.push(ViewFigure {
            look,
            origin: turned(ON_ARM, h),
            heading: h,
            mode: 0,
            still: true,
        });
    }
    for &h in arms {
        figures.push(ViewFigure {
            look: plain(ARM),
            origin: Vec3::ZERO,
            heading: h,
            mode: 0,
            still: true,
        });
    }
    Some(ViewScene {
        rect: rect_of(rect),
        scene: SELECT_SCENE,
        camera,
        light: ViewLight::Scene,
        figures,
        pick: None,
        behaviors: Vec::new(),
        under: false,
    })
}

/// The portal behind the enter button: its scene alone, under the screen's backdrop, which has a
/// hole for it. It runs from when the screen comes up, with or without a character to enter.
// D2-01820
#[must_use]
pub fn portal_scene(records: &WorldRecords, rect: auberean_ui::Rect) -> Option<ViewScene> {
    let scene: UiScene = records.get(PORTAL_SCENE)?;
    let camera = scene_camera(&scene, CAMERA)?;
    Some(ViewScene {
        rect: rect_of(rect),
        scene: PORTAL_SCENE,
        camera,
        light: ViewLight::Scene,
        figures: Vec::new(),
        pick: None,
        behaviors: Vec::new(),
        under: true,
    })
}

/// The avatar hall: every race and sex's avatar with its look, standing in the hall's mode at its
/// place on the floor (empyreans and drudges only when the account may make them: `may_make`'s
/// race flags). Returns the scene and which race and sex each figure is.
// D2-01846, D2-11939, D2-11940, D2-11942
#[must_use]
pub fn hall_scene(
    records: &WorldRecords,
    rect: auberean_ui::Rect,
    creation: &Creation,
    may_make: u32,
    under: Option<usize>,
) -> Option<(ViewScene, Vec<(u32, u32)>)> {
    let scene: UiScene = records.get(HALL_SCENE)?;
    let camera = scene_camera(&scene, CAMERA)?;
    let floor = scene
        .entities
        .iter()
        .find(|e| e.name.as_deref() == Some(LANDMESH))?;
    let at = floor.offset.map(|o| o.frame).unwrap_or_default();
    let object = DataId(floor.object?);
    let visual = if object.0 >> 24 == 0x47 {
        DataId(records.entity(object)?.object?)
    } else {
        object
    };
    let look = Look::of_record(records, visual).ok()?;
    let setup = records.setup(look.root_group()?.setup)?;
    let place = Mat4::from_parts(at.origin, at.rotation, Vec3::new(1.0, 1.0, 1.0));
    let mut figures = Vec::new();
    let mut who = Vec::new();
    for (race, sex, holding) in HALL_PLACES {
        let restricted = race & (chargen::DRUDGE | chargen::EMPYREAN);
        if restricted != 0 && may_make & restricted == 0 {
            continue;
        }
        let Some(loc) = setup.holding_locations.iter().find(|l| l.kind == holding) else {
            continue;
        };
        let Some(desc) = creation.look(records, race, sex) else {
            continue;
        };
        figures.push(ViewFigure {
            look: desc,
            origin: place.point(loc.transform.offset),
            heading: heading_of(loc.transform.rotation) + heading_of(at.rotation),
            mode: if under == Some(who.len()) {
                HALL_UNDER_MODE
            } else {
                HALL_MODE
            },
            still: false,
        });
        who.push((race, sex));
    }
    Some((
        ViewScene {
            rect: rect_of(rect),
            scene: HALL_SCENE,
            camera,
            light: ViewLight::Scene,
            figures,
            pick: None,
            behaviors: Vec::new(),
            under: false,
        },
        who,
    ))
}

/// The appearance screen's scene: the chosen avatar at the scene's middle facing north, in its
/// shaping mode; the camera framing its whole body (or its head, zoomed), turned about it by the
/// view's heading.
// D2-01867, D2-01865, D2-01863, D2-01860
#[must_use]
pub fn shaping_scene(
    records: &WorldRecords,
    rect: auberean_ui::Rect,
    creation: &Creation,
    zoomed: bool,
) -> Option<ViewScene> {
    let (race, sex) = creation.chosen;
    let &(_, scene_id, _) = SHAPING.iter().find(|s| s.0 == race)?;
    let scene: UiScene = records.get(scene_id)?;
    let fov = scene_camera(&scene, CAMERA).map_or(65.0, |c| c.fov_y);
    let desc = creation.look(records, race, sex)?;
    // The view of the whole figure is framed by its height before its scale, so the figure grows
    // and shrinks in it with the height chosen; the view close up follows it.
    let (low, high, scale) = auberean_scene::view::figure_box_and_scale(records, &desc)?;
    let mut h = high.z - low.z;
    if zoomed {
        h *= scale.z;
    }
    match race {
        chargen::HUMAN => h *= 0.95,
        chargen::TUMEROK => h *= 0.84,
        _ => {}
    }
    let (fy, fz, ft) = match (race, zoomed) {
        (chargen::LUGIAN, false) => (0.40, 0.75, 0.55),
        (chargen::LUGIAN, true) => (0.65, 0.25, 0.10),
        (chargen::TUMEROK, false) => (0.80, 0.85, 0.60),
        (chargen::TUMEROK, true) => (1.00, 0.25, 0.05),
        (_, false) => (0.75, 0.85, 0.60),
        (_, true) => (0.75, 0.35, 0.10),
    };
    let (base, hh) = if zoomed {
        (0.75 * h, 0.25 * h)
    } else {
        (0.0, h)
    };
    let r = hh * fy;
    let t = creation.turn.to_radians();
    let eye = Vec3::new(-r * math::sinf(t), -r * math::cosf(t), base + hh * fz);
    let target = Vec3::new(0.0, 0.0, base + hh * ft);
    let camera = ViewCamera::looking(eye, target, fov);
    let light = if scene
        .entities
        .iter()
        .any(|e| e.name.as_deref() == Some("Light_1"))
    {
        ViewLight::Camera
    } else {
        ViewLight::Scene
    };
    Some(ViewScene {
        rect: rect_of(rect),
        scene: scene_id,
        camera,
        light,
        figures: vec![ViewFigure {
            look: desc,
            origin: Vec3::ZERO,
            heading: 0.0,
            mode: SHAPING_MODE,
            still: false,
        }],
        pick: None,
        behaviors: Vec::new(),
        under: false,
    })
}
