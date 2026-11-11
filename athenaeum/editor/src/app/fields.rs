//! The inspector's fields of the selected row: every field of its kind, each with a control for
//! the kind of value it holds (a check box, a choice of values, a quest and where it must be, a
//! string of the install), and plain TOML text for the rest. A change is set when it is made, or
//! for text when the field is left; Escape leaves text as it was. A storekeeper has its shop's
//! title too: what its shop window is called.

use athenaeum_foundry::fields::{self, Control, Field};
use athenaeum_foundry::Entry;
use yalain_world_db::rows::{Gate, Row};

use super::{parse_id, Editor};

/// What is being typed into the fields that are more than one box: a gate's quest and phase, and
/// a name's table and string.
#[derive(Debug, Clone, Default)]
pub(super) struct Typing {
    gate: Option<Gate>,
    quest: String,
    phase: u32,
    name: [String; 2],
    /// The shop title of a storekeeper.
    shop: String,
}

/// Where a gate's quest must be, as the inspector offers it.
const GATES: [(&str, Option<Gate>); 5] = [
    ("anyone", None),
    ("done", Some(Gate::Done)),
    ("under way", Some(Gate::UnderWay)),
    ("at phase", Some(Gate::Phase(1))),
    ("had", Some(Gate::Had)),
];

fn gate_word(g: Option<Gate>) -> &'static str {
    match g {
        None => "anyone",
        Some(Gate::Done) => "done",
        Some(Gate::UnderWay) => "under way",
        Some(Gate::Phase(_)) => "at phase",
        Some(Gate::Had) => "had",
    }
}

/// A row's gate.
const fn gate_of(row: &Row) -> Option<(u32, Gate)> {
    match row {
        Row::Object(o) => o.requires,
        _ => None,
    }
}

/// A row's name of the install's text.
fn name_of(row: &Row) -> Option<(u32, u32)> {
    match row {
        Row::Object(o) => o.name.map(|n| (n.table, n.string)),
        Row::Camp(c) => c.name.map(|n| (n.table, n.string)),
        _ => None,
    }
}

impl Editor<'_> {
    /// The fields the inspector shows for the selected row, each by its name and its text as
    /// shown, as the last frame drew them.
    #[must_use]
    pub fn shown_fields(&self) -> &[(String, String)] {
        &self.fields
    }

    /// The shop title the inspector shows for the selected storekeeper, as the last frame drew it.
    #[must_use]
    pub fn shown_shop_title(&self) -> &str {
        &self.typing.shop
    }

    /// The selected row's fields, after where it stands: each by its kind of value, set as it is
    /// changed.
    pub(super) fn row_fields(&mut self, ui: &mut egui::Ui, item: &str, e: &Entry) {
        let row = &e.row.row;
        let fresh = self.fields_of.as_ref() != Some(&(item.to_string(), self.session.revision()));
        if fresh {
            self.fields = fields::of(row)
                .iter()
                .map(|f| (f.name.to_string(), fields::text(row, f.name)))
                .collect();
            let (quest, phase) = match gate_of(row) {
                Some((q, Gate::Phase(n))) => (format!("0x{q:08X}"), n),
                Some((q, _)) => (format!("0x{q:08X}"), 1),
                None => (String::new(), 1),
            };
            let name = name_of(row).map_or_else(
                || [String::new(), String::new()],
                |(t, s)| [format!("0x{t:08X}"), format!("0x{s:08X}")],
            );
            let shop = match row {
                Row::Object(o) => self.session.shop_title(o.template).unwrap_or_default(),
                _ => "",
            }
            .to_owned();
            self.typing = Typing {
                gate: gate_of(row).map(|(_, g)| g),
                quest,
                phase,
                name,
                shop,
            };
            self.fields_of = Some((item.to_string(), self.session.revision()));
        }
        let mut set: Option<(String, String)> = None;
        let mut texts = std::mem::take(&mut self.fields);
        let mut typing = std::mem::take(&mut self.typing);
        let mut group = "";
        let mut wrong = None;
        let keeper = match row {
            Row::Object(o) => self.session.keeper(o.template).map(|_| o.template),
            _ => None,
        };
        let mut retitled = None;
        egui::Grid::new("fields")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (f, (_, text)) in fields::of(row).iter().zip(texts.iter_mut()) {
                    if f.group != group {
                        group = f.group;
                        ui.label(egui::RichText::new(group).strong());
                        ui.end_row();
                    }
                    ui.label(f.name).on_hover_text(f.hint);
                    match control(ui, f, text, row, &mut typing) {
                        Ok(Some(v)) => set = Some((f.name.to_string(), v)),
                        Ok(None) => {}
                        Err(why) => wrong = Some(why),
                    }
                    ui.end_row();
                }
                if keeper.is_some() {
                    ui.label("Shop title").on_hover_text(
                        "what this storekeeper's shop window is called; empty: the store's own name",
                    );
                    let r = ui.add(egui::TextEdit::singleline(&mut typing.shop).desired_width(220.0));
                    if r.lost_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            typing.shop = String::new();
                            self.fields_of = None;
                        } else {
                            retitled = Some(typing.shop.clone());
                        }
                    }
                    ui.end_row();
                }
            });
        self.fields = texts;
        self.typing = typing;
        if let Some(why) = wrong {
            self.status = why;
            self.fields_of = None;
        }
        if let (Some(template), Some(title)) = (keeper, retitled) {
            if self.session.shop_title(template).unwrap_or_default() != title.trim() {
                self.set_shop_title(item, &title);
            }
            return;
        }
        let unchanged = |field: &str, value: &str| fields::text(row, field) == value.trim();
        let Some((field, value)) = set.filter(|(f, v)| !unchanged(f, v)) else {
            return;
        };
        if field == "key" {
            match self.session.rename_place(item, value.trim()) {
                Ok(renamed) => self.select(Some(renamed)),
                Err(err) => self.status = err,
            }
            self.fields_of = None;
            return;
        }
        match self
            .session
            .set(item, &field, &fields::toml(row, &field, &value))
        {
            Ok(_) => self.status = format!("{item}: {field} set"),
            Err(err) => {
                self.status = err;
                self.fields_of = None;
            }
        }
    }
}

impl Editor<'_> {
    /// Give the shop of the selected storekeeper `item` the title `title` (empty: the store's own
    /// name again), as one step; what came of it is the status.
    pub fn set_shop_title(&mut self, item: &str, title: &str) {
        let Some(Row::Object(o)) = self.session.entry(item).map(|e| e.row.row.clone()) else {
            self.status = format!("{item} is not a person");
            return;
        };
        let store = yalain_world_db::compile::keeper_store(self.install, o.template);
        match self.session.set_shop_title(o.template, title, store) {
            Ok(keeper) => self.status = format!("{keeper}: shop title set"),
            Err(err) => self.status = err,
        }
        self.fields_of = None;
    }
}

/// One field's control: what it is to be set to once changed (TOML, or words for a field of
/// words), or why what was typed is no value.
pub(super) fn control(
    ui: &mut egui::Ui,
    f: &Field,
    text: &mut String,
    row: &Row,
    typing: &mut Typing,
) -> Result<Option<String>, String> {
    match f.control {
        Control::Toml | Control::Words => {
            let r = ui.add(
                egui::TextEdit::singleline(text)
                    .desired_width(220.0)
                    .font(egui::TextStyle::Monospace),
            );
            if r.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                return Ok(Some(text.clone()));
            }
            if r.lost_focus() {
                // Escape: as it was.
                *text = fields::text(row, f.name);
            }
            Ok(None)
        }
        Control::Flag => {
            let mut on = text.trim() == "true";
            if ui.checkbox(&mut on, "").changed() {
                return Ok(Some(if on { "true".into() } else { String::new() }));
            }
            Ok(None)
        }
        Control::Choice {
            values,
            optional,
            none,
        } => {
            let shown = values.iter().find(|(v, _)| *v == text.trim()).map_or_else(
                || {
                    if text.trim().is_empty() {
                        none.to_string()
                    } else {
                        text.clone()
                    }
                },
                |(_, s)| (*s).to_string(),
            );
            let mut chose = None;
            egui::ComboBox::from_id_salt(("field", f.name))
                .selected_text(shown)
                .width(220.0)
                .show_ui(ui, |ui| {
                    if optional && ui.selectable_label(text.trim().is_empty(), none).clicked() {
                        chose = Some(String::new());
                    }
                    for (v, s) in values {
                        if ui.selectable_label(text.trim() == *v, *s).clicked() {
                            chose = Some((*v).to_string());
                        }
                    }
                });
            Ok(chose)
        }
        Control::Gate => gate_control(ui, typing),
        Control::Name => name_control(ui, typing),
    }
}

/// A gate: where the quest must be (or anyone may use it), the quest, and its phase.
fn gate_control(ui: &mut egui::Ui, typing: &mut Typing) -> Result<Option<String>, String> {
    let mut gate = typing.gate;
    let mut changed = false;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("gate")
            .selected_text(gate_word(gate))
            .width(90.0)
            .show_ui(ui, |ui| {
                for (word, g) in GATES {
                    if ui.selectable_label(gate_word(gate) == word, word).clicked() {
                        gate = g;
                        changed = true;
                    }
                }
            });
        if gate.is_some() {
            let r = ui
                .add(
                    egui::TextEdit::singleline(&mut typing.quest)
                        .hint_text("quest")
                        .desired_width(90.0)
                        .font(egui::TextStyle::Monospace),
                )
                .on_hover_text("the quest's id");
            changed |= r.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape));
        }
        if let Some(Gate::Phase(_)) = gate {
            let r = ui.add(egui::DragValue::new(&mut typing.phase).range(1..=1000));
            changed |= r.drag_stopped() || (r.changed() && !r.dragged());
        }
    });
    typing.gate = gate;
    if !changed {
        return Ok(None);
    }
    let Some(gate) = gate else {
        return Ok(Some(String::new()));
    };
    let gate = match gate {
        Gate::Phase(_) => Gate::Phase(typing.phase.max(1)),
        g => g,
    };
    if typing.quest.trim().is_empty() {
        // Nothing to set until the quest is typed.
        return Ok(None);
    }
    let quest = parse_id(&typing.quest)
        .ok_or_else(|| format!("requires: {:?} is no quest id", typing.quest.trim()))?;
    Ok(Some(fields::gate_toml(quest, gate)))
}

/// A string of the install: its table and its string; both left empty take it away.
fn name_control(ui: &mut egui::Ui, typing: &mut Typing) -> Result<Option<String>, String> {
    let mut left = false;
    ui.horizontal(|ui| {
        for (i, hint) in ["table", "string"].iter().enumerate() {
            let r = ui
                .add(
                    egui::TextEdit::singleline(&mut typing.name[i])
                        .hint_text(*hint)
                        .desired_width(90.0)
                        .font(egui::TextStyle::Monospace),
                )
                .on_hover_text(format!("the {hint}'s id"));
            left |= r.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape));
        }
    });
    if !left {
        return Ok(None);
    }
    let [table, string] = [&typing.name[0], &typing.name[1]].map(|s| s.trim());
    if table.is_empty() && string.is_empty() {
        return Ok(Some(String::new()));
    }
    if table.is_empty() || string.is_empty() {
        return Ok(None);
    }
    match (parse_id(table), parse_id(string)) {
        (Some(t), Some(s)) => Ok(Some(fields::name_toml(t, s))),
        _ => Err(format!("name: {table} and {string} are no ids")),
    }
}
