//! The skills page: its set-up, input and read-outs.

use super::*;

impl CharGenScreen {
    // ---------------------------------------------------------------------------------------
    // The skills page
    // ---------------------------------------------------------------------------------------

    /// The skills page's skill-records step.
    ///
    /// A flush, then the four category rows from template **0**, then one row from template **1**
    /// per entry of the `SkillTable`'s hash — each carrying its skill id in the instance property
    /// `0x1000000A`, its name as the row caption, and the two costs the heritage overrides.
    ///
    /// The heritage arm is reproduced as the client writes it and is **not** the trained-cost
    /// lookup: the page keeps the `SkillTable`'s base costs and only reacts when the heritage's own
    /// cost is **zero** — a normal cost of 0 makes the skill free *and* unremovable and takes its
    /// trained cost out of the specialised one; a primary cost of 0 does the same for
    /// specialisation. A heritage that merely made a skill *cheaper* would not be honoured here,
    /// and none of the thirteen shipped heritages does: every one overrides Arcane Lore (14) alone,
    /// with `(0, 2)`.
    pub fn do_skill_records(&mut self, ui: &mut UiSystem) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(list) = ui.get_child_recursive(root, skills_page::LIST) else {
            return;
        };
        // The previous build's rows and headers both go.
        if let Some(mut old) = self.skill_list.take() {
            old.flush(ui);
        }
        for r in std::mem::take(&mut self.skill_rows) {
            if let Some(h) = r.element {
                ui.remove_and_delete_root(h);
            }
        }
        for h in std::mem::take(&mut self.skill_headers) {
            ui.remove_and_delete_root(h);
        }
        let mut lb = ListBoxWidget::bind(ui, list);

        // The four category rows, **all four from template 0** and all four appended first — four
        // adds from template 0, appended. They are the group *bounds*, not a block of headings:
        // every skill row is inserted between two of them below.
        for (label, _tip) in SKILL_CATEGORIES {
            let Some(h) = lb.add_from_template(ui, 0, None) else {
                break;
            };
            let text = self.string(ui, label);
            if let Some(t) = ui
                .get_child_recursive(h, skills_page::HEADER_TEXT)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&text);
            }
            self.skill_headers.push(h);
        }

        let heritage = self.state.heritage_group;
        let over = tables
            .chargen
            .heritage_groups
            .get(&heritage)
            .map(|h| h.skills.clone())
            .unwrap_or_default();
        let mut rows: Vec<SkillRecord> = Vec::new();
        for (id, base) in &tables.skills.skills {
            let mut train = base.trained_cost;
            let mut spec = base.specialized_cost;
            let mut untrainable = train != 0;
            let mut unspecializable = spec != 0;
            if let Some((_, normal, primary)) = over.iter().find(|(s, _, _)| s == id) {
                if *normal == 0 {
                    spec -= train;
                    train = 0;
                    untrainable = false;
                }
                if *primary == 0 {
                    spec = 0;
                    unspecializable = false;
                }
            }
            rows.push(SkillRecord {
                skill: *id,
                name: base.name.clone(),
                train_cost: train,
                spec_cost: spec,
                untrainable,
                unspecializable,
                level: self.state.skill_level(*id),
                score: self.state.skill_score(&tables.skills, *id),
                min_level: base.min_level,
                element: None,
            });
        }
        // The client walks the char-gen state's own skill list, whose order it never relies on: the
        // skill-entry update re-files every row by name. Sorting here only makes `skill_rows` read
        // the way the page does.
        rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.skill.cmp(&b.skill)));
        for r in &mut rows {
            // An add from template 1 — appended at the end, which is where the skill-entry update's
            // remove then takes it from.
            let Some(h) = lb.add_from_template(ui, 1, None) else {
                break;
            };
            if let Some(n) = ui.node_mut(h) {
                n.instance_properties.set(
                    ATTR_ROW_SKILL_ID,
                    dereth_assets::ui::PropertyValue::InstanceId(r.skill),
                );
            }
            ui.set_mouse_visible(h, true);
            if let Some(t) = ui
                .get_child_recursive(h, skills_page::ROW_NAME)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&r.name);
            }
            r.element = Some(h);
        }
        self.skill_rows = rows;
        self.skill_list = Some(lb);
        for i in 0..self.skill_rows.len() {
            self.update_skill_entry(ui, i);
        }
        self.update_credits_meter(ui);
    }

    /// Update and refile one skills-page entry.
    ///
    /// The row is removed from the list's items (not deleted), its text is set, and it is inserted
    /// sorted between its own header and the next one.
    ///
    /// The remove-then-insert is the whole point: **this is what puts a row under its own heading**
    /// in the list's items, and `Self::relayout_skill_list` is what turns that order into the
    /// rectangles the page draws.
    ///
    /// This alone does not move the row on screen: a re-file is **two** steps, the array and the
    /// geometry. If the list box's scrollable half caches each row's origin behind a key that
    /// cannot see a re-order, it puts every row back on the next tick and a trained skill stays
    /// under Useable Untrained Skills until the page is left and re-entered. An assertion that
    /// walks `skill_list.items` proves only the first step.
    pub fn update_skill_entry(&mut self, ui: &mut UiSystem, i: usize) {
        let Some((h, level, min_level, name)) = self
            .skill_rows
            .get(i)
            .and_then(|r| Some((r.element?, r.level, r.min_level, r.name.clone())))
        else {
            return;
        };
        // The client takes the row out of the list's items; it does **not** delete it, because the
        // insert below puts it straight back.
        if let Some(lb) = self.skill_list.as_mut() {
            lb.items.retain(|x| *x != h);
        }
        self.set_skill_text(ui, i);
        let Some(group) = skill_group(level, min_level) else {
            // The skill-text and skill-entry updates' `default:`: an undefined skill is removed and
            // never re-inserted, so it has no row at all.
            ui.remove_and_delete_root(h);
            if let Some(r) = self.skill_rows.get_mut(i) {
                r.element = None;
            }
            self.relayout_skill_list(ui);
            return;
        };
        let at = self.sorted_insert_index(ui, group, &name);
        if let Some(lb) = self.skill_list.as_mut() {
            let at = at.min(lb.items.len());
            lb.items.insert(at, h);
        }
        self.relayout_skill_list(ui);
    }

    /// Where in the list's items a row goes.
    ///
    /// Starting after the group's own header and stopping at the next header (or the end), it reads
    /// each item's `0x1000000A` skill id, skips anything that is not a skill row (a header has no
    /// such attribute), and stops at the first row whose name sorts after the new one (`wcscmp`);
    /// the row is inserted there. So it is an insertion sort by name, ascending, **inside one group
    /// only** — the same shape used by the in-game panel.
    fn sorted_insert_index(&self, ui: &UiSystem, group: usize, name: &str) -> usize {
        let Some(lb) = self.skill_list.as_ref() else {
            return 0;
        };
        let Some(header) = self.skill_headers.get(group).copied() else {
            return lb.items.len();
        };
        let Some(start) = lb.index_of(header) else {
            return lb.items.len();
        };
        let end = self
            .skill_headers
            .get(group + 1)
            .and_then(|h| lb.index_of(*h))
            .unwrap_or(lb.items.len());
        let mut i = start + 1;
        while i < end {
            let existing = lb.items[i];
            let other = ui
                .node(existing)
                .and_then(|n| match n.instance_properties.get(ATTR_ROW_SKILL_ID) {
                    Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => Some(*v),
                    _ => None,
                })
                .and_then(|id| self.skill_rows.iter().find(|r| r.skill == id));
            if let Some(o) = other {
                if name < o.name.as_str() {
                    break;
                }
            }
            i += 1;
        }
        i
    }

    /// The list box's layout update, which the client runs itself whenever its items change —
    /// insert and remove both mark the layout dirty.
    ///
    /// Positioning the rows by hand at a fixed 21-pixel pitch cannot express a header row of a
    /// different height and leaves every row's `y` wrong the moment the list is re-ordered.
    fn relayout_skill_list(&mut self, ui: &mut UiSystem) {
        if let Some(lb) = self.skill_list.as_mut() {
            lb.update_layout(ui);
        }
    }

    /// The *Skill Level* number, the two credit costs
    /// and the two arrow states.
    ///
    /// The skill level is recomputed as the skill score and printed with `"%d"`. Then, by class:
    ///
    /// | class | up cost | down cost | + arrow on when | - arrow on when |
    /// |---|---|---|---|---|
    /// | untrained | train cost (blank if ≥ 999) | 0 | credits ≥ train cost | never |
    /// | trained | spec − train (blank if ≥ 999) | train cost | credits ≥ spec − train | untrainable |
    /// | specialized | 0 | spec − train | never | unspecializable |
    ///
    /// An undefined class writes nothing more. "On" is state `0x1000001B` and "off" `0x1000001A`.
    ///
    /// **Every constant here is the client's own**: the 999 is a `< 0x3E7` test, the empty string
    /// is `L""` and the format is `L"%d"`.
    fn set_skill_text(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some((h, skill, level, train, spec, untrainable, unspecializable)) =
            self.skill_rows.get(i).and_then(|r| {
                Some((
                    r.element?,
                    r.skill,
                    r.level,
                    r.train_cost,
                    r.spec_cost,
                    r.untrainable,
                    r.unspecializable,
                ))
            })
        else {
            return;
        };
        // The skill level is the skill score, recomputed on every call, which is how the number
        // rises by 5 when the skill is trained and by 10 when it is specialised.
        let score = self.state.skill_score(&tables.skills, skill);
        if let Some(r) = self.skill_rows.get_mut(i) {
            r.score = score;
        }
        set_row_text(ui, h, skills_page::ROW_LEVEL, &score.to_string());
        if level == SkillAdvancementClass::Inactive {
            return;
        }
        let credits = self.state.remaining_skill_credits;
        // 999 or more prints as the empty string `L""`.
        let printed = |v: i32| {
            if v < 999 {
                v.to_string()
            } else {
                String::new()
            }
        };
        let (up, down, up_on, down_on) = match level {
            SkillAdvancementClass::Untrained => {
                (printed(train), "0".to_string(), credits >= train, false)
            }
            SkillAdvancementClass::Trained => (
                printed(spec - train),
                train.to_string(),
                credits >= spec - train,
                untrainable,
            ),
            SkillAdvancementClass::Specialized => (
                "0".to_string(),
                (spec - train).to_string(),
                false,
                unspecializable,
            ),
            SkillAdvancementClass::Inactive => return,
        };
        set_row_text(ui, h, skills_page::ROW_UP_COST, &up);
        set_row_text(ui, h, skills_page::ROW_DOWN_COST, &down);
        for (id, on) in [
            (skills_page::ROW_INCREASE, up_on),
            (skills_page::ROW_DECREASE, down_on),
        ] {
            if let Some(c) = ui.get_child_recursive(h, id) {
                ui.set_state(c, if on { STATE_ARROW_ON } else { STATE_ARROW_OFF });
            }
        }
    }

    /// The skill-text update over every record.
    ///
    /// It is what both arrows call after a purchase, and it is not cosmetic: buying one skill
    /// changes the remaining skill credits, and every *other* row's `+` has to go dark the moment
    /// the credits can no longer pay for it.
    pub fn update_all_training_values(&mut self, ui: &mut UiSystem) {
        for i in 0..self.skill_rows.len() {
            self.set_skill_text(ui, i);
        }
    }

    /// The skills page's credits meter update.
    pub fn update_credits_meter(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let v = self.state.remaining_skill_credits;
        if let Some(t) = ui
            .get_child_recursive(root, skills_page::CREDITS_FIELD)
            .and_then(|f| ui.get_child_recursive(f, skills_page::READOUT_TEXT))
            .and_then(|c| ui.text_element_mut(c))
        {
            t.set_text(&v.to_string());
        }
    }

    /// Untrained → trained → specialised, each
    /// gated on the credits the step costs.
    ///
    /// The specialise step costs `spec - train`, not `spec`: the skill has already paid to be
    /// trained.
    pub fn increase_skill_level(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(r) = self.skill_rows.get(i) else {
            return;
        };
        let (skill, train, spec, level) = (r.skill, r.train_cost, r.spec_cost, r.level);
        let credits = self.state.remaining_skill_credits;
        let next = match level {
            SkillAdvancementClass::Untrained | SkillAdvancementClass::Inactive
                if credits >= train =>
            {
                SkillAdvancementClass::Trained
            }
            SkillAdvancementClass::Trained if credits >= spec - train => {
                SkillAdvancementClass::Specialized
            }
            _ => return,
        };
        self.state
            .set_skill_level(&tables.chargen, &tables.skills, skill, next);
        if let Some(r) = self.skill_rows.get_mut(i) {
            r.level = next;
        }
        self.update_skill_entry(ui, i);
        // Every arm of the skill-level increase falls into this tail.
        self.update_all_training_values(ui);
        self.update_credits_meter(ui);
    }

    /// The inverse, refused for a skill the heritage granted (unspecializable / untrainable false).
    pub fn decrease_skill_level(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(r) = self.skill_rows.get(i) else {
            return;
        };
        let (skill, level, untrainable, unspecializable) =
            (r.skill, r.level, r.untrainable, r.unspecializable);
        let next = match level {
            SkillAdvancementClass::Specialized if unspecializable => SkillAdvancementClass::Trained,
            SkillAdvancementClass::Trained if untrainable => SkillAdvancementClass::Untrained,
            _ => return,
        };
        self.state
            .set_skill_level(&tables.chargen, &tables.skills, skill, next);
        if let Some(r) = self.skill_rows.get_mut(i) {
            r.level = next;
        }
        self.update_skill_entry(ui, i);
        // The decrease path reaches the same tail.
        self.update_all_training_values(ui);
        self.update_credits_meter(ui);
    }
}
