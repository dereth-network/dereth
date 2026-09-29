//! The host's text conversion, as a value the model can hold.
//!
//! `ChatState::text_conversion` is a handle, not a `u32` wrapper that calls `kernel32` directly:
//! the chat model, `taboo.rs` and `turbine.rs` must not reach Windows FFI from production code,
//! and the NLS arm is a dependency of `dereth-client-model` for its tests alone. Every
//! conversion goes through [`dereth_primitives::HostEncoding`], which the application implements.
//!
//! # Why a handle and not a code page
//!
//! The alternative is a plain `codepage: u32` that this crate resolves against
//! `dereth_protocol::cp1252`. That deletes the `kernel32` path outright — every
//! Windows build would narrow through the 1252 table, losing the live `CP_ACP` on a host whose
//! code page is not 1252 — and that is a behaviour change. (For 1252 itself the table is Windows'
//! answer, best-fit mappings included.) The handle keeps the real NLS path reachable and lets a
//! headless, browser or non-Windows build answer with the table instead.
//!
//! The default is the table (`dereth_protocol::cp1252::Cp1252`), because a crate with no platform
//! cannot have the host's ACP for a default. `dereth_client::platform::text::install` puts the
//! `kernel32` arm on the production `World`, so nothing about the shipped client changes.

use std::sync::Arc;

use dereth_primitives::HostEncoding;

/// A shared handle to the host's [`HostEncoding`].
///
/// `Arc` rather than `Box` for the reason `Scroll`'s output handle is shared: `ChatState` and
/// `Scroll` are `Clone`, a `World` is cloned for snapshots, and the conversion is one process-wide
/// setting: the host ANSI code page is read once.
#[derive(Clone, Debug)]
pub struct HostText(Arc<dyn HostEncoding>);

impl HostText {
    /// Install a host implementation.
    #[must_use]
    pub fn new(encoding: Arc<dyn HostEncoding>) -> Self {
        Self(encoding)
    }

    /// The implementation, for a caller that would rather name the trait than deref.
    #[must_use]
    pub fn as_encoding(&self) -> &dyn HostEncoding {
        &*self.0
    }
}

/// Windows-1252, the table `dereth_primitives::text` converts through without `host-nls`.
impl Default for HostText {
    fn default() -> Self {
        Self(Arc::new(dereth_protocol::cp1252::Cp1252))
    }
}

impl std::ops::Deref for HostText {
    type Target = dyn HostEncoding;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl From<Arc<dyn HostEncoding>> for HostText {
    fn from(encoding: Arc<dyn HostEncoding>) -> Self {
        Self(encoding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default must be a real table and not a stub: a crate whose default conversion dropped
    /// every non-ASCII byte would pass most of this crate's tests and break the client.
    #[test]
    fn the_default_is_the_windows_1252_table() {
        let host = HostText::default();
        assert_eq!(host.acp(), 1252);
        assert_eq!(host.narrow(&[0x41, 0xe9]), Some(b"A\xe9".to_vec()));
        assert_eq!(host.widen(b"\x80"), Some(vec![0x20ac]));
    }
}
