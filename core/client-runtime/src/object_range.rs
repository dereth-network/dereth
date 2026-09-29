//! The client half of the player's object-range checks.
//!
//! [`dereth_client_model::range`] carries the list, the poll timing and every leave edge; the one thing it
//! cannot answer is *how far away is that object*, because `dereth-client-model` holds no positions. This
//! file is that seam and nothing else: `SceneRangeGeometry` resolves two object ids to two
//! physics bodies and calls the three distance functions `dereth_physics::math` already carries.
//!
//! # Why this is the whole client side
//!
//! The object-in-range query is
//!
//! ```text
//! obj = physics body of the object
//! ply = physics body of the player
//! if either is missing: not in range
//! if ignore_z_delta: return xy_distance(obj, ply) <= range
//! return distance(obj, ply, use_radii) <= range
//! ```
//!
//! and the object distance is point distance when `use_radii` is false and cylinder distance over
//! each body's radius, height and position when it is true, where an object's radius and height
//! are its setup's radius and height times its scale.
//!
//! [`dereth_physics::PhysicsObj::radius`] and [`dereth_physics::PhysicsObj::height`] are those two
//! setup fields times the object's scale. `ObjectPhysics::sync` writes `scale` from
//! `PhysicsDesc.object_scale`, and sphere-path initialization scales the path spheres by it too.
//! The getters must agree with the spheres: a scaled sphere with an unscaled step height costs
//! remote creatures their contact bit every other sub-step; see
//! [`dereth_physics::obj::PhysicsObj::step_up_height`].
//!
//! The player's body is [`crate::character::Character`]'s, not
//! [`crate::object_physics::ObjectPhysics`]', because the client has exactly one physics body for
//! the player and `ObjectPhysics::sync` excludes his id on purpose. Both live in the same
//! [`dereth_physics::PhysicsWorld`], which is `Character::world`.

use dereth_primitives::ObjectId;

/// [`dereth_client_model::range::ObjectRangeGeometry`] over the live physics world.
pub struct SceneRangeGeometry<'a> {
    world: &'a dereth_physics::PhysicsWorld,
    physics: &'a crate::object_physics::ObjectPhysics,
    player_id: ObjectId,
    player_handle: dereth_physics::PhysHandle,
}

impl std::fmt::Debug for SceneRangeGeometry<'_> {
    /// The two borrowed tables are not printable and are not the interesting part; what a failure
    /// message wants is which body the distances are measured *from*.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneRangeGeometry")
            .field("player_id", &self.player_id)
            .field("player_handle", &self.player_handle)
            .field("bodies", &self.physics.len())
            .finish_non_exhaustive()
    }
}

impl<'a> SceneRangeGeometry<'a> {
    /// `None` when there is no local body or no player id — before `0xF746`, or between scenes.
    /// With no body the checks are not run at all rather than run against a missing player, which
    /// would report every watched object out of range and close every panel on the loading screen.
    ///
    /// `physics` and `player_id` are taken apart rather than as one `&ObjectStream` so that the
    /// caller keeps `&mut ObjectStream::world` while this holds `&ObjectStream::physics`; they are
    /// disjoint fields of the object stream and the client's two tables likewise.
    #[must_use]
    pub fn new(
        character: Option<&'a crate::character::Character>,
        physics: &'a crate::object_physics::ObjectPhysics,
        player_id: Option<ObjectId>,
    ) -> Option<Self> {
        let character = character?;
        Some(Self {
            world: &character.world,
            physics,
            player_id: player_id?,
            player_handle: character.handle,
        })
    }

    /// The id's physics body, or `None`.
    fn body(&self, id: ObjectId) -> Option<&dereth_physics::obj::PhysicsObj> {
        let h = if id == self.player_id {
            self.player_handle
        } else {
            // `ObjectPhysics::sync` deliberately does not create a second body when the static
            // scene already owns this object id. The lookup resolves both tables;
            // prefer the server-object handle when present, then fall back to the static body.
            self.physics
                .handle(id)
                .or_else(|| self.world.by_object_id(id))?
        };
        self.world.get(h)
    }
}

impl dereth_client_model::range::ObjectRangeGeometry for SceneRangeGeometry<'_> {
    fn distance(
        &self,
        object: ObjectId,
        player: ObjectId,
        use_radii: bool,
        ignore_z_delta: bool,
    ) -> Option<f32> {
        let obj = self.body(object)?;
        let ply = self.body(player)?;
        Some(if ignore_z_delta {
            dereth_physics::math::xy_distance(&obj.position, &ply.position)
        } else if use_radii {
            dereth_physics::math::cylinder_distance(
                obj.radius(),
                obj.height(),
                &obj.position,
                ply.radius(),
                ply.height(),
                &ply.position,
            )
        } else {
            dereth_physics::math::distance(&obj.position, &ply.position)
        })
    }
}
