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
                useability: w.pwd.useability,
                physics_state: model.tables.physics.get(id).map_or(0, |p| p.state),
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
        // What sets off a recall, or sets where one lands.
        let recall_spell = |spell: u32| {
            model
                .magic
                .spell_table
                .as_deref()
                .and_then(|t| t.spells.get(&spell))
                .is_some_and(|b| NAV_RECALL_KINDS.contains(&b.meta_spell_type))
        };
        for r in &self.interaction.last_sent {
            use dereth_client_model::Request as R;
            if let R::CastTargetedSpell(m) = r {
                if let Some(o) = seen(m.target) {
                    self.travel.spell_cast_at(m.spell_id, &o, now);
                }
            }
            // An item used from the pack (not standing in the world) that casts a recall.
            let item_spell = |id: ObjectId| {
                model
                    .weenie(id)
                    .and_then(|w| w.pwd.spell_id.map(u32::from))
                    .filter(|_| at_of(id).is_none())
            };
            let recall = crate::travel::recall_set_off(r, &recall_spell, &item_spell);
            if let Some(recall) = recall {
                self.travel.recall_used(recall, now);
                continue;
            }
            let dereth_client_model::Request::UseEvent(u) = r else {
                continue;
            };
            let Some(o) = seen(u.object) else { continue };
            if o.kind() == Some(dereth_navigation::PlaceKind::Lifestone) {
                if let Some(at) = o.at {
                    self.travel.lifestone_used(at, now);
                }
                continue;
            }
            if o.kind() != Some(dereth_navigation::PlaceKind::Portal) {
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
        // The positions the server keeps for the character that are recalls' landings.
        if let Some(q) = model.player_qualities() {
            let kept: Vec<(u32, dereth_primitives::Position)> = q
                .positions
                .iter()
                .flatten()
                .map(|(k, p)| (*k, dereth_primitives::Position::from(*p)))
                .collect();
            if !kept.is_empty() && !name.is_empty() {
                self.travel.recall_positions(&name, &kept);
            }
        }
        let at = character.position();
        let portal_space = self.teleport.teleport_in_progress();
        self.travel.body_at(&at, portal_space, now, &name);
        self.travel.save_places();
    }
}
