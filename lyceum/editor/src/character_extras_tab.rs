//! The Characters tab beyond the build. Augments: its summoning mastery and the augmentations a
//! character bought with experience, each with what its gem costs in the world, and the band of
//! what end-of-retail characters of its level held; or luminance and its auras, from level 200. Quests: its titles,
//! the quests that grant skill credits, and the quests it has solved, with the quests the place
//! checks ready to add.
//!
//! What it costs comes out of the build's own experience, so the Build tab shows what is left.
use super::{EditorApp, UiAction};
use crate::character_extras::{experience_words, quest_limits, titles_in};
use crate::characters::AugmentView;
use dereth_progression::augmentations::table;
use dereth_progression::check::{Field, Finding};
use dereth_progression::extras::{Luminous, Mastery};

/// How many titles a search lists at most.
const TITLES_LISTED: usize = 40;

/// How many of the world's quests a name typed lists at most.
const QUESTS_LISTED: usize = 20;

/// How tall, in points, the titles and quests a search finds stand before they scroll.
const FOUND_HEIGHT: f32 = 160.0;

impl EditorApp {
    /// What the extras are offered from, worked out again as the world is read: the
    /// augmentations' costs once more of the server's weenies are known, the titles once, the
    /// quests and their limits as the world is read and edited.
    pub(super) fn extras_frame(&mut self) {
        let known = self.session.server_weenies().len();
        let edits = self.session.history();
        let x = &mut self.characters.extras;
        if x.costs.is_none() || x.costs_from != known {
            x.costs = crate::character_extras::costs_of(&self.session);
            x.costs_from = known;
        }
        if x.titles.is_none() {
            x.titles = Some(titles_in(self.session.base()));
        }
        if x.quests.as_ref().is_none_or(|(n, _)| *n != known) {
            x.quests = Some((known, self.session.quest_names()));
        }
        if x.limits.as_ref().is_none_or(|(k, _)| *k != (known, edits)) {
            x.limits = Some(((known, edits), quest_limits(&self.session)));
        }
    }

    /// The quests the place round the camera checks, gathered with its creatures: in a dungeon,
    /// its own and its entrances'; outdoors, those of the landblocks round the camera.
    pub(super) fn place_quests_frame(&mut self, near: &std::collections::BTreeSet<u16>) {
        let into = self.dungeon.as_ref().map(|d| d.landblock);
        let found = self.session.quests_checked_in(near, into);
        self.characters.extras.place = Some(found);
    }

    /// The Augments tab: the augmentations bought with experience, each one's cost and how many,
    /// the heritage's free one marked; or luminance and its auras.
    pub(super) fn character_augments_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.horizontal(|ui| {
            for (view, key) in [
                (
                    AugmentView::Augmentations,
                    "character_extras.view_augmentations",
                ),
                (AugmentView::Luminance, "character_extras.view_luminance"),
            ] {
                ui.selectable_value(&mut self.characters.augment_view, view, t.get(key));
            }
        });
        ui.separator();
        let before = self.characters.recipe.clone();
        if self.characters.augment_view == AugmentView::Augmentations {
            self.character_augmentations(ui);
        } else {
            self.character_luminance(ui);
        }
        if self.characters.recipe != before {
            out.push(UiAction::RecipeChanged);
        }
    }

    /// The summoning mastery, and the augmentations bought with experience.
    fn character_augmentations(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let table = table();
        let free = self
            .characters
            .tables
            .as_ref()
            .and_then(|x| self.characters.recipe.heritage_id(&x.tables()).ok())
            .and_then(dereth_progression::heritage_augmentation);
        let costs = self.characters.extras.costs.clone();
        let level = self.characters.recipe.level;
        let recipe = &mut self.characters.recipe;
        // The summoning mastery, which a statue in Arwic sets.
        ui.horizontal(|ui| {
            ui.label(t.get("character_extras.mastery"));
            let name = |m: Option<Mastery>| {
                t.get(match m {
                    None => "character_extras.no_mastery",
                    Some(Mastery::Primalist) => "character_extras.primalist",
                    Some(Mastery::Necromancer) => "character_extras.necromancer",
                    Some(Mastery::Naturalist) => "character_extras.naturalist",
                })
            };
            egui::ComboBox::from_id_salt("character-mastery")
                .selected_text(name(recipe.summoning_mastery))
                .show_ui(ui, |ui| {
                    for m in std::iter::once(None).chain(Mastery::ALL.map(Some)) {
                        ui.selectable_value(&mut recipe.summoning_mastery, m, name(m));
                    }
                });
        });
        let bought: u32 = recipe.augmentations.values().sum();
        let spent: u64 = recipe
            .augmentations
            .iter()
            .filter_map(|(id, n)| Some(costs.as_ref()?.get(id)? * u64::from(*n)))
            .sum();
        if bought > 0 {
            ui.label(t.fill(
                "character_extras.augmentations",
                &[("n", &bought), ("xp", &experience_words(spent))],
            ));
        }
        if let Some(band) = table.band(level) {
            ui.weak(t.fill(
                "character_extras.band",
                &[
                    ("level", &level),
                    ("typical", &band.typical),
                    ("past", &band.warn_past),
                ],
            ));
        }
        if costs.is_none() {
            ui.colored_label(
                super::panels::mark::WARN,
                t.get("character_extras.costs_unknown"),
            );
        }
        // The names and the costs each in a column as wide as the widest of them all, whichever
        // rows are in view.
        let cost_words: Vec<String> = table
            .augmentation
            .iter()
            .map(|a| {
                costs
                    .as_ref()
                    .and_then(|c| c.get(&a.id))
                    .map_or_else(|| "—".to_owned(), experience_words)
            })
            .collect();
        let name_width =
            super::seen::widest(ui, table.augmentation.iter().map(|a| a.name.as_str()));
        let cost_width = super::seen::widest(ui, cost_words.iter().map(String::as_str));
        egui::ScrollArea::vertical()
            .id_salt("character-augmentations-list")
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Grid::new("character-augmentations-grid")
                    .num_columns(3)
                    .striped(true)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        // Only the rows in view are drawn.
                        let id = egui::Id::new("character-augmentation-rows");
                        let count = table.augmentation.len();
                        super::seen::grid_rows(
                            ui,
                            id,
                            count,
                            22.0,
                            |_| false,
                            |ui, i| {
                                let a = &table.augmentation[i];
                                let is_free = a.property.is_some() && a.property == free;
                                let most = a.max - u32::from(is_free);
                                let cost = costs.as_ref().and_then(|c| c.get(&a.id));
                                let mut n = recipe.augmentations.get(&a.id).copied().unwrap_or(0);
                                super::seen::name_cell(ui, &a.name, name_width);
                                ui.scope(|ui| {
                                    ui.set_min_width(cost_width);
                                    ui.weak(&cost_words[i]);
                                });
                                ui.horizontal(|ui| {
                                    let can = cost.is_some() && most > 0;
                                    let r = ui.add_enabled(
                                        can,
                                        egui::DragValue::new(&mut n).range(0..=most),
                                    );
                                    if costs.is_some() && cost.is_none() {
                                        r.on_disabled_hover_text(t.get("character_extras.no_gem"));
                                    }
                                    if is_free {
                                        ui.weak(t.get("character_extras.free"));
                                    }
                                });
                                ui.end_row();
                                if n == 0 {
                                    recipe.augmentations.remove(&a.id);
                                } else {
                                    recipe.augmentations.insert(a.id.clone(), n);
                                }
                            },
                        );
                    });
            });
    }

    /// Luminance, from level 200: whether it can earn it, its Seer, and its auras.
    fn character_luminance(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let table = table();
        let level = self.characters.recipe.level;
        let recipe = &mut self.characters.recipe;
        let early = level < table.luminance.from_level;
        if early {
            ui.weak(t.fill(
                "character_extras.luminance_later",
                &[("level", &table.luminance.from_level)],
            ));
        }
        // Below its level, only to be taken away again.
        if early && recipe.luminance.is_none() {
            return;
        }
        let mut luminous = recipe.luminance.is_some();
        ui.checkbox(&mut luminous, t.get("character_extras.luminance"))
            .on_hover_text(t.get("character_extras.luminance_about"));
        if luminous != recipe.luminance.is_some() {
            recipe.luminance = luminous.then(Luminous::default);
        }
        if early {
            return;
        }
        let Some(l) = recipe.luminance.as_mut() else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("character-luminance")
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Grid::new("character-auras")
                    .min_col_width(super::characters_tab::LABEL_WIDTH)
                    .num_columns(2)
                    .striped(true)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        ui.label(t.get("character_extras.seer"));
                        let name = l.seer.as_deref().and_then(|id| table.seer(id)).map_or_else(
                            || t.get("character_extras.no_seer").to_owned(),
                            |s| s.name.clone(),
                        );
                        egui::ComboBox::from_id_salt("character-seer")
                            .selected_text(name)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut l.seer,
                                    None,
                                    t.get("character_extras.no_seer"),
                                );
                                for s in &table.seer {
                                    ui.selectable_value(&mut l.seer, Some(s.id.clone()), &s.name);
                                }
                            });
                        ui.end_row();
                        let seer = l.seer.as_deref().and_then(|id| table.seer(id));
                        for a in &table.aura {
                            let most = table.aura_ranks(a, seer);
                            let mut n = l.auras.get(&a.id).copied().unwrap_or(0).min(most);
                            ui.label(&a.name);
                            ui.add_enabled(most > 0, egui::DragValue::new(&mut n).range(0..=most));
                            ui.end_row();
                            if n == 0 {
                                l.auras.remove(&a.id);
                            } else {
                                l.auras.insert(a.id.clone(), n);
                            }
                        }
                    });
                if let Some(past) = table.band(level).and_then(|b| b.luminance_warn_past) {
                    ui.weak(t.fill(
                        "character_extras.aura_band",
                        &[("level", &level), ("past", &past)],
                    ));
                }
            });
    }

    /// The Quests tab: the titles it holds, the first shown; the quests that grant skill
    /// credits; and its quests, each solved some number of times, with those the place checks
    /// ready to add.
    #[allow(clippy::too_many_lines)]
    pub(super) fn character_quests_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let level = self.characters.recipe.level;
        let c = &mut self.characters;
        let before = c.recipe.clone();
        let titles = c.extras.titles.as_deref().unwrap_or_default();
        let place = c.extras.place.as_deref().unwrap_or_default();
        let name_of = |id: u32| {
            titles
                .iter()
                .find(|(t, _)| *t == id)
                .map_or_else(|| id.to_string(), |(_, n)| n.clone())
        };
        egui::ScrollArea::vertical()
            .id_salt("character-quests")
            .auto_shrink(false)
            .show(ui, |ui| {
                // The titles.
                ui.strong(t.get("character_extras.titles"));
                if titles.is_empty() {
                    ui.weak(t.get("character_extras.no_titles"));
                } else {
                    let mut gone = None;
                    let mut shown = None;
                    egui::Grid::new("character-titles")
                        .num_columns(3)
                        .spacing([12.0, 2.0])
                        .show(ui, |ui| {
                            for (i, id) in c.recipe.titles.iter().enumerate() {
                                ui.label(name_of(*id));
                                if i == 0 {
                                    ui.weak(t.get("character_extras.shown"));
                                } else if ui.small_button(t.get("character_extras.show")).clicked()
                                {
                                    shown = Some(i);
                                }
                                if ui.small_button("×").clicked() {
                                    gone = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(i) = shown {
                        let id = c.recipe.titles.remove(i);
                        c.recipe.titles.insert(0, id);
                    }
                    if let Some(i) = gone {
                        c.recipe.titles.remove(i);
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut c.extras.title_search)
                            .hint_text(t.get("character_extras.find_title"))
                            .desired_width(f32::INFINITY),
                    );
                    let want = c.extras.title_search.trim().to_lowercase();
                    if !want.is_empty() {
                        egui::ScrollArea::vertical()
                            .id_salt("character-title-list")
                            .auto_shrink([false, true])
                            .max_height(FOUND_HEIGHT)
                            .show(ui, |ui| {
                                let held = c.recipe.titles.clone();
                                for (id, name) in titles
                                    .iter()
                                    .filter(|(id, n)| {
                                        n.to_lowercase().contains(&want) && !held.contains(id)
                                    })
                                    .take(TITLES_LISTED)
                                {
                                    if ui.selectable_label(false, name).clicked() {
                                        c.recipe.titles.push(*id);
                                    }
                                }
                            });
                    }
                }
                ui.separator();

                // The quests that grant skill credits.
                ui.checkbox(
                    &mut c.recipe.quest_credits,
                    t.get("character_extras.quest_credits"),
                )
                .on_hover_text(t.get("character_extras.quest_credits_about"));
                if c.recipe.quest_credits {
                    let extras = c.recipe.extras();
                    let had: Vec<String> = table()
                        .quest_credit
                        .iter()
                        .filter(|q| extras.solved(level).contains_key(&q.quest))
                        .map(|q| q.name.clone())
                        .collect();
                    if had.is_empty() {
                        ui.weak(t.get("character_extras.no_credits_yet"));
                    } else {
                        ui.weak(t.fill(
                            "character_extras.credits_from",
                            &[
                                ("n", &extras.quest_credits(level)),
                                ("which", &had.join(", ")),
                            ],
                        ));
                    }
                }
                ui.separator();

                // Its quests.
                ui.strong(t.get("character_extras.quests"));
                let mut gone = None;
                egui::Grid::new("character-quest-list")
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        for (quest, n) in &mut c.recipe.quests {
                            ui.label(quest.as_str());
                            ui.add(egui::DragValue::new(n).range(1..=1000))
                                .on_hover_text(t.get("character_extras.solves"));
                            if ui.small_button("×").clicked() {
                                gone = Some(quest.clone());
                            }
                            ui.end_row();
                        }
                    });
                if let Some(q) = gone {
                    c.recipe.quests.remove(&q);
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut c.extras.quest_name)
                            .hint_text(t.get("character_extras.quest_name"))
                            .desired_width(240.0),
                    );
                    let name = c.extras.quest_name.trim().to_owned();
                    if ui
                        .add_enabled(
                            !name.is_empty(),
                            egui::Button::new(t.get("character_extras.add_quest")),
                        )
                        .clicked()
                    {
                        c.recipe.quests.insert(name, 1);
                        c.extras.quest_name.clear();
                    }
                });
                // The world's quests that match what is typed.
                let want = c.extras.quest_name.trim().to_lowercase();
                if want.len() >= 2 {
                    let mut picked = None;
                    let world = c.extras.quests.as_ref().map_or(&[][..], |(_, q)| q);
                    egui::ScrollArea::vertical()
                        .id_salt("character-quest-found")
                        .auto_shrink([false, true])
                        .max_height(FOUND_HEIGHT)
                        .show(ui, |ui| {
                            for q in world
                                .iter()
                                .filter(|q| q.to_lowercase().contains(&want))
                                .filter(|q| !c.recipe.quests.contains_key(*q))
                                .take(QUESTS_LISTED)
                            {
                                if ui.selectable_label(false, q.as_str()).clicked() {
                                    picked = Some(q.clone());
                                }
                            }
                        });
                    if let Some(q) = picked {
                        c.recipe.quests.insert(q, 1);
                        c.extras.quest_name.clear();
                    }
                }
                ui.separator();

                // The quests the place checks that it has not solved: each a click from its
                // quests.
                ui.strong(t.get("character_extras.place_checks"));
                let held = |quest: &str| {
                    c.recipe
                        .quests
                        .keys()
                        .any(|q| q.eq_ignore_ascii_case(quest))
                };
                let open: Vec<&(String, String)> = place.iter().filter(|(q, _)| !held(q)).collect();
                if place.is_empty() {
                    ui.weak(t.get("character_extras.place_checks_none"));
                }
                let mut added = None;
                egui::Grid::new("character-place-checks")
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        for (quest, who) in open {
                            ui.label(quest.as_str());
                            ui.weak(who.as_str());
                            if ui
                                .small_button(t.get("character_extras.add_quest"))
                                .clicked()
                            {
                                added = Some(quest.clone());
                            }
                            ui.end_row();
                        }
                    });
                if let Some(q) = added {
                    c.recipe.quests.insert(q, 1);
                }
            });
        if c.recipe != before {
            out.push(UiAction::RecipeChanged);
        }
    }

    /// What the checks find of the selected recipe's titles and quests: a title the data files
    /// lack (or any, in a world without titles), and a quest the world does not register
    /// (warned of: the server stamps it all the same) or solved more often than it allows.
    pub fn extras_findings(&mut self) -> Vec<Finding> {
        self.extras_frame();
        let known = self.session.server_weenies().len();
        if let Some((r, n, found)) = &self.characters.extras.checked {
            if *r == self.characters.recipe && *n == known {
                return found.clone();
            }
        }
        let found = self.work_out_extras_findings(known > 0);
        self.characters.extras.checked =
            Some((self.characters.recipe.clone(), known, found.clone()));
        found
    }

    fn work_out_extras_findings(&self, world_known: bool) -> Vec<Finding> {
        let r = &self.characters.recipe;
        let mut out = Vec::new();
        let titles = self.characters.extras.titles.as_deref().unwrap_or_default();
        for id in &r.titles {
            if titles.is_empty() {
                out.push(Finding {
                    field: Field::Titles,
                    text: "This world's data files have no titles.".into(),
                    warning: false,
                });
                break;
            }
            if !titles.iter().any(|(t, _)| t == id) {
                out.push(Finding {
                    field: Field::Titles,
                    text: format!("The data files have no title {id}."),
                    warning: false,
                });
            }
        }
        let limits = self.characters.extras.limits.as_ref().map(|(_, l)| l);
        let extras = r.extras();
        for (quest, n) in extras.solved(r.level) {
            let key = lyceum_foundry::content_check::quest_key(&quest);
            if let Some(most) = limits.and_then(|l| l.get(&key)) {
                if *most > 0 && i64::from(n) > i64::from(*most) {
                    out.push(Finding {
                        field: Field::Quests,
                        text: format!("Quest {quest} can be solved at most {most} times."),
                        warning: false,
                    });
                }
            } else if world_known {
                out.push(Finding {
                    field: Field::Quests,
                    text: format!("The world registers no quest {quest}; it is stamped anyway."),
                    warning: true,
                });
            }
        }
        out
    }
}
