// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Links.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Links.cs`.
//!
//! Linked instances: a world-DB landblock instance may name child instances
//! (`landblock_instance_link`). The factory queues them on the parent (`LinkedInstances`); when
//! the landblock adds the parent, `ActivateLinks` turns them into generator profiles (for a
//! generator parent) or into child objects placed next to it (doors, switches, portals, houses).
//! `ParentLink` and `ChildLinks` are guids into `World.objects`.

use empyrean_content::models::world::LandblockInstance;
use empyrean_entity::enums::PropertyString;
use empyrean_entity::{Biota, ObjectGuid, Position};

use crate::entity::landblock;
use crate::factories::world_object_factory;
use crate::physics::phys_ext;
use crate::world_objects::world_object::CtorEnv;
use crate::World;

/// Non-property fields declared in `WorldObject_Links.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectLinksFields {
    // ACE: WorldObject.LinkedInstances
    pub linked_instances: Vec<LandblockInstance>,
    // ACE: WorldObject.ParentLink
    pub parent_link: Option<ObjectGuid>,
    // ACE: WorldObject.ChildLinks
    pub child_links: Vec<ObjectGuid>,
}

fn name_of(w: &World, wo: ObjectGuid) -> String {
    w.objects
        .get(wo)
        .and_then(|o| o.get_property(PropertyString::Name))
        .unwrap_or_default()
}

/// Creates the linked instances queued on `this`: a generator gets them as generator profiles
/// (`AddGeneratorLinks`); any other object creates each child (from the shard's biota for it when
/// there is one, else from its weenie) at the instance's position, lets `parent` (default `this`)
/// set its link properties, adds it to `this`'s landblock and links it both ways, then
/// activates the child's own links (nested links) with the child as their parent.
///
/// A child joins `World.objects` when it is created (the store owns every live object; in ACE
/// the parent's `ChildLinks` holds it even when `this` has no landblock). A guid missing from the
/// store has no links to activate.
// ACE: WorldObject.ActivateLinks
pub fn activate_links(
    w: &mut World,
    this: ObjectGuid,
    source_objects: &[LandblockInstance],
    biotas: &[Biota],
    parent: Option<ObjectGuid>,
) {
    let Some(o) = w.objects.get(this) else { return };
    if o.wo.world_object_links.linked_instances.is_empty() {
        return;
    }

    if o.is_generator() {
        let utc_now = w.now.utc;
        let o = w.objects.get_mut(this).expect("present");
        let linked_instances = std::mem::take(&mut o.wo.world_object_links.linked_instances);
        o.add_generator_links(&linked_instances, utc_now);
        o.wo.world_object_links.linked_instances = linked_instances;
        return;
    }

    let parent = parent.unwrap_or(this);

    let linked_instances = o.wo.world_object_links.linked_instances.clone();
    for link in &linked_instances {
        let biota = biotas.iter().find(|b| b.id == link.guid);
        let wo = match biota {
            None => CtorEnv::with_world(w, |env| {
                world_object_factory::create_world_object(
                    env,
                    (env.get_cached_weenie)(link.weenie_class_id),
                    ObjectGuid::new(link.guid),
                )
            }),
            Some(biota) => {
                //Console.WriteLine("Loaded child biota " + wo.Name);
                CtorEnv::with_world(w, |env| {
                    world_object_factory::create_world_object_from_biota(env, biota.clone())
                })
            }
        };

        let Some(mut wo) = wo else { continue };

        wo.set_location(Some(Position::from_components(
            link.obj_cell_id,
            link.origin_x,
            link.origin_y,
            link.origin_z,
            link.angles_x,
            link.angles_y,
            link.angles_z,
            link.angles_w,
            false,
        )));
        let child = wo.guid;
        if let Err(dup) = w.objects.insert(wo) {
            log::error!("ActivateLinks: linked object 0x{} is already in the world; the new copy is dropped", dup.guid);
            continue;
        }
        crate::world_objects::creature::post_insert(w, child);

        crate::dispatch::set_link_properties::set_link_properties(w, parent, child);
        if let Some(current_landblock) = w.objects.get(this).and_then(|o| o.current_landblock) {
            landblock::add_world_object(w, current_landblock, child);
        }
        if let Some(h) = phys_ext::physics_obj(w, child) {
            // `wo.PhysicsObj.Order = 0`
            if let Some(e) = phys_ext::ext_mut(w, h) {
                e.order = 0;
            }
        }

        if let Some(c) = w.objects.get_mut(child) {
            c.wo.world_object_links.parent_link = Some(parent);
        }
        if let Some(p) = w.objects.get_mut(parent) {
            p.wo.world_object_links.child_links.push(child);
        }

        // process nested links recursively
        let nested: Vec<LandblockInstance> = link
            .landblock_instance_link
            .iter()
            .filter_map(|sub_link| {
                source_objects
                    .iter()
                    .find(|x| x.guid == sub_link.child_guid)
                    .cloned()
            })
            .collect();
        let Some(c) = w.objects.get_mut(child) else {
            continue;
        };
        c.wo.world_object_links.linked_instances.extend(nested);

        if !c.wo.world_object_links.linked_instances.is_empty() {
            activate_links(w, child, source_objects, biotas, None);
        }
    }
}

/// `UpdateLinkProperties(link)` for each child link and each of its own child links.
// ACE: WorldObject.UpdateLinks
pub fn update_links(w: &mut World, this: ObjectGuid) {
    let child_links = w
        .objects
        .get(this)
        .map(|o| o.wo.world_object_links.child_links.clone())
        .unwrap_or_default();
    for link in child_links {
        crate::dispatch::update_link_properties::update_link_properties(w, this, link);

        let sub_links = w
            .objects
            .get(link)
            .map(|o| o.wo.world_object_links.child_links.clone())
            .unwrap_or_default();
        for sub_link in sub_links {
            crate::dispatch::update_link_properties::update_link_properties(w, this, sub_link);
        }
    }
}

// ---- virtual-dispatch targets ----

/// The empty base: a link parent of a type with no override logs and does nothing.
// ACE: WorldObject.SetLinkProperties
pub fn world_object_set_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    // empty base
    let weenie_type = w
        .objects
        .get(this)
        .map(|o| o.biota.weenie_type)
        .unwrap_or_default();
    // `Console.WriteLine`
    log::info!(
        "{}.SetLinkProperties({}) called for unknown parent type: {:?}",
        name_of(w, this),
        name_of(w, wo),
        weenie_type
    );
}

/// The empty base: a link parent of a type with no override logs and does nothing.
// ACE: WorldObject.UpdateLinkProperties
pub fn world_object_update_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    // empty base
    let weenie_type = w
        .objects
        .get(this)
        .map(|o| o.biota.weenie_type)
        .unwrap_or_default();
    // `Console.WriteLine`
    log::info!(
        "{}.UpdateLinkProperties({}) called for unknown parent type: {:?}",
        name_of(w, this),
        name_of(w, wo),
        weenie_type
    );
}
