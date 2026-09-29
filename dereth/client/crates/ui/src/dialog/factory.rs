//! The dialog controller — the queues and the public API.
//!
//! The factory maps dialog kinds to their runtime implementations.
//!
//! All the state is static in the client. Here it is one owned object, held by
//! [`crate::UiSystem`], because dialog reset runs on framework teardown and the flow
//! is what destroys the framework — so **every UI mode switch clears all dialogs**, as in retail.
//!
//! Two rules that a rebuild loses easily:
//!
//! * **Keep the queues.** The server can fire several confirmation requests in one packet burst;
//!   without the per-context queue and the "N waiting" banner the player sees them stacked or loses
//!   all but the last.
//! * **Queue id 1 is special**: those dialogs are all shown at once (the non-queued list).
//!   Queue 2 is the default single-at-a-time queue.

use std::collections::BTreeMap;

use crate::dialog::base::{CancelReason, Dialog, DialogKind};
use crate::props::attr;
use crate::{ElemHandle, PropertyCollection};

/// The default queue id `make_dialog` uses.
pub const DEFAULT_QUEUE: u64 = 2;
/// The queue whose dialogs are all shown at once.
pub const NON_QUEUED: u64 = 1;

/// Dialog properties, the dialog instance, its parent framework and an unsigned-long context.
#[derive(Debug, Clone)]
pub struct DialogInfo {
    pub context: u64,
    pub kind: DialogKind,
    pub data: PropertyCollection,
    /// The live element, once it has been created.
    pub element: Option<ElemHandle>,
    pub dialog: Option<Dialog>,
}

/// The factory's static state.
#[derive(Debug, Default)]
pub struct DialogController {
    /// The global context counter — monotonically increasing.
    next_context: u64,
    /// The currently open dialog per queue id.
    open: BTreeMap<u64, DialogInfo>,
    /// The waiting dialogs per queue id.
    queues: BTreeMap<u64, Vec<DialogInfo>>,
    /// The non-queued list — queue 1, all shown at once.
    non_queued: Vec<DialogInfo>,
    /// Contexts whose completion callback the caller registered. The callback itself belongs to the
    /// caller; this records that one exists so `close_dialog` can report it.
    callbacks: Vec<u64>,
    /// Dialogs closed since the last drain, with their final data. Closing invokes the
    /// callback with the dialog's data, which now carries the answer under property 0x92.
    pub completed: Vec<DialogInfo>,
}

impl DialogController {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Make a dialog.
    ///
    /// Increments the global context, copies `data`, reads property **0x8D** (a bool: replace what
    /// is open on this queue) and **0xC3** (the queue id, default **2**).
    ///
    /// * queue **1** → create immediately onto the all-at-once list;
    /// * nothing open on that queue → create immediately;
    /// * something open and no `0x8D` → `push_back` and refresh the pending banner;
    /// * something open and `0x8D` → cancel the open one, insert this at the head, create it.
    ///
    /// Both properties matter: `0xC3` selects the queue, so it is what makes the `NON_QUEUED`
    /// arm reachable, and `0x8D` is the only way into the replace arm. The readings are on
    /// [`attr::DIALOG_QUEUE_ID`] and [`attr::DIALOG_REPLACE`], each checked against the retail
    /// client.
    ///
    /// `0x8D` means **replace**, not *reuse*: the
    /// key is a one-byte bool, not a dialog pointer, and the arm it selects cancels the open dialog
    /// before inserting this one at the head of the queue.
    pub fn make_dialog(&mut self, data: PropertyCollection, now: f64) -> Option<u64> {
        let kind = DialogKind::from_property(
            data.get_int(attr::DIALOG_KIND)
                .unwrap_or(0)
                .max(0)
                .try_into()
                .unwrap_or(0),
        )
        .or_else(|| DialogKind::from_property(data.get_enum(attr::DIALOG_KIND).unwrap_or(0)))?;
        self.next_context += 1;
        let context = self.next_context;
        let queue = Self::queue_id(&data);
        let replace = Self::replace_flag(&data);
        let info = DialogInfo {
            context,
            kind,
            data,
            element: None,
            dialog: None,
        };

        if queue == NON_QUEUED {
            let mut i = info;
            i.dialog = Some(Dialog::new(i.kind, i.context, i.data.clone(), now));
            self.non_queued.push(i);
            return Some(context);
        }
        if !self.open.contains_key(&queue) {
            self.create(queue, info, now);
        } else if replace {
            if let Some(cur) = self.open.remove(&queue) {
                self.cancel(cur, CancelReason::Replaced);
            }
            self.queues.entry(queue).or_default().insert(0, info);
            self.open_next_dialog(queue, now);
        } else {
            self.queues.entry(queue).or_default().push(info);
        }
        self.update_pending_display();
        Some(context)
    }

    /// Property **0xC3** — the queue id, default [`DEFAULT_QUEUE`].
    ///
    /// It is a queue id, not a bool: reading it as a bool and hard-coding the queue to `2` makes
    /// the `queue == NON_QUEUED` arm **unreachable**, so no caller could ever put a dialog on the
    /// all-at-once list. See [`attr::DIALOG_QUEUE_ID`] for the retail reading.
    ///
    /// The client's property getter is typed, so a value stored under another type is not
    /// converted; both spellings are accepted here because a host that has not been through the
    /// dat's `MasterProperty` table has no way to know which one to write.
    fn queue_id(data: &PropertyCollection) -> u64 {
        data.get_enum(attr::DIALOG_QUEUE_ID)
            .or_else(|| {
                data.get_int(attr::DIALOG_QUEUE_ID)
                    .and_then(|v| u32::try_from(v).ok())
            })
            .map_or(DEFAULT_QUEUE, u64::from)
    }

    /// Property **0x8D** — replace whatever is open on this queue: cancel it, put this at the head
    /// and create it. [`attr::DIALOG_REPLACE`] carries the retail reading; without it the arm it
    /// selects has no way to run.
    fn replace_flag(data: &PropertyCollection) -> bool {
        data.get_bool(attr::DIALOG_REPLACE).unwrap_or(false)
            || data.get_int(attr::DIALOG_REPLACE).unwrap_or(0) != 0
    }

    /// Make a callback dialog in the current UI — as above plus a completion callback. The
    /// callback itself is the caller's; this records that the context wants one.
    pub fn note_callback(&mut self, context: u64) {
        if !self.callbacks.contains(&context) {
            self.callbacks.push(context);
        }
    }

    #[must_use]
    pub fn has_callback(&self, context: u64) -> bool {
        self.callbacks.contains(&context)
    }

    /// The dialog-done step removes the callback after its one invocation. The host owns the
    /// actual callback and supplies the live element's harvested answer before this step.
    pub fn take_callback(&mut self, context: u64) -> bool {
        let Some(index) = self.callbacks.iter().position(|c| *c == context) else {
            return false;
        };
        self.callbacks.remove(index);
        true
    }

    pub fn set_answer_property(&mut self, context: u64, key: u32, value: crate::PropertyValue) {
        if let Some(info) = self
            .open
            .values_mut()
            .chain(self.non_queued.iter_mut())
            .find(|i| i.context == context)
        {
            info.data.set(key, value.clone());
            if let Some(dialog) = &mut info.dialog {
                dialog.data.set(key, value);
            }
        }
    }

    /// Create the dialog — reads property 0x8E for the kind, creates the element, then
    /// updates its size and position, refreshes the pending-dialog display, brings it to front, and
    /// finally raises the open-dialog notice with `context`.
    ///
    /// The element creation itself needs a layout and is the caller's step; [`Self::pending_create`]
    /// reports what to build.
    fn create(&mut self, queue: u64, mut info: DialogInfo, now: f64) {
        info.dialog = Some(Dialog::new(info.kind, info.context, info.data.clone(), now));
        self.open.insert(queue, info);
    }

    /// What the host still has to instantiate: the open dialogs with no element yet.
    #[must_use]
    pub fn pending_create(&self) -> Vec<(u64, DialogKind)> {
        self.open
            .values()
            .chain(self.non_queued.iter())
            .filter(|i| i.element.is_none())
            .map(|i| (i.context, i.kind))
            .collect()
    }

    /// Record the element the host created for a context. `false` = no open dialog wanted one.
    ///
    /// [`crate::UiSystem::bind_dialog_element`] is the caller: it adds the creation's tail
    /// (bring to front and the open-dialog notice), which needs the element manager and so cannot
    /// live here.
    pub fn bind_element(&mut self, context: u64, h: ElemHandle) -> bool {
        let mut bound = false;
        for i in self.open.values_mut().chain(self.non_queued.iter_mut()) {
            if i.context == context {
                i.element = Some(h);
                bound = true;
            }
        }
        bound
    }

    /// Is a dialog open: `0` = any queue, `1` = the non-queued list, otherwise a specific
    /// queue.
    #[must_use]
    pub fn is_dialog_open(&self, queue: u64) -> bool {
        match queue {
            0 => !self.open.is_empty() || !self.non_queued.is_empty(),
            NON_QUEUED => !self.non_queued.is_empty(),
            q => self.open.contains_key(&q),
        }
    }

    /// The dialog currently open on a queue.
    ///
    /// **Not queue [`NON_QUEUED`].** Those are all shown at once, so they live in a list rather
    /// than in the one-per-queue map; [`Self::non_queued`] is their accessor.
    #[must_use]
    pub fn open_on(&self, queue: u64) -> Option<&DialogInfo> {
        self.open.get(&queue)
    }

    /// The non-queued list — every dialog on the **all-at-once** list, queue [`NON_QUEUED`].
    ///
    /// [`Self::open_on`] structurally cannot answer for queue 1, the `NON_QUEUED` arm of
    /// [`Self::make_dialog`]. `0x0004 Communication_PopUpString` is a caller that uses it —
    /// the communication system's pop-up-string handler sets `0xC3 = 1` — so "the pop-up is on
    /// screen" needs this to be readable from outside this struct.
    #[must_use]
    pub fn non_queued(&self) -> &[DialogInfo] {
        &self.non_queued
    }

    /// The **open** dialog carrying a context, on any queue or on the all-at-once list.
    ///
    /// A context that is still waiting in a queue is deliberately not found here: it has no
    /// `Dialog` yet, and a caller servicing [`Self::pending_create`] must not build an element for
    /// something that has not been created yet.
    #[must_use]
    pub fn info(&self, context: u64) -> Option<&DialogInfo> {
        self.open
            .values()
            .chain(self.non_queued.iter())
            .find(|i| i.context == context)
    }

    /// How many are waiting behind the open one — the number the child-0x33 banner shows.
    #[must_use]
    pub fn waiting_on(&self, queue: u64) -> usize {
        self.queues.get(&queue).map_or(0, Vec::len)
    }

    /// Refresh the pending-dialog banner.
    fn update_pending_display(&mut self) {
        let counts: Vec<(u64, usize)> = self
            .open
            .keys()
            .map(|q| (*q, self.queues.get(q).map_or(0, Vec::len)))
            .collect();
        for (q, n) in counts {
            if let Some(i) = self.open.get_mut(&q) {
                if let Some(d) = i.dialog.as_mut() {
                    d.pending_behind = n;
                }
            }
        }
    }

    /// Queue-only wrapper for older screen hosts, whose callback/element work is separate.
    /// Callback-owning hosts use begin/finish to preserve retail's order: current-info removal,
    /// the dialog-done callback -> close notice -> deletion, then opening the next dialog.
    ///
    /// Returns the element to delete, if there was one.
    pub fn close_dialog(&mut self, context: u64, now: f64) -> Option<ElemHandle> {
        let (queue, info) = self.begin_close_dialog(context)?;
        let element = info.element;
        self.finish_close_dialog(queue, info, now);
        element
    }

    /// Closing removes current-info BEFORE the dialog-done step. A callback-owning host retains
    /// the returned data/element, completes the dialog-done step, then opens the next entry.
    /// `None` queue identifies the non-queued list; no queued successor may open at this point.
    pub fn begin_close_dialog(&mut self, context: u64) -> Option<(Option<u64>, DialogInfo)> {
        if let Some(i) = self.non_queued.iter().position(|i| i.context == context) {
            let info = self.non_queued.remove(i);
            return Some((None, info));
        }
        let queue = *self.open.iter().find(|(_, i)| i.context == context)?.0;
        let info = self.open.remove(&queue)?;
        Some((Some(queue), info))
    }

    /// The open-next-dialog tail after dialog-done; old screen callers retain their wrapper.
    pub fn finish_close_dialog(&mut self, queue: Option<u64>, info: DialogInfo, now: f64) {
        self.completed.push(info);
        if let Some(queue) = queue {
            self.open_next_dialog(queue, now);
            self.update_pending_display();
        }
    }

    /// Open the next dialog — pop the queue head and create it.
    pub fn open_next_dialog(&mut self, queue: u64, now: f64) {
        let next = self.queues.get_mut(&queue).and_then(|q| {
            if q.is_empty() {
                None
            } else {
                Some(q.remove(0))
            }
        });
        if let Some(n) = next {
            self.create(queue, n, now);
        }
    }

    /// Dialog done — remove a queued, not yet shown, dialog.
    pub fn dialog_done(&mut self, context: u64) {
        for q in self.queues.values_mut() {
            q.retain(|i| i.context != context);
        }
    }

    /// The tick every open dialog gets on global message 3: returns the contexts that timed out.
    #[must_use]
    pub fn tick(&self, now: f64) -> Vec<u64> {
        self.open
            .values()
            .chain(self.non_queued.iter())
            .filter(|i| i.dialog.as_ref().and_then(|d| d.tick(now)).is_some())
            .map(|i| i.context)
            .collect()
    }

    fn cancel(&mut self, info: DialogInfo, _reason: CancelReason) {
        self.completed.push(info);
    }

    /// Reset — cancels and destroys everything. Called from the UI framework's
    /// destructor, so **every UI mode switch clears all dialogs**.
    ///
    /// Returns the elements to delete.
    pub fn reset(&mut self) -> Vec<ElemHandle> {
        let mut elements = Vec::new();
        for i in std::mem::take(&mut self.open).into_values() {
            elements.extend(i.element);
            self.completed.push(i);
        }
        for i in std::mem::take(&mut self.non_queued) {
            elements.extend(i.element);
            self.completed.push(i);
        }
        for q in std::mem::take(&mut self.queues).into_values() {
            for i in q {
                elements.extend(i.element);
                self.completed.push(i);
            }
        }
        self.callbacks.clear();
        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PropertyValue;

    #[test]
    fn callback_close_unlinks_current_before_callback_and_opens_next_after_done() {
        let mut factory = DialogController::new();
        let first = factory.make_dialog(data(1), 0.0).unwrap();
        let second = factory.make_dialog(data(1), 0.0).unwrap();
        factory.note_callback(first);
        factory.set_answer_property(first, 0x92, PropertyValue::Bool(true));
        let (queue, info) = factory.begin_close_dialog(first).unwrap();
        assert_eq!(info.data.get_bool(0x92), Some(true));
        assert!(
            factory.info(first).is_none(),
            "close unlinks current-info before dialog-done"
        );
        assert!(
            factory.has_callback(first),
            "dialog-done invokes before removing callback registration"
        );
        assert!(factory.open_on(2).is_none());
        assert!(
            factory.info(second).is_none(),
            "successor cannot be visible during callback"
        );
        assert_eq!(factory.waiting_on(2), 1);
        assert!(factory.pending_create().is_empty());
        assert!(factory.take_callback(first));
        assert!(
            !factory.take_callback(first),
            "registration removed exactly once"
        );
        factory.finish_close_dialog(queue, info, 1.0);
        assert_eq!(factory.open_on(2).unwrap().context, second);
        assert_eq!(factory.completed[0].data.get_bool(0x92), Some(true));
    }

    fn data(kind: u32) -> PropertyCollection {
        let mut p = PropertyCollection::new();
        p.set(
            attr::DIALOG_KIND,
            PropertyValue::Integer(i32::try_from(kind).unwrap()),
        );
        p
    }

    /// Oracle: if nothing is open on the queue, create the dialog immediately; if something
    /// is open and this is not a jump, append it to the queue. Queue three dialogs and assert
    /// their order and that only the front one is modal.
    #[test]
    fn three_queued_dialogs_appear_one_at_a_time_in_order() {
        let mut f = DialogController::new();
        let mut modal = data(1);
        modal.set(attr::DIALOG_MODAL, PropertyValue::Bool(true));
        let a = f.make_dialog(modal.clone(), 0.0).unwrap();
        let b = f.make_dialog(modal.clone(), 0.0).unwrap();
        let c = f.make_dialog(modal, 0.0).unwrap();
        assert_eq!(
            (a, b, c),
            (1, 2, 3),
            "contexts come from the one global context counter"
        );
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().context, a);
        assert_eq!(f.waiting_on(DEFAULT_QUEUE), 2);
        assert_eq!(
            f.open_on(DEFAULT_QUEUE)
                .unwrap()
                .dialog
                .as_ref()
                .unwrap()
                .pending_behind,
            2
        );
        assert!(f.is_dialog_open(0));
        assert!(f.is_dialog_open(DEFAULT_QUEUE));
        assert!(!f.is_dialog_open(NON_QUEUED));

        f.close_dialog(a, 0.0);
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().context, b);
        assert_eq!(f.waiting_on(DEFAULT_QUEUE), 1);
        f.close_dialog(b, 0.0);
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().context, c);
        f.close_dialog(c, 0.0);
        assert!(!f.is_dialog_open(0));
        assert_eq!(f.completed.len(), 3);
    }

    /// A replace dialog cancels the open one and takes its place.
    #[test]
    fn a_replace_dialog_cancels_the_open_one_and_takes_its_place() {
        let mut f = DialogController::new();
        let a = f.make_dialog(data(1), 0.0).unwrap();
        let _b = f.make_dialog(data(3), 0.0).unwrap();
        let mut replace = data(2);
        replace.set(attr::DIALOG_REPLACE, PropertyValue::Bool(true));
        let c = f.make_dialog(replace, 0.0).unwrap();
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().context, c);
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().kind, DialogKind::Wait);
        assert!(
            f.completed.iter().any(|i| i.context == a),
            "the open one was cancelled"
        );
        // The one that was merely waiting is still waiting, behind the replacement.
        assert_eq!(f.waiting_on(DEFAULT_QUEUE), 1);
    }

    /// A queue id of one goes on the all at once list.
    #[test]
    fn a_queue_id_of_one_goes_on_the_all_at_once_list() {
        let mut f = DialogController::new();
        let a = f.make_dialog(data(1), 0.0).unwrap();
        let mut non_queued = data(3);
        non_queued.set(attr::DIALOG_QUEUE_ID, PropertyValue::Integer(1));
        let b = f.make_dialog(non_queued.clone(), 0.0).unwrap();
        let c = f.make_dialog(non_queued, 0.0).unwrap();
        // Both are open at once, and neither disturbed queue 2's own open dialog.
        assert!(f.is_dialog_open(NON_QUEUED));
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().context, a);
        assert_eq!(
            f.waiting_on(DEFAULT_QUEUE),
            0,
            "nothing queued behind: they did not go there"
        );
        assert_eq!(f.pending_create().len(), 3, "all three want an element");
        f.close_dialog(b, 0.0);
        f.close_dialog(c, 0.0);
        assert!(!f.is_dialog_open(NON_QUEUED));
        assert!(f.is_dialog_open(DEFAULT_QUEUE));
    }

    /// A queue id of three is a third queue and not a flag.
    #[test]
    fn a_queue_id_of_three_is_a_third_queue_and_not_a_flag() {
        let mut f = DialogController::new();
        let a = f.make_dialog(data(1), 0.0).unwrap();
        let mut third = data(3);
        third.set(attr::DIALOG_QUEUE_ID, PropertyValue::Integer(3));
        let b = f.make_dialog(third.clone(), 0.0).unwrap();
        let _c = f.make_dialog(third, 0.0).unwrap();
        assert_eq!(f.open_on(DEFAULT_QUEUE).unwrap().context, a);
        assert_eq!(f.open_on(3).unwrap().context, b);
        assert_eq!(f.waiting_on(DEFAULT_QUEUE), 0);
        assert_eq!(
            f.waiting_on(3),
            1,
            "the second one queued behind its own queue's open dialog"
        );
        assert!(f.is_dialog_open(0), "0 means any queue");
        assert!(f.is_dialog_open(3));
    }

    /// Oracle: the factory's reset, called from the UI framework's destructor —
    /// "every UI mode switch clears all dialogs".
    #[test]
    fn reset_destroys_open_and_queued_alike() {
        let mut f = DialogController::new();
        f.make_dialog(data(1), 0.0);
        f.make_dialog(data(1), 0.0);
        f.make_dialog(data(1), 0.0);
        f.bind_element(1, ElemHandle::for_test(9));
        let elements = f.reset();
        assert_eq!(elements, vec![ElemHandle::for_test(9)]);
        assert!(!f.is_dialog_open(0));
        assert_eq!(f.waiting_on(DEFAULT_QUEUE), 0);
        assert_eq!(f.completed.len(), 3);
    }

    /// Pinned behavior: the per-frame expiry check.
    #[test]
    fn a_dialog_with_a_timeout_reports_itself_on_the_tick() {
        let mut f = DialogController::new();
        let mut d = data(3);
        d.set(attr::DIALOG_TIMEOUT, PropertyValue::Float(5.0));
        let c = f.make_dialog(d, 100.0).unwrap();
        assert!(f.tick(104.0).is_empty());
        assert_eq!(f.tick(106.0), vec![c]);
    }
}
