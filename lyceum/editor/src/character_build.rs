//! The Characters tab's Build and Tweak tabs. Build: the skill credits and experience, then the
//! attributes and vitals with their values, or each skill untrained, trained or specialized with
//! its value. Tweak: the innate split and the weights the ranks are spent by.
//!
//! Nothing here takes a rank or a value: a builder chooses what a player chooses, and the build
//! is worked out from it.
use super::{EditorApp, UiAction};
use crate::characters::{BuildView, SkillRow};
use dereth_progression::archetypes::Target;
use dereth_progression::{budget, Sac, ATTRIBUTE_NAMES, AUGMENTATION_ONLY_COST, VITAL_NAMES};

/// The highest weight a builder can give one thing.
const MOST_WEIGHT: u32 = 200;

/// The skills' grid, only the rows in view drawn: each one's name (in a column as wide as the
/// longest a skill usually has, a longer name cut), its choice of untrained, trained or
/// specialized, its credits and its value, every column as wide whichever rows are in view.
/// Answers the choice clicked.
fn skills_grid(ui: &mut egui::Ui, rows: &[SkillRow], t: &crate::text::Table) -> Option<(u32, Sac)> {
    let mut chosen: Option<(u32, Sac)> = None;
    let name = super::seen::widest(ui, [super::seen::LONGEST_SKILL]);
    let cost = super::seen::widest(ui, rows.iter().map(|s| s.cost.as_str()));
    egui::Grid::new("character-skills")
        .num_columns(4)
        .striped(true)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            // Only the rows in view are drawn.
            let id = egui::Id::new("character-skill-rows");
            super::seen::grid_rows(
                ui,
                id,
                rows.len(),
                22.0,
                |_| false,
                |ui, i| {
                    let s = &rows[i];
                    if s.wants > s.sac {
                        let warn = egui::RichText::new(&s.name).color(super::panels::mark::WARN);
                        super::seen::name_cell(ui, warn, name)
                            .on_hover_text(t.get("characters.no_credits"));
                    } else {
                        super::seen::name_cell(ui, &s.name, name);
                    }
                    ui.horizontal(|ui| {
                        for ((sac, key), can) in [
                            (Sac::Untrained, "characters.untrained_short"),
                            (Sac::Trained, "characters.trained_short"),
                            (Sac::Specialized, "characters.specialized_short"),
                        ]
                        .into_iter()
                        .zip(s.can)
                        {
                            let on = s.wants == sac;
                            let pick =
                                ui.add_enabled(can, egui::Button::selectable(on, t.get(key)));
                            if pick.clicked() && !on {
                                chosen = Some((s.id, sac));
                            }
                        }
                    });
                    ui.scope(|ui| {
                        ui.set_min_width(cost);
                        ui.weak(&s.cost);
                    });
                    ui.label(s.value.map(|v| v.to_string()).unwrap_or_default());
                    ui.end_row();
                },
            );
        });
    chosen
}

/// A count as a builder reads it, its thousands apart: `191,226,310,247`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    out
}

impl EditorApp {
    /// The skill rows of the selected recipe's build, worked out again when the recipe or the
    /// augmentations' costs change.
    fn skill_rows(&mut self) -> Vec<SkillRow> {
        let costs = self.characters.extras.costs.clone();
        if let Some((r, c, rows)) = &self.characters.extras.skill_rows {
            if *r == self.characters.recipe && *c == costs {
                return rows.clone();
            }
        }
        let built = self.characters.build().cloned();
        let (Some(owned), Ok(plan), Some(Ok(b))) = (
            self.characters.tables.as_ref(),
            self.characters.recipe.plan(),
            built,
        ) else {
            return Vec::new();
        };
        let x = owned.tables();
        let mut skills: Vec<&dereth_progression::SkillBuild> = b.skills.iter().collect();
        skills.sort_by(|p, q| q.sac.cmp(&p.sac).then_with(|| p.name.cmp(&q.name)));
        let rows: Vec<SkillRow> = skills
            .into_iter()
            .map(|s| {
                let free = budget::starting_class(&x, b.heritage, s.id);
                let wants = plan.wants(&x, s.id).max(free.unwrap_or(Sac::Untrained));
                let (trained, specialized) = budget::costs(&x, b.heritage, s.id);
                SkillRow {
                    id: s.id,
                    name: s.name.clone(),
                    sac: s.sac,
                    wants,
                    can: [
                        free.is_none_or(|f| f < Sac::Trained),
                        trained >= 0,
                        (0..AUGMENTATION_ONLY_COST).contains(&specialized),
                    ],
                    cost: if specialized >= AUGMENTATION_ONLY_COST {
                        format!("{trained}")
                    } else {
                        format!("{trained}/{specialized}")
                    },
                    value: (s.sac >= Sac::Trained).then(|| b.skill(&x, s.id)),
                }
            })
            .collect();
        self.characters.extras.skill_rows =
            Some((self.characters.recipe.clone(), costs, rows.clone()));
        rows
    }

    /// The Build tab: the skill credits and the experience, then the attributes and vitals or
    /// the skills.
    pub(super) fn character_build_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let Some(Ok(b)) = self.characters.build().cloned() else {
            return;
        };
        let rows = if self.characters.build_view == BuildView::Skills {
            self.skill_rows()
        } else {
            Vec::new()
        };
        let Some(owned) = self.characters.tables.as_ref() else {
            return;
        };
        let x = owned.tables();
        ui.label(t.fill(
            "characters.credits",
            &[
                ("credits", &(b.budget.credits() + b.quest_credits)),
                ("left", &b.unspent_credits),
            ],
        ));
        ui.label(t.fill(
            "characters.budget",
            &[
                ("xp", &grouped(b.budget.experience)),
                ("unspent", &grouped(b.unspent_xp)),
            ],
        ));
        ui.horizontal(|ui| {
            for (view, key) in [
                (BuildView::Attributes, "characters.attributes"),
                (BuildView::Skills, "characters.skills"),
            ] {
                ui.selectable_value(&mut self.characters.build_view, view, t.get(key));
            }
        });
        ui.separator();
        if self.characters.build_view == BuildView::Attributes {
            let vitals: Vec<u32> = dereth_progression::VITALS
                .iter()
                .map(|id| b.vital(&x, *id))
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("character-attributes-list")
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Grid::new("character-attributes")
                        .min_col_width(super::characters_tab::LABEL_WIDTH)
                        .num_columns(2)
                        .striped(true)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            for (a, n) in b.attributes.iter().zip(ATTRIBUTE_NAMES) {
                                ui.label(n);
                                ui.label((a.innate + a.ranks).to_string());
                                ui.end_row();
                            }
                            ui.label("");
                            ui.end_row();
                            for (v, n) in vitals.iter().zip(VITAL_NAMES) {
                                ui.label(n);
                                ui.label(v.to_string());
                                ui.end_row();
                            }
                        });
                });
            return;
        }
        let chosen = egui::ScrollArea::vertical()
            .id_salt("character-skills-list")
            .auto_shrink(false)
            .show(ui, |ui| skills_grid(ui, &rows, &t))
            .inner;
        if let Some((id, sac)) = chosen {
            if self
                .characters
                .recipe
                .edit_plan(|p| p.choose(&x, id, sac))
                .is_ok()
            {
                out.push(UiAction::RecipeChanged);
            }
        }
    }

    /// The Tweak tab: the innate split, each from 10 to 100 within the heritage's points, and the
    /// weights the ranks are spent by; back to the archetype's own.
    pub(super) fn character_tweak_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let Some(owned) = self.characters.tables.as_ref() else {
            return;
        };
        let x = owned.tables();
        let Ok(plan) = self.characters.recipe.plan() else {
            return;
        };
        let heritage = self.characters.recipe.heritage_id(&x).unwrap_or(1);
        let points = x
            .chargen
            .heritage_groups
            .get(&heritage)
            .map_or(330, |h| h.attribute_credits);
        let mut innate = plan.innate;
        // The weights of the attributes, the vitals and every skill the plan names.
        let mut weights: Vec<(Target, String, u32)> = Vec::new();
        for (i, n) in ATTRIBUTE_NAMES.iter().enumerate() {
            let what = Target::Attribute(i);
            weights.push((what, (*n).to_owned(), plan.weight_of(&x, what)));
        }
        for (i, n) in VITAL_NAMES.iter().enumerate() {
            let what = Target::Vital(i);
            weights.push((what, (*n).to_owned(), plan.weight_of(&x, what)));
        }
        let mut skills: Vec<u32> = plan
            .specialized
            .iter()
            .chain(&plan.trained)
            .chain(plan.weights.keys())
            .filter_map(|s| x.resolve_skill(s))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        skills.retain(|id| seen.insert(*id));
        for id in skills {
            let what = Target::Skill(id);
            weights.push((what, x.skill_name(id), plan.weight_of(&x, what)));
        }
        let before = weights.clone();
        let mut reset = false;
        let own = !self.characters.recipe.plan_is_archetypes();
        egui::ScrollArea::vertical()
            .id_salt("character-tweak")
            .auto_shrink(false)
            .show(ui, |ui| {
                // The innate split and the weights in one grid, their values lined up.
                egui::Grid::new("character-tweak-grid")
                    .min_col_width(super::characters_tab::LABEL_WIDTH)
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong(t.get("characters.innate"));
                        ui.end_row();
                        for (v, n) in innate.iter_mut().zip(ATTRIBUTE_NAMES) {
                            ui.label(n);
                            ui.add(egui::DragValue::new(v).range(10..=100));
                            ui.end_row();
                        }
                        let used: u32 = innate.iter().sum();
                        let left = i64::from(points) - i64::from(used);
                        let line =
                            t.fill("characters.innate_left", &[("n", &left), ("of", &points)]);
                        ui.label("");
                        if left == 0 {
                            ui.weak(line);
                        } else {
                            ui.colored_label(super::panels::mark::WARN, line);
                        }
                        ui.end_row();
                        ui.label("");
                        ui.end_row();
                        ui.strong(t.get("characters.shares"));
                        ui.end_row();
                        for (_, name, w) in &mut weights {
                            ui.label(name.as_str());
                            ui.add(egui::DragValue::new(w).range(0..=MOST_WEIGHT));
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                reset = own && ui.button(t.get("characters.archetypes_own")).clicked();
            });
        let changed = innate != plan.innate || weights != before;
        if changed {
            let edited = self.characters.recipe.edit_plan(|p| {
                p.innate = innate;
                for ((what, _, w), (_, _, was)) in weights.iter().zip(&before) {
                    if w != was {
                        p.set_weight(&x, *what, *w);
                    }
                }
            });
            if edited.is_ok() {
                out.push(UiAction::RecipeChanged);
            }
        }
        if reset {
            self.characters.recipe.reset_plan();
            out.push(UiAction::RecipeChanged);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the Characters tab's numbers)
    use super::*;

    /// Skill rows: short names at the top, the longest further down, and one longer still.
    fn skill_rows() -> Vec<SkillRow> {
        (0..40)
            .map(|i| SkillRow {
                id: i,
                name: match i {
                    0..20 => "Run".to_owned(),
                    39 => "Creature Enchantment and More".to_owned(),
                    _ => super::super::seen::LONGEST_SKILL.to_owned(),
                },
                sac: Sac::Trained,
                wants: Sac::Trained,
                can: [true; 3],
                cost: if i % 2 == 0 { "4/8" } else { "12/24" }.to_owned(),
                value: Some(1000 + i),
            })
            .collect()
    }

    /// The skills' grid in a window 600 by 300 points scrolled to `offset`, drawn three times
    /// with the panels' fonts: the text it painted last, each with where it starts and whether it
    /// was cut.
    fn painted(offset: f32) -> Vec<(String, f32, bool)> {
        let ctx = crate::gui::Gui::new().ctx;
        let t = crate::text::Text::built_in().table();
        let rows = skill_rows();
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 300.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                egui::ScrollArea::vertical()
                    .vertical_scroll_offset(offset)
                    .show(ui, |ui| skills_grid(ui, &rows, &t));
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(x) => Some((x.galley.job.text.clone(), x.pos.x, x.galley.elided)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_skills_columns_stand_where_they_did_as_rows_scroll_in_and_out_of_view() {
        let value_at = |painted: &[(String, f32, bool)]| -> Vec<f32> {
            painted
                .iter()
                .filter(|(text, _, _)| text.parse::<u32>().is_ok_and(|n| n >= 1000))
                .map(|(_, x, _)| *x)
                .collect()
        };
        let (top, lower) = (painted(0.0), painted(600.0));
        let (at_top, at_bottom) = (value_at(&top), value_at(&lower));
        assert!(!at_top.is_empty() && !at_bottom.is_empty());
        assert!(
            at_top
                .iter()
                .chain(&at_bottom)
                .all(|x| (x - at_top[0]).abs() < 0.5),
            "{at_top:?} {at_bottom:?}"
        );
        // The longest name fits whole; the one longer still is cut.
        let cut = |text: &str| lower.iter().find(|(t, _, _)| t == text).map(|p| p.2);
        assert_eq!(cut(super::super::seen::LONGEST_SKILL), Some(false));
        assert_eq!(cut("Creature Enchantment and More"), Some(true));
    }

    #[test]
    fn a_count_is_written_with_its_thousands_apart() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(191_226_310_247), "191,226,310,247");
    }
}
