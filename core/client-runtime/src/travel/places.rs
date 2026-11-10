//! What the travel learns of a world's places while the character plays, journey or not: the
//! lifestones, portals, vendors and people the server puts in the world, each once it is seen
//! standing somewhere; the houses whose crystals the player used, with their owners; and where
//! each portal the player uses sends them, into this world's portal store.
//!
//! The places are kept per world, as the portal store is, in `<prefix>places-<world>.tsv`
//! beside it, and written when something new is learned. The places a server publishes are
//! taken in under them: what the character saw always wins, and they are never written.

use std::collections::HashSet;

use dereth_navigation::{
    EdgeSource, KnownPlace, PlaceKind, PlaceStore, PortalSighting, PortalUseWatch, Recall,
};
use dereth_primitives::{ObjectId, Position};

use super::{file_name, unix_now, Travel, PORTAL_RECALL};

/// One world's places, and what is under way in learning them.
#[derive(Debug, Default)]
pub(super) struct Learning {
    /// Shared with the readers that hold it, and copied only when it changes while they do.
    store: std::sync::Arc<PlaceStore>,
    /// Written since it was last read or saved.
    changed: bool,
    /// The objects already looked at in this world: places taken in, and things that are not.
    looked_at: HashSet<ObjectId>,
    watch: PortalUseWatch,
    /// Portal uses learned outside a journey, for [`Travel::places_revision`].
    portals_learned: u64,
}

/// An object in the world, as places are learned from it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceSeen {
    pub id: ObjectId,
    /// Its description's flags and item type.
    pub bitfield: u32,
    pub item_type: u32,
    /// Its weenie class; `0` unknown.
    pub wcid: u32,
    pub name: String,
    /// Where it stands, once it stands somewhere.
    pub at: Option<Position>,
}

impl PlaceSeen {
    fn place(&self, kind: PlaceKind, at: Position, owner: Option<String>) -> KnownPlace {
        let now = unix_now();
        KnownPlace {
            kind,
            source: EdgeSource::Learned,
            wcid: (self.wcid != 0).then_some(self.wcid),
            name: self.name.clone(),
            at,
            first_seen: now,
            last_seen: now,
            owner,
            to: None,
        }
    }
}

impl Travel {
    /// This world's places.
    #[must_use]
    pub fn places(&self) -> &PlaceStore {
        &self.places.store
    }

    /// This world's places, to keep: the map holds them while it draws.
    #[must_use]
    pub fn places_shared(&self) -> std::sync::Arc<PlaceStore> {
        std::sync::Arc::clone(&self.places.store)
    }

    /// Goes up whenever this world's places or the portals the player used change: a reader
    /// reads them again when it differs from the last it read.
    #[must_use]
    pub fn places_revision(&self) -> u64 {
        self.places.store.revision() + self.places.portals_learned
    }

    /// Read `world`'s places, the last world's written first.
    pub(super) fn load_places(&mut self, world: &str) {
        self.save_places();
        self.places = Learning {
            store: std::sync::Arc::new(PlaceStore::new(world)),
            ..Learning::default()
        };
        // With nowhere to keep them, the server's places are still taken in.
        if let Some(path) = self.store_path(&format!("places-{}", file_name(world))) {
            match crate::platform::files::read_to_string(&path) {
                Ok(text) => match PlaceStore::from_text(&text) {
                    Ok(mut s) => {
                        world.clone_into(&mut s.world);
                        self.places.store = std::sync::Arc::new(s);
                    }
                    Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
            }
        }
        self.merge_published_places();
        tracing::info!(
            target: "travel",
            "places of {world:?}: {}",
            self.places.store.places().len()
        );
    }

    /// Take in the places the server publishes (the place store's text form, its places of
    /// source `published`): merged now under what the character saw, and into each world's
    /// places as they load, and never written. How many it publishes.
    ///
    /// # Errors
    /// The text is not a place store.
    pub fn set_published_places(&mut self, text: &str) -> Result<usize, String> {
        let table = PlaceStore::from_text(text).map_err(|e| e.to_string())?;
        self.published_places = table
            .places()
            .iter()
            .filter(|p| p.source == EdgeSource::Published)
            .cloned()
            .collect();
        self.merge_published_places();
        tracing::info!(
            target: "travel",
            "the server publishes {} places",
            self.published_places.len()
        );
        Ok(self.published_places.len())
    }

    fn merge_published_places(&mut self) {
        if self.places.store.world.is_empty() || self.published_places.is_empty() {
            return;
        }
        std::sync::Arc::make_mut(&mut self.places.store)
            .merge(self.published_places.iter().cloned());
    }

    /// Write this world's places, when something was learned since they were last written.
    pub fn save_places(&mut self) {
        if !self.places.changed || self.places.store.world.is_empty() {
            return;
        }
        self.places.changed = false;
        let name = format!("places-{}", file_name(&self.places.store.world));
        let Some(path) = self.store_path(&name) else {
            return;
        };
        let text = self
            .places
            .store
            .without(EdgeSource::Hint)
            .without(EdgeSource::Published)
            .to_text();
        if let Err(e) = crate::platform::files::write(&path, text) {
            tracing::warn!(target: "travel", "{}: {e}", path.display());
        }
    }

    /// Whether object `id` still needs looking at for this world's places.
    #[must_use]
    pub fn place_unseen(&self, id: ObjectId) -> bool {
        !self.places.store.world.is_empty() && !self.places.looked_at.contains(&id)
    }

    /// Look at `o` for this world's places: a place standing somewhere is taken in, and anything
    /// that is not a place is not looked at again. A place not yet standing anywhere is looked
    /// at again later.
    pub fn see_object(&mut self, o: &PlaceSeen) {
        if self.places.store.world.is_empty() {
            return;
        }
        let Some(kind) = PlaceKind::of(o.bitfield, o.item_type) else {
            self.places.looked_at.insert(o.id);
            return;
        };
        let Some(at) = o.at else { return };
        self.places.looked_at.insert(o.id);
        if std::sync::Arc::make_mut(&mut self.places.store).see(o.place(kind, at, None)) {
            self.places.changed = true;
        }
    }

    /// The server told the player of the house whose crystal is `crystal` (of `house_type`: 1 a
    /// cottage, 2 a villa, 3 a mansion, 4 an apartment), owned by `owner`, or for sale.
    pub fn see_house(&mut self, crystal: &PlaceSeen, house_type: u32, owner: Option<&str>) {
        let Some(at) = crystal.at else { return };
        if self.places.store.world.is_empty() {
            return;
        }
        let mut place = crystal.place(PlaceKind::House, at, owner.map(str::to_owned));
        if let Some(name) = match house_type {
            1 => Some("Cottage"),
            2 => Some("Villa"),
            3 => Some("Mansion"),
            4 => Some("Apartment"),
            _ => None,
        } {
            name.clone_into(&mut place.name);
        }
        if std::sync::Arc::make_mut(&mut self.places.store).see(place) {
            self.places.changed = true;
        }
    }

    /// The player used `portal` at `now`, by the game clock.
    pub fn portal_used(&mut self, portal: PortalSighting, now: f64) {
        self.places.watch.used(portal, now);
    }

    /// The character's body at `at` at `now`, in portal space or not: where a portal the player
    /// used sends them is learned once it has, into this world's portal store, as a journey
    /// learns it (a journey under way learns its own).
    pub fn body_at(&mut self, at: &Position, portal_space: bool, now: f64, character: &str) {
        let Some((portal, landed)) = self.places.watch.frame(at, portal_space, now) else {
            return;
        };
        if self.trip.is_some() || self.portals.world.is_empty() {
            return;
        }
        let t = unix_now();
        tracing::info!(
            target: "travel",
            "{:?} sends to 0x{:08X} {:?}",
            portal.name,
            landed.cell.0,
            landed.frame.origin
        );
        self.portals.learn_use(&portal, &landed, t);
        if !character.is_empty() {
            // Portal Recall sends a character to where the last portal it used sent it.
            self.portals
                .learn_recall(character, Recall::Spell(PORTAL_RECALL), &landed, t);
        }
        self.save_store();
        self.places.portals_learned += 1;
        std::sync::Arc::make_mut(&mut self.places.store).see(KnownPlace {
            kind: PlaceKind::Portal,
            source: EdgeSource::Learned,
            wcid: portal.wcid,
            name: portal.name.clone(),
            at: portal.position,
            first_seen: t,
            last_seen: t,
            owner: None,
            to: Some(landed),
        });
        self.places.changed = true;
    }
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
