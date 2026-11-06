//! A character's creation choices as the character generator's scripts fill and read them: its race,
//! sex, class, implement, starting area and physique values.
//!
//! A new one holds zeros and no physique values. A physique type with no value reads as -1. Setting
//! a physique value, the class or the implement answers 1; the other setters answer nothing. A read
//! or a change through the null reference crashes the script.

use std::collections::BTreeMap;

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::NULL;

/// The native type of a creation's choices.
pub const CREATION_CHOICES: u32 = 0x9E;

/// What a physique type with no value reads as.
const NO_VALUE: u32 = 0xBF80_0000;

/// A character's creation choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreationChoices {
    pub race: u32,
    pub sex: u32,
    pub class: u32,
    pub implement: u32,
    pub start_area: u32,
    /// Per physique type, the value's bits.
    pub physique: BTreeMap<u32, u32>,
}

impl CreationChoices {
    /// A native object holding it.
    #[must_use]
    pub fn native(self) -> NativeObject {
        NativeObject::Engine(Engine::new(self))
    }

    /// The choices a native object holds.
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<&Self> {
        match o {
            NativeObject::Engine(e) => e.get::<Self>(),
            _ => None,
        }
    }
}

impl EngineValue for CreationChoices {
    fn native_type(&self) -> u32 {
        CREATION_CHOICES
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

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (CREATION_CHOICES, 0x03, get::<RACE>),
    (CREATION_CHOICES, 0x04, set::<RACE>),
    (CREATION_CHOICES, 0x05, get::<SEX>),
    (CREATION_CHOICES, 0x06, set::<SEX>),
    (CREATION_CHOICES, 0x07, set_answering::<CLASS>),
    (CREATION_CHOICES, 0x08, get::<CLASS>),
    (CREATION_CHOICES, 0x09, set_answering::<IMPLEMENT>),
    (CREATION_CHOICES, 0x0B, set_physique_value),
    (CREATION_CHOICES, 0x0C, physique_value),
    (CREATION_CHOICES, 0x0E, set::<START_AREA>),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

const RACE: usize = 0;
const SEX: usize = 1;
const CLASS: usize = 2;
const IMPLEMENT: usize = 3;
const START_AREA: usize = 4;

/// One of the word fields.
fn field(r: &mut CreationChoices, f: usize) -> &mut u32 {
    match f {
        RACE => &mut r.race,
        SEX => &mut r.sex,
        CLASS => &mut r.class,
        IMPLEMENT => &mut r.implement,
        _ => &mut r.start_area,
    }
}

/// The receiver's choices; `None` for the null reference.
fn receiver<'a>(c: &'a mut Call<'_>) -> Result<Option<&'a mut CreationChoices>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Engine(e) => match e.get_mut::<CreationChoices>() {
            Some(v) => Ok(Some(v)),
            None => Err(Stop::Native {
                native,
                what: "the receiver is not a creation's choices".into(),
            }),
        },
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not a creation's choices".into(),
        }),
    }
}

/// Read a word field.
// D2-01589, D2-01422, D2-01586
fn get<const F: usize>(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(r) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let v = *field(r, F);
    c.write_mixed(-2, v)?;
    Ok(Status::Success)
}

/// Set a word field, answering nothing.
// D2-01583, D2-01584, D2-01585
fn set<const F: usize>(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    let Some(r) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    *field(r, F) = v;
    Ok(Status::Success)
}

/// Set a word field, answering 1.
// D2-01593, D2-01594, D2-11910
fn set_answering<const F: usize>(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    let Some(r) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    *field(r, F) = v;
    c.write_mixed(-3, 1)?;
    Ok(Status::Success)
}

/// A physique type's value, or -1 for a type with none.
// D2-01588
fn physique_value(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = c.arg(0)?;
    let Some(r) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let v = r.physique.get(&t).copied().unwrap_or(NO_VALUE);
    c.write_mixed(-3, v)?;
    Ok(Status::Success)
}

/// Set a physique type's value, answering 1.
// D2-01595
fn set_physique_value(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (t, v) = (c.arg(0)?, c.arg(1)?);
    let Some(r) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    r.physique.insert(t, v);
    c.write_mixed(-4, 1)?;
    Ok(Status::Success)
}
