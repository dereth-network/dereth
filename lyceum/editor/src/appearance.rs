//! The weenie editor's look at its weenie: the weenie drawn as the game shows it, turning in the
//! editor's head, and the Appearance page's pickers, which choose its clothing look, its color
//! scheme and its shade by sight.
//!
//! The preview is a preview space of the editor's own ([`SPACE`]), drawn into the head through the
//! overlay where egui lays it out ([`crate::gui::PreviewPaint`]). Its object is the weenie's setup
//! dressed in the object description a server sends for it ([`crate::look`]), applied to its parts
//! as the client applies a server's, at the weenie's size. It is built again whenever the look
//! changes: an edit, a shade being dragged, a wielded piece read from the server's database. It
//! turns by itself until it is dragged; the wheel brings it nearer, and a double-click frames it
//! again.
//!
//! The Clothing look picker lists the clothing tables that dress the weenie's setup, each drawn on
//! it and named by the weenies dressed in it; a clothing look is shown by that name and its id.
//! The Color scheme picker lists the clothing look's schemes, each by the server's name with the
//! colors it gives at the weenie's shade. The shade is a slider the preview follows as it is
//! dragged, written when it is let go. A creature's create list chooses what it wears the same
//! way, row by row: each row's color scheme from those of what it makes, and its shade on a slider.

use std::collections::{BTreeMap, HashMap};
use std::sync::{mpsc, Arc};

use dereth_animation::parts::ObjDesc;
use dereth_client_contract::overlay::PreviewSpace;
use dereth_client_shell::gpu::Renderer;
use dereth_primitives::{DataId, Vec3};
use dereth_rules::appearance::{self as rules, LookFiles, Properties};
use lyceum_foundry::content::{PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;

use super::form::FormEdit;
use super::{EditorApp, UiAction};
use crate::gui::PreviewPaint;
use crate::look::{self, ClothingIndex, DatFiles, SqlProps};

/// The editor's own preview space.
pub const SPACE: PreviewSpace = PreviewSpace::Tool;

/// The preview's side in the editor's head, in points.
pub const SIDE: f32 = 120.0;

/// How fast it turns by itself, in degrees a second, and how far a drag of one point turns it.
const TURN_RATE: f32 = 30.0;
const DRAG_TURN: f32 = 0.8;

/// How much nearer one notch of the wheel brings it, and how near and how far it goes.
const ZOOM_STEP: f32 = 1.15;
const ZOOM: (f32, f32) = (0.5, 4.0);

/// The pickers' popouts: their width, a clothing look's cell and its picture, a button's size.
const POPOUT_WIDTH: f32 = 430.0;
const CLOTHING_CELL: egui::Vec2 = egui::vec2(78.0, 76.0);
const BUTTON: egui::Vec2 = egui::vec2(190.0, 38.0);
const GRID_HEIGHT: f32 = 330.0;

/// A swatch's side, and the width of a scheme's name in its picker, in points.
const SWATCH: f32 = 12.0;
const SCHEME_NAME: f32 = 170.0;

/// A create list row's color scheme dropdown and shade slider, in points wide.
const ROW_SCHEME: f32 = 110.0;
const ROW_SHADE: f32 = 70.0;

/// What the preview draws: a setup dressed in a description, at a size.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub setup: DataId,
    pub objdesc: ObjDesc,
    pub scale: f32,
}

/// Which picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookPicker {
    Clothing,
    Scheme,
}

/// The clothing tables of the files, read off the frame.
#[derive(Default)]
enum Index {
    #[default]
    Unread,
    Reading(mpsc::Receiver<ClothingIndex>),
    Ready(Arc<ClothingIndex>),
}

/// What a weenie's look was worked out from last, and what it gave.
type Memo = (
    (
        Weenie,
        String,
        (usize, usize),
        u64,
        Option<u64>,
        Option<(usize, u64)>,
        usize,
    ),
    Option<Drawn>,
);

/// The preview's and the pickers' state.
pub struct Appearance {
    /// The look asked of the preview this frame, and the one in its space (with the files'
    /// generation it was built from, and where it is framed: the point looked at and the reach).
    pub want: Option<Drawn>,
    built: Option<(Drawn, u64, Vec3, f32)>,
    /// A look whose object would not build, not tried again until the look changes.
    failed: Option<(Drawn, u64)>,
    /// Its turn in degrees, whether it turns by itself, and how much nearer than framed it is.
    pub heading: f32,
    pub turning: bool,
    pub zoom: f32,
    /// The shade being dragged on its slider, before it is let go: the preview follows it.
    pub shade_held: Option<(Weenie, f64)>,
    /// A create list row's shade being dragged the same way: the weenie, the row's place in its
    /// create list, and the shade.
    pub row_shade_held: Option<(Weenie, usize, f64)>,
    /// Where the preview was drawn last.
    pub rect: Option<egui::Rect>,
    /// How many times the preview's object was built.
    pub builds: u64,
    memo: Option<Memo>,
    files: Option<(u64, DatFiles)>,
    index: Index,
    names: Option<(usize, BTreeMap<u32, Vec<String>>)>,
    pictures: HashMap<u64, egui::TextureHandle>,
    last_time: Option<f64>,
    /// The picker open, what is typed in its search, and whether it was just opened.
    pub open: Option<LookPicker>,
    pub search: String,
    just_opened: bool,
    /// As drawn last: each picker's button, the shade's slider, and each cell of the open
    /// popout by what it writes (a clothing table's id, a scheme's number; 0 for none).
    pub buttons: Vec<(LookPicker, egui::Rect)>,
    pub slider: Option<egui::Rect>,
    pub cells: Vec<(u32, egui::Rect)>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            want: None,
            built: None,
            failed: None,
            heading: 0.0,
            turning: true,
            zoom: 1.0,
            shade_held: None,
            row_shade_held: None,
            rect: None,
            builds: 0,
            memo: None,
            files: None,
            index: Index::Unread,
            names: None,
            pictures: HashMap::new(),
            last_time: None,
            open: None,
            search: String::new(),
            just_opened: false,
            buttons: Vec::new(),
            slider: None,
            cells: Vec::new(),
        }
    }
}

impl std::fmt::Debug for Appearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Appearance")
            .field("built", &self.built.as_ref().map(|b| b.0.setup))
            .field("heading", &self.heading)
            .field("zoom", &self.zoom)
            .field("builds", &self.builds)
            .finish_non_exhaustive()
    }
}

impl Appearance {
    /// What was drawn last let go, before a frame draws it again.
    pub fn clear_drawn(&mut self) {
        self.buttons.clear();
        self.slider = None;
        self.cells.clear();
        self.rect = None;
    }

    /// The look in the preview's space now.
    #[must_use]
    pub fn drawn(&self) -> Option<&Drawn> {
        self.built.as_ref().map(|b| &b.0)
    }

    /// The button of picker `p`, as drawn last.
    #[must_use]
    pub fn button(&self, p: LookPicker) -> Option<egui::Rect> {
        self.buttons.iter().find(|(q, _)| *q == p).map(|(_, r)| *r)
    }
}

/// Where a body of box `(min, max)` is looked at as it turns about its own upright: a point on that
/// upright halfway up the box, and how far about it the view reaches: half its height or how far
/// it reaches from its upright at any turn, whichever is more, and a margin.
#[must_use]
pub fn framing(min: Vec3, max: Vec3) -> (Vec3, f32) {
    const MARGIN: f32 = 1.1;
    let across = dereth_primitives::num::math::hypotf(
        min.x.abs().max(max.x.abs()),
        min.y.abs().max(max.y.abs()),
    );
    let half = (max.z - min.z) * 0.5;
    (
        Vec3::new(0.0, 0.0, (min.z + max.z) * 0.5),
        (across.max(half) * MARGIN).max(0.1),
    )
}

/// A key naming a picture of `setup` dressed in `od`, the same from one run to the next.
fn look_key(setup: DataId, od: &ObjDesc) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    let mut eat = |v: u32| {
        for b in v.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
    };
    eat(setup.0);
    eat(od.palette_id.0);
    for p in &od.part_changes {
        eat(p.part_index);
        eat(p.part_id.0);
    }
    for t in &od.texture_changes {
        eat(t.part_index);
        eat(t.old_texture.0);
        eat(t.new_texture.0);
    }
    for s in &od.subpalettes {
        eat(s.palette_set.0);
        eat(s.offset);
        eat(s.length);
    }
    h
}

/// A row of swatches, one for each color.
fn swatches(ui: &mut egui::Ui, colors: &[[u8; 3]]) {
    for [r, g, b] in colors {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 2.0, egui::Color32::from_rgb(*r, *g, *b));
        ui.painter().rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
    }
}

/// The name a clothing look is known by: the name most of its weenies have, and how many other
/// names they have.
fn label_of(names: &[String]) -> Option<String> {
    match names {
        [] => None,
        [one] => Some(one.clone()),
        [first, rest @ ..] => Some(format!("{first} +{}", rest.len())),
    }
}

/// Clothing look `id` in words: the name it is known by and its id, or its id alone when no
/// weenie is dressed in it.
#[must_use]
fn clothing_words(names: &[String], id: u32) -> String {
    match label_of(names) {
        Some(n) => format!("{n} (0x{id:08X})"),
        None => format!("0x{id:08X}"),
    }
}

/// What a clothing look's tooltip says: the first few weenies dressed in it, and its id.
fn clothing_about(names: Option<&[String]>, id: u32, t: &crate::text::Table) -> String {
    format!(
        "{}\n{}",
        names.filter(|n| !n.is_empty()).map_or_else(
            || t.get("appearance.unnamed").to_owned(),
            |n| n.iter().take(6).cloned().collect::<Vec<_>>().join("\n")
        ),
        t.fill("appearance.clothing_id", &[("id", &format!("0x{id:08X}"))])
    )
}

impl EditorApp {
    /// Weenie `w`'s SQL text, as its pack writes it or the server has it; `None` while it is read
    /// or when nothing writes it.
    fn look_text(&mut self, w: &Weenie) -> Option<String> {
        match (self.session.weenie_file(w), w) {
            (Some((f, _)), _) => self.session.sql_text(&f),
            (None, Weenie::Class(c)) => self.server_class(*c).flatten(),
            (None, Weenie::Symbol(_)) => None,
        }
    }

    /// The files the looks are read from: the world's, read again when they are.
    pub(super) fn look_files(&mut self) -> &DatFiles {
        let generation = self.core.store_generation();
        if self
            .appearance
            .files
            .as_ref()
            .is_none_or(|(g, _)| *g != generation)
        {
            self.appearance.files = Some((generation, DatFiles::new(Arc::clone(&self.core.store))));
            self.appearance.memo = None;
        }
        &self.appearance.files.as_ref().expect("made above").1
    }

    /// Weenie `w`'s look as the preview draws it: its setup dressed as a server describes it, at
    /// its size; a shade being dragged (its own, or a create list row's) in place of the one
    /// written. `None` while its SQL is read, or for one with no setup.
    pub fn weenie_look(&mut self, w: &Weenie) -> Option<Drawn> {
        let text = self.look_text(w)?;
        let held = self
            .appearance
            .shade_held
            .as_ref()
            .filter(|(hw, _)| hw == w)
            .map(|(_, s)| *s);
        let row_held = self
            .appearance
            .row_shade_held
            .as_ref()
            .filter(|(hw, ..)| hw == w)
            .map(|(_, i, s)| (*i, *s));
        let generation = self.core.store_generation();
        let key = (
            w.clone(),
            text,
            self.session.history(),
            generation,
            held.map(f64::to_bits),
            row_held.map(|(i, s)| (i, s.to_bits())),
            self.weenies.len(),
        );
        if let Some((k, d)) = &self.appearance.memo {
            if *k == key {
                return d.clone();
            }
        }
        let mut doc = SqlDoc::parse(&key.1);
        if let Some((i, s)) = row_held {
            let mut rows = super::collections::rows_of(&doc, super::collections::CREATE_LIST);
            if let Some(r) = rows.get_mut(i) {
                r.set("shade", super::collections::decimal(s));
                super::collections::write_rows(&mut doc, super::collections::CREATE_LIST, &rows);
            }
        }
        let mut l = look::look_of(&doc);
        if held.is_some() {
            l.shade = held;
        }
        let (wields, unknown) = if l.is_creature() {
            look::wields_of(&doc, &mut |c: &str| {
                let item = self.weenie_written(c)?;
                self.look_text(&item).map(|t| SqlDoc::parse(&t))
            })
        } else {
            (Vec::new(), 0)
        };
        let worn = rules::dress(&wields);
        let drawn = l.setup.map(|setup| {
            let wire = rules::object_description(&l, &worn, self.look_files());
            #[allow(clippy::cast_possible_truncation)] // a size
            let scale = SqlProps(&doc)
                .float(rules::id::DEFAULT_SCALE)
                .map_or(1.0, |s| s as f32);
            Drawn {
                setup: DataId(setup),
                objdesc: dereth_client_runtime::movement::to_anim_objdesc(&wire),
                scale,
            }
        });
        // A look still waiting on a wielded piece's rows is worked out again until it has them.
        if unknown == 0 {
            self.appearance.memo = Some((key, drawn.clone()));
        }
        drawn
    }

    /// The preview of weenie `w` in the editor's head, [`SIDE`] points square: turning by itself,
    /// turned by a drag, brought nearer by the wheel, framed again by a double-click. Nothing for
    /// a weenie with no setup.
    pub(super) fn look_preview(&mut self, ui: &mut egui::Ui, w: &Weenie, has_setup: bool) {
        let drawn = self.weenie_look(w);
        if drawn.is_none() && !has_setup {
            return;
        }
        let t = self.text.table();
        let (rect, r) =
            ui.allocate_exact_size(egui::vec2(SIDE, SIDE), egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
        if drawn.is_some() {
            ui.painter().add(egui::PaintCallback {
                rect,
                callback: Arc::new(PreviewPaint(SPACE)),
            });
        }
        let a = &mut self.appearance;
        if r.dragged() {
            a.heading = (a.heading + r.drag_delta().x * DRAG_TURN).rem_euclid(360.0);
            a.turning = false;
        }
        if r.double_clicked() {
            a.zoom = 1.0;
            a.turning = true;
        }
        if r.hovered() {
            let notches: f32 = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum()
            });
            if notches != 0.0 {
                a.zoom = (a.zoom * dereth_primitives::num::math::powf(ZOOM_STEP, notches))
                    .clamp(ZOOM.0, ZOOM.1);
            }
        }
        r.on_hover_text(t.get("appearance.turn"));
        a.want = drawn;
        a.rect = Some(rect);
    }

    /// The look asked of the preview this frame put in its space: built again when it changed,
    /// turned, framed, and its animation and effects run on. Outside the frame's drawing, as
    /// building an object needs the device.
    pub(super) fn appearance_frame(&mut self) {
        let now = self.core.timer.elapsed_time();
        #[allow(clippy::cast_possible_truncation)] // a frame's seconds
        let dt = self
            .appearance
            .last_time
            .map_or(0.0, |t| (now - t).clamp(0.0, 0.25)) as f32;
        self.appearance.last_time = Some(now);
        let Some(want) = self.appearance.want.take() else {
            return;
        };
        let a = &mut self.appearance;
        if a.turning {
            a.heading = (a.heading + TURN_RATE * dt).rem_euclid(360.0);
        }
        let (heading, zoom) = (a.heading, a.zoom);
        let generation = self.core.store_generation();
        let assets = Arc::clone(&self.core.anim_assets);
        let store = Arc::clone(&self.core.store);
        let Some(r) = self.core.present.as_any_mut().downcast_mut::<Renderer>() else {
            return;
        };
        if r.ensure_preview(SPACE, &assets) {
            self.appearance.built = None;
            if let Some(space) = r.preview_mut(SPACE) {
                space.set_light(
                    dereth_world_render::lighting::LightType::Directional,
                    crate::thumbs::LIGHT,
                    crate::thumbs::LIGHT_DIRECTION,
                );
                space.mode.set_fov(crate::thumbs::FOV);
            }
        }
        let fresh = self
            .appearance
            .built
            .as_ref()
            .is_none_or(|(d, g, ..)| *d != want || *g != generation);
        let failed = self
            .appearance
            .failed
            .as_ref()
            .is_some_and(|(d, g)| *d == want && *g == generation);
        if fresh && failed {
            return;
        }
        if fresh {
            self.appearance.built = None;
            if let Some(space) = r.preview_mut(SPACE) {
                space.remove_all_objects();
            }
            match r.add_preview_object_dressed(SPACE, &store, want.setup, Some(&want.objdesc)) {
                Ok(Some(i)) => {
                    let Some(space) = r.preview_mut(SPACE) else {
                        return;
                    };
                    space.set_scale(i, want.scale);
                    let Some(bb) = space.object(i).map(|o| o.bounding_box(&store)) else {
                        return;
                    };
                    let (center, reach) = framing(bb.min, bb.max);
                    self.appearance.built = Some((want, generation, center, reach));
                    self.appearance.builds += 1;
                }
                Ok(None) => {
                    self.appearance.failed = Some((want, generation));
                    return;
                }
                Err(e) => {
                    tracing::warn!("the weenie's preview: {e}");
                    self.appearance.failed = Some((want, generation));
                    return;
                }
            }
        }
        let Some((_, _, center, reach)) = self.appearance.built.as_ref() else {
            return;
        };
        let (center, reach) = (*center, *reach);
        if let Some(space) = r.preview_mut(SPACE) {
            space.set_heading(0, heading);
            space.mode.view_frame = crate::thumbs::camera(center, reach, crate::thumbs::FOV, zoom);
        }
        if let Err(e) = r.preview_use_time(SPACE, f64::from(dt)) {
            tracing::warn!("the weenie's preview: {e}");
        }
    }

    // ---------------------------------------------------------------------------------------
    // the pickers
    // ---------------------------------------------------------------------------------------

    /// The clothing tables of the files, read off the frame the first time they are asked for.
    fn clothing_index(&mut self) -> Option<Arc<ClothingIndex>> {
        match &self.appearance.index {
            Index::Ready(i) => return Some(Arc::clone(i)),
            Index::Reading(rx) => {
                if let Ok(i) = rx.try_recv() {
                    let i = Arc::new(i);
                    self.appearance.index = Index::Ready(Arc::clone(&i));
                    return Some(i);
                }
                return None;
            }
            Index::Unread => {}
        }
        let (tx, rx) = mpsc::channel();
        let store = Arc::clone(&self.base_store);
        let started = std::thread::Builder::new()
            .name("lyceum clothing".into())
            .spawn(move || {
                let _ = tx.send(ClothingIndex::read(&store));
            });
        if started.is_ok() {
            self.appearance.index = Index::Reading(rx);
        }
        None
    }

    /// Wait for the clothing tables to be read.
    pub fn wait_for_clothing(&mut self) {
        let _ = self.clothing_index();
        if let Index::Reading(rx) = &self.appearance.index {
            if let Ok(i) = rx.recv() {
                self.appearance.index = Index::Ready(Arc::new(i));
            }
        }
    }

    /// The names of the weenies dressed in each clothing table, the commonest first.
    fn clothing_names(&mut self) -> &BTreeMap<u32, Vec<String>> {
        let n = self.weenies.len();
        if self.appearance.names.as_ref().is_none_or(|(m, _)| *m != n) {
            let mut counts: BTreeMap<u32, BTreeMap<String, usize>> = BTreeMap::new();
            for e in &self.weenies {
                if let Some(t) = e.clothing_base {
                    *counts
                        .entry(t)
                        .or_default()
                        .entry(e.name.clone())
                        .or_default() += 1;
                }
            }
            let names = counts
                .into_iter()
                .map(|(t, c)| {
                    let mut v: Vec<(String, usize)> = c.into_iter().collect();
                    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                    (t, v.into_iter().map(|(name, _)| name).collect())
                })
                .collect();
            self.appearance.names = Some((n, names));
        }
        &self.appearance.names.as_ref().expect("made above").1
    }

    /// The picture of `setup` dressed in `od`, drawn as the model browser's are while `budget`
    /// lasts.
    fn look_picture(
        &mut self,
        setup: DataId,
        od: &ObjDesc,
        budget: &mut u32,
    ) -> Option<egui::TextureId> {
        let key = look_key(setup, od);
        if let Some(t) = self.appearance.pictures.get(&key) {
            return Some(t.id());
        }
        if self.thumbs.is_none() {
            let backend = self.backend();
            self.thumbs = Some(crate::thumbs::Worker::open(
                Arc::clone(&self.base_store),
                backend,
                &self.thumbs_root,
            ));
        }
        let data = self
            .thumbs
            .as_mut()?
            .get_look(key, setup, od, budget)?
            .clone();
        let mut rgba = data.levels.first().cloned().unwrap_or_default();
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [data.width as usize, data.height as usize],
            &rgba,
        );
        let handle = self.gui.ctx.load_texture(
            format!("look {key:016X}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        let id = handle.id();
        self.appearance.pictures.insert(key, handle);
        Some(id)
    }

    /// `doc`'s setup dressed in clothing table `table` (none for `None`), in its own color scheme
    /// and shade, as an object of no other look: what a clothing look's picture shows.
    fn dressed_in(&mut self, doc: &SqlDoc, table: Option<u32>) -> Option<(DataId, ObjDesc)> {
        let mut l = look::look_of(doc);
        let setup = l.setup?;
        l.weenie_type = 0;
        l.clothing_base = table;
        l.own = rules::OwnChanges::default();
        let wire = rules::object_description(&l, &[], self.look_files());
        Some((
            DataId(setup),
            dereth_client_runtime::movement::to_anim_objdesc(&wire),
        ))
    }

    /// A picker's button: what it shows drawn by `face` beside `picture`, and the popout it opens
    /// drawn by `popout` while it is open.
    #[allow(clippy::too_many_arguments)]
    fn look_button(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        which: LookPicker,
        picture: Option<egui::TextureId>,
        face: &dyn Fn(&mut egui::Ui, egui::Rect),
        about: &str,
        key: String,
        popout: &mut dyn FnMut(&mut Self, &mut egui::Ui),
    ) {
        let (rect, _) = ui.allocate_exact_size(BUTTON, egui::Sense::hover());
        let r = ui.interact(
            rect,
            egui::Id::new(("look picker", which, w.clone())),
            egui::Sense::click(),
        );
        let v = ui.style().interact(&r);
        let painter = ui.painter_at(rect);
        painter.rect(
            rect,
            v.corner_radius,
            v.weak_bg_fill,
            v.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let side = rect.height() - 6.0;
        let mut words = rect;
        words.min.x += 8.0;
        if which == LookPicker::Clothing {
            let at =
                egui::Rect::from_min_size(rect.min + egui::vec2(3.0, 3.0), egui::vec2(side, side));
            match picture {
                Some(id) => {
                    painter.image(
                        id,
                        at,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                None => {
                    painter.rect_filled(at, 2.0, egui::Color32::from_gray(20));
                }
            }
            words.min.x = at.max.x + 8.0;
        }
        face(ui, words);
        let r = r.on_hover_text(about);
        self.appearance.buttons.push((which, rect));
        self.form_fields.push((key, rect));
        if r.clicked() && !egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&r)) {
            self.appearance.open = Some(which);
            self.appearance.search.clear();
            self.appearance.just_opened = true;
        }
        let shown = egui::Popup::from_toggle_button_response(&r)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(POPOUT_WIDTH)
            .show(|ui| {
                ui.set_width(POPOUT_WIDTH);
                popout(self, ui);
                self.appearance.just_opened = false;
            });
        if shown.is_some() {
            self.appearance.open = Some(which);
        } else if self.appearance.open == Some(which) {
            self.appearance.open = None;
        }
    }

    /// The Clothing look picker of weenie `w` as `doc` has it.
    pub(super) fn clothing_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let now = SqlProps(doc).data_id(rules::id::CLOTHING_BASE);
        let mut budget = super::PICTURES_A_FRAME;
        let picture = self
            .dressed_in(doc, now)
            .filter(|_| now.is_some())
            .and_then(|(s, od)| self.look_picture(s, &od, &mut budget));
        let names = now.and_then(|n| self.clothing_names().get(&n).cloned());
        let name = names.as_deref().and_then(label_of);
        // Its name over its id; its id alone when nothing names it.
        let (words, id_line) = match (now, &name) {
            (None, _) => (t.get("fields.none").to_owned(), None),
            (Some(n), Some(name)) => (name.clone(), Some(format!("0x{n:08X}"))),
            (Some(n), None) => (format!("0x{n:08X}"), None),
        };
        let about = now.map_or_else(
            || t.get("appearance.choose_clothing").to_owned(),
            |n| clothing_about(names.as_deref(), n, &t),
        );
        let key = super::fields::property_key(PropertyKind::DataId, rules::id::CLOTHING_BASE);
        let face = move |ui: &mut egui::Ui, at: egui::Rect| {
            let mut job = egui::text::LayoutJob::simple(
                words.clone(),
                egui::FontId::proportional(12.5),
                ui.visuals().text_color(),
                at.width() - 6.0,
            );
            if id_line.is_some() {
                job.wrap.max_rows = 1;
            }
            let galley = ui.painter().layout_job(job);
            let id = id_line.as_ref().map(|s| {
                ui.painter().layout_no_wrap(
                    s.clone(),
                    egui::FontId::monospace(10.5),
                    ui.visuals().weak_text_color(),
                )
            });
            let high = galley.size().y + id.as_ref().map_or(0.0, |g| g.size().y + 1.0);
            let mut y = at.center().y - high / 2.0;
            let painter = ui.painter().with_clip_rect(at);
            let below = galley.size().y + 1.0;
            painter.galley(egui::pos2(at.min.x, y), galley, egui::Color32::WHITE);
            if let Some(id) = id {
                y += below;
                painter.galley(egui::pos2(at.min.x, y), id, egui::Color32::WHITE);
            }
        };
        let doc2 = doc.clone();
        let w2 = w.clone();
        let mut chosen: Option<Option<u32>> = None;
        let worn = self.worn_names(doc);
        ui.vertical(|ui| {
            self.look_button(
                ui,
                w,
                LookPicker::Clothing,
                picture,
                &face,
                &about,
                key,
                &mut |app, ui| {
                    chosen = app.clothing_popout(ui, &w2, &doc2, now);
                },
            );
            // A creature's clothes are mostly what it wields.
            if !worn.is_empty() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            t.fill("appearance.wears", &[("pieces", &worn.join(", "))]),
                        )
                        .weak(),
                    )
                    .wrap(),
                );
            }
        });
        if let Some(c) = chosen {
            if c != now {
                out.push(UiAction::Form(
                    w.clone(),
                    FormEdit::Property {
                        kind: PropertyKind::DataId,
                        id: rules::id::CLOTHING_BASE,
                        value: c.map(|n| format!("0x{n:08X}")),
                    },
                ));
            }
        }
    }

    /// The names of what a creature of `doc` wears of what its create list wields, in the order it
    /// puts them on; nothing for any other weenie.
    fn worn_names(&mut self, doc: &SqlDoc) -> Vec<String> {
        if !look::look_of(doc).is_creature() {
            return Vec::new();
        }
        let mut names = Vec::new();
        let (wields, _) = look::wields_of(doc, &mut |c: &str| {
            let item = self.weenie_written(c)?;
            let d = SqlDoc::parse(&self.look_text(&item)?);
            names.push(
                SqlProps(&d)
                    .string(1)
                    .unwrap_or_else(|| c.trim().to_owned()),
            );
            Some(d)
        });
        rules::dress(&wields)
            .iter()
            .filter_map(|w| wields.iter().position(|x| *x == w.piece))
            .filter_map(|i| names.get(i).cloned())
            .collect()
    }

    /// The Clothing look picker's popout: its search, and the clothing tables that dress the
    /// weenie's setup, each drawn on it. The one clicked (`Some(None)` for none).
    fn clothing_popout(
        &mut self,
        ui: &mut egui::Ui,
        _w: &Weenie,
        doc: &SqlDoc,
        now: Option<u32>,
    ) -> Option<Option<u32>> {
        let t = self.text.table();
        let r = super::panels::search_box(ui, &mut self.appearance.search, t.get("pickers.search"));
        if self.appearance.just_opened {
            r.request_focus();
        }
        let Some(setup) = SqlProps(doc).data_id(rules::id::SETUP) else {
            ui.weak(t.get("appearance.no_model"));
            return None;
        };
        let Some(index) = self.clothing_index() else {
            ui.weak(t.get("appearance.reading"));
            ui.ctx().request_repaint();
            return None;
        };
        let search = self.appearance.search.trim().to_lowercase();
        let names = self.clothing_names().clone();
        let mut list: Vec<Option<u32>> = vec![None];
        list.extend(
            index
                .dressing(setup)
                .iter()
                .filter(|id| {
                    search.is_empty()
                        || format!("{id:08x}").contains(&search)
                        || names
                            .get(id)
                            .is_some_and(|n| n.iter().any(|x| x.to_lowercase().contains(&search)))
                })
                .map(|id| Some(*id)),
        );
        if list.len() == 1 && index.dressing(setup).is_empty() {
            ui.weak(t.get("appearance.none_fit"));
        }
        let gap = 4.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few columns
        let columns =
            (((ui.available_width() + gap) / (CLOTHING_CELL.x + gap)).floor() as usize).max(1);
        let rows = list.len().div_ceil(columns);
        let row_height = CLOTHING_CELL.y + gap;
        let mut area = egui::ScrollArea::vertical()
            .id_salt("clothing picker")
            .max_height(GRID_HEIGHT)
            .min_scrolled_height(GRID_HEIGHT)
            .auto_shrink([false, true]);
        if self.appearance.just_opened {
            let at = list.iter().position(|x| *x == now).unwrap_or(0);
            #[allow(clippy::cast_precision_loss)] // a row's place
            let y = (at / columns) as f32 * row_height;
            area = area.vertical_scroll_offset((y - GRID_HEIGHT / 2.0 + row_height / 2.0).max(0.0));
        }
        let mut budget = super::PICTURES_A_FRAME;
        let mut chosen = None;
        area.show_rows(ui, CLOTHING_CELL.y, rows, |ui, shown| {
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            for row in shown {
                ui.horizontal(|ui| {
                    for table in list.iter().skip(row * columns).take(columns) {
                        let picture = self
                            .dressed_in(doc, *table)
                            .and_then(|(s, od)| self.look_picture(s, &od, &mut budget));
                        let (caption, about) = match table {
                            None => (
                                t.get("fields.none").to_owned(),
                                t.get("appearance.no_clothing").to_owned(),
                            ),
                            Some(id) => {
                                let label = names.get(id).and_then(|n| label_of(n));
                                let about =
                                    clothing_about(names.get(id).map(Vec::as_slice), *id, &t);
                                (label.unwrap_or_else(|| format!("{id:08X}")), about)
                            }
                        };
                        let r = super::pickers::cell(
                            ui,
                            CLOTHING_CELL,
                            56.0,
                            picture,
                            Some(&caption),
                            *table == now,
                        );
                        self.appearance.cells.push((table.unwrap_or(0), r.rect));
                        if r.on_hover_text(about).clicked() {
                            chosen = Some(*table);
                        }
                    }
                });
            }
        });
        if chosen.is_some() {
            ui.close();
        }
        chosen
    }

    /// The colors of scheme `key` of clothing table `table` at `shade`.
    fn scheme_colors(&mut self, table: u32, key: u32, shade: f64) -> Vec<[u8; 3]> {
        look::scheme_colors(self.look_files(), table, key, shade)
    }

    /// The Color scheme picker of weenie `w` as `doc` has it: its clothing look's schemes, each by
    /// the server's name with its colors at the weenie's shade. A weenie whose clothing look has
    /// no schemes chooses by the server's names alone, as any named value is chosen.
    pub(super) fn scheme_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) -> bool {
        let t = self.text.table();
        let props = SqlProps(doc);
        let Some(table_id) = props.data_id(rules::id::CLOTHING_BASE) else {
            return false;
        };
        let Some(table) = self.look_files().clothing(table_id) else {
            return false;
        };
        if table.palette_templates.is_empty() {
            return false;
        }
        let shade = self
            .appearance
            .shade_held
            .as_ref()
            .filter(|(hw, _)| hw == w)
            .map(|(_, s)| *s)
            .or(props.float(rules::id::SHADE))
            .unwrap_or(0.0);
        let now = props.int(rules::id::PALETTE_TEMPLATE);
        let worn = rules::scheme_of(&table, table_id, now, self.look_files()).map(|(k, _)| k);
        let schema = self.schema.clone();
        let name_of = |k: u32| {
            schema
                .as_ref()
                .and_then(|s| s.value("PaletteTemplate", k))
                .map_or_else(|| k.to_string(), super::fields::spaced)
        };
        let colors = worn
            .map(|k| self.scheme_colors(table_id, k, shade))
            .unwrap_or_default();
        let words = match (now, worn) {
            (None, _) if props.float(rules::id::SHADE).is_none() => {
                t.get("appearance.own_colors").to_owned()
            }
            (_, Some(k)) => name_of(k),
            (Some(n), None) => n.to_string(),
            (None, None) => t.get("fields.none").to_owned(),
        };
        let about = match now {
            Some(n) => t.fill("appearance.scheme_id", &[("n", &n)]),
            None => t.get("appearance.choose_scheme").to_owned(),
        };
        let key = super::fields::property_key(PropertyKind::Int, rules::id::PALETTE_TEMPLATE);
        let face = move |ui: &mut egui::Ui, at: egui::Rect| {
            let mut x = at.min.x;
            let y = at.center().y - SWATCH / 2.0;
            for [r, g, b] in colors.iter().take(4) {
                let s = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(SWATCH, SWATCH));
                ui.painter()
                    .rect_filled(s, 2.0, egui::Color32::from_rgb(*r, *g, *b));
                x += SWATCH + 2.0;
            }
            ui.painter().with_clip_rect(at).text(
                egui::pos2(x + 6.0, at.center().y),
                egui::Align2::LEFT_CENTER,
                &words,
                egui::FontId::proportional(12.5),
                ui.visuals().text_color(),
            );
        };
        let keys: Vec<u32> = table.palette_templates.keys().copied().collect();
        let mut chosen = None;
        self.look_button(
            ui,
            w,
            LookPicker::Scheme,
            None,
            &face,
            &about,
            key,
            &mut |app, ui| {
                let t = app.text.table();
                ui.weak(t.get("appearance.schemes_at_shade"));
                egui::ScrollArea::vertical()
                    .id_salt("scheme picker")
                    .max_height(GRID_HEIGHT)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for k in &keys {
                            let colors = app.scheme_colors(table_id, *k, shade);
                            let r = ui
                                .horizontal(|ui| {
                                    swatches(ui, &colors);
                                    ui.add_space(4.0);
                                    ui.add(
                                        egui::Button::selectable(
                                            worn == Some(*k) && now.is_some(),
                                            name_of(*k),
                                        )
                                        .min_size(egui::vec2(SCHEME_NAME, SWATCH + 6.0)),
                                    )
                                })
                                .inner;
                            app.appearance.cells.push((*k, r.rect));
                            let about = t.fill("appearance.scheme_id", &[("n", k)]);
                            if r.on_hover_text(about).clicked() {
                                chosen = Some(*k);
                            }
                        }
                    });
                if chosen.is_some() {
                    ui.close();
                }
            },
        );
        if let Some(k) = chosen {
            if now != i32::try_from(k).ok() {
                out.push(UiAction::Form(
                    w.clone(),
                    FormEdit::Property {
                        kind: PropertyKind::Int,
                        id: rules::id::PALETTE_TEMPLATE,
                        value: Some(k.to_string()),
                    },
                ));
            }
        }
        true
    }

    /// The Shade slider of weenie `w` as `doc` has it, 0 (dark) to 1 (light): the preview follows
    /// it as it is dragged, and it is written, as one step, when it is let go.
    pub(super) fn shade_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let own = SqlProps(doc).float(rules::id::SHADE);
        let mut v = self
            .appearance
            .shade_held
            .as_ref()
            .filter(|(hw, _)| hw == w)
            .map_or(own.unwrap_or(0.0), |(_, s)| *s);
        let r = ui
            .add(
                egui::Slider::new(&mut v, 0.0..=1.0)
                    .step_by(0.01)
                    .fixed_decimals(2),
            )
            .on_hover_text(t.get("appearance.shade"));
        self.appearance.slider = Some(r.rect);
        self.form_fields.push((
            super::fields::property_key(PropertyKind::Float, rules::id::SHADE),
            r.rect,
        ));
        let v = (v * 100.0).round() / 100.0;
        if r.dragged() {
            self.appearance.shade_held = Some((w.clone(), v));
            return;
        }
        let let_go = r.drag_stopped() || r.changed();
        if self
            .appearance
            .shade_held
            .as_ref()
            .is_some_and(|(hw, _)| hw == w)
        {
            self.appearance.shade_held = None;
        }
        if let_go && own != Some(v) {
            out.push(UiAction::Form(
                w.clone(),
                FormEdit::Property {
                    kind: PropertyKind::Float,
                    id: rules::id::SHADE,
                    value: Some(format!("{v}")),
                },
            ));
        }
    }

    /// Clothing look `id` in words: the name the weenies dressed in it give it, and its id.
    pub(super) fn clothing_look_words(&mut self, id: u32) -> String {
        let names = self.clothing_names().get(&id).cloned().unwrap_or_default();
        clothing_words(&names, id)
    }

    /// The Color of a row (field `key`) of weenie `w`'s create list making `class` in scheme
    /// `palette` (0: its own) at `shade` (`None` for a treasure row, whose shade is its chance:
    /// what it makes keeps its own): the schemes of what it makes, each by the server's name with
    /// its colors at that shade; else the server's names alone; else its number. A choice is
    /// written, as one step, by `edit` of the scheme's number.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(super) fn row_scheme_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        key: &str,
        class: &str,
        (palette, shade): (u32, Option<f64>),
        edit: &dyn Fn(String) -> FormEdit,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let item = self
            .weenie_written(class)
            .and_then(|it| self.look_text(&it))
            .map(|s| SqlDoc::parse(&s));
        let props = item.as_ref().map(SqlProps);
        let own = props
            .as_ref()
            .and_then(|p| p.int(rules::id::PALETTE_TEMPLATE));
        let shade = shade
            .or_else(|| props.as_ref().and_then(|p| p.float(rules::id::SHADE)))
            .unwrap_or(0.0);
        let table = props
            .as_ref()
            .and_then(|p| p.data_id(rules::id::CLOTHING_BASE))
            .and_then(|id| self.look_files().clothing(id).map(|t| (id, t)))
            .filter(|(_, t)| !t.palette_templates.is_empty());
        let schema = self.schema.clone();
        let name_of = |k: u32| {
            schema
                .as_ref()
                .and_then(|s| s.value("PaletteTemplate", k))
                .map_or_else(|| k.to_string(), super::fields::spaced)
        };
        // What it wears: the row's scheme over its own, of its clothing's when it has some.
        let asked = if palette > 0 {
            i32::try_from(palette).ok()
        } else {
            own
        };
        let (keys, worn, own_key) = match &table {
            Some((id, tb)) => (
                tb.palette_templates.keys().copied().collect::<Vec<_>>(),
                rules::scheme_of(tb, *id, asked, self.look_files()).map(|(k, _)| k),
                rules::scheme_of(tb, *id, own, self.look_files()).map(|(k, _)| k),
            ),
            None => {
                let positive =
                    |k: Option<i32>| k.and_then(|k| u32::try_from(k).ok()).filter(|k| *k > 0);
                (
                    super::fields::enum_values(schema.as_ref(), "PaletteTemplate")
                        .into_iter()
                        .map(|(k, _)| k)
                        .collect(),
                    positive(asked),
                    positive(own),
                )
            }
        };
        if keys.is_empty() {
            let refused = t.get("messages.not_a_value").to_owned();
            self.field(
                ui,
                w,
                key,
                &palette.to_string(),
                36.0,
                |v| match v.trim().parse::<u32>() {
                    Ok(n) => edit(n.to_string()),
                    Err(_) => FormEdit::Refused(refused),
                },
                out,
            );
            return;
        }
        let colors_of = |app: &mut Self, k: Option<u32>| match (&table, k) {
            (Some((id, _)), Some(k)) => look::scheme_colors(app.look_files(), *id, k, shade),
            _ => Vec::new(),
        };
        let shown = colors_of(self, worn);
        let own_colors = colors_of(self, own_key);
        let own_words = t.get("collections.own_color").to_owned();
        let words = worn.map_or_else(|| own_words.clone(), name_of);
        // A row of its own colors says so by its name's weaker look.
        let words = if palette == 0 {
            egui::RichText::new(words).weak()
        } else {
            egui::RichText::new(words)
        };
        let about = if palette == 0 {
            own_words.clone()
        } else {
            t.fill("appearance.scheme_id", &[("n", &palette)])
        };
        let mut chosen = None;
        ui.horizontal(|ui| {
            swatches(ui, &shown[..shown.len().min(4)]);
            let combo = egui::ComboBox::from_id_salt(("row scheme", key, w.clone()))
                .selected_text(words)
                .width(ROW_SCHEME)
                .height(GRID_HEIGHT)
                .show_ui(ui, |ui| {
                    for k in std::iter::once(0).chain(keys.iter().copied()) {
                        let (colors, words) = if k == 0 {
                            (own_colors.clone(), own_words.clone())
                        } else {
                            (colors_of(self, Some(k)), name_of(k))
                        };
                        let r = ui
                            .horizontal(|ui| {
                                swatches(ui, &colors);
                                ui.add_space(4.0);
                                ui.selectable_label(palette == k, words)
                            })
                            .inner;
                        self.editor.picker_rows.push((format!("{key}:{k}"), r.rect));
                        if r.clicked() && k != palette {
                            chosen = Some(k);
                        }
                    }
                });
            let r = combo.response.on_hover_text(about);
            self.form_fields.push((key.to_owned(), r.rect));
        });
        if let Some(k) = chosen {
            out.push(UiAction::Form(w.clone(), edit(k.to_string())));
        }
    }

    /// The Shade of a row (field `key`, the `at`th row of its create list) of weenie `w`'s create
    /// list, written as `own`, 0 (dark) to 1 (light): a slider the preview follows as it is
    /// dragged, written, as one step, by `edit` when it is let go.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn row_shade_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        key: &str,
        at: usize,
        own: f64,
        edit: &dyn Fn(String) -> FormEdit,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let held = self
            .appearance
            .row_shade_held
            .as_ref()
            .filter(|(hw, i, _)| hw == w && *i == at)
            .map(|(.., s)| *s);
        let mut v = held.unwrap_or(own);
        let r = ui
            .scope(|ui| {
                ui.spacing_mut().slider_width = ROW_SHADE;
                ui.add(
                    egui::Slider::new(&mut v, 0.0..=1.0)
                        .step_by(0.01)
                        .fixed_decimals(2)
                        .update_while_editing(false),
                )
            })
            .inner
            .on_hover_text(t.get("appearance.shade"));
        self.form_fields.push((key.to_owned(), r.rect));
        let v = (v * 100.0).round() / 100.0;
        if r.dragged() {
            self.appearance.row_shade_held = Some((w.clone(), at, v));
            return;
        }
        let let_go = r.drag_stopped() || r.changed();
        if held.is_some() {
            self.appearance.row_shade_held = None;
        }
        if let_go && (v - own).abs() > 1e-9 {
            out.push(UiAction::Form(
                w.clone(),
                edit(super::collections::decimal(v)),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the weenie editor's preview)
    use super::*;

    #[test]
    fn a_body_is_framed_about_its_upright_so_that_no_turn_takes_it_out_of_view() {
        let (center, reach) = framing(Vec3::new(-0.3, -0.4, 0.0), Vec3::new(0.3, 0.2, 1.8));
        assert_eq!(center, Vec3::new(0.0, 0.0, 0.9));
        // Half its height, 0.9, is more than its widest reach from the upright, 0.5 (0.3 across,
        // 0.4 deep).
        assert!((reach - 0.9 * 1.1).abs() < 1e-5, "{reach}");
        // A long, low body is framed by its reach.
        let (_, long) = framing(Vec3::new(-2.0, -0.5, 0.0), Vec3::new(1.0, 0.5, 0.6));
        assert!(
            (long - dereth_primitives::num::math::hypotf(2.0, 0.5) * 1.1).abs() < 1e-4,
            "{long}"
        );
    }

    #[test]
    fn a_clothing_look_is_shown_by_the_name_its_weenies_give_it_and_its_id_else_by_its_id() {
        let names = ["Shirt".to_owned(), "Tunic".to_owned(), "Smock".to_owned()];
        assert_eq!(clothing_words(&names, 0x1000_012A), "Shirt +2 (0x1000012A)");
        assert_eq!(
            clothing_words(&names[..1], 0x1000_012A),
            "Shirt (0x1000012A)"
        );
        assert_eq!(clothing_words(&[], 0x1000_012A), "0x1000012A");
        // Its tooltip names the first few weenies dressed in it, then its id.
        let t = crate::text::Text::built_in().table();
        let about = clothing_about(Some(&names), 0x1000_012A, &t);
        assert!(
            about.starts_with("Shirt\nTunic\nSmock\n") && about.contains("0x1000012A"),
            "{about}"
        );
        assert!(clothing_about(None, 0x1000_012A, &t).contains("0x1000012A"));
    }

    #[test]
    fn a_looks_picture_key_names_the_setup_and_every_change() {
        let od = ObjDesc {
            subpalettes: vec![dereth_animation::parts::PaletteRange {
                palette_set: DataId(0x0400_0001),
                offset: 0,
                length: 0,
            }],
            ..ObjDesc::default()
        };
        let other = ObjDesc {
            subpalettes: vec![dereth_animation::parts::PaletteRange {
                palette_set: DataId(0x0400_0002),
                offset: 0,
                length: 0,
            }],
            ..ObjDesc::default()
        };
        let a = look_key(DataId(0x0200_0001), &od);
        assert_eq!(a, look_key(DataId(0x0200_0001), &od.clone()));
        assert_ne!(a, look_key(DataId(0x0200_0002), &od));
        assert_ne!(a, look_key(DataId(0x0200_0001), &other));
    }
}
