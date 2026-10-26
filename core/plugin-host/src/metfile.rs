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
        let mut cells = Vec::new();
        for _ in 0..rows {
            let row: Vec<Value> = names
                .iter()
                .map(|_| self.value())
                .collect::<Result<_, _>>()?;
            cells.push(row);
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
            _ => Value::List(
                cells
                    .into_iter()
                    .map(|row| {
                        let mut m = Value::map();
                        for (name, v) in names.iter().zip(row) {
                            m = m.with(name, v);
                        }
                        m
                    })
                    .collect(),
            ),
        })
    }
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

/// A meta read into `{ rules = { { state, condition = { kind, data }, action = { kind, data } },
/// … } }`, rules in file order. A kind the reader does not know is named `unknown:<n>`. An
/// embedded route's data is `{ name, route, text }` (`error` in place of `route` when it does not
/// read).
///
/// # Errors
/// Not a meta, or a value that does not read as its type.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut r = Reader {
        data: text.as_bytes(),
        at: 0,
    };
    let version = r.line()?.trim().to_owned();
    let name = r.line()?.trim().to_owned();
    if name != "CondAct" {
        return Err(format!(
            "{version:?} then {name:?} is not a meta's header (1, CondAct)"
        ));
    }
    let Value::List(rows) = r.table()? else {
        return Err("a meta's rules are not a table".into());
    };
    let mut rules = Vec::new();
    for row in rows {
        let cond = entry(
            CONDITIONS,
            false,
            row.get("CType"),
            row.get("CData").clone(),
        );
        let act = entry(ACTIONS, true, row.get("AType"), row.get("AData").clone());
        rules.push(
            Value::map()
                .with("state", row.get("State").clone())
                .with("condition", cond)
                .with("action", act),
        );
    }
    Ok(Value::map().with("rules", Value::List(rules)))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the file reader; the import's behaviour is pinned by the
    //! conformance case `automation/met-import`).
    use super::parse;
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

    #[test]
    fn a_file_that_is_not_a_meta_is_refused() {
        assert!(parse("uTank2 NAV 1.2\n1\n0\n").is_err());
        assert!(parse("1\nCondAct\n5\n").is_err());
    }
}
