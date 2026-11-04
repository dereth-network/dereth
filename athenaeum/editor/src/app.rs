//! The editor: a world held open (its session), the camera flying through it, the rows standing
//! in it drawn as bodies and marked where they stand, the one selected, and the panels that show
//! and change them.
//!
//! **In the world.** The right button held turns the camera; W, A, S, D fly it, E rises and Q
//! sinks, the wheel changes its speed. A click on a marker or a body selects its row; dragging it moves the
//! row over the ground, and the selected row's handles ([`gizmo`]) move it along east, north and
//! up and turn it, by steps with Snap on (Shift for the other). R turns the selected row a step to the right (with Shift, to the left),
//! Delete or Backspace takes it away, F flies to it, Escape lets it go. T sends the character
//! playing on the editor's own Yalain to the camera. Ctrl+S saves, Ctrl+Z undoes, Ctrl+Y redoes
//! (Cmd on macOS, as well).
//!
//! **The panels.** Along the top: save, undo and redo; what to place (a person, an object, a camp
//! or a place, and its template) and the button that places it at the next click on the ground;
//! the moment of the day; and play ([`play`]: the playtest character, Characters, Live, Play,
//! To camera, Restart). On the left, the
//! rows near the camera, each with the letter of where it came from (C recorded sessions, D the
//! data files, R a rule, A a person), and the database's rows the world takes away. On the right,
//! the selected row: where it came from and what it was, its fields, which are changed there.
//! Along the bottom: the world's problems, as the server's check finds them, and the status.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use athenaeum_foundry::edit::{NewKind, NewRow};
use athenaeum_foundry::{Entry, Layer, Session};
use dereth_builder::camera::{FlyCamera, Held};
use dereth_primitives::num::math;
use dereth_primitives::Vec3;
use winit::keyboard::KeyCode;
use yalain_world_db::compile::{rotation_heading, Install};
use yalain_world_db::rows::{ObjectKind, Prov, Row, Source};
use yalain_world_db::{layer, Problem};

use crate::characters::Recipe;
use crate::gizmo::{self, Handle, Ray, Snap};
use crate::live::{self, Live, Programs};
use crate::scene::{Scene, Standing};
use crate::view::{self, View};

pub mod play;

/// How far from the camera rows are drawn as bodies and listed, meters.
pub const NEAR: f32 = 160.0;
/// How far from the camera rows are marked, meters.
pub const MARKED: f32 = 400.0;
/// How close to a marker a click selects it, pixels.
pub const HIT: f32 = 12.0;
/// How many rows the Find window lists.
pub const FOUND: usize = 200;
/// How far from a row its copy stands, meters.
pub const DUPLICATE_AWAY: f32 = 2.0;
/// The step R turns a row by, degrees.
pub const TURN_STEP: f32 = 15.0;

/// The window's input for one frame that the panels did not take.
#[derive(Debug, Default, Clone)]
pub struct ViewInput {
    /// The pointer, pixels.
    pub pointer: [f32; 2],
    pub left_pressed: bool,
    pub left_released: bool,
    pub left_down: bool,
    pub right_down: bool,
    /// The mouse's motion, counts.
    pub motion: (f32, f32),
    pub wheel: f32,
    pub pressed: Vec<KeyCode>,
    pub down: HashSet<KeyCode>,
    pub ctrl: bool,
    pub shift: bool,
}

/// What the next click on the ground does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Place,
}

/// A row being dragged: where the press was, and the ground under the pointer now.
#[derive(Debug, Clone, PartialEq)]
struct Drag {
    item: String,
    from: [f32; 2],
    moved: bool,
    to: Option<[f32; 3]>,
}

/// A handle of the selected row being dragged: what the row would become when it is let go.
#[derive(Debug, Clone, PartialEq)]
struct Grip {
    item: String,
    handle: Handle,
    /// Where the row stood and which way it faced at the press, and the pointer's line then.
    origin: [f32; 3],
    heading: f32,
    from: Ray,
    at: [f32; 3],
    facing: f32,
}

/// Which rows the list and the world show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    people: bool,
    objects: bool,
    camps: bool,
    places: bool,
    only_world: bool,
}

/// The kind of a row, as the panels name it.
#[must_use]
pub fn kind_word(row: &Row) -> &'static str {
    match row {
        Row::Object(o) if o.kind == ObjectKind::Npc => "person",
        Row::Object(o) if !o.landings.is_empty() => "lifestone",
        Row::Object(o) if o.to.is_some() => "portal",
        Row::Object(_) => "object",
        Row::Camp(_) => "camp",
        Row::Place(_) => "place",
        _ => "row",
    }
}

/// The letter of where a row came from, and its words.
#[must_use]
pub fn badge(p: Option<&Prov>) -> (&'static str, String) {
    match p {
        None => ("A", "authored".into()),
        Some(p) => (
            match p.source {
                Source::Capture => "C",
                Source::Dat => "D",
                Source::Derived => "R",
                Source::Authored => "A",
            },
            p.to_string(),
        ),
    }
}

fn color_of(row: &Row) -> egui::Color32 {
    match kind_word(row) {
        "person" => egui::Color32::from_rgb(250, 210, 90),
        "lifestone" => egui::Color32::from_rgb(110, 230, 140),
        "portal" => egui::Color32::from_rgb(200, 130, 255),
        "object" => egui::Color32::from_rgb(120, 190, 255),
        "camp" => egui::Color32::from_rgb(255, 110, 100),
        _ => egui::Color32::from_rgb(235, 235, 235),
    }
}

/// The editor's state.
pub struct Editor<'w> {
    pub session: Session,
    install: &'w dyn Install,
    pub scene: Scene<'w>,
    pub camera: FlyCamera,
    pub selected: Option<String>,
    pub tool: Tool,
    pub new_kind: NewKind,
    pub template: String,
    drag: Option<Drag>,
    looks: HashMap<u32, Option<(u32, f32)>>,
    labels: HashMap<u32, String>,
    shown: Shown,
    /// The inspector's text for each field of the selected row, as typed.
    fields: Vec<(String, String)>,
    fields_of: Option<(String, u64)>,
    pub status: String,
    pub live: Live,
    pub programs: Programs,
    /// The window, pixels; and pixels to the panels' point.
    pub size: (u32, u32),
    pub scale: f32,
    /// Rows near the camera, by index into the session's entries, nearest first.
    near: Vec<(usize, f32)>,
    near_at: Option<([f32; 3], u64)>,
    /// The base's folder, when the world lies over one.
    pub base: Option<PathBuf>,
    /// The selected row's place and heading while one of them is dragged in the inspector.
    held: Option<(String, [f32; 4])>,
    /// Every template of the install, for the finder; and what the finder looks for.
    templates: Vec<u32>,
    finding: Option<String>,
    /// The last build against the install: the world's revision it was of, its problems and
    /// warnings in the world's own files, and what it made.
    build: Option<(u64, Vec<Problem>, Vec<Problem>, String)>,
    /// How many problems the list showed last frame.
    problems_seen: usize,
    /// The world's playtest characters, the one Play plays as, and recipes that do not read.
    pub recipes: Vec<Recipe>,
    pub character: String,
    recipe_problems: Vec<String>,
    /// Whether the Characters window is open, and the name typed for a new character.
    pub characters_open: bool,
    new_name: String,
    /// The world's revision last published to the editor's own Yalain.
    published: u64,
    /// The skills' names, by number, for the recipes' skills.
    pub skill_names: Vec<(u32, String)>,
    /// What the Characters window is adding to the chosen recipe, and importing.
    adding: play::Adding,
    /// Closing was asked for with changes not saved: the panels ask what to do with them.
    closing: bool,
    /// The window is to close now.
    pub close_now: bool,
    /// What the Find window looks for, and what it reads each row by, of a revision of the world.
    finding_rows: Option<String>,
    focus_find: bool,
    row_index: Option<(u64, Vec<String>)>,
    /// The row under the pointer.
    hovered: Option<String>,
    /// The handle being dragged.
    grip: Option<Grip>,
    /// The steps moves and turns snap to.
    pub snap: Snap,
    /// The block whose rooms the camera was last put among: a dungeon's rooms run on over other
    /// blocks' ground, so where the camera is doesn't say whose they are.
    home: Option<u16>,
    /// The block and interior cell the camera is in.
    inside: Option<(u16, u16)>,
    /// Where the row being placed would stand: the ground under the pointer.
    ghost: Option<[f32; 3]>,
    /// The way the row being placed faces, once R has turned it; until then it faces the camera.
    place_heading: Option<f32>,
}

impl std::fmt::Debug for Editor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor")
            .field("session", &self.session)
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

/// The global point of an entry: its landblock's grid with its point (the ground's height where
/// it gives none).
fn entry_global(e: &Entry, scene: &mut Scene<'_>) -> [f32; 3] {
    let at = e.at();
    let g = view::global(e.landblock(), [at.x, at.y, 0.0]);
    let z = at.z.or_else(|| scene.ground(g[0], g[1])).unwrap_or(0.0);
    [g[0], g[1], z]
}

fn rotation_of(row: &Row) -> [f32; 4] {
    let (q, h) = match row {
        Row::Object(o) => (o.rotation, o.heading),
        Row::Place(p) => (p.rotation, p.heading),
        _ => (None, None),
    };
    q.unwrap_or_else(|| view::heading_rotation(h.unwrap_or(0.0)))
}

/// A row's heading, degrees.
fn heading_of(row: &Row) -> f32 {
    match row {
        Row::Object(o) => o
            .heading
            .unwrap_or_else(|| o.rotation.map_or(0.0, rotation_heading)),
        Row::Place(p) => p
            .heading
            .unwrap_or_else(|| p.rotation.map_or(0.0, rotation_heading)),
        _ => 0.0,
    }
}

/// A field of a row as TOML text, for the inspector.
fn field_text(row: &Row, field: &str) -> String {
    let hex = |v: u32| format!("0x{v:08X}");
    let num = athenaeum_foundry::toml::number_text;
    match (row, field) {
        (Row::Object(o), "template") => hex(o.template),
        (Row::Object(o), "kind") => format!(
            "\"{}\"",
            if o.kind == ObjectKind::Npc {
                "npc"
            } else {
                "static"
            }
        ),
        (Row::Object(o), "cell") => o.cell.map(hex).unwrap_or_default(),
        (Row::Object(o), "id") => o.id.map(|v| format!("0x{v:016X}")).unwrap_or_default(),
        (Row::Object(o), "to") => {
            o.to.as_ref()
                .map(|t| format!("\"{t}\""))
                .unwrap_or_default()
        }
        (Row::Object(o), "name") => o
            .name
            .map(|n| {
                format!(
                    "{{ table = 0x{:08X}, string = 0x{:08X} }}",
                    n.table, n.string
                )
            })
            .unwrap_or_default(),
        (Row::Camp(c), "radius") => num(c.radius),
        (Row::Camp(c), "population") => c.population.to_string(),
        #[allow(clippy::cast_possible_truncation)]
        (Row::Camp(c), "regen") => num(c.regen as f32),
        (Row::Camp(c), "spawns") => format!(
            "[{}]",
            c.spawns
                .iter()
                .map(|(t, w)| format!("{{ template = 0x{t:08X}, weight = {w} }}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Row::Camp(c), "cell") => c.cell.map(hex).unwrap_or_default(),
        (Row::Place(p), "cell") => p.cell.map(hex).unwrap_or_default(),
        (Row::Place(p), "destination") => p
            .destination
            .as_ref()
            .map(|d| format!("\"{d}\""))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Whether a row stands in an interior cell.
fn is_indoors(row: &Row) -> bool {
    let cell = match row {
        Row::Object(o) => o.cell,
        Row::Camp(c) => c.cell,
        Row::Place(p) => p.cell,
        _ => None,
    };
    cell.is_some_and(|c| c & 0xFFFF >= 0x100)
}

/// What is typed in the inspector as the TOML of its field: a field of words needs no quotes typed.
#[must_use]
pub fn field_toml(field: &str, typed: &str) -> String {
    let t = typed.trim();
    let words = matches!(field, "to" | "destination");
    if words && !t.is_empty() && !t.starts_with('"') {
        format!("\"{t}\"")
    } else {
        t.to_string()
    }
}

/// The fields the inspector lets a person change, by kind.
fn editable(row: &Row) -> &'static [&'static str] {
    match row {
        Row::Object(_) => &["template", "kind", "to", "name", "id", "cell"],
        Row::Camp(_) => &["spawns", "radius", "population", "regen", "cell"],
        Row::Place(_) => &["cell", "destination"],
        _ => &[],
    }
}

impl<'w> Editor<'w> {
    #[must_use]
    pub fn new(
        session: Session,
        install: &'w dyn Install,
        scene: Scene<'w>,
        camera: FlyCamera,
        programs: Programs,
    ) -> Self {
        let mut e = Self {
            session,
            install,
            scene,
            camera,
            selected: None,
            tool: Tool::Select,
            new_kind: NewKind::Person,
            template: "0x470033D0".into(),
            drag: None,
            looks: HashMap::new(),
            labels: HashMap::new(),
            shown: Shown {
                people: true,
                objects: true,
                camps: true,
                places: true,
                only_world: false,
            },
            fields: Vec::new(),
            fields_of: None,
            status: String::new(),
            live: Live::default(),
            programs,
            size: (1280, 800),
            scale: 1.0,
            near: Vec::new(),
            near_at: None,
            base: None,
            held: None,
            templates: Vec::new(),
            finding: None,
            build: None,
            problems_seen: 0,
            recipes: Vec::new(),
            character: play::FIRST.to_string(),
            recipe_problems: Vec::new(),
            characters_open: false,
            new_name: String::new(),
            published: 0,
            skill_names: Vec::new(),
            adding: play::Adding::default(),
            closing: false,
            close_now: false,
            finding_rows: None,
            focus_find: false,
            row_index: None,
            hovered: None,
            grip: None,
            snap: Snap::default(),
            home: None,
            inside: None,
            ghost: None,
            place_heading: None,
        };
        e.read_characters();
        e
    }

    /// The camera's view of the window.
    #[must_use]
    pub fn view(&self) -> View {
        View::of(&self.camera, self.scene.fov, self.size)
    }

    /// A template's look and scale, as the server describes it.
    fn look(&mut self, template: u32) -> Option<(u32, f32)> {
        let install = self.install;
        *self.looks.entry(template).or_insert_with(|| {
            let d = yalain_content::describe(template, &|id| install.entity(id), &|id| {
                install.qualities(id)
            })
            .ok()?;
            let mut look = d.look;
            for _ in 0..8 {
                if look >> 24 != 0x47 {
                    break;
                }
                look = install.entity(look)?.object?;
            }
            Some((look, d.scale.unwrap_or(1.0)))
        })
    }

    /// A template's name, to read a row by.
    pub fn label(&mut self, template: u32) -> String {
        let install = self.install;
        self.labels
            .entry(template)
            .or_insert_with(|| plain_name(&install.label(template)))
            .clone()
    }

    fn shows(&self, e: &Entry) -> bool {
        if self.shown.only_world && e.layer == Layer::Base {
            return false;
        }
        // A dungeon's rooms lie over other blocks' ground: a row in one shows among them, or over
        // its own block's ground, and not where it merely falls on the grid.
        if is_indoors(&e.row.row) {
            let lb = e.landblock();
            let among = self.home == Some(lb) || self.inside.is_some_and(|(b, _)| b == lb);
            if !among {
                let at = e.at();
                let (under, _) = view::local(view::global(lb, [at.x, at.y, 0.0]));
                let (camera_over, _) = view::local(self.view().eye);
                if under != lb || camera_over != lb {
                    return false;
                }
            }
        }
        match &e.row.row {
            Row::Object(o) if o.kind == ObjectKind::Npc => self.shown.people,
            Row::Object(_) => self.shown.objects,
            Row::Camp(_) => self.shown.camps,
            Row::Place(_) => self.shown.places,
            _ => false,
        }
    }

    /// The block and interior cell the camera is in, if it is in one.
    #[must_use]
    pub const fn inside(&self) -> Option<(u16, u16)> {
        self.inside
    }

    /// Put the camera among the rooms of a block, if a point of its frame is in one.
    pub fn enter_block(&mut self, lb: u16, at: [f32; 3]) -> bool {
        if self.scene.cell_holding(lb, at).is_some() {
            self.home = Some(lb);
            self.near_at = None;
            true
        } else {
            false
        }
    }

    /// Work out which interior cell the camera is in: of the block it was put among, else of the
    /// block under it.
    fn find_inside(&mut self) {
        let eye = self.view().eye;
        let (under, _) = view::local(eye);
        let blocks = self.home.into_iter().chain(std::iter::once(under));
        let mut inside = None;
        for lb in blocks.collect::<Vec<u16>>() {
            let at = view::local_in(lb, eye);
            if let Some(cell) = self.scene.cell_holding(lb, at) {
                #[allow(clippy::cast_possible_truncation)]
                let low = cell as u16;
                inside = Some((lb, low));
                break;
            }
        }
        if inside != self.inside {
            self.near_at = None;
        }
        self.inside = inside;
    }

    /// The rows near the camera, nearest first, worked out again when the camera or the world
    /// moved.
    fn near(&mut self) {
        let eye = self.view().eye;
        let rev = self.session.revision();
        // A row an undo took away is let go.
        if self
            .selected
            .as_deref()
            .is_some_and(|i| self.session.entry(i).is_none())
        {
            self.select(None);
        }
        if let Some((at, r)) = self.near_at {
            let d = math::hypotf(at[0] - eye[0], at[1] - eye[1]);
            if r == rev && d < 8.0 {
                return;
            }
        }
        let mut near = Vec::new();
        for (i, e) in self.session.entries().iter().enumerate() {
            let at = e.at();
            let g = view::global(e.landblock(), [at.x, at.y, at.z.unwrap_or(eye[2])]);
            let d = math::hypotf(g[0] - eye[0], g[1] - eye[1]);
            if d <= MARKED {
                near.push((i, d));
            }
        }
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        self.near = near;
        self.near_at = Some((eye, rev));
    }

    /// Fly the camera to a row: a little back from it and above, looking at it.
    pub fn fly_to(&mut self, item: &str) {
        let Some(e) = self.session.entry(item).cloned() else {
            return;
        };
        let g = entry_global(&e, &mut self.scene);
        let (sy, cy) = (math::sinf(self.camera.yaw), math::cosf(self.camera.yaw));
        if is_indoors(&e.row.row) {
            // Among its rooms: a little back from it if that is in a room, else over it.
            let lb = e.landblock();
            let back = [g[0] + sy * 3.0, g[1] - cy * 3.0, g[2] + 1.8];
            let over = [g[0], g[1], g[2] + 1.8];
            let held = |s: &mut Self, p: [f32; 3]| s.enter_block(lb, view::local_in(lb, p));
            let at = if held(self, back) {
                back
            } else {
                held(self, over);
                over
            };
            self.camera.position = Vec3::new(at[0], at[1], at[2]);
            let (dx, dy, dz) = (g[0] - at[0], g[1] - at[1], g[2] + 1.0 - at[2]);
            if math::hypotf(dx, dy) > 0.1 {
                self.camera.yaw = math::atan2f(-dx, dy);
                self.camera.pitch = math::atan2f(dz, math::hypotf(dx, dy)).clamp(-1.5, 1.5);
            }
            return;
        }
        let back = 9.0;
        // Back along the way the camera looks now, level.
        self.camera.position = Vec3::new(g[0] + sy * back, g[1] - cy * back, g[2] + 5.0);
        let (dx, dy, dz) = (
            g[0] - self.camera.position.x,
            g[1] - self.camera.position.y,
            g[2] + 1.0 - self.camera.position.z,
        );
        self.camera.yaw = math::atan2f(-dx, dy);
        self.camera.pitch = math::atan2f(dz, math::hypotf(dx, dy)).clamp(-1.5, 1.5);
    }

    /// Select a row, or none.
    pub fn select(&mut self, item: Option<String>) {
        self.selected = item;
        self.fields_of = None;
        self.held = None;
    }

    fn report(&mut self, r: Result<String, String>) -> Option<String> {
        match r {
            Ok(item) => Some(item),
            Err(e) => {
                self.status = e;
                None
            }
        }
    }

    /// Place a new row of the chosen kind at a point of the world's grid, facing `heading`.
    pub fn place_at(&mut self, g: [f32; 3], heading: f32) {
        let template = parse_id(&self.template);
        if self.new_kind != NewKind::Place && template.is_none() {
            self.status = format!("{:?} is not a template id (0x47...)", self.template);
            return;
        }
        let (lb, cell, at) = self.where_is(g);
        let r = self.session.place_in(
            &NewRow {
                kind: self.new_kind,
                template: template.unwrap_or(0),
                landblock: lb,
                at,
                heading,
            },
            cell,
        );
        if let Some(item) = self.report(r) {
            self.status = format!("placed {item}");
            self.select(Some(item));
        }
    }

    /// A point of the world's grid as a row would hold it: among the rooms the camera is in, their
    /// block and the room it falls in; else the block under it, and no cell of its own.
    fn where_is(&mut self, g: [f32; 3]) -> (u16, Option<u32>, [f32; 3]) {
        if let Some((lb, low)) = self.inside {
            let at = view::local_in(lb, g);
            // A floor's point lies on the room's edge: a little above it is in the room.
            let room = self
                .scene
                .cell_holding(lb, [at[0], at[1], at[2] + 0.2])
                .unwrap_or((u32::from(lb) << 16) | u32::from(low));
            return (lb, Some(room), at);
        }
        let (lb, at) = view::local(g);
        (lb, None, at)
    }

    /// The way the row being placed faces: toward the camera until R turns it.
    fn placing_heading(&self) -> f32 {
        self.place_heading
            .unwrap_or_else(|| (self.view().heading() + 180.0).rem_euclid(360.0))
    }

    /// The row being placed, where it would stand; none when the pointer is on no ground.
    #[must_use]
    pub const fn ghost(&self) -> Option<[f32; 3]> {
        self.ghost
    }

    /// Stop placing.
    fn stop_placing(&mut self) {
        self.tool = Tool::Select;
        self.ghost = None;
        self.place_heading = None;
    }

    /// What the row being placed would look like, when it has a look.
    fn ghost_standing(&mut self) -> Option<Standing> {
        let at = self.ghost.filter(|_| self.tool == Tool::Place)?;
        if self.new_kind == NewKind::Place {
            return None;
        }
        let (look, scale) = self.look(parse_id(&self.template)?)?;
        Some(Standing {
            look,
            at,
            rotation: view::heading_rotation(self.placing_heading()),
            scale,
        })
    }

    /// Move a row to a point of the world's grid.
    pub fn move_row(&mut self, item: &str, g: [f32; 3]) {
        let (lb, cell, at) = self.where_is(g);
        // Out from among rooms, a row stands in the land cell under it again.
        let indoors = self
            .session
            .entry(item)
            .is_some_and(|e| is_indoors(&e.row.row));
        let cell = cell
            .or_else(|| indoors.then(|| yalain_world_db::compile::outdoor_cell(lb, at[0], at[1])));
        let r = self.session.move_into(item, lb, at, cell);
        if let Some(moved) = self.report(r) {
            self.status = format!("moved {moved}");
            self.select(Some(moved));
        }
    }

    /// Turn the selected row by `by` degrees.
    pub fn turn_selected(&mut self, by: f32) {
        let Some(item) = self.selected.clone() else {
            return;
        };
        let Some(e) = self.session.entry(&item) else {
            return;
        };
        let h = heading_of(&e.row.row) + by;
        let r = self.session.turn(&item, h);
        self.report(r);
    }

    /// Copy the selected row a little to the camera's right, and select the copy.
    pub fn duplicate_selected(&mut self) {
        let Some(item) = self.selected.clone() else {
            return;
        };
        let Some(e) = self.session.entry(&item).cloned() else {
            return;
        };
        let g = entry_global(&e, &mut self.scene);
        let (sy, cy) = (math::sinf(self.camera.yaw), math::cosf(self.camera.yaw));
        let (x, y) = (g[0] + cy * DUPLICATE_AWAY, g[1] + sy * DUPLICATE_AWAY);
        // On the ground there, as far above it as the row stands above the ground here.
        let height = self.scene.ground(g[0], g[1]).map_or(0.0, |z| g[2] - z);
        let z = self.scene.ground(x, y).map_or(g[2], |z| z + height);
        let (x, y, z) = if self.snap.on {
            let [x, y, _] = self.snapped_on_ground([x, y, z]);
            let z = self.scene.ground(x, y).map_or(z, |z| z + height);
            (x, y, z)
        } else {
            (x, y, z)
        };
        let (lb, at) = view::local([x, y, z]);
        let r = self.session.duplicate(&item, lb, at);
        if let Some(copy) = self.report(r) {
            self.status = format!("copied {item} as {copy}");
            self.select(Some(copy));
        }
    }

    /// Take the selected row away.
    pub fn delete_selected(&mut self) {
        if let Some(item) = self.selected.clone() {
            match self.session.delete(&item) {
                Ok(()) => {
                    self.status = format!("took {item} away");
                    self.select(None);
                }
                Err(e) => self.status = e,
            }
        }
    }

    /// The window was asked to close: at once with nothing to lose, or once the panels have asked.
    pub fn ask_close(&mut self) {
        if self.session.unsaved() {
            self.closing = true;
        } else {
            self.close_now = true;
        }
    }

    /// Whether the panels are asking what to do with changes not saved.
    #[must_use]
    pub const fn closing(&self) -> bool {
        self.closing
    }

    /// What to do with changes not saved, when the window is to close.
    fn closing_window(&mut self, ctx: &egui::Context) {
        if !self.closing {
            return;
        }
        egui::Window::new("Close Athenaeum?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("This world has changes that are not saved.");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.save();
                        self.closing = false;
                        self.close_now = !self.session.unsaved();
                    }
                    if ui.button("Don't save").clicked() {
                        self.closing = false;
                        self.close_now = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.closing = false;
                    }
                });
            });
    }

    /// Save the world. (The editor's own Yalain has every change already, saved or not.)
    pub fn save(&mut self) {
        match self.session.save() {
            Ok(written) => self.status = format!("saved {} files", written.len()),
            Err(e) => self.status = e,
        }
    }

    /// Open the template finder on `text`.
    pub fn find(&mut self, text: &str) {
        self.finding = Some(text.to_string());
        self.focus_find = true;
    }

    /// The install's templates, for the finder.
    pub fn set_templates(&mut self, templates: Vec<u32>) {
        self.templates = templates;
    }

    /// Build the world against the install, as the server will at start, and keep what it says of
    /// the world's own rows.
    pub fn build_check(&mut self) {
        let files = self.session.files().clone();
        let base = self.session.base();
        let (db, read) = layer::read(base, &files);
        let ours: Vec<String> = db
            .as_ref()
            .map(|d| d.packs.iter().map(|p| format!("{}/", p.folder)).collect())
            .unwrap_or_default();
        let mine = |p: &Problem| ours.iter().any(|f| p.file.starts_with(f.as_str()));
        let rev = self.session.revision();
        let Some(db) = db.filter(|_| read.is_empty()) else {
            self.build = Some((rev, read, Vec::new(), "the world does not read".into()));
            return;
        };
        let merged = match layer::merge_over(base, &db) {
            Ok(m) => m,
            Err(p) => {
                self.build = Some((rev, p, Vec::new(), "the world does not merge".into()));
                return;
            }
        };
        self.build = Some(
            match yalain_world_db::compile::compile(&merged, self.install) {
                Ok(t) => {
                    let warnings: Vec<Problem> =
                        t.warnings.iter().filter(|w| mine(w)).cloned().collect();
                    let counts = t
                        .counts()
                        .iter()
                        .map(|(k, n)| format!("{k} {n}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    (rev, Vec::new(), warnings, format!("builds: {counts}"))
                }
                Err(p) => {
                    let n = p.len();
                    (rev, p, Vec::new(), format!("does not build: {n} problems"))
                }
            },
        );
        if let Some((_, _, _, said)) = &self.build {
            self.status.clone_from(said);
        }
    }

    /// Open the Find window, or close it.
    pub fn toggle_row_finder(&mut self) {
        self.finding_rows = match self.finding_rows.take() {
            Some(_) => None,
            None => Some(String::new()),
        };
        self.focus_find = self.finding_rows.is_some();
    }

    /// What the Find window reads each row by: its item, kind, template and name, in small letters.
    fn row_words(&mut self) -> Vec<String> {
        let rev = self.session.revision();
        if let Some((r, words)) = &self.row_index {
            if *r == rev {
                return words.clone();
            }
        }
        let entries: Vec<Entry> = self.session.entries().to_vec();
        let words: Vec<String> = entries
            .iter()
            .map(|e| {
                let (template, more) = match &e.row.row {
                    Row::Object(o) => (Some(o.template), o.to.clone().unwrap_or_default()),
                    Row::Camp(c) => (c.spawns.first().map(|(t, _)| *t), String::new()),
                    Row::Place(p) => (None, p.destination.clone().unwrap_or_default()),
                    _ => (None, String::new()),
                };
                let name = template.map_or_else(String::new, |t| self.label(t));
                let hex = template.map_or_else(String::new, |t| format!("0x{t:08x}"));
                format!("{} {} {name} {hex} {more}", e.item, kind_word(&e.row.row)).to_lowercase()
            })
            .collect();
        self.row_index = Some((rev, words.clone()));
        words
    }

    /// The indexes of the rows whose words hold every word of `text`, the world's own first, at
    /// most `limit`.
    #[must_use]
    pub fn rows_matching(&mut self, text: &str, limit: usize) -> Vec<usize> {
        let want: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        if want.is_empty() {
            return Vec::new();
        }
        let words = self.row_words();
        let hits = words
            .iter()
            .enumerate()
            .filter(|(_, w)| want.iter().all(|t| w.contains(t.as_str())))
            .map(|(i, _)| i);
        let entries = self.session.entries();
        let (own, rest): (Vec<usize>, Vec<usize>) =
            hits.partition(|i| entries[*i].layer != Layer::Base);
        own.into_iter().chain(rest).take(limit).collect()
    }

    /// The camera over a landblock's point, a little above the ground, looking as it was.
    pub fn go_to(&mut self, lb: u16, at: [f32; 2]) {
        let g = view::global(lb, [at[0], at[1], 0.0]);
        let z = self
            .scene
            .ground(g[0], g[1])
            .unwrap_or(self.camera.position.z);
        self.camera.position = Vec3::new(g[0], g[1], z + 20.0);
        self.home = None;
    }

    /// The Find window: any row of the world by its name, template, kind or key, and a landblock to
    /// go to.
    fn row_finder(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.finding_rows.take() else {
            return;
        };
        let mut open = true;
        let mut pick = None;
        let mut go = None;
        egui::Window::new("Find")
            .open(&mut open)
            .default_size([420.0, 440.0])
            .default_pos([340.0, 80.0])
            .show(ctx, |ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text(
                            "a name, a template, a kind, a key, or a landblock (5766:123,45)",
                        )
                        .desired_width(400.0),
                );
                // Opened, it is typed in at once.
                if std::mem::take(&mut self.focus_find) {
                    field.request_focus();
                }
                if let Some((lb, at)) = go_to_of(&text) {
                    if ui
                        .button(format!("Go to {lb:04X} ({:.0}, {:.0})", at[0], at[1]))
                        .clicked()
                    {
                        go = Some((lb, at));
                    }
                }
                let found = self.rows_matching(&text, FOUND);
                if found.is_empty() {
                    if !text.trim().is_empty() {
                        ui.label(egui::RichText::new("no row holds that").weak());
                    }
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for i in &found {
                        let Some(e) = self.session.entries().get(*i).cloned() else {
                            continue;
                        };
                        let name = match &e.row.row {
                            Row::Object(o) => self.label(o.template),
                            Row::Camp(c) => c
                                .spawns
                                .first()
                                .map(|(t, _)| self.label(*t))
                                .unwrap_or_default(),
                            _ => String::new(),
                        };
                        let line = egui::RichText::new(format!("{}  {name}", e.item))
                            .color(color_of(&e.row.row));
                        if ui.selectable_label(false, line).clicked() {
                            pick = Some(e.item.clone());
                        }
                    }
                    if found.len() >= FOUND {
                        ui.label(egui::RichText::new("and more: type more").weak());
                    }
                });
            });
        if let Some(item) = pick {
            self.select(Some(item.clone()));
            self.fly_to(&item);
        }
        if let Some((lb, at)) = go {
            self.go_to(lb, at);
        }
        if open {
            self.finding_rows = Some(text);
        }
    }

    /// The finder: templates whose names hold what is typed.
    fn finder(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.finding.take() else {
            return;
        };
        let mut open = true;
        let mut chosen = None;
        egui::Window::new("Templates")
            .open(&mut open)
            .default_size([360.0, 420.0])
            .default_pos([340.0, 80.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("name or id has");
                    let field = ui.add(egui::TextEdit::singleline(&mut text).desired_width(200.0));
                    if std::mem::take(&mut self.focus_find) {
                        field.request_focus();
                    }
                });
                let want = text.trim().to_lowercase();
                if want.len() < 2 {
                    ui.label(egui::RichText::new("type two letters or more").weak());
                    return;
                }
                let templates = self.templates.clone();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut shown = 0;
                    for t in templates {
                        let name = self.label(t);
                        let hex = format!("0x{t:08x}");
                        if !name.to_lowercase().contains(&want) && !hex.contains(&want) {
                            continue;
                        }
                        // One with no look shows only as its marker in the world.
                        let line = if self.look(t).is_some() {
                            format!("0x{t:08X}  {name}")
                        } else {
                            format!("0x{t:08X}  {name}  (no look)")
                        };
                        if ui.selectable_label(false, line).clicked() {
                            chosen = Some(t);
                        }
                        shown += 1;
                        if shown >= 200 {
                            ui.label(egui::RichText::new("and more: type more of the name").weak());
                            break;
                        }
                    }
                });
            });
        if let Some(t) = chosen {
            self.template = format!("0x{t:08X}");
            self.tool = Tool::Place;
            self.status = "click the ground to place it".into();
        } else if open {
            self.finding = Some(text);
        }
    }

    /// Keep the camera's place for the next session.
    pub fn keep_camera(&self) {
        let c = &self.camera;
        let file = camera_file(self.session.dir());
        if let Some(d) = file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        #[allow(clippy::cast_precision_loss)]
        let text = format!(
            "{} {} {} {} {} {}\n",
            c.position.x, c.position.y, c.position.z, c.yaw, c.pitch, c.speed as f32
        );
        if let Err(e) = std::fs::write(&file, text) {
            tracing::warn!("{}: {e}", file.display());
        }
    }

    /// Where a row put down at a block's point stands: on the floor under it among the rooms the
    /// camera is in, else on the ground.
    #[must_use]
    pub fn standing_at(&mut self, lb: u16, x: f32, y: f32) -> [f32; 3] {
        let g = view::global(lb, [x, y, 0.0]);
        if let Some((home, low)) = self.inside.filter(|(home, _)| *home == lb) {
            let z = self.camera.position.z;
            let from = view::local_in(home, [g[0], g[1], z]);
            if let Some((_, on)) = self.scene.floor_seen(home, low, from, [0.0, 0.0, -1.0]) {
                return view::global(home, on);
            }
        }
        let z = self.scene.ground(g[0], g[1]).unwrap_or(0.0);
        [g[0], g[1], z]
    }

    /// Where a player enters: on the ground under the camera, facing as it faces.
    pub fn entry_point(&mut self) -> live::Entry {
        let p = self.camera.position;
        if let Some((lb, low)) = self.inside {
            // On the floor under the camera when there is one in sight of it.
            let eye = view::local_in(lb, [p.x, p.y, p.z]);
            let down = self.scene.floor_seen(lb, low, eye, [0.0, 0.0, -1.0]);
            let cell = down.map_or((u32::from(lb) << 16) | u32::from(low), |d| d.0);
            let at = down.map_or(eye, |d| d.1);
            return live::Entry {
                cell,
                at: [at[0], at[1], at[2] + 0.5],
                heading: self.view().heading(),
            };
        }
        let z = self.scene.ground(p.x, p.y).unwrap_or(p.z);
        let (lb, at) = view::local([p.x, p.y, z]);
        let v = self.view();
        live::Entry {
            cell: yalain_world_db::compile::outdoor_cell(lb, at[0], at[1]),
            at: [at[0], at[1], z + 0.5],
            heading: v.heading(),
        }
    }

    /// The bodies standing near the camera: people and objects by their templates' looks, and
    /// each camp's first creature at its middle.
    fn standing(&mut self) -> Vec<Standing> {
        let mut out = Vec::new();
        let near: Vec<(usize, f32)> = self.near.clone();
        for (i, d) in near {
            if d > NEAR || out.len() >= 400 {
                break;
            }
            let Some(e) = self.session.entries().get(i).cloned() else {
                continue;
            };
            if !self.shows(&e) {
                continue;
            }
            let template = match &e.row.row {
                Row::Object(o) => o.template,
                Row::Camp(c) => match c.spawns.first() {
                    Some((t, _)) => *t,
                    None => continue,
                },
                _ => continue,
            };
            let Some((look, scale)) = self.look(template) else {
                continue;
            };
            let (held, facing) = self.held_as(&e.item);
            let at = held.unwrap_or_else(|| entry_global(&e, &mut self.scene));
            out.push(Standing {
                look,
                at,
                rotation: facing.map_or_else(|| rotation_of(&e.row.row), view::heading_rotation),
                scale,
            });
        }
        out.extend(self.ghost_standing());
        out
    }

    /// The nearest row whose body the pointer's line of sight goes through.
    fn body_at(&mut self, px: [f32; 2]) -> Option<String> {
        let v = self.view();
        let dir = v.ray(px);
        let mut best: Option<(f32, String)> = None;
        for (i, d) in self.near.clone() {
            if d > NEAR {
                break;
            }
            let Some(e) = self.session.entries().get(i).cloned() else {
                continue;
            };
            if !self.shows(&e) {
                continue;
            }
            let template = match &e.row.row {
                Row::Object(o) => o.template,
                Row::Camp(c) => match c.spawns.first() {
                    Some((t, _)) => *t,
                    None => continue,
                },
                _ => continue,
            };
            let Some((look, scale)) = self.look(template) else {
                continue;
            };
            let Some((lo, hi)) = self.scene.look_box(look) else {
                continue;
            };
            let g = entry_global(&e, &mut self.scene);
            let q = rotation_of(&e.row.row);
            let s = scale.max(0.01);
            let to = view::unrotated(q, [v.eye[0] - g[0], v.eye[1] - g[1], v.eye[2] - g[2]]);
            let along = view::unrotated(q, dir);
            let from = [to[0] / s, to[1] / s, to[2] / s];
            let along = [along[0] / s, along[1] / s, along[2] / s];
            if let Some(t) = view::ray_box(from, along, lo, hi) {
                if best.as_ref().is_none_or(|b| t < b.0) {
                    best = Some((t, e.item.clone()));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// The row a click at a pixel takes: the marker there, else the body.
    fn pick(&mut self, px: [f32; 2]) -> Option<String> {
        self.hit(px).or_else(|| self.body_at(px))
    }

    /// The row whose marker is at a pixel, as a click there would find it.
    pub fn hovered_item(&mut self, px: [f32; 2]) -> Option<String> {
        self.near_at = None;
        self.near();
        self.hit(px)
    }

    /// The marker nearest a pixel, within [`HIT`].
    fn hit(&mut self, px: [f32; 2]) -> Option<String> {
        let v = self.view();
        let mut best: Option<(f32, String)> = None;
        let near = self.near.clone();
        for (i, _) in near {
            let Some(e) = self.session.entries().get(i).cloned() else {
                continue;
            };
            if !self.shows(&e) {
                continue;
            }
            let g = entry_global(&e, &mut self.scene);
            let Some(p) = v.project([g[0], g[1], g[2] + 1.0]) else {
                continue;
            };
            let d = math::hypotf(p[0] - px[0], p[1] - px[1]);
            if d <= HIT && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, e.item.clone()));
            }
        }
        best.map(|b| b.1)
    }

    /// The ground under a pixel, on the world's grid.
    fn ground_at(&mut self, px: [f32; 2]) -> Option<[f32; 3]> {
        let v = self.view();
        let dir = v.ray(px);
        if let Some((lb, low)) = self.inside {
            let from = view::local_in(lb, v.eye);
            let (_, on) = self.scene.floor_seen(lb, low, from, dir)?;
            return Some(view::global(lb, on));
        }
        let scene = &mut self.scene;
        view::hit_ground(v.eye, dir, 2000.0, 2.0, &mut |x, y| scene.ground(x, y))
    }

    /// The name of the row under the pointer, to show beside it.
    fn hover_name(&mut self, item: &str) -> String {
        let Some(e) = self.session.entry(item).cloned() else {
            return String::new();
        };
        let name = match &e.row.row {
            Row::Object(o) => self.label(o.template),
            Row::Camp(c) => c
                .spawns
                .first()
                .map(|(t, _)| format!("camp of {}", self.label(*t)))
                .unwrap_or_default(),
            _ => String::new(),
        };
        format!("{}  {name}", kind_word(&e.row.row))
    }

    /// The world's input this frame: the camera, selecting, dragging, placing, the keys.
    pub fn world_input(&mut self, input: &ViewInput, over_panels: bool, typing: bool, dt: f32) {
        self.find_inside();
        self.near();
        self.hovered = if over_panels || input.right_down || self.tool == Tool::Place {
            None
        } else {
            self.pick(input.pointer)
                .filter(|i| self.selected.as_deref() != Some(i))
        };
        if input.right_down && !over_panels {
            self.camera.look(input.motion.0, input.motion.1);
        }
        if !typing {
            let k = |c: KeyCode| input.down.contains(&c) && !input.ctrl;
            self.camera.fly(
                Held {
                    forward: k(KeyCode::KeyW),
                    back: k(KeyCode::KeyS),
                    left: k(KeyCode::KeyA),
                    right: k(KeyCode::KeyD),
                    up: k(KeyCode::KeyE),
                    down: k(KeyCode::KeyQ),
                },
                dt,
            );
            for code in &input.pressed {
                match code {
                    KeyCode::KeyS if input.ctrl => self.save(),
                    KeyCode::KeyZ if input.ctrl && input.shift => {
                        self.session.redo();
                    }
                    KeyCode::KeyZ if input.ctrl => {
                        self.session.undo();
                    }
                    KeyCode::KeyY if input.ctrl => {
                        self.session.redo();
                    }
                    KeyCode::KeyD if input.ctrl => self.duplicate_selected(),
                    KeyCode::KeyF if input.ctrl => self.toggle_row_finder(),
                    KeyCode::KeyR if !input.ctrl => {
                        let step = self.snap.turn_step(TURN_STEP);
                        let by = if input.shift { -step } else { step };
                        if self.tool == Tool::Place {
                            self.place_heading = Some(self.placing_heading() + by);
                        } else {
                            self.turn_selected(by);
                        }
                    }
                    KeyCode::KeyF if !input.ctrl => {
                        if let Some(item) = self.selected.clone() {
                            self.fly_to(&item);
                        }
                    }
                    KeyCode::KeyT if !input.ctrl => self.teleport_to_camera(),
                    // A Mac's keyboard calls Backspace delete.
                    KeyCode::Delete | KeyCode::Backspace => self.delete_selected(),
                    KeyCode::Escape => {
                        if self.tool == Tool::Place {
                            self.stop_placing();
                        } else if self.grip.take().is_none() && self.drag.take().is_none() {
                            self.select(None);
                        }
                    }
                    _ => {}
                }
            }
        }
        if !over_panels && input.wheel != 0.0 {
            self.camera
                .change_speed(dereth_primitives::num::to_i32(input.wheel.signum()));
        }
        self.ghost = if self.tool == Tool::Place && !over_panels {
            self.ground_at(input.pointer)
        } else {
            None
        };
        // Selecting, dragging and placing with the left button.
        if input.left_pressed && !over_panels {
            match self.tool {
                Tool::Place => {
                    if let Some(g) = self.ghost {
                        let facing = self.placing_heading();
                        self.place_at(g, facing);
                        // Shift keeps placing, facing the same way.
                        if input.shift {
                            self.place_heading = Some(facing);
                        } else {
                            self.stop_placing();
                        }
                    } else {
                        self.status = "no ground there".into();
                    }
                }
                Tool::Select => {
                    if !self.take_handle(input.pointer) {
                        match self.pick(input.pointer) {
                            Some(item) => {
                                self.select(Some(item.clone()));
                                self.drag = Some(Drag {
                                    item,
                                    from: input.pointer,
                                    moved: false,
                                    to: None,
                                });
                            }
                            None => self.select(None),
                        }
                    }
                }
            }
        }
        if let Some(mut d) = self.drag.take() {
            if input.left_down {
                let far = math::hypotf(input.pointer[0] - d.from[0], input.pointer[1] - d.from[1]);
                if far > 4.0 {
                    d.moved = true;
                }
                if d.moved {
                    d.to = self.ground_at(input.pointer).or(d.to);
                    if self.snap.applies(input.shift) {
                        d.to = d.to.map(|t| self.snapped_on_ground(t));
                    }
                }
                self.drag = Some(d);
            } else if d.moved {
                if let Some(to) = d.to {
                    self.move_row(&d.item, to);
                }
            }
        }
        self.hold_handle(input);
    }

    /// A row's handles' place and whether it can turn, when the row is selected.
    fn selected_handles(&mut self) -> Option<(Entry, [f32; 3], bool)> {
        let e = self.session.entry(self.selected.as_deref()?)?.clone();
        if !self.shows(&e) {
            return None;
        }
        let at = entry_global(&e, &mut self.scene);
        let turns = matches!(e.row.row, Row::Object(_) | Row::Place(_));
        Some((e, at, turns))
    }

    /// The pointer's line of sight through a pixel.
    fn ray_at(&self, px: [f32; 2]) -> Ray {
        let v = self.view();
        Ray {
            origin: v.eye,
            dir: v.ray(px),
        }
    }

    /// A press on a handle of the selected row takes it.
    fn take_handle(&mut self, pointer: [f32; 2]) -> bool {
        let Some((e, at, turns)) = self.selected_handles() else {
            return false;
        };
        let v = self.view();
        let Some(handle) =
            gizmo::handles(&v, at, turns).and_then(|h| gizmo::handle_at(&h, pointer))
        else {
            return false;
        };
        let heading = heading_of(&e.row.row);
        self.grip = Some(Grip {
            item: e.item,
            handle,
            origin: at,
            heading,
            from: self.ray_at(pointer),
            at,
            facing: heading,
        });
        true
    }

    /// A handle held follows the pointer; let go, the row takes what it became.
    fn hold_handle(&mut self, input: &ViewInput) {
        let Some(mut g) = self.grip.take() else {
            return;
        };
        if input.left_down {
            let to = self.ray_at(input.pointer);
            let snap = self.snap.applies(input.shift);
            match g.handle {
                Handle::Turn => {
                    if let Some(by) = gizmo::drag_turn(g.origin, &g.from, &to) {
                        let h = g.heading + by;
                        let h = if snap {
                            gizmo::snapped(h, self.snap.angle)
                        } else {
                            h
                        };
                        g.facing = h.rem_euclid(360.0);
                    }
                }
                arrow => {
                    if let (Some(by), Some(axis)) = (
                        gizmo::drag_along(arrow, g.origin, &g.from, &to),
                        arrow.axis(),
                    ) {
                        for ((at, from), along) in g.at.iter_mut().zip(g.origin).zip(axis) {
                            *at = from + along * by;
                            if snap && along != 0.0 {
                                *at = gizmo::snapped(*at, self.snap.grid);
                            }
                        }
                    }
                }
            }
            self.grip = Some(g);
        } else if g.handle == Handle::Turn {
            if (g.facing - g.heading).abs() > f32::EPSILON {
                let r = self.session.turn(&g.item, g.facing);
                self.report(r);
            }
        } else if g.at != g.origin {
            self.move_row(&g.item, g.at);
        }
    }

    /// A point of the ground at the nearest whole number of the grid, on the ground there.
    fn snapped_on_ground(&mut self, g: [f32; 3]) -> [f32; 3] {
        let (x, y) = (
            gizmo::snapped(g[0], self.snap.grid),
            gizmo::snapped(g[1], self.snap.grid),
        );
        let z = if self.inside.is_some() {
            g[2]
        } else {
            self.scene.ground(x, y).unwrap_or(g[2])
        };
        [x, y, z]
    }

    /// What a row would become if what is held were let go: where it stands and which way it faces.
    fn held_as(&self, item: &str) -> (Option<[f32; 3]>, Option<f32>) {
        if let Some(g) = self.grip.as_ref().filter(|g| g.item == item) {
            return (Some(g.at), Some(g.facing));
        }
        let at = self
            .drag
            .as_ref()
            .filter(|d| d.item == item && d.moved)
            .and_then(|d| d.to);
        (at, None)
    }

    /// Draw the selected row's handles.
    fn handles_mark(&mut self, painter: &egui::Painter) {
        let Some((e, at, turns)) = self.selected_handles() else {
            return;
        };
        let (held, _) = self.held_as(&e.item);
        let v = self.view();
        let Some(h) = gizmo::handles(&v, held.unwrap_or(at), turns) else {
            return;
        };
        let s = self.scale;
        let pt = |p: [f32; 2]| egui::pos2(p[0] / s, p[1] / s);
        let taken = self.grip.as_ref().map(|g| g.handle);
        let width = |k: Handle| if taken == Some(k) { 4.0 } else { 2.5 };
        for (k, tip) in &h.arrows {
            let color = match k {
                Handle::X => egui::Color32::from_rgb(240, 80, 80),
                Handle::Y => egui::Color32::from_rgb(90, 220, 90),
                _ => egui::Color32::from_rgb(100, 140, 255),
            };
            painter.line_segment(
                [pt(h.origin), pt(*tip)],
                egui::Stroke::new(width(*k), color),
            );
            painter.circle_filled(pt(*tip), 4.0, color);
        }
        for w in h.ring.windows(2) {
            painter.line_segment(
                [pt(w[0]), pt(w[1])],
                egui::Stroke::new(width(Handle::Turn), egui::Color32::from_rgb(255, 220, 90)),
            );
        }
    }

    /// Mark the rows where they stand: a dot of the kind's color with its source's letter, a
    /// camp's reach, a portal's way to where it leads, the selected row ringed and named.
    #[allow(clippy::too_many_lines)] // one pass over the rows near the camera
    pub fn markers(&mut self, painter: &egui::Painter) {
        let v = self.view();
        let s = self.scale;
        let pt = |p: [f32; 2]| egui::pos2(p[0] / s, p[1] / s);
        let near = self.near.clone();
        let font = egui::FontId::proportional(11.0);
        for (i, d) in near {
            let Some(e) = self.session.entries().get(i).cloned() else {
                continue;
            };
            if !self.shows(&e) {
                continue;
            }
            let mut g = entry_global(&e, &mut self.scene);
            if let (Some(to), _) = self.held_as(&e.item) {
                g = to;
            }
            let selected = self.selected.as_deref() == Some(e.item.as_str());
            let color = color_of(&e.row.row);
            if let Row::Camp(c) = &e.row.row {
                if d < NEAR || selected {
                    let steps = 32;
                    let mut last: Option<egui::Pos2> = None;
                    for k in 0..=steps {
                        #[allow(clippy::cast_precision_loss)]
                        let a = k as f32 / steps as f32 * std::f32::consts::TAU;
                        let (x, y) = (
                            g[0] + c.radius * math::cosf(a),
                            g[1] + c.radius * math::sinf(a),
                        );
                        let z = self.scene.ground(x, y).unwrap_or(g[2]) + 0.2;
                        let p = v.project([x, y, z]).map(pt);
                        if let (Some(a), Some(b)) = (last, p) {
                            painter.line_segment([a, b], egui::Stroke::new(1.5, color));
                        }
                        last = p;
                    }
                }
            }
            if let Row::Object(o) = &e.row.row {
                if let Some(to) = &o.to {
                    if let Some(p) = self.place_of(to, &e.row.pack) {
                        let pg = entry_global(&p, &mut self.scene);
                        let (a, b) = (
                            v.project([g[0], g[1], g[2] + 1.0]),
                            v.project([pg[0], pg[1], pg[2] + 1.0]),
                        );
                        if let (Some(a), Some(b)) = (a, b) {
                            painter.line_segment(
                                [pt(a), pt(b)],
                                egui::Stroke::new(
                                    if selected { 2.5 } else { 1.0 },
                                    egui::Color32::from_rgba_unmultiplied(200, 130, 255, 180),
                                ),
                            );
                        }
                    }
                }
            }
            let Some(p) = v.project([g[0], g[1], g[2] + 1.0]) else {
                continue;
            };
            let p = pt(p);
            let r = if d < 60.0 { 6.0 } else { 4.0 };
            painter.circle_filled(p, r, color);
            painter.circle_stroke(p, r, egui::Stroke::new(1.0, egui::Color32::BLACK));
            if e.layer != Layer::Base {
                painter.circle_stroke(p, r + 2.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if d < 60.0 || selected {
                let (letter, _) = badge(e.prov());
                painter.text(
                    p + egui::vec2(r + 3.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    letter,
                    font.clone(),
                    egui::Color32::WHITE,
                );
            }
            if selected {
                painter.circle_stroke(
                    p,
                    r + 6.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 255, 120)),
                );
                let name = match &e.row.row {
                    Row::Object(o) => {
                        let l = self.label(o.template);
                        format!("{}  {l}", e.item)
                    }
                    _ => e.item.clone(),
                };
                painter.text(
                    p + egui::vec2(0.0, -r - 10.0),
                    egui::Align2::CENTER_BOTTOM,
                    name,
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_rgb(255, 255, 160),
                );
            }
        }
        self.ghost_mark(painter);
        self.handles_mark(painter);
        if let Some(item) = self.hovered.clone() {
            let name = self.hover_name(&item);
            if let Some(e) = self.session.entry(&item).cloned() {
                let g = entry_global(&e, &mut self.scene);
                if let Some(p) = v.project([g[0], g[1], g[2] + 1.0]) {
                    painter.text(
                        pt(p) + egui::vec2(10.0, -8.0),
                        egui::Align2::LEFT_BOTTOM,
                        name,
                        egui::FontId::proportional(13.0),
                        egui::Color32::WHITE,
                    );
                }
            }
        }
    }

    /// Mark the row being placed: a ring where it would stand, a line the way it faces, what to do.
    fn ghost_mark(&mut self, painter: &egui::Painter) {
        let Some(g) = self.ghost else {
            return;
        };
        let v = self.view();
        let s = self.scale;
        let Some(p) = v.project([g[0], g[1], g[2] + 1.0]) else {
            return;
        };
        let p = egui::pos2(p[0] / s, p[1] / s);
        let color = egui::Color32::from_rgb(255, 255, 120);
        painter.circle_stroke(p, 8.0, egui::Stroke::new(2.0, color));
        let h = self.placing_heading().to_radians();
        let ahead = [
            g[0] + 2.0 * math::sinf(h),
            g[1] + 2.0 * math::cosf(h),
            g[2] + 1.0,
        ];
        if let Some(a) = v.project(ahead) {
            painter.line_segment(
                [p, egui::pos2(a[0] / s, a[1] / s)],
                egui::Stroke::new(2.0, color),
            );
        }
        let (look, word) = match parse_id(&self.template) {
            Some(t) if self.new_kind != NewKind::Place => (self.look(t).is_some(), self.label(t)),
            _ => (true, self.new_kind.word().to_string()),
        };
        let text = if look {
            format!("{word}: click to place, Shift to keep placing, R to turn, Esc to stop")
        } else {
            format!("{word} has no look of its own: it will show only as a marker")
        };
        painter.text(
            p + egui::vec2(0.0, -16.0),
            egui::Align2::CENTER_BOTTOM,
            text,
            egui::FontId::proportional(13.0),
            color,
        );
    }

    /// The place a portal's `to` names (`key`, or `pack:key`), from a row of `pack`.
    fn place_of(&self, to: &str, pack: &str) -> Option<Entry> {
        let (pack, key) = to.split_once(':').unwrap_or((pack, to));
        self.session
            .entries()
            .iter()
            .find(|x| matches!(&x.row.row, Row::Place(p) if p.key == key) && x.row.pack == pack)
            .cloned()
    }

    /// The world's frame from the camera, with the rows near it standing in it.
    pub fn frame(&mut self) -> &auberean_draw::scene::Frame {
        self.find_inside();
        self.near();
        let rows = self.standing();
        let v = self.view();
        let inside = self.inside;
        self.scene.frame(&v, &rows, inside)
    }

    /// The panels.
    #[allow(clippy::too_many_lines)] // the editor's chrome, panel by panel
    pub fn panels(&mut self, ui: &mut egui::Ui) {
        self.live_frame();
        let serving = self.live.serving();
        egui::Panel::top(egui::Id::new("top")).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Athenaeum");
                ui.label(format!(
                    "{}  ·  editing {}",
                    self.session
                        .dir()
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().to_string()),
                    self.session.target()
                ));
                ui.separator();
                let (undo, redo) = self.session.steps();
                if ui
                    .add_enabled(self.session.unsaved(), egui::Button::new("Save"))
                    .on_hover_text("Ctrl+S")
                    .clicked()
                {
                    self.save();
                }
                if ui
                    .add_enabled(undo > 0, egui::Button::new("Undo"))
                    .clicked()
                {
                    self.session.undo();
                }
                if ui
                    .add_enabled(redo > 0, egui::Button::new("Redo"))
                    .clicked()
                {
                    self.session.redo();
                }
                ui.separator();
                egui::ComboBox::from_id_salt("new kind")
                    .selected_text(self.new_kind.word())
                    .show_ui(ui, |ui| {
                        for k in [
                            NewKind::Person,
                            NewKind::Static,
                            NewKind::Camp,
                            NewKind::Place,
                        ] {
                            ui.selectable_value(&mut self.new_kind, k, k.word());
                        }
                    });
                if self.new_kind != NewKind::Place {
                    ui.add(egui::TextEdit::singleline(&mut self.template).desired_width(90.0))
                        .on_hover_text(
                            "the template: a person's, an object's, or the creature a camp keeps",
                        );
                    if let Some(t) = parse_id(&self.template) {
                        let l = self.label(t);
                        ui.label(egui::RichText::new(l).weak());
                    }
                    if ui
                        .button("Find")
                        .on_hover_text("a template by its name")
                        .clicked()
                    {
                        self.finding = Some(self.finding.take().unwrap_or_default());
                        self.focus_find = true;
                    }
                }
                let placing = self.tool == Tool::Place;
                if ui
                    .selectable_label(placing, "Place")
                    .on_hover_text("then click the ground; Shift keeps placing, R turns, Esc stops")
                    .clicked()
                {
                    if placing {
                        self.stop_placing();
                    } else {
                        self.tool = Tool::Place;
                    }
                }
                ui.separator();
                if ui
                    .button("Find rows")
                    .on_hover_text("any row of the world, or a landblock to go to (Ctrl+F)")
                    .clicked()
                {
                    self.toggle_row_finder();
                }
                ui.separator();
                ui.checkbox(&mut self.snap.on, "Snap").on_hover_text(
                    "moves and turns go by steps; Shift while dragging does the other",
                );
                egui::ComboBox::from_id_salt("snap grid")
                    .selected_text(format!("{} m", self.snap.grid))
                    .width(54.0)
                    .show_ui(ui, |ui| {
                        for g in [0.25, 0.5, 1.0, 2.0, 5.0, 10.0] {
                            ui.selectable_value(&mut self.snap.grid, g, format!("{g} m"));
                        }
                    });
                egui::ComboBox::from_id_salt("snap angle")
                    .selected_text(format!("{}°", self.snap.angle))
                    .width(44.0)
                    .show_ui(ui, |ui| {
                        for a in [5.0, 15.0, 30.0, 45.0, 90.0] {
                            ui.selectable_value(&mut self.snap.angle, a, format!("{a}°"));
                        }
                    });
                ui.separator();
                if ui
                    .button("Check build")
                    .on_hover_text("build the world against the install, as Yalain will")
                    .clicked()
                {
                    self.build_check();
                }
                ui.separator();
                ui.label("Time");
                ui.add(egui::Slider::new(&mut self.scene.time_of_day, 0.0..=1.0).show_value(false));
                ui.separator();
                self.play_bar(ui);
            });
        });
        egui::Panel::bottom(egui::Id::new("status")).show(ui, |ui| self.status_panel(ui, serving));
        egui::Panel::left(egui::Id::new("rows"))
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| self.rows_panel(ui));
        egui::Panel::right(egui::Id::new("inspector"))
            .resizable(true)
            .default_size(330.0)
            .show(ui, |ui| self.inspector(ui));
        let ctx = ui.ctx().clone();
        self.finder(&ctx);
        self.row_finder(&ctx);
        self.characters_window(&ctx);
        self.closing_window(&ctx);
    }

    fn status_panel(&mut self, ui: &mut egui::Ui, serving: bool) {
        // A build of an older revision says nothing of the world as it stands.
        if self
            .build
            .as_ref()
            .is_some_and(|b| b.0 != self.session.revision())
        {
            self.build = None;
        }
        let (built_problems, built_warnings) = self
            .build
            .as_ref()
            .map_or((Vec::new(), Vec::new()), |b| (b.1.clone(), b.2.clone()));
        let problems = self.session.problems().len() + built_problems.len();
        let warnings = self.session.warnings().len() + built_warnings.len();
        // New problems open the list.
        let open = (problems > self.problems_seen).then_some(true);
        self.problems_seen = problems;
        egui::CollapsingHeader::new(format!("Problems: {problems}   Warnings: {warnings}"))
            .id_salt("problems")
            .open(open)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        let mut go = None;
                        for p in self.session.problems().iter().chain(&built_problems) {
                            let text = egui::RichText::new(p.to_string())
                                .color(egui::Color32::from_rgb(255, 140, 120));
                            if ui
                                .add(egui::Label::new(text).sense(egui::Sense::click()))
                                .clicked()
                            {
                                go = self
                                    .session
                                    .entries()
                                    .iter()
                                    .find(|e| e.row.file == p.file && e.row.line == p.line)
                                    .map(|e| e.item.clone());
                            }
                        }
                        for w in built_warnings
                            .iter()
                            .chain(self.session.warnings())
                            .take(200)
                        {
                            ui.label(egui::RichText::new(w.to_string()).weak());
                        }
                        if let Some(item) = go {
                            self.select(Some(item.clone()));
                            self.fly_to(&item);
                        }
                    });
            });
        let p = self.camera.position;
        let (lb, at) = view::local([p.x, p.y, p.z]);
        let (undo, redo) = self.session.steps();
        ui.horizontal_wrapped(|ui| {
            match self.inside {
                Some((home, low)) => {
                    let at = view::local_in(home, [p.x, p.y, p.z]);
                    ui.label(format!(
                        "in room {home:04X}{low:04X} ({:.1}, {:.1}, {:.1})  {:.0} m/s",
                        at[0],
                        at[1],
                        at[2],
                        self.camera.meters_per_second()
                    ));
                }
                None => {
                    ui.label(format!(
                        "{lb:04X} ({:.1}, {:.1}, {:.1})  {:.0} m/s",
                        at[0],
                        at[1],
                        at[2],
                        self.camera.meters_per_second()
                    ));
                }
            }
            ui.separator();
            ui.label(format!(
                "{} rows{}  ·  undo {undo}, redo {redo}",
                self.session.entries().len(),
                if self.session.unsaved() {
                    "  ·  unsaved"
                } else {
                    ""
                }
            ));
            ui.separator();
            if serving {
                ui.label(
                    egui::RichText::new(&self.live.said)
                        .color(egui::Color32::from_rgb(140, 220, 140)),
                );
                if self.status == self.live.said {
                    return;
                }
                ui.separator();
            }
            ui.label(&self.status);
        });
    }

    fn rows_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Rows near");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.shown.people, "people");
            ui.checkbox(&mut self.shown.objects, "objects");
            ui.checkbox(&mut self.shown.camps, "camps");
            ui.checkbox(&mut self.shown.places, "places");
        });
        ui.checkbox(&mut self.shown.only_world, "only this world's");
        ui.separator();
        let mut pick = None;
        egui::ScrollArea::vertical()
            .id_salt("near rows")
            .max_height(ui.available_height() * 0.7)
            .show(ui, |ui| {
                let near: Vec<(usize, f32)> = self.near.iter().copied().take(400).collect();
                for (i, d) in near {
                    if d > NEAR {
                        break;
                    }
                    let Some(e) = self.session.entries().get(i).cloned() else {
                        continue;
                    };
                    if !self.shows(&e) {
                        continue;
                    }
                    let (letter, words) = badge(e.prov());
                    let name = match &e.row.row {
                        Row::Object(o) => self.label(o.template),
                        Row::Camp(c) => c
                            .spawns
                            .first()
                            .map(|(t, _)| self.label(*t))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    let mark = match e.layer {
                        Layer::Base => "",
                        Layer::Target if e.was.is_some() => " (changed)",
                        _ => " (new)",
                    };
                    let selected = self.selected.as_deref() == Some(e.item.as_str());
                    let text = egui::RichText::new(format!(
                        "[{letter}] {} {}{mark}  {name}  {d:.0} m",
                        kind_word(&e.row.row),
                        e.item.split_whitespace().nth(1).unwrap_or(""),
                    ))
                    .color(color_of(&e.row.row));
                    if ui
                        .selectable_label(selected, text)
                        .on_hover_text(words)
                        .clicked()
                    {
                        pick = Some(e.item.clone());
                    }
                }
            });
        if let Some(item) = pick {
            self.select(Some(item.clone()));
            self.fly_to(&item);
        }
        let removed = self.session.removed().to_vec();
        if !removed.is_empty() {
            ui.separator();
            ui.strong(format!("Taken away: {}", removed.len()));
            let mut back = None;
            egui::ScrollArea::vertical()
                .id_salt("removed")
                .show(ui, |ui| {
                    for (item, _) in &removed {
                        ui.horizontal(|ui| {
                            ui.label(item);
                            if ui.small_button("Put back").clicked() {
                                back = Some(item.clone());
                            }
                        });
                    }
                });
            if let Some(item) = back {
                if let Err(e) = self.session.put_back(&item) {
                    self.status = e;
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)] // one row's fields
    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(item) = self.selected.clone() else {
            ui.heading("No row selected");
            ui.label("Click a marker in the world, or a row on the left.");
            return;
        };
        let Some(e) = self.session.entry(&item).cloned() else {
            ui.heading(&item);
            ui.label("Not in the world any more.");
            return;
        };
        ui.heading(&item);
        let kind = kind_word(&e.row.row);
        match &e.row.row {
            Row::Object(o) => {
                let l = self.label(o.template);
                ui.label(format!("{kind}: {l}"));
            }
            Row::Camp(c) => {
                let names: Vec<String> = c
                    .spawns
                    .iter()
                    .map(|(t, w)| format!("{} x{w}", self.label(*t)))
                    .collect();
                ui.label(format!("camp of {}", names.join(", ")));
            }
            _ => {
                ui.label(kind);
            }
        }
        ui.separator();
        let (letter, words) = badge(e.prov());
        ui.label(format!("[{letter}] {words}"));
        match e.layer {
            Layer::Base => {
                ui.label(
                    egui::RichText::new(format!(
                        "The database's row (pack {}). A change writes this world's own row of its key.",
                        e.row.pack
                    ))
                    .weak(),
                );
            }
            Layer::Other => {
                ui.label(
                    egui::RichText::new(format!(
                        "Pack {}'s row: this session edits {}.",
                        e.row.pack,
                        self.session.target()
                    ))
                    .weak(),
                );
            }
            Layer::Target => {
                ui.label(egui::RichText::new(format!("{}:{}", e.row.file, e.row.line)).weak());
            }
        }
        if let Some(was) = &e.was {
            let (wl, ww) = badge(was.row.prov());
            ui.label(format!("was: [{wl}] {ww}"));
            if ui
                .button("Put back")
                .on_hover_text("as the database has it")
                .clicked()
            {
                match self.session.put_back(&item) {
                    Ok(()) => self.status = format!("put {item} back"),
                    Err(err) => self.status = err,
                }
            }
        }
        ui.separator();
        // Where it stands, and which way it faces: while a value is dragged it is held here, and
        // the row changes once, when the drag ends.
        let at = e.at();
        let held = self
            .held
            .as_ref()
            .filter(|(i, _)| *i == item)
            .map(|(_, v)| *v);
        let [mut x, mut y, mut z, mut h] =
            held.unwrap_or([at.x, at.y, at.z.unwrap_or(0.0), heading_of(&e.row.row)]);
        let (mut moved, mut dragging) = (false, false);
        ui.horizontal(|ui| {
            ui.label("at");
            for v in [&mut x, &mut y, &mut z] {
                let r = ui.add(egui::DragValue::new(v).speed(0.1).max_decimals(3));
                moved |= r.drag_stopped() || (r.changed() && !r.dragged());
                dragging |= r.dragged();
            }
        });
        if moved {
            self.held = None;
            let g = view::global(e.landblock(), [x, y, z]);
            self.move_row(&item, g);
        } else if dragging {
            self.held = Some((item.clone(), [x, y, z, h]));
        }
        if matches!(e.row.row, Row::Object(_) | Row::Place(_)) {
            let mut turned = false;
            ui.horizontal(|ui| {
                ui.label("heading");
                let r = ui.add(
                    egui::DragValue::new(&mut h)
                        .speed(1.0)
                        .range(0.0..=360.0)
                        .max_decimals(1),
                );
                turned |= r.drag_stopped() || (r.changed() && !r.dragged());
                dragging |= r.dragged();
                if ui.small_button("left").clicked() {
                    h -= TURN_STEP;
                    turned = true;
                }
                if ui.small_button("right").clicked() {
                    h += TURN_STEP;
                    turned = true;
                }
            });
            if turned {
                self.held = None;
                let r = self.session.turn(&item, h);
                self.report(r);
            } else if dragging {
                self.held = Some((item.clone(), [x, y, z, h]));
            }
        }
        ui.horizontal(|ui| {
            if ui
                .button("Ground")
                .on_hover_text("stand it on the ground under it")
                .clicked()
            {
                let g = view::global(e.landblock(), [x, y, 0.0]);
                if let Some(gz) = self.scene.ground(g[0], g[1]) {
                    self.move_row(&item, [g[0], g[1], gz]);
                }
            }
            if ui
                .button("Here")
                .on_hover_text("move it under the camera")
                .clicked()
            {
                let p = self.camera.position;
                if let Some(gz) = self.scene.ground(p.x, p.y) {
                    self.move_row(&item, [p.x, p.y, gz]);
                }
            }
            if ui.button("Fly to").clicked() {
                self.fly_to(&item);
            }
        });
        ui.separator();
        // Its other fields, as TOML.
        let fresh = self.fields_of.as_ref() != Some(&(item.clone(), self.session.revision()));
        if fresh {
            self.fields = editable(&e.row.row)
                .iter()
                .map(|f| ((*f).to_string(), field_text(&e.row.row, f)))
                .collect();
            self.fields_of = Some((item.clone(), self.session.revision()));
        }
        let mut set = None;
        egui::Grid::new("fields")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, text) in &mut self.fields {
                    ui.label(name.as_str());
                    let r = ui.add(
                        egui::TextEdit::singleline(text)
                            .desired_width(220.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        set = Some((name.clone(), text.clone()));
                    }
                    ui.end_row();
                }
            });
        if let Some((field, value)) = set {
            match self.session.set(&item, &field, &field_toml(&field, &value)) {
                Ok(_) => self.status = format!("{item}: {field} set"),
                Err(err) => {
                    self.status = err;
                    self.fields_of = None;
                }
            }
        }
        if let Row::Object(o) = &e.row.row {
            if let Some(to) = &o.to {
                if ui.button(format!("Go to {to}")).clicked() {
                    if let Some(t) = self.place_of(to, &e.row.pack).map(|p| p.item) {
                        self.select(Some(t.clone()));
                        self.fly_to(&t);
                    }
                }
            }
            if !o.landings.is_empty() {
                ui.label(format!("{} recall landings", o.landings.len()));
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("Duplicate")
                .on_hover_text("a copy beside it, selected (Ctrl+D)")
                .clicked()
            {
                self.duplicate_selected();
            }
            if ui
                .add_enabled(e.layer != Layer::Other, egui::Button::new("Take away"))
                .on_hover_text("Delete")
                .clicked()
            {
                self.delete_selected();
            }
        });
    }
}

/// The file the camera's place is kept in between sessions, in the world's build folder.
fn camera_file(world: &std::path::Path) -> PathBuf {
    world.join("build").join("editor-camera.txt")
}

/// The camera where it was when the world was last closed.
#[must_use]
pub fn kept_camera(world: &std::path::Path) -> Option<FlyCamera> {
    let text = std::fs::read_to_string(camera_file(world)).ok()?;
    let v: Vec<f32> = text
        .split_whitespace()
        .filter_map(|w| w.parse().ok())
        .collect();
    let [x, y, z, yaw, pitch, speed] = v[..] else {
        return None;
    };
    let mut c = FlyCamera::new(Vec3::new(x, y, z), yaw, pitch);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        c.speed = speed as usize;
    }
    Some(c)
}

/// A name as a person reads it: the game keeps grammar marks in it, as `Frank the Trader[mn]`.
#[must_use]
pub fn plain_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut skipping = false;
    for c in name.chars() {
        match c {
            '[' => skipping = true,
            ']' => skipping = false,
            c if !skipping => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Where a text of the Find window says to go: a landblock in hexadecimal, and a point in it
/// (`5766`, `5766:123,45`).
#[must_use]
pub fn go_to_of(text: &str) -> Option<(u16, [f32; 2])> {
    let t = text.trim();
    let (block, at) = t.split_once(':').map_or((t, None), |(b, a)| (b, Some(a)));
    if block.len() != 4 {
        return None;
    }
    let lb = u16::from_str_radix(block, 16).ok()?;
    let at = match at {
        None => [80.0, 80.0],
        Some(a) => {
            let (x, y) = a.split_once(',')?;
            [x.trim().parse().ok()?, y.trim().parse().ok()?]
        }
    };
    Some((lb, at))
}

/// A data id written in hexadecimal (`0x47002224`, or without `0x`).
#[must_use]
pub fn parse_id(s: &str) -> Option<u32> {
    let t = s.trim();
    let h = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u32::from_str_radix(h, 16).ok().filter(|v| *v != 0)
}
