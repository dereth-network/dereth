//! ACE: Source/ACE.Server/WorldObjects/Monster_Awareness.cs::CheckTargets
//! Monster awareness, navigation, melee/missile/magic combat and tick follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::geometry::AnimFrame;
use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::tables::{CombatManeuver, SpellBase};
use dereth_assets::{AnimHook, Animation, CombatManeuverTable, HookData, MotionTable, SpellTable};
use dereth_primitives::{DataId, Vec3};
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{DotNetDict, Vector3};
use empyrean_common::not_ported::take_local;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AttackType, CombatMode, EquipMask, MotionCommand as Mc, MotionStance, PhysicsState,
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyString, Skill, SkillAdvancementClass, TargetingTactic,
    WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::entity::timers::{self, TimersState};
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::creature_combat::{self, CombatType};
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::monster::{self, State};
use empyrean_world::world_objects::monster_awareness::{self as awareness, TargetDistance};
use empyrean_world::world_objects::monster_combat as combat;
use empyrean_world::world_objects::monster_navigation as nav;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    creature_equipment, monster_melee, monster_missile, monster_tick,
};
use empyrean_world::World;

// ------------------------------------------------------------------------------------ fixtures

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
const SEED: i32 = 4747;
pub(crate) const MT: u32 = 0x0900_0300;
const CMT: u32 = 0x3000_0300;
pub(crate) const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
pub(crate) const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);
const SESSION: empyrean_net::SessionId = empyrean_net::SessionId {
    client_id: 1,
    generation: 1,
};
const BOW: ObjectGuid = ObjectGuid::new(0x8000_0101);
const ARROWS: ObjectGuid = ObjectGuid::new(0x8000_0102);

const NC: u32 = MotionStance::NonCombat.0;
const HC: u32 = MotionStance::HandCombat.0;
const BOWC: u32 = MotionStance::BowCombat.0;

/// Every cycle's animation: 10 frames at 30/s, no root motion.
const CYCLE_ANIM: u32 = 0x0300_0200;
/// A stance change: 15 frames, 0.5 s.
const STANCE_ANIM: u32 = 0x0300_0201;
/// An attack: 30 frames (1 s) with the attack hook on frame 10 (a third of the way in).
const ATTACK_ANIM: u32 = 0x0300_0202;
/// Aim and reload: 15 frames each, 0.5 s.
const AIM_ANIM: u32 = 0x0300_0203;

const RUN_VELOCITY: f32 = 4.0;
/// `GetAnimSpeed()` with Quickness 60 and no weapon: `GetWeaponSpeed` answers ACE's
/// `defaultSpeed` (40) for a non-player, so `1 / (1 - 60/300 + 40/150)`.
const ANIM_SPEED: f32 = 0.9375;
/// The attack's length at that speed: 1 s / 0.9375.
const ANIM_LENGTH: f32 = 1.0 / ANIM_SPEED;
const TURN_OMEGA: f32 = -std::f32::consts::FRAC_PI_2;

fn anim(anim_id: u32) -> AnimData {
    AnimData {
        anim_id: DataId(anim_id),
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

fn animation(id: u32, n: u32, hook_frame: Option<u32>) -> Animation {
    let part_frames = (0..n)
        .map(|i| AnimFrame {
            frames: Vec::new(),
            hooks: if Some(i) == hook_frame {
                vec![AnimHook {
                    hook_type: 3,
                    direction: 1,
                    data: HookData::Attack(dereth_primitives::records::AttackCone {
                        part_index: 0,
                        left: (0.0, 0.0),
                        right: (0.0, 0.0),
                        radius: 1.0,
                        height: 1.0,
                    }),
                }]
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

fn key(style: u32, motion: u32) -> u32 {
    style.wrapping_shl(16) | (motion & 0xFF_FFFF)
}

fn motion_table() -> MotionTable {
    let mut cycles = Vec::new();
    let mut modifiers = Vec::new();
    let mut style_defaults = BTreeMap::new();
    for style in [NC, HC, BOWC] {
        style_defaults.insert(style, Mc::Ready.0);
        cycles.push(data(
            key(style, Mc::Ready.0),
            vec![anim(CYCLE_ANIM)],
            None,
            None,
        ));
        cycles.push(data(
            key(style, Mc::WalkForward.0),
            vec![anim(CYCLE_ANIM)],
            Some(Vec3::new(0.0, 3.12, 0.0)),
            None,
        ));
        cycles.push(data(
            key(style, Mc::RunForward.0),
            vec![anim(CYCLE_ANIM)],
            Some(Vec3::new(0.0, RUN_VELOCITY, 0.0)),
            None,
        ));
        cycles.push(MotionData {
            bitfield: 2,
            ..data(
                key(style, Mc::TurnRight.0),
                vec![anim(CYCLE_ANIM)],
                None,
                Some(Vec3::new(0.0, 0.0, TURN_OMEGA)),
            )
        });
        modifiers.push(data(
            key(style, Mc::TurnRight.0),
            Vec::new(),
            None,
            Some(Vec3::new(0.0, 0.0, TURN_OMEGA)),
        ));
    }
    // The aims and the reload are substates: the client plays one only when it has a cycle (V352).
    for m in [
        Mc::AimLevel,
        Mc::AimHigh15,
        Mc::AimHigh30,
        Mc::AimHigh45,
        Mc::AimLow15,
        Mc::AimLow30,
        Mc::Reload,
    ] {
        cycles.push(data(key(BOWC, m.0), vec![anim(CYCLE_ANIM)], None, None));
    }
    let mut links = BTreeMap::new();
    links.insert(
        key(NC, Mc::Ready.0),
        vec![
            data(HC, vec![anim(STANCE_ANIM)], None, None),
            data(BOWC, vec![anim(STANCE_ANIM)], None, None),
        ],
    );
    links.insert(
        key(HC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(STANCE_ANIM)], None, None),
            data(Mc::AttackHigh1.0, vec![anim(ATTACK_ANIM)], None, None),
            data(Mc::AttackMed1.0, vec![anim(ATTACK_ANIM)], None, None),
            data(Mc::AttackLow1.0, vec![anim(ATTACK_ANIM)], None, None),
        ],
    );
    links.insert(
        key(BOWC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(STANCE_ANIM)], None, None),
            data(Mc::AimLevel.0, vec![anim(AIM_ANIM)], None, None),
            // every elevation step (`Creature.GetAimLevel` picks one from the launch velocity)
            data(Mc::AimHigh15.0, vec![anim(AIM_ANIM)], None, None),
            data(Mc::AimHigh30.0, vec![anim(AIM_ANIM)], None, None),
            data(Mc::AimHigh45.0, vec![anim(AIM_ANIM)], None, None),
            data(Mc::AimLow15.0, vec![anim(AIM_ANIM)], None, None),
            data(Mc::AimLow30.0, vec![anim(AIM_ANIM)], None, None),
            data(Mc::Reload.0, vec![anim(AIM_ANIM)], None, None),
        ],
    );
    MotionTable {
        id: DataId(MT),
        default_style: NC,
        style_defaults,
        cycles,
        modifiers,
        links,
    }
}

/// HandCombat: a high and a medium punch and a low kick; BowCombat: the three heights too (the
/// missile attack rolls one per launch).
fn combat_table() -> CombatManeuverTable {
    let m = |style: u32, height: u32, t: AttackType, motion: Mc| CombatManeuver {
        style,
        attack_height: height,
        attack_type: t.0.cast_unsigned(),
        min_skill_level: 0,
        motion: motion.0,
    };
    CombatManeuverTable {
        id: DataId(CMT),
        maneuvers: vec![
            m(HC, 1, AttackType::Punch, Mc::AttackHigh1),
            m(HC, 2, AttackType::Punch, Mc::AttackMed1),
            m(HC, 3, AttackType::Kick, Mc::AttackLow1),
            m(BOWC, 1, AttackType::Punch, Mc::AttackHigh1),
            m(BOWC, 2, AttackType::Punch, Mc::AttackMed1),
            m(BOWC, 3, AttackType::Kick, Mc::AttackLow1),
        ],
    }
}

const VECTOR_SPELLS: [u32; 4] = [64001, 64002, 64003, 64004];
/// A War Magic spell of range 10 + 0.05 x skill (V336's casters).
const CLOSE_SPELL: u32 = 64010;
/// A War Magic spell of range 90, beyond ACE's cap of 75 (V336).
const FAR_SPELL: u32 = 64011;

fn spell_base(id: u32) -> SpellBase {
    SpellBase {
        name: String::new(),
        description: String::new(),
        school: 1,
        icon: 0,
        category: 0,
        bitfield: 0,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 0,
        meta_spell_id: id,
        duration: None,
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
    }
}

/// The arrows' weenie and dat setup (`Creature.LaunchProjectile` creates a projectile of the ammo's
/// weenie; `GetProjectileRadius` reads its setup's first sphere).
const ARROW_WCID: u32 = 300;
const ARROW_SETUP: u32 = 0x0200_1300;

fn arrow_setup() -> dereth_assets::Setup {
    let sphere = |r: f32| dereth_assets::Sphere {
        center: Vec3::new(0.0, 0.0, r),
        radius: r,
    };
    dereth_assets::Setup {
        id: DataId(ARROW_SETUP),
        flags: 0,
        parts: Vec::new(),
        parent_index: None,
        default_scale: None,
        allow_free_heading: false,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::new(),
        cylspheres: Vec::new(),
        spheres: vec![sphere(0.05)],
        height: 0.1,
        radius: 0.05,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere(0.05),
        selection_sphere: sphere(0.05),
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
    }
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    let spells = SpellTable {
        id: DataId(file_id::SPELL_TABLE),
        spell_buckets: 64,
        spells: VECTOR_SPELLS
            .iter()
            .map(|&id| (id, spell_base(id)))
            .chain([
                (
                    CLOSE_SPELL,
                    SpellBase {
                        base_range_constant: 10.0,
                        base_range_mod: 0.05,
                        school: 1,
                        ..spell_base(CLOSE_SPELL)
                    },
                ),
                (
                    FAR_SPELL,
                    SpellBase {
                        base_range_constant: 90.0,
                        school: 1,
                        ..spell_base(FAR_SPELL)
                    },
                ),
            ])
            .collect(),
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_spell_table(spells)
        .with_portal(MT, motion_table())
        .with_portal(CMT, combat_table())
        .with_portal(CYCLE_ANIM, animation(CYCLE_ANIM, 10, None))
        .with_portal(STANCE_ANIM, animation(STANCE_ANIM, 15, None))
        .with_portal(ATTACK_ANIM, animation(ATTACK_ANIM, 30, Some(10)))
        .with_portal(AIM_ANIM, animation(AIM_ANIM, 15, None))
        .with_portal(ARROW_SETUP, arrow_setup())
        .build()
        .expect("fake dats")
}

pub(crate) struct H {
    pub(crate) w: World,
    clock: VirtualClock,
}

impl H {
    pub(crate) fn new() -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let mut w = World::new(now, dats());
        w.timers = timers;
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 10);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(u64::from(SEED.unsigned_abs()));
        H { w, clock }
    }

    /// One world pass: the delay manager, then `LandblockManager.Tick`.
    pub(crate) fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    pub(crate) fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
    }

    /// Ticks every 50 ms for `secs` seconds, stopping early when `until` holds.
    pub(crate) fn run_until(&mut self, secs: f64, mut until: impl FnMut(&Self) -> bool) -> bool {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few hundred steps
        let steps = (secs / 0.05).round() as u32;
        for _ in 0..steps {
            self.advance(0.05);
            self.tick();
            if until(self) {
                return true;
            }
        }
        false
    }

    pub(crate) fn run(&mut self, secs: f64) {
        self.run_until(secs, |_| false);
    }

    pub(crate) fn o(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("object in store")
    }

    pub(crate) fn running_time(&self) -> f64 {
        timers::running_time(&self.w)
    }

    /// A creature built as the constructor would (vitals, attributes, Run, the damage history,
    /// the NonCombat motion state, `SetMonsterState`), at `x, y` in the landblock's first cell,
    /// home there too. Not placed.
    pub(crate) fn creature(&mut self, class: Class, guid: ObjectGuid, name: &str, x: f32, y: f32) {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.weenie_type = WeenieType::Creature;
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let mut vitals = DotNetDict::new();
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            vitals.insert(
                v,
                PropertiesAttribute2nd {
                    init_level: 100,
                    level_from_cp: 0,
                    cp_spent: 0,
                    current_level: 100,
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
        let mut attributes = DotNetDict::new();
        for a in [
            PropertyAttribute::Strength,
            PropertyAttribute::Endurance,
            PropertyAttribute::Coordination,
            PropertyAttribute::Quickness,
            PropertyAttribute::Focus,
            PropertyAttribute::Self_,
        ] {
            attributes.insert(
                a,
                PropertiesAttribute {
                    init_level: 60,
                    level_from_cp: 0,
                    cp_spent: 0,
                },
            );
        }
        o.biota.properties_attribute = Some(attributes);
        for a in [
            PropertyAttribute::Strength,
            PropertyAttribute::Endurance,
            PropertyAttribute::Coordination,
            PropertyAttribute::Quickness,
            PropertyAttribute::Focus,
            PropertyAttribute::Self_,
        ] {
            let ca =
                empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute::new(
                    &mut o, a,
                );
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
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
        o.set_property(PropertyDataId::MotionTable, MT);
        o.set_property(PropertyBool::Attackable, true);
        o.creature.as_mut().unwrap().creature_death.damage_history =
            DamageHistory::new(guid, self.w.now.utc);
        let pos = Position::from_components(LB | 0x0001, x, y, 20.0, 0.0, 0.0, 0.0, 1.0, false);
        o.set_location(Some(pos));
        o.set_position(PositionType::Home, Some(pos));
        o.wo.world_object_properties.current_motion_state =
            Some(Motion::new(MotionStance::NonCombat, Mc::Ready, 1.0));
        // no heartbeats (an object never initialised would heartbeat on every pass)
        o.set_heartbeat_interval(Some(0.0));
        empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
            &mut o,
            self.w.now.unix_time,
        );
        monster::set_monster_state(&mut o);
        self.w.objects.insert(o).expect("fresh guid");
    }

    /// The player, past login (out of the pink bubble), placed at `x, y`.
    pub(crate) fn player(&mut self, x: f32, y: f32) {
        self.creature(Class::Player, PLAYER, "Tester", x, y);
        // as `DoPlayerEnterWorld`: the session's player is set before the player joins its
        // landblock (tracking what it sees sends CreateObject through that session), and every
        // player has a Character
        self.w
            .objects
            .get_mut(PLAYER)
            .unwrap()
            .player
            .as_mut()
            .expect("a player")
            .player
            .character = Some(empyrean_store::models::shard::Character::default());
        self.w.sessions.insert(
            SESSION,
            empyrean_world::sessions::SessionData {
                state: empyrean_net::SessionState::WorldConnected,
                player: Some(PLAYER),
                ..empyrean_world::sessions::SessionData::default()
            },
        );
        self.place(PLAYER);
        phys_ext::set_physics_state(&mut self.w, PLAYER, PhysicsState::Hidden, Some(false));
    }

    /// A melee monster with the combat table, placed at `x, y`.
    pub(crate) fn monster(&mut self, x: f32, y: f32) {
        self.creature(Class::Creature, MONSTER, "Drudge", x, y);
        let o = self.w.objects.get_mut(MONSTER).unwrap();
        o.set_property(PropertyDataId::CombatTable, CMT);
        o.set_property(PropertyFloat::PowerupTime, 1.0);
        // the attacking body part (`DamageEvent.GetBaseDamage` picks it)
        let mut parts = DotNetDict::new();
        parts.insert(
            empyrean_entity::enums::CombatBodyPart::Head,
            empyrean_entity::models::PropertiesBodyPart {
                d_type: empyrean_entity::enums::DamageType::Bludgeon,
                d_val: 10,
                d_var: 0.5,
                ..Default::default()
            },
        );
        o.biota.properties_body_part = Some(Arc::new(parts));
        self.place(MONSTER);
    }

    pub(crate) fn place(&mut self, g: ObjectGuid) {
        assert!(
            lm::add_object(&mut self.w, g, false),
            "placed on the landblock"
        );
    }

    pub(crate) fn pos(&self, g: ObjectGuid) -> Position {
        self.o(g).location().expect("a location")
    }

    pub(crate) fn distance(&self) -> f32 {
        nav::get_distance_to_target(&self.w, MONSTER)
    }

    /// Moves the player's body (and its location) to `x, y`.
    pub(crate) fn move_player(&mut self, x: f32, y: f32) {
        let pos = Position::from_components(LB | 0x0001, x, y, 20.0, 0.0, 0.0, 0.0, 1.0, false);
        let h = phys_ext::physics_obj(&self.w, PLAYER).unwrap();
        assert!(phys_ext::set_position(
            &mut self.w,
            h,
            &phys_ext::to_physics_position(&pos)
        ));
        self.w
            .objects
            .get_mut(PLAYER)
            .unwrap()
            .set_location(Some(pos));
    }
}

pub(crate) fn lb_id() -> LandblockId {
    LandblockId::new(LB | 0xFFFF)
}

/// The counter property an emote table from [`count_emotes`] increments on the emote's target.
const EMOTE_COUNTER: empyrean_entity::enums::PropertyInt =
    empyrean_entity::enums::PropertyInt(0x7F00);

fn count_emotes(h: &mut H, g: ObjectGuid, category: empyrean_entity::enums::EmoteCategory) {
    use empyrean_entity::models::properties_emote::PropertiesEmote;
    use empyrean_entity::models::properties_emote_action::PropertiesEmoteAction;
    let action = PropertiesEmoteAction {
        r#type: empyrean_entity::enums::EmoteType::IncrementIntStat
            .0
            .cast_unsigned(),
        stat: Some(i32::from(EMOTE_COUNTER.0)),
        ..PropertiesEmoteAction::default()
    };
    let set = PropertiesEmote {
        category,
        probability: 1.0,
        properties_emote_action: vec![action],
        ..PropertiesEmote::default()
    };
    h.w.objects.get_mut(g).unwrap().biota.properties_emote = Some(Arc::new(vec![set]));
}

fn emote_count(h: &H, g: ObjectGuid) -> Option<i32> {
    h.w.objects.get(g).unwrap().get_property(EMOTE_COUNTER)
}

fn stance(h: &H) -> MotionStance {
    h.o(MONSTER)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .unwrap()
        .stance
}

// ------------------------------------------------------------------------------------ scenarios

/// A monster spawning next to a player is alerted by `CheckTargets` (0.75 s later), wakes up,
/// takes its attack stance, runs to melee range with a sticky MoveTo, and attacks at ACE's
/// cadence; each strike at the attack frame runs `DamageEvent.CalculateDamage`: the player takes
/// the damage, or evades.
#[test]
fn a_monster_notices_a_player_closes_to_melee_range_and_attacks_at_aces_cadence() {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 110.0);
    h.monster(100.0, 100.0);
    let _ = take_local();

    // CheckTargets: nothing happens for 0.75 s, then the closest visible target alerts it
    awareness::check_targets(&mut h.w, MONSTER);
    h.run(0.7);
    assert!(!awareness::is_awake(&h.w, MONSTER));
    assert!(
        h.run_until(0.2, |h| awareness::is_awake(&h.w, MONSTER)),
        "alerted after 0.75 s"
    );
    assert_eq!(monster::monster_state(&h.w, MONSTER), State::Awake);
    assert_eq!(combat::attack_target(&h.w, MONSTER), Some(PLAYER));
    // (OnWakeUp is observed in `wake_up_alerts_friendly_monsters_of_its_type`: an emote table here
    // would add GetEmoteSet's RNG draws to the sequence pinned below.)

    // the first think takes the attack stance (HandCombat, unarmed)
    assert!(
        h.run_until(1.0, |h| stance(h) == MotionStance::HandCombat),
        "attack stance"
    );
    assert_eq!(
        creature_combat::combat_mode(&h.w, MONSTER),
        CombatMode::Melee
    );

    // it chooses melee (no spells, no missile weapon): MaxRange is MaxMeleeRange, 0.75 m
    assert!(
        h.run_until(1.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Melee)),
        "melee chosen"
    );
    assert_eq!(combat::fields(&h.w, MONSTER).max_range, 0.75);

    // it closes to melee range (0.75 m between the cylinders) and faces the player
    let start = h.distance();
    assert!(start > 8.0, "{start}");
    assert!(
        h.run_until(10.0, |h| h.distance() <= nav::MAX_MELEE_RANGE),
        "closed in: {}",
        h.distance()
    );
    assert!(
        h.pos(MONSTER).position_y > 105.0,
        "the monster moved, the player did not"
    );
    assert_eq!(h.pos(PLAYER).position_y, 110.0);

    // it attacks: a swing motion, then the strike at a third of the swing
    assert!(
        h.run_until(5.0, |h| combat::fields(&h.w, MONSTER).prev_attack_time
            > 0.0),
        "attacked"
    );
    let attack_time = combat::fields(&h.w, MONSTER).prev_attack_time;
    let motion = h
        .o(MONSTER)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
        .unwrap();
    assert_eq!(motion.target_guid, PLAYER);
    assert_eq!(
        motion.motion_state.forward_speed, ANIM_SPEED,
        "GetAnimSpeed: Quickness 60, unarmed"
    );
    let next_move = nav::fields(&h.w, MONSTER).next_move_time;
    assert_eq!(
        next_move,
        attack_time + f64::from(ANIM_LENGTH) + 0.5,
        "NextMoveTime = start + animLength + 0.5"
    );
    let next_attack = combat::fields(&h.w, MONSTER).next_attack_time;

    // RNG in ACE's order since the seed: FindNextTarget's tactic roll, StartTurn's cancel time,
    // GetCombatManeuver's attack height, MeleeAttack's delay (Next(0, PowerupTime 1.0))
    let mut reference = DotNetRandom::new(SEED);
    let tactics = [
        TargetingTactic::None,
        TargetingTactic::Random,
        TargetingTactic::TopDamager,
    ];
    let tactic = tactics[usize::try_from(reference.next_range(1, 3)).unwrap()];
    assert_eq!(
        awareness::fields(&h.w, MONSTER).current_targeting_tactic,
        tactic
    );
    let cancel = reference.next_range(2, 5);
    let f = nav::fields(&h.w, MONSTER);
    assert_eq!(
        f.next_cancel_time,
        f.last_move_time + f64::from(cancel),
        "NextCancelTime = LastMoveTime + Next(2, 4)"
    );
    let height = reference.next_range(1, 4);
    assert_eq!(
        combat::fields(&h.w, MONSTER).attack_height.map(|h| h.0),
        Some(height)
    );
    let expected_motion =
        [Mc::AttackHigh1, Mc::AttackMed1, Mc::AttackLow1][usize::try_from(height - 1).unwrap()];
    assert_eq!(
        motion.motion_state.forward_command, expected_motion,
        "the maneuver for the rolled height"
    );
    let delay = reference.next_double();
    assert_eq!(
        next_attack,
        attack_time + f64::from(ANIM_LENGTH) + delay,
        "NextAttackTime = start + animLength + meleeDelay"
    );

    let _ = take_local();
    let health_before = h.o(PLAYER).health().current(h.o(PLAYER));
    empyrean_world::network::game_messages::game_message::start_capture();
    assert!(h.run_until(0.5, |h| h.running_time()
        >= attack_time + f64::from(ANIM_LENGTH / 3.0) + 0.05));
    let health_after = h.o(PLAYER).health().current(h.o(PLAYER));
    let evaded = empyrean_world::network::game_messages::game_message::take_sent()
        .iter()
        .filter(|(_, _, b)| {
            b.len() >= 16
                && b[..4] == 0xF7B0u32.to_le_bytes()
                && b[12..16] == 0x01B4u32.to_le_bytes()
        })
        .count();
    assert!(
        health_after < health_before || evaded == 1,
        "the strike at the attack frame: a hit or an evade"
    );

    // the next attack waits for NextAttackTime
    assert!(
        h.run_until(3.0, |h| combat::fields(&h.w, MONSTER).prev_attack_time
            > attack_time),
        "attacks again"
    );
    let second = combat::fields(&h.w, MONSTER).prev_attack_time;
    assert!(second >= next_attack, "{second} >= {next_attack}");
    assert!(
        second < next_attack + 0.25,
        "within a monster tick of NextAttackTime"
    );
}

/// V253: each strike of a multi-strike swing hits with its own frame's
/// body part (a claw, then a bite); ACE used the first strike's for every strike.
#[test]
fn each_strike_of_a_swing_uses_its_own_body_part() {
    let hook = |part_index| AnimHook {
        hook_type: 3,
        direction: 1,
        data: HookData::Attack(dereth_primitives::records::AttackCone {
            part_index,
            left: (0.0, 0.0),
            right: (0.0, 0.0),
            radius: 1.0,
            height: 1.0,
        }),
    };
    let frames: Vec<monster_melee::AttackFrame> = vec![
        (0.2, Some(hook(3))),
        (0.5, Some(hook(3))),
        (0.8, Some(hook(0))),
    ];
    let parts: Vec<Option<u32>> = monster_melee::strike_hooks(&frames)
        .iter()
        .map(|c| c.map(|c| c.part_index))
        .collect();
    assert_eq!(parts, [Some(3), Some(3), Some(0)]);
}

/// Wakes the monster through `CheckTargets` (0.75 s after the call) and lets it think once.
pub(crate) fn wake(h: &mut H) {
    awareness::check_targets(&mut h.w, MONSTER);
    assert!(
        h.run_until(1.0, |h| awareness::is_awake(&h.w, MONSTER)),
        "alerted"
    );
}

/// The player runs out of chase range (96 m): the MoveTo is cancelled, `FindNextTarget` finds no
/// target within chase range and the monster runs home (`MoveToHome`, `State.Return`), where
/// `UpdatePosition` puts it to sleep.
#[test]
fn it_gives_up_and_returns_home_when_the_player_leaves_its_range() {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 115.0);
    h.monster(100.0, 100.0);
    let home = h.pos(MONSTER);
    count_emotes(
        &mut h,
        MONSTER,
        empyrean_entity::enums::EmoteCategory::Homesick,
    );
    wake(&mut h);
    let _ = take_local();

    // chasing: it has left home
    assert!(
        h.run_until(5.0, |h| h.pos(MONSTER).position_y > 104.0),
        "chasing"
    );
    assert!(nav::fields(&h.w, MONSTER).is_moving);

    h.move_player(185.0, 190.0);
    assert!(h.distance() >= nav::MAX_CHASE_RANGE);
    assert!(
        h.run_until(1.0, |h| monster::monster_state(&h.w, MONSTER)
            == State::Return),
        "gave up"
    );
    assert_eq!(
        combat::attack_target(&h.w, MONSTER),
        None,
        "MoveToHome drops the target"
    );
    assert_eq!(
        emote_count(&h, PLAYER),
        Some(1),
        "EmoteManager.OnHomeSick: the Homesick set ran on the old target"
    );
    let body = phys_ext::physics_obj(&h.w, MONSTER).unwrap();
    assert!(
        phys_ext::is_moving_to(&h.w, body),
        "PhysicsObj.MoveToPosition(home)"
    );

    // it runs home and sleeps there
    assert!(
        h.run_until(10.0, |h| monster::monster_state(&h.w, MONSTER)
            == State::Idle),
        "home"
    );
    assert!(!awareness::is_awake(&h.w, MONSTER));
    assert_eq!(
        creature_combat::combat_mode(&h.w, MONSTER),
        CombatMode::NonCombat,
        "Sleep: peace mode"
    );
    let at = h.pos(MONSTER);
    let d = ((at.position_x - home.position_x).powi(2) + (at.position_y - home.position_y).powi(2))
        .sqrt();
    assert!(d < 1.0, "back home: {d}");
}

/// With its target gone and nobody else around, the monster's next think finds no target, is
/// already home and goes back to sleep; with no player on it the landblock then goes dormant and
/// the monster stops thinking.
#[test]
fn it_goes_to_sleep_when_no_players_are_around() {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 110.0);
    h.monster(100.0, 100.0);
    wake(&mut h);
    assert!(awareness::is_awake(&h.w, MONSTER));

    // the player leaves (logs out): off the landblock, out of the monster's visible targets
    empyrean_world::entity::landblock::remove_world_object(
        &mut h.w,
        lb_id(),
        PLAYER,
        false,
        false,
        true,
    );
    h.w.objects.remove(PLAYER);
    assert!(
        h.run_until(1.0, |h| !awareness::is_awake(&h.w, MONSTER)),
        "asleep"
    );
    assert_eq!(monster::monster_state(&h.w, MONSTER), State::Idle);
    assert_eq!(combat::attack_target(&h.w, MONSTER), None);
    assert!(
        monster_tick::fields(&h.w, MONSTER).first_update,
        "Sleep resets firstUpdate"
    );

    // asleep, it only reschedules its think; after a minute the landblock is dormant
    let _ = take_local();
    h.run(61.0);
    assert!(h.w.landblock_manager.landblocks.expect(lb_id()).is_dormant);
    let frozen = monster_tick::fields(&h.w, MONSTER).next_monster_tick_time;
    h.run(1.0);
    assert_eq!(
        monster_tick::fields(&h.w, MONSTER).next_monster_tick_time,
        frozen,
        "a dormant landblock suppresses Monster_Tick"
    );
}

impl H {
    /// A bow (max velocity 20: 40.8 m of range) and arrows, wielded by the monster.
    pub(crate) fn arm_with_bow(&mut self) {
        let mut bow = WorldObject::allocate(Class::MissileLauncher);
        bow.guid = BOW;
        bow.biota.id = BOW.full();
        bow.biota.weenie_type = WeenieType::MissileLauncher;
        bow.set_property(PropertyString::Name, "Bow".to_owned());
        bow.set_property(
            PropertyInt::DefaultCombatStyle,
            empyrean_entity::enums::CombatStyle::Bow.0,
        );
        bow.set_property(PropertyFloat::MaximumVelocity, 20.0);
        self.w.objects.insert(bow).unwrap();
        let mut arrows = WorldObject::allocate(Class::Ammunition);
        arrows.guid = ARROWS;
        arrows.biota.id = ARROWS.full();
        arrows.biota.weenie_type = WeenieType::Ammunition;
        arrows.biota.weenie_class_id = ARROW_WCID;
        arrows.set_property(PropertyString::Name, "Arrow".to_owned());
        self.w.objects.insert(arrows).unwrap();
        self.w.content = Arc::new(
            empyrean_content::MemContent::new().weenie(
                empyrean_content::models::world::Weenie::new(
                    ARROW_WCID,
                    "arrow",
                    WeenieType::Ammunition,
                )
                .with_string(PropertyString::Name, "Arrow")
                .with_did(empyrean_entity::enums::PropertyDataId::Setup, ARROW_SETUP),
            ),
        );
        phys_ext::register_setup(&mut self.w, ARROW_SETUP, {
            let mut g = land::test_setup_geometry();
            g.spheres = vec![dereth_physics::geom::Sphere::new(
                Vec3::new(0.0, 0.0, 0.05),
                0.05,
            )];
            g.radius = 0.05;
            g.height = 0.1;
            g
        });
        assert!(creature_equipment::try_equip_object(
            &mut self.w,
            MONSTER,
            BOW,
            EquipMask::MissileWeapon
        ));
        assert!(creature_equipment::try_equip_object(
            &mut self.w,
            MONSTER,
            ARROWS,
            EquipMask::MissileAmmo
        ));
    }
}

/// A missile monster keeps its range turns and shoots.
#[test]
fn a_missile_monster_keeps_its_range_turns_and_shoots() {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(115.0, 100.0);
    h.monster(100.0, 100.0);
    h.arm_with_bow();
    assert!(monster_missile::is_ranged(&h.w, MONSTER));
    let start = h.pos(MONSTER);
    wake(&mut h);
    let _ = take_local();

    assert!(
        h.run_until(2.0, |h| stance(h) == MotionStance::BowCombat),
        "bow stance"
    );
    assert_eq!(
        creature_combat::combat_mode(&h.w, MONSTER),
        CombatMode::Missile
    );
    assert!(
        h.run_until(2.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Missile)),
        "missile attack chosen"
    );
    let max_range = combat::fields(&h.w, MONSTER).max_range;
    assert!(
        (max_range - 20.0 * 20.0 * 0.102_040_82).abs() < 1e-3,
        "GetMaxMissileRange: {max_range}"
    );

    // it turns to face the player, and shoots from where it stands
    assert!(h.run_until(10.0, |h| hits_peek(h) > 0), "shot");
    let at = h.pos(MONSTER);
    assert!(
        (at.position_x - start.position_x).abs() < 0.05
            && (at.position_y - start.position_y).abs() < 0.05,
        "kept its range: {at:?}"
    );
    assert!(
        nav::is_facing(&h.w, MONSTER, Some(PLAYER)),
        "facing the player"
    );
    assert!(h.distance() > 10.0);
    // V336 (V336): with the ranged-closing option on (the default), a target already within
    // range is only turned to: no closing distance is drawn
    assert!(nav::ranged_closing_enabled(&h.w));
    assert_eq!(nav::fields(&h.w, MONSTER).closing_distance, None);
    let prev = combat::fields(&h.w, MONSTER).prev_attack_time;
    let next = combat::fields(&h.w, MONSTER).next_attack_time;
    // `EnqueueMotion`'s animation lengths: the aim (0.5 s at speed 1) and the reload (0.5 s at
    // GetAnimSpeed); the aim-to-Ready link is not in the table (0 s)
    let time_offset = 0.5f32 + 0.5f32 / ANIM_SPEED;
    let expected = prev + f64::from(time_offset) + f64::from(monster_missile::MISSILE_DELAY);
    assert!(
        (next - expected).abs() < 1e-5,
        "NextAttackTime = start + launch + reload + link + MissileDelay: {next} vs {expected}"
    );
    assert_eq!(
        nav::fields(&h.w, MONSTER).next_move_time,
        next,
        "NextMoveTime = NextAttackTime"
    );
}

/// A bow monster, awake, in its bow stance with the missile attack chosen, 15 m from the player.
fn bow_monster_ready() -> H {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(115.0, 100.0);
    h.monster(100.0, 100.0);
    h.arm_with_bow();
    wake(&mut h);
    let _ = take_local();
    assert!(
        h.run_until(2.0, |h| stance(h) == MotionStance::BowCombat),
        "bow stance"
    );
    assert!(
        h.run_until(2.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Missile)),
        "missile attack chosen"
    );
    h
}

/// The player leaves the world as a finished log-off does: held, but its body destroyed.
fn player_leaves_the_world(h: &mut H) {
    empyrean_world::entity::landblock::remove_world_object(
        &mut h.w,
        lb_id(),
        PLAYER,
        false,
        false,
        true,
    );
    assert!(
        h.w.objects.get(PLAYER).is_some() && phys_ext::physics_obj(&h.w, PLAYER).is_none(),
        "held, but no body"
    );
}

/// Whether a projectile of the monster is in the landblock now.
fn monster_projectile_flying(h: &H) -> bool {
    let l =
        h.w.landblock_manager
            .landblocks
            .get(lb_id())
            .expect("the landblock");
    l.get_all_world_objects_for_diagnostics()
        .into_iter()
        .any(|g| {
            h.w.objects
                .get(g)
                .and_then(|o| o.projectile)
                .is_some_and(|p| p.source == Some(MONSTER))
        })
}

/// A missile monster does not shoot at a target that left the world.
/// V325.
#[test]
fn a_missile_monster_does_not_shoot_at_a_target_that_left_the_world() {
    let mut h = bow_monster_ready();
    player_leaves_the_world(&mut h);
    combat::set_attack_target(&mut h.w, MONSTER, Some(PLAYER));
    monster_missile::launch_missile(&mut h.w, MONSTER);
    assert!(!h.run_until(3.0, monster_projectile_flying), "no shot");
}

/// V325 (V325, a fix): the target leaves the world during the aim motion: the release fires
/// nothing (and with the target in the world, it fires). The monster's think is held off after
/// the launch, so only the queued launch can act on the target.
#[test]
fn a_missile_monster_does_not_release_at_a_target_that_left_during_the_windup() {
    for leaves in [false, true] {
        let mut h = bow_monster_ready();
        let prev = combat::fields(&h.w, MONSTER).prev_attack_time;
        // the launch starts the aim motion; the projectile is released at its end
        assert!(
            h.run_until(10.0, |h| combat::fields(&h.w, MONSTER).prev_attack_time
                != prev),
            "the windup"
        );
        assert!(!monster_projectile_flying(&h), "not yet released");
        monster_tick::fields_mut(&mut h.w, MONSTER).next_monster_tick_time = f64::MAX;
        if leaves {
            player_leaves_the_world(&mut h);
        }
        let fired = h.run_until(2.0, monster_projectile_flying);
        assert_eq!(fired, !leaves, "a shot only at a target in the world");
    }
}

fn hits_peek(h: &H) -> u64 {
    let l =
        h.w.landblock_manager
            .landblocks
            .get(lb_id())
            .expect("the landblock");
    let flying = l
        .get_all_world_objects_for_diagnostics()
        .into_iter()
        .any(|g| {
            h.w.objects
                .get(g)
                .and_then(|o| o.projectile)
                .is_some_and(|p| p.source == Some(MONSTER))
        });
    LAUNCHES.with(|c| {
        if flying {
            c.set(true);
        }
        u64::from(c.get())
    })
}

thread_local! {
    static LAUNCHES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// ------------------------------------------------------------------------------------ ACE vectors

fn int(v: &serde_json::Value) -> i64 {
    v.as_i64().expect("an integer")
}

/// `SelectTargetingTactic` over every `TargetingTactic` value: the flags in `Enum.GetValues`
/// order, one `Next(1, count - 1)` draw.
#[test]
fn select_targeting_tactic_matches_ace() {
    let mut h = H::new();
    let file = vectors::load_named("monster", "select_targeting_tactic");
    let mut failures = Vec::new();
    for (i, case) in file.cases.iter().enumerate() {
        let guid = ObjectGuid::new(0x8000_1000 + u32::try_from(i).unwrap());
        h.creature(Class::Creature, guid, "m", 10.0, 10.0);
        let tactic = i32::try_from(int(&case.input["tactic"])).unwrap();
        if tactic != 0 {
            h.w.objects
                .get_mut(guid)
                .unwrap()
                .set_property(PropertyInt::TargetingTactic, tactic);
        }
        ThreadSafeRandom::seed(u64::try_from(int(&case.input["seed"])).unwrap());
        awareness::select_targeting_tactic(&mut h.w, guid);
        let current = i64::from(awareness::fields(&h.w, guid).current_targeting_tactic.0);
        let next = ThreadSafeRandom::next_float(0.0, 1.0);
        if current != int(&case.output["current"])
            || next != vectors::f64_of(&case.output["next"]).unwrap()
        {
            failures.push(format!("{}: got {current} {next}", case.input));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ: {:?}",
        failures.len(),
        file.cases.len(),
        &failures[..failures.len().min(5)]
    );
}

/// `SelectWeightedDistance`: the roll in `[0, n - 1)`, the inverted ratios summed in float, the
/// LINQ sum of the distances in double, the first-target fallback (all distances zero).
#[test]
fn select_weighted_distance_matches_ace() {
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "m", 10.0, 10.0);
    let file = vectors::load_named("monster", "select_weighted_distance");
    let mut failures = Vec::new();
    for case in &file.cases {
        let targets: Vec<TargetDistance> = case.input["distances"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(k, d)| {
                TargetDistance::new(
                    ObjectGuid::new(0x5000_0001 + u32::try_from(k).unwrap()),
                    vectors::f32_of(d).unwrap(),
                )
            })
            .collect();
        ThreadSafeRandom::seed(u64::try_from(int(&case.input["seed"])).unwrap());
        let chosen = awareness::select_weighted_distance(&h.w, MONSTER, &targets);
        let index =
            i64::try_from(targets.iter().position(|t| t.target == chosen).unwrap()).unwrap();
        let next = ThreadSafeRandom::next_float(0.0, 1.0);
        if index != int(&case.output["index"])
            || next != vectors::f64_of(&case.output["next"]).unwrap()
        {
            failures.push(format!("{}: got {index} {next}", case.input));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ: {:?}",
        failures.len(),
        file.cases.len(),
        &failures[..failures.len().min(5)]
    );
}

/// `GetNextAttackType`: the spellbook rolled in insertion order (one `Next(0.0f, 1.0f)` per spell
/// until one hits, `new Spell(id)` for the hit), else missile with a missile weapon, else melee.
#[test]
fn next_attack_type_matches_ace() {
    let mut h = H::new();
    let file = vectors::load_named("monster", "next_attack_type");
    let mut failures = Vec::new();
    for (i, case) in file.cases.iter().enumerate() {
        let n = u32::try_from(i).unwrap();
        let guid = ObjectGuid::new(0x8000_2000 + 2 * n);
        h.creature(Class::Creature, guid, "m", 10.0, 10.0);
        let spells = case.input["spells"].as_array().unwrap();
        // the harness gives a spellbook to every case with spells, and an empty one to i % 10 == 1
        if !spells.is_empty() || i % 10 == 1 {
            let mut book = DotNetDict::new();
            for s in spells {
                book.insert(
                    i32::try_from(int(&s[0])).unwrap(),
                    vectors::f32_of(&s[1]).unwrap(),
                );
            }
            h.w.objects
                .get_mut(guid)
                .unwrap()
                .biota
                .properties_spell_book = Some(book);
        }
        if case.input["ranged"].as_bool().unwrap() {
            let bow = ObjectGuid::new(0x8000_2001 + 2 * n);
            let mut o = WorldObject::allocate(Class::MissileLauncher);
            o.guid = bow;
            o.biota.id = bow.full();
            h.w.objects.insert(o).unwrap();
            assert!(creature_equipment::try_equip_object(
                &mut h.w,
                guid,
                bow,
                EquipMask::MissileWeapon
            ));
        }
        ThreadSafeRandom::seed(u64::try_from(int(&case.input["seed"])).unwrap());
        let t = combat::get_next_attack_type(&mut h.w, guid);
        let ty = match t {
            CombatType::Melee => 0,
            CombatType::Missile => 1,
            CombatType::Magic => 2,
        };
        let spell = empyrean_world::world_objects::monster_magic::fields(&h.w, guid)
            .current_spell
            .as_ref()
            .map(|s| i64::from(s.id()));
        let next = ThreadSafeRandom::next_float(0.0, 1.0);
        let want_spell = case.output["spell"].as_i64();
        if ty != int(&case.output["type"])
            || spell != want_spell
            || next != vectors::f64_of(&case.output["next"]).unwrap()
        {
            failures.push(format!("{}: got {ty} {spell:?} {next}", case.input));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ: {:?}",
        failures.len(),
        file.cases.len(),
        &failures[..failures.len().min(5)]
    );
}

/// `Creature.GetAngle(Vector3, Vector3)`: `Acos` of the 2D dot product in degrees, 0 for NaN.
#[test]
fn get_angle_matches_ace() {
    let file = vectors::load_named("monster", "get_angle");
    let v = |x: &serde_json::Value| {
        let a = x.as_array().unwrap();
        Vector3::new(
            vectors::f32_of(&a[0]).unwrap(),
            vectors::f32_of(&a[1]).unwrap(),
            vectors::f32_of(&a[2]).unwrap(),
        )
    };
    let mut failures = Vec::new();
    for case in &file.cases {
        let got = empyrean_world::world_objects::creature_navigation::get_angle_between(
            v(&case.input["a"]),
            v(&case.input["b"]),
        );
        if got.to_bits() != vectors::f32_of(&case.output).unwrap().to_bits() {
            failures.push(format!("{}: got {got}", case.input));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ: {:?}",
        failures.len(),
        file.cases.len(),
        &failures[..failures.len().min(5)]
    );
}

/// Rotate toward matches ace including the same point.
/// V318.
#[test]
fn rotate_toward_matches_ace_including_the_same_point() {
    let file = vectors::load_named("monster", "rotate_toward");
    let f = |x: &serde_json::Value| vectors::f32_of(x).unwrap();
    let v = |x: &serde_json::Value| {
        let a = x.as_array().unwrap();
        Vector3::new(f(&a[0]), f(&a[1]), f(&a[2]))
    };
    let mut failures = Vec::new();
    let mut same_point = 0;
    for case in &file.cases {
        let (a, b) = (v(&case.input["a"]), v(&case.input["b"]));
        let dir = empyrean_world::world_objects::creature_navigation::get_direction(a, b);
        let mut p = Position::default();
        p.rotate(dir);
        let r = p.rotation();
        let want_dir = v(&case.output["dir"]);
        let want = case.output["rot"].as_array().unwrap();
        let pairs = [
            (dir.x, want_dir.x),
            (dir.y, want_dir.y),
            (dir.z, want_dir.z),
            (r.x, f(&want[0])),
            (r.y, f(&want[1])),
            (r.z, f(&want[2])),
            (r.w, f(&want[3])),
        ];
        if !pairs.iter().all(|&(g, w)| vectors::same_f32(g, w)) {
            failures.push(format!("{}: got {dir:?} {r:?}", case.input));
        }
        if (a.x, a.y, a.z) == (b.x, b.y, b.z) {
            same_point += 1;
            assert!(
                r.w.is_nan() && r.x.is_nan() && r.y.is_nan() && r.z.is_nan(),
                "ACE's NaN rotation on the same point: {r:?}"
            );
        }
    }
    assert_eq!(same_point, 3, "the same-point cases ran");
    assert!(
        failures.is_empty(),
        "{} of {} differ: {:?}",
        failures.len(),
        file.cases.len(),
        &failures[..failures.len().min(5)]
    );
}

// ------------------------------------------------------------------------------------ unit

/// `Position.CylinderDistance(Sq)`, the shared crate's retail cylinder distance (V310, V310):
/// the vertical gap and the reach combine when both are positive or both not; a positive gap over
/// a horizontal overlap is the gap (ACE answered the reach there); the squared form is the signed
/// square.
#[test]
fn cylinder_distance_is_the_shared_crates() {
    use dereth_primitives::{CellId, Frame, Position as P, Quat};
    let at = |x: f32, z: f32| {
        P::new(
            CellId(LB | 1),
            Frame::new(Vec3::new(x, 10.0, z), Quat::IDENTITY),
        )
    };
    // the reach is the 3-D centre distance less the radii (Position.GetOffset(..).Length())
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    // side by side, 3 m apart, radii 0.5 + 0.5, same height: reach 2, the gap is negative
    assert_eq!(
        nav::cylinder_distance(0.5, 1.0, &at(0.0, 0.0), 0.5, 1.0, &at(3.0, 0.0)),
        2.0
    );
    assert_eq!(
        nav::cylinder_distance_sq(0.5, 1.0, &at(0.0, 0.0), 0.5, 1.0, &at(3.0, 0.0)),
        4.0
    );
    // 3 m apart, the other 4 m up: reach 5 - 1 = 4, gap 4 - 1 = 3, both positive: 5
    assert_eq!(
        nav::cylinder_distance(0.5, 1.0, &at(0.0, 0.0), 0.5, 1.0, &at(3.0, 4.0)),
        5.0
    );
    assert_eq!(
        nav::cylinder_distance_sq(0.5, 1.0, &at(0.0, 0.0), 0.5, 1.0, &at(3.0, 4.0)),
        25.0
    );
    // Changed quadrant (V310): radii 2 + 2, the other 2 m straight up, height 1: reach 2 - 4 = -2,
    // gap 2 - 1 = 1. The distance is the gap, 1 (ACE answered the reach, -2, and 4 squared)
    assert_eq!(
        nav::cylinder_distance(2.0, 1.0, &at(0.0, 0.0), 2.0, 1.0, &at(0.0, 2.0)),
        1.0
    );
    assert_eq!(
        nav::cylinder_distance_sq(2.0, 1.0, &at(0.0, 0.0), 2.0, 1.0, &at(0.0, 2.0)),
        1.0
    );
    // overlapping both ways (reach sqrt(0.5) - 1, gap -0.5): minus the hypotenuse, signed
    let reach = f64::from(0.5f32.hypot(0.5) - 1.0);
    assert!(close(
        nav::cylinder_distance(0.5, 1.0, &at(0.0, 0.0), 0.5, 1.0, &at(0.5, 0.5)),
        -(reach * reach + 0.25).sqrt()
    ));
    assert!(close(
        nav::cylinder_distance_sq(0.5, 1.0, &at(0.0, 0.0), 0.5, 1.0, &at(0.5, 0.5)),
        -(reach * reach + 0.25)
    ));
    // overlapping horizontally, the ends exactly level (gap 0, reach 1 - 4 = -3): the reach, and
    // its square keeps the sign (ACE's squared form answered +9)
    assert_eq!(
        nav::cylinder_distance(2.0, 1.0, &at(0.0, 0.0), 2.0, 1.0, &at(0.0, 1.0)),
        -3.0
    );
    assert_eq!(
        nav::cylinder_distance_sq(2.0, 1.0, &at(0.0, 0.0), 2.0, 1.0, &at(0.0, 1.0)),
        -9.0
    );
}

/// V310 (V310): a wide monster (radius 12 m, 1 m tall) directly below a player standing on a
/// ledge 10 m above its top. The reach is the 3-D centre distance less the radii, 11 - 12.5 =
/// -1.5, so the cylinders overlap "horizontally" and the distance is the vertical gap, 10 m: out
/// of melee range. ACE's distance was the negative reach, so the monster thought the player was
/// touching it.
#[test]
fn a_monster_directly_below_a_player_on_a_ledge_is_the_gap_away() {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 110.0);
    h.monster(100.0, 100.0);
    combat::set_attack_target(&mut h.w, MONSTER, Some(PLAYER));

    let mb = phys_ext::physics_obj(&h.w, MONSTER).unwrap();
    let pb = phys_ext::physics_obj(&h.w, PLAYER).unwrap();
    let (monster_pos, monster_height) = {
        let o = h.w.physics.get_mut(mb).unwrap();
        Arc::make_mut(&mut o.geometry).radius = 12.0;
        (o.position, o.height())
    };
    assert_eq!(monster_height, 1.0);
    assert_eq!(h.w.physics.get(pb).unwrap().radius(), 0.5);
    let mut ledge = monster_pos;
    ledge.frame.origin.z += monster_height + 10.0;
    h.w.physics.get_mut(pb).unwrap().position = ledge;

    let dist = nav::get_distance_to_target(&h.w, MONSTER);
    assert!((dist - 10.0).abs() < 1e-4, "the vertical gap: {dist}");
    assert!(
        !nav::is_melee_range(&h.w, MONSTER),
        "10 m straight up is out of melee range"
    );

    // the player steps off the ledge to stand beside the monster: in reach again
    let mut beside = monster_pos;
    beside.frame.origin.x += 1.0;
    h.w.physics.get_mut(pb).unwrap().position = beside;
    assert!(
        nav::is_melee_range(&h.w, MONSTER),
        "beside it: {}",
        nav::get_distance_to_target(&h.w, MONSTER)
    );
}

/// `IsFacing`: 5 degrees, plus 1.5 degrees for every metre closer than 10.
#[test]
fn is_facing_threshold_widens_up_close() {
    assert!(nav::is_facing_threshold(4.9, 20.0));
    assert!(!nav::is_facing_threshold(5.0, 10.0));
    assert!(nav::is_facing_threshold(19.9, 0.0), "5 + 10 * 1.5 = 20");
    assert!(!nav::is_facing_threshold(20.0, 0.0));
    assert!(nav::is_facing_threshold(12.4, 5.0), "5 + 5 * 1.5 = 12.5");
}

/// `SetMonsterState`: attackable or with a targeting tactic is a monster; the faction flag needs a
/// monster.
#[test]
fn set_monster_state_classifies() {
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "m", 10.0, 10.0);
    assert!(monster::fields(&h.w, MONSTER).is_monster);
    assert!(!monster::fields(&h.w, MONSTER).is_faction_mob);

    let g = ObjectGuid::new(0x8000_0200);
    h.creature(Class::Creature, g, "npc", 10.0, 10.0);
    let o = h.w.objects.get_mut(g).unwrap();
    o.set_property(PropertyBool::Attackable, false);
    o.set_property(PropertyInt::Faction1Bits, 1);
    monster::set_monster_state(o);
    assert!(!monster::fields(&h.w, g).is_monster);
    assert!(
        !monster::fields(&h.w, g).is_faction_mob,
        "a faction needs a monster"
    );
    let o = h.w.objects.get_mut(g).unwrap();
    o.set_property(PropertyInt::TargetingTactic, TargetingTactic::Random.0);
    monster::set_monster_state(o);
    assert!(
        monster::fields(&h.w, g).is_monster,
        "a targeting tactic makes a monster"
    );
    assert!(monster::fields(&h.w, g).is_faction_mob);
}

/// `Creature.TakeDamage` (non-lethal): `Math.Round`, the delta through `UpdateVitalDelta`, the
/// damage history; no source (a combined DoT tick) records nothing.
#[test]
fn take_damage_updates_health_and_the_damage_history() {
    use empyrean_entity::enums::DamageType;
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "m", 10.0, 10.0);
    h.creature(Class::Player, PLAYER, "p", 10.0, 12.0);
    let health = |h: &H| {
        let o = h.o(MONSTER);
        o.health().current(o)
    };
    assert_eq!(
        combat::creature_take_damage(&mut h.w, MONSTER, PLAYER, DamageType::Slash, 30.5, false),
        30,
        "banker's rounding"
    );
    assert_eq!(health(&h), 70);
    let last = empyrean_world::entity::damage_history::of(&h.w, MONSTER).last_damager();
    assert_eq!(last.and_then(|d| d.try_get_attacker(&h.w)), Some(PLAYER));
    assert_eq!(
        combat::creature_take_damage(
            &mut h.w,
            MONSTER,
            ObjectGuid::INVALID,
            DamageType::Fire,
            9.6,
            false
        ),
        10
    );
    assert_eq!(health(&h), 60);
    assert_eq!(
        empyrean_world::entity::damage_history::of(&h.w, MONSTER)
            .log
            .len(),
        1,
        "no source: no history entry"
    );
}

/// A woken monster alerts the sleeping creatures of its type within their aural range, once per
/// target (recorded in `Alerted`).
#[test]
fn wake_up_alerts_friendly_monsters_of_its_type() {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 120.0);
    let friend = ObjectGuid::new(0x8000_0201);
    let far = ObjectGuid::new(0x8000_0202);
    for (g, x) in [(MONSTER, 100.0), (friend, 105.0), (far, 160.0)] {
        h.creature(Class::Creature, g, "Drudge", x, 100.0);
        h.w.objects
            .get_mut(g)
            .unwrap()
            .set_property(PropertyInt::CreatureType, 1);
        h.place(g);
        count_emotes(&mut h, g, empyrean_entity::enums::EmoteCategory::Scream);
    }
    let _ = take_local();
    combat::set_attack_target(&mut h.w, MONSTER, Some(PLAYER));
    awareness::wake_up(&mut h.w, MONSTER, true);
    assert!(
        awareness::is_awake(&h.w, friend),
        "5 m away: within the 18 m aural range"
    );
    assert_eq!(combat::attack_target(&h.w, friend), Some(PLAYER));
    assert!(!awareness::is_awake(&h.w, far), "60 m away");
    assert_eq!(
        emote_count(&h, PLAYER),
        Some(2),
        "OnWakeUp: the monster, then its friend (WakeUp(false): no chain)"
    );
    assert!(awareness::fields(&h.w, MONSTER)
        .alerted
        .as_ref()
        .is_some_and(|a| a.contains_key(&PLAYER.full())));
}

/// The ACE.Entity reductions the maneuver choice falls back on.
#[test]
fn multi_strike_and_subsequent_reductions() {
    use empyrean_entity::enums::DamageType;
    assert_eq!(
        monster_melee::attack_type_reduce_multi_strike(AttackType::TripleSlash),
        AttackType::Slash
    );
    assert_eq!(
        monster_melee::attack_type_reduce_multi_strike(AttackType::OffhandDoubleThrust),
        AttackType::OffhandThrust
    );
    assert_eq!(
        monster_melee::attack_type_reduce_multi_strike(AttackType::Punch),
        AttackType::Undef
    );
    assert_eq!(
        monster_melee::motion_command_reduce_multi_strike(Mc::TripleThrustMed),
        Mc::ThrustMed
    );
    assert_eq!(
        monster_melee::motion_command_reduce_multi_strike(Mc::OffhandDoubleSlashHigh),
        Mc::SlashHigh,
        "offhand reduces to the main hand"
    );
    assert_eq!(
        monster_melee::motion_command_reduce_multi_strike(Mc::AttackHigh1),
        Mc::Invalid
    );
    assert_eq!(
        monster_melee::motion_command_reduce_subsequent(Mc::AttackMed3),
        Mc::AttackMed1
    );
    assert_eq!(
        monster_melee::motion_command_reduce_subsequent(Mc::AttackLow5),
        Mc::AttackLow1
    );
    assert_eq!(
        monster_melee::motion_command_reduce_subsequent(Mc::AttackHigh1),
        Mc::Invalid
    );
    assert!(DamageType(3).is_multi_damage());
    assert!(!DamageType::Fire.is_multi_damage());
}

/// `GetMovementParameters`: ACE's MovementParameters without walking; a melee MoveTo adds fail
/// walk, final heading, sticky and move away; a ranged monster only turns. `GetRunRate` is the
/// Run skill's rate.
#[test]
fn movement_parameters_for_melee_and_ranged() {
    use dereth_animation::motion::flags;
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 110.0);
    h.monster(100.0, 100.0);
    combat::set_attack_target(&mut h.w, MONSTER, Some(PLAYER));
    let mvp = nav::get_movement_parameters(&mut h.w, MONSTER);
    assert_eq!(mvp.flags & flags::CAN_WALK, 0);
    let melee = flags::FAIL_WALK | flags::USE_FINAL_HEADING | flags::STICKY | flags::MOVE_AWAY;
    assert_eq!(mvp.flags & melee, melee);
    h.arm_with_bow();
    let mvp = nav::get_movement_parameters(&mut h.w, MONSTER);
    assert_eq!(mvp.flags & flags::STICKY, 0, "ranged: a TurnTo");
    let run_rate = nav::get_run_rate(&mut h.w, MONSTER);
    let expected: f32 = empyrean_common::dotnet::CsCast::cs_cast(
        empyrean_world::physics::weenie_object::movement_system_get_run_rate(0.0, 100, 1.0),
    );
    assert_eq!(
        run_rate, expected,
        "MovementSystem.GetRunRate(0, Run 100, 1.0)"
    );
}

/// Above eight hundred run a player runs on the clients curve and a creature on aces cap.
/// V332.
#[test]
fn above_eight_hundred_run_a_player_runs_on_the_clients_curve_and_a_creature_on_aces_cap() {
    use empyrean_world::physics::weenie_object::{movement_system_get_run_rate, player_run_rate};
    // The two formulas at the edges.
    for s in [0, 100, 799, 800] {
        assert_eq!(
            player_run_rate(0.0, s).to_bits(),
            movement_system_get_run_rate(0.0, s, 1.0).to_bits(),
            "Run {s}"
        );
    }
    for s in [801, 900, 1100, 2100] {
        assert_eq!(
            movement_system_get_run_rate(0.0, s, 1.0),
            4.5,
            "ACE, Run {s}"
        );
        let p = player_run_rate(0.0, s);
        assert!(p > 3.2 && p < 3.7, "client, Run {s}: {p}");
        assert_eq!(
            p,
            f64::from(dereth_rules::skills::get_run_rate(0.0, s, 1.0))
        );
    }

    // And in the world: both at Run 900 (skill init level; no Quickness in this harness's formula
    // would take it below 800).
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.player(100.0, 110.0);
    h.monster(100.0, 100.0);
    for g in [PLAYER, MONSTER] {
        let o = h.w.objects.get_mut(g).unwrap();
        let mut skills = DotNetDict::new();
        skills.insert(
            Skill::Run,
            PropertiesSkill {
                init_level: 900,
                sac: SkillAdvancementClass::Trained,
                ..PropertiesSkill::default()
            },
        );
        o.biota.properties_skill = Some(skills);
    }
    let monster = nav::get_run_rate(&mut h.w, MONSTER);
    let player = nav::get_run_rate(&mut h.w, PLAYER);
    assert_eq!(monster, 4.5, "creature: ACE's cap");
    assert!(
        player > 3.2 && player < 3.7,
        "player: the client's curve, got {player}"
    );
}

/// `GetCombatManeuver` with a three-height stance rolls the height with `Next(1, 3)` (High from a
/// stance with all three, only Medium and Low otherwise), then the maneuver for it.
#[test]
fn get_combat_maneuver_rolls_the_height_in_aces_order() {
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "m", 10.0, 10.0);
    let o = h.w.objects.get_mut(MONSTER).unwrap();
    o.set_property(PropertyDataId::CombatTable, CMT);
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::HandCombat, Mc::Ready, 1.0));
    combat::get_combat_table(&mut h.w, MONSTER);
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..40 {
        ThreadSafeRandom::seed(seed);
        let got = monster_melee::get_combat_maneuver(&mut h.w, MONSTER);
        let mut reference = DotNetRandom::new(i32::try_from(seed).unwrap());
        let height = reference.next_range(1, 4);
        let want =
            [Mc::AttackHigh1, Mc::AttackMed1, Mc::AttackLow1][usize::try_from(height - 1).unwrap()];
        assert_eq!(got, Some(want), "seed {seed}");
        assert_eq!(
            combat::fields(&h.w, MONSTER).attack_height.map(|a| a.0),
            Some(height)
        );
        let attack_type = if height == 3 {
            AttackType::Kick
        } else {
            AttackType::Punch
        };
        assert_eq!(
            creature_combat::attack_type(&h.w, MONSTER),
            attack_type,
            "unarmed: a punch, low a kick"
        );
        assert_eq!(
            ThreadSafeRandom::next_float(0.0, 1.0),
            reference.next_double(),
            "one draw (one maneuver per type)"
        );
        seen.insert(height);
    }
    assert_eq!(seen.len(), 3, "all three heights occur");
}

// ------------------------------------------------------------------------------------ V336

/// The monster's current motion: its kind, MoveTo flag word, distance, minimum and fail distance.
fn move_to(h: &H) -> (empyrean_entity::enums::MovementType, u32, f32, f32, f32) {
    let m = h
        .o(MONSTER)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .unwrap();
    let p = &m.move_to_parameters;
    (
        m.movement_type,
        p.movement_parameters.0,
        p.distance_to_object,
        p.min_distance,
        p.fail_distance,
    )
}

/// Whether the monster has sent a MoveToObject.
fn moving_to(h: &H) -> bool {
    move_to(h).0 == empyrean_entity::enums::MovementType::MoveToObject
}

/// Gives the monster a spellbook of `spell` alone, always rolled (probability 2.0 + 1.0), and War
/// Magic 200.
fn caster(h: &mut H, spell: u32) {
    let o = h.w.objects.get_mut(MONSTER).unwrap();
    let mut book = DotNetDict::new();
    book.insert(i32::try_from(spell).unwrap(), 3.0f32);
    o.biota.properties_spell_book = Some(book);
    o.biota.properties_skill.as_mut().unwrap().insert(
        Skill::WarMagic,
        PropertiesSkill {
            init_level: 200,
            sac: SkillAdvancementClass::Trained,
            ..PropertiesSkill::default()
        },
    );
    // the spell's world row (a spell with no effect)
    h.w.content = Arc::new(empyrean_content::MemContent::new().spell(
        empyrean_content::models::world::Spell {
            id: spell,
            name: "Test Spell".to_owned(),
            ..empyrean_content::models::world::Spell::default()
        },
    ));
}

/// A monster awake 15 m from the player, who then steps back to 60 m (beyond a 40.8 m bow and a
/// ~20 m spell); `setup` arms it first.
fn target_beyond_range(closing: bool, setup: impl FnOnce(&mut H)) -> H {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    if !closing {
        pm::modify_bool(&h.w, "monster_ranged_closing", false);
    }
    assert_eq!(nav::ranged_closing_enabled(&h.w), closing);
    h.player(100.0, 115.0);
    h.monster(100.0, 100.0);
    setup(&mut h);
    wake(&mut h);
    h.move_player(100.0, 160.0);
    let _ = take_local();
    h
}

/// Asserts the drawn closing distance lies in [2/3 R, 16/15 R] and the MoveTo sent is the
/// non-sticky chase to it; answers it.
fn assert_closing_move(h: &H) -> f32 {
    use empyrean_world::network::motion::move_to_parameters::RetailMoveTo;
    let r = combat::fields(&h.w, MONSTER).max_range;
    let dto = nav::fields(&h.w, MONSTER)
        .closing_distance
        .expect("a closing distance");
    assert!(
        dto >= r * 2.0 / 3.0 - 1e-4 && dto <= r * 16.0 / 15.0 + 1e-4,
        "dto {dto} in [2/3, 16/15] x R {r}"
    );
    let (_, flags, d, min, fail) = move_to(h);
    assert_eq!(
        flags,
        RetailMoveTo::AttackChaseUnstuck.flags(),
        "the non-sticky chase, 0x1EF70"
    );
    assert_eq!(flags, 0x1_EF70);
    assert_eq!(
        (d, min, fail),
        (dto, 0.0, f32::MAX),
        "to the drawn distance, min 0, fail FLT_MAX"
    );
    dto
}

/// V336 (V336, the `monster_ranged_closing` option, on by default): a bow monster whose target
/// is beyond its 40.8 m range runs toward it with the non-sticky chase (0x1EF70) to a distance
/// drawn from [2/3 R, 16/15 R], stops there, turns and shoots. ACE's would switch to melee.
#[test]
fn a_missile_monster_beyond_its_range_closes_to_a_drawn_distance_and_shoots() {
    let mut h = target_beyond_range(true, H::arm_with_bow);
    assert!(
        h.run_until(5.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Missile)
            && moving_to(h)),
        "a MoveTo"
    );
    let r = combat::fields(&h.w, MONSTER).max_range;
    assert!(
        (r - 20.0 * 20.0 * 0.102_040_82).abs() < 1e-3,
        "R = v^2 / 9.8: {r}"
    );
    let dto = assert_closing_move(&h);
    assert!(
        h.distance() > dto,
        "the target is farther than the drawn distance"
    );

    assert!(h.run_until(20.0, |h| hits_peek(h) > 0), "shot");
    let dist = h.distance();
    // the body moves a monster tick (a quarter second, ~2 m of running) at a time
    assert!(
        dist <= dto + 0.05 && dist > dto - 2.5,
        "shot from the drawn distance: {dist} vs {dto}"
    );
    assert!(
        h.pos(MONSTER).position_y > 110.0,
        "it ran toward the player"
    );
    assert!(
        !monster_missile::fields(&h.w, MONSTER).switch_weapons_pending,
        "no switch to melee"
    );
    assert!(
        creature_equipment::get_equipped_missile_weapon(&h.w, MONSTER).is_some(),
        "still wields the bow"
    );
}

/// V336 with the option off: ACE's. The bow monster whose target is beyond its range stands
/// where it is and switches to melee.
#[test]
fn with_the_option_off_a_missile_monster_beyond_its_range_switches_to_melee() {
    let mut h = target_beyond_range(false, H::arm_with_bow);
    let start = h.pos(MONSTER);
    assert!(
        h.run_until(5.0, |h| monster_missile::fields(&h.w, MONSTER)
            .switch_weapons_pending),
        "TrySwitchToMeleeAttack"
    );
    assert_eq!(nav::fields(&h.w, MONSTER).closing_distance, None);
    let at = h.pos(MONSTER);
    assert!(
        (at.position_y - start.position_y).abs() < 0.05,
        "it did not close in"
    );
}

/// V336 (V336): a caster whose target is beyond its spell's range (10 + 0.05 x War Magic)
/// runs with the non-sticky chase to a distance drawn from [2/3 R, 16/15 R], stops there and
/// casts; ACE's chases stickily and casts on the way.
#[test]
fn a_caster_beyond_its_spell_range_closes_to_a_drawn_distance_and_casts() {
    let mut h = target_beyond_range(true, |h| caster(h, CLOSE_SPELL));
    assert!(
        h.run_until(5.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Magic)
            && moving_to(h)),
        "a MoveTo"
    );
    let skill = empyrean_world::world_objects::monster_magic::get_magic_skill_for_range_check(
        &mut h.w, MONSTER,
    );
    let r = combat::fields(&h.w, MONSTER).max_range;
    #[allow(clippy::cast_precision_loss)]
    let want = 10.0 + skill as f32 * 0.05;
    assert_eq!(r, want, "R = constant + mod x skill ({skill})");
    let dto = assert_closing_move(&h);

    assert!(
        h.run_until(20.0, |h| combat::fields(&h.w, MONSTER).prev_attack_time
            > 0.0),
        "cast"
    );
    let dist = h.distance();
    assert!(
        dist <= dto + 0.05 && dist > dto - 2.5,
        "cast from the drawn distance: {dist} vs {dto}"
    );
    assert!(dist > 15.0, "it stopped short of the player: {dist}");
    assert!(!nav::is_sticky(&h.w, MONSTER), "not stuck to the target");
}

/// V336 with the option off: ACE's. The caster beyond its spell's range chases with the sticky
/// attack chase (0x1EFF0).
#[test]
fn with_the_option_off_a_caster_beyond_its_spell_range_chases_stickily() {
    use empyrean_world::network::motion::move_to_parameters::RetailMoveTo;
    let mut h = target_beyond_range(false, |h| caster(h, CLOSE_SPELL));
    assert!(
        h.run_until(5.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Magic)
            && moving_to(h)),
        "a MoveTo"
    );
    assert_eq!(
        move_to(&h).1,
        RetailMoveTo::AttackChase.flags(),
        "the sticky chase"
    );
    assert_eq!(nav::fields(&h.w, MONSTER).closing_distance, None);
}

/// V336 (V336): with the option on, a monster's spell range is not capped (a 90 m spell is
/// 90 m); off, ACE's cap of 75.
#[test]
fn spell_range_is_uncapped_with_the_option_and_capped_at_seventy_five_without() {
    use empyrean_world::world_objects::monster_magic;
    let mut h = H::new();
    h.creature(Class::Creature, MONSTER, "m", 10.0, 10.0);
    caster(&mut h, FAR_SPELL);
    assert_eq!(
        combat::get_next_attack_type(&mut h.w, MONSTER),
        CombatType::Magic
    );
    assert_eq!(monster_magic::get_spell_max_range(&mut h.w, MONSTER), 90.0);
    pm::modify_bool(&h.w, "monster_ranged_closing", false);
    assert_eq!(monster_magic::get_spell_max_range(&mut h.w, MONSTER), 75.0);
}

/// V336 (V336): with the option on, a melee monster still chases with the sticky attack chase
/// (0x1EFF0) and draws no closing distance.
#[test]
fn with_the_option_on_a_melee_monster_still_chases_stickily() {
    use empyrean_world::network::motion::move_to_parameters::RetailMoveTo;
    let mut h = target_beyond_range(true, |_| {});
    assert!(
        h.run_until(5.0, |h| combat::fields(&h.w, MONSTER).current_attack
            == Some(CombatType::Melee)
            && moving_to(h)),
        "a MoveTo"
    );
    assert_eq!(
        move_to(&h).1,
        RetailMoveTo::AttackChase.flags(),
        "the sticky chase"
    );
    assert_eq!(nav::fields(&h.w, MONSTER).closing_distance, None);
    assert!(
        h.run_until(20.0, |h| h.distance() <= nav::MAX_MELEE_RANGE),
        "closed to melee range"
    );
}

mod player_awareness {
    use crate::support::player_world::*;

    /// `Player.CheckMonsters`: an idle monster 10 m away (inside its awareness range) is alerted, and
    /// targets the player (`AlertMonster`: `AttackTarget = this; WakeUp()`).
    #[test]
    fn check_monsters_alerts_an_idle_monster_in_range() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        h.run(1.0);
        h.player(100.0, 110.0);
        h.run(1.0);
        assert!(
            !monster_awareness::is_awake(&h.w, MONSTER),
            "nobody alerted it yet"
        );

        player_monster::check_monsters(&mut h.w, PLAYER);
        assert!(monster_awareness::is_awake(&h.w, MONSTER));
        assert_eq!(monster_combat::attack_target(&h.w, MONSTER), Some(PLAYER));
    }

    /// `Player.CheckMonsters` does nothing while the player is teleporting (or not attackable).
    #[test]
    fn check_monsters_does_nothing_while_teleporting() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        h.run(1.0);
        h.player(100.0, 110.0);
        h.run(1.0);
        h.w.objects
            .get_mut(PLAYER)
            .unwrap()
            .wo
            .world_object
            .teleporting = true;
        player_monster::check_monsters(&mut h.w, PLAYER);
        assert!(!monster_awareness::is_awake(&h.w, MONSTER));
    }

    /// `Player.OnAttackMonster`: a monster that is not awake wakes and targets the attacker; one with
    /// `Tolerance.NoAttack` (`PlayerCombatPet_RetaliateExclude`) stays put. A null monster is ignored.
    #[test]
    fn on_attack_monster_wakes_the_monster_unless_it_never_attacks() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        h.player(100.0, 150.0);
        h.run(0.5);

        h.w.objects
            .get_mut(MONSTER)
            .unwrap()
            .set_property(PropertyInt::Tolerance, Tolerance::NoAttack.0);
        player_monster::on_attack_monster(&mut h.w, PLAYER, Some(MONSTER));
        assert!(
            !monster_awareness::is_awake(&h.w, MONSTER),
            "Tolerance.NoAttack"
        );

        h.w.objects
            .get_mut(MONSTER)
            .unwrap()
            .remove_property(PropertyInt::Tolerance);
        player_monster::on_attack_monster(&mut h.w, PLAYER, None);
        assert!(!monster_awareness::is_awake(&h.w, MONSTER));
        player_monster::on_attack_monster(&mut h.w, PLAYER, Some(MONSTER));
        assert!(monster_awareness::is_awake(&h.w, MONSTER));
        assert_eq!(monster_combat::attack_target(&h.w, MONSTER), Some(PLAYER));
    }
}

mod nearby_players {
    use crate::support::navigation_world::*;

    /// `WorldObject.NotifyPlayers` for a monster checks its targets (`Creature.CheckTargets`): 0.75 s
    /// later the player beside it alerts it. The monster enters first (its landblock's own
    /// `NotifyPlayers` finds no one), then the player arrives, then the call. (`content::quests::kill_tasks_and_portals`
    /// covers the landblock's own call.)
    #[test]
    fn notify_players_makes_a_monster_check_its_targets() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        h.run(1.0);
        h.player(100.0, 110.0);
        h.run(1.0);
        assert!(
            !monster_awareness::is_awake(&h.w, MONSTER),
            "nobody checked the targets yet"
        );
        empyrean_world::world_objects::world_object_networking::notify_players(&mut h.w, MONSTER);
        assert!(
            h.run_until(1.0, |h| monster_awareness::is_awake(&h.w, MONSTER)),
            "alerted by NotifyPlayers' CheckTargets"
        );

        // `IsVisibleTarget` reads the monster's VisibleTargets (a player's is not maintained)
        assert!(wo::is_visible_target(&h.w, MONSTER, PLAYER));
        assert!(!wo::is_visible_target(&h.w, PLAYER, MONSTER));
    }
}

mod entering_awareness {
    use crate::support::quest_world::*;

    /// `Landblock.AddWorldObjectInternal`'s `wo.NotifyPlayers()`: a monster that enters beside a player
    /// checks its targets by itself and is alerted 0.75 s later (no direct `CheckTargets` call).
    #[test]
    fn a_monster_entering_beside_a_player_notices_it_on_its_own() {
        let mut h = Arena::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.player(100.0, 110.0);
        h.monster(100.0, 100.0);
        assert!(!monster_awareness::is_awake(&h.w, MONSTER));
        h.run(0.5);
        assert!(
            !monster_awareness::is_awake(&h.w, MONSTER),
            "CheckTargets_Inner waits 0.75 s"
        );
        assert!(
            h.run_until(1.0, |h| monster_awareness::is_awake(&h.w, MONSTER)),
            "alerted through NotifyPlayers"
        );
    }
}
