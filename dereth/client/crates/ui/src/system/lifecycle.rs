//! Element initialization and destruction.

use crate::{desc, msg, ElemCtx, ElemHandle, ListenerId, PropertyValue, UiSystem};

impl UiSystem {
    // ---- lifecycle -----------------------------------------------------------------------------

    /// Initialize an element in the documented order.
    pub fn initialize(&mut self, h: ElemHandle) {
        let Some(n) = self.node(h) else { return };
        let media: Vec<desc::MediaDesc> = n.desc.base.media.clone();
        let default_state = n.desc.default_state;
        // 1. Reset the media machine from the element description.
        let mut machine = match self.node_mut(h) {
            Some(n) => std::mem::take(&mut n.media),
            None => return,
        };
        machine.reset(&media);
        if let Some(n) = self.node_mut(h) {
            n.media = machine;
        }
        // 2. mark the element initialised
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_initialized(true);
        }
        // 3. Enter the description's default state.
        self.set_state(h, default_state);
        // 4. on_set_attribute for every property in the desc's collection
        let props: Vec<(u32, PropertyValue)> = self
            .node(h)
            .map(|n| n.merged_properties().0.into_iter().collect())
            .unwrap_or_default();
        for (id, v) in props {
            self.on_set_attribute(h, id, Some(&v));
        }
        // 5. mouse-visible = should-be-mouse-visible || the subclass override
        self.update_mouse_visibility(h);
        // 6. if notify-on-create is set, broadcast global message 6 with this element
        if self.node(h).is_some_and(|n| n.flags.notify_on_create()) {
            self.broadcast_global(msg::global::ELEMENT_CREATED, h.0);
        }
        // Run the media machine once so a state with an image shows it immediately.
        let init = true;
        let mut machine = match self.node_mut(h) {
            Some(n) => std::mem::take(&mut n.media),
            None => return,
        };
        let fx = machine.update(0.0, &mut self.rng, init);
        if let Some(n) = self.node_mut(h) {
            n.media = machine;
        }
        for e in fx {
            self.apply_media_effect(h, e);
        }
    }

    /// Initialise a whole subtree, then post-init — which is what a framework does
    /// after [`Self::register_for_element_messages`].
    ///
    /// **The post-init half is essential.** The base post-init walks
    /// the children calling each child's post-init, and a subclass chains to the base first, so
    /// the effective order is children before parent; the button's own post-init then
    /// runs its state update, which is the only thing that ever moves a button out of state 0.
    ///
    /// **The initialise half walks the tree bottom-up.** Retail initialises a
    /// created subtree **bottom-up**, and there is only one routine that ever does it: the
    /// recursive builder initialises each child as it makes it, and whichever of
    /// the root-element create or the child-element create asked for the tree
    /// initialises the subtree root **last**, after every descendant is already attached and
    /// already initialised.
    ///
    /// The recursive builder creates and initializes each child before attaching it. The caller
    /// then initializes the subtree root, attaches that root to its parent, and finally runs the
    /// recursive post-initialization step.
    ///
    /// The root-element create — which is what the **load** path reaches, through
    /// the framework's create-and-add-root-element — has exactly the same shape:
    /// the partial-description recursive create, then the root's initialise, the should-own-object
    /// set, the root-element flag, then its post-init. **There is no second, parent-first order in the client**,
    /// so this one routine serves both paths.
    ///
    /// Element initialization is **not** recursive and no subclass overrides it,
    /// so the only recursion is the one above. Step 3 of it is
    /// setting the description's default state and
    /// step 4 re-dispatches every property through `on_set_attribute`. Both of those reach the
    /// element's **children** — the state set through `pass_to_children`, and `on_set_attribute`
    /// through subclasses that restate a child. The panel's attribute setter reaches its own
    /// update and selection, behavior that every tabbed page in the client depends on. Walking
    /// parent-first, the child's own
    /// default-state set would run *after* the parent had restated it and throw the result
    /// away: every tab caption in the client would come up in its base state rather than
    /// `0x0B`/`0x0C`, and every fresh map note would come up framed.
    pub fn initialize_tree(&mut self, h: ElemHandle) {
        self.initialize_subtree(h);
        self.post_init_tree(h);
    }

    /// The `Initialize` half of [`Self::initialize_tree`]: every descendant in child order,
    /// deepest first, and this element last. See that function for the addresses.
    pub(crate) fn initialize_subtree(&mut self, h: ElemHandle) {
        for c in self.children(h) {
            self.initialize_subtree(c);
        }
        self.initialize(h);
    }

    /// Behavior: children first, then this element's own override.
    pub fn post_init_tree(&mut self, h: ElemHandle) {
        for c in self.children(h) {
            self.post_init_tree(c);
        }
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            b.post_init(&mut ctx);
            self.put_behaviour(h, b);
        }
    }

    /// The element's add-to-delete-queue, which forwards to the manager's.
    /// Elements are **never** deleted inline while the tree is being walked.
    pub fn add_to_delete_queue(&mut self, h: ElemHandle) {
        if let Some(n) = self.node_mut(h) {
            if n.queued_for_delete {
                return;
            }
            n.queued_for_delete = true;
        } else {
            return;
        }
        self.delete_queue.push(h);
    }

    #[must_use]
    pub fn delete_queue_len(&self) -> usize {
        self.delete_queue.len()
    }

    /// Drain the delete queue, at the very top of [`Self::use_time`].
    pub fn clean_delete_queue(&mut self) {
        while let Some(h) = self.delete_queue.pop() {
            self.destroy_now(h);
        }
    }

    /// The framework's remove-root-element, which forwards to the manager's
    /// remove-and-delete-root.
    pub fn remove_and_delete_root(&mut self, h: ElemHandle) {
        self.add_to_delete_queue(h);
        self.clean_delete_queue();
    }

    /// The element's destructor plus the manager's deleting-element hook.
    pub(crate) fn destroy_now(&mut self, h: ElemHandle) {
        // The subclass destructor runs first, as it does in C++: a `Menu` hands its
        // popup back here. See [`element::Element::on_destroy`].
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            b.on_destroy(&mut ctx);
            self.put_behaviour(h, b);
        }
        for c in self.children(h) {
            self.destroy_now(c);
        }
        // Element deletion: clear the element out of every manager pointer.
        let who = ListenerId::Element(h);
        for v in self.element_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        for v in self.global_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        self.notices.unregister_all(who);
        self.serial_seen.remove(&who);
        for v in self.input_action_listeners.values_mut() {
            v.retain(|e| *e != h);
        }
        self.activatable.retain(|e| *e != h);
        for slot in [
            &mut self.mouse.last_over,
            &mut self.mouse.last_entered,
            &mut self.mouse.capture,
            &mut self.drag.last_drag_cursor_over,
            &mut self.drag.element,
            &mut self.drag.owner,
            &mut self.drag.potential,
            &mut self.focus_element,
            &mut self.active_element,
            &mut self.tooltip.owner,
            &mut self.tooltip.element,
        ] {
            if *slot == Some(h) {
                *slot = None;
            }
        }
        if let Some(p) = self.parent(h) {
            if let Some(n) = self.node_mut(p) {
                n.region.remove_child(h);
            }
        }
        self.element_list.retain(|e| *e != h);
        self.delete_queue.retain(|e| *e != h);
        let idx = h.index();
        if let Some(s) = self.slots.get_mut(idx) {
            if s.generation == h.generation() {
                s.node = None;
                s.generation = s.generation.wrapping_add(1);
                if let Ok(i) = u32::try_from(idx) {
                    self.free.push(i);
                }
            }
        }
    }
}
