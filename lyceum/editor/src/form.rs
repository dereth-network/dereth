//! The weenie tab: the form of the weenie the selected spawn places (or the spawn browser's
//! choice), read from its SQL and written back to it. A pack's own weenie is edited in place; a
//! server class is shown, and copied into the pack (by its own class id, so the pack's copy takes
//! its place) to be edited. Each change is one step of the history.

use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::{Op, Weenie};

use super::{EditorApp, UiAction};

/// A change the form makes to its weenie's SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum FormEdit {
    /// Property `id` of type `kind` set to `value` (as typed), or taken away.
    Property {
        kind: PropertyKind,
        id: u32,
        value: Option<String>,
    },
    /// Column `column` of the `row`th row of `table` set to `value` (as typed).
    Cell {
        table: String,
        row: usize,
        column: String,
        value: String,
    },
    /// A row added to `table` with `values` (every other column the export's default).
    AddRow {
        table: String,
        values: Vec<(String, String)>,
    },
    /// The `row`th row of `table` taken away.
    RemoveRow { table: String, row: usize },
    /// An emote set of `category` (a number, or the server's name of one) added, with no
    /// actions.
    AddEmoteSet { category: String },
    /// Emote set `set` taken away, with its actions.
    RemoveEmoteSet { set: usize },
    /// Column `column` of emote set `set` set to `value`.
    EmoteSet {
        set: usize,
        column: String,
        value: String,
    },
    /// An action of `kind` (a number, or the server's name of one) added at the end of set
    /// `set`.
    AddAction { set: usize, kind: String },
    /// Action `action` of set `set` taken away.
    RemoveAction { set: usize, action: usize },
    /// Action `from` of set `set` moved to `to`.
    MoveAction { set: usize, from: usize, to: usize },
    /// Column `column` of action `action` of set `set` set to `value`.
    Action {
        set: usize,
        action: usize,
        column: String,
        value: String,
    },
    /// Quest `name` registered in the pack being edited (once a minute, without end).
    RegisterQuest { name: String },
    /// Column `column` of the pack's quest `name` set to `value`.
    Quest {
        name: String,
        column: String,
        value: String,
    },
    /// What it says when an emote of `category` runs (its tell), the set made when it has none.
    Says { category: u32, text: String },
    /// The quest its use asks after, its two answers and the stamp of it.
    QuestGiver { quest: String },
    /// Its weenie type.
    WeenieType(u32),
    /// Where it sends who uses it: cell `cell` at `at` (that cell's landblock's meters).
    Destination { cell: u32, at: [f32; 3] },
}

/// The quests the emotes of `doc` name: the messages of its quest actions, each once, as the
/// registry knows them (before an `@`).
#[must_use]
pub fn quests_named(doc: &SqlDoc) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for set in doc.emote_sets() {
        for a in doc.actions(&set) {
            let kind = doc
                .cell(a, "type")
                .and_then(|v| v.trim().parse::<u32>().ok());
            if !kind.is_some_and(|k| lyceum_foundry::content_check::QUEST_ACTIONS.contains(&k)) {
                continue;
            }
            let Some(m) = doc.cell(a, "message").filter(|m| m.trim() != "NULL") else {
                continue;
            };
            let name = content::unquote(&m);
            let name = name.split('@').next().unwrap_or("").trim().to_owned();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// `typed` as a value of enum `name`: a number, or the name the server gives one (the number
/// written with it as a comment, as the export writes it).
fn enum_value(typed: &str, name: &str, schema: Option<&content::Schema>) -> Option<String> {
    let t = typed.trim();
    if let Ok(n) = t.parse::<u32>() {
        return Some(match schema.and_then(|s| s.value(name, n)) {
            Some(word) => format!("{n} /* {word} */"),
            None => n.to_string(),
        });
    }
    let (n, word) = schema?
        .enums
        .get(name)?
        .iter()
        .find(|(_, w)| w.eq_ignore_ascii_case(t))?;
    Some(format!("{n} /* {word} */"))
}

/// The columns ACE's export writes for the tables the form adds rows to.
fn columns(table: &str) -> &'static [&'static str] {
    match table {
        "weenie_properties_spell_book" => &["object_Id", "spell", "probability"],
        "weenie_properties_create_list" => &[
            "object_Id",
            "destination_Type",
            "weenie_Class_Id",
            "stack_Size",
            "palette",
            "shade",
            "try_To_Bond",
        ],
        "weenie_properties_generator" => &[
            "object_Id",
            "probability",
            "weenie_Class_Id",
            "delay",
            "init_Create",
            "max_Create",
            "when_Create",
            "where_Create",
            "stack_Size",
            "palette_Id",
            "shade",
            "obj_Cell_Id",
            "origin_X",
            "origin_Y",
            "origin_Z",
            "angles_W",
            "angles_X",
            "angles_Y",
            "angles_Z",
        ],
        _ => &[],
    }
}

/// The value a new row's column takes when the form gives it none.
fn default_of(table: &str, column: &str) -> &'static str {
    match (table, column) {
        ("weenie_properties_spell_book", "probability") => "2",
        ("weenie_properties_create_list", "destination_Type" | "stack_Size") => "1",
        ("weenie_properties_create_list", "palette" | "shade") => "0",
        ("weenie_properties_create_list", "try_To_Bond") => "False",
        ("weenie_properties_generator", "probability" | "init_Create" | "max_Create") => "1",
        ("weenie_properties_generator", "delay") => "180",
        ("weenie_properties_generator", "when_Create") => "2",
        ("weenie_properties_generator", "where_Create") => "4",
        _ => "NULL",
    }
}

/// `typed` as a column's value: a number or a symbol's token as it is, `NULL`, a bit as the
/// export writes it, anything else quoted. `None` for nothing typed.
fn cell_value(typed: &str) -> Option<String> {
    let t = typed.trim();
    if t.is_empty() {
        return None;
    }
    if t.eq_ignore_ascii_case("null") {
        return Some("NULL".into());
    }
    if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") {
        return Some(if t.eq_ignore_ascii_case("true") {
            "True".into()
        } else {
            "False".into()
        });
    }
    if t.parse::<f64>().is_ok() || lyceum_foundry::sql_ids::number(t).is_some() {
        return Some(t.to_owned());
    }
    if t.starts_with("{{") && t.ends_with("}}") {
        return Some(t.to_owned());
    }
    Some(content::quote(&content::unquote(t)))
}

/// `edit` made to `doc`, named by `schema`; false when what was typed is not a value of its
/// column.
pub fn apply_edit(doc: &mut SqlDoc, edit: &FormEdit, schema: Option<&content::Schema>) -> bool {
    match edit {
        FormEdit::Property { kind, id, value } => match value {
            None => doc.remove_property(*kind, *id),
            Some(v) => {
                let name = schema
                    .and_then(|s| s.property(*kind, *id))
                    .map(str::to_owned);
                doc.set_property(*kind, *id, v, name)
            }
        },
        FormEdit::Cell {
            table,
            row,
            column,
            value,
        } => {
            let Some(r) = doc.rows(table).get(*row).copied() else {
                return false;
            };
            let Some(v) = cell_value(value) else {
                return false;
            };
            doc.set_cell(r, column, &v)
        }
        FormEdit::AddRow { table, values } => {
            let cols = columns(table);
            if cols.is_empty() {
                return false;
            }
            let mut vals: Vec<(&str, String)> = Vec::new();
            for c in cols.iter().skip(1) {
                let v = values
                    .iter()
                    .find(|(k, _)| k == c)
                    .and_then(|(_, v)| cell_value(v))
                    .unwrap_or_else(|| default_of(table, c).to_owned());
                vals.push((c, v));
            }
            doc.add_row(table, cols, &vals, None, None);
            true
        }
        FormEdit::RemoveRow { table, row } => {
            let Some(r) = doc.rows(table).get(*row).copied() else {
                return false;
            };
            doc.remove_row(r);
            true
        }
        FormEdit::AddEmoteSet { category } => {
            let Some(c) = enum_value(category, "EmoteCategory", schema) else {
                return false;
            };
            doc.add_emote_set(&c, &[]);
            true
        }
        FormEdit::RemoveEmoteSet { set } => {
            if *set >= doc.emote_sets().len() {
                return false;
            }
            doc.remove_emote_set(*set);
            true
        }
        FormEdit::EmoteSet { set, column, value } => {
            let Some(s) = doc.emote_sets().get(*set).copied() else {
                return false;
            };
            let v = if column == "category" {
                enum_value(value, "EmoteCategory", schema)
            } else {
                cell_value(value)
            };
            v.is_some_and(|v| doc.set_cell(s.emote, column, &v))
        }
        FormEdit::AddAction { set, kind } => {
            let Some(s) = doc.emote_sets().get(*set).copied() else {
                return false;
            };
            let Some(k) = enum_value(kind, "EmoteType", schema) else {
                return false;
            };
            let n = doc.actions(&s).len();
            doc.add_action(
                *set,
                n,
                &[("type", k), ("delay", "0".into()), ("extent", "1".into())],
            );
            true
        }
        FormEdit::RemoveAction { set, action } => {
            let Some(s) = doc.emote_sets().get(*set).copied() else {
                return false;
            };
            if *action >= doc.actions(&s).len() {
                return false;
            }
            doc.remove_action(*set, *action);
            true
        }
        FormEdit::MoveAction { set, from, to } => {
            let Some(s) = doc.emote_sets().get(*set).copied() else {
                return false;
            };
            let n = doc.actions(&s).len();
            if *from >= n || *to >= n {
                return false;
            }
            doc.move_action(*set, *from, *to);
            true
        }
        FormEdit::RegisterQuest { .. } | FormEdit::Quest { .. } => false,
        FormEdit::Says { category, text } => {
            let quest = (*category != 7)
                .then(|| super::flows::quest_of(doc))
                .flatten();
            super::flows::set_says(doc, *category, quest.as_deref(), text);
            true
        }
        FormEdit::QuestGiver { quest } => {
            if quest.trim().is_empty() {
                return false;
            }
            super::flows::set_quest(doc, quest.trim());
            true
        }
        FormEdit::WeenieType(t) => doc
            .rows("weenie")
            .first()
            .copied()
            .is_some_and(|r| doc.set_cell(r, "type", &t.to_string())),
        FormEdit::Destination { cell, at } => {
            super::dungeons::set_destination(doc, *cell, Vec3::new(at[0], at[1], at[2]));
            true
        }
        FormEdit::Action {
            set,
            action,
            column,
            value,
        } => {
            let Some(s) = doc.emote_sets().get(*set).copied() else {
                return false;
            };
            let Some(r) = doc.actions(&s).get(*action).copied() else {
                return false;
            };
            let v = if column == "type" {
                enum_value(value, "EmoteType", schema)
            } else {
                cell_value(value)
            };
            v.is_some_and(|v| doc.set_cell(r, column, &v))
        }
    }
}

/// A property type's heading in the form: its key in the text.
fn kind_name(k: PropertyKind) -> &'static str {
    match k {
        PropertyKind::Int => "weenie.kind_int",
        PropertyKind::Int64 => "weenie.kind_int64",
        PropertyKind::Bool => "weenie.kind_bool",
        PropertyKind::Float => "weenie.kind_float",
        PropertyKind::String => "weenie.kind_string",
        PropertyKind::DataId => "weenie.kind_data_id",
        PropertyKind::InstanceId => "weenie.kind_instance_id",
    }
}

impl EditorApp {
    /// The weenie the form shows: the selected spawn's, else the spawn browser's choice.
    #[must_use]
    pub fn form_weenie(&self) -> Option<Weenie> {
        if let Some(s) = self.selected_spawn() {
            return self.session.spawn_weenie(&s);
        }
        self.panels.weenie.clone()
    }

    /// Make `edit` to the form's weenie's SQL (or a quest's, for a quest's edit), as one step of
    /// the history.
    ///
    /// # Errors
    /// No weenie, one no SQL file of the pack being edited writes, or a value that is not one.
    pub fn form_edit(&mut self, edit: &FormEdit) -> Result<(), String> {
        self.form_edits(std::slice::from_ref(edit))
    }

    /// Make `edits` to the form's weenie's SQL, in order, as one step of the history (a quest's
    /// edit alone).
    ///
    /// # Errors
    /// As [`Self::form_edit`]: the first edit refused stops them all.
    pub fn form_edits(&mut self, edits: &[FormEdit]) -> Result<(), String> {
        let edit = match edits {
            [one] => one,
            _ => return self.weenie_edits(edits),
        };
        match edit {
            FormEdit::RegisterQuest { name } => return self.register_quest(name),
            FormEdit::Quest {
                name,
                column,
                value,
            } => return self.quest_edit(name, column, value),
            _ => {}
        }
        let w = self
            .form_weenie()
            .ok_or_else(|| self.say("messages.no_weenie_shown"))?;
        let (file, mine) = self
            .session
            .weenie_file(&w)
            .ok_or_else(|| self.say("messages.no_sql_writes_it"))?;
        if !mine {
            return Err(self.say("messages.another_packs_weenie"));
        }
        let text = self
            .session
            .sql_text(&file)
            .ok_or_else(|| self.say("messages.sql_gone"))?;
        let mut doc = SqlDoc::parse(&text);
        if !apply_edit(&mut doc, edit, self.schema.as_ref()) {
            return Err(self.say("messages.not_a_value"));
        }
        let new = doc.render();
        if new != text {
            self.apply(
                &Op::WriteSql {
                    file,
                    text: Some(new),
                },
                false,
            )?;
        }
        Ok(())
    }

    /// `edits` of the weenie's own SQL (no quest's) made together.
    fn weenie_edits(&mut self, edits: &[FormEdit]) -> Result<(), String> {
        let w = self
            .form_weenie()
            .ok_or_else(|| self.say("messages.no_weenie_shown"))?;
        let (file, mine) = self
            .session
            .weenie_file(&w)
            .ok_or_else(|| self.say("messages.no_sql_writes_it"))?;
        if !mine {
            return Err(self.say("messages.another_packs_weenie"));
        }
        let text = self
            .session
            .sql_text(&file)
            .ok_or_else(|| self.say("messages.sql_gone"))?;
        let mut doc = SqlDoc::parse(&text);
        for edit in edits {
            if !apply_edit(&mut doc, edit, self.schema.as_ref()) {
                return Err(self.say("messages.not_a_value"));
            }
        }
        let new = doc.render();
        if new != text {
            self.apply(
                &Op::WriteSql {
                    file,
                    text: Some(new),
                },
                false,
            )?;
        }
        Ok(())
    }

    /// Register quest `name` in the pack being edited, unless a pack does.
    fn register_quest(&mut self, name: &str) -> Result<(), String> {
        if matches!(self.session.quest(name), Some((_, Some(_)))) {
            return Ok(());
        }
        let q = lyceum_foundry::sql_ids::Quest {
            min_delta: 60,
            max_solves: -1,
            message: String::new(),
        };
        let file = self.session.quest_file(name);
        let text = self.session.quest_text(name, &q);
        self.apply(
            &Op::WriteSql {
                file,
                text: Some(text),
            },
            false,
        )?;
        Ok(())
    }

    /// Column `column` of the pack's quest `name` set to `value`.
    fn quest_edit(&mut self, name: &str, column: &str, value: &str) -> Result<(), String> {
        let Some((_, Some(file))) = self.session.quest(name) else {
            return Err(self.say_with("messages.not_packs_quest", &[("name", &name)]));
        };
        let text = self
            .session
            .sql_text(&file)
            .ok_or_else(|| self.say("messages.sql_gone"))?;
        let mut doc = SqlDoc::parse(&text);
        let key = lyceum_foundry::content_check::quest_key(name);
        let row = doc
            .rows("quest")
            .into_iter()
            .find(|r| {
                doc.cell(*r, "name").is_some_and(|n| {
                    lyceum_foundry::content_check::quest_key(&content::unquote(&n)) == key
                })
            })
            .ok_or_else(|| self.say("messages.quest_row_gone"))?;
        let v = match column {
            "message" => Some(content::quote(&content::unquote(value))),
            _ => value.trim().parse::<i64>().ok().map(|n| n.to_string()),
        }
        .ok_or_else(|| self.say("messages.not_a_value"))?;
        if !doc.set_cell(row, column, &v) {
            return Err(self.say_with("messages.quest_has_no", &[("column", &column)]));
        }
        let new = doc.render();
        if new != text {
            self.apply(
                &Op::WriteSql {
                    file,
                    text: Some(new),
                },
                false,
            )?;
        }
        Ok(())
    }

    /// Copy server class `class` into the pack being edited, by its own class id: the pack's
    /// copy takes the class's place, and is edited from then on.
    ///
    /// # Errors
    /// No server database was read, it has no such class, or the file would not write.
    pub fn copy_class(&mut self, class: u32) -> Result<(), String> {
        let path = self
            .server_sql
            .clone()
            .ok_or_else(|| self.say("messages.no_server_database"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let world = String::from_utf8_lossy(&bytes);
        let text = content::override_of(&world, class)
            .ok_or_else(|| self.say_with("messages.no_such_class", &[("id", &class)]))?;
        let name = self
            .session
            .class_facts(class)
            .and_then(|f| f.name)
            .unwrap_or_else(|| format!("class {class}"));
        let file = self.session.class_file(class, &name);
        let header = format!(
            "/* {name}: the server's class {class}, written again by this pack, which it takes \
             the place of. */\n"
        );
        self.apply(
            &Op::WriteSql {
                file,
                text: Some(header + &text),
            },
            false,
        )?;
        Ok(())
    }

    /// A new weenie of the pack being edited, symbol `name`, made from server class `from`
    /// (`blank`: stripped to a blank of its kind); the spawn tool and the form take it.
    ///
    /// # Errors
    /// No server database, no such class, a name that is not a symbol, or one taken.
    pub fn new_weenie(&mut self, name: &str, from: u32, blank: bool) -> Result<(), String> {
        let (file, sym) = self
            .session
            .new_weenie_file(name.trim())
            .map_err(|e| e.to_string())?;
        if self.session.sql_text(&file).is_some()
            || self
                .weenies
                .iter()
                .any(|w| w.weenie == Weenie::Symbol(sym.clone()))
        {
            return Err(self.say_with("messages.name_taken", &[("name", &sym)]));
        }
        let path = self
            .server_sql
            .clone()
            .ok_or_else(|| self.say("messages.no_server_database"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let world = String::from_utf8_lossy(&bytes);
        let text = content::template(&world, from, name.trim(), blank)
            .ok_or_else(|| self.say_with("messages.no_such_class", &[("id", &from)]))?;
        let kind = if blank { "a blank" } else { "a copy" };
        let header = format!("/* {name}: made from {kind} of the server's class {from}. */\n");
        self.apply(
            &Op::WriteSql {
                file,
                text: Some(header + &text),
            },
            false,
        )?;
        self.panels.weenie = Some(Weenie::Symbol(sym));
        Ok(())
    }

    /// The name of spell `id` in the files' spell table, read the first time.
    fn spell_name(&mut self, id: u32) -> Option<String> {
        if self.spells.is_none() {
            use dereth_assets::Decode;
            let table = DataId(0x0E00_000E);
            self.spells = Some(
                self.session
                    .base()
                    .read_portal(table)
                    .and_then(|b| dereth_assets::tables::SpellTable::decode_payload(table, &b).ok())
                    .map(|t| t.spells.into_iter().map(|(k, v)| (k, v.name)).collect())
                    .unwrap_or_default(),
            );
        }
        self.spells.as_ref()?.get(&id).cloned()
    }

    /// The name of weenie class `w` as written (a class id or a symbol's token).
    pub(super) fn class_name(&self, w: &str) -> Option<String> {
        let w = w.trim();
        if let Some(c) = lyceum_foundry::sql_ids::number(w) {
            return self.session.class_facts(c).and_then(|f| f.name);
        }
        let sym = w.strip_prefix("{{wcid:")?.strip_suffix("}}")?;
        self.weenies
            .iter()
            .find(|e| matches!(&e.weenie, Weenie::Symbol(s) if s.ends_with(&format!(":{sym}")) || s == sym))
            .map(|e| e.name.clone())
    }

    /// A field of the form: the text as typed while it is being typed, else `now`; a change is
    /// pushed as `edit` of what was typed when the field is left.
    pub(super) fn field(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        now: &str,
        width: f32,
        edit: impl FnOnce(String) -> FormEdit,
        out: &mut Vec<UiAction>,
    ) {
        let mut text = self
            .panels
            .form_typing
            .get(key)
            .cloned()
            .unwrap_or_else(|| now.to_owned());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width));
        self.form_fields.push((key.to_owned(), r.rect));
        if r.has_focus() || r.changed() {
            self.panels.form_typing.insert(key.to_owned(), text.clone());
        }
        if r.lost_focus() {
            self.panels.form_typing.remove(key);
            if text.trim() != now.trim() {
                out.push(UiAction::Form(edit(text)));
            }
        }
    }

    /// The weenie tab.
    pub(super) fn weenie_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        self.form_fields.clear();
        self.new_flows(ui, out);
        let Some(w) = self.form_weenie() else {
            ui.weak(t.get("weenie.nothing"));
            return;
        };
        let file = self.session.weenie_file(&w);
        let entry = self.weenies.iter().find(|e| e.weenie == w).cloned();
        // The head: its picture, its name, where it is written.
        ui.horizontal(|ui| {
            let mut budget = super::PICTURES_A_FRAME;
            if let Some(id) = entry
                .as_ref()
                .and_then(|e| e.setup)
                .and_then(|s| self.picture(DataId(s), &mut budget))
            {
                ui.add(egui::Image::new((id, egui::vec2(48.0, 48.0))));
            }
            ui.vertical(|ui| {
                ui.strong(
                    entry
                        .as_ref()
                        .map_or(t.get("weenie.a_weenie"), |e| e.name.as_str()),
                );
                ui.weak(match &w {
                    Weenie::Symbol(s) => s.clone(),
                    Weenie::Class(c) => t.fill("weenie.class", &[("id", c)]),
                });
                if let Some(e) = &entry {
                    let c = e.category.name();
                    ui.weak(t.or(&format!("categories.{c}"), c));
                }
            });
        });
        self.new_weenie_section(ui, &w, out);
        let (path, mine) = match (&w, file) {
            (_, Some(f)) => f,
            (Weenie::Class(c), None) => {
                ui.label(t.get("weenie.servers_own"));
                if self.server_sql.is_some() {
                    if ui.button(t.get("weenie.copy_into_pack")).clicked() {
                        out.push(UiAction::CopyClass(*c));
                    }
                } else {
                    ui.weak(t.get("weenie.cannot_copy"));
                }
                return;
            }
            (Weenie::Symbol(_), None) => {
                ui.weak(t.get("weenie.no_sql"));
                return;
            }
        };
        ui.weak(
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        if !mine {
            ui.weak(t.get("weenie.another_pack"));
        }
        let Some(text) = self.session.sql_text(&path) else {
            ui.weak(t.get("weenie.sql_gone"));
            return;
        };
        let doc = SqlDoc::parse(&text);
        ui.add_enabled_ui(mine, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("weenie form")
                .auto_shrink(false)
                .show(ui, |ui| {
                    self.flow_form(ui, &w, &doc, out);
                    let advanced = egui::CollapsingHeader::new(t.get("weenie.advanced"))
                        .id_salt("advanced")
                        .open(Some(self.panels.advanced))
                        .show(ui, |ui| self.weenie_form(ui, &doc, out));
                    if advanced.header_response.clicked() {
                        self.panels.advanced = !self.panels.advanced;
                    }
                });
        });
    }

    /// Making a new weenie: from a blank of a kind, or a copy of the server class shown.
    fn new_weenie_section(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        if self.server_sql.is_none() {
            return;
        }
        let t = self.text.table();
        egui::CollapsingHeader::new(t.get("weenie.new"))
            .id_salt("new weenie")
            .show(ui, |ui| {
                let mut name = self
                    .panels
                    .form_typing
                    .get("new:name")
                    .cloned()
                    .unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text(t.get("weenie.new_name_hint"))
                        .desired_width(200.0),
                );
                self.panels
                    .form_typing
                    .insert("new:name".into(), name.clone());
                ui.horizontal_wrapped(|ui| {
                    ui.weak(t.get("weenie.a_blank"));
                    for b in content::BLANKS {
                        if ui.small_button(b.kind).clicked() && !name.trim().is_empty() {
                            out.push(UiAction::NewWeenie(name.clone(), b.class, true));
                        }
                    }
                });
                if let Weenie::Class(c) = w {
                    if ui.button(t.get("weenie.a_copy")).clicked() && !name.trim().is_empty() {
                        out.push(UiAction::NewWeenie(name.clone(), *c, false));
                    }
                }
            });
    }

    /// The form's sections over `doc`.
    fn weenie_form(&mut self, ui: &mut egui::Ui, doc: &SqlDoc, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let schema = self.schema.clone();
        let prop_name = |k: PropertyKind, id: u32| {
            schema
                .as_ref()
                .and_then(|s| s.property(k, id))
                .map_or_else(|| format!("#{id}"), str::to_owned)
        };
        // Its name first, then every property by type.
        let name = doc
            .property(PropertyKind::String, 1)
            .map(|v| content::unquote(&v))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(t.get("weenie.name"));
            self.field(
                ui,
                "string:1",
                &name,
                180.0,
                |v| FormEdit::Property {
                    kind: PropertyKind::String,
                    id: 1,
                    value: Some(v),
                },
                out,
            );
        });
        for kind in PropertyKind::ALL {
            let props = doc.properties(kind);
            egui::CollapsingHeader::new(format!("{} ({})", t.get(kind_name(kind)), props.len()))
                .id_salt(kind.key())
                .default_open(matches!(kind, PropertyKind::Int | PropertyKind::DataId))
                .show(ui, |ui| {
                    egui::Grid::new(("props", kind.key()))
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            for (id, value) in &props {
                                let shown = match kind {
                                    PropertyKind::String => content::unquote(value),
                                    _ => value.clone(),
                                };
                                ui.label(prop_name(kind, *id)).on_hover_text(t.fill(
                                    "weenie.property_tip",
                                    &[("kind", &kind.key()), ("id", id)],
                                ));
                                let key = format!("{}:{id}", kind.key());
                                let (k, i) = (kind, *id);
                                self.field(
                                    ui,
                                    &key,
                                    &shown,
                                    110.0,
                                    move |v| FormEdit::Property {
                                        kind: k,
                                        id: i,
                                        value: Some(v),
                                    },
                                    out,
                                );
                                if ui
                                    .small_button("x")
                                    .on_hover_text(t.get("weenie.take_away"))
                                    .clicked()
                                {
                                    out.push(UiAction::Form(FormEdit::Property {
                                        kind,
                                        id: *id,
                                        value: None,
                                    }));
                                }
                                ui.end_row();
                            }
                        });
                    // A property added by name or number.
                    ui.horizontal(|ui| {
                        let key = format!("add:{}", kind.key());
                        let mut typed = self
                            .panels
                            .form_typing
                            .get(&key)
                            .cloned()
                            .unwrap_or_default();
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut typed)
                                .hint_text(t.get("weenie.add_property_hint"))
                                .desired_width(200.0),
                        );
                        self.panels.form_typing.insert(key.clone(), typed.clone());
                        let entered =
                            r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if entered {
                            if let Some((n, v)) = typed.split_once('=') {
                                let n = n.trim();
                                let id = n.parse::<u32>().ok().or_else(|| {
                                    schema
                                        .as_ref()
                                        .and_then(|s| s.property_named(n))
                                        .filter(|(k, _)| *k == kind)
                                        .map(|(_, id)| id)
                                });
                                match id {
                                    Some(id) => {
                                        out.push(UiAction::Form(FormEdit::Property {
                                            kind,
                                            id,
                                            value: Some(v.trim().to_owned()),
                                        }));
                                        self.panels.form_typing.remove(&key);
                                    }
                                    None => {
                                        self.panels.message = t.fill(
                                            "messages.not_a_property",
                                            &[("name", &n), ("kind", &kind.key())],
                                        );
                                    }
                                }
                            }
                        }
                    });
                });
        }
        self.emotes_section(ui, doc, out);
        self.quests_section(ui, doc, out);
        self.treasure_section(ui, doc, out);
        self.rows_section(
            ui,
            doc,
            "weenie.spells",
            "weenie_properties_spell_book",
            &["spell", "probability"],
            "spell",
            out,
        );
        self.rows_section(
            ui,
            doc,
            "weenie.carries",
            "weenie_properties_create_list",
            &["weenie_Class_Id", "destination_Type", "stack_Size", "shade"],
            "weenie_Class_Id",
            out,
        );
        self.rows_section(
            ui,
            doc,
            "weenie.generates",
            "weenie_properties_generator",
            &[
                "weenie_Class_Id",
                "probability",
                "init_Create",
                "max_Create",
                "delay",
                "where_Create",
            ],
            "weenie_Class_Id",
            out,
        );
    }

    /// The emotes: each set (what calls it, how likely, its quest) and its actions in order, each
    /// action's fields as the type uses them, moved up and down, added and taken away.
    fn emotes_section(&mut self, ui: &mut egui::Ui, doc: &SqlDoc, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let schema = self.schema.clone();
        let named = |e: &str, v: &str| -> String {
            let n = v.trim().parse::<u32>().ok();
            match n.and_then(|n| schema.as_ref().and_then(|s| s.value(e, n))) {
                Some(w) => w.to_owned(),
                None => v.trim().to_owned(),
            }
        };
        let sets = doc.emote_sets();
        egui::CollapsingHeader::new(t.fill("weenie.emotes", &[("n", &sets.len())]))
            .id_salt("emotes")
            .show(ui, |ui| {
                for (i, set) in sets.iter().enumerate() {
                    let category = doc.cell(set.emote, "category").unwrap_or_default();
                    let actions = doc.actions(set);
                    let head = t.fill(
                        "weenie.emote_set",
                        &[
                            ("calls", &named("EmoteCategory", &category)),
                            ("n", &actions.len()),
                        ],
                    );
                    egui::CollapsingHeader::new(head)
                        .id_salt(("emote set", i))
                        .show(ui, |ui| {
                            egui::Grid::new(("set", i)).num_columns(2).show(ui, |ui| {
                                for c in ["category", "probability", "quest", "weenie_Class_Id"] {
                                    ui.weak(c);
                                    let now = doc.cell(set.emote, c).unwrap_or_default();
                                    let now = if c == "category" {
                                        named("EmoteCategory", &now)
                                    } else {
                                        now
                                    };
                                    let col = c.to_owned();
                                    self.field(
                                        ui,
                                        &format!("emote:{i}:{c}"),
                                        &now,
                                        150.0,
                                        move |v| FormEdit::EmoteSet {
                                            set: i,
                                            column: col,
                                            value: v,
                                        },
                                        out,
                                    );
                                    ui.end_row();
                                }
                            });
                            for (n, a) in actions.iter().enumerate() {
                                let kind = doc.cell(*a, "type").unwrap_or_default();
                                ui.horizontal(|ui| {
                                    ui.label(format!("{n}. {}", named("EmoteType", &kind)));
                                    if ui
                                        .small_button("^")
                                        .on_hover_text(t.get("weenie.earlier"))
                                        .clicked()
                                        && n > 0
                                    {
                                        out.push(UiAction::Form(FormEdit::MoveAction {
                                            set: i,
                                            from: n,
                                            to: n - 1,
                                        }));
                                    }
                                    if ui
                                        .small_button("v")
                                        .on_hover_text(t.get("weenie.later"))
                                        .clicked()
                                        && n + 1 < actions.len()
                                    {
                                        out.push(UiAction::Form(FormEdit::MoveAction {
                                            set: i,
                                            from: n,
                                            to: n + 1,
                                        }));
                                    }
                                    if ui
                                        .small_button("x")
                                        .on_hover_text(t.get("weenie.take_away"))
                                        .clicked()
                                    {
                                        out.push(UiAction::Form(FormEdit::RemoveAction {
                                            set: i,
                                            action: n,
                                        }));
                                    }
                                });
                                // The fields the action has, and its type and delay.
                                let mut cols: Vec<&str> = vec!["type", "delay"];
                                for c in content::ACTION_COLUMNS.iter().skip(2) {
                                    if !cols.contains(c)
                                        && doc.cell(*a, c).is_some_and(|v| v.trim() != "NULL")
                                    {
                                        cols.push(c);
                                    }
                                }
                                egui::Grid::new(("action", i, n))
                                    .num_columns(2)
                                    .show(ui, |ui| {
                                        for c in cols {
                                            ui.weak(c);
                                            let now = doc.cell(*a, c).unwrap_or_default();
                                            let now = match c {
                                                "type" => named("EmoteType", &now),
                                                "message" | "test_String" => content::unquote(&now),
                                                _ => now,
                                            };
                                            let col = c.to_owned();
                                            self.field(
                                                ui,
                                                &format!("action:{i}:{n}:{c}"),
                                                &now,
                                                if c == "message" { 220.0 } else { 90.0 },
                                                move |v| FormEdit::Action {
                                                    set: i,
                                                    action: n,
                                                    column: col,
                                                    value: v,
                                                },
                                                out,
                                            );
                                            ui.end_row();
                                        }
                                    });
                                // Any other field, by its column's name.
                                let key = format!("action:{i}:{n}:+");
                                let mut typed = self
                                    .panels
                                    .form_typing
                                    .get(&key)
                                    .cloned()
                                    .unwrap_or_default();
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut typed)
                                        .hint_text(t.get("weenie.set_column_hint"))
                                        .desired_width(200.0),
                                );
                                self.panels.form_typing.insert(key.clone(), typed.clone());
                                if r.lost_focus() && ui.input(|x| x.key_pressed(egui::Key::Enter)) {
                                    if let Some((c, v)) = typed.split_once('=') {
                                        let c = c.trim();
                                        if content::ACTION_COLUMNS.contains(&c) {
                                            out.push(UiAction::Form(FormEdit::Action {
                                                set: i,
                                                action: n,
                                                column: c.to_owned(),
                                                value: v.trim().to_owned(),
                                            }));
                                            self.panels.form_typing.remove(&key);
                                        } else {
                                            self.panels.message =
                                                t.fill("messages.action_has_no", &[("column", &c)]);
                                        }
                                    }
                                }
                                ui.separator();
                            }
                            ui.horizontal(|ui| {
                                let key = format!("emote:{i}:add");
                                let mut typed = self
                                    .panels
                                    .form_typing
                                    .get(&key)
                                    .cloned()
                                    .unwrap_or_default();
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut typed)
                                        .hint_text(t.get("weenie.add_action_hint"))
                                        .desired_width(160.0),
                                );
                                self.panels.form_typing.insert(key.clone(), typed.clone());
                                if r.lost_focus()
                                    && ui.input(|x| x.key_pressed(egui::Key::Enter))
                                    && !typed.trim().is_empty()
                                {
                                    out.push(UiAction::Form(FormEdit::AddAction {
                                        set: i,
                                        kind: typed.trim().to_owned(),
                                    }));
                                    self.panels.form_typing.remove(&key);
                                }
                                if ui.small_button(t.get("weenie.remove_set")).clicked() {
                                    out.push(UiAction::Form(FormEdit::RemoveEmoteSet { set: i }));
                                }
                            });
                        });
                }
                let key = "emote:add".to_owned();
                let mut typed = self
                    .panels
                    .form_typing
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut typed)
                        .hint_text(t.get("weenie.add_set_hint"))
                        .desired_width(200.0),
                );
                self.panels.form_typing.insert(key.clone(), typed.clone());
                if r.lost_focus()
                    && ui.input(|x| x.key_pressed(egui::Key::Enter))
                    && !typed.trim().is_empty()
                {
                    out.push(UiAction::Form(FormEdit::AddEmoteSet {
                        category: typed.trim().to_owned(),
                    }));
                    self.panels.form_typing.remove(&key);
                }
            });
    }

    /// The quests the emotes name: where each is registered, the pack's own editable, and one
    /// not registered registered in the pack.
    fn quests_section(&mut self, ui: &mut egui::Ui, doc: &SqlDoc, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let names = quests_named(doc);
        egui::CollapsingHeader::new(t.fill("weenie.quests", &[("n", &names.len())]))
            .id_salt("quests")
            .show(ui, |ui| {
                for name in names {
                    ui.horizontal(|ui| {
                        ui.strong(&name);
                        match self.session.quest(&name) {
                            None => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(0xF0, 0xC0, 0x60),
                                    t.get("weenie.quest_not_registered"),
                                );
                                if ui.small_button(t.get("weenie.register_quest")).clicked() {
                                    out.push(UiAction::Form(FormEdit::RegisterQuest {
                                        name: name.clone(),
                                    }));
                                }
                            }
                            Some((q, None)) => {
                                ui.weak(t.fill(
                                    "weenie.quest_servers",
                                    &[
                                        ("every", &q.min_delta),
                                        (
                                            "times",
                                            &if q.max_solves < 0 {
                                                t.get("weenie.quest_any").to_owned()
                                            } else {
                                                q.max_solves.to_string()
                                            },
                                        ),
                                    ],
                                ));
                            }
                            Some((q, Some(_))) => {
                                ui.weak(t.get("weenie.quest_packs"));
                                for (c, now) in [
                                    ("min_Delta", q.min_delta.to_string()),
                                    ("max_Solves", q.max_solves.to_string()),
                                    ("message", q.message.clone()),
                                ] {
                                    ui.weak(c);
                                    let (n, col) = (name.clone(), c.to_owned());
                                    self.field(
                                        ui,
                                        &format!("quest:{name}:{c}"),
                                        &now,
                                        if c == "message" { 140.0 } else { 60.0 },
                                        move |v| FormEdit::Quest {
                                            name: n,
                                            column: col,
                                            value: v,
                                        },
                                        out,
                                    );
                                }
                            }
                        }
                    });
                }
            });
    }

    /// What it drops when it dies: its death treasure (data id 35) and what that treasure is.
    fn treasure_section(&mut self, ui: &mut egui::Ui, doc: &SqlDoc, out: &mut Vec<UiAction>) {
        let now = doc
            .property(PropertyKind::DataId, 35)
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        let tx = self.text.table();
        egui::CollapsingHeader::new(tx.get("weenie.treasure"))
            .id_salt("treasure")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tx.get("weenie.death_treasure"));
                    self.field(
                        ui,
                        "treasure",
                        &now.map(|n| n.to_string()).unwrap_or_default(),
                        60.0,
                        |v| FormEdit::Property {
                            kind: PropertyKind::DataId,
                            id: 35,
                            value: (!v.trim().is_empty()).then(|| v.trim().to_owned()),
                        },
                        out,
                    );
                });
                match now.map(|n| (n, self.session.treasure(n).cloned())) {
                    None => {
                        ui.weak(tx.get("weenie.treasure_none"));
                    }
                    Some((n, None)) => {
                        ui.weak(tx.fill("weenie.treasure_unknown", &[("n", &n)]));
                    }
                    Some((_, Some(t))) => {
                        let kind = |(c, lo, hi): (i32, i32, i32)| {
                            tx.fill(
                                "weenie.treasure_chance",
                                &[("chance", &c), ("lo", &lo), ("hi", &hi)],
                            )
                        };
                        ui.weak(tx.fill(
                            "weenie.treasure_tier",
                            &[("tier", &t.tier), ("quality", &t.quality)],
                        ));
                        ui.weak(tx.fill("weenie.treasure_items", &[("chance", &kind(t.items))]));
                        ui.weak(tx.fill("weenie.treasure_magic", &[("chance", &kind(t.magic))]));
                        ui.weak(
                            tx.fill("weenie.treasure_mundane", &[("chance", &kind(t.mundane))]),
                        );
                    }
                }
            });
    }

    /// A section of a table's rows: each row's `shown` columns as fields (the first named), a
    /// button to take it away, and a field that adds a row by its `key` column.
    #[allow(clippy::too_many_arguments)]
    fn rows_section(
        &mut self,
        ui: &mut egui::Ui,
        doc: &SqlDoc,
        heading: &str,
        table: &str,
        shown: &[&str],
        key: &str,
        out: &mut Vec<UiAction>,
    ) {
        let tx = self.text.table();
        let rows = doc.rows(table);
        egui::CollapsingHeader::new(format!("{} ({})", tx.get(heading), rows.len()))
            .id_salt(table)
            .show(ui, |ui| {
                egui::Grid::new(("rows", table))
                    .num_columns(shown.len() + 2)
                    .striped(true)
                    .show(ui, |ui| {
                        for c in shown {
                            ui.weak(*c);
                        }
                        ui.label("");
                        ui.label("");
                        ui.end_row();
                        for (n, r) in rows.iter().enumerate() {
                            for c in shown {
                                let now = doc.cell(*r, c).unwrap_or_default();
                                let (t, col) = (table.to_owned(), (*c).to_owned());
                                self.field(
                                    ui,
                                    &format!("{table}:{n}:{c}"),
                                    &now,
                                    if *c == key { 90.0 } else { 50.0 },
                                    move |v| FormEdit::Cell {
                                        table: t,
                                        row: n,
                                        column: col,
                                        value: v,
                                    },
                                    out,
                                );
                            }
                            let v = doc.cell(*r, key).unwrap_or_default();
                            let named = if key == "spell" {
                                v.trim()
                                    .parse::<u32>()
                                    .ok()
                                    .and_then(|s| self.spell_name(s))
                            } else {
                                self.class_name(&v)
                            };
                            ui.weak(named.unwrap_or_default());
                            if ui
                                .small_button("x")
                                .on_hover_text(tx.get("weenie.take_away"))
                                .clicked()
                            {
                                out.push(UiAction::Form(FormEdit::RemoveRow {
                                    table: table.to_owned(),
                                    row: n,
                                }));
                            }
                            ui.end_row();
                        }
                    });
                let k = format!("add:{table}");
                let mut typed = self.panels.form_typing.get(&k).cloned().unwrap_or_default();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut typed)
                        .hint_text(tx.fill("weenie.add_row_hint", &[("column", &key)]))
                        .desired_width(200.0),
                );
                self.panels.form_typing.insert(k.clone(), typed.clone());
                if r.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !typed.trim().is_empty()
                {
                    out.push(UiAction::Form(FormEdit::AddRow {
                        table: table.to_owned(),
                        values: vec![(key.to_owned(), typed.trim().to_owned())],
                    }));
                    self.panels.form_typing.remove(&k);
                }
            });
    }
}
