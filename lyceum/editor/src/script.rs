//! The scripted mode: what an off-screen run does, frame by frame.
//!
//! ```toml
//! [[step]]
//! frame = 2
//! look_at = { landblock = "A9B4", at = [24.0, 24.0, 60.0], distance = 40.0, yaw = 0.0, pitch = -35.0 }
//!
//! [[step]]
//! frame = 4
//! mode = "select"                  # select, place, terrain, spawn
//! pointer_at = { landblock = "A9B4", at = [24.0, 24.0, 61.0] }   # where that point is drawn
//! press = true                     # the left button down (false: up)
//!
//! [[step]]
//! frame = 6
//! edit = { do = "brush", brush = "raise", amount = 2, landblock = "A9B4", at = [0.0, 72.0], radius = 30.0 }
//! ```
//!
//! A step's keys, each optional, happen in this order: `camera` (`landblock`, `at`, `yaw` and
//! `pitch` in degrees) or `look_at` (the camera put `distance` meters from a point, looking at it);
//! `walk` (true walks with a body, false flies); `mode`; `model` (the static the place tool
//! places); `filter` (what the model browser is typed to find); `shelf` (the browser's category
//! by name, `Tree`, `Building piece`, or `all`); `show` (a tab brought forward: `definition`,
//! `characters`, `quests` or `loot`);
//! `weenie` (the spawn tool's: a symbol or a class id); `dungeon` (`new`: a new dungeon near the
//! camera, gone into; `leave`: out of it again); `look` (the dungeon's look: the landblock of the
//! world's dungeon whose look it takes, `0163`); `piece` (the dungeon kit's, by its label,
//! `0094/0`); `extend` (that piece placed so many times, each at the first free portal of the
//! pack's cells there); `fit` (of what fits at the dungeon's first free doorway, or on its middle
//! square when it has no room yet, the best of a kind: `corridor`, `corner`, `tee`, `cross`,
//! `dead end`, `stairs`, or `any`; `dead ends` closes every doorway left free with one); `inside` (the camera put in the middle of the pack's cell of that symbol in
//! the camera's landblock); `brush` (`kind`, `amount`, `value`, `radius`); `select` (`landblock` and
//! `static` or `index`); `pointer` (pixels) or `pointer_at` (`at` with no height is `lift` meters
//! over the ground); `press`; `drag` (`handle`: `x`, `y`, `z` or `turn`, and `by`: pixels along an
//! arrow, degrees round the ring), which takes the frames after it; `key` (a key pressed and let
//! go: `delete`, `escape`, `undo`, `redo`, `save`); `new_weenie` (`name`, `from`: a server class,
//! `blank`: a blank of it rather than a copy); `edit` (an edit as `lyceum edit` reads it);
//! `live` (true starts the editor's own world and waits for it, false stops it); `export`
//! (`empyrean`, `ace` or `gdle`: the world written out for that server, waited for;
//! `export_gdle = true` is `export = "gdle"`; `gdle_quests`, GDLE's own quests.json, which an
//! export for GDLE merges the world's quests into); `play` (the
//! Dereth client started in
//! that world at the camera, with these switches after the editor's, waited for). The frames after the last step run with nothing new happening.

use dereth_primitives::Vec3;
use lyceum_foundry::brush::Brush;
use lyceum_foundry::script::StaticName;

/// Where to put the camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Camera {
    /// At `at` (landblock meters), turned `yaw` and pitched `pitch` (degrees).
    At {
        landblock: u16,
        at: Vec3,
        yaw: f32,
        pitch: f32,
    },
    /// `distance` meters from `at`, looking at it along `yaw` and `pitch`.
    LookAt {
        landblock: u16,
        at: Vec3,
        distance: f32,
        yaw: f32,
        pitch: f32,
    },
}

/// Where the pointer goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointer {
    Pixel(f32, f32),
    /// Where a point of the world (landblock meters) is drawn; `lift` meters over the ground there
    /// when the point gives no height.
    At {
        landblock: u16,
        at: Vec3,
        lift: Option<f32>,
    },
}

/// The brush settings a step chooses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushChoice {
    pub brush: Brush,
    pub radius: f32,
}

/// The spawn tool's weenie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeenieChoice {
    Symbol(String),
    Class(u32),
}

/// One step of a script.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Step {
    pub frame: u64,
    pub camera: Option<Camera>,
    pub walk: Option<bool>,
    pub mode: Option<String>,
    pub model: Option<u32>,
    pub filter: Option<String>,
    pub shelf: Option<String>,
    /// A tab brought forward: `weenie`.
    pub show: Option<String>,
    pub weenie: Option<WeenieChoice>,
    /// A new dungeon near the camera gone into (`new`), or left (`leave`); and its look, by the
    /// landblock of the world's dungeon whose look it takes.
    pub dungeon: Option<String>,
    pub look: Option<u16>,
    /// The dungeon kit's piece, by its label; and how many of it to place, each at the first free
    /// portal.
    pub piece: Option<String>,
    pub extend: Option<u32>,
    /// The best of what fits at the dungeon's first free doorway, of a kind by its name.
    pub fit: Option<String>,
    /// The camera put in the middle of the pack's cell of this symbol, in its landblock.
    pub inside: Option<String>,
    pub brush: Option<BrushChoice>,
    pub select: Option<(u16, StaticName)>,
    pub pointer: Option<Pointer>,
    pub press: Option<bool>,
    pub key: Option<String>,
    /// A drag of the selected static's or spawn's handle over the frames that follow.
    pub drag: Option<Drag>,
    /// A new weenie: its name, the server class it is made from, and whether as a blank.
    pub new_weenie: Option<(String, u32, bool)>,
    /// An edit, as its table: read against the pack being edited when it runs.
    pub edit: Option<toml::Table>,
    /// The editor's own world started (and waited for) or stopped.
    pub live: Option<bool>,
    /// The world written out for a server, waited for.
    pub export: Option<lyceum_foundry::export::Target>,
    /// GDLE's own quests.json, which an export for GDLE merges the world's quests into.
    pub gdle_quests: Option<String>,
    /// The character recipe selected, which `play` plays as.
    pub character: Option<String>,
    /// Make the selected recipe's character from its recipe now, as Reset does.
    pub reset_character: Option<bool>,
    /// The client started in it with these switches, waited for.
    pub play: Option<Vec<String>>,
}

/// A drag of a handle of the selected static or spawn, as a hand would make it over a few frames:
/// an arrow taken part way along and pulled `pixels` further along its direction on screen, or the
/// ring taken and pulled `degrees` round.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub handle: crate::gizmo::Handle,
    /// How far along the arrow on screen, in pixels (negative back toward the static); for the
    /// ring, how far round, in degrees of the ring as drawn.
    pub by: f32,
}

fn landblock(t: &toml::Table) -> Result<u16, String> {
    let s = t
        .get("landblock")
        .and_then(toml::Value::as_str)
        .ok_or("names its landblock")?;
    u16::from_str_radix(s, 16)
        .ok()
        .filter(|_| s.len() == 4)
        .ok_or_else(|| format!("{s:?} is not a landblock"))
}

fn num(v: &toml::Value) -> Option<f32> {
    match v {
        #[allow(clippy::cast_precision_loss)]
        toml::Value::Integer(i) => Some(*i as f32),
        #[allow(clippy::cast_possible_truncation)]
        toml::Value::Float(f) => Some(*f as f32),
        _ => None,
    }
}

fn get(t: &toml::Table, k: &str) -> Option<f32> {
    t.get(k).and_then(num)
}

fn point(t: &toml::Table) -> Result<Vec3, String> {
    let a = t
        .get("at")
        .and_then(toml::Value::as_array)
        .ok_or("at = [x, y, z]")?;
    let n: Vec<f32> = a.iter().filter_map(num).collect();
    match n[..] {
        [x, y, z] => Ok(Vec3::new(x, y, z)),
        [x, y] => Ok(Vec3::new(x, y, 0.0)),
        _ => Err("at = [x, y, z]".into()),
    }
}

fn table<'t>(t: &'t toml::Table, k: &str) -> Result<Option<&'t toml::Table>, String> {
    match t.get(k) {
        None => Ok(None),
        Some(toml::Value::Table(x)) => Ok(Some(x)),
        Some(_) => Err(format!("{k} is a table")),
    }
}

fn step(t: &toml::Table, pack: &str) -> Result<Step, String> {
    let frame = t
        .get("frame")
        .and_then(toml::Value::as_integer)
        .and_then(|f| u64::try_from(f).ok())
        .ok_or("a step has its frame")?;
    let mut s = Step {
        frame,
        ..Step::default()
    };
    if let Some(c) = table(t, "camera")? {
        s.camera = Some(Camera::At {
            landblock: landblock(c)?,
            at: point(c)?,
            yaw: get(c, "yaw").unwrap_or(0.0),
            pitch: get(c, "pitch").unwrap_or(0.0),
        });
    }
    if let Some(c) = table(t, "look_at")? {
        s.camera = Some(Camera::LookAt {
            landblock: landblock(c)?,
            at: point(c)?,
            distance: get(c, "distance").unwrap_or(30.0),
            yaw: get(c, "yaw").unwrap_or(0.0),
            pitch: get(c, "pitch").unwrap_or(-35.0),
        });
    }
    s.walk = t.get("walk").and_then(toml::Value::as_bool);
    s.mode = t
        .get("mode")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.model = t
        .get("model")
        .and_then(toml::Value::as_integer)
        .and_then(|m| u32::try_from(m).ok());
    s.filter = t
        .get("filter")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.shelf = t
        .get("shelf")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.show = t
        .get("show")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.weenie = match t.get("weenie") {
        None => None,
        Some(toml::Value::String(w)) => Some(WeenieChoice::Symbol(
            lyceum_foundry::symbol::qualify(pack, w)?,
        )),
        Some(toml::Value::Integer(i)) => Some(WeenieChoice::Class(
            u32::try_from(*i).map_err(|e| e.to_string())?,
        )),
        Some(_) => return Err("weenie is a symbol or a class id".into()),
    };
    s.dungeon = t
        .get("dungeon")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    if let Some(l) = t.get("look").and_then(toml::Value::as_str) {
        s.look = Some(
            u16::from_str_radix(l, 16)
                .ok()
                .filter(|_| l.len() == 4)
                .ok_or_else(|| format!("{l:?} is not a landblock"))?,
        );
    }
    s.fit = t
        .get("fit")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.piece = t
        .get("piece")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.extend = t
        .get("extend")
        .and_then(toml::Value::as_integer)
        .and_then(|n| u32::try_from(n).ok());
    s.inside = t
        .get("inside")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    if let Some(b) = table(t, "brush")? {
        let kind = b
            .get("kind")
            .and_then(toml::Value::as_str)
            .ok_or("brush kind")?;
        let amount = || {
            u8::try_from(
                b.get("amount")
                    .and_then(toml::Value::as_integer)
                    .unwrap_or(1),
            )
            .map_err(|e| e.to_string())
        };
        let value = || {
            u16::try_from(
                b.get("value")
                    .and_then(toml::Value::as_integer)
                    .unwrap_or(0),
            )
            .map_err(|e| e.to_string())
        };
        let brush = match kind {
            "raise" => Brush::Raise(amount()?),
            "lower" => Brush::Lower(amount()?),
            "flatten" => Brush::Flatten(
                b.get("value")
                    .and_then(toml::Value::as_integer)
                    .and_then(|v| u8::try_from(v).ok()),
            ),
            "smooth" => Brush::Smooth,
            "terrain" => Brush::Terrain(value()?),
            "scene" => Brush::Scene(value()?),
            "road" => Brush::Road(value()?),
            other => return Err(format!("{other:?} is not a brush")),
        };
        s.brush = Some(BrushChoice {
            brush,
            radius: get(b, "radius").unwrap_or(24.0),
        });
    }
    if let Some(sel) = table(t, "select")? {
        let name = match (sel.get("static"), sel.get("index")) {
            (Some(toml::Value::String(n)), None) => {
                StaticName::Symbol(lyceum_foundry::symbol::qualify(pack, n)?)
            }
            (None, Some(toml::Value::Integer(i))) => {
                StaticName::Index(usize::try_from(*i).map_err(|e| e.to_string())?)
            }
            _ => return Err("select names static = \"symbol\" or index = n".into()),
        };
        s.select = Some((landblock(sel)?, name));
    }
    if let Some(p) = t.get("pointer") {
        let n: Vec<f32> = p
            .as_array()
            .ok_or("pointer = [x, y]")?
            .iter()
            .filter_map(num)
            .collect();
        let [x, y] = n[..] else {
            return Err("pointer = [x, y]".into());
        };
        s.pointer = Some(Pointer::Pixel(x, y));
    }
    if let Some(p) = table(t, "pointer_at")? {
        let two = p
            .get("at")
            .and_then(toml::Value::as_array)
            .is_some_and(|a| a.len() == 2);
        s.pointer = Some(Pointer::At {
            landblock: landblock(p)?,
            at: point(p)?,
            lift: two.then(|| get(p, "lift").unwrap_or(0.0)),
        });
    }
    s.press = t.get("press").and_then(toml::Value::as_bool);
    if let Some(d) = table(t, "drag")? {
        use crate::gizmo::Handle;
        let handle = match d.get("handle").and_then(toml::Value::as_str) {
            Some("x") => Handle::X,
            Some("y") => Handle::Y,
            Some("z") => Handle::Z,
            Some("turn") => Handle::Turn,
            _ => return Err("a drag takes a handle: x, y, z or turn".into()),
        };
        s.drag = Some(Drag {
            handle,
            by: get(d, "by").ok_or("a drag says how far: by")?,
        });
    }
    s.key = t
        .get("key")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    if let Some(n) = table(t, "new_weenie")? {
        s.new_weenie = Some((
            n.get("name")
                .and_then(toml::Value::as_str)
                .ok_or("a new weenie has its name")?
                .to_owned(),
            n.get("from")
                .and_then(toml::Value::as_integer)
                .and_then(|c| u32::try_from(c).ok())
                .ok_or("a new weenie is made from a server class: from")?,
            n.get("blank")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
        ));
    }
    s.edit = table(t, "edit")?.cloned();
    s.live = t.get("live").and_then(toml::Value::as_bool);
    s.character = t
        .get("character")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    s.reset_character = t.get("reset_character").and_then(toml::Value::as_bool);
    s.export = match t.get("export") {
        Some(v) => Some(
            v.as_str()
                .and_then(lyceum_foundry::export::Target::parse)
                .ok_or("export is empyrean, ace or gdle")?,
        ),
        None if t.get("export_gdle").and_then(toml::Value::as_bool) == Some(true) => {
            Some(lyceum_foundry::export::Target::Gdle)
        }
        None => None,
    };
    s.gdle_quests = match t.get("gdle_quests") {
        Some(v) => Some(v.as_str().ok_or("gdle_quests names a file")?.to_owned()),
        None => None,
    };
    if let Some(p) = t.get("play") {
        s.play = Some(
            p.as_array()
                .ok_or("play = [switches]")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or("play's switches are strings")
                })
                .collect::<Result<_, _>>()?,
        );
    }
    Ok(s)
}

/// Every step of `text`, in frame order; `pack` is the pack edits are written for.
///
/// # Errors
/// Text that is not TOML, or a step that does not read, with its place.
pub fn steps(text: &str, pack: &str) -> Result<Vec<Step>, String> {
    let doc: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(list) = doc.get("step") else {
        return Ok(Vec::new());
    };
    let mut out = list
        .as_array()
        .ok_or("steps are [[step]] tables")?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_table()
                .ok_or_else(|| format!("step {}: not a table", i + 1))
                .and_then(|t| step(t, pack).map_err(|why| format!("step {}: {why}", i + 1)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.sort_by_key(|s| s.frame);
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's scripted mode)
    use super::*;

    #[test]
    fn a_script_builds_a_dungeon_with_what_fits_in_a_look() {
        let s = steps(
            r#"
[[step]]
frame = 3
dungeon = "new"
look = "0163"
fit = "corridor"

[[step]]
frame = 5
fit = "dead ends"
dungeon = "leave"
"#,
            "holtburg",
        )
        .unwrap();
        assert_eq!(
            (s[0].dungeon.as_deref(), s[0].look, s[0].fit.as_deref()),
            (Some("new"), Some(0x0163), Some("corridor"))
        );
        assert_eq!(
            (s[1].dungeon.as_deref(), s[1].fit.as_deref()),
            (Some("leave"), Some("dead ends"))
        );
        assert!(steps("[[step]]\nframe = 1\nlook = \"163\"\n", "holtburg").is_err());
    }

    #[test]
    fn a_script_reads_its_steps_in_frame_order() {
        let s = steps(
            r#"
[[step]]
frame = 9
pointer_at = { landblock = "A9B4", at = [24.0, 24.0, 61.0] }
press = true
key = "delete"

[[step]]
frame = 2
look_at = { landblock = "A9B4", at = [24.0, 24.0, 60.0], distance = 40.0 }
mode = "terrain"
brush = { kind = "raise", amount = 2, radius = 30.0 }
select = { landblock = "A9B4", static = "stone-west" }
weenie = "guide"
filter = "gribble"
"#,
            "holtburg",
        )
        .unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].frame, 2);
        assert_eq!(s[0].filter.as_deref(), Some("gribble"));
        assert_eq!(
            s[0].brush,
            Some(BrushChoice {
                brush: Brush::Raise(2),
                radius: 30.0
            })
        );
        assert_eq!(
            s[0].select,
            Some((0xA9B4, StaticName::Symbol("holtburg:stone-west".into())))
        );
        assert_eq!(
            s[0].weenie,
            Some(WeenieChoice::Symbol("holtburg:guide".into()))
        );
        assert_eq!(s[1].press, Some(true));
        assert_eq!(s[1].key.as_deref(), Some("delete"));
        assert!(matches!(
            s[1].pointer,
            Some(Pointer::At {
                landblock: 0xA9B4,
                ..
            })
        ));
    }

    #[test]
    fn a_script_exports_the_world_for_the_server_it_names() {
        let s = steps(
            "[[step]]
frame = 2
export = \"ace\"
",
            "p",
        )
        .unwrap();
        assert_eq!(s[0].export, Some(lyceum_foundry::export::Target::Ace));
        let e = steps(
            "[[step]]
frame = 2
export = \"mysql\"
",
            "p",
        )
        .unwrap_err();
        assert!(e.contains("empyrean, ace or gdle"), "{e}");
    }

    #[test]
    fn a_script_starts_the_editors_world_makes_a_weenie_and_plays() {
        let s = steps(
            r#"
[[step]]
frame = 3
live = true
export_gdle = true
gdle_quests = "server/quests.json"
reset_character = true
character = "Level Eighty"
new_weenie = { name = "Gate Guard", from = 1154, blank = true }

[[step]]
frame = 9
play = ["--headless", "--linger", "20"]
live = false
"#,
            "p",
        )
        .unwrap();
        assert_eq!(s[0].live, Some(true));
        assert_eq!(s[0].new_weenie, Some(("Gate Guard".into(), 1154, true)));
        assert_eq!(s[0].export, Some(lyceum_foundry::export::Target::Gdle));
        assert_eq!(s[0].gdle_quests.as_deref(), Some("server/quests.json"));
        assert_eq!(s[0].reset_character, Some(true));
        assert_eq!(s[0].character.as_deref(), Some("Level Eighty"));
        assert_eq!(
            s[1].play.as_deref(),
            Some(&["--headless".to_owned(), "--linger".into(), "20".into()][..])
        );
        assert_eq!(s[1].live, Some(false));
        assert!(steps("[[step]]\nframe = 1\nplay = [1]\n", "p").is_err());
        assert!(steps("[[step]]\nframe = 1\nnew_weenie = { name = \"x\" }\n", "p").is_err());
    }

    #[test]
    fn a_step_that_does_not_read_says_which() {
        let e = steps(
            "[[step]]\nframe = 1\n\n[[step]]\nbrush = { kind = \"melt\" }\nframe = 2\n",
            "p",
        )
        .unwrap_err();
        assert!(e.starts_with("step 2:"), "{e}");
    }
}
