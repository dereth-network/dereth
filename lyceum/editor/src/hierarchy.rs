//! The compiled landblock's buildings, rooms and scenery as selectable rows.

use std::collections::BTreeSet;

use super::{EditorApp, Selection, UiAction};

impl EditorApp {
    pub(super) fn hierarchy_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some((lb, ..)) = self.work_block() else {
            return;
        };
        self.hierarchy_rects.clear();
        let text = self.text.table();
        let cells = self.cells_of(lb);
        let info = self.session.statics(lb);
        let mut in_buildings = BTreeSet::new();
        ui.strong(text.fill("hierarchy.landblock", &[("id", &format!("{lb:04X}"))]));
        if let Some(info) = info {
            for (index, building) in info.buildings.iter().enumerate() {
                let doors: Vec<u16> = building.portals.iter().map(|p| p.other_cell_id).collect();
                let rooms = lyceum_foundry::cells::building_cells(&doors, &cells);
                in_buildings.extend(rooms.iter().copied());
                let header =
                    egui::CollapsingHeader::new(text.fill("hierarchy.building", &[("n", &index)]))
                        .id_salt((lb, "building", index))
                        .show(ui, |ui| {
                            self.hierarchy_choice(
                                ui,
                                text.get("hierarchy.select_building"),
                                Selection::Building {
                                    landblock: lb,
                                    index,
                                },
                                out,
                            );
                            for n in &rooms {
                                if let Some(cell) = cells.get(n) {
                                    self.hierarchy_room(ui, lb, *n, cell, out);
                                }
                            }
                        });
                let selection = Selection::Building {
                    landblock: lb,
                    index,
                };
                self.hierarchy_rects
                    .push((selection.clone(), header.header_response.rect));
                if header.header_response.clicked() {
                    out.push(UiAction::Select(selection));
                }
            }
            egui::CollapsingHeader::new(text.get("hierarchy.scenery"))
                .id_salt((lb, "scenery"))
                .show(ui, |ui| {
                    for (index, object) in info.objects.iter().enumerate() {
                        let name = self
                            .names
                            .label(object.id)
                            .unwrap_or_else(|| format!("{:#010X}", object.id.raw()));
                        self.hierarchy_choice(
                            ui,
                            &name,
                            Selection::Static {
                                landblock: lb,
                                index,
                            },
                            out,
                        );
                    }
                });
        }
        egui::CollapsingHeader::new(text.get("hierarchy.cells"))
            .id_salt((lb, "cells"))
            .default_open(true)
            .show(ui, |ui| {
                // Not the cells taken away, which keep their numbers far below the world.
                let open = |c: &dereth_assets::world::EnvCell| {
                    c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0
                };
                for (n, cell) in cells
                    .iter()
                    .filter(|(n, c)| !in_buildings.contains(n) && open(c))
                {
                    self.hierarchy_room(ui, lb, *n, cell, out);
                }
            });
    }

    fn hierarchy_room(
        &mut self,
        ui: &mut egui::Ui,
        lb: u16,
        n: u16,
        cell: &dereth_assets::world::EnvCell,
        out: &mut Vec<UiAction>,
    ) {
        let text = self.text.table();
        let header = egui::CollapsingHeader::new(
            text.fill("hierarchy.room", &[("id", &format!("{n:04X}"))]),
        )
        .id_salt((lb, n))
        .show(ui, |ui| {
            for (index, object) in cell.static_objects.iter().enumerate() {
                let name = self
                    .names
                    .label(object.id)
                    .unwrap_or_else(|| format!("{:#010X}", object.id.raw()));
                self.hierarchy_choice(
                    ui,
                    &name,
                    Selection::CellStatic {
                        landblock: lb,
                        cell: n,
                        index,
                    },
                    out,
                );
            }
        });
        self.hierarchy_rects.push((
            Selection::Cell {
                landblock: lb,
                cell: n,
            },
            header.header_response.rect,
        ));
        if header.header_response.clicked() {
            out.push(UiAction::ChooseCell(lb, n));
        }
    }

    fn hierarchy_choice(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        selection: Selection,
        out: &mut Vec<UiAction>,
    ) {
        if ui
            .selectable_label(self.selection.as_ref() == Some(&selection), name)
            .clicked()
        {
            out.push(UiAction::Select(selection));
        }
    }
}
