//! A friend's state as the friends service gives it (its name, its mode, where it is), the native
//! object scripts get it in, and the three natives that read it.
//!
//! The mode is a bit set: nothing set is offline, 1 online, 2 away, 4 not to be disturbed, 8 in a
//! game, 16 in a chat. The location is the service's text about where the friend is, empty when
//! it says nothing.

use std::sync::Arc;

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::{tag, NULL};
use auberean_script::wire::{In, Out};

/// The native type of a friend's state.
pub const USER_STATUS: u32 = 0xAF;

/// A friend's state.
// D2-12146, D2-12148, D2-12140, D2-12147, D2-12141, D2-12142, D2-12144
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserStatus {
    pub name: Vec<u16>,
    pub location: Vec<u16>,
    pub mode: u32,
}

impl UserStatus {
    /// Its packed form: the name, the location, the mode.
    // D2-12149
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        w.wide(&self.name);
        w.wide(&self.location);
        w.u32(self.mode);
        w.buf
    }

    /// One read from its packed form.
    #[must_use]
    pub fn unpack(bytes: &[u8]) -> Option<Self> {
        let mut r = In::new(bytes);
        let name = r.wide().ok()?;
        let location = r.wide().ok()?;
        let mode = r.u32().ok()?;
        Some(Self {
            name,
            location,
            mode,
        })
    }

    /// The state a native object holds: one no native has touched yet is empty; one read from a
    /// packed graph is unpacked.
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        match o {
            NativeObject::Engine(e) => e.get::<Self>().cloned(),
            NativeObject::Opaque { ty } if *ty == USER_STATUS => Some(Self::default()),
            NativeObject::Packed { ty, bytes } if *ty == USER_STATUS => Self::unpack(bytes),
            _ => None,
        }
    }

    /// A native object holding it.
    #[must_use]
    pub fn native(self) -> NativeObject {
        NativeObject::Engine(Engine::new(self))
    }
}

impl EngineValue for UserStatus {
    fn native_type(&self) -> u32 {
        USER_STATUS
    }

    fn clone_value(&self) -> Box<dyn EngineValue> {
        Box::new(self.clone())
    }

    fn same(&self, other: &dyn EngineValue) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn pack(&self) -> Option<Vec<u8>> {
        Some(Self::pack(self))
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

/// The receiver's state; `None` for the null reference.
fn receiver(c: &mut Call<'_>) -> Result<Option<UserStatus>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let o = c.vm.heap.native(r)?;
    UserStatus::of(o)
        .map(Some)
        .ok_or_else(|| c.fail("the receiver is not a friend's state"))
}

/// Put `text` into the string an out reference (`fp[-2]`) reaches; answer 1, or 0 leaving the
/// string as it was when `text` is empty.
fn into_string(c: &mut Call<'_>, text: &[u16]) -> Result<Status, Stop> {
    if text.is_empty() {
        c.write(-3, 0, tag::MIXED)?;
        return Ok(Status::Success);
    }
    let out = c.cx.at(-2)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Ok(Status::Crash);
    }
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => text.clone_into(t),
        _ => return Err(c.fail("the out reference holds no string")),
    }
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The friend's name, into the string an out reference reaches; 0 when it has none.
// D2-01555, D2-12143
fn inq_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(s) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    into_string(c, &s.name)
}

/// Where the friend is, into the string an out reference reaches; 0 when the service said
/// nothing.
// D2-01553, D2-12145
fn inq_location(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(s) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    into_string(c, &s.location)
}

/// The friend's mode, into the number an out reference reaches; answers 1.
// D2-01554, D2-12139
fn inq_mode(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(s) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let out = c.cx.at(-2)?;
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, Some(s.mode), tag::MIXED, &prog)?;
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

const NATIVES: &[(u32, u32, NativeFn)] = &[
    (USER_STATUS, 0x01, inq_name),
    (USER_STATUS, 0x02, inq_location),
    (USER_STATUS, 0x03, inq_mode),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the packed form's round trip)
    use super::*;

    #[test]
    fn a_friends_state_reads_back_from_its_packed_form() {
        let s = UserStatus {
            name: "Cara".encode_utf16().collect(),
            location: Vec::new(),
            mode: 3,
        };
        assert_eq!(UserStatus::unpack(&s.pack()), Some(s.clone()));
        assert_eq!(UserStatus::of(&s.clone().native()), Some(s));
    }
}
