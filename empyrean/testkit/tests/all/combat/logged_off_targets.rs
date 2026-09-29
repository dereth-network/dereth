//! ACE: Source/ACE.Server/WorldObjects/Monster_Combat.cs::AttackTarget
//! A missile/spell aimed at a logged-off player reads the body's cell-zero frame and last cached
//! velocity as ACE did.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

// V325.

use dereth_primitives::Vec3;
use empyrean_common::dotnet::numerics::Vector3;
use empyrean_entity::Position;
use empyrean_testkit::TestServer;
use empyrean_world::entity::position_extensions;
use empyrean_world::entity::spell_projectile_info::SpellProjectileInfo;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::world_object::{ProjectileLinks, WorldObject};
use empyrean_world::world_objects::{creature_missile, player};

use super::monster_ai::{arena, MONSTER, PLAYER};

/// The player's body moves at this (its last update's `CachedVelocity`) when the log-off ends.
const DRIFT: Vec3 = Vec3 {
    x: 1.5,
    y: -0.5,
    z: 0.0,
};

/// The arena, the player's body drifting, then its log-off finished: held, but off its landblock.
fn logged_off() -> TestServer {
    let (mut ts, _client) = arena();
    let h = phys_ext::physics_obj(&ts.world, PLAYER).expect("the player's body");
    ts.world.physics.get_mut(h).expect("a body").cached_velocity = DRIFT;
    ts
}

fn finish_logout(ts: &mut TestServer) {
    player::finalize_logout(&mut ts.world, PLAYER);
    assert!(ts.world.objects.get(PLAYER).is_some(), "still held");
    assert!(
        phys_ext::physics_obj(&ts.world, PLAYER).is_none(),
        "no body"
    );
}

fn close(a: Vector3, b: Vector3) -> bool {
    (a - b).length() < 1e-3
}

/// `Creature.CalculateProjectileVelocity` on its own aims at the destroyed body's frame in
/// landblock 0: the shot would cross landblocks and head for block 0,0. The launches check
/// `target_left_world` first and fire nothing (V325).
#[test]
fn a_missile_at_a_logged_off_player_aims_at_the_bodys_cell_zero_frame() {
    let mut ts = logged_off();
    let body =
        phys_ext::position(&ts.world, phys_ext::physics_obj(&ts.world, PLAYER).unwrap()).unwrap();
    assert!(!creature_missile::target_left_world(&ts.world, PLAYER));
    finish_logout(&mut ts);
    assert!(
        creature_missile::target_left_world(&ts.world, PLAYER),
        "the launches fire nothing at it"
    );

    let (_velocity, _origin, rotation) = creature_missile::calculate_projectile_velocity(
        &mut ts.world,
        MONSTER,
        Vector3::ZERO,
        PLAYER,
        20.0,
    );

    let monster = ts
        .world
        .objects
        .get(MONSTER)
        .and_then(WorldObject::location)
        .expect("the monster's spot");
    let o = body.frame.origin;
    let cell_zero = Position::from_vectors(
        0,
        Vector3::new(o.x, o.y, o.z),
        empyrean_common::dotnet::numerics::Quaternion::IDENTITY,
    );
    let dir = Vector3::normalize(
        position_extensions::to_global(&cell_zero, false)
            - position_extensions::to_global(&monster, false),
    );
    let heading = Vector3::transform(Vector3::UNIT_Y, rotation);
    assert!(
        close(
            Vector3::new(heading.x, heading.y, 0.0),
            Vector3::normalize(Vector3::new(dir.x, dir.y, 0.0))
        ),
        "{heading:?} vs {dir:?}"
    );
    assert!(
        heading.x < 0.0 && heading.y < 0.0,
        "towards block 0,0 from 0xA9B4"
    );
}

/// `Creature.GetProjectileVelocity` leads the target by its destroyed body's cached velocity,
/// exactly as it did while the body was in the world.
#[test]
fn a_missile_at_a_logged_off_player_leads_by_its_last_cached_velocity() {
    let mut ts = logged_off();
    let (origin, dest) = (
        Vector3::new(100.0, 100.0, 21.0),
        Vector3::new(100.0, 112.0, 21.0),
    );
    let dir = Vector3::normalize(dest - origin);
    let before = creature_missile::get_projectile_velocity(
        &ts.world, MONSTER, PLAYER, origin, dir, dest, 20.0, true,
    );
    finish_logout(&mut ts);
    let after = creature_missile::get_projectile_velocity(
        &ts.world, MONSTER, PLAYER, origin, dir, dest, 20.0, true,
    );
    assert_eq!(after, before);
    let still = creature_missile::get_projectile_velocity(
        &ts.world, MONSTER, MONSTER, origin, dir, dest, 20.0, true,
    );
    assert_ne!(after.0, still.0, "a moving target is led");
}

/// `SpellProjectileInfo` reads its target's `PhysicsObj.CachedVelocity`: the destroyed body's.
/// (ACE never builds one; the projectile here is any object with a body and projectile links.)
#[test]
fn spell_projectile_info_reads_a_logged_off_targets_last_cached_velocity() {
    let mut ts = logged_off();
    finish_logout(&mut ts);
    let w = &mut ts.world;
    let at = w
        .objects
        .get(MONSTER)
        .and_then(WorldObject::location)
        .expect("the monster's spot");
    let bolt = empyrean_entity::ObjectGuid::new(0x8000_0301);
    let mut p = WorldObject::allocate(empyrean_world::dispatch::Class::Missile);
    p.guid = bolt;
    p.biota.id = bolt.full();
    p.biota.properties_enchantment_registry = Some(Vec::new());
    p.set_property(
        empyrean_entity::enums::PropertyDataId::Setup,
        empyrean_testkit::land::TEST_SETUP,
    );
    p.projectile = Some(ProjectileLinks {
        source: Some(MONSTER),
        target: Some(PLAYER),
        ..ProjectileLinks::default()
    });
    p.set_location(Some(at));
    w.objects.insert(p).expect("fresh guid");
    assert!(
        lm::add_object(w, bolt, false),
        "the projectile is in flight"
    );
    let info = SpellProjectileInfo::new(w, bolt);
    assert_eq!(
        info.cached_velocity,
        Some(empyrean_entity::Vector3::new(DRIFT.x, DRIFT.y, DRIFT.z))
    );
}

mod queued_attacks {
    //! ACE: Source/ACE.Server/WorldObjects/Monster_Combat.cs::AttackTarget
    use empyrean_entity::enums::{
        DamageType, PropertyDataId, PropertyInt, PropertyString, WeenieType,
    };
    use empyrean_entity::ObjectGuid;
    use empyrean_testkit::{land, TestServer};
    use empyrean_world::dispatch::Class;
    use empyrean_world::entity::damage_event::DamageEvent;
    use empyrean_world::managers::landblock_manager as lm;
    use empyrean_world::physics::phys_ext;
    use empyrean_world::world_objects::world_object::{self, ProjectileLinks, WorldObject};
    use empyrean_world::world_objects::{monster_combat, player, projectile_collision_helper};
    use empyrean_world::World;

    use crate::combat::monster_ai::{arena, MONSTER, PLAYER};

    /// The `DefenderNotification` (a hit) and `EvasionDefenderNotification` (an evade) game events
    /// the client has received so far.
    fn strikes_told(ts: &TestServer, client: empyrean_testkit::ClientId) -> usize {
        ts.received_raw(client)
            .iter()
            .filter(|m| m.opcode == 0xF7B0)
            .filter(|m| {
                matches!(
                    u32::from_le_bytes(m.body[8..12].try_into().expect("an event type")),
                    0x01B2 | 0x01B4
                )
            })
            .count()
    }

    /// The monster's strike is queued, then the player's log-off finishes (`FinalizeLogout`) before
    /// the strike's frame: the strike still runs `CalculateDamage` on the player, which is off its
    /// landblock with no physics body.
    #[test]
    fn a_strike_queued_before_the_targets_log_off_finishes_still_resolves() {
        let (mut ts, client) = arena();
        let _ = TestServer::take_not_ported();

        // the monster wakes, closes in and starts a swing: its strike is queued
        assert!(
            ts.run_until(20.0, |ts| monster_combat::fields(&ts.world, MONSTER)
                .prev_attack_time
                > 0.0),
            "a swing starts"
        );
        let told_before = strikes_told(&ts, client);

        // the log-off ends before the strike lands
        // (a quarter second into the swing: before its strike frame, and before the monster's next
        // tick sees the target gone and looks for another)
        ts.advance(0.25);
        assert_eq!(
            strikes_told(&ts, client),
            told_before,
            "the strike has not landed yet"
        );
        player::finalize_logout(&mut ts.world, PLAYER);
        assert!(
            ts.world.objects.get(PLAYER).is_some(),
            "still held (the session has not let go)"
        );
        assert!(
            phys_ext::physics_obj(&ts.world, PLAYER).is_none(),
            "off its landblock: no body"
        );

        assert!(
            ts.run_until(0.3, |ts| strikes_told(ts, client) > told_before),
            "the strike resolves: the player is told of a hit (DefenderNotification) or an evade"
        );
    }

    /// A thrown weapon: what a monster wields and a projectile carries as its `ProjectileAmmo`.
    const DART: ObjectGuid = ObjectGuid::new(0x8000_0200);
    const PROJECTILE: ObjectGuid = ObjectGuid::new(0x8000_0201);

    fn thing(
        w: &mut World,
        class: Class,
        guid: ObjectGuid,
        weenie_type: WeenieType,
    ) -> &mut WorldObject {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.weenie_type = weenie_type;
        o.biota.properties_enchantment_registry = Some(Vec::new());
        o.set_property(PropertyString::Name, "Dart".to_owned());
        o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
        w.objects.insert(o).expect("fresh guid");
        w.objects.get_mut(guid).expect("inserted")
    }

    /// The monster's thrown dart flies at the player; before it hits, the dart it came from is
    /// destroyed (as `SwitchToMeleeAttack` destroys a monster's missile weapon). ACE's projectile
    /// still holds it as `ProjectileAmmo`, and the hit's `DamageEvent` reads it as the weapon. Once the projectile
    /// leaves the world, nothing holds the dart and it leaves the store.
    #[test]
    fn a_projectile_whose_weapon_was_destroyed_in_flight_still_hits_with_it() {
        let (mut ts, client) = arena();
        assert!(
            ts.run_until(1.0, |ts| monster_combat::attack_target(&ts.world, MONSTER)
                == Some(PLAYER)),
            "the monster targets the player"
        );
        let w = &mut ts.world;
        let at = w
            .objects
            .get(MONSTER)
            .and_then(WorldObject::location)
            .expect("the monster's spot");

        thing(w, Class::Missile, DART, WeenieType::Missile)
            .set_property(PropertyInt::DamageType, DamageType::Pierce.0);
        let p = thing(w, Class::Missile, PROJECTILE, WeenieType::Missile);
        p.projectile = Some(ProjectileLinks {
            source: Some(MONSTER),
            target: Some(PLAYER),
            launcher: None,
            ammo: Some(DART),
        });
        let mut spot = at;
        spot.position_y += 6.0;
        p.set_location(Some(spot));
        assert!(
            lm::add_object(w, PROJECTILE, false),
            "the projectile is in flight on the landblock"
        );
        let told_before = strikes_told(&ts, client);

        world_object::destroy(&mut ts.world, DART, true, false);
        assert!(
            ts.world
                .objects
                .get(DART)
                .is_some_and(|d| d.wo.world_object.is_destroyed),
            "kept for the projectile, destroyed"
        );

        let event = DamageEvent::calculate_damage(
            &mut ts.world,
            MONSTER,
            PLAYER,
            Some(PROJECTILE),
            None,
            None,
        );
        assert_eq!(event.weapon, Some(DART), "the destroyed dart is the weapon");

        projectile_collision_helper::on_collide_object(&mut ts.world, PROJECTILE, PLAYER);
        ts.advance(0.1);
        assert!(
            strikes_told(&ts, client) > told_before,
            "the hit resolves: the player is told of a hit or an evade"
        );
        assert!(
            ts.world.objects.get(DART).is_none(),
            "the projectile left the world: the dart is let go"
        );
    }
}
