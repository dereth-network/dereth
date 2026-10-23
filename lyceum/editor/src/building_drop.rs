//! Buildings in the place tool: the catalog's gallery of buildings, the chosen one's card (its
//! layout, its furniture, and for a house whether it is one players can buy), and its drop. The ghost of its shell follows the pointer, snapped
//! where its example stands in its land cell (Shift places it freely; R turns it a quarter), over
//! the outline of the land cell it would stand in: green where the drop is allowed, red where it
//! is refused, with the reason and what it is warned of beside it. A click drops it, with its
//! rooms, doors and outdoor statics, and selects it.

use std::collections::{BTreeMap, HashMap};

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::buildings::{Entry, Kind};
use lyceum_foundry::drops::Plan;
use lyceum_foundry::pick;
use lyceum_foundry::source::Furnished;

use super::panels::mark;
use super::{add, EditorApp, Mode, Selection, UiAction};
use crate::view::View;

/// Where a drop would stand with the pointer where it is, and what the checks say of it there,
/// as last worked out.
#[derive(Debug, Clone)]
pub struct DropGhost {
    /// What it was worked out for: the model, its layout, whether it is a house players can buy,
    /// the landblock, the origin to the centimeter and the heading to a tenth of a degree.
    key: (u32, usize, bool, u16, i32, i32, i32),
    /// Where the pointer and the camera were, Shift, the turn and the world's reloads: while they
    /// stay, it stands where it stood.
    seen: Seen,
    /// Where it stands: its landblock, its origin there and its heading.
    pub landblock: u16,
    pub at: (f32, f32),
    pub heading: f32,
    /// The plan of the drop there, or why there is none.
    pub plan: Result<Plan, String>,
    /// The outline of the land cell each building of it stands in, on the ground (landblock
    /// meters).
    outline: Vec<Vec<Vec3>>,
}

/// What a ghost was seen with: the pointer, the camera's place and look, Shift, the quarter
/// turns, the model, its layout and whether it is a house players can buy, and how many times the
/// world was drawn again.
type Seen = ([u32; 2], [u32; 5], bool, u8, u32, usize, bool, u64);

impl DropGhost {
    /// Whether the drop is allowed where it stands.
    #[must_use]
    pub fn allowed(&self) -> bool {
        self.plan.as_ref().is_ok_and(|p| p.refusal.is_none())
    }
}

/// The color of a ghost's shell.
const GHOST: egui::Color32 = egui::Color32::from_rgba_premultiplied(0xD8, 0xD8, 0xD8, 0xC0);
/// A land cell's outline where a drop is allowed, and where it is refused.
const ALLOWED: egui::Color32 = egui::Color32::from_rgb(0x50, 0xE0, 0x60);
const REFUSED: egui::Color32 = egui::Color32::from_rgb(0xF0, 0x40, 0x38);

/// What a gallery calls a building: a house by its kind, any other by its kind or its place.
#[must_use]
pub fn building_name(e: &Entry, label: Option<String>) -> String {
    match e.kind {
        Kind::Cottage | Kind::Villa | Kind::Mansion => e.kind.name().to_owned(),
        _ => label.unwrap_or_else(|| e.kind.name().to_owned()),
    }
}

/// Model `m`'s edges as a ghost draws them, in its own frame: each edge of its triangles once,
/// but those between two triangles of one flat face.
fn edges_of(triangles: &[[Vec3; 3]]) -> Vec<(Vec3, Vec3)> {
    let key = |p: Vec3| {
        #[allow(clippy::cast_possible_truncation)] // a model is far smaller than i32 centimeters
        let q = |v: f32| (v * 100.0).round() as i32;
        (q(p.x), q(p.y), q(p.z))
    };
    // Each edge by its two ends to the centimeter: its ends, and the normals of its faces.
    type Ends = ((i32, i32, i32), (i32, i32, i32));
    let mut seen: BTreeMap<Ends, (Vec3, Vec3, Vec<Vec3>)> = BTreeMap::new();
    for t in triangles {
        let n = t[1].sub(t[0]).cross(t[2].sub(t[0]));
        let len = n.magnitude();
        if len < 1e-9 {
            continue;
        }
        let n = n.mul(1.0 / len);
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            let k = if ka <= kb { (ka, kb) } else { (kb, ka) };
            seen.entry(k).or_insert((a, b, Vec::new())).2.push(n);
        }
    }
    seen.into_values()
        .filter(|(_, _, normals)| {
            normals.len() < 2 || normals.iter().any(|n| n.dot(normals[0]).abs() < 0.995)
        })
        .map(|(a, b, _)| (a, b))
        .collect()
}

impl EditorApp {
    /// The building the place tool drops: the catalog's entry of the chosen model (its set's
    /// lead for a partner), when the world places it as a building.
    #[must_use]
    pub fn chosen_building(&self) -> Option<&Entry> {
        let c = self.session.buildings()?;
        let e = c.get(self.model?)?;
        if e.is_lead() {
            Some(e)
        } else {
            c.get(e.set.iter().map(|p| p.model).min()?)
        }
    }

    /// Whether the place tool drops a building where the pointer is: one chosen, and the world
    /// outdoors, not a dungeon.
    #[must_use]
    pub fn dropping_building(&self) -> bool {
        self.mode == Mode::Place && self.dungeon.is_none() && self.chosen_building().is_some()
    }

    /// Whether the chosen building drops as a house players can buy: a house, its card's box
    /// checked.
    #[must_use]
    pub fn chosen_house(&self) -> bool {
        self.panels.building_house
            && self
                .chosen_building()
                .is_some_and(|e| matches!(e.kind, Kind::Cottage | Kind::Villa | Kind::Mansion))
    }

    /// The furniture the card has chosen: a variant's, by the copy that has it, or none.
    #[must_use]
    pub fn chosen_furnished(&self) -> Furnished {
        let Some(l) = self
            .chosen_building()
            .and_then(|e| e.layouts.get(self.panels.building_layout))
        else {
            return Furnished::Template;
        };
        match self
            .panels
            .building_furnishing
            .and_then(|k| l.furnishings.get(k))
        {
            None => Furnished::Bare,
            Some(f) if f.example == l.example => Furnished::Template,
            Some(f) => Furnished::Like(vec![f.example]),
        }
    }

    /// Where the chosen building would stand with the pointer at `p`: its landblock, its origin
    /// there and its heading. Snapped where its example stands in the land cell under the
    /// pointer, or with Shift held, at the pointer; turned by the card's quarter turns.
    pub fn drop_spot(&mut self, p: (f32, f32)) -> Option<(u16, f32, f32, f32)> {
        let model = self.chosen_building()?.model;
        let (lb, at) = self.ground_at(p)?;
        let (x, y, heading) = self.session.drop_snap(
            model,
            self.panels.building_layout,
            at.x,
            at.y,
            self.panels.building_turns,
        )?;
        Some(if self.shift {
            (lb, at.x, at.y, heading)
        } else {
            (lb, x, y, heading)
        })
    }

    /// The ghost of the chosen building with the pointer at `p`: where it would stand looked for
    /// again only when the pointer, the camera or the world has changed, and worked out again only
    /// when that place has.
    pub fn drop_ghost(&mut self, p: (f32, f32)) -> Option<DropGhost> {
        let model = self.chosen_building()?.model;
        let layout = self.panels.building_layout;
        let house = self.chosen_house();
        let c = self.camera;
        let seen: Seen = (
            [p.0.to_bits(), p.1.to_bits()],
            [
                c.position.x.to_bits(),
                c.position.y.to_bits(),
                c.position.z.to_bits(),
                c.yaw.to_bits(),
                c.pitch.to_bits(),
            ],
            self.shift,
            self.panels.building_turns,
            model.raw(),
            layout,
            house,
            self.stats.reloads,
        );
        if let Some(g) = self.ghost.as_ref().filter(|g| g.seen == seen) {
            return Some(g.clone());
        }
        let (lb, x, y, heading) = self.drop_spot(p)?;
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            model.raw(),
            layout,
            house,
            lb,
            (x * 100.0).round() as i32,
            (y * 100.0).round() as i32,
            (heading * 10.0).round() as i32,
        );
        if let Some(g) = self.ghost.as_mut().filter(|g| g.key == key) {
            g.seen = seen;
            return Some(g.clone());
        }
        let plan = self
            .session
            .drop_plan_as(model, layout, house, lb, x, y, heading)
            .map_err(|e| e.to_string());
        let origins: Vec<(f32, f32)> = match &plan {
            Ok(p) if !p.shells.is_empty() => p
                .shells
                .iter()
                .map(|(_, f)| (f.origin.x, f.origin.y))
                .collect(),
            _ => vec![(x, y)],
        };
        let outline = origins
            .into_iter()
            .map(|(ox, oy)| self.land_cell_outline(lb, ox, oy))
            .collect();
        self.ghost = Some(DropGhost {
            key,
            seen,
            landblock: lb,
            at: (x, y),
            heading,
            plan,
            outline,
        });
        self.ghost.clone()
    }

    /// The outline of the land cell `(x, y)` (landblock `lb`'s meters) stands in, on the ground a
    /// little over it: a ring of points, landblock meters.
    fn land_cell_outline(&self, lb: u16, x: f32, y: f32) -> Vec<Vec3> {
        let (cx, cy) = lyceum_foundry::buildings::land_cell(x, y);
        #[allow(clippy::cast_precision_loss)] // a land cell's index
        let (x0, y0) = (cx as f32 * 24.0, cy as f32 * 24.0);
        let corners = [
            (x0, y0),
            (x0 + 24.0, y0),
            (x0 + 24.0, y0 + 24.0),
            (x0, y0 + 24.0),
        ];
        let mut ring = Vec::new();
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            for i in 0..=12 {
                #[allow(clippy::cast_precision_loss)]
                let s = i as f32 / 12.0;
                let (px, py) = (a.0 + (b.0 - a.0) * s, a.1 + (b.1 - a.1) * s);
                let z = self
                    .session
                    .ground(lb, px.clamp(0.0, 191.99), py.clamp(0.0, 191.99))
                    .unwrap_or(0.0);
                ring.push(Vec3::new(px, py, z + 0.15));
            }
        }
        ring
    }

    /// Drop the chosen building where the pointer at `p` puts it, with its rooms, doors and
    /// outdoor statics, as one step, and select it; what the drop warns of is said.
    pub(super) fn drop_at(&mut self, p: (f32, f32)) -> Result<(), String> {
        let Some(e) = self.chosen_building().cloned() else {
            return Ok(());
        };
        let Some((lb, x, y, heading)) = self.drop_spot(p) else {
            return Ok(());
        };
        // Refused for the land cell another building stands in: said as the ghost says it.
        if self
            .drop_ghost(p)
            .is_some_and(|g| g.plan.as_ref().is_ok_and(|p| p.taken.is_some()))
        {
            return Err(self.say("place.cell_taken"));
        }
        let furnished = self.chosen_furnished();
        let (op, warnings) = self
            .session
            .drop_building_as(
                e.model,
                self.panels.building_layout,
                &furnished,
                self.chosen_house(),
                lb,
                x,
                y,
                heading,
            )
            .map_err(|e| e.to_string())?;
        let before = self.session.compiled().dropped.get(&lb).map_or(0, Vec::len);
        self.apply(&op, false)?;
        if let Some((name, index)) = self
            .session
            .compiled()
            .dropped
            .get(&lb)
            .and_then(|l| l.get(before))
            .cloned()
        {
            self.selection = Some(Selection::Building {
                landblock: lb,
                index,
            });
            self.also.clear();
            let bare = name.rsplit(':').next().unwrap_or(&name).to_owned();
            self.panels.message = if warnings.is_empty() {
                self.say_with("place.dropped", &[("name", &bare)])
            } else {
                warnings.join(" ")
            };
        }
        Ok(())
    }

    /// The selected building's drop, when it is a dropped one: its name, bare.
    #[must_use]
    pub fn dropped_name(&self, lb: u16, index: usize) -> Option<String> {
        let (name, _) = self.session.dropped_building(lb, index)?;
        Some(name.rsplit(':').next().unwrap_or(&name).to_owned())
    }

    /// The chosen building's card, over the gallery: what it is, its layout (where it has more
    /// than one), its furniture, and how it is dropped.
    pub(super) fn building_card(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some(e) = self.chosen_building().cloned() else {
            return;
        };
        let t = self.text.table();
        let label = self.names.label(e.model);
        let doors_known = self.session.buildings().is_some_and(|c| c.doors_known);
        let layout = self
            .panels
            .building_layout
            .min(e.layouts.len().saturating_sub(1));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(building_name(&e, label));
                ui.weak(t.fill("place.placed_n", &[("n", &e.count)]));
            });
            if e.layouts.len() > 1 {
                let name = |k: usize| {
                    t.fill(
                        "place.layout_n",
                        &[("n", &(k + 1)), ("copies", &e.layouts[k].count)],
                    )
                };
                ui.horizontal(|ui| {
                    ui.label(t.get("place.layout"));
                    let combo = egui::ComboBox::from_id_salt("building layout")
                        .selected_text(name(layout))
                        .height(240.0)
                        .show_ui(ui, |ui| {
                            for k in 0..e.layouts.len() {
                                if ui.selectable_label(k == layout, name(k)).clicked() {
                                    out.push(UiAction::BuildingLayout(k));
                                }
                            }
                        });
                    self.panels.layout_combo = Some(combo.response.rect);
                });
            }
            let l = &e.layouts[layout];
            let shown = |k: Option<usize>| match k.and_then(|k| l.furnishings.get(k)) {
                Some(f) => t.fill(
                    "place.furnished_like",
                    &[
                        (
                            "town",
                            &f.town
                                .clone()
                                .unwrap_or_else(|| format!("{:04X}", f.example.landblock)),
                        ),
                        ("n", &f.count),
                    ],
                ),
                None => t.get("place.bare").to_owned(),
            };
            let chosen = self
                .panels
                .building_furnishing
                .filter(|k| *k < l.furnishings.len());
            self.panels.furnishing_rows.clear();
            let combo = egui::ComboBox::from_id_salt("building furniture")
                .selected_text(shown(chosen))
                .height(240.0)
                .show_ui(ui, |ui| {
                    for k in (0..l.furnishings.len()).map(Some).chain([None]) {
                        let r = ui.selectable_label(k == chosen, shown(k));
                        self.panels.furnishing_rows.push((k, r.rect));
                        if r.clicked() {
                            out.push(UiAction::BuildingFurnishing(k));
                        }
                    }
                });
            self.panels.furnishing_combo = Some(combo.response.rect);
            self.panels.house_box = None;
            if matches!(e.kind, Kind::Cottage | Kind::Villa | Kind::Mansion) {
                let mut house = self.panels.building_house;
                let r = ui.checkbox(&mut house, t.get("place.house"));
                if r.changed() {
                    out.push(UiAction::BuildingHouse(house));
                }
                self.panels.house_box = Some(r.rect);
            }
            ui.weak(t.get("place.building_help"));
            if !doors_known {
                ui.weak(t.get("place.no_doors"));
            }
        });
        ui.separator();
    }

    /// The edges of model `m` as its ghost draws them, read once.
    fn ghost_edges(&mut self, m: DataId) -> std::rc::Rc<Vec<(Vec3, Vec3)>> {
        if let Some(e) = self.ghost_edges.get(&m.raw()) {
            return std::rc::Rc::clone(e);
        }
        let edges = std::rc::Rc::new(
            self.models
                .get(self.session.base(), m)
                .map(|model| edges_of(&model.triangles))
                .unwrap_or_default(),
        );
        self.ghost_edges.insert(m.raw(), std::rc::Rc::clone(&edges));
        edges
    }

    /// The chosen building's ghost with the pointer at `pointer`: each shell of its set drawn as
    /// its edges where it would stand, the land cell each stands in outlined on the ground (green
    /// where the drop is allowed, red where it is refused), and beside it why it is refused or
    /// what it is warned of.
    pub(super) fn draw_drop_ghost(
        &mut self,
        painter: &egui::Painter,
        view: &View,
        pointer: (f32, f32),
    ) {
        self.ghost_note = None;
        let Some(g) = self.drop_ghost(pointer) else {
            return;
        };
        let lb = g.landblock;
        let o = pick::block_origin(lb);
        let shells = g
            .plan
            .as_ref()
            .map(|p| p.shells.clone())
            .unwrap_or_default();
        let ghost = egui::Stroke::new(1.0, GHOST);
        for (model, f) in &shells {
            let edges = self.ghost_edges(*model);
            for (a, b) in edges.iter() {
                let (Some(p), Some(q)) = (
                    view.project(add(o, localtoglobal(f, *a))),
                    view.project(add(o, localtoglobal(f, *b))),
                ) else {
                    continue;
                };
                painter.line_segment([egui::pos2(p.0, p.1), egui::pos2(q.0, q.1)], ghost);
            }
        }
        // The land cell each building's origin stands in, on the ground.
        let color = if g.allowed() { ALLOWED } else { REFUSED };
        for ring in &g.outline {
            let points: Vec<egui::Pos2> = ring
                .iter()
                .filter_map(|p| view.project(add(o, *p)))
                .map(|q| egui::pos2(q.0, q.1))
                .collect();
            for w in points.windows(2) {
                painter.line_segment([w[0], w[1]], egui::Stroke::new(2.5, color));
            }
        }
        // Why it is refused, or what it is warned of, beside the pointer.
        let words: Vec<(String, egui::Color32)> = match &g.plan {
            Err(why) => vec![(why.clone(), REFUSED)],
            Ok(p) => match &p.refusal {
                Some(why) => {
                    let text = if p.taken.is_some() {
                        self.say("place.cell_taken")
                    } else {
                        why.clone()
                    };
                    vec![(text, REFUSED)]
                }
                None => p.warnings.iter().map(|w| (w.clone(), mark::WARN)).collect(),
            },
        };
        if words.is_empty() {
            return;
        }
        let scale = self.gui.scale();
        let at = egui::pos2(pointer.0 / scale + 16.0, pointer.1 / scale + 16.0);
        let shown = egui::Area::new(egui::Id::new("drop ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(at)
            .fade_in(false)
            .interactable(false)
            .show(painter.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(280.0);
                    for (w, c) in &words {
                        ui.colored_label(*c, w);
                    }
                });
            });
        self.ghost_note = Some((
            shown.response.rect,
            words.into_iter().map(|(w, _)| w).collect(),
        ));
    }
}

/// The ghosts' edges read so far, by model.
pub type GhostEdges = HashMap<u32, std::rc::Rc<Vec<(Vec3, Vec3)>>>;
