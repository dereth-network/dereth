//! The AC2 look's tables: which Asheron's Call 2 thing stands for which Asheron's Call thing, and
//! the numbers each is fitted with. Ids and numbers only; every picture and every vertex comes from
//! the player's own files when the look is built.
//!
//! The tables are files of their own beside this crate (`ac2-look/*.toml`), one a part, built into
//! the program: a table and the code that reads it change together, and a build of the program
//! carries the tables it was made with. Each file names its `version`, which goes up whenever
//! anything else in it changes, so a look built from an older table can be told apart and built
//! again ([`Tables::versions`]).

use std::collections::BTreeSet;

use serde::Deserialize;

/// The ground table, as built into the program.
pub const TERRAIN: &str = include_str!("../../ac2-look/terrain.toml");
/// The trees and cover table.
pub const TREES: &str = include_str!("../../ac2-look/trees.toml");
/// The rocks table.
pub const ROCKS: &str = include_str!("../../ac2-look/rocks.toml");
/// The sky table.
pub const SKY: &str = include_str!("../../ac2-look/sky.toml");
/// The creatures table.
pub const CREATURES: &str = include_str!("../../ac2-look/creatures.toml");

/// One part of the look a player may take or leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    /// AC2's ground on the terrain types.
    Terrain,
    /// The desert's ground in AC2's own red, in place of the lighter sand.
    RedDesert,
    /// AC2's trees and plants, and its ground cover.
    Trees,
    /// AC2's rocks, fitted inside the collision.
    Rocks,
    /// AC2's clouds.
    Sky,
}

impl Part {
    /// Every part, in the order the look lists them.
    pub const ALL: [Self; 5] = [
        Self::Terrain,
        Self::RedDesert,
        Self::Trees,
        Self::Rocks,
        Self::Sky,
    ];

    /// The part's name on a command line and in a build's record.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Terrain => "terrain",
            Self::RedDesert => "red-desert",
            Self::Trees => "trees",
            Self::Rocks => "rocks",
            Self::Sky => "sky",
        }
    }

    /// The part's name as the player reads it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::RedDesert => "Red Desert",
            Self::Trees => "Trees and Ground Cover",
            Self::Rocks => "Rocks",
            Self::Sky => "Sky",
        }
    }

    /// The part named `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// One terrain type's ground.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    /// The Asheron's Call terrain type (0 to 31).
    #[serde(rename = "type")]
    pub ground_type: u8,
    /// The AC2 terrain kind laid on it, by its index in the region.
    pub kind: u32,
    /// The kind's group laid on flat ground, when not its first.
    pub flat: Option<usize>,
    /// The kind's group laid on steep ground, when not its last.
    pub steep: Option<usize>,
    /// A kind with one group lays its flat picture on its slopes too, rather than leaving them to
    /// the scree and rock laid on steep ground of a type with no steep picture.
    #[serde(default)]
    pub steep_flat: bool,
}

/// The red desert: grounds drawn under their own group's coloring color.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedDesert {
    /// The terrain types it changes.
    pub types: Vec<u8>,
    /// The color a group's coloring is taken relative to, red, green and blue.
    pub reference: [u8; 3],
}

/// The ground table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainTable {
    pub version: u32,
    /// The largest picture taken, texels a side.
    pub picture_limit: u32,
    /// How strongly a ground's relief bends its normal.
    pub relief_strength: f32,
    /// Whether each ground carries its own detail picture, the fine grain AC2's ground names
    /// beside its picture.
    #[serde(default)]
    pub detail: bool,
    /// Whether the presentation's own pebbles lie on the grounds the look lays.
    #[serde(default = "pebbles_lie")]
    pub pebbles: bool,
    pub ground: Vec<Ground>,
    pub red_desert: RedDesert,
}

/// The presentation's pebbles lie on a look's grounds unless its table says not.
const fn pebbles_lie() -> bool {
    true
}

/// How a model is fitted to the collision of the setup it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Only the bark is drawn in or out to the collision cylinder, below the crown.
    Girth,
    /// The whole model is narrowed to the cylinder, its height kept.
    Narrow,
    /// The model is fitted wholly inside the collision.
    Inside,
    /// A rock is fitted inside each sphere that collides.
    Spheres,
}

/// What a model counts as, for the player's switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Tree,
    Plant,
    Rock,
}

impl Category {
    /// The manifest's name for it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Plant => "plant",
            Self::Rock => "rock",
        }
    }
}

/// One setup's stand-in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// The Asheron's Call setup it is drawn for.
    pub setup: u32,
    /// The height it is drawn to, meters above its foot.
    pub height: f64,
    /// The AC2 entity drawn.
    pub entity: u32,
    pub category: Category,
    /// Its cut-out materials are growth.
    pub foliage: bool,
    /// The setup's collision cylinder's radius, measured; none for a setup with none.
    pub cylinder: Option<f64>,
    pub fit: Option<Fit>,
}

/// What every cover shares.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverAll {
    /// The least and most size each is drawn at.
    pub scale: [f32; 2],
    /// None grows where the ground rises more than this a meter across.
    pub max_slope: f32,
    /// How far from the eye it grows, meters.
    pub reach: f32,
    /// The terrain types whose own flowers AC2's take the place of.
    pub no_flowers: Vec<u8>,
}

/// One ground cover.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cover {
    /// The terrain types it grows on.
    pub types: Vec<u8>,
    /// The AC2 entity grown.
    pub entity: u32,
    /// How many grow on 100 square meters.
    pub density: f64,
    /// Its height, meters.
    pub height: f64,
}

/// The trees and cover table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreesTable {
    pub version: u32,
    /// Whether each tree carries the coarser levels Asheron's Call 2 draws it at, or only its
    /// most detailed.
    #[serde(default)]
    pub levels: bool,
    /// A material is drawn cut out when this share of its texels is clear...
    pub clear_share: f64,
    /// ...in its picture this many texels a side.
    pub clear_size: u32,
    pub model: Vec<Model>,
    pub cover_all: CoverAll,
    pub cover: Vec<Cover>,
}

/// The rocks table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RocksTable {
    pub version: u32,
    /// The rock a small sphere takes.
    pub small_rock: u32,
    /// How many of a setup's spheres collide.
    pub solid_spheres: usize,
    /// How far out of its collision a vertex may stand, meters.
    pub tolerance: f64,
    pub model: Vec<Model>,
}

/// The sky table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyTable {
    pub version: u32,
    /// AC2's clouds, a picture list.
    pub clouds: u32,
    /// The Asheron's Call cloud decks' pictures it is drawn on.
    pub decks: Vec<u32>,
}

/// The motion slots a creature's body may carry a clip for, in the order its row lists them, and
/// whether each loops (a loop closes on its first keys, anything else holds its last).
pub const SLOTS: [(&str, bool); 24] = [
    ("idle", true),
    ("idle_melee", true),
    ("idle_missile", true),
    ("idle_magic", true),
    ("walk", true),
    ("walk_back", true),
    ("run", true),
    ("side_step", true),
    ("turn", true),
    ("melee_high", false),
    ("melee_med", false),
    ("melee_low", false),
    ("special", false),
    ("missile", false),
    ("cast", false),
    ("telegraph", false),
    ("hit", false),
    ("hit_heavy", false),
    ("die", false),
    ("dead", false),
    ("emote", false),
    ("cheer", false),
    ("fall", true),
    ("jump", false),
];

/// One slot's clip: the AC2 keyframe animation played, and what the client paces it by.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub animation: u32,
    /// Another keyframe animation played with it at the same point of its cycle, as AC2 blends two
    /// cells of a move cube at a speed between them...
    pub mix: Option<u32>,
    /// ...weighed by this share (the first animation taking the rest), the cycle's length weighed
    /// the same way.
    pub share: Option<f64>,
    /// Seconds from its start to the blow.
    pub hit: Option<f64>,
    /// Meters of the body's own space one round of it covers.
    pub stride: Option<f64>,
}

/// One body a creature is drawn with: an AC2 entity posed in its bind clip's first keys, scaled
/// and placed onto an Asheron's Call setup's parts.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    /// The body's folder in the pack.
    pub folder: String,
    pub entity: u32,
    /// The keyframe animation whose first keys the body is bound in.
    pub bind: u32,
    /// How much larger it is drawn than AC2 draws it...
    pub scale: f64,
    /// ...and where it is moved to after (glTF's axes, meters).
    pub offset: [f64; 3],
    /// How many parts the setup it is drawn for has: the body is drawn in place of all of them.
    pub parts: usize,
    /// The first part's inverse bind matrix (column by column): where the body's bind space stands
    /// in that part's own space at rest.
    pub anchor: [f64; 16],
    /// The bones (by place among the body's) whose skin is blended without dual quaternions.
    #[serde(default)]
    pub trunk: Vec<usize>,
    /// The GfxObj each part must be drawing for the body to fit it; none for any.
    pub gfxobj: Option<Vec<u32>>,
    /// The pictures whose differences a masked look's tint is laid by, the body's own first.
    pub mask: Option<Vec<u32>>,
    pub clips: std::collections::BTreeMap<String, Clip>,
}

/// One body drawn under other pictures, for the looks whose palettes its rows name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub name: String,
    /// The body's folder.
    pub body: String,
    /// The picture list drawn in place of the body's own.
    pub picture: u32,
    /// The color it is multiplied by, red, green and blue...
    pub tint: [f64; 3],
    /// ...only where the body's pictures differ from each other.
    #[serde(default)]
    pub masked: bool,
    /// Every texel takes the color the tint makes of the picture's mean, as light or dark as it was.
    #[serde(default)]
    pub colorize: bool,
    /// The body's other pictures take the tint too.
    #[serde(default)]
    pub every: bool,
    /// A picture laid over it, repeated four times each way.
    pub detail: Option<u32>,
    /// A picture drawn as its normal map.
    pub normal: Option<u32>,
}

/// One `[[skin]]` row: the body (or one of its looks) drawn for a setup, and for the objects whose
/// look carries one of its palettes when it names any.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinRow {
    pub setup: u32,
    pub body: String,
    pub look: Option<String>,
    #[serde(default)]
    pub palettes: Vec<u32>,
}

/// One creature the player may take or leave.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Creature {
    /// Its name on a command line and in a build's record.
    pub name: String,
    /// Its name as the player reads it.
    pub label: String,
    /// Objects holding a launcher stay as Asheron's Call draws them.
    pub keep_launchers: bool,
    /// The weenies that stay as Asheron's Call draws them.
    pub keep_weenies: Vec<u32>,
    pub body: Vec<Body>,
    #[serde(default)]
    pub look: Vec<Look>,
    pub skin: Vec<SkinRow>,
}

/// The creatures table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreaturesTable {
    pub version: u32,
    pub creature: Vec<Creature>,
}

impl CreaturesTable {
    /// The creature named `name`.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Creature> {
        self.creature.iter().find(|c| c.name == name)
    }
}

/// Every table.
#[derive(Debug, Clone, PartialEq)]
pub struct Tables {
    pub terrain: TerrainTable,
    pub trees: TreesTable,
    pub rocks: RocksTable,
    pub sky: SkyTable,
    pub creatures: CreaturesTable,
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, text: &str) -> Result<T, String> {
    toml::from_str(text).map_err(|e| format!("the {name} table: {e}"))
}

impl Tables {
    /// The tables built into the program.
    ///
    /// # Errors
    /// A table does not read, or says something no build can do ([`Tables::check`]).
    pub fn built_in() -> Result<Self, String> {
        Self::parse(TERRAIN, TREES, ROCKS, SKY, CREATURES)
    }

    /// Tables from their files' text.
    ///
    /// # Errors
    /// A table does not read, or says something no build can do ([`Tables::check`]).
    pub fn parse(
        terrain: &str,
        trees: &str,
        rocks: &str,
        sky: &str,
        creatures: &str,
    ) -> Result<Self, String> {
        let t = Self {
            terrain: parse("ground", terrain)?,
            trees: parse("trees", trees)?,
            rocks: parse("rocks", rocks)?,
            sky: parse("sky", sky)?,
            creatures: parse("creatures", creatures)?,
        };
        t.check()?;
        Ok(t)
    }

    /// Whether the tables say only what a build can do: each terrain type has one ground, every
    /// setup one stand-in, every model's fit suits its category, every number is in range.
    ///
    /// # Errors
    /// What is wrong, the first thing found.
    pub fn check(&self) -> Result<(), String> {
        let mut types = BTreeSet::new();
        for g in &self.terrain.ground {
            if g.ground_type > 31 {
                return Err(format!("terrain type {} is not one", g.ground_type));
            }
            if !types.insert(g.ground_type) {
                return Err(format!("terrain type {} has two grounds", g.ground_type));
            }
        }
        for t in &self.terrain.red_desert.types {
            if !types.contains(t) {
                return Err(format!(
                    "the red desert names type {t}, which has no ground"
                ));
            }
        }
        if self.terrain.picture_limit == 0 || self.trees.clear_size == 0 {
            return Err("a picture limit of 0".into());
        }
        let mut setups = BTreeSet::new();
        for (m, rock) in self
            .trees
            .model
            .iter()
            .map(|m| (m, false))
            .chain(self.rocks.model.iter().map(|m| (m, true)))
        {
            if !setups.insert(m.setup) {
                return Err(format!("setup {:#010X} has two stand-ins", m.setup));
            }
            if m.setup >> 24 != 0x02 || m.entity >> 24 != 0x47 {
                return Err(format!(
                    "setup {:#010X}: a stand-in is an entity for a setup",
                    m.setup
                ));
            }
            if !(m.height > 0.0 && m.height.is_finite()) {
                return Err(format!("setup {:#010X}: a height of {}", m.setup, m.height));
            }
            let fits = match m.fit {
                None => true,
                Some(Fit::Girth | Fit::Narrow) => !rock && m.cylinder.is_some_and(|r| r > 0.0),
                Some(Fit::Inside | Fit::Spheres) => rock,
            };
            if (m.category == Category::Rock) != rock || !fits {
                return Err(format!(
                    "setup {:#010X}: a {} fitted {:?}",
                    m.setup,
                    m.category.name(),
                    m.fit
                ));
            }
        }
        for c in &self.trees.cover {
            if c.types.is_empty() || c.types.iter().any(|&t| t > 31) {
                return Err(format!("cover {:#010X}: types {:?}", c.entity, c.types));
            }
            if !(c.density > 0.0 && c.height > 0.0) {
                return Err(format!("cover {:#010X}: nothing to grow", c.entity));
            }
        }
        if self.rocks.small_rock >> 24 != 0x47 || self.sky.clouds >> 24 != 0x40 {
            return Err("the small rock or the clouds are not what they should be".into());
        }
        if self.sky.decks.iter().any(|d| d >> 24 != 0x05) {
            return Err("a cloud deck is not a picture".into());
        }
        self.check_creatures()
    }

    /// Whether every creature is named once, apart from the parts; each body draws an entity in
    /// a keyframe animation's first keys and plays known slots; each look and row names a body it
    /// has, and each row a look it has.
    fn check_creatures(&self) -> Result<(), String> {
        let mut names = BTreeSet::new();
        for c in &self.creatures.creature {
            if Part::from_name(&c.name).is_some() || !names.insert(c.name.as_str()) {
                return Err(format!("creature {} is named twice", c.name));
            }
            let mut folders = BTreeSet::new();
            for b in &c.body {
                if !folders.insert(b.folder.as_str()) {
                    return Err(format!("{}: two bodies in {}", c.name, b.folder));
                }
                if b.entity >> 24 != 0x47 || b.bind >> 24 != 0x05 {
                    return Err(format!(
                        "{} {}: not an entity in an animation",
                        c.name, b.folder
                    ));
                }
                if !(b.scale > 0.0 && b.scale.is_finite()) || b.parts == 0 {
                    return Err(format!("{} {}: a scale of {}", c.name, b.folder, b.scale));
                }
                if b.gfxobj.as_ref().is_some_and(|g| g.len() != b.parts) {
                    return Err(format!("{} {}: a GfxObj a part, or none", c.name, b.folder));
                }
                for (slot, clip) in &b.clips {
                    let mixed = match (clip.mix, clip.share) {
                        (None, None) => true,
                        (Some(m), Some(s)) => m >> 24 == 0x05 && (0.0..=1.0).contains(&s),
                        _ => false,
                    };
                    if !SLOTS.iter().any(|(s, _)| s == slot)
                        || clip.animation >> 24 != 0x05
                        || !mixed
                    {
                        return Err(format!("{} {}: slot {slot}", c.name, b.folder));
                    }
                }
            }
            let mut looks = BTreeSet::new();
            for l in &c.look {
                if !folders.contains(l.body.as_str()) || !looks.insert(l.name.as_str()) {
                    return Err(format!("{}: look {} on body {}", c.name, l.name, l.body));
                }
                let masked = c
                    .body
                    .iter()
                    .any(|b| b.folder == l.body && b.mask.is_some());
                if l.masked && !masked {
                    return Err(format!("{}: look {} masked by nothing", c.name, l.name));
                }
            }
            for r in &c.skin {
                let look_body = r.look.as_ref().map(|n| {
                    c.look
                        .iter()
                        .find(|l| &l.name == n)
                        .map(|l| l.body.as_str())
                });
                if !folders.contains(r.body.as_str())
                    || look_body.is_some_and(|b| b != Some(r.body.as_str()))
                {
                    return Err(format!("{}: a row for {:#010X}", c.name, r.setup));
                }
            }
        }
        Ok(())
    }

    /// The version of the table each part is built from, by the part's name.
    #[must_use]
    pub fn versions(&self) -> Vec<(Part, u32)> {
        Part::ALL
            .into_iter()
            .map(|p| (p, self.version(p)))
            .collect()
    }

    /// The version of the table `part` is built from.
    #[must_use]
    pub const fn version(&self, part: Part) -> u32 {
        match part {
            Part::Terrain | Part::RedDesert => self.terrain.version,
            Part::Trees => self.trees.version,
            Part::Rocks => self.rocks.version,
            Part::Sky => self.sky.version,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's tables)
    use super::*;

    #[test]
    fn the_built_in_tables_read_and_hold_together() {
        let t = Tables::built_in().expect("the tables");
        assert!(t.terrain.ground.len() >= 20);
        assert!(!t.trees.model.is_empty() && !t.trees.cover.is_empty());
        assert!(!t.rocks.model.is_empty() && !t.sky.decks.is_empty());
        assert!(t.versions().iter().all(|(_, v)| *v >= 1));
    }

    #[test]
    fn every_part_reads_back_from_its_name() {
        for p in Part::ALL {
            assert_eq!(Part::from_name(p.name()), Some(p));
        }
        assert_eq!(Part::from_name("dungeons"), None);
    }

    #[test]
    fn a_table_that_names_a_type_twice_or_fits_a_tree_inside_is_refused() {
        let twice = format!("{TERRAIN}\n[[ground]]\ntype = 1\nkind = 1\n");
        let e = Tables::parse(&twice, TREES, ROCKS, SKY, CREATURES).expect_err("two grounds");
        assert!(e.contains("type 1 has two grounds"), "{e}");
        let inside = TREES.replacen("fit = \"girth\"", "fit = \"inside\"", 1);
        let e = Tables::parse(TERRAIN, &inside, ROCKS, SKY, CREATURES).expect_err("a tree inside");
        assert!(e.contains("a tree fitted Some(Inside)"), "{e}");
        let unknown = format!("{SKY}\nsun = 1\n");
        assert!(Tables::parse(TERRAIN, TREES, ROCKS, &unknown, CREATURES).is_err());
    }

    #[test]
    fn every_creature_is_named_once_and_its_rows_name_bodies_and_looks_it_has() {
        let t = Tables::built_in().expect("the tables");
        assert_eq!(t.creatures.creature.len(), 18);
        for c in &t.creatures.creature {
            assert!(!c.body.is_empty() && !c.skin.is_empty(), "{}", c.name);
            assert!(
                c.body.iter().all(|b| b.clips.contains_key("idle")),
                "{}",
                c.name
            );
        }
        let olthoi = t.creatures.named("olthoi").expect("the olthoi");
        assert!(olthoi.body.len() > 1, "a body for each of its setups");
        let renamed = CREATURES.replacen("name = \"drudge\"", "name = \"rocks\"", 1);
        let e = Tables::parse(TERRAIN, TREES, ROCKS, SKY, &renamed).expect_err("a part's name");
        assert!(e.contains("rocks is named twice"), "{e}");
        let no_body =
            CREATURES.replacen("body = \"body\"\npicture", "body = \"body-x\"\npicture", 1);
        assert!(Tables::parse(TERRAIN, TREES, ROCKS, SKY, &no_body).is_err());
    }
}
