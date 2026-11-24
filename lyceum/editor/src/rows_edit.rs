//! Editing a world repository's rows: the layer edits go into (the newest, until New Patch makes
//! another), and a weenie the repository holds shown to the forms as its rows viewed as the
//! server's SQL. Each change a form makes is written back as that layer's row of the weenie: the
//! fields that differ from the world below the layer, the rest of its file and its comments kept,
//! as a step of the history with the world's own. A patch takes a weenie away with a `[[remove]]`
//! row; putting a weenie back as below takes the layer's row of it away.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use lyceum_foundry::content::SqlDoc;
use lyceum_foundry::edit::Op;
use lyceum_foundry::layer_rows;
use lyceum_foundry::repo::{Provenance, Repo, RowSchema, ShownItem, ShownWorld};
use lyceum_foundry::row_sql::RowSql;

use super::editor_tab::Owner;
use super::form::{apply_edit, FormEdit};
use super::EditorApp;

/// The kind of row a weenie is.
const WEENIE: &str = "weenie";

/// The file of a layer's pack that takes its items away.
const REMOVALS: &str = "remove.toml";

/// How many documents made from rows are kept.
const DOCS_HELD: usize = 16;

/// The layer edits go into, and the world below it.
#[derive(Debug)]
pub struct Editing {
    /// The layer: its place among the repository's, and its id.
    pub layer: usize,
    pub layer_id: String,
    /// The pack of the layer the rows are written in: its name and folder.
    pub pack_name: String,
    pub pack_dir: PathBuf,
    /// Whether the layer is a patch: the base takes nothing away.
    pub patch: bool,
    /// The world below the pack's rows.
    pub below: ShownWorld,
    /// The file of the pack that writes each item, by item.
    pub files: BTreeMap<String, PathBuf>,
    /// The provenance of the pack's rows that name none.
    pub prov: Provenance,
    /// Documents made from rows: by item and the text of its file they were made from.
    docs: Vec<(String, Option<String>, Rc<SqlDoc>)>,
    /// The documents of the items below, by item (`None`, an item the layer makes).
    below_docs: Vec<(String, Option<Rc<SqlDoc>>)>,
}

impl Editing {
    /// Edits into `repo`'s layer `layer`, in its last pack.
    ///
    /// # Errors
    /// A layer with no pack to write in, or a world that doesn't fold.
    pub fn new(repo: &Repo, vocabulary: &RowSchema, layer: usize) -> Result<Self, String> {
        let l = repo
            .layers
            .get(layer)
            .ok_or_else(|| format!("no layer {layer}"))?;
        let pack = l.packs.len().checked_sub(1).ok_or_else(|| {
            format!(
                "{}: the layer has no pack to write in; New Patch makes a patch with one",
                l.id
            )
        })?;
        let below = repo.fold_below(vocabulary, layer, pack)?;
        let p = &l.packs[pack];
        let mut files = BTreeMap::new();
        for (rel, text) in &p.files {
            let Ok(t) = toml::from_str::<toml::Table>(text) else {
                continue;
            };
            for (kind, rows) in &t {
                for r in rows.as_array().into_iter().flatten() {
                    if let Some(item) = r
                        .as_table()
                        .and_then(|r| lyceum_foundry::repo::item_of(vocabulary, kind, r))
                    {
                        files.insert(item, p.dir.join(rel));
                    }
                }
            }
        }
        Ok(Self {
            layer,
            layer_id: l.id.clone(),
            pack_name: p.name.clone(),
            pack_dir: p.dir.clone(),
            patch: layer > 0,
            below,
            files,
            prov: p.prov.clone(),
            docs: Vec::new(),
            below_docs: Vec::new(),
        })
    }
}

/// The layer's file `text` (`None`, no file yet) with item `item`'s row made what the world needs
/// for the item to be `doc` (as a form left it, viewed by `view`) over `below`: only the fields
/// that differ, or no row when none do. `None` when the file is left with nothing in it.
///
/// # Errors
/// A document whose rows don't read back, or a file that isn't TOML.
pub fn layer_text(
    view: &RowSql<'_>,
    vocabulary: &RowSchema,
    below: Option<&toml::Table>,
    text: Option<&str>,
    item: &str,
    doc: &SqlDoc,
) -> Result<Option<String>, String> {
    let wanted = view.to_row(doc)?;
    let row = layer_rows::delta(vocabulary, WEENIE, below, &wanted);
    layer_rows::write_row(vocabulary, WEENIE, text, item, row.as_ref())
}

/// A title as a patch's name: lowercase letters, digits and hyphens.
#[must_use]
pub fn patch_name(title: &str) -> String {
    let mut out = String::new();
    for c in title.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

impl EditorApp {
    /// Edit the rows of the repository the Rows tab has open, in its newest layer that can be
    /// shown; or none, when it has none open.
    pub(super) fn rows_start_editing(&mut self) {
        let edit = match (&self.rows.repo, &self.rows.vocabulary) {
            (Some(repo), Some(v)) => Some(Editing::new(repo, v, repo.shown_layers() - 1)),
            _ => None,
        };
        let (editing, why) = match edit {
            Some(Ok(e)) => (Some(e), None),
            Some(Err(why)) => (None, Some(why)),
            None => (None, None),
        };
        let dirs = editing.as_ref().zip(self.rows.repo.as_ref()).map(|(e, r)| {
            let root = r
                .path
                .parent()
                .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
            (root, e.pack_dir.clone())
        });
        self.session.set_rows(dirs);
        self.rows.editing = editing;
        if why.is_some() {
            self.rows.edit_error = why;
        }
    }

    /// The Rows tab opened on the repository whose world file is `path`, editing its newest
    /// layer.
    pub(super) fn rows_open(&mut self, path: &std::path::Path) {
        let schema = self.schema.clone();
        self.rows.open(path, schema.as_ref());
        self.rows.edit_error = None;
        self.rows_start_editing();
    }

    /// The view of weenie rows as SQL, with the server's names and the repository's stamp.
    fn weenie_view(&self) -> Option<RowSql<'_>> {
        let stamp = self.rows.repo.as_ref().and_then(|r| r.stamp.as_deref());
        RowSql::new(self.schema.as_ref()?, WEENIE, stamp).ok()
    }

    /// Class `class`'s item, as the rows name it.
    fn weenie_item(&self, class: u32) -> Option<String> {
        let v = self.rows.vocabulary.as_ref()?;
        let mut row = toml::Table::new();
        row.insert("class".into(), toml::Value::Integer(i64::from(class)));
        lyceum_foundry::repo::item_of(v, WEENIE, &row)
    }

    /// Weenie class `class` when the repository being edited holds it (below the layer, or in its
    /// pack): its item, and the file its row is in (or goes in).
    pub(super) fn rows_weenie(&self, class: u32) -> Option<(String, PathBuf)> {
        let e = self.rows.editing.as_ref()?;
        let item = self.weenie_item(class)?;
        if let Some(f) = e.files.get(&item) {
            return Some((item, f.clone()));
        }
        e.below.items.get(&item)?;
        let mut row = toml::Table::new();
        row.insert("class".into(), toml::Value::Integer(i64::from(class)));
        let file = layer_rows::file_of(self.rows.vocabulary.as_ref()?, WEENIE, &row).ok()?;
        Some((item, e.pack_dir.join(file)))
    }

    /// Item `item` as the world below the layer holds it, as one row.
    fn below_row(&self, item: &str) -> Option<toml::Table> {
        let v = self.rows.vocabulary.as_ref()?;
        self.rows
            .editing
            .as_ref()?
            .below
            .items
            .get(item)
            .map(|i| i.row(v))
    }

    /// Item `item`'s row in the layer's file `text`.
    fn layer_row(&self, text: Option<&str>, item: &str) -> Option<toml::Table> {
        let v = self.rows.vocabulary.as_ref()?;
        layer_rows::row_in(v, WEENIE, text?, item).ok().flatten()
    }

    /// Weenie class `class` as the layer being edited holds it, viewed as SQL: `None` for one the
    /// repository doesn't hold, or whose rows don't view.
    pub(super) fn rows_doc(&mut self, class: u32) -> Option<Rc<SqlDoc>> {
        let (item, file) = self.rows_weenie(class)?;
        let text = self.session.rows_text(&file);
        let e = self.rows.editing.as_ref()?;
        if let Some((_, _, d)) = e.docs.iter().find(|(i, t, _)| *i == item && *t == text) {
            return Some(Rc::clone(d));
        }
        let below = self.below_row(&item);
        let row = match self.layer_row(text.as_deref(), &item) {
            Some(r) => layer_rows::over(self.rows.vocabulary.as_ref()?, WEENIE, below.as_ref(), &r),
            None => below?,
        };
        let doc = match self.weenie_view()?.to_sql(&row) {
            Ok(d) => Rc::new(d),
            Err(why) => {
                self.panels.message = format!("{item}: {why}");
                return None;
            }
        };
        let e = self.rows.editing.as_mut()?;
        e.docs.retain(|(i, _, _)| *i != item);
        e.docs.insert(0, (item, text, Rc::clone(&doc)));
        e.docs.truncate(DOCS_HELD);
        Some(doc)
    }

    /// Weenie class `class` as the world below the layer holds it, viewed as SQL (`Some(None)`, one
    /// the layer makes); `None` for one the repository doesn't hold.
    pub(super) fn rows_below_doc(&mut self, class: u32) -> Option<Option<Rc<SqlDoc>>> {
        let (item, _) = self.rows_weenie(class)?;
        let e = self.rows.editing.as_ref()?;
        if let Some((_, d)) = e.below_docs.iter().find(|(i, _)| *i == item) {
            return Some(d.clone());
        }
        let doc = match self.below_row(&item) {
            Some(row) => Some(Rc::new(self.weenie_view()?.to_sql(&row).ok()?)),
            None => None,
        };
        let e = self.rows.editing.as_mut()?;
        e.below_docs.insert(0, (item, doc.clone()));
        e.below_docs.truncate(DOCS_HELD);
        Some(doc)
    }

    /// Whose weenie class `class` is, when the repository being edited holds it: the layer's,
    /// set there or not.
    pub(super) fn rows_owner(&mut self, class: u32) -> Option<Owner> {
        let (item, file) = self.rows_weenie(class)?;
        let text = self.session.rows_text(&file);
        let set = self.layer_row(text.as_deref(), &item).is_some();
        let e = self.rows.editing.as_ref()?;
        Some(Owner::Layer {
            layer: format!("{} ({})", e.layer_id, e.pack_name),
            set,
            patch: e.patch,
        })
    }

    /// `edits` of weenie class `class`, which the repository being edited holds, made together and
    /// written as the layer's row of it.
    ///
    /// # Errors
    /// An edit refused, or a value not of its field.
    pub(super) fn rows_edits(&mut self, class: u32, edits: &[FormEdit]) -> Result<(), String> {
        if let Some(FormEdit::Refused(why)) =
            edits.iter().find(|e| matches!(e, FormEdit::Refused(_)))
        {
            return Err(why.clone());
        }
        let (item, file) = self
            .rows_weenie(class)
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let doc = self
            .rows_doc(class)
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let mut doc = (*doc).clone();
        for edit in edits {
            if !apply_edit(&mut doc, edit, self.schema.as_ref()) {
                return Err(self.say("messages.not_a_value"));
            }
        }
        let below = self.below_row(&item);
        let text = self.session.rows_text(&file);
        let new = {
            let view = self
                .weenie_view()
                .ok_or_else(|| self.say("messages.not_in_repository"))?;
            let v = self
                .rows
                .vocabulary
                .as_ref()
                .ok_or_else(|| self.say("messages.not_in_repository"))?;
            layer_text(&view, v, below.as_ref(), text.as_deref(), &item, &doc)?
        };
        self.write_file(&item, &file, text, new)
    }

    /// The layer's `file` (holding `item`'s row) written as `new`, as one step, when it changes
    /// from `text`.
    fn write_file(
        &mut self,
        item: &str,
        file: &std::path::Path,
        text: Option<String>,
        new: Option<String>,
    ) -> Result<(), String> {
        if new != text {
            self.apply(
                &Op::WriteRows {
                    file: file.to_path_buf(),
                    text: new,
                },
                false,
            )?;
        }
        if let Some(e) = self.rows.editing.as_mut() {
            e.files.insert(item.to_owned(), file.to_path_buf());
        }
        Ok(())
    }

    /// Weenie class `class` put back as the world below the layer holds it: the layer's row of it
    /// taken away.
    ///
    /// # Errors
    /// One the layer doesn't change.
    pub(super) fn rows_put_back(&mut self, class: u32) -> Result<(), String> {
        let (item, file) = self
            .rows_weenie(class)
            .ok_or_else(|| self.say("messages.not_changed_here"))?;
        let text = self.session.rows_text(&file);
        let new = {
            let v = self
                .rows
                .vocabulary
                .as_ref()
                .ok_or_else(|| self.say("messages.not_changed_here"))?;
            layer_rows::write_row(v, WEENIE, text.as_deref(), &item, None)?
        };
        self.write_file(&item, &file, text, new)
    }

    /// Weenie class `class` taken away in the patch being edited (`gone`), or no longer.
    ///
    /// # Errors
    /// The base, which takes nothing away.
    pub(super) fn rows_remove(&mut self, class: u32, gone: bool) -> Result<(), String> {
        let (item, _) = self
            .rows_weenie(class)
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let e = self
            .rows
            .editing
            .as_ref()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        if !e.patch {
            return Err(self.say("messages.base_removes_nothing"));
        }
        let file = e.pack_dir.join(REMOVALS);
        let text = self.session.rows_text(&file);
        let new = layer_rows::write_removal(text.as_deref(), &item, gone)?;
        if new != text {
            self.apply(&Op::WriteRows { file, text: new }, false)?;
        }
        Ok(())
    }

    /// Whether the patch being edited takes item `item` away.
    pub(super) fn rows_removed(&mut self, item: &str) -> bool {
        let Some(e) = self.rows.editing.as_ref() else {
            return false;
        };
        let file = e.pack_dir.join(REMOVALS);
        self.session
            .rows_text(&file)
            .and_then(|t| {
                layer_rows::write_removal(Some(&t), item, true)
                    .ok()
                    .map(|n| (t, n))
            })
            .is_some_and(|(t, n)| n.as_deref() == Some(t.as_str()))
    }

    /// Item `name` as the layer being edited holds it now, its unsaved changes in: each field with
    /// the layer that set it.
    pub(super) fn rows_live_item(&mut self, name: &str) -> Option<ShownItem> {
        let e = self.rows.editing.as_ref()?;
        let below = e.below.items.get(name).cloned();
        let file = e.files.get(name).cloned();
        let (layer, prov) = (e.layer_id.clone(), e.prov.clone());
        let kind = below
            .as_ref()
            .map(|b| b.kind.clone())
            .unwrap_or_else(|| WEENIE.to_owned());
        let row = file.as_ref().and_then(|f| {
            let text = self.session.rows_text(f)?;
            let v = self.rows.vocabulary.as_ref()?;
            layer_rows::row_in(v, &kind, &text, name).ok().flatten()
        });
        let Some(row) = row else {
            return below;
        };
        let at = file.map(|f| lyceum_foundry::Loc::new(&f, 0));
        Some(ShownItem::over(
            below.as_ref(),
            self.rows.vocabulary.as_ref()?,
            &kind,
            &row,
            &layer,
            at,
            &prov,
        ))
    }

    /// A weenie menu's entries for weenie class `class` of the repository being edited: put back
    /// as below, when its layer changes it (`set`); and in a patch, taken away or kept.
    pub(super) fn layer_menu(
        &mut self,
        ui: &mut egui::Ui,
        class: u32,
        set: bool,
        patch: bool,
        out: &mut Vec<super::UiAction>,
    ) {
        let t = self.text.table();
        if set
            && ui
                .button(t.get("definition.put_back_below"))
                .on_hover_text(t.get("definition.put_back_below_about"))
                .clicked()
        {
            out.push(super::UiAction::PutBackServer(class));
        }
        if !patch {
            return;
        }
        let removed = self
            .rows_weenie(class)
            .is_some_and(|(item, _)| self.rows_removed(&item));
        let (key, about) = if removed {
            ("definition.keep_in_patch", "definition.keep_in_patch_about")
        } else {
            (
                "definition.remove_in_patch",
                "definition.remove_in_patch_about",
            )
        };
        if ui.button(t.get(key)).on_hover_text(t.get(about)).clicked() {
            out.push(super::UiAction::RemoveInPatch(class, !removed));
        }
    }

    /// A patch made after the repository's last layer, of month `month` (`yyyy-mm`) and title
    /// `title`, and edits going into it.
    ///
    /// # Errors
    /// Unsaved changes of the repository's rows (the world below the patch is read from the
    /// disk), or a patch that can't be made.
    pub(super) fn rows_new_patch(&mut self, month: &str, title: &str) -> Result<(), String> {
        if self.session.rows_unsaved() {
            return Err(self.say("messages.save_rows_first"));
        }
        let repo = self
            .rows
            .repo
            .as_ref()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let id = format!("{}-{}", month.trim(), patch_name(title));
        let date = format!("{}-01", month.trim());
        let files = repo.new_patch(&id, title.trim(), &date)?;
        for (path, text) in &files {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let path = repo.path.clone();
        self.rows_open(&path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: editing a world repository's rows)
    use super::*;

    use lyceum_foundry::content::{PropertyKind, Schema};

    /// The server's schema and weenie 100's rows, as the Foundry's view of rows is checked
    /// against them.
    fn pair() -> (Schema, RowSchema, toml::Table) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/foundry/tests/cpu/world/row_sql_pair");
        let text = std::fs::read_to_string(dir.join("schema.json")).unwrap();
        let schema = Schema::read(&text).unwrap();
        let vocabulary = RowSchema::read(&text).unwrap().unwrap();
        let rows: toml::Table =
            toml::from_str(&std::fs::read_to_string(dir.join("weenie-100.toml")).unwrap()).unwrap();
        let row = rows["weenie"][0].as_table().unwrap().clone();
        (schema, vocabulary, row)
    }

    /// A form's edits of a repository's weenie are written as the layer's row of it: its class,
    /// the one property set, and the lists changed (whole), nothing else; that row over the world
    /// below is the weenie as the form left it. Put back as it was, the layer writes nothing.
    #[test]
    fn a_forms_edits_are_written_as_the_layers_row_of_only_what_they_change() {
        let (schema, vocabulary, below) = pair();
        let view = RowSql::new(&schema, WEENIE, None).unwrap();
        let mut doc = view.to_sql(&below).unwrap();
        for e in [
            FormEdit::Property {
                kind: PropertyKind::Int,
                id: 25,
                value: Some("6".into()),
            },
            FormEdit::AddAction {
                set: 1,
                kind: "Tell".into(),
            },
            FormEdit::RemoveRow {
                table: "weenie_properties_texture_map".into(),
                row: 0,
            },
        ] {
            assert!(apply_edit(&mut doc, &e, Some(&schema)), "{e:?}");
        }
        let text = layer_text(&view, &vocabulary, Some(&below), None, "weenie 100", &doc)
            .unwrap()
            .expect("a row");
        let row = layer_rows::row_in(&vocabulary, WEENIE, &text, "weenie 100")
            .unwrap()
            .expect("the weenie's row");
        let mut set: Vec<&str> = row.keys().map(String::as_str).collect();
        set.sort_unstable();
        assert_eq!(set, ["class", "emote", "int", "texture_map"], "{text}");
        assert_eq!(
            row["int"].as_table().unwrap().keys().collect::<Vec<_>>(),
            ["Level"],
            "{text}"
        );
        assert_eq!(
            layer_rows::over(&vocabulary, WEENIE, Some(&below), &row),
            view.to_row(&doc).unwrap()
        );
        // Put back: nothing to write.
        let back = view.to_sql(&below).unwrap();
        assert_eq!(
            layer_text(
                &view,
                &vocabulary,
                Some(&below),
                Some(&text),
                "weenie 100",
                &back
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn a_patchs_name_is_its_title_in_lowercase_words_joined_by_hyphens() {
        assert_eq!(patch_name("Frozen Valley"), "frozen-valley");
        assert_eq!(patch_name("  A Change of Heart!  "), "a-change-of-heart");
        assert_eq!(patch_name("Patch #3: Fixes"), "patch-3-fixes");
    }
}
