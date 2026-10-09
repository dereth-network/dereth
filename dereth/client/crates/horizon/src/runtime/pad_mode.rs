//! Gamepad mode carried out: each frame the pad's state becomes the orbit camera's sticks, the
//! game's actions, and the pointer, buttons, keys and typing the interface would have had from a
//! mouse and a keyboard, as [`crate::pad`] lays the buttons out.

use dereth_client_runtime::actions::{Action, ActionId};
use dereth_client_runtime::shell::Shell;
use dereth_input::host::HostEvent;
use dereth_input::pad::{PadButton as B, PadState};

use super::{Cx, HorizonFrontEnd};
use crate::pad::{Osk, OskKey, PadDriver, PadStep};
use crate::ui::input::{vk, PadHints, PadWorld};
use crate::ui::nav::{Cursor, Dir, Kind, PanelKind, Snapshot, Stepped};

/// The jump action, held to charge it.
const JUMP: ActionId = dereth_client_contract::actions::movement::JUMP;

/// How far one press of the d-pad moves a slider's knob, of its whole length.
const SLIDER_STEP: f32 = 0.05;

/// Wheel notches a second the right stick pushed all the way scrolls.
const SCROLL_RATE: f32 = 14.0;

/// Pixels a second the right stick pushed all the way moves the map's cursor.
const MAP_CURSOR_RATE: f32 = 450.0;

/// The name the map's window is known by.
const MAP: &str = "Map";

/// Gamepad mode's state across frames.
#[derive(Debug, Default)]
pub struct PadMode {
    driver: PadDriver,
    /// The focus over the interface.
    pub cursor: Cursor,
    /// The on-screen keyboard's pick, and whether CANCEL has put it away while the line still
    /// has the keyboard.
    osk: Osk,
    osk_hidden: bool,
    /// The pad began a jump, or an attack, that has not been let go: the action to end.
    holding: Vec<(B, u32)>,
    /// A button pressed for one frame only (a slider's step, a double-click, a cancel), let go
    /// the next.
    release_next: Option<usize>,
    /// Something picked up with A, carried until A puts it down.
    carrying: bool,
    /// The slider A has taken hold of, for the d-pad to move.
    adjusting: Option<crate::draw::Rect>,
    /// The right stick's scroll, short of a whole notch.
    scroll: f32,
    /// The map's cursor, while the map has the focus.
    map_cursor: Option<(f32, f32)>,
    /// When the last frame was, in the runtime's seconds.
    last_time: Option<f64>,
    /// A panel whose tab the shoulders turned, and the frames left to bring the focus to the top
    /// of the new page once it is drawn.
    retab: Option<(usize, u8)>,
    /// Where across the focus was in a page of rows when the tab turned, to keep on the new page.
    retab_column: Option<f32>,
    /// Whether the focus was on a line of text last frame: leaving it ends the edit.
    on_chat_line: bool,
    /// A pack the shoulders turned to, by its panel and its icon, and the frames left to bring
    /// the focus to it once it is drawn.
    repack: Option<(usize, crate::draw::Rect, u8)>,
    /// The windows open, by name, in the order they were opened.
    opened: Vec<String>,
    /// Frames since the client started, while it is still looking for a pad.
    looked: u32,
    /// A pad has been seen since the client started.
    seen_pad: bool,
    /// Gamepad mode was switched off for this run, there being no pad at the start: the setting
    /// itself stays on.
    pub off_for_session: bool,
}

/// Frames at the start the client waits for a pad before switching gamepad mode off for the run.
const PAD_WAIT: u32 = 120;

impl PadMode {
    /// One frame of looking for a pad at the start (`present`, with gamepad mode `enabled`):
    /// whether gamepad mode is to be switched off for the run now, none having been seen in the
    /// first frames. A pad plugged in later does not switch it back on.
    fn no_pad_at_start(&mut self, present: bool, enabled: bool) -> bool {
        self.seen_pad |= present;
        if self.seen_pad || self.looked >= PAD_WAIT {
            return false;
        }
        self.looked += 1;
        let off = self.looked == PAD_WAIT && enabled;
        self.off_for_session |= off;
        off
    }

    /// One of the window's events, as it reaches the interface. Gamepad mode is switched only by
    /// its setting: neither the keyboard nor the mouse turns it off, and the pad does not turn it
    /// on.
    pub fn note_event(&mut self, _event: &HostEvent) {}
}

impl HorizonFrontEnd {
    /// One frame of the pad, after the window's own events (`None` with no pad): in gamepad mode
    /// its sticks go to the orbit camera and its buttons to the game and the interface; out of
    /// it, any button switches the mode on. The keys it stands in for that the game should see
    /// too (Escape) come back, for the shell to route as the window's own.
    pub fn pad_input<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        pad: Option<PadState>,
    ) -> Vec<HostEvent> {
        let keys = self.pad_frame(cx, pad);
        // What the pad carries is let go of when the interface has dropped it.
        if self.ui.drag.is_none() {
            self.pad_mode.carrying = false;
        }
        self.frame_input.pad.carrying = self.pad_mode.carrying;
        // Counted as they go out, so they are not taken for keys of the player's own.
        keys
    }

    fn pad_frame<S: Shell>(&mut self, cx: &mut Cx<'_, S>, pad: Option<PadState>) -> Vec<HostEvent> {
        let now = cx.now();
        let dt = self
            .pad_mode
            .last_time
            .map_or(1.0 / 60.0, |t| (now - t).clamp(0.0, 0.25));
        self.pad_mode.last_time = Some(now);
        let step = self.pad_mode.driver.step(pad, &self.ui.options.pad, dt);
        if let Some(b) = self.pad_mode.release_next.take() {
            self.frame_input.down[b] = false;
            self.frame_input.released[b] = true;
        }
        let game = self.in_gameplay();
        // Gamepad mode on with no pad there at the start: off for this run, the setting kept.
        if self
            .pad_mode
            .no_pad_at_start(pad.is_some(), self.ui.options.pad.enabled)
        {
            self.ui.options.pad.enabled = false;
        }
        // Out of gamepad mode the pad is not listened to at all.
        if !self.ui.options.pad.enabled {
            self.leave_pad_mode(cx);
            return Vec::new();
        }
        let mut keys = Vec::new();
        // What is held is let go with its button.
        let mut let_go = Vec::new();
        self.pad_mode.holding.retain(|(b, id)| {
            let up = step.released(*b) || !game;
            if up {
                let_go.push(*id);
            }
            !up
        });
        for id in let_go {
            cx.inject_action(Action::end(ActionId(id)));
        }
        let snap = self.frame_input.nav.last.clone();
        self.pad_mode.opened = keep_open_order(&self.pad_mode.opened, &snap);
        self.pad_mode.cursor.follow(&snap);
        // A pack turned to: the focus into what is in it, at its first place; a pack that holds
        // nothing (a focus) leaves the focus on its own icon.
        if let Some((layer, icon, frames)) = self.pad_mode.repack.take() {
            if !self.pad_mode.cursor.enter_inside(&snap, layer) {
                let seen = snap.seen(None);
                if let Some(t) = seen.targets.iter().find(|t| t.rect == icon) {
                    self.pad_mode.cursor.entered = None;
                    self.pad_mode.cursor.focus = Some(*t);
                }
            }
            if frames > 1 {
                self.pad_mode.repack = Some((layer, icon, frames - 1));
            }
        }
        // A tab turned: the focus to the first control of the new page (under the tabs), in the
        // column kept when the focus came from another page's rows.
        if let Some((layer, frames)) = self.pad_mode.retab.take() {
            if let Some(t) = first_under_tabs(&snap, layer) {
                self.pad_mode.cursor.entered = None;
                self.pad_mode.cursor.focus = Some(t);
                if let Some(x) = self
                    .pad_mode
                    .retab_column
                    .filter(|_| snap.list_of(&t).is_some())
                {
                    self.pad_mode.cursor.keep_column(&snap, x);
                }
            }
            if frames > 1 {
                self.pad_mode.retab = Some((layer, frames - 1));
            } else {
                self.pad_mode.retab_column = None;
            }
        }
        // The item menu goes when the focus leaves it.
        if let Some((i, l)) = snap
            .layers
            .iter()
            .enumerate()
            .find(|(_, l)| l.name == "Item Menu")
        {
            if self.pad_mode.cursor.focus.map(|f| f.layer) != Some(i + 1) {
                self.frame_input.pad.close = Some(l.rect);
            }
        }
        // The focus leaving a line of text ends its edit, as Escape does there: the chat's line
        // closes, a box lets the keyboard go. Coming back to it starts a new edit.
        let on_line = self
            .pad_mode
            .cursor
            .focus
            .is_some_and(|t| t.kind == Kind::Text);
        if self.pad_mode.on_chat_line
            && !on_line
            && (self.ui.hud.log.typing() || self.ui.text_focus)
        {
            self.frame_input.keys.push(vk::ESCAPE);
            self.pad_mode.osk_hidden = false;
        }
        self.pad_mode.on_chat_line = on_line;
        let typing = self.ui.text_focus;
        if !typing {
            self.pad_mode.osk_hidden = false;
        }
        let keyboard = typing && !self.pad_mode.osk_hidden;
        let focus = self.pad_mode.cursor.focus;
        let in_ui = !keyboard && (!game || focus.is_some());
        // The map's cursor, while the map has the focus.
        let on_map = focus
            .and_then(|f| snap.layer(f.layer))
            .filter(|l| l.name == MAP)
            .map(|l| l.rect);
        if on_map.is_none() {
            self.pad_mode.map_cursor = None;
        }
        // The left stick moves in the world whatever the buttons do; the right stick turns the
        // camera only while no panel has the focus: over a panel it scrolls it, and over the map
        // it moves the map's cursor.
        let scrolling = in_ui && focus.is_some_and(|f| f.layer > 0);
        if game {
            let look = if on_map.is_some() || scrolling {
                (0.0, 0.0)
            } else {
                step.look
            };
            cx.orbit_sticks(step.movement, look);
            self.frame_input.pad.set = step.set;
            self.frame_input
                .pad
                .fired
                .extend(step.fired.iter().copied());
            self.frame_input.pad.pick = step.rb_pick;
            if step.pressed(B::LeftStick) {
                cx.inject_action(Action::begin(ActionId(crate::ui::ACTION_TOGGLE_COMBAT)));
            }
            if step.pressed(B::RightStick) {
                let walk = dereth_client_contract::actions::movement::TOGGLE_RUN_WALK;
                cx.inject_action(Action::begin(walk));
                cx.inject_action(Action::end(walk));
            }
            if step.pressed(B::Start) && !keyboard {
                self.frame_input.pad.menu = !self.frame_input.pad.menu;
            }
        } else {
            cx.orbit_sticks(None, (0.0, 0.0));
            self.frame_input.pad.set = None;
            self.frame_input.pad.menu = false;
        }
        if let Some(rect) = on_map {
            // The right stick takes a cursor from the place the focus is on; a step of the d-pad
            // between places puts it away again.
            if step.dpad.is_some() {
                self.pad_mode.map_cursor = None;
            } else if let Some((x, y)) = step.scroll {
                // The cursor lets go of the place chosen: the focus rests on the map itself.
                if let Some(map) = focus.and_then(|f| snap.home_in(f.layer)) {
                    self.pad_mode.cursor.focus = Some(map);
                }
                let start = focus.map_or((rect.x + rect.w / 2.0, rect.y + rect.h / 2.0), |f| {
                    f.centre()
                });
                let at = self.pad_mode.map_cursor.get_or_insert(start);
                let by = MAP_CURSOR_RATE * crate::ui::narrow(dt);
                at.0 = (at.0 + x * by).clamp(rect.x, rect.right());
                at.1 = (at.1 - y * by).clamp(rect.y, rect.bottom());
            }
        } else if scrolling {
            if let Some((_, y)) = step.scroll {
                self.pad_mode.scroll += y * SCROLL_RATE * crate::ui::narrow(dt);
            } else {
                self.pad_mode.scroll = 0.0;
            }
            while self.pad_mode.scroll.abs() >= 1.0 {
                let notch = self.pad_mode.scroll.signum();
                self.frame_input.wheel += notch;
                self.pad_mode.scroll -= notch;
            }
        }
        self.frame_input.pad.map_cursor = self.pad_mode.map_cursor;
        // Back cycles the focus through the panels open, the chat log among them, and in the
        // world back out to the world.
        if step.pressed(B::Back) && !keyboard {
            self.cycle_panels(&snap, game);
        }
        if keyboard {
            self.frame_input.pad.mode = Some(PadHints::Keyboard);
            self.frame_input.pad.focus = None;
            self.keyboard(&step);
            return keys;
        }
        self.frame_input.pad.osk = None;
        if in_ui {
            self.frame_input.pad.mode = Some(PadHints::Cursor);
            self.cursor(cx, &step, &snap, game, &mut keys);
            return keys;
        }
        self.world(cx, &step);
        keys
    }

    /// Out of gamepad mode: nothing of the pad's left held.
    fn leave_pad_mode<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        let p = &mut self.frame_input.pad;
        if p.mode.is_none() {
            return;
        }
        p.mode = None;
        p.focus = None;
        p.osk = None;
        p.set = None;
        p.menu = false;
        p.map_cursor = None;
        cx.orbit_sticks(None, (0.0, 0.0));
        for (_, id) in std::mem::take(&mut self.pad_mode.holding) {
            cx.inject_action(Action::end(ActionId(id)));
        }
        self.pad_mode.cursor.resign();
    }

    /// Back: the focus to the next panel in [`cycle_order`], round past the last. Giving the
    /// focus up (B out of the chat log, or out of a window by closing it) is what returns it to
    /// the world; Back does not stop there.
    fn cycle_panels(&mut self, snap: &Snapshot, _game: bool) {
        self.pad_mode.opened = keep_open_order(&self.pad_mode.opened, snap);
        let panels = cycle_order(snap, &self.pad_mode.opened);
        let at = self.pad_mode.cursor.focus.map(|f| f.layer);
        let next = match at.and_then(|l| panels.iter().position(|p| *p == l)) {
            Some(i) => panels.get((i + 1) % panels.len()).copied(),
            None => panels.first().copied(),
        };
        if let Some(layer) = next {
            self.pad_mode.cursor.enter(snap, layer);
        }
    }

    /// The on-screen keyboard, while a line of text has the keyboard.
    fn keyboard(&mut self, step: &PadStep) {
        let osk = &mut self.pad_mode.osk;
        let f = &mut self.frame_input;
        if let Some(dir) = step.dpad {
            osk.step(dir);
        }
        if step.pressed(B::LeftBumper) {
            osk.shift = !osk.shift;
        }
        let mut press = |key: OskKey, shift: &mut bool| match key {
            OskKey::Char(c) => {
                f.chars.push(c);
                *shift = false;
            }
            OskKey::Space => f.chars.push(' '),
            OskKey::Back => f.keys.push(vk::BACK),
            OskKey::Enter => f.keys.push(vk::ENTER),
            OskKey::Done => f.keys.push(vk::ESCAPE),
            OskKey::Shift => *shift = !*shift,
        };
        if step.pressed(B::South) {
            let key = osk.key();
            press(key, &mut osk.shift);
        }
        if step.pressed(B::West) {
            press(OskKey::Back, &mut osk.shift);
        }
        if step.pressed(B::North) {
            press(OskKey::Space, &mut osk.shift);
        }
        if step.pressed(B::Start) {
            press(OskKey::Enter, &mut osk.shift);
        }
        f.pad.osk = Some(*osk);
        // CANCEL puts the keyboard away; the line keeps the keyboard, and CONFIRM on it brings
        // the keyboard back.
        if step.pressed(B::East) {
            self.pad_mode.osk_hidden = true;
        }
    }

    /// Click `r` for one frame: the pointer there, the left button down and let go the next.
    fn click_at(&mut self, r: crate::draw::Rect) {
        let f = &mut self.frame_input;
        f.mouse = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        f.down[0] = true;
        f.pressed[0] = true;
        self.pad_mode.release_next = Some(0);
    }

    /// The focus over the interface: the d-pad moves it within its panel, and the face buttons
    /// confirm, cancel and act there.
    fn cursor<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        step: &PadStep,
        snap: &Snapshot,
        game: bool,
        keys: &mut Vec<HostEvent>,
    ) {
        let mut pointer = None;
        let mut one_frame = false;
        // A slider is adjusted only once A has taken hold of it, until B lets it go.
        let adjusting = self
            .pad_mode
            .cursor
            .focus
            .is_some_and(|t| Some(t.rect) == self.pad_mode.adjusting);
        if !adjusting {
            self.pad_mode.adjusting = None;
        }
        // In the split box, left and right (one) and the shoulders (ten) change the amount.
        let in_split = self
            .pad_mode
            .cursor
            .focus
            .and_then(|t| snap.layer(t.layer))
            .is_some_and(|l| l.name == crate::ui::panels::pad_items::SPLIT);
        if in_split {
            let mut by = 0;
            match step.dpad {
                Some(Dir::Left) => by -= 1,
                Some(Dir::Right) => by += 1,
                _ => {}
            }
            if step.pressed(B::LeftBumper) {
                by -= 10;
            }
            if step.rb_tap {
                by += 10;
            }
            self.frame_input.pad.nudge = by;
        }
        if let Some(dir) = step
            .dpad
            .filter(|d| !(in_split && matches!(d, Dir::Left | Dir::Right)))
        {
            let across = matches!(dir, Dir::Left | Dir::Right);
            let focus = self.pad_mode.cursor.focus;
            match focus {
                Some(_) if adjusting && !across => {}
                Some(t) if adjusting && matches!(t.kind, Kind::Slider { .. }) => {
                    if let Kind::Slider { x0, x1, t: at } = t.kind {
                        let by = if dir == Dir::Left {
                            -SLIDER_STEP
                        } else {
                            SLIDER_STEP
                        };
                        let to = (at + by).clamp(0.0, 1.0);
                        pointer = Some((x0 + (x1 - x0) * to, t.centre().1));
                        one_frame = true;
                    }
                }
                Some(_) => {
                    let mut stepped = self.pad_mode.cursor.step(snap, dir);
                    // At the panel's end, round to its other end (a list there scrolled to that
                    // end too).
                    if stepped == Stepped::Stayed && matches!(dir, Dir::Up | Dir::Down) {
                        stepped = self.pad_mode.cursor.wrap(snap, dir);
                    }
                    match stepped {
                        Stepped::Scroll(dir) => {
                            self.frame_input.wheel += if dir == Dir::Up { 1.0 } else { -1.0 };
                        }
                        Stepped::ScrollBy { area, by } => {
                            self.frame_input.pad.scroll_list = Some((area, by));
                        }
                        Stepped::Moved | Stepped::Stayed => {}
                    }
                }
                None => {}
            }
        }
        let focus = self.pad_mode.cursor.focus;
        let layer = focus.map_or(0, |t| t.layer);
        let panel = snap.layer(layer);
        let f = &mut self.frame_input;
        f.pad.focus = focus.map(|t| t.rect);
        // The window the focus is in comes to the top, however the focus got there.
        f.pad.raise = panel
            .filter(|l| l.kind == PanelKind::Window)
            .map(|l| l.rect);
        if let Some(at) = pointer
            .or(self.pad_mode.map_cursor)
            .or_else(|| focus.map(|t| t.centre()))
        {
            f.mouse = at;
        }
        if one_frame {
            f.down[0] = true;
            f.pressed[0] = true;
            self.pad_mode.release_next = Some(0);
        }
        // A on a list as a whole enters it.
        if step.pressed(B::South) && focus.is_some_and(|t| t.kind == Kind::List) {
            self.pad_mode.cursor.enter_list(snap);
            return;
        }
        // A on the chat's input line opens it.
        if step.pressed(B::South)
            && panel.is_some_and(|l| l.kind == PanelKind::Chat)
            && focus.is_some_and(|t| t.kind == Kind::Text)
        {
            self.ui.hud.open_chat(String::new());
            return;
        }
        // A on a slider takes hold of it, for the d-pad's left and right; B lets it go.
        if let Some(t) = focus.filter(|t| matches!(t.kind, Kind::Slider { .. })) {
            if step.pressed(B::South) {
                self.pad_mode.adjusting = if adjusting { None } else { Some(t.rect) };
                return;
            }
            if step.pressed(B::East) && adjusting {
                self.pad_mode.adjusting = None;
                return;
            }
        }
        // A on a line of text brings the keyboard back up.
        if step.pressed(B::South) && focus.is_some_and(|t| t.kind == Kind::Text) {
            self.pad_mode.osk_hidden = false;
        }
        // A on a pack's icon shows the pack, and the focus goes to its first place.
        if step.pressed(B::South) && self.ui.windows.moving_pack.is_none() {
            if let (Some(t), Some((rects, _))) = (focus, panel.and_then(|l| l.steps.as_ref())) {
                if rects.contains(&t.rect) {
                    self.pad_mode.repack = Some((layer, t.rect, 2));
                }
            }
        }
        let f = &mut self.frame_input;
        // A: CONFIRM, the left button. What a press picks up (an item) is carried until A puts it
        // down again, so the d-pad can take it elsewhere.
        if step.pressed(B::South) {
            if self.pad_mode.carrying {
                self.pad_mode.carrying = false;
                f.down[0] = false;
                f.released[0] = true;
            } else {
                f.down[0] = true;
                f.pressed[0] = true;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let ms = (cx.now() * 1000.0) as u32;
                f.note_left_press(ms);
            }
        }
        if step.released(B::South) && f.down[0] && !self.pad_mode.carrying {
            if let Some(d) = self.ui.drag.as_mut() {
                // Lifted at once, and never the selection: putting it down anywhere moves it.
                d.active = true;
                d.on_click = None;
                self.pad_mode.carrying = true;
            } else {
                f.down[0] = false;
                f.released[0] = true;
            }
        }
        // X: the right button (an item's options, examine).
        if step.pressed(B::West) {
            f.down[1] = true;
            f.pressed[1] = true;
        }
        if step.released(B::West) && f.down[1] {
            f.down[1] = false;
            f.released[1] = true;
        }
        // Y: a double-click (use, equip).
        if step.pressed(B::North) && !self.pad_mode.carrying {
            f.down[0] = true;
            f.pressed[0] = true;
            f.double = true;
            self.pad_mode.release_next = Some(0);
        }
        // B: CANCEL. What is carried goes back; the menu closes; a panel is cancelled by its own
        // control (a window's close button, a box's No or OK); the chat log lets the focus go;
        // otherwise Escape.
        if step.pressed(B::East) {
            if self.pad_mode.carrying {
                self.pad_mode.carrying = false;
                self.ui.drag = None;
                f.down[0] = false;
            } else if f.pad.menu {
                f.pad.menu = false;
            } else if self.ui.windows.moving_pack.take().is_some() {
                // A pack being moved stays where it was.
            } else if self.ui.hud.targeting {
                // An item's use waiting for what it goes on is put down, the focus left where it
                // is.
                self.ui.hud.targeting = false;
                self.frame_input.pad.cancel_targeting = true;
            } else if self.pad_mode.cursor.back(snap) {
                // Back to where the focus came from, or out of a list to the list as a whole.
            } else if layer == 0 && !game && snap.screen_cancel.is_some() {
                // On a screen before the world, CANCEL brings the focus to its way out (character
                // select's Exit) and leaves pressing it to CONFIRM.
                let out = snap.screen_cancel;
                if let Some(t) = snap.targets.iter().find(|t| Some(t.rect) == out) {
                    self.pad_mode.cursor.focus = Some(*t);
                }
            } else if let Some(cancel) = snap.cancel_of(layer) {
                self.click_at(cancel);
            } else if let Some(window) = panel.filter(|l| l.kind == PanelKind::Window) {
                f.pad.close = Some(window.rect);
            } else if panel.is_some_and(|l| l.kind == PanelKind::Chat) {
                self.pad_mode.cursor.resign();
            } else {
                escape(keys);
            }
        }
        // The shoulders: a panel's packs, else its tabs.
        for (tap, by) in [(step.pressed(B::LeftBumper), -1), (step.rb_tap, 1)] {
            if !tap {
                continue;
            }
            if let Some((rects, at)) = panel.and_then(|l| l.steps.clone()) {
                let n = i32::try_from(rects.len()).unwrap_or(0);
                if n > 0 {
                    let at = i32::try_from(at).unwrap_or(0);
                    let to = usize::try_from((at + by).rem_euclid(n)).unwrap_or(0);
                    self.click_at(rects[to]);
                    self.pad_mode.repack = Some((layer, rects[to], 2));
                }
            } else if let Some(bar) = snap.tabs_in(layer) {
                self.frame_input.pad.tab_step = Some((bar, by));
                self.pad_mode.retab = Some((layer, 3));
                // From inside a page of rows, the new page's first row in the same column.
                self.pad_mode.retab_column = focus
                    .filter(|t| snap.list_of(t).is_some())
                    .map(|t| t.centre().0);
            }
        }
    }

    /// In the world, with no panel holding the focus.
    fn world<S: Shell>(&mut self, cx: &mut Cx<'_, S>, step: &PadStep) {
        let alternate = self.ui.hud.alt.is_some();
        let f = &mut self.frame_input;
        f.pad.mode = Some(if alternate {
            PadHints::Alternate
        } else {
            PadHints::World
        });
        f.pad.focus = None;
        // In gamepad mode the pad has the pointer: with no panel focused, it is over nothing.
        f.mouse = (-1000.0, -1000.0);
        // What is begun now and held until its button comes up.
        let mut hold: Vec<(B, u32)> = Vec::new();
        // The d-pad: up and down the fellowship; left and right the alternate selection. (In a
        // fighting stance the stance's controls are on the cross hotbars' stance set.)
        match step.dpad {
            Some(Dir::Up) => f.pad.world.push(PadWorld::Fellow(-1)),
            Some(Dir::Down) => f.pad.world.push(PadWorld::Fellow(1)),
            Some(Dir::Left) => f.pad.world.push(PadWorld::Alternate(-1)),
            Some(Dir::Right) => f.pad.world.push(PadWorld::Alternate(1)),
            None => {}
        }
        // The alternate selection takes A and B while it is up.
        if alternate {
            if step.pressed(B::South) {
                f.pad.world.push(PadWorld::TakeAlternate);
            }
            if step.pressed(B::East) {
                f.pad.world.push(PadWorld::DropAlternate);
            }
        } else {
            if step.pressed(B::South) {
                f.pad.world.push(PadWorld::Use);
            }
            // X: the selection's options, or with nothing selected the map.
            if step.pressed(B::West) {
                if self.ui.hud.has_target {
                    f.pad.world.push(PadWorld::Options);
                } else {
                    f.actions.push(crate::ui::hud::ACTION_MAP);
                }
            }
            if step.pressed(B::East) {
                // CANCEL in the world: the bind-to-hotbar notice goes, else the selection is let
                // go, in any stance; with nothing selected it does nothing.
                if self.ui.hud.cross.binding.take().is_none() {
                    f.pad.world.push(PadWorld::Deselect);
                }
            }
        }
        if step.pressed(B::North) {
            hold.push((B::North, JUMP.0));
        }
        for (b, id) in hold {
            cx.inject_action(Action::begin(ActionId(id)));
            self.pad_mode.holding.push((b, id));
        }
        // LB tapped: run on, or stop running on.
        if step.lb_tap {
            for a in autorun_tap() {
                cx.inject_action(a);
            }
        }
    }
}

/// What a tap of LB sends: autorun is a toggle, each event flipping it, so one event (a key's
/// press) and not a press and a release, which would turn it on and straight off again.
fn autorun_tap() -> [Action; 1] {
    [Action::begin(
        dereth_client_contract::actions::movement::AUTORUN,
    )]
}

/// Escape, pressed and let go, for the shell to route as the window's own.
fn escape(keys: &mut Vec<HostEvent>) {
    for pressed in [true, false] {
        keys.push(HostEvent::KeyboardInput {
            key: dereth_input::keys::Key::ESCAPE,
            pressed,
            text: None,
        });
    }
}

/// The first control of panel `layer`'s page under its row of tabs (reading from the top left),
/// or its first control where it has no tabs.
fn first_under_tabs(snap: &Snapshot, layer: usize) -> Option<crate::ui::nav::Target> {
    snap.layer(layer)?;
    let seen = snap.seen(None);
    let bar = snap.tabs_in(layer);
    seen.targets
        .iter()
        .filter(|t| t.layer == layer)
        .filter(|t| {
            bar.is_none_or(|b| {
                let (x, _) = t.centre();
                t.rect.y >= b.bottom() - 1.0 && x >= b.x && x <= b.right()
            })
        })
        .min_by(|a, b| {
            ((a.rect.y / 8.0).floor(), a.rect.x)
                .partial_cmp(&((b.rect.y / 8.0).floor(), b.rect.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// The windows open in `snap`, by name, in the order they were opened: those of `was` still open
/// in their order, then any opened since, in drawing order.
fn keep_open_order(was: &[String], snap: &Snapshot) -> Vec<String> {
    let windows: Vec<&str> = snap
        .layers
        .iter()
        .filter(|l| l.kind == PanelKind::Window)
        .map(|l| l.name.as_str())
        .collect();
    let mut order: Vec<String> = was
        .iter()
        .filter(|n| windows.contains(&n.as_str()))
        .cloned()
        .collect();
    for w in windows {
        if !order.iter().any(|n| n == w) {
            order.push(w.to_owned());
        }
    }
    order
}

/// The panels Back steps through, in order: the chat log, then the windows open in the order
/// they were opened (`opened`), each with something to rest on.
fn cycle_order(snap: &Snapshot, opened: &[String]) -> Vec<usize> {
    let with_targets = snap.panels();
    let chat = with_targets
        .iter()
        .copied()
        .find(|l| snap.layer(*l).is_some_and(|l| l.kind == PanelKind::Chat));
    let windows = opened.iter().filter_map(|name| {
        with_targets.iter().copied().find(|l| {
            snap.layer(*l)
                .is_some_and(|l| &l.name == name && l.kind == PanelKind::Window)
        })
    });
    chat.into_iter().chain(windows).collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own pad support)

    #[test]
    fn back_steps_through_the_chat_then_the_windows_in_the_order_they_were_opened_round_and_round()
    {
        use crate::draw::Rect;
        use crate::ui::nav::{Kind, Layer, PanelKind, Snapshot, Target};
        let layer = |name: &str, kind: PanelKind| Layer {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            name: name.into(),
            kind,
            cancel: None,
            steps: None,
        };
        let target = |l: usize| Target {
            rect: Rect::new(1.0, 1.0, 5.0, 5.0),
            kind: Kind::Plain,
            layer: l,
            clip: None,
        };
        // The inventory opened first, then the character window; the inventory has since been
        // raised, so it is drawn last.
        let first = Snapshot {
            layers: vec![
                layer("Chat", PanelKind::Chat),
                layer("Inventory", PanelKind::Window),
            ],
            targets: vec![target(1), target(2)],
            ..Snapshot::default()
        };
        let opened = super::keep_open_order(&[], &first);
        let snap = Snapshot {
            layers: vec![
                layer("Chat", PanelKind::Chat),
                layer("Character", PanelKind::Window),
                layer("Inventory", PanelKind::Window),
            ],
            targets: vec![target(1), target(2), target(3)],
            ..Snapshot::default()
        };
        let opened = super::keep_open_order(&opened, &snap);
        assert_eq!(opened, ["Inventory", "Character"]);
        assert_eq!(
            super::cycle_order(&snap, &opened),
            [1, 3, 2],
            "chat, inventory, character"
        );
        // The inventory closed: it leaves the round.
        let fewer = Snapshot {
            layers: vec![
                layer("Chat", PanelKind::Chat),
                layer("Character", PanelKind::Window),
            ],
            targets: vec![target(1), target(2)],
            ..Snapshot::default()
        };
        let opened = super::keep_open_order(&opened, &fewer);
        assert_eq!(super::cycle_order(&fewer, &opened), [1, 2]);
    }

    #[test]
    fn with_no_pad_at_the_start_gamepad_mode_goes_off_for_the_run_and_a_pad_there_keeps_it() {
        let mut none = super::PadMode::default();
        let offs: Vec<bool> = (0..super::PAD_WAIT + 10)
            .map(|_| none.no_pad_at_start(false, true))
            .collect();
        assert_eq!(offs.iter().filter(|o| **o).count(), 1, "once");
        assert!(none.off_for_session);
        assert!(
            !none.no_pad_at_start(true, true),
            "a pad later changes nothing"
        );
        let mut found = super::PadMode::default();
        assert!(!found.no_pad_at_start(false, true));
        assert!(!found.no_pad_at_start(true, true));
        assert!((0..super::PAD_WAIT).all(|_| !found.no_pad_at_start(false, true)));
        assert!(!found.off_for_session);
    }

    #[test]
    fn a_tap_of_lb_flips_autorun_once() {
        use dereth_client_runtime::actions::movement::{command, on_action, MovementAction};
        let flips = super::autorun_tap()
            .iter()
            .filter(|a| {
                matches!(
                    on_action(a, |_| None),
                    MovementAction::SetMotion(c) if c.command == command::AUTO_RUN
                )
            })
            .count();
        assert_eq!(flips, 1);
    }
}
