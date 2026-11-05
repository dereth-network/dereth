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
mod camera_object;
pub mod cells;
mod figures;
mod followed;
mod fx;
pub mod gaze;
mod held;
mod near;
mod notices;
pub mod objects;
pub mod others;
pub mod profile;
pub mod seen;
pub mod story;
mod targeted;
mod temporaries;
pub mod view;
mod weather;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use auberean_animation::body::{Body as Figure, AIR, EARTH};
use auberean_animation::turns::Ground;
use auberean_animation::CachedRecords;
use auberean_assets::animation::{MotionDesc, MotionParams};
use auberean_client_sdk::model::Object;
use auberean_client_sdk::{Action, Client, Motion};
use auberean_draw::body::BodyDrawer;
use auberean_draw::effects::PlacedEffects;
use auberean_draw::portal::{Look as PortalLook, Passage};
use auberean_draw::scene::{Frame, SceneBuilder, Spot};
use auberean_draw::systems::ParticleSystems;
use auberean_physics::body::missile::{ObjectTable, Objects};
use auberean_physics::body::placing::error as place_error;
use auberean_physics::body::{
    setup_spheres, Animate, AnimationClock, Body, BodyConfig, Notice, Ticker, JUMP_BEHAVIOR, TICK,
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
pub use figures::{desc_of, is_visible, look_of, visual_of};

/// The motion description bodies of the player's kind move by, when the object names none.
pub const PLAYER_MOTION: u32 = 0x3200_0000;
/// A block's side, meters.
pub const BLOCK_LENGTH: f32 = 160.0;
/// The lowest cell index of an interior cell; land cells are 1 to 64.
pub const FIRST_INTERIOR: u32 = 0x100;

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
    /// The behaviors the player's keys began (`true`) or ended (`false`) this frame.
    pub behaviors: Vec<(u32, bool)>,
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
// D2-09633, D2-09634, D2-09647
struct Player {
    id: u64,
    /// The description it was last sent, and its own descriptions (none when its look is a stored
    /// record's).
    visual: auberean_client_sdk::protocol::types::VisualDesc,
    dress: Option<auberean_animation::look::ObjectLook>,
    body: Body,
    figure: Figure,
    placements: u32,
    /// How many times the server's throws of it have been taken.
    thrown: u32,
    /// How many times the server's forcing of its place has been taken.
    forced: u32,
    height: f32,
    /// Its setup's radius.
    radius: f32,
    scale: Vec3,
    /// Just put somewhere by the server: stood on the ground once the ground there is loaded.
    settle: bool,
    /// Made, or teleported, and not yet in the world.
    arriving: bool,
    /// The last teleport of the server's it has taken.
    teleport_stamp: u16,
    /// Arriving at a place with no room for it: out of the world, and trying again each frame.
    no_room: bool,
    /// The behaviors its figure was last told the body shows.
    shown: BTreeSet<u32>,
    /// The effects it plays.
    effects: Option<fx::BodyFx>,
    /// The motion its interpreter was asked at its last tick (sideways, forward, up): what its
    /// reports carry while a move the server ordered steers it.
    ticked: dereth_primitives::Vec3,
    /// The physics record its physics object was made from, as the client named it.
    record: u32,
    /// A behavior that locks the player's actions is playing: what its keys ask of the body waits.
    actions_locked: bool,
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
    /// The landblocks whose objects fade in, each with the opacity its objects are drawn at this
    /// moment (between 0 and 1): those the near window took up within the last second. The
    /// bodies standing on their land are drawn so already (their tints' alpha).
    pub fade: Vec<((u8, u8), f32)>,
}

/// How far the landscape is drawn, as its two preferences say, and its shores, as theirs says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landscape {
    /// Blocks about the viewer's the far landscape reaches.
    pub far_radius: i32,
    /// Land cells about the viewer's that carry their frills (0: no frills).
    pub frill_radius: i32,
    /// How much of those cells' distance the frills show before they have faded.
    pub frill_reach: f32,
    /// The shores: 0 none, 1 their strips in the frame's alpha, 2 those and their foam.
    pub shore_effects: u32,
}

impl Landscape {
    /// The landscape's draw distance (very low to very high: none, 2, 4, 6 or 8 blocks; 4 for a
    /// value out of its range) and frill distance (disabled: none; low and medium: the cells next
    /// to the viewer's, low fading them out sooner, at 0.65 of their distance; high: two cells
    /// about it; one cell for a value out of its range) in `graphics`; and its shore effects
    /// (disabled, medium, high; medium for a value out of its range).
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
            shore_effects: match level(key::LANDSCAPE_SHORE_EFFECTS) {
                0 => 0,
                2 => 2,
                _ => 1,
            },
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
    /// The player's body moves: its velocity, up and down too, faster than about 0.014 m/s.
    pub moving: bool,
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
    /// Portal space is drawn: it comes in at once and goes out after the server is done with it.
    pub portal_drawn: bool,
}

/// The world frame kept between frames and what it was built for.
struct Built {
    frame: Frame,
    /// The block, the cell the camera stood in (or the player's interior cell), and the light's
    /// moment.
    key: (LandblockId, (i32, i32), u32, bool),
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
    /// The bodies the world's placed objects play through, one for each look.
    placed_bodies: auberean_draw::placed::PlacedBodies,
    /// The boxes of the bodies past their visible distance, in each one's own frame.
    far_boxes: HashMap<u64, Option<(auberean_primitives::Vec3, auberean_primitives::Vec3)>>,
    /// The rain, and how hard it falls.
    weather: weather::Weather,
    /// The player's body seen through under the pointer.
    hover: auberean_draw::body::HoverFade,
    /// Where the pointer stands in the window, and the window's size, once known.
    pub pointer: Option<((f32, f32), (u32, u32))>,
    /// The adaptive degrade's bias, with it on.
    adaptive_degrade: Option<f32>,
    /// How fast frames are being drawn.
    frame_rate: auberean_draw::quality::FrameRate,
    /// The meshes' connectivity the dynamic objects' volumes of shadow are made from.
    volumes: auberean_draw::volume::VolumeMaker,
    /// The dynamic object shadows preference (`auberean_draw::volume::setting`).
    object_shadows: u32,
    /// The decals laid on the ground, and the layers of each decal material.
    decals: auberean_draw::decal::DecalManager,
    decal_layers: HashMap<u32, Vec<auberean_draw::material::DrawLayer>>,
    /// The light source effects preference: the lights effects place, and their pools on the land.
    light_effects: bool,
    /// The frames drawn, each its stamp: a streak whose object was not drawn in a frame is not
    /// drawn in it; and the layers of each streak material.
    frames: u64,
    streak_layers: HashMap<u32, Vec<auberean_draw::material::DrawLayer>>,
    /// The lights the objects' effects carried in the last frame drawn, as they stood.
    carried: Vec<auberean_world::inside::Light>,
    land: Land,
    trees: Trees,
    /// The blocks whose interior cells were loaded.
    blocks: HashSet<(u8, u8)>,
    /// The landblocks kept about the player.
    near: near::NearBlocks,
    /// The cells about the player, asked for through the data cache.
    cells: cells::CellAsks,
    /// Where each body looks, by object.
    looks: BTreeMap<u64, gaze::Looking>,
    /// The session's data cache the world's records read the cell file through (none before a
    /// session with data files).
    cache: Option<auberean_client_sdk::runtime::cache::DataCache>,
    pub camera: CameraControl,
    player: Option<Player>,
    /// When physics ticks, for every body.
    ticker: Ticker,
    /// When the bodies' figures animate: once at least a thirtieth of a second has passed.
    anim_clock: AnimationClock,
    /// The world every body moves against: the player's, everyone else's, the objects with a
    /// shape and the scenery, and the cells' collision lists.
    table: World,
    others: others::Others,
    built: Option<Built>,
    /// The world is shown (it is hidden while the story plays).
    world_shown: bool,
    /// The camera's own object, made with the first player.
    camera_object: Option<camera_object::CameraObject>,
    /// When the time of day is next looked at and the world's lighting next worked out again (the
    /// client's own seconds), and how many times it has been.
    sky_at: f64,
    light_at: f64,
    lightings: u32,
    /// The block and the light's tick the dynamic objects' shadows were last taken at.
    land_casters_at: Option<((i32, i32), u32)>,
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
    /// The server moved the player to a new place in the latest update.
    player_teleported: bool,
    /// A scene on the way through a door plays: no portal space while it does.
    pub portal_scene_playing: bool,
    /// The player's body was placed with its cells loaded, last frame.
    ready_to_enter: bool,
    /// How many times the player's body has come to be placed with its cells loaded.
    arrivals: u32,
    /// The effects of the objects drawn with the world.
    placed_effects: PlacedEffects,
    /// The last frame built drew the world.
    world_was_drawn: bool,
    /// The particle systems every object's effects keep going.
    systems: ParticleSystems,
    /// The other objects as the bodies' guidance sees them, refreshed each frame.
    objects: ObjectTable,
    /// Whether the camera's shake has drawn its starts along the noise (once, the first time a
    /// shake moves it).
    shake_drawn: bool,
    /// Where the camera stood when last placed and where it stands now (none until it is first
    /// placed for a player), which a frame draws it between.
    pub camera_stands: Option<camera::Stands>,
    /// What the bodies hold.
    held: held::HeldObjects,
    /// The kinds of mesh the player's own body leaves out when drawn with the camera at its eyes.
    first_person_hidden: Vec<u32>,
    /// Which of the camera's stand-ins hang from the player: the one it hangs from, the one it
    /// looks at.
    stand_ins: [bool; 2],
    /// Effects asked of objects since the last frame: the object, the effect, its strength.
    asked_fx: Vec<(u64, u32, f32)>,
    /// The server's objects a behavior asked to go, hidden until the time each goes.
    going: Vec<(u64, f64)>,
    /// Effects asked of objects or stopped since the last frame, in order, for their bodies: the
    /// object, the effect, and its strength (none to stop it).
    body_fx: Vec<(u64, u32, Option<f32>)>,
    /// The effects playing that are aimed at another object.
    aimed: Vec<targeted::Aimed>,
    /// The aims of the effects asked this frame: object, effect, target.
    fx_aims: Vec<(u64, u32, u64)>,
    /// What the stories did on the bodies, counted.
    pub combat: story::StoryTally,
    /// The story player of each body a story was told on.
    stories: BTreeMap<u64, auberean_physics::story::StoryPlayer<story::Act>>,
    /// The objects the client made itself: the missiles of the stories.
    temporaries: temporaries::Temporaries,
    /// What the scripts last set on each body, and the handle in the world's table it was applied
    /// under (none for a body not in it).
    states: BTreeMap<u64, (Option<u32>, auberean_client_sdk::runtime::body::BodyState)>,
    /// The client's object whose physics object the client is asking the scene to make
    /// ([`objects`]).
    creating: Option<objects::Creating>,
    /// The cell the client was last told each body stands in (0 for none), by object.
    reported: BTreeMap<u64, u32>,
    /// The block the interior cells loaded are about (the player's), when there is a player.
    interiors_about: Option<(u8, u8)>,
    /// The objects the client made that left the world since the last frame's booking.
    left: Vec<u64>,
    /// The time of the frame being updated (the shell's running time, seconds).
    clock: f64,
    /// The modes bodies went into since the client's objects were last told, each with its
    /// object.
    told_modes: Vec<(u64, u32)>,
}

impl std::fmt::Debug for Scene<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// How deep under water the camera standing at `at` is: below its cell's water surface by more
/// than 0.0002 m; `None` above it, or in a cell with no water.
// D2-10899
// D2-10881
fn camera_in_water(land: &Land, at: &Position) -> Option<f32> {
    let plane = land.water_plane(at.cell)?;
    let o = at.frame.origin;
    let side = plane.normal.x * o.x + plane.normal.y * o.y + plane.normal.z * o.z + plane.d;
    (side < -0.0002).then_some(-side)
}

/// How high the water of cell `cell` lies, when it has any.
fn water_height(land: &Land, cell: CellId) -> Option<f32> {
    let plane = land.water_plane(cell)?;
    (plane.normal.z != 0.0).then(|| -plane.d / plane.normal.z)
}

/// How high the water of the land cell the camera stands in lies, when it stands in one.
fn exterior_water(land: &Land, _block: LandblockId, at: &Position) -> Option<f32> {
    if is_interior(at.cell.0) {
        return None;
    }
    water_height(land, at.cell)
}

/// The light `light` under the water of cell `cell`'s block, `depth` meters down: the water's fog
/// at that depth, the light the same.
// D2-10874
fn water_light(
    world: &WorldData,
    cell: CellId,
    light: &auberean_world::sky::DayLight,
    depth: f32,
) -> auberean_world::sky::DayLight {
    let Some(water) = world.region.water.as_ref() else {
        return *light;
    };
    let (x, y) = (cell.0 >> 24, (cell.0 >> 16) & 0xFF);
    auberean_world::fog::water_fog(light, water.lighting(x, y), depth)
}

/// Whether object `id` has a body told not to be drawn.
fn hidden_body(
    player: Option<&Player>,
    others: &others::Others,
    held: &held::HeldObjects,
    temporaries: &temporaries::Temporaries,
    id: u64,
) -> bool {
    let body = match player {
        Some(p) if p.id == id => Some(&p.body),
        _ => others
            .body(id)
            .or_else(|| held.body(id))
            .or_else(|| temporaries.get(id).map(|t| &t.body)),
    };
    body.is_some_and(Body::is_no_draw)
}

/// Whether object `id`'s figure (the player's, another's, a held object's or one the client
/// made) has faded out of sight.
fn faded_figure(
    player: Option<&Player>,
    others: &others::Others,
    held: &held::HeldObjects,
    temporaries: &temporaries::Temporaries,
    id: u64,
) -> bool {
    let figure = match player {
        Some(p) if p.id == id => Some(&p.figure),
        _ => others
            .figure(id)
            .or_else(|| held.figure(id))
            .or_else(|| temporaries.get(id).and_then(|t| t.figure.as_ref())),
    };
    figure.is_some_and(|f| !figures::is_visible(f))
}

fn dv(v: dereth_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// What a body's figure is told of it for an animation step of `dt` seconds: how fast the body
/// moved over its last tick, in its own frame (x to its right, y ahead, z up), with its
/// acceleration's share of the step; its rate of turn; and its size (its look's times its own).
/// Not the velocity the body asks for: a body held against a wall or at a ledge stands while it
/// still asks to run, and a step up or down shows in its vertical speed for the tick it took.
pub fn figure_motion(body: &mut Body, dt: f32) -> (Vec3, f32, Vec3) {
    body.update_animation(dt);
    (
        dv(body.anim_velocity),
        body.turn_rate,
        dv(body.total_scale(None)),
    )
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

/// The block a cell id names.
#[must_use]
pub fn block_of(cell: u32) -> LandblockId {
    #[allow(clippy::cast_possible_truncation)]
    LandblockId::new((cell >> 24) as u8, (cell >> 16) as u8)
}

/// What a behavior asked of a body whose animation is frozen ends with.
const BEHAVIOR_FROZEN: u32 = 0x5D;

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
/// air in the figure's mode, the defaults when it lists none for that mode (a building's mode is
/// its own); and its setup's height.
#[must_use]
pub fn body_config(
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
    let md: Option<MotionDesc> = records.get(motion);
    let values = |element| {
        md.as_ref()
            .and_then(|md| motion_values(md, element, figure.mode))
    };
    let ground = values(EARTH).unwrap_or_else(auberean_physics::motion::values::default_values);
    let air = values(AIR).unwrap_or(ground);
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
/// its making with it, the scale its look gives it and the scale its look gives what it holds; and
/// the whole of the motion description its look names, every element's and mode's values: water's,
/// the sea floor's and each kind of ground's as well as the earth's and the air's.
// D2-06230, D2-06411, D2-06354, D2-06355, D2-06367, D2-06370, D2-06177, D2-05837, D2-06553, D2-06554
// D2-06555
pub(crate) fn give_parts(
    body: &mut Body,
    records: &WorldRecords,
    trees: &mut Trees,
    look: &auberean_animation::look::Look,
) {
    let motion = if look.motion_desc.is_none() {
        PLAYER_MOTION
    } else {
        look.motion_desc.0
    };
    if let Some(md) = records.get::<MotionDesc>(motion) {
        body.minterp
            .set_description(motion, std::sync::Arc::new(md));
    }
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
// D2-06002, D2-06551, D2-06532, D2-06550
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

/// Whether object `id`'s game object is a player or a creature, what a missile that goes off on a
/// miss goes off on. An object the client knows nothing of is neither, and neither is any without
/// the client's scripts, whose records say what an object is.
fn is_player_or_creature(client: &mut Client, id: u64) -> bool {
    use auberean_client_sdk::runtime::repository::{is_creature, is_player};
    client
        .with_host(|h| is_player(h, id) || is_creature(h, id))
        .unwrap_or(false)
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

/// The streaks of what `holder` holds at holding location `location` (everything for 0) turned on
/// (`on`) or off, and of what those hold in turn: the objects the server made and those the client
/// made alike.
fn turn_held_streaks(
    held: &mut held::HeldObjects,
    temporaries: &mut temporaries::Temporaries,
    holder: u64,
    location: u32,
    on: bool,
) {
    held.turn_streaks(holder, location, on);
    let mut holders = vec![holder];
    while let Some(h) = holders.pop() {
        for made in temporaries.turn_streaks(h, location, on) {
            held.turn_streaks(made, location, on);
            holders.push(made);
        }
    }
}

/// What the camera is placed among: the world's cells, what stands in them, the other bodies and
/// the animations.
struct CameraWorld<'a, 'w> {
    land: &'a Land,
    table: &'a mut World,
    /// The camera's own object, which keeps it off the world outdoors.
    object: Option<&'a mut camera_object::CameraObject>,
    others: &'a others::Others,
    anims: &'a CachedRecords<'w>,
    stand_ins: [bool; 2],
}

/// The camera placed for player `p` where its body stands now, as the client places it each time
/// the player's physics ticks; each frame instead in mouse look (`each_frame`), where the frame
/// draws the body, that share of the way through its tick, so it keeps its place before the
/// camera: its rates, the
/// slope, the shoulder, its distance and glide over `dt`; the place it wants (riding the head bone
/// in first person while a behavior moves that camera); kept off the world by its lines of sight
/// (none in first person), in the cell the last ended in. Where it stood becomes where it stood
/// before, and it stands at the new place; each frame in mouse look, both, so a frame draws it
/// there and nowhere between. With no loaded cell for a line to start from it stays where it
/// stood.
// D2-10664, D2-10662, D2-10647
fn place_camera(
    camera: &mut CameraControl,
    stands: &mut Option<camera::Stands>,
    p: &Player,
    w: &mut CameraWorld<'_, '_>,
    dt: f32,
    each_frame: Option<f32>,
) {
    let (pos, alpha) = match each_frame {
        Some(alpha) => (p.body.drawn(alpha).0, alpha),
        None => (p.body.position, 1.0),
    };
    let subject = subject_of(p, pos, p.body.heading);
    let objects = followed_of(Some(p), w.others, w.anims, alpha, w.stand_ins);
    camera.update_with(dt, &subject, &objects);
    // In first person, while a behavior that moves the first-person camera plays and the avatar
    // option shows the player, the camera rides the head bone: its whole frame, a fifth of a meter
    // ahead of it and up.
    let head = if camera.rides_the_head(
        p.figure.modes.desired_mode,
        p.figure.modes.dynamic_fp_camera(),
    ) {
        let [x, y, z] = camera::HEAD_OFFSET;
        held::bone_position(
            &p.figure,
            w.anims,
            &pos,
            p.body.total_scale(None),
            camera::HEAD_BONE,
            &Mat4::translation(Vec3::new(x, y, z)),
        )
    } else {
        None
    };
    let viewer = match head {
        // D2-10700
        Some(at) => camera::Viewer {
            origin: at.frame.origin,
            look: camera::look_of(&at),
            rotation: Some(at.frame.rotation),
        },
        None => camera.viewer_with(&subject, &objects),
    };
    let traced = !camera.no_ray_trace();
    let placed = if traced {
        let lines = auberean_physics::camera::Lines {
            land: w.land,
            cell: pos.cell,
            origin: pos.frame.origin,
            target_offset: camera.target_offset(&subject),
            wanted: viewer.origin,
            env_only: camera.check_env_only(),
        };
        auberean_physics::camera::sight_lines(w.table, &lines).map(|seen| {
            let eye = match w.object.as_deref_mut() {
                Some(o) if !is_interior(seen.eye.cell.0) => o.stand(w.land, w.table, seen.eye),
                _ => seen.eye,
            };
            viewer.stand_at(eye.cell, eye.frame.origin)
        })
    } else {
        Some(viewer.stand_at(pos.cell, viewer.origin))
    };
    match (stands.as_mut(), placed) {
        (Some(s), Some(at)) => {
            s.set_previous(s.current);
            s.set_current(at);
            if each_frame.is_some() && traced {
                s.set_previous(at);
            }
        }
        (Some(s), None) => s.set_previous(s.current),
        (None, Some(at)) => *stands = Some(camera::Stands::at(at)),
        // Placed the first time with no line to keep it off the world: where it wants to be.
        (None, None) => {
            *stands = Some(camera::Stands::at(viewer.stand_at(pos.cell, viewer.origin)));
        }
    }
    // Where the next glide starts from, in the body's landblock's frame.
    if let Some(at) = placed {
        let shift = auberean_physics::geom::block_offset(pos.cell, at.cell);
        let o = at.frame.origin;
        camera.last = Some(camera::Viewer {
            origin: dereth_primitives::Vec3::new(o.x + shift.x, o.y + shift.y, o.z + shift.z),
            look: viewer.look,
            rotation: viewer.rotation,
        });
    }
}

/// What body `b` with figure `f` says of itself when the scripts ask: the element it is in and the
/// motion it was last asked for; its speed, acceleration and jump scales; its radius and height
/// (its setup's, by its look's and its own height scale); its mode, which is the mode its figure
/// was last asked to go to whether the scripts ask for the mode or the mode it is going to (0
/// with no figure); and the motion the server last sent for it, while it follows the positions
/// the server sends.
// D2-05894, D2-05895, D2-06391
#[must_use]
// D2-05903
pub fn reading_of(b: &Body, f: Option<&Figure>) -> auberean_client_sdk::runtime::body::BodyReading {
    let m = b.minterp.asked;
    let mode = f.map_or(0, |f| f.modes.desired_mode);
    auberean_client_sdk::runtime::body::BodyReading {
        element: b.element(),
        last_motion: [m.x, m.y, m.z],
        velocity_scale: b.velocity_scale(),
        acceleration_scale: b.acceleration_scale(),
        jump_scale: b.jump_scale(),
        radius: b.radius(),
        height: b.height(),
        mode,
        desired_mode: mode,
        remote_motion: b.has_remote_do_motion_values().then(|| {
            let v = b.remote_do_motion_values();
            [v.x, v.y, v.z]
        }),
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
/// One animation step of object `id`'s gaze, when it has one: its look sliders asked to move
/// before its figure's sliders do, toward what it looks at (its place among `targets`), the
/// direction the server sent, or for the player the way `camera` looks. Whether the player
/// stops looking at what it looked at.
#[allow(clippy::too_many_arguments)] // the body's parts and the step's world, as they lie
fn gaze_step(
    looks: &mut BTreeMap<u64, gaze::Looking>,
    targets: &HashMap<u64, gaze::Target>,
    camera: Option<&gaze::Camera>,
    id: u64,
    is_player: bool,
    body: &Body,
    figure: &mut Figure,
    step: f32,
) -> bool {
    let Some(l) = looks.get_mut(&id) else {
        return false;
    };
    let dir = l.dir;
    let Some(g) = l.gaze.as_mut() else {
        return false;
    };
    let owner = gaze::Owner {
        is_player,
        position: body.position,
        height: body.height(),
        scale: body.total_scale(None),
        sliders: [gaze::PITCH, gaze::ROLL, gaze::TURN].map(|s| figure.sliders.current(s)),
        dir,
    };
    let target = (g.target != 0).then(|| targets.get(&g.target)).flatten();
    let turned = g.update(step, &owner, target, camera);
    for a in turned.asks {
        figure.sliders.do_slider(0, a.slider, a.value, a.time);
    }
    turned.stop_looking
}

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
            placed_bodies: auberean_draw::placed::PlacedBodies::default(),
            far_boxes: HashMap::new(),
            weather: weather::Weather::default(),
            hover: auberean_draw::body::HoverFade::default(),
            pointer: None,
            adaptive_degrade: None,
            frame_rate: auberean_draw::quality::FrameRate::default(),
            volumes: auberean_draw::volume::VolumeMaker::default(),
            object_shadows: auberean_draw::volume::setting::VOLUMETRIC,
            decals: auberean_draw::decal::DecalManager::default(),
            decal_layers: HashMap::new(),
            light_effects: true,
            frames: 0,
            streak_layers: HashMap::new(),
            carried: Vec::new(),
            land: Land::new(),
            trees: Trees::default(),
            blocks: HashSet::new(),
            near: near::NearBlocks::default(),
            cells: cells::CellAsks::default(),
            looks: BTreeMap::new(),
            cache: None,
            camera: CameraControl::default(),
            player: None,
            ticker: Ticker::new(0.0),
            anim_clock: AnimationClock::new(0.0),
            table: {
                // The physics types' bits come from the install's property list.
                let mut t = World::new();
                if let Some(list) = world
                    .records
                    .get::<auberean_assets::property::PropertyList>(0x3400_0000)
                {
                    t.set_property_list(list);
                }
                // The physics materials what bodies touch may name.
                // D2-06766
                for id in world.records.portal_ids(0x3700_0000, 0x37FF_FFFF) {
                    if let Some(m) = world.records.get(id) {
                        t.add_material(id, m);
                    }
                }
                t
            },
            others: others::Others::default(),
            seen: seen::Seen::default(),
            drawn: Vec::new(),
            built: None,
            world_shown: true,
            camera_object: None,
            sky_at: 0.0,
            light_at: 0.0,
            lightings: 0,
            land_casters_at: None,
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
            player_teleported: false,
            teleport_stamp: None,
            portal_scene_playing: false,
            ready_to_enter: false,
            arrivals: 0,
            placed_effects: PlacedEffects::default(),
            world_was_drawn: false,
            systems: ParticleSystems::default(),
            objects: ObjectTable::default(),
            shake_drawn: false,
            camera_stands: None,
            held: held::HeldObjects::default(),
            first_person_hidden: first_person_hidden(&world.records),
            stand_ins: [false; 2],
            asked_fx: Vec::new(),
            going: Vec::new(),
            body_fx: Vec::new(),
            aimed: Vec::new(),
            fx_aims: Vec::new(),
            combat: story::StoryTally::default(),
            stories: BTreeMap::new(),
            temporaries: temporaries::Temporaries::default(),
            states: BTreeMap::new(),
            creating: None,
            reported: BTreeMap::new(),
            interiors_about: None,
            left: Vec::new(),
            clock: 0.0,
            told_modes: Vec::new(),
        };
        scene.builder.camera_moves = true;
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
    /// with the world around it loaded, which a place with no room for the body never lets it be;
    /// a teleport of the player wants it again, unless a door's scene plays in its place.
    // D2-01910, D2-01944, D2-01909, D2-10657, D2-10665, D2-10643, D2-10644, D2-11001, D2-11002, D2-04056, D2-04057
    fn follow_portal_space(&mut self, client: &Client, now: f64) {
        let in_session = self.entering || client.model.player.is_some();
        if !in_session {
            if self.in_session {
                self.portal.want(false, true, now);
            }
            self.in_session = false;
            self.teleport_stamp = None;
            self.portal_scene_playing = false;
            self.ready_to_enter = false;
        } else {
            if !self.in_session {
                self.portal.want(true, true, now);
            }
            self.in_session = true;
            let server_ready = client.model.player_object().is_some() && !client.in_portal_space();
            let stamp = client.body.teleport_stamp;
            let teleported = self.teleport_stamp.is_some_and(|s| s != stamp);
            self.player_teleported = teleported;
            if client.body.placed() {
                self.teleport_stamp = Some(stamp);
            }
            let client_ready = self.player.as_ref().is_some_and(|p| !p.arriving) && !teleported;
            self.portal.want(
                !(server_ready && client_ready) && !self.portal_scene_playing,
                false,
                now,
            );
            let ready = client_ready && !self.cells.blocking;
            if ready && !self.ready_to_enter {
                self.arrivals = self.arrivals.wrapping_add(1);
            }
            self.ready_to_enter = ready;
        }
        self.portal_look = self.portal.think(now);
        self.status.portal_drawn = self.in_portal_space();
    }

    /// How many times the player's body has come to be placed with its cells loaded: on entering
    /// the world and after each teleport.
    #[must_use]
    pub const fn arrivals(&self) -> u32 {
        self.arrivals
    }

    /// Show the world, or hide it (while the story plays): hidden, no world's frame is made.
    // D2-10654, D2-10655
    pub fn show_world(&mut self, on: bool) {
        self.world_shown = on;
    }

    /// Whether the world is shown.
    // D2-10653
    #[must_use]
    pub const fn world_shown(&self) -> bool {
        self.world_shown
    }

    /// Builds what [`Scene::frames`] hands out at `now`: the world's frame (when `world` and
    /// portal space leaves it in sight) and portal space's. The world's camera and the bodies
    /// drawn are then in [`Scene::seen`].
    pub fn build_frames(&mut self, now: f64, world: bool) {
        self.frame_rate.end_frame(now);
        let look = self.portal_look.clone();
        // D2-10658
        let world_drawn =
            world && self.world_shown && look.draw_world && self.build_world(now, look.world_fov);
        self.world_was_drawn = world_drawn;
        if !world_drawn {
            self.play_player_fx_undrawn(now);
        }
        self.portal_frame = self
            .portal
            .frame(&self.world.records, &mut self.builder, &look, now);
        self.prepared = Some(world_drawn);
    }

    /// A frame without the world: what the server asks of the player's effects still happens as
    /// it asks, its systems waiting to run until the player is drawn again.
    fn play_player_fx_undrawn(&mut self, now: f64) {
        let night = self.is_night();
        let Some(pl) = self.player.as_mut() else {
            return;
        };
        let Some(fx) = pl.effects.as_mut() else {
            return;
        };
        let at = pl.body.position;
        let water = self.land.water_plane(at.cell).map(|p| -p.d);
        fx.play_undrawn(
            &self.systems,
            &self.world.records,
            &self.anims,
            figures::global(at.cell.0, dv(at.frame.origin)),
            pl.body.heading,
            dv(pl.body.total_scale(None)),
            now,
            auberean_draw::effects::FxWho {
                player: true,
                night,
            },
            water,
            Some(&mut pl.figure),
        );
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
            fade: if world_drawn {
                self.near.fade(now)
            } else {
                Vec::new()
            },
        }
    }

    /// Draw every random number of the world from `random`, the client's one generator: the
    /// bodies' animations, their effects' choices and particle systems, missiles' turns, portal
    /// space's tunnels and the camera's shake.
    pub fn set_random(&mut self, random: &Random) {
        self.anims.dice = random.clone();
        self.builder.dice = random.clone();
        self.systems.set_dice(random);
        self.views.dice = random.clone();
        self.objects.dice = random.clone();
        self.portal.set_dice(random.clone());
    }

    /// The graphics preferences the world is drawn by, as the client takes them when it starts and
    /// whenever the player applies them: the landscape's ([`Landscape::of`]), and the model and
    /// material detail, the scenery draw distance, the textures' detail and filtering, the
    /// water's reflections, the landscape's object and cloud shadows, the dynamic objects'
    /// shadows, the light coronas, the lights effects place (and their pools) and the volumetric
    /// fog effects for the rest of the world's drawing. The world is drawn again by them at once.
    // D2-10656, D2-10525
    pub fn init_preferences(&mut self, graphics: &auberean_draw::quality::Graphics) {
        use auberean_draw::quality::key;
        let l = Landscape::of(graphics);
        self.builder.set_far_radius(l.far_radius);
        self.cells.set_far_radius(l.far_radius);
        self.builder
            .set_frills(l.frill_radius != 0, l.frill_radius, l.frill_reach);
        self.builder.shore_effects = l.shore_effects;
        #[allow(clippy::cast_possible_truncation)] // a few levels
        let detail = graphics.get(key::MODEL_DETAIL).map_or(2, |v| v as i32);
        self.builder.model_detail = u32::try_from(detail.clamp(0, 2)).unwrap_or(2);
        self.builder.material_detail = graphics.material_detail();
        self.builder.scenery_draw_distance = graphics.scenery_draw_distance();
        self.builder.textures = graphics.textures();
        self.drawer.model_detail = self.builder.model_detail;
        self.drawer.material_detail = self.builder.material_detail;
        self.drawer.still_scale =
            auberean_draw::mesh::scenery_scale(self.builder.scenery_draw_distance);
        self.adaptive_degrade = graphics.adaptive_degrade();
        self.systems.set_volumetric_fog(graphics.volumetric_fog());
        let on = |k: u32| graphics.get(k).is_some_and(|v| v != 0.0);
        self.builder.water_reflections = on(key::WATER_REFLECTIONS);
        self.builder.water_ripples = graphics.water_ripples();
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
        self.systems.set_streak_effects(on(key::STREAK_EFFECTS));
        self.systems.set_decal_effects(on(key::DECAL_EFFECTS));
        self.built = None;
    }

    /// What the player's effect preferences ask of the effects from now on.
    // D2-06910, D2-06911
    pub fn set_fx_channels(&self, channels: auberean_draw::effects::Channels) {
        self.systems.set_fx_channels(channels);
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

    /// The effects the player's body is playing, with their strengths.
    #[must_use]
    pub fn player_effects(&self) -> Vec<(u32, f32)> {
        self.player
            .as_ref()
            .and_then(|p| p.effects.as_ref())
            .map_or_else(Vec::new, fx::BodyFx::current_fx)
    }

    /// Whether the scene has the player's body.
    #[must_use]
    pub fn has_player(&self) -> bool {
        self.player.is_some()
    }

    /// The player's body: where it is and how fast it moves (nothing before it is made).
    #[must_use]
    pub fn player_motion(&self) -> Option<(Position, dereth_primitives::Vec3)> {
        self.player
            .as_ref()
            .map(|p| (p.body.position, p.body.velocity))
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
        let Some(p) = self.player.as_mut() else {
            return false;
        };
        // D2-06203
        if p.body.is_animation_frozen() {
            return true;
        }
        let took = p.figure.do_mode(anims, 0, mode);
        if took {
            p.body.set_mode(mode);
            self.told_modes.push((p.id, mode));
        }
        took
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

    /// The sounds the bodies' effects played or stopped since last taken, each with the object
    /// that makes it, in the order the bodies had their turns.
    pub fn take_sounds(&mut self) -> Vec<(u64, auberean_draw::effects::FxSound)> {
        let mut out = Vec::new();
        if let Some(fx) = self.player.as_mut().and_then(|p| p.effects.as_mut()) {
            out.extend(fx.sounds.drain(..).map(|s| (fx.id, s)));
        }
        self.others.take_sounds(&mut out);
        self.held.take_sounds(&mut out);
        self.temporaries.take_sounds(&mut out);
        out
    }

    /// What portal space asked of the sound since last taken.
    pub fn take_portal_sounds(&mut self) -> Vec<auberean_draw::portal::PassageSound> {
        self.portal.take_sounds()
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

    /// The world's table of what stands where, to change.
    pub fn table_mut(&mut self) -> &mut World {
        &mut self.table
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

    /// Whether the sounds object `id` makes are silent just now; `None` for an object with no body.
    // D2-06153
    #[must_use]
    pub fn should_mute_sound(&self, id: u64) -> Option<bool> {
        let find = |parent: u64| self.body(parent);
        self.body(id).map(|b| b.should_mute_sound(&find))
    }

    /// The physics record object `id`'s physics object was made from, as the client named it (0
    /// for none): the player's, or another's the scene draws; `None` for an object the scene made
    /// no physics object for.
    #[must_use]
    pub fn physics_record(&self, id: u64) -> Option<u32> {
        match &self.player {
            Some(p) if p.id == id => Some(p.record),
            _ => self.others.physics_record(id),
        }
    }

    /// Whether the bodies' guidance took object `id` for a player or a creature this frame; `None`
    /// for one it does not see.
    #[must_use]
    pub fn noted_player_or_creature(&self, id: u64) -> Option<bool> {
        self.objects.object(id).map(|o| o.player_or_creature)
    }

    /// The objects the client made itself that the scene has, by id.
    #[must_use]
    pub fn made_objects(&self) -> Vec<u64> {
        self.temporaries
            .ids()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// The cell object `id`'s body is lost in, out of the world until the cell is loaded (0 for
    /// none).
    #[must_use]
    pub fn lost_cell(&self, id: u64) -> u32 {
        self.others.lost_cell_of(id)
    }

    /// The batches object `id`'s body was drawn as in the last world frame (indices into its
    /// objects); `None` for a body not drawn in it.
    #[must_use]
    pub fn body_batches(&self, id: u64) -> Option<std::ops::Range<usize>> {
        self.drawn
            .iter()
            .find(|(d, _)| *d == id)
            .map(|(_, r)| r.clone())
    }

    /// When the near window took up landblock `block` (made, or found again among the free blocks),
    /// as the scene's clock reads; `None` for a block it does not hold.
    #[must_use]
    pub fn block_taken_up(&self, block: (u8, u8)) -> Option<f64> {
        self.near.stamp(block)
    }

    /// Whether the world's frame is built and kept for the next frame (made again when the moment
    /// or the camera's cell moves on, or the landscape is refreshed).
    #[must_use]
    pub fn world_built(&self) -> bool {
        self.built.is_some()
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

    /// For each particle system object `id`'s effects carry, the scales it draws at this frame,
    /// each with what they should come to: the object's whole scale times what its look gives its
    /// systems when its description scales it with its object, that alone otherwise.
    #[must_use]
    pub fn system_scales(&self, id: u64) -> Vec<(Vec<Vec3>, Vec3)> {
        let (fx, whole) = match &self.player {
            Some(p) if p.id == id => (p.effects.as_ref(), Some(p.body.total_scale(None))),
            _ => (
                self.others.effects_of(id),
                self.others.body(id).map(|b| b.total_scale(None)),
            ),
        };
        let (Some(fx), Some(whole)) = (fx, whole) else {
            return Vec::new();
        };
        let k = fx.fx.particle_scale();
        fx.fx
            .systems()
            .iter()
            .map(|c| {
                let (drawn, with_object) = self.systems.drawn_scales(&c.system);
                let whole = if with_object {
                    dv(whole)
                } else {
                    Vec3::new(1.0, 1.0, 1.0)
                };
                (drawn, Vec3::new(whole.x * k, whole.y * k, whole.z * k))
            })
            .collect()
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
    pub fn figure_mut(&mut self, id: u64) -> Option<&mut Figure> {
        figure_in(
            &mut self.player,
            &mut self.others,
            &mut self.held,
            &mut self.temporaries,
            id,
        )
    }

    /// The terrain type the player's motion and the player's effects play for.
    #[must_use]
    pub fn player_terrain(&self) -> Option<(u32, u32)> {
        let p = self.player.as_ref()?;
        Some((
            p.body.minterp.terrain_type(),
            p.effects.as_ref()?.fx.terrain(),
        ))
    }

    /// Whether the player's figure plays a behavior.
    #[must_use]
    pub fn player_figure_plays(&self, behavior: u32) -> bool {
        self.player
            .as_ref()
            .is_some_and(|p| p.figure.is_doing_behavior(behavior))
    }

    /// Plays a behavior on object `id`'s body, as the client plays one asked of an object: on its
    /// figure; and when it is passed on, the behavior alone (at its time scale, sending nothing,
    /// passed on again) on everything the body holds. Whether the figure took it: a body with no
    /// figure cannot.
    // D2-06263
    // D2-06262
    pub(crate) fn play_behavior(
        &mut self,
        id: u64,
        params: &auberean_animation::track::Params,
    ) -> bool {
        let frozen = self.body(id).is_some_and(|b| b.is_animation_frozen());
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
        // A frozen body plays nothing: it is not kept for the thaw, and what waits on it hears so.
        if frozen {
            if params.context != 0 {
                f.events
                    .push(auberean_animation::host::Event::BehaviorDone {
                        context: params.context,
                        error: BEHAVIOR_FROZEN,
                    });
            }
            return true;
        }
        // A behavior that carries a mode puts the figure in it first (a death behavior, its body
        // then staying down in the dead mode); the object is told the mode it went into.
        if params.mode != 0 && f.do_mode(anims, 0, params.mode) {
            self.told_modes.push((id, params.mode));
        }
        let took = f.do_behavior(anims, params);
        // What the body's effects aim at and clone from now on, until a behavior names others.
        if let Some(b) = self.body_mut(id) {
            if params.target != 0 {
                b.set_fx_target(params.target);
            }
            if params.clone_visual != 0 {
                b.set_look_to_clone(
                    params.clone_visual,
                    params.clone_appearance,
                    &params.clone_strengths,
                );
            }
        }
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
        if self.body(id).is_some_and(|b| b.is_animation_frozen()) {
            return true;
        }
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
        if let Some(p) = self.player.as_mut().filter(|p| p.id == id) {
            p.body.set_mode(mode);
        } else if let Some(b) = self.others.body_mut(id) {
            b.set_mode(mode);
        }
        self.told_modes.push((id, mode));
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

    /// Object `id` looks at `target` (none: at nothing), its head blending there (or back) over
    /// half a second; an object never looks at itself or at what it holds. What it looks at.
    // D2-06234, D2-08160, D2-05846
    pub fn look_at(&mut self, id: u64, target: u64) -> u64 {
        let held = self
            .held
            .holders()
            .iter()
            .any(|&(o, h)| o == target && h == id);
        let target = if target == id || held { 0 } else { target };
        let sliders = self.figure(id).map_or([0.0; 3], |f| {
            [gaze::PITCH, gaze::ROLL, gaze::TURN].map(|s| f.sliders.current(s))
        });
        let l = self.looks.entry(id).or_default();
        l.target = target;
        let g = l.gaze(id);
        if target == 0 {
            g.release(gaze::FADE);
        } else {
            g.look_at(target, gaze::FADE, sliders);
        }
        target
    }

    /// The camera's options for the player's head taken: the client is told whether the player
    /// looks at its selections, and the player's body is given its gaze (so its head follows the
    /// camera from the moment it arrives) with the options to look around and at the camera.
    // D2-06082, D2-06083, D2-06085
    fn take_look_options(&mut self, client: &mut Client) {
        let o = self.camera.options;
        client.look_at_selection = o.look_at_selection;
        let Some(id) = self.player.as_ref().map(|p| p.id) else {
            return;
        };
        let g = self.looks.entry(id).or_default().gaze(id);
        g.look_around = o.look_around;
        g.look_at_camera = o.look_at_camera;
    }

    /// Whether the cells about the player are being asked for (the land and the cells about it are
    /// made once they are in).
    #[must_use]
    pub fn cells_pending(&self) -> bool {
        self.cells.pending
    }

    /// Where each body looks (its gaze, what it was asked to look at, the direction the server
    /// sent), by object.
    #[must_use]
    pub fn looks(&self) -> &BTreeMap<u64, gaze::Looking> {
        &self.looks
    }

    /// The server says where object `id`'s head points (its pitch and turn): it turns there
    /// over half a second. The player's own head is the client's.
    // D2-06087, D2-05847
    pub fn look_dir(&mut self, id: u64, pitch: f32, turn: f32) {
        if self.player.as_ref().is_some_and(|p| p.id == id) {
            return;
        }
        let l = self.looks.entry(id).or_default();
        l.dir = (pitch, turn);
        l.gaze(id).look_dir();
    }

    /// Where the objects the bodies look at stand, as the gazes see them this step.
    fn gaze_targets(&self) -> HashMap<u64, gaze::Target> {
        self.looks
            .values()
            .filter_map(|l| l.gaze.as_ref())
            .filter(|g| g.target != 0)
            .filter_map(|g| {
                let b = self.body(g.target)?;
                Some((
                    g.target,
                    gaze::Target {
                        position: b.position,
                        height: b.height(),
                        mobile: b.is_mobile(),
                    },
                ))
            })
            .collect()
    }

    /// Whether the client's data cache holds a cell file: the cells about the player are asked
    /// of it then; a client given none reads the world's own file as it is.
    fn cache_has_cells(client: &Client) -> bool {
        let cache = client.data_download().cache();
        !cache.is_empty()
            && cache.stores().by_slot[auberean_client_sdk::dat::disk::Slot::Cell as usize].is_some()
    }

    /// The client's data cache for this session: the world's records read the cell file through
    /// it from now on (a record the server sent is read as the file's own), and the cells' asks
    /// are grown by its rings. A client whose cache holds no cell file leaves the world's file read
    /// as it is.
    fn attach_data_cache(&mut self, client: &mut Client) {
        if !Self::cache_has_cells(client) {
            if self.cache.take().is_some() {
                self.world.records.read_cells_through(None);
            }
            return;
        }
        let d = client.data_download_mut();
        let cache = d.cache().clone();
        if self.cache.as_ref().is_some_and(|c| c.is(&cache)) {
            return;
        }
        let through = cache.clone();
        self.world
            .records
            .read_cells_through(Some(std::sync::Arc::new(move |id, kind| {
                through
                    .stores()
                    .load_data(auberean_client_sdk::dat::QualifiedId { id, kind })
            })));
        d.requests.set_hook(
            cells::CELLS,
            Some(Box::new(cells::CellRings::new(
                std::sync::Arc::new(self.world.region.clone()),
                self.cells.far.clone(),
            ))),
        );
        self.cells.forget();
        self.cache = Some(cache);
    }

    /// What the data cache told the cells' asks: an ask in, or one to let go.
    fn hear_cells(&mut self, client: &mut Client) {
        use auberean_client_sdk::runtime::download::DataDownload;
        let d: &mut DataDownload = client.data_download_mut();
        let mut done = Vec::new();
        d.told.retain(|e| match e {
            auberean_assets::requests::Event::BatchDone { to, batch, .. }
                if *to == cells::CELLS =>
            {
                done.push(*batch);
                false
            }
            auberean_assets::requests::Event::Finished { to, .. } => *to != cells::CELLS,
            _ => true,
        });
        for b in done {
            self.cells.on_batch_done(d, b);
        }
    }

    /// The interior cells of the landblocks about `block` (its own and those beside it), each
    /// block's once its info is in; when `again`, a block some of whose cells were missing when
    /// it was loaded is loaded again (they may have come since).
    fn load_interiors(&mut self, block: LandblockId, again: bool) {
        let records = &self.world.records;
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                let (Ok(x), Ok(y)) = (
                    u8::try_from(i32::from(block.x()) + dx),
                    u8::try_from(i32::from(block.y()) + dy),
                ) else {
                    continue;
                };
                let (bx, by) = (i32::from(x), i32::from(y));
                let Some(info) = records.landblock_info(bx, by) else {
                    continue;
                };
                let complete = || {
                    (0..info.cell_count).all(|k| {
                        let low = u32::from(x) << 24 | u32::from(y) << 16 | (FIRST_INTERIOR + k);
                        self.land.cell(CellId(low)).is_some()
                    })
                };
                if self.blocks.insert((x, y)) || (again && !complete()) {
                    auberean_physics::interior::add_block_cells(
                        &self.land,
                        records,
                        &mut self.trees,
                        x,
                        y,
                    );
                }
            }
        }
    }

    /// The cells about the player follow the cell its body stands in: a new landblock outdoors,
    /// or a new interior cell, is asked of the data cache (with what it needs, a ring of
    /// landblocks at a time); once the ask is in, the interior cells about the player are loaded
    /// and the near window of landblocks follows the cell (inside, the land cell under it when its
    /// cell sees the outdoors). `placed`: the server just put the player there, and the world
    /// stands still until its cells are in. None without a player.
    // D2-10803, D2-10766
    fn follow_player_cell(&mut self, client: &mut Client, placed: bool) {
        let cell = self.player.as_ref().map_or(0, |p| p.body.position.cell.0);
        let visible = if Self::cache_has_cells(client) {
            self.cells.update(client.data_download_mut(), cell, placed)
        } else if cell == 0 {
            cells::Visible::Released
        } else {
            cells::Visible::Finish
        };
        match visible {
            cells::Visible::Released => {
                self.near.release_cells(&self.land);
                self.release_objects(client);
                self.seen.hearing = self.hearing();
                return;
            }
            cells::Visible::Waiting => {
                self.release_objects(client);
                self.seen.hearing = self.hearing();
                return;
            }
            cells::Visible::Finish => {}
        }
        let fresh = self.cells.take_fresh();
        if let Some(p) = &self.player {
            let block = p.body.position.cell.landblock();
            self.load_interiors(block, fresh);
        }
        let (cell, outside) = match &self.player {
            Some(p) => {
                let pos = p.body.position;
                let inside = pos.cell.0 & 0xFFFF >= 0x100;
                let outside = inside
                    .then(|| self.land.cell(pos.cell))
                    .flatten()
                    .filter(|g| g.seen_outside)
                    .map(|_| auberean_physics::geom::position::outside_cell(&pos).0)
                    .filter(|c| *c != 0);
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
            now: self.clock,
        };
        self.near.update_visible_cells(&mut m, cell, outside);
        self.release_objects(client);
        self.seen.hearing = self.hearing();
    }

    /// What the ambient sound hears about the player, as the cells about it stand now (none
    /// without a player): its cell and place, whether the cells are waited for, the interior
    /// cells in the visible set, whether the landscape is heard, and the near window's blocks.
    fn hearing(&self) -> Option<seen::SeenHearing> {
        let p = self.player.as_ref()?;
        let at = p.body.position;
        let cell = at.cell.0;
        let home = (cell >> 24, (cell >> 16) & 0xFF);
        let blocks: Vec<(u8, u8)> = self.near.blocks().collect();
        let records = &self.world.records;
        // The interior cells the window's landblocks see from outside.
        let from_outside = || {
            blocks.iter().flat_map(|&(x, y)| {
                let base = u32::from(x) << 24 | u32::from(y) << 16;
                records
                    .landblock_info(i32::from(x), i32::from(y))
                    .map(|info| {
                        info.outside_visible
                            .iter()
                            .map(|c| base | (c & 0xFFFF))
                            .collect::<Vec<u32>>()
                    })
                    .unwrap_or_default()
            })
        };
        let mut visible: Vec<u32> = Vec::new();
        let inside = cell & 0xFFFF >= FIRST_INTERIOR;
        let mine = inside.then(|| self.land.cell(at.cell)).flatten();
        if inside {
            if let Some(c) = &mine {
                visible.push(cell);
                visible.extend(c.stab_list.iter().map(|s| s.0));
                if c.seen_outside {
                    visible.extend(from_outside());
                }
            }
        } else {
            visible.extend(from_outside());
        }
        let mut seen_cells: Vec<u32> = Vec::new();
        let mut cells = Vec::new();
        for id in visible {
            if seen_cells.contains(&id) {
                continue;
            }
            seen_cells.push(id);
            let Some(c) = self.land.cell(CellId(id)) else {
                continue;
            };
            let o = c.frame.origin;
            #[allow(clippy::cast_precision_loss)] // a block or two
            let shift = |a: u32, b: u32| (i64::from(a) - i64::from(b)) as f32 * BLOCK_LENGTH;
            cells.push((
                self.land.cell_ambient_kind(CellId(id)),
                Vec3::new(
                    o.x + shift(id >> 24, home.0),
                    o.y + shift((id >> 16) & 0xFF, home.1),
                    o.z,
                ),
            ));
        }
        let outside =
            !inside || !seen_cells.contains(&cell) || mine.as_ref().is_some_and(|c| c.seen_outside);
        Some(seen::SeenHearing {
            cell,
            place: dv(at.frame.origin),
            blocking: self.cells.blocking,
            cells,
            outside,
            blocks,
        })
    }

    /// The behaviors the player's keys began or ended: played on its body or stopped, as a behavior
    /// asked by its id alone is.
    // D2-09640, D2-09641, D2-06262
    fn key_behaviors(&mut self, asked: &[(u32, bool)]) {
        let Some(id) = self.player.as_ref().map(|p| p.id) else {
            return;
        };
        for &(behavior, start) in asked {
            if start {
                self.play_behavior(id, &auberean_animation::track::Params::behavior(behavior));
            } else if let Some(p) = &mut self.player {
                p.figure.stop_behavior(&self.anims, behavior);
            }
        }
    }

    /// The world without a player: the camera forgets where it stood and whom it followed.
    // D2-10660, D2-10765, D2-10807, D2-09648, D2-03437
    fn reset_world_state(&mut self, client: &mut Client) {
        self.weather.indoors();
        self.cells.release(client.data_download_mut());
        self.camera.reset_camera();
        self.camera_stands = None;
        // The world's lighting is worked out afresh in the next world entered.
        self.sky_at = 0.0;
        self.light_at = 0.0;
        self.built = None;
        self.camera_object = None;
        // The land let go of, and the objects standing in it with it; the world entered next
        // starts with fresh free blocks.
        self.near.reset_world_state(&self.land);
        self.release_objects(client);
        self.near.notify_change_region();
        self.stand_ins = [false; 2];
        Self::drop_camera_stand_ins(client);
        self.seen.hearing = None;
    }

    /// The camera's stand-ins hung from the player, made with it when its setup has their holding
    /// locations (one the camera hangs from, one it looks at).
    // D2-10680
    fn setup_camera_stand_ins(&mut self, figure: &Figure) {
        self.stand_ins = followed::STAND_IN_LOCATIONS.map(|l| figure.has_holding_location(l));
    }

    /// The player's object made where the client placed its body (`b`): its body, its figure, and
    /// the camera set to follow it; its entry in the world's table takes the properties of the
    /// physics record `record` the client named.
    // D2-10663, D2-10661, D2-09648, D2-03437
    fn make_player(
        &mut self,
        o: &Object,
        record: u32,
        b: &auberean_client_sdk::runtime::Body,
    ) -> Option<Player> {
        let mut dress = None;
        let look = figures::look_after(&self.world.records, &self.anims, &o.visual, &mut dress)?;
        let mut figure = Figure::from_look(&self.anims, &look)?;
        // What its behaviors ask only of the player (the action lock, the camera) is asked.
        figure.is_player = true;
        let scale = look.scale;
        let (config, height) = body_config(&self.world.records, &look, &figure, &o.physics)?;
        let start = Position::new(
            CellId(b.cell),
            PFrame::new(pv(b.origin), dereth_primitives::Quat::IDENTITY),
        );
        let mut body = Body::new(config, start, b.heading);
        give_parts(&mut body, &self.world.records, &mut self.trees, &look);
        body.set_mode(figure.mode);
        profile::init_physics(&self.world.records, o, &mut body);
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
        others::apply_description(&self.world.records, &mut self.table, key, record);
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
        // D2-03437, D2-09648
        Some(Player {
            id: o.id,
            visual: o.visual.clone(),
            dress,
            body,
            figure,
            placements: b.placements(),
            thrown: b.thrown().0,
            forced: b.forced_moves(),
            height,
            radius,
            scale,
            settle: true,
            arriving: true,
            teleport_stamp: b.teleport_stamp,
            no_room: false,
            shown: BTreeSet::new(),
            effects,
            ticked: dereth_primitives::Vec3::ZERO,
            record,
            actions_locked: false,
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
    }

    /// One frame at `now` seconds, `dt` after the last, in the client's order: the player's intent
    /// (the camera's controls, the body steered); every body's physics ticks, the player's and then
    /// everyone else's, on one schedule; then the camera, placed when the frame is made; then what
    /// the figures asked beyond their animation (the camera's requests) and the camera's part of
    /// the effects played. (The camera's blur is not drawn.)
    // D2-10668, D2-05768
    pub fn update(&mut self, client: &mut Client, intent: &Intent, now: f64, dt: f32) {
        self.clock = now;
        self.attach_data_cache(client);
        self.hear_cells(client);
        if let Some(record) = client.world.take_region_change() {
            self.take_up_region(record);
        }
        let facing = self.player.as_ref().map_or(0.0, |p| p.body.heading);
        let mut asked_of: Vec<(u64, auberean_animation::host::Event)> = Vec::new();
        // What each figure finished this frame and the hooks it fired, for its body's story.
        let mut heard: Vec<(u64, Vec<auberean_animation::host::Completion>, Vec<u32>)> = Vec::new();
        for p in &intent.presses {
            self.camera.press_facing(*p, facing);
        }
        self.camera.hold(intent.held);
        self.camera.mouse(intent.mouse.0, intent.mouse.1);

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
                    self.reset_world_state(client);
                }
            }
            (Some(o), Some(p)) if p.id == o.id => {
                if p.visual != o.visual {
                    self.player_takes_visual(o);
                }
            }
            (Some(o), _) => {
                if self.camera_object.is_none() {
                    self.camera_object = client
                        .model
                        .allocate_temporary_id()
                        .and_then(|id| camera_object::CameraObject::new(&self.world.records, id));
                }
                self.create_player(client, o);
                if self.player.is_some() {
                    self.follow_player_cell(client, true);
                }
            }
        }
        if self.player.is_some() {
            self.light_tick(client, now);
        }
        if let Some(p) = &mut self.player {
            if client.body.forced_moves() != p.forced {
                p.forced = client.body.forced_moves();
                self.camera.player_forced_elsewhere();
            }
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
                if b.teleport_stamp != p.teleport_stamp {
                    p.teleport_stamp = b.teleport_stamp;
                    p.arriving = true;
                }
                tracing::info!(
                    cell = format_args!("{:#010x}", b.cell),
                    "the server placed the player"
                );
            }
        }
        if let Some(placed) = self.player.as_ref().map(|p| p.settle) {
            self.follow_player_cell(client, placed);
        }
        // A place under the ground outdoors is lifted onto it, as a placement is; then the body
        // stands where there is room for it nearest that place, as the client puts a body the
        // server placed (beside the start hall's fir it rises 0.69 m and steps 0.45 m aside):
        // once the cells it was put in are in. A body arriving where there is no room stays out
        // of the world, and tries again next frame.
        // D2-10657, D2-06059
        let cells_in = !self.cells.pending;
        if let Some(p) = &mut self.player {
            if cells_in && std::mem::take(&mut p.settle) {
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
                let r = p
                    .body
                    .set_position_simple(&self.land, &mut self.table, at, true);
                p.no_room = p.arriving
                    && matches!(r, place_error::NO_VALID_POSITION | place_error::COLLIDED);
                p.settle = p.no_room;
                if !p.no_room {
                    p.arriving = false;
                    p.body.set_placement_position(at);
                    // The camera is placed afresh there, not drawn from where it stood.
                    self.camera_stands = None;
                }
            }
        }
        client.body.out_of_world = self.player.as_ref().is_none_or(|p| p.arriving);
        // A throw of the server's comes after the place it came with.
        if let Some(p) = self.player.as_mut().filter(|p| !p.settle) {
            let (count, velocity) = client.body.thrown();
            if count != p.thrown {
                p.thrown = count;
                auberean_physics::motion::Owner::apply_impulse(&mut p.body, pv(velocity));
            }
        }
        // The camera's options for the player's head; its selections turn it (as the option to
        // look at them says).
        self.take_look_options(client);
        if let Some(id) = self.player.as_ref().map(|p| p.id) {
            for target in std::mem::take(&mut client.selection_looks) {
                self.look_at(id, target);
            }
        }
        // What the server asked of the bodies since the last frame; a body made this frame hears
        // it once it is made.
        client.stories_in_order = true;
        let events = std::mem::take(&mut client.physics);
        let mut waiting = self.play_server_events(client, events);
        // A story is played before what the server said after it takes effect: a killing arrow's
        // hit is shown on a target not yet dead, as the scripts ask.
        while client.take_held_to_next_story() {
            let events = std::mem::take(&mut client.physics);
            waiting.extend(self.play_server_events(client, events));
        }
        // One schedule of physics ticks for every body, and one animation step for every figure
        // (none this frame unless a thirtieth of a second has passed since the last).
        // While the cells the server put the player in are asked for, the world stands still.
        // D2-10801
        let still = self.cells.blocking;
        let (due, step) = (self.ticker.ticks(now), self.anim_clock.step(now));
        let ticks = if still { 0 } else { due };
        let anim = step.filter(|_| !still);
        // Where the objects the bodies look at stand, and whether the player stops looking.
        let targets = self.gaze_targets();
        let mut stop_looking = false;
        // The others as the bodies' guidance sees them this frame.
        self.objects.clear();
        if let Some(p) = &self.player {
            let asked = is_player_or_creature(client, p.id);
            self.objects.note(p.id, &p.body, asked);
        }
        self.others
            .note_objects(&mut self.objects, |id| is_player_or_creature(client, id));
        self.start_movement_actions(client, &intent.started);
        self.keep_move_target();
        self.key_behaviors(&intent.behaviors);
        if let Some(p) = &mut self.player {
            let frozen = client.body.frozen;
            let locked = p.actions_locked;
            let input = if frozen || locked {
                MotionInput::default()
            } else {
                MotionInput {
                    sideways: intent.sideways,
                    forward: intent.forward,
                    turn: intent.turn,
                }
            };
            // Mouse look steering sets the heading from the camera's direction and leaves no turn
            // to the keys.
            // D2-09649
            if !frozen && !locked {
                if let Some(heading) = self.camera.steering_heading(p.body.is_moving_to()) {
                    let by = (heading - p.body.heading + 540.0).rem_euclid(360.0) - 180.0;
                    if by != 0.0 {
                        p.body.turn_at_once(by);
                    }
                }
            }
            if !locked {
                p.body.steer(if self.camera.steers() {
                    MotionInput { turn: 0.0, ..input }
                } else {
                    input
                });
            }
            // The jump asked: standing, or moving slowly, the figure plays the jump's lead-in,
            // whose animation sets the jump off; one it cannot play, or moving faster, jumps at
            // once.
            let jumps = p.body.jumps;
            if intent.jump
                && !frozen
                && !locked
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
            self.table.file_those_waiting(&self.land);
            // Each tick the body moves while its movement runs, then its other updates run while
            // woken: its detections checked, what its collisions began and ended reported.
            for _ in 0..if p.no_room { 0 } else { ticks } {
                p.body.update_in(
                    &self.land,
                    &mut self.table,
                    &mut self.objects,
                    &NoGameObjects,
                    tick,
                );
                p.ticked = p.body.minterp.asked;
                // The player's physics updated: the camera is placed for where it now stands
                // (each frame instead in mouse look).
                if !self.camera.mouse_look {
                    let mut w = CameraWorld {
                        land: &self.land,
                        table: &mut self.table,
                        object: self.camera_object.as_mut(),
                        others: &self.others,
                        anims: &self.anims,
                        stand_ins: self.stand_ins,
                    };
                    place_camera(
                        &mut self.camera,
                        &mut self.camera_stands,
                        p,
                        &mut w,
                        tick,
                        None,
                    );
                }
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
            // The reports carry the motion the interpreter was asked (a diagonal cut to length
            // 1), the move the server ordered's while it steers the body.
            let asked = if p.body.is_moving_to() {
                p.ticked
            } else {
                p.body.minterp.asked
            };
            client.act(Action::Move(Motion {
                forward: asked.y,
                sideways: asked.x,
                heading: p.body.heading,
                active,
            }));
            let pos = p.body.position;
            client
                .body
                .follow(pos.cell.0, dv(pos.frame.origin), p.body.heading);
            client.body.turning_to(
                p.body
                    .minterp
                    .turning_to
                    .then(|| p.body.minterp.turn_to.degrees()),
            );
            if let Some(step) = anim.filter(|_| !p.body.is_animation_frozen()) {
                let (moved, turn, size) = figure_motion(&mut p.body, step);
                p.figure.set_motion(moved, turn, size);
                forward_behaviors(&p.body, &mut p.shown, &mut p.figure, &self.anims);
                p.figure.ground = ground_of(&p.body);
                let camera = gaze::Camera {
                    offset: self.camera.interp_viewer_offset(),
                    mouse_look: self.camera.mouse_look,
                };
                stop_looking = gaze_step(
                    &mut self.looks,
                    &targets,
                    Some(&camera),
                    p.id,
                    true,
                    &p.body,
                    &mut p.figure,
                    step,
                );
                p.figure.advance(&self.anims, step);
                p.body.figure_stands_in(p.figure.modes.element());
            }
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
            // D2-08202
            self.status.moving = p.body.is_moving();
            self.status.contact = p.body.contact;
            // In mouse look the camera is placed each frame for where the frame draws the body,
            // and first of all once there is a player to place it for; then a frame of its effects.
            if !still && (self.camera.mouse_look || self.camera_stands.is_none()) {
                let mut w = CameraWorld {
                    land: &self.land,
                    table: &mut self.table,
                    object: self.camera_object.as_mut(),
                    others: &self.others,
                    anims: &self.anims,
                    stand_ins: self.stand_ins,
                };
                let alpha = self.ticker.alpha(now);
                place_camera(
                    &mut self.camera,
                    &mut self.camera_stands,
                    p,
                    &mut w,
                    dt,
                    Some(alpha),
                );
            }
            self.camera.use_time(dt);
            self.fov = self.camera.fov;
        }
        // A player that walked into another cell has its surroundings follow it.
        if self.player.is_some() {
            self.follow_player_cell(client, false);
        }
        // While the world waits for its cells the client's objects wait too (their upkeep does
        // not run).
        client.world.blocking_for_cells = self.cells.blocking;

        self.follow_portal_space(client, now);
        // The server hears the scene is ready once a frame of the world has been drawn.
        client.world_frame_drawn = Some(self.world_was_drawn || !self.world_shown);

        // Everything else.
        let player = self.player.as_ref().map(|p| (p.id, p.body.position));
        let game_now = client.game_time();
        self.create_physics_objs(client);
        let looks = &mut self.looks;
        let mut look = |id: u64, body: &Body, figure: &mut Figure, step: f32| {
            gaze_step(looks, &targets, None, id, false, body, figure, step);
        };
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
            anim,
            &mut look,
        );
        for id in self.others.take_described() {
            self.take_description(client, id);
        }
        // The player stopped looking at what had been behind it or far too long.
        if stop_looking {
            if let Some(id) = self.player.as_ref().map(|p| p.id) {
                self.look_at(id, 0);
                client.looking_at = 0;
            }
        }
        // Where the player's head points, for the server to be told.
        if let Some(p) = &self.player {
            client.head = Some((
                p.figure.sliders.current(gaze::PITCH),
                p.figure.sliders.current(gaze::TURN),
            ));
        }
        heard.extend(self.others.take_heard());
        self.update_held(client, anim);
        if let Some(step) = anim {
            self.placed_bodies.advance(&self.anims, step);
        }
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
        self.carry_out_body_requests(client);
        self.stop_camera_fx_asked(client);
        self.update_temporaries(client, ticks, anim, &mut heard);
        self.publish_readings(client);
        // What each figure finished and the hooks it fired, to its body's story.
        for (id, done, hooks) in heard {
            self.story_heard(client, id, &done, &hooks);
        }
        self.report_cells(client);
        self.tell_modes(client);
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
        for b in &asked {
            self.carry_out_behavior(b);
        }
        // What the figures asked beyond their animation, and the effects the scripts played.
        asked_of.extend(self.others.take_figure_events());
        asked_of.extend(self.held.take_figure_events());
        asked_of.extend(self.temporaries.take_figure_events(client.local_time()));
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
        let hooks = played.len();
        self.route_figure_events(asked_of, &mut played, now);
        self.forget_those_gone(client, now);
        // The scripts' effects and the figures' play on the bodies too (those asked through the
        // scene already were).
        for &(id, effect, intensity) in &played[asked..] {
            self.body_fx.push((id, effect, Some(intensity)));
        }
        self.play_camera_effects(client, &played, now);
        let night = self.is_night();
        self.aim_fx(client, &played, hooks, night);
        self.make_startup_objects(client);
        self.ask_body_fx();
        self.end_aimed_fx(client.local_time());
        self.do_sliders_on_children();
        self.status.others = self.others.len();
        self.status.nearest = self.others.nearest(
            figures::global(self.status.cell, self.status.origin),
            4,
            self.ticker.alpha(now),
        );
    }

    /// What each body says of itself when the scripts ask, read for them after the frame's ticks
    /// ([`reading_of`]).
    fn publish_readings(&self, client: &mut Client) {
        let Some(scripts) = client.scripts.as_mut() else {
            return;
        };
        let readings = &mut scripts.host.readings;
        let materials = &mut scripts.host.materials;
        readings.clear();
        let mut note = |id: u64, b: &Body, f: Option<&Figure>| {
            readings.insert(id, reading_of(b, f));
            // Read again only when the body changes cell, which is when it changes.
            if let (Some(cell), Some(properties)) = (b.material_cell(), b.current_properties()) {
                if materials.get(&id).is_none_or(|m| m.cell != cell.0) {
                    let properties = properties.clone();
                    materials.insert(
                        id,
                        auberean_client_sdk::runtime::body::BodyMaterial {
                            cell: cell.0,
                            properties,
                        },
                    );
                }
            }
        };
        if let Some(p) = &self.player {
            note(p.id, &p.body, Some(&p.figure));
        }
        for (id, b) in self.others.bodies() {
            note(id, b, self.others.figure(id));
        }
        for id in self.held.ids() {
            if let Some(b) = self.held.body(id) {
                note(id, b, self.held.figure(id));
            }
        }
        for (id, _) in self.temporaries.ids() {
            if let Some(t) = self.temporaries.get(id) {
                note(id, &t.body, t.figure.as_ref());
            }
        }
        materials.retain(|id, _| readings.contains_key(id));
    }

    /// What the scripts asked of the bodies since the last frame, carried out in order on each
    /// body here as its physics object does: a speed or acceleration scale; drawing it cloaked
    /// (which also stops it being drawn); reporting or ignoring collisions; a push in the world's
    /// frame or its own, which puts it in the air at that velocity; a push away from another
    /// object (none when that object has no place here, or is the body itself); stopping a layer
    /// of its animation (its figure stops it this frame). Turning to look at an object is not
    /// carried out. A request for an object with no body here is dropped.
    // D2-06067, D2-06068, D2-06018, D2-06249, D2-06014, D2-06124, D2-05879, D2-05880, D2-06125
    // D2-05890
    fn carry_out_body_requests(&mut self, client: &mut Client) {
        use auberean_client_sdk::runtime::physics_object::BodyRequest as R;
        let asked = client
            .scripts
            .as_mut()
            .map(|s| std::mem::take(&mut s.host.body_requests))
            .unwrap_or_default();
        for r in asked {
            if let R::LookAt { object, target } = r {
                let looks = self.look_at(object, target);
                if self.player.as_ref().is_some_and(|p| p.id == object) {
                    client.looking_at = looks;
                }
                continue;
            }
            let object = match r {
                R::VelocityScale { object, .. }
                | R::AccelerationScale { object, .. }
                | R::Cloaked { object, .. }
                | R::ReportCollisions { object, .. }
                | R::IgnoreCollisions { object, .. }
                | R::LookAt { object, .. }
                | R::Impulse { object, .. }
                | R::TargetedImpulse { object, .. }
                | R::EndLayer { object, .. } => object,
            };
            // Where the object pushed away from stands, when it has a place here.
            let from = match r {
                R::TargetedImpulse { target, .. } if target != object => self.place_of(target),
                _ => None,
            };
            let Some(b) = self.body_mut(object) else {
                tracing::debug!(?r, "a request for a body the scene does not have");
                continue;
            };
            match r {
                R::VelocityScale { scale, .. } => b.set_velocity_scale(scale),
                R::AccelerationScale { scale, .. } => b.set_acceleration_scale(scale),
                R::Cloaked { on, .. } => b.set_cloaked(on),
                R::ReportCollisions { on, .. } => b.set_report_collisions(on),
                R::IgnoreCollisions { on, .. } => b.set_ignore_collisions(on),
                R::Impulse { vector, local, .. } => {
                    let v = dereth_primitives::Vec3::new(vector[0], vector[1], vector[2]);
                    if local {
                        b.add_impulse_local(v);
                    } else {
                        b.add_impulse(v);
                    }
                }
                R::TargetedImpulse {
                    target,
                    speed,
                    flags,
                    extra,
                    ..
                } => {
                    let Some(at) = from else { continue };
                    // The body measures the push from where the object stands now; what it kept
                    // of that object for a move stays as it was.
                    let kept = b.targets.insert(
                        target,
                        auberean_physics::motion::moveto::TargetBody {
                            position: at,
                            radius: 0.0,
                            height: 0.0,
                        },
                    );
                    b.add_targeted_impulse(target, speed, flags != 0, extra);
                    match kept {
                        Some(k) => {
                            b.targets.insert(target, k);
                        }
                        None => {
                            b.targets.remove(&target);
                        }
                    }
                }
                R::EndLayer { layer, .. } => b.stop_layer(layer),
                R::LookAt { .. } => {}
            }
        }
    }

    /// Turn on (`on`) or off the streaks of object `object` and of what it holds at holding
    /// location `location` (everything it holds for 0), and of what those hold in turn, as an
    /// effect that starts or stops a streak does.
    // D2-06197, D2-06198
    fn start_streak(&mut self, object: u64, location: u32, on: bool) {
        let own = match &mut self.player {
            Some(p) if p.id == object => p.effects.as_mut(),
            _ => self
                .others
                .effects_mut(object)
                .or_else(|| self.held.effects_mut(object).map(|(fx, _)| fx))
                .or_else(|| {
                    self.temporaries
                        .get_mut(object)
                        .and_then(|t| t.effects.as_mut())
                }),
        };
        if let Some(fx) = own {
            fx.fx.turn_streaks(on);
        }
        turn_held_streaks(&mut self.held, &mut self.temporaries, object, location, on);
    }

    /// Where object `id` stands here: its body's place while it is in the world, or its place in
    /// the world's table.
    fn place_of(&self, id: u64) -> Option<Position> {
        if let Some(b) = self.body(id) {
            return b.in_world.then_some(b.position);
        }
        self.table.find(id).and_then(|k| self.table.position(k))
    }

    /// The scripts asked every camera effect stopped since last asked.
    // D2-10690, D2-10754
    fn stop_camera_fx_asked(&mut self, client: &mut Client) {
        let asked = client
            .scripts
            .as_mut()
            .map_or(0, |s| std::mem::take(&mut s.host.camera_fx_stops));
        if asked > 0 {
            self.camera.stop_all_camera_fx();
        }
    }

    /// What the objects' scripts set on their bodies (as they do when an object is made or one
    /// of its qualities changes), applied to the bodies: what changed since it was last applied,
    /// or all of it for a body made again or entered in the world's table under another handle.
    /// The collision state goes to the body's entry in the table (the parts it marks set); drawn
    /// or not, its own scale (its entry sized by its whole scale) and whether it slides along
    /// edges go to the body itself, each once it was set.
    fn apply_body_states(&mut self, client: &Client) {
        let Some(scripts) = client.scripts.as_ref() else {
            return;
        };
        // D2-06029, D2-06030
        for (id, (movement, animation)) in &scripts.host.frozen {
            if let Some(b) = self.body_mut(*id) {
                // A frozen body's updates stop; thawed, they run again, as the keys here don't
                // wake a body the way its motion does.
                if b.is_movement_frozen() && !*movement {
                    b.set_movement_active(true);
                }
                b.set_movement_frozen(*movement);
                b.set_animation_frozen(*animation);
            }
        }
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
    pub fn body_mut(&mut self, id: u64) -> Option<&mut Body> {
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
        let mut changes = self.others.take_world_changes();
        changes.extend(
            std::mem::take(&mut self.left)
                .into_iter()
                .map(|id| (id, false)),
        );
        for (id, entered) in changes {
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

    /// Whether the player's body is making a move on its own (walking up to what it attacks or
    /// uses, or a move the server ordered).
    #[must_use]
    pub fn player_moving_to(&self) -> bool {
        self.player.as_ref().is_some_and(|p| p.body.is_moving_to())
    }

    /// Whether the land's squares blend their corners' grounds, from the next frame worked out.
    pub fn set_terrain_blending(&mut self, on: bool) {
        self.builder.terrain_blending = on;
    }

    /// Whether the server moved the player to a new place in the latest update.
    #[must_use]
    pub fn player_teleported(&self) -> bool {
        self.player_teleported
    }

    /// Whether a behavior that locks actions holds the player's own movement.
    #[must_use]
    pub fn actions_locked(&self) -> bool {
        self.player.as_ref().is_some_and(|p| p.actions_locked)
    }

    /// A behavior asked of an object: played, or stopped.
    pub fn carry_out_behavior(&mut self, b: &auberean_client_sdk::runtime::host::Behavior) {
        use auberean_client_sdk::runtime::host::Behavior;
        let (object, play) = match b {
            Behavior::Play { object, params } => (*object, Some(params_of(params))),
            Behavior::Stop { object, .. } => (*object, None),
        };
        match (play, b) {
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
                // D2-06076, D2-06425, D2-06426
                Event::TargetedCamera {
                    target,
                    behavior,
                    words,
                    time,
                    ramp,
                } => {
                    let asked = auberean_animation::track::Params::behavior(behavior);
                    if self.play_behavior(target, &asked) {
                        let objects = followed_of(
                            self.player.as_ref(),
                            &self.others,
                            &self.anims,
                            alpha,
                            self.stand_ins,
                        );
                        self.camera
                            .set_targeted_camera(&objects, target, words, time, ramp);
                    }
                }
                // What it holds fades to the opacity asked over the time asked.
                // D2-06232
                Event::ChildrenOpacity { opacity, time } => {
                    let children: Vec<u64> = self
                        .held
                        .holders()
                        .into_iter()
                        .filter(|(_, holder)| *holder == id)
                        .map(|(child, _)| child)
                        .chain(self.temporaries.held_by(id))
                        .collect();
                    for child in children {
                        if let Some(f) = self.figure_mut(child) {
                            f.sliders.do_slider(
                                0,
                                auberean_physics::body::parent::FADE_SLIDER,
                                opacity,
                                time,
                            );
                        }
                    }
                }
                // Hidden at once, and it goes three seconds on. (The objects the client made see
                // to their own.)
                // D2-06097, D2-05707
                Event::Remove => {
                    if self.player.as_ref().is_some_and(|p| p.id == id)
                        || self.going.iter().any(|(g, _)| *g == id)
                    {
                        continue;
                    }
                    if let Some(f) = self.figure_mut(id) {
                        f.sliders.do_slider(
                            0,
                            auberean_physics::body::parent::FADE_SLIDER,
                            0.0,
                            0.0,
                        );
                        self.going.push((id, now + temporaries::REMOVED_GONE));
                    }
                }
                // The body stops where it is, and its keys ask nothing more of it.
                // D2-06094, D2-09644
                Event::LockInput => {
                    if let Some(p) = self.player.as_mut().filter(|p| p.id == id) {
                        p.body.steer(MotionInput::default());
                        p.actions_locked = true;
                    }
                }
                // D2-06095
                Event::UnlockInput => {
                    if let Some(p) = self.player.as_mut().filter(|p| p.id == id) {
                        p.actions_locked = false;
                    }
                }
                other => tracing::trace!(
                    object = format_args!("{id:#x}"),
                    asked = ?other,
                    "a figure's request the scene does not take"
                ),
            }
        }
    }

    /// The server's objects whose time to go has come are forgotten, bodies and all.
    // D2-05707
    fn forget_those_gone(&mut self, client: &mut Client, now: f64) {
        let due = |at: f64| at <= now;
        for (id, _) in self.going.iter().filter(|(_, at)| due(*at)) {
            client.model.forget(*id);
            client.queue.remove_object(*id);
        }
        self.going.retain(|(_, at)| !due(*at));
    }

    /// The camera's part of each effect played on an object (`object`, its effect id, its
    /// strength), as the object's effects play it: every entry its effect table lists for the id
    /// (in its default variant) that is for this player if only for the player, and for the time
    /// of day, names a camera effect record, played from the object at the strength (none when the
    /// entry ramps in).
    // D2-06884, D2-06932
    fn play_camera_effects(&mut self, client: &Client, played: &[(u64, u32, f32)], now: f64) {
        if played.is_empty() {
            return;
        }
        // One already playing only takes its strength.
        let starting: Vec<_> = played
            .iter()
            .copied()
            .filter(|&(object, effect, _)| !self.fx_is_playing(object, effect))
            .collect();
        let records = &self.world.records;
        let night = self.is_night();
        let alpha = self.ticker.alpha(now);
        let player = self.player.as_ref().map(|p| p.id);
        for (object, effect, intensity) in starting {
            let Some(o) = client.model.object(object) else {
                continue;
            };
            let Some(look) = figures::look_of(records, &self.anims, &o.visual) else {
                continue;
            };
            let table = auberean_draw::effects::ObjectFxTable::of_group_with(records, &look.global);
            for data in table.fx_data(records, effect) {
                let node = auberean_draw::effects::node_for(&data, 0);
                if self.systems.fx_channels().mismatch(node) {
                    continue;
                }
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
                // D2-06447
                if let Notice::TerrainType(terrain) = n {
                    if let Some(f) = self.fx_of(from) {
                        f.fx.terrain = terrain;
                    }
                    continue;
                }
                // A missile's own streak, and what it holds, as it starts and stops flying.
                // D2-06197, D2-06198
                if matches!(n, Notice::StreakOn | Notice::StreakOff) {
                    self.start_streak(from, 0, n == Notice::StreakOn);
                    continue;
                }
                let Some(n) = self.temporary_notice_or(from, n, now) else {
                    continue;
                };
                // What its motion asks of the object: the effects of leaving the ground, of
                // landing and of a skid.
                if let Notice::Animate(Animate::Fx { fx, intensity }) = n {
                    self.do_fx(from, fx, intensity);
                    continue;
                }
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
    // D2-05999
    fn update_held(&mut self, client: &Client, anim: Option<f32>) {
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
        if let Some(step) = anim {
            self.held.advance(&self.anims, step);
        }
    }

    /// The world's wind at the moment, which missiles feel.
    #[must_use]
    pub fn wind(&self) -> Vec3 {
        dv(self.objects.wind.0)
    }

    /// How hard the rain the camera can see falls: 0 under cover.
    #[must_use]
    pub fn rain_heard(&self) -> f32 {
        self.weather.heard()
    }

    /// The frames of the interface's viewport scenes, their figures animated over `dt` seconds;
    /// the picks asked are answered in `views.picked`.
    pub fn view_frames(&mut self, scenes: &[view::ViewScene], dt: f32) -> Vec<view::ViewFrame> {
        self.drawer.begin_frame();
        // Nothing degrades with distance in the interface's scenes.
        self.drawer.eye = None;
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
        let lightings = self.lightings;
        let Some(p) = self.player.as_ref() else {
            return false;
        };
        // The body is drawn between its last two ticks, and the camera between where it stood
        // when it was last placed and where it stands now, at the same share of the way: in the
        // cell it stood in before, which chooses what the frame draws.
        // D2-10668
        let alpha = self.ticker.alpha(now);
        let (pos, body_heading) = p.body.drawn(alpha);
        let cell = pos.cell.0;
        let block = pos.cell.landblock();
        let origin = pos.frame.origin;
        // While the cells the player was just put in are waited for, the camera is there with it,
        // and from a cell not there yet nothing is drawn.
        // D2-11042
        let there = if is_interior(cell) {
            self.land.cell(pos.cell).is_some()
        } else {
            self.land.block(block.x(), block.y()).is_some()
        };
        if self.cells.blocking && !there {
            return false;
        }
        let Some(stands) = self.camera_stands else {
            return false;
        };
        let at = stands.slerp(alpha);
        let view = at.cell.0;
        // Back in the body's landblock's frame.
        let eye = {
            let shift = auberean_physics::geom::block_offset(pos.cell, at.cell);
            let o = at.frame.origin;
            dereth_primitives::Vec3::new(o.x + shift.x, o.y + shift.y, o.z + shift.z)
        };
        // The shakes under way move where the camera stands now, walls or not: the frames after
        // this one draw it moved (adding up over the frames until it is next placed).
        // D2-10673
        {
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
            if let Some(s) = self.camera_stands.as_mut() {
                let o = s.current.frame.origin;
                s.current.frame.origin =
                    dereth_primitives::Vec3::new(o.x + shake.x, o.y + shake.y, o.z + shake.z);
            }
        }
        let l = camera::look_of(&at);
        let flat = (l.x * l.x + l.y * l.y).sqrt();
        let heading = dereth_primitives::num::math::atan2f(l.x, l.y)
            .to_degrees()
            .rem_euclid(360.0);
        let pitch = dereth_primitives::num::math::atan2f(l.z, flat).to_degrees();
        let roll = camera::lean_of(&at);
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
        let viewer_cell = if is_interior(view) {
            (-1, i32::try_from(view & 0xFFFF).unwrap_or(0))
        } else {
            (
                dereth_primitives::num::floor_to_i32(eye.x / CELL_LENGTH),
                dereth_primitives::num::floor_to_i32(eye.y / CELL_LENGTH),
            )
        };
        // The frame is worked out again when the camera moves to another cell or block (its blocks'
        // order, the light and fog where it stands, the water it reflects about), goes under water
        // or comes up, or the light ticks.
        // D2-10933, D2-10921
        let in_water = !is_interior(view) && camera_in_water(&self.land, &at).is_some();
        self.builder.camera_in_water = in_water;
        let key = (block, viewer_cell, lightings, in_water);
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
                auberean_draw::scene::inside_light(
                    &mut f.light,
                    self.builder.block_lights(home).as_deref(),
                    dv(eye),
                    &outside,
                );
                f
            } else {
                // What the server made prints on the map as it stands when the light ticks, and
                // stays that way until the next tick.
                // D2-10929, D2-10921
                if self.land_casters_at != Some(((bx, by), lightings))
                    && self.builder.static_shadows
                {
                    self.land_casters_at = Some(((bx, by), lightings));
                    self.builder.dynamic_land_casters = self.others.land_casters(
                        &self.world.records,
                        &self.anims,
                        &mut self.drawer,
                        &self.table,
                        (bx, by),
                        self.ticker.alpha(now),
                    );
                }
                let mut f = self.builder.frame(&spot, self.fov);
                // The cells looked into from the land are drawn under the interior light of the
                // block the camera stands in: black when it has none.
                // D2-11007
                if !f.look_in.is_empty() {
                    let home = block_of(view);
                    let home = (i32::from(home.x()), i32::from(home.y()));
                    let outside = auberean_world::inside::Outside::of(&f.light);
                    let lights = self.builder.block_lights(home);
                    let mut light = f.light;
                    light.ambient_color = auberean_world::color::Color::new(0.0, 0.0, 0.0, 1.0);
                    auberean_draw::scene::inside_light(
                        &mut light,
                        lights.as_deref(),
                        dv(eye),
                        &outside,
                    );
                    f.looked_light = Some(light);
                }
                f
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
        let draws_player =
            self.camera.draws_the_player(p.figure.modes.desired_mode) && !p.body.is_no_draw();
        let at_the_eyes = self.camera.first_person_is_ready();
        // The objects the client made, where they stand this frame.
        let made = self.temporaries_placed(self.ticker.alpha(now), draws_player);
        let night = self.is_night();
        let Some(built) = self.built.as_mut() else {
            return false;
        };
        built.frame.objects.truncate(built.world_objects);
        // How opaque the world's objects are from here, as their blocks fade in.
        // D2-03691
        auberean_draw::scene::fade_objects(&mut built.frame.objects, dv(eye), &self.near.fade(now));
        // D2-05768
        self.placed_bodies.pose(
            &self.world.records,
            &self.anims,
            &mut self.drawer,
            &mut built.frame.objects,
        );
        self.drawer.eye = Some(dv(eye));
        self.drawer.reflecting = built.frame.reflection.is_some();
        self.drawer.moving_scale = self.adaptive_degrade.map_or(1.0, |bias| {
            auberean_draw::body::adaptive_scale(self.frame_rate.frames_per_second(), bias)
        });
        built.frame.camera.position = dv(eye);
        built.frame.camera.heading = heading;
        built.frame.camera.pitch = pitch;
        built.frame.camera.roll = roll;
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
                // Under water, the fog is the water's at the camera's depth.
                built.frame.light = match camera_in_water(&self.land, &at) {
                    Some(depth) => water_light(self.world, at.cell, &light, depth),
                    None => light,
                };
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
        let mut player_bones = None;
        if draws_player {
            let poses = p.figure.pose(&self.anims);
            let place = Mat4::from_parts(
                dv(origin),
                dq(pos.frame.rotation),
                dv(p.body.total_scale(None)),
            );
            let start = built.frame.objects.len();
            self.drawer.next_body(p.body.is_mobile());
            let hidden: &[u32] = if at_the_eyes {
                &self.first_person_hidden
            } else {
                &[]
            };
            self.drawer.drawing_player = true;
            built.frame.objects.extend(self.drawer.batches_without(
                &self.world.records,
                &p.figure,
                &poses,
                &place,
                hidden,
            ));
            self.drawer.drawing_player = false;
            noted.push((
                p.id,
                start..built.frame.objects.len(),
                dv(origin),
                body_heading,
            ));
            player_bones = weather::BodyBones::outdoors(pos.cell.0, || weather::BodyBones {
                bones: weather::bones_of(&p.figure, &poses),
                place,
                water: water_height(&self.land, pos.cell),
            });
        }
        let first_other = noted.len();
        self.others.draw_noting(
            &self.world.records,
            &self.anims,
            &mut self.drawer,
            (bx, by),
            self.ticker.alpha(now),
            &mut built.frame.objects,
            &mut noted,
        );
        // A body standing on the land of a block whose objects fade in is drawn at their opacity.
        let fade = self.near.fade(now);
        if !fade.is_empty() {
            for (id, range, _, _) in &noted[first_other..] {
                let Some(cell) = self.others.body(*id).map(|b| b.position.cell.0) else {
                    continue;
                };
                let block = (
                    u8::try_from(cell >> 24).unwrap_or(0),
                    u8::try_from((cell >> 16) & 0xFF).unwrap_or(0),
                );
                let Some((_, opacity)) = fade
                    .iter()
                    .find(|(b, _)| *b == block && cell & 0xFFFF < FIRST_INTERIOR)
                else {
                    continue;
                };
                for b in built
                    .frame
                    .objects
                    .get_mut(range.clone())
                    .into_iter()
                    .flatten()
                {
                    b.tint.a *= opacity;
                }
            }
        }
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
        seen::work_out_bodies(
            &built.frame.objects,
            &noted,
            self.object_shadows >= auberean_draw::volume::setting::VOLUMETRIC,
        );
        // A body shows by the cells it touches, what is held by its holder's.
        // D2-03577, D2-03583
        if built.frame.rooms.is_some() || !built.frame.look_in.is_empty() {
            let holders: HashMap<u64, u64> = self.held.holders().into_iter().collect();
            let inside = built.frame.rooms.is_some();
            let mut culled: HashMap<u64, auberean_draw::scene::Cull> = HashMap::new();
            for (id, range, _, _) in &noted {
                let root = holders.get(id).copied().unwrap_or(*id);
                let stands = if root == p.id {
                    Some((pos.cell.0, p.body.key()))
                } else if let Some(t) = self.temporaries.get(root) {
                    Some((t.body.position.cell.0, t.body.key()))
                } else {
                    self.others
                        .body(root)
                        .map(|b| (b.position.cell.0, self.others.key_of(root)))
                };
                let Some((cell, key)) = stands else {
                    continue;
                };
                // On the land everything is in view from the land, and the player always is.
                if !is_interior(cell) && (!inside || root == p.id) {
                    continue;
                }
                let touching: Vec<u32> = key
                    .and_then(|k| self.table.cells_of(k))
                    .map(|c| c.iter().map(|c| c.0).collect())
                    .unwrap_or_default();
                let Some(batches) = built.frame.objects.get_mut(range.clone()) else {
                    continue;
                };
                let cull = *culled
                    .entry(root)
                    .or_insert_with(|| auberean_draw::cull::body_cull(cell, &touching, batches));
                for b in batches.iter_mut() {
                    b.cull = cull;
                }
            }
        }
        // With the option on, the player's body and what it holds turn see-through under the
        // pointer, with the camera off its eyes.
        // D2-06220
        if self.camera.options.avatar_hover_transparency {
            let mine = noted
                .iter()
                .find(|n| n.0 == p.id)
                .map(|n| vec![(p.id, n.1.clone())])
                .unwrap_or_default();
            let over = !at_the_eyes
                && self.pointer.zip(self.seen.camera.as_ref()).is_some_and(
                    |((at, size), camera)| {
                        #[allow(clippy::cast_precision_loss)]
                        let size = (size.0 as f32, size.1 as f32);
                        seen::pick(camera, size, at, &built.frame.objects, &mine).is_some()
                    },
                );
            self.hover.step(now, over);
            let held: Vec<u64> = self
                .held
                .holders()
                .into_iter()
                .filter(|(_, holder)| *holder == p.id)
                .map(|(id, _)| id)
                .collect();
            for (id, range, _, _) in &noted {
                if *id == p.id || held.contains(id) {
                    for b in built
                        .frame
                        .objects
                        .get_mut(range.clone())
                        .into_iter()
                        .flatten()
                    {
                        b.tint.a *= self.hover.opacity;
                    }
                }
            }
        }
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
            let opacity = p.figure.sliders.opacity();
            placed_lights.extend(fx.lights(me, body_heading, pos.cell.0, now, opacity));
        }
        placed_lights.extend(self.others.effect_lights((bx, by), alpha, now));
        for (id, at, _) in &placed {
            if let Some(fx) = self.held.effects_of(*id) {
                let g = figures::global(at.cell.0, dv(at.frame.origin));
                let h = auberean_physics::geom::frame_heading(&at.frame).degrees();
                let opacity = self.held.figure(*id).map_or(1.0, |f| f.sliders.opacity());
                placed_lights.extend(fx.lights(g, h, at.cell.0, now, opacity));
            }
        }
        for (id, at, _) in &made {
            if let Some(fx) = self.temporaries.get(*id).and_then(|t| t.effects.as_ref()) {
                let g = figures::global(at.cell.0, dv(at.frame.origin));
                let h = auberean_physics::geom::frame_heading(&at.frame).degrees();
                let opacity = self
                    .temporaries
                    .get(*id)
                    .and_then(|t| t.figure.as_ref())
                    .map_or(1.0, |f| f.sliders.opacity());
                placed_lights.extend(fx.lights(g, h, at.cell.0, now, opacity));
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
        // Every body standing on the land takes the land's shadow and the sun's shadow map there,
        // but one seen through.
        // D2-06220
        if !is_interior(view) || built.frame.outside.is_some() {
            let sun = built
                .frame
                .outside
                .as_ref()
                .unwrap_or(&built.frame.light)
                .light_direction;
            for (id, range, at, _) in &noted {
                let body = if *id == p.id {
                    Some(&p.body)
                } else {
                    self.others.body(*id)
                };
                if body.is_some_and(|b| is_interior(b.position.cell.0)) {
                    continue;
                }
                let takes = body.and_then(Body::key).is_none_or(|k| {
                    self.table
                        .graphics_state(k, auberean_physics::world::graphics::TAKES_LAND_SHADOW)
                });
                if !takes {
                    continue;
                }
                let s = self.builder.land_shadow_at(*at, (bx, by), sun);
                for b in built
                    .frame
                    .objects
                    .get_mut(range.clone())
                    .into_iter()
                    .flatten()
                {
                    if !b.seen_through {
                        b.tint = auberean_world::color::Color::new(s, s, s, b.tint.a);
                        b.ground = Some(*at);
                    }
                }
            }
        }
        // Every body is drawn in the color and at the opacity its sliders hold.
        // D2-06474
        for (id, range, _, _) in &noted {
            let figure = if *id == p.id {
                Some(&p.figure)
            } else {
                self.others
                    .figure(*id)
                    .or_else(|| self.held.figure(*id))
                    .or_else(|| self.temporaries.get(*id).and_then(|t| t.figure.as_ref()))
            };
            let Some([r, g, b, a]) = figure.map(|f| f.sliders.draw_color()) else {
                continue;
            };
            for d in built
                .frame
                .objects
                .get_mut(range.clone())
                .into_iter()
                .flatten()
            {
                d.tint.r *= r;
                d.tint.g *= g;
                d.tint.b *= b;
                d.tint.a *= a;
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
        // D2-10588
        self.decals.think(now);
        if lays_light_pools(self.light_effects, view, built.frame.outside.is_some()) {
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
                    block: frame_block,
                    block_len: self.world.region.land.grid().block_length(),
                },
                now,
                &mut self.decals,
            );
        }
        // The decals the objects' effects laid since the last frame.
        // D2-06891, D2-10358
        {
            let mut asked = Vec::new();
            if let Some(p) = self.player.as_mut() {
                if let Some(fx) = p.effects.as_mut() {
                    let place = p.body.position;
                    asked.extend(fx.fx.take_decals().into_iter().map(|d| (place, d)));
                }
            }
            self.others.take_decals(&mut asked);
            self.held.take_decals(&mut asked);
            self.temporaries.take_decals(&mut asked);
            let frame_block = (bx, by);
            let builder = &self.builder;
            let ground = |at: auberean_primitives::Vec3| builder.ground_square(frame_block, at);
            lay_fx_decals(
                &self.world.records,
                &asked,
                &DecalGround {
                    square: &ground,
                    side: builder.square_side(),
                    inside: is_interior(view),
                    block: frame_block,
                    block_len: self.world.region.land.grid().block_length(),
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
            roll: camera.roll,
            fov_y: camera.fov_y,
            near: camera.near,
        });
        self.seen.block = (bx, by);
        let was_seen = std::mem::take(&mut self.seen.bodies);
        let mut far_was = std::mem::take(&mut self.far_boxes);
        let alpha = self.ticker.alpha(now);
        self.drawn.clear();
        for (id, range, at, heading) in noted {
            let mut body = seen::SeenBody {
                id,
                origin: at,
                heading,
                bounds: None,
                faded: faded_figure(
                    self.player.as_ref(),
                    &self.others,
                    &self.held,
                    &self.temporaries,
                    id,
                ),
            };
            if let Some(b) = built.frame.objects.get(range.clone()) {
                body.bounds = seen::bounds_of(&body, b);
            }
            // Past its visible distance a body, or a part of it, is not drawn, but it fills the box
            // it would: what marks it stays. The box is worked out once while it stays that far off.
            // D2-06119, D2-06626
            if self
                .others
                .leaves_out_a_part(id, &mut self.drawer, (bx, by), alpha)
            {
                body.bounds = far_was.remove(&id).unwrap_or_else(|| {
                    self.others
                        .draws_at_any_distance(
                            id,
                            &self.world.records,
                            &self.anims,
                            &mut self.drawer,
                            (bx, by),
                            alpha,
                        )
                        .and_then(|draws| seen::bounds_of(&body, &draws))
                });
                self.far_boxes.insert(id, body.bounds);
            }
            self.seen.bodies.push(body);
            self.drawn.push((id, range));
        }
        // A body told not to be drawn keeps its place on the screen: what marks it stays, though
        // nothing can be picked there.
        // D2-02108
        for body in was_seen {
            let hidden = hidden_body(
                self.player.as_ref(),
                &self.others,
                &self.held,
                &self.temporaries,
                body.id,
            );
            if hidden && !self.seen.bodies.iter().any(|b| b.id == body.id) {
                self.seen.bodies.push(body);
            }
        }
        // The effects the objects play: lit particles take the light the frame's objects do.
        let axes = auberean_draw::particles::leaned_camera_axes(heading, pitch, roll);
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
                    dv(pl.body.total_scale(None)),
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
        // The streaks: each body's effects turn its own and those of what it holds at the holding
        // location they name on or off; then each body's own are placed where it is drawn, each
        // held object's where it hangs and each made object's where it stands, run, and drawn
        // after the particle systems (none while the streaks preference is off).
        let mut asks: Vec<(u64, bool, u32)> = Vec::new();
        if let Some(pl) = self.player.as_mut() {
            if let Some(fx) = pl.effects.as_mut() {
                for (on, location) in fx.fx.take_streak_asks() {
                    fx.fx.turn_streaks(on);
                    asks.push((pl.id, on, location));
                }
            }
        }
        asks.extend(self.others.take_streak_asks());
        asks.extend(self.held.take_streak_asks());
        for (holder, on, location) in asks {
            turn_held_streaks(&mut self.held, &mut self.temporaries, holder, location, on);
        }
        self.frames = self.frames.wrapping_add(1);
        let streaks = {
            let records = &self.world.records;
            let cache = &mut self.streak_layers;
            let mut layers = |m: u32| {
                cache
                    .entry(m)
                    .or_insert_with(|| {
                        auberean_draw::material::instance_material(records, m)
                            .map(|mat| {
                                auberean_draw::material::draw_layers(
                                    &mat,
                                    auberean_draw::material::DEVICE_FLAGS,
                                )
                            })
                            .unwrap_or_default()
                    })
                    .clone()
            };
            let mut frame = held::StreakFrame {
                stamp: self.frames,
                block: (bx, by),
                now,
                layers: &mut layers,
                used: 0,
                out: Vec::new(),
            };
            if let Some(pl) = self.player.as_mut() {
                let scale = pl.body.total_scale(None);
                if let Some(fx) = pl.effects.as_mut() {
                    held::object_streaks(
                        &mut fx.fx,
                        draws_player.then_some((&pos, scale)),
                        &mut frame,
                    );
                }
            }
            self.others.streaks(&mut frame, alpha);
            self.held.streaks(&mut frame, &placed);
            for (id, at, scale) in &made {
                if let Some(fx) = self
                    .temporaries
                    .get_mut(*id)
                    .and_then(|t| t.effects.as_mut())
                {
                    held::object_streaks(&mut fx.fx, Some((at, *scale)), &mut frame);
                }
            }
            frame.out
        };
        if self.systems.streak_effects() {
            built.frame.objects.extend(streaks);
        }
        // Inside, lit particles take the lights kept where each system stands instead, and the
        // carried lights that reach it; outdoors, the carried lights that reach it before the sun.
        if let Some(outside) = built.outside {
            let builder = &mut self.builder;
            let carried = &carried;
            self.systems.light_queued(now, |home, at| {
                Some(builder.particle_lights(home, at, &outside, carried))
            });
        } else if !carried.is_empty() {
            let outside = auberean_world::inside::Outside::of(&built.frame.light);
            let carried = &carried;
            self.systems.light_queued(now, |home, at| {
                auberean_draw::scene::SceneBuilder::particle_lights_outdoors(
                    home, at, &outside, carried, light,
                )
            });
        }
        // The moment's rain and wind; the wind blows on the world's missiles wherever the camera is.
        // D2-11029, D2-10998
        let moment = self.builder.day(&spot).map_or((0.0, Vec3::ZERO), |d| {
            auberean_draw::rain::weather(&d, spot.time_of_day, &self.systems.dice())
        });
        self.objects.wind.set(pv(moment.1));
        self.anims.wind = auberean_world::math::length(moment.1);
        // The rain, and what it and the water do to the player's body.
        let sees_out = !is_interior(view)
            || built
                .frame
                .rooms
                .as_ref()
                .is_some_and(|r| r.sight(&built.frame.camera, 4.0 / 3.0).outdoors());
        if sees_out {
            let (right, _, _) = (axes.0, axes.1, axes.2);
            built.frame.objects.extend(self.weather.frame(
                &self.world.records,
                &mut self.builder,
                &self.systems,
                &self.land,
                moment,
                &weather::Viewer {
                    home: (bx, by),
                    eye: dv(eye),
                    right,
                    water: exterior_water(&self.land, block, &at),
                    now,
                },
                player_bones.as_ref(),
            ));
        } else {
            self.weather.indoors();
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

    /// The range of the last world frame's objects that are the batches of body `id`.
    #[must_use]
    pub fn drawn_batches(&self, id: u64) -> Option<std::ops::Range<usize>> {
        self.drawn
            .iter()
            .find(|(body, _)| *body == id)
            .map(|(_, range)| range.clone())
    }

    /// The body a press of the pointer falls on in the last world frame (window pixels of a window
    /// `size`): see [`seen::select`]. An object the client made itself is never chosen, nor the
    /// player and what it holds with the camera at its eyes; the player's own body is chosen only
    /// where nothing else is met.
    #[must_use]
    pub fn pick_world(&self, pointer: (f32, f32), size: (u32, u32)) -> Option<u64> {
        let built = self.built.as_ref()?;
        let camera = self.seen.camera.as_ref()?;
        #[allow(clippy::cast_precision_loss)]
        let size = (size.0 as f32, size.1 as f32);
        let player = self.player.as_ref().map(|p| p.id);
        let eyes = self.camera.first_person_is_ready();
        let holders = self.held.holders();
        let held_by_player = |id: u64| {
            let mut at = id;
            // What holds it, up to what holds nothing; no chain is longer than the table of what
            // is held.
            for _ in 0..=holders.len() {
                match holders.iter().find(|(item, _)| *item == at) {
                    Some((_, holder)) => at = *holder,
                    None => break,
                }
            }
            at != id && Some(at) == player
        };
        seen::select(
            camera,
            size,
            pointer,
            &built.frame.objects,
            &self.drawn,
            &self.seen.bodies,
            |id| {
                if self.temporaries.get(id).is_some() {
                    seen::Selectable::No
                } else if Some(id) == player {
                    if self.camera.first_person() {
                        seen::Selectable::No
                    } else {
                        seen::Selectable::Player
                    }
                } else if eyes && held_by_player(id) {
                    seen::Selectable::No
                } else {
                    seen::Selectable::Yes
                }
            },
        )
    }

    /// The light's tick, each frame in the world: when the sky's interval has passed (none in the
    /// September 2013 data), the time of day is looked at, and the world's lighting worked out
    /// again if the sky's light interval has passed since it last was (half way between its two;
    /// 30 s in that data) or the time of day was moved. Both intervals run on the client's own
    /// seconds, shortened by the rate the world's time runs at. A world with no sky or no day
    /// keeps its lighting, and looks again a day on.
    // D2-11025, D2-10909
    fn light_tick(&mut self, client: &mut Client, now: f64) {
        if now < self.sky_at {
            return;
        }
        let Some(clock) = client.game_clock() else {
            return;
        };
        let rate = f64::from(clock.dilation());
        let moved = clock.needs_light();
        let tomorrow = clock.time_til_tomorrow();
        let (known, day) = (self.game_time > 0.0, self.moment().1);
        let Some(sky) = self
            .world
            .region
            .sky
            .as_ref()
            .filter(|_| !known || day.is_some())
        else {
            self.sky_at += tomorrow;
            return;
        };
        let (tick, light_tick) = (
            auberean_world::sky::tick_size(sky, 0.5),
            auberean_world::sky::light_tick_size(sky, 0.5),
        );
        if self.light_at <= now || moved {
            self.light_at = light_tick / rate + now;
            self.lightings = self.lightings.wrapping_add(1);
            if let Some(clock) = client.game_clock_mut() {
                clock.lit();
            }
        }
        self.sky_at = tick / rate + now;
    }

    /// The moment of the day and the sky's day, from the game clock.
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
/// opaque draws put after the volumes, and each object the world places that casts one (the
/// frame's builder marked those); those within forty meters of the camera, nearest first,
/// cast their volumes while the frame has room, the shadow falling the way its lights throw it
/// (`throws`, inside; a placed object's own) or the sun's way. At self shadowing the casters are
/// drawn with every other object before the darkening and each makes its volumes as it is drawn,
/// at any distance, in the drawing order (farthest first), from the level drawn. The frame's shadow strength is the light's shadow
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
    use auberean_draw::volume::{
        placed_casters, setting, Caster, VolumeRoom, Volumes, VOLUME_REACH,
    };
    frame.volumes.volumes.clear();
    // A body its drawing properties make seen through is drawn with those seen through.
    for (id, range, _, _) in noted {
        let seen_through = c.table.find(*id).is_some_and(|k| {
            c.table
                .graphics_state(k, auberean_physics::world::graphics::TRANSLUCENT)
        });
        if let Some(batches) = frame.objects.get_mut(range.clone()) {
            for b in batches.iter_mut() {
                b.seen_through = seen_through;
            }
        }
    }
    // At self shadowing every object is drawn before the darkening, the casters too, so their own
    // shadows fall on them; else the casters are drawn after it.
    let as_drawn = c.setting >= setting::SELF_SHADOWING;
    for b in frame.objects.iter_mut().filter(|b| b.caster.is_some()) {
        b.casts_volume = !as_drawn;
    }
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
    // Made as each object is drawn, a caster at any distance casts.
    let reach = if as_drawn {
        f32::INFINITY
    } else {
        VOLUME_REACH
    };
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
            b.casts_volume = !as_drawn;
        }
        let d = auberean_world::math::length(auberean_world::math::sub(*at, eye));
        if d <= reach {
            casters.push(Caster {
                distance: d,
                draws: range.clone(),
                toward: throws.get(id).copied().unwrap_or(sun),
                detail: 1.0,
            });
        }
    }
    // The objects the world places that cast one, each where it stands.
    casters.extend(placed_casters(frame, sun, reach));
    // Made for the frame, nearest first; made as drawn, in the drawing order, farthest first.
    if as_drawn {
        casters.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    } else {
        casters.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    }
    let mut room = VolumeRoom::FRAME;
    let mut out = Vec::new();
    for caster in casters {
        let Some(batches) = frame.objects.get(caster.draws) else {
            continue;
        };
        let make = if as_drawn {
            auberean_draw::volume::VolumeMaker::cast_as_drawn
        } else {
            auberean_draw::volume::VolumeMaker::cast
        };
        make(
            maker,
            c.records,
            batches,
            caster.toward,
            caster.detail,
            c.model_detail,
            &mut room,
            &mut out,
        );
    }
    frame.volumes.volumes = out;
}

/// The ground decals are laid on: the two triangles of the square holding a point, the length of
/// a square's side, and whether the camera's cell is inside; the frame's block and a block's
/// length, where the interior cells' ground is found.
struct DecalGround<'a> {
    square: &'a dyn Fn(auberean_primitives::Vec3) -> Option<[auberean_draw::decal::Corner; 6]>,
    side: f32,
    inside: bool,
    block: (i32, i32),
    block_len: f32,
}

/// The simple setting's shadows: under each drawn body whose object casts a volume of shadow and
/// stands in a cell, a dark disc laid on the land (inside, on its cells' ground about it), its
/// width and depth 0.85 of its physics box's (and more than a quarter meter each), two meters
/// high, running the way the body faces, black with the shadow's strength (its three channels'
/// sum, a third of it) in its alpha, drawn once.
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
    use auberean_draw::decal::{disc, disc_material, dungeon_decal, landscape_decal};
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
        if body.position.cell.0 == 0 {
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
        let cell = body.position.cell;
        let triangles = if is_interior(cell.0) {
            // In its own block's frame, then carried into the frame's.
            let home = cell.landblock();
            let home = (i32::from(home.x()), i32::from(home.y()));
            #[allow(clippy::cast_possible_truncation)]
            let low = (cell.0 & 0xFFFF) as u16;
            #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
            let (dx, dy) = (
                (home.0 - ground.block.0) as f32 * ground.block_len,
                (home.1 - ground.block.1) as f32 * ground.block_len,
            );
            let mut t = dungeon_decal(c.records, home, low, dv(body.position.frame.origin), &desc);
            for v in t.iter_mut().flatten() {
                v.position = auberean_primitives::Vec3::new(
                    v.position.x + dx,
                    v.position.y + dy,
                    v.position.z,
                );
            }
            t
        } else {
            landscape_decal(*at, &desc, ground.side, ground.square)
        };
        decals.create(cell.0, &desc, now, triangles, true);
    }
}

/// The decals effects asked for, each laid off its object's place: inside on its cell's ground
/// about it, outdoors on the land (none where the frame built no land).
// D2-06891
fn lay_fx_decals(
    records: &auberean_world::records::WorldRecords,
    asked: &[(Position, auberean_draw::effects::FxDecal)],
    ground: &DecalGround<'_>,
    now: f64,
    decals: &mut auberean_draw::decal::DecalManager,
) {
    use auberean_draw::decal::{dungeon_decal, landscape_decal};
    for (place, decal) in asked {
        let cell = place.cell;
        if cell.0 == 0 {
            continue;
        }
        let o = dv(place.frame.origin);
        let at = auberean_primitives::Vec3::new(
            o.x + decal.offset.x,
            o.y + decal.offset.y,
            o.z + decal.offset.z,
        );
        let home = cell.landblock();
        let home = (i32::from(home.x()), i32::from(home.y()));
        #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
        let (dx, dy) = (
            (home.0 - ground.block.0) as f32 * ground.block_len,
            (home.1 - ground.block.1) as f32 * ground.block_len,
        );
        let triangles = if is_interior(cell.0) {
            #[allow(clippy::cast_possible_truncation)]
            let low = (cell.0 & 0xFFFF) as u16;
            let mut t = dungeon_decal(records, home, low, at, &decal.desc);
            for v in t.iter_mut().flatten() {
                v.position = auberean_primitives::Vec3::new(
                    v.position.x + dx,
                    v.position.y + dy,
                    v.position.z,
                );
            }
            t
        } else {
            let in_frame = auberean_primitives::Vec3::new(at.x + dx, at.y + dy, at.z);
            landscape_decal(in_frame, &decal.desc, ground.side, ground.square)
        };
        decals.create(cell.0, &decal.desc, now, triangles, true);
    }
}

/// Whether the lights standing outdoors lay their pools on the land this frame: with light effects
/// on, whenever the land is drawn, as it is from `view` outdoors or from a room that looks out
/// (`sees_outside`).
// D2-11035
#[must_use]
pub fn lays_light_pools(light_effects: bool, view: u32, sees_outside: bool) -> bool {
    light_effects && (!is_interior(view) || sees_outside)
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
