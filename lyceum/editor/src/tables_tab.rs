//! The Tables tab: the game's tables as the world has them, changed as the pack being edited's
//! rows of them in its `records/` ([`lyceum_foundry::table_views`]). Four pages:
//!
//! - **Skills:** each skill's trained and specialized cost and its formula; a new skill, copied
//!   from another.
//! - **Heritages:** each heritage's credits, what its skills cost it at creation, and its
//!   templates (the creation screen's "classes").
//! - **Experience:** the level cap, each level's experience and skill credits, and the raise costs.
//!   A level's experience that would make the curve fall is refused as it is typed, before
//!   anything is written; a higher cap gives each level it adds its experience.
//! - **Creature types:** the names the game shows for creature types, each renamed or taken out;
//!   a new type, numbered past the highest.
//!
//! A number is typed and written when its field is left. What the pack changes is marked, and
//! "Below" takes the pack's change of it away.

use dereth_assets::tables::{CharGen, EnumMapper, SkillTable, XpTable};
use dereth_assets::Decode;
use dereth_dat::ModernDat;
use dereth_primitives::DataId;
use lyceum_foundry::edit::Op;
use lyceum_foundry::table_edit;
use lyceum_foundry::table_views::{self, ShownSkill};
use lyceum_foundry::tables::{CHARGEN, CREATURE_TYPES, EXPERIENCE, SKILLS};
use toml::Value as T;

use super::{EditorApp, Tab};

/// The tab's pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Skills,
    Heritages,
    Experience,
    CreatureTypes,
}

/// The Tables tab's state.
#[derive(Debug, Default)]
pub struct TablesView {
    pub page: Page,
    /// Text the shown skills' or heritages' names hold, case aside.
    pub filter: String,
    /// A field being typed: its id and its text, written once it is left.
    pub typing: Option<(String, String)>,
    /// A new skill's name and the skill it starts as a copy of.
    pub new_skill: (String, String),
    /// A new creature type's name.
    pub new_creature_type: String,
    /// Why the last value typed was refused, by its field.
    pub refused: Option<(String, String)>,
}

/// One row change: the records file, the row's kind, its key, and the fields it sets (`None`
/// takes one away).
type RowChange = (
    &'static str,
    &'static str,
    Vec<(&'static str, T)>,
    Vec<(&'static str, Option<T>)>,
);

fn int(v: i64) -> T {
    T::Integer(v)
}

fn txt(s: &str) -> T {
    T::String(s.to_owned())
}

impl EditorApp {
    /// Show the Tables tab: where it is, else floating over the world view.
    pub fn open_tables(&mut self) {
        self.bring_back(Tab::Tables);
        let world = self.world_rect.unwrap_or(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1400.0, 900.0),
        ));
        let size = egui::vec2(
            720.0_f32.min(world.width() - 32.0).max(320.0),
            640.0_f32.min(world.height() - 32.0).max(240.0),
        );
        let at =
            egui::Rect::from_min_size(egui::pos2(world.min.x + 80.0, world.min.y + 40.0), size);
        let dock = if self.dungeon.is_some() {
            self.dungeon_dock
                .get_or_insert_with(crate::docks::in_dungeon)
        } else {
            self.dock.get_or_insert_with(crate::docks::outdoors)
        };
        crate::docks::open_floating(dock, Tab::Tables, at);
        self.show_tab(Tab::Tables);
    }

    /// Table `id` as the world has it, its unsaved changes in.
    fn table_now<D: Decode>(&self, id: u32) -> Option<D> {
        let bytes = self
            .session
            .compiled()
            .records
            .get(&ModernDat::Portal)
            .and_then(|r| r.get(&id))
            .map(|s| s.bytes.to_vec())
            .or_else(|| self.session.base().read_portal(DataId(id)))?;
        D::decode_payload(DataId(id), &bytes).ok()
    }

    /// The pack being edited's records file `name`.
    fn records_file(&self, name: &str) -> std::path::PathBuf {
        self.session.packs()[self.session.target()]
            .dir
            .join("records")
            .join(name)
    }

    /// The pack's row of `kind` and key `key` in records file `file`, when it has one.
    fn table_row(&mut self, file: &str, kind: &str, key: &[(&str, T)]) -> Option<toml::Table> {
        let text = self.session.rows_text(&self.records_file(file));
        table_edit::row(text.as_deref(), kind, key).ok().flatten()
    }

    /// The rows `changes` written into their files, as one change of the world.
    ///
    /// # Errors
    /// A row that doesn't write, or a table that doesn't build with it.
    pub(super) fn table_change(&mut self, changes: &[RowChange]) -> Result<(), String> {
        let mut files: Vec<(std::path::PathBuf, Option<String>, Option<String>)> = Vec::new();
        for (file, kind, key, fields) in changes {
            let path = self.records_file(file);
            let at = match files.iter().position(|(p, ..)| *p == path) {
                Some(i) => i,
                None => {
                    let was = self.session.rows_text(&path);
                    files.push((path, was.clone(), was));
                    files.len() - 1
                }
            };
            files[at].2 = table_edit::set_fields(files[at].2.as_deref(), kind, key, fields)?;
        }
        let ops: Vec<Op> = files
            .into_iter()
            .filter(|(_, was, now)| was != now)
            .map(|(file, _, text)| Op::WriteTables { file, text })
            .collect();
        match ops.len() {
            0 => Ok(()),
            1 => self.apply(&ops[0], false).map(|_| ()),
            _ => self.apply(&Op::Batch(ops), false).map(|_| ()),
        }
    }

    /// A number field: its value now, typed and given back once left (and changed).
    fn number(&mut self, ui: &mut egui::Ui, id: &str, now: i64, width: f32) -> Option<i64> {
        let typing = self
            .tables
            .typing
            .as_ref()
            .filter(|(f, _)| f == id)
            .map(|(_, s)| s.clone());
        let mut s = typing.clone().unwrap_or_else(|| now.to_string());
        let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(width));
        if r.changed() {
            self.tables.typing = Some((id.to_owned(), s.clone()));
        }
        if r.lost_focus() && typing.is_some() {
            self.tables.typing = None;
            return s.trim().parse::<i64>().ok().filter(|v| *v != now);
        }
        None
    }

    /// A text field: its text now, typed and given back once left (and changed).
    fn words(&mut self, ui: &mut egui::Ui, id: &str, now: &str, width: f32) -> Option<String> {
        let typing = self
            .tables
            .typing
            .as_ref()
            .filter(|(f, _)| f == id)
            .map(|(_, s)| s.clone());
        let mut s = typing.clone().unwrap_or_else(|| now.to_owned());
        // Sized as asked: a grid's cell would squeeze it.
        let h = ui.spacing().interact_size.y;
        let r = ui
            .add_sized([width, h], egui::TextEdit::singleline(&mut s))
            .on_hover_text(now);
        if r.changed() {
            self.tables.typing = Some((id.to_owned(), s.clone()));
        }
        if r.lost_focus() && typing.is_some() {
            self.tables.typing = None;
            return (s.trim() != now).then(|| s.trim().to_owned());
        }
        None
    }

    /// The Tables tab's body.
    pub(super) fn tables_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let pack = self.session.packs()[self.session.target()].name.clone();
        ui.label(t.fill("tables.head", &[("pack", &pack)]));
        ui.horizontal(|ui| {
            for (p, key) in [
                (Page::Skills, "tables.skills"),
                (Page::Heritages, "tables.heritages"),
                (Page::Experience, "tables.experience"),
                (Page::CreatureTypes, "tables.creature_types"),
            ] {
                ui.selectable_value(&mut self.tables.page, p, t.get(key));
            }
            if self.tables.page != Page::Experience {
                ui.label(t.get("rows.find"));
                ui.add(egui::TextEdit::singleline(&mut self.tables.filter).desired_width(160.0));
            }
        });
        if let Some((_, why)) = &self.tables.refused {
            ui.colored_label(ui.visuals().warn_fg_color, why);
        }
        ui.separator();
        let Some(skills) = self.table_now::<SkillTable>(SKILLS) else {
            ui.label(t.get("tables.no_table"));
            return;
        };
        let mut change: Option<Vec<RowChange>> = None;
        match self.tables.page {
            Page::Skills => self.skills_page(ui, &t, &skills, &pack, &mut change),
            Page::Heritages => self.heritages_page(ui, &t, &skills, &pack, &mut change),
            Page::Experience => self.experience_page(ui, &t, &pack, &mut change),
            Page::CreatureTypes => self.creature_types_page(ui, &t, &pack, &mut change),
        }
        if let Some(c) = change {
            match self.table_change(&c) {
                Ok(()) => self.tables.refused = None,
                Err(why) => self.panels.message = why,
            }
        }
    }

    fn shown(&self, name: &str) -> bool {
        let want = self.tables.filter.trim().to_lowercase();
        want.is_empty() || name.to_lowercase().contains(&want)
    }

    fn skills_page(
        &mut self,
        ui: &mut egui::Ui,
        t: &crate::text::Table,
        table: &SkillTable,
        pack: &str,
        change: &mut Option<Vec<RowChange>>,
    ) {
        let file = self.records_file(table_views::SKILLS_FILE);
        let text = self.session.rows_text(&file);
        let all = table_views::skills(table);
        ui.horizontal(|ui| {
            ui.label(t.get("tables.new_skill"));
            ui.add(egui::TextEdit::singleline(&mut self.tables.new_skill.0).desired_width(120.0));
            ui.label(t.get("tables.like"));
            egui::ComboBox::from_id_salt("tables-like")
                .selected_text(self.tables.new_skill.1.clone())
                .show_ui(ui, |ui| {
                    for s in &all {
                        ui.selectable_value(&mut self.tables.new_skill.1, s.name.clone(), &s.name);
                    }
                });
            let name = self.tables.new_skill.0.trim().to_owned();
            let taken = all.iter().any(|s| table_views::same_name(&s.name, &name));
            if ui
                .add_enabled(
                    !name.is_empty() && !taken && !self.tables.new_skill.1.is_empty(),
                    egui::Button::new(t.get("tables.add")),
                )
                .clicked()
            {
                let id = table_views::free_skill(table);
                *change = Some(vec![(
                    table_views::SKILLS_FILE,
                    "skill",
                    vec![("id", int(i64::from(id)))],
                    vec![
                        ("like", Some(txt(&self.tables.new_skill.1))),
                        ("name", Some(txt(&name))),
                    ],
                )]);
                self.tables.new_skill = Default::default();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("tables-skills")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("tables-skills-grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for h in [
                            "tables.skill",
                            "tables.trained",
                            "tables.specialized",
                            "tables.formula",
                            "",
                        ] {
                            ui.strong(if h.is_empty() {
                                String::new()
                            } else {
                                t.get(h).to_owned()
                            });
                        }
                        ui.end_row();
                        let shown: Vec<&ShownSkill> =
                            all.iter().filter(|s| self.shown(&s.name)).collect();
                        for s in shown {
                            self.skill_row(ui, t, s, text.as_deref(), pack, change);
                            ui.end_row();
                        }
                    });
            });
    }

    fn skill_row(
        &mut self,
        ui: &mut egui::Ui,
        t: &crate::text::Table,
        s: &ShownSkill,
        text: Option<&str>,
        pack: &str,
        change: &mut Option<Vec<RowChange>>,
    ) {
        let key = table_views::skill_key(text, s.id, &s.name);
        let mine = table_edit::row(text, "skill", &key).ok().flatten();
        let set = |fields: Vec<(&'static str, Option<T>)>| {
            Some(vec![(
                table_views::SKILLS_FILE,
                "skill",
                key.clone(),
                fields,
            )])
        };
        ui.label(&s.name);
        let id = |f: &str| format!("skill {} {f}", s.id);
        if let Some(v) = self.number(ui, &id("trained"), i64::from(s.trained_cost), 40.0) {
            *change = set(vec![("trained_cost", Some(int(v)))]);
        }
        if let Some(v) = self.number(ui, &id("specialized"), i64::from(s.specialized_cost), 40.0) {
            *change = set(vec![("specialized_cost", Some(int(v)))]);
        }
        ui.horizontal(|ui| {
            let f = s.formula;
            let names: Vec<&str> = table_views::ATTRIBUTES.iter().map(|(_, n)| *n).collect();
            let pick = |ui: &mut egui::Ui, which: &str, now: u32| -> Option<u32> {
                let label = table_views::ATTRIBUTES
                    .iter()
                    .find(|(k, _)| *k == now)
                    .map_or("—", |(_, n)| *n);
                let mut chosen = None;
                egui::ComboBox::from_id_salt(("tables-attr", s.id, which))
                    .width(90.0)
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(now == 0, "—").clicked() && now != 0 {
                            chosen = Some(0);
                        }
                        for (k, n) in table_views::ATTRIBUTES {
                            if ui.selectable_label(now == k, n).clicked() && now != k {
                                chosen = Some(k);
                            }
                        }
                    });
                chosen
            };
            let a1 = pick(ui, "1", f.attr1);
            let a2 = pick(ui, "2", f.attr2);
            ui.label("/");
            let d = self.number(ui, &id("divisor"), i64::from(f.z), 28.0);
            if a1.is_some() || a2.is_some() || d.is_some() {
                let (a1, a2) = (a1.unwrap_or(f.attr1), a2.unwrap_or(f.attr2));
                let attrs: Vec<&str> = [a1, a2]
                    .into_iter()
                    .filter_map(|a| names.get((a as usize).wrapping_sub(1)).copied())
                    .collect();
                let mults: Vec<u32> = [(a1, f.x.max(1)), (a2, f.y.max(1))]
                    .into_iter()
                    .filter(|(a, _)| *a != 0)
                    .map(|(_, m)| m)
                    .collect();
                let divisor = d.and_then(|d| u32::try_from(d).ok()).unwrap_or(f.z).max(1);
                *change = set(vec![(
                    "formula",
                    Some(table_views::formula_row(&attrs, &mults, f.w, divisor)),
                )]);
            }
        })
        .response
        .on_hover_text(table_views::formula_words(&s.formula));
        ui.horizontal(|ui| {
            if mine.is_some() {
                ui.weak(t.fill("tables.changed_in", &[("pack", &pack)]));
                if ui
                    .small_button(t.get("tables.below"))
                    .on_hover_text(t.get("tables.below_tip"))
                    .clicked()
                {
                    let fields = mine
                        .iter()
                        .flat_map(|r| r.keys())
                        .filter(|k| !key.iter().any(|(kk, _)| kk == k))
                        .filter_map(|k| {
                            [
                                "trained_cost",
                                "specialized_cost",
                                "formula",
                                "like",
                                "name",
                                "remove",
                            ]
                            .into_iter()
                            .find(|f| f == k)
                        })
                        .map(|f| (f, None))
                        .collect();
                    *change = set(fields);
                }
            }
        });
    }

    fn creature_types_page(
        &mut self,
        ui: &mut egui::Ui,
        t: &crate::text::Table,
        pack: &str,
        change: &mut Option<Vec<RowChange>>,
    ) {
        const FILE: &str = table_views::CREATURE_TYPES_FILE;
        let Some(m) = self.table_now::<EnumMapper>(CREATURE_TYPES) else {
            ui.label(t.get("tables.no_table"));
            return;
        };
        let text = self.session.rows_text(&self.records_file(FILE));
        let all = table_views::creature_types(&m);
        ui.horizontal(|ui| {
            ui.label(t.get("tables.new_creature_type"));
            ui.add(
                egui::TextEdit::singleline(&mut self.tables.new_creature_type).desired_width(160.0),
            );
            let name = self.tables.new_creature_type.trim().to_owned();
            let taken = all.iter().any(|(_, n)| table_views::same_name(n, &name));
            if ui
                .add_enabled(
                    !name.is_empty() && !taken,
                    egui::Button::new(t.get("tables.add")),
                )
                .clicked()
            {
                let id = table_views::free_creature_type(&m);
                *change = Some(vec![(
                    FILE,
                    "creature-type",
                    vec![("id", int(i64::from(id)))],
                    vec![("name", Some(txt(&name)))],
                )]);
                self.tables.new_creature_type.clear();
            }
        });
        let removed = table_views::removed_creature_types(text.as_deref());
        if !removed.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(t.fill("tables.taken_out", &[("pack", &pack)]));
                for id in removed {
                    if ui
                        .small_button(id.to_string())
                        .on_hover_text(t.get("tables.below_tip"))
                        .clicked()
                    {
                        *change = Some(vec![(
                            FILE,
                            "creature-type",
                            vec![("id", int(i64::from(id)))],
                            vec![("remove", None)],
                        )]);
                    }
                }
            });
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("tables-creature-types")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("tables-creature-types-grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["tables.number", "tables.name", "", ""] {
                            ui.strong(if h.is_empty() {
                                String::new()
                            } else {
                                t.get(h).to_owned()
                            });
                        }
                        ui.end_row();
                        let shown: Vec<&(u32, String)> =
                            all.iter().filter(|(_, n)| self.shown(n)).collect();
                        for (id, name) in shown {
                            let key = table_views::creature_type_key(text.as_deref(), *id, name);
                            let mine = table_edit::row(text.as_deref(), "creature-type", &key)
                                .ok()
                                .flatten();
                            let set = |fields: Vec<(&'static str, Option<T>)>| {
                                Some(vec![(FILE, "creature-type", key.clone(), fields)])
                            };
                            ui.label(id.to_string());
                            if let Some(v) =
                                self.words(ui, &format!("creature-type {id} name"), name, 200.0)
                            {
                                if !v.is_empty() {
                                    *change = set(vec![("name", Some(txt(&v)))]);
                                }
                            }
                            ui.horizontal(|ui| {
                                if mine.is_some() {
                                    ui.weak(t.fill("tables.changed_in", &[("pack", &pack)]));
                                    if ui
                                        .small_button(t.get("tables.below"))
                                        .on_hover_text(t.get("tables.below_tip"))
                                        .clicked()
                                    {
                                        *change = set(vec![("name", None), ("remove", None)]);
                                    }
                                }
                            });
                            if ui.small_button(t.get("tables.take_out")).clicked() {
                                *change = set(vec![("remove", Some(T::Boolean(true)))]);
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    fn heritages_page(
        &mut self,
        ui: &mut egui::Ui,
        t: &crate::text::Table,
        skills: &SkillTable,
        pack: &str,
        change: &mut Option<Vec<RowChange>>,
    ) {
        let Some(cg) = self.table_now::<CharGen>(CHARGEN) else {
            ui.label(t.get("tables.no_table"));
            return;
        };
        let file = table_views::HERITAGES_FILE;
        let all_skills = table_views::skills(skills);
        egui::ScrollArea::vertical()
            .id_salt("tables-heritages")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let shown: Vec<_> = table_views::heritages(&cg, skills)
                    .into_iter()
                    .filter(|h| self.shown(&h.name))
                    .collect();
                // The one heritage the filter names is open.
                let one = shown.len() == 1;
                for h in shown {
                    let hk = vec![("heritage", txt(&h.name))];
                    let changed = self.table_row(file, "heritage", &hk).is_some();
                    let head = if changed {
                        format!(
                            "{} · {}",
                            h.name,
                            t.fill("tables.changed_in", &[("pack", &pack)])
                        )
                    } else {
                        h.name.clone()
                    };
                    egui::CollapsingHeader::new(head)
                        .id_salt(("tables-heritage", &h.name))
                        .open(one.then_some(true))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let id = |f: &str| format!("heritage {} {f}", h.name);
                                ui.label(t.get("tables.attribute_credits"));
                                if let Some(v) = self.number(
                                    ui,
                                    &id("attr"),
                                    i64::from(h.attribute_credits),
                                    40.0,
                                ) {
                                    *change = Some(vec![(
                                        file,
                                        "heritage",
                                        hk.clone(),
                                        vec![("attribute_credits", Some(int(v)))],
                                    )]);
                                }
                                ui.label(t.get("tables.skill_credits"));
                                if let Some(v) =
                                    self.number(ui, &id("skill"), i64::from(h.skill_credits), 40.0)
                                {
                                    *change = Some(vec![(
                                        file,
                                        "heritage",
                                        hk.clone(),
                                        vec![("skill_credits", Some(int(v)))],
                                    )]);
                                }
                                if changed && ui.small_button(t.get("tables.below")).clicked() {
                                    *change = Some(vec![(
                                        file,
                                        "heritage",
                                        hk.clone(),
                                        vec![
                                            ("attribute_credits", None),
                                            ("skill_credits", None),
                                            ("remove", None),
                                        ],
                                    )]);
                                }
                            });
                            ui.label(t.get("tables.heritage_skills"));
                            egui::Grid::new(("tables-hs", &h.name))
                                .striped(true)
                                .show(ui, |ui| {
                                    for (skill, tc, sc) in &h.skills {
                                        let key =
                                            vec![("heritage", txt(&h.name)), ("skill", txt(skill))];
                                        let id = |f: &str| format!("hs {} {skill} {f}", h.name);
                                        ui.label(skill);
                                        if let Some(v) =
                                            self.number(ui, &id("t"), i64::from(*tc), 40.0)
                                        {
                                            *change = Some(vec![(
                                                file,
                                                "heritage-skill",
                                                key.clone(),
                                                vec![("trained_cost", Some(int(v)))],
                                            )]);
                                        }
                                        if let Some(v) =
                                            self.number(ui, &id("s"), i64::from(*sc), 40.0)
                                        {
                                            *change = Some(vec![(
                                                file,
                                                "heritage-skill",
                                                key.clone(),
                                                vec![("specialized_cost", Some(int(v)))],
                                            )]);
                                        }
                                        if ui.small_button(t.get("tables.take_off")).clicked() {
                                            *change = Some(vec![(
                                                file,
                                                "heritage-skill",
                                                key.clone(),
                                                vec![("remove", Some(T::Boolean(true)))],
                                            )]);
                                        }
                                        ui.end_row();
                                    }
                                });
                            let offered: Vec<&str> =
                                h.skills.iter().map(|(s, ..)| s.as_str()).collect();
                            egui::ComboBox::from_id_salt(("tables-offer", &h.name))
                                .selected_text(t.get("tables.offer_skill"))
                                .show_ui(ui, |ui| {
                                    for s in all_skills
                                        .iter()
                                        .filter(|s| !offered.contains(&s.name.as_str()))
                                    {
                                        if ui.selectable_label(false, &s.name).clicked() {
                                            *change = Some(vec![(
                                                file,
                                                "heritage-skill",
                                                vec![
                                                    ("heritage", txt(&h.name)),
                                                    ("skill", txt(&s.name)),
                                                ],
                                                vec![
                                                    (
                                                        "trained_cost",
                                                        Some(int(i64::from(s.trained_cost))),
                                                    ),
                                                    (
                                                        "specialized_cost",
                                                        Some(int(i64::from(s.specialized_cost))),
                                                    ),
                                                ],
                                            )]);
                                        }
                                    }
                                });
                            ui.label(t.get("tables.templates"));
                            egui::Grid::new(("tables-tp", &h.name))
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label("");
                                    for a in table_views::TEMPLATE_ATTRIBUTES {
                                        ui.weak(&a[..3]);
                                    }
                                    ui.weak(t.get("tables.trained"));
                                    ui.weak(t.get("tables.specialized"));
                                    ui.end_row();
                                    for x in &h.templates {
                                        let key = vec![
                                            ("heritage", txt(&h.name)),
                                            ("name", txt(&x.name)),
                                        ];
                                        let id = |f: &str| format!("tp {} {} {f}", h.name, x.name);
                                        ui.label(&x.name);
                                        for (i, a) in
                                            table_views::TEMPLATE_ATTRIBUTES.into_iter().enumerate()
                                        {
                                            if let Some(v) = self.number(
                                                ui,
                                                &id(a),
                                                i64::from(x.attributes[i]),
                                                28.0,
                                            ) {
                                                *change = Some(vec![(
                                                    file,
                                                    "heritage-template",
                                                    key.clone(),
                                                    vec![(a, Some(int(v)))],
                                                )]);
                                            }
                                        }
                                        for (f, list) in [
                                            ("trained", &x.trained),
                                            ("specialized", &x.specialized),
                                        ] {
                                            if let Some(v) =
                                                self.words(ui, &id(f), &list.join(", "), 140.0)
                                            {
                                                let names: Vec<T> = v
                                                    .split(',')
                                                    .map(str::trim)
                                                    .filter(|n| !n.is_empty())
                                                    .map(txt)
                                                    .collect();
                                                *change = Some(vec![(
                                                    file,
                                                    "heritage-template",
                                                    key.clone(),
                                                    vec![(f, Some(T::Array(names)))],
                                                )]);
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                }
            });
    }

    fn experience_page(
        &mut self,
        ui: &mut egui::Ui,
        t: &crate::text::Table,
        pack: &str,
        change: &mut Option<Vec<RowChange>>,
    ) {
        let Some(xp) = self.table_now::<XpTable>(EXPERIENCE) else {
            ui.label(t.get("tables.no_table"));
            return;
        };
        let file = table_views::EXPERIENCE_FILE;
        let curve = table_views::curve(&xp);
        let levels: Vec<u64> = curve.iter().map(|(x, _)| *x).collect();
        let max = levels.len().saturating_sub(1);
        ui.horizontal(|ui| {
            ui.label(t.get("tables.max_level"));
            if let Some(v) = self.number(ui, "xp max", i64::try_from(max).unwrap_or(0), 48.0) {
                let Ok(n) = usize::try_from(v) else { return };
                if n < 1 {
                    return;
                }
                let mut c: Vec<RowChange> = vec![(
                    file,
                    "experience",
                    Vec::new(),
                    vec![("max_level", Some(int(v)))],
                )];
                for (level, x) in table_views::raised(&levels, n) {
                    c.push((
                        file,
                        "level",
                        vec![("level", int(i64::try_from(level).unwrap_or(0)))],
                        vec![("xp", Some(int(i64::try_from(x).unwrap_or(i64::MAX))))],
                    ));
                }
                *change = Some(c);
            }
            if self.table_row(file, "experience", &[]).is_some() {
                ui.weak(t.fill("tables.changed_in", &[("pack", &pack)]));
            }
        });
        for (f, list) in [
            ("attribute", &xp.attribute_xp),
            ("vital", &xp.vital_xp),
            ("trained", &xp.trained_xp),
            ("specialized", &xp.specialized_xp),
        ] {
            ui.horizontal(|ui| {
                ui.label(t.get(&format!("tables.raise_{f}")));
                let now = list
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                if let Some(v) = self.words(ui, &format!("xp {f}"), &now, 420.0) {
                    let values: Option<Vec<T>> = v
                        .split(',')
                        .map(|n| n.trim().parse::<i64>().ok().map(int))
                        .collect();
                    match values {
                        Some(values) => {
                            *change = Some(vec![(
                                file,
                                "experience",
                                Vec::new(),
                                vec![(f, Some(T::Array(values)))],
                            )]);
                        }
                        None => {
                            self.tables.refused =
                                Some((format!("xp {f}"), t.get("tables.not_numbers").to_owned()));
                        }
                    }
                }
            });
        }
        ui.separator();
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        egui::ScrollArea::vertical()
            .id_salt("tables-levels")
            .auto_shrink([false, false])
            .show_rows(ui, row_h, curve.len().saturating_sub(1), |ui, range| {
                for n in range.map(|i| i + 1) {
                    let (x, c) = curve[n];
                    let key = vec![("level", int(i64::try_from(n).unwrap_or(0)))];
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{n:>3}"));
                        if let Some(v) = self.number(
                            ui,
                            &format!("level {n} xp"),
                            i64::try_from(x).unwrap_or(i64::MAX),
                            110.0,
                        ) {
                            let v = u64::try_from(v).unwrap_or(0);
                            match table_views::curve_refuses(&levels, n, v) {
                                Some((at, theirs)) => {
                                    self.tables.refused = Some((
                                        format!("level {n} xp"),
                                        t.fill(
                                            "tables.curve_falls",
                                            &[
                                                ("n", &n),
                                                ("xp", &v),
                                                ("at", &at),
                                                ("theirs", &theirs),
                                            ],
                                        ),
                                    ));
                                }
                                None => {
                                    *change = Some(vec![(
                                        file,
                                        "level",
                                        key.clone(),
                                        vec![(
                                            "xp",
                                            Some(int(i64::try_from(v).unwrap_or(i64::MAX))),
                                        )],
                                    )]);
                                }
                            }
                        }
                        ui.weak(t.get("tables.credits"));
                        if let Some(v) =
                            self.number(ui, &format!("level {n} credits"), i64::from(c), 36.0)
                        {
                            *change = Some(vec![(
                                file,
                                "level",
                                key.clone(),
                                vec![("credits", Some(int(v)))],
                            )]);
                        }
                        if self.table_row(file, "level", &key).is_some()
                            && ui.small_button(t.get("tables.below")).clicked()
                        {
                            *change = Some(vec![(
                                file,
                                "level",
                                key.clone(),
                                vec![("xp", None), ("credits", None)],
                            )]);
                        }
                    });
                }
            });
    }
}
