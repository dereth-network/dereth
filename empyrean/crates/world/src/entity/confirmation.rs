// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Confirmation.cs
//! Port of `Source/ACE.Server/Entity/Confirmation.cs`.
//!
//! ACE's abstract `Confirmation` and its eight subclasses are one struct, [`Confirmation`], with
//! the subclass data in [`ConfirmationKind`]; `ProcessConfirmation` dispatches on the kind. A
//! confirmation is owned by the player's `ConfirmationManager` until `HandleResponse` removes it and
//! processes it, so `Confirmation_Custom`'s `Action` is a `FnOnce`.
//!
//! The owners of some targets are not ported yet (the attribute, skill and augmentation devices'
//! confirmed `ActOnUse`, and `Hook.Item`); those calls go through [`shims`], each function named
//! after the ACE member it stands in for.

use empyrean_entity::enums::{ChatMessageType, ConfirmationType, EmoteCategory, WeenieError};
use empyrean_entity::ObjectGuid;

use crate::managers::{player_manager, recipe_manager};
use crate::world_objects::managers::emote_manager;
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::{player, player_allegiance, player_fellowship, player_networking};
use crate::World;

/// `Confirmation_Custom.Action`.
pub type CustomAction = Box<dyn FnOnce(&mut World) + Send>;

/// The data of each `Confirmation` subclass.
pub enum ConfirmationKind {
    /// `Confirmation_AlterAttribute.AttributeTransferDevice`.
    AlterAttribute {
        attribute_transfer_device: ObjectGuid,
    },
    /// `Confirmation_AlterSkill.SkillAlterationDevice`.
    AlterSkill { skill_alteration_device: ObjectGuid },
    /// `Confirmation_Augmentation.AugmentationGuid`.
    Augmentation { augmentation_guid: ObjectGuid },
    /// `Confirmation_CraftInteration`'s `SourceGuid`, `TargetGuid` and `Tinkering`.
    CraftInteraction {
        source_guid: ObjectGuid,
        target_guid: ObjectGuid,
        tinkering: bool,
    },
    /// `Confirmation_Fellowship.InviterGuid`.
    Fellowship { inviter_guid: ObjectGuid },
    /// `Confirmation_SwearAllegiance.VassalGuid`.
    SwearAllegiance { vassal_guid: ObjectGuid },
    /// `Confirmation_YesNo`'s `SourceGuid` and `Quest`.
    YesNo {
        source_guid: ObjectGuid,
        quest: Option<String>,
    },
    /// `Confirmation_Custom.Action`.
    Custom { action: CustomAction },
}

impl std::fmt::Debug for ConfirmationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlterAttribute {
                attribute_transfer_device,
            } => f
                .debug_struct("AlterAttribute")
                .field("attribute_transfer_device", attribute_transfer_device)
                .finish(),
            Self::AlterSkill {
                skill_alteration_device,
            } => f
                .debug_struct("AlterSkill")
                .field("skill_alteration_device", skill_alteration_device)
                .finish(),
            Self::Augmentation { augmentation_guid } => f
                .debug_struct("Augmentation")
                .field("augmentation_guid", augmentation_guid)
                .finish(),
            Self::CraftInteraction {
                source_guid,
                target_guid,
                tinkering,
            } => f
                .debug_struct("CraftInteraction")
                .field("source_guid", source_guid)
                .field("target_guid", target_guid)
                .field("tinkering", tinkering)
                .finish(),
            Self::Fellowship { inviter_guid } => f
                .debug_struct("Fellowship")
                .field("inviter_guid", inviter_guid)
                .finish(),
            Self::SwearAllegiance { vassal_guid } => f
                .debug_struct("SwearAllegiance")
                .field("vassal_guid", vassal_guid)
                .finish(),
            Self::YesNo { source_guid, quest } => f
                .debug_struct("YesNo")
                .field("source_guid", source_guid)
                .field("quest", quest)
                .finish(),
            Self::Custom { .. } => f.debug_struct("Custom").finish_non_exhaustive(),
        }
    }
}

// ACE: Confirmation
/// `abstract class Confirmation` with its subclass data.
#[derive(Debug)]
pub struct Confirmation {
    /// `Confirmation.PlayerGuid`: the player asked.
    pub player_guid: ObjectGuid,
    /// `Confirmation.ConfirmationType`.
    pub confirmation_type: ConfirmationType,
    /// `Confirmation.ContextId`.
    pub context_id: u32,
    /// The subclass and its fields.
    pub kind: ConfirmationKind,
}

impl Confirmation {
    // ACE: Confirmation.Confirmation
    #[must_use]
    pub fn new(
        player_guid: ObjectGuid,
        confirmation_type: ConfirmationType,
        kind: ConfirmationKind,
    ) -> Self {
        Self {
            player_guid,
            confirmation_type,
            context_id: 0,
            kind,
        }
    }

    // ACE: Confirmation_AlterAttribute.Confirmation_AlterAttribute
    #[must_use]
    pub fn alter_attribute(player_guid: ObjectGuid, attribute_transfer_device: ObjectGuid) -> Self {
        Self::new(
            player_guid,
            ConfirmationType::AlterAttribute,
            ConfirmationKind::AlterAttribute {
                attribute_transfer_device,
            },
        )
    }

    // ACE: Confirmation_AlterSkill.Confirmation_AlterSkill
    #[must_use]
    pub fn alter_skill(player_guid: ObjectGuid, skill_alteration_device: ObjectGuid) -> Self {
        Self::new(
            player_guid,
            ConfirmationType::AlterSkill,
            ConfirmationKind::AlterSkill {
                skill_alteration_device,
            },
        )
    }

    // ACE: Confirmation_Augmentation.Confirmation_Augmentation
    #[must_use]
    pub fn augmentation(player_guid: ObjectGuid, augmentation_guid: ObjectGuid) -> Self {
        Self::new(
            player_guid,
            ConfirmationType::Augmentation,
            ConfirmationKind::Augmentation { augmentation_guid },
        )
    }

    // ACE: Confirmation_CraftInteration.Confirmation_CraftInteration
    /// `Tinkering` is a public field ACE's callers set after construction (default false).
    #[must_use]
    pub fn craft_interation(
        player_guid: ObjectGuid,
        source_guid: ObjectGuid,
        target_guid: ObjectGuid,
    ) -> Self {
        Self::new(
            player_guid,
            ConfirmationType::CraftInteraction,
            ConfirmationKind::CraftInteraction {
                source_guid,
                target_guid,
                tinkering: false,
            },
        )
    }

    // ACE: Confirmation_Fellowship.Confirmation_Fellowship
    /// The invited player is the one asked.
    #[must_use]
    pub fn fellowship(inviter_guid: ObjectGuid, invited_guid: ObjectGuid) -> Self {
        Self::new(
            invited_guid,
            ConfirmationType::Fellowship,
            ConfirmationKind::Fellowship { inviter_guid },
        )
    }

    // ACE: Confirmation_SwearAllegiance.Confirmation_SwearAllegiance
    /// The patron is the one asked.
    #[must_use]
    pub fn swear_allegiance(patron_guid: ObjectGuid, vassal_guid: ObjectGuid) -> Self {
        Self::new(
            patron_guid,
            ConfirmationType::SwearAllegiance,
            ConfirmationKind::SwearAllegiance { vassal_guid },
        )
    }

    // ACE: Confirmation_YesNo.Confirmation_YesNo
    #[must_use]
    pub fn yes_no(
        source_guid: ObjectGuid,
        target_player_guid: ObjectGuid,
        quest: Option<&str>,
    ) -> Self {
        Self::new(
            target_player_guid,
            ConfirmationType::Yes_No,
            ConfirmationKind::YesNo {
                source_guid,
                quest: quest.map(str::to_owned),
            },
        )
    }

    // ACE: Confirmation_Custom.Confirmation_Custom
    /// ACE files a custom confirmation under `ConfirmationType.Yes_No`.
    #[must_use]
    pub fn custom(player_guid: ObjectGuid, action: CustomAction) -> Self {
        Self::new(
            player_guid,
            ConfirmationType::Yes_No,
            ConfirmationKind::Custom { action },
        )
    }

    // ACE: Confirmation.Player
    /// `PlayerManager.GetOnlinePlayer(PlayerGuid)`.
    #[must_use]
    pub fn player(&self, w: &World) -> Option<ObjectGuid> {
        player_manager::get_online_player(w, self.player_guid.full())
    }

    // ACE: Confirmation.ProcessConfirmation
    /// The virtual `ProcessConfirmation(response, timeout = false)`: the base is empty; each subclass
    /// override is one arm. Every override reads `Player` once, first.
    pub fn process_confirmation(self, w: &mut World, response: bool, timeout: bool) {
        match self.kind {
            ConfirmationKind::AlterAttribute {
                attribute_transfer_device,
            } => {
                alter_attribute_process_confirmation(
                    w,
                    self.player_guid,
                    attribute_transfer_device,
                    response,
                );
            }
            ConfirmationKind::AlterSkill {
                skill_alteration_device,
            } => {
                alter_skill_process_confirmation(
                    w,
                    self.player_guid,
                    skill_alteration_device,
                    response,
                );
            }
            ConfirmationKind::Augmentation { augmentation_guid } => {
                augmentation_process_confirmation(w, self.player_guid, augmentation_guid, response);
            }
            ConfirmationKind::CraftInteraction {
                source_guid,
                target_guid,
                ..
            } => {
                craft_interation_process_confirmation(
                    w,
                    self.player_guid,
                    source_guid,
                    target_guid,
                    response,
                );
            }
            ConfirmationKind::Fellowship { inviter_guid } => {
                fellowship_process_confirmation(
                    w,
                    self.player_guid,
                    inviter_guid,
                    response,
                    timeout,
                );
            }
            ConfirmationKind::SwearAllegiance { vassal_guid } => {
                swear_allegiance_process_confirmation(
                    w,
                    self.player_guid,
                    vassal_guid,
                    response,
                    timeout,
                );
            }
            ConfirmationKind::YesNo { source_guid, quest } => {
                yes_no_process_confirmation(
                    w,
                    self.player_guid,
                    source_guid,
                    quest.as_deref(),
                    response,
                );
            }
            ConfirmationKind::Custom { action } => {
                custom_process_confirmation(w, self.player_guid, action, response);
            }
        }
    }
}

/// `Confirmation.Player`.
fn online(w: &World, player_guid: ObjectGuid) -> Option<ObjectGuid> {
    player_manager::get_online_player(w, player_guid.full())
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `player.FindObject(guid, locations) as T`: the object when it is of the kind `is_kind` checks.
fn find_as(
    w: &World,
    player: ObjectGuid,
    guid: ObjectGuid,
    locations: SearchLocations,
    is_kind: impl Fn(&crate::world_objects::world_object::WorldObject) -> bool,
) -> Option<ObjectGuid> {
    player_inventory::find_object(w, player, guid, locations)
        .result
        .filter(|&g| w.objects.get(g).is_some_and(&is_kind))
}

// ACE: Confirmation_AlterAttribute.ProcessConfirmation
fn alter_attribute_process_confirmation(
    w: &mut World,
    player_guid: ObjectGuid,
    attribute_transfer_device: ObjectGuid,
    response: bool,
) {
    if !response {
        return;
    }

    let Some(player) = online(w, player_guid) else {
        return;
    };

    let attribute_transfer_device = find_as(
        w,
        player,
        attribute_transfer_device,
        SearchLocations::MyInventory,
        |o| o.is_attribute_transfer_device(),
    );

    if let Some(attribute_transfer_device) = attribute_transfer_device {
        shims::attribute_transfer_device_act_on_use(w, attribute_transfer_device, player, true);
    }
}

// ACE: Confirmation_AlterSkill.ProcessConfirmation
fn alter_skill_process_confirmation(
    w: &mut World,
    player_guid: ObjectGuid,
    skill_alteration_device: ObjectGuid,
    response: bool,
) {
    if !response {
        return;
    }

    let Some(player) = online(w, player_guid) else {
        return;
    };

    let skill_alteration_device = find_as(
        w,
        player,
        skill_alteration_device,
        SearchLocations::MyInventory,
        |o| o.is_skill_alteration_device(),
    );

    if let Some(skill_alteration_device) = skill_alteration_device {
        shims::skill_alteration_device_act_on_use(w, skill_alteration_device, player, true);
    }
}

// ACE: Confirmation_Augmentation.ProcessConfirmation
fn augmentation_process_confirmation(
    w: &mut World,
    player_guid: ObjectGuid,
    augmentation_guid: ObjectGuid,
    response: bool,
) {
    if !response {
        return;
    }

    let Some(player) = online(w, player_guid) else {
        return;
    };

    let augmentation = find_as(
        w,
        player,
        augmentation_guid,
        SearchLocations::MyInventory,
        |o| o.is_augmentation_device(),
    );

    if let Some(augmentation) = augmentation {
        shims::augmentation_device_act_on_use(w, augmentation, player, true);
    }
}

// ACE: Confirmation_CraftInteration.ProcessConfirmation
fn craft_interation_process_confirmation(
    w: &mut World,
    player_guid: ObjectGuid,
    source_guid: ObjectGuid,
    target_guid: ObjectGuid,
    response: bool,
) {
    let Some(player) = online(w, player_guid) else {
        return;
    };

    if !response {
        player_networking::send_weenie_error(w, player, WeenieError::YouChickenOut);

        return;
    }

    // inventory only?
    let source =
        player_inventory::find_object(w, player, source_guid, SearchLocations::LocationsICanMove)
            .result;
    let target =
        player_inventory::find_object(w, player, target_guid, SearchLocations::LocationsICanMove)
            .result;

    let (Some(source), Some(target)) = (source, target) else {
        return;
    };

    recipe_manager::use_object_on_target(w, player, source, target, true);
}

// ACE: Confirmation_Fellowship.ProcessConfirmation
fn fellowship_process_confirmation(
    w: &mut World,
    invited_guid: ObjectGuid,
    inviter_guid: ObjectGuid,
    response: bool,
    timeout: bool,
) {
    //if (!response) return;

    let invited = online(w, invited_guid);
    let inviter = player_manager::get_online_player(w, inviter_guid.full());

    if !response {
        if let Some(inviter) = inviter {
            let invited = invited.expect("ACE: invited is null (NullReferenceException)");
            let text = format!(
                "{} {} your offer of fellowship.",
                name(w, invited),
                if timeout {
                    "did not respond to"
                } else {
                    "has declined"
                }
            );
            player::send_message(w, inviter, &text, ChatMessageType::Broadcast);
        }
        return;
    }

    if let (Some(invited), Some(inviter)) = (invited, inviter) {
        if let Some(fellowship) = player_fellowship::fellowship(w, inviter) {
            crate::entity::fellowship::add_confirmed_member(
                w,
                &fellowship,
                Some(inviter),
                Some(invited),
                response,
            );
        }
    }
}

// ACE: Confirmation_SwearAllegiance.ProcessConfirmation
fn swear_allegiance_process_confirmation(
    w: &mut World,
    patron_guid: ObjectGuid,
    vassal_guid: ObjectGuid,
    response: bool,
    timeout: bool,
) {
    //if (!response) return;

    let Some(patron) = online(w, patron_guid) else {
        return;
    };

    let vassal = player_manager::get_online_player(w, vassal_guid.full());

    if !response {
        if let Some(vassal) = vassal {
            let text = format!(
                "{} {} your offer of allegiance.",
                name(w, patron),
                if timeout {
                    "did not respond to"
                } else {
                    "has declined"
                }
            );
            player::send_message(w, vassal, &text, ChatMessageType::Broadcast);
        }
        return;
    }

    if let Some(vassal) = vassal {
        player_allegiance::swear_allegiance(w, vassal, patron.full(), true, true);
    }
}

// ACE: Confirmation_YesNo.ProcessConfirmation
fn yes_no_process_confirmation(
    w: &mut World,
    player_guid: ObjectGuid,
    source_guid: ObjectGuid,
    quest: Option<&str>,
    response: bool,
) {
    let Some(player) = online(w, player_guid) else {
        return;
    };

    let mut source =
        player_inventory::find_object(w, player, source_guid, SearchLocations::Landblock).result;

    if let Some(hook) = source.filter(|&g| w.objects.get(g).is_some_and(|o| o.is_hook())) {
        if let Some(item) = shims::hook_item(w, hook) {
            source = Some(item);
        }
    }

    if let Some(source) = source {
        let category = if response {
            EmoteCategory::TestSuccess
        } else {
            EmoteCategory::TestFailure
        };
        emote_manager::execute_emote_set_category(w, source, category, quest, Some(player), false);
    }
}

// ACE: Confirmation_Custom.ProcessConfirmation
fn custom_process_confirmation(
    w: &mut World,
    player_guid: ObjectGuid,
    action: CustomAction,
    response: bool,
) {
    if !response {
        return;
    }

    if online(w, player_guid).is_none() {
        return;
    }

    action(w);
}

// ================================================================================ SHIMs

/// Targets whose owners are not ported yet, named after the ACE members they stand in for.
pub mod shims {
    use empyrean_entity::ObjectGuid;

    use crate::World;

    /// `AttributeTransferDevice.ActOnUse(WorldObject activator, bool confirmed)`.
    pub fn attribute_transfer_device_act_on_use(
        w: &mut World,
        device: ObjectGuid,
        activator: ObjectGuid,
        confirmed: bool,
    ) {
        crate::world_objects::attribute_transfer_device::act_on_use(
            w, device, activator, confirmed,
        );
    }

    /// `SkillAlterationDevice.ActOnUse(WorldObject activator, bool confirmed)`
    /// (`SkillAlterationDevice.cs`).
    pub fn skill_alteration_device_act_on_use(
        w: &mut World,
        device: ObjectGuid,
        activator: ObjectGuid,
        confirmed: bool,
    ) {
        crate::world_objects::skill_alteration_device::act_on_use(w, device, activator, confirmed);
    }

    /// `AugmentationDevice.ActOnUse(WorldObject activator, bool confirmed)`.
    pub fn augmentation_device_act_on_use(
        w: &mut World,
        device: ObjectGuid,
        activator: ObjectGuid,
        confirmed: bool,
    ) {
        crate::world_objects::augmentation_device::act_on_use(w, device, activator, confirmed);
    }

    /// SHIM: `Hook.Item` (`Hook.cs`): `Inventory?.Values.FirstOrDefault()`.
    #[must_use]
    pub fn hook_item(w: &World, hook: ObjectGuid) -> Option<ObjectGuid> {
        crate::world_objects::container::inventory_values(w, hook)
            .into_iter()
            .next()
    }
}
