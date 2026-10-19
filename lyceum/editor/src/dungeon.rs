//! The dungeon kit, from the editor's side: the piece browser (every piece retail's cells are
//! built of, by picture, floor plan, size and portal count, grouped and filtered by the look
//! retail draws it in, its shape, the pieces retail joins it to and the dungeons that use it),
//! placing a piece so one of its portals meets a free portal of the pack's cells in a
//! quarter-turn step, the pack's cells of the camera's landblock, each to take away, and the
//! landblock's own cells with their pieces.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::{Frame, Quat, Vec3};
use lyceum_foundry::edit::Op;
use lyceum_foundry::pieces::{opening, snap, Catalog, Family, Opening, Piece, Shape};
use lyceum_foundry::source::{Link, Rotation};

use crate::view::View;

use super::{EditorApp, UiAction};

/// How far from the pointer's ray a free portal is still the one a click means, in meters.
pub const REACH: f32 = 4.0;

/// How far in front of the camera a piece with no portal to meet is placed, in meters.
pub const AHEAD: f32 = 12.0;

/// How the browser groups its pieces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    /// By the look retail draws each in most.
    #[default]
    Look,
    Shape,
    None,
}

/// What a group of the browser is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKey {
    Look(usize),
    Shape(Shape),
}

/// The browser's state: the pieces and their families of looks once gathered (off the frame
/// loop), their environments as read, the filters and the grouping, the turn step, the chosen
/// piece and where each was drawn.
#[derive(Default)]
pub struct Kit {
    pub pieces: Option<std::sync::Arc<Vec<Piece>>>,
    pub families: Vec<Family>,
    /// The list the filters gave last, and what they were.
    listed: Option<(ListKey, std::rc::Rc<Vec<usize>>)>,
    gathering: Option<mpsc::Receiver<Catalog>>,
    envs: BTreeMap<u32, Option<dereth_assets::Environment>>,
    pub filter: String,
    pub shape: Option<Shape>,
    pub family: Option<usize>,
    /// Only the pieces retail joins the chosen one to.
    pub joins_chosen: bool,
    pub group: Group,
    /// The dungeons' names, and how many weenies they were read with.
    names: Option<(usize, BTreeMap<u16, String>)>,
    pub step: f32,
    pub chosen: Option<usize>,
    pub piece_rects: Vec<(usize, egui::Rect)>,
    /// Each group's header as drawn.
    pub header_rects: Vec<(GroupKey, egui::Rect)>,
    /// The pictures of the pieces in their dungeons.
    pub previews: super::preview::Previews,
    /// A piece the browser scrolls to when next drawn.
    pub reveal: Option<usize>,
}

impl std::fmt::Debug for Kit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit")
            .field("pieces", &self.pieces.as_ref().map(|p| p.len()))
            .field("chosen", &self.chosen)
            .finish_non_exhaustive()
    }
}

/// A free portal of the pack's cells (its new ones, and those of the landblock's own it
/// reshaped): the link a new piece's portal there is given, and its opening.
#[derive(Clone, Debug)]
pub struct Free {
    pub link: Link,
    pub opening: Opening,
}

/// What the browser shows: pieces whose label or environment holds `text`, or used by a landblock
/// it names or in a dungeon of `named` (those whose names hold it), of `shape`, drawn in look
/// `family`, among `joined`; each left out when it is empty or `None`.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub text: String,
    pub shape: Option<Shape>,
    pub family: Option<usize>,
    pub joined: Option<BTreeSet<usize>>,
    pub named: BTreeSet<u16>,
}

/// The pieces the browser shows for `filter`, in catalog order.
#[must_use]
pub fn shown(pieces: &[Piece], filter: &Filter) -> Vec<usize> {
    let f = filter.text.trim().to_ascii_uppercase();
    pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            f.is_empty()
                || p.label().contains(&f)
                || format!("{:08X}", p.environment.raw()).contains(&f)
                || u16::from_str_radix(&f, 16)
                    .ok()
                    .is_some_and(|lb| f.len() == 4 && p.landblocks.contains_key(&lb))
                || p.landblocks.keys().any(|lb| filter.named.contains(lb))
        })
        .filter(|(_, p)| filter.shape.is_none_or(|s| p.shape == s))
        .filter(|(_, p)| {
            filter
                .family
                .is_none_or(|f| p.looks.iter().any(|l| l.family == f))
        })
        .filter(|(i, _)| filter.joined.as_ref().is_none_or(|j| j.contains(i)))
        .map(|(i, _)| i)
        .collect()
}

/// What the browser's list was made with: its filters, its grouping and the chosen piece.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ListKey {
    text: String,
    shape: Option<Shape>,
    family: Option<usize>,
    joins: Option<usize>,
    group: Group,
    names: usize,
}

/// A row of the browser: a group's header, or pieces.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Header(GroupKey, usize),
    Pieces(Vec<usize>),
}

impl EditorApp {
    /// The pieces, gathered off the frame loop the first time they are wanted.
    pub(super) fn kit_pieces(&mut self) -> Option<&std::sync::Arc<Vec<Piece>>> {
        if self.kit.pieces.is_none() && self.kit.gathering.is_none() {
            let dir = self.core.cfg.dat_dir.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let catalog = lyceum_foundry::dats::DatSet::open(&dir)
                    .map(|set| lyceum_foundry::pieces::catalog(&set))
                    .unwrap_or_default();
                let _ = tx.send(catalog);
            });
            self.kit.gathering = Some(rx);
        }
        if let Some(c) = self
            .kit
            .gathering
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.kit.pieces = Some(std::sync::Arc::new(c.pieces));
            self.kit.families = c.families;
            self.kit.gathering = None;
        }
        self.kit.pieces.as_ref()
    }

    /// Where the pieces' pictures are kept.
    fn preview_dir(&self) -> std::path::PathBuf {
        crate::thumbs::folder(&self.thumbs_root, &self.base_store).join("pieces")
    }

    /// Piece `p`'s picture in a dungeon that uses it, fitted into `rect`, once it is drawn;
    /// false until then.
    fn draw_preview(&mut self, ui: &egui::Ui, rect: egui::Rect, p: &Piece) -> bool {
        let Some(lb) = p
            .landblocks
            .iter()
            .max_by_key(|(lb, n)| (**n, std::cmp::Reverse(**lb)))
            .map(|(lb, _)| *lb)
        else {
            return false;
        };
        let dir = self.preview_dir();
        let store = std::sync::Arc::clone(&self.base_store);
        let ctx = self.gui.ctx.clone();
        let Some(id) =
            self.kit
                .previews
                .get(&ctx, &store, &dir, (p.environment.raw(), p.piece), lb)
        else {
            return false;
        };
        #[allow(clippy::cast_precision_loss)]
        let aspect = super::preview::WIDTH as f32 / super::preview::HEIGHT as f32;
        let (w, h) = if rect.width() / rect.height() > aspect {
            (rect.height() * aspect, rect.height())
        } else {
            (rect.width(), rect.width() / aspect)
        };
        let r = egui::Rect::from_center_size(rect.center(), egui::vec2(w, h));
        ui.painter_at(rect).image(
            id,
            r,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        true
    }

    /// Wait for the pictures of the pieces the browser asked for, as a test does.
    pub fn wait_for_previews(&mut self) {
        let dir = self.preview_dir();
        let ctx = self.gui.ctx.clone();
        self.kit.previews.wait(&ctx, &dir);
    }

    /// How many pieces' pictures were drawn, and how many read from the cache.
    #[must_use]
    pub fn previews_made(&self) -> (u64, u64) {
        (self.kit.previews.drawn, self.kit.previews.read)
    }

    /// Select cell `n` of landblock `lb`, choose its piece in the browser, and fly into it.
    ///
    /// # Errors
    /// No such cell.
    pub fn choose_cell(&mut self, lb: u16, n: u16) -> Result<(), String> {
        self.selection = Some(super::Selection::Cell {
            landblock: lb,
            cell: n,
        });
        self.also.clear();
        self.choose_piece_of(lb, n);
        self.fly_into_cell(lb, n)
    }

    /// Choose, in the browser, the piece cell `n` of landblock `lb` is built of.
    pub(super) fn choose_piece_of(&mut self, lb: u16, n: u16) {
        let Some(c) = self.cells_of(lb).get(&n).cloned() else {
            return;
        };
        if let Some(i) = self.kit.pieces.as_ref().and_then(|ps| {
            ps.iter()
                .position(|p| p.environment == c.environment && p.piece == c.cell_struct)
        }) {
            self.kit.chosen = Some(i);
            self.kit.reveal = Some(i);
        }
    }

    /// Reshape cell `n` of landblock `lb` with the chosen piece: the piece turned and placed so
    /// that every way the cell has is met by one of its portals (kept), its other portals left
    /// free to build on; drawn as the cell's look draws it when the piece is drawn so, else as
    /// the browser's look or the piece's commonest.
    ///
    /// # Errors
    /// No piece chosen, no such cell, or no turn of the piece that meets every way the cell has.
    pub fn replace_cell(&mut self, lb: u16, n: u16) -> Result<(), String> {
        let i = self
            .kit
            .chosen
            .ok_or_else(|| self.say("messages.choose_piece"))?;
        let piece = self
            .kit
            .pieces
            .as_ref()
            .and_then(|ps| ps.get(i))
            .cloned()
            .ok_or_else(|| self.say("messages.pieces_gathering"))?;
        let old = self
            .session
            .cells(lb)
            .remove(&n)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        let old_env = self
            .environment(old.environment)
            .cloned()
            .ok_or_else(|| self.say("messages.environment_missing"))?;
        let env = self
            .environment(piece.environment)
            .cloned()
            .ok_or_else(|| self.say("messages.environment_missing"))?;
        let step = if self.kit.step > 0.0 {
            self.kit.step
        } else {
            90.0
        };
        let (frame, links) =
            lyceum_foundry::pieces::reshape_fit(&old, &old_env, &env, piece.piece, step)
                .ok_or_else(|| self.say("messages.no_reshape"))?;
        // Drawn as the dungeon's own look draws the piece, else as the browser's look, else as
        // the piece's commonest.
        let home = self
            .kit
            .families
            .iter()
            .position(|f| f.landblocks.iter().any(|(l, _)| *l == lb));
        let surfaces = [home, self.kit.family]
            .into_iter()
            .flatten()
            .find_map(|f| piece.looks.iter().find(|l| l.family == f))
            .map_or_else(|| piece.surfaces.clone(), |l| l.surfaces.clone());
        let q = frame.rotation;
        self.apply(
            &Op::ReplaceCell {
                landblock: lb,
                cell: n,
                environment: piece.environment,
                piece: piece.piece,
                surfaces,
                at: [frame.origin.x, frame.origin.y, frame.origin.z],
                rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
                links,
            },
            false,
        )?;
        self.dungeon_block = Some(lb);
        Ok(())
    }

    /// Choose piece `piece` of environment `env` in the browser, shown: the dungeon tool brought
    /// up, and the browser's filters cleared to show it.
    ///
    /// # Errors
    /// No such piece among those retail's cells are built from.
    pub fn find_piece(&mut self, env: dereth_primitives::DataId, piece: u16) -> Result<(), String> {
        self.wait_for_pieces();
        let i = self
            .kit
            .pieces
            .as_ref()
            .and_then(|ps| {
                ps.iter()
                    .position(|p| p.environment == env && p.piece == piece)
            })
            .ok_or_else(|| self.say("messages.no_such_piece"))?;
        self.kit.chosen = Some(i);
        self.kit.filter.clear();
        self.kit.reveal = Some(i);
        self.mode = super::Mode::Dungeon;
        Ok(())
    }

    /// Wait for the pieces, as a script or a test does.
    pub fn wait_for_pieces(&mut self) {
        while self.kit_pieces().is_none() {
            if let Some(c) = self.kit.gathering.take().and_then(|rx| rx.recv().ok()) {
                self.kit.pieces = Some(std::sync::Arc::new(c.pieces));
                self.kit.families = c.families;
            }
        }
    }

    fn environment(
        &mut self,
        id: dereth_primitives::DataId,
    ) -> Option<&dereth_assets::Environment> {
        let session = &self.session;
        self.kit
            .envs
            .entry(id.raw())
            .or_insert_with(|| session.environment(id))
            .as_ref()
    }

    /// The free portals of the pack's cells in landblock `lb`: each portal its source leaves
    /// shut, of its new cells and of the landblock's own it reshaped.
    #[must_use]
    pub fn free_portals(&mut self, lb: u16) -> Vec<Free> {
        let edits: Vec<lyceum_foundry::source::LandblockEdits> = self.session.packs()
            [self.session.target()]
        .edits
        .iter()
        .filter(|e| e.landblock == lb)
        .cloned()
        .collect();
        let adds: Vec<lyceum_foundry::source::AddCell> = edits
            .iter()
            .flat_map(|e| e.cells.iter().map(|c| c.item.clone()))
            .collect();
        let mut out = Vec::new();
        for r in edits
            .iter()
            .flat_map(|e| e.replaces.iter().map(|r| r.item.clone()))
        {
            let frame = Frame::new(
                Vec3::new(r.at[0], r.at[1], r.at[2]),
                lyceum_foundry::compile::quat_of(r.rotation),
            );
            let Some(env) = self.environment(r.environment).cloned() else {
                continue;
            };
            for (slot, l) in r.portals.iter().enumerate() {
                if *l != Link::Shut {
                    continue;
                }
                if let Some(o) = opening(&env, r.piece, slot, &frame) {
                    out.push(Free {
                        link: Link::Base {
                            cell: r.cell,
                            portal: slot,
                        },
                        opening: o,
                    });
                }
            }
        }
        for a in adds {
            let frame = Frame::new(
                Vec3::new(a.at[0], a.at[1], a.at[2]),
                lyceum_foundry::compile::quat_of(a.rotation),
            );
            let Some(env) = self.environment(a.environment).cloned() else {
                continue;
            };
            for (slot, l) in a.portals.iter().enumerate() {
                if *l != Link::Shut {
                    continue;
                }
                if let Some(o) = opening(&env, a.piece, slot, &frame) {
                    out.push(Free {
                        link: Link::Cell {
                            cell: a.name.clone(),
                            portal: slot,
                        },
                        opening: o,
                    });
                }
            }
        }
        out
    }

    /// What dungeon `lb` is called, when its portals name it.
    fn place_name(&mut self, lb: u16) -> Option<String> {
        if self
            .kit
            .names
            .as_ref()
            .is_none_or(|(n, _)| *n != self.weenies.len())
        {
            let names: BTreeMap<u16, String> = self
                .dungeons()
                .into_iter()
                .filter_map(|d| d.name.map(|n| (d.landblock, n)))
                .collect();
            self.kit.names = Some((self.weenies.len(), names));
        }
        self.kit
            .names
            .as_ref()
            .and_then(|(_, m)| m.get(&lb).cloned())
    }

    /// What a family of looks is called: the dungeons drawn in it that their portals name, most
    /// cells first, else its first landblock.
    pub fn family_name(&mut self, f: usize) -> String {
        let Some(fam) = self.kit.families.get(f).cloned() else {
            return String::new();
        };
        let named: Vec<String> = fam
            .landblocks
            .iter()
            .filter_map(|(lb, _)| self.place_name(*lb))
            .fold(Vec::new(), |mut v, n| {
                if !v.contains(&n) {
                    v.push(n);
                }
                v
            })
            .into_iter()
            .take(2)
            .collect();
        let t = self.text.table();
        let first = fam.landblocks.first().map_or(0, |l| l.0);
        match named.as_slice() {
            [] => t.fill("dungeon.look_of", &[("landblock", &format!("{first:04X}"))]),
            [a] => a.clone(),
            [a, b, ..] => t.fill("dungeon.look_two", &[("a", a), ("b", b)]),
        }
    }

    /// The pieces the browser shows, by its filter, in groups: made again only when the filters
    /// change.
    #[must_use]
    pub fn piece_list(&mut self) -> Vec<usize> {
        self.listed().as_ref().clone()
    }

    fn listed(&mut self) -> std::rc::Rc<Vec<usize>> {
        let key = ListKey {
            text: self.kit.filter.clone(),
            shape: self.kit.shape,
            family: self.kit.family,
            joins: self.kit.joins_chosen.then_some(self.kit.chosen).flatten(),
            group: self.kit.group,
            names: self.kit.names.as_ref().map_or(0, |(n, _)| *n),
        };
        if let Some((k, list)) = &self.kit.listed {
            if *k == key {
                return std::rc::Rc::clone(list);
            }
        }
        let list = std::rc::Rc::new(self.make_list());
        self.kit.listed = Some((key, std::rc::Rc::clone(&list)));
        list
    }

    fn make_list(&mut self) -> Vec<usize> {
        let Some(pieces) = self.kit.pieces.clone() else {
            return Vec::new();
        };
        let text = self.kit.filter.trim().to_lowercase();
        // The dungeons whose names hold the text: a piece used in one is shown.
        let named: BTreeSet<u16> = if text.is_empty() {
            BTreeSet::new()
        } else {
            let all: Vec<u16> = pieces
                .iter()
                .flat_map(|p| p.landblocks.keys().copied())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            all.into_iter()
                .filter(|lb| {
                    self.place_name(*lb)
                        .is_some_and(|n| n.to_lowercase().contains(&text))
                })
                .collect()
        };
        let joined: Option<BTreeSet<usize>> = self
            .kit
            .joins_chosen
            .then_some(self.kit.chosen)
            .flatten()
            .and_then(|c| pieces.get(c))
            .map(|p| p.joins.iter().map(|(i, _)| *i).collect());
        let filter = Filter {
            text: self.kit.filter.clone(),
            shape: self.kit.shape,
            family: self.kit.family,
            joined,
            named,
        };
        let mut list = shown(&pieces, &filter);
        match self.kit.group {
            Group::Look => list.sort_by_key(|i| (pieces[*i].looks.first().map(|l| l.family), *i)),
            Group::Shape => list.sort_by_key(|i| (pieces[*i].shape, *i)),
            Group::None => {}
        }
        list
    }

    /// The group piece `p` is shown under.
    fn group_of(&self, p: &Piece) -> Option<GroupKey> {
        match self.kit.group {
            Group::Look => p.looks.first().map(|l| GroupKey::Look(l.family)),
            Group::Shape => Some(GroupKey::Shape(p.shape)),
            Group::None => None,
        }
    }

    /// The browser's filters: a text, a shape, a look, the chosen piece's neighbors, and how
    /// it groups.
    fn piece_filters(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        ui.add(
            egui::TextEdit::singleline(&mut self.kit.filter)
                .hint_text(t.get("dungeon.filter"))
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.kit.shape.is_none(), t.get("dungeon.any_shape"))
                .clicked()
            {
                self.kit.shape = None;
            }
            for s in Shape::ALL {
                let on = self.kit.shape == Some(s);
                if ui
                    .selectable_label(on, t.get(&format!("dungeon.shape_{}", s.key())))
                    .clicked()
                {
                    self.kit.shape = if on { None } else { Some(s) };
                }
            }
        });
        let families = self.kit.families.len();
        let chosen_name = self.kit.family.map_or_else(
            || t.get("dungeon.any_look").to_owned(),
            |f| self.family_name(f),
        );
        let t = self.text.table();
        let mut pick: Option<Option<usize>> = None;
        egui::ComboBox::from_id_salt("piece look")
            .selected_text(chosen_name)
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.kit.family.is_none(), t.get("dungeon.any_look"))
                    .clicked()
                {
                    pick = Some(None);
                }
                // The families of several landblocks: the looks a dungeon builder works in.
                for f in 0..families {
                    if self.kit.families[f].landblocks.len() < 2 {
                        continue;
                    }
                    let name = self.family_name(f);
                    let label = self.text.table().fill(
                        "dungeon.look_row",
                        &[
                            ("name", &name),
                            ("landblocks", &self.kit.families[f].landblocks.len()),
                            ("pieces", &self.kit.families[f].pieces),
                        ],
                    );
                    if ui
                        .selectable_label(self.kit.family == Some(f), label)
                        .clicked()
                    {
                        pick = Some(Some(f));
                    }
                }
            });
        if let Some(p) = pick {
            self.kit.family = p;
        }
        let t = self.text.table();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.kit.joins_chosen, t.get("dungeon.joins_chosen"))
                .on_hover_text(t.get("dungeon.joins_chosen_tip"));
            ui.label(t.get("dungeon.group_by"));
            for (g, key) in [
                (Group::Look, "dungeon.group_look"),
                (Group::Shape, "dungeon.group_shape"),
                (Group::None, "dungeon.group_none"),
            ] {
                if ui
                    .selectable_label(self.kit.group == g, t.get(key))
                    .clicked()
                {
                    self.kit.group = g;
                }
            }
        });
    }

    /// The chosen piece: its picture, its plan, what it is, where retail uses it, and the pieces
    /// retail joins it to, each to choose.
    fn chosen_card(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some(i) = self.kit.chosen else { return };
        let Some(p) = self.kit.pieces.as_ref().and_then(|ps| ps.get(i)).cloned() else {
            return;
        };
        let t = self.text.table();
        ui.horizontal(|ui| {
            let side = 112.0;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(side * 4.0 / 3.0, side), egui::Sense::hover());
            if !self.draw_preview(ui, rect, &p) {
                ui.painter_at(rect).text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    self.text.table().get("dungeon.drawing"),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(160),
                );
            }
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            self.draw_plan(ui, rect, &p);
        });
        ui.strong(t.fill(
            "dungeon.chosen_title",
            &[
                ("piece", &p.label()),
                ("shape", &t.get(&format!("dungeon.shape_{}", p.shape.key()))),
            ],
        ));
        ui.label(t.fill(
            "dungeon.chosen_size",
            &[
                ("portals", &p.portals),
                (
                    "size",
                    &format!("{:.1} × {:.1} × {:.1}", p.size[0], p.size[1], p.size[2]),
                ),
                ("cells", &p.cells),
            ],
        ));
        // The looks it is drawn in: a click shows that look's pieces.
        let looks: Vec<(usize, usize)> = p
            .looks
            .iter()
            .take(4)
            .map(|l| (l.family, l.cells))
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label(self.text.table().get("dungeon.drawn_in"));
            for (f, cells) in looks {
                let name = self.family_name(f);
                if ui
                    .small_button(format!("{name} ({cells})"))
                    .on_hover_text(self.text.table().get("dungeon.look_tip"))
                    .clicked()
                {
                    self.kit.family = Some(f);
                }
            }
        });
        // Where it is used: a click goes into that dungeon.
        let mut used: Vec<(u16, usize)> = p.landblocks.iter().map(|(l, n)| (*l, *n)).collect();
        used.sort_by_key(|(l, n)| (std::cmp::Reverse(*n), *l));
        ui.horizontal_wrapped(|ui| {
            ui.label(self.text.table().get("dungeon.used_in"));
            for (lb, n) in used.into_iter().take(6) {
                let name = self.place_name(lb).unwrap_or_else(|| format!("{lb:04X}"));
                if ui
                    .small_button(format!("{name} ({n})"))
                    .on_hover_text(self.text.table().get("dungeon.used_in_tip"))
                    .clicked()
                {
                    out.push(UiAction::EnterDungeon(lb, true));
                }
            }
        });
        // The pieces retail joins it to.
        let joins: Vec<(usize, usize)> = p.joins.iter().take(6).copied().collect();
        if !joins.is_empty() {
            ui.label(self.text.table().get("dungeon.usually_joins"));
            let pieces = self.kit.pieces.clone().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for (j, n) in joins {
                    let Some(q) = pieces.get(j) else { continue };
                    let (rect, r) =
                        ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::click());
                    self.draw_plan(ui, rect, q);
                    if r.on_hover_text(format!("{} ({n})", q.label())).clicked() {
                        out.push(UiAction::Piece(j));
                        self.kit.reveal = Some(j);
                    }
                }
            });
        }
    }

    /// The landblock's own cells, the world's and the packs', with their pieces: a click selects
    /// one and chooses its piece.
    fn dungeon_cells(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let Some((lb, ..)) = self.work_block() else {
            return;
        };
        let cells = self.cells_of(lb);
        if cells.is_empty() {
            return;
        }
        let here = self.camera_cell().map(|(c, _)| c.0);
        let names = self.session.cell_names(lb);
        let t = self.text.table();
        let pieces = self.kit.pieces.clone().unwrap_or_default();
        egui::CollapsingHeader::new(t.fill("dungeon.its_cells", &[("n", &cells.len())]))
            .id_salt("dungeon cells")
            .default_open(self.in_dungeon)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("dungeon cells list")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for (n, c) in cells.iter() {
                            let i = pieces.iter().position(|p| {
                                p.environment == c.environment && p.piece == c.cell_struct
                            });
                            let shape = i.map_or_else(String::new, |i| {
                                t.get(&format!("dungeon.shape_{}", pieces[i].shape.key()))
                                    .to_owned()
                            });
                            let label = format!(
                                "{n:04X}  {:04X}/{}  {shape}{}",
                                c.environment.raw() & 0xFFFF,
                                c.cell_struct,
                                names
                                    .get(n)
                                    .map(|s| format!("  {}", s.rsplit(':').next().unwrap_or(s)))
                                    .unwrap_or_default()
                            );
                            let id = (u32::from(lb) << 16) | u32::from(*n);
                            let selected = self.selection
                                == Some(super::Selection::Cell {
                                    landblock: lb,
                                    cell: *n,
                                });
                            let text = if here == Some(id) {
                                egui::RichText::new(label).strong()
                            } else {
                                egui::RichText::new(label)
                            };
                            if ui.selectable_label(selected, text).clicked() {
                                out.push(UiAction::ChooseCell(lb, *n));
                            }
                        }
                    });
            });
    }

    /// A click in the world with a piece chosen: the piece placed so that its portal meets the
    /// free portal nearest the pointer's ray, or ahead of the camera when there is none.
    pub(super) fn piece_at(&mut self, p: (f32, f32)) -> Result<(), String> {
        self.kit
            .chosen
            .ok_or_else(|| self.say("messages.choose_piece"))?;
        let (lb, ..) = self
            .work_block()
            .ok_or_else(|| self.say("messages.off_world"))?;
        let base = lyceum_foundry::pick::block_origin(lb);
        let ray = self.view().ok_or("no view")?.ray(p.0, p.1);
        // The free portal nearest the ray.
        let near = |o: &Opening| -> f32 {
            let m = o.middle.add(base);
            let d = m.sub(ray.origin);
            let t = d.dot(ray.dir).max(0.0);
            let q = ray
                .origin
                .add(Vec3::new(ray.dir.x * t, ray.dir.y * t, ray.dir.z * t));
            let e = m.sub(q);
            e.dot(e).sqrt()
        };
        let target = self
            .free_portals(lb)
            .into_iter()
            .map(|f| (near(&f.opening), f))
            .filter(|(d, _)| *d <= REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, f)| f);
        let at = ray
            .origin
            .add(Vec3::new(
                ray.dir.x * AHEAD,
                ray.dir.y * AHEAD,
                ray.dir.z * AHEAD,
            ))
            .sub(base);
        let snap_half = |v: f32| (v * 2.0).round() / 2.0;
        self.place_piece(
            lb,
            target.as_ref(),
            Vec3::new(snap_half(at.x), snap_half(at.y), snap_half(at.z)),
        )
    }

    /// The chosen piece placed `times` times, each at the first free portal of the pack's cells
    /// in the camera's landblock (the first of all ahead of the camera), as a script grows a
    /// dungeon.
    ///
    /// # Errors
    /// No piece chosen, no free portal, or a placement refused.
    pub fn extend(&mut self, times: u32) -> Result<(), String> {
        for _ in 0..times {
            let (lb, ..) = self
                .work_block()
                .ok_or_else(|| self.say("messages.off_world"))?;
            if self.session.cell_names(lb).is_empty() {
                let vp = self.view().ok_or("no view")?.viewport();
                #[allow(clippy::cast_precision_loss)]
                let middle = (vp.width as f32 * 0.5, vp.height as f32 * 0.5);
                self.piece_at(middle)?;
                continue;
            }
            let free = self
                .free_portals(lb)
                .into_iter()
                .next()
                .ok_or_else(|| self.say("messages.no_free_portal"))?;
            self.place_piece(lb, Some(&free), Vec3::ZERO)?;
        }
        Ok(())
    }

    /// Put the camera in the middle of the pack's cell `name` (its symbol, bare or in full) of the
    /// camera's landblock.
    ///
    /// # Errors
    /// No such cell.
    pub fn go_inside(&mut self, name: &str) -> Result<(), String> {
        let (lb, ..) = self
            .work_block()
            .ok_or_else(|| self.say("messages.off_world"))?;
        self.go_inside_at(lb, name)
    }

    /// Put the camera in the middle of the pack's cell `name` of landblock `lb`.
    ///
    /// # Errors
    /// No such cell.
    pub fn go_inside_at(&mut self, lb: u16, name: &str) -> Result<(), String> {
        let (c, _) = self
            .session
            .cell_names(lb)
            .into_iter()
            .find(|(_, n)| n == name || n.rsplit(':').next() == Some(name))
            .ok_or_else(|| {
                self.say_with(
                    "messages.no_cell",
                    &[("name", &name), ("landblock", &format!("{lb:04X}"))],
                )
            })?;
        self.fly_into_cell(lb, c)
    }

    /// Put the camera in the middle of cell `c` of landblock `lb`, the world's or a pack's.
    ///
    /// # Errors
    /// No such cell, or its environment is not in the files.
    pub fn fly_into_cell(&mut self, lb: u16, c: u16) -> Result<(), String> {
        let cell = self
            .session
            .cells(lb)
            .remove(&c)
            .ok_or_else(|| self.say("messages.cell_not_compiled"))?;
        let cs = self
            .environment(cell.environment)
            .and_then(|e| e.cells.get(usize::from(cell.cell_struct)))
            .cloned()
            .ok_or_else(|| self.say("messages.environment_missing"))?;
        let vs = &cs.vertex_array.vertices;
        #[allow(clippy::cast_precision_loss)]
        let n = vs.len().max(1) as f32;
        let mid = vs.iter().fold(Vec3::ZERO, |m, v| {
            let p = localtoglobal(&cell.frame, v.position);
            Vec3::new(m.x + p.x / n, m.y + p.y / n, m.z + p.z / n)
        });
        self.camera.position = mid.add(lyceum_foundry::pick::block_origin(lb));
        self.dungeon_block = Some(lb);
        self.rooms = None;
        Ok(())
    }

    /// Place the chosen piece in landblock `lb`: joined to `target`, or with its origin at `at`
    /// (landblock meters) when there is none.
    fn place_piece(&mut self, lb: u16, target: Option<&Free>, at: Vec3) -> Result<(), String> {
        let i = self
            .kit
            .chosen
            .ok_or_else(|| self.say("messages.choose_piece"))?;
        let piece = self
            .kit
            .pieces
            .as_ref()
            .and_then(|ps| ps.get(i))
            .cloned()
            .ok_or_else(|| self.say("messages.pieces_gathering"))?;
        let env = self
            .environment(piece.environment)
            .cloned()
            .ok_or_else(|| self.say("messages.environment_missing"))?;
        let n = env
            .cells
            .get(usize::from(piece.piece))
            .map_or(0, |c| c.portals.len());
        let step = if self.kit.step > 0.0 {
            self.kit.step
        } else {
            90.0
        };
        let (frame, links) = if let Some(t) = target {
            // Each of the piece's portals tried: the one that fits best, at the least turn.
            let best = (0..n)
                .filter_map(|slot| {
                    snap(&env, piece.piece, slot, &t.opening, 0.0, step).map(|(f, m)| (slot, f, m))
                })
                .min_by(|a, b| a.2.total_cmp(&b.2))
                .ok_or_else(|| self.say("messages.no_facing_portal"))?;
            let mut links = vec![Link::Shut; n];
            links[best.0] = t.link.clone();
            (best.1, links)
        } else {
            (Frame::new(at, Quat::IDENTITY), vec![Link::Shut; n])
        };
        let q = frame.rotation;
        // Drawn as retail draws it in the look the browser shows, else in the look of the
        // dungeon it joins, else in its commonest.
        let home = self
            .kit
            .families
            .iter()
            .position(|f| f.landblocks.iter().any(|(l, _)| *l == lb));
        let surfaces = [self.kit.family, home]
            .into_iter()
            .flatten()
            .find_map(|f| piece.looks.iter().find(|l| l.family == f))
            .map_or_else(|| piece.surfaces.clone(), |l| l.surfaces.clone());
        self.apply(
            &Op::PlaceCell {
                landblock: lb,
                environment: piece.environment,
                piece: piece.piece,
                surfaces,
                at: [frame.origin.x, frame.origin.y, frame.origin.z],
                rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
                links,
            },
            false,
        )?;
        Ok(())
    }

    /// Cell `cell` of landblock `lb` outlined on the world, every edge of its polygons.
    pub(super) fn outline_cell(
        &mut self,
        painter: &egui::Painter,
        view: &View,
        lb: u16,
        cell: u16,
        width: f32,
        color: egui::Color32,
    ) {
        let Some(c) = self.cells_of(lb).get(&cell).cloned() else {
            return;
        };
        let Some(cs) = self
            .environment(c.environment)
            .and_then(|e| e.cells.get(usize::from(c.cell_struct)))
            .cloned()
        else {
            return;
        };
        let base = lyceum_foundry::pick::block_origin(lb);
        let at = |i: &u16| {
            cs.vertex_array
                .vertices
                .get(usize::from(*i))
                .map(|v| localtoglobal(&c.frame, v.position).add(base))
        };
        for poly in &cs.polygons {
            let pts: Vec<Vec3> = poly.vertex_ids.iter().filter_map(at).collect();
            for (k, p) in pts.iter().enumerate() {
                if let (Some(a), Some(b)) =
                    (view.project(*p), view.project(pts[(k + 1) % pts.len()]))
                {
                    painter.line_segment(
                        [egui::pos2(a.0, a.1), egui::pos2(b.0, b.1)],
                        egui::Stroke::new(width, color),
                    );
                }
            }
        }
    }

    /// The pack's cells of the camera's landblock outlined on the world, their free portals in
    /// the selection's color: the client draws a cell only from inside, so from the flying
    /// camera this is how a dungeon being built is seen.
    pub(super) fn dungeon_marks(&mut self, painter: &egui::Painter, view: &View) {
        let Some((lb, ..)) = self.work_block() else {
            return;
        };
        let names = self.session.cell_names(lb);
        if names.is_empty() {
            return;
        }
        let base = lyceum_foundry::pick::block_origin(lb);
        let line = |a: Vec3, b: Vec3, w: f32, c: egui::Color32| {
            if let (Some(p), Some(q)) = (view.project(a.add(base)), view.project(b.add(base))) {
                painter.line_segment(
                    [egui::pos2(p.0, p.1), egui::pos2(q.0, q.1)],
                    egui::Stroke::new(w, c),
                );
            }
        };
        let cells = self.cells_of(lb);
        for c in names.keys() {
            let Some(cell) = cells.get(c) else { continue };
            let Some(cs) = self
                .environment(cell.environment)
                .and_then(|e| e.cells.get(usize::from(cell.cell_struct)))
                .cloned()
            else {
                continue;
            };
            let at = |i: &u16| {
                cs.vertex_array
                    .vertices
                    .get(usize::from(*i))
                    .map(|v| localtoglobal(&cell.frame, v.position))
            };
            for poly in &cs.polygons {
                let pts: Vec<Vec3> = poly.vertex_ids.iter().filter_map(at).collect();
                for (k, p) in pts.iter().enumerate() {
                    line(
                        *p,
                        pts[(k + 1) % pts.len()],
                        1.0,
                        super::panels::mark::OUTLINE,
                    );
                }
            }
        }
        for f in self.free_portals(lb) {
            let pts = &f.opening.corners;
            for (k, p) in pts.iter().enumerate() {
                line(
                    *p,
                    pts[(k + 1) % pts.len()],
                    2.5,
                    super::panels::mark::SELECT,
                );
            }
        }
    }

    /// The dungeon tab: above, the dungeons, the turn step, the cells here, the browser's
    /// filters and the chosen piece, in at most a little over half the tab; below, the pieces.
    pub(super) fn dungeon_tab(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let top = (ui.available_height() * 0.55).max(120.0);
        egui::ScrollArea::vertical()
            .id_salt("dungeon tool")
            .max_height(top)
            .auto_shrink([false, true])
            .show(ui, |ui| self.dungeon_top(ui, out));
        ui.separator();
        if self.kit.pieces.is_none() {
            return;
        }
        self.piece_grid(ui, out);
    }

    /// The dungeon tab's upper part.
    fn dungeon_top(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        self.dungeons_section(ui, out);
        let t = self.text.table();
        ui.label(t.get("dungeon.help"));
        ui.horizontal(|ui| {
            ui.label(t.get("dungeon.turn_steps"));
            for s in [90.0f32, 45.0] {
                let on = (self.kit.step - s).abs() < 0.1 || (self.kit.step == 0.0 && s == 90.0);
                if ui.selectable_label(on, format!("{s:.0}°")).clicked() {
                    out.push(UiAction::Step(s));
                }
            }
        });
        if let Some((lb, ..)) = self.work_block() {
            let names = self.session.cell_names(lb);
            if !names.is_empty() {
                egui::CollapsingHeader::new(t.fill("dungeon.cells_here", &[("n", &names.len())]))
                    .id_salt("cells here")
                    .default_open(false)
                    .show(ui, |ui| {
                        for (c, name) in &names {
                            ui.horizontal(|ui| {
                                ui.monospace(format!("{c:04X}"));
                                ui.label(name.rsplit(':').next().unwrap_or(name));
                                if ui
                                    .small_button("x")
                                    .on_hover_text(t.get("dungeon.take_away"))
                                    .clicked()
                                {
                                    out.push(UiAction::RemoveCell(lb, name.clone()));
                                }
                            });
                        }
                    });
            }
        }
        self.dungeon_cells(ui, out);
        ui.separator();
        if self.kit_pieces().is_none() {
            ui.weak(self.text.table().get("dungeon.gathering"));
            return;
        }
        self.piece_filters(ui);
        self.chosen_card(ui, out);
    }

    /// The pieces the filters show, in their groups.
    fn piece_grid(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let list = self.listed();
        let pieces = self.kit.pieces.clone().unwrap_or_default();
        ui.label(self.text.table().fill(
            "dungeon.shown",
            &[("n", &list.len()), ("all", &pieces.len())],
        ));
        let cell = egui::vec2(96.0, 118.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = ((ui.available_width() / (cell.x + 4.0)).floor().max(1.0) as usize).max(1);
        // The rows: each group's header, then its pieces.
        let mut rows: Vec<Row> = Vec::new();
        let mut current: Option<GroupKey> = None;
        for i in list.iter() {
            let g = self.group_of(&pieces[*i]);
            if g != current {
                if let Some(k) = g {
                    rows.push(Row::Header(k, 0));
                }
                current = g;
            }
            if let Some(Row::Header(_, n)) =
                rows.iter_mut().rev().find(|r| matches!(r, Row::Header(..)))
            {
                *n += 1;
            }
            match rows.last_mut() {
                Some(Row::Pieces(v)) if v.len() < columns => v.push(*i),
                _ => rows.push(Row::Pieces(vec![*i])),
            }
        }
        let reveal = self.kit.reveal.take().and_then(|r| {
            rows.iter()
                .position(|row| matches!(row, Row::Pieces(v) if v.contains(&r)))
        });
        self.kit.piece_rects.clear();
        self.kit.header_rects.clear();
        let t = self.text.table();
        let mut area = egui::ScrollArea::vertical()
            .id_salt("pieces")
            .auto_shrink(false);
        if let Some(r) = reveal {
            #[allow(clippy::cast_precision_loss)]
            let y = r as f32 * (cell.y + 4.0 + ui.spacing().item_spacing.y);
            area = area.vertical_scroll_offset(y);
        }
        area.show_rows(ui, cell.y + 4.0, rows.len(), |ui, shown_rows| {
            for row in shown_rows {
                match rows[row].clone() {
                    Row::Header(k, n) => {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), cell.y + 4.0),
                            egui::Sense::hover(),
                        );
                        self.draw_header(ui, rect, k, n);
                        self.kit.header_rects.push((k, rect));
                    }
                    Row::Pieces(v) => {
                        ui.horizontal(|ui| {
                            for i in v {
                                let p = pieces[i].clone();
                                let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
                                let chosen = self.kit.chosen == Some(i);
                                self.draw_piece(ui, rect, &p, chosen || r.hovered(), chosen);
                                self.kit.piece_rects.push((i, rect));
                                let shape = t
                                    .get(&format!("dungeon.shape_{}", p.shape.key()))
                                    .to_owned();
                                if r.on_hover_text(t.fill(
                                    "dungeon.piece_tip",
                                    &[
                                        ("piece", &p.label()),
                                        ("shape", &shape),
                                        ("portals", &p.portals),
                                        (
                                            "size",
                                            &format!(
                                                "{:.1} × {:.1} × {:.1}",
                                                p.size[0], p.size[1], p.size[2]
                                            ),
                                        ),
                                        ("cells", &p.cells),
                                        ("landblocks", &p.landblocks.len()),
                                    ],
                                ))
                                .clicked()
                                {
                                    out.push(UiAction::Piece(i));
                                }
                            }
                        });
                    }
                }
            }
        });
    }

    /// A group's header: a look's name, its dungeons and its textures, or a shape's name.
    fn draw_header(&mut self, ui: &egui::Ui, rect: egui::Rect, k: GroupKey, n: usize) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect.shrink(2.0), 3.0, egui::Color32::from_gray(32));
        let t = self.text.table();
        let (title, more) = match k {
            GroupKey::Look(f) => {
                let name = self.family_name(f);
                let fam = self.kit.families.get(f).cloned();
                let more = fam.as_ref().map_or_else(String::new, |fam| {
                    self.text.table().fill(
                        "dungeon.look_header",
                        &[("landblocks", &fam.landblocks.len()), ("n", &n)],
                    )
                });
                (name, more)
            }
            GroupKey::Shape(s) => (
                t.get(&format!("dungeon.shape_{}", s.key())).to_owned(),
                t.fill("dungeon.shape_header", &[("n", &n)]),
            ),
        };
        let at = rect.min + egui::vec2(8.0, 6.0);
        painter.text(
            at,
            egui::Align2::LEFT_TOP,
            title,
            egui::FontId::proportional(14.0),
            egui::Color32::from_gray(230),
        );
        painter.text(
            at + egui::vec2(0.0, 20.0),
            egui::Align2::LEFT_TOP,
            more,
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(170),
        );
        // A look's textures, as swatches.
        if let GroupKey::Look(f) = k {
            let textures = self
                .kit
                .families
                .get(f)
                .map(|fam| fam.textures.clone())
                .unwrap_or_default();
            let mut x = rect.min.x + 8.0;
            for tex in textures.into_iter().take(6) {
                if let Some(id) = self.texture_picture(tex) {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(x, rect.min.y + 44.0),
                        egui::vec2(56.0, 56.0),
                    );
                    painter.image(
                        id,
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    x += 60.0;
                }
            }
        }
    }

    /// One piece in the browser: its picture in a retail dungeon (its floor plan until the
    /// picture is drawn), its label and its portal count.
    fn draw_piece(&mut self, ui: &egui::Ui, rect: egui::Rect, p: &Piece, lit: bool, chosen: bool) {
        let painter = ui.painter_at(rect);
        if lit {
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
        let plan = egui::Rect::from_min_size(
            rect.min + egui::vec2(8.0, 4.0),
            egui::vec2(rect.width() - 16.0, rect.width() - 16.0),
        );
        if !self.draw_preview(ui, plan, p) {
            self.draw_plan(ui, plan, p);
        }
        let text = self.text.table().fill(
            "dungeon.piece_caption",
            &[("piece", &p.label()), ("portals", &p.portals)],
        );
        let galley = painter.layout(
            text,
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(220),
            rect.width() - 4.0,
        );
        painter.galley(
            egui::pos2(rect.center().x - galley.size().x / 2.0, plan.max.y + 2.0),
            galley,
            egui::Color32::from_gray(220),
        );
    }

    /// A piece's floor plan from above in `plan`: floors filled, walls drawn, portals in the
    /// selection's color.
    fn draw_plan(&mut self, ui: &egui::Ui, plan: egui::Rect, p: &Piece) {
        let painter = ui.painter_at(plan);
        painter.rect_filled(plan, 2.0, egui::Color32::from_gray(20));
        if let Some(cs) = self
            .environment(p.environment)
            .and_then(|e| e.cells.get(usize::from(p.piece)))
            .cloned()
        {
            let vs = &cs.vertex_array.vertices;
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for v in vs {
                lo = [lo[0].min(v.position.x), lo[1].min(v.position.y)];
                hi = [hi[0].max(v.position.x), hi[1].max(v.position.y)];
            }
            let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(0.1);
            let k = (plan.width() - 8.0) / span;
            let at = |x: f32, y: f32| {
                egui::pos2(
                    plan.center().x + (x - (lo[0] + hi[0]) / 2.0) * k,
                    plan.center().y - (y - (lo[1] + hi[1]) / 2.0) * k,
                )
            };
            let portals: std::collections::BTreeSet<u16> = cs.portals.iter().copied().collect();
            // Floors filled first, then walls and openings drawn over them.
            let mut lines = Vec::new();
            for (pi, poly) in cs.polygons.iter().enumerate() {
                let corners: Vec<Vec3> = poly
                    .vertex_ids
                    .iter()
                    .filter_map(|i| vs.get(usize::from(*i)).map(|v| v.position))
                    .collect();
                if corners.len() < 2 {
                    continue;
                }
                let pts: Vec<egui::Pos2> = corners.iter().map(|v| at(v.x, v.y)).collect();
                let is_portal = u16::try_from(pi).is_ok_and(|i| portals.contains(&i));
                let up = corners.len() >= 3 && {
                    let (a, b, c) = (corners[0], corners[1], corners[2]);
                    let n = b.sub(a).cross(c.sub(a));
                    n.z * n.z > 0.5 * n.dot(n)
                };
                if up && !is_portal {
                    painter.add(egui::Shape::convex_polygon(
                        pts.clone(),
                        egui::Color32::from_gray(60),
                        egui::Stroke::NONE,
                    ));
                }
                let stroke = if is_portal {
                    egui::Stroke::new(2.0, super::panels::mark::SELECT)
                } else {
                    egui::Stroke::new(1.0, egui::Color32::from_gray(150))
                };
                let mut closed = pts;
                closed.push(closed[0]);
                lines.push(egui::Shape::line(closed, stroke));
            }
            painter.extend(lines);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's piece browser)
    use super::*;
    use dereth_primitives::DataId;

    fn piece(env: u32, n: u16, lbs: &[u16], shape: Shape, family: usize) -> Piece {
        Piece {
            environment: DataId(env),
            piece: n,
            portals: 2,
            size: [10.0, 10.0, 6.0],
            surfaces: Vec::new(),
            cells: lbs.len(),
            landblocks: lbs.iter().map(|l| (*l, 1)).collect(),
            shape,
            looks: vec![lyceum_foundry::pieces::Look {
                family,
                cells: lbs.len(),
                surfaces: Vec::new(),
            }],
            joins: Vec::new(),
        }
    }

    fn text(t: &str) -> Filter {
        Filter {
            text: t.into(),
            ..Filter::default()
        }
    }

    #[test]
    fn the_browser_finds_a_piece_by_its_environment_its_label_or_a_landblock_that_uses_it() {
        let ps = [
            piece(0x0D00_0094, 0, &[0x0111], Shape::Room, 0),
            piece(0x0D00_0196, 2, &[0x0001], Shape::Corridor, 1),
        ];
        assert_eq!(shown(&ps, &text("")), [0, 1]);
        assert_eq!(shown(&ps, &text("0094")), [0]);
        assert_eq!(shown(&ps, &text("0196/2")), [1]);
        assert_eq!(shown(&ps, &text("0d000196")), [1]);
        assert_eq!(shown(&ps, &text("0111")), [0]);
        assert!(shown(&ps, &text("0333")).is_empty());
    }

    #[test]
    fn the_browser_filters_by_shape_look_the_chosen_pieces_neighbors_and_a_dungeons_name() {
        let ps = [
            piece(0x0D00_0094, 0, &[0x0111], Shape::Room, 0),
            piece(0x0D00_0196, 2, &[0x0001], Shape::Corridor, 1),
            piece(0x0D00_0196, 3, &[0x0001, 0x0111], Shape::Corridor, 0),
        ];
        let f = |shape, family, joined: Option<&[usize]>| Filter {
            shape,
            family,
            joined: joined.map(|j| j.iter().copied().collect()),
            ..Filter::default()
        };
        assert_eq!(shown(&ps, &f(Some(Shape::Corridor), None, None)), [1, 2]);
        assert_eq!(shown(&ps, &f(None, Some(0), None)), [0, 2]);
        assert_eq!(shown(&ps, &f(Some(Shape::Corridor), Some(0), None)), [2]);
        assert_eq!(shown(&ps, &f(None, None, Some(&[0, 1]))), [0, 1]);
        // "hideout" names landblock 0x0111.
        let named = Filter {
            text: "hideout".into(),
            named: [0x0111].into(),
            ..Filter::default()
        };
        assert_eq!(shown(&ps, &named), [0, 2]);
    }
}
