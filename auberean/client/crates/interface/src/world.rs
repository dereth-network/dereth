//! The world as the play screen's panels see it, gathered each frame from the game's view and
//! the world as last drawn.
//!
//! The view carries the objects around the player and what the player's scripts said of them
//! (their names, kinds, owners, radar looks, whether they are fellows and may be attacked, as
//! last asked), the selection, the radar's ranges and the player's radar and names options. Each
//! frame ([`Gather::world`]) those, and the bodies the world was drawn with, make the panels'
//! [`World`].

use std::collections::{BTreeMap, BTreeSet};

use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::runtime::text::from_view;
use auberean_shell::interface::Game;
use auberean_ui_game::world::{Body, Camera, Thing, World};

/// The player's scripts' notices the gathering hears: an object's radar look changed (about
/// the object; about the player, every object's).
pub(crate) mod notice {
    pub const CHANGE_RADAR_LOOK: u32 = 0x0085 << 16 | 0x000A;
}

/// The world gathered for the panels.
#[derive(Debug, Default)]
pub struct Gather {
    /// The objects the last frame knew, to tell which went.
    known: BTreeSet<u64>,
    /// Objects' names as their descriptions carry them, kept by object; and the string download
    /// a name waits on, while the string tables it reads are asked for (it shows no text until
    /// then).
    names: BTreeMap<u64, (TextRef, String, Option<u32>)>,
    /// The block the last world's places were in.
    block: (i32, i32),
}

/// A cell's block and a place in it on the one grid across every block.
#[allow(clippy::cast_precision_loss)]
fn global(cell: u32, origin: [f32; 3]) -> [f32; 3] {
    [
        f32::from(u8::try_from(cell >> 24).unwrap_or(0)) * 160.0 + origin[0],
        f32::from(u8::try_from((cell >> 16) & 0xFF).unwrap_or(0)) * 160.0 + origin[1],
        origin[2],
    ]
}

impl Gather {
    /// The panels' world this frame, and the objects that went since the last. Names are written
    /// by `write`; one whose string tables the data lacks waits on a string download of
    /// `strings` until it is done.
    pub fn world(
        &mut self,
        game: &Game<'_>,
        write: &dyn Fn(&TextRef) -> String,
        strings: &dyn auberean_ui::Assets,
    ) -> Option<(World, Vec<u64>)> {
        let w = game.view?.world.as_ref()?;
        let player = w.player;
        let s = game.status;
        let me = global(s.cell, [s.origin.x, s.origin.y, s.origin.z]);
        let (sin, cos) = (
            dereth_primitives::num::math::sinf(s.heading.to_radians()),
            dereth_primitives::num::math::cosf(s.heading.to_radians()),
        );
        let seen = game.seen;
        self.block = seen.block;
        #[allow(clippy::cast_precision_loss)]
        let block = (seen.block.0 as f32 * 160.0, seen.block.1 as f32 * 160.0);
        let mut things = Vec::new();
        let mut now = BTreeSet::new();
        for f in &w.things {
            now.insert(f.id);
            let drawn = seen.body(f.id);
            let at = match (drawn, f.place) {
                (Some(b), _) => [b.origin.x + block.0, b.origin.y + block.1, b.origin.z],
                (None, Some((cell, origin))) => global(cell, origin),
                (None, None) => continue,
            };
            let d = [at[0] - me[0], at[1] - me[1], at[2] - me[2]];
            // D2-01972
            let from_player = [d[0] * cos - d[1] * sin, d[0] * sin + d[1] * cos, d[2]];
            let name = match (&f.name, self.names.get_mut(&f.id)) {
                (Some(n), _) => n.clone(),
                (None, Some((info, text, download))) if *info == f.own_name => {
                    if let Some(d) = *download {
                        if !strings.string_download_waiting(d) {
                            // Written as a text element writes it: a refused table shows the
                            // client's error message.
                            *download = None;
                            *text =
                                String::from_utf16_lossy(&strings.element_text(&from_view(info)));
                        }
                    }
                    text.clone()
                }
                (None, _) => {
                    let download = strings.start_string_download(&from_view(&f.own_name));
                    let text = if download.is_some() {
                        String::new()
                    } else {
                        write(&f.own_name)
                    };
                    self.names
                        .insert(f.id, (f.own_name.clone(), text.clone(), download));
                    text
                }
            };
            things.push(Thing {
                id: f.id,
                is_player: f.id == player,
                from_player,
                selectable: f.selectable,
                kind: f.kind,
                owner: f.owner,
                container: f.container,
                dead: f.dead,
                radar: f.radar,
                groove: f.groove,
                name,
                name_info: f.name_ref.as_ref().map(from_view),
                faded: drawn.is_some_and(|b| b.faded),
                body: drawn.map(|b| Body {
                    origin: [b.origin.x, b.origin.y, b.origin.z],
                    heading: b.heading,
                    bounds: b
                        .bounds
                        .map(|(lo, hi)| ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])),
                }),
                fellow: f.fellow,
                attackable: f.attackable,
                owner_fellow: f.owner_fellow,
            });
        }
        self.names.retain(|id, (_, _, download)| {
            let keep = now.contains(id);
            if let (false, Some(d)) = (keep, *download) {
                strings.release_string_download(d);
            }
            keep
        });
        // The objects that went: the panels listening on each hear that it is being deleted.
        // D2-04049, D2-08102
        let gone: Vec<u64> = self.known.difference(&now).copied().collect();
        self.known = now;
        let region = &game.world.region.region;
        let world = World {
            player,
            player_kind: w
                .things
                .iter()
                .find(|t| t.id == player)
                .map_or(0, |t| t.kind),
            inside: s.cell & 0xFFFF >= 0x100,
            cell: s.cell,
            blocks: (region.blocks_long, region.blocks_wide),
            heading: s.heading,
            things,
            selection: w.selection.current,
            last_selection: w.selection.last,
            auto_target: w.selection.auto_target,
            should_auto_target: w.selection.should_auto_target,
            ranges: w.radar_ranges.unwrap_or((50.0, 75.0)),
            show_coordinates: w.show_coordinates,
            show_names: w.show_names,
            portal_space: w.portal_space,
            portal_drawn: s.portal_drawn,
            moving: s.moving,
            fellow_xp: w.fellow_xp.clone(),
            last_attacker: w.last_attacker,
            camera: seen.camera.map(|c| Camera {
                position: [c.position.x, c.position.y, c.position.z],
                heading: c.heading,
                pitch: c.pitch,
                fov_y: c.fov_y,
            }),
            picked: game.world_pick,
        };
        Some((world, gone))
    }
}

impl Gather {
    /// The camera the world was last drawn from (`seen`'s), in the block the last world's places
    /// were gathered in.
    #[must_use]
    pub fn camera_in_world(&self, seen: &auberean_scene::seen::Seen) -> Option<Camera> {
        let c = seen.camera?;
        #[allow(clippy::cast_precision_loss)]
        let shift = (
            (seen.block.0 - self.block.0) as f32 * 160.0,
            (seen.block.1 - self.block.1) as f32 * 160.0,
        );
        Some(Camera {
            position: [c.position.x + shift.0, c.position.y + shift.1, c.position.z],
            heading: c.heading,
            pitch: c.pitch,
            fov_y: c.fov_y,
        })
    }
}
