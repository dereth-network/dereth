//! The summary page, and the profession and summary pages' first set-up.

use super::*;

impl CharGenScreen {
    // ---------------------------------------------------------------------------------------
    // The summary page
    // ---------------------------------------------------------------------------------------

    /// Update the character-generation summary page.
    ///
    /// Nothing while hidden. Otherwise it sets the per-heritage camera, fits the template to the
    /// character, writes the summary and how-to texts, updates the preview, gives the name box the
    /// focus, and — when no name has been entered — puts `ID_CharGen_NamePrompt` in the box and
    /// selects all of it.
    ///
    /// Taking focus is why a player can type a name the moment the page comes up without clicking
    /// the box first.
    ///
    /// **The update is a gesture, not a tick.** *"re-runs on every call"* is a claim about the
    /// body; how often it is **called** is a separate question, and the answer is not *"every
    /// frame"*. In retail it runs from exactly **three** places — the page initialisation (and only
    /// when visible), the progress-state write and the randomiser — and is never dispatched
    /// indirectly. The two live sites are the two below. A click into the box is none of the three,
    /// which is why retail leaves a plain caret there, as confirmed in a play check.
    pub fn summary_page_update(&mut self, ui: &mut UiSystem) {
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.set_summary_text(ui);
        self.set_how_to_text(ui);
        self.refresh_view(ui);
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(name) = ui.get_child_recursive(root, NAME_FIELD) else {
            return;
        };
        ui.take_focus(name);
        if !self.name_entered {
            let prompt = self.string(ui, NAME_PROMPT);
            if let Some(t) = ui.text_element_mut(name) {
                t.set_text(&prompt);
                t.select_all();
            }
        }
    }

    /// The summary page's summary text write — the left-hand list.
    ///
    /// A flush, then, from the list box's own three templates:
    ///
    /// | template | element | what it is |
    /// |---|---|---|
    /// | 0 | `0x100002F8`, text `0x100002F9` | one plain line |
    /// | 1 | `0x100002FA`, text `0x100000FE` | a section header |
    /// | 2 | `0x100002FB`, text `0x100002FC` / `0x100002FD` | a name/value pair |
    ///
    /// four plain lines (profession, gender, heritage, starting town), an **Attributes** header,
    /// the ten pairs of [`SUMMARY_ROWS`], and then the four skill sections of
    /// [`SUMMARY_SKILL_SECTIONS`], each a header followed by one pair per skill at that level with
    /// its [`CharGenState::skill_score`].
    ///
    /// Skills are written in name order. This implementation keeps its own sorted list
    /// rather than reproducing a hash table's bucket traversal order.
    pub fn set_summary_text(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(list) = ui.get_child_recursive(root, summary_page::LIST) else {
            return;
        };
        let Some(tables) = self.tables.clone() else {
            return;
        };
        for h in std::mem::take(&mut self.summary_rows) {
            ui.remove_and_delete_root(h);
        }
        // The list's template list — the list-box reader, which is the same property `0x64` this
        // module documents in [`ATTR_TEMPLATE_LIST`]; the skills list is a [`ListBoxWidget`] and
        // uses the same reader.
        let templates = crate::panels::listbox::template_list(ui, list);
        let mut y = 0;
        let mut rows: Vec<ElemHandle> = Vec::new();
        let mut add = |ui: &mut UiSystem, template: usize, cells: &[(ElementId, String)]| {
            let Some((layout, element)) = templates.get(template).copied() else {
                return;
            };
            let Ok(h) = ui
                .require_env()
                .and_then(|e| e.create_child_element_by_data_id(ui, list, layout, element))
            else {
                return;
            };
            ui.move_to(h, 0, y);
            let b = ui.screen_box(h);
            y += b.y1 - b.y0 + 1;
            for (id, text) in cells {
                if let Some(t) = ui
                    .get_child_recursive(h, *id)
                    .and_then(|c| ui.text_element_mut(c))
                {
                    t.set_text(text);
                }
            }
            rows.push(h);
        };

        // The four plain lines, each with the client's own bounds check on the name array.
        let name = |arr: &[&str], i: i64| -> String {
            usize::try_from(i)
                .ok()
                .and_then(|i| arr.get(i))
                .map_or_else(String::new, |s| (*s).to_string())
        };
        let lines = [
            format!(
                "Profession: {}",
                name(&PROFESSION_NAMES, i64::from(self.state.template))
            ),
            format!(
                "Gender: {}",
                name(&GENDER_NAMES, i64::from(self.state.gender))
            ),
            format!(
                "Heritage: {}",
                name(&HERITAGE_NAMES, i64::from(self.state.heritage_group))
            ),
            format!(
                "Starting Town: {}",
                usize::try_from(self.state.start_area)
                    .ok()
                    .and_then(|i| {
                        // A world that names the fourth town itself is shown its own name.
                        if i == 3 && self.fourth_town.is_some() {
                            return Some(self.town_name(3));
                        }
                        tables.chargen.starter_areas.get(i).map(|a| a.name.as_str())
                    })
                    .unwrap_or("?")
            ),
        ];
        for l in lines {
            add(ui, 0, &[(summary_page::LINE_TEXT, l)]);
        }
        add(
            ui,
            1,
            &[(summary_page::HEADER_TEXT, "Attributes".to_string())],
        );

        // The ten name/value pairs. The three vitals are the attribute-values update's own
        // endurance/2, endurance, self — attributes 2, 2 and 6.
        let endurance = self.state.get(Attr::Endurance);
        let values = [
            self.state.get(Attr::Strength),
            endurance,
            self.state.get(Attr::Coordination),
            self.state.get(Attr::Quickness),
            self.state.get(Attr::Focus),
            self.state.get(Attr::Self_),
            endurance / 2,
            endurance,
            self.state.get(Attr::Self_),
            self.state.remaining_skill_credits,
        ];
        for (label, v) in SUMMARY_ROWS.iter().zip(values) {
            add(
                ui,
                2,
                &[
                    (summary_page::PAIR_NAME, (*label).to_string()),
                    (summary_page::PAIR_VALUE, v.to_string()),
                ],
            );
        }

        // The four skill sections.
        let mut skills: Vec<(u32, String, i32)> = tables
            .skills
            .skills
            .iter()
            .map(|(id, b)| (*id, b.name.clone(), b.chargen_use))
            .collect();
        skills.sort_by(|a, b| a.1.cmp(&b.1));
        for (i, (header, level)) in SUMMARY_SKILL_SECTIONS.iter().enumerate() {
            add(ui, 1, &[(summary_page::HEADER_TEXT, (*header).to_string())]);
            for (id, sname, use_) in &skills {
                if self.state.skill_level(*id) != *level {
                    continue;
                }
                // The client's two arms: section 2 keeps `chargen_use < 2`, section 3 keeps the
                // rest. Sections 0 and 1 take everything at their level.
                if (i == 2 && *use_ >= 2) || (i == 3 && *use_ < 2) {
                    continue;
                }
                let score = self.state.skill_score(&tables.skills, *id);
                add(
                    ui,
                    2,
                    &[
                        (summary_page::PAIR_NAME, sname.clone()),
                        (summary_page::PAIR_VALUE, score.to_string()),
                    ],
                );
            }
        }
        self.summary_rows = rows;
    }

    /// The instruction pane, three runs concatenated:
    /// `ID_CharGen_SummaryHowTo`, the heritage-and-gender naming examples of
    /// [`SUMMARY_NAME_EXAMPLES`], and `ID_CharGen_SummaryHowToEnd`.
    ///
    /// The client appends each only if the string lookup succeeded **and** the string is not empty,
    /// and bails out of the whole chain on the first failure; the same order is kept here.
    pub fn set_how_to_text(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(h) = ui.get_child_recursive(root, summary_page::HOW_TO) else {
            return;
        };
        if let Some(own) = self.world_naming_text() {
            self.append_literal(ui, h, &own, 0, true);
            return;
        }
        let examples = SUMMARY_NAME_EXAMPLES
            .iter()
            .find(|(g, _, _)| *g == self.state.heritage_group)
            .map(|(_, male, female)| {
                if self.state.gender == 2 {
                    *female
                } else {
                    *male
                }
            });
        self.append_run(ui, h, SUMMARY_HOW_TO, 0, true);
        if let Some(t) = examples {
            self.append_run(ui, h, t, 0, false);
        }
        self.append_run(ui, h, SUMMARY_HOW_TO_END, 0, false);
    }

    /// The client's one write the page never repeats: the six attribute-name captions the char-gen
    /// state's attribute name read.
    ///
    /// The names are built into the client, not string-table tokens — "Strength", "Endurance",
    /// "Quickness", "Coordination", "Focus", "Self" — so the profession page is unlocalised in the
    /// same way the summary page is.
    pub fn initialize_profession_page(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (field, attr) in ATTRIBUTE_SLIDERS {
            let Some(f) = ui.get_child_recursive(root, field) else {
                continue;
            };
            let Some(h) = ui.get_child_recursive(f, slider::NAME) else {
                continue;
            };
            // **This is retail's own slip, reproduced rather than corrected.** The
            // page initialisation installs the number input filter on the attribute-name
            // **caption** and not on the editable number next to it:
            //
            // Find caption `0x100002ED`, install its numeric input filter, then find value
            // `0x100002EF` without installing a filter. The stored value-element member is
            // never read again. Finally write "Strength" on the same caption that got the filter.
            //
            // It looks like a mistake, and it is: the caption is not editable, so the filter never
            // fires. It is installed anyway, because a rebuild that "fixes" it would filter a field
            // retail does not filter, and because a layout change that made the caption editable
            // must diverge from retail here loudly rather than silently.
            dereth_ui::text::set_input_filter(ui, h, dereth_ui::text::number_input_filter);
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(attribute_name(attr));
            }
        }
    }

    /// The client's one line that is not a child binding: install the name input filter on the name
    /// box `0x10000402`.
    ///
    /// The filter is `isalpha(c)` for `c < 0x100` plus `'` (0x27), space (0x20) and
    /// `-` (0x2D) — so the character-name box refuses digits and every other punctuation mark,
    /// which is the one field in the whole client that refuses them. Without it a player could type
    /// `B0b!` into the box retail will not let them.
    pub fn initialize_summary_page(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(h) = ui.get_child_recursive(root, NAME_FIELD) else {
            return;
        };
        dereth_ui::text::set_input_filter(ui, h, dereth_ui::text::name_input_filter);
    }
}
