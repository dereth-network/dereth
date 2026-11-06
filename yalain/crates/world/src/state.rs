//! What a character holds and has done in play, kept between its sessions: its items (where each
//! sits in its pack, or where it is worn), its quests (status and phase), the skills it has
//! trained or raised beyond its start, its money, the effects on it (each kept with when it was
//! cast and how long it lasts, so it runs out on the game's clock while the character is away),
//! and what it knows of crafting (its craft skills and recipes).
//!
//! Kept by the store as one value in the server's own encoding ([`PlayState::to_bytes`]).

use yalain_store::StoredPosition;

/// An item a character holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub id: u64,
    pub template: u32,
    /// Its slot in the pack.
    pub slot: u32,
    /// Where it is worn or wielded (0 when it is not).
    pub location: u32,
    pub quantity: u32,
}

/// A quest a character has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestState {
    pub quest: u32,
    /// Its status (a value of the quest statuses: underway, a phase of it, completed).
    pub status: u32,
    pub phase: u32,
    /// When it was taken, moved on by the time its character has been out of the world since
    /// (its time runs only in the world), and when it was last updated (world seconds since 1970):
    /// under way, when its phase came, moved on the same way; else when it ended, which stays put.
    pub started: i64,
    pub updated: i64,
}

/// A skill a character holds beyond the ones it started with, or changed since.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillState {
    pub skill: u32,
    /// Its level as granted (0 for one trained, 1 a gift).
    pub granted: u32,
    /// Its information's mask (trained, the player's to untrain, ...).
    pub mask: u32,
    /// What it cost when trained, in credits.
    pub cost: u32,
    /// Its trained children and dependents.
    pub children: u32,
    pub dependents: u32,
    /// The experience spent on it.
    pub spent: u64,
}

/// An effect on a character: its record as its client holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectState {
    /// Its id among the character's effects.
    pub eid: u32,
    /// What it is (its saved state).
    pub state: u32,
    /// When it was cast (game time) and how long it lasts (`f32::MAX`: until taken off).
    pub cast: f64,
    pub timeout: f32,
    /// Who cast it, at what spellcraft, whether as a fight's effect.
    pub caster: u64,
    pub spellcraft: f32,
    pub pk: bool,
    /// For whom it was cast, the skill and the item it came from (0: none).
    pub acting: u64,
    pub skill: u32,
    pub item: u64,
    /// Its record's flags and categories.
    pub flags: u32,
    pub categories: u32,
}

/// A craft skill a character holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CraftSkillState {
    pub skill: u32,
    pub level: u32,
    /// Experience its use earned today, and earned toward its next level.
    pub today: u64,
    pub available: u64,
}

/// A recipe a character knows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecipeState {
    pub recipe: u32,
    /// When it may be used again (game time; -1 any time).
    pub use_again: f64,
    pub successes: u32,
    /// Its charges left (-1 for a recipe without charges).
    pub charges: i32,
    pub last_spinner: u32,
}

/// What an item a character holds carries beyond its template and place: its instance stamp
/// (one more each time the character enters), who made it, and how worn it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemState {
    pub id: u64,
    pub instance: u16,
    /// The name of the player who made it (an item made by crafting).
    pub crafter: Option<String>,
    /// Its durability now, once use has worn it below its template's.
    pub durability: Option<u32>,
    /// A surveyor's map's hidden place, once chosen.
    pub destination: Option<StoredPosition>,
    /// Its own effects on whoever wears or wields it (a crafted bonus): each saved state and the
    /// spellcraft it is cast at.
    pub bonuses: Vec<(u32, f32)>,
    /// It becomes bound when used in combat, and has not been yet.
    pub binds_on_use: bool,
    /// What a treasure or made item carries of its own.
    pub own: crate::treasure::Own,
    /// Its own permanent effects (a made item's): each saved state and the spellcraft it is at.
    pub enchantments: Vec<(u32, f32)>,
    /// The effects skills put on it for a while.
    pub timed: Vec<crate::things::TimedEffect>,
    /// The value it was given (a treasure item's draw, a made item's), 0 when its template's is
    /// its value.
    pub value: u32,
    /// What a player inscribed on it, and the player's name.
    pub inscription: Option<(String, String)>,
}

/// Everything a character holds and has done in play.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayState {
    pub items: Vec<Held>,
    pub quests: Vec<QuestState>,
    pub skills: Vec<SkillState>,
    pub money: u64,
    /// Its unspent skill credits.
    pub credits: u32,
    /// The effects on it, and the id its last effect was given.
    pub effects: Vec<EffectState>,
    pub effect_counter: u32,
    /// Its craft skills and recipes (none until it has played a session), and when its craft
    /// skills' daily experience next starts over (game time).
    pub craft_skills: Vec<CraftSkillState>,
    pub recipes: Vec<RecipeState>,
    pub craft_reset: f64,
    /// The scenes of the story it holds, in the order it gained them.
    pub scenes: Vec<u32>,
    /// When it last used each skill it has used (game time), and when it was last kept (game
    /// time): the time it is away does not count toward a skill's wait.
    pub last_used: Vec<(u32, f64)>,
    pub kept_at: f64,
    /// What its items carry of their own.
    pub item_states: Vec<ItemState>,
    /// Its unspent hero skill credits.
    pub hero_credits: u32,
    /// The seconds it has spent in the world, as of its last save.
    pub played_seconds: u64,
    /// It left the world cloaked: it enters so, if it is still an admin's.
    pub cloaked: bool,
    /// Words an admin put after its name.
    pub suffix: String,
    /// Its quest rating: what the quests it has completed added, shown when it is examined.
    pub quest_rating: u32,
}

/// The encoding's version (1 held a skill's granted level, a level and its experience only; 2
/// nothing after the skills; 3 nothing after the recipes; 4 nothing after the scenes; 5 nothing
/// after the skills' last uses; 6 an item's instance stamp and maker only; 7 nothing after an
/// item's durability; 8 nothing after a map's place; 9 nothing after an item's bonuses; 10 nothing
/// after an item's binding; 11 nothing after the hero credits; 12 nothing after the time played; 13 nothing after the cloak; 14 nothing after the suffix; 15 no value of an item's own; 16 no look an item drew, nor the hand it was last in; 17 no effect an item is named for; 18 not whom an item is bound to, nor that a recipe attuned it; 19 no effects a skill put on an item).
const VERSION: u32 = 20;

/// Do these bytes say a newer build kept them (a world that went back to an older build)?
#[must_use]
pub fn kept_by_a_newer_build(b: &[u8]) -> bool {
    b.first_chunk::<4>()
        .is_some_and(|w| u32::from_le_bytes(*w) > VERSION)
}

/// Which of a treasure item's own properties are kept: its least level, its Arcane Lore, its effect.
const OWN_MIN_LEVEL: u32 = 0x1;
const OWN_ARCANE_LORE: u32 = 0x2;
const OWN_EFFECT: u32 = 0x4;
/// What a recipe's item level gave it, and its permanent effects after that.
const OWN_MADE: u32 = 0x8;
/// The color a dye gave it.
const OWN_DYE: u32 = 0x10;
/// The value it was given.
const OWN_VALUE: u32 = 0x20;
/// What a player inscribed on it.
const OWN_INSCRIPTION: u32 = 0x40;
/// The craft flags a recipe gave it.
const OWN_CRAFT_FLAGS: u32 = 0x80;
/// What it drew of its look as it dropped: the table, and three keys and values.
const OWN_LOOK: u32 = 0x100;
/// The hand it was last held in.
const OWN_HAND: u32 = 0x200;
/// An item's own property: the effect it is named for.
const OWN_SUFFIX: u32 = 0x400;
/// Holds whom it is bound to.
const OWN_BOUND_TO: u32 = 0x800;
/// A recipe attuned it.
const OWN_ATTUNED: u32 = 0x1000;
/// Holds the effects skills put on it.
const OWN_TIMED: u32 = 0x2000;

/// An item's durability as kept: none (never worn) is a value no durability takes.
const UNWORN: u32 = u32::MAX;

/// A number a made item may lack, as kept: none is a value none of them takes.
const NO_NUMBER: u32 = u32::MAX;

fn write_made(out: &mut Vec<u8>, m: &crate::made::Made) {
    let w32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let number = |v: Option<i32>| v.map_or(NO_NUMBER, i32::cast_unsigned);
    w32(out, m.prefix);
    w32(out, m.value);
    w32(out, number(m.damage.map(|d| d.0)));
    w32(out, m.damage.map_or(0.0, |d| d.1).to_bits());
    w32(out, number(m.armor));
    w32(out, number(m.delay));
    w32(out, number(m.vigor));
    w32(out, m.slots);
    w32(out, u32::from(m.harmonic));
    w32(out, m.covers);
    w32(
        out,
        u32::from(m.damage_changed) | u32::from(m.vigor_changed) << 1,
    );
    w32(out, m.durability.unwrap_or(NO_NUMBER));
    w32(out, m.tool_yield.unwrap_or(NO_NUMBER));
    w32(out, m.craft_needed.unwrap_or(NO_NUMBER));
}

fn read_made(r: &mut In<'_>) -> Option<crate::made::Made> {
    let (prefix, value) = (r.u32()?, r.u32()?);
    let number = |v: u32| (v != NO_NUMBER).then_some(v.cast_signed());
    let most = number(r.u32()?);
    let variance = r.f32()?;
    let (armor, delay, vigor) = (number(r.u32()?), number(r.u32()?), number(r.u32()?));
    let (slots, harmonic, covers, changed) = (r.u32()?, r.u32()? != 0, r.u32()?, r.u32()?);
    let some = |v: u32| (v != NO_NUMBER).then_some(v);
    let (durability, tool_yield, craft_needed) = (some(r.u32()?), some(r.u32()?), some(r.u32()?));
    Some(crate::made::Made {
        prefix,
        value,
        damage: most.map(|d| (d, variance)),
        armor,
        delay,
        vigor,
        slots,
        harmonic,
        covers,
        damage_changed: changed & 1 != 0,
        vigor_changed: changed & 2 != 0,
        durability,
        tool_yield,
        craft_needed,
    })
}

struct In<'a>(&'a [u8]);

impl In<'_> {
    fn f64(&mut self) -> Option<f64> {
        self.u64().map(f64::from_bits)
    }
    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    fn u32(&mut self) -> Option<u32> {
        let (a, b) = self.0.split_first_chunk::<4>()?;
        self.0 = b;
        Some(u32::from_le_bytes(*a))
    }
    fn u64(&mut self) -> Option<u64> {
        let (a, b) = self.0.split_first_chunk::<8>()?;
        self.0 = b;
        Some(u64::from_le_bytes(*a))
    }
    fn i64(&mut self) -> Option<i64> {
        self.u64().map(u64::cast_signed)
    }
    fn text(&mut self) -> Option<String> {
        let n = usize::try_from(self.u32()?).ok()?;
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        String::from_utf8(a.to_vec()).ok()
    }
}

impl ItemState {
    /// What an item holds of its own, as it stands.
    #[must_use]
    pub fn of(t: &crate::things::Thing) -> Self {
        Self {
            id: t.id,
            instance: t.instance,
            crafter: t.crafter.clone(),
            durability: t.durability,
            destination: t.destination,
            bonuses: t.bonuses.clone(),
            binds_on_use: t.binds_on_use,
            own: t.own,
            enchantments: t.enchantments.clone(),
            timed: t.timed.clone(),
            value: t.value,
            inscription: t.inscription.clone(),
        }
    }

    /// An item takes what was kept of it again (all but its id and its instance stamp, which are
    /// the item's own to give).
    pub fn restore(&self, item: &mut crate::things::Thing) {
        item.crafter.clone_from(&self.crafter);
        item.durability = self.durability;
        item.destination = self.destination;
        item.bonuses.clone_from(&self.bonuses);
        item.binds_on_use = self.binds_on_use;
        item.own = self.own;
        item.enchantments.clone_from(&self.enchantments);
        item.timed.clone_from(&self.timed);
        item.value = self.value;
        item.inscription.clone_from(&self.inscription);
        // A made item saved before an item's own value was kept has it in what its level gave it.
        if let Some(m) = item.own.made {
            item.value = m.value;
        }
    }

    /// It alone, in the server's encoding, saying its version.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = VERSION.to_le_bytes().to_vec();
        self.write(&mut out);
        out
    }

    /// It alone, read back; none when it does not read.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let mut r = In(b);
        let version = r.u32()?;
        if !(6..=VERSION).contains(&version) {
            return None;
        }
        let read = Self::read(&mut r, version)?;
        r.0.is_empty().then_some(read)
    }

    fn write(&self, out: &mut Vec<u8>) {
        let w32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        let w64 = |out: &mut Vec<u8>, v: u64| out.extend_from_slice(&v.to_le_bytes());
        w64(out, self.id);
        w32(out, u32::from(self.instance));
        // A name's length, 0 for none (no maker is named by an empty name).
        let maker = self.crafter.as_deref().unwrap_or("").as_bytes();
        w32(out, u32::try_from(maker.len()).unwrap_or(0));
        out.extend_from_slice(maker);
        w32(out, self.durability.unwrap_or(UNWORN));
        // A place's cell, 0 for none (no place is in cell 0), then its point.
        let (cell, origin) = self
            .destination
            .map_or((0, [0.0; 3]), |d| (d.cell, d.origin));
        w32(out, cell);
        for v in origin {
            w32(out, v.to_bits());
        }
        w32(out, u32::try_from(self.bonuses.len()).unwrap_or(0));
        for (state, spellcraft) in &self.bonuses {
            w32(out, *state);
            w32(out, spellcraft.to_bits());
        }
        w32(out, u32::from(self.binds_on_use));
        let own = &self.own;
        let has = if own.min_level.is_some() {
            OWN_MIN_LEVEL
        } else {
            0
        } | if own.arcane_lore.is_some() {
            OWN_ARCANE_LORE
        } else {
            0
        } | if own.effect.is_some() { OWN_EFFECT } else { 0 }
            | if own.made.is_some() { OWN_MADE } else { 0 }
            | if own.dye.is_some() { OWN_DYE } else { 0 }
            | if self.value > 0 { OWN_VALUE } else { 0 }
            | if self.inscription.is_some() {
                OWN_INSCRIPTION
            } else {
                0
            }
            | if own.craft_flags != 0 {
                OWN_CRAFT_FLAGS
            } else {
                0
            }
            | if own.look.is_some() { OWN_LOOK } else { 0 }
            | if own.hand != 0 { OWN_HAND } else { 0 }
            | if own.suffix.is_some() { OWN_SUFFIX } else { 0 }
            | if own.bound_to != 0 { OWN_BOUND_TO } else { 0 }
            | if own.attuned { OWN_ATTUNED } else { 0 }
            | if self.timed.is_empty() { 0 } else { OWN_TIMED };
        w32(out, has);
        if let Some(v) = own.min_level {
            w32(out, v.cast_unsigned());
        }
        if let Some(v) = own.arcane_lore {
            w32(out, v.cast_unsigned());
        }
        if let Some(e) = own.effect {
            w32(out, e.state);
            w32(out, e.spellcraft.to_bits());
            w32(out, e.table);
            w32(out, e.color.to_bits());
        }
        if let Some(m) = own.made {
            write_made(out, &m);
            w32(out, u32::try_from(self.enchantments.len()).unwrap_or(0));
            for (state, spellcraft) in &self.enchantments {
                w32(out, *state);
                w32(out, spellcraft.to_bits());
            }
        }
        if let Some((table, value)) = own.dye {
            w32(out, table);
            w32(out, value.to_bits());
        }
        if self.value > 0 {
            w32(out, self.value);
        }
        if let Some((said, by)) = &self.inscription {
            for text in [said, by] {
                w32(out, u32::try_from(text.len()).unwrap_or(0));
                out.extend_from_slice(text.as_bytes());
            }
        }
        if own.craft_flags != 0 {
            w32(out, own.craft_flags);
        }
        if let Some(look) = own.look {
            w32(out, look.table);
            for (key, value) in look.values {
                w32(out, key);
                w32(out, value.to_bits());
            }
        }
        if own.hand != 0 {
            w32(out, own.hand);
        }
        if let Some(table) = own.suffix {
            w32(out, table);
        }
        if own.bound_to != 0 {
            out.extend_from_slice(&own.bound_to.to_le_bytes());
        }
        if !self.timed.is_empty() {
            w32(out, u32::try_from(self.timed.len()).unwrap_or(0));
            for e in &self.timed {
                w32(out, e.state);
                w32(out, e.spellcraft.to_bits());
                out.extend_from_slice(&e.until.to_bits().to_le_bytes());
            }
        }
    }

    fn read(r: &mut In<'_>, version: u32) -> Option<Self> {
        let id = r.u64()?;
        let instance = u16::try_from(r.u32()?).ok()?;
        let crafter = r.text()?;
        let durability = if version >= 7 {
            Some(r.u32()?).filter(|d| *d != UNWORN)
        } else {
            None
        };
        let destination = if version >= 8 {
            let cell = r.u32()?;
            let mut origin = [0.0; 3];
            for v in &mut origin {
                *v = f32::from_bits(r.u32()?);
            }
            (cell != 0).then_some(StoredPosition {
                cell,
                origin,
                rotation: [1.0, 0.0, 0.0, 0.0],
            })
        } else {
            None
        };
        let mut bonuses = Vec::new();
        if version >= 9 {
            for _ in 0..r.u32()? {
                bonuses.push((r.u32()?, r.f32()?));
            }
        }
        let binds_on_use = version >= 10 && r.u32()? != 0;
        let mut own = crate::treasure::Own::default();
        let mut enchantments = Vec::new();
        let mut timed = Vec::new();
        let mut value = 0;
        let mut inscription = None;
        if version >= 11 {
            let has = r.u32()?;
            if has & OWN_MIN_LEVEL != 0 {
                own.min_level = Some(r.u32()?.cast_signed());
            }
            if has & OWN_ARCANE_LORE != 0 {
                own.arcane_lore = Some(r.u32()?.cast_signed());
            }
            if has & OWN_EFFECT != 0 {
                own.effect = Some(crate::treasure::OwnEffect {
                    state: r.u32()?,
                    spellcraft: r.f32()?,
                    table: r.u32()?,
                    color: r.f32()?,
                })
            }
            if has & OWN_MADE != 0 {
                own.made = Some(read_made(r)?);
                for _ in 0..r.u32()? {
                    enchantments.push((r.u32()?, r.f32()?));
                }
            }
            if has & OWN_DYE != 0 {
                own.dye = Some((r.u32()?, r.f32()?));
            }
            if has & OWN_VALUE != 0 {
                value = r.u32()?;
            }
            if has & OWN_INSCRIPTION != 0 {
                inscription = Some((r.text()?, r.text()?));
            }
            if has & OWN_CRAFT_FLAGS != 0 {
                own.craft_flags = r.u32()?;
            }
            if has & OWN_LOOK != 0 {
                let table = r.u32()?;
                let mut values = [(0, 0.0); 3];
                for slot in &mut values {
                    *slot = (r.u32()?, r.f32()?);
                }
                own.look = Some(crate::treasure::Look { table, values });
            }
            if has & OWN_HAND != 0 {
                own.hand = r.u32()?;
            }
            if has & OWN_SUFFIX != 0 {
                own.suffix = Some(r.u32()?);
            }
            if has & OWN_BOUND_TO != 0 {
                own.bound_to = r.u64()?;
            }
            own.attuned = has & OWN_ATTUNED != 0;
            if has & OWN_TIMED != 0 {
                for _ in 0..r.u32()? {
                    timed.push(crate::things::TimedEffect {
                        state: r.u32()?,
                        spellcraft: r.f32()?,
                        until: r.f64()?,
                    });
                }
            }
        }
        Some(Self {
            id,
            instance,
            crafter: (!crafter.is_empty()).then_some(crafter),
            durability,
            destination,
            bonuses,
            binds_on_use,
            own,
            enchantments,
            timed,
            value,
            inscription,
        })
    }
}

impl PlayState {
    /// The state in the server's encoding.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let w32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        let w64 = |out: &mut Vec<u8>, v: u64| out.extend_from_slice(&v.to_le_bytes());
        w32(&mut out, VERSION);
        w64(&mut out, self.money);
        w32(&mut out, self.credits);
        w32(&mut out, u32::try_from(self.items.len()).unwrap_or(0));
        for h in &self.items {
            w64(&mut out, h.id);
            w32(&mut out, h.template);
            w32(&mut out, h.slot);
            w32(&mut out, h.location);
            w32(&mut out, h.quantity);
        }
        w32(&mut out, u32::try_from(self.quests.len()).unwrap_or(0));
        for q in &self.quests {
            w32(&mut out, q.quest);
            w32(&mut out, q.status);
            w32(&mut out, q.phase);
            w64(&mut out, q.started.cast_unsigned());
            w64(&mut out, q.updated.cast_unsigned());
        }
        w32(&mut out, u32::try_from(self.skills.len()).unwrap_or(0));
        for s in &self.skills {
            w32(&mut out, s.skill);
            w32(&mut out, s.granted);
            w32(&mut out, s.mask);
            w32(&mut out, s.cost);
            w32(&mut out, s.children);
            w32(&mut out, s.dependents);
            w64(&mut out, s.spent);
        }
        w32(&mut out, self.effect_counter);
        w32(&mut out, u32::try_from(self.effects.len()).unwrap_or(0));
        for e in &self.effects {
            w32(&mut out, e.eid);
            w32(&mut out, e.state);
            w64(&mut out, e.cast.to_bits());
            w32(&mut out, e.timeout.to_bits());
            w64(&mut out, e.caster);
            w32(&mut out, e.spellcraft.to_bits());
            w32(&mut out, u32::from(e.pk));
            w64(&mut out, e.acting);
            w32(&mut out, e.skill);
            w64(&mut out, e.item);
            w32(&mut out, e.flags);
            w32(&mut out, e.categories);
        }
        w64(&mut out, self.craft_reset.to_bits());
        w32(
            &mut out,
            u32::try_from(self.craft_skills.len()).unwrap_or(0),
        );
        for c in &self.craft_skills {
            w32(&mut out, c.skill);
            w32(&mut out, c.level);
            w64(&mut out, c.today);
            w64(&mut out, c.available);
        }
        w32(&mut out, u32::try_from(self.recipes.len()).unwrap_or(0));
        for r in &self.recipes {
            w32(&mut out, r.recipe);
            w64(&mut out, r.use_again.to_bits());
            w32(&mut out, r.successes);
            w32(&mut out, r.charges.cast_unsigned());
            w32(&mut out, r.last_spinner);
        }
        w32(&mut out, u32::try_from(self.scenes.len()).unwrap_or(0));
        for sc in &self.scenes {
            w32(&mut out, *sc);
        }
        w64(&mut out, self.kept_at.to_bits());
        w32(&mut out, u32::try_from(self.last_used.len()).unwrap_or(0));
        for (skill, t) in &self.last_used {
            w32(&mut out, *skill);
            w64(&mut out, t.to_bits());
        }
        w32(&mut out, u32::try_from(self.item_states.len()).unwrap_or(0));
        for i in &self.item_states {
            i.write(&mut out);
        }
        w32(&mut out, self.hero_credits);
        w64(&mut out, self.played_seconds);
        w32(&mut out, u32::from(self.cloaked));
        w32(&mut out, u32::try_from(self.suffix.len()).unwrap_or(0));
        out.extend_from_slice(self.suffix.as_bytes());
        w32(&mut out, self.quest_rating);
        out
    }

    /// Read the state back; an empty value (a character that has not played) is the empty state.
    ///
    /// # Errors
    /// Bytes that are not a state of this encoding.
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.is_empty() {
            return Ok(Self::default());
        }
        let bad = || "a play state that does not read".to_string();
        let mut r = In(b);
        let version = r.u32().ok_or_else(bad)?;
        if !(1..=VERSION).contains(&version) {
            return Err("a play state of another version".into());
        }
        let mut s = Self {
            money: r.u64().ok_or_else(bad)?,
            credits: r.u32().ok_or_else(bad)?,
            ..Self::default()
        };
        for _ in 0..r.u32().ok_or_else(bad)? {
            s.items.push(Held {
                id: r.u64().ok_or_else(bad)?,
                template: r.u32().ok_or_else(bad)?,
                slot: r.u32().ok_or_else(bad)?,
                location: r.u32().ok_or_else(bad)?,
                quantity: r.u32().ok_or_else(bad)?,
            });
        }
        for _ in 0..r.u32().ok_or_else(bad)? {
            s.quests.push(QuestState {
                quest: r.u32().ok_or_else(bad)?,
                status: r.u32().ok_or_else(bad)?,
                phase: r.u32().ok_or_else(bad)?,
                started: r.i64().ok_or_else(bad)?,
                updated: r.i64().ok_or_else(bad)?,
            });
        }
        for _ in 0..r.u32().ok_or_else(bad)? {
            let skill = r.u32().ok_or_else(bad)?;
            let granted = r.u32().ok_or_else(bad)?;
            let (mask, cost, children, dependents) = if version == 1 {
                let _level = r.u32().ok_or_else(bad)?;
                (1, 0, 0, 0)
            } else {
                (
                    r.u32().ok_or_else(bad)?,
                    r.u32().ok_or_else(bad)?,
                    r.u32().ok_or_else(bad)?,
                    r.u32().ok_or_else(bad)?,
                )
            };
            s.skills.push(SkillState {
                skill,
                granted,
                mask,
                cost,
                children,
                dependents,
                spent: r.u64().ok_or_else(bad)?,
            });
        }
        if version >= 3 {
            s.effect_counter = r.u32().ok_or_else(bad)?;
            for _ in 0..r.u32().ok_or_else(bad)? {
                s.effects.push(EffectState {
                    eid: r.u32().ok_or_else(bad)?,
                    state: r.u32().ok_or_else(bad)?,
                    cast: r.f64().ok_or_else(bad)?,
                    timeout: r.f32().ok_or_else(bad)?,
                    caster: r.u64().ok_or_else(bad)?,
                    spellcraft: r.f32().ok_or_else(bad)?,
                    pk: r.u32().ok_or_else(bad)? != 0,
                    acting: r.u64().ok_or_else(bad)?,
                    skill: r.u32().ok_or_else(bad)?,
                    item: r.u64().ok_or_else(bad)?,
                    flags: r.u32().ok_or_else(bad)?,
                    categories: r.u32().ok_or_else(bad)?,
                });
            }
            s.craft_reset = r.f64().ok_or_else(bad)?;
            for _ in 0..r.u32().ok_or_else(bad)? {
                s.craft_skills.push(CraftSkillState {
                    skill: r.u32().ok_or_else(bad)?,
                    level: r.u32().ok_or_else(bad)?,
                    today: r.u64().ok_or_else(bad)?,
                    available: r.u64().ok_or_else(bad)?,
                });
            }
            for _ in 0..r.u32().ok_or_else(bad)? {
                s.recipes.push(RecipeState {
                    recipe: r.u32().ok_or_else(bad)?,
                    use_again: r.f64().ok_or_else(bad)?,
                    successes: r.u32().ok_or_else(bad)?,
                    charges: r.u32().ok_or_else(bad)?.cast_signed(),
                    last_spinner: r.u32().ok_or_else(bad)?,
                });
            }
        }
        if version >= 4 {
            for _ in 0..r.u32().ok_or_else(bad)? {
                s.scenes.push(r.u32().ok_or_else(bad)?);
            }
        }
        if version >= 5 {
            s.kept_at = r.f64().ok_or_else(bad)?;
            for _ in 0..r.u32().ok_or_else(bad)? {
                s.last_used
                    .push((r.u32().ok_or_else(bad)?, r.f64().ok_or_else(bad)?));
            }
        }
        if version >= 6 {
            for _ in 0..r.u32().ok_or_else(bad)? {
                s.item_states
                    .push(ItemState::read(&mut r, version).ok_or_else(bad)?);
            }
        }
        if version >= 11 {
            s.hero_credits = r.u32().ok_or_else(bad)?;
        }
        if version >= 12 {
            s.played_seconds = r.u64().ok_or_else(bad)?;
        }
        if version >= 13 {
            s.cloaked = r.u32().ok_or_else(bad)? != 0;
        }
        if version >= 14 {
            let n = usize::try_from(r.u32().ok_or_else(bad)?).map_err(|_| bad())?;
            let bytes = r.0.get(..n).ok_or_else(bad)?;
            s.suffix = String::from_utf8_lossy(bytes).into_owned();
            r.0 = &r.0[n..];
        }
        if version >= 15 {
            s.quest_rating = r.u32().ok_or_else(bad)?;
        }
        if !r.0.is_empty() {
            return Err(bad());
        }
        Ok(s)
    }
}
