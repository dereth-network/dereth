//! The world a window shows, with no device and no platform: the player's body moved by the
//! world's physics and reported through the client, the bodies of everything else the client
//! knows, the chase camera under the player's control, and the frame drawn from them.
//!
//! **Depends on** the client (`auberean-client-sdk`: its model, its body and its actions), the world
//! (`auberean-world`, `auberean-assets`, `auberean-dat`), bodies (`auberean-physics`,
//! `auberean-animation`), drawing (`auberean-draw`), `auberean-primitives` and `dereth-primitives`.
//! **Used by** the shell (`auberean-shell`), which feeds it the player's intent each frame and hands
//! its frame to the host to draw.
//!
//! Each frame ([`Scene::update`]):
//!
//! - **The player.** When the client has the player's object and a place for it, the scene makes
//!   its body: the figure its visual description puts together, the collision spheres and step
//!   heights of its setup, its motion values. Whenever the server puts the player somewhere (its
//!   creation, a teleport, a forced position) the body goes there. The player's intent steers it;
//!   physics moves it in ticks of a tenth of a second against the ground, the solid objects and the
//!   interior cells around it, and between ticks it is drawn between where it was and where it is;
//!   the client is told where it is and what is asked of it, and reports it to the server by its own
//!   rule.
//! - **Everything else.** Every object in the world with a place and a visual description is drawn
//!   as its figure, with a physics body of its own steered along the path of the positions the
//!   server sends for it, a tick at a time ([`others`]), and animated by how it moved.
//! - **The camera.** The chase camera ([`camera`]) under the player's control, following the body
//!   as it is drawn, kept off the world by its traces.
//!
//! The frame's order is the client's: the player's intent first, then one schedule of physics
//! ticks for every body (the player's and everyone else's), then the camera, which follows the
//! player's body as it is drawn between its last two ticks.
//!
//! [`Scene::frame`] gives the frame: the world around the camera (rebuilt when the camera enters
//! another cell or the moment moves on), the interior cells the player's cell sees when the player
//! is inside, and the bodies.
//!
//! **Must never** reach a device, a window, a socket or a clock: time arrives from the shell.

pub mod camera;
mod figures;
mod followed;
mod fx;
mod held;
mod near;
mod notices;
pub mod others;
pub mod profile;
pub mod seen;
pub mod story;
mod temporaries;
pub mod view;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use auberean_animation::body::{Body as Figure, AIR, EARTH};
use auberean_animation::turns::Ground;
use auberean_animation::CachedRecords;
use auberean_assets::animation::{MotionDesc, MotionParams};
use auberean_client_sdk::model::Object;
use auberean_client_sdk::{Action, Client, Motion};
use auberean_draw::body::BodyDrawer;
use auberean_draw::effects::PlacedEffects;
use auberean_draw::particles::camera_axes;
use auberean_draw::portal::{Look as PortalLook, Passage};
use auberean_draw::scene::{Frame, SceneBuilder, Spot};
use auberean_draw::systems::ParticleSystems;
use auberean_physics::body::missile::ObjectTable;
use auberean_physics::body::{
    setup_spheres, Animate, Body, BodyConfig, Notice, Ticker, JUMP_BEHAVIOR, TICK,
};
use auberean_physics::land::{Land, CELL_LENGTH};
use auberean_physics::mesh::Trees;
use auberean_physics::motion::MotionInput;
use auberean_physics::world::{NoGameObjects, World};
use auberean_primitives::random::Random;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_primitives::{CellId, Frame as PFrame, LandblockId, Position};

pub use camera::CameraControl;
pub use figures::{desc_of, look_of, visual_of};

/// The motion description bodies of the player's kind move by, when the object names none.
pub const PLAYER_MOTION: u32 = 0x3200_0000;
/// A block's side, meters.
pub const BLOCK_LENGTH: f32 = 160.0;
/// The lowest cell index of an interior cell; land cells are 1 to 64.
pub const FIRST_INTERIOR: u32 = 0x100;
/// How often the world's lighting is worked out again while nothing else changes, in seconds of
/// game time, when the region's sky does not say.
pub const LIGHT_EVERY: f64 = 30.0;

/// The world's records and its region, read once.
pub struct WorldData {
    pub records: WorldRecords,
    pub region: RegionSet,
}

impl std::fmt::Debug for WorldData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldData").finish_non_exhaustive()
    }
}

impl WorldData {
    /// The world of `records`.
    ///
    /// # Errors
    /// The region does not read.
    pub fn new(records: WorldRecords) -> Result<Self, String> {
        let region = records.region().map_err(|e| format!("{e:?}"))?;
        Ok(Self { records, region })
    }
}

/// What the player asks of the body and the camera this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Intent {
    /// Ahead positive, -1 to 1.
    pub forward: f32,
    /// Right positive, -1 to 1.
    pub sideways: f32,
    /// Left positive, -1 to 1.
    pub turn: f32,
    /// Jump this frame.
    pub jump: bool,
    /// The camera controls held.
    pub held: camera::Held,
    /// The camera controls pressed this frame.
    pub presses: Vec<camera::Press>,
    /// Mouse motion this frame, in counts (x right, y toward the player).
    pub mouse: (f32, f32),
    /// The movement actions begun this frame, by the client's action numbers ([`movement`]).
    pub started: Vec<u32>,
}

/// The client's movement actions: the keys of its action map that move the player.
pub mod movement {
    pub const FORWARD: u32 = 0x4000_0020;
    pub const BACKWARD: u32 = 0x4000_0021;
    pub const STRAFE_LEFT: u32 = 0x4000_0022;
    pub const STRAFE_RIGHT: u32 = 0x4000_0023;
    pub const TURN_LEFT: u32 = 0x4000_0024;
    pub const TURN_RIGHT: u32 = 0x4000_0025;
    pub const JUMP: u32 = 0x4000_0026;
    pub const RUN_LOCK: u32 = 0x4000_0027;
}

/// The player's body: physics, figure, and what the scene last took from the client.
struct Player {
    id: u64,
    /// The description it was last sent, and its own descriptions (none when its look is a stored
    /// record's).
    visual: auberean_client_sdk::protocol::types::VisualDesc,
    dress: Option<auberean_animation::look::ObjectLook>,
    body: Body,
    figure: Figure,
    placements: u32,
    height: f32,
    /// Its setup's radius.
    radius: f32,
    scale: Vec3,
    /// Just put somewhere by the server: stood on the ground once the ground there is loaded.
    settle: bool,
    /// The behaviors its figure was last told the body shows.
    shown: BTreeSet<u32>,
    /// The effects it plays.
    effects: Option<fx::BodyFx>,
}

/// What a frame draws: the world, and portal space laid over it (`portal_alpha` of the way, the
/// rest the world showing through), then a veil over both.
#[derive(Debug, Default)]
pub struct Frames<'a> {
    pub world: Option<&'a Frame>,
    pub portal: Option<&'a Frame>,
    pub portal_alpha: f32,
    /// A color laid over everything the world shows, with its alpha.
    pub over: Option<[f32; 4]>,
}

/// How far the landscape is drawn, as its two preferences say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landscape {
    /// Blocks about the viewer's the far landscape reaches.
    pub far_radius: i32,
    /// Land cells about the viewer's that carry their frills (0: no frills).
    pub frill_radius: i32,
    /// How much of those cells' distance the frills show before they have faded.
    pub frill_reach: f32,
}

impl Landscape {
    /// The landscape's draw distance (very low to very high: none, 2, 4, 6 or 8 blocks; 4 for a
    /// value out of its range) and frill distance (disabled: none; low and medium: the cells next
    /// to the viewer's, low fading them out sooner, at 0.65 of their distance; high: two cells
    /// about it; one cell for a value out of its range) in `graphics`.
    #[must_use]
    pub fn of(graphics: &auberean_draw::quality::Graphics) -> Self {
        use auberean_draw::quality::key;
        #[allow(clippy::cast_possible_truncation)] // a few levels
        let level = |k: u32| graphics.get(k).map_or(-1, |v| v as i32);
        let far_radius = match level(key::LANDSCAPE_DRAW_DISTANCE) {
            0 => 0,
            1 => 2,
            3 => 6,
            4 => 8,
            _ => 4,
        };
        let frill = level(key::LANDSCAPE_FRILL_DISTANCE);
        Self {
            far_radius,
            frill_radius: match frill {
                0 => 0,
                3 => 2,
                _ => 1,
            },
            frill_reach: if frill == 1 { 0.65 } else { 1.0 },
        }
    }
}

/// Facts about the frame, for a status line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// The player's cell and place in its landblock, and heading.
    pub cell: u32,
    pub origin: Vec3,
    pub heading: f32,
    /// Meters a second along the ground.
    pub speed: f32,
    pub contact: bool,
    /// Bodies drawn besides the player's.
    pub others: usize,
    /// Batches in the last frame.
    pub batches: usize,
    /// Vertices in the last frame: the terrain's and the objects'.
    pub vertices: (usize, usize),
    /// The nearest bodies: id, meters, bearing in degrees.
    pub nearest: Vec<(u64, f32, f32)>,
    /// The effects the bodies' figures asked to start and to stop, counted.
    pub figure_fx: (u64, u64),
}

/// The world frame kept between frames and what it was built for.
struct Built {
    frame: Frame,
    /// The block, the cell the camera stood in (or the player's interior cell), and the light's
    /// moment.
    key: (LandblockId, (i32, i32), u32),
    at: f64,
    /// How many object batches are the world's; the rest are the bodies of this frame.
    world_objects: usize,
    /// Inside, the day outside the frame's lights shine with.
    outside: Option<auberean_world::inside::Outside>,
}

/// The world a window shows.
pub struct Scene<'w> {
    world: &'w WorldData,
    anims: CachedRecords<'w>,
    builder: SceneBuilder<'w>,
    drawer: BodyDrawer,
    /// The meshes' connectivity the dynamic objects' volumes of shadow are made from.
    volumes: auberean_draw::volume::VolumeMaker,
    /// The dynamic object shadows preference (`auberean_draw::volume::setting`).
    object_shadows: u32,
    /// The decals laid on the ground, and the layers of each decal material.
    decals: auberean_draw::decal::DecalManager,
    decal_layers: HashMap<u32, Vec<auberean_draw::material::DrawLayer>>,
    /// The light source effects preference: the lights effects place, and their pools on the land.
    light_effects: bool,
    /// The lights the objects' effects carried in the last frame drawn, as they stood.
    carried: Vec<auberean_world::inside::Light>,
    land: Land,
    trees: Trees,
    /// The blocks whose interior cells were loaded.
    blocks: HashSet<(u8, u8)>,
    /// The landblocks kept about the player.
    near: near::NearBlocks,
    pub camera: CameraControl,
    player: Option<Player>,
    /// When physics ticks, for every body.
    ticker: Ticker,
    /// The world every body moves against: the player's, everyone else's, the objects with a
    /// shape and the scenery, and the cells' collision lists.
    table: World,
    others: others::Others,
    built: Option<Built>,
    /// When the world's lighting is next worked out again (game time), and how many times it has
    /// been.
    light_at: f64,
    lightings: u32,
    /// The vertical field of view, degrees.
    pub fov: f32,
    /// The last frame's facts.
    pub status: Status,
    /// The game clock's seconds, for the light; the shell sets it from the client.
    pub game_time: f64,
    /// The interface's viewport scenes.
    pub views: view::Views,
    /// What the last world frame showed: its camera and the bodies drawn in it.
    pub seen: seen::Seen,
    /// The bodies drawn in the last world frame: each one's id and its batches.
    drawn: Vec<(u64, std::ops::Range<usize>)>,
    /// Portal space, and what it draws this moment.
    pub portal: Passage,
    portal_look: PortalLook,
    portal_frame: Option<Frame>,
    /// The frames are built for this frame ([`Scene::build_frames`]): whether the world's was.
    prepared: Option<bool>,
    /// In a world session (entering it or in it) since the last frame.
    in_session: bool,
    /// The interface asked to enter the world and the player is not there yet; the shell sets it.
    pub entering: bool,
    /// The player's teleport stamp last seen.
    teleport_stamp: Option<u16>,
    /// The effects of the objects drawn with the world.
    placed_effects: PlacedEffects,
    /// The particle systems every object's effects keep going.
    systems: ParticleSystems,
    /// The other objects as the bodies' guidance sees them, refreshed each frame.
    objects: ObjectTable,
    /// Whether the camera's shake has drawn its starts along the noise (once, the first time a
    /// shake moves it).
    shake_drawn: bool,
    /// What the bodies hold.
    held: held::HeldObjects,
    /// The kinds of mesh the player's own body leaves out when drawn with the camera at its eyes.
    first_person_hidden: Vec<u32>,
    /// Which of the camera's stand-ins hang from the player: the one it hangs from, the one it
    /// looks at.
    stand_ins: [bool; 2],
    /// Effects asked of objects since the last frame: the object, the effect, its strength.
    asked_fx: Vec<(u64, u32, f32)>,
    /// Effects asked of objects or stopped since the last frame, in order, for their bodies: the
    /// object, the effect, and its strength (none to stop it).
    body_fx: Vec<(u64, u32, Option<f32>)>,
    /// What the stories did on the bodies, counted.
    pub combat: story::StoryTally,
    /// The story player of each body a story was told on.
    stories: BTreeMap<u64, auberean_physics::story::StoryPlayer<story::Act>>,
    /// The objects the client made itself: the missiles of the stories.
    temporaries: temporaries::Temporaries,
    /// What the scripts last set on each body, and the handle in the world's table it was applied
    /// under (none for a body not in it).
    states: BTreeMap<u64, (Option<u32>, auberean_client_sdk::runtime::body::BodyState)>,
}

impl std::fmt::Debug for Scene<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

fn dv(v: dereth_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Where a body stands, as its figure's slope lean reads it: in touch or not, sliding or not,
/// and the plane it stands on turned into its own frame.
pub(crate) fn ground_of(body: &Body) -> Ground {
    let normal = body.contact_plane.map_or(Vec3::ZERO, |(plane, _)| {
        dv(auberean_physics::geom::global_to_local(
            &body.position.frame,
            plane.normal,
        ))
    });
    Ground {
        contact: body.contact,
        sliding: body.is_sliding_down(),
        normal,
        placed: true,
    }
}

fn pv(v: Vec3) -> dereth_primitives::Vec3 {
    dereth_primitives::Vec3::new(v.x, v.y, v.z)
}

/// The place a placement on the wire names.
fn position_of_placement(p: &auberean_client_sdk::protocol::types::Placement) -> Position {
    let q = p.frame.rotation;
    Position::new(
        CellId(p.cell),
        PFrame::new(
            pv(p.frame.origin),
            dereth_primitives::Quat {
                w: q.w,
                x: q.x,
                y: q.y,
                z: q.z,
            },
        ),
    )
}

/// A move the server orders, as the body takes it: each part the order carries (by its flags),
/// the rest as a move made afresh has them.
fn movement_parameters(
    w: &auberean_client_sdk::protocol::types::MovementParams,
) -> auberean_physics::motion::params::MovementParameters {
    use auberean_client_sdk::protocol::types::{MoveTarget, MovementParams as Wire};
    let mut p = auberean_physics::motion::params::MovementParameters {
        flags: w.flags,
        ..Default::default()
    };
    if w.flags & (Wire::MAX_SUCCESS | Wire::MIN_SUCCESS) != 0 {
        p.desired_velocity = w.desired_velocity;
        p.fail_distance = w.fail_distance;
        p.desired_distance = w.desired_distance;
    }
    if w.flags & Wire::MAX_SUCCESS != 0 {
        p.max_success_distance = w.max_success_distance;
    }
    if w.flags & Wire::MIN_SUCCESS != 0 {
        p.min_success_distance = w.min_success_distance;
    }
    match &w.target {
        MoveTarget::Object(id) => p.target_object = *id,
        MoveTarget::Place(at) => p.target_position = position_of_placement(at),
    }
    p.offset = pv(w.offset);
    p.offset_id = w.offset_id;
    p.waypoints = w.waypoints.iter().map(position_of_placement).collect();
    p.context = w.context;
    p
}

fn dq(q: dereth_primitives::Quat) -> Quat {
    Quat {
        w: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }
}

/// The light of a frame inside: the block's interior ambient; for what stands in the cells, the
/// lights nearest `at` taken together as one light from their side; the interior fog, or none.
// D2-11007, D2-10629
fn inside_light(
    light: &mut auberean_world::sky::DayLight,
    lights: Option<&auberean_world::inside::BlockLights>,
    at: Vec3,
    outside: &auberean_world::inside::Outside,
) {
    use auberean_world::color::Color;
    use auberean_world::math::{length, normalize, sub};
    light.fog_min = 0.0;
    light.fog_max = 0.0;
    let Some(l) = lights else {
        return;
    };
    light.ambient_color = l.ambient;
    light.ambient_brightness = 1.0;
    let mut color = Color::new(0.0, 0.0, 0.0, 1.0);
    let mut from = Vec3::new(0.0, 0.0, 0.0);
    for (origin, shine, range) in l.chosen(at, 1.0, outside) {
        let to = sub(at, origin);
        let d = length(to);
        let k = 1.0 / (1.0 + 10.0 * d * d / (range * range));
        color = Color::new(
            color.r + shine.r * k,
            color.g + shine.g * k,
            color.b + shine.b * k,
            1.0,
        );
        let w = (shine.r + shine.g + shine.b) * k;
        let n = normalize(to);
        from = Vec3::new(from.x + n.x * w, from.y + n.y * w, from.z + n.z * w);
    }
    light.direct_color = Color::new(color.r.min(1.0), color.g.min(1.0), color.b.min(1.0), 1.0);
    light.direct_brightness = 1.0;
    if length(from) > 0.0 {
        light.light_direction = normalize(from);
    }
    if let Some((c, near, far)) = l.fog {
        light.fog_color = c;
        light.fog_min = near;
        light.fog_max = far;
    }
}

/// The block a cell id names.
#[must_use]
pub fn block_of(cell: u32) -> LandblockId {
    #[allow(clippy::cast_possible_truncation)]
    LandblockId::new((cell >> 24) as u8, (cell >> 16) as u8)
}

/// Whether a cell id names an interior cell.
#[must_use]
pub fn is_interior(cell: u32) -> bool {
    cell & 0xFFFF >= FIRST_INTERIOR
}

pub(crate) fn motion_values(md: &MotionDesc, element: u32, mode: u32) -> Option<MotionParams> {
    let (_, set) = md
        .set_of
        .iter()
        .find(|(k, _)| k.element == element && k.mode == mode)?;
    md.sets.get(*set as usize).map(|s| s.default)
}

/// What a body of `look` put together as `figure` is for physics: the collision spheres and step
/// heights of its setup, its scale, and its motion description's values on the ground and in the
/// air in the figure's mode; and its setup's height.
pub(crate) fn body_config(
    records: &WorldRecords,
    look: &auberean_animation::look::Look,
    figure: &Figure,
    _physics: &auberean_client_sdk::protocol::types::PhysicsDesc,
) -> Option<(BodyConfig, f32)> {
    let setup = figure.parts.first()?.setup.clone();
    let motion = if look.motion_desc.is_none() {
        PLAYER_MOTION
    } else {
        look.motion_desc.0
    };
    let md: MotionDesc = records.get(motion)?;
    let ground = motion_values(&md, EARTH, figure.mode)?;
    let air = motion_values(&md, AIR, figure.mode).unwrap_or(ground);
    Some((
        BodyConfig {
            spheres: setup_spheres(&setup),
            step_up: setup.step_up_height,
            step_down: setup.step_down_height,
            scale: look.scale.z.max(0.01),
            ground,
            air,
        },
        setup.height,
    ))
}

/// `body` takes the part group `look` puts together (each mesh read once through `trees`) and ends
/// its making with it, the scale its look gives it and the scale its look gives what it holds.
pub(crate) fn give_parts(
    body: &mut Body,
    records: &WorldRecords,
    trees: &mut Trees,
    look: &auberean_animation::look::Look,
) {
    let mut parts = auberean_physics::parts::PartGroup::default();
    if parts.init_look(
        &auberean_physics::parts::CachedParts::new(records, trees),
        look,
    ) {
        body.set_parts(parts);
        body.init_object_end();
    }
    body.look_scale = pv(look.scale);
    body.child_scale = look.child_scale;
}

/// What a body keeps when its object's look becomes `look` (none: it has no look to draw): its
/// figure brought up to date with it, keeping what it plays (made when it has none), and its
/// effects made again from it with the effects that were playing asked again.
// D2-06002, D2-06551
pub(crate) fn takes_look(
    anims: &CachedRecords<'_>,
    figure: &mut Option<Figure>,
    effects: &mut Option<fx::BodyFx>,
    look: Option<&auberean_animation::look::Look>,
    id: u64,
) {
    let Some(look) = look else {
        *figure = None;
        *effects = None;
        return;
    };
    let kept = figure
        .as_mut()
        .is_some_and(|f| f.compare_and_modify(anims, look));
    if !kept {
        *figure = Figure::from_look(anims, look);
    }
    *effects = fx::BodyFx::again(effects.take(), look, id);
}

/// `body` brought up to date with its object's changed look: its part group compared with the
/// look's groups and changed where they differ (each mesh read once through `trees`), the scale
/// its look gives it and what it holds, and the end of its making again.
// D2-06551
pub(crate) fn body_takes_look(
    body: &mut Body,
    records: &WorldRecords,
    trees: &mut Trees,
    look: &auberean_animation::look::Look,
) {
    body.update_parts_to_look(
        &auberean_physics::parts::CachedParts::new(records, trees),
        look,
    );
    body.look_scale = pv(look.scale);
    body.child_scale = look.child_scale;
    body.init_object_end();
}

/// The game's names of the kinds of mesh, and the names of the ones the player's own body leaves
/// out when it is drawn with the camera at its eyes: its head, its beard, its crest and its totem.
const MESH_KINDS: u32 = 0x4000_000F;
const FIRST_PERSON_HIDDEN: [&str; 4] = ["Head", "Beard", "Crest", "Totem"];

/// The kinds of mesh the player's own body leaves out when it is drawn with the camera at its
/// eyes, each looked up by its name among the game's mesh kinds (in their map, then in the maps it
/// extends); a name the data lacks hides nothing.
fn first_person_hidden(records: &WorldRecords) -> Vec<u32> {
    use auberean_assets::strings::EnumMap;
    let first = records.data_id_by_enum(MESH_KINDS, auberean_assets::database::ENUM_MAP_MAPPING);
    let mut maps = Vec::new();
    let mut next = first;
    while next != 0 && maps.len() < 16 {
        let Some(m) = records.get::<EnumMap>(next) else {
            break;
        };
        next = m.base;
        maps.push(m);
    }
    FIRST_PERSON_HIDDEN
        .iter()
        .filter_map(|name| maps.iter().find_map(|m| m.enum_of(name)))
        .collect()
}

/// The objects the camera reads this frame: the player (drawn `alpha` of the way through its
/// tick), the camera's stand-ins on it, everyone else.
fn followed_of<'a, 'w>(
    player: Option<&'a Player>,
    others: &'a others::Others,
    anims: &'a CachedRecords<'w>,
    alpha: f32,
    stand_ins: [bool; 2],
) -> followed::Followed<'a, 'w> {
    followed::Followed {
        player: player.map(|p| followed::Player {
            id: p.id,
            body: &p.body,
            figure: &p.figure,
            height: p.height * p.scale.z,
            drawn: p.body.drawn(alpha).0,
        }),
        others,
        anims,
        alpha,
        stand_ins,
    }
}

/// The player's body at `pos` facing `heading`, as the camera follows it.
fn subject_of(p: &Player, pos: Position, heading: f32) -> camera::Subject {
    camera::Subject {
        origin: pos.frame.origin,
        heading,
        height: p.height * p.scale.z,
        radius: p.radius * p.scale.z,
        scale_z: p.scale.z,
        local_velocity: p.body.local_velocity(),
    }
}

/// The behaviors the body's motion interpreter started or stopped over the frame's ticks, played
/// or stopped on its figure, as the client hands its physics object's behaviors to its animation.
pub(crate) fn forward_behaviors(
    body: &Body,
    shown: &mut BTreeSet<u32>,
    figure: &mut Figure,
    anims: &CachedRecords<'_>,
) {
    let started: Vec<u32> = body.behaviors.difference(shown).copied().collect();
    let stopped: Vec<u32> = shown.difference(&body.behaviors).copied().collect();
    for b in started {
        figure.do_autonomous_behavior(anims, b);
    }
    for b in stopped {
        figure.stop_behavior(anims, b);
    }
    shown.clone_from(&body.behaviors);
}

/// A behavior's parameters as a script sent them, as the animation takes them.
pub(crate) fn params_of(
    p: &auberean_client_sdk::script::native_object::BehaviorParams,
) -> auberean_animation::track::Params {
    let on = |w: u32| w != 0;
    auberean_animation::track::Params {
        behavior: p.behavior,
        time_scale: f32::from_bits(p.time_scale),
        mode: p.mode,
        hold_cycles: p.hold_cycles,
        fx_script: p.fx_script,
        fx: p.fx,
        target: p.target,
        camera_target: p.camera_target,
        camera_behavior: p.camera_behavior,
        clone_visual: p.visual_to_clone,
        clone_appearance: p.cloned_appearance,
        clone_strengths: p
            .cloned_keys
            .1
            .iter()
            .map(|(k, v)| (*k, f32::from_bits(*v)))
            .collect(),
        impulse: auberean_primitives::Vec3::new(
            f32::from_bits(p.impulse[0]),
            f32::from_bits(p.impulse[1]),
            f32::from_bits(p.impulse[2]),
        ),
        early_callback: on(p.early_callback),
        move_to_cancels: on(p.move_to_cancels),
        camera_parent: on(p.camera_parented),
        camera_targets: on(p.camera_targeted),
        camera_hold: on(p.camera_hold),
        camera_restore: on(p.camera_restore),
        fade_children: on(p.fade_children),
        propagate: on(p.propagate),
        lock_actions: on(p.lock_actions),
        send_event: on(p.send_event),
        emote: p.weenie_emote,
        leave_idle_alone: on(p.leave_idle_alone),
        destroy_on_completion: on(p.destroy_on_completion),
        context: p.context,
    }
}

/// The figure of object `id`: the player's, another body's, a held object's, or one of the objects
/// the client made itself.
pub(crate) fn figure_in<'a>(
    player: &'a mut Option<Player>,
    others: &'a mut others::Others,
    held: &'a mut held::HeldObjects,
    temporaries: &'a mut temporaries::Temporaries,
    id: u64,
) -> Option<&'a mut Figure> {
    match player {
        Some(p) if p.id == id => Some(&mut p.figure),
        _ => others
            .figure_mut(id)
            .or_else(|| held.figure_mut(id))
            .or_else(|| temporaries.figure_mut(id)),
    }
}

/// What a figure's animations set off this frame, played as the client plays a body's trigger
/// update after its animation: the effects played once and those starting are asked of the body
/// (or stopped, for a stopping trigger) among the figure's events; the kinds of its hooks, in
/// order, and its impulses are handed back; the effects stopping are stopped; then the update is
/// cleaned up, what started counting as running from then on.
pub(crate) fn take_triggers(
    figure: &mut Figure,
) -> (Vec<u32>, Vec<auberean_assets::animation::Impulse>) {
    use auberean_animation::host::Event;
    let fx = |t: &auberean_animation::triggers::Trigger| {
        if t.stop {
            Event::StopFx { fx: t.effect }
        } else {
            Event::DoFx {
                fx: t.effect,
                intensity: t.intensity,
            }
        }
    };
    let t = &mut figure.triggers;
    let mut events: Vec<Event> = t.once.iter().map(|(_, tr)| fx(tr)).collect();
    t.once.clear();
    events.extend(
        t.starting
            .iter()
            .filter(|(_, tr)| tr.play)
            .map(|(_, tr)| fx(tr)),
    );
    let hooks = std::mem::take(&mut t.hooks)
        .into_iter()
        .map(|h| h.kind)
        .collect();
    let impulses = std::mem::take(&mut t.impulses);
    events.extend(t.stopping.iter().map(|(k, _)| Event::StopFx { fx: k }));
    t.clean_up();
    figure.events.extend(events);
    (hooks, impulses)
}

/// The speed scale the server gave an object's body, if any.
pub(crate) fn velocity_scale(p: &auberean_client_sdk::protocol::types::PhysicsDesc) -> Option<f32> {
    (p.bits & auberean_client_sdk::protocol::types::PhysicsDesc::VELOCITY_SCALE != 0
        && p.velocity_scale > 0.0)
        .then_some(p.velocity_scale)
}

impl<'w> Scene<'w> {
    /// The scene of `world`, drawn by the graphics preferences `graphics` from its first frame, as
    /// the client reads them when it starts (see [`Self::init_preferences`]).
    #[must_use]
    pub fn new(world: &'w WorldData, graphics: &auberean_draw::quality::Graphics) -> Self {
        let mut scene = Self {
            world,
            anims: CachedRecords::new(&world.records),
            builder: SceneBuilder::new(&world.records, &world.region),
            drawer: BodyDrawer::default(),
            volumes: auberean_draw::volume::VolumeMaker::default(),
            object_shadows: auberean_draw::volume::setting::VOLUMETRIC,
            decals: auberean_draw::decal::DecalManager::default(),
            decal_layers: HashMap::new(),
            light_effects: true,
            carried: Vec::new(),
            land: Land::new(),
            trees: Trees::default(),
            blocks: HashSet::new(),
            near: near::NearBlocks::default(),
            camera: CameraControl::default(),
            player: None,
            ticker: Ticker::new(0.0),
            table: {
                // The physics types' bits come from the install's property list.
                let mut t = World::new();
                if let Some(list) = world
                    .records
                    .get::<auberean_assets::property::PropertyList>(0x3400_0000)
                {
                    t.set_property_list(list);
                }
                t
            },
            others: others::Others::default(),
            seen: seen::Seen::default(),
            drawn: Vec::new(),
            built: None,
            light_at: f64::NEG_INFINITY,
            lightings: 0,
            fov: auberean_physics::camera::BASE_FOV + auberean_physics::camera::MIN_FOV_EFFECTS,
            status: Status::default(),
            game_time: 0.0,
            views: view::Views::default(),
            portal: Passage::new(Random::new()),
            portal_look: Passage::new(Random::new()).think(0.0),
            portal_frame: None,
            prepared: None,
            in_session: false,
            entering: false,
            teleport_stamp: None,
            placed_effects: PlacedEffects::default(),
            systems: ParticleSystems::default(),
            objects: ObjectTable::default(),
            shake_drawn: false,
            held: held::HeldObjects::default(),
            first_person_hidden: first_person_hidden(&world.records),
            stand_ins: [false; 2],
            asked_fx: Vec::new(),
            body_fx: Vec::new(),
            combat: story::StoryTally::default(),
            stories: BTreeMap::new(),
            temporaries: temporaries::Temporaries::default(),
            states: BTreeMap::new(),
        };
        scene.init_preferences(graphics);
        scene
    }

    /// Whether portal space is drawn at all this moment.
    // D2-10645, D2-11003
    #[must_use]
    pub fn in_portal_space(&self) -> bool {
        self.portal.on()
    }

    /// Portal space follows the session: on at once when the player starts entering the world;
    /// then wanted until the server has the player in the world (it took the player out of it for
    /// a trip, or has not said who the player is yet) and the client has the player's body placed
    /// with the world around it loaded; a teleport of the player wants it again.
    // D2-01910, D2-01944, D2-01909, D2-10657, D2-10665, D2-10643, D2-10644, D2-11001, D2-11002
    fn follow_portal_space(&mut self, client: &Client, now: f64) {
        let in_session = self.entering || client.model.player.is_some();
        if !in_session {
            if self.in_session {
                self.portal.want(false, true, now);
            }
            self.in_session = false;
            self.teleport_stamp = None;
        } else {
            if !self.in_session {
                self.portal.want(true, true, now);
            }
            self.in_session = true;
            let server_ready = client.model.player_object().is_some() && !client.in_portal_space();
            let stamp = client.body.teleport_stamp;
            let teleported = self.teleport_stamp.is_some_and(|s| s != stamp);
            if client.body.placed() {
                self.teleport_stamp = Some(stamp);
            }
            let client_ready = self.player.is_some() && !teleported;
            self.portal
                .want(!(server_ready && client_ready), false, now);
        }
        self.portal_look = self.portal.think(now);
    }

    /// Builds what [`Scene::frames`] hands out at `now`: the world's frame (when `world` and
    /// portal space leaves it in sight) and portal space's. The world's camera and the bodies
    /// drawn are then in [`Scene::seen`].
    pub fn build_frames(&mut self, now: f64, world: bool) {
        let look = self.portal_look.clone();
        let world_drawn = world && look.draw_world && self.build_world(now, look.world_fov);
        self.portal_frame = self
            .portal
            .frame(&self.world.records, &mut self.builder, &look, now);
        self.prepared = Some(world_drawn);
    }

    /// What to draw at `now`: the world when the player is in it and portal space leaves it in
    /// sight, and portal space when it is on; over them portal space's veil, or the camera's
    /// screen flash. They are built here unless [`Scene::build_frames`] built them this frame.
    // D2-10659
    pub fn frames(&mut self, now: f64, world: bool) -> Frames<'_> {
        if self.prepared.is_none() {
            self.build_frames(now, world);
        }
        let world_drawn = self.prepared.take().unwrap_or(false);
        let look = &self.portal_look;
        let over_world = |a: Option<[f32; 4]>| a.map_or(0.0, |c| c[3]);
        Frames {
            world: if world_drawn {
                self.built.as_ref().map(|b| &b.frame)
            } else {
                None
            },
            portal: self.portal_frame.as_ref(),
            portal_alpha: if world_drawn {
                over_world(look.under)
            } else {
                1.0
            },
            // Portal space's veil, else the camera's screen flash while one is under way.
            over: look.over.or_else(|| {
                let flash = self.camera.effects.flash;
                (flash[3] != 0.0).then_some(flash)
            }),
        }
    }

    /// Draw every random number of the world from `random`, the client's one generator: the
    /// bodies' animations, their effects' choices and particle systems, missiles' turns, portal
    /// space's tunnels and the camera's shake.
    pub fn set_random(&mut self, random: &Random) {
        self.anims.dice = random.clone();
        self.builder.dice = random.clone();
        self.systems.set_dice(random);
        self.objects.dice = random.clone();
        self.portal.set_dice(random.clone());
    }

    /// The graphics preferences the world is drawn by, as the client takes them when it starts and
    /// whenever the player applies them: the landscape's ([`Landscape::of`]), and the model
    /// detail, the water's reflections, the landscape's object and cloud shadows, the dynamic
    /// objects' shadows, the light coronas and the lights effects place (and their pools) for the
    /// rest of the world's drawing. The world is drawn again by them at once.
    // D2-10656
    pub fn init_preferences(&mut self, graphics: &auberean_draw::quality::Graphics) {
        use auberean_draw::quality::key;
        let l = Landscape::of(graphics);
        self.builder.set_far_radius(l.far_radius);
        self.builder
            .set_frills(l.frill_radius != 0, l.frill_radius, l.frill_reach);
        #[allow(clippy::cast_possible_truncation)] // a few levels
        let detail = graphics.get(key::MODEL_DETAIL).map_or(2, |v| v as i32);
        self.builder.model_detail = u32::try_from(detail.clamp(0, 2)).unwrap_or(2);
        let on = |k: u32| graphics.get(k).is_some_and(|v| v != 0.0);
        self.builder.water_reflections = on(key::WATER_REFLECTIONS);
        self.builder.static_shadows = on(key::LANDSCAPE_STATIC_OBJECT_SHADOWS);
        self.builder.cloud_shadows = on(key::LANDSCAPE_CLOUD_SHADOWS);
        self.builder.light_coronas = on(key::LIGHT_CORONAS);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few choices
        let shadows = graphics
            .get(key::DYNAMIC_OBJECT_SHADOWS)
            .map_or(auberean_draw::volume::setting::VOLUMETRIC, |v| v as u32);
        self.object_shadows = shadows;
        self.light_effects = on(key::LIGHT_SOURCE_EFFECTS);
        self.systems.set_light_sources(self.light_effects);
        self.built = None;
    }

    /// The lights the objects' effects carried in the last frame drawn, as they stood there (each
    /// at its place in its own cell's block): those of the objects placed with the world, the
    /// bodies, what they hold and what the client made.
    #[must_use]
    pub fn carried_lights(&self) -> &[auberean_world::inside::Light] {
        &self.carried
    }

    /// The dynamic objects' shadows the world is drawn with (`auberean_draw::volume::setting`).
    #[must_use]
    pub const fn object_shadows(&self) -> u32 {
        self.object_shadows
    }

    /// Whether the effects place the lights they name, and those lights lay their pools.
    #[must_use]
    pub fn light_source_effects(&self) -> bool {
        self.light_effects && self.systems.light_sources()
    }

    /// The world's land, as physics has loaded it.
    #[must_use]
    pub fn land(&self) -> &Land {
        &self.land
    }

    /// Whether the scene has the player's body.
    #[must_use]
    pub fn has_player(&self) -> bool {
        self.player.is_some()
    }

    /// The player's figure: what it plays and the element it is in.
    #[must_use]
    pub fn player_figure(&self) -> Option<&Figure> {
        self.player.as_ref().map(|p| &p.figure)
    }

    /// Asks the player's figure into mode `mode` (out of peace into combat, and back), as the
    /// game asks the player's object; whether it took the request.
    pub fn player_do_mode(&mut self, mode: u32) -> bool {
        let anims = &self.anims;
        self.player
            .as_mut()
            .is_some_and(|p| p.figure.do_mode(anims, 0, mode))
    }

    /// Plays effect `effect` on object `object` at strength `intensity`, as the game asks an
    /// object for one: its camera part plays on the next frame, its particles where the body next
    /// stands.
    // D2-06162
    pub fn do_fx(&mut self, object: u64, effect: u32, intensity: f32) {
        self.asked_fx.push((object, effect, intensity));
        self.body_fx.push((object, effect, Some(intensity)));
    }

    /// A move the server orders object `object`'s body to make: only the player's body takes one
    /// (turning to face what it attacks, walking up to what it uses).
    pub(crate) fn recv_move_to(
        &mut self,
        object: u64,
        order: &auberean_client_sdk::protocol::physics::MoveOrder,
    ) {
        let Some(p) = self.player.as_mut().filter(|p| p.id == object) else {
            return;
        };
        let start = p.body.notices.len();
        let _ = p
            .body
            .receive_move_to(&movement_parameters(&order.params), order.stamp);
        // The behaviors a move cancels stop as it is taken, before what the server asks after it.
        let mut i = start;
        while i < p.body.notices.len() {
            if matches!(
                p.body.notices[i],
                Notice::Animate(Animate::StopMoveToCancels)
            ) {
                p.body.notices.remove(i);
                p.figure.stop_move_to_cancels(&self.anims);
            } else {
                i += 1;
            }
        }
    }

    /// Where the object the player's body moves or turns to stands, kept on the body for its move:
    /// another body in the world (its place, radius and height); none when it has no such body.
    fn keep_move_target(&mut self) {
        let Some(p) = self.player.as_mut() else {
            return;
        };
        let target = p
            .body
            .minterp
            .move_to
            .as_ref()
            .filter(|m| m.moving_to && m.params.to_object())
            .map(|m| m.params.target_object);
        p.body.targets.clear();
        let Some(id) = target else {
            return;
        };
        let Some(b) = self
            .others
            .body(id)
            .or_else(|| self.held.body(id))
            .filter(|b| b.in_world)
        else {
            return;
        };
        p.body.targets.insert(
            id,
            auberean_physics::motion::moveto::TargetBody {
                position: b.position,
                radius: b.radius(),
                height: b.height(),
            },
        );
    }

    /// Stops effect `effect` on object `object`, where its body next stands.
    // D2-06164
    pub fn stop_fx(&mut self, object: u64, effect: u32) {
        self.body_fx.push((object, effect, None));
    }

    /// The effects asked of the bodies and stopped, each to its body's effects (a body without
    /// effects plays none).
    fn ask_body_fx(&mut self) {
        for (id, effect, play) in std::mem::take(&mut self.body_fx) {
            let fx = match self.player.as_mut() {
                Some(p) if p.id == id => p.effects.as_mut(),
                _ => {
                    if self.others.ask_fx(id, effect, play) {
                        continue;
                    }
                    match self.held.effects_mut(id) {
                        Some((fx, _)) => Some(fx),
                        None => self
                            .temporaries
                            .get_mut(id)
                            .and_then(|t| t.effects.as_mut()),
                    }
                }
            };
            if let Some(fx) = fx {
                fx.asked.push((effect, play));
            }
        }
    }

    /// Whether object `id` has a body here: the player's, another's in the world, a held object's,
    /// or one the client made itself.
    pub(crate) fn has_body(&self, id: u64) -> bool {
        self.player.as_ref().is_some_and(|p| p.id == id)
            || self.others.body(id).is_some()
            || self.held.body(id).is_some()
            || self.temporaries.has(id)
    }

    /// The world's table of what the bodies meet.
    #[must_use]
    pub fn table(&self) -> &World {
        &self.table
    }

    /// The physics body of object `id`: the player's, another's, a held object's or one the client
    /// made itself.
    #[must_use]
    pub fn body(&self, id: u64) -> Option<&Body> {
        match &self.player {
            Some(p) if p.id == id => Some(&p.body),
            _ => self
                .others
                .body(id)
                .or_else(|| self.held.body(id))
                .or_else(|| self.temporaries.get(id).map(|t| &t.body)),
        }
    }

    /// Object `id`'s own descriptions as its body keeps them: its own, the appearance its
    /// effects lay over it laid on, and that appearance; none without a body, or for one whose
    /// look is a stored record's.
    #[must_use]
    pub fn dress(&self, id: u64) -> Option<&auberean_animation::look::ObjectLook> {
        match &self.player {
            Some(p) if p.id == id => p.dress.as_ref(),
            _ => self
                .others
                .dress(id)
                .or_else(|| self.held.dress(id))
                .or_else(|| self.temporaries.get(id).and_then(|t| t.dress.as_ref())),
        }
    }

    /// The figure of object `id`: the player's, another's, a held object's or one the client made.
    #[must_use]
    pub fn figure(&self, id: u64) -> Option<&Figure> {
        match &self.player {
            Some(p) if p.id == id => Some(&p.figure),
            _ => self
                .others
                .figure(id)
                .or_else(|| self.held.figure(id))
                .or_else(|| self.temporaries.get(id).and_then(|t| t.figure.as_ref())),
        }
    }

    /// The figure of object `id` (see [`figure_in`]).
    pub(crate) fn figure_mut(&mut self, id: u64) -> Option<&mut Figure> {
        figure_in(
            &mut self.player,
            &mut self.others,
            &mut self.held,
            &mut self.temporaries,
            id,
        )
    }

    /// Plays a behavior on object `id`'s body, as the client plays one asked of an object: on its
    /// figure; and when it is passed on, the behavior alone (at its time scale, sending nothing,
    /// passed on again) on everything the body holds. Whether the figure took it: a body with no
    /// figure cannot.
    // D2-06263
    pub(crate) fn play_behavior(
        &mut self,
        id: u64,
        params: &auberean_animation::track::Params,
    ) -> bool {
        let anims = &self.anims;
        let Some(f) = figure_in(
            &mut self.player,
            &mut self.others,
            &mut self.held,
            &mut self.temporaries,
            id,
        ) else {
            return false;
        };
        // A behavior that carries a mode puts the figure in it first (a death behavior, its body
        // then staying down in the dead mode).
        if params.mode != 0 {
            f.do_mode(anims, 0, params.mode);
        }
        let took = f.do_behavior(anims, params);
        if params.propagate {
            let held: Vec<u64> = self
                .held
                .holders()
                .into_iter()
                .filter(|(_, holder)| *holder == id)
                .map(|(child, _)| child)
                .chain(self.temporaries.held_by(id))
                .collect();
            let passed = auberean_animation::track::Params {
                behavior: params.behavior,
                time_scale: params.time_scale,
                send_event: false,
                propagate: true,
                ..auberean_animation::track::Params::default()
            };
            for child in held {
                self.play_behavior(child, &passed);
            }
        }
        took
    }

    /// Object `id`'s body goes into mode `mode` by its figure's way there; taken, what it holds
    /// goes there too. Whether the figure took it.
    // D2-06203
    pub(crate) fn do_mode(&mut self, id: u64, mode: u32) -> bool {
        let anims = &self.anims;
        let Some(f) = figure_in(
            &mut self.player,
            &mut self.others,
            &mut self.held,
            &mut self.temporaries,
            id,
        ) else {
            return false;
        };
        if !f.do_mode(anims, 0, mode) {
            return false;
        }
        let held: Vec<u64> = self
            .held
            .holders()
            .into_iter()
            .filter(|(_, holder)| *holder == id)
            .map(|(child, _)| child)
            .chain(self.temporaries.held_by(id))
            .collect();
        for child in held {
            if let Some(f) = figure_in(
                &mut self.player,
                &mut self.others,
                &mut self.held,
                &mut self.temporaries,
                child,
            ) {
                f.do_mode(anims, 0, mode);
            }
        }
        true
    }

    /// Plays behavior `behavior` on the player's figure, as the game asks the player's object;
    /// whether it took the request.
    pub fn player_do_behavior(&mut self, behavior: u32) -> bool {
        let anims = &self.anims;
        self.player.as_mut().is_some_and(|p| {
            p.figure.do_behavior(
                anims,
                &auberean_animation::track::Params::behavior(behavior),
            )
        })
    }

    /// The interior cells about the player loaded, and the near window of landblocks following the
    /// cell the player's body stands in: inside, the land cell under it when its cell sees the
    /// outdoors; none without a player.
    // D2-10803
    fn follow_player_cell(&mut self) {
        // The interior cells of the player's landblock and those beside it, loaded once each.
        if let Some(p) = &self.player {
            let block = p.body.position.cell.landblock();
            for dx in -1i32..=1 {
                for dy in -1i32..=1 {
                    let (Ok(x), Ok(y)) = (
                        u8::try_from(i32::from(block.x()) + dx),
                        u8::try_from(i32::from(block.y()) + dy),
                    ) else {
                        continue;
                    };
                    if self.blocks.insert((x, y)) {
                        auberean_physics::interior::add_block_cells(
                            &self.land,
                            &self.world.records,
                            &mut self.trees,
                            x,
                            y,
                        );
                    }
                }
            }
        }
        let (cell, outside) = match &self.player {
            Some(p) => {
                let pos = p.body.position;
                let inside = pos.cell.0 & 0xFFFF >= 0x100;
                let outside = inside
                    .then(|| self.land.cell(pos.cell))
                    .flatten()
                    .filter(|g| g.seen_outside)
                    .and_then(|_| {
                        let (mut c, mut o) = (pos.cell, pos.frame.origin);
                        auberean_physics::cells::adjust_to_outside(&mut c, &mut o).then_some(c.0)
                    });
                (pos.cell.0, outside)
            }
            None => (0, None),
        };
        let world = self.world;
        let mut m = near::Makers {
            land: &self.land,
            records: &world.records,
            region: &world.region,
            trees: &mut self.trees,
            interiors: &mut self.blocks,
        };
        self.near.update_visible_cells(&mut m, cell, outside);
    }

    /// The world without a player: the camera forgets where it stood and whom it followed.
    // D2-10660, D2-10765, D2-10807
    fn reset_world_state(&mut self) {
        self.camera.reset_camera();
        // The world's lighting is worked out afresh in the next world entered.
        self.light_at = f64::NEG_INFINITY;
        self.built = None;
        // The land let go of; the world entered next starts with fresh free blocks.
        self.near.reset_world_state(&self.land);
        self.near.notify_change_region();
        self.stand_ins = [false; 2];
    }

    /// The camera's stand-ins hung from the player, made with it when its setup has their holding
    /// locations (one the camera hangs from, one it looks at).
    // D2-10680
    fn setup_camera_stand_ins(&mut self, figure: &Figure) {
        self.stand_ins = followed::STAND_IN_LOCATIONS.map(|l| figure.has_holding_location(l));
    }

    /// The player's object made: its body, its figure, and the camera set to follow it.
    // D2-10663, D2-10661
    fn make_player(&mut self, o: &Object, client: &Client) -> Option<Player> {
        let mut dress = None;
        let look = figures::look_after(&self.world.records, &self.anims, &o.visual, &mut dress)?;
        let figure = Figure::from_look(&self.anims, &look)?;
        let scale = look.scale;
        let (config, height) = body_config(&self.world.records, &look, &figure, &o.physics)?;
        let b = &client.body;
        let start = Position::new(
            CellId(b.cell),
            PFrame::new(pv(b.origin), dereth_primitives::Quat::IDENTITY),
        );
        let mut body = Body::new(config, start, b.heading);
        give_parts(&mut body, &self.world.records, &mut self.trees, &look);
        profile::init_physics(&self.world.records, o, &mut body);
        body.minterp.scale = pv(scale);
        let p = &o.physics;
        if let Some(k) = velocity_scale(p) {
            body.minterp.velocity_scale = k;
        }
        if p.bits & auberean_client_sdk::protocol::types::PhysicsDesc::JUMP_SCALE != 0
            && p.jump_scale > 0.0
        {
            body.jump_scale = p.jump_scale;
        }
        // The player's body in the world's table, under its own handle.
        if let Some(k) = self.table.find(o.id) {
            self.table.remove(k);
        }
        let key = self.table.insert_body(&self.land, o.id, &mut body);
        self.table
            .set_ethereality(key, others::ethereality(&o.weenie));
        others::apply_description(
            &self.world.records,
            &mut self.table,
            key,
            others::physics_description(o),
        );
        tracing::info!(
            id = format_args!("{:#x}", o.id),
            visual = format_args!("{:#x}", o.visual.parent),
            cell = format_args!("{:#010x}", b.cell),
            "the player's body"
        );
        let radius = figure.parts.first().map_or(0.0, |p| p.setup.radius);
        let effects = fx::BodyFx::of(&look, o.id);
        // The camera follows the new player: its look-at point at three quarters of its height,
        // the camera behind it.
        self.camera
            .set_player(&camera::NoObjects, o.id, height * scale.z);
        self.setup_camera_stand_ins(&figure);
        Some(Player {
            id: o.id,
            visual: o.visual.clone(),
            dress,
            body,
            figure,
            placements: b.placements(),
            height,
            radius,
            scale,
            settle: true,
            shown: BTreeSet::new(),
            effects,
        })
    }

    /// The player's object was sent a new description: its own descriptions take it (the
    /// appearance its effects lay kept over it), and its figure (keeping what it plays), effects
    /// and body's parts are brought up to date with the total; its body stays where it is.
    // D2-08142
    fn player_takes_visual(&mut self, o: &Object) {
        let Some(p) = self.player.as_mut() else {
            return;
        };
        p.visual = o.visual.clone();
        let records = &self.world.records;
        let Some(look) = figures::look_after(records, &self.anims, &o.visual, &mut p.dress) else {
            return;
        };
        if !p.figure.compare_and_modify(&self.anims, &look) {
            return;
        }
        p.effects = fx::BodyFx::again(p.effects.take(), &look, p.id);
        p.scale = look.scale;
        body_takes_look(&mut p.body, records, &mut self.trees, &look);
        p.body.minterp.scale = pv(look.scale);
    }

    /// One frame at `now` seconds, `dt` after the last, in the client's order: the player's intent
    /// (the camera's controls, the body steered); every body's physics ticks, the player's and then
    /// everyone else's, on one schedule; then the camera, placed when the frame is made; then what
    /// the figures asked beyond their animation (the camera's requests) and the camera's part of
    /// the effects played. (The camera's blur is not drawn.)
    // D2-10668
    pub fn update(&mut self, client: &mut Client, intent: &Intent, now: f64, dt: f32) {
        let facing = self.player.as_ref().map_or(0.0, |p| p.body.heading);
        let mut asked_of: Vec<(u64, auberean_animation::host::Event)> = Vec::new();
        // What each figure finished this frame and the hooks it fired, for its body's story.
        let mut heard: Vec<(u64, Vec<auberean_animation::host::Completion>, Vec<u32>)> = Vec::new();
        for p in &intent.presses {
            self.camera.press_facing(*p, facing);
        }
        self.camera.hold(intent.held);
        let steer = self.camera.mouse(intent.mouse.0, intent.mouse.1);

        // The player.
        let wanted = client
            .model
            .player_object()
            .filter(|_| client.body.placed())
            .cloned();
        match (&wanted, &self.player) {
            (None, _) => {
                if let Some(k) = self.player.as_ref().and_then(|p| self.table.find(p.id)) {
                    self.table.remove(k);
                }
                if self.player.take().is_some() {
                    self.reset_world_state();
                }
            }
            (Some(o), Some(p)) if p.id == o.id => {
                if p.visual != o.visual {
                    self.player_takes_visual(o);
                }
            }
            (Some(o), _) => {
                self.player = self.make_player(o, client);
                if self.player.is_some() {
                    self.follow_player_cell();
                }
            }
        }
        if let Some(p) = &mut self.player {
            if client.body.placements() != p.placements {
                p.placements = client.body.placements();
                let b = &client.body;
                p.body.place(
                    Position::new(
                        CellId(b.cell),
                        PFrame::new(
                            pv(b.origin),
                            auberean_physics::body::heading_rotation(b.heading),
                        ),
                    ),
                    b.heading,
                );
                p.body.velocity = dereth_primitives::Vec3::ZERO;
                p.body.contact = true;
                p.body.on_walkable = true;
                p.body.contact_plane = None;
                p.settle = true;
                tracing::info!(
                    cell = format_args!("{:#010x}", b.cell),
                    "the server placed the player"
                );
            }
        }
        let block = self
            .player
            .as_ref()
            .map(|p| p.body.position.cell.landblock());
        if block.is_some() {
            self.follow_player_cell();
        }
        // A place under the ground outdoors is lifted onto it, as a placement is; then the body
        // stands where there is room for it nearest that place, as the client puts a body the
        // server placed (beside the start hall's fir it rises 0.69 m and steps 0.45 m aside).
        if let Some(p) = &mut self.player {
            if std::mem::take(&mut p.settle) {
                if !is_interior(p.body.position.cell.0) {
                    let at = p.body.position.frame.origin;
                    if let Some((z, _)) = self.land.ground(p.body.position.cell.landblock(), at) {
                        if z > at.z {
                            let mut lifted = p.body.position;
                            lifted.frame.origin.z = z;
                            p.body.place(lifted, p.body.heading);
                        }
                    }
                }
                let at = p.body.position;
                let _ = p
                    .body
                    .set_position_simple(&self.land, &mut self.table, at, true);
            }
        }
        // What the server asked of the bodies since the last frame; a body made this frame hears
        // it once it is made.
        let events = std::mem::take(&mut client.physics);
        let waiting = self.play_server_events(client, events);
        // One schedule of physics ticks for every body.
        let ticks = self.ticker.ticks(now);
        // The others as the bodies' guidance sees them this frame.
        self.objects.clear();
        if let Some(p) = &self.player {
            self.objects.note(p.id, &p.body, true);
        }
        self.others.note_objects(&mut self.objects);
        self.start_movement_actions(client, &intent.started);
        self.keep_move_target();
        if let Some(p) = &mut self.player {
            let frozen = client.body.frozen;
            let input = if frozen {
                MotionInput::default()
            } else {
                MotionInput {
                    sideways: intent.sideways,
                    forward: intent.forward,
                    turn: intent.turn,
                }
            };
            if !frozen && steer.turn != 0.0 {
                p.body.heading = (p.body.heading - steer.turn).rem_euclid(360.0);
            }
            p.body.steer(input);
            // The jump asked: standing, or moving slowly, the figure plays the jump's lead-in,
            // whose animation sets the jump off; one it cannot play, or moving faster, jumps at
            // once.
            let jumps = p.body.jumps;
            if intent.jump
                && !frozen
                && p.body.jump_logic()
                && p.body.behaviors.contains(&JUMP_BEHAVIOR)
            {
                if p.figure.do_autonomous_behavior(&self.anims, JUMP_BEHAVIOR) {
                    p.shown.insert(JUMP_BEHAVIOR);
                } else {
                    p.body.behaviors.remove(&JUMP_BEHAVIOR);
                    p.body.jump();
                }
            }
            if p.body.jumps != jumps {
                let v = p.body.velocity;
                client.act(Action::Jump {
                    velocity: Vec3::new(v.x, v.y, v.z),
                });
            }
            #[allow(clippy::cast_possible_truncation)]
            let tick = TICK as f32;
            self.table.set_now(client.game_time());
            // Each tick the body moves while its movement runs, then its other updates run while
            // woken: its detections checked, what its collisions began and ended reported.
            for _ in 0..ticks {
                p.body.update_in(
                    &self.land,
                    &mut self.table,
                    &mut self.objects,
                    &NoGameObjects,
                    tick,
                );
            }
            // The player's collisions go to its scripts.
            if let Some(k) = p.body.key() {
                for report in self.table.take_reports(k) {
                    use auberean_physics::pack::Pack;
                    let mut w = auberean_physics::pack::Writer::new();
                    report.pack(&mut w);
                    let r = client.handle_collision(p.id, w.finish());
                    tracing::debug!(?report, ?r, "the player's collisions to its scripts");
                }
            }
            let active = input.forward != 0.0 || input.sideways != 0.0 || input.turn != 0.0;
            client.body.driven = true;
            client.act(Action::Move(Motion {
                forward: input.forward,
                sideways: input.sideways,
                heading: p.body.heading,
                active,
            }));
            let pos = p.body.position;
            client
                .body
                .follow(pos.cell.0, dv(pos.frame.origin), p.body.heading);
            p.figure
                .set_motion(dv(p.body.local_velocity()), p.body.turn_rate);
            forward_behaviors(&p.body, &mut p.shown, &mut p.figure, &self.anims);
            p.figure.ground = ground_of(&p.body);
            p.figure.advance(&self.anims, dt);
            // What the figure finished and the hooks it fired go to its story; what it finished
            // goes nowhere else (the client's scripts make no waiting calls).
            let (hooks, impulses) = take_triggers(&mut p.figure);
            heard.push((p.id, p.figure.take_completions(), hooks));
            asked_of.extend(p.figure.take_events().into_iter().map(|e| (p.id, e)));
            // The jump the lead-in's animation set off; the lead-in over, the body no longer
            // shows it.
            let jumps = p.body.jumps;
            for i in impulses {
                if i.jump {
                    p.body.jump();
                }
            }
            if p.body.behaviors.contains(&JUMP_BEHAVIOR)
                && !p.figure.is_doing_behavior(JUMP_BEHAVIOR)
            {
                p.body.behaviors.remove(&JUMP_BEHAVIOR);
                p.shown.remove(&JUMP_BEHAVIOR);
            }
            if p.body.jumps != jumps {
                let v = p.body.velocity;
                client.act(Action::Jump {
                    velocity: Vec3::new(v.x, v.y, v.z),
                });
            }
            let v = p.body.velocity;
            self.status.cell = pos.cell.0;
            self.status.origin = dv(pos.frame.origin);
            self.status.heading = p.body.heading;
            self.status.speed = (v.x * v.x + v.y * v.y).sqrt();
            self.status.contact = p.body.contact;
            // The camera follows: its rates, the slope, the shoulder, its distance and glide.
            let subject = subject_of(p, pos, p.body.heading);
            let alpha = self.ticker.alpha(now);
            let objects = followed_of(Some(&*p), &self.others, &self.anims, alpha, self.stand_ins);
            self.camera.update_with(dt, &subject, &objects);
            self.fov = self.camera.fov;
        }
        // A player that walked into another cell has its surroundings follow it.
        if self.player.is_some() {
            self.follow_player_cell();
        }

        self.follow_portal_space(client, now);

        // Everything else.
        let player = self.player.as_ref().map(|p| (p.id, p.body.position));
        let game_now = client.game_time();
        self.others.update(
            &self.world.records,
            &self.anims,
            &mut client.model,
            &self.land,
            &mut self.table,
            &mut self.trees,
            player,
            &mut self.objects,
            ticks,
            game_now,
            dt,
        );
        heard.extend(self.others.take_heard());
        self.update_held(client, dt);
        let leftover = self.play_server_events(client, waiting);
        self.combat.without_body += leftover
            .iter()
            .filter(|m| matches!(m, auberean_client_sdk::protocol::ServerMessage::DoStory(_)))
            .count() as u64;
        if !leftover.is_empty() {
            tracing::debug!(
                events = leftover.len(),
                "the server's events for bodies the scene does not have"
            );
        }
        self.apply_body_states(client);
        self.update_temporaries(client, ticks, dt, &mut heard);
        self.publish_readings(client);
        // What each figure finished and the hooks it fired, to its body's story.
        for (id, done, hooks) in heard {
            self.story_heard(client, id, &done, &hooks);
        }
        self.book_world_changes(client);
        self.tell_lives(client);
        self.route_notices(client.local_time());
        // The behaviors the scripts asked objects to play or stop, in order; what the figures
        // finish goes nowhere script-side (the client's scripts make no waiting calls).
        let asked = client
            .scripts
            .as_mut()
            .map(|s| std::mem::take(&mut s.host.behaviors))
            .unwrap_or_default();
        for b in asked {
            use auberean_client_sdk::runtime::host::Behavior;
            let (object, play) = match &b {
                Behavior::Play { object, params } => (*object, Some(params_of(params))),
                Behavior::Stop { object, .. } => (*object, None),
            };
            match (play, &b) {
                (Some(params), _) => {
                    self.play_behavior(object, &params);
                }
                (None, Behavior::Stop { behavior, .. }) => {
                    let anims = &self.anims;
                    if let Some(f) = figure_in(
                        &mut self.player,
                        &mut self.others,
                        &mut self.held,
                        &mut self.temporaries,
                        object,
                    ) {
                        f.stop_behavior(anims, *behavior);
                    }
                }
                (None, Behavior::Play { .. }) => {}
            }
        }
        // What the figures asked beyond their animation, and the effects the scripts played.
        asked_of.extend(self.others.take_figure_events());
        asked_of.extend(self.held.take_figure_events());
        let mut played = std::mem::take(&mut self.asked_fx);
        let asked = played.len();
        played.extend(
            client
                .scripts
                .as_mut()
                .map(|s| std::mem::take(&mut s.host.effects))
                .unwrap_or_default()
                .into_iter()
                .map(|e| (e.object, e.effect, e.intensity)),
        );
        self.route_figure_events(asked_of, &mut played, now);
        // The scripts' effects and the figures' play on the bodies too (those asked through the
        // scene already were).
        for &(id, effect, intensity) in &played[asked..] {
            self.body_fx.push((id, effect, Some(intensity)));
        }
        self.play_camera_effects(client, &played, now);
        self.ask_body_fx();
        self.status.others = self.others.len();
        self.status.nearest = self.others.nearest(
            figures::global(self.status.cell, self.status.origin),
            4,
            self.ticker.alpha(now),
        );
    }

    /// What the objects' scripts set on their bodies (as they do when an object is made or one
    /// of its qualities changes), applied to the bodies: what changed since it was last applied,
    /// or all of it for a body made again or entered in the world's table under another handle.
    /// The collision state goes to the body's entry in the table (the parts it marks set); drawn
    /// or not, its own scale (its entry sized by its whole scale) and whether it slides along
    /// edges go to the body itself, each once it was set.
    /// What each body says of itself when the scripts ask, read for them after the frame's ticks:
    /// the element it is in and the motion it was last asked for.
    fn publish_readings(&self, client: &mut Client) {
        use auberean_client_sdk::runtime::body::BodyReading;
        let Some(scripts) = client.scripts.as_mut() else {
            return;
        };
        let readings = &mut scripts.host.readings;
        readings.clear();
        let mut note = |id: u64, b: &Body| {
            let m = b.minterp.asked;
            readings.insert(
                id,
                BodyReading {
                    element: b.element(),
                    last_motion: [m.x, m.y, m.z],
                },
            );
        };
        if let Some(p) = &self.player {
            note(p.id, &p.body);
        }
        for (id, b) in self.others.bodies() {
            note(id, b);
        }
        for id in self.held.ids() {
            if let Some(b) = self.held.body(id) {
                note(id, b);
            }
        }
        for (id, _) in self.temporaries.ids() {
            if let Some(t) = self.temporaries.get(id) {
                note(id, &t.body);
            }
        }
    }

    fn apply_body_states(&mut self, client: &Client) {
        let Some(scripts) = client.scripts.as_ref() else {
            return;
        };
        for (id, state) in &scripts.host.bodies {
            if !self.has_body(*id) {
                continue;
            }
            let key = self.table.find(*id);
            let last = self.states.get(id).filter(|(k, _)| *k == key);
            if last.is_some_and(|(_, s)| s == state) {
                continue;
            }
            let before = last.map(|(_, s)| s.clone()).unwrap_or_default();
            if let Some(k) = key {
                if last.is_none() || before.collision != state.collision {
                    self.table.set_collision_state(k, &state.collision);
                }
            }
            if let Some(b) = self.body_mut(*id) {
                if state.no_draw != before.no_draw {
                    b.set_no_draw(state.no_draw);
                }
                if let Some(on) = state
                    .edge_slide
                    .filter(|_| state.edge_slide != before.edge_slide)
                {
                    b.set_edge_slide(on);
                }
                if let Some(on) = state
                    .undetectable
                    .filter(|_| state.undetectable != before.undetectable)
                {
                    b.set_undetectable(on);
                }
                if let Some(s) = state
                    .jump_scale
                    .filter(|_| state.jump_scale != before.jump_scale)
                {
                    b.set_jump_scale(s);
                }
                if let Some(s) = state.scale.filter(|_| state.scale != before.scale) {
                    b.set_scale(s);
                    let whole = b.total_scale(None);
                    if let Some(k) = key {
                        self.table.set_scale(k, whole);
                    }
                }
            }
            self.states.insert(*id, (key, state.clone()));
            tracing::debug!(id = format_args!("{id:#x}"), "a body's state applied");
        }
    }

    /// The physics body of object `id`: the player's, another's, a held object's or one the client
    /// made itself.
    fn body_mut(&mut self, id: u64) -> Option<&mut Body> {
        match &mut self.player {
            Some(p) if p.id == id => Some(&mut p.body),
            _ => {
                if self.others.body(id).is_some() {
                    self.others.body_mut(id)
                } else if self.held.body(id).is_some() {
                    self.held.body_mut(id)
                } else {
                    self.temporaries.body_mut(id)
                }
            }
        }
    }

    /// The objects' scripts are told their objects entered the world when the bodies did, and
    /// left it when they left (a placement that failed, a cell no longer loaded, a body held,
    /// contained or gone), rather than once an object has a place: the player's body, every other
    /// body, and an object with no body while its place's cell is loaded.
    fn tell_lives(&mut self, client: &mut Client) {
        client.bodies_place = true;
        let ids: Vec<u64> = client.model.objects().map(|o| o.id).collect();
        for id in ids {
            client.set_in_world(id, self.stands(id));
        }
    }

    /// Whether object `id` stands in the world here: the player's body or another's in the
    /// world, or an object with no body whose place's cell is loaded.
    #[must_use]
    pub fn stands(&self, id: u64) -> bool {
        match &self.player {
            Some(p) if p.id == id => p.body.in_world,
            _ => self.others.stands(id),
        }
    }

    /// The movement actions the player began this frame: each interrupts the move the player's
    /// body is making on its own (walking up to what it attacks or uses), and a turn also stops
    /// an attack the scripts started while it moved so.
    // D2-09645, D2-09646
    fn start_movement_actions(&mut self, client: &mut Client, started: &[u32]) {
        for action in started {
            if let Some(p) = &mut self.player {
                p.body.interrupt_move_to();
            }
            if matches!(*action, movement::TURN_LEFT | movement::TURN_RIGHT) {
                client.stop_attacking_if_auto_moving();
            }
        }
    }

    /// The object table's book of the bodies that left the world or entered it again: one that
    /// left (not the player) is to be destroyed after a while, with what it holds, unless it
    /// enters the world again first, which calls that off (it is already out of its lost cell).
    // D2-05636, D2-05635
    fn book_world_changes(&mut self, client: &mut Client) {
        let player = self.player.as_ref().map(|p| p.id);
        let now = client.local_time();
        for (id, entered) in self.others.take_world_changes() {
            if !entered && Some(id) == player {
                continue;
            }
            let mut ids = vec![id];
            ids.extend(
                client
                    .model
                    .objects()
                    .filter(|o| o.parent.is_some_and(|(p, _)| p == id))
                    .map(|o| o.id),
            );
            for i in ids {
                if entered {
                    client.queue.remove_object_to_be_destroyed(i);
                } else {
                    client.queue.add_object_to_be_destroyed(i, now);
                }
            }
        }
    }

    /// What the figures asked beyond their animation: an effect to play (kept in `played`), and
    /// for the player's, the camera to ride the stand-ins its animations move, to come back to the
    /// player, or to turn to an object.
    fn route_figure_events(
        &mut self,
        events: Vec<(u64, auberean_animation::host::Event)>,
        played: &mut Vec<(u64, u32, f32)>,
        now: f64,
    ) {
        use auberean_animation::host::Event;
        let alpha = self.ticker.alpha(now);
        for (id, e) in events {
            let objects = followed_of(
                self.player.as_ref(),
                &self.others,
                &self.anims,
                alpha,
                self.stand_ins,
            );
            match e {
                Event::DoFx { fx, intensity } => {
                    self.status.figure_fx.0 += 1;
                    played.push((id, fx, intensity));
                }
                Event::StopFx { fx } => {
                    self.status.figure_fx.1 += 1;
                    self.body_fx.push((id, fx, None));
                }
                Event::CameraAnimParent { time, ramp } => {
                    self.camera.set_camera_anim_parent(&objects, time, ramp);
                }
                Event::CameraAnimTarget { time, ramp } => {
                    self.camera.set_camera_anim_target(&objects, time, ramp);
                }
                Event::CameraBack => self.camera.restore_player_camera(&objects),
                Event::TargetedCamera {
                    target,
                    words,
                    time,
                    ramp,
                } => {
                    self.camera
                        .set_targeted_camera(&objects, target, words, time, ramp);
                }
                other => tracing::trace!(
                    object = format_args!("{id:#x}"),
                    asked = ?other,
                    "a figure's request the scene does not take"
                ),
            }
        }
    }

    /// The camera's part of each effect played on an object (`object`, its effect id, its
    /// strength), as the object's effects play it: every entry its effect table lists for the id
    /// (in its default variant) that is for this player if only for the player, and for the time
    /// of day, names a camera effect record, played from the object at the strength (none when the
    /// entry ramps in).
    // D2-06884
    fn play_camera_effects(&mut self, client: &Client, played: &[(u64, u32, f32)], now: f64) {
        if played.is_empty() {
            return;
        }
        let records = &self.world.records;
        let night = self.is_night();
        let alpha = self.ticker.alpha(now);
        let player = self.player.as_ref().map(|p| p.id);
        for &(object, effect, intensity) in played {
            let Some(o) = client.model.object(object) else {
                continue;
            };
            let Some(look) = figures::look_of(records, &self.anims, &o.visual) else {
                continue;
            };
            let table = auberean_draw::effects::ObjectFxTable::of_group_with(records, &look.global);
            for data in table.fx_data(records, effect) {
                let node = auberean_draw::effects::node_for(&data, 0);
                if node.player_only == Some(true) && player != Some(object) {
                    continue;
                }
                let day_ok = node.day_time.unwrap_or(true) || night;
                let night_ok = node.night_time.unwrap_or(true) || !night;
                if !(day_ok && night_ok) {
                    continue;
                }
                let Some(id) = node.camera_fx.filter(|d| !d.is_none()) else {
                    continue;
                };
                let Some(fx) = records.get::<auberean_assets::fx::CameraFx>(id.0) else {
                    continue;
                };
                let scalar = if node.ramp_time.unwrap_or(0.0) >= 0.0002 {
                    0.0
                } else {
                    intensity
                };
                let objects = followed_of(
                    self.player.as_ref(),
                    &self.others,
                    &self.anims,
                    alpha,
                    self.stand_ins,
                );
                self.camera.do_camera_fx(&objects, &fx, scalar, object);
            }
        }
    }

    /// Whether the game's time of day now is a night one: the part of the day the time falls in
    /// (the last to begin at or before it).
    fn is_night(&self) -> bool {
        let Some(cal) = self.world.region.calendar.as_ref() else {
            return false;
        };
        if cal.day_parts.is_empty() {
            return false;
        }
        let f = auberean_world::sky::day_clock(cal, self.game_time).fraction;
        let mut i = 0;
        while i + 1 < cal.day_parts.len() && f >= cal.day_parts[i + 1].begin {
            i += 1;
        }
        cal.day_parts[i].night != 0
    }

    /// What the bodies asked of each other, of their figures and of their game objects over the
    /// frame's ticks: a body asked to be watched or to stop, or a watcher told where its target
    /// is, hears it at once (what that makes it ask in turn is handed on in the same frame, a few
    /// rounds at most); a request for a body's animation goes to its figure; the rest goes to
    /// the game objects, which are the server's here, and is only noted.
    fn route_notices(&mut self, now: f64) {
        // What the cells' watchers were told of objects entering and leaving them.
        for (watcher, info) in self.table.take_detection_updates() {
            let body = match self.player.as_mut() {
                Some(p) if p.id == watcher => Some(&mut p.body),
                _ => match self.others.body_mut(watcher) {
                    Some(b) => Some(b),
                    None => match self.held.body_mut(watcher) {
                        Some(b) => Some(b),
                        None => self.temporaries.body_mut(watcher),
                    },
                },
            };
            if let Some(b) = body {
                b.receive_detection_update(&info);
            }
        }
        for _ in 0..notices::ROUNDS {
            let mut asked: Vec<(u64, Notice)> = Vec::new();
            if let Some(p) = &mut self.player {
                asked.extend(p.body.take_requests().into_iter().map(|n| (p.id, n)));
            }
            asked.extend(self.others.take_notices());
            asked.extend(self.held.take_notices());
            asked.extend(self.temporaries.take_notices());
            if asked.is_empty() {
                break;
            }
            for (from, n) in asked {
                if let Some(to) = n.addressee() {
                    let body = match self.player.as_mut() {
                        Some(p) if p.id == to => Some(&mut p.body),
                        _ => match self.others.body_mut(to) {
                            Some(b) => Some(b),
                            None => match self.held.body_mut(to) {
                                Some(b) => Some(b),
                                None => self.temporaries.body_mut(to),
                            },
                        },
                    };
                    if let Some(b) = body {
                        b.deliver(&n);
                    }
                    continue;
                }
                let Some(n) = self.temporary_notice_or(from, n, now) else {
                    continue;
                };
                let figure = match self.player.as_mut() {
                    Some(p) if p.id == from => Some(&mut p.figure),
                    _ => match self.others.figure_mut(from) {
                        Some(f) => Some(f),
                        None => self.held.figure_mut(from),
                    },
                };
                notices::hand_on(from, n, figure, &self.anims);
            }
        }
    }

    /// What the bodies hold, as the server says: each object held is taken up by its holder's
    /// body at its holding location (taken up again when its holder or location changes, or the
    /// holder was put together again), let go of once the server no longer calls it held, and
    /// fades as its holder's element changes; their figures are animated over `dt` seconds.
    fn update_held(&mut self, client: &Client, dt: f32) {
        use auberean_client_sdk::model::ids::is_temporary;
        use auberean_physics::body::parent::{set_parent, TakeUp, FADE_SLIDER, FADE_TIME};
        let wanted: Vec<(u64, u64, u32, u32)> = client
            .model
            .objects()
            .filter_map(|o| o.parent.map(|(p, l)| (o.id, p, l, o.parent_placement)))
            .collect();
        for id in self.held.ids() {
            if wanted.iter().any(|w| w.0 == id) {
                continue;
            }
            for n in self.held.let_go(id) {
                let Some(to) = n.addressee() else { continue };
                let body = match self.player.as_mut() {
                    Some(p) if p.id == to => Some(&mut p.body),
                    _ => self.others.body_mut(to),
                };
                if let Some(b) = body {
                    b.deliver(&n);
                }
            }
        }
        for (id, holder, location, placement) in wanted {
            let Some(o) = client.model.object(id) else {
                continue;
            };
            let (holder_body, holder_figure) = match self.player.as_mut() {
                Some(p) if p.id == holder => (&mut p.body, &p.figure),
                _ => match self.others.holder_mut(holder) {
                    Some(h) => h,
                    None => continue,
                },
            };
            let at = holder_body.position;
            let h = self.held.ensure(&self.world.records, &self.anims, o, at);
            if h.holder == Some((holder, location)) && holder_body.look_up_in_children(id) {
                continue;
            }
            let how = TakeUp {
                location,
                holds: auberean_physics::body::parent::holds_at(
                    location,
                    holder_figure.has_holding_location(location),
                ),
                parent_mode: holder_figure.mode,
                child_is_temporary: is_temporary(id),
            };
            h.holder = set_parent(
                &mut h.body,
                holder_body,
                how,
                &self.land,
                &mut self.table,
                &mut self.objects,
            )
            .then_some((holder, location));
            // Taken up, it hangs in the pose the server said.
            if h.holder.is_some() {
                h.body.set_placement_frame(placement);
            }
            tracing::debug!(
                id = format_args!("{id:#x}"),
                holder = format_args!("{holder:#x}"),
                location = format_args!("{location:#x}"),
                taken = h.holder.is_some(),
                "a held object taken up"
            );
        }
        // A holder whose element changed fades what it holds; then the change is no longer news.
        let holders = self.held.holders();
        for (id, holder) in &holders {
            let holder_body = match self.player.as_mut() {
                Some(p) if p.id == *holder => Some(&mut p.body),
                _ => self.others.body_mut(*holder),
            };
            let Some(hb) = holder_body else { continue };
            let info = hb
                .children
                .as_ref()
                .and_then(|c| c.children.iter().find(|c| c.id == *id).copied());
            let fade = info.and_then(|i| hb.child_fade(&i, is_temporary(*id)));
            if let (Some(value), Some(f)) = (fade, self.held.figure_mut(*id)) {
                f.sliders.do_slider(0, FADE_SLIDER, value, FADE_TIME);
            }
        }
        for (_, holder) in holders {
            let holder_body = match self.player.as_mut() {
                Some(p) if p.id == holder => Some(&mut p.body),
                _ => self.others.body_mut(holder),
            };
            if let Some(b) = holder_body {
                b.children_drawn();
            }
        }
        self.held.advance(&self.anims, dt);
    }

    /// The frames of the interface's viewport scenes, their figures animated over `dt` seconds;
    /// the picks asked are answered in `views.picked`.
    pub fn view_frames(&mut self, scenes: &[view::ViewScene], dt: f32) -> Vec<view::ViewFrame> {
        self.views.frames(
            &self.world.records,
            &self.anims,
            &mut self.builder,
            &mut self.drawer,
            scenes,
            dt,
        )
    }

    /// The frame to draw: the world around the camera and the bodies in it. `None` before the
    /// player has a body.
    pub fn frame(&mut self, now: f64) -> Option<&Frame> {
        if self.build_world(now, 1.0) {
            self.built.as_ref().map(|b| &b.frame)
        } else {
            None
        }
    }

    /// Make the world's frame at `now`, its field of view times `fov_scale`; whether there is one.
    fn build_world(&mut self, now: f64, fov_scale: f32) -> bool {
        if self.player.is_none() {
            return false;
        }
        let lightings = self.light_tick();
        let Some(p) = self.player.as_ref() else {
            return false;
        };
        // The body and the camera are drawn between the body's last two ticks.
        let (pos, body_heading) = p.body.drawn(self.ticker.alpha(now));
        let cell = pos.cell.0;
        let block = pos.cell.landblock();
        let origin = pos.frame.origin;
        // The camera's wanted place for the body as drawn, kept off the world: walls, ground and
        // solid objects (other bodies, things the server made, the scenery), not in first person.
        let subject = subject_of(p, pos, body_heading);
        // In first person, while a behavior that moves the first-person camera plays and the
        // avatar option shows the player, the camera rides the head bone instead: its whole frame,
        // a fifth of a meter ahead of it and up.
        let mode = p.figure.modes.desired_mode;
        let head = if self
            .camera
            .rides_the_head(mode, p.figure.modes.dynamic_fp_camera())
        {
            let [x, y, z] = camera::HEAD_OFFSET;
            held::bone_position(
                &p.figure,
                &self.anims,
                &pos,
                p.body.total_scale(None),
                camera::HEAD_BONE,
                &Mat4::translation(Vec3::new(x, y, z)),
            )
        } else {
            None
        };
        let viewer = match head {
            Some(at) => {
                let m = auberean_physics::geom::rotation_axes(at.frame.rotation);
                camera::Viewer {
                    origin: at.frame.origin,
                    look: dereth_primitives::Vec3::new(m[3], m[4], m[5]),
                }
            }
            None => {
                let objects = followed_of(
                    Some(p),
                    &self.others,
                    &self.anims,
                    self.ticker.alpha(now),
                    self.stand_ins,
                );
                self.camera.viewer_with(&subject, &objects)
            }
        };
        let eye = if self.camera.no_ray_trace() {
            viewer.origin
        } else {
            let target_offset = self.camera.target_offset(&subject);
            let lines = auberean_physics::camera::Lines {
                land: &self.land,
                cell: pos.cell,
                origin,
                target_offset,
                wanted: viewer.origin,
                env_only: self.camera.check_env_only(),
            };
            let table = &self.table;
            let skip = table.find(p.id);
            let camera_ethereal = table.camera_ethereal();
            let solid = auberean_physics::ethereal::ObjectEthereality::default();
            let objects = |from, to| {
                table.trace_ray_objects(pos.cell, from, to, skip, &camera_ethereal, &solid)
            };
            auberean_physics::camera::keep_off_the_world(&lines, &objects)
                .or(self.camera.last.map(|l| l.origin))
                .unwrap_or(viewer.origin)
        };
        // The shakes under way move the camera from there, walls or not.
        // D2-10673
        let eye = {
            let here = |p: &auberean_physics::body::Body| {
                let at = p.position;
                let g = figures::global(at.cell.0, dv(at.frame.origin));
                dereth_primitives::Vec3::new(g.x, g.y, g.z)
            };
            let me = here(&p.body);
            // The first shake draws its three starts along the noise from the client's numbers.
            if self.camera.effects.shake != 0.0 && !self.shake_drawn {
                let r = &self.anims.dice;
                self.camera.effects.shake_seeds = [
                    r.roll_float(0.0, 10.0),
                    r.roll_float(0.0, 10.0),
                    r.roll_float(0.0, 10.0),
                ];
                self.shake_drawn = true;
            }
            let shake = self.camera.effects.distortion(Some(me), |id| {
                if id == p.id {
                    Some(me)
                } else {
                    self.others.body(id).map(here)
                }
            });
            dereth_primitives::Vec3::new(eye.x + shake.x, eye.y + shake.y, eye.z + shake.z)
        };
        // The scene camera stands there: where the next glide starts from.
        // D2-10647
        self.camera.last = Some(camera::Viewer {
            origin: eye,
            look: viewer.look,
        });
        let l = viewer.look;
        let flat = (l.x * l.x + l.y * l.y).sqrt();
        let heading = dereth_primitives::num::math::atan2f(l.x, l.y)
            .to_degrees()
            .rem_euclid(360.0);
        let pitch = dereth_primitives::num::math::atan2f(l.z, flat).to_degrees();
        let (bx, by) = (i32::from(block.x()), i32::from(block.y()));
        let (time_of_day, day) = self.moment();
        let spot = Spot {
            block: (bx, by),
            position: dv(eye),
            heading,
            pitch,
            time_of_day,
            day,
        };
        // The camera's own cell chooses what the frame draws: inside, the rooms about it and what
        // they look out on; outside, the land.
        // D2-11042
        let view = auberean_physics::camera::camera_cell(&self.land, pos.cell, eye)
            .map_or(cell, |(c, _)| c.0);
        let viewer_cell = if is_interior(view) {
            (-1, i32::try_from(view & 0xFFFF).unwrap_or(0))
        } else {
            (
                dereth_primitives::num::floor_to_i32(eye.x / CELL_LENGTH),
                dereth_primitives::num::floor_to_i32(eye.y / CELL_LENGTH),
            )
        };
        let key = (block, viewer_cell, lightings);
        let stale = self.built.as_ref().is_none_or(|b| b.key != key);
        if stale {
            let started = now;
            // A cell first made ready in this frame bakes its lights as they stand now.
            self.builder.uptime = now;
            let mut outside_day = None;
            let frame = if is_interior(view) {
                #[allow(clippy::cast_possible_truncation)]
                let low = (view & 0xFFFF) as u16;
                let home = block_of(view);
                let home = (i32::from(home.x()), i32::from(home.y()));
                // Inside, no sun: the sky lights shine with the day's sun and ambient, the
                // cells carry their light in their vertices, the rest is lit by the interior
                // ambient and the lights nearest the camera, in the interior fog. What the rooms
                // look out on (the sky, the land) keeps the light outdoors.
                let (mut f, outside) = self.builder.inside_frame(&spot, self.fov, home, low);
                outside_day = Some(outside);
                inside_light(
                    &mut f.light,
                    self.builder.block_lights(home).as_deref(),
                    dv(eye),
                    &outside,
                );
                f
            } else {
                self.builder.frame(&spot, self.fov)
            };
            let world_objects = frame.objects.len();
            self.placed_effects
                .sync(&self.systems, &self.world.records, &frame.placed, now);
            self.built = Some(Built {
                frame,
                key,
                at: started,
                world_objects,
                outside: outside_day,
            });
            tracing::debug!(
                block = format_args!("{:02X}{:02X}", bx, by),
                objects = world_objects,
                "the world's frame rebuilt"
            );
        }
        // The player's own body and what it holds are drawn unless the camera has settled at its
        // eyes and the avatar option hides it; drawn there, it leaves out its head.
        let draws_player = self.camera.draws_the_player(mode) && !p.body.is_no_draw();
        let at_the_eyes = self.camera.first_person_is_ready();
        // The objects the client made, where they stand this frame.
        let made = self.temporaries_placed(self.ticker.alpha(now), draws_player);
        let night = self.is_night();
        let Some(built) = self.built.as_mut() else {
            return false;
        };
        built.frame.objects.truncate(built.world_objects);
        built.frame.camera.position = dv(eye);
        built.frame.camera.heading = heading;
        built.frame.camera.pitch = pitch;
        built.frame.camera.fov_y = self.fov * fov_scale;
        // The client's running time: the clouds' shadows, the water's ripples and the flares'
        // fades follow it.
        built.frame.uptime = now;
        let _ = built.at;
        // Outdoors the sky stands at the camera and moves on with the moment every frame, and the
        // light and fog are the moment's at the camera; the ground keeps the lighting it was lit
        // with until the light's tick.
        // Inside, the sky and the land the rooms look out on do the same under the light outdoors.
        if !is_interior(view) || built.frame.outside.is_some() {
            let day = self.builder.day(&spot);
            let light = self.builder.light(&spot, day.as_ref());
            if is_interior(view) {
                built.frame.outside = Some(light);
            } else {
                built.frame.light = light;
            }
            let mut counts = built.frame.counts;
            counts.sky_objects = 0;
            let (before, after) =
                self.builder
                    .sky(spot.position, day.as_ref(), spot.time_of_day, &mut counts);
            built.frame.sky_before = before;
            built.frame.sky_after = after;
            built.frame.counts = counts;
        }
        let mut noted = Vec::new();
        if draws_player {
            let poses = p.figure.pose(&self.anims);
            let place = Mat4::from_parts(
                dv(origin),
                dq(pos.frame.rotation),
                dv(p.body.total_scale(None)),
            );
            let start = built.frame.objects.len();
            let hidden: &[u32] = if at_the_eyes {
                &self.first_person_hidden
            } else {
                &[]
            };
            built.frame.objects.extend(self.drawer.batches_without(
                &self.world.records,
                &p.figure,
                &poses,
                &place,
                hidden,
            ));
            noted.push((
                p.id,
                start..built.frame.objects.len(),
                dv(origin),
                body_heading,
            ));
        }
        self.others.draw_noting(
            &self.world.records,
            &self.anims,
            &mut self.drawer,
            (bx, by),
            self.ticker.alpha(now),
            &mut built.frame.objects,
            &mut noted,
        );
        // What the bodies hold, each where it hangs from its holder as drawn.
        let alpha = self.ticker.alpha(now);
        let mut placed = Vec::new();
        for (id, holder) in self.held.holders() {
            if p.id == holder && !draws_player {
                continue;
            }
            let found = if p.id == holder {
                Some((&p.body, &p.figure, p.scale))
            } else {
                self.others.holder(holder)
            };
            let Some((hb, hf, _)) = found else {
                continue;
            };
            let Some(info) = hb
                .children
                .as_ref()
                .and_then(|c| c.children.iter().find(|c| c.id == id))
            else {
                continue;
            };
            let (drawn, _) = hb.drawn(alpha);
            let m = held::hanging(hf, &self.anims, info);
            let whole = hb.total_scale(None);
            let child = self
                .held
                .body(id)
                .map_or(whole, |c| c.total_scale(Some((whole, hb.child_scale))));
            placed.push((
                id,
                Body::child_place(&drawn, whole, info, m.as_ref()),
                child,
            ));
        }
        self.held.draw(
            &self.world.records,
            &self.anims,
            &mut self.drawer,
            (bx, by),
            &placed,
            &mut built.frame.objects,
            &mut noted,
        );
        self.temporaries.draw(
            &self.world.records,
            &self.anims,
            &mut self.drawer,
            (bx, by),
            &made,
            &mut built.frame.objects,
            &mut noted,
        );
        // The lights the objects' effects carry, as they stand now (the placed objects', the
        // bodies', what they hold and what the client made), each at its place in its own cell's
        // block: they light what is near them as the lights the cells keep do. They are gathered
        // anew every frame.
        // D2-10579
        let block_len = self.world.region.land.grid().block_length();
        let mut placed_lights: Vec<(auberean_draw::effects::PlacedLight, (i32, i32))> = self
            .placed_effects
            .lights((bx, by), block_len, now)
            .into_iter()
            .map(|l| (l, (bx, by)))
            .collect();
        if let Some(fx) = p.effects.as_ref() {
            let me = figures::global(pos.cell.0, dv(origin));
            placed_lights.extend(fx.lights(me, body_heading, pos.cell.0, now));
        }
        placed_lights.extend(self.others.effect_lights((bx, by), alpha, now));
        for (id, at, _) in &placed {
            if let Some(fx) = self.held.effects_of(*id) {
                let g = figures::global(at.cell.0, dv(at.frame.origin));
                let h = auberean_physics::geom::frame_heading(&at.frame).degrees();
                placed_lights.extend(fx.lights(g, h, at.cell.0, now));
            }
        }
        for (id, at, _) in &made {
            if let Some(fx) = self.temporaries.get(*id).and_then(|t| t.effects.as_ref()) {
                let g = figures::global(at.cell.0, dv(at.frame.origin));
                let h = auberean_physics::geom::frame_heading(&at.frame).degrees();
                placed_lights.extend(fx.lights(g, h, at.cell.0, now));
            }
        }
        let carried: Vec<auberean_world::inside::Light> = placed_lights
            .iter()
            .map(|(l, home)| l.standing(*home, block_len, now, &self.anims.dice))
            .collect();
        self.carried.clone_from(&carried);
        // Inside, each body is lit by the lights it keeps where it stands and the carried lights
        // that reach it, not by the sun; one standing outdoors, seen through a doorway out, is lit
        // as the land it stands on. The lights it keeps throw its shadow; outdoors the carried
        // lights reaching it do, else the sun. The cells and what stands in them take the carried
        // lights that reach them too.
        let mut throws: HashMap<u64, auberean_primitives::Vec3> = HashMap::new();
        if let Some(outside) = built.outside {
            let world_objects = built.world_objects;
            self.builder.light_carried(
                &spot,
                &mut built.frame.objects[..world_objects],
                &carried,
                &outside,
            );
            let looks_out = built.frame.outside.is_some();
            for (id, range, at, _) in &noted {
                let Some(batches) = built.frame.objects.get_mut(range.clone()) else {
                    continue;
                };
                let cell = if *id == p.id {
                    Some(pos.cell.0)
                } else {
                    self.others.body(*id).map(|b| b.position.cell.0)
                };
                if looks_out && cell.is_some_and(|c| !is_interior(c)) {
                    for b in batches.iter_mut() {
                        b.outside = true;
                    }
                    continue;
                }
                let toward = self.builder.light_inside(
                    &spot,
                    (bx, by),
                    batches,
                    *at,
                    cell.unwrap_or(view),
                    &outside,
                    &carried,
                );
                throws.insert(*id, toward);
            }
        } else {
            // Outdoors no light is kept but those objects carry: what they reach (the objects
            // placed with the world, the bodies) is lit by the sun and by them.
            let outside = auberean_world::inside::Outside::of(&built.frame.light);
            let world_objects = built.world_objects;
            self.builder.light_carried(
                &spot,
                &mut built.frame.objects[..world_objects],
                &carried,
                &outside,
            );
            if !carried.is_empty() {
                for (id, range, at, _) in &noted {
                    let Some(batches) = built.frame.objects.get_mut(range.clone()) else {
                        continue;
                    };
                    let cell = if *id == p.id {
                        pos.cell.0
                    } else {
                        self.others.body(*id).map_or(cell, |b| b.position.cell.0)
                    };
                    if let Some(toward) = self
                        .builder
                        .light_outdoors(&spot, batches, *at, cell, &outside, &carried)
                    {
                        throws.insert(*id, toward);
                    }
                }
            }
        }
        let casting = Casting {
            records: &self.world.records,
            table: &self.table,
            temporaries: &self.temporaries,
            setting: self.object_shadows,
            model_detail: self.builder.model_detail,
        };
        cast_volumes(
            &mut built.frame,
            &noted,
            &throws,
            view,
            &casting,
            &mut self.volumes,
        );
        // The decals laid before move on; the lights the objects' effects carry lay their pools on
        // the land, and at the simple setting the bodies their discs, each drawn once.
        self.decals.think(now);
        if self.light_effects && !is_interior(view) {
            let frame_block = (bx, by);
            let builder = &self.builder;
            let material =
                auberean_draw::decal::pool_material(&self.world.records, spot.time_of_day);
            let ground = |at: auberean_primitives::Vec3| builder.ground_square(frame_block, at);
            light_pools(
                &carried,
                frame_block,
                material,
                &ground,
                now,
                &mut self.decals,
            );
        }
        if self.object_shadows == auberean_draw::volume::setting::SIMPLE {
            let player = self.player.as_ref();
            let (others, held, temporaries) = (&self.others, &self.held, &self.temporaries);
            let body_of = |id: u64| {
                player
                    .filter(|p| p.id == id)
                    .map(|p| &p.body)
                    .or_else(|| others.body(id))
                    .or_else(|| held.body(id))
                    .or_else(|| temporaries.get(id).map(|t| &t.body))
            };
            let frame_block = (bx, by);
            let builder = &self.builder;
            let ground = |at: auberean_primitives::Vec3| builder.ground_square(frame_block, at);
            disc_shadows(
                &built.frame,
                &noted,
                &casting,
                &body_of,
                &DecalGround {
                    square: &ground,
                    side: builder.square_side(),
                    inside: is_interior(view),
                },
                now,
                &mut self.decals,
            );
        }
        built.frame.decals = auberean_draw::decal::decal_batches(
            &self.world.records,
            &mut self.decals,
            &mut self.decal_layers,
        );
        // Every draw's material as it stands at this moment of the client's run.
        built.frame.move_materials(now, &self.anims.dice);
        // What the frame shows, for the interface.
        let camera = &built.frame.camera;
        self.seen.camera = Some(seen::SeenCamera {
            position: camera.position,
            heading: camera.heading,
            pitch: camera.pitch,
            fov_y: camera.fov_y,
            near: camera.near,
        });
        self.seen.block = (bx, by);
        self.seen.bodies.clear();
        self.drawn.clear();
        for (id, range, at, heading) in noted {
            let mut body = seen::SeenBody {
                id,
                origin: at,
                heading,
                bounds: None,
            };
            if let Some(b) = built.frame.objects.get(range.clone()) {
                body.bounds = seen::bounds_of(&body, b);
            }
            self.seen.bodies.push(body);
            self.drawn.push((id, range));
        }
        // The effects the objects play: lit particles take the light the frame's objects do.
        let axes = camera_axes(heading, pitch);
        let l = built.frame.light;
        let light = auberean_world::color::Color::new(
            l.direct_color.r * l.direct_brightness,
            l.direct_color.g * l.direct_brightness,
            l.direct_color.b * l.direct_brightness,
            1.0,
        );
        self.placed_effects.draw(&self.systems, now, light);
        self.others.draw_effects(
            &self.systems,
            &self.world.records,
            &self.anims,
            &mut self.trees,
            &self.land,
            (bx, by),
            self.ticker.alpha(now),
            now,
            light,
            night,
        );
        // The player's effects, what the bodies hold and what the client made, each where it is
        // drawn.
        let global = |at: &Position| figures::global(at.cell.0, dv(at.frame.origin));
        let heading_of = |at: &Position| auberean_physics::geom::frame_heading(&at.frame).degrees();
        let who = |player: bool| auberean_draw::effects::FxWho { player, night };
        let land = &self.land;
        let water = |at: &Position| land.water_plane(at.cell).map(|p| -p.d);
        if let Some(pl) = self.player.as_mut() {
            if let Some(fx) = pl.effects.as_mut() {
                // Its repulsors take the speed its physics moves it at.
                fx.fx.set_velocity(dv(pl.body.velocity));
                let changed = fx.think(
                    &self.systems,
                    &self.world.records,
                    &self.anims,
                    global(&pos),
                    body_heading,
                    pl.scale,
                    light,
                    now,
                    who(true),
                    water(&pos),
                    Some(&mut pl.figure),
                    pl.dress.as_mut(),
                );
                if let Some(look) = changed {
                    pl.scale = look.scale;
                    body_takes_look(&mut pl.body, &self.world.records, &mut self.trees, &look);
                    pl.body.minterp.scale = pv(look.scale);
                }
            }
        }
        for (id, at, scale) in &placed {
            self.held.think_effects(
                *id,
                &self.systems,
                &self.world.records,
                &self.anims,
                &mut self.trees,
                (global(at), heading_of(at), dv(*scale)),
                light,
                now,
                who(false),
                water(at),
            );
        }
        for (id, at, scale) in &made {
            if let Some(t) = self.temporaries.get_mut(*id) {
                if let Some(fx) = t.effects.as_mut() {
                    let changed = fx.think(
                        &self.systems,
                        &self.world.records,
                        &self.anims,
                        global(at),
                        heading_of(at),
                        dv(*scale),
                        light,
                        now,
                        who(false),
                        water(at),
                        t.figure.as_mut(),
                        t.dress.as_mut(),
                    );
                    if let Some(look) = changed {
                        t.scale = look.scale;
                        body_takes_look(&mut t.body, &self.world.records, &mut self.trees, &look);
                    }
                }
            }
        }
        // Inside, lit particles take the lights kept where each system stands instead, and the
        // carried lights that reach it.
        if let Some(outside) = built.outside {
            let builder = &mut self.builder;
            let carried = &carried;
            self.systems.light_queued(now, |home, at| {
                Some(builder.particle_lights(home, at, &outside, carried))
            });
        }
        built.frame.objects.extend(self.systems.batches(
            (bx, by),
            BLOCK_LENGTH,
            now,
            dv(eye),
            axes,
        ));
        self.status.batches = built.frame.objects.len() + built.frame.terrain.len();
        self.status.vertices = (
            built.frame.terrain.iter().map(|b| b.vertices.len()).sum(),
            built.frame.objects.iter().map(|b| b.vertices.len()).sum(),
        );
        true
    }

    /// The body under the pointer in the last world frame (window pixels of a window `size`):
    /// the first a ray from the camera through it meets.
    #[must_use]
    pub fn pick_world(&self, pointer: (f32, f32), size: (u32, u32)) -> Option<u64> {
        let built = self.built.as_ref()?;
        let camera = self.seen.camera.as_ref()?;
        #[allow(clippy::cast_precision_loss)]
        let size = (size.0 as f32, size.1 as f32);
        seen::pick(camera, size, pointer, &built.frame.objects, &self.drawn)
    }

    /// The moment of the day and the sky's day, from the game clock.
    /// The light's tick: once the sky's light interval (half way between its two; 30 s in the
    /// September 2013 data) has passed in game time since the world's lighting was last worked
    /// out, it is worked out again. How many times it has been.
    // D2-11025, D2-10909
    fn light_tick(&mut self) -> u32 {
        if self.game_time >= self.light_at {
            let every = self.world.region.sky.as_ref().map_or(LIGHT_EVERY, |s| {
                auberean_world::sky::light_tick_size(s, 0.5)
            });
            self.light_at = self.game_time + every;
            self.lightings = self.lightings.wrapping_add(1);
        }
        self.lightings
    }

    fn moment(&self) -> (f32, Option<DataId>) {
        let r = &self.world.region;
        match (r.calendar.as_ref(), r.sky.as_ref()) {
            (Some(cal), Some(sky)) if self.game_time > 0.0 => {
                let c = auberean_world::sky::day_clock(cal, self.game_time);
                let day = auberean_world::sky::day_for(
                    sky,
                    c.year * i64::from(cal.days_per_year) + c.day,
                );
                (c.fraction, day)
            }
            _ => (0.4, None),
        }
    }
}

/// What deciding the volumes of shadow reads from the scene.
struct Casting<'a> {
    records: &'a auberean_world::records::WorldRecords,
    table: &'a World,
    temporaries: &'a temporaries::Temporaries,
    /// The dynamic object shadows preference.
    setting: u32,
    model_detail: u32,
}

/// The dynamic objects' volumes of shadow for the frame just built, at the volumetric quality
/// (and above): each drawn body whose object casts one (none the client made itself), its
/// opaque draws put after the volumes; those within forty meters of the camera, nearest first,
/// cast their volumes while the frame has room, the shadow falling the way its lights throw it
/// (`throws`, inside) or the sun's way. The frame's shadow strength is the light's shadow
/// color outdoors, a mid gray inside (the camera's cell `view`). Bodies are drawn at full
/// detail, and their volumes are made from the level full detail takes.
// D2-11042, D2-11024, D2-11012, D2-06220, D2-06658
fn cast_volumes(
    frame: &mut Frame,
    noted: &[(u64, std::ops::Range<usize>, auberean_primitives::Vec3, f32)],
    throws: &HashMap<u64, auberean_primitives::Vec3>,
    view: u32,
    c: &Casting<'_>,
    maker: &mut auberean_draw::volume::VolumeMaker,
) {
    use auberean_draw::volume::{setting, VolumeRoom, Volumes, VOLUME_REACH};
    frame.volumes.volumes.clear();
    if c.setting < setting::VOLUMETRIC {
        return;
    }
    let inside = is_interior(view);
    frame.volumes.strength = Volumes::strength_of(frame.light.shadow_color, inside);
    let sun = frame
        .outside
        .as_ref()
        .unwrap_or(&frame.light)
        .light_direction;
    let eye = frame.camera.position;
    let mut casters = Vec::new();
    for (id, range, at, _) in noted {
        if c.temporaries.get(*id).is_some() {
            continue;
        }
        let casts = c.table.find(*id).is_some_and(|k| {
            c.table
                .graphics_state(k, auberean_physics::world::graphics::VOLUME_SHADOW)
        });
        if !casts {
            continue;
        }
        let Some(batches) = frame.objects.get_mut(range.clone()) else {
            continue;
        };
        for b in batches.iter_mut() {
            b.casts_volume = true;
        }
        let d = auberean_world::math::length(auberean_world::math::sub(*at, eye));
        if d <= VOLUME_REACH {
            casters.push((d, range.clone(), throws.get(id).copied().unwrap_or(sun)));
        }
    }
    casters.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut room = VolumeRoom::FRAME;
    let mut out = Vec::new();
    for (_, range, toward) in casters {
        let Some(batches) = frame.objects.get(range) else {
            continue;
        };
        maker.cast(
            c.records,
            batches,
            toward,
            1.0,
            c.model_detail,
            &mut room,
            &mut out,
        );
    }
    frame.volumes.volumes = out;
}

/// The ground decals are laid on: the two triangles of the square holding a point, the length of
/// a square's side, and whether the camera's cell is inside.
struct DecalGround<'a> {
    square: &'a dyn Fn(auberean_primitives::Vec3) -> Option<[auberean_draw::decal::Corner; 6]>,
    side: f32,
    inside: bool,
}

/// The simple setting's shadows: under each drawn body whose object casts a volume of shadow and
/// stands in a cell, a dark disc laid on the land, its width and depth 0.85 of its physics box's
/// (and more than a quarter meter each), two meters high, running the way the body faces, black
/// with the shadow's strength (its three channels' sum, a third of it) in its alpha, drawn once.
/// A body inside lays none (its rooms' floors are not taken).
// D2-11013
fn disc_shadows<'b>(
    frame: &Frame,
    noted: &[(u64, std::ops::Range<usize>, auberean_primitives::Vec3, f32)],
    c: &Casting<'_>,
    body_of: &dyn Fn(u64) -> Option<&'b Body>,
    ground: &DecalGround<'_>,
    now: f64,
    decals: &mut auberean_draw::decal::DecalManager,
) {
    use auberean_draw::decal::{disc, disc_material, landscape_decal};
    let material = disc_material(c.records);
    if material == 0 {
        return;
    }
    let strength =
        auberean_draw::volume::Volumes::strength_of(frame.light.shadow_color, ground.inside);
    for (id, _, at, heading) in noted {
        let casts = c.table.find(*id).is_some_and(|k| {
            c.table
                .graphics_state(k, auberean_physics::world::graphics::VOLUME_SHADOW)
        });
        let Some(body) = body_of(*id).filter(|_| casts) else {
            continue;
        };
        if body.position.cell.0 == 0 || is_interior(body.position.cell.0) {
            continue;
        }
        let b = body
            .parts
            .as_ref()
            .map(|p| p.array.physics_bounding_box())
            .unwrap_or_default();
        let lo = auberean_primitives::Vec3::new(b.min.x, b.min.y, b.min.z);
        let hi = auberean_primitives::Vec3::new(b.max.x, b.max.y, b.max.z);
        let Some(desc) = disc(
            material,
            lo,
            hi,
            dv(body.total_scale(None)),
            *heading,
            strength,
        ) else {
            continue;
        };
        let triangles = landscape_decal(*at, &desc, ground.side, ground.square);
        decals.create(body.position.cell.0, &desc, now, triangles, true);
    }
}

/// The pools of light the point lights standing outdoors (`lights`, each at its place in its own
/// cell's block, its range and intensity as they stand now, the intensity times how much of it
/// shows) lay on the land below them with `material`, each drawn once: the ground's height found
/// under each (in the frame of block `frame`).
// D2-11035
fn light_pools(
    lights: &[auberean_world::inside::Light],
    frame: (i32, i32),
    material: u32,
    ground: &dyn Fn(auberean_primitives::Vec3) -> Option<[auberean_draw::decal::Corner; 6]>,
    now: f64,
    decals: &mut auberean_draw::decal::DecalManager,
) {
    use auberean_draw::decal::{ground_height, point_light_pool};
    if material == 0 {
        return;
    }
    #[allow(clippy::cast_sign_loss)]
    let frame_cell = ((frame.0 as u32) << 24) | ((frame.1 as u32 & 0xFF) << 16) | 1;
    for l in lights {
        if l.cell == 0 || is_interior(l.cell) || l.kind != auberean_world::inside::LightKind::Point
        {
            continue;
        }
        let origin = auberean_world::math::add(
            l.origin,
            auberean_world::inside::block_offset(frame_cell, l.cell),
        );
        let Some(square) = ground(origin) else {
            continue;
        };
        let Some(height) = ground_height(&square, origin.x, origin.y) else {
            continue;
        };
        let Some(desc) =
            point_light_pool(material, origin, height, l.range, l.intensity, l.color, 1.0)
        else {
            continue;
        };
        let triangles =
            auberean_draw::decal::landscape_decal(origin, &desc, square_side_of(&square), ground);
        decals.create(l.cell, &desc, now, triangles, true);
    }
}

/// The side of a landscape square, from its two triangles.
fn square_side_of(square: &[auberean_draw::decal::Corner; 6]) -> f32 {
    let xs = square.iter().map(|c| c.0.x);
    let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
    hi - lo
}
