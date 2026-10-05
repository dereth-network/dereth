//! Message registration and ordered delivery.

use crate::{
    element, msg, props, widgets, Delivery, ElemCtx, ElemHandle, ElementId, ElementMessage,
    ElementMessageListenResult, ElementNode, ListenerId, MessageId, NoticeId, NoticePayload,
    UiSystem,
};

impl UiSystem {
    // ---- messages ---------------------------------------------------------------------------

    /// The listener's register-for-element-message.
    ///
    /// Registering for 0x19 / 0x1C / 0x1D / 0x40 also walks the element list and calls
    /// [`Self::set_mouse_visible`]`(true)` on every live element with that id, so an element becomes
    /// hit-testable retroactively.
    /// The element listener table's entry for `(element_id, message_id)` — who is registered, in registration
    /// order. [`Self::broadcast_element_message`] consults exactly this list, **before** the source's own
    /// ancestors. Empty for a pair nobody registered.
    ///
    /// The registrations are keyed on an `ElementId`, which is shared across every layout in the
    /// build, so one left behind by a destroyed screen answers a later screen's dialog. That is
    /// what this exists to let a caller check.
    #[must_use]
    pub fn element_message_listeners(
        &self,
        element: ElementId,
        message: MessageId,
    ) -> &[msg::element::ListenerReg] {
        self.element_listeners
            .get(&(element, message))
            .map_or(&[], Vec::as_slice)
    }

    pub fn register_for_element_message(
        &mut self,
        element: ElementId,
        message: MessageId,
        who: ListenerId,
    ) {
        msg::element::register(
            self.element_listeners
                .entry((element, message))
                .or_default(),
            who,
        );
        if msg::element::id::AUTO_MOUSE_VISIBLE.contains(&message) {
            for h in self.element_list.clone() {
                if self.node(h).is_some_and(|n| n.element_id() == element) {
                    self.set_mouse_visible(h, true);
                }
            }
        }
    }

    /// Behavior: **deferred** while a broadcast is in progress.
    pub fn unregister_for_element_message(
        &mut self,
        element: ElementId,
        message: MessageId,
        who: ListenerId,
    ) {
        if self.broadcasting {
            self.removals.push(msg::element::Removal::ElementMessage {
                element,
                message,
                who,
            });
            return;
        }
        if let Some(v) = self.element_listeners.get_mut(&(element, message)) {
            msg::element::unregister(v, who);
        }
    }

    /// The listener's register-for-global-message.
    pub fn register_for_global_message(&mut self, id: MessageId, who: ListenerId) {
        msg::element::register(self.global_listeners.entry(id).or_default(), who);
    }

    /// Unregister for one global message — deferred while broadcasting.
    pub fn unregister_for_global_message(&mut self, id: MessageId, who: ListenerId) {
        if self.broadcasting {
            self.removals
                .push(msg::element::Removal::GlobalMessage { message: id, who });
            return;
        }
        if let Some(v) = self.global_listeners.get_mut(&id) {
            msg::element::unregister(v, who);
        }
    }

    /// Unregister for all messages — deferred while broadcasting.
    pub fn unregister_for_all_messages(&mut self, who: ListenerId) {
        if self.broadcasting {
            self.removals
                .push(msg::element::Removal::AllMessages { who });
            return;
        }
        for v in self.element_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        for v in self.global_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        self.notices.unregister_all(who);
        for h in self.element_list.clone() {
            if let Some(n) = self.node_mut(h) {
                n.listeners.retain(|l| *l != who);
            }
        }
    }

    /// Behavior: register on the element *pointer*, the
    /// second of the two independent mechanisms. This is what the main UI framework uses on its root
    /// elements and what every game UI panel uses on its children.
    pub fn register_for_element_messages(&mut self, h: ElemHandle, who: ListenerId) {
        if let Some(n) = self.node_mut(h) {
            if !n.listeners.contains(&who) {
                n.listeners.push(who);
            }
        }
    }

    pub fn unregister_from_element(&mut self, h: ElemHandle, who: ListenerId) {
        if self.broadcasting {
            self.removals
                .push(msg::element::Removal::FromElement { element: h, who });
            return;
        }
        if let Some(n) = self.node_mut(h) {
            n.listeners.retain(|l| *l != who);
        }
    }

    /// The removal-data pass, run at the top of [`Self::use_time`].
    ///
    /// This is the only protection against a listener destroying itself inside its own callback.
    pub fn process_removal_data(&mut self) {
        let pending = std::mem::take(&mut self.removals);
        for r in pending {
            match r {
                msg::element::Removal::ElementMessage {
                    element,
                    message,
                    who,
                } => {
                    if let Some(v) = self.element_listeners.get_mut(&(element, message)) {
                        msg::element::unregister(v, who);
                    }
                }
                msg::element::Removal::GlobalMessage { message, who } => {
                    if let Some(v) = self.global_listeners.get_mut(&message) {
                        msg::element::unregister(v, who);
                    }
                }
                msg::element::Removal::AllMessages { who } => {
                    self.unregister_for_all_messages(who);
                }
                msg::element::Removal::FromElement { element, who } => {
                    if let Some(n) = self.node_mut(element) {
                        n.listeners.retain(|l| *l != who);
                    }
                }
            }
        }
    }

    /// Who is registered for one global message right now — the global-message listener table's entry for `id`.
    ///
    /// **It makes an *unregistration* falsifiable.** Every listener in this
    /// crate registers for message 3 only while it has work and drops out when it is idle
    /// (`msg::global::TICK`'s own documentation says so of seven element types), and that half is
    /// invisible from behaviour: clears
    /// state bit `0x400000` *and* unregisters, so deleting the unregistration changes nothing
    /// a test of the position could see. Deleting it is still a defect — a screen full of
    /// finished scrollbars would tick for ever — and this is the only way to say so.
    #[must_use]
    pub fn global_message_listeners(&self, id: MessageId) -> Vec<ListenerId> {
        self.global_listeners
            .get(&id)
            .map(|v| v.iter().map(|r| r.who).collect())
            .unwrap_or_default()
    }

    /// Behavior: flat, no bubbling, no serial filter.
    pub fn broadcast_global(&mut self, id: MessageId, param: u32) {
        let listeners: Vec<ListenerId> = self
            .global_listeners
            .get(&id)
            .map(|v| v.iter().map(|r| r.who).collect())
            .unwrap_or_default();
        if listeners.is_empty() {
            return;
        }
        let was = self.broadcasting;
        self.broadcasting = true;
        for who in listeners {
            match who {
                ListenerId::Element(h) => {
                    let Some(mut b) = self.take_behaviour(h) else {
                        continue;
                    };
                    let mut ctx = ElemCtx { ui: self, me: h };
                    b.listen_to_global_message(&mut ctx, id, param);
                    self.put_behaviour(h, b);
                }
                ListenerId::External(_) => {
                    self.outbox.push(Delivery::Global { to: who, id, param });
                }
            }
        }
        self.broadcasting = was;
    }

    /// Latest element-message serial. A screen adapter may retain a synchronous retail
    /// operation's ending serial while its External deliveries wait in the outbox.
    #[must_use]
    pub fn element_message_serial(&self) -> u32 {
        self.serial
    }

    /// The element's broadcast, which forwards to the manager's own
    /// broadcast.
    ///
    /// Synchronous, inside the caller's stack frame. The table listeners run first, then the source
    /// element's own listeners, then its parent's, then its grandparent's — and the serial number
    /// stops a listener registered on several elements in the chain from seeing it twice.
    pub fn broadcast_element_message(&mut self, src: ElemHandle, id: MessageId, p1: u32, p2: u32) {
        self.broadcast_element_message_at(src, id, p1, p2, msg::MessagePoint::default());
    }

    /// The same with the mouse points filled in, as the mouse paths do.
    pub fn broadcast_element_message_at(
        &mut self,
        src: ElemHandle,
        id: MessageId,
        p1: u32,
        p2: u32,
        point: msg::MessagePoint,
    ) {
        let Some(source_id) = self.node(src).map(ElementNode::element_id) else {
            return;
        };
        self.serial = self.serial.wrapping_add(1);
        let m = ElementMessage {
            source_id,
            source: src,
            id,
            p1,
            p2,
            point,
            serial: self.serial,
        };
        let was = self.broadcasting;
        self.broadcasting = true;

        // (a) registrations by element id.
        let by_id: Vec<ListenerId> = self
            .element_listeners
            .get(&(source_id, id))
            .map(|v| v.iter().map(|r| r.who).collect())
            .unwrap_or_default();
        for who in by_id {
            // **The last-listened serial number belongs to the *listener*, not to the
            // route.** The listener's last-listened serial `<` the message's serial check is stamped
            // wherever a listener is reached, including here: a listener that has
            // registered **both** by element id here *and* on an ancestor (which
            // `register_for_element_messages(root, ME)` is, for every screen in this build)
            // would otherwise receive one broadcast **twice**.
            //
            // A dialog answer control — `GamePlayScreen`'s logout `Yes`/`No`, and the accept/cancel
            // pairs `CharGenScreen` and `CharacterManagementScreen` register — hides the problem,
            // because `DialogElement::listen_to_element_message` answers stop-processing for
            // exactly those ids, so the bubble never reaches the root. The vitals windows register
            // by id *and* let the message keep bubbling: without this stamp one press toggles the
            // bar twice, which on screen is a bar that does not toggle at all.
            if !self.claim_serial(who, m.serial) {
                continue;
            }
            if self.deliver_element(who, &m) == ElementMessageListenResult::StopProcessing {
                self.broadcasting = was;
                return;
            }
        }

        // (b) the element itself, then the bubble up the parent chain.
        self.dispatch_and_forward(src, &m);
        self.broadcasting = was;
        // Once the outermost broadcast has unwound, send whatever a handler asked to raise
        // *after* it — see [`Self::queue_element_message`].
        if !was {
            loop {
                if !self.queued_visibility.is_empty() {
                    let (h, v) = self.queued_visibility.remove(0);
                    self.set_visible(h, v);
                    continue;
                }
                let Some((h, id, p1, p2)) = self.queued_messages.pop() else {
                    break;
                };
                self.broadcast_element_message(h, id, p1, p2);
            }
        }
    }

    /// Raise an element message **after** the broadcast currently in flight has finished, rather
    /// than inside it.
    ///
    /// **This is required, not a convenience.** Serial numbers stop a
    /// listener registered on both a child and its parent from hearing one message twice. A nested
    /// broadcast would allocate a larger serial and stamp it on the ancestors, causing the outer
    /// message to be rejected when it resumes. The client avoids that by queuing nested element
    /// messages and draining them FIFO after the current dispatch.
    ///
    /// The list-box selected-item setter and button click path both depend on this ordering. A
    /// button release produces release, click, and button-click messages in sequence; dispatching
    /// button-click from inside click would prevent ancestors from receiving the outer click.
    /// The radar padlock listens for that ancestor click, so its button would otherwise do nothing.
    pub fn queue_element_message(&mut self, h: ElemHandle, id: MessageId, p1: u32, p2: u32) {
        if self.broadcasting {
            self.queued_messages.insert(0, (h, id, p1, p2));
        } else {
            self.broadcast_element_message(h, id, p1, p2);
        }
    }

    /// [`Self::set_visible`], deferred until the broadcast in flight has unwound — the same fix as
    /// [`Self::queue_element_message`], for a handler that changes an element's *visibility*
    /// rather than raising a message.
    ///
    /// **The same serial-number problem, for visibility.** Setting visibility raises
    /// `0x18` (the element's set-visible broadcasts it),
    /// so calling it from inside a `0x18` handler allocates a **larger** serial and stamps every
    /// ancestor as it bubbles; when the outer `0x18` resumes, `claim_serial` refuses it at each
    /// one and it dies below whatever was listening. Measured over the shipped tree, that is why
    /// **six of the sixteen `PanelStack` pages — the six of element type `0x00000008`,
    /// `Panel` — never re-broadcast their own visibility**: `Panel`'s own `0x18` arm
    /// puts its open tab page back up, and that inner visibility set out-stamps the message still
    /// bubbling. Six of the seven toolbar panel buttons open one of those six pages, so with this
    /// deferral absent the toolbar cannot follow the panels once button click handling is the only
    /// thing driving them.
    ///
    /// The panel update's trailing `TAB_PAGE_CHANGED` broadcast is a separate site with the same
    /// shape; it is not deferred here, and `dereth-ui-screens` pins its behaviour.
    pub fn queue_set_visible(&mut self, h: ElemHandle, v: bool) {
        if self.broadcasting {
            self.queued_visibility.push((h, v));
        } else {
            self.set_visible(h, v);
        }
    }

    /// The element's message handler followed by its message forwarder.
    pub(crate) fn dispatch_and_forward(
        &mut self,
        at: ElemHandle,
        m: &ElementMessage,
    ) -> ElementMessageListenResult {
        let mut result = self.deliver_element(ListenerId::Element(at), m);
        if result == ElementMessageListenResult::StopProcessing {
            return result;
        }
        // The element's own listener table, filtered by serial number.
        let listeners = self
            .node(at)
            .map(|n| n.listeners.clone())
            .unwrap_or_default();
        for who in listeners {
            if !self.claim_serial(who, m.serial) {
                continue;
            }
            let r = self.deliver_element(who, m);
            if r == ElementMessageListenResult::StopProcessing {
                return r;
            }
            if r != ElementMessageListenResult::Default {
                result = r;
            }
        }
        // The parent, once per serial number.
        if let Some(p) = self.parent(at) {
            if self.claim_serial(ListenerId::Element(p), m.serial) {
                let r = self.dispatch_and_forward(p, m);
                if r != ElementMessageListenResult::Default {
                    return r;
                }
            }
        }
        result
    }

    /// The listener's last-listened serial number `<` the message's serial — and stamp it.
    pub(crate) fn claim_serial(&mut self, who: ListenerId, serial: u32) -> bool {
        let e = self.serial_seen.entry(who).or_insert(0);
        if *e < serial {
            *e = serial;
            true
        } else {
            false
        }
    }

    pub(crate) fn deliver_element(
        &mut self,
        who: ListenerId,
        m: &ElementMessage,
    ) -> ElementMessageListenResult {
        match who {
            ListenerId::Element(h) => {
                let Some(mut b) = self.take_behaviour(h) else {
                    return ElementMessageListenResult::Default;
                };
                let mut ctx = ElemCtx { ui: self, me: h };
                let r = b.listen_to_element_message(&mut ctx, m);
                self.put_behaviour(h, b);
                if r == ElementMessageListenResult::Default {
                    self.base_listen_to_element_message(h, m)
                } else {
                    r
                }
            }
            ListenerId::External(_) => {
                self.outbox.push(Delivery::Element {
                    to: who,
                    msg: m.clone(),
                });
                ElementMessageListenResult::Default
            }
        }
    }

    /// The four messages the base handles itself,
    /// and only when the message's source element is itself.
    ///
    /// The state ids are defined in [`element::state`].
    pub(crate) fn base_listen_to_element_message(
        &mut self,
        at: ElemHandle,
        m: &ElementMessage,
    ) -> ElementMessageListenResult {
        if m.source != at {
            return ElementMessageListenResult::Default;
        }
        let Some(n) = self.node(at) else {
            return ElementMessageListenResult::Default;
        };
        let st = n.state;
        use msg::element::id;
        if m.id == id::ACTIVATED {
            if st == element::state::NORMAL || st == element::state::ROLLOVER {
                self.set_state(at, element::state::ACTIVE);
            }
        } else if m.id == id::DEACTIVATED {
            if st == element::state::ACTIVE {
                self.set_state(at, element::state::NORMAL);
            }
        } else if m.id == id::FOCUS_CHANGED {
            let gained = m.p1 != 0;
            if gained
                && (st == element::state::NORMAL
                    || st == element::state::ROLLOVER
                    || st == element::state::ACTIVE)
            {
                self.set_state(at, element::state::FOCUSED);
            } else if !gained && st == element::state::FOCUSED {
                self.set_state(at, element::state::NORMAL);
            }
        } else if m.id == id::VISIBILITY_TOGGLE {
            // Enum `0x58` uses 1 to invert current visibility, 2 to show and 3 to hide.
            //
            // The toggle reads the region's current visible flag. A value that is not 1, 2 or 3
            // falls out and does nothing.
            //
            // **Why it matters, measured rather than argued.** All **41** element descriptions in
            // the 101 shipped layouts that carry `0x57` also carry `0x58`, and every one of the 41
            // carries `0x58 = 1` — so under the opposite reading (*1 show*) a key or a button
            // bound to a panel could open it and never close it.
            let mode = n
                .merged_properties()
                .get_enum(props::attr::VISIBILITY_TOGGLE_MODE);
            let vis = n.region.flags.visible;
            match mode {
                Some(1) => self.set_visible(at, !vis),
                Some(2) => self.set_visible(at, true),
                Some(3) => self.set_visible(at, false),
                _ => {}
            }
        }
        ElementMessageListenResult::Default
    }

    /// Take the deliveries queued for listeners this crate does not own, in dispatch order.
    pub fn drain_outbox(&mut self) -> Vec<Delivery> {
        std::mem::take(&mut self.outbox)
    }

    /// The selected menu index on an element, or **-1** when `h` is not a
    /// menu, has no list box, or has one with nothing selected — the client's own three ways of
    /// answering -1, which it does not distinguish either.
    #[must_use]
    pub fn menu_selected_index(&self, h: ElemHandle) -> i32 {
        widgets::menu::selected_index(self, h)
    }

    /// The client's tail for a context whose element the host has just built: record the element
    /// and raise the dialog-opened notice with that context.
    ///
    /// **This is `bind_element`'s production caller.** Dialog creation is split in
    /// this build because the factory has no asset source and cannot build an element: it records
    /// what it wants (in the dialog factory's pending-create queue), the screen instantiates
    /// the `Dialog` layout, and this closes the loop. It is also the producer of the
    /// dialog-opened notice.
    ///
    /// Returns `false` when no open dialog carries that context.
    pub fn bind_dialog_element(&mut self, context: u64, h: ElemHandle) -> bool {
        if !self.dialogs.bind_element(context, h) {
            return false;
        }
        // Dialog creation also assigns the context on the actual subclass, not only
        // the factory's context->element record. Older screen answer readers stay unchanged.
        if let Some(mut behaviour) = self.take_behaviour(h) {
            if let Some(dialog) = behaviour.as_dialog_mut() {
                dialog.context = context;
            }
            self.put_behaviour(h, behaviour);
        }
        self.bring_to_front(h);
        let payload = NoticePayload {
            a: u32::try_from(context).unwrap_or(u32::MAX),
            ..NoticePayload::default()
        };
        self.send_notice(NoticeId::DialogOpened, &payload);
        true
    }

    /// The client's global event handler's fan-out. An id with no handler is a no-op.
    pub fn send_notice(&mut self, id: NoticeId, payload: &NoticePayload) {
        let to: Vec<ListenerId> = self.notices.handlers(id).to_vec();
        for who in to {
            self.outbox.push(Delivery::Notice {
                to: who,
                id,
                payload: payload.clone(),
            });
        }
    }
}
