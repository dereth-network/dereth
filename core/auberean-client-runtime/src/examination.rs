//! The examination request natives: what the player's scripts ask the server to examine.
//!
//! A request has a kind (an object, an attackable target, an effect, a quest, a skill...), an
//! enumerated datum, a data id, an admin word, the object it is about, a combat mode, a skill level
//! and the tabs it asks for (0x203 when new). It packs as its kind and its admin word, then by
//! kind: the enumerated datum (kinds 1, 3 to 6); the object and the tabs (an object, 2); the data
//! id (7 and 11); the object, the mode, the skill level and the tabs (a target, 13); nothing for
//! the others. A new request is of kind 0, asking about nothing.
//!
//! The script machine holds a request as its packed form; the host keeps each request whole beside
//! it, by reference, and takes the whole request while it still packs to what the machine holds
//! (a field the kind does not pack, such as the tabs of a request about nothing, is kept there).

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};

use crate::host::ClientHost;

/// Examination profiles, as the scripts build them and the examination window reads them.
pub use auberean_glue::examination::{kind, Node, Profile};

/// The native package of examination requests.
pub const PACKAGE: u32 = 0x9A;

/// The kinds whose request carries the object it is about.
pub const OBJECT: u32 = 0x4000_0002;
pub const TARGET: u32 = 0x4000_000D;

/// One examination request. A copy is a copy of every field.
// D2-12228, D2-12230
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub kind: u32,
    pub data_enum: u32,
    pub data_id: u32,
    pub admin: u32,
    pub object: u64,
    pub mode: u32,
    pub skill_level: u32,
    /// The tabs asked for.
    pub current: u32,
}

impl Default for Request {
    // D2-12229, D2-12226
    fn default() -> Self {
        Self {
            kind: 0,
            data_enum: 0,
            data_id: 0,
            admin: 0,
            object: 0,
            mode: 0,
            skill_level: 0,
            current: 0x203,
        }
    }
}

impl Request {
    /// The packed form.
    // D2-12231
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w: Vec<u32> = vec![self.kind, self.admin];
        #[allow(clippy::cast_possible_truncation)]
        let (lo, hi) = (self.object as u32, (self.object >> 32) as u32);
        match self.kind {
            0x4000_0001 | 0x4000_0003..=0x4000_0006 => w.push(self.data_enum),
            OBJECT => w.extend([lo, hi, self.current]),
            0x4000_0007 | 0x4000_000B => w.push(self.data_id),
            TARGET => w.extend([lo, hi, self.mode, self.skill_level, self.current]),
            _ => {}
        }
        w.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// A request from its packed form; what its kind does not pack takes its default.
    #[must_use]
    pub fn unpack(bytes: &[u8]) -> Self {
        let word = |i: usize| {
            bytes
                .get(i * 4..i * 4 + 4)
                .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let mut r = Self {
            kind: word(0),
            admin: word(1),
            ..Self::default()
        };
        let long = |i: usize| u64::from(word(i)) | u64::from(word(i + 1)) << 32;
        match r.kind {
            0x4000_0001 | 0x4000_0003..=0x4000_0006 => r.data_enum = word(2),
            OBJECT => {
                r.object = long(2);
                r.current = word(4);
            }
            0x4000_0007 | 0x4000_000B => r.data_id = word(2),
            TARGET => {
                r.object = long(2);
                r.mode = word(4);
                r.skill_level = word(5);
                r.current = word(6);
            }
            _ => {}
        }
        r
    }
}

/// The interface's function that adds an effect's lines to a profile (the line's depth, its
/// order, the effect, the profile).
pub const ADD_EFFECT_DATA: u32 = 0x00B4_00FF;

/// The scripts' notice of a profile of what is examined.
pub const UPDATE_EXAMINATION_PROFILE: u32 = 0x0089 << 16 | 0x0001;

impl crate::host::Scripts {
    /// The lines of an effect line of a profile, as the examination window asks the scripts for
    /// them when it lays the line out: a new profile whose request about nothing asks for the
    /// line's tabs, filled by the interface's function from the line's depth (its appearance), its
    /// order (its tabs) and the effect. `None` when there is no player here or the call did not
    /// run to its end.
    // D2-02603, D2-00524, D2-01071
    pub fn effect_lines(&mut self, depth: u32, order: u32, eid: u32) -> Option<Profile> {
        let mut a = crate::skills::Asker {
            s: self,
            incomplete: Vec::new(),
        };
        let given = Profile::asking(order).native();
        let answer = a.ask_with(
            None,
            ADD_EFFECT_DATA,
            &[crate::skills::Slot::Given(
                native_type::EXAMINATION_PROFILE,
                Box::new(given),
            )],
            &[eid, order, depth],
            false,
        )?;
        if answer.result != crate::host::CallResult::Completed {
            return None;
        }
        answer
            .objects
            .into_iter()
            .next()
            .flatten()
            .and_then(|o| Profile::of(&o))
    }
}

/// The natives this module ports: the number within [`PACKAGE`] and the function.
pub const NATIVES: &[(u32, NativeFn)] = &[
    (0x01, eq),
    (0x02, ne),
    (0x03, get_kind),
    (0x04, get_data_enum),
    (0x06, set_kind),
    (0x07, set_data_enum),
    (0x08, set_object),
    (0x09, get_data_id),
    (0x0A, get_admin),
    (0x0B, set_data_id),
    (0x0C, set_admin),
    (0x0D, set_mode),
    (0x0E, get_mode),
    (0x0F, set_skill_level),
    (0x10, get_skill_level),
    (0x11, set_current),
    (0x12, get_current),
];

/// Each native's id with its function.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(PACKAGE, *n), *f))
        .chain(std::iter::once((
            NativeId::new(PROFILE_PACKAGE, 0x03),
            profile_set_request as NativeFn,
        )))
}

/// The native package of examination profiles.
const PROFILE_PACKAGE: u32 = 0x99;

/// The request below the receiver copied into the profile whole: the tabs it asks for go with it,
/// though a request about anything but an object packs none.
// D2-01359, D2-12228
fn profile_set_request(c: &mut Call<'_>) -> Result<Status, Stop> {
    let q = c.arg(0)?;
    let whole = read(c, q)?;
    auberean_glue::examination::take_request(c, whole.pack(), Some(whole.current))
}

fn host<'a>(c: &'a mut Call<'_>) -> Result<&'a mut ClientHost, Stop> {
    c.host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))
}

/// The request a reference holds: the host's whole copy while it packs to the machine's bytes,
/// else what the bytes say.
fn read(c: &mut Call<'_>, r: u32) -> Result<Request, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let bytes = match c.vm.heap.native(r)? {
        NativeObject::Packed { bytes, .. } => bytes.clone(),
        _ => Request::default().pack(),
    };
    let whole = host(c)?.requests.get(&r).copied();
    Ok(whole
        .filter(|w| w.pack() == bytes)
        .unwrap_or_else(|| Request::unpack(&bytes)))
}

/// Store a request in the object a reference names, and keep it whole.
fn store(c: &mut Call<'_>, r: u32, req: &Request) -> Result<(), Stop> {
    let ty = native_type::EXAMINATION_REQUEST;
    *c.vm.heap.native_mut(r)? = NativeObject::Packed {
        ty,
        bytes: req.pack(),
    };
    host(c)?.requests.insert(r, *req);
    Ok(())
}

/// The receiver's request.
fn this(c: &mut Call<'_>) -> Result<(u32, Request), Stop> {
    let r = c.receiver()?;
    Ok((r, read(c, r)?))
}

/// A field read into the result slot below the receiver.
fn get(c: &mut Call<'_>, f: impl Fn(&Request) -> u32) -> Result<Status, Stop> {
    let (_, req) = this(c)?;
    c.write(-2, f(&req), tag::MIXED)?;
    Ok(Status::Success)
}

/// A field set from the argument below the receiver.
fn set(c: &mut Call<'_>, f: impl Fn(&mut Request, u32)) -> Result<Status, Stop> {
    let (r, mut req) = this(c)?;
    f(&mut req, c.arg(0)?);
    store(c, r, &req)?;
    Ok(Status::Success)
}

/// Whether the receiver equals the request below it, into the slot below that.
// D2-01374, D2-12227
fn eq(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (_, a) = this(c)?;
    let other = c.arg(0)?;
    let b = read(c, other)?;
    c.write(-3, u32::from(a == b), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-01375, D2-12227
fn ne(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (_, a) = this(c)?;
    let other = c.arg(0)?;
    let b = read(c, other)?;
    c.write(-3, u32::from(a != b), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-18959
fn get_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.kind)
}

// D2-08505
fn get_data_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.data_enum)
}

// D2-01370
fn set_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.kind = v)
}

// D2-01365
fn set_data_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.data_enum = v)
}

/// The object, a 64-bit argument below the receiver.
// D2-01367
fn set_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (r, mut req) = this(c)?;
    req.object = c.long_at(-3)?;
    store(c, r, &req)?;
    Ok(Status::Success)
}

// D2-08629
fn get_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.data_id)
}

// D2-08536
fn get_admin(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.admin)
}

// D2-01366
fn set_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.data_id = v)
}

// D2-01363
fn set_admin(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.admin = v)
}

// D2-01368
fn set_mode(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.mode = v)
}

// D2-01372
fn get_mode(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.mode)
}

// D2-01369
fn set_skill_level(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.skill_level = v)
}

// D2-01373
fn get_skill_level(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.skill_level)
}

// D2-01364
fn set_current(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |r, v| r.current = v)
}

// D2-01371
fn get_current(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |r| r.current)
}
