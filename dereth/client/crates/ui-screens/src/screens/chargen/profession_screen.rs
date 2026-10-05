//! The profession page: its set-up, input and read-outs.

use super::*;

impl CharGenScreen {
    // ---------------------------------------------------------------------------------------
    // The profession page
    // ---------------------------------------------------------------------------------------

    /// Put the previously-current profession button back to state `0x10000016`, put the one the
    /// template names into `0x10000017`, and show that template's blurb in the text box.
    ///
    /// A template outside 0..=6 falls into the switch's `default:`, which leaves the current
    /// profession button **null** and sets no text at all.
    pub fn update_profession(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        if let Some(prev) = self.current_profession_button {
            if let Some(h) = ui.get_child_recursive(root, prev) {
                ui.set_state(h, STATE_PROFESSION_OFF);
            }
        }
        let found = PROFESSION_BUTTONS
            .iter()
            .find(|(_, t, _)| *t == self.state.template);
        self.current_profession_button = found.map(|(id, _, _)| *id);
        self.profession_text = found.map(|(_, _, tok)| *tok);
        if let Some((id, _, _)) = found {
            if let Some(h) = ui.get_child_recursive(root, *id) {
                ui.set_state(h, STATE_PROFESSION_ON);
            }
        }
        if let (Some(tok), Some(h)) = (
            self.profession_text,
            ui.get_child_recursive(root, PROFESSION_DESC),
        ) {
            let text = self
                .world_profession_text()
                .unwrap_or_else(|| self.string(ui, tok));
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&text);
            }
        }
    }

    /// The six sliders and the four read-outs, and then the template fit + profession update off
    /// its own tail.
    ///
    /// Each slider gets its value as text on the value field and as float attribute `0x86 = value *
    /// 0.01` on the scroll bar; the credits-available field gets the remaining attribute credits.
    pub fn update_attribute_values(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (field, attr) in ATTRIBUTE_SLIDERS {
            let Some(f) = ui.get_child_recursive(root, field) else {
                continue;
            };
            let v = self.state.get(attr);
            if let Some(h) = ui.get_child_recursive(f, slider::VALUE) {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&v.to_string());
                }
            }
            if let Some(h) = ui.get_child_recursive(f, slider::SCROLL) {
                #[allow(clippy::cast_precision_loss)] // 10..=100 is exact in f32
                let pos = v as f32 * 0.01;
                // Setting the float attribute *stores* the value and then invokes attribute
                // handling. A bare handler call would leave the position the page wrote out of the
                // element's own property collection — and with the scrollbar reading attribute 0x86
                // back out of it every layout, a write that stores nothing puts the thumb back
                // where it was.
                ui.set_attribute_float(h, ATTR_SCROLL_POSITION, pos);
            }
        }
        // The credits available, then the three vitals.
        //
        // These readouts use endurance/2, endurance and self directly; this path does not
        // load the secondary-attribute table.
        let endurance = self.state.get(Attr::Endurance);
        let readouts = [
            self.state.remaining_atrb_credits,
            endurance / 2,
            endurance,
            self.state.get(Attr::Self_),
        ];
        for ((field, text), v) in PROFESSION_READOUTS.iter().zip(readouts) {
            let Some(f) = ui.get_child_recursive(root, *field) else {
                continue;
            };
            let Some(h) = ui.get_child_recursive(f, *text) else {
                continue;
            };
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&v.to_string());
            }
        }
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.update_profession(ui);
    }

    /// The profession page's attribute-value write.
    ///
    /// The page clamps the request to what the budget can pay for — the current value plus the
    /// remaining credits with this attribute excluded — **before** calling the setter, which is why
    /// raising a slider past the budget lands on the highest legal value instead of being refused.
    pub fn set_attrib_value(&mut self, ui: &mut UiSystem, a: Attr, value: i32) {
        let cur = self.state.get(a);
        let head = self.state.get_abs_remaining_credits(Some(a));
        let value = if value - cur >= 0 && head < value - cur {
            cur + head
        } else {
            value
        };
        self.state.set_attribute(a, value);
        self.update_attribute_values(ui);
    }

    /// The toggle, its state writes on both the scrollbar and the lock button, and the attribute
    /// lock.
    pub fn set_lock(&mut self, ui: &mut UiSystem, a: Attr) {
        let now = !self.state.is_locked(a);
        self.state.set_locked(a, now);
        let state = if now {
            STATE_SLIDER_LOCKED
        } else {
            STATE_SLIDER_UNLOCKED
        };
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(field) = ATTRIBUTE_SLIDERS
            .iter()
            .find(|(_, at)| *at == a)
            .and_then(|(id, _)| ui.get_child_recursive(root, *id))
        else {
            return;
        };
        for id in [slider::SCROLL, slider::LOCK] {
            if let Some(h) = ui.get_child_recursive(field, id) {
                ui.set_state(h, state);
            }
        }
    }

    /// Reset the attribute locks first, then clear the lock on every slider that still *shows* as
    /// locked, which is what puts the states back.
    pub fn clear_locks(&mut self, ui: &mut UiSystem) {
        for a in Attr::ALL {
            self.state.set_locked(a, false);
        }
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (id, _) in ATTRIBUTE_SLIDERS {
            let Some(field) = ui.get_child_recursive(root, id) else {
                continue;
            };
            for c in [slider::SCROLL, slider::LOCK] {
                if let Some(h) = ui.get_child_recursive(field, c) {
                    ui.set_state(h, STATE_SLIDER_UNLOCKED);
                }
            }
        }
    }

    /// The profession page's "update to default attributes", which every profession button runs
    /// after setting and applying the template.
    pub fn update_to_default_attributes(&mut self, ui: &mut UiSystem) {
        self.clear_locks(ui);
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.update_profession(ui);
        self.update_attribute_values(ui);
    }

    /// One of the seven profession buttons: set and apply the template, then update to the default
    /// attributes.
    pub fn choose_profession(&mut self, ui: &mut UiSystem, template: i32) {
        let Some(t) = self.tables.clone() else { return };
        self.state
            .set_template(&t.chargen, &t.skills, template, true);
        self.update_to_default_attributes(ui);
        self.do_skill_records(ui);
    }

    /// The profession page's update — fit the template to the character, update the profession,
    /// update the attribute values.
    pub fn profession_page_update(&mut self, ui: &mut UiSystem) {
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.update_profession(ui);
        self.update_attribute_values(ui);
    }
}
