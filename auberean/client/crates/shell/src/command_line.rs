//! The command line as the September 2013 client reads it: its options, how a line is split into
//! words, how each word is taken, what each option keeps, and the usage text shown when the line
//! does not read.
//!
//! **Options.** Each has a long name and may have a one-letter name; a word starting with `-` or
//! `/` names one: doubled (`--host`, `//host`) the long name, a single letter after it (`-h`) the
//! letter, otherwise the long name again (`-host`). An option is a switch (no value), takes the
//! next word as its value (even one that looks like an option), or takes it only when it is not
//! an option. An option's value is kept as its form says: a number (decimal, `0x` hexadecimal or
//! `0` octal), a text, a port, or a switch that sets a number. A word that is no option and
//! follows none is refused unless the table has an option with no names for such words.
//!
//! **Errors and usage.** An option the table lacks, a word with nothing to take it, or a value
//! missing at the end fails the line with a message; `-?` fails it with none. On failure the
//! message and then every option's usage line are written out: the options sorted by long name
//! (letter case ignored), each `-x, --name` padded to 20 columns, then `: ` and its help, wrapped
//! at 80 columns with the wrapped lines indented 22.

use core::fmt::Write as _;

use auberean_dat::tree::number;

/// What an option takes: the low four bits of its form.
pub mod takes {
    /// Nothing: a switch.
    pub const NOTHING: u32 = 1;
    /// The next word, whatever it is.
    pub const VALUE: u32 = 2;
    /// The next word when it is not an option.
    pub const MAYBE: u32 = 3;
}

/// How an option keeps what it takes: its form's high bits.
pub mod keeps {
    /// A switch setting a number to the option's own value.
    pub const SET_NUMBER: u32 = 0x10;
    /// A whole number.
    pub const NUMBER: u32 = 0x20;
    /// A text.
    pub const TEXT: u32 = 0x30;
    /// A single-precision number.
    pub const FLOAT: u32 = 0x40;
    /// A double-precision number.
    pub const DOUBLE: u32 = 0x50;
    /// An unsigned 16-bit number (a port).
    pub const PORT: u32 = 0x60;
    /// A switch setting a yes-or-no to whether the option's own value is not zero.
    pub const SET_FLAG: u32 = 0x70;
    /// A whole number kept as a yes-or-no.
    pub const NUMBER_FLAG: u32 = 0x80;
}

/// An option: its form (what it takes and how it keeps it), its letter (0: none), its long name,
/// its help, and the value a switch sets.
// D2-17755, D2-17754
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arg {
    pub form: u32,
    pub letter: u8,
    pub name: String,
    pub help: String,
    pub value: i32,
}

/// What an option keeps from its value.
#[derive(Clone, Debug, PartialEq)]
pub enum Kept {
    /// Nothing (the client reads the option by its name).
    Nothing,
    Number(i32),
    Text(String),
    Float(f32),
    Double(f64),
    Port(u16),
    Flag(bool),
}

/// The scanned prefix of a number as `sscanf` reads one: white space, then the number; whether
/// any digit was read.
fn scan_float(text: &str) -> Option<f64> {
    let s = number::strtod(text.as_bytes());
    let lead = text.bytes().take_while(u8::is_ascii_whitespace).count();
    (s.end > lead).then_some(s.value)
}

fn scan_int(text: &str) -> Option<i64> {
    let b = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (neg, digits) = match b.as_bytes().first() {
        Some(b'-') => (true, &b[1..]),
        Some(b'+') => (false, &b[1..]),
        _ => (false, b),
    };
    let n = digits.bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return None;
    }
    let v = digits[..n].bytes().fold(0i64, |a, d| {
        a.wrapping_mul(10).wrapping_add(i64::from(d - b'0'))
    });
    Some(if neg { v.wrapping_neg() } else { v })
}

impl Arg {
    /// What the option keeps from a value, by its form; none when the value does not scan.
    // D2-17756
    #[must_use]
    pub fn keep(&self, value: &str) -> Option<Kept> {
        Some(match self.form {
            0x11 => Kept::Number(self.value),
            0x22 => Kept::Number(number::strtol(value.as_bytes()).value),
            0x32 => Kept::Text(value.to_owned()),
            #[allow(clippy::cast_possible_truncation)] // read in single precision
            0x42 => Kept::Float(scan_float(value)? as f32),
            0x52 => Kept::Double(scan_float(value)?),
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // `%hu`
            0x62 => Kept::Port(scan_int(value)? as u16),
            0x71 => Kept::Flag(self.value != 0),
            0x82 => Kept::Flag(scan_int(value)? != 0),
            _ => Kept::Nothing,
        })
    }

    /// What it takes.
    #[must_use]
    pub const fn takes(&self) -> u32 {
        self.form & 0xF
    }
}

/// The options, in the order added, with their letters and names.
// D2-17773
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArgList {
    pub args: Vec<Arg>,
    letters: Vec<(u8, usize)>,
    names: Vec<(String, usize)>,
}

impl ArgList {
    /// Add an option, unless its letter or its long name is taken; an option with neither takes
    /// the words no option takes.
    // D2-17772
    pub fn add(&mut self, arg: Arg) -> bool {
        if arg.letter != 0 && self.letter(arg.letter).is_some() {
            return false;
        }
        if self.name(&arg.name).is_some() {
            return false;
        }
        let i = self.args.len();
        if arg.letter != 0 {
            self.letters.push((arg.letter, i));
        }
        if !arg.name.is_empty() || arg.letter == 0 {
            self.names.push((arg.name.clone(), i));
        }
        self.args.push(arg);
        true
    }

    /// Add an option from its parts.
    // D2-00083
    pub fn add_cmd(&mut self, form: u32, letter: u8, name: &str, help: &str, value: i32) -> bool {
        self.add(Arg {
            form,
            letter,
            name: name.to_owned(),
            help: help.to_owned(),
            value,
        })
    }

    /// The option a letter names.
    #[must_use]
    pub fn letter(&self, c: u8) -> Option<usize> {
        self.letters.iter().find(|(l, _)| *l == c).map(|(_, i)| *i)
    }

    /// The option a long name names (letter case counts).
    #[must_use]
    pub fn name(&self, n: &str) -> Option<usize> {
        self.names.iter().find(|(m, _)| m == n).map(|(_, i)| *i)
    }
}

/// The parts of reading a command line a program fills in: its options, what each does, the check
/// once every word is read, and where its messages go.
pub trait Reader {
    /// Add the program's options.
    fn build(&mut self, list: &mut ArgList);

    /// An option and its value (empty for a switch) taken: whether it was.
    fn evaluate(&mut self, arg: &Arg, value: &str) -> bool;

    /// Every word read: whether the line is good.
    // D2-00072
    fn done(&mut self) -> bool {
        true
    }

    /// The message saying why the line failed.
    fn error_text(&mut self) -> &mut String;

    /// Text written out.
    fn output(&mut self, text: &str);

    /// The written text finished with.
    fn finish_output(&mut self) {}

    /// The column the usage text wraps at (none when negative).
    // D2-00071
    fn wrap_width(&self) -> i32 {
        80
    }

    /// Whether a word names an option: it starts with `-` or `/`.
    // D2-17757
    fn is_argument(&self, word: &str) -> bool {
        word.starts_with('-') || word.starts_with('/')
    }
}

/// Set the message saying why the line failed.
// D2-01333
pub fn set_error_text(r: &mut dyn Reader, text: &str) {
    text.clone_into(r.error_text());
}

/// Two options in the usage text's order: by long name, letter case ignored.
// D2-17751
#[must_use]
pub fn usage_order(a: &Arg, b: &Arg) -> core::cmp::Ordering {
    a.name
        .to_ascii_lowercase()
        .cmp(&b.name.to_ascii_lowercase())
}

/// Write `text` out wrapped at the reader's width, a line break (and a wrapped word) starting a
/// new line indented `indent` columns.
// D2-17764
pub fn append_and_word_wrap(r: &mut dyn Reader, text: &str, indent: usize) {
    let width = usize::try_from(r.wrap_width()).unwrap_or(usize::MAX);
    let mut break_line = String::from("\n");
    break_line.extend(core::iter::repeat_n(' ', indent));
    let bytes = text.as_bytes();
    let mut column = 0usize;
    let mut word: Option<usize> = None;
    let mut i = 0;
    loop {
        let c = bytes.get(i).copied().unwrap_or(0);
        if c == 0 || b" \t\n".contains(&c) {
            if let Some(start) = word.take() {
                if width <= column + (i - start) {
                    r.output(&break_line);
                    column = indent;
                }
                column += i - start;
                r.output(&text[start..i]);
            }
            match c {
                b'\n' => {
                    r.output(&break_line);
                    column = indent;
                }
                0 => break,
                _ => {
                    if column < width.saturating_sub(1) {
                        r.output(&text[i..=i]);
                        column += 1;
                    }
                }
            }
        } else if word.is_none() {
            word = Some(i);
        }
        i += 1;
    }
}

/// Every option's usage line written out, sorted by long name.
// D2-17763
pub fn display_usage(r: &mut dyn Reader, list: &ArgList) {
    let mut sorted: Vec<&Arg> = list.args.iter().collect();
    sorted.sort_by(|a, b| usage_order(a, b));
    for a in sorted {
        let mut line = String::new();
        if a.letter == 0 {
            line.push_str("  ");
        } else {
            let _ = write!(line, "-{}", char::from(a.letter));
        }
        line.push_str(if a.letter != 0 && !a.name.is_empty() {
            ", "
        } else {
            "  "
        });
        if !a.name.is_empty() {
            let _ = write!(line, "--{}", a.name);
        }
        while line.len() < 0x14 {
            line.push(' ');
        }
        line.push_str(": ");
        line.push_str(&a.help);
        append_and_word_wrap(r, &line, 0x16);
        r.output("\n");
    }
}

/// The line failed, saying why.
fn fail(r: &mut dyn Reader, ok: &mut bool, text: String) {
    set_error_text(r, &text);
    *ok = false;
}

/// Read a command line's words (the program's own name first when `skip_first`): each option and
/// its value taken by the reader, then the reader's check. A line that fails writes out why and
/// the usage text. Whether it read.
// D2-17774
pub fn parse_args(r: &mut dyn Reader, argv: &[&str], skip_first: bool) -> bool {
    let mut list = ArgList::default();
    r.build(&mut list);
    let mut ok = true;
    let mut pending: Option<usize> = None;
    let loose = list.name("");
    let mut cur = usize::from(skip_first);
    while cur <= argv.len() {
        let word = argv.get(cur).copied().unwrap_or("");
        let is_arg = r.is_argument(word);
        let mut take_value = !is_arg;
        if let Some(p) = pending {
            if is_arg {
                match list.args[p].takes() {
                    takes::MAYBE => {
                        ok &= r.evaluate(&list.args[p].clone(), "");
                        pending = None;
                        // The same word read again, now with nothing waiting.
                        continue;
                    }
                    takes::VALUE => take_value = true,
                    _ => {}
                }
            }
        }
        if take_value {
            let target = match pending {
                Some(p) => Some(p),
                None if word.is_empty() => {
                    cur += 1;
                    continue;
                }
                None => loose,
            };
            match target {
                None => fail(r, &mut ok, format!("Argument '{word}' unknown")),
                Some(p) if list.args[p].takes() == takes::VALUE && word.is_empty() => {
                    let named = argv.get(cur.wrapping_sub(1)).copied().unwrap_or("");
                    fail(r, &mut ok, format!("Argument {named} requires parameter"));
                }
                Some(p) => {
                    ok &= r.evaluate(&list.args[p].clone(), word);
                    pending = None;
                }
            }
            cur += 1;
            continue;
        }
        let prefix = word.as_bytes()[0];
        let rest = &word[1..];
        let found = if rest.as_bytes().first() == Some(&prefix) {
            list.name(&rest[1..])
        } else if rest.len() == 1 {
            list.letter(rest.as_bytes()[0]).or_else(|| list.name(rest))
        } else {
            list.name(rest)
        };
        pending = found;
        match found {
            Some(p) => {
                if list.args[p].takes() == takes::NOTHING {
                    ok &= r.evaluate(&list.args[p].clone(), "");
                    pending = None;
                }
            }
            None => {
                let name = if rest.as_bytes().first() == Some(&prefix) {
                    &rest[1..]
                } else {
                    rest
                };
                if name.starts_with('?') {
                    fail(r, &mut ok, String::new());
                } else {
                    fail(r, &mut ok, format!("Unrecognized argument {word}"));
                }
            }
        }
        cur += 1;
    }
    if ok {
        ok = r.done();
        if ok {
            return true;
        }
    }
    let error = r.error_text().clone();
    if !error.is_empty() {
        r.output(&error);
        r.output("\n\n");
    }
    display_usage(r, &list);
    r.finish_output();
    ok
}

/// A command line split into words: at spaces outside double quotes, the quotes dropped. None
/// when a quote is left open.
// D2-17765
#[must_use]
pub fn convert_command_line(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            ' ' if !quoted => {
                if let Some(w) = word.take() {
                    out.push(w);
                }
            }
            '"' => quoted = !quoted,
            _ => word.get_or_insert_with(String::new).push(c),
        }
    }
    if let Some(w) = word {
        out.push(w);
    }
    (!quoted).then_some(out)
}

/// Read a command line given as one text.
// D2-17766
pub fn parse_command_line(r: &mut dyn Reader, line: &str, skip_first: bool) -> bool {
    match convert_command_line(line) {
        Some(words) => {
            let w: Vec<&str> = words.iter().map(String::as_str).collect();
            parse_args(r, &w, skip_first)
        }
        None => {
            set_error_text(r, "Unmatched quote");
            false
        }
    }
}

/// The preferences' sections and keys the client's switches set for the run.
mod run {
    pub const RENDER_DEVICE: u32 = 0x4000_0002;
    pub const GRAPHICS: u32 = 0x4000_0004;
    pub const USE_STENCIL_BUFFER: u32 = 0x4000_0008;
    pub const FORCE_SOFTWARE_VERTICES: u32 = 0x4000_000C;
    pub const FORCE_FIXED_FUNCTION: u32 = 0x4000_000D;
    pub const TERRAIN_BLENDING: u32 = 0x4000_0014;
}

/// An option as a table row: its form, letter, long name and help, and the value a switch sets.
type Row = (u32, u8, &'static str, &'static str, i32);

/// The September 2013 client's options.
// D2-00100, D2-00101
const CLIENT_OPTIONS: [Row; 40] = [
    (0x02, b'a', "account", "<string>: Specifies the account name to logon with.", 0),
    (0x03, 0, "autonomy", "<int>: Specify the level of autonomy. 0,1 or 2.", 0),
    (0x02, b'c', "chargen", "<filename>: Creating a character saves data the stress clients need.", 0),
    (0x02, 0, "debug", "<32 bitfield>: Controls what kinds of debug outputs are enabled.", 0),
    (0x02, 0, "dropmip", "<0-10>: Drop specified number of mip levels.", 0),
    (0x02, 0, "frill", "<0-5>: Enable and set frill object radius.", 0),
    (0x11, 0, "fullscreen", "Forces fullscreen mode on debug builds", 0),
    (0x02, b'h', "host", "[host/IP]:Specifies where to find the server to talk to.", 0),
    (0x22, 0, "height", "<int>: Specify window height.", 0),
    (0x22, 0, "width", "<int> : Specify windows width.", 0),
    (0x11, 0, "imeon", "Actives the IME system for typing extended characters.", 1),
    (0x02, 0, "maxgore", "Forces the maximum gore level (fun,none,low,normal)", 0),
    (0x01, 0, "greenblood", "Forces the game to use green blood", 0),
    (0x01, 0, "ff", "Force fixed function vertex pipeline.", 0),
    (0x02, 0, "language", "<string>: Language to run the client in.", 0),
    (0x01, 0, "ls", "Activate landscape shadows.", 0),
    (0x01, 0, "lb", "Activate landscape terrain blending.", 0),
    (0x01, 0, "nolb", "Disable landscape terrain blending.", 0),
    (0x03, 0, "nodbwlib", "Tells the client not to use the database cache for locating the wlib.  Instead, the wlib will be loaded directly from the hard drive.", 0),
    (0x22, 0, "outport", "<1-65535>: Specify the outgoing network port to use.", 0),
    (0x22, b'p', "port", "<1-65535>: Specify the server port to contact. See 'host'", 0),
    (0x32, 0, "prefs", "<string>: Specify the preferences file to use.", 0),
    (0x32, b'z', "glsticketdirect", "<string>: Specify ticket data.", 0),
    (0x01, 0, "glsticket", "Load ticket data", 0),
    (0x02, 0, "farview", "<1-25>: Set the landscape farview radius.", 0),
    (0x01, 0, "refreshrate", "Turn off optimal refresh rate", 0),
    (0x01, 0, "regserver", "Stores a string value with the client's current version into the registry.", 0),
    (0x03, 0, "rodat", "Opens the DAT files readonly", 0),
    (0x01, 0, "sb", "Activate stencil buffer.", 0),
    (0x01, 0, "sw", "Force software vertex processing.", 0),
    (0x11, 0, "usemem", "Enables the use of the memory manager.", 1),
    (0x11, 0, "vectgraph", "Will have a client with vector graphics UI.", 1),
    (0x03, 0, "watson", "Handler crashes using watson. You can optionally specify a watson server.", 0),
    (0x01, 0, "watsondxdiagoff", "If specified, then watson reports will not include dxdiag info.", 0),
    (0x02, 0, "watsondll", "<string>: Specify a DLL that will be included in a watson debug report.", 0),
    (0x02, 0, "wfilelog", "<32 bitmask>: Specifies bitmask for weenie file logging.", 0),
    (0x02, 0, "wprintlog", "<32 bitmask>: Specifies bitmask for weenie printf logging.", 0),
    (0x32, 0, "dbwlib", "<wlib name> : Specifies the exact wlib contained in the datfiles to use.", 0),
    (0x11, 0, "tchat", "Use the Turbine chat room system.", 1),
    (0x32, 0, "tchatdll", "<dll path> : Specifies the dll to load for chat client services", 0),
];

/// The game's own options, added before the client's.
// D2-01690
const GAME_OPTIONS: [Row; 10] = [
    (0x32, 0, "migrationurl", "<url> : The URL to talk to when checking for billing migration status.", 0),
    (0x32, b'u', "user", "<name> : Character Name you would like to play", 0),
    (0x32, b'r', "create", "<name> : Character Name you would like to create/play", 0),
    (0x32, 0, "gunclientdll", "<file> : Name of alternate gundll", 0),
    (0x32, 0, "gunserver", "<hostname/IP Addr> : Connect to the gun server specified.", 0),
    (0x62, 0, "gunport", "<port> : Connect to the gun server at the given port. See 'gunserver'.", 0),
    (0x11, 0, "gunticket", "Login with gun ticketing. You must specify the gunserver argument as well.", 1),
    (0x32, 0, "gunticketoverride", "<ticket> The string of the gun auth ticket to use. If ommitted but -gunticket is used, then the client will get its gun ticket from GunDll.", 0),
    (0x22, 0, "migration1Timeout", "Specify the number of milliseconds to wait for a response before giving up.", 0),
    (0x22, 0, "migration2Timeout", "Specify the number of milliseconds to wait for a response before giving up.", 0),
];

/// What the September 2013 client keeps from its command line.
// D2-00086, D2-00087, D2-00089, D2-00090
#[derive(Clone, Debug, PartialEq)]
pub struct ClientLine {
    /// The account, in small letters.
    pub account: String,
    pub host: String,
    pub port: i32,
    pub width: i32,
    pub height: i32,
    pub full_screen: i32,
    pub ime: i32,
    pub out_port: i32,
    /// The preferences file named.
    pub prefs: Option<String>,
    /// The login's ticket given directly.
    pub ticket: Option<String>,
    pub language: Option<String>,
    /// The character to play, or to make and play.
    pub user: Option<String>,
    pub create: Option<String>,
    pub gun_client_dll: Option<String>,
    pub gun_server: Option<String>,
    pub gun_port: u16,
    pub gun_ticket: i32,
    pub gun_ticket_override: Option<String>,
    /// Chat through the game's own rooms rather than the friends service.
    pub turbine_chat: i32,
    /// The gore and blood the line forces (`maxgore`: 1 fun, 2 none, 3 low, 4 normal;
    /// `greenblood`).
    pub top_gore: Option<u32>,
    pub green_blood: bool,
    /// Preferences set for the run: each one's section, key and value.
    pub run_values: Vec<(u32, u32, i32)>,
    /// Options taken that this client has nothing to do with (a developer's, the registry's, the
    /// crash reporter's).
    pub ignored: Vec<String>,
    error: String,
    /// What was written out, and whether it was shown.
    pub output: String,
    pub shown: bool,
}

impl Default for ClientLine {
    /// Before the line is read: the server's port 7304, a window of 1024 by 768, the friends
    /// service's port 2310, nothing else named.
    // D2-00109, D2-01687
    fn default() -> Self {
        Self {
            account: String::new(),
            host: String::new(),
            port: 0x1C88,
            width: 0x400,
            height: 0x300,
            full_screen: 0,
            ime: 0,
            out_port: 0,
            prefs: None,
            ticket: None,
            language: None,
            user: None,
            create: None,
            gun_client_dll: None,
            gun_server: None,
            gun_port: 0x906,
            gun_ticket: 0,
            gun_ticket_override: None,
            turbine_chat: 0,
            top_gore: None,
            green_blood: false,
            run_values: Vec::new(),
            ignored: Vec::new(),
            error: String::new(),
            output: String::new(),
            shown: false,
        }
    }
}

impl ClientLine {
    /// Read a command line's words (the program's name first): what it keeps, and whether it read.
    #[must_use]
    pub fn parse(argv: &[&str]) -> (Self, bool) {
        let mut line = Self::default();
        let ok = parse_args(&mut line, argv, true);
        (line, ok)
    }

    /// The host named.
    // D2-00093
    pub fn set_host_name(&mut self, host: &str) {
        host.clone_into(&mut self.host);
    }

    /// The account named, in small letters.
    // D2-00108, D2-00081
    pub fn set_account(&mut self, account: &str) {
        self.account = account.to_ascii_lowercase();
    }

    /// The language named.
    // D2-00074
    pub fn set_language(&mut self, language: &str) {
        self.language = Some(language.to_owned());
    }

    fn keep(&mut self, arg: &Arg, value: &str) -> bool {
        let Some(kept) = arg.keep(value) else {
            return false;
        };
        let text = |k: &Kept| match k {
            Kept::Text(t) => Some(t.clone()),
            _ => None,
        };
        let number = |k: &Kept| match k {
            Kept::Number(n) => *n,
            _ => 0,
        };
        match arg.name.as_str() {
            "fullscreen" => self.full_screen = number(&kept),
            "height" => self.height = number(&kept),
            "width" => self.width = number(&kept),
            "imeon" => self.ime = number(&kept),
            "outport" => self.out_port = number(&kept),
            "port" => self.port = number(&kept),
            "prefs" => self.prefs = text(&kept),
            "glsticketdirect" => self.ticket = text(&kept),
            "tchat" => self.turbine_chat = number(&kept),
            "user" => self.user = text(&kept),
            "create" => self.create = text(&kept),
            "gunclientdll" => self.gun_client_dll = text(&kept),
            "gunserver" => self.gun_server = text(&kept),
            "gunport" => {
                if let Kept::Port(p) = kept {
                    self.gun_port = p;
                }
            }
            "gunticket" => self.gun_ticket = number(&kept),
            "gunticketoverride" => self.gun_ticket_override = text(&kept),
            _ => {}
        }
        true
    }

    /// The preferences for this run: the file's, with the values the line set over them.
    #[must_use]
    pub fn preferences(
        &self,
        file: crate::profile::Profile,
        names: crate::preferences::Names,
    ) -> crate::preferences::Preferences {
        let mut p = crate::preferences::Preferences::startup(file, names);
        for (s, k, v) in &self.run_values {
            p.set_bool(*s, *k, *v);
        }
        p
    }
}

impl Reader for ClientLine {
    // D2-00100, D2-01690
    fn build(&mut self, list: &mut ArgList) {
        for (form, letter, name, help, value) in GAME_OPTIONS.iter().chain(&CLIENT_OPTIONS) {
            list.add_cmd(*form, *letter, name, help, *value);
        }
    }

    /// What each option does: kept by its form, then by its name.
    // D2-00102, D2-01691
    fn evaluate(&mut self, arg: &Arg, value: &str) -> bool {
        if !self.keep(arg, value) {
            return false;
        }
        let set = |s: u32, k: u32, v: i32| (s, k, v);
        let run = match arg.name.as_str() {
            "account" => {
                self.set_account(value);
                None
            }
            "host" => {
                self.set_host_name(value);
                None
            }
            "language" => {
                self.set_language(value);
                None
            }
            "ff" => Some(set(run::RENDER_DEVICE, run::FORCE_FIXED_FUNCTION, 1)),
            "sb" => Some(set(run::RENDER_DEVICE, run::USE_STENCIL_BUFFER, 1)),
            "sw" => Some(set(run::RENDER_DEVICE, run::FORCE_SOFTWARE_VERTICES, 1)),
            "lb" => Some(set(run::GRAPHICS, run::TERRAIN_BLENDING, 1)),
            "nolb" => Some(set(run::GRAPHICS, run::TERRAIN_BLENDING, 0)),
            "maxgore" => {
                self.top_gore = match value.to_ascii_lowercase().as_str() {
                    "fun" => Some(1),
                    "none" => Some(2),
                    "low" => Some(3),
                    "normal" => Some(4),
                    _ => {
                        set_error_text(
                            self,
                            "gore level must be one of these: (fun,none,low,normal)",
                        );
                        return false;
                    }
                };
                None
            }
            "greenblood" => {
                self.green_blood = true;
                None
            }
            "outport" => {
                if !(1..0x10000).contains(&self.out_port) {
                    set_error_text(self, "Cli port must be between 1 and 65535\n");
                    return false;
                }
                None
            }
            "tchat" => {
                self.turbine_chat = 1;
                None
            }
            "autonomy" | "chargen" | "debug" | "dropmip" | "frill" | "ls" | "farview"
            | "glsticket" | "nodbwlib" | "refreshrate" | "regserver" | "rodat" | "watson"
            | "watsondxdiagoff" | "watsondll" | "wfilelog" | "wprintlog" | "dbwlib" => {
                self.ignored.push(arg.name.clone());
                None
            }
            _ => None,
        };
        if let Some(r) = run {
            self.run_values.push(r);
        }
        true
    }

    /// Every word read: an account and a host are needed, and the friends service's ticket needs
    /// the service named.
    // D2-01692
    fn done(&mut self) -> bool {
        let error = if self.account.is_empty() {
            "You must specify an account name"
        } else if self.host.is_empty() {
            "You must specify a host name"
        } else if self.gun_server.as_deref().unwrap_or("").is_empty() && self.gun_ticket != 0 {
            "You must specify a gun server in order to use GUN ticketing"
        } else {
            return true;
        };
        set_error_text(self, error);
        false
    }

    fn error_text(&mut self) -> &mut String {
        &mut self.error
    }

    /// Text written out, kept to be shown.
    // D2-00112, D2-17750
    fn output(&mut self, text: &str) {
        self.output.push_str(text);
    }

    /// The text written out shown (the host shows it).
    // D2-00103
    fn finish_output(&mut self) {
        self.shown = true;
    }
}
