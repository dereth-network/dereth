//! New Weenie: a small dialog over the window that makes a weenie of the pack being edited and
//! opens the editor on it, as the spawn tool's choice to place. It takes a name, what the weenie
//! is, and what it starts from: a plain one of that kind, or a copy of any weenie (the one shown
//! first, the rest of that kind by picture and name).
//!
//! A plain one is a blank: a class of the server's of that kind, its emotes, inventory, generator
//! profiles, books and words taken out, then the kind's first changes. A copy is every row of the
//! weenie copied, the server's as the server has it and a pack's as its file writes it, under the
//! new name. Either is one step of the history.

use std::path::PathBuf;

use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::{Op, Weenie};
use lyceum_foundry::models::Category;

use super::flows::{symbol_of, Flow};
use super::form::{apply_edit, weenie_label, FormEdit};
use super::kinds::Kind;
use super::{EditorApp, UiAction, WeenieEntry};

/// The weenie types the copy lists tell apart, as the server numbers them.
mod wt {
    pub const GENERIC: u32 = 1;
    pub const SWITCH: u32 = 26;
}

/// What a new weenie is made as: the kinds the dialog offers, each with the blank it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewKind {
    #[default]
    Talker,
    QuestGiver,
    Vendor,
    Creature,
    Item,
    MeleeWeapon,
    Armor,
    Clothing,
    Door,
    Chest,
    Portal,
    Sign,
    Switch,
    Generator,
}

impl NewKind {
    /// Every kind, in the dialog's order: who, what is carried, what stands in the world.
    pub const ALL: [Self; 14] = [
        Self::Talker,
        Self::QuestGiver,
        Self::Vendor,
        Self::Creature,
        Self::Item,
        Self::MeleeWeapon,
        Self::Armor,
        Self::Clothing,
        Self::Door,
        Self::Chest,
        Self::Portal,
        Self::Sign,
        Self::Switch,
        Self::Generator,
    ];

    /// Its key in the text: `creator.kind_<key>`, `creator.plain_<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Talker => "talker",
            Self::QuestGiver => "quest_giver",
            Self::Vendor => "vendor",
            Self::Creature => "creature",
            Self::Item => "item",
            Self::MeleeWeapon => "melee_weapon",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
            Self::Door => "door",
            Self::Chest => "chest",
            Self::Portal => "portal",
            Self::Sign => "sign",
            Self::Switch => "switch",
            Self::Generator => "generator",
        }
    }

    /// What the weenie's form shows it as, for the kinds that have a form of their own.
    #[must_use]
    pub const fn flow(self) -> Option<Flow> {
        match self {
            Self::Talker => Some(Flow::Talker),
            Self::QuestGiver => Some(Flow::QuestGiver),
            Self::Vendor => Some(Flow::Vendor),
            Self::Creature => Some(Flow::Creature),
            Self::Item => Some(Flow::Item),
            Self::Door => Some(Flow::Door),
            Self::Chest => Some(Flow::Chest),
            Self::Portal => Some(Flow::Portal),
            _ => None,
        }
    }

    /// What the weenie it makes is: a sign is an item that stands in the world.
    #[must_use]
    pub const fn kind(self) -> Kind {
        match self.flow() {
            Some(f) => f.kind(),
            None => match self {
                Self::MeleeWeapon => Kind::MeleeWeapon,
                Self::Armor => Kind::Armor,
                Self::Clothing => Kind::Clothing,
                Self::Switch => Kind::Switch,
                Self::Generator => Kind::Generator,
                _ => Kind::Item,
            },
        }
    }

    /// The server's class a plain one is a blank of.
    #[must_use]
    pub const fn blank(self) -> u32 {
        match self.flow() {
            Some(f) => f.blank(),
            None => match self {
                Self::MeleeWeapon => 351,
                Self::Armor => 50,
                Self::Clothing => 130,
                Self::Sign => 459,
                Self::Switch => 285,
                _ => 381,
            },
        }
    }

    /// The spawn browser's category of its kind.
    #[must_use]
    pub const fn category(self) -> Category {
        match self {
            Self::Talker | Self::QuestGiver | Self::Vendor => Category::Npc,
            Self::Creature => Category::Creature,
            Self::Item | Self::MeleeWeapon | Self::Armor | Self::Clothing => Category::Item,
            Self::Door => Category::Door,
            Self::Chest => Category::Chest,
            Self::Portal => Category::Portal,
            Self::Sign | Self::Switch | Self::Generator => Category::Object,
        }
    }

    /// Whether weenie `e` is listed among the copies of this kind: a switch, a sign and a
    /// generator by their weenie type (a sign and a generator are both generic weenies, a
    /// generator one that makes things, a sign one fixed where it stands), the rest by their
    /// category.
    #[must_use]
    pub fn lists(self, e: &WeenieEntry) -> bool {
        match self {
            Self::Switch => e.weenie_type == Some(wt::SWITCH),
            Self::Generator => e.weenie_type == Some(wt::GENERIC) && e.generates,
            Self::Sign => {
                e.weenie_type == Some(wt::GENERIC) && e.category == Category::Object && !e.generates
            }
            _ => e.category == self.category(),
        }
    }

    /// The kind weenie `e` is copied as: the first that lists it.
    #[must_use]
    pub fn of_entry(e: &WeenieEntry) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.lists(e))
    }

    /// The changes a plain one makes to its blank after its name (`words`), its words from `t`.
    #[must_use]
    pub fn start(self, words: &str, t: &crate::text::Table) -> Vec<FormEdit> {
        if let Some(f) = self.flow() {
            return super::flows::start(f, t);
        }
        match self {
            // A sign reads what it is named, as the server's own do.
            Self::Sign => vec![FormEdit::Property {
                kind: PropertyKind::String,
                id: 16,
                value: Some(words.to_owned()),
            }],
            // The blank of each of the rest is a plain one already.
            _ => Vec::new(),
        }
    }
}

/// The dialog's state: open or not, what is typed and chosen in it, and what went wrong last.
#[derive(Debug, Clone, Default)]
pub struct Creator {
    pub open: bool,
    pub name: String,
    pub kind: NewKind,
    /// Whether it starts from a copy (of `of`) rather than a plain one.
    pub copy: bool,
    pub of: Option<Weenie>,
    /// The copy list's search.
    pub search: String,
    /// The weenie shown when the dialog opened: first in the copy list.
    pub shown: Option<Weenie>,
    /// Why the last Make it made nothing.
    pub problem: Option<String>,
    /// The name field takes the keys when the dialog opens.
    focus: bool,
    /// Its fields, choices, list rows and buttons, as drawn last.
    pub name_field: Option<egui::Rect>,
    pub plain_button: Option<egui::Rect>,
    pub copy_button: Option<egui::Rect>,
    pub rows: Vec<(Weenie, egui::Rect)>,
    pub make_button: Option<egui::Rect>,
    pub cancel_button: Option<egui::Rect>,
}

/// `text`, a pack's file of a weenie, without the comment it starts with: what the file is, which
/// a copy says again of itself.
fn without_head(text: &str) -> &str {
    let t = text.trim_start();
    if !t.starts_with("/*") {
        return text;
    }
    t.find("*/")
        .map_or(text, |end| t[end + 2..].trim_start_matches(['\r', '\n']))
}

impl EditorApp {
    /// The dialog opened: plain, or (`copy_of`) a copy of that weenie, named as it is. The weenie
    /// the editor or the Definition pane shows is first in the copy list.
    pub fn open_creator(&mut self, copy_of: Option<Weenie>) {
        let shown = copy_of
            .clone()
            .or_else(|| self.editor.weenie().cloned())
            .or_else(|| self.form_weenie());
        let entry = shown.as_ref().and_then(|w| self.weenie_entry(w)).cloned();
        let server = self.server_sql.is_some();
        // The server's database read while a name is typed, for a plain one to start from.
        if let Some(path) = self.server_sql.clone() {
            self.server_classes.read_ahead(path);
        }
        let c = &mut self.editor.creator;
        c.open = true;
        c.focus = true;
        c.problem = None;
        c.search.clear();
        c.of.clone_from(&shown);
        c.shown = shown;
        c.copy = copy_of.is_some() || !server;
        c.name.clear();
        // A copy is named as what it copies, and listed among its own kind.
        if let (Some(_), Some(e)) = (&copy_of, entry) {
            c.name.clone_from(&e.name);
            if !c.kind.lists(&e) {
                c.kind = NewKind::of_entry(&e).unwrap_or(c.kind);
            }
        }
    }

    /// Make it pressed: the weenie made as the dialog says, and the dialog closed; or why not,
    /// shown in it.
    ///
    /// # Errors
    /// As [`Self::new_plain`] and [`Self::new_copy`].
    pub fn make_weenie(&mut self) -> Result<Weenie, String> {
        let c = self.editor.creator.clone();
        let made = match (c.copy, &c.of) {
            (false, _) => self.new_plain(c.kind, &c.name),
            (true, Some(of)) => self.new_copy(of, &c.name),
            (true, None) => Err(self.say("creator.choose_copy")),
        };
        match &made {
            Ok(_) => {
                self.editor.creator.open = false;
                self.editor.creator.problem = None;
            }
            Err(e) => self.editor.creator.problem = Some(e.clone()),
        }
        made
    }

    /// A new weenie of the pack being edited, a plain one of `kind` named `words` (in plain words;
    /// its symbol made of them): a blank of its kind with the kind's first changes, as one step.
    /// The editor opens on it, and the spawn tool takes it.
    ///
    /// # Errors
    /// No name, no server database to make it from, or a name taken.
    pub fn new_plain(&mut self, kind: NewKind, words: &str) -> Result<Weenie, String> {
        let words = words.trim();
        let name = symbol_of(words);
        if name.is_empty() {
            return Err(self.say("messages.name_it"));
        }
        let (file, sym) = self.new_file(&name)?;
        let text = self.server_template(kind.blank(), &name, true)?;
        let t = self.text.table();
        let mut edits = vec![FormEdit::Property {
            kind: PropertyKind::String,
            id: 1,
            value: Some(words.to_owned()),
        }];
        edits.extend(kind.start(words, &t));
        // A portal leads into the dungeon last gone into, at its drop point, when there is one;
        // its card chooses another.
        if kind == NewKind::Portal {
            if let Some(lb) = self.dungeon_list.marked {
                if let Ok((cell, at)) = self.drop_point(lb) {
                    edits.push(FormEdit::Destination {
                        cell,
                        at: [at.x, at.y, at.z],
                    });
                }
            }
        }
        let mut doc = SqlDoc::parse(&text);
        for e in &edits {
            if !apply_edit(&mut doc, e, self.schema.as_ref()) {
                return Err(self.say("messages.not_a_value"));
            }
        }
        let from = format!("made from a blank of the server's class {}", kind.blank());
        let w = self.write_new(file, sym, &name, &doc.render(), &from)?;
        // The quest it asks for, registered in the pack with it.
        if kind == NewKind::QuestGiver {
            self.register_quest(t.get("flows.start_quest"), true)?;
        }
        self.made(w.clone());
        Ok(w)
    }

    /// A new weenie of the pack being edited, a copy of `of` named `words` as players will see
    /// it: a server's class as the server has it, a pack's weenie (or a pack's change of a
    /// server's class) as its file writes it. One step; the editor opens on it, and the spawn tool
    /// takes it.
    ///
    /// # Errors
    /// No name, nothing to copy it from, or a name taken.
    pub fn new_copy(&mut self, of: &Weenie, words: &str) -> Result<Weenie, String> {
        let words = words.trim();
        let name = symbol_of(words);
        if name.is_empty() {
            return Err(self.say("messages.name_it"));
        }
        let (file, sym) = self.new_file(&name)?;
        let (text, from) = match (self.session.weenie_file(of), of) {
            (Some((path, _)), _) => {
                let text = self
                    .session
                    .sql_text(&path)
                    .ok_or_else(|| self.say("messages.sql_gone"))?;
                let packs = self.session.packs();
                let pack = packs
                    .iter()
                    .find(|p| p.sql.iter().any(|f| f.path == path))
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                let other = pack != packs[self.session.target()].name;
                let copy =
                    content::renamed(without_head(&text), &name, other.then_some(pack.as_str()))
                        .ok_or_else(|| self.say("messages.no_sql_writes_it"))?;
                let what = match of {
                    Weenie::Class(c) => format!("the server's class {c} as {pack} changes it"),
                    Weenie::Symbol(s) => s.clone(),
                };
                (copy, format!("a copy of {what}"))
            }
            (None, Weenie::Class(c)) => (
                self.server_template(*c, &name, false)?,
                format!("made from a copy of the server's class {c}"),
            ),
            (None, Weenie::Symbol(_)) => return Err(self.say("messages.no_sql_writes_it")),
        };
        let mut doc = SqlDoc::parse(&text);
        let named = FormEdit::Property {
            kind: PropertyKind::String,
            id: 1,
            value: Some(words.to_owned()),
        };
        if !apply_edit(&mut doc, &named, self.schema.as_ref()) {
            return Err(self.say("messages.not_a_value"));
        }
        let w = self.write_new(file, sym, &name, &doc.render(), &from)?;
        self.made(w.clone());
        Ok(w)
    }

    /// A new weenie of the pack being edited, symbol `name`, made from server class `from`
    /// (`blank`: stripped to a blank of its kind); the spawn tool and the form take it.
    ///
    /// # Errors
    /// No server database, no such class, a name that is not a symbol, or one taken.
    pub fn new_weenie(&mut self, name: &str, from: u32, blank: bool) -> Result<(), String> {
        let name = name.trim();
        let (file, sym) = self.new_file(name)?;
        let text = self.server_template(from, name, blank)?;
        let kind = if blank { "a blank" } else { "a copy" };
        let made = format!("made from {kind} of the server's class {from}");
        self.write_new(file, sym, name, &text, &made)?;
        Ok(())
    }

    /// The file and symbol of a new weenie of the pack being edited named `name`.
    ///
    /// # Errors
    /// A name that is not a symbol, or one taken.
    fn new_file(&mut self, name: &str) -> Result<(PathBuf, String), String> {
        let (file, sym) = self
            .session
            .new_weenie_file(name)
            .map_err(|e| e.to_string())?;
        let mine = Weenie::Symbol(sym.clone());
        if self.session.sql_text(&file).is_some() || self.weenies.iter().any(|w| w.weenie == mine) {
            return Err(self.say_with("messages.name_taken", &[("name", &sym)]));
        }
        Ok((file, sym))
    }

    /// Server class `class`'s rows as a new weenie named `name` (`blank`: stripped to a blank).
    ///
    /// # Errors
    /// No server database, or no such class in it.
    fn server_template(&mut self, class: u32, name: &str, blank: bool) -> Result<String, String> {
        let world = self.server_world()?;
        content::template(&world, class, name, blank)
            .ok_or_else(|| self.say_with("messages.no_such_class", &[("id", &class)]))
    }

    /// New weenie `sym`'s file written, `text` under a line saying what it was `made` from, as
    /// one step; the spawn tool takes it.
    fn write_new(
        &mut self,
        file: PathBuf,
        sym: String,
        name: &str,
        text: &str,
        made: &str,
    ) -> Result<Weenie, String> {
        let header = format!("/* {name}: {made}. */\n");
        self.apply(
            &Op::WriteSql {
                file,
                text: Some(header + text),
            },
            false,
        )?;
        let w = Weenie::Symbol(sym);
        self.panels.weenie = Some(w.clone());
        Ok(w)
    }

    /// A weenie just made, shown: the Definition pane and the editor on it, not a selected
    /// spawn's.
    fn made(&mut self, w: Weenie) {
        self.selection = None;
        self.also.clear();
        self.show_weenie();
        self.open_editor(w);
    }

    /// The copy list: the weenie shown when the dialog opened, then every weenie of the kind's
    /// category, by their place in [`Self::weenies`], those answering the search.
    fn copy_list(&self) -> Vec<usize> {
        let c = &self.editor.creator;
        let f = c.search.trim().to_lowercase();
        let kind = c.kind;
        let first = c
            .shown
            .as_ref()
            .and_then(|w| self.weenie_at.get(w).copied());
        let mut out: Vec<usize> = first
            .into_iter()
            .filter(|i| self.weenies[*i].matches(&f))
            .collect();
        out.extend(
            self.weenies
                .iter()
                .enumerate()
                .filter(|(i, w)| Some(*i) != first && kind.lists(w) && w.matches(&f))
                .map(|(i, _)| i),
        );
        out
    }

    /// The dialog, over the whole window, when it is open.
    pub(super) fn creator_dialog(&mut self, ctx: &egui::Context, out: &mut Vec<UiAction>) {
        if !self.editor.creator.open {
            return;
        }
        let t = self.text.table();
        let server = self.server_sql.is_some();
        let shown = egui::Modal::new(egui::Id::new("new weenie")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(t.get("creator.title"));
            ui.add_space(6.0);
            let mut enter = false;
            egui::Grid::new("new weenie")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t.get("creator.name"));
                    let c = &mut self.editor.creator;
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut c.name)
                            .hint_text(t.get("creator.name_hint"))
                            .desired_width(280.0),
                    );
                    c.name_field = Some(r.rect);
                    if std::mem::take(&mut c.focus) {
                        r.request_focus();
                    }
                    if r.changed() {
                        c.problem = None;
                    }
                    enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.end_row();

                    ui.label(t.get("creator.what"));
                    let now = c.kind;
                    egui::ComboBox::from_id_salt("new weenie kind")
                        .selected_text(t.get(&format!("creator.kind_{}", now.key())))
                        .height(400.0)
                        .show_ui(ui, |ui| {
                            for k in NewKind::ALL {
                                let r = ui.selectable_label(
                                    now == k,
                                    t.get(&format!("creator.kind_{}", k.key())),
                                );
                                if r.clicked() {
                                    c.kind = k;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label(t.get("creator.start"));
                    ui.vertical(|ui| {
                        let plain = ui.add_enabled(
                            server,
                            egui::RadioButton::new(
                                !c.copy,
                                t.get(&format!("creator.plain_{}", c.kind.key())),
                            ),
                        );
                        c.plain_button = Some(plain.rect);
                        if plain.clicked() {
                            c.copy = false;
                        }
                        let copy = ui.radio(c.copy, t.get("creator.copy_of"));
                        c.copy_button = Some(copy.rect);
                        if copy.clicked() {
                            c.copy = true;
                        }
                    });
                    ui.end_row();
                });
            self.editor.creator.rows.clear();
            if self.editor.creator.copy {
                self.copy_picker(ui);
            }
            let c = &self.editor.creator;
            // Why Make is not pressed yet, when it is said: a plain one waits for the server's
            // database without a word.
            let why = if symbol_of(&c.name).is_empty() {
                Some("creator.name_first")
            } else if c.copy && c.of.is_none() {
                Some("creator.choose_copy")
            } else {
                None
            };
            let ready = why.is_none() && (c.copy || server);
            if let Some(p) = &c.problem {
                ui.add_space(4.0);
                ui.colored_label(super::panels::mark::FAULT, p);
            }
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let make = ui.add_enabled(ready, egui::Button::new(t.get("creator.make")));
                let make = match why {
                    Some(k) => make.on_disabled_hover_text(t.get(k)),
                    None => make,
                };
                let cancel = ui.button(t.get("creator.cancel"));
                let c = &mut self.editor.creator;
                c.make_button = Some(make.rect);
                c.cancel_button = Some(cancel.rect);
                if make.clicked() || (enter && ready) {
                    out.push(UiAction::MakeWeenie);
                }
                if cancel.clicked() {
                    c.open = false;
                }
            });
        });
        if shown.should_close() {
            self.editor.creator.open = false;
        }
    }

    /// The copy list under the dialog's choices: a search, then each weenie by picture, name
    /// and id, the one chosen marked; a click chooses it.
    fn copy_picker(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        super::panels::search_box(ui, &mut self.editor.creator.search, t.get("creator.search"));
        let list = self.copy_list();
        if list.is_empty() {
            ui.weak(t.get("creator.no_match"));
            return;
        }
        let side = 32.0;
        let mut budget = super::PICTURES_A_FRAME;
        egui::ScrollArea::vertical()
            .id_salt("new weenie copy of")
            .max_height(260.0)
            .min_scrolled_height(220.0)
            .auto_shrink([false, true])
            .show_rows(ui, side + 4.0, list.len(), |ui, range| {
                for i in range {
                    let e = self.weenies[list[i]].clone();
                    let picture = e
                        .setup
                        .and_then(|s| self.picture(dereth_primitives::DataId(s), &mut budget));
                    ui.horizontal(|ui| {
                        match picture {
                            Some(id) => {
                                ui.add(egui::Image::new((id, egui::vec2(side, side))));
                            }
                            None => {
                                let _ = ui.allocate_exact_size(
                                    egui::vec2(side, side),
                                    egui::Sense::hover(),
                                );
                            }
                        }
                        let c = &mut self.editor.creator;
                        let chosen = c.of.as_ref() == Some(&e.weenie);
                        let r = ui.selectable_label(chosen, &e.name);
                        ui.weak(weenie_label(&e.weenie, &t));
                        if r.clicked() {
                            c.of = Some(e.weenie.clone());
                        }
                        c.rows.push((e.weenie, r.rect));
                    });
                }
            });
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's New Weenie dialog)
    use super::*;

    #[test]
    fn every_kind_has_a_name_and_a_plain_choice_and_the_dialog_its_words() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let mut missing = Vec::new();
        for k in NewKind::ALL {
            for key in [
                format!("creator.kind_{}", k.key()),
                format!("creator.plain_{}", k.key()),
            ] {
                if !t.contains_key(&key) {
                    missing.push(key);
                }
            }
        }
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn every_kind_starts_from_a_blank_the_server_has_in_both_eras_and_every_blank_is_offered() {
        // Each kind's blank is of its weenie type (and item type, where that tells it apart). A
        // vendor is an NPC's blank, made a vendor by its first change.
        let want = |k: NewKind| match k {
            NewKind::Talker | NewKind::QuestGiver | NewKind::Vendor | NewKind::Creature => {
                (10, None)
            }
            NewKind::Item | NewKind::Sign | NewKind::Generator => (1, None),
            NewKind::MeleeWeapon => (6, Some(1)),
            NewKind::Armor => (2, Some(2)),
            NewKind::Clothing => (2, Some(4)),
            NewKind::Door => (19, None),
            NewKind::Chest => (20, None),
            NewKind::Portal => (7, None),
            NewKind::Switch => (26, None),
        };
        for k in NewKind::ALL {
            let b = content::BLANKS
                .iter()
                .find(|b| b.class == k.blank())
                .unwrap_or_else(|| panic!("{k:?}: class {} is no blank", k.blank()));
            assert_eq!((b.weenie_type, b.item_type), want(k), "{k:?}");
        }
        for b in content::BLANKS {
            assert!(
                NewKind::ALL.iter().any(|k| k.blank() == b.class),
                "{} is offered by no kind",
                b.kind
            );
        }
        // The kinds the form shows start as they always have.
        for f in Flow::ALL {
            let k = NewKind::ALL
                .into_iter()
                .find(|k| k.flow() == Some(f))
                .unwrap();
            assert_eq!(k.blank(), f.blank(), "{f:?}");
        }
    }

    #[test]
    fn a_switch_a_sign_and_a_generator_list_their_own_kind_and_the_rest_their_category() {
        use lyceum_foundry::sql_ids::WeenieFacts;
        let entry = |kind: u32, stuck: bool, attackable: Option<bool>, generates: bool| {
            let f = WeenieFacts {
                name: Some("x".into()),
                kind: Some(kind),
                stuck: Some(stuck),
                attackable,
                generates,
                ..WeenieFacts::default()
            };
            WeenieEntry::new(Weenie::Class(1), &f, false)
        };
        let lever = entry(26, true, None, false);
        let generator = entry(1, true, None, true);
        let sign = entry(1, true, None, false);
        let crier = entry(10, true, Some(false), false);
        let gem = entry(1, false, None, false);
        let listed = |e: &WeenieEntry| -> Vec<NewKind> {
            NewKind::ALL.into_iter().filter(|k| k.lists(e)).collect()
        };
        // Each of the three in its own list alone, though all are fixed in the world.
        assert_eq!(listed(&lever), [NewKind::Switch]);
        assert_eq!(listed(&generator), [NewKind::Generator]);
        assert_eq!(listed(&sign), [NewKind::Sign]);
        // The rest by their category: an NPC among every kind of person, a gem among the things
        // carried.
        assert_eq!(
            listed(&crier),
            [NewKind::Talker, NewKind::QuestGiver, NewKind::Vendor]
        );
        assert_eq!(
            listed(&gem),
            [
                NewKind::Item,
                NewKind::MeleeWeapon,
                NewKind::Armor,
                NewKind::Clothing
            ]
        );
        // A weenie copied chooses the first kind that lists it.
        for (e, k) in [
            (&lever, NewKind::Switch),
            (&generator, NewKind::Generator),
            (&sign, NewKind::Sign),
            (&crier, NewKind::Talker),
            (&gem, NewKind::Item),
        ] {
            assert_eq!(NewKind::of_entry(e), Some(k));
        }
        assert_eq!(NewKind::of_entry(&entry(25, true, None, false)), None);
    }

    #[test]
    fn the_kind_each_new_kind_makes_is_the_kind_its_weenie_shows_as() {
        assert_eq!(NewKind::QuestGiver.kind(), Kind::Npc);
        assert_eq!(NewKind::Vendor.kind(), Kind::Vendor);
        assert_eq!(NewKind::MeleeWeapon.kind(), Kind::MeleeWeapon);
        assert_eq!(NewKind::Armor.kind(), Kind::Armor);
        assert_eq!(NewKind::Clothing.kind(), Kind::Clothing);
        assert_eq!(NewKind::Generator.kind(), Kind::Generator);
        assert_eq!(NewKind::Switch.kind(), Kind::Switch);
        // A sign is an item fixed where it stands.
        assert_eq!(NewKind::Sign.kind(), Kind::Item);
    }

    #[test]
    fn a_plain_sign_reads_its_name_and_the_new_kinds_start_as_their_blanks() {
        let t = crate::text::Table::default();
        assert_eq!(
            NewKind::Sign.start("Holtburg: 2 miles", &t),
            [FormEdit::Property {
                kind: PropertyKind::String,
                id: 16,
                value: Some("Holtburg: 2 miles".into()),
            }]
        );
        for k in [
            NewKind::MeleeWeapon,
            NewKind::Armor,
            NewKind::Clothing,
            NewKind::Switch,
            NewKind::Generator,
        ] {
            assert!(k.start("x", &t).is_empty(), "{k:?}");
        }
        // The vendor's first change makes it one.
        assert!(NewKind::Vendor
            .start("x", &t)
            .contains(&FormEdit::WeenieType(12)));
    }

    #[test]
    fn a_copy_leaves_out_the_comment_its_file_starts_with_and_keeps_the_rest() {
        let file = "/* Gribble, a drudge. */\nDELETE FROM `weenie` WHERE `class_Id` = 7;\n";
        assert_eq!(
            without_head(file),
            "DELETE FROM `weenie` WHERE `class_Id` = 7;\n"
        );
        let bare = "DELETE FROM `weenie` WHERE `class_Id` = 7;\n/* later */\n";
        assert_eq!(without_head(bare), bare);
    }
}
