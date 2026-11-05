//! Play and Live from the editor's side: the playtest characters and their window, the editor's
//! own Yalain started on the world (one server for every character, switched between without a
//! restart), each change published to it once the hand that made it lets go, its answers shown
//! (Yalain takes every change of the world at once), Restart by hand, the character chosen made,
//! given its build or put where its recipe starts it and a client started into it, the character
//! sent to the camera, and characters written to a file for another world and brought from one.

use std::path::Path;

use athenaeum_foundry::Entry;
use dereth_primitives::num::math;
use yalain_world_db::rows::Row;

use super::{entry_global, heading_of, Editor};
use crate::characters::{self, Item, Making, Quest, QuestAt, Recipe, Skill, Start, RACES, SEXES};
use crate::live::{self, Ask, Event, Reloaded, Setup};
use crate::view;

/// The recipe Play makes when a world has none yet.
pub const FIRST: &str = "Builder";

/// How far in front of the selected row a character starting there stands, meters.
const IN_FRONT: f32 = 2.0;

/// How many names a half-typed skill or quest offers.
const OFFERED: usize = 6;

/// What the Characters window is adding to the chosen recipe as it is typed, and the character
/// file it brings in.
#[derive(Debug, Clone)]
pub struct Adding {
    /// A skill by its name or number, and the rating it is raised to.
    pub skill: String,
    pub rating: u32,
    /// An item by its template, and how many.
    pub item: String,
    pub quantity: u32,
    /// A quest by its name or number, and its phase (0: done).
    pub quest: String,
    pub phase: u32,
    /// A character file to bring in, and the name it comes in under (its own when empty).
    pub import: String,
    pub import_name: String,
    /// The base's quests, by number, with their names (read once).
    pub quests: Option<Vec<(u32, String)>>,
}

impl Default for Adding {
    fn default() -> Self {
        Self {
            skill: String::new(),
            rating: 1,
            item: String::new(),
            quantity: 1,
            quest: String::new(),
            phase: 0,
            import: String::new(),
            import_name: String::new(),
            quests: None,
        }
    }
}

/// A number written in decimal, or in hexadecimal after `0x`.
#[must_use]
pub fn number(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// What `typed` names among `names`: a number, a name in any case, or else the one name that
/// holds it.
#[must_use]
pub fn pick(names: &[(u32, String)], typed: &str) -> Option<u32> {
    let t = typed.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(n) = number(t) {
        return Some(n);
    }
    if let Some((n, _)) = names.iter().find(|(_, w)| w.eq_ignore_ascii_case(t)) {
        return Some(*n);
    }
    let mut holding = matching(names, t, 2).into_iter();
    match (holding.next(), holding.next()) {
        (Some((n, _)), None) => Some(*n),
        _ => None,
    }
}

/// The first `most` of `names` holding `typed`, in any case.
#[must_use]
pub fn matching<'a>(
    names: &'a [(u32, String)],
    typed: &str,
    most: usize,
) -> Vec<&'a (u32, String)> {
    let t = typed.trim().to_ascii_lowercase();
    if t.is_empty() {
        return Vec::new();
    }
    names
        .iter()
        .filter(|(_, w)| w.to_ascii_lowercase().contains(&t))
        .take(most)
        .collect()
}

/// The name `names` gives `n`, or its number.
fn name_in(names: &[(u32, String)], n: u32, hex: bool) -> String {
    let num = if hex {
        format!("0x{n:08X}")
    } else {
        n.to_string()
    };
    match names.iter().find(|(x, _)| *x == n) {
        Some((_, w)) if !w.is_empty() => format!("{w} ({num})"),
        _ => num,
    }
}

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

    /// The quests of the world's base, by number, with their names.
    fn quest_names(&self) -> Vec<(u32, String)> {
        let mut out: Vec<(u32, String)> = self
            .session
            .base()
            .merged
            .rows
            .iter()
            .filter_map(|l| match &l.row {
                Row::Quest(q) => Some((q.id, q.id_name.clone().unwrap_or_default())),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup_by_key(|(id, _)| *id);
        out
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
        // A room's point is in its own block's frame, whichever block's ground it lies over.
        let at = indoors.map_or(at, |c| {
            #[allow(clippy::cast_possible_truncation)]
            view::local_in((c >> 16) as u16, [x, y, z])
        });
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
                let make = characters::making(&self.setup().folder, r) == Making::Make;
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

    /// Start the editor's own Yalain on the world as it stands, unless it runs: answers whether it
    /// runs now.
    fn ensure_serving(&mut self) -> bool {
        if self.live.serving() {
            return true;
        }
        if !self.session.problems().is_empty() {
            self.status = "the world has problems: Yalain would not start on it".into();
            return false;
        }
        let setup = self.setup();
        match self.live.start(&setup, self.session.files()) {
            Ok(()) => {
                self.published = self.session.revision();
                self.status.clone_from(&self.live.said);
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    /// Ask Yalain for `recipe`'s character as it plays next, where its recipe starts it (the
    /// recipe kept in the world first when it is new): made, given its build, made a hero or put
    /// there. Answers whether it was asked.
    fn prepare(&mut self, recipe: &Recipe) -> bool {
        if !self.recipes.iter().any(|r| r.name == recipe.name) {
            if let Err(e) = self.keep_character(recipe) {
                self.status = e;
                return false;
            }
        }
        let entry = match self.entry_for(recipe) {
            Ok(e) => e,
            Err(e) => {
                self.status = e;
                return false;
            }
        };
        let setup = self.setup();
        match self.live.prepare(&setup, recipe, entry) {
            Ok(()) => true,
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    /// Start the editor's own Yalain on the world as it stands, and ask it for the character
    /// chosen where its recipe starts it.
    pub fn serve(&mut self) {
        let recipe = self.recipe();
        if self.ensure_serving() {
            self.prepare(&recipe);
        }
    }

    /// Start a client into the editor's own Yalain as the character chosen, where its recipe
    /// starts it, starting Yalain first when it does not run. A client of that kind playing that
    /// character already, as its recipe makes it, goes on: the character is made a hero or no
    /// longer one when that changed, and sent through portal space to its start. Otherwise the
    /// client playing goes, and the character is made, given its build or put at its start (once
    /// it has left the world, when it was in it), then played.
    pub fn play(&mut self, retail: bool) {
        let recipe = self.recipe();
        let name = recipe.name.clone();
        let setup = self.setup();
        let same = matches!(characters::making(&setup.folder, &recipe), Making::Same(_));
        if same
            && self.live.up()
            && self.live.playing_as().as_deref() == Some(name.as_str())
            && self.live.retail() == retail
        {
            let entry = self.entry_for(&recipe);
            // Its hero, when that changed: asked in the world.
            if let Err(e) = self.live.prepare(&setup, &recipe, None) {
                self.status = e;
                return;
            }
            self.status = match entry {
                Err(e) => e,
                Ok(Some(to)) => {
                    self.live.teleport(&name, to);
                    format!("{name} goes to its start")
                }
                Ok(None) => format!("{name} plays already"),
            };
            return;
        }
        let was_in = self.live.in_world(&name);
        self.live.stop_client();
        if !self.ensure_serving() || !self.prepare(&recipe) {
            return;
        }
        self.live.play_when_ready(&name, retail);
        self.status = if was_in || self.live.in_world(&name) {
            format!("{name} plays once it has left the world and Yalain has it ready")
        } else {
            format!("{name} plays once Yalain has it ready")
        };
    }

    /// Send the character playing on the editor's own Yalain to the camera, through portal space.
    pub fn teleport_to_camera(&mut self) {
        let Some(name) = self.live.playing_as() else {
            self.status = "no character plays: nothing to send".into();
            return;
        };
        let to = self.entry_point();
        self.live.teleport(&name, to);
        self.status = format!("{name} goes to the camera");
    }

    /// Start the server again on the world as it stands, the character where it stands.
    pub fn restart(&mut self) {
        let setup = self.setup();
        match self.live.restart(&setup, self.session.files()) {
            Ok(()) => {
                self.published = self.session.revision();
                self.status = "Yalain starts again, the character where it stands".into();
            }
            Err(e) => self.status = e,
        }
    }

    /// Stop the client and the server, keeping the character where it stands.
    pub fn stop_playing(&mut self) {
        self.live.stop_all();
        self.status = "Yalain stopped, its characters kept".into();
    }

    /// Take a character back to its recipe. While Yalain runs it is made anew now (deleted at
    /// once, taken out of the world first, then made at its recipe's start), and a client that
    /// played it plays it again; otherwise its next play makes it anew.
    pub fn reset_character(&mut self, name: &str) {
        let setup = self.setup();
        if !self.live.serving() {
            self.status = match characters::unmark(&setup.folder, name) {
                Ok(()) => format!("{name} is made from its recipe at its next play"),
                Err(e) => e,
            };
            return;
        }
        let Some(recipe) = self.recipes.iter().find(|r| r.name == name).cloned() else {
            self.status = format!("no character {name}");
            return;
        };
        let replay = (self.live.playing_as().as_deref() == Some(name)).then(|| self.live.retail());
        if replay.is_some() {
            self.live.stop_client();
        }
        let entry = match self.entry_for(&Recipe {
            start: match recipe.start {
                // Made anew, it has left nowhere: it starts at the camera.
                Start::Left => Start::Camera,
                ref s => s.clone(),
            },
            ..recipe.clone()
        }) {
            Ok(e) => e,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if let Err(e) = self.live.reset(&setup, &recipe, entry) {
            self.status = e;
            return;
        }
        if let Some(retail) = replay {
            self.live.play_when_ready(name, retail);
        }
        self.status = format!("{name} is made from its recipe again");
    }

    /// Take a recipe away, and its character from the playtest store (at once while Yalain runs,
    /// else when it is next made under that name).
    pub fn delete_character(&mut self, name: &str) {
        if self.live.playing_as().as_deref() == Some(name) {
            self.live.stop_client();
        }
        if self.live.serving() {
            self.live.delete(name);
        }
        self.status = match characters::delete(self.session.dir(), &self.setup().folder, name) {
            Ok(()) => format!("{name} deleted"),
            Err(e) => e,
        };
        self.read_characters();
    }

    /// Write a character to a file in the world's build folder, for another world to bring in,
    /// starting Yalain first when it does not run.
    pub fn export_character(&mut self, name: &str) {
        if !self.recipes.iter().any(|r| r.name == name) {
            self.status = format!("no character {name}");
            return;
        }
        if !self.ensure_serving() {
            return;
        }
        let file = characters::export_file(self.session.dir(), name);
        self.status = match self.live.export(name, &file) {
            Ok(()) => format!("writing {name} to {}", file.display()),
            Err(e) => e,
        };
    }

    /// Bring a character from another world's file into this one as a new playtest character
    /// (under `name`, else its own), starting Yalain first when it does not run: its recipe is the
    /// build the file gives, starting where it stood, and it comes in with everything it has.
    pub fn import_character(&mut self, file: &Path, name: &str) {
        let mut r = match characters::exported(file) {
            Ok(r) => r,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if !name.trim().is_empty() {
            r.name = name.trim().to_string();
        }
        let name = r.name.clone();
        if self
            .recipes
            .iter()
            .any(|x| x.name.eq_ignore_ascii_case(&name))
        {
            self.status = format!("{name} is a character already: give it another name");
            return;
        }
        if let Err(e) = self.keep_character(&r) {
            self.status = e;
            return;
        }
        let _ = characters::reset(&self.setup().folder, &name);
        if !self.ensure_serving() {
            return;
        }
        self.live.import(&r, file);
        self.status = format!("bringing {name} in from {}", file.display());
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
                    "Yalain is up on 127.0.0.1:{}",
                    self.live.port().unwrap_or(0)
                );
            }
            Event::Moved(port) => {
                self.status = format!(
                    "another program took Yalain's ports: it starts again on 127.0.0.1:{port}"
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
                } else {
                    self.status = format!("published: {} wait for Restart", restart.join(", "));
                }
            }
            Event::Answered(Ask::Import(name), Err(why)) => {
                // Nothing came in: the recipe made for it goes.
                let _ = characters::delete(self.session.dir(), &self.setup().folder, &name);
                self.read_characters();
                self.status = format!("{name} was not brought in: {why}");
            }
            Event::Answered(Ask::Import(name), Ok(_)) => {
                self.status = format!("{name} brought in: Play plays it");
            }
            Event::Answered(Ask::Make(name) | Ask::Build(name), Err(why)) => {
                self.status = format!("Yalain did not make {name} as its recipe says: {why}");
            }
            // A delete of a name the store does not hold is no matter.
            Event::Answered(Ask::Delete(_), Err(_)) => {}
            // Nor is taking out of the world one that is not in it.
            Event::Answered(Ask::Logoff(_), Err(_)) => {}
            Event::Answered(ask, Ok(said)) => {
                self.status = match ask {
                    Ask::Export(file) => format!("written to {}", file.display()),
                    _ => said,
                };
            }
            Event::Answered(ask, Err(why)) => self.status = format!("{}: {why}", ask.command()),
            Event::Stopped(tail) => self.status = format!("Yalain stopped: {tail}"),
        }
    }

    /// The top bar's play: the character, Characters, Live, Play, To camera and Restart.
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
        if self.live.up()
            && self.live.playing()
            && ui
                .button("To camera")
                .on_hover_text("send the character in the world to the camera (T)")
                .clicked()
        {
            self.teleport_to_camera();
        }
        if serving && ui.button("Restart").clicked() {
            self.restart();
        }
    }

    /// The chosen recipe's build in the Characters window: its level and money, then its skills,
    /// items and quests, each listed with a way to take it away, and a line to add one.
    fn build_fields(&mut self, ui: &mut egui::Ui, r: &mut Recipe, quests: &[(u32, String)]) {
        egui::Grid::new("build").num_columns(2).show(ui, |ui| {
            ui.label("Level");
            match r.experience {
                Some(x) => {
                    ui.label(format!("experience {x}"))
                        .on_hover_text("its recipe gives an experience in place of a level");
                }
                None => {
                    let mut level = r.level.unwrap_or(1);
                    ui.add(egui::DragValue::new(&mut level).range(1..=999));
                    r.level = (level > 1).then_some(level);
                }
            }
            ui.end_row();
            ui.label("Money");
            let mut money = r.money.unwrap_or(0);
            ui.add(egui::DragValue::new(&mut money).range(0..=i32::MAX.unsigned_abs()));
            r.money = (money > 0).then_some(money);
            ui.end_row();
        });
        let gone = egui::Color32::from_rgb(255, 140, 120);
        ui.label(egui::RichText::new("Skills").strong());
        let mut out = None;
        for (i, s) in r.skills.iter().enumerate() {
            ui.horizontal(|ui| {
                let skill = name_in(&self.skill_names, s.skill, false);
                ui.label(match s.rating {
                    // Trained, raised no further.
                    0 => format!("{skill} trained"),
                    n => format!("{skill} to {n}"),
                });
                if ui.small_button("x").clicked() {
                    out = Some(i);
                }
            });
        }
        if let Some(i) = out.take() {
            r.skills.remove(i);
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.adding.skill)
                    .hint_text("skill: name or number")
                    .desired_width(150.0),
            );
            ui.add(egui::DragValue::new(&mut self.adding.rating).range(0..=999))
                .on_hover_text("the rating it is raised to");
            let picked = pick(&self.skill_names, &self.adding.skill);
            if ui
                .add_enabled(picked.is_some(), egui::Button::new("Add"))
                .clicked()
            {
                if let Some(skill) = picked {
                    r.skills.retain(|s| s.skill != skill);
                    r.skills.push(Skill {
                        skill,
                        rating: self.adding.rating,
                    });
                    self.adding.skill.clear();
                }
            }
        });
        if pick(&self.skill_names, &self.adding.skill).is_none() {
            ui.horizontal_wrapped(|ui| {
                for (_, w) in matching(&self.skill_names, &self.adding.skill, OFFERED) {
                    if ui.small_button(w).clicked() {
                        self.adding.skill.clone_from(w);
                    }
                }
            });
        }
        ui.label(egui::RichText::new("Items").strong());
        for (i, it) in r.items.iter().enumerate() {
            let label = self.label(it.template);
            ui.horizontal(|ui| {
                let what = if label.is_empty() {
                    format!("0x{:08X}", it.template)
                } else {
                    format!("{label} (0x{:08X})", it.template)
                };
                ui.label(format!("{what} x{}", it.quantity));
                if ui.small_button("x").clicked() {
                    out = Some(i);
                }
            });
        }
        if let Some(i) = out.take() {
            r.items.remove(i);
        }
        let typed = number(&self.adding.item);
        let typed_label = typed.map(|t| self.label(t)).unwrap_or_default();
        ui.horizontal(|ui| {
            if ui
                .small_button("<")
                .on_hover_text("the template Place puts")
                .clicked()
            {
                self.adding.item.clone_from(&self.template);
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.adding.item)
                    .hint_text("template, 0x47...")
                    .desired_width(110.0),
            );
            ui.add(egui::DragValue::new(&mut self.adding.quantity).range(1..=9999))
                .on_hover_text("how many");
            if ui
                .add_enabled(typed.is_some(), egui::Button::new("Add"))
                .clicked()
            {
                if let Some(template) = typed {
                    r.items.push(Item {
                        template,
                        quantity: self.adding.quantity,
                    });
                    self.adding.item.clear();
                }
            }
        });
        if !typed_label.is_empty() {
            ui.label(egui::RichText::new(typed_label).weak());
        }
        ui.label(egui::RichText::new("Quests").strong());
        for (i, q) in r.quests.iter().enumerate() {
            ui.horizontal(|ui| {
                let at = match q.at {
                    QuestAt::Done => "done".to_string(),
                    QuestAt::Phase(n) => format!("at phase {n}"),
                };
                ui.label(format!("{} {at}", name_in(quests, q.quest, true)));
                if ui.small_button("x").clicked() {
                    out = Some(i);
                }
            });
        }
        if let Some(i) = out.take() {
            r.quests.remove(i);
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.adding.quest)
                    .hint_text("quest: name or number")
                    .desired_width(150.0),
            );
            ui.add(egui::DragValue::new(&mut self.adding.phase).range(0..=99))
                .on_hover_text("its phase: 0 for done");
            let picked = pick(quests, &self.adding.quest);
            if ui
                .add_enabled(picked.is_some(), egui::Button::new("Add"))
                .clicked()
            {
                if let Some(quest) = picked {
                    r.quests.retain(|q| q.quest != quest);
                    r.quests.push(Quest {
                        quest,
                        at: match self.adding.phase {
                            0 => QuestAt::Done,
                            n => QuestAt::Phase(n),
                        },
                    });
                    self.adding.quest.clear();
                }
            }
        });
        if pick(quests, &self.adding.quest).is_none() {
            ui.horizontal_wrapped(|ui| {
                for (_, w) in matching(quests, &self.adding.quest, OFFERED) {
                    if ui.small_button(w).clicked() {
                        self.adding.quest.clone_from(w);
                    }
                }
            });
        }
        if !self.adding.quest.trim().is_empty() && pick(quests, &self.adding.quest).is_none() {
            let any = !matching(quests, &self.adding.quest, 1).is_empty();
            if !any {
                ui.label(egui::RichText::new("no quest of the world holds that").color(gone));
            }
        }
    }

    /// The Characters window: the recipes, a new one, the chosen one's fields, build and actions,
    /// and a character brought in from another world.
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
        let quests = match self.adding.quests.take() {
            Some(q) => q,
            None => self.quest_names(),
        };
        let folder = self.setup().folder;
        egui::Window::new("Characters")
            .open(&mut open)
            .default_size([380.0, 560.0])
            .vscroll(true)
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
                if let Some(mut r) = self
                    .recipes
                    .iter()
                    .find(|r| r.name == self.character)
                    .cloned()
                {
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
                    self.build_fields(ui, &mut r, &quests);
                    if r != before {
                        changed = Some(r.clone());
                    }
                    let reborn = characters::made(&folder, &r.name)
                        .is_some_and(|m| m.race != r.race || m.sex != r.sex);
                    ui.label(
                        egui::RichText::new(match characters::making(&folder, &r) {
                            Making::Same(_) => "made: it plays on as it was left",
                            Making::Build(_) => "build changed: given at its next play",
                            Making::Make if reborn => {
                                "race or sex changed: made anew at its next play"
                            }
                            Making::Make => "not made yet: its first play makes it",
                        })
                        .weak(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Play as").clicked() {
                            act = Some(("play", r.name.clone()));
                        }
                        if ui
                            .button("Reset")
                            .on_hover_text("make it from its recipe again")
                            .clicked()
                        {
                            act = Some(("reset", r.name.clone()));
                        }
                        if ui.button("Delete").clicked() {
                            act = Some(("delete", r.name.clone()));
                        }
                        if ui
                            .button("Export")
                            .on_hover_text(
                                "write it to build/characters in the world's folder, for another \
                                 world to bring in",
                            )
                            .clicked()
                        {
                            act = Some(("export", r.name.clone()));
                        }
                    });
                } else {
                    ui.label(egui::RichText::new("no character yet: make one with New").weak());
                }
                ui.separator();
                ui.label(egui::RichText::new("Bring in a character from another world").strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.adding.import)
                            .hint_text("its file")
                            .desired_width(170.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.adding.import_name)
                            .hint_text("its own name")
                            .desired_width(100.0),
                    );
                    if ui
                        .add_enabled(
                            !self.adding.import.trim().is_empty(),
                            egui::Button::new("Import"),
                        )
                        .clicked()
                    {
                        act = Some(("import", self.adding.import.trim().to_string()));
                    }
                });
            });
        self.adding.quests = Some(quests);
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
            Some(("export", name)) => self.export_character(&name),
            Some(("import", file)) => {
                let name = std::mem::take(&mut self.adding.import_name);
                self.import_character(Path::new(&file), &name);
                self.adding.import.clear();
            }
            _ => {}
        }
    }
}
