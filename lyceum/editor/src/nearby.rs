//! The Nearby list: what stands round the camera, the world's and the packs' — statics,
//! buildings, spawns, generators and the packs' own indoor cells — each with its name, its kind and
//! how far it is, sorted and filtered; a click selects it and flies there.

use std::collections::BTreeSet;

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::Vec3;
use lyceum_foundry::compile::StaticOrigin;
use lyceum_foundry::pick;

use super::{add, EditorApp, Selection, UiAction};

/// How far round the camera the list looks, in meters.
pub const REACH: f32 = 120.0;

/// How far the camera moves before the list is gathered again, in meters.
const MOVED: f32 = 1.0;

/// How many frames the list stands at most before it is gathered again, so names learned since
/// and spawns read since show.
const FRAMES: u32 = 30;

/// What a thing in the list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NearbyKind {
    Static,
    Building,
    Spawn,
    Generator,
    Cell,
}

impl NearbyKind {
    pub const ALL: [Self; 5] = [
        Self::Static,
        Self::Building,
        Self::Spawn,
        Self::Generator,
        Self::Cell,
    ];

    /// Its name's key in the editor's text, and the key of its kind's button.
    #[must_use]
    pub const fn key(self) -> (&'static str, &'static str) {
        match self {
            Self::Static => ("nearby.static", "nearby.statics"),
            Self::Building => ("nearby.building", "nearby.buildings"),
            Self::Spawn => ("nearby.spawn", "nearby.spawns"),
            Self::Generator => ("nearby.generator", "nearby.generators"),
            Self::Cell => ("nearby.cell", "nearby.cells"),
        }
    }
}

/// How the list is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NearbySort {
    #[default]
    Distance,
    Name,
    Kind,
}

/// One thing near the camera.
#[derive(Debug, Clone, PartialEq)]
pub struct NearbyEntry {
    pub name: String,
    pub kind: NearbyKind,
    /// From the camera, in meters.
    pub distance: f32,
    pub landblock: u16,
    /// Where it stands, landblock meters.
    pub at: Vec3,
    /// What a click on it selects; with none, it is only flown to.
    pub select: Option<Selection>,
    /// The indoor cell a click goes into, for a cell.
    pub cell: Option<u16>,
}

/// The list as it stands, and how it is shown.
#[derive(Debug, Default)]
pub struct Nearby {
    pub entries: Vec<NearbyEntry>,
    pub sort: NearbySort,
    pub filter: String,
    /// The kinds shown; all when empty.
    pub kinds: BTreeSet<NearbyKind>,
    /// Where the camera was when the list was gathered; `None` when it is to be gathered again.
    from: Option<Vec3>,
    /// Frames since it was gathered.
    age: u32,
    /// The editor's frame the tab was last drawn in: the list is gathered only while it is shown.
    drawn: Option<u64>,
    /// Each indoor cell's middle in its own frame, by its environment and cell structure.
    middles: std::collections::HashMap<(u32, u16), Vec3>,
    /// Where each shown entry was drawn, as last drawn: its place in [`Self::shown`] and its row.
    pub rects: Vec<(usize, egui::Rect)>,
}

impl Nearby {
    /// Gather the list again at the next frame: after an edit.
    pub fn stale(&mut self) {
        self.from = None;
    }

    /// The entries shown, filtered and sorted.
    #[must_use]
    pub fn shown(&self) -> Vec<&NearbyEntry> {
        let f = self.filter.trim().to_lowercase();
        let mut v: Vec<&NearbyEntry> = self
            .entries
            .iter()
            .filter(|e| self.kinds.is_empty() || self.kinds.contains(&e.kind))
            .filter(|e| f.is_empty() || e.name.to_lowercase().contains(&f))
            .collect();
        let by_distance = |a: &&NearbyEntry, b: &&NearbyEntry| a.distance.total_cmp(&b.distance);
        match self.sort {
            NearbySort::Distance => v.sort_by(by_distance),
            NearbySort::Name => v.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then(by_distance(a, b))
            }),
            NearbySort::Kind => v.sort_by(|a, b| a.kind.cmp(&b.kind).then(by_distance(a, b))),
        }
        v
    }
}

impl EditorApp {
    /// Everything within [`REACH`] of the camera, in no order.
    #[must_use]
    pub fn nearby_entries(&mut self) -> Vec<NearbyEntry> {
        let eye = self.eye();
        let t = self.text.table();
        let mut out = Vec::new();
        let mut push = |name: String, kind, lb: u16, at: Vec3, select, cell| {
            let w = add(pick::block_origin(lb), at);
            let d = w.sub(eye);
            let distance = d.dot(d).sqrt();
            if distance <= REACH {
                out.push(NearbyEntry {
                    name,
                    kind,
                    distance,
                    landblock: lb,
                    at,
                    select,
                    cell,
                });
            }
        };
        for lb in self.near_blocks() {
            if let Some(info) = self.session.statics(lb) {
                let origins = self.session.origins(lb);
                for (index, o) in info.objects.iter().enumerate() {
                    let name = match origins.get(index) {
                        // A pack's own by its name in the pack.
                        Some(StaticOrigin::Pack {
                            name: Some(name), ..
                        }) => name.rsplit(':').next().unwrap_or(name).to_owned(),
                        _ => self
                            .names
                            .label(o.id)
                            .unwrap_or_else(|| t.fill("nearby.static_n", &[("n", &index)])),
                    };
                    let select = Selection::Static {
                        landblock: lb,
                        index,
                    };
                    push(
                        name,
                        NearbyKind::Static,
                        lb,
                        o.frame.origin,
                        Some(select),
                        None,
                    );
                }
                for (index, b) in info.buildings.iter().enumerate() {
                    let name = self
                        .names
                        .label(b.id)
                        .unwrap_or_else(|| t.fill("nearby.building_n", &[("n", &index)]));
                    let select = Selection::Building {
                        landblock: lb,
                        index,
                    };
                    push(
                        name,
                        NearbyKind::Building,
                        lb,
                        b.frame.origin,
                        Some(select),
                        None,
                    );
                }
            }
            // Every indoor cell, the packs' by name, and what stands in each.
            let names = self.session.cell_names(lb);
            for (index, cell) in self.cells_of(lb).iter() {
                let index = *index;
                let name = names.get(&index).map_or_else(
                    || t.fill("nearby.cell_n", &[("n", &format!("{index:04X}"))]),
                    |n| n.rsplit(':').next().unwrap_or(n).to_owned(),
                );
                for (k, o) in cell.static_objects.iter().enumerate() {
                    let what = self.names.label(o.id).unwrap_or_else(|| {
                        t.fill("nearby.static_in", &[("cell", &format!("{index:04X}"))])
                    });
                    let select = Selection::CellStatic {
                        landblock: lb,
                        cell: index,
                        index: k,
                    };
                    push(
                        what,
                        NearbyKind::Static,
                        lb,
                        o.frame.origin,
                        Some(select),
                        None,
                    );
                }
                let middle = self
                    .cell_middle(cell.environment, cell.cell_struct)
                    .map_or(cell.frame.origin, |m| localtoglobal(&cell.frame, m));
                let select = Selection::Cell {
                    landblock: lb,
                    cell: index,
                };
                push(
                    name,
                    NearbyKind::Cell,
                    lb,
                    middle,
                    Some(select),
                    Some(index),
                );
            }
        }
        let near: BTreeSet<u16> = self.near_blocks().into_iter().collect();
        let mut push_spawn = Vec::new();
        for s in self.session.spawns() {
            let lb = s.landblock();
            if !near.contains(&lb) {
                continue;
            }
            let weenie = self.session.spawn_weenie(&s);
            let name = weenie
                .as_ref()
                .and_then(|w| self.weenie_entry(w))
                .map_or_else(|| s.weenie.clone(), |w| w.name.clone());
            push_spawn.push((name, s));
        }
        for (name, s) in push_spawn {
            let kind = if self.session.generator(&s).is_some() {
                NearbyKind::Generator
            } else {
                NearbyKind::Spawn
            };
            let select = Selection::Spawn {
                file: s.file.clone(),
                guid: s.guid.clone(),
            };
            push(
                name,
                kind,
                s.landblock(),
                s.frame.origin,
                Some(select),
                None,
            );
        }
        out
    }

    /// The middle of the vertices of cell structure `cs` of environment `env`, in the cell's own
    /// frame; read once.
    fn cell_middle(&mut self, env: dereth_primitives::DataId, cs: u16) -> Option<Vec3> {
        if let Some(m) = self.nearby.middles.get(&(env.raw(), cs)) {
            return Some(*m);
        }
        let vs = &self
            .environment(env)?
            .cells
            .get(usize::from(cs))?
            .vertex_array
            .vertices;
        #[allow(clippy::cast_precision_loss)]
        let n = vs.len().max(1) as f32;
        let m = vs
            .iter()
            .fold(Vec3::ZERO, |m, v| m.add(v.position.mul(1.0 / n)));
        self.nearby.middles.insert((env.raw(), cs), m);
        Some(m)
    }

    /// Gather the list again when the camera has moved, an edit has made it stale, or the tab
    /// was not shown the frame before. Called as the tab is drawn, so a hidden list costs nothing.
    pub(super) fn nearby_frame(&mut self) {
        let eye = self.eye();
        let moved = self.nearby.from.is_none_or(|f| {
            let d = f.sub(eye);
            d.dot(d) > MOVED * MOVED
        });
        let frame = self.stats.frames;
        let hidden = self.nearby.drawn.is_none_or(|d| d + 1 != frame);
        self.nearby.drawn = Some(frame);
        self.nearby.age += 1;
        if moved || hidden || self.nearby.age >= FRAMES {
            self.stats.nearby_gathers += 1;
            self.nearby.entries = self.nearby_entries();
            self.nearby.from = Some(eye);
            self.nearby.age = 0;
        }
    }

    /// Entry `i` of the list as shown: selected, and the camera flown to it (into it, for a cell).
    ///
    /// # Errors
    /// A cell the camera cannot be put in.
    pub fn nearby_go(&mut self, i: usize) -> Result<(), String> {
        let Some(e) = self.nearby.shown().get(i).map(|e| (*e).clone()) else {
            return Ok(());
        };
        if let Some(index) = e.cell {
            if let Some(s) = e.select {
                self.selection = Some(s);
                self.also.clear();
            }
            return self.fly_into_cell(e.landblock, index);
        }
        // What stands in a room is gone to in its room, so the room is drawn round it.
        let room = match &e.select {
            Some(Selection::CellStatic { cell, .. }) => Some(*cell),
            Some(Selection::Spawn { file, guid }) => self
                .session
                .spawns()
                .into_iter()
                .find(|s| s.file == *file && s.guid == *guid)
                .and_then(|s| u16::try_from(s.cell & 0xFFFF).ok()),
            _ => None,
        };
        self.go_to(e.landblock, e.at, room, 14.0);
        if let Some(s) = e.select {
            self.selection = Some(s);
            self.also.clear();
        }
        Ok(())
    }

    /// The Nearby tab.
    pub(super) fn nearby_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.nearby_frame();
        let t = self.text.table();
        ui.label(t.get("nearby.help"));
        super::panels::search_box(ui, &mut self.nearby.filter, t.get("nearby.filter_hint"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.nearby.kinds.is_empty(), t.get("nearby.all"))
                .clicked()
            {
                self.nearby.kinds.clear();
            }
            for k in NearbyKind::ALL {
                let on = self.nearby.kinds.contains(&k);
                if ui.selectable_label(on, t.get(k.key().1)).clicked() {
                    if on {
                        self.nearby.kinds.remove(&k);
                    } else {
                        self.nearby.kinds.insert(k);
                    }
                }
            }
        });
        let shown: Vec<NearbyEntry> = self.nearby.shown().into_iter().cloned().collect();
        ui.horizontal(|ui| {
            ui.label(t.get("nearby.sort"));
            for (s, key) in [
                (NearbySort::Distance, "nearby.by_distance"),
                (NearbySort::Name, "nearby.by_name"),
                (NearbySort::Kind, "nearby.by_kind"),
            ] {
                if ui
                    .selectable_label(self.nearby.sort == s, t.get(key))
                    .clicked()
                {
                    self.nearby.sort = s;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(t.fill(
                    "nearby.shown",
                    &[("n", &shown.len()), ("all", &self.nearby.entries.len())],
                ));
            });
        });
        ui.separator();
        self.nearby.rects.clear();
        let row = 18.0;
        egui::ScrollArea::vertical()
            .id_salt("nearby")
            .auto_shrink(false)
            .show_rows(ui, row, shown.len(), |ui, rows| {
                for i in rows {
                    let e = &shown[i];
                    let chosen = e.select.is_some() && e.select == self.selection;
                    let r = ui
                        .horizontal(|ui| {
                            let r = ui.selectable_label(chosen, &e.name);
                            ui.weak(t.get(e.kind.key().0));
                            ui.monospace(
                                t.fill("nearby.meters", &[("n", &format!("{:.0}", e.distance))]),
                            );
                            r
                        })
                        .inner;
                    self.nearby.rects.push((i, r.rect));
                    if r.clicked() {
                        out.push(UiAction::Nearby(i));
                    }
                }
            });
    }
}
