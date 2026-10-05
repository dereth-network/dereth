//! Each of the sixteen selection actions (closest, next and previous compass item, item, monster,
//! player, and the unopened-corpse select and use actions) picks the right object or refuses,
//! including the wrap-around retry when nothing new is found; the selection cycle reads both
//! physics-state bits, refuses an object with no position, and gates at the radius the player's
//! own cell chooses. Fixture: `interaction::use_time` (the step `App::frame` calls) with a real
//! body on a headless WARP device, objects created through `0xF745` at exact player-space offsets
//! and the real visibility-table sweep; the retail dats. No window, no injected input.
//!
//! # Why every action gets its own assertion
//!
//! Sixteen actions asserted in aggregate would pass with fifteen dead. Each one passes its own
//! `(closer, ignore_current, kind, exclude_own_wielded)` tuple, and exactly one,
//! `SelectionClosestItem 0x10000032`, sets the own-wielded exclusion. So each action below is
//! driven on its own, at a station where a *neighbouring* action's tuple would give a different
//! answer, and the expected object is named.
//!
//! Three things are asserted separately from the sixteen, because each would pass with the
//! sixteen right and itself dead:
//!
//! 1. **The re-call on no movement**: when the first search leaves the selected object unchanged,
//!    the action searches again. It is the whole wrap-around, and **a station where a candidate is
//!    always found cannot see it**: the first call succeeds, the selection moves, and the retry
//!    never runs. The two tests for it stand at the end of the cycle on purpose.
//! 2. **That the four "Closest" actions have no retry**, the negative control for (1): the same
//!    "nothing was found" station must leave the selection alone and the wrap counter at 0.
//! 3. **The own-wielded exclusion**, at the one action that sets it and at the two neighbours that
//!    do not.
//!
//! The actions are constructed as `InputEvent`s and handed to `use_time`; that they also arrive
//! from a *shipped key* is asserted by the action census. Every station is on a **non-cardinal**
//! heading (3/4/5 triples, 53.13° off axis), so an axis-only transform cannot satisfy the geometry
//! checks.

#![cfg(gpu)]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_client_model::selection::{CLOAKED_PS, REPORT_COLLISIONS_AS_ENVIRONMENT_PS};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::PLAYER_OBJECT_ID;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::selection_geometry::SceneSelectionPhysics;
use dereth_dat::RetailDatStore;
use dereth_input::{ActionId, InputMapId};
use dereth_physics::LandSource;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};
use {
    dereth_client_runtime::interaction::action as ia,
    dereth_client_runtime::interaction::Interaction,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

/// `ItemSelectionCommands`, the map every one of the sixteen is bound in. The value only has to
/// be a map; `on_actions` does not look at it, and which map dispatches these ids is the action
/// census's question, not this file's.
const MAP: InputMapId = InputMapId(0x1000_0007);

// =================================================================================================
// Bench
// =================================================================================================

/// A live body, a live object stream, and one `Interaction` — the three things `use_time` needs.
struct Bench {
    _gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    inter: Interaction,
    /// The local body's position, cached so placements can be computed backwards from a
    /// desired **player-space** offset rather than guessed in block space.
    player: Position,
    now: f64,
}

impl Bench {
    fn new() -> Self {
        let store = Arc::new(
            dereth_dat::testing::open_store()
                .expect("the retail dats are `use_time`'s own argument: set DERETH_TEST_DAT_DIR"),
        );
        let cfg = DeviceConfig {
            width: 800,
            height: 600,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new(None, &cfg).expect("a D3D12 WARP device");
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        // The cheapest scene that still produces a body. Nothing in this file touches terrain,
        // statics or collision: `use_time` reads exactly one thing off the scene for the selection
        // cycle -- `character.position()` -- and the geometry seam is fed from the object stream.
        // A one-block window without cell statics or part BSPs loads in a fraction of the time,
        // which matters because every test here builds its own scene and the whole file is a
        // mutation target. `character: true` is the only field that is load-bearing.
        let cfg = SceneConfig {
            cell_statics: false,
            mesh_collision: false,
            land_radius: 1,
            scenery_radius: 0,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        let player = scene.character.as_ref().expect("a body").position();
        assert!(
            dereth_physics::landdefs::is_outdoors(player.cell),
            "the premise: the body stands on the landscape, so the cycle's radius is the outdoor \
             75.0 and every station below is inside it"
        );

        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);

        Self {
            _gpu: gpu,
            scene,
            store,
            objects,
            inter: Interaction::new(),
            player,
            now: 1.0,
        }
    }

    /// Put an object in the world at an exact **player-space** offset, through the client's own
    /// `0xF745 Item_CreateObject` path, and prove it landed where the station meant.
    ///
    /// The position is computed backwards through the same transform the seam reads forwards:
    /// `localtolocal(player, obj, 0)` with both in one cell is `globaltolocal(player.frame,
    /// obj.origin)`, so placing `obj.origin = localtoglobal(player.frame, offset)` makes the
    /// measured offset exactly the requested one. It is asserted rather than assumed — without
    /// that, a body whose spawn frame carries a rotation would silently move every station.
    fn place(&mut self, id: ObjectId, offset: (f32, f32, f32), state: u32) {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};

        let v = Vec3::new(offset.0, offset.1, offset.2);
        let origin = dereth_physics::math::localtoglobal(&self.player.frame, v);
        let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id,
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield: flags::POSITION,
                    state,
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: self.player.cell.0,
                        frame: dereth_protocol::types::Frame {
                            origin: origin.into(),
                            orientation: Quat::IDENTITY.into(),
                        },
                    }),
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );

        // Premises. Without these a station that never reached the tables reads as a filter that
        // refused it, which is the same silence twice over.
        assert!(
            self.objects.presence(id).is_some(),
            "the create made a presence"
        );
        // There is one copy of the physics-state word, and `ObjectStream::physics_state` reads it.
        assert_eq!(
            self.objects.world.physics(id).expect("the game table has it").state,
            state,
            "and `dereth_client_model::objects::PhysicsPresence` carries the whole word too, not bit 0; without this the two selection-cycle state bits have no producer there"
        );
        let measured = SceneSelectionPhysics::new(Some(&self.player), &self.objects)
            .get(id)
            .expect("the seam can answer for it")
            .player_space;
        for (got, want, axis) in [
            (measured.0, offset.0, 'x'),
            (measured.1, offset.1, 'y'),
            (measured.2, offset.2, 'z'),
        ] {
            assert!(
                (got - want).abs() < 1e-3,
                "{id:?} is at player-space {axis} = {got}, the station asked for {want}"
            );
        }
        self.objects.world.update_visible_object_list();
        assert!(
            self.objects.world.tables.visible.contains(&id),
            "the actual visibility sweep must have made {id:?} visible"
        );
    }

    /// The same create with **no `POSITION` flag** — `PhysicsDesc.position` absent, which is what
    /// a contained or wielded object's `0xF745` carries and what
    /// `dereth_client_runtime::objects::Presence::position`'s own doc calls "`None` for a contained or
    /// wielded object, which has no position of its own".
    ///
    /// The player-space conversion fails when the object has no cell, and the selection cycle
    /// skips such an object. Without a positionless object in the file, "the seam refuses it"
    /// would be unfalsifiable: giving it the *player's own* position would pass unnoticed.
    fn place_unpositioned(&mut self, id: ObjectId) {
        use dereth_protocol::types::{ObjDesc, PhysicsDesc, PublicWeenieDesc};
        let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id,
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield: 0,
                    state: 0,
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
        assert!(
            self.objects
                .presence(id)
                .expect("created")
                .position
                .is_none(),
            "the premise: this object has no position of its own"
        );
        // It is in no cell, so the real sweep leaves it out of the visible table as well -- which
        // is a *second* reason it cannot be selected, and the reason the assertion below has to be
        // about the seam and not only about the winner.
        self.objects.world.update_visible_object_list();
    }

    fn weenie_mut(&mut self, id: ObjectId) -> &mut dereth_client_model::Weenie {
        self.objects
            .world
            .tables
            .weenies
            .get_mut(id)
            .expect("placed")
    }

    /// The monster selection kind: attackable, not a vendor, shown on the radar, not a fellow.
    fn make_monster(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4); // ShowAlways
        assert!(
            self.objects.world.object_is_attackable(id),
            "premise: {id:?} is attackable"
        );
    }

    /// The item selection kind: not wielded, and no radar behaviour — which is the arm that reads
    /// backwards. Loose loot is what "closest item" wants, and an NPC with `ShowAlways` is not.
    fn make_item(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.radar_enum = None;
        w.pwd.wielder_id = None;
    }

    /// The compass-item selection kind: shown on the radar. In `NonCombat` the whole attackability
    /// half of that arm is skipped (only modes 2 and 4 reach it), which is the state
    /// this bench is in.
    fn make_compass_item(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.radar_enum = Some(4); // ShowAlways
    }

    /// The player selection kind: player bit `0x08`, plus radar visibility.
    fn make_player(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.bitfield |= bitfield::PLAYER;
        w.pwd.radar_enum = Some(4);
        assert!(
            self.objects.world.weenie(id).expect("placed").is_player(),
            "premise: a player"
        );
    }

    /// The unopened-corpse selection kind: the corpse bit and not opened this session.
    fn make_corpse(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.bitfield |= bitfield::CORPSE;
        // Useable, so the object-use arm is reachable for the two cases that use it.
        w.pwd.useability = Some(0x0000_0080);
        assert!(
            self.objects.world.weenie(id).expect("placed").is_corpse(),
            "premise: a corpse"
        );
        assert!(
            !self.objects.world.has_corpse_been_opened(id),
            "premise: this session has not opened it"
        );
    }

    /// One press of one action, through the production frame slot.
    fn press(&mut self, action: u32) {
        self.now += 1.0;
        let e = dereth_client_runtime::actions::Action {
            id: ActionId(action),
            phase: dereth_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        let (unowned, left) = dereth_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            Some(&self.scene),
            &mut self.objects,
            None,
            vec![e],
            false,
            (800, 600),
            LocalTime(self.now),
        );
        assert!(unowned.is_empty(), "no unowned requests were expected");
        assert!(
            left.is_empty(),
            "{action:#010X} must be consumed by `on_actions`, not returned to the caller"
        );
    }

    fn selected(&self) -> Option<ObjectId> {
        self.objects.world.selected
    }

    fn select(&mut self, id: Option<ObjectId>) {
        let mut out = dereth_client_model::RecordingSink::default();
        self.objects.world.set_selected_object(id, false, &mut out);
    }
}

// Three of a kind on a 53.13° heading at 3 m, 6 m and 9 m — well inside the outdoor 75.
const NEAR: ObjectId = ObjectId(0x8000_0010);
const MID: ObjectId = ObjectId(0x8000_0011);
const FAR: ObjectId = ObjectId(0x8000_0012);
const NEAR_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);
const MID_AT: (f32, f32, f32) = (3.6, 4.8, 0.0);
const FAR_AT: (f32, f32, f32) = (5.4, 7.2, 0.0);

/// Three candidates of one kind, nothing selected.
fn three(kind: fn(&mut Bench, ObjectId)) -> Bench {
    let mut b = Bench::new();
    for (id, at) in [(NEAR, NEAR_AT), (MID, MID_AT), (FAR, FAR_AT)] {
        b.place(id, at, 0);
        kind(&mut b, id);
    }
    b
}

// =================================================================================================
// 0. Calibration — the bench must be able to produce a selection at all
// =================================================================================================

/// Before any "it selected the right one" is believed, the bench has to be shown to select
/// *anything*, and to be shown capable of the opposite.
///
/// Both directions in one test: the same press against three visible monsters selects one, and
/// against a world whose only candidate is out of the outdoor radius selects nothing. Without the
/// second half, every assertion in this file would pass against a bench that selects the nearest
/// object unconditionally.
#[test]
fn the_bench_can_select_and_can_refuse() {
    let mut b = three(Bench::make_monster);
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR), "the bench selects");
    assert_eq!(b.inter.stats.selection_cycles, 1, "one arm ran, once");
    assert_eq!(
        b.inter.stats.selection_cycles_without_geometry, 0,
        "the snapshot was populated: this is the denominator that separates 'filtered' from \
         'there was no body'"
    );

    // And the refusal: one monster at 3/4/5 scaled to 100 m, outside the outdoor 75.
    let mut b = Bench::new();
    b.place(FAR, (60.0, 80.0, 0.0), 0);
    b.make_monster(FAR);
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(
        b.selected(),
        None,
        "100 m is outside the outdoor radar radius of 75.0"
    );
    assert_eq!(b.inter.stats.selection_cycles, 1, "the arm still ran");
}

/// The seam itself, apart from the cycle: the two state bits `select_next` reads have a
/// **producer** in this build, and it is the whole physics-state word `PhysicsPresence::state`
/// carries, the only copy of it. Neither bit is a hard-coded `false` in `selection_geometry.rs`.
#[test]
fn the_seam_reads_both_state_bits_off_the_live_word() {
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT, CLOAKED_PS);
    b.place(MID, MID_AT, REPORT_COLLISIONS_AS_ENVIRONMENT_PS);
    b.place(FAR, FAR_AT, 0);

    let snap = SceneSelectionPhysics::new(Some(&b.player), &b.objects);
    let near = snap.get(NEAR).expect("placed");
    assert!(near.cloaked, "CLOAKED_PS is 0x100000 and the seam reads it");
    assert!(!near.reports_collisions_as_environment);
    let mid = snap.get(MID).expect("placed");
    assert!(
        mid.reports_collisions_as_environment,
        "REPORT_COLLISIONS_AS_ENVIRONMENT_PS is 0x200000"
    );
    assert!(!mid.cloaked);
    let far = snap.get(FAR).expect("placed");
    assert!(!far.cloaked && !far.reports_collisions_as_environment);

    // And the cloaked one is refused by the cycle, which is the behaviour the bit exists for.
    // Both are monsters, so the only difference between them is the state word.
    b.make_monster(NEAR);
    b.make_monster(MID);
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(
        b.selected(),
        Some(MID),
        "the nearer candidate is CLOAKED_PS and is skipped"
    );
}

/// **A positionless object is not a candidate, and the seam is where it is refused.**
///
/// Player-space conversion fails when the object has no cell, and the selection cycle skips that
/// result, so a wielded sword or an item in your pack is never tabbed to, however near its holder
/// is. In this build the same fact is `Presence::position == None`.
///
/// Answering such an object with the *player's own* position instead of `None` would put it at
/// distance zero, where it wins every cycle; this station is the one that catches that.
#[test]
fn an_object_with_no_position_is_refused_by_the_seam() {
    const HELD: ObjectId = ObjectId(0x8000_0020);
    let mut b = three(Bench::make_monster);
    b.place_unpositioned(HELD);
    b.make_monster(HELD);

    let snap = SceneSelectionPhysics::new(Some(&b.player), &b.objects);
    assert_eq!(snap.get(HELD), None, "no position, no player space");
    assert_eq!(
        snap.len(),
        3,
        "the denominator: the other three ARE answerable"
    );
    assert!(
        snap.get(NEAR).is_some(),
        "the control -- a placed object is answered"
    );

    // ...and it does not win the cycle, which is what the `None` is for. Driven through the wire
    // so the refusal is the production one.
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "the nearest PLACED monster, not the positionless one"
    );
}

/// With no local body there is no player space, so the snapshot is empty and the cycle can select
/// nothing: the conversion's no-player answer, propagated. The counter is
/// what makes that a statement rather than a silence.
#[test]
fn with_no_body_the_snapshot_is_empty_and_the_counter_says_so() {
    let mut b = three(Bench::make_monster);
    let empty = SceneSelectionPhysics::new(None, &b.objects);
    assert!(
        empty.is_empty(),
        "no origin, no player space, no candidates"
    );
    assert_eq!(empty.get(NEAR), None);

    // Driven through the frame with `world = None`, which is what a headless frame passes.
    let (unowned, left) = dereth_client_runtime::interaction::use_time(
        &mut b.inter,
        &b.store,
        None,
        &mut b.objects,
        None,
        vec![dereth_client_runtime::actions::Action {
            id: ActionId(ia::SELECTION_CLOSEST_MONSTER),
            phase: dereth_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        }],
        false,
        (800, 600),
        LocalTime(9.0),
    );
    assert!(unowned.is_empty() && left.is_empty());
    assert_eq!(b.selected(), None);
    assert_eq!(b.inter.stats.selection_cycles, 1, "the arm ran");
    assert_eq!(
        b.inter.stats.selection_cycles_without_geometry, 1,
        "and it says WHY it selected nothing"
    );
}

// =================================================================================================
// 1. The sixteen arms, one at a time
// =================================================================================================
//
// Each of the twelve non-corpse arms is driven at a station where its own tuple is the only one
// that gives the asserted answer:
//
// * "Closest" — nothing selected, expect NEAR. ("Next" from an empty selection gives FAR because
//   the direction flip also fires when nothing is selected; that is asserted below.)
// * "Previous" — MID selected, expect NEAR (inward).
// * "Next" — MID selected, expect FAR (outward).

#[test]
fn closest_compass_item() {
    let mut b = three(Bench::make_compass_item);
    b.press(ia::SELECTION_CLOSEST_COMPASS_ITEM);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn previous_compass_item() {
    let mut b = three(Bench::make_compass_item);
    b.select(Some(MID));
    b.press(ia::SELECTION_PREVIOUS_COMPASS_ITEM);
    assert_eq!(b.selected(), Some(NEAR), "previous is inward");
    assert_eq!(
        b.inter.stats.selection_cycle_wraps, 0,
        "it found one, so it did not wrap"
    );
}

#[test]
fn next_compass_item() {
    let mut b = three(Bench::make_compass_item);
    b.select(Some(MID));
    b.press(ia::SELECTION_NEXT_COMPASS_ITEM);
    assert_eq!(b.selected(), Some(FAR), "next is outward");
    assert_eq!(b.inter.stats.selection_cycle_wraps, 0);
}

#[test]
fn closest_item() {
    let mut b = three(Bench::make_item);
    b.press(ia::SELECTION_CLOSEST_ITEM);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn previous_item() {
    let mut b = three(Bench::make_item);
    b.select(Some(MID));
    b.press(ia::SELECTION_PREVIOUS_ITEM);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn next_item() {
    let mut b = three(Bench::make_item);
    b.select(Some(MID));
    b.press(ia::SELECTION_NEXT_ITEM);
    assert_eq!(b.selected(), Some(FAR));
}

#[test]
fn closest_monster() {
    let mut b = three(Bench::make_monster);
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn previous_monster() {
    let mut b = three(Bench::make_monster);
    b.select(Some(MID));
    b.press(ia::SELECTION_PREVIOUS_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn next_monster() {
    let mut b = three(Bench::make_monster);
    b.select(Some(MID));
    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(b.selected(), Some(FAR));
}

#[test]
fn closest_player() {
    let mut b = three(Bench::make_player);
    b.press(ia::SELECTION_CLOSEST_PLAYER);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn previous_player() {
    let mut b = three(Bench::make_player);
    b.select(Some(MID));
    b.press(ia::SELECTION_PREVIOUS_PLAYER);
    assert_eq!(b.selected(), Some(NEAR));
}

#[test]
fn next_player() {
    let mut b = three(Bench::make_player);
    b.select(Some(MID));
    b.press(ia::SELECTION_NEXT_PLAYER);
    assert_eq!(b.selected(), Some(FAR));
}

/// `SelectionClosestUnopenedCorpse`. Selects and **stops**: no object-use tail.
#[test]
fn closest_unopened_corpse_selects_and_does_not_use() {
    let mut b = three(Bench::make_corpse);
    b.press(ia::SELECTION_CLOSEST_UNOPENED_CORPSE);
    assert_eq!(b.selected(), Some(NEAR));
    assert_eq!(
        b.inter.stats.selection_corpse_uses, 0,
        "closest unopened corpse has no object-use tail"
    );
}

/// `SelectionNextUnopenedCorpse`. The same, outward.
#[test]
fn next_unopened_corpse_selects_and_does_not_use() {
    let mut b = three(Bench::make_corpse);
    b.select(Some(MID));
    b.press(ia::SELECTION_NEXT_UNOPENED_CORPSE);
    assert_eq!(b.selected(), Some(FAR));
    assert_eq!(b.inter.stats.selection_corpse_uses, 0);
}

/// `SelectionUseClosestUnopenedCorpse`. Selects **and** uses.
#[test]
fn use_closest_unopened_corpse_selects_and_uses() {
    let mut b = three(Bench::make_corpse);
    b.press(ia::SELECTION_USE_CLOSEST_UNOPENED_CORPSE);
    assert_eq!(b.selected(), Some(NEAR));
    assert_eq!(
        b.inter.stats.selection_corpse_uses, 1,
        "the winner is a corpse, so it is used"
    );
}

/// `SelectionUseNextUnopenedCorpse`. The same, outward.
#[test]
fn use_next_unopened_corpse_selects_and_uses() {
    let mut b = three(Bench::make_corpse);
    b.select(Some(MID));
    b.press(ia::SELECTION_USE_NEXT_UNOPENED_CORPSE);
    assert_eq!(b.selected(), Some(FAR));
    assert_eq!(b.inter.stats.selection_corpse_uses, 1);
}

/// The object-use tail is gated on the corpse predicate and on nothing else — so the two
/// "Use" arms do **not** use a selection that is not a corpse, even one they just made.
///
/// The station is the pair that separates the gate from the arm: the same press, the same
/// selection, and the only difference is whether the winner carries `BF_CORPSE`. Without this the
/// `use_corpse` flag could be unconditional and every test above would still pass.
#[test]
fn the_use_tail_is_gated_on_is_corpse() {
    let mut b = three(Bench::make_corpse);
    // Take the corpse bit off the one that will win, leaving everything else identical.
    b.weenie_mut(NEAR).pwd.bitfield &= !bitfield::CORPSE;
    b.select(Some(NEAR));
    b.press(ia::SELECTION_USE_CLOSEST_UNOPENED_CORPSE);
    assert_eq!(
        b.selected(),
        Some(MID),
        "NEAR is no longer an unopened corpse, so MID wins"
    );
    assert_eq!(
        b.inter.stats.selection_corpse_uses, 1,
        "MID is one, so it is used"
    );

    // And the other direction: a selection that survives the press unchanged and is not a corpse.
    let mut b = three(Bench::make_corpse);
    for id in [NEAR, MID, FAR] {
        b.weenie_mut(id).pwd.bitfield &= !bitfield::CORPSE;
    }
    b.select(Some(MID));
    b.press(ia::SELECTION_USE_CLOSEST_UNOPENED_CORPSE);
    assert_eq!(
        b.selected(),
        Some(MID),
        "no candidate passes the filter, so nothing moves"
    );
    assert_eq!(
        b.inter.stats.selection_corpse_uses, 0,
        "and the standing selection is not a corpse"
    );
}

// =================================================================================================
// 2. The second search when the selected object id does not move
// =================================================================================================

/// **The wrap-around, asserted where it is the only thing that can produce the answer.**
///
/// FAR is selected, so "next" has nothing farther to find: the first
/// `select_next(false, false, kind, false)` selects nothing, the selected object id does not
/// move, and the arm re-calls with `(true, true, …)`. Inside, `ignore_current` takes the
/// direction-flip branch, which inverts `closer` **again** and plants the reference at `0.0` — so
/// the second call answers with the **nearest** candidate.
///
/// A station where a candidate is always found cannot see any of this: the first call would
/// succeed and the retry would never run. That is why this is its own test and not a clause of
/// `next_monster`.
#[test]
fn next_past_the_farthest_wraps_to_the_nearest() {
    let mut b = three(Bench::make_monster);
    b.select(Some(FAR));
    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "the cycle wrapped round to the near end"
    );
    assert_eq!(b.inter.stats.selection_cycles, 1, "one arm ran");
    assert_eq!(
        b.inter.stats.selection_cycle_wraps, 1,
        "and it made the second call"
    );
}

/// The mirror: NEAR is selected, "previous" has nothing nearer, and the retry gives the
/// **farthest**.
///
/// Asserted separately from the "next" direction because the two arms pass different first tuples
/// and the retry inverts each of them; a wrap that only worked one way round would pass the other
/// test.
#[test]
fn previous_past_the_nearest_wraps_to_the_farthest() {
    let mut b = three(Bench::make_monster);
    b.select(Some(NEAR));
    b.press(ia::SELECTION_PREVIOUS_MONSTER);
    assert_eq!(b.selected(), Some(FAR));
    assert_eq!(b.inter.stats.selection_cycle_wraps, 1);
}

/// **The negative control for the wrap: the four "Closest" arms have no retry.**
///
/// Same "nothing was found" station, a "Closest" action instead of a "Next" one. Retail's
/// "Closest" actions end with no second call, because they already passed
/// `ignore_current = true` and are therefore already at an end of the cycle. If the `wraps` flag
/// were wired on for every arm, this would select something and the counter would move.
#[test]
fn the_closest_arms_never_re_call() {
    let mut b = Bench::new();
    // One candidate, and it is the standing selection — so "closest" finds nothing new.
    b.place(MID, MID_AT, 0);
    b.make_monster(MID);
    b.select(Some(MID));
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(MID), "unchanged");
    assert_eq!(b.inter.stats.selection_cycles, 1);
    assert_eq!(
        b.inter.stats.selection_cycle_wraps, 0,
        "closest monster has no retry"
    );

    // The positive that proves the station really is a "nothing was found" one: the same world,
    // the same standing selection, driven through an arm that *does* wrap, moves.
    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(
        b.inter.stats.selection_cycle_wraps, 1,
        "the station can produce a wrap"
    );
    assert_eq!(
        b.selected(),
        Some(MID),
        "and with one candidate the wrap lands back on it"
    );
}

// =================================================================================================
// 3. Own-wielded exclusion, at the one call site of twenty-six that sets it
// =================================================================================================

/// **The own-wielded exclusion is INERT at the one call site that sets it.**
///
/// The exclusion is set at exactly one of the twenty-six call sites, `SelectionClosestItem
/// 0x10000032`, but that does not mean "closest item" skips your own drawn weapon while "next
/// item" picks it. The item-selection filter opens with
///
/// ```text
/// item selection type:
///   if object has any wielder, reject                    // any wielder, not just you
/// ```
///
/// and the own-wielded gate rejects the player's own wielded object, a **strict subset** of that
/// rejection whenever there is a player, which is every in-game state. So neither "closest item"
/// nor "next item" can pick a wielded object, and the flag cannot change the answer at the only
/// pairing any action ships.
///
/// Asserted in two halves, because "the flag does nothing" and "the flag is not transcribed" are
/// the same observation otherwise:
///
/// * **through the wire**, that all three item arms refuse a wielded object; and
/// * **at the game world's `select_next` method directly**, that `true` and `false` give the
///   *same* answer for the item kind and *different* answers for
///   the monster kind — so the argument is live and its inertness is a property of the
///   pairing, not of the code.
#[test]
fn exclude_own_wielded_is_inert_at_the_one_pairing_that_ships_it() {
    // (a) Through the wire.
    let mut b = three(Bench::make_item);
    b.weenie_mut(NEAR).pwd.wielder_id = Some(PLAYER_OBJECT_ID);

    b.press(ia::SELECTION_CLOSEST_ITEM);
    assert_eq!(b.selected(), Some(MID), "0x10000032 skips the wielded NEAR");

    // `SelectionPreviousItem` clears the own-wielded exclusion and still cannot reach it: the
    // filter refused it, so the first call finds nothing nearer than MID and the arm wraps to FAR.
    b.press(ia::SELECTION_PREVIOUS_ITEM);
    assert_eq!(
        b.selected(),
        Some(FAR),
        "0x10000033 clears the flag and STILL cannot select a wielded item -- it wrapped instead"
    );
    assert_eq!(
        b.inter.stats.selection_cycle_wraps, 1,
        "and the wrap is why, not a near hit"
    );

    // Wielded by somebody else is refused by the same gate, which is what makes it the filter's
    // rejection rather than the flag's.
    let mut b = three(Bench::make_item);
    b.weenie_mut(NEAR).pwd.wielder_id = Some(ObjectId(0x5000_00FF));
    b.press(ia::SELECTION_CLOSEST_ITEM);
    assert_eq!(
        b.selected(),
        Some(MID),
        "the wielder gate is about ANY wielder"
    );

    // (b) The argument itself, at `select_next`, where the two values can be compared. Two
    //     worlds, because the two kinds need different candidates: the `MONSTER` filter accepts a
    //     wielded creature and the `ITEM` filter does not, which is the whole point.
    use dereth_client_model::selection::SelectionType as K;

    // MONSTER: the flag is the only thing that can refuse NEAR, so the two values disagree.
    let mut b = three(Bench::make_monster);
    b.weenie_mut(NEAR).pwd.wielder_id = Some(PLAYER_OBJECT_ID);
    assert_eq!(
        (
            closest_with(&mut b, K::Monster, true),
            closest_with(&mut b, K::Monster, false)
        ),
        (Some(MID), Some(NEAR)),
        "the argument is live: it is transcribed and it works"
    );

    // ITEM: the any-wielder gate already refused NEAR, so the two values agree — and this is the
    // only pairing the action handler passes `true` with.
    let mut b = three(Bench::make_item);
    b.weenie_mut(NEAR).pwd.wielder_id = Some(PLAYER_OBJECT_ID);
    assert_eq!(
        (
            closest_with(&mut b, K::Item, true),
            closest_with(&mut b, K::Item, false)
        ),
        (Some(MID), Some(MID)),
        "and at the shipped pairing it cannot change the answer"
    );
}

/// `select_next(closer = true, ignore_current = true, kind, exclude)` against a cleared selection —
/// the action handler's "Closest" tuple with the flag under test, called directly so the two
/// values of the flag can be compared at one station.
fn closest_with(
    b: &mut Bench,
    kind: dereth_client_model::selection::SelectionType,
    exclude: bool,
) -> Option<ObjectId> {
    let snap = SceneSelectionPhysics::new(Some(&b.player), &b.objects);
    let mut out = dereth_client_model::RecordingSink::default();
    b.objects.world.selected = None;
    b.objects.world.select_next(
        true,
        true,
        kind,
        exclude,
        &|id| snap.get(id),
        dereth_client_model::range::RADAR_RADIUS_OUTDOORS,
        &mut out,
    );
    b.objects.world.selected
}

// =================================================================================================
// 4. Two readings of `select_next` that the wire is the first thing able to expose
// =================================================================================================

/// **"Next" on an empty selection selects the FARTHEST candidate, not the nearest.**
///
/// The direction flip fires whenever the reference object cannot be resolved
/// — including when nothing is selected at all — so `SelectionNextItem` with no selection inverts
/// `closer` to `true` and takes the reference from the `73728.0` sentinel. It reads like a defect,
/// it is faithful, and it is exactly why "Closest" is a separate action rather than an alias.
///
/// Pinned here at the *wire* because this is the first place a player-visible gesture can reach it.
#[test]
fn next_with_nothing_selected_selects_the_farthest() {
    let mut b = three(Bench::make_monster);
    assert_eq!(b.selected(), None, "premise: nothing is selected");
    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(
        b.selected(),
        Some(FAR),
        "the direction flip fires on an unresolvable reference, and an empty selection is one"
    );
    assert_eq!(
        b.inter.stats.selection_cycle_wraps, 0,
        "the first call found something"
    );
}

/// Behaviour: selection.cycle.each-selection-action-picks-or-refuses-as-the-client-does
///
/// **The five filters are not interchangeable, driven through five different actions in one
/// world.**
///
/// One monster, one loose item, one player and one corpse, at four distances, and each "closest"
/// action must pick its own. A build whose `kind` argument was ignored — or in which all sixteen
/// arms shared one tuple — would pick the same object four times.
#[test]
fn each_kind_selects_its_own_and_no_other() {
    const CORPSE: ObjectId = ObjectId(0x8000_0013);
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT, 0);
    b.make_monster(NEAR);
    b.place(MID, MID_AT, 0);
    b.make_item(MID);
    b.place(FAR, FAR_AT, 0);
    b.make_player(FAR);
    // 3/4/5 scaled to 12 m, the same heading.
    b.place(CORPSE, (7.2, 9.6, 0.0), 0);
    b.make_corpse(CORPSE);

    for (action, want, what) in [
        (ia::SELECTION_CLOSEST_MONSTER, NEAR, "monster"),
        (ia::SELECTION_CLOSEST_ITEM, MID, "item"),
        (ia::SELECTION_CLOSEST_PLAYER, FAR, "player"),
        (
            ia::SELECTION_CLOSEST_UNOPENED_CORPSE,
            CORPSE,
            "unopened corpse",
        ),
    ] {
        b.select(None);
        b.press(action);
        assert_eq!(
            b.selected(),
            Some(want),
            "{action:#010X} must select the {what}"
        );
    }

    // And the compass arm, which is the one that takes several of them: in `NonCombat` its whole
    // attackability half is skipped, so anything the radar shows qualifies — the monster and the
    // player here, and the nearest of those is the monster.
    b.select(None);
    b.press(ia::SELECTION_CLOSEST_COMPASS_ITEM);
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "compass item takes what the radar shows"
    );
}

/// **The claim the "no `e.start` gate" arm rests on, measured rather than asserted.**
///
/// The input manager issues a release event **only** for the three hold
/// families (`Hold`, `HoldRepeat`, `HoldContinuous`); a `OneShot` produces one `start: true` and
/// nothing else. So "the gate would be inert on the shipped keymap" is a claim about the shipped
/// `ActionMap`, and this reads it.
///
/// The assertion is `!is_hold()` rather than `== OneShot`, because `Invalid` (0) is also not a
/// hold — the input manager substitutes 3 for an invalid toggle — and an id the map does not carry
/// answers `Invalid`. The denominator is
/// asserted so that a lookup that silently examined nothing cannot read as sixteen passes.
#[test]
fn no_shipped_binding_for_the_sixteen_is_a_hold() {
    let store = dereth_dat::testing::open_store().expect("the shipped ActionMap is in the dats");
    let shell =
        dereth_client_shell::input::InputShell::new(&store, None).expect("the input tables decode");
    let sixteen = [
        ia::SELECTION_CLOSEST_COMPASS_ITEM,
        ia::SELECTION_PREVIOUS_COMPASS_ITEM,
        ia::SELECTION_NEXT_COMPASS_ITEM,
        ia::SELECTION_CLOSEST_ITEM,
        ia::SELECTION_PREVIOUS_ITEM,
        ia::SELECTION_NEXT_ITEM,
        ia::SELECTION_CLOSEST_MONSTER,
        ia::SELECTION_PREVIOUS_MONSTER,
        ia::SELECTION_NEXT_MONSTER,
        ia::SELECTION_CLOSEST_PLAYER,
        ia::SELECTION_PREVIOUS_PLAYER,
        ia::SELECTION_NEXT_PLAYER,
        ia::SELECTION_USE_CLOSEST_UNOPENED_CORPSE,
        ia::SELECTION_USE_NEXT_UNOPENED_CORPSE,
        ia::SELECTION_CLOSEST_UNOPENED_CORPSE,
        ia::SELECTION_NEXT_UNOPENED_CORPSE,
    ];
    assert_eq!(
        sixteen.len(),
        16,
        "the sixteen selection-action handler cases"
    );
    let mut examined = 0;
    for a in sixteen {
        let t = shell.manager.action_map.toggle_type(MAP, ActionId(a));
        assert!(
            !t.is_hold(),
            "{a:#010X} is bound as {t:?}, which the action dispatcher releases on key-up -- the \
             cycle would then run twice per press and the missing start/release gate would be \
             visible"
        );
        examined += 1;
    }
    assert_eq!(examined, 16, "the denominator: all sixteen were looked up");
}

/// The player-action handler reads the event's start flag **nowhere**, so a release event
/// runs the cycle exactly as a press does.
///
/// Every shipped binding for these sixteen is `ToggleType::OneShot`, which emits only a `start:
/// true` — so this is unreachable on the shipped keymap and reachable the moment a player rebinds
/// one as a hold. It is asserted because the alternative (a `if e.start` gate, as the two
/// neighbouring arms in this function have) is invisible in a diff and would be a deviation.
#[test]
fn a_release_event_runs_the_cycle_because_the_action_handler_never_reads_the_start_flag() {
    let mut b = three(Bench::make_monster);
    b.now += 1.0;
    let e = dereth_client_runtime::actions::Action {
        id: ActionId(ia::SELECTION_CLOSEST_MONSTER),
        phase: dereth_client_runtime::actions::ActionPhase::End,
        extent: 0.0,
        repeats: 0,
    };
    let (_, left) = dereth_client_runtime::interaction::use_time(
        &mut b.inter,
        &b.store,
        Some(&b.scene),
        &mut b.objects,
        None,
        vec![e],
        false,
        (800, 600),
        LocalTime(b.now),
    );
    assert!(left.is_empty(), "the release is consumed too");
    assert_eq!(b.selected(), Some(NEAR));
    assert_eq!(b.inter.stats.selection_cycles, 1);
}

/// The cell the body stands in picks the radius, exactly as it does for the object-range checks —
/// so a selection 40 m away is a candidate outdoors and is not one indoors.
///
/// This is the one input the frame supplies that `select_next` cannot derive. Both arms in one
/// run, with the premise of each asserted.
///
/// The outside check reads the body's own cell id and treats it as outdoors when its low 16 bits
/// are less than `0x100`; the per-candidate radar-radius query then answers 75 or 25. The body
/// has to be *in* the room: `Character::teleport` places the body, resolving an interior id
/// through visible child cells and, when the point is in no visible cell and the room is marked
/// `seen_outside`, adjusting it to the land cell the building stands in. So the indoor arm stands
/// the body at a point the room's own `cell_bsp` says is inside, and places a second monster at
/// the *same* 40 m player-space offset from there.
#[test]
fn the_players_own_cell_picks_the_radius_the_cycle_gates_at() {
    /// The 40 m monster of the indoor arm, at the same offset from the indoor body as `FAR` is
    /// from the outdoor one.
    const FAR_INDOORS: ObjectId = ObjectId(0x8000_0014);
    /// 3/4/5 scaled to 40 m: inside the outdoor 75, outside the indoor 25.
    const FORTY_M: (f32, f32, f32) = (24.0, 32.0, 0.0);

    let mut b = Bench::new();
    b.place(FAR, FORTY_M, 0);
    b.make_monster(FAR);
    assert!(
        dereth_physics::landdefs::is_outdoors(b.player.cell),
        "premise: outdoors, so the radius is 75.0"
    );
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(FAR), "40 m is inside the outdoor 75");

    // Move the body into a real resident interior cell of the same landblock, at a point inside it,
    // and re-drive against a monster at the same offset. Only the cell the *radius* is chosen
    // from changes between the two presses.
    let (indoor, inside) = a_room(&b.scene);
    assert!(
        !dereth_physics::landdefs::is_outdoors(indoor),
        "premise: {indoor:?} is an interior"
    );
    b.scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(indoor, Frame::new(inside, Quat::IDENTITY)));
    b.player = b.scene.character.as_ref().expect("a body").position();
    assert_eq!(
        b.player.cell, indoor,
        "the placement kept the room the point is inside"
    );
    b.place(FAR_INDOORS, FORTY_M, 0);
    b.make_monster(FAR_INDOORS);
    // The outdoor arm's monster is still in the world; the premise that it is not what an
    // indoor 25 would admit is asserted rather than assumed, so that a `None` below can only be
    // the radius.
    let far_2d = {
        let (x, y, _) = SceneSelectionPhysics::new(Some(&b.player), &b.objects)
            .get(FAR)
            .expect("the outdoor monster is still measurable")
            .player_space;
        math::hypotf(x, y)
    };
    assert!(
        far_2d > 25.0,
        "premise: {FAR:?} is {far_2d:.1} m from the room, outside the indoor 25"
    );
    b.select(None);
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(
        b.selected(),
        None,
        "the same 40 m offset is outside the indoor 25 — the radius followed the body"
    );
}

/// The first resident interior cell of the scene's own landblock — a **real loaded cell** rather
/// than an index chosen to satisfy `is_outdoors` — **and a point inside it**, from the cell's own
/// `cell_bsp` through `WorldScene::standable_point`. A cell with no standable point is skipped,
/// because placement would put the body outside.
fn a_room(scene: &WorldScene) -> (CellId, Vec3) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let block = scene.character.as_ref().expect("a body").position().cell.0 >> 16;
    for index in 0x0100_u32..0x0200 {
        let id = CellId((block << 16) | index);
        if land.env_cell(id).is_none() {
            continue;
        }
        if let Some(inside) = scene.standable_point(id) {
            return (id, inside);
        }
    }
    panic!("landblock {block:#06X}: no resident interior cell in 0x100..0x200 is standable");
}
