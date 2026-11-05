//! Property collections as the scripts read them: an admin's dump of an object's physics
//! properties walks a collection's groups and each group's properties, and reads each by its type.
//!
//! The natives fill objects the scripts made empty, through a reference: a group from a collection,
//! a property from a group. Each query answers whether it found something; a property asked for a
//! value of a kind it does not hold answers 0, as a property with no value does.

use std::sync::Arc;

use auberean_assets::property::{Property, PropertyCollection, PropertyGroup};
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::{tag, NULL};

use crate::bind_table;

/// The natives' packages.
const BASE_PROPERTY: u32 = 0x16;
const COLLECTION: u32 = 0x5B;
const GROUP: u32 = 0x5C;
const VECTOR: u32 = 0x70;

/// The enumeration that names a property's value type.
const VALUE_TYPES: u32 = 0x4000_0013;

macro_rules! engine_value {
    ($name:ident, $inner:ty, $ty:expr) => {
        /// An engine object the scripts hold.
        #[derive(Clone, Debug, PartialEq, Default)]
        pub struct $name(pub $inner);

        impl EngineValue for $name {
            fn native_type(&self) -> u32 {
                $ty
            }
            fn clone_value(&self) -> Box<dyn EngineValue> {
                Box::new(self.clone())
            }
            fn same(&self, other: &dyn EngineValue) -> bool {
                other.as_any().downcast_ref::<Self>() == Some(self)
            }
            fn pack(&self) -> Option<Vec<u8>> {
                None
            }
            fn as_any(&self) -> &dyn core::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
                self
            }
        }
    };
}

engine_value!(CollectionValue, PropertyCollection, COLLECTION);
engine_value!(GroupValue, PropertyGroup, GROUP);
engine_value!(PropertyValue, Property, BASE_PROPERTY);

const NATIVES: &[(u32, u32, NativeFn)] = &[
    (BASE_PROPERTY, 0x03, inq_integer),
    (BASE_PROPERTY, 0x05, inq_float),
    (BASE_PROPERTY, 0x07, inq_vector),
    (BASE_PROPERTY, 0x09, inq_string),
    (BASE_PROPERTY, 0x0B, inq_enum),
    (BASE_PROPERTY, 0x14, property_type),
    (BASE_PROPERTY, 0x15, property_name),
    (BASE_PROPERTY, 0x18, property_type_name),
    (COLLECTION, 0x0B, group_count),
    (COLLECTION, 0x0C, group_at),
    (GROUP, 0x06, group_name),
    (GROUP, 0x0B, property_count),
    (GROUP, 0x0C, optional_count),
    (GROUP, 0x0D, property_at),
    (GROUP, 0x0E, optional_at),
    (VECTOR, 0x0F, vector_string),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    bind_table(t, NATIVES);
}

/// Put a collection into the empty object the reference at `fp[out]` reaches; whether it could.
///
/// # Errors
/// A reference that does not read.
pub fn give_collection(
    c: &mut Call<'_>,
    out: u32,
    collection: PropertyCollection,
) -> Result<bool, Stop> {
    let target = reached(c, out)?;
    if target == NULL {
        return Ok(false);
    }
    *c.vm.heap.native_mut(target)? = NativeObject::Engine(Engine::new(CollectionValue(collection)));
    Ok(true)
}

/// What the reference `out` reaches: a native the script handed over itself, or the object in the
/// slot it names.
fn reached(c: &mut Call<'_>, out: u32) -> Result<u32, Stop> {
    if c.vm.heap.native(out).is_ok() {
        return Ok(out);
    }
    let prog = Arc::clone(c.vm.program());
    Ok(c.vm.read_slot(c.host, c.cx, out, 0, &prog)?.0)
}

/// Write through a reference.
fn write_out(c: &mut Call<'_>, out: u32, value: u32) -> Result<(), Stop> {
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, Some(value), tag::MIXED, &prog)?;
    Ok(())
}

/// A string into the string a reference reaches.
fn write_text(c: &mut Call<'_>, out: u32, text: &str) -> Result<(), Stop> {
    let target = reached(c, out)?;
    if target != NULL {
        *c.vm.heap.native_mut(target)? = NativeObject::Text(text.encode_utf16().collect());
    }
    Ok(())
}

fn engine<T: 'static + Clone>(c: &Call<'_>, r: u32) -> Result<Option<T>, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    Ok(match c.vm.heap.native(r)? {
        NativeObject::Engine(e) => e.get::<T>().cloned(),
        _ => None,
    })
}

fn receiver_property(c: &Call<'_>) -> Result<Option<Property>, Stop> {
    Ok(engine::<PropertyValue>(c, c.receiver()?)?.map(|p| p.0))
}

fn receiver_group(c: &Call<'_>) -> Result<Option<PropertyGroup>, Stop> {
    Ok(engine::<GroupValue>(c, c.receiver()?)?.map(|g| g.0))
}

/// A word through the reference at `fp[-2]`, and whether there was one, at `fp[-3]`.
fn answer_word(c: &mut Call<'_>, word: Option<u32>) -> Result<Status, Stop> {
    let out = c.cx.at(-2)?;
    if let Some(w) = word {
        write_out(c, out, w)?;
    } else {
        write_out(c, out, 0)?;
    }
    c.write(-3, u32::from(word.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08392
fn inq_integer(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = receiver_property(c)?.and_then(|p| p.value.inq_integer());
    answer_word(c, v.map(i32::cast_unsigned))
}

// D2-08391
fn inq_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = receiver_property(c)?.and_then(|p| p.value.inq_float());
    answer_word(c, v.map(f32::to_bits))
}

// D2-08390
fn inq_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = receiver_property(c)?.and_then(|p| p.value.inq_enum());
    answer_word(c, v)
}

/// A vector into the vector the reference at `fp[-2]` reaches; whether there was one.
// D2-08400
fn inq_vector(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = receiver_property(c)?.and_then(|p| p.value.inq_vector());
    let out = c.cx.at(-2)?;
    let target = reached(c, out)?;
    if let (Some(v), true) = (v, target != NULL) {
        *c.vm.heap.native_mut(target)? =
            NativeObject::Vector([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]);
    }
    c.write(-3, u32::from(v.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// A string value into the string the reference at `fp[-2]` reaches; whether there was one.
// D2-08399
fn inq_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = receiver_property(c)?.and_then(|p| p.value.inq_string().map(str::to_owned));
    let out = c.cx.at(-2)?;
    if let Some(v) = &v {
        write_text(c, out, v)?;
    }
    c.write(-3, u32::from(v.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// Its value's declared type, as a number.
// D2-08372
fn property_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = receiver_property(c)?.map_or(0, |p| p.value_type);
    c.write(-2, t, tag::MIXED)?;
    Ok(Status::Success)
}

/// The name a property's or a group's number has in the master list.
fn listed_name(c: &mut Call<'_>, id: u32) -> Option<String> {
    let db = c.host.database()?;
    db.property_list()?.name_of(id).map(str::to_owned)
}

// D2-08396
fn property_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_property(c)?.map_or(0, |p| p.id);
    let name = listed_name(c, id);
    let out = c.cx.at(-2)?;
    if let Some(n) = &name {
        write_text(c, out, n)?;
    }
    c.write(-3, u32::from(name.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The name of its value's type, as the enumeration of value types has it.
// D2-08397
fn property_type_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = receiver_property(c)?.map_or(0, |p| p.value_type);
    let name = c
        .host
        .database()
        .and_then(|db| db.enumeration_name(VALUE_TYPES, t));
    let out = c.cx.at(-2)?;
    if let Some(n) = &name {
        write_text(c, out, n)?;
    }
    c.write(-3, u32::from(name.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08536
fn group_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    let n = engine::<CollectionValue>(c, c.receiver()?)?.map_or(0, |v| v.0.groups.len());
    c.write(-2, u32::try_from(n).unwrap_or(u32::MAX), tag::MIXED)?;
    Ok(Status::Success)
}

/// The group at an index (`fp[-2]`) into the group the reference at `fp[-3]` reaches; whether there
/// was one (`fp[-4]`).
// D2-08899, D2-16117
fn group_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    let i = c.cx.at(-2)? as usize;
    let g = engine::<CollectionValue>(c, c.receiver()?)?.and_then(|v| v.0.groups.get(i).cloned());
    let out = c.cx.at(-3)?;
    let target = reached(c, out)?;
    let found = g.is_some();
    if let (Some(g), true) = (g, target != NULL) {
        *c.vm.heap.native_mut(target)? = NativeObject::Engine(Engine::new(GroupValue(g)));
    }
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08915
fn group_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_group(c)?.map_or(0, |g| g.group);
    let name = listed_name(c, id);
    let out = c.cx.at(-2)?;
    if let Some(n) = &name {
        write_text(c, out, n)?;
    }
    c.write(-3, u32::from(name.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-09203
fn property_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    let n = receiver_group(c)?.map_or(0, |g| g.properties.len());
    c.write(-2, u32::try_from(n).unwrap_or(u32::MAX), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-01373
fn optional_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    let n = receiver_group(c)?.map_or(0, |g| g.optional.len());
    c.write(-2, u32::try_from(n).unwrap_or(u32::MAX), tag::MIXED)?;
    Ok(Status::Success)
}

/// A property of a list (`fp[-2]` its index) into the property the reference at `fp[-3]` reaches;
/// whether there was one (`fp[-4]`).
pub fn give_property(c: &mut Call<'_>, p: Option<Property>) -> Result<Status, Stop> {
    let out = c.cx.at(-3)?;
    let target = reached(c, out)?;
    let found = p.is_some();
    if let (Some(p), true) = (p, target != NULL) {
        *c.vm.heap.native_mut(target)? = NativeObject::Engine(Engine::new(PropertyValue(p)));
    }
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08914, D2-16095
fn property_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    let i = c.cx.at(-2)? as usize;
    let p = receiver_group(c)?.and_then(|g| g.properties.get(i).cloned());
    give_property(c, p)
}

// D2-08912, D2-16096
fn optional_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    let i = c.cx.at(-2)? as usize;
    let p = receiver_group(c)?.and_then(|g| g.optional.get(i).cloned());
    give_property(c, p)
}

/// A vector as `(x, y, z)` to three places, into the string the reference at `fp[-2]` reaches.
// D2-09021
fn vector_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let NativeObject::Vector([x, y, z]) = *c.vm.heap.native(r)? else {
        return Err(Stop::Host("not a vector".into()));
    };
    let text = format!(
        "({:.3}, {:.3}, {:.3})",
        f32::from_bits(x),
        f32::from_bits(y),
        f32::from_bits(z)
    );
    let out = c.cx.at(-2)?;
    write_text(c, out, &text)?;
    Ok(Status::Success)
}
