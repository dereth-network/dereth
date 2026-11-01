//! A preference's value as an option of the options window holds it: the value now, the value
//! last applied, the default; a number's range, a choice's choices.
//!
//! Every option of the window keeps one: the window's control shows what the player is setting,
//! the preference what was applied. An option has changed when its control differs from its
//! preference; applying it copies the control into the preference and marks that value applied;
//! recalling it puts the preference back into the control; the default puts the default there.

/// What kind of value a preference holds.
pub mod kind {
    /// One of a set of choices.
    pub const CHOICE: u32 = 0x4000_0001;
    /// A number within a range.
    pub const NUMBER: u32 = 0x4000_0002;
    /// Yes or no.
    pub const FLAG: u32 = 0x4000_0003;
}

/// What names a preference: its section and its key, the string table and string of its label,
/// the string of its help (0: none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Description {
    pub section: u32,
    pub key: u32,
    pub table: u32,
    pub label: u32,
    pub help: u32,
}

impl Description {
    /// The label and help in the client's own table of preference strings.
    // D2-15914
    pub fn describe_by_label(&mut self, label: u32) {
        self.table = 0x4000_0008;
        self.label = label;
        self.help = 0;
    }

    /// The label and its help in a table.
    // D2-15916
    pub fn describe(&mut self, table: u32, label: u32, help: u32) {
        self.table = table;
        self.label = label;
        self.help = help;
    }

    /// Which preference it is: its section and its key.
    // D2-15921
    #[must_use]
    pub const fn identify(&self) -> (u32, u32) {
        (self.section, self.key)
    }
}

/// A yes-or-no preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flag {
    pub description: Description,
    current: bool,
    previous: bool,
    default: bool,
}

impl Flag {
    /// A new one: no, applied, by default.
    // D2-15926
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Its kind.
    // D2-15939
    #[must_use]
    pub const fn kind(&self) -> u32 {
        kind::FLAG
    }

    /// Another's description and values, when it is a yes-or-no one.
    // D2-15930
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description = other.description;
        self.current = other.get();
        self.default = other.default;
        true
    }

    /// Set the value now.
    // D2-15933
    pub fn set_value(&mut self, v: bool) -> bool {
        self.current = v;
        true
    }

    /// Set the default, and the value now with it.
    // D2-15934
    pub fn set_default(&mut self, v: bool) -> bool {
        self.default = v;
        self.current = v;
        true
    }

    /// Whether the value now differs from the one last applied.
    // D2-15935
    #[must_use]
    pub fn changed(&self) -> bool {
        self.get() != self.previous
    }

    /// The value now marked applied.
    // D2-15936
    pub fn apply(&mut self) {
        self.previous = self.get();
    }

    /// The value last applied made the value now.
    // D2-15937
    pub fn recall(&mut self) {
        self.set_value(self.previous);
    }

    /// The default made the value now.
    // D2-15938
    pub fn reset(&mut self) {
        self.set_value(self.default);
    }

    /// The value now.
    // D2-15940
    #[must_use]
    pub const fn get(&self) -> bool {
        self.current
    }

    /// The default.
    #[must_use]
    pub const fn default_value(&self) -> bool {
        self.default
    }
}

/// A number within a range.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Number {
    pub description: Description,
    current: f32,
    previous: f32,
    default: f32,
    least: f32,
    greatest: f32,
}

impl Number {
    /// A new one: 0, applied, by default, over no range.
    // D2-15946
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Its kind.
    // D2-15959
    #[must_use]
    pub const fn kind(&self) -> u32 {
        kind::NUMBER
    }

    /// Another's description, values and range.
    // D2-15950
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description = other.description;
        self.current = other.get();
        self.default = other.default;
        self.least = other.least;
        self.greatest = other.greatest;
        true
    }

    /// The range, unless its least is greater than its greatest.
    // D2-15952
    pub fn set_range(&mut self, least: f32, greatest: f32) -> bool {
        if greatest < least {
            return false;
        }
        self.least = least;
        self.greatest = greatest;
        true
    }

    /// Set the value now (not held to the range).
    // D2-15953
    pub fn set_value(&mut self, v: f32) -> bool {
        self.current = v;
        true
    }

    /// Set the default, and the value now with it.
    // D2-15954
    pub fn set_default(&mut self, v: f32) -> bool {
        self.default = v;
        self.current = v;
        true
    }

    /// The range: its least and its greatest.
    // D2-15955
    #[must_use]
    pub const fn range(&self) -> (f32, f32) {
        (self.least, self.greatest)
    }

    /// Whether the value now differs from the one last applied.
    // D2-15956
    #[must_use]
    pub fn changed(&self) -> bool {
        #[allow(clippy::float_cmp)]
        let same = self.get() == self.previous;
        !same
    }

    /// The value now marked applied.
    // D2-15957
    pub fn apply(&mut self) {
        self.previous = self.get();
    }

    /// The value last applied made the value now.
    // D2-15958
    pub fn recall(&mut self) {
        self.set_value(self.previous);
    }

    /// The default made the value now.
    pub fn reset(&mut self) {
        self.set_value(self.default);
    }

    /// The value now.
    // D2-15960
    #[must_use]
    pub const fn get(&self) -> f32 {
        self.current
    }

    /// The default.
    // D2-15961
    #[must_use]
    pub const fn default_value(&self) -> f32 {
        self.default
    }
}

/// One of a set of choices, each a number with its label.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub description: Description,
    current: i32,
    previous: i32,
    default: i32,
    /// The choices: each one's number and its label (a string table's enumeration value and the
    /// string).
    choices: Vec<(i32, (u32, u32))>,
}

impl Choice {
    /// A new one: 0, applied, by default, with no choices.
    // D2-16014
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Its kind.
    // D2-15990
    #[must_use]
    pub const fn kind(&self) -> u32 {
        kind::CHOICE
    }

    /// Another's description, values and choices.
    // D2-16000
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description = other.description;
        self.current = other.get();
        self.default = other.default;
        self.choices.clone_from(&other.choices);
        true
    }

    /// Whether `v` is one of the choices.
    fn holds(&self, v: i32) -> bool {
        self.choices.iter().any(|(k, _)| *k == v)
    }

    /// Set the value now, when it is one of the choices.
    // D2-16002
    pub fn set_value(&mut self, v: i32) -> bool {
        if !self.holds(v) {
            return false;
        }
        self.current = v;
        true
    }

    /// Set the default, and the value now with it, when it is one of the choices.
    // D2-16004
    pub fn set_default(&mut self, v: i32) -> bool {
        if !self.holds(v) {
            return false;
        }
        self.default = v;
        self.current = v;
        true
    }

    /// Whether the value now differs from the one last applied.
    // D2-15986
    #[must_use]
    pub fn changed(&self) -> bool {
        self.get() != self.previous
    }

    /// The value now marked applied.
    // D2-15987
    pub fn apply(&mut self) {
        self.previous = self.get();
    }

    /// The value last applied made the value now.
    // D2-15988
    pub fn recall(&mut self) {
        self.set_value(self.previous);
    }

    /// The default made the value now.
    // D2-15989
    pub fn reset(&mut self) {
        self.set_value(self.default);
    }

    /// The value now.
    // D2-15991
    #[must_use]
    pub const fn get(&self) -> i32 {
        self.current
    }

    /// The default.
    // D2-15992
    #[must_use]
    pub const fn default_value(&self) -> i32 {
        self.default
    }

    /// A choice and its label, put in (or its label replaced).
    // D2-16007
    pub fn assign_choice(&mut self, v: i32, label: (u32, u32)) -> bool {
        match self.choices.iter_mut().find(|(k, _)| *k == v) {
            Some(c) => c.1 = label,
            None => self.choices.push((v, label)),
        }
        true
    }

    /// The choices and their labels.
    // D2-16011
    #[must_use]
    pub fn list_choices(&self) -> &[(i32, (u32, u32))] {
        &self.choices
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a preference's values on their own, below the options window)
    use super::*;

    #[test]
    fn a_choice_takes_only_one_of_its_choices() {
        let mut c = Choice::new();
        assert!(!c.set_default(1));
        c.assign_choice(1, (0, 1));
        c.assign_choice(2, (0, 2));
        assert!(c.set_default(1));
        assert!(!c.set_value(7));
        assert_eq!(c.get(), 1);
        assert!(c.set_value(2));
        assert!(c.changed());
        c.apply();
        assert!(!c.changed());
        c.reset();
        assert_eq!(c.get(), 1);
        c.recall();
        assert_eq!(c.get(), 2);
    }

    #[test]
    fn a_number_keeps_an_ordered_range() {
        let mut n = Number::new();
        assert!(!n.set_range(1.0, 0.0));
        assert!(n.set_range(0.0, 1.0));
        n.set_default(0.5);
        assert_eq!(n.range(), (0.0, 1.0));
        assert!(n.changed());
        n.apply();
        assert!(!n.changed());
    }
}
