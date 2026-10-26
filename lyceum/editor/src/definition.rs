//! The Definition pane: what the weenie of the selected spawn (or the spawn browser's choice) is,
//! read only. Its head names it, its kind, its id and whose it is; under it, its fields by what
//! they are for as plain words, what it holds, sells, wields, casts and spawns, its emotes, where
//! it appears and what names it as counts that open into lists, and every property folded at the
//! bottom.
//! Edit, or a double click on a value, opens the editor on it.

use dereth_primitives::DataId;
use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::{generator_of, Generator, Weenie};

use super::collections;
use super::editor_tab::Owner;
use super::fields::{self, field_key, plain_words, tooltip, Field, Target, Widget};
use super::kinds::Kind;
use super::{EditorApp, UiAction};

/// The height of the Definition pane's head and the editor's, the picture's.
pub(super) const HEAD_HEIGHT: f32 = 48.0;

/// A list the pane opens into in place of its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drill {
    Wields,
    Holds,
    Sells,
    Spells,
    Spawns,
    Emotes,
    Skills,
    BodyParts,
    Appears,
    /// The weenies whose lists, generators and emotes name it.
    UsedBy,
}

impl Drill {
    /// Its name's key in the text.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Wields => "definition.wields",
            Self::Holds => "definition.holds",
            Self::Sells => "definition.sells",
            Self::Spells => "definition.spells",
            Self::Spawns => "definition.spawns",
            Self::Emotes => "definition.emotes",
            Self::Skills => "definition.skills",
            Self::BodyParts => "definition.body_parts",
            Self::Appears => "definition.appears",
            Self::UsedBy => "definition.used_by",
        }
    }
}

/// The pane's state: the list it is opened into, for the weenie it was opened on.
#[derive(Debug, Clone, Default)]
pub struct Definition {
    drill: Option<(Weenie, Drill)>,
    /// Its Edit button, its head's words and where they may start (after the picture), and where
    /// each collection's count was drawn, as drawn last.
    pub edit_button: Option<egui::Rect>,
    pub head_words: Option<egui::Rect>,
    pub head_words_from: Option<f32>,
    pub drill_rects: Vec<(Drill, egui::Rect)>,
    /// Each field's words, by the field's key, as drawn last.
    pub words: Vec<(&'static str, String)>,
    /// Its menu's button, the words of each of the menu's items (while it is open) and its
    /// Delete, as drawn last.
    pub menu_button: Option<egui::Rect>,
    pub menu_items: Vec<String>,
    pub delete_button: Option<egui::Rect>,
    /// What names a weenie, as last found.
    pub(super) used_by: Option<super::usages::UsedBy>,
    /// Where a weenie appears, as last gathered and drawn.
    pub appears: super::appears::Appears,
}

impl Definition {
    /// The list the pane shows for weenie `w`, when it is opened into one.
    #[must_use]
    pub fn drill_of(&self, w: &Weenie) -> Option<Drill> {
        self.drill
            .as_ref()
            .filter(|(of, _)| of == w)
            .map(|(_, d)| *d)
    }

    /// The pane opened into list `d` of weenie `w`.
    pub fn drill_into(&mut self, w: &Weenie, d: Drill) {
        self.drill = Some((w.clone(), d));
    }

    /// The pane's body again.
    pub fn back(&mut self) {
        self.drill = None;
    }
}

/// A field as a line of the pane shows it: its value in words, or a list's count.
enum Shown {
    Words(String),
    Count(Drill, usize),
}

impl EditorApp {
    /// The words of field `f` of `doc` as the pane shows them: a death treasure by its tier, a
    /// spell by its name, a clothing look by its name and id, a destination by its place.
    pub(super) fn field_words(&mut self, f: &Field, doc: &SqlDoc) -> String {
        let t = self.text.table();
        match (f.target, f.widget) {
            (Target::Property(kind, id), Widget::Treasure(loot)) => doc
                .property(kind, id)
                .filter(|v| !v.trim().is_empty())
                .map_or_else(
                    || t.get("fields.none").to_owned(),
                    |v| self.loot_field_words(loot, &v),
                ),
            (Target::Property(kind, id), Widget::Spell) => doc
                .property(kind, id)
                .and_then(|v| lyceum_foundry::sql_ids::number(&content::bare(&v)))
                .map(|n| match self.spell_name(n) {
                    Some(name) => format!("{name} ({n})"),
                    None => n.to_string(),
                })
                .unwrap_or_default(),
            (Target::Property(kind, id), Widget::ClothingLook) => doc
                .property(kind, id)
                .and_then(|v| lyceum_foundry::sql_ids::number(&content::bare(&v)))
                .map(|n| self.clothing_look_words(n))
                .unwrap_or_default(),
            (Target::Destination, _) => self.destination_words(doc),
            _ => plain_words(f, doc, &t, self.schema.as_ref()),
        }
    }

    /// The generator weenie `w` is, read from `doc`: the selected spawn's, with what it owns, when
    /// that is a spawn of it. `None` for a weenie that is no generator.
    fn definition_generator(&mut self, w: &Weenie, doc: &SqlDoc) -> Option<Generator> {
        if let Some(s) = self.selected_spawn() {
            if self.session.spawn_weenie(&s).as_ref() == Some(w) {
                return self.spawn_generator(&s);
            }
        }
        let g = generator_of(doc);
        (g.radius.is_some() || !g.profiles.is_empty() || g.most.is_some()).then_some(g)
    }

    /// The Definition pane.
    pub(super) fn definition_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        self.definition.drill_rects.clear();
        self.definition.words.clear();
        self.definition.edit_button = None;
        self.definition.head_words = None;
        self.definition.head_words_from = None;
        let Some(w) = self.form_weenie() else {
            ui.weak(t.get("weenie.nothing"));
            if ui.button(t.get("creator.open")).clicked() {
                out.push(UiAction::NewWeenie(None));
            }
            return;
        };
        // Its rows: the pack's file of it, else the server's class.
        let Some(doc) = self.weenie_doc(&w) else {
            self.definition_head(ui, &w, None, out);
            ui.weak(t.get("weenie.reading"));
            ui.ctx().request_repaint();
            return;
        };
        self.definition_head(ui, &w, doc.as_deref(), out);
        ui.separator();
        let Some(doc) = doc else {
            ui.weak(t.get(match (&w, self.server_sql.is_some()) {
                (Weenie::Symbol(_), _) => "weenie.no_sql",
                (Weenie::Class(_), true) => "weenie.not_in_database",
                (Weenie::Class(_), false) => "weenie.cannot_copy",
            }));
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("definition")
            .auto_shrink(false)
            .show(ui, |ui| {
                // Off the pane's divider.
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: super::fields::FIELDS_MARGIN,
                        ..egui::Margin::ZERO
                    })
                    .show(ui, |ui| match self.definition.drill_of(&w) {
                        Some(d) => self.drill_list(ui, &w, &doc, d, out),
                        None => self.definition_body(ui, &w, &doc, out),
                    });
            });
    }

    /// The pane's head: picture, name, kind, id and whose it is; Edit and the menu.
    fn definition_head(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: Option<&SqlDoc>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let entry = self.weenie_entry(w).cloned();
        let owner = self.owner_of(w);
        ui.horizontal(|ui| {
            let mut budget = super::PICTURES_A_FRAME;
            if let Some(id) = entry
                .as_ref()
                .and_then(|e| e.setup)
                .and_then(|s| self.picture(DataId(s), &mut budget))
            {
                ui.add(egui::Image::new((id, egui::vec2(48.0, 48.0))));
            }
            self.definition.head_words_from = Some(ui.cursor().min.x);
            // The menu and Edit first, from the right; the words take what is left, cut short
            // before them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.definition_menu(ui, w, &owner, out);
                let edit = ui.add_enabled(
                    owner.editable(),
                    egui::Button::new(t.get("definition.edit")),
                );
                let edit = edit.on_disabled_hover_text(t.get("weenie.another_pack"));
                self.definition.edit_button = Some(edit.rect);
                if edit.clicked() {
                    out.push(UiAction::EditWeenie(w.clone(), None));
                }
                let room = egui::vec2(ui.available_width().max(0.0), HEAD_HEIGHT);
                let words = ui.allocate_ui_with_layout(
                    room,
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width(room.x);
                        let name = ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    entry
                                        .as_ref()
                                        .map_or(t.get("weenie.a_weenie"), |e| e.name.as_str()),
                                )
                                .strong(),
                            )
                            .truncate(),
                        );
                        let mut line = Vec::new();
                        if let Some(doc) = doc {
                            line.push(t.get(&format!("kinds.{}", Kind::of(doc).key())).to_owned());
                        }
                        line.push(super::form::weenie_label(w, &t));
                        line.push(owner.words(&t));
                        let line = ui.add(
                            egui::Label::new(egui::RichText::new(line.join(" · ")).weak())
                                .truncate(),
                        );
                        name.rect.union(line.rect)
                    },
                );
                self.definition.head_words = Some(words.inner);
            });
        });
    }

    /// The pane's menu: Make a copy, Find usages, Copy class id, New weenie; for the pack's
    /// change of a server's weenie, Put back the server's; for the pack's own, Delete.
    fn definition_menu(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        owner: &Owner,
        out: &mut Vec<UiAction>,
    ) {
        use egui::containers::menu::MenuButton;
        let t = self.text.table();
        let mut items = Vec::new();
        let (menu, _) = MenuButton::new("…").ui(ui, |ui| {
            let mut item = |ui: &mut egui::Ui, words: &str| {
                items.push(words.to_owned());
                ui.button(words)
            };
            if item(ui, t.get("definition.make_a_copy")).clicked() {
                out.push(UiAction::NewWeenie(Some(w.clone())));
            }
            if item(ui, t.get("definition.find_usages"))
                .on_hover_text(t.get("definition.find_usages_about"))
                .clicked()
            {
                self.definition.drill_into(w, Drill::UsedBy);
            }
            if item(ui, t.get("definition.copy_id")).clicked() {
                let id = match w {
                    Weenie::Class(c) => c.to_string(),
                    Weenie::Symbol(s) => {
                        format!("{{{{wcid:{}}}}}", s.rsplit(':').next().unwrap_or(s))
                    }
                };
                ui.ctx().copy_text(id);
            }
            if let (Weenie::Class(c), Owner::ChangedIn { mine: true, .. }) = (w, owner) {
                if item(ui, t.get("definition.put_back_server"))
                    .on_hover_text(t.get("definition.put_back_server_about"))
                    .clicked()
                {
                    out.push(UiAction::PutBackServer(*c));
                }
            }
            if item(ui, t.get("creator.open")).clicked() {
                out.push(UiAction::NewWeenie(None));
            }
            if matches!(owner, Owner::Pack { mine: true, .. }) {
                let r = item(ui, t.get("definition.delete"));
                self.definition.delete_button = Some(r.rect);
                if r.clicked() {
                    out.push(UiAction::DeleteWeenie(w.clone()));
                }
            }
        });
        self.definition.menu_button = Some(menu.rect);
        self.definition.menu_items = items;
    }

    /// The pane's body: a line for each of its sections, each field's value in words and each
    /// list's count opening into it; where it appears; every property.
    fn definition_body(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let schema = self.schema.clone();
        let kind = Kind::of(doc);
        for (section, list) in fields::shown(kind, doc) {
            // Each field set, in words, or a list and its count.
            let mut items: Vec<(&Field, Shown)> = Vec::new();
            for f in list {
                match f.target {
                    Target::List(d) => {
                        let n = match d {
                            Drill::Spawns => self
                                .definition_generator(w, doc)
                                .map_or_else(|| fields::count(doc, d), |g| g.profiles.len()),
                            _ => fields::count(doc, d),
                        };
                        items.push((f, Shown::Count(d, n)));
                    }
                    _ => {
                        let v = self.field_words(f, doc);
                        if !v.is_empty() {
                            items.push((f, Shown::Words(v)));
                        }
                    }
                }
            }
            if items.is_empty() {
                continue;
            }
            ui.horizontal_wrapped(|ui| {
                ui.strong(t.get(&section.name_key()));
                for (i, (f, item)) in items.iter().enumerate() {
                    if i > 0 {
                        ui.weak("·");
                    }
                    match item {
                        Shown::Words(v) => {
                            self.definition.words.push((f.key, v.clone()));
                            let r = fields::with_tip(
                                ui.add(
                                    egui::Label::new(format!(
                                        "{} {v}",
                                        t.get(&fields::label_key(f))
                                    ))
                                    .sense(egui::Sense::click()),
                                ),
                                &tooltip(f, &t, schema.as_ref()),
                            );
                            if r.double_clicked() {
                                out.push(UiAction::EditWeenie(w.clone(), Some(field_key(f))));
                            }
                        }
                        Shown::Count(d, n) => {
                            let r = ui.button(
                                t.fill("definition.count", &[("what", &t.get(d.key())), ("n", n)]),
                            );
                            self.definition.drill_rects.push((*d, r.rect));
                            if r.clicked() {
                                self.definition.drill_into(w, *d);
                            }
                        }
                    }
                }
            });
        }
        // Where it appears and what names it.
        ui.horizontal_wrapped(|ui| {
            let n = self.appears_of(w).len();
            let used = self.usages_of(w).len() + self.loot_holding_of(w).len();
            for (d, n) in [(Drill::Appears, n), (Drill::UsedBy, used)] {
                let r =
                    ui.button(t.fill("definition.count", &[("what", &t.get(d.key())), ("n", &n)]));
                self.definition.drill_rects.push((d, r.rect));
                if r.clicked() {
                    self.definition.drill_into(w, d);
                }
            }
        });
        self.every_property(ui, doc);
    }

    /// Every property, folded: the raw table by the server's names, read only.
    fn every_property(&mut self, ui: &mut egui::Ui, doc: &SqlDoc) {
        let t = self.text.table();
        let n: usize = PropertyKind::ALL
            .iter()
            .map(|k| doc.properties(*k).len())
            .sum();
        egui::CollapsingHeader::new(t.fill("definition.every_property", &[("n", &n)]))
            .id_salt("every property")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("every property")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for kind in PropertyKind::ALL {
                            for (id, value) in doc.properties(kind) {
                                let name = self
                                    .schema
                                    .as_ref()
                                    .and_then(|s| s.property(kind, id))
                                    .map_or_else(|| format!("#{id}"), str::to_owned);
                                ui.label(name).on_hover_text(t.fill(
                                    "weenie.property_tip",
                                    &[("kind", &kind.key()), ("id", &id)],
                                ));
                                ui.label(match kind {
                                    PropertyKind::String => content::unquote(&value),
                                    _ => value,
                                });
                                ui.end_row();
                            }
                        }
                    });
            });
    }

    /// The list the pane is opened into: each entry by name (with its picture, for a weenie, a
    /// click on which shows it), under a way back.
    fn drill_list(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        d: Drill,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let name = self
            .weenie_entry(w)
            .map_or_else(|| super::form::weenie_label(w, &t), |e| e.name.clone());
        if ui.link(format!("‹ {name}")).clicked() {
            self.definition.back();
        }
        ui.strong(t.get(d.key()));
        let mut budget = super::PICTURES_A_FRAME;
        match d {
            Drill::Wields | Drill::Holds | Drill::Sells => {
                // A creature's treasure by its chance, set by set.
                let list = collections::list_of(doc, d);
                let creature = Kind::of(doc).is_creature();
                let sets = if creature {
                    collections::sets(&list)
                } else {
                    vec![None; list.len()]
                };
                let id = egui::Id::new(("drill rows", w.clone(), d.key()));
                super::seen::blocks(
                    ui,
                    id,
                    list.len(),
                    36.0,
                    |_| false,
                    |ui, i| {
                        let (r, set) = (&list[i], sets[i]);
                        // The first of a set: under its heading.
                        if let Some(s) = set.filter(|s| i == 0 || sets[i - 1] != Some(*s)) {
                            ui.weak(t.fill("collections.set", &[("n", &(s + 1))]));
                        }
                        let class = r.get("weenie_Class_Id").unwrap_or("0").to_owned();
                        #[allow(clippy::cast_possible_truncation)] // a count
                        let many = r.number("stack_Size").map(|n| n as i64).filter(|n| *n > 1);
                        ui.horizontal(|ui| {
                            if lyceum_foundry::sql_ids::number(&class) == Some(0) {
                                ui.weak(t.get("collections.nothing"));
                            } else {
                                self.weenie_row(ui, &class, many, &mut budget, out);
                            }
                            if set.is_some() {
                                let p = r.number("shade").unwrap_or(0.0) * 100.0;
                                ui.weak(format!("{}%", collections::percent_words(p)));
                            }
                        });
                    },
                );
            }
            Drill::Spells => {
                let creature = Kind::of(doc).is_creature();
                let rows = collections::rows_of(doc, collections::SPELL_BOOK);
                let id = egui::Id::new(("drill rows", w.clone(), d.key()));
                super::seen::blocks(
                    ui,
                    id,
                    rows.len(),
                    18.0,
                    |_| false,
                    |ui, i| {
                        let r = &rows[i];
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let id = r.number("spell").map_or(0, |n| n as u32);
                        let named = self.spell_name(id);
                        let mut line =
                            named.map_or_else(|| id.to_string(), |n| format!("{n} ({id})"));
                        if creature {
                            let p =
                                collections::cast_chance(r.number("probability").unwrap_or(2.0));
                            line.push_str(&format!(" · {}%", collections::percent_words(p)));
                        }
                        ui.label(line);
                    },
                );
            }
            Drill::Skills => {
                let names = super::fields::enum_values(self.schema.as_ref(), "Skill");
                for r in collections::rows_of(doc, collections::SKILLS) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let ty = r.number("type").map_or(0, |n| n as u32);
                    let name = names.iter().find(|(v, _)| *v == ty).map_or_else(
                        || t.fill("collections.skill_n", &[("n", &ty)]),
                        |(_, n)| n.clone(),
                    );
                    let level = r.get("init_Level").map(content::bare).unwrap_or_default();
                    ui.label(format!("{name} {level}"));
                }
            }
            Drill::BodyParts => {
                for r in collections::rows_of(doc, collections::BODY_PARTS) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let key = r.number("key").map_or(0, |n| n as u32);
                    let name = collections::BODY_PART_NAMES
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map_or_else(
                            || t.fill("collections.part_n", &[("n", &key)]),
                            |(_, n)| t.get(&format!("collections.{n}")).to_owned(),
                        );
                    let num = |c: &str| r.get(c).map(content::bare).unwrap_or_default();
                    ui.label(t.fill(
                        "collections.part_line",
                        &[
                            ("part", &name),
                            ("damage", &num("d_Val")),
                            ("armor", &num("base_Armor")),
                        ],
                    ));
                }
            }
            Drill::Spawns => {
                if let Some(g) = self.definition_generator(w, doc) {
                    self.generator_card(ui, &g, out);
                }
            }
            Drill::Emotes => self.emote_lines(ui, w, doc, out),
            Drill::UsedBy => self.used_by_list(ui, w, out),
            Drill::Appears => self.appears_list(ui, w, out),
        }
    }

    /// A weenie a list names as written (a class id or a pack weenie's token): its picture and its
    /// name (`many` of it), a click on which shows it.
    pub(super) fn weenie_row(
        &mut self,
        ui: &mut egui::Ui,
        written: &str,
        many: Option<i64>,
        budget: &mut u32,
        out: &mut Vec<UiAction>,
    ) {
        let name = self
            .class_name(written)
            .unwrap_or_else(|| written.trim().to_owned());
        let weenie = self.weenie_written(written);
        let setup = weenie
            .as_ref()
            .and_then(|w| self.weenie_entry(w))
            .and_then(|e| e.setup);
        ui.horizontal(|ui| {
            if let Some(id) = setup.and_then(|s| self.picture(DataId(s), budget)) {
                ui.add(egui::Image::new((id, egui::vec2(32.0, 32.0))));
            }
            let words = match many {
                Some(n) => format!("{name} ×{n}"),
                None => name,
            };
            if ui.link(words).clicked() {
                if let Some(w) = weenie {
                    out.push(UiAction::Weenie(w));
                    out.push(UiAction::ShowWeenie);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's Definition pane)
    use super::*;

    const CREATURE: &str = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (7, 'drudgeskulker', 10, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES (7, 19, True);
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (7, 25, 5) /* Level */
     , (7, 16, 32);
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (7, 1, 0x020007DD);
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES (7, 2, 3, 1, 0, 0, False)
     , (7, 10, 4, 1, 0, 0.5, False)
     , (7, 1, 5, 1, 0, 0, False);
";

    fn field(key: &str) -> &'static Field {
        fields::FIELDS.iter().find(|f| f.key == key).unwrap()
    }

    #[test]
    fn a_fields_value_is_shown_in_plain_words_and_unset_is_nothing_but_a_yes_or_no() {
        let t = crate::text::Text::built_in().table();
        let doc = SqlDoc::parse(CREATURE);
        assert_eq!(plain_words(field("level"), &doc, &t, None), "5");
        assert_eq!(
            plain_words(field("attackable"), &doc, &t, None),
            t.get("fields.yes")
        );
        assert_eq!(plain_words(field("model"), &doc, &t, None), "0x020007DD");
        assert_eq!(
            plain_words(field("usable"), &doc, &t, None),
            t.get("fields.usable_remote")
        );
        // Unset: a yes or no is the server's value, anything else nothing.
        assert_eq!(
            plain_words(field("locked"), &doc, &t, None),
            t.get("fields.no")
        );
        assert_eq!(plain_words(field("scale"), &doc, &t, None), "");
    }

    #[test]
    fn what_it_carries_is_counted_by_destination_a_treasure_wield_among_its_wields() {
        let doc = SqlDoc::parse(CREATURE);
        // A creature drops what it holds and its treasure not wielded.
        assert_eq!(fields::count(&doc, Drill::Wields), 2);
        assert_eq!(fields::count(&doc, Drill::Holds), 1);
        assert_eq!(fields::count(&doc, Drill::Sells), 0);
    }

    #[test]
    fn the_pane_opens_into_a_list_for_its_weenie_only_and_goes_back() {
        let a = Weenie::Class(7);
        let b = Weenie::Class(8);
        let mut d = Definition::default();
        d.drill_into(&a, Drill::Wields);
        assert_eq!(d.drill_of(&a), Some(Drill::Wields));
        assert_eq!(d.drill_of(&b), None, "another weenie shows its body");
        d.back();
        assert_eq!(d.drill_of(&a), None);
    }

    #[test]
    fn every_list_the_pane_names_has_words_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        for d in [
            Drill::Wields,
            Drill::Holds,
            Drill::Sells,
            Drill::Spells,
            Drill::Spawns,
            Drill::Emotes,
            Drill::Skills,
            Drill::BodyParts,
            Drill::Appears,
            Drill::UsedBy,
        ] {
            assert!(t.contains_key(d.key()), "{}", d.key());
        }
    }
}
