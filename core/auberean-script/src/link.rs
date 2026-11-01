//! The load pass: the library's code made ready to run.
//!
//! The client walks the code once after loading it and rewrites two kinds of instruction:
//!
//! - a native call's operand, a native id, becomes the native itself (here: an index into the
//!   program's list of bound natives);
//! - a call that names its function outright becomes a direct call to the function's code
//!   offset, looked up in the declaring package's own tables.
//!
//! Nothing else changes: package ids, literal indexes and the function ids of calls through the
//! receiver stay as compiled.

use crate::library::{Library, NO_CODE};
use crate::native::{Binding, NativeTable};
use crate::opcode::{self, op};
use crate::value::FunctionId;

/// Why a library could not be linked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// The code ends inside an instruction.
    Truncated { at: usize },
    /// A native call names an id the native table does not know.
    Unbound { native: u32, at: usize },
    /// A call that names its function outright names nothing the tables hold.
    BadDirectCall {
        function: u32,
        at: usize,
        what: String,
    },
}

impl core::fmt::Display for LinkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated { at } => {
                write!(f, "the code ends inside the instruction at word {at}")
            }
            Self::Unbound { native, at } => {
                write!(
                    f,
                    "no native {} for the call at word {at}",
                    crate::value::NativeId(*native)
                )
            }
            Self::BadDirectCall { function, at, what } => {
                write!(f, "the direct call of {function:#x} at word {at}: {what}")
            }
        }
    }
}

impl std::error::Error for LinkError {}

/// A native the program calls: its id and what it is bound to.
#[derive(Clone, Copy, Debug)]
pub struct BoundNative {
    pub id: u32,
    pub binding: Binding,
}

/// The library, linked.
#[derive(Debug)]
pub struct Program {
    pub library: Library,
    /// The code with native calls and direct calls rewritten.
    pub code: Vec<u32>,
    /// The natives the code calls; a native call's operand is an index into this list.
    pub natives: Vec<BoundNative>,
    /// Natives by id, for natives found other than through the code.
    pub table: NativeTable,
}

impl Program {
    /// Link a library against a native table.
    // D2-08019
    pub fn link(library: Library, table: NativeTable) -> Result<Self, LinkError> {
        let mut code = library.code.clone();
        let mut natives: Vec<BoundNative> = Vec::new();
        let mut index: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut at = 0;
        while at < code.len() {
            let len = opcode::length(&code, at).ok_or(LinkError::Truncated { at })?;
            if at + len > code.len() {
                return Err(LinkError::Truncated { at });
            }
            let word = code[at];
            if opcode::low_byte(word) == op::CALL_NATIVE {
                let id = code[at + 1];
                let i = match index.get(&id) {
                    Some(i) => *i,
                    None => {
                        let binding = table.get(id).ok_or(LinkError::Unbound { native: id, at })?;
                        #[allow(clippy::cast_possible_truncation)]
                        let i = natives.len() as u32;
                        natives.push(BoundNative { id, binding });
                        index.insert(id, i);
                        i
                    }
                };
                code[at + 1] = i;
            } else if opcode::low_byte(word) == op::CALL && code[at + 1] & FunctionId::DIRECT != 0 {
                let f = FunctionId(code[at + 1]);
                let offset = library
                    .dispatch(f.package(), FunctionId(f.0 & !FunctionId::DIRECT))
                    .map_err(|what| LinkError::BadDirectCall {
                        function: f.0,
                        at,
                        what,
                    })?;
                code[at] = u32::from(op::CALL_AT);
                code[at + 1] = offset.unwrap_or(NO_CODE);
            }
            at += len;
        }
        Ok(Self {
            library,
            code,
            natives,
            table,
        })
    }

    /// The native a linked native call names.
    #[must_use]
    pub fn native(&self, index: u32) -> Option<&BoundNative> {
        self.natives.get(index as usize)
    }
}
