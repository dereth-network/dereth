//! The radar (top right): the dial turning with the player, the blips of what is around, the
//! place line under it.
//!
//! The radar shows what stands within its range around the player (50 meters inside, 75 outside,
//! as the radar's control script gives them), the player at its middle and the way the player
//! faces up: each object the player's scripts put on the radar is a blip of its color and shape,
//! its place scaled from the range to the radar's radius. The dial over it turns so its north
//! points north. Every tick while it shows it draws again.
//!
//! The blip under the pointer (within four pixels) names its object in the radar's tooltip, and a
//! press on the radar then selects it. The selection's blip has brackets around it.
//!
//! The place line under the dial reads "Inside" in a building or a dungeon, and outdoors the
//! map coordinates (north or south, then east or west, to a tenth); its button shows and hides
//! it, kept in the player's options.
//!
//! The objects around with a groove level (music that plays as they come near) add theirs when in
//! range: the sum sets the music's groove level, worked out again every so often.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Rect, Token, Ui};

use crate::gameplay::{Child, Notice, Request, View, PLAY_LAYOUT_ENUM};
use crate::world::{World, NO_BLIP};

mod name {
    use auberean_ui::Token;
    pub const RADAR_FIELD: Token = Token::of("RadarField");
    pub const HEADING_FIELD: Token = Token::of("HeadingField");
    pub const BASE_FIELD: Token = Token::of("BaseField");
    pub const COORD_BOX_BUTTON: Token = Token::of("CoordBoxButton");
    pub const COORDINATES_TEXT: Token = Token::of("CoordinatesText");
    pub const RADAR_CENTER_X: Token = Token::of("RadarCenter_X");
    pub const RADAR_CENTER_Y: Token = Token::of("RadarCenter_Y");
    pub const RADAR_RADIUS: Token = Token::of("RadarRadius");
    pub const GROOVE_DELAY: Token = Token::of("GrooveDelay");
    pub const OPEN: Token = Token::of("open");
    pub const CLOSED: Token = Token::of("closed");
}

/// The player's scripts' notices the radar hears.
pub mod notice {
    /// An object's radar look changed (about the player: everyone's).
    pub const CHANGE_RADAR_LOOK: u32 = 0x0085 << 16 | 0x000A;
    /// The player's options came back.
    pub const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
}

/// The radar's shapes, as the scripts name them.
pub mod shape {
    pub const POINT: u32 = 0x4000_0001;
    pub const FILL: u32 = 0x4000_0002;
    pub const CROSS: u32 = 0x4000_0003;
    pub const X: u32 = 0x4000_0004;
    pub const HOLLOW: u32 = 0x4000_0005;
    pub const L: u32 = 0x4000_0006;
    pub const EDGES: u32 = 0x4000_0007;
    pub const CORNERS: u32 = 0x4000_0008;
    pub const T: u32 = 0x4000_0009;
    pub const FACE: u32 = 0x4000_000A;
}

/// The string table of the place line, by enumeration value.
pub const PLACE_TABLE_ENUM: u32 = 0x4000_0007;

/// The string table of the radar's tooltips, by enumeration value.
pub const TOOLTIP_TABLE_ENUM: u32 = 0x4100_0018;

/// The place line: "Inside" in an interior cell, else the map coordinates of the cell (the world
/// `blocks` long and wide): north or south and east or west of the middle, each a hundred at the
/// world's edge, to a tenth.
// D2-13199
#[must_use]
pub fn map_coordinates(ui: &Ui, cell: u32, blocks: (u32, u32)) -> String {
    let table = ui
        .assets()
        .data_id_by_enum(PLACE_TABLE_ENUM, crate::flow::TABLE_MAPPING);
    let token = |s: &str| Token::of(s).raw();
    let (long, wide) = blocks;
    let land = (
        i32::try_from(wide * 8).unwrap_or(0),
        i32::try_from(long * 8).unwrap_or(0),
    );
    let Some((x, y)) = auberean_primitives::land::map_coords(cell, land) else {
        let info = auberean_text::StringInfo::entry(table, token("Inside"));
        return ui.assets().write(&info).unwrap_or_default();
    };
    let mut info = auberean_text::StringInfo::entry(table, token("RealMapCoords"));
    let named = |s: &str| {
        auberean_text::Value::Text(Box::new(auberean_text::StringInfo {
            string_id: token(s),
            ..auberean_text::StringInfo::default()
        }))
    };
    let number = |v: f32| auberean_text::Value::Float {
        value: f64::from(v.abs()),
        decimals: 1,
    };
    for (id, value) in [
        ("NUM1", number(y)),
        ("DIR1", named(if y >= 0.0 { "North" } else { "South" })),
        ("NUM2", number(x)),
        ("DIR2", named(if x >= 0.0 { "East" } else { "West" })),
    ] {
        info.variables.push(auberean_text::Variable {
            id: token(id),
            value,
        });
    }
    ui.assets().write(&info).unwrap_or_default()
}

/// An object on the radar: its color (red, green, blue, alpha) and shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blip {
    pub object: u64,
    pub color: [f32; 4],
    pub shape: u32,
}

impl Blip {
    /// An object not yet given a look.
    // D2-02016
    #[must_use]
    pub const fn new(object: u64) -> Self {
        Self {
            object,
            color: [0.0; 4],
            shape: 0,
        }
    }
}

/// A color's texel as the radar's picture holds it: each part times 255, cut to a whole number,
/// in the picture's own order (the color's word as it is: the picture's format is the color's).
// D2-09952
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn texel(c: [f32; 4]) -> [u8; 4] {
    let p = |v: f32| (f64::from(v) * 255.0).clamp(0.0, 255.0) as u8;
    [p(c[2]), p(c[1]), p(c[0]), p(c[3])]
}

/// One texel at `(x, y)`: the radar's drawing paints rectangles.
const fn at(x: i32, y: i32) -> Rect {
    Rect::new(x, y, x, y)
}

/// The radar's panel.
#[derive(Debug)]
pub struct Radar {
    frame: FrameId,
    /// The radar, and the dial over it.
    pub root: Option<Handle>,
    pub heading: Option<Handle>,
    /// The part the coordinates' box and the tooltip belong to, and where it stands in the radar.
    base: Option<Handle>,
    offset: (i32, i32),
    /// The radar's middle and radius on its picture; how often the groove is worked out again.
    center: (i32, i32),
    radius: i32,
    groove_delay: f32,
    /// The objects on the radar, and those with a groove level.
    infos: Vec<Blip>,
    grooves: Vec<(u64, i8)>,
    groove_level: i8,
    next_groove: f64,
    /// What was last shown: the heading, the place line, the tooltip.
    old_heading: f32,
    old_place: Option<String>,
    old_tooltip: Option<String>,
    /// The object whose blip is under the pointer.
    last_under_mouse: u64,
    /// The player's options are to be read (at the start).
    reload: bool,
    /// The objects the radar knows, to see new ones come.
    known: Vec<u64>,
    /// What the listener heard this frame, acted on with the frame's view of the world.
    mail: Vec<Mail>,
}

impl Radar {
    /// The radar under the play screen: the radar from its record, its middle, radius and groove
    /// delay from its attributes, its coordinates' part, the dial; the presses and the button
    /// heard, the clock heard.
    // D2-02054
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::RADAR_FIELD);
        let mut me = Self {
            frame,
            root,
            heading: None,
            base: None,
            offset: (0, 0),
            center: (0, 0),
            radius: 0,
            groove_delay: 2.0,
            infos: Vec::new(),
            grooves: Vec::new(),
            groove_level: 0,
            next_groove: 0.0,
            old_heading: f32::NAN,
            old_place: None,
            old_tooltip: None,
            last_under_mouse: 0,
            reload: true,
            known: Vec::new(),
            mail: Vec::new(),
        };
        if let Some(r) = root {
            if let Some(n) = ui.node(r) {
                me.center = (
                    n.int_attr(name::RADAR_CENTER_X).unwrap_or(0),
                    n.int_attr(name::RADAR_CENTER_Y).unwrap_or(0),
                );
                me.radius = n.int_attr(name::RADAR_RADIUS).unwrap_or(0);
                me.groove_delay = n.float_attr(name::GROOVE_DELAY).unwrap_or(2.0);
            }
            me.base = ui.child_recursive(r, name::BASE_FIELD);
            if let Some(b) = me.base.and_then(|b| ui.node(b)) {
                me.offset = (b.rect.x0, b.rect.y0);
            }
        }
        me.heading = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::HEADING_FIELD);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::BASE_FIELD, msg::MOUSE_PRESS_TOP);
            ui.listen_for_message(l, name::COORD_BOX_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::BASE_FIELD, msg::LOCATION_CHANGED);
            ui.listen_for_global_message(l, global::TIME);
        }
        me
    }

    /// The radar's place line under its dial, when it changed.
    // D2-02045
    pub fn set_place(&mut self, ui: &mut Ui, text: &str) {
        if self.old_place.as_deref() == Some(text) {
            return;
        }
        self.old_place = Some(text.to_owned());
        let Some(r) = self.root else { return };
        if let Some(t) = ui.child_recursive(r, name::COORDINATES_TEXT) {
            ui.set_text(t, text);
        }
    }

    /// The objects on the radar.
    #[must_use]
    pub fn infos(&self) -> &[Blip] {
        &self.infos
    }

    /// The groove level the objects in range add up to.
    #[must_use]
    pub const fn groove_level(&self) -> i8 {
        self.groove_level
    }

    /// The radar shown or hidden: the radar and its dial with it.
    // D2-02017
    fn show_parts(&self, ui: &mut Ui) {
        let shown = ui.frame_shown(self.frame);
        for h in [self.root, self.heading].into_iter().flatten() {
            ui.set_visible(h, shown);
        }
    }

    /// The coordinates' button's tooltip: whether the place line shows.
    // D2-02027
    fn set_coord_button_tooltip(ui: &mut Ui, button: Handle, shown: bool) {
        let table = ui
            .assets()
            .data_id_by_enum(TOOLTIP_TABLE_ENUM, crate::flow::TABLE_MAPPING);
        let id = if shown {
            "ID_Radar_TT_CurLoc_Shown"
        } else {
            "ID_Radar_TT_CurLoc_Hidden"
        };
        let info = auberean_text::StringInfo::entry(table, Token::of(id).raw());
        let text = ui.assets().write(&info).unwrap_or_default();
        ui.set_tooltip(button, &text);
    }

    /// Start over: the objects and their grooves forgotten, the coordinates' button's tooltip
    /// set, every object of the world added.
    // D2-02051
    fn init(&mut self, ui: &mut Ui, world: &World) {
        self.infos.clear();
        self.grooves.clear();
        self.known.clear();
        let Some(r) = self.root else { return };
        let Some(button) = ui.child_recursive(r, name::COORD_BOX_BUTTON) else {
            return;
        };
        let open = self
            .base
            .and_then(|b| ui.node(b))
            .is_some_and(|n| n.state == name::OPEN);
        Self::set_coord_button_tooltip(ui, button, open);
        for t in &world.things {
            self.add_object(world, t.id);
        }
    }

    /// An object comes onto the radar: its groove level kept when it has one; on the radar when
    /// its scripts say how it shows (and it does show).
    // D2-02049, D2-02050
    fn add_object(&mut self, world: &World, id: u64) {
        let Some(t) = world.thing(id) else { return };
        if !self.known.contains(&id) {
            self.known.push(id);
        }
        if t.groove != 0 && !self.grooves.iter().any(|(o, _)| *o == id) {
            self.grooves.push((id, t.groove));
        }
        if let Some((color, shape)) = t.radar {
            if shape != NO_BLIP && !self.infos.iter().any(|i| i.object == id) {
                self.infos.push(Blip {
                    object: id,
                    color,
                    shape,
                });
            }
        }
    }

    /// An object leaves the radar (and its groove), the last taking its place.
    // D2-02034, D2-02035
    fn remove_object(&mut self, id: u64) {
        if let Some(i) = self.grooves.iter().position(|(o, _)| *o == id) {
            self.grooves.swap_remove(i);
        }
        if let Some(i) = self.infos.iter().position(|r| r.object == id) {
            self.infos.swap_remove(i);
        }
        self.known.retain(|o| *o != id);
    }

    /// An object's look changed: the player's starts the radar over; another's is taken off and
    /// put on again.
    // D2-02052
    fn change_radar_look(&mut self, ui: &mut Ui, world: &World, id: u64) {
        if id == world.player {
            self.init(ui, world);
            return;
        }
        self.remove_object(id);
        self.add_object(world, id);
    }

    /// The player's options came back: the place line read again, its box opened or closed as
    /// they say, the radar started over.
    // D2-02053, D2-02055
    fn reload_options(&mut self, ui: &mut Ui, world: &World) {
        self.old_place = None;
        if let Some(b) = self.base {
            let state = if world.show_coordinates {
                name::OPEN
            } else {
                name::CLOSED
            };
            ui.set_state(b, state);
            self.init(ui, world);
        }
    }

    /// The groove level: what the objects in range add up to; the music told when it changed;
    /// worked out again after the delay.
    // D2-02033
    fn update_groove(&mut self, ui: &Ui, world: &World, out: &mut Vec<Request>) {
        if world.player == 0 {
            return;
        }
        let range = world.range();
        let mut level: i8 = 0;
        for (id, g) in &self.grooves {
            let Some(t) = world.thing(*id) else { continue };
            let [x, y, _] = t.from_player;
            if x * x + y * y <= range * range {
                level = level.wrapping_add(*g);
            }
        }
        if level != self.groove_level {
            self.groove_level = level;
            out.push(Request::GrooveLevel(i32::from(level)));
        }
        self.next_groove = ui.now() + f64::from(self.groove_delay);
    }

    /// Draw the radar: the place line, the dial at the radar's corner turned with the player's
    /// heading, each object on the radar in range as its blip (the selection's bracketed), and
    /// the tooltip naming the blip under the pointer.
    // D2-02045
    pub fn update_radar(&mut self, ui: &mut Ui, world: &World) {
        let (Some(root), Some(heading)) = (self.root, self.heading) else {
            return;
        };
        if !ui.frame_shown(self.frame) || world.player == 0 {
            return;
        }
        let range = world.range();
        let range_sq = range * range;
        let place = map_coordinates(ui, world.cell, world.blocks);
        self.set_place(ui, &place);
        let (ax, ay) = ui.object_origin(root).unwrap_or((0, 0));
        ui.move_to(heading, self.offset.0 + ax, self.offset.1 + ay);
        if self.old_heading.to_bits() != world.heading.to_bits() {
            self.old_heading = world.heading;
            ui.set_rotation(heading, dereth_primitives::num::to_i32(world.heading));
        }
        self.last_under_mouse = 0;
        let mut tooltip = String::new();
        let (px, py) = ui.pointer();
        let (mx, my) = (px - ax, py - ay);
        #[allow(clippy::cast_precision_loss)]
        let scale = self.radius as f32 / range;
        let (cx, cy, r) = (self.center.0, self.center.1, self.radius);
        let mut fills = Vec::new();
        for info in &self.infos {
            let Some(t) = world.thing(info.object) else {
                continue;
            };
            let [x, y, _] = t.from_player;
            if x * x + y * y > range_sq {
                continue;
            }
            let bx = cx + dereth_primitives::num::to_i32_f64(f64::from(x) * f64::from(scale));
            let by = cy - dereth_primitives::num::to_i32_f64(f64::from(y) * f64::from(scale));
            if bx < cx - r || bx > cx + r || by < cy - r || by > cy + r {
                continue;
            }
            let (dx, dy) = (mx - bx, my - by);
            if (dx * dx + dy * dy).cast_unsigned() <= 16 && self.last_under_mouse == 0 {
                self.last_under_mouse = t.id;
                tooltip.clone_from(&t.name);
            }
            // The selection as the scripts last said it.
            // D2-02019
            draw_blip(&mut fills, info, bx, by, t.id == world.selection);
        }
        ui.set_fills(root, fills);
        if self.old_tooltip.as_deref() != Some(tooltip.as_str()) {
            if let Some(b) = self.base {
                ui.set_tooltip(b, &tooltip);
                self.old_tooltip = Some(tooltip);
            }
        }
    }

    /// What the radar's listener heard: the coordinates' button opens or closes the place line
    /// (its tooltip turning, the player's option kept); a press on the radar selects the blip
    /// under the pointer; the radar moved, the places are kept; the clock draws the radar again
    /// and works out the groove when it is due.
    // D2-02040, D2-02048
    fn hear(&mut self, ui: &mut Ui, world: &World, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element {
                from,
                element,
                what,
                ..
            } => {
                if element == name::COORD_BOX_BUTTON && what == msg::BUTTON_RELEASE_ON {
                    if world.player == 0 {
                        return;
                    }
                    self.old_place = None;
                    let Some(b) = self.base else { return };
                    let open = ui.node(b).is_some_and(|n| n.state != name::OPEN);
                    ui.set_state(b, if open { name::OPEN } else { name::CLOSED });
                    Self::set_coord_button_tooltip(ui, from, open);
                    out.push(Request::ShowCoordinates(open));
                    out.push(Request::KeepOptions);
                } else if element == name::BASE_FIELD && what == msg::MOUSE_PRESS_TOP {
                    if self.last_under_mouse != 0 && world.player != 0 {
                        out.push(Request::SetSelection(self.last_under_mouse));
                    }
                } else if element == name::BASE_FIELD && what == msg::LOCATION_CHANGED {
                    crate::gameplay::GamePlay::save_locations_changed(out);
                }
            }
            // The kept places put back: the radar stands where it was kept, its dial with it.
            // D2-02018
            Mail::PlacesPutBack => {
                if let (Some(root), Some(heading)) = (self.root, self.heading) {
                    let (ax, ay) = ui.object_origin(root).unwrap_or((0, 0));
                    ui.move_to(heading, self.offset.0 + ax, self.offset.1 + ay);
                    let shown = ui.shown(root);
                    self.show(ui, shown, out);
                }
            }
            Mail::Global {
                what: global::TIME, ..
            } => {
                if ui.frame_shown(self.frame) {
                    self.update_radar(ui, world);
                }
                if self.next_groove < ui.now() {
                    self.update_groove(ui, world, out);
                }
            }
            _ => {}
        }
    }

    /// A notice of the player's scripts about any object.
    fn notice_about(&mut self, ui: &mut Ui, world: &World, n: &Notice) {
        match n.native {
            notice::CHANGE_RADAR_LOOK => {
                if let Some(id) = n.about {
                    self.change_radar_look(ui, world, id);
                }
            }
            notice::RELOAD_OPTIONS | crate::options::notice::OPTIONS_CHANGED
                if n.about == Some(world.player) =>
            {
                self.reload_options(ui, world);
            }
            _ => {}
        }
    }
}

/// One object's blip at `(x, y)` of the radar's picture, by its shape, bracketed when it is the
/// selection.
// D2-02043
fn draw_blip(fills: &mut Vec<(Rect, [u8; 4])>, info: &Blip, x: i32, y: i32, selected: bool) {
    let c = texel(info.color);
    let mut put = |r: Rect| fills.push((r, c));
    // D2-02022
    let edges = |put: &mut dyn FnMut(Rect)| {
        put(at(x, y - 1));
        put(at(x, y + 1));
        put(at(x - 1, y));
        put(at(x + 1, y));
    };
    // D2-02023
    let corners = |put: &mut dyn FnMut(Rect)| {
        put(at(x + 1, y + 1));
        put(at(x - 1, y - 1));
        put(at(x - 1, y + 1));
        put(at(x + 1, y - 1));
    };
    match info.shape {
        // D2-02020
        shape::POINT => put(at(x, y)),
        // D2-02041
        shape::FILL => {
            put(at(x, y));
            edges(&mut put);
            corners(&mut put);
        }
        // D2-02036
        shape::CROSS => {
            put(at(x, y));
            edges(&mut put);
        }
        // D2-02037
        shape::X => {
            put(at(x, y));
            corners(&mut put);
        }
        // D2-02038
        shape::HOLLOW => {
            edges(&mut put);
            corners(&mut put);
        }
        // D2-02021
        shape::L => {
            put(at(x, y + 1));
            put(Rect::new(x - 1, y - 1, x - 1, y + 1));
        }
        shape::EDGES => edges(&mut put),
        shape::CORNERS => corners(&mut put),
        // D2-02024
        shape::T => {
            put(Rect::new(x - 1, y - 1, x + 1, y - 1));
            put(Rect::new(x, y - 1, x, y + 1));
        }
        // D2-02025
        shape::FACE => {
            put(Rect::new(x - 1, y + 1, x + 1, y + 1));
            put(at(x - 1, y - 1));
            put(at(x + 1, y - 1));
        }
        _ => {}
    }
    if selected {
        // D2-02026
        put(Rect::new(x - 2, y + 3, x + 2, y + 3));
        put(Rect::new(x + 3, y - 2, x + 3, y + 2));
        put(Rect::new(x - 2, y - 3, x + 2, y - 3));
        put(Rect::new(x - 3, y - 2, x - 3, y + 2));
    }
}

impl Radar {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x1_040B, 0x443, 0x408, 0x417, 0x46D, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Radar {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        self.show_parts(ui);
    }

    fn listen(&mut self, _ui: &mut Ui, mail: &Mail, _out: &mut Vec<Request>) {
        self.mail.push(*mail);
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let Some(world) = view.world else {
            self.mail.clear();
            if let Some(p) = view.place {
                self.set_place(ui, p);
            }
            return;
        };
        if self.reload {
            self.reload = false;
            self.reload_options(ui, world);
        }
        // Objects come and go.
        for gone in view.gone {
            self.remove_object(*gone);
        }
        let fresh: Vec<u64> = world
            .things
            .iter()
            .map(|t| t.id)
            .filter(|id| !self.known.contains(id))
            .collect();
        for id in fresh {
            self.add_object(world, id);
        }
        for n in view.everyone {
            self.notice_about(ui, world, n);
        }
        for m in std::mem::take(&mut self.mail) {
            self.hear(ui, world, &m, out);
        }
    }

    // D2-02032, D2-02039
    fn close(self: Box<Self>, ui: &mut Ui) {
        for h in [self.root, self.heading].into_iter().flatten() {
            ui.frame_remove_root(self.frame, h);
            ui.remove(h);
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
