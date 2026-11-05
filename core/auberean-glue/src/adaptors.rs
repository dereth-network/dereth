//! Interface adaptors: objects scripts fill for the interface's panels (a craft skill's line, an
//! allegiance member, a fellow, a shop) and hand to a panel notice.
//!
//! An adaptor has no packed form; a setter copies its argument into one of its fields (answering
//! nothing unless said below), and a getter reads a field back (0 when it was never set). A field is named by
//! the native that sets it (`SetLevel` sets `Level`, `GetLevel` reads it). What a setter copies:
//!
//! - a word, as given; a yes-or-no, as its low byte; a 64-bit value, from two slots, low first;
//! - a float, through the engine's floating-point unit, which quiets a signalling not-a-number;
//! - a string reference, a string, an icon description or another adaptor, as a copy of its value
//!   (changing the script's object later does not change the adaptor's);
//! - a shop sale's state, as a number its setter names: restricted 1, expired 2, sold 3.
//!
//! A fellowship holds its fellows by id: adding one whose id is there already adds nothing and
//! answers 0. An allegiance lists its vassals (each a copy of its adaptor and a yes-or-no) in the
//! order added (adding one answers 1), and holds its officers' ids as a set.
//!
//! A shop holds its sales (each a copy of the sale's adaptor) in order of their ordinals, lowest
//! first: a sale added goes before the first whose ordinal is greater (after those with the same
//! one); adding answers nothing.
//!
//! The crafting adaptors (a craft skill, a recipe, an ingredient) are read back as well: a word
//! as set (a new recipe has -1 charges, a new craft skill a usage experience factor of 1.0); a
//! 64-bit value (a craft skill's cost to raise, a recipe's time before it may be used again)
//! into two slots; a string reference as a new one holding a copy; a yes-or-no (an ingredient
//! that fills itself in, a recipe whose spinner may not be turned) into the low byte of its slot.
//! A recipe's time before it may be used again is a 64-bit float stored through the floating-point
//! unit; setting the difficulty less the skill's cutoff marks that the recipe has one; setting an
//! ingredient to fill itself in sets it whatever the argument.

use std::collections::BTreeMap;

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Field, NativeObject, TextInfo};
use auberean_script::value::{join, tag, NULL};

const ALLEGIANCE_DATA: u32 = 0x82;
const ALLEGIANCE_PROFILE: u32 = 0x83;
const FELLOW: u32 = 0x93;
const FELLOWSHIP: u32 = 0x94;
const CRAFT_SKILL: u32 = 0x1AD4;
const INGREDIENT: u32 = 0xA5;
const RECIPE: u32 = 0xAA;
const SHOP: u32 = 0x1AD6;
const SHOP_SALE: u32 = 0x1AD7;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (ALLEGIANCE_DATA, 0x01, set_long),
    (ALLEGIANCE_DATA, 0x02, set_value),
    (ALLEGIANCE_DATA, 0x03, set_value),
    (ALLEGIANCE_DATA, 0x04, set_long),
    (ALLEGIANCE_DATA, 0x05, set_word),
    (ALLEGIANCE_DATA, 0x06, set_word),
    (ALLEGIANCE_DATA, 0x07, set_word),
    (ALLEGIANCE_DATA, 0x08, set_byte),
    (ALLEGIANCE_DATA, 0x09, set_word),
    (ALLEGIANCE_DATA, 0x0A, set_word),
    (ALLEGIANCE_PROFILE, 0x01, set_member),
    (ALLEGIANCE_PROFILE, 0x02, set_member),
    (ALLEGIANCE_PROFILE, 0x03, set_member),
    (ALLEGIANCE_PROFILE, 0x04, add_vassal),
    (ALLEGIANCE_PROFILE, 0x05, set_word),
    (ALLEGIANCE_PROFILE, 0x06, set_value),
    (ALLEGIANCE_PROFILE, 0x07, set_value),
    (ALLEGIANCE_PROFILE, 0x08, set_word),
    (ALLEGIANCE_PROFILE, 0x09, add_officer),
    (FELLOW, 0x01, set_value),
    (FELLOW, 0x02, set_word),
    (FELLOW, 0x03, set_word),
    (FELLOW, 0x04, set_word),
    (FELLOW, 0x05, set_word),
    (FELLOW, 0x06, set_word),
    (FELLOW, 0x07, set_word),
    (FELLOW, 0x08, set_word),
    (FELLOW, 0x09, set_word),
    (FELLOWSHIP, 0x01, add_fellow),
    (FELLOWSHIP, 0x02, set_text),
    (FELLOWSHIP, 0x03, set_long),
    (FELLOWSHIP, 0x04, set_word),
    (FELLOWSHIP, 0x05, set_word),
    (CRAFT_SKILL, 0x01, get_word),
    (CRAFT_SKILL, 0x02, set_word),
    (CRAFT_SKILL, 0x04, set_value),
    (CRAFT_SKILL, 0x06, set_value),
    (CRAFT_SKILL, 0x08, set_word),
    (CRAFT_SKILL, 0x0A, set_word),
    (CRAFT_SKILL, 0x0C, set_long),
    (CRAFT_SKILL, 0x0E, set_word),
    (CRAFT_SKILL, 0x10, set_float),
    (CRAFT_SKILL, 0x03, get_value),
    (CRAFT_SKILL, 0x05, get_value),
    (CRAFT_SKILL, 0x07, get_word),
    (CRAFT_SKILL, 0x09, get_word),
    (CRAFT_SKILL, 0x0B, get_long),
    (CRAFT_SKILL, 0x0D, get_word),
    (CRAFT_SKILL, 0x0F, get_word),
    (INGREDIENT, 0x01, get_word),
    (INGREDIENT, 0x02, set_word),
    (INGREDIENT, 0x05, get_value),
    (INGREDIENT, 0x06, set_value),
    (INGREDIENT, 0x07, get_word),
    (INGREDIENT, 0x08, set_word),
    (INGREDIENT, 0x09, get_byte),
    (INGREDIENT, 0x0A, set_auto_populate),
    (RECIPE, 0x03, get_value),
    (RECIPE, 0x04, set_value),
    (RECIPE, 0x05, get_value),
    (RECIPE, 0x06, set_value),
    (RECIPE, 0x07, set_word),
    (RECIPE, 0x08, get_word),
    (RECIPE, 0x0D, set_double),
    (RECIPE, 0x0E, get_long),
    (RECIPE, 0x0F, set_word),
    (RECIPE, 0x10, get_word),
    (RECIPE, 0x11, set_word),
    (RECIPE, 0x12, get_word),
    (RECIPE, 0x13, set_word),
    (RECIPE, 0x14, get_word),
    (RECIPE, 0x15, set_word),
    (RECIPE, 0x16, get_word),
    (RECIPE, 0x17, get_word),
    (RECIPE, 0x18, set_word),
    (RECIPE, 0x19, set_word),
    (RECIPE, 0x1A, set_word),
    (RECIPE, 0x1B, set_word),
    (RECIPE, 0x1C, set_word),
    (RECIPE, 0x1D, set_word),
    (RECIPE, 0x1E, set_word),
    (RECIPE, 0x1F, set_word),
    (RECIPE, 0x20, set_word),
    (RECIPE, 0x21, set_word),
    (RECIPE, 0x22, set_cutoff),
    (RECIPE, 0x23, set_word),
    (RECIPE, 0x24, get_word),
    (RECIPE, 0x25, set_word),
    (RECIPE, 0x26, get_word),
    (RECIPE, 0x27, has_read_only_spinner),
    (RECIPE, 0x28, set_word),
    (SHOP, 0x01, set_word),
    (SHOP, 0x02, set_value),
    (SHOP, 0x03, set_value),
    (SHOP, 0x04, set_value),
    (SHOP, 0x05, set_word),
    (SHOP, 0x06, add_sale),
    (SHOP, 0x07, set_word),
    (SHOP, 0x08, set_word),
    (SHOP_SALE, 0x01, set_long),
    (SHOP_SALE, 0x02, set_word),
    (SHOP_SALE, 0x03, set_value),
    (SHOP_SALE, 0x04, set_word),
    (SHOP_SALE, 0x05, set_value),
    (SHOP_SALE, 0x06, set_value),
    (SHOP_SALE, 0x07, set_word),
    (SHOP_SALE, 0x08, set_float),
    (SHOP_SALE, 0x09, set_word),
    (SHOP_SALE, 0x0A, set_word),
    (SHOP_SALE, 0x0B, set_sale_state),
    (SHOP_SALE, 0x0C, set_sale_state),
    (SHOP_SALE, 0x0D, set_sale_state),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The field a native sets or reads: its name without `Set` or `Get`.
fn field_name(c: &Call<'_>) -> String {
    let name = c.vm.program().library.native_name(c.native);
    let method = name.rsplit("::").next().unwrap_or(&name);
    method
        .strip_prefix("Set")
        .or_else(|| method.strip_prefix("Get"))
        .unwrap_or(method)
        .to_owned()
}

/// The receiver's fields (an adaptor no setter has written to yet becomes an empty one), or
/// `None` when the receiver is the null reference.
fn fields<'a>(c: &'a mut Call<'_>) -> Result<Option<&'a mut BTreeMap<String, Field>>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let object = c.vm.heap.native_mut(r)?;
    if let NativeObject::Opaque { ty } = *object {
        *object = NativeObject::Adaptor {
            ty,
            fields: BTreeMap::new(),
        };
    }
    match object {
        NativeObject::Adaptor { fields, .. } => Ok(Some(fields)),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not an adaptor".into(),
        }),
    }
}

pub(crate) fn set(c: &mut Call<'_>, value: Field) -> Result<Status, Stop> {
    let name = field_name(c);
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    f.insert(name, value);
    Ok(Status::Success)
}

/// Set a word field from `fp[-2]`.
// D2-00793, D2-00795, D2-00797, D2-00802, D2-00807, D2-00942, D2-00943, D2-00944, D2-00946
// D2-00947, D2-00948, D2-01363, D2-01364, D2-01502, D2-01504, D2-01505, D2-01513, D2-01539
// D2-01540, D2-01556, D2-01557, D2-01571, D2-01584, D2-01637, D2-01638, D2-08561, D2-08563
// D2-08565, D2-01499, D2-01500, D2-01507, D2-15900
pub(crate) fn set_word(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    set(c, Field::Word(v))
}

/// Set a yes-or-no field from `fp[-2]`'s low byte.
// D2-00796
fn set_byte(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)? & 0xFF;
    set(c, Field::Word(v))
}

/// Set a 64-bit field from `fp[-3..-2]`.
// D2-00794, D2-00799, D2-00949, D2-01322, D2-01567
fn set_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = join(c.cx.at(-3)?, c.cx.at(-2)?);
    set(c, Field::Long(v))
}

/// Set a float field from `fp[-2]`, a signalling not-a-number quieted.
// D2-01323, D2-01564
fn set_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let mut v = c.arg(0)?;
    if f32::from_bits(v).is_nan() {
        v |= 1 << 22;
    }
    set(c, Field::Word(v))
}

/// Set a field to a copy of a string reference's or an icon description's value (`fp[-2]`).
// D2-00798, D2-00801, D2-00805, D2-00945, D2-01487, D2-01497, D2-01506, D2-01558, D2-01563
// D2-01566, D2-01568
fn set_value(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let v = c.vm.heap.native(r)?.clone();
    if !matches!(
        v,
        NativeObject::TextInfo(_) | NativeObject::Packed { .. } | NativeObject::Opaque { .. }
    ) {
        return Err(c.fail("the argument is not a string reference or an icon description"));
    }
    set(c, Field::Value(Box::new(v)))
}

/// A copy of the adaptor a reference names (one no setter has written to is an empty one).
fn adaptor_copy(c: &mut Call<'_>, r: u32) -> Result<NativeObject, Stop> {
    match c.vm.heap.native(r)? {
        NativeObject::Opaque { ty } => Ok(NativeObject::Adaptor {
            ty: *ty,
            fields: BTreeMap::new(),
        }),
        a @ NativeObject::Adaptor { .. } => Ok(a.clone()),
        _ => Err(c.fail("the argument is not an adaptor")),
    }
}

/// Set an allegiance's monarch, patron or member: a copy of a member's adaptor (`fp[-2]`) and a
/// yes-or-no (`fp[-3]`, its low byte).
// D2-00803, D2-00804, D2-00806, D2-12376
fn set_member(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let flag = c.arg(1)? & 0xFF;
    let member = adaptor_copy(c, r)?;
    set(
        c,
        Field::Many(vec![Field::Value(Box::new(member)), Field::Word(flag)]),
    )
}

/// A list field, made empty the first time.
fn list<'a>(c: &'a mut Call<'_>, name: &str) -> Result<Option<&'a mut Vec<Field>>, Stop> {
    let native = c.native;
    let Some(f) = fields(c)? else {
        return Ok(None);
    };
    match f
        .entry(name.to_owned())
        .or_insert_with(|| Field::Many(Vec::new()))
    {
        Field::Many(v) => Ok(Some(v)),
        _ => Err(Stop::Native {
            native,
            what: format!("the adaptor's {name} is not a list"),
        }),
    }
}

/// Add a vassal to an allegiance: a copy of its adaptor (`fp[-2]`) and a yes-or-no (`fp[-3]`,
/// its low byte), at the end of the list. Answers 1.
// D2-00810, D2-00808, D2-00809, D2-12377
fn add_vassal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let flag = c.arg(1)? & 0xFF;
    let vassal = adaptor_copy(c, r)?;
    let Some(l) = list(c, "Vassal")? else {
        return Ok(Status::Crash);
    };
    l.push(Field::Many(vec![
        Field::Value(Box::new(vassal)),
        Field::Word(flag),
    ]));
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Add an officer's id (`fp[-3..-2]`) to an allegiance's set of officers. No result.
// D2-00812, D2-00811
fn add_officer(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = Field::Long(join(c.cx.at(-3)?, c.cx.at(-2)?));
    let Some(l) = list(c, "Officer")? else {
        return Ok(Status::Crash);
    };
    if !l.contains(&id) {
        l.push(id);
    }
    Ok(Status::Success)
}

/// Add a fellow to a fellowship: its id (`fp[-3..-2]`) and a copy of its adaptor (`fp[-4]`),
/// unless a fellow with that id is there. Answers whether it was added.
// D2-00951, D2-12346, D2-12351
fn add_fellow(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = join(c.cx.at(-3)?, c.cx.at(-2)?);
    let r = c.cx.at(-4)?;
    let fellow = adaptor_copy(c, r)?;
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    let Field::Many(fellows) = f
        .entry("Fellow".to_owned())
        .or_insert_with(|| Field::Many(Vec::new()))
    else {
        return Err(c.fail("a fellowship's fellows are not a list"));
    };
    let there = fellows.iter().any(|x| match x {
        Field::Many(e) => e.first() == Some(&Field::Long(id)),
        _ => false,
    });
    if !there {
        fellows.push(Field::Many(vec![
            Field::Long(id),
            Field::Value(Box::new(fellow)),
        ]));
    }
    c.write(-5, u32::from(!there), tag::MIXED)?;
    Ok(Status::Success)
}

/// Add a sale to a shop: a copy of the sale's adaptor (`fp[-2]`), before the first sale whose
/// ordinal is greater (a sale's ordinal is 0 until set). No result.
// D2-01562, D2-01561, D2-01559, D2-12389, D2-12387, D2-12388
fn add_sale(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let r = c.arg(0)?;
    let sale = adaptor_copy(c, r)?;
    let ordinal = |a: &NativeObject| match a {
        NativeObject::Adaptor { fields, .. } => match fields.get("Ordinal") {
            Some(Field::Word(v)) => *v,
            _ => 0,
        },
        _ => 0,
    };
    let key = ordinal(&sale);
    let Some(l) = list(c, "Sale")? else {
        return Ok(Status::Crash);
    };
    let at = l
        .iter()
        .position(|x| matches!(x, Field::Value(v) if key < ordinal(v)))
        .unwrap_or(l.len());
    l.insert(at, Field::Value(Box::new(sale)));
    Ok(Status::Success)
}

/// Set a shop sale's state to the number its setter names: restricted 1, expired 2, sold 3.
// D2-01565, D2-01569, D2-01570
fn set_sale_state(c: &mut Call<'_>) -> Result<Status, Stop> {
    let state = match c.native & 0xFFFF {
        0x0B => 1,
        0x0C => 2,
        _ => 3,
    };
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    f.insert("State".to_owned(), Field::Word(state));
    Ok(Status::Success)
}

/// Set a field to a copy of a string's value (`fp[-2]`).
// D2-00952, D2-00950
fn set_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let v = c.vm.heap.native(r)?.clone();
    if !matches!(v, NativeObject::Text(_)) {
        return Err(c.fail("the argument is not a string"));
    }
    set(c, Field::Value(Box::new(v)))
}

/// What a word field of a new adaptor holds: a recipe's charges -1, a craft skill's usage
/// experience factor 1.0, anything else 0 (an allegiance member's, a fellow's, a fellowship's, a
/// shop's and a sale's ids, numbers and flags all start at 0, their lists empty).
// D2-12362, D2-12352, D2-12366, D2-12375, D2-12373, D2-12350, D2-12348, D2-12386, D2-12383
#[must_use]
pub fn new_word(ty: u32, field: &str) -> u32 {
    match (ty, field) {
        (RECIPE, "Charges") => u32::MAX,
        (CRAFT_SKILL, "UsageXPMod") => 1.0f32.to_bits(),
        _ => 0,
    }
}

/// The receiver's type, for its new fields' values.
fn receiver_type(c: &Call<'_>) -> u32 {
    c.native >> 16
}

/// Read a word field into the result slot (a new adaptor's when never set).
// D2-01489, D2-01490, D2-01422, D2-01326, D2-01325, D2-01327, D2-18959, D2-08788, D2-01510, D2-08713
// D2-08621, D2-01509, D2-08512
fn get_word(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name = field_name(c);
    let ty = receiver_type(c);
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    let v = match f.get(&name) {
        Some(Field::Word(v)) => *v,
        _ => new_word(ty, &name),
    };
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Read a yes-or-no field into the low byte of the result slot (the rest of the slot as it was).
// D2-01488
fn get_byte(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name = field_name(c);
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    let v = match f.get(&name) {
        Some(Field::Word(v)) => *v & 0xFF,
        _ => 0,
    };
    write_byte(c, v)
}

/// The low byte of the result slot below the receiver, the slot marked as a native's.
fn write_byte(c: &mut Call<'_>, v: u32) -> Result<Status, Stop> {
    let i = c.cx.frame_index(-2)?;
    let old = c.cx.data[i];
    c.write(-2, (old & !0xFF) | (v & 0xFF), tag::MIXED)?;
    Ok(Status::Success)
}

/// Read a 64-bit field into the two slots below the receiver (0 when never set).
// D2-01324, D2-01511
fn get_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name = field_name(c);
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    let v = match f.get(&name) {
        Some(Field::Long(v)) => *v,
        _ => 0,
    };
    let (lo, hi) = auberean_script::value::split(v);
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// Read a string reference field: a new one holding a copy (an empty one when never set).
// D2-01329, D2-01328, D2-01492
fn get_value(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name = field_name(c);
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    let t = match f.get(&name) {
        Some(Field::Value(v)) => match &**v {
            NativeObject::TextInfo(t) => t.clone(),
            _ => TextInfo::default(),
        },
        _ => TextInfo::default(),
    };
    let id = c.vm.heap.add_native(
        auberean_script::native_object::native_type::STRING_INFO,
        NativeObject::TextInfo(t),
    );
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Set a 64-bit float field from `fp[-3..-2]`, through the floating-point unit (a signalling
/// not-a-number quieted).
// D2-01508
fn set_double(c: &mut Call<'_>) -> Result<Status, Stop> {
    let mut v = join(c.cx.at(-3)?, c.cx.at(-2)?);
    if f64::from_bits(v).is_nan() {
        v |= 1 << 51;
    }
    set(c, Field::Long(v))
}

/// A recipe's difficulty less the skill's cutoff (`fp[-2]`), marked as one it has.
// D2-01498, D2-12364
fn set_cutoff(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    set(c, Field::Word(v))
}

/// An ingredient fills itself in (whatever the argument).
// D2-01486
fn set_auto_populate(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, Field::Word(1))
}

/// Whether a recipe's spinner may not be turned, into the low byte of the result slot.
// D2-01512
fn has_read_only_spinner(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(f) = fields(c)? else {
        return Ok(Status::Crash);
    };
    let v = match f.get("ReadOnlySpinner") {
        Some(Field::Word(v)) => u32::from(*v != 0),
        _ => 0,
    };
    write_byte(c, v)
}
