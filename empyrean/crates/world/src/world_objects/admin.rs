// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Admin.cs
//! Port of `Source/ACE.Server/WorldObjects/Admin.cs`.

/// Non-property fields declared in `Admin.cs`.
#[derive(Debug, Default)]
pub struct AdminFields {}

// ---- constructors and SetEphemeralValues ----

/// Admin's two constructors: the `Sentinel` constructor, then Admin's `SetEphemeralValues`.
// ACE: Admin.Admin
pub fn admin_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
    args: crate::world_objects::player::PlayerCtorArgs,
) {
    crate::world_objects::sentinel::sentinel_ctor(o, env, src, args);
    admin_set_ephemeral_values(o);
}

// ACE: Admin.SetEphemeralValues
fn admin_set_ephemeral_values(o: &mut crate::world_objects::world_object::WorldObject) {
    use empyrean_entity::enums::Channel;

    //BaseDescriptionFlags |= ObjectDescriptionFlag.Admin;

    let channels = Channel::QA1 | Channel::QA2 | Channel::ValidChans;
    match o.channels_allowed() {
        None => o.set_channels_allowed(Some(channels)),
        Some(allowed) => o.set_channels_allowed(Some(allowed | channels)),
    }
}
