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

impl Learning {
    /// The places, as a borrow of this field alone: travel reads them while its trip is borrowed.
    pub(super) fn store(&self) -> &PlaceStore {
        &self.store
    }
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
    /// How it may be used, when its description says; `1` for not at all.
    pub useability: Option<u32>,
    /// Its physics state word.
    pub physics_state: u32,
}

impl PlaceSeen {
    /// What place it is ([`PlaceKind::of`]); none for a portal that can be neither used nor walked
    /// into, as a destroyed one is: it may not be used, and nothing collides with it.
    #[must_use]
    pub fn kind(&self) -> Option<PlaceKind> {
        const USEABLE_NO: u32 = 1;
        const IGNORE_COLLISIONS: u32 = 0x10;
        let kind = PlaceKind::of(self.bitfield, self.item_type)?;
        let destroyed = kind == PlaceKind::Portal
            && self.useability == Some(USEABLE_NO)
            && self.physics_state & IGNORE_COLLISIONS != 0;
        (!destroyed).then_some(kind)
    }

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
        self.merge_package_places();
        self.merge_hinted_places();
        self.merge_published_places();
        tracing::info!(
            target: "travel",
            "places of {world:?}: {}",
            self.places.store.places().len()
        );
    }

    /// The towns and landmarks the server publishes as the names the world gives its own places
    /// ([`PlaceStore::own_names`]), or sends as the world's own places document
    /// ([`Self::take_world_places`]), which the maps name in place of the game's map's notes;
    /// `None` until it does, and for a world whose places it does not say are its own.
    #[must_use]
    pub fn published_own_places(
        &self,
    ) -> Option<&dereth_client_contract::panels::map::WorldPlaces> {
        self.published_own_places.as_ref()
    }

    /// Take in the places the server publishes (the place store's text form, its places of
    /// source `published`): merged now under what the character saw, and into each world's
    /// places as they load, and never written; and, when it says its towns and landmarks are the
    /// world's own names, those for the maps ([`Self::published_own_places`]). How many it
    /// publishes.
    ///
    /// # Errors
    /// The text is not a place store.
    pub fn set_published_places(&mut self, text: &str) -> Result<usize, String> {
        let table = PlaceStore::from_text(text).map_err(|e| e.to_string())?;
        self.published_own_places = crate::map_places::published_own_places(&table)
            .map(|own| dereth_client_contract::panels::map::WorldPlaces(own.into()));
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

    /// Take in the world's own places document as the server that patched the client sends it
    /// (the places document of the world's overlay folder, which the client's copy of the world's
    /// data does not hold): its towns and landmarks name the maps, as the server's published own
    /// names do ([`Self::published_own_places`]). How many it names; `None`, and nothing changed,
    /// for text that is not a places document.
    pub fn take_world_places(&mut self, document: &str) -> Option<usize> {
        let own = crate::map_places::own_places(document)?;
        let n = own.len();
        self.published_own_places =
            Some(dereth_client_contract::panels::map::WorldPlaces(own.into()));
        Some(n)
    }

    /// Forget what the server published: its portal table, its places and the names it gives the
    /// world's own, none of which are another world's.
    pub(super) fn forget_published(&mut self) {
        self.published.clear();
        self.published_places.clear();
        self.published_own_places = None;
    }

    /// The places of the world's era's package ([`super::StoreFiles::hints`]), as hints: under
    /// what the character saw and what the server publishes.
    fn merge_package_places(&mut self) {
        let Some(path) = self
            .files
            .hints
            .as_ref()
            .map(|d| d.join(super::HINT_PLACES))
        else {
            return;
        };
        match crate::platform::files::read_to_string(&path) {
            Ok(text) => match PlaceStore::from_text(&text) {
                Ok(p) => std::sync::Arc::make_mut(&mut self.places.store).merge(
                    p.places().iter().cloned().map(|mut k| {
                        k.source = EdgeSource::Hint;
                        k
                    }),
                ),
                Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
            },
            Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
        }
    }

    /// The places a place package gave as text ([`super::Travel::take_hints`]), as hints.
    pub(super) fn merge_hinted_places(&mut self) {
        if self.places.store.world.is_empty() || self.hinted_places.is_empty() {
            return;
        }
        std::sync::Arc::make_mut(&mut self.places.store).merge(self.hinted_places.iter().cloned());
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
        let Some(kind) = o.kind() else {
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
        self.recall_landing(at, portal_space, now, character);
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
