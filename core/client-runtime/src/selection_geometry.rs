//! The client half of selection's per-object geometry.
//!
//! [`dereth_client_model::selection`] carries the cycle, the ordering and all five filters; the
//! one thing it cannot answer is *where is that object, and what is its state word*, because
//! `dereth-client-model` holds neither positions nor physics bodies. This file is that seam and nothing
//! else, in the same shape [`crate::object_range::SceneRangeGeometry`] is for range queries.
//!
//! # What retail asks, and where each answer comes from here
//!
//! Retail's next-target selection asks two questions of every candidate's physics body:
//!
//! * the candidate's offset in player space (the player-space conversion);
//! * `state & 0x100000` — `CLOAKED_PS`, a cloaked candidate is skipped;
//!
//! and the [`dereth_client_model::selection::SelectionType::CompassItem`] arm asks a third,
//! `state & REPORT_COLLISIONS_AS_ENVIRONMENT_PS`. Both bit tests read the **same state word**
//! live off the same object
//! the conversion was performed on.
//!
//! * The **position** half is a cell-aware local-to-local conversion of
//!   the object's position with a zero offset into the player's position frame, so
//!   it is a real offset across a landblock boundary and not a coordinate subtraction. That is
//!   [`dereth_physics::math::localtolocal`], and it is the identical call
//!   [`crate::hud::Hud::sync`] already makes once per frame for the radar and
//!   [`crate::hud::Hud::speaker_player_space`] reads back for the chat hearing-range test. The
//!   inputs are the same two: the local body's `Position` for the player, and
//!   [`crate::objects::Presence::position`] for the object.
//! * The **state** half is [`dereth_client_model::objects::PhysicsPresence::state`], reached through
//!   [`crate::objects::ObjectStream::physics_state`]. That is the whole physics state word —
//!   the descriptor's at create, and thereafter whatever `0xF74B Item_SetState` last set through
//!   [`crate::objects::ObjectStream::set_state`]. It is the same word
//!   [`crate::object_physics::ObjectPhysics::sync`] carries onto the live body,
//!   so this seam and the body agree by construction:
//!   **there is one word, not two kept in step.**
//!
//! **This is why there are no hard-coded `false`s below for `cloaked` and
//! `reports_collisions_as_environment`.** `dereth_client_model::objects::PhysicsPresence` keeps
//! the whole state word, `0xF74B` updates it, and it is the only copy, so both bits have a
//! producer and this seam reads it.
//!
//! # Why the whole table is built at once, and why that is faithful
//!
//! Retail runs the player-space conversion **twice per candidate** (once for the filter, once
//! for the distance) with the same arguments and the same answer, and reads
//! the radar radius once per candidate for a value that cannot move during the scan. Nothing
//! inside next-target selection can move an object, and the action handler's retry arms call it twice in a row
//! within one input event, so a snapshot taken once per press is the same geometry every one of
//! those reads would have seen. Taking it once also means the position a candidate is *ordered*
//! by, the position its blip is *drawn* at and the range a speaker is *heard* at cannot drift
//! apart — the same argument [`crate::hud::Hud::speaker_player_space`] makes for sharing the
//! radar's snapshot.
//!
//! # `None` is one condition, not three
//!
//! The player-space conversion returns 0 in exactly three cases: the player has no physics
//! object, the argument object is null, or **the object's
//! cell is null**. Next-target selection tests all three separately and skips on any of them, so one
//! `Option` reproduces all three — which is the reading
//! `SelectionPhysics` already carries. Here they are, in order:
//!
//! | retail | here |
//! |---|---|
//! | the world view has no player | `origin` is `None` — no local body, so the table is empty |
//! | the object lookup finds nothing | the id has no [`crate::objects::Presence`] |
//! | the object has no cell | `Presence::position` is `None` — a contained or wielded object |
//!
//! # The one place this is not `object_range.rs`
//!
//! [`crate::object_range::SceneRangeGeometry`] resolves ids to `dereth_physics::PhysicsObj` bodies,
//! because `dereth_client_model::range::objects_in_range` needs each body's radius and height. This seam does not: retail reads
//! the object's `position` and its `state`, and both of those exist for an object the server has
//! placed whether or not this build gave it a body. Going through
//! [`crate::object_physics::ObjectPhysics`] would have silently dropped every object with no
//! `setup_id` — an object retail *would* have offered as a candidate — so it does not.

use std::collections::BTreeMap;

use dereth_client_model::selection::{
    SelectionPhysics, CLOAKED_PS, REPORT_COLLISIONS_AS_ENVIRONMENT_PS,
};
use dereth_primitives::{ObjectId, Position, Vec3};

/// One `SelectionPhysics` per object held by object maintenance, taken once and read many times.
///
/// Owned rather than borrowed on purpose: the selection walk takes `&mut World`
/// **and** the geometry closure, and in this build the world and the presences are two fields of
/// one [`crate::objects::ObjectStream`]. A snapshot ends the immutable borrow at construction.
#[derive(Debug, Default, Clone)]
pub struct SceneSelectionPhysics {
    table: BTreeMap<ObjectId, SelectionPhysics>,
}

impl SceneSelectionPhysics {
    /// Build the snapshot. `origin` is the local body's `Position` —
    /// the player's position, the same one [`crate::hud::ViewerFrame::position`] carries
    /// and the same one [`crate::object_range::SceneRangeGeometry`] measures from.
    ///
    /// With no `origin` the table is **empty**, which makes every `phys(id)` answer `None` and
    /// therefore makes `select_next` select nothing. That is the player-space conversion's own
    /// null-player return propagated to every candidate, and it is the state a
    /// headless frame or a loading screen is in.
    ///
    /// It is fed from every object the server has placed, matching the object-table lookup,
    /// rather than only from the visible
    /// list, because next-target selection asks this question about the *reference* object
    /// (the selected id, else the previous selected id) as well, and the reference need not be
    /// visible.
    ///
    /// **It takes the stream rather than an iterator of presences.** Two of the three
    /// facts below are bits of the physics state word, which this build stores once in
    /// `dereth_client_model::objects::PhysicsPresence` — see [`crate::objects::Presence`] for why it cannot
    /// live in the client's table. A presence alone therefore cannot answer the question this
    /// snapshot is built to answer.
    #[must_use]
    pub fn new(origin: Option<&Position>, stream: &crate::objects::ObjectStream) -> Self {
        let mut table = BTreeMap::new();
        let Some(at) = origin else {
            return Self { table };
        };
        for (id, p) in stream.presences() {
            // The object's state word, from the one table that holds it. An object the game table does not
            // hold takes `PhysicsPresence::default()`'s zero word, which answers "not cloaked,
            // reports as an object" — the same answer a zero descriptor word gives.
            let state = stream.physics_state(id).unwrap_or(0);
            // An object with no cell — the third of the player-space conversion's three zero returns.
            let Some(pos) = p.position.as_ref() else {
                continue;
            };
            let v = dereth_physics::math::localtolocal(at, pos, Vec3::ZERO);
            table.insert(
                id,
                SelectionPhysics {
                    player_space: (v.x, v.y, v.z),
                    // The filter is `state & 0x100000`: the `CLOAKED_PS` bit in the physics
                    // state word, confirmed against retail.
                    cloaked: state & CLOAKED_PS != 0,
                    // The same shape with `0x200000`, read only by the `CompassItem` arm.
                    reports_collisions_as_environment: state & REPORT_COLLISIONS_AS_ENVIRONMENT_PS
                        != 0,
                },
            );
        }
        Self { table }
    }

    /// The stored position plus the conversion — the closure the selection walk calls once per
    /// candidate and once for the reference.
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<SelectionPhysics> {
        self.table.get(&id).copied()
    }

    /// During object deletion the physical object is already gone, while its old Weenie
    /// still exists and its client Presence projection has not yet been retired. Synchronous
    /// selection subscribers must observe that null object lookup, not resurrect the snapshot.
    pub fn for_current_objects(&self, world: &dereth_client_model::World) -> Self {
        let mut current = self.clone();
        current.table.retain(|id, facts| {
            let Some(body) = world.physics(*id) else {
                return false;
            };
            if body.cell.is_none() {
                return false;
            }
            facts.cloaked = body.state & CLOAKED_PS != 0;
            facts.reports_collisions_as_environment =
                body.state & REPORT_COLLISIONS_AS_ENVIRONMENT_PS != 0;
            true
        });
        current
    }

    /// How many objects the snapshot could answer for. A denominator: a cycle that selects
    /// nothing because the table is empty and one that selects nothing because every candidate was
    /// filtered are different failures, and only this separates them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}
