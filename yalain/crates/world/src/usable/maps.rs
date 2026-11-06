//! Surveyor's maps: each map leads to a hidden place inside its region's rectangle: the center of
//! one of the rectangle's 20 m cells, any one alike, kept with the map from its first use. Used within its radius of that place, it conjures its region's resource
//! node in front of the player as a delving stone does, lets its region's map recipes be drawn
//! again, and is thrown away; used farther off, it
//! says how far the place is ("You're close to the map's destination") with a marker on the
//! player's map, and whether the player is going the right way since it last took its bearings.

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use dereth_primitives::num::math;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::places::Use;
use crate::{progress, session, Clock, Server};

/// The calls that put a marker on the player's map and clear its markers.
pub mod function {
    pub const DISPLAY_MARKER: u32 = 0x8212_006C;
    pub const CLEAR_MARKERS: u32 = 0x8212_0061;
}

/// The kind of marker a map shows.
pub const MARKER_KIND: u32 = 1;

/// Why a map's use is refused: not near enough to its place.
pub const NOT_THERE: u32 = 10_054;

/// The control flags of a map's answer when it found its place, and when it did not.
pub const FOUND_FLAGS: u32 = 9149;
pub const NOT_THERE_FLAGS: u32 = 9020;

/// The kind of text a map's directions are shown as, and its find.
pub const SHOWN_AS: u32 = 1;
pub const FOUND_SHOWN_AS: u32 = 2;

/// The texts, in the map's own table of messages.
pub mod text {
    /// How far the place is, nearest first, each with the marker it shows; then "You're outside
    /// the borders of the map." beyond the farthest.
    pub const BANDS: [(u32, u32); 4] = [
        (0xEE1D_47E8, 0x4100_499C),
        (0x81EB_E5BB, 0x4100_499D),
        (0x3827_85D1, 0x4100_499E),
        (0x9CF9_A076, 0x4100_499F),
    ];
    pub const OUTSIDE: u32 = 0x68F1_9CA9;
    /// The first bearings taken this time in the world.
    pub const FIRST: u32 = 0x0B6B_893E;
    /// Not moved far from the last bearings.
    pub const STILL: u32 = 0x8D47_63B1;
    /// The change of distance since the last bearings, from much closer to much farther.
    pub const PROGRESS: [u32; 7] = [
        0x9A8F_597A,
        0xF840_EA41,
        0x6E28_9969,
        0xFCCE_9D30,
        0x6820_99D9,
        0x98C0_E141,
        0x928F_E97C,
    ];
    /// "Your search reveals a rich new source of raw materials! You discard the now-useless map.",
    /// with the summoning effect's name and the player's.
    pub const FOUND: (u32, u32) = (0x912E_FF1D, 0x2500_2B5F);
    pub const EFFECT_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_2B5F);
}

/// How far a player must move from its last bearings before the map compares distances, and the
/// steps of the change it tells apart, in meters.
pub const STILL_WITHIN: f32 = 20.0;
pub const STEPS: [f32; 6] = [-60.0, -40.0, -20.0, 20.0, 40.0, 60.0];

/// The band radii a map's template gives when it names none (close, medium, far, very far).
pub const BANDS: [f32; 4] = [100.0, 200.0, 400.0, 800.0];

/// The side of an outdoor cell and of a landblock, in meters.
const CELL: f32 = 20.0;
const BLOCK: f32 = 160.0;

/// The first resource node of each region's set.
const REGION_NODES: [(&str, u32); 3] = [
    ("Linvak", 0x4700_0C97),
    ("Omishan", 0x4700_0CA1),
    ("Osteth", 0x4700_0CA6),
];

/// The materials of a region's set of nodes, in its order.
const NODE_MATERIALS: [&str; 5] = ["Crystal", "Iron", "Silver", "Stone", "Wood"];

/// Every resource node a map can conjure.
pub fn nodes() -> impl Iterator<Item = u32> {
    REGION_NODES
        .iter()
        .flat_map(|(_, first)| (*first..).take(NODE_MATERIALS.len()))
}

/// The resource node a map's summoning effect names: its region's set, one node per material.
#[must_use]
pub fn summoned_by(effect: &str) -> Option<u32> {
    let rest = effect.strip_prefix("Eff_SummonMine_")?;
    let (material, region) = rest.split_once('_')?;
    let (_, first) = REGION_NODES.iter().find(|(r, _)| *r == region)?;
    let n = NODE_MATERIALS.iter().position(|m| *m == material)?;
    Some(first + u32::try_from(n).ok()?)
}

/// The region a map's summoning effect names (`Linvak`, `Omishan`, `Osteth`): the land it is a map
/// of.
#[must_use]
pub fn region(props: &yalain_content::Props) -> Option<&str> {
    ["Usage_Effect1", "Usage_Effect2"].iter().find_map(|k| {
        let rest = props.text(k)?.strip_prefix("Eff_SummonMine_")?;
        let (_, region) = rest.split_once('_')?;
        Some(region)
    })
}

/// A place in world meters: across and up from the world's corner.
#[must_use]
pub fn global(cell: u32, x: f32, y: f32) -> (f32, f32) {
    let lb = cell >> 16;
    #[allow(clippy::cast_precision_loss)]
    let (bx, by) = ((lb >> 8 & 0xFF) as f32, (lb & 0xFF) as f32);
    (bx * BLOCK + x, by * BLOCK + y)
}

/// An outdoor cell's square in world meters: its least and greatest corners.
#[must_use]
pub fn cell_square(cell: u32) -> ((f32, f32), (f32, f32)) {
    let i = (cell & 0xFFFF).saturating_sub(1);
    #[allow(clippy::cast_precision_loss)]
    let (col, row) = ((i / 8) as f32, (i % 8) as f32);
    let (x, y) = global(cell, col * CELL, row * CELL);
    ((x, y), (x + CELL, y + CELL))
}

/// The rectangle two outdoor cells span (each cell whole), in world meters.
#[must_use]
pub fn rectangle(a: u32, b: u32) -> ((f32, f32), (f32, f32)) {
    let ((ax0, ay0), (ax1, ay1)) = cell_square(a);
    let ((bx0, by0), (bx1, by1)) = cell_square(b);
    ((ax0.min(bx0), ay0.min(by0)), (ax1.max(bx1), ay1.max(by1)))
}

/// A place in world meters as a cell and a point in its landblock.
#[must_use]
pub fn place_of(gx: f32, gy: f32) -> StoredPosition {
    let (cell, x, y) = crate::spawn::outdoor(0, gx, gy);
    StoredPosition {
        cell,
        origin: [x, y, 0.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// How far across the ground one place lies from another, in meters along each axis.
#[must_use]
pub fn offset(from: &StoredPosition, to: &StoredPosition) -> (f32, f32) {
    let (ax, ay) = global(from.cell, from.origin[0], from.origin[1]);
    let (bx, by) = global(to.cell, to.origin[0], to.origin[1]);
    (bx - ax, by - ay)
}

/// The distance across the ground between two places, in meters.
#[must_use]
pub fn ground_distance(a: &StoredPosition, b: &StoredPosition) -> f32 {
    let (dx, dy) = offset(a, b);
    math::hypotf(dx, dy)
}

/// What a map's template says: where its place may be (a fixed place, or a rectangle to choose
/// one in), how near it must be used, its bands, its table of messages, its animation, and the
/// node it conjures.
#[derive(Debug, Clone, PartialEq)]
pub struct MapDef {
    pub fixed: Option<StoredPosition>,
    pub rectangle: Option<((f32, f32), (f32, f32))>,
    pub radius: f32,
    pub bands: [f32; 4],
    pub table: u32,
    pub animation: u32,
    pub node: u32,
    /// The region whose map recipes the map's find lets be drawn again, when its usage says so.
    pub resets: Option<usize>,
}

/// A template's map, if it is a surveyor's map.
#[must_use]
pub fn map_def(props: &yalain_content::Props) -> Option<MapDef> {
    let node = ["Usage_Effect1", "Usage_Effect2"]
        .iter()
        .find_map(|k| props.text(k).and_then(summoned_by))?;
    let fixed = props
        .place("Usage_RequiredLocation")
        .map(|(cell, origin)| StoredPosition {
            cell,
            origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
        });
    let rectangle = props
        .place("Usage_RequiredLocationTsysMin")
        .zip(props.place("Usage_RequiredLocationTsysMax"))
        .map(|((a, _), (b, _))| rectangle(a, b));
    if fixed.is_none() && rectangle.is_none() {
        return None;
    }
    let band = |k: &str, d: f32| props.float(k).unwrap_or(d);
    Some(MapDef {
        fixed,
        rectangle,
        radius: props.float("Usage_RequiredLocationRadius").unwrap_or(40.0),
        bands: [
            band("Usage_RequiredLocationCloseProximityMsgRadius", BANDS[0]),
            band("Usage_RequiredLocationMediumProximityMsgRadius", BANDS[1]),
            band("Usage_RequiredLocationFarProximityMsgRadius", BANDS[2]),
            band("Usage_RequiredLocationVeryFarProximityMsgRadius", BANDS[3]),
        ],
        table: props
            .word("Usage_ErrorMessagesTableID")
            .unwrap_or(0x2500_1B52),
        animation: props.word("Usage_Animation").unwrap_or(0x4100_00FC),
        node,
        resets: ["Usage_Effect1", "Usage_Effect2"]
            .iter()
            .find_map(|k| props.text(k).and_then(crate::craft::map_region_reset_by)),
    })
}

/// The text telling how the distance changed since the last bearings (negative: closer): much
/// closer, closer, a little closer (each by more than 60, 40, 20 m), about the same (within 20 m),
/// a little farther, farther, much farther.
#[must_use]
pub fn progress_text(change: f32) -> u32 {
    let i = if change < STEPS[0] {
        0
    } else if change < STEPS[1] {
        1
    } else if change < STEPS[2] {
        2
    } else if change <= STEPS[3] {
        3
    } else if change <= STEPS[4] {
        4
    } else if change <= STEPS[5] {
        5
    } else {
        6
    };
    text::PROGRESS[i]
}

/// The band of a distance (none: outside the map).
#[must_use]
pub fn band_of(bands: &[f32; 4], d: f32) -> Option<usize> {
    bands.iter().position(|r| d < *r)
}

/// The map's place: kept with the map, else chosen now (a fixed place, or the center of any of its
/// rectangle's cells, each alike) and kept.
fn destination<S: Store>(s: &mut Server<S>, item: u64, def: &MapDef) -> Option<StoredPosition> {
    if let Some(d) = s.things.get(item).and_then(|t| t.destination) {
        return Some(d);
    }
    let d = match (def.fixed, def.rectangle) {
        (Some(p), _) => p,
        (None, Some(((x0, y0), (x1, y1)))) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let cells = |lo: f32, hi: f32| ((hi - lo) / CELL).round().max(1.0) as usize;
            let (cx, cy) = (s.rng.below(cells(x0, x1)), s.rng.below(cells(y0, y1)));
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (x0 + (cx as f32 + 0.5) * CELL, y0 + (cy as f32 + 0.5) * CELL);
            place_of(x, y)
        }
        (None, None) => return None,
    };
    if let Some(t) = s.things.get_mut(item) {
        t.destination = Some(d);
    }
    Some(d)
}

fn show<S: Store>(_s: &Server<S>, net: &mut ServerNet, world: ConnId, kind: u32, t: (u32, u32)) {
    progress::private(net, world, &crate::chat::show_text(kind, progress::text(t)));
}

/// A player uses an item it holds: a surveyor's map is answered. Returns false for any other item.
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(def) = s
        .things
        .get(u.item)
        .and_then(|t| s.catalog.get(t.template))
        .and_then(|t| map_def(&t.props))
    else {
        return false;
    };
    let Some(to) = destination(s, u.item, &def) else {
        return false;
    };
    let Some(p) = s.players.get(&world) else {
        return true;
    };
    let (me, here, name) = (p.character, p.position, p.name.clone());
    let d = ground_distance(&here, &to);
    if d <= def.radius {
        // A node that can't be made takes no map: it's kept, and the summoning fails.
        if !crate::makes::can_make(s, def.node) {
            show(s, net, world, SHOWN_AS, crate::pets::text::SUMMONING_FAILS);
            let no = crate::places::use_refused(u, me, None, 0.0, crate::places::USE_STATUS_OK);
            progress::private(net, world, &no);
            return true;
        }
        found(s, net, world, u, &def, me, &name, clock);
        crate::world::keep_player(s, world);
        return true;
    }
    // Not there: how far, with a marker, and the bearings compared.
    if let Some(b) = band_of(&def.bands, d) {
        let (sid, marker) = text::BANDS[b];
        let band = (sid, def.table);
        show(s, net, world, SHOWN_AS, band);
        let m = session::call(
            function::DISPLAY_MARKER,
            &[
                Arg::Int(MARKER_KIND),
                Arg::LongInt(0),
                Arg::Int(here.cell),
                Arg::Int(marker),
                crate::chat::text_arg(progress::text(band)),
            ],
        )
        .expect("a marker packs");
        progress::private(net, world, &m);
        let bearing = s.things.get(u.item).and_then(|t| t.bearing);
        let (said, take) = match bearing {
            None => (text::FIRST, true),
            Some((at, _)) if ground_distance(&at, &here) < STILL_WITHIN => (text::STILL, false),
            Some((_, before)) => {
                let t = progress_text(d - before);
                (t, t != text::PROGRESS[3])
            }
        };
        show(s, net, world, SHOWN_AS, (said, def.table));
        if take {
            if let Some(t) = s.things.get_mut(u.item) {
                t.bearing = Some((here, d));
            }
        }
    } else {
        show(s, net, world, SHOWN_AS, (text::OUTSIDE, def.table));
    }
    let no = crate::places::use_refused(u, 0, Some(NOT_THERE_FLAGS), 0.0, NOT_THERE);
    progress::private(net, world, &no);
    crate::world::keep_player(s, world);
    true
}

/// The map's place is found: the map's animation, the surveying skill's experience, the find
/// told and a quest step it makes, the markers cleared, the map thrown away, the use answered, the
/// node in front of the player.
#[allow(clippy::too_many_arguments)]
fn found<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    def: &MapDef,
    me: u64,
    name: &str,
    clock: Clock,
) {
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    if let Some(p) = s.players.get_mut(&world) {
        let plays = ServerMessage::PlayBehavior(crate::combat::player_event(
            p,
            BehaviorParams {
                bits: BehaviorParams::BEHAVIOR | BehaviorParams::CONTEXT,
                behavior: def.animation,
                context,
                ..BehaviorParams::default()
            },
        ));
        crate::combat::to_all_seeing(s, net, me, &plays);
    }
    crate::mining::surveying_xp(s, net, world);
    if let Some(region) = def.resets {
        let now = net.game_time(clock.local);
        crate::craft::map_recipes_wait(s, net, world, region, now, now);
    }
    let said = progress::text_with(
        text::FOUND,
        vec![
            progress::nested(
                crate::mining::text::SUMMONED_EFFECT,
                progress::text(text::EFFECT_NAME),
            ),
            progress::nested(
                crate::mining::text::SUMMONED_PLAYER,
                crate::profile::literal(name),
            ),
        ],
    );
    progress::private(net, world, &crate::chat::show_text(FOUND_SHOWN_AS, said));
    // A quest step the map's find makes (the starter map's) comes with the find.
    let template = s.things.get(u.item).map_or(0, |t| t.template);
    crate::quests::triggered(
        s,
        net,
        world,
        yalain_content::quests::Trigger::Use(template),
        None,
        clock,
    );
    let clear =
        session::call(function::CLEAR_MARKERS, &[Arg::Int(MARKER_KIND)]).expect("an integer packs");
    progress::private(net, world, &clear);
    crate::inventory::take_item(s, net, world, u.item, template);
    let done = crate::places::use_done(u, me, FOUND_FLAGS, false, 0.0);
    progress::private(net, world, &done);
    crate::mining::conjure(s, net, world, def.node, true, clock);
}
