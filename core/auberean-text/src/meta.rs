//! The string tables' meta language: how a table's text agrees with the values put into it.
//!
//! # Overview
//!
//! A table entry is a list of fragments with a variable between each two. To write it, the
//! fragments and the variables' values are joined into one input, each value between two `U+0001`
//! units, so the reader knows which text came from the table and which from a value. The input is
//! read into nodes, and the nodes are written out:
//!
//! - **text** is written as it is (escapes and all: the caller unescapes the whole result);
//! - **a variable** (a value) is written without its flags; a value's text may end in `[flags]`,
//!   else it gets flags of its own: `1` when it is a number the language takes as singular, `+`
//!   when it is a number not below zero (an empty value counts as the number 0). `M` and `F` in a
//!   value's flags stand for the language's male and female letters (and themselves);
//! - `#n:` before a variable or a choice numbers it; `^` before a variable capitalizes its first
//!   letter;
//! - **a choice** `{one[flags]|other[flags]|...}` writes one alternative: the one whose flags best
//!   match the flags of the variable with its number (an unnumbered choice takes the unnumbered
//!   variable, else the one numbered -1). Each flag letter the variable has scores 2, `!x` scores 2
//!   when it lacks `x`, an alternative with no flags scores 1, and the last of the best scores wins;
//!   a choice nothing scores for writes nothing;
//! - `{{name}}` is an option for the whole text: `keepspaces` keeps runs of spaces (else the result
//!   is trimmed on the left and each run of spaces and tabs becomes its first), and `item` marks
//!   the text as a generated item name, whose prefix, middle and suffix variables' letters are
//!   written after it in brackets for the text it goes into.
//!
//! Escapes: `\` before one of `[ ] ! { } # \ | ^` makes it plain text (the escape stays until the
//! caller unescapes), and `\q` is a double quote.
//!
//! # How it is read
//!
//! As the client reads it: a hand-written tokenizer feeding the grammar's LALR(1) tables, whose
//! actions build the nodes. The tokenizer looks at whether a choice is open (inside one, `}` and
//! `|` are marks and `{` and `^` are text; outside, the reverse), so when the parser asks for its
//! next token matters, and the tables decide that exactly as the client's do. Any syntax error
//! stops the reading; the rules' own errors are recorded and the reading goes on. Only the last
//! error is kept, and any error leaves the result unwritten (or, if the error comes while writing,
//! written up to it).

use crate::crt::{
    cstr, decimal, find_unescaped, format_s, is_digit, is_punct, is_space, to_upper, trim, wcschr,
    wcstol,
};
use crate::language::{female_letters, is_singular, male_letters, treasure_letters, Language};

/// The unit around each value in the joined input.
const SENTINEL: u16 = 0x0001;
const BACKSLASH: u16 = 0x5C;

/// What a rendering error was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderCode {
    #[default]
    Success = 0,
    Syntax = 1,
    DuplicateIds = 2,
    DuplicateChoices = 3,
    Internal = 4,
}

/// The last error of a rendering: its code, where in the input it was, and what it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderError {
    pub code: RenderCode,
    pub position: i32,
    pub message: &'static str,
}

impl Default for RenderError {
    fn default() -> Self {
        Self {
            code: RenderCode::Success,
            position: -1,
            message: "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Root,
    Text,
    Var,
    Choice,
    Outside,
}

/// A node of the read text. Its children are places in the reading's list of nodes; the reading
/// drops them all with itself.
// D2-14253, D2-14250, D2-14234
#[derive(Clone, Debug)]
struct Node {
    text: Vec<u16>,
    kind: Kind,
    children: Vec<usize>,
    id: i32,
    capital: bool,
    flags: Vec<u16>,
    pos: i32,
}

impl Node {
    // D2-14252
    fn new(kind: Kind) -> Self {
        Self {
            text: Vec::new(),
            kind,
            children: Vec::new(),
            id: 0,
            capital: false,
            flags: Vec::new(),
            pos: 0,
        }
    }
}

// Tokens: the tokenizer's numbers, and the marks as their own units.
const EOF: i32 = -1;
const TEXT: i32 = 0x101;
const VAR: i32 = 0x102;
const NUMBER: i32 = 0x103;
const FLAGS: i32 = 0x104;
const OUTSIDE: i32 = 0x105;
const CARET: u16 = 0x5E;
const OPEN: u16 = 0x7B;
const BAR: u16 = 0x7C;
const CLOSE: u16 = 0x7D;

// The grammar:
//
//   text_all : (nothing) | items
//   items    : item | items item
//   item     : text | var | choice | outside
//   var      : NUMBER VAR | NUMBER '^' VAR | VAR | '^' VAR
//   choice   : '{' begin alts '}' | NUMBER '{' begin_numbered alts '}'
//   outside  : OUTSIDE
//   alts     : alt | alts '|' alt
//   alt      : text | text flags
//   text     : TEXT
//   flags    : FLAGS
//
// and its LALR(1) tables. Symbols: 0 the end, 1 the error token, 2 an unknown token, 3 TEXT,
// 4 VAR, 5 NUMBER, 6 FLAGS, 7 OUTSIDE, 8 '^', 9 '{', 10 '}', 11 '|'; rules' left sides from 12.
const FINAL: i32 = 33;
const FLAG: i16 = -32768;
const LAST: i32 = 22;
const NT_BASE: i32 = 12;
const R1: [i16; 24] = [
    0, 12, 12, 13, 13, 14, 14, 14, 14, 15, 15, 15, 15, 17, 16, 18, 16, 19, 20, 20, 21, 21, 22, 23,
];
const R2: [i16; 24] = [
    0, 0, 1, 1, 2, 1, 1, 1, 1, 2, 3, 1, 2, 0, 4, 0, 5, 1, 1, 3, 1, 2, 1, 1,
];
const DEFACT: [i16; 34] = [
    1, 22, 11, 0, 17, 0, 13, 2, 3, 6, 7, 8, 5, 9, 0, 15, 12, 0, 4, 10, 0, 0, 18, 20, 0, 14, 0, 23,
    21, 16, 19, 0, 0, 0,
];
const DEFGOTO: [i16; 12] = [31, 7, 8, 9, 10, 17, 20, 11, 21, 22, 23, 28];
const PACT: [i16; 34] = [
    1, FLAG, FLAG, 10, FLAG, -3, FLAG, 1, FLAG, FLAG, FLAG, FLAG, FLAG, FLAG, -2, FLAG, FLAG, 9,
    FLAG, FLAG, 9, -7, 2, 14, 5, FLAG, 9, FLAG, FLAG, FLAG, FLAG, 16, 21, FLAG,
];
const PGOTO: [i16; 12] = [
    FLAG, FLAG, 15, FLAG, FLAG, FLAG, FLAG, FLAG, -9, FLAG, 0, FLAG,
];
const TABLE: [i16; 23] = [
    12, 16, 19, 25, 1, 2, 3, 12, 4, 5, 6, 24, 1, 26, 13, 29, 32, 30, 14, 15, 27, 33, 18,
];
const CHECK: [i16; 23] = [
    0, 4, 4, 10, 3, 4, 5, 7, 7, 8, 9, 20, 3, 11, 4, 10, 0, 26, 8, 9, 6, 0, 7,
];

/// A token's grammar symbol.
fn symbol(token: i32) -> i32 {
    match token {
        0x5E => 8,
        0x7B => 9,
        0x7D => 10,
        0x7C => 11,
        TEXT => 3,
        VAR => 4,
        NUMBER => 5,
        FLAGS => 6,
        OUTSIDE => 7,
        t if (0..=0x105).contains(&t) => 2,
        _ => 24,
    }
}

fn table_at(n: i32) -> Option<(i16, i16)> {
    let i = usize::try_from(n).ok()?;
    Some((*TABLE.get(i)?, *CHECK.get(i)?))
}

/// One reading of an input: the tokenizer's place, the nodes, and the last error. Dropping it
/// throws away every node read, as the client does once a text is written.
// D2-14255
struct Reader<'a> {
    input: Vec<u16>,
    at: usize,
    value: Vec<u16>,
    nodes: Vec<Node>,
    open: Option<usize>,
    last_added: Option<usize>,
    error: RenderError,
    lang: Option<&'a Language>,
}

impl<'a> Reader<'a> {
    // D2-14254
    fn new(input: Vec<u16>, lang: Option<&'a Language>) -> Self {
        Self {
            input,
            at: 0,
            value: Vec::new(),
            nodes: vec![Node::new(Kind::Root)],
            open: None,
            last_added: None,
            error: RenderError::default(),
            lang,
        }
    }

    /// The unit at `i`, zero past the end.
    fn unit(&self, i: usize) -> u16 {
        self.input.get(i).copied().unwrap_or(0)
    }

    // D2-14229, D2-14230
    fn set_error(&mut self, position: usize, code: RenderCode, message: &'static str) {
        self.error = RenderError {
            code,
            position: i32::try_from(position).unwrap_or(i32::MAX),
            message,
        };
    }

    // D2-14246
    fn in_choice(&self) -> bool {
        self.open
            .is_some_and(|n| self.nodes[n].kind == Kind::Choice)
    }

    // ---------------------------------------------------------------------------------------
    // The tokenizer
    // ---------------------------------------------------------------------------------------

    /// Whether a unit is a mark here: `}` and `|` inside a choice, `{` and `^` outside one.
    // D2-14237
    fn is_mark(&self, c: u16) -> bool {
        if c == CLOSE || c == BAR {
            self.in_choice()
        } else if c == OPEN || c == CARET {
            !self.in_choice()
        } else {
            false
        }
    }

    fn skip_blanks(&self, mut i: usize) -> usize {
        while matches!(self.unit(i), 0x20 | 0x09) {
            i += 1;
        }
        i
    }

    /// A number label at `start` (outside a choice): `#`, blanks, an optional `-`, blanks, digits,
    /// blanks, `:`, blanks, and then the sentinel, `^` or `{`. Answers its number and where the
    /// unit after the blanks is.
    // D2-14239
    fn number_label(&self, start: usize) -> Option<(i32, usize)> {
        if self.in_choice() || self.unit(start) != 0x23 {
            return None;
        }
        let mut i = self.skip_blanks(start + 1);
        if self.unit(i) != 0x2D && !is_digit(self.unit(i)) {
            return None;
        }
        let mut digits = Vec::new();
        if self.unit(i) == 0x2D {
            digits.push(self.unit(i));
            i += 1;
        }
        i = self.skip_blanks(i);
        if !is_digit(self.unit(i)) {
            return None;
        }
        while is_digit(self.unit(i)) && digits.len() < 0x400 {
            digits.push(self.unit(i));
            i += 1;
        }
        let (n, _) = wcstol(&digits);
        i = self.skip_blanks(i);
        if self.unit(i) != 0x3A {
            return None;
        }
        i = self.skip_blanks(i + 1);
        matches!(self.unit(i), SENTINEL | CARET | OPEN).then_some((n, i))
    }

    /// A flags block at `start` (inside a choice): `[`, the flags, `]` before the input ends.
    /// Answers the flags and where the unit after the `]` is.
    // D2-14241
    fn flags_block(&self, start: usize) -> Option<(Vec<u16>, usize)> {
        if !self.in_choice() || self.unit(start) != 0x5B {
            return None;
        }
        let last = self.input.len();
        let mut i = start + 1;
        let mut flags = Vec::new();
        while self.unit(i) != 0x5D && i < last {
            flags.push(self.unit(i));
            i += 1;
        }
        (self.unit(i) == 0x5D).then_some((flags, i + 1))
    }

    /// The next token, its text in `value`.
    // D2-14242, D2-14243
    fn lex(&mut self) -> i32 {
        let n = self.input.len();
        if self.at >= n {
            return EOF;
        }
        let c = self.unit(self.at);
        if c == SENTINEL {
            let start = self.at + 1;
            let mut i = start;
            while i < n && self.unit(i) != SENTINEL {
                i += 1;
            }
            self.value = self.input[start.min(n)..i.max(start).min(n)].to_vec();
            self.at = i + 1;
            return VAR;
        }
        if c == OPEN && self.unit(self.at + 1) == OPEN && !self.in_choice() {
            self.at += 2;
            let start = self.at;
            while !(self.unit(self.at) == CLOSE && self.unit(self.at + 1) == CLOSE)
                && self.unit(self.at) != 0
            {
                self.at += 1;
            }
            self.value = self.input[start.min(n)..self.at.min(n)].to_vec();
            if self.unit(self.at) == CLOSE && self.unit(self.at + 1) == CLOSE {
                self.at += 2;
            }
            return OUTSIDE;
        }
        if self.is_mark(c) {
            self.at += 1;
            return i32::from(c);
        }
        if let Some((v, next)) = self.number_label(self.at) {
            self.at = next;
            self.value = decimal(i64::from(v));
            return NUMBER;
        }
        if let Some((flags, next)) = self.flags_block(self.at) {
            self.value = flags.into_iter().filter(|&u| u != 0x20).collect();
            self.at = self.skip_blanks(next);
            return FLAGS;
        }
        self.value.clear();
        while self.at < n {
            let c = self.unit(self.at);
            if c == SENTINEL || self.is_mark(c) {
                break;
            }
            if self.number_label(self.at).is_some() || self.flags_block(self.at).is_some() {
                break;
            }
            if c == BACKSLASH && is_escape_char(self.unit(self.at + 1)) {
                self.value.push(BACKSLASH);
                self.value.push(self.unit(self.at + 1));
                self.at += 2;
            } else if c == BACKSLASH && self.unit(self.at + 1) == 0x71 {
                self.value.push(0x22);
                self.at += 2;
            } else {
                self.value.push(c);
                self.at += 1;
            }
        }
        TEXT
    }

    // ---------------------------------------------------------------------------------------
    // The rules
    // ---------------------------------------------------------------------------------------

    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Place a node: in the open choice, else in the text; the open choice itself closes into the
    /// text.
    // D2-14247, D2-14238
    fn add(&mut self, node: usize) {
        self.nodes[node].pos = i32::try_from(self.at).unwrap_or(i32::MAX) - 1;
        self.last_added = Some(node);
        match self.open {
            Some(open) if open != node => self.nodes[open].children.push(node),
            Some(_) => {
                self.nodes[0].children.push(node);
                self.open = None;
            }
            None => self.nodes[0].children.push(node),
        }
    }

    // D2-14256
    fn rule_text(&mut self, text: Vec<u16>) {
        let mut n = Node::new(Kind::Text);
        n.text = text;
        let n = self.push(n);
        self.add(n);
    }

    // D2-14248
    fn rule_flags(&mut self, flags: Vec<u16>) {
        if let Some(n) = self.last_added {
            self.nodes[n].flags = flags;
        }
    }

    // D2-14259
    fn rule_outside(&mut self, text: Vec<u16>) {
        let mut n = Node::new(Kind::Outside);
        n.text = text;
        let n = self.push(n);
        self.add(n);
    }

    // D2-14258
    fn rule_choice_begin(&mut self, id: i32) {
        let mut n = Node::new(Kind::Choice);
        n.id = id;
        self.open = Some(self.push(n));
    }

    /// A choice may have one alternative without flags.
    // D2-14251, D2-14249
    fn rule_choice_end(&mut self) {
        let Some(open) = self.open else {
            return;
        };
        let mut seen = false;
        for &c in &self.nodes[open].children.clone() {
            if self.nodes[c].flags.is_empty() {
                if seen {
                    let pos = usize::try_from(self.nodes[c].pos).unwrap_or(0);
                    self.set_error(
                        pos,
                        RenderCode::DuplicateChoices,
                        "multiple default choices in this block",
                    );
                    break;
                }
                seen = true;
            }
        }
        self.add(open);
    }

    /// A value: its text and flags, as the module header says.
    // D2-14257
    fn rule_var(&mut self, value: &[u16], id: i32, capital: bool) {
        let mut node = Node::new(Kind::Var);
        node.id = id;
        node.capital = capital;
        let c = cstr(value);
        let Some(open) = find_unescaped(c, 0x5B, BACKSLASH) else {
            node.text = value.to_vec();
            let (n, end) = wcstol(value);
            if end == c.len() {
                if is_singular(self.lang, n) {
                    node.flags.push(0x31);
                }
                if n >= 0 {
                    node.flags.push(0x2B);
                }
            }
            let n = self.push(node);
            self.add(n);
            return;
        };
        node.text = c[..open].to_vec();
        trim(&mut node.text, false, true);
        let Some(close) = find_unescaped(c, 0x5D, BACKSLASH) else {
            self.set_error(
                self.at,
                RenderCode::Syntax,
                "variable-substitution string is missing ']'",
            );
            return;
        };
        if close < open + 1 {
            // The client runs off the value here; there is nothing to write.
            self.set_error(
                self.at,
                RenderCode::Internal,
                "the flags close before they open",
            );
            return;
        }
        for &f in &c[open + 1..close] {
            match f {
                0x4D => {
                    node.flags.extend(male_letters(self.lang));
                    node.flags.push(0x4D);
                }
                0x46 => {
                    node.flags.extend(female_letters(self.lang));
                    node.flags.push(0x46);
                }
                0x20 => {}
                f => node.flags.push(f),
            }
        }
        for &t in &c[close + 1..] {
            if t != 0x20 && t != 0x09 {
                self.set_error(
                    self.at,
                    RenderCode::Syntax,
                    "there is text after the ']' in a variable-substitution string",
                );
            }
        }
        let n = self.push(node);
        self.add(n);
    }

    // ---------------------------------------------------------------------------------------
    // The parser
    // ---------------------------------------------------------------------------------------

    fn number_of(v: &[u16]) -> i32 {
        wcstol(v).0
    }

    /// A rule's action, given the values on the stack (the last is the rule's last symbol's).
    fn act(&mut self, rule: i32, values: &[Vec<u16>]) {
        let top = values.len() - 1;
        let back = |k: usize| values[top - k].clone();
        match rule {
            9 => self.rule_var(&back(0), Self::number_of(&back(1)), false),
            10 => self.rule_var(&back(0), Self::number_of(&back(2)), true),
            11 => self.rule_var(&back(0), 0, false),
            12 => self.rule_var(&back(0), 0, true),
            13 => self.rule_choice_begin(0),
            14 | 16 => self.rule_choice_end(),
            15 => self.rule_choice_begin(Self::number_of(&back(1))),
            17 => self.rule_outside(back(0)),
            22 => self.rule_text(back(0)),
            23 => self.rule_flags(back(0)),
            _ => {}
        }
    }

    /// Read the whole input with the grammar's tables, as the parser the client was generated
    /// with does (a syntax error is recorded once, and recovery, which no state of this grammar
    /// can make, gives up).
    // D2-14226, D2-14240
    fn parse(&mut self) {
        let mut states: Vec<i32> = vec![0];
        let mut values: Vec<Vec<u16>> = vec![Vec::new()];
        let mut lookahead: Option<i32> = None;
        let mut recovering = 0;
        let mut state = 0;
        loop {
            // Decide: shift, reduce, or error.
            let mut action = None;
            let pact = PACT[state as usize];
            if pact != FLAG {
                let token = *lookahead.get_or_insert_with(|| self.lex());
                let (token, sym) = if token <= 0 {
                    (0, 0)
                } else {
                    (token, symbol(token))
                };
                lookahead = Some(token);
                let n = i32::from(pact) + sym;
                if (0..=LAST).contains(&n) {
                    let (t, c) = table_at(n).unwrap_or((0, -1));
                    if i32::from(c) == sym {
                        action = Some(i32::from(t));
                    }
                }
            }
            let rule = match action {
                Some(t) if t > 0 => {
                    if t == FINAL {
                        return;
                    }
                    if lookahead != Some(0) {
                        lookahead = None;
                    }
                    values.push(self.value.clone());
                    recovering = (recovering - 1).max(0);
                    state = t;
                    states.push(state);
                    continue;
                }
                Some(t) if t < 0 && t != i32::from(FLAG) => -t,
                Some(_) => 0,
                None => i32::from(DEFACT[state as usize]),
            };
            if rule == 0 {
                // Error.
                if recovering == 0 {
                    self.set_error(self.at, RenderCode::Syntax, "parse error");
                } else if recovering == 3 {
                    if lookahead == Some(0) {
                        return;
                    }
                    lookahead = None;
                }
                recovering = 3;
                loop {
                    let pact = PACT[state as usize];
                    if pact != FLAG {
                        let n = i32::from(pact) + 1;
                        if let Some((t, c)) = table_at(n).filter(|_| (0..=LAST).contains(&n)) {
                            if c == 1 && t > 0 && i32::from(t) != FINAL {
                                values.push(self.value.clone());
                                state = i32::from(t);
                                states.push(state);
                                break;
                            }
                        }
                    }
                    if states.len() == 1 {
                        return;
                    }
                    states.pop();
                    values.pop();
                    state = *states.last().unwrap_or(&0);
                }
                continue;
            }
            // Reduce.
            let len = R2[rule as usize] as usize;
            self.act(rule, &values);
            let keep = states.len() - len;
            states.truncate(keep);
            values.truncate(keep);
            values.push(Vec::new());
            let lhs = i32::from(R1[rule as usize]) - NT_BASE;
            let below = *states.last().unwrap_or(&0);
            let n = i32::from(PGOTO[lhs as usize]) + below;
            state = match table_at(n).filter(|_| (0..=LAST).contains(&n)) {
                Some((t, c)) if i32::from(c) == below => i32::from(t),
                _ => i32::from(DEFGOTO[lhs as usize]),
            };
            states.push(state);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Writing the nodes
    // ---------------------------------------------------------------------------------------

    /// Write the text's nodes into `out` (cleared first, once the variables are checked), the
    /// options into `outside`. False on an error, with what was written so far left in `out`.
    // D2-14235, D2-14245
    fn render(
        &mut self,
        out: &mut Vec<u16>,
        outside: &mut Vec<Vec<u16>>,
        pre: i32,
        mid: i32,
        post: i32,
    ) -> bool {
        let root = self.nodes[0].children.clone();
        let mut vars = Vec::new();
        let mut choices = Vec::new();
        let mut item = false;
        for &n in &root {
            match self.nodes[n].kind {
                Kind::Var => vars.push(n),
                Kind::Choice => choices.push(n),
                Kind::Outside => {
                    if cstr(&self.nodes[n].text) == wide("item") {
                        item = true;
                    }
                    outside.push(self.nodes[n].text.clone());
                }
                _ => {}
            }
        }
        for (j, &v) in vars.iter().enumerate() {
            for &u in &vars[..j] {
                if self.nodes[u].id != self.nodes[v].id {
                    continue;
                }
                let pos = usize::try_from(self.nodes[v].pos).unwrap_or(0);
                if self.nodes[v].id != 0 {
                    self.set_error(
                        pos,
                        RenderCode::DuplicateIds,
                        "multiple variables with the same ID; ambiguity",
                    );
                    return false;
                }
                if choices.iter().any(|&c| self.nodes[c].id == 0) {
                    self.set_error(
                        pos,
                        RenderCode::DuplicateIds,
                        "multiple un-numbered variables; ambiguity",
                    );
                    return false;
                }
            }
        }
        out.clear();
        let mut count = 0;
        let (mut pre_flags, mut mid_flags, mut post_flags) = (Vec::new(), Vec::new(), Vec::new());
        for &n in &root {
            let node = &self.nodes[n];
            match node.kind {
                Kind::Text => out.extend_from_slice(&node.text),
                Kind::Var => {
                    if node.id < 0 {
                        continue;
                    }
                    if item {
                        if count == pre {
                            pre_flags.clone_from(&node.flags);
                        } else if count == mid {
                            mid_flags.clone_from(&node.flags);
                        } else if count == post {
                            post_flags.clone_from(&node.flags);
                        }
                        count += 1;
                    }
                    if node.capital {
                        out.extend(capitalize(&node.text));
                    } else {
                        out.extend_from_slice(&node.text);
                    }
                }
                Kind::Choice => {
                    let var = vars
                        .iter()
                        .find(|&&v| self.nodes[v].id == node.id)
                        .or_else(|| {
                            if node.id == 0 {
                                vars.iter().find(|&&v| self.nodes[v].id == -1)
                            } else {
                                None
                            }
                        });
                    let Some(&var) = var else {
                        let pos = usize::try_from(node.pos).unwrap_or(0);
                        let message = if node.id == 0 {
                            "no default variable exists to match with this default choice block"
                        } else {
                            "no variable has the same ID as this choice block"
                        };
                        self.set_error(pos, RenderCode::Syntax, message);
                        return false;
                    };
                    if let Some(chosen) = self.choose(var, n) {
                        out.extend_from_slice(&self.nodes[chosen].text);
                    }
                }
                Kind::Outside => {}
                Kind::Root => {
                    let pos = usize::try_from(node.pos).unwrap_or(0);
                    self.set_error(pos, RenderCode::Internal, "unknown node type");
                    return false;
                }
            }
        }
        if item {
            let [prefix, middle, suffix] = treasure_letters(self.lang);
            let mut letters = take_valid_letters(&pre_flags, &prefix);
            letters.extend(take_valid_letters(&mid_flags, &middle));
            letters.extend(take_valid_letters(&post_flags, &suffix));
            out.extend(format_s(&wide("[%s]"), &[&letters]));
        }
        true
    }

    /// The alternative of `choice` that best matches `var`'s flags: the last of the highest
    /// positive score.
    // D2-14231
    fn choose(&self, var: usize, choice: usize) -> Option<usize> {
        let mut best = -1;
        let mut chosen = None;
        for &c in &self.nodes[choice].children {
            let s = match_score(&self.nodes[var].flags, &self.nodes[c].flags);
            if best <= s && s > 0 {
                chosen = Some(c);
                best = s;
            }
        }
        chosen
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// How well an alternative's flags match a variable's: 1 for none, else 2 for each letter the
/// variable has and each `!letter` it lacks.
// D2-14228
#[must_use]
pub fn match_score(var_flags: &[u16], choice_flags: &[u16]) -> i32 {
    if choice_flags.is_empty() {
        return 1;
    }
    let mut score = 0;
    let mut i = 0;
    while i < choice_flags.len() {
        let c = choice_flags[i];
        if c == 0x21 {
            i += 1;
            let d = choice_flags.get(i).copied().unwrap_or(0);
            if !wcschr(var_flags, d) {
                score += 2;
            }
        } else if wcschr(var_flags, c) {
            score += 2;
        }
        i += 1;
    }
    score
}

/// The units of `src` that `valid` holds.
// D2-14233
#[must_use]
pub fn take_valid_letters(src: &[u16], valid: &[u16]) -> Vec<u16> {
    src.iter().copied().filter(|&c| wcschr(valid, c)).collect()
}

/// The units a backslash escapes.
const ESCAPED: [u16; 9] = [0x5B, 0x5D, 0x21, 0x7B, 0x7D, 0x23, 0x5C, 0x7C, 0x5E];

/// Whether a unit is one the meta language escapes.
// D2-14260
#[must_use]
pub fn is_escape_char(c: u16) -> bool {
    c != 0 && ESCAPED.contains(&c)
}

/// Escape the meta language's marks with a backslash.
// D2-14272
#[must_use]
pub fn escape(s: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len() + 4);
    for &c in s {
        if is_escape_char(c) {
            out.push(BACKSLASH);
        }
        out.push(c);
    }
    out
}

/// Remove escapes: `\q` is a double quote, `\x` is `x` (a backslash at the very end leaves a zero
/// unit).
// D2-14273
#[must_use]
pub fn unescape(s: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == BACKSLASH {
            let next = s.get(i + 1).copied().unwrap_or(0);
            out.push(if next == 0x71 { 0x22 } else { next });
            i += 2;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// Cut a text at its first unescaped `[` (its flags) and trim the right.
// D2-14271
#[must_use]
pub fn strip_meta_letters(s: &[u16]) -> Vec<u16> {
    let mut out = s.to_vec();
    if let Some(p) = find_unescaped(s, 0x5B, BACKSLASH) {
        out.truncate(p);
        trim(&mut out, false, true);
    }
    out
}

/// Split a text into what comes before its flags and the flags: the text up to its first `[`
/// (escapes not considered) with the white space on its right trimmed, and the flags between that
/// `[` and the next `]` (or the end). A text without `[` is all text and no flags.
///
/// As the client does it: a `[` first in the text leaves the text whole, and a `[` last in it
/// leaves the flags as `[`; the search stops at a zero unit.
// D2-14270
#[must_use]
pub fn retrieve_meta_letters(s: &[u16]) -> (Vec<u16>, Vec<u16>) {
    let unit = |v: &[u16], i: usize| v.get(i).copied().unwrap_or(0);
    // Forward to the first `c`, stopping on a zero unit or the end.
    let find = |v: &[u16], from: usize, c: u16| {
        let mut at = from;
        loop {
            if unit(v, at) == c {
                return (true, at);
            }
            at = (at + 1).min(v.len());
            if unit(v, at) == 0 {
                return (false, at);
            }
        }
    };
    let (found, at) = find(s, 0, 0x5B);
    if !found {
        return (s.to_vec(), Vec::new());
    }
    // Everything before the `[`; with the `[` first, the end wraps and the whole text is taken.
    let mut text = if at == 0 {
        s.to_vec()
    } else {
        s[..at].to_vec()
    };
    let mut letters = s[at..].to_vec();
    let (_, open) = find(&letters, 0, 0x5B);
    let mut at = (open + 1).min(letters.len());
    if unit(&letters, at) != 0 {
        let rest = &letters[at..];
        let end = rest.iter().position(|&u| u == 0).unwrap_or(rest.len());
        letters = rest[..end].to_vec();
        at = 0;
    }
    let (_, close) = find(&letters, at, 0x5D);
    letters.truncate(close);
    trim(&mut text, false, true);
    (text, letters)
}

/// A player's name as a value of the meta language: its flags cut off, trimmed, then ` [flags]`
/// after it, the flags `M` for a man (else `F`) and `v` when it begins with a vowel.
///
/// The client also means to add `s` for a name ending in `s`, but it looks at the unit past the
/// name's end (always zero), so it never does.
// D2-14274
#[must_use]
pub fn apply_meta_tags_to_name(name: &[u16], male: bool) -> Vec<u16> {
    let mut out = strip_meta_letters(name);
    trim(&mut out, true, true);
    let mut tags = wide(if male { "M" } else { "F" });
    if crate::names::is_vowel(out.first().copied().unwrap_or(0)) {
        tags.extend(wide("v"));
    }
    out.extend(format_s(&wide(" [%s]"), &[&tags]));
    out
}

/// Upper-case the first unit that is not white space or punctuation (or is `\`, `/` or `@`).
// D2-14269
#[must_use]
pub fn capitalize(s: &[u16]) -> Vec<u16> {
    let mut out = s.to_vec();
    for u in &mut out {
        let c = *u;
        let skip = is_space(c) || is_punct(c) || c == 0xBF || c == 0xA1;
        if skip && !matches!(c, 0x5C | 0x2F | 0x40) {
            continue;
        }
        *u = to_upper(c);
        break;
    }
    out
}

/// Trim the left, then make each run of spaces and tabs its first unit.
// D2-14268, D2-14266
pub fn trim_excess_spaces(s: &mut Vec<u16>) {
    trim(s, true, false);
    let mut out = Vec::with_capacity(s.len());
    let mut in_run = false;
    for &c in s.iter() {
        let blank = c == 0x20 || c == 0x09;
        if !(blank && in_run) {
            out.push(c);
        }
        in_run = blank;
    }
    *s = out;
}

/// Join fragments and values into the input: each fragment, then (while values last) a value
/// between two sentinels. An empty fragment or value adds nothing between them.
// D2-14244
#[must_use]
pub fn join(fragments: &[Option<Vec<u16>>], values: &[Vec<u16>]) -> Vec<u16> {
    let mut input = Vec::new();
    for (i, f) in fragments.iter().enumerate() {
        if let Some(f) = f {
            input.extend_from_slice(f);
        }
        if let Some(v) = values.get(i) {
            input.push(SENTINEL);
            if !cstr(v).is_empty() {
                input.extend_from_slice(v);
            }
            input.push(SENTINEL);
        }
    }
    input
}

/// Render a table entry: its fragments, its variables' values (in the entry's order), and its
/// variables' names (for the item-name letters), into `out`; the text's options go to
/// `outside`. With `flags & 1`, excess spaces are trimmed unless the text keeps them. Answers the
/// last error; on any error `out` holds what it held before, or what was written up to the error.
// D2-14236
pub fn render_string(
    fragments: &[Option<Vec<u16>>],
    values: &[Vec<u16>],
    names: &[Vec<u16>],
    out: &mut Vec<u16>,
    outside: &mut Vec<Vec<u16>>,
    flags: u32,
    lang: Option<&Language>,
) -> RenderError {
    let (mut pre, mut mid, mut post) = (-1, -1, -1);
    for (i, name) in names.iter().enumerate() {
        let name = cstr(name);
        let i = i32::try_from(i).unwrap_or(i32::MAX);
        if name == wide("PRE") {
            pre = i;
        } else if name == wide("MID") || name == wide("NAME") {
            mid = i;
        } else if name == wide("POST") {
            post = i;
        }
    }
    let input = if mid != -1 && (pre != -1 || post != -1) {
        // The prefix and suffix were rendered already: their escapes come off.
        let mut v = values.to_vec();
        for k in [pre, post] {
            if let Some(x) = usize::try_from(k).ok().and_then(|k| v.get_mut(k)) {
                *x = unescape(x);
            }
        }
        join(fragments, &v)
    } else {
        join(fragments, values)
    };
    let mut r = Reader::new(input, lang);
    r.parse();
    if r.error.code == RenderCode::Success {
        r.render(out, outside, pre, mid, post);
        if r.error.code == RenderCode::Success
            && flags & 1 != 0
            && !outside.iter().any(|o| cstr(o) == wide("keepspaces"))
        {
            trim_excess_spaces(out);
        }
    }
    r.error
}
