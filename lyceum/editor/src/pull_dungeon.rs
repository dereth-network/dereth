//! Pulling a dungeon from another world into the world repository being edited
//! ([`lyceum_foundry::pull`]): what that world places in the dungeon as the layer's rows, and a
//! portal into it where the camera stands, one change that one undo takes back. What it can't
//! settle (the exits to re-aim, the classes the repository lacks) is listed under it.

use std::path::{Path, PathBuf};

use lyceum_foundry::edit::Op;
use lyceum_foundry::pull::{self, Entrance, Request};

use super::EditorApp;

/// The pull section's state.
#[derive(Debug, Default)]
pub struct PullView {
    /// The source world's file and the landblock, as typed.
    pub world: String,
    pub landblock: String,
    pub alongside: bool,
    /// What the last pull left to its author, a line each.
    pub report: Vec<String>,
}

impl EditorApp {
    /// Where the camera stands, as a pulled dungeon's way in: its cell, its spot on the floor or
    /// the ground under it, and its heading.
    fn entrance_here(&mut self) -> Result<Entrance, String> {
        let c = self
            .camera_spot()
            .ok_or_else(|| self.say("messages.off_world"))?;
        let (cell, z) = match c.cell {
            Some((index, floor)) => (index, floor.ok_or_else(|| self.say("areas.no_floor"))?),
            None => {
                let ground = c.ground.ok_or_else(|| self.say("areas.off_land"))?;
                let cell = u16::try_from(lyceum_foundry::edit::outdoor_cell(c.x, c.y)).unwrap_or(0);
                (cell, ground)
            }
        };
        Ok(Entrance {
            cell: (u32::from(c.lb) << 16) | u32::from(cell),
            at: [c.x, c.y, z],
            heading: c.heading,
        })
    }

    /// Dungeon `lb` of the world `world` pulled into the repository's layer being edited, with a
    /// way in where the camera stands (`way_in`), beside what the layer places there
    /// (`alongside`).
    ///
    /// # Errors
    /// No repository being edited, no server database for `world`, no place for the way in, or
    /// the pull's refusal.
    pub fn pull_dungeon(
        &mut self,
        world: &Path,
        lb: u16,
        way_in: bool,
        alongside: bool,
    ) -> Result<(), String> {
        let m = self
            .session
            .mirror()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let (schema, pack) = (m.vocabulary().clone(), m.pack_name.clone());
        let era = lyceum_foundry::world::World::read(world)
            .map_err(|e| e.to_string())?
            .era;
        let database =
            crate::catalog::server_database(era).ok_or_else(|| self.say("pull.no_database"))?;
        let entrance = if way_in {
            Some(self.entrance_here()?)
        } else {
            None
        };
        let land: Option<PathBuf> = self.session.world().imports.first().cloned();
        let dats = self.session.base().dir.clone();
        let target = self.world_path.clone();
        let r = Request {
            source: world,
            database: &database,
            landblock: lb,
            target: &target,
            schema: &schema,
            land: land.as_deref(),
            pack: Some(&pack),
            entrance,
            portal: None,
            alongside,
            dry_run: false,
            library: true,
        };
        let prepared = pull::prepare(&r, &dats, Some(&self.session)).map_err(|e| e.to_string())?;
        let session = &mut self.session;
        let texts = pull::write_rows(&schema, &prepared.pack, &prepared.rows, &mut |p| {
            session.rows_text(p)
        })?;
        let ops: Vec<Op> = texts
            .into_iter()
            .map(|(file, text)| Op::WriteRows {
                file,
                text: Some(text),
            })
            .collect();
        if !ops.is_empty() {
            self.apply(&Op::Batch(ops), false)?;
        }
        // The library list is the repository's, beside its layers: written as it is.
        if let (Some((list, _)), Some(text)) = (&prepared.pulled.library, &prepared.library_text) {
            std::fs::write(list, text).map_err(|e| format!("{}: {e}", list.display()))?;
        }
        let done = &prepared.pulled;
        let mut report = Vec::new();
        for e in &done.plan.exits {
            let key = if e.outdoors() {
                "pull.exit_land"
            } else {
                "pull.exit_dungeon"
            };
            report.push(self.say_with(
                key,
                &[
                    ("guid", &format!("{:#010X}", e.guid)),
                    ("name", &e.name.clone().unwrap_or_default()),
                    ("to", &format!("{:04X}", e.to >> 16)),
                ],
            ));
        }
        let listed: Vec<u32> = done
            .library
            .as_ref()
            .map(|(_, added)| added.clone())
            .unwrap_or_default();
        for (class, name) in done.missing.iter().filter(|(c, _)| !listed.contains(c)) {
            report.push(self.say_with(
                "pull.missing",
                &[
                    ("class", &class.to_string()),
                    ("name", &name.clone().unwrap_or_default()),
                ],
            ));
        }
        if let Some((list, added)) = &done.library {
            report.push(self.say_with(
                "pull.listed",
                &[
                    ("n", &added.len().to_string()),
                    ("list", &list.display().to_string()),
                ],
            ));
        }
        self.pull.report = report;
        self.panels.message = self.say_with(
            "pull.done",
            &[
                ("n", &done.plan.rows.len().to_string()),
                ("lb", &format!("{lb:04X}")),
            ],
        );
        Ok(())
    }

    /// The Dungeons tab's pull section, for a world repository.
    pub(super) fn pull_ui(&mut self, ui: &mut egui::Ui) {
        if !self.repo_world {
            return;
        }
        let t = self.text.table();
        egui::CollapsingHeader::new(t.get("pull.head"))
            .id_salt("pull dungeon")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t.get("pull.world"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pull.world)
                            .hint_text(t.get("pull.world_hint"))
                            .desired_width(220.0),
                    );
                    ui.label(t.get("pull.landblock"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pull.landblock)
                            .hint_text("01A9")
                            .desired_width(50.0),
                    );
                });
                ui.checkbox(&mut self.pull.alongside, t.get("pull.alongside"));
                let lb = u16::from_str_radix(self.pull.landblock.trim(), 16).ok();
                let world = PathBuf::from(self.pull.world.trim());
                let ready = lb.is_some() && world.is_file();
                let mut go = None;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(ready, egui::Button::new(t.get("pull.here")))
                        .clicked()
                    {
                        go = Some(true);
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new(t.get("pull.go")))
                        .clicked()
                    {
                        go = Some(false);
                    }
                });
                if let (Some(way_in), Some(lb)) = (go, lb) {
                    let alongside = self.pull.alongside;
                    if let Err(why) = self.pull_dungeon(&world, lb, way_in, alongside) {
                        self.panels.message = why;
                    }
                }
                for line in &self.pull.report {
                    ui.weak(line);
                }
            });
    }
}
