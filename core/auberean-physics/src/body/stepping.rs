//! A body a host moves by hand: no motion of its own, each move carried by the transition against
//! the world's walls, floors and solid objects, as a tick's move is.

use dereth_primitives::Vec3;

use super::Body;
use crate::land::Land;
use crate::world::World;

impl Body {
    /// Moves the body by `d` (in its landblock's frame), facing `heading`, through the transition
    /// against `world`: it slides along what it meets, steps up and down as its step heights allow,
    /// and takes the cell it ends in. Nothing else of a tick runs (no motion, gravity, friction or
    /// turning). A body not yet in the world is put in it ([`World::insert_body`]).
    pub fn move_through(&mut self, land: &Land, world: &mut World, d: Vec3, heading: f32) {
        let key = match self.key.filter(|k| world.handle(*k).is_some()) {
            Some(k) => k,
            None => world.insert_body(land, 0, self),
        };
        self.tell_world(world, key);
        self.pos_anim = self.previous.0;
        self.previous = (self.position, self.heading);
        self.move_by(land, world, key, d, heading);
        if world.position(key) != Some(self.position) {
            world.place(land, key, self.position);
        }
    }
}
