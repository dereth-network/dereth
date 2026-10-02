//! Vectors: fixtures/vectors/trade/
//! Trade flow and confirmations; trade-item validity replays ACE trade vectors.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::DotNetHashSet;
use empyrean_common::vectors::{self, Case};
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CombatMode, ConfirmationType, EndTradeReason, MotionCommand, MotionStance, PropertyInstanceId,
    PropertyInt, PropertyString, WeenieError, WeenieType,
};
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::models::shard::Character;
use empyrean_testkit::land;
use empyrean_world::dispatch::{self, Class};
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::confirmation::Confirmation;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::player_manager::OnlinePlayer;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::managers::confirmation_manager::{
    self as cm, ConfirmationManager,
};
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{container, creature_combat, player_magic, player_trade};
use empyrean_world::World;

// ------------------------------------------------------------------------------------ harness

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

const LB: u32 = 0xA9B4_0000;
const A: ObjectGuid = ObjectGuid::new(0x5000_0001);
const B: ObjectGuid = ObjectGuid::new(0x5000_0002);

const GEM: u32 = 3;
const UNIQUE: u32 = 4; // Unique 1
const ESSENCE: u32 = 5; // a pet device

fn content() -> MemContent {
    MemContent::new()
        .weenie(
            Weenie::new(GEM, "gem", WeenieType::Generic)
                .with_string(PropertyString::Name, "Gem")
                .with_int(PropertyInt::EncumbranceVal, 5),
        )
        .weenie(
            Weenie::new(UNIQUE, "orb", WeenieType::Generic)
                .with_string(PropertyString::Name, "Orb")
                .with_int(PropertyInt::Unique, 1),
        )
        .weenie(
            Weenie::new(ESSENCE, "essence", WeenieType::PetDevice)
                .with_string(PropertyString::Name, "Essence"),
        )
}

struct H {
    w: World,
    clock: VirtualClock,
    next_session: u16,
}

impl H {
    fn new() -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats");
        let mut w = World::new(now, dats);
        w.timers = timers;
        w.content = Arc::new(content());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        pm::initialize(&mut w, true);
        lm::get_landblock(&mut w, LandblockId::new(LB | 0xFFFF), false, false);
        H {
            w,
            clock,
            next_session: 1,
        }
    }

    fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
    }

    /// An online player with a session and a Character on the landblock at (x, 20).
    fn player(&mut self, guid: ObjectGuid, name: &str, x: f32) -> SessionId {
        let mut o = WorldObject::allocate(Class::Player);
        o.guid = guid;
        o.biota.id = guid.full();
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(
            empyrean_entity::enums::PropertyDataId::Setup,
            land::TEST_SETUP,
        );
        o.set_property(PropertyInt::ItemsCapacity, 102);
        o.set_property(PropertyInt::ContainersCapacity, 7);
        o.set_encumbrance_val(Some(0));
        o.set_location(Some(Position::from_components(
            LB | 0x0001,
            x,
            20.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        o.set_heartbeat_interval(Some(0.0));
        empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
            &mut o,
            self.w.now.unix_time,
        );
        o.player.as_mut().expect("a player").player.character = Some(Character::default());
        o.wo.world_object_properties.current_motion_state = Some(Motion::new(
            MotionStance::NonCombat,
            MotionCommand::Ready,
            1.0,
        ));
        self.w.objects.insert(o).expect("fresh guid");

        let session = SessionId {
            client_id: self.next_session,
            generation: 1,
        };
        self.next_session += 1;
        self.w.sessions.insert(
            session,
            SessionData {
                player: Some(guid),
                ..SessionData::default()
            },
        );
        self.w.player_manager.online_players.insert(
            guid.full(),
            OnlinePlayer {
                guid,
                account: None,
            },
        );
        assert!(
            lm::add_object(&mut self.w, guid, false),
            "placed on the landblock"
        );
        session
    }

    /// A new object of `wcid` in `player`'s main pack.
    fn give(&mut self, player: ObjectGuid, wcid: u32) -> ObjectGuid {
        let weenie = self.w.content.get_cached_weenie(wcid).expect("test weenie");
        let guid = gm::new_dynamic_guid(&mut self.w);
        let o = CtorEnv::with_world(&self.w, |env| {
            factory::create_world_object(env, Some(weenie), guid)
        })
        .expect("constructible");
        self.w.objects.insert(o).expect("fresh");
        assert!(container::try_add_to_inventory(
            &mut self.w,
            player,
            guid,
            0,
            false,
            false
        ));
        guid
    }

    /// Alpha and Bravo side by side with a registered trade.
    fn trading() -> (Self, SessionId, SessionId) {
        let mut h = H::new();
        let sa = h.player(A, "Alpha", 20.0);
        let sb = h.player(B, "Bravo", 20.5);
        player_trade::handle_action_open_trade_negotiations(&mut h.w, B, A.full(), false);
        (h, sa, sb)
    }
}

/// `(session, opcode, game event type or 0, bytes)` of each captured send.
type Sent = (SessionId, u32, u32, Vec<u8>);

fn sent() -> Vec<Sent> {
    take_sent()
        .into_iter()
        .map(|(s, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let ev = if op == GameMessageOpcode::GameEvent.0 {
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            } else {
                0
            };
            (s, op, ev, b)
        })
        .collect()
}

fn string16l(b: &[u8], at: usize) -> String {
    let len = usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    String::from_utf8(b[at + 2..at + 2 + len].to_vec()).unwrap()
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The game event types `session` got, in order.
fn events_to(msgs: &[Sent], session: SessionId) -> Vec<GameEventType> {
    msgs.iter()
        .filter(|m| m.0 == session && m.2 != 0)
        .map(|m| GameEventType(m.2))
        .collect()
}

/// The first event body word (after the 16-byte header) of each event of `ty` to `session`.
fn first_words(msgs: &[Sent], session: SessionId, ty: GameEventType) -> Vec<u32> {
    msgs.iter()
        .filter(|m| m.0 == session && m.2 == ty.0)
        .map(|m| word(&m.3, 16))
        .collect()
}

fn transients_to(msgs: &[Sent], session: SessionId) -> Vec<String> {
    msgs.iter()
        .filter(|m| m.0 == session && m.2 == GameEventType::CommunicationTransientString.0)
        .map(|m| string16l(&m.3, 16))
        .collect()
}

fn chats_to(msgs: &[Sent], session: SessionId) -> Vec<String> {
    msgs.iter()
        .filter(|m| m.0 == session && m.1 == GameMessageOpcode::ServerMessage.0)
        .map(|m| string16l(&m.3, 4))
        .collect()
}

fn errors_to(msgs: &[Sent], session: SessionId) -> Vec<u32> {
    first_words(msgs, session, GameEventType::WeenieError)
}

fn manager(h: &H, g: ObjectGuid) -> &ConfirmationManager {
    &h.w.objects
        .get(g)
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .confirmation_manager
}

// ------------------------------------------------------------------------------------ vectors

/// `IsAttunedOrContainsAttuned`, `IsUniqueOrContainsUnique` and
/// `IsBeingTradedOrContainsItemBeingTraded` over ACE's cases: loose items and packs holding items,
/// with every Attuned and Unique value, some traded.
#[test]
fn trade_item_checks_match_ace() {
    let file = vectors::load_named("trade", "item_checks");
    let mut h = H::new();
    let mut failures = Vec::new();
    for Case { input, output, .. } in &file.cases {
        let make = |h: &mut H, v: &serde_json::Value, class: Class| -> ObjectGuid {
            let mut o = WorldObject::allocate(class);
            o.guid = gm::new_dynamic_guid(&mut h.w);
            if let Some(a) = v["attuned"].as_i64() {
                o.set_property(PropertyInt::Attuned, i32::try_from(a).unwrap());
            }
            if let Some(u) = v["unique"].as_i64() {
                o.set_property(PropertyInt::Unique, i32::try_from(u).unwrap());
            }
            if class == Class::Container {
                o.set_property(PropertyInt::ItemsCapacity, 24);
            }
            let g = o.guid;
            h.w.objects.insert(o).expect("fresh");
            g
        };
        let is_pack = input["contents"].is_array();
        let root = make(
            &mut h,
            input,
            if is_pack {
                Class::Container
            } else {
                Class::GenericObject
            },
        );
        let mut all = vec![root];
        for c in input["contents"].as_array().into_iter().flatten() {
            let g = make(&mut h, c, Class::GenericObject);
            assert!(container::try_add_to_inventory(
                &mut h.w, root, g, 0, false, false
            ));
            all.push(g);
        }
        let traded: DotNetHashSet<ObjectGuid> = input["traded"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| all[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();

        let got = serde_json::json!([
            dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(&h.w, root),
            dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(&h.w, root),
            dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(&h.w, root, &traded),
        ]);
        if &got != output {
            failures.push(format!("{input}: expected {output} got {got}"));
        }
        for g in all.into_iter().rev() {
            h.w.objects.remove(g);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        file.cases.len(),
        failures.join("\n")
    );
}

// ------------------------------------------------------------------------------------ trade

/// `HandleActionOpenTradeNegotiations` (the partner side, which the initiator's MoveTo callback
/// runs): both players trade with each other; a third party is refused `TradeAlreadyTrading`; a
/// player in combat mode `TradeNonCombatMode`.
#[test]
fn opening_registers_both_and_refuses_busy_or_combat() {
    let mut h = H::new();
    let sa = h.player(A, "Alpha", 20.0);
    let sb = h.player(B, "Bravo", 20.5);
    let c = ObjectGuid::new(0x5000_0003);
    let sc = h.player(c, "Charlie", 21.0);

    creature_combat::set_combat_mode_field(&mut h.w, c, CombatMode::Melee);
    start_capture();
    player_trade::handle_action_open_trade_negotiations(&mut h.w, A, c.full(), true);
    assert_eq!(
        errors_to(&sent(), sa),
        [WeenieError::TradeNonCombatMode.0.cast_unsigned()]
    );
    creature_combat::set_combat_mode_field(&mut h.w, c, CombatMode::NonCombat);

    player_trade::handle_action_open_trade_negotiations(&mut h.w, B, A.full(), false);
    let msgs = sent();
    assert_eq!(events_to(&msgs, sb), [GameEventType::RegisterTrade]);
    assert!(events_to(&msgs, sa).is_empty());
    let reg = msgs.iter().find(|m| m.0 == sb).unwrap();
    assert_eq!(
        (word(&reg.3, 16), word(&reg.3, 20), &reg.3[24..32]),
        (A.full(), A.full(), &[0u8; 8][..])
    );
    assert_eq!(player_trade::trade_partner(&h.w, A), B);
    assert_eq!(player_trade::trade_partner(&h.w, B), A);

    player_trade::handle_action_open_trade_negotiations(&mut h.w, c, A.full(), true);
    assert_eq!(
        errors_to(&sent(), sc),
        [WeenieError::TradeAlreadyTrading.0.cast_unsigned()]
    );
    assert!(!player_trade::is_trading(&h.w, c));
}

/// `GameActionCloseTradeNegotiations` → `HandleActionCloseTradeNegotiations` on both: CloseTrade
/// (Normal) then WeenieError TradeClosed each, state cleared; a transfer in progress blocks it.
#[test]
fn closing_clears_both_sides_unless_a_transfer_is_in_progress() {
    let (mut h, sa, sb) = H::trading();
    let gem = h.give(A, GEM);
    player_trade::handle_action_add_to_trade(&mut h.w, A, gem.full(), 0);

    h.w.objects
        .get_mut(A)
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_trade
        .trade_transfer_in_progress = true;
    start_capture();
    player_trade::handle_action_close_trade_negotiations(&mut h.w, A, EndTradeReason::Normal);
    assert!(
        events_to(&sent(), sa).is_empty(),
        "a transfer in progress ignores the close"
    );
    assert!(player_trade::is_trading(&h.w, A));
    h.w.objects
        .get_mut(A)
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_trade
        .trade_transfer_in_progress = false;

    for (g, s) in [(A, sa), (B, sb)] {
        player_trade::handle_action_close_trade_negotiations(&mut h.w, g, EndTradeReason::Normal);
        let msgs = sent();
        assert_eq!(
            events_to(&msgs, s),
            [GameEventType::CloseTrade, GameEventType::WeenieError]
        );
        assert_eq!(first_words(&msgs, s, GameEventType::CloseTrade), [1]);
        assert_eq!(
            errors_to(&msgs, s),
            [WeenieError::TradeClosed.0.cast_unsigned()]
        );
        assert!(!player_trade::is_trading(&h.w, g));
        assert_eq!(player_trade::trade_partner(&h.w, g), ObjectGuid::INVALID);
        assert!(player_trade::items_in_trade_window(&h.w, g).is_empty());
    }
}

/// `HandleActionResetTrade`: the window empties, the accept clears, `ResetTrade(whoReset)`.
/// `HandleActionDeclineTrade`: the decliner's accept clears; both get DeclineTrade naming the
/// decliner and "Trade confirmation failed...".
#[test]
fn reset_and_decline() {
    let (mut h, sa, sb) = H::trading();
    let gem = h.give(A, GEM);
    player_trade::handle_action_add_to_trade(&mut h.w, A, gem.full(), 0);
    player_trade::handle_action_accept_trade(&mut h.w, A);
    assert!(player_trade::trade_accepted(&h.w, A));

    start_capture();
    player_trade::handle_action_reset_trade(&mut h.w, A, A);
    player_trade::handle_action_reset_trade(&mut h.w, B, A);
    let msgs = sent();
    assert_eq!(
        first_words(&msgs, sa, GameEventType::ResetTrade),
        [A.full()]
    );
    assert_eq!(
        first_words(&msgs, sb, GameEventType::ResetTrade),
        [A.full()]
    );
    assert!(player_trade::items_in_trade_window(&h.w, A).is_empty());
    assert!(!player_trade::trade_accepted(&h.w, A));

    player_trade::handle_action_accept_trade(&mut h.w, B);
    let _ = sent();
    player_trade::handle_action_decline_trade(&mut h.w, B, sb);
    let msgs = sent();
    for s in [sa, sb] {
        assert_eq!(
            events_to(&msgs, s),
            [
                GameEventType::DeclineTrade,
                GameEventType::CommunicationTransientString
            ]
        );
        assert_eq!(
            first_words(&msgs, s, GameEventType::DeclineTrade),
            [B.full()]
        );
        assert_eq!(transients_to(&msgs, s), ["Trade confirmation failed..."]);
    }
    assert!(!player_trade::trade_accepted(&h.w, B));
}

/// `HandleActionTradeSwitchToCombatMode`, reached from `Creature.SetCombatMode`'s callers: each
/// side gets TradeNonCombatMode, then the close with `EndTradeReason.EnteredCombat`.
#[test]
fn entering_combat_closes_the_trade_on_both_sides() {
    let (mut h, sa, sb) = H::trading();
    creature_combat::set_combat_mode_field(&mut h.w, A, CombatMode::Melee);
    start_capture();
    player_trade::handle_action_trade_switch_to_combat_mode(&mut h.w, A);
    let msgs = sent();
    for s in [sa, sb] {
        assert_eq!(
            events_to(&msgs, s),
            [
                GameEventType::WeenieError,
                GameEventType::CloseTrade,
                GameEventType::WeenieError
            ]
        );
        assert_eq!(
            errors_to(&msgs, s),
            [
                WeenieError::TradeNonCombatMode.0.cast_unsigned(),
                WeenieError::TradeClosed.0.cast_unsigned()
            ]
        );
        assert_eq!(first_words(&msgs, s, GameEventType::CloseTrade), [2]);
    }
    assert!(!player_trade::is_trading(&h.w, A) && !player_trade::is_trading(&h.w, B));

    // not trading: nothing
    player_trade::handle_action_trade_switch_to_combat_mode(&mut h.w, A);
    assert!(sent().is_empty());
}

/// `HandleActionAddToTrade`'s refusals: a summoned pet's essence ("You must unsummon your pet
/// ..." + TradeFailure AttunedItem); a unique the partner already holds (TradeFailure None, and
/// `CheckUniques` tells the giver); an item the player does not have (nothing).
#[test]
fn add_to_trade_refuses_summoned_pets_uniques_and_missing_items() {
    let (mut h, sa, sb) = H::trading();
    let essence = h.give(A, ESSENCE);
    h.w.objects
        .get_mut(essence)
        .unwrap()
        .set_property(PropertyInstanceId::Pet, 0x8000_0999);
    let orb = h.give(A, UNIQUE);
    let _held = h.give(B, UNIQUE);

    start_capture();
    player_trade::handle_action_add_to_trade(&mut h.w, A, essence.full(), 0);
    let msgs = sent();
    assert_eq!(
        events_to(&msgs, sa),
        [
            GameEventType::CommunicationTransientString,
            GameEventType::TradeFailure
        ]
    );
    assert_eq!(
        transients_to(&msgs, sa),
        ["You must unsummon your pet before you can trade this item!"]
    );
    let failure = msgs
        .iter()
        .find(|m| m.2 == GameEventType::TradeFailure.0)
        .unwrap();
    assert_eq!(
        (word(&failure.3, 16), word(&failure.3, 20)),
        (essence.full(), WeenieError::AttunedItem.0.cast_unsigned())
    );

    player_trade::handle_action_add_to_trade(&mut h.w, A, orb.full(), 0);
    let msgs = sent();
    // `CheckUniques(wo, giver)` tells the giver, with the TooManyUniqueItems error (V296).
    assert!(chats_to(&msgs, sa).is_empty());
    assert_eq!(
        errors_to(&msgs, sa),
        [WeenieError::TooManyUniqueItems.0.cast_unsigned()]
    );
    let failure = msgs
        .iter()
        .find(|m| m.2 == GameEventType::TradeFailure.0)
        .unwrap();
    assert_eq!(
        (word(&failure.3, 16), word(&failure.3, 20)),
        (orb.full(), 0)
    );
    assert!(events_to(&msgs, sb).is_empty());

    player_trade::handle_action_add_to_trade(&mut h.w, A, 0x8000_7777, 0);
    player_trade::handle_action_add_to_trade(&mut h.w, A, 0, 0);
    assert!(sent().is_empty());
    assert!(player_trade::items_in_trade_window(&h.w, A).is_empty());
}

/// `VerifyTrade_BusyState`: the busy side hears "You are too busy", the other "Your trading
/// partner is too busy"; both acceptances clear (ClearTradeAcceptance each).
#[test]
fn a_busy_partner_stops_the_finalise() {
    let (mut h, sa, sb) = H::trading();
    player_trade::handle_action_accept_trade(&mut h.w, A);
    player_magic::set_is_busy(&mut h.w, A, true);
    start_capture();
    player_trade::handle_action_accept_trade(&mut h.w, B);
    let msgs = sent();
    assert_eq!(
        transients_to(&msgs, sb),
        [
            "You have accepted the offer",
            "Your trading partner is too busy to complete the trade!"
        ]
    );
    assert_eq!(
        transients_to(&msgs, sa),
        [
            "Bravo has accepted the offer",
            "You are too busy to complete the trade!"
        ]
    );
    assert_eq!(
        events_to(&msgs, sa).last(),
        Some(&GameEventType::ClearTradeAcceptance)
    );
    assert_eq!(
        events_to(&msgs, sb).last(),
        Some(&GameEventType::ClearTradeAcceptance)
    );
    assert!(!player_trade::trade_accepted(&h.w, A) && !player_trade::trade_accepted(&h.w, B));
}

/// `VerifyTrade_Inventory` / `GetItemsInTradeWindow`: an offered item that has left the offerer
/// makes the offerer decline (`partner.HandleActionDeclineTrade`).
#[test]
fn an_offered_item_that_is_gone_declines() {
    let (mut h, sa, sb) = H::trading();
    let gem = h.give(A, GEM);
    player_trade::handle_action_add_to_trade(&mut h.w, A, gem.full(), 0);
    player_trade::handle_action_accept_trade(&mut h.w, A);
    assert!(container::try_remove_from_inventory(
        &mut h.w, A, gem, false
    ));
    start_capture();
    player_trade::handle_action_accept_trade(&mut h.w, B);
    let msgs = sent();
    assert_eq!(
        first_words(&msgs, sa, GameEventType::DeclineTrade),
        [A.full()]
    );
    assert_eq!(
        first_words(&msgs, sb, GameEventType::DeclineTrade),
        [A.full()]
    );
    assert!(
        player_trade::trade_accepted(&h.w, B),
        "only the decliner's accept clears"
    );
    assert!(!player_trade::trade_transfer_in_progress(&h.w, B));
}

/// `AddKnownTradeObj`, `GetKnownTradeObj`, `PruneKnownTradeObjs`: the partner who offered an item
/// is found while its body is known and within 96 units; an unknown partner is pruned.
#[test]
fn known_trade_objects() {
    let (mut h, _, _) = H::trading();
    for _ in 0..3 {
        h.tick();
        h.advance(0.2);
    }
    let gem = h.give(A, GEM);
    player_trade::handle_action_add_to_trade(&mut h.w, A, gem.full(), 0);
    let known =
        &h.w.objects
            .get(B)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player_trade
            .known_trade_objs;
    assert_eq!(
        known
            .iter()
            .map(|(k, v)| (*k, v.iter().copied().collect::<Vec<_>>()))
            .collect::<Vec<_>>(),
        [(A, vec![gem])]
    );

    let found = player_trade::get_known_trade_obj(&mut h.w, B, gem);
    let a_known = {
        let hb = h.w.objects.get(B).unwrap().phys.unwrap();
        empyrean_world::physics::object_maint::get_known_object(&h.w, hb, A.full()).is_some()
    };
    assert_eq!(
        found,
        a_known.then_some(A),
        "found exactly when Bravo's body knows Alpha's"
    );
    assert_eq!(
        player_trade::get_known_trade_obj(&mut h.w, B, ObjectGuid::new(0x8000_1234)),
        None
    );

    // an entry for a player Bravo does not know is pruned
    player_trade::add_known_trade_obj(&mut h.w, B, ObjectGuid::new(0x5000_0099), gem);
    player_trade::prune_known_trade_objs(&mut h.w, B);
    let known =
        &h.w.objects
            .get(B)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player_trade
            .known_trade_objs;
    assert!(!known.contains_key(&ObjectGuid::new(0x5000_0099)));
    assert_eq!(known.contains_key(&A), a_known);
}

// ------------------------------------------------------------------------------------ confirmations

/// `EnqueueSend`: the request (type, context, text) is sent and a second one of the same type is
/// refused; context ids count up from 1 per player.
#[test]
fn enqueue_send_numbers_contexts_and_refuses_duplicates() {
    let mut h = H::new();
    let sa = h.player(A, "Alpha", 20.0);
    start_capture();
    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::yes_no(B, A, Some("quest")),
        "Sure?"
    ));
    assert!(!cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::yes_no(B, A, None),
        "Again?"
    ));
    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::alter_skill(A, B),
        "Skill?"
    ));
    let msgs = sent();
    let requests: Vec<(u32, u32, String)> = msgs
        .iter()
        .filter(|m| m.0 == sa && m.2 == GameEventType::CharacterConfirmationRequest.0)
        .map(|m| (word(&m.3, 16), word(&m.3, 20), string16l(&m.3, 24)))
        .collect();
    assert_eq!(
        requests,
        [(7, 1, "Sure?".to_owned()), (2, 3, "Skill?".to_owned())],
        "the refused one still took context 2"
    );
    assert!(manager(&h, A).contains(ConfirmationType::Yes_No));
    assert_eq!(
        manager(&h, A)
            .get(ConfirmationType::AlterSkill)
            .map(|c| c.context_id),
        Some(3)
    );
}

/// `EnqueueAbort` after 30 s: ConfirmationDone always; a Yes_No (and the other client-answered
/// types) only says "You waited too long to answer the question!" and stays open until the
/// client answers; a SwearAllegiance is processed as a timeout ("did not respond to").
#[test]
fn timeouts_follow_the_type() {
    let mut h = H::new();
    let sa = h.player(A, "Alpha", 20.0);
    let sb = h.player(B, "Bravo", 20.5);
    start_capture();
    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::yes_no(B, A, None),
        "Sure?"
    ));
    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::swear_allegiance(A, B),
        "Bravo"
    ));
    let _ = sent();
    h.advance(29.5);
    h.tick();
    assert!(first_words(&sent(), sa, GameEventType::CharacterConfirmationDone).is_empty());
    h.advance(1.0);
    h.tick();
    h.tick();
    let msgs = sent();
    let done: Vec<u32> = first_words(&msgs, sa, GameEventType::CharacterConfirmationDone);
    assert_eq!(done, [7, 1]);
    assert_eq!(
        chats_to(&msgs, sa),
        ["You waited too long to answer the question!"]
    );
    assert_eq!(
        chats_to(&msgs, sb),
        ["Alpha did not respond to your offer of allegiance."]
    );
    assert!(
        manager(&h, A).contains(ConfirmationType::Yes_No),
        "left for the client's answer"
    );
    assert!(!manager(&h, A).contains(ConfirmationType::SwearAllegiance));

    // the client's answer to the aborted Yes_No closes it (no source object: nothing runs)
    assert!(cm::handle_response(
        &mut h.w,
        A,
        ConfirmationType::Yes_No,
        1,
        false,
        false
    ));
    assert!(!manager(&h, A).contains(ConfirmationType::Yes_No));
}

/// `HandleResponse`: a wrong context re-adds the confirmation and answers false; an unknown type
/// answers false; a custom action runs only on yes; a declined craft says YouChickenOut.
#[test]
fn responses_route_by_type_and_context() {
    let mut h = H::new();
    let sa = h.player(A, "Alpha", 20.0);
    let ran = Arc::new(Mutex::new(0));

    let r = ran.clone();
    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::custom(A, Box::new(move |_w: &mut World| *r.lock().unwrap() += 1)),
        "Keep?"
    ));
    start_capture();
    assert!(
        !cm::handle_response(&mut h.w, A, ConfirmationType::Yes_No, 9, true, false),
        "wrong context"
    );
    assert!(
        manager(&h, A).contains(ConfirmationType::Yes_No),
        "re-added"
    );
    assert!(
        !cm::handle_response(
            &mut h.w,
            A,
            ConfirmationType::AlterAttribute,
            1,
            true,
            false
        ),
        "none open"
    );
    assert!(cm::handle_response(
        &mut h.w,
        A,
        ConfirmationType::Yes_No,
        1,
        true,
        false
    ));
    assert_eq!(*ran.lock().unwrap(), 1);

    let r = ran.clone();
    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::custom(A, Box::new(move |_w: &mut World| *r.lock().unwrap() += 1)),
        "Keep?"
    ));
    assert!(cm::handle_response(
        &mut h.w,
        A,
        ConfirmationType::Yes_No,
        2,
        false,
        false
    ));
    assert_eq!(*ran.lock().unwrap(), 1, "no runs nothing");

    assert!(cm::enqueue_send(
        &mut h.w,
        A,
        Confirmation::craft_interation(A, B, B),
        "Craft?"
    ));
    let _ = sent();
    assert!(cm::handle_response(
        &mut h.w,
        A,
        ConfirmationType::CraftInteraction,
        3,
        false,
        false
    ));
    assert_eq!(
        errors_to(&sent(), sa),
        [WeenieError::YouChickenOut.0.cast_unsigned()]
    );
}

/// Divergence: V419
/// A world without secure trade refuses opening one: no trade registers on either side and the
/// player is told why; February 2005 had trade, so an Infiltration world opens it.
#[test]
fn a_world_without_trade_refuses_opening_one() {
    use empyrean_common::era::{with_features, EraExt as _, EraFeatures, EraId};
    let mut h = H::new();
    let sa = h.player(A, "Alpha", 20.0);
    let sb = h.player(B, "Bravo", 20.5);
    h.w.era = with_features(
        EraId::Eor.rules(),
        EraFeatures {
            trade: false,
            ..EraFeatures::ALL
        },
    );
    start_capture();
    player_trade::handle_action_open_trade_negotiations(&mut h.w, A, B.full(), true);
    let msgs = sent();
    assert!(events_to(&msgs, sa).is_empty() && events_to(&msgs, sb).is_empty());
    assert_eq!(chats_to(&msgs, sa), ["This world has no secure trade."]);
    assert!(!player_trade::is_trading(&h.w, A) && !player_trade::is_trading(&h.w, B));

    h.w.era = EraId::Infiltration.rules();
    player_trade::handle_action_open_trade_negotiations(&mut h.w, B, A.full(), false);
    assert_eq!(events_to(&sent(), sb), [GameEventType::RegisterTrade]);
    assert!(player_trade::is_trading(&h.w, A));
}
