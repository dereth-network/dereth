//! Edits written as text: what `lyceum edit` runs, and what the world editor's scripted
//! mode runs between its frames.
//!
//! ```toml
//! [[edit]]
//! do = "place-static"
//! landblock = "A9B4"
//! model = 0x0200040B
//! at = [24.0, 24.0, "ground"]      # landblock meters; z a height or "ground"
//! heading = 30.0
//!
//! [[edit]]
//! do = "move-static"               # or "remove-static"
//! landblock = "A9B4"
//! static = "stone-west"            # a pack's static by its symbol, or: index = 3 (the base's)
//! at = [26.0, 24.0, "ground"]
//!
//! [[edit]]
//! do = "brush"                     # raise, lower, flatten, smooth, terrain, scene, road
//! brush = "raise"
//! amount = 2                       # raise and lower: entries of the height table
//! value = 4                        # flatten (an entry), terrain, scene, road
//! landblock = "A9B4"
//! at = [30.0, 40.0]                # the center, landblock meters
//! radius = 30.0
//!
//! [[edit]]
//! do = "spawn"
//! weenie = "guide"                 # a pack's weenie by its symbol, or a class id
//! landblock = "A9B4"
//! at = [36.0, 18.0, "ground"]
//! heading = 180.0
//!
//! [[edit]]
//! do = "move-building"             # a building of the landblock's statics record, by its index:
//! landblock = "A9B4"               # its rooms, doors and the spawns inside go with it
//! index = 0
//! at = [60.0, 40.0, 94.0]
//! heading = 90.0
//!
//! [[edit]]
//! do = "add-building"              # a copy of a retail building, from the building catalog:
//! model = 0x01002228               # its rooms, doors and outdoor statics come with it, and the
//! landblock = "A9B4"               # ground under it is made level
//! at = [60.0, 40.0]                # its origin, landblock meters
//! heading = 90.0
//! layout = 0                       # which of the model's layouts; the most common when left out
//! furnished = "bare"               # or another copy's furniture, "3DC7/1"; left out, the copy's
//! house = false                    # a house as a plain building; left out, one players can buy
//!
//! [[edit]]
//! do = "undo"                      # or "redo", "save"
//! ```

use dereth_primitives::{DataId, Vec3};

use crate::brush::{self, Brush};
use crate::compile::StaticOrigin;
use crate::edit::{Applied, Op, Session, Weenie};
use crate::error::Error;
use crate::source::{Height, Place, Rotation};
use crate::symbol;

/// A static an edit names: a pack's by its symbol, or the base's by its index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StaticName {
    Symbol(String),
    Index(usize),
}

/// One edit, as text gives it.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Op(Op),
    /// A static named by symbol or index, moved or removed.
    Move {
        landblock: u16,
        name: StaticName,
        at: Option<Place>,
        rotation: Option<Rotation>,
    },
    Remove {
        landblock: u16,
        name: StaticName,
    },
    /// A building moved, with the spawns that go with it.
    MoveBuilding {
        landblock: u16,
        index: usize,
        at: [f32; 3],
        rotation: Rotation,
    },
    /// A copy of a retail building dropped, from the session's building catalog.
    AddBuilding {
        model: DataId,
        layout: usize,
        furnished: crate::source::Furnished,
        /// For a house: one players can buy, or a plain building.
        house: bool,
        landblock: u16,
        x: f32,
        y: f32,
        heading: f32,
    },
    /// One dab of a brush, centered in landblock meters.
    Brush {
        brush: Brush,
        landblock: u16,
        x: f32,
        y: f32,
        radius: f32,
    },
    Undo,
    Redo,
    Save,
}

fn landblock(t: &toml::Table) -> Result<u16, String> {
    let s = t
        .get("landblock")
        .and_then(toml::Value::as_str)
        .ok_or("an edit names its landblock, as landblock = \"A9B4\"")?;
    u16::from_str_radix(s, 16)
        .ok()
        .filter(|lb| (lb >> 8) < 0xFF && (lb & 0xFF) < 0xFF && s.len() == 4)
        .ok_or_else(|| format!("{s:?} is not a landblock: four hex digits, as A9B4"))
}

fn number(v: &toml::Value) -> Option<f32> {
    match v {
        #[allow(clippy::cast_precision_loss)]
        toml::Value::Integer(i) => Some(*i as f32),
        #[allow(clippy::cast_possible_truncation)]
        toml::Value::Float(f) => Some(*f as f32),
        _ => None,
    }
}

fn place(t: &toml::Table) -> Result<Option<Place>, String> {
    let Some(at) = t.get("at") else {
        return Ok(None);
    };
    let a = at.as_array().ok_or("at is [x, y, z]")?;
    let (Some(x), Some(y)) = (a.first().and_then(number), a.get(1).and_then(number)) else {
        return Err("at is [x, y, z], each a number (z may be \"ground\")".into());
    };
    let z = match a.get(2) {
        None => Height::Ground,
        Some(toml::Value::String(s)) if s == "ground" => Height::Ground,
        Some(v) => Height::At(number(v).ok_or("z is a number or \"ground\"")?),
    };
    Ok(Some(Place { x, y, z }))
}

fn rotation(t: &toml::Table) -> Result<Option<Rotation>, String> {
    if let Some(h) = t.get("heading") {
        return Ok(Some(Rotation::Heading(
            number(h).ok_or("a heading is a number of degrees")?,
        )));
    }
    if let Some(r) = t.get("rotation") {
        let q: Vec<f32> = r
            .as_array()
            .ok_or("a rotation is [w, x, y, z]")?
            .iter()
            .filter_map(number)
            .collect();
        let [w, x, y, z] = q[..] else {
            return Err("a rotation is [w, x, y, z]".into());
        };
        return Ok(Some(Rotation::Quat([w, x, y, z])));
    }
    Ok(None)
}

fn static_name(t: &toml::Table, pack: &str) -> Result<StaticName, String> {
    match (t.get("static"), t.get("index")) {
        (Some(toml::Value::String(s)), None) => symbol::qualify(pack, s).map(StaticName::Symbol),
        (None, Some(toml::Value::Integer(i))) => usize::try_from(*i)
            .map(StaticName::Index)
            .map_err(|_| "an index is 0 or more".into()),
        _ => Err("a static is named by static = \"symbol\" or index = n".into()),
    }
}

fn small(t: &toml::Table, key: &str, max: i64) -> Result<u16, String> {
    let v = t
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| format!("this brush takes {key}"))?;
    if (0..=max).contains(&v) {
        u16::try_from(v).map_err(|e| e.to_string())
    } else {
        Err(format!("{key} is 0 to {max}"))
    }
}

/// A dropped building's furniture as an edit names it: left out the copy's own, `"bare"`, or
/// another copy's, `"3DC7/1"`.
///
/// # Errors
/// Anything else.
pub fn furnished(v: Option<&toml::Value>) -> Result<crate::source::Furnished, String> {
    use crate::source::Furnished;
    match v {
        None => Ok(Furnished::Template),
        Some(toml::Value::String(s)) if s == "bare" => Ok(Furnished::Bare),
        Some(toml::Value::String(s)) => crate::buildings::Site::parse(s)
            .map(|site| Furnished::Like(vec![site]))
            .ok_or_else(|| format!("{s:?} is not a retail building: \"3DC7/1\" or \"bare\"")),
        Some(_) => Err("furnished is \"bare\" or a retail building, \"3DC7/1\"".into()),
    }
}

/// A drop as the command line writes it: `<model> <landblock> <x> <y> <heading>`, with the
/// layout, the furniture of another copy, or none, and for a house, whether it is a plain
/// building rather than one players can buy.
///
/// # Errors
/// Words that do not say that, with the reason.
pub fn add_building(
    words: &[String],
    layout: Option<&str>,
    furniture: Option<&str>,
    bare: bool,
    plain: bool,
) -> Result<Action, String> {
    let [model, lb, x, y, heading] = words else {
        return Err("add-building takes <model> <landblock> <x> <y> <heading>".into());
    };
    let number = |s: &str, what: &str| {
        s.parse::<f32>()
            .map_err(|_| format!("add-building's {what} is a number, not {s}"))
    };
    let furnished = match (furniture, bare) {
        (Some(_), true) => return Err("add-building takes --furnished or --bare, not both".into()),
        (Some(f), false) => furnished(Some(&toml::Value::String(f.to_owned())))?,
        (None, true) => crate::source::Furnished::Bare,
        (None, false) => crate::source::Furnished::Template,
    };
    let landblock = {
        let mut t = toml::Table::new();
        t.insert("landblock".into(), toml::Value::String(lb.clone()));
        landblock(&t)?
    };
    Ok(Action::AddBuilding {
        model: u32::from_str_radix(model.trim_start_matches("0x"), 16)
            .ok()
            .filter(|m| matches!(m >> 24, 0x01 | 0x02))
            .map(DataId)
            .ok_or_else(|| {
                format!("add-building's model is a hexadecimal model id, not {model}")
            })?,
        layout: layout
            .map_or(Ok(0), str::parse)
            .map_err(|_| "a layout is 0, 1, ...".to_owned())?,
        furnished,
        house: !plain,
        landblock,
        x: number(x, "x")?,
        y: number(y, "y")?,
        heading: number(heading, "heading")?,
    })
}

/// One edit from its table; `pack` is the pack the edit is written for, whose symbols may be
/// named bare.
///
/// # Errors
/// A table that is not an edit, with the reason.
pub fn action(t: &toml::Table, pack: &str) -> Result<Action, String> {
    let what = t
        .get("do")
        .and_then(toml::Value::as_str)
        .ok_or("an edit says what it does: do = \"place-static\", \"brush\", ...")?;
    Ok(match what {
        "place-static" => {
            let model = t
                .get("model")
                .and_then(toml::Value::as_integer)
                .and_then(|m| u32::try_from(m).ok())
                .filter(|m| matches!(m >> 24, 0x01 | 0x02))
                .ok_or("a static's model is a GfxObj (0x01...) or a Setup (0x02...)")?;
            Action::Op(Op::PlaceStatic {
                landblock: landblock(t)?,
                model: DataId(model),
                at: place(t)?.ok_or("a placed static says where: at = [x, y, z]")?,
                rotation: rotation(t)?.unwrap_or(Rotation::Heading(0.0)),
                name: None,
            })
        }
        "move-static" => Action::Move {
            landblock: landblock(t)?,
            name: static_name(t, pack)?,
            at: place(t)?,
            rotation: rotation(t)?,
        },
        "remove-static" => Action::Remove {
            landblock: landblock(t)?,
            name: static_name(t, pack)?,
        },
        "brush" => {
            let kind = t.get("brush").and_then(toml::Value::as_str).ok_or(
                "a brush names its kind: raise, lower, flatten, smooth, terrain, scene, road",
            )?;
            let amount = || -> Result<u8, String> {
                u8::try_from(small(t, "amount", 255)?).map_err(|e| e.to_string())
            };
            let brush = match kind {
                "raise" => Brush::Raise(amount()?),
                "lower" => Brush::Lower(amount()?),
                "flatten" => Brush::Flatten(
                    t.get("value")
                        .map(|_| small(t, "value", 255))
                        .transpose()?
                        .map(|v| u8::try_from(v).unwrap_or(u8::MAX)),
                ),
                "smooth" => Brush::Smooth,
                "terrain" => Brush::Terrain(small(t, "value", 31)?),
                "scene" => Brush::Scene(small(t, "value", 31)?),
                "road" => Brush::Road(small(t, "value", 3)?),
                other => return Err(format!("{other:?} is not a brush")),
            };
            let at = place(t)?.ok_or("a brush says where: at = [x, y]")?;
            Action::Brush {
                brush,
                landblock: landblock(t)?,
                x: at.x,
                y: at.y,
                radius: t.get("radius").and_then(number).unwrap_or(24.0),
            }
        }
        "spawn" => {
            let weenie = match t.get("weenie") {
                Some(toml::Value::String(s)) => Weenie::Symbol(symbol::qualify(pack, s)?),
                Some(toml::Value::Integer(i)) => {
                    Weenie::Class(u32::try_from(*i).map_err(|e| e.to_string())?)
                }
                _ => return Err("a spawn names its weenie: a symbol or a class id".into()),
            };
            let at = place(t)?.ok_or("a spawn says where: at = [x, y, z]")?;
            Action::Op(Op::PlaceSpawn {
                landblock: landblock(t)?,
                weenie,
                origin: Vec3::new(at.x, at.y, f32::NAN),
                heading: match rotation(t)? {
                    Some(Rotation::Heading(h)) => h,
                    _ => 0.0,
                },
            })
            .with_height(at.z)
        }
        "move-building" => {
            let at = place(t)?.ok_or("a moved building says where: at = [x, y, z]")?;
            let Height::At(z) = at.z else {
                return Err("a moved building's height is a number".into());
            };
            Action::MoveBuilding {
                landblock: landblock(t)?,
                index: t
                    .get("index")
                    .and_then(toml::Value::as_integer)
                    .and_then(|i| usize::try_from(i).ok())
                    .ok_or("a moved building names its place in the statics record: index = 0")?,
                at: [at.x, at.y, z],
                rotation: rotation(t)?.unwrap_or(Rotation::Heading(0.0)),
            }
        }
        "add-building" => {
            let at = place(t)?.ok_or("a dropped building says where: at = [x, y]")?;
            Action::AddBuilding {
                model: t
                    .get("model")
                    .and_then(toml::Value::as_integer)
                    .and_then(|m| u32::try_from(m).ok())
                    .map(DataId)
                    .ok_or("a dropped building names its model: model = 0x01002228")?,
                layout: match t.get("layout") {
                    None => 0,
                    Some(v) => v
                        .as_integer()
                        .and_then(|i| usize::try_from(i).ok())
                        .ok_or("a layout is its place in the model's list: 0, 1, ...")?,
                },
                furnished: furnished(t.get("furnished"))?,
                house: match t.get("house") {
                    None => true,
                    Some(v) => v
                        .as_bool()
                        .ok_or("house is true (one players can buy) or false (a plain building)")?,
                },
                landblock: landblock(t)?,
                x: at.x,
                y: at.y,
                heading: match rotation(t)? {
                    Some(Rotation::Heading(h)) => h,
                    None => 0.0,
                    Some(Rotation::Quat(_)) => {
                        return Err("a dropped building is turned by a heading".into())
                    }
                },
            }
        }
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        "save" => Action::Save,
        other => return Err(format!("{other:?} is not an edit")),
    })
}

impl Action {
    /// A spawn's height: given, or marked to stand on the ground (NaN) for [`Session::act`].
    fn with_height(self, z: Height) -> Self {
        match self {
            Self::Op(Op::PlaceSpawn {
                landblock,
                weenie,
                origin,
                heading,
            }) => Self::Op(Op::PlaceSpawn {
                landblock,
                weenie,
                origin: Vec3::new(
                    origin.x,
                    origin.y,
                    match z {
                        Height::At(z) => z,
                        Height::Ground => f32::NAN,
                    },
                ),
                heading,
            }),
            other => other,
        }
    }
}

/// Every `[[edit]]` of `text`, for pack `pack`.
///
/// # Errors
/// Text that is not TOML, or an edit that does not read, with its place.
pub fn actions(text: &str, pack: &str) -> Result<Vec<Action>, String> {
    let doc: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(edits) = doc.get("edit") else {
        return Ok(Vec::new());
    };
    let edits = edits.as_array().ok_or("edits are [[edit]] tables")?;
    edits
        .iter()
        .enumerate()
        .map(|(i, e)| {
            e.as_table()
                .ok_or_else(|| format!("edit {}: not a table", i + 1))
                .and_then(|t| action(t, pack).map_err(|why| format!("edit {}: {why}", i + 1)))
        })
        .collect()
}

impl Session {
    /// The static of landblock `lb` that `name` names, as the compiled world holds it.
    ///
    /// # Errors
    /// No such static.
    pub fn static_named(&self, lb: u16, name: &StaticName) -> Result<StaticOrigin, Error> {
        let origins = self.origins(lb);
        let found = match name {
            StaticName::Index(i) => origins.into_iter().find(|o| *o == StaticOrigin::Base(*i)),
            StaticName::Symbol(s) => origins
                .into_iter()
                .find(|o| matches!(o, StaticOrigin::Pack { name: Some(n), .. } if n == s)),
        };
        found.ok_or_else(|| Error::Refused(format!("landblock {lb:04X} has no static {name:?}")))
    }

    /// Do `action`.
    ///
    /// # Errors
    /// As the operation it is, a static it names that is not there, a file that will not save.
    pub fn act(&mut self, action: &Action) -> Result<Applied, Error> {
        match action {
            Action::Op(Op::PlaceSpawn {
                landblock,
                weenie,
                origin,
                heading,
            }) if origin.z.is_nan() => {
                let z = self.ground(*landblock, origin.x, origin.y).unwrap_or(0.0);
                self.apply(&Op::PlaceSpawn {
                    landblock: *landblock,
                    weenie: weenie.clone(),
                    origin: Vec3::new(origin.x, origin.y, z),
                    heading: *heading,
                })
            }
            Action::Op(op) => self.apply(op),
            Action::Move {
                landblock,
                name,
                at,
                rotation,
            } => {
                let which = self.static_named(*landblock, name)?;
                self.apply(&Op::MoveStatic {
                    landblock: *landblock,
                    which,
                    at: *at,
                    rotation: *rotation,
                })
            }
            Action::MoveBuilding {
                landblock,
                index,
                at,
                rotation,
            } => {
                let op = self
                    .move_building(*landblock, *index, *at, *rotation)
                    .ok_or_else(|| {
                        Error::Refused(format!("landblock {landblock:04X} has no building {index}"))
                    })?;
                self.apply(&op)
            }
            Action::AddBuilding {
                model,
                layout,
                furnished,
                house,
                landblock,
                x,
                y,
                heading,
            } => {
                let (op, warnings) = self.drop_building_as(
                    *model, *layout, furnished, *house, *landblock, *x, *y, *heading,
                )?;
                let mut done = self.apply(&op)?;
                done.warnings = warnings;
                Ok(done)
            }
            Action::Remove { landblock, name } => {
                let which = self.static_named(*landblock, name)?;
                self.apply(&Op::RemoveStatic {
                    landblock: *landblock,
                    which,
                })
            }
            Action::Brush {
                brush,
                landblock,
                x,
                y,
                radius,
            } => {
                let o = crate::pick::block_origin(*landblock);
                let sets = brush::dab(self, *brush, o.x + x, o.y + y, *radius);
                self.apply(&Op::SetVertices(sets))
            }
            Action::Undo => Ok(Applied {
                touched: self.undo()?,
                ..Applied::default()
            }),
            Action::Redo => Ok(Applied {
                touched: self.redo()?,
                ..Applied::default()
            }),
            Action::Save => {
                self.save()?;
                Ok(Applied::default())
            }
        }
    }
}
