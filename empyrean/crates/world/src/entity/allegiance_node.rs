// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AllegianceNode.cs
//! Port of `Source/ACE.Server/Entity/AllegianceNode.cs`.
//!
//! ACE's nodes are heap objects linked by references (`Monarch`, `Patron`, `Vassals`). Here the
//! nodes of one allegiance live in an arena ([`AllegianceTree`], held by the `Allegiance` world
//! object) and link by index ([`NodeId`]). A node reached from outside its allegiance (a
//! player's `AllegianceNode`, `AllegianceManager.Players`) is named by a [`NodeRef`]: the
//! allegiance object's guid and the node's player guid, resolved on use.
//! See `allegiance.rs` for how allegiance instances map to guids (DIVERGE arch).

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::ObjectGuid;

use crate::entity::i_player::{self, IPlayer};
use crate::managers::player_manager;
use crate::World;

/// The index of a node in its [`AllegianceTree`].
pub type NodeId = usize;

/// A node named from outside its allegiance: `(allegiance, player)`. ACE holds the node object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef {
    /// The guid of the `Allegiance` world object the node belongs to (`node.Allegiance.Guid`).
    pub allegiance: ObjectGuid,
    /// `node.PlayerGuid`.
    pub player: ObjectGuid,
}

// ACE: AllegianceNode
#[derive(Debug, Clone)]
pub struct AllegianceNode {
    // ACE: AllegianceNode.PlayerGuid
    pub player_guid: ObjectGuid,
    /// `Allegiance`: the guid of the allegiance object whose tree holds this node (0 for the
    /// temporary allegiance `new Allegiance(monarch)` builds, which has no guid).
    // ACE: AllegianceNode.Allegiance
    pub allegiance: ObjectGuid,
    // ACE: AllegianceNode.Monarch
    pub monarch: NodeId,
    // ACE: AllegianceNode.Patron
    pub patron: Option<NodeId>,
    /// `Vassals`: null until `BuildChain` runs, then keyed by the vassal's guid in insertion order.
    // ACE: AllegianceNode.Vassals
    pub vassals: Option<DotNetDict<u32, NodeId>>,
    // ACE: AllegianceNode.Rank
    pub rank: u32,
}

/// The nodes of one allegiance.
#[derive(Debug, Clone, Default)]
pub struct AllegianceTree {
    pub nodes: Vec<AllegianceNode>,
}

impl AllegianceTree {
    // ACE: AllegianceNode.AllegianceNode
    /// `new AllegianceNode(playerGuid, allegiance, monarch = null, patron = null)`: `Monarch` is
    /// the node itself when no monarch is given.
    pub fn new_node(
        &mut self,
        player_guid: ObjectGuid,
        allegiance: ObjectGuid,
        monarch: Option<NodeId>,
        patron: Option<NodeId>,
    ) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(AllegianceNode {
            player_guid,
            allegiance,
            monarch: monarch.unwrap_or(id),
            patron,
            vassals: None,
            rank: 0,
        });
        id
    }

    /// The node at `id`.
    #[must_use]
    pub fn node(&self, id: NodeId) -> &AllegianceNode {
        &self.nodes[id]
    }

    // ACE: AllegianceNode.IsMonarch
    #[must_use]
    pub fn is_monarch(&self, id: NodeId) -> bool {
        self.nodes[id].patron.is_none()
    }

    // ACE: AllegianceNode.HasVassals
    #[must_use]
    pub fn has_vassals(&self, id: NodeId) -> bool {
        self.total_vassals(id) > 0
    }

    // ACE: AllegianceNode.TotalVassals
    #[must_use]
    pub fn total_vassals(&self, id: NodeId) -> i32 {
        self.nodes[id]
            .vassals
            .as_ref()
            .map_or(0, |v| i32::try_from(v.len()).unwrap_or(i32::MAX))
    }

    /// `Vassals.Values`, in dictionary order.
    ///
    /// # Panics
    /// When `Vassals` is null (ACE's `NullReferenceException`; never after `BuildChain`).
    #[must_use]
    pub fn vassals(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes[id]
            .vassals
            .as_ref()
            .expect("ACE: AllegianceNode.Vassals is null (NullReferenceException)")
            .values()
            .copied()
            .collect()
    }

    // ACE: AllegianceNode.TotalFollowers
    /// Every node under this one, at any depth. `int` arithmetic (unchecked).
    #[must_use]
    pub fn total_followers(&self, id: NodeId) -> i32 {
        let mut total_followers = 0i32;

        for vassal in self.vassals(id) {
            total_followers =
                total_followers.wrapping_add(self.total_followers(vassal).wrapping_add(1));
        }

        total_followers
    }

    // ACE: AllegianceNode.BuildChain
    /// Builds this node's vassals from `patron_vassals` (patron guid to its vassals, in member
    /// order), depth first, then its rank. (ACE also passes the member list, which it never reads.)
    pub fn build_chain(
        &mut self,
        id: NodeId,
        allegiance: ObjectGuid,
        patron_vassals: &DotNetDict<u32, Vec<IPlayer>>,
    ) {
        let vassals = patron_vassals
            .get(&self.nodes[id].player_guid.full())
            .cloned();

        self.nodes[id].vassals = Some(DotNetDict::new());

        if let Some(vassals) = vassals {
            for vassal in vassals {
                let monarch = self.nodes[id].monarch;
                let node = self.new_node(vassal.guid(), allegiance, Some(monarch), Some(id));
                self.build_chain(node, allegiance, patron_vassals);

                self.nodes[id]
                    .vassals
                    .as_mut()
                    .expect("just set")
                    .add(vassal.guid().full(), node);
            }
        }
        self.calculate_rank(id);
    }

    // ACE: AllegianceNode.CalculateRank
    pub fn calculate_rank(&mut self, id: NodeId) {
        // http://asheron.wikia.com/wiki/Rank

        // A player's allegiance rank is a function of the number of Vassals and how they are
        // organized. First, take the two highest ranked vassals. Now the Patron's rank will either be
        // one higher than the lower of the two, or equal to the highest rank vassal, whichever is greater.

        // sort vassals by rank (`OrderByDescending` is a stable sort; only the ranks are read)
        let mut sorted_vassals: Vec<u32> = self
            .vassals(id)
            .into_iter()
            .map(|v| self.nodes[v].rank)
            .collect();
        sorted_vassals.sort_by(|a, b| b.cmp(a));

        // get 2 highest rank vassals
        let r1 = sorted_vassals.first().copied().unwrap_or(0);
        let r2 = sorted_vassals.get(1).copied().unwrap_or(0);

        let lower = r1.min(r2);
        let higher = r1.max(r2);

        self.nodes[id].rank = 10u32.min(lower.wrapping_add(1).max(higher));
    }

    // ACE: AllegianceNode.Walk
    /// The nodes `Walk(action, self)` visits, in visiting order: this node (when `include_self`),
    /// then each vassal's walk, depth first. The caller runs its action over them; ACE's actions
    /// never change the tree, so collecting first is the same walk.
    #[must_use]
    pub fn walk(&self, id: NodeId, include_self: bool) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.walk_into(id, include_self, &mut out);
        out
    }

    fn walk_into(&self, id: NodeId, include_self: bool, out: &mut Vec<NodeId>) {
        if include_self {
            out.push(id);
        }

        for vassal in self.vassals(id) {
            self.walk_into(vassal, true, out);
        }
    }

    // ACE: AllegianceNode.ShowInfo
    /// The tree as indented names, one line each (ACE writes them to the console; DIVERGE log:
    /// returned for the caller to log).
    #[must_use]
    pub fn show_info(&self, w: &World, id: NodeId, depth: usize) -> Vec<String> {
        let prefix = " ".repeat(depth * 2);
        let name = player(w, self.nodes[id].player_guid)
            .and_then(|p| i_player::name(w, p))
            .unwrap_or_default();
        let mut lines = vec![format!("{prefix}- {name}")];
        for vassal in self.vassals(id) {
            lines.extend(self.show_info(w, vassal, depth + 1));
        }
        lines
    }
}

// ACE: AllegianceNode.Player
/// `PlayerManager.FindByGuid(PlayerGuid)`.
#[must_use]
pub fn player(w: &World, player_guid: ObjectGuid) -> Option<IPlayer> {
    player_manager::find_by_guid(w, player_guid.full()).0
}

// ACE: AllegianceNode.OnLevelUp
/// Called when the node's player levels up: its vassals who could not pass up XP start to when
/// the patron now meets their level. A node no longer in its allegiance does nothing.
pub fn on_level_up(w: &mut World, node: NodeRef) {
    let Some(tree) = crate::world_objects::allegiance::tree(w, node.allegiance) else {
        return;
    };
    let Some(id) = crate::world_objects::allegiance::member_node(w, node) else {
        return;
    };
    let vassal_guids: Vec<ObjectGuid> = tree
        .vassals(id)
        .into_iter()
        .map(|v| tree.node(v).player_guid)
        .collect();

    // patron = self node
    let patron = player(w, node.player)
        .expect("ACE: AllegianceNode.Player is null (NullReferenceException)");
    let patron_level = i_player::level(w, patron).unwrap_or(1);

    // find vassals who are not passing xp
    for vassal_guid in vassal_guids {
        let vassal = player(w, vassal_guid)
            .expect("ACE: AllegianceNode.Player is null (NullReferenceException)");
        if i_player::existed_before_allegiance_xp_changes(w, vassal) {
            continue;
        }

        let vassal_level = i_player::level(w, vassal).unwrap_or(1);

        // check if vassal now meets criteria for passing xp
        if patron_level >= vassal_level {
            crate::world_objects::player_allegiance::i_player_set_existed_before_allegiance_xp_changes(w, vassal, true);
        }
    }
}
