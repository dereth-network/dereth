//! The selection natives: what the player's scripts say about the object the player selected.
//!
//! A selection has a type and six numbers: the current health, the damage taken from other
//! players, the maximum health, the current vigor, the vigor lost to other players and the maximum
//! vigor. It packs as its type, then the six numbers only when it is of type 0x40000001; a new
//! selection is all zeros.
//!
//! The script machine holds a selection as its packed form; the host keeps each selection whole
//! beside it, by reference, and takes the whole selection while it still packs to what the machine
//! holds (the numbers of a selection of another type are kept there).

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};

use crate::host::ClientHost;

/// The native package of selections.
pub const PACKAGE: u32 = 0xAB;

/// The type whose selection packs its numbers.
pub const WITH_NUMBERS: u32 = 0x4000_0001;

/// One selection.
// D2-12233, D2-12234
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub kind: u32,
    pub cur_health: u32,
    pub pk_damage: u32,
    pub max_health: u32,
    pub cur_vigor: u32,
    pub pk_vigor_loss: u32,
    pub max_vigor: u32,
}

impl Selection {
    /// The packed form.
    // D2-12235
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = vec![self.kind];
        if self.kind == WITH_NUMBERS {
            w.extend([
                self.cur_health,
                self.pk_damage,
                self.max_health,
                self.cur_vigor,
                self.pk_vigor_loss,
                self.max_vigor,
            ]);
        }
        w.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// A selection from its packed form; what its type does not pack is zero.
    #[must_use]
    pub fn unpack(bytes: &[u8]) -> Self {
        let word = |i: usize| {
            bytes
                .get(i * 4..i * 4 + 4)
                .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let kind = word(0);
        if kind != WITH_NUMBERS {
            return Self {
                kind,
                ..Self::default()
            };
        }
        Self {
            kind,
            cur_health: word(1),
            pk_damage: word(2),
            max_health: word(3),
            cur_vigor: word(4),
            pk_vigor_loss: word(5),
            max_vigor: word(6),
        }
    }
}

/// The natives this module ports: the number within [`PACKAGE`] and the function.
pub const NATIVES: &[(u32, NativeFn)] = &[
    (0x01, eq),
    (0x02, ne),
    (0x03, get_kind),
    (0x04, set_kind),
    (0x05, get_cur_health),
    (0x06, set_cur_health),
    (0x07, get_max_health),
    (0x08, set_max_health),
    (0x09, get_cur_vigor),
    (0x0A, set_cur_vigor),
    (0x0B, get_max_vigor),
    (0x0C, set_max_vigor),
    (0x0D, get_pk_damage),
    (0x0E, set_pk_damage),
    (0x0F, get_pk_vigor_loss),
    (0x10, set_pk_vigor_loss),
];

/// Each native's id with its function.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(PACKAGE, *n), *f))
}

fn host<'a>(c: &'a mut Call<'_>) -> Result<&'a mut ClientHost, Stop> {
    c.host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))
}

/// The selection a reference holds: the host's whole copy while it packs to the machine's bytes,
/// else what the bytes say.
fn read(c: &mut Call<'_>, r: u32) -> Result<Selection, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let bytes = match c.vm.heap.native(r)? {
        NativeObject::Packed { bytes, .. } => bytes.clone(),
        _ => Selection::default().pack(),
    };
    let whole = host(c)?.selections.get(&r).copied();
    Ok(whole
        .filter(|w| w.pack() == bytes)
        .unwrap_or_else(|| Selection::unpack(&bytes)))
}

/// Store a selection in the object a reference names, and keep it whole.
fn store(c: &mut Call<'_>, r: u32, s: &Selection) -> Result<(), Stop> {
    *c.vm.heap.native_mut(r)? = NativeObject::Packed {
        ty: native_type::SELECTION_INFO,
        bytes: s.pack(),
    };
    host(c)?.selections.insert(r, *s);
    Ok(())
}

/// The receiver's selection.
fn this(c: &mut Call<'_>) -> Result<(u32, Selection), Stop> {
    let r = c.receiver()?;
    Ok((r, read(c, r)?))
}

/// A number read into the result slot below the receiver.
fn get(c: &mut Call<'_>, f: impl Fn(&Selection) -> u32) -> Result<Status, Stop> {
    let (_, s) = this(c)?;
    c.write(-2, f(&s), tag::MIXED)?;
    Ok(Status::Success)
}

/// A number set from the argument below the receiver.
fn set(c: &mut Call<'_>, f: impl Fn(&mut Selection, u32)) -> Result<Status, Stop> {
    let (r, mut s) = this(c)?;
    f(&mut s, c.arg(0)?);
    store(c, r, &s)?;
    Ok(Status::Success)
}

/// Whether the receiver equals the selection below it, into the slot below that.
// D2-01515, D2-12232
fn eq(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (_, a) = this(c)?;
    let other = c.arg(0)?;
    let b = read(c, other)?;
    c.write(-3, u32::from(a == b), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-01516, D2-12232
fn ne(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (_, a) = this(c)?;
    let other = c.arg(0)?;
    let b = read(c, other)?;
    c.write(-3, u32::from(a != b), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-18959
fn get_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.kind)
}

// D2-01370
fn set_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.kind = v)
}

// D2-08505
fn get_cur_health(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.cur_health)
}

// D2-01365
fn set_cur_health(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.cur_health = v)
}

// D2-08536
fn get_max_health(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.max_health)
}

// D2-01363
fn set_max_health(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.max_health = v)
}

// D2-09203
fn get_cur_vigor(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.cur_vigor)
}

// D2-01513
fn set_cur_vigor(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.cur_vigor = v)
}

// D2-01521
fn get_max_vigor(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.max_vigor)
}

// D2-01517
fn set_max_vigor(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.max_vigor = v)
}

// D2-08629
fn get_pk_damage(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.pk_damage)
}

// D2-01366
fn set_pk_damage(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.pk_damage = v)
}

// D2-01611
fn get_pk_vigor_loss(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |s| s.pk_vigor_loss)
}

// D2-01514
fn set_pk_vigor_loss(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |s, v| s.pk_vigor_loss = v)
}
