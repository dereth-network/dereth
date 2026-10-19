//! The editor: the runtime's application over the world's scratch overlay, the edit session, the
//! tools and panels, and the reload after each edit.
//!
//! **One frame.** The device events the window queued are read ([`EditorApp::read_input`]); the
//! camera flies or the body walks; the panels are built and answered; the tool of the mode acts
//! on the world under the pointer (select, move and turn with the handles, place a static, paint
//! the terrain, place a spawn); every edit is an operation of the session, and after one the
//! session writes the scratch overlay afresh, the store is laid over it again and the scene builds
//! the landblocks it reached again ([`EditorApp::reload`]); then the overlay is composed and the
//! runtime draws the frame. The scratch overlay is two folders that take turns
//! ([`lyceum_foundry::scratch`]): an edit writes only the records it reached, into the folder the
//! drawn store is not reading.
//!
//! **The modes.** Select picks statics, buildings, spawns and terrain vertices, and moves and
//! turns a selected static or spawn with its handles; Place puts the browser's model on the ground
//! or at a set height, the browser showing each model's picture ([`crate::thumbs`]) and name
//! ([`lyceum_foundry::models`]); Terrain paints with the brush; Spawn places the chosen weenie.

use dereth_assets::world::EnvCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_client_contract::actions::{movement, Action};
use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_runtime::present::Presentation;
use dereth_client_shell::gpu::Renderer;
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_primitives::{DataId, Frame, Position, Vec3};
use lyceum_foundry::brush::Brush;
use lyceum_foundry::compile::{Field, StaticOrigin};
use lyceum_foundry::edit::{Fault, Op, Session, Touched, VertexSet, Weenie};
use lyceum_foundry::models::{Category, ModelNames, Placements, Tags};
use lyceum_foundry::pick::{self, Hit, Mask, Models, Ray};
use lyceum_foundry::scratch::Scratch;
use lyceum_foundry::script::{Action as Edit, StaticName};
use lyceum_foundry::source::{Height, Place, Rotation};

use crate::args::Args;
use crate::camera::{landblock_of, FlyCamera, Held};
use crate::gizmo::{self, Handle};
use crate::script::{Camera, Pointer, Step, WeenieChoice};
use crate::shell::EditorShell;
use crate::view::View;

#[path = "panels.rs"]
mod panels;

#[path = "form.rs"]
mod form;
pub use form::{quests_named, FormEdit};

#[path = "play.rs"]
mod play;
pub use play::{playtest_dir, LiveArgs, PlayInterface, PlayOptions};

#[path = "dungeon.rs"]
mod dungeon;
pub use dungeon::{Filter as PieceFilter, Free, Group as PieceGroup, Kit};

#[path = "dungeons.rs"]
mod dungeons;
pub use dungeons::{with_destination, DungeonList};

#[path = "flows.rs"]
mod flows;
pub use flows::{Flow, Group as FlowGroup};

#[path = "fly.rs"]
mod fly;

#[path = "preview.rs"]
mod preview;
pub use fly::{Collider, RADIUS as FLY_RADIUS};

#[path = "nearby.rs"]
mod nearby;
pub use nearby::{Nearby, NearbyEntry, NearbyKind, NearbySort, REACH as NEARBY_REACH};
pub use panels::Tab;

/// The runtime's application with the editor's front end in it.
pub type CoreApp = dereth_client_runtime::app::App<EditorShell>;

/// What a press in the world does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Select,
    Place,
    Terrain,
    Spawn,
    Dungeon,
}

impl Mode {
    /// The mode a script names.
    #[must_use]
    pub fn named(s: &str) -> Option<Self> {
        Some(match s {
            "select" => Self::Select,
            "place" => Self::Place,
            "terrain" => Self::Terrain,
            "spawn" => Self::Spawn,
            "dungeon" => Self::Dungeon,
            _ => return None,
        })
    }
}

/// What is selected.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    /// A static, by its place in its landblock's compiled statics record.
    Static {
        landblock: u16,
        index: usize,
    },
    Building {
        landblock: u16,
        index: usize,
    },
    /// A world grid vertex.
    Vertex {
        gx: u32,
        gy: u32,
    },
    /// A spawn, by the file that places it and its guid as the file writes it.
    Spawn {
        file: PathBuf,
        guid: String,
    },
    /// A static standing in an indoor cell, by its place among the cell's compiled statics.
    CellStatic {
        landblock: u16,
        cell: u16,
        index: usize,
    },
    /// An indoor cell: a building's room or a dungeon's.
    Cell {
        landblock: u16,
        cell: u16,
    },
}

/// What a drag on the handles moves.
#[derive(Debug, Clone, PartialEq)]
enum Dragged {
    Static {
        which: StaticOrigin,
    },
    Spawn {
        file: PathBuf,
        guid: String,
    },
    /// A building of the base: the spawns that go with it go too.
    Building {
        index: usize,
    },
    /// A static standing in an indoor cell.
    CellStatic {
        cell: u16,
        index: usize,
    },
}

/// A drag on a handle of the selected static or spawn.
#[derive(Debug, Clone)]
struct Drag {
    handle: Handle,
    landblock: u16,
    what: Dragged,
    /// The pointer's ray when the drag began.
    from: Ray,
    /// The thing's frame when the drag began (landblock meters).
    start: Frame,
    /// Whether the drag has made its first step of the history.
    stepped: bool,
    /// The rest of the selection, each with its landblock and its frame when the drag began:
    /// moved and turned with it.
    group: Vec<(u16, Dragged, Frame)>,
}

/// What a drag on the handles does: a move (world meters; whether it is the up arrow's) or a turn
/// (degrees clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Change {
    Move(Vec3, bool),
    Turn(f32),
}

/// How far the pointer goes, in pixels, before a press in the world draws a box rather than
/// clicking.
const BOX_MIN: f32 = 6.0;

/// How many pictures a frame draws at most; the rest wait for the frames after.
const PICTURES_A_FRAME: u32 = 4;

/// Each model's picture, as the panels' texture (kept for as long as the browser may show it).
#[derive(Default)]
struct Pictures(std::collections::HashMap<u32, egui::TextureHandle>);

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} picture(s)", self.0.len())
    }
}

/// The world map's place among the pictures: no model's or texture's id.
const MAP_PICTURE: u32 = u32::MAX;

/// The Jump skill the walking body jumps with: a trained character's.
const JUMP_SKILL: i32 = 200;

/// One frame's pointer and wheel, as the world's tools read them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Input {
    pub pointer: (f32, f32),
    /// The left button is held.
    pub down: bool,
    /// The left button went down this frame.
    pub pressed: bool,
    /// The left button came up this frame.
    pub released: bool,
    /// Wheel notches this frame, away from the player positive.
    pub wheel: f32,
}

/// The panels' state.
#[derive(Debug, Clone)]
pub struct Panels {
    /// Whether the weenie tab shows every property (Advanced) under the form by what it is for.
    pub advanced: bool,
    pub filter: String,
    pub brush: Brush,
    pub radius: f32,
    pub on_ground: bool,
    pub height: f32,
    pub weenie: Option<Weenie>,
    /// The spawn browser's filter, and its category when it shows only one.
    pub weenie_filter: String,
    pub weenie_category: Option<Category>,
    /// What is being typed in the weenie form's fields, by field, until it is left.
    pub form_typing: BTreeMap<String, String>,
    pub message: String,
    /// The browser's category and tag, when it shows only one; the tag being typed.
    pub category: Option<Category>,
    pub tag: Option<String>,
    pub new_tag: String,
    /// The terrain whose scenes the scene brush lists: the one under the brush, last it was
    /// over the world.
    pub scene_terrain: u16,
    /// How far the map is zoomed in: 1 shows the whole world.
    pub map_zoom: f32,
}

/// A selected generator's marks as drawn: where it is on screen and the radius it scatters
/// over (meters), the places its profiles make things at, and the instances it owns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeneratorMarks {
    pub ring: Option<((f32, f32), f32)>,
    pub places: Vec<(f32, f32)>,
    pub children: Vec<(f32, f32)>,
}

/// A weenie the spawn tool can place.
#[derive(Debug, Clone, PartialEq)]
pub struct WeenieEntry {
    pub weenie: Weenie,
    pub name: String,
    pub category: Category,
    /// The setup it is drawn with, when its SQL says.
    pub setup: Option<u32>,
    /// Whether it is a pack's own.
    pub pack: bool,
    /// Its name, its symbol or class id, and its category, in lower case: what the filter reads.
    words: String,
}

impl WeenieEntry {
    fn new(weenie: Weenie, f: &lyceum_foundry::sql_ids::WeenieFacts, pack: bool) -> Self {
        let category = Category::of_weenie(f);
        let name = f
            .name
            .as_deref()
            .map(str::trim)
            .map(str::to_owned)
            .unwrap_or_else(|| match &weenie {
                Weenie::Symbol(s) => s.clone(),
                Weenie::Class(c) => format!("class {c}"),
            });
        let id = match &weenie {
            Weenie::Symbol(s) => s.clone(),
            Weenie::Class(c) => c.to_string(),
        };
        let words = format!("{name} {id} {}", category.name()).to_lowercase();
        Self {
            weenie,
            name,
            category,
            setup: f.setup,
            pack,
            words,
        }
    }

    /// Whether the entry answers the filter `f` (lower case): its name, its symbol or class id,
    /// or its category holds it.
    #[must_use]
    pub fn matches(&self, f: &str) -> bool {
        f.is_empty() || self.words.contains(f)
    }
}

/// A vertex the brush covers, as drawn: the world vertex, whether a dab changes it, and where it is
/// on screen.
pub type VertexMark = ((u32, u32), bool, (f32, f32));

/// The work of the frame drawn last: how long the panels (with the input and the tools) and the
/// world took, and how much each did.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Work {
    pub ui_ms: f32,
    pub world_ms: f32,
    /// The panels' triangles' corners, as drawn over the world.
    pub overlay_vertices: usize,
    /// The device's draws, the world's and the panels'.
    pub draw_calls: u64,
    /// Reads of a landblock's indoor cells.
    pub cells_read: u64,
    pub picks: u64,
}

/// What the editor did, for the tests and the status line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub frames: u64,
    pub edits: u64,
    pub reloads: u64,
    /// Landblocks built again after an edit.
    pub blocks_reread: u64,
    pub picks: u64,
    /// Records the edits put into or took out of the scratch overlay, and whole writes of it.
    pub records_written: u64,
    pub whole_writes: u64,
    /// Model pictures shown in the browser.
    pub pictures: u64,
}

/// The editor.
#[derive(Debug)]
pub struct EditorApp {
    pub core: CoreApp,
    pub shell: EditorShell,
    pub session: Session,
    scratch: Scratch,
    pub camera: FlyCamera,
    pub walking: bool,
    /// Whether the flying camera stops at the ground, walls and things ([`Collider`]).
    pub collide: bool,
    /// The physics world it is swept through, once it has flown.
    pub collider: Option<Collider>,
    pub mode: Mode,
    /// What is selected: the one the handles and the inspector show, and the rest of a selection
    /// of several statics and spawns.
    pub selection: Option<Selection>,
    pub also: Vec<Selection>,
    /// Where a press in the world began, until it ends as a click or a box.
    pub box_from: Option<(f32, f32)>,
    drag: Option<Drag>,
    /// A brush stroke in progress, which the next dab joins.
    stroke: bool,
    pub models: Models,
    /// The model the place tool places.
    pub model: Option<DataId>,
    all_models: Vec<DataId>,
    /// What each model is called, for the browser.
    pub names: ModelNames,
    /// Where the data files place each model, once gathered, and the gathering while it runs.
    placements: Option<Placements>,
    /// The world from above, once drawn, with every edit in it (its texture is among the
    /// pictures); where the map tab drew it last, and the part of it (in map pixels) it showed.
    pub world_map: Option<lyceum_foundry::map::WorldMap>,
    pub map_rect: Option<egui::Rect>,
    pub map_view: egui::Rect,
    /// Each terrain type's color on the map: its texture's mean.
    terrain_colors: Vec<[u8; 3]>,
    /// The landblock the server's spawns were last shown around, and how many landblocks
    /// either side are shown.
    shown_around: Option<u16>,
    radius: u32,
    catalog: Option<crate::catalog::Catalog>,
    /// The tags the user gives models, kept beside the world.
    pub tags: Tags,
    /// The browser's pictures: opened when first shown, with the folder they are kept in.
    thumbs: Option<Result<crate::thumbs::Thumbs, String>>,
    thumbs_root: PathBuf,
    /// The files the pictures are drawn from: the player's, without the world's overlay.
    base_store: Arc<dereth_dat::RetailDatStore>,
    /// Each model's picture, as the panels' texture.
    pictures: Pictures,
    /// The panels, and their docked layout (taken out while the panels are drawn).
    gui: crate::gui::Gui,
    dock: Option<egui_dock::DockState<panels::Tab>>,
    /// Where each of the checks' findings is listed on screen, as last drawn.
    pub fault_rects: Vec<egui::Rect>,
    /// The world tab's rectangle, as last drawn.
    pub world_rect: Option<egui::Rect>,
    /// The Nearby list: what stands round the camera.
    pub nearby: Nearby,
    /// The dungeon list.
    pub dungeon_list: DungeonList,
    /// The dungeon the camera was taken into or started, whose rooms it is looked for in first:
    /// most of a dungeon's rooms lie outside its landblock's square.
    pub dungeon_block: Option<u16>,
    /// Whether the camera is in a room of that dungeon, as last looked.
    pub in_dungeon: bool,
    /// The rooms of the landblock the flying camera is over, for which one it is in: read again
    /// after every edit.
    rooms: Option<(u16, Vec<lyceum_foundry::walk::CellShape>)>,
    /// Where the browser drew each model's picture, as last drawn.
    pub picture_rects: Vec<(DataId, egui::Rect)>,
    /// The vertices the brush covers, as drawn last.
    pub vertex_marks: Vec<VertexMark>,
    /// Every weenie the spawn tool can place: the packs' and the server's.
    pub weenies: Vec<WeenieEntry>,
    /// The server's names for the forms, when given; the server SQL its weenies were read from;
    /// the spell table's names, read the first time a form names a spell.
    pub schema: Option<lyceum_foundry::content::Schema>,
    pub server_sql: Option<PathBuf>,
    spells: Option<BTreeMap<u32, String>>,
    /// Each spawn's landblock and setup as last drawn, so a change of a weenie's SQL redraws
    /// what it changes.
    looks: BTreeMap<(PathBuf, String), (u16, Option<DataId>)>,
    /// Where each field of the weenie form was drawn last, by its key.
    pub form_fields: Vec<(String, egui::Rect)>,
    /// Where each weenie's cell is in the spawn browser, as drawn last.
    pub weenie_rects: Vec<(Weenie, egui::Rect)>,
    /// Where each terrain type's picture is in the terrain brush's list, and each scene value's
    /// row in the scene brush's, as drawn last.
    pub terrain_rects: Vec<(u16, egui::Rect)>,
    pub scene_rects: Vec<(u16, egui::Rect)>,
    /// Where the note beside a selected building was drawn last.
    pub building_note: Option<egui::Rect>,
    /// A selected generator's marks, as drawn last.
    pub generator_marks: GeneratorMarks,
    /// What each terrain's scenes grow, read the first time the scene brush shows it.
    scenes: BTreeMap<u16, Vec<Vec<DataId>>>,
    input: Input,
    held: Held,
    look: Option<(f32, f32)>,
    ctrl: bool,
    /// Shift held; a click made while it was (the rest of that hold does not lower the camera);
    /// Q held; and whether the body is walking because Shift is held.
    shift: bool,
    shift_clicked: bool,
    q_down: bool,
    shift_walking: bool,
    /// Device events a script queued for the frames to come, one batch a frame.
    later: std::collections::VecDeque<Vec<HostEvent>>,
    last_time: Option<f64>,
    pub panels: Panels,
    pub faults: Vec<Fault>,
    pub stats: Stats,
    /// The work of the frame drawn last.
    pub work: Work,
    /// Each landblock's indoor cells as read since the world last changed.
    cell_cache: std::cell::RefCell<BTreeMap<u16, std::rc::Rc<BTreeMap<u16, EnvCell>>>>,
    /// The editor's own world: where its parts are, the server while it starts and once it is
    /// up, whether an edit waits to reach it, how far its log is read, and a client playing it.
    live_args: LiveArgs,
    starting: Option<crate::live::Starting>,
    live: Option<crate::live::Live>,
    live_dirty: bool,
    live_read: usize,
    playing: Option<std::process::Child>,
    /// How the playtest client starts (the Play options), kept in its own settings.
    pub play_options: play::PlayOptions,
    /// Every word the editor shows, read from the creator's file and again when it changes.
    pub text: crate::text::Text,
    /// The content being written for GDLE, off the frame loop.
    gdle: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// The dungeon kit's browser and its choices.
    pub kit: Kit,
}

/// A landblock's coordinates in the window's terms.
fn block_xy(lb: u16) -> (i32, i32) {
    (i32::from(lb >> 8), i32::from(lb & 0xFF))
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

/// The device the editor draws on: the backend asked for (the default otherwise), on the
/// machine's own graphics card whether or not there is a window.
fn device(
    window: &dyn dereth_client_runtime::platform::window::WindowHost,
    w: u32,
    h: u32,
    cfg: &Config,
) -> Result<Box<dyn Presentation>, StartupError> {
    use dereth_render::device::{Backend, DeviceConfig, Gpu};
    let fallback = Backend::default_backend().ok_or_else(|| StartupError::Device {
        cause: "this build has no graphics backend".into(),
    })?;
    let backend = cfg.renderer.map_or(fallback, Backend::from);
    let device_cfg = DeviceConfig {
        width: w,
        height: h,
        force_software: false,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(backend, window.render_handles(), &device_cfg).map_err(|e| {
        StartupError::Device {
            cause: format!("graphics engine: {e}"),
        }
    })?;
    let scene =
        dereth_scene::gpu::SceneRenderer::on_device(gpu).map_err(|e| StartupError::Device {
            cause: format!("graphics engine: {e}"),
        })?;
    if scene.software() {
        return Err(StartupError::Device {
            cause: "the device is a software rasterizer; the editor draws on a graphics card"
                .into(),
        });
    }
    tracing::info!(
        "graphics backend {} on {}",
        backend.name(),
        scene.adapter_name()
    );
    Ok(Box::new(Renderer::around(scene)))
}

impl EditorApp {
    /// Open the world `args` names over the data files `cfg` names (the client's configuration
    /// for the runtime under the editor, from [`Args::runtime_argv`]), and draw it.
    ///
    /// # Errors
    /// The world, the data files or the device, with the reason.
    pub fn open(args: &Args, mut cfg: Config) -> Result<Self, String> {
        let text = match &args.text {
            Some(p) => crate::text::Text::open(p),
            // Beside the editor's settings; a run with none (a script's) has the built-in text.
            None => cfg
                .preferences_file
                .parent()
                .filter(|_| !cfg.preferences_file.as_os_str().is_empty())
                .map_or_else(crate::text::Text::built_in, |d| {
                    crate::text::Text::open(&d.join(crate::text::FILE_NAME))
                }),
        };
        let mut session = Session::open(&args.world, &cfg.dat_dir, args.pack.as_deref())
            .map_err(|e| e.to_string())?;
        let dat_dir = cfg.dat_dir.clone();
        if let Some(w) = &args.weenies {
            let text = std::fs::read_to_string(w).map_err(|e| format!("{}: {e}", w.display()))?;
            session.learn_weenies(&text);
        }
        let landblock = args
            .landblock
            .or_else(|| {
                session
                    .packs()
                    .iter()
                    .flat_map(|p| p.edits.iter().map(|e| e.landblock))
                    .next()
            })
            .unwrap_or(cfg.landblock);
        let scratch_root = session.world().out.join(".editor");
        let _ = std::fs::remove_dir_all(&scratch_root);
        let (scratch, first) =
            Scratch::create(&scratch_root, &session).map_err(|e| e.to_string())?;
        let first = first.dir;
        cfg.landblock = landblock;
        cfg.overlay_dat_dir = Some(first.clone());
        cfg.width = args.size.0;
        cfg.height = args.size.1;
        // The files as the player has them; the runtime lays the overlay over them, and again
        // after every edit.
        let base_cfg = Config {
            overlay_dat_dir: None,
            connect: false,
            ..cfg.clone()
        };
        let base = Arc::new(
            dereth_client_runtime::assets::open_store(&base_cfg).map_err(|e| e.cause.clone())?,
        );
        let base_store = Arc::clone(&base);
        let mut shell = EditorShell::default();
        let events = std::rc::Rc::clone(&shell.events);
        let headless = cfg.headless;
        let (w, h) = args.size;
        let mut core = CoreApp::bring_up_with_store(
            cfg,
            Some(base),
            move |cfg| {
                if headless {
                    Ok(Platform::headless(w, h))
                } else {
                    dereth_desktop::launch::open_platform::<crate::Lyceum>(cfg, events)
                }
            },
            device,
        )
        .map_err(|e| e.to_string())?;
        core.start_shell(&mut shell).map_err(|e| e.to_string())?;
        // Always day and no weather: a world is edited in a clear light, and a scripted run's
        // frames do not depend on the rain.
        {
            use dereth_client_model::player::option;
            let options = &mut core.objects.world.player_system.options;
            options.set(option::PERSISTENT_AT_DAY, true);
            options.set(option::DISABLE_MOST_WEATHER_EFFECTS, true);
            // The body runs, as a character does by default; Shift walks it.
            options.set(option::TOGGLE_RUN, true);
        }
        let scene = core.cfg.scene_config();
        core.load_static_scene(scene).map_err(|e| e.to_string())?;
        let block = core
            .world
            .as_ref()
            .and_then(dereth_client_runtime::world_state::WorldState::viewer_block)
            .unwrap_or_else(|| block_xy(landblock));
        let camera = core
            .world
            .as_ref()
            .map_or(FlyCamera::new(Vec3::ZERO, 0.0, 0.0), |w| {
                FlyCamera::from_free(&w.camera, block, 3)
            });
        let all_models: Vec<DataId> = session
            .base()
            .portal
            .iter_ids()
            .filter(|id| matches!(id.raw() >> 24, 0x01 | 0x02))
            .collect();
        let thumbs_root = args
            .thumbnails
            .clone()
            .unwrap_or_else(crate::thumbs::default_root);
        let schema = read_schema(args, &thumbs_root)?;
        session.set_schema(schema.clone());
        let faults = session.check(&first).map_err(|e| e.to_string())?;
        // What each model is, gathered on a thread of its own; the server database in the
        // user's cache is read there too, when the command line named no SQL.
        let database = args
            .weenies
            .is_none()
            .then(|| args.server_database.then(crate::catalog::server_database))
            .flatten()
            .flatten();
        if let Some(d) = &database {
            tracing::info!("the model browser names models by {}", d.display());
        }
        let terrain_colors = session
            .region()
            .map(|r| terrain_colors(&base_store, r))
            .unwrap_or_default();
        let catalog = crate::catalog::Catalog::start(
            dat_dir,
            session.region().cloned(),
            database,
            thumbs_root.clone(),
            terrain_colors.clone(),
        );
        let tags = Tags::read(&args.world)?;
        let mut app = Self {
            core,
            shell,
            session,
            scratch,
            camera,
            walking: false,
            collide: false,
            collider: None,
            mode: Mode::Select,
            selection: None,
            also: Vec::new(),
            box_from: None,
            drag: None,
            stroke: false,
            models: Models::default(),
            model: None,
            all_models,
            names: ModelNames::default(),
            placements: None,
            world_map: None,
            map_rect: None,
            map_view: egui::Rect::NOTHING,
            terrain_colors,
            shown_around: None,
            radius: args.radius,
            catalog: Some(catalog),
            tags,
            thumbs: None,
            thumbs_root,
            base_store,
            pictures: Pictures::default(),
            gui: crate::gui::Gui::new(),
            dock: Some(panels::default_dock()),
            fault_rects: Vec::new(),
            world_rect: None,
            rooms: None,
            nearby: Nearby::default(),
            dungeon_list: DungeonList::default(),
            dungeon_block: None,
            in_dungeon: false,
            picture_rects: Vec::new(),
            vertex_marks: Vec::new(),
            weenies: Vec::new(),
            schema,
            server_sql: args.weenies.clone(),
            spells: None,
            looks: BTreeMap::new(),
            form_fields: Vec::new(),
            weenie_rects: Vec::new(),
            terrain_rects: Vec::new(),
            scene_rects: Vec::new(),
            building_note: None,
            generator_marks: GeneratorMarks::default(),
            scenes: BTreeMap::new(),
            input: Input::default(),
            held: Held::default(),
            look: None,
            ctrl: false,
            shift: false,
            shift_clicked: false,
            q_down: false,
            shift_walking: false,
            later: std::collections::VecDeque::new(),
            last_time: None,
            panels: Panels {
                filter: String::new(),
                brush: Brush::Raise(1),
                radius: 24.0,
                on_ground: true,
                height: 0.0,
                weenie: None,
                weenie_filter: String::new(),
                weenie_category: None,
                form_typing: BTreeMap::new(),
                message: String::new(),
                category: None,
                tag: None,
                new_tag: String::new(),
                scene_terrain: 1,
                map_zoom: 1.0,
                advanced: false,
            },
            faults,
            stats: Stats::default(),
            work: Work::default(),
            cell_cache: std::cell::RefCell::new(BTreeMap::new()),
            live_args: LiveArgs {
                empyrean: args.empyrean.clone(),
                dir: args.live_dir.clone(),
                client: args.client.clone(),
                playtest: play::playtest_dir(&args.world),
            },
            play_options: play::read_play_options(&play::playtest_dir(&args.world).join("client")),
            text,
            starting: None,
            live: None,
            live_dirty: false,
            live_read: 0,
            playing: None,
            gdle: None,
            kit: Kit::default(),
        };
        app.rebuild_weenies();
        app.looks = app.spawn_looks();
        if args.live {
            app.set_live(true)?;
        }
        Ok(app)
    }

    // ---------------------------------------------------------------------------------------
    // the world as drawn
    // ---------------------------------------------------------------------------------------

    /// The block the drawn window is centered on.
    #[must_use]
    pub fn viewer_block(&self) -> Option<(i32, i32)> {
        self.core.world.as_ref()?.viewer_block()
    }

    /// This frame's projection.
    #[must_use]
    pub fn view(&self) -> Option<View> {
        let (w, h) = self.core.present.size();
        let block = self.viewer_block()?;
        let scene = self.core.present.scene(self.core.world.as_ref())?;
        Some(View::new(&scene.view_params(w, h), block))
    }

    /// Fly the camera for `dt` seconds with the keys held: through everything, or with
    /// [`Self::collide`] on, stopping at the ground, walls and things.
    fn fly(&mut self, dt: f32) {
        let from = self.camera.position;
        self.camera.fly(self.held, dt);
        if !self.collide || self.camera.position == from {
            return;
        }
        let to = self.camera.position;
        let Some(region) = self.session.region().cloned() else {
            return;
        };
        if self.collider.is_none() {
            let cfg = self.core.cfg.scene_config();
            match Collider::new(&self.core.store, &region, &cfg) {
                Ok(c) => self.collider = Some(c),
                Err(e) => {
                    self.panels.message = e;
                    self.collide = false;
                    return;
                }
            }
        }
        let indoor = self.camera_cell().map(|(c, _)| c);
        let Some((lb, ..)) = self.work_block() else {
            return;
        };
        let session = &self.session;
        let store = std::sync::Arc::clone(&self.core.store);
        let Some(c) = self.collider.as_mut() else {
            return;
        };
        c.reach(&store, &region, &|b| session.terrain(b), lb);
        self.camera.position = c.sweep(from, to, indoor);
    }

    /// The camera's eye, in world meters.
    #[must_use]
    pub fn eye(&self) -> Vec3 {
        let Some(view) = self.view() else {
            return self.camera.position;
        };
        let vp = view.viewport();
        #[allow(clippy::cast_precision_loss)]
        let r = view.ray(vp.width as f32 * 0.5, vp.height as f32 * 0.5);
        r.origin
    }

    /// The landblocks around the camera a pick looks in.
    fn near_blocks(&self) -> Vec<u16> {
        let Some((lb, _, _)) = self.work_block() else {
            return Vec::new();
        };
        let (bx, by) = block_xy(lb);
        let mut out = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (x, y) = (bx + dx, by + dy);
                if (0..0xFF).contains(&x) && (0..0xFF).contains(&y) {
                    out.push(u16::try_from((x << 8) | y).unwrap_or(0));
                }
            }
        }
        out
    }

    /// What the pointer at `(x, y)` is over.
    pub fn pick_at(&mut self, x: f32, y: f32, mask: Mask) -> Option<pick::Picked> {
        let ray = self.view()?.ray(x, y);
        let blocks = self.near_blocks();
        self.stats.picks += 1;
        pick::pick(&self.session, &mut self.models, &ray, &blocks, mask)
    }

    /// A static's frame in its landblock, as compiled.
    fn static_frame(&self, lb: u16, index: usize) -> Option<(DataId, Frame)> {
        let o = *self.session.statics(lb)?.objects.get(index)?;
        Some((o.id, o.frame))
    }

    /// The model and frame (landblock meters) of static `index` standing in indoor cell `cell`
    /// of landblock `lb`, as compiled.
    pub(crate) fn cell_static_frame(
        &self,
        lb: u16,
        cell: u16,
        index: usize,
    ) -> Option<(DataId, Frame)> {
        let cells = self.cells_of(lb);
        let o = cells.get(&cell)?.static_objects.get(index)?;
        Some((o.id, o.frame))
    }

    /// Landblock `lb`'s indoor cells as compiled, read once until the world changes.
    pub fn cells_of(&self, lb: u16) -> std::rc::Rc<BTreeMap<u16, EnvCell>> {
        std::rc::Rc::clone(
            self.cell_cache
                .borrow_mut()
                .entry(lb)
                .or_insert_with(|| std::rc::Rc::new(self.session.cells(lb))),
        )
    }

    // ---------------------------------------------------------------------------------------
    // edits and the reload
    // ---------------------------------------------------------------------------------------

    /// Do `edit`, then show what it reached.
    ///
    /// # Errors
    /// The session's refusal, or a reload that failed.
    pub fn edit(&mut self, edit: &Edit) -> Result<lyceum_foundry::edit::Applied, String> {
        let applied = self.session.act(edit).map_err(|e| e.to_string())?;
        self.after(&applied.touched)?;
        Ok(applied)
    }

    /// Apply `op` as a new step, or into the last one (a drag or a stroke going on).
    fn apply(&mut self, op: &Op, into_last: bool) -> Result<lyceum_foundry::edit::Applied, String> {
        let applied = if into_last {
            self.session.apply_into_last(op)
        } else {
            self.session.apply(op)
        }
        .map_err(|e| e.to_string())?;
        self.after(&applied.touched)?;
        Ok(applied)
    }

    /// Each spawn's landblock and the setup it is drawn with, by its file and guid.
    fn spawn_looks(&self) -> BTreeMap<(PathBuf, String), (u16, Option<DataId>)> {
        self.session
            .spawns()
            .into_iter()
            .map(|s| ((s.file.clone(), s.guid.clone()), (s.landblock(), s.setup)))
            .collect()
    }

    fn after(&mut self, touched: &Touched) -> Result<(), String> {
        self.stats.edits += 1;
        self.rooms = None;
        self.nearby.stale();
        self.live_dirty = true;
        if touched.sql {
            // A change of a weenie's SQL redraws the landblocks of the spawns it changes the look
            // of: their setups, or spawns come or gone.
            let now = self.spawn_looks();
            let mut more = touched.clone();
            for (k, v) in &now {
                if self.looks.get(k) != Some(v) {
                    more.landblocks.insert(v.0);
                }
            }
            for (k, v) in &self.looks {
                if !now.contains_key(k) {
                    more.landblocks.insert(v.0);
                }
            }
            self.rebuild_weenies();
            if !more.landblocks.is_empty() {
                return self.reload(&more);
            }
            // Nothing to draw again: the SQL is checked again all the same.
            if touched.landblocks.is_empty() {
                return self.recheck();
            }
        }
        if !touched.landblocks.is_empty() || touched.sql {
            self.reload(touched)?;
        }
        Ok(())
    }

    /// Bring the scratch overlay up to the edit (the records it reached, into the folder the
    /// store is not reading), lay the store over it, and build the landblocks `touched` names
    /// again; then check the world.
    ///
    /// # Errors
    /// An overlay that will not write or open.
    pub fn reload(&mut self, touched: &Touched) -> Result<(), String> {
        self.cell_cache.borrow_mut().clear();
        self.looks = self.spawn_looks();
        if let Some(map) = self.world_map.as_mut() {
            map.redraw(
                &self.session,
                touched.landblocks.iter().copied(),
                &self.terrain_colors,
            );
            if let Some(t) = self.pictures.0.get_mut(&MAP_PICTURE) {
                for lb in &touched.landblocks {
                    let (x, y) = lyceum_foundry::map::WorldMap::block_corner(*lb);
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied([8, 8], &map.block_pixels(*lb));
                    t.set_partial([x, y], image, egui::TextureOptions::LINEAR);
                }
            }
        }
        let written = self
            .scratch
            .update(&self.session, &touched.landblocks)
            .map_err(|e| e.to_string())?;
        if written.whole {
            self.stats.whole_writes += 1;
        } else {
            self.stats.records_written += (written.put + written.reverted) as u64;
        }
        let dir = written.dir;
        // The flying camera's world is built again over what the edit changed.
        if self
            .collider
            .as_ref()
            .is_some_and(|c| touched.landblocks.iter().any(|lb| c.blocks.contains(lb)))
        {
            self.collider = None;
        }
        self.core.cfg.overlay_dat_dir = Some(dir.clone());
        let fresh = Arc::new(self.core.reopen_store().map_err(|e| e.cause)?);
        self.core.adopt_store(Arc::clone(&fresh));
        let blocks: Vec<(i32, i32)> = touched.landblocks.iter().map(|lb| block_xy(*lb)).collect();

        let ids: Vec<DataId> = touched
            .landblocks
            .iter()
            .map(|lb| DataId((u32::from(*lb) << 16) | 0xFFFF))
            .collect();
        if let Some(c) = self.core.world.as_ref().and_then(|w| w.character.as_ref()) {
            c.land().resupply(Arc::clone(&fresh), &ids);
        }
        let present = &mut self.core.present;
        let world = &mut self.core.world;
        if let (Some(r), Some(ws)) = (
            present.as_any_mut().downcast_mut::<Renderer>(),
            world.as_mut(),
        ) {
            if let Some(draw) = r.world_mut() {
                self.stats.blocks_reread += draw.reread_blocks(ws, &blocks) as u64;
            }
        }
        self.stats.reloads += 1;
        self.recheck()
    }

    /// Check the world again, as the scratch overlay holds it now.
    fn recheck(&mut self) -> Result<(), String> {
        let Some(dir) = self.core.cfg.overlay_dat_dir.clone() else {
            return Ok(());
        };
        self.faults = self.session.check(&dir).map_err(|e| e.to_string())?;
        // A preview standing in a statics record past the compiled ones is not a static.
        let session = &self.session;
        self.faults.retain(|f| {
            let Some((lb, _)) = f.at.filter(|_| f.text.starts_with("0x")) else {
                return true;
            };
            let n = session.statics(lb).map_or(0, |i| i.objects.len());
            !f.text
                .split_once("static ")
                .and_then(|(_, r)| r.split_whitespace().next())
                .and_then(|i| i.parse::<usize>().ok())
                .is_some_and(|i| i >= n)
        });
        Ok(())
    }

    /// Write the frame drawn last as a PNG.
    ///
    /// # Errors
    /// The device's.
    pub fn capture(&mut self, path: &Path) -> Result<(), String> {
        self.core
            .present
            .capture_png(path)
            .map_err(|e| e.to_string())
    }

    // ---------------------------------------------------------------------------------------
    // the frame
    // ---------------------------------------------------------------------------------------

    /// One frame: the editor's step, then the runtime's. False when the window closed.
    pub fn frame(&mut self) -> bool {
        let started = std::time::Instant::now();
        let (cells, picks, draws) = (
            self.session.cells_read(),
            self.stats.picks,
            self.draw_calls(),
        );
        self.update();
        let ui = started.elapsed();
        self.stats.frames += 1;
        let going = self.core.frame(&mut self.shell);
        self.work = Work {
            ui_ms: ui.as_secs_f32() * 1000.0,
            world_ms: (started.elapsed() - ui).as_secs_f32() * 1000.0,
            overlay_vertices: self.work.overlay_vertices,
            draw_calls: self.draw_calls() - draws,
            cells_read: self.session.cells_read() - cells,
            picks: self.stats.picks - picks,
        };
        going
    }

    /// The device's draws so far.
    fn draw_calls(&mut self) -> u64 {
        self.core
            .present
            .as_any_mut()
            .downcast_mut::<Renderer>()
            .map_or(0, |r| r.world_mut_and_gpu().1.draw_calls())
    }

    /// Run the main loop until the window closes.
    pub fn run(&mut self) {
        while self.frame() {}
    }

    /// Read this frame's device events for the world: the pointer and its buttons (unless the
    /// panels took them: `over`), the held keys and the shortcuts (unless a field takes the typing:
    /// `typing`). Answers each key pressed, with whether Ctrl was held as it went down.
    fn read_input(&mut self, events: Vec<HostEvent>, over: bool, typing: bool) -> Vec<(u32, bool)> {
        let mut keys = Vec::new();
        let last = self.input.pointer;
        let down = self.input.down;
        self.input = Input {
            pointer: last,
            down,
            ..Input::default()
        };
        for e in events {
            match e {
                HostEvent::CursorMoved { x, y } => {
                    #[allow(clippy::cast_possible_truncation)]
                    let p = (x as f32, y as f32);
                    if let Some(from) = self.look {
                        self.camera.look(p.0 - from.0, p.1 - from.1);
                        self.look = Some(p);
                    }
                    self.input.pointer = p;
                }
                HostEvent::PointerMotion { dx, dy } => {
                    #[allow(clippy::cast_possible_truncation)]
                    self.camera.look(dx as f32, dy as f32);
                }
                HostEvent::MouseInput { button, pressed } => match button {
                    MouseButton::Left => {
                        // A press the panels took is theirs; a release always ends what the
                        // world was doing.
                        if pressed && over {
                            continue;
                        }
                        if pressed && self.shift {
                            self.shift_clicked = true;
                        }
                        self.input.down = pressed;
                        if pressed {
                            self.input.pressed = true;
                        } else {
                            self.input.released = true;
                        }
                    }
                    MouseButton::Right => {
                        self.look = (pressed && !over).then_some(self.input.pointer);
                    }
                    _ => {}
                },
                HostEvent::MouseWheel { notches } if !over => self.input.wheel += notches,
                HostEvent::KeyboardInput { key, pressed, .. } => {
                    let vk = u32::try_from(key.virtual_key).unwrap_or(0);
                    match vk {
                        0x11 | 0xA2 | 0xA3 => self.ctrl = pressed,
                        0x10 | 0xA0 | 0xA1 => {
                            // Shift lowers the flying camera, and walks the body (the run key
                            // held); a click with it held adds to the selection instead, and
                            // stops the camera for the rest of the hold.
                            self.shift = pressed;
                            if !pressed {
                                self.shift_clicked = false;
                            }
                            if self.walking && pressed != self.shift_walking {
                                self.shift_walking = pressed;
                                self.core.inject_action(if pressed {
                                    Action::begin(movement::TOGGLE_RUN_WALK)
                                } else {
                                    Action::end(movement::TOGGLE_RUN_WALK)
                                });
                            }
                        }
                        _ => {}
                    }
                    // Keys typed into a field are the field's; a key let go always lets go.
                    if typing && pressed {
                        continue;
                    }
                    let set = |h: &mut bool| *h = pressed;
                    match vk {
                        0x57 => set(&mut self.held.forward),
                        0x53 if !self.ctrl => set(&mut self.held.back),
                        0x41 => set(&mut self.held.left),
                        0x44 => set(&mut self.held.right),
                        // Space rises as E does; walking, it charges a jump (below).
                        0x45 | 0x20 => set(&mut self.held.up),
                        0x51 => self.q_down = pressed,
                        _ => {}
                    }
                    if self.walking {
                        let id = match vk {
                            0x57 => Some(movement::MOVE_FORWARD),
                            0x53 => Some(movement::MOVE_BACKWARD),
                            0x41 => Some(movement::TURN_LEFT),
                            0x44 => Some(movement::TURN_RIGHT),
                            0x51 => Some(movement::STRAFE_LEFT),
                            0x45 => Some(movement::STRAFE_RIGHT),
                            0x20 => Some(movement::JUMP),
                            _ => None,
                        };
                        if let Some(id) = id {
                            self.core.inject_action(if pressed {
                                Action::begin(id)
                            } else {
                                Action::end(id)
                            });
                        }
                    }
                    if pressed {
                        keys.push((vk, self.ctrl));
                    }
                }
                _ => {}
            }
        }
        self.held.down = self.q_down || self.shift && !self.shift_clicked;
        keys
    }

    /// The editor's step of one frame, before the runtime's.
    fn update(&mut self) {
        // The creator's text, read again when they save it.
        self.text.refresh();
        self.nearby_frame();
        if let Err(e) = self.take_catalog(false) {
            self.panels.message = e;
        }
        // The server's own spawns around the camera, as it goes.
        let shown = self.show_server_spawns_around();
        if !shown.is_empty() {
            if let Err(e) = self.reload(&Touched {
                landblocks: shown,
                sql: false,
            }) {
                self.panels.message = e;
            }
        }
        if let Some(batch) = self.later.pop_front() {
            self.shell.inbox.extend(batch);
        }
        let events = std::mem::take(&mut self.shell.inbox);
        let now = self.core.timer.local_time;
        // The panels first: what they take, the world does not.
        let (w, h) = self.core.present.size();
        let raw = self.gui.input(&events, (w, h), now);
        let ctx = self.gui.ctx.clone();
        let mut pass = panels::Pass::default();
        let out = ctx.run_ui(raw, |ui| pass = self.panels_ui(ui));
        self.world_rect = pass.world;
        let over = ctx.egui_is_using_pointer()
            || ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
                !pass.world.is_some_and(|r| r.contains(p))
                    || ctx
                        .layer_id_at(p)
                        .is_some_and(|l| l.order != egui::Order::Background)
            });
        let typing = ctx.egui_wants_keyboard_input();
        self.shell.items = self.gui.paint(out, (w, h));
        self.work.overlay_vertices = self
            .shell
            .items
            .iter()
            .map(|i| match i {
                dereth_client_contract::overlay::OverlayItem::Triangles { vertices, .. } => {
                    vertices.len()
                }
                _ => 0,
            })
            .sum();
        self.shell.uploads.append(&mut self.gui.uploads);
        let keys = self.read_input(events, over, typing);
        #[allow(clippy::cast_possible_truncation)]
        let dt = self
            .last_time
            .map_or(0.0, |t| (now - t) as f32)
            .clamp(0.0, 0.25);
        self.last_time = Some(now);
        let mut actions = Vec::new();
        for (vk, ctrl) in keys {
            match vk {
                0x5A if ctrl => actions.push(UiAction::Undo),
                0x59 if ctrl => actions.push(UiAction::Redo),
                0x53 if ctrl => actions.push(UiAction::Save),
                0x2E => actions.push(UiAction::Delete),
                0x1B => actions.push(UiAction::Deselect),
                0x46 => actions.push(UiAction::ToggleWalk),
                0x47 => actions.push(UiAction::Snap),
                0x31 => actions.push(UiAction::Mode(Mode::Select)),
                0x32 => actions.push(UiAction::Mode(Mode::Place)),
                0x33 => actions.push(UiAction::Mode(Mode::Terrain)),
                0x34 => actions.push(UiAction::Mode(Mode::Spawn)),
                0x35 => actions.push(UiAction::Mode(Mode::Dungeon)),
                _ => {}
            }
        }
        // The camera.
        if !self.walking {
            self.fly(dt);
        }
        actions.append(&mut pass.actions);
        for a in actions {
            self.ui_action(a);
        }
        if !over && self.look.is_none() {
            if self.input.wheel != 0.0 {
                #[allow(clippy::cast_possible_truncation)]
                self.camera.change_speed(self.input.wheel.signum() as i32);
                self.walk_speed();
            }
            self.world_tool();
        } else if self.drag.is_some() || self.box_from.is_some() {
            // A drag or a box begun in the world goes on, and ends, over the panels too.
            self.world_tool();
        }
        if self.input.released {
            self.drag = None;
            self.stroke = false;
            self.box_from = None;
        }
        if !self.walking {
            let cell = self.camera_cell();
            if let (Some(block), Some(ws)) = (self.viewer_block(), self.core.world.as_mut()) {
                ws.camera = self.camera.to_free(block);
                // In a room, the world draws it from inside, as from a body's camera there.
                ws.free_camera_cell = cell;
            }
        }
        self.live_frame();
    }

    /// Answer one action of the panels or the keys.
    fn ui_action(&mut self, a: UiAction) {
        let result = match a {
            UiAction::Mode(m) => {
                self.mode = m;
                Ok(())
            }
            UiAction::Undo => self.edit(&Edit::Undo).map(|_| ()),
            UiAction::Redo => self.edit(&Edit::Redo).map(|_| ()),
            UiAction::Piece(i) => {
                self.kit.chosen = Some(i);
                Ok(())
            }
            UiAction::FindPiece(env, piece) => self.find_piece(env, piece),
            UiAction::ChooseCell(lb, n) => self.choose_cell(lb, n),
            UiAction::ReplaceCell(lb, n) => self.replace_cell(lb, n),
            UiAction::Step(s) => {
                self.kit.step = s;
                Ok(())
            }
            UiAction::RemoveCell(lb, name) => self
                .apply(
                    &Op::RemoveCell {
                        landblock: lb,
                        name,
                    },
                    false,
                )
                .map(|_| ()),
            UiAction::Live(on) => self.set_live(on),
            UiAction::Play => self.play(&[], false).map(|_| ()),
            UiAction::Nearby(i) => self.nearby_go(i),
            UiAction::NewFor(flow, name) => self.new_for(flow, &name).map(|_| ()),
            UiAction::PlaceHere(w) => self.place_one_here(&w),
            UiAction::DestinationHere => self.destination_here(),
            UiAction::DestinationDungeon(lb) => self.destination_dungeon(lb),
            UiAction::GoToSpawn(file, guid) => {
                if let Some(s) = self
                    .session
                    .spawns()
                    .into_iter()
                    .find(|s| s.file == file && s.guid == guid)
                {
                    self.selection = Some(Selection::Spawn { file, guid });
                    self.also.clear();
                    let yaw = self.camera.yaw.to_degrees();
                    self.look_at(s.landblock(), s.frame.origin, 12.0, yaw, -30.0);
                }
                Ok(())
            }
            UiAction::EnterDungeon(lb, edit) => self.enter_dungeon(lb, edit),
            UiAction::NewDungeon => self.new_dungeon().map(|_| ()),
            UiAction::ShowTab(tab) => {
                self.show_tab(tab);
                Ok(())
            }
            UiAction::PortalToDungeon(lb) => self.portal_to_dungeon(lb),
            UiAction::PlayOptions(o) => {
                self.play_options = o;
                play::write_play_options(&self.live_args.playtest.join("client"), o)
            }
            UiAction::ExportGdle => self.export_gdle(false),
            UiAction::Save => match self.session.save() {
                Ok(w) => {
                    self.panels.message = self.say_with("messages.saved", &[("n", &w.len())]);
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            },
            UiAction::Delete => self.delete_selected(),
            UiAction::Deselect => {
                self.selection = None;
                self.also.clear();
                // In the dungeon tool, the chosen piece is let go too: a click selects again.
                if self.mode == Mode::Dungeon {
                    self.kit.chosen = None;
                }
                Ok(())
            }
            UiAction::ToggleWalk => self.set_walking(!self.walking),
            UiAction::Collide => {
                self.collide = !self.collide;
                Ok(())
            }
            UiAction::Snap => {
                if let Some((lb, x, y)) = landblock_of(self.camera.position) {
                    if let Some(g) = self.session.ground(lb, x, y) {
                        self.camera.snap(g);
                    }
                }
                Ok(())
            }
            UiAction::Speed(d) => {
                self.camera.change_speed(d);
                self.walk_speed();
                Ok(())
            }
            UiAction::Model(m) => {
                self.model = Some(m);
                self.mode = Mode::Place;
                Ok(())
            }
            UiAction::Brush(b) => {
                self.panels.radius =
                    lyceum_foundry::brush::chosen_radius(self.panels.brush, b, self.panels.radius);
                self.panels.brush = b;
                self.mode = Mode::Terrain;
                Ok(())
            }
            UiAction::Radius(steps) => {
                self.panels.radius = lyceum_foundry::brush::step_radius(
                    self.panels.brush,
                    self.panels.radius,
                    steps,
                );
                Ok(())
            }
            UiAction::Amount(d) => {
                self.panels.brush = step_brush(self.panels.brush, d);
                Ok(())
            }
            UiAction::OnGround(on) => {
                self.panels.on_ground = on;
                Ok(())
            }
            UiAction::Category(c) => {
                self.panels.category = c;
                Ok(())
            }
            UiAction::Tag(t) => {
                self.panels.tag = t;
                Ok(())
            }
            UiAction::TagModel(m, t, on) => self.tag_model(m, &t, on),
            UiAction::Form(e) => self.form_edit(&e),
            UiAction::ShowWeenie => {
                self.show_weenie();
                Ok(())
            }
            UiAction::CopyClass(c) => self.copy_class(c),
            UiAction::NewWeenie(name, from, blank) => {
                self.new_weenie(&name, from, blank).map(|()| {
                    self.panels.form_typing.remove("new:name");
                })
            }
            UiAction::WeenieCategory(c) => {
                self.panels.weenie_category = c;
                Ok(())
            }
            UiAction::FlyTo(x, y) => self.fly_to(x, y),
            UiAction::MapZoom(f) => {
                self.panels.map_zoom = (self.panels.map_zoom * f).clamp(1.0, 64.0);
                Ok(())
            }
            UiAction::Weenie(wn) => {
                self.panels.weenie = Some(wn);
                self.mode = Mode::Spawn;
                // The form shows the spawn tool's weenie, not a spawn that happens to be chosen.
                if matches!(self.selection, Some(Selection::Spawn { .. })) {
                    self.selection = None;
                    self.also.clear();
                }
                Ok(())
            }
            UiAction::Vertex(field, d) => self.step_vertex(field, d),
            UiAction::SnapStatic => self.snap_selected(),
            UiAction::GoTo(lb, at) => {
                self.look_at(lb, at, 40.0, self.camera.yaw.to_degrees(), -35.0);
                Ok(())
            }
        };
        if let Err(e) = result {
            self.panels.message = e;
        }
    }

    /// The indoor cells the drawn world holds, and how many meshes the body's cell is drawn
    /// with (none while the camera flies or the body is outdoors).
    pub fn cells_drawn(&mut self) -> (BTreeSet<u32>, usize) {
        let r = self.core.present.as_any_mut().downcast_mut::<Renderer>();
        match (
            r.and_then(|r| r.world_mut_and_gpu().0),
            self.core.world.as_ref(),
        ) {
            (Some(draw), Some(ws)) => (
                draw.interior_looks().keys().map(|c| c.0).collect(),
                draw.env_cell_counts(ws).1,
            ),
            _ => (BTreeSet::new(), 0),
        }
    }

    /// A message of the editor's, in its text.
    pub(crate) fn say(&self, key: &str) -> String {
        self.text.table().get(key).to_owned()
    }

    /// A message of the editor's, in its text, with what it names filled in.
    pub(crate) fn say_with(&self, key: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
        self.text.table().fill(key, values)
    }

    /// The indoor cells the last frame drew, each with the screen area (pixels) it was drawn
    /// through: from outdoors its building's doorways, from inside the room around the camera.
    pub fn cells_seen(&mut self) -> BTreeMap<u32, f32> {
        let Some(r) = self.core.present.as_any_mut().downcast_mut::<Renderer>() else {
            return BTreeMap::new();
        };
        let Some(draw) = r.world_mut_and_gpu().0 else {
            return BTreeMap::new();
        };
        let area = |poly: &[(f32, f32)]| {
            let n = poly.len();
            (0..n)
                .map(|i| {
                    let (a, b) = (poly[i], poly[(i + 1) % n]);
                    a.0 * b.1 - b.0 * a.1
                })
                .sum::<f32>()
                .abs()
                / 2.0
        };
        draw.drawn_cells()
            .unwrap_or_default()
            .into_iter()
            .map(|c| {
                let seen = draw
                    .cell_view_polys(dereth_primitives::CellId(c))
                    .iter()
                    .map(|p| area(p))
                    .sum();
                (c, seen)
            })
            .collect()
    }

    /// The indoor cell the flying camera is in, and whether it is seen from outside; `None`
    /// outdoors.
    pub fn camera_cell(&mut self) -> Option<(dereth_primitives::CellId, bool)> {
        let mut blocks = Vec::new();
        if let Some(d) = self.dungeon_block {
            blocks.push(d);
        }
        if let Some((lb, ..)) = landblock_of(self.camera.position) {
            blocks.push(lb);
        }
        for (k, lb) in blocks.into_iter().enumerate() {
            if self.rooms.as_ref().is_none_or(|(at, _)| *at != lb) {
                self.rooms = Some((lb, self.session.cell_shapes(lb)));
            }
            let o = pick::block_origin(lb);
            let p = self.camera.position;
            let local = Vec3::new(p.x - o.x, p.y - o.y, p.z - o.z);
            let room = self
                .rooms
                .as_ref()
                .and_then(|(_, rooms)| rooms.iter().find(|r| r.holds(local)))
                .map(|r| (r.index, r.seen_outside));
            if k == 0 && self.dungeon_block == Some(lb) {
                self.in_dungeon = room.is_some();
            }
            if let Some((index, seen)) = room {
                return Some((
                    dereth_primitives::CellId((u32::from(lb) << 16) | u32::from(index)),
                    seen,
                ));
            }
        }
        None
    }

    /// The cell the walking body is in.
    #[must_use]
    pub fn body_cell(&self) -> Option<u32> {
        self.core
            .world
            .as_ref()
            .and_then(|ws| ws.character.as_ref())
            .map(|c| c.position().cell.0)
    }

    /// Fly or walk.
    ///
    /// # Errors
    /// The body will not build.
    pub fn set_walking(&mut self, walk: bool) -> Result<(), String> {
        if walk == self.walking {
            return Ok(());
        }
        if walk {
            let Some((lb, x, y)) = self.work_block() else {
                return Err(self.say("messages.off_world"));
            };
            // In the cell the camera is in, on its floor; else on the ground under it.
            let local_z = self.camera.position.z - lyceum_foundry::pick::block_origin(lb).z;
            let (cell, z) = match self.session.cell_at(lb, Vec3::new(x, y, local_z)) {
                Some((c, floor)) => ((u32::from(lb) << 16) | u32::from(c), floor),
                None => (
                    (u32::from(lb) << 16) | lyceum_foundry::edit::outdoor_cell(x, y),
                    self.session
                        .ground(lb, x, y)
                        .unwrap_or(self.camera.position.z),
                ),
            };
            let region = self
                .session
                .region()
                .cloned()
                .ok_or("the world has no region")?;
            let store = Arc::clone(&self.core.store);
            let present = &mut self.core.present;
            let world = &mut self.core.world;
            let (Some(r), Some(ws)) = (
                present.as_any_mut().downcast_mut::<Renderer>(),
                world.as_mut(),
            ) else {
                return Err("no world is drawn".into());
            };
            let (draw, gpu) = r.world_mut_and_gpu();
            let draw = draw.ok_or("no world is drawn")?;
            draw.attach_character(ws, &store, &region, gpu)
                .map_err(|e| e.to_string())?;
            if let Some(c) = ws.character.as_mut() {
                c.teleport(Position::new(
                    dereth_primitives::CellId(cell),
                    Frame::new(
                        Vec3::new(x, y, z),
                        lyceum_foundry::compile::heading_quat(-self.camera.yaw.to_degrees()),
                    ),
                ));
            }
            self.walking = true;
            self.walk_speed();
            // The run word set from the run-by-default option, as taking control does: the body
            // runs, and Shift held walks it.
            self.core
                .inject_action(Action::end(movement::TOGGLE_RUN_WALK));
        } else {
            self.held = Held::default();
            if self.shift_walking {
                self.shift_walking = false;
                self.core
                    .inject_action(Action::end(movement::TOGGLE_RUN_WALK));
            }
            let eye = self.eye();
            if let Some(ws) = self.core.world.as_mut() {
                ws.character = None;
            }
            self.camera.position = eye;
            self.walking = false;
        }
        Ok(())
    }

    /// Give the walking body the speed the camera flies at, as its run rate, and a Jump skill,
    /// as the editor's body has no character behind it.
    fn walk_speed(&self) {
        if let Some(c) = self.core.world.as_ref().and_then(|w| w.character.as_ref()) {
            c.hold_run_rate(Some(self.camera.run_rate()));
            c.hold_jump_skill(Some(JUMP_SKILL));
        }
    }

    /// Put the camera `distance` meters from `at` (landblock meters of `lb`), looking at it along
    /// `yaw` and `pitch` (degrees).
    pub fn look_at(&mut self, lb: u16, at: Vec3, distance: f32, yaw: f32, pitch: f32) {
        let target = add(pick::block_origin(lb), at);
        let mut c = FlyCamera::new(target, yaw.to_radians(), pitch.to_radians());
        c.speed = self.camera.speed;
        let f = c.forward();
        c.position = Vec3::new(
            target.x - f.x * distance,
            target.y - f.y * distance,
            target.z - f.z * distance,
        );
        self.camera = c;
    }

    /// Take every selected static and spawn away, as one step. A pack's own statics in one file
    /// go from the last entry back, so each names its entry as it still is.
    fn delete_selected(&mut self) -> Result<(), String> {
        let all: Vec<Selection> = self.selection.iter().chain(&self.also).cloned().collect();
        let mut statics: Vec<(u16, StaticOrigin)> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        let mut server: Vec<(PathBuf, String)> = Vec::new();
        let mut in_cells: Vec<(u16, u16, usize)> = Vec::new();
        for s in all {
            match s {
                Selection::CellStatic {
                    landblock,
                    cell,
                    index,
                } => in_cells.push((landblock, cell, index)),
                Selection::Static { landblock, index } => {
                    let which = self
                        .session
                        .origins(landblock)
                        .get(index)
                        .cloned()
                        .ok_or_else(|| self.say("messages.static_gone"))?;
                    statics.push((landblock, which));
                }
                Selection::Spawn { file, guid }
                    if self.session.override_landblock(&file).is_some() =>
                {
                    server.push((file, guid));
                }
                Selection::Spawn { file, .. } if !files.contains(&file) => files.push(file),
                _ => {}
            }
        }
        let entry = |w: &StaticOrigin| match w {
            StaticOrigin::Pack { entry, .. } => *entry,
            StaticOrigin::Base(_) => 0,
        };
        statics.sort_by_key(|(lb, w)| (*lb, std::cmp::Reverse(entry(w))));
        let mut ops: Vec<Op> = statics
            .into_iter()
            .map(|(landblock, which)| Op::RemoveStatic { landblock, which })
            .collect();
        // A cell's from its last: each one's place is the same after the ones past it go.
        in_cells.sort_by_key(|(lb, cell, index)| (*lb, *cell, std::cmp::Reverse(*index)));
        ops.extend(
            in_cells
                .into_iter()
                .map(|(landblock, cell, index)| Op::RemoveCellStatic {
                    landblock,
                    cell,
                    index,
                }),
        );
        ops.extend(files.into_iter().map(|file| Op::RemoveSpawn { file }));
        ops.extend(
            server
                .into_iter()
                .map(|(file, guid)| Op::RemoveServerSpawn { file, guid }),
        );
        if ops.is_empty() {
            return Ok(());
        }
        let op = if ops.len() == 1 {
            ops.remove(0)
        } else {
            Op::Batch(ops)
        };
        self.apply(&op, false)?;
        self.selection = None;
        self.also.clear();
        Ok(())
    }

    fn snap_selected(&mut self) -> Result<(), String> {
        let Some(Selection::Static { landblock, index }) = self.selection.clone() else {
            return Ok(());
        };
        let (_, f) = self
            .static_frame(landblock, index)
            .ok_or_else(|| self.say("messages.static_gone"))?;
        let which = self
            .session
            .origins(landblock)
            .get(index)
            .cloned()
            .ok_or_else(|| self.say("messages.static_gone"))?;
        self.apply(
            &Op::MoveStatic {
                landblock,
                which,
                at: Some(Place {
                    x: f.origin.x,
                    y: f.origin.y,
                    z: Height::Ground,
                }),
                rotation: None,
            },
            false,
        )?;
        Ok(())
    }

    fn step_vertex(&mut self, field: Field, d: i32) -> Result<(), String> {
        let Some(Selection::Vertex { gx, gy }) = self.selection else {
            return Ok(());
        };
        let v = self
            .session
            .vertex(gx, gy)
            .ok_or_else(|| self.say("messages.vertex_gone"))?;
        let (now, max) = match field {
            Field::Height => (i32::from(v.height), 255),
            Field::Terrain => (i32::from(v.terrain), 31),
            Field::Scene => (i32::from(v.scene), 31),
            Field::Road => (i32::from(v.road), 3),
        };
        let value = u16::try_from((now + d).clamp(0, max)).unwrap_or(0);
        if i32::from(value) == now {
            return Ok(());
        }
        self.apply(
            &Op::SetVertices(vec![VertexSet {
                gx,
                gy,
                field,
                value,
            }]),
            false,
        )?;
        Ok(())
    }

    /// The tool of the mode, on the world under the pointer.
    fn world_tool(&mut self) {
        let p = self.input.pointer;
        let result = match self.mode {
            Mode::Select => self.select_tool(p),
            Mode::Place => {
                if self.input.pressed {
                    self.place_at(p)
                } else {
                    Ok(())
                }
            }
            Mode::Terrain => {
                if self.input.down {
                    self.brush_at(p)
                } else {
                    Ok(())
                }
            }
            Mode::Spawn => {
                if self.input.pressed {
                    self.spawn_at(p)
                } else {
                    Ok(())
                }
            }
            // With a piece chosen a click places it; with none, it selects what is under it (a
            // cell, and so its piece).
            Mode::Dungeon if self.kit.chosen.is_none() => self.select_tool(p),
            Mode::Dungeon => {
                if self.input.pressed {
                    self.piece_at(p)
                } else {
                    Ok(())
                }
            }
        };
        if let Err(e) = result {
            self.panels.message = e;
        }
    }

    /// The handles of the selected static or spawn, as drawn now.
    #[must_use]
    pub fn handles(&self) -> Option<gizmo::Handles> {
        self.selected_handles().map(|(h, ..)| h)
    }

    /// The selected spawn, as the session reads it now.
    fn selected_spawn(&self) -> Option<lyceum_foundry::edit::Spawn> {
        let Some(Selection::Spawn { file, guid }) = &self.selection else {
            return None;
        };
        self.session
            .spawns()
            .into_iter()
            .find(|s| s.file == *file && s.guid == *guid)
    }

    /// The handles of the selected static or spawn as drawn now, its landblock, its frame there,
    /// and what a drag on them moves.
    fn selected_handles(&self) -> Option<(gizmo::Handles, u16, Frame, Dragged)> {
        let (landblock, f, what) = match self.selection.clone()? {
            Selection::Static { landblock, index } => {
                let (_, f) = self.static_frame(landblock, index)?;
                let which = self.session.origins(landblock).get(index).cloned()?;
                (landblock, f, Dragged::Static { which })
            }
            Selection::Spawn { file, guid } => {
                let s = self.selected_spawn()?;
                (s.landblock(), s.frame, Dragged::Spawn { file, guid })
            }
            Selection::Building { landblock, index } => {
                let b = self
                    .session
                    .statics(landblock)?
                    .buildings
                    .get(index)?
                    .clone();
                (landblock, b.frame, Dragged::Building { index })
            }
            Selection::CellStatic {
                landblock,
                cell,
                index,
            } => {
                let (_, f) = self.cell_static_frame(landblock, cell, index)?;
                (landblock, f, Dragged::CellStatic { cell, index })
            }
            Selection::Vertex { .. } | Selection::Cell { .. } => return None,
        };
        let view = self.view()?;
        let origin = add(pick::block_origin(landblock), f.origin);
        let h = gizmo::handles(&view, self.eye(), origin)?;
        Some((h, landblock, f, what))
    }

    fn select_tool(&mut self, p: (f32, f32)) -> Result<(), String> {
        if self.input.pressed {
            if let Some((h, landblock, f, what)) = self.selected_handles() {
                if let Some(handle) = gizmo::handle_at(&h, p) {
                    let from = self.view().ok_or("no view")?.ray(p.0, p.1);
                    let group = self.group_frames();
                    self.drag = Some(Drag {
                        handle,
                        landblock,
                        what,
                        from,
                        start: f,
                        stepped: false,
                        group,
                    });
                    return Ok(());
                }
            }
            // A click or a box: which, when the button comes up.
            self.box_from = Some(p);
            return Ok(());
        }
        if self.input.released {
            if let Some(from) = self.box_from.take() {
                let add = self.ctrl || self.shift;
                let (dx, dy) = (p.0 - from.0, p.1 - from.1);
                if dx * dx + dy * dy < BOX_MIN * BOX_MIN {
                    self.click_select(p, add);
                } else {
                    self.box_select(from, p, add);
                }
            }
            return Ok(());
        }
        let Some(drag) = self.drag.clone() else {
            return Ok(());
        };
        if !self.input.down {
            return Ok(());
        }
        let to = self.view().ok_or("no view")?.ray(p.0, p.1);
        let pivot = add(pick::block_origin(drag.landblock), drag.start.origin);
        // What the drag does to the primary, as a move (world meters) or a turn (degrees): every
        // other selected static and spawn takes the same.
        let change = match drag.handle {
            Handle::Turn => {
                let Some(d) = gizmo::drag_turn(pivot, &drag.from, &to) else {
                    return Ok(());
                };
                Change::Turn(d)
            }
            axis => {
                let Some(d) = gizmo::drag_along(axis, pivot, &drag.from, &to) else {
                    return Ok(());
                };
                let dir = axis.axis().unwrap_or(Vec3::ZERO);
                Change::Move(
                    Vec3::new(dir.x * d, dir.y * d, dir.z * d),
                    axis == Handle::Z,
                )
            }
        };
        let mut ops = Vec::new();
        let members = std::iter::once((drag.landblock, drag.what.clone(), drag.start))
            .chain(drag.group.iter().cloned());
        for (lb, what, start) in members {
            if let Some(op) = self.moved(lb, &what, &start, pivot, change) {
                ops.push(op);
            }
        }
        if ops.is_empty() {
            return Ok(());
        }
        let op = if ops.len() == 1 {
            ops.remove(0)
        } else {
            Op::Batch(ops)
        };
        self.apply(&op, drag.stepped)?;
        if let Some(d) = self.drag.as_mut() {
            d.stepped = true;
        }
        Ok(())
    }

    /// The operation that moves or turns `what` (whose frame was `start` in landblock `lb` when
    /// the drag began) by `change`, a turn going about `pivot` (world meters). `None` when it would
    /// leave its landblock.
    fn moved(
        &self,
        lb: u16,
        what: &Dragged,
        start: &Frame,
        pivot: Vec3,
        change: Change,
    ) -> Option<Op> {
        let o = start.origin;
        let base = pick::block_origin(lb);
        let heading = gizmo::heading_of(start.rotation);
        let ground_now = self.session.ground(lb, o.x, o.y);
        let on_ground = ground_now.is_some_and(|g| (o.z - g).abs() < 0.01);
        let (moved, turn, upright) = match change {
            Change::Move(d, up) => (Vec3::new(o.x + d.x, o.y + d.y, o.z + d.z), 0.0, up),
            Change::Turn(deg) => {
                // About the pivot, clockwise as a heading turns.
                let (s, c) = (
                    dereth_primitives::num::math::sinf(deg.to_radians()),
                    dereth_primitives::num::math::cosf(deg.to_radians()),
                );
                let (rx, ry) = (base.x + o.x - pivot.x, base.y + o.y - pivot.y);
                // The one turned about itself stays exactly where it is.
                let at = if rx.abs() < 1e-3 && ry.abs() < 1e-3 {
                    o
                } else {
                    let (x, y) = (rx * c + ry * s, -rx * s + ry * c);
                    Vec3::new(pivot.x + x - base.x, pivot.y + y - base.y, o.z)
                };
                (at, deg, false)
            }
        };
        // What stands in an indoor cell keeps its cell, which may lie outside its landblock's
        // square; what stands outdoors stays in its landblock.
        let indoor = match what {
            Dragged::CellStatic { .. } => true,
            Dragged::Spawn { file, guid } => self
                .session
                .spawns()
                .iter()
                .find(|s| s.file == *file && s.guid == *guid)
                .is_some_and(|s| s.cell & 0xFFFF >= u32::from(lyceum_foundry::cells::FIRST_CELL)),
            _ => false,
        };
        if !indoor && (!(0.0..192.0).contains(&moved.x) || !(0.0..192.0).contains(&moved.y)) {
            return None;
        }
        // One that stood on the ground goes over it; one up or down keeps its height.
        let z = if upright || !on_ground || indoor {
            Height::At(moved.z)
        } else {
            Height::Ground
        };
        let h = (heading + turn + 360.0) % 360.0;
        let turned = matches!(change, Change::Turn(_));
        Some(match what {
            Dragged::Static { which } => Op::MoveStatic {
                landblock: lb,
                which: which.clone(),
                at: (moved != o).then_some(Place {
                    x: moved.x,
                    y: moved.y,
                    z,
                }),
                rotation: turned.then_some(Rotation::Heading(h)),
            },
            // The building, and everything that goes with it, by the same move.
            Dragged::Building { index } => {
                let to_z = match z {
                    Height::At(z) => z,
                    Height::Ground => self.session.ground(lb, moved.x, moved.y).unwrap_or(o.z),
                };
                let rotation = if turned {
                    Rotation::Heading(h)
                } else {
                    let q = start.rotation;
                    Rotation::Quat([q.w, q.x, q.y, q.z])
                };
                self.session
                    .move_building(lb, *index, [moved.x, moved.y, to_z], rotation)?
            }
            Dragged::CellStatic { cell, index } => Op::MoveCellStatic {
                landblock: lb,
                cell: *cell,
                index: *index,
                at: [moved.x, moved.y, moved.z],
                rotation: turned.then_some(Rotation::Heading(h)),
            },
            // A spawn's SQL holds its height, never "on the ground": the ground's is taken now.
            Dragged::Spawn { file, guid } => Op::MoveSpawn {
                file: file.clone(),
                guid: guid.clone(),
                origin: Vec3::new(
                    moved.x,
                    moved.y,
                    match z {
                        Height::At(z) => z,
                        Height::Ground => self.session.ground(lb, moved.x, moved.y).unwrap_or(o.z),
                    },
                ),
                heading: h,
            },
        })
    }

    /// Every selected static and spawn but the primary, with its landblock and its frame now.
    fn group_frames(&self) -> Vec<(u16, Dragged, Frame)> {
        self.also.iter().filter_map(|s| self.member(s)).collect()
    }

    /// A selected static's or spawn's landblock, what a drag moves, and its frame now.
    fn member(&self, s: &Selection) -> Option<(u16, Dragged, Frame)> {
        match s {
            Selection::Static { landblock, index } => {
                let (_, f) = self.static_frame(*landblock, *index)?;
                let which = self.session.origins(*landblock).get(*index).cloned()?;
                Some((*landblock, Dragged::Static { which }, f))
            }
            Selection::Spawn { file, guid } => {
                let sp = self
                    .session
                    .spawns()
                    .into_iter()
                    .find(|x| x.file == *file && x.guid == *guid)?;
                Some((
                    sp.landblock(),
                    Dragged::Spawn {
                        file: file.clone(),
                        guid: guid.clone(),
                    },
                    sp.frame,
                ))
            }
            Selection::CellStatic {
                landblock,
                cell,
                index,
            } => {
                let (_, f) = self.cell_static_frame(*landblock, *cell, *index)?;
                Some((
                    *landblock,
                    Dragged::CellStatic {
                        cell: *cell,
                        index: *index,
                    },
                    f,
                ))
            }
            _ => None,
        }
    }

    /// What the pointer at `p` is over, as a selection.
    fn picked(&mut self, p: (f32, f32)) -> Option<Selection> {
        let picked = self.pick_at(p.0, p.1, Mask::ALL)?;
        Some(match picked.hit {
            Hit::Static {
                landblock, index, ..
            } => Selection::Static { landblock, index },
            Hit::Building {
                landblock, index, ..
            } => Selection::Building { landblock, index },
            Hit::Ground { vertex, .. } => Selection::Vertex {
                gx: vertex.0,
                gy: vertex.1,
            },
            Hit::Spawn { index } => {
                let s = &self.session.spawns()[index];
                Selection::Spawn {
                    file: s.file.clone(),
                    guid: s.guid.clone(),
                }
            }
            Hit::CellStatic {
                landblock,
                cell,
                index,
                ..
            } => Selection::CellStatic {
                landblock,
                cell,
                index,
            },
            Hit::Cell {
                landblock, cell, ..
            } => Selection::Cell { landblock, cell },
        })
    }

    /// A click at `p`: what is under it selected, or `adding` (Ctrl or Shift held) a static or
    /// spawn added to the selection, or taken out of it when it is in it already.
    fn click_select(&mut self, p: (f32, f32), adding: bool) {
        let picked = self.picked(p);
        let groupable = |s: &Selection| {
            matches!(
                s,
                Selection::Static { .. } | Selection::Spawn { .. } | Selection::CellStatic { .. }
            )
        };
        match picked {
            Some(s) if adding && groupable(&s) => {
                if self.selection.as_ref() == Some(&s) {
                    // The primary taken out: the next becomes it.
                    self.selection = if self.also.is_empty() {
                        None
                    } else {
                        Some(self.also.remove(0))
                    };
                } else if let Some(i) = self.also.iter().position(|x| *x == s) {
                    self.also.remove(i);
                } else {
                    if let Some(old) = self.selection.take().filter(groupable) {
                        self.also.push(old);
                    }
                    self.selection = Some(s);
                }
            }
            Some(_) | None if adding => {}
            other => {
                self.selection = other;
                self.also.clear();
            }
        }
        // A cell's piece shown in the dungeon kit.
        if let Some(Selection::Cell { landblock, cell }) = self.selection {
            self.choose_piece_of(landblock, cell);
        }
    }

    /// A box from `a` to `b` on screen: every static and spawn around the camera standing inside
    /// it selected, or `adding` them to the selection.
    fn box_select(&mut self, a: (f32, f32), b: (f32, f32), adding: bool) {
        let Some(view) = self.view() else {
            return;
        };
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        let inside = |p: Vec3| {
            view.project(p)
                .is_some_and(|(x, y)| (x0..=x1).contains(&x) && (y0..=y1).contains(&y))
        };
        let mut found = Vec::new();
        for lb in self.near_blocks() {
            let o = pick::block_origin(lb);
            if let Some(info) = self.session.statics(lb) {
                let n = self.session.origins(lb).len().min(info.objects.len());
                for (index, obj) in info.objects.iter().enumerate().take(n) {
                    if inside(add(o, obj.frame.origin)) {
                        found.push(Selection::Static {
                            landblock: lb,
                            index,
                        });
                    }
                }
            }
            for (cell, c) in self.cells_of(lb).iter() {
                let cell = *cell;
                for (index, obj) in c.static_objects.iter().enumerate() {
                    if inside(add(o, obj.frame.origin)) {
                        found.push(Selection::CellStatic {
                            landblock: lb,
                            cell,
                            index,
                        });
                    }
                }
            }
        }
        for s in self.session.spawns() {
            if inside(add(pick::block_origin(s.landblock()), s.frame.origin)) {
                found.push(Selection::Spawn {
                    file: s.file.clone(),
                    guid: s.guid.clone(),
                });
            }
        }
        if !adding {
            self.selection = None;
            self.also.clear();
        }
        for s in found {
            if self.selection.as_ref() == Some(&s) || self.also.contains(&s) {
                continue;
            }
            match &self.selection {
                Some(
                    Selection::Static { .. }
                    | Selection::Spawn { .. }
                    | Selection::CellStatic { .. },
                ) => self.also.push(s),
                _ => self.selection = Some(s),
            }
        }
    }

    /// The ground under the pointer: its landblock, its meters there, and its height.
    fn ground_at(&mut self, p: (f32, f32)) -> Option<(u16, Vec3)> {
        let hit = self.pick_at(p.0, p.1, Mask::GROUND)?;
        let Hit::Ground { point, .. } = hit.hit else {
            return None;
        };
        let (lb, x, y) = landblock_of(point)?;
        Some((lb, Vec3::new(x, y, point.z)))
    }

    /// With the body or the flying camera in an indoor cell: the cell and the point (landblock
    /// meters) the pointer's ray meets first on a floor or wall of the landblock's cells.
    fn cell_point(&mut self, p: (f32, f32)) -> Option<(u16, u16, Vec3)> {
        let lb = match self.body_cell() {
            Some(c) if c & 0xFFFF >= 0x0100 => u16::try_from(c >> 16).ok()?,
            Some(_) => return None,
            None => {
                let eye = self.eye();
                let (lb, x, y) = self.work_block()?;
                let base = pick::block_origin(lb);
                self.session.cell_at(lb, Vec3::new(x, y, eye.z - base.z))?;
                lb
            }
        };
        let base = pick::block_origin(lb);
        let ray = self.view()?.ray(p.0, p.1);
        let from = Vec3::new(
            ray.origin.x - base.x,
            ray.origin.y - base.y,
            ray.origin.z - base.z,
        );
        let (cell, at) = self.session.cell_hit(lb, from, ray.dir)?;
        Some((lb, cell, at))
    }

    fn place_at(&mut self, p: (f32, f32)) -> Result<(), String> {
        let model = self
            .model
            .ok_or_else(|| self.say("messages.choose_model"))?;
        // Indoors: on the floor of the pack's cell under the pointer.
        if let Some((lb, cell, at)) = self.cell_point(p) {
            let name = self
                .session
                .cell_names(lb)
                .remove(&cell)
                .ok_or_else(|| self.say("messages.cell_not_packs"))?;
            self.apply(
                &Op::PlaceCellStatic {
                    landblock: lb,
                    cell: name,
                    model,
                    at: [at.x, at.y, at.z],
                    rotation: Rotation::Heading(0.0),
                },
                false,
            )?;
            return Ok(());
        }
        let Some((lb, at)) = self.ground_at(p) else {
            return Ok(());
        };
        let z = if self.panels.on_ground {
            Height::Ground
        } else {
            Height::At(at.z + self.panels.height)
        };
        let applied = self.apply(
            &Op::PlaceStatic {
                landblock: lb,
                model,
                at: Place {
                    x: at.x,
                    y: at.y,
                    z,
                },
                rotation: Rotation::Heading(0.0),
            },
            false,
        )?;
        if let Some((lb, origin)) = applied.placed {
            if let Some(index) = self.session.origins(lb).iter().position(|o| *o == origin) {
                self.selection = Some(Selection::Static {
                    landblock: lb,
                    index,
                });
                self.also.clear();
            }
        }
        Ok(())
    }

    /// Where the brush under pointer `p` is centered, in world meters: the ground there, or for
    /// a brush that sets each vertex, the vertex nearest it.
    #[must_use]
    pub fn brush_center(&mut self, p: (f32, f32)) -> Option<(f32, f32)> {
        let (lb, at) = self.ground_at(p)?;
        let o = pick::block_origin(lb);
        let (x, y) = (o.x + at.x, o.y + at.y);
        Some(if self.panels.brush.by_vertex() {
            lyceum_foundry::brush::nearest_vertex(x, y)
        } else {
            (x, y)
        })
    }

    fn brush_at(&mut self, p: (f32, f32)) -> Result<(), String> {
        let Some((x, y)) = self.brush_center(p) else {
            return Ok(());
        };
        let sets =
            lyceum_foundry::brush::dab(&self.session, self.panels.brush, x, y, self.panels.radius);
        if sets.is_empty() {
            return Ok(());
        }
        let into = self.stroke;
        self.apply(&Op::SetVertices(sets), into)?;
        self.stroke = true;
        Ok(())
    }

    fn spawn_at(&mut self, p: (f32, f32)) -> Result<(), String> {
        let weenie = self
            .panels
            .weenie
            .clone()
            .ok_or_else(|| self.say("messages.choose_weenie"))?;
        // Indoors: in the cell under the pointer.
        let op = if let Some((lb, cell, at)) = self.cell_point(p) {
            Op::PlaceSpawnInCell {
                landblock: lb,
                cell,
                weenie,
                origin: at,
                heading: 0.0,
            }
        } else {
            let Some((lb, at)) = self.ground_at(p) else {
                return Ok(());
            };
            Op::PlaceSpawn {
                landblock: lb,
                weenie,
                origin: at,
                heading: 0.0,
            }
        };
        let applied = self.apply(&op, false)?;
        if let Some(file) = applied.spawned {
            if let Some(s) = self.session.spawns().into_iter().find(|s| s.file == file) {
                self.selection = Some(Selection::Spawn { file, guid: s.guid });
                self.also.clear();
            }
        }
        Ok(())
    }

    /// Bring the weenie tab forward.
    pub fn show_weenie(&mut self) {
        self.show_tab(Tab::Weenie);
    }

    /// Tab `tab` brought forward in its dock.
    pub fn show_tab(&mut self, tab: Tab) {
        if let Some(dock) = self.dock.as_mut() {
            if let Some(at) = dock.find_tab(&tab) {
                let _ = dock.set_active_tab(at);
            }
        }
    }

    /// The browser's models, in its order: those standing around the camera first, then every
    /// other, each answering the filter (an id or a name); those whose own name holds the filter
    /// before those only one of whose weenies' names does.
    #[must_use]
    pub fn browser_list(&self) -> Vec<DataId> {
        let filter = self.panels.filter.trim();
        let nearby = self.nearby_models();
        let list: Vec<DataId> = nearby
            .iter()
            .copied()
            .chain(
                self.all_models
                    .iter()
                    .copied()
                    .filter(|m| !nearby.contains(m)),
            )
            .filter(|m| self.in_shelf(*m))
            .filter(|m| self.names.matches(*m, filter, self.tags.of(*m)))
            .collect();
        let f = filter.to_lowercase();
        let (mut named, rest): (Vec<DataId>, Vec<DataId>) = list.into_iter().partition(|m| {
            f.is_empty()
                || self
                    .names
                    .label(*m)
                    .is_some_and(|l| l.to_lowercase().contains(&f))
        });
        named.extend(rest);
        named
    }

    /// Whether model `m` is on the browser's shelf: in its category, and with its tag, when it
    /// shows only one.
    fn in_shelf(&self, m: DataId) -> bool {
        self.panels
            .category
            .is_none_or(|c| self.names.category(m) == c)
            && self
                .panels
                .tag
                .as_ref()
                .is_none_or(|t| self.tags.of(m).contains(t))
    }

    /// Every weenie the spawn tool can place, from the packs' SQL and the server's: the packs'
    /// first, then the server's by name.
    fn rebuild_weenies(&mut self) {
        let mut out: Vec<WeenieEntry> = self
            .session
            .pack_weenies()
            .iter()
            .map(|(s, f)| WeenieEntry::new(Weenie::Symbol(s.clone()), f, true))
            .collect();
        // The server's classes, a pack's copy of one in its place; and a class only a pack
        // writes.
        let copies = self.session.pack_classes();
        let mut server: Vec<WeenieEntry> = self
            .session
            .server_weenies()
            .iter()
            .map(|(c, f)| match copies.get(c) {
                Some(copy) => WeenieEntry::new(Weenie::Class(*c), copy, true),
                None => WeenieEntry::new(Weenie::Class(*c), f, false),
            })
            .collect();
        server.extend(
            copies
                .iter()
                .filter(|(c, _)| !self.session.server_weenies().contains_key(c))
                .map(|(c, f)| WeenieEntry::new(Weenie::Class(*c), f, true)),
        );
        server.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(server);
        self.weenies = out;
    }

    /// The spawn browser's weenies, by their place in [`Self::weenies`]: those answering its
    /// filter in its category.
    #[must_use]
    pub fn spawn_list(&self) -> Vec<usize> {
        let f = self.panels.weenie_filter.trim().to_lowercase();
        self.weenies
            .iter()
            .enumerate()
            .filter(|(_, w)| self.panels.weenie_category.is_none_or(|c| w.category == c))
            .filter(|(_, w)| w.matches(&f))
            .map(|(i, _)| i)
            .collect()
    }

    /// How many models each category holds.
    #[must_use]
    pub fn category_counts(&self) -> BTreeMap<Category, usize> {
        let mut out = BTreeMap::new();
        for m in &self.all_models {
            *out.entry(self.names.category(*m)).or_default() += 1;
        }
        out
    }

    /// Take in what the gathering found once it has finished: the server's weenies, so its
    /// spawns are drawn, and every model's name and category.
    fn take_catalog(&mut self, wait: bool) -> Result<(), String> {
        let Some(c) = self.catalog.as_mut() else {
            return Ok(());
        };
        let Some(ready) = (if wait { c.wait() } else { c.poll() }) else {
            if !c.running() {
                self.catalog = None;
            }
            return Ok(());
        };
        self.catalog = None;
        if let Some(p) = &ready.problem {
            tracing::warn!("the model browser: {p}");
        }
        let learned = !ready.server.weenies.is_empty();
        if self.server_sql.is_none() {
            self.server_sql.clone_from(&ready.database);
        }
        if learned {
            self.session.learn_server(ready.server);
        }
        self.names = ModelNames::new(&ready.placements, &self.session.weenies_by_setup());
        self.rebuild_weenies();
        self.placements = Some(ready.placements);
        if let Some(mut map) = ready.map {
            // The packs' own landblocks, as the world holds them.
            let edited: Vec<u16> = lyceum_foundry::map::terrain_blocks(
                self.session
                    .compiled()
                    .records
                    .get(&dereth_dat::ModernDat::Cell),
            )
            .collect();
            map.redraw(&self.session, edited, &self.terrain_colors);
            self.world_map = Some(map);
            self.pictures.0.remove(&MAP_PICTURE);
        }
        if learned {
            // Spawns of the server's weenies are drawn now that their setups are known, and
            // the server's own around the camera.
            let mut landblocks: BTreeSet<u16> = self
                .session
                .spawns()
                .iter()
                .filter(|s| s.setup.is_some())
                .map(lyceum_foundry::edit::Spawn::landblock)
                .collect();
            self.shown_around = None;
            landblocks.extend(self.show_server_spawns_around());
            if !landblocks.is_empty() {
                self.reload(&Touched {
                    landblocks,
                    sql: false,
                })?;
            }
        }
        Ok(())
    }

    /// Show the server's own spawns of the landblocks drawn around the camera, when it has
    /// moved to another landblock: the landblocks newly shown that have some.
    fn show_server_spawns_around(&mut self) -> BTreeSet<u16> {
        // The landblock worked in: a dungeon's, though its rooms lie outside its square.
        let Some((lb, ..)) = self.work_block() else {
            return BTreeSet::new();
        };
        if self.shown_around == Some(lb) {
            return BTreeSet::new();
        }
        self.shown_around = Some(lb);
        let r = i32::try_from(self.radius).unwrap_or(2);
        let (bx, by) = (i32::from(lb >> 8), i32::from(lb & 0xFF));
        let near = (-r..=r).flat_map(|dx| (-r..=r).map(move |dy| (bx + dx, by + dy)));
        let near: Vec<u16> = near
            .filter(|(x, y)| (0..0xFF).contains(x) && (0..0xFF).contains(y))
            .filter_map(|(x, y)| u16::try_from((x << 8) | y).ok())
            .collect();
        self.session.show_server_spawns(near)
    }

    /// Wait for the model browser's names, as a script or a test that reads them does.
    ///
    /// # Errors
    /// The world would not draw the spawns it learned of again.
    pub fn wait_for_names(&mut self) -> Result<(), String> {
        self.take_catalog(true)
    }

    /// Whether the model browser's names are still being gathered.
    #[must_use]
    pub fn naming(&self) -> bool {
        self.catalog.is_some()
    }

    /// Give model `m` the tag `tag`, or take it away, and keep the tags.
    ///
    /// # Errors
    /// The tags file cannot be written.
    pub fn tag_model(&mut self, m: DataId, tag: &str, on: bool) -> Result<(), String> {
        if on {
            self.tags.add(m, tag)?;
        } else {
            self.tags.remove(m, tag)?;
            if self.panels.tag.as_deref() == Some(tag) && !self.tags.all().contains_key(tag) {
                self.panels.tag = None;
            }
        }
        Ok(())
    }

    /// Model `m`'s picture as the panels' texture: held, else read from the cache, or drawn while
    /// `budget` lasts.
    fn picture(&mut self, m: DataId, budget: &mut u32) -> Option<egui::TextureId> {
        if let Some(t) = self.pictures.0.get(&m.raw()) {
            return Some(t.id());
        }
        if self.thumbs.is_none() {
            let backend = self.backend();
            let opened = crate::thumbs::Thumbs::open(
                Arc::clone(&self.base_store),
                backend,
                &self.thumbs_root,
            );
            if let Err(e) = &opened {
                tracing::warn!("the model browser shows no pictures: {e}");
            }
            self.thumbs = Some(opened);
        }
        let data = self.thumbs.as_mut()?.as_mut().ok()?.get(m, budget)?.clone();
        let mut rgba = data.levels.first().cloned().unwrap_or_default();
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [data.width as usize, data.height as usize],
            &rgba,
        );
        let handle = self.gui.ctx.load_texture(
            format!("model {:08X}", m.raw()),
            image,
            egui::TextureOptions::LINEAR,
        );
        let id = handle.id();
        self.pictures.0.insert(m.raw(), handle);
        self.stats.pictures += 1;
        Some(id)
    }

    /// The region's terrain types, in order: each one's value, its name, and the texture its
    /// ground is drawn with.
    #[must_use]
    pub fn terrain_types(&self) -> Vec<(u16, String, Option<DataId>)> {
        let Some(region) = self.session.region() else {
            return Vec::new();
        };
        let descs = region
            .land_surf
            .tex_merge
            .as_ref()
            .map(|t| t.terrain_desc.as_slice())
            .unwrap_or_default();
        region
            .terrain_types
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let v = u16::try_from(i).ok()?;
                let tex = descs
                    .iter()
                    .find(|d| d.terrain_type == u32::from(v))
                    .map(|d| d.tex_gid);
                Some((v, lyceum_foundry::models::words(&t.terrain_name), tex))
            })
            .collect()
    }

    /// What each scene value grows on terrain `terrain`: the models its scenes place.
    pub fn scenes_of(&mut self, terrain: u16) -> &[Vec<DataId>] {
        let session = &self.session;
        self.scenes.entry(terrain).or_insert_with(|| {
            session.region().map_or_else(Vec::new, |r| {
                lyceum_foundry::models::scenery_of(session.base(), r, terrain)
            })
        })
    }

    /// A few words for what a scene grows: how many of each kind of thing, or nothing.
    #[must_use]
    pub fn scene_words(&self, models: &[DataId]) -> String {
        let mut counts: BTreeMap<Category, usize> = BTreeMap::new();
        for m in models {
            *counts.entry(self.names.category(*m)).or_default() += 1;
        }
        if counts.is_empty() {
            return "Nothing grows".into();
        }
        let mut counts: Vec<(Category, usize)> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        counts
            .iter()
            .map(|(c, n)| {
                let word = match c {
                    Category::Other => "thing".to_owned(),
                    c => c.name().to_lowercase(),
                };
                format!("{n} {word}{}", if *n == 1 { "" } else { "s" })
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The world map's texture in the panels, made the first time it is shown.
    fn map_texture(&mut self) -> Option<egui::TextureId> {
        if let Some(t) = self.pictures.0.get(&MAP_PICTURE) {
            return Some(t.id());
        }
        let map = self.world_map.as_ref()?;
        let side = lyceum_foundry::map::SIDE;
        let image = egui::ColorImage::from_rgba_unmultiplied([side, side], &map.rgba);
        let handle = self
            .gui
            .ctx
            .load_texture("world map", image, egui::TextureOptions::LINEAR);
        let id = handle.id();
        self.pictures.0.insert(MAP_PICTURE, handle);
        Some(id)
    }

    /// Where world meters `(x, y)` are in the map tab, as it was drawn last.
    #[must_use]
    pub fn map_point(&self, x: f32, y: f32) -> Option<egui::Pos2> {
        let rect = self.map_rect?;
        let (px, py) = lyceum_foundry::map::pixel_of(x, y);
        let v = self.map_view;
        let p = egui::pos2(
            rect.min.x + (px - v.min.x) / v.width() * rect.width(),
            rect.min.y + (py - v.min.y) / v.height() * rect.height(),
        );
        rect.contains(p).then_some(p)
    }

    /// Fly the camera to world meters `(x, y)`, as high over the ground there as it is over the
    /// ground now (ten meters at least); a walking body is put away first.
    ///
    /// # Errors
    /// The body would not be put away.
    pub fn fly_to(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.dungeon_block = None;
        if self.walking {
            self.set_walking(false)?;
        }
        let ground = |s: &Session, wx: f32, wy: f32| {
            let (lb, lx, ly) = landblock_of(Vec3::new(wx, wy, 0.0))?;
            s.ground(lb, lx, ly)
        };
        let p = self.camera.position;
        let over = ground(&self.session, p.x, p.y).map_or(30.0, |g| p.z - g);
        let z = ground(&self.session, x, y).unwrap_or(0.0) + over.max(10.0);
        self.camera.position = Vec3::new(x, y, z);
        Ok(())
    }

    /// Texture `tex` as a small picture for the panels, read the first time.
    fn texture_picture(&mut self, tex: DataId) -> Option<egui::TextureId> {
        if let Some(t) = self.pictures.0.get(&tex.raw()) {
            return Some(t.id());
        }
        let img = dereth_scene::textures::TextureStore::new(&self.base_store)
            .bgra8(tex)
            .ok()?;
        let (w, h) = (img.width as usize, img.height as usize);
        if w == 0 || h == 0 {
            return None;
        }
        // Each picture pixel the mean of the texels it covers.
        let side = crate::thumbs::SIZE as usize;
        let mut rgba = vec![0u8; side * side * 4];
        for py in 0..side {
            for px in 0..side {
                let (x0, x1) = (px * w / side, ((px + 1) * w / side).max(px * w / side + 1));
                let (y0, y1) = (py * h / side, ((py + 1) * h / side).max(py * h / side + 1));
                let mut sum = [0u32; 3];
                let mut n = 0u32;
                for y in y0..y1.min(h) {
                    for x in x0..x1.min(w) {
                        let [b, g, r, _] = img.pixels[y * w + x];
                        sum[0] += u32::from(r);
                        sum[1] += u32::from(g);
                        sum[2] += u32::from(b);
                        n += 1;
                    }
                }
                let o = (py * side + px) * 4;
                for (k, v) in sum.iter().enumerate() {
                    rgba[o + k] = u8::try_from(v / n.max(1)).unwrap_or(u8::MAX);
                }
                rgba[o + 3] = 255;
            }
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([side, side], &rgba);
        let handle = self.gui.ctx.load_texture(
            format!("texture {:08X}", tex.raw()),
            image,
            egui::TextureOptions::LINEAR,
        );
        let id = handle.id();
        self.pictures.0.insert(tex.raw(), handle);
        Some(id)
    }

    /// The device backend the editor draws with.
    fn backend(&self) -> dereth_render::device::Backend {
        use dereth_render::device::Backend;
        let fallback = Backend::default_backend().unwrap_or(Backend::Vulkan);
        self.core.cfg.renderer.map_or(fallback, Backend::from)
    }

    /// Where finding `i` of the checks is listed on screen, as last drawn: a click there goes to
    /// the place it names.
    #[must_use]
    pub fn fault_rect(&self, i: usize) -> Option<egui::Rect> {
        self.fault_rects.get(i).copied()
    }

    /// The pictures drawn and read from the cache so far.
    #[must_use]
    pub fn pictures(&self) -> (u64, u64) {
        match &self.thumbs {
            Some(Ok(t)) => (t.drawn, t.read),
            _ => (0, 0),
        }
    }

    /// Where model `m`'s picture is kept, when the browser has opened its pictures.
    #[must_use]
    pub fn picture_path(&self, m: DataId) -> Option<PathBuf> {
        match &self.thumbs {
            Some(Ok(t)) => Some(t.path(m)),
            _ => None,
        }
    }

    /// The models the statics around the camera stand as, most used first.
    fn nearby_models(&self) -> Vec<DataId> {
        let mut count: std::collections::BTreeMap<DataId, usize> =
            std::collections::BTreeMap::new();
        for lb in self.near_blocks() {
            if let Some(i) = self.session.statics(lb) {
                for o in &i.objects {
                    *count.entry(o.id).or_default() += 1;
                }
            }
        }
        let mut v: Vec<(usize, DataId)> = count.into_iter().map(|(m, n)| (n, m)).collect();
        v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.raw().cmp(&b.1.raw())));
        v.into_iter().map(|(_, m)| m).take(40).collect()
    }

    // ---------------------------------------------------------------------------------------
    // the scripted mode
    // ---------------------------------------------------------------------------------------

    /// Queue a device event, as the window would.
    pub fn queue(&mut self, e: HostEvent) {
        self.shell.inbox.push(e);
    }

    /// The window resized to `width` by `height`, as the window system says it: the back buffer
    /// follows at the next frame, and the panels and the world view are laid out again in it.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.shell
            .events
            .borrow_mut()
            .push(HostEvent::Resized { width, height });
    }

    /// Do one step of a script, before its frame.
    ///
    /// # Errors
    /// An edit the session refuses, or a step that cannot be done.
    pub fn step(&mut self, s: &Step) -> Result<(), String> {
        match s.camera {
            Some(Camera::At {
                landblock,
                at,
                yaw,
                pitch,
            }) => {
                let mut c = FlyCamera::new(
                    add(pick::block_origin(landblock), at),
                    yaw.to_radians(),
                    pitch.to_radians(),
                );
                c.speed = self.camera.speed;
                self.camera = c;
            }
            Some(Camera::LookAt {
                landblock,
                at,
                distance,
                yaw,
                pitch,
            }) => self.look_at(landblock, at, distance, yaw, pitch),
            None => {}
        }
        if let Some(walk) = s.walk {
            self.set_walking(walk)?;
        }
        if let Some(m) = &s.mode {
            self.mode = Mode::named(m).ok_or_else(|| format!("{m:?} is not a mode"))?;
        }
        if let Some(m) = s.model {
            self.model = Some(DataId(m));
        }
        if let Some(f) = &s.filter {
            f.clone_into(&mut self.panels.filter);
        }
        if let Some(tab) = &s.show {
            match tab.as_str() {
                "weenie" => self.show_weenie(),
                other => return Err(format!("{other:?} is not a tab to show")),
            }
        }
        if let Some(name) = &s.shelf {
            self.panels.category = if name.eq_ignore_ascii_case("all") {
                None
            } else {
                Some(
                    Category::ALL
                        .into_iter()
                        .find(|c| c.name().eq_ignore_ascii_case(name))
                        .ok_or_else(|| format!("{name:?} is not a shelf"))?,
                )
            };
        }
        if let Some(w) = &s.weenie {
            self.panels.weenie = Some(match w {
                WeenieChoice::Symbol(s) => Weenie::Symbol(s.clone()),
                WeenieChoice::Class(c) => Weenie::Class(*c),
            });
        }
        if let Some(label) = &s.piece {
            self.wait_for_pieces();
            let i = self
                .kit
                .pieces
                .as_ref()
                .and_then(|ps| ps.iter().position(|p| p.label() == *label))
                .ok_or_else(|| format!("{label:?} is not a piece"))?;
            self.kit.chosen = Some(i);
        }
        if let Some(n) = s.extend {
            self.extend(n)?;
        }
        if let Some(name) = &s.inside {
            self.go_inside(name)?;
        }
        if let Some(b) = s.brush {
            self.panels.brush = b.brush;
            self.panels.radius = b.radius;
        }
        if let Some((lb, name)) = &s.select {
            let origin = self
                .session
                .static_named(*lb, name)
                .map_err(|e| e.to_string())?;
            let index = self
                .session
                .origins(*lb)
                .iter()
                .position(|o| *o == origin)
                .ok_or_else(|| self.say("messages.static_gone"))?;
            self.selection = Some(Selection::Static {
                landblock: *lb,
                index,
            });
            self.also.clear();
        }
        if let Some(p) = s.pointer {
            let (x, y) = match p {
                Pointer::Pixel(x, y) => (x, y),
                Pointer::At {
                    landblock,
                    at,
                    lift,
                } => {
                    let at = match lift {
                        Some(l) => Vec3::new(
                            at.x,
                            at.y,
                            self.session.ground(landblock, at.x, at.y).unwrap_or(0.0) + l,
                        ),
                        None => at,
                    };
                    self.view()
                        .and_then(|v| v.project(add(pick::block_origin(landblock), at)))
                        .ok_or("that place is not on screen")?
                }
            };
            self.queue(HostEvent::CursorMoved {
                x: f64::from(x),
                y: f64::from(y),
            });
        }
        if let Some(press) = s.press {
            self.queue(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed: press,
            });
        }
        if let Some(d) = s.drag {
            self.queue_drag(d)?;
        }
        if let Some(k) = &s.key {
            let (vk, ctrl) = match k.as_str() {
                "delete" => (0x2E, false),
                "escape" => (0x1B, false),
                "undo" => (0x5A, true),
                "redo" => (0x59, true),
                "save" => (0x53, true),
                other => return Err(format!("{other:?} is not a key a script presses")),
            };
            let key = |vk: usize| dereth_input::keys::Key::new(vk, 0);
            if ctrl {
                self.queue(HostEvent::KeyboardInput {
                    key: key(0x11),
                    pressed: true,
                    text: None,
                });
            }
            for pressed in [true, false] {
                self.queue(HostEvent::KeyboardInput {
                    key: key(vk),
                    pressed,
                    text: None,
                });
            }
            if ctrl {
                self.queue(HostEvent::KeyboardInput {
                    key: key(0x11),
                    pressed: false,
                    text: None,
                });
            }
        }
        if let Some((name, from, blank)) = &s.new_weenie {
            self.new_weenie(name, *from, *blank)?;
        }
        if let Some(t) = &s.edit {
            let pack = self.session.packs()[self.session.target()].name.clone();
            let edit = lyceum_foundry::script::action(t, &pack)?;
            self.edit(&edit)?;
        }
        match s.live {
            Some(true) => {
                self.set_live(true)?;
                self.wait_live()?;
            }
            Some(false) if s.play.is_none() => self.set_live(false)?,
            _ => {}
        }
        if s.export_gdle == Some(true) {
            self.export_gdle(true)?;
        }
        if let Some(extra) = &s.play {
            let code = self.play(extra, true)?;
            tracing::info!("the client ended: {code:?}");
            if code != Some(0) {
                return Err(format!("the client ended with {code:?}"));
            }
            if s.live == Some(false) {
                self.set_live(false)?;
            }
        }
        Ok(())
    }

    /// Queue a drag of the selected static's or spawn's handle over the next four frames: the
    /// press, two moves, the release.
    ///
    /// # Errors
    /// Nothing is selected, or the handle is not on screen.
    pub fn queue_drag(&mut self, d: crate::script::Drag) -> Result<(), String> {
        let h = self
            .handles()
            .ok_or("a drag needs a selected static or spawn")?;
        let (from, to, mid) = if d.handle == Handle::Turn {
            let n = h.ring.len().max(2) - 1;
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let at = |deg: f32| {
                let i = ((deg.rem_euclid(360.0) / 360.0) * n as f32).round() as usize;
                h.ring[i.min(n)]
            };
            // Taken on the side of the ring nearest the camera, below the static on screen.
            let start = 270.0;
            (at(start), at(start + d.by), at(start + d.by * 0.5))
        } else {
            let tip = h
                .arrows
                .iter()
                .find(|(k, _)| *k == d.handle)
                .ok_or("that arrow is not on screen")?
                .1;
            let (dx, dy) = (tip.0 - h.origin.0, tip.1 - h.origin.1);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let from = (h.origin.0 + dx * 0.6, h.origin.1 + dy * 0.6);
            let to = (from.0 + dx / len * d.by, from.1 + dy / len * d.by);
            (from, to, ((from.0 + to.0) * 0.5, (from.1 + to.1) * 0.5))
        };
        let moved = |p: (f32, f32)| HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        };
        let button = |pressed| HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed,
        };
        self.later.push_back(vec![moved(from), button(true)]);
        self.later.push_back(vec![moved(mid)]);
        self.later.push_back(vec![moved(to)]);
        self.later.push_back(vec![button(false)]);
        Ok(())
    }

    /// Run `steps` over `frames` frames, writing `captures` as their frames are drawn.
    ///
    /// # Errors
    /// A step that fails, or a capture that will not write.
    pub fn run_script(
        &mut self,
        steps: &[Step],
        frames: u64,
        captures: &[(u64, PathBuf)],
    ) -> Result<(), String> {
        // The model browser's names first, so a capture shows them whatever the machine.
        self.wait_for_names()?;
        for n in 1..=frames {
            for s in steps.iter().filter(|s| s.frame == n) {
                self.step(s).map_err(|e| format!("frame {n}: {e}"))?;
            }
            if !self.frame() {
                break;
            }
            for (_, path) in captures.iter().filter(|(f, _)| *f == n) {
                self.capture(path)?;
            }
        }
        Ok(())
    }

    /// The static `name` of landblock `lb`, as selected.
    ///
    /// # Errors
    /// No such static.
    pub fn select_static(&mut self, lb: u16, name: &StaticName) -> Result<(), String> {
        self.step(&Step {
            select: Some((lb, name.clone())),
            ..Step::default()
        })
    }
}

/// What a click on the panels or a key asks for.
#[derive(Debug, Clone, PartialEq)]
enum UiAction {
    Mode(Mode),
    Undo,
    Redo,
    Save,
    Delete,
    Deselect,
    ToggleWalk,
    Snap,
    /// The flying camera made to stop at things, or to pass through them.
    Collide,
    Speed(i32),
    Model(DataId),
    Brush(Brush),
    /// The brush's radius a step bigger or smaller.
    Radius(i32),
    Amount(i32),
    OnGround(bool),
    Weenie(Weenie),
    Vertex(Field, i32),
    SnapStatic,
    GoTo(u16, Vec3),
    /// The browser's shelf: one category (or all), one tag (or any).
    Category(Option<Category>),
    Tag(Option<String>),
    /// A tag given to a model, or taken from it.
    TagModel(DataId, String, bool),
    /// The spawn browser's category, or all.
    WeenieCategory(Option<Category>),
    /// A change of the weenie form.
    Form(FormEdit),
    /// The weenie tab brought forward.
    ShowWeenie,
    /// A server class copied into the pack being edited, to edit.
    CopyClass(u32),
    /// A new weenie named so, made from a class (a blank of it, or a copy).
    NewWeenie(String, u32, bool),
    /// The camera flown to world meters.
    FlyTo(f32, f32),
    /// The map zoomed in (`> 1`) or out by a factor.
    MapZoom(f32),
    /// The dungeon kit's piece chosen (its place in the browser's list), its turn step, a cell
    /// of the pack taken away (its landblock and symbol).
    Piece(usize),
    Step(f32),
    RemoveCell(u16, String),
    /// The dungeon kit's piece of an environment chosen, shown in the browser.
    FindPiece(DataId, u16),
    /// A cell of a landblock selected, its piece chosen, and flown into.
    ChooseCell(u16, u16),
    /// A cell of a landblock reshaped with the chosen piece.
    ReplaceCell(u16, u16),
    /// The editor's own world started, or stopped; the client started in it, and how.
    Live(bool),
    Play,
    PlayOptions(play::PlayOptions),
    /// Entry of the Nearby list as shown: selected, and flown to.
    Nearby(usize),
    /// Into a dungeon, to build on it or (true) to edit what is in it; a new one; a portal in.
    EnterDungeon(u16, bool),
    NewDungeon,
    PortalToDungeon(u16),
    /// A docked tab brought forward.
    ShowTab(Tab),
    /// A new weenie made for what it is for, named so.
    NewFor(Flow, String),
    /// A spawn of the weenie where the camera is.
    PlaceHere(Weenie),
    /// The form's portal sent where the camera is, or into a dungeon.
    DestinationHere,
    DestinationDungeon(u16),
    /// A spawn selected and flown to.
    GoToSpawn(PathBuf, String),
    /// The world's content written for GDLE.
    ExportGdle,
}

fn amount_of(b: Brush) -> u8 {
    match b {
        Brush::Raise(n) | Brush::Lower(n) => n,
        _ => 1,
    }
}

fn value_of(b: Brush) -> u16 {
    match b {
        Brush::Terrain(v) | Brush::Scene(v) | Brush::Road(v) => v,
        _ => 0,
    }
}

/// The brush with its amount or value moved by `d`.
fn step_brush(b: Brush, d: i32) -> Brush {
    let n = |v: u8| u8::try_from((i32::from(v) + d).clamp(1, 40)).unwrap_or(1);
    let v = |x: u16, max: i32| u16::try_from((i32::from(x) + d).clamp(0, max)).unwrap_or(0);
    match b {
        Brush::Raise(a) => Brush::Raise(n(a)),
        Brush::Lower(a) => Brush::Lower(n(a)),
        Brush::Terrain(x) => Brush::Terrain(v(x, 31)),
        Brush::Scene(x) => Brush::Scene(v(x, 31)),
        Brush::Road(x) => Brush::Road(v(x, 3)),
        other => other,
    }
}

/// Each terrain type's color on the map: the mean of the texture its ground is drawn with (a
/// gray where it has none).
fn terrain_colors(
    store: &dereth_dat::RetailDatStore,
    region: &dereth_assets::Region,
) -> Vec<[u8; 3]> {
    let textures = dereth_scene::textures::TextureStore::new(store);
    let descs = region
        .land_surf
        .tex_merge
        .as_ref()
        .map(|t| t.terrain_desc.as_slice())
        .unwrap_or_default();
    (0..region.terrain_types.len().max(33))
        .map(|i| {
            let tex = descs
                .iter()
                .find(|d| usize::try_from(d.terrain_type).ok() == Some(i))
                .map(|d| d.tex_gid);
            tex.and_then(|t| textures.bgra8(t).ok())
                .filter(|img| !img.pixels.is_empty())
                .map_or([128, 128, 128], |img| {
                    let mut sum = [0u64; 3];
                    for [b, g, r, _] in &img.pixels {
                        sum[0] += u64::from(*r);
                        sum[1] += u64::from(*g);
                        sum[2] += u64::from(*b);
                    }
                    let n = img.pixels.len() as u64;
                    sum.map(|v| u8::try_from(v / n).unwrap_or(u8::MAX))
                })
        })
        .collect()
}

/// The server's names the forms show: from `--schema`, else written by `empyrean-import schema`
/// when the server's tools are found (`--empyrean`, else beside the editor; kept under `cache`); none
/// otherwise.
fn read_schema(
    args: &Args,
    cache: &Path,
) -> Result<Option<lyceum_foundry::content::Schema>, String> {
    let read = |p: &Path| -> Result<lyceum_foundry::content::Schema, String> {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        lyceum_foundry::content::Schema::read(&text).map_err(|e| format!("{}: {e}", p.display()))
    };
    if let Some(p) = &args.schema {
        return read(p).map(Some);
    }
    let Some(tool) = crate::live::program(args.empyrean.as_deref(), "empyrean-import") else {
        return Ok(None);
    };
    let out = cache.join("schema.json");
    let _ = std::fs::create_dir_all(cache);
    let ran = std::process::Command::new(&tool)
        .args(["schema", "--out"])
        .arg(&out)
        .status();
    match ran {
        Ok(s) if s.success() => read(&out).map(Some),
        _ => {
            tracing::warn!("{} wrote no schema: the forms show numbers", tool.display());
            Ok(None)
        }
    }
}
