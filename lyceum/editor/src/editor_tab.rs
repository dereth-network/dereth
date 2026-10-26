//! The weenie editor: a tab of its own, opened on a weenie and closed with its ×. It opens
//! floating over the world view, can be docked anywhere, and keeps its place outdoors and in
//! dungeon mode alike. It edits one weenie at a time, with Back and Forward through those it has
//! shown, and stays on its weenie while the selection moves. Its menu makes new weenies, finds
//! what names one, and puts one back as saved or, for the pack's change of a server's weenie,
//! back as the server has it. Its footer lists what the checks found in its weenie's file, each
//! finding going to its field when clicked.
//!
//! Edit opens a server's weenie at once, as the server has it. Its first change has the pack
//! write it again by its class id, so every one of it in the world changes; a copy, a new weenie
//! of the pack, is made from the menu instead.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use lyceum_foundry::content::SqlDoc;
use lyceum_foundry::edit::{Op, Weenie};

use super::fields::Page;
use super::kinds::Kind;
use super::{EditorApp, UiAction};
use crate::docks::{self, Tab};

/// The editor's state: the weenies it has shown, and Back and Forward through them.
#[derive(Debug, Clone, Default)]
pub struct WeenieEditor {
    /// Whether its tab is open.
    pub open: bool,
    shown: Vec<Weenie>,
    at: usize,
    /// The field to show when the editor next draws a weenie's form: the weenie, and the field's
    /// key.
    pub focus: Option<(Weenie, String)>,
    /// The editor's Back and Forward, as drawn last.
    pub back_button: Option<egui::Rect>,
    pub forward_button: Option<egui::Rect>,
    /// The page of the weenie's form shown: one of its sections, or Advanced. A weenie without
    /// that section shows its first.
    pub page: Page,
    /// Each page's entry in the list, and each mark of a field changed from the server's (by the
    /// field's key), as drawn last.
    pub page_rects: Vec<(Page, egui::Rect)>,
    pub changed_marks: Vec<(String, egui::Rect)>,
    /// A changed field's Put back, while its menu is open.
    pub put_back_button: Option<egui::Rect>,
    /// Where the head's words were drawn last.
    pub head_words: Option<egui::Rect>,
    /// New Weenie, the dialog its menu opens.
    pub creator: super::creator::Creator,
    /// What is typed into an open picker's search, and where its box was drawn while it was
    /// open.
    pub picker_search: String,
    pub picker_search_field: Option<egui::Rect>,
    /// As drawn last: each picker's button and each row of an open one (by what it writes), and
    /// each list row's buttons (by the field's key and what the button does).
    pub add_buttons: Vec<(String, egui::Rect)>,
    pub picker_rows: Vec<(String, egui::Rect)>,
    pub row_buttons: Vec<(String, egui::Rect)>,
    /// Each Equipped and Items row's mark of whether it drops when the creature dies, by the row's
    /// key, as drawn last.
    pub drop_marks: Vec<(String, bool)>,
    /// Each field's link to the tab of what it names, by the field's key.
    pub links: Vec<(String, egui::Rect)>,
    /// Advanced's folds as drawn last, by their table or their properties' type: each heading,
    /// and how far open it is (0 closed, 1 open, between while it opens or closes).
    pub folds: Vec<(String, egui::Rect, f32)>,
    /// Whether Advanced shows everything, what the other pages show too; and its switch, as
    /// drawn last.
    pub advanced_all: bool,
    pub show_all_switch: Option<egui::Rect>,
    /// Each finding of the footer, by its line, as drawn last.
    pub finding_rows: Vec<(usize, egui::Rect)>,
    /// Its menu's button and the menu's Delete, as drawn last.
    pub menu_button: Option<egui::Rect>,
    pub delete_button: Option<egui::Rect>,
    /// The Model and Icon pickers.
    pub pickers: super::pickers::Pickers,
    /// The weenies' rows as last read, each read once for each change.
    pub(super) docs: Docs,
    /// The weenie pickers' matches as last found: for which search, of how many weenies; and
    /// the spells by name, of how many.
    pub(super) picker_found: Option<(String, usize, Rc<Vec<usize>>)>,
    pub(super) spell_list: Option<(usize, Rc<Vec<SpellLine>>)>,
}

/// A spell as a picker lists it: its id, its name, and its name in lower case.
pub(super) type SpellLine = (u32, String, String);

/// How many weenies' rows [`Docs`] holds of each kind.
const DOCS_HELD: usize = 8;

/// The weenies' rows as last read: the packs' files by their text, the server's classes by their
/// ids (from the database they were read from), the most recent first.
#[derive(Debug, Clone, Default)]
pub struct Docs {
    files: Vec<(PathBuf, String, Rc<SqlDoc>)>,
    server: Vec<(u32, Rc<SqlDoc>)>,
    server_from: Option<PathBuf>,
}

impl WeenieEditor {
    /// The weenie it edits.
    #[must_use]
    pub fn weenie(&self) -> Option<&Weenie> {
        self.shown.get(self.at)
    }

    /// `w` shown next: those shown after the one shown now are let go, as a browser's history.
    pub fn show(&mut self, w: Weenie) {
        if self.weenie() == Some(&w) {
            return;
        }
        if !self.shown.is_empty() {
            self.shown.truncate(self.at + 1);
        }
        self.shown.push(w);
        self.at = self.shown.len() - 1;
    }

    #[must_use]
    pub fn can_go_back(&self) -> bool {
        self.at > 0
    }

    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        self.at + 1 < self.shown.len()
    }

    pub fn back(&mut self) {
        if self.can_go_back() {
            self.at -= 1;
        }
    }

    pub fn forward(&mut self) {
        if self.can_go_forward() {
            self.at += 1;
        }
    }

    /// `w` let go of, as gone from the world: out of the weenies shown, the editor on the one
    /// shown before it (or on nothing).
    pub fn forget(&mut self, w: &Weenie) {
        let shown = self.weenie().cloned();
        let before = self.shown[..self.at.min(self.shown.len())]
            .iter()
            .filter(|x| *x == w)
            .count();
        self.shown.retain(|x| x != w);
        self.at = self.at.saturating_sub(before);
        if shown.as_ref() == Some(w) {
            self.at = self.at.saturating_sub(1);
        }
        self.at = self.at.min(self.shown.len().saturating_sub(1));
    }

    /// Whether field `key` of weenie `w` is the one to show.
    #[must_use]
    pub fn focus_on(&self, w: &Weenie, key: &str) -> bool {
        self.focus
            .as_ref()
            .is_some_and(|(fw, fk)| fw == w && fk == key)
    }
}

/// Whose a weenie is, as its head says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    /// The server's own: no pack writes it.
    Server,
    /// A server's class a pack writes again (`mine`: the pack being edited).
    ChangedIn { pack: String, mine: bool },
    /// A pack's own weenie (`mine`: the pack being edited's).
    Pack { pack: String, mine: bool },
    /// A pack's weenie no file writes.
    Unwritten,
}

impl Owner {
    /// Whether the pack being edited may change it (the server's, by writing it again).
    #[must_use]
    pub fn editable(&self) -> bool {
        match self {
            Self::Server => true,
            Self::ChangedIn { mine, .. } | Self::Pack { mine, .. } => *mine,
            Self::Unwritten => false,
        }
    }

    /// Its words, from `t`.
    #[must_use]
    pub fn words(&self, t: &crate::text::Table) -> String {
        match self {
            Self::Server => t.get("definition.the_servers").to_owned(),
            Self::ChangedIn { pack, .. } => t.fill("definition.changed_in", &[("pack", pack)]),
            Self::Pack { pack, mine: true } => t.fill("definition.packs_own", &[("pack", pack)]),
            Self::Pack { pack, mine: false } => t.fill("definition.packs", &[("pack", pack)]),
            Self::Unwritten => t.get("weenie.no_sql").to_owned(),
        }
    }
}

impl EditorApp {
    /// The rows of the pack's file `path` as they stand, read again only once its text changes.
    pub(super) fn file_doc(&mut self, path: &Path) -> Option<Rc<SqlDoc>> {
        let text = self.session.sql_text(path)?;
        let docs = &mut self.editor.docs;
        if let Some(i) = docs
            .files
            .iter()
            .position(|(p, t, _)| p == path && *t == text)
        {
            return Some(Rc::clone(&docs.files[i].2));
        }
        let doc = Rc::new(SqlDoc::parse(&text));
        docs.files.retain(|(p, _, _)| p != path);
        docs.files
            .insert(0, (path.to_path_buf(), text, Rc::clone(&doc)));
        docs.files.truncate(DOCS_HELD);
        Some(doc)
    }

    /// The server's class `class`'s rows, read once: `None` while they are being read,
    /// `Some(None)` for a class the server's database lacks.
    pub(super) fn server_doc_of(&mut self, class: u32) -> Option<Option<Rc<SqlDoc>>> {
        if self.editor.docs.server_from != self.server_sql {
            self.editor.docs.server.clear();
            self.editor.docs.server_from.clone_from(&self.server_sql);
        }
        if let Some((_, d)) = self.editor.docs.server.iter().find(|(c, _)| *c == class) {
            return Some(Some(Rc::clone(d)));
        }
        let Some(text) = self.server_class(class)? else {
            return Some(None);
        };
        let doc = Rc::new(SqlDoc::parse(&text));
        let held = &mut self.editor.docs.server;
        held.insert(0, (class, Rc::clone(&doc)));
        held.truncate(DOCS_HELD);
        Some(Some(doc))
    }

    /// Weenie `w`'s rows as they stand: the pack's file of it, else the server's class (`None`
    /// while that is being read, `Some(None)` for none).
    pub(super) fn weenie_doc(&mut self, w: &Weenie) -> Option<Option<Rc<SqlDoc>>> {
        match (self.session.weenie_file(w), w) {
            (Some((f, _)), _) => Some(self.file_doc(&f)),
            (None, Weenie::Class(c)) => self.server_doc_of(*c),
            (None, Weenie::Symbol(_)) => Some(None),
        }
    }

    /// Whose weenie `w` is.
    #[must_use]
    pub fn owner_of(&self, w: &Weenie) -> Owner {
        let Some((file, mine)) = self.session.weenie_file(w) else {
            return match w {
                Weenie::Class(_) => Owner::Server,
                Weenie::Symbol(_) => Owner::Unwritten,
            };
        };
        let pack = self
            .session
            .packs()
            .iter()
            .find(|p| p.sql.iter().any(|f| f.path == file))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        match w {
            Weenie::Class(_) => Owner::ChangedIn { pack, mine },
            Weenie::Symbol(_) => Owner::Pack { pack, mine },
        }
    }

    /// Edit pressed on weenie `w` (at field `field`, when one was asked for): the editor opened
    /// on it at once. A server's weenie no pack writes is shown as the server has it, and its
    /// first change makes the pack's copy.
    pub fn edit_weenie(&mut self, w: &Weenie, field: Option<String>) {
        self.editor.focus = field.map(|f| (w.clone(), f));
        self.open_editor(w.clone());
    }

    /// The editor opened on weenie `w`, brought forward.
    pub fn open_editor(&mut self, w: Weenie) {
        self.editor.show(w);
        self.editor.open = true;
        self.place_editor_tab();
        self.show_tab(Tab::Editor);
    }

    /// The editor's tab in the layout shown now when it is open, floating over the world view
    /// if the layout lacks it; out of it when it is closed.
    pub(super) fn place_editor_tab(&mut self) {
        let in_dungeon = self.dungeon.is_some();
        let kept = if in_dungeon {
            self.dungeon_dock.take()
        } else {
            self.dock.take()
        };
        let Some(mut dock) = kept else { return };
        let _ = self.editor_tab_in(&mut dock);
        if in_dungeon {
            self.dungeon_dock = Some(dock);
        } else {
            self.dock = Some(dock);
        }
    }

    /// The editor's tab in `dock` when it is open, floating over the world view if `dock` lacks
    /// it (once the world view is laid out, so it opens over it, or over a tab shown beside it);
    /// out of `dock` when it is closed. Answers whether `dock` holds it. Open in a window a
    /// playtest hid, it is left there.
    pub(super) fn editor_tab_in(&self, dock: &mut egui_dock::DockState<Tab>) -> bool {
        if self.editor.open && self.hides(Tab::Editor) {
            return false;
        }
        let world = self.world_rect.or_else(|| docks::body_of(dock, Tab::World));
        match (self.editor.open, world) {
            (true, Some(world)) => {
                docks::open_floating(dock, Tab::Editor, docks::editor_rect(world));
            }
            (true, None) => {}
            (false, _) => docks::close(dock, Tab::Editor),
        }
        docks::holds(dock, Tab::Editor)
    }

    /// Weenie `w`'s file put back as it was last saved, as one step: a file never saved is taken
    /// away.
    ///
    /// # Errors
    /// No file of the pack being edited writes it.
    pub fn put_back_saved(&mut self, w: &Weenie) -> Result<(), String> {
        let (file, mine) = self
            .session
            .weenie_file(w)
            .ok_or_else(|| self.say("messages.no_sql_writes_it"))?;
        if !mine {
            return Err(self.say("messages.another_packs_weenie"));
        }
        if !self.session.sql_unsaved(&file) {
            return Ok(());
        }
        let text = self.session.saved_sql_text(&file);
        self.apply(&Op::WriteSql { file, text }, false)?;
        Ok(())
    }

    /// The pack's change of server class `class` taken away, as one step: the server's class
    /// stands again everywhere.
    ///
    /// # Errors
    /// The pack being edited does not change it.
    pub fn put_back_server(&mut self, class: u32) -> Result<(), String> {
        let (file, mine) = self
            .session
            .weenie_file(&Weenie::Class(class))
            .ok_or_else(|| self.say("messages.not_changed_here"))?;
        if !mine {
            return Err(self.say("messages.another_packs_weenie"));
        }
        self.apply(&Op::WriteSql { file, text: None }, false)?;
        Ok(())
    }

    /// The pack being edited's own weenie `w` deleted, as one step: refused while anything names
    /// it, the Definition pane then listing what does. The editor, the pane and the spawn tool let
    /// go of it.
    ///
    /// # Errors
    /// One of the server's weenies, another pack's, or one still named anywhere.
    pub fn delete_weenie(&mut self, w: &Weenie) -> Result<(), String> {
        let named = self.session.named_anywhere(w);
        if !named.is_empty() {
            let name = self.weenie_entry(w).map_or_else(
                || super::form::weenie_label(w, &self.text.table()),
                |e| e.name.clone(),
            );
            self.find_usages(w);
            return Err(self.say_with(
                "messages.still_used",
                &[("name", &name), ("n", &named.places())],
            ));
        }
        let op = self.session.weenie_removal(w).map_err(|e| e.to_string())?;
        self.apply(&op, false)?;
        self.editor.forget(w);
        if self.editor.weenie().is_none() {
            self.editor.open = false;
            self.place_editor_tab();
        }
        if self.panels.weenie.as_ref() == Some(w) {
            self.panels.weenie = None;
        }
        Ok(())
    }

    /// The editor's tab: its weenie's head, Back and Forward and the menu, and the form.
    pub(super) fn editor_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        self.form_fields.clear();
        self.editor.changed_marks.clear();
        self.editor.put_back_button = None;
        self.editor.add_buttons.clear();
        self.editor.picker_rows.clear();
        self.editor.picker_search_field = None;
        self.editor.folds.clear();
        self.editor.row_buttons.clear();
        self.editor.drop_marks.clear();
        self.editor.show_all_switch = None;
        self.editor.links.clear();
        self.editor.pickers.clear_drawn();
        self.appearance.clear_drawn();
        let w = self.editor.weenie().cloned();
        let entry = w.as_ref().and_then(|w| self.weenie_entry(w)).cloned();
        let owner = w.as_ref().map(|w| self.owner_of(w));
        let file = w.as_ref().and_then(|w| self.session.weenie_file(w));
        let unsaved = file
            .as_ref()
            .is_some_and(|(f, _)| self.session.sql_unsaved(f));
        // What it is, read from its file or the server's class (`None` while that is being read).
        let doc = w.as_ref().and_then(|w| self.weenie_doc(w));
        let reading = w.is_some() && doc.is_none();
        let kind = doc.flatten().map(|d| Kind::of(&d));
        self.editor.head_words = None;
        ui.horizontal(|ui| {
            // The weenie as the game shows it, turning.
            if let Some(w) = &w {
                let has_setup = entry.as_ref().is_some_and(|e| e.setup.is_some());
                self.look_preview(ui, w, has_setup);
            }
            // The menu, Forward and Back first, from the right; the words take what is left, cut
            // short before them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.editor_menu(ui, w.as_ref(), owner.as_ref(), unsaved, out);
                let fwd = ui
                    .add_enabled(self.editor.can_go_forward(), egui::Button::new("›"))
                    .on_hover_text(t.get("editor.forward"));
                self.editor.forward_button = Some(fwd.rect);
                if fwd.clicked() {
                    out.push(UiAction::EditorForward);
                }
                let back = ui
                    .add_enabled(self.editor.can_go_back(), egui::Button::new("‹"))
                    .on_hover_text(t.get("editor.back"));
                self.editor.back_button = Some(back.rect);
                if back.clicked() {
                    out.push(UiAction::EditorBack);
                }
                let room = egui::vec2(
                    ui.available_width().max(0.0),
                    super::definition::HEAD_HEIGHT,
                );
                let words = ui.allocate_ui_with_layout(
                    room,
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width(room.x);
                        let name = match (&entry, &w) {
                            (Some(e), _) => e.name.as_str(),
                            (None, Some(_)) => t.get("weenie.a_weenie"),
                            (None, None) => t.get("editor.nothing"),
                        };
                        let mut drawn = ui
                            .add(egui::Label::new(egui::RichText::new(name).strong()).truncate())
                            .rect;
                        if let (Some(w), Some(owner)) = (&w, &owner) {
                            let mut line = Vec::new();
                            if let Some(k) = kind {
                                line.push(t.get(&format!("kinds.{}", k.key())).to_owned());
                            }
                            line.push(super::form::weenie_label(w, &t));
                            line.push(owner.words(&t));
                            if unsaved {
                                line.push(t.get("editor.not_saved").to_owned());
                            }
                            let r = ui.add(
                                egui::Label::new(egui::RichText::new(line.join(" · ")).weak())
                                    .truncate(),
                            );
                            drawn = drawn.union(r.rect);
                        }
                        drawn
                    },
                );
                self.editor.head_words = Some(words.inner);
            });
        });
        ui.separator();
        if w.is_none() {
            self.editor.finding_rows.clear();
        }
        match w {
            Some(w) => {
                self.findings_footer(ui, &w, out);
                self.form_body(ui, &w, out);
                // The field asked for is shown once, or not at all when it is folded away; kept
                // while the server's class is being read, to be shown once it is.
                if !reading && self.editor.focus.as_ref().is_some_and(|(fw, _)| *fw == w) {
                    self.editor.focus = None;
                }
            }
            None => {
                ui.weak(t.get("editor.empty"));
                if ui.button(t.get("creator.open")).clicked() {
                    out.push(UiAction::NewWeenie(None));
                }
            }
        }
    }

    /// The editor's menu: New Weenie, Make a copy, Find usages, the two ways to put a weenie
    /// back, and for the pack's own, Delete.
    fn editor_menu(
        &mut self,
        ui: &mut egui::Ui,
        w: Option<&Weenie>,
        owner: Option<&Owner>,
        unsaved: bool,
        out: &mut Vec<UiAction>,
    ) {
        use egui::containers::menu::MenuButton;
        let t = self.text.table();
        let class = match w {
            Some(Weenie::Class(c)) => Some(*c),
            _ => None,
        };
        let (menu, _) = MenuButton::new("…").ui(ui, |ui| {
            if ui.button(t.get("creator.open")).clicked() {
                out.push(UiAction::NewWeenie(None));
            }
            if let Some(w) = w {
                if ui.button(t.get("definition.make_a_copy")).clicked() {
                    out.push(UiAction::NewWeenie(Some(w.clone())));
                }
                if ui
                    .button(t.get("definition.find_usages"))
                    .on_hover_text(t.get("definition.find_usages_about"))
                    .clicked()
                {
                    out.push(UiAction::FindUsages(w.clone()));
                }
            }
            if let Some(w) = w {
                let mine = owner.is_some_and(|o| o.editable() && *o != Owner::Server);
                if ui
                    .add_enabled(
                        mine && unsaved,
                        egui::Button::new(t.get("editor.put_back_saved")),
                    )
                    .on_hover_text(t.get("editor.put_back_saved_about"))
                    .clicked()
                {
                    out.push(UiAction::PutBackSaved(w.clone()));
                }
                if let (Some(c), Some(Owner::ChangedIn { mine: true, .. })) = (class, owner) {
                    if ui
                        .button(t.get("definition.put_back_server"))
                        .on_hover_text(t.get("definition.put_back_server_about"))
                        .clicked()
                    {
                        out.push(UiAction::PutBackServer(c));
                    }
                }
                if matches!(owner, Some(Owner::Pack { mine: true, .. })) {
                    let r = ui.button(t.get("definition.delete"));
                    self.editor.delete_button = Some(r.rect);
                    if r.clicked() {
                        out.push(UiAction::DeleteWeenie(w.clone()));
                    }
                }
            }
        });
        self.editor.menu_button = Some(menu.rect);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's weenie editor)
    use super::*;

    #[test]
    fn back_and_forward_go_through_the_weenies_shown_and_a_new_one_lets_go_of_those_ahead() {
        let a = Weenie::Class(7);
        let b = Weenie::Symbol("holtburg:guide".into());
        let c = Weenie::Class(371);
        let mut e = WeenieEditor::default();
        assert!(e.weenie().is_none() && !e.can_go_back() && !e.can_go_forward());
        e.show(a.clone());
        e.show(b.clone());
        e.show(b.clone());
        assert_eq!(e.weenie(), Some(&b));
        e.back();
        assert_eq!(e.weenie(), Some(&a));
        assert!(!e.can_go_back() && e.can_go_forward());
        e.back();
        assert_eq!(e.weenie(), Some(&a), "nothing before the first");
        e.forward();
        assert_eq!(e.weenie(), Some(&b));
        e.back();
        // Shown from the first, the second is let go.
        e.show(c.clone());
        assert_eq!(e.weenie(), Some(&c));
        assert!(!e.can_go_forward());
        e.back();
        assert_eq!(e.weenie(), Some(&a));
    }

    #[test]
    fn a_weenie_let_go_of_leaves_the_one_shown_before_it_and_back_and_forward_skip_it() {
        let (a, b, c) = (
            Weenie::Class(7),
            Weenie::Symbol("holtburg:guide".into()),
            Weenie::Class(371),
        );
        let mut e = WeenieEditor::default();
        for w in [&a, &b, &c, &b] {
            e.show(w.clone());
        }
        // Shown last, and earlier too: both go, and the one before the last is shown.
        e.forget(&b);
        assert_eq!(e.weenie(), Some(&c));
        e.back();
        assert_eq!(e.weenie(), Some(&a));
        assert!(!e.can_go_back());
        // One not shown now goes from the history alone.
        e.forget(&c);
        assert_eq!(e.weenie(), Some(&a));
        assert!(!e.can_go_forward());
        e.forget(&a);
        assert_eq!(e.weenie(), None);
    }

    #[test]
    fn whose_a_weenie_is_says_whether_the_pack_being_edited_may_change_it() {
        let t = crate::text::Table::default();
        let mine = |m| Owner::ChangedIn {
            pack: "holtburg".into(),
            mine: m,
        };
        assert!(Owner::Server.editable());
        assert!(mine(true).editable() && !mine(false).editable());
        assert!(!Owner::Unwritten.editable());
        assert!(Owner::Pack {
            pack: "holtburg".into(),
            mine: true
        }
        .editable());
        for o in [
            Owner::Server,
            mine(true),
            Owner::Pack {
                pack: "holtburg".into(),
                mine: false,
            },
        ] {
            assert!(!o.words(&t).is_empty(), "{o:?}");
        }
    }
}
