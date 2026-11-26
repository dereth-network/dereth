//! Pulling one of AC1's dungeons into the world repository being edited, over a land
//! ([`lyceum_foundry::pull`]): its rooms, copied from the player's own AC1 files, and what AC1's
//! world places in it, as the layer's rows; one change that one undo takes back. Its way in is
//! placed later, as any portal is. A copy moved to another landblock takes the repository's ways
//! in with it, re-aimed. What it can't settle (the exits and ways in to re-aim, the classes the
//! repository lacks) is listed under it.

use std::collections::BTreeMap;

use lyceum_foundry::edit::Op;
use lyceum_foundry::pull::{self, Request};

use super::EditorApp;

/// The pull section's state.
#[derive(Debug, Default)]
pub struct PullView {
    /// The dungeons of the player's own files, with their rooms, and what is typed to find one.
    pub dungeons: BTreeMap<u16, u32>,
    pub find: String,
    /// The dungeon chosen.
    pub chosen: Option<u16>,
    /// What the last pull left to its author, a line each.
    pub report: Vec<String>,
}

impl EditorApp {
    /// AC1's dungeon `lb` pulled into the repository's layer being edited: its rooms from the
    /// player's own files, and what AC1's world places in it, beside what the layer places there.
    ///
    /// # Errors
    /// No repository being edited, no server database for AC1's world, or the pull's refusal.
    pub fn pull_dungeon(&mut self, lb: u16) -> Result<(), String> {
        let m = self
            .session
            .mirror()
            .ok_or_else(|| self.say("messages.not_in_repository"))?;
        let (schema, pack) = (m.vocabulary().clone(), m.pack_name.clone());
        let database = crate::catalog::server_database(dereth_primitives::EraId::Eor)
            .ok_or_else(|| self.say("pull.no_database"))?;
        let land = self.session.world().imports.first().cloned();
        let dats = self.session.base().dir.clone();
        let target = self.world_path.clone();
        let r = Request {
            source: None,
            database: &database,
            landblock: lb,
            target: &target,
            schema: &schema,
            land: land.as_deref(),
            pack: Some(&pack),
            entrance: None,
            portal: None,
            alongside: true,
            dry_run: false,
            library: true,
            cells_only: false,
        };
        let prepared = pull::prepare(&r, &dats, Some(&self.session)).map_err(|e| e.to_string())?;
        let session = &mut self.session;
        let mut texts = pull::write_rows(&schema, &prepared.pack, &prepared.rows, &mut |p| {
            session.rows_text(p)
        })?;
        texts.extend(pull::write_weenies(&schema, &prepared.weenies, &mut |p| {
            session.rows_text(p)
        })?);
        // The dungeon's cells first, copied in where the repository's world doesn't hold them, so
        // the rows after name cells the world has.
        let ops: Vec<Op> = prepared
            .pulled
            .copied
            .iter()
            .map(|c| Op::AddDungeon {
                landblock: c.landblock,
                dungeon: c.dungeon.clone(),
            })
            .chain(texts.into_iter().map(|(file, text)| Op::WriteRows {
                file,
                text: Some(text),
            }))
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
        if let Some(c) = &done.copied {
            for (key, list) in [("pull.reaimed", &c.reaimed), ("pull.reaim", &c.reaim)] {
                for (class, from, to) in list {
                    report.push(self.say_with(
                        key,
                        &[
                            ("class", &class.to_string()),
                            ("from", &format!("{from:#010X}")),
                            ("to", &format!("{to:#010X}")),
                        ],
                    ));
                }
            }
        }
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

    /// The Dungeons tab's pull section, for a world repository over a land: AC1's dungeons, by
    /// name, to find one in and pull.
    pub(super) fn pull_ui(&mut self, ui: &mut egui::Ui) {
        if !self.repo_world || self.session.world().imports.is_empty() {
            return;
        }
        let t = self.text.table();
        let names = self.session.portal_names();
        egui::CollapsingHeader::new(t.get("pull.head"))
            .id_salt("pull dungeon")
            .default_open(true)
            .show(ui, |ui| {
                super::panels::search_box(ui, &mut self.pull.find, t.get("pull.find"));
                let find = self.pull.find.trim().to_lowercase();
                let named = |lb: u16| {
                    names.get(&lb).map_or_else(
                        || t.fill("dungeon.unnamed", &[("landblock", &format!("{lb:04X}"))]),
                        |n| format!("{n} ({lb:04X})"),
                    )
                };
                // The dungeons AC1's portals name first, by name; then the others, by landblock.
                let mut found: Vec<(u16, String)> = self
                    .pull
                    .dungeons
                    .keys()
                    .map(|&lb| (lb, named(lb)))
                    .filter(|(_, n)| find.is_empty() || n.to_lowercase().contains(&find))
                    .collect();
                found.sort_by_key(|(lb, n)| (!names.contains_key(lb), n.clone(), *lb));
                egui::ScrollArea::vertical()
                    .id_salt("pull list")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (lb, name) in &found {
                            if ui
                                .selectable_label(self.pull.chosen == Some(*lb), name)
                                .clicked()
                            {
                                self.pull.chosen = Some(*lb);
                            }
                        }
                    });
                let chosen = self.pull.chosen;
                if ui
                    .add_enabled(chosen.is_some(), egui::Button::new(t.get("pull.go")))
                    .on_hover_text(t.get("pull.go_tip"))
                    .clicked()
                {
                    if let Some(lb) = chosen {
                        if let Err(why) = self.pull_dungeon(lb) {
                            self.panels.message = why;
                        }
                    }
                }
                for line in &self.pull.report {
                    ui.weak(line);
                }
            });
    }
}
