//! A look file: TOML, each value a number or a curve, and a table for each kind of day.
//!
//! ```toml
//! # A factor on the sun's brightness; 1 leaves it as retail has it.
//! sun = 1.1
//! # The evening and night tint: 1 keeps retail's purple, 0 takes it to gray. A curve is points of
//! # (time of day, value), the day running from 0 to 1, joined by straight lines round the day.
//! tint = [[0.70, 1.0], [0.78, 0.4], [0.95, 0.4], [0.05, 1.0]]
//! fog_color = [120, 120, 130]
//!
//! # On Rainy days, these in place of the values above.
//! [rainy]
//! fog_density = 1.5
//! ```
//!
//! The values: `sun`, `sky`, `tint`, `fog_density`, `fog_color`, `night_floor`, `exposure`,
//! `foliage_hue` (degrees, 0 to 360) and `foliage_pull` (0 to 1). The tables: `sunny`, `rainy`,
//! `clear` and `cloudy`. Anything else is an error, so a misspelt value is said, not ignored.

use super::{Curve, DayKind, LookToml, Values};

/// Why a look file was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookError(pub String);

impl std::fmt::Display for LookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LookError {}

fn err(s: impl Into<String>) -> LookError {
    LookError(s.into())
}

/// A TOML number as an `f32`, integer or float.
fn number(v: &toml::Value) -> Option<f32> {
    match v {
        // A look's numbers are small; the conversion keeps them.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        toml::Value::Integer(i) => Some(*i as f32),
        #[allow(clippy::cast_possible_truncation)]
        toml::Value::Float(f) => Some(*f as f32),
        _ => None,
    }
}

/// The least and most each value may be.
fn range(name: &str) -> (f32, f32) {
    match name {
        "tint" => (0.0, 1.0),
        "exposure" => (-16.0, 16.0),
        "night_floor" => (0.0, 1.0),
        "fog_density" => (0.001, 100.0),
        "foliage_hue" => (0.0, 360.0),
        "foliage_pull" => (0.0, 1.0),
        _ => (0.0, 100.0),
    }
}

/// One value, `name`, said as `label` in what goes wrong: a number, or a curve of
/// `[time, value]` points with times from 0 to 1.
fn curve(name: &str, label: &str, v: &toml::Value) -> Result<Curve, LookError> {
    let (lo, hi) = range(name);
    let name = label;
    let check = |x: f32| -> Result<f32, LookError> {
        if x.is_finite() && (lo..=hi).contains(&x) {
            Ok(x)
        } else {
            Err(err(format!("{name} = {x} is outside {lo} to {hi}")))
        }
    };
    if let Some(x) = number(v) {
        return Ok(Curve::Constant(check(x)?));
    }
    let toml::Value::Array(points) = v else {
        return Err(err(format!(
            "{name} is a number or a list of [time of day, value] points"
        )));
    };
    let mut out = Vec::with_capacity(points.len());
    for p in points {
        let pair = p.as_array().filter(|a| a.len() == 2);
        let (Some(t), Some(x)) = (
            pair.and_then(|a| number(&a[0])),
            pair.and_then(|a| number(&a[1])),
        ) else {
            return Err(err(format!(
                "each point of {name} is [time of day, value], two numbers"
            )));
        };
        if !(0.0..=1.0).contains(&t) {
            return Err(err(format!(
                "{name} has a point at time {t}; the day runs from 0 to 1"
            )));
        }
        out.push((t, check(x)?));
    }
    if out.is_empty() {
        return Err(err(format!("{name} has no points")));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(Curve::Points(out))
}

/// The values of one table.
fn values(table: &toml::Table, at: &str, days: bool) -> Result<Values, LookError> {
    let mut v = Values::default();
    for (name, value) in table {
        let slot = match name.as_str() {
            "sun" => &mut v.sun,
            "sky" => &mut v.sky,
            "tint" => &mut v.tint,
            "fog_density" => &mut v.fog_density,
            "night_floor" => &mut v.night_floor,
            "exposure" => &mut v.exposure,
            "foliage_hue" => &mut v.foliage_hue,
            "foliage_pull" => &mut v.foliage_pull,
            "fog_color" => {
                let c = value
                    .as_array()
                    .filter(|a| a.len() == 3)
                    .and_then(|a| {
                        a.iter()
                            .map(|x| x.as_integer().and_then(|i| u8::try_from(i).ok()))
                            .collect::<Option<Vec<u8>>>()
                    })
                    .ok_or_else(|| {
                        err(format!(
                            "{at}fog_color is [red, green, blue], each 0 to 255"
                        ))
                    })?;
                v.fog_color = Some([c[0], c[1], c[2]]);
                continue;
            }
            other if days && DayKind::ALL.iter().any(|k| k.name() == other) => continue,
            other => {
                return Err(err(format!(
                    "{at}{other} is not a value a look has (sun, sky, tint, fog_density, \
                     fog_color, night_floor, exposure, foliage_hue, foliage_pull){}",
                    if days {
                        ", nor a kind of day (sunny, rainy, clear, cloudy)"
                    } else {
                        ""
                    }
                )))
            }
        };
        *slot = Some(curve(name, &format!("{at}{name}"), value)?);
    }
    Ok(v)
}

/// A look file's text, read.
pub(super) fn parse(text: &str) -> Result<LookToml, LookError> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| err(format!("not TOML: {}", e.message())))?;
    let mut file = LookToml {
        values: values(&table, "", true)?,
        ..LookToml::default()
    };
    for kind in DayKind::ALL {
        if let Some(t) = table.get(kind.name()) {
            let t = t
                .as_table()
                .ok_or_else(|| err(format!("{} is a table of values", kind.name())))?;
            file.days
                .insert(kind, values(t, &format!("{}.", kind.name()), false)?);
        }
    }
    Ok(file)
}
