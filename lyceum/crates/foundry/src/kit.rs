//! The dungeon kit: the pieces a dungeon is built of, as the world's own dungeons are built.
//!
//! **A piece is an environment placed whole.** An environment holds several parts (its cell
//! structs), and the world places every part of one at a single frame: of the 347,496 placements
//! in its dungeons, 347,489 hold every part (`tests/dat/dungeon.rs` measures it). Each part
//! becomes a cell; the parts join each other through the openings they share, and the piece's
//! other openings are its doorways. A piece is what a builder sets down; a part is a detail of how it is drawn.
//!
//! **What a piece is on the grid** ([`Kind`]). Nearly every piece fills one square of the grid
//! (645,917 of the 647,080 dungeon rooms): rooms bigger than a square are built of square pieces
//! open to each other. A piece is told by the sides of its square its doorways open on: one side a
//! dead end, two opposite a corridor, two beside each other a corner, three a T, four a cross;
//! those bigger than a square are rooms of their footprint. A piece whose doorways stand at
//! different heights is stairs or a ramp. One that opens only up or down is a shaft, or a ceiling
//! when it is thin: most rooms are open above, closed by a ceiling piece or by the room over them.
//!
//! **Looks.** A piece's parts are drawn with one list of surfaces (106,119 of the 106,137
//! placements of more than one part), so a look of a piece is one list. A look a piece was never
//! drawn in dresses it by role ([`Role`]): each of its surfaces covers floors, ceilings or walls,
//! or several of them (trim), and the look's own pieces say which surface it uses for each.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::Environment;
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, Frame, Quat, Vec3};

use crate::grid::PITCH;
use crate::pieces::{mismatch, opening, Look, Opening};

/// How far apart two parts' openings may be and still be one opening between them, in meters.
const SHARED: f32 = 0.05;

/// How far a doorway's middle may stand off the side of its square and still open on it, in
/// meters.
const ON_SIDE: f32 = 0.3;

/// How much higher one doorway's sill stands than another's for a piece to be stairs, in meters.
pub const RISE: f32 = 1.5;

/// How tall a piece that opens only up or down may be and still be a ceiling, in meters.
pub const CEILING: f32 = 2.0;

/// A side of a square, by the way out of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    /// The side a level way out faces.
    #[must_use]
    pub fn of(out: Vec3) -> Self {
        if out.x.abs() > out.y.abs() {
            if out.x > 0.0 {
                Self::East
            } else {
                Self::West
            }
        } else if out.y > 0.0 {
            Self::North
        } else {
            Self::South
        }
    }

    /// Its place clockwise from north.
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }

    /// This side turned `turns` quarter turns clockwise.
    #[must_use]
    pub const fn turned(self, turns: u8) -> Self {
        Self::ALL[((self.index() + turns) % 4) as usize]
    }

    /// The side across the square.
    #[must_use]
    pub const fn opposite(self) -> Self {
        self.turned(2)
    }
}

/// One doorway of a piece: the part and the portal it is, where it is in the piece's own frame,
/// the side of the square it opens on (none for an opening in a floor or a ceiling), whether such
/// an opening leads up, its sill and its width.
#[derive(Clone, Debug, PartialEq)]
pub struct Door {
    pub part: u16,
    pub slot: usize,
    pub opening: Opening,
    pub side: Option<Side>,
    pub up: bool,
    pub sill: f32,
    pub width: f32,
}

/// What a piece is on the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    DeadEnd,
    Corridor,
    Corner,
    Tee,
    Cross,
    /// More than one square: so many squares along its longer side and its shorter.
    Room {
        long: u8,
        short: u8,
    },
    /// Its doorways stand at different heights: a stair, a ramp.
    Stairs,
    /// It opens only up or down.
    Shaft,
    /// It opens only up or down and is thin: what closes a room's open top.
    Ceiling,
    /// Its doorways do not stand on the sides of its squares, or it has none.
    Other,
}

impl Kind {
    /// The groups a browser shows, in order, rooms by the footprints `rooms` lists.
    #[must_use]
    pub fn groups(rooms: &BTreeSet<(u8, u8)>) -> Vec<Self> {
        let mut out = vec![
            Self::DeadEnd,
            Self::Corridor,
            Self::Corner,
            Self::Tee,
            Self::Cross,
        ];
        out.extend(rooms.iter().map(|(long, short)| Self::Room {
            long: *long,
            short: *short,
        }));
        out.extend([Self::Stairs, Self::Shaft, Self::Ceiling, Self::Other]);
        out
    }

    /// Its name's key.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::DeadEnd => "dead_end",
            Self::Corridor => "corridor",
            Self::Corner => "corner",
            Self::Tee => "tee",
            Self::Cross => "cross",
            Self::Room { .. } => "room",
            Self::Stairs => "stairs",
            Self::Shaft => "shaft",
            Self::Ceiling => "ceiling",
            Self::Other => "other",
        }
    }
}

/// What a surface of a piece covers: floors, ceilings or walls (by the way the polygons it lays
/// face), more than one of them (trim), or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Floor,
    Ceiling,
    Wall,
    Trim,
    Unused,
}

/// A piece's shape: its parts, the openings they share, its doorways, what it is on the grid,
/// how far it rises, its extent (in its own frame), and what each of its surfaces covers.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    /// Each part's portals, by count.
    pub slots: Vec<usize>,
    /// Each opening two of its parts share: (part, portal) and (part, portal).
    pub inner: Vec<((u16, usize), (u16, usize))>,
    pub doors: Vec<Door>,
    pub kind: Kind,
    /// Whether it opens up through its top, or down through its bottom.
    pub above: bool,
    pub below: bool,
    /// How much higher its highest doorway's sill stands than its lowest's.
    pub rise: f32,
    pub lo: Vec3,
    pub hi: Vec3,
    /// The squares it covers, along its own x and y.
    pub footprint: (u8, u8),
    /// What each surface of its list covers; as long as the list its polygons draw from.
    pub roles: Vec<Role>,
}

impl Form {
    /// Its parts.
    #[must_use]
    pub fn parts(&self) -> u16 {
        u16::try_from(self.slots.len()).unwrap_or(u16::MAX)
    }

    /// Its height, from its lowest point to its highest.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.hi.z - self.lo.z
    }
}

/// The inward facing of a polygon of cell struct `cs` with corners `pts`: its normal, turned to
/// face the part's middle.
fn inward(pts: &[Vec3], middle: Vec3) -> Option<Vec3> {
    if pts.len() < 3 {
        return None;
    }
    let mut n = Vec3::ZERO;
    for k in 0..pts.len() {
        let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    let len = n.dot(n).sqrt();
    if len < 1e-6 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // a polygon's corners
    let c = pts
        .iter()
        .fold(Vec3::ZERO, |s, p| s.add(*p))
        .mul(1.0 / pts.len() as f32);
    let n = n.mul(1.0 / len);
    Some(if n.dot(middle.sub(c)) < 0.0 {
        n.mul(-1.0)
    } else {
        n
    })
}

/// What each surface of environment `env`'s parts covers, by the way the polygons drawn with it
/// face: a floor faces up into its part, a ceiling down, a wall across.
#[must_use]
pub fn roles(env: &Environment) -> Vec<Role> {
    let mut seen: BTreeMap<usize, BTreeSet<Role>> = BTreeMap::new();
    for cs in &env.cells {
        let vs = &cs.vertex_array.vertices;
        #[allow(clippy::cast_precision_loss)] // a part's corners
        let middle = vs
            .iter()
            .fold(Vec3::ZERO, |s, v| s.add(v.position))
            .mul(1.0 / vs.len().max(1) as f32);
        let portals: BTreeSet<u16> = cs.portals.iter().copied().collect();
        for (i, p) in cs.polygons.iter().enumerate() {
            if u16::try_from(i).is_ok_and(|i| portals.contains(&i)) {
                continue;
            }
            let pts: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|k| vs.get(usize::from(*k)).map(|v| v.position))
                .collect();
            let Some(n) = inward(&pts, middle) else {
                continue;
            };
            let role = if n.z > 0.7 {
                Role::Floor
            } else if n.z < -0.7 {
                Role::Ceiling
            } else {
                Role::Wall
            };
            seen.entry(usize::from(p.pos_surface))
                .or_default()
                .insert(role);
        }
    }
    // As long as the list any polygon draws from, its openings' too.
    let named = env
        .cells
        .iter()
        .flat_map(|cs| cs.polygons.iter().map(|p| usize::from(p.pos_surface) + 1))
        .max()
        .unwrap_or(0);
    let len = seen.keys().next_back().map_or(0, |k| k + 1).max(named);
    (0..len)
        .map(
            |k| match seen.get(&k).map(|s| s.iter().copied().collect::<Vec<_>>()) {
                None => Role::Unused,
                Some(r) if r.len() == 1 => r[0],
                Some(_) => Role::Trim,
            },
        )
        .collect()
}

/// Environment `env` as a piece: its parts placed together at its own frame. `None` for one with
/// no parts.
#[must_use]
pub fn form(env: &Environment) -> Option<Form> {
    if env.cells.is_empty() {
        return None;
    }
    let at = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    let slots: Vec<usize> = env.cells.iter().map(|cs| cs.portals.len()).collect();
    let mut openings: Vec<(u16, usize, Opening)> = Vec::new();
    for (p, n) in slots.iter().enumerate() {
        let Ok(part) = u16::try_from(p) else { continue };
        for s in 0..*n {
            if let Some(o) = opening(env, part, s, &at) {
                openings.push((part, s, o));
            }
        }
    }
    let mut inner = Vec::new();
    let mut shared: BTreeSet<(u16, usize)> = BTreeSet::new();
    for (i, (pa, sa, a)) in openings.iter().enumerate() {
        for (pb, sb, b) in &openings[i + 1..] {
            if pa != pb && mismatch(a, b) < SHARED && a.out.dot(b.out) < -0.99 {
                inner.push(((*pa, *sa), (*pb, *sb)));
                shared.insert((*pa, *sa));
                shared.insert((*pb, *sb));
            }
        }
    }
    let (mut lo, mut hi) = (
        Vec3::new(f32::MAX, f32::MAX, f32::MAX),
        Vec3::new(f32::MIN, f32::MIN, f32::MIN),
    );
    for cs in &env.cells {
        for v in &cs.vertex_array.vertices {
            let p = v.position;
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
    }
    if lo.x > hi.x {
        return None;
    }
    let side_line = |v: f32| {
        let r = (v - PITCH / 2.0).rem_euclid(PITCH);
        r < ON_SIDE || r > PITCH - ON_SIDE
    };
    let mut doors = Vec::new();
    let mut off_grid = false;
    for (part, slot, o) in openings {
        if shared.contains(&(part, slot)) {
            continue;
        }
        let sill = o.corners.iter().map(|c| c.z).fold(f32::MAX, f32::min);
        let level = o.out.z.abs() <= 0.5;
        let side = level.then(|| Side::of(o.out));
        let along = |c: &Vec3| match side {
            Some(Side::East | Side::West) => c.y,
            _ => c.x,
        };
        let (a, b) = o.corners.iter().fold((f32::MAX, f32::MIN), |(a, b), c| {
            (a.min(along(c)), b.max(along(c)))
        });
        if let Some(s) = side {
            let line = match s {
                Side::East | Side::West => o.middle.x,
                Side::North | Side::South => o.middle.y,
            };
            off_grid |= !side_line(line);
        }
        doors.push(Door {
            part,
            slot,
            up: !level && o.out.z > 0.0,
            side,
            sill,
            width: b - a,
            opening: o,
        });
    }
    let level: Vec<&Door> = doors.iter().filter(|d| d.side.is_some()).collect();
    let sills = level.iter().map(|d| d.sill);
    let rise = sills.clone().fold(f32::MIN, f32::max) - sills.fold(f32::MAX, f32::min);
    let rise = if level.is_empty() { 0.0 } else { rise };
    let above = doors.iter().any(|d| d.side.is_none() && d.up);
    let below = doors.iter().any(|d| d.side.is_none() && !d.up);
    let squares = |a: f32, b: f32| {
        let n = dereth_primitives::num::floor_to_i32((b - a) / PITCH + 0.5).max(1);
        u8::try_from(n).unwrap_or(u8::MAX)
    };
    let footprint = (squares(lo.x, hi.x), squares(lo.y, hi.y));
    let open: BTreeSet<Side> = level.iter().filter_map(|d| d.side).collect();
    let kind = if off_grid {
        Kind::Other
    } else if rise >= RISE {
        Kind::Stairs
    } else if open.is_empty() {
        if !above && !below {
            Kind::Other
        } else if hi.z - lo.z < CEILING {
            Kind::Ceiling
        } else {
            Kind::Shaft
        }
    } else if footprint.0 > 1 || footprint.1 > 1 {
        Kind::Room {
            long: footprint.0.max(footprint.1),
            short: footprint.0.min(footprint.1),
        }
    } else {
        let has = |s: Side| open.contains(&s);
        match open.len() {
            1 => Kind::DeadEnd,
            2 if (has(Side::North) && has(Side::South)) || (has(Side::East) && has(Side::West)) => {
                Kind::Corridor
            }
            2 => Kind::Corner,
            3 => Kind::Tee,
            _ => Kind::Cross,
        }
    };
    Some(Form {
        slots,
        inner,
        doors,
        kind,
        above,
        below,
        rise,
        lo,
        hi,
        footprint,
        roles: roles(env),
    })
}

/// A piece of the kit: its environment, its shape, how often and where the world places it, the
/// looks it draws it in (most placements first), and the pieces it joins it to (by their place in
/// the kit, with how many times, most first).
#[derive(Clone, Debug, PartialEq)]
pub struct KitPiece {
    pub environment: DataId,
    pub form: Form,
    pub placed: usize,
    pub landblocks: BTreeMap<u16, usize>,
    pub looks: Vec<Look>,
    pub joins: Vec<(usize, usize)>,
    /// The surfaces it is drawn with when no look says otherwise: its commonest look's, or a new
    /// piece's own.
    pub surfaces: Vec<DataId>,
}

impl KitPiece {
    /// A short name: the environment's low half, `0046`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{:04X}", self.environment.raw() & 0xFFFF)
    }

    /// Whether the world draws it in look `family`.
    #[must_use]
    pub fn in_look(&self, family: usize) -> bool {
        self.looks.iter().any(|l| l.family == family)
    }

    /// A piece of `env` no world places yet, drawn with `surfaces`.
    #[must_use]
    pub fn new(env: &Environment, surfaces: &[DataId]) -> Option<Self> {
        Some(Self {
            environment: env.id,
            form: form(env)?,
            placed: 0,
            landblocks: BTreeMap::new(),
            looks: Vec::new(),
            joins: Vec::new(),
            surfaces: surfaces.to_vec(),
        })
    }
}

/// What a look draws each role with: the surface the pieces drawn in it use most for it.
pub type Dress = BTreeMap<Role, DataId>;

/// Each family's [`Dress`], from the looks of `pieces`.
#[must_use]
pub fn dresses(pieces: &[KitPiece], families: usize) -> Vec<Dress> {
    let mut votes: Vec<BTreeMap<Role, BTreeMap<DataId, usize>>> = vec![BTreeMap::new(); families];
    for p in pieces {
        for l in &p.looks {
            let Some(v) = votes.get_mut(l.family) else {
                continue;
            };
            for (k, s) in l.surfaces.iter().enumerate() {
                let role = p.form.roles.get(k).copied().unwrap_or(Role::Unused);
                if role == Role::Unused {
                    continue;
                }
                *v.entry(role).or_default().entry(*s).or_default() += l.cells;
            }
        }
    }
    votes
        .into_iter()
        .map(|v| {
            v.into_iter()
                .filter_map(|(r, counts)| {
                    counts
                        .into_iter()
                        .max_by_key(|(s, n)| (*n, std::cmp::Reverse(s.raw())))
                        .map(|(s, _)| (r, s))
                })
                .collect()
        })
        .collect()
}

/// How a piece is dressed in a look: by the world's own list, by role, or not at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dressed {
    /// The world draws the piece in this look: its list.
    Exact,
    /// Each surface by its role, as the look's own pieces draw that role.
    ByRole,
    /// The piece's own list kept: no look draws it (a pack's own piece), or the look draws no
    /// role.
    Kept,
}

/// The surfaces piece `p` is drawn with in look `family` (whose roles `dress` gives): the world's
/// own list for it there, else each surface by its role, a role the look never draws as its walls
/// are drawn (trim as walls, then anything the look draws), and one the piece leaves unused as
/// the piece's own; its own list, for a piece no look draws (a pack's own) or a look that draws
/// no role.
#[must_use]
pub fn dress(p: &KitPiece, family: usize, dress: Option<&Dress>) -> (Vec<DataId>, Dressed) {
    if let Some(l) = p.looks.iter().find(|l| l.family == family) {
        return (l.surfaces.clone(), Dressed::Exact);
    }
    let Some(d) = dress.filter(|d| !d.is_empty() && !p.looks.is_empty()) else {
        return (p.surfaces.clone(), Dressed::Kept);
    };
    let any = d.values().next().copied();
    let list = p
        .form
        .roles
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let own = p.surfaces.get(k).copied();
            match r {
                Role::Unused => own.or(any),
                Role::Trim => d.get(&Role::Trim).or_else(|| d.get(&Role::Wall)).copied(),
                r => d.get(r).or_else(|| d.get(&Role::Wall)).copied(),
            }
            .or(own)
            .or(any)
            .unwrap_or(DataId(0))
        })
        .collect();
    (list, Dressed::ByRole)
}

/// A kit piece as a landblock holds it: its environment, its frame, and its cells, one for each
/// part placed (by number).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub environment: DataId,
    pub frame: Frame,
    pub cells: Vec<u16>,
}

impl Placed {
    /// Whether it holds cell `n`.
    #[must_use]
    pub fn holds(&self, n: u16) -> bool {
        self.cells.contains(&n)
    }
}

/// The kit pieces landblock cells `cells` hold: its cells grouped by their environment and their
/// frame (to the tenth of a meter and the thousandth of a turn), each group in order of its first cell.
/// A cell taken away (closed far under the landblock) is in none.
#[must_use]
pub fn placed_pieces(cells: &BTreeMap<u16, dereth_assets::world::EnvCell>) -> Vec<Placed> {
    let round = |v: f32, k: f32| dereth_primitives::num::floor_to_i32(v * k + 0.5);
    let mut groups: BTreeMap<(u32, [i32; 7]), Placed> = BTreeMap::new();
    let mut order: Vec<(u32, [i32; 7])> = Vec::new();
    for (n, c) in cells {
        if c.frame.origin.z <= crate::cells::CLOSED_DEPTH + 1.0 {
            continue;
        }
        let (o, q) = (c.frame.origin, c.frame.rotation);
        let key = (
            c.environment.raw(),
            [
                round(o.x, 10.0),
                round(o.y, 10.0),
                round(o.z, 10.0),
                round(q.w, 1000.0),
                round(q.x, 1000.0),
                round(q.y, 1000.0),
                round(q.z, 1000.0),
            ],
        );
        groups
            .entry(key)
            .or_insert_with(|| {
                order.push(key);
                Placed {
                    environment: c.environment,
                    frame: c.frame,
                    cells: Vec::new(),
                }
            })
            .cells
            .push(*n);
    }
    order
        .into_iter()
        .filter_map(|k| groups.remove(&k))
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the dungeon kit's pieces)
    use super::*;

    #[test]
    fn a_side_turns_clockwise_by_quarters_and_faces_its_opposite_across_the_square() {
        assert_eq!(Side::North.turned(1), Side::East);
        assert_eq!(Side::West.turned(1), Side::North);
        assert_eq!(Side::South.turned(3), Side::East);
        assert_eq!(Side::East.opposite(), Side::West);
        assert_eq!(Side::of(Vec3::new(0.1, -1.0, 0.0)), Side::South);
        assert_eq!(Side::of(Vec3::new(-0.9, 0.2, 0.0)), Side::West);
    }

    fn piece(roles: Vec<Role>, surfaces: Vec<u32>, looks: Vec<(usize, Vec<u32>)>) -> KitPiece {
        KitPiece {
            environment: DataId(0x0D00_0001),
            form: Form {
                slots: vec![2],
                inner: Vec::new(),
                doors: Vec::new(),
                kind: Kind::Corridor,
                above: false,
                below: false,
                rise: 0.0,
                lo: Vec3::ZERO,
                hi: Vec3::new(10.0, 10.0, 6.0),
                footprint: (1, 1),
                roles,
            },
            placed: 1,
            landblocks: BTreeMap::new(),
            looks: looks
                .into_iter()
                .map(|(family, s)| Look {
                    family,
                    cells: 1,
                    surfaces: s.into_iter().map(DataId).collect(),
                })
                .collect(),
            joins: Vec::new(),
            surfaces: surfaces.into_iter().map(DataId).collect(),
        }
    }

    #[test]
    fn a_look_dresses_its_own_pieces_by_their_list_and_any_other_by_the_role_of_each_surface() {
        use Role::{Ceiling, Floor, Trim, Wall};
        // Look 0 draws a piece's floor with 0x11, its walls with 0x12, its ceiling with 0x13.
        let own = piece(
            vec![Floor, Wall, Ceiling],
            vec![1, 2, 3],
            vec![(0, vec![0x11, 0x12, 0x13])],
        );
        let table = dresses(std::slice::from_ref(&own), 1);
        assert_eq!(table[0].get(&Floor), Some(&DataId(0x11)));
        // Its own list for its own piece.
        let (list, how) = dress(&own, 0, table.first());
        assert_eq!(
            (list, how),
            (
                vec![DataId(0x11), DataId(0x12), DataId(0x13)],
                Dressed::Exact
            )
        );
        // Another piece, its surfaces laid out otherwise, by role: trim as walls.
        let other = piece(
            vec![Wall, Trim, Floor],
            vec![7, 8, 9],
            vec![(1, vec![7, 8, 9])],
        );
        let (list, how) = dress(&other, 0, table.first());
        assert_eq!(
            (list, how),
            (
                vec![DataId(0x12), DataId(0x12), DataId(0x11)],
                Dressed::ByRole
            )
        );
        // A look that draws no role: its own list, kept.
        assert_eq!(
            dress(&other, 0, Some(&Dress::new())),
            (vec![DataId(7), DataId(8), DataId(9)], Dressed::Kept)
        );
        // A piece no look draws, a pack's own: its own materials, kept.
        let made = piece(vec![Wall, Trim, Floor], vec![7, 8, 9], Vec::new());
        assert_eq!(
            dress(&made, 0, table.first()),
            (vec![DataId(7), DataId(8), DataId(9)], Dressed::Kept)
        );
    }
}
