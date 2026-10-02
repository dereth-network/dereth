// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/HousePortal.cs
//! Port of `Source/ACE.Server/WorldObjects/HousePortal.cs`.

use std::panic::{catch_unwind, AssertUnwindSafe};

use empyrean_content::models::world::HousePortal as DbHousePortal;
use empyrean_entity::enums::WeenieError;
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::activation_result::ActivationResult;
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{house, player_house, portal};
use crate::{dispatch, World};

/// Non-property fields declared in `HousePortal.cs`.
#[derive(Debug, Default)]
pub struct HousePortalFields {}

// ACE: HousePortal.House
#[must_use]
pub fn house(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    let parent = w.objects.get(this)?.wo.world_object_links.parent_link?;
    w.objects
        .get(parent)
        .is_some_and(WorldObject::is_house)
        .then_some(parent)
}

fn loc_string(w: &World, g: ObjectGuid) -> String {
    w.objects
        .get(g)
        .and_then(WorldObject::location)
        .map_or_else(String::new, |l| l.to_loc_string())
}

fn describe(w: &World, g: Option<ObjectGuid>) -> Option<String> {
    let g = g?;
    let o = w.objects.get(g)?;
    Some(format!(
        "{}:0x{}:{}",
        dispatch::name::name(w, g).unwrap_or_default(),
        g,
        o.biota.weenie_class_id
    ))
}

// ACE: HousePortal.SetLinkProperties
pub fn house_portal_set_link_properties(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    // `try { ... } catch (Exception ex) { log.Error(...) }`
    let result = catch_unwind(AssertUnwindSafe(|| set_link_properties_inner(w, this, wo)));
    if let Err(ex) = result {
        let ex = ex
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| ex.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default();
        let parent_link = w
            .objects
            .get(this)
            .and_then(|o| o.wo.world_object_links.parent_link);
        log::error!(
            "[HOUSE] HousePortal.SetLinkProperties({}): {} {} for HousePortal 0x{} at {}\n Exception: {}",
            describe(w, Some(wo)).unwrap_or_else(|| "null".to_owned()),
            describe(w, parent_link).map_or_else(|| "ParentLink is null".to_owned(), |d| format!("ParentLink = {d}")),
            describe(w, house(w, this)).map_or_else(|| "House is null".to_owned(), |d| format!("House = {d}")),
            this,
            loc_string(w, this),
            ex
        );
    }
}

fn set_link_properties_inner(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    let Some(h) = house(w, this) else {
        log::warn!(
            "[HOUSE] HousePortal.SetLinkProperties({}): House is null for HousePortal 0x{} at {}",
            describe(w, Some(wo)).unwrap_or_else(|| "null".to_owned()),
            this,
            loc_string(w, this)
        );
        return;
    };

    if !w.objects.contains(wo) {
        log::warn!(
            "[HOUSE] HousePortal.SetLinkProperties(null): WorldObject is null for HousePortal 0x{} at {} | House = {}",
            this,
            loc_string(w, this),
            describe(w, Some(h)).unwrap_or_default()
        );
        return;
    }

    // get properties from parent?
    let (house_id, house_owner, house_instance) = {
        let o = w.objects.get(h).expect("present");
        (o.house_id(), o.house_owner(), o.house_instance())
    };
    {
        let o = w.objects.get_mut(wo).expect("present");
        o.set_house_id(house_id);
        o.set_house_owner_prop(house_owner);
        o.set_house_instance(house_instance);
    }

    if house::is_link_spot(w, wo) {
        let house_portals = house::get_house_portals(w, h);
        if house_portals.is_empty() {
            // `Console.WriteLine`
            log::info!(
                "{}.SetLinkProperties({}): found LinkSpot, but empty HousePortals",
                dispatch::name::name(w, this).unwrap_or_default(),
                dispatch::name::name(w, wo).unwrap_or_default()
            );
            return;
        }
        let mut i = house_portals[0].clone();

        let cell = w
            .objects
            .get(this)
            .and_then(WorldObject::location)
            .expect("System.NullReferenceException: Location")
            .cell();
        if i.obj_cell_id == cell {
            if house_portals.len() > 1 {
                i = house_portals[1].clone();
            } else {
                // there are some houses that for some reason, don't have return locations, so we'll fake the entry with a reference to the root house portal location mimicking other database entries.
                let root_house =
                    house::root_house(w, h).expect("System.NullReferenceException: RootHouse");
                let root_portal = house::house_portal(w, root_house)
                    .expect("System.NullReferenceException: RootHouse.HousePortal");
                let l = w
                    .objects
                    .get(root_portal)
                    .and_then(WorldObject::location)
                    .expect("System.NullReferenceException: HousePortal.Location");
                i = DbHousePortal {
                    obj_cell_id: l.cell(),
                    origin_x: l.position_x,
                    origin_y: l.position_y,
                    origin_z: l.position_z,
                    angles_x: l.rotation_x,
                    angles_y: l.rotation_y,
                    angles_z: l.rotation_z,
                    angles_w: l.rotation_w,
                    ..DbHousePortal::default()
                };
            }
        }

        let destination = Position::from_components(
            i.obj_cell_id,
            i.origin_x,
            i.origin_y,
            i.origin_z,
            i.angles_x,
            i.angles_y,
            i.angles_z,
            i.angles_w,
            false,
        );

        w.objects
            .get_mut(wo)
            .expect("present")
            .set_destination(Some(destination));

        // set portal destination directly?
        w.objects
            .get_mut(this)
            .expect("present")
            .set_destination(Some(destination));
    }
}

// ACE: HousePortal.CheckUseRequirements
pub fn house_portal_check_use_requirements(
    w: &mut World,
    this: ObjectGuid,
    activator: ObjectGuid,
) -> ActivationResult {
    let root_house = house(w, this).and_then(|h| house::root_house(w, h));

    let activator_exists = w.objects.contains(activator);
    let Some(root_house) = root_house.filter(|_| activator_exists) else {
        log::warn!(
            "HousePortal.CheckUseRequirements: 0x{} - {}",
            this,
            loc_string(w, this)
        );
        log::warn!(
            "HousePortal.CheckUseRequirements: activator is null - {} | House is null - {} | RootHouse is null - {}",
            if activator_exists { "False" } else { "True" },
            if house(w, this).is_none() { "True" } else { "False" },
            if root_house.is_none() { "True" } else { "False" }
        );
        return ActivationResult::new(false);
    };

    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    let refusal = |w: &mut World, error: WeenieError| {
        let session = player_manager::player_session(w, player)
            .expect("System.NullReferenceException: player.Session");
        let data = w.sessions.get_mut(session).expect("the player's session");
        ActivationResult::with_message(game_event_weenie_error(data, error))
    };

    if player_house::is_olthoi_player(w, player) {
        return refusal(w, WeenieError::OlthoiMayNotUsePortal);
    }

    let player_lb = w
        .objects
        .get(player)
        .and_then(|o| o.current_landblock)
        .expect("System.NullReferenceException: player.CurrentLandblock");
    let is_dungeon = w
        .landblock_manager
        .landblocks
        .get_mut(player_lb)
        .expect("System.NullReferenceException: CurrentLandblock")
        .is_dungeon();
    if is_dungeon {
        let destination = w
            .objects
            .get(this)
            .and_then(WorldObject::destination)
            .expect("System.NullReferenceException: Destination");
        if destination.landblock_id() != player_lb {
            return ActivationResult::new(true); // allow escape to overworld always
        }
    }

    if w.objects
        .get(player)
        .is_some_and(WorldObject::ignore_portal_restrictions)
    {
        return ActivationResult::new(true);
    }

    let house_owner = w.objects.get(root_house).and_then(WorldObject::house_owner);

    if house_owner.is_none() {
        //return new ActivationResult(new GameEventWeenieError(player.Session, WeenieError.YouMustBeHouseGuestToUsePortal));
        return ActivationResult::new(true);
    }

    if w.objects
        .get(root_house)
        .is_some_and(WorldObject::open_to_everyone)
    {
        return ActivationResult::new(true);
    }

    if !house::has_permission(w, root_house, player, false) {
        return refusal(w, WeenieError::YouMustBeHouseGuestToUsePortal);
    }

    ActivationResult::new(true)
}

/// House Portals are on Use activated, rather than collision based activation
/// The actual portal process is wrapped to the base portal class ActOnUse, after ACL check are performed
// ACE: HousePortal.ActOnUse
pub fn house_portal_act_on_use(w: &mut World, this: ObjectGuid, world_object: ObjectGuid) {
    // DIVERGE: a world without the house's kind (`EraFeatures::housing`, `apartments`) refuses
    // entering it through its portal (V420).
    if w.objects
        .get(world_object)
        .is_some_and(WorldObject::is_player)
    {
        let kind = house(w, this).and_then(|h| house::root_house(w, h)).map_or(
            empyrean_entity::enums::HouseType::Undef,
            |h| {
                w.objects.get(h).map_or(
                    empyrean_entity::enums::HouseType::Undef,
                    WorldObject::house_type,
                )
            },
        );
        if !crate::world_objects::era_gates::has_house(w, world_object, kind) {
            return;
        }
    }

    // if house portal in dungeon,
    // set destination to outdoor house slumlord
    if let Some(lb) = w.objects.get(this).and_then(|o| o.current_landblock) {
        let is_dungeon = w
            .landblock_manager
            .landblocks
            .get_mut(lb)
            .is_some_and(|l| l.is_dungeon());
        if is_dungeon {
            let destination = w
                .objects
                .get(this)
                .and_then(WorldObject::destination)
                .expect("System.NullReferenceException: Destination");
            if destination.landblock_id() == lb {
                let h = house(w, this).expect("System.NullReferenceException: House");
                let root_house =
                    house::root_house(w, h).expect("System.NullReferenceException: RootHouse");
                let slum_lord = house::slum_lord(w, root_house)
                    .expect("System.NullReferenceException: SlumLord");
                let location = w
                    .objects
                    .get(slum_lord)
                    .and_then(WorldObject::location)
                    .expect("System.NullReferenceException: SlumLord.Location");
                w.objects
                    .get_mut(this)
                    .expect("present")
                    .set_destination(Some(Position::from_position(&location)));
            }
        }
    }

    portal::portal_act_on_use(w, this, world_object);
}

// ---- constructors and SetEphemeralValues ----

/// `new HousePortal(weenie, guid)` / `new HousePortal(biota)`: the `Portal` constructor, then
/// Portal's protected `SetEphemeralValues` a second time (HousePortal declares none of its own).
// ACE: HousePortal.HousePortal
pub fn house_portal_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    portal::portal_ctor(o, env, src);
    portal::portal_set_ephemeral_values(o, env);
}
