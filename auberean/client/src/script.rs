//! A developer's script of input (`--dev-script FILE`): keys, buttons and mouse motion handed to the
//! shell at set moments, and screenshots of the window, for trying the world the same way twice.
//! Never a player's.
//!
//! One step a line; `#` starts a comment:
//!
//! ```text
//! wait SECONDS          wait
//! until world           wait until the player is in the world (or `characters`, `disconnected`)
//! press KEY             the key goes down this frame and up the next
//! hold KEY SECONDS      the key is held down for that long
//! down KEY / up KEY     the key goes down / up
//! click left|right|middle
//! button left|right|middle down|up
//!                       a mouse button goes down / up (held between, as for a drag)
//! point X Y             the pointer to a point of the window, in pixels from its top left
//! type TEXT             the rest of the line typed, as the keyboard types text
//! mouse DX DY           mouse motion, in counts
//! wheel NOTCHES
//! shot NAME             save the window's picture as NAME.png in the screenshot folder
//! goto BLOCK X Y SECS   run toward a place (block in hex, meters in it), steering with the turn
//!                       keys, until within 3 m or the seconds are up
//! quit
//! ```

use std::path::{Path, PathBuf};

use auberean_scene::Status;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::Phase;

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Wait(f64),
    Until(Phase),
    Press(Key),
    Hold(Key, f64),
    Down(Key),
    Up(Key),
    Click(Button),
    ButtonTo(Button, bool),
    Point(f32, f32),
    Type(String),
    Mouse(f32, f32),
    Wheel(f32),
    Shot(String),
    Goto(f32, f32, f64),
    Quit,
}

/// A script being played.
#[derive(Debug)]
pub struct Script {
    steps: Vec<Step>,
    at: usize,
    /// Until when the current step waits.
    until: Option<f64>,
    /// Keys to let up on the next frame.
    release: Vec<Key>,
    buttons: Vec<Button>,
    shots: PathBuf,
}

/// What the script asks of the host this frame.
#[derive(Debug, Default)]
pub struct Asked {
    pub events: Vec<Event>,
    pub shot: Option<PathBuf>,
    pub quit: bool,
}

fn parse(text: &str) -> Result<Vec<Step>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        let bad = || format!("line {}: {line}", n + 1);
        let key = |s: Option<&&str>| s.and_then(|s| crate::keys::named(s)).ok_or_else(bad);
        let button = |s: Option<&&str>| match s.copied() {
            Some("left") => Ok(Button::Left),
            Some("right") => Ok(Button::Right),
            Some("middle") => Ok(Button::Middle),
            _ => Err(bad()),
        };
        let num = |s: Option<&&str>| {
            s.and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| format!("line {}: {line}", n + 1))
        };
        out.push(match w[0] {
            "wait" => Step::Wait(num(w.get(1))?),
            "until" => Step::Until(match w.get(1).copied() {
                Some("world") => Phase::World,
                Some("characters") => Phase::Characters,
                Some("disconnected") => Phase::Disconnected,
                _ => return Err(bad()),
            }),
            "press" => Step::Press(key(w.get(1))?),
            "hold" => Step::Hold(key(w.get(1))?, num(w.get(2))?),
            "down" => Step::Down(key(w.get(1))?),
            "up" => Step::Up(key(w.get(1))?),
            "click" => Step::Click(button(w.get(1))?),
            "button" => Step::ButtonTo(
                button(w.get(1))?,
                match w.get(2).copied() {
                    Some("down") => true,
                    Some("up") => false,
                    _ => return Err(bad()),
                },
            ),
            #[allow(clippy::cast_possible_truncation)]
            "mouse" => Step::Mouse(num(w.get(1))? as f32, num(w.get(2))? as f32),
            #[allow(clippy::cast_possible_truncation)]
            "point" => Step::Point(num(w.get(1))? as f32, num(w.get(2))? as f32),
            "type" => Step::Type(
                line.split_once(char::is_whitespace)
                    .map_or("", |(_, t)| t.trim_start())
                    .to_owned(),
            ),
            #[allow(clippy::cast_possible_truncation)]
            "wheel" => Step::Wheel(num(w.get(1))? as f32),
            "shot" => Step::Shot((*w.get(1).ok_or_else(bad)?).to_owned()),
            "goto" => {
                let b = w
                    .get(1)
                    .and_then(|s| u32::from_str_radix(s, 16).ok())
                    .ok_or_else(bad)?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                Step::Goto(
                    (b >> 8) as f32 * 160.0 + num(w.get(2))? as f32,
                    (b & 0xFF) as f32 * 160.0 + num(w.get(3))? as f32,
                    num(w.get(4))?,
                )
            }
            "quit" => Step::Quit,
            _ => return Err(bad()),
        });
    }
    Ok(out)
}

impl Script {
    /// The script in `file`, saving its screenshots in `shots`.
    ///
    /// # Errors
    /// The file does not read, or a line is not a step.
    pub fn load(file: &Path, shots: PathBuf) -> Result<Self, String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(Self {
            steps: parse(&text)?,
            at: 0,
            until: None,
            release: Vec::new(),
            buttons: Vec::new(),
            shots,
        })
    }

    /// What to do at `now`, in `phase`, with the player where `status` says.
    pub fn step(&mut self, now: f64, phase: Phase, status: &Status) -> Asked {
        let mut a = Asked::default();
        for k in self.release.drain(..) {
            a.events.push(Event::Key {
                key: k,
                down: false,
                repeat: false,
            });
        }
        for b in self.buttons.drain(..) {
            a.events.push(Event::Button {
                button: b,
                down: false,
            });
        }
        while let Some(s) = self.steps.get(self.at).cloned() {
            match s {
                Step::Wait(t) => {
                    let until = *self.until.get_or_insert(now + t);
                    if now < until {
                        return a;
                    }
                    self.until = None;
                }
                Step::Until(p) => {
                    if phase != p {
                        return a;
                    }
                }
                Step::Press(k) => {
                    a.events.push(Event::Key {
                        key: k,
                        down: true,
                        repeat: false,
                    });
                    self.release.push(k);
                    self.at += 1;
                    return a;
                }
                Step::Hold(k, t) => match self.until {
                    None => {
                        a.events.push(Event::Key {
                            key: k,
                            down: true,
                            repeat: false,
                        });
                        self.until = Some(now + t);
                        return a;
                    }
                    Some(u) if now < u => return a,
                    Some(_) => {
                        a.events.push(Event::Key {
                            key: k,
                            down: false,
                            repeat: false,
                        });
                        self.until = None;
                    }
                },
                Step::Down(k) => a.events.push(Event::Key {
                    key: k,
                    down: true,
                    repeat: false,
                }),
                Step::Up(k) => a.events.push(Event::Key {
                    key: k,
                    down: false,
                    repeat: false,
                }),
                Step::Click(b) => {
                    a.events.push(Event::Button {
                        button: b,
                        down: true,
                    });
                    self.buttons.push(b);
                    self.at += 1;
                    return a;
                }
                Step::ButtonTo(button, down) => {
                    a.events.push(Event::Button { button, down });
                    self.at += 1;
                    return a;
                }
                Step::Mouse(dx, dy) => a.events.push(Event::Motion { dx, dy }),
                Step::Point(x, y) => a.events.push(Event::Pointer { x, y }),
                Step::Type(t) => a.events.push(Event::Text(t)),
                Step::Wheel(n) => a.events.push(Event::Wheel { notches: n }),
                Step::Shot(name) => {
                    a.shot = Some(self.shots.join(format!("{name}.png")));
                    self.at += 1;
                    return a;
                }
                Step::Goto(x, y, secs) => {
                    let until = *self.until.get_or_insert(now + secs);
                    #[allow(clippy::cast_precision_loss)]
                    let (px, py) = (
                        (status.cell >> 24) as f32 * 160.0 + status.origin.x,
                        ((status.cell >> 16) & 0xFF) as f32 * 160.0 + status.origin.y,
                    );
                    let (dx, dy) = (x - px, y - py);
                    let far = (dx * dx + dy * dy).sqrt();
                    let bearing = dereth_primitives::num::math::atan2f(dx, dy).to_degrees();
                    let off = (bearing - status.heading + 540.0).rem_euclid(360.0) - 180.0;
                    let key = |k: Key, down: bool| Event::Key {
                        key: k,
                        down,
                        repeat: false,
                    };
                    if far < 3.0 || now >= until {
                        a.events.extend([
                            key(Key::W, false),
                            key(Key::A, false),
                            key(Key::D, false),
                        ]);
                        self.until = None;
                    } else {
                        a.events.push(key(Key::W, off.abs() < 60.0));
                        a.events.push(key(Key::A, off < -8.0));
                        a.events.push(key(Key::D, off > 8.0));
                        return a;
                    }
                }
                Step::Quit => {
                    a.quit = true;
                    return a;
                }
            }
            self.at += 1;
        }
        a
    }
}
