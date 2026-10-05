//! ACE: Source/ACE.Server/WorldObjects/Monster_Awareness.cs::CheckTargets
//! Through the world loop a monster beside a player is alerted, takes its stance, runs to melee
//! range and swings (hit or evade).
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::geometry::AnimFrame;
use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::tables::CombatManeuver;
use dereth_assets::{AnimHook, Animation, CombatManeuverTable, HookData, MotionTable};
use dereth_primitives::{DataId, Vec3};
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::not_ported::take_local;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AttackType, CombatBodyPart, CombatMode, DamageType, MotionCommand as Mc, MotionStance,
    PhysicsState, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyInt64, PropertyString, Skill, SkillAdvancementClass,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionState;
use empyrean_testkit::{dats, land, ClientId, TestServer};
use empyrean_world::dispatch::Class;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    monster, monster_awareness, monster_combat, monster_navigation, world_object_tick,
};
use empyrean_world::World;

pub(crate) struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

pub(crate) const LB: u32 = 0xA9B4_0000;
pub(crate) const MT: u32 = 0x0900_0310;
pub(crate) const CMT: u32 = 0x3000_0310;
const CYCLE: u32 = 0x0300_0210;
const STANCE: u32 = 0x0300_0211;
const ATTACK: u32 = 0x0300_0212;
pub(crate) const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
pub(crate) const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);
const NC: u32 = MotionStance::NonCombat.0;
const HC: u32 = MotionStance::HandCombat.0;

fn anim(id: u32) -> AnimData {
    AnimData {
        anim_id: DataId(id),
        low_frame: 0,
        high_frame: -1,
        framerate: 30.0,
    }
}

fn data(key: u32, anims: Vec<AnimData>, velocity: Option<Vec3>, omega: Option<Vec3>) -> MotionData {
    MotionData {
        key,
        bitfield: 0,
        flags: 0,
        anims,
        velocity,
        omega,
    }
}

fn key(style: u32, motion: u32) -> u32 {
    style.wrapping_shl(16) | (motion & 0xFF_FFFF)
}

fn animation(id: u32, n: u32, hook_frame: Option<u32>) -> Animation {
    let attack = AnimHook {
        hook_type: 3,
        direction: 1,
        data: HookData::Attack(dereth_primitives::records::AttackCone {
            part_index: 0,
            left: (0.0, 0.0),
            right: (0.0, 0.0),
            radius: 1.0,
            height: 1.0,
        }),
    };
    let part_frames = (0..n)
        .map(|i| AnimFrame {
            frames: Vec::new(),
            hooks: if Some(i) == hook_frame {
                vec![attack.clone()]
            } else {
                Vec::new()
            },
        })
        .collect();
    Animation {
        id: DataId(id),
        flags: 0,
        num_parts: 0,
        num_frames: n,
        has_hooks: hook_frame.is_some(),
        pos_frames: None,
        part_frames,
    }
}

pub(crate) fn world_dats() -> Arc<empyrean_dat::DatManager> {
    let mut cycles = Vec::new();
    let mut modifiers = Vec::new();
    let mut style_defaults = BTreeMap::new();
    for style in [NC, HC] {
        style_defaults.insert(style, Mc::Ready.0);
        cycles.push(data(key(style, Mc::Ready.0), vec![anim(CYCLE)], None, None));
        cycles.push(data(
            key(style, Mc::RunForward.0),
            vec![anim(CYCLE)],
            Some(Vec3::new(0.0, 4.0, 0.0)),
            None,
        ));
        let omega = Some(Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2));
        cycles.push(MotionData {
            bitfield: 2,
            ..data(key(style, Mc::TurnRight.0), vec![anim(CYCLE)], None, omega)
        });
        modifiers.push(data(key(style, Mc::TurnRight.0), Vec::new(), None, omega));
    }
    let mut links = BTreeMap::new();
    links.insert(
        key(NC, Mc::Ready.0),
        vec![data(HC, vec![anim(STANCE)], None, None)],
    );
    links.insert(
        key(HC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(STANCE)], None, None),
            data(Mc::AttackHigh1.0, vec![anim(ATTACK)], None, None),
        ],
    );
    let table = MotionTable {
        id: DataId(MT),
        default_style: NC,
        style_defaults,
        cycles,
        modifiers,
        links,
    };
    let maneuver = |height: u32, t: AttackType| CombatManeuver {
        style: HC,
        attack_height: height,
        attack_type: t.0.cast_unsigned(),
        min_skill_level: 0,
        motion: Mc::AttackHigh1.0,
    };
    let cmt = CombatManeuverTable {
        id: DataId(CMT),
        maneuvers: vec![
            maneuver(1, AttackType::Punch),
            maneuver(2, AttackType::Punch),
            maneuver(3, AttackType::Kick),
        ],
    };
    dats::with_stat_tables(FakeDats::new())
        // a kill grants XP (Creature.OnDeath_GrantXP reads the level table)
        .with_xp_table(empyrean_dat::fake::sample::xp_table())
        .with_portal(MT, table)
        .with_portal(CMT, cmt)
        .with_portal(CYCLE, animation(CYCLE, 10, None))
        .with_portal(STANCE, animation(STANCE, 15, None))
        .with_portal(ATTACK, animation(ATTACK, 30, Some(10)))
        .build()
        .expect("fake dats")
}

/// A creature as its constructor leaves it (vitals, attributes, Run, the motion state, the
/// damage history, `SetMonsterState`), home where it stands. Not placed.
pub(crate) fn creature(w: &mut World, class: Class, guid: ObjectGuid, x: f32, y: f32) {
    let mut o = WorldObject::allocate(class);
    o.guid = guid;
    o.biota.id = guid.full();
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let vitals = [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ];
    let mut v2 = DotNetDict::new();
    for v in vitals {
        v2.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 100,
            },
        );
    }
    o.biota.properties_attribute_2nd = Some(v2);
    for v in vitals {
        let cv = CreatureVital::new(&mut o, v);
        o.vitals_mut().insert(v, cv);
    }
    let attributes = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ];
    let mut a1 = DotNetDict::new();
    for a in attributes {
        a1.insert(
            a,
            PropertiesAttribute {
                init_level: 60,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
    }
    o.biota.properties_attribute = Some(a1);
    for a in attributes {
        let ca = CreatureAttribute::new(&mut o, a);
        o.attributes_mut().insert(a, ca);
    }
    let mut skills = DotNetDict::new();
    skills.insert(
        Skill::Run,
        PropertiesSkill {
            init_level: 100,
            sac: SkillAdvancementClass::Trained,
            ..PropertiesSkill::default()
        },
    );
    o.biota.properties_skill = Some(skills);
    o.set_property(PropertyString::Name, format!("{guid}"));
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_property(PropertyDataId::MotionTable, MT);
    o.set_property(PropertyBool::Attackable, true);
    o.creature.as_mut().unwrap().creature_death.damage_history =
        DamageHistory::new(guid, w.now.utc);
    let pos = Position::from_components(LB | 0x0001, x, y, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    o.set_location(Some(pos));
    o.set_position(PositionType::Home, Some(pos));
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::NonCombat, Mc::Ready, 1.0));
    o.set_heartbeat_interval(Some(0.0));
    world_object_tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    monster::set_monster_state(&mut o);
    w.objects.insert(o).expect("fresh guid");
}

/// A player (logged in on a client, entered as `DoPlayerEnterWorld` has it) 12 m from a drudge-like
/// monster whose one body part hits for 10 (variance 0.5, bludgeon); `CheckTargets` has been
/// called on the monster.
pub(crate) fn arena() -> (TestServer, ClientId) {
    let mut ts = TestServer::with_setup(world_dats(), |w| {
        gm::initialize(w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(w, &[0xA9B4], 10);
    });
    // the player enters as `DoPlayerEnterWorld` has it: a world-connected session whose player
    // is set before it joins its landblock (tracking what it sees sends through that session)
    let client = ts.connect("acct", "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .next()
        .expect("the session");
    let w = &mut ts.world;
    lm::get_landblock(w, LandblockId::new(LB | 0xFFFF), false, false);
    creature(w, Class::Player, PLAYER, 100.0, 112.0);
    let p = w.objects.get_mut(PLAYER).unwrap();
    p.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    // V221.
    p.set_level(Some(1));
    p.set_property(PropertyInt64::TotalExperience, 0);
    p.set_property(PropertyInt64::AvailableExperience, 0);
    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(PLAYER));
    assert!(
        lm::add_object(w, PLAYER, false),
        "the player joins its landblock"
    );
    phys_ext::set_physics_state(w, PLAYER, PhysicsState::Hidden, Some(false));
    creature(w, Class::Creature, MONSTER, 100.0, 100.0);
    let o = w.objects.get_mut(MONSTER).unwrap();
    o.set_property(PropertyDataId::CombatTable, CMT);
    o.set_property(PropertyString::Name, "Drudge".to_owned());
    let mut parts = DotNetDict::new();
    let head = PropertiesBodyPart {
        d_type: DamageType::Bludgeon,
        d_val: 10,
        d_var: 0.5,
        ..PropertiesBodyPart::default()
    };
    parts.insert(CombatBodyPart::Head, head);
    o.biota.properties_body_part = Some(Arc::new(parts));
    assert!(
        lm::add_object(w, MONSTER, false),
        "the monster joins its landblock"
    );
    monster_awareness::check_targets(w, MONSTER);
    (ts, client)
}

#[test]
fn through_the_world_loop_a_monster_wakes_closes_in_and_swings() {
    let (mut ts, client) = arena();
    let _ = take_local();

    assert!(
        ts.run_until(1.0, |ts| monster_awareness::is_awake(&ts.world, MONSTER)),
        "CheckTargets alerts it"
    );
    assert_eq!(
        monster_combat::attack_target(&ts.world, MONSTER),
        Some(PLAYER)
    );

    assert!(
        ts.run_until(
            2.0,
            |ts| empyrean_world::world_objects::creature_combat::combat_mode(&ts.world, MONSTER)
                == CombatMode::Melee
        ),
        "attack stance"
    );

    let far = monster_navigation::get_distance_to_target(&ts.world, MONSTER);
    assert!(
        ts.run_until(10.0, |ts| monster_navigation::get_distance_to_target(
            &ts.world, MONSTER
        ) <= monster_navigation::MAX_MELEE_RANGE),
        "closed to melee range from {far}"
    );
    assert_eq!(
        monster::monster_state(&ts.world, MONSTER),
        monster::State::Awake
    );

    let _ = take_local();
    let mut evades = 0;
    assert!(
        ts.run_until(5.0, |ts| {
            evades += take_local()
                .get("ACE: Player.OnEvade")
                .copied()
                .unwrap_or(0);
            evades + defender_notifications(ts, client) >= 2
        }),
        "two strikes run DamageEvent.CalculateDamage: a hit (DefenderNotification) or an evade"
    );
}

/// The `DefenderNotification` game events the client has received so far.
pub(crate) fn defender_notifications(ts: &TestServer, client: ClientId) -> u64 {
    let n = ts
        .received_raw(client)
        .iter()
        .filter(|m| {
            m.opcode == 0xF7B0
                && u32::from_le_bytes(m.body[8..12].try_into().expect("an event type")) == 0x01B2
        })
        .count();
    u64::try_from(n).expect("a count")
}

#[cfg(feature = "real-content")]
mod arrival_real {
    //! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
    use crate::support::real_content_bot::real::*;

    /// A real Holtburg drudge notices a player who arrives within its awareness range (10 m): the
    /// drudge skulker the landblock's generator spawns at `0xAAB40040` (187.1, 183.7), and the
    /// character teleported (`@teleloc`, as an admin) to 4 m west of it. `Player.OnTeleportComplete`
    /// and every move run `Player.CheckMonsters`, which alerts it.
    #[test]
    fn a_holtburg_drudge_notices_a_player_who_arrives() {
        let mut l = create_and_enter();
        let mark = l.mark();
        l.admin_command("@teleloc 0xAAB40040 183.0 183.69 27.6");
        let id = l.id;
        assert!(
            l.ts.run_until(5.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "teleported"
        );
        l.advance(1.0);
        l.action(&CharacterLoginCompleteNotification);
        l.advance(1.0);
        assert_eq!(l.location().cell(), 0xAAB4_0040);
        let drudges = l.nearby_wcid(DRUDGE_SKULKER);
        assert_eq!(drudges.len(), 1, "the generator's drudge skulker");
        let drudge = drudges[0];
        assert!(
            l.ts.world.objects.get(drudge).is_some_and(|o| o
                .wo
                .world_object_generators
                .generator
                .is_some()),
            "spawned by a generator"
        );
        l.advance(10.0);
        assert!(
            empyrean_world::world_objects::monster_awareness::is_awake(&l.ts.world, drudge),
            "the drudge is alerted"
        );
        assert_eq!(
            monster_combat::attack_target(&l.ts.world, drudge),
            Some(l.g),
            "and targets the character"
        );
    }
}

mod strike_messages {
    //! ACE: Source/ACE.Server/WorldObjects/Monster_Melee.cs::MeleeAttack
    use dereth_protocol::combat::DefenderNotification;
    use dereth_protocol::events::split_ui_blob;
    use dereth_protocol::qualities::QualitiesPrivateUpdateAttribute2ndLevel;
    use dereth_protocol::Message;
    use empyrean_testkit::{ClientId, TestServer};

    use crate::combat::monster_ai::{arena, defender_notifications, MONSTER, PLAYER};

    /// `Vital.Health` in `QualitiesPrivateUpdateAttribute2ndLevel`.
    const HEALTH: u32 = 2;

    /// The `DefenderNotification`s the client has received, decoded through their `0xF7B0` wrapper.
    fn defender_events(ts: &TestServer, client: ClientId) -> Vec<DefenderNotification> {
        ts.received_raw(client)
            .iter()
            .filter(|m| m.opcode == 0xF7B0)
            .filter_map(|m| {
                let mut blob = m.opcode.to_le_bytes().to_vec();
                blob.extend_from_slice(&m.body);
                let split = split_ui_blob(&blob).expect("a game event");
                (split.sub_type.0 == DefenderNotification::OPCODE.0).then(|| {
                    let mut body = split.body;
                    DefenderNotification::read(&mut body)
                        .expect("dereth-protocol decodes the DefenderNotification")
                })
            })
            .collect()
    }

    #[test]
    fn a_monster_strike_lands_through_the_damage_event_and_the_player_is_told() {
        let (mut ts, client) = arena();
        let _ = TestServer::take_not_ported();
        let max_health = {
            let p = ts.world.objects.get(PLAYER).expect("the player");
            p.health().current(p)
        };

        // the monster wakes, closes in and swings until a strike lands (the evade chance is small:
        // the player has no Melee Defense)
        assert!(
            ts.run_until(20.0, |ts| defender_notifications(ts, client) >= 1),
            "a strike lands"
        );
        let not_ported = TestServer::take_not_ported();
        assert_eq!(
            not_ported.get("ACE: DamageEvent.CalculateDamage"),
            None,
            "the real damage event"
        );

        let hits = defender_events(&ts, client);
        let first = &hits[0];
        assert_eq!(first.attacker_name, "Drudge");
        assert_eq!(first.damage_type, 4, "the body part's Bludgeon");
        assert!(first.damage > 0, "{first:?}");

        // the health update the client received for that hit: max - damage
        let updates: Vec<(u32, u32)> = ts
            .received::<QualitiesPrivateUpdateAttribute2ndLevel>(client)
            .into_iter()
            .map(|u| (u.0.property_id, u.0.value))
            .filter(|&(id, _)| id == HEALTH)
            .collect();
        assert_eq!(
            updates.first().copied(),
            Some((HEALTH, max_health - first.damage)),
            "Vital.Health, the new current"
        );

        // and the world agrees, summed over every hit so far
        let total: u32 = hits.iter().map(|d| d.damage).sum();
        let p = ts.world.objects.get(PLAYER).expect("the player");
        assert_eq!(
            p.health().current(p),
            max_health - total,
            "the player's health dropped by the damage"
        );

        // the attacker is on record: the damage history and the player's last attacker
        let history = empyrean_world::entity::damage_history::of(&ts.world, PLAYER);
        assert_eq!(history.last_damager().map(|d| d.guid), Some(MONSTER));
    }
}

mod kill_tasks {
    //! ACE: Source/ACE.Server/WorldObjects/Creature_Death.cs::OnDeath_HandleKillTask
    use crate::inventory::player_inventory::*;

    /// A spawned drudge notices is killed counts for the kill task and its corpse is looted.
    #[test]
    fn a_spawned_drudge_notices_is_killed_counts_for_the_kill_task_and_its_corpse_is_looted() {
        use dereth_protocol::objects::{ItemCreateObject, ItemOnViewContents};
        use empyrean_entity::enums::{DamageType, PhysicsState, PropertyBool};
        use empyrean_world::managers::quest_manager::{self as qm, QuestOwner};
        use empyrean_world::world_objects::monster_awareness;

        let mut ts = TestServer::with_setup(crate::combat::monster_ai::world_dats(), |w| {
            w.content = Arc::new(hunting_content());
            guid_manager::initialize(w, &mut EmptyShard);
        });
        land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
        let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 26.0), 100);
        let player = ObjectGuid::new(ALPHA);
        {
            let w = &mut ts.world;
            let o = w.objects.get_mut(player).unwrap();
            o.set_property(PropertyBool::Attackable, true);
            // a player's level and XP (the kill grants XP: CheckForLevelup loops on a null Level, an
            // ACE-BUG a real player never meets)
            o.set_level(Some(1));
            o.set_property(empyrean_entity::enums::PropertyInt64::TotalExperience, 0);
            o.set_property(
                empyrean_entity::enums::PropertyInt64::AvailableExperience,
                0,
            );
            phys_ext::set_physics_state(w, player, PhysicsState::Hidden, Some(false));
            // an NPC gave the player the kill task, at 0 kills
            qm::set_quest_completions(w, &mut QuestOwner::Creature(player), KILL_TASK, 0);
        }

        // the generator spawns the drudge at its own spot, 6 m from the player
        let generator = drop_on_ground(&mut ts.world, DRUDGE_GENERATOR, at(20.0, 20.0));
        assert!(
            ts.run_until(2.0, |ts| find_by_wcid(&ts.world, DRUDGE).is_some()),
            "the generator spawns a drudge"
        );
        let drudge = find_by_wcid(&ts.world, DRUDGE).unwrap();
        assert_eq!(
            obj(&ts, drudge).wo.world_object_generators.generator,
            Some(generator)
        );
        assert!(!monster_awareness::is_awake(&ts.world, drudge));

        // nobody calls CheckTargets: the spawn's NotifyPlayers did, and 0.75 s later it is alerted
        assert!(
            ts.run_until(1.5, |ts| monster_awareness::is_awake(&ts.world, drudge)),
            "the drudge notices the player"
        );
        assert_eq!(
            empyrean_world::world_objects::monster_combat::attack_target(&ts.world, drudge),
            Some(player)
        );

        // the player kills it (Creature.TakeDamage: OnDeath, then Die)
        let before_kill = ts.received_raw(id).len();
        let damage = empyrean_world::dispatch::take_damage::take_damage(
            &mut ts.world,
            drudge,
            player,
            DamageType::Slash,
            200.0,
            false,
        );
        assert!(damage > 0);
        ts.advance(0.5);
        assert!(
            ts.world.objects.get(drudge).is_none(),
            "destroyed after the death animation"
        );

        // the kill counted toward the task, and the player was told
        let registry =
            qm::get_quest(&ts.world, &QuestOwner::Creature(player), KILL_TASK).expect("the task");
        assert_eq!(registry.num_times_completed, 1);
        let chat: Vec<String> = got(&ts, id, before_kill)
            .iter()
            .filter(|m| m.kind == CHAT)
            .map(|m| m.decode::<CommunicationTextboxString>().text)
            .collect();
        assert!(
            chat.contains(
                &"You have killed 1 Drudges! You must kill 10 to complete your task.".to_owned()
            ),
            "{chat:?}"
        );

        // the corpse holds the gems in its Container.Inventory
        let corpse = find_by_wcid(&ts.world, CORPSE).expect("a corpse");
        let loot = container::inventory_values(&ts.world, corpse);
        assert_eq!(loot.len(), 2);
        let (gem, other_gem) = (loot[0], loot[1]);
        assert_eq!(obj(&ts, other_gem).biota.weenie_class_id, GEM);
        assert_eq!(obj(&ts, gem).biota.weenie_class_id, GEM);
        assert_eq!(obj(&ts, gem).container_id(), Some(corpse.full()));
        assert_eq!(obj(&ts, corpse).killer_id(), Some(ALPHA));

        // the player walks over and opens it (Player.HandleActionUseItem is not ported: its
        // ActOnUse runs directly): Container.Open sends the gem's CreateObject, then ViewContents
        let corpse_at = obj(&ts, corpse).location().expect("on the ground");
        walk_to(
            &mut ts,
            ALPHA,
            at(corpse_at.position_x, corpse_at.position_y + 0.5),
        );
        let n = ts.received_raw(id).len();
        empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, corpse, player);
        ts.advance(0.2);
        let opened = got(&ts, id, n);
        let created: Vec<u32> = opened
            .iter()
            .filter(|m| m.kind == CREATE_OBJECT)
            .map(|m| m.decode::<ItemCreateObject>().0.id.0)
            .collect();
        assert_eq!(created, [gem.full(), other_gem.full()]);
        let view: ItemOnViewContents = first(&opened, VIEW_CONTENTS).decode();
        assert_eq!(view.container.0, corpse.full());
        let contents: Vec<(u32, u32)> = view
            .contents
            .iter()
            .map(|c| (c.iid.0, c.container_properties))
            .collect();
        assert_eq!(contents, [(other_gem.full(), 0), (gem.full(), 0)]);

        // and takes the gem (HandleActionPutItemInContainer, corpse to player)
        let (sent, g) = exchange(&mut ts, id, session, 1.0, |ts| {
            ts.send_game_action(
                id,
                &InventoryPutItemInContainer {
                    item: ObjectId(gem.full()),
                    container: ObjectId(ALPHA),
                    slot: 0,
                },
            );
        });
        assert!(sent.contains(&CONTAIN_ID), "{sent:04X?}");
        let contain: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
        assert_eq!((contain.item.0, contain.container.0), (gem.full(), ALPHA));
        assert_eq!(container::inventory_values(&ts.world, player), vec![gem]);
        assert_eq!(
            container::inventory_values(&ts.world, corpse),
            vec![other_gem],
            "one gem left"
        );
        assert_eq!(obj(&ts, gem).container_id(), Some(ALPHA));

        empyrean_world::world_objects::world_object::destroy(&mut ts.world, corpse, true, false);
        ts.advance(0.2);
        assert!(
            ts.world.objects.get(corpse).is_none(),
            "the corpse is destroyed"
        );
        assert!(
            ts.world.objects.get(other_gem).is_none(),
            "and the gem inside it"
        );
        assert_eq!(container::inventory_values(&ts.world, player), vec![gem]);
    }
}
