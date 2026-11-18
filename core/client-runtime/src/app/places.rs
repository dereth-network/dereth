//! The places of the world the player is in, learned as they play ([`crate::travel::Travel`]
//! keeps them per world): each frame, the objects in the world not yet looked at, the portals
//! the player used, the house profile the server last told of, and where the body stands.

use super::*;
use dereth_primitives::ObjectId;

impl<S: Shell> App<S> {
    /// This frame's learning of the world's places.
    pub(super) fn places_use_time(&mut self) {
        let Some(world) = self.travel_world() else {
            return;
        };
        self.travel.note_world(&world);
        if self.playable_at.is_none() {
            return;
        }
        let Some(character) = self.world.as_ref().and_then(|w| w.character.as_ref()) else {
            return;
        };
        let model = &self.objects.world;
        let at_of = |id: ObjectId| {
            character
                .world
                .by_object_id(id)
                .and_then(|h| character.world.get(h))
                .map(|o| o.position)
        };
        let seen = |id: ObjectId| {
            model.weenie(id).map(|w| crate::travel::PlaceSeen {
                id,
                bitfield: w.pwd.bitfield,
                item_type: w.pwd.obj_type,
                wcid: w.pwd.wcid,
                name: w.pwd.name.clone(),
                at: at_of(id),
            })
        };
        for (id, presence) in model.tables.physics.iter() {
            if presence.cell.is_none() || model.player == Some(id) || !self.travel.place_unseen(id)
            {
                continue;
            }
            if let Some(o) = seen(id) {
                self.travel.see_object(&o);
            }
        }
        // The house the server last told the player of, once each time it tells.
        if model.house_profile_notices != self.places_house_notices {
            self.places_house_notices = model.house_profile_notices;
            if let Some((crystal, profile)) = &model.slumlord {
                if let Some(o) = seen(*crystal) {
                    let owner = (profile.owner.0 != 0).then_some(profile.name.as_str());
                    self.travel.see_house(&o, profile.house_type, owner);
                }
            }
        }
        let now = self.timer.cur_time;
        for r in &self.interaction.last_sent {
            let dereth_client_model::Request::UseEvent(u) = r else {
                continue;
            };
            let Some(o) = seen(u.object) else { continue };
            if dereth_navigation::PlaceKind::of(o.bitfield, o.item_type)
                != Some(dereth_navigation::PlaceKind::Portal)
            {
                continue;
            }
            if let Some(position) = o.at {
                self.travel.portal_used(
                    dereth_navigation::PortalSighting {
                        wcid: (o.wcid != 0).then_some(o.wcid),
                        name: o.name,
                        position,
                        shape: None,
                    },
                    now,
                );
            }
        }
        let name = model
            .player
            .and_then(|p| model.weenie(p))
            .map(|w| w.pwd.name.clone())
            .unwrap_or_default();
        let at = character.position();
        let portal_space = self.teleport.teleport_in_progress();
        self.travel.body_at(&at, portal_space, now, &name);
        self.travel.save_places();
    }
}
