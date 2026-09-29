// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Common/ObjectMaint.cs
//! Port of `Source/ACE.Server/Physics/Common/ObjectMaint.cs`.
//!
//! Player visibility tracking: which objects are currently known / visible to a player, which
//! players know each object (for broadcasts), and the monster / combat-pet target lists.
//!
//! ACE keeps one `ObjectMaint` on each `PhysicsObj` and its methods reach into the other object's
//! instance (`obj.ObjMaint.AddKnownPlayer(PhysicsObj)`). Here the instance lives in the body's
//! server-side record (`phys_ext`), every method is a free function on the `World` whose `this` is
//! the owning body's handle, and the other object is a handle too. The dictionaries are
//! `DotNetDict`s keyed as ACE keys them (the object id, or the body itself for the destruction
//! queue), so their enumeration order is ACE's. ACE's static reader-writer lock is dropped: the
//! world is single-threaded (V32's reasoning).

use dereth_physics::{LandSource, PhysHandle};
use dereth_primitives::{CellId, Position as PPosition};
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::PlayerKillerStatus;
use empyrean_entity::ObjectGuid;

use crate::physics::phys_ext::{self, distance_2d_squared};
use crate::physics::server_object_manager;
use crate::physics::weenie_object::WeenieObject;
use crate::World;

/// How long a known object stays known to a player after leaving the player's view; then it is
/// forgotten silently, and a return to view creates it afresh.
// ACE: ObjectMaint.DestructionTime
/// Not ACE's (retail, V278; the retail captures): ACE forgot after 25 s; the
/// retail server forgot 21 s after the object left the view, a sharp step (0 of 129 returns at
/// 20.0 s were created again, 100 of 133 at 21.0 s, 97% from 21 to 60 s), with no exception for
/// combat. A return after the deadline is created again at once whether or not the sweep has run
/// (`forget_if_expired`).
pub const DESTRUCTION_TIME: f32 = 21.0;

// ACE: ObjectMaint.InitialClamp
pub const INITIAL_CLAMP: bool = true;

// ACE: ObjectMaint.InitialClamp_Dist
pub const INITIAL_CLAMP_DIST: f32 = 112.5;
// ACE: ObjectMaint.InitialClamp_DistSq
pub const INITIAL_CLAMP_DIST_SQ: f32 = INITIAL_CLAMP_DIST * INITIAL_CLAMP_DIST;

/// One body's visibility tables. ACE's constructors (`new ObjectMaint(PhysicsObj)`) are `Default`:
/// the owner is the body whose server-side record holds the instance.
// ACE: ObjectMaint.ObjectMaint
#[derive(Debug, Default)]
pub struct ObjectMaint {
    /// Objects known to a player: added when they enter PVS / VisibleCell range, removed when they
    /// have stayed out of it for `DestructionTime`. Only maintained for players.
    /// Not ACE's (retail, V287; the retail captures): added when they enter the
    /// player's create set (`in_create_set`), removed after `DestructionTime` out of it.
    // ACE: ObjectMaint.KnownObjects
    pub(crate) known_objects: DotNetDict<u32, PhysHandle>,
    /// Objects currently within PVS / VisibleCell range. Only maintained for players.
    /// Not ACE's (retail, V287): this is ACE's landblock view only (monster wake-ups and targets,
    /// cleave, fellowship sharing read it); what the player is sent is the create set.
    // ACE: ObjectMaint.VisibleObjects
    pub(crate) visible_objects: DotNetDict<u32, PhysHandle>,
    /// Objects that were visible and have been outside the PVS for less than `DestructionTime`,
    /// with the time they expire. Keyed by the body, as ACE keys it by the `PhysicsObj` reference.
    // ACE: ObjectMaint.DestructionQueue
    pub(crate) destruction_queue: DotNetDict<PhysHandle, f64>,
    /// Players that currently know about this object; maintained for all server-spawned objects
    /// and used for broadcasting.
    // ACE: ObjectMaint.KnownPlayers
    pub(crate) known_players: DotNetDict<u32, PhysHandle>,
    /// For monster and CombatPet FindNextTarget: players and combat pets for monsters, monsters for
    /// combat pets.
    // ACE: ObjectMaint.VisibleTargets
    pub(crate) visible_targets: DotNetDict<u32, PhysHandle>,
    /// Targets a monster retaliates against that it would not normally target.
    // ACE: ObjectMaint.RetaliateTargets
    pub(crate) retaliate_targets: DotNetDict<u32, PhysHandle>,
}

/// `ObjectMaint.VisibleObjectType`.
// ACE: ObjectMaint.VisibleObjectType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleObjectType {
    All,
    Players,
    AttackTargets,
}

fn om(w: &World, this: PhysHandle) -> Option<&ObjectMaint> {
    phys_ext::ext(w, this).map(|e| &e.obj_maint)
}

fn om_mut(w: &mut World, this: PhysHandle) -> Option<&mut ObjectMaint> {
    phys_ext::ext_mut(w, this).map(|e| &mut e.obj_maint)
}

/// `obj.ID`; 0 for a body that is gone (no live body is ever id 0 once `set_object_guid` ran).
fn oid(w: &World, obj: PhysHandle) -> u32 {
    phys_ext::id(w, obj).unwrap_or(0)
}

/// `PhysicsObj.Position.Distance2DSquared(obj.Position)`.
fn dist_sq(w: &World, a: PhysHandle, b: PhysHandle) -> f32 {
    match (phys_ext::position(w, a), phys_ext::position(w, b)) {
        (Some(pa), Some(pb)) => distance_2d_squared(&pa, &pb),
        _ => f32::INFINITY,
    }
}

fn values(d: &DotNetDict<u32, PhysHandle>) -> Vec<PhysHandle> {
    d.values().copied().collect()
}

/// The world objects of `objs` that are creatures (`.Select(v => v.WeenieObj.WorldObject).OfType<Creature>()`).
fn of_type(w: &World, objs: Vec<PhysHandle>, player: bool) -> Vec<ObjectGuid> {
    objs.into_iter()
        .filter_map(|o| phys_ext::weenie_obj(w, o).world_object(w))
        .filter(|g| {
            w.objects.get(*g).is_some_and(|o| {
                if player {
                    o.is_player()
                } else {
                    o.is_creature()
                }
            })
        })
        .collect()
}

// ---- KnownObjects ------------------------------------------------------------------------

// ACE: ObjectMaint.GetKnownObject
#[must_use]
pub fn get_known_object(w: &World, this: PhysHandle, object_guid: u32) -> Option<PhysHandle> {
    om(w, this)?.known_objects.get(&object_guid).copied()
}

// ACE: ObjectMaint.GetKnownObjectsCount
#[must_use]
pub fn get_known_objects_count(w: &World, this: PhysHandle) -> usize {
    om(w, this).map_or(0, |m| m.known_objects.len())
}

// ACE: ObjectMaint.KnownObjectsContainsKey
#[must_use]
pub fn known_objects_contains_key(w: &World, this: PhysHandle, guid: u32) -> bool {
    om(w, this).is_some_and(|m| m.known_objects.contains_key(&guid))
}

// ACE: ObjectMaint.KnownObjectsContainsValue
#[must_use]
pub fn known_objects_contains_value(w: &World, this: PhysHandle, value: PhysHandle) -> bool {
    om(w, this).is_some_and(|m| m.known_objects.values().any(|v| *v == value))
}

// ACE: ObjectMaint.GetKnownObjectsWhere
pub fn get_known_objects_where(
    w: &World,
    this: PhysHandle,
    mut predicate: impl FnMut(u32, PhysHandle) -> bool,
) -> Vec<(u32, PhysHandle)> {
    om(w, this).map_or_else(Vec::new, |m| {
        m.known_objects
            .iter()
            .map(|(k, v)| (*k, *v))
            .filter(|(k, v)| predicate(*k, *v))
            .collect()
    })
}

// ACE: ObjectMaint.GetKnownObjectsValues
#[must_use]
pub fn get_known_objects_values(w: &World, this: PhysHandle) -> Vec<PhysHandle> {
    om(w, this).map_or_else(Vec::new, |m| values(&m.known_objects))
}

// ACE: ObjectMaint.GetKnownObjectsValuesWhere
pub fn get_known_objects_values_where(
    w: &World,
    this: PhysHandle,
    mut predicate: impl FnMut(PhysHandle) -> bool,
) -> Vec<PhysHandle> {
    get_known_objects_values(w, this)
        .into_iter()
        .filter(|v| predicate(*v))
        .collect()
}

/// Adds an object to the list of known objects (only maintained for players), and keeps
/// `KnownPlayers` for both parties. Returns true if the object was previously unknown.
// ACE: ObjectMaint.AddKnownObject
pub fn add_known_object(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let id = oid(w, obj);
    let Some(m) = om_mut(w, this) else {
        return false;
    };
    if m.known_objects.contains_key(&id) {
        return false;
    }

    m.known_objects.try_add(id, obj);

    // maintain KnownPlayers for both parties
    if phys_ext::is_player(w, obj) {
        add_known_player(w, this, obj);
    }

    add_known_player(w, obj, this);

    true
}

/// Adds a list of objects to the known objects list; returns the ones that were previously unknown.
// ACE: ObjectMaint.AddKnownObjects
fn add_known_objects(w: &mut World, this: PhysHandle, objs: &[PhysHandle]) -> Vec<PhysHandle> {
    let mut new_objs = Vec::new();

    for &obj in objs {
        if add_known_object(w, this, obj) {
            new_objs.push(obj);
        }
    }

    new_objs
}

// ACE: ObjectMaint.RemoveKnownObject
pub fn remove_known_object(
    w: &mut World,
    this: PhysHandle,
    obj: PhysHandle,
    inverse_player: bool,
) -> bool {
    let id = oid(w, obj);
    let result = om_mut(w, this).is_some_and(|m| m.known_objects.remove(&id).is_some());

    if inverse_player && phys_ext::is_player(w, this) {
        remove_known_player(w, obj, this);
    }

    result
}

// ---- VisibleObjects ----------------------------------------------------------------------

// ACE: ObjectMaint.GetVisibleObjectsCount
#[must_use]
pub fn get_visible_objects_count(w: &World, this: PhysHandle) -> usize {
    om(w, this).map_or(0, |m| m.visible_objects.len())
}

// ACE: ObjectMaint.VisibleObjectsContainsKey
#[must_use]
pub fn visible_objects_contains_key(w: &World, this: PhysHandle, key: u32) -> bool {
    om(w, this).is_some_and(|m| m.visible_objects.contains_key(&key))
}

// ACE: ObjectMaint.GetVisibleObjectsWhere
pub fn get_visible_objects_where(
    w: &World,
    this: PhysHandle,
    mut predicate: impl FnMut(u32, PhysHandle) -> bool,
) -> Vec<(u32, PhysHandle)> {
    om(w, this).map_or_else(Vec::new, |m| {
        m.visible_objects
            .iter()
            .map(|(k, v)| (*k, *v))
            .filter(|(k, v)| predicate(*k, *v))
            .collect()
    })
}

// ACE: ObjectMaint.GetVisibleObjectsValues
#[must_use]
pub fn get_visible_objects_values(w: &World, this: PhysHandle) -> Vec<PhysHandle> {
    om(w, this).map_or_else(Vec::new, |m| values(&m.visible_objects))
}

// ACE: ObjectMaint.GetVisibleObjectsValuesWhere
pub fn get_visible_objects_values_where(
    w: &World,
    this: PhysHandle,
    mut predicate: impl FnMut(PhysHandle) -> bool,
) -> Vec<PhysHandle> {
    get_visible_objects_values(w, this)
        .into_iter()
        .filter(|v| predicate(*v))
        .collect()
}

// ACE: ObjectMaint.GetVisibleObjectsValuesOfTypeCreature
#[must_use]
pub fn get_visible_objects_values_of_type_creature(w: &World, this: PhysHandle) -> Vec<ObjectGuid> {
    of_type(w, get_visible_objects_values(w, this), false)
}

/// Returns a list of objects that are currently visible from a cell: for an interior cell, its
/// PVS; otherwise the current landblock and its adjacents, minus interiors not seen outside.
// ACE: ObjectMaint.GetVisibleObjects
#[must_use]
pub fn get_visible_objects(
    w: &World,
    this: PhysHandle,
    cell: Option<CellId>,
    ty: VisibleObjectType,
) -> Vec<PhysHandle> {
    let cur_landblock = phys_ext::ext(w, this).and_then(|e| e.cur_landblock);
    let (Some(cur_landblock), Some(cell)) = (cur_landblock, cell) else {
        return Vec::new();
    };

    // use PVS / VisibleCells for EnvCells not seen outside
    // (mostly dungeons, also some large indoor areas ie. caves)
    if phys_ext::is_env_cell(cell) {
        return get_visible_objects_env(w, this, cell, ty);
    }

    // use current landblock + adjacents for outdoors,
    // and envcells seen from outside (all buildings)
    let visible_objs = phys_ext::get_server_objects(w, cur_landblock, true);

    let this_id = oid(w, this);
    apply_filter(w, this, visible_objs, ty)
        .into_iter()
        .filter(|i| oid(w, *i) != this_id && outdoors_or_seen_outside(w, *i))
        .collect()
}

/// `!(i.CurCell is EnvCell indoors) || indoors.SeenOutside`.
fn outdoors_or_seen_outside(w: &World, i: PhysHandle) -> bool {
    match phys_ext::cur_cell(w, i) {
        Some(c) if phys_ext::is_env_cell(c) => phys_ext::seen_outside(w, c),
        _ => true,
    }
}

/// Returns a list of objects that are currently visible from a dungeon cell.
// ACE: ObjectMaint.GetVisibleObjects
fn get_visible_objects_env(
    w: &World,
    this: PhysHandle,
    cell: CellId,
    ty: VisibleObjectType,
) -> Vec<PhysHandle> {
    let mut visible_objs = Vec::new();

    // add objects from current cell
    phys_ext::add_object_list_to(w, cell, &mut visible_objs);

    // add objects from visible cells
    for env_cell in phys_ext::visible_cells(w, cell).into_iter().flatten() {
        phys_ext::add_object_list_to(w, env_cell, &mut visible_objs);
    }

    // if SeenOutside, add objects from outdoor landblock
    if phys_ext::seen_outside(w, cell) {
        if let Some(lb) = phys_ext::ext(w, this).and_then(|e| e.cur_landblock) {
            let outside_objs = phys_ext::get_server_objects(w, lb, true)
                .into_iter()
                .filter(|i| outdoors_or_seen_outside(w, *i));

            visible_objs.extend(outside_objs);
        }
    }

    let this_id = oid(w, this);
    let mut result: Vec<PhysHandle> = Vec::new();
    for i in apply_filter(w, this, visible_objs, ty) {
        let dat_object = phys_ext::ext(w, i).is_some_and(|e| e.dat_object);
        if !dat_object && oid(w, i) != this_id && !result.contains(&i) {
            result.push(i);
        }
    }
    result
}

// ACE: ObjectMaint.ApplyFilter
fn apply_filter(
    w: &World,
    this: PhysHandle,
    objs: Vec<PhysHandle>,
    ty: VisibleObjectType,
) -> Vec<PhysHandle> {
    let me = phys_ext::weenie_obj(w, this);
    match ty {
        VisibleObjectType::All => objs,
        VisibleObjectType::Players => objs
            .into_iter()
            .filter(|i| phys_ext::is_player(w, *i))
            .collect(),
        VisibleObjectType::AttackTargets => {
            if me.is_combat_pet {
                // combat pets cannot attack pk-only creatures (ie. faction banners)
                objs.into_iter()
                    .filter(|i| {
                        let wo = phys_ext::weenie_obj(w, *i);
                        wo.is_monster && wo.player_killer_status != PlayerKillerStatus::PK
                    })
                    .collect()
            } else if me.is_faction_mob {
                objs.into_iter()
                    .filter(|i| {
                        let wo = phys_ext::weenie_obj(w, *i);
                        phys_ext::is_player(w, *i)
                            || wo.is_combat_pet
                            || wo.is_monster && !wo.same_faction(&me)
                    })
                    .collect()
            } else {
                // adding faction mobs here, even though they are retaliate-only, for inverse visible targets
                objs.into_iter()
                    .filter(|i| {
                        let wo = phys_ext::weenie_obj(w, *i);
                        phys_ext::is_player(w, *i)
                            || wo.is_combat_pet && me.player_killer_status != PlayerKillerStatus::PK
                            || wo.is_faction_mob
                            || wo.potential_foe(w, &me)
                    })
                    .collect()
            }
        }
    }
}

// ---- The create set (retail, V287) -------------------------------------------------------

/// Not ACE's (retail, V287; the retail captures): the round outdoor window, as
/// the squared radius in outdoor cells. An outdoor object is in a player's create set when the
/// global outdoor cells of the two differ by (dx, dy) with dx² + dy² ≤ 13: 45 cells, the (±3, ±2)
/// cells in (created at 104-112 m), the (±3, ±3) corners and (0, ±4) out (refused at 72 m).
pub const CREATE_WINDOW_CELLS_SQ: i32 = 13;

/// A position's global outdoor cell coordinates (landblock × 8 + floor(origin / 24) on each axis),
/// continuous across landblock edges. An interior cell's origin is landblock-relative too, so this
/// is the outdoor cell over a body inside a building.
fn outside_lcoord(p: &PPosition) -> Option<(i32, i32)> {
    dereth_physics::landdefs::get_outside_lcoord(p.cell, p.frame.origin)
}

/// The landblock (x, y) of a cell id.
fn block_xy(cell: CellId) -> (i32, i32) {
    let [x, y] = cell.landblock().0.to_be_bytes();
    (i32::from(x), i32::from(y))
}

/// Not ACE's (retail, V287; the retail captures): whether `obj` is in player
/// `this`'s create set, the objects the retail server created for a player and kept it informed
/// of. ACE sent every object of the 3×3 landblocks (a building's inside when seen from outside,
/// a dungeon's by its visible cells), held back on first sight beyond 112.5 m; the retail server
/// kept a smaller, cell-shaped set with no distance cap ([`create_set_contains`]). Players only:
/// the monsters' views are ACE's (`get_visible_objects`).
#[must_use]
pub fn in_create_set(w: &World, this: PhysHandle, obj: PhysHandle) -> bool {
    let (Some(cell), Some(obj_cell)) = (phys_ext::cur_cell(w, this), phys_ext::cur_cell(w, obj))
    else {
        return false;
    };
    if phys_ext::ext(w, obj).is_none_or(|e| e.dat_object) {
        return false;
    }
    let (Some(at), Some(there)) = (phys_ext::position(w, this), phys_ext::position(w, obj)) else {
        return false;
    };
    create_set_contains(
        w.physics.land(),
        &PPosition { cell, ..at },
        &PPosition {
            cell: obj_cell,
            ..there
        },
    )
}

/// Not ACE's (retail, V287; the retail captures): whether an observer at `at` has
/// an object at `obj` in its create set, over the cells of `land`:
///
/// * the observer's own cell always, and from an interior cell every cell on its visible-cell list;
/// * from an interior cell not seen from outside (a dungeon, a cellar) nothing else, at any
///   distance;
/// * otherwise (outdoors, or in a building room seen from outside): an outdoor object within the
///   round window ([`CREATE_WINDOW_CELLS_SQ`]) around the observer's outdoor cell; an object in a
///   building room seen from outside when that room is in the observer's landblock, one of its
///   four edge neighbours, or a diagonal neighbour the observer's cell faces (on at least one axis
///   the observer's cell index 0-7 in its landblock is 4 or more toward a + neighbour, 4 or less
///   toward a − neighbour); never a landblock two away. A room not seen from outside (a cellar)
///   is seen only through the visible-cell list, so never from outdoors.
#[must_use]
pub fn create_set_contains(land: &dyn LandSource, at: &PPosition, obj: &PPosition) -> bool {
    let (cell, obj_cell) = (at.cell, obj.cell);
    if cell == obj_cell {
        return true;
    }
    let seen_outside = |c: CellId| land.env_cell(c).is_some_and(|e| e.seen_outside);

    if phys_ext::is_env_cell(cell) {
        let on_list = cell.0 >> 16 == obj_cell.0 >> 16
            && land.env_cell(cell).is_some_and(|e| {
                e.stab_list
                    .iter()
                    .any(|v| v.0 & 0xFFFF == obj_cell.0 & 0xFFFF)
            });
        if on_list {
            return true;
        }
        if !seen_outside(cell) {
            // a dungeon or cellar: its own cell and visible cells only
            return false;
        }
    }

    let Some((lx, ly)) = outside_lcoord(at) else {
        return false;
    };
    if phys_ext::is_env_cell(obj_cell) {
        if !seen_outside(obj_cell) {
            return false;
        }
        let ((bx, by), (ox, oy)) = (block_xy(cell), block_xy(obj_cell));
        let (sx, sy) = (ox - bx, oy - by);
        if sx.abs() > 1 || sy.abs() > 1 {
            return false;
        }
        if sx == 0 || sy == 0 {
            return true;
        }
        // a diagonal: the observer's cell must face it on at least one axis
        let (cx, cy) = ((lx - bx * 8).clamp(0, 7), (ly - by * 8).clamp(0, 7));
        let faces = |s: i32, c: i32| if s > 0 { c >= 4 } else { c <= 4 };
        faces(sx, cx) || faces(sy, cy)
    } else {
        let Some((ox, oy)) = outside_lcoord(obj) else {
            return false;
        };
        let (dx, dy) = (ox - lx, oy - ly);
        dx * dx + dy * dy <= CREATE_WINDOW_CELLS_SQ
    }
}

/// Not ACE's (retail, V287; the retail captures): player `this`'s create set
/// ([`in_create_set`]) among the server objects of its landblock and the adjacent ones, in their
/// order.
#[must_use]
pub fn get_create_set(w: &World, this: PhysHandle) -> Vec<PhysHandle> {
    let Some(lb) = phys_ext::cur_landblock(w, this) else {
        return Vec::new();
    };
    let mut result: Vec<PhysHandle> = Vec::new();
    for i in phys_ext::get_server_objects(w, lb, true) {
        if i != this && !result.contains(&i) && in_create_set(w, this, i) {
            result.push(i);
        }
    }
    result
}

/// Not ACE's (retail, V287; the retail captures): `obj` is in player `this`'s
/// create set. A return after the forget deadline is forgotten first (V278), a pending forget
/// is cancelled, and the object becomes known. Returns true when it was unknown (the client is to
/// be sent its create).
pub fn enter_create_set(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    forget_if_expired(w, this, obj);
    cancel_object_to_be_destroyed(w, this, obj);
    add_known_object(w, this, obj)
}

/// Not ACE's (retail, V287; the retail captures): `obj` is out of player
/// `this`'s create set. A known object starts its forget clock (`DestructionTime`) unless it is
/// already running.
pub fn leave_create_set(w: &mut World, this: PhysHandle, obj: PhysHandle) {
    let id = oid(w, obj);
    if om(w, this).is_some_and(|m| m.known_objects.contains_key(&id)) {
        add_object_to_be_destroyed(w, this, obj);
    }
}

/// Not ACE's (retail, V287): takes an object out of player `this`'s visible objects by its id
/// (a body destroyed since it was added answers no id), and the player out of its visible targets.
pub fn remove_visible_entry(w: &mut World, this: PhysHandle, id: u32, obj: PhysHandle) {
    if let Some(m) = om_mut(w, this) {
        m.visible_objects.remove(&id);
    }
    remove_visible_target(w, obj, this);
}

/// Adds an object to the list of visible objects (only maintained for players). Returns true if the
/// object was previously not visible and was added. A never-known object further than
/// `InitialClamp_Dist` is not added.
// ACE: ObjectMaint.AddVisibleObject
pub fn add_visible_object(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let id = oid(w, obj);
    let Some(m) = om(w, this) else { return false };
    if m.visible_objects.contains_key(&id) {
        return false;
    }

    if INITIAL_CLAMP && !m.known_objects.contains_key(&id) {
        let dist_sq = dist_sq(w, this, obj);

        if dist_sq > INITIAL_CLAMP_DIST_SQ {
            return false;
        }
    }

    if let Some(m) = om_mut(w, this) {
        m.visible_objects.try_add(id, obj);
    }

    if phys_ext::weenie_obj(w, obj).is_monster {
        add_visible_target(w, obj, this, false, false);
    }

    true
}

/// Adds a list of visible objects, maintaining both known and visible objects; returns the
/// visible ones that were previously unknown (the objects to create on the client).
// ACE: ObjectMaint.AddVisibleObjects
pub fn add_visible_objects(
    w: &mut World,
    this: PhysHandle,
    objs: &[PhysHandle],
) -> Vec<PhysHandle> {
    let mut visible_added = Vec::new();

    for &obj in objs {
        if add_visible_object(w, this, obj) {
            visible_added.push(obj);
        }
    }

    remove_objects_to_be_destroyed(w, this, objs);

    add_known_objects(w, this, &visible_added)
}

/// Removes an object from the visible objects list (only run for players), and the player from the
/// object's visible targets.
// ACE: ObjectMaint.RemoveVisibleObject
pub fn remove_visible_object(
    w: &mut World,
    this: PhysHandle,
    obj: PhysHandle,
    inverse_target: bool,
) -> bool {
    let id = oid(w, obj);
    let removed = om_mut(w, this).is_some_and(|m| m.visible_objects.remove(&id).is_some());

    if inverse_target {
        remove_visible_target(w, obj, this);
    }

    removed
}

// ---- DestructionQueue --------------------------------------------------------------------

// ACE: ObjectMaint.GetDestructionQueueCount
#[must_use]
pub fn get_destruction_queue_count(w: &World, this: PhysHandle) -> usize {
    om(w, this).map_or(0, |m| m.destruction_queue.len())
}

// ACE: ObjectMaint.GetDestructionQueueCopy
#[must_use]
pub fn get_destruction_queue_copy(w: &World, this: PhysHandle) -> Vec<(PhysHandle, f64)> {
    om(w, this).map_or_else(Vec::new, |m| {
        m.destruction_queue.iter().map(|(k, v)| (*k, *v)).collect()
    })
}

/// Adds an object to the destruction queue when it exits the PVS range (only maintained for
/// players). Returns false if it was already queued.
///
/// Not ACE's (retail, V287; the retail captures): the queue follows the player's
/// create set ([`in_create_set`]), not the visible objects, so queueing leaves the visible objects
/// (and the monsters' targets they feed) alone; the caller that finds the object out of view
/// removes it from them.
// ACE: ObjectMaint.AddObjectToBeDestroyed
pub fn add_object_to_be_destroyed(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let now = phys_ext::physics_timer_current_time(w);
    let Some(m) = om_mut(w, this) else {
        return false;
    };
    if m.destruction_queue.contains_key(&obj) {
        return false;
    }

    m.destruction_queue
        .try_add(obj, now + f64::from(DESTRUCTION_TIME));

    true
}

/// Adds a list of objects to the destruction queue; returns the ones newly queued.
// ACE: ObjectMaint.AddObjectsToBeDestroyed
pub fn add_objects_to_be_destroyed(
    w: &mut World,
    this: PhysHandle,
    objs: &[PhysHandle],
) -> Vec<PhysHandle> {
    let mut queued = Vec::new();
    for &obj in objs {
        if add_object_to_be_destroyed(w, this, obj) {
            queued.push(obj);
        }
    }

    queued
}

/// Removes an object from the destruction queue if it has been invisible for less than `DestructionTime`.
// ACE: ObjectMaint.RemoveObjectToBeDestroyed
pub fn remove_object_to_be_destroyed(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let now = phys_ext::physics_timer_current_time(w);
    let Some(m) = om_mut(w, this) else {
        return false;
    };
    // `double time = -1; TryGetValue(obj, out time)`: a miss writes default(double), 0, so the
    // `time != -1` guard never fires and a miss answers `0 > now`.
    let time = m.destruction_queue.get(&obj).copied().unwrap_or(0.0);
    #[allow(clippy::float_cmp)]
    if time != -1.0 && time > now {
        m.destruction_queue.remove(&obj);
        return true;
    }

    false
}

/// Not ACE's (a fix, V260): an object back in view is taken off the destruction queue, so the
/// next sweep does not forget it while it stands in view. A return after the deadline is first
/// forgotten and created afresh (`forget_if_expired`, V278).
pub fn cancel_object_to_be_destroyed(w: &mut World, this: PhysHandle, obj: PhysHandle) {
    if let Some(m) = om_mut(w, this) {
        m.destruction_queue.remove(&obj);
    }
}

/// Not ACE's (retail, V278): an object coming back into a player's view after
/// its forget deadline is forgotten now, from every list, as the sweep would have done, so the
/// return creates it afresh at the moment it comes back instead of at the next sweep (retail
/// created such a return within a second). Returns whether it was forgotten.
pub fn forget_if_expired(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let now = phys_ext::physics_timer_current_time(w);
    let expired = om(w, this)
        .and_then(|m| m.destruction_queue.get(&obj).copied())
        .is_some_and(|t| t <= now);
    if expired {
        forget_object(w, this, obj);
    }

    expired
}

/// Not ACE's (retail, V287; the retail captures): a player forgets an object that
/// has been out of its create set for `DestructionTime`: the object leaves the player's known
/// objects and forget queue, and the player leaves the object's known players. The player's visible
/// objects, and the monsters' targets they feed, are left to the visibility pass (they follow the
/// wider landblock view, which the forget does not change).
pub fn forget_object(w: &mut World, this: PhysHandle, obj: PhysHandle) {
    remove_known_object(w, this, obj, true);
    if let Some(m) = om_mut(w, this) {
        m.destruction_queue.remove(&obj);
    }

    if phys_ext::is_player(w, obj) {
        remove_known_player(w, this, obj);
    }
}

/// Removes objects from the destruction queue when they re-enter visibility within `DestructionTime`.
// ACE: ObjectMaint.RemoveObjectsToBeDestroyed
fn remove_objects_to_be_destroyed(w: &mut World, this: PhysHandle, objs: &[PhysHandle]) {
    for &obj in objs {
        remove_object_to_be_destroyed(w, this, obj);
    }
}

/// Removes any objects that have been in the destruction queue for more than `DestructionTime`, from every list;
/// returns them.
// ACE: ObjectMaint.DestroyObjects
pub fn destroy_objects(w: &mut World, this: PhysHandle) -> Vec<PhysHandle> {
    let now = phys_ext::physics_timer_current_time(w);
    // find the list of objects that have been in the destruction queue > DestructionTime
    let expired_objs: Vec<PhysHandle> = om(w, this).map_or_else(Vec::new, |m| {
        m.destruction_queue
            .iter()
            .filter(|(_, v)| **v <= now)
            .map(|(k, _)| *k)
            .collect()
    });

    // remove expired objects from all lists
    // Not ACE's (retail, V287): from the knowledge lists only
    // (`forget_object`); the visible objects follow the landblock view.
    for &expired_obj in &expired_objs {
        forget_object(w, this, expired_obj);
    }

    expired_objs
}

/// Removes an object after it has expired from the destruction queue, or it has been destroyed.
// ACE: ObjectMaint.RemoveObject
pub fn remove_object(w: &mut World, this: PhysHandle, obj: PhysHandle, inverse: bool) {
    remove_known_object(w, this, obj, inverse);
    remove_visible_object(w, this, obj, inverse);
    if let Some(m) = om_mut(w, this) {
        m.destruction_queue.remove(&obj);
    }

    if phys_ext::is_player(w, obj) {
        remove_known_player(w, this, obj);
    }

    remove_visible_target(w, this, obj);
    remove_retaliate_target(w, this, obj);
}

// ---- KnownPlayers ------------------------------------------------------------------------

// ACE: ObjectMaint.GetKnownPlayersCount
#[must_use]
pub fn get_known_players_count(w: &World, this: PhysHandle) -> usize {
    om(w, this).map_or(0, |m| m.known_players.len())
}

// ACE: ObjectMaint.GetKnownPlayersWhere
pub fn get_known_players_where(
    w: &World,
    this: PhysHandle,
    mut predicate: impl FnMut(u32, PhysHandle) -> bool,
) -> Vec<(u32, PhysHandle)> {
    om(w, this).map_or_else(Vec::new, |m| {
        m.known_players
            .iter()
            .map(|(k, v)| (*k, *v))
            .filter(|(k, v)| predicate(*k, *v))
            .collect()
    })
}

// ACE: ObjectMaint.GetKnownPlayersValues
#[must_use]
pub fn get_known_players_values(w: &World, this: PhysHandle) -> Vec<PhysHandle> {
    om(w, this).map_or_else(Vec::new, |m| values(&m.known_players))
}

// ACE: ObjectMaint.GetKnownPlayersValuesAsPlayer
#[must_use]
pub fn get_known_players_values_as_player(w: &World, this: PhysHandle) -> Vec<ObjectGuid> {
    of_type(w, get_known_players_values(w, this), true)
}

// ACE: ObjectMaint.GetKnownObjectsValuesAsCreature
#[must_use]
pub fn get_known_objects_values_as_creature(w: &World, this: PhysHandle) -> Vec<ObjectGuid> {
    of_type(w, get_known_objects_values(w, this), false)
}

/// Adds a player who currently knows about this object (maintained for all server-spawned
/// objects: a broadcast goes to every such player). Returns true if newly added.
// ACE: ObjectMaint.AddKnownPlayer
fn add_known_player(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    // only tracking players who know about this object
    if !phys_ext::is_player(w, obj) {
        log::debug!(
            "{:08X}.ObjectMaint.AddKnownPlayer({:08X}): tried to add a non-player",
            oid(w, this),
            oid(w, obj)
        );
        return false;
    }
    if phys_ext::ext(w, this).is_some_and(|e| e.dat_object) {
        log::debug!(
            "{:08X}.ObjectMaint.AddKnownPlayer({:08X}): tried to add player for dat object",
            oid(w, this),
            oid(w, obj)
        );
        return false;
    }

    let id = oid(w, obj);
    let Some(m) = om_mut(w, this) else {
        return false;
    };
    // TryAdd for existing keys still modifies collection?
    if m.known_players.contains_key(&id) {
        return false;
    }

    m.known_players.try_add(id, obj);
    true
}

/// Adds a list of players known to this object; returns the ones newly added.
// ACE: ObjectMaint.AddKnownPlayers
pub fn add_known_players(w: &mut World, this: PhysHandle, objs: &[PhysHandle]) -> Vec<PhysHandle> {
    let mut new_objs = Vec::new();

    for &obj in objs {
        if add_known_player(w, this, obj) {
            new_objs.push(obj);
        }
    }

    new_objs
}

/// Removes a known player for this object.
// ACE: ObjectMaint.RemoveKnownPlayer
pub fn remove_known_player(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let id = oid(w, obj);
    om_mut(w, this).is_some_and(|m| m.known_players.remove(&id).is_some())
}

// ---- VisibleTargets / RetaliateTargets ---------------------------------------------------

// ACE: ObjectMaint.GetVisibleTargetsCount
#[must_use]
pub fn get_visible_targets_count(w: &World, this: PhysHandle) -> usize {
    om(w, this).map_or(0, |m| m.visible_targets.len())
}

// ACE: ObjectMaint.GetRetaliateTargetsCount
#[must_use]
pub fn get_retaliate_targets_count(w: &World, this: PhysHandle) -> usize {
    om(w, this).map_or(0, |m| m.retaliate_targets.len())
}

// ACE: ObjectMaint.VisibleTargetsContainsKey
#[must_use]
pub fn visible_targets_contains_key(w: &World, this: PhysHandle, key: u32) -> bool {
    om(w, this).is_some_and(|m| m.visible_targets.contains_key(&key))
}

// ACE: ObjectMaint.RetaliateTargetsContainsKey
#[must_use]
pub fn retaliate_targets_contains_key(w: &World, this: PhysHandle, key: u32) -> bool {
    om(w, this).is_some_and(|m| m.retaliate_targets.contains_key(&key))
}

// ACE: ObjectMaint.GetVisibleTargetsValues
#[must_use]
pub fn get_visible_targets_values(w: &World, this: PhysHandle) -> Vec<PhysHandle> {
    om(w, this).map_or_else(Vec::new, |m| values(&m.visible_targets))
}

// ACE: ObjectMaint.GetRetaliateTargetsValues
#[must_use]
pub fn get_retaliate_targets_values(w: &World, this: PhysHandle) -> Vec<PhysHandle> {
    om(w, this).map_or_else(Vec::new, |m| values(&m.retaliate_targets))
}

// ACE: ObjectMaint.GetVisibleTargetsValuesOfTypeCreature
#[must_use]
pub fn get_visible_targets_values_of_type_creature(w: &World, this: PhysHandle) -> Vec<ObjectGuid> {
    of_type(w, get_visible_targets_values(w, this), false)
}

/// `WeenieObj.WorldObject?.CreatureType`.
fn creature_type(w: &World, wo: &WeenieObject) -> Option<empyrean_entity::enums::CreatureType> {
    wo.world_object(w)
        .and_then(|g| w.objects.get(g))
        .and_then(|o| o.creature_type())
}

/// For monster and CombatPet FindNextTarget: monsters track players and combat pets, combat pets
/// track monsters, faction mobs track players, combat pets and monsters of another faction. The
/// inverse is kept for non-players.
// ACE: ObjectMaint.AddVisibleTarget
// Not ACE's (a fix, V308): the 112.5 m initial clamp holds back only a player
// that does not already know this object; a player it was created for (the create set reaches
// past 112.5 m) is added at any distance. ACE's test asked whether the player knew itself, which
// never holds, so every player target was clamped.
fn add_visible_target(
    w: &mut World,
    this: PhysHandle,
    obj: PhysHandle,
    clamp: bool,
    _foe_type: bool,
) -> bool {
    let me = phys_ext::weenie_obj(w, this);
    let other = phys_ext::weenie_obj(w, obj);
    let obj_is_player = phys_ext::is_player(w, obj);
    let (this_id, obj_id) = (oid(w, this), oid(w, obj));

    if me.is_combat_pet {
        // only tracking monsters
        if !other.is_monster {
            log::debug!("{this_id:08X}.ObjectMaint.AddVisibleTarget({obj_id:08X}): tried to add a non-monster");
            return false;
        }
    } else if me.is_faction_mob {
        // only tracking players, combat pets, and monsters of differing faction
        if !obj_is_player && !other.is_combat_pet && (!other.is_monster || me.same_faction(&other))
        {
            log::debug!(
                "{this_id:08X}.ObjectMaint.AddVisibleTarget({obj_id:08X}): tried to add a non-player / non-combat pet / non-opposing faction mob"
            );
            return false;
        }
    } else {
        // handle special case:
        // we want to select faction mobs for monsters inverse targets,
        // but not add to the original monster
        if other.is_faction_mob {
            add_visible_target(w, obj, this, true, false);
            return false;
        }

        // handle special case:
        // if obj has a FoeType of this creature, and this creature doesn't have a FoeType for obj,
        // we only want to perform the inverse
        let other_ct = creature_type(w, &other);
        if other.foe_type.is_some()
            && other.foe_type == creature_type(w, &me)
            && (me.foe_type.is_none() || other.world_object(w).is_some() && me.foe_type != other_ct)
        {
            add_visible_target(w, obj, this, true, false);
            return false;
        }

        // only tracking players and combat pets
        if !obj_is_player && !other.is_combat_pet && me.foe_type.is_none() {
            log::debug!("{this_id:08X}.ObjectMaint.AddVisibleTarget({obj_id:08X}): tried to add a non-player / non-combat pet");
            return false;
        }
    }
    if phys_ext::ext(w, this).is_some_and(|e| e.dat_object) {
        log::debug!("{this_id:08X}.ObjectMaint.AddVisibleTarget({obj_id:08X}): tried to add player for dat object");
        return false;
    }

    if clamp && INITIAL_CLAMP && obj_is_player && !known_objects_contains_key(w, obj, this_id) {
        let dist_sq = dist_sq(w, this, obj);

        if dist_sq > INITIAL_CLAMP_DIST_SQ {
            return false;
        }
    }

    let Some(m) = om_mut(w, this) else {
        return false;
    };
    // TryAdd for existing keys still modifies collection?
    if m.visible_targets.contains_key(&obj_id) {
        return false;
    }

    m.visible_targets.add(obj_id, obj);

    // maintain inverse for monsters / combat pets
    if !obj_is_player {
        add_visible_target(w, obj, this, true, false);
    }

    true
}

/// Adds visible targets; returns the players among the added ones.
///
/// Not ACE's (retail, V287; the retail captures): ACE also made those players
/// known players of this object, the path by which a player came to be sent a monster that saw it.
/// A player now knows an object only when the object was created for it (the player's create set,
/// evaluated for every player in the object's reach as the object moves), so the targets are left
/// out of the known players; the targets themselves are ACE's.
// ACE: ObjectMaint.AddVisibleTargets
pub fn add_visible_targets(
    w: &mut World,
    this: PhysHandle,
    objs: &[PhysHandle],
) -> Vec<PhysHandle> {
    let mut visible_added = Vec::new();

    for &obj in objs {
        if add_visible_target(w, this, obj, true, false) {
            visible_added.push(obj);
        }
    }

    visible_added
        .into_iter()
        .filter(|o| phys_ext::is_player(w, *o))
        .collect()
}

// ACE: ObjectMaint.RemoveVisibleTarget
fn remove_visible_target(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let id = oid(w, obj);
    om_mut(w, this).is_some_and(|m| m.visible_targets.remove(&id).is_some())
}

/// The visible objects from `cell` within `InitialClamp_Dist` in 2D.
// ACE: ObjectMaint.GetVisibleObjectsDist
#[must_use]
pub fn get_visible_objects_dist(
    w: &World,
    this: PhysHandle,
    cell: Option<CellId>,
    ty: VisibleObjectType,
) -> Vec<PhysHandle> {
    let visible_objs = get_visible_objects(w, this, cell, ty);

    let mut dist = Vec::new();
    for obj in visible_objs {
        let dist_sq = dist_sq(w, this, obj);

        if dist_sq <= INITIAL_CLAMP_DIST_SQ {
            dist.push(obj);
        }
    }

    dist
}

/// Adds a retaliate target for a monster that it would not normally attack; it is added to the
/// visible targets too.
// ACE: ObjectMaint.AddRetaliateTarget
pub fn add_retaliate_target(w: &mut World, this: PhysHandle, obj: PhysHandle) {
    let id = oid(w, obj);
    let Some(m) = om_mut(w, this) else { return };
    if m.retaliate_targets.contains_key(&id) {
        return;
    }
    m.retaliate_targets.add(id, obj);

    // we're going to add retaliate targets to the list of visible targets as well,
    // so that we don't have to traverse both VisibleTargets and RetaliateTargets
    // in all of the logic based on VisibleTargets
    if m.visible_targets.contains_key(&id) {
        return;
    }
    m.visible_targets.add(id, obj);
}

/// Called when a monster goes back to sleep.
// ACE: ObjectMaint.ClearRetaliateTargets
pub fn clear_retaliate_targets(w: &mut World, this: PhysHandle) {
    let Some(m) = om_mut(w, this) else { return };
    // remove retaliate targets from visible targets
    let keys: Vec<u32> = m.retaliate_targets.keys().copied().collect();
    for k in keys {
        m.visible_targets.remove(&k);
    }

    m.retaliate_targets.clear();
}

// ACE: ObjectMaint.RemoveRetaliateTarget
fn remove_retaliate_target(w: &mut World, this: PhysHandle, obj: PhysHandle) -> bool {
    let id = oid(w, obj);
    om_mut(w, this).is_some_and(|m| m.retaliate_targets.remove(&id).is_some())
}

/// Clears all of the ObjMaint tables for an object.
// ACE: ObjectMaint.RemoveAllObjects
fn remove_all_objects(w: &mut World, this: PhysHandle) {
    let Some(m) = om_mut(w, this) else { return };
    m.known_objects.clear();
    m.visible_objects.clear();
    m.destruction_queue.clear();
    m.known_players.clear();
    m.visible_targets.clear();
    m.retaliate_targets.clear();
}

/// The destructor: removes this object from every other object's tables, clears its own, and
/// drops it from the server object table.
// ACE: ObjectMaint.DestroyObject
pub fn destroy_object(w: &mut World, this: PhysHandle) {
    for obj in get_known_objects_values(w, this) {
        remove_object(w, obj, this, true);
    }

    // we are maintaining the inverses here,
    // so passing false to iterate with modifying these collections
    for obj in get_known_players_values(w, this) {
        remove_object(w, obj, this, false);
    }

    for obj in get_visible_targets_values(w, this) {
        remove_object(w, obj, this, false);
    }

    for obj in get_retaliate_targets_values(w, this) {
        remove_object(w, obj, this, false);
    }

    remove_all_objects(w, this);

    server_object_manager::remove_server_object(w, this);
}
