//! Reading VTank-format metas (`.met` files), for `dereth.meta`'s import.
//!
//! The format is a typed table written one value a line (read from the public files players
//! share; no program's code was consulted):
//!
//! ```text
//! 1                     version
//! CondAct               the table's name
//! 5                     columns: CType, AType, CData, AData, State
//! CType … State         the column names
//! n × 5                 a flag per column
//! 332                   rows
//! i / 26                each cell: a type tag and the value (see below), five per row
//! ```
//!
//! A cell is `i` (a whole number), `s` (a line of text), `d` (a number), `b` (`True` or
//! `False`), `ba` (a byte count, then that many bytes: an embedded route or a meta view), or
//! `TABLE` (a nested table: columns, names, flags, rows, cells). Two nested shapes carry the
//! rules' data: a `k`/`v` table is a set of named fields, and a `K`/`V` table is a list of
//! conditions or actions, each a type and its data.
//!
//! Files that passed through a tool that changed their line endings to a single line feed still
//! read: a byte count covers `\r\n` line ends, so a lone `\n` inside a counted run counts as
//! two.

use dereth_plugin_api::Value;

/// The condition kinds, by the number the file stores.
const CONDITIONS: &[&str] = &[
    "never",
    "always",
    "all",
    "any",
    "chat_match",
    "pack_slots_le",
    "seconds_in_state_ge",
    "route_empty",
    "died",
    "vendor_open",
    "vendor_closed",
    "item_count_le",
    "item_count_ge",
    "monster_count_within",
    "monster_priority_count_within",
    "need_to_buff",
    "no_monsters_within",
    "landblock",
    "landcell",
    "portal_entered",
    "portal_exited",
    "no",
    "seconds_in_state_persist_ge",
    "spell_time_ge",
    "burden_ge",
    "route_distance_ge",
    "expr",
    "dialog_popup",
    "chat_capture",
];

/// The action kinds, by the number the file stores.
const ACTIONS: &[&str] = &[
    "none",
    "set_state",
    "chat",
    "multiple",
    "walk",
    "call_state",
    "return_state",
    "expr",
    "chat_expr",
    "watchdog_set",
    "watchdog_clear",
    "get_option",
    "set_option",
    "create_view",
    "destroy_view",
    "destroy_all_views",
];

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `text`.
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            data: text.as_bytes(),
            at: 0,
        }
    }

    pub(crate) fn line(&mut self) -> Result<&str, String> {
        if self.at >= self.data.len() {
            return Err("the file ends early".into());
        }
        let rest = &self.data[self.at..];
        let end = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
        self.at += (end + 1).min(rest.len());
        let line = rest[..end].strip_suffix(b"\r").unwrap_or(&rest[..end]);
        std::str::from_utf8(line).map_err(|_| format!("line at byte {} is not text", self.at))
    }

    pub(crate) fn int(&mut self, what: &str) -> Result<i64, String> {
        let l = self.line()?.trim();
        l.parse()
            .map_err(|_| format!("{l:?} is not a whole number ({what})"))
    }

    /// `count` bytes as the writer counted them (see the module documentation).
    fn bytes(&mut self, count: usize) -> Result<String, String> {
        let start = self.at;
        let mut counted = 0;
        while counted < count {
            let Some(&b) = self.data.get(self.at) else {
                return Err("the file ends inside an embedded block".into());
            };
            let lone_lf = b == b'\n' && (self.at == 0 || self.data[self.at - 1] != b'\r');
            counted += if lone_lf { 2 } else { 1 };
            self.at += 1;
        }
        Ok(String::from_utf8_lossy(&self.data[start..self.at]).into_owned())
    }

    fn value(&mut self) -> Result<Value, String> {
        let tag = self.line()?.trim().to_owned();
        Ok(match tag.as_str() {
            "i" => Value::Int(self.int("a whole number")?),
            "s" => Value::from(self.line()?.to_owned()),
            "d" => {
                let l = self.line()?.trim();
                Value::Num(l.parse().map_err(|_| format!("{l:?} is not a number"))?)
            }
            "b" => Value::Bool(self.line()?.trim().eq_ignore_ascii_case("true")),
            "ba" => {
                let n = self.int("a byte count")?;
                let n = usize::try_from(n).map_err(|_| format!("{n} is not a byte count"))?;
                Value::from(self.bytes(n)?)
            }
            "TABLE" => self.table()?,
            other => return Err(format!("{other:?} is not a value's type")),
        })
    }

    pub(crate) fn table(&mut self) -> Result<Value, String> {
        let (names, rows) = self.table_head()?;
        let mut cells = Vec::new();
        for _ in 0..rows {
            cells.push(self.row(&names)?);
        }
        let shape: Vec<&str> = names.iter().map(String::as_str).collect();
        Ok(match shape.as_slice() {
            // Named fields.
            ["k", "v"] => {
                let mut m = Value::map();
                for mut row in cells {
                    let v = row.pop().unwrap_or_default();
                    let k = row.pop().unwrap_or_default();
                    let key = match &k {
                        Value::Int(i) => i.to_string(),
                        other => other.as_str().unwrap_or_default().to_owned(),
                    };
                    m = m.with(&key, v);
                }
                m
            }
            // A list of typed entries.
            ["K", "V"] => Value::List(
                cells
                    .into_iter()
                    .map(|mut row| {
                        let data = row.pop().unwrap_or_default();
                        let ty = row.pop().unwrap_or_default();
                        Value::map().with("type", ty).with("data", data)
                    })
                    .collect(),
            ),
            _ => Value::List(cells.into_iter().map(|row| named(&names, row)).collect()),
        })
    }

    /// A table's columns and its row count, before its rows.
    fn table_head(&mut self) -> Result<(Vec<String>, usize), String> {
        let columns = self.int("a table's column count")?;
        if !(1..=64).contains(&columns) {
            return Err(format!("{columns} columns is not a table"));
        }
        let names: Vec<String> = (0..columns)
            .map(|_| self.line().map(str::to_owned))
            .collect::<Result<_, _>>()?;
        for _ in 0..columns {
            self.line()?;
        }
        let rows = self.int("a table's row count")?;
        if !(0..=1_000_000).contains(&rows) {
            return Err(format!("{rows} rows is not a table"));
        }
        Ok((names, usize::try_from(rows).unwrap_or_default()))
    }

    /// One row's cells, a value for each of `names`.
    fn row(&mut self, names: &[String]) -> Result<Vec<Value>, String> {
        names.iter().map(|_| self.value()).collect()
    }
}

/// A row as a map of its cells by their columns' names.
fn named(names: &[String], row: Vec<Value>) -> Value {
    let mut m = Value::map();
    for (name, v) in names.iter().zip(row) {
        m = m.with(name, v);
    }
    m
}

fn kind_name(table: &[&str], v: &Value) -> Value {
    v.as_i64()
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| table.get(i))
        .map_or_else(
            || Value::from(format!("unknown:{}", v.as_i64().unwrap_or(-1))),
            |n| Value::from(*n),
        )
}

/// An embedded route: a name line and a count line before the route's own text, which is kept
/// as it was (`text`) so that the meta can be written again.
fn route(text: &str) -> Value {
    let name = text.lines().next().unwrap_or_default().trim().to_owned();
    let body = text.find("uTank2 NAV").map_or("", |i| &text[i..]);
    let read = match crate::navfile::parse(body) {
        Ok(r) => Value::map().with("name", name).with("route", r),
        Err(e) => Value::map().with("name", name).with("error", e),
    };
    read.with("text", body.to_owned())
}

/// A condition's or action's entry `{ type, data }` given names and nested entries resolved.
fn entry(table: &[&str], is_action: bool, ty: &Value, data: Value) -> Value {
    let kind = kind_name(table, ty);
    let data = match (kind.as_str().unwrap_or_default(), data) {
        // The nested lists.
        ("all" | "any" | "no", Value::List(items)) if !is_action => Value::List(
            items
                .into_iter()
                .map(|e| entry(CONDITIONS, false, e.get("type"), e.get("data").clone()))
                .collect(),
        ),
        ("multiple", Value::List(items)) if is_action => Value::List(
            items
                .into_iter()
                .map(|e| entry(ACTIONS, true, e.get("type"), e.get("data").clone()))
                .collect(),
        ),
        ("walk", Value::Str(text)) => route(&text),
        (_, d) => d,
    };
    Value::map().with("kind", kind).with("data", data)
}

/// A rule of the meta's table, its kinds named and its nested entries resolved.
fn rule(row: &Value) -> Value {
    let cond = entry(
        CONDITIONS,
        false,
        row.get("CType"),
        row.get("CData").clone(),
    );
    let act = entry(ACTIONS, true, row.get("AType"), row.get("AData").clone());
    Value::map()
        .with("state", row.get("State").clone())
        .with("condition", cond)
        .with("action", act)
}

/// A reader past a meta's header, at its first rule, with its table's columns and rule count.
fn rules_head(text: &str) -> Result<(Reader<'_>, Vec<String>, usize), String> {
    let mut r = Reader::new(text);
    let version = r.line()?.trim().to_owned();
    let name = r.line()?.trim().to_owned();
    if name != "CondAct" {
        return Err(format!(
            "{version:?} then {name:?} is not a meta's header (1, CondAct)"
        ));
    }
    let (names, rows) = r.table_head()?;
    if names == ["k", "v"] {
        return Err("a meta's rules are not a table".into());
    }
    Ok((r, names, rows))
}

/// A meta read into `{ rules = { { state, condition = { kind, data }, action = { kind, data } },
/// … } }`, rules in file order. A kind the reader does not know is named `unknown:<n>`. An
/// embedded route's data is `{ name, route, text }` (`error` in place of `route` when it does not
/// read).
///
/// # Errors
/// Not a meta, or a value that does not read as its type.
pub fn parse(text: &str) -> Result<Value, String> {
    let (mut r, names, rows) = rules_head(text)?;
    let mut rules = Vec::new();
    for _ in 0..rows {
        let row = r.row(&names)?;
        rules.push(rule(&named(&names, row)));
    }
    Ok(Value::map().with("rules", Value::List(rules)))
}

/// About how much of a meta's text one page reads: a page ends at the first rule that ends past
/// this many bytes from where it began.
pub const PAGE_BYTES: usize = 16 << 10;

/// One page of a meta's rules ([`page`]).
struct Page {
    rules: Vec<Value>,
    /// The meta's rule count.
    total: usize,
    /// Where the next page begins (its rule and byte), unless these were the last.
    next: Option<(usize, usize)>,
}

/// One page of a meta's rules, as [`parse`] reads them, beginning at rule `row` (from 0), which
/// begins at byte `at` of `text` (the first rule's when `None`): the rules that begin in about
/// [`PAGE_BYTES`] of the text (one at least). A page costs its own rules, not the text before it.
///
/// # Errors
/// Not a meta, a place that is not one of its rules', or a value that does not read as its type.
fn page(text: &str, row: usize, at: Option<usize>) -> Result<Page, String> {
    let (mut r, names, rows) = rules_head(text)?;
    if let Some(at) = at {
        if at < r.at || at > r.data.len() {
            return Err(format!("{at} is not a place in the meta"));
        }
        r.at = at;
    }
    if row > rows {
        return Err(format!("the meta has {rows} rules, not {row}"));
    }
    let start = r.at;
    let mut rules = Vec::new();
    let mut next = row;
    while next < rows && (rules.is_empty() || r.at - start < PAGE_BYTES) {
        let cells = r.row(&names)?;
        rules.push(rule(&named(&names, cells)));
        next += 1;
    }
    Ok(Page {
        rules,
        total: rows,
        next: (next < rows).then_some((next, r.at)),
    })
}

/// The most metas a plugin may be reading a page at a time; a read begun past it drops the oldest.
pub const HELD_READS: usize = 4;

/// The metas a plugin is reading a page at a time: each one's text, held from its first page to
/// its last, so that a later page is not handed the whole text again.
#[derive(Debug, Default)]
pub struct Reads {
    held: std::collections::VecDeque<(i64, String)>,
    last_id: i64,
}

impl Reads {
    /// `meta.parse` with a page: `{}` begins a read of `text`, and the `next` of a page answers
    /// the page after it from the text held (`text` is not read again). Answers `{ rules, total,
    /// next }`, `next` absent once the last page is read, when the text is let go.
    ///
    /// # Errors
    /// Not a meta, a place that is not one of its rules', a value that does not read as its type,
    /// or a page of a read no longer held.
    pub fn page(&mut self, text: &str, cursor: &Value) -> Result<Value, String> {
        let row = cursor.get("row").as_i64().unwrap_or(0);
        let row = usize::try_from(row).map_err(|_| format!("{row} is not a rule's place"))?;
        let at = match cursor.get("at").as_i64() {
            Some(at) => {
                Some(usize::try_from(at).map_err(|_| format!("{at} is not a place in the meta"))?)
            }
            None => None,
        };
        let (id, read) = match cursor.get("read").as_i64() {
            None => (None, page(text, row, at)?),
            Some(id) => {
                let i = self
                    .held
                    .iter()
                    .position(|(held, _)| *held == id)
                    .ok_or("the meta being read is no longer held; read it again")?;
                let read = page(&self.held[i].1, row, at);
                if !matches!(&read, Ok(p) if p.next.is_some()) {
                    self.held.remove(i);
                }
                (Some(id), read?)
            }
        };
        let mut out = Value::map()
            .with("rules", Value::List(read.rules))
            .with("total", i64::try_from(read.total).unwrap_or_default());
        if let Some((row, at)) = read.next {
            let id = id.unwrap_or_else(|| {
                if self.held.len() >= HELD_READS {
                    self.held.pop_front();
                }
                self.last_id += 1;
                self.held.push_back((self.last_id, text.to_owned()));
                self.last_id
            });
            out = out.with(
                "next",
                Value::map()
                    .with("read", id)
                    .with("row", i64::try_from(row).unwrap_or_default())
                    .with("at", i64::try_from(at).unwrap_or_default()),
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the file reader; the import's behaviour is pinned by the
    //! conformance case `automation/met-import`).
    use super::{parse, Reads, HELD_READS, PAGE_BYTES};
    use dereth_plugin_api::Value;

    fn sample(crlf: bool) -> String {
        let embedded = "[None]\r\n1\r\nuTank2 NAV 1.2\r\n4\r\n1\r\n0\r\n33.8\r\n42.2\r\n0\r\n0\r\n";
        let lines = [
            "1",
            "CondAct",
            "5",
            "CType",
            "AType",
            "CData",
            "AData",
            "State",
            "n",
            "n",
            "n",
            "n",
            "n",
            "3", // three rules
            "i",
            "18",
            "i",
            "1",
            "i",
            "-1433092096",
            "s",
            "Next",
            "s",
            "Default",
            "i",
            "2",
            "i",
            "3",
            "TABLE",
            "2",
            "K",
            "V",
            "n",
            "n",
            "2",
            "i",
            "4",
            "s",
            "You say, \"(?<word>.*)\"",
            "i",
            "26",
            "TABLE",
            "2",
            "k",
            "v",
            "n",
            "n",
            "1",
            "s",
            "e",
            "s",
            "getvar[x]==1",
            "TABLE",
            "2",
            "K",
            "V",
            "n",
            "n",
            "2",
            "i",
            "2",
            "s",
            "/say hi",
            "i",
            "9",
            "TABLE",
            "2",
            "k",
            "v",
            "n",
            "n",
            "3",
            "s",
            "s",
            "s",
            "Stuck",
            "s",
            "r",
            "d",
            "2.5",
            "s",
            "t",
            "d",
            "10",
            "s",
            "Default",
        ];
        let mut out = lines.join("\n") + "\n";
        out.push_str(&format!(
            "i\n1\ni\n4\ni\n0\nba\n{}\n{}s\nNext\n",
            embedded.len(),
            embedded
        ));
        if crlf {
            out.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            out.replace("\r\n", "\n")
        }
    }

    fn check(meta: &Value) {
        let rules = meta.get("rules").as_list().unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].get("state").as_str(), Some("Default"));
        assert_eq!(
            rules[0].get("condition").get("kind").as_str(),
            Some("landcell")
        );
        assert_eq!(
            rules[0].get("action").get("kind").as_str(),
            Some("set_state")
        );
        assert_eq!(rules[0].get("action").get("data").as_str(), Some("Next"));
        let any = rules[1].get("condition");
        assert_eq!(any.get("kind").as_str(), Some("all"));
        let parts = any.get("data").as_list().unwrap();
        assert_eq!(parts[0].get("kind").as_str(), Some("chat_match"));
        assert_eq!(parts[1].get("kind").as_str(), Some("expr"));
        assert_eq!(parts[1].get("data").get("e").as_str(), Some("getvar[x]==1"));
        let multi = rules[1].get("action").get("data").as_list().unwrap();
        assert_eq!(multi[1].get("kind").as_str(), Some("watchdog_set"));
        assert_eq!(multi[1].get("data").get("r").as_f64(), Some(2.5));
        let walk = rules[2].get("action");
        assert_eq!(walk.get("kind").as_str(), Some("walk"));
        assert_eq!(
            walk.get("data").get("route").get("kind").as_str(),
            Some("once")
        );
        assert!(walk
            .get("data")
            .get("text")
            .as_str()
            .is_some_and(|t| t.starts_with("uTank2 NAV 1.2")));
        assert_eq!(rules[2].get("state").as_str(), Some("Next"));
    }

    #[test]
    fn a_meta_reads_with_either_line_ending() {
        check(&parse(&sample(true)).unwrap());
        check(&parse(&sample(false)).unwrap());
    }

    /// A meta of `rules` rules, each a chat line said always.
    fn many(rules: usize) -> String {
        let mut out = String::from(
            "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\nCData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n",
        );
        out.push_str(&format!("{rules}\r\n"));
        for i in 0..rules {
            out.push_str(&format!(
                "i\r\n1\r\ni\r\n2\r\ni\r\n0\r\ns\r\n/say {i}\r\ns\r\nDefault\r\n"
            ));
        }
        out
    }

    /// The rules of `text` read a page at a time through `reads`, and the pages it took.
    fn paged(reads: &mut Reads, text: &str) -> (Vec<Value>, usize) {
        let (mut rules, mut pages, mut page) = (Vec::new(), 0, Value::map());
        loop {
            let p = reads.page(text, &page).unwrap();
            pages += 1;
            assert_eq!(p.get("total").as_i64(), Some(1000));
            rules.extend(p.get("rules").as_list().unwrap().iter().cloned());
            match p.get("next") {
                Value::Nil => return (rules, pages),
                next => page = next.clone(),
            }
        }
    }

    #[test]
    fn a_meta_read_a_page_at_a_time_is_the_same_rules_as_read_at_once() {
        let text = many(1000);
        let mut reads = Reads::default();
        let (rules, pages) = paged(&mut reads, &text);
        assert!(pages >= text.len() / PAGE_BYTES, "{pages} pages");
        assert_eq!(
            parse(&text).unwrap().get("rules").as_list().unwrap(),
            &rules[..]
        );
        // Each read lets its text go at its last page; a later page needs only the cursor.
        assert!(reads.held.is_empty());
        let first = reads.page(&text, &Value::map()).unwrap();
        let next = first.get("next").clone();
        assert!(reads.page("", &next).is_ok());
    }

    #[test]
    fn a_read_begun_past_the_held_reads_lets_the_oldest_go() {
        let text = many(1000);
        let mut reads = Reads::default();
        let firsts: Vec<Value> = (0..=HELD_READS)
            .map(|_| {
                reads
                    .page(&text, &Value::map())
                    .unwrap()
                    .get("next")
                    .clone()
            })
            .collect();
        assert!(reads.page("", &firsts[0]).is_err());
        assert!(reads.page("", &firsts[HELD_READS]).is_ok());
    }

    #[test]
    fn a_file_that_is_not_a_meta_is_refused() {
        assert!(parse("uTank2 NAV 1.2\n1\n0\n").is_err());
        assert!(parse("1\nCondAct\n5\n").is_err());
    }
}
