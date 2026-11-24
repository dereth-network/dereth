//! The Rows tab: a world repository's rows (Empyrean's worlds of layers), folded at a layer, read
//! only. Each item of the world, found by its name; each of its fields with the layer that set it
//! (and, held over it, the file and line and the row's provenance) and its value.
//!
//! The vocabulary of rows is the server's: the `rows` member of what `empyrean-import schema`
//! writes, which the editor reads with the forms' names. Without it the tab says so.

use std::path::Path;

use lyceum_foundry::repo::{Repo, RowSchema, ShownWorld};

use super::{EditorApp, Tab, UiAction};

/// How many matching items the list shows at once; the filter narrows the rest.
const SHOWN: usize = 400;

/// The Rows tab's state: the repository, the layer, the filter and the chosen item.
#[derive(Debug, Default)]
pub struct RowsView {
    /// The repository's `world.toml`, as typed.
    pub path: String,
    pub repo: Option<Repo>,
    pub vocabulary: Option<RowSchema>,
    /// The layer the world is shown at.
    pub at: usize,
    pub world: Option<ShownWorld>,
    /// Text the shown items' names hold, case aside.
    pub filter: String,
    pub selected: Option<String>,
    /// Why the repository or its fold didn't read.
    pub error: Option<String>,
}

impl RowsView {
    /// Open the repository at `path`, with the vocabulary in `schema`, and show its world at the
    /// last layer that can be shown.
    pub fn open(&mut self, path: &Path, schema: Option<&lyceum_foundry::content::Schema>) {
        self.path = path.display().to_string();
        self.repo = None;
        self.world = None;
        self.selected = None;
        self.error = None;
        self.vocabulary = schema
            .and_then(|s| s.rows.clone())
            .map(|rows| RowSchema::of_schema(&serde_json::json!({ "rows": rows })))
            .transpose()
            .unwrap_or_else(|e| {
                self.error = Some(e);
                None
            })
            .flatten();
        if self.vocabulary.is_none() && self.error.is_none() {
            self.error = Some(
                "The server's tools gave no vocabulary of rows: an older empyrean-import, or none beside the editor.".into(),
            );
            return;
        }
        match Repo::open(path) {
            Ok(r) => {
                let last = r.shown_layers() - 1;
                self.repo = Some(r);
                self.fold_at(last);
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Show the world at layer `at`.
    pub fn fold_at(&mut self, at: usize) {
        let (Some(repo), Some(vocabulary)) = (&self.repo, &self.vocabulary) else {
            return;
        };
        self.at = at;
        match repo.fold(vocabulary, at) {
            Ok(w) => {
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|s| !w.items.contains_key(s))
                {
                    self.selected = None;
                }
                self.world = Some(w);
                self.error = None;
            }
            Err(e) => {
                self.world = None;
                self.error = Some(e);
            }
        }
    }

    /// The items whose names hold the filter, case aside, in name order; at most `cap`, and how
    /// many matched in all.
    #[must_use]
    pub fn matching(&self, cap: usize) -> (Vec<&str>, usize) {
        let Some(w) = &self.world else {
            return (Vec::new(), 0);
        };
        let want = self.filter.trim().to_lowercase();
        let all: Vec<&str> = w
            .items
            .keys()
            .map(String::as_str)
            .filter(|n| want.is_empty() || n.to_lowercase().contains(&want))
            .collect();
        let n = all.len();
        (all.into_iter().take(cap).collect(), n)
    }
}

impl EditorApp {
    /// Show the Rows tab: where it is, else floating over the world view's left.
    pub fn open_rows(&mut self) {
        self.bring_back(Tab::Rows);
        let world = self.world_rect.unwrap_or(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1400.0, 900.0),
        ));
        let size = egui::vec2(
            720.0_f32.min(world.width() - 32.0).max(320.0),
            640.0_f32.min(world.height() - 32.0).max(240.0),
        );
        let at =
            egui::Rect::from_min_size(egui::pos2(world.min.x + 16.0, world.min.y + 16.0), size);
        let dock = if self.dungeon.is_some() {
            self.dungeon_dock
                .get_or_insert_with(crate::docks::in_dungeon)
        } else {
            self.dock.get_or_insert_with(crate::docks::outdoors)
        };
        crate::docks::open_floating(dock, Tab::Rows, at);
        self.show_tab(Tab::Rows);
    }

    /// The Rows tab's body.
    pub(super) fn rows_tab(&mut self, ui: &mut egui::Ui, _out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let mut open = false;
        ui.horizontal(|ui| {
            ui.label(t.get("rows.world"));
            ui.add(egui::TextEdit::singleline(&mut self.rows.path).desired_width(360.0));
            open = ui.button(t.get("rows.open")).clicked();
        });
        if open {
            let path = std::path::PathBuf::from(self.rows.path.trim());
            let schema = self.schema.clone();
            self.rows.open(&path, schema.as_ref());
        }
        if let Some(e) = &self.rows.error {
            ui.colored_label(ui.visuals().warn_fg_color, e);
        }
        let Some(repo) = &self.rows.repo else {
            return;
        };
        let ids: Vec<String> = repo.ids().iter().map(|s| (*s).to_owned()).collect();
        let shown = repo.shown_layers();
        let title = repo.title.clone().unwrap_or_default();
        let mut at = self.rows.at;
        ui.horizontal(|ui| {
            ui.label(title);
            egui::ComboBox::from_id_salt("rows-layer")
                .selected_text(ids.get(at).cloned().unwrap_or_default())
                .show_ui(ui, |ui| {
                    for (n, id) in ids.iter().enumerate() {
                        let r = ui.add_enabled(n < shown, egui::Button::selectable(at == n, id));
                        if r.clicked() {
                            at = n;
                        }
                        if n >= shown {
                            r.on_disabled_hover_text(t.get("rows.after_checkpoint"));
                        }
                    }
                });
            ui.label(t.get("rows.find"));
            ui.add(egui::TextEdit::singleline(&mut self.rows.filter).desired_width(180.0));
        });
        if at != self.rows.at {
            self.rows.fold_at(at);
        }
        let (names, n) = self.rows.matching(SHOWN);
        let names: Vec<String> = names.into_iter().map(str::to_owned).collect();
        let findings = self.rows.world.as_ref().map_or(0, |w| w.findings.len());
        ui.label(t.fill(
            "rows.count",
            &[("n", &n), ("shown", &names.len()), ("findings", &findings)],
        ));
        ui.separator();
        let mut chosen = None;
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("rows-items")
                .auto_shrink([false, false])
                .show(&mut cols[0], |ui| {
                    for name in &names {
                        let on = self.rows.selected.as_deref() == Some(name.as_str());
                        if ui.selectable_label(on, name).clicked() {
                            chosen = Some(name.clone());
                        }
                    }
                });
            let item = self
                .rows
                .selected
                .as_ref()
                .and_then(|s| self.rows.world.as_ref()?.items.get(s));
            egui::ScrollArea::vertical()
                .id_salt("rows-fields")
                .auto_shrink([false, false])
                .show(&mut cols[1], |ui| {
                    let Some(item) = item else {
                        ui.label(t.get("rows.choose"));
                        return;
                    };
                    egui::Grid::new("rows-grid").striped(true).show(ui, |ui| {
                        for f in &item.fields {
                            let at = f.at.as_ref().map_or_else(String::new, |a| a.to_string());
                            ui.label(&f.name);
                            ui.label(&f.layer)
                                .on_hover_text(format!("{at}\n{}", f.prov));
                            ui.label(f.value.to_string());
                            ui.end_row();
                        }
                    });
                });
        });
        if let Some(c) = chosen {
            self.rows.selected = Some(c);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the editor's Rows tab over a world repository)
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A schema as the server's tools write it, with its `rows`.
    fn schema() -> lyceum_foundry::content::Schema {
        lyceum_foundry::content::Schema::read(
            r#"{ "properties": {}, "enums": {}, "rows": {
                "quest": { "file": "quest/{name}.toml", "item": "quest {name:lower}",
                           "key": ["name"], "fields": { "name": "text" } } } }"#,
        )
        .unwrap()
    }

    /// The tab opens a repository at its last layer, its items found by any part of their names,
    /// case aside; without the server's vocabulary it says why and shows nothing.
    #[test]
    fn the_rows_tab_shows_a_repositorys_items_found_by_name() {
        let dir = std::env::temp_dir().join(format!("lyceum-rows-tab-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(
            &dir.join("base/main/pack.toml"),
            "name = \"main\"\nversion = \"1\"\n",
        );
        for q in ["DrudgeKills", "MiteHunt", "WaspHunt"] {
            write(
                &dir.join(format!("base/main/quest/{q}.toml")),
                &format!("[[quest]]\nname = \"{q}\"\n"),
            );
        }
        let world = dir.join("world.toml");
        write(
            &world,
            "format_version = 1\ngame = \"ac1\"\npacks = [\"base/main\"]\nlineage = []\n",
        );
        let mut v = RowsView::default();
        v.open(&world, Some(&schema()));
        assert_eq!(v.error, None);
        assert_eq!(
            v.matching(10),
            (
                vec!["quest drudgekills", "quest mitehunt", "quest wasphunt"],
                3
            )
        );
        v.filter = "HUNT".into();
        assert_eq!(v.matching(1), (vec!["quest mitehunt"], 2));
        let mut none = RowsView::default();
        none.open(&world, Some(&lyceum_foundry::content::Schema::default()));
        assert!(none.repo.is_none() && none.error.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
