//! Where a weenie appears: every spawn of it in the world, the packs' and the server's own in
//! every landblock, loaded or not, by landblock under the landblock's dungeon or town and its
//! count, a page at a time. A click on one flies there: into its room, for one indoors.
//! Gathered again only after an edit, or once more of the server's world is known; the
//! server's spawns of a class come from the session's index of them, not a scan of its world.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;

use lyceum_foundry::edit::{Spawn, Weenie};

use super::{EditorApp, UiAction};

/// How many spawns a page lists.
pub const PAGE: usize = 40;

/// How many weenies' lists are kept at once: the Definition pane's and the editor's.
const KEPT: usize = 2;

/// A weenie's spawns as gathered: of which weenie, the spawns by landblock, and each
/// landblock's place (its dungeon's name or its town, when the editor knows one) and count.
#[derive(Debug, Clone)]
struct Gathered {
    of: Weenie,
    found: Rc<Vec<Spawn>>,
    places: Rc<BTreeMap<u16, (String, usize)>>,
}

/// The list's state: the weenies' lists gathered since the last edit (with how much of the
/// server's world and its dungeons was known then), the page each is shown at, and where each
/// part was drawn.
#[derive(Debug, Clone, Default)]
pub struct Appears {
    gathered: Vec<Gathered>,
    after: (u64, usize, bool),
    pages: HashMap<Weenie, usize>,
    /// Each spawn's button as drawn last (those in view): its landblock, cell, file and guid.
    pub rows: Vec<(u16, u32, PathBuf, String, egui::Rect)>,
    /// Each landblock's words as drawn last: the landblock and its count.
    pub groups: Vec<(u16, usize, egui::Rect)>,
    /// The page buttons, as drawn last.
    pub previous: Option<egui::Rect>,
    pub next: Option<egui::Rect>,
}

impl Appears {
    /// The page weenie `w`'s list is shown at, from 0.
    #[must_use]
    pub fn page(&self, w: &Weenie) -> usize {
        self.pages.get(w).copied().unwrap_or(0)
    }
}

impl EditorApp {
    /// Every spawn of weenie `w` in the world, by landblock: gathered again only after an edit,
    /// or once more of the server's world or its dungeons are known.
    pub fn appears_of(&mut self, w: &Weenie) -> Rc<Vec<Spawn>> {
        Rc::clone(&self.appears_gathered(w).found)
    }

    fn appears_gathered(&mut self, w: &Weenie) -> Gathered {
        let after = (
            self.stats.edits,
            self.session.server_weenies().len(),
            self.dungeon_list.retail.is_some(),
        );
        let a = &mut self.definition.appears;
        if a.after != after {
            a.after = after;
            a.gathered.clear();
        }
        if let Some(g) = a.gathered.iter().find(|g| g.of == *w) {
            return g.clone();
        }
        self.stats.appears_gathers += 1;
        let found = self.session.appearances(w);
        // A dungeon's name, once the world's dungeons are known; else the town it is in.
        let dungeons: BTreeMap<u16, String> = if after.2 {
            self.dungeons()
                .into_iter()
                .map(|d| (d.landblock, d.name.unwrap_or_default()))
                .collect()
        } else {
            BTreeMap::new()
        };
        let mut places: BTreeMap<u16, (String, usize)> = BTreeMap::new();
        for s in &found {
            let lb = s.landblock();
            places
                .entry(lb)
                .or_insert_with(|| {
                    let place = dungeons
                        .get(&lb)
                        .cloned()
                        .unwrap_or_else(|| crate::catalog::town_of(lb).unwrap_or_default());
                    (place, 0)
                })
                .1 += 1;
        }
        let g = Gathered {
            of: w.clone(),
            found: Rc::new(found),
            places: Rc::new(places),
        };
        let a = &mut self.definition.appears;
        a.gathered.insert(0, g.clone());
        a.gathered.truncate(KEPT);
        g
    }

    /// The list of where weenie `w` appears, a page at a time under the way to the others: each
    /// landblock's place and count, then a button for each of its spawns there, a click on which
    /// flies to it.
    pub(super) fn appears_list(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let g = self.appears_gathered(w);
        // The server's database being fetched or read: the packs' spawns only, till it is.
        let reading = (self.fetching.is_some()
            || self
                .catalog
                .as_ref()
                .is_some_and(crate::catalog::Catalog::reading_database))
            && self.session.server_weenies().is_empty();
        let a = &mut self.definition.appears;
        a.rows.clear();
        a.groups.clear();
        a.previous = None;
        a.next = None;
        if reading {
            ui.weak(t.get("flows.appears_reading"));
            ui.ctx().request_repaint();
        } else if g.found.is_empty() {
            ui.weak(t.get("flows.appears_nowhere"));
        }
        let total = g.found.len();
        let pages = total.div_ceil(PAGE).max(1);
        let page = a.page(w).min(pages - 1);
        let shown = &g.found[page * PAGE..total.min((page + 1) * PAGE)];
        // Which of them, and the way to the others, over the list.
        if pages > 1 {
            let mut turn = None;
            ui.horizontal(|ui| {
                let back = ui.add_enabled(page > 0, egui::Button::new("‹"));
                a.previous = Some(back.rect);
                if back.clicked() {
                    turn = Some(page - 1);
                }
                ui.label(t.fill(
                    "flows.appears_page",
                    &[
                        ("from", &(page * PAGE + 1)),
                        ("to", &(page * PAGE + shown.len())),
                        ("n", &total),
                    ],
                ));
                let on = ui.add_enabled(page + 1 < pages, egui::Button::new("›"));
                a.next = Some(on.rect);
                if on.clicked() {
                    turn = Some(page + 1);
                }
            });
            if let Some(p) = turn {
                a.pages.insert(w.clone(), p);
            }
        }
        let mut from = 0;
        while from < shown.len() {
            let lb = shown[from].landblock();
            let to = shown[from..]
                .iter()
                .position(|s| s.landblock() != lb)
                .map_or(shown.len(), |n| from + n);
            let (place, n) = g.places.get(&lb).cloned().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                let words = t.fill(
                    "flows.appears_group",
                    &[
                        ("landblock", &format!("{lb:04X}")),
                        ("place", &place),
                        ("n", &n),
                    ],
                );
                let r = ui.strong(words.split_whitespace().collect::<Vec<_>>().join(" "));
                a.groups.push((lb, n, r.rect));
                for s in &shown[from..to] {
                    let r = ui.small_button(format!("0x{:08X}", s.cell));
                    if ui.is_rect_visible(r.rect) {
                        a.rows
                            .push((lb, s.cell, s.file.clone(), s.guid.clone(), r.rect));
                    }
                    if r.clicked() {
                        out.push(UiAction::GoToSpawn(s.file.clone(), s.guid.clone()));
                    }
                }
            });
            from = to;
        }
    }
}
