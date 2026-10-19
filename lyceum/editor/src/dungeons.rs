//! The world's dungeons from the editor's side: the list (the world's own and the packs', named
//! by the portals that lead in), going into one to look round or to edit it, starting a new one in
//! an empty landblock, and a portal in to one from outdoors.

use std::collections::BTreeMap;

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::Vec3;
use lyceum_foundry::content::{self, SqlDoc};
use lyceum_foundry::dungeons::Dungeon;
use lyceum_foundry::edit::{Op, Weenie};
use lyceum_foundry::pick;

use crate::camera::landblock_of;

use super::{EditorApp, Mode, UiAction};

/// The class a portal is made a blank of.
const PORTAL_BLANK: u32 = 371;

/// How far ahead of the camera a portal in is put on the ground, in meters.
const PORTAL_AHEAD: f32 = 8.0;

/// How far under a new dungeon's landblock's ground its first piece goes, in meters.
const UNDERGROUND: f32 = 60.0;

/// The dungeon list's state: the world's own dungeons once read, the typed filter, whether the
/// steps are shown, and where each row's buttons were drawn.
#[derive(Debug)]
pub struct DungeonList {
    retail: Option<BTreeMap<u16, u32>>,
    pub filter: String,
    /// Whether the dungeon tool shows its steps.
    pub steps: bool,
    /// Each row as drawn: its landblock, its Enter button and its Edit button.
    pub rects: Vec<(u16, egui::Rect, egui::Rect)>,
}

impl Default for DungeonList {
    fn default() -> Self {
        Self {
            retail: None,
            filter: String::new(),
            steps: true,
            rects: Vec::new(),
        }
    }
}

impl EditorApp {
    /// Every dungeon, the world's own and the packs', read the first time it is wanted.
    pub fn dungeons(&mut self) -> Vec<Dungeon> {
        if self.dungeon_list.retail.is_none() {
            self.dungeon_list.retail = Some(self.session.retail_dungeons());
        }
        let retail = self.dungeon_list.retail.clone().unwrap_or_default();
        self.session.dungeons(&retail)
    }

    /// What a dungeon is called: its portals' name, else its landblock.
    #[must_use]
    pub fn dungeon_name(&self, d: &Dungeon) -> String {
        d.name.clone().unwrap_or_else(|| {
            self.text.table().fill(
                "dungeon.unnamed",
                &[("landblock", &format!("{:04X}", d.landblock))],
            )
        })
    }

    /// The landblock the camera works in: the dungeon it was taken into while it is in one of its
    /// rooms or building it, else the one it is over.
    #[must_use]
    pub fn work_block(&self) -> Option<(u16, f32, f32)> {
        if let Some(d) = self.dungeon_block {
            if self.mode == Mode::Dungeon || self.in_dungeon {
                let o = pick::block_origin(d);
                return Some((
                    d,
                    self.camera.position.x - o.x,
                    self.camera.position.y - o.y,
                ));
            }
        }
        landblock_of(self.camera.position)
    }

    /// Dungeon `lb`'s first cell: the world's own first, else the packs' first.
    fn first_cell(&self, lb: u16) -> Option<u16> {
        let cells = self.cells_of(lb);
        cells
            .keys()
            .copied()
            .find(|c| *c == lyceum_foundry::cells::FIRST_CELL)
            .or_else(|| self.session.cell_names(lb).keys().copied().min())
            .or_else(|| cells.keys().copied().next())
    }

    /// Where a portal into dungeon `lb` sends who uses it: its first cell, and the middle of it
    /// on its floor.
    ///
    /// # Errors
    /// A landblock with no cell.
    pub(super) fn dungeon_door(&self, lb: u16) -> Result<(u32, Vec3), String> {
        let first = self.first_cell(lb).ok_or_else(|| {
            self.say_with("messages.no_cells", &[("landblock", &format!("{lb:04X}"))])
        })?;
        let at = self
            .cell_floor(lb, first)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        Ok(((u32::from(lb) << 16) | u32::from(first), at))
    }

    /// The middle of cell `index` of landblock `lb` on its floor (landblock meters).
    fn cell_floor(&self, lb: u16, index: u16) -> Option<Vec3> {
        let cell = self.cells_of(lb).get(&index)?.clone();
        let env = self.session.environment(cell.environment)?;
        let cs = env.cells.get(usize::from(cell.cell_struct))?;
        let vs = &cs.vertex_array.vertices;
        #[allow(clippy::cast_precision_loss)]
        let n = vs.len().max(1) as f32;
        let mid = vs.iter().fold(Vec3::ZERO, |m, v| {
            m.add(localtoglobal(&cell.frame, v.position).mul(1.0 / n))
        });
        let floor = self
            .session
            .cell_at(lb, mid)
            .filter(|(c, _)| *c == index)
            .map_or(mid.z, |(_, z)| z);
        Some(Vec3::new(mid.x, mid.y, floor))
    }

    /// Into dungeon `lb`: the camera in its first cell, to build it on in dungeon mode, or with
    /// `edit` to select and change what is in it, with the Nearby list of its cells, statics and
    /// spawns brought forward.
    ///
    /// # Errors
    /// A landblock with no cell.
    pub fn enter_dungeon(&mut self, lb: u16, edit: bool) -> Result<(), String> {
        let first = self.first_cell(lb).ok_or_else(|| {
            self.say_with("messages.no_cells", &[("landblock", &format!("{lb:04X}"))])
        })?;
        let at = self
            .cell_floor(lb, first)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        self.dungeon_block = Some(lb);
        self.in_dungeon = true;
        self.camera.position = pick::block_origin(lb).add(Vec3::new(at.x, at.y, at.z + 1.6));
        self.rooms = None;
        if edit {
            self.mode = Mode::Select;
            self.nearby.stale();
            self.show_tab(super::Tab::Nearby);
        } else {
            self.mode = Mode::Dungeon;
        }
        Ok(())
    }

    /// A new dungeon: the nearest landblock to the camera with no cells and no buildings, the
    /// camera put under its middle to place the first piece in front of, in dungeon mode.
    ///
    /// # Errors
    /// The camera is off the world, or no landblock round it is free.
    pub fn new_dungeon(&mut self) -> Result<u16, String> {
        let (here, ..) =
            landblock_of(self.camera.position).ok_or_else(|| self.say("messages.off_world"))?;
        let (hx, hy) = (i32::from(here >> 8), i32::from(here & 0xFF));
        let free = |lb: u16| {
            self.session.cell_names(lb).is_empty()
                && self
                    .session
                    .statics(lb)
                    .is_none_or(|i| i.num_cells == 0 && i.buildings.is_empty())
        };
        let mut found = None;
        'out: for r in 1..=64_i32 {
            for dx in -r..=r {
                for dy in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (x, y) = (hx + dx, hy + dy);
                    let Ok(lb) = u16::try_from((x << 8) | y) else {
                        continue;
                    };
                    if (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y) && free(lb) {
                        found = Some(lb);
                        break 'out;
                    }
                }
            }
        }
        let lb = found.ok_or_else(|| self.say("messages.no_free_landblock"))?;
        let ground = self.session.ground(lb, 96.0, 96.0).unwrap_or(0.0);
        self.camera.position = pick::block_origin(lb).add(Vec3::new(
            96.0,
            96.0 - dungeon_ahead(),
            ground - UNDERGROUND,
        ));
        self.camera.yaw = 0.0;
        self.camera.pitch = 0.0;
        self.dungeon_block = Some(lb);
        self.in_dungeon = false;
        self.mode = Mode::Dungeon;
        self.panels.message = self.say_with(
            "messages.dungeon_started",
            &[("landblock", &format!("{lb:04X}"))],
        );
        Ok(lb)
    }

    /// A portal into dungeon `lb` from outdoors: a weenie of the pack, a blank of the server's
    /// portal named for the dungeon, sending who uses it to the middle of the dungeon's first
    /// cell; and a spawn of it on the ground ahead of the camera when the camera is outdoors,
    /// else in the middle of the dungeon's landblock. One step.
    ///
    /// # Errors
    /// No server database to make a portal from, a dungeon with no cell, or a name taken.
    pub fn portal_to_dungeon(&mut self, lb: u16) -> Result<(), String> {
        let first = self.first_cell(lb).ok_or_else(|| {
            self.say_with("messages.no_cells", &[("landblock", &format!("{lb:04X}"))])
        })?;
        let to = self
            .cell_floor(lb, first)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        let path = self
            .server_sql
            .clone()
            .ok_or_else(|| self.say("messages.no_server_database"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let world = String::from_utf8_lossy(&bytes);
        let dungeon = self
            .dungeons()
            .into_iter()
            .find(|d| d.landblock == lb)
            .map_or_else(|| format!("{lb:04X}"), |d| self.dungeon_name(&d));
        let name = format!("portal-{lb:04x}");
        let (file, sym) = self
            .session
            .new_weenie_file(&name)
            .map_err(|e| e.to_string())?;
        if self.session.sql_text(&file).is_some() {
            return Err(self.say_with("messages.name_taken", &[("name", &sym)]));
        }
        let text = content::template(&world, PORTAL_BLANK, &name, true)
            .ok_or_else(|| self.say_with("messages.no_such_class", &[("id", &PORTAL_BLANK)]))?;
        let title = self.say_with("dungeon.portal_name", &[("dungeon", &dungeon)]);
        let cell = (u32::from(lb) << 16) | u32::from(first);
        let text = with_destination(&text, cell, to, &title);
        let header = self.say_with(
            "dungeon.portal_header",
            &[("dungeon", &dungeon), ("cell", &format!("0x{cell:08X}"))],
        );
        // Where it stands: ahead of the camera outdoors, else over the dungeon.
        let (at_lb, at) = match self.camera_cell() {
            None => {
                let f = self.camera.forward();
                let len = (f.x * f.x + f.y * f.y).sqrt().max(1e-3);
                let p = self.camera.position;
                let ahead = Vec3::new(
                    p.x + f.x / len * PORTAL_AHEAD,
                    p.y + f.y / len * PORTAL_AHEAD,
                    p.z,
                );
                let (alb, x, y) =
                    landblock_of(ahead).ok_or_else(|| self.say("messages.off_world"))?;
                (
                    alb,
                    Vec3::new(x, y, self.session.ground(alb, x, y).unwrap_or(0.0)),
                )
            }
            Some(_) => (
                lb,
                Vec3::new(
                    96.0,
                    96.0,
                    self.session.ground(lb, 96.0, 96.0).unwrap_or(0.0),
                ),
            ),
        };
        let weenie = Weenie::Symbol(sym.clone());
        let applied = self.apply(
            &Op::Batch(vec![
                Op::WriteSql {
                    file,
                    text: Some(format!("/* {header} */\n{text}")),
                },
                Op::PlaceSpawn {
                    landblock: at_lb,
                    weenie: weenie.clone(),
                    origin: at,
                    heading: 0.0,
                },
            ]),
            false,
        )?;
        self.panels.weenie = Some(weenie);
        if let Some(file) = applied.spawned {
            if let Some(s) = self.session.spawns().into_iter().find(|s| s.file == file) {
                self.selection = Some(super::Selection::Spawn { file, guid: s.guid });
                self.also.clear();
            }
        }
        self.panels.message = self.say_with("messages.portal_made", &[("dungeon", &dungeon)]);
        Ok(())
    }

    /// The dungeon tab's head: the steps, the dungeons, and the buttons that start one or lead
    /// into one.
    pub(super) fn dungeons_section(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let steps = egui::CollapsingHeader::new(t.get("dungeon.steps_title"))
            .id_salt("dungeon steps")
            .open(Some(self.dungeon_list.steps))
            .show(ui, |ui| {
                for k in 1..=6 {
                    ui.label(t.get(&format!("dungeon.step_{k}")));
                }
            });
        if steps.header_response.clicked() {
            self.dungeon_list.steps = !self.dungeon_list.steps;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(t.get("dungeon.new"))
                .on_hover_text(t.get("dungeon.new_tip"))
                .clicked()
            {
                out.push(UiAction::NewDungeon);
            }
            let here = self
                .dungeon_block
                .or_else(|| self.work_block().map(|w| w.0));
            if let Some(lb) = here {
                if ui
                    .add_enabled(
                        self.server_sql.is_some(),
                        egui::Button::new(t.get("dungeon.portal_in")),
                    )
                    .on_hover_text(t.fill(
                        "dungeon.portal_in_tip",
                        &[("landblock", &format!("{lb:04X}"))],
                    ))
                    .on_disabled_hover_text(t.get("dungeon.portal_in_no_server"))
                    .clicked()
                {
                    out.push(UiAction::PortalToDungeon(lb));
                }
            }
            if ui
                .button(t.get("dungeon.all"))
                .on_hover_text(t.get("dungeon.all_tip"))
                .clicked()
            {
                out.push(UiAction::ShowTab(super::Tab::Dungeons));
            }
        });
        ui.separator();
    }

    /// The Dungeons tab: every dungeon, the world's and the packs', by name, with a button to go
    /// in and one to edit it.
    pub(super) fn dungeons_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let all = self.dungeons();
        ui.strong(t.fill("dungeon.list", &[("n", &all.len())]));
        {
            {
                ui.add(
                    egui::TextEdit::singleline(&mut self.dungeon_list.filter)
                        .hint_text(t.get("dungeon.list_filter"))
                        .desired_width(f32::INFINITY),
                );
                let f = self.dungeon_list.filter.trim().to_lowercase();
                let shown: Vec<(Dungeon, String)> = all
                    .into_iter()
                    .map(|d| {
                        let n = self.dungeon_name(&d);
                        (d, n)
                    })
                    .filter(|(d, n)| {
                        f.is_empty()
                            || n.to_lowercase().contains(&f)
                            || format!("{:04x}", d.landblock).contains(&f)
                    })
                    .collect();
                self.dungeon_list.rects.clear();
                egui::ScrollArea::vertical().id_salt("dungeons").show_rows(
                    ui,
                    20.0,
                    shown.len(),
                    |ui, rows| {
                        for i in rows {
                            let (d, name) = &shown[i];
                            ui.horizontal(|ui| {
                                let enter = ui
                                    .small_button(t.get("dungeon.enter"))
                                    .on_hover_text(t.get("dungeon.enter_tip"));
                                let edit = ui
                                    .small_button(t.get("dungeon.edit"))
                                    .on_hover_text(t.get("dungeon.edit_tip"));
                                ui.label(name);
                                ui.weak(t.fill(
                                    "dungeon.list_row",
                                    &[
                                        ("landblock", &format!("{:04X}", d.landblock)),
                                        ("n", &d.cells),
                                        (
                                            "whose",
                                            &t.get(if d.pack {
                                                "dungeon.packs"
                                            } else {
                                                "dungeon.worlds"
                                            }),
                                        ),
                                    ],
                                ));
                                self.dungeon_list
                                    .rects
                                    .push((d.landblock, enter.rect, edit.rect));
                                if enter.clicked() {
                                    out.push(UiAction::EnterDungeon(d.landblock, false));
                                }
                                if edit.clicked() {
                                    out.push(UiAction::EnterDungeon(d.landblock, true));
                                }
                            });
                        }
                    },
                );
            }
        }
    }
}

/// How far behind the middle of a new dungeon's landblock the camera starts, so the first piece,
/// placed ahead of it, lands in the middle.
fn dungeon_ahead() -> f32 {
    super::dungeon::AHEAD
}

/// Weenie `text` (one weenie's SQL) named `name` and sending who uses it to cell `cell` at `at`
/// (its landblock's meters): its destination, position 2, set where it has one and added where
/// it has none.
#[must_use]
pub fn with_destination(text: &str, cell: u32, at: Vec3, name: &str) -> String {
    let mut doc = SqlDoc::parse(text);
    set_destination(&mut doc, cell, at);
    doc.set_property(content::PropertyKind::String, 1, name, None);
    doc.render()
}

/// `doc`'s weenie sending who uses it to cell `cell` at `at`: its destination set or added.
pub fn set_destination(doc: &mut SqlDoc, cell: u32, at: Vec3) {
    let place = [
        ("position_Type", "2".to_owned()),
        ("obj_Cell_Id", format!("0x{cell:08X}")),
        ("origin_X", format!("{:.3}", at.x)),
        ("origin_Y", format!("{:.3}", at.y)),
        ("origin_Z", format!("{:.3}", at.z)),
        ("angles_W", "1".to_owned()),
        ("angles_X", "0".to_owned()),
        ("angles_Y", "0".to_owned()),
        ("angles_Z", "0".to_owned()),
    ];
    let table = "weenie_properties_position";
    let row = doc.rows(table).into_iter().find(|r| {
        doc.cell(*r, "position_Type")
            .is_some_and(|v| v.trim() == "2")
    });
    match row {
        Some(r) => {
            for (c, v) in &place {
                doc.set_cell(r, c, v);
            }
        }
        None => {
            let mut columns = vec!["object_Id"];
            columns.extend(place.iter().map(|(c, _)| *c));
            doc.add_row(table, &columns, &place, Some("position_Type"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's portal into a dungeon)
    use super::*;

    const PORTAL: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:portal-019e}}, 'portal-019e', 7, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:portal-019e}}, 1, 'Portal');
";

    #[test]
    fn a_portal_is_given_a_destination_in_the_cell_and_named_for_the_dungeon() {
        let at = Vec3::new(40.0, -40.0, 0.5);
        let once = with_destination(PORTAL, 0x019E_0100, at, "Drudge Hideout Portal");
        let doc = SqlDoc::parse(&once);
        let rows = doc.rows("weenie_properties_position");
        assert_eq!(rows.len(), 1, "{once}");
        let cell = |c: &str| doc.cell(rows[0], c).unwrap();
        assert_eq!(
            (cell("position_Type"), cell("obj_Cell_Id"), cell("origin_Y")),
            (
                "2".to_owned(),
                "0x019E0100".to_owned(),
                "-40.000".to_owned()
            )
        );
        assert_eq!(cell("object_Id"), "{{wcid:portal-019e}}");
        assert_eq!(
            doc.property(content::PropertyKind::String, 1).as_deref(),
            Some("'Drudge Hideout Portal'")
        );
        // Sent somewhere else, it has the one destination still.
        let twice = with_destination(&once, 0x019E_0114, at, "Drudge Hideout Portal");
        let doc = SqlDoc::parse(&twice);
        let rows = doc.rows("weenie_properties_position");
        assert_eq!(rows.len(), 1);
        assert_eq!(doc.cell(rows[0], "obj_Cell_Id").unwrap(), "0x019E0114");
    }
}
