use std::sync::Arc;

use dereth_client::character::Character;
use dereth_client::interaction::Interaction;
use dereth_client::world::{load_region, DEFAULT_LANDBLOCK};
use dereth_client_model::combat::{CombatMode, COMBAT_TABLE_DID};
use dereth_client_model::Request;
use dereth_dat::RetailDatStore;
use dereth_input::spec::ControlChord;
use dereth_input::{ActionId, InputMapId};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_testkit::{ClientSpec, HeadlessClient};

/// The combat table every character is born carrying.
const A_COMBAT_TABLE: u32 = 0x3000_0021;
/// The middle of the starting landblock, which is where every body below settles.
const SPAWN: (f32, f32) = (96.0, 96.0);

pub fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// A body standing still on the retail terrain, settled for two seconds of thirty-hertz
/// frames. The settle is not a nicety: an unsettled body still has its entry motion
/// outstanding, so the ready arm below would be measuring a body that was never ready.
pub fn settled(store: &Arc<RetailDatStore>) -> (Character, u32) {
    let region = load_region(store).expect("the region decodes");
    let mut c =
        Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("the body is created");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the body must settle before anything is measured"
    );
    assert!(
        !c.driver().movement.motions_pending(),
        "and it must have finished its entry motion, or the ready arm measures the entry"
    );
    (c, 60)
}

/// A world with a player in `mode`, an attackable creature selected, and optionally the
/// combat table the melee arm reads off the wielded weapon.
///
/// The creature is not decoration: the attack request refuses without an attackable target,
/// so a world without one would make every silence a refusal the scenario did not intend.
pub fn world_in(mode: CombatMode, table: bool) -> dereth_client_model::World {
    let mut w = dereth_client_model::World::new();
    let player = ObjectId(0x5000_064D);
    w.player = Some(player);
    let mut pw = dereth_client_model::Weenie::new(player);
    pw.pwd.name = "Aldis".into();
    if table {
        pw.qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::StatKey::new(
                    dereth_client_model::StatType::Did,
                    COMBAT_TABLE_DID,
                ),
                dereth_client_model::StatValue::Did(DataId(A_COMBAT_TABLE)),
            );
    }
    w.tables.weenies.insert(player, pw);
    w.tables.inventories.insert(
        player,
        dereth_client_model::objects::ObjectInventory::new(player),
    );
    let monster = ObjectId(0x8000_064D);
    let mut m = dereth_client_model::Weenie::new(monster);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
    m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(monster, m);
    w.set_selected_object(Some(monster), false, &mut dereth_client_model::NullSink);
    w.combat.combat_mode = mode;
    w
}

/// One combat action, as the input dispatch delivers it.
pub fn key(action: u32, start: bool) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: ActionId(action),
        phase: if start {
            dereth_client_runtime::actions::ActionPhase::Begin
        } else {
            dereth_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 1,
    }
}

/// How many of `requests` are the melee attack the player's swing sends.
///
/// The requests are counted by variant rather than by opcode, because a variant is what the
/// claim is about and an opcode would be one indirection further from it.
pub fn melee_attacks(requests: &[Request]) -> usize {
    requests
        .iter()
        .filter(|r| matches!(r, Request::TargetedMeleeAttack(_)))
        .count()
}

/// How many of `requests` are the cancel that breaks an automatic attack.
pub fn cancels(requests: &[Request]) -> usize {
    requests
        .iter()
        .filter(|r| matches!(r, Request::CancelAttack(_)))
        .count()
}

/// The client's per-frame outbox. The frame replaces it every pass, so reading it once per
/// frame is "what did this frame send" rather than "what has ever been sent".
pub fn drain(c: &HeadlessClient) -> Vec<Request> {
    c.view().expect_app().interaction().last_sent.to_vec()
}

/// The shipped keymap's own control for `action` in `map` -- discovered, never written down.
pub fn shipped_control(c: &mut HeadlessClient, map: InputMapId, action: ActionId) -> ControlChord {
    let shell = c.app_mut().input_manager_mut().expect("the input shell");
    let section = shell
        .manager
        .keymap
        .section(map)
        .unwrap_or_else(|| panic!("the shipped keymap has a section for {map:?}"));
    // The unmodified defaults only: a binding carrying a meta mode would need its modifier
    // held across the press.
    let found: Vec<ControlChord> = section
        .bindings()
        .iter()
        .filter(|(q, a)| *a == action && q.meta_mode == 0)
        .map(|(q, _)| *q)
        .collect();
    assert!(
        !found.is_empty(),
        "the shipped keymap binds {action:?} in {map:?}"
    );
    found[0]
}

/// Fire one control edge through the client's own input manager.
pub fn fire(c: &mut HeadlessClient, qc: ControlChord, down: bool, t: u32) {
    use dereth_input::fire::ControlType;
    let data = if down { 0x80 } else { 0 };
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .fire_input_event(qc.control, ControlType::Button, data, t);
}

// -----------------------------------------------------------------------------------------
// The model bench: the client's own frame slot, driven at clocks the scenario chooses.
// -----------------------------------------------------------------------------------------

/// `App::frame`'s interaction slot over a synthetic melee world.
///
/// A whole client cannot be driven at arbitrary clocks -- its clock is the fixed-step one --
/// and the charge claims are about what the bar does between two chosen instants, so this
/// drives the production frame slot directly.
pub struct Bench {
    inter: Interaction,
    objects: dereth_client::objects::ObjectStream,
    store: Arc<RetailDatStore>,
    /// Accumulated, because the frame's outbox is replaced every pass: reading it directly
    /// would make "one swing, five frames ago" and "no swing at all" the same observation.
    sent: Vec<Request>,
}

impl Bench {
    pub fn new() -> Self {
        let mut objects = dereth_client::objects::ObjectStream::default();
        objects.world = world_in(CombatMode::Melee, true);
        Self {
            inter: Interaction::new(),
            objects,
            store: store(),
            sent: Vec::new(),
        }
    }

    /// One frame of the interaction slot, at `t`, carrying `actions`.
    pub fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, t: f64) {
        let _ = dereth_client::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(t),
        );
        self.sent.extend(self.inter.last_sent.iter().cloned());
    }

    /// Queue the requests a panel raises, as the combat window's own arms do.
    pub fn queue(&mut self, requests: Vec<dereth_ui_screens::view::UiRequest>) {
        self.inter.queue(Vec::new(), requests);
    }

    pub fn combat(&self) -> &dereth_client_model::combat::CombatState {
        &self.objects.world.combat
    }

    /// The power level of every melee attack this bench would have sent, in order.
    pub fn attacks(&self) -> Vec<f32> {
        self.sent
            .iter()
            .filter_map(|r| match r {
                Request::TargetedMeleeAttack(a) => Some(a.power_level),
                _ => None,
            })
            .collect()
    }
}

// -----------------------------------------------------------------------------------------
// The whole-client bench: the shipped keymap, the shipped panels, the client's own frames.
// -----------------------------------------------------------------------------------------

/// A whole headless client in the gameplay screen, with a player who carries the combat table
/// the shard's own description brings, an attackable creature selected, melee mode live and
/// the window's slider at the top -- so the shipped melee input map is registered and its
/// keys resolve.
pub fn a_melee_bench() -> AppBench {
    use dereth_protocol::archive::{PackedHash, Reader};
    use dereth_protocol::login::LoginPlayerDescription;
    use dereth_protocol::types::qualities::base_flags;
    use dereth_protocol::Message as _;

    const PLAYER: ObjectId = ObjectId(0x5000_064E);
    const MONSTER: ObjectId = ObjectId(0x8000_064E);

    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    {
        let w = c.world_mut();
        w.player = Some(PLAYER);
        let mut pw = dereth_client_model::Weenie::new(PLAYER);
        pw.pwd.name = "Aldis".into();
        pw.qualities = Some(dereth_client_model::Qualities::new());
        w.tables.weenies.insert(PLAYER, pw);
        w.tables.inventories.insert(
            PLAYER,
            dereth_client_model::objects::ObjectInventory::new(PLAYER),
        );
        let mut m = dereth_client_model::Weenie::new(MONSTER);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        w.tables.weenies.insert(MONSTER, m);
        w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
        w.inventory_mask = dereth_client_model::inventory::slots::loc::MELEE_WEAPON
            | dereth_client_model::inventory::slots::loc::HELD;
        w.combat.combat_mode = CombatMode::Melee;
        // The window's slider at the top: the cap the bar must reach before the frame swings.
        w.combat.set_ui_requested_power_from_scrollbar(1000);
    }

    // The description a shard sends a melee character: the combat table the melee arm reads,
    // and the two option words that carry the automatic-repeat setting.
    let mut d = LoginPlayerDescription::default();
    d.qualities.base.weenie_type = 0x0A;
    d.qualities.base.flags |= base_flags::DID;
    let o = dereth_client_model::player::options::Options::default();
    d.player_module.options = o.options;
    d.player_module.options2 = o.options2;
    d.qualities.base.tables.dids = Some(PackedHash {
        table_size: 8,
        entries: vec![(COMBAT_TABLE_DID, A_COMBAT_TABLE)],
    });
    let bytes = dereth_protocol::write_body(&d).expect("the description encodes");
    let d = LoginPlayerDescription::read(&mut Reader::new(&bytes)).expect("and decodes");
    c.when(dereth_testkit::Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
    ));

    attach_endpoint(&mut c);
    let mut b = AppBench { c, t: 100_000 };
    // Two frames, so the combat input maps are registered before any combat key is pressed.
    let _ = b.run(2);
    assert_eq!(
        b.c.view().world().combat_table_did(),
        Some(DataId(A_COMBAT_TABLE)),
        "the description reached the game model, so the melee ready arm can answer"
    );
    assert!(
        b.c.view()
            .world()
            .player_system
            .options
            .auto_repeat_attack(),
        "the automatic repeat is on, which is what makes a clean acknowledgement re-arm"
    );
    b
}

/// The wire reader is `dereth_testkit::wire::Wire`, and `HeadlessClient::take_wire_count` is
/// what these scenarios read it through; the endpoint under it is `dereth_testkit::replay`'s.
pub fn attach_endpoint(c: &mut HeadlessClient) {
    c.attach_replay(dereth_testkit::replay::connected_endpoint(
        dereth_testkit::replay::PEER_ADDR,
    ));
}

/// A whole client, plus the input device's own millisecond clock.
pub struct AppBench {
    c: HeadlessClient,
    /// Well clear of the double-click window, so one press is not read as a double click of
    /// itself.
    t: u32,
}

impl AppBench {
    /// Run `n` frames and answer the requests they sent.
    pub fn run(&mut self, n: u64) -> Vec<Request> {
        let mut out = Vec::new();
        for _ in 0..n {
            self.c.tick(1);
            out.extend(drain(&self.c));
        }
        out
    }

    pub fn control(&mut self, map: InputMapId, action: ActionId) -> ControlChord {
        shipped_control(&mut self.c, map, action)
    }

    /// One control edge and the frames that dispatch what it raised.
    pub fn press(&mut self, qc: ControlChord, down: bool, frames: u64) -> Vec<Request> {
        self.t += 2_000;
        fire(&mut self.c, qc, down, self.t);
        self.run(frames)
    }

    /// One click of the shipped medium-attack key and the frames the bar needs to fill.
    pub fn one_real_attack(&mut self) {
        let k = self.control(
            dereth_input::combat::MELEE_COMBAT_MAP,
            ActionId(dereth_client::interaction::action::COMBAT_MEDIUM_ATTACK),
        );
        let mut sent = self.press(k, true, 1);
        sent.extend(self.press(k, false, 1));
        sent.extend(self.run(60));
        assert!(
            melee_attacks(&sent) == 1,
            "the shipped attack key must put exactly one melee attack on the wire, got {sent:?}"
        );
        assert!(
            self.combat_state().attack_server_response_pending,
            "and the client is waiting for its acknowledgement"
        );
        assert!(self.repeating(), "the automatic repeat armed the loop");
    }

    /// The shard's acknowledgement, arriving the way it does in a session.
    pub fn attack_done(&mut self, error: u32) -> Vec<Request> {
        let m = dereth_protocol::combat::CombatHandleAttackDoneEvent { error };
        self.c.when(dereth_testkit::Inbound::message(&m));
        let mut out = drain(&self.c);
        out.extend(self.run(2));
        out
    }

    pub fn combat_state(&self) -> &dereth_client_model::combat::CombatState {
        &self.c.view().world().combat
    }

    pub fn repeating(&self) -> bool {
        self.c.view().world().combat.repeat_attacking
    }

    pub fn world_attack_in_progress(&self) -> bool {
        self.c.view().world().attack_in_progress
    }

    pub fn set_attack_in_progress(&mut self, v: bool) {
        self.c.world_mut().combat.attack_in_progress = v;
        self.c.world_mut().attack_in_progress = v;
    }

    pub fn set_ui_requested_power_from_scrollbar(&mut self, v: u32) -> f32 {
        self.c
            .world_mut()
            .combat
            .set_ui_requested_power_from_scrollbar(v)
    }

    pub fn attacks_done_count(&self) -> u64 {
        self.c.view().interaction().stats.attacks_done
    }

    pub fn chat_len(&self) -> usize {
        self.c.view().chat_lines().len()
    }

    pub fn char_input(&self) -> dereth_client::character::CharacterInput {
        self.c.view().expect_app().probe().char_input()
    }

    pub fn forward_attack_aborts(&self) -> (u64, u64) {
        self.c
            .view()
            .expect_app()
            .probe()
            .new_forward_attack_aborts()
    }

    /// How many cancels this client has built into a datagram since the last look.
    pub fn wire_cancels(&mut self) -> usize {
        /// The ordered sub-type of the message that breaks an automatic attack.
        const CANCEL_ATTACK: u32 = 0x01B7;
        self.c.take_wire_count(CANCEL_ATTACK)
    }

    pub fn shutdown(self) {
        self.c.shutdown();
    }
}
