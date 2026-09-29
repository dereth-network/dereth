//! Melee: whether the player may swing, the swing itself, and getting back out of combat.
//!
//! Whether the player may leave a combat mode depends on whether his motions are drained; the
//! answer a real body settled on Holtburg's terrain gives is stated directly here, so the claim
//! needs no dat.
//!
//! The drive here is the game model's own combat entry points rather than a `Player` step: the
//! power bar is charged by frames of the combat system's use-time, and the harness has no gesture
//! for "hold the attack button while the meter fills". The requests those calls produce are read
//! back **through the production sender** over a mock transport, so a request no arm of the sender
//! sends contributes nothing -- which is the whole difference between this and a counter.

use dereth_client_model::combat::{
    AttackHeight, CombatMode, COMBAT_TABLE_DID, MISSILE_READY_STYLES,
};
use dereth_client_model::{RecordingRequests, World};
use dereth_client_net::client_session::testing::MockTransport;
use dereth_client_net::client_session::{Session, SessionEvent};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::archive::{PackedHash, Reader};
use dereth_protocol::login::LoginPlayerDescription;
use dereth_protocol::types::qualities::base_flags;
use dereth_protocol::Message as _;
use dereth_testkit::{HeadlessClient, Inbound};

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "readiness_comes_from_the_combat_table",
        &["melee.readiness.comes-from-the-combat-table-the-login-carried"],
        readiness_comes_from_the_combat_table,
    ),
    (
        "a_melee_attack_swings_and_repeats",
        &["melee.attack.swings-when-the-power-bar-fills-and-repeats"],
        a_melee_attack_swings_and_repeats,
    ),
    (
        "leaving_combat_reaches_the_shard",
        &["combat.mode.the-toggle-out-of-combat-reaches-the-shard"],
        leaving_combat_reaches_the_shard,
    ),
    (
        "leaving_a_mode_is_judged_by_the_mode_being_left",
        &["combat.mode.whether-the-player-may-change-is-decided-by-the-mode-he-is-leaving"],
        leaving_a_mode_is_judged_by_the_mode_being_left,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

const PLAYER: ObjectId = ObjectId(0x5000_0476);
const MONSTER: ObjectId = ObjectId(0x8000_0777);
/// The combat table the character's login description carries.
const COMBAT_TABLE: u32 = 0x3000_0021;
/// The ordered game-action wrapper, and the two opcodes this scenario watches for.
const GAME_ACTION: u32 = 0xF7B1;
const TARGETED_MELEE_ATTACK: u32 = 0x0008;
const CHANGE_COMBAT_MODE: u32 = 0x0053;

/// One blob the production sender wrote, split back into its parts.
struct OnWire {
    wrapper: u32,
    opcode: u32,
    body: Vec<u8>,
}

/// Every request, through the production sender, read back as bytes.
fn on_the_wire(requests: &RecordingRequests) -> Vec<OnWire> {
    let mut s = Session::new(MockTransport::new());
    for r in &requests.0 {
        dereth_client::interaction::send_request(&mut s, r);
    }
    s.transport
        .sent
        .iter()
        .map(|b| {
            let mut r = Reader::new(&b.payload);
            let wrapper = r.u32().expect("the wrapper dword");
            let _stamp = r.u32().expect("the order stamp");
            let opcode = r.u32().expect("the opcode dword");
            OnWire {
                wrapper,
                opcode,
                body: r.rest().to_vec(),
            }
        })
        .collect()
}

fn opcodes(wire: &[OnWire]) -> Vec<u32> {
    wire.iter().map(|w| w.opcode).collect()
}

/// The login description an ACE shard sends a melee character, through the production writer and
/// reader so that the table under test has survived a wire round trip.
fn description() -> LoginPlayerDescription {
    let mut d = LoginPlayerDescription::default();
    d.qualities.base.weenie_type = 0x0A;
    d.qualities.base.flags |= base_flags::DID;
    // A real description carries the character's own options; the default carries none, which
    // would switch automatic repeat off and make this measure the fixture instead of the client.
    let o = dereth_client_model::player::options::Options::default();
    d.player_module.options = o.options;
    d.player_module.options2 = o.options2;
    d.qualities.base.tables.dids = Some(PackedHash {
        table_size: 8,
        entries: vec![(COMBAT_TABLE_DID, COMBAT_TABLE)],
    });
    let bytes = dereth_protocol::write_body(&d).expect("the description encodes");
    let mut r = Reader::new(&bytes);
    LoginPlayerDescription::read(&mut r).expect("and decodes")
}

/// A player in melee mode with an attackable target selected, and an empty set of qualities --
/// nothing here writes the combat table.
fn a_player_facing_a_monster(c: &mut HeadlessClient) {
    use dereth_client_model::inventory::slots::loc;
    let w: &mut World = c.world_mut();
    w.player = Some(PLAYER);
    let mut me = dereth_client_model::Weenie::new(PLAYER);
    me.pwd.name = "Aldis".to_owned();
    me.qualities = Some(dereth_client_model::Qualities::new());
    w.tables.weenies.insert(PLAYER, me);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    let mut monster = dereth_client_model::Weenie::new(MONSTER);
    monster.pwd.name = "Mosswart".to_owned();
    monster.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
    monster.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(MONSTER, monster);
    w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
    w.inventory_mask = loc::MELEE_WEAPON | loc::HELD;
    w.combat.combat_mode = CombatMode::Melee;
}

fn ready_for_attack(w: &World) -> bool {
    w.player_in_ready_position(true, Some(false))
}

fn ready_for_mode_change(w: &World) -> bool {
    w.player_in_ready_position(false, Some(false))
}

/// The click, and then the frames the meter fills over.
fn click_and_charge(w: &mut World, out: &mut RecordingRequests, frames: u32, t0: f64) {
    let r = ready_for_attack(w);
    let _ = w.set_requested_attack_height(AttackHeight::Medium, r, LocalTime(t0));
    let h = w.combat.requested_attack_height;
    w.end_attack_request(out, h, None, r, LocalTime(t0));
    for i in 1..=frames {
        let now = LocalTime(t0 + f64::from(i) / 30.0);
        let r = ready_for_attack(w);
        w.combat_power_bar_use_time(out, r, now);
    }
}

// =============================================================================================
// 1. melee.readiness.comes-from-the-combat-table-the-login-carried
// =============================================================================================

/// Whether the player may swing comes off the table the login description carried, and a
/// description that arrives before the body does is kept until there is somewhere to put it.
pub fn readiness_comes_from_the_combat_table() {
    let mut c = HeadlessClient::model();
    a_player_facing_a_monster(&mut c);
    let nothing_yet = c.view().world().combat_table_did().is_none();

    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        description(),
    ))));
    let delivered = c.view().world().combat_table_did() == Some(DataId(COMBAT_TABLE))
        && ready_for_attack(c.view().world())
        && ready_for_mode_change(c.view().world());

    // The shard can send the description before the player's own body exists: the table is
    // parked and installed when the body appears.
    let mut early = HeadlessClient::model();
    let no_player = early.view().world().player.is_none();
    early.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        description(),
    ))));
    let parked = early.view().world().combat_table_did().is_none();
    {
        let w = early.world_mut();
        assert!(w.set_player(PLAYER), "the player's own create");
        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id: PLAYER,
            ..dereth_protocol::objects::ObjectCreatePayload::default()
        };
        p.wdesc.name = "Aldis".to_owned();
        w.create_or_merge(
            &p,
            dereth_primitives::ServerTime(0.0),
            &mut dereth_client_model::NullSink,
        )
        .expect("the player's own create lands");
    }
    let installed = early.view().world().combat_table_did() == Some(DataId(COMBAT_TABLE));

    c.assert_behaviour(
        "melee.readiness.comes-from-the-combat-table-the-login-carried",
        move |_| nothing_yet && delivered && no_player && parked && installed,
    );
}

// =============================================================================================
// 2. melee.attack.swings-when-the-power-bar-fills-and-repeats
// =============================================================================================

/// One click swings once when the meter fills, the shard's own repeat sends nothing, and the
/// next click swings again.
pub fn a_melee_attack_swings_and_repeats() {
    let mut c = HeadlessClient::model();
    a_player_facing_a_monster(&mut c);
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        description(),
    ))));
    // "i can set the power bar": the combat window's slider.
    c.world_mut()
        .combat
        .set_ui_requested_power_from_scrollbar(1000);
    let repeat_is_on = c.view().world().player_system.options.auto_repeat_attack();

    let mut first = RecordingRequests::default();
    click_and_charge(c.world_mut(), &mut first, 40, 10.0);
    let wire = on_the_wire(&first);
    let swung = opcodes(&wire) == [TARGETED_MELEE_ATTACK] && wire[0].wrapper == GAME_ACTION && {
        let mut r = Reader::new(&wire[0].body);
        let m = dereth_protocol::combat::CombatTargetedMeleeAttack::read(&mut r)
            .expect("the body decodes");
        m.target == MONSTER
            && m.attack_height == AttackHeight::Medium as u32
            && (m.power_level - 1.0).abs() < 1e-6
    };

    // Three cycles of the shard's own repeat: it re-swings by itself and tells the client so the
    // meter refills. Nothing -- least of all a cancel -- goes out while it does.
    let mut during = RecordingRequests::default();
    let mut t = 12.0;
    let mut meter_restarted = true;
    for _ in 0..3 {
        let w = c.world_mut();
        w.handle_commence_attack();
        let r = ready_for_attack(w);
        w.handle_attack_done(&mut during, 0, r, LocalTime(t));
        meter_restarted &= w.combat.build_in_progress;
        for i in 1..=40 {
            let now = LocalTime(t + f64::from(i) / 30.0);
            let r = ready_for_attack(w);
            w.combat_power_bar_use_time(&mut during, r, now);
        }
        t += 2.0;
    }
    let silent_while_repeating = opcodes(&on_the_wire(&during)).is_empty();

    let mut second = RecordingRequests::default();
    click_and_charge(c.world_mut(), &mut second, 40, 20.0);
    let swung_again = opcodes(&on_the_wire(&second)) == [TARGETED_MELEE_ATTACK];

    c.assert_behaviour(
        "melee.attack.swings-when-the-power-bar-fills-and-repeats",
        move |_| repeat_is_on && swung && meter_restarted && silent_while_repeating && swung_again,
    );
}

// =============================================================================================
// 3. combat.mode.the-toggle-out-of-combat-reaches-the-shard
// =============================================================================================

/// Leaving combat tells the shard once and leaves nothing parked.
pub fn leaving_combat_reaches_the_shard() {
    let mut c = HeadlessClient::model();
    a_player_facing_a_monster(&mut c);
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        description(),
    ))));

    let mut out = RecordingRequests::default();
    let mut sink = dereth_client_model::NullSink;
    let w = c.world_mut();
    let (to, _refusal) = w.toggle_combat_mode_target(false);
    let leaves_combat = to == CombatMode::NonCombat;
    let ready = ready_for_mode_change(w);
    let _ = w.set_combat_mode(&mut out, &mut sink, to, true, ready, false);
    // Ten frames of the retry, so a defect that merely delays the send by a frame does not read
    // as this one.
    for _ in 0..10 {
        let ready = ready_for_mode_change(w);
        w.combat_use_time(&mut out, &mut sink, ready);
    }

    let wire = on_the_wire(&out);
    let sent_once = opcodes(&wire) == [CHANGE_COMBAT_MODE] && {
        let mut r = Reader::new(&wire[0].body);
        dereth_protocol::combat::CombatChangeCombatMode::read(&mut r)
            .expect("the body decodes")
            .combat_mode
            == CombatMode::NonCombat.raw()
    };

    c.assert_behaviour(
        "combat.mode.the-toggle-out-of-combat-reaches-the-shard",
        move |v| {
            leaves_combat
                && sent_once
                && v.world().combat.combat_mode == CombatMode::NonCombat
                && v.world().combat.pending_combat_mode == CombatMode::Undef
        },
    );
}

// =============================================================================================
// 4. combat.mode.whether-the-player-may-change-is-decided-by-the-mode-he-is-leaving
// =============================================================================================

/// The stance a body carries before any weapon is drawn. **Not** one of the missile-ready styles,
/// which is the whole of the discriminating case below.
const NONCOMBAT_STANCE: u32 = 0x8000_003D;

/// A player in one combat mode, carrying the combat table a real character is born with, and an
/// inventory under which every mode used below is compatible -- so no answer here can be a
/// compatibility refusal in disguise.
fn a_player_in(mode: CombatMode, style: u32) -> World {
    use dereth_client_model::inventory::slots::loc;
    let mut w = World::new();
    w.player = Some(PLAYER);
    let mut me = dereth_client_model::Weenie::new(PLAYER);
    me.pwd.name = "Aldis".to_owned();
    me.qualities
        .get_or_insert_with(dereth_client_model::Qualities::new)
        .set(
            dereth_client_model::StatKey::new(dereth_client_model::StatType::Did, COMBAT_TABLE_DID),
            dereth_client_model::StatValue::Did(DataId(COMBAT_TABLE)),
        );
    w.tables.weenies.insert(PLAYER, me);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    w.inventory_mask = loc::MISSILE_WEAPON | loc::MELEE_WEAPON | loc::HELD;
    w.combat.combat_mode = mode;
    w.combat.current_style = style;
    w
}

/// Everything one attempt to change mode produced, so a refusal and a success are read the same
/// way and neither is inferred from the absence of the other.
struct Attempt {
    mode: CombatMode,
    pending: CombatMode,
    sent: Vec<u32>,
}

fn attempt(w: &mut World, to: CombatMode, ready: bool) -> Attempt {
    let mut req = RecordingRequests::default();
    let mut sink = dereth_client_model::NullSink;
    let _ = w.set_combat_mode(&mut req, &mut sink, to, true, ready, false);
    let sent = on_the_wire(&req)
        .into_iter()
        .filter(|b| b.opcode == CHANGE_COMBAT_MODE)
        .map(|b| {
            let mut r = Reader::new(&b.body);
            dereth_protocol::combat::CombatChangeCombatMode::read(&mut r)
                .expect("the body decodes")
                .combat_mode
        })
        .collect();
    Attempt {
        mode: w.combat.combat_mode,
        pending: w.combat.pending_combat_mode,
        sent,
    }
}

/// The body is idle: this is the answer a real body settled on the terrain gives the predicate,
/// stated here rather than paid for with a landblock.
const MOTIONS_DRAINED: Option<bool> = Some(false);

fn may_change_mode(w: &World) -> bool {
    w.player_in_ready_position(false, MOTIONS_DRAINED)
}

/// A request to stand down is judged by the mode being left, not the one being asked for.
pub fn leaving_a_mode_is_judged_by_the_mode_being_left() {
    // **The premise**, without which the measurement below is a coincidence: the two modes really
    // do answer differently for the same body. A missile mode whose stance is not up refuses;
    // out-of-combat, the mode being *requested*, would have allowed it.
    let leaving = a_player_in(CombatMode::Missile, NONCOMBAT_STANCE);
    let requested = a_player_in(CombatMode::NonCombat, NONCOMBAT_STANCE);
    let they_disagree = !may_change_mode(&leaving) && may_change_mode(&requested);

    // **The difference.** The client took the answer for the mode being left: the change is held
    // in the pending slot, the mode does not move, and the shard hears nothing.
    let mut c = HeadlessClient::model();
    *c.world_mut() = a_player_in(CombatMode::Missile, NONCOMBAT_STANCE);
    let held = {
        let w = c.world_mut();
        let ready = may_change_mode(w);
        attempt(w, CombatMode::NonCombat, ready)
    };

    // **The control.** A test whose success condition is "nothing happened" scores a broken
    // fixture as a pass: raise the weapon stance, change nothing else, and the identical request
    // must go all the way through.
    let mut up = a_player_in(CombatMode::Missile, MISSILE_READY_STYLES[0]);
    let stance_is_up = may_change_mode(&up);
    let went = attempt(&mut up, CombatMode::NonCombat, stance_is_up);

    c.assert_behaviour(
        "combat.mode.whether-the-player-may-change-is-decided-by-the-mode-he-is-leaving",
        move |v| {
            they_disagree
                && held.mode == CombatMode::Missile
                && held.pending == CombatMode::NonCombat
                && held.sent.is_empty()
                && stance_is_up
                && went.mode == CombatMode::NonCombat
                && went.pending == CombatMode::Undef
                && went.sent == vec![CombatMode::NonCombat.raw()]
                // and the client the scenario drove is the one that held the change back.
                && v.world().combat.pending_combat_mode == CombatMode::NonCombat
                && v.outbound_opcodes().is_empty()
        },
    );
}

// -------------------------------------------------------------------------------------------

#[test]
fn scenario_readiness_comes_from_the_combat_table() {
    scenario("readiness_comes_from_the_combat_table");
}

#[test]
fn scenario_a_melee_attack_swings_and_repeats() {
    scenario("a_melee_attack_swings_and_repeats");
}

#[test]
fn scenario_leaving_combat_reaches_the_shard() {
    scenario("leaving_combat_reaches_the_shard");
}

#[test]
fn scenario_leaving_a_mode_is_judged_by_the_mode_being_left() {
    scenario("leaving_a_mode_is_judged_by_the_mode_being_left");
}
