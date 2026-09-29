// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Sentinel.cs
//! Port of `Source/ACE.Server/WorldObjects/Sentinel.cs`.

/// Non-property fields declared in `Sentinel.cs`.
#[derive(Debug, Default)]
pub struct SentinelFields {}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Sentinel.InitPhysicsObj
pub fn sentinel_init_physics_obj(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use crate::physics::phys_ext;
    use empyrean_entity::enums::{CloakStatus, PhysicsState, PropertyBool};

    crate::world_objects::player::player_init_physics_obj(w, this);

    let Some(cloak_status) = w.objects.get(this).map(|o| o.cloak_status()) else {
        return;
    };
    let on = cloak_status == CloakStatus::On;
    if on {
        //Translucency = 0.5f;
        phys_ext::set_physics_state(w, this, PhysicsState::Cloaked, Some(true));
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::Ethereal,
            PhysicsState::Ethereal,
            Some(true),
        );
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::NoDraw,
            PhysicsState::NoDraw,
            Some(true),
        );
        if let Some(o) = w.objects.get_mut(this) {
            o.set_visibility(true);
        }
        return;
    }

    if cloak_status == CloakStatus::Creature {
        if let Some(o) = w.objects.get_mut(this) {
            o.set_attackable(true);
        }
    }

    // default (Off, Player, and Creature after Attackable)
    if let Some(o) = w.objects.get_mut(this) {
        o.set_translucency(None);
    }
    phys_ext::set_physics_state(w, this, PhysicsState::Cloaked, Some(false));
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        Some(false),
    );
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::NoDraw,
        PhysicsState::NoDraw,
        Some(false),
    );
    if let Some(o) = w.objects.get_mut(this) {
        o.set_visibility(false);
    }
}

// ---- constructors and SetEphemeralValues ----

/// Sentinel's two constructors: the `Player` constructor, marking the character plussed, then
/// Sentinel's `SetEphemeralValues`.
// ACE: Sentinel.Sentinel
pub fn sentinel_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
    args: crate::world_objects::player::PlayerCtorArgs,
) {
    crate::world_objects::player::player_ctor(o, env, src, args);

    // A sentinel's character is always plussed; marking it so is a character change to save.
    let p = o.player.as_mut().expect("ACE: a Sentinel is a Player");
    let character = p
        .player
        .character
        .as_mut()
        .expect("ACE: Player.Character is null (NullReferenceException)");
    if !character.is_plussed {
        character.is_plussed = true;
        p.player_database.character_changes_detected = true;
    }

    sentinel_set_ephemeral_values(o);
}

// ACE: Sentinel.SetEphemeralValues
fn sentinel_set_ephemeral_values(o: &mut crate::world_objects::world_object::WorldObject) {
    use empyrean_entity::enums::Channel;

    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Admin;

    let channels = Channel::Audit
        | Channel::Advocate1
        | Channel::Advocate2
        | Channel::Advocate3
        | Channel::Sentinel
        | Channel::AllBroadcast;
    match o.channels_allowed() {
        None => o.set_channels_allowed(Some(channels)),
        Some(allowed) => o.set_channels_allowed(Some(allowed | channels)),
    }
}
