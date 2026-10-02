//! Vectors: fixtures/vectors/death/
//! Divergence: V390, V398, V400, V405, V406
//! XP/levelling, death, corpses, damage history, death string tables and DeathItems follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::DataId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, Case};
use empyrean_content::models::world::Weenie as WeenieRow;
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, XpTable};
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    DamageType, EquipMask, ItemType, PKLevel, PlayerKillerStatus, PropertyAttribute2nd,
    PropertyDataId, PropertyInt, PropertyInt64, PropertyString, ShareType, WeenieType, XpType,
};
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::models::shard::Character;
use empyrean_testkit::land;
use empyrean_world::dispatch::{self, Class};
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::damage_history::{self, DamageHistory};
use empyrean_world::entity::damage_history_info::DamageHistoryInfo;
use empyrean_world::entity::death_item::{DeathItem, DeathItemCategory, DeathItems};
use empyrean_world::entity::death_message::string_format;
use empyrean_world::entity::strings;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::managers::enchantment_manager_with_caching as emc;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    corpse, creature_death, player_death, player_skills, player_xp,
};
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

const S1: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const S2: SessionId = SessionId {
    client_id: 2,
    generation: 1,
};
const P1: ObjectGuid = ObjectGuid::new(0x5000_0001);
const P2: ObjectGuid = ObjectGuid::new(0x5000_0002);
const MONSTER: ObjectGuid = ObjectGuid::new(0x7A9B_4100);
const LB: u32 = 0xA9B4_0000;
const CORPSE_WCID: u32 = 21;
const SEED: i32 = 49;

/// The level curve of these tests: level 2 at 1,000, 3 at 2,500 (no credit), 4 (max) at 5,000.
fn xp_table() -> XpTable {
    XpTable {
        id: DataId(file_id::XP_TABLE),
        level_xp: vec![0, 0, 1_000, 2_500, 5_000],
        level_credits: vec![0, 0, 1, 0, 1],
        ..empyrean_dat::fake::sample::xp_table()
    }
}

/// No attribute contribution to the vitals: a max vital is its starting value plus ranks.
fn secondary() -> SecondaryAttributeTable {
    let zero = SkillFormula {
        w: 0,
        x: 0,
        y: 0,
        z: 0,
        attr1: 0,
        attr2: 0,
    };
    SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: zero,
        stamina: zero,
        mana: zero,
    }
}

/// The Vitae spell (666), as the dat and the world database describe it.
fn spell_table() -> dereth_assets::SpellTable {
    let vitae = dereth_assets::tables::SpellBase {
        name: "Vitae".to_owned(),
        description: String::new(),
        school: 4,
        icon: 0,
        category: 204,
        bitfield: 0,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 1.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 1,
        meta_spell_id: 666,
        duration: Some((-1.0, 0.0, 0.0)), // vitae never expires (the retail dat: -1)
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    };
    dereth_assets::SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells: [(666, vitae)].into_iter().collect(),
        spellset_bucket_index: 1,
        spellsets: std::collections::BTreeMap::new(),
    }
}

fn content() -> MemContent {
    use empyrean_entity::enums::EnchantmentTypeFlags as F;
    MemContent::new()
        .weenie(
            WeenieRow::new(CORPSE_WCID, "corpse", WeenieType::Corpse)
                .with_string(PropertyString::Name, "Corpse")
                .with_int(PropertyInt::ItemsCapacity, 120)
                .with_did(PropertyDataId::Setup, land::TEST_SETUP),
        )
        .spell(empyrean_content::models::world::Spell {
            id: 666,
            name: "Vitae".to_owned(),
            stat_mod_type: Some(
                (F::MultipleStat.0 | F::Multiplicative.0 | F::Vitae.0).cast_unsigned(),
            ),
            stat_mod_key: Some(0),
            stat_mod_val: Some(1.0),
            ..empyrean_content::models::world::Spell::default()
        })
}

struct H {
    w: World,
    clock: VirtualClock,
}

impl H {
    fn new() -> Self {
        Self::with_xp_table(xp_table())
    }

    fn with_xp_table(xp_table: XpTable) -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let dats = FakeDats::new()
            .with_xp_table(xp_table)
            .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, secondary())
            .with_spell_table(spell_table())
            // the death animation (`ExecuteMotion`) reads the run rate, so the Run skill's table
            .with_skill_table(empyrean_testkit::dats::empty_skill_table())
            .build()
            .expect("fake dats");
        let mut w = World::new(now, dats);
        w.timers = timers;
        w.content = Arc::new(content());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(u64::from(SEED.unsigned_abs()));
        H { w, clock }
    }

    /// One pass: the delay manager, then `LandblockManager.Tick`.
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

    fn o(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("object in store")
    }

    fn o_mut(&mut self, g: ObjectGuid) -> &mut WorldObject {
        self.w.objects.get_mut(g).expect("object in store")
    }

    /// A creature (or player) with `health` max and current health, built as the constructor
    /// would (the Creature ctor's DamageHistory included), not yet placed.
    fn creature(&mut self, class: Class, guid: ObjectGuid, name: &str, health: u32) {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let mut vitals = empyrean_common::dotnet::DotNetDict::new();
        for (v, level) in [
            (PropertyAttribute2nd::MaxHealth, health),
            (PropertyAttribute2nd::MaxStamina, 100),
            (PropertyAttribute2nd::MaxMana, 100),
        ] {
            vitals.insert(
                v,
                PropertiesAttribute2nd {
                    init_level: level,
                    level_from_cp: 0,
                    cp_spent: 0,
                    current_level: level,
                },
            );
        }
        o.biota.properties_attribute_2nd = Some(vitals);
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            let cv = CreatureVital::new(&mut o, v);
            o.vitals_mut().insert(v, cv);
        }
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
        o.creature.as_mut().unwrap().creature_death.damage_history =
            DamageHistory::new(guid, self.w.now.utc);
        o.set_location(Some(Position::from_components(
            LB | 0x0001,
            20.0,
            20.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        // no heartbeats (an object never initialised would heartbeat on every pass)
        o.set_heartbeat_interval(Some(0.0));
        empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
            &mut o,
            self.w.now.unix_time,
        );
        self.w.objects.insert(o).expect("fresh guid");
    }

    /// A level-`level` player with a session, `total` total and available XP.
    fn player(&mut self, guid: ObjectGuid, session: SessionId, name: &str, level: i32, total: i64) {
        self.creature(Class::Player, guid, name, 100);
        let o = self.o_mut(guid);
        o.set_level(Some(level));
        o.set_property(PropertyInt64::TotalExperience, total);
        o.set_property(PropertyInt64::AvailableExperience, total);
        o.set_property(PropertyInt::AvailableSkillCredits, 0);
        o.set_property(PropertyInt::TotalSkillCredits, 0);
        // every ACE player has a Character (Player.CalculateObjDesc reads its hair texture)
        o.player.as_mut().expect("a player").player.character = Some(Character::default());
        // a Strength record: `HasEnoughBurdenToAddToInventory` reads the burden limit from it
        o.biota
            .properties_attribute
            .get_or_insert_with(Default::default)
            .get_or_insert_with(
                empyrean_entity::enums::PropertyAttribute::Strength,
                Default::default,
            )
            .init_level = 100;
        // a player's pack sizes (the player weenie's): `TryAddToInventory` checks them
        o.set_property(PropertyInt::ItemsCapacity, 102);
        o.set_property(PropertyInt::ContainersCapacity, 7);
        // (a null EncumbranceVal fails ACE's burden check; a player's is set at creation)
        o.set_property(PropertyInt::EncumbranceVal, 0);
        self.w.sessions.insert(
            session,
            SessionData {
                player: Some(guid),
                ..SessionData::default()
            },
        );
    }

    /// Places an object on the landblock (its body enters the physics world).
    fn place(&mut self, g: ObjectGuid) {
        assert!(
            lm::add_object(&mut self.w, g, false),
            "placed on the landblock"
        );
    }

    fn item(&mut self, guid: u32, name: &str, item_type: ItemType, value: i32) -> ObjectGuid {
        let g = ObjectGuid::new(guid);
        let mut o = WorldObject::allocate(Class::GenericObject);
        o.guid = g;
        o.biota.id = guid;
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyInt::ItemType, i32::try_from(item_type.0).unwrap());
        o.set_property(PropertyInt::Value, value);
        self.w.objects.insert(o).expect("fresh guid");
        g
    }

    fn health(&self, g: ObjectGuid) -> u32 {
        let o = self.o(g);
        o.health().current(o)
    }

    fn set_health(&mut self, g: ObjectGuid, v: u32) {
        let o = self.o_mut(g);
        let h = o.health();
        h.set_current(o, v);
    }

    /// A bare corpse object.
    fn corpse(&mut self, guid: ObjectGuid) {
        let mut c = WorldObject::allocate(Class::Corpse);
        c.guid = guid;
        c.biota.id = guid.full();
        self.w.objects.insert(c).expect("fresh guid");
    }

    fn info(&self, g: ObjectGuid) -> DamageHistoryInfo {
        DamageHistoryInfo::new(&self.w, g, 0.0)
    }

    /// The first dynamic object that is a corpse (corpses take dynamic guids).
    fn find_corpse(&self) -> Option<ObjectGuid> {
        (0x8000_0000..0x8000_0010u32)
            .map(ObjectGuid::new)
            .find(|g| self.w.objects.get(*g).is_some_and(WorldObject::is_corpse))
    }
}

fn lb_id() -> LandblockId {
    LandblockId::new(LB | 0xFFFF)
}

/// `(session, opcode, game event type or 0, bytes)` of each captured send.
fn sent() -> Vec<(SessionId, u32, u32, Vec<u8>)> {
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

/// A `String16L` at `at`.
fn string16l(b: &[u8], at: usize) -> String {
    let len = usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    String::from_utf8(b[at + 2..at + 2 + len].to_vec()).unwrap()
}

/// The texts of the system chats in `msgs`.
fn chats(msgs: &[(SessionId, u32, u32, Vec<u8>)]) -> Vec<String> {
    msgs.iter()
        .filter(|m| m.1 == GameMessageOpcode::ServerMessage.0)
        .map(|m| string16l(&m.3, 4))
        .collect()
}

/// A game event's text payload (a `String16L` after the 16-byte event header).
fn event_text(
    msgs: &[(SessionId, u32, u32, Vec<u8>)],
    event: GameEventType,
) -> Vec<(SessionId, String)> {
    msgs.iter()
        .filter(|m| m.2 == event.0)
        .map(|m| (m.0, string16l(&m.3, 16)))
        .collect()
}

// ------------------------------------------------------------------------------------ DamageHistory

/// `Add` logs negative entries and sums per attacker (first-seen order); `TopDamager` is the first
/// highest; `LastDamager` the last damage entry's attacker; `OnHeal` scales every total by
/// `1 - heal / previous missing health` in float; `Prune` drops entries older than 3 minutes and
/// rebuilds the totals from what is left (heals included).
#[test]
fn damage_history_sums_scales_and_prunes_as_ace_does() {
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    h.player(P1, S1, "Tester", 10, 0);
    h.player(P2, S2, "Helper", 10, 0);
    let t0 = h.w.now.utc;

    damage_history::add(&mut h.w, MONSTER, P1, DamageType::Slash, 30);
    damage_history::add(&mut h.w, MONSTER, P2, DamageType::Pierce, 50);
    damage_history::add(&mut h.w, MONSTER, P1, DamageType::Slash, 10);
    damage_history::add(&mut h.w, MONSTER, P2, DamageType::Pierce, 0); // nothing
    let dh = damage_history::of(&h.w, MONSTER);
    assert_eq!(dh.log.len(), 3);
    assert_eq!(dh.log[0].amount, -30);
    assert_eq!((dh.log[0].current_health, dh.log[0].max_health), (100, 100));
    let totals: Vec<(ObjectGuid, f32)> = dh
        .total_damage
        .values()
        .map(|i| (i.guid, i.total_damage))
        .collect();
    assert_eq!(totals, [(P1, 40.0), (P2, 50.0)]);
    assert_eq!(dh.total_health(), 90.0);
    assert_eq!(dh.top_damager().unwrap().guid, P2);
    assert_eq!(dh.last_damager().unwrap().guid, P1);
    assert_eq!(dh.last_damager().unwrap().name.as_deref(), Some("Tester"));
    assert!(dh.has_damager(P1, true) && !dh.has_damager(MONSTER, false));
    assert_eq!(dh.get_top_damager(false).unwrap().guid, P2);

    // heal 20 after taking 90: missing before the heal was 100 - (30 - 20) = 90
    h.set_health(MONSTER, 30);
    damage_history::on_heal(&mut h.w, MONSTER, 20);
    let scalar = 1.0f32 - 20.0f32 / 90.0f32;
    let dh = damage_history::of(&h.w, MONSTER);
    assert_eq!(dh.log[3].attacker, ObjectGuid::INVALID);
    assert_eq!((dh.log[3].amount, dh.log[3].current_health), (20, 30));
    let totals: Vec<f32> = dh.total_damage.values().map(|i| i.total_damage).collect();
    assert_eq!(totals, [40.0 * scalar, 50.0 * scalar]);

    // an equal total keeps the first damager on top
    let mut tie = DamageHistory::new(MONSTER, t0);
    tie.total_damage.insert(
        P2,
        DamageHistoryInfo {
            total_damage: 5.0,
            ..h.info(P2)
        },
    );
    tie.total_damage.insert(
        P1,
        DamageHistoryInfo {
            total_damage: 5.0,
            ..h.info(P1)
        },
    );
    assert_eq!(tie.top_damager().unwrap().guid, P2);

    h.advance(120.0);
    damage_history::add(&mut h.w, MONSTER, P2, DamageType::Pierce, 5);
    h.advance(61.0);
    let now = h.w.now.utc;
    let dh = damage_history::of_mut(&mut h.w, MONSTER);
    dh.try_prune(now);
    assert_eq!(dh.log.len(), 1);
    let totals: Vec<(ObjectGuid, f32)> = dh
        .total_damage
        .values()
        .map(|i| (i.guid, i.total_damage))
        .collect();
    assert_eq!(totals, [(P2, 5.0)]);
    assert_eq!(dh.last_prune_time, now);
    // within 30 s of the last prune nothing happens
    dh.log[0].time = t0;
    dh.try_prune(now + TimeSpan::from_seconds(30.0));
    assert_eq!(dh.log.len(), 1);
    dh.reset();
    assert!(dh.log.is_empty() && dh.total_damage.is_empty());
}

// ------------------------------------------------------------------------------------ XP

/// Two players hurt a monster 25/75; its death splits `XpOverride` 1000 by damage share
/// (`Math.Round(xp * share)`), each through EarnXP -> GrantXP -> the player's queued
/// UpdateXpAndLevel. The last damager (a player) gets the killer notification; the death message
/// draws once from the damage type's table.
#[test]
fn a_creature_killed_by_two_attackers_grants_xp_by_damage_share() {
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    h.o_mut(MONSTER).set_property(PropertyInt::XpOverride, 1000);
    h.player(P1, S1, "Tester", 1, 0);
    h.player(P2, S2, "Helper", 1, 0);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(P1);
    h.place(P2);

    damage_history::add(&mut h.w, MONSTER, P1, DamageType::Slash, 25);
    damage_history::add(&mut h.w, MONSTER, P2, DamageType::Slash, 75);

    let mut reference = DotNetRandom::new(SEED);
    start_capture();
    let last = damage_history::of(&h.w, MONSTER).last_damager();
    let msg = dispatch::on_death::on_death(&mut h.w, MONSTER, last, DamageType::Slash, false);
    let idx = reference.next_range(0, 4);
    assert_eq!(msg, strings::SLASHING[usize::try_from(idx).unwrap()]);
    let msgs = sent();
    assert_eq!(
        event_text(&msgs, GameEventType::KillerNotification),
        [(S2, string_format(msg.killer, &["Drudge"]))]
    );
    // the XP waits on each player's queue
    assert_eq!(h.o(P1).total_experience(), Some(0));
    h.tick();
    assert_eq!(h.o(P1).total_experience(), Some(250));
    assert_eq!(h.o(P2).total_experience(), Some(750));
    assert_eq!(h.o(P2).available_experience(), Some(750));
    assert_eq!(h.o(P1).level(), Some(1));
    assert_eq!(h.o(P2).level(), Some(1));
    // a second OnDeath returns a message without granting again
    let _ = creature_death::creature_on_death(&mut h.w, MONSTER, None, DamageType::Slash, false);
    h.tick();
    assert_eq!(h.o(P2).total_experience(), Some(750));
}

/// A kill by a player without a level does not hang the world step.
/// V221.
#[test]
fn a_kill_by_a_player_without_a_level_does_not_hang_the_world_step() {
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    h.o_mut(MONSTER).set_property(PropertyInt::XpOverride, 1000);
    h.player(P1, S1, "Tester", 1, 0);
    let o = h.o_mut(P1);
    o.set_level(None);
    o.remove_property(PropertyInt64::TotalExperience);
    o.remove_property(PropertyInt64::AvailableExperience);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(P1);
    damage_history::add(&mut h.w, MONSTER, P1, DamageType::Slash, 100);
    let last = damage_history::of(&h.w, MONSTER).last_damager();
    let _ = dispatch::on_death::on_death(&mut h.w, MONSTER, last, DamageType::Slash, false);
    h.tick();
    let o = h.o(P1);
    assert_eq!(
        (o.level(), o.total_experience(), o.available_experience()),
        (None, None, None)
    );
    // with a Level the same kill grants as usual
    h.o_mut(P1).set_level(Some(1));
    player_xp::update_xp_and_level(&mut h.w, P1, 1000, XpType::Kill);
    assert_eq!(
        h.o(P1).level(),
        Some(1),
        "no TotalExperience: the grant stays null, no level-up"
    );
}

/// UpdateXpAndLevel: the level rises at the table's thresholds, adding the table's skill credits,
/// with ACE's messages; the grant is capped at the max level's XP.
#[test]
fn a_player_levels_up_at_the_table_threshold() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 1, 900);
    start_capture();

    player_xp::update_xp_and_level(&mut h.w, P1, 99, XpType::Kill);
    assert_eq!(
        (h.o(P1).level(), h.o(P1).total_experience()),
        (Some(1), Some(999))
    );
    assert!(chats(&sent()).is_empty());

    player_xp::update_xp_and_level(&mut h.w, P1, 101, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (
            o.level(),
            o.total_experience(),
            o.available_skill_credits(),
            o.total_skill_credits()
        ),
        (Some(2), Some(1100), Some(1), Some(1))
    );
    let msgs = sent();
    assert_eq!(
        chats(&msgs),
        ["You are now level 2!\nYou have 1,100 experience points and 1 skill credits available to raise skills and attributes."]
    );
    let ops: Vec<u32> = msgs.iter().map(|m| m.1).collect();
    let (int64, int) = (
        GameMessageOpcode::PrivateUpdatePropertyInt64.0,
        GameMessageOpcode::PrivateUpdatePropertyInt.0,
    );
    assert_eq!(
        ops[..3],
        [int64, int64, int],
        "TotalExperience, AvailableExperience, then Level"
    );
    assert_eq!(
        ops[ops.len() - 2..],
        [GameMessageOpcode::ServerMessage.0, int],
        "the chat, then AvailableSkillCredits"
    );

    // level 3 grants no credit: the message names the next level with one
    player_xp::update_xp_and_level(&mut h.w, P1, 1500, XpType::Quest);
    let chats3 = chats(&sent());
    assert_eq!(
        chats3,
        [
            "You are now level 3!\nYou have 2,600 experience points and 1 skill credits available to raise skills and attributes.\nYou will earn another skill credit at level 4.",
            "You've earned 1,500 experience."
        ]
    );

    // past the curve: capped at 5,000 total, max level
    player_xp::update_xp_and_level(&mut h.w, P1, 10_000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (
            o.level(),
            o.total_experience(),
            o.available_experience(),
            o.available_skill_credits()
        ),
        (Some(4), Some(5000), Some(5000), Some(2))
    );
    assert_eq!(
        chats(&sent()),
        ["You have reached the maximum level of 4!\nYou have 5,000 experience points and 2 skill credits available to raise skills and attributes."]
    );
    assert!(player_xp::is_max_level(&h.w, P1));
    // at max level nothing more is added
    player_xp::update_xp_and_level(&mut h.w, P1, 10, XpType::Kill);
    assert_eq!(h.o(P1).total_experience(), Some(5000));
}

/// An era's level cap below the table's last level: advancement stops at the cap and the curve
/// helpers end there. Infiltration caps the 275-level table at 126; its experience keeps arriving
/// past the cap's (V434).
/// Divergence: V390
#[test]
fn an_era_level_cap_stops_advancement_below_the_tables_last_level() {
    // levels 0..=275, level n at n * 1,000 XP, a credit every level
    let table = XpTable {
        id: DataId(file_id::XP_TABLE),
        level_xp: (0..=275u64).map(|n| n * 1_000).collect(),
        level_credits: (0..=275u32).map(|n| u32::from(n > 1)).collect(),
        ..empyrean_dat::fake::sample::xp_table()
    };
    let mut h = H::with_xp_table(table.clone());
    assert_eq!(
        player_xp::get_max_level(&h.w),
        275,
        "the end of retail: the table's"
    );
    h.w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert_eq!(player_xp::get_max_level(&h.w), 126);
    assert_eq!(player_xp::max_level_xp(&h.w), 126_000);

    h.player(P1, S1, "Tester", 125, 125_000);
    start_capture();
    player_xp::update_xp_and_level(&mut h.w, P1, 10_000_000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (o.level(), o.total_experience(), o.available_experience()),
        (Some(126), Some(10_125_000), Some(10_125_000)),
        "level 126 however far past its experience"
    );
    assert!(chats(&sent())[0].starts_with("You have reached the maximum level of 126!"));
    assert!(player_xp::is_max_level(&h.w, P1));
    player_xp::update_xp_and_level(&mut h.w, P1, 10, XpType::Kill);
    assert_eq!(h.o(P1).level(), Some(126), "no level past the cap");

    // The same grant on the end of retail's rules reaches the table's levels.
    let mut h = H::with_xp_table(table);
    h.player(P1, S1, "Tester", 125, 125_000);
    player_xp::update_xp_and_level(&mut h.w, P1, 10_000, XpType::Kill);
    assert_eq!(h.o(P1).level(), Some(135));
}

/// The curve helpers against the table (see also the `death/xp_curve` vectors).
#[test]
fn xp_curve_helpers() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 2, 1200);
    let w = &h.w;
    assert_eq!(player_xp::get_max_level(w), 4);
    assert_eq!(player_xp::max_level_xp(w), 5000);
    assert_eq!(player_xp::get_total_xp(w, 3), 2500);
    assert_eq!(player_xp::get_total_xp(w, 5), 0);
    assert_eq!(player_xp::get_total_xp(w, -1), 0);
    assert_eq!(
        player_xp::get_xp_between_levels(w, 0, 9),
        5000,
        "A clamps to 1, B to 4"
    );
    assert_eq!(
        player_xp::get_xp_between_levels(w, 4, 4),
        2500,
        "A clamps to 3"
    );
    assert_eq!(player_xp::get_xp_to_next_level(w, 2), 1500);
    assert_eq!(player_xp::get_remaining_xp(w, P1), 1300);
    assert_eq!(player_xp::get_remaining_xp_to_level(w, P1, 4), Some(3800));
    assert_eq!(player_xp::get_remaining_xp_to_level(w, P1, 0), None);
    // SpendXP / RefundXP now live in Player_Xp; player_skills re-exports them
    assert!(!player_skills::spend_xp(&mut h.w, P1, 1201, false));
    assert!(player_xp::spend_xp(&mut h.w, P1, 200, false));
    player_skills::refund_xp(&mut h.w, P1, 50);
    assert_eq!(h.o(P1).available_experience(), Some(1050));
    // EarnXP: xp_modifier 1.0 (DefaultPropertyManager), no enchantment bonus; GrantXP queues it
    player_xp::earn_xp(&mut h.w, P1, 10, XpType::Kill, ShareType::All);
}

// ------------------------------------------------------------------------------------ player death

/// Player.Die: health 0, NumDeaths, the vitae penalty (DeathLevel, VitaeCpPool, 5% vitae), the
/// purge; after the death animation (+1 s) the corpse (empty: 15 s to rot) and the teleport.
/// Earning XP afterwards pays the vitae back through the VitaeCPPool thresholds.
#[test]
fn vitae_after_a_player_death() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 50_000);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(P1);
    damage_history::add(&mut h.w, P1, MONSTER, DamageType::Bludgeon, 100);
    h.set_health(P1, 0);

    start_capture();
    let history = damage_history::of(&h.w, P1);
    let (last, top) = (history.last_damager(), history.top_damager());
    dispatch::die::die(&mut h.w, P1, last, top);
    let o = h.o(P1);
    assert_eq!(
        (
            o.num_deaths(),
            o.death_level(),
            o.vitae_cp_pool(),
            o.killer_id()
        ),
        (1, Some(10), Some(0), Some(MONSTER.full()))
    );
    assert!(emc::has_vitae(&mut h.w, P1));
    assert_eq!(
        empyrean_world::world_objects::managers::enchantment_manager::get_vitae(&h.w, P1)
            .unwrap()
            .stat_mod_value,
        0.95
    );
    let msgs = sent();
    let kinds: Vec<(u32, u32)> = msgs.iter().map(|m| (m.1, m.2)).collect();
    let int = GameMessageOpcode::PrivateUpdatePropertyInt.0;
    let event = GameMessageOpcode::GameEvent.0;
    let tail = &kinds[kinds.len() - 6..];
    assert_eq!(
        tail,
        [
            (GameMessageOpcode::PrivateUpdateAttribute2ndLevel.0, 0),
            (int, 0),
            (int, 0),
            (int, 0),
            (event, GameEventType::MagicUpdateEnchantment.0),
            (event, GameEventType::MagicPurgeEnchantments.0),
        ],
        "health, NumDeaths, DeathLevel, VitaeCpPool, the vitae enchantment, the purge"
    );

    // the corpse after the death animation (0 s until the motion table lands) + 1 s
    h.tick();
    assert!(h.find_corpse().is_none());
    h.advance(1.0);
    h.tick();
    let corpse = h.find_corpse().expect("a corpse");
    let c = h.o(corpse);
    assert_eq!(
        c.get_property(PropertyString::Name).as_deref(),
        Some("Corpse of Tester")
    );
    assert_eq!(c.long_desc().as_deref(), Some("Killed by Drudge."));
    assert_eq!(
        (c.victim_id(), c.killer_id()),
        (Some(P1.full()), Some(MONSTER.full()))
    );
    assert_eq!(
        (c.time_to_rot(), c.level()),
        (Some(corpse::EMPTY_DECAY_TIME), Some(10))
    );
    let chats_after = chats(&sent());
    assert!(chats_after.contains(
        &"You have retained all your items. You do not need to recover your corpse!".to_owned()
    ));

    // XP pays vitae back: each full pool of XP is 1%
    let threshold = |v: f32| -> i32 {
        empyrean_common::dotnet::CsCast::cs_cast(player_xp::vitae_cp_pool_threshold(v, 10))
    };
    let first = threshold(0.95);
    assert_eq!(first, 627, "(10^2.5 * 2.5 + 20) * 0.95^5 + 0.5");
    player_xp::update_xp_vitae(&mut h.w, P1, i64::from(first) + 100);
    assert_eq!(h.o(P1).vitae_cp_pool(), Some(100));
    let vitae = empyrean_world::world_objects::managers::enchantment_manager::get_vitae(&h.w, P1)
        .unwrap()
        .stat_mod_value;
    assert_eq!(vitae, 0.95 + 0.01);
    assert_eq!(
        chats(&sent()),
        ["Your experience has reduced your Vitae penalty!"]
    );
}

// ------------------------------------------------------------------------------------ death items

/// GetNumItemsDropped: none to level 10 (no draw), `Next(0, 1)` to 20, then `level / 20 +
/// Next(0, 2)` capped at 14, less 5 per Clutch of the Miser unless a PK death.
#[test]
fn item_loss_counts_follow_aces_rules_with_a_seed() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 1, 0);
    h.player(P2, S2, "Helper", 1, 0);
    h.corpse(ObjectGuid::new(0x8000_0100));
    let corpse = ObjectGuid::new(0x8000_0100);
    let mut reference = DotNetRandom::new(SEED);
    let count = |h: &mut H, level: i32, aug: i32| {
        h.o_mut(P1).set_level(Some(level));
        h.o_mut(P1)
            .set_property(PropertyInt::AugmentationLessDeathItemLoss, aug);
        player_death::get_num_items_dropped(&h.w, P1, corpse)
    };
    assert_eq!(count(&mut h, 5, 0), 0);
    assert_eq!(count(&mut h, 10, 0), 0);
    assert_eq!(count(&mut h, 15, 0), reference.next_range(0, 2));
    assert_eq!(count(&mut h, 20, 0), reference.next_range(0, 2));
    assert_eq!(count(&mut h, 21, 0), 1 + reference.next_range(0, 3));
    assert_eq!(count(&mut h, 126, 0), 6 + reference.next_range(0, 3));
    assert_eq!(
        count(&mut h, 400, 0),
        14.min(20 + reference.next_range(0, 3)),
        "capped"
    );
    assert_eq!(
        count(&mut h, 275, 2),
        0.max(14.min(13 + reference.next_range(0, 3)) - 10),
        "two Miser augs"
    );
    // a PK death ignores the augmentation
    h.o_mut(P1)
        .set_property_player_killer_status(PlayerKillerStatus::PK);
    h.o_mut(corpse).set_killer_id(Some(P2.full()));
    assert_eq!(
        count(&mut h, 275, 2),
        14.min(13 + reference.next_range(0, 3))
    );
    // coins: half above level 5
    h.o_mut(P1).set_property(PropertyInt::CoinValue, 1001);
    h.o_mut(P1).set_level(Some(6));
    assert_eq!(player_death::get_num_coins_dropped(&h.w, P1), 500);
    h.o_mut(P1).set_level(Some(5));
    assert_eq!(player_death::get_num_coins_dropped(&h.w, P1), 0);
}

/// DeathItems: known categories only, sorted by category then value, all but the most valuable of
/// a category halved, one variance draw per item in that order, then sorted by the final value.
#[test]
fn death_items_sort_halve_and_draw_in_aces_order() {
    let mut h = H::new();
    let sword = h.item(0x8000_0200, "Sword", ItemType::MeleeWeapon, 10_000);
    let armor = h.item(0x8000_0201, "Armor", ItemType::Armor, 5_000);
    let dagger = h.item(0x8000_0202, "Dagger", ItemType::MeleeWeapon, 3_000);
    let gem = h.item(0x8000_0203, "Gem", ItemType::Gem, 2_000);
    let pack = h.item(0x8000_0204, "Pack", ItemType::Container, 9_000);
    let notes = h.item(0x8000_0205, "Notes", ItemType::Misc, 600);
    h.o_mut(notes).set_property(PropertyInt::StackSize, 3);
    assert_eq!(
        DeathItem::get_category(ItemType::Container),
        DeathItemCategory::None
    );
    assert_eq!(
        DeathItem::get_category(ItemType::CraftFletchingBase),
        DeathItemCategory::CraftingIngredient
    );
    // The fletching base is the client's bit (V327); the bit ACE numbered it with is no type.
    assert_eq!(
        DeathItem::get_category(ItemType(0x0100_0000)),
        DeathItemCategory::CraftingIngredient
    );
    assert_eq!(
        DeathItem::get_category(ItemType(0x0200_0000)),
        DeathItemCategory::None
    );
    // The Brewmaster's Spine and Bibles are retail's CraftCookingBase: still an ingredient.
    assert_eq!(
        DeathItem::get_category(ItemType::CraftCookingBase),
        DeathItemCategory::CraftingIngredient
    );

    let mut reference = DotNetRandom::new(SEED);
    let d = DeathItems::new(&h.w, &[armor, gem, sword, pack, dagger, notes]);
    // draws in category order: Sword 10000, Dagger 1500 (halved), Armor 5000, Gem 2000, Notes 200
    let mut expected = Vec::new();
    for (g, v) in [
        (sword, 10_000),
        (dagger, 1_500),
        (armor, 5_000),
        (gem, 2_000),
        (notes, 200),
    ] {
        let variance = reference.next_double() * f64::from(0.1f32 - -0.1f32) + f64::from(-0.1f32);
        #[allow(clippy::cast_possible_truncation)]
        expected.push((g, (f64::from(v) + f64::from(v) * variance) as i32));
    }
    expected.sort_by_key(|e| std::cmp::Reverse(e.1));
    let got: Vec<(ObjectGuid, i32)> = d
        .inventory
        .iter()
        .map(|i| (i.world_object, i.adjusted_value))
        .collect();
    assert_eq!(got, expected);
    assert_eq!(
        got.iter().map(|g| g.0).collect::<Vec<_>>(),
        [sword, armor, gem, dagger, notes]
    );
    assert!(
        d.inventory_groups.get(&DeathItemCategory::None).is_some(),
        "groups are built before the filter"
    );
}

/// A level-40 player's corpse holds the rolled number of highest-valued items (bonded ones never
/// drop); the drop message lists them; the corpse rots after max(1 h, 5 min * level).
#[test]
fn a_corpse_is_created_with_the_right_contents_and_decay_time() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 40, 0);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    let sword = h.item(0x8000_0200, "Sword", ItemType::MeleeWeapon, 10_000);
    let armor = h.item(0x8000_0201, "Armor", ItemType::Armor, 5_000);
    let gem = h.item(0x8000_0203, "Gem", ItemType::Gem, 2_000);
    let dagger = h.item(0x8000_0202, "Dagger", ItemType::MeleeWeapon, 3_000);
    let bonded = h.item(0x8000_0206, "Heirloom", ItemType::Jewelry, 99_999);
    h.o_mut(bonded).set_property(PropertyInt::Bonded, 1);
    for g in [sword, gem, dagger, bonded] {
        assert!(creature_death::container_try_add_to_inventory(
            &mut h.w, P1, g
        ));
    }
    assert!(creature_death::creature_try_equip_object(
        &mut h.w,
        P1,
        armor,
        EquipMask::ChestArmor
    ));

    // on the landscape: `CreateCorpse` reads the physics body's position and velocity
    h.place(P1);

    let mut reference = DotNetRandom::new(SEED);
    let n = 2 + reference.next_range(0, 3);
    let killer = h.info(MONSTER);
    start_capture();
    creature_death::create_corpse(&mut h.w, P1, Some(&killer), false);
    let corpse = h.find_corpse().expect("a corpse");
    let order = [sword, armor, gem, dagger];
    let dropped = &order[..usize::try_from(n).unwrap()];
    assert_eq!(creature_death::container_inventory(&h.w, corpse), dropped);
    for g in dropped {
        assert_eq!(h.o(*g).container_id(), Some(corpse.full()));
    }
    assert!(creature_death::container_inventory(&h.w, P1).contains(&bonded));
    let c = h.o(corpse);
    assert_eq!((c.time_to_rot(), c.level()), (Some(12_000.0), Some(40)));
    assert_eq!(
        PKLevel(c.pk_level_modifier().try_into().unwrap()),
        PKLevel::NPK
    );
    let names = ["Sword", "Armor", "Gem", "Dagger"];
    let listed: Vec<String> = names[..usize::try_from(n).unwrap()]
        .iter()
        .map(|n| format!("your {n}"))
        .collect();
    let expected = if listed.len() == 2 {
        format!("You've lost {}, and {}!", listed[0], listed[1])
    } else {
        format!(
            "You've lost {}, and {}!",
            listed[..listed.len() - 1].join(", "),
            listed[listed.len() - 1]
        )
    };
    let msgs = chats(&sent());
    assert!(msgs.contains(&expected), "{msgs:?}");
    assert!(
        msgs.iter()
            .any(|m| m.starts_with("Your corpse is located at (")),
        "{msgs:?}"
    );
}

/// Corpse.HasPermission: the victim, the killer, a looted corpse, a /permit (used up, remembered
/// for this corpse), and anyone once a monster corpse is past half its rot time.
#[test]
fn corpse_looting_rights() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    h.player(P2, S2, "Helper", 10, 0);
    let corpse_guid = ObjectGuid::new(0x8000_0100);
    let mut c = WorldObject::allocate(Class::Corpse);
    c.guid = corpse_guid;
    c.set_victim_id(Some(P1.full()));
    c.set_killer_id(Some(MONSTER.full()));
    c.set_time_to_rot(Some(3600.0));
    h.w.objects.insert(c).unwrap();
    assert!(corpse::has_permission(&mut h.w, corpse_guid, P1));
    assert!(!corpse::has_permission(&mut h.w, corpse_guid, P2));
    let expires = h.w.now.utc + player_death::permit_time();
    h.o_mut(P2)
        .player
        .as_mut()
        .unwrap()
        .player_death
        .loot_permission
        .insert(P1, expires);
    assert!(corpse::has_permission(&mut h.w, corpse_guid, P2));
    assert!(
        h.o(P2)
            .player
            .as_ref()
            .unwrap()
            .player_death
            .loot_permission
            .is_empty(),
        "used up"
    );
    assert!(
        corpse::has_permission(&mut h.w, corpse_guid, P2),
        "remembered for this corpse"
    );
    // a monster corpse opens to all below the half-life
    let m = ObjectGuid::new(0x8000_0101);
    let mut mc = WorldObject::allocate(Class::Corpse);
    mc.guid = m;
    mc.set_victim_id(Some(MONSTER.full()));
    mc.set_killer_id(Some(P1.full()));
    mc.set_time_to_rot(Some(181.0));
    h.w.objects.insert(mc).unwrap();
    assert!(!corpse::has_permission(&mut h.w, m, P2));
    h.o_mut(m).set_time_to_rot(Some(179.0));
    assert!(corpse::has_permission(&mut h.w, m, P2));
    h.o_mut(m).set_corpse_generated_rare(true);
    assert!(!corpse::has_permission(&mut h.w, m, P2));
    corpse::corpse_close(&mut h.w, m, P1);
    assert!(corpse::has_permission(&mut h.w, m, P2), "looted");
}

/// V322: revoking the /permit of a player who is not online answers
/// that the player has no permission (ACE threw on the missing player's name and answered nothing).
#[test]
fn revoking_a_permit_of_a_player_not_online_is_answered() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    start_capture();
    player_death::handle_action_remove_player_permission(&mut h.w, P1, "Nobody");
    assert_eq!(
        chats(&sent()),
        ["Nobody doesn't have permission to loot your corpse."]
    );
}

/// A monster's death chain: KillerId and the killer's CreatureKills at once; after the (0 s) death
/// animation its corpse (named, killed by, monster, no Value) and the monster is destroyed.
#[test]
fn a_monster_dies_leaves_a_corpse_and_is_destroyed() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    h.o_mut(MONSTER).set_level(Some(12));
    h.o_mut(MONSTER).set_property(PropertyInt::Value, 55);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(MONSTER);
    damage_history::add(&mut h.w, MONSTER, P1, DamageType::Slash, 100);
    creature_death::die(&mut h.w, MONSTER);
    assert_eq!(h.health(MONSTER), 0);
    assert_eq!(h.o(MONSTER).killer_id(), Some(P1.full()));
    assert_eq!(h.o(P1).creature_kills(), Some(1));
    // ExecuteMotion(Dead) made the death motion current (and broadcast it)
    let motion = h
        .o(MONSTER)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
        .expect("a motion");
    assert_eq!(
        motion.motion_state.forward_command,
        empyrean_entity::enums::MotionCommand::Dead
    );
    creature_death::die(&mut h.w, MONSTER); // entered once
    assert_eq!(h.o(P1).creature_kills(), Some(1));
    h.tick();
    assert!(h.w.objects.get(MONSTER).is_none(), "destroyed");
    let corpse = h.find_corpse().expect("a corpse");
    let c = h.o(corpse);
    assert_eq!(
        c.get_property(PropertyString::Name).as_deref(),
        Some("Corpse of Drudge")
    );
    assert_eq!(c.long_desc().as_deref(), Some("Killed by Tester."));
    assert_eq!(
        (c.victim_id(), c.killer_id()),
        (Some(MONSTER.full()), Some(P1.full()))
    );
    assert_eq!(c.get_property(PropertyInt::Value), None);
    assert!(corpse::fields(&h.w, corpse).is_monster);
}

/// Divergence: V398
/// A kill of a higher-level creature rolls for a rare at the end of retail (with real-time rares
/// the roll starts the killer's rare timer) and not at all in an era without rares.
#[test]
fn an_era_without_rares_rolls_none_on_a_kill() {
    for (era, rolled) in [
        (empyrean_common::era::EraId::Eor, true),
        (empyrean_common::era::EraId::Infiltration, false),
    ] {
        let mut h = H::new();
        h.w.era = era.rules();
        h.player(P1, S1, "Tester", 10, 0);
        h.creature(Class::Creature, MONSTER, "Drudge", 100);
        h.o_mut(MONSTER).set_level(Some(12));
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.place(MONSTER);
        damage_history::add(&mut h.w, MONSTER, P1, DamageType::Slash, 100);
        creature_death::die(&mut h.w, MONSTER);
        h.tick();
        assert!(h.find_corpse().is_some(), "{era}: a corpse");
        assert_eq!(
            h.o(P1).rares_login_timestamp().is_some(),
            rolled,
            "{era}: the rare roll"
        );
    }
}

// ------------------------------------------------------------------------------------ messages

/// Strings.GetDeathMessage and Creature.GetDeathMessage: no killer, a suicide or an unknown damage
/// type answers `General[1]` without a draw; otherwise one draw in the damage type's table (a
/// critical one from `Critical`, and `PKCritical[0]` for a critical kill of a player by a player).
/// Player.OnDeath formats the victim text with the victim's and killer's names.
#[test]
fn the_death_message_text() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    h.player(P2, S2, "Helper", 10, 0);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    let mut reference = DotNetRandom::new(SEED);

    assert_eq!(
        creature_death::get_death_message(&mut h.w, MONSTER, None, DamageType::Fire, false),
        strings::GENERAL[1]
    );
    let own = h.info(MONSTER);
    assert_eq!(
        creature_death::get_death_message(&mut h.w, MONSTER, Some(&own), DamageType::Fire, false),
        strings::GENERAL[1]
    );
    assert_eq!(
        strings::get_death_message(DamageType::Physical, false),
        strings::GENERAL[1]
    );
    assert_eq!(strings::GENERAL[1].victim, "You died!");

    let fire = strings::get_death_message(DamageType::Fire, false);
    assert_eq!(
        fire,
        strings::FIRE[usize::try_from(reference.next_range(0, 4)).unwrap()]
    );
    let crit = strings::get_death_message(DamageType::Fire, true);
    assert_eq!(
        crit,
        strings::CRITICAL[usize::try_from(reference.next_range(0, 7)).unwrap()]
    );
    assert_eq!(
        strings::death_messages(DamageType::Health),
        Some(&strings::GENERAL[..])
    );
    assert_eq!(strings::death_messages(DamageType::Stamina), None);

    // a player killed by a player with a critical: PKCritical, after the Critical draw
    let killer = h.info(P2);
    start_capture();
    let msg = dispatch::on_death::on_death(&mut h.w, P1, Some(killer), DamageType::Cold, true);
    reference.next_range(0, 7);
    assert_eq!(msg, strings::PK_CRITICAL[0]);
    let msgs = sent();
    assert_eq!(
        event_text(&msgs, GameEventType::KillerNotification),
        [(
            S2,
            "You send Tester to death so violently that even the lifestone flinches!".to_owned()
        )]
    );
    assert_eq!(
        event_text(&msgs, GameEventType::VictimNotification),
        [(
            S1,
            "Helper sends you to your death so violently that even the lifestone flinches!"
                .to_owned()
        )]
    );
    // the next draw is the reference's next
    let after = strings::get_death_message(DamageType::Slash, false);
    assert_eq!(
        after,
        strings::SLASHING[usize::try_from(reference.next_range(0, 4)).unwrap()]
    );
    // a victim message without a killer is the raw text
    assert_eq!(
        string_format(strings::BLUDGEONING[3].victim, &["V", "K"]),
        "The thunder of K crushing V is followed by the deafening silence of your death!"
    );
}

/// DropMessage: "You've lost ..., and ...!", coins once with the dropped amount, stacks plural.
#[test]
fn drop_message_lists_the_items() {
    let mut h = H::new();
    let a = h.item(0x8000_0300, "Sword", ItemType::MeleeWeapon, 1);
    let coins = h.item(0x8000_0301, "Pyreal", ItemType::Money, 1);
    let b = h.item(0x8000_0302, "Gem", ItemType::Gem, 1);
    h.o_mut(b).set_property(PropertyInt::StackSize, 1200);
    h.o_mut(b)
        .set_property(PropertyString::PluralName, "Gems".to_owned());
    assert_eq!(
        player_death::drop_message(&h.w, &[a], 0, &player_death::DeadItems::new()),
        "You've lost your Sword!"
    );
    assert_eq!(
        player_death::drop_message(&h.w, &[coins, a, b], 250, &player_death::DeadItems::new()),
        "You've lost 250 Pyreals, your Sword, and your 1,200 Gems!"
    );
    assert_eq!(
        player_death::drop_message(&h.w, &[], 0, &player_death::DeadItems::new()),
        ""
    );

    let mut dead = player_death::DeadItems::new();
    dead.remember(&h.w, coins);
    dead.remember(&h.w, b);
    h.w.objects.remove(coins);
    h.w.objects.remove(b);
    assert_eq!(
        player_death::drop_message(&h.w, &[coins, a, b], 250, &dead),
        "You've lost 250 Pyreals, your Sword, and your 1,200 Gems!"
    );
}

// ------------------------------------------------------------------------------------ vectors

fn u(v: &serde_json::Value) -> u64 {
    v.as_u64().unwrap_or_else(|| panic!("not a uint: {v}"))
}

/// ACE's own `Player` XP-curve statics over the synthetic table `death/tables` records, and
/// `VitaeCPPoolThreshold` (private, called by reflection).
#[test]
fn xp_curve_and_vitae_pool_match_ace() {
    let file = vectors::load_named("death", "xp_curve");
    let table = &file.cases[0].input["level_xp"];
    let credits = &file.cases[0].input["level_credits"];
    let dats = FakeDats::new()
        .with_xp_table(XpTable {
            level_xp: table.as_array().unwrap().iter().map(u).collect(),
            level_credits: credits
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u32::try_from(u(v)).unwrap())
                .collect(),
            ..xp_table()
        })
        .build()
        .unwrap();
    let w = World::new(H::new().w.now, dats);
    let mut failures = Vec::new();
    for Case { input, output, .. } in &file.cases {
        let got = match input["op"].as_str().unwrap() {
            "table" => continue,
            "max_level" => serde_json::json!(player_xp::get_max_level(&w)),
            "max_level_xp" => serde_json::json!(player_xp::max_level_xp(&w)),
            "total_xp" => serde_json::json!(player_xp::get_total_xp(
                &w,
                i32::try_from(input["level"].as_i64().unwrap()).unwrap()
            )),
            "between" => {
                let (a, b) = (input["a"].as_i64().unwrap(), input["b"].as_i64().unwrap());
                serde_json::json!(player_xp::get_xp_between_levels(
                    &w,
                    i32::try_from(a).unwrap(),
                    i32::try_from(b).unwrap()
                ))
            }
            "vitae_pool" => {
                let vitae = empyrean_common::vectors::f32_of(&input["vitae"]).unwrap();
                let level = i32::try_from(input["level"].as_i64().unwrap()).unwrap();
                serde_json::json!(player_xp::vitae_cp_pool_threshold(vitae, level))
            }
            other => panic!("unknown op {other}"),
        };
        let same = match (&got, output) {
            (serde_json::Value::Number(g), serde_json::Value::Number(o)) => {
                g.as_f64()
                    .zip(o.as_f64())
                    .is_some_and(|(a, b)| a.to_bits() == b.to_bits())
                    || g == o
            }
            _ => got == *output,
        };
        if !same {
            failures.push(format!("{input} expected {output} got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// ACE's `Player.GetNumItemsDropped` and `GetNumCoinsDropped` on uninitialised players over a
/// seeded `ThreadSafeRandom`: the count and the draws, per level, augmentation and PK death.
#[test]
fn item_loss_count_maths_match_ace() {
    let file = vectors::load_named("death", "items_dropped");
    let mut h = H::new();
    h.player(P1, S1, "Tester", 1, 0);
    h.player(P2, S2, "Helper", 1, 0);
    let corpse = ObjectGuid::new(0x8000_0100);
    h.corpse(corpse);
    let mut failures = Vec::new();
    for Case { input, output, .. } in &file.cases {
        ThreadSafeRandom::seed(u(&input["seed"]));
        let o = h.o_mut(P1);
        o.set_level(input["level"].as_i64().map(|l| i32::try_from(l).unwrap()));
        o.set_property(
            PropertyInt::AugmentationLessDeathItemLoss,
            i32::try_from(input["aug"].as_i64().unwrap()).unwrap(),
        );
        o.set_property(
            PropertyInt::CoinValue,
            i32::try_from(input["coins"].as_i64().unwrap()).unwrap(),
        );
        let status = PlayerKillerStatus(u32::try_from(u(&input["pk_status"])).unwrap());
        o.set_property_player_killer_status(status);
        h.o_mut(corpse)
            .set_killer_id(input["killer"].as_u64().map(|k| u32::try_from(k).unwrap()));
        let items = player_death::get_num_items_dropped(&h.w, P1, corpse);
        let coins = player_death::get_num_coins_dropped(&h.w, P1);
        let next = ThreadSafeRandom::next(0, 1_000_000);
        let got = serde_json::json!({"items": items, "coins": coins, "next": next});
        if got != *output {
            failures.push(format!("{input} expected {output} got {got}"));
        }
    }
    assert!(!file.cases.is_empty());
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        file.cases.len(),
        failures.join("\n")
    );
}

/// Test-only: `PlayerKillerStatus` through the plain property setter.
trait SetPk {
    fn set_property_player_killer_status(&mut self, v: PlayerKillerStatus);
}

impl SetPk for WorldObject {
    fn set_property_player_killer_status(&mut self, v: PlayerKillerStatus) {
        self.set_player_killer_status_prop(v);
    }
}

fn tick_entry(
    caster: ObjectGuid,
    amount: f32,
) -> empyrean_entity::models::PropertiesEnchantmentRegistry {
    empyrean_entity::models::PropertiesEnchantmentRegistry {
        caster_object_id: caster.full(),
        stat_mod_value: amount,
        ..Default::default()
    }
}

/// `EnchantmentManager.ApplyHealingTick` (EnchantmentManager.cs): the ticks' total, times the
/// healing rating mod (1 here), rounded, is healed; the damage history records the heal.
#[test]
fn a_heal_over_time_tick_heals_the_total() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(MONSTER);
    let health = h.o(MONSTER).health();
    empyrean_world::world_objects::creature_vitals::update_vital_delta(
        &mut h.w, MONSTER, health, -40,
    );
    assert_eq!(h.health(MONSTER), 60);

    empyrean_world::world_objects::managers::enchantment_manager::apply_healing_tick(
        &mut h.w,
        MONSTER,
        &[tick_entry(P1, 7.0), tick_entry(P1, 5.6)],
    );
    assert_eq!(h.health(MONSTER), 73, "Math.Round(12.6) = 13");
}

/// `EnchantmentManager.ApplyDamageTick`: each tick from a damager on the landblock (resistance and
/// rating mods 1 here) is added to the damage history and the total is taken as damage over
/// time; a tick from a caster not on the landblock is skipped.
#[test]
fn a_damage_over_time_tick_hurts_and_records_the_damager() {
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(MONSTER);
    h.place(P1);

    let absent = ObjectGuid::new(0x5000_0099);
    empyrean_world::world_objects::managers::enchantment_manager::apply_damage_tick(
        &mut h.w,
        MONSTER,
        &[tick_entry(P1, 12.0), tick_entry(absent, 50.0)],
        DamageType::Fire,
        false,
    );
    assert_eq!(h.health(MONSTER), 88);
    let top = damage_history::of(&h.w, MONSTER)
        .top_damager()
        .expect("a damager");
    assert_eq!(top.guid, P1);

    // An invincible creature (not only a player) takes no damage over time (ACE c5f16b44; before
    // it, ACE left the test to the player's own damage-over-time path).
    h.w.objects.get_mut(MONSTER).unwrap().set_invincible(true);
    empyrean_world::world_objects::managers::enchantment_manager::apply_damage_tick(
        &mut h.w,
        MONSTER,
        &[tick_entry(P1, 12.0)],
        DamageType::Fire,
        false,
    );
    assert_eq!(h.health(MONSTER), 88);
}

/// `Corpse.CalculateObjDesc` (Corpse.cs) with a saved appearance: `AddBaseModelData` first (a head
/// without a hair style is anim part 0x10, the base palette), then the saved parts.
#[test]
fn a_corpse_with_a_saved_appearance_keeps_its_head() {
    use empyrean_entity::enums::{PropertyDataId, PropertyInt};
    use empyrean_entity::models::PropertiesAnimPart;
    let mut h = H::new();
    let c = ObjectGuid::new(0x8000_0200);
    h.corpse(c);
    let o = h.o_mut(c);
    o.biota.properties_anim_part = Some(vec![PropertiesAnimPart {
        index: 1,
        animation_id: 0x0100_0001,
    }]);
    o.set_property(PropertyDataId::HeadObject, 0x0100_04AF);
    o.set_property(PropertyInt::HeritageGroup, 1);
    o.set_property(PropertyDataId::PaletteBase, 0x0400_007E);

    let d = corpse::corpse_calculate_obj_desc(&mut h.w, c);
    assert_eq!(d.palette_id, 0x0400_007E);
    let parts: Vec<(u8, u32)> = d
        .anim_part_changes
        .iter()
        .map(|p| (p.index, p.animation_id))
        .collect();
    assert_eq!(parts, [(0x10, 0x0100_04AF), (1, 0x0100_0001)]);
}

/// A corpse takes the bodys physics position and velocity.
#[test]
fn a_corpse_takes_the_bodys_physics_position_and_velocity() {
    use empyrean_world::physics::phys_ext;
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    h.place(MONSTER);
    let body = h.o(MONSTER).phys.expect("a body");
    let physics_position = phys_ext::position(&h.w, body).expect("placed");
    // the Location drifts from the body (as a player's does between AutonomousPositions)
    h.o_mut(MONSTER)
        .set_location(Some(Position::from_components(
            LB | 0x0001,
            50.0,
            60.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
    phys_ext::set_velocity_field(
        &mut h.w,
        body,
        empyrean_entity::Vector3::new(0.0, 0.0, -3.0),
    );

    creature_death::create_corpse(&mut h.w, MONSTER, None, false);
    let corpse = h.find_corpse().expect("a corpse");
    let location = h.o(corpse).location().expect("on the landscape");
    assert_eq!(
        (location.cell(), location.position_x, location.position_y),
        (
            physics_position.cell.0,
            physics_position.frame.origin.x,
            physics_position.frame.origin.y
        )
    );
    let corpse_body = h.o(corpse).phys.expect("the corpse's body");
    assert_eq!(
        phys_ext::velocity(&h.w, corpse_body),
        empyrean_entity::Vector3::new(0.0, 0.0, -3.0)
    );
}

/// The physics description carries the bodys acceleration and omega.
#[test]
fn the_physics_description_carries_the_bodys_acceleration_and_omega() {
    use empyrean_entity::enums::PhysicsDescriptionFlag as F;
    use empyrean_world::world_objects::world_object_networking::calculated_physics_description_flag;
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    h.place(MONSTER);
    let flags = calculated_physics_description_flag(&h.w, MONSTER);
    assert!(!flags.intersects(F::Acceleration | F::Omega));
    let body = h.o(MONSTER).phys.expect("a body");
    let b = h.w.physics.get_mut(body).expect("live");
    b.acceleration_vector.z = -9.8;
    b.omega_vector.z = 1.0;
    let flags = calculated_physics_description_flag(&h.w, MONSTER);
    assert!(flags.contains(F::Acceleration | F::Omega), "{flags:?}");
}

/// A players stance mod follows its current movement data.
#[test]
fn a_players_stance_mod_follows_its_current_movement_data() {
    use empyrean_entity::enums::{MotionCommand, MovementType};
    use empyrean_world::network::motion::movement_data::MovementData;
    use empyrean_world::network::motion::movement_invalid::MovementInvalid;
    let mut h = H::new();
    h.player(P1, S1, "Tester", 10, 0);
    let health = |h: &H| {
        h.o(P1)
            .vitals()
            .get(&PropertyAttribute2nd::MaxHealth)
            .cloned()
            .expect("health")
    };
    assert_eq!(h.o(P1).get_stance_mod(health(&h)), 1.0);

    for (command, expected) in [
        (MotionCommand::Sitting, 2.5f32),
        (MotionCommand::Sleeping, 3.0),
        (MotionCommand::Crouch, 2.0),
        (MotionCommand::RunForward, 0.5),
    ] {
        let mut data = MovementData::new(P1);
        data.movement_type = MovementType::Invalid;
        let mut invalid = MovementInvalid::new(&data);
        invalid.state.forward_command = command;
        data.invalid = Some(invalid);
        h.o_mut(P1).wo.world_object_properties.current_movement_data = data;
        assert_eq!(h.o(P1).get_stance_mod(health(&h)), expected, "{command:?}");
    }
}

/// `Player.VitaeCPPoolThreshold`, as its callers use it (cast to `int`), against
/// the client's `vitae_cp_pool_threshold` (`dereth_rules::advancement`) for every vitae a
/// player can carry (1% steps from the 40% floor to none) and every DeathLevel from 1 to 275. The
/// same function there; the ACE port keeps its `double` result because the vectors pin it. (A
/// negative DeathLevel, which no death records, would differ: the client takes the level unsigned.)
#[test]
fn rule3_vitae_cp_pool_threshold_agrees_with_dereth_rules() {
    let mut bad = Vec::new();
    for step in 0..=60u8 {
        let vitae = 1.0f32 - f32::from(step) * 0.01;
        for level in 1..=275i32 {
            let ours: i32 = empyrean_common::dotnet::CsCast::cs_cast(
                player_xp::vitae_cp_pool_threshold(vitae, level),
            );
            let theirs = dereth_rules::advancement::vitae_cp_pool_threshold(
                f64::from(vitae),
                f64::from(level),
            );
            if ours != theirs {
                bad.push(format!(
                    "vitae {vitae} level {level}: ACE {ours}, client {theirs}"
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} disagreements:\n  {}",
        bad.len(),
        bad.iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// Divergence: V400
/// Before Throne of Destiny a point of vitae costs `(level^2 * 5 + 20) * vitae^5 + 0.5`, at most
/// 12,500, and the client's curve for the era agrees at every vitae and DeathLevel; a player
/// dying at level 10 in that era wins back a point with 402 experience rather than 627.
#[test]
fn an_era_before_throne_of_destiny_prices_vitae_by_the_older_curve() {
    use empyrean_common::era::VitaeRecovery;
    let older = VitaeRecovery::BeforeThroneOfDestiny;
    let at = |v: f32, level: i32| -> i32 {
        empyrean_common::dotnet::CsCast::cs_cast(player_xp::vitae_cp_pool_threshold_in(
            older, v, level,
        ))
    };
    assert_eq!(at(0.95, 10), 402, "(10^2 * 5 + 20) * 0.95^5 + 0.5");
    assert_eq!(at(1.0, 126), 12_500, "capped");
    assert_eq!(
        at(0.6, 126),
        6_174,
        "under the cap once the penalty is deep"
    );
    for step in 0..=60u8 {
        let vitae = 1.0f32 - f32::from(step) * 0.01;
        for level in 1..=275i32 {
            assert_eq!(
                at(vitae, level),
                dereth_rules::advancement::vitae_cp_pool_threshold_in(
                    older,
                    f64::from(vitae),
                    f64::from(level)
                ),
                "vitae {vitae} level {level}"
            );
            assert_eq!(
                player_xp::vitae_cp_pool_threshold_in(VitaeRecovery::EndOfRetail, vitae, level)
                    .to_bits(),
                player_xp::vitae_cp_pool_threshold(vitae, level).to_bits(),
                "the end of retail's is ACE's"
            );
        }
    }

    let mut h = H::new();
    h.w.era = empyrean_common::era::EraId::Infiltration.rules();
    h.player(P1, S1, "Tester", 10, 50_000);
    h.creature(Class::Creature, MONSTER, "Drudge", 100);
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.place(P1);
    damage_history::add(&mut h.w, P1, MONSTER, DamageType::Bludgeon, 100);
    h.set_health(P1, 0);
    let history = damage_history::of(&h.w, P1);
    let (last, top) = (history.last_damager(), history.top_damager());
    dispatch::die::die(&mut h.w, P1, last, top);
    player_xp::update_xp_vitae(&mut h.w, P1, 402 + 100);
    assert_eq!(h.o(P1).vitae_cp_pool(), Some(100));
    let vitae = empyrean_world::world_objects::managers::enchantment_manager::get_vitae(&h.w, P1)
        .unwrap()
        .stat_mod_value;
    assert_eq!(vitae, 0.95 + 0.01, "one point back for 402");
}

/// Divergence: V405
/// From level 21 a player drops `level / 10` plus 0 to 2 items before the later halving: 12 plus
/// the draw at level 126, where the end of retail drops 6 plus it.
#[test]
fn an_era_before_the_halving_drops_a_tenth_of_the_level_in_items() {
    let mut h = H::new();
    h.w.era = empyrean_common::era::EraId::Infiltration.rules();
    h.player(P1, S1, "Tester", 1, 0);
    h.corpse(ObjectGuid::new(0x8000_0100));
    let corpse = ObjectGuid::new(0x8000_0100);
    let mut reference = DotNetRandom::new(SEED);
    let mut count = |level: i32| {
        h.o_mut(P1).set_level(Some(level));
        player_death::get_num_items_dropped(&h.w, P1, corpse)
    };
    assert_eq!(count(10), 0);
    assert_eq!(count(20), reference.next_range(0, 2));
    assert_eq!(count(21), 2 + reference.next_range(0, 3));
    assert_eq!(count(126), 12 + reference.next_range(0, 3));
}

/// Divergence: V406
/// Unassigned experience stops at 4,294,967,295 before Throne of Destiny (a 32-bit count), and so
/// does the total (V434). The end of retail has no such cap.
#[test]
fn an_era_before_throne_of_destiny_caps_unassigned_experience_at_32_bits() {
    let big = i64::from(u32::MAX) - 10;
    let mut h = H::with_xp_table(XpTable {
        level_xp: vec![0, 0, 1000, 2500, 20_000_000_000],
        level_credits: vec![0, 0, 1, 1, 1],
        ..xp_table()
    });
    h.player(P1, S1, "Tester", 3, 5_000_000_000);
    h.o_mut(P1)
        .set_property(PropertyInt64::AvailableExperience, big);
    player_xp::update_xp_and_level(&mut h.w, P1, 1000, XpType::Kill);
    assert_eq!(h.o(P1).available_experience(), Some(big + 1000), "no cap");

    h.w.era = empyrean_common::era::EraId::Infiltration.rules();
    h.o_mut(P1)
        .set_property(PropertyInt64::AvailableExperience, big);
    player_xp::update_xp_and_level(&mut h.w, P1, 1000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(o.available_experience(), Some(i64::from(u32::MAX)));
    assert_eq!(
        o.total_experience(),
        Some(i64::from(u32::MAX)),
        "the total is a 32-bit count too"
    );
}

/// Divergence: V434
/// Before Throne of Destiny experience keeps arriving at the level cap: the total and the
/// unassigned counts each stop at 4,294,967,295, and a character at level 126 who spends its
/// unassigned experience earns again while the level stays 126. The end of retail adds nothing at
/// the cap.
#[test]
fn an_era_before_throne_of_destiny_keeps_earning_at_the_level_cap_up_to_32_bit_counts() {
    // The February 2005 curve's last two levels: 125 and 126 (4,286,609,098).
    let mut level_xp: Vec<u64> = (0..=126u64).map(|n| n * 1_000).collect();
    level_xp[125] = 4_200_000_000;
    level_xp[126] = 4_286_609_098;
    let table = XpTable {
        level_xp,
        level_credits: vec![0; 127],
        ..xp_table()
    };
    let max = i64::from(u32::MAX);

    let mut h = H::with_xp_table(table.clone());
    h.w.era = empyrean_common::era::EraId::Infiltration.rules();
    h.player(P1, S1, "Tester", 126, 4_286_609_098);
    h.o_mut(P1)
        .set_property(PropertyInt64::AvailableExperience, 0);
    player_xp::update_xp_and_level(&mut h.w, P1, 5_000_000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (o.level(), o.total_experience(), o.available_experience()),
        (Some(126), Some(4_291_609_098), Some(5_000_000)),
        "experience arrives at the cap"
    );
    player_xp::update_xp_and_level(&mut h.w, P1, 4_000_000_000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (o.level(), o.total_experience(), o.available_experience()),
        (Some(126), Some(max), Some(4_005_000_000)),
        "the total stops at 32 bits; unassigned is still under its cap"
    );
    player_xp::update_xp_and_level(&mut h.w, P1, 1_000_000_000, XpType::Kill);
    assert_eq!(
        h.o(P1).available_experience(),
        Some(max),
        "unassigned stops at 32 bits"
    );

    // Spend it all (on Strength, say): the next kill fills it again.
    assert!(player_xp::spend_xp(&mut h.w, P1, max, false));
    assert_eq!(h.o(P1).available_experience(), Some(0));
    player_xp::update_xp_and_level(&mut h.w, P1, 1_000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (o.level(), o.total_experience(), o.available_experience()),
        (Some(126), Some(max), Some(1_000))
    );

    // The end of retail's rule: nothing at the cap.
    let mut h = H::with_xp_table(table);
    h.player(P1, S1, "Tester", 126, 4_286_609_098);
    h.o_mut(P1)
        .set_property(PropertyInt64::AvailableExperience, 0);
    player_xp::update_xp_and_level(&mut h.w, P1, 5_000_000, XpType::Kill);
    let o = h.o(P1);
    assert_eq!(
        (o.total_experience(), o.available_experience()),
        (Some(4_286_609_098), Some(0))
    );
}

mod corpse_limit {
    use crate::support::content_interactions::*;

    /// Over `corpse_spam_limit` with no corpse that has more than `Corpse.EmptyDecayTime` (15 s) left
    /// to rot, the corpse is still added and none is shortened. ACE's search for the oldest such
    /// corpse threw, failing the add (and the death that made the corpse). A later corpse over the
    /// limit still shortens the oldest one that has time left.
    #[test]
    fn a_corpse_over_the_limit_is_added_when_none_can_be_shortened() {
        let mut h = H::new();
        assert!(pm::modify_long(&h.w, "corpse_spam_limit", 0));

        let empty = corpse(&mut h.w, 0x8000_0200, 100, 15.0);
        let added = catch_unwind(AssertUnwindSafe(|| lm::add_object(&mut h.w, empty, false)));
        assert!(
            matches!(added, Ok(true)),
            "the corpse joins its landblock: {:?}",
            added.map_err(|_| "a panic")
        );
        assert_eq!(
            h.w.objects.get(empty).unwrap().time_to_rot(),
            Some(15.0),
            "nothing to shorten"
        );

        let full = corpse(&mut h.w, 0x8000_0201, 200, 3600.0);
        assert!(lm::add_object(&mut h.w, full, false));
        assert_eq!(
            h.w.objects.get(full).unwrap().time_to_rot(),
            Some(15.0),
            "the corpse with time left is shortened"
        );
    }
}

mod luminance {
    use crate::support::social_world::*;

    /// `Player_Luminance.cs`: kill luminance is modified (1.0 by default) and capped at the maximum,
    /// quest luminance announces the amount earned, a full pool sends nothing, and SpendLuminance
    /// refuses more than is available.
    #[test]
    fn luminance_is_earned_up_to_the_maximum_and_spent() {
        let mut h = H::small();
        let sa = h.player(A, "Alpha", 3);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_property(PropertyInt64::MaximumLuminance, 1000);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_property(PropertyInt64::AvailableLuminance, 900);

        start_capture();
        player_luminance::earn_luminance(&mut h.w, A, 60, XpType::Kill, ShareType::All);
        assert_eq!(available_luminance(&h), Some(960));
        let msgs = sent();
        assert_eq!(
            opcodes_to(&msgs, sa),
            [GameMessageOpcode::PrivateUpdatePropertyInt64.0]
        );
        assert_eq!(
            i64::from_le_bytes(msgs[0].3[9..17].try_into().unwrap()),
            960
        );

        // capped: Math.Min(amount, remaining), but the quest message names the full amount
        start_capture();
        player_luminance::earn_luminance(&mut h.w, A, 100, XpType::Quest, ShareType::All);
        assert_eq!(available_luminance(&h), Some(1000));
        let msgs = sent();
        assert_eq!(chats_to(&msgs, sa), ["You've earned 100 Luminance."]);

        // available == maximum: nothing
        start_capture();
        player_luminance::grant_luminance(&mut h.w, A, 5, XpType::Kill, ShareType::All);
        assert!(sent().is_empty());

        assert!(!player_luminance::spend_luminance(&mut h.w, A, 1001));
        assert_eq!(available_luminance(&h), Some(1000));
        assert!(player_luminance::spend_luminance(&mut h.w, A, 400));
        assert_eq!(available_luminance(&h), Some(600));
    }

    /// Divergence: V425
    /// A world without luminance awards none, by kill or quest, and spends none, so a luminance
    /// augmentation's purchase fails as an unaffordable one does.
    #[test]
    fn a_world_without_luminance_awards_and_spends_none() {
        use empyrean_common::era::{with_features, EraExt as _, EraFeatures, EraId};
        let mut h = H::small();
        let _sa = h.player(A, "Alpha", 3);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_property(PropertyInt64::MaximumLuminance, 1000);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_property(PropertyInt64::AvailableLuminance, 500);
        let eor = EraId::Eor.rules();
        h.w.era = with_features(
            eor,
            EraFeatures {
                luminance: false,
                ..eor.features
            },
        );
        start_capture();
        player_luminance::earn_luminance(&mut h.w, A, 60, XpType::Kill, ShareType::All);
        player_luminance::earn_luminance(&mut h.w, A, 60, XpType::Quest, ShareType::All);
        player_luminance::grant_luminance(&mut h.w, A, 60, XpType::Kill, ShareType::All);
        assert!(sent().is_empty());
        assert!(!player_luminance::spend_luminance(&mut h.w, A, 100));
        assert_eq!(available_luminance(&h), Some(500));

        h.w.era = eor;
        assert!(player_luminance::spend_luminance(&mut h.w, A, 100));
        assert_eq!(available_luminance(&h), Some(400));
    }
}
