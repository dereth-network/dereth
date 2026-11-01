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
//! serve | play | play-retail | stop  Yalain and a client
//! shot FILE                          save the window's picture
//! quit
//! ```
//!
//! An item is written as the panels name it, with `_` for its space (`object_person-1@77B8`).

use std::path::{Path, PathBuf};

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
    Shot(PathBuf),
    Quit,
}

fn item(s: &str) -> String {
    s.replacen('_', " ", 1)
}

fn lb(s: &str) -> Result<u16, String> {
    u16::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

fn num(s: &str) -> Result<f32, String> {
    s.parse().map_err(|e| format!("{s}: {e}"))
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
            ["serve"] => Step::Serve,
            ["play"] => Step::Play(false),
            ["play-retail"] => Step::Play(true),
            ["stop"] => Step::Stop,
            ["shot", f] => Step::Shot(dir.join(f)),
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
