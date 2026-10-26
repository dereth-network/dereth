//! The Characters tab and Play As: a recipe made into the playtest character by the editor's own
//! server, then played at the camera, buffed.
//!
//! The tab's top row chooses the recipe and acts on its character: Play As, Reset, Delete and
//! Export. Under it are the recipe's heritage, gender, level, archetype and where it starts, then
//! five tabs: Build (its attributes, vitals and skills), Tweak (its innate split and the weights
//! its ranks are spent by), Items, Augments and Quests.
//!
//! Play As makes the character only when it is no longer its recipe's: the server makes it when
//! it is new, as character creation makes one, or applies the recipe to it; then the playtest
//! starts. Otherwise the character plays on as it was left, its loot, experience and quest flags
//! kept. Reset makes it from its recipe now, without playing. Export writes the character to a
//! file in the world's output folder, which another world's server can import.
//!
//! While the tab is open the server is asked what the recipe would make, each time what Play As
//! would send changes: the Items tab shows that gear before anything is made, and Play As and
//! Reset make exactly it.
use std::path::PathBuf;

use dereth_progression::order::{Made, Order};

use super::{EditorApp, Tab, UiAction};
use crate::characters::{
    valid_name, AugmentView, BuildView, CharacterTab, Gender, OwnedTables, Place,
};

/// What the server is asked to do with the selected recipe's character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    /// Make it, new, on the playtest account.
    Create,
    /// Make the one it has the recipe's again.
    Apply,
    /// Write it to `file`, with what `note` says of it.
    Export { file: PathBuf, note: PathBuf },
}

#[derive(Debug)]
pub(super) struct Work {
    /// Play once the character is made.
    pub play: bool,
    pub extra: Vec<String>,
    pub step: Step,
    /// The other step was tried first, and answered that this one is wanted.
    pub retried: bool,
    pub mark: Option<usize>,
    /// What the checks found of a character the server exported anyway.
    pub found: Option<String>,
}

/// How often, in frames, the creatures round the camera are gathered again while it stays in one
/// landblock and nothing is edited.
const PLACE_FRAMES: u64 = 300;

/// How tall the checks' findings stand at most, in points, before they scroll: four lines.
const FINDINGS_HEIGHT: f32 = 72.0;

/// How wide the recipe's choice and the new recipe's name are, in points.
const NAME_WIDTH: f32 = 170.0;

/// How wide the labels before the tab's choices are at least, in points, so that the choices of
/// one tab line up with those above it.
pub(super) const LABEL_WIDTH: f32 = 72.0;

impl EditorApp {
    /// Show the Characters tab: where it is, else floating over the world view's right, about
    /// 560 by 720 points.
    pub fn open_characters(&mut self) {
        // In a window a playtest hid: that window, back.
        self.bring_back(Tab::Characters);
        let world = self.world_rect.unwrap_or(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1400.0, 900.0),
        ));
        let size = egui::vec2(
            560.0_f32.min(world.width() - 32.0).max(320.0),
            720.0_f32.min(world.height() - 32.0).max(240.0),
        );
        let at = egui::Rect::from_min_size(
            egui::pos2(
                (world.max.x - 16.0 - size.x).max(world.min.x),
                world.min.y + 16.0,
            ),
            size,
        );
        let dock = if self.dungeon.is_some() {
            self.dungeon_dock
                .get_or_insert_with(crate::docks::in_dungeon)
        } else {
            self.dock.get_or_insert_with(crate::docks::outdoors)
        };
        crate::docks::open_floating(dock, Tab::Characters, at);
        self.show_tab(Tab::Characters);
    }

    /// Whether the Characters tab is showing.
    #[must_use]
    pub fn characters_open(&self) -> bool {
        let dock = if self.dungeon.is_some() {
            self.dungeon_dock.as_ref()
        } else {
            self.dock.as_ref()
        };
        dock.is_some_and(|d| crate::docks::holds(d, Tab::Characters))
    }

    /// The tables builds are worked out from, read from the world's files the first time.
    fn character_tables(&mut self) {
        if self.characters.tables.is_none() {
            match OwnedTables::read(self.session.base()) {
                Ok(t) => self.characters.tables = Some(t),
                Err(e) => self.characters.error = Some(e),
            }
        }
    }

    /// The creatures round the camera, gathered again once the camera is in another landblock,
    /// the world is edited, more of the server's weenies are known, or a while has passed.
    pub(super) fn place_frame(&mut self) {
        let Some((lb, ..)) = self.work_block() else {
            return;
        };
        self.place_ticks += 1;
        let key = (
            lb,
            self.session.history(),
            self.session.server_weenies().len(),
        );
        if self.characters.place_at == Some(key) && !self.place_ticks.is_multiple_of(PLACE_FRAMES) {
            return;
        }
        self.characters.place_at = Some(key);
        // In a dungeon, its own creatures; outdoors, those of the landblocks round the camera.
        let near = match self.dungeon.as_ref() {
            Some(d) => std::iter::once(d.landblock).collect(),
            None => self.near_camera(),
        };
        let creatures = self.session.creatures_in(&near);
        let found: Vec<(u32, Option<u32>)> = creatures
            .into_iter()
            .filter_map(|f| {
                let level = u32::try_from(f.level?).ok()?;
                let tier = f
                    .death_treasure
                    .and_then(|t| self.session.treasure(t))
                    .and_then(|t| u32::try_from(t.tier).ok())
                    .filter(|t| (1..=8).contains(t));
                Some((level, tier))
            })
            .collect();
        self.characters.place = Place::of(found);
        self.place_quests_frame(&near);
    }

    /// A new recipe named `name`, selected: of the level the place suggests, else the selected
    /// one's. A name that already has a recipe selects it.
    ///
    /// # Errors
    /// A name that is not one, or a file that will not write.
    pub fn new_character(&mut self, name: &str) -> Result<(), String> {
        let level = self
            .characters
            .place
            .map_or(self.characters.recipe.level, |p| p.median);
        self.characters.new_name.clear();
        name.clone_into(&mut self.characters.name);
        self.characters.load()?;
        if !self.characters.path()?.is_file() {
            self.characters.recipe.level = level.max(1);
        }
        self.characters.save()
    }

    /// The class a pack weenie's symbol has in this world.
    fn class_of(&self, symbol: &str) -> Option<u32> {
        self.session
            .compiled()
            .lock
            .of(lyceum_foundry::symbol::Kind::Wcid)
            .find(|(s, _)| *s == symbol)
            .map(|(_, id)| id)
    }

    /// The order the server makes the selected recipe's character from: its recipe at the
    /// place's tier, its kit's items first, its heritage and gender by the creation table's ids.
    /// Worked out again only when the recipe, the place's tier, the server's weenies known or
    /// the world's edits change.
    ///
    /// # Errors
    /// An archetype Lyceum lacks, an item that names no class, or tables not read.
    pub fn character_order(&mut self) -> Result<Order, String> {
        let tier = self.characters.place.and_then(|p| p.tier);
        let known = self.session.server_weenies().len();
        let edits = self.session.history();
        if let Some(((r, t, n, e), o)) = &self.characters.ordered {
            if *r == self.characters.recipe && *t == tier && *n == known && *e == edits {
                return o.clone();
            }
        }
        let made = (|| {
            let mut order = self.characters.recipe.order(tier, |s| self.class_of(s))?;
            order.items.splice(0..0, self.kit_items());
            let (heritage, gender) = self.characters.body()?;
            order.heritage = Some(heritage);
            order.gender = Some(gender);
            Ok(order)
        })();
        self.characters.ordered = Some((
            (self.characters.recipe.clone(), tier, known, edits),
            made.clone(),
        ));
        made
    }

    /// The Characters tab: the top row (the recipe, Play As, Reset, Delete, Export), the new
    /// recipe's name, the recipe's heritage, gender, level, archetype and start, what is said of
    /// it, then its tabs.
    pub(super) fn characters_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.character_tables();
        self.extras_frame();
        self.characters.tab_rects.clear();
        self.characters.picker_rows.clear();
        self.character_top(ui, out);
        self.character_checks(ui);
        ui.separator();
        self.character_tab_row(ui);
        ui.separator();
        match self.characters.tab {
            CharacterTab::Build => self.character_build_tab(ui, out),
            CharacterTab::Tweak => self.character_tweak_tab(ui, out),
            CharacterTab::Items => self.character_items_tab(ui, out),
            CharacterTab::Augments => self.character_augments_tab(ui, out),
            CharacterTab::Quests => self.character_quests_tab(ui, out),
        }
    }

    /// The rows above the tabs.
    fn character_top(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let busy = self.character_work.is_some();
        let live = self.live.is_some();
        let max_level = self
            .characters
            .tables
            .as_ref()
            .map_or(275, |x| x.tables().max_level());
        let offered = self.offered_archetypes();
        let heritages = self
            .characters
            .tables
            .as_ref()
            .map(|x| x.tables().heritages())
            .unwrap_or_default();
        let place = self.characters.place;
        let in_dungeon = self.dungeon.is_some();
        let can_keep = self.characters.archetypes_dir.is_some();
        let c = &mut self.characters;
        let before = c.recipe.clone();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("character-recipe")
                .selected_text(&c.name)
                .width(NAME_WIDTH)
                .show_ui(ui, |ui| {
                    for name in &c.names {
                        if ui.selectable_label(c.name == *name, name).clicked() {
                            out.push(UiAction::ChooseRecipe(name.clone()));
                        }
                    }
                });
            let play = ui
                .add_enabled(
                    !busy && live,
                    egui::Button::new(t.get("characters.play_as")),
                )
                .on_disabled_hover_text(t.get("top.play_tip"));
            if play.clicked() {
                out.push(UiAction::Play);
            }
            let reset = ui.add_enabled(!busy && live, egui::Button::new(t.get("characters.reset")));
            c.reset_button = Some(reset.rect);
            if reset.clicked() {
                out.push(UiAction::ResetCharacter);
            }
            if ui
                .add_enabled(!busy, egui::Button::new(t.get("characters.delete")))
                .clicked()
            {
                out.push(UiAction::DeleteRecipe(c.name.clone()));
            }
            let can_export = !busy && live && c.made.is_some();
            if ui
                .add_enabled(can_export, egui::Button::new(t.get("characters.export")))
                .on_hover_text(t.get("characters.export_tip"))
                .on_disabled_hover_text(t.get("characters.export_needs"))
                .clicked()
            {
                out.push(UiAction::ExportCharacter);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut c.new_name)
                    .hint_text(t.get("characters.new_name"))
                    .desired_width(NAME_WIDTH),
            );
            if ui
                .add_enabled(
                    valid_name(c.new_name.trim()),
                    egui::Button::new(t.get("characters.new")),
                )
                .clicked()
            {
                out.push(UiAction::NewRecipe(c.new_name.trim().to_owned()));
            }
        });
        if let Some(name) = c.replace_asked.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    super::panels::mark::WARN,
                    t.fill("characters.replace_ask", &[("name", &name)]),
                );
                if ui.button(t.get("characters.replace")).clicked() {
                    out.push(UiAction::ReplaceCharacter(name));
                }
            });
        }
        ui.add_space(4.0);
        // Shown as players read them; the recipe keeps the table's name.
        let heritage = c
            .tables
            .as_ref()
            .and_then(|x| c.recipe.heritage_id(&x.tables()).ok())
            .and_then(|h| heritages.iter().find(|(id, _)| *id == h))
            .map_or_else(
                || c.recipe.heritage.clone().unwrap_or_default(),
                |(id, n)| crate::characters::heritage_title(*id, n),
            );
        egui::Grid::new("character-recipe-grid")
            .min_col_width(LABEL_WIDTH)
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(t.get("characters.heritage"));
                egui::ComboBox::from_id_salt("character-heritage")
                    .selected_text(heritage.as_str())
                    .width(NAME_WIDTH)
                    .show_ui(ui, |ui| {
                        for (id, name) in &heritages {
                            let shown = crate::characters::heritage_title(*id, name);
                            let on = heritage == shown;
                            if ui.selectable_label(on, shown).clicked() && !on {
                                c.recipe.heritage = (*id != dereth_progression::heritage::ALUVIAN)
                                    .then(|| name.clone());
                            }
                        }
                    });
                ui.end_row();
                ui.label(t.get("characters.gender"));
                ui.horizontal(|ui| {
                    for (g, key) in [
                        (Gender::Male, "characters.male"),
                        (Gender::Female, "characters.female"),
                    ] {
                        ui.selectable_value(&mut c.recipe.gender, g, t.get(key));
                    }
                });
                ui.end_row();
                ui.label(t.get("characters.level"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut c.recipe.level).range(1..=max_level));
                    if let Some(p) = place {
                        ui.weak(t.fill(
                            "characters.monsters",
                            &[("least", &p.least), ("most", &p.most)],
                        ));
                    }
                });
                ui.end_row();
                ui.label(t.get("characters.archetype"));
                ui.horizontal(|ui| {
                    let name = offered
                        .iter()
                        .find(|(id, _)| *id == c.recipe.archetype)
                        .map_or(c.recipe.archetype.as_str(), |(_, n)| n.as_str());
                    let name = if c.recipe.plan_is_archetypes() {
                        name.to_owned()
                    } else {
                        t.fill("characters.archetype_changed", &[("name", &name)])
                    };
                    let mut own = None;
                    egui::ComboBox::from_id_salt("character-archetype")
                        .selected_text(name)
                        .width(NAME_WIDTH)
                        .show_ui(ui, |ui| {
                            for (id, name) in &offered {
                                ui.selectable_value(&mut c.recipe.archetype, id.clone(), name);
                            }
                            if !c.own.is_empty() {
                                ui.separator();
                                for (name, a) in &c.own {
                                    if ui.selectable_label(false, name).clicked() {
                                        own = Some(a.clone());
                                    }
                                }
                            }
                        });
                    // Another archetype is its own choices, not the old one's with this one's
                    // name.
                    if c.recipe.archetype != before.archetype {
                        c.recipe.reset_plan();
                    }
                    if let Some(a) = own {
                        c.recipe.take(&a);
                    }
                    save_as_archetype(ui, &mut c.own_name, can_keep, &t, out);
                });
                ui.end_row();
                ui.label(t.get("characters.start"));
                ui.horizontal(|ui| {
                    super::character_gear::start_choice(ui, &mut c.recipe.start, &t, in_dungeon);
                    ui.checkbox(&mut c.recipe.buffed, t.get("characters.buffed"));
                    if c.recipe.buffed {
                        let mut level = c.recipe.spell_level();
                        if ui
                            .add(egui::DragValue::new(&mut level).range(1..=8))
                            .changed()
                        {
                            c.recipe.buff_level = Some(level);
                        }
                    }
                });
                ui.end_row();
            });
        if !c.message.is_empty() {
            ui.label(&c.message);
        }
        if c.recipe != before {
            out.push(UiAction::RecipeChanged);
        }
    }

    /// The archetypes the world's tables offer, by id and name (the built-in ones before the
    /// tables are read), worked out once.
    fn offered_archetypes(&mut self) -> Vec<(String, String)> {
        let tables = self.characters.tables.is_some();
        if self
            .characters
            .extras
            .archetypes
            .as_ref()
            .is_none_or(|(read, _)| *read != tables)
        {
            let list = self.characters.tables.as_ref().map_or_else(
                || {
                    dereth_progression::archetypes::builtin()
                        .into_iter()
                        .map(|a| (a.id, a.name))
                        .collect()
                },
                |x| {
                    dereth_progression::archetypes::offered(&x.tables())
                        .into_iter()
                        .map(|a| (a.id, a.name))
                        .collect()
                },
            );
            self.characters.extras.archetypes = Some((tables, list));
        }
        self.characters
            .extras
            .archetypes
            .as_ref()
            .map(|(_, l)| l.clone())
            .unwrap_or_default()
    }

    /// The row of tabs; a weenie dropped on Items is carried.
    fn character_tab_row(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let mut dropped = None;
        ui.horizontal(|ui| {
            for tab in CharacterTab::ALL {
                let r = ui.selectable_value(&mut self.characters.tab, tab, t.get(tab.key()));
                if tab == CharacterTab::Items {
                    if let Some(w) = r.dnd_release_payload::<lyceum_foundry::edit::Weenie>() {
                        dropped = Some((*w).clone());
                    }
                }
                self.characters.tab_rects.push((tab, r.rect));
            }
        });
        if let Some(w) = dropped {
            self.carry(&w);
        }
    }

    /// The checks: one line when all is well, else each finding.
    fn character_checks(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        if let Some(e) = &self.characters.error {
            ui.colored_label(super::panels::mark::FAULT, e);
        }
        let mut gear = self.gear_findings();
        gear.extend(self.extras_findings());
        match self.characters.findings() {
            Ok(mut found) => {
                found.extend(gear);
                if found.iter().all(|f| f.warning) {
                    ui.colored_label(super::panels::mark::Y, t.get("characters.authentic"));
                }
                // What refuses the character first; a few lines, then they scroll.
                found.sort_by_key(|f| f.warning);
                egui::ScrollArea::vertical()
                    .id_salt("character-findings")
                    .auto_shrink([false, true])
                    .max_height(FINDINGS_HEIGHT)
                    .show(ui, |ui| {
                        for f in found {
                            let color = if f.warning {
                                super::panels::mark::WARN
                            } else {
                                super::panels::mark::FAULT
                            };
                            ui.colored_label(color, f.text);
                        }
                    });
            }
            Err(e) => {
                ui.colored_label(super::panels::mark::FAULT, e);
            }
        }
    }

    /// Make the selected recipe's character on the server: made new when the server has none of
    /// it, else the recipe applied to it; with `play`, the playtest starts after.
    pub(super) fn prepare_character(&mut self, play: bool) -> Result<(), String> {
        if self.character_work.is_some() {
            return Err("A character is already being prepared.".into());
        }
        self.characters.error = None;
        self.characters.save()?;
        if let Some(f) = self
            .gear_findings()
            .into_iter()
            .chain(self.extras_findings())
            .chain(self.characters.findings()?)
            .find(|f| !f.warning)
        {
            return Err(f.text);
        }
        // The order the tab showed the items of.
        let order = self.character_order()?;
        let path = self.characters.order_path();
        std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))
            .map_err(|e| e.to_string())?;
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&order).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        self.stop_embedded();
        let live = self
            .live
            .as_mut()
            .ok_or("Start Live before preparing a character.")?;
        if !live.wait_empty() || !live.wait_account() {
            return Err("The character has not finished logging out.".into());
        }
        // A character the server made before is applied to; any other is made, and the server
        // says which it was when that is wrong.
        let step = if self.characters.made.is_some() {
            Step::Apply
        } else {
            Step::Create
        };
        self.character_work = Some(Work {
            play,
            extra: Vec::new(),
            step,
            retried: false,
            mark: None,
            found: None,
        });
        self.characters.message = self.say("characters.preparing");
        Ok(())
    }

    /// Ask the server to do `step` to the selected recipe's character once it is logged out:
    /// any playtest stopped first.
    fn character_task(&mut self, step: Step, saying: &str) -> Result<(), String> {
        if self.character_work.is_some() {
            return Err("A character is already being prepared.".into());
        }
        self.characters.error = None;
        self.stop_embedded();
        let live = self
            .live
            .as_mut()
            .ok_or("Start Live before preparing a character.")?;
        if !live.wait_empty() || !live.wait_account() {
            return Err("The character has not finished logging out.".into());
        }
        self.character_work = Some(Work {
            play: false,
            extra: Vec::new(),
            step,
            retried: false,
            mark: None,
            found: None,
        });
        self.characters.message = self.say(saying);
        Ok(())
    }

    /// Write the selected recipe's character, as the server holds it, to its file in the world's
    /// output folder (an earlier export of it replaced), for another world to import.
    ///
    /// # Errors
    /// No server running, a character it has not made, or a file that will not write or go.
    pub fn export_character(&mut self) -> Result<(), String> {
        if self.characters.made.is_none() {
            return Err(self.say("characters.export_needs"));
        }
        let symbols = self
            .session
            .compiled()
            .lock
            .of(lyceum_foundry::symbol::Kind::Wcid)
            .map(|(s, id)| (id, s.to_owned()))
            .collect();
        let note = self.characters.export_note(symbols);
        let note_path = self.characters.export_note_path();
        let file = self.characters.export_path(&self.session.world().out);
        for dir in [note_path.parent(), file.parent()].into_iter().flatten() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &note_path,
            serde_json::to_vec_pretty(&note).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if file.exists() {
            std::fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
        self.character_task(
            Step::Export {
                file,
                note: note_path,
            },
            "characters.exporting",
        )
    }

    /// Each frame: the creatures round the camera and what the server says the recipe would make
    /// while the tab shows, and a character being made: the server asked to make it or apply its
    /// recipe, and its answer read.
    pub(super) fn character_frame(&mut self) {
        if self.characters_open() {
            self.place_frame();
            self.preview_frame();
        }
        let Some(mut work) = self.character_work.take() else {
            return;
        };
        let result = (|| -> Result<bool, String> {
            let live = self.live.as_mut().ok_or("The playtest server stopped.")?;
            if let Some(mark) = work.mark {
                for line in live.log_from(mark) {
                    if line.contains("Playtest character updated")
                        || line.contains("Playtest character created.")
                    {
                        return Ok(true);
                    }
                    if let Some((_, rest)) = line.split_once("Playtest character exported") {
                        work.found = rest
                            .split_once("so no world will import it: ")
                            .map(|(_, why)| why.to_owned());
                        return Ok(true);
                    }
                    if let Some((_, why)) = line.split_once("Playtest character not exported: ") {
                        return Err(why.to_owned());
                    }
                    if let Some((_, why)) = line.split_once("Playtest character unchanged: ") {
                        // The server has the character after all, or has none: the other step.
                        let other = match work.step {
                            Step::Create if why.contains("the name is taken") => Step::Apply,
                            Step::Apply if why.contains("must exist") => Step::Create,
                            _ => return Err(why.to_owned()),
                        };
                        if work.retried {
                            return Err(why.to_owned());
                        }
                        work.step = other;
                        work.retried = true;
                        work.mark = None;
                        return Ok(false);
                    }
                }
                return Ok(false);
            }
            let made = self.characters.made_path();
            std::fs::create_dir_all(made.parent().unwrap_or(std::path::Path::new(".")))
                .map_err(|e| e.to_string())?;
            let name = &self.characters.name;
            if let Step::Export { file, note } = &work.step {
                work.mark = Some(live.log_from(0).len());
                live.command(&format!(
                    "playtest-character export \"+{name}\" {} {}",
                    crate::live::console_path(file),
                    crate::live::console_path(note),
                ))?;
                return Ok(false);
            }
            if work.step == Step::Create {
                // The playtest account, an administrator's, as its first login would make it.
                live.command("accountcreate builder builder Admin")?;
                work.mark = Some(live.log_from(0).len());
                live.command(&format!(
                    "playtest-character create builder \"+{}\" {} {}",
                    self.characters.name,
                    crate::live::console_path(&self.characters.order_path()),
                    crate::live::console_path(&made),
                ))?;
                return Ok(false);
            }
            // No backup is kept: the recipe makes the character again whenever it is wanted.
            work.mark = Some(live.log_from(0).len());
            live.command(&format!(
                "playtest-character apply \"+{}\" {} - {}{}",
                self.characters.name,
                crate::live::console_path(&self.characters.order_path()),
                crate::live::console_path(&made),
                if self.characters.may_replace() {
                    " replace"
                } else {
                    ""
                }
            ))?;
            Ok(false)
        })();
        match result {
            Ok(false) => self.character_work = Some(work),
            Ok(true) if matches!(work.step, Step::Export { .. }) => {
                self.character_exported(&work);
            }
            Ok(true) => {
                let done = self.characters.applied();
                if done.exists() {
                    if let Err(e) = std::fs::remove_file(done) {
                        self.characters.error = Some(e.to_string());
                    }
                }
                self.characters.replace_asked = None;
                self.characters.read_made();
                self.characters.message = self.say(if work.step == Step::Create {
                    "characters.created"
                } else {
                    "characters.ready"
                });
                if work.play {
                    if let Err(e) = self.play(&work.extra, false) {
                        self.characters.error = Some(e.clone());
                        self.characters.message = e;
                    }
                }
            }
            Err(e) => {
                if e.contains("not made from a recipe") {
                    self.characters.replace_asked = Some(self.characters.name.clone());
                }
                self.characters.error = Some(e.clone());
                self.characters.message.clone_from(&e);
                self.panels.message = e;
            }
        }
    }

    /// What the server says the selected recipe would make: asked again whenever the order Play
    /// As would send, or the world the server holds, changes; one question at a time, never while
    /// a character is being made.
    fn preview_frame(&mut self) {
        let Some(revision) = self.live.as_ref().map(|l| l.revision) else {
            self.characters.preview.asked = None;
            self.characters.preview.mark = None;
            return;
        };
        if let Some(mark) = self.characters.preview.mark {
            let Some(live) = self.live.as_ref() else {
                return;
            };
            let said = live.log_from(mark).into_iter().find_map(|l| {
                if l.contains("Playtest character previewed.") {
                    return Some(None);
                }
                l.split_once("Playtest character not previewed: ")
                    .map(|(_, why)| Some(why.trim().to_owned()))
            });
            let Some(said) = said else {
                return;
            };
            self.characters.preview.mark = None;
            let Some((order, _)) = self.characters.preview.asked.clone() else {
                return;
            };
            let answer = match said {
                Some(why) => Err(why),
                None => std::fs::read(self.characters.preview_paths().1)
                    .map_err(|e| e.to_string())
                    .and_then(|b| serde_json::from_slice::<Made>(&b).map_err(|e| e.to_string())),
            };
            self.characters.preview.said = Some((order, answer));
            return;
        }
        // A character still its recipe's shows what it has; nothing is asked while one is made.
        if self.character_work.is_some() || self.live_dirty || !self.characters.needs_making() {
            return;
        }
        let Ok(order) = self.character_order() else {
            return;
        };
        if self
            .characters
            .preview
            .asked
            .as_ref()
            .is_some_and(|(o, r)| *o == order && *r == revision)
        {
            return;
        }
        let (input, result) = self.characters.preview_paths();
        let asked = (|| -> Result<usize, String> {
            std::fs::create_dir_all(input.parent().unwrap_or(std::path::Path::new(".")))
                .map_err(|e| e.to_string())?;
            std::fs::write(
                &input,
                serde_json::to_vec_pretty(&order).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let live = self.live.as_mut().ok_or("no server")?;
            let mark = live.log_len();
            live.command(&format!(
                "playtest-character preview {} {}",
                crate::live::console_path(&input),
                crate::live::console_path(&result),
            ))?;
            Ok(mark)
        })();
        self.characters.preview.asked = Some((order.clone(), revision));
        match asked {
            Ok(mark) => self.characters.preview.mark = Some(mark),
            Err(e) => self.characters.preview.said = Some((order, Err(e))),
        }
    }

    /// Wait until the server has said what the selected recipe would make, as a test does.
    ///
    /// # Errors
    /// No server, or one that did not answer.
    pub fn wait_preview(&mut self) -> Result<Option<Made>, String> {
        let start = std::time::Instant::now();
        loop {
            if !self.frame() {
                return Err("The editor closed while asking the server.".into());
            }
            let order = self.character_order()?;
            let live = self.live.is_some();
            match self.characters.shown(Some(&order), live) {
                crate::characters::Shown::Preview(m) => return Ok(Some(m.clone())),
                crate::characters::Shown::Made(_) => return Ok(None),
                crate::characters::Shown::Refused(e) => return Err(e.to_owned()),
                crate::characters::Shown::NoServer => return Err("No server runs.".into()),
                crate::characters::Shown::Asking(_) => {}
            }
            // Only against a hang: the server's answer is what ends the wait.
            if start.elapsed() > std::time::Duration::from_secs(120) {
                return Err("The server did not say what the recipe would make.".into());
            }
        }
    }

    /// An export the server has done: what was written said.
    fn character_exported(&mut self, work: &Work) {
        let Step::Export { file, .. } = &work.step else {
            return;
        };
        let name = self.characters.name.clone();
        let path = file.display().to_string();
        self.characters.message = match &work.found {
            None => self.say_with("characters.exported", &[("name", &name), ("path", &path)]),
            Some(why) => self.say_with(
                "characters.exported_unchecked",
                &[("name", &name), ("path", &path), ("why", why)],
            ),
        };
        self.panels.message.clone_from(&self.characters.message);
    }

    /// The `@buff` line the playtest's character says after it arrives, when its recipe is
    /// buffed.
    pub(super) fn buff_line(&self) -> Option<String> {
        let r = &self.characters.recipe;
        r.buffed
            .then(|| format!("@buff {} {}", self.characters.name, r.spell_level()))
    }

    /// Wait until the character is made, as a script does.
    ///
    /// # Errors
    /// Why it was not.
    pub fn wait_character(&mut self) -> Result<(), String> {
        let start = std::time::Instant::now();
        while self.character_work.is_some() {
            if !self.frame() {
                return Err("The editor closed while preparing a character.".into());
            }
            // Only against a hang: the server's answer is what ends the wait.
            if start.elapsed() > std::time::Duration::from_secs(120) {
                self.character_work = None;
                self.stop_embedded();
                return Err("The server did not finish preparing the character.".into());
            }
        }
        self.characters.error.clone().map_or(Ok(()), Err)
    }

    /// Reset: make the selected recipe's character from its recipe now, without playing, even
    /// when it is still the recipe's (what it gained in play goes).
    ///
    /// # Errors
    /// No server running, or a recipe the checks refuse.
    pub fn reset_character(&mut self) -> Result<(), String> {
        self.prepare_character(false)
    }

    /// Make the selected recipe's character now and wait for it, without playing it (Reset, as
    /// a script's `reset_character` asks).
    ///
    /// # Errors
    /// No server running, or one that refused.
    pub fn make_character(&mut self) -> Result<(), String> {
        self.reset_character()?;
        self.wait_character()
    }
}

/// Save as archetype: a button opening a name for the builder's own archetype and Save.
fn save_as_archetype(
    ui: &mut egui::Ui,
    name: &mut String,
    can_keep: bool,
    t: &crate::text::Table,
    out: &mut Vec<UiAction>,
) {
    use egui::containers::menu::{MenuButton, MenuConfig};
    let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    MenuButton::new(t.get("characters.save_own"))
        .config(config)
        .ui(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text(t.get("characters.own_name"))
                        .desired_width(NAME_WIDTH),
                );
                let can = can_keep && valid_name(name.trim());
                if ui
                    .add_enabled(can, egui::Button::new(t.get("characters.save")))
                    .clicked()
                {
                    out.push(UiAction::SaveArchetype(name.trim().to_owned()));
                    ui.close();
                }
            });
        });
}

impl crate::characters::Characters {
    /// Show `tab`, and in the Build and Augments tabs `build` and `augment`, as a script or a
    /// test asks.
    pub fn show(&mut self, tab: CharacterTab, build: BuildView, augment: AugmentView) {
        self.tab = tab;
        self.build_view = build;
        self.augment_view = augment;
    }
}
