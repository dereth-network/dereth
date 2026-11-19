//! A preference's value as an option of the options window holds it: the value now, the value
//! last applied, the default; a number's range, a choice's choices.
//!
//! Every option of the window keeps one: the window's control shows what the player is setting,
//! the preference what was applied. An option has changed when its control differs from its
//! preference; applying it copies the control into the preference and marks that value applied;
//! recalling it puts the preference back into the control; the default puts the default there.
//!
//! Each kind also reads and writes its value as the preferences file keeps it: a yes-or-no as `1`
//! or `0`, a number with six places, a choice by its name in its enumeration (or its number), a
//! text as it is.

use auberean_dat::tree::number;

/// What kind of value a preference holds.
pub mod kind {
    /// One of a set of choices.
    pub const CHOICE: u32 = 0x4000_0001;
    /// A number within a range.
    pub const NUMBER: u32 = 0x4000_0002;
    /// Yes or no.
    pub const FLAG: u32 = 0x4000_0003;
    /// A text.
    pub const TEXT: u32 = 0x4000_0004;
}

/// The names the data gives an enumeration's values, which a choice's text form is written in.
pub trait Enumerations {
    /// The value a name has in an enumeration, letter case ignored.
    fn value_of(&self, enumeration: u32, name: &str) -> Option<u32>;
    /// A value's name in an enumeration.
    fn name_of(&self, enumeration: u32, value: u32) -> Option<String>;
}

/// No enumeration names anything.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoEnumerations;

impl Enumerations for NoEnumerations {
    fn value_of(&self, _: u32, _: &str) -> Option<u32> {
        None
    }
    fn name_of(&self, _: u32, _: u32) -> Option<String> {
        None
    }
}

/// What names a preference: its section and its key, the string table and string of its label,
/// the string of its help (0: none). A new one names nothing.
// D2-15910, D2-15911, D2-15923
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

    /// How many of the five names differ from another's.
    #[must_use]
    pub fn compare(&self, other: &Self) -> u32 {
        u32::from(self.section != other.section)
            + u32::from(self.key != other.key)
            + u32::from(self.table != other.table)
            + u32::from(self.label != other.label)
            + u32::from(self.help != other.help)
    }

    /// Another's names taken.
    // D2-15913
    pub fn copy_from(&mut self, other: &Self) {
        *self = *other;
    }

    /// The string of its help (0: none).
    // D2-15918
    #[must_use]
    pub const fn help(&self) -> u32 {
        self.help
    }

    /// The label as a string reference: its table and its string.
    // D2-15917
    #[must_use]
    pub const fn string(&self) -> (u32, u32) {
        (self.table, self.label)
    }
}

/// A number as the file keeps one: six places after the point, as `%f` writes it.
fn six_places(v: f32) -> String {
    let mut text = Vec::new();
    auberean_text::wide::append_float(&mut text, v);
    String::from_utf16_lossy(&text)
}

/// A yes-or-no preference.
// D2-15926, D2-15927, D2-15941
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

    /// A new one with this one's description, value and default (not the value last applied).
    // D2-15928
    #[must_use]
    pub fn duplicate(&self) -> Self {
        let mut f = Self::new();
        f.copy(self);
        f
    }

    /// How it differs from another: its names, its kind, and when the other is a yes-or-no one,
    /// the value now and the default.
    // D2-15929, D2-15912
    #[must_use]
    pub fn compare(&self, other: &Pref) -> u32 {
        let mut n =
            self.description.compare(other.description()) + u32::from(other.kind() != kind::FLAG);
        if let Pref::Flag(o) = other {
            n += u32::from(self.current != o.get()) + u32::from(self.default != o.default);
        }
        n
    }

    /// Another's description and values, when it is a yes-or-no one.
    // D2-15930
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description.copy_from(&other.description);
        self.current = other.get();
        self.default = other.default;
        true
    }

    /// Take another's value now, when it is a yes-or-no one.
    // D2-15932
    pub fn connect_target(&mut self, other: &Self) -> bool {
        self.current = other.get();
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

    /// Set the value now from the file's text: `0` or `1`, nothing else.
    // D2-15942
    pub fn set_value_text(&mut self, text: &str) -> bool {
        match text {
            "0" => self.current = false,
            "1" => self.current = true,
            _ => return false,
        }
        true
    }

    /// Set the default from the file's text (`0` or `1`), and the value now to the default
    /// whatever the text.
    // D2-15943
    pub fn set_default_text(&mut self, text: &str) -> bool {
        let read = match text {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        };
        if let Some(v) = read {
            self.default = v;
        }
        self.current = self.default;
        read.is_some()
    }

    /// The value now as the file keeps it.
    // D2-15944
    #[must_use]
    pub fn value_text(&self) -> String {
        if self.get() { "1" } else { "0" }.to_owned()
    }

    /// The default as the file keeps it.
    // D2-15945
    #[must_use]
    pub fn default_text(&self) -> String {
        if self.default { "1" } else { "0" }.to_owned()
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
// D2-15946, D2-15947, D2-15962
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Number {
    pub description: Description,
    current: f32,
    previous: f32,
    default: f32,
    least: f32,
    greatest: f32,
}

impl Default for Number {
    fn default() -> Self {
        Self::new()
    }
}

impl Number {
    /// A new one: a half, applied, by default, over 0 to 1.
    // D2-15946
    #[must_use]
    pub const fn new() -> Self {
        Self {
            description: Description {
                section: 0,
                key: 0,
                table: 0,
                label: 0,
                help: 0,
            },
            current: 0.5,
            previous: 0.5,
            default: 0.5,
            least: 0.0,
            greatest: 1.0,
        }
    }

    /// Its kind.
    // D2-15959
    #[must_use]
    pub const fn kind(&self) -> u32 {
        kind::NUMBER
    }

    /// A new one with this one's description, value, default and range (the value last applied a
    /// new one's).
    // D2-15948
    #[must_use]
    pub fn duplicate(&self) -> Self {
        let mut n = Self::new();
        n.copy(self);
        n
    }

    /// How it differs from another: its names, its kind, and when the other is a number, the value
    /// now, the default and the range's two ends.
    // D2-15949
    #[must_use]
    #[allow(clippy::float_cmp)] // the client compares them exactly
    pub fn compare(&self, other: &Pref) -> u32 {
        let mut n =
            self.description.compare(other.description()) + u32::from(other.kind() != kind::NUMBER);
        if let Pref::Number(o) = other {
            n += u32::from(o.get() != self.current)
                + u32::from(o.default != self.default)
                + u32::from(self.least != o.least)
                + u32::from(self.greatest != o.greatest);
        }
        n
    }

    /// Another's description, values and range.
    // D2-15950
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description.copy_from(&other.description);
        self.current = other.get();
        self.default = other.default;
        self.least = other.least;
        self.greatest = other.greatest;
        true
    }

    /// Take another's value now.
    // D2-15951
    pub fn connect_target(&mut self, other: &Self) -> bool {
        self.current = other.get();
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

    /// The file's text as a number: only when the whole text reads as one.
    #[allow(clippy::cast_possible_truncation)] // kept in single precision
    fn read_text(text: &str) -> Option<f32> {
        number::is_double(text.as_bytes()).then(|| number::strtod(text.as_bytes()).value as f32)
    }

    /// Set the value now from the file's text, when it reads whole as a number.
    // D2-15963
    pub fn set_value_text(&mut self, text: &str) -> bool {
        Self::read_text(text).is_some_and(|v| self.set_value(v))
    }

    /// Set the default, and the value now with it, from the file's text, when it reads whole as a
    /// number.
    // D2-15964
    pub fn set_default_text(&mut self, text: &str) -> bool {
        Self::read_text(text).is_some_and(|v| self.set_default(v))
    }

    /// The value now as the file keeps it.
    // D2-15965
    #[must_use]
    pub fn value_text(&self) -> String {
        six_places(self.get())
    }

    /// The default as the file keeps it.
    // D2-15966
    #[must_use]
    pub fn default_text(&self) -> String {
        six_places(self.default)
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

/// One of a set of choices, each a number with its label and, when the choice is named by an
/// enumeration, the value naming it there.
// D2-15999
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub description: Description,
    /// The enumeration the choices' names are in (0: none, the choices written as numbers).
    enumeration: u32,
    current: i32,
    previous: i32,
    default: i32,
    /// The choices: each one's number and its label (a string table's enumeration value and the
    /// string).
    choices: Vec<(i32, (u32, u32))>,
    /// The choices named in the enumeration: each one's number and the value naming it.
    named: Vec<(i32, u32)>,
    /// The choices labeled in words of their own, no string table's: each one's number and words.
    literals: Vec<(i32, String)>,
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

    /// A new one with this one's description, values, enumeration and choices.
    // D2-16015
    #[must_use]
    pub fn duplicate(&self) -> Self {
        let mut c = Self::new();
        c.copy(self);
        c
    }

    /// How it differs from another: its names, its kind, and when the other is a choice, the
    /// enumeration, the value now and the default.
    // D2-15983
    #[must_use]
    pub fn compare(&self, other: &Pref) -> u32 {
        let mut n =
            self.description.compare(other.description()) + u32::from(other.kind() != kind::CHOICE);
        if let Pref::Choice(o) = other {
            n += u32::from(self.enumeration != o.enumeration)
                + u32::from(self.current != o.get())
                + u32::from(self.default != o.default_value());
        }
        n
    }

    /// Another's description, enumeration, values and choices.
    // D2-16000
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description.copy_from(&other.description);
        self.enumeration = other.enumeration;
        self.current = other.get();
        self.default = other.default;
        self.choices.clone_from(&other.choices);
        self.named.clone_from(&other.named);
        self.literals.clone_from(&other.literals);
        true
    }

    /// Take another's value now.
    // D2-15984
    pub fn connect_target(&mut self, other: &Self) -> bool {
        self.current = other.get();
        true
    }

    /// The enumeration the choices' names are in.
    // D2-15985
    pub fn map(&mut self, enumeration: u32) -> bool {
        self.enumeration = enumeration;
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

    /// The choice an enumeration value names; the default when none does.
    // D2-15994
    #[must_use]
    pub fn find_value(&self, value: u32) -> i32 {
        self.named
            .iter()
            .find(|(_, v)| *v == value)
            .map_or_else(|| self.default_value(), |(k, _)| *k)
    }

    /// The choice the file's text names: a whole number as it reads (decimal, `0x` hexadecimal or
    /// `0` octal), else, with an enumeration, the choice its name in the enumeration names.
    fn read_text(&self, text: &str, names: &dyn Enumerations) -> Option<i32> {
        if number::is_int32(text.as_bytes()) {
            return Some(number::strtol(text.as_bytes()).value);
        }
        if self.enumeration == 0 {
            return None;
        }
        let value = names.value_of(self.enumeration, text)?;
        Some(self.find_value(value))
    }

    /// Set the value now from the file's text, when it names one of the choices.
    // D2-16001
    pub fn set_value_text(&mut self, text: &str, names: &dyn Enumerations) -> bool {
        self.read_text(text, names)
            .is_some_and(|v| self.set_value(v))
    }

    /// Set the default, and the value now with it, from the file's text, when it names one of the
    /// choices.
    // D2-16003
    pub fn set_default_text(&mut self, text: &str, names: &dyn Enumerations) -> bool {
        self.read_text(text, names)
            .is_some_and(|v| self.set_default(v))
    }

    /// A choice as the file keeps it: by its name in the enumeration when it has one there, else
    /// as a number.
    fn text_of(&self, v: i32, names: &dyn Enumerations) -> String {
        if self.enumeration != 0 {
            if let Some((_, named)) = self.named.iter().find(|(k, _)| *k == v) {
                return names.name_of(self.enumeration, *named).unwrap_or_default();
            }
        }
        v.to_string()
    }

    /// The value now as the file keeps it.
    // D2-16005
    #[must_use]
    pub fn value_text(&self, names: &dyn Enumerations) -> String {
        self.text_of(self.get(), names)
    }

    /// The default as the file keeps it.
    // D2-16006
    #[must_use]
    pub fn default_text(&self, names: &dyn Enumerations) -> String {
        self.text_of(self.default, names)
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

    /// A choice, its label and the enumeration value naming it, put in (or replaced).
    // D2-16008
    pub fn assign_named_choice(&mut self, v: i32, label: (u32, u32), named: u32) -> bool {
        match self.named.iter_mut().find(|(k, _)| *k == v) {
            Some(c) => c.1 = named,
            None => self.named.push((v, named)),
        }
        self.assign_choice(v, label)
    }

    /// A choice labeled in words of its own, put in (or its words replaced).
    // D2-16007
    pub fn assign_literal_choice(&mut self, v: i32, words: &str) -> bool {
        match self.literals.iter_mut().find(|(k, _)| *k == v) {
            Some(c) => words.clone_into(&mut c.1),
            None => self.literals.push((v, words.to_owned())),
        }
        self.assign_choice(v, (0, 0))
    }

    /// A choice's words of its own, when it has them.
    #[must_use]
    pub fn literal_choice(&self, v: i32) -> Option<&str> {
        self.literals
            .iter()
            .find(|(k, _)| *k == v)
            .map(|(_, w)| w.as_str())
    }

    /// A choice's label.
    // D2-16009
    #[must_use]
    pub fn examine_choice(&self, v: i32) -> Option<(u32, u32)> {
        self.choices.iter().find(|(k, _)| *k == v).map(|(_, l)| *l)
    }

    /// A choice's label and the enumeration value naming it, when it has both.
    // D2-16010
    #[must_use]
    pub fn examine_named_choice(&self, v: i32) -> Option<((u32, u32), u32)> {
        let named = self.named.iter().find(|(k, _)| *k == v).map(|(_, n)| *n)?;
        Some((self.examine_choice(v)?, named))
    }

    /// The choices and their labels.
    // D2-16011
    #[must_use]
    pub fn list_choices(&self) -> &[(i32, (u32, u32))] {
        &self.choices
    }

    /// No choices any more.
    // D2-15996
    pub fn clear_choices(&mut self) {
        self.choices.clear();
        self.named.clear();
        self.literals.clear();
    }
}

/// A text preference (a file's name).
// D2-15981, D2-15972, D2-15980
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text {
    pub description: Description,
    current: String,
    previous: String,
    default: String,
}

impl Text {
    /// A new one: empty, applied, by default.
    // D2-15981
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Its kind.
    // D2-15971
    #[must_use]
    pub const fn kind(&self) -> u32 {
        kind::TEXT
    }

    /// A new one with this one's description, value and default.
    // D2-15982
    #[must_use]
    pub fn duplicate(&self) -> Self {
        let mut t = Self::new();
        t.copy(self);
        t
    }

    /// How it differs from another: its names, its kind, and when the other is a text, the value
    /// now and the default, letter case ignored.
    // D2-15973
    #[must_use]
    pub fn compare(&self, other: &Pref) -> u32 {
        let mut n =
            self.description.compare(other.description()) + u32::from(other.kind() != kind::TEXT);
        if let Pref::Text(o) = other {
            n += u32::from(!o.get().eq_ignore_ascii_case(&self.current))
                + u32::from(!o.default.eq_ignore_ascii_case(&self.default));
        }
        n
    }

    /// Another's description, value and default.
    // D2-15967
    pub fn copy(&mut self, other: &Self) -> bool {
        self.description.copy_from(&other.description);
        other.get().clone_into(&mut self.current);
        other.default.clone_into(&mut self.default);
        true
    }

    /// Take another's value now.
    // D2-15968
    pub fn connect_target(&mut self, other: &Self) -> bool {
        other.get().clone_into(&mut self.current);
        true
    }

    /// Set the value now.
    // D2-15974
    pub fn set_value(&mut self, v: &str) -> bool {
        v.clone_into(&mut self.current);
        true
    }

    /// Set the default, and the value now with it.
    // D2-15975
    pub fn set_default(&mut self, v: &str) -> bool {
        v.clone_into(&mut self.default);
        v.clone_into(&mut self.current);
        true
    }

    /// The value now.
    // D2-15976
    #[must_use]
    pub fn get(&self) -> &str {
        &self.current
    }

    /// The default.
    // D2-15977
    #[must_use]
    pub fn default_value(&self) -> &str {
        &self.default
    }

    /// Whether the value now differs from the one last applied, letter case ignored.
    // D2-15978
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.current.eq_ignore_ascii_case(&self.previous)
    }

    /// The value now marked applied.
    // D2-15979
    pub fn apply(&mut self) {
        self.previous.clone_from(&self.current);
    }

    /// The value last applied made the value now.
    // D2-15969
    pub fn recall(&mut self) {
        let p = self.previous.clone();
        self.set_value(&p);
    }

    /// The default made the value now.
    // D2-15970
    pub fn reset(&mut self) {
        let d = self.default.clone();
        self.set_value(&d);
    }
}

/// A preference of any kind.
#[derive(Clone, Debug, PartialEq)]
pub enum Pref {
    Flag(Flag),
    Number(Number),
    Choice(Choice),
    Text(Text),
}

impl Pref {
    /// Its kind.
    #[must_use]
    pub const fn kind(&self) -> u32 {
        match self {
            Self::Flag(_) => kind::FLAG,
            Self::Number(_) => kind::NUMBER,
            Self::Choice(_) => kind::CHOICE,
            Self::Text(_) => kind::TEXT,
        }
    }

    /// What names it.
    #[must_use]
    pub const fn description(&self) -> &Description {
        match self {
            Self::Flag(p) => &p.description,
            Self::Number(p) => &p.description,
            Self::Choice(p) => &p.description,
            Self::Text(p) => &p.description,
        }
    }

    /// What names it, to change.
    pub fn description_mut(&mut self) -> &mut Description {
        match self {
            Self::Flag(p) => &mut p.description,
            Self::Number(p) => &mut p.description,
            Self::Choice(p) => &mut p.description,
            Self::Text(p) => &mut p.description,
        }
    }

    /// A new one of the same kind with its description and values.
    #[must_use]
    pub fn duplicate(&self) -> Self {
        match self {
            Self::Flag(p) => Self::Flag(p.duplicate()),
            Self::Number(p) => Self::Number(p.duplicate()),
            Self::Choice(p) => Self::Choice(p.duplicate()),
            Self::Text(p) => Self::Text(p.duplicate()),
        }
    }

    /// How many things differ from another.
    #[must_use]
    pub fn compare(&self, other: &Self) -> u32 {
        match self {
            Self::Flag(p) => p.compare(other),
            Self::Number(p) => p.compare(other),
            Self::Choice(p) => p.compare(other),
            Self::Text(p) => p.compare(other),
        }
    }

    /// Another's description and values, when it is of the same kind.
    pub fn copy_from(&mut self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flag(a), Self::Flag(b)) => a.copy(b),
            (Self::Number(a), Self::Number(b)) => a.copy(b),
            (Self::Choice(a), Self::Choice(b)) => a.copy(b),
            (Self::Text(a), Self::Text(b)) => a.copy(b),
            _ => false,
        }
    }

    /// Take another's value now, when it is of the same kind.
    pub fn connect_target(&mut self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flag(a), Self::Flag(b)) => a.connect_target(b),
            (Self::Number(a), Self::Number(b)) => a.connect_target(b),
            (Self::Choice(a), Self::Choice(b)) => a.connect_target(b),
            (Self::Text(a), Self::Text(b)) => a.connect_target(b),
            _ => false,
        }
    }

    /// Whether the value now differs from the one last applied.
    #[must_use]
    pub fn changed(&self) -> bool {
        match self {
            Self::Flag(p) => p.changed(),
            Self::Number(p) => p.changed(),
            Self::Choice(p) => p.changed(),
            Self::Text(p) => p.changed(),
        }
    }

    /// The value now marked applied.
    pub fn apply(&mut self) {
        match self {
            Self::Flag(p) => p.apply(),
            Self::Number(p) => p.apply(),
            Self::Choice(p) => p.apply(),
            Self::Text(p) => p.apply(),
        }
    }

    /// The value last applied made the value now.
    pub fn recall(&mut self) {
        match self {
            Self::Flag(p) => p.recall(),
            Self::Number(p) => p.recall(),
            Self::Choice(p) => p.recall(),
            Self::Text(p) => p.recall(),
        }
    }

    /// The default made the value now.
    pub fn reset(&mut self) {
        match self {
            Self::Flag(p) => p.reset(),
            Self::Number(p) => p.reset(),
            Self::Choice(p) => p.reset(),
            Self::Text(p) => p.reset(),
        }
    }

    /// Set the value now from the file's text.
    pub fn set_value_text(&mut self, text: &str, names: &dyn Enumerations) -> bool {
        match self {
            Self::Flag(p) => p.set_value_text(text),
            Self::Number(p) => p.set_value_text(text),
            Self::Choice(p) => p.set_value_text(text, names),
            Self::Text(p) => p.set_value(text),
        }
    }

    /// Set the default from the file's text.
    pub fn set_default_text(&mut self, text: &str, names: &dyn Enumerations) -> bool {
        match self {
            Self::Flag(p) => p.set_default_text(text),
            Self::Number(p) => p.set_default_text(text),
            Self::Choice(p) => p.set_default_text(text, names),
            Self::Text(p) => p.set_default(text),
        }
    }

    /// The value now as the file keeps it.
    #[must_use]
    pub fn value_text(&self, names: &dyn Enumerations) -> String {
        match self {
            Self::Flag(p) => p.value_text(),
            Self::Number(p) => p.value_text(),
            Self::Choice(p) => p.value_text(names),
            Self::Text(p) => p.get().to_owned(),
        }
    }

    /// The default as the file keeps it.
    #[must_use]
    pub fn default_text(&self, names: &dyn Enumerations) -> String {
        match self {
            Self::Flag(p) => p.default_text(),
            Self::Number(p) => p.default_text(),
            Self::Choice(p) => p.default_text(names),
            Self::Text(p) => p.default_value().to_owned(),
        }
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
        n.set_default(0.25);
        assert_eq!(n.range(), (0.0, 1.0));
        assert!(n.changed());
        n.apply();
        assert!(!n.changed());
    }
}
