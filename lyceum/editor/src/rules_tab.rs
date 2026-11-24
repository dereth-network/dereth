//! The Rules tab: a world repository's rules (its `[[rules]]` row) at the layer edits go into.
//! Every rule the row can say, by name: the value of the era the world plays (its profile), the
//! value it has (the layer that set it beside it), and, held over a system, what it is. A change
//! is the layer's row of only what it changes ([`lyceum_foundry::rules`]); "Below" puts a rule back
//! as the world below the layer has it, "Era" as the era does.
//!
//! The vocabulary is the server's: the `rules` member of what `empyrean-import schema` writes.

use lyceum_foundry::edit::Op;
use lyceum_foundry::layer_rows;
use lyceum_foundry::repo::ShownItem;
use lyceum_foundry::rules::{self, Change, ShownRule};

use super::{EditorApp, Tab};

/// The Rules tab's state.
#[derive(Debug, Default)]
pub struct RulesView {
    /// Text the shown rules' names hold, case aside.
    pub filter: String,
    /// A rule being typed: its field and its text, written once it reads.
    pub typing: Option<(String, String)>,
}

/// A value as typed into a rule: TOML's (`true`, `275`, `["Axe", "Sword"]`), or else the text.
#[must_use]
pub fn typed(text: &str) -> toml::Value {
    let t = text.trim();
    toml::from_str::<toml::Table>(&format!("v = {t}"))
        .ok()
        .and_then(|mut v| v.remove("v"))
        .unwrap_or_else(|| toml::Value::String(t.to_owned()))
}

/// How many characters of a value the tab shows; the rest is held over it.
const SHORT: usize = 22;

/// A rule's section (its map: `features`, `formulas`, `properties`; none for the row's own
/// fields) and its name in it.
#[must_use]
pub fn section(field: &str) -> (Option<&str>, &str) {
    match field.split_once('.') {
        Some((map, name)) => (Some(map), name),
        None => (None, field),
    }
}

/// `text` cut to [`SHORT`] characters, marked where it is cut.
#[must_use]
pub fn short(text: &str) -> String {
    if text.chars().count() <= SHORT {
        return text.to_owned();
    }
    let mut s: String = text.chars().take(SHORT - 1).collect();
    s.push('…');
    s
}

/// A value as the tab writes it: text without its quotes.
fn shown(v: Option<&toml::Value>) -> String {
    match v {
        None => "—".into(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

impl EditorApp {
    /// Show the Rules tab: where it is, else floating over the world view.
    pub fn open_rules(&mut self) {
        self.bring_back(Tab::Rules);
        let world = self.world_rect.unwrap_or(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1400.0, 900.0),
        ));
        let size = egui::vec2(
            780.0_f32.min(world.width() - 32.0).max(320.0),
            640.0_f32.min(world.height() - 32.0).max(240.0),
        );
        let at =
            egui::Rect::from_min_size(egui::pos2(world.min.x + 48.0, world.min.y + 48.0), size);
        let dock = if self.dungeon.is_some() {
            self.dungeon_dock
                .get_or_insert_with(crate::docks::in_dungeon)
        } else {
            self.dock.get_or_insert_with(crate::docks::outdoors)
        };
        crate::docks::open_floating(dock, Tab::Rules, at);
        self.show_tab(Tab::Rules);
    }

    /// The rules below the layer being edited, and the layer's own row, its unsaved changes in.
    fn rules_rows(&mut self) -> Option<(Option<toml::Table>, Option<toml::Table>)> {
        let vocabulary = self.rows.vocabulary.clone()?;
        let below = self
            .rows
            .editing
            .as_ref()?
            .below
            .items
            .get(rules::KIND)
            .map(|i| i.row(&vocabulary));
        let file = self.session.mirror()?.pack.join(rules::FILE);
        let layer = self.session.rows_text(&file).and_then(|t| {
            layer_rows::row_in(&vocabulary, rules::KIND, &t, rules::KIND)
                .ok()
                .flatten()
        });
        Some((below, layer))
    }

    /// The world's rules at the layer being edited, as the tab shows them.
    pub(super) fn rules_shown(&mut self) -> Option<Vec<ShownRule>> {
        let vocabulary = self.rows.vocabulary.clone()?;
        let words = self.rows.rules.clone()?;
        let profile = self.rows.repo.as_ref()?.era.name();
        let layer = self.session.mirror()?.layer.clone();
        let (_, row) = self.rules_rows()?;
        let below_item = self
            .rows
            .editing
            .as_ref()
            .and_then(|e| e.below.items.get(rules::KIND).cloned());
        let world = match &row {
            Some(r) => Some(ShownItem::over(
                below_item.as_ref(),
                &vocabulary,
                rules::KIND,
                r,
                &layer,
                None,
                &Default::default(),
            )),
            None => below_item,
        };
        Some(rules::shown(&words, profile, world.as_ref()))
    }

    /// Rule `field` changed as `change` says, in the layer being edited.
    ///
    /// # Errors
    /// A world that isn't a repository being edited, or a file that doesn't read.
    pub(super) fn rules_change(&mut self, field: &str, change: &Change) -> Result<(), String> {
        let vocabulary = self
            .rows
            .vocabulary
            .clone()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let (below, layer) = self
            .rules_rows()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let row = rules::changed(&vocabulary, below.as_ref(), layer.as_ref(), field, change);
        let file = self
            .session
            .mirror()
            .ok_or_else(|| self.say("messages.not_in_repository"))?
            .pack
            .join(rules::FILE);
        let text = self.session.rows_text(&file);
        let new = layer_rows::write_row(
            &vocabulary,
            rules::KIND,
            text.as_deref(),
            rules::KIND,
            row.as_ref(),
        )?;
        if new != text {
            self.apply(&Op::WriteRows { file, text: new }, false)?;
        }
        Ok(())
    }

    /// The Rules tab's body.
    pub(super) fn rules_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        if self.rows.rules.is_none() {
            ui.label(t.get("rules.no_vocabulary"));
            return;
        }
        let Some(rules) = self.rules_shown() else {
            ui.label(t.get("rules.not_editing"));
            return;
        };
        let patch = self.session.mirror().is_some_and(|m| m.patch);
        let (layer, profile) = (
            self.session
                .mirror()
                .map(|m| m.layer.clone())
                .unwrap_or_default(),
            self.rows
                .repo
                .as_ref()
                .map(|r| r.era.name())
                .unwrap_or_default(),
        );
        ui.label(t.fill("rules.head", &[("layer", &layer), ("profile", &profile)]));
        ui.horizontal(|ui| {
            ui.label(t.get("rows.find"));
            ui.add(egui::TextEdit::singleline(&mut self.rules.filter).desired_width(180.0));
        });
        ui.separator();
        let want = self.rules.filter.trim().to_lowercase();
        let words = self.rows.rules.clone().unwrap_or_default();
        let mut change: Option<(String, Change)> = None;
        egui::ScrollArea::vertical()
            .id_salt("rules-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("rules-grid").striped(true).show(ui, |ui| {
                    ui.strong(t.get("rules.rule"));
                    ui.strong(t.fill("rules.profile", &[("profile", &profile)]));
                    ui.strong(t.get("rules.world"));
                    ui.strong(t.get("rules.set_by"));
                    ui.end_row();
                    let mut was = None;
                    for r in rules
                        .iter()
                        .filter(|r| want.is_empty() || r.field.to_lowercase().contains(&want))
                    {
                        let (part, plain) = section(&r.field);
                        if part != was {
                            was = part;
                            if let Some(p) = part {
                                ui.end_row();
                                ui.strong(t.get(&format!("rules.section_{p}")));
                                ui.end_row();
                            }
                        }
                        let name = ui.label(plain);
                        if let Some(about) = words.about(&r.field) {
                            name.on_hover_text(about);
                        }
                        let era = shown(r.profile.as_ref());
                        let cut = short(&era);
                        let l = ui.label(&cut);
                        if cut != era {
                            l.on_hover_text(era);
                        }
                        if let Some(c) = self.rule_value(ui, r, words.choices.get(&r.field)) {
                            change = Some((r.field.clone(), Change::To(c)));
                        }
                        ui.horizontal(|ui| match &r.set {
                            Some((_, by)) => {
                                ui.label(by);
                                // The base has nothing below it: only the era's.
                                if patch
                                    && by == &layer
                                    && ui
                                        .small_button(t.get("rules.below"))
                                        .on_hover_text(t.get("rules.below_tip"))
                                        .clicked()
                                {
                                    change = Some((r.field.clone(), Change::Below));
                                }
                                if ui
                                    .small_button(t.get("rules.era"))
                                    .on_hover_text(t.get("rules.era_tip"))
                                    .clicked()
                                {
                                    change = Some((r.field.clone(), Change::Profile));
                                }
                            }
                            None => {
                                ui.weak(t.get("rules.from_era"));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        if let Some((field, c)) = change {
            if let Err(why) = self.rules_change(&field, &c) {
                self.panels.message = why;
            }
        }
    }

    /// Rule `r`'s value, to change: a switch, a choice among `choices`, else its text, written once
    /// it is left. Answers the new value.
    fn rule_value(
        &mut self,
        ui: &mut egui::Ui,
        r: &ShownRule,
        choices: Option<&Vec<String>>,
    ) -> Option<toml::Value> {
        let value = r.value().cloned();
        if let Some(toml::Value::Boolean(on)) = value {
            let mut on2 = on;
            return (ui.checkbox(&mut on2, "").changed()).then_some(toml::Value::Boolean(on2));
        }
        if let Some(names) = choices {
            let now = shown(value.as_ref());
            let mut pick = None;
            egui::ComboBox::from_id_salt(("rule", &r.field))
                .selected_text(&now)
                .show_ui(ui, |ui| {
                    for n in names {
                        if ui.selectable_label(*n == now, n).clicked() && *n != now {
                            pick = Some(toml::Value::String(n.clone()));
                        }
                    }
                });
            return pick;
        }
        let typing = self
            .rules
            .typing
            .as_ref()
            .filter(|(f, _)| *f == r.field)
            .map(|(_, s)| s.clone());
        let mut text = typing.clone().unwrap_or_else(|| match &value {
            None => String::new(),
            Some(toml::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        });
        let whole = text.clone();
        let edit = ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(180.0))
            .on_hover_text(whole);
        if edit.changed() {
            self.rules.typing = Some((r.field.clone(), text.clone()));
        }
        if edit.lost_focus() && typing.is_some() {
            self.rules.typing = None;
            let v = typed(&text);
            return (Some(&v) != value.as_ref()).then_some(v);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's Rules tab)
    use super::*;

    /// A rule's typed text is read as TOML's value where it is one, else as text.
    #[test]
    fn a_typed_rule_reads_as_a_value_else_as_text() {
        assert_eq!(typed(" 275 "), toml::Value::Integer(275));
        assert_eq!(typed("false"), toml::Value::Boolean(false));
        assert_eq!(
            typed(r#"["Axe", "Sword"]"#),
            toml::Value::Array(vec!["Axe".into(), "Sword".into()])
        );
        assert_eq!(typed("all"), toml::Value::String("all".into()));
    }

    /// A rule is named in its section: a system by its name under the systems.
    #[test]
    fn a_rule_is_named_in_its_section() {
        assert_eq!(section("features.housing"), (Some("features"), "housing"));
        assert_eq!(section("max_level"), (None, "max_level"));
    }

    /// A long value is cut to a few words, marked where it is cut; a short one is whole.
    #[test]
    fn a_long_value_is_cut_and_marked() {
        assert_eq!(short("true"), "true");
        let long = r#"["Aluvian", "Gharundim", "Sho", "Viamontian"]"#;
        let cut = short(long);
        assert_eq!(cut.chars().count(), SHORT);
        assert!(cut.ends_with('…') && long.starts_with(cut.trim_end_matches('…')));
    }
}
