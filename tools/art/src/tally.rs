//! Counting where each model is used, and ranking the models by it.
//!
//! The walk over the world sees one landblock at a time, so it counts into a [`BlockTally`] and
//! folds that into a [`Tally`]: each model a block holds adds its instances and one landblock.
//! Every fold and merge is a sum, a minimum, a maximum or a bounded ordered set, so a tally comes
//! out the same whatever order the blocks were walked in, and blocks can be walked in parallel.

use std::collections::BTreeMap;

use dereth_primitives::{DataId, Vec3};

/// A model the world is built from: what a replacement would be keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelId {
    /// A graphics object (`0x01…`) or a setup (`0x02…`).
    Object(DataId),
    /// One cell structure of an environment (`0x0D…`), the piece interior cells are built from.
    Piece {
        environment: DataId,
        cell_struct: u16,
    },
}

/// What kind of record a [`ModelId`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    GfxObj,
    Setup,
    Piece,
    /// An object id of another record type.
    Other,
}

impl Kind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::GfxObj => "gfxobj",
            Self::Setup => "setup",
            Self::Piece => "environment",
            Self::Other => "other",
        }
    }
}

impl ModelId {
    #[must_use]
    pub fn kind(self) -> Kind {
        match self {
            Self::Object(id) => match id.0 >> 24 {
                0x01 => Kind::GfxObj,
                0x02 => Kind::Setup,
                _ => Kind::Other,
            },
            Self::Piece { .. } => Kind::Piece,
        }
    }

    /// `0x01000A2B`, or `0x0D00012C/3` for the fourth cell structure of an environment.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Object(id) => format!("0x{:08X}", id.0),
            Self::Piece {
                environment,
                cell_struct,
            } => format!("0x{:08X}/{cell_struct}", environment.0),
        }
    }

    /// [`Self::text`] as a file name: `0x0D00012C-3` for a piece.
    #[must_use]
    pub fn file_stem(self) -> String {
        self.text().replace('/', "-")
    }

    /// Reads [`Self::text`] back, with or without the `0x`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let hex = |t: &str| {
            let t = t.trim();
            let t = t
                .strip_prefix("0x")
                .or_else(|| t.strip_prefix("0X"))
                .unwrap_or(t);
            u32::from_str_radix(t, 16).ok()
        };
        match s.split_once('/') {
            Some((env, cs)) => Some(Self::Piece {
                environment: DataId(hex(env)?),
                cell_struct: cs.trim().parse().ok()?,
            }),
            None => Some(Self::Object(DataId(hex(s)?))),
        }
    }
}

/// One way the world uses a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Use {
    /// Placed by the client over the terrain, from the region's scene tables.
    Scenery,
    /// A landblock's static object.
    Static,
    /// A landblock's building shell.
    Building,
    /// A cell no building reaches: a dungeon's.
    DungeonPiece,
    /// A cell reached through a building's portals.
    InteriorPiece,
    /// A static object inside a dungeon cell.
    DungeonStatic,
    /// A static object inside a building's cell.
    InteriorStatic,
    /// An object the server spawns: a creature, an NPC, a door, a portal, a chest, an item.
    Spawn,
    /// A player's body, from the character-creation table.
    Character,
    /// A part a clothing table swaps onto a body.
    Clothing,
}

impl Use {
    /// The uses that are instances in the world, which a model's total counts. The character
    /// uses count creation choices and clothing tables, not objects in the world.
    pub const WORLD: [Self; 8] = [
        Self::Scenery,
        Self::Static,
        Self::Building,
        Self::DungeonPiece,
        Self::InteriorPiece,
        Self::DungeonStatic,
        Self::InteriorStatic,
        Self::Spawn,
    ];

    pub const ALL: [Self; 10] = [
        Self::Scenery,
        Self::Static,
        Self::Building,
        Self::DungeonPiece,
        Self::InteriorPiece,
        Self::DungeonStatic,
        Self::InteriorStatic,
        Self::Spawn,
        Self::Character,
        Self::Clothing,
    ];

    /// The column name in the tables.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Scenery => "scenery",
            Self::Static => "static",
            Self::Building => "building",
            Self::DungeonPiece => "dungeon",
            Self::InteriorPiece => "interior",
            Self::DungeonStatic => "dungeon-static",
            Self::InteriorStatic => "interior-static",
            Self::Spawn => "spawn",
            Self::Character => "character",
            Self::Clothing => "clothing",
        }
    }

    /// A heading for people.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Scenery => "Procedural scenery",
            Self::Static => "Landblock statics",
            Self::Building => "Buildings",
            Self::DungeonPiece => "Dungeon cells (environment pieces)",
            Self::InteriorPiece => "Building interior cells (environment pieces)",
            Self::DungeonStatic => "Statics inside dungeon cells",
            Self::InteriorStatic => "Statics inside building interiors",
            Self::Spawn => "Server spawns",
            Self::Character => "Character bodies and appearance parts (creation choices)",
            Self::Clothing => "Clothing part swaps (clothing tables)",
        }
    }

    /// Whether this use's landblocks are dungeons: one landblock's cells that no building reaches
    /// are counted as one dungeon.
    #[must_use]
    pub fn counts_dungeons(self) -> bool {
        matches!(self, Self::DungeonPiece | Self::DungeonStatic)
    }
}

/// How many example places a use keeps per model.
pub const EXAMPLES: usize = 5;

/// One place to look at a model: a landblock where it is common, and where in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Example {
    pub landblock: u16,
    /// How many instances that landblock holds.
    pub count: u64,
    /// Block-local, of the first instance seen there; `None` when the source gives none.
    pub position: Option<Vec3>,
    /// The cell, for an instance inside one.
    pub cell: Option<u32>,
}

/// Keeps `examples` to the [`EXAMPLES`] landblocks holding the most instances, ties to the
/// lower landblock id, one entry per landblock.
pub fn keep_examples(examples: &mut Vec<Example>) {
    examples.sort_by(|a, b| b.count.cmp(&a.count).then(a.landblock.cmp(&b.landblock)));
    let mut seen = std::collections::BTreeSet::new();
    examples.retain(|e| seen.insert(e.landblock));
    examples.truncate(EXAMPLES);
}

/// Everything counted about one model in one use.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UseStats {
    pub instances: u64,
    /// Landblocks holding at least one instance (for the dungeon uses, the dungeons).
    pub landblocks: u64,
    /// Scenery: instances by the terrain type and scene type of the vertex that grew them.
    pub terrain: BTreeMap<(u8, u8), u64>,
    /// Scenery: the smallest and largest scale drawn.
    pub scale: Option<(f32, f32)>,
    /// Buildings: the interior cells behind their portals, summed over every instance.
    pub interior_cells: u64,
    pub examples: Vec<Example>,
    /// Free labels with counts: the weenies and kinds of a spawn, the heritages of a body, the
    /// parts a clothing swap replaces.
    pub notes: BTreeMap<String, u64>,
}

impl UseStats {
    /// Adds `other`, as if its instances had been counted here.
    pub fn merge(&mut self, other: &Self) {
        self.instances += other.instances;
        self.landblocks += other.landblocks;
        for (k, n) in &other.terrain {
            *self.terrain.entry(*k).or_default() += n;
        }
        self.scale = merge_range(self.scale, other.scale);
        self.interior_cells += other.interior_cells;
        self.examples.extend(other.examples.iter().copied());
        keep_examples(&mut self.examples);
        for (k, n) in &other.notes {
            *self.notes.entry(k.clone()).or_default() += n;
        }
    }
}

fn merge_range(a: Option<(f32, f32)>, b: Option<(f32, f32)>) -> Option<(f32, f32)> {
    match (a, b) {
        (Some((a0, a1)), Some((b0, b1))) => Some((a0.min(b0), a1.max(b1))),
        (x, None) | (None, x) => x,
    }
}

/// One instance, as the walk sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sighting {
    /// Block-local.
    pub position: Option<Vec3>,
    pub cell: Option<u32>,
    pub scale: Option<f32>,
    /// The terrain type and scene type that placed a scenery object.
    pub terrain: Option<(u8, u8)>,
    /// A building's interior cells.
    pub interior_cells: u64,
}

/// One landblock's instances of each model in each use.
#[derive(Debug, Clone, Default)]
pub struct BlockTally {
    pub landblock: u16,
    entries: BTreeMap<(ModelId, Use), BlockEntry>,
}

#[derive(Debug, Clone, Default)]
struct BlockEntry {
    count: u64,
    first: Option<Sighting>,
    terrain: BTreeMap<(u8, u8), u64>,
    scale: Option<(f32, f32)>,
    interior_cells: u64,
}

impl BlockTally {
    #[must_use]
    pub fn new(landblock: u16) -> Self {
        Self {
            landblock,
            entries: BTreeMap::new(),
        }
    }

    /// Counts one instance of `id` in `use_`.
    pub fn see(&mut self, id: ModelId, use_: Use, s: Sighting) {
        let e = self.entries.entry((id, use_)).or_default();
        e.count += 1;
        if e.first.is_none() {
            e.first = Some(s);
        }
        if let Some(t) = s.terrain {
            *e.terrain.entry(t).or_default() += 1;
        }
        e.scale = merge_range(e.scale, s.scale.map(|x| (x, x)));
        e.interior_cells += s.interior_cells;
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// What the walk found about the world as a whole, beside the per-model counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldCounts {
    /// Landblocks the cell file holds.
    pub landblocks: u64,
    /// Landblocks the client grows scenery on.
    pub scenery_landblocks: u64,
    /// Landblocks with at least one static object.
    pub static_landblocks: u64,
    /// Landblocks with at least one building.
    pub building_landblocks: u64,
    /// Landblocks with at least one building interior cell.
    pub interior_landblocks: u64,
    pub buildings: u64,
    /// Landblocks with at least one cell no building reaches.
    pub dungeons: u64,
    pub dungeon_cells: u64,
    pub interior_cells: u64,
    /// Landblock statics whose origin falls outside their own block: drawn, but given no
    /// collision record.
    pub statics_outside_block: u64,
    /// Interior cells promised by a landblock that would not load.
    pub cells_unloaded: u64,
}

impl WorldCounts {
    pub fn merge(&mut self, o: &Self) {
        self.landblocks += o.landblocks;
        self.scenery_landblocks += o.scenery_landblocks;
        self.static_landblocks += o.static_landblocks;
        self.building_landblocks += o.building_landblocks;
        self.interior_landblocks += o.interior_landblocks;
        self.buildings += o.buildings;
        self.dungeons += o.dungeons;
        self.dungeon_cells += o.dungeon_cells;
        self.interior_cells += o.interior_cells;
        self.statics_outside_block += o.statics_outside_block;
        self.cells_unloaded += o.cells_unloaded;
    }
}

/// Where every model is used, and how often.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    pub uses: BTreeMap<ModelId, BTreeMap<Use, UseStats>>,
    /// For each environment piece, the lowest-numbered cell built from it and that cell's surface
    /// list, which is what the piece's textures and picture are read through.
    pub piece_surfaces: BTreeMap<ModelId, (u32, Vec<DataId>)>,
    pub world: WorldCounts,
}

impl Tally {
    /// Adds one landblock's counts: each model it holds gains its instances and one landblock.
    pub fn fold(&mut self, block: BlockTally) {
        for ((id, use_), e) in block.entries {
            let s = self.stats_mut(id, use_);
            s.instances += e.count;
            s.landblocks += 1;
            for (k, n) in e.terrain {
                *s.terrain.entry(k).or_default() += n;
            }
            s.scale = merge_range(s.scale, e.scale);
            s.interior_cells += e.interior_cells;
            let first = e.first.unwrap_or_default();
            s.examples.push(Example {
                landblock: block.landblock,
                count: e.count,
                position: first.position,
                cell: first.cell,
            });
            keep_examples(&mut s.examples);
        }
    }

    /// Records the surface list a piece's cell is drawn with, keeping the lowest cell id's.
    pub fn piece_cell(&mut self, id: ModelId, cell: u32, surfaces: &[DataId]) {
        match self.piece_surfaces.get(&id) {
            Some((c, _)) if *c <= cell => {}
            _ => {
                self.piece_surfaces.insert(id, (cell, surfaces.to_vec()));
            }
        }
    }

    /// The stats of `id` in `use_`, made empty when it has none yet.
    pub fn stats_mut(&mut self, id: ModelId, use_: Use) -> &mut UseStats {
        self.uses.entry(id).or_default().entry(use_).or_default()
    }

    /// Adds another tally's counts to this one.
    pub fn merge(&mut self, other: Self) {
        for (id, uses) in other.uses {
            for (use_, s) in uses {
                self.stats_mut(id, use_).merge(&s);
            }
        }
        for (id, (cell, surfaces)) in other.piece_surfaces {
            self.piece_cell(id, cell, &surfaces);
        }
        self.world.merge(&other.world);
    }

    /// Every model's instances in `use_`, ranked.
    #[must_use]
    pub fn ranked(&self, use_: Use) -> Vec<Ranked> {
        rank(
            self.uses
                .iter()
                .filter_map(|(id, u)| u.get(&use_).map(|s| (*id, s.instances))),
        )
    }

    /// Every model's instances over several uses together, ranked.
    #[must_use]
    pub fn ranked_over(&self, uses: &[Use]) -> Vec<Ranked> {
        rank(self.uses.iter().filter_map(|(id, u)| {
            let n: u64 = uses
                .iter()
                .filter_map(|x| u.get(x))
                .map(|s| s.instances)
                .sum();
            u.keys().any(|k| uses.contains(k)).then_some((*id, n))
        }))
    }

    /// Every model's instances in the world over all its uses, ranked: the model's whole
    /// coverage. Models used only by characters are left out.
    #[must_use]
    pub fn ranked_total(&self) -> Vec<Ranked> {
        self.ranked_over(&Use::WORLD)
    }
}

/// One model's place in a ranking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ranked {
    pub id: ModelId,
    pub count: u64,
    /// `count` over the ranking's total.
    pub share: f64,
    /// This model's and every higher-ranked model's shares together.
    pub cumulative: f64,
}

/// Ranks models by count, most first, ties to the lower id; models with no instances are kept,
/// last.
#[must_use]
pub fn rank(counts: impl IntoIterator<Item = (ModelId, u64)>) -> Vec<Ranked> {
    let mut v: Vec<(ModelId, u64)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: u64 = v.iter().map(|x| x.1).sum();
    let mut running = 0u64;
    v.into_iter()
        .map(|(id, count)| {
            running += count;
            Ranked {
                id,
                count,
                share: fraction(count, total),
                cumulative: fraction(running, total),
            }
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)] // counts far below 2^52
fn fraction(n: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        n as f64 / total as f64
    }
}

/// How many of the top-ranked models it takes to cover `percent` of the instances: the smallest
/// `N` whose running count reaches it. 0 for a ranking with no instances.
#[must_use]
pub fn ids_covering(ranked: &[Ranked], percent: u64) -> usize {
    let total: u64 = ranked.iter().map(|r| r.count).sum();
    if total == 0 {
        return 0;
    }
    let mut running = 0u64;
    for (i, r) in ranked.iter().enumerate() {
        running += r.count;
        if u128::from(running) * 100 >= u128::from(percent) * u128::from(total) {
            return i + 1;
        }
    }
    ranked.len()
}

/// The total instances of a ranking.
#[must_use]
pub fn total(ranked: &[Ranked]) -> u64 {
    ranked.iter().map(|r| r.count).sum()
}
