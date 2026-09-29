// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Common/WeenieObject.cs
//! Port of `Source/ACE.Server/Physics/Common/WeenieObject.cs`.
//!
//! The glue between a physics body and its `WorldObject`. ACE hangs one on each `PhysicsObj`; here
//! it lives in the body's server-side record (`phys_ext`), and it names its world object by guid
//! (ACE's `WorldObjectInfo` is a weak reference, resolved on use).
//!
//! ACE's physics calls `DoCollision` synchronously inside a transition. The shared `PhysicsWorld`
//! reports collisions as `PhysicsNotice`s instead, which `phys_ext` drains right after each body's
//! update and routes here (V6).

use dereth_rules::burden as encumbrance_system;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    CreatureType, FactionBits, ItemType, PlayerKillerStatus, Skill, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// ACE's `WeenieObject`. `Default` is ACE's `DummyObject` (no world object, every flag clear).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeenieObject {
    /// The world object, by guid; resolve with [`WeenieObject::world_object`].
    // ACE: WeenieObject.WorldObjectInfo
    pub world_object_info: Option<ObjectGuid>,
    // ACE: WeenieObject.IsPlayer
    pub is_player: bool,
    // ACE: WeenieObject.IsCreature
    pub is_creature: bool,
    // ACE: WeenieObject.IsStorage
    pub is_storage: bool,
    // ACE: WeenieObject.IsCorpse
    pub is_corpse: bool,
    // ACE: WeenieObject.IsMonster
    pub is_monster: bool,
    // ACE: WeenieObject.IsCombatPet
    pub is_combat_pet: bool,
    // ACE: WeenieObject.IsFactionMob
    pub is_faction_mob: bool,
    // ACE: WeenieObject.Faction1Bits
    pub faction1_bits: FactionBits,
    // ACE: WeenieObject.FoeType
    pub foe_type: Option<CreatureType>,
    // ACE: WeenieObject.PlayerKillerStatus
    pub player_killer_status: PlayerKillerStatus,
}

/// ACE's `ObjCollisionProfile` as `InqCollisionProfile` fills it: the item type and the flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjCollisionProfile {
    pub item_type: ItemType,
    pub flags: u32,
}

/// `ObjCollisionProfileFlags` bits `InqCollisionProfile` sets.
pub mod obj_collision_profile_flags {
    pub const CREATURE: u32 = 0x1;
    pub const PLAYER: u32 = 0x2;
    pub const ATTACKABLE: u32 = 0x4;
    pub const DOOR: u32 = 0x40;
}

/// `Creature.IsMonster`, the value `Monster.SetMonsterState` caches at construction.
fn creature_is_monster(w: &World, guid: ObjectGuid) -> bool {
    crate::world_objects::monster::fields(w, guid).is_monster
}

impl WeenieObject {
    /// ACE's `WeenieObject(WorldObject worldObject)`. A guid that names no object gives a record
    /// with only the reference set, which resolves to nothing.
    // ACE: WeenieObject.WeenieObject
    #[must_use]
    pub fn new(w: &World, world_object: ObjectGuid) -> Self {
        let mut r = WeenieObject {
            world_object_info: Some(world_object),
            ..Default::default()
        };
        let Some(wo) = w.objects.get(world_object) else {
            return r;
        };

        if !wo.is_creature() {
            if wo.is_corpse() {
                r.is_corpse = true;
            } else if wo.is_storage() {
                r.is_storage = true;
            }
            return r;
        }

        r.is_creature = true;

        if wo.is_player() {
            r.is_player = true;
        } else if wo.is_combat_pet() {
            r.is_combat_pet = true;
        } else if creature_is_monster(w, world_object) {
            r.is_monster = true;
        }

        r.faction1_bits = wo.faction1_bits().unwrap_or(FactionBits::None);

        r.is_faction_mob = r.is_monster && r.faction1_bits != FactionBits::None;

        r.foe_type = wo.foe_type();

        r.player_killer_status = wo.player_killer_status();
        r
    }

    /// The world object, if it is still alive (`WorldObjectInfo.TryGetWorldObject()`).
    // ACE: WeenieObject.WorldObject
    #[must_use]
    pub fn world_object(&self, w: &World) -> Option<ObjectGuid> {
        self.world_object_info.filter(|g| w.objects.contains(*g))
    }

    /// `(Faction1Bits & obj.WeenieObj.Faction1Bits) != 0`.
    // ACE: WeenieObject.SameFaction
    #[must_use]
    pub fn same_faction(&self, obj: &WeenieObject) -> bool {
        (self.faction1_bits.0 & obj.faction1_bits.0) != 0
    }

    /// Either side names the other's creature type as its foe.
    // ACE: WeenieObject.PotentialFoe
    #[must_use]
    pub fn potential_foe(&self, w: &World, obj: &WeenieObject) -> bool {
        let creature_type = |wo: &WeenieObject| {
            wo.world_object(w)
                .and_then(|g| w.objects.get(g))
                .and_then(|o| o.creature_type())
        };
        self.foe_type.is_some() && self.foe_type == creature_type(obj)
            || obj.foe_type.is_some() && obj.foe_type == creature_type(self)
    }

    // ACE: WeenieObject.CanJump
    #[must_use]
    pub fn can_jump(&self, _extent: f32) -> bool {
        true
    }

    /// `InqJumpVelocity(extent, out velocity_z)`: `(answered, velocity_z)`, from the player's
    /// burden, stamina and Jump skill (`GetCreatureSkill` adds a missing Jump record, as ACE's
    /// does, so this takes the world mutably; the skill reads the caching `EnchantmentManager`).
    // ACE: WeenieObject.InqJumpVelocity
    #[must_use]
    pub fn inq_jump_velocity(&self, w: &mut World, extent: f32) -> (bool, f32) {
        let mut velocity_z = 0.0f32;

        if !self.is_player {
            return (false, velocity_z);
        }

        // `WorldObject as Player`
        let Some(player) = self
            .world_object(w)
            .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player))
        else {
            return (false, velocity_z);
        };

        let Some(burden) = self.inq_burden(w) else {
            return (false, velocity_z);
        };

        let stamina = obj(w, player).stamina().current(obj(w, player));

        let jump = obj_mut(w, player)
            .get_creature_skill(Skill::Jump, true)
            .expect("GetCreatureSkill(add: true)");
        let mut jump_skill = jump.current(w, player);

        if stamina == 0 {
            jump_skill = 0;
        }

        // Retail's speed (V230): ACE's `GetJumpHeight` then `Math.Sqrt(height * 19.6)`
        // rounds the height to float first; the shared rule takes the root of the unrounded height.
        #[allow(clippy::cast_precision_loss)]
        let skill = jump_skill as f32; // ACE's skill is unsigned
        velocity_z = dereth_rules::movement::jump_velocity_of(burden, skill, extent, 1.0);

        (true, velocity_z)
    }

    /// Returns the player's load / burden as a percentage, usually 0.0 - 3.0; `None` for a
    /// non-player. `Strength.Current` reads the caching `EnchantmentManager`.
    // ACE: WeenieObject.InqBurden
    #[must_use]
    pub fn inq_burden(&self, w: &mut World) -> Option<f32> {
        if !self.is_player {
            return None;
        }

        // `WorldObject as Player`
        let player = self
            .world_object(w)
            .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player))?;

        let strength_attribute = obj(w, player).strength();
        let strength: i32 = strength_attribute
            .current(&mut StatCtx::in_world(w, player))
            .cs_cast();

        let num_augs = obj(w, player).augmentation_increased_carrying_capacity();

        let capacity = encumbrance_system_encumbrance_capacity(strength, num_augs);

        let encumbrance = obj(w, player).encumbrance_val().unwrap_or(0);

        let burden = encumbrance_system::load(capacity, encumbrance);

        Some(burden)
    }

    /// `InqRunRate(ref rate)`: `(answered, rate)`, the run rate the physics motion interpreter moves
    /// the creature at between its position updates. For a creature it is the creature's own run
    /// rate, burden included ([`crate::world_objects::monster_navigation::get_run_rate`]); a weenie
    /// object with no creature runs at skill 0 and no burden. `GetCreatureSkill(Skill.Run)` adds a
    /// missing Run record, as ACE's does, so this takes the world mutably.
    // ACE: WeenieObject.InqRunRate
    // Not ACE's (a fix, V332): ACE's asks for the rate at burden 0, so the server's
    // own simulation ran an encumbered character at unencumbered speed; here the creature's real
    // burden applies, exactly as in the run rate the creature reports. Other players' view is not
    // affected either way: the server relays the client's motion state, and an encumbered player
    // is seen running slower on a second client. This corrects only the server's internal
    // simulation.
    #[must_use]
    pub fn inq_run_rate(&self, w: &mut World) -> (bool, f32) {
        if let Some(creature) = self
            .world_object(w)
            .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature))
        {
            return (
                true,
                crate::world_objects::monster_navigation::get_run_rate(w, creature),
            );
        }

        //rate = (float)MovementSystem.GetRunRate(0.0f, 300, 1.0f);
        let rate: f32 = if self.is_player {
            player_run_rate(0.0, 0).cs_cast()
        } else {
            movement_system_get_run_rate(0.0, 0, 1.0).cs_cast()
        };
        (true, rate)
    }

    // ACE: WeenieObject.IsImpenetrable
    #[must_use]
    pub fn is_impenetrable(&self, w: &World) -> bool {
        self.is_player
            && self
                .player(w)
                .is_some_and(|s| s == PlayerKillerStatus::Free)
    }

    // ACE: WeenieObject.IsPK
    #[must_use]
    pub fn is_pk(&self, w: &World) -> bool {
        // Player.IsPK => PlayerKillerStatus == PlayerKillerStatus.PK
        self.is_player && self.player(w).is_some_and(|s| s == PlayerKillerStatus::PK)
    }

    // ACE: WeenieObject.IsPKLite
    #[must_use]
    pub fn is_pk_lite(&self, w: &World) -> bool {
        // Player.IsPKL => PlayerKillerStatus == PlayerKillerStatus.PKLite
        self.is_player
            && self
                .player(w)
                .is_some_and(|s| s == PlayerKillerStatus::PKLite)
    }

    /// `WorldObject is Player player`: the player's current PlayerKillerStatus.
    fn player(&self, w: &World) -> Option<PlayerKillerStatus> {
        let o = w.objects.get(self.world_object(w)?)?;
        o.is_player().then(|| o.player_killer_status())
    }

    // ACE: WeenieObject.JumpStaminaCost
    #[must_use]
    pub fn jump_stamina_cost(&self, _extent: f32, _stamina_cost: i32) -> f32 {
        0.0
    }

    /// Fills the collision profile: item type, creature/player/door and attackable flags.
    // ACE: WeenieObject.InqCollisionProfile
    pub fn inq_collision_profile(&self, w: &World, prof: &mut ObjCollisionProfile) {
        let Some(wo) = self.world_object(w).and_then(|g| w.objects.get(g)) else {
            return;
        };

        prof.item_type = wo.item_type();

        if self.is_creature {
            prof.flags |= obj_collision_profile_flags::CREATURE;

            if self.is_player {
                prof.flags |= obj_collision_profile_flags::PLAYER;
            }
        } else if wo.is_door() {
            prof.flags |= obj_collision_profile_flags::DOOR;
        }

        if wo.attackable() {
            prof.flags |= obj_collision_profile_flags::ATTACKABLE;
        }
    }

    /// `DoCollision(ObjCollisionProfile prof, ObjectGuid guid, PhysicsObj target)`: the object
    /// collided with `target`'s world object. Answers -1 when either world object is gone or they
    /// are the same object, else 0.
    // ACE: WeenieObject.DoCollision
    pub fn do_collision_object(&self, w: &mut World, target: &WeenieObject) -> i32 {
        let Some(wo) = self.world_object(w) else {
            return -1;
        };

        let Some(target_wo) = target.world_object(w) else {
            return -1;
        };

        // no collision with self
        if wo == target_wo {
            return -1;
        }

        dispatch::on_collide_object::on_collide_object(w, wo, target_wo);

        0
    }

    /// `DoCollision(EnvCollisionProfile prof, ObjectGuid guid, PhysicsObj target)`: a player takes
    /// falling damage from the profile; anything else runs `OnCollideEnvironment`.
    // ACE: WeenieObject.DoCollision
    pub fn do_collision_environment(&self, w: &mut World) -> i32 {
        let Some(wo) = self.world_object(w) else {
            return 0;
        };

        let is_player_wo = w.objects.get(wo).is_some_and(|o| o.is_player());
        if self.is_player && is_player_wo {
            crate::world_objects::player_move::handle_falling_damage(w, wo);
        } else {
            dispatch::on_collide_environment::on_collide_environment(w, wo);
        }

        0
    }

    /// The collision with `target_guid` ended: `OnCollideObjectEnd` if the target is found from
    /// this object's landblock.
    // ACE: WeenieObject.DoCollisionEnd
    pub fn do_collision_end(&self, w: &mut World, target_guid: ObjectGuid) {
        let Some(wo) = self.world_object(w) else {
            return;
        };

        let Some(target) = landblock_get_object(w, wo, target_guid) else {
            return;
        };

        dispatch::on_collide_object_end::on_collide_object_end(w, wo, target);
    }

    // ACE: WeenieObject.OnMotionDone
    pub fn on_motion_done(&self, w: &mut World, motion_id: u32, success: bool) {
        // ACE dereferences WorldObject unguarded; a gone object is its NullReferenceException.
        let wo = self
            .world_object(w)
            .expect("WeenieObject.OnMotionDone: the world object is gone");
        dispatch::handle_motion_done::handle_motion_done(w, wo, motion_id, success);
    }

    // ACE: WeenieObject.OnMoveComplete
    pub fn on_move_complete(&self, w: &mut World, status: WeenieError) {
        let wo = self
            .world_object(w)
            .expect("WeenieObject.OnMoveComplete: the world object is gone");
        dispatch::on_move_complete::on_move_complete(w, wo, status);
    }

    /// acclient checks both IgnoreHouseBarriers and Admin here; ACE only the first.
    // ACE: WeenieObject.CanBypassMoveRestrictions
    #[must_use]
    pub fn can_bypass_move_restrictions(&self, w: &World) -> bool {
        let wo = self.world_object(w).and_then(|g| w.objects.get(g));
        wo.expect("WeenieObject.CanBypassMoveRestrictions: the world object is gone")
            .ignore_house_barriers()
    }

    /// Whether `mover` may enter this house's cells.
    // ACE: WeenieObject.CanMoveInto
    #[must_use]
    pub fn can_move_into(&self, w: &World, mover: &WeenieObject) -> bool {
        let wo = self.world_object(w);
        let describe = |g: Option<ObjectGuid>| {
            g.map(|g| {
                format!(
                    "{} ({})",
                    w.objects
                        .get(g)
                        .and_then(|o| o.get_property(empyrean_entity::enums::PropertyString::Name))
                        .unwrap_or_default(),
                    g
                )
            })
            .unwrap_or_else(|| " ()".to_owned())
        };
        let Some(house) = wo.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_house))
        else {
            log::error!(
                "{}.CanMoveInto({} - couldn't find house",
                describe(wo),
                describe(mover.world_object(w))
            );
            return true;
        };
        // `house.RootHouse`: the loaded root or its offline copy (see `House.cs`'s port)
        let root_house = crate::world_objects::house::root_house_ref(w, house);
        let Some(player) = mover
            .world_object(w)
            .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player))
        else {
            log::error!(
                "{}.CanMoveInto({} - couldn't find player",
                describe(wo),
                describe(mover.world_object(w))
            );
            return true;
        };
        let r = w.objects.get(root_house).expect("present");
        //Console.WriteLine($"{player.Name} can move into {rootHouse.Name} ({rootHouse.Guid}): {result}");
        r.house_owner().is_none()
            || r.open_to_everyone()
            || crate::world_objects::house::has_permission(w, root_house, player, false)
    }
}

/// `wo.CurrentLandblock?.GetObject(targetGuid)` (`Landblock.GetObject`, searching adjacents): the
/// target, if it is on `wo`'s landblock or a loaded adjacent one. A target gone from the store
/// is ACE's destroyed object still listed until the landblock's next pass: `None` here.
fn landblock_get_object(w: &World, wo: ObjectGuid, target_guid: ObjectGuid) -> Option<ObjectGuid> {
    let lb = w.objects.get(wo)?.current_landblock?;
    w.landblock_manager.landblocks.get(lb)?;
    crate::entity::landblock::get_object(w, lb, target_guid, true)
        .filter(|&g| w.objects.contains(g))
}

// ---------------------------------------------------------------------------------------------
// The static helpers these inquiries call (ACE's physics statics `MovementSystem` and
// `EncumbranceSystem`). `EncumbranceSystem`'s load and load modifier are the shared rules'
// (`dereth_rules::burden`, pinned to ACE by its vectors). Its capacity stays ACE's: ACE's
// arithmetic wraps where the client's saturates, which ACE's vectors reach with augmentation
// counts past 71 million (a player can buy 5). `MovementSystem`'s run rate and jump height stay
// ACE's (the run rate differs from the client's above 800 Run).
// ---------------------------------------------------------------------------------------------

fn obj(w: &World, guid: ObjectGuid) -> &WorldObject {
    w.objects
        .get(guid)
        .unwrap_or_else(|| panic!("WeenieObject: {guid:?} is not in the world"))
}

fn obj_mut(w: &mut World, guid: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(guid)
        .unwrap_or_else(|| panic!("WeenieObject: {guid:?} is not in the world"))
}

/// The run rate for a Run skill (`double`, computed in `float`): 4.5 from skill 800 up.
///
/// Below 800 this is the one shared formula, `dereth_rules::movement::get_run_rate` (ACE's
/// `loadMod * (s / (s + 200) * 11)` is the same product in the other order, and `float`
/// multiplication commutes, so the bits are ACE's). What stays ACE's is the cap: 4.5 for **every**
/// skill from 800 up, where the client pays 4.5 only at exactly 800. Players no longer come here
/// ([`player_run_rate`]); creatures still do.
// ACE: MovementSystem.GetRunRate
pub fn movement_system_get_run_rate(burden: f32, run_skill: i32, scaling: f32) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let skill = run_skill as f32;
    if skill >= 800.0f32 {
        // max run speed?
        f64::from(18.0f32 / 4.0f32)
    } else {
        f64::from(dereth_rules::movement::get_run_rate(
            burden, run_skill, scaling,
        ))
    }
}

/// A **player's** run rate: the client's formula, not ACE's (retail).
///
/// ACE returns 4.5 for any Run skill of 800 or more. The client returns 4.5 only at exactly 800
/// (a float equality) and otherwise the general expression, which tops out near 3.7. A player's
/// client computes its own run rate that way, so for a player the server now agrees with it: a
/// server-driven MoveTo of a player with Run above 800 no longer outruns what the client expects.
/// Creatures keep ACE's [`movement_system_get_run_rate`] (the
/// client has no creature qualities; it uses the rate the server sends).
#[must_use]
pub fn player_run_rate(burden: f32, run_skill: i32) -> f64 {
    f64::from(dereth_rules::skills::get_run_rate(burden, run_skill, 1.0))
}

/// The jump height for a burden, Jump skill and power (0..1), at least 0.35.
// ACE: MovementSystem.GetJumpHeight
pub fn movement_system_get_jump_height(
    burden: f32,
    jump_skill: u32,
    power: f32,
    scaling: f32,
) -> f32 {
    // The one shared formula, `dereth_rules::movement::get_jump_height` (retail, V230):
    // ACE's `loadMod * (s / (s + 1300) * 22.2 + 0.05) * power / scaling` rounds each step to float;
    // retail's evaluates the product at double precision and rounds once, and a NaN power gives the
    // 0.35 floor where ACE's returns NaN.
    #[allow(clippy::cast_precision_loss)]
    let skill = jump_skill as f32; // ACE's skill is unsigned
    dereth_rules::movement::jump_height_of(burden, skill, power, scaling)
}

/// The burden units a strength (and carrying-capacity augmentations) can carry.
///
/// **Retail's rule (V231):** the one shared implementation,
/// `dereth_rules::burden::encumbrance_capacity`, which saturates where ACE's `int` arithmetic
/// wrapped (unreachable: about 71 million augmentations or Strength above 14 million).
// ACE: EncumbranceSystem.EncumbranceCapacity
#[must_use]
pub fn encumbrance_system_encumbrance_capacity(strength: i32, num_augs: i32) -> i32 {
    dereth_rules::burden::encumbrance_capacity(strength, num_augs)
}
