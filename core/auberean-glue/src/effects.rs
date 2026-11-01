//! An effect as the effects panel lists it, as scripts fill one in.
//!
//! Scripts make a new node for each effect on the player and set its number, name, description,
//! icon, how long it lasts and when it was cast (two 64-bit floats, through the engine's
//! floating-point unit, which quiets a signalling not-a-number), and whether it is shown
//! enabled, toggled or pulsed (each a bit; disabling clears the enabled bit). The name and
//! description are copies of the strings the arguments name. Setting a node from another copies
//! the other's every field.

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{EffectNode, NativeObject};
use auberean_script::value::NULL;

const NODE: u32 = 0x98;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (NODE, 0x02, set_eid),
    (NODE, 0x04, set_name),
    (NODE, 0x06, set_icon),
    (NODE, 0x08, set_description),
    (NODE, 0x0A, set_enabled),
    (NODE, 0x0C, set_disabled),
    (NODE, 0x0D, set),
    (NODE, 0x0E, set_toggled),
    (NODE, 0x0F, set_pulsed),
    (NODE, 0x10, set_duration_info),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The node a reference names (`None` for the null reference: the call crashes).
fn node_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<Option<&'a mut EffectNode>, Stop> {
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let o = c.vm.heap.native_mut(r)?;
    if let NativeObject::Packed { ty, .. } = o {
        if *ty == NODE {
            *o = o.unpacked();
        }
    }
    match o {
        NativeObject::EffectNode(n) => Ok(Some(n)),
        _ => Err(Stop::Native {
            native,
            what: "not an effect node".into(),
        }),
    }
}

/// Store the word argument into a field of the receiver's node.
fn set_word(c: &mut Call<'_>, f: fn(&mut EffectNode, u32)) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let v = if r == NULL { 0 } else { c.arg(0)? };
    let Some(n) = node_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    f(n, v);
    Ok(Status::Success)
}

/// Change the receiver's node's bits.
fn set_flags(c: &mut Call<'_>, f: fn(u32) -> u32) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let Some(n) = node_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    n.flags = f(n.flags);
    Ok(Status::Success)
}

/// A copy of the string the argument names.
fn text_arg(c: &Call<'_>) -> Result<Vec<u16>, Stop> {
    let t = c.arg(0)?;
    match c.vm.heap.native(t)? {
        NativeObject::Text(u) => Ok(u.clone()),
        _ => Err(c.fail("not a string")),
    }
}

/// A 64-bit float through the floating-point unit: a signalling not-a-number comes out quiet.
fn through_fpu(v: u64) -> u64 {
    if f64::from_bits(v).is_nan() {
        v | 1 << 51
    } else {
        v
    }
}

/// The effect's number.
// D2-01370
fn set_eid(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, |n, v| n.eid = v)
}

/// The icon.
// D2-01514
fn set_icon(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, |n, v| n.icon = v)
}

/// The name: a copy of the string the argument names.
// D2-01345, D2-01343
fn set_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let text = text_arg(c)?;
    if let Some(n) = node_mut(c, r)? {
        n.name = text;
    }
    Ok(Status::Success)
}

/// The description: a copy of the string the argument names.
// D2-01344, D2-14553
fn set_description(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let text = text_arg(c)?;
    if let Some(n) = node_mut(c, r)? {
        n.description = text;
    }
    Ok(Status::Success)
}

// D2-01340, D2-12164
fn set_enabled(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_flags(c, |f| f | EffectNode::ENABLED)
}

// D2-01338, D2-12165
fn set_disabled(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_flags(c, |f| f & !EffectNode::ENABLED)
}

// D2-01342, D2-12166
fn set_toggled(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_flags(c, |f| f | EffectNode::TOGGLED)
}

// D2-01341, D2-12167
fn set_pulsed(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_flags(c, |f| f | EffectNode::PULSED)
}

/// Every field copied from the node the argument names (a node set from itself is unchanged).
// D2-01337, D2-12169
fn set(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let from = c.arg(0)?;
    let copy = match node_mut(c, from)? {
        Some(n) => n.clone(),
        None => return Err(Stop::NullReference),
    };
    if let Some(n) = node_mut(c, r)? {
        *n = copy;
    }
    Ok(Status::Success)
}

/// How long it lasts (the first argument, 64 bits) and when it was cast (the second).
// D2-01339
fn set_duration_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let duration = through_fpu(c.long_at(-5)?);
    let time_cast = through_fpu(c.long_at(-3)?);
    if let Some(n) = node_mut(c, r)? {
        n.duration = duration;
        n.time_cast = time_cast;
    }
    Ok(Status::Success)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a node's fields as its setters leave them, on hand-built calls)
    use super::through_fpu;

    #[test]
    fn a_signalling_not_a_number_comes_out_quiet_and_others_unchanged() {
        let snan = 0x7FF0_0000_0000_0001;
        assert_eq!(through_fpu(snan), 0x7FF8_0000_0000_0001);
        assert_eq!(through_fpu((-1.0f64).to_bits()), (-1.0f64).to_bits());
        assert_eq!(through_fpu(0x0000_0000_0000_0001), 1);
    }
}
