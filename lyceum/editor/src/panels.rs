//! The editor's panels, in egui: the bar of tools along the top, the status along the bottom, and
//! between them docked tabs ([`crate::docks`]): the world view, and around it the tool of the mode,
//! the map (in dungeon mode, the dungeon's grid and its floors from the south), the lists, the
//! inspector and the log. The world shows through its own tab, and the marks on it (the
//! selection's box, the handles, the brush) are painted under the panels.

use dereth_primitives::frame::{globaltolocal, localtoglobal};
use dereth_primitives::{DataId, Frame, Vec3};
use egui_dock::{DockArea, TabViewer};
use lyceum_foundry::brush::Brush;
use lyceum_foundry::compile::{Field, StaticOrigin};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::models::Category;
use lyceum_foundry::pick;

use super::{add, amount_of, step_brush, value_of, EditorApp, Mode, Selection, UiAction};
use crate::camera::landblock_of;
pub use crate::docks::Tab;
use crate::gizmo::{self, Handle};
use crate::view::View;

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

/// What one pass of the panels found.
#[derive(Debug, Default)]
pub struct Pass {
    pub actions: Vec<UiAction>,
    /// The world tab's rectangle on screen.
    pub world: Option<egui::Rect>,
    /// The keys pressed for the world, each with Ctrl held or not, and whether the pointer was
    /// over the panels.
    pub keys: Vec<(u32, bool)>,
    pub over: bool,
}

/// The tabs' contents, drawn from the editor.
struct Viewer<'a> {
    app: &'a mut EditorApp,
    pass: &'a mut Pass,
}

/// A tab's contents in a vertical scroll that fills the tab, whatever their height.
fn filled(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink(false)
        .show(ui, add);
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
            Tab::Side => t.get("tabs.side").into(),
            Tab::Tool => t.get("tabs.tool").into(),
            Tab::Map => t.get("tabs.map").into(),
            Tab::Grid => t.get("tabs.grid").into(),
            Tab::Dungeons => t.get("tabs.dungeons").into(),
            Tab::Nearby => t.get("tabs.nearby").into(),
            Tab::Definition => t.get("tabs.definition").into(),
            Tab::Editor => match self
                .app
                .editor
                .weenie()
                .and_then(|w| self.app.weenie_entry(w))
            {
                Some(e) => t.fill("tabs.editor_of", &[("name", &e.name)]).into(),
                None => t.get("tabs.editor").into(),
            },
            Tab::Inspector => t.get("tabs.inspector").into(),
            Tab::Hierarchy => t.get("tabs.hierarchy").into(),
            Tab::Characters => t.get("tabs.characters").into(),
            Tab::Quests => t.get("quests.tab").into(),
            Tab::Loot => t.get("loot.tab").into(),
            Tab::Log => {
                let n = self.app.faults.len();
                if n == 0 {
                    t.get("tabs.log").into()
                } else {
                    t.fill("tabs.log_n", &[("n", &n)]).into()
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let out = &mut self.pass.actions;
        match tab {
            Tab::World => {
                self.pass.world = Some(ui.max_rect());
                // The marks drawn after the tabs, and the frame drawn after the panels, take the
                // world view as it is laid out now.
                self.app.world_rect = Some(ui.max_rect());
            }
            Tab::Side => self.app.side_tab(ui, out),
            Tab::Tool => {
                self.app.tool_body = Some(ui.max_rect());
                self.app.tools_tab(ui, out);
            }
            Tab::Map => self.app.map_tab(ui, out),
            Tab::Grid => self.app.grid_ui(ui, out),
            Tab::Dungeons => self.app.dungeons_tab(ui, out),
            Tab::Nearby => self.app.nearby_tab(ui, out),
            Tab::Definition => self.app.definition_tab(ui, out),
            Tab::Editor => self.app.editor_tab(ui, out),
            Tab::Inspector => filled(ui, "inspector", |ui| self.app.inspector_tab(ui, out)),
            Tab::Hierarchy => filled(ui, "hierarchy", |ui| self.app.hierarchy_tab(ui, out)),
            Tab::Log => filled(ui, "log", |ui| self.app.log_tab(ui, out)),
            // Its tabs scroll on their own, each the tab's whole width.
            Tab::Characters => self.app.characters_tab(ui, out),
            Tab::Quests => self.app.quests_tab(ui, out),
            Tab::Loot => self.app.loot_tab(ui, out),
        }
    }

    /// Only the optional tabs close: every other tab is always in the layout.
    fn is_closeable(&self, tab: &Tab) -> bool {
        crate::docks::OPTIONAL.contains(tab)
    }

    fn clear_background(&self, tab: &Tab) -> bool {
        *tab != Tab::World
    }

    /// Each tab scrolls what it holds itself, so a list fills its tab.
    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        *tab != Tab::World
    }
}

/// How long ago a save `ago` seconds since was, as the status bar says it: the key of its words
/// and the number they name.
#[must_use]
pub fn saved_ago(ago: f64) -> (&'static str, u64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // seconds since a save
    let s = ago.max(0.0) as u64;
    match s {
        0..60 => ("status.saved_now", 0),
        60..3600 => ("status.saved_minutes", s / 60),
        _ => ("status.saved_hours", s / 3600),
    }
}

/// The number of edits the status bar counts as not saved: none when the files are as saved, else
/// the steps between the history's place now and where it was saved, one at least.
#[must_use]
pub fn unsaved_edits(unsaved: bool, steps: usize, saved_at: usize) -> usize {
    if unsaved {
        steps.abs_diff(saved_at).max(1)
    } else {
        0
    }
}

impl EditorApp {
    /// One pass of the panels inside the root `ui`: the bars, the docked tabs, and the marks on the
    /// world under them.
    pub(super) fn panels_ui(&mut self, ui: &mut egui::Ui) -> Pass {
        let mut pass = Pass::default();
        egui::Panel::top("bar").show(ui, |ui| self.top_bar(ui, &mut pass.actions));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui, &mut pass.actions));
        // Outdoors and in dungeon mode each have a layout of their own.
        let in_dungeon = self.dungeon.is_some();
        let mut dock = if in_dungeon {
            self.dungeon_dock
                .take()
                .unwrap_or_else(crate::docks::in_dungeon)
        } else {
            self.dock.take().unwrap_or_else(crate::docks::outdoors)
        };
        // The editor, when open, in whichever layout is shown.
        let editor_shown = self.editor_tab_in(&mut dock);
        DockArea::new(&mut dock)
            .show_close_buttons(true)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show_inside(
                ui,
                &mut Viewer {
                    app: self,
                    pass: &mut pass,
                },
            );
        // Closed with its ×, or still open.
        if editor_shown {
            self.editor.open = crate::docks::holds(&dock, Tab::Editor);
        }
        if in_dungeon {
            self.dungeon_dock = Some(dock);
        } else {
            if self.fit_map {
                self.fit_to_map(&mut dock, ui.spacing());
            }
            self.dock = Some(dock);
        }
        // The world view laid out, the camera moves: the marks are drawn from where the world is.
        self.camera_step(ui.ctx(), &mut pass);
        if let Some(rect) = pass.world {
            // The marks on the world are drawn in the window's pixels, as the world is; the
            // words over it in the panels' points.
            let scale = self.gui.scale();
            let marks_layer = egui::LayerId::new(egui::Order::Background, egui::Id::new("marks"));
            ui.ctx().set_transform_layer(
                marks_layer,
                egui::emath::TSTransform::from_scaling(1.0 / scale),
            );
            // Clipped to the world view in pixels alone: a layer's painter starts clipped to the
            // window in points, which at a scale over 100% would cut the marks off short of the
            // view's right and bottom edges.
            let mut marks = ui.ctx().layer_painter(marks_layer);
            marks.set_clip_rect(egui::Rect::from_min_max(rect.min * scale, rect.max * scale));
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
        self.delete_dungeon_question(ui.ctx(), &mut pass.actions);
        self.creator_dialog(ui.ctx(), &mut pass.actions);
        pass
    }

    /// Fit the default layout's upper right part to the map, as soon as it is laid out: as tall
    /// as the map's square, its zoom over it, needs at the part's width. The Tool tab shares the
    /// part and is shown first; its body is the map's.
    fn fit_to_map(&mut self, dock: &mut egui_dock::DockState<Tab>, spacing: &egui::style::Spacing) {
        let Some(body) = self.tool_body.filter(|b| b.width() > 1.0) else {
            return;
        };
        self.fit_map = false;
        let tree = dock.main_surface();
        let shared = matches!(
            (tree.find_tab(&Tab::Tool), tree.find_tab(&Tab::Map)),
            (Some((a, _)), Some((b, _))) if a == b
        );
        if shared {
            let wanted = body.width() + spacing.interact_size.y + spacing.item_spacing.y;
            crate::docks::grow(dock, Tab::Map, wanted - body.height());
        }
    }

    /// The bar of tools: the four tools, undo and redo, play, the editor's own server and the
    /// settings, and save; each an icon named by its tooltip.
    fn top_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        egui::ScrollArea::horizontal()
            .id_salt("mode-bar")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let in_dungeon = self.dungeon.is_some();
                    self.tool_rects.clear();
                    for (icon, label, m) in [
                        (Icon::Select, "top.select", Mode::Select),
                        (Icon::Place, "top.place", Mode::Place),
                        (Icon::Spawn, "top.spawn", Mode::Spawn),
                        (Icon::Terrain, "top.terrain", Mode::Terrain),
                    ] {
                        // A dungeon has no land to paint.
                        let enabled = !(in_dungeon && m == Mode::Terrain);
                        let r = tip(tool_button(ui, icon, self.mode == m, enabled), t.get(label));
                        self.tool_rects.push((m, r.rect));
                        if r.clicked() {
                            out.push(UiAction::Mode(m));
                        }
                    }
                    ui.separator();
                    let (undo, redo) = self.session.history();
                    if tip(
                        tool_button(ui, Icon::Undo, false, undo > 0),
                        t.get("top.undo"),
                    )
                    .clicked()
                    {
                        out.push(UiAction::Undo);
                    }
                    if tip(
                        tool_button(ui, Icon::Redo, false, redo > 0),
                        t.get("top.redo"),
                    )
                    .clicked()
                    {
                        out.push(UiAction::Redo);
                    }
                    ui.separator();
                    if self.server_sql.is_none() {
                        let fetching = self.fetching.is_some();
                        let r = tip(
                            tool_button(ui, Icon::Download, false, !fetching),
                            t.get(if fetching {
                                "top.database_fetching"
                            } else {
                                "top.database_fetch_tip"
                            }),
                        );
                        if r.clicked() {
                            out.push(UiAction::FetchDatabase);
                        }
                    }
                    let (up, starting) = self.live_state();
                    if self.playing.is_some() || self.waiting_play.is_some() {
                        let stop = tip(tool_button(ui, Icon::Stop, true, true), t.get("top.stop"));
                        self.play_button = Some(stop.rect);
                        if stop.clicked() {
                            out.push(UiAction::StopPlay);
                        }
                    } else {
                        let play = tip(
                            tool_button(ui, Icon::Play, false, up),
                            t.get("top.play_tip"),
                        );
                        self.play_button = Some(play.rect);
                        if play.clicked() {
                            out.push(UiAction::Play);
                        }
                        self.play_menu(ui, out);
                    }
                    let live = tip(
                        tool_button(ui, Icon::Live, up || starting, true),
                        t.get(if starting {
                            "top.live_starting"
                        } else {
                            "top.live_tip"
                        }),
                    );
                    if live.clicked() {
                        out.push(UiAction::Live(!(up || starting)));
                    }
                    self.settings_menu(ui, out);
                    ui.separator();
                    let save = tip(
                        tool_button(ui, Icon::Save, false, self.session.unsaved()),
                        t.get("top.save"),
                    );
                    if save.clicked() {
                        out.push(UiAction::Save);
                    }
                    ui.separator();
                    self.speed_control(ui, out);
                });
            });
    }

    /// Play's menu, a small arrow beside it: the world's character recipes, the one Play plays
    /// as ticked, and the Characters tab.
    fn play_menu(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        use egui::containers::menu::MenuButton;
        let t = self.text.table();
        let (r, _) = MenuButton::from_button(
            egui::Button::selectable(false, "").min_size(egui::vec2(MENU_ARROW_WIDTH, BUTTON.y)),
        )
        .ui(ui, |ui| {
            for name in &self.characters.names {
                if ui.radio(self.characters.name == *name, name).clicked() {
                    out.push(UiAction::ChooseRecipe(name.clone()));
                    ui.close();
                }
            }
            if !self.characters.names.is_empty() {
                ui.separator();
            }
            if ui.button(t.get("top.characters")).clicked() {
                out.push(UiAction::OpenCharacters);
                ui.close();
            }
        });
        let color = ui.style().interact_selectable(&r, false).fg_stroke.color;
        draw_icon(ui.painter(), r.rect, Icon::Menu, color);
        self.play_menu_button = Some(r.rect);
        tip(r, t.get("top.play_menu_tip"));
    }

    /// The settings: the playtest's interface, walking or flying, the key hints, and the panels'
    /// scale.
    fn settings_menu(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        use egui::containers::menu::{MenuButton, SubMenuButton};
        let t = self.text.table();
        let open = self.settings_open;
        let (r, menu) = MenuButton::from_button(
            egui::Button::selectable(open, "").min_size(BUTTON),
        )
        .ui(ui, |ui| {
            SubMenuButton::from_button(egui::Button::new(t.get("top.interface")).right_text(">"))
                .ui(ui, |ui| {
                    let mut o = self.play_options;
                    for i in super::PlayInterface::ALL {
                        ui.radio_value(
                            &mut o.interface,
                            i,
                            t.or(&format!("top.{}", i.word()), i.name()),
                        );
                    }
                    if o != self.play_options {
                        out.push(UiAction::PlayOptions(o));
                    }
                });
            let mut walk = self.walking;
            if ui.checkbox(&mut walk, t.get("top.walk_mode")).changed() {
                out.push(UiAction::ToggleWalk);
            }
            ui.checkbox(&mut self.panels.show_hints, t.get("views.hints"));
            SubMenuButton::from_button(egui::Button::new(t.get("views.ui_scale")).right_text(">"))
                .ui(ui, |ui| {
                    let now = self.gui.scale();
                    for scale in crate::gui::SCALES {
                        let label = t.fill("views.ui_scale_n", &[("n", &super::percent(scale))]);
                        if ui.radio(now == scale, label).clicked() {
                            out.push(UiAction::UiScale(scale));
                        }
                    }
                    ui.weak(t.get("views.ui_scale_keys"));
                });
        });
        // Lit only while its menu is open.
        self.settings_open = menu.is_some();
        let color = ui.style().interact_selectable(&r, open).fg_stroke.color;
        draw_icon(ui.painter(), r.rect, Icon::Settings, color);
        self.settings_button = Some(r.rect);
        let _ = r.on_hover_text(t.get("top.settings"));
    }

    /// The flying speed, a step slower or faster; indoors and outdoors each have their own.
    fn speed_control(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let last = crate::camera::SPEEDS.len() - 1;
        if tip(
            ui.add_enabled(self.camera.speed > 0, egui::Button::new("-")),
            t.get("top.slower"),
        )
        .clicked()
        {
            out.push(UiAction::Speed(-1));
        }
        ui.monospace(t.fill(
            "top.speed_value",
            &[("n", &format!("{:>3}", self.camera.meters_per_second()))],
        ))
        .on_hover_text(t.get("top.speed_tip"));
        if tip(
            ui.add_enabled(self.camera.speed < last, egui::Button::new("+")),
            t.get("top.faster"),
        )
        .clicked()
        {
            out.push(UiAction::Speed(1));
        }
    }

    /// Where the status bar says the eye is: its landblock and its meters in it. In a dungeon,
    /// the dungeon's landblock, whose rooms may lie beyond its own square.
    #[must_use]
    pub fn status_place(&self) -> Option<(u16, Vec3)> {
        let eye = self.eye();
        let lb = match self.dungeon_mode() {
            Some(lb) => lb,
            None => landblock_of(eye)?.0,
        };
        let o = pick::block_origin(lb);
        Some((lb, Vec3::new(eye.x - o.x, eye.y - o.y, eye.z - o.z)))
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let place = self.status_place().map_or_else(
            || t.get("status.off_world").to_owned(),
            |(lb, p)| format!("{lb:04X} {:.1}, {:.1}, {:.1}", p.x, p.y, p.z),
        );
        let edits = unsaved_edits(
            self.session.unsaved(),
            self.session.history().0,
            self.saved.map_or(0, |(steps, _)| steps),
        );
        ui.horizontal(|ui| {
            ui.monospace(place);
            if ui
                .selectable_label(self.collide, t.get("status.collision"))
                .on_hover_text(t.get("status.collision_tip"))
                .clicked()
            {
                out.push(UiAction::Collide);
            }
            ui.menu_button(t.get("status.view"), |ui| {
                ui.checkbox(&mut self.panels.show_grid, t.get("status.grid"));
                ui.checkbox(&mut self.panels.show_bounds, t.get("status.bounds"));
            });
            if edits > 0 {
                ui.label(t.fill("status.edits", &[("n", &edits)]));
            } else if let Some((_, at)) = self.saved {
                let (key, n) = saved_ago(self.core.timer.local_time - at);
                ui.weak(t.fill(key, &[("n", &n)]));
            }
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

    /// The Tool tab: what the tool of the mode works with. In dungeon mode the dungeon's card heads
    /// it, and the select tool lays out its pieces.
    fn tools_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.brush_rects.clear();
        if let Some(lb) = self.dungeon_mode() {
            self.dungeon_card(ui, lb, out);
            if self.mode == Mode::Select {
                self.dungeon_pieces(ui, out);
                return;
            }
        }
        match self.mode {
            Mode::Select => {
                let t = self.text.table();
                filled(ui, "select", |ui| {
                    ui.strong(t.get("select.title"));
                    ui.add_space(6.0);
                    ui.label(t.get("select.help"));
                });
            }
            Mode::Place => self.browser(ui, out),
            Mode::Terrain => filled(ui, "brushes", |ui| self.brushes(ui, out)),
            Mode::Spawn => self.spawn_browser(ui, out),
        }
    }

    /// The Log tab: the checks' findings under a header that folds, then every message the status
    /// bar has shown, the newest last.
    fn log_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let faults = self.faults.iter().filter(|f| !f.warning).count();
        let warnings = self.faults.len() - faults;
        let count = |one: &str, many: &str, n: usize| {
            if n == 1 {
                t.get(one).to_owned()
            } else {
                t.fill(many, &[("n", &n)])
            }
        };
        let head = format!(
            "{}, {}",
            count("checks.error", "checks.errors", faults),
            count("checks.warning", "checks.warnings", warnings)
        );
        let open = self.panels.findings_open;
        let header = egui::CollapsingHeader::new(head)
            .id_salt("findings")
            .open(Some(open))
            .show(ui, |ui| self.findings(ui, out));
        if header.header_response.clicked() {
            self.panels.findings_open = !open;
        }
        if !open {
            self.fault_rects.clear();
        }
        ui.separator();
        for line in &self.log {
            ui.label(line);
        }
    }

    /// The checks' findings, each going to what it is about when clicked.
    fn findings(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
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
                    // A weenie's finding: the editor on it.
                    out.push(UiAction::Weenie(w.clone()));
                    out.push(UiAction::EditWeenie(w, None));
                }
            }
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
        // New..., and how many are shown, right-aligned.
        ui.horizontal(|ui| {
            let new = ui.button(t.get("spawn.new"));
            self.panels.new_button = Some(new.rect);
            if new.clicked() {
                out.push(UiAction::NewWeenie(None));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(t.fill(
                    "spawn.shown",
                    &[("n", &list.len()), ("all", &self.weenies.len())],
                ));
                if self.naming() {
                    ui.weak(t.get("spawn.reading"));
                }
            });
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
                            // Dragged onto the Characters tab, it is given to the character.
                            r.dnd_set_drag_payload(w.weenie.clone());
                            let id = match &w.weenie {
                                Weenie::Symbol(s) => s.rsplit(':').next().unwrap_or(s).to_owned(),
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
        self.panels.tag_menu.clear();
        self.panels.tag_field = None;
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
        // Its tags, on a right click: each to take away or give, and a new one. It stays open
        // while it is used; a click outside it, Escape, or a new tag added closes it.
        let menu = egui::Popup::context_menu(&r)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        menu.show(|ui| {
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
            self.panels.tag_field = Some(field.rect);
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
            ui.label(kind);
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
                let r = icon_button(ui, icon, on).on_hover_text(t.get(label));
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
                0 => t.fill(
                    "brush.radius_under",
                    &[("m", &format!("{:.0}", self.panels.radius))],
                ),
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
            let shown_tex = types
                .iter()
                .find(|(v, ..)| *v == chosen)
                .and_then(|(_, _, tex)| *tex)
                .and_then(|tex| self.texture_picture(tex));
            if let Some(id) = shown_tex {
                ui.add(egui::Image::new((id, egui::vec2(40.0, 40.0))));
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
        // What the chosen scene grows, under the dropdown.
        if let Some(models) = scenes.get(usize::from(chosen)) {
            ui.horizontal_wrapped(|ui| {
                for m in models {
                    let (rect, r) =
                        ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
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
                            ui.painter()
                                .rect_filled(rect, 2.0, egui::Color32::from_gray(20));
                        }
                    }
                    let label = self.names.label(*m).unwrap_or_default();
                    r.on_hover_text(format!("{label}\n{:#010X}", m.raw()));
                }
            });
        }
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
            ui.horizontal(|ui| {
                if ui.button(t.get("inspector.delete_all")).clicked() {
                    out.push(UiAction::Delete);
                }
                let dup = ui
                    .button(t.get("inspector.duplicate_all"))
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
                if let Some(label) = self.names.label(model) {
                    ui.weak(label);
                }
                model_link(ui, &t, model, out);
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
                if let Some(StaticOrigin::Pack { pack, name, .. }) = origin {
                    ui.weak(name.unwrap_or_else(|| {
                        t.fill(
                            "inspector.pack_unnamed",
                            &[("pack", &self.session.packs()[pack].name)],
                        )
                    }));
                }
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
                if let Some(label) = self.names.label(model) {
                    ui.weak(label);
                }
                model_link(ui, &t, model, out);
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
                model_link(ui, &t, b.id, out);
                let dropped = self.dropped_name(landblock, index);
                let sel = Selection::Building { landblock, index };
                self.place_fields(ui, &sel, landblock, &b.frame, None, out);
                if dropped.is_some() && ui.button(t.get("inspector.delete")).clicked() {
                    out.push(UiAction::Delete);
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
                let name = which
                    .as_ref()
                    .and_then(|w| self.weenie_entry(w))
                    .map(|w| w.name.clone());
                ui.strong(name.unwrap_or_else(|| t.get("inspector.spawn").into()));
                let lb = (s.cell >> 16) as u16;
                if s.server {
                    // The server's own: no file of the packs places it yet.
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
                match s.setup {
                    Some(id) => model_link(ui, &t, id, out),
                    None => {
                        ui.weak(t.get("inspector.setup_unknown"));
                    }
                }
                if let Some(to) = self.portal_destination(&s) {
                    let at = self.portal_destination_at(&s);
                    if let Some(w) = self.session.spawn_weenie(&s) {
                        self.destination_card(ui, &w, to, at, out);
                    }
                }
                self.links_card(ui, &s, out);
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
            let name = which
                .as_ref()
                .and_then(|w| self.weenie_entry(w))
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
        if room.is_some() {
            ui.weak(t.get("inspector.place_in_room"));
        }
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

    /// A portal's destination on its card: where it sends who uses it (`to`, a cell; `at`, the place
    /// in its landblock), a click on which goes there: into a dungeon at its drop point, or the
    /// camera to the place. Then the choices: where the camera is, or a dungeon.
    fn destination_card(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        to: Option<u32>,
        at: Option<Vec3>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let dungeons = self.dungeons();
        let dungeon = to.and_then(|c| {
            let lb = u16::try_from(c >> 16).unwrap_or(0);
            dungeons
                .iter()
                .find(|d| d.landblock == lb && c & 0xFFFF >= 0x0100)
        });
        let shown = match (to, dungeon) {
            (None, _) => t.get("fields.nowhere").to_owned(),
            (Some(_), Some(d)) => self.dungeon_name(d),
            (Some(c), None) => format!("0x{c:08X}"),
        };
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.strong(t.get("fields.destination"));
            match to {
                None => {
                    ui.label(shown);
                }
                Some(c) => {
                    let r = ui.add(egui::Link::new(shown));
                    self.panels.destination_link = Some(r.rect);
                    if r.clicked() {
                        let lb = u16::try_from(c >> 16).unwrap_or(0);
                        match (dungeon, at) {
                            (Some(d), _) => out.push(UiAction::EnterDungeon(d.landblock)),
                            (None, Some(at)) => out.push(UiAction::GoTo(lb, at)),
                            (None, None) => {
                                out.push(UiAction::GoTo(lb, Vec3::new(96.0, 96.0, 0.0)));
                            }
                        }
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(t.get("fields.set_destination"));
            if ui.button(t.get("fields.destination_here")).clicked() {
                out.push(UiAction::DestinationHere(w.clone()));
            }
            egui::ComboBox::from_id_salt("portal destination")
                .selected_text(t.get("fields.destination_dungeon"))
                .height(240.0)
                .show_ui(ui, |ui| {
                    for d in &dungeons {
                        if ui.selectable_label(false, self.dungeon_name(d)).clicked() {
                            out.push(UiAction::DestinationDungeon(w.clone(), d.landblock));
                        }
                    }
                });
        });
    }

    // -------------------------------------------------------------------------------------------
    // the marks on the world
    // -------------------------------------------------------------------------------------------

    /// The selection's box, the handles and the brush.
    fn world_marks(&mut self, painter: &egui::Painter) {
        self.marks_view = self.view();
        let Some(view) = self.marks_view else {
            return;
        };
        let line = |a: (f32, f32), b: (f32, f32), w: f32, c: egui::Color32| {
            painter.line_segment(
                [egui::pos2(a.0, a.1), egui::pos2(b.0, b.1)],
                egui::Stroke::new(w, c),
            );
        };
        if self.landscape_grid_shown() {
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
        self.links.lines.clear();
        if let Some(s) = self.selected_spawn() {
            if let Some(g) = self.session.generator(&s) {
                self.draw_generator(painter, &view, &s, &g);
            }
            // What it sets off, and what sets it off.
            self.draw_links(painter, &view, &s);
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

/// An icon the panels draw: the tools, the bar's buttons and the ground's brushes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Icon {
    Select,
    Place,
    Spawn,
    Terrain,
    Undo,
    Redo,
    Download,
    Play,
    Stop,
    Live,
    Settings,
    Save,
    Raise,
    Lower,
    Flatten,
    Smooth,
    /// A menu's arrow, pointing down.
    Menu,
}

/// The size of a button of the bar of tools.
const BUTTON: egui::Vec2 = egui::vec2(30.0, 28.0);

/// The width of a menu's arrow beside the button it belongs to.
const MENU_ARROW_WIDTH: f32 = 16.0;

/// `r` with `text` as its tooltip, enabled or not.
fn tip(r: egui::Response, text: &str) -> egui::Response {
    r.on_hover_text(text).on_disabled_hover_text(text)
}

/// A button of the bar of tools showing `icon`: lit when `on`, dimmed when not `enabled`.
fn tool_button(ui: &mut egui::Ui, icon: Icon, on: bool, enabled: bool) -> egui::Response {
    let r = ui.add_enabled(enabled, egui::Button::selectable(on, "").min_size(BUTTON));
    let color = if enabled {
        ui.style().interact_selectable(&r, on).fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    draw_icon(ui.painter(), r.rect, icon, color);
    r
}

/// A square button showing a brush's `icon`, lit when `on`.
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
    draw_icon(&painter, rect, icon, egui::Color32::from_gray(225));
    r
}

/// `icon` drawn in the middle of `rect` in `color`, as lines and convex shapes.
#[allow(clippy::too_many_lines)]
fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let c = rect.center();
    let p = |x: f32, y: f32| c + egui::vec2(x, y);
    let line = egui::Stroke::new(2.0, color);
    let thin = egui::Stroke::new(1.5, color);
    let ground = egui::Stroke::new(2.0, egui::Color32::from_rgb(140, 190, 110));
    let fill = |pts: Vec<egui::Pos2>| {
        painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
    };
    // An arc about `o` of radius `r` from angle `a` to `b` (radians, clockwise on screen).
    let arc = |o: egui::Pos2, r: f32, a: f32, b: f32| -> Vec<egui::Pos2> {
        (0..=16)
            .map(|k| {
                #[allow(clippy::cast_precision_loss)]
                let t = a + (b - a) * (k as f32) / 16.0;
                o + egui::vec2(
                    dereth_primitives::num::math::cosf(t) * r,
                    dereth_primitives::num::math::sinf(t) * r,
                )
            })
            .collect()
    };
    match icon {
        Icon::Select => {
            // A pointer.
            painter.add(egui::Shape::closed_line(
                vec![
                    p(-5.0, -9.0),
                    p(-5.0, 6.0),
                    p(-1.5, 2.5),
                    p(1.5, 9.0),
                    p(4.0, 8.0),
                    p(1.0, 1.5),
                    p(6.0, 1.5),
                ],
                thin,
            ));
        }
        Icon::Place => {
            // A tree.
            fill(vec![p(0.0, -10.0), p(7.0, 4.0), p(-7.0, 4.0)]);
            painter.rect_filled(
                egui::Rect::from_min_max(p(-1.5, 4.0), p(1.5, 9.0)),
                0.0,
                color,
            );
        }
        Icon::Spawn => {
            // Someone.
            painter.circle_filled(p(0.0, -5.5), 3.5, color);
            fill(vec![p(-6.0, 9.0), p(-4.0, 0.0), p(4.0, 0.0), p(6.0, 9.0)]);
        }
        Icon::Terrain => {
            // Hills.
            fill(vec![p(-11.0, 8.0), p(-3.0, -6.0), p(5.0, 8.0)]);
            fill(vec![p(0.0, 8.0), p(5.0, -1.0), p(11.0, 8.0)]);
        }
        Icon::Undo | Icon::Redo => {
            // A turning arrow, back or on.
            let s = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let pts: Vec<egui::Pos2> = arc(
                p(0.0, 2.0),
                7.0,
                std::f32::consts::PI,
                std::f32::consts::TAU,
            )
            .into_iter()
            .map(|q| egui::pos2(c.x + (q.x - c.x) * s, q.y))
            .collect();
            painter.add(egui::Shape::line(pts, line));
            let at = p(-7.0 * s, 2.0);
            fill(vec![
                at + egui::vec2(-4.0, -1.0),
                at + egui::vec2(4.0, -1.0),
                at + egui::vec2(0.0, 5.0),
            ]);
        }
        Icon::Download => {
            painter.line_segment([p(0.0, -9.0), p(0.0, 3.0)], line);
            fill(vec![p(-5.0, 1.0), p(5.0, 1.0), p(0.0, 7.0)]);
            painter.line_segment([p(-8.0, 9.0), p(8.0, 9.0)], line);
        }
        Icon::Play => fill(vec![p(-5.0, -8.0), p(8.0, 0.0), p(-5.0, 8.0)]),
        Icon::Stop => {
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(13.0, 13.0)),
                1.0,
                mark::FAULT,
            );
        }
        Icon::Live => {
            // A server sending: a dot, and its waves either side.
            painter.circle_filled(c, 2.5, color);
            let half = std::f32::consts::PI;
            for r in [5.5, 9.0] {
                painter.add(egui::Shape::line(arc(c, r, -0.9, 0.9), thin));
                painter.add(egui::Shape::line(arc(c, r, half - 0.9, half + 0.9), thin));
            }
        }
        Icon::Settings => {
            // A gear: a ring, and its teeth round it.
            painter.circle_stroke(c, 5.0, line);
            for k in 0..8 {
                #[allow(clippy::cast_precision_loss)]
                let a = std::f32::consts::TAU * (k as f32) / 8.0;
                let d = egui::vec2(
                    dereth_primitives::num::math::cosf(a),
                    dereth_primitives::num::math::sinf(a),
                );
                painter.line_segment([c + d * 6.0, c + d * 9.5], egui::Stroke::new(2.5, color));
            }
        }
        Icon::Menu => {
            // A small triangle pointing down.
            fill(vec![p(-4.5, -2.5), p(4.5, -2.5), p(0.0, 3.0)]);
        }
        Icon::Save => {
            // A disk: its outline, its shutter and its label.
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-8.0, -8.0), p(8.0, 8.0)),
                1.5,
                thin,
                egui::StrokeKind::Middle,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(p(-4.5, -8.0), p(4.5, -3.0)),
                0.0,
                color,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-5.0, 1.5), p(5.0, 8.0)),
                0.0,
                thin,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Raise | Icon::Lower => {
            // A mound or a dip over the ground, and an arrow up or down.
            let sign = if icon == Icon::Raise { -1.0 } else { 1.0 };
            painter.line_segment([p(-10.0, 8.0), p(10.0, 8.0)], ground);
            painter.line_segment([p(0.0, 5.0 * -sign), p(0.0, 9.0 * sign)], line);
            fill(vec![
                p(0.0, 11.0 * sign),
                p(-5.0, 4.0 * sign),
                p(5.0, 4.0 * sign),
            ]);
        }
        Icon::Flatten => {
            painter.line_segment([p(-11.0, 2.0), p(11.0, 2.0)], ground);
            painter.line_segment([p(-7.0, -5.0), p(7.0, -5.0)], line);
            painter.line_segment([p(-7.0, -5.0), p(-7.0, -1.0)], line);
            painter.line_segment([p(7.0, -5.0), p(7.0, -1.0)], line);
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
                    p(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(wave, ground));
        }
    }
}

/// A model's id as a link: a click chooses it in the Place tool.
fn model_link(ui: &mut egui::Ui, t: &crate::text::Table, m: DataId, out: &mut Vec<UiAction>) {
    let r = ui.add(egui::Link::new(
        t.fill("inspector.model", &[("id", &format!("{:#010X}", m.raw()))]),
    ));
    if r.clicked() {
        out.push(UiAction::OpenModel(m));
    }
}

/// A search box: a magnifier, then the field, its hint `hint` while empty.
pub(super) fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
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
    let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click_and_drag());
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's status bar)
    use super::*;

    #[test]
    fn the_status_bar_says_how_long_ago_the_world_was_saved_by_the_clock_it_is_given() {
        assert_eq!(saved_ago(0.0), ("status.saved_now", 0));
        assert_eq!(saved_ago(59.9), ("status.saved_now", 0));
        assert_eq!(saved_ago(60.0), ("status.saved_minutes", 1));
        assert_eq!(saved_ago(3599.0), ("status.saved_minutes", 59));
        assert_eq!(saved_ago(7300.0), ("status.saved_hours", 2));
        assert_eq!(
            saved_ago(-5.0),
            ("status.saved_now", 0),
            "a clock gone back"
        );
    }

    #[test]
    fn edits_not_saved_are_counted_from_where_the_history_was_saved_and_none_once_saved() {
        assert_eq!(unsaved_edits(false, 7, 3), 0);
        assert_eq!(unsaved_edits(true, 7, 3), 4);
        assert_eq!(unsaved_edits(true, 1, 3), 2, "undone past the save");
        assert_eq!(
            unsaved_edits(true, 3, 3),
            1,
            "changed, though the steps match"
        );
    }
}
