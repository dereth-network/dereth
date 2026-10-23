//! The editor's panels, in egui: the bar of modes and switches along the top, the status along the
//! bottom, and between them docked tabs: the world view at the top left, the map (in dungeon
//! mode, the dungeon's grid) under it, and one column of controls on the right, the tool of the
//! mode over the inspector, the lists and the checks. The world shows through its own tab, and
//! the marks on it (the selection's box, the handles, the brush) are painted under the panels.

use dereth_primitives::frame::{globaltolocal, localtoglobal};
use dereth_primitives::{DataId, Frame, Vec3};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use lyceum_foundry::brush::Brush;
use lyceum_foundry::compile::{Field, StaticOrigin};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::models::Category;
use lyceum_foundry::pick;

use super::{add, amount_of, step_brush, value_of, EditorApp, Mode, Selection, UiAction};
use crate::camera::landblock_of;
use crate::gizmo::{self, Handle};
use crate::view::View;

/// A docked tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    World,
    Tools,
    Inspector,
    Weenie,
    Map,
    Checks,
    Nearby,
    Dungeons,
    Hierarchy,
}

/// Colours of the marks on the world.
pub(crate) mod mark {
    use egui::Color32;
    pub const X: Color32 = Color32::from_rgb(0xE8, 0x4A, 0x3C);
    pub const Y: Color32 = Color32::from_rgb(0x5C, 0xD0, 0x5A);
    pub const Z: Color32 = Color32::from_rgb(0x3C, 0x8C, 0xF0);
    pub const TURN: Color32 = Color32::from_rgb(0xF0, 0xD0, 0x40);
    pub const SELECT: Color32 = Color32::WHITE;
    pub const OUTLINE: Color32 = Color32::from_rgba_premultiplied(0xB0, 0xB0, 0xB0, 0xB0);
    pub const BRUSH: Color32 = Color32::from_rgb(0xFF, 0xC0, 0x40);
    pub const WARN: Color32 = Color32::from_rgb(0xF0, 0xC0, 0x60);
    pub const FAULT: Color32 = Color32::from_rgb(0xFF, 0x6A, 0x5A);
    pub const CAMERA: Color32 = Color32::from_rgb(0xFF, 0x30, 0x30);
    pub const GENERATOR: Color32 = Color32::from_rgb(0x60, 0xD0, 0xFF);
}

/// The layout: the world view at the top left with the map (the
/// dungeon's grid, in dungeon mode) under it, and one column of controls on the right: the tool
/// over the inspector, the lists and the checks.
#[must_use]
pub fn default_dock() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::World]);
    let tree = d.main_surface_mut();
    let [world, tools] = tree.split_right(NodeIndex::root(), 0.68, vec![Tab::Tools]);
    tree.split_below(
        tools,
        0.6,
        vec![
            Tab::Inspector,
            Tab::Nearby,
            Tab::Hierarchy,
            Tab::Dungeons,
            Tab::Weenie,
            Tab::Checks,
        ],
    );
    tree.split_below(world, 0.64, vec![Tab::Map]);
    d
}

/// What one pass of the panels found.
#[derive(Debug, Default)]
pub struct Pass {
    pub actions: Vec<UiAction>,
    /// The world tab's rectangle on screen.
    pub world: Option<egui::Rect>,
}

/// The tabs' contents, drawn from the editor.
struct Viewer<'a> {
    app: &'a mut EditorApp,
    pass: &'a mut Pass,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let t = self.app.text.table();
        match tab {
            Tab::World => t.get("tabs.world").into(),
            Tab::Tools => match self.app.mode {
                Mode::Select if self.app.dungeon.is_some() => t.get("tabs.dungeon").into(),
                Mode::Select => t.get("tabs.select").into(),
                Mode::Place => t.get("tabs.models").into(),
                Mode::Terrain => t.get("tabs.brush").into(),
                Mode::Spawn => t.get("tabs.spawn").into(),
            },
            Tab::Hierarchy => t.get("tabs.hierarchy").into(),
            Tab::Inspector => t.get("tabs.inspector").into(),
            Tab::Weenie => t.get("tabs.weenie").into(),
            Tab::Map if self.app.grid_shown() => t.get("tabs.grid").into(),
            Tab::Map => t.get("tabs.map").into(),
            Tab::Nearby => t.get("tabs.nearby").into(),
            Tab::Dungeons => t.get("tabs.dungeons").into(),
            Tab::Checks => {
                let n = self.app.faults.len();
                if n == 0 {
                    t.get("tabs.checks").into()
                } else {
                    t.fill("tabs.checks_n", &[("n", &n)]).into()
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::World => {
                self.pass.world = Some(ui.max_rect());
                // The marks drawn after the tabs, and the frame drawn after the panels, take the
                // world view as it is laid out now.
                self.app.world_rect = Some(ui.max_rect());
            }
            Tab::Tools => self.app.tools_tab(ui, &mut self.pass.actions),
            Tab::Hierarchy => self.app.hierarchy_tab(ui, &mut self.pass.actions),
            Tab::Inspector => self.app.inspector_tab(ui, &mut self.pass.actions),
            Tab::Weenie => self.app.weenie_tab(ui, &mut self.pass.actions),
            Tab::Map if self.app.grid_shown() => self.app.grid_ui(ui, &mut self.pass.actions),
            Tab::Map => self.app.map_tab(ui, &mut self.pass.actions),
            Tab::Nearby => self.app.nearby_tab(ui, &mut self.pass.actions),
            Tab::Dungeons => self.app.dungeons_tab(ui, &mut self.pass.actions),
            Tab::Checks => self.app.checks_tab(ui, &mut self.pass.actions),
        }
    }

    fn is_closeable(&self, _tab: &Tab) -> bool {
        false
    }

    fn clear_background(&self, tab: &Tab) -> bool {
        *tab != Tab::World
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        if *tab == Tab::World {
            [false, false]
        } else {
            [false, true]
        }
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        *tab != Tab::World
    }
}

impl EditorApp {
    /// One pass of the panels inside the root `ui`: the bars, the docked tabs, and the marks on the
    /// world under them.
    pub(super) fn panels_ui(&mut self, ui: &mut egui::Ui) -> Pass {
        let mut pass = Pass::default();
        egui::Panel::top("bar").show(ui, |ui| self.top_bar(ui, &mut pass.actions));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui, &mut pass.actions));
        let mut dock = self.dock.take().unwrap_or_else(default_dock);
        DockArea::new(&mut dock)
            .show_close_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show_inside(
                ui,
                &mut Viewer {
                    app: self,
                    pass: &mut pass,
                },
            );
        self.dock = Some(dock);
        if let Some(rect) = pass.world {
            // The marks on the world are drawn in the window's pixels, as the world is; the
            // words over it in the panels' points.
            let scale = self.gui.scale();
            let marks_layer = egui::LayerId::new(egui::Order::Background, egui::Id::new("marks"));
            ui.ctx().set_transform_layer(
                marks_layer,
                egui::emath::TSTransform::from_scaling(1.0 / scale),
            );
            let marks = ui
                .ctx()
                .layer_painter(marks_layer)
                .with_clip_rect(egui::Rect::from_min_max(rect.min * scale, rect.max * scale));
            let painter = ui
                .ctx()
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(rect);
            if self.playing.is_some() {
                self.play_marks(ui.ctx(), &painter, rect, &mut pass.actions);
            } else {
                self.world_marks(&marks);
            }
            if self.panels.show_hints && self.playing.is_none() {
                let key = match self.mode {
                    Mode::Select if self.dungeon.is_some() => "hints.dungeon",
                    Mode::Select => "hints.select",
                    Mode::Place if self.dropping_building() => "hints.place_building",
                    Mode::Place => "hints.place",
                    Mode::Terrain => "hints.terrain",
                    Mode::Spawn => "hints.spawn",
                };
                painter.text(
                    rect.left_bottom() + egui::vec2(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    self.text.table().get(key),
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
            }
        }
        self.character_panel(ui.ctx(), &mut pass.actions);
        pass
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        egui::ScrollArea::horizontal()
            .id_salt("mode-bar")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let in_dungeon = self.dungeon.is_some();
                    for (label, tip, m) in [
                        ("top.select", "top.select_tip", Mode::Select),
                        ("top.place", "top.place_tip", Mode::Place),
                        ("top.terrain", "top.terrain_tip", Mode::Terrain),
                        ("top.spawn", "top.spawn_tip", Mode::Spawn),
                    ] {
                        // A dungeon has no land to paint.
                        let enabled = !(in_dungeon && m == Mode::Terrain);
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::selectable(self.mode == m, t.get(label)),
                            )
                            .on_hover_text(t.get(tip))
                            .clicked()
                        {
                            out.push(UiAction::Mode(m));
                        }
                    }
                    ui.separator();
                    let (undo, redo) = self.session.history();
                    if ui
                        .add_enabled(undo > 0, egui::Button::new(t.get("top.undo")))
                        .clicked()
                    {
                        out.push(UiAction::Undo);
                    }
                    if ui
                        .add_enabled(redo > 0, egui::Button::new(t.get("top.redo")))
                        .clicked()
                    {
                        out.push(UiAction::Redo);
                    }
                    if ui
                        .add_enabled(self.session.unsaved(), egui::Button::new(t.get("top.save")))
                        .clicked()
                    {
                        out.push(UiAction::Save);
                    }
                    ui.separator();
                    ui.label(t.get("top.speed"));
                    if ui.small_button("-").clicked() {
                        out.push(UiAction::Speed(-1));
                    }
                    ui.monospace(t.fill(
                        "top.speed_value",
                        &[("n", &format!("{:>5}", self.camera.meters_per_second()))],
                    ));
                    if ui.small_button("+").clicked() {
                        out.push(UiAction::Speed(1));
                    }
                    ui.separator();
                    if self.server_sql.is_none() {
                        let fetching = self.fetching.is_some();
                        if ui
                            .add_enabled(
                                !fetching,
                                egui::Button::new(t.get(if fetching {
                                    "top.database_fetching"
                                } else {
                                    "top.database_fetch"
                                })),
                            )
                            .on_hover_text(t.get("top.database_fetch_tip"))
                            .clicked()
                        {
                            out.push(UiAction::FetchDatabase);
                        }
                    }
                    let (up, starting) = self.live_state();
                    if self.playing.is_some() || self.waiting_play.is_some() {
                        let stop = ui
                            .add(egui::Button::new(
                                egui::RichText::new(t.get("top.stop")).color(mark::FAULT),
                            ))
                            .on_hover_text(t.get("top.stop_tip"));
                        self.play_button = Some(stop.rect);
                        if stop.clicked() {
                            out.push(UiAction::StopPlay);
                        }
                    } else {
                        let play = ui
                            .add_enabled(up, egui::Button::new(t.get("top.play")))
                            .on_hover_text(t.get("top.play_tip"));
                        self.play_button = Some(play.rect);
                        if play.clicked() {
                            out.push(UiAction::Play);
                        }
                    }
                    if ui
                        .selectable_label(
                            up || starting,
                            t.get(if starting {
                                "top.live_starting"
                            } else {
                                "top.live"
                            }),
                        )
                        .on_hover_text(t.get("top.live_tip"))
                        .clicked()
                    {
                        out.push(UiAction::Live(!(up || starting)));
                    }
                    if ui.button(t.get("characters.title")).clicked() {
                        self.characters.open = true;
                    }
                    ui.menu_button(t.get("top.play_options"), |ui| {
                        let mut o = self.play_options;
                        ui.label(t.get("top.interface"));
                        for i in super::PlayInterface::ALL {
                            ui.radio_value(
                                &mut o.interface,
                                i,
                                t.or(&format!("top.{}", i.word()), i.name()),
                            );
                        }
                        ui.weak(t.get("top.play_options_note"));
                        if o != self.play_options {
                            out.push(UiAction::PlayOptions(o));
                        }
                    });
                    ui.menu_button(t.get("top.export"), |ui| {
                        use lyceum_foundry::export::Target;
                        for target in Target::ALL {
                            let (label, tip) = match target {
                                Target::Empyrean => {
                                    ("top.export_for_empyrean", "top.export_for_empyrean_tip")
                                }
                                Target::Ace => ("top.export_for_ace", "top.export_for_ace_tip"),
                                Target::Gdle => ("top.export_for_gdle", "top.export_for_gdle_tip"),
                            };
                            let ready = !target.needs_world_database() || self.server_sql.is_some();
                            if ui
                                .add_enabled(ready, egui::Button::new(t.get(label)))
                                .on_hover_text(t.get(tip))
                                .on_disabled_hover_text(t.get("top.export_needs_database"))
                                .clicked()
                            {
                                out.push(UiAction::Export(target));
                                ui.close();
                            }
                        }
                        ui.separator();
                        ui.label(t.get("top.export_gdle_quests"))
                            .on_hover_text(t.get("top.export_gdle_quests_tip"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gdle_quests)
                                .hint_text(t.get("top.export_gdle_quests_hint"))
                                .desired_width(280.0),
                        )
                        .on_hover_text(t.get("top.export_gdle_quests_tip"));
                    });
                    ui.menu_button(t.get("views.title"), |ui| {
                        if ui
                            .selectable_label(self.panels.top_down, t.get("views.top_down"))
                            .clicked()
                        {
                            out.push(UiAction::TopDown);
                        }
                        if ui
                            .selectable_label(self.walking, t.get("top.walking"))
                            .clicked()
                        {
                            out.push(UiAction::ToggleWalk);
                        }
                        if ui.button(t.get("top.snap")).clicked() {
                            out.push(UiAction::Snap);
                        }
                        ui.checkbox(&mut self.panels.show_hints, t.get("views.hints"));
                        ui.menu_button(t.get("views.ui_scale"), |ui| {
                            let now = self.gui.scale();
                            for scale in crate::gui::SCALES {
                                let label =
                                    t.fill("views.ui_scale_n", &[("n", &super::percent(scale))]);
                                if ui.radio(now == scale, label).clicked() {
                                    out.push(UiAction::UiScale(scale));
                                }
                            }
                            ui.weak(t.get("views.ui_scale_keys"));
                        });
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let place = landblock_of(self.eye()).map_or_else(
            || t.get("status.off_world").to_owned(),
            |(lb, x, y)| format!("{lb:04X} {x:.1}, {y:.1}, {:.1}", self.eye().z),
        );
        let (edits, saved) = (self.session.history().0, !self.session.unsaved());
        ui.horizontal(|ui| {
            let checks = ui.button(t.fill("tabs.checks_n", &[("n", &self.faults.len())]));
            self.checks_button = Some(checks.rect);
            if checks.clicked() {
                out.push(UiAction::ShowTab(Tab::Checks));
            }
            ui.monospace(place);
            ui.label(t.fill(
                "status.selected",
                &[(
                    "n",
                    &(usize::from(self.selection.is_some()) + self.also.len()),
                )],
            ));
            ui.checkbox(&mut self.panels.show_grid, t.get("status.grid"));
            if ui
                .selectable_label(self.collide, t.get("top.collide"))
                .clicked()
            {
                out.push(UiAction::Collide);
            }
            ui.checkbox(&mut self.panels.show_bounds, t.get("status.bounds"));
            ui.label(t.get(match self.mode {
                Mode::Select if self.dungeon.is_some() => "status.dungeon",
                Mode::Select => "status.select",
                Mode::Place => "status.place",
                Mode::Terrain => "status.terrain",
                Mode::Spawn => "status.spawn",
            }));
            ui.label(t.fill(
                if saved {
                    "status.edits_saved"
                } else {
                    "status.edits"
                },
                &[("n", &edits)],
            ));
            if let Some(p) = &self.text.problem {
                ui.colored_label(mark::WARN, p);
            }
            if let Some(note) = self.ui_scale_note() {
                ui.strong(note);
                self.ui_scale_note -= 1;
            }
            ui.label(&self.panels.message);
        });
    }

    fn tools_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.brush_rects.clear();
        // In dungeon mode, the dungeon's card heads every tool, and the select tool lays out its
        // pieces.
        if let Some(lb) = self.dungeon_mode() {
            if self.mode == Mode::Select {
                self.dungeon_tab(ui, out);
                return;
            }
            self.dungeon_card(ui, lb, out);
        }
        match self.mode {
            Mode::Select => {
                ui.label(self.text.table().get("select.help"));
            }
            Mode::Place => self.browser(ui, out),
            Mode::Terrain => self.brushes(ui, out),
            Mode::Spawn => self.spawn_browser(ui, out),
        }
    }

    /// The spawn browser: every weenie of the packs and the server, by name, kind and picture,
    /// filtered by a typed name or class id and by kind. A click chooses the one the spawn tool
    /// places.
    fn spawn_browser(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        search_box(ui, &mut self.panels.weenie_filter, t.get("spawn.search"));
        let mut counts: std::collections::BTreeMap<Category, usize> =
            std::collections::BTreeMap::new();
        for w in &self.weenies {
            *counts.entry(w.category).or_default() += 1;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.panels.weenie_category.is_none(), t.get("spawn.all"))
                .clicked()
            {
                out.push(UiAction::WeenieCategory(None));
            }
            for c in Category::ALL {
                let Some(n) = counts.get(&c).copied().filter(|n| *n > 0) else {
                    continue;
                };
                let on = self.panels.weenie_category == Some(c);
                let name = t.or(&format!("categories.{}", c.name()), c.name());
                if ui.selectable_label(on, format!("{name} {n}")).clicked() {
                    out.push(UiAction::WeenieCategory((!on).then_some(c)));
                }
            }
        });
        let list = self.spawn_list();
        ui.horizontal(|ui| {
            ui.label(t.fill(
                "spawn.shown",
                &[("n", &list.len()), ("all", &self.weenies.len())],
            ));
            if self.naming() {
                ui.weak(t.get("spawn.reading"));
            }
        });
        if self.session.server_weenies().is_empty() && !self.naming() {
            ui.weak(t.get("spawn.only_packs"));
        }
        ui.separator();
        let side = crate::thumbs::SIZE as f32;
        let cell = egui::vec2(side + 30.0, side + 28.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few columns
        let columns = ((ui.available_width() / (cell.x + 4.0)).floor().max(1.0) as usize).max(1);
        let rows = list.len().div_ceil(columns);
        let mut budget = super::PICTURES_A_FRAME;
        self.weenie_rects.clear();
        // A new portal, placed like any spawn: its own weenie, its destination on its card.
        ui.horizontal(|ui| {
            let picture = self
                .session
                .class_facts(super::dungeons::PORTAL_BLANK)
                .and_then(|f| f.setup)
                .and_then(|s| self.picture(DataId(s), &mut budget));
            let (r, rect) =
                picker_cell(ui, cell, picture, t.get("spawn.portal"), self.panels.portal);
            self.panels.portal_rect = Some(rect);
            if r.on_hover_text(t.get("spawn.portal_tip")).clicked() {
                out.push(UiAction::NewPortalTool);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("weenies")
            .auto_shrink(false)
            .show_rows(ui, cell.y + 4.0, rows, |ui, shown| {
                for row in shown {
                    ui.horizontal(|ui| {
                        for i in list.iter().skip(row * columns).take(columns) {
                            let w = self.weenies[*i].clone();
                            let picture =
                                w.setup.and_then(|s| self.picture(DataId(s), &mut budget));
                            let chosen = self.panels.weenie.as_ref() == Some(&w.weenie);
                            let (r, rect) = picker_cell(ui, cell, picture, &w.name, chosen);
                            self.weenie_rects.push((w.weenie.clone(), rect));
                            let id = match &w.weenie {
                                Weenie::Symbol(s) => s.clone(),
                                Weenie::Class(c) => t.fill("spawn.class", &[("id", c)]),
                            };
                            let from = t.get(if w.pack {
                                "spawn.from_pack"
                            } else {
                                "spawn.from_server"
                            });
                            let kind = t.or(
                                &format!("categories.{}", w.category.name()),
                                w.category.name(),
                            );
                            if r.on_hover_text(format!("{}\n{id}, {from}\n{kind}", w.name))
                                .clicked()
                            {
                                out.push(UiAction::Weenie(w.weenie.clone()));
                            }
                        }
                    });
                }
            });
    }

    /// The model browser: the shelves (every model, the recent and the favorites), a search
    /// (an id or a name), the filters (a category and a tag, each one dropdown), and every model
    /// with its picture and name, those standing around the camera first; each with a star to
    /// keep it among the favorites, and its tags on a right click.
    fn browser(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.horizontal(|ui| {
            for (shelf, key) in [
                (crate::browser_state::Shelf::All, "models.all"),
                (crate::browser_state::Shelf::Recent, "models.recent"),
                (crate::browser_state::Shelf::Favorites, "models.favorites"),
            ] {
                ui.selectable_value(&mut self.panels.shelf, shelf, t.get(key));
            }
        });
        search_box(ui, &mut self.panels.filter, t.get("models.search"));
        // The filters: the kinds (several at once, each a check box, and Clear) and a tag, each
        // one dropdown, and Clear.
        let counts = self.category_counts();
        let tags = self.tags.all();
        ui.horizontal_wrapped(|ui| {
            ui.label(t.get("models.filters"));
            let name_of = |c: Category| {
                t.or(&format!("categories.{}", c.name()), c.name())
                    .to_owned()
            };
            let shown = match self.panels.kinds.len() {
                0 => t.get("models.all_kinds").to_owned(),
                1 => self.panels.kinds.iter().map(|c| name_of(*c)).collect(),
                n => t.fill("models.kinds_n", &[("n", &n)]),
            };
            let combo = egui::ComboBox::from_id_salt("model category")
                .selected_text(shown)
                .height(360.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    self.panels.category_rows.clear();
                    let all = ui.add_enabled(
                        !self.panels.kinds.is_empty(),
                        egui::Button::new(t.get("models.clear_kinds")),
                    );
                    self.panels.category_rows.push((None, all.rect));
                    if all.clicked() {
                        out.push(UiAction::Category(None));
                    }
                    for c in Category::ALL {
                        let Some(n) = counts.get(&c).copied().filter(|n| *n > 0) else {
                            continue;
                        };
                        let mut on = self.panels.kinds.contains(&c);
                        let r = ui.checkbox(&mut on, format!("{} ({n})", name_of(c)));
                        self.panels.category_rows.push((Some(c), r.rect));
                        if r.clicked() {
                            out.push(UiAction::Category(Some(c)));
                        }
                    }
                });
            self.panels.category_combo = Some(combo.response.rect);
            if !tags.is_empty() {
                let shown = self
                    .panels
                    .tag
                    .as_ref()
                    .map_or_else(|| t.get("models.any_tag").to_owned(), |g| format!("#{g}"));
                egui::ComboBox::from_id_salt("model tag")
                    .selected_text(shown)
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.panels.tag.is_none(), t.get("models.any_tag"))
                            .clicked()
                        {
                            out.push(UiAction::Tag(None));
                        }
                        for (g, n) in &tags {
                            let on = self.panels.tag.as_deref() == Some(g.as_str());
                            if ui.selectable_label(on, format!("#{g} ({n})")).clicked() {
                                out.push(UiAction::Tag(Some(g.clone())));
                            }
                        }
                    });
            }
            self.panels.clear_filters = None;
            if !self.panels.kinds.is_empty() || self.panels.tag.is_some() {
                let clear = ui.small_button(t.get("models.clear"));
                self.panels.clear_filters = Some(clear.rect);
                if clear.clicked() {
                    out.push(UiAction::Category(None));
                    out.push(UiAction::Tag(None));
                }
            }
        });
        let list = self.browser_list();
        let nearby = self.nearby_models();
        ui.horizontal(|ui| {
            ui.label(t.fill(
                "models.shown",
                &[("n", &list.len()), ("all", &self.all_models.len())],
            ));
            if self.naming() {
                ui.weak(t.get("models.naming"));
            }
        });
        ui.separator();
        // The chosen building's card: its layout and its furniture.
        self.building_card(ui, out);
        // The grid: a picture a cell, its name under it.
        let side = crate::thumbs::SIZE as f32;
        // Room under the picture for two lines of its name.
        let cell = egui::vec2(side + 30.0, side + 28.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few columns
        let columns = ((ui.available_width() / (cell.x + 4.0)).floor().max(1.0) as usize).max(1);
        let rows = list.len().div_ceil(columns);
        let mut budget = super::PICTURES_A_FRAME;
        self.picture_rects.clear();
        self.picture_stars.clear();
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            cell.y + 4.0,
            rows,
            |ui, shown| {
                for row in shown {
                    ui.horizontal(|ui| {
                        for m in list.iter().skip(row * columns).take(columns) {
                            self.grid_cell(ui, *m, cell, nearby.contains(m), &mut budget, out);
                        }
                    });
                }
            },
        );
    }

    /// One model in the browser's grid: its picture, its name under it, what it is on hover; a
    /// click chooses it.
    fn grid_cell(
        &mut self,
        ui: &mut egui::Ui,
        m: DataId,
        cell: egui::Vec2,
        nearby: bool,
        budget: &mut u32,
        out: &mut Vec<UiAction>,
    ) {
        let side = crate::thumbs::SIZE as f32;
        let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
        let picture = egui::Rect::from_min_size(
            egui::pos2(rect.center().x - side / 2.0, rect.min.y),
            egui::vec2(side, side),
        );
        let chosen = self.model == Some(m);
        let painter = ui.painter_at(rect);
        if chosen || r.hovered() {
            painter.rect_filled(
                rect,
                3.0,
                if chosen {
                    egui::Color32::from_rgb(40, 60, 90)
                } else {
                    egui::Color32::from_gray(45)
                },
            );
        }
        match self.picture(m, budget) {
            Some(id) => {
                painter.image(
                    id,
                    picture,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                painter.rect_filled(picture, 2.0, egui::Color32::from_gray(20));
            }
        }
        self.picture_rects.push((m, picture));
        let label = self.names.label(m);
        // A building of the gallery by what it is: a house by its kind.
        let building = self
            .panels
            .kinds
            .contains(&lyceum_foundry::models::Category::Building)
            .then(|| self.session.buildings().and_then(|c| c.get(m)).cloned())
            .flatten();
        let words = match &building {
            Some(e) => super::building_name(e, label.clone()),
            None => label.clone().unwrap_or_else(|| format!("{:08X}", m.raw())),
        };
        let font = egui::FontId::proportional(10.0);
        let galley = painter.layout(
            words,
            font,
            egui::Color32::from_gray(if nearby { 255 } else { 190 }),
            cell.x - 2.0,
        );
        let at = egui::pos2(
            rect.center().x - galley.size().x.min(cell.x) / 2.0,
            picture.max.y + 1.0,
        );
        painter.galley(at, galley, egui::Color32::WHITE);
        let tags = self.tags.of(m).to_vec();
        let t = self.text.table();
        // The star in the top-right corner: filled for a favorite; a click keeps it or lets it go.
        let favorite = self.browser_state.favorites.contains(&m.raw());
        let star = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - 17.0, rect.min.y + 1.0),
            egui::vec2(16.0, 16.0),
        );
        let s = ui
            .interact(star, ui.id().with(("star", m.raw())), egui::Sense::click())
            .on_hover_text(t.get(if favorite {
                "models.unfavorite"
            } else {
                "models.favorite"
            }));
        draw_star(&painter, star, favorite, s.hovered());
        self.picture_stars.push((m, star));
        if s.clicked() {
            out.push(UiAction::Favorite(m));
        }
        // Its tags, on a right click: each to take away or give, and a new one.
        r.context_menu(|ui| {
            ui.strong(t.get("models.tags"));
            self.panels.tag_menu.clear();
            for (g, _) in self.tags.all() {
                let mut on = tags.contains(&g);
                let r = ui.checkbox(&mut on, format!("#{g}"));
                self.panels.tag_menu.push((g.clone(), r.rect));
                if r.changed() {
                    out.push(UiAction::TagModel(m, g.clone(), on));
                }
            }
            ui.separator();
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.panels.new_tag)
                    .hint_text(t.get("models.tag_hint"))
                    .desired_width(120.0),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (entered || ui.small_button(t.get("models.tag_add")).clicked())
                && !self.panels.new_tag.trim().is_empty()
            {
                out.push(UiAction::TagModel(
                    m,
                    std::mem::take(&mut self.panels.new_tag),
                    true,
                ));
                ui.close();
            }
        });
        let r = r.on_hover_ui(|ui| {
            match &building {
                Some(e) => {
                    ui.strong(super::building_name(e, label.clone()));
                    ui.label(t.fill("place.placed_n", &[("n", &e.count)]));
                }
                None => {
                    ui.strong(label.as_deref().unwrap_or(t.get("models.unused")));
                }
            }
            ui.monospace(format!("{:#010X}", m.raw()));
            let c = self.names.category(m).name();
            let kind = t.or(&format!("categories.{c}"), c);
            ui.label(if nearby {
                t.fill("models.kind_nearby", &[("kind", &kind)])
            } else {
                kind.to_owned()
            });
            if !tags.is_empty() {
                ui.label(
                    tags.iter()
                        .map(|t| format!("#{t}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        });
        if r.clicked() {
            out.push(UiAction::Model(m));
        }
    }

    fn brushes(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let b = self.panels.brush;
        // The ground's brushes, each an icon; the painting brushes by name.
        ui.horizontal_wrapped(|ui| {
            for (label, brush, icon) in [
                ("brush.raise", Brush::Raise(amount_of(b)), Icon::Raise),
                ("brush.lower", Brush::Lower(amount_of(b)), Icon::Lower),
                ("brush.flatten", Brush::Flatten(None), Icon::Flatten),
                ("brush.smooth", Brush::Smooth, Icon::Smooth),
            ] {
                let on = std::mem::discriminant(&b) == std::mem::discriminant(&brush);
                let r = icon_button(ui, icon, on).on_hover_text(format!(
                    "{}\n{}",
                    t.get(label),
                    t.get(&format!("{label}_tip"))
                ));
                self.brush_rects.push((brush, r.rect));
                if r.clicked() {
                    out.push(UiAction::Brush(brush));
                }
            }
            ui.separator();
            for (label, brush) in [
                ("brush.terrain", Brush::Terrain(value_of(b))),
                ("brush.scene", Brush::Scene(value_of(b))),
                ("brush.road", Brush::Road(1)),
            ] {
                let on = std::mem::discriminant(&b) == std::mem::discriminant(&brush);
                let r = ui
                    .selectable_label(on, t.get(label))
                    .on_hover_text(t.get(&format!("{label}_tip")));
                self.brush_rects.push((brush, r.rect));
                if r.clicked() {
                    out.push(UiAction::Brush(brush));
                }
            }
        });
        ui.separator();
        // The radius, in the land's vertices: the ground has a height only at each, 24 m apart.
        ui.horizontal(|ui| {
            ui.label(t.get("brush.radius"));
            if ui.small_button("-").clicked() {
                out.push(UiAction::Radius(-1));
            }
            let g = lyceum_foundry::brush::GRID;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ten at most
            let n = (self.panels.radius / g).round().max(0.0) as u32;
            ui.monospace(match n {
                0 => t.get("brush.radius_under").to_owned(),
                1 => t.fill("brush.radius_one", &[("m", &format!("{g:.0}"))]),
                _ => t.fill(
                    "brush.radius_n",
                    &[("n", &n), ("m", &format!("{:.0}", self.panels.radius))],
                ),
            });
            if ui.small_button("+").clicked() {
                out.push(UiAction::Radius(1));
            }
        });
        if let Brush::Road(r) = b {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(r != 0, t.get("brush.paint_road"))
                    .clicked()
                {
                    out.push(UiAction::Brush(Brush::Road(1)));
                }
                if ui
                    .selectable_label(r == 0, t.get("brush.erase_road"))
                    .clicked()
                {
                    out.push(UiAction::Brush(Brush::Road(0)));
                }
            });
        }
        if let Brush::Terrain(v) = b {
            self.terrain_picker(ui, v, out);
        }
        if let Brush::Scene(v) = b {
            self.scene_picker(ui, v, out);
        }
        let (label, value) = match b {
            Brush::Raise(n) | Brush::Lower(n) => (t.get("brush.steps"), n.to_string()),
            Brush::Scene(_)
            | Brush::Terrain(_)
            | Brush::Road(_)
            | Brush::Flatten(_)
            | Brush::Smooth => ("", String::new()),
        };
        if !label.is_empty() {
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.small_button("-").clicked() {
                    out.push(UiAction::Amount(-1));
                }
                ui.monospace(value);
                if ui.small_button("+").clicked() {
                    out.push(UiAction::Amount(1));
                }
            });
        }
        let _ = step_brush;
    }

    /// The terrain brush's terrain: one dropdown of the region's terrains, each by its picture
    /// and name; one chosen paints with it.
    fn terrain_picker(&mut self, ui: &mut egui::Ui, chosen: u16, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let types = self.terrain_types();
        let name = types.iter().find(|(v, ..)| *v == chosen).map_or_else(
            || t.fill("brush.terrain_n", &[("n", &chosen)]),
            |(_, n, _)| n.clone(),
        );
        self.terrain_rects.clear();
        ui.horizontal(|ui| {
            ui.label(t.get("brush.terrain"));
            let shown_tex = types
                .iter()
                .find(|(v, ..)| *v == chosen)
                .and_then(|(_, _, tex)| *tex)
                .and_then(|tex| self.texture_picture(tex));
            if let Some(id) = shown_tex {
                ui.add(egui::Image::new((id, egui::vec2(22.0, 22.0))));
            }
            let combo = egui::ComboBox::from_id_salt("terrain brush")
                .selected_text(name)
                .width(ui.available_width() - 8.0)
                .height(360.0)
                .show_ui(ui, |ui| {
                    for (v, name, tex) in types.clone() {
                        let picture = tex.and_then(|tx| self.texture_picture(tx));
                        let row = ui
                            .horizontal(|ui| {
                                let (rect, picked) = ui.allocate_exact_size(
                                    egui::vec2(40.0, 40.0),
                                    egui::Sense::click(),
                                );
                                match picture {
                                    Some(id) => {
                                        ui.painter().image(
                                            id,
                                            rect,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            egui::Color32::WHITE,
                                        );
                                    }
                                    None => {
                                        ui.painter().rect_filled(
                                            rect,
                                            2.0,
                                            egui::Color32::from_gray(20),
                                        );
                                    }
                                }
                                let r = ui.selectable_label(v == chosen, &name);
                                (rect, r.clicked() || picked.clicked())
                            })
                            .inner;
                        self.terrain_rects.push((v, row.0));
                        if row.1 {
                            out.push(UiAction::Brush(Brush::Terrain(v)));
                        }
                    }
                });
            self.panels.brush_combo = Some(combo.response.rect);
        });
    }

    /// The scene brush's scenes: what each scene value grows on the terrain under the brush, in
    /// words and pictures; a click paints with it.
    fn scene_picker(&mut self, ui: &mut egui::Ui, chosen: u16, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.separator();
        ui.weak(t.get("brush.scene_help"));
        let terrain = self.panels.scene_terrain;
        let name = self
            .terrain_types()
            .into_iter()
            .find(|(v, ..)| *v == terrain)
            .map_or_else(
                || t.fill("brush.terrain_n", &[("n", &terrain)]),
                |(_, n, _)| n,
            );
        ui.label(t.fill("brush.scene_on", &[("terrain", &name)]));
        let scenes = self.scenes_of(terrain).to_vec();
        if usize::from(chosen) >= scenes.len() {
            ui.weak(t.fill("brush.scene_nothing", &[("n", &chosen), ("terrain", &name)]));
        }
        let mut budget = super::PICTURES_A_FRAME;
        self.scene_rects.clear();
        self.picture_rects.clear();
        let shown = scenes.get(usize::from(chosen)).map_or_else(
            || chosen.to_string(),
            |m| format!("{chosen}  {}", self.scene_words(m)),
        );
        let combo = egui::ComboBox::from_id_salt("scene brush")
            .selected_text(shown)
            .width(ui.available_width() - 8.0)
            .height(420.0)
            .show_ui(ui, |ui| {
                for (i, models) in scenes.iter().enumerate() {
                    let Ok(v) = u16::try_from(i) else { break };
                    let words = self.scene_words(models);
                    let row = ui
                        .push_id(i, |ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(format!("{v}  {words}")).strong());
                                ui.horizontal_wrapped(|ui| {
                                    for m in models.iter().take(8) {
                                        let side = 36.0;
                                        let (rect, r) = ui.allocate_exact_size(
                                            egui::vec2(side, side),
                                            egui::Sense::hover(),
                                        );
                                        match self.picture(*m, &mut budget) {
                                            Some(id) => {
                                                ui.painter().image(
                                                    id,
                                                    rect,
                                                    egui::Rect::from_min_max(
                                                        egui::pos2(0.0, 0.0),
                                                        egui::pos2(1.0, 1.0),
                                                    ),
                                                    egui::Color32::WHITE,
                                                );
                                            }
                                            None => {
                                                ui.painter().rect_filled(
                                                    rect,
                                                    2.0,
                                                    egui::Color32::from_gray(20),
                                                );
                                            }
                                        }
                                        self.picture_rects.push((*m, rect));
                                        let label = self.names.label(*m).unwrap_or_default();
                                        r.on_hover_text(format!("{label}\n{:#010X}", m.raw()));
                                    }
                                    if models.len() > 8 {
                                        ui.weak(format!("+{}", models.len() - 8));
                                    }
                                });
                            })
                        })
                        .response;
                    let rect = row.rect;
                    if v == chosen {
                        ui.painter().rect_stroke(
                            rect.expand(2.0),
                            3.0,
                            egui::Stroke::new(1.5, egui::Color32::from_rgb(90, 140, 220)),
                            egui::StrokeKind::Outside,
                        );
                    }
                    self.scene_rects.push((v, rect));
                    if ui
                        .interact(rect, ui.id().with(("scene", i)), egui::Sense::click())
                        .clicked()
                    {
                        out.push(UiAction::Brush(Brush::Scene(v)));
                    }
                    ui.add_space(4.0);
                }
            });
        self.panels.brush_combo = Some(combo.response.rect);
    }

    /// The world from above, centered on the camera and zoomed with the wheel: the camera's spot
    /// and heading marked, a click flies there.
    fn map_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let Some(tex) = self.map_texture() else {
            ui.weak(t.get("map.drawing"));
            self.map_rect = None;
            return;
        };
        ui.horizontal(|ui| {
            if ui.small_button("-").clicked() {
                out.push(UiAction::MapZoom(0.5));
            }
            ui.monospace(format!("x{:.1}", self.panels.map_zoom));
            if ui.small_button("+").clicked() {
                out.push(UiAction::MapZoom(2.0));
            }
        });
        // The map at one scale both ways: at zoom 1 the whole world, square, as large as the
        // panel allows and in its middle; zoomed in, as much as the panel holds about the camera.
        let avail = ui.available_size();
        let (w, h) = (avail.x.max(64.0), avail.y.max(64.0));
        let (panel, r) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
        #[allow(clippy::cast_precision_loss)] // 2,048
        let full = lyceum_foundry::map::SIDE as f32;
        self.panels.map_min_zoom = 1.0;
        let zoom = self.panels.map_zoom.max(1.0);
        let scale = w.min(h) * zoom / full;
        let (span_x, span_y) = ((w / scale).min(full), (h / scale).min(full));
        let rect = egui::Rect::from_center_size(
            panel.center(),
            egui::vec2(span_x * scale, span_y * scale),
        );
        let cam = self.camera.position;
        let (cx, cy) = lyceum_foundry::map::pixel_of(cam.x, cam.y);
        let x0 = (cx - span_x / 2.0).clamp(0.0, full - span_x);
        let y0 = (cy - span_y / 2.0).clamp(0.0, full - span_y);
        let view = egui::Rect::from_min_size(egui::pos2(x0, y0), egui::vec2(span_x, span_y));
        self.map_rect = Some(rect);
        self.map_panel = Some(panel);
        self.map_view = view;
        let painter = ui.painter_at(rect);
        painter.image(
            tex,
            rect,
            egui::Rect::from_min_max(
                egui::pos2(x0 / full, y0 / full),
                egui::pos2((x0 + span_x) / full, (y0 + span_y) / full),
            ),
            egui::Color32::WHITE,
        );
        // The camera: a spot, and a line the way it looks.
        let to_screen = |px: f32, py: f32| {
            egui::pos2(
                rect.min.x + (px - x0) / span_x * rect.width(),
                rect.min.y + (py - y0) / span_y * rect.height(),
            )
        };
        let at = to_screen(cx, cy);
        let f = self.camera.forward();
        let (fx, fy) = (f.x, f.y);
        let len = (fx * fx + fy * fy).sqrt().max(1e-3);
        let tip = egui::pos2(at.x + fx / len * 14.0, at.y - fy / len * 14.0);
        painter.line_segment([at, tip], egui::Stroke::new(2.0, mark::CAMERA));
        painter.circle_filled(at, 4.0, mark::CAMERA);
        // A selected generator: the ground it scatters what it makes over.
        if let Some(s) = self.selected_spawn() {
            if let Some(r) = self.spawn_generator(&s).and_then(|g| g.radius) {
                let w = add(
                    lyceum_foundry::pick::block_origin((s.cell >> 16) as u16),
                    s.frame.origin,
                );
                let (px, py) = lyceum_foundry::map::pixel_of(w.x, w.y);
                let (ex, _) = lyceum_foundry::map::pixel_of(w.x + r, w.y);
                let c = to_screen(px, py);
                let radius = (to_screen(ex, py).x - c.x).max(3.0);
                painter.circle_stroke(c, radius, egui::Stroke::new(1.5, mark::GENERATOR));
                painter.circle_filled(c, 2.5, mark::GENERATOR);
            }
        }
        // The dungeon last gone into: its entrances that stand outdoors, each a mark.
        self.dungeon_list.map_marks.clear();
        if let Some(lb) = self.dungeon_list.marked {
            let name = self.dungeon_name_of(lb);
            for e in self.entrances(lb).into_iter().filter(|e| e.outdoors()) {
                let w = add(lyceum_foundry::pick::block_origin(e.landblock), e.at);
                let (px, py) = lyceum_foundry::map::pixel_of(w.x, w.y);
                let p = to_screen(px, py);
                if !rect.contains(p) {
                    continue;
                }
                painter.circle_filled(p, 4.5, mark::GENERATOR);
                painter.circle_stroke(p, 4.5, egui::Stroke::new(1.0, egui::Color32::BLACK));
                painter.text(
                    p + egui::vec2(7.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &name,
                    egui::FontId::proportional(11.0),
                    mark::GENERATOR,
                );
                self.dungeon_list.map_marks.push((
                    egui::Rect::from_center_size(p, egui::vec2(12.0, 12.0)),
                    super::MapMark::Entrance(e.landblock, e.at),
                ));
            }
        }
        // Where the pointer is, and a click flies there.
        let world = |p: egui::Pos2| {
            lyceum_foundry::map::world_of(
                x0 + (p.x - rect.min.x) / rect.width() * span_x,
                y0 + (p.y - rect.min.y) / rect.height() * span_y,
            )
        };
        if let Some(p) = r.hover_pos().filter(|p| rect.contains(*p)) {
            let (x, y) = world(p);
            let scroll: f32 = ui.input(|i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum()
            });
            if scroll != 0.0 {
                out.push(UiAction::MapZoom(if scroll > 0.0 { 1.25 } else { 0.8 }));
            }
            if let Some((lb, ..)) = landblock_of(Vec3::new(x, y, 0.0)) {
                let town = crate::catalog::town_of(lb).unwrap_or_default();
                r.clone()
                    .on_hover_text(format!("{lb:04X} {town}\n{}", t.get("map.tip")));
            }
        }
        if r.clicked() {
            if let Some(p) = r.interact_pointer_pos().filter(|p| rect.contains(*p)) {
                // A portal's mark clicked: the camera put on it.
                let hit = self
                    .dungeon_list
                    .map_marks
                    .iter()
                    .find(|(rect, _)| rect.contains(p))
                    .map(|(_, m)| m.clone());
                match hit {
                    Some(super::MapMark::Entrance(lb, at)) => out.push(UiAction::GoTo(lb, at)),
                    None => {
                        let (x, y) = world(p);
                        out.push(UiAction::FlyTo(x, y));
                    }
                }
            }
        }
    }

    fn inspector_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        self.panels.place_rects.clear();
        if !self.also.is_empty() {
            ui.strong(t.fill("inspector.selected_n", &[("n", &(self.also.len() + 1))]));
            ui.weak(t.get("inspector.several_help"));
            ui.horizontal(|ui| {
                if ui.button(t.get("inspector.delete_all")).clicked() {
                    out.push(UiAction::Delete);
                }
                let dup = ui
                    .button(t.get("inspector.duplicate"))
                    .on_hover_text(t.get("inspector.duplicate_tip"));
                self.panels.duplicate = Some(dup.rect);
                if dup.clicked() {
                    out.push(UiAction::Duplicate);
                }
            });
            ui.separator();
        }
        let at = |o: Vec3| {
            t.fill(
                "inspector.at",
                &[
                    ("x", &format!("{:.2}", o.x)),
                    ("y", &format!("{:.2}", o.y)),
                    ("z", &format!("{:.2}", o.z)),
                ],
            )
        };
        match self.selection.clone() {
            None => {
                ui.weak(t.get("inspector.nothing"));
            }
            Some(Selection::Static { landblock, index }) => {
                let origin = self.session.origins(landblock).get(index).cloned();
                let Some((model, f)) = self.static_frame(landblock, index) else {
                    return;
                };
                ui.strong(t.fill(
                    "inspector.static_title",
                    &[("n", &index), ("landblock", &format!("{landblock:04X}"))],
                ));
                ui.label(t.fill(
                    "inspector.model",
                    &[("id", &format!("{:#010X}", model.raw()))],
                ));
                if let Some(label) = self.names.label(model) {
                    ui.weak(label);
                }
                // The world's own, moved by the pack being edited: changed there.
                if let Some(StaticOrigin::Base(i)) = origin {
                    let target = &self.session.packs()[self.session.target()];
                    let moved = target
                        .edits
                        .iter()
                        .filter(|e| e.landblock == landblock)
                        .flat_map(|e| e.moves.iter())
                        .any(|m| m.item.which == lyceum_foundry::source::Which::Index(i));
                    if moved {
                        ui.colored_label(
                            mark::TURN,
                            t.fill("weenie.changed_in", &[("pack", &target.name)]),
                        );
                    }
                }
                ui.weak(match origin {
                    Some(StaticOrigin::Base(i)) => t.fill("inspector.base_static", &[("n", &i)]),
                    Some(StaticOrigin::Pack { pack, name, .. }) => name.unwrap_or_else(|| {
                        t.fill(
                            "inspector.pack_unnamed",
                            &[("pack", &self.session.packs()[pack].name)],
                        )
                    }),
                    None => String::new(),
                });
                self.place_fields(
                    ui,
                    &Selection::Static { landblock, index },
                    landblock,
                    &f,
                    None,
                    out,
                );
                if let Some(g) = self.session.ground(landblock, f.origin.x, f.origin.y) {
                    ui.weak(t.fill(
                        "inspector.over_ground",
                        &[("n", &format!("{:+.2}", f.origin.z - g))],
                    ));
                }
                let warn = format!("static {index} (");
                if self
                    .faults
                    .iter()
                    .any(|x| x.at.is_some_and(|(lb, _)| lb == landblock) && x.text.contains(&warn))
                {
                    ui.colored_label(mark::WARN, t.get("inspector.inside_building"));
                }
                ui.horizontal(|ui| {
                    if ui.button(t.get("inspector.onto_ground")).clicked() {
                        out.push(UiAction::SnapStatic);
                    }
                    if ui.button(t.get("inspector.delete")).clicked() {
                        out.push(UiAction::Delete);
                    }
                    let dup = ui
                        .button(t.get("inspector.duplicate"))
                        .on_hover_text(t.get("inspector.duplicate_tip"));
                    self.panels.duplicate = Some(dup.rect);
                    if dup.clicked() {
                        out.push(UiAction::Duplicate);
                    }
                });
            }
            Some(Selection::CellStatic {
                landblock,
                cell,
                index,
            }) => {
                let Some((model, f)) = self.cell_static_frame(landblock, cell, index) else {
                    return;
                };
                ui.strong(t.fill(
                    "inspector.cell_static_title",
                    &[
                        ("n", &index),
                        ("cell", &format!("{cell:04X}")),
                        ("landblock", &format!("{landblock:04X}")),
                    ],
                ));
                ui.label(t.fill(
                    "inspector.model",
                    &[("id", &format!("{:#010X}", model.raw()))],
                ));
                if let Some(label) = self.names.label(model) {
                    ui.weak(label);
                }
                let room = self.cells_of(landblock).get(&cell).map(|c| c.frame);
                self.place_fields(
                    ui,
                    &Selection::CellStatic {
                        landblock,
                        cell,
                        index,
                    },
                    landblock,
                    &f,
                    room.as_ref(),
                    out,
                );
                ui.horizontal(|ui| {
                    if ui.button(t.get("inspector.its_room")).clicked() {
                        out.push(UiAction::Select(Selection::Cell { landblock, cell }));
                    }
                    if ui.button(t.get("inspector.delete")).clicked() {
                        out.push(UiAction::Delete);
                    }
                    let dup = ui
                        .button(t.get("inspector.duplicate"))
                        .on_hover_text(t.get("inspector.duplicate_tip"));
                    self.panels.duplicate = Some(dup.rect);
                    if dup.clicked() {
                        out.push(UiAction::Duplicate);
                    }
                });
            }
            Some(Selection::Cell { landblock, cell }) => {
                let Some(c) = self.cells_of(landblock).get(&cell).cloned() else {
                    return;
                };
                let name = self.session.cell_names(landblock).get(&cell).cloned();
                ui.strong(t.fill(
                    "inspector.cell_title",
                    &[
                        ("cell", &format!("{cell:04X}")),
                        ("landblock", &format!("{landblock:04X}")),
                    ],
                ));
                if let Some(n) = &name {
                    ui.weak(n.rsplit(':').next().unwrap_or(n));
                }
                ui.label(t.fill(
                    "inspector.cell_piece",
                    &[
                        (
                            "piece",
                            &format!("{:04X}/{}", c.environment.raw() & 0xFFFF, c.cell_struct),
                        ),
                        ("portals", &c.portals.len()),
                        ("statics", &c.static_objects.len()),
                    ],
                ));
                ui.monospace(at(c.frame.origin));
                self.objects_in_place(ui, landblock, cell, &c, out);
                if self.dungeon.is_none() {
                    return;
                }
                ui.separator();
                // The piece the room is part of.
                if let Some(piece) = self.room_piece(landblock, cell) {
                    let kit = self
                        .piece_of(piece.environment)
                        .and_then(|i| self.kit.pieces.as_ref().and_then(|p| p.get(i)).cloned());
                    let kind = kit
                        .as_ref()
                        .map(|k| self.kind_name(k.form.kind))
                        .unwrap_or_default();
                    ui.label(t.fill(
                        "inspector.room_piece",
                        &[
                            (
                                "piece",
                                &format!("{:04X}", piece.environment.raw() & 0xFFFF),
                            ),
                            ("kind", &kind),
                            ("parts", &piece.cells.len()),
                        ],
                    ));
                }
                ui.horizontal_wrapped(|ui| {
                    if name.is_some()
                        && ui
                            .button(t.get("inspector.move_room"))
                            .on_hover_text(t.get("inspector.move_room_tip"))
                            .clicked()
                    {
                        out.push(UiAction::LiftRoom(landblock, cell));
                    }
                    let del = ui
                        .button(t.get("inspector.delete_room"))
                        .on_hover_text(t.get("inspector.delete_room_tip"));
                    self.panels.delete_room = Some(del.rect);
                    if del.clicked() {
                        out.push(UiAction::Delete);
                    }
                });
            }
            Some(Selection::Building { landblock, index }) => {
                let Some(b) = self
                    .session
                    .statics(landblock)
                    .and_then(|i| i.buildings.get(index).cloned())
                else {
                    return;
                };
                ui.strong(t.fill(
                    "inspector.building_title",
                    &[("n", &index), ("landblock", &format!("{landblock:04X}"))],
                ));
                ui.label(t.fill(
                    "inspector.model",
                    &[("id", &format!("{:#010X}", b.id.raw()))],
                ));
                ui.weak(t.fill("inspector.portals", &[("n", &b.portals.len())]));
                let dropped = self.dropped_name(landblock, index);
                let sel = Selection::Building { landblock, index };
                self.place_fields(ui, &sel, landblock, &b.frame, None, out);
                match dropped {
                    Some(name) => {
                        ui.label(t.fill("inspector.dropped_building", &[("name", &name)]));
                        if ui.button(t.get("inspector.delete")).clicked() {
                            out.push(UiAction::Delete);
                        }
                    }
                    None => {
                        ui.label(t.get("inspector.building_moves"));
                    }
                }
            }
            Some(Selection::Vertex { gx, gy }) => {
                let Some(v) = self.session.vertex(gx, gy) else {
                    return;
                };
                let (lb, lx, ly) = lyceum_foundry::edit::vertex_owner(gx, gy);
                ui.strong(t.fill("inspector.vertex_title", &[("x", &gx), ("y", &gy)]));
                ui.weak(format!("{lb:04X} ({lx}, {ly})"));
                for (label, field, value) in [
                    (
                        "inspector.height",
                        Field::Height,
                        t.fill(
                            "inspector.height_value",
                            &[("n", &v.height), ("m", &format!("{:.1}", v.z))],
                        ),
                    ),
                    ("inspector.terrain", Field::Terrain, v.terrain.to_string()),
                    ("inspector.scene", Field::Scene, v.scene.to_string()),
                    ("inspector.road", Field::Road, v.road.to_string()),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(t.get(label));
                        if ui.small_button("-").clicked() {
                            out.push(UiAction::Vertex(field, -1));
                        }
                        ui.monospace(value);
                        if ui.small_button("+").clicked() {
                            out.push(UiAction::Vertex(field, 1));
                        }
                    });
                }
            }
            Some(Selection::Spawn { file, .. }) => {
                let Some(s) = self.selected_spawn() else {
                    return;
                };
                let which = self.session.spawn_weenie(&s);
                let name = self
                    .weenies
                    .iter()
                    .find(|w| Some(&w.weenie) == which.as_ref())
                    .map(|w| w.name.clone());
                ui.strong(name.unwrap_or_else(|| t.get("inspector.spawn").into()));
                let lb = (s.cell >> 16) as u16;
                if s.server {
                    ui.weak(t.get("inspector.server_spawn"));
                } else if file == self.session.override_file(lb) {
                    // The server's own spawn, which the pack places again.
                    ui.colored_label(
                        mark::TURN,
                        t.fill(
                            "weenie.changed_in",
                            &[("pack", &self.session.packs()[self.session.target()].name)],
                        ),
                    );
                } else {
                    ui.weak(
                        file.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                }
                ui.label(t.fill(
                    "inspector.weenie_guid",
                    &[("weenie", &s.weenie), ("guid", &s.guid)],
                ));
                ui.monospace(t.fill("inspector.cell", &[("id", &format!("{:#010X}", s.cell))]));
                let room = (s.cell & 0xFFFF >= u32::from(lyceum_foundry::cells::FIRST_CELL))
                    .then(|| {
                        let c = u16::try_from(s.cell & 0xFFFF).unwrap_or(0);
                        self.cells_of(lb).get(&c).map(|c| c.frame)
                    })
                    .flatten();
                let sel = Selection::Spawn {
                    file: file.clone(),
                    guid: s.guid.clone(),
                };
                self.place_fields(ui, &sel, lb, &s.frame, room.as_ref(), out);
                if let Some(to) = self.portal_destination(&s) {
                    self.destination_card(ui, to, out);
                }
                ui.weak(match s.setup {
                    Some(id) => t.fill(
                        "inspector.drawn_as",
                        &[("id", &format!("{:#010X}", id.raw()))],
                    ),
                    None => t.get("inspector.setup_unknown").into(),
                });
                ui.horizontal(|ui| {
                    if ui.button(t.get("inspector.its_weenie")).clicked() {
                        out.push(UiAction::ShowWeenie);
                    }
                    if ui.button(t.get("inspector.delete")).clicked() {
                        out.push(UiAction::Delete);
                    }
                    let dup = ui
                        .button(t.get("inspector.duplicate"))
                        .on_hover_text(t.get("inspector.duplicate_tip"));
                    self.panels.duplicate = Some(dup.rect);
                    if dup.clicked() {
                        out.push(UiAction::Duplicate);
                    }
                });
                if let Some(g) = self.spawn_generator(&s) {
                    self.generator_card(ui, &g);
                }
            }
        }
    }

    /// What stands in room `cell` of landblock `lb`: each static and spawn by name, with its place
    /// in the room (meters from the room's origin, along the room's own axes) and its heading
    /// there; a click selects it.
    fn objects_in_place(
        &mut self,
        ui: &mut egui::Ui,
        lb: u16,
        cell: u16,
        c: &dereth_assets::world::EnvCell,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let id = (u32::from(lb) << 16) | u32::from(cell);
        let spawns: Vec<lyceum_foundry::edit::Spawn> = self
            .session
            .spawns()
            .into_iter()
            .filter(|s| s.cell == id)
            .collect();
        ui.separator();
        ui.strong(t.fill(
            "inspector.in_place",
            &[("n", &(c.static_objects.len() + spawns.len()))],
        ));
        self.panels.in_place.clear();
        if c.static_objects.is_empty() && spawns.is_empty() {
            ui.weak(t.get("inspector.in_place_none"));
            return;
        }
        let room = c.frame;
        let place = |f: &Frame| {
            let p = globaltolocal(&room, f.origin);
            let h = (gizmo::heading_of(f.rotation) - gizmo::heading_of(room.rotation))
                .rem_euclid(360.0);
            t.fill(
                "inspector.in_place_at",
                &[
                    ("x", &format!("{:.2}", p.x)),
                    ("y", &format!("{:.2}", p.y)),
                    ("z", &format!("{:.2}", p.z)),
                    ("h", &format!("{h:.0}")),
                ],
            )
        };
        let mut rows: Vec<(Selection, String, String)> = Vec::new();
        for (index, o) in c.static_objects.iter().enumerate() {
            let name = self
                .names
                .label(o.id)
                .unwrap_or_else(|| format!("{:#010X}", o.id.raw()));
            rows.push((
                Selection::CellStatic {
                    landblock: lb,
                    cell,
                    index,
                },
                name,
                place(&o.frame),
            ));
        }
        for s in &spawns {
            let which = self.session.spawn_weenie(s);
            let name = self
                .weenies
                .iter()
                .find(|w| Some(&w.weenie) == which.as_ref())
                .map_or_else(|| s.weenie.clone(), |w| w.name.clone());
            rows.push((
                Selection::Spawn {
                    file: s.file.clone(),
                    guid: s.guid.clone(),
                },
                name,
                place(&s.frame),
            ));
        }
        egui::Grid::new(("in place", lb, cell))
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (sel, name, at) in rows {
                    let r = ui.selectable_label(false, name);
                    let a = ui.monospace(at);
                    ui.end_row();
                    self.panels
                        .in_place
                        .push((sel.clone(), r.rect.union(a.rect)));
                    if r.clicked() {
                        out.push(UiAction::Select(sel));
                    }
                }
            });
    }

    /// The selection's place as numbers, each to type or step: in its room (`room`, the room's
    /// frame) along the room's own axes, else in its landblock; and its heading. `frame` is its
    /// frame in landblock `lb`'s meters.
    fn place_fields(
        &mut self,
        ui: &mut egui::Ui,
        sel: &Selection,
        lb: u16,
        frame: &Frame,
        room: Option<&Frame>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let (p, h) = match room {
            Some(r) => (
                globaltolocal(r, frame.origin),
                gizmo::heading_of(frame.rotation) - gizmo::heading_of(r.rotation),
            ),
            None => (frame.origin, gizmo::heading_of(frame.rotation)),
        };
        let now = [p.x, p.y, p.z, h.rem_euclid(360.0)];
        ui.weak(t.get(if room.is_some() {
            "inspector.place_in_room"
        } else {
            "inspector.place_in_landblock"
        }));
        let steps = (self.panels.step, self.panels.turn_step);
        let mut set: Option<[f32; 4]> = None;
        egui::Grid::new(("place", format!("{sel:?}")))
            .num_columns(4)
            .show(ui, |ui| {
                for (k, key) in [
                    "inspector.x",
                    "inspector.y",
                    "inspector.z",
                    "inspector.heading_field",
                ]
                .into_iter()
                .enumerate()
                {
                    let step = if k == 3 { steps.1 } else { steps.0 };
                    ui.label(t.get(key));
                    let minus = ui.small_button("-");
                    if minus.clicked() {
                        let mut v = now;
                        v[k] -= step;
                        set = Some(v);
                    }
                    let mut v = match self.panels.place_drag {
                        Some((f, v)) if f == k => v,
                        _ => now[k],
                    };
                    let r = ui.add(
                        egui::DragValue::new(&mut v)
                            .speed(step / 10.0)
                            .fixed_decimals(if k == 3 { 1 } else { 2 }),
                    );
                    if r.dragged() {
                        self.panels.place_drag = Some((k, v));
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.panels.place_drag = None;
                        let mut all = now;
                        all[k] = v;
                        set = Some(all);
                    }
                    let plus = ui.small_button("+");
                    if plus.clicked() {
                        let mut v = now;
                        v[k] += step;
                        set = Some(v);
                    }
                    self.panels
                        .place_rects
                        .push((k, r.rect, minus.rect, plus.rect));
                    ui.end_row();
                }
            });
        ui.horizontal_wrapped(|ui| {
            ui.label(t.get("inspector.step"));
            for m in [0.1f32, 0.5, 1.0, 5.0] {
                if ui
                    .selectable_label((self.panels.step - m).abs() < 1e-3, format!("{m}"))
                    .clicked()
                {
                    self.panels.step = m;
                }
            }
            ui.label(t.get("inspector.turn_step"));
            for d in [5.0f32, 15.0, 45.0, 90.0] {
                if ui
                    .selectable_label((self.panels.turn_step - d).abs() < 1e-3, format!("{d}°"))
                    .clicked()
                {
                    self.panels.turn_step = d;
                }
            }
        });
        if let Some([x, y, z, hd]) = set {
            let (at, heading) = match room {
                Some(r) => (
                    localtoglobal(r, Vec3::new(x, y, z)),
                    hd + gizmo::heading_of(r.rotation),
                ),
                None => (Vec3::new(x, y, z), hd),
            };
            out.push(UiAction::SetPlace(
                sel.clone(),
                lb,
                at,
                heading.rem_euclid(360.0),
            ));
        }
    }

    /// A portal's destination on its card: where it sends who uses it, and the choices: here
    /// (where the camera is), or a dungeon (at its drop point).
    fn destination_card(&mut self, ui: &mut egui::Ui, to: Option<u32>, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let dungeons = self.dungeons();
        let shown = to.map_or_else(
            || t.get("fields.nowhere").to_owned(),
            |c| {
                let lb = u16::try_from(c >> 16).unwrap_or(0);
                dungeons
                    .iter()
                    .find(|d| d.landblock == lb && c & 0xFFFF >= 0x0100)
                    .map_or_else(|| format!("0x{c:08X}"), |d| self.dungeon_name(d))
            },
        );
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.strong(t.get("fields.destination"));
            ui.label(shown);
        });
        ui.horizontal(|ui| {
            if ui
                .button(t.get("fields.destination_here"))
                .on_hover_text(t.get("fields.destination_here_about"))
                .clicked()
            {
                out.push(UiAction::DestinationHere);
            }
            egui::ComboBox::from_id_salt("portal destination")
                .selected_text(t.get("fields.destination_dungeon"))
                .height(240.0)
                .show_ui(ui, |ui| {
                    for d in &dungeons {
                        if ui.selectable_label(false, self.dungeon_name(d)).clicked() {
                            out.push(UiAction::DestinationDungeon(d.landblock));
                        }
                    }
                });
        });
    }

    fn checks_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let faults = self.faults.iter().filter(|f| !f.warning).count();
        let warnings = self.faults.len() - faults;
        ui.strong(self.text.table().fill(
            "checks.count",
            &[("faults", &faults), ("warnings", &warnings)],
        ));
        self.fault_rects.clear();
        for f in &self.faults {
            let text = egui::RichText::new(&f.text).color(if f.warning {
                mark::WARN
            } else {
                mark::FAULT
            });
            let r = ui.selectable_label(false, text);
            self.fault_rects.push(r.rect);
            if r.clicked() {
                if let Some((lb, at)) = f.at {
                    out.push(UiAction::GoTo(lb, at));
                } else if let Some(w) = f.text.split_once(".sql:").and_then(|(p, _)| {
                    self.session
                        .weenie_of_file(std::path::Path::new(&format!("{p}.sql")))
                }) {
                    // A weenie's finding: its form.
                    out.push(UiAction::Weenie(w));
                    out.push(UiAction::ShowWeenie);
                }
            }
        }
    }

    // -------------------------------------------------------------------------------------------
    // the marks on the world
    // -------------------------------------------------------------------------------------------

    /// The selection's box, the handles and the brush.
    fn world_marks(&mut self, painter: &egui::Painter) {
        self.building_note = None;
        let Some(view) = self.view() else {
            return;
        };
        let line = |a: (f32, f32), b: (f32, f32), w: f32, c: egui::Color32| {
            painter.line_segment(
                [egui::pos2(a.0, a.1), egui::pos2(b.0, b.1)],
                egui::Stroke::new(w, c),
            );
        };
        if self.panels.show_grid {
            if let Some((lb, ..)) = self.work_block() {
                for x in 0..=8u32 {
                    for y in 0..=8u32 {
                        let gx = u32::from(lb >> 8) * 8 + x;
                        let gy = u32::from(lb & 255) * 8 + y;
                        for (dx, dy) in [(1, 0), (0, 1)] {
                            if x + dx > 8 || y + dy > 8 {
                                continue;
                            }
                            if let (Some(a), Some(b)) = (
                                self.session.vertex(gx, gy),
                                self.session.vertex(gx + dx, gy + dy),
                            ) {
                                #[allow(clippy::cast_precision_loss)]
                                let points = (
                                    Vec3::new(gx as f32 * 24.0, gy as f32 * 24.0, a.z + 0.05),
                                    Vec3::new(
                                        (gx + dx) as f32 * 24.0,
                                        (gy + dy) as f32 * 24.0,
                                        b.z + 0.05,
                                    ),
                                );
                                if let (Some(a), Some(b)) =
                                    (view.project(points.0), view.project(points.1))
                                {
                                    line(a, b, 1.0, mark::OUTLINE);
                                }
                            }
                        }
                    }
                }
            }
        }
        // A selected generator: the ground it scatters over, the places it makes things at,
        // and lines to what it owns.
        self.generator_marks = super::GeneratorMarks::default();
        if let Some(s) = self.selected_spawn() {
            if let Some(g) = self.session.generator(&s) {
                self.draw_generator(painter, &view, &s, &g);
            }
        }
        // The rest of a selection of several: their boxes and marks.
        for s in self.also.clone() {
            match s {
                Selection::Static { landblock, index } => {
                    if let Some((model, f)) = self.static_frame(landblock, index) {
                        self.model_box(painter, &view, landblock, model, &f);
                    }
                }
                Selection::CellStatic {
                    landblock,
                    cell,
                    index,
                } => {
                    if let Some((model, f)) = self.cell_static_frame(landblock, cell, index) {
                        self.model_box(painter, &view, landblock, model, &f);
                    }
                }
                Selection::Spawn { file, guid } => {
                    if let Some(sp) = self
                        .session
                        .spawns()
                        .into_iter()
                        .find(|x| x.file == file && x.guid == guid)
                    {
                        let o = add(pick::block_origin(sp.landblock()), sp.frame.origin);
                        let top = Vec3::new(o.x, o.y, o.z + 2.0);
                        if let (Some(a), Some(b)) = (view.project(o), view.project(top)) {
                            line(a, b, 3.0, mark::SELECT);
                        }
                    }
                }
                _ => {}
            }
        }
        if self.dungeon.is_some() {
            self.dungeon_marks(painter, &view);
        }
        // A box being drawn.
        if let Some(from) = self.box_from {
            let p = self.input.pointer;
            let (dx, dy) = (p.0 - from.0, p.1 - from.1);
            if dx * dx + dy * dy >= super::BOX_MIN * super::BOX_MIN {
                painter.rect_stroke(
                    egui::Rect::from_two_pos(egui::pos2(from.0, from.1), egui::pos2(p.0, p.1)),
                    0.0,
                    egui::Stroke::new(1.5, mark::SELECT),
                    egui::StrokeKind::Middle,
                );
            }
        }
        match self.selection.clone() {
            Some(Selection::Static { landblock, index }) => {
                if let Some((model, f)) = self.static_frame(landblock, index) {
                    self.model_box(painter, &view, landblock, model, &f);
                }
                self.draw_handles(painter);
            }
            Some(Selection::CellStatic {
                landblock,
                cell,
                index,
            }) => {
                if let Some((model, f)) = self.cell_static_frame(landblock, cell, index) {
                    self.model_box(painter, &view, landblock, model, &f);
                }
                self.draw_handles(painter);
            }
            Some(Selection::Cell { landblock, cell }) => {
                if self.panels.show_bounds {
                    self.outline_cell(painter, &view, landblock, cell, 2.0, mark::SELECT);
                }
            }
            Some(Selection::Building { landblock, index }) => {
                if let Some(b) = self
                    .session
                    .statics(landblock)
                    .and_then(|i| i.buildings.get(index).cloned())
                {
                    self.model_box(painter, &view, landblock, b.id, &b.frame);
                    self.draw_handles(painter);
                    // What goes with it, said beside it.
                    let at = add(pick::block_origin(landblock), b.frame.origin);
                    let note = match self.dropped_name(landblock, index) {
                        Some(name) => self
                            .text
                            .table()
                            .fill("inspector.dropped_building", &[("name", &name)]),
                        None => self.text.table().get("inspector.building_moves").to_owned(),
                    };
                    if let Some((x, y)) = view.project(at) {
                        let shown = egui::Area::new(egui::Id::new("building moves"))
                            .order(egui::Order::Tooltip)
                            .fixed_pos(egui::pos2(x + 12.0, y + 12.0))
                            .interactable(false)
                            .show(painter.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.set_max_width(220.0);
                                    ui.label(&note);
                                });
                            });
                        self.building_note = Some(shown.response.rect);
                    }
                }
            }
            Some(Selection::Vertex { gx, gy }) => {
                if let Some(v) = self.session.vertex(gx, gy) {
                    #[allow(clippy::cast_precision_loss)]
                    let p = Vec3::new(gx as f32 * 24.0, gy as f32 * 24.0, v.z);
                    if let Some((x, y)) = view.project(p) {
                        painter.rect_filled(
                            egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(10.0, 10.0)),
                            0.0,
                            mark::SELECT,
                        );
                    }
                }
            }
            Some(Selection::Spawn { .. }) => {
                if let Some(s) = self.selected_spawn() {
                    let o = add(pick::block_origin(s.landblock()), s.frame.origin);
                    let top = Vec3::new(o.x, o.y, o.z + 2.0);
                    if let (Some(a), Some(b)) = (view.project(o), view.project(top)) {
                        line(a, b, 3.0, mark::SELECT);
                    }
                }
                self.draw_handles(painter);
            }
            None => {}
        }
        // The pointer as this frame's events left it, in the window's pixels.
        let scale = self.gui.scale();
        let pointer = painter
            .ctx()
            .input(|i| i.pointer.latest_pos())
            .map_or(self.input.pointer, |p| (p.x * scale, p.y * scale));
        // The brush is drawn only while the pointer is over the world, not the panels.
        let in_world = self
            .world_rect
            .is_none_or(|r| r.contains(egui::pos2(pointer.0 / scale, pointer.1 / scale)));
        self.ghost_note = None;
        if in_world && self.look.is_none() && self.dropping_building() {
            self.draw_drop_ghost(painter, &view, pointer);
        }
        if !in_world {
            self.vertex_marks.clear();
        } else if self.mode == Mode::Terrain && self.look.is_none() && self.panels.brush.by_vertex()
        {
            // Each vertex the brush covers: filled where a dab changes it.
            self.vertex_marks.clear();
            if let Some((x, y)) = self.brush_center(pointer) {
                let changes: Vec<(u32, u32)> = lyceum_foundry::brush::dab(
                    &self.session,
                    self.panels.brush,
                    x,
                    y,
                    self.panels.radius,
                )
                .iter()
                .map(|s| (s.gx, s.gy))
                .collect();
                if matches!(self.panels.brush, Brush::Scene(_)) {
                    let (gx, gy) = lyceum_foundry::brush::nearest_vertex(x, y);
                    let g = lyceum_foundry::brush::GRID;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // a grid point
                    let v = self.session.vertex((gx / g) as u32, (gy / g) as u32);
                    if let Some(v) = v {
                        self.panels.scene_terrain = v.terrain;
                    }
                }
                for (gx, gy) in lyceum_foundry::brush::covered(x, y, self.panels.radius) {
                    let Some(v) = self.session.vertex(gx, gy) else {
                        continue;
                    };
                    let g = lyceum_foundry::brush::GRID;
                    #[allow(clippy::cast_precision_loss)] // a grid point, at most 2,040
                    let at = Vec3::new(gx as f32 * g, gy as f32 * g, v.z + 0.2);
                    let Some(s) = view.project(at) else {
                        continue;
                    };
                    let r =
                        egui::Rect::from_center_size(egui::pos2(s.0, s.1), egui::vec2(9.0, 9.0));
                    let change = changes.contains(&(gx, gy));
                    if change {
                        painter.rect_filled(r, 1.0, mark::BRUSH);
                    } else {
                        painter.rect_stroke(
                            r,
                            1.0,
                            egui::Stroke::new(1.5, mark::BRUSH),
                            egui::StrokeKind::Middle,
                        );
                    }
                    self.vertex_marks.push(((gx, gy), change, s));
                }
            }
        } else if self.mode == Mode::Terrain && self.look.is_none() {
            let p = pointer;
            if let Some((lb, at)) = self.ground_at(p) {
                let o = pick::block_origin(lb);
                let mut ring = Vec::new();
                #[allow(clippy::cast_precision_loss)]
                for i in 0..=48 {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    let (x, y) = (
                        at.x + self.panels.radius * dereth_primitives::num::math::cosf(a),
                        at.y + self.panels.radius * dereth_primitives::num::math::sinf(a),
                    );
                    let z = self
                        .session
                        .ground(lb, x.clamp(0.0, 191.99), y.clamp(0.0, 191.99));
                    if let Some(s) = view.project(add(o, Vec3::new(x, y, z.unwrap_or(at.z) + 0.2)))
                    {
                        ring.push(s);
                    }
                }
                for w in ring.windows(2) {
                    line(w[0], w[1], 2.0, mark::BRUSH);
                }
            }
        }
    }

    /// Generator `g` of spawn `s`, drawn: its ring of scatter, its profiles' own places, and
    /// lines to the instances it owns.
    fn draw_generator(
        &mut self,
        painter: &egui::Painter,
        view: &View,
        s: &lyceum_foundry::edit::Spawn,
        g: &lyceum_foundry::edit::Generator,
    ) {
        let lb = s.landblock();
        let o = pick::block_origin(lb);
        let at = s.frame.origin;
        let here = view.project(add(o, at));
        let stroke = egui::Stroke::new(2.0, mark::GENERATOR);
        if let Some(r) = g.radius.filter(|r| *r > 0.0) {
            let mut ring = Vec::new();
            #[allow(clippy::cast_precision_loss)]
            for i in 0..=48 {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                let (x, y) = (
                    at.x + r * dereth_primitives::num::math::cosf(a),
                    at.y + r * dereth_primitives::num::math::sinf(a),
                );
                let z = self
                    .session
                    .ground(lb, x.clamp(0.0, 191.99), y.clamp(0.0, 191.99))
                    .unwrap_or(at.z);
                if let Some(p) = view.project(add(o, Vec3::new(x, y, z + 0.2))) {
                    ring.push(egui::pos2(p.0, p.1));
                }
            }
            for w in ring.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            self.generator_marks.ring = here.map(|p| (p, r));
        }
        for p in &g.profiles {
            let (Some(origin), true) = (p.origin, p.place & 4 != 0) else {
                continue;
            };
            let base = p
                .cell
                .and_then(|c| u16::try_from(c >> 16).ok())
                .map_or(o, pick::block_origin);
            let Some(q) = view.project(add(base, origin)) else {
                continue;
            };
            let c = egui::pos2(q.0, q.1);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(0.0, -6.0),
                    c + egui::vec2(6.0, 0.0),
                    c + egui::vec2(0.0, 6.0),
                    c + egui::vec2(-6.0, 0.0),
                ],
                mark::GENERATOR,
                egui::Stroke::NONE,
            ));
            if let Some(h) = here {
                painter.line_segment(
                    [egui::pos2(h.0, h.1), c],
                    egui::Stroke::new(1.0, mark::GENERATOR),
                );
            }
            self.generator_marks.places.push(q);
        }
        if let Some(h) = here {
            let spawns = self.session.spawns();
            for kid in &g.children {
                let Some(k) = spawns.iter().find(|x| &x.guid == kid) else {
                    continue;
                };
                let ko = pick::block_origin(k.landblock());
                if let Some(q) = view.project(add(ko, k.frame.origin)) {
                    painter.line_segment(
                        [egui::pos2(h.0, h.1), egui::pos2(q.0, q.1)],
                        egui::Stroke::new(1.0, mark::GENERATOR),
                    );
                    self.generator_marks.children.push(q);
                }
            }
        }
    }

    /// The selection's handles: the turn ring and the three arrows.
    fn draw_handles(&self, painter: &egui::Painter) {
        let Some((h, ..)) = self.selected_handles() else {
            return;
        };
        let p = |a: (f32, f32)| egui::pos2(a.0, a.1);
        for w in h.ring.windows(2) {
            painter.line_segment([p(w[0]), p(w[1])], egui::Stroke::new(2.0, mark::TURN));
        }
        for (k, tip) in &h.arrows {
            let c = match k {
                Handle::X => mark::X,
                Handle::Y => mark::Y,
                _ => mark::Z,
            };
            painter.line_segment([p(h.origin), p(*tip)], egui::Stroke::new(3.0, c));
            painter.rect_filled(
                egui::Rect::from_center_size(p(*tip), egui::vec2(8.0, 8.0)),
                0.0,
                c,
            );
        }
    }

    /// A model's box at `f` in landblock `lb`.
    fn model_box(
        &mut self,
        painter: &egui::Painter,
        view: &View,
        lb: u16,
        model: DataId,
        f: &Frame,
    ) {
        if !self.panels.show_bounds {
            return;
        }
        let Some(m) = self.models.get(self.session.base(), model) else {
            return;
        };
        let mut lo = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in m.triangles.iter().flatten() {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        if m.triangles.is_empty() {
            lo = Vec3::new(-0.5, -0.5, 0.0);
            hi = Vec3::new(0.5, 0.5, 1.0);
        }
        let o = pick::block_origin(lb);
        let corner = |i: usize| {
            let c = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            view.project(add(o, localtoglobal(f, c)))
        };
        let pts: Vec<Option<(f32, f32)>> = (0..8).map(corner).collect();
        for (a, b) in [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ] {
            if let (Some(p), Some(q)) = (pts[a], pts[b]) {
                painter.line_segment(
                    [egui::pos2(p.0, p.1), egui::pos2(q.0, q.1)],
                    egui::Stroke::new(1.0, mark::OUTLINE),
                );
            }
        }
    }
}

/// A brush's icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Icon {
    Raise,
    Lower,
    Flatten,
    Smooth,
}

/// A square button showing `icon`, lit when `on`.
fn icon_button(ui: &mut egui::Ui, icon: Icon, on: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if on {
        egui::Color32::from_rgb(40, 60, 90)
    } else if r.hovered() {
        egui::Color32::from_gray(55)
    } else {
        egui::Color32::from_gray(38)
    };
    painter.rect_filled(rect, 4.0, fill);
    let c = rect.center();
    let line = egui::Stroke::new(2.0, egui::Color32::from_gray(225));
    let ground = egui::Stroke::new(2.0, egui::Color32::from_rgb(140, 190, 110));
    match icon {
        Icon::Raise | Icon::Lower => {
            // A mound or a dip over the ground, and an arrow up or down.
            let up = icon == Icon::Raise;
            let sign = if up { -1.0 } else { 1.0 };
            painter.line_segment(
                [c + egui::vec2(-10.0, 8.0), c + egui::vec2(10.0, 8.0)],
                ground,
            );
            painter.line_segment(
                [
                    c + egui::vec2(0.0, 5.0 * -sign),
                    c + egui::vec2(0.0, 9.0 * sign),
                ],
                line,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(0.0, 11.0 * sign),
                    c + egui::vec2(-5.0, 4.0 * sign),
                    c + egui::vec2(5.0, 4.0 * sign),
                ],
                egui::Color32::from_gray(225),
                egui::Stroke::NONE,
            ));
        }
        Icon::Flatten => {
            painter.line_segment(
                [c + egui::vec2(-11.0, 2.0), c + egui::vec2(11.0, 2.0)],
                ground,
            );
            painter.line_segment(
                [c + egui::vec2(-7.0, -5.0), c + egui::vec2(7.0, -5.0)],
                line,
            );
            painter.line_segment(
                [c + egui::vec2(-7.0, -5.0), c + egui::vec2(-7.0, -1.0)],
                line,
            );
            painter.line_segment([c + egui::vec2(7.0, -5.0), c + egui::vec2(7.0, -1.0)], line);
        }
        Icon::Smooth => {
            let wave: Vec<egui::Pos2> = (0..=16)
                .map(|k| {
                    #[allow(clippy::cast_precision_loss)]
                    let x = -11.0 + 22.0 * (k as f32) / 16.0;
                    let y = 2.0
                        + 4.0
                            * dereth_primitives::num::math::sinf(x * 0.6)
                            * (1.0 - (x + 11.0) / 26.0);
                    c + egui::vec2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(wave, ground));
        }
    }
    r
}

/// A search box: a magnifier, then the field, its hint `hint` while empty.
fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 18.0), egui::Sense::hover());
        let c = r.center() - egui::vec2(1.5, 1.5);
        let stroke = egui::Stroke::new(1.6, ui.visuals().text_color());
        ui.painter().circle_stroke(c, 5.0, stroke);
        ui.painter()
            .line_segment([c + egui::vec2(3.6, 3.6), c + egui::vec2(7.5, 7.5)], stroke);
        ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        )
    })
    .inner
}

/// A five-pointed star in `r`: filled gold for a favorite, an outline otherwise.
fn draw_star(painter: &egui::Painter, r: egui::Rect, filled: bool, hot: bool) {
    let c = r.center();
    let (outer, inner) = (r.width() * 0.48, r.width() * 0.2);
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let a = std::f32::consts::PI * (k as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
            let len = if k % 2 == 0 { outer } else { inner };
            c + egui::vec2(
                dereth_primitives::num::math::cosf(a) * len,
                dereth_primitives::num::math::sinf(a) * len,
            )
        })
        .collect();
    let gold = egui::Color32::from_rgb(0xF0, 0xC0, 0x30);
    if filled {
        // Filled as five triangles round its middle, each convex.
        for k in 0..5 {
            let (a, b, d) = (pts[2 * k], pts[2 * k + 1], pts[(2 * k + 9) % 10]);
            painter.add(egui::Shape::convex_polygon(
                vec![c, d, a, b],
                gold,
                egui::Stroke::NONE,
            ));
        }
    } else {
        let line = if hot {
            gold
        } else {
            egui::Color32::from_gray(200)
        };
        painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.3, line)));
    }
}

/// A cell of a picker: a picture (or an empty slot) with a caption of up to two lines under it,
/// lit when chosen or under the pointer. The cell's response, and where its picture is.
fn picker_cell(
    ui: &mut egui::Ui,
    cell: egui::Vec2,
    picture: Option<egui::TextureId>,
    caption: &str,
    chosen: bool,
) -> (egui::Response, egui::Rect) {
    let side = crate::thumbs::SIZE as f32;
    let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
    let at = egui::Rect::from_min_size(
        egui::pos2(rect.center().x - side / 2.0, rect.min.y),
        egui::vec2(side, side),
    );
    let painter = ui.painter_at(rect);
    if chosen || r.hovered() {
        painter.rect_filled(
            rect,
            3.0,
            if chosen {
                egui::Color32::from_rgb(40, 60, 90)
            } else {
                egui::Color32::from_gray(45)
            },
        );
    }
    match picture {
        Some(id) => {
            painter.image(
                id,
                at,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            painter.rect_filled(at, 2.0, egui::Color32::from_gray(20));
        }
    }
    let galley = painter.layout(
        caption.to_owned(),
        egui::FontId::proportional(10.0),
        egui::Color32::from_gray(210),
        cell.x - 2.0,
    );
    let x = rect.center().x - galley.size().x.min(cell.x) / 2.0;
    painter.galley(egui::pos2(x, at.max.y + 1.0), galley, egui::Color32::WHITE);
    (r, at)
}
