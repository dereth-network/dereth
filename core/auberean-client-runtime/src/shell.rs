//! The player's command line: the lines the player typed, the commands the player's scripts made
//! known, and a line typed carried out.
//!
//! **The history.** Each line typed goes to the front of the list (an earlier copy of the same
//! line is taken out first), and stepping starts again from before the front. Stepping back goes
//! one line older, stepping on one line newer; past the newest there is nothing, past the oldest
//! the oldest stays. The list is meant to keep 20 lines, but its trimming never takes one away
//! (the client compares the other way round), so it keeps every line.
//!
//! **The commands.** The player's scripts make their commands known when the player comes into the
//! world: each a list of names (separated by `;`, `:` or `,`, each kept in lower case, trimmed)
//! and the script function that carries it out. A name already known, or one with a space in it,
//! is refused.
//!
//! **A line typed.** Trimmed, a line starting with `/` is a command (the `/`s cut off); any other
//! line is said: the default command's name (`say`) and a space put before it. The command's line
//! splits into words at spaces, a run in double quotes one word without its quotes (a quote not
//! closed fails the line); the first word, in lower case, names the command; the rest of the line
//! is what follows it, the first word's letters and then the spaces cut from its front. The
//! command's function is called on the player with every word (the command's first) and the rest.
//! A name no command has is an unknown command.
//!
//! **Aliases.** Before it is carried out, a line is expanded when it is expandable: a line with no
//! command mark is; a command's line is when its command was made known as expandable, or names
//! no command (a line whose command's name is empty is not). Expanding replaces each `;` and the
//! letters and digits after it (`;name`) with what the player's scripts say the alias stands
//! for, itself expanded; an alias already being expanded, or one the scripts do not know, stays
//! as it is. The line kept among the lines typed is the line as typed.
//!
//! **A line typed inside a call.** A script may hand its object's command line a line (a toolbar
//! slot holding a line of text does): the line is taken exactly as a typed one, its command's
//! function called on that object within the call.
//!
//! **The help.** The scripts make each command's help known as they make the command known: names
//! and the script function that answers with its text. Asked for a command's help (`/help <name>`,
//! the name in lower case), its help function is called on the interface asking, and its answer
//! shown as help text; a name the help does not know shows the unrecognized-command line, naming
//! it; a function that answers no text shows nothing. Asked with no name, every help function is
//! asked for its line, and each answer `  name -- text` is put after the help list's label, in the
//! names' order.

use std::collections::BTreeMap;

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::{tag, FunctionId, NativeId, NULL};
use auberean_script::Vm;

use crate::host::{call_on_process, ClientHost, InterfaceArg, BUDGET};
use crate::interface::function;
use crate::CallResult;

/// How a line typed went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellStatus {
    /// A command carried it out.
    Success,
    /// It could not be read (a quote not closed), or nothing carried it out.
    Failure,
    /// Nothing but spaces.
    EmptyCommand,
    /// No command has its name.
    UnknownCommand,
}

impl ShellStatus {
    /// The number the scripts are given.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            Self::Success => 1,
            Self::Failure => 2,
            Self::UnknownCommand => 3,
            Self::EmptyCommand => 5,
        }
    }
}

/// The lines typed, newest first, and where stepping stands (-1: before the newest).
// D2-07664, D2-07667
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    lines: Vec<String>,
    at: i64,
    max: usize,
}

impl Default for History {
    /// An empty history meant to keep 20 lines.
    // D2-07666
    fn default() -> Self {
        let mut h = Self {
            lines: Vec::new(),
            at: 0,
            max: 20,
        };
        h.resize();
        h.at = if h.lines.is_empty() { -1 } else { 0 };
        h
    }
}

impl History {
    /// The lines kept, newest first.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// A line typed goes to the front (an earlier copy taken out), stepping starts again before
    /// it. An empty line is not kept. Returns whether it was kept.
    // D2-07665
    pub fn add(&mut self, line: &str) -> bool {
        if line.is_empty() || self.max == 0 {
            return false;
        }
        self.at = -1;
        self.lines.retain(|l| l != line);
        self.lines.insert(0, line.to_owned());
        self.resize();
        true
    }

    /// The trim the client makes: only while the list is at least as long as it may be, and then
    /// only while it is shorter, so never.
    // D2-07663
    fn resize(&mut self) {
        if self.lines.len() < self.max {
            return;
        }
        while self.max > self.lines.len() {
            self.lines.pop();
        }
    }

    /// One step older (`older`) or newer: the line there, if any.
    // D2-07659
    fn step(&mut self, older: bool) -> Option<String> {
        let count = i64::try_from(self.lines.len()).unwrap_or(i64::MAX);
        let at = (self.at + if older { 1 } else { -1 }).clamp(-1, count - 1);
        self.at = at;
        usize::try_from(at)
            .ok()
            .and_then(|i| self.lines.get(i))
            .cloned()
    }

    /// One line newer.
    // D2-07661
    pub fn newer(&mut self) -> Option<String> {
        self.step(false)
    }

    /// One line older.
    // D2-07662
    pub fn older(&mut self) -> Option<String> {
        self.step(true)
    }
}

/// The commands the player's scripts made known: each name to the function that carries it out
/// and whether a line naming it is expanded; the names the help knows. The player's own table,
/// made empty with the player's shell and dropped with it.
// D2-07679, D2-07678, D2-12937, D2-12933
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Commands {
    pub names: BTreeMap<String, u32>,
    pub expandable: BTreeMap<String, u32>,
    pub help: BTreeMap<String, u32>,
}

/// What separates a command's names.
const SEPARATORS: &[char] = &[';', ':', ','];

/// The mark that starts a command: the player's shell is made with it.
// D2-07672, D2-07671
pub const PREFIX: &str = "/";

impl Commands {
    /// Make the names of a list known for a function, a line naming them expanded or not;
    /// whether every name was taken.
    // D2-08097, D2-12934
    pub fn add(&mut self, names: &str, function: u32, expandable: u32) -> bool {
        let mut all = true;
        for name in split_names(names) {
            if name.contains(' ') || self.names.contains_key(&name) {
                all = false;
                continue;
            }
            self.expandable.insert(name.clone(), expandable);
            self.names.insert(name, function);
        }
        all
    }

    /// Whether a line naming a command is expanded: as the command was made known; a name no
    /// command has, yes.
    // D2-12926
    #[must_use]
    pub fn expandable_command(&self, name: &str) -> bool {
        self.expandable
            .get(&name.to_lowercase())
            .is_none_or(|e| *e != 0)
    }

    /// Make the names of a list known to the help.
    // D2-08098, D2-12935
    pub fn add_help(&mut self, names: &str, function: u32) -> bool {
        let mut all = true;
        for name in split_names(names) {
            if name.contains(' ') || self.help.contains_key(&name) {
                all = false;
                continue;
            }
            self.help.insert(name, function);
        }
        all
    }
}

/// A list's names: split at the separators, lower case, trimmed, the empty ones dropped.
fn split_names(names: &str) -> Vec<String> {
    names
        .split(SEPARATORS)
        .map(|n| n.to_lowercase().trim().to_owned())
        .filter(|n| !n.is_empty())
        .collect()
}

/// A command's line read: its words (the command's first), or `None` when a quote is not closed.
// D2-12924
#[must_use]
pub fn words(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        let word;
        if c == '"' {
            let inner = &rest[1..];
            let end = inner.find('"')?;
            word = inner[..end].trim_matches('"').trim();
            rest = &inner[end + 1..];
        } else {
            let end = rest.find(' ').unwrap_or(rest.len());
            word = rest[..end].trim();
            rest = &rest[end..];
            rest = rest.strip_prefix(' ').unwrap_or(rest);
        }
        if !word.is_empty() {
            out.push(word.to_owned());
        }
    }
    Some(out)
}

/// What follows a command's name: the line with the name's letters and then the spaces cut from
/// its front.
// D2-12924
#[must_use]
pub fn rest_of(line: &str, command: &str) -> String {
    line.trim_start_matches(|c: char| command.contains(c))
        .trim()
        .to_owned()
}

/// A line typed, as the player's command line reads it: the command's line (the mark cut off, or
/// the default command put before a line that is said), or the status of an empty line.
// D2-07677, D2-07675
pub fn command_line(line: &str, default_command: &str) -> Result<String, ShellStatus> {
    if line.is_empty() {
        return Err(ShellStatus::Failure);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ShellStatus::EmptyCommand);
    }
    if trimmed.starts_with(PREFIX) {
        let rest = trimmed.trim_start_matches(PREFIX);
        if rest.is_empty() {
            return Err(ShellStatus::EmptyCommand);
        }
        return Ok(rest.to_owned());
    }
    Ok(format!("{default_command} {trimmed}"))
}

/// The default command's name in the string tables: its token and the table's enumeration value
/// (the say command's).
pub(crate) const DEFAULT_COMMAND: (u32, u32) = (0x8852_3C2E, 0x4000_0009);

/// The interface's function asked what an alias stands for.
const INQ_ALIAS: u32 = (0xB4 << 16) | 0x6E;

/// Whether a line typed is expanded before it is carried out: a line with no command mark is; a
/// command's line is when the name after the first mark is not empty and its command, if any, was
/// made known as expandable.
// D2-07674
#[must_use]
pub fn expandable_text(line: &str, commands: &Commands) -> bool {
    let Some(at) = line.find(PREFIX) else {
        return true;
    };
    let rest = &line[at + PREFIX.len()..];
    let name = &rest[..rest.find(' ').unwrap_or(rest.len())];
    !name.is_empty() && commands.expandable_command(name)
}

/// Whether a unit continues an alias's name: a letter or a digit.
fn in_name(u: u16) -> bool {
    char::from_u32(u32::from(u)).is_some_and(char::is_alphanumeric)
}

/// A line typed, taken inside the scripts: the machine, the host, and the process of the object
/// whose command line takes it (none: nothing of the scripts is called).
pub(crate) struct Typing<'a> {
    pub vm: &'a mut Vm,
    pub host: &'a mut ClientHost,
    pub process: Option<usize>,
    /// Inside a call: a run the machine stops stops the call too. Otherwise each call's end is
    /// kept and the line goes on.
    pub nested: bool,
    /// Each interface function called, and how it went.
    pub asked: Vec<(u32, CallResult)>,
}

impl Typing<'_> {
    /// A line typed: kept among the lines typed; expanded when expandable; carried out by the
    /// command line; then the interface is told the line is done. An empty line fails at once.
    // D2-08090
    pub fn process_text(&mut self, text: &str) -> Result<ShellStatus, Stop> {
        if text.is_empty() {
            return Ok(ShellStatus::Failure);
        }
        self.host.history.add(text);
        let line = if expandable_text(text, &self.host.commands) {
            let units: Vec<u16> = text.encode_utf16().collect();
            String::from_utf16_lossy(&self.expand(&units, &mut Vec::new())?)
        } else {
            text.to_owned()
        };
        let status = self.carry_out(&line)?;
        self.done()?;
        Ok(status)
    }

    /// How a call went, kept; a stop ends the line only inside a call.
    fn record<T>(
        &mut self,
        function: u32,
        outcome: Result<(Status, T), Stop>,
    ) -> Result<Option<T>, Stop> {
        let how = crate::host::how(outcome.as_ref().map(|(s, _)| *s).map_err(Clone::clone));
        self.asked.push((function, how));
        match outcome {
            Ok((_, v)) => Ok(Some(v)),
            Err(e) if self.nested => Err(e),
            Err(_) => Ok(None),
        }
    }

    /// One of the interface's functions called on the object.
    fn call(&mut self, function: u32, args: &[InterfaceArg]) -> Result<(), Stop> {
        let Some(p) = self.process else {
            self.asked.push((function, CallResult::NoReceiver));
            return Ok(());
        };
        if !self.nested {
            self.vm.budget = BUDGET;
        }
        let outcome = call_on_process(self.vm, self.host, p, function, args);
        self.record(function, outcome)?;
        Ok(())
    }

    /// A line's aliases expanded: from the first `;` on, each `;` and the letters and digits after
    /// it replaced by the alias's expansion, the search going on after what replaced it.
    // D2-00709, D2-08081
    fn expand(&mut self, text: &[u16], stack: &mut Vec<Vec<u16>>) -> Result<Vec<u16>, Stop> {
        const MARK: u16 = b';' as u16;
        let find = |s: &[u16], from: usize| {
            s.get(from..)
                .and_then(|t| t.iter().position(|u| *u == MARK))
                .map(|i| i + from)
        };
        let mut out = text.to_vec();
        let Some(mut at) = find(&out, 0) else {
            return Ok(out);
        };
        loop {
            let mut end = at;
            loop {
                end = (end + 1).min(out.len());
                if !out.get(end).is_some_and(|u| *u != 0 && in_name(*u)) {
                    break;
                }
            }
            let name = out[at..end].to_vec();
            let with = self.handle_alias(&name, stack)?;
            let next = at + with.len();
            out.splice(at..end, with);
            match find(&out, next) {
                Some(i) => at = i,
                None => return Ok(out),
            }
        }
    }

    /// One alias's expansion: the alias itself while it is being expanded already; else what
    /// the scripts say it stands for (the alias itself when they say nothing), expanded in turn.
    // D2-00703
    fn handle_alias(&mut self, name: &[u16], stack: &mut Vec<Vec<u16>>) -> Result<Vec<u16>, Stop> {
        if stack.iter().any(|n| n == name) {
            return Ok(name.to_vec());
        }
        stack.push(name.to_vec());
        let text = self.inq_alias(name)?;
        let out = self.expand(&text, stack)?;
        stack.pop();
        Ok(out)
    }

    /// What an alias stands for, asked of the object's scripts: a new string holding the alias in
    /// a slot, the result slot, a reference to the first slot, a new string holding the alias,
    /// the instance; the answer is the string the first slot names after the call (the context is
    /// let go of however the call ended).
    // D2-01158
    fn inq_alias(&mut self, name: &[u16]) -> Result<Vec<u16>, Stop> {
        let Some(p) = self.process else {
            return Ok(name.to_vec());
        };
        if !self.nested {
            self.vm.budget = BUDGET;
        }
        let outcome = inq_alias_call(self.vm, self.host, p, name);
        Ok(self
            .record(INQ_ALIAS, outcome)?
            .unwrap_or_else(|| name.to_vec()))
    }

    /// The line carried out by the command line: the command's line (the mark cut off, or the
    /// default command's name put before a line that is said), then the command.
    // D2-07677, D2-07675
    fn carry_out(&mut self, text: &str) -> Result<ShellStatus, Stop> {
        let h = &mut *self.host;
        let table = h.text.data_id_by_enum(DEFAULT_COMMAND.1, TABLE_MAPPING);
        let default = auberean_text::StringInfo::entry(table, DEFAULT_COMMAND.0).text(&mut h.text);
        match command_line(text, &default) {
            Ok(line) => self.process_command(&line),
            Err(status) => Ok(status),
        }
    }

    /// A command's line carried out: its words read, its first naming the command; the
    /// command's function called on the object with every word and the rest of the line (the
    /// command carried it out whatever the call did; with no process to call, it failed).
    // D2-07669, D2-12924, D2-08095, D2-08091
    fn process_command(&mut self, line: &str) -> Result<ShellStatus, Stop> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(ShellStatus::EmptyCommand);
        }
        let Some(words) = words(line) else {
            return Ok(ShellStatus::Failure);
        };
        let Some(command) = words.first() else {
            return Ok(ShellStatus::EmptyCommand);
        };
        let rest = rest_of(line, command);
        let Some(function) = self
            .host
            .commands
            .names
            .get(&command.to_lowercase())
            .copied()
        else {
            return Ok(ShellStatus::UnknownCommand);
        };
        if self.process.is_none() {
            return Ok(ShellStatus::Failure);
        }
        let words = words.iter().map(|w| w.encode_utf16().collect()).collect();
        let args = [
            InterfaceArg::Texts(words),
            InterfaceArg::Text(rest.encode_utf16().collect()),
        ];
        self.call(function, &args)?;
        Ok(ShellStatus::Success)
    }

    /// The help for command `name`, or for every command when `name` is empty, shown: the help's
    /// text as help text (kind 8) when it has any; for a name the help does not know, the
    /// unrecognized-command line naming it (kind 1).
    // D2-08083, D2-08072, D2-04674
    pub fn display_help_text(&mut self, name: &str) -> Result<(), Stop> {
        let (status, text) = self.help_text(name)?;
        let shown = match status {
            ShellStatus::UnknownCommand => {
                let h = &mut *self.host;
                let table = h.text.data_id_by_enum(HELP_TABLE, TABLE_MAPPING);
                let mut info = auberean_text::StringInfo::entry(table, token(UNRECOGNIZED));
                let name: Vec<u16> = name.encode_utf16().collect();
                info.add_variable_string(token(COMMAND_VARIABLE), &name);
                Some((
                    UNRECOGNIZED_KIND,
                    info.text(&mut h.text).encode_utf16().collect(),
                ))
            }
            ShellStatus::Success if !text.is_empty() => Some((HELP_KIND, text)),
            _ => None,
        };
        if let Some((kind, text)) = shown {
            let args = [InterfaceArg::Text(text), InterfaceArg::Word(kind)];
            self.call(crate::latent::function::DISPLAY_TEXT, &args)?;
        }
        Ok(())
    }

    /// The help for command `name`: every command's when `name` is empty, after the help list's
    /// label.
    // D2-07673, D2-12936
    fn help_text(&mut self, name: &str) -> Result<(ShellStatus, Vec<u16>), Stop> {
        if !name.is_empty() {
            return self.command_help_text(name);
        }
        let list = self.all_help_text()?;
        let h = &mut *self.host;
        let table = h.text.data_id_by_enum(HELP_TABLE, TABLE_MAPPING);
        let label =
            auberean_text::StringInfo::entry(table, token(HELP_LIST_LABEL)).text(&mut h.text);
        let mut text: Vec<u16> = label.encode_utf16().collect();
        text.extend(list);
        Ok((ShellStatus::Success, text))
    }

    /// Every command's help line, `  name -- text`, in the names' order: each help function asked
    /// for its line, those that answer none left out.
    // D2-12922
    fn all_help_text(&mut self) -> Result<Vec<u16>, Stop> {
        let mut out = Vec::new();
        for (name, function) in self.host.commands.help.clone() {
            if let (ShellStatus::Success, text) = self.do_help(function, 1)? {
                out.extend(format!("  {name} -- ").encode_utf16());
                out.extend(text);
            }
        }
        Ok(out)
    }

    /// One command's help, its name in lower case: a name the help does not know is an unknown
    /// command.
    // D2-12925
    fn command_help_text(&mut self, name: &str) -> Result<(ShellStatus, Vec<u16>), Stop> {
        match self.host.commands.help.get(&name.to_lowercase()).copied() {
            Some(function) => self.do_help(function, 0),
            None => Ok((ShellStatus::UnknownCommand, Vec::new())),
        }
    }

    /// A help function asked for its text (`all`: 1 for the list's line, 0 for the command's own
    /// help) on the interface: the text it answers, or a failure when it answers no string or
    /// there is no interface to ask.
    // D2-08096
    fn do_help(&mut self, function: u32, all: u32) -> Result<(ShellStatus, Vec<u16>), Stop> {
        let Some(p) = self.process else {
            self.asked.push((function, CallResult::NoReceiver));
            return Ok((ShellStatus::Failure, Vec::new()));
        };
        if !self.nested {
            self.vm.budget = BUDGET;
        }
        let outcome = help_call(self.vm, self.host, p, function, all);
        Ok(match self.record(function, outcome)?.flatten() {
            Some(text) => (ShellStatus::Success, text),
            None => (ShellStatus::Failure, Vec::new()),
        })
    }

    /// The interface told the line is done: the tells' histories start again from their newest.
    // D2-00639, D2-01229, D2-01228
    fn done(&mut self) -> Result<(), Stop> {
        self.call(function::RESET_RETELL_HISTORY_INDEX, &[])?;
        self.call(function::RESET_REPLY_HISTORY_INDEX, &[])
    }
}

/// A help function called on a process's instance (see [`Typing::do_help`]): the result slot, the
/// kind of help asked, the instance; the answer is the string the result slot names after the
/// call, if it names one.
fn help_call(
    vm: &mut Vm,
    host: &mut ClientHost,
    p: usize,
    function: u32,
    all: u32,
) -> Result<(Status, Option<Vec<u16>>), Stop> {
    let mut cx = vm.new_context(p);
    cx.push(0, tag::INT)?;
    cx.push(all, tag::INT)?;
    let receiver = vm.instance_of(p);
    cx.push(receiver, tag::REFERENCE)?;
    let prog = std::sync::Arc::clone(&host.program);
    let package = vm.package_of(host, receiver, &prog)?;
    let status = vm.call(host, &mut cx, package, FunctionId(function))?;
    let (answer, _) = cx.slot(0)?;
    let text = match vm.heap.native(answer) {
        Ok(NativeObject::Text(t)) if answer != NULL => Some(t.clone()),
        _ => None,
    };
    Ok((status, text))
}

/// The interface's alias question put to a process's instance (see [`Typing::inq_alias`]):
/// how the call ended and the string the first slot names after it.
fn inq_alias_call(
    vm: &mut Vm,
    host: &mut ClientHost,
    p: usize,
    name: &[u16],
) -> Result<(Status, Vec<u16>), Stop> {
    let mut cx = vm.new_context(p);
    let held = vm
        .heap
        .add_native(native_type::WP_STRING, NativeObject::Text(name.to_vec()));
    cx.push(held, tag::REFERENCE)?;
    cx.push(0, tag::INT)?;
    let first = vm.heap.add_relative(NULL, 0);
    cx.push(first, tag::REFERENCE)?;
    let asked = vm
        .heap
        .add_native(native_type::WP_STRING, NativeObject::Text(name.to_vec()));
    cx.push(asked, tag::REFERENCE)?;
    let receiver = vm.instance_of(p);
    cx.push(receiver, tag::REFERENCE)?;
    let prog = std::sync::Arc::clone(&host.program);
    let package = vm.package_of(host, receiver, &prog)?;
    let status = vm.call(host, &mut cx, package, FunctionId(INQ_ALIAS))?;
    let (now, _) = cx.slot(0)?;
    let text = match vm.heap.native(now) {
        Ok(NativeObject::Text(t)) => t.clone(),
        _ => name.to_vec(),
    };
    Ok((status, text))
}

/// The natives that make the commands known, by id: a list of names for a function (by a string,
/// or by a string table's token), and the same for the help; each answers whether every name was
/// taken. And a line handed to an object's command line from inside a call.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    [
        (0x3D_u32, add_shell_cmd as NativeFn),
        (0x3C, add_shell_cmd_by_string_id),
        (0x3F, add_shell_help),
        (0x3E, add_shell_help_by_string_id),
        (0x52, process_text),
        (0x53, display_help_text),
    ]
    .into_iter()
    .map(|(n, f)| (NativeId::new(INTERFACE, n), f))
}

const INTERFACE: u32 = 0x7F;

/// The string table mapping's number.
const TABLE_MAPPING: u32 = 0x4000_0004;

/// The table the help's own lines are in (by its enumeration value), their names, and the
/// unrecognized-command line's variable naming the command.
const HELP_TABLE: u32 = 0x4000_0009;
const HELP_LIST_LABEL: &str = "ID_HelpListLabel";
const UNRECOGNIZED: &str = "ID_UnrecognizedCommand";
const COMMAND_VARIABLE: &str = "CMD";
/// The kinds of text the help shows: its text, and the unrecognized-command line.
const HELP_KIND: u32 = 8;
const UNRECOGNIZED_KIND: u32 = 1;

/// A string's token.
fn token(name: &str) -> u32 {
    auberean_assets::strings::token(name)
}

fn host<'a>(c: &'a mut Call<'_>) -> Result<&'a mut crate::host::ClientHost, Stop> {
    c.host
        .as_any()
        .downcast_mut::<crate::host::ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))
}

/// A string argument's text.
fn text_at(c: &Call<'_>, k: i64) -> Result<String, Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match c.vm.heap.native(r)? {
        NativeObject::Text(t) => Ok(String::from_utf16_lossy(t)),
        _ => Err(c.fail("not a string")),
    }
}

/// A string table's entry by its token, the table named by an enumeration value.
fn table_text(c: &mut Call<'_>, token: u32, table_enum: u32) -> Result<String, Stop> {
    let h = host(c)?;
    let table = h.text.data_id_by_enum(table_enum, TABLE_MAPPING);
    Ok(auberean_text::StringInfo::entry(table, token).text(&mut h.text))
}

/// Make a list of names known for a function: `fp[-2]` the names, `fp[-3]` the function,
/// `fp[-4]` whether a line naming them is expanded; answers at `fp[-5]`.
// D2-04661, D2-08097
fn add_shell_cmd(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let names = text_at(c, -2)?;
    let (function, expandable) = (c.cx.at(-3)?, c.cx.at(-4)?);
    let ok = host(c)?.commands.add(&names, function, expandable);
    c.write(-5, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// The same by a string table's token: `fp[-2]` the token, `fp[-3]` the table's enumeration
/// value, `fp[-4]` the function, `fp[-5]` whether expanded; answers at `fp[-6]`.
// D2-04662, D2-08099
fn add_shell_cmd_by_string_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let (token, table_enum, function) = (c.cx.at(-2)?, c.cx.at(-3)?, c.cx.at(-4)?);
    let expandable = c.cx.at(-5)?;
    let names = table_text(c, token, table_enum)?;
    let ok = host(c)?.commands.add(&names, function, expandable);
    c.write(-6, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Make a list of names known to the help: `fp[-2]` the names, `fp[-3]` the function; answers at
/// `fp[-4]`.
// D2-04663, D2-08098
fn add_shell_help(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let names = text_at(c, -2)?;
    let function = c.cx.at(-3)?;
    let ok = host(c)?.commands.add_help(&names, function);
    c.write(-4, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// The same by a string table's token: `fp[-2]` the token, `fp[-3]` the table's enumeration
/// value, `fp[-4]` the function; answers at `fp[-5]`.
// D2-04664, D2-08100
fn add_shell_help_by_string_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let (token, table_enum, function) = (c.cx.at(-2)?, c.cx.at(-3)?, c.cx.at(-4)?);
    let names = table_text(c, token, table_enum)?;
    let ok = host(c)?.commands.add_help(&names, function);
    c.write(-5, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// A line handed to the receiver's command line from inside a call (`fp[-2]`): taken as a typed
/// line, its command carried out on the receiver within the call; how it went at `fp[-3]`.
// D2-04668
fn process_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let text = text_at(c, -2)?;
    let process = c.vm.process_with_instance(r);
    let h = c
        .host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))?;
    let mut t = Typing {
        vm: &mut *c.vm,
        host: h,
        process,
        nested: true,
        asked: Vec::new(),
    };
    let status = t.process_text(&text)?;
    c.write(-3, status.code(), tag::MIXED)?;
    Ok(Status::Success)
}

/// The help for a command shown on the receiving interface (`fp[-2]` the command's name; an empty
/// name: every command's), within the call. A null receiver crashes the call.
// D2-04659
fn display_help_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let name = text_at(c, -2)?;
    let process = c.vm.process_with_instance(r);
    let h = c
        .host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))?;
    let mut t = Typing {
        vm: &mut *c.vm,
        host: h,
        process,
        nested: true,
        asked: Vec::new(),
    };
    t.display_help_text(&name)?;
    Ok(Status::Success)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the command line's own bookkeeping; the chat replay takes lines through it)
    use super::*;

    #[test]
    fn lines_step_back_and_on_newest_first_and_a_repeat_moves_to_the_front() {
        let mut h = History::default();
        assert_eq!(h.older(), None);
        for l in ["one", "two", "three", "two"] {
            h.add(l);
        }
        assert_eq!(h.lines(), ["two", "three", "one"]);
        assert_eq!(h.older().as_deref(), Some("two"));
        assert_eq!(h.older().as_deref(), Some("three"));
        assert_eq!(h.older().as_deref(), Some("one"));
        assert_eq!(h.older().as_deref(), Some("one"), "the oldest stays");
        assert_eq!(h.newer().as_deref(), Some("three"));
        assert_eq!(h.newer().as_deref(), Some("two"));
        assert_eq!(h.newer(), None, "past the newest, nothing");
        for i in 0..30 {
            h.add(&i.to_string());
        }
        assert_eq!(h.lines().len(), 33, "never trimmed");
    }

    #[test]
    fn a_line_is_a_command_or_said_and_splits_into_words() {
        assert_eq!(
            command_line("  hi there ", "say"),
            Ok("say hi there".into())
        );
        assert_eq!(command_line("//e wave", "say"), Ok("e wave".into()));
        assert_eq!(command_line("   ", "say"), Err(ShellStatus::EmptyCommand));
        assert_eq!(command_line("/", "say"), Err(ShellStatus::EmptyCommand));
        assert_eq!(
            words("tell \"Bo Ann\" hi  there"),
            Some(vec![
                "tell".to_owned(),
                "Bo Ann".into(),
                "hi".into(),
                "there".into()
            ])
        );
        assert_eq!(words("tell \"Bo"), None, "a quote not closed");
        assert_eq!(rest_of("say say it", "say"), "say it");
        let mut c = Commands::default();
        assert!(c.add("Say; S, :", 7, 1));
        assert!(!c.add("s; tell me", 8, 0), "taken, and a space");
        assert!(c.add("alias", 9, 0));
        assert!(!c.expandable_command("ALIAS") && c.expandable_command("say"));
        assert!(c.expandable_command("nothing"), "no command: expanded");
        assert!(expandable_text("hi ;a", &c));
        assert!(!expandable_text("/alias ;a hi", &c));
        assert!(!expandable_text("/ say", &c), "a command's name empty");
        assert!(expandable_text("/s ;a", &c));
        assert_eq!(c.names.get("say"), Some(&7));
        assert_eq!(c.names.get("s"), Some(&7));
    }
}
