//! The Pets window: the world's kinds of pet and what each summoning skill allows, each with the
//! letter of where it came from and whether the world changed it or added it. The one chosen shows
//! every field, changed as the inspector's are, with Put back for one the world changed and Take
//! away; a new kind of pet (its effect and the template it makes) and a new skill's limit are added
//! below.

use athenaeum_foundry::fields;
use athenaeum_foundry::Layer;
use yalain_world_db::merge::item_key;
use yalain_world_db::rows::{Located, Row};

use super::fields::{control, Typing};
use super::{badge, parse_id, Editor};

/// A number typed: decimal, or hexadecimal after `0x`.
fn number(s: &str) -> Option<u32> {
    let t = s.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => t.parse().ok(),
    }
}

/// What the Pets window holds between frames.
#[derive(Debug, Clone, Default)]
pub struct Pets {
    pub open: bool,
    /// The item of the kind of pet or limit chosen.
    pub chosen: Option<String>,
    /// Its fields' text as typed, and the item and revision they are of.
    fields: Vec<(String, String)>,
    fields_of: Option<(String, u64)>,
    /// A new kind of pet's effect and template, and a new limit's skill, most and classes.
    new_pet: [String; 2],
    new_limit: [String; 3],
}

impl Editor<'_> {
    /// The Pets window's fields of the chosen kind of pet, as the last frame drew them.
    #[must_use]
    pub fn pet_fields(&self) -> &[(String, String)] {
        &self.pets.fields
    }

    /// The world's row of a kind of pet or limit, by its item.
    fn pet_row(&self, item: &str) -> Option<Located> {
        self.session
            .pets()
            .iter()
            .find(|l| item_key(l).as_deref() == Some(item))
            .cloned()
    }

    /// Add a kind of pet of the effect and template typed (data ids), and choose it.
    pub fn add_pet(&mut self, effect: &str, template: &str) {
        let (Some(e), Some(t)) = (parse_id(effect), parse_id(template)) else {
            self.status = format!(
                "a new pet needs an effect and a template id, not {effect:?} and {template:?}"
            );
            return;
        };
        let r = self.session.add_pet(e, t);
        if let Some(item) = self.report(r) {
            self.status = format!("added {item}");
            self.pets.chosen = Some(item);
        }
    }

    /// Add a skill's limit of the skill, most and classes typed, and choose it.
    pub fn add_pet_limit(&mut self, skill: &str, most: &str, classes: &str) {
        let (Some(s), Some(m), Some(c)) = (number(skill), number(most), parse_id(classes)) else {
            self.status = format!(
                "a new limit needs a skill, a most and classes, not {skill:?}, {most:?} and {classes:?}"
            );
            return;
        };
        let r = self.session.add_pet_limit(s, m, c);
        if let Some(item) = self.report(r) {
            self.status = format!("added {item}");
            self.pets.chosen = Some(item);
        }
    }

    pub(super) fn pets_window(&mut self, ctx: &egui::Context) {
        if !self.pets.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Pets")
            .open(&mut open)
            .default_size([420.0, 560.0])
            .show(ctx, |ui| {
                self.pets_list(ui);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("pet fields")
                    .show(ui, |ui| {
                        self.chosen_pet(ui);
                        ui.separator();
                        self.new_pets(ui);
                    });
            });
        self.pets.open = open;
    }

    /// Every kind of pet and limit of the world, each to choose.
    fn pets_list(&mut self, ui: &mut egui::Ui) {
        let rows: Vec<Located> = self.session.pets().to_vec();
        let mut chose = None;
        egui::ScrollArea::vertical()
            .id_salt("pets")
            .max_height(220.0)
            .show(ui, |ui| {
                for l in &rows {
                    let Some(item) = item_key(l) else { continue };
                    let (letter, words) = badge(l.row.prov());
                    let what = match &l.row {
                        Row::Pet(r) => self.label(r.template),
                        Row::PetLimit(r) => format!("{} limits", r.limits.len()),
                        _ => String::new(),
                    };
                    let mark = match self.session.pet_layer(l) {
                        Layer::Base => "",
                        Layer::Target if self.session.pet_was(&item).is_some() => " (changed)",
                        Layer::Target => " (new)",
                        Layer::Other => " (another pack's)",
                    };
                    let chosen = self.pets.chosen.as_deref() == Some(item.as_str());
                    if ui
                        .selectable_label(chosen, format!("[{letter}] {item}  {what}{mark}"))
                        .on_hover_text(words)
                        .clicked()
                    {
                        chose = Some(item);
                    }
                }
            });
        if let Some(item) = chose {
            self.pets.chosen = Some(item);
        }
    }

    /// The chosen kind of pet or limit: where it came from, its fields, Put back and Take away.
    fn chosen_pet(&mut self, ui: &mut egui::Ui) {
        let Some(item) = self.pets.chosen.clone() else {
            ui.label("Choose a kind of pet or a skill's limit above.");
            return;
        };
        let Some(l) = self.pet_row(&item) else {
            ui.label(format!("{item} is not in the world any more."));
            return;
        };
        ui.strong(&item);
        if let Row::Pet(r) = &l.row {
            let name = self.label(r.template);
            ui.label(format!("makes {name}"));
        }
        let (letter, words) = badge(l.row.prov());
        ui.label(format!("[{letter}] {words}"));
        let layer = self.session.pet_layer(&l);
        let was = self.session.pet_was(&item).is_some();
        match layer {
            Layer::Base => {
                ui.label(
                    egui::RichText::new(format!(
                        "The database's row (pack {}). A change writes this world's own row.",
                        l.pack
                    ))
                    .weak(),
                );
            }
            Layer::Other => {
                ui.label(
                    egui::RichText::new(format!(
                        "Pack {}'s row: this session edits {}.",
                        l.pack,
                        self.session.target()
                    ))
                    .weak(),
                );
            }
            Layer::Target => {
                ui.label(egui::RichText::new(format!("{}:{}", l.file, l.line)).weak());
            }
        }
        let fresh = self.pets.fields_of.as_ref() != Some(&(item.clone(), self.session.revision()));
        if fresh {
            self.pets.fields = fields::of(&l.row)
                .iter()
                .map(|f| (f.name.to_string(), fields::text(&l.row, f.name)))
                .collect();
            self.pets.fields_of = Some((item.clone(), self.session.revision()));
        }
        let mut set: Option<(String, String)> = None;
        let mut wrong = None;
        let mut typing = Typing::default();
        let mut group = "";
        let mut texts = std::mem::take(&mut self.pets.fields);
        egui::Grid::new("pet fields")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (f, (_, text)) in fields::of(&l.row).iter().zip(texts.iter_mut()) {
                    if f.group != group {
                        group = f.group;
                        ui.label(egui::RichText::new(group).strong());
                        ui.end_row();
                    }
                    ui.label(f.name).on_hover_text(f.hint);
                    match control(ui, f, text, &l.row, &mut typing) {
                        Ok(Some(v)) => set = Some((f.name.to_string(), v)),
                        Ok(None) => {}
                        Err(why) => wrong = Some(why),
                    }
                    ui.end_row();
                }
            });
        self.pets.fields = texts;
        if let Some(why) = wrong {
            self.status = why;
            self.pets.fields_of = None;
        }
        if let Some((field, value)) = set.filter(|(f, v)| fields::text(&l.row, f) != v.trim()) {
            match self
                .session
                .set(&item, &field, &fields::toml(&l.row, &field, &value))
            {
                Ok(_) => self.status = format!("{item}: {field} set"),
                Err(err) => {
                    self.status = err;
                    self.pets.fields_of = None;
                }
            }
        }
        ui.horizontal(|ui| {
            if was
                && layer == Layer::Target
                && ui
                    .button("Put back")
                    .on_hover_text("as the database has it")
                    .clicked()
            {
                match self.session.put_back(&item) {
                    Ok(()) => self.status = format!("put {item} back"),
                    Err(err) => self.status = err,
                }
            }
            if ui
                .add_enabled(layer != Layer::Other, egui::Button::new("Take away"))
                .clicked()
            {
                match self.session.remove_pet(&item) {
                    Ok(()) => {
                        self.status = format!("took {item} away");
                        self.pets.chosen = None;
                    }
                    Err(err) => self.status = err,
                }
            }
        });
    }

    /// A new kind of pet, and a new skill's limit.
    fn new_pets(&mut self, ui: &mut egui::Ui) {
        ui.strong("New kind of pet");
        let mut add = false;
        ui.horizontal(|ui| {
            for (i, hint) in ["effect", "template"].iter().enumerate() {
                ui.add(
                    egui::TextEdit::singleline(&mut self.pets.new_pet[i])
                        .hint_text(*hint)
                        .desired_width(100.0)
                        .font(egui::TextStyle::Monospace),
                );
            }
            add = ui
                .button("Add")
                .on_hover_text("the pet effect's id (or a contract's template), and what it makes")
                .clicked();
        });
        if add {
            let [effect, template] = self.pets.new_pet.clone();
            self.add_pet(&effect, &template);
        }
        ui.strong("New skill's limit");
        let mut add = false;
        ui.horizontal(|ui| {
            for (i, hint) in ["skill", "most", "classes"].iter().enumerate() {
                ui.add(
                    egui::TextEdit::singleline(&mut self.pets.new_limit[i])
                        .hint_text(*hint)
                        .desired_width(70.0)
                        .font(egui::TextStyle::Monospace),
                );
            }
            add = ui
                .button("Add")
                .on_hover_text(
                    "a summoning skill, and as many as most of pets sharing a bit of classes",
                )
                .clicked();
        });
        if add {
            let [skill, most, classes] = self.pets.new_limit.clone();
            self.add_pet_limit(&skill, &most, &classes);
        }
    }
}
