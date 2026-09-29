// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/AdvocateItem.cs
//! Port of `Source/ACE.Server/WorldObjects/AdvocateItem.cs`.

/// Non-property fields declared in `AdvocateItem.cs`.
#[derive(Debug, Default)]
pub struct AdvocateItemFields {}

// ---- virtual-dispatch targets ----

/// `creature.EnqueueBroadcast(true, new GameMessagePublicUpdatePropertyInt(creature, RadarBlipColor, value))`.
fn broadcast_radar_blip_color(
    w: &mut crate::World,
    creature: empyrean_entity::ObjectGuid,
    value: i32,
) {
    let o = w
        .objects
        .get_mut(creature)
        .expect("ACE: creature is null (NullReferenceException)");
    let msg = crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int(
        o,
        empyrean_entity::enums::PropertyInt::RadarBlipColor,
        value,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, creature, true, &[msg]);
}

// ACE: AdvocateItem.OnWield
pub fn advocate_item_on_wield(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
) {
    let o = w
        .objects
        .get_mut(creature)
        .expect("ACE: creature is null (NullReferenceException)");
    o.set_radar_color(Some(empyrean_entity::enums::RadarColor::Advocate));
    let radar_color = o.radar_color().map_or(0, |c| i32::from(c.0));
    broadcast_radar_blip_color(w, creature, radar_color);
    w.objects
        .get_mut(creature)
        .expect("ACE: creature is null")
        .set_property(empyrean_entity::enums::PropertyBool::AdvocateState, true);

    crate::world_objects::world_object_equipment::world_object_on_wield(w, this, creature);
}

// ACE: AdvocateItem.OnUnWield
pub fn advocate_item_on_un_wield(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
) {
    w.objects
        .get_mut(creature)
        .expect("ACE: creature is null (NullReferenceException)")
        .set_radar_color(None);
    broadcast_radar_blip_color(w, creature, 0);
    w.objects
        .get_mut(creature)
        .expect("ACE: creature is null")
        .set_property(empyrean_entity::enums::PropertyBool::AdvocateState, false);

    crate::world_objects::world_object_equipment::world_object_on_un_wield(w, this, creature);
}

// ---- constructors and SetEphemeralValues ----

/// `new AdvocateItem(weenie, guid)` / `new AdvocateItem(biota)`: the `GenericObject` constructor, then
/// AdvocateItem's `SetEphemeralValues`.
// ACE: AdvocateItem.AdvocateItem
pub fn advocate_item_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::generic_object::generic_object_ctor(o, env, src);
    advocate_item_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: AdvocateItem.SetEphemeralValues
fn advocate_item_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
