//! The character form and the normal login/logout used before applying a template.
use super::{EditorApp, UiAction};

#[derive(Debug)]
pub(super) struct Work {
    pub reset: bool,
    pub existed: Option<bool>,
    pub extra: Vec<String>,
    pub mark: Option<usize>,
}

impl EditorApp {
    pub(super) fn character_panel(&mut self, ctx: &egui::Context, out: &mut Vec<UiAction>) {
        if !self.characters.open {
            return;
        }
        let t = self.text.table();
        if self.character_work.is_some() {
            egui::Window::new(t.get("characters.title")).show(ctx, |ui| {
                ui.label(&self.characters.message);
            });
            return;
        }
        let c = &mut self.characters;
        let mut open = c.open;
        egui::Window::new(t.get("characters.title"))
            .open(&mut open)
            .default_width(480.0)
            .default_height(620.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(600.0)
                    .show(ui, |ui| {
                        let mut selected = None;
                        egui::ComboBox::from_id_salt("character-choice")
                            .selected_text(&c.name)
                            .show_ui(ui, |ui| {
                                for name in &c.names {
                                    if ui.selectable_label(c.name == *name, name).clicked() {
                                        selected = Some(name.clone());
                                    }
                                }
                            });
                        if let Some(name) = selected {
                            c.name = name;
                            if let Err(e) = c.load() {
                                c.message = e;
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.label(t.get("characters.name"));
                            ui.text_edit_singleline(&mut c.name);
                        });
                        if ui.button(t.get("characters.new")).clicked() {
                            c.template = Default::default();
                        }
                        ui.horizontal(|ui| {
                            ui.label(t.get("characters.level"));
                            ui.add(
                                egui::DragValue::new(&mut c.template.level)
                                    .range(1..=c.schema.as_ref().map_or(275, |s| s.max_level)),
                            );
                        });
                        for (key, value) in [
                            "characters.strength",
                            "characters.endurance",
                            "characters.coordination",
                            "characters.quickness",
                            "characters.focus",
                            "characters.self",
                        ]
                        .into_iter()
                        .zip(c.template.attributes.iter_mut())
                        {
                            ui.horizontal(|ui| {
                                ui.label(t.get(key));
                                ui.add(egui::DragValue::new(value).range(1..=10000));
                            });
                        }
                        ui.heading(t.get("characters.skills"));
                        let mut remove = None;
                        for (i, skill) in c.template.skills.iter_mut().enumerate() {
                            ui.push_id(i, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(&skill.name);
                                    egui::ComboBox::from_id_salt("training")
                                        .selected_text(&skill.training)
                                        .show_ui(ui, |ui| {
                                            for (value, key) in [
                                                ("Untrained", "characters.untrained"),
                                                ("Trained", "characters.trained"),
                                                ("Specialized", "characters.specialized"),
                                            ] {
                                                ui.selectable_value(
                                                    &mut skill.training,
                                                    value.to_owned(),
                                                    t.get(key),
                                                );
                                            }
                                        });
                                    ui.add(egui::DragValue::new(&mut skill.ranks))
                                        .on_hover_text(t.get("characters.ranks"));
                                    if ui.button(t.get("characters.remove")).clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                        }
                        if let Some(i) = remove {
                            c.template.skills.remove(i);
                        }
                        ui.horizontal(|ui| {
                            ui.label(t.get("characters.skill"));
                            egui::ComboBox::from_id_salt("skill-choice")
                                .selected_text(&c.skill_name)
                                .show_ui(ui, |ui| {
                                    if let Some(schema) = &c.schema {
                                        for name in &schema.skills {
                                            ui.selectable_value(
                                                &mut c.skill_name,
                                                name.clone(),
                                                name,
                                            );
                                        }
                                    }
                                });
                            if ui.button(t.get("characters.add")).clicked()
                                && !c.skill_name.is_empty()
                            {
                                c.template.skills.push(crate::characters::Skill {
                                    name: std::mem::take(&mut c.skill_name),
                                    training: "Trained".into(),
                                    ranks: 0,
                                });
                            }
                        });
                        ui.heading(t.get("characters.equipment"));
                        let mut remove = None;
                        for (i, item) in c.template.equipment.iter_mut().enumerate() {
                            ui.push_id(("gear", i), |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(&mut item.weenie))
                                        .on_hover_text(t.get("characters.weenie"));
                                    let label = c
                                        .schema
                                        .as_ref()
                                        .and_then(|s| {
                                            s.locations.iter().find(|v| {
                                                item.location != 0 && v.value == item.location
                                            })
                                        })
                                        .map_or(t.get("characters.inventory"), |v| v.name.as_str());
                                    egui::ComboBox::from_id_salt("location")
                                        .selected_text(label)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut item.location,
                                                0,
                                                t.get("characters.inventory"),
                                            );
                                            if let Some(schema) = &c.schema {
                                                for location in &schema.locations {
                                                    if location.value != 0 {
                                                        ui.selectable_value(
                                                            &mut item.location,
                                                            location.value,
                                                            &location.name,
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    if ui.button(t.get("characters.remove")).clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                        }
                        if let Some(i) = remove {
                            c.template.equipment.remove(i);
                        }
                        ui.horizontal(|ui| {
                            ui.label(t.get("characters.weenie"));
                            ui.add(egui::DragValue::new(&mut c.equipment_id));
                            if ui.button(t.get("characters.add")).clicked() && c.equipment_id != 0 {
                                c.template.equipment.push(crate::characters::Equipment {
                                    weenie: c.equipment_id,
                                    location: 0,
                                });
                            }
                        });
                        ui.label(t.get("characters.note"));
                        ui.horizontal(|ui| {
                            if ui.button(t.get("characters.save")).clicked() {
                                out.push(UiAction::SaveCharacter);
                            }
                            if ui
                                .add_enabled(
                                    self.character_work.is_none(),
                                    egui::Button::new(t.get("characters.reset")),
                                )
                                .clicked()
                            {
                                out.push(UiAction::ResetCharacter);
                            }
                        });
                        ui.label(&c.message);
                    });
            });
        c.open = open;
    }

    pub(super) fn prepare_character(&mut self, reset: bool) -> Result<(), String> {
        if self.character_work.is_some() {
            return Err("A character is already being prepared.".into());
        }
        self.characters.error = None;
        self.characters.save()?;
        self.stop_embedded();
        let live = self
            .live
            .as_mut()
            .ok_or("Start Live before preparing a character.")?;
        if !live.wait_empty() || !live.wait_account() {
            return Err("The character has not finished logging out.".into());
        }
        let mut args = live.play_args(&self.core.cfg.dat_dir, 0, [0.0; 3]);
        let at = args
            .iter()
            .position(|s| s == "-u" || s == "-r")
            .ok_or("No character argument")?;
        args[at] = "-r".into();
        args[at + 1] = self.characters.name.clone();
        let at = args.iter().position(|s| s == "--say").unwrap();
        args.truncate(at);
        args.extend(["--linger".into(), "0".into()]);
        self.start_embedded(&args, &self.live_args.playtest.join("client"))?;
        self.character_work = Some(Work {
            reset,
            existed: None,
            extra: Vec::new(),
            mark: None,
        });
        self.characters.message = self.say("characters.preparing");
        Ok(())
    }
}

impl EditorApp {
    pub(super) fn character_frame(&mut self) {
        let Some(mut work) = self.character_work.take() else {
            return;
        };
        if let Some(play) = self.playing.as_ref() {
            if work.existed.is_none() {
                if let Some(set) = &play.game.host_state.character_set {
                    work.existed = Some(set.set.iter().any(|c| {
                        c.name
                            .trim_start_matches('+')
                            .eq_ignore_ascii_case(&self.characters.name)
                    }));
                    if work.existed == Some(false) {
                        if let Err(e) = std::fs::write(
                            self.characters.applied().with_extension("pending"),
                            b"1\n",
                        ) {
                            self.characters.error = Some(e.to_string());
                            self.characters.message = e.to_string();
                            self.stop_embedded();
                            return;
                        }
                    }
                }
            }
            self.character_work = Some(work);
            return;
        }
        let result = (|| -> Result<bool, String> {
            let live = self.live.as_mut().ok_or("The playtest server stopped.")?;
            if !work.reset
                && work.existed == Some(true)
                && !self.characters.applied().with_extension("pending").exists()
            {
                return Ok(true);
            }
            if let Some(mark) = work.mark {
                for line in live.log_from(mark) {
                    if line.contains("Playtest character updated;") {
                        return Ok(true);
                    }
                    if let Some((_, why)) = line.split_once("Playtest character unchanged: ") {
                        return Err(why.to_owned());
                    }
                }
                return Ok(false);
            }
            let backups = self.live_args.playtest.join("character-backups");
            std::fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
            let mut serial = 0_u64;
            let backup = loop {
                let p = backups.join(format!("{}-{serial}.sqlite", self.characters.name));
                if !p.exists() {
                    break p;
                }
                serial += 1;
            };
            let input = self.characters.path()?;
            work.mark = Some(live.log_from(0).len());
            live.command(&format!(
                "playtest-character apply \"+{}\" {} {}",
                self.characters.name,
                crate::live::console_path(&input),
                crate::live::console_path(&backup)
            ))?;
            Ok(false)
        })();
        match result {
            Ok(false) => self.character_work = Some(work),
            Ok(true) => {
                if let Err(e) = std::fs::write(self.characters.applied(), b"1\n") {
                    self.characters.error = Some(e.to_string());
                    self.characters.message = e.to_string();
                    return;
                }
                let pending = self.characters.applied().with_extension("pending");
                if pending.exists() {
                    if let Err(e) = std::fs::remove_file(pending) {
                        self.characters.error = Some(e.to_string());
                        self.characters.message = e.to_string();
                        return;
                    }
                }
                self.characters.message = self.say(
                    if work.mark.is_none() && !work.reset && work.existed == Some(true) {
                        "characters.kept"
                    } else {
                        "characters.ready"
                    },
                );
                if !work.reset {
                    if let Err(e) = self.play(&work.extra, false) {
                        self.characters.error = Some(e.clone());
                        self.characters.message = e;
                    }
                }
            }
            Err(e) => {
                self.characters.error = Some(e.clone());
                self.characters.message = e.clone();
                self.panels.message = e;
            }
        }
    }
}

impl EditorApp {
    pub(super) fn wait_character(&mut self) -> Result<(), String> {
        let start = std::time::Instant::now();
        while self.character_work.is_some() {
            if !self.frame() {
                return Err("The editor closed while preparing a character.".into());
            }
            if start.elapsed() > std::time::Duration::from_secs(120) {
                self.character_work = None;
                self.stop_embedded();
                return Err("The server did not finish preparing the character.".into());
            }
        }
        self.characters.error.clone().map_or(Ok(()), Err)
    }
}
