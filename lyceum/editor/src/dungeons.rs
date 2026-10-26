//! The world's dungeons from the editor's side: the list (the world's own and the packs', named
//! as a pack names them or by the portals that lead in), dungeon mode (going into one at its drop
//! point and back out where the camera was), starting a new one on the open sea floor, and the
//! dungeon's name and look, kept in the pack.

use std::collections::BTreeMap;

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::Vec3;
use lyceum_foundry::content::{self, SqlDoc};
use lyceum_foundry::dungeons::Dungeon;
use lyceum_foundry::edit::Op;
use lyceum_foundry::pick;

use crate::camera::{landblock_of, FlyCamera};

use super::{EditorApp, Mode, UiAction};

/// A portal's weenie type, as the server numbers it.
const PORTAL_KIND: u32 = 7;

/// How far under a new dungeon's landblock's ground the camera starts, in meters.
const UNDERGROUND: f32 = 20.0;

/// How high over the floor the camera goes in, in meters: a body's eye.
const EYE: f32 = 1.6;

/// Dungeon mode: the dungeon being edited, and where the camera was before it went in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InDungeon {
    pub landblock: u16,
    pub back: FlyCamera,
}

/// The dungeon list's state: the world's own dungeons once read, the typed filter, the landblock
/// typed for a new dungeon to start near, and where each row was drawn.
#[derive(Debug, Default)]
pub struct DungeonList {
    pub(super) retail: Option<BTreeMap<u16, u32>>,
    /// Every dungeon, as last listed (till the next edit, or the server's weenies learned).
    pub(super) listed: Option<Vec<Dungeon>>,
    pub filter: String,
    pub start_near: String,
    /// Each row as drawn: its landblock and its rectangle.
    pub rects: Vec<(u16, egui::Rect)>,
    /// The New dungeon button, and the dungeon card's Leave button, as drawn.
    pub new_button: Option<egui::Rect>,
    pub leave_button: Option<egui::Rect>,
    /// The portals that lead into a dungeon, as last found (till the next edit).
    pub entrances: Option<(u16, Vec<lyceum_foundry::edit::Entrance>)>,
    /// The dungeon whose outdoor entrances the map marks: the one last gone into.
    pub marked: Option<u16>,
    /// Its marks on the map, as last drawn: each portal into it with where it stands.
    pub map_marks: Vec<(egui::Rect, MapMark)>,
    /// The name being typed in dungeon mode, until it is left.
    pub naming: Option<String>,
    /// The world's own dungeon whose taking away waits for the creator's word.
    pub confirm: Option<u16>,
    /// The card's look dropdown, its Delete, and the question's Delete, as drawn.
    pub look_combo: Option<egui::Rect>,
    pub delete_button: Option<egui::Rect>,
    pub confirm_button: Option<egui::Rect>,
    /// Each row's Delete in the list, as drawn.
    pub delete_rects: Vec<(u16, egui::Rect)>,
}

/// What a mark of a dungeon on the map is.
#[derive(Clone, Debug, PartialEq)]
pub enum MapMark {
    /// A portal into it: its landblock and where it stands there.
    Entrance(u16, Vec3),
}

impl EditorApp {
    /// Every dungeon, the world's own and the packs', read the first time it is wanted.
    pub fn dungeons(&mut self) -> Vec<Dungeon> {
        if let Some(listed) = &self.dungeon_list.listed {
            return listed.clone();
        }
        if self.dungeon_list.retail.is_none() {
            // Before the gathering off the frame has found them.
            self.stats.dungeon_scans += 1;
            self.dungeon_list.retail = Some(self.session.retail_dungeons());
        }
        let retail = self.dungeon_list.retail.clone().unwrap_or_default();
        let listed = self.session.dungeons(&retail);
        self.dungeon_list.listed = Some(listed.clone());
        listed
    }

    /// What a dungeon is called: as the pack names it, else its portals' name, else its
    /// landblock.
    #[must_use]
    pub fn dungeon_name(&self, d: &Dungeon) -> String {
        d.name.clone().unwrap_or_else(|| {
            self.text.table().fill(
                "dungeon.unnamed",
                &[("landblock", &format!("{:04X}", d.landblock))],
            )
        })
    }

    /// The name of dungeon `lb`, as the list shows it.
    #[must_use]
    pub fn dungeon_name_of(&mut self, lb: u16) -> String {
        self.dungeons()
            .into_iter()
            .find(|d| d.landblock == lb)
            .map_or_else(|| format!("{lb:04X}"), |d| self.dungeon_name(&d))
    }

    /// The dungeon being edited, in dungeon mode.
    #[must_use]
    pub fn dungeon_mode(&self) -> Option<u16> {
        self.dungeon.map(|d| d.landblock)
    }

    /// Whether the landscape grid is drawn: as the View menu says, but never in a dungeon (in
    /// dungeon mode, or with the camera in a room not seen from outside). The setting stands for
    /// when the camera is out again.
    pub fn landscape_grid_shown(&mut self) -> bool {
        self.panels.show_grid
            && self.dungeon.is_none()
            && self.camera_cell().is_none_or(|(_, seen)| seen)
    }

    /// The landblock the camera works in: in dungeon mode, or with the camera in one of a
    /// dungeon's rooms, the dungeon's; else the one it is over.
    #[must_use]
    pub fn work_block(&self) -> Option<(u16, f32, f32)> {
        let d = self
            .dungeon_mode()
            .or(self.dungeon_block.filter(|_| self.in_dungeon));
        if let Some(d) = d {
            let o = pick::block_origin(d);
            return Some((
                d,
                self.camera.position.x - o.x,
                self.camera.position.y - o.y,
            ));
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

    /// Where who goes into dungeon `lb` arrives, its drop point: the destination of the portals
    /// that lead in (those standing outdoors first, then the one most share), else the middle of
    /// its first cell on its floor. The cell, and the place in its landblock.
    ///
    /// # Errors
    /// A dungeon no portal leads into that has no cell.
    pub fn drop_point(&mut self, lb: u16) -> Result<(u32, Vec3), String> {
        let entrances = self.entrances(lb);
        if let Some(to) = lyceum_foundry::edit::drop_point(&entrances) {
            return Ok(to);
        }
        let first = self.first_cell(lb).ok_or_else(|| {
            self.say_with("messages.no_cells", &[("landblock", &format!("{lb:04X}"))])
        })?;
        let at = self
            .cell_floor(lb, first)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        Ok(((u32::from(lb) << 16) | u32::from(first), at))
    }

    /// The middle of cell `index` of landblock `lb` on its floor (landblock meters).
    pub(super) fn cell_floor(&self, lb: u16, index: u16) -> Option<Vec3> {
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

    /// Into dungeon `lb`, in dungeon mode: the camera at its drop point (or, for a dungeon with
    /// no room yet, under the middle of its landblock), its grid shown. Leaving puts
    /// the camera back where it was before the first dungeon gone into.
    ///
    /// # Errors
    /// None for a dungeon the packs name; a landblock with no cells otherwise.
    pub fn enter_dungeon(&mut self, lb: u16) -> Result<(), String> {
        let empty = self.cells_of(lb).is_empty();
        let at = if empty {
            None
        } else {
            Some(self.drop_point(lb)?)
        };
        if self.walking {
            self.set_walking(false)?;
        }
        let back = self.dungeon.map_or(self.camera, |d| d.back);
        // Outdoors again, the world view is what shows, not the list it was entered from.
        if self.dungeon.is_none() {
            self.show_tab(super::Tab::World);
        }
        let mut camera = self.camera;
        match at {
            Some((cell, p)) => {
                let o = pick::block_origin(u16::try_from(cell >> 16).unwrap_or(lb));
                camera.position = Vec3::new(o.x + p.x, o.y + p.y, o.z + p.z + EYE);
                camera.pitch = -0.2;
            }
            None => {
                camera.position = pick::block_origin(lb).add(Vec3::new(96.0, 96.0, -UNDERGROUND));
                camera.pitch = 0.0;
            }
        }
        self.camera = camera;
        self.dungeon = Some(InDungeon {
            landblock: lb,
            back,
        });
        self.dungeon_block = Some(lb);
        self.dungeon_list.marked = Some(lb);
        self.dungeon_list.naming = None;
        self.in_dungeon = !empty;
        self.rooms.clear();
        self.selection = None;
        self.also.clear();
        if self.mode == Mode::Terrain {
            self.mode = Mode::Select;
        }
        self.layout.reset();
        self.kit.chosen = None;
        self.kit.look_of = None;
        Ok(())
    }

    /// Out of dungeon mode: the camera back where it was before going in.
    pub fn leave_dungeon(&mut self) {
        let Some(d) = self.dungeon.take() else {
            return;
        };
        if self.walking {
            let _ = self.set_walking(false);
        }
        self.camera = d.back;
        self.dungeon_block = None;
        self.in_dungeon = false;
        self.rooms.clear();
        self.selection = None;
        self.also.clear();
        self.layout.reset();
        self.kit.chosen = None;
        self.dungeon_list.naming = None;
    }

    /// A new dungeon: the landblock nearest `near` (else the camera's) of the open sea floor that
    /// nothing uses, gone into in dungeon mode.
    ///
    /// # Errors
    /// The camera is off the world, or no landblock round it is free.
    pub fn new_dungeon(&mut self, near: Option<u16>) -> Result<u16, String> {
        let here = match near {
            Some(lb) => lb,
            None => {
                landblock_of(self.camera.position)
                    .ok_or_else(|| self.say("messages.off_world"))?
                    .0
            }
        };
        let lb = self
            .room_near(here)
            .ok_or_else(|| self.say("messages.no_free_landblock"))?;
        self.enter_dungeon(lb)?;
        self.panels.message = self.say_with(
            "messages.dungeon_started",
            &[("landblock", &format!("{lb:04X}"))],
        );
        Ok(lb)
    }

    /// The portals that lead into dungeon `lb`, found once till the next edit.
    pub fn entrances(&mut self, lb: u16) -> Vec<lyceum_foundry::edit::Entrance> {
        if let Some((at, found)) = &self.dungeon_list.entrances {
            if *at == lb {
                return found.clone();
            }
        }
        let found = self.session.entrances(lb);
        self.dungeon_list.entrances = Some((lb, found.clone()));
        found
    }

    /// The landblock nearest `from` with room for a new dungeon ([`Self::room_for_dungeon`]),
    /// ring by ring outward.
    #[must_use]
    pub fn room_near(&self, from: u16) -> Option<u16> {
        let (hx, hy) = (i32::from(from >> 8), i32::from(from & 0xFF));
        for r in 0..=0xFF_i32 {
            for dx in -r..=r {
                for dy in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (x, y) = (hx + dx, hy + dy);
                    if !(0..0xFF).contains(&x) || !(0..0xFF).contains(&y) {
                        continue;
                    }
                    let Ok(lb) = u16::try_from((x << 8) | y) else {
                        continue;
                    };
                    if self.room_for_dungeon(lb).is_ok()
                        && self.session.dungeon_info(lb) == Default::default()
                    {
                        return Some(lb);
                    }
                }
            }
        }
        None
    }

    /// Whether landblock `lb` has room for a new dungeon, as the packs leave it: the open sea
    /// floor, where retail builds its dungeons, with no statics and no rooms
    /// ([`lyceum_foundry::room::room_for_dungeon`]).
    ///
    /// # Errors
    /// What uses it, as a sentence.
    pub fn room_for_dungeon(&self, lb: u16) -> Result<(), String> {
        use lyceum_foundry::room::Taken;
        let rooms = self.session.cell_names(lb).len();
        lyceum_foundry::room::room_for_dungeon(
            lb,
            self.session.terrain(lb).as_ref(),
            self.session.statics(lb).as_ref(),
            rooms,
        )
        .map_err(|why| {
            let key = match why {
                Taken::Land => "messages.room_land",
                Taken::Statics => "messages.room_statics",
                Taken::Rooms => "messages.room_rooms",
                Taken::Edge => "messages.room_edge",
            };
            self.say_with(key, &[("landblock", &format!("{lb:04X}"))])
        })
    }

    /// The portal weenies that lead into dungeon `lb`, by their destination: the server's own
    /// (or the pack's copy that takes one's place) and the packs' own.
    #[must_use]
    pub fn portals_into(&self, lb: u16) -> Vec<lyceum_foundry::edit::Weenie> {
        use lyceum_foundry::edit::Weenie;
        let leads = |f: &lyceum_foundry::sql_ids::WeenieFacts| {
            f.kind == Some(PORTAL_KIND) && f.destination.is_some_and(|d| d >> 16 == u32::from(lb))
        };
        let copies = self.session.pack_classes();
        let mut out: Vec<Weenie> = Vec::new();
        for (c, f) in self.session.server_weenies() {
            if leads(copies.get(c).unwrap_or(f)) {
                out.push(Weenie::Class(*c));
            }
        }
        for (c, f) in &copies {
            if leads(f) && !out.contains(&Weenie::Class(*c)) {
                out.push(Weenie::Class(*c));
            }
        }
        for (sym, f) in self.session.pack_weenies() {
            if leads(&f) {
                out.push(Weenie::Symbol(sym));
            }
        }
        out
    }

    /// The portals that lead into dungeon `lb`, each with its name as the packs have it.
    #[must_use]
    pub fn portal_names(&self, lb: u16) -> Vec<(lyceum_foundry::edit::Weenie, String)> {
        use lyceum_foundry::edit::Weenie;
        let copies = self.session.pack_classes();
        let own = self.session.pack_weenies();
        self.portals_into(lb)
            .into_iter()
            .map(|w| {
                let name = match &w {
                    Weenie::Class(c) => copies
                        .get(c)
                        .cloned()
                        .or_else(|| self.session.class_facts(*c))
                        .and_then(|f| f.name),
                    Weenie::Symbol(s) => own.get(s).and_then(|f| f.name.clone()),
                };
                (w, name.unwrap_or_default())
            })
            .collect()
    }

    /// Name dungeon `lb`, kept in the pack, and its portals with it: each portal that leads in
    /// (the server's own, by the pack's copy of its weenie, and the packs' own) called by the new
    /// name where it said the old one, else by the new name. One step.
    ///
    /// # Errors
    /// The pack would not take it, or a server portal's weenie cannot be read to copy.
    pub fn name_dungeon(&mut self, lb: u16, name: &str) -> Result<(), String> {
        let look = self.session.dungeon_info(lb).look;
        let old = self.dungeon_name_of(lb);
        let name = Some(name.trim().to_owned()).filter(|n| !n.is_empty());
        let mut ops = vec![Op::SetDungeon {
            landblock: lb,
            name: name.clone(),
            look,
        }];
        if let Some(new) = &name {
            for w in self.portals_into(lb) {
                let Ok((file, text)) = self.writable_sql(&w) else {
                    continue;
                };
                let mut doc = SqlDoc::parse(&text);
                let now = doc
                    .property(content::PropertyKind::String, 1)
                    .map(|v| content::unquote(&v))
                    .unwrap_or_default();
                let renamed = if !old.is_empty() && now.contains(&old) {
                    now.replace(&old, new)
                } else {
                    new.clone()
                };
                if renamed == now {
                    continue;
                }
                doc.set_property(content::PropertyKind::String, 1, &renamed, None);
                ops.push(Op::WriteSql {
                    file,
                    text: Some(doc.render()),
                });
            }
        }
        let renamed = ops.len() - 1;
        self.apply(&Op::Batch(ops), false)?;
        self.dungeon_list.listed = None;
        if renamed > 0 {
            self.panels.message = self.say_with("dungeon.renamed", &[("n", &renamed)]);
        }
        Ok(())
    }

    /// Choose the look dungeon `lb` is drawn in, kept in the pack: the look of the world's dungeon
    /// `of`, or each piece as the world draws it most (`None`). Every room of it is drawn in that
    /// look, the world's own too: by the world's own list where it draws the piece so, else each
    /// surface by its role. One step.
    ///
    /// # Errors
    /// The pack would not take it.
    pub fn choose_dungeon_look(&mut self, lb: u16, of: Option<u16>) -> Result<(), String> {
        self.wait_for_pieces();
        let name = self.session.dungeon_info(lb).name;
        let mut ops = vec![Op::SetDungeon {
            landblock: lb,
            name,
            look: of,
        }];
        let family = of.and_then(|l| self.family_of_landblock(l));
        let pieces = self.kit.pieces.clone().unwrap_or_default();
        let cells = self.cells_of(lb);
        let (mut exact, mut by_role, mut kept) = (0usize, 0usize, 0usize);
        for placed in lyceum_foundry::kit::placed_pieces(&cells) {
            let Some(p) = pieces.iter().find(|p| p.environment == placed.environment) else {
                continue;
            };
            let (surfaces, how) = match family {
                Some(f) => lyceum_foundry::kit::dress(p, f, self.kit.dress.get(f)),
                None => (p.surfaces.clone(), lyceum_foundry::kit::Dressed::Exact),
            };
            match how {
                lyceum_foundry::kit::Dressed::Exact => exact += placed.cells.len(),
                lyceum_foundry::kit::Dressed::ByRole => by_role += placed.cells.len(),
                lyceum_foundry::kit::Dressed::Kept => kept += placed.cells.len(),
            }
            for c in &placed.cells {
                if cells.get(c).is_some_and(|cell| cell.surfaces != surfaces) {
                    ops.push(Op::DressCell {
                        landblock: lb,
                        cell: *c,
                        surfaces: surfaces.clone(),
                    });
                }
            }
        }
        self.apply(&Op::Batch(ops), false)?;
        self.kit.look_of = None;
        self.kit.look = family;
        self.panels.message =
            self.say_with("dungeon.restyled", &[("exact", &exact), ("role", &by_role)]);
        if kept > 0 {
            let more = self.say_with("dungeon.restyled_kept", &[("n", &kept)]);
            self.panels.message = format!("{} {more}", self.panels.message);
        }
        Ok(())
    }

    /// Whether the card offers look `f`: one the world draws more than one dungeon in, with a
    /// surface for each role its pieces' surfaces cover.
    #[must_use]
    pub fn offered_look(&self, f: usize) -> bool {
        self.kit
            .families
            .get(f)
            .is_some_and(|fam| fam.landblocks.len() >= 2)
            && self.kit.dress.get(f).is_some_and(|d| !d.is_empty())
    }

    /// The family of looks the world's dungeon `lb` is drawn in.
    #[must_use]
    pub fn family_of_landblock(&self, lb: u16) -> Option<usize> {
        self.kit
            .families
            .iter()
            .position(|f| f.landblocks.iter().any(|(l, _)| *l == lb))
    }

    /// What taking dungeon `lb` away takes: its rooms, the things standing in them (statics and
    /// spawns), and the portals that lead into it.
    pub fn dungeon_contents(&mut self, lb: u16) -> (usize, usize, usize) {
        let cells = self.cells_of(lb);
        let rooms = cells
            .values()
            .filter(|c| c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0)
            .count();
        let ids: std::collections::BTreeSet<u32> = cells
            .keys()
            .map(|n| (u32::from(lb) << 16) | u32::from(*n))
            .collect();
        self.session.show_server_spawns([lb]);
        let things = cells
            .values()
            .map(|c| c.static_objects.len())
            .sum::<usize>()
            + self
                .session
                .spawns()
                .iter()
                .filter(|s| ids.contains(&s.cell))
                .count();
        let portals = self.entrances(lb).len();
        (rooms, things, portals)
    }

    /// Take dungeon `lb` away, whole: its rooms, what stands in them, the portals that lead into
    /// it (their spawns, and the packs' own portal weenies), and its name and look. One step; the
    /// camera leaves it first when it is in it.
    ///
    /// # Errors
    /// The pack would not take it.
    pub fn delete_dungeon(&mut self, lb: u16) -> Result<(), String> {
        let cells = self.cells_of(lb);
        let names = self.session.cell_names(lb);
        let mut ops: Vec<Op> = names
            .values()
            .map(|name| Op::RemoveCell {
                landblock: lb,
                name: name.clone(),
            })
            .collect();
        let world: Vec<u16> = cells
            .iter()
            .filter(|(n, c)| {
                !names.contains_key(n)
                    && c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0
            })
            .map(|(n, _)| *n)
            .collect();
        if !world.is_empty() {
            self.session.show_server_spawns([lb]);
            ops.push(Op::TakeCells {
                landblock: lb,
                cells: world,
            });
        }
        ops.push(Op::SetDungeon {
            landblock: lb,
            name: None,
            look: None,
        });
        // The portals that lead in: their spawns, and the packs' own weenies of them.
        let spawns = self.session.spawns();
        for e in self.entrances(lb) {
            if e.landblock == lb {
                continue;
            }
            match spawns.iter().find(|s| s.guid == e.guid) {
                Some(s) if s.server => ops.push(Op::RemoveServerSpawn {
                    file: s.file.clone(),
                    guid: s.guid.clone(),
                }),
                Some(s) => {
                    if !ops
                        .iter()
                        .any(|o| matches!(o, Op::RemoveSpawn { file } if *file == s.file))
                    {
                        ops.push(Op::RemoveSpawn {
                            file: s.file.clone(),
                        });
                    }
                }
                None => ops.push(Op::RemoveServerSpawn {
                    file: self.session.override_file(e.landblock),
                    guid: e.guid.clone(),
                }),
            }
        }
        for w in self.portals_into(lb) {
            if let lyceum_foundry::edit::Weenie::Symbol(_) = &w {
                if let Some((file, true)) = self.session.weenie_file(&w) {
                    ops.push(Op::WriteSql { file, text: None });
                }
            }
        }
        if self.dungeon_mode() == Some(lb) {
            self.leave_dungeon();
        }
        self.apply(&Op::Batch(ops), false)?;
        self.dungeon_list.confirm = None;
        self.dungeon_list.listed = None;
        self.panels.message = self.say("dungeon.deleted");
        Ok(())
    }

    /// The dungeon card at the head of the tools in dungeon mode: its name with Leave and Delete,
    /// and folded under them its new name, its look (chosen from the world's, every room drawn in
    /// it) and the look's textures.
    pub(super) fn dungeon_card(&mut self, ui: &mut egui::Ui, lb: u16, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let name = self.dungeon_name_of(lb);
        let id = ui.make_persistent_id(("dungeon card", lb));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                ui.strong(t.fill("dungeon.header", &[("name", &name)]));
                let leave = ui.button(t.get("dungeon.leave"));
                self.dungeon_list.leave_button = Some(leave.rect);
                if leave.clicked() {
                    out.push(UiAction::LeaveDungeon);
                }
                let del = ui.button(t.get("dungeon.delete_short"));
                self.dungeon_list.delete_button = Some(del.rect);
                if del.clicked() {
                    out.push(UiAction::AskDeleteDungeon(lb));
                }
            })
            .body(|ui| self.dungeon_card_body(ui, lb, out));
        ui.separator();
    }

    /// The card's folded part: the new name, the look and its textures.
    fn dungeon_card_body(&mut self, ui: &mut egui::Ui, lb: u16, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let now = self.dungeon_name_of(lb);
        egui::Grid::new(("dungeon card", lb))
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(t.get("dungeon.rename"));
                let typed = self
                    .dungeon_list
                    .naming
                    .get_or_insert_with(|| self.session.dungeon_info(lb).name.unwrap_or_default());
                let r = ui.add(
                    egui::TextEdit::singleline(typed)
                        .hint_text(now)
                        .desired_width(200.0),
                );
                if r.lost_focus() {
                    let typed = typed.clone();
                    if Some(typed.trim()) != self.session.dungeon_info(lb).name.as_deref() {
                        out.push(UiAction::NameDungeon(lb, typed));
                    }
                }
                ui.end_row();
                ui.label(t.get("dungeon.look"));
                let shown = self.dungeon_look(lb);
                let shown_name = shown.map_or_else(
                    || t.get("dungeon.no_look").to_owned(),
                    |f| self.family_name(f),
                );
                let mut pick: Option<Option<u16>> = None;
                self.kit.look_rects.clear();
                let combo = egui::ComboBox::from_id_salt("dungeon look")
                    .selected_text(shown_name)
                    .width(200.0)
                    .height(360.0)
                    .show_ui(ui, |ui| {
                        for f in 0..self.kit.families.len() {
                            if !self.offered_look(f) {
                                continue;
                            }
                            let first = self.kit.families[f].landblocks[0].0;
                            let label = self.family_name(f);
                            let r = ui
                                .horizontal(|ui| {
                                    let r = ui.selectable_label(shown == Some(f), label);
                                    self.swatches(ui, f, 16.0, 6);
                                    r
                                })
                                .inner;
                            self.kit.look_rects.push((f, r.rect));
                            if r.clicked() {
                                pick = Some(Some(first));
                            }
                        }
                    });
                self.dungeon_list.look_combo = Some(combo.response.rect);
                if let Some(p) = pick {
                    out.push(UiAction::DungeonLook(lb, p));
                }
                ui.end_row();
            });
        if let Some(f) = self.dungeon_look(lb) {
            ui.horizontal_wrapped(|ui| {
                self.swatches(ui, f, 34.0, 8);
            });
        }
    }

    /// The question before a dungeon is taken away, over the whole window.
    pub(super) fn delete_dungeon_question(&mut self, ctx: &egui::Context, out: &mut Vec<UiAction>) {
        let Some(lb) = self.dungeon_list.confirm else {
            return;
        };
        let name = self.dungeon_name_of(lb);
        let t = self.text.table();
        let shown = egui::Modal::new(egui::Id::new("delete dungeon")).show(ctx, |ui| {
            ui.label(t.fill("dungeon.delete_confirm", &[("name", &name)]));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let yes = ui.button(t.get("dungeon.delete_yes"));
                self.dungeon_list.confirm_button = Some(yes.rect);
                if yes.clicked() {
                    out.push(UiAction::DeleteDungeon(lb));
                }
                if ui.button(t.get("dungeon.delete_no")).clicked() {
                    out.push(UiAction::KeepDungeon);
                }
            });
        });
        if shown.should_close() {
            out.push(UiAction::KeepDungeon);
        }
    }

    /// The textures of look `f` as swatches `side` points square, at most `most`.
    fn swatches(&mut self, ui: &mut egui::Ui, f: usize, side: f32, most: usize) {
        let textures = self
            .kit
            .families
            .get(f)
            .map(|fam| fam.textures.clone())
            .unwrap_or_default();
        for tex in textures.into_iter().take(most) {
            let (r, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            match self.texture_picture(tex) {
                Some(id) => {
                    ui.painter().image(
                        id,
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                None => {
                    ui.painter()
                        .rect_filled(r, 2.0, egui::Color32::from_gray(60));
                }
            }
        }
    }

    /// Where portal spawn `s` sends who uses it (`Some(None)`: nowhere yet); `None` when it is not
    /// a portal.
    #[must_use]
    pub fn portal_destination(&self, s: &lyceum_foundry::edit::Spawn) -> Option<Option<u32>> {
        let facts = match self.session.spawn_weenie(s)? {
            lyceum_foundry::edit::Weenie::Symbol(sym) => self.session.pack_weenies().remove(&sym),
            lyceum_foundry::edit::Weenie::Class(c) => self
                .session
                .pack_classes()
                .remove(&c)
                .or_else(|| self.session.class_facts(c)),
        }?;
        (facts.kind == Some(PORTAL_KIND)).then_some(facts.destination)
    }

    /// Where portal spawn `s` sends who uses it in its cell's landblock, when its weenie says.
    #[must_use]
    pub fn portal_destination_at(&self, s: &lyceum_foundry::edit::Spawn) -> Option<Vec3> {
        let facts = match self.session.spawn_weenie(s)? {
            lyceum_foundry::edit::Weenie::Symbol(sym) => self.session.pack_weenies().remove(&sym),
            lyceum_foundry::edit::Weenie::Class(c) => self
                .session
                .pack_classes()
                .remove(&c)
                .or_else(|| self.session.class_facts(c)),
        }?;
        facts.destination_at.map(|[x, y, z]| Vec3::new(x, y, z))
    }

    /// The Dungeons tab: every dungeon, the world's and the packs', by landblock and name; a click on
    /// one goes into it. New dungeon starts one near a landblock typed, or near the camera.
    pub(super) fn dungeons_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        ui.horizontal_wrapped(|ui| {
            let b = ui
                .button(t.get("dungeon.new"))
                .on_hover_text(t.get("dungeon.new_tip"));
            self.dungeon_list.new_button = Some(b.rect);
            if b.clicked() {
                let near = u16::from_str_radix(self.dungeon_list.start_near.trim(), 16).ok();
                out.push(UiAction::NewDungeon(near));
            }
            ui.label(t.get("dungeon.near"));
            ui.add(
                egui::TextEdit::singleline(&mut self.dungeon_list.start_near)
                    .hint_text(t.get("dungeon.near_hint"))
                    .desired_width(70.0),
            );
        });
        ui.separator();
        let all = self.dungeons();
        ui.strong(t.fill("dungeon.list", &[("n", &all.len())]));
        super::panels::search_box(
            ui,
            &mut self.dungeon_list.filter,
            t.get("dungeon.list_filter"),
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
        self.dungeon_list.delete_rects.clear();
        let here = self.dungeon_mode();
        let area = egui::ScrollArea::vertical()
            .id_salt("dungeons")
            .auto_shrink(false);
        let rows = egui::Id::new("dungeons rows");
        super::seen::rows(ui, rows, area, shown.len(), |ui, i| {
            let (d, name) = &shown[i];
            ui.horizontal(|ui| {
                ui.monospace(format!("{:04X}", d.landblock));
                let del = ui.small_button(t.get("dungeon.delete_short"));
                self.dungeon_list.delete_rects.push((d.landblock, del.rect));
                if del.clicked() {
                    out.push(UiAction::AskDeleteDungeon(d.landblock));
                }
                let row = ui.selectable_label(here == Some(d.landblock), name);
                ui.weak(t.fill("dungeon.cells_n", &[("n", &d.cells)]));
                self.dungeon_list.rects.push((d.landblock, row.rect));
                if row.clicked() {
                    out.push(UiAction::EnterDungeon(d.landblock));
                }
            });
        });
    }
}

/// `doc`'s weenie sending who uses it to cell `cell` at `at` (its landblock's meters): its
/// destination, position 2, set where it has one and added where it has none.
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
    fn a_portal_is_given_a_destination_in_the_cell_and_has_the_one_however_often_it_is_sent() {
        let at = Vec3::new(40.0, -40.0, 0.5);
        let mut doc = SqlDoc::parse(PORTAL);
        set_destination(&mut doc, 0x019E_0100, at);
        let rows = doc.rows("weenie_properties_position");
        assert_eq!(rows.len(), 1, "{}", doc.render());
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
        // Sent somewhere else, it has the one destination still.
        set_destination(&mut doc, 0x019E_0114, at);
        let rows = doc.rows("weenie_properties_position");
        assert_eq!(rows.len(), 1);
        assert_eq!(doc.cell(rows[0], "obj_Cell_Id").unwrap(), "0x019E0114");
    }
}
