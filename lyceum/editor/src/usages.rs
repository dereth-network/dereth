//! Find usages: the weenies whose create lists, generators and emotes name a weenie, each with
//! how, and the wielded loot tables that hold it. The packs'
//! files are read for it as they stand; the server's own weenies come from what its database said
//! of them when the world opened (read once, off the frame loop), but for one a pack writes again,
//! whose pack's file stands for it.

use std::rc::Rc;

use lyceum_foundry::edit::Weenie;
use lyceum_foundry::loot::LootKind;
use lyceum_foundry::sql_ids::Use;

use super::collections::{dest, written_as};
use super::definition::Drill;
use super::{EditorApp, LootAction, Selection, UiAction};

/// What a weenie's usages were last found to be: after which edit, with how many of the
/// server's weenies known, of which weenie, the usages, and the wielded loot tables that hold it.
#[derive(Debug, Clone)]
pub struct UsedBy {
    edits: u64,
    learned: usize,
    of: Weenie,
    found: Rc<Vec<(Weenie, Use)>>,
    loot: Rc<Vec<u32>>,
    /// The list's rows as last made from them, and the text they were worded by.
    rows: Option<(Rc<crate::text::Table>, Rc<Vec<UseRow>>)>,
}

/// A row of the list of what names a weenie: the weenie's name, the weenie, and how it names it.
type UseRow = (String, Weenie, String);

/// The key of the words saying how a weenie names another, and the category of the emote that
/// does, when one does.
#[must_use]
pub fn use_key(how: Use) -> (&'static str, Option<u32>) {
    match how {
        Use::CreateList { destination: d } => (
            if d & dest::SHOP != 0 {
                "usages.sells"
            } else if d & dest::WIELD != 0 {
                if d & dest::TREASURE != 0 {
                    "usages.wields_by_chance"
                } else {
                    "usages.wields"
                }
            } else if d & dest::CONTAIN != 0 {
                if d & dest::TREASURE != 0 {
                    "usages.holds_by_chance"
                } else {
                    "usages.holds"
                }
            } else if d & dest::TREASURE != 0 {
                "usages.drops"
            } else {
                "usages.lists"
            },
            None,
        ),
        Use::Generator => ("usages.spawns", None),
        Use::Emote {
            category,
            action: None,
        } => (
            match category {
                1 => "usages.refuses",
                6 => "usages.takes_given",
                _ => "usages.answers",
            },
            Some(category),
        ),
        Use::Emote {
            category,
            action: Some(ty),
        } => (
            match ty {
                3 => "usages.gives",
                74 => "usages.takes",
                76 => "usages.asks_for",
                _ => "usages.names",
            },
            Some(category),
        ),
    }
}

impl EditorApp {
    /// The Definition pane on weenie `w`, opened into the weenies that name it, and brought
    /// forward: the spawn browser's choice when the pane shows another.
    pub fn find_usages(&mut self, w: &Weenie) {
        if self.form_weenie().as_ref() != Some(w) {
            self.panels.weenie = Some(w.clone());
            if matches!(self.selection, Some(Selection::Spawn { .. })) {
                self.selection = None;
                self.also.clear();
            }
        }
        self.definition.drill_into(w, Drill::UsedBy);
        self.show_weenie();
    }

    /// Every weenie that names weenie `w`, and how, as the packs and the server have them now:
    /// found again only after an edit, or once more of the server's weenies are known.
    pub fn usages_of(&mut self, w: &Weenie) -> Rc<Vec<(Weenie, Use)>> {
        Rc::clone(&self.used_by(w).found)
    }

    /// The wielded loot tables that hold weenie `w`, by id, found again as [`Self::usages_of`]
    /// finds its usages.
    pub fn loot_holding_of(&mut self, w: &Weenie) -> Rc<Vec<u32>> {
        Rc::clone(&self.used_by(w).loot)
    }

    fn used_by(&mut self, w: &Weenie) -> &UsedBy {
        let (edits, learned) = (self.stats.edits, self.session.server_weenies().len());
        let fresh = self
            .definition
            .used_by
            .as_ref()
            .is_some_and(|u| u.edits == edits && u.learned == learned && u.of == *w);
        if !fresh {
            self.definition.used_by = Some(UsedBy {
                edits,
                learned,
                of: w.clone(),
                found: Rc::new(self.session.uses_of(w)),
                loot: Rc::new(self.session.loot_holding(w)),
                rows: None,
            });
        }
        self.definition.used_by.as_ref().expect("found just now")
    }

    /// How a weenie names another, in words: what its list does with it, or its emote's, and
    /// which emote.
    fn use_words(&self, how: Use) -> String {
        let t = self.text.table();
        let (key, category) = use_key(how);
        match category {
            Some(c) => t.fill(
                key,
                &[(
                    "emote",
                    &t.or(&format!("emotes.category_{c}"), "").to_owned(),
                )],
            ),
            None => t.get(key).to_owned(),
        }
    }

    /// The Definition pane's list of what names weenie `w`: each weenie by its picture and
    /// name, a click on which shows it, and how it names it.
    pub(super) fn used_by_list(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let rows = self.use_rows(w);
        let loot = self.loot_holding_of(w);
        if rows.is_empty() && loot.is_empty() {
            ui.weak(t.get("usages.none"));
        }
        // The wielded loot tables that hold it, each a click from the Loot tab.
        for id in loot.iter() {
            ui.horizontal(|ui| {
                if ui.link(t.fill("usages.in_loot", &[("id", id)])).clicked() {
                    out.push(UiAction::Loot(LootAction::Open(Some((
                        LootKind::Wielded,
                        *id,
                    )))));
                }
                ui.weak(t.get("usages.lists"));
            });
        }
        if self.session.server_weenies().is_empty() {
            ui.weak(t.get("usages.packs_only"));
        }
        let mut budget = super::PICTURES_A_FRAME;
        let id = egui::Id::new(("used by", w.clone()));
        super::seen::blocks(
            ui,
            id,
            rows.len(),
            36.0,
            |_| false,
            |ui, i| {
                let (_, by, words) = &rows[i];
                ui.horizontal(|ui| {
                    self.weenie_row(ui, &written_as(by), None, &mut budget, out);
                    ui.weak(words);
                });
            },
        );
    }

    /// The rows of the list of what names weenie `w`: each weenie once for each way it names it,
    /// by name; made again only when its usages are found again or the text changes.
    fn use_rows(&mut self, w: &Weenie) -> Rc<Vec<UseRow>> {
        let t = self.text.table();
        if let Some((by, rows)) = self.used_by(w).rows.as_ref() {
            if Rc::ptr_eq(by, &t) {
                return Rc::clone(rows);
            }
        }
        let found = self.usages_of(w);
        let mut seen: std::collections::HashSet<(Weenie, String)> =
            std::collections::HashSet::new();
        let mut rows: Vec<UseRow> = Vec::new();
        for (by, how) in found.iter() {
            let words = self.use_words(*how);
            if !seen.insert((by.clone(), words.clone())) {
                continue;
            }
            let name = self
                .weenie_entry(by)
                .map_or_else(|| super::form::weenie_label(by, &t), |e| e.name.clone());
            rows.push((name, by.clone(), words));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
        let rows = Rc::new(rows);
        if let Some(u) = self.definition.used_by.as_mut() {
            u.rows = Some((t, Rc::clone(&rows)));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's Find usages)
    use super::*;

    #[test]
    fn how_a_weenie_is_named_has_words_for_every_list_generator_and_emote() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let ways = [
            Use::CreateList { destination: 4 },
            Use::CreateList { destination: 2 },
            Use::CreateList { destination: 10 },
            Use::CreateList { destination: 1 },
            Use::CreateList { destination: 9 },
            Use::CreateList { destination: 8 },
            Use::CreateList { destination: 0 },
            Use::Generator,
            Use::Emote {
                category: 1,
                action: None,
            },
            Use::Emote {
                category: 6,
                action: None,
            },
            Use::Emote {
                category: 22,
                action: None,
            },
            Use::Emote {
                category: 7,
                action: Some(3),
            },
            Use::Emote {
                category: 7,
                action: Some(74),
            },
            Use::Emote {
                category: 7,
                action: Some(76),
            },
            Use::Emote {
                category: 7,
                action: Some(10),
            },
        ];
        let mut keys: Vec<&str> = ways.iter().map(|w| use_key(*w).0).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 15, "each way its own words");
        for k in keys
            .iter()
            .chain(&["usages.none", "usages.packs_only", "usages.more"])
        {
            assert!(t.contains_key(*k), "{k}");
        }
        // An emote's way says which emote.
        assert!(t["usages.gives"].contains("{emote}"));
        assert_eq!(use_key(Use::Generator), ("usages.spawns", None));
    }
}
