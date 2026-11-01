//! Play and Live from the editor's side: the playtest characters and their window, the editor's
//! own Yalain started for the character chosen where its recipe starts it, each change published
//! to it once the hand that made it lets go, its answers shown, a restart when the placed rows
//! need one, and a client started into it.

use athenaeum_foundry::Entry;
use dereth_primitives::num::math;
use yalain_world_db::rows::Row;

use super::{entry_global, heading_of, Editor};
use crate::characters::{self, Recipe, Start, RACES, SEXES};
use crate::live::{self, Event, Reloaded, Setup};
use crate::view;

/// The recipe Play makes when a world has none yet.
pub const FIRST: &str = "Builder";

/// How far in front of the selected row a character starting there stands, meters.
const IN_FRONT: f32 = 2.0;

impl Editor<'_> {
    /// Where the world's playtest is kept, and what it plays with.
    #[must_use]
    pub fn setup(&self) -> Setup {
        Setup {
            programs: self.programs.clone(),
            folder: self.session.dir().join(live::FOLDER),
            base: self.base.clone(),
        }
    }

    /// Read the world's recipes again.
    pub fn read_characters(&mut self) {
        let (recipes, problems) = characters::list(self.session.dir());
        self.recipes = recipes;
        self.recipe_problems = problems;
        if !self.recipes.iter().any(|r| r.name == self.character) {
            self.character = self
                .recipes
                .first()
                .map_or_else(|| FIRST.to_string(), |r| r.name.clone());
        }
    }

    /// The recipe Play plays as: the one chosen, or a human man at the camera when the world has
    /// none yet.
    #[must_use]
    pub fn recipe(&self) -> Recipe {
        self.recipes
            .iter()
            .find(|r| r.name == self.character)
            .cloned()
            .unwrap_or_else(|| Recipe::new(&self.character))
    }

    /// Write a recipe into the world and choose it.
    ///
    /// # Errors
    /// A name the server would refuse, or a file that will not write.
    pub fn keep_character(&mut self, r: &Recipe) -> Result<(), String> {
        characters::save(self.session.dir(), r)?;
        self.character.clone_from(&r.name);
        self.read_characters();
        Ok(())
    }

    /// Where a row stands as a place to enter: its cell, its point (on the ground where it gives
    /// no height) and its heading; `ahead` meters in front of it, facing it, when `ahead` is not 0.
    fn entry_at(&mut self, e: &Entry, ahead: f32) -> live::Entry {
        let g = entry_global(e, &mut self.scene);
        let h = heading_of(&e.row.row);
        let cell = match &e.row.row {
            Row::Object(o) => o.cell,
            Row::Camp(c) => c.cell,
            Row::Place(p) => p.cell,
            _ => None,
        };
        let indoors = cell.filter(|c| c & 0xFFFF >= 0x100);
        let r = h.to_radians();
        let (x, y) = (g[0] + ahead * math::sinf(r), g[1] + ahead * math::cosf(r));
        let z = if indoors.is_some() {
            g[2]
        } else {
            self.scene.ground(x, y).unwrap_or(g[2])
        };
        let (lb, at) = view::local([x, y, z]);
        let facing = if ahead == 0.0 { h } else { h + 180.0 };
        live::Entry {
            cell: indoors
                .unwrap_or_else(|| yalain_world_db::compile::outdoor_cell(lb, at[0], at[1])),
            at: [at[0], at[1], z + 0.5],
            heading: facing.rem_euclid(360.0),
        }
    }

    /// Where a recipe's character enters: `None` where it last left the world (the camera's place
    /// when it has not been made yet).
    ///
    /// # Errors
    /// A start at the selection with nothing selected, or at a place the world does not have.
    pub fn entry_for(&mut self, r: &Recipe) -> Result<Option<live::Entry>, String> {
        Ok(match &r.start {
            Start::Camera => Some(self.entry_point()),
            Start::Left => {
                let (make, _) = characters::to_make(&self.setup().folder, r);
                make.then(|| self.entry_point())
            }
            Start::Selection => {
                let item = self
                    .selected
                    .clone()
                    .ok_or("select a row for the character to start in front of")?;
                let e = self
                    .session
                    .entry(&item)
                    .cloned()
                    .ok_or_else(|| format!("no {item}"))?;
                Some(self.entry_at(&e, IN_FRONT))
            }
            Start::Place(key) => {
                let item = format!("place {key}");
                let e = self
                    .session
                    .entry(&item)
                    .cloned()
                    .ok_or_else(|| format!("{}'s start: the world has no {item}", r.name))?;
                Some(self.entry_at(&e, 0.0))
            }
        })
    }

    /// Start the editor's own Yalain on the world as it stands, as the character chosen, entering
    /// where its recipe says.
    pub fn serve(&mut self) {
        if !self.session.problems().is_empty() {
            self.status = "the world has problems: Yalain would not start on it".into();
            return;
        }
        let recipe = self.recipe();
        if !self.recipes.iter().any(|r| r.name == recipe.name) {
            if let Err(e) = self.keep_character(&recipe) {
                self.status = e;
                return;
            }
        }
        let entry = match self.entry_for(&recipe) {
            Ok(e) => e,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let setup = self.setup();
        match self
            .live
            .start(&setup, self.session.files(), &recipe, entry)
        {
            Ok(()) => {
                self.published = self.session.revision();
                self.waiting.clear();
                self.status.clone_from(&self.live.said);
            }
            Err(e) => self.status = e,
        }
    }

    /// Start a client into the editor's own Yalain as the character chosen: the server is started
    /// first (again, so the character enters where its recipe says, and a client playing already
    /// leaves it with the character kept), unless it serves that character already, no client
    /// plays, and its recipe keeps it where it left the world.
    pub fn play(&mut self, retail: bool) {
        let recipe = self.recipe();
        let serving = self.live.serving() && self.live.character() == Some(recipe.name.as_str());
        if !(serving && recipe.start == Start::Left && !self.live.playing()) {
            self.live.stop_client();
            self.serve();
        }
        if self.live.serving() {
            self.live.play_when_ready(retail);
            self.status = format!("{} plays once Yalain listens", recipe.name);
        }
    }

    /// Start the server again on the world as it stands, the character where it stands.
    pub fn restart(&mut self) {
        let setup = self.setup();
        match self.live.restart(&setup, self.session.files()) {
            Ok(()) => {
                self.published = self.session.revision();
                self.waiting.clear();
                self.status = "Yalain starts again for the placed rows and camps".into();
            }
            Err(e) => self.status = e,
        }
    }

    /// Stop the client and the server, keeping the character where it stands.
    pub fn stop_playing(&mut self) {
        self.live.stop_all();
        self.waiting.clear();
        self.status = "Yalain stopped, its characters kept".into();
    }

    /// Take a character back to its recipe: its next play makes it anew. The server stops first
    /// when it serves that character.
    pub fn reset_character(&mut self, name: &str) {
        if self.live.character() == Some(name) {
            self.live.stop_all();
        }
        self.status = match characters::reset(&self.setup().folder, name) {
            Ok(()) => format!("{name} is made from its recipe at its next play"),
            Err(e) => e,
        };
    }

    /// Take a recipe and its character away.
    pub fn delete_character(&mut self, name: &str) {
        if self.live.character() == Some(name) {
            self.live.stop_all();
        }
        self.status = match characters::delete(self.session.dir(), &self.setup().folder, name) {
            Ok(()) => format!("{name} deleted"),
            Err(e) => e,
        };
        self.read_characters();
    }

    /// Each frame: what the server did, and the world published to it when it changed and no
    /// hand is still on what changed.
    pub(super) fn live_frame(&mut self) {
        let setup = self.setup();
        match self.live.tick(&setup) {
            Ok(events) => {
                for e in events {
                    self.on_live(e);
                }
            }
            Err(e) => self.status = e,
        }
        let changed = self.session.revision() != self.published;
        if !changed || self.drag.is_some() || self.held.is_some() || !self.live.up() {
            return;
        }
        if !self.session.problems().is_empty() {
            return;
        }
        match self.live.publish(&setup, self.session.files()) {
            Ok(true) => self.published = self.session.revision(),
            Ok(false) => {}
            Err(e) => self.status = e,
        }
    }

    fn on_live(&mut self, e: Event) {
        match e {
            Event::Up => {
                self.status = format!(
                    "Yalain is up on 127.0.0.1:{} as {}",
                    self.live.port().unwrap_or(0),
                    self.live.character().unwrap_or_default()
                );
            }
            Event::Playing => self.status.clone_from(&self.live.said),
            Event::Reloaded(Reloaded::Refused(why)) => {
                self.status = format!("Yalain kept the world as it was: {why}");
            }
            Event::Reloaded(Reloaded::Taken { applied, restart }) => {
                if restart.is_empty() {
                    self.status = if applied.is_empty() {
                        "published: nothing Yalain plays changed".into()
                    } else {
                        format!("published: Yalain took {}", applied.join(", "))
                    };
                } else if self.restart_changes {
                    self.restart();
                } else {
                    self.status = format!("published: {} wait for Restart", restart.join(", "));
                    self.waiting = restart;
                }
            }
            Event::Stopped(tail) => self.status = format!("Yalain stopped: {tail}"),
            Event::Note(said) => self.status = said,
        }
    }

    /// The top bar's play: the character, Characters, Live, Play and Restart.
    pub(super) fn play_bar(&mut self, ui: &mut egui::Ui) {
        let serving = self.live.serving();
        let mut chosen = self.character.clone();
        egui::ComboBox::from_id_salt("character")
            .selected_text(&chosen)
            .show_ui(ui, |ui| {
                for r in &self.recipes {
                    ui.selectable_value(&mut chosen, r.name.clone(), &r.name);
                }
            })
            .response
            .on_hover_text("the playtest character Play plays as");
        self.character = chosen;
        if ui
            .selectable_label(self.characters_open, "Characters")
            .clicked()
        {
            self.characters_open = !self.characters_open;
        }
        if ui
            .selectable_label(serving, "Live")
            .on_hover_text("Yalain on this computer alone, serving the world as it is edited")
            .clicked()
        {
            if serving {
                self.stop_playing();
            } else {
                self.serve();
            }
        }
        if self.programs.client.is_some()
            && ui
                .button("Play")
                .on_hover_text("our client, as the character, where its recipe starts it")
                .clicked()
        {
            self.play(false);
        }
        if self.programs.retail.is_some() && ui.button("Play retail").clicked() {
            self.play(true);
        }
        if serving && !self.waiting.is_empty() && ui.button("Restart").clicked() {
            self.restart();
        }
    }

    /// The Characters window: the recipes, a new one, and the chosen one's fields and actions.
    #[allow(clippy::too_many_lines)] // one window, field by field
    pub(super) fn characters_window(&mut self, ctx: &egui::Context) {
        if !self.characters_open {
            return;
        }
        let mut open = true;
        let mut act: Option<(&str, String)> = None;
        let mut changed: Option<Recipe> = None;
        let places: Vec<String> = self
            .session
            .entries()
            .iter()
            .filter(|e| matches!(e.row.row, Row::Place(_)))
            .filter_map(|e| e.item.strip_prefix("place ").map(str::to_string))
            .collect();
        let folder = self.setup().folder;
        egui::Window::new("Characters")
            .open(&mut open)
            .default_size([340.0, 380.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for r in &self.recipes {
                        if ui
                            .selectable_label(r.name == self.character, &r.name)
                            .clicked()
                        {
                            self.character.clone_from(&r.name);
                        }
                    }
                });
                for p in &self.recipe_problems {
                    ui.label(egui::RichText::new(p).color(egui::Color32::from_rgb(255, 140, 120)));
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_name)
                            .hint_text("name")
                            .desired_width(140.0),
                    );
                    if ui.button("New").clicked() {
                        act = Some(("new", self.new_name.trim().to_string()));
                    }
                });
                ui.separator();
                let Some(mut r) = self
                    .recipes
                    .iter()
                    .find(|r| r.name == self.character)
                    .cloned()
                else {
                    ui.label(egui::RichText::new("no character yet: make one with New").weak());
                    return;
                };
                let before = r.clone();
                ui.heading(&r.name);
                egui::Grid::new("recipe").num_columns(2).show(ui, |ui| {
                    ui.label("Race");
                    egui::ComboBox::from_id_salt("race")
                        .selected_text(r.race_word())
                        .show_ui(ui, |ui| {
                            for (w, v) in RACES {
                                ui.selectable_value(&mut r.race, v, w);
                            }
                        });
                    ui.end_row();
                    ui.label("Sex");
                    egui::ComboBox::from_id_salt("sex")
                        .selected_text(r.sex_word())
                        .show_ui(ui, |ui| {
                            for (w, v) in SEXES {
                                ui.selectable_value(&mut r.sex, v, w);
                            }
                        });
                    ui.end_row();
                    ui.label("Starts");
                    egui::ComboBox::from_id_salt("start")
                        .selected_text(r.start.word())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut r.start, Start::Camera, "at the camera");
                            ui.selectable_value(
                                &mut r.start,
                                Start::Selection,
                                "in front of the selection",
                            );
                            ui.selectable_value(&mut r.start, Start::Left, "where it left");
                            for k in &places {
                                ui.selectable_value(
                                    &mut r.start,
                                    Start::Place(k.clone()),
                                    format!("place {k}"),
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("Hero");
                    ui.checkbox(&mut r.hero, "earns focus");
                    ui.end_row();
                });
                if r != before {
                    changed = Some(r.clone());
                }
                let made = characters::made(&folder, &r.name);
                ui.label(
                    egui::RichText::new(match made {
                        Some(m) if r.same_body(&m) => "made: it plays on as it was left",
                        Some(_) => "its race or sex changed: it is made anew at its next play",
                        None => "not made yet: its first play makes it",
                    })
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Play as").clicked() {
                        act = Some(("play", r.name.clone()));
                    }
                    if ui
                        .button("Reset")
                        .on_hover_text("make it from its recipe at its next play")
                        .clicked()
                    {
                        act = Some(("reset", r.name.clone()));
                    }
                    if ui.button("Delete").clicked() {
                        act = Some(("delete", r.name.clone()));
                    }
                });
                ui.separator();
                ui.checkbox(
                    &mut self.restart_changes,
                    "start Yalain again for changes to placed rows and camps",
                )
                .on_hover_text(
                    "Yalain takes most changes at once; placed rows and camps need it to start \
                     again, the character staying where it stands",
                );
            });
        if !open {
            self.characters_open = false;
        }
        if let Some(r) = changed {
            if let Err(e) = self.keep_character(&r) {
                self.status = e;
            }
        }
        match act {
            Some(("new", name)) => {
                if self
                    .recipes
                    .iter()
                    .any(|r| r.name.eq_ignore_ascii_case(&name))
                {
                    self.status = format!("{name} is a character already");
                } else {
                    match self.keep_character(&Recipe::new(&name)) {
                        Ok(()) => {
                            self.new_name.clear();
                            self.status = format!("{name} made: a human man at the camera");
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            Some(("play", name)) => {
                self.character = name;
                self.play(false);
            }
            Some(("reset", name)) => self.reset_character(&name),
            Some(("delete", name)) => self.delete_character(&name),
            _ => {}
        }
    }
}
