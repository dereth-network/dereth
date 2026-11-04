//! A script of editor steps, one a line, run in order as frames pass: for showing the editor at
//! work and for trying it the same way twice. `#` starts a comment.
//!
//! ```text
//! wait SECONDS                       let frames pass
//! camera XXYY X Y Z YAW PITCH        put the camera at a point of a landblock (degrees)
//! fly ITEM                           fly the camera to a row
//! select ITEM | none                 select a row
//! place KIND TEMPLATE XXYY X Y [H]   place a row (person, static, camp, place) on the ground
//! move ITEM XXYY X Y                 move a row onto the ground there
//! turn ITEM HEADING                  turn a row
//! set ITEM FIELD VALUE...            set a field, the rest of the line its TOML value
//! delete ITEM | putback ITEM | undo | redo | save | check   (check: build against the install)
//! find WORDS                         open the template finder on a name
//! character NAME RACE SEX START [hero]  write a playtest character's recipe and choose it
//!                                    (START: camera, selection, left, or place KEY)
//! recipe NAME KEY = VALUE            set a field of a recipe, the rest of the line a TOML line
//!                                    (`recipe Scout level = 10`, `recipe Scout items = [...]`)
//! as NAME | reset NAME               choose the character Play plays as; make it anew
//! export NAME                        write a character to the world's build/characters
//! import FILE [NAME]                 bring a character in from another world's file
//! characters                         open or close the Characters window
//! live | play | play-retail | stop   the editor's own Yalain, and a client in it (serve: live)
//! teleport                           send the character in the world to the camera
//! restart                            start Yalain again, the character where it stands
//! point X Y                          the pointer to a pixel of the window
//! button left|right down|up           a mouse button goes down / up
//! click X Y | drag X Y X2 Y2          the pointer there, and the left button down and up
//! wheel NOTCHES                      the mouse's wheel
//! key KEY [ctrl] [shift]              a key pressed, as `KeyR`, `Delete` or `Digit1`
//! type TEXT                          the rest of the line typed
//! shot FILE                          save the window's picture
//! close                              what the window's close button does
//! quit
//! ```
//!
//! An item is written as the panels name it, with `_` for its space (`object_person-1@77B8`), and
//! a character's name with `_` for each of its spaces.

use std::path::{Path, PathBuf};

use crate::characters::{Recipe, Start, RACES, SEXES};

/// One step.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Wait(f64),
    Camera(u16, [f32; 3], f32, f32),
    Fly(String),
    Select(Option<String>),
    Place(String, String, u16, f32, f32, f32),
    Move(String, u16, f32, f32),
    Turn(String, f32),
    Set(String, String, String),
    Delete(String),
    PutBack(String),
    Undo,
    Redo,
    Save,
    Check,
    Find(String),
    Serve,
    Play(bool),
    Stop,
    Restart,
    Character(Recipe),
    /// A recipe's name, and a TOML line setting one of its fields.
    Recipe(String, String),
    As(String),
    Reset(String),
    Export(String),
    /// A character file, and the name it comes in under (its own when empty).
    Import(PathBuf, String),
    Teleport,
    Characters,
    Point(f32, f32),
    /// The left button or the right, down or up.
    Button(bool, bool),
    Wheel(f32),
    /// A key by its name, with Ctrl and with Shift.
    Key(String, bool, bool),
    Type(String),
    Shot(PathBuf),
    Close,
    Quit,
}

/// How long a script's click or drag keeps to each of its stages, seconds.
const PRESS: f64 = 0.1;

fn item(s: &str) -> String {
    s.replacen('_', " ", 1)
}

fn lb(s: &str) -> Result<u16, String> {
    u16::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

fn num(s: &str) -> Result<f32, String> {
    s.parse().map_err(|e| format!("{s}: {e}"))
}

fn name(s: &str) -> String {
    s.replace('_', " ")
}

/// A `character` line's recipe: its name, race, sex and start, and `hero` after them.
fn recipe(w: &[&str]) -> Result<Recipe, String> {
    let [n, race, sex, rest @ ..] = w else {
        return Err("character NAME RACE SEX START [hero]".into());
    };
    let pick = |table: &[(&str, u32)], v: &str| {
        table
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(v))
            .map(|(_, x)| *x)
            .ok_or_else(|| format!("{v}: not one of {table:?}"))
    };
    let mut r = Recipe::new(&name(n));
    r.race = pick(&RACES, race)?;
    r.sex = pick(&SEXES, sex)?;
    let (start, rest) = match rest {
        ["place", key, rest @ ..] => (format!("place {key}"), rest),
        [start, rest @ ..] => ((*start).to_string(), rest),
        [] => ("camera".to_string(), rest),
    };
    r.start = Start::parse(&start)?;
    r.hero = rest == ["hero"];
    Ok(r)
}

/// Read a script's text.
///
/// # Errors
/// A line that is not a step, with its number.
pub fn parse(text: &str, dir: &Path) -> Result<Vec<Step>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        let bad = |why: String| format!("line {}: {why}", n + 1);
        let step = match w.as_slice() {
            ["wait", s] => Step::Wait(s.parse().map_err(|e| bad(format!("{e}")))?),
            ["camera", b, x, y, z, yaw, pitch] => Step::Camera(
                lb(b).map_err(bad)?,
                [
                    num(x).map_err(bad)?,
                    num(y).map_err(bad)?,
                    num(z).map_err(bad)?,
                ],
                num(yaw).map_err(bad)?,
                num(pitch).map_err(bad)?,
            ),
            ["fly", i] => Step::Fly(item(i)),
            ["select", "none"] => Step::Select(None),
            ["select", i] => Step::Select(Some(item(i))),
            ["place", k, t, b, x, y, rest @ ..] => Step::Place(
                (*k).to_string(),
                (*t).to_string(),
                lb(b).map_err(bad)?,
                num(x).map_err(bad)?,
                num(y).map_err(bad)?,
                rest.first().map_or(Ok(0.0), |h| num(h)).map_err(bad)?,
            ),
            ["move", i, b, x, y] => Step::Move(
                item(i),
                lb(b).map_err(bad)?,
                num(x).map_err(bad)?,
                num(y).map_err(bad)?,
            ),
            ["turn", i, h] => Step::Turn(item(i), num(h).map_err(bad)?),
            ["set", i, f, v @ ..] => Step::Set(item(i), (*f).to_string(), v.join(" ")),
            ["delete", i] => Step::Delete(item(i)),
            ["putback", i] => Step::PutBack(item(i)),
            ["undo"] => Step::Undo,
            ["redo"] => Step::Redo,
            ["save"] => Step::Save,
            ["check"] => Step::Check,
            ["find", w @ ..] => Step::Find(w.join(" ")),
            ["serve" | "live"] => Step::Serve,
            ["play"] => Step::Play(false),
            ["play-retail"] => Step::Play(true),
            ["stop"] => Step::Stop,
            ["restart"] => Step::Restart,
            ["teleport"] => Step::Teleport,
            ["character", rest @ ..] => Step::Character(recipe(rest).map_err(bad)?),
            ["as", n] => Step::As(name(n)),
            ["reset", n] => Step::Reset(name(n)),
            ["recipe", n, rest @ ..] if rest.contains(&"=") => {
                Step::Recipe(name(n), rest.join(" "))
            }
            ["export", n] => Step::Export(name(n)),
            ["import", f] => Step::Import(PathBuf::from(f), String::new()),
            ["import", f, n] => Step::Import(PathBuf::from(f), name(n)),
            ["characters"] => Step::Characters,
            ["point", x, y] => Step::Point(num(x).map_err(bad)?, num(y).map_err(bad)?),
            ["button", b @ ("left" | "right"), d @ ("down" | "up")] => {
                Step::Button(*b == "left", *d == "down")
            }
            ["click", x, y] => {
                let at = Step::Point(num(x).map_err(bad)?, num(y).map_err(bad)?);
                out.extend([
                    at,
                    Step::Wait(PRESS),
                    Step::Button(true, true),
                    Step::Wait(PRESS),
                ]);
                Step::Button(true, false)
            }
            ["drag", x, y, x2, y2] => {
                let from = Step::Point(num(x).map_err(bad)?, num(y).map_err(bad)?);
                let to = Step::Point(num(x2).map_err(bad)?, num(y2).map_err(bad)?);
                out.extend([
                    from,
                    Step::Wait(PRESS),
                    Step::Button(true, true),
                    Step::Wait(PRESS),
                    to,
                    Step::Wait(PRESS),
                ]);
                Step::Button(true, false)
            }
            ["wheel", n] => Step::Wheel(num(n).map_err(bad)?),
            ["key", k, held @ ..] => Step::Key(
                (*k).to_string(),
                held.contains(&"ctrl"),
                held.contains(&"shift"),
            ),
            ["type", t @ ..] => Step::Type(t.join(" ")),
            ["shot", f] => Step::Shot(dir.join(f)),
            ["close"] => Step::Close,
            ["quit"] => Step::Quit,
            _ => return Err(bad(format!("not a step: {line}"))),
        };
        out.push(step);
    }
    Ok(out)
}

/// A script being run: its steps and when the current wait ends.
#[derive(Debug, Clone, Default)]
pub struct Script {
    steps: Vec<Step>,
    next: usize,
    until: f64,
}

impl Script {
    /// The script in `file`; shots are named relative to `shots`.
    ///
    /// # Errors
    /// The file does not read, or a line is not a step.
    pub fn load(file: &Path, shots: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(Self {
            steps: parse(&text, shots)?,
            next: 0,
            until: 0.0,
        })
    }

    /// The steps due at `now` seconds: every step up to and including the next wait.
    pub fn due(&mut self, now: f64) -> Vec<Step> {
        let mut out = Vec::new();
        if now < self.until {
            return out;
        }
        while let Some(s) = self.steps.get(self.next).cloned() {
            self.next += 1;
            if let Step::Wait(w) = s {
                self.until = now + w;
                break;
            }
            let stop = matches!(s, Step::Shot(_));
            out.push(s);
            if stop {
                // A shot is of the frame drawn after the steps before it.
                break;
            }
        }
        out
    }
}
