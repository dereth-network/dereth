//! What the world database says of gathering: what each mining craft skill's nodes play, answer and
//! unearth, the node a delving stone of the skill conjures, and the nodes each region's surveyor's
//! maps lead to.

use std::collections::BTreeMap;

/// What a mining craft skill's nodes do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mine {
    /// The craft skill (its saved state).
    pub skill: u32,
    /// The behavior a miner plays with none of the skill's tools in hand.
    pub behavior: u32,
    /// The skill's own tools: one in the miner's hand plays its own use behavior, adds its yield
    /// and wears.
    pub tools: Vec<u32>,
    /// A use's answers, plain and rich: (string, table).
    pub answer: (u32, u32),
    pub rich_answer: (u32, u32),
    /// The name of the effect a delving stone conjures the skill's node by, and the node.
    pub delving: Option<(String, u32)>,
    /// The lodestones a find unearths, by the finder's least level, rising from 0.
    pub lodestones: Vec<(i32, u32)>,
    /// How often a use of a node a map led to is a find, from 0 to 1.
    pub find_chance: f32,
    /// The other items a find may be, each by its share of finds; the rest are lodestones.
    pub finds: Vec<(u32, f32)>,
}

impl Mine {
    /// The lodestone a finder of a level unearths: the last whose least level it reaches.
    #[must_use]
    pub fn lodestone(&self, level: i32) -> Option<u32> {
        self.lodestones
            .iter()
            .rev()
            .find(|(from, _)| level >= *from)
            .or(self.lodestones.first())
            .map(|(_, t)| *t)
    }
}

/// The nodes a region's surveyor's maps lead to: each by the name of the effect that conjures it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapRegion {
    pub region: String,
    pub nodes: Vec<(String, u32)>,
}

/// The world's gathering: its mines by craft skill, its map regions by name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gathering {
    pub mines: BTreeMap<u32, Mine>,
    pub map_regions: BTreeMap<String, MapRegion>,
}

impl Gathering {
    /// The mine of a craft skill.
    #[must_use]
    pub fn mine(&self, skill: u32) -> Option<&Mine> {
        self.mines.get(&skill)
    }

    /// The mine whose tools hold a template.
    #[must_use]
    pub fn mine_of_tool(&self, template: u32) -> Option<&Mine> {
        self.mines.values().find(|m| m.tools.contains(&template))
    }

    /// The node a delving stone's effect conjures, by the effect's name.
    #[must_use]
    pub fn delving_node(&self, effect: &str) -> Option<u32> {
        self.mines
            .values()
            .filter_map(|m| m.delving.as_ref())
            .find(|(e, _)| e == effect)
            .map(|(_, n)| *n)
    }

    /// The node a map's effect conjures, by the effect's name.
    #[must_use]
    pub fn map_node(&self, effect: &str) -> Option<u32> {
        self.map_regions
            .values()
            .flat_map(|r| &r.nodes)
            .find(|(e, _)| e == effect)
            .map(|(_, n)| *n)
    }

    /// Every node a map conjures.
    pub fn map_nodes(&self) -> impl Iterator<Item = u32> + '_ {
        self.map_regions
            .values()
            .flat_map(|r| r.nodes.iter().map(|(_, n)| *n))
    }

    /// Every template gathering makes or names: the nodes delving stones and maps conjure, the
    /// tools, the lodestones and the other finds.
    pub fn templates(&self) -> impl Iterator<Item = u32> + '_ {
        let mines = self.mines.values().flat_map(|m| {
            m.delving
                .iter()
                .map(|(_, n)| *n)
                .chain(m.tools.iter().copied())
                .chain(m.lodestones.iter().map(|(_, t)| *t))
                .chain(m.finds.iter().map(|(t, _)| *t))
        });
        mines.chain(self.map_nodes())
    }
}
