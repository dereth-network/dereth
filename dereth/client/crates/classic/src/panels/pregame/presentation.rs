use super::*;
use crate::int::i32_from;
use dereth_client_contract::{persist::CharacterIdentity, pregame::PregameView};
use dereth_primitives::num::to_i32_f64;

pub(super) struct CharacterRow<'a> {
    pub wire_slot: Option<usize>,
    pub character: Option<&'a CharacterIdentity>,
}

// Choosing a character uses its index in the server's list; the screen sorts its rows, not
// that list.
pub(super) fn characters(view: &PregameView) -> Vec<CharacterRow<'_>> {
    let Some(set) = &view.character_set else {
        return vec![];
    };
    let mut rows: Vec<_> = set
        .set
        .iter()
        .enumerate()
        .filter(|(_, c)| c.id.0 != 0)
        .map(|(slot, c)| CharacterRow {
            wire_slot: Some(slot),
            character: Some(c),
        })
        .collect();
    rows.sort_by(|a, b| a.character.unwrap().name.cmp(&b.character.unwrap().name));
    let blanks = (set.num_allowed_characters as usize).saturating_sub(rows.len());
    rows.extend((0..blanks).map(|_| CharacterRow {
        wire_slot: None,
        character: None,
    }));
    rows
}
pub(super) fn first_empty_slot(view: &PregameView) -> Option<i32> {
    let set = view.character_set.as_ref()?;
    (0..set.num_allowed_characters as usize)
        .find(|&i| set.set.get(i).is_none_or(|c| c.id.0 == 0))
        .map(i32_from)
}

pub(super) struct SkillRow {
    pub skill: Option<usize>,
    pub caption: String,
}
pub(super) fn skill_rows(d: &CreationData, state: &SelectionView<'_>) -> Vec<SkillRow> {
    let mut rows = vec![];
    for (level, label) in [
        (3, "Specialized Skills"),
        (2, "Trained Skills"),
        (1, "Untrained Skills"),
    ] {
        rows.push(SkillRow {
            skill: None,
            caption: label.into(),
        });
        let mut indices: Vec<_> = d
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.chargen != 0 && state.skills.get(&s.id).copied().unwrap_or(1) == level
            })
            .map(|(i, _)| i)
            .collect();
        indices.sort_by(|&a, &b| d.skills[a].name.cmp(&d.skills[b].name));
        if indices.is_empty() {
            rows.push(SkillRow {
                skill: None,
                caption: "(none)".into(),
            });
        }
        for i in indices {
            rows.push(SkillRow {
                skill: Some(i),
                caption: d.skills[i].name.clone(),
            });
        }
    }
    rows
}
fn clipped_image(f: &mut PanelFrame, did: u32, r: crate::widgets::Rect, clip: [i32; 4]) {
    f.image(&format!("{did:08X}"), r, false, false);
    if let Some(crate::Command::Image { clip: c, .. }) = f.screen.commands.last_mut() {
        *c = Some(clip);
    }
}

/// One 250x60 profession row at `y` in `bounds`: the portrait at half size, a plate beside it (lit
/// when chosen) and the name centred on the plate in white; the custom row's name is smaller and
/// lower.
#[allow(clippy::too_many_arguments)]
fn profession_row(
    fonts: &crate::renderer::FontMetrics,
    f: &mut PanelFrame,
    bounds: crate::widgets::Rect,
    y: i32,
    name: &str,
    icon: u32,
    chosen: bool,
    custom: bool,
) {
    let clip = [bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h];
    clipped(f, icon, rect(bounds.x, y, 40, 60), clip, false);
    let plate = rect(bounds.x + 40, y, 210, 60);
    f.image_native(
        if chosen { "06000FA8" } else { "06000FA9" },
        plate.x,
        plate.y,
        plate.intersect(bounds).unwrap_or_default(),
        false,
    );
    let (top, font) = if custom {
        (16, "20-8")
    } else {
        (10, "times-35-13-bold")
    };
    line(
        fonts,
        f,
        rect(plate.x, y + top, plate.w, 40),
        name,
        font,
        WHITE,
        TextAlign::Center,
        clip,
    );
}

impl Pregame {
    /// The profession page: the custom row, the list of professions under it (each row the
    /// profession's portrait at half size and its name on a plate), and the chosen one's portrait.
    pub(super) fn profession_frame(&self, f: &mut PanelFrame, d: &CreationData) {
        const ROW: i32 = 60;
        let state = self.view(d);
        let templates = &state.sex(d).templates;
        let chosen = |i: usize| !self.custom_selected && state.template == i && i < templates.len();
        f.fill(rect(508, 288, 254, 214), 0xff00_0000);
        f.fill(rect(388, 283, 84, 124), 0xff00_0000);
        if let Some(t) = templates.get(state.template) {
            clipped(
                f,
                t.icon,
                rect(390, 285, 80, 120),
                [390, 285, 470, 405],
                false,
            );
        }
        text(f, rect(510, 170, 250, 18), "Choose abilities from scratch:");
        let custom = rect(510, 190, 250, ROW);
        f.control(
            "custom",
            custom,
            ControlKind::HitList {
                row_count: usize::from(!templates.is_empty()),
                row_height: ROW,
                selected: (self.custom_selected).then_some(0),
                offset: 0,
            },
            true,
        );
        if let Some(t) = templates.first() {
            profession_row(
                &self.resources.fonts,
                f,
                custom,
                custom.y,
                "Custom Character",
                t.icon,
                self.custom_selected,
                true,
            );
        }
        text(f, rect(510, 270, 250, 18), "...or start with a profession:");
        let bounds = rect(510, 290, 250, 210);
        let count = templates.len();
        let max = (i32_from(count) * ROW - bounds.h).max(0);
        let offset = self.profession_scroll.min(max);
        f.control(
            "profession",
            bounds,
            ControlKind::HitList {
                row_count: count,
                row_height: ROW,
                selected: (!self.custom_selected && self.state.template >= 0)
                    .then_some(state.template),
                offset,
            },
            true,
        );
        for (i, t) in templates.iter().enumerate() {
            let y = bounds.y + i32_from(i) * ROW - offset;
            if y + ROW > bounds.y && y < bounds.y + bounds.h {
                profession_row(
                    &self.resources.fonts,
                    f,
                    bounds,
                    y,
                    &t.name,
                    t.icon,
                    chosen(i),
                    false,
                );
            }
        }
        f.control(
            "profession-scroll",
            rect(760, 290, 20, 210),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: offset,
                page: bounds.h,
                step: ROW,
                vertical: true,
                arrow_size: 20,
                thumb_size: 28,
            },
            true,
        );
    }
    pub(super) fn skills_frame(&self, f: &mut PanelFrame, d: &CreationData) {
        const ROW: i32 = 26;
        let rows = skill_rows(d, &self.view(d));
        let bounds = rect(385, 195, 376, 260);
        let max = (i32_from(rows.len()) * ROW - bounds.h).max(0);
        let offset = self.skill_scroll.min(max);
        text(f, rect(415, 170, 200, 20), "Available Skill Credits:");
        f.label(
            694,
            171,
            self.state.remaining_skill_credits.to_string(),
            "16-7",
            COLOR,
            None,
        );
        f.control(
            "skills",
            bounds,
            ControlKind::HitList {
                row_count: rows.len(),
                row_height: ROW,
                selected: rows
                    .iter()
                    .position(|r| r.skill == self.selected_skill && r.skill.is_some()),
                offset,
            },
            true,
        );
        f.control(
            "skills-scroll",
            rect(763, 195, 20, 260),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: offset,
                page: bounds.h,
                step: ROW,
                vertical: true,
                arrow_size: 20,
                thumb_size: 28,
            },
            true,
        );
        let clip = [bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h];
        for (row, item) in rows.iter().enumerate() {
            let y = bounds.y + i32_from(row) * ROW - offset;
            if y + ROW <= bounds.y || y >= bounds.y + bounds.h {
                continue;
            }
            let at = |x: i32, w: i32| rect(bounds.x + x, y, w, ROW);
            let Some(i) = item.skill else {
                // A group's header, or the note that the group is empty.
                if item.caption.starts_with('(') {
                    line(
                        &self.resources.fonts,
                        f,
                        at(2, 374),
                        &item.caption,
                        "16-7",
                        COLOR,
                        TextAlign::Left,
                        clip,
                    );
                } else {
                    clipped_image(f, 0x0600_02EA, at(0, bounds.w), clip);
                    line(
                        &self.resources.fonts,
                        f,
                        at(2, 161),
                        &item.caption,
                        "16-7",
                        COLOR,
                        TextAlign::Left,
                        clip,
                    );
                    line(
                        &self.resources.fonts,
                        f,
                        at(163, 211),
                        "Skill Level",
                        "16-7",
                        COLOR,
                        TextAlign::Left,
                        clip,
                    );
                }
                continue;
            };
            let selected = item.skill == self.selected_skill;
            clipped_image(
                f,
                if selected { 0x0600_1246 } else { 0x0600_1248 },
                at(0, bounds.w),
                clip,
            );
            let skill = &d.skills[i];
            clipped(f, skill.icon, rect(bounds.x, y + 3, 20, 20), clip, false);
            line(
                &self.resources.fonts,
                f,
                at(29, 169),
                &skill.name,
                "16-7",
                COLOR,
                TextAlign::Left,
                clip,
            );
            line(
                &self.resources.fonts,
                f,
                at(170, 44),
                self.view(d).skill_value(d, skill).to_string(),
                "16-7",
                COLOR,
                TextAlign::Right,
                clip,
            );
            let level = self.view(d).skills.get(&skill.id).copied().unwrap_or(0);
            let trained = self.view(d).cost(d, skill.id, 2);
            let spec = self.view(d).cost(d, skill.id, 3);
            let up = if level == 1 { trained } else { spec - trained };
            let down = if level == 3 { spec - trained } else { trained };
            if y >= bounds.y && y + ROW <= bounds.y + bounds.h {
                for (id, x, caption, enabled, images) in [
                    (
                        "skill-down",
                        319,
                        if level > 1 && down > 0 {
                            down.to_string()
                        } else {
                            String::new()
                        },
                        level > 1 && self.view(d).cost(d, skill.id, level) != 0,
                        ["0600123F", "06001240", "06001242"],
                    ),
                    (
                        "skill-up",
                        344,
                        if level < 3 && up != 999 {
                            up.to_string()
                        } else {
                            String::new()
                        },
                        level > 0
                            && level < 3
                            && up >= 0
                            && self.state.remaining_skill_credits >= up,
                        ["06001243", "06001244", "06001241"],
                    ),
                ] {
                    let b = f.button(format!("{id}-{i}"), at(x, 25), caption, enabled);
                    b.images = Some(images.map(String::from));
                    b.font = "15-6".into();
                    b.color = WHITE;
                }
            }
        }
        if let Some(s) = self.selected_skill.and_then(|i| d.skills.get(i)) {
            let height = self
                .resources
                .fonts
                .text_height("15-6", &s.description, 380)
                .unwrap_or(66);
            let box_ = rect(385, 462, 380, 66);
            let max = (height - box_.h).max(0);
            let offset = self.skill_help_scroll.min(max);
            f.text_box(
                rect(box_.x, box_.y - offset, box_.w, box_.h + offset),
                &s.description,
                "15-6",
                COLOR,
                TextAlign::Left,
                true,
                Some([box_.x, box_.y, box_.x + box_.w, box_.y + box_.h]),
            );
            f.control(
                "skill-help",
                box_,
                ControlKind::HitList {
                    row_count: usize::try_from(height.max(box_.h)).unwrap_or(0),
                    row_height: 1,
                    selected: None,
                    offset,
                },
                true,
            );
            f.control(
                "skill-help-scroll",
                rect(765, 462, 16, 66),
                ControlKind::ScrollBar {
                    min: 0,
                    max,
                    value: offset,
                    page: box_.h,
                    step: 15,
                    vertical: true,
                    arrow_size: 16,
                    thumb_size: 16,
                },
                true,
            );
        }
    }

    pub(super) fn credits(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = PanelFrame::new(800, 600);
        f.fill(rect(0, 0, 800, 600), 0xff000000);
        let dm = self.credits_variant;
        let photos: &[u32] = if dm {
            &[
                0x1a98, 0x1a9f, 0x1a99, 0x1aa0, 0x1a9a, 0x1aa1, 0x1a9b, 0x1aa2, 0x1a9c, 0x1aa3,
                0x1a9d, 0x1a9e,
            ]
        } else {
            &[0x1a98, 0x1a99, 0x1a9a, 0x1a9b, 0x1a9c, 0x1a9d, 0x1a9e]
        };
        let elapsed = crate::clock::seconds(c.now, self.credits_started);
        let distance = to_i32_f64(elapsed * 32.);
        let cycle = i32_from(photos.len()) * 300;
        for (i, &did) in photos.iter().enumerate() {
            let y = (100 + i32_from(i) * 300 - distance + 300).rem_euclid(cycle) - 300;
            clipped_image(
                &mut f,
                0x06000000 | did,
                rect(0, y, 400, 300),
                [0, 0, 400, 600],
            );
        }
        if let Ok(d) = &self.data {
            let id = if dm { 0x31000020u32 } else { 0x31000022 };
            if let Some(t) = d.help_text.get(&id.to_string()) {
                f.text_box(
                    rect(400, 10 - to_i32_f64(elapsed * 25.), 400, 100000),
                    t,
                    "16-7",
                    COLOR,
                    TextAlign::Center,
                    true,
                    Some([400, 0, 800, 600]),
                );
            }
        }
        f.button("credits-exit", rect(0, 0, 800, 600), "", true)
            .paint = false;
        f
    }
    pub(super) fn credits_finished(&self, now: dereth_primitives::LocalTime) -> bool {
        let dm = self.credits_variant;
        let key = if dm { 0x31000020u32 } else { 0x31000022 };
        self.data
            .as_ref()
            .ok()
            .and_then(|d| d.text_heights.get(&key.to_string()))
            .is_some_and(|&height| {
                crate::clock::seconds(now, self.credits_started) * 25. - 10. >= height as f64
            })
    }

    pub(super) fn keyboard(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = PanelFrame::new(800, 600);
        f.image("06000508", rect(0, 0, 800, 600), true, false);
        f.text_box(
            rect(100, 30, 300, 40),
            "Keyboard Configuration",
            "times-25-11",
            0xfffafaf0,
            TextAlign::Left,
            false,
            None,
        );
        text(&mut f, rect(40, 100, 200, 20), "Current Keyboard Scheme:");
        f.image("06001276", rect(240, 100, 273, 18), false, false);
        f.image("06001274", rect(513, 100, 20, 19), false, false);
        f.text_box(
            rect(242, 102, 269, 16),
            c.keyboard
                .schemes
                .get(c.keyboard.scheme as usize)
                .map_or("", String::as_str),
            "15-6",
            0xff080808,
            TextAlign::Left,
            false,
            None,
        );
        f.control(
            "scheme",
            rect(240, 100, 293, 19),
            ControlKind::Choice {
                options: c.keyboard.schemes.clone(),
                selected: c.keyboard.scheme as usize,
            },
            true,
        )
        .paint = false;
        f.button("key-save", rect(600, 90, 100, 36), "Save As...", true);
        f.button("key-done", rect(600, 130, 100, 36), "Done", true);
        f.button(
            "key-reset",
            rect(235, 125, 150, 36),
            "Reset Keys",
            c.keyboard.dirty && (c.keyboard.warning.is_none() || c.keyboard.scheme != 0),
        );
        if let Some(warning) = &c.keyboard.warning {
            f.text_box(
                rect(20, 562, 760, 38),
                warning,
                "14-6",
                0xffffc080,
                TextAlign::Left,
                true,
                None,
            );
        }
        f.button(
            "key-delete",
            rect(390, 125, 150, 36),
            "Delete Scheme",
            c.keyboard.scheme != 0,
        );
        f.text_box(
            rect(0, 170, 800, 25),
            "Select a slot to add or change a key.",
            "times-18-7-bold",
            COLOR,
            TextAlign::Center,
            false,
            None,
        );
        let count = c.keyboard.bindings.len();
        let offset = self
            .keyboard_scroll
            .min((i32_from(count) * 17 - 360).max(0));
        f.control(
            "bindings",
            rect(50, 200, 680, 360),
            ControlKind::HitList {
                row_count: count,
                row_height: 17,
                selected: self.keyboard_row,
                offset,
            },
            true,
        );
        f.control(
            "bindings-scroll",
            rect(730, 200, 20, 360),
            ControlKind::ScrollBar {
                min: 0,
                max: (i32_from(count) * 17 - 360).max(0),
                value: offset,
                page: 360,
                step: 17,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        for row in 0..count {
            let y = 200 + i32_from(row) * 17 - offset;
            if y + 17 <= 200 || y >= 560 {
                continue;
            }
            let clip = [50, 200, 730, 560];
            for (x, w) in [(50, 215), (265, 155), (420, 155), (575, 155)] {
                clipped_image(
                    &mut f,
                    if c.keyboard.bindings[row].action == 0 {
                        0x6000f90
                    } else {
                        0x60011a6
                    },
                    rect(x + 1, y, w - 1, 17),
                    clip,
                );
            }
            let label = &c.keyboard.bindings[row].label;
            f.text_box(
                rect(52, y, 211, 17),
                label,
                "14-6",
                COLOR,
                TextAlign::Left,
                false,
                Some(clip),
            );
            for slot in 0..3 {
                let caption = c.keyboard.bindings[row]
                    .keys
                    .get(slot)
                    .cloned()
                    .unwrap_or_default();
                let r = rect(265 + i32_from(slot) * 155, y, 155, 17);
                f.text_box(
                    r,
                    caption,
                    "14-6",
                    COLOR,
                    TextAlign::Left,
                    false,
                    Some(clip),
                );
                if c.keyboard.bindings[row].action > 0
                    && c.keyboard.bindings[row].action != u32::MAX
                    && y >= 200
                    && y + 17 <= 560
                {
                    f.button(format!("binding-{row}-{slot}"), r, "", true).paint = false;
                }
            }
        }
        if let Some(name) = &self.save_scheme {
            for control in &mut f.controls {
                control.enabled = false;
            }
            f.image("06000523", rect(200, 200, 403, 253), false, false);
            f.text_box(
                rect(220, 220, 363, 153),
                "Save Current Scheme",
                "times-25-11",
                0xfffafaf0,
                TextAlign::Center,
                true,
                None,
            );
            f.image("06001274", rect(523, 300, 20, 19), false, false);
            f.control(
                "scheme-saved-name",
                rect(250, 300, 293, 19),
                ControlKind::Choice {
                    options: c.keyboard.schemes.clone(),
                    selected: c.keyboard.scheme as usize,
                },
                true,
            )
            .paint = false;
            let edit = f.edit(
                "scheme-name",
                rect(250, 300, 273, 18),
                name,
                79,
                false,
                true,
            );
            edit.background = None;
            edit.color = 0xff080808;
            edit.images = Some(["06001276".into(), "0600127A".into(), "06001276".into()]);
            f.button(
                "key-save-confirm",
                rect(230, 403, 100, 36),
                "Save",
                !name.trim().is_empty(),
            );
            f.button("key-save-cancel", rect(473, 403, 100, 36), "Cancel", true);
        } else if self.page == "key-edit" {
            for control in &mut f.controls {
                control.enabled = false;
            }
            f.image("06000523", rect(200, 200, 403, 253), false, false);
            let binding = self.keyboard_row.and_then(|i| c.keyboard.bindings.get(i));
            f.text_box(
                rect(220, 220, 363, 153),
                format!(
                    "\nType a key for {}",
                    binding.map_or("", |b| b.label.as_str())
                ),
                "times-25-11",
                0xfffafaf0,
                TextAlign::Center,
                true,
                None,
            );
            f.text_box(
                rect(200, 340, 403, 40),
                format!(
                    "Currently mapped to {}.",
                    binding
                        .and_then(|b| b.keys.get(self.key_slot))
                        .map_or("", String::as_str)
                ),
                "16-7",
                COLOR,
                TextAlign::Center,
                true,
                None,
            );
            f.button(
                "key-remove",
                rect(230, 403, 100, 36),
                "Remove Current Key",
                binding.is_some(),
            );
            f.button("key-cancel", rect(473, 403, 100, 36), "Cancel", true);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn sorted_character_display_does_not_change_first_empty_wire_slot() {
        let mut view = PregameView::default();
        let set = dereth_client_contract::persist::CharacterSet {
            set: vec![
                CharacterIdentity {
                    id: ObjectId(3),
                    name: "Zed".into(),
                    seconds_grace_period: 0,
                },
                CharacterIdentity::default(),
                CharacterIdentity {
                    id: ObjectId(4),
                    name: "Amy".into(),
                    seconds_grace_period: 10,
                },
            ],
            ..Default::default()
        };
        view.character_set = Some(set);
        assert_eq!(characters(&view)[0].character.unwrap().name, "Amy");
        assert_eq!(first_empty_slot(&view), Some(1));
        assert_eq!(characters(&view).len(), 5);
    }
}
