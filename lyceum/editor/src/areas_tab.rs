//! The Starting Areas tab: the character-creation table's starting areas as the world has them,
//! each with its places and the heritages that offer it, changed as the pack being edited's
//! `[[starter-area]]` rows ([`lyceum_foundry::starter_areas`]), in its `records/chargen.toml`.
//!
//! A place is added where the camera is, standing on the floor of the cell it is in or on the
//! ground; a spot with no floor under it, off the land or under the ground is refused. A new area
//! is offered to every heritage until it names its own.

use dereth_assets::tables::CharGen;
use dereth_assets::Decode;
use dereth_dat::ModernDat;
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::edit::Op;
use lyceum_foundry::starter_areas::{self, Camera, Refused, ShownArea, FILE, KIND};
use lyceum_foundry::table_edit;
use lyceum_foundry::tables::CHARGEN;

use super::{EditorApp, Tab};

/// The Starting Areas tab's state.
#[derive(Debug, Default)]
pub struct AreasView {
    /// A new area's name, being typed.
    pub new_name: String,
}

/// A change of one area.
#[derive(Clone, Debug, PartialEq)]
pub enum AreaChange {
    /// A place added where the camera is.
    AddHere,
    /// Its place `n` taken away.
    RemovePlace(usize),
    /// Offered to these heritages (every one: `"all"`).
    Heritages(Vec<String>),
    /// Taken out of the world (`true`), or no longer.
    Remove(bool),
    /// The pack's row of it taken away: the area as the world below has it.
    Below,
}

impl EditorApp {
    /// Show the Starting Areas tab: where it is, else floating over the world view.
    pub fn open_areas(&mut self) {
        self.bring_back(Tab::Areas);
        let world = self.world_rect.unwrap_or(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1400.0, 900.0),
        ));
        let size = egui::vec2(
            560.0_f32.min(world.width() - 32.0).max(320.0),
            600.0_f32.min(world.height() - 32.0).max(240.0),
        );
        let at =
            egui::Rect::from_min_size(egui::pos2(world.min.x + 64.0, world.min.y + 64.0), size);
        let dock = if self.dungeon.is_some() {
            self.dungeon_dock
                .get_or_insert_with(crate::docks::in_dungeon)
        } else {
            self.dock.get_or_insert_with(crate::docks::outdoors)
        };
        crate::docks::open_floating(dock, Tab::Areas, at);
        self.show_tab(Tab::Areas);
    }

    /// The character-creation table as the world has it, its unsaved changes in.
    fn chargen_now(&self) -> Option<CharGen> {
        let bytes = self
            .session
            .compiled()
            .records
            .get(&ModernDat::Portal)
            .and_then(|r| r.get(&CHARGEN))
            .map(|s| s.bytes.clone())
            .or_else(|| self.session.base().read_portal(DataId(CHARGEN)))?;
        CharGen::decode_payload(DataId(CHARGEN), &bytes).ok()
    }

    /// The pack being edited's file of starter-area rows.
    fn areas_file(&self) -> std::path::PathBuf {
        self.session.packs()[self.session.target()]
            .dir
            .join("records")
            .join(FILE)
    }

    /// Where the camera stands, for a place.
    fn camera_spot(&mut self) -> Option<Camera> {
        let (lb, x, y) = self.work_block()?;
        let o = lyceum_foundry::pick::block_origin(lb);
        let z = self.camera.position.z - o.z;
        let cell = self.camera_cell().map(|(c, _)| {
            let index = u16::try_from(c.0 & 0xFFFF).unwrap_or(0);
            let floor = self
                .session
                .cell_at(lb, Vec3::new(x, y, z))
                .filter(|(i, _)| *i == index)
                .map(|(_, f)| f);
            (index, floor)
        });
        Some(Camera {
            lb,
            x,
            y,
            z,
            heading: (-self.camera.yaw.to_degrees()).rem_euclid(360.0),
            cell,
            ground: self.session.ground(lb, x, y),
        })
    }

    /// A place where the camera is, or why there is none.
    fn place_here(&mut self) -> Result<toml::Value, String> {
        let spot = self
            .camera_spot()
            .ok_or_else(|| self.say("messages.off_world"))?;
        starter_areas::place_here(&spot).map_err(|r| {
            self.say(match r {
                Refused::NoFloor => "areas.no_floor",
                Refused::OffLand => "areas.off_land",
                Refused::UnderGround => "areas.under_ground",
            })
        })
    }

    /// Area `area` (as the world has it; `None`, a new one named `name`) changed as `change`
    /// says, in the pack being edited.
    ///
    /// # Errors
    /// A place refused, or a file that doesn't write.
    pub(super) fn area_change(
        &mut self,
        name: &str,
        area: Option<&ShownArea>,
        change: &AreaChange,
    ) -> Result<(), String> {
        let file = self.areas_file();
        let text = self.session.rows_text(&file);
        let key = [("name", toml::Value::String(name.to_owned()))];
        let places = |extra: Option<toml::Value>, skip: Option<usize>| {
            let mut v: Vec<toml::Value> = area
                .map(|a| {
                    a.places
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| Some(*i) != skip)
                        .map(|(_, p)| starter_areas::row_of(p))
                        .collect()
                })
                .unwrap_or_default();
            v.extend(extra);
            toml::Value::Array(v)
        };
        let fields: Vec<(&str, Option<toml::Value>)> = match change {
            AreaChange::AddHere => vec![("at", Some(places(Some(self.place_here()?), None)))],
            AreaChange::RemovePlace(n) => {
                let left = places(None, Some(*n));
                if left.as_array().is_some_and(Vec::is_empty) {
                    return Err(self.say("areas.last_place"));
                }
                vec![("at", Some(left))]
            }
            AreaChange::Heritages(list) => {
                let all = self
                    .chargen_now()
                    .map(|cg| starter_areas::heritages(&cg))
                    .unwrap_or_default();
                let v = if list.len() == all.len() {
                    toml::Value::String("all".into())
                } else {
                    toml::Value::Array(list.iter().cloned().map(toml::Value::String).collect())
                };
                vec![("heritages", Some(v))]
            }
            AreaChange::Remove(gone) => {
                vec![("remove", gone.then_some(toml::Value::Boolean(true)))]
            }
            AreaChange::Below => vec![("at", None), ("heritages", None), ("remove", None)],
        };
        let new = table_edit::set_fields(text.as_deref(), KIND, &key, &fields)?;
        if new != text {
            self.apply(&Op::WriteTables { file, text: new }, false)?;
        }
        Ok(())
    }

    /// A place where the camera is for the area named `name`: a place more for one the world
    /// has, else a new area.
    ///
    /// # Errors
    /// As [`Self::area_change`].
    pub(super) fn add_area_here(&mut self, name: &str) -> Result<(), String> {
        let area = self.chargen_now().and_then(|cg| {
            starter_areas::areas(&cg)
                .into_iter()
                .find(|a| table_edit::key_of(&a.name) == table_edit::key_of(name))
        });
        self.area_change(name, area.as_ref(), &AreaChange::AddHere)
    }

    /// The Starting Areas tab's body.
    pub(super) fn areas_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let Some(cg) = self.chargen_now() else {
            ui.label(t.get("areas.no_table"));
            return;
        };
        let areas = starter_areas::areas(&cg);
        let every = starter_areas::heritages(&cg);
        let file = self.areas_file();
        let text = self.session.rows_text(&file);
        let pack = self.session.packs()[self.session.target()].name.clone();
        ui.label(t.fill("areas.head", &[("pack", &pack)]));
        let mut change: Option<(String, Option<ShownArea>, AreaChange)> = None;
        ui.horizontal(|ui| {
            ui.label(t.get("areas.new"));
            ui.add(egui::TextEdit::singleline(&mut self.areas.new_name).desired_width(160.0));
            let name = self.areas.new_name.trim().to_owned();
            let taken = areas
                .iter()
                .any(|a| table_edit::key_of(&a.name) == table_edit::key_of(&name));
            if ui
                .add_enabled(
                    !name.is_empty() && !taken,
                    egui::Button::new(t.get("areas.add_here")),
                )
                .clicked()
            {
                change = Some((name, None, AreaChange::AddHere));
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("areas-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for a in &areas {
                    let key = [("name", toml::Value::String(a.name.clone()))];
                    let row = table_edit::row(text.as_deref(), KIND, &key).ok().flatten();
                    ui.horizontal(|ui| {
                        ui.strong(&a.name);
                        if row.is_some() {
                            ui.weak(t.fill("areas.changed_in", &[("pack", &pack)]));
                            if ui
                                .small_button(t.get("areas.below"))
                                .on_hover_text(t.get("areas.below_tip"))
                                .clicked()
                            {
                                change = Some((a.name.clone(), Some(a.clone()), AreaChange::Below));
                            }
                        }
                        if ui.small_button(t.get("areas.take_out")).clicked() {
                            change =
                                Some((a.name.clone(), Some(a.clone()), AreaChange::Remove(true)));
                        }
                    });
                    for (i, p) in a.places.iter().enumerate() {
                        let o = p.frame.origin;
                        ui.horizontal(|ui| {
                            ui.monospace(format!(
                                "0x{:08X}  {:.1}, {:.1}, {:.1}  {:.0}°",
                                p.cell.0,
                                // Adding zero shows no height as 0.0, not -0.0.
                                o.x + 0.0,
                                o.y + 0.0,
                                o.z + 0.0,
                                starter_areas::heading(&p.frame.rotation)
                            ));
                            if ui.small_button(t.get("areas.go")).clicked() {
                                let lb = u16::try_from(p.cell.0 >> 16).unwrap_or(0);
                                self.look_at(
                                    lb,
                                    o,
                                    12.0,
                                    -starter_areas::heading(&p.frame.rotation),
                                    -20.0,
                                );
                            }
                            if a.places.len() > 1
                                && ui.small_button(t.get("areas.remove_place")).clicked()
                            {
                                change = Some((
                                    a.name.clone(),
                                    Some(a.clone()),
                                    AreaChange::RemovePlace(i),
                                ));
                            }
                        });
                    }
                    if ui.button(t.get("areas.add_place")).clicked() {
                        change = Some((a.name.clone(), Some(a.clone()), AreaChange::AddHere));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(t.get("areas.offered_to"));
                        for h in &every {
                            let mut on = a.heritages.contains(h);
                            if ui.checkbox(&mut on, h).changed() {
                                let list: Vec<String> = every
                                    .iter()
                                    .filter(|x| if *x == h { on } else { a.heritages.contains(x) })
                                    .cloned()
                                    .collect();
                                change = Some((
                                    a.name.clone(),
                                    Some(a.clone()),
                                    AreaChange::Heritages(list),
                                ));
                            }
                        }
                    });
                    ui.separator();
                }
                // Areas the pack takes out: no longer in the table, but put back from here.
                for r in table_edit_rows(text.as_deref()) {
                    let name = r
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    if r.get("remove").and_then(toml::Value::as_bool) == Some(true) {
                        ui.horizontal(|ui| {
                            ui.weak(t.fill("areas.taken_out", &[("name", &name)]));
                            if ui.small_button(t.get("areas.put_back")).clicked() {
                                change = Some((name.clone(), None, AreaChange::Remove(false)));
                            }
                        });
                    }
                }
            });
        if let Some((name, area, c)) = change {
            match self.area_change(&name, area.as_ref(), &c) {
                Ok(()) => {
                    if matches!(c, AreaChange::AddHere) && area.is_none() {
                        self.areas.new_name.clear();
                    }
                }
                Err(why) => self.panels.message = why,
            }
        }
    }
}

/// Every `[[starter-area]]` row of records file `text`.
fn table_edit_rows(text: Option<&str>) -> Vec<toml::Table> {
    text.and_then(|t| toml::from_str::<toml::Table>(t).ok())
        .and_then(|mut t| t.remove(KIND))
        .and_then(|v| match v {
            toml::Value::Array(a) => Some(a),
            _ => None,
        })
        .into_iter()
        .flatten()
        .filter_map(|v| match v {
            toml::Value::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}
