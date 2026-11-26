//! The settings window's key bindings page, laid out as Dereth's Horizon lays out its own: a group
//! of actions chosen at the top, then a row for each of the group's actions, its name (brighter
//! where its keys differ from Horizon's own layout, [`changed`]) and three buttons for its keys. Click one of
//! an action's keys, or an empty button beside them, and press a key (with Shift or Ctrl, or
//! alone) to bind it there; the key is taken from any other action it was bound to, and Escape
//! leaves the keys as they were. A right click takes a key off the action. Apply writes Horizon's
//! key files and binds them from then on; Restore Defaults puts Horizon's own layout back, to be
//! applied.

use auberean_shell::interface_keys::{own, HORIZON_ACTIONS, HORIZON_KEYS};
use auberean_shell::keys::{META_CTRL, META_SHIFT};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::draw::Rect;

/// The maps the page edits: keys alone, with Shift, with Ctrl; each a list of an input and its
/// action.
pub type Maps = [Vec<(u32, u32)>; 3];

/// The modifier of each map, in [`Maps`]' order.
pub const METAS: [u32; 3] = [0, META_SHIFT, META_CTRL];

/// Horizon's name for one of its actions.
#[must_use]
pub fn action_name(action: u32) -> String {
    let named = match action {
        0x4000_0020 => "Walk Forward",
        0x4000_0021 => "Walk Backwards",
        0x4000_0022 => "Side Step Left",
        0x4000_0023 => "Side Step Right",
        0x4000_0024 => "Turn Left",
        0x4000_0025 => "Turn Right",
        0x4000_0026 => "Jump",
        0x4000_0027 => "Auto Run",
        0x4000_00A3 => "Stop Moving",
        0x4000_002E => "Camera Left Rotate",
        0x4000_002F => "Camera Right Rotate",
        0x4000_00B6 => "Camera Raise",
        0x4000_00B7 => "Camera Lower",
        own::LOOK_AT_FRONT => "Front View",
        own::CRAFTING => "Crafting Panel",
        0x4100_0006 => "Skills Panel",
        0x4100_0002 => "Inventory Panel",
        0x4100_0003 => "Map Panel",
        0x4100_0005 => "Journal Panel",
        0x4100_0004 => "Settings Panel",
        0x4100_0007 => "Social Panel",
        0x4100_0001 => "Examine Selected",
        0x4100_000D => "Use Selected",
        0x4100_001A => "Next Monster",
        0x4100_0019 => "Previous Monster",
        0x4100_001E => "Closest Item",
        0x4100_001C => "Previous Item",
        0x4100_001D => "Next Item",
        0x4100_0010 => "Issue Slash Command",
        0x4000_0017 => "Enter Chat Mode",
        0x4100_0049 => "Reply",
        0x4100_000E => "Log Out",
        0x4100_0017 => "Screenshot",
        0x4100_0013 => "Attack",
        0x4100_0040 => "Previous Shortcut Bar",
        0x4100_005C => "Previous Pet",
        0x4100_005D => "Next Pet",
        0x4100_003F => "Next Shortcut Bar",
        a if (0x4100_002B..=0x4100_0034).contains(&a) => {
            return format!("Use Shortcut {}", a - 0x4100_002B + 1)
        }
        a if (0x4100_005F..=0x4100_0068).contains(&a) => {
            return format!("Use Row 2 Shortcut {}", a - 0x4100_005F + 1)
        }
        a if (0x4100_0041..=0x4100_0044).contains(&a) => {
            return format!("Chat Window {}", a - 0x4100_0041 + 1)
        }
        a if (0x4100_0053..=0x4100_005B).contains(&a) => {
            return format!("Select Fellow {}", a - 0x4100_0053 + 1)
        }
        a => return format!("Action {a:#x}"),
    };
    named.to_owned()
}

/// A key's name, by its input word.
#[must_use]
pub fn key_name(input: u32) -> String {
    let scan = input.wrapping_sub(0x8100_0400);
    let letters = "QWERTYUIOP";
    let named = match scan {
        0x01 => "Esc",
        0x02..=0x0A => return char::from(b'1' + u8::try_from(scan - 2).unwrap_or(0)).to_string(),
        0x0B => "0",
        0x0C => "-",
        0x0D => "=",
        0x0E => "Backspace",
        0x0F => "Tab",
        0x10..=0x19 => {
            let i = usize::try_from(scan - 0x10).unwrap_or(0);
            return letters[i..=i].to_owned();
        }
        0x1A => "[",
        0x1B => "]",
        0x1C => "Enter",
        0x1E..=0x26 => {
            let i = usize::try_from(scan - 0x1E).unwrap_or(0);
            return "ASDFGHJKL"[i..=i].to_owned();
        }
        0x27 => ";",
        0x28 => "'",
        0x29 => "`",
        0x2B => "\\",
        0x2C..=0x32 => {
            let i = usize::try_from(scan - 0x2C).unwrap_or(0);
            return "ZXCVBNM"[i..=i].to_owned();
        }
        0x33 => ",",
        0x34 => ".",
        0x35 => "/",
        0x37 => "Num *",
        0x39 => "Space",
        0x3B..=0x44 => return format!("F{}", scan - 0x3B + 1),
        0x57 => "F11",
        0x58 => "F12",
        0x47 => "Num 7",
        0x48 => "Num 8",
        0x49 => "Num 9",
        0x4A => "Num -",
        0x4B => "Num 4",
        0x4C => "Num 5",
        0x4D => "Num 6",
        0x4E => "Num +",
        0x4F => "Num 1",
        0x50 => "Num 2",
        0x51 => "Num 3",
        0x52 => "Num 0",
        0x53 => "Num Del",
        0x9C => "Num Enter",
        0xB5 => "Num /",
        0xC7 => "Home",
        0xC8 => "Up",
        0xC9 => "Page Up",
        0xCB => "Left",
        0xCD => "Right",
        0xCF => "End",
        0xD0 => "Down",
        0xD1 => "Page Down",
        0xD2 => "Insert",
        0xD3 => "Delete",
        _ => return format!("Key {input:#x}"),
    };
    named.to_owned()
}

/// Horizon's own layout, as the page edits it.
#[must_use]
pub fn defaults() -> Maps {
    let mut maps: Maps = Default::default();
    for &(input, meta, action) in HORIZON_KEYS.iter() {
        if let Some(i) = METAS.iter().position(|m| *m == meta) {
            maps[i].push((input, action));
        }
    }
    maps
}

/// The actions the page lists, in Horizon's order, each once, those its layout binds to no key
/// among them.
#[must_use]
pub fn actions() -> Vec<u32> {
    HORIZON_ACTIONS.iter().map(|(a, _)| *a).collect()
}

/// The keys bound to `action` in `maps`, each its map's place in [`Maps`] and its input, in
/// order: keys alone, then with Shift, then with Ctrl.
#[must_use]
pub fn bound(maps: &Maps, action: u32) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    for (i, map) in maps.iter().enumerate() {
        out.extend(
            map.iter()
                .filter(|(_, a)| *a == action)
                .map(|(k, _)| (i, *k)),
        );
    }
    out
}

/// A key bound in map `map` ([`Maps`]' place), by name.
#[must_use]
pub fn bound_name(map: usize, input: u32) -> String {
    let prefix = ["", "Shift+", "Ctrl+"].get(map).copied().unwrap_or("");
    format!("{prefix}{}", key_name(input))
}

/// Whether `action`'s keys in `maps` differ from those Horizon's own layout gives it: the page
/// marks its row.
#[must_use]
pub fn changed(maps: &Maps, action: u32) -> bool {
    bound(maps, action) != bound(&defaults(), action)
}

/// The keys bound to `action` in `maps`, by name.
#[must_use]
pub fn keys_of(maps: &Maps, action: u32) -> Vec<String> {
    bound(maps, action)
        .into_iter()
        .map(|(m, k)| bound_name(m, k))
        .collect()
}

/// The groups the page's actions fall in, in the order the page offers them.
pub const GROUPS: [&str; 8] = [
    "Movement",
    "Camera",
    "Combat",
    "Selection",
    "Windows",
    "Chat",
    "Shortcuts",
    "Other",
];

/// The group `action` falls in, by its place in [`GROUPS`].
#[must_use]
pub fn group_of(action: u32) -> usize {
    match action {
        0x4000_0020..=0x4000_0027 | 0x4000_00A3 => 0,
        0x4000_002E | 0x4000_002F | 0x4000_00B6 | 0x4000_00B7 | own::LOOK_AT_FRONT => 1,
        0x4100_0013 => 2,
        0x4100_0001 | 0x4100_000D | 0x4100_0019..=0x4100_001E | 0x4100_0053..=0x4100_005D => 3,
        0x4100_0002..=0x4100_0007 | own::CRAFTING => 4,
        0x4100_0010 | 0x4000_0017 | 0x4100_0049 => 5,
        0x4100_002B..=0x4100_0034 | 0x4100_003F | 0x4100_0040 | 0x4100_005F..=0x4100_0068 => 6,
        _ => 7,
    }
}

/// How many keys a row shows.
pub const KEY_SLOTS: usize = 3;

/// A row's height, in layout units.
pub const ROW: f32 = 30.0;

/// The key bindings page.
#[derive(Debug, Default)]
pub struct Bindings {
    /// The maps edited, once the page has been shown (from the keys bound then).
    pub maps: Option<Maps>,
    /// The action waiting for its key.
    pub capturing: Option<u32>,
    /// The key the one pressed takes the place of (its map and input), or none to add it.
    pub replacing: Option<(usize, u32)>,
    /// The maps differ from those applied.
    pub changed: bool,
    /// Apply was pressed: the maps to write and bind.
    pub applied: Option<Maps>,
    /// The group shown, by its place in [`GROUPS`].
    pub group: usize,
    /// The groups' list is open under its box, scrolled to its `menu_top`th row.
    menu: bool,
    menu_top: usize,
    scroll: f32,
    /// Where the rows' key buttons were drawn this frame: each action's, in order, and its
    /// scrollbar's lane while the list scrolls.
    pub drawn: Vec<(u32, [Rect; KEY_SLOTS])>,
    pub bar: Option<Rect>,
    /// Where the Cancel of the box asking for a key was drawn this frame.
    pub cancel_drawn: Option<Rect>,
}

/// Where the page's parts are, in a page drawn in `area` at `k` pixels a layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    /// The group's box.
    pub group: Rect,
    /// The rows.
    pub list: Rect,
    pub apply: Rect,
    pub defaults: Rect,
}

impl Bindings {
    /// A key pressed while an action waits for one: `input` with the modifier `meta` (0, Shift or
    /// Ctrl) bound to it in place of the key clicked (or beside its keys), and taken from any
    /// other action it was bound to.
    pub fn bind(&mut self, input: u32, meta: u32) {
        let replacing = self.replacing.take();
        let (Some(action), Some(maps)) = (self.capturing.take(), self.maps.as_mut()) else {
            return;
        };
        if let Some((m, k)) = replacing {
            if let Some(map) = maps.get_mut(m) {
                map.retain(|&(i, a)| !(i == k && a == action));
            }
        }
        let i = METAS.iter().position(|m| *m == meta).unwrap_or(0);
        maps[i].retain(|&(k, _)| k != input);
        maps[i].push((input, action));
        self.changed = true;
    }

    /// Stop waiting for a key.
    pub fn cancel(&mut self) {
        self.capturing = None;
        self.replacing = None;
        self.menu = false;
    }

    /// Where the parts are in a page drawn in `area`.
    #[must_use]
    pub fn parts(area: Rect, k: f32) -> Parts {
        let group = Rect::new(area.x, area.y, 260.0 * k, 28.0 * k);
        let list = Rect::new(area.x, area.y + 36.0 * k, area.w, area.h - 80.0 * k);
        let apply = Rect::new(
            area.right() - 130.0 * k,
            area.bottom() - 34.0 * k,
            120.0 * k,
            30.0 * k,
        );
        let defaults = Rect::new(area.x, apply.y, 180.0 * k, 30.0 * k);
        Parts {
            group,
            list,
            apply,
            defaults,
        }
    }

    /// The key buttons of a row whose top is `y` in `list`, `gutter` pixels left at the right
    /// for the scrollbar.
    #[must_use]
    pub fn slots(list: Rect, y: f32, k: f32, gutter: f32) -> [Rect; KEY_SLOTS] {
        let x0 = list.x + list.w * 0.44;
        let w = ((list.right() - gutter - x0) - 8.0 * k) / 3.0;
        std::array::from_fn(|i| {
            #[allow(clippy::cast_precision_loss)]
            let x = x0 + (w + 4.0 * k) * i as f32;
            Rect::new(x, y + 2.0 * k, w, (ROW - 4.0) * k)
        })
    }

    /// The page in `area`.
    pub fn frame(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, area: Rect) {
        let k = p.scale;
        self.drawn.clear();
        self.bar = None;
        self.cancel_drawn = None;
        let Some(maps) = self.maps.clone() else {
            return;
        };
        let parts = Self::parts(area, k);
        let capturing = self.capturing.is_some();
        // The groups' open list takes the presses over it before anything under it is drawn, and
        // a press anywhere else closes it.
        let occluders = ctx.input.occluders.len();
        if self.menu {
            if kit::dropdown_dismissed(p, ctx, parts.group, GROUPS.len()) {
                self.menu = false;
            } else {
                let r = kit::dropdown_list_rect(p, parts.group, GROUPS.len());
                ctx.input.occluders.push(r);
            }
        }
        self.group = self.group.min(GROUPS.len() - 1);
        if kit::dropdown_box(
            p,
            ctx,
            parts.group,
            GROUPS[self.group],
            self.menu,
            !capturing,
        ) {
            self.menu = !self.menu;
            self.menu_top = 0;
        }
        self.rows(p, ctx, &maps, parts.list, capturing);
        if kit::button(p, ctx, parts.defaults, "Restore Defaults", !capturing) {
            self.maps = Some(defaults());
            self.changed = true;
        }
        if kit::button(p, ctx, parts.apply, "Apply", self.changed && !capturing) {
            self.applied.clone_from(&self.maps);
            self.changed = false;
        }
        ctx.input.occluders.truncate(occluders);
        if self.menu {
            let at = self.group;
            if let Some(i) =
                kit::dropdown_list(p, ctx, parts.group, &GROUPS, at, &mut self.menu_top)
            {
                self.menu = false;
                if i != self.group {
                    self.group = i;
                    self.scroll = 0.0;
                }
            }
        }
        // Asked from the frame after the key's button was clicked, so that press is not its own.
        if let Some(action) = self.capturing.filter(|_| capturing) {
            self.question(p, ctx, area, action);
        }
    }

    /// The rows of the group shown, in `pane`.
    fn rows(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        maps: &Maps,
        pane: Rect,
        capturing: bool,
    ) {
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let all: Vec<u32> = actions()
            .into_iter()
            .filter(|a| group_of(*a) == self.group)
            .collect();
        let row_h = ROW * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * all.len() as f32;
        let offset = kit::scroll(p, ctx, pane, content, &mut self.scroll);
        if content > pane.h {
            let w = kit::scrollbar_width(p).unwrap_or(4.0) * k;
            self.bar = Some(Rect::new(pane.right() - w, pane.y, w, pane.h));
        }
        let gutter = crate::ui::options::gutter(p);
        p.list.push_clip(pane);
        #[allow(clippy::cast_precision_loss)]
        for (i, a) in all.iter().enumerate() {
            let y = pane.y + row_h * i as f32 - offset;
            if y + row_h < pane.y || y > pane.bottom() {
                continue;
            }
            let keys = bound(maps, *a);
            let style = if changed(maps, *a) { &label } else { &dim };
            p.text_in(
                style,
                Rect::new(pane.x + 12.0 * k, y, pane.w * 0.42, row_h),
                Align::Left,
                &action_name(*a),
            );
            let slots = Self::slots(pane, y, k, gutter);
            self.drawn.push((*a, slots));
            for (slot, r) in slots.iter().enumerate() {
                let key = keys.get(slot).copied();
                let name = key.map_or_else(String::new, |(m, k)| bound_name(m, k));
                let inside = ctx.input.hover(&pane);
                if kit::button(p, ctx, *r, &name, !capturing) && inside {
                    self.capturing = Some(*a);
                    self.replacing = key;
                }
                // A right click takes the key off the action.
                if let Some((m, input)) = key.filter(|_| !capturing) {
                    if inside && ctx.input.hover(r) && ctx.input.pressed[1] {
                        ctx.input.pressed[1] = false;
                        if let Some(edited) = self.maps.as_mut().and_then(|ms| ms.get_mut(m)) {
                            edited.retain(|&(i, act)| !(i == input && act == *a));
                            self.changed = true;
                        }
                    }
                }
            }
        }
        p.list.pop_clip();
    }

    /// While an action waits for its key: the box over the page that says so, and its Cancel.
    fn question(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, area: Rect, action: u32) {
        let k = p.scale;
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let w = (380.0 * k).min(area.w);
        let text = format!(
            "Press the key for {}.\n\nPress Escape to leave it as it is.",
            action_name(action)
        );
        let mut lines = Vec::new();
        for para in text.split('\n') {
            if para.is_empty() {
                lines.push(String::new());
            } else {
                lines.extend(p.wrap(&style, para, w - 32.0 * k));
            }
        }
        let lh = p.line_height(&style);
        #[allow(clippy::cast_precision_loss)]
        let h = 16.0 * k + lh * lines.len() as f32 + 12.0 * k + 30.0 * k + 14.0 * k;
        let r = Rect::new(
            (area.x + (area.w - w) / 2.0).round(),
            (area.y + (area.h - h) / 2.0).round(),
            w,
            h,
        );
        // The page dimmed under it, and the box opaque over it.
        p.fill(area, 0x6000_0000);
        let ground = p.art.palette().list_ground | 0xFF00_0000;
        p.fill(r, ground);
        kit::panel(p, r, 1.0);
        let mut y = r.y + 16.0 * k;
        for line in &lines {
            p.text_in(
                &style,
                Rect::new(r.x + 16.0 * k, y, r.w - 32.0 * k, lh),
                Align::Centre,
                line,
            );
            y += lh;
        }
        let cancel = Rect::new(
            r.x + (r.w - 120.0 * k) / 2.0,
            y + 12.0 * k,
            120.0 * k,
            30.0 * k,
        );
        self.cancel_drawn = Some(cancel);
        if kit::button(p, ctx, cancel, "Cancel", true) {
            self.cancel();
        }
        // The box takes the presses over it.
        if ctx.input.hover(&r) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.pressed = [false; 3];
        }
    }
}
