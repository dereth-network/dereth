//! Which joins a kit piece is offered at, for an AC2 kit whose rooms are entered through a room
//! entrance (the tomb): only the joins AC2's own dungeons of that kit make. Across them, no hall
//! meets a room tile directly, and an entrance's first doorway meets only a room tile; a room
//! tile meets only another of the kit's room tiles or an entrance's first doorway. Everything
//! else is offered as for any kit: halls to halls and to other kits' pieces, and an entrance's
//! second doorway to a hall, another kit's piece or another entrance's second doorway.
//!
//! This narrows what is offered, never what a layout may hold: a layout that already joins
//! otherwise is loaded, built and checked as before.

use std::collections::BTreeMap;

use dereth_primitives::DataId;

use super::Session;
use crate::grid::{Doorway, PiecePlacement};
use crate::kit::Kind;
use crate::source::Link;

/// What a doorway of a cell is, for the joins of a kit with a room entrance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinRole {
    /// One of the kit's room tiles.
    Tile,
    /// One of the kit's room entrances, by doorway: the first opens on the room.
    Entrance(usize),
    /// Anything else: a hall, another kit's piece, the world's own pieces.
    Other,
}

/// Whether AC2's own dungeons join a doorway of role `a` to one of role `b`.
#[must_use]
pub fn join_seen(a: JoinRole, b: JoinRole) -> bool {
    use JoinRole::{Entrance, Tile};
    match (a, b) {
        (Tile, x) | (x, Tile) => matches!(x, Tile | Entrance(0)),
        (Entrance(0), x) | (x, Entrance(0)) => x == Tile,
        _ => true,
    }
}

impl Session {
    /// Part `part` of environment `environment`'s space, judged as its AC2 kit's are
    /// ([`crate::ac2_kit::Kit::space`]) and reaching as far past its doorways as its piece does
    /// ([`crate::ac2_kit::reach`]); any other environment's, its cell tree's, reaching none.
    #[must_use]
    pub fn ac2_shape(&self, environment: DataId, part: u16) -> Option<crate::room::Shape> {
        let env = self.environment(environment)?;
        let piece = self
            .compiled
            .ac2_pieces
            .iter()
            .find(|p| p.environment == environment);
        let reach = piece.map_or(0.0, |p| crate::ac2_kit::reach(p.mesh));
        let shape = match piece {
            Some(p) if p.kit.space() == crate::ac2_kit::Space::Voxels => self
                .ac2_voxels
                .borrow_mut()
                .entry((p.mesh, part))
                .or_insert_with(|| crate::room::Shape::of_voxels(&env, part))
                .clone(),
            _ => crate::room::Shape::of(&env, part),
        };
        shape.map(|s| s.reaching(reach))
    }

    /// What doorway `slot` of a cell of environment `environment` is, for the joins of its kit.
    fn join_role(
        &self,
        environment: DataId,
        slot: usize,
        tiles: &mut BTreeMap<DataId, bool>,
    ) -> JoinRole {
        let Some(piece) = self
            .compiled
            .ac2_pieces
            .iter()
            .find(|p| p.environment == environment)
        else {
            return JoinRole::Other;
        };
        let Some(entrance) = piece.kit.room_entrance() else {
            return JoinRole::Other;
        };
        if piece.label.starts_with(entrance) {
            return JoinRole::Entrance(slot);
        }
        let kit = piece.kit;
        let tile = *tiles.entry(environment).or_insert_with(|| {
            self.environment(environment)
                .and_then(|env| crate::kit::form_on(&env, &kit.grid()))
                .is_some_and(|f| f.kind == Kind::Tile)
        });
        if tile {
            JoinRole::Tile
        } else {
            JoinRole::Other
        }
    }

    /// The environment and doorway of landblock `lb`'s cell that `link` names.
    fn linked(&self, lb: u16, link: &Link) -> Option<(DataId, usize)> {
        let edits = self.packs[self.target]
            .edits
            .iter()
            .filter(|e| e.landblock == lb);
        match link {
            Link::Cell { cell, portal } => edits
                .flat_map(|e| &e.cells)
                .find(|a| a.item.name == *cell)
                .map(|a| (a.item.environment, *portal)),
            Link::Base { cell, portal } => edits
                .flat_map(|e| &e.replaces)
                .find(|r| r.item.cell == *cell)
                .map(|r| (r.item.environment, *portal)),
            _ => None,
        }
    }

    /// `placements` of the piece of environment `piece` in landblock `lb`, less those that join
    /// one of the dungeon's `doorways` in a way AC2's own dungeons never do ([`join_seen`]), at any
    /// doorway they join.
    #[must_use]
    pub fn ac2_offered(
        &self,
        lb: u16,
        piece: DataId,
        doorways: &[Doorway],
        placements: Vec<PiecePlacement>,
    ) -> Vec<PiecePlacement> {
        let mut tiles = BTreeMap::new();
        placements
            .into_iter()
            .filter(|p| {
                p.links.iter().all(|part| {
                    part.iter().enumerate().all(|(slot, link)| {
                        let ends: Vec<&Link> = match link {
                            Link::Many(many) => many.iter().collect(),
                            l => vec![l],
                        };
                        ends.into_iter().all(|end| {
                            if !doorways.iter().any(|d| d.link == *end) {
                                return true;
                            }
                            let Some((env, portal)) = self.linked(lb, end) else {
                                return true;
                            };
                            join_seen(
                                self.join_role(piece, slot, &mut tiles),
                                self.join_role(env, portal, &mut tiles),
                            )
                        })
                    })
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use JoinRole::{Entrance, Other, Tile};

    #[test]
    fn the_joins_are_those_of_the_census_less_the_three_it_never_makes() {
        // Never made: a hall straight on a room tile, an entrance's first doorway on a hall, and
        // a room tile on another kit's piece or on an entrance's second doorway.
        for (a, b) in [
            (Other, Tile),
            (Entrance(0), Other),
            (Tile, Entrance(1)),
            (Entrance(0), Entrance(0)),
            (Entrance(0), Entrance(1)),
        ] {
            assert!(!join_seen(a, b), "{a:?} - {b:?}");
            assert!(!join_seen(b, a), "{b:?} - {a:?}");
        }
        // Made, each kind the census counts: tile to tile, an entrance's first doorway to a tile,
        // its second to a hall; hall to hall; and the three of other kinds: a hall to another
        // kit's piece, an entrance's second doorway to another kit's piece, and an entrance to an
        // entrance through their second doorways.
        for (a, b) in [
            (Tile, Tile),
            (Entrance(0), Tile),
            (Entrance(1), Other),
            (Other, Other),
            (Entrance(1), Entrance(1)),
        ] {
            assert!(join_seen(a, b), "{a:?} - {b:?}");
            assert!(join_seen(b, a), "{b:?} - {a:?}");
        }
    }
}
