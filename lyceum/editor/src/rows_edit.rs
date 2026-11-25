//! Editing a world repository's rows: the layer edits go into (the newest, until New Patch makes
//! another), opened in the session as the pack's SQL ([`lyceum_foundry::mirror`]). So every form,
//! the Quests tab and the Loot tab edit the repository as they edit a pack: the world below the
//! layer is the server's, a change of a server's weenie, quest or table is the layer's change of
//! it, put back as the server has it is the layer's row of it taken away, and a pack's own is one
//! the layer makes. Each change is written as the layer's rows in the same step of the history. A
//! patch takes a weenie, a quest or a loot table away with a `[[remove]]` row, all of them in its
//! pack's `remove.toml`.

use std::path::PathBuf;

use lyceum_foundry::edit::Op;
use lyceum_foundry::layer_rows;
use lyceum_foundry::repo::{ShownItem, ShownWorld};

use super::editor_tab::Owner;
use super::EditorApp;

use lyceum_foundry::mirror::REMOVALS;

/// The layer edits go into, as the Rows tab shows it.
#[derive(Debug)]
pub struct Editing {
    /// Its place among the repository's layers.
    pub layer: usize,
    /// The world below the layer's pack, for the fields each item has from below.
    pub below: ShownWorld,
}

/// What a patch can take away: a weenie, a quest, a loot table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Removable {
    Weenie(u32),
    Quest(String),
    Loot(lyceum_foundry::loot::LootKind, u32),
}

impl Removable {
    /// Its item, as the rows of `vocabulary` name it.
    #[must_use]
    pub fn item(&self, vocabulary: &lyceum_foundry::repo::RowSchema) -> Option<String> {
        use lyceum_foundry::loot::LootKind;
        let (kind, field, value) = match self {
            Self::Weenie(c) => ("weenie", "class", toml::Value::Integer(i64::from(*c))),
            Self::Quest(name) => ("quest", "name", toml::Value::String(name.clone())),
            Self::Loot(LootKind::Death, id) => (
                "treasure_death",
                "type",
                toml::Value::Integer(i64::from(*id)),
            ),
            Self::Loot(LootKind::Wielded, id) => (
                "treasure_wielded",
                "type",
                toml::Value::Integer(i64::from(*id)),
            ),
        };
        let mut row = toml::Table::new();
        row.insert(field.into(), value);
        lyceum_foundry::repo::item_of(vocabulary, kind, &row)
    }
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
    /// Edit the repository the Rows tab has open, in its newest layer that can be shown; or none,
    /// when it has none open.
    pub(super) fn rows_start_editing(&mut self) {
        self.rows.editing = None;
        let (Some(repo), Some(v)) = (&self.rows.repo, &self.rows.vocabulary) else {
            if let Err(e) = self.session.close_repository() {
                self.rows.edit_error = Some(e.to_string());
            }
            return;
        };
        let layer = repo.shown_layers() - 1;
        let at = repo.layers[layer].packs.len().checked_sub(1);
        let opened = at
            .ok_or_else(|| {
                format!(
                    "{}: the layer has no pack to write in; New Patch makes a patch with one",
                    repo.layers[layer].id
                )
            })
            .and_then(|at| repo.fold_below(v, layer, at))
            .and_then(|below| {
                self.session
                    .open_repository(repo, layer)
                    .map_err(|e| e.to_string())?;
                Ok(Editing { layer, below })
            });
        match opened {
            Ok(e) => self.rows.editing = Some(e),
            Err(why) => self.rows.edit_error = Some(why),
        }
        self.after_repository();
    }

    /// What the editor knows of the server's content, let go of: it is the repository's now.
    fn after_repository(&mut self) {
        self.editor.docs.clear();
        self.rebuild_weenies();
    }

    /// The Rows tab opened on the repository whose world file is `path`, editing its newest
    /// layer.
    pub(super) fn rows_open(&mut self, path: &std::path::Path) {
        let schema = self.schema.clone();
        self.rows.open(path, schema.as_ref());
        self.rows.edit_error = None;
        self.rows_start_editing();
    }

    /// Class `class`'s item, as the rows name it.
    fn weenie_item(&self, class: u32) -> Option<String> {
        let m = self.session.mirror()?;
        let mut row = toml::Table::new();
        row.insert("class".into(), toml::Value::Integer(i64::from(class)));
        lyceum_foundry::repo::item_of(m.vocabulary(), "weenie", &row)
    }

    /// Server class `class`'s rows, as the world below the layer being edited has them: `None`
    /// when no repository is being edited, `Some(None)` for a class the world below lacks.
    pub(super) fn rows_server_class(&self, class: u32) -> Option<Option<String>> {
        let m = self.session.mirror()?;
        Some(self.weenie_item(class).and_then(|i| m.below_text(&i)))
    }

    /// Whose weenie class `class` is, when a repository's layer is being edited: the layer's,
    /// changed there or as below, or one it makes; `None` for no repository, or a class it lacks.
    pub(super) fn rows_owner(&mut self, class: u32) -> Option<Owner> {
        let item = self.weenie_item(class)?;
        let mirrored = self
            .session
            .weenie_file(&lyceum_foundry::edit::Weenie::Class(class))
            .is_some_and(|(f, _)| self.session.mirror().is_some_and(|m| m.holds(&f)));
        let m = self.session.mirror()?;
        let below = m.below_has(&item);
        if !mirrored && !below {
            return None;
        }
        Some(Owner::Layer {
            layer: format!("{} ({})", m.layer, m.pack_name),
            set: mirrored,
            own: !below,
            patch: m.patch,
        })
    }

    /// Whether the patch being edited takes `what` away (`Some(true)`) or not; `None` when no
    /// patch is being edited, or the world below it lacks `what`, so there is nothing to take away.
    pub(super) fn patch_removal(&mut self, what: &Removable) -> Option<bool> {
        let m = self.session.mirror()?;
        let item = what.item(m.vocabulary())?;
        if !m.patch || !m.below_has(&item) {
            return None;
        }
        Some(self.rows_removed(&item))
    }

    /// `what` taken away in the patch being edited (`gone`), or no longer.
    ///
    /// # Errors
    /// The base, which takes nothing away.
    pub(super) fn rows_remove(&mut self, what: &Removable, gone: bool) -> Result<(), String> {
        let item = self
            .session
            .mirror()
            .and_then(|m| what.item(m.vocabulary()))
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let m = self
            .session
            .mirror()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        if !m.patch {
            return Err(self.say("messages.base_removes_nothing"));
        }
        let file = m.pack.join(REMOVALS);
        let text = self.session.rows_text(&file);
        let new = layer_rows::write_removal(text.as_deref(), &item, gone)?;
        if new != text {
            self.apply(&Op::WriteRows { file, text: new }, false)?;
        }
        Ok(())
    }

    /// Whether the patch being edited takes item `item` away.
    pub(super) fn rows_removed(&mut self, item: &str) -> bool {
        let Some(m) = self.session.mirror() else {
            return false;
        };
        let file = m.pack.join(REMOVALS);
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
        let below = self.rows.editing.as_ref()?.below.items.get(name).cloned();
        let m = self.session.mirror()?;
        let (layer, file) = (m.layer.clone(), m.row_file(name).map(PathBuf::from));
        let kind = below
            .as_ref()
            .map_or_else(|| "weenie".to_owned(), |b| b.kind.clone());
        let row = file.as_ref().and_then(|f| {
            let text = self.session.rows_text(f)?;
            let v = self.rows.vocabulary.as_ref()?;
            layer_rows::row_in(v, &kind, &text, name).ok().flatten()
        });
        let Some(row) = row else {
            return below;
        };
        let prov = self
            .rows
            .repo
            .as_ref()?
            .layers
            .iter()
            .find(|l| l.id == layer)
            .and_then(|l| l.packs.last())
            .map(|p| p.prov.clone())
            .unwrap_or_default();
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
        let Some(removed) = self.patch_removal(&Removable::Weenie(class)) else {
            return;
        };
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
        if self.repo_world {
            // The world's packs are those it was opened with: the patch's joins them, and its
            // edits begin, once the world is opened again.
            self.panels.message = self.say_with("messages.patch_made_reopen", &[("id", &id)]);
            return Ok(());
        }
        self.rows_open(&path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: editing a world repository's rows)
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    use lyceum_foundry::content::{PropertyKind, Schema, SqlDoc};
    use lyceum_foundry::mirror::{Files, Mirror};
    use lyceum_foundry::repo::Repo;

    use crate::app::form::{apply_edit, FormEdit};

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A repository whose base holds the server's fixture weenie 100, with an empty patch.
    fn repo(dir: &Path) -> (Schema, Repo) {
        let pair = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/foundry/tests/cpu/world/row_sql_pair");
        let schema =
            Schema::read(&std::fs::read_to_string(pair.join("schema.json")).unwrap()).unwrap();
        write(
            &dir.join("base/library/weenie/00100.toml"),
            &std::fs::read_to_string(pair.join("weenie-100.toml")).unwrap(),
        );
        write(
            &dir.join("base/library/pack.toml"),
            "name = \"library\"\nversion = \"1\"\n",
        );
        write(
            &dir.join("patches/2027-01-first/patch.toml"),
            "id = \"2027-01-first\"\ntitle = \"First\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"main\"]\n",
        );
        write(
            &dir.join("patches/2027-01-first/packs/main/pack.toml"),
            "name = \"main\"\nversion = \"1\"\n",
        );
        let world = dir.join("world.toml");
        write(
            &world,
            "format_version = 1\ngame = \"ac1\"\npacks = [\"base/library\"]\nlineage = [\"2027-01-first\"]\n",
        );
        (schema, Repo::open(&world).unwrap())
    }

    /// The disk as it is, under the texts a session holds changed.
    struct Disk(BTreeMap<std::path::PathBuf, Option<String>>);
    impl Files for Disk {
        fn now(&mut self, path: &Path) -> Option<String> {
            self.0
                .get(path)
                .cloned()
                .unwrap_or_else(|| std::fs::read_to_string(path).ok())
        }
        fn saved(&mut self, _: &Path) -> Option<Option<String>> {
            None
        }
    }

    /// A form's edits of a repository's weenie, made to the pack's copy of it, are written as the
    /// patch's row of it: its class, the one property set, and the lists changed (whole), nothing
    /// else. Put back as it was, the patch writes nothing.
    #[test]
    fn a_forms_edits_are_written_as_the_layers_row_of_only_what_they_change() {
        let dir = std::env::temp_dir().join(format!("lyceum-rows-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (schema, repo) = repo(&dir);
        let mut mirror = Mirror::new(&repo, &schema, 1, &dir.join("world/sql")).unwrap();
        let mut doc = SqlDoc::parse(&mirror.below_text("weenie 100").unwrap());
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
        let file = mirror.file_of("weenie", "weenie 100");
        let changed = BTreeMap::from([(file.clone(), Some(doc.render()))]);
        let rows = mirror
            .rows_for(&changed, &mut Disk(BTreeMap::new()))
            .unwrap();
        let (path, text) = rows.iter().next().expect("the patch's row file");
        assert!(path.ends_with("patches/2027-01-first/packs/main/weenie/00100.toml"));
        let text = text.clone().unwrap();
        let row = layer_rows::row_in(mirror.vocabulary(), "weenie", &text, "weenie 100")
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

        // Put back as it was: the file it wrote, holding nothing.
        let path = path.clone();
        let held = BTreeMap::from([
            (file.clone(), Some(doc.render())),
            (path.clone(), Some(text)),
        ]);
        let back = BTreeMap::from([(file, mirror.below_text("weenie 100"))]);
        let rows = mirror.rows_for(&back, &mut Disk(held)).unwrap();
        assert_eq!(rows.get(&path), Some(&None), "{rows:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a patch takes away is named as the rows name it: a weenie by its class, a quest by
    /// its name in lower case, a loot table by its kind and id.
    #[test]
    fn what_a_patch_takes_away_is_named_as_the_rows_name_it() {
        let dir = std::env::temp_dir().join(format!("lyceum-removable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (schema, _) = repo(&dir);
        let v = lyceum_foundry::repo::RowSchema::of_schema(&serde_json::json!({
            "rows": schema.rows.clone().unwrap()
        }))
        .unwrap()
        .unwrap();
        use lyceum_foundry::loot::LootKind;
        let named = |r: Removable| r.item(&v).unwrap();
        assert_eq!(named(Removable::Weenie(100)), "weenie 100");
        assert_eq!(
            named(Removable::Quest("TestQuest".into())),
            "quest testquest"
        );
        assert_eq!(
            named(Removable::Loot(LootKind::Death, 5)),
            "death-treasure 5"
        );
        assert_eq!(
            named(Removable::Loot(LootKind::Wielded, 9)),
            "wielded-treasure 9"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_patchs_name_is_its_title_in_lowercase_words_joined_by_hyphens() {
        assert_eq!(patch_name("Frozen Valley"), "frozen-valley");
        assert_eq!(patch_name("  A Change of Heart!  "), "a-change-of-heart");
        assert_eq!(patch_name("Patch #3: Fixes"), "patch-3-fixes");
    }
}
