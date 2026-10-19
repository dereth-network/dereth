//! The editor's panels, in egui: the bar of modes and switches along the top, the status along the
//! bottom, and between them docked tabs around the world: the tool of the mode, the inspector of
//! what is selected, and the checks' findings. The world shows through its own tab, and the marks
//! on it (the selection's box, the handles, the brush) are painted under the panels.

use dereth_primitives::frame::localtoglobal;
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
}

/// Colours of the marks on the world.
pub(super) mod mark {
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

/// The docked layout the editor opens with: the tools on the left, the inspector and the map on
/// the right,
/// the checks under the world.
#[must_use]
pub fn default_dock() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::World]);
    let tree = d.main_surface_mut();
    let [world, _tools] = tree.split_left(NodeIndex::root(), 0.24, vec![Tab::Tools]);
    let [world, inspector] = tree.split_right(
        world,
        0.74,
        vec![Tab::Inspector, Tab::Nearby, Tab::Weenie, Tab::Dungeons],
    );
    tree.split_below(inspector, 0.62, vec![Tab::Map]);
    tree.split_below(world, 0.82, vec![Tab::Checks]);
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
                Mode::Select => t.get("tabs.select").into(),
                Mode::Place => t.get("tabs.models").into(),
                Mode::Terrain => t.get("tabs.brush").into(),
                Mode::Spawn => t.get("tabs.spawn").into(),
                Mode::Dungeon => t.get("tabs.dungeon").into(),
            },
            Tab::Inspector => t.get("tabs.inspector").into(),
            Tab::Weenie => t.get("tabs.weenie").into(),
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
            Tab::World => self.pass.world = Some(ui.max_rect()),
            Tab::Tools => self.app.tools_tab(ui, &mut self.pass.actions),
            Tab::Inspector => self.app.inspector_tab(ui, &mut self.pass.actions),
            Tab::Weenie => self.app.weenie_tab(ui, &mut self.pass.actions),
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
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
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
            let painter = ui
                .ctx()
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(rect);
            self.world_marks(&painter);
        }
        pass
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.horizontal(|ui| {
            for (label, tip, m) in [
                ("top.select", "top.select_tip", Mode::Select),
                ("top.place", "top.place_tip", Mode::Place),
                ("top.terrain", "top.terrain_tip", Mode::Terrain),
                ("top.spawn", "top.spawn_tip", Mode::Spawn),
                ("top.dungeon", "top.dungeon_tip", Mode::Dungeon),
            ] {
                if ui
                    .selectable_label(self.mode == m, t.get(label))
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
            if ui
                .selectable_label(
                    self.walking,
                    t.get(if self.walking {
                        "top.walking"
                    } else {
                        "top.flying"
                    }),
                )
                .on_hover_text(t.get("top.walk_tip"))
                .clicked()
            {
                out.push(UiAction::ToggleWalk);
            }
            if ui
                .selectable_label(self.collide, t.get("top.collide"))
                .on_hover_text(t.get("top.collide_tip"))
                .clicked()
            {
                out.push(UiAction::Collide);
            }
            if ui
                .button(t.get("top.snap"))
                .on_hover_text(t.get("top.snap_tip"))
                .clicked()
            {
                out.push(UiAction::Snap);
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
            let (up, starting) = self.live_state();
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
            if ui
                .add_enabled(up, egui::Button::new(t.get("top.play")))
                .on_hover_text(t.get("top.play_tip"))
                .clicked()
            {
                out.push(UiAction::Play);
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
                ui.checkbox(&mut o.full_screen, t.get("top.full_screen"));
                ui.weak(t.get("top.play_options_note"));
                if o != self.play_options {
                    out.push(UiAction::PlayOptions(o));
                }
            });
            if ui
                .add_enabled(
                    self.server_sql.is_some(),
                    egui::Button::new(t.get("top.export_gdle")),
                )
                .on_hover_text(t.get("top.export_gdle_tip"))
                .clicked()
            {
                out.push(UiAction::ExportGdle);
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let place = landblock_of(self.eye()).map_or_else(
            || t.get("status.off_world").to_owned(),
            |(lb, x, y)| format!("{lb:04X} {x:.1}, {y:.1}, {:.1}", self.eye().z),
        );
        let (edits, saved) = (self.session.history().0, !self.session.unsaved());
        ui.horizontal(|ui| {
            ui.monospace(place);
            ui.label(t.get(match self.mode {
                Mode::Select => "status.select",
                Mode::Place => "status.place",
                Mode::Terrain => "status.terrain",
                Mode::Spawn => "status.spawn",
                Mode::Dungeon => "status.dungeon",
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
            ui.label(&self.panels.message);
        });
    }

    fn tools_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        match self.mode {
            Mode::Select => {
                ui.label(self.text.table().get("select.help"));
            }
            Mode::Place => self.browser(ui, out),
            Mode::Terrain => self.brushes(ui, out),
            Mode::Spawn => self.spawn_browser(ui, out),
            Mode::Dungeon => self.dungeon_tab(ui, out),
        }
    }

    /// The spawn browser: every weenie of the packs and the server, by name, kind and picture,
    /// filtered by a typed name or class id and by kind. A click chooses the one the spawn tool
    /// places.
    fn spawn_browser(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.label(t.get("spawn.filter"));
        ui.text_edit_singleline(&mut self.panels.weenie_filter);
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

    /// The model browser: a typed filter (an id or a name), and every model with its picture and
    /// name, those standing around the camera first.
    fn browser(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.label(t.get("models.filter"));
        ui.text_edit_singleline(&mut self.panels.filter);
        // The shelves: every category with something in it, then the user's tags.
        let counts = self.category_counts();
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.panels.category.is_none(), t.get("models.all"))
                .clicked()
            {
                out.push(UiAction::Category(None));
            }
            for c in Category::ALL {
                let Some(n) = counts.get(&c).copied().filter(|n| *n > 0) else {
                    continue;
                };
                let on = self.panels.category == Some(c);
                let name = t.or(&format!("categories.{}", c.name()), c.name());
                if ui.selectable_label(on, format!("{name} {n}")).clicked() {
                    out.push(UiAction::Category((!on).then_some(c)));
                }
            }
        });
        let tags = self.tags.all();
        if !tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.weak(t.get("models.tags"));
                for (t, n) in &tags {
                    let on = self.panels.tag.as_deref() == Some(t.as_str());
                    if ui.selectable_label(on, format!("#{t} {n}")).clicked() {
                        out.push(UiAction::Tag((!on).then(|| t.clone())));
                    }
                }
            });
        }
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
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.panels.on_ground, t.get("models.on_ground"))
                .clicked()
            {
                out.push(UiAction::OnGround(true));
            }
            if ui
                .selectable_label(!self.panels.on_ground, t.get("models.at_height"))
                .clicked()
            {
                out.push(UiAction::OnGround(false));
            }
        });
        if !self.panels.on_ground {
            ui.add(
                egui::DragValue::new(&mut self.panels.height)
                    .speed(0.1)
                    .suffix(t.get("models.height_suffix")),
            );
        }
        if let Some(m) = self.model {
            self.chosen_model(ui, m, out);
        }
        ui.separator();
        // The grid: a picture a cell, its name under it.
        let side = crate::thumbs::SIZE as f32;
        // Room under the picture for two lines of its name.
        let cell = egui::vec2(side + 30.0, side + 28.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few columns
        let columns = ((ui.available_width() / (cell.x + 4.0)).floor().max(1.0) as usize).max(1);
        let rows = list.len().div_ceil(columns);
        let mut budget = super::PICTURES_A_FRAME;
        self.picture_rects.clear();
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
        let words = label.clone().unwrap_or_else(|| format!("{:08X}", m.raw()));
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
        let r = r.on_hover_ui(|ui| {
            ui.strong(label.as_deref().unwrap_or(t.get("models.unused")));
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

    /// The chosen model: what it is, and its tags, which a word typed and Enter adds to.
    fn chosen_model(&mut self, ui: &mut egui::Ui, m: DataId, out: &mut Vec<UiAction>) {
        let tx = self.text.table();
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.monospace(format!("{:#010X}", m.raw()));
            ui.label(self.names.label(m).unwrap_or_default());
        });
        ui.horizontal_wrapped(|ui| {
            for t in self.tags.of(m).to_vec() {
                if ui
                    .small_button(format!("#{t} x"))
                    .on_hover_text(tx.get("models.untag"))
                    .clicked()
                {
                    out.push(UiAction::TagModel(m, t, false));
                }
            }
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.panels.new_tag)
                    .hint_text(tx.get("models.tag_hint"))
                    .desired_width(90.0),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (entered || ui.small_button(tx.get("models.tag_add")).clicked())
                && !self.panels.new_tag.trim().is_empty()
            {
                out.push(UiAction::TagModel(
                    m,
                    std::mem::take(&mut self.panels.new_tag),
                    true,
                ));
            }
        });
    }

    fn brushes(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let b = self.panels.brush;
        ui.horizontal_wrapped(|ui| {
            for (label, brush) in [
                ("brush.raise", Brush::Raise(amount_of(b))),
                ("brush.lower", Brush::Lower(amount_of(b))),
                ("brush.flatten", Brush::Flatten(None)),
                ("brush.smooth", Brush::Smooth),
                ("brush.terrain", Brush::Terrain(value_of(b))),
                ("brush.scene", Brush::Scene(value_of(b))),
                ("brush.road", Brush::Road(1)),
            ] {
                let on = std::mem::discriminant(&b) == std::mem::discriminant(&brush);
                if ui
                    .selectable_label(on, t.get(label))
                    .on_hover_text(t.get(&format!("{label}_tip")))
                    .clicked()
                {
                    out.push(UiAction::Brush(brush));
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(t.get("brush.radius"));
            if ui.small_button("-").clicked() {
                out.push(UiAction::Radius(-1));
            }
            ui.monospace(t.fill(
                "brush.meters",
                &[("n", &format!("{:.0}", self.panels.radius))],
            ));
            if ui.small_button("+").clicked() {
                out.push(UiAction::Radius(1));
            }
            if b.by_vertex() {
                // Centered on a vertex, as the brush is.
                let g = lyceum_foundry::brush::GRID;
                let n =
                    lyceum_foundry::brush::covered(40.0 * g, 40.0 * g, self.panels.radius).len();
                ui.weak(match n {
                    1 => t.get("brush.one_vertex").to_owned(),
                    _ => t.fill("brush.vertices", &[("n", &n)]),
                });
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

    /// The terrain brush's terrains: each type's picture and name; a click paints with it.
    fn terrain_picker(&mut self, ui: &mut egui::Ui, chosen: u16, out: &mut Vec<UiAction>) {
        ui.separator();
        let side = crate::thumbs::SIZE as f32;
        let cell = egui::vec2(side + 30.0, side + 28.0);
        self.terrain_rects.clear();
        egui::ScrollArea::vertical()
            .id_salt("terrains")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (v, name, tex) in self.terrain_types() {
                        let picture = tex.and_then(|t| self.texture_picture(t));
                        let (r, rect) = picker_cell(ui, cell, picture, &name, v == chosen);
                        self.terrain_rects.push((v, rect));
                        let tip = self
                            .text
                            .table()
                            .fill("brush.terrain_type", &[("n", &v), ("name", &name)]);
                        if r.on_hover_text(tip).clicked() {
                            out.push(UiAction::Brush(Brush::Terrain(v)));
                        }
                    }
                });
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
        egui::ScrollArea::vertical()
            .id_salt("scenes")
            .auto_shrink(false)
            .show(ui, |ui| {
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
            ui.monospace(format!("x{:.0}", self.panels.map_zoom));
            if ui.small_button("+").clicked() {
                out.push(UiAction::MapZoom(2.0));
            }
        });
        let avail = ui.available_size();
        let side = avail.x.min(avail.y).max(64.0);
        let (rect, r) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
        #[allow(clippy::cast_precision_loss)] // 2,048
        let full = lyceum_foundry::map::SIDE as f32;
        let span = full / self.panels.map_zoom;
        let cam = self.camera.position;
        let (cx, cy) = lyceum_foundry::map::pixel_of(cam.x, cam.y);
        let x0 = (cx - span / 2.0).clamp(0.0, full - span);
        let y0 = (cy - span / 2.0).clamp(0.0, full - span);
        let view = egui::Rect::from_min_size(egui::pos2(x0, y0), egui::vec2(span, span));
        self.map_rect = Some(rect);
        self.map_view = view;
        let painter = ui.painter_at(rect);
        painter.image(
            tex,
            rect,
            egui::Rect::from_min_max(
                egui::pos2(x0 / full, y0 / full),
                egui::pos2((x0 + span) / full, (y0 + span) / full),
            ),
            egui::Color32::WHITE,
        );
        // The camera: a spot, and a line the way it looks.
        let to_screen = |px: f32, py: f32| {
            egui::pos2(
                rect.min.x + (px - x0) / span * side,
                rect.min.y + (py - y0) / span * side,
            )
        };
        let at = to_screen(cx, cy);
        let f = self.camera.forward();
        let (fx, fy) = (f.x, f.y);
        let len = (fx * fx + fy * fy).sqrt().max(1e-3);
        let tip = egui::pos2(at.x + fx / len * 14.0, at.y - fy / len * 14.0);
        painter.line_segment([at, tip], egui::Stroke::new(2.0, mark::CAMERA));
        painter.circle_filled(at, 4.0, mark::CAMERA);
        // Where the pointer is, and a click flies there.
        let world = |p: egui::Pos2| {
            lyceum_foundry::map::world_of(
                x0 + (p.x - rect.min.x) / side * span,
                y0 + (p.y - rect.min.y) / side * span,
            )
        };
        if let Some(p) = r.hover_pos() {
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
            if let Some(p) = r.interact_pointer_pos() {
                let (x, y) = world(p);
                out.push(UiAction::FlyTo(x, y));
            }
        }
    }

    fn inspector_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        if !self.also.is_empty() {
            ui.strong(t.fill("inspector.selected_n", &[("n", &(self.also.len() + 1))]));
            ui.weak(t.get("inspector.several_help"));
            if ui.button(t.get("inspector.delete_all")).clicked() {
                out.push(UiAction::Delete);
            }
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
        let heading = |r| {
            t.fill(
                "inspector.heading",
                &[("n", &format!("{:.1}", gizmo::heading_of(r)))],
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
                ui.monospace(at(f.origin));
                ui.monospace(heading(f.rotation));
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
                ui.monospace(at(f.origin));
                ui.monospace(heading(f.rotation));
                if ui.button(t.get("inspector.delete")).clicked() {
                    out.push(UiAction::Delete);
                }
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
                if let Some(n) = name {
                    ui.weak(n);
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
                if ui
                    .button(t.get("inspector.find_piece"))
                    .on_hover_text(t.get("inspector.find_piece_tip"))
                    .clicked()
                {
                    out.push(UiAction::FindPiece(c.environment, c.cell_struct));
                }
                let chosen = self
                    .kit
                    .chosen
                    .and_then(|i| self.kit.pieces.as_ref().and_then(|p| p.get(i)))
                    .map(lyceum_foundry::pieces::Piece::label);
                let label = chosen.map_or_else(
                    || t.get("inspector.reshape_none").to_owned(),
                    |p| t.fill("inspector.reshape", &[("piece", &p)]),
                );
                if ui
                    .add_enabled(self.kit.chosen.is_some(), egui::Button::new(label))
                    .on_hover_text(t.get("inspector.reshape_tip"))
                    .clicked()
                {
                    out.push(UiAction::ReplaceCell(landblock, cell));
                }
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
                ui.monospace(at(b.frame.origin));
                ui.weak(t.fill("inspector.portals", &[("n", &b.portals.len())]));
                ui.label(t.get("inspector.building_moves"));
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
                if s.server {
                    ui.weak(t.get("inspector.server_spawn"));
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
                ui.monospace(at(s.frame.origin));
                ui.weak(match s.setup {
                    Some(id) => t.fill(
                        "inspector.drawn_as",
                        &[("id", &format!("{:#010X}", id.raw()))],
                    ),
                    None => t.get("inspector.setup_unknown").into(),
                });
                ui.monospace(heading(s.frame.rotation));
                if let Some(g) = self.session.generator(&s) {
                    let mut said = vec![t.fill("inspector.generator", &[("n", &g.profiles.len())])];
                    if let Some(r) = g.radius {
                        said.push(t.fill("inspector.scatters", &[("n", &r)]));
                    }
                    if !g.children.is_empty() {
                        said.push(t.fill("inspector.owns", &[("n", &g.children.len())]));
                    }
                    ui.label(said.join(", "));
                }
                ui.horizontal(|ui| {
                    if ui.button(t.get("inspector.its_weenie")).clicked() {
                        out.push(UiAction::ShowWeenie);
                    }
                    if ui.button(t.get("inspector.delete")).clicked() {
                        out.push(UiAction::Delete);
                    }
                });
            }
        }
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
        if self.mode == Mode::Dungeon {
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
                self.outline_cell(painter, &view, landblock, cell, 2.0, mark::SELECT);
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
                    if let Some((x, y)) = view.project(at) {
                        let shown = egui::Area::new(egui::Id::new("building moves"))
                            .order(egui::Order::Tooltip)
                            .fixed_pos(egui::pos2(x + 12.0, y + 12.0))
                            .interactable(false)
                            .show(painter.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.set_max_width(220.0);
                                    ui.label(self.text.table().get("inspector.building_moves"));
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
        // The pointer as this frame's events left it.
        let pointer = painter
            .ctx()
            .input(|i| i.pointer.latest_pos())
            .map_or(self.input.pointer, |p| (p.x, p.y));
        // The brush is drawn only while the pointer is over the world, not the panels.
        let in_world = self
            .world_rect
            .is_none_or(|r| r.contains(egui::pos2(pointer.0, pointer.1)));
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
