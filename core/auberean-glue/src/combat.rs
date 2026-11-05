//! What a hit shows, as scripts fill it in for the engine: the effects and behaviors on the
//! attacker, its target and its weapon, and the floating text over the target.
//!
//! **A hit's effects.** The engine hands the attacker's scripts a new object to fill: three
//! effects (the attacker's, the target's, the weapon's), each a word, and two behaviors (the
//! target's, the weapon's), each a copy of the parameters the argument names. Setting answers
//! nothing; nothing set is no effect and no behavior. The engine reads the object back when the
//! call ends.
//!
//! **The floating text** ([`DamageText`]): its string (a copy of the string the argument
//! names), its color (the four floats copied as they are), how long it shows and its scale (each a
//! float through the engine's floating-point unit, which quiets a signalling not-a-number).
//! Reading the string or the color answers a new object holding a copy; reading the duration or
//! the scale answers the float as stored.
//!
//! A null receiver ends the script; a null argument where an object is wanted stops the run (the
//! client would read through it).

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, DamageText, Field, NativeObject};
use auberean_script::value::{tag, NULL};

use crate::adaptors::{self, set_word};

/// A hit's effects and behaviors.
pub const FX_AND_BEHAVIOR_INFO: u32 = 0x2B;
/// The floating text.
const DAMAGE_TEXT: u32 = 0x97;
/// A string, and a color.
const STRING: u32 = native_type::WP_STRING;
const COLOR: u32 = 0x64;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (FX_AND_BEHAVIOR_INFO, 0x01, set_word),
    (FX_AND_BEHAVIOR_INFO, 0x02, set_word),
    (FX_AND_BEHAVIOR_INFO, 0x03, set_word),
    (FX_AND_BEHAVIOR_INFO, 0x04, set_behavior),
    (FX_AND_BEHAVIOR_INFO, 0x05, set_behavior),
    (DAMAGE_TEXT, 0x01, set_display_string),
    (DAMAGE_TEXT, 0x02, get_display_string),
    (DAMAGE_TEXT, 0x03, set_font_color),
    (DAMAGE_TEXT, 0x04, get_font_color),
    (DAMAGE_TEXT, 0x05, set_duration),
    (DAMAGE_TEXT, 0x06, get_duration),
    (DAMAGE_TEXT, 0x07, set_scale),
    (DAMAGE_TEXT, 0x08, get_scale),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// Set the target's or the weapon's behavior: a copy of the parameters the argument names.
// D2-08562, D2-08564, D2-15865, D2-15904
fn set_behavior(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let r = c.arg(0)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let b = match c.vm.heap.native(r)?.unpacked() {
        b @ NativeObject::Behavior(_) => b,
        _ => return Err(c.fail("not a behavior's parameters")),
    };
    adaptors::set(c, Field::Value(Box::new(b)))
}

/// The floating text the receiver names (`None` for the null reference: the call crashes).
fn blob<'a>(c: &'a mut Call<'_>) -> Result<Option<&'a mut DamageText>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let o = c.vm.heap.native_mut(r)?;
    if matches!(o, NativeObject::Packed { ty, .. } if *ty == DAMAGE_TEXT) {
        *o = o.unpacked();
    }
    match o {
        NativeObject::DamageText(d) => Ok(Some(d)),
        _ => Err(Stop::Native {
            native,
            what: "not a damage text".into(),
        }),
    }
}

/// The object an argument names; a null one stops the run.
fn argument(c: &Call<'_>) -> Result<NativeObject, Stop> {
    let r = c.arg(0)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    Ok(c.vm.heap.native(r)?.clone())
}

/// A float through the floating-point unit: a signalling not-a-number comes out quiet.
pub(crate) const fn through_fpu(v: u32) -> u32 {
    if f32::from_bits(v).is_nan() {
        v | 1 << 22
    } else {
        v
    }
}

/// The string: a copy of the string the argument names.
// D2-01334
fn set_display_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let NativeObject::Text(t) = argument(c)? else {
        return Err(c.fail("not a string"));
    };
    if let Some(d) = blob(c)? {
        d.text = t;
    }
    Ok(Status::Success)
}

/// A new string holding a copy of the text's string.
// D2-01335
fn get_display_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(d) = blob(c)? else {
        return Ok(Status::Crash);
    };
    let t = d.text.clone();
    let r = c.vm.heap.add_native(STRING, NativeObject::Text(t));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The color: the argument's four floats, copied as they are.
// D2-01331
fn set_font_color(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let v = crate::color::of(&argument(c)?);
    if let Some(d) = blob(c)? {
        d.color = v.map(f32::to_bits);
    }
    Ok(Status::Success)
}

/// A new color holding a copy of the text's color.
// D2-01336
fn get_font_color(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(d) = blob(c)? else {
        return Ok(Status::Crash);
    };
    let v = d.color.map(f32::from_bits);
    let r = c.vm.heap.add_native(COLOR, crate::color::object(v));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// How long the text shows.
// D2-01330
fn set_duration(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let v = through_fpu(c.arg(0)?);
    if let Some(d) = blob(c)? {
        d.duration = v;
    }
    Ok(Status::Success)
}

// D2-01611
fn get_duration(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(d) = blob(c)? else {
        return Ok(Status::Crash);
    };
    let v = d.duration;
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The text's scale.
// D2-01332
fn set_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let v = through_fpu(c.arg(0)?);
    if let Some(d) = blob(c)? {
        d.scale = v;
    }
    Ok(Status::Success)
}

// D2-01521
fn get_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(d) = blob(c)? else {
        return Ok(Status::Crash);
    };
    let v = d.scale;
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}
