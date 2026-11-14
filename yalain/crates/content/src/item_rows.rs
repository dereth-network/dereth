//! What the world database says of items beyond their templates: what an item's use does where its
//! template names nothing, what a book, card or person shows when used, what every new character
//! is given as it first enters, and what an item used on another makes; the one of several items
//! quest steps and gives hand out by chance; the states the world's objects of some templates
//! hold, which items given them change, and the orders of uses judged against those states.

use std::collections::BTreeMap;

/// An item's use where its template names no effect for it: the effects it puts on its user, the
/// behavior it plays, and how many uses it wears out after.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemUse {
    pub template: u32,
    /// Each effect put on, in order: its saved state and the spellcraft it is cast at.
    pub effects: Vec<(u32, f32)>,
    /// The behavior its use plays, where its template names none.
    pub behavior: Option<u32>,
    /// It wears by its uses rather than going at its first: the uses it has when it was not made
    /// with a durability of its own.
    pub wears: Option<u32>,
}

/// What a book, a card or a person shows when used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shows {
    /// A book: its pages' string table, with its page controls.
    Book(u32),
    /// A picture, with no page controls.
    Picture(u32),
}

/// What an item used on another makes, where the item's interaction table lets it be used so and
/// the table's outcome names nothing made: items given its user, and what it makes standing in the
/// world at the object it was used on.
#[derive(Debug, Clone, PartialEq)]
pub struct Interaction {
    /// The template of the item used.
    pub item: u32,
    /// What it is used on.
    pub on: On,
    /// What it makes: one of these templates, picked by weight each use (one alone is always
    /// made), and how many.
    pub gives: Vec<(u32, u32)>,
    pub count: u32,
    /// What it makes instead where the interaction the use met is for some characters only, picked
    /// so (none: `gives`).
    pub gated_gives: Vec<(u32, u32)>,
    /// What it makes at the object it was used on, each at its offset as the user faces, for its
    /// time: a chest or another object, or a creature (its guards).
    pub places: Vec<crate::effect_rules::Placing>,
    /// The effects it puts on a camp's living creatures, in order, cast by its user.
    pub effects_on: Option<EffectsOn>,
    /// The effects the object it was used on holds after, each for its own time: while it holds
    /// any of them, it refuses the use.
    pub effects_here: Vec<u32>,
    /// The line the object it was used on says to everyone near, naming its user: a string and its
    /// table.
    pub tell: Option<(u32, u32)>,
}

/// The effects a use puts on a camp's living creatures: the camp (by its place in the spawn table)
/// and the effects, by their saved states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectsOn {
    pub camp: usize,
    pub effects: Vec<u32>,
}

impl Interaction {
    /// What a use picks what it makes from: `gated_gives` where the interaction it met is for some
    /// characters only and the row names them, else `gives`.
    #[must_use]
    pub fn gives_where(&self, gated: bool) -> &[(u32, u32)] {
        if gated && !self.gated_gives.is_empty() {
            &self.gated_gives
        } else {
            &self.gives
        }
    }
}

/// What an item is used on: a template, or a class (a script package) the item's interaction table
/// names, any template of which counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum On {
    Template(u32),
    Class(u32),
}

/// The world database's item rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemRows {
    /// Item uses, by template.
    pub uses: BTreeMap<u32, ItemUse>,
    /// What each template shows when used.
    pub books: BTreeMap<u32, Shows>,
    /// What every new character is given as it first enters: template, count.
    pub start_items: BTreeMap<u32, u32>,
    /// What items used on others make, in the rows' order.
    pub interactions: Vec<Interaction>,
    /// The states objects of some templates hold, in the rows' order.
    pub object_states: Vec<ObjectStates>,
    /// Orders of uses judged against the used objects' states, in the rows' order.
    pub sequences: Vec<Sequence>,
    /// Quest steps that also hand out one of several items, picked by weight: each step with its
    /// templates and their weights.
    pub step_picks: Vec<(crate::quests::Step, Vec<(u32, u32)>)>,
    /// Gives that move no quest and also hand out one of several items, likewise.
    pub give_picks: Vec<(crate::quests::Give, Vec<(u32, u32)>)>,
    /// Quest steps that run only where an order of uses keeping its own count stands as they
    /// name: each step with its gate.
    pub sequence_steps: Vec<(crate::quests::Step, SequenceGate)>,
    /// Gives that move no quest and take their item only so, likewise.
    pub sequence_gives: Vec<(crate::quests::Give, SequenceGate)>,
}

/// Where an order of uses keeping its own count must stand for a step or a give to run: its row's
/// key, and the count it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceGate {
    pub key: String,
    pub need: SequenceNeed,
}

/// The count an order of uses must stand at: so many right uses made, or every one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceNeed {
    At(u32),
    Done,
}

/// The states the world's objects of some templates hold: each object one of them, its first at the
/// start; an item given an object changes its state where a change names the item and the state
/// it is in; it goes back to its first some time after its last change.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectStates {
    /// The row's key.
    pub key: String,
    /// The templates whose objects hold a state, each with its objects' first state.
    pub templates: Vec<(u32, String)>,
    /// The states' names.
    pub states: Vec<String>,
    /// Seconds after its last change an object goes back to its first state (none: never).
    pub reverts: Option<f64>,
    /// What changes a state: the item given, the state it changes from and the state it becomes.
    pub changes: Vec<StateChange>,
}

/// An item given to an object in one state puts it in another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange {
    pub item: u32,
    pub from: String,
    pub to: String,
}

/// Uses in an order, judged against the used objects' states: each use of the next object, in the
/// state it names, moves the row's quest on a phase (the first gives it), and the last takes the
/// player where the row says; a use of another, or of the next in another state, takes the quest
/// away from a player who has it under way, and the order starts over (unless the row starts over
/// only for an object already used). A use names a template, or one placed object of it. A row with
/// no quest keeps its own count of the right uses made, its fellowship's for a player in one.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    /// The row's key.
    pub key: String,
    /// The quest whose phase counts the right uses made; none: the row keeps its own count, one for
    /// each fellowship and one for each player in none.
    pub quest: Option<u32>,
    /// Seconds after its last right use a count of its own is forgotten, done or not (none: never).
    pub forgets_after: Option<f64>,
    /// The uses in order: the template used and the state its object must be in (none: any).
    pub uses: Vec<(u32, Option<String>)>,
    /// The placed object each use must be of, by its place in the world's list of placed objects,
    /// where the row names one; none, or empty: any object of the use's template.
    pub objects: Vec<Option<usize>>,
    /// Whether a use of any object the row names other than the next starts the order over: else
    /// only one of an object already used does, and a use of one further on is told the wrong
    /// line and changes nothing.
    pub wrong_starts_over: bool,
    /// What the object used says at a right use and at a wrong one: (string, table).
    pub right: Option<(u32, u32)>,
    pub wrong: Option<(u32, u32)>,
    /// Where the last right use takes the player.
    pub to: Option<crate::quests::Place>,
}

impl ItemRows {
    /// The effects an item of this template's use puts on that its template doesn't name: saved
    /// state and spellcraft.
    pub fn effects(&self, template: u32) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.uses
            .get(&template)
            .into_iter()
            .flat_map(|u| u.effects.iter().copied())
    }

    /// The behavior an item of this template's use plays, where its template names none.
    #[must_use]
    pub fn behavior(&self, template: u32) -> Option<u32> {
        self.uses.get(&template).and_then(|u| u.behavior)
    }

    /// The uses an item of this template wears out after, when it wears by its uses.
    #[must_use]
    pub fn wears(&self, template: u32) -> Option<u32> {
        self.uses.get(&template).and_then(|u| u.wears)
    }

    /// Every saved state the rows' uses put on.
    pub fn states(&self) -> impl Iterator<Item = u32> + '_ {
        self.uses
            .values()
            .flat_map(|u| u.effects.iter().map(|(s, _)| *s))
    }

    /// Every saved state the rows' interactions put on a camp's creatures or leave on the object
    /// used on.
    pub fn interaction_states(&self) -> impl Iterator<Item = u32> + '_ {
        self.interactions.iter().flat_map(|i| {
            i.effects_on
                .iter()
                .flat_map(|o| o.effects.iter().copied())
                .chain(i.effects_here.iter().copied())
        })
    }

    /// The states row of a template whose objects hold a state.
    #[must_use]
    pub fn states_of(&self, template: u32) -> Option<&ObjectStates> {
        self.object_states
            .iter()
            .find(|r| r.templates.iter().any(|(t, _)| *t == template))
    }

    /// Does an order of uses name a use of this template?
    #[must_use]
    pub fn in_a_sequence(&self, template: u32) -> bool {
        self.sequences
            .iter()
            .any(|q| q.uses.iter().any(|(t, _)| *t == template))
    }

    /// Every template the rows give.
    pub fn given(&self) -> impl Iterator<Item = u32> + '_ {
        let picked = self
            .step_picks
            .iter()
            .map(|(_, p)| p)
            .chain(self.give_picks.iter().map(|(_, p)| p));
        self.start_items
            .keys()
            .copied()
            .chain(
                self.interactions
                    .iter()
                    .flat_map(|i| i.gives.iter().chain(&i.gated_gives).map(|(t, _)| *t)),
            )
            .chain(picked.flat_map(|p| p.iter().map(|(t, _)| *t)))
    }

    /// The one of several items a step also hands out, if it names some: the templates and their
    /// weights of the step of its quest and key.
    #[must_use]
    pub fn step_pick(&self, step: &crate::quests::Step) -> Option<&[(u32, u32)]> {
        self.step_picks
            .iter()
            .find(|(s, _)| s.quest == step.quest && s.key == step.key)
            .map(|(_, p)| p.as_slice())
    }

    /// The orders of uses a step waits on: the gates of the step of its quest and key.
    pub fn step_sequences<'a>(
        &'a self,
        step: &'a crate::quests::Step,
    ) -> impl Iterator<Item = &'a SequenceGate> + 'a {
        self.sequence_steps
            .iter()
            .filter(move |(s, _)| s.quest == step.quest && s.key == step.key)
            .map(|(_, g)| g)
    }

    /// The orders of uses a give waits on.
    pub fn give_sequences<'a>(
        &'a self,
        give: &'a crate::quests::Give,
    ) -> impl Iterator<Item = &'a SequenceGate> + 'a {
        self.sequence_gives
            .iter()
            .filter(move |(g, _)| g == give)
            .map(|(_, g)| g)
    }

    /// The one of several items a give also hands out, if it names some.
    #[must_use]
    pub fn give_pick(&self, give: &crate::quests::Give) -> Option<&[(u32, u32)]> {
        self.give_picks
            .iter()
            .find(|(g, _)| g == give)
            .map(|(_, p)| p.as_slice())
    }

    /// What an item of a template used on one of another makes: the first row for the item that
    /// names the target's template, else the first that names a class `in_class` says the target
    /// counts in.
    #[must_use]
    pub fn made_by(
        &self,
        item: u32,
        target: u32,
        in_class: impl Fn(u32) -> bool,
    ) -> Option<&Interaction> {
        let rows = || self.interactions.iter().filter(move |i| i.item == item);
        rows()
            .find(|i| i.on == On::Template(target))
            .or_else(|| rows().find(|i| matches!(i.on, On::Class(c) if in_class(c))))
    }
}
