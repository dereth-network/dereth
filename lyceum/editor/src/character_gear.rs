//! The Characters tab's Items: the gear it is given and its kit, what it wears and wields slot by
//! slot (each rolled again on its own, or filled with a chosen weenie), the weenies it carries,
//! and what it all weighs; and where Play As puts the character.
//!
//! The gear shown is what the server says the recipe would make, before it is made (or what it
//! made, while the character is still the recipe's): rolling a slot again changes the recipe, and
//! the server rolls it again. A weenie is carried by Add, from the weenies a player can keep in a
//! pack (by the server's own checks of what a player picks up), or dragged from the spawn browser.
//!
//! The checks of what it carries run here before the server's: every weenie named is one the
//! world has, a chosen weenie can go in its slot and the character can wear or wield it, and its
//! packs and strength can carry the rest.

use dereth_primitives::{DataId, Vec3};
use dereth_progression::check::{Field, Finding};
use dereth_progression::gear::{self, Requirement, Slot};
use dereth_progression::load::{self, Carried};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::sql_ids::WeenieFacts;

use super::{EditorApp, Selection, UiAction};
use crate::characters::{GearChoice, Recipe, RecipeItem, RecipeSlot, Shown, Start};
use crate::text::Table;

/// How many of a stackable weenie a character is given at once.
const GIVE_COUNT: u32 = 20;

/// How many weenies the Add picker lists at once.
const PICKER_ROWS: usize = 60;

/// The weenie types the server makes creatures of: creatures, administrators, vendors, cows,
/// sentinels, game pieces and pets.
const CREATURES: [u32; 8] = [10, 11, 12, 15, 41, 61, 69, 71];

/// The weenie types that stand where they are put whatever their weenie says: portals (a house's
/// too), hot spots, corpses, doors, pressure plates, lifestones, switches, houses, their slum
/// lords, hooks, storage and boot spots, and allegiance bindstones.
const FIXED: [u32; 14] = [7, 13, 14, 19, 24, 25, 26, 53, 55, 56, 57, 58, 59, 65];

/// Whether a player can keep weenie `f` in a pack, by the server's own checks of what a player
/// picks up: nothing stuck where it stands, and no creature. Things that stand where they are put
/// (a portal, a door and the like) and generators are left out even when their weenie forgets to
/// say it is stuck.
#[must_use]
pub fn keepable(f: &WeenieFacts) -> bool {
    f.stuck != Some(true)
        && !f.generates
        && f.kind
            .is_none_or(|k| !CREATURES.contains(&k) && !FIXED.contains(&k))
}

/// How far in front of the selected object Play As puts the character, in meters.
const IN_FRONT: f32 = 2.0;

/// A weenie as a recipe names it: a pack weenie by its symbol, a server class by its number.
#[must_use]
pub fn written(w: &Weenie) -> String {
    match w {
        Weenie::Class(c) => c.to_string(),
        Weenie::Symbol(s) => s.clone(),
    }
}

/// The weenie a recipe names as `w`.
fn weenie_of(w: &str) -> Weenie {
    let w = w.trim();
    w.parse()
        .map_or_else(|_| Weenie::Symbol(w.to_owned()), Weenie::Class)
}

/// Each wield requirement weenie `f` states.
fn requirements(f: &WeenieFacts) -> Vec<Requirement> {
    f.wield
        .chunks(3)
        .filter(|r| r[0] != 0)
        .map(|r| Requirement {
            kind: r[0],
            of: r[1],
            at: r[2],
        })
        .collect()
}

/// The kit row of the tab: the recipe's kit, by its archetype's unless it names one.
pub(super) fn kit_choice(ui: &mut egui::Ui, r: &mut Recipe, t: &Table) {
    let kits = crate::kits::builtin();
    let id = r.kit_id();
    let name = |id: &str| {
        kits.iter().find(|k| k.id == id).map_or_else(
            || t.get("character_gear.kit_none").to_owned(),
            |k| k.name.clone(),
        )
    };
    egui::ComboBox::from_id_salt("character-kit")
        .selected_text(name(&id))
        .show_ui(ui, |ui| {
            for k in kits {
                if ui.selectable_label(id == k.id, &k.name).clicked() {
                    r.kit = Some(k.id.clone());
                }
            }
            if ui
                .selectable_label(id == crate::kits::NONE, t.get("character_gear.kit_none"))
                .clicked()
            {
                r.kit = Some(crate::kits::NONE.to_owned());
            }
        });
    // Back to the archetype's own when it is the one chosen.
    let mut by_archetype = r.clone();
    by_archetype.kit = None;
    if r.kit.as_deref() == Some(by_archetype.kit_id().as_str()) {
        r.kit = None;
    }
}

/// The start row of the tab: where Play As puts the character; the dungeon's entrance only in a
/// dungeon.
pub(super) fn start_choice(ui: &mut egui::Ui, start: &mut Start, t: &Table, in_dungeon: bool) {
    egui::ComboBox::from_id_salt("character-start")
        .selected_text(t.get(start.key()))
        .show_ui(ui, |ui| {
            for s in Start::ALL {
                let on = s != Start::Entrance || in_dungeon;
                if ui
                    .add_enabled(on, egui::Button::selectable(*start == s, t.get(s.key())))
                    .on_disabled_hover_text(t.get("character_gear.no_dungeon"))
                    .clicked()
                {
                    *start = s;
                }
            }
        });
}

/// The gear choices the tab offers, in its order, with their words' keys.
pub(super) const GEAR_CHOICES: [(GearChoice, &str); 5] = [
    (GearChoice::Level, "characters.gear_level"),
    (GearChoice::Better, "character_gear.gear_better"),
    (GearChoice::Tier, "characters.gear_tier"),
    (GearChoice::Starter, "character_gear.gear_starter"),
    (GearChoice::None, "characters.gear_none"),
];

impl EditorApp {
    /// The tier of loot the selected recipe's level hunts at, from the server's creatures and the
    /// tiers they drop (worked out again as more of the server's weenies are known).
    pub(super) fn level_tier(&mut self) -> Option<u32> {
        let weenies = self.session.server_weenies();
        if weenies.is_empty() {
            return None;
        }
        if self
            .characters
            .level_tiers
            .is_none_or(|(n, _)| n != weenies.len())
        {
            let creatures: Vec<(u32, u32)> = weenies
                .values()
                .filter(|f| f.kind == Some(10))
                .filter_map(|f| {
                    let level = u32::try_from(f.level?).ok()?;
                    let tier =
                        u32::try_from(self.session.treasure(f.death_treasure?)?.tier).ok()?;
                    Some((level, tier))
                })
                .collect();
            self.characters.level_tiers = Some((weenies.len(), gear::level_medians(creatures)));
        }
        let (_, medians) = self.characters.level_tiers?;
        Some(gear::tier_for_level(&medians, self.characters.recipe.level))
    }

    /// The tier the selected recipe's gear is rolled at, when the editor can tell before the
    /// server makes it: for its level, the higher of its level's and the place's.
    pub fn gear_tier(&mut self) -> Option<u32> {
        let place = self.characters.place.and_then(|p| p.tier);
        let r = &self.characters.recipe;
        let gear = match r.gear {
            GearChoice::None | GearChoice::Starter => return None,
            GearChoice::Tier => return r.tier,
            GearChoice::Level => dereth_progression::order::Gear::Level,
            GearChoice::Better => dereth_progression::order::Gear::Better,
        };
        let own = self.level_tier().or(place)?;
        Some(gear::tier(gear, place, own))
    }

    /// What the SQL says of the weenie a recipe names as `w`: a pack's weenie, a pack's change
    /// of a server class, or the server's own (kept until the checks are worked out again).
    fn facts_of(&mut self, w: &str) -> Option<WeenieFacts> {
        if let Some(f) = self.characters.facts.get(w) {
            return f.clone();
        }
        let f = match weenie_of(w) {
            Weenie::Class(c) => self.session.class_facts(c),
            Weenie::Symbol(s) => self.session.pack_weenies().remove(&s),
        };
        self.characters.facts.insert(w.to_owned(), f.clone());
        f
    }

    /// The name of the weenie a recipe names as `w`, else `w` as written.
    fn written_name(&self, w: &str) -> String {
        self.weenie_entry(&weenie_of(w))
            .map_or_else(|| w.trim().to_owned(), |e| e.name.clone())
    }

    /// Whether the world has the weenie a recipe names as `w`: a pack weenie its packs make, or
    /// a server class the server's database or a pack has (any, before the database is read).
    fn world_has(&self, w: &str) -> bool {
        match weenie_of(w) {
            Weenie::Class(c) => {
                self.session.server_weenies().is_empty()
                    || self.session.server_weenies().contains_key(&c)
                    || self.weenie_entry(&Weenie::Class(c)).is_some()
            }
            s @ Weenie::Symbol(_) => self.weenie_entry(&s).is_some(),
        }
    }

    /// The items the selected recipe's kit gives at its level: those the world has.
    pub(super) fn kit_items(&self) -> Vec<dereth_progression::order::Item> {
        self.characters
            .recipe
            .kit_items(|c| self.world_has(&c.to_string()))
    }

    /// What the checks find of what the selected recipe's character wears and carries, worked out
    /// again when the recipe changes or more of the server's weenies are known.
    pub fn gear_findings(&mut self) -> Vec<Finding> {
        let known = self.session.server_weenies().len();
        if let Some((r, n, found)) = &self.characters.gear_checked {
            if *r == self.characters.recipe && *n == known {
                return found.clone();
            }
        }
        let found = self.work_out_gear_findings();
        self.characters.gear_checked = Some((self.characters.recipe.clone(), known, found.clone()));
        found
    }

    fn work_out_gear_findings(&mut self) -> Vec<Finding> {
        self.characters.facts.clear();
        let r = self.characters.recipe.clone();
        let kit: Vec<RecipeItem> = self
            .kit_items()
            .into_iter()
            .map(|i| RecipeItem {
                weenie: i.class.to_string(),
                count: i.count,
            })
            .collect();
        // Everything the recipe names: whether the world has it, what its SQL says, its name.
        let mut named = std::collections::BTreeMap::new();
        for w in r
            .slots
            .values()
            .filter_map(|s| s.weenie.clone())
            .chain(kit.iter().chain(&r.items).map(|i| i.weenie.clone()))
        {
            let has = self.world_has(&w);
            let facts = self.facts_of(&w);
            let name = self.written_name(&w);
            named.insert(w, (has, facts, name));
        }
        let build = self
            .characters
            .build()
            .and_then(|b| b.as_ref().ok())
            .cloned();
        let t = self.text.table();
        let tables = self
            .characters
            .tables
            .as_ref()
            .map(crate::characters::OwnedTables::tables);
        let mut out = Vec::new();
        let mut refuse = |field, text| {
            out.push(Finding {
                field,
                text,
                warning: false,
            });
        };

        // The chosen gear: each one the world has, that goes in its slot, alone there, and that
        // the character can wear or wield.
        let mut covered: Vec<(u32, &str)> = Vec::new();
        for (slot, s) in &r.slots {
            let Some((has, facts, what)) = s.weenie.as_ref().and_then(|w| named.get(w)) else {
                continue;
            };
            if !has {
                refuse(
                    Field::Gear,
                    t.fill("character_gear.no_weenie", &[("what", what)]),
                );
                continue;
            }
            let Some(f) = facts else {
                continue;
            };
            let Some(at) = f.locations.and_then(|l| slot.location(l)) else {
                let place = t
                    .get(&format!("character_gear.slot_{}", slot.word()))
                    .to_owned();
                refuse(
                    Field::Gear,
                    t.fill(
                        "character_gear.cannot_wear",
                        &[("what", what), ("slot", &place)],
                    ),
                );
                continue;
            };
            if let Some((_, other)) = covered.iter().find(|(c, _)| c & at != 0) {
                refuse(
                    Field::Gear,
                    t.fill(
                        "character_gear.both_take",
                        &[("what", what), ("other", other)],
                    ),
                );
            }
            covered.push((at, what));
            if let (Some(b), Some(x)) = (&build, &tables) {
                let unmet: Vec<String> = requirements(f)
                    .iter()
                    .filter(|q| !gear::meets(b, x, q))
                    .map(|q| gear::requirement_text(x, q))
                    .collect();
                if !unmet.is_empty() {
                    refuse(
                        Field::Gear,
                        t.fill(
                            "character_gear.needs",
                            &[("what", what), ("needs", &unmet.join(", "))],
                        ),
                    );
                }
            }
        }

        // What it carries: its kit and the weenies given it, in its packs and on its back.
        let mut carried = Vec::new();
        for i in kit.iter().chain(&r.items) {
            let Some((has, facts, what)) = named.get(&i.weenie) else {
                continue;
            };
            if !has {
                refuse(
                    Field::Items,
                    t.fill("character_gear.no_weenie", &[("what", what)]),
                );
                continue;
            }
            let Some(f) = facts else {
                continue;
            };
            let stack = f
                .max_stack
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 1);
            let burden = if stack.is_some() {
                f.unit_burden.or(f.burden)
            } else {
                f.burden
            };
            carried.push(Carried {
                name: what.clone(),
                burden: burden.and_then(|b| u32::try_from(b).ok()).unwrap_or(0),
                count: i.count.max(1),
                stack: stack.unwrap_or(1),
                holds: if f.kind == Some(21) {
                    f.holds.and_then(|h| u32::try_from(h).ok()).unwrap_or(0)
                } else {
                    0
                },
            });
        }
        if let Some(b) = &build {
            let held = |p: u32| {
                b.held
                    .get(&p)
                    .and_then(|n| u32::try_from(*n).ok())
                    .unwrap_or(0)
            };
            // The carrying capacity and pack augmentations, bought or the heritage's.
            let mut l = load::load(b.attribute(1), held(230), 0, &carried);
            l.extra_packs = held(229);
            out.extend(load::findings(&l));
        }
        out
    }

    /// Give weenie `w` to the selected recipe's character: `count` more of it, carried from the
    /// next Play As.
    ///
    /// # Errors
    /// A recipe that will not save.
    pub fn give_to_character(&mut self, w: &Weenie, count: u32) -> Result<(), String> {
        let weenie = written(w);
        let items = &mut self.characters.recipe.items;
        match items.iter_mut().find(|i| i.weenie == weenie) {
            Some(i) => i.count = i.count.saturating_add(count.max(1)),
            None => items.push(RecipeItem {
                weenie: weenie.clone(),
                count: count.max(1),
            }),
        }
        self.characters.save()?;
        let what = self.written_name(&weenie);
        self.panels.message = self.say_with(
            "character_gear.given",
            &[("what", &what), ("name", &self.characters.name.clone())],
        );
        Ok(())
    }

    /// Carry weenie `w`: some of it when it stacks, else one; said when it would not save.
    pub(super) fn carry(&mut self, w: &Weenie) {
        let stacks = self
            .facts_of(&written(w))
            .and_then(|f| f.max_stack)
            .is_some_and(|n| n > 1);
        let n = if stacks { GIVE_COUNT } else { 1 };
        if let Err(e) = self.give_to_character(w, n) {
            self.characters.error = Some(e);
        }
    }

    /// The Items tab: the gear it is given and its kit; then what it wears and wields, slot by
    /// slot, as the server says the recipe would make it (or made it); the kit's items; the
    /// weenies it carries, with Add; and what it all weighs.
    #[allow(clippy::too_many_lines)]
    pub(super) fn character_items_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let tier = self.gear_tier();
        let order = self.character_order();
        let live = self.live.is_some();
        let shown = match &order {
            Ok(o) => match self.characters.shown(Some(o), live) {
                Shown::Made(m) => (Some(m.clone()), None),
                Shown::Preview(m) => (Some(m.clone()), None),
                Shown::Asking(m) => (m.cloned(), Some((false, t.get("character_gear.rolling")))),
                Shown::Refused(e) => (None, Some((true, e))),
                Shown::NoServer => (None, Some((false, t.get("character_gear.needs_live")))),
            },
            Err(e) => (None, Some((true, e.as_str()))),
        };
        let (made, note) = (
            shown.0,
            shown.1.map(|(fault, words)| (fault, words.to_owned())),
        );
        let mut r = self.characters.recipe.clone();
        // What is shown, worked out first: the names of what the recipe names, the weenie
        // chosen in the spawn browser and the slots it can go in, and the kit.
        let item_names: Vec<String> = r
            .items
            .iter()
            .map(|i| self.written_name(&i.weenie))
            .collect();
        let slot_names: std::collections::BTreeMap<Slot, String> = r
            .slots
            .iter()
            .filter_map(|(s, x)| Some((*s, self.written_name(x.weenie.as_deref()?))))
            .collect();
        let chosen = self.form_weenie().map(|w| {
            let name = self
                .weenie_entry(&w)
                .map_or_else(|| written(&w), |e| e.name.clone());
            let places = self
                .facts_of(&written(&w))
                .and_then(|f| f.locations)
                .map(|l| {
                    Slot::ALL
                        .into_iter()
                        .filter(|s| s.location(l).is_some())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (w, name, places)
        });
        let kit: Vec<String> = self
            .kit_items()
            .iter()
            .map(|i| {
                let name = self.written_name(&i.class.to_string());
                if i.count > 1 {
                    format!("{name} ×{}", i.count)
                } else {
                    name
                }
            })
            .collect();

        // The gear it is given, and its kit.
        egui::Grid::new("character-items-grid")
            .min_col_width(super::characters_tab::LABEL_WIDTH)
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(t.get("characters.gear"));
                ui.horizontal(|ui| {
                    let label = GEAR_CHOICES
                        .iter()
                        .find(|(g, _)| *g == r.gear)
                        .map_or("", |(_, key)| t.get(key));
                    egui::ComboBox::from_id_salt("character-gear")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (g, key) in GEAR_CHOICES {
                                ui.selectable_value(&mut r.gear, g, t.get(key));
                            }
                        });
                    match r.gear {
                        GearChoice::Tier => {
                            let mut tier = r.tier.unwrap_or(1);
                            ui.add(egui::DragValue::new(&mut tier).range(1..=8));
                            r.tier = Some(tier);
                        }
                        GearChoice::Level | GearChoice::Better => {
                            if let Some(tier) = tier {
                                ui.weak(t.fill("characters.tier_n", &[("n", &tier)]));
                            }
                        }
                        GearChoice::None | GearChoice::Starter => {}
                    }
                    if !matches!(r.gear, GearChoice::None | GearChoice::Starter)
                        && ui.button(t.get("character_gear.roll_all")).clicked()
                    {
                        r.seed = r.seed.wrapping_add(1);
                    }
                });
                ui.end_row();
                ui.label(t.get("character_gear.kit"));
                kit_choice(ui, &mut r, &t);
                ui.end_row();
            });
        if let Some(m) = &made {
            let worn = m.items.iter().filter(|i| i.slot != 0).count();
            // Loot says its tier; starter gear and none have none.
            let key = if m.tier == 0 {
                "character_gear.items_made_untiered"
            } else {
                "character_gear.items_made"
            };
            ui.label(t.fill(
                key,
                &[
                    ("worn", &worn),
                    ("carried", &(m.items.len() - worn)),
                    ("tier", &m.tier),
                ],
            ));
        }
        match &note {
            Some((true, words)) => {
                ui.colored_label(super::panels::mark::FAULT, words);
            }
            Some((false, words)) => {
                ui.horizontal(|ui| {
                    ui.weak(words);
                    if !live && ui.button(t.get("character_gear.start_live")).clicked() {
                        out.push(UiAction::Live(true));
                    }
                });
            }
            None => {}
        }
        ui.separator();

        let mut dropped_on: Option<(Slot, Weenie)> = None;
        let mut given: Option<Weenie> = None;
        let mut carried_rect = None;
        egui::ScrollArea::vertical()
            .id_salt("character-items-list")
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Grid::new("character-gear-slots")
                    .num_columns(3)
                    .striped(true)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for slot in Slot::ALL {
                            let item = made
                                .as_ref()
                                .and_then(|m| m.items.iter().find(|i| i.gear == Some(slot)));
                            let picked = slot_names.get(&slot);
                            if item.is_none() && picked.is_none() {
                                continue;
                            }
                            ui.label(t.get(&format!("character_gear.slot_{}", slot.word())));
                            let words = match (picked, item) {
                                (Some(name), _) => {
                                    format!("{name} ({})", t.get("character_gear.chosen"))
                                }
                                (None, Some(i)) if i.wield.is_empty() => i.name.clone(),
                                (None, Some(i)) => format!("{} ({})", i.name, i.wield),
                                (None, None) => String::new(),
                            };
                            let row = ui.label(words);
                            if let Some(w) = row.dnd_release_payload::<Weenie>() {
                                dropped_on = Some((slot, (*w).clone()));
                            }
                            ui.horizontal(|ui| {
                                if picked.is_some() {
                                    if ui.small_button(t.get("character_gear.put_back")).clicked() {
                                        if let Some(s) = r.slots.get_mut(&slot) {
                                            s.weenie = None;
                                        }
                                    }
                                } else if ui
                                    .small_button(t.get("character_gear.roll_again"))
                                    .clicked()
                                {
                                    r.slots.entry(slot).or_default().roll += 1;
                                }
                                if let Some((w, name, places)) = &chosen {
                                    if places.contains(&slot)
                                        && ui
                                            .small_button(t.fill(
                                                "character_gear.use_chosen",
                                                &[("name", name)],
                                            ))
                                            .clicked()
                                    {
                                        r.slots.entry(slot).or_default().weenie = Some(written(w));
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                // The chosen weenie into a slot that has no row yet.
                if let Some((w, name, places)) = &chosen {
                    let open: Vec<Slot> = places
                        .iter()
                        .copied()
                        .filter(|s| {
                            !slot_names.contains_key(s)
                                && made
                                    .as_ref()
                                    .is_none_or(|m| !m.items.iter().any(|i| i.gear == Some(*s)))
                        })
                        .collect();
                    if !open.is_empty() {
                        egui::ComboBox::from_id_salt("character-wear-as")
                            .selected_text(t.fill("character_gear.wear_as", &[("name", name)]))
                            .show_ui(ui, |ui| {
                                for s in open {
                                    if ui
                                        .selectable_label(
                                            false,
                                            t.get(&format!("character_gear.slot_{}", s.word())),
                                        )
                                        .clicked()
                                    {
                                        r.slots.entry(s).or_default().weenie = Some(written(w));
                                    }
                                }
                            });
                    }
                }
                if !kit.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                t.fill("character_gear.kit_line", &[("items", &kit.join(", "))]),
                            )
                            .weak(),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.strong(t.get("character_gear.carried"));
                    given = self.carried_picker(ui);
                });
                let (zone, dropped) =
                    ui.dnd_drop_zone::<Weenie, _>(egui::Frame::group(ui.style()), |ui| {
                        ui.set_min_width(ui.available_width());
                        if r.items.is_empty() {
                            ui.weak(t.get("character_gear.drop_here"));
                        }
                        let mut gone = None;
                        egui::Grid::new("character-carried")
                            .num_columns(3)
                            .spacing([8.0, 2.0])
                            .show(ui, |ui| {
                                for (i, (item, name)) in
                                    r.items.iter_mut().zip(&item_names).enumerate()
                                {
                                    ui.label(name);
                                    ui.add(egui::DragValue::new(&mut item.count).range(1..=10_000));
                                    if ui.small_button("×").clicked() {
                                        gone = Some(i);
                                    }
                                    ui.end_row();
                                }
                            });
                        if let Some(i) = gone {
                            r.items.remove(i);
                        }
                    });
                carried_rect = Some(zone.response.rect);
                if let Some(w) = dropped {
                    given = Some((*w).clone());
                }
                if let Some(m) = &made {
                    ui.weak(t.fill(
                        "character_gear.burden",
                        &[("burden", &m.burden), ("capacity", &m.capacity)],
                    ));
                }
            });
        self.characters.carried_rect = carried_rect;
        r.slots.retain(|_, s| *s != RecipeSlot::default());
        if let Some((slot, w)) = dropped_on {
            r.slots.entry(slot).or_default().weenie = Some(written(&w));
        }
        if r != self.characters.recipe {
            self.characters.recipe = r;
            out.push(UiAction::RecipeChanged);
        }
        if let Some(w) = given {
            self.carry(&w);
        }
    }

    /// The weenies a player can keep in a pack whose name, id or category holds `filter` (lower
    /// case): those the Add picker lists, in the spawn browser's order.
    #[must_use]
    pub fn keepable_weenies(&self, filter: &str) -> Vec<usize> {
        (0..self.weenies.len())
            .filter(|i| self.weenies[*i].keepable && self.weenies[*i].matches(filter))
            .collect()
    }

    /// Add: a button opening the weenies a player can keep in a pack, searched by name or id,
    /// each with its picture: the one clicked.
    fn carried_picker(&mut self, ui: &mut egui::Ui) -> Option<Weenie> {
        use egui::containers::menu::{MenuButton, MenuConfig};
        let t = self.text.table();
        let mut chosen = None;
        let config =
            MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        let (r, _) = MenuButton::new(t.get("character_gear.add"))
            .config(config)
            .ui(ui, |ui| {
                ui.set_min_width(300.0);
                super::panels::search_box(
                    ui,
                    &mut self.characters.picker_search,
                    t.get("collections.search"),
                );
                let f = self.characters.picker_search.trim().to_lowercase();
                let list = self.keepable_weenies(&f);
                if list.is_empty() {
                    ui.weak(t.get("collections.no_match"));
                }
                let mut budget = super::PICTURES_A_FRAME;
                egui::ScrollArea::vertical()
                    .id_salt("character-carried-picker")
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for i in list.into_iter().take(PICKER_ROWS) {
                            let e = self.weenies[i].clone();
                            let picture =
                                e.setup.and_then(|s| self.picture(DataId(s), &mut budget));
                            ui.horizontal(|ui| {
                                match picture {
                                    Some(id) => {
                                        ui.add(egui::Image::new((id, egui::vec2(24.0, 24.0))));
                                    }
                                    None => {
                                        let _ = ui.allocate_exact_size(
                                            egui::vec2(24.0, 24.0),
                                            egui::Sense::hover(),
                                        );
                                    }
                                }
                                let r = ui.selectable_label(false, &e.name);
                                ui.weak(super::form::weenie_label(&e.weenie, &t));
                                self.characters.picker_rows.push((e.weenie.clone(), r.rect));
                                if r.clicked() {
                                    chosen = Some(e.weenie.clone());
                                    ui.close();
                                }
                            });
                        }
                    });
            });
        self.characters.add_button = Some(r.rect);
        if chosen.is_some() {
            self.characters.picker_search.clear();
        }
        chosen
    }

    /// The selected object's landblock and its frame there.
    fn selected_frame(&self) -> Option<(u16, dereth_primitives::Frame)> {
        let s = self.selection.clone()?;
        if let Selection::Building { landblock, index } = s {
            let b = self
                .session
                .statics(landblock)?
                .buildings
                .get(index)?
                .clone();
            return Some((landblock, b.frame));
        }
        let (lb, _, f) = self.member(&s)?;
        Some((lb, f))
    }

    /// Where a character put at `at` (landblock `lb`'s meters) stands: on the floor under it in
    /// the room there (down an open shaft to its bottom), or on the ground. The cell, and its
    /// place.
    ///
    /// # Errors
    /// No room there, and a place off the landblock's ground: the game would put the character in
    /// another landblock.
    fn standing_at(&self, lb: u16, at: Vec3) -> Result<(u32, [f32; 3]), String> {
        if let Some((c, floor)) = self.session.landing(lb, at) {
            return Ok((
                (u32::from(lb) << 16) | u32::from(c),
                [at.x, at.y, floor + 0.5],
            ));
        }
        let ground = 0.0..lyceum_foundry::pick::BLOCK;
        if !ground.contains(&at.x) || !ground.contains(&at.y) {
            return Err(self.say("character_gear.no_floor"));
        }
        Ok((
            (u32::from(lb) << 16) | lyceum_foundry::edit::outdoor_cell(at.x, at.y),
            [
                at.x,
                at.y,
                self.session
                    .ground(lb, at.x, at.y)
                    .map_or(at.z, |g| g + 0.5),
            ],
        ))
    }

    /// Where Play As puts the selected recipe's character: its cell and place, or `None` for
    /// where it was left.
    ///
    /// # Errors
    /// The camera off the world, no object selected, or no dungeon to start at the entrance of.
    pub fn start_point(&mut self) -> Result<Option<(u32, [f32; 3])>, String> {
        match self.characters.recipe.start {
            Start::Left => Ok(None),
            Start::Camera => {
                let (lb, x, y) = self
                    .work_block()
                    .ok_or_else(|| self.say("messages.off_world"))?;
                let z = self.camera.position.z - lyceum_foundry::pick::block_origin(lb).z;
                self.standing_at(lb, Vec3::new(x, y, z)).map(Some)
            }
            Start::Selected => {
                let (lb, f) = self
                    .selected_frame()
                    .ok_or_else(|| self.say("character_gear.no_selection"))?;
                let at = dereth_primitives::frame::localtoglobal(&f, Vec3::new(0.0, IN_FRONT, 0.5));
                self.standing_at(lb, at).map(Some)
            }
            Start::Entrance => {
                let lb = self
                    .dungeon
                    .as_ref()
                    .map(|d| d.landblock)
                    .ok_or_else(|| self.say("character_gear.no_dungeon"))?;
                let (cell, at) = self.drop_point(lb)?;
                // Standing where the portals drop it, in the room the game puts it in there: the
                // cell a portal names need not be the room that holds its place.
                let block = u16::try_from(cell >> 16).unwrap_or(lb);
                let indoors = cell & 0xFFFF >= u32::from(lyceum_foundry::cells::FIRST_CELL);
                let landed = indoors
                    .then(|| {
                        self.session
                            .landing(block, Vec3::new(at.x, at.y, at.z + 0.5))
                    })
                    .flatten();
                Ok(Some(landed.map_or(
                    (cell, [at.x, at.y, at.z + 0.5]),
                    |(c, floor)| {
                        (
                            (u32::from(block) << 16) | u32::from(c),
                            [at.x, at.y, floor + 0.5],
                        )
                    },
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character's items)
    use super::*;

    #[test]
    fn a_weenie_is_written_as_a_recipe_names_it_and_read_back() {
        assert_eq!(written(&Weenie::Class(629)), "629");
        assert_eq!(written(&Weenie::Symbol("mine:food".into())), "mine:food");
        assert_eq!(weenie_of(" 629 "), Weenie::Class(629));
        assert_eq!(weenie_of("mine:food"), Weenie::Symbol("mine:food".into()));
    }

    #[test]
    fn a_player_keeps_in_a_pack_only_what_is_not_stuck_a_creature_a_fixture_or_a_generator() {
        let of = |kind: u32, stuck: Option<bool>| WeenieFacts {
            kind: Some(kind),
            stuck,
            ..WeenieFacts::default()
        };
        // A sword, food, a pack, a key, a lantern, and a thing the SQL says nothing of.
        for f in [
            of(6, None),
            of(18, Some(false)),
            of(21, None),
            of(22, None),
            of(29, None),
            WeenieFacts::default(),
        ] {
            assert!(keepable(&f), "{f:?}");
        }
        // A sign stuck where it stands; a creature, a vendor, a cow and a pet; a portal, a door,
        // a lifestone, a corpse and a house's hook, even when their weenies forget to say they
        // are stuck; a generator.
        let generator = WeenieFacts {
            kind: Some(1),
            generates: true,
            ..WeenieFacts::default()
        };
        for f in [
            of(1, Some(true)),
            of(10, None),
            of(12, None),
            of(15, None),
            of(69, None),
            of(7, None),
            of(19, Some(false)),
            of(25, None),
            of(14, None),
            of(56, None),
            generator,
        ] {
            assert!(!keepable(&f), "{f:?}");
        }
    }

    #[test]
    fn a_weenies_wield_requirements_are_read_three_numbers_each() {
        let f = WeenieFacts {
            wield: [7, 0, 180, 0, 0, 0, 2, 44, 300, 0, 0, 0],
            ..WeenieFacts::default()
        };
        assert_eq!(
            requirements(&f),
            [
                Requirement {
                    kind: 7,
                    of: 0,
                    at: 180
                },
                Requirement {
                    kind: 2,
                    of: 44,
                    at: 300
                }
            ]
        );
    }
}
