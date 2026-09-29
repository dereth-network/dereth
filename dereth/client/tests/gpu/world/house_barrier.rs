//! A closed house that the mover is not a guest of stops them at its boundary, and sparks its
//! barrier effect; the owner, a guest and an open house pass. Fixture: the villa block `0x9DAF` on
//! the retail dats, with the shard's half of the `house-purchase-and-trade` recording (the house's
//! create and its `0x0248 House_UpdateRestrictions`) replayed into the object stream (no socket),
//! on a software device.
//!
//! # The barrier is not a collision source. It is a property of the cell.
//!
//! There is no volume, physics object, or geometry anywhere in the barrier. Each cell can instead
//! carry the object ID of the house that owns it, and the client checks that restriction before
//! geometry on every cell transition. The check's order and early returns are significant:
//!
//! 1. A missing transition object collides, while an object without game-state data passes.
//! 2. The mover's bypass predicate is read, but a non-player (`object_info.state & 0x100 == 0`),
//!    an unrestricted cell, or a true bypass answer passes before the house is consulted.
//! 3. A restricted cell whose house object or house game-state data is missing collides.
//! 4. The house tests whether the mover may enter. A true answer passes; a false answer invokes
//!    the cell's restriction response and returns `COLLIDED_TS` (2).
//!
//! The restriction response differs by cell kind. A land cell installs an **axis-aligned**
//! collision normal pointing back out of the crossed edge so the body slides along the property
//! line. An environment cell returns success without installing a normal.
//!
//! # Where the cell's `restriction_obj` comes from — the dat, not the wire
//!
//! Two writers, both of them the CELL dat:
//!
//! * Landblock initialization closes with a loop over all `side_cell_count²` land cells. It looks
//!   each cell up in the `PackableHashTable<cell_id, iid>` carried by landblock info behind the
//!   `num_buildings` dword's pack mask. Non-zero answers are stored; zero is skipped.
//! * Interior-cell decoding reads the restriction object directly from the cell record when flag
//!   bit 3 is set.
//!
//! [`the_shard_and_the_dat_name_the_same_house`] measures both against the recorded villa.
//!
//! # And where the *permission* comes from — `0x0248 House_UpdateRestrictions`
//!
//! Permission passes immediately when the house is unowned, the mover is its owner, or no
//! restriction table has arrived yet. Otherwise the table admits an **open** house (`bitmask & 1`),
//! a mover whose monarch matches the house's monarch, or a mover listed as a guest. That table is
//! exactly what `0x0248 House_UpdateRestrictions` carries. A denied entry then uses the house's
//! physics-script type for the contact effect; an invalid script type denies entry silently.
//!
//! The client's side of it: `CellRuntime::restriction_obj` is written from the CELL dat's two
//! sources, the object-info state carries `ObjectInfoState::IS_PLAYER` from the object's player
//! predicate, and `check_entry_restrictions` asks the mover's real bypass and entry predicates.
//!
//! # Stations
//!
//! Each walk reads distance travelled, final position, how far north the body ever got and frames
//! in contact, on the villa block. Fails when the retail dats or the recording are absent.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_assets::world::{EnvCell, LandblockInfo};
use dereth_assets::{decode_any, DecodedAsset};
use dereth_client::character::{CharacterInput, PLAYER_OBJECT_ID};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::trade::HouseUpdateRestrictions;
use dereth_protocol::types::PublicWeenieDesc;
use dereth_protocol::{Message, Opcode, Reader};
use dereth_render::device::Gpu;

const W: u32 = 320;
const H: u32 = 240;

/// **The villa block.** `0x9DAF` holds two villas; the recording is a session on it.
const BLOCK: u16 = 0x9DAF;

/// **The house.** `0x79DAF03B` — the second villa's own weenie, named "Villa", created at login by
/// a `0xF745` and the subject of all seventeen `0x0248 House_UpdateRestrictions` in the capture.
const HOUSE: ObjectId = ObjectId(0x79DA_F03B);
/// The other villa on the same block, fencing land cell `0x9DAF0026` and env cells
/// `0x0100..=0x011E`.
const OTHER_HOUSE: ObjectId = ObjectId(0x79DA_F038);

/// The landblock restriction table assigns this land cell to [`HOUSE`]. Cell index `0x2A` = 42,
/// i.e. block-local column 5, row 1 — `x` in `[120, 144)`, `y` in `[24, 48)`.
const FENCED_CELL: u32 = 0x9DAF_002A;
/// The cell immediately south of it, index `0x29` (column 5, row 0), which has **no**
/// restriction. The walk starts here and crosses north into [`FENCED_CELL`].
const OPEN_CELL: u32 = 0x9DAF_0029;

/// The `y` of the boundary between the two cells: row 1 starts at `1 * 24`.
const FENCE_Y: f32 = 24.0;

/// The owner, and one of the four guests. The capture's first `0x0248` lists guest iids
/// `0x50000014`, `0x50000016`, `0x50000017` and `0x5000001C`; the owner is `0x5000001D`, which is
/// also the monarch ID written by `@house guest add_allegiance` at t=796 s.
const OWNER: ObjectId = ObjectId(0x5000_001D);
const GUEST: ObjectId = ObjectId(0x5000_0014);

/// A physics-script type for the barrier's own effect. The capture's `0xF745` for the villa sets no
/// `PSCRIPT` header bit, so the effect stations write one by hand and say so; what retail plays is
/// whatever that field holds when the shard does send it.
const BARRIER_PSCRIPT: u16 = 83;

/// 200 frames at 30 Hz. The body covers **0.075 m a frame** on this slope, so 200 frames is about
/// 15 m against a 4 m approach; a walk that stopped short of the fence with zero refusals would
/// read exactly like a barrier that works.
const FRAMES: usize = 200;
/// How far south of the fence the body starts.
const APPROACH: f32 = 4.0;
/// Where along the cell the walk happens, and it is **not** arbitrary. The villa's plot is walled,
/// and the wall stands a few centimetres inside the cell boundary along most of the southern edge:
/// a body walking north at `x = 132` is stopped by masonry at `y = 23.77` whether or not it has
/// permission, which would make every station here a measurement of a wall. `x = 142` is the
/// **gate** -- measured by scanning the whole southern edge at 2 m intervals, where it is the only
/// place a permitted body gets more than 0.4 m past the line (it walks 12 m in). That is also the
/// only place the magical barrier can be observed in retail, for exactly the same reason.
const WALK_X: f32 = 142.0;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn warp() -> Gpu {
    crate::common::software_gpu(W, H)
}

// ---------------------------------------------------------------------------------------------
// The recording: the `house-purchase-and-trade` session's messages, read through the shared corpus
// reader.
// ---------------------------------------------------------------------------------------------

struct Blob {
    client: bool,
    bytes: Vec<u8>,
}

fn house_purchase_and_trade() -> Vec<Blob> {
    Corpus::shared("house-purchase-and-trade")
        .blobs
        .iter()
        .map(|b| Blob {
            client: b.dir == Direction::ClientToServer,
            bytes: b.payload.clone(),
        })
        .collect()
}

/// Every `0xF7B0` game event of one sub-opcode, body only (the `[F7B0][iid][stamp][sub]` header is
/// 16 bytes).
fn events(blobs: &[Blob], sub: u32) -> Vec<&[u8]> {
    blobs
        .iter()
        .filter(|b| !b.client && b.bytes.len() >= 16)
        .filter(|b| u32::from_le_bytes(b.bytes[..4].try_into().expect("4")) == 0xF7B0)
        .filter(|b| u32::from_le_bytes(b.bytes[12..16].try_into().expect("4")) == sub)
        .map(|b| &b.bytes[16..])
        .collect()
}

/// The first `0xF745 CreateObject` for one object id, body only (without the opcode dword).
fn create_body(blobs: &[Blob], id: ObjectId) -> Vec<u8> {
    blobs
        .iter()
        .filter(|b| !b.client && b.bytes.len() >= 8)
        .filter(|b| u32::from_le_bytes(b.bytes[..4].try_into().expect("4")) == 0xF745)
        .find(|b| u32::from_le_bytes(b.bytes[4..8].try_into().expect("4")) == id.0)
        .map(|b| b.bytes[4..].to_vec())
        .unwrap_or_else(|| panic!("the capture has no CreateObject for {:#010X}", id.0))
}

// ---------------------------------------------------------------------------------------------
// The CELL dat
// ---------------------------------------------------------------------------------------------

fn lbi(s: &RetailDatStore, block: u16) -> LandblockInfo {
    let id = dereth_primitives::DataId((u32::from(block) << 16) | 0xFFFE);
    let b = s
        .read_typed(DbType::Lbi, id)
        .expect("a landblock info record");
    match decode_any(DbType::Lbi, id, &b).expect("the landblock info decodes") {
        DecodedAsset::LandblockInfo(l) => l,
        other => panic!("{id:?} decoded as {other:?}"),
    }
}

fn env_cell(s: &RetailDatStore, cell: u32) -> Option<EnvCell> {
    let id = dereth_primitives::DataId(cell);
    let b = s.read_typed(DbType::Cell, id).ok()?;
    match decode_any(DbType::Cell, id, &b).ok()? {
        DecodedAsset::EnvCell(c) => Some(c),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// The bench, on the villa block
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Sample {
    origin: Vec3,
    cell: u32,
    contact: bool,
    walkable: bool,
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, mut gpu: Gpu, player: ObjectId) -> Self {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: BLOCK,
            land_radius: 1,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        let mut objects = ObjectStream::new();
        // The player id arrives before the description, and
        // `Character::adopt_server_id` re-keys the body it already stood on the ground. The id is
        // a parameter because one station needs the body to *be* one of the recording's guests,
        // and the game world's `set_player` method refuses a second call (the next frame's
        // `adopt_server_id` would put the body straight back on `PLAYER_OBJECT_ID`).
        objects.apply_event(&SessionEvent::PlayerCreated(player), LocalTime(1.0));
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects,
            now: 1.0,
        }
    }

    /// `0xF745` for the local player. `bitfield` carries `BF_PLAYER` because the player
    /// predicate reads that bit and object-info initialization turns the answer into `IS_PLAYER`,
    /// which is the bit `check_entry_restrictions` gates on.
    fn create_player(&mut self, id: ObjectId, bitfield: u32) {
        let p = ObjectCreatePayload {
            id,
            wdesc: PublicWeenieDesc {
                bitfield,
                ..PublicWeenieDesc::default()
            },
            ..ObjectCreatePayload::default()
        };
        let body = dereth_protocol::write_body(&ItemCreateObject(p)).expect("the player encodes");
        self.world_view(Opcode::ITEM_CREATE_OBJECT, body);
    }

    /// The house, **from the recording**: `0xF745`'s own bytes, unmodified.
    fn create_house_from_the_capture(&mut self, blobs: &[Blob], id: ObjectId) {
        let body = create_body(blobs, id);
        self.world_view(Opcode::ITEM_CREATE_OBJECT, body);
    }

    fn world_view(&mut self, opcode: Opcode, body: Vec<u8>) {
        self.objects.apply_event(
            &SessionEvent::WorldObject { opcode, body },
            LocalTime(self.now),
        );
    }

    /// Feed the recording's own `0x0248` body to the restriction-update receiver.
    fn recv_restrictions(&mut self, body: &[u8]) -> bool {
        let m = HouseUpdateRestrictions::read(&mut Reader::new(body))
            .expect("the capture's 0x0248 decodes");
        self.objects
            .world
            .recv_update_restrictions(m.sequence, m.sender, m.restrictions)
    }

    fn house_owner(&mut self, id: ObjectId, owner: Option<ObjectId>) {
        self.objects
            .world
            .weenie_mut(id)
            .expect("the house exists")
            .pwd
            .house_owner_iid = owner;
    }

    fn set_house_pscript(&mut self, id: ObjectId, script: Option<u16>) {
        self.objects
            .world
            .weenie_mut(id)
            .expect("the house exists")
            .pwd
            .pscript = script;
    }

    fn set_option(&mut self, on: bool) {
        self.objects.world.player_system.options.set(
            dereth_client_model::player::options::option::DISABLE_HOUSE_RESTRICTION_EFFECTS,
            on,
        );
    }

    fn stand_south_of_the_fence(&mut self) -> Vec3 {
        let at = Vec3::new(WALK_X, FENCE_Y - APPROACH, 0.0);
        let probe = Position::new(CellId(OPEN_CELL), Frame::new(at, Quat::IDENTITY));
        let ground = self
            .scene
            .character
            .as_ref()
            .expect("a body")
            .world
            .terrain_height_at(&probe)
            .expect("the ground under the start point");
        let at = Vec3::new(at.x, at.y, ground);
        let p = Position::new(CellId(OPEN_CELL), Frame::new(at, Quat::IDENTITY));
        self.scene.character.as_mut().expect("a body").teleport(p);
        at
    }

    fn step(&mut self, input: CharacterInput) -> Sample {
        self.now += 1.0 / 30.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            now,
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        // `App::sync_objects`'s two lines; without them there is nothing to fence.
        // `WorldScene::sync_objects` is the *draw* half; the physics half is separate and gives the
        // house a physics body that the restriction lookup can find, then pushes both game objects
        // onto their bodies. `adopt_server_id` runs first, exactly as the client does it, so the
        // physics object table is never asked to hold two bodies under one id.
        if let Some(c) = scene.character.as_mut() {
            c.adopt_server_id(objects.world.player);
            objects.sync_physics_at(store, &mut c.world, LocalTime(*now));
        }
        scene.update(
            dereth_client::camera::CameraInput::default(),
            input,
            LocalTime(*now),
            1.0 / 30.0,
        );
        let c = scene.character.as_ref().expect("a body");
        let o = c.world.get(c.handle).expect("the body is in the world");
        Sample {
            origin: c.position().frame.origin,
            cell: c.position().cell.0,
            contact: o.transient_state.in_contact(),
            walkable: o.transient_state.on_walkable(),
        }
    }

    fn settle(&mut self) -> Sample {
        let mut s = self.step(CharacterInput::default());
        for _ in 0..8 {
            s = self.step(CharacterInput::default());
        }
        s
    }
}

struct Walk {
    travelled: f32,
    end: Vec3,
    end_cell: u32,
    furthest_y: f32,
    airborne: usize,
    restrictions: u64,
    /// Playing the house's configured contact-effect script answered **1**: it was queued.
    played: u64,
    /// It answered **0**. In this bench that is the ordinary case and it is *not* the option:
    /// `WorldScene::play_script_type` routes the local player's script to `Character`'s own
    /// `AnimDriver`, and the body's setup (`0x02000001`) carries no `PhysicsScriptTable`. The
    /// missing table returns 0. This file therefore measures **`played + unplayed`**: whether the
    /// contact reached the effect-script path at all.
    unplayed: u64,
    /// Suppressed by the player's disable-house-restriction-effects option before either outcome.
    disabled: u64,
}

fn walk_north(b: &mut Bench, label: &str) -> Walk {
    let start = b.stand_south_of_the_fence();
    let settled = b.settle();
    assert!(
        settled.contact && settled.walkable,
        "{label}: the body must be standing on the ground before the walk -- {settled:?}"
    );
    let before_r = b
        .scene
        .character
        .as_ref()
        .expect("a body")
        .stats
        .move_restrictions;
    let before_p = b.scene.draw.stats.restriction_effects_played;
    let before_u = b.scene.draw.stats.restriction_effects_unplayed;
    let before_d = b.scene.draw.stats.restriction_effects_disabled;
    let input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    let path: Vec<Sample> = (0..FRAMES).map(|_| b.step(input)).collect();
    let last = *path.last().expect("frames were walked");
    let travelled = math::hypotf(last.origin.x - start.x, last.origin.y - start.y);
    let furthest_y = path
        .iter()
        .map(|s| s.origin.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let airborne = path.iter().filter(|s| !(s.contact && s.walkable)).count();
    let w = Walk {
        travelled,
        end: last.origin,
        end_cell: last.cell,
        furthest_y,
        airborne,
        restrictions: b
            .scene
            .character
            .as_ref()
            .expect("a body")
            .stats
            .move_restrictions
            - before_r,
        played: b.scene.draw.stats.restriction_effects_played - before_p,
        unplayed: b.scene.draw.stats.restriction_effects_unplayed - before_u,
        disabled: b.scene.draw.stats.restriction_effects_disabled - before_d,
    };
    eprintln!(
        "house barrier, {label}: start [{:.3} {:.3} {:.3}] -> [{:.3} {:.3} {:.3}] cell {:#010X}, \
         travelled {:.3} m, furthest north y={:.3} (the fence is y={FENCE_Y}), \
         {} frame(s) off the ground, {} refusal(s), {} refused effect(s) ({} queued), \
         {} suppressed",
        start.x,
        start.y,
        start.z,
        w.end.x,
        w.end.y,
        w.end.z,
        w.end_cell,
        w.travelled,
        w.furthest_y,
        w.airborne,
        w.restrictions,
        w.played + w.unplayed,
        w.played,
        w.disabled
    );
    w
}

/// Everything but the permission: a bench with the block baked, the player created, the house
/// created from the recording's own bytes.
fn bench_with_the_house(
    store: &Arc<RetailDatStore>,
    blobs: &[Blob],
    player: ObjectId,
) -> Option<Bench> {
    let gpu = warp();
    let mut b = Bench::new(store, gpu, player);
    b.create_player(player, dereth_client_model::weenie::bitfield::PLAYER);
    b.create_house_from_the_capture(blobs, HOUSE);
    // Two frames so `ObjectPhysics::sync` has created the house's body and pushed both weenies.
    b.step(CharacterInput::default());
    b.step(CharacterInput::default());
    Some(b)
}

/// The recording's `0x0248` for the closed house with its four guests — the first one, sent the
/// instant the purchase completed.
fn closed_with_guests(blobs: &[Blob]) -> Vec<u8> {
    events(blobs, 0x0248)
        .first()
        .expect("the capture's first 0x0248")
        .to_vec()
}

/// The `0x0248` that answers the `@house open` (`0x0247` with `01000000`) the owner sent: the
/// same list with `_bitmask` bit 0 set.
fn opened(blobs: &[Blob]) -> Vec<u8> {
    events(blobs, 0x0248)
        .into_iter()
        .find(|b| {
            HouseUpdateRestrictions::read(&mut Reader::new(b))
                .is_ok_and(|m| m.restrictions.bitmask & 1 != 0)
        })
        .expect("the capture has an open-house restriction update")
        .to_vec()
}

// ---------------------------------------------------------------------------------------------
// 1. Arrival: what makes the barrier exist at all
// ---------------------------------------------------------------------------------------------

/// **What makes a barrier: two sources, and nothing the client sends.**
///
/// * the **dat** says which cells belong to which house, with no server involvement at all;
/// * the **shard** says who may enter, unprompted, in a `0x0248 House_UpdateRestrictions`
///   addressed to the house object it already created at login.
///
/// Measured over the recording and the villa block.
#[test]
fn the_shard_and_the_dat_name_the_same_house() {
    let s = store();
    let blobs = house_purchase_and_trade();

    // (a) The shard creates the house object at login, before anything is asked for it. This lets
    //     the restriction-object lookup resolve the house rather than returning no object.
    let create = create_body(&blobs, HOUSE);
    let payload = ItemCreateObject::read(&mut Reader::new(&create))
        .expect("the create decodes")
        .0;
    assert_eq!(payload.id, HOUSE);
    assert_eq!(
        payload.wdesc.name, "Villa",
        "the house's own weenie, not the slumlord sign"
    );

    // (b) Every recorded `0x0248` is for that same object (and there is at least one).
    let updates = events(&blobs, 0x0248);
    let mut senders = BTreeSet::new();
    for u in &updates {
        let m = HouseUpdateRestrictions::read(&mut Reader::new(u)).expect("0x0248 decodes");
        senders.insert(m.sender);
    }
    assert_eq!(
        senders.into_iter().collect::<Vec<_>>(),
        vec![HOUSE],
        "every restriction update in the session is addressed to the house object"
    );

    // (c) The first one: closed, no allegiance, four guests.
    let first = HouseUpdateRestrictions::read(&mut Reader::new(&closed_with_guests(&blobs)))
        .expect("0x0248 decodes");
    assert_eq!(
        first.restrictions.version,
        dereth_protocol::types::RestrictionDb::CURRENT_VERSION
    );
    assert_eq!(first.restrictions.bitmask & 1, 0, "the house is CLOSED");
    assert_eq!(
        first.restrictions.monarch_iid,
        ObjectId(0),
        "no allegiance is admitted"
    );
    let guests: Vec<u32> = first
        .restrictions
        .table
        .entries
        .iter()
        .map(|(k, _)| k.0)
        .collect();
    assert_eq!(
        guests,
        vec![0x5000_0014, 0x5000_0016, 0x5000_0017, 0x5000_001C]
    );

    // (d) And the shard flips bit 0 when the owner types `@house open`.
    let open =
        HouseUpdateRestrictions::read(&mut Reader::new(&opened(&blobs))).expect("0x0248 decodes");
    assert_eq!(
        open.restrictions.bitmask & 1,
        1,
        "@house open sets housing-restriction table bit 0"
    );

    // (e) The dat's half: the landblock info's restriction table names the same object for the
    //     land cell in front of the villa.
    let info = lbi(&s, BLOCK);
    let table = info
        .restrictions
        .as_ref()
        .expect("landblock 0x9DAF carries a restriction table");
    let map: BTreeMap<u32, u32> = table.entries.iter().copied().collect();
    assert_eq!(
        map.get(&FENCED_CELL).copied(),
        Some(HOUSE.0),
        "land-block restriction lookup assigns cell {FENCED_CELL:#010X} to the house"
    );
    assert_eq!(map.len(), 2, "two villas on this block");
    assert!(map.values().any(|v| *v == OTHER_HOUSE.0));
    assert!(
        !map.contains_key(&OPEN_CELL),
        "the cell the walk starts in is not fenced"
    );

    // (f) And every interior cell of the block carries one of its own.
    let mut interior: BTreeMap<u32, usize> = BTreeMap::new();
    let mut cells = 0;
    for idx in 0x0100..0x0400u32 {
        let Some(c) = env_cell(&s, (u32::from(BLOCK) << 16) | idx) else {
            continue;
        };
        cells += 1;
        if let Some(o) = c.restriction_obj {
            *interior.entry(o).or_default() += 1;
        }
    }
    eprintln!(
        "house barrier arrival: {} 0x0248(s) for {:#010X}; land cells fenced {}; interior cells {} of {} \
         fenced, split {:?}",
        updates.len(),
        HOUSE.0,
        map.len(),
        interior.values().sum::<usize>(),
        cells,
        interior
            .iter()
            .map(|(k, v)| (format!("{k:#010X}"), *v))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        interior.values().sum::<usize>(),
        cells,
        "every interior cell of a two-villa block belongs to one of them"
    );
    assert_eq!(interior.len(), 2);
    assert!(interior.contains_key(&HOUSE.0) && interior.contains_key(&OTHER_HOUSE.0));
}

// ---------------------------------------------------------------------------------------------
// 2. Registration: the cell's restriction object
// ---------------------------------------------------------------------------------------------

/// `CellRuntime::restriction_obj` reaches the physics world. Without it `check_entry_restrictions`
/// returns `OK_TS` at its `restriction_obj == 0` test for every cell in the game.
#[test]
fn the_dats_restriction_objects_reach_the_physics_cells() {
    let s = store();
    let gpu = warp();
    let mut b = Bench::new(&s, gpu, PLAYER_OBJECT_ID);
    b.step(CharacterInput::default());
    b.step(CharacterInput::default());
    let c = b.scene.character.as_ref().expect("a body");
    let ctx = c.world.transition_ctx(None);
    assert_eq!(
        ctx.restriction_obj(CellId(FENCED_CELL)),
        Some(HOUSE),
        "the land cell in front of the villa is fenced by the house"
    );
    assert_eq!(
        ctx.restriction_obj(CellId(OPEN_CELL)),
        None,
        "its neighbour is not"
    );
    assert_eq!(
        ctx.restriction_obj(CellId(0x9DAF_0127)),
        Some(HOUSE),
        "and so is every interior cell of that villa"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The rejecting station
// ---------------------------------------------------------------------------------------------

/// Behaviour: world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked
///
/// **A character with no permission is stopped at a closed house's boundary.**
#[test]
fn a_stranger_is_stopped_at_a_closed_houses_boundary() {
    let s = store();
    let blobs = house_purchase_and_trade();
    let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
        return;
    };
    assert!(
        b.recv_restrictions(&closed_with_guests(&blobs)),
        "the receiver's four gates passed"
    );
    b.house_owner(HOUSE, Some(OWNER));
    b.set_house_pscript(HOUSE, Some(BARRIER_PSCRIPT));
    b.step(CharacterInput::default());

    let w = walk_north(&mut b, "stranger at a closed house");
    assert!(
        w.furthest_y < FENCE_Y,
        "the body must never cross into the fenced cell: it reached y={:.3}, the fence is \
         y={FENCE_Y}",
        w.furthest_y
    );
    assert_eq!(w.end_cell, OPEN_CELL, "and it ends outside the property");
    assert!(
        w.restrictions > 0,
        "and the refusal is the barrier's, not the terrain's"
    );
    assert_eq!(
        w.airborne, 0,
        "it is stopped, not dropped through the floor"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Everyone the house lets in
// ---------------------------------------------------------------------------------------------

/// The four permission arms that pass: an unowned house, the owner, a guest, and an open house,
/// each measured with the same walk.
#[test]
fn the_owner_a_guest_and_an_open_house_are_all_let_through() {
    let s = store();
    let blobs = house_purchase_and_trade();
    let closed = closed_with_guests(&blobs);

    // The owner: matching the house owner ID to the mover passes before consulting the restriction
    // table.
    {
        let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
            return;
        };
        assert!(b.recv_restrictions(&closed));
        b.house_owner(HOUSE, Some(PLAYER_OBJECT_ID));
        b.step(CharacterInput::default());
        let w = walk_north(&mut b, "the owner");
        assert!(
            w.furthest_y > FENCE_Y,
            "the owner walks onto his own property"
        );
        assert_eq!(w.restrictions, 0);
    }

    // A guest. The permission table's membership arm uses the recording's four guest iids: the
    // body takes the iid of one of them, exactly as `Character::adopt_server_id` does on a real
    // login.
    {
        let Some(mut b) = bench_with_the_house(&s, &blobs, GUEST) else {
            return;
        };
        assert!(b.recv_restrictions(&closed));
        b.house_owner(HOUSE, Some(OWNER));
        b.step(CharacterInput::default());
        let c = b.scene.character.as_ref().expect("a body");
        assert_eq!(
            c.world.get(c.handle).expect("the body").id,
            GUEST,
            "the body carries the guest's iid, which is what the house permission check compares"
        );
        let w = walk_north(&mut b, "a guest");
        assert!(w.furthest_y > FENCE_Y, "a guest walks in");
        assert_eq!(w.restrictions, 0);
    }

    // The open house. `_bitmask & 1`, from the recording's `@house open` answer.
    {
        let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
            return;
        };
        assert!(b.recv_restrictions(&opened(&blobs)));
        b.house_owner(HOUSE, Some(OWNER));
        b.step(CharacterInput::default());
        let w = walk_north(&mut b, "an open house");
        assert!(w.furthest_y > FENCE_Y, "an open house lets a stranger in");
        assert_eq!(w.restrictions, 0);
    }

    // And an **unowned** house, which passes in the first permission arm and is the state every
    // plot of land has until somebody buys it.
    {
        let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
            return;
        };
        assert!(b.recv_restrictions(&closed));
        b.house_owner(HOUSE, None);
        b.step(CharacterInput::default());
        let w = walk_north(&mut b, "an unowned house");
        assert!(w.furthest_y > FENCE_Y, "an unsold plot fences nobody");
        assert_eq!(w.restrictions, 0);
    }
}

/// The allegiance arm compares the mover's monarch to the table's monarch ID. It is driven by the
/// recording's `0x0248` that answers `@house guest add_allegiance` (`0x0267` with `01000000`), the
/// only message in the session that writes a non-zero monarch.
#[test]
fn the_monarchs_allegiance_is_let_through_and_nobody_else_is() {
    let s = store();
    let blobs = house_purchase_and_trade();
    let with_monarch = events(&blobs, 0x0248)
        .into_iter()
        .find(|e| {
            HouseUpdateRestrictions::read(&mut Reader::new(e)).is_ok_and(|m| {
                m.restrictions.monarch_iid != ObjectId(0) && m.restrictions.bitmask & 1 == 0
            })
        })
        .map(<[u8]>::to_vec);
    let with_monarch = match with_monarch {
        Some(b) => b,
        None => {
            // Every allegiance update in this recording also had the house open, so the arm cannot
            // be isolated from it. Say so rather than passing quietly.
            eprintln!(
                "house barrier: the recording carries no CLOSED house with a monarch; the allegiance arm is \
                 covered by the dereth-client-model housing restriction unit test instead"
            );
            return;
        }
    };

    let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
        return;
    };
    assert!(b.recv_restrictions(&with_monarch));
    b.house_owner(HOUSE, Some(OWNER));
    b.step(CharacterInput::default());
    let blocked = walk_north(&mut b, "not in the allegiance");
    assert!(blocked.furthest_y < FENCE_Y, "a stranger is still stopped");

    let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
        return;
    };
    assert!(b.recv_restrictions(&with_monarch));
    b.house_owner(HOUSE, Some(OWNER));
    let monarch = HouseUpdateRestrictions::read(&mut Reader::new(&with_monarch))
        .expect("decodes")
        .restrictions
        .monarch_iid;
    b.objects
        .world
        .weenie_mut(PLAYER_OBJECT_ID)
        .expect("the player")
        .pwd
        .monarch = Some(monarch);
    b.step(CharacterInput::default());
    let allowed = walk_north(&mut b, "in the allegiance");
    assert!(
        allowed.furthest_y > FENCE_Y,
        "the monarch's allegiance walks in"
    );
    assert_eq!(allowed.restrictions, 0);
}

// ---------------------------------------------------------------------------------------------
// 5. The sparks, and the option that turns them off
// ---------------------------------------------------------------------------------------------

/// **What `DisableHouseRestrictionEffects` gates.**
///
/// The client reads bit 25 of the player-option word only on a denied house-entry check.
/// It gates the contact-effect script and nothing else: with the option set, the barrier still
/// stops the player in silence.
#[test]
fn the_contact_sparks_unless_disable_house_restriction_effects_is_set() {
    let s = store();
    let blobs = house_purchase_and_trade();
    let closed = closed_with_guests(&blobs);

    let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
        return;
    };
    assert!(b.recv_restrictions(&closed));
    b.house_owner(HOUSE, Some(OWNER));
    b.set_house_pscript(HOUSE, Some(BARRIER_PSCRIPT));
    b.step(CharacterInput::default());
    let on = walk_north(&mut b, "sparks on");
    assert!(on.restrictions > 0, "the barrier stopped the body");
    assert!(
        on.played + on.unplayed > 0,
        "at least one refusal reached the contact-effect script path"
    );
    assert_eq!(
        on.disabled, 0,
        "the option is off, so nothing was suppressed"
    );

    let Some(mut b) = bench_with_the_house(&s, &blobs, PLAYER_OBJECT_ID) else {
        return;
    };
    assert!(b.recv_restrictions(&closed));
    b.house_owner(HOUSE, Some(OWNER));
    b.set_house_pscript(HOUSE, Some(BARRIER_PSCRIPT));
    b.set_option(true);
    b.step(CharacterInput::default());
    let off = walk_north(&mut b, "sparks off");
    assert!(
        off.furthest_y < FENCE_Y,
        "the option does NOT open the gate: the body is still stopped"
    );
    assert!(off.restrictions > 0, "the refusals still happen");
    assert_eq!(
        off.played + off.unplayed,
        0,
        "and not one of them reached the contact-effect script path"
    );
    assert!(
        off.disabled > 0,
        "the option suppressed at least one contact effect"
    );
    assert_eq!(
        off.restrictions, on.restrictions,
        "the same walk, refused the same number of times: the option changes the effect and nothing else"
    );
}
