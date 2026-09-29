//! Notices — the gameplay→UI channel.
//!
//! Notice registration, payloads, and delivery live here.
//!
//! The global event handler keeps a table from notice id to a list of handlers;
//! registration adds one handler, and every handler's destructor unregisters all of its
//! notices. There is no bubbling and no return value.
//!
//! This crate owns the **bus**, not the payloads: what each notice carries and who acts on it
//! belongs to the game model and the screens. [`NoticeId`] names only the notices this client
//! registers for or sends; a notice nobody in this client raises or receives has no entry.

use std::collections::BTreeMap;

use crate::msg::ListenerId;

/// A notice: which gameplay→UI broadcast this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoticeId {
    /// Raised when the dialog factory binds a new dialog's element; `a` carries the context.
    DialogOpened,
    /// Raised when a dialog closes; `a` carries the context.
    DialogClosed,
    /// A clicked data-file text tag.
    DidTagClicked,
    /// A clicked object text tag.
    IidTagClicked,
    /// A clicked object-and-enum text tag.
    IidEnumTagClicked,
    /// A clicked object-and-string text tag; the main chat panel acts on it.
    IidStringTagClicked,
    /// End the character session; `a` carries the `int` argument, forwarded verbatim.
    EndCharacterSession,
    /// Log off: raise the logout confirmation and quit once it is answered.
    Logoff,
}

impl NoticeId {
    /// Every notice, in declaration order.
    pub const ALL: [Self; 8] = [
        Self::DialogOpened,
        Self::DialogClosed,
        Self::DidTagClicked,
        Self::IidTagClicked,
        Self::IidEnumTagClicked,
        Self::IidStringTagClicked,
        Self::EndCharacterSession,
        Self::Logoff,
    ];
}

/// What a notice carries. Deliberately opaque here: the payloads belong to the game model and the
/// screens, and a receiver reads the fields its notice defines.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NoticePayload {
    pub a: u32,
    pub b: u32,
    pub text: Option<String>,
}

/// The notice bus: a table from notice id to a list of handlers.
///
/// A notice with no handler is a **no-op, not an error**: its lookup simply returns an empty
/// list.
#[derive(Debug, Default)]
pub struct NoticeBus {
    handlers: BTreeMap<NoticeId, Vec<ListenerId>>,
}

impl NoticeBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a notice handler. Registering twice adds twice, as the
    /// original list append does.
    pub fn register(&mut self, id: NoticeId, who: ListenerId) {
        self.handlers.entry(id).or_default().push(who);
    }

    pub fn unregister(&mut self, id: NoticeId, who: ListenerId) {
        if let Some(v) = self.handlers.get_mut(&id) {
            if let Some(i) = v.iter().position(|w| *w == who) {
                v.remove(i);
            }
            if v.is_empty() {
                self.handlers.remove(&id);
            }
        }
    }

    /// Drop every notice registration of `who` — retail does this from every handler's destructor.
    pub fn unregister_all(&mut self, who: ListenerId) {
        self.handlers.retain(|_, v| {
            v.retain(|w| *w != who);
            !v.is_empty()
        });
    }

    /// The handlers registered for one notice, in registration order.
    #[must_use]
    pub fn handlers(&self, id: NoticeId) -> &[ListenerId] {
        self.handlers.get(&id).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unknown notice id has no handlers and dispatching it is a no-op.
    #[test]
    fn an_id_with_no_handler_is_a_no_op() {
        let mut b = NoticeBus::new();
        assert!(b.handlers(NoticeId::Logoff).is_empty());
        let who = ListenerId::External(1);
        b.register(NoticeId::Logoff, who);
        assert_eq!(b.handlers(NoticeId::Logoff), &[who]);
        assert!(b.handlers(NoticeId::EndCharacterSession).is_empty());
        b.unregister_all(who);
        assert!(b.is_empty());
    }
}
