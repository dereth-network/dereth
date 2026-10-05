//! The stack splitter: the selected stack's quantity box and slider.

use super::*;

impl GamePlayScreen {
    /// The selected-stack hotkey receiver.
    ///
    /// The native gates are in this order: the notice id still equals the current selection, the
    /// object resolves, its raw stack size is nonzero and greater than one, and the registered
    /// entry box exists. The receiver then gives that box focus and selects all of its text, so
    /// the next digit replaces the whole quantity.
    pub fn recv_split_stack(&mut self, ui: &mut UiSystem, selected: ObjectId, view: &dyn GameView) {
        if view.selection() != Some(selected) {
            return;
        }
        let Some(stack) = view.slot_decoration(selected).map(|d| d.stack_size) else {
            return;
        };
        if stack <= 1 {
            return;
        }
        let Some(box_h) = self.toolbar_children.get("stack_size_entry_box") else {
            return;
        };
        ui.take_focus(box_h);
        if let Some(text) = ui.text_element_mut(box_h) {
            text.select_all();
        }
    }

    /// The element-message handler's `0x100001A3` / message `0x2F` arm — the stack-size
    /// entry box gaining and losing focus.
    ///
    /// Gaining focus (`p1 != 0`) selects all the text and stops. Losing it parses the text
    /// (`wcstoul`), clamps it to `1..=max` (0 becomes 1), writes the clamped value back if it
    /// differs from what was typed, stores it as the split size, and — when it changed — sets the
    /// slider's `0x86` to `v / max` and sends the stack-slider-changed notice.
    ///
    /// [`crate::toolbar::splitter::Splitter::on_text`] is the parse-clamp-store part and had no
    /// caller outside its own tests, which is why `UiRequest::StackSliderChanged` was emitted by
    /// nothing and **every** split ran on the default whole-stack `SplitState` in
    /// `interaction.rs`.
    pub fn on_stack_box_focus(&mut self, ui: &mut UiSystem, gained: bool) {
        let Some(box_h) = self.toolbar_children.get("stack_size_entry_box") else {
            return;
        };
        if gained {
            // The text element's select all.
            if let Some(t) = ui.text_element_mut(box_h) {
                t.select_all();
            }
            return;
        }
        self.commit_stack_box(ui, box_h);
    }

    /// Commit the text currently visible in the stack entry through the same clamp/slider/notice
    /// path as its ordinary focus-loss handler.
    pub(super) fn commit_stack_box(&mut self, ui: &mut UiSystem, box_h: ElemHandle) {
        let before = self.splitter.split_size;
        let typed = read_text(ui, box_h);
        let (notice, position) = self.splitter.on_text(&typed);
        let clamped = self.splitter.split_size;
        // "write it back if it was clamped".
        if crate::toolbar::splitter::parse_stack_quantity(&typed) != clamped {
            if let Some(t) = ui.text_element_mut(box_h) {
                t.set_text(&clamped.to_string());
            }
        }
        if clamped != before {
            if let Some(s) = self.toolbar_children.get("stack_size_slider") {
                set_attr_float(ui, s, attr::SLIDER_POSITION, position);
            }
            ui.requests.emit(notice);
        }
    }

    /// The element-message handler's `0x100001A4` / message `0x0A` arm — the slider
    /// moved.
    ///
    /// `split = unsigned_clamp(1 - trunc(p1 * max * widened(-0.001f)), 1, max)`, then
    /// the text box is written and the notice sent. The position0 endpoint is one item.
    ///
    /// Retail reads the message's `p1` as unsigned and multiplies by the widened -0.001f constant;
    /// it does not read the slider attribute back or reverse the formula. See
    /// `Splitter::on_slider`.
    pub fn on_stack_slider(&mut self, ui: &mut UiSystem, position_thousandths: u32) {
        let notice = self.splitter.on_slider(position_thousandths);
        if let Some(b) = self.toolbar_children.get("stack_size_entry_box") {
            let v = self.splitter.split_size.to_string();
            if let Some(t) = ui.text_element_mut(b) {
                t.set_text(&v);
            }
        }
        ui.requests.emit(notice);
    }

    /// The vendor panel's item list begin drag notice's partial sell-row tail:
    /// split size = maximum split size, then the update-toolbar-selection-display notice.
    ///
    /// Retail has one global pair. This rebuild has this visible toolbar copy and Interaction's
    /// inventory-facing copy, so the existing [`UiRequest::StackSliderChanged`] carries the same
    /// assignment across after the text and slider are refreshed here.
    pub fn reset_stack_split_to_max(&mut self, ui: &mut UiSystem) {
        self.splitter.split_size = self.splitter.max_split_size;
        if let Some(b) = self.toolbar_children.get("stack_size_entry_box") {
            if let Some(text) = ui.text_element_mut(b) {
                text.set_text(&self.splitter.split_size.to_string());
            }
        }
        if let Some(slider) = self.toolbar_children.get("stack_size_slider") {
            set_attr_float(ui, slider, attr::SLIDER_POSITION, 1.0);
        }
        ui.requests.emit(UiRequest::StackSliderChanged {
            split: self.splitter.split_size,
            max: self.splitter.max_split_size,
        });
    }

    /// The client's focus arm: global message 1, action
    /// `0x27`, **while the stack-size box has focus** — write the split size into the box and
    /// relinquish focus.
    ///
    /// Retail loads the split size, not the maximum split size: cancel restores the committed
    /// quantity. This global arm does not bypass the text element's own action handler consuming
    /// an ordinary focused `EditControls` Escape key.
    ///
    /// Relinquishing focus raises `0x2F` with `p1 = 0` on the box, so the read-back and the notice
    /// come out of [`Self::on_stack_box_focus`] rather than being duplicated here — which is the
    /// client's own path too.
    pub fn reset_stack_size_box(&mut self, ui: &mut UiSystem) {
        let Some(h) = self.toolbar_children.get("stack_size_entry_box") else {
            return;
        };
        if ui.focus_element() != Some(h) {
            return;
        }
        let committed = self.splitter.split_size.to_string();
        if let Some(t) = ui.text_element_mut(h) {
            t.set_text(&committed);
        }
        ui.relinquish_focus(h);
    }

    /// Project the selected stack quantity and visibility. Missing selection or object rows
    /// leave the shared pair alone. The model owns reseeding; this gate only draws its answer
    /// and retains the selected-object meter queries.
    pub(super) fn apply_stack_split_gate(
        &mut self,
        ui: &mut UiSystem,
        edge: bool,
        stack: Option<u32>,
        sel: Option<ObjectId>,
        view: &dyn GameView,
    ) -> Option<SelectionQuery> {
        use crate::toolbar::splitter;

        let entry = self.toolbar_children.get("stack_size_entry_box");
        let slider = self.toolbar_children.get("stack_size_slider");
        let hide_or_show = |ui: &mut UiSystem, v: bool| {
            for h in [entry, slider].into_iter().flatten() {
                ui.set_visible(h, v);
            }
        };

        // The edge block: both hidden, and **the pair deliberately left alone**.
        if edge {
            hide_or_show(ui, false);
        }
        // The two early returns: nothing selected, and no weenie row.
        let stack_size = stack?;

        self.splitter = splitter::Splitter {
            split_size: u32::try_from(view.split_size()).unwrap_or(1),
            max_split_size: u32::try_from(view.max_split_size()).unwrap_or(1),
        };
        hide_or_show(ui, false);

        if !splitter::shows_split_widget(stack_size) {
            if let Some(h) = self.toolbar_children.get("sel_object_field") {
                // Both non-stack branches: state `0x1000000B`.
                ui.set_state(h, dereth_ui::StateId(0x1000_000B));
            }
            // **The not-a-stack arm.** The health and mana queries belong here and nowhere else;
            // the client's own behaviour, immediately after the "stack size below 2" test, is: for
            // an object that is not a player, has no pet owner and is not attackable, put the
            // selected-object field in state `0x1000000B`, send `QueryItemMana` if the player owns
            // it, and stop — **not** the health query. Everything else gets the same state and
            // `QueryHealth`.
            //
            // It is not "`QueryHealth` for anything `object_is_attackable` accepts": being a player
            // or having a pet owner reaches the health leg **without consulting
            // `object_is_attackable` at all**, and the mana leg is further gated on
            // `is_owned_by_player`, so an object that is none of those gets **neither** query. Read,
            // not inferred.
            let id = sel?;
            let Some(f) = view.selection_query_facts(id) else {
                return Some(SelectionQuery::NotAsked);
            };
            if !f.is_player && !f.has_pet_owner && !f.attackable {
                if f.owned_by_player {
                    ui.requests.emit(UiRequest::QueryItemMana(id));
                    return Some(SelectionQuery::ItemMana);
                }
                return Some(SelectionQuery::Neither);
            }
            ui.requests.emit(UiRequest::QueryHealth(id));
            return Some(SelectionQuery::Health);
        };
        if let Some(h) = self.toolbar_children.get("sel_object_field") {
            // Stack arm: state `0x1000000C`.
            ui.set_state(h, dereth_ui::StateId(0x1000_000C));
        }
        let s = self.splitter;
        // Write the split size into the stack-size entry box.
        if let Some(h) = entry {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&s.split_size.to_string());
            }
        }
        // Set the slider's float attribute `0x86` to split size / maximum split size.
        //
        // A fresh seed writes 1.0, the whole-stack end. Retail's slider handler confirms the same
        // direction; retail does not invert this value.
        if let Some(h) = slider {
            set_attr_float(ui, h, attr::SLIDER_POSITION, s.slider_position());
        }
        hide_or_show(ui, true);
        // The stack arm asks for neither query: the client's `else` seeds the pair and falls
        // straight through to the common return. A stack is asked for **nothing**.
        None
    }
}
