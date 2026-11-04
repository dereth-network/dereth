//! What effects are, read from their saved states with the script library's names for their
//! fields: how much each changes and for how long at a spellcraft, what it changes, the class it
//! competes in, the effect it shows.
//!
//! **Scaled values.** An effect's duration, change and magnitude are tables of points (a
//! spellcraft and a value); the value at a spellcraft is the straight line between the points on
//! either side of it, held at the first point's value below it and the last's above it.
//!
//! **Kinds**, by the package an effect derives from: an instant change to vitals (health, vigor or
//! focus, by its flags); a change to vitals every second it runs; a change to qualities (added to
//! them, or multiplying them, by its flags); a chain of such changes; a quest moved on; a recipe
//! learned or forgotten; a combination that adds one effect or another as the target has an
//! effect of some type or not; an aura, held by its user, that puts its pulsed effects on the
//! players around it every so often; a pet summoned, of a class, for a lifetime or for good; a
//! teleport, which sends its target to its place when its time is up; a recall, which is its
//! skill's ([`crate::skill_use`]): to a place its user keeps (a link), to one of a list of named
//! places, or to its allegiance's hall.
//!
//! **A combination** looks for the target's effects of its types (or classes): found, it adds its
//! effect for that case at the cast's spellcraft adjusted by its own amount, gives its user the
//! effect it gives back, and takes off the oldest of the effects found or all of them, as its
//! flags say; not found, the other effect and adjustment.

use std::collections::HashMap;

use auberean_script::graph::{Graph, PackedObject};
use auberean_script::library::Library;
use auberean_script::native_object::{Item, NativeObject};

use crate::graph::NULL;
use crate::start::Nav;

/// A table of points: a spellcraft and the value there.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scale(pub Vec<(f32, f32)>);

impl Scale {
    /// The value at a spellcraft: the line between the points either side, held at the ends; none
    /// for no points.
    #[must_use]
    pub fn at(&self, x: f32) -> Option<f32> {
        let mut pts = self.0.clone();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (first, last) = (*pts.first()?, *pts.last()?);
        if x <= first.0 {
            return Some(first.1);
        }
        if x >= last.0 {
            return Some(last.1);
        }
        pts.windows(2).find_map(|w| {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            (x >= x0 && x <= x1).then(|| {
                if (x1 - x0).abs() < f32::EPSILON {
                    y1
                } else {
                    y0 + (y1 - y0) * ((x - x0) / (x1 - x0))
                }
            })
        })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Which vitals an effect changes, and how (its flags).
pub mod vital {
    pub const HEALTH: u32 = 0x1;
    pub const VIGOR: u32 = 0x2;
    pub const FOCUS: u32 = 0x4;
    /// The change is a share of the vital's greatest value.
    pub const OF_MAX: u32 = 0x40;
}

/// How a change to qualities applies (its flags).
pub mod numeric {
    pub const ADD: u32 = 0x4;
    pub const MULTIPLY: u32 = 0x8;
}

/// An effect's internal flags: taken off at a resurrection.
pub const REMOVED_ON_DEATH: u32 = 0x40_0000;

/// An effect's internal flags: one its holder's client is shown (an item's effect without it is
/// never put on its wearer).
pub const CLIENT_VISIBLE: u32 = 0x1;

/// An effect's internal flags that file its records under a category besides the ordinary one,
/// each with its category: it beats (changes over time), it pulses, it watches vitals change, it
/// watches effects put on, it watches effects change vitals.
pub const CATEGORY_FLAGS: [(u32, u32); 5] = [
    (0x2000_0000, 0x4),
    (0x2_0000, 0x20),
    (0x1000_0000, 0x2),
    (0x800_0000, 0x8),
    (0x1000, 0x10),
];

/// A combination's flags (its effect's external flags): what it takes off of the effects it found.
pub mod combo_flag {
    /// The oldest of them.
    pub const REMOVE_OLDEST: u32 = 0x1;
    /// All of them.
    pub const REMOVE_ALL: u32 = 0x2;
}

/// What a combination looks for and what it does either way.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Combo {
    /// The types and classes of effect it looks for on its target.
    pub types: Vec<u32>,
    pub classes: Vec<u32>,
    /// The effect it adds when one is found, and when none is.
    pub if_present: Option<u32>,
    pub if_not_present: Option<u32>,
    /// The effect it gives its user when one is found, and when none is.
    pub give_back_if_present: Option<u32>,
    pub give_back_if_not_present: Option<u32>,
    /// What it adds to the spellcraft of what it adds, either way.
    pub adjust_if_present: i32,
    pub adjust_if_not_present: i32,
}

/// Whom an aura's pulses reach (its flags).
pub mod aura_target {
    pub const PLAYER: u32 = 0x1;
    pub const WEAPON: u32 = 0x2;
    pub const SHIELD: u32 = 0x4;
    pub const ITEM: u32 = 0x8;
    pub const PET: u32 = 0x10;
    pub const OTHER_PLAYER: u32 = 0x20;
    pub const MASTER: u32 = 0x40;
}

/// An aura: the effects its pulses put on, whom they reach and how far.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Aura {
    pub pulsed: Vec<u32>,
    pub targets: u32,
    /// The farthest a pulse reaches, in meters.
    pub range: f32,
}

/// What an effect does.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Changes vitals once: which (flags) and by how much.
    InstantVital { flags: u32, change: Scale },
    /// Changes vitals by so much a second while it runs.
    VitalOverTime { flags: u32, per_second: Scale },
    /// Changes qualities (each `type << 16 | quality`: 3 a float, 4 an integer) by its
    /// magnitude: added or multiplying, by its flags.
    Numeric {
        flags: u32,
        stats: Vec<u32>,
        magnitude: Scale,
    },
    /// Several changes to qualities at once.
    Chained(Vec<EffectDef>),
    /// A weapon's: its hits harm creatures of these superclasses by its spellcraft times the number
    /// with each.
    Slayer(Vec<(u32, f32)>),
    /// Moves a quest on: the quest and the update.
    Quest { quest: u32, update: u32 },
    /// The player learns a recipe.
    GrantRecipe(u32),
    /// The player forgets a recipe.
    RemoveRecipe(u32),
    /// Adds one effect if the target has an effect of one of these types, another if not.
    Combo(Box<Combo>),
    /// Held by its user while it is on, putting its pulsed effects on the players around it.
    Aura(Aura),
    /// Summons a pet of a class, which leaves the world after its lifetime at a spellcraft (none:
    /// it stays).
    Pet { class: u32, lifetime: Scale },
    /// Sends its target to a place when its time is up, and is no record: the place.
    Teleport(yalain_store::StoredPosition),
    /// A recall: where its skill takes its user.
    Recall(Recall),
    /// Anything else (kept as a record only).
    Other,
}

/// Where a recall takes its user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recall {
    /// To the place of a kind the user keeps (its lifestone's, its last portal's).
    Link(u32),
    /// To one of these places, by their names, chosen at random; `told`: the user is told it
    /// arrived (a recall of a class of its own; 2017: a city's 208 of 208, the generic class's
    /// none of 4).
    Named { names: Vec<String>, told: bool },
    /// To the hall its user's allegiance is bound to.
    Allegiance,
}

/// An effect's definition.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectDef {
    /// Its saved state (0 for one held inside another).
    pub state: u32,
    pub kind: Kind,
    /// How long it lasts at a spellcraft (none: it does not last).
    pub duration: Scale,
    /// Its class priority at a spellcraft.
    pub priority: Scale,
    /// The effect it shows while it runs (0: none).
    pub fx: u32,
    pub external_flags: u32,
    pub internal_flags: u32,
    /// Its type, and the class whose one top effect applies.
    pub enum_value: u32,
    pub eq_class: u32,
    /// Its name (string id, table), when it has one.
    pub name: Option<(u32, u32)>,
}

impl EffectDef {
    /// Does it last (a record on the target), rather than happen once?
    #[must_use]
    pub fn lasts(&self) -> bool {
        !self.duration.is_empty() && !matches!(self.kind, Kind::InstantVital { .. })
    }

    /// The effects it adds in turn: a combination's, either way; an aura's pulsed ones.
    #[must_use]
    pub fn adds(&self) -> Vec<u32> {
        match &self.kind {
            Kind::Combo(c) => [
                c.if_present,
                c.if_not_present,
                c.give_back_if_present,
                c.give_back_if_not_present,
            ]
            .into_iter()
            .flatten()
            .collect(),
            Kind::Aura(a) => a.pulsed.clone(),
            _ => Vec::new(),
        }
    }

    /// Is it taken off at a resurrection?
    #[must_use]
    pub const fn removed_on_death(&self) -> bool {
        self.internal_flags & REMOVED_ON_DEATH != 0
    }

    /// Is its holder's client shown it?
    #[must_use]
    pub const fn client_visible(&self) -> bool {
        self.internal_flags & CLIENT_VISIBLE != 0
    }

    /// The categories its records are filed under: the ordinary one, and one for each of
    /// [`CATEGORY_FLAGS`] its internal flags hold.
    #[must_use]
    pub fn categories(&self) -> u32 {
        CATEGORY_FLAGS
            .iter()
            .filter(|(f, _)| self.internal_flags & f != 0)
            .fold(1, |c, (_, k)| c | k)
    }

    /// The qualities it changes (each `type << 16 | quality`), with how: added or multiplying,
    /// by how much at a spellcraft.
    #[must_use]
    pub fn changes(&self, spellcraft: f32) -> Vec<(u32, bool, f32)> {
        match &self.kind {
            Kind::Numeric {
                flags,
                stats,
                magnitude,
            } => {
                let m = magnitude.at(spellcraft).unwrap_or(0.0);
                let mult = flags & numeric::MULTIPLY != 0;
                stats.iter().map(|s| (*s, mult, m)).collect()
            }
            Kind::Chained(list) => list.iter().flat_map(|d| d.changes(spellcraft)).collect(),
            _ => Vec::new(),
        }
    }

    /// Read an effect from its saved state's graph.
    ///
    /// # Errors
    /// A field every effect holds is missing.
    pub fn from_state(state: u32, lib: &Library, g: &Graph) -> Result<Self, String> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root().ok_or("an empty graph")?)?;
        Self::from_object(state, lib, &nav, root)
    }

    /// The effect enumeration's value a saved state's graph holds, when it holds one.
    #[must_use]
    pub fn enum_value_of(lib: &Library, g: &Graph) -> Option<u32> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root()?).ok()?;
        nav.field(root, "m_enum").ok()
    }

    fn from_object(
        state: u32,
        lib: &Library,
        nav: &Nav<'_>,
        o: &PackedObject,
    ) -> Result<Self, String> {
        let word = |name: &str| nav.field(o, name);
        let is = |base: &str| {
            lib.package_id(base)
                .is_some_and(|b| lib.derives_from(o.package_id(), b))
        };
        let scale = |name: &str| -> Scale {
            Scale(
                nav.list(o, name)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|p| {
                        Some((
                            nav.number(p, "m_level").ok()?,
                            nav.number(p, "m_value").ok()?,
                        ))
                    })
                    .collect(),
            )
        };
        let words = |name: &str| -> Vec<u32> {
            nav.field(o, name)
                .ok()
                .filter(|r| *r != 0 && *r != NULL)
                .and_then(|r| nav.items(r).ok())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|i| match i {
                    Item::Word(w) => Some(w),
                    _ => None,
                })
                .collect()
        };
        let state_of = |name: &str| -> Option<u32> {
            let r = nav.field(o, name).ok().filter(|r| *r != 0 && *r != NULL)?;
            nav.states_of_ref(r)
        };
        let flags = word("m_uiExternalFlags")?;
        let kind = if is("InstantVitalEffect") {
            Kind::InstantVital {
                flags,
                change: scale("m_changeData"),
            }
        } else if is("VitalOverTimeEffect") {
            Kind::VitalOverTime {
                flags,
                per_second: scale("m_changeData"),
            }
        } else if is("ChainedNumericEffect") {
            Kind::Chained(
                nav.list(o, "m_listEffect")?
                    .into_iter()
                    .map(|c| Self::from_object(0, lib, nav, c))
                    .collect::<Result<_, _>>()?,
            )
        } else if is("SlayerEffect") {
            let classes = nav
                .field(o, "m_SlayerHash")
                .and_then(|r| nav.entries(r))
                .unwrap_or_default();
            Kind::Slayer(
                classes
                    .into_iter()
                    .filter_map(|(k, v)| {
                        Some((Nav::word(k).ok()?, f32::from_bits(Nav::word(v).ok()?)))
                    })
                    .collect(),
            )
        } else if is("AIPetEffect") {
            // A pet effect is a numeric effect whose magnitude is its lifetime.
            Kind::Pet {
                class: word("m_petClass").unwrap_or(0),
                lifetime: scale("m_magData"),
            }
        } else if is("ParameterizedNumericEffect") {
            Kind::Numeric {
                flags,
                stats: words("m_statList"),
                magnitude: scale("m_magData"),
            }
        } else if is("QuestTriggerEffect") {
            Kind::Quest {
                quest: word("m_questID")?,
                update: word("m_questUpdateType")?,
            }
        } else if is("TeleportEffect") {
            let at = nav
                .field(o, "m_toLocation")
                .ok()
                .filter(|r| *r != 0 && *r != NULL)
                .and_then(|r| match nav.native(r).ok()? {
                    NativeObject::Position { cell, frame } => Some((*cell, *frame)),
                    _ => None,
                });
            match at {
                Some((cell, f)) => Kind::Teleport(yalain_store::StoredPosition {
                    cell,
                    origin: [
                        f32::from_bits(f[0]),
                        f32::from_bits(f[1]),
                        f32::from_bits(f[2]),
                    ],
                    rotation: [
                        f32::from_bits(f[3]),
                        f32::from_bits(f[4]),
                        f32::from_bits(f[5]),
                        f32::from_bits(f[6]),
                    ],
                }),
                None => Kind::Other,
            }
        } else if is("TravelRecallEffect") {
            Kind::Recall(Recall::Link(word("m_link")?))
        } else if is("RandomRecallEffect") {
            let names = nav
                .field(o, "m_destinationArray")
                .ok()
                .filter(|r| *r != 0 && *r != NULL)
                .and_then(|r| nav.items(r).ok())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|i| match nav.native(i.reference()?).ok()? {
                    NativeObject::Text(t) => Some(String::from_utf16_lossy(t)),
                    _ => None,
                })
                .collect();
            let generic = lib.package_id("RandomRecallEffect") == Some(o.package_id());
            Kind::Recall(Recall::Named {
                names,
                told: !generic,
            })
        } else if is("Eff_Com_AllegianceRecall") {
            Kind::Recall(Recall::Allegiance)
        } else if is("GrantRecipeEffect") {
            Kind::GrantRecipe(word("m_didRecipe")?)
        } else if is("RemoveRecipeEffect") {
            Kind::RemoveRecipe(word("m_didRecipe")?)
        } else if is("ComboEffect") {
            let adjust = |name: &str| word(name).map_or(0, u32::cast_signed);
            Kind::Combo(Box::new(Combo {
                types: words("m_listEffectPresentByType"),
                classes: words("m_listEffectPresentByClass"),
                if_present: state_of("m_effToAddIfPresent"),
                if_not_present: state_of("m_effToAddIfNotPresent"),
                give_back_if_present: state_of("m_effToGiveBackIfPresent"),
                give_back_if_not_present: state_of("m_effToGiveBackIfNotPresent"),
                adjust_if_present: adjust("m_iSpellcraftAdjustmentIfPresent"),
                adjust_if_not_present: adjust("m_iSpellcraftAdjustmentIfNotPresent"),
            }))
        } else if is("AuraEffect") {
            let range_sq = nav.number(o, "m_fMaxRangeSqr").unwrap_or(0.0);
            Kind::Aura(Aura {
                pulsed: nav.states(o, "m_listEffect").unwrap_or_default(),
                targets: word("m_uiAuraFlags").unwrap_or(0),
                range: range_sq.max(0.0).sqrt(),
            })
        } else {
            Kind::Other
        };
        Ok(Self {
            state: if state == 0 { o.state } else { state },
            kind,
            duration: scale("m_durationData"),
            priority: scale("m_priData"),
            fx: word("m_fxID")?,
            external_flags: flags,
            internal_flags: word("m_uiInternalFlags")?,
            enum_value: word("m_enum")?,
            eq_class: word("m_eqClass")?,
            name: nav
                .field(o, "m_strName")
                .ok()
                .filter(|r| *r != 0 && *r != NULL)
                .and_then(|r| match nav.native(r).ok()? {
                    NativeObject::TextInfo(t) if t.string_id != 0 => Some((t.string_id, t.table)),
                    _ => None,
                }),
        })
    }
}

/// The effects the world puts on players, by saved state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectTable {
    pub effects: HashMap<u32, EffectDef>,
}

impl EffectTable {
    /// The definitions of these saved states and of the effects they add in turn (a state that
    /// does not read is left out).
    #[must_use]
    pub fn read(lib: &Library, states: &[u32], graph: &dyn Fn(u32) -> Option<Graph>) -> Self {
        let mut t = Self::default();
        let mut todo: Vec<u32> = states.to_vec();
        while let Some(s) = todo.pop() {
            if s == 0 || t.effects.contains_key(&s) {
                continue;
            }
            let Some(g) = graph(s) else { continue };
            if let Ok(d) = EffectDef::from_state(s, lib, &g) {
                todo.extend(d.adds());
                t.effects.insert(s, d);
            }
        }
        t
    }

    #[must_use]
    pub fn get(&self, state: u32) -> Option<&EffectDef> {
        self.effects.get(&state)
    }
}

impl Nav<'_> {
    /// The saved state a reference names: a singleton's, or an object's own.
    #[must_use]
    pub fn states_of_ref(&self, r: u32) -> Option<u32> {
        self.singleton(r)
            .or_else(|| self.object(r).ok().map(|o| o.state))
    }
}
