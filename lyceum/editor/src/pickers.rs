//! The weenie editor's Model and Icon pickers: a button showing the weenie's picture and id, which
//! opens a popout of pictures to choose from. A click on one writes it, as one step, and closes the
//! popout; a click outside it or Escape closes it too, so its search and its id field take keys.
//!
//! The Model picker lists the setups the Place tool's model browser does, by the browser's names,
//! categories, recent and favorite models and search, the weenie's own category first; and offers
//! the model chosen in Place. The Icon picker lists the icons weenies use, those of weenies of the
//! weenie's own type first, each named by the weenies that use it; and with Show all, every other
//! icon the files hold, by id. The icons are read and listed on threads of their own, as the
//! model browser's pictures are drawn. Either takes an id typed in too.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, TextureFormat};
use dereth_scene::textures::TextureStore;
use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::models::Category;
use lyceum_foundry::sql_ids;

use super::form::FormEdit;
use super::{EditorApp, UiAction};
use crate::browser_state::Shelf;

/// Which picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Picker {
    Model,
    Icon,
    /// A base palette, a sound table and a motion table: each from a list.
    Palette,
    Sound,
    Motion,
}

impl Picker {
    /// The data id property it writes.
    #[must_use]
    pub const fn property(self) -> u32 {
        match self {
            Self::Model => 1,
            Self::Icon => 8,
            Self::Palette => 6,
            Self::Sound => 3,
            Self::Motion => 2,
        }
    }

    /// The first byte of the ids of the files a list picker chooses from.
    #[must_use]
    pub const fn file_kind(self) -> Option<u32> {
        match self {
            Self::Palette => Some(0x04),
            Self::Sound => Some(0x20),
            Self::Motion => Some(0x09),
            Self::Model | Self::Icon => None,
        }
    }

    /// The table the weenies list of `e` use for it.
    fn of_weenie(self, e: &super::WeenieEntry) -> Option<u32> {
        match self {
            Self::Palette => e.palette_base,
            Self::Sound => e.sound_table,
            Self::Motion => e.motion_table,
            Self::Model => e.setup,
            Self::Icon => e.icon,
        }
    }
}

/// An icon's side, in pixels: the size the files keep icons at.
pub const ICON: usize = 32;

/// The most bytes an icon takes in the files: a texture larger than that is no icon.
const ICON_BYTES: u32 = 8 * 1024;

/// How many icons are asked of the worker at most before it has answered.
const IN_FLIGHT: usize = 64;

/// The popout's width, the button's size, and the size of a model's and an icon's cell.
const POPOUT_WIDTH: f32 = 430.0;
const BUTTON: egui::Vec2 = egui::vec2(150.0, 38.0);
const MODEL_CELL: egui::Vec2 = egui::vec2(78.0, 76.0);
const ICON_CELL: egui::Vec2 = egui::vec2(38.0, 38.0);

/// How tall the popout's pictures stand at most, in points.
const GRID_HEIGHT: f32 = 330.0;

/// A palette's strip: how wide it is, and how many of its colors it shows.
const STRIP_WIDTH: f32 = 64.0;
const STRIP_COLORS: usize = 8;

/// The pickers' state: what is typed and chosen in the one open, and where its parts were drawn
/// last.
#[derive(Debug, Clone, Default)]
pub struct Pickers {
    /// The picker open, as drawn last.
    pub open: Option<Picker>,
    pub search: String,
    /// The id typed into its id field.
    pub typed: String,
    /// The Model picker's shelf, and its category (every one when none).
    pub shelf: Shelf,
    pub category: Option<Category>,
    /// Whether the Icon picker shows every icon, not only those weenies use.
    pub show_all: bool,
    /// The picker just opened: it shows the weenie's own choice, and its search takes the keys.
    just_opened: bool,
    /// As drawn last: each picker's button; the open one's search, id field, Show all, the model
    /// chosen in Place, its shelves and categories, and each of its pictures by id.
    pub buttons: Vec<(Picker, egui::Rect)>,
    pub search_field: Option<egui::Rect>,
    pub id_field: Option<egui::Rect>,
    pub show_all_button: Option<egui::Rect>,
    pub place_button: Option<egui::Rect>,
    pub shelf_rects: Vec<(Shelf, egui::Rect)>,
    pub category_rects: Vec<(Option<Category>, egui::Rect)>,
    pub cells: Vec<(u32, egui::Rect)>,
}

impl Pickers {
    /// What was drawn last let go, before a frame draws it again.
    pub fn clear_drawn(&mut self) {
        self.buttons.clear();
        self.search_field = None;
        self.id_field = None;
        self.show_all_button = None;
        self.place_button = None;
        self.shelf_rects.clear();
        self.category_rects.clear();
        self.cells.clear();
    }

    /// The button's rectangle of picker `p`, as drawn last.
    #[must_use]
    pub fn button(&self, p: Picker) -> Option<egui::Rect> {
        self.buttons.iter().find(|(q, _)| *q == p).map(|(_, r)| *r)
    }

    /// Picker `p` opened: nothing typed in it yet.
    fn opened(&mut self, p: Picker) {
        self.open = Some(p);
        self.search.clear();
        self.typed.clear();
        self.just_opened = true;
    }
}

/// An icon weenies use: its id, the names of the weenies that use it (the commonest first), and
/// their weenie types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedIcon {
    pub id: u32,
    pub names: Vec<String>,
    pub types: BTreeSet<u32>,
    /// Its names in lower case, one to a line: what the search reads.
    words: String,
}

impl UsedIcon {
    /// What it is called: the name most of its weenies have, and how many other names they have.
    #[must_use]
    pub fn label(&self) -> String {
        match self.names.as_slice() {
            [] => format!("0x{:08X}", self.id),
            [one] => one.clone(),
            [first, rest @ ..] => format!("{first} +{}", rest.len()),
        }
    }
}

/// A table weenies use (a base palette, a sound table or a motion table): its id, the names of
/// the weenies that use it with how many have each (the commonest first), how many use it, and
/// the models they are drawn with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedTable {
    pub id: u32,
    pub names: Vec<(String, usize)>,
    pub count: usize,
    pub setups: BTreeSet<u32>,
    /// Its names in lower case, one to a line: what the search reads.
    words: String,
}

/// The tables `weenies` use (each weenie's table, name and model), by id. A table of 0 is none.
#[must_use]
pub fn used_tables<'a>(
    weenies: impl IntoIterator<Item = (u32, &'a str, Option<u32>)>,
) -> Vec<UsedTable> {
    let mut by: BTreeMap<u32, (BTreeMap<&str, usize>, BTreeSet<u32>)> = BTreeMap::new();
    for (table, name, setup) in weenies {
        if table == 0 {
            continue;
        }
        let e = by.entry(table).or_default();
        *e.0.entry(name).or_default() += 1;
        e.1.extend(setup);
    }
    by.into_iter()
        .map(|(id, (names, setups))| {
            let mut names: Vec<(String, usize)> =
                names.into_iter().map(|(n, c)| (n.to_owned(), c)).collect();
            names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let count = names.iter().map(|(_, c)| c).sum();
            let words = names
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(
                    "
",
                )
                .to_lowercase();
            UsedTable {
                id,
                names,
                count,
                setups,
                words,
            }
        })
        .collect()
}

/// A list picker's tables, in order: those of `used` answering `search` (any part of an id in
/// hexadecimal, or of a weenie's name) that weenies drawn with model `setup` use first, then the
/// rest of them, the most used first in each; then every other of the files' (`all`) answering it
/// by id, by id.
#[must_use]
pub fn table_list(used: &[UsedTable], all: &[u32], setup: Option<u32>, search: &str) -> Vec<u32> {
    let f = search.trim().to_lowercase();
    let hits = |u: &&UsedTable| f.is_empty() || id_holds(u.id, &f) || u.words.contains(&f);
    let mine = |u: &UsedTable| setup.is_some_and(|s| u.setups.contains(&s));
    let mut listed: Vec<&UsedTable> = used.iter().filter(hits).collect();
    listed.sort_by(|a, b| {
        mine(b)
            .cmp(&mine(a))
            .then(b.count.cmp(&a.count))
            .then(a.id.cmp(&b.id))
    });
    let mut out: Vec<u32> = listed.into_iter().map(|u| u.id).collect();
    let used_ids: BTreeSet<u32> = used.iter().map(|u| u.id).collect();
    out.extend(
        all.iter()
            .copied()
            .filter(|id| !used_ids.contains(id) && (f.is_empty() || id_holds(*id, &f))),
    );
    out
}

/// A list picker's index: the tables weenies use and every one of the kind the files hold, made
/// once, again only when the weenies listed change in number.
#[derive(Debug, Default)]
pub struct TableIndex {
    pub used: Vec<UsedTable>,
    pub all: Vec<u32>,
    /// How many weenies it was made from.
    from: Option<usize>,
}

impl TableIndex {
    /// Table `id`'s entry among those weenies use.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&UsedTable> {
        self.used
            .binary_search_by_key(&id, |u| u.id)
            .ok()
            .map(|i| &self.used[i])
    }
}

/// The icons `weenies` use (each weenie's icon, name and weenie type), by id. An icon of 0 is
/// none.
#[must_use]
pub fn used_icons<'a>(
    weenies: impl IntoIterator<Item = (u32, &'a str, Option<u32>)>,
) -> Vec<UsedIcon> {
    let mut by: BTreeMap<u32, (BTreeMap<&str, usize>, BTreeSet<u32>)> = BTreeMap::new();
    for (icon, name, ty) in weenies {
        if icon == 0 {
            continue;
        }
        let e = by.entry(icon).or_default();
        *e.0.entry(name).or_default() += 1;
        e.1.extend(ty);
    }
    by.into_iter()
        .map(|(id, (names, types))| {
            let mut names: Vec<(&str, usize)> = names.into_iter().collect();
            names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            let names: Vec<String> = names.into_iter().map(|(n, _)| n.to_owned()).collect();
            let words = names.join("\n").to_lowercase();
            UsedIcon {
                id,
                names,
                types,
                words,
            }
        })
        .collect()
}

/// Whether id `id` answers search `f` (trimmed, in lower case): only a search of hexadecimal
/// digits finds an id, any part of it.
fn id_holds(id: u32, f: &str) -> bool {
    let digits = f.trim_start_matches("0x");
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_hexdigit())
        && format!("{id:08x}").contains(digits)
}

/// The Icon picker's icons, in order: those of `used` answering `search` (any part of an id in
/// hexadecimal, or of a weenie's name), those weenies of type `ty` use first, then the rest of
/// them, each by id; then, with Show all (`all`, every icon the files hold), every other icon
/// answering it by id.
#[must_use]
pub fn icon_list(
    used: &[UsedIcon],
    all: Option<&[u32]>,
    ty: Option<u32>,
    search: &str,
) -> Vec<u32> {
    let f = search.trim().to_lowercase();
    let hits = |u: &&UsedIcon| f.is_empty() || id_holds(u.id, &f) || u.words.contains(&f);
    let mine = |u: &&UsedIcon| ty.is_some_and(|t| u.types.contains(&t));
    let mut out: Vec<u32> = used
        .iter()
        .filter(hits)
        .filter(mine)
        .map(|u| u.id)
        .collect();
    out.extend(used.iter().filter(hits).filter(|u| !mine(u)).map(|u| u.id));
    if let Some(all) = all {
        let listed: BTreeSet<u32> = used.iter().map(|u| u.id).collect();
        out.extend(
            all.iter()
                .copied()
                .filter(|id| !listed.contains(id) && (f.is_empty() || id_holds(*id, &f))),
        );
    }
    out
}

/// The icons the pickers show: read on a thread of their own, held as the panels' textures; the
/// icons weenies use; and every icon the files hold, once listed.
#[derive(Default)]
pub struct Icons {
    worker: Option<Worker>,
    listing: Option<Listing>,
    all: Option<Vec<u32>>,
    textures: HashMap<u32, egui::TextureHandle>,
    failed: BTreeSet<u32>,
    used: Vec<UsedIcon>,
    /// How many weenies the icons used were gathered from.
    used_from: Option<usize>,
    /// The list pickers' indexes, by picker.
    tables: HashMap<Picker, TableIndex>,
}

impl std::fmt::Debug for Icons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icons")
            .field("textures", &self.textures.len())
            .field("used", &self.used.len())
            .field("all", &self.all.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

/// The thread that reads icons, each as [`ICON`] pixels square, red, green, blue and alpha.
struct Worker {
    asks: Option<mpsc::Sender<u32>>,
    answers: mpsc::Receiver<(u32, Option<Vec<u8>>)>,
    thread: Option<std::thread::JoinHandle<()>>,
    asked: BTreeSet<u32>,
}

impl Worker {
    fn open(store: Arc<RetailDatStore>) -> Self {
        let (asks, asked) = mpsc::channel::<u32>();
        let (answer, answers) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("lyceum icons".into())
            .spawn(move || {
                let textures = TextureStore::new(&store);
                for id in asked {
                    if answer.send((id, icon_pixels(&textures, id))).is_err() {
                        return;
                    }
                }
            })
            .ok();
        Self {
            asks: Some(asks),
            answers,
            thread,
            asked: BTreeSet::new(),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // It ends after the icon it is on once nothing more can be asked of it.
        self.asks = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The thread that lists every icon the files hold.
struct Listing {
    found: mpsc::Receiver<Vec<u32>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Listing {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Icon `id` as [`ICON`] pixels square, red, green, blue and alpha: the icon's own picture, as
/// an item's icon is drawn, nearest pixels taken for one of another size. `None` for one that
/// will not read.
fn icon_pixels(textures: &TextureStore<'_>, id: u32) -> Option<Vec<u8>> {
    let data = textures.icon_data(DataId(id)).ok()?;
    let (w, h, bgra) = if data.format == TextureFormat::Bgra8 {
        (
            data.width as usize,
            data.height as usize,
            data.levels.into_iter().next()?,
        )
    } else {
        let img = textures.bgra8(DataId(id)).ok()?;
        (
            img.width as usize,
            img.height as usize,
            img.pixels.into_iter().flatten().collect(),
        )
    };
    if w == 0 || h == 0 || bgra.len() < w * h * 4 {
        return None;
    }
    let mut rgba = vec![0u8; ICON * ICON * 4];
    for y in 0..ICON {
        for x in 0..ICON {
            let i = ((y * h / ICON) * w + x * w / ICON) * 4;
            let o = (y * ICON + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i], bgra[i + 3]]);
        }
    }
    Some(rgba)
}

/// Every icon `store`'s portal file holds, by id: each small texture [`ICON`] pixels square.
/// Cut short, with what it found so far, once `stop` is set.
fn every_icon_in(store: &RetailDatStore, stop: &AtomicBool) -> Vec<u32> {
    let textures = TextureStore::new(store);
    let small: Vec<u32> = store
        .portal()
        .iter_entries()
        .filter(|(id, e)| id.raw() >> 24 == 0x06 && e.size <= ICON_BYTES)
        .map(|(id, _)| id.raw())
        .collect();
    let mut out = Vec::new();
    for id in small {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Ok((_, rs, _)) = textures.resolve(DataId(id)) {
            if rs.width as usize == ICON && rs.height as usize == ICON {
                out.push(id);
            }
        }
    }
    out.sort_unstable();
    out
}

/// Every file of kind `kind` (its ids' first byte) `store`'s portal file holds, by id.
fn files_of_kind(store: &RetailDatStore, kind: u32) -> Vec<u32> {
    let mut out: Vec<u32> = store
        .portal()
        .iter_entries()
        .map(|(id, _)| id.raw())
        .filter(|id| id >> 24 == kind)
        .collect();
    out.sort_unstable();
    out
}

/// What a weenie of `doc` is, as the server numbers its weenie types.
fn weenie_type(doc: &SqlDoc) -> Option<u32> {
    doc.rows("weenie")
        .first()
        .and_then(|r| doc.cell(*r, "type"))
        .and_then(|v| sql_ids::number(&v))
}

/// A cell of a popout's grid: a picture `side` points square (or an empty slot) at its top, a
/// caption under it when it has one; lit under the pointer, and marked when it is the weenie's
/// own.
pub(super) fn cell(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    side: f32,
    picture: Option<egui::TextureId>,
    caption: Option<&str>,
    own: bool,
) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    if own || r.hovered() {
        painter.rect_filled(
            rect,
            3.0,
            if own {
                egui::Color32::from_rgb(40, 60, 90)
            } else {
                egui::Color32::from_gray(45)
            },
        );
    }
    if own {
        painter.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(110, 160, 230)),
            egui::StrokeKind::Inside,
        );
    }
    let at = egui::Rect::from_min_size(
        egui::pos2(
            rect.center().x - side / 2.0,
            rect.min.y + (size.x - side).min(6.0) / 2.0,
        ),
        egui::vec2(side, side),
    );
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
    if let Some(words) = caption {
        let galley = painter.layout(
            words.to_owned(),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(210),
            size.x - 2.0,
        );
        let x = rect.center().x - galley.size().x.min(size.x) / 2.0;
        painter.galley(egui::pos2(x, at.max.y + 1.0), galley, egui::Color32::WHITE);
    }
    r
}

impl EditorApp {
    /// The picker of weenie `w`'s model or icon as `doc` has it: a button with its picture and
    /// id, and the popout it opens.
    pub(super) fn picker(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        which: Picker,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let now = doc
            .property(PropertyKind::DataId, which.property())
            .and_then(|v| sql_ids::number(&content::bare(&v)));
        let (rect, _) = ui.allocate_exact_size(BUTTON, egui::Sense::hover());
        let r = ui.interact(
            rect,
            egui::Id::new(("weenie picker", which, w.clone())),
            egui::Sense::click(),
        );
        let mut budget = super::PICTURES_A_FRAME;
        let picture = now.and_then(|n| self.picker_picture(which, n, &mut budget));
        let words = now.map_or_else(|| t.get("fields.none").to_owned(), |n| format!("0x{n:08X}"));
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
        let at = egui::Rect::from_min_size(rect.min + egui::vec2(3.0, 3.0), egui::vec2(side, side));
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
        painter.text(
            egui::pos2(at.max.x + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            words,
            egui::FontId::monospace(12.0),
            v.text_color(),
        );
        let about = match which {
            Picker::Model => now
                .and_then(|n| self.names.label(DataId(n)))
                .unwrap_or_else(|| t.get("pickers.choose_model").to_owned()),
            _ => t.get("pickers.choose_icon").to_owned(),
        };
        let r = r.on_hover_text(about);
        self.editor.pickers.buttons.push((which, rect));
        let key = super::fields::property_key(PropertyKind::DataId, which.property());
        self.form_fields.push((key, rect));
        if r.clicked() && !egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&r)) {
            self.editor.pickers.opened(which);
        }
        let shown = egui::Popup::from_toggle_button_response(&r)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(POPOUT_WIDTH)
            .show(|ui| {
                ui.set_width(POPOUT_WIDTH);
                match which {
                    Picker::Model => self.model_popout(ui, w, now, out),
                    _ => self.icon_popout(ui, w, doc, now, out),
                }
                self.editor.pickers.just_opened = false;
            });
        let p = &mut self.editor.pickers;
        if shown.is_some() {
            p.open = Some(which);
        } else if p.open == Some(which) {
            p.open = None;
        }
    }

    /// The picture a picker shows of `id`: a model's, drawn as the model browser draws it while
    /// `budget` lasts, or an icon.
    fn picker_picture(
        &mut self,
        which: Picker,
        id: u32,
        budget: &mut u32,
    ) -> Option<egui::TextureId> {
        match which {
            Picker::Model => self.picture(DataId(id), budget),
            _ => self.icon_texture(id),
        }
    }

    /// `id` chosen in picker `which` of weenie `w`, whose own is `now`: written as one step, and
    /// the popout closed.
    fn choose_in(
        &self,
        ui: &egui::Ui,
        w: &Weenie,
        which: Picker,
        now: Option<u32>,
        id: u32,
        out: &mut Vec<UiAction>,
    ) {
        if now != Some(id) {
            out.push(UiAction::Form(
                w.clone(),
                FormEdit::Property {
                    kind: PropertyKind::DataId,
                    id: which.property(),
                    value: Some(format!("0x{id:08X}")),
                },
            ));
        }
        ui.close();
    }

    /// A popout's search, which takes the keys when the popout opens.
    fn picker_search(&mut self, ui: &mut egui::Ui) {
        let t = self.text.table();
        let p = &mut self.editor.pickers;
        let r = super::panels::search_box(ui, &mut p.search, t.get("pickers.search"));
        p.search_field = Some(r.rect);
        if p.just_opened {
            r.request_focus();
        }
    }

    /// A popout's id field: an id typed and Enter chose it.
    fn picker_id(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        which: Picker,
        now: Option<u32>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let hint = now.map_or_else(String::new, |n| format!("0x{n:08X}"));
        let typed = ui
            .horizontal(|ui| {
                ui.label(t.get("pickers.id"));
                let p = &mut self.editor.pickers;
                let r = ui.add(
                    egui::TextEdit::singleline(&mut p.typed)
                        .hint_text(hint)
                        .desired_width(110.0),
                );
                p.id_field = Some(r.rect);
                let entered = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                entered.then(|| sql_ids::number(p.typed.trim())).flatten()
            })
            .inner;
        if let Some(id) = typed {
            self.choose_in(ui, w, which, now, id, out);
        }
    }

    /// A popout's pictures, `cell` points each, `side` points of it the picture, those of `list`
    /// in rows across its width: the weenie's own (`now`) marked, and scrolled to when the
    /// popout opens. The one clicked.
    fn picker_grid(
        &mut self,
        ui: &mut egui::Ui,
        which: Picker,
        list: &[u32],
        now: Option<u32>,
        (cell_size, side): (egui::Vec2, f32),
    ) -> Option<u32> {
        let t = self.text.table();
        if list.is_empty() {
            ui.weak(t.get("pickers.no_match"));
            return None;
        }
        let gap = 4.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few columns
        let columns =
            (((ui.available_width() + gap) / (cell_size.x + gap)).floor() as usize).max(1);
        let rows = list.len().div_ceil(columns);
        let row_height = cell_size.y + gap;
        let mut area = egui::ScrollArea::vertical()
            .id_salt(("picker", which))
            .max_height(GRID_HEIGHT)
            .min_scrolled_height(GRID_HEIGHT)
            .auto_shrink([false, true]);
        if self.editor.pickers.just_opened {
            if let Some(i) = list.iter().position(|id| Some(*id) == now) {
                #[allow(clippy::cast_precision_loss)] // a row's place
                let at = (i / columns) as f32 * row_height;
                area = area
                    .vertical_scroll_offset((at - GRID_HEIGHT / 2.0 + row_height / 2.0).max(0.0));
            } else {
                area = area.vertical_scroll_offset(0.0);
            }
        }
        let mut budget = super::PICTURES_A_FRAME;
        let mut chosen = None;
        area.show_rows(ui, cell_size.y, rows, |ui, shown| {
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            for row in shown {
                ui.horizontal(|ui| {
                    for id in list.iter().skip(row * columns).take(columns) {
                        let picture = self.picker_picture(which, *id, &mut budget);
                        let (caption, about) = match which {
                            Picker::Model => {
                                let label = self.names.label(DataId(*id));
                                let caption = label.clone().unwrap_or_else(|| format!("{id:08X}"));
                                let c = self.names.category(DataId(*id)).name();
                                let kind = t.or(&format!("categories.{c}"), c).to_owned();
                                let about = format!(
                                    "{}\n0x{id:08X}\n{kind}",
                                    label.as_deref().unwrap_or(t.get("models.unused"))
                                );
                                (Some(caption), about)
                            }
                            _ => {
                                let about = match self.icons.used.binary_search_by_key(id, |u| u.id)
                                {
                                    Ok(i) => {
                                        let u = &self.icons.used[i];
                                        format!("{}\n0x{id:08X}", u.label())
                                    }
                                    Err(_) => format!("{}\n0x{id:08X}", t.get("pickers.unused")),
                                };
                                (None, about)
                            }
                        };
                        let r = cell(
                            ui,
                            cell_size,
                            side,
                            picture,
                            caption.as_deref(),
                            Some(*id) == now,
                        );
                        self.editor.pickers.cells.push((*id, r.rect));
                        if r.on_hover_text(about).clicked() {
                            chosen = Some(*id);
                        }
                    }
                });
            }
        });
        chosen
    }

    /// The Model picker's popout: its shelves, its search, its categories, the model chosen in
    /// Place, its pictures and its id field.
    fn model_popout(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        now: Option<u32>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        ui.horizontal(|ui| {
            for (shelf, key) in [
                (Shelf::All, "models.all"),
                (Shelf::Recent, "models.recent"),
                (Shelf::Favorites, "models.favorites"),
            ] {
                let p = &mut self.editor.pickers;
                let r = ui.selectable_label(p.shelf == shelf, t.get(key));
                p.shelf_rects.push((shelf, r.rect));
                if r.clicked() {
                    p.shelf = shelf;
                }
            }
        });
        self.picker_search(ui);
        let counts = self.setup_counts();
        ui.horizontal_wrapped(|ui| {
            let p = &mut self.editor.pickers;
            let r = ui.selectable_label(p.category.is_none(), t.get("pickers.every_kind"));
            p.category_rects.push((None, r.rect));
            if r.clicked() {
                p.category = None;
            }
            for (c, n) in &counts {
                let name = t.or(&format!("categories.{}", c.name()), c.name());
                let r = ui.selectable_label(p.category == Some(*c), format!("{name} {n}"));
                p.category_rects.push((Some(*c), r.rect));
                if r.clicked() {
                    p.category = (p.category != Some(*c)).then_some(*c);
                }
            }
        });
        // The model chosen in Place, when it is a setup.
        if let Some(m) = self.model.filter(|m| m.raw() >> 24 == 0x02) {
            let mut budget = super::PICTURES_A_FRAME;
            let picture = self.picture(m, &mut budget);
            let clicked = ui
                .horizontal(|ui| {
                    match picture {
                        Some(id) => {
                            ui.add(egui::Image::new((id, egui::vec2(24.0, 24.0))));
                        }
                        None => {
                            let _ = ui
                                .allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                        }
                    }
                    let label = self
                        .names
                        .label(m)
                        .unwrap_or_else(|| format!("0x{:08X}", m.raw()));
                    let b = ui
                        .button(t.get("pickers.place_model"))
                        .on_hover_text(format!("{label}\n0x{:08X}", m.raw()));
                    self.editor.pickers.place_button = Some(b.rect);
                    b.clicked()
                })
                .inner;
            if clicked {
                self.choose_in(ui, w, Picker::Model, now, m.raw(), out);
            }
        }
        ui.separator();
        let first = self.weenie_entry(w).map(|e| e.category);
        let list = self.model_list(first);
        let side = crate::thumbs::SIZE as f32;
        if let Some(id) = self.picker_grid(ui, Picker::Model, &list, now, (MODEL_CELL, side)) {
            self.choose_in(ui, w, Picker::Model, now, id, out);
        }
        ui.separator();
        self.picker_id(ui, w, Picker::Model, now, out);
    }

    /// How many setups each category holds, those holding some.
    fn setup_counts(&self) -> BTreeMap<Category, usize> {
        let mut out: BTreeMap<Category, usize> = BTreeMap::new();
        for m in self.all_models.iter().filter(|m| m.raw() >> 24 == 0x02) {
            *out.entry(self.names.category(*m)).or_default() += 1;
        }
        out
    }

    /// The Model picker's setups, in order: those on its shelf in its category answering its
    /// search, as the model browser finds them; the recent ones newest first, else those whose own
    /// name holds the search first and those of category `first` (the weenie's) before the rest,
    /// each by id.
    fn model_list(&self, first: Option<Category>) -> Vec<u32> {
        let p = &self.editor.pickers;
        let f = p.search.trim().to_lowercase();
        let recent = &self.browser_state.recent;
        let mut list: Vec<DataId> = self
            .all_models
            .iter()
            .copied()
            .filter(|m| m.raw() >> 24 == 0x02)
            .filter(|m| p.category.is_none_or(|c| self.names.category(*m) == c))
            .filter(|m| match p.shelf {
                Shelf::All => true,
                Shelf::Recent => recent.contains(&m.raw()),
                Shelf::Favorites => self.browser_state.favorites.contains(&m.raw()),
            })
            .filter(|m| self.names.matches_lower(*m, &f, self.tags.of(*m)))
            .collect();
        if p.shelf == Shelf::Recent {
            list.sort_by_key(|m| recent.iter().position(|id| *id == m.raw()));
        } else {
            list.sort_by_key(|m| {
                (
                    !(f.is_empty() || self.names.label_holds(*m, &f)),
                    first.is_some_and(|c| self.names.category(*m) != c),
                )
            });
        }
        list.into_iter().map(|m| m.raw()).collect()
    }

    /// The Icon picker's popout: its search, Show all, its icons and its id field.
    fn icon_popout(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        now: Option<u32>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        self.gather_used_icons();
        self.picker_search(ui);
        ui.horizontal(|ui| {
            let p = &mut self.editor.pickers;
            let r = ui.checkbox(&mut p.show_all, t.get("pickers.show_all"));
            p.show_all_button = Some(r.rect);
            if p.show_all && self.icons.all.is_none() {
                ui.weak(t.get("pickers.listing"));
            }
        });
        if self.editor.pickers.show_all {
            self.list_every_icon();
        }
        let all = if self.editor.pickers.show_all {
            self.icons.all.as_deref()
        } else {
            None
        };
        let list = icon_list(
            &self.icons.used,
            all,
            weenie_type(doc),
            &self.editor.pickers.search,
        );
        ui.separator();
        if let Some(id) = self.picker_grid(ui, Picker::Icon, &list, now, (ICON_CELL, ICON as f32)) {
            self.choose_in(ui, w, Picker::Icon, now, id, out);
        }
        ui.separator();
        self.picker_id(ui, w, Picker::Icon, now, out);
        if self.icons.listing.is_some()
            || self
                .icons
                .worker
                .as_ref()
                .is_some_and(|k| !k.asked.is_empty())
        {
            ui.ctx().request_repaint();
        }
    }

    /// The list picker of weenie `w`'s base palette, sound table or motion table as `doc` has
    /// it: a button with its id and the weenies that use it, and the popout it opens.
    pub(super) fn table_picker(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        which: Picker,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let now = doc
            .property(PropertyKind::DataId, which.property())
            .and_then(|v| sql_ids::number(&content::bare(&v)));
        self.table_index(which);
        let words = now.map_or_else(
            || t.get("fields.none").to_owned(),
            |n| format!("0x{n:08X}  {}", self.table_label(which, n)),
        );
        let r = ui
            .horizontal(|ui| {
                if let (Picker::Palette, Some(n)) = (which, now) {
                    self.palette_strip(ui, n);
                }
                ui.add(egui::Button::new(words).truncate())
                    .on_hover_text(t.get(match which {
                        Picker::Palette => "pickers.choose_palette",
                        Picker::Sound => "pickers.choose_sound",
                        _ => "pickers.choose_motion",
                    }))
            })
            .inner;
        self.editor.pickers.buttons.push((which, r.rect));
        let key = super::fields::property_key(PropertyKind::DataId, which.property());
        self.form_fields.push((key, r.rect));
        if r.clicked() && !egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&r)) {
            self.editor.pickers.opened(which);
        }
        let setup = doc
            .property(PropertyKind::DataId, 1)
            .and_then(|v| sql_ids::number(&content::bare(&v)));
        let shown = egui::Popup::from_toggle_button_response(&r)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(POPOUT_WIDTH)
            .show(|ui| {
                ui.set_width(POPOUT_WIDTH);
                self.table_popout(ui, w, which, setup, now, out);
                self.editor.pickers.just_opened = false;
            });
        let p = &mut self.editor.pickers;
        if shown.is_some() {
            p.open = Some(which);
        } else if p.open == Some(which) {
            p.open = None;
        }
    }

    /// A list picker's popout: its search, its tables (those weenies drawn with model `setup` use
    /// first, in strong words), and its id field.
    fn table_popout(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        which: Picker,
        setup: Option<u32>,
        now: Option<u32>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        self.picker_search(ui);
        ui.separator();
        let search = self.editor.pickers.search.clone();
        let index = self.table_index(which);
        let list = table_list(&index.used, &index.all, setup, &search);
        if list.is_empty() {
            ui.weak(t.get("pickers.no_match"));
        }
        let row_height = ui.spacing().interact_size.y;
        let mut area = egui::ScrollArea::vertical()
            .id_salt(("picker", which))
            .max_height(GRID_HEIGHT)
            .min_scrolled_height(GRID_HEIGHT)
            .auto_shrink([false, true]);
        if self.editor.pickers.just_opened {
            let at = list
                .iter()
                .position(|id| Some(*id) == now)
                .map_or(0.0, |i| {
                    #[allow(clippy::cast_precision_loss)] // a row's place
                    let y = i as f32 * (row_height + ui.spacing().item_spacing.y);
                    (y - GRID_HEIGHT / 2.0).max(0.0)
                });
            area = area.vertical_scroll_offset(at);
        }
        let mut chosen = None;
        area.show_rows(ui, row_height, list.len(), |ui, rows| {
            for id in &list[rows] {
                let mine = setup.is_some_and(|s| {
                    self.icons
                        .tables
                        .get(&which)
                        .and_then(|x| x.entry(*id))
                        .is_some_and(|u| u.setups.contains(&s))
                });
                let label = self.table_label(which, *id);
                let r = ui
                    .horizontal(|ui| {
                        if which == Picker::Palette {
                            self.palette_strip(ui, *id);
                        }
                        let words = egui::RichText::new(format!("0x{id:08X}  {label}"));
                        let words = if mine { words.strong() } else { words };
                        ui.add(
                            egui::Button::selectable(Some(*id) == now, words)
                                .truncate()
                                .min_size(egui::vec2(ui.available_width(), 0.0)),
                        )
                    })
                    .inner;
                self.editor.pickers.cells.push((*id, r.rect));
                if r.clicked() {
                    chosen = Some(*id);
                }
            }
        });
        if let Some(id) = chosen {
            self.choose_in(ui, w, which, now, id, out);
        }
        ui.separator();
        self.picker_id(ui, w, which, now, out);
    }

    /// Table `id` of list picker `which` in words: the name most of the weenies that use it have
    /// and how many more use it, or that none does.
    fn table_label(&self, which: Picker, id: u32) -> String {
        let t = self.text.table();
        let Some(u) = self.icons.tables.get(&which).and_then(|x| x.entry(id)) else {
            return t.get("pickers.unused").to_owned();
        };
        match u.names.first() {
            None => t.get("pickers.unused").to_owned(),
            Some((name, n)) if *n == u.count => name.clone(),
            Some((name, n)) => t.fill(
                "pickers.used_more",
                &[("name", name), ("n", &(u.count - n))],
            ),
        }
    }

    /// List picker `which`'s index: made once, and again when the weenies listed change in
    /// number.
    fn table_index(&mut self, which: Picker) -> &TableIndex {
        let n = self.weenies.len();
        let stale = self
            .icons
            .tables
            .get(&which)
            .is_none_or(|x| x.from != Some(n));
        if stale {
            let used = used_tables(
                self.weenies
                    .iter()
                    .filter_map(|e| Some((which.of_weenie(e)?, e.name.as_str(), e.setup))),
            );
            let all = match (self.icons.tables.remove(&which), which.file_kind()) {
                (Some(old), _) if !old.all.is_empty() => old.all,
                (_, Some(kind)) => files_of_kind(&self.base_store, kind),
                (_, None) => Vec::new(),
            };
            self.icons.tables.insert(
                which,
                TableIndex {
                    used,
                    all,
                    from: Some(n),
                },
            );
        }
        &self.icons.tables[&which]
    }

    /// A few of palette `id`'s colors side by side, from its first to its last.
    fn palette_strip(&mut self, ui: &mut egui::Ui, id: u32) {
        let colors = self.look_files().palette(id);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(STRIP_WIDTH, ui.spacing().interact_size.y - 4.0),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        let Some(colors) = colors.filter(|c| !c.is_empty()) else {
            painter.rect_filled(rect, 2.0, egui::Color32::from_gray(20));
            return;
        };
        #[allow(clippy::cast_precision_loss)] // a few swatches
        let w = rect.width() / STRIP_COLORS as f32;
        for i in 0..STRIP_COLORS {
            let [_, r, g, b] = colors[i * colors.len() / STRIP_COLORS].to_be_bytes();
            #[allow(clippy::cast_precision_loss)] // a few swatches
            let x = rect.min.x + i as f32 * w;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, rect.min.y), egui::vec2(w, rect.height())),
                0.0,
                egui::Color32::from_rgb(r, g, b),
            );
        }
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
    }

    /// The icons weenies use, gathered again when the weenies listed change in number.
    fn gather_used_icons(&mut self) {
        if self.icons.used_from == Some(self.weenies.len()) {
            return;
        }
        self.icons.used = used_icons(
            self.weenies
                .iter()
                .filter_map(|e| Some((e.icon?, e.name.as_str(), e.weenie_type))),
        );
        self.icons.used_from = Some(self.weenies.len());
    }

    /// Every icon the files hold listed off the frame, once.
    fn list_every_icon(&mut self) {
        self.take_listing(false);
        if self.icons.all.is_some() || self.icons.listing.is_some() {
            return;
        }
        let store = Arc::clone(&self.base_store);
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, found) = mpsc::channel();
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("lyceum icon list".into())
            .spawn(move || {
                let _ = tx.send(every_icon_in(&store, &flag));
            })
            .ok();
        if thread.is_none() {
            self.icons.all = Some(Vec::new());
            return;
        }
        self.icons.listing = Some(Listing {
            found,
            stop,
            thread,
        });
    }

    /// The icon list, once it has come back (waited for when `wait`).
    fn take_listing(&mut self, wait: bool) {
        let Some(l) = &self.icons.listing else { return };
        let found = if wait {
            l.found.recv().ok()
        } else {
            match l.found.try_recv() {
                Ok(f) => Some(f),
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => None,
            }
        };
        self.icons.all = Some(found.unwrap_or_default());
        self.icons.listing = None;
    }

    /// Icon `id` as the panels' texture: held, else asked of the worker and shown once it is
    /// read.
    fn icon_texture(&mut self, id: u32) -> Option<egui::TextureId> {
        if let Some(t) = self.icons.textures.get(&id) {
            return Some(t.id());
        }
        if self.icons.failed.contains(&id) {
            return None;
        }
        self.take_icons(false);
        if let Some(t) = self.icons.textures.get(&id) {
            return Some(t.id());
        }
        let store = &self.base_store;
        let worker = self
            .icons
            .worker
            .get_or_insert_with(|| Worker::open(Arc::clone(store)));
        if !worker.asked.contains(&id) && worker.asked.len() < IN_FLIGHT {
            if let Some(asks) = &worker.asks {
                if asks.send(id).is_ok() {
                    worker.asked.insert(id);
                }
            }
        }
        None
    }

    /// The icons the worker has read, made the panels' textures (each waited for when `wait`).
    fn take_icons(&mut self, wait: bool) {
        let Some(worker) = self.icons.worker.as_mut() else {
            return;
        };
        let mut got = Vec::new();
        while !worker.asked.is_empty() {
            let answer = if wait {
                worker.answers.recv().ok()
            } else {
                worker.answers.try_recv().ok()
            };
            let Some((id, pixels)) = answer else { break };
            worker.asked.remove(&id);
            got.push((id, pixels));
        }
        for (id, pixels) in got {
            match pixels {
                Some(rgba) => {
                    let image = egui::ColorImage::from_rgba_unmultiplied([ICON, ICON], &rgba);
                    let handle = self.gui.ctx.load_texture(
                        format!("icon {id:08X}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.icons.textures.insert(id, handle);
                }
                None => {
                    self.icons.failed.insert(id);
                }
            }
        }
    }

    /// Wait until every icon the pickers have asked for is read, and the list of every icon when
    /// it is being made, so the next frame shows them.
    pub fn wait_for_icons(&mut self) {
        self.take_icons(true);
        self.take_listing(true);
    }

    /// Whether icon `id` has been read and is held as the panels' texture.
    #[must_use]
    pub fn icon_shown(&self, id: u32) -> bool {
        self.icons.textures.contains_key(&id)
    }

    /// How many icons the Icon picker's Show all lists: every icon the files hold, once listed.
    #[must_use]
    pub fn icons_listed(&self) -> Option<usize> {
        self.icons.all.as_ref().map(Vec::len)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's Icon picker)
    use super::*;

    #[test]
    fn a_tables_list_puts_those_used_with_the_weenies_model_first_then_the_most_used_then_the_files(
    ) {
        // Drudges share a motion table; a golem and a statue another; an olthoi its own.
        let drudge = Some(0x0200_0001);
        let golem = Some(0x0200_0002);
        let used = used_tables([
            (0x0900_0001, "Drudge Skulker", drudge),
            (0x0900_0001, "Drudge Skulker", drudge),
            (0x0900_0001, "Drudge Slinker", drudge),
            (0x0900_0002, "Golem", golem),
            (0x0900_0002, "Statue", golem),
            (0x0900_0002, "Golem", golem),
            (0x0900_0002, "Golem", golem),
            (0x0900_0003, "Olthoi", None),
            (0, "Nothing", drudge),
        ]);
        assert_eq!(
            used.iter().map(|u| u.id).collect::<Vec<_>>(),
            [0x0900_0001, 0x0900_0002, 0x0900_0003],
            "a table of 0 is none"
        );
        assert_eq!(used[0].count, 3);
        assert_eq!(
            used[0].names,
            [
                ("Drudge Skulker".to_owned(), 2),
                ("Drudge Slinker".to_owned(), 1)
            ]
        );
        let all = [
            0x0900_0000,
            0x0900_0001,
            0x0900_0002,
            0x0900_0003,
            0x0900_0004,
        ];
        // A drudge's: its model's first, then the most used, then the files' others by id.
        assert_eq!(
            table_list(&used, &all, drudge, ""),
            [
                0x0900_0001,
                0x0900_0002,
                0x0900_0003,
                0x0900_0000,
                0x0900_0004
            ]
        );
        // A golem's: its own first.
        assert_eq!(
            table_list(&used, &all, golem, "")[..2],
            [0x0900_0002, 0x0900_0001]
        );
        // No model: the most used first.
        assert_eq!(table_list(&used, &all, None, "")[0], 0x0900_0002);
        // A search by a weenie's name finds the tables used; by id, any.
        assert_eq!(table_list(&used, &all, drudge, "slinker"), [0x0900_0001]);
        assert_eq!(table_list(&used, &all, drudge, "09000004"), [0x0900_0004]);
    }

    #[test]
    fn the_icons_weenies_use_come_first_their_own_types_first_then_every_other_by_id() {
        // Two swords and a gem share an icon; a chest and a drudge have their own.
        let used = used_icons([
            (0x0600_1003, "Longsword", Some(6)),
            (0x0600_1003, "Longsword", Some(6)),
            (0x0600_1003, "Broadsword", Some(6)),
            (0x0600_1001, "Chest", Some(20)),
            (0x0600_1007, "Drudge", Some(10)),
            (0x0600_1005, "Fire Opal", Some(38)),
            (0, "Nothing", Some(1)),
        ]);
        let ids: Vec<u32> = used.iter().map(|u| u.id).collect();
        assert_eq!(
            ids,
            [0x0600_1001, 0x0600_1003, 0x0600_1005, 0x0600_1007],
            "an icon of 0 is none"
        );
        assert_eq!(
            used[1].names,
            ["Longsword", "Broadsword"],
            "the commonest name first"
        );
        assert_eq!(used[1].label(), "Longsword +1");
        assert_eq!(used[0].label(), "Chest");
        let all = [
            0x0600_1000,
            0x0600_1001,
            0x0600_1002,
            0x0600_1003,
            0x0600_1004,
            0x0600_1005,
            0x0600_1006,
            0x0600_1007,
        ];
        // Used only, without Show all: a gem's own first, then the rest by id.
        assert_eq!(
            icon_list(&used, None, Some(38), ""),
            [0x0600_1005, 0x0600_1001, 0x0600_1003, 0x0600_1007]
        );
        // Show all: the used first, then every other icon by id.
        assert_eq!(
            icon_list(&used, Some(&all), Some(6), ""),
            [
                0x0600_1003,
                0x0600_1001,
                0x0600_1005,
                0x0600_1007,
                0x0600_1000,
                0x0600_1002,
                0x0600_1004,
                0x0600_1006
            ]
        );
        // A search by a weenie's name finds the icons used; by id, any icon.
        assert_eq!(icon_list(&used, Some(&all), None, "SWORD"), [0x0600_1003]);
        assert_eq!(icon_list(&used, Some(&all), None, " broad"), [0x0600_1003]);
        assert_eq!(
            icon_list(&used, Some(&all), None, "0x06001006"),
            [0x0600_1006]
        );
        assert_eq!(icon_list(&used, None, None, "06001006"), Vec::<u32>::new());
        assert_eq!(
            icon_list(&used, Some(&all), Some(20), "100").len(),
            all.len()
        );
    }
}
