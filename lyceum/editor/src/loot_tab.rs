//! The Loot tab: every loot table of the world, death treasures and wielded treasures, the packs'
//! and the server's, and those something names that nothing holds; searched by id or by a name
//! they hold or are used by. Each row says what the table is, whose, what it gives in a few words
//! and how many weenies use it; a click opens it: its fields (the pack's own editable), its
//! changes, a roll of it on the editor's world, and every weenie that names it, a click on which
//! shows that weenie in the Definition pane and opens it in the editor.
//!
//! - A **death treasure** shows its tier, quality and heritage, and for items, magic items and
//!   mundane items each a chance, a least, a most and the mix they are drawn from, each mix by what
//!   it makes.
//! - A **wielded treasure** shows its sets as the server draws them ([`lyceum_foundry::wielded`]):
//!   each entry's chance of being made in its set in percent, what each set makes nothing at, and
//!   under an entry the sets drawn when it is made. The pack's own is edited there: a chance, how
//!   many, by how many fewer at most, the color and shade; entries moved, taken away, and added
//!   by a picker of every weenie with its picture, to a set, as a set of its own, or under an
//!   entry. Each edit writes the server's own numbers for what it shows.
//!
//! A server's table is changed in this world by a copy of it in the pack, under its own id, which
//! the pack then edits, and put back by taking the copy away. A new table is made plain or as a
//! copy of another, and a pack's own is deleted once nothing names it. Each change is one step of
//! the history. A roll asks the editor's world (Live) to draw the table, making nothing.

use std::rc::Rc;

use dereth_primitives::DataId;
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::loot::{Group, LootKind, LootTable, LootUser, Whose, HERITAGE_MIXES, TIERS};
use lyceum_foundry::sql_ids::{LootUse, Treasure, WieldedItem};
use lyceum_foundry::wielded::{self, Refused, Wielded};

use super::{EditorApp, Selection, UiAction};

/// A change the Loot tab asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum LootAction {
    /// A table opened in the tab (bringing the tab forward), or the list again.
    Open(Option<(LootKind, u32)>),
    /// The server's table copied into the pack, to be changed there; and the copy taken away.
    ChangeHere(LootKind, u32),
    PutBack(LootKind, u32),
    /// The pack's own table taken away, once nothing names it.
    Delete(LootKind, u32),
    /// A new table of the pack's: a copy of `from`, else a plain start (a wielded one's with
    /// `first`, as a list's row writes it); named `name`, else the first free name.
    New {
        kind: LootKind,
        from: Option<u32>,
        first: Option<String>,
        name: Option<String>,
    },
    /// Column `column` of the pack's death treasure `id` set to `value`, as typed.
    SetDeath {
        id: u32,
        column: String,
        value: String,
    },
    /// The pack's wielded treasure `id` written as `rows`.
    SetWielded {
        id: u32,
        rows: Vec<WieldedItem>,
    },
    /// A weenie that names a table, shown in the Definition pane and opened in the editor.
    OpenUse(Weenie),
    /// A table drawn on the editor's world, `times` times.
    Roll {
        id: u32,
        times: u32,
    },
    /// Why a change was not made, said in the tab.
    Refused(String),
}

/// A roll asked of the editor's world: of which table, and where its log stood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rolling {
    pub id: u32,
    pub mark: usize,
}

/// A roll's answer: of which table, and each draw (or why there was none).
#[derive(Debug, Clone, PartialEq)]
pub struct Rolled {
    pub id: u32,
    pub draws: Result<Vec<crate::live::Draw>, String>,
}

/// The tables as last gathered: after which edit, with how much of the server's database
/// learned, and the words each is found by.
#[derive(Debug, Clone)]
struct Gathered {
    edits: u64,
    learned: (usize, usize, usize),
    tables: Rc<Vec<LootTable>>,
    words: Rc<Vec<String>>,
}

/// The tab's state.
#[derive(Debug, Clone)]
pub struct LootTab {
    /// What the search is typed to find; which kinds are listed; whether only the pack being
    /// edited's tables (and those nothing holds) are.
    pub filter: String,
    pub death: bool,
    pub wielded: bool,
    pub packs_only: bool,
    /// The table opened; the list when none.
    pub open: Option<(LootKind, u32)>,
    /// The name a new table is to have, as typed; the first free one when empty.
    pub new_name: String,
    /// Why the last change asked was refused, until another is asked.
    pub note: Option<String>,
    /// How many times a roll draws, as typed; the roll asked, and its answer.
    pub times: String,
    pub rolling: Option<Rolling>,
    pub rolled: Option<Rolled>,
    gathered: Option<Gathered>,
    /// The opened table's uses, a row for each weenie by name, as made last: for which table of
    /// which gathering.
    uses: Option<UsesMade>,
    /// As drawn last: the search, each row by its table, each button by what it does, each field
    /// by its column (a wielded entry's by its place and column), and each use by its weenie.
    pub search_field: Option<egui::Rect>,
    pub rows: Vec<((LootKind, u32), egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub fields: Vec<(String, egui::Rect)>,
    pub use_rows: Vec<(Weenie, egui::Rect)>,
}

impl Default for LootTab {
    fn default() -> Self {
        Self {
            filter: String::new(),
            death: true,
            wielded: true,
            packs_only: false,
            open: None,
            new_name: String::new(),
            note: None,
            times: "20".into(),
            rolling: None,
            rolled: None,
            gathered: None,
            uses: None,
            search_field: None,
            rows: Vec::new(),
            buttons: Vec::new(),
            fields: Vec::new(),
            use_rows: Vec::new(),
        }
    }
}

impl LootTab {
    /// Where button `what` was drawn last.
    #[must_use]
    pub fn button(&self, what: &str) -> Option<egui::Rect> {
        self.buttons
            .iter()
            .find(|(b, _)| b == what)
            .map(|(_, r)| *r)
    }

    /// Where the row of table `id` of `kind` was drawn last.
    #[must_use]
    pub fn row(&self, kind: LootKind, id: u32) -> Option<egui::Rect> {
        self.rows
            .iter()
            .find(|(k, _)| *k == (kind, id))
            .map(|(_, r)| *r)
    }

    /// Where field `key` of the table opened was drawn last.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<egui::Rect> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, r)| *r)
    }

    fn clear_drawn(&mut self) {
        self.search_field = None;
        self.rows.clear();
        self.buttons.clear();
        self.fields.clear();
        self.use_rows.clear();
    }
}

/// The tables answering `filter` (lower case) by their words (`words`, one for each table),
/// of the kinds asked for; with `packs_only`, only those the pack being edited holds (its own,
/// or its copy of the server's) and those nothing holds.
#[must_use]
pub fn shown<'a>(
    all: &'a [LootTable],
    words: &[String],
    filter: &str,
    (death, wielded, packs_only): (bool, bool, bool),
) -> Vec<&'a LootTable> {
    let mine = |t: &LootTable| {
        matches!(
            t.whose,
            Whose::Pack { mine: true } | Whose::Changed { mine: true } | Whose::Nobody
        )
    };
    all.iter()
        .zip(words)
        .filter(|(t, _)| match t.kind {
            LootKind::Death => death,
            LootKind::Wielded => wielded,
        })
        .filter(|(t, _)| !packs_only || mine(t))
        .filter(|(_, w)| filter.is_empty() || w.contains(filter))
        .map(|(t, _)| t)
        .collect()
}

/// Each weenie that names a table, once, in the order first met: whether it is the server's own
/// (as its database has it), and how it names it, each way once, in order.
#[must_use]
pub fn by_weenie(uses: &[LootUser]) -> Vec<(Weenie, bool, Vec<LootUse>)> {
    let mut out: Vec<(Weenie, bool, Vec<LootUse>)> = Vec::new();
    let mut at: std::collections::HashMap<&Weenie, usize> = std::collections::HashMap::new();
    for u in uses {
        let how = match u.how {
            LootUse::Generator { .. } => LootUse::Generator { place: 0 },
            h => h,
        };
        match at.get(&u.by) {
            Some(i) => {
                let list = &mut out[*i].2;
                if !list.contains(&how) {
                    list.push(how);
                }
            }
            None => {
                at.insert(&u.by, out.len());
                out.push((u.by.clone(), u.file.is_none(), vec![how]));
            }
        }
    }
    out
}

/// The key of the words for how a weenie names a table.
#[must_use]
pub const fn use_key(how: LootUse) -> &'static str {
    match how {
        LootUse::Dies => "loot.use_dies",
        LootUse::Wields => "loot.use_wields",
        LootUse::Generator { .. } => "loot.use_generator",
    }
}

/// The key of a kind's name.
#[must_use]
pub const fn kind_key(kind: LootKind) -> &'static str {
    match kind {
        LootKind::Death => "loot.death",
        LootKind::Wielded => "loot.wielded",
    }
}

/// How many of a table's uses its words for the search name, and of what a roll made the list
/// shows, at most.
const USES_SHOWN: usize = 200;

/// A row of a table's uses: the weenie's name, the weenie, whether it is the server's own, and
/// how it names the table.
type UseLine = (String, Weenie, bool, Vec<LootUse>);

/// A table's uses as made: for which table, of which gathering.
type UsesMade = ((LootKind, u32), Rc<Vec<LootTable>>, Rc<Vec<UseLine>>);

/// How many draws a roll makes at most.
const MOST_TIMES: u32 = 1000;

/// A mix's share as its words show it: a whole percent, or a tenth.
fn share_words(p: f32) -> String {
    super::collections::percent_words(f64::from(p) * 100.0)
}

impl EditorApp {
    /// Every loot table of the world, as [`lyceum_foundry::edit::Session::world_loot`] gathers
    /// them, with the words each is found by: gathered again only after an edit, or once more of
    /// the server's database is learned.
    fn world_loot(&mut self) -> (Rc<Vec<LootTable>>, Rc<Vec<String>>) {
        let learned = (
            self.session.server_treasures().len() + self.session.server_wielded().len(),
            self.session.server_loot_uses().len(),
            self.session.server_weenies().len(),
        );
        if let Some(g) = &self.loot.gathered {
            if g.edits == self.stats.edits && g.learned == learned {
                return (Rc::clone(&g.tables), Rc::clone(&g.words));
            }
        }
        let tables = Rc::new(self.session.world_loot());
        let t = self.text.table();
        let words: Vec<String> = tables
            .iter()
            .map(|table| {
                let mut w = format!("{} {}", t.get(kind_key(table.kind)), table.id);
                if let Some(s) = &table.symbol {
                    w.push(' ');
                    w.push_str(s);
                }
                for u in table.uses.iter().take(USES_SHOWN) {
                    if let Some(e) = self.weenie_entry(&u.by) {
                        w.push(' ');
                        w.push_str(&e.name);
                    }
                }
                for r in table.wielded().unwrap_or_default() {
                    if let Some(n) = self.class_name(&r.weenie) {
                        w.push(' ');
                        w.push_str(&n);
                    }
                }
                w.to_lowercase()
            })
            .collect();
        let words = Rc::new(words);
        self.loot.gathered = Some(Gathered {
            edits: self.stats.edits,
            learned,
            tables: Rc::clone(&tables),
            words: Rc::clone(&words),
        });
        (tables, words)
    }

    /// Table `id` of `kind` of the world, as the tab has it.
    fn loot_table(&mut self, kind: LootKind, id: u32) -> Option<LootTable> {
        self.world_loot()
            .0
            .iter()
            .find(|t| t.kind == kind && t.id == id)
            .cloned()
    }

    /// Answer a change the tab asks for; a refusal is said in the tab too.
    ///
    /// # Errors
    /// Why it was refused.
    pub fn loot_action(&mut self, a: LootAction) -> Result<(), String> {
        let keeps_note = matches!(a, LootAction::OpenUse(_) | LootAction::Roll { .. });
        if !keeps_note {
            self.loot.note = None;
        }
        let result = self.loot_action_now(a);
        if let Err(e) = &result {
            self.loot.note = Some(e.clone());
        }
        result
    }

    fn loot_action_now(&mut self, a: LootAction) -> Result<(), String> {
        let refused = |e: lyceum_foundry::error::Error| e.to_string();
        match a {
            LootAction::Open(at) => {
                if at.is_some() {
                    self.show_tab(super::Tab::Loot);
                }
                if at != self.loot.open {
                    self.loot.rolled = None;
                }
                self.loot.open = at;
                Ok(())
            }
            LootAction::ChangeHere(kind, id) => {
                let op = self.session.loot_change_here(kind, id).map_err(refused)?;
                self.apply(&op, false).map(|_| ())
            }
            LootAction::PutBack(kind, id) => {
                let op = self
                    .session
                    .loot_take_away(kind, id, true)
                    .map_err(refused)?;
                self.apply(&op, false).map(|_| ())
            }
            LootAction::Delete(kind, id) => {
                if let Some(t) = self.loot_table(kind, id).filter(|t| !t.uses.is_empty()) {
                    return Err(self.say_with("loot.still_used", &[("n", &t.places())]));
                }
                let op = self
                    .session
                    .loot_take_away(kind, id, false)
                    .map_err(refused)?;
                self.apply(&op, false)?;
                self.loot.open = None;
                Ok(())
            }
            LootAction::New {
                kind,
                from,
                first,
                name,
            } => {
                let (op, symbol) = self
                    .session
                    .loot_new(kind, from, first.as_deref(), name.as_deref())
                    .map_err(refused)?;
                self.apply(&op, false)?;
                self.loot.new_name.clear();
                self.loot.open = self.session.treasure_id(&symbol).map(|id| (kind, id));
                self.show_tab(super::Tab::Loot);
                Ok(())
            }
            LootAction::SetDeath { id, column, value } => {
                let op = self
                    .session
                    .death_set(id, &column, &value)
                    .map_err(refused)?;
                self.apply(&op, false).map(|_| ())
            }
            LootAction::SetWielded { id, rows } => {
                let op = self.session.wielded_set(id, &rows).map_err(refused)?;
                self.apply(&op, false).map(|_| ())
            }
            LootAction::OpenUse(w) => {
                self.panels.weenie = Some(w.clone());
                if matches!(self.selection, Some(Selection::Spawn { .. })) {
                    self.selection = None;
                    self.also.clear();
                }
                self.show_weenie();
                self.edit_weenie(&w, None);
                Ok(())
            }
            LootAction::Roll { id, times } => self.roll_loot(id, times),
            LootAction::Refused(why) => Err(why),
        }
    }

    /// Ask the editor's world to draw table `id` `times` times, once what waits to reach it has.
    fn roll_loot(&mut self, id: u32, times: u32) -> Result<(), String> {
        if self.live.is_none() {
            return Err(self.say("loot.roll_needs_live"));
        }
        if self.live_dirty {
            self.push_live()?;
        }
        let needs = self.say("loot.roll_needs_live");
        let live = self.live.as_mut().ok_or(needs)?;
        let mark = live.roll(id, times.clamp(1, MOST_TIMES))?;
        self.loot.rolling = Some(Rolling { id, mark });
        self.loot.rolled = None;
        Ok(())
    }

    /// The answer to the roll asked, once the world has given all of it.
    fn roll_frame(&mut self) {
        let Some(r) = self.loot.rolling.clone() else {
            return;
        };
        let answer = match self.live.as_ref() {
            Some(live) => live.rolled(r.id, r.mark),
            None => Some(Err(self.say("loot.roll_needs_live"))),
        };
        if let Some(draws) = answer {
            self.loot.rolling = None;
            self.loot.rolled = Some(Rolled { id: r.id, draws });
        }
    }

    /// Wait for the roll asked to be answered, as a script or a test does.
    ///
    /// # Errors
    /// No roll asked, no world, or one that did not answer.
    pub fn wait_loot_roll(&mut self) -> Result<Vec<crate::live::Draw>, String> {
        let r = self.loot.rolling.clone().ok_or("no roll was asked")?;
        let needle = format!("Rolled treasure {}: ", r.id);
        let live = self
            .live
            .as_mut()
            .ok_or("the editor's world is not running")?;
        live.wait_for_any(
            &[
                &needle,
                "Unknown command: roll-treasure",
                "Draw a table 1 to ",
            ],
            r.mark,
        )
        .ok_or("the editor's world did not answer the roll")?;
        self.roll_frame();
        self.loot
            .rolled
            .clone()
            .ok_or_else(|| "no answer".to_owned())?
            .draws
    }

    /// Whose table `t` is, in words.
    fn loot_whose(&self, t: &LootTable) -> String {
        let tx = self.text.table();
        let pack = t.pack.clone().unwrap_or_default();
        match t.whose {
            Whose::Pack { .. } => tx.fill("quests.whose_pack", &[("pack", &pack)]),
            Whose::Server => tx.get("quests.whose_server").to_owned(),
            Whose::Changed { .. } => tx.fill("quests.whose_changed", &[("pack", &pack)]),
            Whose::Nobody => tx.get("loot.held_nowhere").to_owned(),
        }
    }

    /// What table `t` gives, in a few words.
    fn loot_summary(&self, t: &LootTable) -> String {
        let tx = self.text.table();
        if let Some(d) = t.death() {
            return tx.fill(
                "loot.death_summary",
                &[
                    ("tier", &d.tier),
                    ("items", &d.items.0),
                    ("magic", &d.magic.0),
                    ("mundane", &d.mundane.0),
                ],
            );
        }
        if let Some(rows) = t.wielded() {
            let w = wielded::read(rows);
            return tx.fill(
                "loot.wielded_summary",
                &[("n", &w.len()), ("sets", &w.sets.len())],
            );
        }
        String::new()
    }

    /// The Loot tab: the list, or the table opened.
    pub(super) fn loot_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.loot.clear_drawn();
        self.roll_frame();
        let (all, words) = self.world_loot();
        match self
            .loot
            .open
            .and_then(|(k, id)| all.iter().find(|t| t.kind == k && t.id == id))
        {
            Some(t) => self.loot_card(ui, t, out),
            None => self.loot_list(ui, &all, &words, out),
        }
    }

    /// Every table answering the search, a row each, and new tables.
    fn loot_list(
        &mut self,
        ui: &mut egui::Ui,
        all: &[LootTable],
        words: &[String],
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let r = super::panels::search_box(ui, &mut self.loot.filter, t.get("loot.filter_hint"));
        self.loot.search_field = Some(r.rect);
        let f = self.loot.filter.trim().to_lowercase();
        let list = shown(
            all,
            words,
            &f,
            (self.loot.death, self.loot.wielded, self.loot.packs_only),
        );
        ui.horizontal_wrapped(|ui| {
            for (key, on) in [
                ("death", &mut self.loot.death),
                ("wielded", &mut self.loot.wielded),
                ("this_pack_only", &mut self.loot.packs_only),
            ] {
                let r = ui.checkbox(on, t.get(&format!("loot.show_{key}")));
                self.loot.buttons.push((format!("show_{key}"), r.rect));
            }
            ui.weak(t.fill("quests.shown", &[("n", &list.len()), ("all", &all.len())]));
            if self.session.server_treasures().is_empty() {
                ui.weak(t.get("loot.server_not_read"));
            }
        });
        ui.horizontal_wrapped(|ui| {
            let name = self.new_loot_name(ui);
            let r = ui.button(t.get("loot.new_death"));
            self.loot.buttons.push(("new_death".into(), r.rect));
            if r.clicked() {
                out.push(UiAction::Loot(LootAction::New {
                    kind: LootKind::Death,
                    from: None,
                    first: None,
                    name: name.clone(),
                }));
            }
            if let Some(first) =
                self.weenie_picker(ui, "loot:new-wielded", t.get("loot.new_wielded"))
            {
                out.push(UiAction::Loot(LootAction::New {
                    kind: LootKind::Wielded,
                    from: None,
                    first: Some(first),
                    name,
                }));
            }
        });
        if let Some(note) = self.loot.note.clone() {
            ui.colored_label(super::panels::mark::FAULT, note);
        }
        ui.separator();
        let area = egui::ScrollArea::vertical()
            .id_salt("loot")
            .auto_shrink(false);
        let rows = egui::Id::new("loot rows");
        super::seen::rows(ui, rows, area, list.len(), |ui, i| {
            let table = list[i];
            ui.horizontal(|ui| {
                let name = t.fill(
                    "loot.table_name",
                    &[("kind", &t.get(kind_key(table.kind))), ("id", &table.id)],
                );
                let r = ui.selectable_label(false, name);
                self.loot.rows.push(((table.kind, table.id), r.rect));
                if r.clicked() {
                    out.push(UiAction::Loot(LootAction::Open(Some((
                        table.kind, table.id,
                    )))));
                }
                if let Some(s) = &table.symbol {
                    ui.monospace(s);
                }
                if table.whose == Whose::Nobody {
                    ui.colored_label(super::panels::mark::WARN, self.loot_whose(table));
                } else {
                    ui.weak(self.loot_whose(table));
                }
                ui.weak(t.fill("quests.used_in", &[("n", &table.places())]));
                ui.add(
                    egui::Label::new(egui::RichText::new(self.loot_summary(table)).weak())
                        .truncate(),
                );
            });
        });
    }

    /// The table opened: its fields, its changes, a roll of it, and what uses it.
    fn loot_card(&mut self, ui: &mut egui::Ui, table: &LootTable, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let back = ui.link(t.get("loot.back"));
        self.loot.buttons.push(("back".into(), back.rect));
        if back.clicked() {
            out.push(UiAction::Loot(LootAction::Open(None)));
        }
        ui.horizontal_wrapped(|ui| {
            ui.strong(t.fill(
                "loot.table_name",
                &[("kind", &t.get(kind_key(table.kind))), ("id", &table.id)],
            ));
            if let Some(s) = &table.symbol {
                ui.monospace(s);
            }
            ui.weak(self.loot_whose(table));
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("loot table", table.kind, table.id))
            .auto_shrink(false)
            .show(ui, |ui| {
                self.loot_changes(ui, table, out);
                if let Some(note) = self.loot.note.clone() {
                    ui.colored_label(super::panels::mark::FAULT, note);
                }
                ui.separator();
                let editable = matches!(
                    table.whose,
                    Whose::Pack { mine: true } | Whose::Changed { mine: true }
                );
                if let Some(d) = table.death().cloned() {
                    self.death_fields(ui, table.id, &d, editable, out);
                } else if let Some(rows) = table.wielded() {
                    let w = wielded::read(rows);
                    self.wielded_sets(ui, table.id, &w, editable, out);
                } else {
                    ui.colored_label(super::panels::mark::WARN, t.get("loot.held_nowhere_about"));
                }
                if table.rows.is_some() {
                    ui.separator();
                    self.loot_roll(ui, table, out);
                }
                ui.separator();
                self.loot_uses(ui, table, out);
            });
    }

    /// What may be done with the table: changed in this world and put back, copied, deleted.
    fn loot_changes(&mut self, ui: &mut egui::Ui, table: &LootTable, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let (kind, id) = (table.kind, table.id);
        let change = match table.whose {
            Whose::Server => Some(("change_here", LootAction::ChangeHere(kind, id))),
            Whose::Changed { mine: true } => Some(("put_back", LootAction::PutBack(kind, id))),
            Whose::Pack { mine: true } => Some(("delete", LootAction::Delete(kind, id))),
            _ => None,
        };
        ui.horizontal_wrapped(|ui| {
            if let Some((what, action)) = change {
                let r = ui.button(t.get(&format!("loot.{what}")));
                self.loot.buttons.push((what.into(), r.rect));
                if r.clicked() {
                    out.push(UiAction::Loot(action));
                }
            }
            if table.rows.is_some() {
                let name = self.new_loot_name(ui);
                let r = ui.button(t.get("loot.copy"));
                self.loot.buttons.push(("copy".into(), r.rect));
                if r.clicked() {
                    out.push(UiAction::Loot(LootAction::New {
                        kind,
                        from: Some(id),
                        first: None,
                        name,
                    }));
                }
            }
        });
        if matches!(
            table.whose,
            Whose::Pack { mine: false } | Whose::Changed { mine: false }
        ) {
            ui.weak(t.get("loot.another_pack"));
        }
    }

    /// The box a new table's name is typed in; the name typed, if any.
    fn new_loot_name(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let t = self.text.table();
        let r = ui.add(
            egui::TextEdit::singleline(&mut self.loot.new_name)
                .hint_text(t.get("loot.new_name_hint"))
                .desired_width(120.0),
        );
        self.loot.fields.push(("new_name".into(), r.rect));
        let name = self.loot.new_name.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// A field of the table opened: the text as typed while it is being typed, else `now`; what
    /// was typed when the field is left and differs from `now`.
    fn loot_field(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        now: &str,
        width: f32,
    ) -> Option<String> {
        let typing = format!("loot|{key}");
        let mut text = self
            .panels
            .form_typing
            .get(&typing)
            .cloned()
            .unwrap_or_else(|| now.to_owned());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width));
        self.loot.fields.push((key.to_owned(), r.rect));
        if r.has_focus() || r.changed() {
            self.panels.form_typing.insert(typing.clone(), text.clone());
        }
        if r.lost_focus() {
            self.panels.form_typing.remove(&typing);
            if text.trim() != now.trim() {
                return Some(text);
            }
        }
        None
    }

    /// A mix's words: what it draws from, each with its share; its number first.
    fn mix_words(&self, group: Group, n: i32) -> String {
        let t = self.text.table();
        if n == 0 {
            return t.get("loot.mix_none").to_owned();
        }
        let Some(mix) = usize::try_from(n - 1)
            .ok()
            .and_then(|i| group.mixes().get(i))
        else {
            return t.fill("loot.mix_unknown", &[("n", &n)]);
        };
        let parts: Vec<String> = mix
            .iter()
            .map(|(k, p)| {
                let key = format!("loot.kind_{}", k.key());
                let name = t.get(&key);
                if mix.len() == 1 {
                    name.to_owned()
                } else {
                    format!("{name} {}%", share_words(*p))
                }
            })
            .collect();
        format!("{n}: {}", parts.join(", "))
    }

    /// A heritage mix's words: whom things are made for, each with its share; its number first.
    fn heritage_words(&self, n: i32) -> String {
        let t = self.text.table();
        let Some(mix) = usize::try_from(n - 1)
            .ok()
            .and_then(|i| HERITAGE_MIXES.get(i))
        else {
            return t.fill("loot.heritage_any", &[("n", &n)]);
        };
        let parts: Vec<String> = mix
            .iter()
            .map(|(h, p)| {
                let key = format!("loot.heritage_{}", h.key());
                let name = t.get(&key);
                if mix.len() == 1 {
                    name.to_owned()
                } else {
                    format!("{name} {}%", share_words(*p))
                }
            })
            .collect();
        format!("{n}: {}", parts.join(", "))
    }

    /// A choice of column `column` of death treasure `id` among `choices` (each value with its
    /// words), showing `now`; read only when not `editable`.
    #[allow(clippy::too_many_arguments)]
    fn death_choice(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        column: &str,
        now: i32,
        choices: &[(i32, String)],
        editable: bool,
        out: &mut Vec<UiAction>,
    ) {
        let shown = choices
            .iter()
            .find(|(v, _)| *v == now)
            .map_or_else(|| now.to_string(), |(_, w)| w.clone());
        if !editable {
            let r = ui.add(egui::Label::new(&shown).truncate());
            r.on_hover_text(&shown);
            return;
        }
        let r = egui::ComboBox::from_id_salt(("loot choice", id, column))
            .selected_text(shown)
            .width(160.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                for (v, words) in choices {
                    if ui.selectable_label(*v == now, words).clicked() && *v != now {
                        out.push(UiAction::Loot(LootAction::SetDeath {
                            id,
                            column: column.to_owned(),
                            value: v.to_string(),
                        }));
                    }
                }
            });
        self.loot.fields.push((column.to_owned(), r.response.rect));
    }

    /// A number field of column `column` of death treasure `id`, showing `now`, with `after` beside
    /// it; read only when not `editable`.
    #[allow(clippy::too_many_arguments)]
    fn death_number(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        column: &str,
        now: &str,
        after: &str,
        editable: bool,
        out: &mut Vec<UiAction>,
    ) {
        ui.horizontal(|ui| {
            if editable {
                if let Some(v) = self.loot_field(ui, column, now, 36.0) {
                    out.push(UiAction::Loot(LootAction::SetDeath {
                        id,
                        column: column.to_owned(),
                        value: v.trim().trim_end_matches('%').to_owned(),
                    }));
                }
                if !after.is_empty() {
                    ui.weak(after);
                }
            } else {
                ui.label(format!("{now}{after}"));
            }
        });
    }

    /// A death treasure's fields: its tier, quality and heritage, and each group's chance, least,
    /// most and mix; the pack's own editable.
    fn death_fields(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        d: &Treasure,
        editable: bool,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let tiers: Vec<(i32, String)> = TIERS.map(|n| (n, n.to_string())).collect();
        let heritages: Vec<(i32, String)> = (1..=i32::try_from(HERITAGE_MIXES.len()).unwrap_or(0))
            .map(|n| (n, self.heritage_words(n)))
            .collect();
        egui::Grid::new(("death fields", id))
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label(t.get("loot.tier"));
                self.death_choice(ui, id, "tier", d.tier, &tiers, editable, out);
                ui.end_row();
                ui.label(t.get("loot.quality"));
                let q = format!("{}", d.quality);
                self.death_number(ui, id, "loot_Quality_Mod", &q, "", editable, out);
                ui.end_row();
                ui.label(t.get("loot.heritage"));
                self.death_choice(
                    ui,
                    id,
                    "unknown_Chances",
                    d.heritage,
                    &heritages,
                    editable,
                    out,
                );
                ui.end_row();
            });
        ui.add_space(4.0);
        egui::Grid::new(("death groups", id))
            .num_columns(5)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("");
                for k in ["loot.chance", "loot.least", "loot.most", "loot.drawn_from"] {
                    ui.weak(t.get(k));
                }
                ui.end_row();
                for g in Group::ALL {
                    let (chance, lo, hi, mix) = g.of(d);
                    let [c_col, lo_col, hi_col, mix_col] = g.columns();
                    ui.label(t.get(&format!("loot.{}", g.key())));
                    self.death_number(ui, id, c_col, &chance.to_string(), "%", editable, out);
                    self.death_number(ui, id, lo_col, &lo.to_string(), "", editable, out);
                    self.death_number(ui, id, hi_col, &hi.to_string(), "", editable, out);
                    let choices: Vec<(i32, String)> = (0..=i32::try_from(g.mixes().len())
                        .unwrap_or(0))
                        .map(|n| (n, self.mix_words(g, n)))
                        .collect();
                    self.death_choice(ui, id, mix_col, mix, &choices, editable, out);
                    ui.end_row();
                }
            });
    }

    /// A wielded treasure's sets, as the server draws them: each entry's chance in its set, what
    /// each set makes nothing at, and under an entry the sets drawn when it is made; the pack's
    /// own editable.
    fn wielded_sets(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        w: &Wielded,
        editable: bool,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        if w.is_empty() {
            ui.weak(t.get("loot.makes_nothing"));
        }
        let mut budget = super::PICTURES_A_FRAME;
        self.wielded_level(ui, id, w, &[], &w.sets, editable, &mut budget, out);
        if editable {
            let salt = format!("loot:{id}:add-set");
            if let Some(tok) = self.weenie_picker(ui, &salt, t.get("collections.add_set")) {
                let mut new = w.clone();
                let edit = new.add_set(&[], item_of(&tok));
                out.push(write_wielded(id, &new, edit, &t));
            }
        }
        if !w.unread.is_empty() {
            ui.colored_label(
                super::panels::mark::WARN,
                t.fill("loot.unread", &[("n", &w.unread.len())]),
            );
            for r in &w.unread {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    self.weenie_row(ui, &r.weenie, None, &mut budget, out);
                });
            }
        }
    }

    /// The sets of one level (under the entry at `at`, the top's for none), indented by its depth.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn wielded_level(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        w: &Wielded,
        at: &[(usize, usize)],
        sets: &[wielded::Set],
        editable: bool,
        budget: &mut u32,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let indent = 16.0 * at.len() as f32;
        let place = |at: &[(usize, usize)]| {
            at.iter()
                .map(|(s, e)| format!("{s}.{e}"))
                .collect::<Vec<_>>()
                .join("/")
        };
        for (s, set) in sets.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(indent);
                ui.strong(t.fill("collections.set", &[("n", &(s + 1))]));
                if editable {
                    let salt = format!("loot:{id}:{}:add-to:{s}", place(at));
                    if let Some(tok) =
                        self.weenie_picker(ui, &salt, t.get("collections.add_to_set"))
                    {
                        let mut new = w.clone();
                        let edit = new.add_to_set(at, s, item_of(&tok));
                        out.push(write_wielded(id, &new, edit, &t));
                    }
                }
            });
            // The entries with a field being typed in: drawn whether they are seen or not.
            let typed: Vec<String> = self
                .panels
                .form_typing
                .keys()
                .filter_map(|k| k.strip_prefix("loot|").map(str::to_owned))
                .collect();
            let held = |e: usize| {
                let mut here = at.to_vec();
                here.push((s, e));
                let key = place(&here);
                typed.iter().any(|k| {
                    k.strip_prefix(&key)
                        .is_some_and(|r| r.starts_with(':') || r.starts_with('/'))
                })
            };
            let rows = egui::Id::new(("wielded entries", id, place(at), s));
            super::seen::blocks(ui, rows, set.entries.len(), 50.0, held, |ui, e| {
                let entry = &set.entries[e];
                let mut here = at.to_vec();
                here.push((s, e));
                let key = place(&here);
                // Its line: what it makes, its chance, and its order; under it, its own fields.
                ui.horizontal(|ui| {
                    ui.add_space(indent + 12.0);
                    if lyceum_foundry::sql_ids::number(&entry.item.weenie) == Some(0) {
                        ui.weak(t.get("collections.nothing"));
                    } else {
                        let many = (!editable && entry.item.stack > 1)
                            .then(|| i64::from(entry.item.stack));
                        self.weenie_row(ui, &entry.item.weenie, many, budget, out);
                    }
                    let chance = super::collections::percent_words(entry.chance * 100.0);
                    if !editable {
                        ui.weak(format!("{chance}%"));
                        return;
                    }
                    if let Some(v) = self.loot_field(ui, &format!("{key}:chance"), &chance, 46.0) {
                        let mut new = w.clone();
                        let edit = match super::collections::percent_typed(&v) {
                            Some(p) => new.set_chance(&here, p),
                            None => Err(Refused::NoChance),
                        };
                        out.push(write_wielded(id, &new, edit, &t));
                    }
                    ui.weak("%");
                    let up = e > 0;
                    let down = e + 1 < set.entries.len();
                    for (enabled, later, glyph) in [(up, false, "▲"), (down, true, "▼")] {
                        if ui
                            .add_enabled(enabled, egui::Button::new(glyph).small())
                            .clicked()
                        {
                            let mut new = w.clone();
                            let edit = new.swap(&here, later);
                            out.push(write_wielded(id, &new, edit, &t));
                        }
                    }
                    let x = ui.small_button("×");
                    self.loot.buttons.push((format!("{key}:remove"), x.rect));
                    if x.clicked() {
                        let mut new = w.clone();
                        let edit = new.remove(&here);
                        out.push(write_wielded(id, &new, edit, &t));
                    }
                });
                if editable {
                    // Wrapped under its line, each wrapped line indented as the first.
                    ui.horizontal(|ui| {
                        ui.add_space(indent + 24.0);
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                self.wielded_item_fields(ui, id, w, &here, &key, &entry.item, out);
                                let salt = format!("loot:{id}:{key}:under");
                                if let Some(tok) =
                                    self.weenie_picker(ui, &salt, t.get("loot.add_under"))
                                {
                                    let mut new = w.clone();
                                    let edit = new.add_set(&here, item_of(&tok));
                                    out.push(write_wielded(id, &new, edit, &t));
                                }
                            });
                        });
                    });
                }
                if !entry.then.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(indent + 24.0);
                        ui.weak(t.get("loot.when_made"));
                    });
                    self.wielded_level(ui, id, w, &here, &entry.then, editable, budget, out);
                }
            });
            if set.nothing() > 1e-6 {
                ui.horizontal(|ui| {
                    ui.add_space(indent + 12.0);
                    ui.weak(t.get("collections.nothing_rest"));
                    ui.weak(format!(
                        "{}%",
                        super::collections::percent_words(set.nothing() * 100.0)
                    ));
                });
            }
        }
    }

    /// An entry's own fields: how many, by how many fewer at most, its color and shade.
    #[allow(clippy::too_many_arguments)]
    fn wielded_item_fields(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        w: &Wielded,
        at: &[(usize, usize)],
        key: &str,
        item: &WieldedItem,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let fields: [(&str, String, f32); 4] = [
            ("stack", item.stack.max(1).to_string(), 34.0),
            (
                "variance",
                super::collections::percent_words(f64::from(item.variance) * 100.0),
                34.0,
            ),
            ("palette", item.palette.to_string(), 30.0),
            ("shade", format!("{}", item.shade), 40.0),
        ];
        for (column, now, width) in fields {
            ui.weak(t.get(&format!("loot.item_{column}")));
            let Some(v) = self.loot_field(ui, &format!("{key}:{column}"), &now, width) else {
                continue;
            };
            let v = v.trim().trim_end_matches('%').trim().to_owned();
            let mut new = w.clone();
            let edit = match new.entry_mut(at) {
                None => Err(Refused::Gone),
                Some(e) => {
                    let ok = match column {
                        "stack" => v.parse::<i32>().ok().filter(|n| *n >= 1).map(|n| {
                            e.item.stack = n;
                        }),
                        "variance" => v
                            .parse::<f32>()
                            .ok()
                            .filter(|p| (0.0..=100.0).contains(p))
                            .map(|p| e.item.variance = p / 100.0),
                        "palette" => v.parse::<u32>().ok().map(|n| e.item.palette = n),
                        _ => v
                            .parse::<f32>()
                            .ok()
                            .filter(|s| (0.0..=1.0).contains(s))
                            .map(|s| e.item.shade = s),
                    };
                    ok.ok_or(Refused::NoChance)
                }
            };
            let action = match edit {
                Ok(()) => LootAction::SetWielded {
                    id,
                    rows: wielded::write(&new),
                },
                Err(_) => LootAction::Refused(t.get("messages.not_a_value").to_owned()),
            };
            out.push(UiAction::Loot(action));
        }
    }

    /// Roll it: the table drawn on the editor's world, so many times, and what it made.
    fn loot_roll(&mut self, ui: &mut egui::Ui, table: &LootTable, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let live = self.live.is_some();
        ui.horizontal(|ui| {
            let r = ui.add_enabled(live, egui::Button::new(t.get("loot.roll")));
            let r = r.on_disabled_hover_text(t.get("loot.roll_needs_live"));
            self.loot.buttons.push(("roll".into(), r.rect));
            let times = self.loot.times.trim().parse::<u32>().ok();
            let f = ui.add(egui::TextEdit::singleline(&mut self.loot.times).desired_width(40.0));
            self.loot.fields.push(("times".into(), f.rect));
            ui.weak(t.get("loot.times"));
            if r.clicked() {
                out.push(UiAction::Loot(
                    match times.filter(|n| (1..=MOST_TIMES).contains(n)) {
                        Some(times) => LootAction::Roll {
                            id: table.id,
                            times,
                        },
                        None => {
                            LootAction::Refused(t.fill("loot.times_bad", &[("most", &MOST_TIMES)]))
                        }
                    },
                ));
            }
        });
        if !live {
            ui.weak(t.get("loot.roll_needs_live"));
        }
        if self.loot.rolling.as_ref().is_some_and(|r| r.id == table.id) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(t.get("loot.rolling"));
            });
            ui.ctx().request_repaint();
        }
        if table.kind == LootKind::Wielded && self.session.treasure(table.id).is_some() {
            ui.weak(t.get("loot.roll_takes_death"));
        }
        let Some(rolled) = self.loot.rolled.clone().filter(|r| r.id == table.id) else {
            return;
        };
        match rolled.draws {
            Err(why) => {
                ui.colored_label(super::panels::mark::FAULT, why);
            }
            Ok(draws) => self.roll_result(ui, &draws, out),
        }
    }

    /// What a roll made: how many draws made nothing, then each thing by its picture and name,
    /// in how many draws and how many in all, the commonest first.
    fn roll_result(
        &mut self,
        ui: &mut egui::Ui,
        draws: &[crate::live::Draw],
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let nothing = draws.iter().filter(|d| d.is_empty()).count();
        ui.label(t.fill("loot.rolled", &[("n", &draws.len()), ("nothing", &nothing)]));
        // Each thing: in how many draws, how many in all, and its name.
        let mut made: Vec<(u32, String, usize, i64)> = Vec::new();
        for d in draws {
            let mut seen: Vec<u32> = Vec::new();
            for (class, n, name) in d {
                let first = !seen.contains(class);
                seen.push(*class);
                match made.iter_mut().find(|m| m.0 == *class) {
                    Some(m) => {
                        m.2 += usize::from(first);
                        m.3 += i64::from(*n);
                    }
                    None => made.push((*class, name.clone(), 1, i64::from(*n))),
                }
            }
        }
        made.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
        let mut budget = super::PICTURES_A_FRAME;
        for (class, name, draws_with, total) in made.iter().take(USES_SHOWN) {
            ui.horizontal(|ui| {
                let w = Weenie::Class(*class);
                if let Some(id) = self
                    .weenie_entry(&w)
                    .and_then(|e| e.setup)
                    .and_then(|s| self.picture(DataId(s), &mut budget))
                {
                    ui.add(egui::Image::new((id, egui::vec2(24.0, 24.0))));
                }
                if ui.link(name).clicked() {
                    out.push(UiAction::Weenie(w));
                    out.push(UiAction::ShowWeenie);
                }
                ui.weak(t.fill(
                    "loot.made_in",
                    &[("n", draws_with), ("of", &draws.len()), ("all", total)],
                ));
            });
        }
    }

    /// Every weenie that names the table, by name, with how: a click opens it.
    fn loot_uses(&mut self, ui: &mut egui::Ui, table: &LootTable, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let list = self.loot_use_lines(table);
        ui.strong(t.fill("quests.uses", &[("n", &list.len())]));
        if list.is_empty() {
            ui.weak(t.get("quests.uses_none"));
        }
        if self.session.server_weenies().is_empty() {
            ui.weak(t.get("usages.packs_only"));
        }
        let id = egui::Id::new(("loot uses", table.kind, table.id));
        super::seen::blocks(
            ui,
            id,
            list.len(),
            20.0,
            |_| false,
            |ui, i| {
                let (name, w, server, how) = &list[i];
                ui.horizontal_wrapped(|ui| {
                    let r = ui.link(name);
                    self.loot.use_rows.push((w.clone(), r.rect));
                    if r.clicked() {
                        out.push(UiAction::Loot(LootAction::OpenUse(w.clone())));
                    }
                    let mut words: Vec<&str> = how.iter().map(|h| t.get(use_key(*h))).collect();
                    if *server {
                        words.push(t.get("quests.whose_server"));
                    }
                    ui.weak(words.join(" • "));
                });
            },
        );
    }

    /// The uses of `table`, a row for each weenie, by name: made again only once the tables are
    /// gathered again.
    fn loot_use_lines(&mut self, table: &LootTable) -> Rc<Vec<UseLine>> {
        let key = (table.kind, table.id);
        let gathered = self.loot.gathered.as_ref().map(|g| Rc::clone(&g.tables));
        if let (Some((k, of, lines)), Some(now)) = (&self.loot.uses, &gathered) {
            if *k == key && Rc::ptr_eq(of, now) {
                return Rc::clone(lines);
            }
        }
        let t = self.text.table();
        let mut list: Vec<UseLine> = by_weenie(&table.uses)
            .into_iter()
            .map(|(w, server, how)| {
                let name = self
                    .weenie_entry(&w)
                    .map_or_else(|| super::form::weenie_label(&w, &t), |e| e.name.clone());
                (name, w, server, how)
            })
            .collect();
        list.sort_by_cached_key(|a| a.0.to_lowercase());
        let list = Rc::new(list);
        self.loot.uses = gathered.map(|g| (key, g, Rc::clone(&list)));
        list
    }

    /// A generator's treasure entry's words, `Loot table 37 (death)`, and the table it names.
    pub(super) fn treasure_entry(&self, written: &str) -> Option<(String, LootKind, u32)> {
        let id = self.session.loot_id_of(written)?;
        let kind = self.session.loot_kind_of(id);
        let t = self.text.table();
        Some((
            t.fill(
                "loot.generator_entry",
                &[("id", &id), ("kind", &t.get(kind_key(kind)).to_lowercase())],
            ),
            kind,
            id,
        ))
    }

    /// Loot table `written` of `kind` (its id, or a pack's token), as a field naming it shows it:
    /// a death treasure by its tier and quality, a wielded one by what it makes; and a pack's
    /// by its name too.
    pub(super) fn loot_field_words(&mut self, kind: LootKind, written: &str) -> String {
        let Some(id) = self.session.loot_id_of(written) else {
            return written.trim().to_owned();
        };
        match self.loot_table(kind, id) {
            Some(table) => self.loot_choice_words(&table),
            None => id.to_string(),
        }
    }

    /// Table `table` as a field's choice shows it.
    fn loot_choice_words(&self, table: &LootTable) -> String {
        let t = self.text.table();
        let n = table.id;
        let words = match table.death() {
            Some(d) => t.fill(
                "fields.treasure_choice",
                &[("n", &n), ("tier", &d.tier), ("quality", &d.quality)],
            ),
            None if table.rows.is_some() => t.fill(
                "loot.wielded_choice",
                &[("n", &n), ("summary", &self.loot_summary(table))],
            ),
            None => n.to_string(),
        };
        match table.symbol_name() {
            Some(name) => format!("{words} ({name})"),
            None => words,
        }
    }

    /// Every table of `kind` the world holds, as a field offers them: its id, its words, and how
    /// the pack being edited writes it (a pack's by its symbol's token).
    pub(super) fn loot_choices(&mut self, kind: LootKind) -> Vec<(u32, String, String)> {
        let (all, _) = self.world_loot();
        all.iter()
            .filter(|t| t.kind == kind && t.rows.is_some())
            .map(|t| {
                (
                    t.id,
                    self.loot_choice_words(t),
                    self.session.loot_written(t),
                )
            })
            .collect()
    }

    /// A link to the loot table a generator's treasure entry names, as the generator's lists show
    /// it in place of a weenie.
    pub(super) fn treasure_link(
        &self,
        ui: &mut egui::Ui,
        written: &str,
        out: &mut Vec<UiAction>,
    ) -> egui::Response {
        let t = self.text.table();
        let Some((words, kind, id)) = self.treasure_entry(written) else {
            return ui.weak(written.trim());
        };
        let r = ui.link(words).on_hover_text(t.get("loot.open_in_tab"));
        if r.clicked() {
            out.push(UiAction::Loot(LootAction::Open(Some((kind, id)))));
        }
        r
    }
}

/// A wielded treasure's row making `written` (as a list's row writes it).
fn item_of(written: &str) -> WieldedItem {
    WieldedItem {
        weenie: written.trim().to_owned(),
        ..WieldedItem::default()
    }
}

/// The change that writes `w` as wielded treasure `id`, once `edit` was made of it; or what says
/// why not.
fn write_wielded(
    id: u32,
    w: &Wielded,
    edit: Result<(), Refused>,
    t: &crate::text::Table,
) -> UiAction {
    UiAction::Loot(match edit {
        Ok(()) => LootAction::SetWielded {
            id,
            rows: wielded::write(w),
        },
        Err(Refused::TooLikely { most }) => LootAction::Refused(t.fill(
            "collections.too_likely",
            &[("most", &super::collections::percent_words(most))],
        )),
        Err(Refused::NoChance) => LootAction::Refused(t.get("loot.no_chance").to_owned()),
        Err(Refused::Gone) => LootAction::Refused(t.get("messages.not_a_value").to_owned()),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's Loot tab)
    use super::*;

    fn table(kind: LootKind, id: u32, whose: Whose, uses: Vec<LootUser>) -> LootTable {
        LootTable {
            kind,
            id,
            rows: None,
            server: None,
            file: None,
            pack: None,
            symbol: None,
            whose,
            uses,
        }
    }

    #[test]
    fn the_list_is_searched_by_its_words_and_filtered_by_kind_and_whose() {
        let all = [
            table(LootKind::Death, 4, Whose::Server, Vec::new()),
            table(
                LootKind::Death,
                7,
                Whose::Changed { mine: true },
                Vec::new(),
            ),
            table(LootKind::Wielded, 37, Whose::Server, Vec::new()),
            table(
                LootKind::Wielded,
                100_000,
                Whose::Pack { mine: true },
                Vec::new(),
            ),
        ];
        let words: Vec<String> = [
            "death 4 drudge",
            "death 7",
            "wielded 37 club",
            "wielded 100000",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let ids = |f: &str, k: (bool, bool, bool)| -> Vec<u32> {
            shown(&all, &words, f, k).iter().map(|t| t.id).collect()
        };
        assert_eq!(ids("", (true, true, false)), [4, 7, 37, 100_000]);
        assert_eq!(ids("club", (true, true, false)), [37]);
        assert_eq!(ids("", (true, false, false)), [4, 7]);
        assert_eq!(
            ids("", (true, true, true)),
            [7, 100_000],
            "this pack's only"
        );
        assert_eq!(ids("drudge", (false, true, false)), Vec::<u32>::new());
    }

    #[test]
    fn each_weenie_is_listed_once_with_each_way_it_names_the_table() {
        let user = |by: u32, how: LootUse, server: bool| LootUser {
            by: Weenie::Class(by),
            how,
            file: (!server).then(|| "a.sql".into()),
        };
        let uses = [
            user(1, LootUse::Dies, true),
            user(1, LootUse::Generator { place: 72 }, true),
            user(1, LootUse::Generator { place: 66 }, true),
            user(2, LootUse::Wields, false),
        ];
        assert_eq!(
            by_weenie(&uses),
            [
                (
                    Weenie::Class(1),
                    true,
                    vec![LootUse::Dies, LootUse::Generator { place: 0 }]
                ),
                (Weenie::Class(2), false, vec![LootUse::Wields]),
            ]
        );
    }

    #[test]
    fn every_word_the_tab_shows_is_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let mut need: Vec<String> = Vec::new();
        for h in [
            LootUse::Dies,
            LootUse::Wields,
            LootUse::Generator { place: 0 },
        ] {
            need.push(use_key(h).to_owned());
        }
        for k in LootKind::ALL {
            need.push(kind_key(k).to_owned());
        }
        for g in Group::ALL {
            need.push(format!("loot.{}", g.key()));
            for mix in g.mixes() {
                for (k, _) in *mix {
                    need.push(format!("loot.kind_{}", k.key()));
                }
            }
        }
        for mix in HERITAGE_MIXES {
            for (h, _) in *mix {
                need.push(format!("loot.heritage_{}", h.key()));
            }
        }
        for what in ["change_here", "put_back", "delete"] {
            need.push(format!("loot.{what}"));
        }
        for c in ["stack", "variance", "palette", "shade"] {
            need.push(format!("loot.item_{c}"));
        }
        // Every word of the tab's own.
        let code = include_str!("loot_tab.rs");
        let code = code.split("#[cfg(test)]").next().unwrap_or_default();
        let mut rest = code;
        while let Some(at) = rest.find("\"loot.") {
            let tail = &rest[at + 1..];
            let end = tail.find('"').unwrap_or(0);
            let key = &tail[..end];
            if !key.contains('{') {
                need.push(key.to_owned());
            }
            rest = &tail[end..];
        }
        let missing: Vec<&String> = need.iter().filter(|k| !t.contains_key(*k)).collect();
        assert!(missing.is_empty(), "{missing:?}");
    }
}
