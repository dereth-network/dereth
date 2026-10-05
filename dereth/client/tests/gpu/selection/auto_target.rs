//! Auto-target re-selects your last attacker inside fifteen seconds and otherwise the closest
//! compass item; the select-last-attacker action works inside an exclusive radar range; Examine
//! and Use also run on a release event. The defender notifications `0x01B2` and `0x01B4` stamp
//! the last-attacked clock and run auto-targeting behind their own gates, and a deselect's
//! selection-changed notice is the one path that reaches the 15-second fallback and aborts an
//! automatic attack. Fixture: a WARP D3D12 device, a `WorldScene` with the attached body
//! outdoors, objects created through `0xF745` at player-space offsets, and actions and
//! notifications handed to `interaction::apply_events` and `interaction::use_time` (the slots
//! `App::frame` calls); no window, no injected input.
//! Every station is on a **non-cardinal** 3/4/5 heading, so an axis-only transform cannot satisfy
//! the geometry checks.
//!
//! # The three arms, each asserted on its own
//!
//! 1. **Auto-target** re-selects the thing that last hit you, or falls back to the closest
//!    compass item. Its fallback tuple is `(1, 1, COMPASS_ITEM, 0)`, and its production caller is
//!    the combat-mode-change tail.
//! 2. **Select last attacker**: one match arm on `SELECTION_LAST_ATTACKER 0x10000038`, at the
//!    **exclusive** boundary, which this consumer reaches without the radar cull's `- 1`.
//! 3. **Examine and Use have no start gate.** Both actions belong to the UI-action handler, which
//!    reads the event-start flag **once** in the whole function and not in either arm.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_client_model::combat::CombatMode;
use dereth_client_model::qualities::{StatKey, StatType, StatValue};
use dereth_client_model::range::RADAR_RADIUS_OUTDOORS;
use dereth_client_model::selection::LAST_ATTACKER_IID;
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::PLAYER_OBJECT_ID;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::selection_geometry::SceneSelectionPhysics;
use dereth_dat::RetailDatStore;
use dereth_input::{ActionId, InputMapId};
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};
use {
    dereth_client_runtime::interaction::action as ia,
    dereth_client_runtime::interaction::Interaction,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The map the ids are dispatched in. `on_actions` does not read it.
const MAP: InputMapId = InputMapId(0x1000_0007);

/// `PLAYER_OPTIONS[13] = ("AutoTarget", ...)` — the character's auto-target option.
const AUTO_TARGET_OPTION: usize = dereth_client_model::player::options::option::AUTO_TARGET;

// =================================================================================================
// Bench
// =================================================================================================

struct Bench {
    _gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    inter: Interaction,
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
        let region =
            dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
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
            "the premise: the body is outdoors, so every radius below is the outdoor 75.0"
        );

        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        me.qualities = Some(dereth_client_model::qualities::Qualities::new());
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

    /// Place an object at an exact **player-space** offset through the client's own `0xF745`, and
    /// assert it landed where the station asked — the premise, without which a filter that refused
    /// it and a placement that missed read alike.
    fn place(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};

        let v = Vec3::new(offset.0, offset.1, offset.2);
        let origin = dereth_physics::math::localtoglobal(&self.player.frame, v);
        let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id,
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield: flags::POSITION,
                    state: 0,
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

        let measured = self
            .phys()
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

    fn phys(&self) -> SceneSelectionPhysics {
        SceneSelectionPhysics::new(Some(&self.player), &self.objects)
    }

    fn weenie_mut(&mut self, id: ObjectId) -> &mut dereth_client_model::Weenie {
        self.objects
            .world
            .tables
            .weenies
            .get_mut(id)
            .expect("placed")
    }

    /// The compass-item selection kind's first gate: whether the object is shown on the radar.
    fn make_compass_item(&mut self, id: ObjectId) {
        self.weenie_mut(id).pwd.radar_enum = Some(4); // ShowAlways
    }

    /// The compass-item selection kind as the **melee and missile** stances see it: past the
    /// radar-visibility gate *and* past the attackability half, which the stance comparison
    /// only runs in those two modes. A plain radar-visible object is accepted while
    /// casting and **rejected** in melee, so a station built for one stance is not a station for
    /// the other -- which is what
    /// [`the_compass_item_stance_gate_admits_a_vendor_while_casting`] is about.
    fn make_attackable_compass_item(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.radar_enum = Some(4); // ShowAlways
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        assert!(
            self.objects.world.object_is_attackable(id),
            "premise: {id:?} is attackable"
        );
    }

    /// A compass item that is also **attackable** and a **vendor** — the one station that
    /// distinguishes the three stances, because the vendor test lives behind the stance compare.
    fn make_attackable_vendor(&mut self, id: ObjectId) {
        let w = self.weenie_mut(id);
        w.pwd.radar_enum = Some(4);
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE | bitfield::VENDOR;
        assert!(
            self.objects.world.object_is_attackable(id),
            "premise: {id:?} is attackable"
        );
    }

    fn set_last_attacker(&mut self, id: ObjectId) {
        self.objects
            .world
            .player_qualities_mut()
            .expect("the player description")
            .set(
                StatKey::new(StatType::Iid, LAST_ATTACKER_IID),
                StatValue::Iid(id),
            );
    }

    /// One event through the production frame slot. `start` is the parameter, because the
    /// Examine and Use arms are entirely about the release event.
    fn deliver(&mut self, action: u32, start: bool) -> usize {
        let e = dereth_client_runtime::actions::Action {
            id: ActionId(action),
            phase: if start {
                dereth_client_runtime::actions::ActionPhase::Begin
            } else {
                dereth_client_runtime::actions::ActionPhase::End
            },
            extent: 1.0,
            repeats: 0,
        };
        self.frame(vec![e])
    }

    fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>) -> usize {
        self.now += 1.0;
        let (unowned, left) = dereth_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            Some(&self.scene),
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(self.now),
        );
        assert!(unowned.is_empty(), "no unowned requests were expected");
        left.len()
    }

    /// Advance the actual motion queue rather than hand-writing Interaction's cached ready
    /// answer (which use_time would overwrite on the next frame).
    fn settle_body(&mut self) {
        let body = self.scene.character.as_mut().expect("the attached body");
        for i in 1..=60 {
            body.update(LocalTime(self.now + f64::from(i) / 30.0));
        }
        self.now += 2.0;
        assert!(
            !body.driver().movement.motions_pending(),
            "the motion queue must drain"
        );
        self.player = body.position();
    }

    fn modes_sent(&self) -> Vec<u32> {
        self.inter
            .last_sent
            .iter()
            .filter_map(|r| match r {
                dereth_client_model::Request::ChangeCombatMode(m) => Some(m.combat_mode),
                _ => None,
            })
            .collect()
    }

    fn press(&mut self, action: u32) {
        assert_eq!(
            self.deliver(action, true),
            0,
            "{action:#010X} must be consumed by `on_actions`"
        );
    }

    /// How many times the `USE` arm has reached object-use handling.
    ///
    /// `uses_undispatched` rather than a request on the wire: this bench has **no server**, so the
    /// `PlaceInBackpack` the arm resolves to is decided, counted and then not sent
    /// (`UseOutcome::Dispatched { sent: false }`). The counter is the arm's own and increments
    /// once per delivery, which is exactly the question this test asks.
    fn uses(&self) -> u64 {
        self.inter.stats.uses_undispatched
    }

    fn selected(&self) -> Option<ObjectId> {
        self.objects.world.selected
    }

    fn select(&mut self, id: Option<ObjectId>) {
        let mut out = dereth_client_model::RecordingSink::default();
        self.objects.world.set_selected_object(id, false, &mut out);
    }
}

// Two candidates on a 53.13° heading at 3 m and 9 m, plus a boundary pair at 74.83 m and 75.17 m
// (the 3/4/5 triple scaled to 75, moved 0.1 either way so the placement's own 1e-3 tolerance
// cannot decide the answer). The exact boundary is asserted separately, without the transform.
const NEAR: ObjectId = ObjectId(0x8100_0001);
const FAR: ObjectId = ObjectId(0x8100_0002);
const NEAR_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);
const FAR_AT: (f32, f32, f32) = (5.4, 7.2, 0.0);
const JUST_INSIDE: (f32, f32, f32) = (44.9, 59.9, 0.0);
const JUST_OUTSIDE: (f32, f32, f32) = (45.1, 60.1, 0.0);

// =================================================================================================
// 0. Calibration — the bench must be able to select and to refuse
// =================================================================================================

/// Every assertion below is either "it selected X" or "it selected nothing"; before either is
/// worth anything the bench has to be shown capable of both from the same gesture.
#[test]
fn the_bench_can_select_and_can_refuse() {
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.make_compass_item(NEAR);
    b.press(ia::SELECTION_CLOSEST_COMPASS_ITEM);
    assert_eq!(b.selected(), Some(NEAR), "the bench can select");

    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    // Not shown on the radar and not `COMPASS_ALWAYS`: the arm's first gate rejects it.
    b.press(ia::SELECTION_CLOSEST_COMPASS_ITEM);
    assert_eq!(b.selected(), None, "and it can refuse");
}

// =================================================================================================
// 1. Auto-target behavior
// =================================================================================================

/// **The fallback tuple, in all three stances.**
///
/// The client passes
/// `(closer = 1, ignoreCurrent = 1, kind = 2 = compass item, excludeOwnWielded =
/// 0)` — so auto-target is literally "select the closest compass item". Asserted at a station
/// where `ignoreCurrent = 1` matters: the **far** object is already selected, and only an arm that
/// ignores the current selection can move to the near one.
///
/// Three stances rather than two, because auto-targeting has **no mode gate of its own** — every one
/// of its four call sites carries one — and the `CompassItem` filter it runs does. A two-mode
/// station cannot see a stance-dependent filter change its answer; see
/// [`the_compass_item_stance_gate_admits_a_vendor_while_casting`] for the case where it does.
#[test]
fn auto_target_falls_back_to_the_closest_compass_item_in_every_stance() {
    for mode in [CombatMode::Melee, CombatMode::Missile, CombatMode::Magic] {
        let mut b = Bench::new();
        for (id, at) in [(NEAR, NEAR_AT), (FAR, FAR_AT)] {
            b.place(id, at);
            b.make_attackable_compass_item(id);
        }
        b.objects.world.combat.combat_mode = mode;
        assert_eq!(
            b.selected(),
            None,
            "the premise: nothing is selected, as at every call site"
        );

        let phys = b.phys();
        let lookup = |id| phys.get(id);
        let mut out = dereth_client_model::RecordingSink::default();
        b.objects
            .world
            .auto_target(&lookup, RADAR_RADIUS_OUTDOORS, LocalTime(b.now), &mut out);

        assert_eq!(
            b.selected(),
            Some(NEAR),
            "{mode:?}: `closer = 1` takes the nearer candidate; the call tuple is `(closer = 1, ignoreCurrent = 1, kind = 2, excludeOwnWielded = 0)`"
        );
    }
}

/// Behaviour: selection.auto-target.reselects-the-last-attacker-within-fifteen-seconds-else-the-closest-compass-item
///
/// **The other arm: a fresh last attacker is re-selected instead**, and the 15-second window is
/// asserted on **both** sides of its boundary.
///
/// The client compares the elapsed time against `15.0` and falls back exactly when it is
/// **not less than** 15.
/// At exactly 15.0 the attacker is *not* re-selected: the condition is `15.0 <= cur - last`.
///
/// **This test writes `last_attacked_time` directly.** It is the *boundary* station: 14.9 / 15.0
/// / 15.1 need three different elapsed times against one comparison, and the production writer
/// (the tail shared by both defender-notification handlers) stamps the current time
/// **immediately above** the auto-target call — so a notification can only ever produce an
/// elapsed time of zero, and can never drive the far side of the boundary at all. The
/// defender-notification tests re-take the same three points with the stamp written by the
/// real producer.
#[test]
fn auto_target_reselects_a_last_attacker_inside_fifteen_seconds_and_not_at_fifteen() {
    for (elapsed, expected, why) in [
        (
            14.9_f64,
            ATTACKER,
            "inside the window, so the attacker is re-selected",
        ),
        (
            15.0_f64,
            NEAR,
            "at exactly 15.0 the jump to the fallback is taken",
        ),
        (15.1_f64, NEAR, "and beyond it"),
    ] {
        let mut b = Bench::new();
        b.place(NEAR, NEAR_AT);
        b.make_attackable_compass_item(NEAR);
        b.place(ATTACKER, FAR_AT);
        b.make_attackable_compass_item(ATTACKER);
        b.set_last_attacker(ATTACKER);
        b.objects.world.combat.combat_mode = CombatMode::Melee;
        b.objects.world.combat.last_attacked_time = b.now - elapsed;

        let phys = b.phys();
        let lookup = |id| phys.get(id);
        let mut out = dereth_client_model::RecordingSink::default();
        b.objects
            .world
            .auto_target(&lookup, RADAR_RADIUS_OUTDOORS, LocalTime(b.now), &mut out);
        assert_eq!(b.selected(), Some(expected), "{elapsed} s: {why}");
    }
}

/// **The removal flag and "not in the tables" both take the fallback** — two separate gates
/// that a single "the attacker is gone" station would collapse into one.
#[test]
fn auto_target_falls_back_when_the_attacker_is_gone_or_being_removed() {
    // (a) named, fresh, and not in the weenie tables at all.
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.objects.world.combat.combat_mode = CombatMode::Melee;
    b.set_last_attacker(ObjectId(0x8100_00FF));
    b.objects.world.combat.last_attacked_time = b.now;
    let phys = b.phys();
    let lookup = |id| phys.get(id);
    let mut out = dereth_client_model::RecordingSink::default();
    b.objects
        .world
        .auto_target(&lookup, RADAR_RADIUS_OUTDOORS, LocalTime(b.now), &mut out);
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "the object lookup returned null -> the fallback"
    );

    // (b) present and fresh, but flagged for removal — the control for (a), differing in one
    // field, so "the fallback ran" cannot be the fixture failing to create anything.
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.place(ATTACKER, FAR_AT);
    b.make_attackable_compass_item(ATTACKER);
    b.objects.world.combat.combat_mode = CombatMode::Melee;
    b.set_last_attacker(ATTACKER);
    b.objects.world.combat.last_attacked_time = b.now;
    let phys = b.phys();
    let lookup = |id| phys.get(id);
    let mut out = dereth_client_model::RecordingSink::default();
    b.objects
        .world
        .auto_target(&lookup, RADAR_RADIUS_OUTDOORS, LocalTime(b.now), &mut out);
    assert_eq!(
        b.selected(),
        Some(ATTACKER),
        "the control: it re-selects him"
    );

    b.weenie_mut(ATTACKER).being_removed = true;
    b.select(None);
    let phys = b.phys();
    let lookup = |id| phys.get(id);
    b.objects
        .world
        .auto_target(&lookup, RADAR_RADIUS_OUTDOORS, LocalTime(b.now), &mut out);
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "an attacker being removed -> the fallback"
    );
}

/// **The stance gate is where casting differs, and it is the only station that shows it.**
///
/// The stance gate compares `combatMode` with `2` and then `4`, and everything else takes the
/// accept path — so the whole attackability half of the `CompassItem` filter (attackable, not a
/// fellow, **not a vendor**, not `REPORT_COLLISIONS_AS_ENVIRONMENT_PS`) runs **only** in melee and
/// missile. A vendor refused in melee is therefore accepted while casting.
///
/// `COMBAT_MODE`'s members are `1/2/4/8` and `dereth_client_model::combat::CombatMode` cannot
/// hold a union, so a mode compare (`== 2 || == 4`) and a mask test (`& 6 != 0`) give the
/// **same** answer for every value the field can take, casting included. The behaviour asserted
/// below is what a two-mode station cannot see; the mode-versus-mask distinction is not
/// observable without a value the enum forbids.
///
/// Driven through the wire (`0x1000002F SelectionClosestCompassItem`), because unlike
/// auto-targeting — whose four call sites are *all* gated on melee or missile, so it is unreachable
/// while casting in retail — the three `Selection*CompassItem` actions reach this filter in any
/// stance.
#[test]
fn the_compass_item_stance_gate_admits_a_vendor_while_casting() {
    let mut answers = Vec::new();
    for mode in [CombatMode::Melee, CombatMode::Missile, CombatMode::Magic] {
        let mut b = Bench::new();
        b.place(NEAR, NEAR_AT);
        b.make_attackable_vendor(NEAR);
        b.objects.world.combat.combat_mode = mode;
        b.press(ia::SELECTION_CLOSEST_COMPASS_ITEM);
        answers.push((mode, b.selected()));
    }
    assert_eq!(
        answers,
        vec![
            (CombatMode::Melee, None),
            (CombatMode::Missile, None),
            (CombatMode::Magic, Some(NEAR)),
        ],
        "the vendor test lives behind the stance compare, so it does not run while casting"
    );
}

/// **The combat-mode-change tail, one gate at a time.**
///
/// Four stations, because the block has three gates above the call and three arms asserted in
/// aggregate would pass with two dead:
///
/// * the mode test, which jumps past this block as well as past the selection fixup — so
///   auto-targeting does **not** run on entering magic or peace;
/// * the character's auto-target option;
/// * the attack-target lookup plus attackability check, which *keeps* a live
///   target instead of re-targeting;
/// * and the call itself.
///
/// # What this direct-tail test does not reach
///
/// The last link — `Interaction::run_combat_mode_toggle` calling this immediately after
/// `set_combat_mode` — is **not** asserted here. The combat-mode change's readiness gate requires
/// `!motions_pending` in every mode, and a frame that does not step the body's movement reads
/// `motions_pending() == true` from the frame the body is attached onwards, so a toggle stays
/// **queued** and the mode never moves. The settled-body pending-combat retry tests below drive
/// the production interaction frame and cover that end-to-end link.
#[test]
fn the_set_combat_mode_tail_runs_auto_target_and_every_gate_above_it_can_refuse() {
    // (a) the whole ladder open: melee, option on, nothing worth keeping selected.
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.objects.world.combat.combat_mode = CombatMode::Melee;
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_TARGET_OPTION, true);
    assert!(
        b.objects.world.player_system.options.auto_target(),
        "the premise: the character option supplies the gate read here"
    );
    let phys = b.phys();
    let lookup = |id| phys.get(id);
    let mut out = dereth_client_model::RecordingSink::default();
    assert!(
        b.objects.world.combat_mode_auto_target(
            &lookup,
            RADAR_RADIUS_OUTDOORS,
            LocalTime(b.now),
            &mut out
        ),
        "the tail ran"
    );
    assert_eq!(b.selected(), Some(NEAR), "and it auto-targeted");

    // (b) the option off. One field differs from (a).
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.objects.world.combat.combat_mode = CombatMode::Melee;
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_TARGET_OPTION, false);
    let phys = b.phys();
    let lookup = |id| phys.get(id);
    assert!(
        !b.objects.world.combat_mode_auto_target(
            &lookup,
            RADAR_RADIUS_OUTDOORS,
            LocalTime(b.now),
            &mut out
        ),
        "the option gate refused it"
    );
    assert_eq!(b.selected(), None);

    // (c) the mode gate: magic and peace jump past the whole block.
    for mode in [CombatMode::Magic, CombatMode::NonCombat] {
        let mut b = Bench::new();
        b.place(NEAR, NEAR_AT);
        b.make_attackable_compass_item(NEAR);
        b.objects.world.combat.combat_mode = mode;
        b.objects
            .world
            .player_system
            .options
            .set(AUTO_TARGET_OPTION, true);
        let phys = b.phys();
        let lookup = |id| phys.get(id);
        assert!(
            !b.objects.world.combat_mode_auto_target(
                &lookup,
                RADAR_RADIUS_OUTDOORS,
                LocalTime(b.now),
                &mut out
            ),
            "{mode:?}: the combat-mode equality branch skips both auto-target and its fixup"
        );
        assert_eq!(b.selected(), None, "{mode:?}");
    }

    // (d) a live attackable target is kept by the attackability gate.
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.place(FAR, FAR_AT);
    b.make_attackable_compass_item(FAR);
    b.objects.world.combat.combat_mode = CombatMode::Melee;
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_TARGET_OPTION, true);
    b.select(Some(FAR));
    let phys = b.phys();
    let lookup = |id| phys.get(id);
    assert!(
        !b.objects.world.combat_mode_auto_target(
            &lookup,
            RADAR_RADIUS_OUTDOORS,
            LocalTime(b.now),
            &mut out
        ),
        "the current attack target remains selected while it is attackable rather than re-targeted"
    );
    assert_eq!(b.selected(), Some(FAR), "and the selection is untouched");
}

/// Oracle: the per-frame interaction retry attempts the combat-mode change before clearing the
/// pending mode; auto-targeting is reached after an actual mode change.
/// Unlike the direct-tail test above, this drives the production interaction frame, including
/// a toggle while busy and a later frame with a genuinely settled Character.
#[test]
fn a_pending_combat_retry_auto_targets_on_the_first_ready_frame_only() {
    let mut b = Bench::new();
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_TARGET_OPTION, true);
    // The object-range-exit handler may otherwise clear a selection because this
    // bench never renders a frame. Visibility is a premise here, not a culling assertion.
    b.objects.world.selected_object_in_view = true;
    assert!(b
        .scene
        .character
        .as_ref()
        .unwrap()
        .driver()
        .movement
        .motions_pending());
    assert_eq!(b.objects.world.combat.combat_mode, CombatMode::NonCombat);

    b.press(ia::COMBAT_TOGGLE_COMBAT);
    assert_eq!(
        b.objects.world.combat.pending_combat_mode,
        CombatMode::Melee
    );
    for _ in 0..3 {
        assert_eq!(b.frame(Vec::new()), 0);
    }
    assert_eq!(b.objects.world.combat.combat_mode, CombatMode::NonCombat);
    assert_eq!(
        b.inter.stats.auto_targets, 0,
        "not-ready frames never enter the tail"
    );
    assert!(
        b.modes_sent().is_empty(),
        "the queued toggle is not sent early"
    );

    b.settle_body();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.place(FAR, FAR_AT);
    b.make_attackable_compass_item(FAR);
    assert_eq!(b.frame(Vec::new()), 0);
    assert_eq!(b.objects.world.combat.combat_mode, CombatMode::Melee);
    assert_eq!(
        b.objects.world.combat.pending_combat_mode,
        CombatMode::Undef
    );
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "the retry must select the closest compass target"
    );
    assert_eq!(b.inter.stats.auto_targets, 1);
    assert_eq!(
        b.modes_sent(),
        vec![2],
        "one local ChangeCombatMode request"
    );
    for _ in 0..3 {
        assert_eq!(b.frame(Vec::new()), 0);
    }
    assert_eq!(b.selected(), Some(NEAR));
    assert_eq!(b.inter.stats.auto_targets, 1, "the retry is one-shot");
    assert!(
        b.modes_sent().is_empty(),
        "later frames emit no mode request"
    );
    assert_eq!(
        b.inter.stats.requests_undeliverable, 1,
        "one request, no live session"
    );
}

/// Oracle: the combat-mode equality return precedes the selection/auto-target tail; the per-frame
/// retry still clears that pending request. No pending request also means no tail.
#[test]
fn a_pending_combat_noop_and_an_empty_retry_do_not_auto_target() {
    let mut b = Bench::new();
    b.settle_body();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_TARGET_OPTION, true);
    // The melee readiness arm reads the player's combat-table data id.
    b.objects.world.player_qualities_mut().unwrap().set(
        StatKey::new(StatType::Did, dereth_client_model::combat::COMBAT_TABLE_DID),
        StatValue::Did(dereth_primitives::DataId(0x3000_0000)),
    );
    b.objects.world.combat.combat_mode = CombatMode::Melee;
    b.objects.world.combat.pending_combat_mode = CombatMode::Melee;
    assert!(
        b.objects.world.player_in_ready_position(false, Some(false)),
        "ready premise"
    );
    for _ in 0..2 {
        assert_eq!(b.frame(Vec::new()), 0);
        assert_eq!(
            b.objects.world.combat.pending_combat_mode,
            CombatMode::Undef
        );
        assert_eq!(
            b.selected(),
            None,
            "an equality return is not a mode-change tail"
        );
        assert_eq!(b.inter.stats.auto_targets, 0);
        assert!(b.modes_sent().is_empty());
    }
}

/// Oracle: the per-frame retry clears the pending mode even when combat-mode compatibility
/// refuses it. The refusal and missing tail must be one-shot, just like a successful retry.
#[test]
fn an_incompatible_pending_combat_retry_is_dropped_without_auto_target() {
    let mut b = Bench::new();
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_TARGET_OPTION, true);
    b.press(ia::COMBAT_TOGGLE_COMBAT);
    assert_eq!(
        b.objects.world.combat.pending_combat_mode,
        CombatMode::Melee
    );
    b.settle_body();
    b.place(NEAR, NEAR_AT);
    b.make_attackable_compass_item(NEAR);
    // The equipment changes while busy. Combat-mode compatibility rejects melee with
    // only a missile weapon in the ready slot. This tests the existing refusal, not its text.
    b.objects.world.inventory_mask = dereth_client_model::inventory::slots::loc::MISSILE_WEAPON;
    assert!(!b.objects.world.compatible_combat_mode(CombatMode::Melee));
    assert_eq!(b.frame(Vec::new()), 0);
    assert_eq!(b.objects.world.combat.combat_mode, CombatMode::NonCombat);
    assert_eq!(
        b.objects.world.combat.pending_combat_mode,
        CombatMode::Undef
    );
    assert_eq!(b.inter.stats.requests_refused, 1);
    assert!(b.inter.last_refusal.is_some());
    assert_eq!(b.frame(Vec::new()), 0);
    assert_eq!(
        b.inter.stats.requests_refused, 1,
        "the refusal is not repeated"
    );
    assert_eq!(b.inter.stats.auto_targets, 0);
    assert_eq!(b.selected(), None);
    assert!(b.modes_sent().is_empty());
}

// =================================================================================================
// 2. Select last attacker — one arm, and the exclusive boundary
// =================================================================================================

const ATTACKER: ObjectId = ObjectId(0x8100_0003);

/// **The arm exists and reaches `select_last_attacker`.**
///
/// The player-action handler's case `0xD` is the only caller of this radar-range check.
#[test]
fn the_last_attacker_action_selects_him() {
    let mut b = Bench::new();
    b.place(ATTACKER, NEAR_AT);
    b.set_last_attacker(ATTACKER);
    assert_eq!(b.selected(), None, "the premise: nothing is selected");

    b.press(ia::SELECTION_LAST_ATTACKER);

    assert_eq!(b.selected(), Some(ATTACKER));
    assert_eq!(b.inter.stats.selection_last_attacker, 1, "the arm ran once");
    assert_eq!(
        b.inter.stats.selection_cycles, 0,
        "and it is not a selection cycle"
    );
}

/// **Both sides of the boundary, through the wire.**
///
/// The margin is 0.1 m either side of the exact 75.0 because the station is placed by a frame
/// transform whose round trip is asserted only to 1e-3; the **exact** boundary is asserted
/// without the transform in the next test. `(45, 60)` is the 3/4/5 triple scaled to 75, so
/// neither station is on an axis.
#[test]
fn the_last_attacker_arm_carries_the_radar_range_on_both_sides() {
    for (at, expected, why) in [
        (
            JUST_INSIDE,
            Some(ATTACKER),
            "74.83 m is inside the outdoor 75.0",
        ),
        (JUST_OUTSIDE, None, "75.17 m is outside it"),
    ] {
        let mut b = Bench::new();
        b.place(ATTACKER, at);
        b.set_last_attacker(ATTACKER);
        b.press(ia::SELECTION_LAST_ATTACKER);
        assert_eq!(b.selected(), expected, "{why}");
        assert_eq!(
            b.inter.stats.selection_last_attacker, 1,
            "the arm ran in both cases"
        );
    }
}

/// **The boundary itself is exclusive, and this consumer carries no `- 1`.**
///
/// The comparison refuses `d == r` as well as `d > r` — where the selection cycle's comparison
/// accepts the equal case and the radar's own blip cull subtracts 1.0 first.
///
/// Asserted at the *function*, with exact `f32` inputs, because a station placed through the
/// body's frame cannot be put on an exact float boundary. `(45, 60)` is exact in `f32` and
/// `sqrt(45² + 60²)` is exactly 75.
#[test]
fn the_boundary_is_exclusive_and_has_no_minus_one() {
    let mut b = Bench::new();
    b.place(ATTACKER, NEAR_AT);
    b.set_last_attacker(ATTACKER);
    let mut out = dereth_client_model::RecordingSink::default();

    assert!(b.objects.world.select_last_attacker(
        Some((45.0, 60.0)),
        RADAR_RADIUS_OUTDOORS,
        &mut out
    ));
    assert_eq!(b.selected(), None, "d == r is refused, as well as d > r");

    // The largest `f32` below 45.0 on the x leg: still 74.999… and therefore inside.
    assert!(b.objects.world.select_last_attacker(
        Some((f32::from_bits(45.0_f32.to_bits() - 1), 60.0)),
        RADAR_RADIUS_OUTDOORS,
        &mut out
    ));
    assert_eq!(
        b.selected(),
        Some(ATTACKER),
        "and one ulp inside it is accepted"
    );

    // And the `- 1` the radar cull applies is not applied here: at 74.5 m an object is selectable
    // as your last attacker while the radar has already stopped drawing it.
    b.select(None);
    assert!(b.objects.world.select_last_attacker(
        Some((44.7, 59.6)),
        RADAR_RADIUS_OUTDOORS,
        &mut out
    ));
    assert_eq!(
        b.selected(),
        Some(ATTACKER),
        "74.5 m: inside 75.0, outside the cull's 74.0"
    );
}

// =================================================================================================
// 3. Examine and Use have no start gate
// =================================================================================================

/// **Measured on the shipped data: a release is never issued for Use or Examine.**
///
/// The input manager issues a release event **only** for the three hold families, so on the
/// shipped keymap a start gate on these arms would be inert. That is a claim about the shipped
/// `ActionMap`, and this reads it. The denominator is asserted so a lookup that examined nothing
/// cannot read as two passes.
#[test]
fn neither_shipped_binding_for_use_or_examine_is_a_hold() {
    let store = dereth_dat::testing::open_store().expect("the shipped ActionMap is in the dats");
    let shell =
        dereth_client_shell::input::InputShell::new(&store, None).expect("the input tables decode");
    let mut examined = 0;
    for a in [ia::USE, ia::SELECTION_EXAMINE] {
        let t = shell.manager.action_map.toggle_type(MAP, ActionId(a));
        assert!(
            !t.is_hold(),
            "{a:#010X} is bound as {t:?}, but must not use hold/release toggle semantics"
        );
        examined += 1;
    }
    assert_eq!(examined, 2, "the denominator: both ids were looked up");
}

/// **A release event runs both arms, as the UI-action handler does.**
///
/// Both directions in one test per arm: the press is the control, so "the release did something"
/// cannot be the whole arm being dead. `SELECTION_EXAMINE` is observed at the game world's
/// `appraisal.examining` field, the state armed by the examine notice and consumed when
/// appraisal information arrives, rather than at the request alone.
#[test]
fn a_release_event_runs_the_examine_arm() {
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    b.select(Some(NEAR));

    assert_eq!(
        b.deliver(ia::SELECTION_EXAMINE, true),
        0,
        "the press is consumed"
    );
    assert_eq!(
        b.objects.world.appraisal.examining,
        Some(NEAR),
        "the control: the press examines"
    );

    b.objects.world.appraisal.examining = None;
    assert_eq!(
        b.deliver(ia::SELECTION_EXAMINE, false),
        0,
        "the release is consumed too"
    );
    assert_eq!(
        b.objects.world.appraisal.examining,
        Some(NEAR),
        "and it runs the examine action `0x1000002B`, which ignores the start/release flag"
    );
}

/// The same for `USE` (`0x10000025`), whose arm invokes object use as `(selected object, 0, 0)`.
///
/// Observed at the arm's own counter, because this bench has no server and the request it decides
/// on is therefore counted rather than sent. The press is the control, so "the release did
/// something" cannot be the whole arm being dead.
#[test]
fn a_release_event_runs_the_use_arm() {
    let mut b = Bench::new();
    b.place(NEAR, NEAR_AT);
    // Useable -- the arm that ends in a use request rather than in a refusal.
    b.weenie_mut(NEAR).pwd.useability = Some(0x0000_0080);
    b.select(Some(NEAR));

    assert_eq!(b.deliver(ia::USE, true), 0, "the press is consumed");
    assert_eq!(b.uses(), 1, "the control: the press runs the use action");

    assert_eq!(b.deliver(ia::USE, false), 0, "the release is consumed too");
    assert_eq!(
        b.uses(),
        2,
        "and the use action `0x10000025` runs as well because it also ignores the start/release flag"
    );
}

/// A defender notification stamps the last-attacked clock and runs automatic targeting, which
/// re-selects the last attacker. Both handlers, standard `0x01B2` and evasion `0x01B4`, are
/// asserted separately: the stamp is unconditional (but an undecodable body stamps nothing), only
/// melee and missile auto-target, and the option and any existing selection gate the arm. Every
/// notification is a real encoded body pushed through `interaction::apply_events` and then
/// `interaction::use_time`, in `App::frame`'s order.
mod defender_notification {
    //! # What the tail is
    //!
    //! Both handlers end in the same shared tail, the `dereth-client-model` world's
    //! `defender_notification_auto_target` method: stamp the hit, then conditionally auto-target. A
    //! test that drives only `0x01B2` would pass with `0x01B4`'s arm missing, because the part they
    //! share is the part that works; so every test drives both. Four facts, each with a test below:
    //!
    //! * the stamp is **unconditional** — it is not under the squelch gate, and not under the mode
    //!   gate either ([`the_stamp_is_unconditional_even_when_the_gate_below_it_refuses`]);
    //! * the mode gate is a **compare** against modes 2 and 4
    //!   ([`only_melee_and_missile_auto_target`]);
    //! * the character's automatic-target option is bit 13
    //!   ([`the_auto_target_option_gates_the_arm`]);
    //! * the selection gate is `selected_id == 0` — a **different** gate from the mode-change
    //!   path's attack-target lookup plus attackability test
    //!   ([`anything_selected_at_all_blocks_the_arm_here`]).
    //!
    //! # The window never expires inside a notification
    //!
    //! The stamp is **immediately above** the automatic-target call, so a defender notification
    //! always reaches the re-select arm with an elapsed time of **zero**. The fallback leg of the
    //! 15-second window is reachable only from automatic targeting's other two call sites (mode
    //! change and selection change), some seconds after the last hit. The window test therefore
    //! stamps through a notification and then calls the world's target-selection routine directly
    //! at a later time.

    use dereth_scene::world_scene::SceneWrites;
    use std::sync::Arc;

    use dereth_client_model::combat::CombatMode;
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};
    use dereth_client_model::range::RADAR_RADIUS_OUTDOORS;
    use dereth_client_model::selection::LAST_ATTACKER_IID;
    use dereth_client_model::weenie::{bitfield, item_type};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::character::PLAYER_OBJECT_ID;
    use dereth_client_runtime::interaction::Interaction;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_client_runtime::selection_geometry::SceneSelectionPhysics;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::Message;
    use dereth_render::device::{DeviceConfig, Gpu};
    use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

    /// `PLAYER_OPTIONS[13] = ("AutoTarget", ...)` — the option read by automatic targeting.
    const AUTO_TARGET_OPTION: usize = dereth_client_model::player::options::option::AUTO_TARGET;

    /// The thing that hit you, stored in instance-id quality `0x0B` (`LAST_ATTACKER_IID`).
    const ATTACKER: ObjectId = ObjectId(0x8100_0011);
    /// The fallback's winner: the closest compass item.
    const NEAR: ObjectId = ObjectId(0x8100_0001);
    const FAR: ObjectId = ObjectId(0x8100_0002);
    const NEAR_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);
    const FAR_AT: (f32, f32, f32) = (5.4, 7.2, 0.0);
    /// The attacker sits further out than both, so "the fallback ran" and "the attacker was
    /// re-selected" can never be the same answer.
    const ATTACKER_AT: (f32, f32, f32) = (9.0, 12.0, 0.0);

    // =============================================================================================
    // Which handler is under test. Every test that drives one drives BOTH, in its own loop, and
    // names the one it is on in every failure message.
    // =============================================================================================

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Handler {
        /// `0x01B2` — the standard defender notification.
        Defender,
        /// `0x01B4` — the evasion defender notification.
        Evasion,
    }

    const BOTH: [Handler; 2] = [Handler::Defender, Handler::Evasion];

    impl Handler {
        fn event(self) -> SessionEvent {
            let (op, body) = match self {
                Self::Defender => {
                    let m = dereth_protocol::combat::DefenderNotification {
                        attacker_name: "Drudge Slave".into(),
                        damage_type: 0x04,
                        percent: 0.125,
                        damage: 7,
                        damage_location: 3,
                        critical: 0,
                        attack_conditions: 0,
                        attack_conditions_high: 0,
                    };
                    (
                        dereth_protocol::combat::DefenderNotification::OPCODE,
                        dereth_protocol::write_body(&m).expect("encode 0x01B2"),
                    )
                }
                Self::Evasion => {
                    let m = dereth_protocol::combat::EvasionDefenderNotification {
                        attacker_name: "Drudge Slave".into(),
                    };
                    (
                        dereth_protocol::combat::EvasionDefenderNotification::OPCODE,
                        dereth_protocol::write_body(&m).expect("encode 0x01B4"),
                    )
                }
            };
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend_from_slice(&body);
            SessionEvent::UiEvent { opcode: op, blob }
        }

        /// A body that cannot decode: the opcode is right and the payload is one byte of a
        /// `pstring` length that promises more than is there. Used as the negative control.
        fn corrupt_event(self) -> SessionEvent {
            let op = match self {
                Self::Defender => dereth_protocol::combat::DefenderNotification::OPCODE,
                Self::Evasion => dereth_protocol::combat::EvasionDefenderNotification::OPCODE,
            };
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend_from_slice(&[0xFF, 0x7F]);
            SessionEvent::UiEvent { opcode: op, blob }
        }
    }

    // =============================================================================================
    // Bench — the automatic-targeting bench, one step earlier: notifications enter before use_time.
    // =============================================================================================

    struct Bench {
        _gpu: Gpu,
        scene: WorldScene,
        store: Arc<RetailDatStore>,
        objects: ObjectStream,
        inter: Interaction,
        player: Position,
        now: f64,
    }

    impl Bench {
        fn new() -> Self {
            let store =
                Arc::new(dereth_dat::testing::open_store().expect(
                    "the retail dats are `use_time`'s own argument: set DERETH_TEST_DAT_DIR",
                ));
            let cfg = DeviceConfig {
                width: 800,
                height: 600,
                ..DeviceConfig::default()
            };
            let mut gpu = Gpu::new(None, &cfg).expect("a D3D12 WARP device");
            let region =
                dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
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
                "the premise: the body is outdoors, so the radius below is the outdoor 75.0"
            );

            let mut objects = ObjectStream::new();
            objects.world.player = Some(PLAYER_OBJECT_ID);
            let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
            me.valid = true;
            me.has_phys_obj = true;
            me.qualities = Some(dereth_client_model::qualities::Qualities::new());
            objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);

            let mut b = Self {
                _gpu: gpu,
                scene,
                store,
                objects,
                inter: Interaction::new(),
                player,
                now: 1.0,
            };
            // The state every automatic-target call site is reached in: a stance it runs in, the
            // option on, and nothing selected. Each of the three is taken away again by its own
            // test.
            b.objects.world.combat.combat_mode = CombatMode::Melee;
            b.objects
                .world
                .player_system
                .options
                .set(AUTO_TARGET_OPTION, true);
            assert!(
                b.objects.world.player_system.options.auto_target(),
                "premise: the character's automatic-target option is enabled"
            );
            b
        }

        /// Place an object at an exact player-space offset through the client's own `0xF745`, and
        /// assert it landed there — the premise, without which a refusal and a miss read alike.
        fn place(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
            use dereth_protocol::types::{
                physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc,
            };

            let v = Vec3::new(offset.0, offset.1, offset.2);
            let origin = dereth_physics::math::localtoglobal(&self.player.frame, v);
            let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
                dereth_protocol::objects::ObjectCreatePayload {
                    id,
                    objdesc: ObjDesc::default(),
                    physicsdesc: PhysicsDesc {
                        bitfield: flags::POSITION,
                        state: 0,
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
            let measured = self
                .phys()
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
                "the production visibility sweep must have made {id:?} visible"
            );
        }

        fn phys(&self) -> SceneSelectionPhysics {
            SceneSelectionPhysics::new(Some(&self.player), &self.objects)
        }

        /// A compass item as melee and missile targeting see it: showable on radar and past the
        /// attackability test added by the stance branch.
        fn make_attackable_compass_item(&mut self, id: ObjectId) {
            let w = self
                .objects
                .world
                .tables
                .weenies
                .get_mut(id)
                .expect("placed");
            w.pwd.radar_enum = Some(4); // ShowAlways
            w.pwd.obj_type |= item_type::CREATURE;
            w.pwd.bitfield |= bitfield::ATTACKABLE;
            assert!(
                self.objects.world.object_is_attackable(id),
                "premise: {id:?} is attackable"
            );
        }

        /// Instance-id quality `0x0B`, which automatic targeting reads as the last attacker.
        fn set_last_attacker(&mut self, id: ObjectId) {
            self.objects
                .world
                .player_qualities_mut()
                .expect("the player description")
                .set(
                    StatKey::new(StatType::Iid, LAST_ATTACKER_IID),
                    StatValue::Iid(id),
                );
        }

        fn select(&mut self, id: Option<ObjectId>) {
            let mut out = dereth_client_model::RecordingSink::default();
            self.objects.world.set_selected_object(id, false, &mut out);
        }

        fn selected(&self) -> Option<ObjectId> {
            self.objects.world.selected
        }

        fn stamp(&self) -> f64 {
            self.objects.world.combat.last_attacked_time
        }

        /// The two production slots, in `App::frame`'s own order: `interaction::apply_events` (the
        /// net blob drain, which records the notification) and then `interaction::use_time` (which
        /// runs the tail, because that is where the selection geometry exists).
        fn deliver(&mut self, e: &SessionEvent) {
            dereth_client_runtime::interaction::apply_events(
                &mut self.inter,
                std::slice::from_ref(e),
                &mut self.objects.world,
            );
            let (unowned, left) = dereth_client_runtime::interaction::use_time(
                &mut self.inter,
                &self.store,
                Some(&self.scene),
                &mut self.objects,
                None,
                vec![],
                false,
                (800, 600),
                LocalTime(self.now),
            );
            assert!(unowned.is_empty(), "no unowned requests were expected");
            assert!(left.is_empty(), "no actions were delivered");
        }

        fn notifications(&self) -> u64 {
            self.inter.stats.defender_notifications
        }

        fn defender_auto_targets(&self) -> u64 {
            self.inter.stats.defender_auto_targets
        }
    }

    // =============================================================================================
    // 0. Calibration
    // =============================================================================================

    /// **The denominator that makes every zero below a measurement.**
    ///
    /// The stamp starts at `CombatState::begin`'s `0.0` and `now` is deliberately not zero, so "the
    /// clock was stamped" cannot be satisfied by the field never being touched. And the bench must
    /// be able to produce **both** answers of the arm under test — the attacker and the fallback —
    /// from the same fixtures, or a test that sees one of them proves nothing about the other.
    #[test]
    fn the_bench_can_produce_both_answers_and_starts_unstamped() {
        for h in BOTH {
            let mut b = Bench::new();
            b.now = 40.0;
            assert_eq!(
                b.stamp(),
                0.0,
                "{h:?}: combat state begins with the last-attacked timestamp at 0.0"
            );

            // Answer A: the fallback, with no last attacker at all.
            b.place(NEAR, NEAR_AT);
            b.place(FAR, FAR_AT);
            b.make_attackable_compass_item(NEAR);
            b.make_attackable_compass_item(FAR);
            b.deliver(&h.event());
            assert_eq!(
                b.selected(),
                Some(NEAR),
                "{h:?}: the fallback takes the closer of the two"
            );

            // Answer B: the attacker, who is the FARTHEST of the three, so the two answers can
            // never coincide.
            let mut b = Bench::new();
            b.now = 40.0;
            b.place(NEAR, NEAR_AT);
            b.place(FAR, FAR_AT);
            b.place(ATTACKER, ATTACKER_AT);
            b.make_attackable_compass_item(NEAR);
            b.make_attackable_compass_item(FAR);
            b.set_last_attacker(ATTACKER);
            b.deliver(&h.event());
            assert_eq!(
                b.selected(),
                Some(ATTACKER),
                "{h:?}: the re-select arm takes the attacker, who is further away than either fallback \
             candidate -- so the two arms cannot be confused"
            );
        }
    }

    // =============================================================================================
    // 1. The stamp
    // =============================================================================================

    /// **`CombatState::last_attacked_time` has two production writers.** Both defender
    /// handlers store the timer's two-word value into the same timestamp field before applying the
    /// gates below.
    ///
    /// Asserted per handler, with a denominator, so *"no notification arrived"* and *"a
    /// notification arrived and did not stamp"* are different failures.
    #[test]
    fn each_defender_handler_stamps_the_clock() {
        for h in BOTH {
            let mut b = Bench::new();
            b.now = 37.5;
            assert_eq!(b.stamp(), 0.0);
            assert_eq!(b.notifications(), 0);

            b.deliver(&h.event());

            assert_eq!(
                b.notifications(),
                1,
                "{h:?}: exactly one notification reached the tail"
            );
            assert!(
                (b.stamp() - 37.5).abs() < f64::EPSILON,
                "{h:?}: the last-attacked timestamp must equal the current time, got {}",
                b.stamp()
            );

            // A second one at a later time re-stamps: the store is unconditional and has no edge.
            b.now = 41.25;
            b.deliver(&h.event());
            assert_eq!(b.notifications(), 2);
            assert!(
                (b.stamp() - 41.25).abs() < f64::EPSILON,
                "{h:?}: and it moves again"
            );
        }
    }

    /// **The stamp is above every gate.** In both handlers the timestamp store precedes the
    /// combat-mode compare, and the message-squelch branches have already rejoined. So a
    /// notification in peace mode, or with the option off, or with something selected, still moves
    /// the clock — and only automatic targeting under it is skipped.
    ///
    /// Three refusals rather than one, because they are three different gates.
    #[test]
    fn the_stamp_is_unconditional_even_when_the_gate_below_it_refuses() {
        for h in BOTH {
            for (why, setup) in [
                ("peace mode", 0u8),
                ("the automatic-target option off", 1),
                ("something already selected", 2),
            ] {
                let mut b = Bench::new();
                b.now = 21.0;
                b.place(NEAR, NEAR_AT);
                b.make_attackable_compass_item(NEAR);
                match setup {
                    0 => b.objects.world.combat.combat_mode = CombatMode::NonCombat,
                    1 => {
                        b.objects
                            .world
                            .player_system
                            .options
                            .set(AUTO_TARGET_OPTION, false);
                    }
                    _ => {
                        b.place(FAR, FAR_AT);
                        b.select(Some(FAR));
                    }
                }
                b.deliver(&h.event());
                assert_eq!(b.notifications(), 1, "{h:?}/{why}: the tail ran");
                assert!(
                    (b.stamp() - 21.0).abs() < f64::EPSILON,
                    "{h:?}/{why}: the stamp is above the gate and must have happened anyway"
                );
                assert_eq!(
                    b.defender_auto_targets(),
                    0,
                    "{h:?}/{why}: and automatic targeting must NOT have run"
                );
            }
        }
    }

    /// A body that will not decode never reaches the tail — the client's message queue unpacks a
    /// body before dispatching its handler, so a malformed `0x01B2` stamps nothing.
    ///
    /// The control that makes this a measurement rather than a silence: the same bench, the same
    /// opcode, a **well-formed** body, does stamp.
    #[test]
    fn an_undecodable_body_stamps_nothing() {
        for h in BOTH {
            let mut b = Bench::new();
            b.now = 12.0;
            b.deliver(&h.corrupt_event());
            assert_eq!(
                b.notifications(),
                0,
                "{h:?}: an undecodable body is not a notification"
            );
            assert_eq!(b.stamp(), 0.0, "{h:?}: and it stamps nothing");

            b.deliver(&h.event());
            assert_eq!(
                b.notifications(),
                1,
                "{h:?}: the control -- a good body on the same bench"
            );
            assert!((b.stamp() - 12.0).abs() < f64::EPSILON);
        }
    }

    // =============================================================================================
    // 2. The arm the stamp makes reachable
    // =============================================================================================

    /// Behaviour:
    /// selection.auto-target.a-defender-notification-stamps-the-attack-clock-and-reselects
    ///
    /// **A defender notification stamps the clock and the re-select arm fires.**
    ///
    /// End to end through the two production slots, per handler, with the attacker placed *further
    /// away* than the fallback's winner so the two arms are distinguishable, and with the
    /// fallback's candidates present so *"the arm fired"* is not satisfied by there being nothing
    /// else to pick.
    #[test]
    fn each_defender_handler_reselects_the_last_attacker() {
        for h in BOTH {
            let mut b = Bench::new();
            b.now = 60.0;
            b.place(NEAR, NEAR_AT);
            b.place(ATTACKER, ATTACKER_AT);
            b.make_attackable_compass_item(NEAR);
            b.set_last_attacker(ATTACKER);
            assert_eq!(
                b.selected(),
                None,
                "the premise: nothing is selected, as at the call site"
            );

            b.deliver(&h.event());

            assert_eq!(b.notifications(), 1, "{h:?}: the tail ran");
            assert_eq!(
                b.defender_auto_targets(),
                1,
                "{h:?}: and it reached automatic targeting"
            );
            assert_eq!(
                b.selected(),
                Some(ATTACKER),
                "{h:?}: the elapsed time at the call is zero, because the stamp comes immediately \
             before it -- so the re-select arm, not the fallback"
            );
        }
    }

    /// **Mode 2 or mode 4 — the mode compare, all five stances.**
    ///
    /// This is the mode gate shared by all **four** automatic-target call sites: mode change,
    /// selection change, and the two defender notifications. Their call and tail-call sites all
    /// admit only melee and missile, making automatic targeting unreachable while casting. Asserted
    /// over every `CombatMode` rather than over the two that pass, so a widened gate is visible.
    #[test]
    fn only_melee_and_missile_auto_target() {
        for h in BOTH {
            for (mode, expect) in [
                (CombatMode::Melee, true),
                (CombatMode::Missile, true),
                (CombatMode::Magic, false),
                (CombatMode::NonCombat, false),
                (CombatMode::Undef, false),
            ] {
                let mut b = Bench::new();
                b.now = 5.0;
                b.place(NEAR, NEAR_AT);
                b.make_attackable_compass_item(NEAR);
                b.objects.world.combat.combat_mode = mode;
                b.deliver(&h.event());
                assert_eq!(
                    b.defender_auto_targets() == 1,
                    expect,
                    "{h:?}/{mode:?}: the gate is `== MELEE || == MISSILE`"
                );
                assert_eq!(
                    b.selected().is_some(),
                    expect,
                    "{h:?}/{mode:?}: and the selection follows it"
                );
                assert_eq!(
                    b.notifications(),
                    1,
                    "{h:?}/{mode:?}: the tail ran either way"
                );
            }
        }
    }

    /// Automatic targeting reads bit 13 of the first character-option word.
    ///
    /// Both directions from the same fixture, so "the option refused" and "the arm is dead" cannot
    /// be confused.
    #[test]
    fn the_auto_target_option_gates_the_arm() {
        for h in BOTH {
            for on in [true, false] {
                let mut b = Bench::new();
                b.now = 5.0;
                b.place(NEAR, NEAR_AT);
                b.make_attackable_compass_item(NEAR);
                b.objects
                    .world
                    .player_system
                    .options
                    .set(AUTO_TARGET_OPTION, on);
                b.deliver(&h.event());
                assert_eq!(
                    b.defender_auto_targets() == 1,
                    on,
                    "{h:?}: option {on} decides the arm"
                );
                assert_eq!(b.selected(), if on { Some(NEAR) } else { None });
            }
        }
    }

    /// **A nonzero selected id blocks this path, unlike the mode-change gate.**
    ///
    /// The mode-change path looks up an attack target and tests whether it is attackable; the two
    /// defender handlers ask only whether anything at all is selected. The discriminating station
    /// is therefore a selected object that is **not** attackable: the defender path must refuse it
    /// and the mode-change path must not. Both are asserted here, because a single-gate reading
    /// would collapse the two gates into one.
    #[test]
    fn anything_selected_at_all_blocks_the_arm_here() {
        for h in BOTH {
            let mut b = Bench::new();
            b.now = 5.0;
            b.place(NEAR, NEAR_AT);
            b.place(FAR, FAR_AT);
            b.make_attackable_compass_item(NEAR);
            // `FAR` is placed and visible but is **not** attackable and not a compass item.
            assert!(
                !b.objects.world.object_is_attackable(FAR),
                "the premise: the selected object is not attackable"
            );
            b.select(Some(FAR));

            b.deliver(&h.event());

            assert_eq!(b.notifications(), 1, "{h:?}: the tail ran");
            assert_eq!(
                b.defender_auto_targets(),
                0,
                "{h:?}: an existing selection refuses, attackable or not"
            );
            assert_eq!(
                b.selected(),
                Some(FAR),
                "{h:?}: and nothing moved the selection"
            );

            // The other gate, on the same world, answers the other way — which is what makes the
            // two functions two functions.
            let phys = b.phys();
            let lookup = |id| phys.get(id);
            let mut out = dereth_client_model::RecordingSink::default();
            assert!(
                b.objects.world.combat_mode_auto_target(
                    &lookup,
                    RADAR_RADIUS_OUTDOORS,
                    LocalTime(b.now),
                    &mut out
                ),
                "{h:?}: the mode-change gate tests whether its target is attackable and therefore ACCEPTS the same \
             state -- if this ever fails, the two gates have been collapsed into one"
            );
        }
    }

    // =============================================================================================
    // 3. The window, measured against a number a production path wrote
    // =============================================================================================

    /// **The 15-second window, both sides of the boundary, over a stamp a notification wrote.**
    ///
    /// The comparison tests the elapsed time against `15.0` and takes the fallback exactly when the
    /// elapsed time is **not less than** 15.0. The number under the comparison comes out of
    /// `Handler::event`, not a hand-written field.
    ///
    /// The `>= 15.0` leg cannot be reached *through a notification at all*: the stamp is taken
    /// immediately before the call, so the elapsed time inside a handler is always zero. The other
    /// side of the boundary belongs to automatic targeting's other two call sites — mode change and
    /// selection change — some seconds after the hit. This test isolates the time boundary by
    /// calling the world's target-selection routine at a later `now`; it is the only direct call to
    /// that routine in this module.
    #[test]
    fn the_window_is_measured_from_the_stamp_the_notification_wrote() {
        for h in BOTH {
            for (elapsed, expected, why) in [
                (
                    14.9_f64,
                    ATTACKER,
                    "inside the window, so the attacker is re-selected",
                ),
                (
                    15.0_f64,
                    NEAR,
                    "at exactly 15.0 the elapsed time is not less than 15, so the fallback is taken",
                ),
                (15.1_f64, NEAR, "and beyond it, plainly"),
            ] {
                let mut b = Bench::new();
                b.now = 30.0;
                b.place(NEAR, NEAR_AT);
                b.place(ATTACKER, ATTACKER_AT);
                b.make_attackable_compass_item(NEAR);
                b.make_attackable_compass_item(ATTACKER);
                b.set_last_attacker(ATTACKER);

                // The production writer. Nothing here sets `last_attacked_time` by hand.
                b.deliver(&h.event());
                assert!(
                    (b.stamp() - 30.0).abs() < f64::EPSILON,
                    "{h:?}: the premise -- the notification stamped 30.0"
                );
                assert_eq!(
                    b.selected(),
                    Some(ATTACKER),
                    "{h:?}: and the notification's own automatic targeting re-selected, at elapsed 0"
                );
                // Clear it again, so the later call meets the empty selection its own call sites
                // do.
                b.select(None);

                let phys = b.phys();
                let lookup = |id| phys.get(id);
                let mut out = dereth_client_model::RecordingSink::default();
                b.objects.world.auto_target(
                    &lookup,
                    RADAR_RADIUS_OUTDOORS,
                    LocalTime(30.0 + elapsed),
                    &mut out,
                );
                assert_eq!(b.selected(), Some(expected), "{h:?}/{elapsed}: {why}");
            }
        }
    }

    /// **Two notifications in one frame run the tail twice**, because the tail runs once per
    /// handler call and automatic targeting under it is not idempotent: the first one selects, and
    /// the second then finds something already selected and refuses.
    ///
    /// Collapsing the batch to a flag would make the second invisible, and the counter is what says
    /// so. Driven with the two **different** handlers in one batch, which is also the only test
    /// here that proves they share one queue rather than each having its own.
    #[test]
    fn two_notifications_in_one_frame_are_two_calls() {
        let mut b = Bench::new();
        b.now = 8.0;
        b.place(NEAR, NEAR_AT);
        b.make_attackable_compass_item(NEAR);

        let events = [Handler::Defender.event(), Handler::Evasion.event()];
        dereth_client_runtime::interaction::apply_events(
            &mut b.inter,
            &events,
            &mut b.objects.world,
        );
        let (unowned, left) = dereth_client_runtime::interaction::use_time(
            &mut b.inter,
            &b.store,
            Some(&b.scene),
            &mut b.objects,
            None,
            vec![],
            false,
            (800, 600),
            LocalTime(b.now),
        );
        assert!(unowned.is_empty());
        assert!(left.is_empty());

        assert_eq!(b.notifications(), 2, "both handlers' tails ran");
        assert_eq!(
            b.defender_auto_targets(),
            1,
            "the first one selected NEAR; the second found a selection and refused -- which is \
         only visible because the batch was not collapsed into one call"
        );
        assert_eq!(b.selected(), Some(NEAR));
        assert!((b.stamp() - 8.0).abs() < f64::EPSILON);
    }
}

/// A deselect raises the selection-changed notice, and its handler is the one path from which
/// auto-targeting can reach the 15-second fallback: the boundary is exclusive, the defender path
/// never reaches it, a willingly lost target is refused once and cleared, each gate refuses on its
/// own, and losing the target aborts an automatic attack. Objects sit on 3/4/5 headings so
/// distance and compass ordering are independently visible.
mod selection_change {
    //! # Why only a selection change reaches the fallback
    //!
    //! Auto-targeting has two arms. The first re-selects the object that last hit you; the second —
    //! reached when there is no attacker, when the attacker is gone, or when the elapsed time since
    //! the last attack is **not** less than `15.0` — selects the next compass item with the
    //! `(1, 1, 2 = compass item, 0)` arguments. The defender notifications stamp the
    //! last-attacker time immediately before the transfer and the mode change carries no clock at
    //! all, so a defender notification always arrives at the freshness test with an elapsed time of
    //! exactly zero. **Only the selection-change site can be seconds old.**
    //!
    //! # What is asserted, and why each is its own test
    //!
    //! 1. The notice reaches the handler at all, through the production absorb seam.
    //! 2. The **fallback leg fires from it**, asserted **on both sides of the boundary** — and the
    //!    two sides select *different objects*, so the assertion is a discrimination and not a
    //!    liveness check.
    //! 3. The willing-target-loss flag is **clear-once**, tested **twice**: the notice that meets
    //!    the flag is refused *and clears it*, and the next notice of the same shape is answered.
    //! 4. Each gate above the tail jump can refuse on its own.
    //! 5. The inlined head aborts automatic attack when repeat attack is in progress.
    //!
    //! A non-forced deselect while nothing is selected raises no notice: the setter returns before
    //! doing anything when the selected id already equals the requested id, and even a forced call
    //! skips the notice if the id did not change. So every deselect below first holds a target.

    use dereth_scene::world_scene::SceneWrites;
    use std::sync::Arc;

    use dereth_client_model::combat::CombatMode;
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};
    use dereth_client_model::selection::LAST_ATTACKER_IID;
    use dereth_client_model::weenie::{bitfield, item_type};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::character::PLAYER_OBJECT_ID;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_client_runtime::selection_geometry::SceneSelectionPhysics;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::Message;
    use dereth_render::device::{DeviceConfig, Gpu};
    use dereth_ui_screens::view::UiRequest;
    use {
        dereth_client_runtime::interaction::action as ia,
        dereth_client_runtime::interaction::Interaction,
    };
    use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

    const AUTO_TARGET_OPTION: usize = dereth_client_model::player::options::option::AUTO_TARGET;
    const AUTO_REPEAT_OPTION: usize =
        dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK;

    /// The auto-target last-attacker interval, used by the floating-point freshness compare.
    const MAX_INTERVAL: f64 = 15.0;

    /// The attacker: an attackable compass item at 9 m, so it is also a next-selection candidate
    /// and the two auto-target arms have to be told apart by *which* object they picked.
    const ATTACKER: ObjectId = ObjectId(0x8200_0001);
    const ATTACKER_AT: (f32, f32, f32) = (5.4, 7.2, 0.0);
    /// The fallback's answer: the **closer** attackable compass item, at 3 m.
    const NEARER: ObjectId = ObjectId(0x8200_0002);
    const NEARER_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);

    // =============================================================================================
    // Bench — the automatic-targeting bench
    // =============================================================================================

    struct Bench {
        _gpu: Gpu,
        scene: WorldScene,
        store: Arc<RetailDatStore>,
        objects: ObjectStream,
        inter: Interaction,
        player: Position,
        now: f64,
    }

    impl Bench {
        fn new() -> Self {
            let store =
                Arc::new(dereth_dat::testing::open_store().expect(
                    "the retail dats are `use_time`'s own argument: set DERETH_TEST_DAT_DIR",
                ));
            let cfg = DeviceConfig {
                width: 800,
                height: 600,
                ..DeviceConfig::default()
            };
            let mut gpu = Gpu::new(None, &cfg).expect("a D3D12 WARP device");
            let region =
                dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
            let scfg = SceneConfig {
                cell_statics: false,
                mesh_collision: false,
                land_radius: 1,
                scenery_radius: 0,
                particles: false,
                ..SceneConfig::default()
            };
            let mut scene = WorldScene::load(&store, &mut gpu, scfg).expect("the scene loads");
            scene
                .attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            let player = scene.character.as_ref().expect("a body").position();
            assert!(
                dereth_physics::landdefs::is_outdoors(player.cell),
                "the premise: outdoors, so the next-selection radius is 75.0"
            );

            let mut objects = ObjectStream::new();
            objects.world.player = Some(PLAYER_OBJECT_ID);
            let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
            me.valid = true;
            me.has_phys_obj = true;
            me.qualities = Some(dereth_client_model::qualities::Qualities::new());
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

        /// The full ladder open: melee, the character option on, both candidates placed, and the
        /// attacker recorded in the player state.
        fn ready_to_auto_target() -> Self {
            let mut b = Self::new();
            b.place(NEARER, NEARER_AT);
            b.make_attackable_compass_item(NEARER);
            b.place(ATTACKER, ATTACKER_AT);
            b.make_attackable_compass_item(ATTACKER);
            b.set_last_attacker(ATTACKER);
            b.objects.world.combat.combat_mode = CombatMode::Melee;
            b.objects
                .world
                .player_system
                .options
                .set(AUTO_TARGET_OPTION, true);
            // **A premise.** The range-exit path is a **third** producer of a selected-id-zero
            // notice, and in this bench it fires on its own: it deselects whenever the selected
            // object is not marked in view. The rendering path produces that mark, while this
            // interaction-only bench does not run the render pass, so an unattended headless
            // interaction frame drops the selection at the next poll and raises an extra notice. It
            // is pinned true here so that every notice below is one the test asked for; the
            // producer itself is driven by the selection-persistence tests.
            b.objects.world.selected_object_in_view = true;
            // The standing-still query reads the body's own motion interpreter, and a freshly
            // attached body is **not** on the ground — so `EscapeKey`'s cascade legs are
            // unreachable until it is. Set the motion-layer state read by the standing-still query,
            // rather than changing the action arm.
            b.stand_still(true);
            b
        }

        /// The motion interpreter's standing-still predicate begins with `env.on_ground`.
        fn stand_still(&self, still: bool) {
            let c = self.scene.character.as_ref().expect("a body");
            c.driver_mut().env.on_ground = still;
        }

        fn place(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
            use dereth_protocol::types::{
                physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc,
            };

            let v = Vec3::new(offset.0, offset.1, offset.2);
            let origin = dereth_physics::math::localtoglobal(&self.player.frame, v);
            let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
                dereth_protocol::objects::ObjectCreatePayload {
                    id,
                    objdesc: ObjDesc::default(),
                    physicsdesc: PhysicsDesc {
                        bitfield: flags::POSITION,
                        state: 0,
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
            let measured = self
                .phys()
                .get(id)
                .expect("the seam can answer for it")
                .player_space;
            assert!(
                (measured.0 - offset.0).abs() < 1e-3 && (measured.1 - offset.1).abs() < 1e-3,
                "{id:?} landed at {measured:?}, the station asked for {offset:?}"
            );
            self.objects.world.update_visible_object_list();
            assert!(
                self.objects.world.tables.visible.contains(&id),
                "{id:?} must be visible"
            );
        }

        fn phys(&self) -> SceneSelectionPhysics {
            SceneSelectionPhysics::new(Some(&self.player), &self.objects)
        }

        fn make_attackable_compass_item(&mut self, id: ObjectId) {
            let w = self
                .objects
                .world
                .tables
                .weenies
                .get_mut(id)
                .expect("placed");
            w.pwd.radar_enum = Some(4); // ShowAlways
            w.pwd.obj_type |= item_type::CREATURE;
            w.pwd.bitfield |= bitfield::ATTACKABLE;
            assert!(
                self.objects.world.object_is_attackable(id),
                "premise: {id:?} is attackable"
            );
        }

        fn set_last_attacker(&mut self, id: ObjectId) {
            self.objects
                .world
                .player_qualities_mut()
                .expect("the player description")
                .set(
                    StatKey::new(StatType::Iid, LAST_ATTACKER_IID),
                    StatValue::Iid(id),
                );
            assert_eq!(
                self.objects.world.last_attacker(),
                id,
                "premise: the `0xB` quality is set"
            );
        }

        /// One `App::frame` worth of `interaction::use_time`, with whatever was queued.
        fn frame(&mut self) {
            let (unowned, left) = dereth_client_runtime::interaction::use_time(
                &mut self.inter,
                &self.store,
                Some(&self.scene),
                &mut self.objects,
                None,
                Vec::new(),
                false,
                (800, 600),
                LocalTime(self.now),
            );
            assert!(unowned.is_empty(), "no unowned UI requests were expected");
            assert!(
                left.is_empty(),
                "no actions were delivered, so none can be left over"
            );
        }

        /// The toolbar's selection request uses `(id, 0)` and raises a selection-change notice when
        /// the id actually moves. `ObjectId(0)` is the client's deselect, and it is the notice this
        /// whole module is about.
        fn request_select(&mut self, id: ObjectId) {
            self.inter.queue(Vec::new(), vec![UiRequest::Select(id)]);
        }

        /// A defender notification through the production decode in `interaction::apply_events` —
        /// **the only writer of the last-attacker timestamp**.
        fn take_a_hit(&mut self) {
            let msg = dereth_protocol::combat::DefenderNotification {
                attacker_name: "Sparring Golem".into(),
                damage_type: 0x4,
                percent: 0.1,
                damage: 3,
                damage_location: 0,
                critical: 0,
                attack_conditions: 0,
                attack_conditions_high: 0,
            };
            let mut blob = dereth_protocol::combat::DefenderNotification::OPCODE
                .0
                .to_le_bytes()
                .to_vec();
            blob.extend(dereth_protocol::write_body(&msg).expect("encode"));
            dereth_client_runtime::interaction::apply_events(
                &mut self.inter,
                &[SessionEvent::UiEvent {
                    opcode: dereth_protocol::combat::DefenderNotification::OPCODE,
                    blob,
                }],
                &mut self.objects.world,
            );
        }

        fn selected(&self) -> Option<ObjectId> {
            self.objects.world.selected
        }

        fn handler_calls(&self) -> u64 {
            self.inter.stats.selection_change_notices
        }

        fn tail_jumps(&self) -> u64 {
            self.inter.stats.selection_change_auto_targets
        }

        fn willingly_lost(&self) -> u64 {
            self.inter.stats.selection_changes_willingly_lost
        }
    }

    // =============================================================================================
    // 0. Calibration — the bench must be able to raise the notice AND to raise none
    // =============================================================================================

    /// **The instrument is checked in both directions.** Everything below counts notices; before a
    /// count is worth anything the instrument has to be shown to produce a non-zero *and* a zero
    /// from the same gesture. The zero is the interesting half: the selection setter returns early
    /// when the id has not moved, so a re-select of the same object with `force = 0` raises nothing
    /// at all — which is a positive that is hard for this instrument in exactly the way the corpus
    /// is hard, because it looks identical to a handler that never ran.
    #[test]
    fn the_bench_can_raise_a_selection_notice_and_can_raise_none() {
        let mut b = Bench::ready_to_auto_target();
        b.request_select(NEARER);
        b.frame();
        assert_eq!(b.selected(), Some(NEARER), "the request selected it");
        assert_eq!(
            b.handler_calls(),
            1,
            "one notice, answered in the same frame it was absorbed in"
        );

        // The same request again: no change, no notice, no handler call.
        b.request_select(NEARER);
        b.now += 1.0;
        b.frame();
        assert_eq!(
            b.handler_calls(),
            1,
            "a re-select of the same id raises nothing"
        );
        b.now += 1.0;
        b.frame();
        assert_eq!(
            b.handler_calls(),
            1,
            "and a quiet frame raises nothing either"
        );

        // **The other zero.** A non-forced deselection while the selected id is already zero is the
        // same early return: with force clear, the setter compares the selected id against the
        // requested id and returns when they match. A deselect of nothing is not a selection
        // change, so it can never reach the selection-change handler, let alone auto-targeting.
        // Encoding the deselect as `Some(ObjectId(0))` instead of `None` makes this leg red (one
        // notice, one tail jump).
        let mut b = Bench::ready_to_auto_target();
        assert_eq!(b.selected(), None, "premise: nothing is selected");
        b.request_select(ObjectId(0));
        b.frame();
        b.now += 1.0;
        b.frame();
        assert_eq!(b.handler_calls(), 0, "a deselect of nothing raises nothing");
        assert_eq!(
            b.tail_jumps(),
            0,
            "so auto-targeting cannot be reached from it"
        );
        assert_eq!(b.selected(), None, "and nothing was selected on its behalf");
    }

    // =============================================================================================
    // 1. The notice reaches the handler
    // =============================================================================================

    /// **The arm exists and is wired**: the notice has a subscriber. The selection setter sends it
    /// only when the id actually moves, after its `!force && old == new` early return.
    #[test]
    fn a_deselect_raises_the_selection_changed_notice() {
        let mut b = Bench::ready_to_auto_target();
        b.request_select(NEARER);
        b.frame();
        assert_eq!(b.handler_calls(), 1);
        assert_eq!(b.tail_jumps(), 0, "a held selection refuses auto-targeting");

        // Select zero without force: the client's deselect.
        b.now += 1.0;
        b.request_select(ObjectId(0));
        b.frame();
        assert_eq!(
            b.handler_calls(),
            2,
            "the deselect raised the second notice"
        );
        assert_eq!(
            b.tail_jumps(),
            1,
            "with nothing selected, the auto-target transfer ran"
        );
        assert_ne!(
            b.selected(),
            Some(ObjectId(0)),
            "the selection is not represented as object id zero"
        );
    }

    // =============================================================================================
    // 2. The 15-second fallback leg, from a production path, on both sides of the boundary
    // =============================================================================================

    /// Drive one deselect notice with `elapsed` seconds since the last hit, and report what
    /// auto-targeting selected.
    ///
    /// The stamp is made by the **production** writer: a real `0x01B2` defender notification
    /// decoded by `interaction::apply_events`. Nothing in this helper writes the last-attacker
    /// timestamp.
    fn what_auto_target_picks_after(elapsed: f64) -> (Option<ObjectId>, u64) {
        let mut b = Bench::ready_to_auto_target();
        // Something must be selected while the hit lands, or the defender notification's own
        // selected-id-zero gate would auto-target here instead and there would be nothing left for
        // the selection change to do.
        b.request_select(NEARER);
        b.frame();
        assert_eq!(
            b.selected(),
            Some(NEARER),
            "premise: a target is held while the hit lands"
        );

        b.now += 1.0;
        let stamped_at = b.now;
        b.take_a_hit();
        b.frame();
        assert!(
            (b.objects.world.combat.last_attacked_time - stamped_at).abs() < 1e-9,
            "premise: the production defender handler stamped the clock at {stamped_at}"
        );
        assert_eq!(
            b.inter.stats.defender_auto_targets, 0,
            "premise: the defender path refused, because a target was held"
        );
        let before = b.tail_jumps();

        // The clock moves, and then the player loses the target for a reason that is *not* Escape.
        b.now = stamped_at + elapsed;
        b.request_select(ObjectId(0));
        b.frame();
        assert_eq!(
            b.tail_jumps(),
            before + 1,
            "the selection change reached auto-targeting"
        );
        (b.selected(), b.tail_jumps())
    }

    /// Behaviour: selection.auto-target.a-deselect-reaches-the-fifteen-second-fallback
    ///
    /// **The fallback leg fires from a production path, and only past the boundary.**
    ///
    /// The freshness rule is strict: elapsed time below 15.0 seconds re-selects the attacker;
    /// equal, greater, and unordered comparisons take the fallback. At exactly 15.0 the attacker is
    /// *not* re-selected.
    ///
    /// The two sides pick **different objects** — the attacker at 9 m against the closer compass
    /// item at 3 m — so this is a discrimination and not a liveness check: a build that always took
    /// one leg would fail on the other side, and a build that selected nothing would fail on both.
    #[test]
    fn the_fifteen_second_fallback_leg_fires_from_a_selection_change_and_the_boundary_is_exclusive()
    {
        // Inside the window the attacker is re-selected without force.
        let (picked, n) = what_auto_target_picks_after(MAX_INTERVAL - 0.1);
        assert_eq!(
            picked,
            Some(ATTACKER),
            "14.9 s: still fresh, so the attacker comes back"
        );
        assert_eq!(n, 1);

        // **Exactly on it.** `>= 15.0` takes the fallback, the leg no other production path
        // reaches.
        let (picked, n) = what_auto_target_picks_after(MAX_INTERVAL);
        assert_eq!(
            picked,
            Some(NEARER),
            "15.0 s exactly takes the fallback, so the closest-compass selection answers with the NEARER object and not with the attacker"
        );
        assert_eq!(n, 1);

        // Well past it, so the boundary case is not the only stale reading.
        let (picked, _) = what_auto_target_picks_after(MAX_INTERVAL + 30.0);
        assert_eq!(picked, Some(NEARER), "45 s: the fallback again");
    }

    /// **The defender notification cannot reach the fallback leg.**
    ///
    /// The negative control for the test above: the same world, the same option, the same attacker
    /// — but auto-targeting reached from the defender handler is *always* fresh, because the stamp
    /// is taken immediately before the call. There is no elapsed time at which it takes the
    /// fallback.
    #[test]
    fn the_defender_path_can_never_reach_the_fallback_however_old_the_last_hit_is() {
        let mut b = Bench::ready_to_auto_target();
        // Age the clock by ten minutes with nothing selected, then get hit.
        b.now += 600.0;
        b.take_a_hit();
        b.frame();
        assert_eq!(
            b.inter.stats.defender_auto_targets, 1,
            "the defender path ran auto-targeting"
        );
        assert_eq!(
            b.selected(),
            Some(ATTACKER),
            "it re-selected the attacker: the timestamp written immediately before auto-targeting is this frame's clock, so elapsed time is 0.0 and cannot be >= 15.0"
        );
    }

    // =============================================================================================
    // 3. Willing target loss — a once-only flag, tested twice
    // =============================================================================================

    /// **Clear-once, asserted on both notices.**
    ///
    /// The handler tests the willing-loss flag and, on the set leg, clears it — the flag is cleared
    /// by the very notice it refuses. A player who pressed Escape to drop a target is therefore not
    /// immediately handed one back, and the *next* selection change behaves normally.
    ///
    /// One notice cannot see this: a flag that blocked for ever and a flag that blocks once are the
    /// same reading after a single delivery. Both notices here have the identical shape — a
    /// deselect with the same world, mode, option and attacker — and they get **different
    /// answers**, which is the only thing that distinguishes the two.
    ///
    /// The production writer is the Escape-key arm, which the Escape-cascade tests also exercise on
    /// their own.
    #[test]
    fn target_willingly_lost_is_cleared_by_the_notice_it_refuses() {
        let mut b = Bench::ready_to_auto_target();
        b.request_select(NEARER);
        b.frame();
        assert_eq!(b.selected(), Some(NEARER), "premise: a target is held");
        assert!(
            !b.objects.world.combat.target_willingly_lost,
            "premise: the flag starts clear"
        );
        let notices_before = b.handler_calls();

        // ---- notice 1: raised by Escape, which sets the flag on its way past
        // ----------------------
        //
        // The arm runs in `on_actions`, which is **below** `run_selection_change_notices` in the
        // frame, so the notice it raises is absorbed here and answered on the next frame. That is a
        // deliberate deviation of the queued notice path; the retail client dispatches it
        // synchronously from inside the selection setter.
        b.now += 1.0;
        press(&mut b, ia::ESCAPE_KEY);
        assert_eq!(
            b.inter.stats.escape_deselects, 1,
            "Escape took its deselect leg"
        );
        assert!(
            b.objects.world.combat.target_willingly_lost,
            "Escape set the willing-target-loss flag before deselection"
        );
        assert_eq!(b.selected(), None, "and deselected");

        b.now += 1.0;
        b.frame();
        assert_eq!(
            b.handler_calls(),
            notices_before + 1,
            "the deselect's notice was delivered"
        );
        assert_eq!(b.willingly_lost(), 1, "and the flag refused it");
        assert_eq!(b.tail_jumps(), 0, "auto-targeting did NOT run");
        assert!(
            !b.objects.world.combat.target_willingly_lost,
            "the notice cleared the willing-target-loss flag"
        );
        assert_eq!(b.selected(), None, "nothing was re-selected");

        // ---- notice 2: the same shape, and now it is answered
        // -------------------------------------
        b.now += 1.0;
        b.request_select(NEARER);
        b.frame();
        assert_eq!(b.selected(), Some(NEARER), "a target is held again");
        b.now += 1.0;
        b.request_select(ObjectId(0));
        b.frame();
        assert_eq!(
            b.willingly_lost(),
            1,
            "the flag was not set again, so it refused nothing"
        );
        assert_eq!(
            b.tail_jumps(),
            1,
            "the second deselect reached auto-targeting"
        );
        assert_eq!(
            b.selected(),
            Some(ATTACKER),
            "it answered -- this bench has taken no hit, so the last-attacker timestamp is still 0.0 and the elapsed time here is a few seconds: inside the window, so the re-select arm"
        );
    }

    /// One action through the production frame slot.
    fn press(b: &mut Bench, action: u32) {
        let e = dereth_client_runtime::actions::Action {
            id: dereth_input::ActionId(action),
            phase: dereth_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        let (unowned, left) = dereth_client_runtime::interaction::use_time(
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
        assert!(unowned.is_empty());
        assert!(
            left.is_empty(),
            "{action:#010X} must be consumed by `on_actions`"
        );
    }

    // =============================================================================================
    // 4. Every gate above the tail jump, each on its own
    // =============================================================================================

    /// Hold `NEARER`, so that the drop below is a selection change retail actually raises (the
    /// setter refuses a deselect of nothing -- calibration test, third leg). The select's own
    /// notice is delivered here and refused while a target remains selected, so it cannot be
    /// mistaken for the drop's.
    fn hold_target(b: &mut Bench) {
        let notices = b.handler_calls();
        let jumps = b.tail_jumps();
        b.request_select(NEARER);
        b.frame();
        assert_eq!(b.selected(), Some(NEARER), "premise: a target is held");
        assert_eq!(
            b.handler_calls(),
            notices + 1,
            "premise: the select's notice was delivered"
        );
        assert_eq!(
            b.tail_jumps(),
            jumps,
            "premise: a held selection refused auto-targeting"
        );
    }

    /// Selecting zero without force while a target is held finds old and new ids different and
    /// sends the selection-change notice. Two frames are used so a follow-up notice that
    /// auto-targeting raises by re-selecting is delivered as well.
    fn drop_target(b: &mut Bench) {
        b.now += 1.0;
        b.request_select(ObjectId(0));
        b.frame();
        b.now += 1.0;
        b.frame();
    }

    /// Each refusal is a separate comparison in the client and each is asserted from the same open
    /// ladder with **one** field changed, so a gate that stopped working could not hide behind
    /// another.
    ///
    /// Legs (a), (c) and (d) hold a target before dropping it -- see [`hold_target`].
    #[test]
    fn each_gate_above_the_tail_jump_can_refuse_on_its_own() {
        // (a) the whole ladder open -> the tail jump runs. The positive control for (b)..(d).
        let mut b = Bench::ready_to_auto_target();
        hold_target(&mut b);
        drop_target(&mut b);
        assert_eq!(
            b.tail_jumps(),
            1,
            "(a) the open ladder reaches auto-targeting"
        );
        assert_eq!(
            b.selected(),
            Some(ATTACKER),
            "(a) auto-targeting re-selected the attacker (no hit taken, so the clock is fresh)"
        );
        assert_eq!(
            b.handler_calls(),
            3,
            "(a) the select's notice, the drop's, and the notice raised by auto-targeting's own re-selection -- delivered on `drop_target`'s second frame and refused because a selection was then held, so the count is 3 and the transfer count stays at 1"
        );

        // (b) something is selected, so the handler refuses before auto-targeting.
        let mut b = Bench::ready_to_auto_target();
        b.request_select(NEARER);
        b.frame();
        assert_eq!(b.handler_calls(), 1, "(b) the handler ran");
        assert_eq!(b.tail_jumps(), 0, "(b) a held selection refused it");

        // (c) the mode compare is `== 2 || == 4`, so magic and peace refuse.
        for mode in [CombatMode::Magic, CombatMode::NonCombat] {
            let mut b = Bench::ready_to_auto_target();
            b.objects.world.combat.combat_mode = mode;
            hold_target(&mut b);
            drop_target(&mut b);
            assert_eq!(
                b.handler_calls(),
                2,
                "(c) {mode:?}: the handler ran for the drop"
            );
            assert_eq!(
                b.tail_jumps(),
                0,
                "(c) {mode:?}: the mode compare refused it"
            );
        }

        // (d) the character auto-target option is off.
        let mut b = Bench::ready_to_auto_target();
        b.objects
            .world
            .player_system
            .options
            .set(AUTO_TARGET_OPTION, false);
        hold_target(&mut b);
        drop_target(&mut b);
        assert_eq!(b.handler_calls(), 2, "(d) the handler ran for the drop");
        assert_eq!(b.tail_jumps(), 0, "(d) and the option gate refused it");
    }

    // =============================================================================================
    // 5. The head aborts automatic attack when repeat attack is in progress
    // =============================================================================================

    /// The handler head contains the repeat check and automatic-attack abort inline. The thing a
    /// player sees is that losing a target stops the automatic attack that was running against it.
    ///
    /// The repeat-attack predicate reads the **option first**, so the head is
    /// asserted with the option both ways.
    ///
    /// A target is held *before* the automatic attack is set running and dropped after, so the
    /// notice that reaches the head is the drop's and not the select's, and it is a notice the
    /// client raises (the setter refuses a deselect of nothing).
    #[test]
    fn losing_the_target_aborts_an_automatic_attack() {
        let mut b = Bench::ready_to_auto_target();
        hold_target(&mut b);
        b.objects
            .world
            .player_system
            .options
            .set(AUTO_REPEAT_OPTION, true);
        b.objects.world.combat.attack_in_progress = true;
        b.objects.world.combat.repeat_attacking = true;
        assert!(
            b.objects.world.repeat_attack_in_progress(),
            "premise: repeat attack is enabled and an attack is in progress"
        );
        drop_target(&mut b);
        assert!(
            !b.objects.world.combat.repeat_attacking,
            "the inlined automatic-attack abort cleared `repeat_attacking`"
        );

        // The option-off branch makes the repeat-attack predicate false whatever the
        // power bar is doing, so the head does nothing.
        let mut b = Bench::ready_to_auto_target();
        hold_target(&mut b);
        b.objects
            .world
            .player_system
            .options
            .set(AUTO_REPEAT_OPTION, false);
        b.objects.world.combat.attack_in_progress = true;
        b.objects.world.combat.repeat_attacking = true;
        assert!(
            !b.objects.world.repeat_attack_in_progress(),
            "the option dominates"
        );
        drop_target(&mut b);
        assert_eq!(
            b.handler_calls(),
            3,
            "the drop's notice was delivered to the head (and, with auto-targeting enabled, so was the re-selection's -- see leg (a) of the gate test)"
        );
        assert!(
            b.objects.world.combat.repeat_attacking,
            "so nothing was aborted"
        );
    }
}
