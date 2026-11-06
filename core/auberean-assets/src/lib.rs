//! One decoder per Asheron's Call 2 record type, each held to exact consumption.
//!
//! **Depends on** `auberean-dat` (the container and the record reader) and `auberean-primitives`. **Used
//! by** everything that needs a record's contents: the world, drawing, sound, the user interface,
//! a server.
//!
//! Every record type has a type in this crate implementing [`Decode`]; [`decode`] reads a record
//! with it and succeeds only when every byte was consumed, so "decodes" always means every byte
//! was accounted for. [`Record`] holds any one record by its type, and [`verify`] runs the
//! decoders over a whole data file and counts what decoded exactly and what failed.
//!
//! Records that hold properties read each value by the type the install's property list declares
//! for it, so decoding them needs a [`Context`] made from the install's portal file.
//!
//! **Must never** do I/O: bytes come in, values come out.

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};

pub mod animation;
pub mod bsp;
pub mod cache;
pub mod common;
pub mod context;
pub mod database;
pub mod entity;
pub mod fx;
pub mod keymap;
pub mod landblock;
pub mod lookup;
pub mod material;
pub mod mesh;
pub mod names;
pub mod picture;
pub mod property;
pub mod record;
pub mod region;
pub mod requests;
pub mod rle;
pub mod script;
pub mod setup;
pub mod sound;
pub mod strings;
pub mod tables;
pub mod ui;
mod ui_pack;
pub mod verify;
pub mod visual;

pub use cache::Obj;
pub use context::Context;
pub use database::Database;
pub use record::Record;

/// A record type's decoder.
pub trait Decode: Sized {
    /// Read the record from its first byte. `id` is the id it is filed under; `ctx` holds what
    /// the install's other records say that this one needs.
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self>;
}

/// A record type's writer: the record as the client packs it, so that a decoded record encodes
/// to the bytes it was decoded from.
pub trait Encode {
    /// Write the record from its first byte; `id` is the id it is filed under.
    fn encode(&self, id: u32, w: &mut Writer);
}

/// A whole record's bytes.
// D2-03386, D2-03547, D2-03546
#[must_use]
pub fn encode<T: Encode>(id: u32, record: &T) -> Vec<u8> {
    let mut w = Writer::new();
    record.encode(id, &mut w);
    w.finish()
}

/// Decode a whole record, failing unless every byte is consumed.
// D2-03387, D2-00239
pub fn decode<T: Decode>(id: u32, bytes: &[u8], ctx: &Context) -> Result<T> {
    let mut r = Reader::new(bytes);
    let v = T::decode(&mut r, id, ctx)?;
    r.finish()?;
    Ok(v)
}

/// A placeholder for a record type whose layout is not decoded yet: decoding it always fails, so
/// [`verify`] counts its records as not decoded.
macro_rules! undecoded {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name {
            _private: (),
        }

        impl $crate::Decode for $name {
            fn decode(
                r: &mut ::auberean_dat::Reader<'_>,
                _id: u32,
                _ctx: &$crate::Context,
            ) -> ::auberean_dat::reader::Result<Self> {
                r.fail("not decoded yet")
            }
        }
    };
}
pub(crate) use undecoded;
