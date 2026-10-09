//! The interface: the pre-game screens (title, data centre and world select, lobby) and the
//! in-game HUD and windows, drawn from the interface's own art.
//!
//! It is an immediate-mode interface: every frame the current screen is drawn from the game's
//! state ([`game::GameState`], taken by the shell from the runtime) and this frame's input
//! ([`input::InputFrame`]), and what the player did comes back as requests the shell hands to the
//! runtime ([`Outcome`]).

pub mod chat;
pub mod colours;
pub mod creation;
pub mod edit;
pub mod game;
pub mod hud;
pub mod input;
pub mod kit;
pub mod layout;
pub mod nav;
pub mod paint;
pub mod panels;
pub mod pregame;

use std::sync::Arc;

use dereth_client_contract::pregame::CharacterAction;
use dereth_client_contract::UiRequest;

use crate::art::Art;
use crate::draw::DrawList;
use crate::options::{HorizonOptions, StartScreen};

/// A time or a small fraction of one, narrowed for drawing arithmetic.
#[must_use]
pub fn narrow(v: f64) -> f32 {
    // A drawing fraction or a few seconds; never beyond f32's range.
    #[allow(clippy::cast_possible_truncation)]
    let f = v as f32;
    f
}

/// A sine wave of `time` at `rate` radians a second, from `0` to `1`.
#[must_use]
pub fn wave(time: f64, rate: f64) -> f32 {
    narrow(dereth_primitives::num::math::sin(time * rate) * 0.5 + 0.5)
}

/// The action that toggles between peace and the combat mode the wielded weapon calls for.
pub const ACTION_TOGGLE_COMBAT: u32 = 0x1000_005A;

/// An item or shortcut in hand: picked up by a press, a drag once the pointer moves.
#[derive(Debug, Clone)]
pub struct Drag {
    pub item: dereth_primitives::ObjectId,
    /// The shortcut slot it was picked up from, if it is a shortcut.
    pub from_shortcut: Option<u32>,
    /// Its icon, drawn under the pointer when there is no better picture.
    pub icon: Option<u32>,
    /// What it is, for the picture that follows the pointer: the item, or the spell.
    pub look: Option<crate::ui::game::Item>,
    /// A spell's icon power and bitfield, for its composed icon.
    pub spell_look: Option<(u32, u32)>,
    /// Where the press was.
    pub origin: (f32, f32),
    /// The pointer has moved far enough for this to be a drag and not a click.
    pub active: bool,
    /// What a click (a release without a drag) asks for.
    pub on_click: Option<UiRequest>,
    /// A spell rather than an item: its id, and the spell bar slot it was picked up from.
    pub spell: Option<u32>,
    pub from_spell_slot: Option<(usize, usize)>,
    /// A spell component, its icon drawn as the component lists draw it.
    pub component: bool,
    /// One of the fighting stance's controls, picked up from the spellbook's Combat page: only a
    /// cross hotbar slot takes it.
    pub stance: Option<hud::cross::PowerAct>,
}

/// How far the pointer must move, in pixels, before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 5.0;

/// Which screen is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// Character select, the first screen.
    Lobby,
    /// A log-on has gone out; the world is loading.
    Entering,
    /// A new character is being made.
    Creation,
    Game,
}

/// What the player asked for this frame, for the shell to carry out.
#[derive(Debug, Default)]
pub struct Outcome {
    pub requests: Vec<UiRequest>,
    pub character_actions: Vec<CharacterAction>,
    /// What character creation asked for.
    pub chargen_actions: Vec<dereth_client_contract::pregame::CharGenAction>,
    /// What the key bindings page asked of the input manager.
    pub key_requests: Vec<crate::ui::game::KeyRequest>,
    /// Chat lines typed, to go through the runtime's command interpreter.
    pub chat: Vec<String>,
    /// The client should close.
    pub quit: bool,
    /// Log the character off.
    pub log_off: bool,
    /// Actions to fire, as a key bound to them would (the action map's ids).
    pub actions: Vec<u32>,
    /// A setting changed: `(name, value)` in the preferences file's terms.
    pub settings: Vec<(String, String)>,
    /// The game's boxes answered: `(id, yes)`, OK counting as yes.
    pub answers: Vec<(u64, bool)>,
    /// Objects to select by name, from a script: the nearest on the radar with that name.
    pub select: Vec<String>,
    /// The spell whose details the spellbook shows.
    pub examine_spell: Option<u32>,
    /// The spell whose details the spell information window shows.
    pub identify_spell: Option<u32>,
    /// Open the chat line with this already typed (a tell to a friend).
    pub chat_prefill: Option<String>,
    /// Actions to fire as a key tapped: pressed and let go.
    pub action_taps: Vec<u32>,
    /// Something to bind to a cross hotbar slot: the bind-to-hotbar notice comes up with it.
    pub bind_to_hotbar: Option<hud::cross::CrossBind>,
    /// A drop on a cross hotbar slot: the slot, and what it binds there.
    pub cross_bind: Option<(usize, hud::cross::CrossBind)>,
    /// The book window was closed.
    pub book_closed: bool,
    /// An allegiance action to ask about: the action, the other player, the question's string
    /// token, and the name that fills it.
    pub allegiance_question: Option<(
        dereth_client_contract::view::AllegianceAction,
        dereth_primitives::ObjectId,
        &'static str,
        String,
    )>,
}

/// The interface's whole state.
#[derive(Debug)]
pub struct HorizonUi {
    pub art: Arc<Art>,
    pub options: HorizonOptions,
    pub screen: Screen,
    pub colours: colours::Colours,
    pub pregame: pregame::Pregame,
    pub hud: hud::Hud,
    pub windows: panels::Windows,
    /// Seconds since the interface came up.
    pub time: f64,
    /// Whether the interface has something Escape would close.
    pub wants_escape: bool,
    /// Where a drag could land last frame, and what a drop there asked for.
    pub drops: Vec<(crate::draw::Rect, Option<kit::Drop>)>,
    /// Whether the pointer is over something of the interface this frame.
    pub pointer_over_ui: bool,
    /// Whether a text box has the keyboard.
    pub text_focus: bool,
    preview_shown: bool,
    /// When the world came up, and how many of the scripted lines have been sent.
    in_world_since: Option<f64>,
    said: usize,
    /// What the player has in hand.
    pub drag: Option<Drag>,
}

impl HorizonUi {
    #[must_use]
    pub fn new(art: Arc<Art>, options: HorizonOptions) -> Self {
        let colours = colours::Colours::default();
        let screen = match options.screen {
            StartScreen::Lobby => Screen::Lobby,
            StartScreen::Game => Screen::Game,
        };
        let mut windows = panels::Windows::default();
        for w in &options.open {
            windows.open_by_name(w);
        }
        Self {
            art,
            options,
            screen,
            colours,
            pregame: pregame::Pregame::default(),
            hud: hud::Hud::default(),
            windows,
            time: 0.0,
            wants_escape: false,
            drops: Vec::new(),
            pointer_over_ui: false,
            text_focus: false,
            preview_shown: false,
            in_world_since: None,
            said: 0,
            drag: None,
        }
    }

    /// The sample player, with its chat only on the first frame.
    fn preview_state(&mut self) -> game::GameState {
        let mut s = game::GameState::preview();
        if self.preview_shown {
            s.new_chat.clear();
        }
        self.preview_shown = true;
        s
    }

    /// The layout scale for a screen of `height` pixels: the player's scale, a layout unit to a
    /// pixel at 100% on any screen of 1080 lines or more (a taller screen shows more, not
    /// larger); a shorter screen, which the layouts do not fit, shrinks them to fit its height.
    #[must_use]
    pub fn layout_scale(&self, screen: (f32, f32)) -> f32 {
        let fit = (screen.1 / 1080.0).clamp(0.5, 1.0);
        fit * self.options.scale.unwrap_or(1.0)
    }

    /// Draw one frame of the interface into `list` and say what the player did.
    pub fn frame(
        &mut self,
        list: &mut DrawList,
        screen: (f32, f32),
        dt: f64,
        state: &game::GameState,
        input: &mut input::InputFrame,
    ) -> Outcome {
        self.time += dt;
        list.clear();
        // With no server to reach, a start on the game screen shows the HUD over a sample player.
        let preview;
        let state = if self.options.screen == StartScreen::Game && state.host.is_empty() {
            preview = {
                // The sample player speaks where the game would let them.
                let mut s = self.preview_state();
                s.chat_focus = state.chat_focus.clone();
                s
            };
            &preview
        } else {
            state
        };

        let scale = self.layout_scale(screen);
        let art = Arc::clone(&self.art);
        let mut p = paint::Painter {
            list,
            art: &art,
            scale,
            screen,
            fade: 1.0,
        };
        let mut out = Outcome::default();
        // The scripted steps: the first three seconds into the world, then one every two.
        if state.in_world && state.connected {
            let since = *self.in_world_since.get_or_insert(self.time);
            let due = self.said < self.options.script.len()
                && self.time - since
                    >= 3.0 + 2.0 * f64::from(u32::try_from(self.said).unwrap_or(0));
            if due {
                match &self.options.script[self.said] {
                    crate::options::ScriptStep::Say(line) => out.chat.push(line.clone()),
                    crate::options::ScriptStep::Action(id) => out.actions.push(*id),
                    crate::options::ScriptStep::Select(name) => out.select.push(name.clone()),
                }
                self.said += 1;
            }
        } else {
            self.in_world_since = None;
        }
        let mut ctx = kit::Ctx {
            time: self.time,
            dt,
            input,
            colours: &self.colours,
            hot: false,
            drag: &mut self.drag,
            drops: Vec::new(),
        };
        // The screen follows the game: the world is up once the player is in it.
        if state.in_world && self.screen != Screen::Game {
            self.screen = Screen::Game;
        }
        if !state.in_world && self.screen == Screen::Game && state.connected {
            self.screen = Screen::Lobby;
        }
        match self.screen {
            Screen::Lobby | Screen::Entering | Screen::Creation => {
                self.pregame.frame(
                    &mut self.screen,
                    &mut p,
                    &mut ctx,
                    state,
                    &self.options,
                    &mut out,
                );
            }
            Screen::Game => {
                // A modal box takes this frame's presses, keys and typing before anything under
                // it: they are held back from the HUD and the windows until the box's turn.
                // While the key bindings page waits for a key, every key is the game's: the input
                // manager hands it to the page.
                if state
                    .key_bindings
                    .as_ref()
                    .and_then(|k| k.capture.as_ref())
                    .is_some_and(|c| c.question.is_none())
                {
                    ctx.input.keys.clear();
                    ctx.input.chars.clear();
                }
                let modal = self.hud.log_out_open()
                    || state.prompts.iter().any(|p| p.modal)
                    || self.windows.options_modal(state)
                    || self.windows.questions_open();
                let held = modal.then(|| {
                    (
                        std::mem::take(&mut ctx.input.pressed),
                        std::mem::take(&mut ctx.input.keys),
                        std::mem::take(&mut ctx.input.chars),
                    )
                });
                // The game's boxes are drawn last, over everything: a press on one is the box's,
                // and nothing drawn under it may take it first.
                let (mx, my) = ctx.input.mouse;
                let on_a_box = !modal
                    && state.prompts.iter().enumerate().any(|(i, prompt)| {
                        pregame::dialog_rect(&p, &prompt.text, i).contains(mx, my)
                    });
                let held_press = on_a_box.then(|| std::mem::take(&mut ctx.input.pressed));
                self.hud.other_text_focus = self.windows.has_text_focus();
                // The HUD is under every window: the pointer over a window is not over it.
                ctx.input.occluders = self
                    .windows
                    .rects(scale)
                    .into_iter()
                    .map(|(_, r)| r)
                    .chain(self.windows.pad_box)
                    .chain(self.hud.pad_menu_rect)
                    .collect();
                // The log window's popped-out tabs are over the HUD too.
                ctx.input.occluders.extend(self.hud.log.popped_rects());
                ctx.input.occluders.extend(self.hud.log.menu_rect());
                self.hud.screen_middle = p.screen.0 / 2.0;
                self.hud
                    .pad_world(&mut ctx, state, &mut self.windows, &mut out);
                self.hud
                    .game_keys(&mut ctx, state, &mut self.windows, &mut out);
                self.hud
                    .frame(&mut p, &mut ctx, state, &mut self.windows, &mut out);
                self.hud.alt_ring(&mut p, &ctx, state);
                self.windows.spell_tab = self.hud.spell_tab();
                self.windows.options_page.scale = self.options.scale.unwrap_or(1.0);
                self.windows.options_page.orbit = self.options.orbit;
                self.windows.options_page.pad = self.options.pad;
                self.windows.options_page.minimap_rotates = self.options.minimap_rotates;
                self.hud.minimap_rotates = self.options.minimap_rotates;
                self.windows
                    .options_page
                    .chat_tabs
                    .clone_from(&self.options.chat_tabs);
                self.hud.log.tab_names.clone_from(&self.options.chat_tabs);
                self.windows.options_page.chat_opacity = self.options.chat_opacity;
                self.hud.log.opacity = self.options.chat_opacity;
                self.hud.log.popped_saved = self.options.chat_popped;
                self.hud.log.sizes_saved = self.options.chat_sizes;
                self.hud.log.editing = self.windows.is_open(panels::WindowId::Layout);
                // The log window's popped-out tabs and its menus are over the HUD and under every
                // window: the pointer over a window is not over them.
                ctx.input.occluders = self
                    .windows
                    .rects(scale)
                    .into_iter()
                    .map(|(_, r)| r)
                    .chain(self.windows.pad_box)
                    .chain(self.hud.pad_menu_rect)
                    .collect();
                // The HUD Layout window and the outlines it moves are over them too.
                let layout = self.hud.layout_rects(&self.windows, scale, p.screen);
                let windows_only = ctx.input.occluders.len();
                ctx.input.occluders.extend(layout);
                self.hud.log.overlays(&mut p, &mut ctx, state, &mut out);
                ctx.input.occluders.truncate(windows_only);
                self.hud.layout_overlay(&mut p, &mut ctx, &mut self.windows);
                self.windows.frame(&mut p, &mut ctx, state, &mut out);
                self.hud.pad_menu(&mut p, &mut ctx, &mut self.windows);
                if let Some(text) = out.chat_prefill.take() {
                    self.hud.open_chat(text);
                }
                self.hud.frame_overlays(&mut p, &mut ctx, state, &mut out);
                if let Some((pressed, keys, chars)) = held {
                    ctx.input.pressed = pressed;
                    ctx.input.keys = keys;
                    ctx.input.chars = chars;
                }
                if let Some(pressed) = held_press {
                    ctx.input.pressed = pressed;
                }
                prompts(&mut p, &mut ctx, state, &mut out);
                self.hud.log_out_question(&mut p, &mut ctx, &mut out);
                self.windows
                    .options_questions(&mut p, &mut ctx, state, &mut out);
                self.windows.questions(&mut p, &mut ctx, state, &mut out);
                drag_step(&mut p, &mut ctx, state, &self.hud.drop_slots, &mut out);
                if let Some((slot, bind)) = out.cross_bind.take() {
                    self.hud.cross.assign(slot, Some(bind));
                }
                if let Some(bind) = out.bind_to_hotbar.take() {
                    self.hud.cross.binding = Some(bind);
                }
            }
        }
        if state.disconnected {
            let text = state
                .failure
                .clone()
                .unwrap_or_else(|| "The connection to the server was lost.".to_owned());
            if pregame::message_box(&mut p, &mut ctx, "Unable to Continue", &text, &["Exit"])
                .is_some()
            {
                out.quit = true;
            }
        }
        hud::pad::overlays(&mut p, &mut ctx);
        self.drops = std::mem::take(&mut ctx.drops);
        self.pointer_over_ui = ctx.hot || ctx.input.captured;
        self.text_focus = ctx.input.text_focus;
        // Escape is the interface's while it has something to close or let go of; otherwise it is
        // the game's (cancel a use, let a power bar go).
        self.wants_escape = self.screen != Screen::Game
            || self.text_focus
            || self.drag.is_some()
            || state.prompts.iter().any(|p| p.modal)
            || self.hud.wants_escape()
            || self.windows.any_open()
            || (self.screen == Screen::Game
                && (hud::escape_deselects(state, false) || hud::escape_logs_out(state, false)));
        out
    }
}

/// The game's boxes: its messages, one OK each, and its question on screen, Yes or No, with how
/// many more wait behind it.
fn prompts(
    p: &mut paint::Painter<'_>,
    ctx: &mut kit::Ctx<'_>,
    state: &game::GameState,
    out: &mut Outcome,
) {
    // The pad's A says yes to the question on top.
    if std::mem::take(&mut ctx.input.pad.confirm) {
        if let Some(prompt) = state.prompts.last() {
            out.answers.push((prompt.id, true));
        }
    }
    for (i, prompt) in state.prompts.iter().enumerate() {
        let (title, buttons): (&str, &[&str]) = if prompt.question {
            ("Confirm", &["Yes", "No"])
        } else {
            ("Notice", &["OK"])
        };
        let title = if prompt.waiting > 0 {
            format!("{title} ({} more waiting)", prompt.waiting)
        } else {
            title.to_owned()
        };
        if let Some(b) = pregame::dialog_box(p, ctx, &title, &prompt.text, buttons, prompt.modal, i)
        {
            out.answers.push((prompt.id, b == 0));
        }
    }
}

/// The game's actions the interface answers itself, rather than the game: the shortcut keys and
/// the make-shortcut key, the windows' keys, and logging out and quitting.
#[must_use]
pub fn takes_action(action: u32) -> bool {
    dereth_ui_screens::toolbar::shortcuts::shortcut_action(action).is_some()
        || hud::window_for_action(action).is_some()
        || chat::is_chat_action(action)
        || action == hud::ACTION_LOG_OUT
        || action == hud::ACTION_QUIT
        || action == hud::ACTION_USE
        || action == hud::ACTION_NEXT_MONSTER
        || action == hud::ACTION_PREVIOUS_MONSTER
}

/// The requests that put `spell_id` at `index` on a tab holding `rows`. A spell the tab already
/// holds moves there instead of appearing twice; a slot after its old place is one earlier once
/// it has left.
pub(crate) fn place_spell(rows: &[u32], spell_id: u32, index: usize, tab: usize) -> Vec<UiRequest> {
    let index = i32::try_from(index).unwrap_or(i32::MAX);
    dereth_client_contract::spellbook::FavoritePlan::new(rows, rows, spell_id, index, true)
        .map_or_else(Vec::new, |plan| plan.requests(spell_id, tab))
}

/// A thing carried, drawn at `(x, y)`: lifted a little off where it is held, on a dark slot of
/// its own, so it reads over windows and the world alike.
pub(crate) fn draw_lifted(
    p: &mut paint::Painter<'_>,
    icon: &crate::art::Sprite,
    (x, y): (f32, f32),
) {
    let k = p.scale;
    let s = 40.0 * k;
    let r = crate::draw::Rect::new(x - s / 2.0 - 4.0 * k, y - s / 2.0 - 6.0 * k, s, s);
    let back = r.inset(-5.0 * k);
    p.fill(back.offset(3.0 * k, 4.0 * k), 0x6000_0000);
    p.fill(back, 0xE010_0E0C);
    if let Some(frame) = p.piece("slot.empty") {
        p.sprite(&frame, back, crate::draw::WHITE);
    }
    p.sprite(icon, r, crate::draw::WHITE);
}

/// The drag's frame: it becomes a drag once the pointer moves; while it is one its icon follows
/// the pointer; and the release either clicks what was pressed or drops what is in hand.
fn drag_step(
    p: &mut paint::Painter<'_>,
    ctx: &mut kit::Ctx<'_>,
    state: &game::GameState,
    drop_slots: &[(crate::draw::Rect, u32)],
    out: &mut Outcome,
) {
    let Some(drag) = ctx.drag.as_mut() else {
        return;
    };
    let (mx, my) = ctx.input.mouse;
    if !drag.active
        && ctx.input.down[0]
        && ((mx - drag.origin.0).abs() > DRAG_THRESHOLD
            || (my - drag.origin.1).abs() > DRAG_THRESHOLD)
    {
        drag.active = true;
        // A shortcut picked up leaves its slot at once, as the game's own bar does; a drop that
        // lands nowhere leaves it removed.
        if drag.from_shortcut.is_some() {
            out.requests.push(UiRequest::RemoveShortcut(drag.item));
        }
        if let (Some(spell_id), Some((tab, _))) = (drag.spell, drag.from_spell_slot) {
            out.requests
                .push(UiRequest::RemoveSpellFavorite { spell_id, tab });
        }
    }
    if drag.active {
        ctx.input.captured = true;
        if let Some(icon) = drag_picture(p, drag) {
            draw_lifted(p, &icon, (mx, my));
        }
    }
    if ctx.input.down[0] {
        return;
    }
    let Some(drag) = ctx.drag.take() else {
        return;
    };
    if !drag.active {
        out.requests.extend(drag.on_click);
        return;
    }
    let slot = drop_slots
        .iter()
        .find(|(r, _)| r.contains(mx, my))
        .map(|(_, s)| *s);
    // A drop on a cross hotbar slot binds what was dropped there.
    if let Some((_, Some(kit::Drop::CrossSlot(index)))) =
        ctx.drops.iter().rev().find(|(r, _)| r.contains(mx, my))
    {
        let bind = match (drag.stance, drag.spell) {
            (Some(act), _) => hud::cross::CrossBind::Power(act),
            (None, Some(id)) => hud::cross::CrossBind::Spell(id),
            (None, None) => hud::cross::CrossBind::Item(drag.item),
        };
        out.cross_bind = Some((*index, bind));
        return;
    }
    // A stance control lands nowhere but on a cross hotbar slot.
    if drag.stance.is_some() {
        return;
    }
    if let Some(spell_id) = drag.spell {
        let slot = ctx.drops.iter().rev().find(|(r, _)| r.contains(mx, my));
        if let Some((_, Some(kit::Drop::SpellSlot { tab, index }))) = slot {
            let mut rows = state.spell_tabs.get(*tab).cloned().unwrap_or_default();
            let mut index = *index;
            // A move along the same tab has already left its old place. While the bar still
            // shows it there, a slot after it is one earlier once it has gone.
            if let Some((from_tab, from)) = drag.from_spell_slot {
                if from_tab == *tab && rows.get(from) == Some(&spell_id) {
                    rows.remove(from);
                    if from < index {
                        index -= 1;
                    }
                }
            }
            out.requests
                .extend(place_spell(&rows, spell_id, index, *tab));
        }
        return;
    }
    // A drop on a window: its slot, its container, or nothing.
    let zone = ctx
        .drops
        .iter()
        .rev()
        .find(|(r, _)| r.contains(mx, my))
        .map(|(_, t)| *t);
    if slot.is_none() {
        if let Some(target) = zone {
            if let (Some(target), None) = (target, drag.from_shortcut) {
                out.requests.extend(target.request(drag.item));
                // What was dropped is selected, wherever it went.
                out.requests.push(UiRequest::Select(drag.item));
            }
            return;
        }
    }
    match (slot, drag.from_shortcut) {
        (Some(slot), Some(from)) => out.requests.push(UiRequest::DragDrop {
            item: drag.item,
            target: dereth_client_contract::view::DropTarget::ShortcutAlias {
                slot,
                from: i32::try_from(from).unwrap_or(-1),
            },
        }),
        (Some(slot), None) => out.requests.push(UiRequest::DragDrop {
            item: drag.item,
            target: dereth_client_contract::view::DropTarget::ShortcutSlot(slot),
        }),
        (None, None) if !ctx.hot => {
            out.requests.push(UiRequest::DragDrop {
                item: drag.item,
                target: dereth_client_contract::view::DropTarget::World,
            });
            out.requests.push(UiRequest::Select(drag.item));
        }
        _ => {}
    }
}

/// What follows the pointer while `drag` is in hand: an item's drag picture, a spell's composed
/// icon, or the bare icon.
fn drag_picture(p: &paint::Painter<'_>, drag: &Drag) -> Option<crate::art::Sprite> {
    if let Some(item) = &drag.look {
        if let Some(s) = p.art.ac_item_drag(item) {
            return Some(s);
        }
    }
    if let (Some(icon), Some((power, bitfield))) = (drag.icon, drag.spell_look) {
        if let Some(s) = p.art.ac_spell(icon, power, bitfield) {
            return Some(s);
        }
    }
    if drag.component {
        return drag.icon.and_then(|d| p.art.ac_component(d));
    }
    drag.icon.and_then(|d| p.art.ac_icon(d))
}
