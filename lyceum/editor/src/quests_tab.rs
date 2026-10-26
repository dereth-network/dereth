//! The Quests tab: every quest of the world, the packs' and the server's, and those something
//! names that nothing registers, searched by name, or only the pack being edited's and those
//! nothing registers. Each row says whose the quest is, how often it
//! can be done and how many weenies use it; a click opens it: its row (the pack's own editable,
//! as the weenie form's Quests fold shows it), its changes, and every weenie that names it, a
//! click on which shows that weenie in the Definition pane and opens it in the editor; a server's
//! weenie is asked about first, as Edit asks, since the editor writes it into the pack at its
//! first change.
//!
//! A server's quest is changed in this world by a copy of its row in the pack under its own name,
//! which the pack then edits, and put back by taking that copy away. It is not renamed, as the
//! server's own weenies name it, but may be copied under a new name. A pack's own quest is
//! renamed through every file of the pack that names it, shown first, and deleted once nothing
//! names it. Each change is one step of the history. The quests are gathered again only after an
//! edit, or once more of the server's database is learned.

use std::path::Path;
use std::rc::Rc;

use lyceum_foundry::content_check::quest_key;
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::quests::{QuestUser, Renamed, Whose, WorldQuest};
use lyceum_foundry::sql_ids::QuestDoes;

use super::{EditorApp, Selection, UiAction};

/// A change the Quests tab asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum QuestAction {
    /// Quest `name` registered in the pack being edited.
    Register(String),
    /// Column `column` of the pack's quest `name` set to `value`, as typed.
    Set {
        name: String,
        column: String,
        value: String,
    },
    /// The server's quest registered again in the pack, to be changed there; and taken away
    /// again.
    ChangeHere(String),
    PutBack(String),
    /// Quest `from` copied under the name `to`.
    Copy {
        from: String,
        to: String,
    },
    /// The pack's quest `from` to be renamed `to`: what it changes is shown first; then the
    /// rename shown made, or let go.
    AskRename {
        from: String,
        to: String,
    },
    Rename,
    CancelRename,
    /// The pack's own quest taken away, once nothing names it.
    Delete(String),
    /// A quest opened in the tab, or the list again.
    Open(Option<String>),
    /// A weenie that names a quest, shown in the Definition pane and opened in the editor (a
    /// server's weenie by Edit's question first).
    OpenUse(Weenie),
}

/// A rename shown before it is made: the quest, its new name, what it changes, and after which
/// edit it was worked out (another edit lets it go).
#[derive(Debug, Clone, PartialEq)]
pub struct RenameAsked {
    pub from: String,
    pub to: String,
    pub renamed: Renamed,
    pub edits: u64,
}

/// The quests as last gathered: after which edit, with how much of the server's database
/// learned.
#[derive(Debug, Clone)]
struct Gathered {
    edits: u64,
    learned: (usize, usize),
    quests: Rc<Vec<WorldQuest>>,
}

/// The tab's state.
#[derive(Debug, Clone, Default)]
pub struct QuestsTab {
    /// What the search is typed to find, and whether only the pack being edited's quests (and
    /// those nothing registers) are listed.
    pub filter: String,
    pub packs_only: bool,
    /// The quest opened, by name; the list when none.
    pub open: Option<String>,
    /// The name typed for a copy or a rename.
    pub new_name: String,
    pub renaming: Option<RenameAsked>,
    /// Why the last change asked was refused, until another is asked.
    pub note: Option<String>,
    gathered: Option<Gathered>,
    /// The opened quest's uses, a row for each weenie by name, as made last: for which quest (by
    /// its key) of which gathering.
    uses: Option<UsesMade>,
    /// As drawn last: the search, each row by its quest's name, each button by what it does,
    /// the new name's field, each of the quest's fields by its column, and each use by its
    /// weenie.
    pub search_field: Option<egui::Rect>,
    pub rows: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(&'static str, egui::Rect)>,
    pub name_field: Option<egui::Rect>,
    pub fields: Vec<(String, egui::Rect)>,
    pub use_rows: Vec<(Weenie, egui::Rect)>,
    /// Each quest's link in the weenie form's Quests fold, as drawn last.
    pub fold_links: Vec<(String, egui::Rect)>,
}

impl QuestsTab {
    /// Where button `what` was drawn last.
    #[must_use]
    pub fn button(&self, what: &str) -> Option<egui::Rect> {
        self.buttons
            .iter()
            .find(|(b, _)| *b == what)
            .map(|(_, r)| *r)
    }

    /// Where the row of quest `name` was drawn last.
    #[must_use]
    pub fn row(&self, name: &str) -> Option<egui::Rect> {
        self.rows.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
    }

    /// Where field `column` of the quest opened was drawn last.
    #[must_use]
    pub fn field(&self, column: &str) -> Option<egui::Rect> {
        self.fields
            .iter()
            .find(|(c, _)| c == column)
            .map(|(_, r)| *r)
    }

    fn clear_drawn(&mut self) {
        self.search_field = None;
        self.rows.clear();
        self.buttons.clear();
        self.name_field = None;
        self.fields.clear();
        self.use_rows.clear();
    }
}

/// The quests answering `filter` (lower case): those whose name holds it; with `packs_only`,
/// only those the pack being edited registers (its own, or its change of the server's) and those
/// nothing registers.
#[must_use]
pub fn shown<'a>(all: &'a [WorldQuest], filter: &str, packs_only: bool) -> Vec<&'a WorldQuest> {
    let mine = |q: &WorldQuest| {
        matches!(
            q.whose,
            Whose::Pack { mine: true } | Whose::Changed { mine: true } | Whose::Nobody
        )
    };
    all.iter()
        .filter(|q| !packs_only || mine(q))
        .filter(|q| filter.is_empty() || q.name.to_lowercase().contains(filter))
        .collect()
}

/// Each weenie that names a quest, once, in the order first met: whether it is the server's own
/// (as its database has it), and what it does with the quest, each once, in order.
#[must_use]
pub fn by_weenie(uses: &[QuestUser]) -> Vec<(Weenie, bool, Vec<QuestDoes>)> {
    let mut out: Vec<(Weenie, bool, Vec<QuestDoes>)> = Vec::new();
    let mut at: std::collections::HashMap<&Weenie, usize> = std::collections::HashMap::new();
    for u in uses {
        let does = u.how.does();
        match at.get(&u.by) {
            Some(i) => {
                let list = &mut out[*i].2;
                if !list.contains(&does) {
                    list.push(does);
                }
            }
            None => {
                at.insert(&u.by, out.len());
                out.push((u.by.clone(), u.file.is_none(), vec![does]));
            }
        }
    }
    out
}

/// The key of the words for what a use does with a quest.
#[must_use]
pub const fn does_key(d: QuestDoes) -> &'static str {
    match d {
        QuestDoes::Stamps => "quests.does_stamps",
        QuestDoes::Tests => "quests.does_tests",
        QuestDoes::Erases => "quests.does_erases",
        QuestDoes::Changes => "quests.does_changes",
        QuestDoes::Answers => "quests.does_answers",
        QuestDoes::Needs => "quests.does_needs",
        QuestDoes::Kills => "quests.does_kills",
        QuestDoes::StampsWhenTaken => "quests.does_stamps_when_taken",
    }
}

/// A row of a quest's uses: the weenie's name, the weenie, whether it is the server's own, and
/// what it does with the quest.
type UseLine = (String, Weenie, bool, Vec<QuestDoes>);

/// A quest's uses as made: for which quest (by its key), of which gathering.
type UsesMade = (String, Rc<Vec<WorldQuest>>, Rc<Vec<UseLine>>);

impl EditorApp {
    /// Every quest of the world, as [`lyceum_foundry::edit::Session::world_quests`] gathers
    /// them: gathered again only after an edit, or once more of the server's database is
    /// learned.
    pub fn world_quests(&mut self) -> Rc<Vec<WorldQuest>> {
        let learned = (
            self.session.server_quests().len(),
            self.session.server_quest_uses().len() + self.session.server_weenies().len(),
        );
        if let Some(g) = &self.quests.gathered {
            if g.edits == self.stats.edits && g.learned == learned {
                return Rc::clone(&g.quests);
            }
        }
        let quests = Rc::new(self.session.world_quests());
        self.quests.gathered = Some(Gathered {
            edits: self.stats.edits,
            learned,
            quests: Rc::clone(&quests),
        });
        quests
    }

    /// Quest `name` of the world, as the tab has it.
    fn world_quest(&mut self, name: &str) -> Option<WorldQuest> {
        let key = quest_key(name);
        self.world_quests()
            .iter()
            .find(|q| quest_key(&q.name) == key)
            .cloned()
    }

    /// Why `name` cannot be a new quest's name, when it cannot: none, or one with an `@` in it,
    /// or one a quest of the world has.
    fn refuse_quest_name(&mut self, name: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty() || name.contains('@') {
            return Some(self.say("quests.bad_name"));
        }
        self.world_quest(name)
            .map(|q| self.say_with("quests.name_taken", &[("name", &q.name)]))
    }

    /// Answer a change the tab asks for; a refusal is said in the tab too.
    ///
    /// # Errors
    /// Why it was refused.
    pub fn quest_action(&mut self, a: QuestAction) -> Result<(), String> {
        let keeps_note = matches!(a, QuestAction::Set { .. } | QuestAction::OpenUse(_));
        if !keeps_note {
            self.quests.note = None;
        }
        let result = self.quest_action_now(a);
        if let Err(e) = &result {
            self.quests.note = Some(e.clone());
        }
        result
    }

    fn quest_action_now(&mut self, a: QuestAction) -> Result<(), String> {
        let refused = |e: lyceum_foundry::error::Error| e.to_string();
        match a {
            QuestAction::Register(name) => self.register_quest(&name, false),
            QuestAction::Set {
                name,
                column,
                value,
            } => self.quest_edit(&name, &column, &value),
            QuestAction::ChangeHere(name) => {
                let op = self.session.quest_change_here(&name).map_err(refused)?;
                self.apply(&op, false).map(|_| ())
            }
            QuestAction::PutBack(name) => {
                let op = self
                    .session
                    .quest_unregister(&name, true)
                    .map_err(refused)?;
                self.apply(&op, false).map(|_| ())
            }
            QuestAction::Copy { from, to } => {
                if let Some(why) = self.refuse_quest_name(&to) {
                    return Err(why);
                }
                let op = self.session.quest_copy(&from, &to).map_err(refused)?;
                self.apply(&op, false)?;
                self.quests.open = Some(to.trim().to_owned());
                self.quests.new_name.clear();
                Ok(())
            }
            QuestAction::AskRename { from, to } => {
                // Its own name again, in other letters, is no other quest's.
                if quest_key(&from) != quest_key(&to) {
                    if let Some(why) = self.refuse_quest_name(&to) {
                        return Err(why);
                    }
                }
                let renamed = self.session.quest_rename(&from, &to).map_err(refused)?;
                self.quests.renaming = Some(RenameAsked {
                    from,
                    to: to.trim().to_owned(),
                    renamed,
                    edits: self.stats.edits,
                });
                Ok(())
            }
            QuestAction::Rename => {
                let Some(r) = self.quests.renaming.take() else {
                    return Ok(());
                };
                // Worked out again from the files as they are now, which are those it showed.
                let renamed = self.session.quest_rename(&r.from, &r.to).map_err(refused)?;
                self.apply(&renamed.op(), false)?;
                self.quests.open = Some(r.to);
                self.quests.new_name.clear();
                Ok(())
            }
            QuestAction::CancelRename => {
                self.quests.renaming = None;
                Ok(())
            }
            QuestAction::Delete(name) => {
                if let Some(q) = self.world_quest(&name).filter(|q| !q.uses.is_empty()) {
                    return Err(self.say_with(
                        "quests.still_used",
                        &[("name", &q.name), ("n", &q.places())],
                    ));
                }
                let op = self
                    .session
                    .quest_unregister(&name, false)
                    .map_err(refused)?;
                self.apply(&op, false)?;
                self.quests.open = None;
                Ok(())
            }
            QuestAction::Open(name) => {
                if name.is_some() {
                    self.show_tab(super::Tab::Quests);
                }
                self.quests.open = name;
                self.quests.renaming = None;
                self.quests.new_name.clear();
                Ok(())
            }
            QuestAction::OpenUse(w) => {
                self.panels.weenie = Some(w.clone());
                if matches!(self.selection, Some(Selection::Spawn { .. })) {
                    self.selection = None;
                    self.also.clear();
                }
                self.show_weenie();
                self.edit_weenie(&w, None);
                Ok(())
            }
        }
    }

    /// Whose quest `q` is, in words.
    fn whose_words(&self, q: &WorldQuest) -> String {
        let t = self.text.table();
        let pack = q.pack.clone().unwrap_or_default();
        match q.whose {
            Whose::Pack { .. } => t.fill("quests.whose_pack", &[("pack", &pack)]),
            Whose::Server => t.get("quests.whose_server").to_owned(),
            Whose::Changed { .. } => t.fill("quests.whose_changed", &[("pack", &pack)]),
            Whose::Nobody => t.get("weenie.quest_not_registered").to_owned(),
        }
    }

    /// How often quest `q` can be done, in words: every so many seconds, so many times.
    fn timing_words(&self, q: &WorldQuest) -> Option<String> {
        let t = self.text.table();
        let row = q.quest.as_ref()?;
        let times = if row.max_solves < 0 {
            t.get("weenie.quest_any").to_owned()
        } else {
            row.max_solves.to_string()
        };
        Some(t.fill(
            "quests.timing",
            &[("every", &row.min_delta), ("times", &times)],
        ))
    }

    /// The Quests tab: the list, or the quest opened.
    pub(super) fn quests_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.quests.clear_drawn();
        let all = self.world_quests();
        let open = self.quests.open.as_deref().map(quest_key);
        match open.and_then(|key| all.iter().find(|q| quest_key(&q.name) == key)) {
            Some(q) => self.quest_card(ui, q, out),
            None => self.quests_list(ui, &all, out),
        }
    }

    /// Every quest answering the search, a row each.
    fn quests_list(&mut self, ui: &mut egui::Ui, all: &[WorldQuest], out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let r = super::panels::search_box(ui, &mut self.quests.filter, t.get("quests.filter_hint"));
        self.quests.search_field = Some(r.rect);
        let f = self.quests.filter.trim().to_lowercase();
        let list = shown(all, &f, self.quests.packs_only);
        ui.horizontal_wrapped(|ui| {
            let r = ui.checkbox(&mut self.quests.packs_only, t.get("quests.this_pack_only"));
            self.quests.buttons.push(("this_pack_only", r.rect));
            ui.weak(t.fill("quests.shown", &[("n", &list.len()), ("all", &all.len())]));
            if self.session.server_quests().is_empty() {
                ui.weak(t.get("quests.server_not_read"));
            }
        });
        ui.separator();
        let area = egui::ScrollArea::vertical()
            .id_salt("quests")
            .auto_shrink(false);
        let rows = egui::Id::new("quests rows");
        super::seen::rows(ui, rows, area, list.len(), |ui, i| {
            let q = list[i];
            ui.horizontal(|ui| {
                let r = ui.selectable_label(false, &q.name);
                self.quests.rows.push((q.name.clone(), r.rect));
                if r.clicked() {
                    out.push(UiAction::Quest(QuestAction::Open(Some(q.name.clone()))));
                }
                if q.whose == Whose::Nobody {
                    ui.colored_label(
                        super::panels::mark::WARN,
                        t.get("weenie.quest_not_registered"),
                    );
                } else {
                    ui.weak(self.whose_words(q));
                }
                if let Some(w) = self.timing_words(q) {
                    ui.weak(w);
                }
                ui.weak(t.fill("quests.used_in", &[("n", &q.places())]));
                if q.whose == Whose::Nobody
                    && ui
                        .small_button(t.get("quests.register"))
                        .on_hover_text(t.get("weenie.register_quest"))
                        .clicked()
                {
                    out.push(UiAction::Quest(QuestAction::Register(q.name.clone())));
                }
            });
        });
    }

    /// The quest opened: its row, its changes, and what uses it.
    fn quest_card(&mut self, ui: &mut egui::Ui, q: &WorldQuest, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let back = ui.link(t.get("quests.back"));
        self.quests.buttons.push(("back", back.rect));
        if back.clicked() {
            out.push(UiAction::Quest(QuestAction::Open(None)));
        }
        ui.horizontal_wrapped(|ui| {
            ui.strong(&q.name);
            ui.weak(self.whose_words(q));
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("quest")
            .auto_shrink(false)
            .show(ui, |ui| {
                self.quest_row(ui, q, out);
                self.quest_changes(ui, q, out);
                if let Some(note) = self.quests.note.clone() {
                    ui.colored_label(super::panels::mark::FAULT, note);
                }
                ui.separator();
                self.quest_uses(ui, q, out);
            });
    }

    /// The quest's row, as the weenie form's Quests fold shows it: the pack's own editable.
    fn quest_row(&mut self, ui: &mut egui::Ui, q: &WorldQuest, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.horizontal_wrapped(|ui| match (q.whose, &q.quest) {
            (Whose::Pack { mine: true } | Whose::Changed { mine: true }, Some(row)) => {
                let pack = q.pack.clone().unwrap_or_default();
                ui.weak(t.fill("weenie.quest_packs", &[("pack", &pack)]));
                // Every so many seconds, so many times (or any), each typed.
                let [before, between, after] = timing_pieces(t.get("quests.timing"));
                let any = t.get("weenie.quest_any");
                ui.weak(before);
                let every = row.min_delta.to_string();
                self.quest_field(ui, &q.name, "min_Delta", &every, 60.0, false, out);
                ui.weak(between);
                let times = solves_words(i64::from(row.max_solves), any);
                self.quest_field(ui, &q.name, "max_Solves", &times, 50.0, false, out);
                ui.weak(after);
            }
            (Whose::Server, Some(row)) => {
                let times = if row.max_solves < 0 {
                    t.get("weenie.quest_any").to_owned()
                } else {
                    row.max_solves.to_string()
                };
                ui.weak(t.fill(
                    "weenie.quest_servers",
                    &[("every", &row.min_delta), ("times", &times)],
                ));
            }
            (Whose::Nobody, _) | (_, None) => {
                ui.colored_label(
                    super::panels::mark::WARN,
                    t.get("weenie.quest_not_registered"),
                );
                let r = ui.small_button(t.get("weenie.register_quest"));
                self.quests.buttons.push(("register", r.rect));
                if r.clicked() {
                    out.push(UiAction::Quest(QuestAction::Register(q.name.clone())));
                }
            }
            (Whose::Pack { mine: false } | Whose::Changed { mine: false }, Some(_)) => {
                ui.weak(t.get("quests.another_pack"));
                if let Some(w) = self.timing_words(q) {
                    ui.weak(w);
                }
            }
        });
        // Its description, several lines.
        if let (Some(row), true) = (&q.quest, row_editable(q.whose)) {
            ui.weak("message");
            let now = row.message.clone();
            self.quest_field(
                ui,
                &q.name,
                "message",
                &now,
                super::fields::LINES_WIDTH,
                true,
                out,
            );
        }
        if let (Some(row), false) = (&q.quest, row_editable(q.whose)) {
            if !row.message.is_empty() {
                ui.weak(&row.message);
            }
        }
    }

    /// What may be done with the quest: changed in this world and put back, copied, renamed,
    /// deleted; and a rename asked, shown before it is made.
    fn quest_changes(&mut self, ui: &mut egui::Ui, q: &WorldQuest, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let push = |out: &mut Vec<UiAction>, a: QuestAction| out.push(UiAction::Quest(a));
        let button = |s: &mut Self, ui: &mut egui::Ui, what: &'static str| {
            let r = ui.button(t.get(&format!("quests.{what}")));
            s.quests.buttons.push((what, r.rect));
            r.clicked()
        };
        let server = matches!(q.whose, Whose::Server | Whose::Changed { .. });
        let change = match q.whose {
            Whose::Server => Some(("change_here", QuestAction::ChangeHere(q.name.clone()))),
            Whose::Changed { mine: true } => {
                Some(("put_back", QuestAction::PutBack(q.name.clone())))
            }
            Whose::Pack { mine: true } => Some(("delete", QuestAction::Delete(q.name.clone()))),
            _ => None,
        };
        if let Some((what, action)) = change {
            if button(self, ui, what) {
                push(out, action);
            }
        }
        let named = matches!(q.whose, Whose::Pack { mine: true }) || server;
        if named {
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.quests.new_name)
                        .hint_text(t.get("quests.new_name"))
                        .desired_width(160.0),
                );
                self.quests.name_field = Some(r.rect);
                let to = self.quests.new_name.clone();
                if server {
                    if button(self, ui, "copy") {
                        push(
                            out,
                            QuestAction::Copy {
                                from: q.name.clone(),
                                to,
                            },
                        );
                    }
                } else if button(self, ui, "rename") {
                    push(
                        out,
                        QuestAction::AskRename {
                            from: q.name.clone(),
                            to,
                        },
                    );
                }
            });
        }
        if server {
            ui.weak(t.get("quests.server_no_rename"));
        }
        let asked =
            self.quests.renaming.clone().filter(|r| {
                quest_key(&r.from) == quest_key(&q.name) && r.edits == self.stats.edits
            });
        if let Some(r) = asked {
            self.rename_shown(ui, &r, out);
        }
    }

    /// A rename asked: the files it changes and what it leaves, with Rename it and Cancel.
    fn rename_shown(&mut self, ui: &mut egui::Ui, r: &RenameAsked, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let pack = self.session.packs()[self.session.target()].dir.clone();
        ui.group(|ui| {
            ui.label(t.fill("quests.rename_files", &[("from", &r.from), ("to", &r.to)]));
            for (file, text) in &r.renamed.files {
                let line = shown_path(file, &pack);
                if text.is_some() {
                    ui.monospace(line);
                } else {
                    ui.monospace(t.fill("quests.file_goes", &[("file", &line)]));
                }
            }
            if !r.renamed.left.is_empty() {
                let left = by_weenie(&r.renamed.left);
                ui.label(t.fill("quests.rename_left", &[("n", &left.len())]));
                for (w, _, _) in left.iter().take(12) {
                    ui.weak(self.use_name(w));
                }
                if left.len() > 12 {
                    ui.weak(t.fill("usages.more", &[("n", &(left.len() - 12))]));
                }
            }
            ui.horizontal(|ui| {
                let go = ui.button(t.get("quests.rename_do"));
                self.quests.buttons.push(("rename_do", go.rect));
                if go.clicked() {
                    out.push(UiAction::Quest(QuestAction::Rename));
                }
                let no = ui.button(t.get("quests.cancel"));
                self.quests.buttons.push(("cancel", no.rect));
                if no.clicked() {
                    out.push(UiAction::Quest(QuestAction::CancelRename));
                }
            });
        });
    }

    /// A weenie's name as the uses list it.
    fn use_name(&self, w: &Weenie) -> String {
        self.weenie_entry(w).map_or_else(
            || super::form::weenie_label(w, &self.text.table()),
            |e| e.name.clone(),
        )
    }

    /// Every weenie that names the quest, by name, with what it does with it: a click opens it.
    fn quest_uses(&mut self, ui: &mut egui::Ui, q: &WorldQuest, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let list = self.quest_use_lines(q);
        ui.strong(t.fill("quests.uses", &[("n", &list.len())]));
        if list.is_empty() {
            ui.weak(t.get("quests.uses_none"));
        }
        if self.session.server_weenies().is_empty() {
            ui.weak(t.get("usages.packs_only"));
        }
        let id = egui::Id::new(("quest uses", quest_key(&q.name)));
        super::seen::blocks(
            ui,
            id,
            list.len(),
            20.0,
            |_| false,
            |ui, i| {
                let (name, w, server, does) = &list[i];
                ui.horizontal_wrapped(|ui| {
                    let r = ui.link(name);
                    self.quests.use_rows.push((w.clone(), r.rect));
                    if r.clicked() {
                        out.push(UiAction::Quest(QuestAction::OpenUse(w.clone())));
                    }
                    let mut words: Vec<&str> = does.iter().map(|d| t.get(does_key(*d))).collect();
                    if *server {
                        words.push(t.get("quests.whose_server"));
                    }
                    ui.weak(words.join(" • "));
                });
            },
        );
    }

    /// The uses of quest `q`, a row for each weenie, by name: made again only once the quests are
    /// gathered again.
    fn quest_use_lines(&mut self, q: &WorldQuest) -> Rc<Vec<UseLine>> {
        let key = quest_key(&q.name);
        let gathered = self.quests.gathered.as_ref().map(|g| Rc::clone(&g.quests));
        if let (Some((k, of, lines)), Some(now)) = (&self.quests.uses, &gathered) {
            if *k == key && Rc::ptr_eq(of, now) {
                return Rc::clone(lines);
            }
        }
        let mut list: Vec<UseLine> = by_weenie(&q.uses)
            .into_iter()
            .map(|(w, server, does)| (self.use_name(&w), w, server, does))
            .collect();
        list.sort_by_cached_key(|a| a.0.to_lowercase());
        let list = Rc::new(list);
        self.quests.uses = gathered.map(|g| (key, g, Rc::clone(&list)));
        list
    }

    /// A field of the pack's quest `name`: the text as typed while it is being typed, else
    /// `now`; a change is asked of column `column` when the field is left.
    #[allow(clippy::too_many_arguments)]
    fn quest_field(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        column: &str,
        now: &str,
        width: f32,
        lines: bool,
        out: &mut Vec<UiAction>,
    ) {
        let typing = format!("quests|{name}|{column}");
        let mut text = self
            .panels
            .form_typing
            .get(&typing)
            .cloned()
            .unwrap_or_else(|| now.to_owned());
        let r = if lines {
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_width(width)
                    .desired_rows(super::form::LINES),
            )
        } else {
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(width))
        };
        self.quests.fields.push((column.to_owned(), r.rect));
        if r.has_focus() || r.changed() {
            self.panels.form_typing.insert(typing.clone(), text.clone());
        }
        if r.lost_focus() {
            self.panels.form_typing.remove(&typing);
            if text.trim() != now.trim() {
                let any = self.text.table().get("weenie.quest_any").to_owned();
                let value = if column == "max_Solves" {
                    solves_typed(&text, &any)
                } else if lines {
                    text.trim_end().to_owned()
                } else {
                    text
                };
                out.push(UiAction::Quest(QuestAction::Set {
                    name: name.to_owned(),
                    column: column.to_owned(),
                    value,
                }));
            }
        }
    }
}

/// The words before, between and after the two numbers of `timing` (`every {every} s, {times}
/// time(s)`), where a quest's own are typed; a text without both in that order puts the boxes
/// side by side.
#[must_use]
pub fn timing_pieces(timing: &str) -> [&str; 3] {
    let Some((before, rest)) = timing.split_once("{every}") else {
        return ["", " ", ""];
    };
    match rest.split_once("{times}") {
        Some((between, after)) => [before, between, after],
        None => ["", " ", ""],
    }
}

/// How many times a quest can be done as its box shows it: `any` for no limit (-1).
#[must_use]
pub fn solves_words(max_solves: i64, any: &str) -> String {
    if max_solves < 0 {
        any.to_owned()
    } else {
        max_solves.to_string()
    }
}

/// What a typed number of times writes: `any` (in any case) is no limit, -1; anything else as
/// typed.
#[must_use]
pub fn solves_typed(typed: &str, any: &str) -> String {
    let typed = typed.trim();
    if typed.eq_ignore_ascii_case(any.trim()) {
        "-1".to_owned()
    } else {
        typed.to_owned()
    }
}

/// Whether a quest's row of `whose` is edited in the tab: the pack being edited's.
const fn row_editable(whose: Whose) -> bool {
    matches!(
        whose,
        Whose::Pack { mine: true } | Whose::Changed { mine: true }
    )
}

/// A path as the tab shows it: from the pack's folder, with forward slashes.
#[must_use]
pub fn shown_path(file: &Path, pack: &Path) -> String {
    file.strip_prefix(pack)
        .unwrap_or(file)
        .display()
        .to_string()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's Quests tab)
    use super::*;
    use lyceum_foundry::sql_ids::{Quest, QuestUse};

    #[test]
    fn a_quests_timing_reads_every_box_seconds_box_times_and_any_is_no_limit() {
        let t = crate::text::Text::built_in().table();
        assert_eq!(
            timing_pieces(t.get("quests.timing")),
            ["every ", " s, ", " time(s)"]
        );
        // A text without both numbers in order: the boxes side by side.
        assert_eq!(timing_pieces("{times} then {every}"), ["", " ", ""]);
        assert_eq!(timing_pieces("no numbers"), ["", " ", ""]);
        let any = t.get("weenie.quest_any");
        assert_eq!(solves_words(-1, any), any);
        assert_eq!(solves_words(3, any), "3");
        // Typed: any, in any case, is no limit; a number as typed.
        assert_eq!(solves_typed(&any.to_uppercase(), any), "-1");
        assert_eq!(solves_typed(" any ", "any"), "-1");
        assert_eq!(solves_typed(" 12 ", any), "12");
        assert_eq!(solves_typed("-1", any), "-1");
    }

    fn quest(name: &str, whose: Whose, uses: Vec<QuestUser>) -> WorldQuest {
        WorldQuest {
            name: name.into(),
            quest: Some(Quest::default()),
            server: None,
            file: None,
            pack: None,
            whose,
            uses,
        }
    }

    #[test]
    fn the_search_finds_a_quest_by_any_part_of_its_name_in_any_case() {
        let all = [
            quest("CowTipCounter", Whose::Server, Vec::new()),
            quest("HoltburgGreetingGiven", Whose::Server, Vec::new()),
        ];
        let names = |f: &str| -> Vec<String> {
            shown(&all, f, false)
                .into_iter()
                .map(|q| q.name.clone())
                .collect()
        };
        assert_eq!(names("tip"), ["CowTipCounter"]);
        assert_eq!(names("greeting"), ["HoltburgGreetingGiven"]);
        assert_eq!(names("").len(), 2);
        assert!(names("well").is_empty());
    }

    #[test]
    fn this_packs_only_lists_its_own_quests_its_changes_and_those_nothing_registers() {
        let all = [
            quest("Bell", Whose::Server, Vec::new()),
            quest("Gate", Whose::Pack { mine: true }, Vec::new()),
            quest("Kept", Whose::Changed { mine: true }, Vec::new()),
            quest("Lent", Whose::Pack { mine: false }, Vec::new()),
            quest("Other", Whose::Changed { mine: false }, Vec::new()),
            quest("Stray", Whose::Nobody, Vec::new()),
        ];
        let names = |f: &str| -> Vec<&str> {
            shown(&all, f, true)
                .into_iter()
                .map(|q| q.name.as_str())
                .collect()
        };
        assert_eq!(names(""), ["Gate", "Kept", "Stray"]);
        assert_eq!(names("st"), ["Stray"], "searched as well");
        assert_eq!(shown(&all, "", false).len(), 6, "off: every one");
    }

    #[test]
    fn each_weenie_is_listed_once_with_what_it_does_each_once_in_order() {
        let user = |by: u32, how: QuestUse, server: bool| QuestUser {
            by: Weenie::Class(by),
            how,
            file: (!server).then(|| "a.sql".into()),
        };
        let stamp = QuestUse::Action {
            category: 7,
            action: 22,
        };
        let ask = QuestUse::Action {
            category: 7,
            action: 21,
        };
        let uses = [
            user(1, ask, true),
            user(1, QuestUse::Answers { category: 12 }, true),
            user(1, QuestUse::Answers { category: 13 }, true),
            user(1, stamp, true),
            user(2, QuestUse::Property(45), false),
        ];
        assert_eq!(
            by_weenie(&uses),
            [
                (
                    Weenie::Class(1),
                    true,
                    vec![QuestDoes::Tests, QuestDoes::Answers, QuestDoes::Stamps]
                ),
                (Weenie::Class(2), false, vec![QuestDoes::Kills]),
            ]
        );
    }

    #[test]
    fn every_use_and_change_of_the_tab_has_words_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        for d in [
            QuestDoes::Stamps,
            QuestDoes::Tests,
            QuestDoes::Erases,
            QuestDoes::Changes,
            QuestDoes::Answers,
            QuestDoes::Needs,
            QuestDoes::Kills,
            QuestDoes::StampsWhenTaken,
        ] {
            assert!(t.contains_key(does_key(d)), "{}", does_key(d));
        }
        for what in ["change_here", "put_back", "copy", "rename", "delete"] {
            let k = format!("quests.{what}");
            assert!(t.contains_key(&k), "{k}");
        }
        assert_eq!(
            shown_path(Path::new("p/server/sql/quests/A.sql"), Path::new("p")),
            "server/sql/quests/A.sql"
        );
    }
}
