//! The map window (a toolbar window): the world map, a closer map of the land around a place, the
//! markers on them, and the overview maps of the regions.
//!
//! [`Navigation`] opens on the world map: the region's map picture fitted to the window, the
//! player's mark where the player stands (off the map indoors), the place's map coordinates, the
//! game's date and time, and the buttons of the five regions' overview maps ([`OverviewMap`]). A
//! click on the world map zooms in to the land around the place clicked: the closer map is
//! 128 land cells a side, made of the detail map pictures covering it, with their labels (each
//! with its own coordinates in its tooltip); the edges' buttons move it nine cells (three with
//! Shift) each time the window hears them, and it hears a click three times; the lock button keeps
//! it on the player as the player moves outdoors, the player's mark takes it to the player, the
//! world map button goes back out (so would a right press, which the player's keys never give the
//! interface).
//!
//! Markers come in kinds: the scripts put markers on the map and take them off (the latest ten of
//! a kind kept; a new one too near one of its kind is not put), the fellows are markers of their
//! own, the labels of the closer map are the fixed kind. A marker shows where it stands on the map
//! it is on (on the world map only a marker meant for it), its tooltip its words and its map
//! coordinates; clicked, its words show at the window's foot.

use std::any::Any;
use std::sync::Arc;

use auberean_text::{StringInfo, Value};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui};
use dereth_primitives::num::{to_i32, to_i32_f64};

use crate::examination::{window_to_front, BRING_TO_FRONT, WINDOW_FIELDS};
use crate::gameplay::{self, Child, Notice, Record, Request, View, PLAY_LAYOUT_ENUM};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;
use crate::social::entry;

/// The scripts' notices the map hears: a marker put on the map (its kind, an instance id, the
/// cell, the picture, the label), one taken off (its kind, an instance id), every one of a kind.
pub mod notice {
    pub use crate::social::fellows::marker::REMOVE as REMOVE_MARKER;
    pub use crate::social::fellows::marker::{ADD as ADD_MARKER, CLEAR as CLEAR_MARKERS};
}

/// The fixed kind of marker: the closer map's labels.
pub const STATIC_MARKER: u32 = 3;

/// A landblock's side in land cells, as a shift; the first cell of a block's interior.
const LBLOCK_SHIFT: u32 = 3;
const FIRST_ENVCELL: u32 = 0x100;

/// The closer map's side, in land cells; how far the edges' buttons move it.
const DETAIL_SIDE: i32 = 0x80;
const STEP: i32 = 9;
const SHIFT_STEP: i32 = 3;

/// Where each of the four detail pictures stands in the closer map, in pictures: across, down.
const QUADRANTS: [(i32, i32); 4] = [(1, 0), (0, 0), (1, 1), (0, 1)];

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("NavigationField");
    pub const FIELD_TOP: Token = Token::of("NavigationFieldTop");
    pub const BUTTON: Token = Token::of("NavigationButton");
    pub const CLOSE_BUTTON: Token = Token::of("NavigationCloseButton");
    pub const HELP_BUTTON: Token = Token::of("NavigationHelpButton");
    pub const COORDINATES_TEXT: Token = Token::of("CoordinatesText");
    pub const PLAYER_MARKER: Token = Token::of("PlayerMarker");
    pub const MAP_FIELD: Token = Token::of("MapField");
    pub const MAP_PARENT_FIELD: Token = Token::of("MapParentField");
    pub const PARENT_FIELD: Token = Token::of("ParentField");
    pub const DETAIL_PANEL: Token = Token::of("DetailMapTemplate");
    pub const MARKER_TEMPLATE: Token = Token::of("MarkerTemplate");
    pub const NOTE_TEXT: Token = Token::of("NoteText");
    pub const TIME_TEXT: Token = Token::of("TimeText");
    pub const DATE_TEXT: Token = Token::of("DateText");
    pub const MACRO_MAP_BUTTON: Token = Token::of("MacroMapButton");
    pub const LOCK_POSITION_BUTTON: Token = Token::of("LockPositionButton");
    pub const UP: Token = Token::of("NavigateUpButton");
    pub const LEFT: Token = Token::of("NavigateLeftButton");
    pub const RIGHT: Token = Token::of("NavigateRightButton");
    pub const DOWN: Token = Token::of("NavigateDownButton");
    pub const OSTETH: Token = Token::of("NavigationZoomToOverviewOstethButton");
    pub const OMISHAN: Token = Token::of("NavigationZoomToOverviewOmishanButton");
    pub const ARRAMORA: Token = Token::of("NavigationZoomToOverviewArramoraButton");
    pub const LINVAK_MASSIF: Token = Token::of("NavigationZoomToOverviewLinvakMassifButton");
    pub const KNORR: Token = Token::of("NavigationZoomToOverviewKnorrButton");
    pub const MACRO_X_OFFSET: Token = Token::of("MacroXOffset");
    pub const MACRO_Y_OFFSET: Token = Token::of("MacroYOffset");
    pub const MACRO_SCALE: Token = Token::of("MacroScale");
    pub const MACRO_MAP: Token = Token::of("MacroMap");
    pub const DETAIL_MAP: Token = Token::of("DetailMap");
    pub const BLANK: Token = Token::of("blank");
    pub const NORMAL: Token = Token::of("normal");
    pub const MESSAGE: Token = Token::of("message");
    pub const POSITION: Token = Token::of("position");
    pub const STATUS: Token = Token::of("status");
    pub const MAP_COORDINATES: Token = Token::of("ID_Navigation_MapCoordinates");
    pub const OVERVIEW: Token = Token::of("OverviewMap");
    pub const OVERVIEW_LABEL: Token = Token::of("Panel_Label_OverviewMap");
    pub const OVERVIEW_CLIENT_AREA: Token = Token::of("OverviewMapClientArea");
    pub const OVERVIEW_CLOSE: Token = Token::of("OverviewMapCloseButton");
    pub const OVERVIEW_HELP: Token = Token::of("OverviewMapHelpButton");
    pub const OSTETH_MAP: Token = Token::of("OstethMap");
    pub const OMISHAN_MAP: Token = Token::of("OmishanMap");
    pub const ARRAMORA_MAP: Token = Token::of("ArramoraMap");
    pub const LINVAK_MASSIF_MAP: Token = Token::of("LinvakMassifMap");
    pub const KNORR_MAP: Token = Token::of("KnorrMap");
}

/// A rectangle of the closer map: the value of the block map it is under, its picture, where it
/// lies (in landblocks) and its size, and the labels it shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetailRow {
    pub value: u32,
    pub picture: u32,
    pub x: i32,
    pub y: i32,
    pub height: i32,
    pub width: i32,
    pub notes: Vec<u32>,
}

/// A label of the closer map: its words (by token), its picture, where it is (in land cells).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapNote {
    pub text: u32,
    pub icon: u32,
    pub x: i32,
    pub y: i32,
}

/// The labels of one picture, and the string table their words are in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapNotes {
    pub strings: u32,
    pub notes: Vec<MapNote>,
}

/// The closer map's table: the block map of its pictures (`height` rows of `width` bytes, by
/// landblock) and the pictures by the block map's value, with their labels by record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetailTable {
    pub height: u32,
    pub width: u32,
    pub cells: Vec<u8>,
    pub rows: Vec<DetailRow>,
    pub notes: Vec<(u32, MapNotes)>,
}

impl DetailTable {
    /// The block map's value for a landblock: its row's, the first cell's beyond the map.
    // D2-12591
    #[must_use]
    pub fn value(&self, x: u32, y: u32) -> u8 {
        if self.cells.is_empty() {
            return 0;
        }
        if x < self.width && y < self.height {
            let at = usize::try_from(self.height * x + y).unwrap_or(usize::MAX);
            return self.cells.get(at).copied().unwrap_or(0);
        }
        self.cells[0]
    }

    /// The picture whose value is `value`.
    // D2-07607
    #[must_use]
    pub fn row_from_value(&self, value: u32) -> Option<&DetailRow> {
        self.rows.iter().find(|r| r.value == value)
    }

    /// The picture under a landblock.
    // D2-07609
    #[must_use]
    pub fn row_from_xy(&self, x: u32, y: u32) -> Option<&DetailRow> {
        self.row_from_value(u32::from(self.value(x, y)))
    }

    fn notes(&self, id: u32) -> Option<&MapNotes> {
        self.notes.iter().find(|(i, _)| *i == id).map(|(_, n)| n)
    }
}

/// What the map draws on, from the region: the world map's picture, the land's size (in land
/// cells and in blocks) and the closer map's table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Atlas {
    pub game_map: u32,
    /// Land cells across and down.
    pub land: (i32, i32),
    /// Landblocks across and down.
    pub blocks: (i32, i32),
    pub detail: Option<DetailTable>,
}

/// The game's date and time as the calendar has them now: the year and its name, the day of the
/// season and the season's name, the part of the day's name (names by token).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub year: i32,
    pub year_name: u32,
    pub day: i32,
    pub season: u32,
    pub part: u32,
}

/// The calendar's strings' table, by enumeration value.
const CALENDAR_TABLE: u32 = 0x4000_0002;

/// The date and the time of day as the client writes them.
// D2-13062, D2-13068
#[must_use]
pub fn date_time_strings(ui: &Ui, c: &Clock) -> (StringInfo, StringInfo) {
    let table = ui
        .assets()
        .data_id_by_enum(CALENDAR_TABLE, crate::flow::TABLE_MAPPING);
    let token = |s: &str| Token::of(s).raw();
    let mut date = StringInfo::entry(table, token("ID_DateString"));
    let mut time = StringInfo::entry(table, token("ID_TimeString"));
    let named = |id: u32| {
        Value::Text(Box::new(StringInfo {
            string_id: id,
            ..StringInfo::default()
        }))
    };
    time.add_var(token("TIME"), named(c.part));
    date.add_var(token("YEAR"), Value::Int(c.year));
    date.add_var(token("YEARSPEC"), named(c.year_name));
    date.add_var(token("DAY"), Value::Int(c.day));
    date.add_var(token("SEASON"), named(c.season));
    (date, time)
}

// A cell's land coordinates, the land cell at land coordinates, and a cell's map coordinates as
// the client writes them, shared with the scripts' positions.
pub use auberean_primitives::land::{cell_at as lcoord_to_gid, land_coords as gid_to_lcoord};
pub use auberean_text::place::real_map_coords;

/// `a * b / c` rounded to the nearest (halves away from zero), as the system's helper does; -1
/// when `c` is 0 or the result does not fit.
#[must_use]
pub fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        return -1;
    }
    let (mut a, mut c) = (i64::from(a), i64::from(c));
    if c < 0 {
        a = -a;
        c = -c;
    }
    let p = a * i64::from(b);
    let r = if (a < 0) == (b < 0) || p == 0 {
        (p + c / 2) / c
    } else {
        (p - c / 2) / c
    };
    i32::try_from(r)
        .ok()
        .filter(|v| *v > i32::MIN)
        .unwrap_or(-1)
}

/// A marker on the map: its element, the object it stands for (0 for none), whether it shows on
/// the world map, its cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub elem: Option<Handle>,
    pub id: u64,
    pub on_macro: bool,
    pub cell: u32,
}

/// The overview maps of the regions: one window, showing the region asked for.
#[derive(Debug)]
pub struct OverviewMap {
    frame: FrameId,
    pub field: Option<Handle>,
    label: Option<Handle>,
    client_area: Option<Handle>,
}

impl OverviewMap {
    /// The window under the map's framework: its buttons heard; hidden (nothing made yet).
    // D2-02258
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::OVERVIEW_CLOSE, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::OVERVIEW_HELP, msg::BUTTON_RELEASE_ON);
        }
        let mut me = Self {
            frame,
            field: None,
            label: None,
            client_area: None,
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// Its window made from the play layout, once: its title and its client area found.
    // D2-02253
    pub fn create_managed_elements(&mut self, ui: &mut Ui) -> bool {
        if self.field.is_some() {
            return true;
        }
        self.field = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::OVERVIEW);
        let Some(f) = self.field else { return false };
        self.label = ui.child_recursive(f, name::OVERVIEW_LABEL);
        self.client_area = ui.child_recursive(f, name::OVERVIEW_CLIENT_AREA);
        self.label.is_some() && self.client_area.is_some()
    }

    /// A region's overview: shown, its title and its picture in that region's state.
    // D2-02256
    pub fn show_map(&mut self, ui: &mut Ui, map: Token) {
        self.show(ui, true, &mut Vec::new());
        if let Some(l) = self.label {
            ui.set_state(l, map);
        }
        if let Some(c) = self.client_area {
            ui.set_state(c, map);
        }
    }
}

impl Child for OverviewMap {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown: its window made (once) and shown; hidden: its window gone.
    // D2-02255
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if on {
            if self.create_managed_elements(ui) {
                if let Some(f) = self.field {
                    ui.set_visible(f, true);
                }
            }
        } else {
            if let Some(f) = self.field.take() {
                ui.frame_remove_root(self.frame, f);
                ui.remove(f);
            }
            self.label = None;
            self.client_area = None;
        }
    }

    /// Its close button (hidden) and help button.
    // D2-02257
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let Mail::Element { element, .. } = *mail else {
            return;
        };
        if element == name::OVERVIEW_CLOSE {
            self.show(ui, false, out);
        } else if element == name::OVERVIEW_HELP {
            out.push(Request::Help { open: true });
        }
    }

    /// It goes: its window gone.
    // D2-02254
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        if let Some(f) = self.field.take() {
            ui.frame_remove_root(self.frame, f);
            ui.remove(f);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The map window's panel.
#[derive(Debug)]
pub struct Navigation {
    frame: FrameId,
    pub field: Option<Handle>,
    player_marker: Option<Handle>,
    parent_field: Option<Handle>,
    map_parent: Option<Handle>,
    map: Option<Handle>,
    /// The regions' overview buttons: Osteth, Omishan, Linvak Massif, Arramora, Knorr.
    zoom: [Option<Handle>; 5],
    coordinates: Option<Handle>,
    time_text: Option<Handle>,
    date_text: Option<Handle>,
    /// The part of the land the map shows: its corner and size, in land cells.
    pub view: (i32, i32, i32, i32),
    /// Where the world map was last clicked, on it.
    click: (i32, i32),
    /// The player's land coordinates.
    pub player: (i32, i32),
    /// The world map's corner and scale over the land, from the window's description.
    macro_offset: (i32, i32),
    macro_scale: f32,
    /// What the texts show now: the coordinates, the date, the time.
    coords_shown: String,
    date_shown: String,
    time_shown: String,
    /// The world map shows (else the closer map); a click waits to be acted on at the next tick;
    /// the closer map keeps to the player; the player is outdoors (it may).
    pub on_macro: bool,
    pending: bool,
    pub locked: bool,
    pub can_lock: bool,
    /// The closer map's four pictures and their rows.
    pub panels: [Option<Handle>; 4],
    rows: [Option<DetailRow>; 4],
    /// The markers, by kind.
    pub markers: Vec<(u32, Vec<Marker>)>,
    pub overview: Option<OverviewMap>,
    /// What the map draws on, and the game as last shown: the player's cell, the clock, shift.
    atlas: Option<Arc<Atlas>>,
    cell: u32,
    clock: Option<Clock>,
    shift: bool,
    in_world: bool,
}

impl Navigation {
    /// The window under the toolbar's framework: its field and parts found; the closer map's
    /// four pictures made under the map's frame; the world map's corner and scale read; the
    /// markers' notices, its buttons, the map, the windows' fields and the clock heard; the
    /// overview window made hidden; hidden. The world map is put up once the region is known
    /// (the first frame of the world).
    // D2-02589
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::FIELD);
        let mut me = Self {
            frame,
            field,
            player_marker: None,
            parent_field: None,
            map_parent: None,
            map: None,
            zoom: [None; 5],
            coordinates: None,
            time_text: None,
            date_text: None,
            view: (0, 0, 0, 0),
            click: (0, 0),
            player: (0, 0),
            macro_offset: (0, 0),
            macro_scale: 1.0,
            coords_shown: String::new(),
            date_shown: String::new(),
            time_shown: String::new(),
            on_macro: true,
            pending: false,
            locked: false,
            can_lock: false,
            panels: [None; 4],
            rows: [None, None, None, None],
            markers: Vec::new(),
            overview: None,
            atlas: None,
            cell: 0,
            clock: None,
            shift: false,
            in_world: false,
        };
        if let Some(f) = field {
            let part = |ui: &Ui, n| ui.child_recursive(f, n);
            me.coordinates = part(ui, name::COORDINATES_TEXT);
            me.player_marker = part(ui, name::PLAYER_MARKER);
            me.map = part(ui, name::MAP_FIELD);
            me.map_parent = part(ui, name::MAP_PARENT_FIELD);
            for p in &mut me.panels {
                *p = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::DETAIL_PANEL);
                if let (Some(p), Some(mp)) = (*p, me.map_parent) {
                    ui.set_parent(p, mp);
                }
            }
            me.parent_field = part(ui, name::PARENT_FIELD);
            me.zoom = [
                part(ui, name::OSTETH),
                part(ui, name::OMISHAN),
                part(ui, name::LINVAK_MASSIF),
                part(ui, name::ARRAMORA),
                part(ui, name::KNORR),
            ];
            if let Some(n) = ui.node(f) {
                me.macro_offset = (
                    n.int_attr(name::MACRO_X_OFFSET).unwrap_or(0),
                    n.int_attr(name::MACRO_Y_OFFSET).unwrap_or(0),
                );
                me.macro_scale = n.float_attr(name::MACRO_SCALE).unwrap_or(1.0);
            }
        }
        if let Some(l) = ui.frame_listener(frame) {
            let release_on = [
                name::CLOSE_BUTTON,
                name::MACRO_MAP_BUTTON,
                name::LOCK_POSITION_BUTTON,
                name::HELP_BUTTON,
            ];
            for b in release_on {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            for b in [name::LEFT, name::RIGHT, name::UP, name::DOWN] {
                ui.listen_for_message(l, b, msg::MOUSE_PRESS);
                ui.listen_for_message(l, b, msg::HOT_CLICK);
            }
            for b in [
                name::OSTETH,
                name::OMISHAN,
                name::ARRAMORA,
                name::LINVAK_MASSIF,
                name::KNORR,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::MAP_FIELD, msg::MOUSE_PRESS_TOP);
            ui.listen_for_message(l, name::MAP_PARENT_FIELD, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::PLAYER_MARKER, msg::MOUSE_PRESS_TOP);
            ui.listen_for_message(l, name::MARKER_TEMPLATE, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::FIELD_TOP, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::FIELD_TOP, msg::MOUSE_RELEASE);
            ui.listen_for_message(l, name::BUTTON, msg::BUTTON_RELEASE_ON);
            for f in WINDOW_FIELDS {
                ui.listen_for_message(l, Token::of(f), BRING_TO_FRONT);
            }
            // The client hears the examination window's field shown or hidden here, and acts
            // only on its own field's.
            ui.listen_for_message(l, Token::of("ExaminationField"), msg::SET_SHOW_STATE);
            ui.listen_for_global_message(l, global::TIME);
        }
        me.overview = Some(OverviewMap::new(ui, parent));
        let mut out = Vec::new();
        me.framework_show(ui, false);
        me.after_show(ui, &mut out);
        me
    }

    /// The region known: the world map's picture put on the map, the world map put up.
    fn start(&mut self, ui: &mut Ui, atlas: Arc<Atlas>) {
        if let Some(m) = self.map {
            ui.add_image_to_state(m, atlas.game_map, 2, state::STATELESS);
        }
        self.atlas = Some(atlas);
        self.init_macro_map(ui);
    }

    fn land(&self) -> (i32, i32) {
        self.atlas.as_ref().map_or((0, 0), |a| a.land)
    }

    fn part(&self, ui: &Ui, n: Token) -> Option<Handle> {
        self.field.and_then(|f| ui.child_recursive(f, n))
    }

    /// The edges' buttons shown only on the closer map not kept to the player, while the window
    /// shows.
    // D2-02542
    pub fn auto_show_scroll_buttons(&self, ui: &mut Ui) {
        let on = ui.frame_shown(self.frame) && !self.on_macro && !self.locked;
        for b in [name::UP, name::LEFT, name::RIGHT, name::DOWN] {
            if let Some(h) = self.part(ui, b) {
                ui.set_visible(h, on);
            }
        }
    }

    /// The world map's texts: the date and the time as they change; the player's mark.
    // D2-02543
    pub fn update_macro_map(&mut self, ui: &mut Ui) {
        let (Some(_), Some(t), Some(d)) = (self.player_marker, self.time_text, self.date_text)
        else {
            return;
        };
        if let Some(c) = self.clock {
            let (date, time) = date_time_strings(ui, &c);
            let date = crate::social::write(ui, &date);
            let time = crate::social::write(ui, &time);
            if date != self.date_shown {
                ui.set_text(d, &date);
                self.date_shown = date;
            }
            if time != self.time_shown {
                ui.set_text(t, &time);
                self.time_shown = time;
            }
        }
        self.update_player_pos(ui);
    }

    /// The lock button's tooltip: on or off, or (outdoors only) that it cannot be used now.
    // D2-02544
    pub fn update_lock_position_button_tooltip(&self, ui: &mut Ui) {
        let Some(b) = self
            .parent_field
            .and_then(|p| ui.child_recursive(p, name::LOCK_POSITION_BUTTON))
        else {
            return;
        };
        let which = if self.locked {
            "ID_Navigation_TT_LockButtonOn"
        } else {
            "ID_Navigation_TT_LockButtonOff"
        };
        let mut s = entry(ui, which);
        if !self.can_lock {
            s.string_id = Token::of("ID_Navigation_TT_LockButtonError").raw();
            s.add_var(
                name::STATUS.raw(),
                Value::Text(Box::new(StringInfo {
                    string_id: Token::of(which).raw(),
                    ..StringInfo::default()
                })),
            );
        }
        let tip = crate::social::write(ui, &s);
        ui.set_tooltip(b, &tip);
    }

    /// Land coordinates to a point on the map (the world map's 238 a side, the closer map's
    /// 250), and whether it falls on the map.
    // D2-02545
    #[must_use]
    pub fn convert_landscape_to_pixel(&self, ui: &Ui, lx: i32, ly: i32) -> (f32, f32, bool) {
        let Some(r) = self.map.and_then(|m| ui.node(m)).map(|n| n.rect) else {
            return (0.0, 0.0, false);
        };
        let side = if self.on_macro { 238.0 } else { 250.0 };
        let (x0, y0, w, h) = self.view;
        #[allow(clippy::cast_precision_loss)]
        let py = ((ly as f32 - y0 as f32) * side) / h as f32;
        #[allow(clippy::cast_precision_loss)]
        let px = ((lx as f32 - x0 as f32) * side) / w as f32;
        let py = side - py;
        #[allow(clippy::cast_precision_loss)]
        let on = (0.0..r.height() as f32).contains(&py) && (0.0..r.width() as f32).contains(&px);
        (px, py, on)
    }

    /// A point on the map to land coordinates, when it is within the part shown.
    // D2-02546
    #[must_use]
    pub fn convert_pixel_to_landscape(&self, px: i32, py: i32) -> Option<(i32, i32)> {
        self.map?;
        let (x0, y0, w, h) = self.view;
        let (side, per) = if self.on_macro {
            (0xEE, 0.004_201_681_f32)
        } else {
            (0xFA, 0.004_f32)
        };
        let at = |n: i32| to_i32_f64(f64::from(n) * f64::from(per));
        let ly = at((side - py).wrapping_mul(h));
        let lx = at(w.wrapping_mul(px));
        ((0..h).contains(&ly) && (0..w).contains(&lx)).then_some((lx + x0, ly + y0))
    }

    /// An element put with its middle at a point.
    // D2-02547
    pub fn center_marker(ui: &mut Ui, el: Handle, x: i32, y: i32) {
        let Some(r) = ui.node(el).map(|n| n.rect) else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let half = |n: i32| to_i32(n as f32 * 0.5);
        ui.move_to(el, x - half(r.width()), y - half(r.height()));
    }

    /// The map's pictures sized: on the world map the region's picture fitted to the frame,
    /// kept to its proportions, in the middle; on the closer map each detail picture at its size,
    /// placed where its rectangle lies in the part shown (one with no size at the corner).
    // D2-02548
    pub fn size_map(&self, ui: &mut Ui) {
        let Some(frame) = self.map_parent.and_then(|m| ui.node(m)).map(|n| n.rect) else {
            return;
        };
        let (fw, fh) = (frame.width(), frame.height());
        if self.on_macro {
            let Some(m) = self.map else { return };
            let Some((iw, ih)) = ui.image_size(m) else {
                return;
            };
            let (iw, ih) = (
                i32::try_from(iw).unwrap_or(i32::MAX),
                i32::try_from(ih).unwrap_or(i32::MAX),
            );
            #[allow(clippy::cast_precision_loss)]
            let scale = if ih < iw {
                fw as f32 / iw as f32
            } else {
                fh as f32 / ih as f32
            };
            #[allow(clippy::cast_precision_loss)]
            let (w, h) = (iw as f32 * scale, ih as f32 * scale);
            #[allow(clippy::cast_precision_loss)]
            let mut x = (fw as f32 - w) * 0.5;
            #[allow(clippy::cast_precision_loss)]
            let mut y = (fh as f32 - h) * 0.5;
            if iw < ih {
                y = 0.0;
            } else if iw > ih {
                x = 0.0;
            }
            ui.move_to(m, to_i32(x), to_i32(y));
            ui.resize_to(m, to_i32(w), to_i32(h));
            return;
        }
        for (i, (panel, row)) in self.panels.iter().zip(&self.rows).enumerate() {
            let Some(p) = *panel else { continue };
            let Some((iw, ih)) = ui.image_size(p) else {
                continue;
            };
            let (iw, ih) = (
                i32::try_from(iw).unwrap_or(i32::MAX),
                i32::try_from(ih).unwrap_or(i32::MAX),
            );
            ui.resize_to(p, iw, ih);
            let r = row.clone().unwrap_or_default();
            let (w, h) = (r.width << LBLOCK_SHIFT, r.height << LBLOCK_SHIFT);
            if w == 0 || h == 0 {
                ui.move_to(p, 0, 0);
                continue;
            }
            let (ox, oy) = QUADRANTS[i];
            let x = mul_div((r.x << LBLOCK_SHIFT) + ox * w - self.view.0, iw, w);
            let y = mul_div(self.view.1 - ((r.y << LBLOCK_SHIFT) + oy * h), ih, h);
            ui.move_to(p, x - ox * iw, oy * ih + y);
        }
    }

    /// A marker clicked: its words (its tooltip) at the window's foot.
    // D2-02549
    pub fn read_marker_info(&self, ui: &mut Ui, marker: Handle) {
        let Some(t) = self.part(ui, name::NOTE_TEXT) else {
            return;
        };
        let words = ui.element_tooltip(marker).unwrap_or_default().to_owned();
        ui.set_text(t, &words);
    }

    /// While the window shows: the player's map coordinates outdoors (none inside); on the world
    /// map its texts and the player's mark, else the player's mark; the edges' buttons.
    // D2-02558
    pub fn update(&mut self, ui: &mut Ui) {
        if !ui.frame_shown(self.frame) || !self.in_world {
            return;
        }
        let Some(c) = self.coordinates else { return };
        if self.cell & 0xFFFF < FIRST_ENVCELL {
            let (wide, long) = self.atlas.as_ref().map_or((0, 0), |a| a.blocks);
            let blocks = (
                u32::try_from(long).unwrap_or(0),
                u32::try_from(wide).unwrap_or(0),
            );
            let words = crate::radar::map_coordinates(ui, self.cell, blocks);
            if words != self.coords_shown {
                ui.set_text(c, &words);
                self.coords_shown = words;
            }
        } else {
            self.coords_shown.clear();
            ui.clear_all_text(c);
        }
        if self.on_macro {
            self.update_macro_map(ui);
        } else {
            self.update_player_pos(ui);
        }
        self.auto_show_scroll_buttons(ui);
    }

    /// A marker put where it stands on the map: hidden on the world map unless meant for it, or
    /// off the land or the map.
    // D2-02559
    pub fn place_marker(&self, ui: &mut Ui, m: &Marker) -> bool {
        let Some(el) = m.elem else { return false };
        if self.on_macro && !m.on_macro {
            ui.set_visible(el, false);
            return false;
        }
        let Some((lx, ly)) = gid_to_lcoord(m.cell, self.land()) else {
            ui.set_visible(el, false);
            return false;
        };
        let (px, py, on) = self.convert_landscape_to_pixel(ui, lx, ly);
        if !on {
            ui.set_visible(el, false);
            return false;
        }
        ui.set_visible(el, true);
        Self::center_marker(ui, el, to_i32(px), to_i32(py));
        true
    }

    /// Shown or hidden: it comes to the front, its field shown as it is; brought up to date.
    // D2-02564
    fn after_show(&mut self, ui: &mut Ui, _out: &mut Vec<Request>) {
        let Some(f) = self.field else { return };
        ui.broadcast(f, BRING_TO_FRONT, 0);
        let shown = ui.frame_shown(self.frame);
        ui.set_visible(f, shown);
        self.update(ui);
        self.auto_show_scroll_buttons(ui);
    }

    fn framework_show(&mut self, ui: &mut Ui, on: bool) {
        ui.frame_show(self.frame, on);
        if let Some(o) = &mut self.overview {
            gameplay::hear(o, ui, &mut Vec::new());
        }
    }

    /// Every marker put where it stands.
    // D2-02570
    pub fn place_markers(&self, ui: &mut Ui) {
        for (_, list) in &self.markers {
            for m in list {
                self.place_marker(ui, m);
            }
        }
    }

    /// A kind's markers gone.
    // D2-02571
    pub fn flush_markers(ui: &mut Ui, list: &mut Vec<Marker>) {
        for m in list.drain(..) {
            if let Some(e) = m.elem {
                ui.remove(e);
            }
        }
    }

    /// Every marker gone.
    // D2-02572
    pub fn flush_all_markers(&mut self, ui: &mut Ui) {
        for (_, mut list) in std::mem::take(&mut self.markers) {
            Self::flush_markers(ui, &mut list);
        }
    }

    fn list_mut(&mut self, kind: u32) -> Option<&mut Vec<Marker>> {
        self.markers
            .iter_mut()
            .find(|(k, _)| *k == kind)
            .map(|(_, l)| l)
    }

    /// The world map or the closer map: the closer map's labels gone; the world map's picture or
    /// the four detail pictures shown, their tooltips cleared; the frame in that map's state (the
    /// player's mark kept under the map); the edges' buttons; the regions' buttons on the world
    /// map only.
    // D2-02576
    pub fn setup_elements_for_map_mode(&mut self, ui: &mut Ui, on_macro: bool) -> bool {
        let (Some(pf), Some(map)) = (self.parent_field, self.map) else {
            return false;
        };
        if let Some(l) = self.list_mut(STATIC_MARKER) {
            let mut l = std::mem::take(l);
            Self::flush_markers(ui, &mut l);
        }
        ui.set_visible(map, on_macro);
        ui.clear_element_tooltip(map);
        for p in self.panels.iter().flatten() {
            ui.set_visible(*p, !on_macro);
            ui.clear_element_tooltip(*p);
        }
        if let Some(pm) = self.player_marker {
            ui.set_parent(pm, map);
        }
        let set = ui.set_state(
            pf,
            if on_macro {
                name::MACRO_MAP
            } else {
                name::DETAIL_MAP
            },
        );
        if let (Some(pm), Some(mp)) = (self.player_marker, self.map_parent) {
            ui.set_parent(pm, mp);
        }
        self.auto_show_scroll_buttons(ui);
        for z in self.zoom.iter().flatten() {
            ui.set_visible(*z, on_macro);
        }
        set
    }

    /// The world map: its texts found; the picture sized; the part shown the world map's corner
    /// and the land's size times the scale; the date and time written; its tooltip; the closer
    /// map no longer kept to the player; the markers placed.
    // D2-02577
    pub fn init_macro_map(&mut self, ui: &mut Ui) {
        self.on_macro = true;
        self.setup_elements_for_map_mode(ui, true);
        let Some(pf) = self.parent_field else { return };
        self.time_text = ui.child_recursive(pf, name::TIME_TEXT);
        self.date_text = ui.child_recursive(pf, name::DATE_TEXT);
        let (Some(t), Some(d), Some(map)) = (self.time_text, self.date_text, self.map) else {
            return;
        };
        self.size_map(ui);
        let blocks = self.atlas.as_ref().map_or((0, 0), |a| a.blocks);
        #[allow(clippy::cast_precision_loss)]
        let scaled = |n: i32| to_i32((n << LBLOCK_SHIFT) as f32 * self.macro_scale);
        self.view = (
            self.macro_offset.0,
            self.macro_offset.1,
            scaled(blocks.0),
            scaled(blocks.1),
        );
        self.coords_shown.clear();
        if let Some(c) = self.clock {
            let (date, time) = date_time_strings(ui, &c);
            self.date_shown = crate::social::write(ui, &date);
            self.time_shown = crate::social::write(ui, &time);
            ui.set_text(d, &self.date_shown);
            ui.set_text(t, &self.time_shown);
        }
        let tip = crate::social::write(ui, &entry(ui, "ID_Navigation_TT_MacroMap"));
        ui.set_tooltip(map, &tip);
        self.locked = false;
        self.place_markers(ui);
        self.update(ui);
    }

    /// Every marker of a kind gone (not the fixed kind).
    // D2-02578
    pub fn clear_markers(&mut self, ui: &mut Ui, kind: u32) {
        if kind == STATIC_MARKER {
            return;
        }
        if let Some(i) = self.markers.iter().position(|(k, _)| *k == kind) {
            let (_, mut list) = self.markers.remove(i);
            Self::flush_markers(ui, &mut list);
        }
    }

    /// A marker of a kind (whose list there is): its element made, the same object's marker
    /// replaced, its picture, its tooltip (the words and the cell's map coordinates), under the
    /// map, placed.
    // D2-02579
    #[allow(clippy::too_many_arguments)]
    pub fn add_marker(
        &mut self,
        ui: &mut Ui,
        kind: u32,
        id: u64,
        cell: u32,
        icon: u32,
        words: &StringInfo,
        on_macro: bool,
    ) -> bool {
        if self.list_mut(kind).is_none() {
            return false;
        }
        let Some(el) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::MARKER_TEMPLATE) else {
            return false;
        };
        let info = Marker {
            elem: Some(el),
            id,
            on_macro,
            cell,
        };
        let land = self.land();
        let map_parent = self.map_parent;
        let Some(list) = self.list_mut(kind) else {
            return false;
        };
        if id != 0 {
            if let Some(i) = list.iter().position(|m| m.id == id) {
                let old = list.remove(i);
                if let Some(e) = old.elem {
                    ui.remove(e);
                }
            }
        }
        list.push(info.clone());
        if icon != 0 {
            ui.add_image_to_state(el, icon, 2, state::STATELESS);
        }
        let mut s = entry(ui, "ID_Navigation_Marker");
        s.add_var(name::MESSAGE.raw(), Value::Text(Box::new(words.clone())));
        let at = real_map_coords(cell, land);
        s.add_var(
            name::POSITION.raw(),
            Value::Text(Box::new(StringInfo::literal(&at))),
        );
        let tip = crate::social::write(ui, &s);
        ui.set_tooltip(el, &tip);
        if let Some(mp) = map_parent {
            ui.set_parent(el, mp);
        }
        self.place_marker(ui, &info);
        true
    }

    /// An object's marker of a kind gone; whether there was one.
    // D2-02580
    pub fn flush_marker_by_iid(&mut self, ui: &mut Ui, kind: u32, id: u64) -> bool {
        let Some(list) = self.list_mut(kind) else {
            return false;
        };
        let Some(i) = list.iter().position(|m| m.id == id) else {
            return false;
        };
        let m = list.remove(i);
        if let Some(e) = m.elem {
            ui.remove(e);
        }
        true
    }

    /// The scripts put a marker on the map (not of the fixed kind): its kind's list made when it
    /// has none; none put when one of its kind stands within about two and a half cells; the
    /// kind's oldest dropped beyond ten.
    // D2-02582
    pub fn recv_add_marker(
        &mut self,
        ui: &mut Ui,
        kind: u32,
        id: u64,
        cell: u32,
        icon: u32,
        words: &StringInfo,
    ) {
        if !self.in_world || kind == STATIC_MARKER {
            return;
        }
        let land = self.land();
        if let Some(list) = self.list_mut(kind) {
            if let Some((x, y)) = gid_to_lcoord(cell, land) {
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (x as f32, y as f32);
                for m in list.iter() {
                    if let Some((mx, my)) = gid_to_lcoord(m.cell, land) {
                        #[allow(clippy::cast_precision_loss)]
                        let (dx, dy) = (fx - mx as f32, fy - my as f32);
                        if dx * dx + dy * dy < 6.0 {
                            return;
                        }
                    }
                }
            }
            while list.len() > 9 {
                let m = list.remove(0);
                if let Some(e) = m.elem {
                    ui.remove(e);
                }
            }
        } else {
            self.markers.push((kind, Vec::new()));
        }
        self.add_marker(ui, kind, id, cell, icon, words, false);
    }

    /// The labels of one of the closer map's pictures: each label of each of its records that
    /// falls on the map made a fixed marker, with its picture, its words and coordinates, placed.
    // D2-02584
    pub fn add_map_notes(&mut self, ui: &mut Ui, i: usize) {
        if self.panels[i].is_none() {
            return;
        }
        let Some(row) = self.rows[i].clone() else {
            return;
        };
        let Some(atlas) = self.atlas.clone() else {
            return;
        };
        let Some(table) = atlas.detail.as_ref() else {
            return;
        };
        let land = atlas.land;
        for id in &row.notes {
            let Some(notes) = table.notes(*id) else {
                continue;
            };
            for n in &notes.notes {
                let (px, py, on) = self.convert_landscape_to_pixel(ui, n.x, n.y);
                if !on {
                    continue;
                }
                let cell = lcoord_to_gid(n.x, n.y, land);
                let Some(el) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::MARKER_TEMPLATE)
                else {
                    continue;
                };
                let mut s = StringInfo::entry(notes.strings, n.text);
                let at = real_map_coords(cell, land);
                s.add_var(
                    name::MAP_COORDINATES.raw(),
                    Value::Text(Box::new(StringInfo::literal(&at))),
                );
                if self.list_mut(STATIC_MARKER).is_none() {
                    self.markers.push((STATIC_MARKER, Vec::new()));
                }
                if let Some(l) = self.list_mut(STATIC_MARKER) {
                    l.push(Marker {
                        elem: Some(el),
                        id: 0,
                        on_macro: false,
                        cell,
                    });
                }
                ui.add_image_to_state(el, n.icon, 2, state::STATELESS);
                if let Some(mp) = self.map_parent {
                    ui.set_parent(el, mp);
                }
                let tip = crate::social::write(ui, &s);
                ui.set_tooltip(el, &tip);
                Self::center_marker(ui, el, to_i32(px), to_i32(py));
            }
        }
    }

    /// The closer map around land coordinates: the part shown 128 cells a side with them in the
    /// middle, kept on the land; each of its four pictures from the detail map table (one with
    /// none blank, behind), with its labels; when any was found, sized, the markers placed, the
    /// lock's tooltip, brought up to date. Whether any was found.
    // D2-02586
    pub fn init_detail_map_from_cell_coords(&mut self, ui: &mut Ui, x: i32, y: i32) -> bool {
        let land_width = self.land().0;
        let mut x0 = x - DETAIL_SIDE / 2;
        let mut y0 = y - DETAIL_SIDE / 2;
        if x0 < 0 {
            x0 = 0;
        }
        if land_width < x0 + DETAIL_SIDE {
            x0 = land_width - DETAIL_SIDE;
        }
        if y0 < 0 {
            y0 = 0;
        }
        if land_width < y0 + DETAIL_SIDE {
            y0 = land_width - DETAIL_SIDE;
        }
        self.view = (x0, y0, DETAIL_SIDE, DETAIL_SIDE);
        let corners = [
            (x0, y0 + DETAIL_SIDE),
            (x0 + DETAIL_SIDE, y0 + DETAIL_SIDE),
            (x0, y0),
            (x0 + DETAIL_SIDE, y0),
        ];
        self.on_macro = false;
        self.setup_elements_for_map_mode(ui, false);
        let mut any = false;
        let atlas = self.atlas.clone();
        for (i, (cx, cy)) in corners.into_iter().enumerate() {
            #[allow(clippy::cast_sign_loss)]
            let found = atlas
                .as_ref()
                .and_then(|a| a.detail.as_ref())
                .and_then(|t| {
                    t.row_from_xy((cx >> LBLOCK_SHIFT) as u32, (cy >> LBLOCK_SHIFT) as u32)
                })
                .cloned();
            any |= found.is_some();
            let Some(p) = self.panels[i] else {
                return false;
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            match found {
                None => {
                    ui.set_state(p, name::BLANK);
                    ui.set_z_level(p, i as i32 + 10);
                    self.rows[i] = None;
                }
                Some(row) => {
                    ui.set_state(p, state::STATELESS);
                    ui.set_z_level(p, 2);
                    ui.add_image_to_state(p, row.picture, 0, state::STATELESS);
                    self.rows[i] = Some(row);
                    self.add_map_notes(ui, i);
                }
            }
        }
        if any {
            self.size_map(ui);
            self.coords_shown.clear();
            self.place_markers(ui);
            self.update_lock_position_button_tooltip(ui);
            self.update(ui);
        }
        any
    }

    /// The closer map around the player.
    // D2-02587
    pub fn zoom_to_char_pos(&mut self, ui: &mut Ui) {
        let (x, y) = self.player;
        self.init_detail_map_from_cell_coords(ui, x, y);
    }

    /// The player moved to another land cell: outdoors the lock may be used, and kept to the
    /// player the closer map follows; the lock's tooltip.
    // D2-02588
    pub fn on_player_block_changed(&mut self, ui: &mut Ui) {
        self.can_lock = self.cell & 0xFFFF < FIRST_ENVCELL;
        if self.can_lock && self.locked {
            let (x, y) = self.player;
            self.init_detail_map_from_cell_coords(ui, x, y);
        }
        self.update_lock_position_button_tooltip(ui);
    }

    /// The closer map around the place last clicked on the world map.
    // D2-02591
    pub fn init_detail_map_from_last_click(&mut self, ui: &mut Ui) {
        if let Some((x, y)) = self.convert_pixel_to_landscape(self.click.0, self.click.1) {
            self.init_detail_map_from_cell_coords(ui, x, y);
        }
    }

    /// The player's mark: where the player stands on the map (off the map inside, or when the
    /// player is in no land cell), the player's land cell kept as it changes.
    // D2-02592
    pub fn update_player_pos(&mut self, ui: &mut Ui) {
        let at = gid_to_lcoord(self.cell, self.land());
        match at {
            Some(p) if p != self.player => {
                self.player = p;
                self.on_player_block_changed(ui);
            }
            Some(_) => {}
            None => self.can_lock = false,
        }
        let Some(pm) = self.player_marker else { return };
        let Some(r) = ui.node(pm).map(|n| n.rect) else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let (mut px, mut py) = (-(r.width() as f32), -(r.height() as f32));
        if self.can_lock {
            let (lx, ly) = at.unwrap_or(self.player);
            let (x, y, _) = self.convert_landscape_to_pixel(ui, lx, ly);
            if self.map.is_some() {
                (px, py) = (x, y);
            }
        }
        Self::center_marker(ui, pm, to_i32(px), to_i32(py));
    }
}

impl Child for Navigation {
    fn frame(&self) -> FrameId {
        self.frame
    }

    // D2-02564
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        self.framework_show(ui, on);
        self.after_show(ui, out);
    }

    /// The field shown or hidden; a window come to the front; the title bar pressed (to the
    /// front) and let go (the places kept); its button, close and help buttons; the regions'
    /// overview buttons; the lock (kept to the player as it is pressed in); on the world map a
    /// click (the closer map there at the next tick) or the player's mark (the closer map around
    /// the player); on the closer map the world map button or a right click (the world map at the
    /// next tick), a marker (its words), the player's mark (its description), the edges' buttons
    /// (nine cells, three with Shift; not while kept to the player); the clock (a click waiting
    /// acted on; brought up to date).
    // D2-02590, D2-02593
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let (element, what, from, param) = match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if self.pending {
                    if self.on_macro {
                        self.init_detail_map_from_last_click(ui);
                    } else {
                        self.init_macro_map(ui);
                    }
                    self.pending = false;
                }
                self.update(ui);
                return;
            }
            Mail::Element {
                element,
                what,
                from,
                param,
            } => (element, what, from, param),
            _ => return,
        };
        if what == msg::SET_SHOW_STATE && Some(from) == self.field {
            self.show(ui, param != 0, out);
        }
        if what == BRING_TO_FRONT {
            window_to_front(ui, self.field, Some(from));
        }
        if element == name::FIELD_TOP {
            if what == msg::MOUSE_PRESS {
                if let Some(f) = self.field {
                    ui.broadcast(f, BRING_TO_FRONT, 0);
                }
            }
            if what == msg::MOUSE_RELEASE {
                gameplay::GamePlay::save_locations_changed(out);
            }
        }
        let overview = |me: &mut Self, ui: &mut Ui, map: Token| {
            if let Some(o) = &mut me.overview {
                o.show_map(ui, map);
            }
        };
        match element {
            e if e == name::BUTTON => {
                let shown = ui.frame_shown(self.frame);
                self.show(ui, !shown, out);
            }
            e if e == name::CLOSE_BUTTON => self.show(ui, false, out),
            e if e == name::HELP_BUTTON => out.push(Request::Help { open: true }),
            e if e == name::OSTETH => overview(self, ui, name::OSTETH_MAP),
            e if e == name::OMISHAN => overview(self, ui, name::OMISHAN_MAP),
            e if e == name::ARRAMORA => overview(self, ui, name::ARRAMORA_MAP),
            e if e == name::KNORR => overview(self, ui, name::KNORR_MAP),
            e if e == name::LINVAK_MASSIF => overview(self, ui, name::LINVAK_MASSIF_MAP),
            e if e == name::LOCK_POSITION_BUTTON => {
                self.locked = ui.node(from).is_some_and(|n| n.state != name::NORMAL);
                self.update_lock_position_button_tooltip(ui);
                self.auto_show_scroll_buttons(ui);
                if self.locked {
                    self.zoom_to_char_pos(ui);
                }
            }
            e if self.on_macro => {
                if e == name::MAP_FIELD {
                    if param == button_id::LEFT {
                        let (px, py) = ui.pointer();
                        let o = ui.screen_rect(from).unwrap_or_default();
                        self.click = (px - o.x0, py - o.y0);
                        self.pending = true;
                    }
                } else if e == name::PLAYER_MARKER {
                    self.zoom_to_char_pos(ui);
                }
            }
            e if e == name::MACRO_MAP_BUTTON
                || (e == name::MAP_PARENT_FIELD && param == button_id::RIGHT) =>
            {
                self.pending = true;
            }
            e if e == name::MARKER_TEMPLATE => self.read_marker_info(ui, from),
            e if e == name::PLAYER_MARKER => {
                if let Some(t) = self.part(ui, name::NOTE_TEXT) {
                    let s = entry(ui, "ID_Navigation_PlayerMarkerDescription");
                    crate::social::set_string_info(ui, t, &s);
                }
            }
            e if !self.locked => {
                let step = if self.shift { SHIFT_STEP } else { STEP };
                let (cx, cy) = (self.view.2 / 2 + self.view.0, self.view.3 / 2 + self.view.1);
                let to = match e {
                    e if e == name::RIGHT => Some((cx + step, cy)),
                    e if e == name::LEFT => Some((cx - step, cy)),
                    e if e == name::UP => Some((cx, cy + step)),
                    e if e == name::DOWN => Some((cx, cy - step)),
                    _ => None,
                };
                if let Some((x, y)) = to {
                    self.init_detail_map_from_cell_coords(ui, x, y);
                }
            }
            _ => {}
        }
    }

    /// The scripts' markers put on the map, taken off, cleared by kind.
    // D2-02583
    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        match n.native {
            notice::ADD_MARKER => {
                let words = match n.records.get(5) {
                    Some(Some(Record::Text(s))) => s.clone(),
                    _ => StringInfo::default(),
                };
                self.recv_add_marker(
                    ui,
                    n.word_at(0),
                    n.long(1),
                    n.word_at(3),
                    n.word_at(4),
                    &words,
                );
            }
            notice::REMOVE_MARKER => {
                self.flush_marker_by_iid(ui, n.word_at(0), n.long(1));
            }
            notice::CLEAR_MARKERS => self.clear_markers(ui, n.word_at(0)),
            _ => {}
        }
    }

    /// The game as shown this frame: the region (the world map put up the first time), the
    /// player's cell, the clock, shift; the overview's frame.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        if let Some(w) = view.world {
            self.cell = w.cell;
            self.in_world = w.player != 0;
        }
        self.clock = view.clock.or(self.clock);
        self.shift = view.shift;
        if self.atlas.is_none() {
            if let Some(a) = view.atlas {
                self.start(ui, Arc::clone(a));
            }
        }
        if let Some(o) = &mut self.overview {
            gameplay::hear(o, ui, out);
        }
    }

    /// It goes: the overview, every marker, the closer map's pictures.
    // D2-02575, D2-02581
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        if let Some(o) = self.overview.take() {
            Box::new(o).close(ui);
        }
        self.flush_all_markers(ui);
        for p in self.panels.iter().flatten() {
            ui.remove(*p);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
