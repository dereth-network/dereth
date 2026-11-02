//! The music's scripts: routines a piece's script track calls, written in the music's small Basic
//! language. The scripts the game holds are of one shape: read the master groove level into a
//! variable, compare it with numbers in `If`/`ElseIf`/`Else` blocks, and play pieces the script
//! holds by name. This runs that shape; a line of any other shape is refused when the script is
//! read, so a script it cannot run is known.

/// One statement.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Stmt {
    /// `x = GetMasterGrooveLevel()`.
    ReadGroove(String),
    /// `If`/`ElseIf` arms (a variable, a comparison, a number, and the statements), then `Else`.
    If {
        arms: Vec<(Cond, Vec<Stmt>)>,
        otherwise: Vec<Stmt>,
    },
    /// `Name.Play(flags)`.
    Play { name: String, flags: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cond {
    var: String,
    op: String,
    value: i32,
}

/// A piece a routine plays: the name the script holds it by, and its play flags' names (such as
/// `AtFinish`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayCall {
    pub name: String,
    pub flags: Vec<String>,
}

/// A script's routines, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routines {
    routines: Vec<(String, Vec<Stmt>)>,
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '\'' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

fn cond(text: &str) -> Option<Cond> {
    for op in ["<=", ">=", "<>", "<", ">", "="] {
        if let Some((l, r)) = text.split_once(op) {
            return Some(Cond {
                var: l.trim().to_lowercase(),
                op: op.to_owned(),
                value: r.trim().parse().ok()?,
            });
        }
    }
    None
}

/// Read a script's text.
///
/// # Errors
/// A line of a shape this does not run, with its number.
pub fn parse(source: &str) -> Result<Routines, String> {
    let lines: Vec<(usize, String)> = source
        .lines()
        .enumerate()
        .map(|(n, l)| (n + 1, strip_comment(l).trim().to_owned()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let mut routines = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (n, l) = &lines[i];
        let lower = l.to_lowercase();
        if lower.starts_with("dim ") {
            i += 1;
        } else if let Some(rest) = lower.strip_prefix("sub ") {
            let name = rest.trim_end_matches("()").trim().to_owned();
            i += 1;
            let body = block(&lines, &mut i, &["end sub"])?;
            i += 1;
            routines.push((name, body));
        } else {
            return Err(format!("line {n}: {l}"));
        }
    }
    Ok(Routines { routines })
}

/// Statements until a line that starts one of `ends` (left unread).
fn block(lines: &[(usize, String)], i: &mut usize, ends: &[&str]) -> Result<Vec<Stmt>, String> {
    let mut out = Vec::new();
    while *i < lines.len() {
        let (n, l) = &lines[*i];
        let lower = l.to_lowercase();
        if ends.iter().any(|e| lower.starts_with(e)) {
            return Ok(out);
        }
        let bad = || format!("line {n}: {l}");
        if let Some(rest) = lower.strip_prefix("if ") {
            let c = cond(rest.strip_suffix(" then").ok_or_else(bad)?).ok_or_else(bad)?;
            *i += 1;
            let mut arms = vec![(c, block(lines, i, &["elseif ", "else", "end if"])?)];
            let mut otherwise = Vec::new();
            loop {
                let (n2, l2) = lines.get(*i).ok_or_else(bad)?;
                let lower2 = l2.to_lowercase();
                if let Some(rest) = lower2.strip_prefix("elseif ") {
                    let bad2 = || format!("line {n2}: {l2}");
                    let c = cond(rest.strip_suffix(" then").ok_or_else(bad2)?).ok_or_else(bad2)?;
                    *i += 1;
                    arms.push((c, block(lines, i, &["elseif ", "else", "end if"])?));
                } else if lower2 == "else" {
                    *i += 1;
                    otherwise = block(lines, i, &["end if"])?;
                } else {
                    *i += 1;
                    break;
                }
            }
            out.push(Stmt::If { arms, otherwise });
        } else if let Some((var, rhs)) = l.split_once('=') {
            if rhs.trim().to_lowercase() != "getmastergroovelevel()" {
                return Err(bad());
            }
            out.push(Stmt::ReadGroove(var.trim().to_lowercase()));
            *i += 1;
        } else if let Some((name, rest)) = l.split_once('.') {
            let r = rest.trim();
            if !r.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("play")) {
                return Err(bad());
            }
            let args = r[4..].trim();
            let args = args
                .strip_prefix('(')
                .and_then(|a| a.strip_suffix(')'))
                .unwrap_or(args);
            let flags = args
                .split(',')
                .map(|f| f.trim().to_owned())
                .filter(|f| !f.is_empty())
                .collect();
            out.push(Stmt::Play {
                name: name.trim().to_owned(),
                flags,
            });
            *i += 1;
        } else {
            return Err(bad());
        }
    }
    Err("a block runs past the script's end".to_owned())
}

impl Routines {
    /// The routines' names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.routines.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// Run a routine with the master groove level at `groove`: the pieces it plays, in order.
    /// `None` when there is no such routine.
    #[must_use]
    pub fn run(&self, routine: &str, groove: i32) -> Option<Vec<PlayCall>> {
        let body = &self
            .routines
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(routine))?
            .1;
        let mut vars: Vec<(String, i32)> = Vec::new();
        let mut out = Vec::new();
        exec(body, groove, &mut vars, &mut out);
        Some(out)
    }
}

fn exec(body: &[Stmt], groove: i32, vars: &mut Vec<(String, i32)>, out: &mut Vec<PlayCall>) {
    for s in body {
        match s {
            Stmt::ReadGroove(v) => {
                vars.retain(|(n, _)| n != v);
                vars.push((v.clone(), groove));
            }
            Stmt::Play { name, flags } => out.push(PlayCall {
                name: name.clone(),
                flags: flags.clone(),
            }),
            Stmt::If { arms, otherwise } => {
                let pick = arms.iter().find(|(c, _)| {
                    // An unset variable is empty, which compares as 0.
                    let x = vars
                        .iter()
                        .find(|(n, _)| *n == c.var)
                        .map_or(0, |(_, v)| *v);
                    match c.op.as_str() {
                        "<" => x < c.value,
                        ">" => x > c.value,
                        "<=" => x <= c.value,
                        ">=" => x >= c.value,
                        "<>" => x != c.value,
                        _ => x == c.value,
                    }
                });
                match pick {
                    Some((_, b)) => exec(b, groove, vars, out),
                    None => exec(otherwise, groove, vars, out),
                }
            }
        }
    }
}
