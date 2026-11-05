//! Listeners: whoever hears what elements say and what the manager announces. An element is a
//! listener in its own right; a panel, the screen flow or anything else outside the element tree
//! is a [`ListenerId`] made here, and what it hears is kept as [`Mail`] for it to take.
//!
//! A listener keeps the list of what it registered for, so that going away undoes all of it; the
//! manager keeps how many times a listener registered for the same global message beyond the
//! first, so that each registration needs its own undoing.

use crate::system::{Handle, Ui};
use crate::token::Token;

/// A listener that is not an element: a panel, the screen flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ListenerId(pub u32);

/// Whoever hears messages: an element, or a listener outside the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Listener {
    Element(Handle),
    Other(ListenerId),
}

impl From<Handle> for Listener {
    fn from(h: Handle) -> Self {
        Self::Element(h)
    }
}

impl From<ListenerId> for Listener {
    fn from(l: ListenerId) -> Self {
        Self::Other(l)
    }
}

/// What a listener outside the tree heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mail {
    /// An element said something: its name, the message, the detail.
    Element {
        from: Handle,
        element: Token,
        what: Token,
        param: u32,
    },
    /// The manager announced one of its global messages (see [`crate::manager::global`]).
    Global { what: u32, param: u32 },
    /// A framework tells a child framework to show or hide by its own flag (see
    /// [`crate::framework`]).
    Show { shown: bool },
    /// The screen's kept places were put back.
    PlacesPutBack,
}

/// What one listener outside the tree registered for, in order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Registrations {
    pub messages: Vec<(Token, Token)>,
    pub globals: Vec<u32>,
}

impl Ui {
    /// A new listener outside the tree, registered for nothing.
    // D2-11410
    pub fn new_listener(&mut self) -> ListenerId {
        let id = ListenerId(self.mgr.next_listener);
        self.mgr.next_listener += 1;
        self.mgr.registrations.insert(id, Registrations::default());
        id
    }

    /// A listener goes away: every registration undone (a global one as many times as it was
    /// made), and what it had not taken dropped.
    // D2-11411, D2-11409
    pub fn drop_listener(&mut self, l: ListenerId) {
        self.unregister_for_all_messages(l);
        self.unregister_for_all_global_messages(l);
        let who = Listener::Other(l);
        let extra: Vec<(u32, i32)> = self
            .mgr
            .global_counts
            .iter()
            .filter(|((x, _), _)| *x == who)
            .map(|((_, m), n)| (*m, *n))
            .collect();
        for (m, n) in extra {
            for _ in 0..n.max(0) {
                self.unregister_for_global_message(who, m);
            }
        }
        self.clean_queues();
        self.mgr.registrations.remove(&l);
        self.mgr.mail.remove(&l);
    }

    /// Have a listener outside the tree hear what elements named `element` say (`what`), noted
    /// so that going away undoes it.
    // D2-11415
    pub fn listen_for_message(&mut self, l: ListenerId, element: Token, what: Token) {
        self.register_for_message(Listener::Other(l), element, what);
        if let Some(r) = self.mgr.registrations.get_mut(&l) {
            r.messages.push((element, what));
        }
    }

    /// Have a listener outside the tree hear one of the manager's global messages, noted so that
    /// going away undoes it.
    // D2-11416
    pub fn listen_for_global_message(&mut self, l: ListenerId, what: u32) {
        self.register_for_global_message(Listener::Other(l), what);
        if let Some(r) = self.mgr.registrations.get_mut(&l) {
            r.globals.push(what);
        }
    }

    /// Undo every element message a listener registered for.
    // D2-11405
    pub fn unregister_for_all_messages(&mut self, l: ListenerId) {
        let list = self
            .mgr
            .registrations
            .get_mut(&l)
            .map(|r| std::mem::take(&mut r.messages))
            .unwrap_or_default();
        for (e, w) in list {
            self.unregister_for_message(Listener::Other(l), e, w);
        }
    }

    /// Undo every global message a listener registered for.
    // D2-11401
    pub fn unregister_for_all_global_messages(&mut self, l: ListenerId) {
        let list = self
            .mgr
            .registrations
            .get_mut(&l)
            .map(|r| std::mem::take(&mut r.globals))
            .unwrap_or_default();
        for w in list {
            self.unregister_for_global_message(Listener::Other(l), w);
        }
    }

    /// What a listener outside the tree heard since it last looked, oldest first.
    pub fn take_mail(&mut self, l: ListenerId) -> Vec<Mail> {
        self.mgr
            .mail
            .get_mut(&l)
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Put mail in a listener's box.
    pub(crate) fn post(&mut self, l: ListenerId, m: Mail) {
        if self.mgr.registrations.contains_key(&l) {
            self.mgr.mail.entry(l).or_default().push(m);
        }
    }

    /// How many times beyond the first a listener registered for a global message.
    // D2-11400
    pub(crate) fn extra_registrations(&self, who: Listener, what: u32) -> i32 {
        self.mgr
            .global_counts
            .get(&(who, what))
            .copied()
            .unwrap_or(0)
    }

    /// One more registration for a global message beyond the first.
    // D2-11403
    pub(crate) fn increment_message(&mut self, who: Listener, what: u32) {
        *self.mgr.global_counts.entry((who, what)).or_insert(0) += 1;
    }

    /// One fewer registration beyond the first; at none the count is dropped.
    // D2-11404
    pub(crate) fn decrement_message(&mut self, who: Listener, what: u32) {
        if let Some(n) = self.mgr.global_counts.get_mut(&(who, what)) {
            *n -= 1;
            if *n <= 0 {
                self.mgr.global_counts.remove(&(who, what));
            }
        }
    }

    /// Mark a listener registered for an element's message (once).
    // D2-11414, D2-11399
    pub(crate) fn increment_element_message(&mut self, who: Listener, element: Token, what: Token) {
        self.mgr.element_marks.insert((who, element, what));
    }

    /// Clear that mark.
    // D2-11408
    pub(crate) fn decrement_element_message(&mut self, who: Listener, element: Token, what: Token) {
        self.mgr.element_marks.remove(&(who, element, what));
    }
}
