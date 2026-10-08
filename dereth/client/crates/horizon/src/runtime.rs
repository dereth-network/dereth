//! The Horizon front end: everything the shared runtime's frame hands the Horizon interface, and the
//! state it keeps across frames. It reaches the game through the front-end context alone: it
//! reads the model, and asks for everything it wants done.
//!
//! The client's shell owns the device input and the window; it hands each raw event to
//! [`HorizonFrontEnd::host_event`] first, which keeps what lands on the interface (presses over a
//! window, keys while a text box has the keyboard) and says whether the rest goes on to the input
//! manager. The actions the input manager makes reach [`HorizonFrontEnd::frame`], which keeps the ones
//! the interface answers itself (the shortcut and window keys, logging out) and the mouse actions
//! that began over the world, and gives the rest back.

use std::path::PathBuf;
use std::sync::Arc;

use dereth_client_contract::requests::Outbox;
use dereth_client_contract::view::GameView;
use dereth_client_contract::UiRequest;
use dereth_client_runtime::interaction::UiMouseEvent;
use dereth_client_runtime::present::Presentation as _;
use dereth_client_runtime::shell::{Shell, UiNotices};
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_primitives::{LocalTime, ObjectId};

use crate::art::Art;
use crate::dialogs::{HorizonDialogs, Presenter};
use crate::draw::{DrawList, Overlay};
use crate::options::HorizonOptions;
use crate::state::Projections;
use crate::ui::input::InputFrame;
use crate::ui::HorizonUi;

/// The front-end context the shared runtime hands the Horizon interface at each step.
pub type Cx<'a, S> = dereth_client_runtime::ui_context::UiContext<'a, S>;

/// The string table the panels' messages are words of.
const PANEL_STRING_TABLE: u32 = 0x2300_0001;

/// The string table the disconnection notices are words of.
const DISCONNECT_TABLE: u32 = dereth_ui_screens::screens::disconnected::STRING_TABLE_ENUM;

/// The input manager's mouse actions: the wheel (5, 6), the buttons (7 left, 8 right, 9 middle),
/// the double-clicks (10 to 12) and the taps (13 to 15).
const MOUSE_ACTIONS: std::ops::RangeInclusive<u32> = 5..=15;

/// The Horizon interface's receivers for what the HUD model offers: every line goes to the chat, where
/// the log window takes the game's floating feedback out for itself, and the abuse log's answer
/// is kept for the report form.
#[derive(Debug, Default)]
pub struct HorizonHudPanels {
    pub abuse: Option<u32>,
}

impl dereth_client_runtime::hud::HudPanels for HorizonHudPanels {
    fn spew_offer(
        &mut self,
        _ty: u8,
        _body: &str,
        _feedback: dereth_client_contract::feedback::Feedback,
    ) -> bool {
        false
    }
    fn spew_trace(&self) -> (bool, usize, u64) {
        (false, 0, 0)
    }
    fn spew_clear_pending(&mut self) {}
    fn abuse_response(&mut self, code: u32) {
        self.abuse = Some(code);
    }
}

/// What became of one raw window event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Routed {
    /// It goes on to the input manager and the world.
    pub forward: bool,
}

/// The box id the screen-size question is shown under, apart from the game's questions.
const RESOLUTION_PROMPT: u64 = u64::MAX;

/// The block the screens before the world show behind them: Holtburg.
const BACKDROP_LANDBLOCK: u16 = 0xA9B4;
/// How far either way the backdrop's camera swings, in radians, and how long a swing there and
/// back takes, in seconds.
const BACKDROP_SWING: f32 = 0.45;
const BACKDROP_PERIOD: f64 = 80.0;

/// How much further back than the other interfaces' the doll's camera stands.
const DOLL_DISTANCE: f32 = 1.15;
/// How much higher the doll's camera stands, in metres, so the figure sits in the middle of its
/// column rather than towards the top.
const DOLL_RAISE: f32 = 0.12;
/// How much higher the doll's camera stands in the Character window, in metres: lower than on
/// character select, so the figure stands higher in its frame, the feet clear of its foot.
const WINDOW_DOLL_RAISE: f32 = -0.03;

/// The creation preview's vertical field of view, in radians: a figure of two metres fills it
/// from the shared creation camera's distance.
const CREATION_FOV: f32 = 0.75;
/// How much further back than the shared creation camera the model is seen from.
const CREATION_DISTANCE: f32 = 1.45;
/// How much further back than the shared face camera the close camera stands, so the shoulders
/// show with the face.
const CREATION_FACE_DISTANCE: f32 = 2.6;

/// How often the link is timed, in seconds.
const PING_SECONDS: f64 = 10.0;

/// The Horizon front end.
#[derive(Debug)]
pub struct HorizonFrontEnd {
    pub ui: HorizonUi,
    pub frame_input: InputFrame,
    list: DrawList,
    overlay: Overlay,
    /// The requests no step of the interface took, for the runtime's own owners.
    pub outbox: Outbox,
    last_time: Option<f64>,
    /// When the previews last moved, in the runtime's seconds.
    preview_clock: Option<f64>,
    /// Whether each pointer button went down over the world, so its release goes there too.
    world_press: [bool; 2],
    /// The game's string tables, read as they are needed.
    strings: crate::strings::Strings,
    /// The game's questions and messages on screen.
    pub dialogs: HorizonDialogs,
    /// The object the examine window shows, and the last examine request seen.
    examining: Option<ObjectId>,
    examine_serial: u64,
    /// The game asked for the examine window to close.
    examine_close: bool,
    /// Whether the examine window is on screen.
    examine_open: bool,
    /// The chest or corpse open on the ground.
    ground: Option<ObjectId>,
    /// The game asked for the vendor's buying tab.
    shop_buying: bool,
    /// The spell the spellbook shows the details of.
    spell_detail: Option<u32>,
    /// The spell the spell information window shows.
    spell_identify: Option<u32>,
    /// `@clear` was asked for since the last frame.
    chat_cleared: bool,
    /// The link: when the last ping went, how many answers had come by then, and the last
    /// round trip.
    ping_sent: Option<(f64, u64)>,
    ping_ms: Option<f64>,
    /// When the last ping went.
    ping_last: Option<f64>,
    /// The book opening the player closed or walked away from, which stays closed.
    book_closed: Option<u64>,
    /// The power bar: the mode it was begun in, its level, and whether it is up.
    power_mode: dereth_client_contract::powerbar::PowerBarMode,
    power_level: f32,
    power_up: bool,
    /// The data update's progress, followed across frames.
    patch: crate::state::PatchProgress,
    /// Chat lines that arrived while the player was entering the world, before the game screen
    /// was up to show them.
    early_chat: Vec<dereth_client_contract::chat::interface::ChatMessage>,
    /// The selection the ring lies under, kept from the frame, and the time it was kept at.
    ring_target: Option<(ObjectId, crate::ui::game::Relation)>,
    ring_time: f64,
    /// Whether the ring's picture is uploaded.
    ring_uploaded: bool,
    /// Whether the art gives the ring an inner layer, which turns against the outer one.
    ring_inner: bool,
    /// Whether the game screen was up last frame, for its entry and exit edges.
    was_gameplay: Option<bool>,
    /// Where the objects whose names may show are on screen, as the shell projected them.
    pub projections: Projections,
    /// Where each object's own origin stands on screen, raised to its middle: the point names
    /// and the selection marker centre on, which does not swing as the object turns.
    pub origins: std::collections::BTreeMap<ObjectId, (i32, i32)>,
    /// The objects the shell is asked to project for the next frame.
    pub wanted_projections: Vec<ObjectId>,
    /// The interface's settings file.
    settings_path: Option<PathBuf>,
    /// The runtime placed the portal space this frame: the swirl is drawn under everything.
    pub portal: bool,
    /// The key bindings as the input manager holds them, which the shell keeps up to date.
    pub keys: Option<crate::ui::game::KeyBindingsView>,
    /// What the key bindings page asked of the input manager, for the shell to carry out.
    pub key_requests: Vec<crate::ui::game::KeyRequest>,
    /// The chat channels have been joined since the character came into the world.
    channels_joined: bool,
    /// The abuse log's last answer, by its code, until it is put in words.
    abuse_code: Option<u32>,
    /// The abuse log's last answer, in the game's words.
    abuse_answer: Option<String>,
    /// The screen-size question the runtime asks, while it asks it.
    resolution: Option<dereth_client_contract::resolution::ResolutionPrompt>,
    /// What the creation model in its preview space was dressed from, and how it is turned.
    creation_built: Option<(
        dereth_primitives::DataId,
        dereth_animation::parts::ObjDesc,
        u32,
    )>,
    /// The palette sets the creation model is coloured from, read once each.
    creation_palettes: dereth_scene::preview::PaletteSetCache,
    /// What the paper doll was dressed from: the player's setup and look.
    doll_built: Option<(dereth_primitives::DataId, dereth_animation::parts::ObjDesc)>,
    /// How each character looked when it last stood in the world, for character select.
    looks: crate::looks::Looks,
    /// The look of the character selected on character select, when one is remembered.
    lobby_look: Option<crate::looks::Look>,
    /// What went wrong, for the shell to log.
    pub errors: Vec<String>,
    /// The chat windows' titles the game has set, by window id.
    chat_titles: std::collections::BTreeMap<u32, String>,
    /// The destinations' labels and the floating windows' first titles, in the game's words.
    chat_words: Option<(Vec<String>, Vec<Option<String>>)>,
    /// Which talk-focus destinations could be picked as of the last frame, for the notices that
    /// switch one off.
    focus_enabled: [bool; 14],
}

impl HorizonFrontEnd {
    /// The front end over `art`, started with `options`; its settings are saved to
    /// `settings_path` when it has one.
    #[must_use]
    pub fn new(art: Arc<Art>, options: HorizonOptions, settings_path: Option<PathBuf>) -> Self {
        let looks = crate::looks::Looks::load(
            settings_path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|d| d.join(crate::looks::FILE_NAME)),
        );
        Self {
            ui: HorizonUi::new(art, options),
            frame_input: InputFrame::default(),
            list: DrawList::default(),
            overlay: Overlay::default(),
            outbox: Outbox::owned(),
            last_time: None,
            preview_clock: None,
            world_press: [false; 2],
            strings: crate::strings::Strings::default(),
            dialogs: HorizonDialogs::default(),
            examining: None,
            examine_serial: 0,
            examine_close: false,
            examine_open: false,
            ground: None,
            shop_buying: false,
            spell_detail: None,
            spell_identify: None,
            chat_cleared: false,
            ping_sent: None,
            ping_ms: None,
            ping_last: None,
            book_closed: None,
            power_mode: dereth_client_contract::powerbar::PowerBarMode::Undef,
            power_level: 0.0,
            power_up: false,
            early_chat: Vec::new(),
            patch: crate::state::PatchProgress::default(),
            ring_target: None,
            ring_time: 0.0,
            ring_uploaded: false,
            ring_inner: false,
            was_gameplay: None,
            projections: Projections::new(),
            origins: std::collections::BTreeMap::new(),
            wanted_projections: Vec::new(),
            settings_path,
            portal: false,
            resolution: None,
            creation_built: None,
            creation_palettes: dereth_scene::preview::PaletteSetCache::default(),
            doll_built: None,
            looks,
            lobby_look: None,
            keys: None,
            key_requests: Vec::new(),
            channels_joined: false,
            abuse_code: None,
            abuse_answer: None,
            errors: Vec::new(),
            chat_titles: std::collections::BTreeMap::new(),
            chat_words: None,
            focus_enabled: [true; 14],
        }
    }

    /// Bring the interface up: the game's own data for its icons and words, and the option rows
    /// System Configuration lists.
    pub fn start<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        let store = Arc::clone(cx.store());
        self.ui.art.set_ac_store(Arc::clone(&store));
        self.ui.hud.settings_dir = cx
            .config()
            .preferences_file
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf);
        self.ui.hud.log_out_text = self.strings.text(
            &store,
            dereth_ui_screens::screens::gameplay::logout::STRING_TABLE_ENUM,
            dereth_ui_screens::screens::gameplay::logout::END_SESSION_CONFIRM,
        );
        self.reread_option_rows(cx);
    }

    /// The option rows System Configuration lists, read again from the store and the data.
    pub fn reread_option_rows<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        let store = Arc::clone(cx.store());
        self.ui.windows.options = crate::state::option_rows(&store, &mut self.strings);
        self.ui.windows.character_options =
            crate::state::character_options(&store, &mut self.strings);
    }

    /// Read the data files again, as a data patch left them.
    pub fn reread_files<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        self.strings = crate::strings::Strings::default();
        self.chat_words = None;
        let art = Arc::new(self.ui.art.reopened());
        art.set_ac_store(Arc::clone(cx.store()));
        self.set_art(cx, art);
        self.reread_option_rows(cx);
    }

    /// Draw from `art` from now on, letting go of every picture drawn from the old.
    pub fn set_art<S: Shell>(&mut self, cx: &mut Cx<'_, S>, art: Arc<Art>) {
        self.overlay.release(cx.present_mut());
        // This frame's list names the old art's pictures: none of it is drawn from the new art.
        self.list.clear();
        self.ui.art = art;
    }

    /// Whether the game screen is up.
    #[must_use]
    pub fn in_gameplay(&self) -> bool {
        self.ui.screen == crate::ui::Screen::Game
    }

    /// Whether the examine window is on screen.
    #[must_use]
    pub fn examine_panel_open(&self) -> bool {
        self.examine_open
    }

    /// The game asks for the examine window to close.
    pub fn close_examine_panel(&mut self) {
        self.examine_close = true;
    }

    /// The game asks for the vendor's buying tab.
    pub fn open_vendor_buying(&mut self) {
        self.shop_buying = true;
    }

    /// `@clear`: the log empties.
    pub fn clear_chat(&mut self) {
        self.chat_cleared = true;
    }

    /// The abuse log's answer to a report, by its code.
    pub fn abuse_response(&mut self, code: u32) {
        self.abuse_code = Some(code);
    }

    /// Whether the key bindings page is waiting for a key: every key and button then goes on to
    /// the input manager, which hands it to the page.
    #[must_use]
    pub fn capturing_key(&self) -> bool {
        self.keys.as_ref().is_some_and(|k| k.capture.is_some())
    }

    /// Whether the key going down now (a virtual key) pastes into the text being edited:
    /// Control-V or Shift-Insert while a text box has the keyboard. The host then reads its
    /// clipboard and offers the text with [`Self::offer_paste`].
    #[must_use]
    pub fn wants_paste(&self, virtual_key: usize) -> bool {
        let f = &self.frame_input;
        self.ui.text_focus && ((virtual_key == 0x56 && f.ctrl) || (virtual_key == 0x2D && f.shift))
    }

    /// The clipboard's text, for this frame's paste key.
    pub fn offer_paste(&mut self, text: Option<String>) {
        self.frame_input.paste = text;
    }

    /// Text copied or cut since the last call, for the host's clipboard.
    pub fn take_copied(&mut self) -> Option<String> {
        self.frame_input.copied.take()
    }

    /// The screen-size question, while the runtime asks it.
    pub fn project_resolution(
        &mut self,
        prompt: Option<dereth_client_contract::resolution::ResolutionPrompt>,
    ) {
        self.resolution = prompt;
    }

    /// The destinations' labels and the floating chat windows' first titles, as the game's
    /// caption strings word them, read once.
    fn chat_words<S: Shell>(&mut self, cx: &Cx<'_, S>) -> &(Vec<String>, Vec<Option<String>>) {
        const MENU: [&str; 14] = [
            "",
            "ID_Chat_ChatTargetMenu",
            "ID_Chat_ChatTargetMenuSelected",
            "ID_Chat_ChatTargetMenuFellows",
            "ID_Chat_ChatTargetMenuPatron",
            "ID_Chat_ChatTargetMenuMonarch",
            "ID_Chat_ChatTargetMenuVassals",
            "ID_Chat_ChatTargetMenuAllegiance",
            "ID_Chat_ChatTargetMenuGeneral",
            "ID_Chat_ChatTargetMenuTrade",
            "ID_Chat_ChatTargetMenuLFG",
            "ID_Chat_ChatTargetMenuRoleplay",
            "ID_Chat_ChatTargetMenuSociety",
            "ID_Chat_ChatTargetMenuOlthoi",
        ];
        if self.chat_words.is_none() {
            let store = std::sync::Arc::clone(cx.store());
            let mut word = |token: &str| {
                (!token.is_empty())
                    .then(|| self.strings.fill(&store, PANEL_STRING_TABLE, token, &[]))
                    .flatten()
            };
            let labels = MENU.iter().map(|t| word(t).unwrap_or_default()).collect();
            let titles = std::iter::once(None)
                .chain((1..=4).map(|n| word(&format!("ID_Chat_Chat{n}_DefaultTitle"))))
                .collect();
            self.chat_words = Some((labels, titles));
        }
        self.chat_words.get_or_insert_with(Default::default)
    }

    /// One talk-focus notice, a destination switched on or off: true when the destination the
    /// player has is the one switched off, and talk falls back to All, as the shared rule says.
    pub fn talk_focus_notice(
        &mut self,
        focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool {
        let row = notice.focus as usize;
        let previous = self.focus_enabled.get(row).copied().unwrap_or(false);
        let (enabled, fall_back) = dereth_client_contract::chat::mainchat::focus_enable_transition(
            previous,
            focus as u32,
            notice.focus as u32,
            notice.enabled,
            notice.is_olthoi,
        );
        if let Some(slot) = self.focus_enabled.get_mut(row) {
            *slot = enabled;
        }
        fall_back
    }

    /// The magic notices, for the spellbook.
    pub fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>) {
        self.frame_input.magic.extend(notices);
    }

    /// Whether the player is in the air, for the log-out question.
    pub fn before_ui_input(&mut self, player_airborne: bool) {
        self.frame_input.airborne = player_airborne;
    }

    /// The power bar's charge as the HUD draws it, while it is up.
    #[must_use]
    pub fn power(&self) -> Option<f32> {
        self.power_up.then_some(self.power_level)
    }

    /// The power bar's notices, in the order the game raised them: begun in a mode, levels for
    /// that mode, and finished in it.
    pub fn deliver_power_bar_notices(
        &mut self,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    ) {
        use dereth_client_model::combat::PowerBarNotice as N;
        for n in notices {
            match n {
                N::Begin { mode, .. } => {
                    self.power_mode = mode;
                    self.power_level = 0.0;
                    self.power_up = true;
                }
                // The attack's bar is never begun or finished: its levels alone fill it, and a
                // zero empties it.
                N::SetLevel {
                    mode: mode @ dereth_client_contract::powerbar::PowerBarMode::Combat,
                    level,
                } => {
                    self.power_mode = mode;
                    self.power_level = level.clamp(0.0, 1.0);
                    self.power_up = level > 0.0;
                }
                N::SetLevel { mode, level } if mode == self.power_mode => {
                    self.power_level = level.clamp(0.0, 1.0);
                }
                N::Finish { mode } if mode == self.power_mode => self.power_up = false,
                N::SetLevel { .. } | N::Finish { .. } => {}
            }
        }
    }

    /// The game's questions: shown, and answered as the dialog service's rules say.
    pub fn service_dialogs<S: Shell>(&mut self, cx: &mut Cx<'_, S>, now: LocalTime) {
        let store = Arc::clone(cx.store());
        let strings = &mut self.strings;
        let mut panel = |token: &str, name: &str| {
            strings
                .fill(&store, PANEL_STRING_TABLE, token, &[("PLAYER", name)])
                .unwrap_or_else(|| name.to_owned())
        };
        let mut presenter = Presenter {
            dialogs: &mut self.dialogs,
            accepting: self.ui.screen == crate::ui::Screen::Game,
            panel: &mut panel,
        };
        cx.service_dialogs(Some(&mut presenter), now);
        for request in self.dialogs.delivered.drain(..) {
            // A house payment question's answer is the housing window's to act on.
            if let UiRequest::HousePaymentConfirmationAnswer { rent, confirmed } = request {
                self.ui.windows.world.answers.push((rent, confirmed));
            } else {
                self.outbox.emit(request);
            }
        }
    }

    /// The character-creation tables the barber's appearance rules read.
    pub fn set_chargen_tables(
        &mut self,
        tables: Option<std::rc::Rc<dereth_ui_screens::screens::chargen::CharGenTables>>,
    ) {
        self.ui.pregame.creation_source = tables.as_ref().map(|t| {
            (
                std::rc::Rc::clone(&t.world),
                std::rc::Rc::new(t.texts.clone()),
            )
        });
        if let Some(tables) = tables {
            self.ui.windows.world.barber.set_tables(tables);
        }
    }

    /// One raw window event, a device event (lifecycle events are the shell's): the interface
    /// keeps presses over it and keys while a text box has the keyboard, and says whether the rest
    /// goes on.
    pub fn host_event(&mut self, event: &HostEvent, time_ms: u32) -> Routed {
        // While the HUD is being laid out the pointer, the wheel and the keys are the layout's:
        // nothing goes on to the game but a release, so nothing stays held.
        let laying_out = self.ui.windows.is_open(crate::ui::panels::WindowId::Layout);
        let f = &mut self.frame_input;
        let mut forward = true;
        match event {
            HostEvent::CursorMoved { x, y } => {
                #[allow(clippy::cast_possible_truncation)]
                {
                    f.mouse = (*x as f32, *y as f32);
                }
            }
            HostEvent::MouseInput { button, pressed } => {
                let i = match button {
                    MouseButton::Left => Some(0),
                    MouseButton::Right => Some(1),
                    MouseButton::Middle => Some(2),
                    _ => None,
                };
                if let Some(i) = i {
                    f.down[i] = *pressed;
                    if *pressed && i == 0 {
                        f.note_left_press(time_ms);
                    }
                    if *pressed {
                        f.pressed[i] = true;
                    } else {
                        f.released[i] = true;
                    }
                    // A press over the interface stays with it; a release goes wherever its press
                    // went.
                    if i < 2 {
                        if *pressed {
                            self.world_press[i] = !self.ui.pointer_over_ui && !laying_out;
                        }
                        forward = self.world_press[i];
                    }
                }
            }
            HostEvent::MouseWheel { notches } => {
                f.wheel += notches;
                // Over the world it goes on to the key map, which binds it to the camera's zoom;
                // over a window it scrolls that window.
                forward = !self.ui.pointer_over_ui && !laying_out;
            }
            HostEvent::KeyboardInput { key, pressed, text } => {
                if *pressed {
                    f.keys.push(key.virtual_key);
                    if let Some(t) = text {
                        f.chars.extend(t.chars().filter(|c| !c.is_control()));
                    }
                }
                if matches!(key.virtual_key, 0x10 | 0xA0 | 0xA1) {
                    f.shift = *pressed;
                }
                if matches!(key.virtual_key, 0x11 | 0xA2 | 0xA3) {
                    f.ctrl = *pressed;
                }
                // Keys go to the text box that has the keyboard, and never on to the world; a
                // release is still forwarded so no action stays held.
                if (self.ui.text_focus || laying_out) && *pressed {
                    forward = false;
                }
                // Control-C or Control-Insert copying the log's selection is the interface's: the
                // game's C key does not see it.
                if *pressed
                    && f.ctrl
                    && matches!(key.virtual_key, 0x43 | 0x2D)
                    && self.ui.hud.log.has_selection()
                {
                    forward = false;
                }
                // Escape is the interface's while it has something to close or let go of, and
                // otherwise the game's (cancel a use, let a power bar go).
                if *pressed && key.virtual_key == 0x1B && self.ui.wants_escape {
                    forward = false;
                }
            }
            _ => {}
        }
        // A key or button the key bindings page is waiting for goes on to the input manager,
        // which hands it to the page and fires nothing with it.
        if self.capturing_key()
            && matches!(
                event,
                HostEvent::KeyboardInput { .. } | HostEvent::MouseInput { .. }
            )
        {
            forward = true;
        }
        Routed { forward }
    }

    /// The actions the input manager made this frame: the interface keeps its own and the mouse
    /// actions over the world, and gives the rest back. `mouse` is where the input manager has the
    /// pointer.
    pub fn take_actions<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        events: Vec<dereth_input::InputEvent>,
        mouse: (i32, i32),
        now: LocalTime,
    ) -> Vec<dereth_input::InputEvent> {
        let gameplay = self.in_gameplay();
        let mut rest = Vec::with_capacity(events.len());
        for e in events {
            let a = e.action.0;
            // The actions the interface answers itself: shortcut and window keys, and logging
            // out. Taken on the press; the release goes nowhere.
            if gameplay && crate::ui::takes_action(a) {
                if e.start {
                    self.frame_input.actions.push(a);
                }
                continue;
            }
            if MOUSE_ACTIONS.contains(&a) {
                // A press over the interface never reaches the input manager, so every one here
                // began over the world: a click, double-click or right-click there, which the
                // world's own handler picks and acts on.
                if gameplay {
                    cx.pointer(UiMouseEvent {
                        action: a,
                        start: e.start,
                        x: mouse.0,
                        y: mouse.1,
                        over: None,
                    });
                }
                continue;
            }
            rest.push(e);
        }
        let laying_out = self.ui.windows.is_open(crate::ui::panels::WindowId::Layout);
        if gameplay && !self.ui.pointer_over_ui && !laying_out {
            cx.hover(mouse, None, true, now);
        }
        rest
    }

    /// One frame of the interface over the game's state, and what the player asked for handed to
    /// the runtime. True when the game screen came or went this frame.
    pub fn frame<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        now: LocalTime,
        mut notices: UiNotices,
    ) -> bool {
        // This interface's own camera, with the movement scheme and pointer directions chosen.
        cx.set_orbit_camera(Some(self.ui.options.orbit));
        self.deliver_power_bar_notices(std::mem::take(&mut notices.power_bar));
        let dt = self
            .last_time
            .map_or(1.0 / 60.0, |t| (now.0 - t).clamp(0.0, 0.25));
        self.last_time = Some(now.0);
        let screen = cx.present().size();
        self.ring_time = now.0;
        cx.deliver_selection_notices(now);
        // The game's view reads time (a cooldown's seconds left, an enchantment's) against the
        // clock the interface stamps each frame, as the other interfaces do; unstamped it moves
        // only when a message arrives, and what counts down stalls and jumps.
        cx.hud_mut().now = now;
        cx.sync_hud();
        self.join_channels(cx);
        let chat = cx.hud_mut().take_chat_lines(u64::from(self.in_gameplay()));
        let entering = cx.pregame().in_world
            || cx.pregame().phase == dereth_client_contract::pregame::GamePhase::EnteringWorld;
        let in_game = self.in_gameplay();
        let chat = hold_early_chat(&mut self.early_chat, in_game, entering, chat);
        let mut state = crate::state::snapshot(cx, chat);
        state.camera_heading = cx.camera_heading().unwrap_or(state.heading);
        self.patch.follow(&cx.pregame().ddd);
        state.patch_progress = self
            .patch
            .shown()
            .filter(|_| state.connect_phase == crate::ui::game::ConnectPhase::Updating);
        state.chat_entry = cx.take_chat_entry_updates();
        self.chat_titles.extend(cx.take_chat_window_titles());
        let (labels, titles) = self.chat_words(cx).clone();
        state.chat_focus_labels = labels;
        for (n, w) in state.chat_windows.iter_mut().enumerate() {
            w.title = self
                .chat_titles
                .get(&w.id)
                .cloned()
                .or_else(|| titles.get(n).cloned().flatten());
        }
        self.focus_enabled = state.chat_focus.enabled;
        state.teleporting = cx.teleporting();
        {
            let places: std::collections::BTreeMap<_, _> =
                crate::state::nameplate_candidates(&state)
                    .into_iter()
                    .filter_map(|id| Some((id, cx.object_place(id)?)))
                    .collect();
            let view = cx.hud().view(cx.objects());
            state.nameplates =
                crate::state::nameplates(&view, &state, &self.projections, &self.origins, &places);
        }
        self.wanted_projections = crate::state::nameplate_candidates(&state);
        self.ring_target = state
            .target
            .as_ref()
            .filter(|t| crate::ring::ringed(t.relation))
            .map(|t| (t.id, t.relation));
        self.remember_look(cx, &state);
        state.known_looks = state
            .characters
            .iter()
            .filter(|c| {
                self.looks
                    .get(&crate::looks::key(&state.host, &c.name))
                    .is_some()
            })
            .map(|c| c.id)
            .collect();
        state.heritages = state
            .characters
            .iter()
            .filter_map(|c| {
                let look = self.looks.get(&crate::looks::key(&state.host, &c.name))?;
                Some((c.id, look.heritage))
            })
            .collect();
        state.levels = state
            .characters
            .iter()
            .filter_map(|c| {
                let look = self.looks.get(&crate::looks::key(&state.host, &c.name))?;
                (look.level != 0).then_some((c.id, look.level))
            })
            .collect();
        self.examine_and_containers(cx, &mut state, notices);
        self.info_words(cx, &mut state);
        state.shop_buying = std::mem::take(&mut self.shop_buying);
        state.power = self.power();
        state.power_jump = self.power_mode == dereth_client_contract::powerbar::PowerBarMode::Jump;
        state.chat_cleared = std::mem::take(&mut self.chat_cleared);
        self.ping(cx, now);
        state.ping_ms = self.ping_ms;
        let shown = self.dialogs.shown();
        let waiting = shown
            .iter()
            .filter(|d| !d.message)
            .count()
            .saturating_sub(1);
        state.prompts = shown
            .iter()
            .map(|d| crate::ui::game::Prompt {
                id: d.id,
                question: !d.message,
                modal: d.modal,
                waiting: if d.message { 0 } else { waiting },
                text: d.text.clone(),
            })
            .collect();
        if let Some(p) = self.resolution {
            use dereth_client_contract::resolution::ResolutionPromptKind as K;
            let text = match p.kind {
                K::OfferTest => "Test new screen size first, for 15 seconds?",
                K::Accept => "Accept this setting?",
                K::ApplyFailed => "Unable to change screen size!",
                K::RevertFailed => "Unable to restore the previous screen size!",
                K::Reset => "Resolution Reset",
            };
            state.prompts.push(crate::ui::game::Prompt {
                id: RESOLUTION_PROMPT,
                question: matches!(p.kind, K::OfferTest | K::Accept),
                modal: true,
                waiting: 0,
                text: text.to_owned(),
            });
        }
        // A refusal of a character sent, in the creation wizard's own words.
        state.chargen_refusal = cx.pregame().chargen_response.and_then(|code| {
            let token = dereth_chargen::CgVerification::from_code(code).error_string_id()?;
            Some(self.strings.text(
                cx.store(),
                crate::ui::creation::CREATION_STRING_TABLE,
                token,
            ))
        });
        if let Some(code) = self.abuse_code.take() {
            use dereth_ui_screens::panels::abuse;
            self.abuse_answer = abuse::token(code)
                .and_then(|t| self.strings.lookup(cx.store(), abuse::STRING_TABLE_ENUM, t));
        }
        state.abuse_answer.clone_from(&self.abuse_answer);
        state.key_bindings.clone_from(&self.keys);
        state.spell_tab_keys = spell_tab_keys(self.keys.as_ref());
        // A disconnection's notice, in the game's words: a token of the disconnect table, or the
        // sentence itself for the two notices that carry their own.
        if let Some(notice) = cx.pregame().error.clone() {
            let text = self
                .strings
                .lookup(cx.store(), DISCONNECT_TABLE, &notice)
                .unwrap_or(notice);
            state.failure = Some(text);
            state.disconnected = true;
        }
        {
            let view = cx.hud().view(cx.objects());
            state.game_time = view.game_date_time();
        }
        // The barber's special option, in the game's words.
        if let Some((token, _)) = self.ui.windows.world.barber.special_option() {
            if self.ui.windows.world.barber_caption.is_none() {
                self.ui.windows.world.barber_caption = self.strings.fill(
                    cx.store(),
                    dereth_ui_screens::panels::barber::CAPTION_STRING_TABLE.0,
                    token,
                    &[],
                );
            }
        } else {
            self.ui.windows.world.barber_caption = None;
        }
        #[allow(clippy::cast_precision_loss)]
        let screen_f = (screen.0 as f32, screen.1 as f32);
        let out = self
            .ui
            .frame(&mut self.list, screen_f, dt, &state, &mut self.frame_input);
        self.examine_open = self
            .ui
            .windows
            .is_open(crate::ui::panels::WindowId::Examine);
        self.frame_input.next_frame();
        self.lobby_look = (self.ui.screen == crate::ui::Screen::Lobby)
            .then(|| state.characters.get(self.ui.pregame.selected))
            .flatten()
            .and_then(|c| {
                self.looks
                    .get(&crate::looks::key(&state.host, &c.name))
                    .cloned()
            });
        self.backdrop(cx, now);
        let gameplay = self.in_gameplay();
        let changed = self.was_gameplay.is_some_and(|was| was != gameplay);
        self.was_gameplay = Some(gameplay);
        self.carry_out(cx, out);
        self.hand_on_requests(cx, now);
        changed
    }

    /// Keep the player's look while they stand in the world, so character select can show them.
    /// Once the character is in the world and its options have come, listen to every chat
    /// channel the game offers that it is not listening to yet: General, Trade, LFG, Roleplay,
    /// Society and the allegiance's, each as far as the server lets the character in. Each chat window's filters say
    /// what it shows of them; leaving a channel lasts until the character next comes into the
    /// world.
    fn join_channels<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        use dereth_client_contract::view::PlayerOption as O;
        if !self.in_gameplay() {
            self.channels_joined = false;
            return;
        }
        let world = &cx.objects().world;
        if self.channels_joined
            || dereth_client_runtime::hud::character_option(world, O::HearGeneralChat).is_none()
        {
            return;
        }
        for o in [
            O::HearGeneralChat,
            O::HearTradeChat,
            O::HearLFGChat,
            O::HearRoleplayChat,
            O::HearSocietyChat,
            O::HearAllegianceChat,
        ] {
            if dereth_client_runtime::hud::character_option(world, o) == Some(false) {
                self.outbox.emit(UiRequest::SetPlayerOption(o, true));
            }
        }
        self.channels_joined = true;
    }

    fn remember_look<S: Shell>(&mut self, cx: &mut Cx<'_, S>, state: &crate::ui::game::GameState) {
        if !state.in_world || state.name.is_empty() {
            return;
        }
        let Some(presence) = cx.model().player.and_then(|p| cx.objects().presence(p)) else {
            return;
        };
        let Some(setup) = presence.setup_id else {
            return;
        };
        let desc = dereth_client_runtime::movement::to_anim_objdesc(&presence.objdesc);
        let scale = presence.scale;
        let heritage = cx.hud().player_desc(cx.model()).map_or(0, |q| {
            q.inq_int(dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY)
        });
        let look = crate::looks::Look {
            setup,
            desc,
            heritage: u32::try_from(heritage).unwrap_or(0),
            scale,
            level: state.level,
            motion_table: presence.mtable_id.unwrap_or(dereth_primitives::DataId(0)),
        };
        if !self
            .looks
            .remember(&crate::looks::key(&state.host, &state.name), look)
        {
            self.errors
                .push("the characters' looks could not be saved".to_owned());
        }
    }

    /// Holtburg behind the screens before the world, the camera swinging slowly back and forth
    /// over the town; released while the player is logging in.
    fn backdrop<S: Shell>(&mut self, cx: &mut Cx<'_, S>, now: LocalTime) {
        use crate::ui::Screen;
        if matches!(self.ui.screen, Screen::Lobby | Screen::Creation) {
            // From -1 to 1.
            let t = crate::ui::wave(now.0, std::f64::consts::TAU / BACKDROP_PERIOD) * 2.0 - 1.0;
            cx.show_backdrop(BACKDROP_LANDBLOCK, BACKDROP_SWING * t, 0.0);
        }
    }

    /// The vitae and burden windows' explanations, in the game's own words, while each is open.
    fn info_words<S: Shell>(&mut self, cx: &mut Cx<'_, S>, state: &mut crate::ui::game::GameState) {
        use crate::ui::panels::{info, WindowId};
        let windows = &self.ui.windows;
        let (vitae, burden) = (
            windows.is_open(WindowId::Vitae),
            windows.is_open(WindowId::Burden),
        );
        if !vitae && !burden {
            return;
        }
        let store = Arc::clone(cx.store());
        let strings = &mut self.strings;
        let mut fill = |token: &str, values: &[(&str, &str)]| {
            strings.fill(&store, info::STRING_TABLE, token, values)
        };
        if vitae {
            state.vitae_words = state
                .vitae_display
                .and_then(|d| info::vitae_words(d, &mut fill));
        }
        if burden {
            state.burden_words = state
                .character_info
                .as_ref()
                .and_then(|c| info::burden_words(c, &mut fill));
        }
    }

    /// The examine window, a chest or corpse on the ground, salvage, books, the vendor, trade,
    /// spell details and the combat bar, as the game's notices and views say.
    fn examine_and_containers<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        state: &mut crate::ui::game::GameState,
        notices: UiNotices,
    ) {
        let mut open_container = None;
        {
            let view = cx.hud().view(cx.objects());
            if let Some((id, serial)) = view.examine_request() {
                if serial != self.examine_serial {
                    self.examine_serial = serial;
                    self.examining = Some(id);
                    state.examine_opened = true;
                }
            }
            state.examined = self
                .examining
                .and_then(|id| crate::state::examined(&view, id));
            state.examine_closed = std::mem::take(&mut self.examine_close);
            state.book_session = view.book_session();
            let book = view.open_book();
            if let Some(b) = &book {
                if notices.book_range_exits.contains(&b.book_id) {
                    self.book_closed = Some(b.opening);
                }
            }
            state.book = book.filter(|b| Some(b.opening) != self.book_closed);
            // The game's salvage session: the tool and what is gathered, while it is up.
            let salvage = view.salvage_list();
            state.salvage = salvage.tool.filter(|_| salvage.visible).map(|tool| {
                (
                    tool,
                    salvage
                        .items
                        .iter()
                        .map(|id| crate::state::item_of(&view, *id))
                        .collect(),
                )
            });
            for notice in notices.external_container {
                use dereth_client_contract::panels::external_container::ExternalContainerNotice as N;
                if let N::SetGroundObject(id) = notice {
                    self.ground = (id.0 != 0).then_some(id);
                    let container = view.slot_decoration(id).is_some_and(|d| d.is_container);
                    if self.ground.is_some() && container {
                        open_container = Some(id);
                    }
                }
            }
            if let Some(id) = self.ground {
                state.loot = crate::state::loot(&view, id);
            }
            let shop = view.shop();
            state.shop = shop.open.then_some(shop);
            let trade = view.trade();
            state.trade = trade.open.then_some(trade);
            // The pictures of what the vendor and trade windows list, composed as tiles are.
            let mut listed: Vec<ObjectId> = Vec::new();
            if let Some(shop) = &state.shop {
                listed.extend(
                    [&shop.stock, &shop.buy_list, &shop.sell_list]
                        .into_iter()
                        .flatten()
                        .map(|r| r.item),
                );
            }
            if let Some(trade) = &state.trade {
                listed.extend(
                    trade
                        .self_rows
                        .iter()
                        .chain(&trade.partner_rows)
                        .map(|r| r.item),
                );
            }
            listed.sort_unstable();
            listed.dedup();
            state.looks = listed
                .into_iter()
                .map(|id| crate::state::item_of(&view, id))
                .collect();
            state.spell_detail = self
                .spell_detail
                .and_then(|id| view.spell_examine(id).map(|d| (id, d)));
            state.spell_identify = self
                .spell_identify
                .and_then(|id| view.spell_examine(id).map(|d| (id, d)));
            state.combat_bar = view.combat_bar();
        }
        if let Some(id) = open_container {
            self.outbox.emit(UiRequest::NewParentContainer(id));
        }
    }

    /// The ping: one every ten seconds in the world, its round trip timed from the send to the
    /// frame its answer is counted.
    fn ping<S: Shell>(&mut self, cx: &mut Cx<'_, S>, now: LocalTime) {
        if !self.in_gameplay() {
            return;
        }
        let answers = cx.hud().view(cx.objects()).ping_returns();
        if let Some((at, before)) = self.ping_sent {
            if answers > before {
                self.ping_ms = Some((now.0 - at) * 1000.0);
                self.ping_sent = None;
            }
        }
        // One ping every ten seconds, answered or not.
        if self.ping_last.is_none_or(|at| now.0 - at >= PING_SECONDS) {
            self.outbox.emit(UiRequest::RequestPing);
            self.ping_sent = Some((now.0, answers));
            self.ping_last = Some(now.0);
        }
    }

    /// Every request waiting in the outbox handed to the runtime, as the other interfaces hand
    /// theirs: each to the runtime's owners at once, with the talk-focus notices it raises answered
    /// on the way, and what no owner took queued for the interaction step. A request one raises
    /// is handed on in the same pass.
    pub fn hand_on_requests<S: Shell>(&mut self, cx: &mut Cx<'_, S>, now: LocalTime) {
        let mut unowned = Vec::new();
        loop {
            let pending = self.outbox.take();
            if pending.is_empty() {
                break;
            }
            for request in pending {
                // A drop on the world picks what is under the pointer where the item was let go.
                if matches!(
                    request,
                    UiRequest::DragDrop {
                        target: dereth_client_contract::view::DropTarget::World,
                        ..
                    }
                ) {
                    let (x, y) = self.frame_input.mouse;
                    cx.note_pointer(
                        dereth_primitives::num::to_i32(x.round()),
                        dereth_primitives::num::to_i32(y.round()),
                    );
                }
                // A use the cursor was waiting for is done once its target is given: the cursor
                // goes, after the target mode has been read, as the game's own shortcut keys end it.
                let ends_target_mode = matches!(request, UiRequest::ExecuteTargetItem(_));
                let mut answer = |focus, notice| self.talk_focus_notice(focus, notice);
                unowned.extend(cx.run_request(request, now, &mut answer));
                if ends_target_mode {
                    unowned.extend(cx.run_request(
                        UiRequest::SetTargetMode(dereth_client_contract::view::TargetMode::None),
                        now,
                        &mut answer,
                    ));
                }
            }
            cx.deliver_selection_notices(now);
        }
        if !unowned.is_empty() {
            cx.queue(Vec::new(), unowned);
        }
    }

    /// What the player asked for this frame, handed to the runtime.
    fn carry_out<S: Shell>(&mut self, cx: &mut Cx<'_, S>, out: crate::ui::Outcome) {
        for (id, yes) in out.answers {
            if id == RESOLUTION_PROMPT {
                use dereth_client_contract::resolution::{ResolutionAction, ResolutionPromptKind};
                if let Some(p) = self.resolution.take() {
                    let question = matches!(
                        p.kind,
                        ResolutionPromptKind::OfferTest | ResolutionPromptKind::Accept
                    );
                    self.outbox.emit(UiRequest::Resolution(if question {
                        ResolutionAction::Answer {
                            token: p.token,
                            yes,
                        }
                    } else {
                        ResolutionAction::Dismiss { token: p.token }
                    }));
                }
                continue;
            }
            self.dialogs.answer(id, yes);
        }
        if out.examine_spell.is_some() {
            self.spell_detail = out.examine_spell;
        }
        self.spell_identify = out.identify_spell;
        {
            let view = cx.hud().view(cx.objects());
            if out.book_closed {
                self.book_closed = view.open_book().map(|b| b.opening);
            }
        }
        if let Some((action, target, token, who)) = out.allegiance_question.clone() {
            let prompt = self
                .strings
                .fill(
                    cx.store(),
                    PANEL_STRING_TABLE,
                    token,
                    &[("PLAYER", who.as_str())],
                )
                .unwrap_or(who);
            self.outbox.emit(UiRequest::AllegianceConfirmation {
                action,
                target,
                prompt,
            });
        }
        if out
            .requests
            .iter()
            .any(|r| matches!(r, UiRequest::CloseExternalContainer(_)))
        {
            self.ground = None;
        }
        for name in &out.select {
            let view = cx.hud().view(cx.objects());
            let found = view
                .radar_objects()
                .iter()
                .filter(|r| {
                    view.name(r.id)
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
                .min_by(|a, b| {
                    let d = |r: &&dereth_client_contract::view::RadarEntry| {
                        r.player_space.0 * r.player_space.0 + r.player_space.1 * r.player_space.1
                    };
                    d(a).total_cmp(&d(b))
                })
                .map(|r| r.id);
            match found {
                Some(id) => self.outbox.emit(UiRequest::Select(id)),
                None => self
                    .errors
                    .push(format!("nothing called {name:?} on the radar")),
            }
        }
        if out
            .requests
            .iter()
            .any(|r| matches!(r, UiRequest::AbuseLog { .. }))
        {
            self.abuse_answer = None;
        }
        for request in out.requests {
            self.outbox.emit(request);
        }
        for line in out.chat {
            self.outbox.emit(UiRequest::ChatLine {
                text: line,
                window: dereth_client_contract::chat::interface::window::MAIN,
            });
        }
        for id in out.actions {
            cx.inject_action(dereth_client_runtime::actions::Action::begin(
                dereth_client_runtime::actions::ActionId(id),
            ));
        }
        self.key_requests.extend(out.key_requests);
        if !out.settings.is_empty() {
            self.apply_settings(&out.settings);
        }
        if out.log_off {
            cx.log_off_character();
        }
        if !out.character_actions.is_empty() {
            cx.run_character_actions(out.character_actions);
        }
        if !out.chargen_actions.is_empty() {
            cx.run_chargen_actions(out.chargen_actions);
        }
        if out.quit {
            cx.quit_game();
        }
    }

    /// The interface's own settings, changed in System Configuration: applied live and saved.
    fn apply_settings(&mut self, settings: &[(String, String)]) {
        let mut options = self.ui.options.clone();
        for (name, value) in settings {
            match name.as_str() {
                "scale" => options.scale = value.parse::<f32>().ok().map(|s| s.clamp(0.5, 3.0)),
                "movement" => {
                    if let Some(m) = crate::options::parse_movement(value) {
                        options.orbit.movement = m;
                    }
                }
                "reverse-x" => options.orbit.reverse_x = value == "true",
                "reverse-y" => options.orbit.reverse_y = value == "true",
                "sidestep" => options.orbit.sidestep = value == "true",
                "minimap-rotates" => options.minimap_rotates = value == "true",
                "mouse-turn" | "key-turn" | "tilt-min" | "tilt-max" | "camera-height" => {
                    if let Ok(v) = value.parse::<f32>() {
                        let o = &mut options.orbit;
                        *match name.as_str() {
                            "mouse-turn" => &mut o.mouse_turn,
                            "key-turn" => &mut o.key_turn,
                            "tilt-min" => &mut o.pitch_min,
                            "tilt-max" => &mut o.pitch_max,
                            _ => &mut o.height,
                        } = v;
                        options.orbit = options.orbit.held_to_ranges();
                    }
                }
                other => {
                    if let Some(slot) = crate::options::TabNames::slot_of(other) {
                        options.chat_tabs.set(slot, value);
                    } else if let Some(slot) = crate::options::popped_slot(other) {
                        options.chat_popped[slot] = crate::options::parse_point(value);
                    } else if let Some(slot) = crate::options::size_slot(other) {
                        options.chat_sizes[slot] = crate::options::parse_point(value);
                    } else if let Ok(v) = value.parse::<f32>() {
                        options.chat_opacity.set_by_key(other, v);
                    }
                }
            }
        }
        if options == self.ui.options {
            return;
        }
        self.ui.options = options.clone();
        if let Some(path) = &self.settings_path {
            if let Err(e) = options.save(path) {
                self.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    /// Turn this frame's draw list into the overlay, uploading what it newly needs. Outside the
    /// frame bracket.
    pub fn compose<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        self.ground_ring(cx);
        let art = Arc::clone(&self.ui.art);
        let portal = std::mem::take(&mut self.portal);
        // The previews' bodies move by the time that passed, so they play at one pace at any
        // frame rate.
        let dt = preview_dt(&mut self.preview_clock, cx.now());
        let spaces: Vec<_> = [self.creation_preview(cx, dt), self.doll_preview(cx, dt)]
            .into_iter()
            .flatten()
            .collect();
        self.overlay
            .compose(cx.present_mut(), &art, &self.list, portal, &spaces);
    }

    /// The ring under the selection, on the game screen: its picture uploaded once, and the
    /// marker asked for each frame (none with nothing selected, or off the game screen).
    fn ground_ring<S: Shell>(&mut self, cx: &mut Cx<'_, S>) {
        let target = self.ring_target.filter(|_| self.in_gameplay());
        if target.is_some() && !self.ring_uploaded {
            self.ring_uploaded = true;
            let inner = crate::ring::inner_picture(&self.ui.art);
            self.ring_inner = inner.is_some();
            let pictures = std::iter::once((
                crate::ring::RING_TEXTURE,
                crate::ring::picture(&self.ui.art),
            ))
            .chain(inner.map(|p| (crate::ring::RING_INNER_TEXTURE, p)));
            for (texture, picture) in pictures {
                if let Err(e) = cx.present_mut().overlay_upload(texture, &picture) {
                    self.ring_uploaded = false;
                    self.errors.push(format!("the selection ring: {e}"));
                }
            }
        }
        cx.present_mut().set_ground_markers(&crate::ring::markers(
            target,
            self.ring_time,
            self.ring_inner,
        ));
    }

    /// The character being made, in the creation preview space where the creation screen shows
    /// it: dressed from the creation model as the other interfaces dress it, turning slowly.
    fn creation_preview<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        dt: f64,
    ) -> Option<(
        dereth_client_contract::overlay::PreviewSpace,
        crate::draw::Rect,
        usize,
    )> {
        use dereth_client_contract::overlay::{PreviewLight, PreviewSpace};
        let id = PreviewSpace::CharGen;
        let Some((rect, creation)) = self
            .ui
            .pregame
            .creation_preview()
            .filter(|_| self.ui.screen == crate::ui::Screen::Creation)
        else {
            if self.creation_built.take().is_some() {
                cx.present_mut().preview_remove_all_objects(id);
            }
            return None;
        };
        let tables = creation.tables();
        let state = &creation.state;
        let setup = state.get_setup_id(&tables.chargen);
        let environment = tables
            .chargen
            .heritage_groups
            .get(&state.heritage_group)
            .map_or(0, |h| h.environment_setup.0);
        let heritage = state.heritage_group;
        // The heritage's standing pose, still, as the other interfaces show the model while it
        // is not being turned.
        let rest = {
            let mut view = dereth_ui_screens::screens::chargen::Cg3dView::default();
            view.initialize(state);
            view.rest_animation_enum
        };
        let heading = creation.heading;
        let store = Arc::clone(cx.store());
        let assets = Arc::clone(cx.anim_assets());
        let palettes = &mut self.creation_palettes;
        // Unclothed until the clothes are come to.
        let shown = undressed(state, creation.dressed());
        let (objdesc, _) = dereth_scene::preview::chargen_objdesc(
            &tables.chargen,
            &shown,
            &tables.clothing,
            setup,
            &mut |pal| palettes.palettes(&store, pal),
        );
        // The model stands before the world rather than in its heritage's room.
        let _ = environment;
        // The sex's own motion table: a change of sex changes the body's motions with it.
        let motions = state
            .sex(&tables.chargen)
            .map_or(dereth_primitives::DataId(0), |sex| sex.motion_table);
        let built = (setup, objdesc, motions.0);
        let present = cx.present_mut();
        present.preview_ensure(id, &assets);
        if self.creation_built.as_ref() != Some(&built) {
            present.preview_remove_all_objects(id);
            let dressed = present.preview_add_object_dressed(id, &store, built.0, Some(&built.1));
            if !matches!(dressed, Ok(Some(_))) {
                self.errors.push(format!(
                    "the creation model {:08X} did not load",
                    built.0 .0
                ));
            }
            // The body's own idle, from its sex's motion table; the still standing pose where
            // the table has none.
            let idle = idle_cycle(&*assets, motions);
            present.preview_clear_sequence_anims(id, 0);
            if idle.is_empty() {
                if let Some(a) = dereth_client_runtime::assets::enum_did(
                    &*store,
                    dereth_scene::preview::UIASSET_GROUP,
                    rest,
                ) {
                    present.preview_set_sequence_animation(id, 0, a, true, 0, 0.0);
                }
            } else {
                for (n, anim) in idle.iter().enumerate() {
                    present.preview_set_sequence_animation(
                        id,
                        0,
                        anim.anim_id,
                        n == 0,
                        anim.low_frame,
                        anim.framerate,
                    );
                }
            }
            self.creation_built = Some(built);
        }
        // While the face is chosen, the camera comes in on the face and shoulders.
        let close = creation.close_up();
        let camera = dereth_presentation::creation::camera(
            heritage,
            close,
            dereth_presentation::DisplayVariant::Modern,
        );
        let distance = if close {
            CREATION_FACE_DISTANCE
        } else {
            CREATION_DISTANCE
        };
        // A lens of its own, framing a whole figure from the camera's distance whatever the
        // world camera's field of view.
        present.preview_set_fov(id, CREATION_FOV);
        // Stood further back than the shared camera, so the whole figure stands in its frame
        // with the world around it.
        present.preview_set_camera_position(
            id,
            dereth_primitives::Vec3::new(camera[0], camera[1] * distance, camera[2]),
        );
        present
            .preview_set_camera_direction_degrees(id, dereth_primitives::Vec3::new(-5.0, 0.0, 0.0));
        present.preview_set_heading(id, 0, heading);
        present.preview_set_light(
            id,
            PreviewLight::Directional,
            2.0,
            dereth_primitives::Vec3::new(0.3, 1.9, 0.65),
        );
        present.preview_use_time(id, dt);
        Some((id, rect, 0))
    }

    /// The player in the paper doll's preview space where the Character window shows it, over the
    /// window and under anything drawn after it, wearing what they wear, as the other interfaces
    /// dress their dolls.
    fn doll_preview<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        dt: f64,
    ) -> Option<(
        dereth_client_contract::overlay::PreviewSpace,
        crate::draw::Rect,
        usize,
    )> {
        use dereth_client_contract::overlay::{PreviewLight, PreviewSpace};
        let id = PreviewSpace::PaperDoll;
        use dereth_ui_screens::panels::inventory::{paper_doll as pd, PAPER_DOLL_ANIMATION_ENUM};
        let in_world = self.ui.screen == crate::ui::Screen::Game;
        let window = self.ui.windows.doll.take();
        let lobby = self.ui.pregame.doll.take();
        // In the world, the player as they stand; on character select, the selected character
        // as they stood when they were last in the world.
        let (placed, look, heritage, scale, motion_table) = if in_world {
            let presence = cx.model().player.and_then(|p| cx.objects().presence(p));
            let look = presence.and_then(|p| {
                Some((
                    p.setup_id?,
                    dereth_client_runtime::movement::to_anim_objdesc(&p.objdesc),
                ))
            });
            let scale = presence.map_or(1.0, |p| p.scale);
            let heritage = cx.hud().player_desc(cx.model()).map_or(0, |q| {
                q.inq_int(dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY)
            });
            (
                window,
                look,
                u32::try_from(heritage).unwrap_or(0),
                scale,
                dereth_primitives::DataId(0),
            )
        } else if let (Some(placed), Some(l)) = (lobby, self.lobby_look.as_ref()) {
            (
                Some(placed),
                Some((l.setup, l.desc.clone())),
                l.heritage,
                l.scale,
                l.motion_table,
            )
        } else {
            (None, None, 0, 1.0, dereth_primitives::DataId(0))
        };
        let (Some((rect, at)), Some(look)) = (placed, look) else {
            if self.doll_built.take().is_some() {
                cx.present_mut().preview_remove_all_objects(id);
            }
            return None;
        };
        // The doll as the other interfaces pose it: the heritage's camera and pose, the paper
        // doll's lens and light.
        let race = pd::for_race(heritage);
        // The heritage's camera, centred on the figure and stood further back, so the whole doll
        // stands in the middle of its column with room around it.
        let camera = race.map_or(pd::CAMERA_POSITION, |(c, _)| c);
        let raise = if in_world {
            WINDOW_DOLL_RAISE
        } else {
            DOLL_RAISE
        };
        let camera = (0.0, camera.1 * DOLL_DISTANCE, camera.2 + raise);
        let animation = race
            .and_then(|(_, a)| a)
            .unwrap_or(PAPER_DOLL_ANIMATION_ENUM);
        let store = Arc::clone(cx.store());
        let assets = Arc::clone(cx.anim_assets());
        let present = cx.present_mut();
        if present.preview_ensure(id, &assets) {
            present.preview_use_sharp_mode(id);
            present.preview_set_light(
                id,
                PreviewLight::Directional,
                pd::LIGHT_INTENSITY,
                dereth_primitives::Vec3::new(
                    pd::LIGHT_DIRECTION.0,
                    pd::LIGHT_DIRECTION.1,
                    pd::LIGHT_DIRECTION.2,
                ),
            );
        }
        // The paper doll's own 45-degree lens, whatever the world camera's zoom.
        present.preview_set_fov(id, std::f32::consts::FRAC_PI_4);
        if self.doll_built.as_ref() != Some(&look) {
            present.preview_remove_all_objects(id);
            if !matches!(
                present.preview_add_object_dressed(id, &store, look.0, Some(&look.1)),
                Ok(Some(_))
            ) {
                self.errors
                    .push(format!("the paper doll {:08X} did not load", look.0 .0));
            }
            present.preview_set_heading(id, 0, pd::HEADING_DEGREES);
            // On character select the body plays its own idle, from its setup's motion table;
            // the Character window, and a body without one, holds the paper doll's pose.
            let idle = if in_world {
                Vec::new()
            } else {
                // The motion table the character had in the world, else its setup's own.
                let motions = Some(motion_table).filter(|m| m.0 != 0).or_else(|| {
                    dereth_animation::data::AnimAssets::setup(&*assets, look.0)
                        .and_then(|s| s.default_motion_table)
                });
                motions.map_or_else(Vec::new, |m| idle_cycle(&*assets, m))
            };
            if idle.is_empty() {
                if let Some(a) = dereth_client_runtime::assets::enum_did(
                    &*store,
                    dereth_scene::preview::UIASSET_GROUP,
                    animation,
                ) {
                    present.preview_set_sequence_animation(
                        id,
                        0,
                        a,
                        true,
                        pd::LOW_FRAME,
                        pd::FRAMERATE,
                    );
                }
            } else {
                present.preview_clear_sequence_anims(id, 0);
                for (n, anim) in idle.iter().enumerate() {
                    present.preview_set_sequence_animation(
                        id,
                        0,
                        anim.anim_id,
                        n == 0,
                        anim.low_frame,
                        anim.framerate,
                    );
                }
            }
            self.doll_built = Some(look);
        }
        present.preview_set_scale(id, 0, scale);
        present.preview_set_camera_position(
            id,
            dereth_primitives::Vec3::new(camera.0, camera.1, camera.2),
        );
        present.preview_set_camera_direction(
            id,
            dereth_primitives::Vec3::new(
                pd::CAMERA_TARGET.0,
                pd::CAMERA_TARGET.1,
                pd::CAMERA_TARGET.2,
            ),
        );
        present.preview_use_time(id, dt);
        Some((id, rect, at))
    }

    /// Draw the overlay [`Self::compose`] built, inside the frame bracket.
    ///
    /// # Errors
    /// Whatever the device answers.
    pub fn draw<P: dereth_client_runtime::present::Presentation + ?Sized>(
        &self,
        present: &mut P,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        self.overlay.draw(present)
    }

    /// What the overlay has drawn so far.
    #[must_use]
    pub fn overlay_stats(&self) -> crate::draw::OverlayStats {
        self.overlay.stats
    }

    /// This frame's overlay.
    #[must_use]
    pub fn overlay_items(&self) -> &[dereth_client_contract::overlay::OverlayItem] {
        self.overlay.items()
    }

    /// The interface is put away for another: every press it holds is let go, and its boxes come
    /// down unanswered.
    pub fn suspend(&mut self) {
        self.frame_input = InputFrame::default();
        self.world_press = [false; 2];
        self.ui.drag = None;
        self.dialogs.generation += 1;
    }

    /// The interface is shown again: it comes up on the screen the game is at.
    pub fn shown_again(&mut self) {
        self.last_time = None;
        self.was_gameplay = None;
        // Another interface may have dressed the shared preview spaces with its own models.
        self.creation_built = None;
        self.doll_built = None;
    }

    /// Chat lines the interface missed while another was shown, or the history a switch hands
    /// over.
    pub fn chat_history(
        &mut self,
        lines: Vec<dereth_client_contract::chat::interface::ChatMessage>,
    ) {
        self.ui.hud.replace_log(
            lines
                .into_iter()
                .map(crate::ui::game::ChatLine::from_message)
                .collect(),
        );
    }
}

/// The chat lines the game screen shows this frame. A line that arrives while the player is
/// entering the world, before the game screen is up (the world's welcome, sent as they enter),
/// waits in `early` for it; one that arrives anywhere else before the game screen is let go.
fn hold_early_chat(
    early: &mut Vec<dereth_client_contract::chat::interface::ChatMessage>,
    in_game: bool,
    entering: bool,
    mut chat: Vec<dereth_client_contract::chat::interface::ChatMessage>,
) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
    if in_game {
        let mut lines = std::mem::take(early);
        lines.append(&mut chat);
        lines
    } else {
        if entering {
            early.append(&mut chat);
        } else {
            early.clear();
        }
        Vec::new()
    }
}

/// The animations of a body's idle: `motion_table`'s default stance and that stance's default
/// motion, as a body standing in the world plays them. Empty when there is no such table or
/// cycle.
#[must_use]
pub fn idle_cycle(
    assets: &dyn dereth_animation::data::AnimAssets,
    motion_table: dereth_primitives::DataId,
) -> Vec<dereth_primitives::records::AnimData> {
    let Some(table) = assets.motion_table(motion_table) else {
        return Vec::new();
    };
    let table = dereth_animation::table::MotionTable::new(table);
    let style = table.default_style();
    table
        .style_default(style)
        .and_then(|motion| table.cycle(style, motion))
        .map(|cycle| cycle.anims.clone())
        .unwrap_or_default()
}

/// `state` as the creation model shows it: with its clothes while `dressed`, and with none of the
/// four garments otherwise.
fn undressed(state: &dereth_chargen::CharGenState, dressed: bool) -> dereth_chargen::CharGenState {
    let mut shown = state.clone();
    if !dressed {
        shown.headgear_style = -1;
        shown.shirt_style = -1;
        shown.trousers_style = -1;
        shown.footwear_style = -1;
    }
    shown
}

/// The seconds since the previews last moved, read from the runtime's clock `now` and
/// remembered in `clock`: none on the first frame, and at most a quarter of a second, so a stall
/// does not throw a body through its motions.
fn preview_dt(clock: &mut Option<f64>, now: f64) -> f64 {
    let dt = clock.map_or(0.0, |then| (now - then).clamp(0.0, 0.25));
    *clock = Some(now);
    dt
}

/// The keys that turn the spell bar to the previous and the next tab, as the key map names them:
/// each action's first key, or nothing while the map is not known or the action is unbound.
fn spell_tab_keys(keys: Option<&crate::ui::game::KeyBindingsView>) -> (String, String) {
    use dereth_client_contract::actions::mapped::{COMBAT_NEXT_SPELL_TAB, COMBAT_PREV_SPELL_TAB};
    let first = |action: dereth_client_contract::actions::ActionId| {
        keys.and_then(|k| k.rows.iter().find(|r| r.action == action.0))
            .and_then(|r| r.keys.first().cloned())
            .unwrap_or_default()
    };
    (first(COMBAT_PREV_SPELL_TAB), first(COMBAT_NEXT_SPELL_TAB))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use dereth_client_model::chat::{TalkFocus, TalkFocusNotice};

    #[test]
    fn the_creation_model_wears_nothing_until_dressed_and_what_is_made_keeps_its_clothes() {
        let mut made = dereth_chargen::CharGenState::default();
        made.headgear_style = 1;
        made.shirt_style = 2;
        made.trousers_style = 3;
        made.footwear_style = 0;
        let bare = undressed(&made, false);
        assert_eq!(
            [
                bare.headgear_style,
                bare.shirt_style,
                bare.trousers_style,
                bare.footwear_style
            ],
            [-1; 4]
        );
        let worn = undressed(&made, true);
        assert_eq!(
            [
                worn.headgear_style,
                worn.shirt_style,
                worn.trousers_style,
                worn.footwear_style
            ],
            [1, 2, 3, 0]
        );
        assert_eq!(made.shirt_style, 2, "the choice itself is kept");
    }

    #[test]
    fn a_preview_moves_by_the_time_that_passed_whatever_the_frame_rate() {
        let moved = |fps: u32| {
            let mut clock = None;
            (0..=fps)
                .map(|n| preview_dt(&mut clock, 10.0 + f64::from(n) / f64::from(fps)))
                .sum::<f64>()
        };
        assert!((moved(60) - 1.0).abs() < 1e-9);
        assert!(
            (moved(240) - 1.0).abs() < 1e-9,
            "four times the frames, the same second"
        );
    }

    fn line(body: &str) -> dereth_client_contract::chat::interface::ChatMessage {
        dereth_client_contract::chat::interface::ChatMessage {
            body: body.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_spell_bar_names_the_keys_of_its_tab_actions() {
        use crate::ui::game::{KeyBindingsView, KeyRow};
        let row = |action: u32, key: &str| KeyRow {
            map: 0,
            action,
            group: 0,
            caption: String::new(),
            keys: vec![key.into()],
            changed: false,
        };
        let view = KeyBindingsView {
            rows: vec![row(0x1000_0064, "Page Up"), row(0x1000_0063, "Insert")],
            capture: None,
            refused: None,
        };
        assert_eq!(
            spell_tab_keys(Some(&view)),
            ("Insert".to_owned(), "Page Up".to_owned())
        );
        assert_eq!(spell_tab_keys(None), (String::new(), String::new()));
    }

    #[test]
    fn the_welcome_sent_while_entering_the_world_is_shown_when_the_game_screen_comes_up() {
        let mut early = Vec::new();
        assert!(
            hold_early_chat(&mut early, false, true, vec![line("Welcome to Dereth")]).is_empty()
        );
        let shown = hold_early_chat(&mut early, true, true, vec![line("You have entered")]);
        let bodies: Vec<&str> = shown.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["Welcome to Dereth", "You have entered"]);
        // A line from before entering, at character select, is not kept for the next session.
        assert!(hold_early_chat(&mut early, false, false, vec![line("stale")]).is_empty());
        assert!(hold_early_chat(&mut early, true, true, Vec::new()).is_empty());
    }

    #[test]
    fn the_destination_switched_off_under_the_player_falls_back_and_another_does_not() {
        let mut front =
            HorizonFrontEnd::new(Arc::new(Art::empty()), HorizonOptions::default(), None);
        let off = |focus| TalkFocusNotice {
            focus,
            enabled: false,
            is_olthoi: false,
        };
        assert!(!front.talk_focus_notice(TalkFocus::All, off(TalkFocus::Patron)));
        assert!(front.talk_focus_notice(TalkFocus::Monarch, off(TalkFocus::Monarch)));
        assert!(
            !front.talk_focus_notice(TalkFocus::Monarch, off(TalkFocus::Monarch)),
            "a destination already off is no edge"
        );
    }
    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn a_creation_model_idles_in_its_motion_table_s_standing_cycle() {
        let store =
            std::sync::Arc::new(dereth_dat::testing::open_store().expect("the retail dats"));
        let assets = dereth_world_data::anim_assets::DatAnimAssets::new(store);
        // The human motion table's standing cycle, and nothing for a table that is not there.
        let idle = super::idle_cycle(&assets, dereth_primitives::DataId(0x0900_0001));
        assert!(!idle.is_empty());
        assert!(idle.iter().all(|a| a.anim_id.0 != 0 && a.framerate > 0.0));
        assert!(super::idle_cycle(&assets, dereth_primitives::DataId(0)).is_empty());
    }
}
