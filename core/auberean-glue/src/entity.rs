//! Entity descriptions: filled from an entity record, asked for its properties.
//!
//! A description is filled from an entity record by data id (answering whether there is one), and
//! keeps the record as it is stored. Asked for a property by id it looks only at the entity's own
//! properties (its parent entity's are not consulted): the property list's group for the id, the
//! description's first group with that id, the first property there with the id
//! (`auberean_assets::entity::Entity`'s queries).
//!
//! Each question asks for one type: a yes-or-no, an integer, a float, an enumeration value, a data
//! id, a package id (each written into the slot the out argument names, whose tag says a native
//! wrote it even when there is nothing to write), a vector or a string reference (each copied into
//! the object the out argument holds). A property of another type answers 0 and leaves the out
//! argument as it was, as does a property that is not there; one of the type answers 1
//! ([`answer`]).

use std::sync::Arc;

use auberean_assets::entity::Entity;
use auberean_assets::property::{PropertyList, StringInfo, VariableValue as V};
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{NativeObject, TextInfo, Variable, VariableValue};
use auberean_script::value::{tag, NULL};

const ENTITY_DESC: u32 = 0x22;

/// What one question asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    Bool,
    Integer,
    Float,
    Vector,
    Enum,
    DataId,
    Package,
    StringInfo,
}

const ASKED: [Asked; 8] = [
    Asked::Bool,
    Asked::Integer,
    Asked::Float,
    Asked::Vector,
    Asked::Enum,
    Asked::DataId,
    Asked::Package,
    Asked::StringInfo,
];

/// What a question answers with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A word, into the out argument's slot.
    Word(u32),
    /// A vector's bits, into the out argument's vector.
    Vector([u32; 3]),
    /// A string reference, into the out argument's string reference.
    Text(TextInfo),
}

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (ENTITY_DESC, 0x10, fill_from_record),
    (ENTITY_DESC, 0x13, inq::<0>),
    (ENTITY_DESC, 0x14, inq::<1>),
    (ENTITY_DESC, 0x15, inq::<2>),
    (ENTITY_DESC, 0x16, inq::<3>),
    (ENTITY_DESC, 0x17, inq::<4>),
    (ENTITY_DESC, 0x18, inq::<5>),
    (ENTITY_DESC, 0x19, inq::<6>),
    (ENTITY_DESC, 0x1C, inq::<7>),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// What an entity answers to a question for one type of a property: `None` when it has no such
/// property, or one of another type.
///
/// # Errors
/// A string reference with a variable of a kind that stores nothing, which a script's string
/// reference cannot hold.
pub fn answer(
    entity: &Entity,
    list: &PropertyList,
    id: u32,
    asked: Asked,
) -> Result<Option<Answer>, String> {
    #[allow(clippy::cast_sign_loss)]
    Ok(match asked {
        Asked::Bool => entity
            .inq_bool(list, id)
            .map(|b| Answer::Word(u32::from(b))),
        Asked::Integer => entity.inq_integer(list, id).map(|i| Answer::Word(i as u32)),
        Asked::Float => entity
            .inq_float(list, id)
            .map(|f| Answer::Word(f.to_bits())),
        Asked::Enum => entity.inq_enum(list, id).map(Answer::Word),
        Asked::DataId => entity.inq_data_file(list, id).map(|d| Answer::Word(d.0)),
        Asked::Package => entity.inq_package(list, id).map(Answer::Word),
        Asked::Vector => entity
            .inq_vector(list, id)
            .map(|v| Answer::Vector([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()])),
        Asked::StringInfo => match entity.inq_string_info(list, id) {
            Some(s) => Some(Answer::Text(text_info(s)?)),
            None => None,
        },
    })
}

/// A string reference as a script holds it.
///
/// # Errors
/// A variable of a kind that stores nothing.
pub fn text_info(s: &StringInfo) -> Result<TextInfo, String> {
    let word = |b: &[u8], at: usize| -> u32 {
        b.get(at..at + 4)
            .and_then(|x| x.try_into().ok())
            .map_or(0, u32::from_le_bytes)
    };
    let half = |b: &[u8], at: usize| -> u16 {
        b.get(at..at + 2)
            .and_then(|x| x.try_into().ok())
            .map_or(0, u16::from_le_bytes)
    };
    let long = |b: &[u8], at: usize| u64::from(word(b, at)) | u64::from(word(b, at + 4)) << 32;
    let mut variables = Vec::with_capacity(s.variables.len());
    for v in &s.variables {
        #[allow(clippy::cast_sign_loss)]
        let (kind, value) = match &v.value {
            V::Int(i) => (2, VariableValue::Word(*i as u32)),
            V::UInt(u) => (6, VariableValue::Word(*u)),
            V::Float { value, precision } => {
                (4, VariableValue::Double(value.to_bits(), *precision))
            }
            V::Long(i) => (10, VariableValue::Long(*i as u64)),
            V::ULong(u) => (12, VariableValue::Long(*u)),
            V::Text(t) => (9, VariableValue::Nested(Box::new(text_info(t)?))),
            V::Formatted { kind, bytes } => (
                *kind,
                match kind {
                    3 | 7 => VariableValue::Words(word(bytes, 0), word(bytes, 4), half(bytes, 8)),
                    5 => VariableValue::DoubleWords(
                        long(bytes, 0),
                        half(bytes, 8),
                        word(bytes, 10),
                        half(bytes, 14),
                    ),
                    _ => VariableValue::LongWords(long(bytes, 0), word(bytes, 8), half(bytes, 12)),
                },
            ),
            V::Empty(k) => return Err(format!("a string reference variable of kind {k}")),
        };
        variables.push(Variable {
            id: v.key,
            kind,
            value,
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    let count = variables.len() as u16;
    Ok(TextInfo {
        string_id: s.token,
        table: s.table.0,
        count,
        has_literal: u16::from(s.literal.is_some()),
        literal: s.literal.as_ref().map(|l| l.encode_utf16().collect()),
        variables,
    })
}

/// Fill an entity description (through the out reference `fp[-3]`) from the entity record a data
/// id (`fp[-2]`) names: whether there is one. The description keeps the record as it is stored.
// D2-08526, D2-13022, D2-13008
fn fill_from_record(c: &mut Call<'_>) -> Result<Status, Stop> {
    let did = c.cx.at(-2)?;
    let out = c.cx.at(-3)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Ok(Status::Crash);
    }
    let record = c
        .host
        .database()
        .and_then(|db| db.entity(did).and_then(|_| db.read(did)));
    let ok = match record {
        Some(bytes) => {
            let o = c.vm.heap.native_mut(target)?;
            let ty = match o {
                NativeObject::Opaque { ty } | NativeObject::Packed { ty, .. } => *ty,
                _ => return Err(c.fail("the out reference holds no entity description")),
            };
            *o = NativeObject::Packed { ty, bytes };
            true
        }
        None => false,
    };
    c.write(-4, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// The description a reference names: what a question for one type of its property answers
/// (`None` for a description never filled).
fn ask(c: &mut Call<'_>, r: u32, id: u32, asked: Asked) -> Result<Option<Answer>, Stop> {
    let bytes = match c.vm.heap.native(r)? {
        NativeObject::Packed { bytes, .. } => bytes.clone(),
        NativeObject::Opaque { .. } => return Ok(None),
        _ => return Err(c.fail("the receiver is not an entity description")),
    };
    let native = c.native;
    let Some(db) = c.host.database() else {
        return Err(Stop::Host("no data files to read properties with".into()));
    };
    let Some(list) = db.property_list() else {
        return Err(Stop::Host("the data files have no property list".into()));
    };
    let entity = Entity::read(&mut auberean_dat::Reader::new(&bytes), db.context())
        .map_err(|e| Stop::Host(format!("an entity record that does not read: {e}")))?;
    answer(&entity, list, id, asked).map_err(|what| Stop::Native { native, what })
}

/// Ask a description (`fp[-1]`) for a property (`fp[-2]`) of one type, into the out argument
/// (`fp[-3]`): 1 when it has one of the type, else 0 (`fp[-4]`).
// D2-08522, D2-08530, D2-08529, D2-08534, D2-08528, D2-08525, D2-08531, D2-08532
fn inq<const K: usize>(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let asked = ASKED[K];
    let id = c.cx.at(-2)?;
    let out = c.cx.at(-3)?;
    let prog = Arc::clone(c.vm.program());
    // A word goes into the slot itself, tagged first; an object is the one the slot holds.
    let object = if matches!(asked, Asked::Vector | Asked::StringInfo) {
        let (o, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
        c.vm.heap.native(o)?;
        o
    } else {
        c.vm.write_slot(c.host, c.cx, out, 0, None, tag::MIXED, &prog)?;
        NULL
    };
    let answered = match ask(c, r, id, asked)? {
        None => false,
        Some(Answer::Word(w)) => {
            c.vm.write_data(c.host, c.cx, out, 0, w, &prog)?;
            true
        }
        Some(Answer::Vector(v)) => {
            match c.vm.heap.native_mut(object)? {
                NativeObject::Vector(x) => *x = v,
                _ => return Err(c.fail("the out argument holds no vector")),
            }
            true
        }
        Some(Answer::Text(t)) => {
            match c.vm.heap.native_mut(object)? {
                NativeObject::TextInfo(x) => *x = t,
                _ => return Err(c.fail("the out argument holds no string reference")),
            }
            true
        }
    };
    c.write(-4, u32::from(answered), tag::MIXED)?;
    Ok(Status::Success)
}
