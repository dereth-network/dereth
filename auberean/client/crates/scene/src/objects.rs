//! The scene's side of the client's objects, which the client keeps as the game's: the physics
//! object it asks the scene to make for each of its objects, the cell each body stands in, and the
//! bodies that leave the world when the cells they stand in are let go of.
//!
//! - **Made through the client.** The physics object of each object the scene draws is made when
//!   the client asks for it, from the physics record the object's repository names; the player's
//!   too. The player's arrival makes the camera's two stand-ins in the client's objects, and has
//!   the player's scripts preload what they keep about the player.
//! - **The cells told.** After each frame the client is told the cell each body stands in, and
//!   that a body out of the world stands in none; the player's cell is told before any cell is let
//!   go of.
//! - **Cells let go of.** When the land lets a landblock go, and when the interior cells about the
//!   player change, the client is asked which of the objects standing in each cell let go of leave
//!   the world: those that move and hang from nothing.
//! - **Modes.** An object whose body goes into a mode (asked of it, or with a behavior that carries
//!   one) is told the mode, as a notice about it.
//! - **The region.** The region the client's world takes up starts the land's free blocks afresh.

use std::collections::BTreeMap;

use auberean_client_sdk::model::{ids, Object};
use auberean_client_sdk::runtime::world::{self, PhysicsObjOrder, WorldScene};
use auberean_client_sdk::runtime::Body as ClientBody;
use auberean_client_sdk::Client;

use crate::{others, Scene, FIRST_INTERIOR};

/// The interface's function the player's scripts preload what they keep about the player with.
pub const PRELOAD_SINGLETONS: u32 = (0x7F << 16) | 0x5C;

/// The notice an object is told the mode its body went into with (the mode its one word).
pub const MODE_NOTICE: u32 = 0x0001_040F;

/// The client's object whose physics object the scene is asked to make, and for the player, the
/// body the client placed.
pub(crate) struct Creating {
    pub object: Object,
    pub player: Option<ClientBody>,
}

/// The landblock a cell is in.
fn block_of(cell: u32) -> (u8, u8) {
    (
        u8::try_from(cell >> 24).unwrap_or(0),
        u8::try_from((cell >> 16) & 0xFF).unwrap_or(0),
    )
}

/// Whether block `b` is within the near window about block `about`.
fn about(b: (u8, u8), about: Option<(u8, u8)>) -> bool {
    about.is_some_and(|a| {
        let r = u8::try_from(others::NEAR_RADIUS).unwrap_or(1);
        b.0.abs_diff(a.0) <= r && b.1.abs_diff(a.1) <= r
    })
}

impl WorldScene for Scene<'_> {
    fn make_physics_obj(&mut self, order: &PhysicsObjOrder) -> bool {
        let Some(c) = self.creating.take().filter(|c| c.object.id == order.id) else {
            return false;
        };
        if let Some(body) = c.player {
            self.player = self.make_player(&c.object, order.physics_record, &body);
            return self.player.is_some();
        }
        let looks = others::Looks {
            records: &self.world.records,
            anims: &self.anims,
            world: &mut self.table,
            trees: &mut self.trees,
        };
        self.others.make_physics_obj(order, &c.object, looks)
    }

    fn is_static(&self, id: u64) -> bool {
        if let Some(p) = self.player.as_ref().filter(|p| p.id == id) {
            return p.body.is_static();
        }
        if let Some(t) = self.temporaries.get(id) {
            return t.body.is_static();
        }
        self.others.is_static(id)
    }

    fn has_parent(&self, id: u64) -> bool {
        if id == ids::camera_parent() {
            return self.stand_ins[0];
        }
        if id == ids::camera_target() {
            return self.stand_ins[1];
        }
        self.temporaries
            .get(id)
            .is_some_and(|t| t.holder.is_some() || t.body.has_parent)
    }

    fn leave_visibility(&mut self, id: u64) {
        if self.others.leave_visibility(id, false, &mut self.table) {
            return;
        }
        // An object the client made leaves the world for good: it goes as one asked to be
        // destroyed does.
        if let Some(t) = self.temporaries.get_mut(id).filter(|t| t.body.in_world) {
            t.body
                .leave_world(&self.land, &mut self.table, &mut self.objects, None);
            self.left.push(id);
        }
    }

    fn refresh_landscape(&mut self) -> bool {
        self.built = None;
        true
    }
}

impl Scene<'_> {
    /// The physics objects of the client's objects the scene draws and has none for (or one for an
    /// older incarnation), each made as the client asks.
    pub(crate) fn create_physics_objs(&mut self, client: &mut Client) {
        let player = self.player.as_ref().map(|p| p.id);
        let wanted: Vec<u64> = client
            .model
            .objects()
            .filter(|o| Some(o.id) != player && others::drawn(o))
            .filter(|o| self.others.wants_physics_obj(o))
            .map(|o| o.id)
            .collect();
        for id in wanted {
            let Some(object) = client.model.object(id).cloned() else {
                continue;
            };
            self.creating = Some(Creating {
                object,
                player: None,
            });
            client.create_physics_obj(id, self);
            self.creating = None;
        }
    }

    /// The player's physics object, made as the client asks, where the client placed it.
    pub(crate) fn create_player(&mut self, client: &mut Client, o: &Object) {
        self.player = None;
        self.creating = Some(Creating {
            object: o.clone(),
            player: Some(client.body.clone()),
        });
        client.create_physics_obj(o.id, self);
        self.creating = None;
        if self.player.is_some() {
            self.player_arrived(client);
        }
    }

    /// The player made: the camera's stand-ins its setup has are among the client's objects, and
    /// its scripts preload what they keep about it.
    // D2-08074, D2-04688
    fn player_arrived(&mut self, client: &mut Client) {
        for (made, id) in self
            .stand_ins
            .into_iter()
            .zip([ids::camera_parent(), ids::camera_target()])
        {
            if made {
                client.queue.create_dummy_object(id);
            }
        }
        if let Some(r) = client.interface(PRELOAD_SINGLETONS, &[]) {
            tracing::debug!(?r, "the player's scripts preloaded");
        }
    }

    /// The camera's stand-ins go from the client's objects.
    pub(crate) fn drop_camera_stand_ins(client: &mut Client) {
        client.queue.remove_object(ids::camera_parent());
        client.queue.remove_object(ids::camera_target());
    }

    /// The client is told the cell each body stands in where that changed since it was last told
    /// (none for a body out of the world, or one gone while its object stays).
    pub(crate) fn report_cells(&mut self, client: &mut Client) {
        let mut now: BTreeMap<u64, u32> = BTreeMap::new();
        if let Some(p) = &self.player {
            now.insert(p.id, p.body.position.cell.0);
        }
        for (id, cell) in self.others.cells() {
            now.insert(id, cell.unwrap_or(0));
        }
        for (id, _) in self.temporaries.ids() {
            if let Some(t) = self.temporaries.get(id) {
                now.insert(
                    id,
                    if t.body.in_world {
                        t.body.position.cell.0
                    } else {
                        0
                    },
                );
            }
        }
        for (id, cell) in &now {
            if self.reported.get(id) != Some(cell) {
                client.queue.change_cell(*id, (*cell != 0).then_some(*cell));
            }
        }
        for (id, cell) in &self.reported {
            if *cell != 0 && !now.contains_key(id) {
                client.queue.change_cell(*id, None);
            }
        }
        self.reported = now;
    }

    /// The player's cell told to the client, before any cell is let go of.
    fn report_player_cell(&mut self, client: &mut Client) {
        let Some(p) = &self.player else {
            return;
        };
        let cell = p.body.position.cell.0;
        if self.reported.get(&p.id) != Some(&cell) {
            client.queue.change_cell(p.id, Some(cell));
            self.reported.insert(p.id, cell);
        }
    }

    /// The cells let go of since last asked: every land cell of each landblock the land let go
    /// of, and the interior cells (those the client was told a body stands in) of the blocks no
    /// longer about the player's; the objects standing in each leave the world as the client says.
    // D2-03638, D2-03871
    pub(crate) fn release_objects(&mut self, client: &mut Client) {
        self.report_player_cell(client);
        let mut cells: Vec<u32> = Vec::new();
        for (x, y) in self.near.take_released() {
            let base = u32::from(x) << 24 | u32::from(y) << 16;
            cells.extend((1..=64).map(|i| base | i));
            self.others.set_block_on_free_list((x, y), true);
        }
        for block in self.near.take_handed_out() {
            self.others.set_block_on_free_list(block, false);
        }
        let now = self
            .player
            .as_ref()
            .map(|p| block_of(p.body.position.cell.0));
        if now != self.interiors_about {
            let was = std::mem::replace(&mut self.interiors_about, now);
            let mut inside: Vec<u32> = self
                .reported
                .values()
                .copied()
                .filter(|c| c & 0xFFFF >= FIRST_INTERIOR)
                .filter(|c| about(block_of(*c), was) && !about(block_of(*c), now))
                .collect();
            inside.sort_unstable();
            inside.dedup();
            cells.extend(inside);
        }
        for cell in cells {
            world::release_obj_cell(&client.queue, &client.model, cell, self);
            self.table
                .release_cell_watchers(dereth_primitives::CellId(cell));
        }
    }

    /// Each object whose body went into a mode since the last frame is told the mode, in order: a
    /// notice about the object for what listens to it.
    // D2-08187, D2-04054
    pub(crate) fn tell_modes(&mut self, client: &mut Client) {
        for (id, mode) in std::mem::take(&mut self.told_modes) {
            client
                .notices
                .push(auberean_client_sdk::runtime::host::PanelNotice {
                    native: MODE_NOTICE,
                    about: Some(id),
                    words: vec![mode],
                    ..Default::default()
                });
        }
    }

    /// The region the client's world takes up (0 to let it go): the land's free blocks start
    /// afresh.
    pub(crate) fn take_up_region(&mut self, record: u32) {
        tracing::info!(
            record = format_args!("{record:#010x}"),
            "the world takes up a region"
        );
        self.near.notify_change_region();
    }
}
