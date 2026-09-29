// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Networking.cs`.
//!
//! The object description (`CreateObject` / `UpdateObject`), the model data (`ObjDesc`), and the
//! broadcast helpers.
//!
//! Every C# expression `writer.Write(X ?? 0)` writes the type of `X` (C#'s `??` with a constant
//! takes the nullable's underlying type), so each write below uses the width of the ACE property:
//! for example `ItemCapacity` is a `byte?`, `CombatUse` an `sbyte`, `Structure` a `ushort?`.
//!
//! What this file needs from other systems (physics bodies, the object's motion state,
//! the player's `Character`, the property manager, the enchantment manager,
//! ...) goes through [`shims`], one function per ACE member, each either reading the data where it
//! already lives or a `not_ported!` pointer.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // C# casts

use dereth_protocol::types::PublicWeenieDesc;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    ChatMessageType, CloakStatus, HeritageGroup, HouseType, MotionCommand, MotionStance,
    ObjectDescriptionFlag, PhysicsDescriptionFlag, PhysicsState, Placement, PlayerKillerStatus,
    PropertyDataId, PropertyInt, PropertyType, WeenieHeaderFlag, WeenieHeaderFlag2, WeenieType,
};
use empyrean_entity::models::{PropertiesAnimPart, PropertiesPalette, PropertiesTextureMap};
use empyrean_entity::shared_types::wire_vec3;
use empyrean_entity::{ObjDesc, ObjectGuid, Vector3};
use empyrean_net::SessionId;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::network::game_messages::game_message::{
    self, ace_str, known_type_did, ushort_sequence, BinaryWriter, GameMessage,
};
use crate::network::game_messages::messages::{
    game_message_private_update_property_int, game_message_update_motion,
    game_message_update_position,
};
use crate::network::motion::movement_data::{self, Motion, MovementData};
use crate::network::sequence::sequence_type::SequenceType;
use crate::network::structure::restriction_db::{self, restriction_db_new};
use crate::physics::phys_ext;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_properties;
use crate::World;

/// Non-property fields declared in `WorldObject_Networking.cs`, and the
/// shim storage for data other files declare that this file needs before they are ported (see
/// [`shims`]).
#[derive(Debug, Default)]
pub struct WorldObjectNetworkingFields {
    // ACE: WorldObject.LastUpdatePosition
    pub last_update_position: DotNetDateTime,
}

// ================================================================================ serialisation

// ACE: WorldObject.SerializeUpdateObject
/// `SerializeUpdateObject(BinaryWriter writer, bool adminvision = false, bool changenodraw =
/// false)`: the same body as a create object ("content of these 2 is the same? TODO: Validate
/// that?").
#[allow(clippy::ptr_arg)]
pub fn world_object_serialize_update_object(
    w: &mut World,
    this: ObjectGuid,
    writer: &mut Vec<u8>,
    adminvision: bool,
    changenodraw: bool,
) {
    serialize_create_object(w, this, writer, false, adminvision, changenodraw);
}

// ACE: WorldObject.SerializeCreateObject
/// `SerializeCreateObject(BinaryWriter writer, bool adminvision = false, bool changenodraw =
/// false)`.
#[allow(clippy::ptr_arg)]
pub fn world_object_serialize_create_object(
    w: &mut World,
    this: ObjectGuid,
    writer: &mut Vec<u8>,
    adminvision: bool,
    changenodraw: bool,
) {
    serialize_create_object(w, this, writer, false, adminvision, changenodraw);
}

// ACE: WorldObject.SerializeGameDataOnly
/// `SerializeGameDataOnly(BinaryWriter writer, bool adminvision = false)`: the weenie description
/// without model and physics data.
#[allow(clippy::ptr_arg)]
pub fn world_object_serialize_game_data_only(
    w: &mut World,
    this: ObjectGuid,
    writer: &mut Vec<u8>,
    adminvision: bool,
) {
    serialize_create_object(w, this, writer, true, adminvision, false);
}

// ACE: WorldObject.SerializeUpdateModelData
/// The body of `GameMessage.ObjDescEvent`.
#[allow(clippy::ptr_arg)]
pub fn world_object_serialize_update_model_data(
    w: &mut World,
    this: ObjectGuid,
    writer: &mut Vec<u8>,
) {
    writer.write_guid(this);
    serialize_model_data(w, this, writer);
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    writer.write_bytes(
        &o.sequences
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    writer.write_bytes(
        &o.sequences
            .get_next_sequence(SequenceType::ObjectVisualDesc),
    );
}

// ACE: WorldObject.SerializeCreateObject
/// The private `SerializeCreateObject(writer, bool gamedataonly, bool adminvision, bool
/// changenodraw)`: guid, model and physics data (unless game data only), then the weenie header
/// and every optional field in ACE's order.
#[allow(clippy::too_many_lines)]
fn serialize_create_object(
    w: &mut World,
    this: ObjectGuid,
    writer: &mut Vec<u8>,
    gamedataonly: bool,
    adminvision: bool,
    changenodraw: bool,
) {
    writer.write_guid(this);

    if !gamedataonly {
        serialize_model_data(w, this, writer);
        serialize_physics_data(w, this, writer, adminvision, changenodraw);
    }

    let weenie_flags = calculate_weenie_header_flag(w, this);
    let weenie_flags2 = calculate_weenie_header_flag2(obj(w, this));

    update_object_description_flags(w, this);

    let mut obj_description_flags = obj(w, this).wo.world_object.object_description_flags;

    if adminvision {
        obj_description_flags &= !ObjectDescriptionFlag::UiHidden;
    }

    let name = dispatch::name::name(w, this);
    let o = obj(w, this);
    let mut desc = PublicWeenieDesc {
        header: weenie_flags.0,
        name: ace_str(name.as_deref()),
        wcid: o.biota.weenie_class_id,
        icon_id: known_type_did(o.icon_id(), 0x600_0000, this, "icon"),
        obj_type: o.item_type().0,
        bitfield: obj_description_flags.0.cast_unsigned(),
        ..PublicWeenieDesc::default()
    };

    if (obj_description_flags & ObjectDescriptionFlag::IncludesSecondHeader).0 != 0 {
        desc.header2 = Some(weenie_flags2.0);
    }

    let has = |f: WeenieHeaderFlag| (weenie_flags & f).0 != 0;

    let plural_name = o.plural_name();
    if has(WeenieHeaderFlag::PluralName) {
        desc.plural_name = Some(ace_str(plural_name.as_deref()));
    }

    if has(WeenieHeaderFlag::ItemsCapacity) {
        desc.items_capacity = Some(o.item_capacity().unwrap_or(0));
    }

    if has(WeenieHeaderFlag::ContainersCapacity) {
        desc.containers_capacity = Some(o.container_capacity().unwrap_or(0));
    }

    if has(WeenieHeaderFlag::AmmoType) {
        desc.ammo_type = Some(o.ammo_type().map_or(0, |a| a.0));
    }

    if has(WeenieHeaderFlag::Value) {
        desc.value = Some(o.value().unwrap_or(0).cast_unsigned());
    }

    if has(WeenieHeaderFlag::Usable) {
        desc.useability = Some(o.item_useable().map_or(0, |u| u.0));
    }

    if has(WeenieHeaderFlag::UseRadius) {
        desc.use_radius = Some(o.use_radius().unwrap_or(0.0));
    }

    if has(WeenieHeaderFlag::TargetType) {
        desc.target_type = Some(o.target_type().map_or(0, |t| t.0));
    }

    if has(WeenieHeaderFlag::UiEffects) {
        desc.effects = Some(o.ui_effects().map_or(0, |u| u.0));
    }

    if has(WeenieHeaderFlag::CombatUse) {
        desc.combat_use = Some(o.combat_use().map_or(0, |c| c.0));
    }

    if has(WeenieHeaderFlag::Structure) {
        desc.structure = Some(o.structure().unwrap_or(0));
    }

    if has(WeenieHeaderFlag::MaxStructure) {
        desc.max_structure = Some(o.max_structure().unwrap_or(0));
    }

    if has(WeenieHeaderFlag::StackSize) {
        desc.stack_size = Some(o.stack_size().map_or(0, |s| s as u16));
    }

    if has(WeenieHeaderFlag::MaxStackSize) {
        desc.max_stack_size = Some(o.max_stack_size().unwrap_or(0));
    }

    if has(WeenieHeaderFlag::Container) {
        desc.container_id = Some(dereth_primitives::ObjectId(o.container_id().unwrap_or(0)));
    }

    if has(WeenieHeaderFlag::Wielder) {
        desc.wielder_id = Some(dereth_primitives::ObjectId(o.wielder_id().unwrap_or(0)));
    }

    if has(WeenieHeaderFlag::ValidLocations) {
        desc.valid_locations = Some(o.valid_locations().map_or(0, |v| v.0));
    }

    if has(WeenieHeaderFlag::CurrentlyWieldedLocation) {
        desc.location = Some(o.current_wielded_location().map_or(0, |v| v.0));
    }

    if has(WeenieHeaderFlag::Priority) {
        desc.priority = Some(o.clothing_priority().map_or(0, |v| v.0));
    }

    if has(WeenieHeaderFlag::RadarBlipColor) {
        desc.blip_color = Some(o.radar_color().map_or(0, |v| v.0));
    }

    if has(WeenieHeaderFlag::RadarBehavior) {
        desc.radar_enum = Some(o.radar_behavior().map_or(0, |v| v.0));
    }

    // Not ACE's (retail, V334): the description's script is data id 44 (the
    // house restriction effect), the id the client mirrors into the description's script on an
    // update. Data id 30 goes only in the physics description's default script.
    if has(WeenieHeaderFlag::PScript) {
        desc.pscript = Some(
            o.get_property(PropertyDataId::RestrictionEffect)
                .map_or(0, |v| v.cs_cast()),
        );
    }

    if has(WeenieHeaderFlag::Workmanship) {
        // `Workmanship` writes back an old encoding to `ItemWorkmanship`: `&mut`.
        let workmanship = w
            .objects
            .get_mut(this)
            .expect("ACE: this is null")
            .workmanship();
        desc.workmanship = Some(workmanship.unwrap_or(0.0));
    }

    let o = obj(w, this);
    if has(WeenieHeaderFlag::Burden) {
        desc.burden = Some(o.encumbrance_val().unwrap_or(0) as u16);
    }

    if has(WeenieHeaderFlag::Spell) {
        desc.spell_id = Some(o.spell_did().map_or(0, |s| s as u16));
    }

    if has(WeenieHeaderFlag::HouseOwner) {
        // if mansion, send house owner from master copy
        let mut house_owner = o.house_owner();
        if o.is_house() && o.house_type() == HouseType::Mansion {
            house_owner = shims::house_linked_house_owner(w, this, 0);
        }

        desc.house_owner_iid = Some(dereth_primitives::ObjectId(house_owner.unwrap_or(0)));
    }

    if has(WeenieHeaderFlag::HouseRestrictions) {
        // `this as House`: the flag is only set for houses.
        let mut house = this;
        let h = obj(w, this);
        let house_type = h.house_type();

        // if house object is in dungeon,
        // send the permissions from the outdoor house
        if house_type != HouseType::Apartment && shims::current_landblock_is_dungeon(w, this) {
            house = shims::house_root_house(w, this);
        } else {
            // if mansion or villa, send permissions from master copy
            if house_type == HouseType::Villa || house_type == HouseType::Mansion {
                house = shims::house_root_house(w, this);
            }
        }

        let restrictions = restriction_db_new(w, Some(house));
        desc.restrictions = Some(restriction_db::record(&restrictions));
    }

    let o = obj(w, this);
    if has(WeenieHeaderFlag::HookItemTypes) {
        desc.hook_item_types = Some(o.hook_item_type().map_or(0, i32::cast_unsigned));
    }

    if has(WeenieHeaderFlag::Monarch) {
        desc.monarch = Some(dereth_primitives::ObjectId(o.monarch_id().unwrap_or(0)));
    }

    if has(WeenieHeaderFlag::HookType) {
        desc.hook_type = Some(o.hook_type().unwrap_or(0));
    }

    if has(WeenieHeaderFlag::IconOverlay) {
        desc.icon_overlay_id = Some(known_type_did(
            o.icon_overlay_id().unwrap_or(0),
            0x600_0000,
            this,
            "icon overlay",
        ));
    }

    if (weenie_flags2 & WeenieHeaderFlag2::IconUnderlay).0 != 0 {
        desc.icon_underlay_id = Some(known_type_did(
            o.icon_underlay_id().unwrap_or(0),
            0x600_0000,
            this,
            "icon underlay",
        ));
    }

    if has(WeenieHeaderFlag::MaterialType) {
        desc.material_type = Some(o.material_type().map_or(0, |m| m.0));
    }

    if (weenie_flags2 & WeenieHeaderFlag2::Cooldown).0 != 0 {
        desc.cooldown_id = Some(o.cooldown_id().unwrap_or(0).cast_unsigned());
    }

    if (weenie_flags2 & WeenieHeaderFlag2::CooldownDuration).0 != 0 {
        desc.cooldown_duration = Some(o.cooldown_duration().unwrap_or(0.0));
    }

    if (weenie_flags2 & WeenieHeaderFlag2::PetOwner).0 != 0 {
        desc.pet_owner = Some(dereth_primitives::ObjectId(o.pet_owner().unwrap_or(0)));
    }

    // The header, then each field its flags name, then the alignment (ACE's two `Align()`s: after
    // the object description flags and at the end).
    let strings = [
        name.as_deref().unwrap_or(""),
        plural_name.as_deref().unwrap_or(""),
    ];
    game_message::write_record(writer, &strings, |w| desc.write(w));
}

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .expect("ACE: this is null (the object is not in the world)")
}

// ACE: WorldObject.SerializeModelData
/// The `ModelData`: `0x11`, the three counts, the palette, sub-palettes, texture changes and part
/// changes, aligned.
fn serialize_model_data(w: &mut World, this: ObjectGuid, writer: &mut Vec<u8>) {
    let obj_desc = dispatch::calculate_obj_desc::calculate_obj_desc(w, this);
    let record = obj_desc_record(&obj_desc, this);
    game_message::write_record(writer, &[], |w| record.write(w));
}

/// The `ModelData` as dereth-protocol's record: `0x11`, the three counts, the palette (only with
/// sub-palettes), the sub-palettes, texture changes and part changes, each id a known-type DataID
/// of object `this` ([`known_type_did`]), and the alignment. ACE narrows the counts and offsets to
/// bytes; a count past 255 wraps and is logged ([`game_message::write_record`]).
fn obj_desc_record(obj_desc: &ObjDesc, this: ObjectGuid) -> dereth_protocol::types::ObjDesc {
    let did =
        |value: u32, known_type: u32, field: &str| known_type_did(value, known_type, this, field);
    dereth_protocol::types::ObjDesc {
        palette_id: if obj_desc.sub_palettes.is_empty() {
            0
        } else {
            did(obj_desc.palette_id, 0x400_0000, "palette")
        },
        subpalettes: obj_desc
            .sub_palettes
            .iter()
            .map(|palette| dereth_protocol::types::Subpalette {
                sub_id: did(palette.sub_palette_id, 0x400_0000, "sub-palette"),
                offset: palette.offset as u8,
                num_colors: palette.length as u8,
            })
            .collect(),
        texture_changes: obj_desc
            .texture_changes
            .iter()
            .map(|texture| dereth_protocol::types::TextureMapChange {
                part_index: texture.part_index,
                old_tex_id: did(texture.old_texture, 0x500_0000, "old texture"),
                new_tex_id: did(texture.new_texture, 0x500_0000, "new texture"),
            })
            .collect(),
        anim_part_changes: obj_desc
            .anim_part_changes
            .iter()
            .map(|model| dereth_protocol::types::AnimPartChange {
                part_index: model.index,
                part_id: did(model.animation_id, 0x100_0000, "part"),
            })
            .collect(),
    }
}

// ACE: WorldObject.GetPhysicsStateOrDefault
/// The current physics state, falling back to defaults if no `PhysicsObj` is loaded (inventory
/// items): a player logging in gets the pink bubble state; otherwise the `PhysicsState` property,
/// else `PhysicsGlobals.DefaultState`.
fn get_physics_state_or_default(w: &World, this: ObjectGuid) -> PhysicsState {
    if let Some(h) = obj(w, this).phys {
        return phys_ext::state(w, h);
    }

    let o = obj(w, this);

    // special case for players logging in - sets pink bubble state here
    if o.is_player() {
        return PhysicsState::IgnoreCollisions
            | PhysicsState::Gravity
            | PhysicsState::Hidden
            | PhysicsState::EdgeSlide;
    }

    let default_obj_state = o.get_property(PropertyInt::PhysicsState);

    match default_obj_state {
        Some(s) => PhysicsState(s),
        None => PhysicsState(dereth_physics::globals::DEFAULT_STATE as i32),
    }
}

// ACE: WorldObject.SerializePhysicsData
/// The `PhysicsDesc` of a `CreateObject`.
#[allow(clippy::too_many_lines)]
fn serialize_physics_data(
    w: &mut World,
    this: ObjectGuid,
    writer: &mut Vec<u8>,
    adminvision: bool,
    changenodraw: bool,
) {
    let mut physics_description_flag = calculated_physics_description_flag(w, this);

    let o = obj(w, this);
    let admin_cloaked = adminvision && o.is_player() && o.cloak_status() == CloakStatus::On;
    if admin_cloaked {
        physics_description_flag |= PhysicsDescriptionFlag::Translucency;
    }

    let mut physics_state = get_physics_state_or_default(w, this);

    if changenodraw {
        physics_state &= !PhysicsState::NoDraw;
        physics_state &= !PhysicsState::Cloaked;
    }

    if obj(w, this).is_spell_projectile()
        && shims::property_manager_get_bool(w, "spell_projectile_ethereal", false)
    {
        physics_state |= PhysicsState::Ethereal;
    }

    let mut desc = dereth_protocol::types::PhysicsDesc {
        bitfield: physics_description_flag.0.cast_unsigned(),
        state: physics_state.0.cast_unsigned(),
        ..Default::default()
    };

    let has = |f: PhysicsDescriptionFlag| (physics_description_flag & f).0 != 0;

    if has(PhysicsDescriptionFlag::Movement) {
        /* OLD METHOD
        var movementData = new MovementData(this, CurrentMotionState).Serialize();

        writer.Write((uint)movementData.Length);

        if (movementData.Length > 0)
        {
            writer.Write(movementData);
            writer.Write(Convert.ToUInt32(CurrentMotionState.IsAutonomous));
        }
        */

        let current_motion_state = shims::current_motion_state(obj(w, this))
            .cloned()
            .expect("flag set from CurrentMotionState");
        let movement_data = MovementData::from_motion(this, &current_motion_state);

        // The length, then (when there is any) the movement data and the autonomous flag. The
        // data starts 4-aligned in the message (after the object description and three dwords),
        // so written on its own it is the same bytes.
        let mut buffer = Vec::new();
        let sequences = &mut w
            .objects
            .get_mut(this)
            .expect("ACE: this is null")
            .sequences;
        movement_data::write(&mut buffer, &movement_data, false, sequences);
        debug_assert_eq!(writer.len() % 4, 0, "the movement data starts aligned");

        desc.movement = Some((buffer, u32::from(current_motion_state.is_autonomous)));
    } else if has(PhysicsDescriptionFlag::AnimationFrame) {
        desc.animframe_id = Some(obj(w, this).placement().unwrap_or(Placement::Default).0);
    }

    let o = obj(w, this);
    if has(PhysicsDescriptionFlag::Position) {
        desc.position = Some((&o.location().expect("flag set from Location")).into());
    }

    if has(PhysicsDescriptionFlag::MTable) {
        desc.mtable_id = Some(o.motion_table_id());
    }

    if has(PhysicsDescriptionFlag::STable) {
        desc.stable_id = Some(o.sound_table_id());
    }

    if has(PhysicsDescriptionFlag::PeTable) {
        desc.phstable_id = Some(o.physics_table_id());
    }

    if has(PhysicsDescriptionFlag::CSetup) {
        desc.setup_id = Some(o.setup_table_id());
    }

    if has(PhysicsDescriptionFlag::Parent) {
        desc.parent = Some((
            dereth_primitives::ObjectId(o.wielder_id().unwrap_or(0)),
            o.parent_location().map_or(0, |p| p.0.cast_unsigned()),
        ));
    }

    if has(PhysicsDescriptionFlag::Children) {
        let children = &o.wo.world_object_properties.children;
        let _ = children.len() as i32;
        desc.children = Some(
            children
                .iter()
                .map(|child| dereth_protocol::types::physicsdesc::ChildLink {
                    child_id: dereth_primitives::ObjectId(child.guid),
                    location_id: child.location_id.cast_unsigned(),
                })
                .collect(),
        );
    }

    if has(PhysicsDescriptionFlag::ObjScale) {
        desc.object_scale = Some(o.obj_scale().unwrap_or(0.0));
    }

    if has(PhysicsDescriptionFlag::Friction) {
        desc.friction = Some(o.friction().unwrap_or(0.0));
    }

    if has(PhysicsDescriptionFlag::Elasticity) {
        desc.elasticity = Some(o.elasticity().unwrap_or(0.0));
    }

    if has(PhysicsDescriptionFlag::Translucency) {
        if admin_cloaked {
            desc.translucency = Some(0.5);
        } else {
            desc.translucency = Some(o.translucency().unwrap_or(0.0));
        }
    }

    if has(PhysicsDescriptionFlag::Velocity) {
        desc.velocity = Some(wire_vec3(world_object_properties::velocity(w, this)));
    }

    if has(PhysicsDescriptionFlag::Acceleration) {
        desc.acceleration = Some(wire_vec3(world_object_properties::acceleration(w, this)));
    }

    if has(PhysicsDescriptionFlag::Omega) {
        desc.omega = Some(wire_vec3(world_object_properties::omega(w, this)));
    }

    let o = obj(w, this);
    if has(PhysicsDescriptionFlag::DefaultScript) {
        desc.default_script = Some(o.default_script_id().unwrap_or(0));
    }

    if has(PhysicsDescriptionFlag::DefaultScriptIntensity) {
        desc.default_script_intensity = Some(o.default_script_intensity().unwrap_or(0.0));
    }

    // timestamps
    let s = &mut w
        .objects
        .get_mut(this)
        .expect("ACE: this is null")
        .sequences;
    desc.timestamps = dereth_protocol::types::PhysicsTimestamps {
        position: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectPosition)), // 0
        movement: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectMovement)), // 1
        state: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectState)),       // 2
        vector: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectVector)),     // 3
        teleport: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectTeleport)), // 4
        server_controlled_move: ushort_sequence(
            &s.get_current_sequence(SequenceType::ObjectServerControl),
        ), // 5
        force_position: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectForcePosition)), // 6
        objdesc: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectVisualDesc)), // 7
        instance: ushort_sequence(&s.get_current_sequence(SequenceType::ObjectInstance)),  // 8
    };

    // Then the alignment.
    game_message::write_record(writer, &[], |w| desc.write(w));
}

// ACE: WorldObject.SendUpdatePosition
/// Broadcast position updates to players within range. `admin_move` (default false) is only used
/// if an admin is teleporting a non-player object.
pub fn send_update_position(w: &mut World, this: ObjectGuid, admin_move: bool) {
    //Console.WriteLine($"{Name}.SendUpdatePosition({Location.ToLOCString()})");

    let msg = game_message_update_position::game_message_update_position(w, this, admin_move);
    enqueue_broadcast(w, this, true, &[msg]);

    let now = w.now.utc;
    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object_networking.last_update_position = now;
    }
}

// ACE: WorldObject.SendPartialUpdates
/// Sends each `PropertyInt` of `properties` the object has to `target_session` (other property
/// types are logged and skipped).
pub fn world_object_send_partial_updates(
    w: &mut World,
    this: ObjectGuid,
    target_session: SessionId,
    properties: &[empyrean_entity::GenericPropertyId],
) {
    for property in properties {
        match property.property_type {
            PropertyType::PropertyInt => {
                let value = obj(w, this).get_property(PropertyInt(property.property_id as u16));
                if let Some(value) = value {
                    let player = w
                        .sessions
                        .player(target_session)
                        .expect("ACE: targetSession.Player is null");
                    let p = w
                        .objects
                        .get_mut(player)
                        .expect("ACE: targetSession.Player is null");
                    let msg = game_message_private_update_property_int::game_message_private_update_property_int(
                        p,
                        PropertyInt(property.property_id as u16),
                        value,
                    );
                    game_message::enqueue_send(w, target_session, msg);
                }
            }
            _ => {
                log::debug!(
                    "Unsupported property in SendPartialUpdates: id {}, type {}.",
                    property.property_id,
                    property.property_type
                );
            }
        }
    }
}

// ACE: WorldObject.CalculatedPhysicsDescriptionFlag
/// Calculates the PhysicsDesc flags from the current object state.
pub fn calculated_physics_description_flag(w: &World, this: ObjectGuid) -> PhysicsDescriptionFlag {
    let o = obj(w, this);
    let mut physics_description_flag = PhysicsDescriptionFlag::None;

    if shims::current_motion_state(o).is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::Movement;
    } else if o.placement().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::AnimationFrame;
    }

    if o.location().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::Position;
    }

    if o.motion_table_id() != 0 {
        physics_description_flag |= PhysicsDescriptionFlag::MTable;
    }

    if o.sound_table_id() != 0 {
        physics_description_flag |= PhysicsDescriptionFlag::STable;
    }

    if o.physics_table_id() != 0 {
        physics_description_flag |= PhysicsDescriptionFlag::PeTable;
    }

    if o.setup_table_id() != 0 {
        physics_description_flag |= PhysicsDescriptionFlag::CSetup;
    }

    if !o.wo.world_object_properties.children.is_empty() {
        physics_description_flag |= PhysicsDescriptionFlag::Children;
    }

    if o.wielder_id().is_some() && o.parent_location().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::Parent;
    }

    // where did this epsilon value come from?
    // why is it different from the physics engine epsilon?
    if let Some(obj_scale) = o.obj_scale() {
        if f64::from(obj_scale.abs()) >= 0.001 {
            physics_description_flag |= PhysicsDescriptionFlag::ObjScale;
        }
    }

    if o.friction().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::Friction;
    }

    if o.elasticity().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::Elasticity;
    }

    if let Some(translucency) = o.translucency() {
        if f64::from(translucency.abs()) >= 0.001 {
            physics_description_flag |= PhysicsDescriptionFlag::Translucency;
        }
    }

    if world_object_properties::velocity(w, this) != Vector3::ZERO {
        physics_description_flag |= PhysicsDescriptionFlag::Velocity;
    }

    if world_object_properties::acceleration(w, this) != Vector3::ZERO {
        physics_description_flag |= PhysicsDescriptionFlag::Acceleration;
    }

    if world_object_properties::omega(w, this) != Vector3::ZERO {
        physics_description_flag |= PhysicsDescriptionFlag::Omega;
    }

    if o.default_script_id().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::DefaultScript;
    }

    if o.default_script_intensity().is_some() {
        physics_description_flag |= PhysicsDescriptionFlag::DefaultScriptIntensity;
    }

    physics_description_flag
}

// ACE: WorldObject.CalculatedPhysicsState
/// Pulls the default flags from `PropertyInt.PhysicsState`, sets the `PropertyBool` counterparts
/// that are null, then builds the state from those properties. `InitPhysicsObj` gives a new body
/// this state.
///
/// Not ACE's (a fix, V305): `Static`, `Missile`, `Pushable`, `AlignPath`,
/// `PathClipped`, `ParticleEmitter`, `Hidden`, `Cloaked` and `Sledding` have no property of their
/// own; they read the physics body, and before `InitPhysicsObj` has made one they now take the
/// weenie's `PhysicsState` bit. ACE read them as false there, so those bits of a weenie never
/// reached the body or the CreateObject, and a static weenie got a dynamic body.
#[allow(clippy::too_many_lines)]
pub fn calculated_physics_state(w: &mut World, this: ObjectGuid) -> PhysicsState {
    // This is doing 2 things. It's pulling the default flags from the PropertyInt.PhysicsState, then in turn, setting the PropertyBool counterparts ONLY if they are null.
    // This seems a bit confusing...
    // If we really want to set default states on create or load, we need to separate this function into two parts.

    // Read in Object's Default PhysicsState
    let mut physics_state = get_physics_state_or_default(w, this);
    let phys_state = obj(w, this).phys.map(|h| phys_ext::state(w, h));
    let phys = w.objects.get(this).and_then(|o| o.phys);
    // the body's bit; before there is a body, the weenie's own
    let weenie_state = physics_state;
    let phys_bit = |s: PhysicsState| (phys_state.unwrap_or(weenie_state) & s).0 != 0;

    let hf = |s: PhysicsState| physics_state.contains(s);
    // `if (physicsState.HasFlag(flag)) if (!X.HasValue) X = true;` through the setter, which also
    // writes the body's state when there is one (`SetPhysicsPropertyState`)
    macro_rules! default_true {
        ($flag:ident, $get:ident, $set:ident) => {
            if hf(PhysicsState::$flag) && obj(w, this).$get().is_none() {
                crate::world_objects::world_object_properties::$set(w, this, Some(true));
            }
        };
    }

    // `Static`, `Missile`, `Pushable`, `AlignPath`, `PathClipped`, `ParticleEmitter`, `Hidden`,
    // `Cloaked` and `Sledding` read `GetPhysicsState`, which is never null, so their
    // `if (!X.HasValue)` blocks never set anything.
    default_true!(Ethereal, ethereal, set_ethereal);
    default_true!(ReportCollisions, report_collisions, set_report_collisions);
    default_true!(IgnoreCollisions, ignore_collisions, set_ignore_collisions);
    default_true!(NoDraw, no_draw, set_no_draw);
    // Missile, Pushable, AlignPath, PathClipped: physics-state getters, never null.
    default_true!(Gravity, gravity_status, set_gravity_status);
    default_true!(LightingOn, lights_status, set_lights_status);
    // ParticleEmitter, Hidden: physics-state getters, never null.
    default_true!(
        ScriptedCollision,
        scripted_collision,
        set_scripted_collision
    );
    default_true!(Inelastic, inelastic, set_inelastic);
    // Cloaked: a physics-state getter, never null.
    default_true!(
        ReportCollisionsAsEnvironment,
        report_collisions_as_environment,
        set_report_collisions_as_environment
    );
    default_true!(EdgeSlide, allow_edge_slide, set_allow_edge_slide);
    // Sledding: a physics-state getter, never null.
    default_true!(Frozen, is_frozen, set_is_frozen);

    let o = obj(w, this);
    let mut apply = |on: bool, bit: PhysicsState| {
        if on {
            physics_state |= bit;
        } else {
            physics_state &= !bit;
        }
    };

    apply(phys_bit(PhysicsState::Static), PhysicsState::Static);
    apply(o.ethereal().unwrap_or(false), PhysicsState::Ethereal);
    apply(
        o.report_collisions().unwrap_or(false),
        PhysicsState::ReportCollisions,
    );
    apply(
        o.ignore_collisions().unwrap_or(false),
        PhysicsState::IgnoreCollisions,
    );
    apply(o.no_draw().unwrap_or(false), PhysicsState::NoDraw);
    apply(phys_bit(PhysicsState::Missile), PhysicsState::Missile);
    apply(phys_bit(PhysicsState::Pushable), PhysicsState::Pushable);
    apply(phys_bit(PhysicsState::AlignPath), PhysicsState::AlignPath);
    apply(
        phys_bit(PhysicsState::PathClipped),
        PhysicsState::PathClipped,
    );
    apply(o.gravity_status().unwrap_or(false), PhysicsState::Gravity);
    apply(o.lights_status().unwrap_or(false), PhysicsState::LightingOn);
    apply(
        phys_bit(PhysicsState::ParticleEmitter),
        PhysicsState::ParticleEmitter,
    );
    apply(phys_bit(PhysicsState::Hidden), PhysicsState::Hidden);
    apply(
        o.scripted_collision().unwrap_or(false),
        PhysicsState::ScriptedCollision,
    );
    let setup = o.setup_table_id();
    let inelastic = o.inelastic().unwrap_or(false);
    let report_env = o.report_collisions_as_environment().unwrap_or(false);
    let edge_slide = o.allow_edge_slide().unwrap_or(false);
    let frozen = o.is_frozen().unwrap_or(false);

    // `CSetup.HasPhysicsBSP`: `ReadFromDat` answers an empty setup (no BSP, no default
    // animation or script) for a file the dat does not have.
    let c_setup = shims::c_setup(w, setup);
    let has_physics_bsp = c_setup.as_ref().is_some_and(|c| c.has_physics_bsp);
    let default_anim_id = c_setup.as_ref().map_or(0, |c| c.default_anim_id.0);
    let default_script_id = c_setup.as_ref().map_or(0, |c| c.default_script_id.0);
    apply(has_physics_bsp, PhysicsState::HasPhysicsBSP);
    apply(inelastic, PhysicsState::Inelastic);
    // `PhysicsObj != null && PhysicsObj.HasDefaultAnimation && CSetup.DefaultAnimation > 0`
    apply(
        phys.is_some_and(|h| phys_ext::has_default_animation(w, h)) && default_anim_id > 0,
        PhysicsState::HasDefaultAnim,
    );
    // `PhysicsObj != null && PhysicsObj.HasDefaultScript && CSetup.DefaultScript > 0`
    apply(
        phys.is_some_and(|h| phys_ext::has_default_script(w, h)) && default_script_id > 0,
        PhysicsState::HasDefaultScript,
    );
    apply(phys_bit(PhysicsState::Cloaked), PhysicsState::Cloaked);
    apply(report_env, PhysicsState::ReportCollisionsAsEnvironment);
    apply(edge_slide, PhysicsState::EdgeSlide);
    apply(phys_bit(PhysicsState::Sledding), PhysicsState::Sledding);
    apply(frozen, PhysicsState::Frozen);

    physics_state
}

// ACE: WorldObject.CalculateWeenieHeaderFlag
pub fn calculate_weenie_header_flag(w: &mut World, this: ObjectGuid) -> WeenieHeaderFlag {
    let mut weenie_header_flag = WeenieHeaderFlag::None;

    let o = obj(w, this);
    let mut set = |on: bool, f: WeenieHeaderFlag| {
        if on {
            weenie_header_flag |= f;
        }
    };

    set(o.plural_name().is_some(), WeenieHeaderFlag::PluralName);
    set(
        o.item_capacity().is_some() && !o.is_slum_lord(),
        WeenieHeaderFlag::ItemsCapacity,
    );
    set(
        o.container_capacity().is_some() && !o.is_slum_lord(),
        WeenieHeaderFlag::ContainersCapacity,
    );
    set(o.ammo_type().is_some(), WeenieHeaderFlag::AmmoType);
    set(o.value().is_some_and(|v| v > 0), WeenieHeaderFlag::Value);
    set(o.item_useable().is_some(), WeenieHeaderFlag::Usable);
    set(o.use_radius().is_some(), WeenieHeaderFlag::UseRadius);
    set(o.target_type().is_some(), WeenieHeaderFlag::TargetType);
    set(o.ui_effects().is_some(), WeenieHeaderFlag::UiEffects);
    set(o.combat_use().is_some(), WeenieHeaderFlag::CombatUse);
    set(o.structure().is_some(), WeenieHeaderFlag::Structure);
    set(o.max_structure().is_some(), WeenieHeaderFlag::MaxStructure);
    set(o.stack_size().is_some(), WeenieHeaderFlag::StackSize);
    set(o.max_stack_size().is_some(), WeenieHeaderFlag::MaxStackSize);
    set(o.container_id().is_some(), WeenieHeaderFlag::Container);
    set(o.wielder_id().is_some(), WeenieHeaderFlag::Wielder);
    set(
        o.valid_locations().is_some(),
        WeenieHeaderFlag::ValidLocations,
    );
    set(
        o.current_wielded_location().is_some_and(|c| c.0 != 0)
            && o.wielder_id().is_some_and(|wi| wi != 0),
        WeenieHeaderFlag::CurrentlyWieldedLocation,
    );
    set(o.clothing_priority().is_some(), WeenieHeaderFlag::Priority);
    set(o.radar_color().is_some(), WeenieHeaderFlag::RadarBlipColor);
    set(
        o.radar_behavior().is_some(),
        WeenieHeaderFlag::RadarBehavior,
    );

    // Not ACE's (retail, V334): the script flag follows data id 44, not 30.
    let restriction_effect_did = o.get_property(PropertyDataId::RestrictionEffect);
    set(
        restriction_effect_did.is_some_and(|p| p != 0),
        WeenieHeaderFlag::PScript,
    );

    // `(Workmanship != null) && (uint?)Workmanship != 0u`: the getter may write back (`&mut`).
    let workmanship = w
        .objects
        .get_mut(this)
        .expect("ACE: this is null")
        .workmanship();
    let o = obj(w, this);
    let mut set = |on: bool, f: WeenieHeaderFlag| {
        if on {
            weenie_header_flag |= f;
        }
    };
    set(
        workmanship.is_some_and(|wk| CsCast::<u32>::cs_cast(wk) != 0),
        WeenieHeaderFlag::Workmanship,
    );

    set(
        o.encumbrance_val().is_some_and(|e| e != 0) && !o.is_creature() && !o.is_spell_projectile(),
        WeenieHeaderFlag::Burden,
    );

    set(
        o.spell_did().is_some_and(|s| s != 0),
        WeenieHeaderFlag::Spell,
    );

    let mut house_owner = o.house_owner();
    if o.is_house() {
        set(true, WeenieHeaderFlag::HouseRestrictions);

        if o.house_type() == HouseType::Mansion {
            house_owner = shims::house_linked_house_owner(w, this, 0);
        }
    }

    set(house_owner.is_some(), WeenieHeaderFlag::HouseOwner);

    let hook_item_type_int = o.get_property(PropertyInt::HookItemType);
    set(
        hook_item_type_int.is_some(),
        WeenieHeaderFlag::HookItemTypes,
    );

    set(o.monarch_id().is_some(), WeenieHeaderFlag::Monarch);
    set(o.hook_type().is_some(), WeenieHeaderFlag::HookType);
    set(
        o.icon_overlay_id().is_some_and(|i| i != 0),
        WeenieHeaderFlag::IconOverlay,
    );
    set(o.material_type().is_some(), WeenieHeaderFlag::MaterialType);

    weenie_header_flag
}

// ACE: WorldObject.CalculateWeenieHeaderFlag2
pub fn calculate_weenie_header_flag2(o: &WorldObject) -> WeenieHeaderFlag2 {
    let mut weenie_header_flag2 = WeenieHeaderFlag2::None;

    if o.icon_underlay_id().is_some_and(|i| i != 0) {
        weenie_header_flag2 |= WeenieHeaderFlag2::IconUnderlay;
    }

    if o.cooldown_id().is_some_and(|c| c != 0) {
        weenie_header_flag2 |= WeenieHeaderFlag2::Cooldown;
    }

    // `Math.Abs((float)CooldownDuration) >= 0.001`
    if o.cooldown_duration()
        .is_some_and(|c| f64::from((c as f32).abs()) >= 0.001)
    {
        weenie_header_flag2 |= WeenieHeaderFlag2::CooldownDuration;
    }

    if o.pet_owner().is_some_and(|p| p != 0) {
        weenie_header_flag2 |= WeenieHeaderFlag2::PetOwner;
    }

    weenie_header_flag2
}

// ACE: WorldObject.UpdateObjectDescriptionFlags
fn update_object_description_flags(w: &mut World, this: ObjectGuid) {
    let o = obj(w, this);
    let weenie_type = o.biota.weenie_type;

    let mut updates: Vec<(ObjectDescriptionFlag, bool)> = Vec::new();

    if weenie_type == WeenieType::Container
        || weenie_type == WeenieType::Corpse
        || weenie_type == WeenieType::Chest
        || weenie_type == WeenieType::Hook
        || weenie_type == WeenieType::Storage
    {
        let mut openable = !o.is_locked();

        if weenie_type == WeenieType::Chest
            && !openable
            && shims::property_manager_get_bool(w, "fix_chest_missing_inventory_window", false)
        {
            openable = true;
        }

        updates.push((ObjectDescriptionFlag::Openable, openable));
    }

    updates.push((ObjectDescriptionFlag::Inscribable, o.inscribable()));
    updates.push((ObjectDescriptionFlag::Stuck, o.stuck()));

    let admin_or_sentinel = weenie_type == WeenieType::Admin || weenie_type == WeenieType::Sentinel;
    if admin_or_sentinel {
        updates.push((
            ObjectDescriptionFlag::Player,
            o.cloak_status() < CloakStatus::Creature,
        ));
    }

    updates.push((ObjectDescriptionFlag::Attackable, o.attackable()));
    updates.push((
        ObjectDescriptionFlag::PlayerKiller,
        o.player_killer_status() == PlayerKillerStatus::PK,
    ));
    updates.push((ObjectDescriptionFlag::HiddenAdmin, o.hidden_admin()));
    updates.push((ObjectDescriptionFlag::UiHidden, o.ui_hidden()));

    if admin_or_sentinel {
        updates.push((
            ObjectDescriptionFlag::Admin,
            o.cloak_status() < CloakStatus::Player,
        ));
    }

    updates.push((
        ObjectDescriptionFlag::FreePkStatus,
        o.player_killer_status() == PlayerKillerStatus::Free,
    ));
    updates.push((
        ObjectDescriptionFlag::ImmuneCellRestrictions,
        o.ignore_house_barriers(),
    ));
    updates.push((
        ObjectDescriptionFlag::RequiresPackSlot,
        o.requires_pack_slot(),
    ));
    updates.push((ObjectDescriptionFlag::Retained, o.retained()));
    updates.push((
        ObjectDescriptionFlag::PkLiteStatus,
        o.player_killer_status() == PlayerKillerStatus::PKLite,
    ));

    let weenie_flags2 = calculate_weenie_header_flag2(o);

    updates.push((
        ObjectDescriptionFlag::IncludesSecondHeader,
        weenie_flags2 > WeenieHeaderFlag2::None,
    ));

    updates.push((ObjectDescriptionFlag::WieldOnUse, o.wield_on_use()));
    updates.push((ObjectDescriptionFlag::WieldLeft, o.wield_left()));

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    for (flag, value) in updates {
        update_object_description_flag(o, flag, value);
    }
}

// ACE: WorldObject.UpdateObjectDescriptionFlag
fn update_object_description_flag(o: &mut WorldObject, flag: ObjectDescriptionFlag, value: bool) {
    if value {
        o.wo.world_object.object_description_flags |= flag;
    } else {
        o.wo.world_object.object_description_flags &= !flag;
    }
}

// ACE: WorldObject.CalculateObjDesc
/// The base object description: a hook with an item shows the item's; otherwise the base model
/// data plus the clothing table's effects for this object's setup.
pub fn world_object_calculate_obj_desc(w: &mut World, this: ObjectGuid) -> ObjDesc {
    if obj(w, this).is_hook() {
        if let Some(item) = shims::hook_item(w, this) {
            return dispatch::calculate_obj_desc::calculate_obj_desc(w, item);
        }
    }

    let mut obj_desc = ObjDesc::default();

    add_base_model_data(w, this, &mut obj_desc);

    let o = obj(w, this);
    let Some(clothing_base) = o.clothing_base() else {
        return obj_desc;
    };
    // `ReadFromDat<ClothingTable>`: a missing file is ACE's empty table.
    let item = shims::read_clothing_table(w, clothing_base);

    let setup_table_id = o.setup_table_id();
    // Check if the ClothingBase is applicable for this Setup. (Gear Knights, this is usually you.)
    if let Some(clothing_base_effect) = item.as_ref().and_then(|i| {
        i.clothing_bases
            .get(&dereth_primitives::DataId(setup_table_id))
    }) {
        // Add the model and texture(s)
        for t in clothing_base_effect {
            obj_desc.anim_part_changes.push(PropertiesAnimPart {
                index: t.part_num as u8,
                animation_id: t.object_id.0,
            });
            for t1 in &t.texture_effects {
                obj_desc.texture_changes.push(PropertiesTextureMap {
                    part_index: t.part_num as u8,
                    old_texture: t1.old_texture.0,
                    new_texture: t1.new_texture.0,
                });
            }
        }

        let item = item.as_ref().expect("checked above");
        let shade_prop = o.shade();
        let palette_template = o.palette_template();

        //if (item.ClothingSubPalEffects.Count == 1 && (PaletteTemplate.HasValue | Shade.HasValue))
        //    Console.WriteLine($"Found an item with 1 ClothingSubPalEffects and a PaletteTemplate = {PaletteTemplate} and/or Shade = {Shade} ");

        // If there are no ClothingSubPalEffects, or this item has no Shade and no PaletteTemplate set, we will not apply any Palette changes
        if !item.palette_templates.is_empty()
            && (shade_prop.is_some() || palette_template.is_some())
        {
            let pal_option = palette_template.unwrap_or(0);

            // Load the correct ClothingSubPalEffects for the assigned PaletteTemplate, or the first in the Dictionary if none set or it is set to an invalid value
            let item_sub_pal = match item.palette_templates.get(&(pal_option as u32)) {
                Some(p) => p.clone(),
                None => shims::first_palette_template(w, clothing_base, item),
            };

            if item_sub_pal.icon.0 > 0 && !o.ignore_clo_icons().unwrap_or(false) {
                w.objects
                    .get_mut(this)
                    .expect("ACE: this is null")
                    .set_icon_id(item_sub_pal.icon.0);
            }

            // Default if no shade is set. `(float)Shade`, widened again by `GetPaletteID(double)`.
            let shade = f64::from(shade_prop.map_or(0.0f32, |s| s as f32));

            for sub in &item_sub_pal.subpalette_effects {
                let item_pal =
                    shims::palette_set_get_palette_id(w, sub.palette_set.0, shade) as u16;

                for range in &sub.ranges {
                    let pal_offset = (range.offset / 8) as u16;
                    let num_colors = (range.length / 8) as u16;
                    obj_desc.sub_palettes.push(PropertiesPalette {
                        sub_palette_id: u32::from(item_pal),
                        offset: pal_offset,
                        length: num_colors,
                    });
                }
            }
        }
    }

    obj_desc
}

// ACE: WorldObject.AddBaseModelData
/// The head, hair, skin, eyes, nose and mouth of a character-like object.
pub fn add_base_model_data(w: &World, this: ObjectGuid, obj_desc: &mut ObjDesc) {
    let o = obj(w, this);

    // Hair/head

    // if (HeadObjectDID.HasValue && !HairStyle.HasValue)
    // This Heritage check has been added for backwards compatibility. It works around the butthead Gear Knights appearance.
    if o.head_object_did().is_some()
        && o.hair_style().is_none()
        && o.heritage()
            .is_some_and(|h| h != HeritageGroup::Gearknight.0)
    {
        obj_desc.anim_part_changes.push(PropertiesAnimPart {
            index: 0x10,
            animation_id: o.head_object_did().expect("checked"),
        });
    } else if let (Some(hair_style), Some(heritage), Some(gender)) =
        (o.hair_style(), o.heritage(), o.gender())
    {
        // This indicates we have a Gear Knight or Olthoi(that is, player types treat "hairstyle" as a "Body Style")

        // Load the CharGen data. It has all the anim & texture changes for the Body Style defined within it
        let cg = w.dats.portal_dat().char_gen();
        let heritage_group = cg
            .heritage_groups
            .get(&(heritage as u32))
            .unwrap_or_else(|| panic!("ACE: KeyNotFoundException heritage {heritage}"));
        let sex = heritage_group
            .sexes
            .get(&(gender as u32))
            .unwrap_or_else(|| panic!("ACE: KeyNotFoundException gender {gender}"));
        if (sex.hair_styles.len() as i64) > i64::from(hair_style) {
            // just check for a valid entry...
            let hairstyle = &sex.hair_styles[hair_style as usize];

            // Add all the texture changes
            for (part_index, old_texture, new_texture) in &hairstyle.objdesc.texture_changes {
                obj_desc.texture_changes.push(PropertiesTextureMap {
                    part_index: *part_index,
                    old_texture: old_texture.0,
                    new_texture: new_texture.0,
                });
            }

            // Add all the animation part changes
            for (part_index, part_id) in &hairstyle.objdesc.anim_part_changes {
                obj_desc.anim_part_changes.push(PropertiesAnimPart {
                    index: *part_index,
                    animation_id: part_id.0,
                });
            }
        }
    }

    if o.is_player() {
        let character = shims::player_character(o)
            .expect("ACE: player.Character is null (NullReferenceException)");
        obj_desc.texture_changes.push(PropertiesTextureMap {
            part_index: 0x10,
            old_texture: character.default_hair_texture,
            new_texture: character.hair_texture,
        });
    }
    //AddTexture(0x10, DefaultHairTextureDID.Value, HairTextureDID.Value);
    if let Some(hair_palette_did) = o.hair_palette_did() {
        obj_desc.sub_palettes.push(PropertiesPalette {
            sub_palette_id: hair_palette_did,
            offset: 0x18,
            length: 0x8,
        });
    }
    //AddPalette(HairPaletteDID.Value, 0x18, 0x8);

    // Skin
    // PaletteBaseId = PaletteBaseDID;
    if let Some(palette_base_did) = o.palette_base_did() {
        obj_desc.palette_id = palette_base_did;
    }
    if let Some(skin_palette_did) = o.skin_palette_did() {
        obj_desc.sub_palettes.push(PropertiesPalette {
            sub_palette_id: skin_palette_did,
            offset: 0x0,
            length: 0x18,
        });
    }
    //AddPalette(SkinPalette.Value, 0x0, 0x18);

    // Eyes
    if let (Some(default_eyes), Some(eyes)) = (o.default_eyes_texture_did(), o.eyes_texture_did()) {
        obj_desc.texture_changes.push(PropertiesTextureMap {
            part_index: 0x10,
            old_texture: default_eyes,
            new_texture: eyes,
        });
    }
    //AddTexture(0x10, DefaultEyesTextureDID.Value, EyesTextureDID.Value);
    if let Some(eyes_palette_did) = o.eyes_palette_did() {
        obj_desc.sub_palettes.push(PropertiesPalette {
            sub_palette_id: eyes_palette_did,
            offset: 0x20,
            length: 0x8,
        });
    }
    //AddPalette(EyesPaletteDID.Value, 0x20, 0x8);

    // Nose & Mouth
    if let (Some(default_nose), Some(nose)) = (o.default_nose_texture_did(), o.nose_texture_did()) {
        obj_desc.texture_changes.push(PropertiesTextureMap {
            part_index: 0x10,
            old_texture: default_nose,
            new_texture: nose,
        });
    }
    //AddTexture(0x10, NoseTextureDID.Value, NoseTextureDID.Value);
    if let (Some(default_mouth), Some(mouth)) =
        (o.default_mouth_texture_did(), o.mouth_texture_did())
    {
        obj_desc.texture_changes.push(PropertiesTextureMap {
            part_index: 0x10,
            old_texture: default_mouth,
            new_texture: mouth,
        });
    }
    //AddTexture(0x10, DefaultMouthTextureDID.Value, MouthTextureDID.Value);
}

// ================================================================================ broadcasts

// ACE: WorldObject.EnqueueActionBroadcast
/// Runs `delegate_action` for all Players that currently know about this object, each on its own
/// queue. The known players are read now; `delegate_action` must capture by value (guids and the
/// values it needs), because the object may leave the world before the actions run
/// (`Destroy` removes it from `World.objects`). `exclude_self` defaults to false.
pub fn enqueue_action_broadcast(
    w: &mut World,
    this: ObjectGuid,
    delegate_action: impl Fn(&mut World, ObjectGuid) + Clone + Send + 'static,
    exclude_self: bool,
) {
    enqueue_action_broadcast_to(w, this, delegate_action, exclude_self, Recipients::Known);
}

/// Not ACE's (retail captures, V286): [`enqueue_action_broadcast`] to
/// the object's broadcast reach ([`reach_players`]) rather than to the players who know it, for the
/// actions that tell a player an object is gone (retail's decay deletes reached players who had
/// forgotten the corpse). Actions that create an object for the player stay on
/// [`enqueue_action_broadcast`]: a create belongs to the create window, not the reach.
pub fn enqueue_action_broadcast_to_reach(
    w: &mut World,
    this: ObjectGuid,
    delegate_action: impl Fn(&mut World, ObjectGuid) + Clone + Send + 'static,
    exclude_self: bool,
) {
    enqueue_action_broadcast_to(w, this, delegate_action, exclude_self, Recipients::Reach);
}

/// Which players a broadcast goes to.
#[derive(Clone, Copy)]
enum Recipients {
    /// The players who know the object (ACE's set for every broadcast; kept for creates).
    Known,
    /// Not ACE's (V286): the object's landblock reach.
    Reach,
}

fn recipients(w: &World, this: ObjectGuid, to: Recipients) -> Vec<ObjectGuid> {
    match to {
        Recipients::Known => known_players(w, this),
        Recipients::Reach => reach_players(w, this),
    }
}

fn enqueue_action_broadcast_to(
    w: &mut World,
    this: ObjectGuid,
    delegate_action: impl Fn(&mut World, ObjectGuid) + Clone + Send + 'static,
    exclude_self: bool,
    to: Recipients,
) {
    let Some(o) = w.objects.get(this) else { return };
    if o.phys.is_none() {
        return;
    }

    if !exclude_self && o.is_player() {
        let action = delegate_action.clone();
        ActionChain::with_action(Actor::Object(this), move |w| action(w, this)).enqueue_chain(w);
    }

    let visibility = obj(w, this).visibility();
    for player in recipients(w, this, to) {
        if visibility && !shims::player_adminvision(w, player) {
            continue;
        }

        if exclude_self && this == player {
            continue;
        }

        let action = delegate_action.clone();
        ActionChain::with_action(Actor::Object(player), move |w| action(w, player))
            .enqueue_chain(w);
    }
}

// ACE: WorldObject.GetParentLandblock
/// Traverses the owner list for the input item, and returns the object in the current landblock.
pub fn get_parent_landblock(w: &World, this: ObjectGuid, item: ObjectGuid) -> Option<ObjectGuid> {
    let mut iterator = item;
    while obj(w, iterator).current_landblock.is_none() {
        let Some(owner_id) = obj(w, iterator).owner_id() else {
            break;
        };

        let current_landblock = obj(w, this)
            .current_landblock
            .expect("ACE: CurrentLandblock is null (NullReferenceException)");
        iterator = crate::entity::landblock::get_object(
            w,
            current_landblock,
            ObjectGuid::new(owner_id),
            true,
        )
        .expect("ACE: GetObject returned null (NullReferenceException)");
    }
    if obj(w, iterator).current_landblock.is_none() {
        None
    } else {
        Some(iterator)
    }
}

// ACE: WorldObject.EnqueueMotionMagic
/// `speed` defaults to 1.0 in ACE.
pub fn enqueue_motion_magic(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    motion_command: MotionCommand,
    speed: f32,
) -> f32 {
    let mut motion = Motion::new(MotionStance::Magic, motion_command, speed);
    motion.motion_state.turn_speed = 2.25; // ??

    let motion_table_id = obj(w, this).motion_table_id();
    let anim_length = shims::motion_table_get_animation_length(
        w,
        motion_table_id,
        MotionStance::Magic,
        motion_command,
        None,
        speed,
    );

    action_chain.add_action(Actor::Object(this), move |w| {
        if w.objects.get(this).is_some_and(WorldObject::is_player)
            && shims::player_magic_state_is_casting(w, this)
        {
            enqueue_broadcast_motion(w, this, &motion, None, None);
        }
    });
    action_chain.add_delay_seconds(w, f64::from(anim_length));

    anim_length
}

// ACE: WorldObject.EnqueueMotion
/// `EnqueueMotion(ActionChain, MotionCommand, float speed = 1.0f, bool useStance = true,
/// MotionCommand? prevCommand = null, bool castGesture = false, bool half = false)`.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn enqueue_motion(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    motion_command: MotionCommand,
    speed: f32,
    use_stance: bool,
    prev_command: Option<MotionCommand>,
    cast_gesture: bool,
    half: bool,
) -> f32 {
    let current = shims::current_motion_state(obj(w, this)).map(|m| m.stance);
    let mut stance = match current {
        Some(s) if use_stance => s,
        _ => MotionStance::NonCombat,
    };

    if cast_gesture {
        stance = MotionStance::Magic;
    }

    let mut motion = Motion::new(stance, motion_command, speed);
    motion.motion_state.turn_speed = 2.25; // ??

    let motion_table_id = obj(w, this).motion_table_id();
    let mut anim_length = match prev_command {
        Some(prev) => shims::motion_table_get_animation_length(
            w,
            motion_table_id,
            stance,
            prev,
            Some(motion_command),
            speed,
        ),
        None => shims::motion_table_get_animation_length(
            w,
            motion_table_id,
            stance,
            motion_command,
            None,
            speed,
        ),
    };

    action_chain.add_action(Actor::Object(this), move |w| {
        if cast_gesture
            && w.objects.get(this).is_some_and(WorldObject::is_player)
            && !shims::player_magic_state_is_casting(w, this)
        {
            return;
        }

        shims::set_current_motion_state(w, this, Some(motion.clone()));
        enqueue_broadcast_motion(w, this, &motion, None, None);
    });

    if half {
        anim_length *= 0.5;
    }

    action_chain.add_delay_seconds(w, f64::from(anim_length));

    anim_length
}

// ACE: WorldObject.EnqueueMotion
/// `EnqueueMotion(ActionChain, MotionStance, MotionCommand, float speed = 1.0f)`: specialized
/// function to mitigate odd client behavior w/ swapping bows during repeat attacks.
pub fn enqueue_motion_stance(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    stance: MotionStance,
    motion_command: MotionCommand,
    speed: f32,
) -> f32 {
    // TODO: fix the CurrentMotionState mess
    let mut motion = Motion::new(stance, motion_command, speed);
    motion.motion_state.turn_speed = 2.25; // ??

    let motion_table_id = obj(w, this).motion_table_id();
    let anim_length = shims::motion_table_get_animation_length(
        w,
        motion_table_id,
        stance,
        motion_command,
        None,
        speed,
    );

    action_chain.add_action(Actor::Object(this), move |w| {
        // if no longer in missile combat, don't bother
        if w.objects.get(this).is_some_and(WorldObject::is_player)
            && !shims::player_combat_mode_is_missile(w, this)
        {
            return;
        }

        // retain original profile of function, but if something else has changed the stance (such as weapon swapping),
        // do not thrash CurrentMotionState.Stance
        let current = shims::current_motion_state(obj(w, this))
            .expect("ACE: CurrentMotionState is null")
            .stance;
        if current == stance {
            shims::set_current_motion_state(w, this, Some(motion.clone()));
        }

        enqueue_broadcast_motion(w, this, &motion, None, None);
    });
    action_chain.add_delay_seconds(w, f64::from(anim_length));

    anim_length
}

// ACE: WorldObject.EnqueueMotionPersist
/// `EnqueueMotionPersist(ActionChain, MotionStance, MotionCommand, float speed = 1.0f)`.
pub fn enqueue_motion_persist_stance(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    stance: MotionStance,
    motion_command: MotionCommand,
    speed: f32,
) -> f32 {
    if !shims::property_manager_get_bool(w, "persist_movement", false) {
        return enqueue_motion_stance(w, this, action_chain, stance, motion_command, speed);
    }

    // specialized function to mitigate odd client behavior w/ swapping bows during repeat attacks
    // TODO: fix the CurrentMotionState mess
    let mut motion = Motion::new(stance, motion_command, speed);
    motion.motion_state.turn_speed = 2.25; // ??

    let motion_table_id = obj(w, this).motion_table_id();
    let anim_length = shims::motion_table_get_animation_length(
        w,
        motion_table_id,
        stance,
        motion_command,
        None,
        speed,
    );

    action_chain.add_action(Actor::Object(this), move |w| {
        // if no longer in missile combat, don't bother
        if w.objects.get(this).is_some_and(WorldObject::is_player)
            && !shims::player_combat_mode_is_missile(w, this)
        {
            return;
        }

        // retain original profile of function, but if something else has changed the stance (such as weapon swapping),
        // do not thrash CurrentMotionState.Stance
        let current = shims::current_motion_state(obj(w, this))
            .cloned()
            .expect("ACE: CurrentMotionState is null");
        let mut motion = motion;
        // Not ACE's (a fix, V330): the sidestep and turn are kept from the
        // previous state before an unchanged stance stores the motion as the current state, so
        // they are not dropped.
        motion.persist(&current);
        if current.stance == stance {
            shims::set_current_motion_state(w, this, Some(motion.clone()));
        }

        enqueue_broadcast_motion(w, this, &motion, None, None);
    });
    action_chain.add_delay_seconds(w, f64::from(anim_length));

    anim_length
}

// ACE: WorldObject.EnqueueMotionPersist
/// `EnqueueMotionPersist(ActionChain, MotionCommand, float speed = 1.0f, bool useStance = true,
/// MotionCommand? prevCommand = null, bool castGesture = false, bool half = false)`.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn enqueue_motion_persist(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    motion_command: MotionCommand,
    speed: f32,
    use_stance: bool,
    prev_command: Option<MotionCommand>,
    cast_gesture: bool,
    half: bool,
) -> f32 {
    if !shims::property_manager_get_bool(w, "persist_movement", false) {
        return enqueue_motion(
            w,
            this,
            action_chain,
            motion_command,
            speed,
            use_stance,
            prev_command,
            cast_gesture,
            half,
        );
    }

    let current = shims::current_motion_state(obj(w, this)).map(|m| m.stance);
    let mut stance = match current {
        Some(s) if use_stance => s,
        _ => MotionStance::NonCombat,
    };

    if cast_gesture {
        stance = MotionStance::Magic;
    }

    let motion_table_id = obj(w, this).motion_table_id();
    let mut anim_length = match prev_command {
        Some(prev) => shims::motion_table_get_animation_length(
            w,
            motion_table_id,
            stance,
            prev,
            Some(motion_command),
            speed,
        ),
        None => shims::motion_table_get_animation_length(
            w,
            motion_table_id,
            stance,
            motion_command,
            None,
            speed,
        ),
    };

    action_chain.add_action(Actor::Object(this), move |w| {
        if cast_gesture
            && w.objects.get(this).is_some_and(WorldObject::is_player)
            && !shims::player_magic_state_is_casting(w, this)
        {
            return;
        }

        let mut motion = Motion::new(stance, motion_command, speed);
        let current = shims::current_motion_state(obj(w, this))
            .cloned()
            .expect("ACE: CurrentMotionState is null");
        motion.persist(&current);

        motion.motion_state.turn_speed = 2.25; // ??

        shims::set_current_motion_state(w, this, Some(motion.clone()));
        enqueue_broadcast_motion(w, this, &motion, None, None);
    });

    if half {
        anim_length *= 0.5;
    }

    action_chain.add_delay_seconds(w, f64::from(anim_length));

    anim_length
}

// ACE: WorldObject.EnqueueMotionAction
/// `EnqueueMotionAction(ActionChain, List<MotionCommand>, float speed = 1.0f, MotionStance?
/// useStance = null, bool usePrevCommand = false, bool checkCasting = false)`.
#[allow(clippy::too_many_arguments)]
pub fn enqueue_motion_action(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    motion_commands: &[MotionCommand],
    speed: f32,
    use_stance: Option<MotionStance>,
    use_prev_command: bool,
    check_casting: bool,
) -> f32 {
    let current = shims::current_motion_state(obj(w, this)).cloned();
    let stance = use_stance.unwrap_or_else(|| {
        current
            .as_ref()
            .expect("ACE: CurrentMotionState is null")
            .stance
    });

    let mut motion = Motion::new(stance, MotionCommand::Ready, speed);

    for motion_command in motion_commands {
        motion
            .motion_state
            .add_command(this, *motion_command, speed);
    }

    motion.motion_state.turn_speed = 2.25; // ??

    let motion_table_id = obj(w, this).motion_table_id();
    let mut anim_length = 0.0f32;
    if use_prev_command {
        let prev_command = current
            .as_ref()
            .expect("ACE: CurrentMotionState is null")
            .motion_state
            .forward_command;

        for motion_command in motion_commands {
            anim_length += shims::motion_table_get_animation_length(
                w,
                motion_table_id,
                stance,
                prev_command,
                Some(*motion_command),
                speed,
            );
        }
    } else {
        for motion_command in motion_commands {
            anim_length += shims::motion_table_get_animation_length(
                w,
                motion_table_id,
                stance,
                *motion_command,
                None,
                speed,
            );
        }
    }

    let commands: Vec<MotionCommand> = motion_commands.to_vec();
    action_chain.add_action(Actor::Object(this), move |w| {
        if check_casting
            && w.objects.get(this).is_some_and(WorldObject::is_player)
            && !shims::player_magic_state_is_casting(w, this)
        {
            return;
        }

        shims::set_current_motion_state(w, this, Some(motion.clone()));
        enqueue_broadcast_motion(w, this, &motion, None, Some(false));

        apply_physics_motion(w, this, &Motion::new(stance, MotionCommand::Ready, speed));

        for motion_command in &commands {
            apply_physics_motion(w, this, &Motion::new(stance, *motion_command, speed));
        }
    });

    action_chain.add_delay_seconds(w, f64::from(anim_length));

    anim_length
}

// ACE: WorldObject.EnqueueMotion_Force
/// `EnqueueMotion_Force(ActionChain, MotionStance, MotionCommand, MotionCommand? prevCommand =
/// null, float speed = 1.0f, float animMod = 1.0f)`.
#[allow(clippy::too_many_arguments)]
pub fn enqueue_motion_force(
    w: &mut World,
    this: ObjectGuid,
    action_chain: &mut ActionChain,
    stance: MotionStance,
    motion_command: MotionCommand,
    prev_command: Option<MotionCommand>,
    speed: f32,
    anim_mod: f32,
) -> f32 {
    let motion = Motion::new(stance, motion_command, speed);

    let motion_table_id = obj(w, this).motion_table_id();
    let anim_length = match prev_command {
        None => shims::motion_table_get_animation_length(
            w,
            motion_table_id,
            stance,
            motion_command,
            None,
            speed,
        ),
        Some(prev) => {
            // `Enum.IsDefined(typeof(MotionStance), (uint)prevCommand)`
            let is_stance = MotionStance(prev.0).is_defined();

            if is_stance {
                shims::motion_table_get_animation_length_stances(
                    w,
                    motion_table_id,
                    MotionStance(prev.0),
                    motion_command,
                    MotionCommand(stance.0),
                    speed,
                )
            } else {
                shims::motion_table_get_animation_length(
                    w,
                    motion_table_id,
                    stance,
                    prev,
                    Some(motion_command),
                    speed,
                )
            }
        }
    };

    action_chain.add_action(Actor::Object(this), move |w| {
        shims::set_current_motion_state(w, this, Some(motion.clone()));

        enqueue_broadcast_motion(w, this, &motion, None, None);
    });

    action_chain.add_delay_seconds(w, f64::from(anim_length * anim_mod));
    anim_length
}

// ACE: WorldObject.EnqueueBroadcastMotion_Physics
/// A `public static bool` in ACE that nothing reassigns.
pub const ENQUEUE_BROADCAST_MOTION_PHYSICS: bool = true;

// ACE: WorldObject.EnqueueBroadcastMotion
/// `EnqueueBroadcastMotion(Motion motion, float? maxRange = null, bool? applyPhysics = null)`.
pub fn enqueue_broadcast_motion(
    w: &mut World,
    this: ObjectGuid,
    motion: &Motion,
    max_range: Option<f32>,
    apply_physics: Option<bool>,
) {
    let apply_physics = match apply_physics {
        Some(a) => a,
        None => {
            if obj(w, this).is_player() {
                shims::player_fast_tick(w, this)
            } else {
                false
            }
        }
    };

    // `new GameMessageUpdateMotion(this, motion)`: `new MovementData(wo, motion)` and the send.
    let movement_data = MovementData::from_motion(this, motion);
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let msg = game_message_update_motion::game_message_update_motion(o, &movement_data);

    match max_range {
        None => {
            enqueue_broadcast(w, this, true, &[msg]);
        }
        Some(range) => enqueue_broadcast_range(w, this, &msg, range, None),
    }

    if ENQUEUE_BROADCAST_MOTION_PHYSICS && apply_physics {
        apply_physics_motion(w, this, motion);
    }
}

// ACE: WorldObject.ApplyPhysicsMotion
/// Applies the motion's stance, forward command and speed to the physics body's raw motion state
/// and applies it (3.6's physics half, which reads `allowJump` first and restarts an idle clock).
/// Without a body, nothing (ACE: `NullReferenceException` on `PhysicsObj`).
pub fn apply_physics_motion(w: &mut World, this: ObjectGuid, motion: &Motion) {
    let Some(h) = obj(w, this).phys else { return };
    crate::physics::phys_ext::apply_physics_motion(
        w,
        h,
        motion.stance.0,
        motion.motion_state.forward_command.0,
        motion.motion_state.forward_speed,
    );
}

// ACE: WorldObject.PlayersInRange
/// TRUE if there are any players within range of this object (`range` defaults to 96).
pub fn players_in_range(w: &mut World, this: ObjectGuid, range: f32) -> bool {
    let is_dungeon = obj(w, this).current_landblock.is_some()
        && crate::entity::landblock::current_physics_landblock_is_dungeon(w, this);

    let range_squared = range * range;

    let location = obj(w, this).location().expect("ACE: Location is null");
    let visibility = obj(w, this).visibility();
    // Not ACE's (retail captures, V286): the range test runs over the
    // broadcast reach, not only the players who know the object.
    for player in reach_players(w, this) {
        let player_location = obj(w, player)
            .location()
            .expect("ACE: player.Location is null");
        if is_dungeon && location.landblock() != player_location.landblock() {
            continue;
        }

        if visibility && !shims::player_adminvision(w, player) {
            continue;
        }

        //var dist = Vector3.Distance(Location.ToGlobal(), player.Location.ToGlobal());
        //var distSquared = Vector3.DistanceSquared(Location.ToGlobal(), player.Location.ToGlobal());
        let dist_squared = location.squared_distance_to(&player_location);
        if dist_squared <= range_squared {
            return true;
        }
    }
    false
}

// ACE: WorldObject.EnqueueBroadcast
/// `EnqueueBroadcast(GameMessage msg, float range, ChatMessageType? squelchType = null)`: sends to
/// the players who know about this object within `range` (and to itself, if a player).
pub fn enqueue_broadcast_range(
    w: &mut World,
    this: ObjectGuid,
    msg: &GameMessage,
    range: f32,
    squelch_type: Option<ChatMessageType>,
) {
    let Some(o) = w.objects.get(this) else { return };
    if o.phys.is_none() || o.current_landblock.is_none() {
        return;
    }

    let mut self_ = None;
    if o.is_player() {
        self_ = Some(this);
        if let Some(session) = shims::player_session(w, this) {
            game_message::enqueue_send(w, session, msg.clone());
        }
    }

    let is_dungeon = crate::entity::landblock::current_physics_landblock_is_dungeon(w, this);

    let range_squared = range * range;

    let location = obj(w, this).location().expect("ACE: Location is null");
    let visibility = obj(w, this).visibility();
    // Not ACE's (retail captures, V286): the range filter runs over the
    // broadcast reach, so a speaker in range is heard whether or not the listener was sent it.
    for player in reach_players(w, this) {
        if let (Some(self_), Some(squelch_type)) = (self_, squelch_type) {
            if shims::player_squelches_contains(w, player, self_, squelch_type) {
                continue;
            }
        }

        let player_location = obj(w, player)
            .location()
            .expect("ACE: player.Location is null");
        if is_dungeon && location.landblock() != player_location.landblock() {
            continue;
        }

        if visibility && !shims::player_adminvision(w, player) {
            continue;
        }

        //var dist = Vector3.Distance(Location.ToGlobal(), player.Location.ToGlobal());
        //var distSquared = Vector3.DistanceSquared(Location.ToGlobal(), player.Location.ToGlobal());
        let dist_squared = location.squared_distance_to(&player_location);
        if dist_squared <= range_squared {
            if let Some(session) = shims::player_session(w, player) {
                game_message::enqueue_send(w, session, msg.clone());
            }
        }
    }
}

// ACE: WorldObject.EnqueueBroadcast
/// `EnqueueBroadcast(bool sendSelf = true, params GameMessage[] msgs)` (and `EnqueueBroadcast(params
/// GameMessage[] msgs)`, which is `sendSelf` true): sends to every player who knows about this
/// object. Without a physics body, a contained object broadcasts through its container; `None` is
/// ACE's `null` (neither).
///
/// Not ACE's (retail captures, V286): sent to the object's broadcast
/// reach ([`reach_players`], its 3×3 landblocks), whether or not each player knows the object, as
/// retail sent its updates and deletes; ACE sent to the known players only. A CreateObject or an
/// UpdateObject (the cloak's re-create, a hook's or a tinkered item's full update) still goes to
/// the known players only: either one creates the object on a client that does not have it, and a
/// create belongs to the create window, not the reach (the server would not know that player had
/// it, so would never forget or delete it there).
pub fn enqueue_broadcast(
    w: &mut World,
    this: ObjectGuid,
    send_self: bool,
    msgs: &[GameMessage],
) -> Option<Vec<ObjectGuid>> {
    let creates = msgs.iter().any(|m| {
        m.opcode == crate::network::game_messages::game_message_opcode::GameMessageOpcode::ObjectCreate
            || m.opcode == crate::network::game_messages::game_message_opcode::GameMessageOpcode::UpdateObject
    });
    enqueue_broadcast_to(
        w,
        this,
        send_self,
        msgs,
        if creates {
            Recipients::Known
        } else {
            Recipients::Reach
        },
    )
}

fn enqueue_broadcast_to(
    w: &mut World,
    this: ObjectGuid,
    send_self: bool,
    msgs: &[GameMessage],
    to: Recipients,
) -> Option<Vec<ObjectGuid>> {
    let o = w.objects.get(this)?;
    if o.phys.is_none() {
        if let Some(container) = o.wo.world_object_properties.container {
            return enqueue_broadcast_to(w, container, send_self, msgs, to);
        }

        return None;
    }

    if send_self && o.is_player() {
        if let Some(session) = shims::player_session(w, this) {
            game_message::enqueue_send_many(w, session, msgs.iter().cloned());
        }
    }

    let nearby_players = recipients(w, this, to);
    let visibility = obj(w, this).visibility();
    for player in &nearby_players {
        if visibility && !shims::player_adminvision(w, *player) {
            continue;
        }

        if let Some(session) = shims::player_session(w, *player) {
            game_message::enqueue_send_many(w, session, msgs.iter().cloned());
        }
    }
    Some(nearby_players)
}

// ACE: WorldObject.EnqueueBroadcast
/// `EnqueueBroadcast(List<Player> excludePlayers, bool sendSelf = true, params GameMessage[]
/// msgs)`.
pub fn enqueue_broadcast_excluding(
    w: &mut World,
    this: ObjectGuid,
    exclude_players: &[ObjectGuid],
    send_self: bool,
    msgs: &[GameMessage],
) -> Option<Vec<ObjectGuid>> {
    let o = w.objects.get(this)?;
    o.phys?;

    if send_self && o.is_player() {
        if let Some(session) = shims::player_session(w, this) {
            game_message::enqueue_send_many(w, session, msgs.iter().cloned());
        }
    }

    // Not ACE's (retail captures, V286): the broadcast reach, not only
    // the players who know the object.
    let nearby_players = reach_players(w, this);
    let visibility = obj(w, this).visibility();
    // `nearbyPlayers.Except(excludePlayers)`: set difference, keeping first occurrences in order.
    let mut seen: Vec<ObjectGuid> = Vec::new();
    for player in &nearby_players {
        if exclude_players.contains(player) || seen.contains(player) {
            continue;
        }
        seen.push(*player);

        if visibility && !shims::player_adminvision(w, *player) {
            continue;
        }

        if let Some(session) = shims::player_session(w, *player) {
            game_message::enqueue_send_many(w, session, msgs.iter().cloned());
        }
    }
    Some(nearby_players)
}

// ACE: WorldObject.NotifyPlayers
/// Called when a new PhysicsObj enters the world: sends the create object to the players who can
/// see it (`Player.AddTrackedObject`), and a non-player creature checks its targets.
pub fn notify_players(w: &mut World, this: ObjectGuid) {
    // send create object network message to visible players
    for player in known_players(w, this) {
        shims::player_add_tracked_object(w, player, this);
    }

    let o = obj(w, this);
    if o.is_creature() && !o.is_player() {
        shims::creature_check_targets(w, this);
    }
}

// ================================================================================ shims

/// Named stand-ins for members of other ACE files that this file's code calls. Each is named after the ACE member (`<class>_<member>`) and carries no ledger
/// anchor. A shim either reads the data where it already lives (so it is faithful now), or is a
/// `not_ported!` pointer that answers ACE's value for the common case (no physics body, no
/// enchantments, the property manager's fallback).
/// `PhysicsObj.ObjMaint.GetKnownPlayersValuesAsPlayer()`: nobody without a physics body (ACE's
/// callers return first in that case).
fn known_players(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let Some(h) = w.objects.get(this).and_then(|o| o.phys) else {
        return Vec::new();
    };
    crate::physics::object_maint::get_known_players_values_as_player(w, h)
}

/// Not ACE's (retail captures, V286): the players an object's updates
/// and deletes reach. Every player in the object's landblock and its adjacent landblocks (the 3×3
/// outdoors; a dungeon has no adjacents), whether or not that player has been sent the object or
/// has since forgotten it; nobody past them. The object itself is left out (each broadcast sends
/// to itself separately). An object off the landblock lists (a wielded item, one just taken out
/// of the world) reaches from its wielder's or container's landblock, or else from the landblock
/// of its last location; one with no landblock at all falls back to the players who know it.
/// ACE sent every broadcast to the known players only.
pub fn reach_players(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let Some(centre) = reach_landblock(w, this) else {
        return known_players(w, this);
    };
    let lm = &w.landblock_manager.landblocks;
    let Some(centre_lb) = lm.get(centre) else {
        return known_players(w, this);
    };
    let mut blocks = vec![centre];
    blocks.extend(
        centre_lb
            .adjacents
            .iter()
            .copied()
            .filter(|&a| lm.get(a).is_some()),
    );

    let mut players: Vec<ObjectGuid> = Vec::new();
    for &block in &blocks {
        let l = lm.get(block).expect("filtered above");
        // a player just moved in is still in the landblock's pending additions until its tick
        for &g in l.world_object_guids().chain(l.pending_addition_guids()) {
            if g == this || players.contains(&g) {
                continue;
            }
            let Some(o) = w.objects.get(g) else { continue };
            // one leaving for another landblock is listed in both until the tick: count it where
            // it is now
            if o.is_player()
                && o.current_landblock
                    .is_some_and(|c| blocks.iter().any(|b| same_landblock(*b, c)))
            {
                players.push(g);
            }
        }
    }
    players.sort_by_key(|g| g.full());
    players
}

fn same_landblock(a: empyrean_entity::LandblockId, b: empyrean_entity::LandblockId) -> bool {
    a.landblock_x() == b.landblock_x() && a.landblock_y() == b.landblock_y()
}

/// The landblock [`reach_players`] measures from.
fn reach_landblock(w: &World, this: ObjectGuid) -> Option<empyrean_entity::LandblockId> {
    let mut at = this;
    // wielder / container chains are short; the bound only guards a cycle
    for _ in 0..8 {
        let o = w.objects.get(at)?;
        if let Some(lb) = o.current_landblock {
            return Some(lb);
        }
        match o.wielder.or(o.wo.world_object_properties.container) {
            Some(next) if next != at => at = next,
            _ => break,
        }
    }
    let lb = w.objects.get(this)?.location()?.landblock_id();
    (lb.landblock_x() < 255
        && lb.landblock_y() < 255
        && w.landblock_manager.landblocks.get(lb).is_some())
    .then_some(lb)
}

pub mod shims {
    use std::sync::Arc;

    use dereth_assets::motion::PaletteTemplate;
    use dereth_assets::ClothingTable;
    use empyrean_entity::enums::{
        CharacterOption, ChatMessageType, DamageType, EnchantmentTypeFlags, ImbuedEffectType,
        MagicSchool, MotionCommand, MotionStance, PropertyAttribute, PropertyAttribute2nd,
        PropertyFloat, PropertyInt, Skill, SpellCategory, WeenieType,
    };
    use empyrean_entity::ObjectGuid;
    use empyrean_net::SessionId;
    use empyrean_store::models::shard::Character;

    use crate::network::motion::movement_data::Motion;
    use crate::network::structure::weapon_profile::WeaponProfile;
    use crate::world_objects::world_object::WorldObject;
    use crate::World;

    // ---- the object itself ----------------------------------------------------------------

    /// `WorldObject.CurrentMotionState` (its field in `WorldObjectPropertiesFields`).
    #[must_use]
    pub fn current_motion_state(o: &WorldObject) -> Option<&Motion> {
        o.wo.world_object_properties.current_motion_state.as_ref()
    }

    /// `WorldObject.CurrentMotionState = value`.
    pub fn set_current_motion_state(w: &mut World, this: ObjectGuid, value: Option<Motion>) {
        if let Some(o) = w.objects.get_mut(this) {
            o.wo.world_object_properties.current_motion_state = value;
        }
    }

    /// `WorldObject.Name`, virtual.
    #[must_use]
    pub fn name(w: &World, this: ObjectGuid) -> Option<String> {
        w.objects.get(this)?;
        crate::dispatch::name::name(w, this)
    }

    /// `WorldObject.SetStackSize(int? value)` (`WorldObject_Properties.cs`, 4.5a's port in `stackable.rs`).
    pub fn set_stack_size(o: &mut WorldObject, value: Option<i32>) {
        o.set_stack_size(value);
    }

    /// `WorldObject.IsEnchantable` (`WorldObject_Weapon.cs`): `(ResistMagic ?? 0) < 9999`.
    #[must_use]
    pub fn is_enchantable(w: &World, wo: ObjectGuid) -> bool {
        w.objects
            .get(wo)
            .is_some_and(|o| o.resist_magic().unwrap_or(0) < 9999)
    }

    /// `WorldObject.CSetup`: the setup model from the portal dat.
    #[must_use]
    pub fn c_setup(
        w: &World,
        setup_table_id: u32,
    ) -> Option<Arc<empyrean_dat::file_types::SetupModel>> {
        w.dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::SetupModel>(setup_table_id)
    }

    /// `DatManager.PortalDat.ReadFromDat<ClothingTable>(id)`.
    #[must_use]
    pub fn read_clothing_table(w: &World, id: u32) -> Option<Arc<ClothingTable>> {
        w.dats.portal_dat().read_from_dat::<ClothingTable>(id)
    }

    /// `item.ClothingSubPalEffects[item.ClothingSubPalEffects.Keys.ElementAt(0)]`: the first
    /// palette template in the dat file's order (ACE's dictionary keeps file order; the shared
    /// decoder's map is sorted). Read from the raw file when the dat has one; a table that exists
    /// only as a decoded object (synthetic test dats) is taken in key order.
    #[must_use]
    pub fn first_palette_template(
        w: &World,
        clothing_table_id: u32,
        item: &ClothingTable,
    ) -> PaletteTemplate {
        let key = w
            .dats
            .portal_dat()
            .get_reader_for_file(clothing_table_id)
            .and_then(|bytes| first_palette_template_key(&bytes))
            .filter(|k| item.palette_templates.contains_key(k));
        match key {
            Some(k) => item.palette_templates[&k].clone(),
            None => item
                .palette_templates
                .values()
                .next()
                .cloned()
                .expect("ACE: ElementAt(0) of an empty dictionary"),
        }
    }

    /// The first key of a raw `ClothingTable`'s palette-template table.
    fn first_palette_template_key(bytes: &[u8]) -> Option<u32> {
        let mut r = empyrean_common::dotnet::binary_reader::BinaryReader::new(bytes);
        r.read_u32().ok()?; // id
        let count = r.read_u16().ok()?;
        r.read_u16().ok()?; // buckets
        for _ in 0..count {
            r.read_u32().ok()?; // setup
            let n = r.read_u32().ok()?;
            for _ in 0..n {
                r.read_u32().ok()?; // part
                r.read_u32().ok()?; // object
                let nt = r.read_u32().ok()?;
                for _ in 0..nt {
                    r.read_u32().ok()?;
                    r.read_u32().ok()?;
                }
            }
        }
        let count2 = r.read_u16().ok()?;
        r.read_u16().ok()?;
        if count2 == 0 {
            return None;
        }
        r.read_u32().ok()
    }

    /// `DatManager.PortalDat.ReadFromDat<PaletteSet>(id).GetPaletteID(hue)`: a missing file is
    /// ACE's empty set (palette 0).
    #[must_use]
    pub fn palette_set_get_palette_id(w: &World, palette_set: u32, hue: f64) -> u32 {
        use empyrean_dat::file_types::palette_set::PaletteSetExt;
        w.dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::PaletteSet>(palette_set)
            .map_or(0, |p| p.get_palette_id(hue))
    }

    /// `WorldObject.setVisualClothingPriority()` (`WorldObject_Properties.cs`).
    pub fn set_visual_clothing_priority(w: &mut World, wo: ObjectGuid) {
        crate::world_objects::world_object_properties::set_visual_clothing_priority(w, wo);
    }

    /// `WorldObject.WeenieClassName` (`WorldObject_Properties.cs`).
    #[must_use]
    pub fn weenie_class_name(w: &World, wcid: u32) -> String {
        crate::world_objects::world_object_properties::weenie_class_name(
            &|id| w.content.get_cached_weenie(id),
            wcid,
        )
    }

    /// `DatabaseManager.World.GetCachedWeenie(wcid).GetValue()`.
    #[must_use]
    pub fn weenie_get_value(w: &World, wcid: u32) -> Option<i32> {
        let weenie = w
            .content
            .get_cached_weenie(wcid)
            .expect("ACE: GetCachedWeenie returned null");
        weenie.get_property(PropertyInt::Value)
    }

    /// `DatabaseManager.World.GetCachedWeenie(wcid).GetProperty(PropertyInt)`.
    #[must_use]
    pub fn weenie_get_property_int(w: &World, wcid: u32, property: PropertyInt) -> Option<i32> {
        w.content
            .get_cached_weenie(wcid)
            .expect("ACE: GetCachedWeenie returned null")
            .get_property(property)
    }

    /// `WorldObjectFactory.CreateNewWorldObject(string weenieClassName)`: returned, not added to
    /// `World.objects`.
    #[must_use]
    pub fn world_object_factory_create_new_world_object_by_name(
        w: &mut World,
        class_name: &str,
    ) -> Option<WorldObject> {
        crate::factories::world_object_factory::create_new_world_object_by_name_detached(
            w, class_name,
        )
    }

    /// `WorldObject.GetRemainingLifespan()` (`WorldObject_Decay.cs`).
    #[must_use]
    pub fn get_remaining_lifespan(w: &World, wo: ObjectGuid) -> i32 {
        w.objects
            .get(wo)
            .expect("ACE: wo is null")
            .get_remaining_lifespan(w.now.utc)
    }

    /// `WorldObject.HasImbuedEffect(ImbuedEffectType)` (`WorldObject_Weapon.cs`).
    ///
    /// # Panics
    /// When `wo` is gone (ACE: `NullReferenceException`).
    #[must_use]
    pub fn has_imbued_effect(w: &World, wo: ObjectGuid, r#type: ImbuedEffectType) -> bool {
        let o = w
            .objects
            .get(wo)
            .expect("ACE: wo is null (NullReferenceException)");
        crate::world_objects::world_object_weapon::has_imbued_effect(o, r#type)
    }

    // ---- physics ---------------------------------------------------------------

    /// `CurrentLandblock.IsDungeon` (`Entity/Landblock.cs`); a null landblock throws in ACE.
    #[must_use]
    pub fn current_landblock_is_dungeon(w: &mut World, this: ObjectGuid) -> bool {
        let id = w
            .objects
            .get(this)
            .and_then(|o| o.current_landblock)
            .expect("ACE: CurrentLandblock is null (NullReferenceException)");
        w.landblock_manager
            .landblocks
            .get_mut(id)
            .expect("ACE: CurrentLandblock is not loaded")
            .is_dungeon()
    }

    /// `Physics.Animation.MotionTable.GetAnimationLength(motionTableId, stance, [prevCommand,]
    /// motion, speed)` (the physics `motion_table`). With `second` the call is the four-command
    /// overload `(stance, first, second)`.
    #[must_use]
    pub fn motion_table_get_animation_length(
        w: &World,
        motion_table_id: u32,
        stance: MotionStance,
        first: MotionCommand,
        second: Option<MotionCommand>,
        speed: f32,
    ) -> f32 {
        match second {
            Some(second) => crate::physics::motion_table::get_animation_length_between(
                w,
                motion_table_id,
                stance,
                first,
                second,
                speed,
            ),
            None => crate::physics::motion_table::get_animation_length(
                w,
                motion_table_id,
                stance,
                first,
                speed,
            ),
        }
    }

    /// `MotionTable.GetAnimationLength(motionTableId, MotionStance prevStance, MotionCommand
    /// motion, MotionCommand currentStance, float speed)`.
    #[must_use]
    pub fn motion_table_get_animation_length_stances(
        w: &World,
        motion_table_id: u32,
        prev_stance: MotionStance,
        motion: MotionCommand,
        current_stance: MotionCommand,
        speed: f32,
    ) -> f32 {
        crate::physics::motion_table::get_animation_length_between(
            w,
            motion_table_id,
            prev_stance,
            motion,
            current_stance,
            speed,
        )
    }

    // ---- players and sessions -------------------------------------------------------------

    /// `Player.Session`: the session whose player this is.
    #[must_use]
    pub fn player_session(w: &World, player: ObjectGuid) -> Option<SessionId> {
        w.sessions
            .iter()
            .find(|(_, s)| s.player == Some(player))
            .map(|(id, _)| id)
    }

    /// `player.Session.Account` (the account name).
    #[must_use]
    pub fn player_session_account(w: &World, player: ObjectGuid) -> Option<String> {
        let id = player_session(w, player)?;
        w.sessions.get(id)?.account.clone()
    }

    /// `Player.Character` (shim storage in `PlayerNetworkingFields`).
    #[must_use]
    pub fn player_character(o: &WorldObject) -> Option<&Character> {
        o.player.as_ref()?.player.character.as_ref()
    }

    /// `Player.GetCharacterOption(CharacterOption)` (`Player_Character.cs`).
    #[must_use]
    pub fn player_get_character_option(
        w: &World,
        player: ObjectGuid,
        option: CharacterOption,
    ) -> bool {
        crate::world_objects::player_character::get_character_option(w, player, option)
    }

    /// `Player.IsAdmin || IsSentinel || IsEnvoy || IsArch || IsPsr` (`Player.cs`).
    #[must_use]
    pub fn player_is_staff(w: &World, player: ObjectGuid) -> bool {
        w.objects.get(player).is_some_and(|o| {
            o.is_admin_prop() || o.is_sentinel_prop() || o.is_envoy() || o.is_arch() || o.is_psr()
        })
    }

    /// `Player.Adminvision`.
    #[must_use]
    pub fn player_adminvision(w: &World, player: ObjectGuid) -> bool {
        w.objects
            .get(player)
            .and_then(|o| o.player.as_ref())
            .is_some_and(|p| p.player.adminvision)
    }

    /// `Player.FastTick`.
    #[must_use]
    pub fn player_fast_tick(w: &World, player: ObjectGuid) -> bool {
        crate::world_objects::player_tick::fast_tick(w, player)
    }

    /// `player.MagicState.IsCasting`.
    #[must_use]
    pub fn player_magic_state_is_casting(w: &World, player: ObjectGuid) -> bool {
        crate::world_objects::player_magic::fields(w, player)
            .magic_state
            .is_casting
    }

    /// `player.CombatMode == CombatMode.Missile`.
    #[must_use]
    pub fn player_combat_mode_is_missile(w: &World, player: ObjectGuid) -> bool {
        crate::world_objects::creature_combat::combat_mode(w, player)
            == empyrean_entity::enums::CombatMode::Missile
    }

    /// `player.SquelchManager.Squelches.Contains(source, type)`.
    #[must_use]
    pub fn player_squelches_contains(
        w: &World,
        player: ObjectGuid,
        source: ObjectGuid,
        r#type: ChatMessageType,
    ) -> bool {
        crate::world_objects::managers::squelch_manager::squelches_contains(
            w,
            player,
            Some(source),
            r#type,
        )
    }

    /// `Player.AddTrackedObject(WorldObject)`.
    pub fn player_add_tracked_object(w: &mut World, player: ObjectGuid, wo: ObjectGuid) {
        crate::world_objects::player_tracking::add_tracked_object(w, player, wo);
    }

    /// `Creature.CheckTargets()` (`Monster_Awareness.cs`).
    pub fn creature_check_targets(w: &mut World, creature: ObjectGuid) {
        crate::world_objects::monster_awareness::check_targets(w, creature);
    }

    /// `Creature.GetRunRate()` (`Monster_Navigation.cs`; it adds a missing Run skill record and
    /// reads the enchanted Run, hence `&mut World`).
    pub fn creature_get_run_rate(w: &mut World, creature: ObjectGuid) -> f32 {
        crate::world_objects::monster_navigation::get_run_rate(w, creature)
    }

    /// What `PlayerManager.FindByGuid` returns (an `IPlayer`), as far as this file reads it.
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct IPlayerView {
        pub guid: ObjectGuid,
        pub name: String,
        /// `Account.AccountName` (`null` for a missing account).
        pub account_name: Option<String>,
        /// `Account.AccountId`.
        pub account_id: Option<u32>,
        /// `GetProperty(PropertyFloat.LoginTimestamp)`.
        pub login_timestamp: Option<f64>,
    }

    /// `PlayerManager.FindByGuid(guid)`: an online player (in the world with a session), else an
    /// offline player from `PlayerManager`.
    #[must_use]
    pub fn player_manager_find_by_guid(w: &World, guid: u32) -> Option<IPlayerView> {
        let g = ObjectGuid::new(guid);
        match w.objects.get(g).filter(|o| o.is_player()) {
            Some(o) => {
                let session = player_session(w, g).and_then(|s| w.sessions.get(s));
                Some(IPlayerView {
                    guid: g,
                    name: crate::dispatch::name::name(w, g).unwrap_or_default(),
                    account_name: session.and_then(|s| s.account.clone()),
                    account_id: session.map(|s| s.account_id),
                    login_timestamp: o.get_property(PropertyFloat::LoginTimestamp),
                })
            }
            None => match crate::managers::player_manager::find_by_guid(w, guid).0 {
                Some(p @ crate::entity::i_player::IPlayer::Offline(_)) => Some(i_player_view(w, p)),
                _ => None,
            },
        }
    }

    /// The view of an `IPlayer` from `PlayerManager` (its name, account and login time).
    fn i_player_view(w: &World, p: crate::entity::i_player::IPlayer) -> IPlayerView {
        use crate::entity::i_player;

        let account = i_player::account(w, p);
        IPlayerView {
            guid: p.guid(),
            name: i_player::name(w, p).unwrap_or_default(),
            account_name: account.as_ref().map(|a| a.account_name.clone()),
            account_id: account.map(|a| a.account_id),
            login_timestamp: i_player::get_property(w, p, PropertyFloat::LoginTimestamp),
        }
    }

    /// `PlayerManager.GetAccountPlayers(accountId)`: guid -> player, in dictionary order.
    #[must_use]
    pub fn player_manager_get_account_players(
        w: &World,
        account_id: u32,
    ) -> Option<Vec<(u32, IPlayerView)>> {
        let players = crate::managers::player_manager::get_account_players(w, account_id)?;
        Some(
            players
                .iter()
                .map(|(guid, p)| (*guid, i_player_view(w, *p)))
                .collect(),
        )
    }

    /// `SquelchManager.IsLegalChannel(ChatMessageType)`.
    #[must_use]
    pub fn squelch_manager_is_legal_channel(r#type: ChatMessageType) -> bool {
        crate::world_objects::managers::squelch_manager::is_legal_channel(r#type)
    }

    /// `QuestManager.HasQuest(questName)`.
    #[must_use]
    pub fn quest_manager_has_quest(w: &World, player: ObjectGuid, quest: &str) -> bool {
        crate::managers::quest_manager::has_quest(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(player),
            quest,
        )
    }

    /// `QuestManager.GetNextSolveTime(questName).TotalSeconds`.
    #[must_use]
    pub fn quest_manager_get_next_solve_time_total_seconds(
        w: &World,
        player: ObjectGuid,
        quest: &str,
    ) -> f64 {
        crate::managers::quest_manager::get_next_solve_time(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(player),
            quest,
        )
        .total_seconds()
    }

    /// `QuestManager.GetQuest(questName).NumTimesCompleted`.
    #[must_use]
    pub fn quest_manager_get_quest_num_times_completed(
        w: &World,
        player: ObjectGuid,
        quest: &str,
    ) -> i32 {
        crate::managers::quest_manager::get_quest(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(player),
            quest,
        )
        .expect("ACE: GetQuest(questName) is null (NullReferenceException)")
        .num_times_completed
    }

    /// `PropertyManager.GetBool(key, fallback).Item`.
    #[must_use]
    pub fn property_manager_get_bool(w: &World, key: &str, fallback: bool) -> bool {
        crate::managers::property_manager::get_bool(w, key, fallback, true).item
    }

    /// `PropertyManager.GetLong(key, fallback).Item`.
    #[must_use]
    pub fn property_manager_get_long(w: &World, key: &str, fallback: i64) -> i64 {
        crate::managers::property_manager::get_long(w, key, fallback, true).item
    }

    // ---- allegiance and fellowship -------------------------------------------------------

    /// A monarch or patron as appraisal reads them.
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct AllegianceMemberView {
        pub heritage: Option<i32>,
        pub gender: Option<i32>,
        pub rank: u32,
        pub name: String,
    }

    /// What `AppraiseInfo.BuildProperties` reads from `player.Allegiance` / `AllegianceNode`.
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct AllegianceAppraisalView {
        pub allegiance_name: Option<String>,
        pub is_monarch: bool,
        pub total_followers: i32,
        pub monarch: AllegianceMemberView,
        pub patron: AllegianceMemberView,
    }

    /// `player.Allegiance != null && player.AllegianceNode != null`, and what is read from them.
    #[must_use]
    pub fn player_allegiance_appraisal(
        w: &World,
        player: ObjectGuid,
    ) -> Option<AllegianceAppraisalView> {
        crate::world_objects::allegiance::appraisal_view(w, player)
    }

    /// `player.Fellowship?.FellowshipName`.
    #[must_use]
    pub fn player_fellowship_name(w: &World, player: ObjectGuid) -> Option<String> {
        crate::world_objects::player_fellowship::fellowship(w, player)
            .map(|f| f.get(w).fellowship_name.clone())
    }

    // ---- houses ---------------------------------------------------------------------------

    /// `house.Guests` (a `Dictionary<ObjectGuid, bool>`, in dictionary order).
    #[must_use]
    pub fn house_guests(w: &World, house: ObjectGuid) -> Vec<(ObjectGuid, bool)> {
        crate::world_objects::house::fields(w, house)
            .guests
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect()
    }

    /// `house.HouseInstance`.
    #[must_use]
    pub fn house_instance(w: &World, house: ObjectGuid) -> Option<u32> {
        w.objects.get(house)?.house_instance()
    }

    /// `house.LinkedHouses[index].HouseOwner`.
    #[must_use]
    pub fn house_linked_house_owner(w: &World, house: ObjectGuid, index: usize) -> Option<u32> {
        let linked = *crate::world_objects::house::fields(w, house)
            .linked_houses
            .get(index)
            .expect("System.ArgumentOutOfRangeException: LinkedHouses[index]");
        w.objects.get(linked)?.house_owner()
    }

    /// `house.RootHouse`.
    #[must_use]
    pub fn house_root_house(w: &World, house: ObjectGuid) -> ObjectGuid {
        crate::world_objects::house::root_house_ref(w, house)
    }

    /// `slumlord.House`.
    #[must_use]
    pub fn slum_lord_house(w: &World, slumlord: ObjectGuid) -> Option<ObjectGuid> {
        crate::world_objects::slum_lord::house(w, slumlord)
    }

    /// `slumLord.IsRentPaid()`.
    #[must_use]
    pub fn slum_lord_is_rent_paid(w: &World, slumlord: ObjectGuid) -> bool {
        crate::world_objects::slum_lord::is_rent_paid_ref(w, slumlord)
    }

    /// `wo.ParentLink.HouseOwner`.
    #[must_use]
    pub fn parent_link_house_owner(w: &World, wo: ObjectGuid) -> Option<u32> {
        let parent = w.objects.get(wo)?.wo.world_object_links.parent_link?;
        w.objects.get(parent)?.house_owner()
    }

    /// `wo.ParentLink.HouseOwnerName`.
    #[must_use]
    pub fn parent_link_house_owner_name(w: &World, wo: ObjectGuid) -> Option<String> {
        let parent = w.objects.get(wo)?.wo.world_object_links.parent_link?;
        w.objects.get(parent)?.house_owner_name()
    }

    /// `wo.ParentLink.HouseOwnerName = name; wo.ParentLink.SaveBiotaToDatabase();`.
    pub fn parent_link_set_house_owner_name_and_save(w: &mut World, wo: ObjectGuid, name: String) {
        let parent = w
            .objects
            .get(wo)
            .and_then(|o| o.wo.world_object_links.parent_link)
            .expect("ACE: WorldObject.ParentLink is null (NullReferenceException)");
        if let Some(p) = w.objects.get_mut(parent) {
            p.set_house_owner_name(Some(name));
        }
        crate::dispatch::save_biota_to_database::save_biota_to_database(w, parent, true);
    }

    /// `hook.HasItem ? hook.Item : null`.
    #[must_use]
    pub fn hook_item(w: &World, hook: ObjectGuid) -> Option<ObjectGuid> {
        crate::world_objects::hook::item(w, hook)
    }

    // ---- locks, skills, ratings, vitals --------------------------------------------------

    /// `LockHelper.GetResistLockpick(wo)`.
    #[must_use]
    pub fn lock_helper_get_resist_lockpick(w: &World, wo: ObjectGuid) -> Option<i32> {
        crate::world_objects::lock::get_resist_lockpick_view(w, wo)
    }

    /// `creature.Skills[skill].Current`.
    #[must_use]
    pub fn creature_skill_current(w: &mut World, creature: ObjectGuid, skill: Skill) -> u32 {
        crate::world_objects::world_object_magic::creature_get_creature_skill_current(
            w, creature, skill,
        )
    }

    /// `creature.<Attribute>.Current` (base + enchantments).
    #[must_use]
    pub fn creature_attribute_current(
        w: &mut World,
        creature: ObjectGuid,
        attribute: PropertyAttribute,
    ) -> u32 {
        crate::world_objects::creature_combat::attribute_current(w, creature, attribute)
    }

    /// `creature.<Vital>.Current`: the biota's `CurrentLevel` (0 when the creature has no entry,
    /// which ACE's constructor would have created).
    #[must_use]
    pub fn creature_vital_current(
        w: &World,
        creature: ObjectGuid,
        vital: PropertyAttribute2nd,
    ) -> u32 {
        w.objects
            .get(creature)
            .and_then(|o| {
                o.biota
                    .properties_attribute_2nd
                    .as_ref()?
                    .get(&vital)
                    .map(|v| v.current_level)
            })
            .unwrap_or(0)
    }

    /// `creature.<Vital>.MaxValue` (base + attribute formula + enchantments).
    #[must_use]
    pub fn creature_vital_max_value(
        w: &mut World,
        creature: ObjectGuid,
        vital: PropertyAttribute2nd,
    ) -> u32 {
        crate::world_objects::world_object_magic::vital_max_value(w, creature, vital)
    }

    /// The ratings `AppraiseInfo.AddRatings` reads (each a `Creature_Rating.cs` getter).
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct CreatureRatings {
        pub damage_rating: i32,
        pub damage_resist_rating: i32,
        pub crit_rating: i32,
        pub crit_damage_rating: i32,
        pub crit_resist_rating: i32,
        pub crit_damage_resist_rating: i32,
        pub healing_boost_rating: i32,
        pub dot_resist_rating: i32,
        pub nether_resist_rating: i32,
        pub life_resist_rating: i32,
        pub gear_max_health: i32,
        pub pk_damage_rating: i32,
        pub pk_damage_resist_rating: i32,
    }

    /// `AddRatings`' reads: `GetDamageRating()` (+5 with the heritage bonus of the equipped
    /// weapon or wand), `GetDamageResistRating()`, ... in ACE's order.
    #[must_use]
    pub fn creature_ratings(w: &mut World, creature: ObjectGuid) -> CreatureRatings {
        use crate::world_objects::creature_combat::shim;
        use crate::world_objects::creature_equipment;
        use crate::world_objects::creature_rating as rating;

        let mut damage_rating = rating::get_damage_rating(w, creature);

        // include heritage / weapon type rating?
        let weapon = creature_equipment::get_equipped_weapon(w, creature, false)
            .or_else(|| creature_equipment::get_equipped_wand(w, creature));
        if shim::virt_get_heritage_bonus(w, creature, weapon) {
            damage_rating = damage_rating.wrapping_add(5);
        }

        // factor in weakness here?

        let damage_resist_rating = rating::get_damage_resist_rating(w, creature, None, true);

        // factor in nether dot damage here?

        let crit_rating = rating::get_crit_rating(w, creature);
        let crit_damage_rating = rating::get_crit_damage_rating(w, creature);

        let crit_resist_rating = rating::get_crit_resist_rating(w, creature);
        let crit_damage_resist_rating = rating::get_crit_damage_resist_rating(w, creature);

        let healing_boost_rating = rating::get_healing_boost_rating(w, creature);
        let dot_resist_rating = rating::get_dot_resistance_rating(w, creature);
        let nether_resist_rating = rating::get_nether_resist_rating(w, creature);

        let life_resist_rating = rating::get_life_resist_rating(w, creature); // drain / harm resistance
        let gear_max_health = rating::get_gear_max_health(w, creature);

        let pk_damage_rating = rating::get_pk_damage_rating(w, creature);
        let pk_damage_resist_rating = rating::get_pk_damage_resist_rating(w, creature);

        CreatureRatings {
            damage_rating,
            damage_resist_rating,
            crit_rating,
            crit_damage_rating,
            crit_resist_rating,
            crit_damage_resist_rating,
            healing_boost_rating,
            dot_resist_rating,
            nether_resist_rating,
            life_resist_rating,
            gear_max_health,
            pk_damage_rating,
            pk_damage_resist_rating,
        }
    }

    // ---- enchantment manager and the mask helpers ----------------------------------------

    /// `wo.EnchantmentManager.GetArmorMod()`.
    #[must_use]
    pub fn enchantment_manager_get_armor_mod(w: &World, wo: ObjectGuid) -> i32 {
        crate::world_objects::managers::enchantment_manager::get_armor_mod(w, wo)
    }

    /// `EnchantmentManager.GetImpenBaneKey(DamageType)`: the resistance property of a damage
    /// type (a pure mapping in ACE).
    #[must_use]
    pub fn enchantment_manager_get_impen_bane_key(damage_type: DamageType) -> PropertyFloat {
        match damage_type {
            DamageType::Slash => PropertyFloat::ArmorModVsSlash,
            DamageType::Pierce => PropertyFloat::ArmorModVsPierce,
            DamageType::Bludgeon => PropertyFloat::ArmorModVsBludgeon,
            DamageType::Fire => PropertyFloat::ArmorModVsFire,
            DamageType::Cold => PropertyFloat::ArmorModVsCold,
            DamageType::Acid => PropertyFloat::ArmorModVsAcid,
            DamageType::Electric => PropertyFloat::ArmorModVsElectric,
            DamageType::Nether => PropertyFloat::ArmorModVsNether,
            _ => PropertyFloat(0),
        }
    }

    /// `wo.EnchantmentManager.GetArmorModVsType(DamageType)`.
    #[must_use]
    pub fn enchantment_manager_get_armor_mod_vs_type(
        w: &World,
        wo: ObjectGuid,
        damage_type: DamageType,
    ) -> f32 {
        crate::world_objects::managers::enchantment_manager::get_armor_mod_vs_type(
            w,
            wo,
            damage_type,
        )
    }

    /// `EnchantmentManager.GetDamageBonus()`.
    #[must_use]
    pub fn enchantment_manager_get_damage_bonus(w: &World, wo: ObjectGuid) -> i32 {
        crate::world_objects::managers::enchantment_manager::get_damage_bonus(w, wo)
    }

    /// `EnchantmentManager.GetWeaponSpeedMod()`.
    #[must_use]
    pub fn enchantment_manager_get_weapon_speed_mod(w: &World, wo: ObjectGuid) -> i32 {
        crate::world_objects::managers::enchantment_manager::get_weapon_speed_mod(w, wo)
    }

    /// `EnchantmentManager.GetVarianceMod()` (multiplicative: 1 without enchantments).
    #[must_use]
    pub fn enchantment_manager_get_variance_mod(w: &World, wo: ObjectGuid) -> f32 {
        crate::world_objects::managers::enchantment_manager::get_variance_mod(w, wo)
    }

    /// `EnchantmentManager.GetDamageMod()`.
    #[must_use]
    pub fn enchantment_manager_get_damage_mod(w: &World, wo: ObjectGuid) -> f32 {
        crate::world_objects::managers::enchantment_manager::get_damage_mod(w, wo)
    }

    /// `EnchantmentManager.GetAttackMod()`.
    #[must_use]
    pub fn enchantment_manager_get_attack_mod(w: &World, wo: ObjectGuid) -> f32 {
        crate::world_objects::managers::enchantment_manager::get_attack_mod(w, wo)
    }

    /// `EnchantmentManager.GetDefenseMod()`.
    #[must_use]
    pub fn enchantment_manager_get_defense_mod(w: &World, wo: ObjectGuid) -> f32 {
        crate::world_objects::managers::enchantment_manager::get_defense_mod(w, wo)
    }

    /// An enchantment as appraisal reads it (`SpellId`, `SpellCategory`).
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct EnchantmentView {
        pub spell_id: i32,
        pub spell_category: SpellCategory,
    }

    /// `EnchantmentManager.GetEnchantments(MagicSchool)`.
    #[must_use]
    pub fn enchantment_manager_get_enchantments(
        w: &World,
        wo: ObjectGuid,
        school: MagicSchool,
    ) -> Vec<EnchantmentView> {
        crate::world_objects::managers::enchantment_manager::get_enchantments_school(w, wo, school)
            .iter()
            .map(|e| EnchantmentView {
                spell_id: e.spell_id,
                spell_category: e.spell_category,
            })
            .collect()
    }

    /// `EnchantmentManager.HasEnchantments`: the biota has registry entries.
    #[must_use]
    pub fn enchantment_manager_has_enchantments(w: &World, wo: ObjectGuid) -> bool {
        w.objects.get(wo).is_some_and(|o| {
            empyrean_entity::models::properties_enchantment_registry_extensions::has_enchantments(
                o.biota.properties_enchantment_registry.as_ref(),
            )
        })
    }

    /// `EnchantmentManager.Dispel(PropertiesEnchantmentRegistry)`.
    pub fn enchantment_manager_dispel(
        w: &mut World,
        wo: ObjectGuid,
        entry: Option<empyrean_entity::models::PropertiesEnchantmentRegistry>,
    ) {
        crate::world_objects::managers::enchantment_manager_with_caching::dispel(
            w,
            wo,
            entry.as_ref(),
        );
    }

    /// `ACE.Server.Network.Enum.ArmorMask` members.
    mod armor_mask {
        pub const ARMOR_LEVEL: u32 = 0x1;
        pub const SLASHING_PROTECTION: u32 = 0x2;
        pub const PIERCING_PROTECTION: u32 = 0x4;
        pub const BLUDGEONING_PROTECTION: u32 = 0x8;
        pub const COLD_PROTECTION: u32 = 0x10;
        pub const FIRE_PROTECTION: u32 = 0x20;
        pub const ACID_PROTECTION: u32 = 0x40;
        pub const LIGHTNING_PROTECTION: u32 = 0x80;
    }

    /// `ACE.Server.Network.Enum.ResistMask` members.
    mod resist_mask {
        pub const RESIST_SLASH: u32 = 0x1;
        pub const RESIST_PIERCE: u32 = 0x2;
        pub const RESIST_BLUDGEON: u32 = 0x4;
        pub const RESIST_FIRE: u32 = 0x8;
        pub const RESIST_COLD: u32 = 0x10;
        pub const RESIST_ELECTRIC: u32 = 0x40;
        pub const RESIST_HEALTH_BOOST: u32 = 0x80;
        pub const RESIST_STAMINA_DRAIN: u32 = 0x100;
        pub const RESIST_STAMINA_BOOST: u32 = 0x200;
        pub const RESIST_MANA_DRAIN: u32 = 0x400;
        pub const RESIST_MANA_BOOST: u32 = 0x800;
        pub const MANA_CONVERSION_MOD: u32 = 0x1000;
        pub const ELEMENTAL_DAMAGE_MOD: u32 = 0x2000;
        pub const RESIST_NETHER: u32 = 0x4000;
    }

    /// `ACE.Server.Network.Enum.AttributeMask` members.
    mod attribute_mask {
        pub const STRENGTH: u32 = 0x1;
        pub const ENDURANCE: u32 = 0x2;
        pub const QUICKNESS: u32 = 0x4;
        pub const COORDINATION: u32 = 0x8;
        pub const FOCUS: u32 = 0x10;
        pub const SELF: u32 = 0x20;
        pub const HEALTH: u32 = 0x40;
        pub const STAMINA: u32 = 0x80;
        pub const MANA: u32 = 0x100;
    }

    /// The armor enchantment reads of the two armor mask helpers, in ACE's order: the armor
    /// level mod, then the protection mods vs slash, pierce, bludgeon, cold, fire, acid and
    /// electric. `armor.EnchantmentManager` is the caching manager, whose values are the base
    /// manager's.
    fn armor_mods(w: &World, armor: ObjectGuid) -> (i32, [(u32, f32); 7]) {
        use crate::world_objects::managers::enchantment_manager as em;
        let vs = |t: DamageType| em::get_armor_mod_vs_type(w, armor, t);
        (
            em::get_armor_mod(w, armor),
            [
                (armor_mask::SLASHING_PROTECTION, vs(DamageType::Slash)),
                (armor_mask::PIERCING_PROTECTION, vs(DamageType::Pierce)),
                (armor_mask::BLUDGEONING_PROTECTION, vs(DamageType::Bludgeon)),
                (armor_mask::COLD_PROTECTION, vs(DamageType::Cold)),
                (armor_mask::FIRE_PROTECTION, vs(DamageType::Fire)),
                (armor_mask::ACID_PROTECTION, vs(DamageType::Acid)),
                (armor_mask::LIGHTNING_PROTECTION, vs(DamageType::Electric)),
            ],
        )
    }

    /// Determines if there is a highlight for each armor protection vs. damage type (0 for a null
    /// armor).
    // ACE: ArmorMaskHelper.GetHighlightMask
    #[must_use]
    pub fn armor_mask_helper_get_highlight_mask(w: &World, wo: ObjectGuid) -> u32 {
        let mut highlight_mask = 0;

        if w.objects.get(wo).is_none() {
            return highlight_mask;
        }

        // item enchanments are currently being cast on wielder
        let (armor_mod, vs) = armor_mods(w, wo);
        if armor_mod != 0 {
            highlight_mask |= armor_mask::ARMOR_LEVEL;
        }
        for (bit, value) in vs {
            if value != 0.0 {
                highlight_mask |= bit;
            }
        }

        highlight_mask
    }

    /// Determines the red/green color for each armor protection vs. damage type.
    // ACE: ArmorMaskHelper.GetColorMask
    #[must_use]
    pub fn armor_mask_helper_get_color_mask(w: &World, wo: ObjectGuid) -> u32 {
        let mut color_mask = 0;

        if w.objects.get(wo).is_none() {
            return color_mask;
        }

        let (armor_mod, vs) = armor_mods(w, wo);
        if armor_mod > 0 {
            color_mask |= armor_mask::ARMOR_LEVEL;
        }
        for (bit, value) in vs {
            if value > 0.0 {
                color_mask |= bit;
            }
        }

        color_mask
    }

    /// The wielder's resistance mods the two resist mask helpers read, in ACE's order, each with
    /// its bit. ACE-BUG (below): Acid is never read, so an acid resistance buff is never shown,
    /// and Stamina and Mana set both the drain and the boost bits.
    fn wielder_resistance_mods(w: &World, wielder: ObjectGuid) -> [(u32, f32); 12] {
        use crate::world_objects::managers::enchantment_manager as em;
        let r = |t: DamageType| em::get_resistance_mod(w, wielder, t);
        [
            (resist_mask::RESIST_SLASH, r(DamageType::Slash)),
            (resist_mask::RESIST_PIERCE, r(DamageType::Pierce)),
            (resist_mask::RESIST_BLUDGEON, r(DamageType::Bludgeon)),
            (resist_mask::RESIST_FIRE, r(DamageType::Fire)),
            (resist_mask::RESIST_COLD, r(DamageType::Cold)),
            (resist_mask::RESIST_ELECTRIC, r(DamageType::Electric)),
            (resist_mask::RESIST_HEALTH_BOOST, r(DamageType::Health)), // ??
            (resist_mask::RESIST_STAMINA_DRAIN, r(DamageType::Stamina)),
            (resist_mask::RESIST_STAMINA_BOOST, r(DamageType::Stamina)),
            (resist_mask::RESIST_MANA_DRAIN, r(DamageType::Mana)),
            (resist_mask::RESIST_MANA_BOOST, r(DamageType::Mana)),
            (resist_mask::RESIST_NETHER, r(DamageType::Nether)),
        ]
    }

    /// `weapon.Wielder`.
    fn wielder_of(w: &World, weapon: ObjectGuid) -> Option<ObjectGuid> {
        w.objects
            .get(weapon)
            .and_then(|o| o.wielder)
            .filter(|&g| w.objects.contains(g))
    }

    // ACE: ResistMaskHelper.GetHighlightMask
    // ACE-BUG: the wielder's Acid resistance is never checked (no ResistAcid bit is ever set), and Stamina / Mana each set both their Drain and Boost bits from the same resistance mod.
    #[must_use]
    pub fn resist_mask_helper_get_highlight_mask(w: &World, wo: ObjectGuid) -> u32 {
        let mut highlight_mask = 0;

        if let Some(wielder) = wielder_of(w, wo) {
            for (bit, value) in wielder_resistance_mods(w, wielder) {
                if value != 1.0 {
                    highlight_mask |= bit;
                }
            }
        }

        // ManaConversionMod and ElementalDamageMod are only needed for weapons
        let mana_conversion_mod = resist_mask_helper_get_mana_conversion_mod(w, wo);

        if mana_conversion_mod != 1.0 {
            highlight_mask |= resist_mask::MANA_CONVERSION_MOD;
        }

        let elemental_damage_mod = resist_mask_helper_get_elemental_damage_bonus(w, wo);

        if elemental_damage_mod != 0.0 {
            highlight_mask |= resist_mask::ELEMENTAL_DAMAGE_MOD;
        }

        highlight_mask
    }

    // ACE: ResistMaskHelper.GetColorMask
    #[must_use]
    pub fn resist_mask_helper_get_color_mask(w: &World, wo: ObjectGuid) -> u32 {
        let mut color_mask = 0;

        if let Some(wielder) = wielder_of(w, wo) {
            for (bit, value) in wielder_resistance_mods(w, wielder) {
                if value > 1.0 {
                    color_mask |= bit;
                }
            }
        }

        // ManaConversionMod and ElementalDamageMod are only needed for weapons
        let mana_conversion_mod = resist_mask_helper_get_mana_conversion_mod(w, wo);

        if mana_conversion_mod > 1.0 {
            color_mask |= resist_mask::MANA_CONVERSION_MOD;
        }

        let elemental_damage_mod = resist_mask_helper_get_elemental_damage_bonus(w, wo);

        if elemental_damage_mod > 0.0 {
            color_mask |= resist_mask::ELEMENTAL_DAMAGE_MOD;
        }

        color_mask
    }

    /// The wielder's (for an enchantable weapon) times the weapon's own mana conversion mod.
    // ACE: ResistMaskHelper.GetManaConversionMod
    #[must_use]
    pub fn resist_mask_helper_get_mana_conversion_mod(w: &World, wo: ObjectGuid) -> f32 {
        use crate::world_objects::managers::enchantment_manager as em;
        let wielder_mana_conv_mod = match wielder_of(w, wo) {
            Some(wielder) if is_enchantable(w, wo) => em::get_mana_conv_mod(w, wielder),
            _ => 1.0,
        };
        let weapon_mana_conv_mod = if w.objects.get(wo).is_some() {
            em::get_mana_conv_mod(w, wo)
        } else {
            1.0
        };

        wielder_mana_conv_mod * weapon_mana_conv_mod
    }

    /// The wielder's (for an enchantable weapon) plus the weapon's own elemental damage mod.
    // ACE: ResistMaskHelper.GetElementalDamageBonus
    #[must_use]
    pub fn resist_mask_helper_get_elemental_damage_bonus(w: &World, wo: ObjectGuid) -> f32 {
        use crate::world_objects::managers::enchantment_manager as em;
        let wielder_elemental_damage_mod = match wielder_of(w, wo) {
            Some(wielder) if is_enchantable(w, wo) => em::get_elemental_damage_mod(w, wielder),
            _ => 0.0,
        };
        let weapon_elemental_damage_mod = if w.objects.get(wo).is_some() {
            em::get_elemental_damage_mod(w, wo)
        } else {
            0.0
        };

        wielder_elemental_damage_mod + weapon_elemental_damage_mod
    }

    /// Each attribute and vital's `ModifierType`, in ACE's order, with its bit.
    fn attribute_modifier_types(
        w: &mut World,
        creature: ObjectGuid,
    ) -> [(u32, empyrean_entity::enums::ModifierType); 9] {
        use crate::world_objects::entity::creature_attribute::StatCtx;
        use empyrean_entity::enums::ModifierType;
        let o = w
            .objects
            .get(creature)
            .expect("ACE: creature is null (NullReferenceException)");
        let attr = |a: PropertyAttribute| {
            *o.attributes()
                .get(&a)
                .expect("Creature.Attributes: every attribute is present")
        };
        let attributes = [
            (attribute_mask::STRENGTH, attr(PropertyAttribute::Strength)),
            (
                attribute_mask::ENDURANCE,
                attr(PropertyAttribute::Endurance),
            ),
            (
                attribute_mask::QUICKNESS,
                attr(PropertyAttribute::Quickness),
            ),
            (
                attribute_mask::COORDINATION,
                attr(PropertyAttribute::Coordination),
            ),
            (attribute_mask::FOCUS, attr(PropertyAttribute::Focus)),
            (attribute_mask::SELF, attr(PropertyAttribute::Self_)),
        ];
        let vitals = [
            (attribute_mask::HEALTH, o.health()),
            (attribute_mask::STAMINA, o.stamina()),
            (attribute_mask::MANA, o.mana()),
        ];
        let mut c = StatCtx::in_world(w, creature);
        let mut out = [(0u32, ModifierType::None); 9];
        for (i, (bit, a)) in attributes.into_iter().enumerate() {
            out[i] = (bit, a.modifier_type(&mut c));
        }
        for (i, (bit, v)) in vitals.into_iter().enumerate() {
            out[6 + i] = (bit, v.modifier_type(&mut c));
        }
        out
    }

    // ACE: AttributeMaskHelper.GetAttributeHighlights
    #[must_use]
    pub fn attribute_mask_helper_get_attribute_highlights(
        w: &mut World,
        creature: ObjectGuid,
    ) -> u32 {
        let mut highlight_mask = 0;

        for (bit, modifier_type) in attribute_modifier_types(w, creature) {
            if modifier_type != empyrean_entity::enums::ModifierType::None {
                highlight_mask |= bit;
            }
        }

        highlight_mask
    }

    // ACE: AttributeMaskHelper.GetAttributeColors
    #[must_use]
    pub fn attribute_mask_helper_get_attribute_colors(w: &mut World, creature: ObjectGuid) -> u32 {
        let mut color_mask = 0;

        // defaults to debuffed - highlight masked above
        for (bit, modifier_type) in attribute_modifier_types(w, creature) {
            if modifier_type == empyrean_entity::enums::ModifierType::Buffed {
                color_mask |= bit;
            }
        }

        color_mask
    }

    /// `ACE.Server.Network.Enum.WeaponMask` members.
    mod weapon_mask {
        pub const ATTACK_SKILL: u32 = 0x1;
        pub const MELEE_DEFENSE: u32 = 0x2;
        pub const SPEED: u32 = 0x4;
        pub const DAMAGE: u32 = 0x8;
        pub const DAMAGE_VARIANCE: u32 = 0x10;
        pub const DAMAGE_MOD: u32 = 0x20;
    }

    /// `WeaponMaskHelper.GetHighlightMask(WeaponProfile)` (`Network/Enum/WeaponMask.cs`): it
    /// reads only the profile and the weapon's type.
    // ACE: WeaponMaskHelper.GetHighlightMask
    #[must_use]
    pub fn weapon_mask_helper_get_highlight_mask(w: &World, profile: &WeaponProfile) -> u32 {
        let mut highlight_mask = 0;
        let weenie_type = w
            .objects
            .get(profile.weapon)
            .expect("ACE: profile.Weapon is null")
            .biota
            .weenie_type;

        // Enchant applies to all weapons
        if profile.enchantment_weapon_defense != 0.0 {
            highlight_mask |= weapon_mask::MELEE_DEFENSE;
        }

        // Following enchants do not apply to caster weapons
        if weenie_type != WeenieType::Caster {
            if profile.enchantment_weapon_offense != 0.0 {
                highlight_mask |= weapon_mask::ATTACK_SKILL;
            }
            if profile.enchantment_weapon_time != 0 {
                highlight_mask |= weapon_mask::SPEED;
            }
            if profile.enchantment_damage != 0 {
                highlight_mask |= weapon_mask::DAMAGE;
            }
            if profile.enchantment_damage_variance != 1.0 {
                highlight_mask |= weapon_mask::DAMAGE_VARIANCE;
            }
            if profile.enchantment_damage_mod != 0.0 {
                highlight_mask |= weapon_mask::DAMAGE_MOD;
            }
        }

        highlight_mask
    }

    /// `WeaponMaskHelper.GetColorMask(WeaponProfile)`.
    // ACE: WeaponMaskHelper.GetColorMask
    #[must_use]
    pub fn weapon_mask_helper_get_color_mask(w: &World, profile: &WeaponProfile) -> u32 {
        let mut color_mask = 0;
        let weenie_type = w
            .objects
            .get(profile.weapon)
            .expect("ACE: profile.Weapon is null")
            .biota
            .weenie_type;

        // Enchant applies to all weapons
        if profile.enchantment_weapon_defense > 0.0 {
            color_mask |= weapon_mask::MELEE_DEFENSE;
        }

        // Following enchants do not apply to caster weapons
        if weenie_type != WeenieType::Caster
            && (weenie_type != WeenieType::Ammunition
                || property_manager_get_bool(w, "show_ammo_buff", false))
        {
            // item enchantments are currently being cast on wielder
            if profile.enchantment_weapon_offense > 0.0 {
                color_mask |= weapon_mask::ATTACK_SKILL;
            }
            if profile.enchantment_weapon_time < 0 {
                color_mask |= weapon_mask::SPEED;
            }
            if profile.enchantment_damage > 0 {
                color_mask |= weapon_mask::DAMAGE;
            }
            if profile.enchantment_damage_variance < 1.0 {
                color_mask |= weapon_mask::DAMAGE_VARIANCE;
            }
            if profile.enchantment_damage_mod > 0.0 {
                color_mask |= weapon_mask::DAMAGE_MOD;
            }
        }

        color_mask
    }

    // ---- spells ---------------------------------------------------------------------------

    /// What `Enchantment.Init` reads from `new Spell(id)` (`Entity/Spell.cs`).
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct SpellView {
        pub id: u32,
        pub name: String,
        pub category: SpellCategory,
        pub power: u32,
        pub duration: f64,
        pub degrade_modifier: f32,
        pub degrade_limit: f32,
        /// `spell._spell != null` (the world database has the spell).
        pub has_spell_base: bool,
        pub stat_mod_type: EnchantmentTypeFlags,
        pub stat_mod_key: u32,
        pub stat_mod_val: f32,
        pub is_beneficial: bool,
    }

    /// `new Spell(spellId)`, as `Enchantment.Init` reads it. A spell the dat does not have is
    /// ACE's `NullReferenceException` on `Spell.Id` (`_spellBase`).
    #[must_use]
    pub fn spell_new(w: &World, spell_id: u32) -> SpellView {
        let spell = crate::entity::spell::Spell::new(w, spell_id, true);
        let has_spell_base = spell.spell.is_some();
        SpellView {
            id: spell.id(),
            name: spell.name().to_owned(),
            category: spell.category(),
            power: spell.power(),
            duration: spell.duration(),
            degrade_modifier: spell.degrade_modifier(),
            degrade_limit: spell.degrade_limit(),
            has_spell_base,
            // the world-database half, read only when present (`if (spell._spell != null)`)
            stat_mod_type: if has_spell_base {
                spell.stat_mod_type()
            } else {
                EnchantmentTypeFlags::default()
            },
            stat_mod_key: if has_spell_base {
                spell.stat_mod_key()
            } else {
                0
            },
            stat_mod_val: if has_spell_base {
                spell.stat_mod_val()
            } else {
                0.0
            },
            is_beneficial: has_spell_base && spell.is_beneficial(),
        }
    }

    /// `SoulEmote.SoulEmotes.Contains(command)` (`Entity/SoulEmote.cs`): a copy of that set's
    /// members, in ACE's order, until the file is ported.
    #[must_use]
    pub fn soul_emote_contains(command: MotionCommand) -> bool {
        const SOUL_EMOTES: &[MotionCommand] = &[
            MotionCommand::AFKState,
            MotionCommand::AkimboState,
            MotionCommand::AtEaseState,
            MotionCommand::ATOYOT,
            MotionCommand::Beckon,
            MotionCommand::BeSeeingYou,
            MotionCommand::BlowKiss,
            MotionCommand::BowDeepState,
            MotionCommand::Cheer,
            MotionCommand::ClapHands,
            MotionCommand::ClapHandsState,
            MotionCommand::Cringe,
            MotionCommand::CrossArmsState,
            MotionCommand::Cry,
            MotionCommand::CurtseyState,
            MotionCommand::DrudgeDance,
            MotionCommand::DrudgeDanceState,
            MotionCommand::HaveASeat,
            MotionCommand::HaveASeatState,
            MotionCommand::HeartyLaugh,
            MotionCommand::Helper,
            MotionCommand::KneelState,
            MotionCommand::Knock,
            MotionCommand::Laugh,
            MotionCommand::LeanState,
            MotionCommand::MeditateState,
            MotionCommand::MimeDrink,
            MotionCommand::MimeEat,
            MotionCommand::Mock,
            MotionCommand::Nod,
            MotionCommand::NudgeLeft,
            MotionCommand::NudgeRight,
            MotionCommand::PleadState,
            MotionCommand::PointDown,
            MotionCommand::PointDownState,
            MotionCommand::PointLeft,
            MotionCommand::PointLeftState,
            MotionCommand::PointRight,
            MotionCommand::PointRightState,
            MotionCommand::PointState,
            MotionCommand::PossumState,
            MotionCommand::PrayState,
            MotionCommand::ReadState,
            MotionCommand::SaluteState,
            MotionCommand::ScanHorizon,
            MotionCommand::ScratchHead,
            MotionCommand::ScratchHeadState,
            MotionCommand::ShakeFist,
            MotionCommand::ShakeFistState,
            MotionCommand::ShakeHead,
            MotionCommand::Shiver,
            MotionCommand::Shoo,
            MotionCommand::Shrug,
            MotionCommand::SitBackState,
            MotionCommand::SitCrossleggedState,
            MotionCommand::SitState,
            MotionCommand::SlouchState,
            MotionCommand::SmackHead,
            MotionCommand::SnowAngelState,
            MotionCommand::Spit,
            MotionCommand::SurrenderState,
            MotionCommand::TalktotheHandState,
            MotionCommand::TapFootState,
            MotionCommand::Teapot,
            MotionCommand::ThinkerState,
            MotionCommand::WarmHands,
            MotionCommand::Wave,
            MotionCommand::WaveHigh,
            MotionCommand::WaveLow,
            MotionCommand::WaveState,
            MotionCommand::WindedState,
            MotionCommand::WoahState,
            MotionCommand::YawnStretch,
            MotionCommand::YMCA,
        ];
        SOUL_EMOTES.contains(&command)
    }

    // ---- world-object info ---------------------------------------------------------------

    /// Stand-in for `ACE.Server.Entity.WorldObjectInfo<T>` (`Entity/WorldObjectInfo.cs`): the
    /// cached values and the payload.
    #[derive(Debug, Clone, PartialEq)]
    pub struct WorldObjectInfo<T> {
        pub guid: ObjectGuid,
        pub name: Option<String>,
        pub weenie_class_id: u32,
        pub weenie_type: WeenieType,
        pub value: T,
    }

    impl<T> WorldObjectInfo<T> {
        /// `new WorldObjectInfo<T>(worldObject, value)`.
        pub fn new(w: &World, wo: ObjectGuid, value: T) -> Self {
            let o = w.objects.get(wo).expect("ACE: worldObject is null");
            WorldObjectInfo {
                guid: wo,
                name: crate::dispatch::name::name(w, wo),
                weenie_class_id: o.biota.weenie_class_id,
                weenie_type: o.biota.weenie_type,
                value,
            }
        }
    }
}
