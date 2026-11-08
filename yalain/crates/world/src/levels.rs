//! The character levels: the experience each level starts at, and the health, vigor, skill
//! credits and hero skill credits it brings, read from the client's level table (a saved state the client also reads to
//! show a character's progress).

use auberean_script::graph::Graph;
use auberean_script::library::Library;

use crate::start::Nav;

/// The level table's saved state.
pub const LEVEL_TABLE: u32 = 0x7000_0380;

/// The highest level a character that is not a hero reaches. Its experience goes on adding up, and
/// counts once it is a hero.
pub const NOT_A_HEROS_TOP: i32 = 50;

/// One level's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Level {
    /// The total experience the level starts at.
    pub xp: u64,
    pub health: i32,
    pub vigor: i32,
    /// The skill credits reaching the level grants.
    pub skill_credits: i32,
    /// The hero skill credits reaching the level grants (100 a level from level 45).
    pub hero_credits: i32,
}

/// Every level, from level 1.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LevelTable {
    /// `levels[0]` is level 1.
    pub levels: Vec<Level>,
}

impl LevelTable {
    /// Read the level table's graph: its map of levels (index = level), each a level's data.
    ///
    /// # Errors
    /// A field the table should hold is missing or of another shape.
    pub fn from_state(lib: &Library, g: &Graph) -> Result<Self, String> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root().ok_or("an empty graph")?)?;
        let mut levels = Vec::new();
        for (i, item) in nav.items(nav.field(root, "mMap")?)?.into_iter().enumerate() {
            let Some(r) = item.reference() else {
                if i == 0 {
                    continue; // no level 0
                }
                return Err(format!("level {i} is missing"));
            };
            let o = nav.object(r)?;
            let lo = u64::from(nav.field(o, "mXP")?);
            // A 64-bit field spans two slots: the high word follows the low one.
            let xp = match high_slot(o, lib, "mXP") {
                Some(hi) => lo | u64::from(hi) << 32,
                None => lo,
            };
            levels.push(Level {
                xp,
                health: nav.field(o, "mHealth")?.cast_signed(),
                vigor: nav.field(o, "mVigor")?.cast_signed(),
                skill_credits: nav.field(o, "mSkillCredits")?.cast_signed(),
                hero_credits: nav
                    .field(o, "mHeroSkillCredits")
                    .map_or(0, u32::cast_signed),
            });
        }
        if levels.is_empty() {
            return Err("a level table without levels".into());
        }
        Ok(Self { levels })
    }

    /// The level a total of experience reaches (at least 1).
    #[must_use]
    pub fn level_for(&self, xp: u64) -> i32 {
        let n = self.levels.iter().take_while(|l| l.xp <= xp).count().max(1);
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    /// The level this much experience takes a character to: a hero as far as the table goes, anyone
    /// else no further than [`NOT_A_HEROS_TOP`]; never below the level it holds.
    #[must_use]
    pub fn reached(&self, xp: u64, hero: bool, holds: i32) -> i32 {
        if self.levels.is_empty() {
            return holds;
        }
        let by_xp = self.level_for(xp);
        (if hero {
            by_xp
        } else {
            by_xp.min(NOT_A_HEROS_TOP)
        })
        .max(holds)
    }

    /// A level's numbers.
    #[must_use]
    pub fn level(&self, level: i32) -> Option<&Level> {
        usize::try_from(level - 1)
            .ok()
            .and_then(|i| self.levels.get(i))
    }

    /// What a level takes to reach the next; nothing at the highest.
    #[must_use]
    pub fn to_next(&self, level: i32) -> Option<u64> {
        Some(
            self.level(level + 1)?
                .xp
                .saturating_sub(self.level(level)?.xp),
        )
    }

    /// The highest level.
    #[must_use]
    pub fn max_level(&self) -> i32 {
        i32::try_from(self.levels.len()).unwrap_or(i32::MAX)
    }
}

/// The high word of a 64-bit field of a script object (its next slot, tagged as a high word), or
/// none for a 32-bit field.
fn high_slot(o: &auberean_script::graph::PackedObject, lib: &Library, name: &str) -> Option<u32> {
    let pkg = lib.package_name(o.package_id());
    let slot = lib
        .frame(&pkg)
        .and_then(|f| f.members.iter().find(|m| m.name == name))
        .and_then(|m| usize::try_from(m.slot).ok())?;
    (o.tags.get(slot + 1) == Some(&crate::graph::tag::HIGH))
        .then(|| o.slots.get(slot + 1).copied())
        .flatten()
}
