//! Element state, properties and visibility.

use crate::{desc, msg, props, ElemCtx, ElemHandle, PropertyValue, StateId, UiSystem};

impl UiSystem {
    // ---- state and properties -----------------------------------------------------------------

    /// Move the element to a new state.
    ///
    /// Compute the old merged set and the new merged set, call `on_set_attribute` for every property
    /// that changed, appeared, or **disappeared** (value-less), then reset the media machine when
    /// the new state has media. If the state record says `pass_to_children`, recurse.
    pub fn set_state(&mut self, h: ElemHandle, s: StateId) {
        // The subclass override runs first, because the button can refuse a state outright. It
        // answers a state its own attributes contradict by
        // writing the attribute and returning, and only falls through to the base state setter
        // when they agree. Without it a button whose layout ships `Disabled = true`
        // stays disabled for ever, so the char-gen wizard's *Exit*, *Help* and
        // *Random* would refuse a real click while the retail client answers one.
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            let handled = b.set_state(&mut ctx, s);
            self.put_behaviour(h, b);
            if handled {
                return;
            }
        }
        let Some(n) = self.node(h) else { return };
        // Resolve the requested state description; an element handed a state it does not declare
        // records **state 0**, not the id. It matters for `pass_to_children`: the intro root
        // broadcasts its state to two children and only one of them has it, and the other must end
        // up in state 0 rather than remembering an id it has no description for.
        let s = if n.desc.access_state(s).is_some() {
            s
        } else {
            StateId(0)
        };
        // **Setting a state is idempotent.** After resolving an
        // undeclared id to zero, retail compares it with the current state. Equality returns
        // successfully before storing the state, applying the property diff, or recursing through
        // `pass_to_children`. A re-set of the state an element is already in is not a cheap
        // no-op in retail, it is the **absence** of one.
        //
        // **The char-gen town page shows why.** The town page's
        // set-town lights the chosen map pin and *then* puts the page itself into
        // `0x10000034 + area`. Those four page
        // states carry `pass_to_children`, so the page broadcasts one of them down its subtree;
        // the map plate `0x1000040A` does not declare it, resolves to **0**, is *already* 0 — and
        // retail stops right here. Without this gate the cascade would continue through the plate
        // to the four pins `0x1000040B/D/E/F`, which do not declare it either, so every one of
        // them would be re-stated to 0 and the pin lit one call earlier would go dark again.
        //
        // It is not only the pins. Without the gate every `pass_to_children` broadcast in the
        // shipped layouts would reach the whole subtree instead of stopping at the first
        // descendant that does not declare the state, and every redundant state set would re-run
        // `on_set_attribute` and reset a `MediaPlayback` that retail leaves alone.
        if self.node(h).is_some_and(|n| n.state == s) {
            return;
        }
        let Some(n) = self.node(h) else { return };
        let old = n.merged_properties();
        let new = n.merged_properties_for(s);
        // A state set reads the pass-to-children flag from the selected alternate state when one
        // exists, or from the element's base state when state zero selects no alternate record.
        // The state-zero arm therefore consults the base record rather than answering "no".
        //
        // **The selected-object field needs that arm.** It (`0x1000019E`) ships base
        // `pass_to_children = true` and its alternate state `0x1000000B` carries
        // a 0.25 s pause step then a state step to state 1. State 1 is not authored, so the client
        // records **0** and broadcasts 0 to the children — which is what takes `0x100001A0`'s
        // green plate (`0x06001937`) back to its base media, `INVALID_DID`. Without the base arm
        // the field alone would return to state 0 and the plate would stay green for ever.
        let pass = n
            .desc
            .access_state(s)
            .map_or_else(|| n.desc.base.pass_to_children, |d| d.pass_to_children);
        let media: Vec<desc::MediaDesc> = n.media_for(s).to_vec();
        if let Some(n) = self.node_mut(h) {
            n.state = s;
        }
        for c in props::state_diff(&old, &new) {
            self.on_set_attribute(h, c.id, c.value.as_ref());
        }
        if !media.is_empty() {
            let init = self.node(h).is_some_and(|n| n.flags.is_initialized());
            let mut machine = match self.node_mut(h) {
                Some(n) => std::mem::take(&mut n.media),
                None => return,
            };
            machine.reset(&media);
            let now = self.now.0;
            let fx = machine.update(now, &mut self.rng, init);
            if let Some(n) = self.node_mut(h) {
                n.media = machine;
            }
            for e in fx {
                self.apply_media_effect(h, e);
            }
        }
        if pass {
            for c in self.children(h) {
                self.set_state(c, s);
            }
        }
    }

    /// Whether a drag element exists, meaning something is being dragged right now.
    ///
    /// Drop-target handling tests it before it will show a drop highlight,
    /// which is what makes that highlight a *drag*-over and not a hover.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.element.is_some()
    }

    /// Take what `MasterProperty 0x39000001` gives this system: the id->type table every layout
    /// decode needs and the per-row defaults [`Self::get_attribute_enum`] falls back to.
    pub fn install_master(&mut self, master: &dereth_assets::MasterProperty) {
        self.property_types = master.property_types();
        self.property_defaults = master
            .properties
            .iter()
            .filter_map(|(id, d)| d.default.clone().map(|v| (*id, v)))
            .collect();
    }

    /// The property description's default value for `id`, out of the installed `MasterProperty`.
    #[must_use]
    pub fn master_default(&self, id: u32) -> Option<&PropertyValue> {
        self.property_defaults.get(&id)
    }

    /// The element's enum attribute getter, whole: if the element's property query finds an enum
    /// value it returns true with it; otherwise it builds a property from the `MasterProperty`
    /// description's default and returns false with that enum; failing both, it writes `0` and
    /// returns false.
    ///
    /// The element's merged properties first (instance, current state, description -- what
    /// its property query searches), then the **master default**, then `0`. The `bool` is
    /// the one retail returns; the caller that matters, the list box's selected-item setter,
    /// never tests it (it pushes the out-slot straight
    /// into the state set), so both come back and that caller takes `.1`.
    ///
    /// The master default matters: reading only the merged properties and skipping the state
    /// set when the list box names no state would leave the eleven shipped lists that
    /// enable `0x61` without naming the pair -- Friends, Squelch, Titles, Page List among them --
    /// never entering their row templates' authored state 6.
    #[must_use]
    pub fn get_attribute_enum(&self, h: ElemHandle, id: u32) -> (bool, u32) {
        if let Some(v) = self
            .node(h)
            .and_then(|n| n.merged_properties().get_enum(id))
        {
            return (true, v);
        }
        match self.master_default(id) {
            Some(PropertyValue::Enum(v)) => (false, *v),
            _ => (false, 0),
        }
    }

    /// Behavior: write into the instance properties, then call `on_set_attribute`.
    /// Every typed setter follows this order.
    pub fn set_attribute_bool(&mut self, h: ElemHandle, id: u32, v: bool) {
        let value = PropertyValue::Bool(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// Behavior: the same shape as
    /// [`UiSystem::set_attribute_bool`]. The scrollbar's own position
    /// (attribute 0x86), which is the only coordinate `Scrollbar` has, is written through it.
    pub fn set_attribute_float(&mut self, h: ElemHandle, id: u32, v: f32) {
        let value = PropertyValue::Float(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// The element's int attribute setter.
    pub fn set_attribute_int(&mut self, h: ElemHandle, id: u32, v: i32) {
        let value = PropertyValue::Integer(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// Set an element's enum attribute.
    ///
    /// **Not interchangeable with [`Self::set_attribute_int`]**:
    /// `base_on_set_attribute`'s `0x57` arm matches `PropertyValue::Enum` and nothing
    /// else, so writing an input action through the `Int` setter registers no listener and reports
    /// success. The shipped layouts author these properties as enums, which is why the live tree
    /// registers 40 elements and a hand-written `set_attribute_int` registers none.
    pub fn set_attribute_enum(&mut self, h: ElemHandle, id: u32, v: u32) {
        let value = PropertyValue::Enum(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// Behavior: the base id table, then the subclass override.
    ///
    /// Order matters: the subclass handles its own ids and **chains to this one**, so the base runs
    /// second here.
    pub fn on_set_attribute(&mut self, h: ElemHandle, id: u32, v: Option<&PropertyValue>) {
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            b.on_set_attribute(&mut ctx, id, v);
            self.put_behaviour(h, b);
        }
        self.base_on_set_attribute(h, id, v);
    }

    pub(crate) fn base_on_set_attribute(
        &mut self,
        h: ElemHandle,
        id: u32,
        v: Option<&PropertyValue>,
    ) {
        use props::attr;
        let b = |v: Option<&PropertyValue>| matches!(v, Some(PropertyValue::Bool(true)));
        let i = |v: Option<&PropertyValue>| match v {
            Some(PropertyValue::Integer(x)) => *x,
            _ => 0,
        };
        let f = |v: Option<&PropertyValue>| match v {
            Some(PropertyValue::Float(x)) => *x,
            _ => 0.0,
        };
        let e = |v: Option<&PropertyValue>| match v {
            Some(PropertyValue::Enum(x)) => Some(*x),
            _ => None,
        };
        match id {
            // **`0x3B` is `UICore_Element_hide`, so the sense is inverted** — see
            // [`props::attr::HIDE`] for the four independent facts that fix it.
            // A value-less change (the property disappeared from the new state) lands here with
            // `v == None`; `b(None)` is false, so the element goes back to visible, which is the
            // constructor default and what the row's absent `MasterProperty` default implies.
            attr::HIDE => self.set_visible(h, !b(v)),
            attr::BLOCK_CLICKS => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.block_clicks = b(v);
                }
                self.update_mouse_visibility(h);
            }
            attr::TOOLTIP_ON => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.tooltip = b(v);
                }
            }
            attr::ALPHA_BLEND_MOD => {
                if let Some(n) = self.node_mut(h) {
                    n.region.alpha_blend_mod = f(v);
                }
            }
            attr::ERASE_BACKGROUND => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.erase_background = b(v);
                }
            }
            attr::DRAW_AFTER_CHILDREN => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.draw_after_children = b(v);
                }
            }
            attr::TILING_OFFSET_X => {
                if let Some(n) = self.node_mut(h) {
                    n.region.tiling_offset.0 = i(v);
                }
            }
            attr::TILING_OFFSET_Y => {
                if let Some(n) = self.node_mut(h) {
                    n.region.tiling_offset.1 = i(v);
                }
            }
            attr::TILING_OFFSET_BOTH => {
                // "set both tiling offsets (re-dispatches 0x55 and 0x54)".
                self.base_on_set_attribute(h, attr::TILING_OFFSET_Y, v);
                self.base_on_set_attribute(h, attr::TILING_OFFSET_X, v);
            }
            attr::INPUT_MAP => {
                if let Some(n) = self.node_mut(h) {
                    n.input_map = e(v);
                }
            }
            attr::INPUT_ACTION => {
                if let Some(a) = e(v) {
                    // Registration refuses action 0 and a null element, and appends uniquely, so
                    // re-setting `0x57`
                    // on an element already in the bucket does not put it in twice. A bare `push`
                    // would deliver `0x31` once per registration.
                    if a != 0 {
                        let bucket = self.input_action_listeners.entry(a).or_default();
                        if !bucket.contains(&h) {
                            bucket.push(h);
                        }
                    }
                }
            }
            attr::DROP_CATCHER => {
                if let Some(n) = self.node_mut(h) {
                    n.drop_catcher = b(v);
                }
            }
            attr::CONTEXT_MENU => {
                if let Some(n) = self.node_mut(h) {
                    n.flags.set_context_menu(b(v));
                }
                self.update_mouse_visibility(h);
            }
            attr::UI_OBJECT_MODE => {
                // `0xCD` is an enum, not a bool. The element's attribute setter
                // reads it into a local initialised to `3`, passes
                // `value != 0` to the should-own-object setter and then
                // updates for the parent size. The bool is kept because it is a
                // real field — flag word bit 14 — but it is *derived* from the mode
                // rather than being the whole of what `0xCD` says.
                //
                // An absent or non-enum value is mode 3, not mode 0: that is the initial value in
                // the setter, the UI-object construction's fall-back and
                // the current-mode walk's caller-supplied default. A value outside `0..=3` is
                // left at 3 for the same reason — retail's readers compare against 1, 2 and 3 and
                // treat anything else as the element-size arm.
                let mode = e(v)
                    .and_then(props::UiObjectMode::from_value)
                    .unwrap_or(props::UiObjectMode::ElementSize);
                if let Some(n) = self.node_mut(h) {
                    n.flags.set_should_own_object(mode.owns_object());
                    n.region.object_mode = mode;
                }
                self.update_for_parent_size_change(h);
            }
            _ => {
                if let Some(n) = self.node_mut(h) {
                    match id {
                        attr::ACTIVATABLE => n.flags.set_activatable(b(v)),
                        attr::ACTIVATE_ON_SHOW => n.flags.set_activate_on_show(b(v)),
                        attr::DRAGABLE => n.flags.set_dragable(b(v)),
                        attr::NOTIFY_ON_RESIZE => n.flags.set_notify_on_resize(b(v)),
                        attr::NOTIFY_ON_MOVE => n.flags.set_notify_on_move(b(v)),
                        attr::NOTIFY_ON_CREATE => n.flags.set_notify_on_create(b(v)),
                        attr::RESIZE_LINE => n.flags.set_resize_line(b(v)),
                        attr::SAVE_LOCATION => n.flags.set_save_location(b(v)),
                        attr::SAVE_SIZE => n.flags.set_save_size(b(v)),
                        // The full group-8 id→name map is unknown. Anything else passes through
                        // to the property bag unchanged: the value is already in the merged
                        // collection and nothing else needs doing.
                        _ => {}
                    }
                }
            }
        }
    }

    /// The visibility test: every own-visible bit must be set and the
    /// parent chain must end at the manager's root. An unattached element is not visible.
    #[must_use]
    pub fn is_visible(&self, h: ElemHandle) -> bool {
        let mut at = h;
        loop {
            let Some(n) = self.node(at) else { return false };
            if !n.region.flags.visible {
                return false;
            }
            let Some(parent) = n.region.parent else {
                return at == self.root();
            };
            at = parent;
        }
    }

    /// The element's set-visible, including the activation half of
    /// the show. Its guard is an effective visibility edge, not
    /// the argument alone: a repeated show or a show below a hidden parent cannot raise a window.
    pub fn set_visible(&mut self, h: ElemHandle, v: bool) {
        let was_visible = self.is_visible(h);
        let Some(n) = self.node_mut(h) else { return };
        if n.region.flags.visible == v {
            let mut out = std::mem::take(&mut self.requests);
            if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
                b.after_set_visible(self, &mut out, h, v);
            }
            self.requests = out;
            return;
        }
        n.region.flags.visible = v;
        let visible = self.is_visible(h);
        self.broadcast_element_message(h, msg::element::id::VISIBILITY_CHANGED, u32::from(v), 0);
        self.broadcast_global(msg::global::ELEMENT_GEOMETRY, 0);
        if was_visible != visible {
            if visible {
                if self.node(h).is_some_and(|n| n.flags.activate_on_show()) {
                    self.activate(h);
                }
            } else {
                // Retail unregisters BEFORE deactivation/fallback. A hidden popup must not
                // remain a candidate when activate-next searches the existing registration order.
                self.activatable.retain(|&candidate| candidate != h);
                if self.node(h).is_some_and(|n| n.flags.is_active()) {
                    self.deactivate(h);
                    if self.node(h).is_some_and(|n| n.flags.is_root_element()) {
                        self.activate_next();
                    }
                }
            }
        }
        let mut out = std::mem::take(&mut self.requests);
        if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
            b.after_set_visible(self, &mut out, h, v);
        }
        self.requests = out;
    }

    /// Set whether the element should be mouse-visible.
    pub fn set_mouse_visible(&mut self, h: ElemHandle, v: bool) {
        if let Some(n) = self.node_mut(h) {
            n.should_be_mouse_visible = v;
            n.is_mouse_visible = v;
        }
    }

    /// Recompute mouse visibility — the element is mouse-visible if it should be *or* if the
    /// subclass's should-be-mouse-visible override says so.
    ///
    /// **Deferred while the element's own behaviour is lifted out of its slot.** In the
    /// client the should-be-mouse-visible query is a virtual call on a live object and is always
    /// answerable; here a widget running one of its own callbacks has been moved out of the arena,
    /// so asking the node for it answers `false` — and `false` is not "no opinion", it is
    /// `Button`'s folded `return true;` stub silently becoming a `return
    /// false;`. That is not theoretical: the button's attribute setter calls
    /// the mouse-visibility update for attribute `0x0D` from *inside* itself, so **every button whose
    /// `Disabled` was set after creation would stop being hit-testable** — the wizard's Profession,
    /// Skills and Town tabs, which the wizard's progress-state setter writes `0x0D` on
    /// every time it runs, would be inert to a real pointer while every other button on the same
    /// screen answered one. The same queue [`Self::queue_set_state`] uses, for the same reason.
    pub fn update_mouse_visibility(&mut self, h: ElemHandle) {
        if self.node(h).is_some_and(|n| n.behaviour.is_none()) {
            self.deferred_mouse_visibility.push(h);
            return;
        }
        // The base implementation makes elements with a context menu or tooltip mouse-visible.
        // Every type that does not always return true inherits this behavior.
        //
        // Without it an element that is mouse-visible *because it has a
        // tooltip* — which is how `Radar` becomes clickable over a blip, and how any plain
        // region with hover text becomes hoverable — would be mouse-invisible, and the pointer would
        // go straight through it. The subclass override is OR-ed on top, not instead: the five
        // grab-with-the-mouse types return true unconditionally.
        let base = self
            .node(h)
            .is_some_and(|n| n.flags.context_menu() || n.tooltip_text.is_some());
        let sub = self
            .node(h)
            .and_then(|n| n.behaviour.as_ref().map(|b| b.should_be_mouse_visible()))
            .unwrap_or(false);
        if let Some(n) = self.node_mut(h) {
            n.is_mouse_visible = n.should_be_mouse_visible || base || sub;
        }
    }
}
