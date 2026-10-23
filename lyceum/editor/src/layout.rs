//! The dungeon's grid, its own tab in dungeon mode: the dungeon's rooms from above, one floor at a
//! time, on the grid the world's dungeons are built on ([`lyceum_foundry::grid`]), with the floor
//! selector; and in a tab of its own a side view of the floors. The floor shown is the one the
//! camera is on, or one chosen until the camera goes to another floor.
//!
//! A click on a free doorway (a dot), or on an empty square beside the rooms, chooses where to
//! build: the palette then lists what fits there, best first, and the pointer over one shows its
//! ghost here and in the world view. A click on a room selects it (Delete takes it away). With a
//! piece chosen from the palette, the squares where it fits are marked, it follows the pointer as
//! a ghost, green where it fits, R turns it, and a click sets it there. A room of the pack's own
//! can be lifted and set down again elsewhere, what stands in it carried along.
//!
//! **Floors.** Floors are counted from the one players arrive on, the dungeon's drop point: 0,
//! then +1 above and −1 below, [`grid::RISE`] meters apart. Nothing bounds how high or deep a
//! dungeon's rooms may go, so the floor selector goes one floor past the highest and the lowest
//! built, to start another either way.
//!
//! **Openings up and down** are doorways like those on the sides. An open top a new room has is
//! closed by a lid, a ceiling piece of the look, as the world closes every top nothing stands on;
//! but a lid never stands in the way: the opening under it is offered on the floor above as a
//! free one, and a piece set over it takes the lid's place. An opening in a floor is left free,
//! to build under from the floor below. Each is marked on its own floor and, as a dot to build
//! on, on the floor it opens onto.

use std::collections::{BTreeMap, BTreeSet};

use dereth_primitives::frame::localtoglobal;
use dereth_primitives::{DataId, Frame, Vec3};
use lyceum_foundry::edit::Op;
use lyceum_foundry::grid::{self, PiecePlacement, PITCH};
use lyceum_foundry::kit::{Kind, Placed};
use lyceum_foundry::room::{sea_floor, Shape as Space, Solid};
use lyceum_foundry::source::{Link, Rotation};

use super::dungeon::{Free, Target};
use super::{EditorApp, UiAction};

/// How far apart, up or down, two rooms' floors stand and still make one floor, in meters.
pub const FLOOR_BAND: f32 = 2.0;

/// How near a doorway's sill stands to a floor to open on it, in meters.
pub const SILL: f32 = 1.0;

/// The height of a new dungeon's first floor, in meters: the sea floor's, where most of the world's
/// own dungeon rooms stand.
pub const FIRST_FLOOR: f32 = 0.0;

/// The lowest a floor may be, in meters: above where rooms taken away are kept.
pub const DEEPEST: f32 = lyceum_foundry::cells::CLOSED_DEPTH + 100.0;

/// A dungeon's lids: the ceiling pieces over its rooms' open tops, and the openings they close,
/// each with the lid (its place among them) that closes it.
#[derive(Clone, Debug, Default)]
pub struct Lids {
    pub pieces: Vec<Placed>,
    pub ways: Vec<(Free, usize)>,
}

impl Lids {
    /// The lids' cells.
    #[must_use]
    pub fn cells(&self) -> BTreeSet<u16> {
        self.pieces
            .iter()
            .flat_map(|p| p.cells.iter().copied())
            .collect()
    }
}

/// How floor `z` is called, counted from the arrival's at `arrival`: `0`, `+1`, `−1`.
#[must_use]
pub fn floor_number(z: f32, arrival: f32) -> i32 {
    dereth_primitives::num::floor_to_i32((z - arrival) / grid::RISE + 0.5)
}

/// Floor number `n` as the selector shows it, signed but for the arrival's.
#[must_use]
pub fn floor_label(n: i32) -> String {
    match n {
        0 => "0".to_owned(),
        n if n > 0 => format!("+{n}"),
        n => format!("\u{2212}{}", n.unsigned_abs()),
    }
}

/// A part from above, in its own frame: its floors and its outline as polygons, its doorways as
/// the two ends of each, its height from its lowest point to its highest.
#[derive(Clone, Debug, Default)]
pub struct PiecePlan {
    pub floors: Vec<Vec<Vec3>>,
    pub doorways: Vec<(Vec3, Vec3)>,
    pub low: f32,
    pub high: f32,
}

impl PiecePlan {
    /// Part `piece` of `env`, from above.
    #[must_use]
    pub fn of(env: &dereth_assets::Environment, piece: u16) -> Option<Self> {
        let cs = env.cells.get(usize::from(piece))?;
        let at = |i: &u16| {
            cs.vertex_array
                .vertices
                .get(usize::from(*i))
                .map(|v| v.position)
        };
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for v in &cs.vertex_array.vertices {
            low = low.min(v.position.z);
            high = high.max(v.position.z);
        }
        let portals: std::collections::BTreeSet<u16> = cs.portals.iter().copied().collect();
        let mut floors = Vec::new();
        for (i, poly) in cs.polygons.iter().enumerate() {
            if u16::try_from(i).is_ok_and(|i| portals.contains(&i)) {
                continue;
            }
            let pts: Vec<Vec3> = poly.vertex_ids.iter().filter_map(at).collect();
            if pts.len() < 3 {
                continue;
            }
            // Newell's normal: a floor faces up, near the room's bottom.
            let mut n = Vec3::ZERO;
            for k in 0..pts.len() {
                let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                n.x += (a.y - b.y) * (a.z + b.z);
                n.y += (a.z - b.z) * (a.x + b.x);
                n.z += (a.x - b.x) * (a.y + b.y);
            }
            let len = n.dot(n).sqrt();
            #[allow(clippy::cast_precision_loss)]
            let mid_z = pts.iter().map(|p| p.z).sum::<f32>() / pts.len() as f32;
            if len > 1e-6 && (n.z / len).abs() > 0.6 && mid_z < (low + high) / 2.0 {
                floors.push(pts);
            }
        }
        let doorways = cs
            .portals
            .iter()
            .filter_map(|p| cs.polygons.get(usize::from(*p)))
            .map(|poly| {
                let pts: Vec<Vec3> = poly.vertex_ids.iter().filter_map(at).collect();
                // The two corners farthest apart across: the doorway's sill, seen from above.
                let mut ends = (Vec3::ZERO, Vec3::ZERO);
                let mut far = -1.0f32;
                for a in &pts {
                    for b in &pts {
                        let d = (a.x - b.x).powi(2) + (a.y - b.y).powi(2);
                        if d > far {
                            far = d;
                            ends = (*a, *b);
                        }
                    }
                }
                ends
            })
            .collect();
        Some(Self {
            floors,
            doorways,
            low,
            high,
        })
    }
}

/// A kit piece where it would go: the piece, its frame and its parts' links, and why it does not
/// fit there, if it does not.
#[derive(Clone, Debug)]
pub struct Ghost {
    pub piece: usize,
    pub frame: Frame,
    pub links: Vec<Vec<Link>>,
    pub misfit: Option<String>,
}

impl Ghost {
    /// Whether it fits where it is.
    #[must_use]
    pub fn fits(&self) -> bool {
        self.misfit.is_none()
    }

    /// Where a placement puts kit piece `piece`.
    #[must_use]
    pub fn of(piece: usize, p: &PiecePlacement) -> Self {
        Self {
            piece,
            frame: p.frame,
            links: p.links.clone(),
            misfit: None,
        }
    }
}

/// What the marks were worked out for: the piece, the floor (in centimeters), the edits made,
/// and the room lifted.
type MarksKey = (usize, i32, u64, Option<u16>);

/// A room of the pack's own lifted to be set down elsewhere: its piece's cells and their symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lifted {
    pub cells: Vec<u16>,
    pub names: Vec<String>,
}

/// How near the grid starts, in points to the meter: a room or two round the camera.
pub const GRID_ZOOM: f32 = 3.5;

/// How near the cross-section may be zoomed, times its fit.
const SIDE_ZOOM_MOST: f32 = 40.0;

/// The grid's state.
#[derive(Default)]
pub struct Layout {
    /// The landblock meters at the middle of the grid, and pixels to the meter; worked out to
    /// fit the dungeon the first time.
    pub center: Option<(f32, f32)>,
    pub zoom: f32,
    /// The floor chosen to be shown, by its height (landblock meters), with the floor the camera
    /// was on as it was chosen: shown while the camera stays on that floor. The floors, as last
    /// worked out.
    pub floor: Option<(f32, f32)>,
    pub floors: Vec<f32>,
    /// Quarter turns of the first room, and which placement at a square R has chosen.
    pub turn: u8,
    /// Where to build, chosen on the grid: the palette then offers what fits there.
    pub target: Option<Target>,
    /// The chosen piece's placements on the floor shown, as last worked out.
    pub marks: Option<(MarksKey, std::rc::Rc<Vec<PiecePlacement>>)>,
    /// The ghost under the pointer, or of the fit the pointer is over in the palette.
    pub ghost: Option<Ghost>,
    /// A room of the pack's lifted to be set down elsewhere.
    pub moving: Option<Lifted>,
    /// Edits made since the dungeon was gone into: what the marks were worked out for.
    revision: u64,
    /// The cross-section's zoom, times its fit (1 or more), and how far it is panned from the
    /// dungeon's middle, east and up, in meters.
    pub side_zoom: f32,
    pub side_pan: (f32, f32),
    /// Parts from above, by environment and part; their spaces; the landblock's rooms.
    plans: BTreeMap<(u32, u16), Option<PiecePlan>>,
    spaces: BTreeMap<(u32, u16), Option<Space>>,
    rooms: Option<(u16, std::rc::Rc<BTreeMap<u16, Solid>>)>,
    /// The floor players arrive on, and the lids, as last worked out.
    arrival: Option<(u16, f32)>,
    lids: Option<(u16, std::rc::Rc<Lids>)>,
    /// The grid's rectangle, and where the pointer was over it, in landblock meters.
    pub rect: Option<egui::Rect>,
    pub pointer: Option<(f32, f32)>,
    /// The floor buttons, as drawn: down and up.
    pub floor_buttons: Option<(egui::Rect, egui::Rect)>,
    /// Each free doorway's dot on the floor shown, as drawn: its link and where.
    pub doorway_dots: Vec<(Link, egui::Pos2)>,
    /// Each opening up or down marked on its own floor, the floor shown, as drawn.
    pub marked_ways: Vec<(Link, egui::Pos2)>,
    /// The room outlines the floors seen from the south drew last.
    pub side_outlines: usize,
}

impl std::fmt::Debug for Layout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layout")
            .field("floor", &self.floor)
            .field("turn", &self.turn)
            .field("target", &self.target)
            .field("moving", &self.moving)
            .finish_non_exhaustive()
    }
}

impl Layout {
    /// Forget what was worked out of the world: after an edit.
    pub fn invalidate(&mut self) {
        self.rooms = None;
        self.arrival = None;
        self.lids = None;
        self.ghost = None;
        self.marks = None;
        self.revision += 1;
    }

    /// As a dungeon is gone into: the view fitted to it again, the floor players arrive on shown.
    pub fn reset(&mut self) {
        self.invalidate();
        self.center = None;
        self.zoom = 0.0;
        self.side_zoom = 1.0;
        self.side_pan = (0.0, 0.0);
        self.floor = None;
        self.turn = 0;
        self.target = None;
        self.moving = None;
    }

    /// The edits made since the dungeon was gone into, as the marks and fits count them.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Where landblock meters `p` are on the grid drawn last.
    #[must_use]
    pub fn to_screen(&self, p: (f32, f32)) -> Option<egui::Pos2> {
        let rect = self.rect?;
        let (cx, cy) = self.center?;
        Some(egui::pos2(
            rect.center().x + (p.0 - cx) * self.zoom,
            rect.center().y - (p.1 - cy) * self.zoom,
        ))
    }

    /// The landblock meters at point `p` of the grid drawn last.
    #[must_use]
    pub fn to_world(&self, p: egui::Pos2) -> Option<(f32, f32)> {
        let rect = self.rect?;
        let (cx, cy) = self.center?;
        Some((
            cx + (p.x - rect.center().x) / self.zoom,
            cy - (p.y - rect.center().y) / self.zoom,
        ))
    }
}

/// The floors of rooms whose lowest points are `lows`: each a band [`FLOOR_BAND`] deep, by the
/// lowest point in it, lowest first.
#[must_use]
pub fn floors_of(mut lows: Vec<f32>) -> Vec<f32> {
    lows.sort_by(f32::total_cmp);
    let mut out: Vec<f32> = Vec::new();
    for z in lows {
        if out.last().is_none_or(|f| z - f > FLOOR_BAND) {
            out.push(z);
        }
    }
    out
}

/// Which floor of `floors` a room whose lowest point is `low` stands on.
#[must_use]
pub fn floor_of(floors: &[f32], low: f32) -> Option<usize> {
    floors
        .iter()
        .rposition(|f| low >= *f - 0.01)
        .or((!floors.is_empty()).then_some(0))
}

/// The floor of `floors` the camera at height `camera` (landblock meters) is on.
#[must_use]
pub fn camera_floor(floors: &[f32], camera: f32) -> Option<f32> {
    floor_of(floors, camera - 1.0).and_then(|i| floors.get(i).copied())
}

/// The floor of `floors` the grid shows with the camera at height `camera`: the one chosen
/// (`chosen`, with the floor the camera was on as it was chosen) while the camera stays on that
/// floor, else the camera's own; `arrival` when there are none.
#[must_use]
pub fn shown_floor(floors: &[f32], chosen: Option<(f32, f32)>, camera: f32, arrival: f32) -> f32 {
    let near = |z: f32| {
        floors
            .iter()
            .copied()
            .min_by(|a, b| (a - z).abs().total_cmp(&(b - z).abs()))
    };
    let own = camera_floor(floors, camera);
    chosen
        .filter(|(_, from)| own.is_none_or(|o| (o - from).abs() < 0.01))
        .and_then(|(f, _)| near(f))
        .or(own)
        .unwrap_or(arrival)
}

/// The lowest point of an opening: its sill.
#[must_use]
pub fn sill(f: &Free) -> f32 {
    f.opening
        .corners
        .iter()
        .map(|c| c.z)
        .fold(f32::MAX, f32::min)
}

/// Whether opening `f`, up or down, belongs to a room on floor `shown` of `floors`: an opening
/// down stands at its room's floor, and one up over it, under the next floor or higher.
#[must_use]
pub fn on_own_floor(f: &Free, floors: &[f32], shown: f32) -> bool {
    let s = sill(f);
    if f.opening.out.z < -0.5 {
        (s - shown).abs() <= FLOOR_BAND
    } else {
        floors
            .iter()
            .copied()
            .filter(|z| *z < s - SILL)
            .fold(None, |a: Option<f32>, z| Some(a.map_or(z, |a| a.max(z))))
            .is_some_and(|own| (own - shown).abs() < 0.01)
    }
}

/// Whether a piece standing on floor `floor` may join free doorway `f`: one on that floor, the
/// open top of a room under it, or the open floor of a room over it.
#[must_use]
pub fn opens_on(f: &Free, floor: f32) -> bool {
    let s = sill(f);
    if f.opening.out.z < -0.5 {
        s > floor + SILL && s <= floor + grid::RISE + SILL
    } else {
        (s - floor).abs() <= SILL
    }
}

impl EditorApp {
    /// Whether the grid is shown: in dungeon mode.
    #[must_use]
    pub fn grid_shown(&self) -> bool {
        self.dungeon.is_some() && self.playing.is_none()
    }

    /// Part `(env, piece)` from above, read once.
    fn piece_plan(&mut self, env: DataId, piece: u16) -> Option<PiecePlan> {
        if let Some(p) = self.layout.plans.get(&(env.raw(), piece)) {
            return p.clone();
        }
        let plan = self
            .session
            .environment(env)
            .and_then(|e| PiecePlan::of(&e, piece));
        self.layout.plans.insert((env.raw(), piece), plan.clone());
        plan
    }

    /// Part `(env, piece)`'s space, read once.
    pub(super) fn layout_space(&mut self, env: DataId, piece: u16) -> Option<Space> {
        if let Some(s) = self.layout.spaces.get(&(env.raw(), piece)) {
            return s.clone();
        }
        let space = self
            .session
            .environment(env)
            .and_then(|e| Space::of(&e, piece));
        self.layout.spaces.insert((env.raw(), piece), space.clone());
        space
    }

    /// The rooms of landblock `lb` as spaces, worked out once till the next edit; a room taken
    /// away (closed) in none.
    fn room_spaces(&mut self, lb: u16) -> std::rc::Rc<BTreeMap<u16, Solid>> {
        if let Some((at, rooms)) = &self.layout.rooms {
            if *at == lb {
                return std::rc::Rc::clone(rooms);
            }
        }
        let cells = self.cells_of(lb);
        let mut rooms = BTreeMap::new();
        for (n, c) in cells.iter() {
            if c.frame.origin.z <= lyceum_foundry::cells::CLOSED_DEPTH + 1.0 {
                continue;
            }
            if let Some(space) = self.layout_space(c.environment, c.cell_struct) {
                rooms.insert(*n, space.placed(&c.frame));
            }
        }
        let rooms = std::rc::Rc::new(rooms);
        self.layout.rooms = Some((lb, std::rc::Rc::clone(&rooms)));
        rooms
    }

    /// The lids of dungeon `lb`: the ceiling pieces over its rooms' open tops, and the openings
    /// they close, as a free doorway there would be given; worked out once till the next edit.
    /// None before the kit is gathered.
    pub fn lids(&mut self, lb: u16) -> std::rc::Rc<Lids> {
        if let Some((at, lids)) = &self.layout.lids {
            if *at == lb {
                return std::rc::Rc::clone(lids);
            }
        }
        let Some(pieces) = self.kit.pieces.clone() else {
            return std::rc::Rc::new(Lids::default());
        };
        let ceilings: BTreeSet<u32> = pieces
            .iter()
            .filter(|p| p.form.kind == Kind::Ceiling)
            .map(|p| p.environment.raw())
            .collect();
        let cells = self.cells_of(lb);
        let found: Vec<Placed> = lyceum_foundry::kit::placed_pieces(&cells)
            .into_iter()
            .filter(|p| ceilings.contains(&p.environment.raw()))
            .collect();
        let lid_of: BTreeMap<u16, usize> = found
            .iter()
            .enumerate()
            .flat_map(|(k, p)| p.cells.iter().map(move |c| (*c, k)))
            .collect();
        let into: BTreeSet<u16> = lid_of.keys().copied().collect();
        let ways = self
            .ways_into(lb, &into)
            .into_iter()
            .filter_map(|(f, to)| Some((f, *lid_of.get(&to)?)))
            .collect();
        let lids = std::rc::Rc::new(Lids {
            pieces: found,
            ways,
        });
        self.layout.lids = Some((lb, std::rc::Rc::clone(&lids)));
        lids
    }

    /// The doorways of dungeon `lb`'s rooms, but those of `into`, that lead into a room of
    /// `into`, each as a free doorway there would be given, with the room it leads into.
    pub(super) fn ways_into(&mut self, lb: u16, into: &BTreeSet<u16>) -> Vec<(Free, u16)> {
        let cells = self.cells_of(lb);
        let names = self.session.cell_names(lb);
        let mut ways = Vec::new();
        for (n, c) in cells.iter() {
            if into.contains(n) || c.frame.origin.z <= lyceum_foundry::cells::CLOSED_DEPTH + 1.0 {
                continue;
            }
            for q in c.portals.iter().filter(|q| q.flags & 4 == 0) {
                let Some(to) = u16::try_from(q.other_cell_id)
                    .ok()
                    .filter(|o| into.contains(o))
                else {
                    continue;
                };
                let Some(env) = self.environment(c.environment) else {
                    continue;
                };
                let Some(slot) = env
                    .cells
                    .get(usize::from(c.cell_struct))
                    .and_then(|cs| cs.portals.iter().position(|p| *p == q.polygon_id))
                else {
                    continue;
                };
                let Some(opening) =
                    lyceum_foundry::pieces::opening(env, c.cell_struct, slot, &c.frame)
                else {
                    continue;
                };
                let link = match names.get(n) {
                    Some(name) => Link::Cell {
                        cell: name.clone(),
                        portal: slot,
                    },
                    None => Link::Base {
                        cell: *n,
                        portal: slot,
                    },
                };
                ways.push((Free { link, opening }, to));
            }
        }
        ways
    }

    /// The open tops of dungeon `lb`'s rooms that rooms of `gone` stand on: what is left open
    /// once those are taken away or moved.
    pub(super) fn tops_under(&mut self, lb: u16, gone: &BTreeSet<u16>) -> Vec<Free> {
        self.ways_into(lb, gone)
            .into_iter()
            .map(|(f, _)| f)
            .filter(|f| f.opening.out.z > 0.5)
            .collect()
    }

    /// The doorways of dungeon `lb` a piece may join: its free ones (but those of the pack's
    /// cells `lifting`, and with those they joined), and the openings its lids close (but those of
    /// the cells lifted).
    pub fn open_ways(&mut self, lb: u16, lifting: &[String]) -> Vec<Free> {
        let mut out = self.free_portals(lb, lifting);
        let lids = self.lids(lb);
        out.extend(
            lids.ways
                .iter()
                .filter(
                    |(f, _)| !matches!(&f.link, Link::Cell { cell, .. } if lifting.contains(cell)),
                )
                .map(|(f, _)| f.clone()),
        );
        out
    }

    /// The height of the floor players arrive on in dungeon `lb`, landblock meters: its drop
    /// point's, else the first floor of a new dungeon.
    pub fn arrival_floor(&mut self, lb: u16) -> f32 {
        if let Some((at, z)) = self.layout.arrival {
            if at == lb {
                return z;
            }
        }
        let z = if self.cells_of(lb).is_empty() {
            FIRST_FLOOR
        } else {
            self.drop_point(lb)
                .ok()
                .filter(|(cell, _)| *cell >> 16 == u32::from(lb))
                .map_or(FIRST_FLOOR, |(_, at)| at.z)
        };
        self.layout.arrival = Some((lb, z));
        z
    }

    /// The floors of dungeon `lb`, lowest first: those its rooms stand on and its open doorways
    /// open onto, and one more past the lowest and the highest room; and the floor shown: the one
    /// chosen, else the camera's (going in, the one players arrive on).
    pub fn plan_floors(&mut self, lb: u16) -> (Vec<f32>, f32) {
        let cells = self.cells_of(lb);
        let lids = self.lids(lb).cells();
        let rooms: Vec<f32> = cells
            .iter()
            .filter(|(n, c)| {
                !lids.contains(n) && c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0
            })
            .filter_map(|(_, c)| {
                self.piece_plan(c.environment, c.cell_struct)
                    .map(|p| c.frame.origin.z + p.low)
            })
            .collect();
        let arrival = self.arrival_floor(lb);
        let mut lows = rooms.clone();
        lows.extend(self.open_ways(lb, &[]).iter().map(sill));
        let built = floors_of(rooms);
        if let (Some(lo), Some(hi)) = (built.first(), built.last()) {
            lows.push(hi + grid::RISE);
            if lo - grid::RISE >= DEEPEST {
                lows.push(lo - grid::RISE);
            }
        }
        if lows.is_empty() {
            lows.push(arrival);
        }
        let floors = floors_of(lows);
        let camera = self.camera_height(lb);
        let shown = shown_floor(&floors, self.layout.floor, camera, arrival);
        self.layout.floors.clone_from(&floors);
        (floors, shown)
    }

    /// The camera's eye's height in dungeon `lb`, landblock meters: the flying camera's, or the
    /// walking body's.
    fn camera_height(&self, lb: u16) -> f32 {
        self.eye().z - lyceum_foundry::pick::block_origin(lb).z
    }

    /// Show floor `f` of the dungeon being edited, until the camera goes to another floor.
    pub fn choose_floor(&mut self, f: f32) {
        let Some(lb) = self.dungeon_mode() else {
            return;
        };
        let (floors, _) = self.plan_floors(lb);
        let own = camera_floor(&floors, self.camera_height(lb)).unwrap_or(f);
        self.layout.floor = Some((f, own));
    }

    /// The selector's name of floor `z` of dungeon `lb`: its number from the floor players arrive
    /// on, and its height.
    pub fn floor_name(&mut self, lb: u16, z: f32) -> String {
        let n = floor_number(z, self.arrival_floor(lb));
        self.say_with(
            "layout.floor_n",
            &[("n", &floor_label(n)), ("z", &format!("{z:.1}"))],
        )
    }

    /// A step up (`1`) or down (`-1`) a floor of the dungeon being edited.
    pub fn step_floor(&mut self, by: i32) {
        let Some(lb) = self.dungeon_mode() else {
            return;
        };
        let (floors, shown) = self.plan_floors(lb);
        let at = floors
            .iter()
            .position(|f| (f - shown).abs() < 0.01)
            .map_or(0, |i| i64::try_from(i).unwrap_or(0));
        let to = (at + i64::from(by)).clamp(0, i64::try_from(floors.len()).unwrap_or(1) - 1);
        if let Some(f) = usize::try_from(to)
            .ok()
            .and_then(|i| floors.get(i).copied())
        {
            self.choose_floor(f);
            self.layout.ghost = None;
            self.layout.marks = None;
            self.layout.target = None;
            self.panels.message = self.floor_name(lb, f);
        }
    }

    /// The dungeon `lb` as a piece set in it sees it: its rooms (but those lifted, and its lids),
    /// the doorways it may join (with those the lifted room joined, and those the lids close),
    /// and whether its land has ground.
    pub(super) fn grid_world(&mut self, lb: u16) -> (BTreeMap<u16, Solid>, Vec<Free>, bool) {
        let lifted = self.layout.moving.clone();
        let mut rooms = (*self.room_spaces(lb)).clone();
        if let Some(l) = &lifted {
            for c in &l.cells {
                rooms.remove(c);
            }
        }
        for c in self.lids(lb).cells() {
            rooms.remove(&c);
        }
        let names = lifted.map(|l| l.names).unwrap_or_default();
        let doorways = self.open_ways(lb, &names);
        let land = self.session.terrain(lb).is_some_and(|t| !sea_floor(&t));
        (rooms, doorways, land)
    }

    /// Every placement of kit piece `i` joined to an open doorway on the floor shown.
    pub fn piece_marks(&mut self, lb: u16, i: usize) -> std::rc::Rc<Vec<PiecePlacement>> {
        let (_, floor) = self.plan_floors(lb);
        #[allow(clippy::cast_possible_truncation)]
        let key: MarksKey = (
            i,
            (floor * 100.0).round() as i32,
            self.layout.revision,
            self.layout
                .moving
                .as_ref()
                .and_then(|m| m.cells.first().copied()),
        );
        if let Some((k, m)) = &self.layout.marks {
            if *k == key {
                return std::rc::Rc::clone(m);
            }
        }
        let marks = std::rc::Rc::new(self.placements_of(lb, i, floor));
        self.layout.marks = Some((key, std::rc::Rc::clone(&marks)));
        marks
    }

    /// The placements of kit piece `i` joined to an open doorway whose sill stands near floor
    /// `floor`.
    fn placements_of(&mut self, lb: u16, i: usize, floor: f32) -> Vec<PiecePlacement> {
        let Some(piece) = self.kit.pieces.as_ref().and_then(|p| p.get(i)).cloned() else {
            return Vec::new();
        };
        let Some(spaces) = self.piece_spaces(i) else {
            return Vec::new();
        };
        let (rooms, doorways, land) = self.grid_world(lb);
        let among: Vec<usize> = doorways
            .iter()
            .enumerate()
            .filter(|(_, d)| opens_on(d, floor))
            .map(|(k, _)| k)
            .collect();
        let session = &self.session;
        let ground = |x: f32, y: f32| session.ground(lb, x, y);
        let dungeon = grid::Dungeon {
            rooms: &rooms,
            doorways: &doorways,
            ground: land.then_some(&ground as &dyn Fn(f32, f32) -> Option<f32>),
        };
        grid::piece_placements(&piece.form, &spaces, &dungeon, &among)
    }

    /// Where the chosen piece would go with the pointer over square `s` on the floor shown: the
    /// placement there R has chosen, or for a dungeon's first room the piece standing on it.
    pub fn ghost_at(&mut self, lb: u16, s: (i32, i32)) -> Option<Ghost> {
        let i = self.kit.chosen?;
        let piece = self.kit.pieces.as_ref()?.get(i)?.clone();
        let (_, floor) = self.plan_floors(lb);
        let (rooms, _, _) = self.grid_world(lb);
        if rooms.is_empty() {
            // The first room: on the square, its floor on the floor shown.
            let frame = grid::square_frame(s, floor - piece.form.lo.z, self.layout.turn);
            let misfit = if self.layout.moving.is_some() {
                None
            } else {
                self.room_for_dungeon(lb).err()
            };
            return Some(Ghost {
                piece: i,
                frame,
                links: piece
                    .form
                    .slots
                    .iter()
                    .map(|n| vec![Link::Shut; *n])
                    .collect(),
                misfit,
            });
        }
        let marks = self.piece_marks(lb, i);
        let here: Vec<&PiecePlacement> = marks
            .iter()
            .filter(|p| {
                p.square().map(|(q, _)| q) == Some(s)
                    || (p.square().is_none()
                        && grid::square_of(p.frame.origin.x, p.frame.origin.y) == s)
            })
            .collect();
        if here.is_empty() {
            let frame = grid::square_frame(s, floor - piece.form.lo.z, self.layout.turn);
            return Some(Ghost {
                piece: i,
                frame,
                links: Vec::new(),
                misfit: Some(self.say("layout.no_fit")),
            });
        }
        let p = here[usize::from(self.layout.turn) % here.len()];
        Some(Ghost::of(i, p))
    }

    /// Set the chosen piece down on square `s`: placed (or, a room lifted, moved there) where it
    /// fits.
    ///
    /// # Errors
    /// It does not fit there, said plainly.
    pub fn place_on_square(&mut self, s: (i32, i32)) -> Result<(), String> {
        let lb = self
            .dungeon_mode()
            .ok_or_else(|| self.say("messages.off_world"))?;
        let ghost = self
            .ghost_at(lb, s)
            .ok_or_else(|| self.say("messages.choose_piece"))?;
        if let Some(why) = ghost.misfit {
            return Err(why);
        }
        let placement = PiecePlacement {
            frame: ghost.frame,
            links: ghost.links.clone(),
            joined: Vec::new(),
        };
        if let Some(lifted) = self.layout.moving.take() {
            return self.set_lifted(lb, &lifted, ghost.piece, &placement);
        }
        self.set_piece(lb, ghost.piece, &placement)
    }

    /// The room `lifted` set down at `placement` (it is kit piece `i`): each part moved, its
    /// statics and spawns with it, joined anew, its lid made again over it there, and an open top
    /// it stood on lidded. One step.
    fn set_lifted(
        &mut self,
        lb: u16,
        lifted: &Lifted,
        i: usize,
        placement: &PiecePlacement,
    ) -> Result<(), String> {
        let pieces = self
            .kit
            .pieces
            .clone()
            .ok_or_else(|| self.say("messages.pieces_gathering"))?;
        let form = &pieces[i].form;
        let cells = self.cells_of(lb);
        // Each part's name, by its place in the environment.
        let mut name_of: BTreeMap<u16, String> = BTreeMap::new();
        for (c, n) in lifted.cells.iter().zip(&lifted.names) {
            if let Some(cell) = cells.get(c) {
                name_of.insert(cell.cell_struct, n.clone());
            }
        }
        let mut links = placement.links.clone();
        for ((pa, sa), (pb, sb)) in &form.inner {
            let (Some(na), Some(nb)) = (name_of.get(pa), name_of.get(pb)) else {
                continue;
            };
            if let Some(l) = links.get_mut(usize::from(*pa)).and_then(|v| v.get_mut(*sa)) {
                *l = Link::Cell {
                    cell: nb.clone(),
                    portal: *sb,
                };
            }
            if let Some(l) = links.get_mut(usize::from(*pb)).and_then(|v| v.get_mut(*sb)) {
                *l = Link::Cell {
                    cell: na.clone(),
                    portal: *sa,
                };
            }
        }
        let q = placement.frame.rotation;
        let o = placement.frame.origin;
        // Its lid goes, and so do those over the openings it joins where it is set.
        let joined = super::dungeon::joined_links(placement.links.iter());
        let names = lifted.names.clone();
        let mut ops = self.lids_away(lb, &|f| {
            joined.contains(&f.link)
                || matches!(&f.link, Link::Cell { cell, .. } if names.contains(cell))
        });
        ops.extend(name_of.iter().map(|(part, name)| Op::MoveCell {
            landblock: lb,
            name: name.clone(),
            at: [o.x, o.y, o.z],
            rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
            links: links.get(usize::from(*part)).cloned().unwrap_or_default(),
        }));
        // Lids over its open tops where it is set, and over those it stood on where it was.
        let mut tops: Vec<Free> = form
            .doors
            .iter()
            .filter(|d| {
                d.side.is_none()
                    && d.up
                    && links
                        .get(usize::from(d.part))
                        .and_then(|p| p.get(d.slot))
                        .is_some_and(|l| *l == Link::Shut)
            })
            .filter_map(|d| {
                Some(Free {
                    link: Link::Cell {
                        cell: name_of.get(&d.part)?.clone(),
                        portal: d.slot,
                    },
                    opening: grid::placed(&d.opening, &placement.frame),
                })
            })
            .collect();
        let from: BTreeSet<u16> = lifted.cells.iter().copied().collect();
        tops.extend(
            self.tops_under(lb, &from)
                .into_iter()
                .filter(|f| !joined.contains(&f.link)),
        );
        if !tops.is_empty() {
            let (mut rooms, _, _) = self.grid_world(lb);
            for c in &lifted.cells {
                rooms.remove(c);
            }
            for (k, s) in self.piece_spaces(i).unwrap_or_default().iter().enumerate() {
                rooms.insert(
                    0xF000 + u16::try_from(k).unwrap_or(0),
                    s.placed(&placement.frame),
                );
            }
            let lids = self.lids_over(lb, &rooms, tops);
            if !lids.is_empty() {
                ops.push(Op::PlacePieces {
                    landblock: lb,
                    pieces: lids,
                    joins: Vec::new(),
                });
            }
        }
        self.apply(&Op::Batch(ops), false)?;
        self.kit.chosen = None;
        self.panels.message = self.say("layout.moved");
        Ok(())
    }

    /// Lift the pack's room `n` of dungeon `lb`, its whole piece: its piece chosen, the squares
    /// it can be set down on marked.
    ///
    /// # Errors
    /// A room that is not the pack's own.
    pub fn lift_room(&mut self, lb: u16, n: u16) -> Result<(), String> {
        let names = self.session.cell_names(lb);
        if !names.contains_key(&n) {
            return Err(self.say("messages.room_not_packs"));
        }
        self.wait_for_pieces();
        let piece = self
            .room_piece(lb, n)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        let i = self
            .piece_of(piece.environment)
            .ok_or_else(|| self.say("messages.no_such_piece"))?;
        let lifted = Lifted {
            names: piece
                .cells
                .iter()
                .filter_map(|c| names.get(c).cloned())
                .collect(),
            cells: piece.cells,
        };
        self.kit.chosen = Some(i);
        self.layout.moving = Some(lifted);
        self.layout.marks = None;
        self.layout.target = None;
        // Set down on the grid: it is brought forward.
        self.show_tab(super::Tab::Grid);
        self.panels.message = self.say("layout.lifted");
        Ok(())
    }

    /// The Grid tab: the floor selector, the rooms of the floor shown, its open doorways, the
    /// marks where the chosen piece fits and its ghost; answering the pointer.
    #[allow(clippy::too_many_lines)]
    pub(super) fn grid_ui(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some(lb) = self.dungeon_mode() else {
            return;
        };
        let t = self.text.table();
        let (floors, shown) = self.plan_floors(lb);
        let at = floors
            .iter()
            .position(|f| (f - shown).abs() < 0.01)
            .map_or(1, |i| i + 1);
        let name = self.floor_name(lb, shown);
        // The floor selector.
        ui.horizontal(|ui| {
            let down = ui
                .add_enabled(at > 1, egui::Button::new("▼"))
                .on_hover_text(t.get("layout.floor_down"));
            if down.clicked() {
                out.push(UiAction::FloorStep(-1));
            }
            ui.strong(name);
            let up = ui
                .add_enabled(at < floors.len(), egui::Button::new("▲"))
                .on_hover_text(t.get("layout.floor_up"));
            if up.clicked() {
                out.push(UiAction::FloorStep(1));
            }
            self.layout.floor_buttons = Some((down.rect, up.rect));
        });
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(18, 20, 24));
        self.layout.rect = Some(rect);
        let cells = self.cells_of(lb);
        // The first time: about the camera, near enough to see the rooms round it.
        if self.layout.center.is_none() || self.layout.zoom <= 0.0 {
            let o = lyceum_foundry::pick::block_origin(lb);
            let cam = self.camera.position;
            self.layout.center = Some((cam.x - o.x, cam.y - o.y));
            self.layout.zoom = GRID_ZOOM;
        }
        let (cx, cy) = self.layout.center.unwrap_or((96.0, 96.0));
        let zoom = self.layout.zoom;
        let to_screen = move |p: Vec3| {
            egui::pos2(
                rect.center().x + (p.x - cx) * zoom,
                rect.center().y - (p.y - cy) * zoom,
            )
        };
        let to_world = move |p: egui::Pos2| {
            (
                cx + (p.x - rect.center().x) / zoom,
                cy - (p.y - rect.center().y) / zoom,
            )
        };
        // The grid's squares: a line at each square's edge.
        let grid_line = egui::Stroke::new(1.0, egui::Color32::from_gray(38));
        let (wx0, wy1) = to_world(rect.min);
        let (wx1, wy0) = to_world(rect.max);
        if self.layout.zoom * PITCH >= 4.0 {
            #[allow(clippy::cast_possible_truncation)]
            for gx in ((wx0 / PITCH).floor() as i32)..=((wx1 / PITCH).ceil() as i32) {
                #[allow(clippy::cast_precision_loss)]
                let x = to_screen(Vec3::new((gx as f32 + 0.5) * PITCH, 0.0, 0.0)).x;
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                    grid_line,
                );
            }
            #[allow(clippy::cast_possible_truncation)]
            for gy in ((wy0 / PITCH).floor() as i32)..=((wy1 / PITCH).ceil() as i32) {
                #[allow(clippy::cast_precision_loss)]
                let y = to_screen(Vec3::new(0.0, (gy as f32 + 0.5) * PITCH, 0.0)).y;
                painter.line_segment(
                    [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                    grid_line,
                );
            }
        }
        // The rooms: the floor shown solid, those under it dim, those over it faded out; the
        // selected room's whole piece lit.
        let draw_plan = |painter: &egui::Painter,
                         plan: &PiecePlan,
                         f: &Frame,
                         fill: egui::Color32,
                         line: egui::Color32| {
            for poly in &plan.floors {
                let pts: Vec<egui::Pos2> = poly
                    .iter()
                    .map(|p| to_screen(localtoglobal(f, *p)))
                    .collect();
                painter.add(egui::Shape::convex_polygon(
                    pts.clone(),
                    fill,
                    egui::Stroke::NONE,
                ));
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.0, line)));
            }
        };
        let pointer = response.hover_pos();
        let mut under: Option<u16> = None;
        let mut joined: Vec<(Vec3, Vec3)> = Vec::new();
        let lifted: Vec<u16> = self
            .layout
            .moving
            .as_ref()
            .map(|m| m.cells.clone())
            .unwrap_or_default();
        let selected: Vec<u16> = match &self.selection {
            Some(super::Selection::Cell { landblock, cell }) if *landblock == lb => self
                .room_piece(lb, *cell)
                .map(|p| p.cells)
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        // The lids are not drawn: the openings they close are, as free ones.
        let lids = self.lids(lb).cells();
        for (n, c) in cells.iter() {
            if c.frame.origin.z <= lyceum_foundry::cells::CLOSED_DEPTH + 1.0 || lids.contains(n) {
                continue;
            }
            let Some(plan) = self.piece_plan(c.environment, c.cell_struct) else {
                continue;
            };
            let low = c.frame.origin.z + plan.low;
            let on_floor = (low - shown).abs() <= FLOOR_BAND;
            let (fill, line) = if lifted.contains(n) {
                (
                    egui::Color32::from_rgba_unmultiplied(120, 120, 140, 30),
                    egui::Color32::from_rgb(240, 200, 90),
                )
            } else if on_floor {
                (
                    if selected.contains(n) {
                        egui::Color32::from_rgb(70, 90, 120)
                    } else {
                        egui::Color32::from_gray(78)
                    },
                    egui::Color32::from_gray(170),
                )
            } else if low < shown {
                (egui::Color32::from_gray(40), egui::Color32::from_gray(70))
            } else {
                (
                    egui::Color32::from_rgba_unmultiplied(120, 120, 140, 18),
                    egui::Color32::from_rgba_unmultiplied(150, 150, 170, 40),
                )
            };
            draw_plan(&painter, &plan, &c.frame, fill, line);
            if on_floor {
                for (a, b) in &plan.doorways {
                    joined.push((localtoglobal(&c.frame, *a), localtoglobal(&c.frame, *b)));
                }
                // The room under the pointer: a point on the edge between two of its floor's
                // polygons is in it too.
                if let Some(p) = pointer {
                    let w = to_world(p);
                    let hit = plan.floors.iter().any(|poly| {
                        let pts: Vec<(f32, f32)> = poly
                            .iter()
                            .map(|v| {
                                let g = localtoglobal(&c.frame, *v);
                                (g.x, g.y)
                            })
                            .collect();
                        [
                            (0.0, 0.0),
                            (0.05, 0.05),
                            (-0.05, 0.05),
                            (0.05, -0.05),
                            (-0.05, -0.05),
                        ]
                        .iter()
                        .any(|(dx, dy)| inside(&pts, (w.0 + dx, w.1 + dy)))
                    });
                    if hit {
                        under = Some(*n);
                    }
                }
            }
        }
        for (a, b) in joined {
            painter.line_segment(
                [to_screen(a), to_screen(b)],
                egui::Stroke::new(2.0, egui::Color32::from_gray(120)),
            );
        }
        // The open doorways of the floor shown: dots, the one chosen ringed.
        let lifting: Vec<String> = self
            .layout
            .moving
            .as_ref()
            .map(|m| m.names.clone())
            .unwrap_or_default();
        self.layout.doorway_dots.clear();
        self.layout.marked_ways.clear();
        let mut over_doorway: Option<Link> = None;
        for f in self.open_ways(lb, &lifting) {
            let p = to_screen(f.opening.middle);
            let vertical = f.opening.out.z.abs() > 0.5;
            if vertical && on_own_floor(&f, &floors, shown) {
                // On its own floor, an opening up or down is marked by the way it goes.
                let up = f.opening.out.z > 0.0;
                let (color, tip) = if up {
                    (egui::Color32::from_rgb(90, 150, 255), -6.0)
                } else {
                    (egui::Color32::from_rgb(240, 90, 90), 6.0)
                };
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(p.x, p.y + tip),
                        egui::pos2(p.x - 5.0, p.y - tip * 0.6),
                        egui::pos2(p.x + 5.0, p.y - tip * 0.6),
                    ],
                    color,
                    egui::Stroke::new(1.0, egui::Color32::BLACK),
                ));
                self.layout.marked_ways.push((f.link.clone(), p));
            }
            if !opens_on(&f, shown) {
                continue;
            }
            let color = if vertical {
                if f.opening.out.z > 0.0 {
                    egui::Color32::from_rgb(90, 150, 255)
                } else {
                    egui::Color32::from_rgb(240, 90, 90)
                }
            } else {
                super::panels::mark::TURN
            };
            let chosen = self.layout.target == Some(Target::Doorway(f.link.clone()));
            let hot = pointer.is_some_and(|q| q.distance(p) <= 8.0);
            painter.circle_filled(p, if hot || chosen { 7.0 } else { 5.0 }, color);
            painter.circle_stroke(
                p,
                if chosen { 9.0 } else { 6.0 },
                egui::Stroke::new(
                    if chosen { 2.0 } else { 1.0 },
                    if chosen {
                        egui::Color32::from_rgb(90, 160, 240)
                    } else {
                        egui::Color32::BLACK
                    },
                ),
            );
            if hot && over_doorway.is_none() {
                over_doorway = Some(f.link.clone());
            }
            self.layout.doorway_dots.push((f.link.clone(), p));
        }
        // The square under the pointer, and the square chosen.
        let square = pointer.map(|p| {
            let (x, y) = to_world(p);
            grid::square_of(x, y)
        });
        let square_rect = |s: (i32, i32)| {
            let (x, y) = grid::square_middle(s);
            egui::Rect::from_two_pos(
                to_screen(Vec3::new(x - PITCH / 2.0, y - PITCH / 2.0, 0.0)),
                to_screen(Vec3::new(x + PITCH / 2.0, y + PITCH / 2.0, 0.0)),
            )
        };
        if let Some(Target::Square(s)) = self.layout.target {
            painter.rect_stroke(
                square_rect(s).shrink(1.0),
                2.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(90, 160, 240)),
                egui::StrokeKind::Inside,
            );
        }
        // Where the chosen piece fits: each square marked; its ghost under the pointer. Else the
        // ghost of the fit the pointer is over in the palette.
        self.layout.ghost = None;
        if let Some(i) = self.kit.chosen {
            let marks = self.piece_marks(lb, i);
            for p in marks.iter() {
                let s = p.square().map_or_else(
                    || grid::square_of(p.frame.origin.x, p.frame.origin.y),
                    |(s, _)| s,
                );
                painter.rect_filled(
                    square_rect(s).shrink(2.0),
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(70, 200, 90, 60),
                );
            }
            if let Some(s) = square {
                self.layout.ghost = self.ghost_at(lb, s);
            }
        } else if let Some(k) = self.kit.hover_fit {
            let fits = self.fits(lb);
            self.layout.ghost = fits.get(k).map(|f| Ghost::of(f.piece, &f.placement));
        }
        if let Some(g) = self.layout.ghost.clone() {
            let env = self
                .kit
                .pieces
                .as_ref()
                .and_then(|p| p.get(g.piece))
                .map(|p| p.environment);
            let parts = self
                .kit
                .pieces
                .as_ref()
                .and_then(|p| p.get(g.piece))
                .map_or(0, |p| p.form.parts());
            let (fill, line) = if g.fits() {
                (
                    egui::Color32::from_rgba_unmultiplied(70, 200, 90, 90),
                    egui::Color32::from_rgb(90, 230, 110),
                )
            } else {
                (
                    egui::Color32::from_rgba_unmultiplied(220, 60, 50, 90),
                    egui::Color32::from_rgb(255, 90, 80),
                )
            };
            if let Some(env) = env {
                for part in 0..parts {
                    if let Some(plan) = self.piece_plan(env, part) {
                        draw_plan(&painter, &plan, &g.frame, fill, line);
                        for (a, b) in &plan.doorways {
                            painter.line_segment(
                                [
                                    to_screen(localtoglobal(&g.frame, *a)),
                                    to_screen(localtoglobal(&g.frame, *b)),
                                ],
                                egui::Stroke::new(3.0, line),
                            );
                        }
                    }
                }
            }
            if let Some(why) = g.misfit.filter(|_| pointer.is_some()) {
                painter.text(
                    rect.left_top() + egui::vec2(8.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    why,
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_rgb(255, 120, 110),
                );
            }
        }
        // The camera: a spot, and a line the way it looks.
        let o = lyceum_foundry::pick::block_origin(lb);
        let cam = self.camera.position;
        let c = to_screen(Vec3::new(cam.x - o.x, cam.y - o.y, 0.0));
        if rect.contains(c) {
            let f = self.camera.forward();
            let len = (f.x * f.x + f.y * f.y).sqrt().max(1e-3);
            let tip = egui::pos2(c.x + f.x / len * 14.0, c.y - f.y / len * 14.0);
            painter.line_segment(
                [c, tip],
                egui::Stroke::new(2.0, super::panels::mark::CAMERA),
            );
            painter.circle_filled(c, 4.0, super::panels::mark::CAMERA);
        }
        // The pointer: a click sets the chosen piece down; else chooses a doorway, selects a
        // room, or chooses an empty square.
        if response.clicked() {
            if let Some(s) = square {
                if self.kit.chosen.is_some() {
                    out.push(UiAction::PlaceOnSquare(s));
                } else if let Some(l) = over_doorway {
                    out.push(UiAction::Target(Some(Target::Doorway(l))));
                } else if let Some(n) = under {
                    out.push(UiAction::SelectHere(super::Selection::Cell {
                        landblock: lb,
                        cell: n,
                    }));
                } else {
                    out.push(UiAction::Target(Some(Target::Square(s))));
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let d = response.drag_delta();
            if let Some((cx, cy)) = self.layout.center {
                let zoom = self.layout.zoom;
                self.layout.center = Some((cx - d.x / zoom, cy + d.y / zoom));
            }
        }
        if response.hovered() {
            let scroll = wheel(ui);
            if scroll != 0.0 {
                self.layout.zoom =
                    (self.layout.zoom * if scroll > 0.0 { 1.25 } else { 0.8 }).clamp(0.5, 60.0);
            }
        }
        self.layout.pointer = pointer.map(to_world);
    }

    /// The Side tab: every room of the dungeon across it from the south, its floors stacked; a
    /// click on a floor shows it.
    pub(super) fn side_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some(lb) = self.dungeon_mode() else {
            return;
        };
        let (floors, shown) = self.plan_floors(lb);
        let side = ui.available_rect_before_wrap();
        ui.allocate_rect(side, egui::Sense::hover());
        self.side_view(ui, side, lb, &floors, shown, out);
    }

    /// The dungeon's rooms from the south: each by its east-west reach and its height, the floor
    /// shown marked; a click on a floor shows it.
    fn side_view(
        &mut self,
        ui: &mut egui::Ui,
        side: egui::Rect,
        lb: u16,
        floors: &[f32],
        shown: f32,
        out: &mut Vec<UiAction>,
    ) {
        let painter = ui.painter_at(side);
        painter.rect_filled(side, 0.0, egui::Color32::from_rgb(26, 28, 34));
        painter.line_segment(
            [side.left_top(), side.left_bottom()],
            egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
        );
        let cells = self.cells_of(lb);
        let mut boxes: Vec<(f32, f32, f32, f32)> = Vec::new();
        for c in cells.values() {
            if c.frame.origin.z <= lyceum_foundry::cells::CLOSED_DEPTH + 1.0 {
                continue;
            }
            let Some(plan) = self.piece_plan(c.environment, c.cell_struct) else {
                continue;
            };
            let (mut x0, mut x1) = (f32::MAX, f32::MIN);
            for poly in &plan.floors {
                for p in poly {
                    let g = localtoglobal(&c.frame, *p);
                    x0 = x0.min(g.x);
                    x1 = x1.max(g.x);
                }
            }
            if x0 > x1 {
                x0 = c.frame.origin.x - 2.0;
                x1 = c.frame.origin.x + 2.0;
            }
            boxes.push((
                x0,
                x1,
                c.frame.origin.z + plan.low,
                c.frame.origin.z + plan.high,
            ));
        }
        let lo_z = floors.iter().copied().fold(shown, f32::min);
        let hi_z = floors.iter().copied().fold(shown, f32::max) + 6.0;
        let (x0, x1, z0, z1) = boxes.iter().fold((f32::MAX, f32::MIN, lo_z, hi_z), |a, b| {
            (a.0.min(b.0), a.1.max(b.1), a.2.min(b.2), a.3.max(b.3))
        });
        let (x0, x1) = if x0 > x1 { (0.0, 10.0) } else { (x0, x1) };
        let inner = side.shrink(10.0);
        let sx = inner.width() / (x1 - x0).max(1.0);
        let sz = inner.height() / (z1 - z0).max(1.0);
        // Fitted, then zoomed with the wheel and panned with a right or middle drag, as the grid.
        let zoom = self.layout.side_zoom.max(1.0);
        let s = sx.min(sz) * zoom;
        let (px, pz) = self.layout.side_pan;
        let (mx, mz) = ((x0 + x1) / 2.0 + px, (z0 + z1) / 2.0 + pz);
        let at = |x: f32, z: f32| {
            egui::pos2(
                inner.center().x + (x - mx) * s,
                inner.center().y - (z - mz) * s,
            )
        };
        let response = ui.interact(
            side,
            ui.id().with("floors side"),
            egui::Sense::click_and_drag(),
        );
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let d = response.drag_delta();
            self.layout.side_pan = (px - d.x / s, pz + d.y / s);
        }
        if response.hovered() {
            let scroll = wheel(ui);
            if scroll != 0.0 {
                // About the point under the pointer: it stays under it.
                let factor = if scroll > 0.0 { 1.25 } else { 0.8 };
                let now = (zoom * factor).clamp(1.0, SIDE_ZOOM_MOST);
                if let Some(p) = response.hover_pos() {
                    let (wx, wz) = (
                        mx + (p.x - inner.center().x) / s,
                        mz - (p.y - inner.center().y) / s,
                    );
                    let s2 = s / zoom * now;
                    let (nx, nz) = (
                        wx - (p.x - inner.center().x) / s2,
                        wz + (p.y - inner.center().y) / s2,
                    );
                    self.layout.side_pan = (nx - (x0 + x1) / 2.0, nz - (z0 + z1) / 2.0);
                }
                if now <= 1.0 {
                    self.layout.side_pan = (0.0, 0.0);
                }
                self.layout.side_zoom = now;
            }
        }
        for f in floors {
            let band = egui::Rect::from_two_pos(at(x0, *f), at(x1, *f + FLOOR_BAND.max(3.0)));
            let lit = (f - shown).abs() < 0.01;
            painter.rect_filled(
                band.expand2(egui::vec2(6.0, 0.0)),
                0.0,
                if lit {
                    egui::Color32::from_rgba_unmultiplied(90, 120, 170, 60)
                } else {
                    egui::Color32::from_rgba_unmultiplied(80, 80, 90, 25)
                },
            );
            if response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| band.expand2(egui::vec2(6.0, 2.0)).contains(p))
            {
                out.push(UiAction::Floor(*f));
            }
        }
        // Rooms in a line north to south stand on one another seen from the south: each outline
        // is drawn once.
        let mut drawn = std::collections::HashSet::new();
        self.layout.side_outlines = 0;
        for (bx0, bx1, bz0, bz1) in boxes {
            let r = egui::Rect::from_two_pos(at(bx0, bz0), at(bx1, bz1));
            #[allow(clippy::cast_possible_truncation)] // a point on screen, to an eighth of one
            let key = [r.min.x, r.min.y, r.max.x, r.max.y].map(|v| (v * 8.0).round() as i32);
            if !drawn.insert(key) {
                continue;
            }
            self.layout.side_outlines += 1;
            painter.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(150)),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// The wheel's turn this frame, up positive.
fn wheel(ui: &egui::Ui) -> f32 {
    ui.input(|i| {
        i.raw
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                _ => None,
            })
            .sum::<f32>()
    })
}

/// Whether point `p` is inside polygon `poly` (both from above).
fn inside(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut odd = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            odd = !odd;
        }
    }
    odd
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a dungeon laid out on its grid)
    use super::*;

    #[test]
    fn rooms_whose_floors_stand_close_make_one_floor_and_a_room_belongs_to_the_one_under_it() {
        let floors = floors_of(vec![0.0, 0.5, -0.4, 6.0, 6.2, 12.0, -12.0]);
        assert_eq!(floors, vec![-12.0, -0.4, 6.0, 12.0]);
        assert_eq!(floor_of(&floors, 6.1), Some(2));
        assert_eq!(floor_of(&floors, -0.4), Some(1));
        assert_eq!(floor_of(&floors, -20.0), Some(0));
        assert_eq!(floor_of(&[], 3.0), None);
    }

    #[test]
    fn the_grid_shows_the_floor_the_camera_is_on_and_one_chosen_holds_till_the_camera_changes_floor(
    ) {
        let floors = [-6.0, 0.0, 6.0];
        // The eye a body's height over each floor.
        assert_eq!(shown_floor(&floors, None, 1.6, 0.0), 0.0);
        assert_eq!(shown_floor(&floors, None, 7.6, 0.0), 6.0);
        assert_eq!(shown_floor(&floors, None, -4.4, 0.0), -6.0);
        // The floor above chosen from floor 0: shown while the camera stays there.
        let chosen = Some((6.0, 0.0));
        assert_eq!(shown_floor(&floors, chosen, 1.6, 0.0), 6.0);
        assert_eq!(shown_floor(&floors, chosen, 3.0, 0.0), 6.0);
        // The camera flies down a floor: the grid follows it.
        assert_eq!(shown_floor(&floors, chosen, -4.4, 0.0), -6.0);
        assert_eq!(shown_floor(&[], chosen, 1.6, 12.0), 12.0);
    }

    #[test]
    fn floors_are_counted_in_six_meter_steps_from_the_arrival_signed_but_for_its_own() {
        assert_eq!(floor_number(0.0, 0.005), 0);
        assert_eq!(floor_number(6.0, 0.0), 1);
        assert_eq!(floor_number(-6.1, 0.0), -1);
        assert_eq!(floor_number(-24.0, -12.0), -2);
        assert_eq!(floor_number(2.9, 0.0), 0);
        assert_eq!(floor_label(0), "0");
        assert_eq!(floor_label(2), "+2");
        assert_eq!(floor_label(-3), "\u{2212}3");
    }

    /// An opening ten meters square at height `z`, leading up or down.
    fn opening(z: f32, up: bool) -> Free {
        let corners = vec![
            Vec3::new(-5.0, -5.0, z),
            Vec3::new(5.0, -5.0, z),
            Vec3::new(5.0, 5.0, z),
            Vec3::new(-5.0, 5.0, z),
        ];
        Free {
            link: Link::Shut,
            opening: lyceum_foundry::pieces::Opening {
                corners,
                middle: Vec3::new(0.0, 0.0, z),
                out: Vec3::new(0.0, 0.0, if up { 1.0 } else { -1.0 }),
            },
        }
    }

    #[test]
    fn an_opening_up_or_down_is_marked_on_its_rooms_floor_and_built_on_from_the_floor_it_opens_onto(
    ) {
        let floors = [-6.0, 0.0, 6.0];
        // A room's open top six meters over floor 0: its own floor 0, built on from floor +1.
        let top = opening(6.0, true);
        assert!(on_own_floor(&top, &floors, 0.0));
        assert!(!on_own_floor(&top, &floors, 6.0));
        assert!(opens_on(&top, 6.0) && !opens_on(&top, 0.0));
        // A room's open floor on floor 0: its own floor 0, built on from floor -1.
        let bottom = opening(0.0, false);
        assert!(on_own_floor(&bottom, &floors, 0.0));
        assert!(!on_own_floor(&bottom, &floors, -6.0));
        assert!(opens_on(&bottom, -6.0) && !opens_on(&bottom, 0.0));
    }

    #[test]
    fn a_point_is_inside_a_floor_plan_when_it_crosses_its_edges_an_odd_number_of_times() {
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(inside(&square, (5.0, 5.0)));
        assert!(!inside(&square, (15.0, 5.0)));
        assert!(!inside(&square, (5.0, -1.0)));
    }
}
