//! Vectors: fixtures/vectors/use/
//! Using doors, chests, locks, keys, food, healers, gems; lockpick and heal checks replay ACE use
//! vectors.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, u64_of, Case};
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    HeritageGroup, ItemType, MotionCommand, PlayerKillerStatus, PropertyAttribute,
    PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyString,
    Skill, SkillAdvancementClass, Sound, Usable, WeenieError, WeenieType,
};
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_testkit::land;
use empyrean_world::dispatch::{self, Class};
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::lock::{self, UnlockResults};
use empyrean_world::world_objects::managers::enchantment_manager_with_caching as emc;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    container, door, food, gem, healer, player_use, skill_check, world_object_use,
};
use empyrean_world::World;

// ------------------------------------------------------------------------------------ vectors

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("use", name);
    assert!(!file.cases.is_empty(), "use/{name}: no cases");
    let failures: Vec<String> = file
        .cases
        .iter()
        .filter_map(|c| {
            each(c)
                .err()
                .map(|got| format!("in {} expected {} got {got}", c.input, c.output))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "use/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

fn u32_in(c: &Case, key: &str) -> u32 {
    u32::try_from(u64_of(&c.input[key]).unwrap_or_else(|| panic!("in.{key}"))).unwrap()
}

fn sac_of(v: i64) -> SkillAdvancementClass {
    SkillAdvancementClass(u32::try_from(v).unwrap())
}

/// `UnlockerHelper.GetEffectiveLockpickSkill` and the pick chance, against ACE (500 cases).
#[test]
fn lockpick_skill_matches_ace() {
    replay("lockpick_skill", |c| {
        let current = u32_in(c, "current");
        let add = i64_of(&c.input["add"]).map_or(0, |a| i32::try_from(a).unwrap());
        let mult = f64_of(&c.input["mult"]).unwrap_or(f64::from(1.0f32));
        let difficulty = i32::try_from(i64_of(&c.input["difficulty"]).unwrap()).unwrap();

        let effective = lock::effective_lockpick_skill(current, add, mult);
        let chance = skill_check::get_skill_chance(
            i32::try_from(effective).unwrap(),
            difficulty,
            skill_check::DEFAULT_FACTOR,
        );
        let want = (
            u32::try_from(u64_of(&c.output[0]).unwrap()).unwrap(),
            f64_of(&c.output[1]).unwrap(),
        );
        if (effective, chance.to_bits()) == (want.0, want.1.to_bits()) {
            Ok(())
        } else {
            Err(format!("[{effective}, {chance}]"))
        }
    });
}

/// `Healer.DoSkillCheck`: the result, the difficulty and the one draw, against ACE (600 cases).
#[test]
fn heal_skill_check_matches_ace() {
    replay("heal_skill_check", |c| {
        ThreadSafeRandom::seed(u64_of(&c.input["seed"]).unwrap());
        let sac = sac_of(i64_of(&c.input["sac"]).unwrap());
        let boost = i64_of(&c.input["boost"]).map_or(0, |b| i32::try_from(b).unwrap());
        let non_combat = i64_of(&c.input["combat"]).unwrap() == 1;
        let mut difficulty = 0;
        let result = healer::skill_check_roll(
            u32_in(c, "current"),
            sac,
            boost,
            non_combat,
            u32_in(c, "missing"),
            &mut difficulty,
        );
        let next = ThreadSafeRandom::next(0, 1_000_000);
        let got = serde_json::json!([result, difficulty, next]);
        if got == c.output {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

/// `Healer.GetHealAmount`: the amount, the critical, the stamina cost and the two draws, against
/// ACE (600 cases; the target's healing rating is ACE's, an input here).
#[test]
fn heal_amount_matches_ace() {
    replay("heal_amount", |c| {
        ThreadSafeRandom::seed(u64_of(&c.input["seed"]).unwrap());
        let rating = f32_of(&c.input["rating"]).unwrap();
        let (amount, critical, cost) = healer::heal_amount(
            u32_in(c, "current"),
            f64_of(&c.input["healkit"]).unwrap(),
            u32_in(c, "missing"),
            u32_in(c, "stamina"),
            || rating,
        );
        let next = ThreadSafeRandom::next(0, 1_000_000);
        let got = serde_json::json!([amount, critical, cost, next]);
        if got == c.output {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

/// `Food.BoostVital`: `(int)Math.Round(BoostValue * ratingMod)`, an int times a float, rounded
/// half to even (Food.cs).
#[test]
fn food_boost_rounds_the_float_product_half_to_even() {
    assert_eq!(food::boost_vital_amount(10, 1.0), 10);
    // 25 * 1.1f is exactly 27.5 in float: to even
    assert_eq!(food::boost_vital_amount(25, 1.1), 28);
    assert_eq!(food::boost_vital_amount(35, 1.1), 38);
    assert_eq!(food::boost_vital_amount(-15, 1.0), -15);
    assert_eq!(food::boost_vital_amount(7, 0.5), 4);
    assert_eq!(food::boost_vital_amount(5, 0.5), 2);
}

// ------------------------------------------------------------------------------------ world

const LB: u32 = 0xA9B4_0000;
const P: ObjectGuid = ObjectGuid::new(0x5000_0001);
const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

const PLAYER_WCID: u32 = 1;
const DOOR: u32 = 10;
const CHEST: u32 = 11;
const KEY: u32 = 12;
const LOCKPICK: u32 = 13;
const APPLE: u32 = 14;
const KIT: u32 = 15;
const RARE: u32 = 16;
const TORCH: u32 = 17; // a generic object (GenericObject.ActOnUse plays its UseSound)
const BOX: u32 = 18; // a plain container
const SHIRT: u32 = 19; // clothing: no ActOnUse override

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> ContentWeenie {
    ContentWeenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(weenie(DOOR, "Door", WeenieType::Door))
        .weenie(
            weenie(CHEST, "Chest", WeenieType::Chest)
                .with_float(PropertyFloat::ResetInterval, 60.0),
        )
        .weenie(
            weenie(KEY, "Key", WeenieType::Key)
                .with_string(PropertyString::KeyCode, "OAK")
                .with_int(PropertyInt::Structure, 3)
                .with_int(PropertyInt::MaxStructure, 3)
                .with_int(PropertyInt::Value, 30)
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Key.0).unwrap(),
                ),
        )
        .weenie(
            weenie(LOCKPICK, "Lockpick", WeenieType::Lockpick)
                .with_int(PropertyInt::Structure, 1)
                .with_int(PropertyInt::MaxStructure, 1)
                .with_int(PropertyInt::Value, 10),
        )
        .weenie(
            weenie(APPLE, "Apple", WeenieType::Food)
                .with_int(
                    PropertyInt::BoosterEnum,
                    i32::from(PropertyAttribute2nd::Health.0),
                )
                .with_int(PropertyInt::BoostValue, 10)
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 1),
        )
        .weenie(
            weenie(KIT, "Healing Kit", WeenieType::Healer)
                .with_int(
                    PropertyInt::BoosterEnum,
                    i32::from(PropertyAttribute2nd::Health.0),
                )
                .with_int(PropertyInt::BoostValue, 50)
                .with_float(PropertyFloat::HealkitMod, 1.0)
                .with_int(PropertyInt::Structure, 2)
                .with_int(PropertyInt::MaxStructure, 10)
                .with_int(PropertyInt::Value, 100)
                .with_int(
                    PropertyInt::TargetType,
                    i32::try_from(ItemType::Creature.0).unwrap(),
                ),
        )
        .weenie(
            weenie(RARE, "Rare Gem", WeenieType::Gem).with_bool(PropertyBool::RareUsesTimer, true),
        )
        .weenie(weenie(TORCH, "Torch", WeenieType::Generic))
        .weenie(weenie(SHIRT, "Shirt", WeenieType::Clothing))
        .weenie(weenie(BOX, "Box", WeenieType::Container).with_int(PropertyInt::ItemsCapacity, 10))
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

/// A world on flat land in virtual time, with the player P (session S, on the landblock at
/// 20,20, Healing and Lockpick trained, every attribute 100) and ACE's default server properties.
struct H {
    w: World,
    clock: VirtualClock,
}

impl H {
    fn new() -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let mut w = World::new(
            now,
            empyrean_testkit::dats::with_stat_tables(
                FakeDats::new().with_xp_table(empyrean_dat::fake::sample::xp_table()),
            )
            .build()
            .expect("fake dats"),
        );
        w.timers = timers;
        w.content = Arc::new(content());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(514);
        let mut h = H { w, clock };

        let weenie =
            h.w.content
                .get_cached_weenie(PLAYER_WCID)
                .expect("player weenie");
        let mut o = CtorEnv::with_world(&h.w, |env| {
            empyrean_world::world_objects::player::player_from_weenie(
                env,
                Class::Player,
                weenie,
                P,
                1,
            )
        });
        for a in [
            PropertyAttribute::Strength,
            PropertyAttribute::Endurance,
            PropertyAttribute::Coordination,
            PropertyAttribute::Quickness,
            PropertyAttribute::Focus,
            PropertyAttribute::Self_,
        ] {
            o.biota
                .properties_attribute
                .get_or_insert_with(Default::default)
                .get_or_insert_with(a, Default::default)
                .init_level = 100;
        }
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            let rec = o
                .biota
                .properties_attribute_2nd
                .get_or_insert_with(Default::default)
                .get_or_insert_with(v, Default::default);
            rec.init_level = 100;
            rec.current_level = 100;
        }
        for skill in [Skill::Healing, Skill::Lockpick] {
            let s = o.get_creature_skill(skill, true).unwrap();
            s.set_advancement_class(&mut o, SkillAdvancementClass::Trained);
            s.set_init_level(&mut o, 10);
        }
        o.set_encumbrance_val(Some(0));
        o.set_value(Some(0));
        o.set_location(Some(at(20.0, 20.0)));
        o.player.as_mut().unwrap().player.character =
            Some(empyrean_store::models::shard::Character::default());
        h.w.objects.insert(o).expect("fresh");
        let mut s = SessionData::default();
        s.set_account(
            7,
            "acct".to_owned(),
            empyrean_entity::enums::AccessLevel::Player,
        );
        s.set_player(Some(P));
        h.w.sessions.insert(S, s);
        lm::get_landblock(&mut h.w, LandblockId::new(LB | 0xFFFF), false, false);
        assert!(lm::add_object(&mut h.w, P, false), "the player is placed");
        h
    }

    fn spawn(&mut self, wcid: u32) -> ObjectGuid {
        let weenie = self.w.content.get_cached_weenie(wcid).expect("test weenie");
        let guid = gm::new_dynamic_guid(&mut self.w);
        let o = CtorEnv::with_world(&self.w, |env| {
            factory::create_world_object(env, Some(weenie), guid)
        })
        .expect("constructible");
        assert!(self.w.objects.insert(o).is_ok());
        guid
    }

    /// A new object of `wcid` on the landblock at (x, y).
    fn place(&mut self, wcid: u32, x: f32, y: f32) -> ObjectGuid {
        let g = self.spawn(wcid);
        self.o_mut(g).set_location(Some(at(x, y)));
        assert!(lm::add_object(&mut self.w, g, false));
        g
    }

    /// A new object of `wcid` in the player's pack.
    fn give(&mut self, wcid: u32) -> ObjectGuid {
        let g = self.spawn(wcid);
        assert!(container::try_add_to_inventory(
            &mut self.w,
            P,
            g,
            0,
            false,
            true
        ));
        g
    }

    fn o(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("live object")
    }

    fn o_mut(&mut self, g: ObjectGuid) -> &mut WorldObject {
        self.w.objects.get_mut(g).expect("live object")
    }

    /// Advances virtual time by `secs`, then one pass: the delay manager, then the landblocks.
    fn run(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn health(&self) -> (u32, u32) {
        let o = self.o(P);
        let h = o.health();
        (h.current(o), 0)
    }
}

/// What the world sent to the session S (opcode, game-event type or 0, bytes).
fn sent_to_s() -> Vec<Msg> {
    take_sent()
        .into_iter()
        .filter(|(s, _, _)| *s == S)
        .map(|(_, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let event = if op == GameMessageOpcode::GameEvent.0 {
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            } else {
                0
            };
            Msg { op, event, b }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct Msg {
    op: u32,
    event: u32,
    b: Vec<u8>,
}

impl Msg {
    fn is_event(&self, t: GameEventType) -> bool {
        self.op == GameMessageOpcode::GameEvent.0 && self.event == t.0
    }

    /// A `String16L` at `at`.
    fn string16l(&self, at: usize) -> String {
        let len = usize::from(u16::from_le_bytes([self.b[at], self.b[at + 1]]));
        String::from_utf8(self.b[at + 2..at + 2 + len].to_vec()).unwrap()
    }

    fn u32_at(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.b[at..at + 4].try_into().unwrap())
    }
}

/// The UseDone events' error codes.
fn use_dones(m: &[Msg]) -> Vec<u32> {
    m.iter()
        .filter(|m| m.is_event(GameEventType::UseDone))
        .map(|m| m.u32_at(16))
        .collect()
}

/// The transient strings.
fn transients(m: &[Msg]) -> Vec<String> {
    m.iter()
        .filter(|m| m.is_event(GameEventType::CommunicationTransientString))
        .map(|m| m.string16l(16))
        .collect()
}

/// The system chat lines.
fn chats(m: &[Msg]) -> Vec<String> {
    m.iter()
        .filter(|m| m.op == GameMessageOpcode::ServerMessage.0)
        .map(|m| m.string16l(4))
        .collect()
}

/// The sounds (source guid, sound).
fn sounds(m: &[Msg]) -> Vec<(u32, u32)> {
    m.iter()
        .filter(|m| m.op == GameMessageOpcode::Sound.0)
        .map(|m| (m.u32_at(4), m.u32_at(8)))
        .collect()
}

fn ops(m: &[Msg]) -> Vec<u32> {
    m.iter()
        .map(|m| if m.event != 0 { m.event } else { m.op })
        .collect()
}

fn we(e: WeenieError) -> u32 {
    u32::try_from(e.0).unwrap()
}

// ------------------------------------------------------------------------------------ WorldObject_Use

/// `WorldObject.CheckUseRequirements`: the player level, heritage and cooldown refusals, each with
/// ACE's message; a non-player activator always passes (WorldObject_Use.cs).
#[test]
fn check_use_requirements_refuses_in_aces_order() {
    let mut h = H::new();
    let torch = h.give(TORCH);

    // level: "You are not high enough level to use that!" as the result's message (not sent yet)
    h.o_mut(torch)
        .set_property(PropertyInt::UseRequiresLevel, 10);
    h.o_mut(P).set_level(Some(5));
    start_capture();
    let r = world_object_use::world_object_check_use_requirements(&mut h.w, torch, P);
    assert!(!r.success);
    assert!(sent_to_s().is_empty(), "the message is returned, not sent");
    h.o_mut(P).set_level(Some(10));
    assert!(
        world_object_use::world_object_check_use_requirements(&mut h.w, torch, P).success,
        "Level == UseRequiresLevel passes"
    );

    // heritage, on a non-creature: YouMustBe_ToUseItemMagic with the group's sentence
    h.o_mut(torch)
        .set_property(PropertyInt::HeritageGroup, HeritageGroup::Sho.0);
    h.o_mut(P)
        .set_property(PropertyInt::HeritageGroup, HeritageGroup::Aluvian.0);
    let r = world_object_use::world_object_check_use_requirements(&mut h.w, torch, P);
    assert!(!r.success && r.message.is_some());
    h.o_mut(P)
        .set_property(PropertyInt::HeritageGroup, HeritageGroup::Sho.0);
    assert!(world_object_use::world_object_check_use_requirements(&mut h.w, torch, P).success);

    // cooldown: OnActivate starts it; a second check within it is refused with the transient
    h.o_mut(torch).set_property(PropertyInt::SharedCooldown, 77);
    h.o_mut(torch)
        .set_property(PropertyFloat::CooldownDuration, 30.0);
    assert!(emc::start_cooldown(&mut h.w, P, torch));
    start_capture();
    let r = world_object_use::world_object_check_use_requirements(&mut h.w, torch, P);
    assert!(!r.success && r.message.is_none());
    assert_eq!(
        transients(&sent_to_s()),
        ["You have used this item too recently"]
    );

    // a non-player activator
    let other = h.place(TORCH, 30.0, 30.0);
    assert!(world_object_use::world_object_check_use_requirements(&mut h.w, torch, other).success);
}

/// `WorldObject.OnActivate` -> `ActOnUse` on a class with no override: the error line to the
/// player; an inactive object does nothing (WorldObject_Use.cs).
#[test]
fn on_activate_runs_the_use_response_and_skips_an_inactive_object() {
    let mut h = H::new();
    let torch = h.give(SHIRT);
    h.o_mut(torch)
        .set_property(PropertyInt::ActivationResponse, 2); // Use
    start_capture();
    dispatch::on_activate::on_activate(&mut h.w, torch, P);
    let m = sent_to_s();
    let wcid = h.o(torch).biota.weenie_class_id;
    assert_eq!(
        chats(&m),
        [format!(
            "Shirt.ActOnUse(human) - undefined for wcid {wcid} type Clothing"
        )]
    );

    h.o_mut(torch).set_property(PropertyInt::Active, 0);
    start_capture();
    dispatch::on_activate::on_activate(&mut h.w, torch, P);
    assert!(sent_to_s().is_empty(), "Active false: nothing");
}

/// `GenericObject.ActOnUse`: a player hears the object's UseSound (none without one); nothing for
/// a non-player (GenericObject.cs).
#[test]
fn a_generic_object_plays_its_use_sound_to_the_player() {
    let mut h = H::new();
    let torch = h.give(TORCH);
    h.o_mut(torch)
        .set_property(PropertyInt::ActivationResponse, 2); // Use
    start_capture();
    dispatch::on_activate::on_activate(&mut h.w, torch, P);
    assert!(sent_to_s().is_empty(), "no UseSound: nothing");

    h.o_mut(torch).set_property(
        PropertyDataId::UseSound,
        empyrean_entity::enums::Sound::OpenFailDueToLock.0,
    );
    start_capture();
    dispatch::on_activate::on_activate(&mut h.w, torch, P);
    let m = sent_to_s();
    assert_eq!(
        sounds(&m),
        [(P.full(), empyrean_entity::enums::Sound::OpenFailDueToLock.0)]
    );
    assert!(chats(&m).is_empty(), "no ActOnUse error line");
}

// ------------------------------------------------------------------------------------ Player_Use

/// `Player.HandleActionUseItem`: an item in the pack is used at once and `UseDone` follows after
/// `LastUseTime` (0 here, so on the next pass); an unknown guid answers `UseDone` at once; a PK
/// logout refuses (Player_Use.cs).
#[test]
fn use_item_activates_then_use_done() {
    let mut h = H::new();
    let torch = h.give(SHIRT);
    h.o_mut(torch)
        .set_property(PropertyInt::ActivationResponse, 2);
    start_capture();
    player_use::handle_action_use_item(&mut h.w, P, torch.full());
    let m = sent_to_s();
    assert_eq!(chats(&m).len(), 1, "ActOnUse ran at once");
    assert!(use_dones(&m).is_empty(), "UseDone waits for the chain");
    h.run(0.1);
    assert_eq!(use_dones(&sent_to_s()), [0]);

    start_capture();
    player_use::handle_action_use_item(&mut h.w, P, 0x8000_7777);
    assert_eq!(use_dones(&sent_to_s()), [0], "couldn't find object");

    h.o_mut(P).player.as_mut().unwrap().player.pk_logout = true;
    start_capture();
    player_use::handle_action_use_item(&mut h.w, P, torch.full());
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::YouHaveBeenInPKBattleTooRecently)]
    );
}

/// `Player.HandleActionUseItem` on a landblock object out of reach while busy: YoureTooBusy; not
/// busy: a MoveTo chain, and the use happens when the player arrives (Player_Use.cs).
#[test]
fn use_item_out_of_reach_moves_to_it_first() {
    let mut h = H::new();
    let door = h.place(DOOR, 30.0, 20.0);
    h.o_mut(P).wo.world_object.is_busy = true;
    start_capture();
    player_use::handle_action_use_item(&mut h.w, P, door.full());
    assert_eq!(use_dones(&sent_to_s()), [we(WeenieError::YoureTooBusy)]);
    h.o_mut(P).wo.world_object.is_busy = false;

    start_capture();
    player_use::handle_action_use_item(&mut h.w, P, door.full());
    let m = sent_to_s();
    assert!(
        ops(&m).contains(&GameMessageOpcode::Motion.0),
        "the MoveToObject motion: {:04X?}",
        ops(&m)
    );
    assert!(!h.o(door).is_open(), "not in reach yet");
    assert!(empyrean_world::world_objects::player_move::is_player_moving_to(&h.w, P));

    // the player arrives: the chain's next 0.1 s check succeeds and the door opens
    let near = at(29.0, 20.0);
    h.o_mut(P).set_location(Some(near));
    let hp = h.o(P).phys.unwrap();
    empyrean_world::physics::phys_ext::set_position(
        &mut h.w,
        hp,
        &empyrean_world::physics::phys_ext::to_physics_position(&near),
    );
    assert!(
        world_object_use::is_within_use_radius_of(&h.w, P, door, None)
            || world_object_use::get_cylinder_distance(&h.w, P, door) < 1.0
    );
    h.run(0.2);
    assert!(h.o(door).is_open(), "TryUseItem ran on arrival");
}

/// `Player.HandleActionUseWithTarget`: a missing source or target answers UseDone; a target of
/// the wrong ItemType is refused with ACE's transient (Player_Use.cs).
#[test]
fn use_with_target_refusals() {
    let mut h = H::new();
    let kit = h.give(KIT);
    let torch = h.give(TORCH);
    start_capture();
    player_use::handle_action_use_with_target(&mut h.w, P, 0x8000_7777, P.full());
    player_use::handle_action_use_with_target(&mut h.w, P, kit.full(), 0x8000_7777);
    assert_eq!(use_dones(&sent_to_s()), [0, 0]);

    start_capture();
    player_use::handle_action_use_with_target(&mut h.w, P, kit.full(), torch.full());
    let m = sent_to_s();
    assert_eq!(
        transients(&m),
        ["Cannot use the Healing Kit with the Torch"]
    );
    assert_eq!(use_dones(&m), [0]);
}

/// `Player.HandleActionNoLongerViewingContents`: the container the player views closes; one
/// someone else views stays open (Player_Use.cs).
#[test]
fn no_longer_viewing_contents_closes_the_viewed_container() {
    let mut h = H::new();
    let b = h.place(BOX, 20.5, 20.0);
    dispatch::act_on_use::act_on_use(&mut h.w, b, P);
    assert!(h.o(b).is_open());
    assert_eq!(player_use::fields(&h.w, P).last_opened_container_id, b);
    h.o_mut(b).set_viewer(0x5000_0099);
    player_use::handle_action_no_longer_viewing_contents(&mut h.w, P, b.full());
    assert!(h.o(b).is_open(), "another viewer");
    h.o_mut(b).set_viewer(P.full());
    player_use::handle_action_no_longer_viewing_contents(&mut h.w, P, b.full());
    assert!(!h.o(b).is_open());
    assert_eq!(
        player_use::fields(&h.w, P).last_opened_container_id,
        ObjectGuid::INVALID
    );
}

// ------------------------------------------------------------------------------------ Door

/// `Door.ActOnUse`: opens (motion On, ethereal, busy for the animation), closes on a second use,
/// and the auto-close timer resets it `ResetInterval` (30 s) after the last use; a locked door
/// refuses from the front with ACE's transient and the locked sound, and relocks on reset
/// (Door.cs).
#[test]
fn door_opens_closes_and_resets() {
    let mut h = H::new();
    let d = h.place(DOOR, 21.0, 20.0);
    assert!(!h.o(d).is_open());
    assert_eq!(h.o(d).reset_interval(), Some(30.0));

    start_capture();
    dispatch::act_on_use::act_on_use(&mut h.w, d, P);
    let m = sent_to_s();
    assert!(h.o(d).is_open() && h.o(d).ethereal() == Some(true));
    assert!(h.o(d).wo.world_object.is_busy, "busy for the animation");
    assert_eq!(h.o(d).use_timestamp(), Some(h.w.now.unix_time));
    assert!(
        ops(&m).contains(&GameMessageOpcode::Motion.0),
        "the open motion reaches the player"
    );
    let cm = h
        .o(d)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
        .unwrap();
    assert_eq!(cm.motion_state.forward_command, MotionCommand::On);

    h.run(0.1);
    assert!(!h.o(d).wo.world_object.is_busy);

    // the auto-close: 30 s after the use it closes
    h.run(29.5);
    assert!(h.o(d).is_open());
    h.run(0.6);
    assert!(!h.o(d).is_open(), "Reset closed it");
    assert_eq!(h.o(d).reset_timestamp(), Some(h.w.now.unix_time));

    // locked, from the front
    h.run(1.0);
    h.o_mut(d).set_is_locked(true);
    h.o_mut(d).set_default_locked(true);
    start_capture();
    dispatch::act_on_use::act_on_use(&mut h.w, d, P);
    let m = sent_to_s();
    assert_eq!(transients(&m), ["The door is locked!"]);
    assert_eq!(sounds(&m), [(d.full(), Sound::OpenFailDueToLock.0)]);
    assert!(!h.o(d).is_open());

    // a reset after an unlock relocks it (PublicUpdatePropertyBool Locked)
    h.o_mut(d).set_is_locked(false);
    dispatch::act_on_use::act_on_use(&mut h.w, d, P);
    assert!(h.o(d).is_open());
    start_capture();
    h.run(30.5);
    let m = sent_to_s();
    assert!(!h.o(d).is_open() && h.o(d).is_locked());
    assert!(
        ops(&m).contains(&GameMessageOpcode::PublicUpdatePropertyBool.0),
        "{:04X?}",
        ops(&m)
    );
}

/// A door used twice within its interval resets only on the second use's timer (the first
/// timer's `useTimestamp` no longer matches), Door.Reset.
#[test]
fn door_reset_ignores_a_stale_timer() {
    let mut h = H::new();
    let d = h.place(DOOR, 21.0, 20.0);
    dispatch::act_on_use::act_on_use(&mut h.w, d, P); // open at t0
    h.run(10.0);
    dispatch::act_on_use::act_on_use(&mut h.w, d, P); // close at t0+10
    h.run(1.0);
    dispatch::act_on_use::act_on_use(&mut h.w, d, P); // open at t0+11
    h.run(19.5); // t0+30.5: the first timer fires with a stale timestamp
    assert!(h.o(d).is_open(), "the stale timer does nothing");
    h.run(21.0); // t0+41.5: the third use's timer
    assert!(!h.o(d).is_open());
}

// ------------------------------------------------------------------------------------ Lock

/// `LockHelper.Unlock(target, key, keyCode)`: the lock code ignores case, the Sonic Screwdriver's
/// code and OpensAnyLock open anything; an unlocked lock, an open lock and a wrong key are
/// refused (Lock.cs).
#[test]
fn keys_unlock_by_code() {
    let mut h = H::new();
    let c = h.place(CHEST, 21.0, 20.0);
    h.o_mut(c).set_lock_code(Some("oak".to_owned()));
    h.o_mut(c).set_is_locked(true);
    let key = h.give(KEY);

    assert_eq!(
        lock::unlock_key(&mut h.w, c, Some(key), Some("birch")),
        UnlockResults::IncorrectKey
    );
    start_capture();
    assert_eq!(
        lock::unlock_key(&mut h.w, c, Some(key), None),
        UnlockResults::UnlockSuccess,
        "OAK fits oak"
    );
    let m = sent_to_s();
    assert!(!h.o(c).is_locked());
    assert!(ops(&m).contains(&GameMessageOpcode::PublicUpdatePropertyBool.0));
    assert_eq!(sounds(&m), [(c.full(), Sound::LockSuccess.0)]);
    assert_eq!(
        lock::unlock_key(&mut h.w, c, Some(key), None),
        UnlockResults::AlreadyUnlocked
    );

    h.o_mut(c).set_is_locked(true);
    assert_eq!(
        lock::unlock_key(&mut h.w, c, None, Some("_bohemund's_magic_key_")),
        UnlockResults::UnlockSuccess
    );
    h.o_mut(c).set_is_locked(true);
    h.o_mut(key)
        .set_property(PropertyString::KeyCode, "nope".to_owned());
    h.o_mut(key).set_property(PropertyBool::OpensAnyLock, true);
    assert_eq!(
        lock::unlock_key(&mut h.w, c, Some(key), None),
        UnlockResults::UnlockSuccess
    );
    h.o_mut(c).set_is_open(true);
    assert_eq!(
        lock::unlock_key(&mut h.w, c, Some(key), None),
        UnlockResults::Open
    );

    // not a lock: no lock code
    let t = h.place(TORCH, 22.0, 20.0);
    assert_eq!(
        lock::unlock_key(&mut h.w, t, Some(key), None),
        UnlockResults::IncorrectKey
    );
}

/// `LockHelper.IsPickable`/`Unlock(target, skill, ref difficulty)`: no ResistLockpick or 9999 is
/// unpickable; the difficulty is returned; one `Next(0, 1)` draw against `GetSkillChance` (Lock.cs).
#[test]
fn lockpicking_rolls_against_the_skill_chance() {
    let mut h = H::new();
    let c = h.place(CHEST, 21.0, 20.0);
    h.o_mut(c).set_is_locked(true);
    let mut difficulty = -1;
    assert_eq!(
        lock::unlock_lockpick(&mut h.w, c, 100, &mut difficulty),
        UnlockResults::CannotBePicked
    );
    assert_eq!(difficulty, -1, "not set");
    h.o_mut(c).set_property(PropertyInt::ResistLockpick, 9999);
    assert!(!lock::is_pickable(h.o(c)));
    h.o_mut(c).set_property(PropertyInt::ResistLockpick, 150);
    assert!(lock::is_pickable(h.o(c)));
    assert_eq!(lock::get_resist_lockpick(&mut h.w, c), Some(150));
    assert_eq!(lock::get_resist_lockpick_view(&h.w, c), Some(150));

    // the roll, against a reference System.Random with the same seed
    let mut ok = 0;
    for seed in 0..40u64 {
        h.o_mut(c).set_is_locked(true);
        ThreadSafeRandom::seed(seed);
        let mut reference = DotNetRandom::new(i32::try_from(seed).unwrap());
        let chance = skill_check::get_skill_chance(140, 150, skill_check::DEFAULT_FACTOR);
        let want = if reference.next_double() >= chance {
            UnlockResults::PickLockFailed
        } else {
            UnlockResults::UnlockSuccess
        };
        let mut difficulty = 0;
        let got = lock::unlock_lockpick(&mut h.w, c, 140, &mut difficulty);
        assert_eq!((got, difficulty), (want, 150), "seed {seed}");
        assert_eq!(h.o(c).is_locked(), got != UnlockResults::UnlockSuccess);
        ok += usize::from(got == UnlockResults::UnlockSuccess);
    }
    assert!(ok > 5 && ok < 35, "both outcomes occur ({ok}/40)");

    h.o_mut(c).set_is_locked(false);
    assert_eq!(
        lock::unlock_lockpick(&mut h.w, c, 140, &mut difficulty),
        UnlockResults::AlreadyUnlocked
    );
    h.o_mut(c).set_is_open(true);
    assert_eq!(
        lock::unlock_lockpick(&mut h.w, c, 140, &mut difficulty),
        UnlockResults::Open
    );
}

/// `Key.HandleActionUseOnTarget` -> `UnlockerHelper.UseUnlocker` (on the next action pass) ->
/// `Chest.Unlock` -> `ConsumeUnlocker`: the message with the uses left, the Structure update,
/// UseDone; the unlocker's window; a wrong key; the last use consumes the key (Key.cs, Lock.cs,
/// Chest.cs).
#[test]
fn a_key_unlocks_a_chest_and_wears_out() {
    let mut h = H::new();
    let c = h.place(CHEST, 21.0, 20.0);
    h.o_mut(c).set_lock_code(Some("oak".to_owned()));
    h.o_mut(c).set_is_locked(true);
    let key = h.give(KEY);

    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, key, P, c);
    h.run(0.0);
    let m = sent_to_s();
    assert_eq!(
        chats(&m),
        ["The Chest has been unlocked.\nYour key has 2 uses left."]
    );
    assert_eq!(use_dones(&m), [0]);
    assert!(ops(&m).contains(&GameMessageOpcode::PublicUpdatePropertyInt.0));
    assert_eq!(h.o(key).structure(), Some(2));
    assert_eq!(
        h.o(key).value(),
        Some(20),
        "Value -= StructureUnitValue (30 / 3)"
    );
    assert_eq!(
        (h.o(c).last_unlocker(), h.o(c).use_lock_timestamp()),
        (Some(P.full()), Some(h.w.now.unix_time))
    );

    // the lock is open: a second use is AlreadyUnlocked
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, key, P, c);
    h.run(0.0);
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::LockAlreadyUnlocked)]
    );

    // a wrong key
    h.o_mut(c).set_is_locked(true);
    h.o_mut(c).set_lock_code(Some("elm".to_owned()));
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, key, P, c);
    h.run(0.0);
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::KeyDoesntFitThisLock)]
    );

    // one use left: "use", not "uses"
    h.o_mut(c).set_lock_code(Some("oak".to_owned()));
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, key, P, c);
    h.run(0.0);
    assert_eq!(
        chats(&sent_to_s()),
        ["The Chest has been unlocked.
Your key has 1 use left."]
    );
    h.o_mut(c).set_is_locked(true);

    // the last use: "is used up." and the key leaves the pack
    h.o_mut(key).set_structure(Some(1));
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, key, P, c);
    h.run(0.0);
    let m = sent_to_s();
    assert_eq!(
        chats(&m),
        ["The Chest has been unlocked.\nYour key is used up."]
    );
    assert!(ops(&m).contains(&GameMessageOpcode::InventoryRemoveObject.0));
    assert!(!h.w.objects.contains(key), "consumed");
}

/// A lockpick: an untrained picker is refused; a failed pick sounds from the lock and uses the
/// pick up ("lockpicks are used up"); a door whose LockCode is "" takes no key (Lock.cs).
#[test]
fn lockpicks_and_keyless_doors() {
    let mut h = H::new();
    let c = h.place(CHEST, 21.0, 20.0);
    h.o_mut(c).set_is_locked(true);
    h.o_mut(c)
        .set_property(PropertyInt::ResistLockpick, 10_000 - 1);
    let pick = h.give(LOCKPICK);

    // untrained
    let s = h
        .o_mut(P)
        .get_creature_skill(Skill::Lockpick, true)
        .unwrap();
    let o = h.o_mut(P);
    s.set_advancement_class(o, SkillAdvancementClass::Untrained);
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, pick, P, c);
    h.run(0.0);
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::YouArentTrainedInLockpicking)]
    );
    let o = h.o_mut(P);
    s.set_advancement_class(o, SkillAdvancementClass::Trained);

    // 9999 cannot be picked
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, pick, P, c);
    h.run(0.0);
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::YouCannotLockOrUnlockThat)]
    );

    // an impossible pick fails: the fail sound from the chest, the message, the pick used up
    h.o_mut(c).set_property(PropertyInt::ResistLockpick, 2000);
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, pick, P, c);
    h.run(0.0);
    let m = sent_to_s();
    assert_eq!(sounds(&m), [(c.full(), Sound::PicklockFail.0)]);
    assert_eq!(
        chats(&m),
        ["You have failed to pick the lock.  It is still locked.  Your lockpicks are used up."]
    );
    assert!(!h.w.objects.contains(pick));
    assert!(h.o(c).is_locked());

    // a keyless door
    let d = h.place(DOOR, 22.0, 20.0);
    h.o_mut(d).set_is_locked(true);
    let key = h.give(KEY);
    start_capture();
    dispatch::handle_action_use_on_target::handle_action_use_on_target(&mut h.w, key, P, d);
    h.run(0.0);
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::YouCannotLockOrUnlockThat)]
    );
    assert_eq!(
        h.o(d).lock_code().as_deref(),
        Some(""),
        "Door.SetEphemeralValues: LockCode ?? \"\""
    );
}

// ------------------------------------------------------------------------------------ Chest

/// `Chest.CheckUseRequirements`: locked (the locked sound, no message by default); the unlocker's
/// 10 s window refuses anyone else; `Chest.Open` starts the reset timer, which closes the chest,
/// relocks it and clears what players left in it (Chest.cs).
#[test]
fn chest_requirements_open_and_reset() {
    let mut h = H::new();
    let c = h.place(CHEST, 21.0, 20.0);
    h.o_mut(c).set_is_locked(true);
    h.o_mut(c).set_default_locked(true);
    start_capture();
    let r = dispatch::check_use_requirements::check_use_requirements(&mut h.w, c, P);
    let m = sent_to_s();
    assert!(!r.success);
    assert_eq!(sounds(&m), [(c.full(), Sound::OpenFailDueToLock.0)]);
    assert!(
        transients(&m).is_empty(),
        "fix_chest_missing_inventory_window is off"
    );

    // unlocked by someone else a moment ago
    h.o_mut(c).set_is_locked(false);
    let now = h.w.now.unix_time;
    h.o_mut(c).set_use_lock_timestamp(Some(now));
    h.o_mut(c).set_last_unlocker(Some(0x5000_0099));
    start_capture();
    assert!(!dispatch::check_use_requirements::check_use_requirements(&mut h.w, c, P).success);
    assert_eq!(
        transients(&sent_to_s()),
        ["The Chest is already in use by someone else!"]
    );
    h.run(10.5);
    assert!(
        dispatch::check_use_requirements::check_use_requirements(&mut h.w, c, P).success,
        "the window has passed"
    );

    // open, then a player item left inside, then the reset (ResetInterval 60)
    dispatch::act_on_use::act_on_use(&mut h.w, c, P);
    assert!(h.o(c).is_open() && h.o(c).reset_message_pending());
    assert_eq!(h.o(c).use_lock_timestamp(), None);
    let left = h.spawn(TORCH);
    assert!(container::try_add_to_inventory(
        &mut h.w, c, left, 0, false, true
    ));
    h.run(59.0);
    assert!(h.o(c).is_open());
    start_capture();
    h.run(1.5);
    let m = sent_to_s();
    assert!(!h.o(c).is_open(), "Reset closed it");
    assert!(h.o(c).is_locked(), "and relocked it (DefaultLocked)");
    assert!(!h.o(c).reset_message_pending());
    assert!(
        !container::inventory_values(&h.w, c).contains(&left),
        "ClearUnmanagedInventory"
    );
    assert!(
        ops(&m).contains(&GameEventType::CloseGroundContainer.0),
        "{:04X?}",
        ops(&m)
    );
}

// ------------------------------------------------------------------------------------ Food

/// `Food.ActOnUse` -> `Player.ApplyConsumable` -> `Food.ApplyConsumable`: busy while eating, the
/// health boost and its message, the eat sound, one of the stack consumed; a busy player is
/// refused (Food.cs, Player_Use.cs).
#[test]
fn eating_restores_health() {
    let mut h = H::new();
    let apple = h.give(APPLE);
    let o = h.o_mut(P);
    let health = o.health();
    health.set_current(o, 5);

    h.o_mut(P).wo.world_object.is_busy = true;
    start_capture();
    dispatch::act_on_use::act_on_use(&mut h.w, apple, P);
    assert!(sent_to_s()
        .iter()
        .any(|m| m.is_event(GameEventType::WeenieError)
            && m.u32_at(16) == we(WeenieError::YoureTooBusy)));
    h.o_mut(P).wo.world_object.is_busy = false;

    start_capture();
    dispatch::act_on_use::act_on_use(&mut h.w, apple, P);
    assert!(h.o(P).wo.world_object.is_busy, "ApplyConsumable: busy");
    for _ in 0..5 {
        h.run(0.1); // the motion chain's actions, one pass each
    }
    let m = sent_to_s();
    assert_eq!(h.health().0, 15, "5 + BoostValue 10");
    assert_eq!(chats(&m), ["The Apple restores 10 points of your Health."]);
    assert_eq!(sounds(&m), [(P.full(), Sound::Eat1.0)]);
    assert!(!h.w.objects.contains(apple), "the one apple is eaten");
    assert!(!h.o(P).wo.world_object.is_busy);
    assert!(
        ops(&m).contains(&GameMessageOpcode::Motion.0),
        "the eat motion"
    );

    // a harmful food "takes" (a negative boost) and is recorded as damage
    let bad = h.give(APPLE);
    h.o_mut(bad).set_property(PropertyInt::BoostValue, -4);
    start_capture();
    food::apply_consumable(&mut h.w, bad, P);
    assert_eq!(
        chats(&sent_to_s()),
        ["The Apple takes 4 points of your Health."]
    );
    assert_eq!(h.health().0, 11);
}

// ------------------------------------------------------------------------------------ Healer

/// `Healer.HandleActionUseOnTarget` refusals: untrained, a non-player target, full health
/// (Healer.cs).
#[test]
fn healing_kit_refusals() {
    let mut h = H::new();
    let kit = h.give(KIT);

    let s = h.o_mut(P).get_creature_skill(Skill::Healing, true).unwrap();
    let o = h.o_mut(P);
    s.set_advancement_class(o, SkillAdvancementClass::Untrained);
    start_capture();
    healer::healer_handle_action_use_on_target(&mut h.w, kit, P, P);
    assert_eq!(
        use_dones(&sent_to_s()),
        [we(WeenieError::YouArentTrainedInHealing)]
    );
    let o = h.o_mut(P);
    s.set_advancement_class(o, SkillAdvancementClass::Trained);

    let torch = h.place(TORCH, 21.0, 20.0);
    start_capture();
    healer::healer_handle_action_use_on_target(&mut h.w, kit, P, torch);
    assert_eq!(use_dones(&sent_to_s()), [we(WeenieError::YouCantHealThat)]);

    // full health: WeenieErrorWithString _IsAtFullHealth, then UseDone
    let max = {
        let o = h.o(P);
        o.health()
    };
    let max_value = max.max_value(
        &mut empyrean_world::world_objects::entity::creature_attribute::StatCtx::in_world(
            &mut h.w, P,
        ),
    );
    let o = h.o_mut(P);
    max.set_current(o, max_value);
    start_capture();
    healer::healer_handle_action_use_on_target(&mut h.w, kit, P, P);
    let m = sent_to_s();
    assert!(m
        .iter()
        .any(|m| m.is_event(GameEventType::WeenieErrorWithString)));
    assert_eq!(use_dones(&m), [0]);
}

/// `Healer.DoHealing` on the healer: one use spent (the uses message), the skill check and heal
/// amount draws in ACE's order, the stamina cost, the message and the Structure update; the last
/// use consumes the kit (Healer.cs).
#[test]
fn healing_yourself() {
    let mut h = H::new();
    let kit = h.give(KIT);
    let o = h.o_mut(P);
    let health = o.health();
    health.set_current(o, 1);
    let stamina_before = {
        let o = h.o(P);
        o.stamina().current(o)
    };

    // the reference: the same seed, the skill check's draw then the heal's two; the first seed
    // whose check passes (the failing branch is checked below)
    let missing = {
        let o = h.o(P);
        let hv = o.health();
        hv.max_value(
            &mut empyrean_world::world_objects::entity::creature_attribute::StatCtx::in_world(
                &mut h.w, P,
            ),
        ) - 1
    };
    let current = {
        let s = h.o_mut(P).get_creature_skill(Skill::Healing, true).unwrap();
        s.current(&mut h.w, P)
    };
    let roll = |seed: u64| {
        ThreadSafeRandom::seed(seed);
        let mut difficulty = 0;
        let passes = healer::skill_check_roll(
            current,
            SkillAdvancementClass::Trained,
            50,
            true,
            missing,
            &mut difficulty,
        );
        (
            passes,
            healer::heal_amount(current, 1.0, missing, stamina_before, || 1.0),
        )
    };
    let pass_seed = (0..200u64).find(|&s| roll(s).0).expect("a passing seed");
    let fail_seed = (0..200u64).find(|&s| !roll(s).0).expect("a failing seed");
    let (_, (amount, critical, cost)) = roll(pass_seed);
    assert!(amount > 0);

    ThreadSafeRandom::seed(pass_seed);
    start_capture();
    healer::do_healing(&mut h.w, kit, P, P, missing);
    let m = sent_to_s();
    assert_eq!(h.o(kit).structure(), Some(1));
    let crit = if critical { "expertly " } else { "" };
    assert_eq!(
        chats(&m),
        [format!(
            "You {crit}heal yourself for {amount} Health points. Your Healing Kit has 1 use left."
        )]
    );
    assert_eq!(h.health().0, 1 + amount);
    let o = h.o(P);
    assert_eq!(o.stamina().current(o), stamina_before - cost);
    assert!(
        ops(&m).contains(&GameMessageOpcode::PublicUpdatePropertyInt.0),
        "the Structure update"
    );
    assert_eq!(
        h.o(kit).value(),
        Some(90),
        "Value -= StructureUnitValue (100 / 10)"
    );

    // the last use, failing: the kit is used up and consumed all the same
    ThreadSafeRandom::seed(fail_seed);
    start_capture();
    healer::do_healing(&mut h.w, kit, P, P, missing);
    let m = sent_to_s();
    assert_eq!(
        chats(&m),
        ["You fail to heal yourself. Your Healing Kit is used up."]
    );
    assert!(ops(&m).contains(&GameMessageOpcode::InventoryRemoveObject.0));
    assert!(!h.w.objects.contains(kit));
}

// ------------------------------------------------------------------------------------ Gem

/// `Gem.ActOnUse`: a timed rare within 180 s of the last one is refused with the seconds left
/// (`Math.Ceiling`); a busy player is refused (Gem.cs).
#[test]
fn a_timed_rare_waits_for_the_timer() {
    let mut h = H::new();
    let rare = h.give(RARE);
    let now = h.w.now.unix_time;
    h.o_mut(P).set_last_rare_used_timestamp(now - 10.5);
    start_capture();
    gem::gem_act_on_use(&mut h.w, rare, P);
    assert_eq!(
        chats(&sent_to_s()),
        ["You may use another timed rare in 170s"]
    );
    assert!(h.w.objects.contains(rare));

    h.o_mut(P).set_last_rare_used_timestamp(now - 200.0);
    start_capture();
    gem::gem_act_on_use(&mut h.w, rare, P);
    let m = sent_to_s();
    assert_eq!(h.o(P).last_rare_used_timestamp(), now);
    assert!(!h.w.objects.contains(rare), "used and consumed");
    assert!(ops(&m).contains(&GameMessageOpcode::InventoryRemoveObject.0));

    let other = h.give(RARE);
    h.o_mut(P).wo.world_object.is_busy = true;
    start_capture();
    gem::gem_act_on_use(&mut h.w, other, P);
    assert!(sent_to_s()
        .iter()
        .any(|m| m.is_event(GameEventType::WeenieError)
            && m.u32_at(16) == we(WeenieError::YoureTooBusy)));

    // a Contained gem outside the pack does nothing on activation
    h.o_mut(P).wo.world_object.is_busy = false;
    let ground = h.place(RARE, 21.0, 20.0);
    h.o_mut(ground).set_property(
        PropertyInt::ItemUseable,
        i32::try_from(Usable::Contained.0).unwrap(),
    );
    start_capture();
    gem::gem_on_activate(&mut h.w, ground, P);
    assert!(sent_to_s().is_empty());
}

/// A door that is a player's `PlayerKillerStatus` NPK is irrelevant; the healer's PK check refuses
/// a target of another PK type with WeenieErrorWithString and UseDone (Healer.cs).
#[test]
fn healing_another_pk_type_is_refused() {
    let mut h = H::new();
    let kit = h.give(KIT);
    let other = ObjectGuid::new(0x5000_0002);
    let weenie = h.w.content.get_cached_weenie(PLAYER_WCID).unwrap();
    let mut o = CtorEnv::with_world(&h.w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            Class::Player,
            weenie,
            other,
            1,
        )
    });
    o.set_location(Some(at(21.0, 20.0)));
    o.set_player_killer_status_prop(PlayerKillerStatus::PK);
    h.w.objects.insert(o).unwrap();
    start_capture();
    healer::healer_handle_action_use_on_target(&mut h.w, kit, P, other);
    let m = sent_to_s();
    assert!(m
        .iter()
        .any(|m| m.is_event(GameEventType::WeenieErrorWithString)));
    assert_eq!(use_dones(&m), [0]);
    let _ = door::DoorMotion::Open;
}

mod hotspot_cycle {
    use crate::support::content_interactions::*;

    /// A creature touching a hotspot starts one loop, which draws its cycle time once. ACE built a
    /// loop, kept it, and enqueued a second one: two draws, the first discarded.
    #[test]
    fn a_hotspot_draws_its_cycle_time_once_when_touched() {
        let mut h = H::new();
        let spot = object(&mut h.w, Class::Hotspot, 0x8000_0400);
        o(&mut h.w, spot).set_property(PropertyFloat::HotspotCycleTime, 2.0);
        o(&mut h.w, spot).set_property(PropertyFloat::HotspotCycleTimeVariance, 0.5);

        ThreadSafeRandom::seed(99);
        let _first = ThreadSafeRandom::next_float(0.0, 1.0);
        let second = ThreadSafeRandom::next_float(0.0, 1.0);

        ThreadSafeRandom::seed(99);
        hotspot::hotspot_on_collide_object(&mut h.w, spot, P1);
        assert_eq!(
            ThreadSafeRandom::next_float(0.0, 1.0),
            second,
            "one draw was taken"
        );
    }
}

mod requirements {
    use crate::support::navigation_world::*;

    /// `WorldObject.CheckUseRequirements` is typed now: the Use flow gets the portal's
    /// `ActivationResult` through the dispatch (a success, then a refusal while teleporting).
    #[test]
    fn the_use_requirement_result_comes_back_through_the_dispatch() {
        let mut w = world();
        let p = spawn_portal(&mut w, None);
        let r = dispatch::check_use_requirements::check_use_requirements(&mut w, p, g());
        assert!(r.success && r.message.is_none());
        w.objects.get_mut(g()).unwrap().wo.world_object.teleporting = true;
        let r = dispatch::check_use_requirements::check_use_requirements(&mut w, p, g());
        assert!(!r.success, "teleporting: refused");
    }
}
