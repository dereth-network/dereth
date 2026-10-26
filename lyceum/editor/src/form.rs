//! The weenie editor's form: a weenie's SQL shown as fields and written back to it. A pack's own
//! weenie is edited in place; a server class is copied into the pack (by its own class id, so the
//! pack's copy takes its place) to be edited. Each change names the weenie it is made to, and is
//! one step of the history.

use std::collections::BTreeMap;
use std::path::PathBuf;

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
    /// actions: right after set `after` when there is one, else after the last.
    AddEmoteSet {
        category: String,
        after: Option<usize>,
    },
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
    /// Nothing changed, for this reason: what was asked cannot be had.
    Refused(String),
    /// Every row of `table` written as `rows`, in order.
    Rows {
        table: String,
        rows: Vec<super::collections::TableRow>,
    },
    /// The probabilities of emote sets, each by its place, as written.
    Probabilities(Vec<(usize, String)>),
    /// Every emote set and its actions written as these.
    Emotes(Vec<super::emotes::EmoteRows>),
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
pub(super) fn columns(table: &str) -> &'static [&'static str] {
    match table {
        "weenie_properties_attribute" => &[
            "object_Id",
            "type",
            "init_Level",
            "level_From_C_P",
            "c_P_Spent",
        ],
        "weenie_properties_attribute_2nd" => &[
            "object_Id",
            "type",
            "init_Level",
            "level_From_C_P",
            "c_P_Spent",
            "current_Level",
        ],
        "weenie_properties_skill" => &[
            "object_Id",
            "type",
            "level_From_P_P",
            "s_a_c",
            "p_p",
            "init_Level",
            "resistance_At_Last_Check",
            "last_Used_Time",
        ],
        "weenie_properties_body_part" => &[
            "object_Id",
            "key",
            "d_Type",
            "d_Val",
            "d_Var",
            "base_Armor",
            "armor_Vs_Slash",
            "armor_Vs_Pierce",
            "armor_Vs_Bludgeon",
            "armor_Vs_Cold",
            "armor_Vs_Fire",
            "armor_Vs_Acid",
            "armor_Vs_Electric",
            "armor_Vs_Nether",
            "b_h",
            "h_l_f",
            "m_l_f",
            "l_l_f",
            "h_r_f",
            "m_r_f",
            "l_r_f",
            "h_l_b",
            "m_l_b",
            "l_l_b",
            "h_r_b",
            "m_r_b",
            "l_r_b",
        ],
        "weenie_properties_position" => &[
            "object_Id",
            "position_Type",
            "obj_Cell_Id",
            "origin_X",
            "origin_Y",
            "origin_Z",
            "angles_W",
            "angles_X",
            "angles_Y",
            "angles_Z",
        ],
        "weenie_properties_event_filter" => &["object_Id", "event"],
        "weenie_properties_book" => &["object_Id", "max_Num_Pages", "max_Num_Chars_Per_Page"],
        "weenie_properties_book_page_data" => &[
            "object_Id",
            "page_Id",
            "author_Id",
            "author_Name",
            "author_Account",
            "ignore_Author",
            "page_Text",
        ],
        "weenie_properties_palette" => &["object_Id", "sub_Palette_Id", "offset", "length"],
        "weenie_properties_texture_map" => &["object_Id", "index", "old_Id", "new_Id"],
        "weenie_properties_anim_part" => &["object_Id", "index", "animation_Id"],
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
        ("weenie_properties_generator", "when_Create") => "1",
        ("weenie_properties_generator", "where_Create") => "2",
        ("weenie_properties_position", "angles_W") => "1",
        ("weenie_properties_book", "max_Num_Pages") => "1",
        ("weenie_properties_book", "max_Num_Chars_Per_Page") => "1000",
        ("weenie_properties_book_page_data", "author_Id") => "4294967295",
        ("weenie_properties_book_page_data", "author_Name" | "author_Account" | "page_Text") => {
            "''"
        }
        ("weenie_properties_book_page_data", "ignore_Author") => "False",
        (
            "weenie_properties_attribute"
            | "weenie_properties_attribute_2nd"
            | "weenie_properties_skill"
            | "weenie_properties_body_part"
            | "weenie_properties_position"
            | "weenie_properties_event_filter"
            | "weenie_properties_book_page_data"
            | "weenie_properties_palette"
            | "weenie_properties_texture_map"
            | "weenie_properties_anim_part",
            _,
        ) => "0",
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
        FormEdit::AddEmoteSet { category, after } => {
            let Some(c) = enum_value(category, "EmoteCategory", schema) else {
                return false;
            };
            doc.add_emote_set_after(*after, &c, &[]);
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
        FormEdit::Refused(_) => false,
        FormEdit::Rows { table, rows } => {
            if columns(table).is_empty() {
                return false;
            }
            super::collections::write_rows(doc, table, rows);
            true
        }
        FormEdit::Probabilities(list) => {
            let sets = doc.emote_sets();
            list.iter().all(|(i, p)| {
                sets.get(*i)
                    .is_some_and(|s| doc.set_cell(s.emote, "probability", p))
            })
        }
        FormEdit::Emotes(sets) => {
            super::emotes::write_emotes(doc, sets);
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

/// The key what is typed into field `key` of weenie `w`'s form is kept by until it is left.
#[must_use]
pub fn typing_key(w: &Weenie, key: &str) -> String {
    match w {
        Weenie::Symbol(s) => format!("{s}|{key}"),
        Weenie::Class(c) => format!("{c}|{key}"),
    }
}

/// How the weenie tab names weenie `w` under its name: a server's class by its id, a pack's own by
/// its name in its pack (its symbol without the pack's name before it).
#[must_use]
pub fn weenie_label(w: &Weenie, t: &crate::text::Table) -> String {
    match w {
        Weenie::Symbol(s) => s.rsplit(':').next().unwrap_or(s).to_owned(),
        Weenie::Class(c) => t.fill("weenie.class", &[("id", c)]),
    }
}

/// A table Advanced lists row by row.
pub(super) struct RawTable {
    /// Its heading's key in the text.
    pub heading: &'static str,
    pub table: &'static str,
    /// The columns each row shows as fields.
    pub shown: &'static [&'static str],
    /// The column whose spell or weenie names each row, when one does.
    pub named: Option<&'static str>,
    /// The columns a new row is typed with, each with its box's words' key in the text; none:
    /// every column shown, each by its own name.
    pub adds: Option<&'static [(&'static str, &'static str)]>,
}

/// Every column of a position's row but whose it is.
const POSITION: &[&str] = &[
    "position_Type",
    "obj_Cell_Id",
    "origin_X",
    "origin_Y",
    "origin_Z",
    "angles_W",
    "angles_X",
    "angles_Y",
    "angles_Z",
];

/// The tables Advanced lists row by row: the spell book, the create list and the generator,
/// which have editors of their own as well, then those that have none, every column of each.
pub(super) const RAW_TABLES: &[RawTable] = &[
    RawTable {
        heading: "weenie.spells",
        table: "weenie_properties_spell_book",
        shown: &["spell", "probability"],
        named: Some("spell"),
        adds: Some(&[
            ("spell", "weenie.add_spell_hint"),
            ("probability", "weenie.add_probability_hint"),
        ]),
    },
    RawTable {
        heading: "weenie.carries",
        table: "weenie_properties_create_list",
        shown: &["weenie_Class_Id", "destination_Type", "stack_Size", "shade"],
        named: Some("weenie_Class_Id"),
        adds: Some(&[("weenie_Class_Id", "weenie.add_wcid_hint")]),
    },
    RawTable {
        heading: "weenie.generates",
        table: "weenie_properties_generator",
        shown: &[
            "weenie_Class_Id",
            "probability",
            "init_Create",
            "max_Create",
            "delay",
            "where_Create",
        ],
        named: Some("weenie_Class_Id"),
        adds: Some(&[("weenie_Class_Id", "weenie.add_wcid_hint")]),
    },
    RawTable {
        heading: "weenie.positions",
        table: "weenie_properties_position",
        shown: POSITION,
        named: None,
        adds: None,
    },
    RawTable {
        heading: "weenie.event_filters",
        table: "weenie_properties_event_filter",
        shown: &["event"],
        named: None,
        adds: None,
    },
    RawTable {
        heading: "weenie.book",
        table: "weenie_properties_book",
        shown: &["max_Num_Pages", "max_Num_Chars_Per_Page"],
        named: None,
        adds: None,
    },
    RawTable {
        heading: "weenie.book_pages",
        table: "weenie_properties_book_page_data",
        shown: &[
            "page_Id",
            "author_Id",
            "author_Name",
            "author_Account",
            "ignore_Author",
            "page_Text",
        ],
        named: None,
        adds: None,
    },
    RawTable {
        heading: "weenie.palettes",
        table: "weenie_properties_palette",
        shown: &["sub_Palette_Id", "offset", "length"],
        named: None,
        adds: None,
    },
    RawTable {
        heading: "weenie.texture_maps",
        table: "weenie_properties_texture_map",
        shown: &["index", "old_Id", "new_Id"],
        named: None,
        adds: None,
    },
    RawTable {
        heading: "weenie.anim_parts",
        table: "weenie_properties_anim_part",
        shown: &["index", "animation_Id"],
        named: None,
        adds: None,
    },
];

/// Whether `typed` names a weenie or a spell as a row writes it: a number, or a pack weenie's
/// token.
fn names_one(typed: &str) -> bool {
    let t = typed.trim();
    lyceum_foundry::sql_ids::number(t).is_some() || (t.starts_with("{{") && t.ends_with("}}"))
}

/// A property type's heading in the form: its key in the text.
pub(super) fn kind_name(k: PropertyKind) -> &'static str {
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
    /// The weenie the Definition pane shows: the selected spawn's, else the spawn browser's choice.
    #[must_use]
    pub fn form_weenie(&self) -> Option<Weenie> {
        if let Some(s) = self.selected_spawn() {
            return self.session.spawn_weenie(&s);
        }
        self.panels.weenie.clone()
    }

    /// Make `edit` to weenie `w`'s SQL (or a quest's, for a quest's edit), as one step of the
    /// history.
    ///
    /// # Errors
    /// One no SQL file of the pack being edited writes, or a value that is not one.
    pub fn form_edit(&mut self, w: &Weenie, edit: &FormEdit) -> Result<(), String> {
        self.form_edits(w, std::slice::from_ref(edit))
    }

    /// Make `edits` to weenie `w`'s SQL, in order, as one step of the history (a quest's edit
    /// alone).
    ///
    /// # Errors
    /// As [`Self::form_edit`]: the first edit refused stops them all.
    pub fn form_edits(&mut self, w: &Weenie, edits: &[FormEdit]) -> Result<(), String> {
        let edit = match edits {
            [one] => one,
            _ => return self.weenie_edits(w, edits),
        };
        match edit {
            FormEdit::RegisterQuest { name } => return self.register_quest(name, false),
            FormEdit::Refused(why) => return Err(why.clone()),
            FormEdit::Quest {
                name,
                column,
                value,
            } => return self.quest_edit(name, column, value),
            _ => {}
        }
        let (file, text) = self.writable_sql(w)?;
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
    fn weenie_edits(&mut self, w: &Weenie, edits: &[FormEdit]) -> Result<(), String> {
        if let Some(FormEdit::Refused(why)) =
            edits.iter().find(|e| matches!(e, FormEdit::Refused(_)))
        {
            return Err(why.clone());
        }
        let (file, text) = self.writable_sql(w)?;
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

    /// Register quest `name` in the pack being edited, unless a pack does: a step of its own, or
    /// part of the last (`into_last`).
    pub(super) fn register_quest(&mut self, name: &str, into_last: bool) -> Result<(), String> {
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
            into_last,
        )?;
        Ok(())
    }

    /// Column `column` of the pack's quest `name` set to `value`.
    pub(super) fn quest_edit(
        &mut self,
        name: &str,
        column: &str,
        value: &str,
    ) -> Result<(), String> {
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
        if self.session.weenie_file(&Weenie::Class(class)).is_some() {
            return Ok(());
        }
        let (file, text) = self.class_copy(class)?;
        self.apply(
            &Op::WriteSql {
                file,
                text: Some(text),
            },
            false,
        )?;
        Ok(())
    }

    /// The file and text of server class `class` as the pack being edited would write it again:
    /// its rows, under a line saying whose class it is.
    ///
    /// # Errors
    /// No server database was read, or it has no such class.
    fn class_copy(&mut self, class: u32) -> Result<(PathBuf, String), String> {
        let text = self
            .server_class_now(class)?
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
        Ok((file, header + &text))
    }

    /// The SQL an edit of weenie `w`'s form is made to: the pack's own file of it, or for a
    /// server class the pack does not write yet, the copy the pack will write (copy on write).
    ///
    /// # Errors
    /// Another pack writes it, no file writes it, or the server's class cannot be read.
    pub(super) fn writable_sql(&mut self, w: &Weenie) -> Result<(PathBuf, String), String> {
        if let Some((file, mine)) = self.session.weenie_file(w) {
            if !mine {
                return Err(self.say("messages.another_packs_weenie"));
            }
            let text = self
                .session
                .sql_text(&file)
                .ok_or_else(|| self.say("messages.sql_gone"))?;
            return Ok((file, text));
        }
        match w {
            Weenie::Class(c) => self.class_copy(*c),
            Weenie::Symbol(_) => Err(self.say("messages.no_sql_writes_it")),
        }
    }

    /// Server class `class`'s rows as the pack would write them, read from the server's database
    /// off the frame loop: `None` while it is being read, then the rows (`None` inside for a
    /// class the database does not have). The database is read once and kept.
    pub fn server_class(&mut self, class: u32) -> Option<Option<String>> {
        self.server_classes.poll();
        if let Some(found) = self.server_classes.done.get(&class) {
            return Some(found.clone());
        }
        if self.server_classes.job.is_none() {
            if let Some(path) = self.server_sql.clone() {
                self.server_classes.start(path, Some(class));
            } else {
                return Some(None);
            }
        }
        None
    }

    /// Server class `class`'s rows as [`Self::server_class`] reads them, waited for.
    ///
    /// # Errors
    /// No server database was read.
    pub fn server_class_now(&mut self, class: u32) -> Result<Option<String>, String> {
        if self.server_sql.is_none() {
            return Err(self.say("messages.no_server_database"));
        }
        loop {
            if let Some(found) = self.server_class(class) {
                return Ok(found);
            }
            self.server_classes.wait();
        }
    }

    /// The server's database's text, as the forms keep it: read from its file once, off the
    /// frame loop, and waited for when it is not read yet. New weenies are made from it.
    ///
    /// # Errors
    /// No server database was read.
    pub(super) fn server_world(&mut self) -> Result<std::sync::Arc<String>, String> {
        let path = self
            .server_sql
            .clone()
            .ok_or_else(|| self.say("messages.no_server_database"))?;
        self.server_classes
            .world_now(path)
            .ok_or_else(|| self.say("messages.no_server_database"))
    }

    /// How many times the server's database was read from its file for the forms and the new
    /// weenies.
    #[must_use]
    pub fn server_database_reads(&self) -> usize {
        self.server_classes.reads()
    }

    /// Hold what the server's database is read for from the frame loop until let go, as a test
    /// holds it: a class asked for meanwhile is drawn as still being read.
    pub fn hold_server_reads(&mut self, hold: bool) {
        self.server_classes.held = hold;
    }

    /// The name of spell `id` in the files' spell table, read the first time.
    pub(super) fn spell_name(&mut self, id: u32) -> Option<String> {
        if self.spells.is_none() {
            let table = DataId(0x0E00_000E);
            let base = self.session.base();
            self.spells = Some(
                base.read_portal(table)
                    .and_then(|b| {
                        base.decode::<dereth_assets::tables::SpellTable>(table, &b)
                            .ok()
                    })
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

    /// A field of weenie `w`'s form: the text as typed while it is being typed, else `now`; a
    /// change is pushed as `edit` of `w` of what was typed when the field is left.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn field(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        key: &str,
        now: &str,
        width: f32,
        edit: impl FnOnce(String) -> FormEdit,
        out: &mut Vec<UiAction>,
    ) {
        self.field_hinted(ui, w, key, now, width, "", edit, out);
    }

    /// A field of weenie `w`'s form as [`Self::field`], saying `hint` while it is empty.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn field_hinted(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        key: &str,
        now: &str,
        width: f32,
        hint: &str,
        edit: impl FnOnce(String) -> FormEdit,
        out: &mut Vec<UiAction>,
    ) {
        let typing = typing_key(w, key);
        let mut text = self
            .panels
            .form_typing
            .get(&typing)
            .cloned()
            .unwrap_or_else(|| now.to_owned());
        let mut line = egui::TextEdit::singleline(&mut text).desired_width(width);
        if !hint.is_empty() {
            line = line.hint_text(hint);
        }
        let r = ui.add(line);
        self.form_fields.push((key.to_owned(), r.rect));
        if self.editor.focus_on(w, key) {
            r.request_focus();
            r.scroll_to_me(Some(egui::Align::Center));
        }
        if r.has_focus() || r.changed() {
            self.panels.form_typing.insert(typing.clone(), text.clone());
        }
        if r.lost_focus() {
            self.panels.form_typing.remove(&typing);
            if text.trim() != now.trim() {
                out.push(UiAction::Form(w.clone(), edit(text)));
            }
        }
    }

    /// A field of weenie `w`'s form for words players read as a paragraph: several lines tall,
    /// growing with its text and wrapping it. Enter starts a new line; Ctrl+Enter, or leaving it,
    /// pushes a change as `edit` of `w` of what was typed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn field_lines(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        key: &str,
        now: &str,
        width: f32,
        edit: impl FnOnce(String) -> FormEdit,
        out: &mut Vec<UiAction>,
    ) {
        let typing = typing_key(w, key);
        let mut text = self
            .panels
            .form_typing
            .get(&typing)
            .cloned()
            .unwrap_or_else(|| now.to_owned());
        let r = ui.add(
            egui::TextEdit::multiline(&mut text)
                .desired_width(width)
                .desired_rows(LINES),
        );
        self.form_fields.push((key.to_owned(), r.rect));
        if self.editor.focus_on(w, key) {
            r.request_focus();
            r.scroll_to_me(Some(egui::Align::Center));
        }
        // Ctrl+Enter, by the key's own modifiers: they may be let go within the same frame.
        let done = r.has_focus()
            && ui.input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            modifiers,
                            ..
                        } if modifiers.command
                    )
                })
            });
        if done {
            r.surrender_focus();
        }
        if (r.has_focus() || r.changed()) && !done {
            self.panels.form_typing.insert(typing.clone(), text.clone());
        }
        if r.lost_focus() || done {
            self.panels.form_typing.remove(&typing);
            if text.trim_end() != now.trim_end() {
                out.push(UiAction::Form(w.clone(), edit(text.trim_end().to_owned())));
            }
        }
    }

    /// Weenie `w`'s form: its sections by what it is, then every property under Advanced. A
    /// server's class the pack does not write yet is shown as the server has it, and its first
    /// change makes the pack's copy; another pack's is shown, not changed.
    pub(super) fn form_body(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let (path, mine) = match (w, self.session.weenie_file(w)) {
            (_, Some(f)) => f,
            (Weenie::Class(c), None) => {
                let doc = match self.server_doc_of(*c) {
                    None => {
                        ui.weak(t.get("weenie.reading"));
                        ui.ctx().request_repaint();
                        return;
                    }
                    Some(None) => {
                        ui.weak(t.get(if self.server_sql.is_some() {
                            "weenie.not_in_database"
                        } else {
                            "weenie.cannot_copy"
                        }));
                        return;
                    }
                    Some(Some(doc)) => doc,
                };
                self.editor_form(ui, w, &doc, out);
                return;
            }
            (Weenie::Symbol(_), None) => {
                ui.weak(t.get("weenie.no_sql"));
                return;
            }
        };
        if !mine {
            ui.weak(t.get("weenie.another_pack"));
        }
        let Some(doc) = self.file_doc(&path) else {
            ui.weak(t.get("weenie.sql_gone"));
            return;
        };
        ui.add_enabled_ui(mine, |ui| self.editor_form(ui, w, &doc, out));
    }

    /// The form's Advanced page: every property of `doc` and every row of its tables that the
    /// weenie's other pages do not show, or with "Show all" every one.
    pub(super) fn weenie_form(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let schema = self.schema.clone();
        let prop_name = |k: PropertyKind, id: u32| {
            schema
                .as_ref()
                .and_then(|s| s.property(k, id))
                .map_or_else(|| format!("#{id}"), str::to_owned)
        };
        let left_out = if self.editor.advanced_all {
            super::fields::OnPages::default()
        } else {
            super::fields::on_pages(doc)
        };
        // Its name first, then every property by type.
        let name = doc
            .property(PropertyKind::String, 1)
            .map(|v| content::unquote(&v))
            .unwrap_or_default();
        if !left_out.has_property(PropertyKind::String, 1) {
            ui.horizontal(|ui| {
                ui.label(t.get("weenie.name"));
                self.field(
                    ui,
                    w,
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
        }
        for kind in PropertyKind::ALL {
            let props: Vec<(u32, String)> = doc
                .properties(kind)
                .into_iter()
                .filter(|(id, _)| !left_out.has_property(kind, *id))
                .collect();
            let fold = egui::CollapsingHeader::new(format!(
                "{} ({})",
                t.get(kind_name(kind)),
                props.len()
            ))
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
                            ui.label(prop_name(kind, *id)).on_hover_text(
                                t.fill("weenie.property_tip", &[("kind", &kind.key()), ("id", id)]),
                            );
                            let key = format!("{}:{id}", kind.key());
                            let (k, i) = (kind, *id);
                            self.field(
                                ui,
                                w,
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
                            if ui.small_button("x").clicked() {
                                out.push(UiAction::Form(
                                    w.clone(),
                                    FormEdit::Property {
                                        kind,
                                        id: *id,
                                        value: None,
                                    },
                                ));
                            }
                            ui.end_row();
                        }
                    });
                // A property added by its name or number and its value.
                let typed = self.typed_row(
                    ui,
                    w,
                    &format!("add:{}", kind.key()),
                    &[
                        ("name", t.get("weenie.add_name_hint"), 160.0),
                        ("value", t.get("weenie.add_value_hint"), 110.0),
                    ],
                    t.get("weenie.add"),
                );
                if let Some([n, v]) = typed.as_deref() {
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
                            out.push(UiAction::Form(
                                w.clone(),
                                FormEdit::Property {
                                    kind,
                                    id,
                                    value: Some(v.trim().to_owned()),
                                },
                            ));
                            self.forget_typed(w, &format!("add:{}", kind.key()), 2);
                        }
                        None => {
                            self.panels.message = t.fill(
                                "messages.not_a_property",
                                &[("name", &n), ("kind", &kind.key())],
                            );
                        }
                    }
                }
            });
            self.editor.folds.push((
                kind.key().to_owned(),
                fold.header_response.rect,
                fold.openness,
            ));
        }
        if !left_out.has_table(super::fields::EMOTE_TABLE) {
            self.emotes_section(ui, w, doc, false, out);
            self.quests_section(ui, w, doc, false, out);
        }
        if !left_out.has_property(PropertyKind::DataId, 35) {
            self.treasure_section(ui, w, doc, out);
        }
        for raw in RAW_TABLES.iter().filter(|r| !left_out.has_table(r.table)) {
            self.rows_section(ui, w, doc, raw, false, out);
        }
    }

    /// A row of boxes for something new, `key` naming it, each box by its own key, hint and
    /// width, then a button of `button`'s words: what each box holds, when the button was
    /// pressed (or Enter in the last box) with the first filled in. What is typed is kept until
    /// it is let go ([`Self::forget_typed`]).
    pub(super) fn typed_row(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        key: &str,
        boxes: &[(&str, &str, f32)],
        button: &str,
    ) -> Option<Vec<String>> {
        let mut typed: Vec<String> = boxes
            .iter()
            .enumerate()
            .map(|(i, _)| {
                self.panels
                    .form_typing
                    .get(&typing_key(w, &format!("{key}:{i}")))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();
        let mut pressed = false;
        ui.horizontal_wrapped(|ui| {
            let last = boxes.len().saturating_sub(1);
            for (i, (name, hint, width)) in boxes.iter().enumerate() {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut typed[i])
                        .hint_text(*hint)
                        .desired_width(*width),
                );
                self.form_fields.push((format!("{key}:{name}"), r.rect));
                if i == last && r.lost_focus() && ui.input(|x| x.key_pressed(egui::Key::Enter)) {
                    pressed = true;
                }
            }
            let ready = typed.first().is_some_and(|f| !f.trim().is_empty());
            let add = ui.add_enabled(ready, egui::Button::new(button));
            self.editor.add_buttons.push((key.to_owned(), add.rect));
            pressed |= add.clicked();
        });
        for (i, now) in typed.iter().enumerate() {
            let k = typing_key(w, &format!("{key}:{i}"));
            if now.is_empty() {
                self.panels.form_typing.remove(&k);
            } else {
                self.panels.form_typing.insert(k, now.clone());
            }
        }
        (pressed && typed.first().is_some_and(|f| !f.trim().is_empty())).then_some(typed)
    }

    /// What was typed into the `n` boxes of row `key`, let go once it is added.
    pub(super) fn forget_typed(&mut self, w: &Weenie, key: &str, n: usize) {
        for i in 0..n {
            self.panels
                .form_typing
                .remove(&typing_key(w, &format!("{key}:{i}")));
        }
    }

    /// The emotes, raw: each set (what calls it, how likely, its quest) and its actions in order,
    /// each action's type, delay and the columns it sets as fields, and a button that takes an
    /// action away. Sets and actions are added, moved and taken away in the emote editor.
    /// `open`: unfolded to start with.
    pub(super) fn emotes_section(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        open: bool,
        out: &mut Vec<UiAction>,
    ) {
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
            .id_salt(("emotes", open))
            .default_open(open)
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
                        .default_open(open)
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
                                        w,
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
                                    if ui.small_button("x").clicked() {
                                        out.push(UiAction::Form(
                                            w.clone(),
                                            FormEdit::RemoveAction { set: i, action: n },
                                        ));
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
                                                w,
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
                                ui.separator();
                            }
                        });
                }
            });
    }

    /// The quests the emotes name: where each is registered, the pack's own editable, and one
    /// not registered registered in the pack; each name opens it in the Quests tab. `open`:
    /// unfolded to start with.
    pub(super) fn quests_section(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        open: bool,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let names = quests_named(doc);
        self.quests.fold_links.clear();
        egui::CollapsingHeader::new(t.fill("weenie.quests", &[("n", &names.len())]))
            .id_salt(("quests", open))
            .default_open(open)
            .show(ui, |ui| {
                for name in names {
                    let found = self.session.quest(&name);
                    ui.horizontal(|ui| {
                        // Its name opens it in the Quests tab.
                        let link = ui
                            .link(egui::RichText::new(&name).strong())
                            .on_hover_text(t.get("quests.open_in_tab"));
                        self.quests.fold_links.push((name.clone(), link.rect));
                        if link.clicked() {
                            out.push(UiAction::Quest(super::QuestAction::Open(Some(
                                name.clone(),
                            ))));
                        }
                        match &found {
                            None => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(0xF0, 0xC0, 0x60),
                                    t.get("weenie.quest_not_registered"),
                                );
                                if ui.small_button(t.get("weenie.register_quest")).clicked() {
                                    out.push(UiAction::Form(
                                        w.clone(),
                                        FormEdit::RegisterQuest { name: name.clone() },
                                    ));
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
                            Some((q, Some(file))) => {
                                let pack = self
                                    .session
                                    .packs()
                                    .iter()
                                    .find(|p| file.starts_with(&p.dir))
                                    .map(|p| p.name.clone())
                                    .unwrap_or_default();
                                ui.weak(t.fill("weenie.quest_packs", &[("pack", &pack)]));
                                // Every so many seconds, so many times (or any), each typed.
                                let [before, between, after] =
                                    super::quests_tab::timing_pieces(t.get("quests.timing"));
                                let any = t.get("weenie.quest_any").to_owned();
                                let times =
                                    super::quests_tab::solves_words(i64::from(q.max_solves), &any);
                                let every = q.min_delta.to_string();
                                for (piece, column, now) in [
                                    (before, "min_Delta", every.as_str()),
                                    (between, "max_Solves", times.as_str()),
                                ] {
                                    ui.weak(piece);
                                    let (n, any) = (name.clone(), any.clone());
                                    self.field(
                                        ui,
                                        w,
                                        &format!("quest:{name}:{column}"),
                                        now,
                                        60.0,
                                        move |v| FormEdit::Quest {
                                            name: n,
                                            column: column.to_owned(),
                                            value: if column == "max_Solves" {
                                                super::quests_tab::solves_typed(&v, &any)
                                            } else {
                                                v
                                            },
                                        },
                                        out,
                                    );
                                }
                                ui.weak(after);
                            }
                        }
                    });
                    // A pack's quest's description, several lines.
                    if let Some((q, Some(_))) = &found {
                        ui.weak("message");
                        let n = name.clone();
                        self.field_lines(
                            ui,
                            w,
                            &format!("quest:{name}:message"),
                            &q.message,
                            super::fields::LINES_WIDTH,
                            move |v| FormEdit::Quest {
                                name: n,
                                column: "message".to_owned(),
                                value: v,
                            },
                            out,
                        );
                    }
                }
            });
    }

    /// What it drops when it dies: its death treasure (data id 35) and what that treasure is.
    fn treasure_section(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
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
                        w,
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
                match now.map(|n| (n, self.session.treasure(n))) {
                    None => {
                        ui.weak(tx.get("weenie.treasure_none"));
                    }
                    Some((n, None)) => {
                        ui.weak(tx.fill("weenie.treasure_unknown", &[("n", &n)]));
                    }
                    Some((n, Some(t))) => {
                        if ui.link(tx.get("loot.open_in_tab")).clicked() {
                            out.push(UiAction::Loot(super::LootAction::Open(Some((
                                lyceum_foundry::loot::LootKind::Death,
                                n,
                            )))));
                        }
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

    /// A section of a table's rows, raw: each row's columns as fields, the spell or weenie it
    /// names, and a button to take it away; under them, boxes for a new row's columns and Add.
    /// `open`: unfolded to start with.
    pub(super) fn rows_section(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        raw: &RawTable,
        open: bool,
        out: &mut Vec<UiAction>,
    ) {
        let tx = self.text.table();
        let (table, shown) = (raw.table, raw.shown);
        let rows = doc.rows(table);
        let fold = egui::CollapsingHeader::new(format!("{} ({})", tx.get(raw.heading), rows.len()))
            .id_salt((table, open))
            .default_open(open)
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
                                    w,
                                    &format!("{table}:{n}:{c}"),
                                    &now,
                                    if Some(*c) == raw.named || *c == "page_Text" {
                                        90.0
                                    } else {
                                        50.0
                                    },
                                    move |v| FormEdit::Cell {
                                        table: t,
                                        row: n,
                                        column: col,
                                        value: v,
                                    },
                                    out,
                                );
                            }
                            let named = match raw.named {
                                Some("spell") => doc
                                    .cell(*r, "spell")
                                    .and_then(|v| v.trim().parse::<u32>().ok())
                                    .and_then(|s| self.spell_name(s)),
                                Some(key) => {
                                    self.class_name(&doc.cell(*r, key).unwrap_or_default())
                                }
                                None => None,
                            };
                            ui.weak(named.unwrap_or_default());
                            if ui.small_button("x").clicked() {
                                out.push(UiAction::Form(
                                    w.clone(),
                                    FormEdit::RemoveRow {
                                        table: table.to_owned(),
                                        row: n,
                                    },
                                ));
                            }
                            ui.end_row();
                        }
                    });
                // A new row, by the columns it is typed with; any other takes the export's
                // default.
                let adds: Vec<(&str, &str)> = match raw.adds {
                    Some(a) => a.iter().map(|(c, k)| (*c, tx.get(k))).collect(),
                    None => shown.iter().map(|c| (*c, *c)).collect(),
                };
                let boxes: Vec<(&str, &str, f32)> = adds
                    .iter()
                    .map(|(c, hint)| (*c, *hint, if adds.len() > 3 { 70.0 } else { 110.0 }))
                    .collect();
                let key = format!("add:{table}");
                if let Some(typed) = self.typed_row(ui, w, &key, &boxes, tx.get("weenie.add")) {
                    let values: Vec<(String, String)> = adds
                        .iter()
                        .zip(&typed)
                        .filter(|(_, v)| !v.trim().is_empty())
                        .map(|((c, _), v)| ((*c).to_owned(), v.trim().to_owned()))
                        .collect();
                    let names = |c: &str| matches!(c, "spell" | "weenie_Class_Id");
                    let fits = values.iter().all(|(c, v)| {
                        if names(c) {
                            names_one(v)
                        } else if c == "probability" {
                            v.parse::<f64>().is_ok()
                        } else {
                            true
                        }
                    });
                    if fits {
                        out.push(UiAction::Form(
                            w.clone(),
                            FormEdit::AddRow {
                                table: table.to_owned(),
                                values,
                            },
                        ));
                        self.forget_typed(w, &key, adds.len());
                    } else {
                        self.panels.message = tx.get("messages.not_a_value").to_owned();
                    }
                }
            });
        self.editor
            .folds
            .push((table.to_owned(), fold.header_response.rect, fold.openness));
    }
}

/// How many lines tall a field of words players read as a paragraph starts.
pub(super) const LINES: usize = 3;

/// The database's text, a class, and that class's rows cut from it, as the reading thread answers.
type ClassCut = (std::sync::Arc<String>, Option<(u32, Option<String>)>);

/// The server's classes as the forms read them: its database's text, read once off the frame
/// loop and kept, and each class's rows, cut from it as they are asked for. New weenies are made
/// from the same text.
#[derive(Default)]
pub struct ServerClasses {
    world: Option<std::sync::Arc<String>>,
    done: BTreeMap<u32, Option<String>>,
    job: Option<std::sync::mpsc::Receiver<ClassCut>>,
    /// How many times the text was read from the database's file.
    reads: usize,
    /// Whether the thread's answer is held from the frame loop, which then finds the class still
    /// being read (a wait takes it all the same).
    held: bool,
}

impl std::fmt::Debug for ServerClasses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerClasses")
            .field("read", &self.world.is_some())
            .field("classes", &self.done.len())
            .finish_non_exhaustive()
    }
}

impl ServerClasses {
    /// Cut class `class` (none: only read the text) from the database at `path` on a thread of
    /// its own. The text is read from the file the first time only.
    fn start(&mut self, path: PathBuf, class: Option<u32>) {
        let world = self.world.clone();
        if world.is_none() {
            self.reads += 1;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let world = world.unwrap_or_else(|| {
                let bytes = std::fs::read(&path).unwrap_or_default();
                std::sync::Arc::new(String::from_utf8_lossy(&bytes).into_owned())
            });
            let cut = class.map(|c| (c, content::override_of(&world, c)));
            let _ = tx.send((world, cut));
        });
        self.job = Some(rx);
    }

    /// What the thread answered: the text kept, and the class it cut.
    fn take(&mut self, (world, cut): ClassCut) {
        self.world = Some(world);
        if let Some((class, rows)) = cut {
            self.done.insert(class, rows);
        }
    }

    /// Take what the thread has answered, if it has and it is not held.
    fn poll(&mut self) {
        if self.held {
            return;
        }
        let Some(rx) = &self.job else { return };
        match rx.try_recv() {
            Ok(answer) => {
                self.take(answer);
                self.job = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Wait for what the thread is doing.
    fn wait(&mut self) {
        let Some(rx) = self.job.take() else { return };
        if let Ok(answer) = rx.recv() {
            self.take(answer);
        }
    }

    /// The text of the database at `path` read off the frame loop, unless it is kept or being
    /// read already.
    pub fn read_ahead(&mut self, path: PathBuf) {
        self.poll();
        if self.world.is_none() && self.job.is_none() {
            self.start(path, None);
        }
    }

    /// The text of the database at `path`: as kept, else read off the frame loop and waited for.
    /// `None` when the reading thread was lost.
    pub fn world_now(&mut self, path: PathBuf) -> Option<std::sync::Arc<String>> {
        self.poll();
        if self.world.is_none() {
            if self.job.is_none() {
                self.start(path, None);
            }
            self.wait();
        }
        self.world.clone()
    }

    /// How many times the text was read from the database's file.
    #[must_use]
    pub fn reads(&self) -> usize {
        self.reads
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's weenie tab)
    use super::*;

    #[test]
    fn a_packs_weenie_is_named_by_its_own_name_not_its_packs() {
        let t = crate::text::Table::default();
        let w = Weenie::Symbol("holtburg:coolest-dungeon".into());
        assert_eq!(weenie_label(&w, &t), "coolest-dungeon");
        assert_eq!(
            super::super::flows::symbol_of("Coolest Dungeon!"),
            "coolest-dungeon"
        );
    }

    #[test]
    fn the_servers_database_is_read_from_its_file_once_for_every_class_and_weenie_made_from_it() {
        let dir = std::env::temp_dir().join(format!("lyceum-server-reads-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.sql");
        std::fs::write(
            &path,
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (7, 'drudgeskulker', 10, '2005-02-09 10:00:00')
     , (8, 'other', 10, '2005-02-09 10:00:00');
",
        )
        .unwrap();
        let mut s = ServerClasses::default();
        // Read ahead while a dialog is open, then waited for when a weenie is made.
        s.read_ahead(path.clone());
        s.read_ahead(path.clone());
        let world = s.world_now(path.clone()).unwrap();
        assert!(world.contains("drudgeskulker"));
        // A class cut, then the file gone: the next class and the text still come from what was
        // read.
        s.start(path.clone(), Some(7));
        s.wait();
        assert!(s.done[&7]
            .as_deref()
            .is_some_and(|t| t.contains("'drudgeskulker'")));
        std::fs::remove_dir_all(&dir).unwrap();
        s.start(path.clone(), Some(8));
        s.wait();
        assert!(s.done[&8].as_deref().is_some_and(|t| t.contains("'other'")));
        assert!(s.world_now(path).is_some_and(|w| w.contains("'other'")));
        assert_eq!(s.reads(), 1);
    }
}
