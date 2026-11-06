//! Names over heads, and the selection's marks: the words the play screen sets on the world.
//!
//! **The selection.** The selected object's name stands over it in the color its radar look
//! gives (white without one), with the selection's health and vigor meters under the name once
//! the player's scripts send them. Four marks point out from the corners of the box the object
//! fills on the screen; the name and the meters stand centered over the box. The box is the one
//! the object's drawn shape fills, averaged over the last few frames so it does not shake as the
//! object moves. Nothing is placed above the screen's top band or below its foot (what the
//! toolbar covers), nor off its sides. When no corner of the box is on the screen an arrow on the
//! screen's edge points the way to it (on a corner, between the two ways). An object farther
//! than the greatest selection distance (400 meters) is no longer selected.
//!
//! **Names over heads** (their key shows and hides them, kept in the player's options): every
//! object the scripts put on the radar, within the radar's range on the ground, has its name over
//! its box in its radar color; a name that would lie over another name (or the selection's marks)
//! is moved off it the shortest way it may go, a number of times at most, and a name off the
//! screen is not shown. A click on a name selects its object.
//!
//! **Damage text** (the scripts send it when a hit is shown): its text, in its color, over the
//! struck object for its duration, rising three of its own heights over its life (above the
//! selection's name, meters and marks when the object is the selection), moved off the other
//! damage texts as names are moved off names; hidden in portal space, where it does not age, and
//! for an object not in the world. It is drawn at its scale and fades out over its life.

use std::any::Any;
use std::collections::VecDeque;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::gameplay::{Child, Notice, Record, Request, View, PLAY_LAYOUT_ENUM};
use crate::world::{clip_to_pixel, Thing, World, NO_BLIP};

mod name {
    use auberean_ui::Token;
    pub const TARGETTING_RETICLE: Token = Token::of("TargettingReticle");
    pub const OFF_SCREEN_INDICATOR: Token = Token::of("OffScreenIndicator");
    pub const SELECTION_NAME: Token = Token::of("SelectionName");
    pub const SELECTION_STUFF: Token = Token::of("SelectionStuff");
    pub const NAME_TEXT_TEMPLATE: Token = Token::of("NameTextTemplate");
    pub const DAMAGE_TEXT_TEMPLATE: Token = Token::of("DamageTextTemplate");
    pub const AVERAGE_BBOX_FRAMES: Token = Token::of("AverageBBoxFrames");
    pub const MAX_NAME_PLACEMENT_ATTEMPTS: Token = Token::of("MaxNamePlacementAttempts");
    pub const FLOATY_TEXT_TRAVEL: Token = Token::of("FloatyTextTravel");
    pub const MAX_SELECTION_DISTANCE: Token = Token::of("MaxSelectionDistance");
    pub const OBJECT_ID: Token = Token::of("ObjectID");
    pub const HEALTH_METER: Token = Token::of("SelectionHealthMeter");
    pub const PK_DAMAGE_METER: Token = Token::of("SelectionPKDamageMeter");
    pub const VIGOR_METER: Token = Token::of("SelectionVigorMeter");
    pub const PK_VIGOR_LOSS_METER: Token = Token::of("SelectionPKVigorlossMeter");
    pub const RIGHT_FILL: Token = Token::of("right_fill");
}

/// The interface actions the names hear.
pub mod action {
    /// Show and hide the names over heads.
    pub const SHOW_NAMES: u32 = 0x4100_000F;
}

/// The player's scripts' notices the names hear.
pub mod notice {
    /// The selection changed (about the player).
    pub const SELECTION_CHANGED: u32 = 0x0085 << 16 | 0x0003;
    /// An object's radar look changed (about the player: everyone's).
    pub const CHANGE_RADAR_LOOK: u32 = 0x0085 << 16 | 0x000A;
    /// The selection's vitals: its words are the selection's kind and its six numbers (current
    /// health, health lost to players, greatest health, current vigor, vigor lost to players,
    /// greatest vigor), as the interface reads them from the scripts' selection.
    pub const UPDATE_SELECTION_INFO: u32 = 0x0085 << 16 | 0x000B;
    /// The player's options came back.
    pub const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
    /// An item changed (its quantity among others).
    pub const ITEM_UPDATE: u32 = 0x008B << 16 | 0x0004;
    /// A hit's damage text to show over the object it is about.
    pub const DO_3D_DAMAGE: u32 = 0x008D << 16 | 0x0001;
}

/// A hit's damage text as the scripts filled it: the text, its color (red, green, blue, alpha),
/// how long it shows (seconds) and its scale.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageBlob {
    pub text: String,
    pub color: [f32; 4],
    pub duration: f32,
    pub scale: f32,
}

/// A damage text shown: its element, when it began (the interface's clock), how long it lasts,
/// and the object it rises over.
// D2-02105
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageText {
    pub element: Handle,
    pub start: f64,
    pub duration: f64,
    /// How large it is drawn.
    pub scale: f32,
    pub object: u64,
}

/// The kind of selection whose vitals are shown.
pub const SELECTION_WITH_NUMBERS: u32 = 0x4000_0001;

/// A rectangle of the screen with its edges inclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
// D2-02058
pub struct Box2D {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Box2D {
    /// A box `w` by `h` at `(x, y)`.
    // D2-02057
    #[must_use]
    pub const fn sized(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + w - 1,
            y1: y + h - 1,
        }
    }

    /// Whether two boxes overlap (neither empty).
    // D2-02059
    #[must_use]
    pub const fn collide(&self, other: &Self) -> bool {
        if self.x1 < self.x0 || self.y1 < self.y0 || other.x1 < other.x0 || other.y1 < other.y0 {
            return false;
        }
        self.x0 <= other.x1 && self.x1 >= other.x0 && self.y0 <= other.y1 && self.y1 >= other.y0
    }

    /// Where two boxes overlap, when they do.
    // D2-09620
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let b = Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        (b.x1 >= b.x0 && b.y1 >= b.y0).then_some(b)
    }

    /// Whether `other` lies wholly inside this box.
    // D2-11193
    #[must_use]
    pub const fn includes(&self, other: &Self) -> bool {
        self.x0 <= other.x0 && other.x1 <= self.x1 && self.y0 <= other.y0 && other.y1 <= self.y1
    }

    /// What is left of this box with `other` taken out, as up to four boxes: the part left of it
    /// and the part right of it at this box's whole height, then below and above it between them.
    /// Nothing when the two do not overlap.
    // D2-11229
    #[must_use]
    pub fn subtract(&self, other: &Self) -> Vec<Self> {
        let mut left = Vec::new();
        if !self.collide(other) {
            return left;
        }
        let (mut x0, mut x1) = (self.x0, self.x1);
        if x0 <= other.x0 {
            left.push(Self {
                x1: other.x0 - 1,
                ..*self
            });
            x0 = other.x0;
        }
        if other.x1 <= x1 {
            left.push(Self {
                x0: other.x1 + 1,
                ..*self
            });
            x1 = other.x1;
        }
        if self.y0 <= other.y0 {
            left.push(Self {
                x0,
                x1,
                y0: self.y0,
                y1: other.y0 - 1,
            });
        }
        if other.y1 <= self.y1 {
            left.push(Self {
                x0,
                x1,
                y0: other.y1 + 1,
                y1: self.y1,
            });
        }
        left
    }
}

/// The first of the boxes placed that `b` lies over.
// D2-02071
#[must_use]
pub fn place_here(b: &Box2D, placed: &[Box2D]) -> Option<usize> {
    placed.iter().position(|p| p.collide(b))
}

/// Move a box off one it lies over the shortest way among the ways still allowed (`ways`, a bit
/// each: left, up, right, down; ties to the first), and stop it going back the way it came.
// D2-02065
pub fn make_new_data(b: &mut Box2D, over: &Box2D, ways: &mut u32) {
    let d = [
        (b.x1 - over.x0 - 1).abs(),
        (b.y1 - over.y0 - 1).abs(),
        (over.x1 - b.x0 + 1).abs(),
        (over.y1 - b.y0 + 1).abs(),
    ];
    let mut way = (0..4).find(|w| *ways & (1 << w) != 0).unwrap_or(4);
    let mut best = d.get(way).copied().unwrap_or(0);
    for (w, dw) in d.iter().enumerate().skip(way + 1) {
        if *ways & (1 << w) != 0 && *dw < best {
            best = *dw;
            way = w;
        }
    }
    let (dx, dy) = match way {
        0 => (-best, 0),
        2 => (best, 0),
        1 => (0, -best),
        3 => (0, best),
        _ => (0, 0),
    };
    b.x0 += dx;
    b.x1 += dx;
    b.y0 += dy;
    b.y1 += dy;
    *ways &= !(1 << (way.wrapping_sub(2) & 3));
}

/// A box of a body's own frame: least and most corners.
type Bounds = ([f32; 3], [f32; 3]);

/// The box a body's shape fills, averaged over the last few frames (`keep` at most, this one
/// among them).
// D2-02106
fn average_bounding_box(b: Bounds, history: &mut VecDeque<Bounds>, keep: usize) -> Bounds {
    while !history.is_empty() && history.len() >= keep {
        history.pop_front();
    }
    history.push_back(b);
    #[allow(clippy::cast_precision_loss)]
    let k = 1.0 / history.len() as f32;
    let mut lo = [0.0f32; 3];
    let mut hi = [0.0f32; 3];
    for (l, h) in history.iter() {
        for i in 0..3 {
            lo[i] += l[i];
            hi[i] += h[i];
        }
    }
    (lo.map(|v| v * k), hi.map(|v| v * k))
}

/// One name over a head: its element and its object, with the object's recent boxes.
#[derive(Debug)]
pub struct NameText {
    pub element: Handle,
    pub object: u64,
    history: VecDeque<Bounds>,
}

impl NameText {
    /// A name for an object, with no boxes yet.
    // D2-02104
    #[must_use]
    pub const fn new(element: Handle, object: u64) -> Self {
        Self {
            element,
            object,
            history: VecDeque::new(),
        }
    }
}

/// Which parts of the selection's marks are to be hidden once they were placed.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct Hide {
    reticle: bool,
    off_screen: bool,
    name: bool,
    stuff: bool,
}

/// The names' panel.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Names {
    frame: FrameId,
    selection: u64,
    /// The selection's marks: four corners, the arrow off the screen's edge, its name and its
    /// meters.
    pub reticle: [Option<Handle>; 4],
    pub off_screen: Option<Handle>,
    pub selection_name: Option<Handle>,
    pub selection_stuff: Option<Handle>,
    /// Where the selection's marks stand on the screen (names keep off it).
    reticle_box: Box2D,
    /// The selection's recent boxes.
    history: VecDeque<Bounds>,
    /// How many pixels at the screen's top and foot are kept clear.
    preclude_top: i32,
    preclude_bottom: i32,
    /// The selection has meters to show, and a name.
    has_stuff: bool,
    has_name: bool,
    names: Vec<NameText>,
    /// The damage texts shown, oldest first.
    damage: Vec<DamageText>,
    /// The marks' attributes: frames averaged, tries to place a name, the floating text's travel,
    /// the greatest selection distance.
    average_frames: usize,
    max_attempts: i32,
    floaty_travel: i32,
    max_selection_distance: i32,
    /// The radar's ranges, squared: names show within them.
    indoor_sq: f32,
    outdoor_sq: f32,
    showing_names: bool,
    started: bool,
    mail: Vec<Mail>,
    /// The clock's placing waits until the world moved on and its frame's camera is known
    /// ([`Names::place_after_world`]), as the game places them after the world's update.
    pub after_world: bool,
    /// The clock asked for a placing not yet made.
    due: bool,
}

/// Whether a thing shows a name: the scripts put it on the radar.
fn shows(t: &Thing) -> bool {
    t.radar.is_some_and(|(_, s)| s != NO_BLIP)
}

impl Names {
    /// The names' framework under the play screen: a press on a name heard, the names' key and
    /// the clock heard; the selection's marks made (hidden).
    // D2-02138
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::NAME_TEXT_TEMPLATE, msg::MOUSE_PRESS_TOP);
            ui.listen_for_global_message(l, global::TIME);
            ui.listen_for_global_message(l, global::KEY_DOWN);
            ui.listen_for_global_message(l, global::KEY_UP);
        }
        let mut me = Self {
            frame,
            selection: 0,
            reticle: [None; 4],
            off_screen: None,
            selection_name: None,
            selection_stuff: None,
            reticle_box: Box2D::default(),
            history: VecDeque::new(),
            preclude_top: 0,
            preclude_bottom: 0,
            has_stuff: false,
            has_name: false,
            names: Vec::new(),
            damage: Vec::new(),
            average_frames: 1,
            max_attempts: 0,
            floaty_travel: 0,
            max_selection_distance: 0,
            indoor_sq: 0.0,
            outdoor_sq: 0.0,
            showing_names: false,
            started: false,
            mail: Vec::new(),
            after_world: false,
            due: false,
        };
        me.init_reticle(ui);
        me
    }

    /// The names over heads.
    #[must_use]
    pub fn names(&self) -> &[NameText] {
        &self.names
    }

    /// Whether names show over heads.
    #[must_use]
    pub const fn showing_names(&self) -> bool {
        self.showing_names
    }

    /// The text that floats up from a struck object travels this many times its height.
    #[must_use]
    pub const fn floaty_travel(&self) -> i32 {
        self.floaty_travel
    }

    fn root(ui: &mut Ui, frame: FrameId, n: Token) -> Option<Handle> {
        ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, n)
    }

    /// Show or hide the four corner marks.
    // D2-02062
    fn show_reticle(&self, ui: &mut Ui, on: bool) {
        for h in self.reticle.into_iter().flatten() {
            ui.set_visible(h, on);
        }
    }

    /// The selection's marks made: the four corners, the arrow off the edge, the name, the
    /// meters; the attributes read from the first corner; all hidden.
    // D2-02069
    fn init_reticle(&mut self, ui: &mut Ui) {
        for r in &mut self.reticle {
            if let Some(h) = Self::root(ui, self.frame, name::TARGETTING_RETICLE) {
                *r = Some(h);
            }
        }
        self.off_screen = Self::root(ui, self.frame, name::OFF_SCREEN_INDICATOR);
        self.selection_name = Self::root(ui, self.frame, name::SELECTION_NAME);
        self.selection_stuff = Self::root(ui, self.frame, name::SELECTION_STUFF);
        if self.off_screen.is_none()
            || self.selection_name.is_none()
            || self.selection_stuff.is_none()
        {
            return;
        }
        if let Some(n) = self.reticle[0].and_then(|r| ui.node(r)) {
            self.average_frames =
                usize::try_from(n.int_attr(name::AVERAGE_BBOX_FRAMES).unwrap_or(1)).unwrap_or(1);
            self.max_attempts = n.int_attr(name::MAX_NAME_PLACEMENT_ATTEMPTS).unwrap_or(0);
            self.floaty_travel = n.int_attr(name::FLOATY_TEXT_TRAVEL).unwrap_or(0);
            self.max_selection_distance = n.int_attr(name::MAX_SELECTION_DISTANCE).unwrap_or(0);
        }
        self.show_reticle(ui, false);
        for h in [self.off_screen, self.selection_name, self.selection_stuff]
            .into_iter()
            .flatten()
        {
            ui.set_visible(h, false);
        }
    }

    /// Start over: the names gone, the radar's ranges read again (squared).
    // D2-02133
    fn init(&mut self, ui: &mut Ui, world: &World) {
        self.clean_up(ui);
        self.indoor_sq = world.ranges.0 * world.ranges.0;
        self.outdoor_sq = world.ranges.1 * world.ranges.1;
    }

    /// Every name gone.
    // D2-02125
    fn clean_up(&mut self, ui: &mut Ui) {
        self.clear_names(ui);
        for d in std::mem::take(&mut self.damage) {
            ui.frame_remove_root(self.frame, d.element);
            ui.remove(d.element);
        }
    }

    /// The damage texts shown.
    #[must_use]
    pub fn damage(&self) -> &[DamageText] {
        &self.damage
    }

    /// A hit's damage text over object `object`: an element of the damage text template in the
    /// text's color, sized to its text, kept with when it began; then everything placed again.
    // D2-02129
    pub fn do_3d_damage(&mut self, ui: &mut Ui, world: &World, object: u64, b: &DamageBlob) {
        let Some(e) = Self::root(ui, self.frame, name::DAMAGE_TEXT_TEMPLATE) else {
            return;
        };
        set_color(ui, e, b.color);
        if let Some((w, h)) = ui.inq_size_with_margins(e, &b.text) {
            ui.resize_to(e, w, h);
        }
        ui.set_text(e, &b.text);
        ui.set_object_scale(e, b.scale);
        ui.set_visible(e, false);
        self.damage.push(DamageText {
            element: e,
            start: ui.now(),
            duration: f64::from(b.duration),
            scale: b.scale,
            object,
        });
        self.place_all(ui, world, &mut Vec::new());
    }

    /// Object `object`'s damage texts gone.
    fn remove_damage(&mut self, ui: &mut Ui, object: u64) {
        let (gone, kept): (Vec<DamageText>, Vec<DamageText>) = std::mem::take(&mut self.damage)
            .into_iter()
            .partition(|d| d.object == object);
        for d in gone {
            ui.frame_remove_root(self.frame, d.element);
            ui.remove(d.element);
        }
        self.damage = kept;
    }

    /// The damage texts' frame, oldest first: each hidden, then (out of portal space) aged: one
    /// whose life is over goes; the rest rise with their age over their object in the world, above
    /// the selection's marks when it is the selection, moved off the damage texts placed before
    /// them, and shown when on the screen.
    // D2-02119
    fn update_damage(&mut self, ui: &mut Ui, world: &World, in_portal: bool) {
        let (w, h) = ui.display();
        let screen = Box2D {
            x0: 0,
            y0: 0,
            x1: w - 1,
            y1: h - 1,
        };
        let now = ui.now();
        let mut placed: Vec<Box2D> = Vec::new();
        let mut kept = Vec::with_capacity(self.damage.len());
        for d in std::mem::take(&mut self.damage) {
            ui.set_visible(d.element, false);
            if in_portal {
                kept.push(d);
                continue;
            }
            let age = ((now - d.start) / d.duration).max(0.0);
            if age >= 1.0 {
                ui.frame_remove_root(self.frame, d.element);
                ui.remove(d.element);
                continue;
            }
            kept.push(d);
            let (tw, th) = Self::size_of(ui, Some(d.element));
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let (tw, th) = ((tw as f32 * d.scale) as i32, (th as f32 * d.scale) as i32); // LINT-OK: a size in pixels cuts its fraction
            let rise = dereth_primitives::num::to_i32_f64(f64::from(self.floaty_travel * th) * age);
            let mut above = rise;
            if d.object == self.selection {
                for (on, e) in [
                    (self.has_name, self.selection_name),
                    (self.has_stuff, self.selection_stuff),
                    (true, self.reticle[0]),
                ] {
                    if on {
                        above += Self::size_of(ui, e).1;
                    }
                }
            }
            let Some(t) = world.thing(d.object) else {
                continue;
            };
            let mut history = VecDeque::new();
            let Some((cx, cy)) = self.calc_screen_position(ui, world, t, &mut history, above + th)
            else {
                continue;
            };
            let x0 = dereth_primitives::num::to_i32_f64(f64::from(cx) - f64::from(tw) * 0.5);
            let mut b = Box2D::sized(x0, cy, tw, th);
            let mut ways = 0xF;
            let mut off = false;
            for _ in 0..self.max_attempts {
                if !screen.collide(&b) {
                    off = true;
                    break;
                }
                let Some(i) = place_here(&b, &placed) else {
                    break;
                };
                make_new_data(&mut b, &placed[i], &mut ways);
                if ways == 0 {
                    ways = 0xF;
                }
            }
            if off {
                continue;
            }
            placed.push(b);
            #[allow(clippy::cast_possible_truncation)] // a share of 1
            ui.set_object_alpha(d.element, (1.0 - age) as f32);
            ui.move_picture(d.element, b.x0, b.y0);
            ui.set_visible(d.element, true);
        }
        self.damage = kept;
    }

    /// Every name over a head gone.
    // D2-02120
    fn clear_names(&mut self, ui: &mut Ui) {
        while let Some(n) = self.names.pop() {
            ui.frame_remove_root(self.frame, n.element);
            ui.remove(n.element);
        }
    }

    /// The name over one object's head gone (the last taking its place).
    // D2-02121
    fn remove_name(&mut self, ui: &mut Ui, object: u64) {
        if let Some(i) = self.names.iter().position(|n| n.object == object) {
            let n = self.names.swap_remove(i);
            ui.frame_remove_root(self.frame, n.element);
            ui.remove(n.element);
        }
    }

    /// A name for an object the scripts put on the radar (once, with `once`): its element over
    /// its head, holding its object's id, in its radar color, sized to its name.
    // D2-02127
    fn add_name(&mut self, ui: &mut Ui, t: &Thing, once: bool) {
        if once && self.names.iter().any(|n| n.object == t.id) {
            return;
        }
        let Some((color, _)) = t.radar.filter(|_| shows(t)) else {
            return;
        };
        let Some(e) = Self::root(ui, self.frame, name::NAME_TEXT_TEMPLATE) else {
            return;
        };
        ui.set_attr(e, name::OBJECT_ID, Value::InstanceId(t.id));
        set_color(ui, e, color);
        set_name(ui, e, t);
        ui.set_visible(e, false);
        self.names.push(NameText::new(e, t.id));
    }

    /// A name for every object the scripts put on the radar.
    // D2-02134
    fn init_names(&mut self, ui: &mut Ui, world: &World) {
        self.clear_names(ui);
        for t in &world.things {
            self.add_name(ui, t, false);
        }
    }

    /// The names' key: names shown, or no longer (the player's option kept).
    // D2-02137
    fn toggle_names(&mut self, ui: &mut Ui, world: &World, out: &mut Vec<Request>) {
        if world.player == 0 {
            return;
        }
        if self.showing_names {
            self.clear_names(ui);
            out.push(Request::ShowNames(false));
            self.showing_names = false;
        } else {
            self.init_names(ui, world);
            out.push(Request::ShowNames(true));
            self.showing_names = true;
        }
    }

    /// The selection's name in its radar color (white without one), sized to it.
    // D2-02063
    fn check_name(&mut self, ui: &mut Ui, world: &World) {
        self.has_name = false;
        let Some(t) = world.thing(self.selection) else {
            return;
        };
        let Some(e) = self.selection_name else { return };
        let color = t.radar.map_or([1.0; 4], |(c, _)| c);
        set_color(ui, e, color);
        set_name(ui, e, t);
        self.has_name = true;
    }

    /// One of the selection's meters set to a share: `current` (no more than `greatest`) over
    /// `greatest`; a meter that fills from the right takes what is left.
    // D2-02064
    fn do_stuff_meter(&self, ui: &mut Ui, meter: Token, current: u32, greatest: u32) {
        let Some(m) = self
            .selection_stuff
            .and_then(|s| ui.child_recursive(s, meter))
        else {
            return;
        };
        let current = current.min(greatest);
        #[allow(clippy::cast_precision_loss)]
        let mut share = if greatest == 0 {
            0.0
        } else {
            current as f32 / greatest as f32
        };
        if ui.child(m, name::RIGHT_FILL).is_some() {
            share = 1.0 - share;
        }
        ui.set_attr(m, attr::GOAL_POSITION, Value::Float(share));
        ui.set_meter(m, share);
    }

    /// The selection's vitals: its meters, once it is a kind that has them.
    // D2-02070
    pub fn update_selection_info(&mut self, ui: &mut Ui, words: &[u32]) {
        let w = |i: usize| words.get(i).copied().unwrap_or(0);
        if w(0) != SELECTION_WITH_NUMBERS {
            return;
        }
        self.has_stuff = true;
        self.do_stuff_meter(ui, name::HEALTH_METER, w(1), w(3));
        self.do_stuff_meter(ui, name::PK_DAMAGE_METER, w(2), w(3));
        self.do_stuff_meter(ui, name::VIGOR_METER, w(4), w(6));
        self.do_stuff_meter(ui, name::PK_VIGOR_LOSS_METER, w(5), w(6));
    }

    /// The selection changed: its name read again, no meters until the scripts send them, its
    /// boxes forgotten; the names made again; the marks placed.
    // D2-02136
    fn selection_changed(&mut self, ui: &mut Ui, world: &World) {
        self.selection = world.selection;
        self.has_stuff = false;
        self.check_name(ui, world);
        self.history.clear();
        if self.showing_names {
            self.init_names(ui, world);
        }
        self.place_all(ui, world, &mut Vec::new());
    }

    /// An object's radar look changed: the selection's or the player's name read again; its name
    /// over its head made again (the player's: everyone's).
    // D2-02135
    fn change_radar_look(&mut self, ui: &mut Ui, world: &World, object: u64) {
        if object == self.selection || object == world.player {
            self.check_name(ui, world);
        }
        if self.showing_names {
            if object == world.player {
                self.init_names(ui, world);
                return;
            }
            self.remove_name(ui, object);
            if let Some(t) = world.thing(object) {
                self.add_name(ui, t, true);
            }
        }
    }

    /// The player's options came back: names shown or not as they say.
    // D2-02141
    fn reload_options(&mut self, ui: &mut Ui, world: &World) {
        self.showing_names = world.show_names;
        if self.showing_names {
            self.init_names(ui, world);
        }
    }

    /// The box an object fills on the screen (clip square: x and y from -1 to 1), its recent
    /// boxes averaged; and whether any corner of it is on the screen. `None` when it is not
    /// drawn or the world has no camera yet.
    // D2-02108, D2-13233, D2-06337, D2-13225, D2-08173, D2-08174, D2-08175
    fn object_to_window(
        world: &World,
        t: &Thing,
        history: &mut VecDeque<Bounds>,
        keep: usize,
        aspect: f32,
    ) -> Option<([f32; 4], bool)> {
        let camera = world.camera?;
        let body = t.body?;
        let b = average_bounding_box(body.bounds?, history, keep);
        let mut out = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        let mut any = false;
        for i in 0..8 {
            let p = [
                if i & 1 == 0 { b.0[0] } else { b.1[0] },
                if i & 2 == 0 { b.0[1] } else { b.1[1] },
                if i & 4 == 0 { b.0[2] } else { b.1[2] },
            ];
            let (c, on) = camera.clip(body.to_block(p), aspect);
            any |= on;
            out = [
                out[0].min(c[0]),
                out[1].min(c[1]),
                out[2].max(c[0]),
                out[3].max(c[1]),
            ];
        }
        Some((out, any))
    }

    /// Where an object's name or text of `height` goes: centered over the middle of its box on the
    /// screen, standing on its top, kept on the screen and clear of its top band and its foot.
    // D2-02111
    fn calc_screen_position(
        &self,
        ui: &Ui,
        world: &World,
        t: &Thing,
        history: &mut VecDeque<Bounds>,
        height: i32,
    ) -> Option<(i32, i32)> {
        let (w, h) = ui.display();
        #[allow(clippy::cast_precision_loss)]
        let aspect = w as f32 / h.max(1) as f32;
        let (b, any) = Self::object_to_window(world, t, history, self.average_frames, aspect)?;
        if !any {
            return None;
        }
        let (x0, _bottom) = clip_to_pixel([b[0], b[1]], w, h);
        let (x1, top) = clip_to_pixel([b[2], b[3]], w, h);
        let x = ((x1 - x0) / 2 + x0).clamp(0, w);
        let low = h - self.preclude_bottom;
        let mut y = top - height;
        if y < self.preclude_top {
            y = self.preclude_top;
        } else if y > low {
            y = low;
        }
        Some((x, y))
    }

    /// An element's size on the screen.
    fn size_of(ui: &Ui, h: Option<Handle>) -> (i32, i32) {
        h.and_then(|h| ui.screen_rect(h))
            .map_or((0, 0), |r| (r.width(), r.height()))
    }

    /// The arrow at the screen's edge pointing the way to a selection none of whose box is on the
    /// screen (`b`, the clip square's box).
    fn point_off_screen(&mut self, ui: &mut Ui, b: [f32; 4], above: i32, hide: &mut Hide) {
        let Some(o) = self.off_screen else { return };
        let (w, h) = ui.display();
        let (ow, oh) = Self::size_of(ui, Some(o));
        let max_x = w - ow;
        let max_y = h - self.preclude_bottom - oh;
        let min_y = self.preclude_top + above;
        let mid = [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5];
        let (mut px, mut py) = clip_to_pixel(mid, w, h);
        let (mut right, mut left, mut top, mut bottom) = (false, false, false, false);
        if px > max_x {
            right = true;
            px = max_x;
        } else if px < 0 {
            left = true;
            px = 0;
        }
        let angle;
        if py > max_y {
            py = max_y;
            bottom = true;
            angle = 270;
        } else if py < min_y {
            top = true;
            py = min_y;
            angle = if left { 180 } else { 90 };
        } else if left {
            angle = 180;
        } else if right {
            angle = 0;
        } else {
            px = max_x / 2;
            py = max_y;
            bottom = true;
            angle = 270;
        }
        let tilt = if right && bottom {
            45
        } else if (left && (bottom || top)) || (right && top) {
            -45
        } else {
            0
        };
        ui.set_visible(o, true);
        hide.off_screen = false;
        ui.set_rotation(o, angle + tilt);
        ui.move_picture(o, px, py);
        self.reticle_box = Box2D::sized(px, py, ow, oh);
    }

    /// Place the selection's marks (see the module's description); says which parts are to be
    /// hidden after.
    // D2-02110, D2-01972
    fn update_reticle_internal(
        &mut self,
        ui: &mut Ui,
        world: &World,
        in_portal: bool,
        hide: &mut Hide,
        out: &mut Vec<Request>,
    ) {
        if in_portal || self.selection == 0 {
            return;
        }
        let Some(t) = world.thing(self.selection) else {
            return;
        };
        // D2-08176
        if t.faded || t.body.is_none() {
            return;
        }
        let [x, y, _] = t.from_player;
        let far = self.max_selection_distance;
        #[allow(clippy::cast_precision_loss)]
        if ((far * far) as f32) < y * y + x * x {
            out.push(Request::SetSelection(0));
            return;
        }
        let (w, h) = ui.display();
        #[allow(clippy::cast_precision_loss)]
        let aspect = w as f32 / h.max(1) as f32;
        let mut history = std::mem::take(&mut self.history);
        let seen = Self::object_to_window(world, t, &mut history, self.average_frames, aspect);
        self.history = history;
        let Some((b, any)) = seen else { return };
        let name_h = if self.has_name {
            Self::size_of(ui, self.selection_name).1
        } else {
            0
        };
        let stuff_h = if self.has_stuff {
            Self::size_of(ui, self.selection_stuff).1
        } else {
            0
        };
        if !any {
            self.point_off_screen(ui, b, name_h + stuff_h, hide);
            return;
        }
        let (left, bottom) = clip_to_pixel([b[0], b[1]], w, h);
        let (right, top) = clip_to_pixel([b[2], b[3]], w, h);
        let Some(r0) = self.reticle[0] else { return };
        let (rw, rh) = Self::size_of(ui, Some(r0));
        let max_x = w - rw;
        let max_y = h - self.preclude_bottom - rh;
        let (Some(name), Some(stuff)) = (self.selection_name, self.selection_stuff) else {
            return;
        };
        let min_y = self.preclude_top + name_h + stuff_h;
        let mut xs = [left - rw, left - rw, right, right];
        let mut ys = [top - rh, bottom, top - rh, bottom];
        // Each corner's mark turned on from the last: a quarter, a half, three quarters more.
        let (mut turn, mut step) = (0, 0);
        for i in 0..4 {
            if xs[i] < 0 {
                xs[i] = 0;
            } else if xs[i] > max_x {
                xs[i] = max_x;
            }
            if ys[i] < min_y {
                ys[i] = min_y;
            } else if ys[i] > max_y {
                ys[i] = max_y;
            }
            turn += step;
            if let Some(m) = self.reticle[i] {
                ui.set_visible(m, true);
                ui.set_rotation(m, turn);
                ui.move_picture(m, xs[i], ys[i]);
            }
            step += 90;
        }
        self.reticle_box = Box2D {
            x0: xs[0],
            y0: ys[0],
            x1: xs[3],
            y1: ys[3],
        };
        if !self.has_name {
            return;
        }
        let half = (xs[2] - xs[0] + rw) / 2;
        let (nw, nh) = Self::size_of(ui, Some(name));
        let name_x = xs[0] - nw / 2 + half;
        let mut name_y = ys[0] - nh;
        let mut box_x0 = xs[0].min(name_x);
        let mut box_x1 = xs[3].max(name_x + rw - 1);
        if self.has_stuff {
            let (sw, sh) = Self::size_of(ui, Some(stuff));
            name_y -= sh;
            let stuff_x = xs[0] - sw / 2 + half;
            let stuff_y = name_y + nh;
            box_x0 = box_x0.min(stuff_x);
            box_x1 = box_x1.max(stuff_x + rw - 1);
            ui.move_picture(stuff, stuff_x, stuff_y);
            ui.set_visible(stuff, true);
            hide.stuff = false;
        }
        self.reticle_box.x0 = box_x0;
        self.reticle_box.x1 = box_x1;
        self.reticle_box.y0 = name_y;
        ui.move_picture(name, name_x, name_y);
        ui.set_visible(name, true);
        hide.name = false;
        hide.reticle = false;
    }

    /// Place the selection's marks, then hide what was not placed.
    // D2-02114
    fn update_reticle(
        &mut self,
        ui: &mut Ui,
        world: &World,
        in_portal: bool,
        out: &mut Vec<Request>,
    ) {
        let mut hide = Hide {
            reticle: true,
            off_screen: true,
            name: true,
            stuff: true,
        };
        self.update_reticle_internal(ui, world, in_portal, &mut hide, out);
        if hide.reticle {
            self.show_reticle(ui, false);
        }
        for (gone, h) in [
            (hide.off_screen, self.off_screen),
            (hide.name, self.selection_name),
            (hide.stuff, self.selection_stuff),
        ] {
            if let (true, Some(h)) = (gone, h) {
                ui.set_visible(h, false);
            }
        }
    }

    /// Place the names over heads (see the module's description).
    // D2-02113, D2-13180
    fn update_names(&mut self, ui: &mut Ui, world: &World, in_portal: bool) {
        let (w, h) = ui.display();
        let screen = Box2D {
            x0: 0,
            y0: 0,
            x1: w - 1,
            y1: h - 1,
        };
        let mut placed = Vec::new();
        if self.selection != 0 {
            placed.push(self.reticle_box);
        }
        if world.player == 0 {
            return;
        }
        let range_sq = if world.inside {
            self.indoor_sq
        } else {
            self.outdoor_sq
        };
        let mut names = std::mem::take(&mut self.names);
        for n in &mut names {
            let shown = 'place: {
                if in_portal || n.object == self.selection {
                    break 'place false;
                }
                let Some(t) = world.thing(n.object) else {
                    break 'place false;
                };
                let [x, y, _] = t.from_player;
                if x * x + y * y > range_sq {
                    break 'place false;
                }
                let (nw, nh) = Self::size_of(ui, Some(n.element));
                let Some((cx, cy)) = self.calc_screen_position(ui, world, t, &mut n.history, nh)
                else {
                    break 'place false;
                };
                let x0 = dereth_primitives::num::to_i32_f64(f64::from(cx) - f64::from(nw) * 0.5);
                let mut b = Box2D::sized(x0, cy, nw, nh);
                let mut ways = 0xF;
                let mut off = false;
                for _ in 0..self.max_attempts {
                    if !screen.collide(&b) {
                        off = true;
                        break;
                    }
                    let Some(i) = place_here(&b, &placed) else {
                        break;
                    };
                    make_new_data(&mut b, &placed[i], &mut ways);
                    if ways == 0 {
                        ways = 0xF;
                    }
                }
                if off {
                    break 'place false;
                }
                placed.push(b);
                ui.move_picture(n.element, b.x0, b.y0);
                true
            };
            ui.set_visible(n.element, shown);
        }
        self.names = names;
    }

    /// The names' frame: the selection's marks placed, then the names over heads.
    // D2-02126
    pub fn place_all(&mut self, ui: &mut Ui, world: &World, out: &mut Vec<Request>) {
        let in_portal = world.portal_drawn;
        self.update_reticle(ui, world, in_portal, out);
        self.update_damage(ui, world, in_portal);
        if self.showing_names {
            self.update_names(ui, world, in_portal);
        }
    }

    /// The placing the clock asked for, once the world moved on (see [`Names::after_world`]):
    /// made with `world`, which has this frame's camera.
    pub fn place_after_world(&mut self, ui: &mut Ui, world: &World, out: &mut Vec<Request>) {
        if std::mem::take(&mut self.due) {
            self.place_all(ui, world, out);
        }
    }

    /// What the names' listener heard: a press on a name selects its object; the names' key
    /// shows and hides them; the clock places everything again (or asks for it, see
    /// [`Names::after_world`]).
    // D2-02060, D2-02140
    fn hear(&mut self, ui: &mut Ui, world: &World, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element {
                from,
                element,
                what,
                ..
            } if element == name::NAME_TEXT_TEMPLATE && what == msg::MOUSE_PRESS_TOP => {
                let id = ui
                    .node(from)
                    .and_then(|n| n.instance_id_attr(name::OBJECT_ID))
                    .unwrap_or(0);
                if id != 0 && world.player != 0 {
                    out.push(Request::SetSelection(id));
                }
            }
            Mail::Global {
                what: global::KEY_DOWN,
                param: action::SHOW_NAMES,
            } => self.toggle_names(ui, world, out),
            Mail::Global {
                what: global::TIME, ..
            } => {
                if self.after_world {
                    self.due = true;
                } else {
                    self.place_all(ui, world, out);
                }
            }
            _ => {}
        }
    }

    /// A notice of the player's scripts about any object.
    fn notice_about(&mut self, ui: &mut Ui, world: &World, n: &Notice) {
        match n.native {
            notice::SELECTION_CHANGED if n.about == Some(world.player) => {
                self.selection_changed(ui, world);
            }
            notice::UPDATE_SELECTION_INFO if n.about == Some(world.player) => {
                self.update_selection_info(ui, &n.words);
            }
            notice::CHANGE_RADAR_LOOK => {
                if let Some(o) = n.about {
                    self.change_radar_look(ui, world, o);
                }
            }
            notice::RELOAD_OPTIONS | crate::options::notice::OPTIONS_CHANGED
                if n.about == Some(world.player) =>
            {
                self.reload_options(ui, world);
            }
            // An item changing (a stack's count among others) names it anew, as a new look does.
            // D2-02061
            notice::ITEM_UPDATE => {
                if let Some(o) = n.about {
                    self.change_radar_look(ui, world, o);
                }
            }
            notice::DO_3D_DAMAGE => {
                if let (Some(o), Some(Some(Record::DamageText(b)))) = (n.about, n.records.first()) {
                    self.do_3d_damage(ui, world, o, b);
                }
            }
            _ => {}
        }
    }

    /// The screen's top band (`top`) or its foot (`bottom`) kept clear changed: everything placed
    /// again.
    // D2-02130, D2-02131
    pub fn preclude(&mut self, ui: &mut Ui, world: &World, top: i32, bottom: i32) {
        if (top, bottom) == (self.preclude_top, self.preclude_bottom) {
            return;
        }
        self.preclude_top = top;
        self.preclude_bottom = bottom;
        self.place_all(ui, world, &mut Vec::new());
    }
}

/// An object's name in a text element, the element sized to it: its string reference when the
/// scripts gave one (written once the string tables it reads are at hand; sized as it writes now,
/// the client's error message while a table is missing), else its written name.
fn set_name(ui: &mut Ui, e: Handle, t: &Thing) {
    match &t.name_info {
        Some(info) => {
            let text = String::from_utf16_lossy(&ui.assets().element_text(info));
            if let Some((w, h)) = ui.inq_size_with_margins(e, &text) {
                ui.resize_to(e, w, h);
            }
            ui.set_string_info(e, info);
        }
        None => {
            if let Some((w, h)) = ui.inq_size_with_margins(e, &t.name) {
                ui.resize_to(e, w, h);
            }
            ui.set_text(e, &t.name);
        }
    }
}

/// A text element's color: its font's red, green and blue.
fn set_color(ui: &mut Ui, e: Handle, c: [f32; 4]) {
    ui.set_attr(e, attr::FONT_COLOR_RED, Value::Float(c[0]));
    ui.set_attr(e, attr::FONT_COLOR_GREEN, Value::Float(c[1]));
    ui.set_attr(e, attr::FONT_COLOR_BLUE, Value::Float(c[2]));
}

impl Names {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x1_040B, 0x462, 0x463, 0x44D, 0x443, 0x408, 0x1_040C, 0x417, 0x46D, 0x446, 0x454,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Names {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn listen(&mut self, _ui: &mut Ui, mail: &Mail, _out: &mut Vec<Request>) {
        self.mail.push(*mail);
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let Some(world) = view.world else {
            self.mail.clear();
            return;
        };
        if !self.started {
            self.started = true;
            self.selection = world.selection;
            self.init(ui, world);
            self.reload_options(ui, world);
            self.check_name(ui, world);
        }
        self.preclude(ui, world, view.preclude.0, view.preclude.1);
        // Objects come and go.
        for gone in view.gone {
            // D2-02122
            if self.showing_names {
                self.remove_name(ui, *gone);
            }
            self.remove_damage(ui, *gone);
        }
        if self.showing_names {
            // D2-02128
            let fresh: Vec<&Thing> = world
                .things
                .iter()
                .filter(|t| !self.names.iter().any(|n| n.object == t.id))
                .filter(|t| shows(t))
                .collect();
            for t in fresh {
                self.add_name(ui, t, true);
            }
        }
        for n in view.everyone {
            self.notice_about(ui, world, n);
        }
        if world.selection != self.selection {
            self.selection_changed(ui, world);
        }
        // An item chosen in the pack has its name once it lies in the world.
        // D2-02063
        if !self.has_name && world.thing(self.selection).is_some() {
            self.check_name(ui, world);
        }
        for m in std::mem::take(&mut self.mail) {
            self.hear(ui, world, &m, out);
        }
    }

    // D2-02132, D2-02139
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.clean_up(ui);
        let parts = self.reticle.into_iter().chain([
            self.off_screen,
            self.selection_name,
            self.selection_stuff,
        ]);
        for h in parts.flatten() {
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

#[cfg(test)]
mod box_tests {
    //! Behaviour: none (a box's sums, held to hand-worked values)
    use super::Box2D;

    const fn b(x0: i32, y0: i32, x1: i32, y1: i32) -> Box2D {
        Box2D { x0, y0, x1, y1 }
    }

    #[test]
    fn boxes_overlap_include_and_subtract() {
        let big = b(0, 0, 9, 9);
        let inner = b(3, 4, 5, 6);
        assert_eq!(big.intersection(&b(5, 5, 20, 20)), Some(b(5, 5, 9, 9)));
        assert_eq!(big.intersection(&b(10, 0, 20, 9)), None);
        assert!(big.includes(&inner));
        assert!(!inner.includes(&big));
        assert!(!big.includes(&b(5, 5, 10, 6)));
        assert_eq!(
            big.subtract(&inner),
            vec![b(0, 0, 2, 9), b(6, 0, 9, 9), b(3, 0, 5, 3), b(3, 7, 5, 9),]
        );
        assert_eq!(big.subtract(&b(20, 20, 30, 30)), Vec::new());
    }
}
