//! Script calls: most gameplay travels as a call of a script function, named by a 32-bit id,
//! with a packed argument buffer. The client calls the server's functions (one carrier); the
//! server calls the client's, either for the player or naming an object (three carriers).
//!
//! The envelope is read here and keeps the argument buffer as sent ([`ScriptArgs`]), so a
//! message decodes and encodes without the script library. The buffer's values are the script
//! machine's packed values ([`Arg`]): [`ScriptArgs::values`] reads them against the library (an
//! object argument is a packed object graph whose packages the library names) and
//! [`ScriptArgs::from_values`] packs them. [`ArgumentCodec`] is the same pair as a trait, which
//! [`LibraryCodec`] implements.

use alloc::string::String;
use alloc::vec::Vec;

pub use auberean_script::args::Arg;
pub use auberean_script::value::FunctionId;
use auberean_script::Library;

use crate::types::ObjectStamp;
use crate::wire::{Field, Reader, Result, Writer};

/// A call's packed arguments, as sent: the called function's parameters, last first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptArgs(pub Vec<u8>);

impl ScriptArgs {
    /// The values, read against the script library: every value, every byte.
    ///
    /// # Errors
    /// A value of an unknown tag, an object graph the library cannot read, or bytes left over.
    pub fn values(
        &self,
        lib: &Library,
    ) -> core::result::Result<Vec<Arg>, auberean_script::wire::CodecError> {
        auberean_script::args::decode(&self.0, lib)
    }

    /// Values packed for sending, in the order given (the called function's last parameter
    /// first).
    ///
    /// # Errors
    /// An object graph that cannot be packed.
    pub fn from_values(values: &[Arg]) -> core::result::Result<Self, String> {
        auberean_script::args::encode(values).map(Self)
    }

    /// Read these arguments with a codec.
    ///
    /// # Errors
    /// The codec's.
    pub fn read_with<C: ArgumentCodec>(
        &self,
        codec: &C,
        function: FunctionId,
    ) -> core::result::Result<C::Args, C::Error> {
        codec.read_args(function, &self.0)
    }

    /// Pack arguments with a codec.
    ///
    /// # Errors
    /// The codec's.
    pub fn write_with<C: ArgumentCodec>(
        codec: &C,
        function: FunctionId,
        args: &C::Args,
    ) -> core::result::Result<Self, C::Error> {
        codec.write_args(function, args).map(Self)
    }
}

/// Reads and writes a call's arguments.
pub trait ArgumentCodec {
    /// The arguments, read.
    type Args;
    type Error;

    /// Read the arguments of a call of `function`.
    ///
    /// # Errors
    /// The bytes are not that function's arguments.
    fn read_args(
        &self,
        function: FunctionId,
        bytes: &[u8],
    ) -> core::result::Result<Self::Args, Self::Error>;

    /// The bytes of a call of `function` with these arguments.
    ///
    /// # Errors
    /// The arguments cannot be packed.
    fn write_args(
        &self,
        function: FunctionId,
        args: &Self::Args,
    ) -> core::result::Result<Vec<u8>, Self::Error>;
}

/// The script machine's packed values, read against a script library.
#[derive(Debug, Clone, Copy)]
pub struct LibraryCodec<'a>(pub &'a Library);

impl ArgumentCodec for LibraryCodec<'_> {
    type Args = Vec<Arg>;
    type Error = String;

    fn read_args(&self, _: FunctionId, bytes: &[u8]) -> core::result::Result<Vec<Arg>, String> {
        auberean_script::args::decode(bytes, self.0).map_err(|e| alloc::format!("{e}"))
    }

    fn write_args(&self, _: FunctionId, args: &Vec<Arg>) -> core::result::Result<Vec<u8>, String> {
        auberean_script::args::encode(args)
    }
}

/// The client calls a server function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerCall {
    pub function: FunctionId,
    pub args: ScriptArgs,
}

impl Field for ServerCall {
    // D2-03980
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let function = FunctionId(r.u32()?);
        let n = r.count()?;
        Ok(Self {
            function,
            args: ScriptArgs(r.bytes(n)?.to_vec()),
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.function.0);
        w.count(self.args.0.len());
        w.bytes(&self.args.0);
    }
}

/// The server calls a client function. A call with no arguments may end after the id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientCall {
    pub function: FunctionId,
    pub args: Option<ScriptArgs>,
}

impl Field for ClientCall {
    // D2-03978
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let function = FunctionId(r.u32()?);
        r.align()?;
        let args = if r.remaining() >= 4 {
            let n = r.count()?;
            Some(ScriptArgs(r.bytes(n)?.to_vec()))
        } else {
            None
        };
        Ok(Self { function, args })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.function.0);
        w.align();
        if let Some(a) = &self.args {
            w.count(a.0.len());
            w.bytes(&a.0);
        }
    }
}

/// The server calls a client function on an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectCall {
    pub object: ObjectStamp,
    pub call: ClientCall,
}

impl Field for ObjectCall {
    // D2-03979
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            object: ObjectStamp::read(r)?,
            call: ClientCall::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        self.object.write(w);
        self.call.write(w);
    }
}
