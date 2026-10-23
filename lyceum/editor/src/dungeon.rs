//! The dungeon kit, from the editor's side.
//!
//! **The pieces** are the kit's ([`lyceum_foundry::kit`]): each an environment placed whole, as
//! the world's own dungeons are built. The palette shows the pieces of the dungeon's look, each as
//! its floor plan from above (its outline, its doorways, its stairs, its openings up and down),
//! grouped by what it is on the grid: dead ends, corridors, corners, T junctions, crossings, rooms
//! by their footprint, stairs and ramps, shafts and ceilings.
//!
//! **What fits here.** With a free doorway or an empty square beside the rooms chosen on the grid,
//! the palette lists the pieces of the look that fit there, each way one can stand there, given
//! every neighbor's doorways, best first: those that join the most doorways, then those the world
//! joins to those neighbors most, then the commonest. A click sets one in, with a lid (a ceiling
//! of the look) over its open top; a lid over an opening it joins gives way to it.
//!
//! **Taking a room away** takes its piece: the pack's own rows, or the world's own cells closed,
//! a ceiling over it that closed it alone with it; the doorways that led into it are free to
//! build on again, and an open top it stood on is lidded again, as the world leaves no top open.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::sync::Arc;

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::{DataId, Frame, Vec3};
use lyceum_foundry::edit::{NewPiece, Op, PartPortal};
use lyceum_foundry::grid::{self, Doorway, PiecePlacement};
use lyceum_foundry::kit::{Dress, Kind, KitPiece, Side};
use lyceum_foundry::pieces::{Catalog, Family};
use lyceum_foundry::source::{Link, Rotation};

use crate::view::View;

use super::{EditorApp, UiAction};

/// A free doorway of the dungeon.
pub type Free = Doorway;

/// Where the grid's choice of where to build is: an empty square of the floor shown, or a free
/// doorway, by the link a piece joining it is given.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Square((i32, i32)),
    Doorway(Link),
}

/// A piece that fits where the grid's choice is: the piece (its place in the kit), where it would
/// go, and how well: how many doorways it joins, how often the world joins it to the pieces round
/// it, and how often the world places it.
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    pub piece: usize,
    pub placement: PiecePlacement,
    pub score: (usize, usize, usize),
}

/// Two parts' doorways joined, of the pieces set in at once.
type Join = (PartPortal, PartPortal);

/// What a link to a part of a piece not set in yet starts with, before the part's number.
const PART: char = '\u{1}';

/// Each doorway the parts' links `links` join, one by one.
pub(super) fn joined_links<'a>(links: impl Iterator<Item = &'a Vec<Link>>) -> Vec<Link> {
    links
        .flatten()
        .flat_map(|l| match l {
            Link::Many(all) => all.clone(),
            Link::Shut => Vec::new(),
            l => vec![l.clone()],
        })
        .collect()
}

/// A frame's origin, as an edit writes it.
fn at_of(f: &Frame) -> [f32; 3] {
    [f.origin.x, f.origin.y, f.origin.z]
}

/// A frame's turn, as an edit writes it.
fn rotation_of(f: &Frame) -> Rotation {
    let q = f.rotation;
    Rotation::Quat([q.w, q.x, q.y, q.z])
}

/// What the fits were worked out for: the choice, the floor (in centimeters), the edits made, the
/// look, and whether every piece was offered.
type FitsKey = (Target, i32, u64, Option<usize>, bool);

/// The browser's state: the kit once gathered (off the frame loop), its families of looks and
/// what each draws each role with; the environments as read; the look shown, and whether every
/// piece is offered in it; the text filter; the chosen piece; the fits worked out last; and where
/// each thing was drawn.
#[derive(Default)]
pub struct Kit {
    pub pieces: Option<Arc<Vec<KitPiece>>>,
    pub families: Vec<Family>,
    pub dress: Vec<Dress>,
    gathering: Option<mpsc::Receiver<Catalog>>,
    envs: BTreeMap<u32, Option<dereth_assets::Environment>>,
    plans: BTreeMap<u32, Option<Plan>>,
    /// The look the palette shows: the dungeon's, or one being looked at.
    pub look: Option<usize>,
    /// Whether the palette offers every piece of the kit, each dressed in the look by the role of
    /// its surfaces, rather than the look's own.
    pub every: bool,
    pub filter: String,
    pub chosen: Option<usize>,
    fits: Option<(FitsKey, std::rc::Rc<Vec<Fit>>)>,
    /// The dungeons' names, and how many weenies they were read with.
    names: Option<(usize, BTreeMap<u16, String>)>,
    /// The look of the dungeon of a landblock, as last worked out (till the next edit).
    pub look_of: Option<(u16, Option<usize>)>,
    /// A piece the palette scrolls to when next drawn.
    pub reveal: Option<usize>,
    /// The fit the pointer is over in the list, whose ghost is shown.
    pub hover_fit: Option<usize>,
    /// Where each piece's tile, each group's header, each fit's tile and each look's row (while
    /// the list is open) were drawn last.
    pub piece_rects: Vec<(usize, egui::Rect)>,
    pub group_rects: Vec<(Kind, egui::Rect)>,
    pub fit_rects: Vec<(usize, egui::Rect)>,
    /// The part of the fits' list in view, as drawn last.
    pub fits_view: Option<egui::Rect>,
    pub look_rects: Vec<(usize, egui::Rect)>,
}

impl std::fmt::Debug for Kit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit")
            .field("pieces", &self.pieces.as_ref().map(|p| p.len()))
            .field("look", &self.look)
            .field("chosen", &self.chosen)
            .finish_non_exhaustive()
    }
}

/// A kit piece from above, in its own frame: its floors (each polygon with its height), the edges
/// round them where no other floor goes on, its doorways, and its extent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub floors: Vec<(Vec<Vec3>, f32)>,
    pub edges: Vec<(Vec3, Vec3)>,
    pub doors: Vec<PlanDoor>,
    pub lo: Vec3,
    pub hi: Vec3,
}

/// A doorway as a plan draws it: a side doorway by the two ends of its sill, an opening in a
/// floor or a ceiling by its outline; which way it leads, and how high its sill stands.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanDoor {
    pub corners: Vec<Vec3>,
    pub ends: (Vec3, Vec3),
    pub side: Option<Side>,
    pub up: bool,
    pub sill: f32,
}

impl Plan {
    /// Environment `env` as a kit piece, from above.
    #[must_use]
    pub fn of(env: &dereth_assets::Environment) -> Option<Self> {
        let form = lyceum_foundry::kit::form(env)?;
        let mut floors = Vec::new();
        for cs in &env.cells {
            let vs = &cs.vertex_array.vertices;
            let (mut low, mut high) = (f32::MAX, f32::MIN);
            for v in vs {
                low = low.min(v.position.z);
                high = high.max(v.position.z);
            }
            let portals: BTreeSet<u16> = cs.portals.iter().copied().collect();
            for (i, poly) in cs.polygons.iter().enumerate() {
                if u16::try_from(i).is_ok_and(|i| portals.contains(&i)) {
                    continue;
                }
                let pts: Vec<Vec3> = poly
                    .vertex_ids
                    .iter()
                    .filter_map(|k| vs.get(usize::from(*k)).map(|v| v.position))
                    .collect();
                if pts.len() < 3 {
                    continue;
                }
                let mut n = Vec3::ZERO;
                for k in 0..pts.len() {
                    let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                    n.x += (a.y - b.y) * (a.z + b.z);
                    n.y += (a.z - b.z) * (a.x + b.x);
                    n.z += (a.x - b.x) * (a.y + b.y);
                }
                let len = n.dot(n).sqrt();
                #[allow(clippy::cast_precision_loss)]
                let mid = pts.iter().map(|p| p.z).sum::<f32>() / pts.len() as f32;
                if len > 1e-6 && (n.z / len).abs() > 0.6 && mid < (low + high) / 2.0 {
                    floors.push((pts, mid));
                }
            }
        }
        // The edges no other floor shares: the floors' outline.
        let key = |p: Vec3| {
            (
                dereth_primitives::num::floor_to_i32(p.x * 20.0 + 0.5),
                dereth_primitives::num::floor_to_i32(p.y * 20.0 + 0.5),
            )
        };
        // Each edge by its two ends, rounded: how many floors have it, and its ends.
        type Ends = ((i32, i32), (i32, i32));
        let mut seen: BTreeMap<Ends, (usize, (Vec3, Vec3))> = BTreeMap::new();
        for (poly, _) in &floors {
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let (ka, kb) = (key(a), key(b));
                if ka == kb {
                    continue;
                }
                let e = seen
                    .entry(if ka < kb { (ka, kb) } else { (kb, ka) })
                    .or_insert((0, (a, b)));
                e.0 += 1;
            }
        }
        let edges = seen
            .into_values()
            .filter(|(n, _)| *n == 1)
            .map(|(_, e)| e)
            .collect();
        let doors = form
            .doors
            .iter()
            .map(|d| {
                let pts = &d.opening.corners;
                let mut ends = (Vec3::ZERO, Vec3::ZERO);
                let mut far = -1.0f32;
                for a in pts {
                    for b in pts {
                        let dd = (a.x - b.x).powi(2) + (a.y - b.y).powi(2);
                        if dd > far {
                            far = dd;
                            ends = (*a, *b);
                        }
                    }
                }
                PlanDoor {
                    corners: pts.clone(),
                    ends,
                    side: d.side,
                    up: d.up,
                    sill: d.sill,
                }
            })
            .collect();
        Some(Self {
            floors,
            edges,
            doors,
            lo: form.lo,
            hi: form.hi,
        })
    }
}

/// Point `p` of a piece's own frame turned `turns` quarter turns clockwise about its origin.
#[must_use]
pub fn turned(p: Vec3, turns: u8) -> Vec3 {
    match turns % 4 {
        1 => Vec3::new(p.y, -p.x, p.z),
        2 => Vec3::new(-p.x, -p.y, p.z),
        3 => Vec3::new(-p.y, p.x, p.z),
        _ => p,
    }
}

/// What the palette shows of the kit `pieces` in look `look` (every piece, when `every`; the
/// packs' own in any look), those whose label or kind holds `text` (or used in a dungeon of
/// `named`): in groups by what each is on the grid, in the groups' order, the commonest first in
/// each.
#[must_use]
pub fn grouped(
    pieces: &[KitPiece],
    look: Option<usize>,
    every: bool,
    text: &str,
    named: &BTreeSet<u16>,
) -> Vec<(Kind, Vec<usize>)> {
    let f = text.trim().to_ascii_uppercase();
    let mut chosen: Vec<usize> = (0..pieces.len())
        .filter(|i| {
            let p = &pieces[*i];
            every || p.looks.is_empty() || look.is_some_and(|l| p.in_look(l))
        })
        .filter(|i| {
            let p = &pieces[*i];
            f.is_empty()
                || p.label().contains(&f)
                || format!("{:08X}", p.environment.raw()).contains(&f)
                || p.form
                    .kind
                    .key()
                    .to_ascii_uppercase()
                    .replace('_', " ")
                    .contains(&f)
                || p.landblocks.keys().any(|lb| named.contains(lb))
        })
        .collect();
    chosen.sort_by_key(|i| (std::cmp::Reverse(pieces[*i].placed), *i));
    let rooms: BTreeSet<(u8, u8)> = chosen
        .iter()
        .filter_map(|i| match pieces[*i].form.kind {
            Kind::Room { long, short } => Some((long, short)),
            _ => None,
        })
        .collect();
    Kind::groups(&rooms)
        .into_iter()
        .map(|k| {
            (
                k,
                chosen
                    .iter()
                    .copied()
                    .filter(|i| pieces[*i].form.kind == k)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

impl EditorApp {
    /// The kit, gathered off the frame loop the first time it is wanted, with the packs' own
    /// pieces after the world's.
    pub(super) fn kit_pieces(&mut self) -> Option<&Arc<Vec<KitPiece>>> {
        if self.kit.pieces.is_none() && self.kit.gathering.is_none() {
            let (dir, era) = (self.session.base().dir.clone(), self.session.base().era);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let catalog = lyceum_foundry::dats::DatSet::open_era(&dir, era)
                    .map(|set| lyceum_foundry::pieces::catalog(&set))
                    .unwrap_or_default();
                let _ = tx.send(catalog);
            });
            self.kit.gathering = Some(rx);
        }
        if let Some(c) = self
            .kit
            .gathering
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.take_kit(c);
        }
        self.kit.pieces.as_ref()
    }

    /// The gathered catalog taken in: its kit, and the packs' own pieces.
    fn take_kit(&mut self, c: Catalog) {
        let mut pieces = c.kit;
        let mut own: BTreeMap<DataId, Vec<DataId>> = BTreeMap::new();
        for p in self.session.new_pieces() {
            own.entry(p.environment).or_insert(p.surfaces);
        }
        for (env, surfaces) in own {
            if pieces.iter().any(|p| p.environment == env) {
                continue;
            }
            if let Some(p) = self
                .session
                .environment(env)
                .and_then(|e| KitPiece::new(&e, &surfaces))
            {
                pieces.push(p);
            }
        }
        self.kit.pieces = Some(Arc::new(pieces));
        self.kit.families = c.families;
        self.kit.dress = c.dress;
        self.kit.gathering = None;
    }

    /// Wait for the kit, as a script or a test does.
    pub fn wait_for_pieces(&mut self) {
        while self.kit_pieces().is_none() {
            if let Some(c) = self.kit.gathering.take().and_then(|rx| rx.recv().ok()) {
                self.take_kit(c);
            }
        }
    }

    /// Environment `id`, read once.
    pub(super) fn environment(&mut self, id: DataId) -> Option<&dereth_assets::Environment> {
        let session = &self.session;
        self.kit
            .envs
            .entry(id.raw())
            .or_insert_with(|| session.environment(id))
            .as_ref()
    }

    /// Kit piece `i`'s plan from above, read once.
    pub fn piece_plan_of(&mut self, i: usize) -> Option<Plan> {
        let env = self.kit.pieces.as_ref()?.get(i)?.environment;
        if let Some(p) = self.kit.plans.get(&env.raw()) {
            return p.clone();
        }
        let plan = self.environment(env).and_then(Plan::of);
        self.kit.plans.insert(env.raw(), plan.clone());
        plan
    }

    /// The kit piece of environment `env`, by its place in the kit.
    #[must_use]
    pub fn piece_of(&self, env: DataId) -> Option<usize> {
        self.kit
            .pieces
            .as_ref()?
            .iter()
            .position(|p| p.environment == env)
    }

    /// Select cell `n` of landblock `lb`, and fly into it.
    ///
    /// # Errors
    /// No such cell.
    pub fn choose_cell(&mut self, lb: u16, n: u16) -> Result<(), String> {
        self.selection = Some(super::Selection::Cell {
            landblock: lb,
            cell: n,
        });
        self.also.clear();
        self.fly_into_cell(lb, n)
    }

    /// What dungeon `lb` is called, when its portals name it.
    fn place_name(&mut self, lb: u16) -> Option<String> {
        if self
            .kit
            .names
            .as_ref()
            .is_none_or(|(n, _)| *n != self.weenies.len())
        {
            let names: BTreeMap<u16, String> = self
                .dungeons()
                .into_iter()
                .filter_map(|d| d.name.map(|n| (d.landblock, n)))
                .collect();
            self.kit.names = Some((self.weenies.len(), names));
        }
        self.kit
            .names
            .as_ref()
            .and_then(|(_, m)| m.get(&lb).cloned())
    }

    /// The look dungeon `lb` is drawn in: as the pack keeps it, else the look most of its pieces
    /// are drawn in as the world draws them, else, for one of the world's own, the look of its
    /// landblock. `None` before the kit is gathered, or for a dungeon with no room yet.
    pub fn dungeon_look(&mut self, lb: u16) -> Option<usize> {
        if let Some((at, look)) = self.kit.look_of {
            if at == lb {
                return look;
            }
        }
        let pieces = self.kit.pieces.clone()?;
        let kept = self
            .session
            .dungeon_info(lb)
            .look
            .and_then(|l| self.family_of_landblock(l));
        let look = kept.or_else(|| {
            let cells = self.cells_of(lb);
            let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
            for placed in lyceum_foundry::kit::placed_pieces(&cells) {
                let Some(p) = pieces.iter().find(|p| p.environment == placed.environment) else {
                    continue;
                };
                let surfaces = &cells[&placed.cells[0]].surfaces;
                if let Some(l) = p.looks.iter().find(|l| l.surfaces == *surfaces) {
                    *counts.entry(l.family).or_default() += placed.cells.len();
                }
            }
            counts
                .into_iter()
                .max_by_key(|(f, n)| (*n, std::cmp::Reverse(*f)))
                .map(|(f, _)| f)
                .or_else(|| self.family_of_landblock(lb))
        });
        self.kit.look_of = Some((lb, look));
        look
    }

    /// What a family of looks is called: the dungeons drawn in it that their portals name, most
    /// cells first, else its first landblock.
    pub fn family_name(&mut self, f: usize) -> String {
        let Some(fam) = self.kit.families.get(f).cloned() else {
            return String::new();
        };
        let named: Vec<String> = fam
            .landblocks
            .iter()
            .filter_map(|(lb, _)| self.place_name(*lb))
            .fold(Vec::new(), |mut v, n| {
                if !v.contains(&n) {
                    v.push(n);
                }
                v
            })
            .into_iter()
            .take(2)
            .collect();
        let t = self.text.table();
        let first = fam.landblocks.first().map_or(0, |l| l.0);
        match named.as_slice() {
            [] => t.fill("dungeon.look_of", &[("landblock", &format!("{first:04X}"))]),
            [a] => a.clone(),
            [a, b, ..] => t.fill("dungeon.look_two", &[("a", a), ("b", b)]),
        }
    }

    /// The palette's groups for look `look` (every piece, when `every`), by the browser's text:
    /// a piece's number or kind, or a dungeon's name or landblock its pieces are used in.
    pub fn piece_groups(&mut self, look: Option<usize>, every: bool) -> Vec<(Kind, Vec<usize>)> {
        let Some(pieces) = self.kit.pieces.clone() else {
            return Vec::new();
        };
        let text = self.kit.filter.trim().to_lowercase();
        let named: BTreeSet<u16> = if text.is_empty() {
            BTreeSet::new()
        } else {
            let all: BTreeSet<u16> = pieces
                .iter()
                .flat_map(|p| p.landblocks.keys().copied())
                .collect();
            all.into_iter()
                .filter(|lb| {
                    format!("{lb:04x}") == text
                        || self
                            .place_name(*lb)
                            .is_some_and(|n| n.to_lowercase().contains(&text))
                })
                .collect()
        };
        grouped(&pieces, look, every, &self.kit.filter, &named)
    }

    /// The surfaces kit piece `piece` is drawn with in dungeon `lb`: as its look dresses it (the
    /// look the palette shows, else the dungeon's), else its own.
    pub(super) fn surfaces_for(&mut self, lb: u16, piece: &KitPiece) -> Vec<DataId> {
        let look = self.kit.look.or_else(|| self.dungeon_look(lb));
        match look {
            Some(f) => lyceum_foundry::kit::dress(piece, f, self.kit.dress.get(f)).0,
            None => piece.surfaces.clone(),
        }
    }

    /// The free doorways of dungeon `lb`, but those of the pack's cells `lifting`.
    #[must_use]
    pub fn free_portals(&self, lb: u16, lifting: &[String]) -> Vec<Free> {
        self.session.free_doorways(lb, lifting)
    }

    /// The free doorways of dungeon `lb` on the sides of its rooms, not up or down.
    #[must_use]
    pub fn side_doorways(&self, lb: u16) -> Vec<Free> {
        self.free_portals(lb, &[])
            .into_iter()
            .filter(|f| f.opening.out.z.abs() <= 0.5)
            .collect()
    }

    /// Kit piece `i`'s parts' spaces, read once.
    pub(super) fn piece_spaces(&mut self, i: usize) -> Option<Vec<lyceum_foundry::room::Shape>> {
        let env = self.kit.pieces.as_ref()?.get(i)?.environment;
        let n = self.kit.pieces.as_ref()?.get(i)?.form.parts();
        (0..n)
            .map(|p| self.layout_space(env, p))
            .collect::<Option<Vec<_>>>()
    }

    /// The pieces that fit where the grid's choice is, on the floor shown, best first: worked out
    /// again only when the choice, the floor, the dungeon or the look changes.
    pub fn fits(&mut self, lb: u16) -> std::rc::Rc<Vec<Fit>> {
        let Some(target) = self.layout.target.clone() else {
            return std::rc::Rc::new(Vec::new());
        };
        let (_, floor) = self.plan_floors(lb);
        let look = self.kit.look.or_else(|| self.dungeon_look(lb));
        #[allow(clippy::cast_possible_truncation)]
        let key: FitsKey = (
            target.clone(),
            (floor * 100.0).round() as i32,
            self.layout.revision(),
            look,
            self.kit.every,
        );
        if let Some((k, f)) = &self.kit.fits {
            if *k == key {
                return std::rc::Rc::clone(f);
            }
        }
        let fits = std::rc::Rc::new(self.work_out_fits(lb, &target, floor, look));
        self.kit.fits = Some((key, std::rc::Rc::clone(&fits)));
        fits
    }

    /// [`Self::fits`], worked out.
    fn work_out_fits(
        &mut self,
        lb: u16,
        target: &Target,
        floor: f32,
        look: Option<usize>,
    ) -> Vec<Fit> {
        let Some(pieces) = self.kit.pieces.clone() else {
            return Vec::new();
        };
        let (rooms, doorways, land) = self.grid_world(lb);
        let among: Vec<usize> = match target {
            Target::Doorway(link) => doorways
                .iter()
                .position(|d| d.link == *link)
                .into_iter()
                .collect(),
            Target::Square(s) => {
                let (x, y) = grid::square_middle(*s);
                doorways
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| {
                        let m = d.opening.middle;
                        (m.x - x).abs() <= grid::PITCH * 0.75
                            && (m.y - y).abs() <= grid::PITCH * 0.75
                            && super::layout::opens_on(d, floor)
                    })
                    .map(|(k, _)| k)
                    .collect()
            }
        };
        let candidates: Vec<usize> = (0..pieces.len())
            .filter(|i| {
                self.kit.every
                    || pieces[*i].looks.is_empty()
                    || look.is_some_and(|l| pieces[*i].in_look(l))
                    || look.is_none()
            })
            .collect();
        // The pieces round the choice, for how often the world joins each candidate to them.
        let cells = self.cells_of(lb);
        let placed = lyceum_foundry::kit::placed_pieces(&cells);
        let near: Vec<usize> = among
            .iter()
            .filter_map(|k| match &doorways[*k].link {
                Link::Base { cell, .. } => Some(*cell),
                Link::Cell { cell, .. } => self
                    .session
                    .cell_names(lb)
                    .into_iter()
                    .find(|(_, n)| n == cell)
                    .map(|(c, _)| c),
                _ => None,
            })
            .filter_map(|c| placed.iter().find(|p| p.holds(c)))
            .filter_map(|p| pieces.iter().position(|k| k.environment == p.environment))
            .collect();
        let spaces_of: Vec<(usize, Vec<lyceum_foundry::room::Shape>)> = candidates
            .iter()
            .filter_map(|i| self.piece_spaces(*i).map(|s| (*i, s)))
            .collect();
        let session = &self.session;
        let ground = |x: f32, y: f32| session.ground(lb, x, y);
        let ground_fn: Option<&dyn Fn(f32, f32) -> Option<f32>> =
            if land { Some(&ground) } else { None };
        let mut out: Vec<Fit> = Vec::new();
        let first = rooms.is_empty();
        for (i, spaces) in spaces_of {
            let form = &pieces[i].form;
            let dungeon = grid::Dungeon {
                rooms: &rooms,
                doorways: &doorways,
                ground: ground_fn,
            };
            let found: Vec<PiecePlacement> = if first {
                // The first room: on the square, its floor on the floor shown, at each turn.
                let Target::Square(s) = target else { continue };
                (0..4u8)
                    .map(|t| grid::square_frame(*s, floor - form.lo.z, t))
                    .filter_map(|f| grid::fit_piece(form, &spaces, &f, &dungeon).ok())
                    .collect()
            } else {
                let mut v = grid::piece_placements(form, &spaces, &dungeon, &among);
                if let Target::Square(s) = target {
                    v.retain(|p| p.square().is_some_and(|(q, _)| q == *s));
                }
                v
            };
            // One of each way it can stand where it joins something: the doorways it joins, and
            // the sides of its square its other doorways then open on, at their heights. Joining
            // nothing, one of each piece.
            type Way = (Vec<usize>, Vec<(u8, i32)>);
            let mut seen: Vec<Way> = Vec::new();
            for p in found {
                let turn = grid::quarter_turns(&p.frame).unwrap_or(0);
                let mut sides: Vec<(u8, i32)> = form
                    .doors
                    .iter()
                    .filter(|_| !p.joined.is_empty())
                    .filter_map(|d| {
                        let s = d.side?.turned(turn).index();
                        Some((s, dereth_primitives::num::floor_to_i32(d.sill * 10.0 + 0.5)))
                    })
                    .collect();
                sides.sort_unstable();
                let way = (p.joined.clone(), sides);
                if seen.contains(&way) {
                    continue;
                }
                seen.push(way);
                let joined_often: usize = near
                    .iter()
                    .map(|n| {
                        pieces[i]
                            .joins
                            .iter()
                            .find(|(j, _)| j == n)
                            .map_or(0, |(_, c)| *c)
                    })
                    .sum();
                out.push(Fit {
                    piece: i,
                    score: (p.joins(), joined_often, pieces[i].placed),
                    placement: p,
                });
            }
        }
        out.sort_by(|a, b| b.score.cmp(&a.score).then(a.piece.cmp(&b.piece)));
        out
    }

    /// The piece and its parts' links of kit piece `i` set at `placement` in dungeon `lb`: the
    /// new piece, and the joins between its own parts; with a lid, a ceiling of the look, over
    /// its open tops nothing closes, joined to it.
    fn new_pieces(
        &mut self,
        lb: u16,
        i: usize,
        placement: &PiecePlacement,
    ) -> Option<(Vec<NewPiece>, Vec<Join>)> {
        let pieces = self.kit.pieces.clone()?;
        let piece = pieces.get(i)?;
        let surfaces = self.surfaces_for(lb, piece);
        let mut news = vec![NewPiece {
            environment: piece.environment,
            surfaces,
            at: at_of(&placement.frame),
            rotation: rotation_of(&placement.frame),
            parts: placement
                .links
                .iter()
                .enumerate()
                .map(|(k, l)| (u16::try_from(k).unwrap_or(0), l.clone()))
                .collect(),
        }];
        let mut joins: Vec<Join> = piece
            .form
            .inner
            .iter()
            .map(|((pa, sa), (pb, sb))| ((0, usize::from(*pa), *sa), (0, usize::from(*pb), *sb)))
            .collect();
        // Its open tops: a lid over them, each joined to the part it closes.
        let tops: Vec<Doorway> = piece
            .form
            .doors
            .iter()
            .filter(|d| {
                d.side.is_none()
                    && d.up
                    && placement
                        .links
                        .get(usize::from(d.part))
                        .and_then(|p| p.get(d.slot))
                        .is_some_and(|l| *l == Link::Shut)
            })
            .map(|d| Doorway {
                link: Link::Cell {
                    cell: format!("{PART}{}", d.part),
                    portal: d.slot,
                },
                opening: grid::placed(&d.opening, &placement.frame),
            })
            .collect();
        if tops.is_empty() {
            return Some((news, joins));
        }
        let (mut rooms, _, _) = self.grid_world(lb);
        let spaces = self.piece_spaces(i)?;
        for (k, s) in spaces.iter().enumerate() {
            rooms.insert(
                0xF000 + u16::try_from(k).unwrap_or(0),
                s.placed(&placement.frame),
            );
        }
        if let Some((lid, mut more)) = self.lid_piece(lb, &rooms, &tops, (news.len(), 0)) {
            news.push(lid);
            joins.append(&mut more);
        }
        Some((news, joins))
    }

    /// A lid for dungeon `lb` over the open tops `tops` among `rooms`: the ceiling piece of the
    /// look that closes the most of them, then the commonest, as piece `with.0` of those set in
    /// together; each top's link joins it, a top whose link names a part of piece `with.1` set
    /// with it (by [`PART`] and the part's number) by a join of the two.
    pub(super) fn lid_piece(
        &mut self,
        lb: u16,
        rooms: &BTreeMap<u16, lyceum_foundry::room::Solid>,
        tops: &[Doorway],
        with: (usize, usize),
    ) -> Option<(NewPiece, Vec<Join>)> {
        let pieces = self.kit.pieces.clone()?;
        let look = self.kit.look.or_else(|| self.dungeon_look(lb));
        let mut ceilings: Vec<usize> = (0..pieces.len())
            .filter(|c| pieces[*c].form.kind == Kind::Ceiling)
            .collect();
        ceilings.sort_by_key(|c| {
            (
                !look.is_some_and(|l| pieces[*c].in_look(l)),
                std::cmp::Reverse(pieces[*c].placed),
            )
        });
        let among: Vec<usize> = (0..tops.len()).collect();
        for c in ceilings {
            let Some(cs) = self.piece_spaces(c) else {
                continue;
            };
            let dungeon = grid::Dungeon {
                rooms,
                doorways: tops,
                ground: None,
            };
            let Some(best) = grid::piece_placements(&pieces[c].form, &cs, &dungeon, &among)
                .into_iter()
                .max_by_key(PiecePlacement::joins)
            else {
                continue;
            };
            let mut joins = Vec::new();
            let mut parts = Vec::new();
            for (k, links) in best.links.iter().enumerate() {
                let mut own = links.clone();
                for (slot, l) in links.iter().enumerate() {
                    let named: Vec<&Link> = match l {
                        Link::Many(all) => all.iter().collect(),
                        Link::Cell { .. } => vec![l],
                        _ => Vec::new(),
                    };
                    for p in named {
                        if let Link::Cell { cell, portal } = p {
                            if let Some(part) = cell
                                .strip_prefix(PART)
                                .and_then(|n| n.parse::<usize>().ok())
                            {
                                joins.push(((with.0, k, slot), (with.1, part, *portal)));
                                own[slot] = Link::Shut;
                            }
                        }
                    }
                }
                parts.push((u16::try_from(k).unwrap_or(0), own));
            }
            let surfaces = self.surfaces_for(lb, &pieces[c]);
            return Some((
                NewPiece {
                    environment: pieces[c].environment,
                    surfaces,
                    at: at_of(&best.frame),
                    rotation: rotation_of(&best.frame),
                    parts,
                },
                joins,
            ));
        }
        None
    }

    /// Lids for dungeon `lb` over the open tops `tops` among `rooms`, as many as it takes: each
    /// the ceiling of the look that closes the most of the tops left.
    pub(super) fn lids_over(
        &mut self,
        lb: u16,
        rooms: &BTreeMap<u16, lyceum_foundry::room::Solid>,
        mut tops: Vec<Doorway>,
    ) -> Vec<NewPiece> {
        let mut out = Vec::new();
        while !tops.is_empty() {
            let Some((lid, _)) = self.lid_piece(lb, rooms, &tops, (0, 0)) else {
                break;
            };
            let closes = joined_links(lid.parts.iter().map(|(_, l)| l));
            let left = tops.len();
            tops.retain(|t| !closes.contains(&t.link));
            if tops.len() == left {
                break;
            }
            out.push(lid);
        }
        out
    }

    /// The steps that take away the lids of dungeon `lb` over the openings `links` join: the
    /// pack's own cells removed, the world's own taken away.
    pub(super) fn lids_giving_way(&mut self, lb: u16, links: &[Vec<Link>]) -> Vec<Op> {
        let joined = joined_links(links.iter());
        self.lids_away(lb, &|f| joined.contains(&f.link))
    }

    /// The steps that take away the lids of dungeon `lb` over any opening `over` takes: the
    /// pack's own cells removed, the world's own taken away.
    pub(super) fn lids_away(&mut self, lb: u16, over: &dyn Fn(&Free) -> bool) -> Vec<Op> {
        let lids = self.lids(lb);
        let going: BTreeSet<usize> = lids
            .ways
            .iter()
            .filter(|(f, _)| over(f))
            .map(|(_, k)| *k)
            .collect();
        let names = self.session.cell_names(lb);
        let mut ops = Vec::new();
        let mut world = Vec::new();
        for k in going {
            for c in &lids.pieces[k].cells {
                match names.get(c) {
                    Some(name) => ops.push(Op::RemoveCell {
                        landblock: lb,
                        name: name.clone(),
                    }),
                    None => world.push(*c),
                }
            }
        }
        if !world.is_empty() {
            ops.push(Op::TakeCells {
                landblock: lb,
                cells: world,
            });
        }
        ops
    }

    /// Set kit piece `i` down at `placement` in dungeon `lb` (a lid over its open top with it, and
    /// the lids over the openings it joins taken away), as one step: its first room selected
    /// after.
    ///
    /// # Errors
    /// The pack would not take it.
    pub fn set_piece(
        &mut self,
        lb: u16,
        i: usize,
        placement: &PiecePlacement,
    ) -> Result<(), String> {
        if self.cells_of(lb).is_empty() {
            self.room_for_dungeon(lb)?;
        }
        let (pieces, joins) = self
            .new_pieces(lb, i, placement)
            .ok_or_else(|| self.say("messages.pieces_gathering"))?;
        let before: BTreeSet<String> = self.session.cell_names(lb).into_values().collect();
        let first = self.cells_of(lb).is_empty();
        let mut ops = self.lids_giving_way(lb, &placement.links);
        ops.push(Op::PlacePieces {
            landblock: lb,
            pieces,
            joins,
        });
        let op = if ops.len() == 1 {
            ops.remove(0)
        } else {
            Op::Batch(ops)
        };
        self.apply(&op, false)?;
        self.dungeon_block = Some(lb);
        let made: Vec<u16> = self
            .session
            .cell_names(lb)
            .into_iter()
            .filter(|(_, name)| !before.contains(name))
            .map(|(c, _)| c)
            .collect();
        if let Some(n) = made.first() {
            self.selection = Some(super::Selection::Cell {
                landblock: lb,
                cell: *n,
            });
            self.also.clear();
            if first {
                let _ = self.fly_into_cell(lb, *n);
                self.in_dungeon = true;
            }
        }
        self.layout.target = None;
        self.kit.hover_fit = None;
        self.layout.turn = 0;
        let label = self
            .kit
            .pieces
            .as_ref()
            .and_then(|p| p.get(i))
            .map(KitPiece::label)
            .unwrap_or_default();
        self.panels.message = self.say_with("layout.placed", &[("piece", &label)]);
        Ok(())
    }

    /// Put in the best of what fits at the dungeon's first free doorway on a side (its middle
    /// square when it has no room yet) of the kind named `kind`: `corridor`, `corner`, `tee`,
    /// `cross`, `dead end`, `stairs`, or `any`; `dead ends` closes every side doorway left free
    /// with one.
    ///
    /// # Errors
    /// Not in a dungeon, no doorway left free, nothing of that kind fits, or the pack would not
    /// take it.
    pub fn fit_kind(&mut self, kind: &str) -> Result<(), String> {
        use lyceum_foundry::kit::Kind;
        if kind == "dead ends" {
            let lb = self
                .dungeon_mode()
                .ok_or_else(|| self.say("messages.off_world"))?;
            for _ in 0..64 {
                if self.side_doorways(lb).is_empty() {
                    return Ok(());
                }
                self.fit_kind("dead end")?;
            }
            return Err(self.say("messages.no_free_portal"));
        }
        let want = match kind {
            "corridor" => Some(Kind::Corridor),
            "corner" => Some(Kind::Corner),
            "tee" => Some(Kind::Tee),
            "cross" => Some(Kind::Cross),
            "dead end" => Some(Kind::DeadEnd),
            "stairs" => Some(Kind::Stairs),
            "any" => None,
            other => return Err(format!("{other:?} is not a kind of piece")),
        };
        self.wait_for_pieces();
        let lb = self
            .dungeon_mode()
            .ok_or_else(|| self.say("messages.off_world"))?;
        self.layout.target = Some(if self.cells_of(lb).is_empty() {
            Target::Square((9, 9))
        } else {
            let free = self.side_doorways(lb);
            Target::Doorway(
                free.into_iter()
                    .next()
                    .ok_or_else(|| self.say("messages.no_free_portal"))?
                    .link,
            )
        });
        let fits = self.fits(lb);
        let pieces = self.kit.pieces.clone().unwrap_or_default();
        let k = fits
            .iter()
            .position(|f| {
                want.is_none_or(|w| pieces.get(f.piece).is_some_and(|p| p.form.kind == w))
            })
            .ok_or_else(|| self.say("layout.no_fit"))?;
        self.place_fit(k)
    }

    /// Put fit `k` of the fits listed now in.
    ///
    /// # Errors
    /// No such fit, or the pack would not take it.
    pub fn place_fit(&mut self, k: usize) -> Result<(), String> {
        let lb = self
            .dungeon_mode()
            .ok_or_else(|| self.say("messages.off_world"))?;
        let fit = self
            .fits(lb)
            .get(k)
            .cloned()
            .ok_or_else(|| self.say("layout.no_fit"))?;
        self.set_piece(lb, fit.piece, &fit.placement)
    }

    /// The piece the room `cell` of landblock `lb` is part of: its environment, frame and cells.
    #[must_use]
    pub fn room_piece(&self, lb: u16, cell: u16) -> Option<lyceum_foundry::kit::Placed> {
        lyceum_foundry::kit::placed_pieces(&self.cells_of(lb))
            .into_iter()
            .find(|p| p.holds(cell))
    }

    /// Take the room `cell` of landblock `lb` away: its piece, whole, with what stands in it, and
    /// a ceiling over it that closes it alone; the doorways into it free again, but an open top it
    /// stood on, closed by a lid. One step.
    ///
    /// # Errors
    /// No such room, or the pack would not take it.
    pub fn delete_room(&mut self, lb: u16, cell: u16) -> Result<(), String> {
        let piece = self
            .room_piece(lb, cell)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        let cells = self.cells_of(lb);
        let mut gone: BTreeSet<u16> = piece.cells.iter().copied().collect();
        // A ceiling joined to nothing but it goes with it.
        let ceiling = |c: &lyceum_foundry::kit::Placed, this: &Self| {
            this.piece_of(c.environment)
                .and_then(|i| this.kit.pieces.as_ref().and_then(|p| p.get(i)))
                .is_some_and(|p| p.form.kind == Kind::Ceiling)
        };
        for other in lyceum_foundry::kit::placed_pieces(&cells) {
            if other.cells.iter().any(|c| gone.contains(c)) || !ceiling(&other, self) {
                continue;
            }
            let leads: BTreeSet<u16> = other
                .cells
                .iter()
                .filter_map(|c| cells.get(c))
                .flat_map(|c| c.portals.iter())
                .filter(|p| p.flags & 4 == 0)
                .filter_map(|p| u16::try_from(p.other_cell_id).ok())
                .collect();
            if !leads.is_empty() && leads.iter().all(|c| gone.contains(c)) {
                gone.extend(other.cells.iter().copied());
            }
        }
        let names = self.session.cell_names(lb);
        let mut ops: Vec<Op> = gone
            .iter()
            .filter_map(|c| names.get(c))
            .map(|name| Op::RemoveCell {
                landblock: lb,
                name: name.clone(),
            })
            .collect();
        let world: Vec<u16> = gone
            .iter()
            .copied()
            .filter(|c| !names.contains_key(c))
            .collect();
        if !world.is_empty() {
            ops.push(Op::TakeCells {
                landblock: lb,
                cells: world,
            });
        }
        // The open tops it stood on, closed again by lids.
        let tops = self.tops_under(lb, &gone);
        if !tops.is_empty() {
            let (mut rooms, _, _) = self.grid_world(lb);
            for c in &gone {
                rooms.remove(c);
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
        let op = if ops.len() == 1 {
            ops.remove(0)
        } else {
            Op::Batch(ops)
        };
        self.apply(&op, false)?;
        self.selection = None;
        self.also.clear();
        self.panels.message = self.say_with("layout.deleted", &[("n", &gone.len())]);
        Ok(())
    }

    /// The chosen piece set down `times` times, each at the first free doorway of the dungeon the
    /// camera is in where it fits (the first room ahead of the camera), as a script grows a
    /// dungeon.
    ///
    /// # Errors
    /// No piece chosen, no free doorway it fits, or a placement refused.
    pub fn extend(&mut self, times: u32) -> Result<(), String> {
        let i = self
            .kit
            .chosen
            .ok_or_else(|| self.say("messages.choose_piece"))?;
        for _ in 0..times {
            let (lb, ..) = self
                .work_block()
                .ok_or_else(|| self.say("messages.off_world"))?;
            let pieces = self
                .kit
                .pieces
                .clone()
                .ok_or_else(|| self.say("messages.pieces_gathering"))?;
            let spaces = self
                .piece_spaces(i)
                .ok_or_else(|| self.say("messages.environment_missing"))?;
            let placement = if self.cells_of(lb).is_empty() {
                let o = lyceum_foundry::pick::block_origin(lb);
                let p = self.camera.position;
                let s = grid::square_of(p.x - o.x, p.y - o.y);
                grid::fit_piece(
                    &pieces[i].form,
                    &spaces,
                    &grid::square_frame(s, super::layout::FIRST_FLOOR - pieces[i].form.lo.z, 0),
                    &grid::Dungeon {
                        rooms: &BTreeMap::new(),
                        doorways: &[],
                        ground: None,
                    },
                )
                .map_err(|_| self.say("layout.no_fit"))?
            } else {
                let (rooms, doorways, _) = self.grid_world(lb);
                let among: Vec<usize> = (0..doorways.len()).collect();
                grid::piece_placements(
                    &pieces[i].form,
                    &spaces,
                    &grid::Dungeon {
                        rooms: &rooms,
                        doorways: &doorways,
                        ground: None,
                    },
                    &among,
                )
                .into_iter()
                .next()
                .ok_or_else(|| self.say("messages.no_free_portal"))?
            };
            self.set_piece(lb, i, &placement)?;
        }
        Ok(())
    }

    /// Put the camera in the middle of the pack's cell `name` (its symbol, bare or in full) of the
    /// camera's landblock.
    ///
    /// # Errors
    /// No such cell.
    pub fn go_inside(&mut self, name: &str) -> Result<(), String> {
        let (lb, ..) = self
            .work_block()
            .ok_or_else(|| self.say("messages.off_world"))?;
        self.go_inside_at(lb, name)
    }

    /// Put the camera in the middle of the pack's cell `name` of landblock `lb`.
    ///
    /// # Errors
    /// No such cell.
    pub fn go_inside_at(&mut self, lb: u16, name: &str) -> Result<(), String> {
        let (c, _) = self
            .session
            .cell_names(lb)
            .into_iter()
            .find(|(_, n)| n == name || n.rsplit(':').next() == Some(name))
            .ok_or_else(|| {
                self.say_with(
                    "messages.no_cell",
                    &[("name", &name), ("landblock", &format!("{lb:04X}"))],
                )
            })?;
        self.fly_into_cell(lb, c)
    }

    /// Put the camera in the middle of cell `c` of landblock `lb`, the world's or a pack's.
    ///
    /// # Errors
    /// No such cell, or its environment is not in the files.
    pub fn fly_into_cell(&mut self, lb: u16, c: u16) -> Result<(), String> {
        let cell = self
            .session
            .cells(lb)
            .remove(&c)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        let cs = self
            .environment(cell.environment)
            .and_then(|e| e.cells.get(usize::from(cell.cell_struct)))
            .cloned()
            .ok_or_else(|| self.say("messages.environment_missing"))?;
        let vs = &cs.vertex_array.vertices;
        #[allow(clippy::cast_precision_loss)]
        let n = vs.len().max(1) as f32;
        let mid = vs.iter().fold(Vec3::ZERO, |m, v| {
            let p = localtoglobal(&cell.frame, v.position);
            Vec3::new(m.x + p.x / n, m.y + p.y / n, m.z + p.z / n)
        });
        self.camera.position = mid.add(lyceum_foundry::pick::block_origin(lb));
        self.dungeon_block = Some(lb);
        self.rooms = None;
        Ok(())
    }

    /// Cell `cell` of landblock `lb` outlined on the world, every edge of its polygons.
    pub(super) fn outline_cell(
        &mut self,
        painter: &egui::Painter,
        view: &View,
        lb: u16,
        cell: u16,
        width: f32,
        color: egui::Color32,
    ) {
        let Some(c) = self.cells_of(lb).get(&cell).cloned() else {
            return;
        };
        let Some(cs) = self
            .environment(c.environment)
            .and_then(|e| e.cells.get(usize::from(c.cell_struct)))
            .cloned()
        else {
            return;
        };
        let base = lyceum_foundry::pick::block_origin(lb);
        let at = |i: &u16| {
            cs.vertex_array
                .vertices
                .get(usize::from(*i))
                .map(|v| localtoglobal(&c.frame, v.position).add(base))
        };
        for poly in &cs.polygons {
            let pts: Vec<Vec3> = poly.vertex_ids.iter().filter_map(at).collect();
            for (k, p) in pts.iter().enumerate() {
                if let (Some(a), Some(b)) =
                    (view.project(*p), view.project(pts[(k + 1) % pts.len()]))
                {
                    painter.line_segment(
                        [egui::pos2(a.0, a.1), egui::pos2(b.0, b.1)],
                        egui::Stroke::new(width, color),
                    );
                }
            }
        }
    }

    /// The dungeon's marks on the world: its free doorways in the selection's color, the room
    /// selected outlined whole (every cell of its piece), and the ghost's edges.
    pub(super) fn dungeon_marks(&mut self, painter: &egui::Painter, view: &View) {
        let Some(lb) = self.dungeon_mode() else {
            return;
        };
        let base = lyceum_foundry::pick::block_origin(lb);
        let line = |a: Vec3, b: Vec3, w: f32, c: egui::Color32| {
            if let (Some(p), Some(q)) = (view.project(a.add(base)), view.project(b.add(base))) {
                painter.line_segment(
                    [egui::pos2(p.0, p.1), egui::pos2(q.0, q.1)],
                    egui::Stroke::new(w, c),
                );
            }
        };
        for f in self.free_portals(lb, &[]) {
            let pts = &f.opening.corners;
            for (k, p) in pts.iter().enumerate() {
                line(
                    *p,
                    pts[(k + 1) % pts.len()],
                    2.5,
                    super::panels::mark::SELECT,
                );
            }
        }
        if let Some(super::Selection::Cell { landblock, cell }) = self.selection.clone() {
            if let Some(piece) = self.room_piece(landblock, cell) {
                for c in piece.cells {
                    if c != cell {
                        self.outline_cell(
                            painter,
                            view,
                            landblock,
                            c,
                            1.0,
                            super::panels::mark::OUTLINE,
                        );
                    }
                }
            }
        }
        // The ghost's edges: green where it fits.
        let Some(g) = self.layout.ghost.clone() else {
            return;
        };
        let Some(env) = self
            .kit
            .pieces
            .as_ref()
            .and_then(|p| p.get(g.piece))
            .map(|p| p.environment)
        else {
            return;
        };
        let Some(e) = self.environment(env).cloned() else {
            return;
        };
        let color = if g.fits() {
            egui::Color32::from_rgb(90, 230, 110)
        } else {
            egui::Color32::from_rgb(255, 90, 80)
        };
        for cs in &e.cells {
            for poly in &cs.polygons {
                let pts: Vec<Vec3> = poly
                    .vertex_ids
                    .iter()
                    .filter_map(|i| cs.vertex_array.vertices.get(usize::from(*i)))
                    .map(|v| localtoglobal(&g.frame, v.position))
                    .collect();
                for (k, p) in pts.iter().enumerate() {
                    line(*p, pts[(k + 1) % pts.len()], 1.5, color);
                }
            }
        }
    }

    /// The tools in dungeon mode with the select tool: the dungeon's card (its look first), then
    /// what fits where the grid's choice is, or the look's pieces in their groups.
    pub(super) fn dungeon_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some(lb) = self.dungeon_mode() else {
            return;
        };
        if self.kit_pieces().is_none() {
            self.dungeon_card(ui, lb, out);
            ui.weak(self.text.table().get("dungeon.gathering"));
            return;
        }
        if self.kit.look.is_none() {
            self.kit.look = self.dungeon_look(lb);
        }
        self.dungeon_card(ui, lb, out);
        if self.layout.target.is_some() {
            self.fits_list(ui, lb, out);
            return;
        }
        let t = self.text.table();
        if self.layout.moving.is_some() {
            ui.colored_label(super::panels::mark::TURN, t.get("layout.lifted"));
        } else if self.kit.look.is_none() && !self.kit.every {
            ui.colored_label(super::panels::mark::TURN, t.get("dungeon.choose_look"));
        } else {
            ui.weak(t.get("dungeon.help"));
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.kit.filter)
                    .hint_text(t.get("dungeon.filter"))
                    .desired_width((ui.available_width() - 120.0).max(80.0)),
            );
            ui.checkbox(&mut self.kit.every, t.get("dungeon.every"))
                .on_hover_text(t.get("dungeon.every_tip"));
        });
        self.palette(ui, out);
    }

    /// The pieces that fit where the grid's choice is, best first, each turned as it would stand;
    /// the pointer over one shows its ghost, a click sets it in.
    fn fits_list(&mut self, ui: &mut egui::Ui, lb: u16, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let fits = self.fits(lb);
        ui.horizontal_wrapped(|ui| {
            let what = match &self.layout.target {
                Some(Target::Square(s)) => t.fill("dungeon.square", &[("x", &s.0), ("y", &s.1)]),
                _ => t.get("dungeon.doorway").to_owned(),
            };
            ui.colored_label(egui::Color32::from_rgb(90, 160, 240), what);
            if ui.small_button(t.get("dungeon.square_clear")).clicked() {
                out.push(UiAction::Target(None));
            }
        });
        if fits.is_empty() {
            ui.weak(t.get("dungeon.nothing_fits"));
            return;
        }
        ui.label(t.fill("dungeon.fits", &[("n", &fits.len())]));
        let pieces = self.kit.pieces.clone().unwrap_or_default();
        let cell = egui::vec2(96.0, 118.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = ((ui.available_width() / (cell.x + 4.0)).floor().max(1.0) as usize).max(1);
        let rows = fits.len().div_ceil(columns);
        self.kit.fit_rects.clear();
        let mut hovered = None;
        // Each choice of where to build lists from its top.
        let choice = format!("{:?}", self.layout.target);
        egui::ScrollArea::vertical()
            .id_salt(("fits", choice))
            .auto_shrink(false)
            .show_rows(ui, cell.y + 4.0, rows, |ui, shown| {
                self.kit.fits_view = Some(ui.clip_rect());
                for row in shown {
                    ui.horizontal(|ui| {
                        for k in (row * columns)..((row + 1) * columns).min(fits.len()) {
                            let f = &fits[k];
                            let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
                            let turn = f.placement.square().map_or(0, |(_, t)| t);
                            let lit = r.hovered() || self.kit.hover_fit == Some(k);
                            self.draw_piece(ui, rect, f.piece, turn, lit, false);
                            self.kit.fit_rects.push((k, rect));
                            if r.hovered() {
                                hovered = Some(k);
                            }
                            let p = &pieces[f.piece];
                            let tip = self.piece_tip(p, Some(f.score.0));
                            if r.on_hover_text(tip).clicked() {
                                out.push(UiAction::PlaceFit(k));
                            }
                        }
                    });
                }
            });
        if hovered != self.kit.hover_fit {
            self.kit.hover_fit = hovered;
        }
    }

    /// What a piece is, for its tooltip: its name, what it is on the grid, its doorways and
    /// size, how often the world places it, and the doorways it joins here.
    fn piece_tip(&mut self, p: &KitPiece, joins: Option<usize>) -> String {
        let t = self.text.table();
        let kind = self.kind_name(p.form.kind);
        let mut s = t.fill(
            "dungeon.piece_tip",
            &[
                ("piece", &p.label()),
                ("kind", &kind),
                ("doors", &p.form.doors.len()),
                (
                    "size",
                    &format!(
                        "{:.0} × {:.0} × {:.0}",
                        p.form.hi.x - p.form.lo.x,
                        p.form.hi.y - p.form.lo.y,
                        p.form.height()
                    ),
                ),
                ("placed", &p.placed),
            ],
        );
        if p.form.rise > 0.0 {
            s.push('\n');
            s.push_str(&t.fill("dungeon.rises", &[("m", &format!("{:.0}", p.form.rise))]));
        }
        if let Some(n) = joins {
            s.push('\n');
            s.push_str(&t.fill("dungeon.joins_here", &[("n", &n)]));
        }
        let look = self.kit.look;
        if !p.looks.is_empty() && look.is_some_and(|l| !p.in_look(l)) {
            s.push('\n');
            s.push_str(t.get("dungeon.by_role"));
        }
        s
    }

    /// The name of what a piece is on the grid, as a group's header says it.
    #[must_use]
    pub fn kind_name(&self, k: Kind) -> String {
        let t = self.text.table();
        match k {
            Kind::Room { long, short } => {
                t.fill("dungeon.kind_room", &[("long", &long), ("short", &short)])
            }
            k => t.get(&format!("dungeon.kind_{}", k.key())).to_owned(),
        }
    }

    /// The look's pieces in their groups, each group under its header.
    fn palette(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let look = self.kit.look;
        let every = self.kit.every;
        let groups = self.piece_groups(look, every);
        let pieces = self.kit.pieces.clone().unwrap_or_default();
        let cell = egui::vec2(96.0, 118.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = ((ui.available_width() / (cell.x + 4.0)).floor().max(1.0) as usize).max(1);
        // The rows: each group's header, then its pieces.
        enum Row {
            Header(Kind, usize),
            Pieces(Vec<usize>),
        }
        let mut rows: Vec<Row> = Vec::new();
        for (k, list) in &groups {
            rows.push(Row::Header(*k, list.len()));
            for chunk in list.chunks(columns) {
                rows.push(Row::Pieces(chunk.to_vec()));
            }
        }
        let reveal = self.kit.reveal.take().and_then(|r| {
            rows.iter()
                .position(|row| matches!(row, Row::Pieces(v) if v.contains(&r)))
        });
        self.kit.piece_rects.clear();
        self.kit.group_rects.clear();
        let header_h = 26.0;
        let mut area = egui::ScrollArea::vertical()
            .id_salt("pieces")
            .auto_shrink(false);
        if let Some(r) = reveal {
            let y: f32 = rows[..r]
                .iter()
                .map(|row| match row {
                    Row::Header(..) => header_h,
                    Row::Pieces(_) => cell.y + 4.0,
                })
                .sum::<f32>();
            area = area.vertical_scroll_offset(y);
        }
        area.show(ui, |ui| {
            for row in &rows {
                match row {
                    Row::Header(k, n) => {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), header_h),
                            egui::Sense::hover(),
                        );
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect.shrink(1.0), 3.0, egui::Color32::from_gray(32));
                        let name = self.kind_name(*k);
                        painter.text(
                            rect.left_center() + egui::vec2(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            format!("{name}  ({n})"),
                            egui::FontId::proportional(14.0),
                            egui::Color32::from_gray(230),
                        );
                        self.kit.group_rects.push((*k, rect));
                    }
                    Row::Pieces(v) => {
                        ui.horizontal(|ui| {
                            for i in v {
                                let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
                                let chosen = self.kit.chosen == Some(*i);
                                self.draw_piece(ui, rect, *i, 0, chosen || r.hovered(), chosen);
                                self.kit.piece_rects.push((*i, rect));
                                let tip = self.piece_tip(&pieces[*i], None);
                                if r.on_hover_text(tip).clicked() {
                                    out.push(UiAction::Piece(*i));
                                }
                            }
                        });
                    }
                }
            }
        });
    }

    /// One piece's tile: its floor plan from above, turned `turn` quarter turns, and its label
    /// under it.
    pub(super) fn draw_piece(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        i: usize,
        turn: u8,
        lit: bool,
        chosen: bool,
    ) {
        let painter = ui.painter_at(rect);
        if lit {
            painter.rect_filled(
                rect,
                3.0,
                if chosen {
                    egui::Color32::from_rgb(40, 60, 90)
                } else {
                    egui::Color32::from_gray(45)
                },
            );
        }
        let plan_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(6.0, 4.0),
            egui::vec2(rect.width() - 12.0, rect.width() - 12.0),
        );
        self.draw_plan(&painter, plan_rect, i, turn);
        let Some(p) = self.kit.pieces.as_ref().and_then(|p| p.get(i)).cloned() else {
            return;
        };
        let galley = painter.layout(
            p.label(),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(220),
            rect.width() - 4.0,
        );
        painter.galley(
            egui::pos2(
                rect.center().x - galley.size().x / 2.0,
                plan_rect.max.y + 2.0,
            ),
            galley,
            egui::Color32::from_gray(220),
        );
    }

    /// Kit piece `i`'s floor plan in `rect`, turned `turn` quarter turns: its floors filled, the
    /// higher lighter; its outline; its side doorways as bars in the selection's color, its
    /// openings up and down outlined (up blue, down red) with an arrow; and how far it rises.
    pub(super) fn draw_plan(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        i: usize,
        turn: u8,
    ) {
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(20));
        let Some(plan) = self.piece_plan_of(i) else {
            return;
        };
        // Square tiles show a square of the grid as a square: the plan's extent, turned.
        let corners = [
            Vec3::new(plan.lo.x, plan.lo.y, 0.0),
            Vec3::new(plan.hi.x, plan.hi.y, 0.0),
        ];
        let t0 = turned(corners[0], turn);
        let t1 = turned(corners[1], turn);
        let (lo, hi) = (
            (t0.x.min(t1.x), t0.y.min(t1.y)),
            (t0.x.max(t1.x), t0.y.max(t1.y)),
        );
        let span = (hi.0 - lo.0).max(hi.1 - lo.1).max(0.1);
        let k = (rect.width() - 10.0) / span;
        let to = |p: Vec3| {
            let q = turned(p, turn);
            egui::pos2(
                rect.center().x + (q.x - (lo.0 + hi.0) / 2.0) * k,
                rect.center().y - (q.y - (lo.1 + hi.1) / 2.0) * k,
            )
        };
        let height = (plan.hi.z - plan.lo.z).max(0.1);
        for (poly, z) in &plan.floors {
            let shade = 45.0 + 70.0 * ((z - plan.lo.z) / height).clamp(0.0, 1.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let g = shade as u8;
            let pts: Vec<egui::Pos2> = poly.iter().map(|p| to(*p)).collect();
            painter.add(egui::Shape::convex_polygon(
                pts,
                egui::Color32::from_rgb(g, g, g.saturating_add(8)),
                egui::Stroke::NONE,
            ));
        }
        for (a, b) in &plan.edges {
            painter.line_segment(
                [to(*a), to(*b)],
                egui::Stroke::new(1.0, egui::Color32::from_gray(170)),
            );
        }
        let low_sill = plan
            .doors
            .iter()
            .filter(|d| d.side.is_some())
            .map(|d| d.sill)
            .fold(f32::MAX, f32::min);
        for d in &plan.doors {
            match d.side {
                Some(_) => {
                    let color = if d.sill > low_sill + lyceum_foundry::kit::RISE {
                        egui::Color32::from_rgb(120, 200, 255)
                    } else {
                        super::panels::mark::TURN
                    };
                    painter
                        .line_segment([to(d.ends.0), to(d.ends.1)], egui::Stroke::new(3.0, color));
                }
                None => {
                    let color = if d.up {
                        egui::Color32::from_rgb(90, 150, 255)
                    } else {
                        egui::Color32::from_rgb(240, 90, 90)
                    };
                    let mut pts: Vec<egui::Pos2> = d.corners.iter().map(|p| to(*p)).collect();
                    if let Some(first) = pts.first().copied() {
                        pts.push(first);
                    }
                    painter.add(egui::Shape::dashed_line(
                        &pts,
                        egui::Stroke::new(1.0, color),
                        3.0,
                        2.0,
                    ));
                }
            }
        }
        if plan.doors.iter().any(|d| d.side.is_none()) {
            let up = plan.doors.iter().any(|d| d.side.is_none() && d.up);
            let down = plan.doors.iter().any(|d| d.side.is_none() && !d.up);
            let mark = match (up, down) {
                (true, true) => "▲▼",
                (true, false) => "▲",
                _ => "▼",
            };
            painter.text(
                rect.right_top() + egui::vec2(-3.0, 2.0),
                egui::Align2::RIGHT_TOP,
                mark,
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(150, 190, 255),
            );
        }
        let rise = plan
            .doors
            .iter()
            .filter(|d| d.side.is_some())
            .map(|d| d.sill)
            .fold(f32::MIN, f32::max)
            - low_sill;
        if rise >= lyceum_foundry::kit::RISE && low_sill < f32::MAX {
            painter.text(
                rect.left_top() + egui::vec2(3.0, 2.0),
                egui::Align2::LEFT_TOP,
                format!("+{rise:.0} m"),
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(120, 200, 255),
            );
        }
    }

    /// Choose piece `i`, as the palette's click does: shown on the grid where it fits, unless a
    /// square or doorway is chosen, where it is then set at once when it fits there.
    ///
    /// # Errors
    /// It does not fit where the grid's choice is.
    pub fn choose_piece(&mut self, i: usize) -> Result<(), String> {
        self.kit.chosen = Some(i);
        self.layout.turn = 0;
        let Some(lb) = self.dungeon_mode() else {
            return Ok(());
        };
        if self.layout.target.is_none() {
            return Ok(());
        }
        let fit = self.fits(lb).iter().position(|f| f.piece == i);
        match fit {
            Some(k) => self.place_fit(k),
            None => Err(self.say("layout.no_fit")),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's piece palette)
    use super::*;
    use lyceum_foundry::kit::Form;
    use lyceum_foundry::pieces::Look;

    fn piece(env: u32, kind: Kind, looks: &[usize], placed: usize) -> KitPiece {
        KitPiece {
            environment: DataId(env),
            form: Form {
                slots: vec![2],
                inner: Vec::new(),
                doors: Vec::new(),
                kind,
                above: false,
                below: false,
                rise: 0.0,
                lo: Vec3::ZERO,
                hi: Vec3::new(10.0, 10.0, 6.0),
                footprint: (1, 1),
                roles: Vec::new(),
            },
            placed,
            landblocks: [(0x0111, 1)].into_iter().collect(),
            looks: looks
                .iter()
                .map(|f| Look {
                    family: *f,
                    cells: 1,
                    surfaces: Vec::new(),
                })
                .collect(),
            joins: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn the_palette_shows_a_looks_pieces_by_what_they_are_on_the_grid_the_commonest_first() {
        let ps = [
            piece(0x0D00_0001, Kind::Corridor, &[0], 10),
            piece(0x0D00_0002, Kind::DeadEnd, &[0], 5),
            piece(0x0D00_0003, Kind::Corridor, &[0, 1], 40),
            piece(0x0D00_0004, Kind::Room { long: 2, short: 1 }, &[0], 1),
            piece(0x0D00_0005, Kind::Corner, &[1], 99),
        ];
        let none = BTreeSet::new();
        let g = grouped(&ps, Some(0), false, "", &none);
        assert_eq!(
            g,
            vec![
                (Kind::DeadEnd, vec![1]),
                (Kind::Corridor, vec![2, 0]),
                (Kind::Room { long: 2, short: 1 }, vec![3]),
            ]
        );
        // Every piece, in the look's groups.
        let all = grouped(&ps, Some(0), true, "", &none);
        assert_eq!(all.iter().map(|(_, v)| v.len()).sum::<usize>(), 5);
        // By its label.
        assert_eq!(
            grouped(&ps, Some(0), false, "0004", &none),
            vec![(Kind::Room { long: 2, short: 1 }, vec![3])]
        );
    }

    #[test]
    fn a_point_turned_by_quarters_goes_clockwise_about_the_origin() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(turned(p, 1), Vec3::new(2.0, -1.0, 3.0));
        assert_eq!(turned(p, 2), Vec3::new(-1.0, -2.0, 3.0));
        assert_eq!(turned(turned(p, 3), 1), p);
    }
}
