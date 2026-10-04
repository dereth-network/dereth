//! The classic front end: everything the shared runtime's frame hands the classic interface, and
//! the state it keeps across frames. It reaches the game through the front-end context alone: it
//! reads the model, and asks for everything it wants done.
use crate::dialogs::{ClassicDialogs, Presenter};
use crate::int::u32_from;
use crate::{desktop::Desktop, panels::*, renderer::Canvas, widgets::Input, Screen};
use dereth_client_contract::requests::Outbox;
use dereth_client_runtime::{hud::HudPanels, present::Presentation, shell::Shell};
use dereth_primitives::num::to_i32_f64;

/// The front-end context the shared runtime hands the classic interface at each step.
pub type Cx<'a, S> = dereth_client_runtime::ui_context::UiContext<'a, S>;

/// The lines the HUD model offers the classic interface's receivers: the transient line the
/// game view shows, the game window's status lines and the abuse-log answer.
#[derive(Debug, Default)]
pub struct ClassicHudPanels {
    pub abuse: Option<u32>,
    /// Lines of the default text type, as they reach the chat. A game's status line is one.
    pub default_lines: Vec<String>,
    pub transient: Vec<(String, dereth_client_contract::feedback::Feedback)>,
}
impl HudPanels for ClassicHudPanels {
    fn spew_offer(
        &mut self,
        ty: u8,
        body: &str,
        feedback: dereth_client_contract::feedback::Feedback,
    ) -> bool {
        if ty == 0 {
            self.default_lines.push(body.into());
        }
        if ty != 0x1a || feedback.kind == dereth_client_contract::feedback::FeedbackKind::Ordinary {
            return false;
        }
        self.transient.push((body.into(), feedback));
        true
    }
    fn spew_trace(&self) -> (bool, usize, u64) {
        (false, 0, 0)
    }
    fn spew_clear_pending(&mut self) {
        self.transient.clear();
    }
    fn abuse_response(&mut self, code: u32) {
        self.abuse = Some(code);
    }
}

/// A HUD slot with the classic receivers beside the model: the slot of any shell that runs the
/// classic interface.
pub trait ClassicHudSlot: dereth_client_runtime::hud::HudSlot {
    /// The classic receivers.
    fn classic_panels(&mut self) -> &mut ClassicHudPanels;
}

/// A shell that can run the classic interface.
pub trait Host: Shell<Hud: ClassicHudSlot> {}
impl<T: Shell<Hud: ClassicHudSlot>> Host for T {}

/// The host's clipboard, as the classic text fields reach it.
pub trait Clipboard {
    /// The text on offer, if any.
    fn get(&mut self) -> Option<String>;
    /// Put `text` on offer.
    fn set(&mut self, text: &str);
}

pub struct ClassicUi {
    pub quit_requested: bool,
    game_status_area: bool,
    /// Which kinds of message the chat window shows: the main chat window's filter, which the
    /// Chat Options page sets.
    chat_filter: u64,
    examine_seen: Option<(ObjectId, u64)>,
    examine_open: Option<ObjectId>,
    stretch_saved: Option<u32>,
    target_commands: Vec<crate::Command>,
    leave_target_after_click: bool,
    /// Where and when the last left press in the world landed, to tell a double click.
    world_press: Option<(i32, i32, std::time::Instant)>,
    /// Where the left button went down over the world while it is held, to start a drag.
    world_held: Option<(i32, i32)>,
    /// Movement held by the right button while right-click mouse look is off.
    steering: crate::steering::Steering,
    /// The spells the character knows, and how many spell-learned messages had arrived, as of
    /// the last frame: a new one opens the spellbook on the spell.
    known_spells: Option<std::collections::BTreeSet<u32>>,
    spells_added_seen: u64,
    /// The last book opening the server made, so each new one opens the book window once.
    book_opening: Option<u64>,
    house_profile_seen: u64,
    /// Where the portal-space swirl is drawn this frame, while the player travels.
    portal_rect: Option<crate::widgets::Rect>,
    minigame_lines_seen: u64,
    game_info: String,
    overlay: crate::world_overlay::OverlayState,
    flash_notices: Vec<Option<ObjectId>>,
    game_visible: bool,
    resolution_prompt: Option<dereth_client_contract::resolution::ResolutionPrompt>,
    resolution_completion: u64,
    cursor_commands: Vec<crate::Command>,
    /// The pointer the window system shows this frame; `None` hides it (the mouse is looking
    /// around, or a dragged icon is drawn in its place).
    system_pointer: Option<crate::cursor::SystemPointer>,
    selection_queries: hud::SelectionQueries,
    options_seen: Option<u32>,
    dialogs: ClassicDialogs,
    panel_events: Vec<(String, ControlEvent)>,
    preview_click: Option<(u64, i32, i32, std::time::Instant)>,
    settings_host: Option<crate::settings_host::SettingsHost>,
    /// The classic key map as the host last handed it over.
    classic_keys: crate::keystore::ClassicKeys,
    ui_actions: Vec<String>,
    /// A tell to begin in the chat entry on the next refresh.
    pending_tell: Option<String>,
    pending_chat_entry: Option<dereth_client_contract::chat::entry::EntryUpdate>,
    pub initial_panel: String,
    bindings: Option<crate::keybindings::KeyBindings>,
    previews: crate::previews::Previews,
    /// The character sheet's augmentation and luminance section, over the world's string tables.
    augmentation_sheet: hud::AugmentationSheet,
    actions: Vec<dereth_client_contract::actions::Action>,
    pending_split: Option<(u64, u32, u32)>,
    pending_social: Option<(u64, u8)>,
    social_pick: bool,
    callbacks: Vec<(u64, ControlEvent)>,
    panel_actions: Vec<PanelAction>,
    close_panels: Vec<String>,
    pending_size: Option<(u32, u32)>,
    /// The requests no step of the classic interface took, for the runtime's own owners.
    pub outbox: Outbox,
    /// The last real pointer sample, which the inverted mouse look reflects against.
    last_pointer: Option<(f64, f64)>,
    /// Where the pointer is, in window pixels.
    pointer: (i32, i32),
    shift: bool,
    ctrl: bool,
    alt: bool,
    /// The keys held down, so a repeated press is told from a new one.
    held_keys: std::collections::BTreeSet<u16>,
    /// Wheel movement not yet a whole step.
    wheel: f64,
    pub desktop: Desktop,
    pub keyboard: KeyboardState,
    pub settings: ClassicSettings,
    pub classic: ClassicState,
    pub inputs: Vec<Input>,
    pub art: std::sync::Arc<crate::art::ClassicArt>,
    pub paths: crate::art::ClassicPaths,
    canvas: Option<Canvas>,
    screen: Screen,
    last_in_world: bool,
    /// When Configure Keyboard asked to leave the world for the key page, until the world is
    /// left.
    keyboard_on_leaving: Option<f64>,
    pub errors: Vec<String>,
}

/// How long after Configure Keyboard a leaving of the world still opens the key page, in
/// seconds: longer than a logging-out character takes to leave.
const KEYBOARD_ON_LEAVING_WAIT: f64 = 60.0;
impl std::fmt::Debug for ClassicUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassicUi")
            .field("quit_requested", &self.quit_requested)
            .field("initial_panel", &self.initial_panel)
            .field("last_in_world", &self.last_in_world)
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}
impl ClassicUi {
    pub fn new(
        art: std::sync::Arc<crate::art::ClassicArt>,
        paths: crate::art::ClassicPaths,
        factory: fn(&str) -> Option<Box<dyn Panel>>,
        size: (u32, u32),
    ) -> Self {
        let previews = crate::previews::Previews::default();
        Self {
            quit_requested: false,
            game_status_area: false,
            chat_filter: dereth_client_contract::options::sheet::MAIN_WINDOW_DEFAULT_FILTER,
            examine_seen: None,
            examine_open: None,
            stretch_saved: None,
            target_commands: Vec::new(),
            leave_target_after_click: false,
            world_press: None,
            world_held: None,
            steering: Default::default(),
            known_spells: None,
            spells_added_seen: 0,
            book_opening: None,
            house_profile_seen: 0,
            portal_rect: None,
            minigame_lines_seen: 0,
            game_info: String::new(),
            overlay: Default::default(),
            flash_notices: vec![],
            game_visible: false,
            resolution_prompt: None,
            resolution_completion: 0,
            cursor_commands: vec![],
            system_pointer: None,
            selection_queries: Default::default(),
            options_seen: None,
            dialogs: Default::default(),
            panel_events: vec![],
            preview_click: None,
            settings_host: None,
            classic_keys: crate::keystore::ClassicKeys::default(),
            ui_actions: vec![],
            pending_tell: None,
            pending_chat_entry: None,
            initial_panel: "login".into(),
            bindings: None,
            previews,
            augmentation_sheet: hud::AugmentationSheet::default(),
            actions: vec![],
            pending_split: None,
            pending_social: None,
            social_pick: false,
            callbacks: vec![],
            panel_actions: vec![],
            close_panels: vec![],
            pending_size: None,
            outbox: Outbox::default(),
            last_pointer: None,
            pointer: (0, 0),
            shift: false,
            ctrl: false,
            alt: false,
            held_keys: std::collections::BTreeSet::new(),
            wheel: 0.0,
            desktop: Desktop::new(factory, size),
            keyboard: KeyboardState::default(),
            settings: ClassicSettings::default(),
            classic: ClassicState::default(),
            inputs: vec![],
            art,
            paths,
            canvas: None,
            screen: Screen {
                width: size.0,
                height: size.1,
                commands: vec![],
            },
            last_in_world: false,
            keyboard_on_leaving: None,
            errors: vec![],
        }
    }
    fn present_feedback(
        &mut self,
        text: &str,
        feedback: dereth_client_contract::feedback::Feedback,
        now: f64,
    ) -> Option<UiRequest> {
        let warning = feedback.severity == Some(crate::panels::FeedbackSeverity::Warning);
        (self.overlay.message(text, false, warning, now) && self.settings.interface).then_some(
            UiRequest::PlaySound {
                file: DataId(0x2000004b),
                sound_type: 0x6e,
            },
        )
    }

    /// Insert panel feedback through the shared scroll with its operation meaning.
    fn local_feedback<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        text: String,
        severity: crate::panels::FeedbackSeverity,
    ) {
        cx.add_feedback_line(
            &text,
            0x1a,
            dereth_client_contract::feedback::Feedback::local(severity),
        );
    }
    #[must_use]
    pub fn composed_screen(&self) -> &Screen {
        &self.screen
    }
    /// Whether the classic interface shows the world now.
    #[must_use]
    pub fn in_gameplay(&self) -> bool {
        self.last_in_world
    }
    fn chat_recall(&mut self) -> bool {
        if self
            .desktop
            .focused_control()
            .is_some_and(|id| id == "chat:input" || id == "input")
        {
            self.ui_actions.push("RecallLastMessage".into());
            true
        } else {
            false
        }
    }
    /// Bring the classic settings into effect: once, after start-up, when sound is available.
    pub fn initialize_settings<S: Host>(&mut self, cx: &mut Cx<'_, S>) -> Result<(), String> {
        if let Some(mut host) = self.settings_host.take() {
            let result = host.initialize(cx);
            self.settings = host.snapshot();
            self.settings_host = Some(host);
            cx.queue(Vec::new(), result?);
        }
        Ok(())
    }
    /// The settings that follow the frame: the status area, the screen-size choice under way, the
    /// field of view, and whether the game is full screen now.
    fn sync_settings<S: Host>(&mut self, cx: &mut Cx<'_, S>) -> Result<(), String> {
        let stretched = crate::keyboard_runtime::stretch_ui();
        let height = cx.present().size().1.saturating_sub(118);
        self.game_status_area = stretched && height >= 413;
        if let Some(mut host) = self.settings_host.take() {
            if let Some(done) = cx
                .resolution_completion()
                .filter(|d| d.serial != self.resolution_completion)
            {
                self.resolution_completion = done.serial;
                host.resolution_readback(done.size, done.save);
                self.settings.resolution = host.snapshot().resolution;
            }
            let result = host.sync(cx);
            self.settings_host = Some(host);
            result?;
        }
        // The options Client page shows whether the game is full screen now (Alt+Enter too); out
        // of the world, where the screens are always windowed, it shows the saved choice the
        // world will follow.
        self.settings.full_screen = if self.last_in_world {
            cx.full_screen()
        } else {
            cx.config().display.full_screen
        };
        if self.last_in_world {
            let r = self.desktop.game_rect();
            if let Some(fov) = classic_field_of_view(r.w, r.h) {
                let current = cx.scene().map(|s| s.render_preferences().field_of_view);
                if current.is_some_and(|c| (c - fov).abs() > f32::EPSILON) {
                    cx.present_mut().apply_render_preference_requests(vec![
                        UiRequest::SetPreference(
                            dereth_client_runtime::render_prefs::FIELD_OF_VIEW,
                            dereth_client_contract::PrefValue::Float(fov),
                        ),
                    ]);
                }
            }
        }
        Ok(())
    }
    /// Draw the current prompt without owning its clock or rollback.
    pub fn project_resolution(
        &mut self,
        prompt: Option<dereth_client_contract::resolution::ResolutionPrompt>,
    ) {
        use dereth_client_contract::resolution::{ResolutionAction, ResolutionPromptKind};
        if self.resolution_prompt == prompt {
            return;
        }
        self.desktop.dismiss_dialog("resolution");
        self.resolution_prompt = prompt;
        let Some(p) = prompt else {
            return;
        };
        let text = match p.kind {
            ResolutionPromptKind::OfferTest => "Test new screen size first, for 15 seconds?",
            ResolutionPromptKind::Accept => "Accept this setting?",
            ResolutionPromptKind::ApplyFailed => "Unable to change screen size!",
            ResolutionPromptKind::RevertFailed => "Unable to restore the previous screen size!",
            ResolutionPromptKind::Reset => "Resolution Reset",
        };
        let question = matches!(
            p.kind,
            ResolutionPromptKind::OfferTest | ResolutionPromptKind::Accept
        );
        let action = |yes| {
            PanelAction::Game(UiRequest::Resolution(if question {
                ResolutionAction::Answer {
                    token: p.token,
                    yes,
                }
            } else {
                ResolutionAction::Dismiss { token: p.token }
            }))
        };
        self.desktop.show_dialog(
            "resolution".into(),
            text.into(),
            vec![action(true)],
            if question {
                vec![action(false)]
            } else {
                vec![]
            },
        );
        if !question {
            self.desktop
                .set_dialog_labels("resolution", "OK".into(), None);
        }
    }
    /// Complete an earlier pointer focus change before the next keyboard message is mapped.
    pub fn prepare_host_input<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        event: &dereth_input::host::HostEvent,
    ) {
        if matches!(event, dereth_input::host::HostEvent::KeyboardInput { .. })
            && self
                .inputs
                .iter()
                .any(|input| matches!(input, Input::PointerDown { .. } | Input::PointerUp { .. }))
        {
            self.process_inputs(
                cx,
                dereth_primitives::LocalTime(cx.now()),
                cx.pregame().in_world,
            );
        }
    }

    /// The window's device events this frame, as the classic interface takes them: keys through
    /// its key map and its text fields, the pointer and buttons to its windows and the world.
    /// Lifecycle events are the shell's; a lost focus releases what was held.
    pub fn window_input<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        events: &[dereth_input::host::HostEvent],
        clipboard: &mut dyn Clipboard,
    ) {
        use dereth_input::host::HostEvent;
        use dereth_input::keys::MouseButton;
        for event in events {
            self.prepare_host_input(cx, event);
            let first_input = self.inputs.len();
            let input = match event {
                HostEvent::Focused(on) => {
                    if !on {
                        self.keyboard_focus_lost(cx);
                        cx.mouse_look_button(false);
                    }
                    (!on).then_some(Input::Cancel)
                }
                HostEvent::CursorMoved { x, y } => {
                    // While the mouse looks around, its movement turns the camera and the pointer
                    // stays where it was.
                    let looking = cx.mouse_look();
                    crate::keyboard_runtime::cursor_moved(cx, &mut self.last_pointer, *x, *y);
                    if looking {
                        None
                    } else {
                        self.pointer = (to_i32_f64(*x), to_i32_f64(*y));
                        Some(Input::PointerMove {
                            x: self.pointer.0,
                            y: self.pointer.1,
                        })
                    }
                }
                HostEvent::MouseInput {
                    button: MouseButton::Right,
                    pressed,
                } => self.right_button(cx, *pressed, self.pointer.0, self.pointer.1),
                HostEvent::MouseInput {
                    button: MouseButton::Left,
                    pressed,
                } => Some(if *pressed {
                    Input::PointerDown {
                        x: self.pointer.0,
                        y: self.pointer.1,
                    }
                } else {
                    Input::PointerUp {
                        x: self.pointer.0,
                        y: self.pointer.1,
                    }
                }),
                HostEvent::MouseWheel { notches } => {
                    self.wheel -= f64::from(*notches);
                    let delta = to_i32_f64(self.wheel.trunc());
                    self.wheel -= f64::from(delta);
                    Some(Input::Wheel {
                        x: self.pointer.0,
                        y: self.pointer.1,
                        delta,
                    })
                }
                HostEvent::ModifiersChanged { alt } => {
                    self.alt = *alt;
                    None
                }
                HostEvent::KeyboardInput { key, pressed, text } => {
                    self.key_input(cx, key.virtual_key, *pressed, text.as_deref(), clipboard)
                }
                _ => None,
            };
            if let Some(input) = input {
                self.inputs.push(input);
            }
            self.finish_chat_keyboard(cx, first_input);
        }
    }
    /// Chat keyboard edits complete before the next host key is classified.
    /// Other device inputs and window requests keep their usual frame queue.
    fn finish_chat_keyboard<S: Host>(&mut self, cx: &mut Cx<'_, S>, first_input: usize) {
        let now = dereth_primitives::LocalTime(cx.now());
        let mut retained = Vec::new();
        for action in std::mem::take(&mut self.ui_actions) {
            if matches!(
                action.as_str(),
                "Reply"
                    | "MonarchReply"
                    | "PatronReply"
                    | "TellSelected"
                    | "RecallLastMessage"
                    | "PreviousMessage"
                    | "NextMessage"
                    | "IssueSlashCommand"
                    | "START_COMMAND"
                    | "EnterChat"
                    | "ChatMode"
                    | "Chat"
                    | "EnterChatMode"
                    | "ToggleChatEntry"
            ) {
                let view = cx.hud().view(cx.objects());
                let context = Context {
                    game: &view,
                    pregame: cx.pregame(),
                    keyboard: &self.keyboard,
                    settings: &self.settings,
                    map_teleport_allowed: false,
                    classic: &self.classic,
                };
                self.desktop
                    .dispatch_panel("hud", ControlEvent::Action(action), &context);
                self.desktop.finish_chat_focus(&context);
                self.carry_out_chat_entries(cx, now);
            } else {
                retained.push(action);
            }
        }
        self.ui_actions = retained;
        let mut retained = Vec::new();
        for input in self.inputs.split_off(first_input) {
            if self.desktop.focused_control() == Some("chat:input")
                && matches!(input, Input::Key { .. } | Input::Text(_))
            {
                let view = cx.hud().view(cx.objects());
                let context = Context {
                    game: &view,
                    pregame: cx.pregame(),
                    keyboard: &self.keyboard,
                    settings: &self.settings,
                    map_teleport_allowed: false,
                    classic: &self.classic,
                };
                self.desktop.input(input, &context);
                self.desktop.finish_chat_focus(&context);
                self.carry_out_chat_entries(cx, now);
            } else {
                retained.push(input);
            }
        }
        self.inputs.extend(retained);
    }
    /// One key transition: the fixed keys, the key map, then the text fields.
    fn key_input<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        vk: usize,
        pressed: bool,
        text: Option<&str>,
        clipboard: &mut dyn Clipboard,
    ) -> Option<Input> {
        use crate::widgets::Key;
        let Ok(vk) = u16::try_from(vk) else {
            return None;
        };
        match vk {
            0x10 | 0xA0 | 0xA1 => self.shift = pressed,
            0x11 | 0xA2 | 0xA3 => self.ctrl = pressed,
            0x12 | 0xA4 | 0xA5 => self.alt = pressed,
            _ => {}
        }
        // Alt+Enter goes neither to the bindings nor to the interface: the window switches
        // between a window and full screen.
        if self.alt && vk == 0x0D {
            return None;
        }
        let help = self
            .desktop
            .focused_panel()
            .is_some_and(|id| id.starts_with("help-"));
        // The modifiers held: a key bound with them takes their chord; a key bound only plainly
        // still works with them held.
        let modifiers = (u8::from(self.shift) * crate::keystore::SHIFT)
            | (u8::from(self.ctrl) * crate::keystore::CTRL)
            | (u8::from(self.alt) * crate::keystore::ALT);
        // The copy key copies selected text before the key map sees it, so a selection in the
        // chat log is copied rather than the character strafing.
        if !help && self.ctrl && pressed && vk == 0x43 {
            if let Some(text) = self.desktop.selected_text() {
                clipboard.set(&text);
                return None;
            }
        }
        let repeat = pressed && !self.held_keys.insert(vk);
        if !pressed {
            self.held_keys.remove(&vk);
        }
        if self.keyboard_event(cx, vk, pressed, repeat, modifiers) && !self.desktop.modal_open() {
            return None;
        }
        if !pressed {
            return None;
        }
        if cx.pregame().in_world && !self.desktop.editing() && !self.desktop.modal_open() && !help {
            return None;
        }
        if self.ctrl && !help {
            return match vk {
                0x41 => Some(Input::Key {
                    key: Key::SelectAll,
                    shift: false,
                }),
                0x56 => clipboard.get().map(Input::Text),
                0x43 | 0x58 => {
                    if let Some(text) = self.desktop.selected_text() {
                        clipboard.set(&text);
                    }
                    (vk == 0x58).then_some(Input::Key {
                        key: Key::Delete,
                        shift: false,
                    })
                }
                _ => None,
            };
        }
        let world_keys =
            cx.pregame().in_world && !self.desktop.editing() && !self.desktop.modal_open() && !help;
        let key = match vk {
            0x1B if world_keys => None,
            0x1B => Some(Key::Escape),
            0x25 => Some(if self.ctrl { Key::WordLeft } else { Key::Left }),
            0x27 => Some(if self.ctrl {
                Key::WordRight
            } else {
                Key::Right
            }),
            0x21 => Some(Key::PageUp),
            0x22 => Some(Key::PageDown),
            0x2E => Some(Key::Delete),
            0x09 => Some(Key::Tab),
            0x0D => Some(Key::Enter),
            0x08 => Some(Key::Backspace),
            0x26 => Some(Key::Up),
            0x28 => Some(Key::Down),
            0x24 => Some(Key::Home),
            0x23 => Some(Key::End),
            _ => None,
        };
        if let Some(key) = key {
            return Some(Input::Key {
                key,
                shift: self.shift,
            });
        }
        match text.filter(|t| !t.is_empty() && !t.chars().any(char::is_control)) {
            Some(text) => Some(Input::Text(text.to_owned())),
            None if help && !matches!(vk, 0x10 | 0x11 | 0xA0..=0xA3) => Some(Input::Key {
                key: Key::Other,
                shift: self.shift,
            }),
            None => None,
        }
    }
    pub fn keyboard_event<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        vk: u16,
        pressed: bool,
        repeat: bool,
        modifiers: u8,
    ) -> bool {
        if pressed
            && self
                .desktop
                .focused_control()
                .is_some_and(|id| id == "chat:input" || id == "input")
        {
            // Escape leaves the chat entry, and so does the key that enters and leaves it.
            if vk == 0x1B {
                self.desktop.focus_control("");
                return true;
            }
            let name = match vk {
                0x26 => Some("PreviousMessage"),
                0x28 => Some("NextMessage"),
                _ => None,
            };
            if let Some(name) = name {
                self.ui_actions.push(name.into());
                return true;
            }
        }
        let Some(bindings) = &mut self.bindings else {
            return false;
        };
        match bindings.key(vk, pressed, repeat, modifiers) {
            Ok(outcome) => {
                let consumed = outcome.consumed || !outcome.actions.is_empty();
                self.key_outcome(cx, outcome);
                consumed
            }
            Err(error) => {
                self.desktop
                    .show_dialog("key-error".into(), error, vec![], vec![]);
                true
            }
        }
    }
    /// Focus and capture are projected into the shared input stack before each device message.
    pub fn input_scope(&self, key: Option<u16>) -> (bool, bool, bool) {
        let capture = self
            .bindings
            .as_ref()
            .is_some_and(|b| key.map_or_else(|| b.is_capturing(), |key| b.captures_key(key)));
        (self.desktop.editing(), self.desktop.modal_open(), capture)
    }

    /// A widget owns this keyboard event before gameplay sees it; releases still reach input.
    pub fn keyboard_barrier(&self, key: Option<u16>) -> bool {
        self.desktop
            .focused_panel()
            .is_some_and(|id| id.starts_with("help-"))
            || (self.ctrl && key == Some(0x43) && self.desktop.selected_text().is_some())
    }
    pub fn chat_focused(&self) -> bool {
        self.desktop.focused_control() == Some("chat:input")
    }

    /// A mapped action from the shared input manager, delivered before the next host message.
    pub fn mapped_action<S: Host>(&mut self, cx: &mut Cx<'_, S>, event: dereth_input::InputEvent) {
        // Text editing is a widget operation; the shared maps supply its typing barrier.
        if matches!(event.input_map.0, 1 | 7 | 8 | 9 | 10) {
            return;
        }
        if event.action == dereth_client_contract::actions::chat_entry::TOGGLE_CHAT_ENTRY
            && event.start
            && self.desktop.focused_control() == Some("chat:input")
        {
            self.desktop.focus_control("");
            return;
        }
        if event.action.0 == dereth_client_contract::actions::dereth::REPEAT_LAST_MESSAGE
            && event.start
            && self.chat_recall()
        {
            return;
        }
        if dereth_input::presentation::find(event.input_map, event.action).is_some_and(|row| {
            row.not_used(dereth_input::presentation::Interface::Classic)
                .is_some()
        }) {
            return;
        }
        let first = self.inputs.len();
        self.action_from_key(cx, event.to_action());
        self.finish_chat_keyboard(cx, first);
    }

    /// Characters already accepted by the shared text-mode and ignore-next-character gates.
    pub fn mapped_characters<S: Host>(&mut self, cx: &mut Cx<'_, S>, chars: Vec<char>) {
        for ch in chars.into_iter().filter(|ch| !ch.is_control()) {
            let first = self.inputs.len();
            if ch == ' ' && self.desktop.focused_control() == Some("chat:input") {
                self.inputs.push(Input::Key {
                    key: crate::widgets::Key::Space,
                    shift: self.shift,
                });
            }
            self.inputs.push(Input::Text(ch.to_string()));
            self.finish_chat_keyboard(cx, first);
        }
    }

    /// The right mouse button pressed or released at (`x`, `y`); returns what the interface sees.
    ///
    /// With right-click mouse look on, holding the button over the world looks around as the
    /// mouse-look key does: the camera's own mouse look starts with the press and ends with the
    /// release. The release also reaches the world's click handler, which examines on a click and
    /// not after a look. The Help book takes the release instead of the press.
    fn right_button<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        down: bool,
        x: i32,
        y: i32,
    ) -> Option<Input> {
        use dereth_client_contract::actions::{camera, Action};
        let help = self
            .desktop
            .panel_at(x, y)
            .is_some_and(|id| id.starts_with("help-"));
        let over = self.desktop.pointer_over_panel(x, y);
        let in_world = cx.pregame().in_world;
        if !down || !over {
            let look = down
                && in_world
                && crate::keyboard_runtime::classic_bits()
                    & crate::keyboard_runtime::RIGHT_CLICK_LOOK
                    != 0;
            if look != cx.mouse_look() {
                cx.inject_action(if look {
                    Action::begin(camera::TOGGLE_MOUSELOOK)
                } else {
                    Action::end(camera::TOGGLE_MOUSELOOK)
                });
            }
            cx.mouse_look_button(look);
        }
        // With right-click mouse look off, holding the button in the 3D view steers, and a press
        // that steers examines nothing, wherever it lands.
        let steered = self.steering.active();
        let steer = if !down {
            self.steering.release()
        } else if !over
            && in_world
            && crate::keyboard_runtime::classic_bits() & crate::keyboard_runtime::RIGHT_CLICK_LOOK
                == 0
        {
            let run = cx.model().player_system.options.toggle_run();
            self.steering.press(self.desktop.game_rect(), x, y, run)
        } else {
            vec![]
        };
        self.actions.extend(steer);
        if !down && in_world && !help {
            cx.pointer(right_release(x, y, over || steered));
        }
        (if help { !down } else { down }).then_some(Input::RightClick { x, y })
    }
    fn keyboard_focus_lost<S: Host>(&mut self, cx: &mut Cx<'_, S>) {
        let steer = self.steering.release();
        self.actions.extend(steer);
        self.shift = false;
        self.ctrl = false;
        self.alt = false;
        if let Some(bindings) = &mut self.bindings {
            let result = bindings.focus_lost();
            self.key_outcome(cx, result);
        }
    }
    /// Release transient device state before this front end is suspended.
    pub fn suspend<S: Host>(&mut self, cx: &mut Cx<'_, S>) {
        self.keyboard_focus_lost(cx);
        cx.mouse_look_button(false);
        cx.accept_actions(std::mem::take(&mut self.actions));
        self.inputs.clear();
        self.ui_actions.clear();
        self.desktop.dismiss_dialog("resolution");
        self.resolution_prompt = None;
    }

    /// The classic key map as it now is.
    pub fn set_classic_keys(&mut self, keys: crate::keystore::ClassicKeys) {
        if let Some(bindings) = &mut self.bindings {
            bindings.set_keys(&keys);
            self.keyboard = bindings.snapshot();
        }
        self.classic_keys = keys;
    }

    /// What the key page asked of the classic key map since the last call, oldest first.
    pub fn take_key_store_requests(&mut self) -> Vec<crate::keystore::KeyStoreRequest> {
        self.bindings
            .as_mut()
            .map(|b| std::mem::take(&mut b.requests))
            .unwrap_or_default()
    }

    /// An action as if its key had been pressed and let go: a window toggle or a chat command to
    /// the interface's own windows, everything else to the game.
    pub fn press_action(&mut self, id: dereth_client_contract::actions::ActionId) {
        use dereth_client_contract::actions::{names, Action};
        let name = names::enum_name_for_action(id);
        if is_ui_action(&name) {
            self.ui_actions.push(name);
        } else {
            self.actions.push(Action::begin(id));
            self.actions.push(Action::end(id));
        }
    }

    /// One action a key fired, to whatever answers it in this interface: the shortcut bar, the
    /// chat entry, the help book and the cancel key here; a window's toggle to the windows; the
    /// mouse look and this interface's own settings to the host; everything else, hold sidestep
    /// and the run key among them, to the game.
    fn action_from_key<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        action: dereth_client_contract::actions::Action,
    ) {
        use dereth_client_contract::actions::{chat_entry, dereth as own, names};
        let id = action.id.0;
        let start = action.is_start();
        let quiet = self.desktop.modal_open();
        // The mouse look key and the keys that flip one of this interface's settings.
        let host_command = match id {
            0x3D => Some("ShiftView"),
            own::PLAYER_OPTION_AUTO_CREATE_SHORTCUTS => Some("AutoCreateShortcuts"),
            own::TOGGLE_INVERT_MOUSE_LOOK => Some("InvertMouseLook"),
            own::TOGGLE_RIGHT_CLICK_MOUSE_LOOK => Some("RightClickToMouseLook"),
            own::TOGGLE_STRETCH_UI => Some("StretchUI"),
            own::TOGGLE_MUTE_ON_LOSING_FOCUS => Some("MuteOnLosingFocus"),
            _ => None,
        };
        if let Some(command) = host_command {
            if action.phase == dereth_client_contract::actions::ActionPhase::Repeat {
                return;
            }
            if let Err(error) = crate::keyboard_runtime::handle(cx, command, start) {
                self.desktop
                    .show_dialog("key-error".into(), error, vec![], vec![]);
            }
            return;
        }
        let name = names::enum_name_for_action(action.id);
        let shortcut = name
            .strip_prefix("UseQuickSlot_")
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| (1..=9).contains(n));
        match id {
            _ if !start => {}
            _ if shortcut.is_some() => {
                if !quiet && action.phase != dereth_client_contract::actions::ActionPhase::Repeat {
                    self.use_shortcut(cx, shortcut.unwrap_or(1) - 1);
                }
            }
            0x1000_010D if !quiet => self.create_shortcut(cx),
            own::CANCEL => self.inputs.push(Input::Key {
                key: crate::widgets::Key::Escape,
                shift: false,
            }),
            own::REPEAT_LAST_MESSAGE => {}
            own::TOGGLE_TRADE_PANEL => self.ui_actions.push("TradePanel".into()),
            own::TOGGLE_SPELL_RESEARCH_PANEL => self.ui_actions.push("SpellResearchPanel".into()),
            _ if (id == chat_entry::BEGIN_CHAT_MODE.0 || id == chat_entry::TOGGLE_CHAT_ENTRY.0)
                && quiet => {}
            _ if is_ui_action(&name) => self.ui_actions.push(name.clone()),
            _ => {}
        }
        if !is_ui_action(&name) && !is_interface_action(id) {
            self.actions.push(action);
        }
    }

    /// A shortcut's key: in magic combat, cast that spell of the spell bar's tab; with a targeted
    /// use armed, make that shortcut the target; with the examine cursor, examine it; otherwise
    /// use it.
    fn use_shortcut<S: Host>(&mut self, cx: &mut Cx<'_, S>, slot: u32) {
        use dereth_client_runtime::interaction::TargetMode;
        let target_mode = cx.target_mode();
        let view = cx.hud().view(cx.objects());
        if view.combat_mode() == 8 {
            self.panel_events.push((
                "spell-favorites".into(),
                ControlEvent::Magic(
                    dereth_client_contract::view::MagicNotice::CastQuickslotSpell {
                        slot: slot as usize,
                    },
                ),
            ));
            return;
        }
        if let Some(object) = view.shortcut(slot) {
            self.desktop.requests.push(match target_mode {
                TargetMode::UseTarget => UiRequest::ExecuteTargetItem(object),
                TargetMode::Examine => UiRequest::Examine(object),
                _ => UiRequest::Use(object),
            });
        }
    }

    /// The make-a-shortcut key: the selection goes on the first free shortcut.
    fn create_shortcut<S: Host>(&mut self, cx: &mut Cx<'_, S>) {
        let view = cx.hud().view(cx.objects());
        if let Some(object) = view.selected_object() {
            self.desktop
                .requests
                .push(UiRequest::CreateShortcut(object));
        }
    }

    fn key_outcome<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        outcome: crate::keybindings::KeyOutcome,
    ) {
        use crate::keybindings::CaptureResult;
        match outcome.capture {
            CaptureResult::Conflict(label) => self.desktop.show_dialog(
                "binding-conflict".into(),
                format!("This key is assigned to {label}. Reassign it?"),
                vec![PanelAction::Host(HostAction::ConfirmBinding(true))],
                vec![PanelAction::Host(HostAction::ConfirmBinding(false))],
            ),
            CaptureResult::Rejected(text) => {
                self.desktop
                    .show_dialog("binding-rejected".into(), text, vec![], vec![])
            }
            _ => {}
        }
        for action in outcome.actions {
            self.action_from_key(cx, action);
        }
        if let Some(bindings) = &self.bindings {
            self.keyboard = bindings.snapshot();
        }
    }
    fn host_action<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        origin: u64,
        action: HostAction,
    ) -> Result<(), String> {
        if let Some(bindings) = &mut self.bindings {
            if bindings.handle(&action)? {
                self.keyboard = bindings.snapshot();
                return Ok(());
            }
        }
        let ask = |cx: &mut Cx<'_, S>, request: UiRequest| cx.queue(Vec::new(), vec![request]);
        match action {
            HostAction::KeyboardOnLeaving => {
                self.keyboard_on_leaving = Some(cx.now());
            }
            HostAction::Quit => {
                self.quit_requested = true;
                cx.quit();
            }
            HostAction::VendorSellAll => ask(cx, UiRequest::VendorSellAll),
            HostAction::CloseGroundForced => {
                if let Some(ground) = cx.model().ground_object.filter(|g| g.0 != 0) {
                    ask(cx, UiRequest::CloseExternalContainer(ground));
                }
            }
            HostAction::CloseVendorForced => ask(cx, UiRequest::VendorClose),
            HostAction::LegacyHelp(context) => {
                if cx.config().display.full_screen {
                    let id = if context == 50 {
                        "help-chargen"
                    } else {
                        "help-game"
                    };
                    if crate::help::make(id).is_some() {
                        self.keyboard_focus_lost(cx);
                        self.panel_actions.push(PanelAction::Open(id.into()));
                    } else {
                        self.desktop.show_dialog(
                            "help-unavailable".into(),
                            "The local Help book is not available.".into(),
                            vec![],
                            vec![],
                        );
                        self.desktop
                            .set_dialog_labels("help-unavailable", "OK".into(), None);
                    }
                } else {
                    ask(
                        cx,
                        UiRequest::DisplayChatText {
                            feedback: dereth_client_contract::feedback::Feedback::WARNING,
                            channel: 0x1a,
                            text: "Can not run help in windowed mode!".into(),
                        },
                    );
                }
            }
            HostAction::PrintLegacyHelp { .. } => {
                self.desktop.show_dialog(
                    "help-print-error".into(),
                    "Printing is not available.".into(),
                    vec![],
                    vec![],
                );
                self.desktop
                    .set_dialog_labels("help-print-error", "OK".into(), None);
            }
            HostAction::ClassicShortcutDrop { object, slot, from } => {
                use dereth_client_contract::view::DropTarget;
                let target = match from {
                    Some(from) => DropTarget::ShortcutAlias {
                        slot,
                        from: i32::try_from(from).unwrap_or(-1),
                    },
                    None => DropTarget::ShortcutSlot(slot),
                };
                ask(
                    cx,
                    UiRequest::DragDrop {
                        item: object,
                        target,
                    },
                );
            }
            HostAction::DialogAnswer { id, accepted } => self.dialogs.answer(&id, accepted),
            HostAction::ApplyClassicSettings(s) => {
                if let Some(mut host) = self.settings_host.take() {
                    let result = host.apply(cx, s, true);
                    self.settings = host.snapshot();
                    self.settings_host = Some(host);
                    cx.queue(Vec::new(), result?);
                }
            }
            HostAction::PreviewClassicSettings(s) => {
                if let Some(mut host) = self.settings_host.take() {
                    let result = host.preview(cx, &s);
                    self.settings = host.snapshot();
                    self.settings_host = Some(host);
                    result?;
                }
            }
            HostAction::DefaultClassicSettings(s) => {
                if let Some(mut host) = self.settings_host.take() {
                    let result = host.defaults(cx, s);
                    self.settings = host.snapshot();
                    self.settings_host = Some(host);
                    cx.queue(Vec::new(), result?);
                }
            }
            HostAction::ResetClassicSettings => {
                if let Some(mut host) = self.settings_host.take() {
                    let result = host.reset(cx);
                    self.settings = host.snapshot();
                    self.settings_host = Some(host);
                    cx.queue(Vec::new(), result?);
                }
            }
            HostAction::FocusControl(id) => {
                self.desktop.focus_control(&id);
            }
            HostAction::StartTell(name) => self.pending_tell = Some(name),
            HostAction::ConfirmBinding(accepted) => {
                if let Some(bindings) = &mut self.bindings {
                    bindings.confirm_capture(accepted)?;
                    self.keyboard = bindings.snapshot();
                }
            }
            HostAction::CombatMode(raw) => ask(cx, UiRequest::SetCombatMode(raw)),
            HostAction::SocialTarget(mode) => {
                self.pending_social = if mode == 0 {
                    None
                } else {
                    Some((origin, mode))
                };
                self.social_pick = false;
            }
            HostAction::LocalFeedback { text, severity } => {
                self.local_feedback(cx, text, severity);
            }
            HostAction::SplitForPanel { object, amount } => {
                let world = cx.model();
                match world.weenie(object) {
                    Some(item) => {
                        let wcid = item.pwd.wcid;
                        let max = u32::from(item.pwd.stack_size.unwrap_or(1));
                        let name = item.display_name(
                            dereth_client_model::weenie::NameType::Appropriate,
                            world.material_name(item.pwd.material_type.unwrap_or(0)),
                        );
                        self.pending_split = Some((origin, wcid, amount.max(1)));
                        ask(
                            cx,
                            UiRequest::HouseSplitItem {
                                item: object,
                                split: amount,
                                max,
                            },
                        );
                        self.local_feedback(
                            cx,
                            format!("Splitting the {name} before adding to housing panel"),
                            crate::panels::FeedbackSeverity::Information,
                        );
                    }
                    None => self.callbacks.push((origin, ControlEvent::SplitFailed)),
                }
            }

            HostAction::CharacterOptions {
                words,
                timestamp_format,
                save,
            } => {
                // The page sets only the bits it shows; every other bit of the two words (this
                // interface's own settings, which live in the profile, and the options set from
                // elsewhere, such as appearing offline) keeps what the character holds now.
                let shown = crate::screens::shown_bits();
                let options = &cx.model().player_system.options;
                let old = [options.options, options.options2];
                let word = (words[0] & shown[0]) | (options.options & !shown[0]);
                let word2 = (words[1] & shown[1]) | (options.options2 & !shown[1]);
                // Each named option the page changed goes through the game's own option change,
                // as the other interface's page sends it: an option the server acts on at once
                // (the chat channels, the fellowship and allegiance options) is told to it at
                // once, and the rest wait for the save below.
                for (option, on) in changed_options(old, [word, word2]) {
                    ask(cx, UiRequest::SetPlayerOption(option, on));
                }
                self.classic.option_words = words;
                self.classic.timestamp_format.clone_from(&timestamp_format);
                ask(
                    cx,
                    UiRequest::SetOptionWords {
                        options: word,
                        options2: word2,
                        timestamp_format: Some(timestamp_format),
                        save,
                    },
                );
                crate::keyboard_runtime::sync_options(cx);
            }
            HostAction::AllegianceSend { action, target } => {
                use dereth_client_contract::view::AllegianceAction;
                let name = cx
                    .hud()
                    .view(cx.objects())
                    .name(target)
                    .map(str::to_owned)
                    .unwrap_or_default();
                let prompt = match action {
                    AllegianceAction::Swear => {
                        format!("\nAre you sure you want to swear allegiance to {name}?\n\n(Default is No)")
                    }
                    AllegianceAction::Break => {
                        format!("\nAre you sure you want to break allegiance with {name}?\n\n(Default is No)")
                    }
                    AllegianceAction::Kick => {
                        format!("\nAre you sure you want to remove {name} from your allegiance?\n\n(Default is No)")
                    }
                };
                ask(
                    cx,
                    UiRequest::AllegianceConfirmation {
                        action,
                        target,
                        prompt,
                    },
                );
            }
            HostAction::MapTeleport { lx, ly } => {
                if self.map_allowed(cx) {
                    ask(cx, UiRequest::MapTeleport { x: lx, y: ly });
                }
            }
            HostAction::TestSpellFormula { components } => {
                ask(cx, UiRequest::TestSpellFormula { components });
            }
            HostAction::QueryHouse => ask(cx, UiRequest::QueryHouse),
            HostAction::OpenTrade(partner) => ask(cx, UiRequest::OpenTrade(partner)),
            HostAction::AbuseLog {
                target,
                enabled,
                complaint,
            } => ask(
                cx,
                UiRequest::AbuseLogStatus {
                    target,
                    enabled,
                    complaint,
                },
            ),
            other => {
                return Err(format!(
                    "Classic host action awaiting integration: {other:?}"
                ))
            }
        }
        Ok(())
    }
    fn cancel_pending_split(&mut self) {
        if let Some((origin, _, _)) = self.pending_split.take() {
            self.callbacks.push((origin, ControlEvent::SplitFailed));
        }
    }
    fn map_allowed<S: Host>(&self, cx: &Cx<'_, S>) -> bool {
        cx.model()
            .player_qualities()
            .and_then(|q| q.bools.as_ref())
            .is_some_and(|b| {
                [0x2c, 0x2d, 0x61]
                    .iter()
                    .any(|k| b.get(k).copied().unwrap_or(false))
            })
    }
    fn refresh_classic<S: Host>(&mut self, cx: &mut Cx<'_, S>) {
        (self.classic.active_right, self.classic.active_bottom) = self.desktop.active_regions();
        // A spell learned (from a scroll or a spell research test alike) is made the spellbook's
        // current spell, and the spellbook page shows it.
        let world = cx.model();
        let book: std::collections::BTreeSet<u32> = world
            .player_qualities()
            .and_then(|q| q.spell_book.as_ref())
            .map(|b| b.keys().copied().collect())
            .unwrap_or_default();
        let added = cx.hud().stats.spells_added;
        if let Some(spell) = learned_spell(
            self.known_spells.as_ref(),
            &book,
            added != self.spells_added_seen,
        ) {
            self.panel_actions.push(PanelAction::OpenSpell {
                id: "spellbook".into(),
                spell,
            });
        }
        self.spells_added_seen = added;
        self.known_spells = Some(book);
        self.classic.equipment_priority = world
            .player
            .and_then(|id| world.inventory(id))
            .map(|inv| inv.placements.iter().map(|p| (p.iid, p.priority)).collect())
            .unwrap_or_default();
        self.classic.book_edit_privileged = world
            .player_qualities()
            .and_then(|q| q.bools.as_ref())
            .is_some_and(|b| {
                [0x2c, 0x2d]
                    .iter()
                    .any(|k| b.get(k).copied().unwrap_or(false))
            });
        self.classic.portraits.clear();
        for (id, object) in world.tables.weenies.iter() {
            // A face comes from the object's own qualities (the player's) or, for anyone else,
            // from their appraisal, which carries the face textures and palettes.
            let own = object.qualities.as_ref().and_then(|q| q.dids.as_ref());
            let appraised = world.appraisal.get(id);
            let did = |k: u32| {
                own.and_then(|d| d.get(&k).map(|v| v.0)).or_else(|| {
                    appraised.and_then(|p| dereth_client_model::appraisal_model::inq::data_id(p, k))
                })
            };
            if let Some(portrait) = portrait_from(did) {
                self.classic.portraits.insert(id, portrait);
            }
        }
        // The Character page shows this interface's own settings as their classic checkboxes.
        let own = crate::keyboard_runtime::CLASSIC_ONLY;
        let bits = crate::keyboard_runtime::classic_bits();
        self.classic.option_words = [
            (world.player_system.options.options & !own) | bits,
            world.player_system.options.options2,
        ];
        if let Some(module) = &world.player_system.module {
            self.classic.timestamp_format = module.timestamp_format.clone().unwrap_or_default();
        }
        {
            let view = cx.hud().view(cx.objects());
            let text = self.augmentation_sheet.text(&view, cx.store());
            self.classic.augmentations.clear();
            self.classic.augmentations.push_str(text);
        }
        if self.stretch_saved != Some(bits) {
            // Stretching or unstretching lays the open windows out again at once.
            if self
                .stretch_saved
                .is_some_and(|old| (old ^ bits) & crate::keyboard_runtime::STRETCH_UI != 0)
            {
                self.pending_size = Some(cx.present().size());
            }
            self.stretch_saved = Some(bits);
        }
        if let Some(code) = cx.hud_mut().classic_panels().abuse.take() {
            self.classic.abuse_response=match code {
                0x4b8=>Some("No character by that name exists. Please check your spelling and try again.".into()),
                0x4b9=>Some("You may not complain about yourself. You have only yourself to blame.".into()),
                0x4ba=>Some("Your complaint has been received.\nCommunication from the implicated party is being logged for later review by admins.\n\nThank you.".into()),
                _=>None,
            };
        }
        self.chat_filter = cx
            .hud()
            .view(cx.objects())
            .chat_window_filter(dereth_client_contract::options::sheet::window::MAIN)
            .unwrap_or(dereth_client_contract::options::sheet::MAIN_WINDOW_DEFAULT_FILTER);
        for line in cx.hud_mut().take_chat_lines(0) {
            self.chat_line(
                u32::from(line.ty),
                line.prefix.unwrap_or_default(),
                &line.body,
                line.window,
            );
        }
        if self.classic.chat.len() > 2000 {
            self.classic.chat.drain(..self.classic.chat.len() - 2000);
        }
    }
    /// One line for the classic chat, from the game's chat feed or the history a switch hands
    /// over.
    pub fn chat_line(&mut self, ty: u32, prefix: String, body: &str, window: u32) {
        use dereth_client_contract::chat::interface::{route, window::MAIN, Routed};
        if route(MAIN, self.chat_filter, ty, window) != Routed::Accepted {
            return;
        }
        let body = dereth_client_contract::chat::interface::add_text_to_scroll_trim(body);
        let text = format!("{prefix}{body}");
        self.classic.chat.push((ty, chat_text(&text)));
    }
    /// The classic interface comes up: its help book, its own settings and key map, its settings
    /// page's capabilities and its first screen.
    ///
    /// # Errors
    /// The key map cannot be read or the settings file is damaged.
    pub fn start<S: Host>(&mut self, cx: &mut Cx<'_, S>) -> Result<(), String> {
        if let Some(help_path) =
            std::env::var_os("DERETH_CLASSIC_HELP_BOOK").map(std::path::PathBuf::from)
        {
            if help_path.exists() && crate::help::make("help-game").is_none() {
                crate::help::install(&help_path)?;
            }
        }
        // The character screen's welcome text for a server that sends none: the world's own
        // message, when it sends one, is shown first (the pre-game view's).
        self.classic.welcome = std::env::var("DERETH_CLASSIC_WELCOME").unwrap_or_default();
        // This interface's own settings are in the shared store; a file of them left in its old
        // folder is carried there once.
        let retired = &self.paths.state;
        if let Some(bits) = std::fs::read_to_string(retired.join("classic-options"))
            .ok()
            .and_then(|s| u32::from_str_radix(s.trim(), 16).ok())
        {
            crate::keyboard_runtime::set_classic_bits(bits);
            let _ = std::fs::remove_file(retired.join("classic-options"));
            tracing::info!("the classic interface's own options moved into the profile");
        }
        let bindings = crate::keybindings::KeyBindings::new(&self.classic_keys);
        self.keyboard = bindings.snapshot();
        self.bindings = Some(bindings);
        let size = cx.present().size();
        self.canvas =
            Some(Canvas::new(std::sync::Arc::clone(&self.art), size).map_err(|e| e.to_string())?);
        self.settings.hardware_acceleration = true;
        // The detail-texture boxes are live wherever the page can be opened: the scene draws
        // all three classes of detail texture.
        self.settings.detail_available = true;
        self.settings.sound_available = cx.audio_mut().is_some();
        self.settings.resolutions = cx
            .display_modes()
            .into_iter()
            // The presentation refuses anything smaller than 800x600.
            .filter(|&(w, h)| w >= 800 && h >= 600)
            .collect();
        self.settings.resolutions.sort_unstable();
        self.settings.resolutions.dedup();
        self.settings.effects = true;
        self.settings.ambient = true;
        self.settings.interface = true;
        self.settings.stereo = true;
        self.settings.effects_volume = 0.66;
        self.settings.ambient_volume = 0.66;
        // With nothing saved, the slider starts where the world's brightness already is.
        self.settings.brightness =
            crate::settings_host::slider_of_brightness(cx.config().render.screen_brightness);
        self.settings.performance = 0.5;
        self.settings.camera_stiffness = 0.23;
        self.settings.auto_degrade = true;
        self.settings.resolution = self
            .settings
            .resolutions
            .iter()
            .position(|s| *s == size)
            .unwrap_or(0);
        match crate::settings_host::migrate_settings_file(
            &self.paths.state.join("settings.json"),
            &self.settings,
        ) {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!("{n} classic sound and graphics setting(s) moved into the profile")
            }
            Err(e) => tracing::warn!("the classic interface's old settings file: {e}"),
        }
        retire_folder(&self.paths.state);
        let host = crate::settings_host::SettingsHost::load(self.settings.clone())?;
        self.settings = host.snapshot();
        self.settings_host = Some(host);
        let in_world = cx.pregame().in_world;
        let initial = if in_world {
            "hud".to_owned()
        } else {
            self.initial_panel.clone()
        };
        {
            let view = cx.hud().view(cx.objects());
            let context = Context {
                game: &view,
                pregame: cx.pregame(),
                keyboard: &self.keyboard,
                settings: &self.settings,
                map_teleport_allowed: false,
                classic: &self.classic,
            };
            self.desktop.open(&initial, &context);
            if in_world {
                self.desktop.open("inventory", &context);
            }
        }
        self.last_in_world = in_world;
        self.initialize_settings(cx)
    }
    /// One pass of the shared dialog service, its questions in classic boxes.
    pub fn service_dialogs<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        now: dereth_primitives::LocalTime,
    ) {
        let mut presenter = Presenter {
            dialogs: &mut self.dialogs,
            desktop: &mut self.desktop,
            accepting: self.last_in_world,
        };
        cx.service_dialogs(Some(&mut presenter), now);
    }
    /// One talk-focus notice: true when the talk focus falls back to All.
    pub fn talk_focus_notice(
        &mut self,
        focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool {
        let _ = (focus, notice);
        false
    }
    pub fn open_vendor_buying(&mut self) {
        self.panel_events
            .push(("vendor".into(), ControlEvent::Activate("tab1".into())));
    }
    pub fn deliver_power_bar_notices(
        &mut self,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    ) {
        for notice in notices {
            self.overlay.power_bar_notice(notice);
        }
        self.classic.classic_power_level = Some(self.overlay.classic_power_level());
    }
    pub fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>) {
        self.panel_events.extend(
            notices
                .into_iter()
                .map(|notice| ("spell-favorites".into(), ControlEvent::Magic(notice))),
        );
    }
    pub fn close_examine_panel(&mut self) {
        self.close_panels.push("examine".into());
    }
    pub fn dispatch_input_action(&mut self, action: u32) -> Option<bool> {
        let name = dereth_client_contract::actions::names::enum_name_for_action(
            dereth_client_contract::actions::ActionId(action),
        );
        if is_ui_action(&name) {
            self.ui_actions.push(name);
            Some(true)
        } else {
            None
        }
    }
    /// A notice the game raised for the object panels: the windows it opens and closes.
    pub fn object_panel_notice(
        &mut self,
        world: &dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) -> Vec<UiRequest> {
        use dereth_client_model::Notice;
        if let Notice::SelectionChanged { current, .. } = notice {
            self.flash_notices.push(*current);
        }
        let open = match notice {
            Notice::OpenBook(id) => Some(("book", *id)),
            Notice::OpenVendor { vendor, .. } => Some(("vendor", *vendor)),
            Notice::OpenSalvagePanel(id) => Some(("salvage", *id)),
            Notice::OpenContainedContainer(id) => Some(("inventory", *id)),
            Notice::SetGroundObject(id) if id.0 != 0 => Some(("external-container", *id)),
            Notice::OpenSecureTrade { source } => Some(("trade", *source)),
            Notice::TradeRegistered { partner, .. } => Some(("trade", *partner)),
            Notice::BeginGame(id) => Some(("game-center", *id)),
            _ => None,
        };
        if let Some((id, object)) = open {
            self.panel_actions.push(PanelAction::OpenObject {
                id: id.into(),
                object,
            });
        }
        let close = match notice {
            Notice::CloseBook(_) => Some("book"),
            Notice::CloseVendor => Some("vendor"),
            Notice::CloseSecureTrade { .. } => Some("trade"),
            Notice::CloseSlumlord(_) => Some("maintenance"),
            Notice::SetGroundObject(id) if id.0 == 0 => Some("external-container"),
            _ => None,
        };
        if let Some(id) = close {
            self.close_panels.push(id.into());
        }
        // A failed attempt is broadcast to every panel; a pending split, like the housing
        // panel's pending request, is cleared without comparing the failed object ID.
        if matches!(notice, Notice::AttemptFailed { .. }) {
            self.cancel_pending_split();
        }
        if let Some((origin, wcid, count)) = self.pending_split {
            let id = match notice {
                Notice::ObjectCreated(id) => Some(*id),
                Notice::ItemAttributesChanged { object, kind } if kind & 1 != 0 => Some(*object),
                _ => None,
            };
            if let Some(id) = id.filter(|id| {
                world.weenie(*id).is_some_and(|w| {
                    w.pwd.wcid == wcid && u32::from(w.pwd.stack_size.unwrap_or(1)).max(1) == count
                })
            }) {
                self.callbacks.push((origin, ControlEvent::SplitReady(id)));
                self.pending_split = None;
            }
        }
        match notice {
            Notice::OpenContainedContainer(id) => vec![UiRequest::NewParentContainer(*id)],
            _ => Vec::new(),
        }
    }
    #[must_use]
    pub fn hides_world(&self) -> bool {
        !self.last_in_world
    }
    #[must_use]
    pub fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool {
        self.desktop.game_rect().contains(cursor.0, cursor.1)
            && !self.desktop.pointer_over_panel(cursor.0, cursor.1)
    }
    #[must_use]
    pub fn game_viewport(&self) -> Option<dereth_primitives::Viewport> {
        if !self.last_in_world {
            return None;
        }
        let r = self.desktop.game_rect();
        Some(dereth_primitives::Viewport {
            x: r.x.max(0).unsigned_abs(),
            y: r.y.max(0).unsigned_abs(),
            width: r.w.max(0).unsigned_abs(),
            height: r.h.max(0).unsigned_abs(),
        })
    }
    #[must_use]
    pub fn examine_panel_open(&self) -> bool {
        self.desktop.is_visible("examine")
    }
    /// Where the portal-space swirl draws this frame: the 3D view in the world, the window before.
    pub fn place_portal_space(&mut self, size: (u32, u32)) {
        self.portal_rect = Some(if self.last_in_world {
            self.desktop.game_rect()
        } else {
            crate::panels::rect(0, 0, size.0 as i32, size.1 as i32)
        });
    }
    pub fn hand_on_actions(&mut self, actions: &mut dereth_client_runtime::actions::ActionQueue) {
        actions.submit(self.take_runtime_actions());
    }

    /// Actions declined by this interface, in dispatch order.
    pub fn take_runtime_actions(&mut self) -> Vec<dereth_client_contract::actions::Action> {
        std::mem::take(&mut self.actions)
    }
    /// The 3D previews this frame shows, and where.
    #[must_use]
    pub fn shown_previews(
        &self,
    ) -> &[(
        dereth_client_contract::overlay::PreviewSpace,
        crate::widgets::Rect,
    )] {
        self.previews.shown()
    }

    /// The interface is shown again: its settings page reads the shared store again, which the
    /// other interface may have changed meanwhile.
    pub fn shown_again(&mut self) {
        if let Some(host) = &mut self.settings_host {
            host.reload();
            self.settings = host.snapshot();
        }
    }
    pub fn set_display(&mut self, size: (i32, i32)) {
        self.pending_size = Some((size.0.max(1).unsigned_abs(), size.1.max(1).unsigned_abs()));
        self.screen.width = size.0.max(1).unsigned_abs();
        self.screen.height = size.1.max(1).unsigned_abs();
    }
    /// The classic interface's step of the frame: the game's notices to its windows, its windows'
    /// input, and what they ask for, carried out through the context.
    pub fn ui_frame<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        now: dereth_primitives::LocalTime,
        notices: dereth_client_runtime::shell::UiNotices,
    ) {
        if let Err(error) = self.sync_settings(cx) {
            self.errors.push(error);
        }
        let in_world = cx.pregame().in_world;
        if in_world != self.last_in_world {
            self.selection_queries = Default::default();
            self.cancel_pending_split();
            self.overlay.reset();
            self.flash_notices.clear();
        }
        // The contents of an opened chest or corpse arrive as the ground object's notice: it opens
        // the container window (and a cleared ground object closes it).
        for notice in notices.external_container {
            use dereth_client_contract::panels::external_container::ExternalContainerNotice;
            match notice {
                ExternalContainerNotice::SetGroundObject(object) if object.0 != 0 => {
                    self.panel_actions.push(PanelAction::OpenObject {
                        id: "external-container".into(),
                        object,
                    });
                }
                ExternalContainerNotice::SetGroundObject(_) => {
                    self.close_panels.push("external-container".into());
                }
                ExternalContainerNotice::ItemMoved { .. } => {}
            }
        }
        let game_visible = cx.model().minigame.visible;
        if self.game_visible && !game_visible {
            self.close_panels.push("game-center".into());
        }
        self.game_visible = game_visible;
        let word = cx.model().player_system.options.options;
        if self.options_seen != Some(word) {
            crate::keyboard_runtime::sync_options(cx);
            self.pending_size = Some(cx.present().size());
            self.options_seen = Some(word);
        }
        self.classic.cursor_mode = match cx.target_mode() {
            dereth_client_runtime::interaction::TargetMode::None => 0,
            dereth_client_runtime::interaction::TargetMode::Use => 1,
            dereth_client_runtime::interaction::TargetMode::Examine => 2,
            dereth_client_runtime::interaction::TargetMode::UseTarget => 4,
        };
        if let Some((x, y)) = self
            .inputs
            .iter()
            .rev()
            .find_map(|i| {
                if let Input::PointerMove { x, y } = i {
                    Some((*x, *y))
                } else {
                    None
                }
            })
            .or_else(|| {
                cx.last_cursor()
                    .map(|(x, y)| (to_i32_f64(x), to_i32_f64(y)))
            })
        {
            let over_view =
                self.desktop.game_rect().contains(x, y) && !self.desktop.pointer_over_panel(x, y);
            cx.hover((x, y), self.desktop.item_at(x, y), over_view, now);
        }
        // The panels read the HUD model during this step, so it is brought up to date first.
        cx.sync_hud();
        {
            let (hud, objects) = cx.hud_and_objects();
            hud.refresh_display_names(&objects.world);
            // The link lamp reads the time since the server was last heard from; the HUD's own
            // clock advances every frame, not only when a message arrives.
            hud.now = now;
            hud.link_status =
                dereth_client_runtime::net::link_status_holder::connection_status(now.0);
        }
        let game_date_time = cx.scene().and_then(|s| s.game_date_time());
        cx.hud_mut().game_date_time = game_date_time;
        let (selected, examine, selection_facts, meters) = {
            use dereth_client_contract::view::GameView;
            let view = cx.hud().view(cx.objects());
            let selected = view.selection();
            (
                selected,
                view.examine_request(),
                selected.and_then(|id| view.selection_query_facts(id)),
                view.selected_meters(),
            )
        };
        // Every examination the shared runtime starts (the identify key, the examine cursor, a
        // click in the world) opens the classic examination panel on its object.
        if examine != self.examine_seen {
            self.examine_seen = examine;
            if let Some((object, _)) = examine {
                self.examine_open = Some(object);
            }
        }
        self.classic.reply_targets = cx.hud().reply_targets(cx.model());
        let requests = self
            .selection_queries
            .update(selected, selection_facts, meters);
        cx.queue(Vec::new(), requests);
        let split = cx.model().split;
        self.classic.stack_split = Some((split.split_size, split.max_split_size));
        self.refresh_classic(cx);
        let target = cx.model().chat.last_speakable_target;
        self.classic.chat_target = target.and_then(|id| {
            cx.hud()
                .view(cx.objects())
                .name(id)
                .map(|name| (id, name.to_owned()))
        });
        let talk_focus = cx.model().chat.talk_focus;
        self.classic.chat_focus = Some((talk_focus as u8, cx.model().chat.selectable_focuses()));
        let transient = std::mem::take(&mut cx.hud_mut().classic_panels().transient);
        for (text, feedback) in transient {
            if let Some(sound) = self.present_feedback(&text, feedback, now.0) {
                cx.play_sounds(vec![sound]);
            }
        }
        self.deliver_power_bar_notices(notices.power_bar);
        // The game window's status line: the latest default-type line that arrived in a frame in
        // which the game routed text. The classic status area is shown when the stretched
        // interface is tall enough.
        let default_lines = std::mem::take(&mut cx.hud_mut().classic_panels().default_lines);
        let minigame_lines = cx.hud().stats.minigame_lines;
        if minigame_lines != self.minigame_lines_seen {
            self.minigame_lines_seen = minigame_lines;
            if let Some(last) = default_lines.last() {
                self.game_info.clone_from(last);
            }
        }
        self.classic.game_status = if self.game_status_area {
            self.game_info.clone()
        } else {
            String::new()
        };
        for notice in notices.salvage {
            // Using a salvaging tool opens the salvage window on it; the window then takes the
            // notice, as it takes the items added and removed.
            if let dereth_client_contract::panels::salvage::SalvageNotice::Open(tool) = notice {
                self.panel_actions.push(PanelAction::OpenObject {
                    id: "salvage".into(),
                    object: tool,
                });
            }
            self.panel_events
                .push(("salvage".into(), ControlEvent::Salvage(notice)));
        }
        if !notices.slumlord_range_exits.is_empty() {
            self.close_panels.push("maintenance".into());
        }
        if !notices.book_range_exits.is_empty() {
            self.close_panels.push("book".into());
        }
        for item in notices.trade_for_dummies {
            self.panel_events.push((
                "trade".into(),
                ControlEvent::Drop {
                    id: "offer".into(),
                    payload: DragPayload::Object(item),
                    slot: 0,
                },
            ));
        }
        if self.social_pick && !cx.looking_for_object() {
            if let Some((origin, _)) = self.pending_social.take() {
                let id = cx.found_object();
                self.callbacks
                    .push((origin, ControlEvent::WorldTarget((id.0 != 0).then_some(id))));
            }
            self.social_pick = false;
        }
        // The dialog service's answers for a panel: the house payment's goes to its window.
        for request in std::mem::take(&mut self.dialogs.delivered) {
            if let UiRequest::HousePaymentConfirmationAnswer { rent, confirmed } = request {
                self.panel_events.push((
                    "maintenance".into(),
                    ControlEvent::HousePaymentConfirmation { rent, confirmed },
                ));
            } else {
                self.desktop.requests.push(request);
            }
        }
        let map_allowed = self.map_allowed(cx);
        {
            let view = cx.hud().view(cx.objects());
            let context = Context {
                game: &view,
                pregame: cx.pregame(),
                keyboard: &self.keyboard,
                settings: &self.settings,
                map_teleport_allowed: map_allowed,
                classic: &self.classic,
            };
            if let Some(size) = self.pending_size.take() {
                self.desktop.resize(size, &context);
            }
            // Windows the game closes are taken down without their close button's request (a
            // container's close would use the container again and reopen it).
            for id in std::mem::take(&mut self.close_panels) {
                self.desktop.remove_visual(&id, &context);
            }
            // A notice over the side column brings the side panel back (on the inventory) when
            // it was closed; the inventory stays when the notice goes.
            if self.last_in_world
                && self.desktop.side_notice_showing()
                && self.desktop.active_regions().0.is_empty()
            {
                self.desktop.open("inventory", &context);
            }
            if let Some(object) = self.examine_open.take() {
                self.desktop.open_object("examine", object, &context);
            }
            // A book opens each time the server opens it; the vendor and trade windows show while
            // the game holds an open shop or trade.
            if self.last_in_world {
                if let Some(book) = context.game.open_book() {
                    if self.book_opening != Some(book.opening) || !self.desktop.is_open("book") {
                        self.book_opening = Some(book.opening);
                        self.desktop.open_object("book", book.book_id, &context);
                    }
                } else if self.desktop.is_open("book") {
                    self.desktop.remove_visual("book", &context);
                }
                open_house_profile(&mut self.desktop, &mut self.house_profile_seen, &context);
                let salvage = context.game.salvage_list();
                match (
                    salvage.visible,
                    salvage.tool,
                    self.desktop.is_open("salvage"),
                ) {
                    (true, Some(tool), false) => {
                        self.desktop.open_object("salvage", tool, &context)
                    }
                    (false, _, true) => self.desktop.remove_visual("salvage", &context),
                    _ => {}
                }
                let shop = context.game.shop();
                match (shop.open, shop.vendor, self.desktop.is_open("vendor")) {
                    (true, Some(vendor), false) => {
                        self.desktop.open_object("vendor", vendor, &context);
                    }
                    (false, _, true) => self.desktop.remove_visual("vendor", &context),
                    _ => {}
                }
                let trade = context.game.trade();
                match (trade.open, trade.partner, self.desktop.is_open("trade")) {
                    (true, Some(partner), false) => {
                        self.desktop.open_object("trade", partner, &context);
                    }
                    (false, _, true) => self.desktop.remove_visual("trade", &context),
                    _ => {}
                }
            }
            self.desktop
                .apply(0, std::mem::take(&mut self.panel_actions), &context);
            for (origin, event) in std::mem::take(&mut self.callbacks) {
                self.desktop.dispatch(origin, event, &context);
            }
            for (id, event) in std::mem::take(&mut self.panel_events) {
                self.desktop.dispatch_panel(&id, event, &context);
            }
            if in_world != self.last_in_world {
                self.dialogs.generation += 1;
                self.desktop.close_all(&context);
                self.desktop
                    .open(if in_world { "hud" } else { "login" }, &context);
                // Configure Keyboard left the world for the key page: it opens now, on the
                // character screen, as the screen's own keyboard button opens it. A leave that
                // did not come of it (the asking was long ago) opens nothing.
                if let Some(asked) = self.keyboard_on_leaving.take() {
                    if !in_world && now.0 - asked < KEYBOARD_ON_LEAVING_WAIT {
                        self.desktop.dispatch_panel(
                            "login",
                            ControlEvent::Activate("keyboard".into()),
                            &context,
                        );
                    }
                }
                // The side panel shows the inventory when a character enters the world.
                if in_world {
                    self.desktop.open("inventory", &context);
                }
                self.last_in_world = in_world;
            }
            self.desktop.tick_controls(now.0, &context);
            self.desktop.tick(&context);
            // The combat bar shows in melee and missile combat, unless the "advanced combat
            // interface" character option hides it.
            let advanced_combat = self.classic.option_words[0] & 0x1000 != 0;
            if self.last_in_world {
                match context.game.combat_mode() {
                    2 | 4 if advanced_combat => {
                        self.desktop.close("combat", &context);
                        self.desktop.close("spell-favorites", &context);
                    }
                    2 | 4 => {
                        self.desktop.close("spell-favorites", &context);
                        if !self.desktop.is_open("combat") {
                            self.desktop.open("combat", &context);
                        }
                    }
                    8 => {
                        self.desktop.close("combat", &context);
                        if !self.desktop.is_open("spell-favorites") {
                            self.desktop.open("spell-favorites", &context);
                        }
                    }
                    _ => {
                        self.desktop.close("combat", &context);
                        self.desktop.close("spell-favorites", &context);
                    }
                }
            }
            for name in std::mem::take(&mut self.ui_actions) {
                self.desktop
                    .dispatch_panel("hud", ControlEvent::Action(name), &context);
            }
            if let Some(name) = self.pending_tell.take() {
                self.desktop.requests.push(UiRequest::StartTell { name });
            }
        }
        if let Some(update) = self.pending_chat_entry.take() {
            self.apply_chat_entry(cx, update);
        }
        self.carry_out_chat_entries(cx, now);
        self.process_inputs(cx, now, in_world);
        self.carry_out_requests(cx, now);
    }
    /// Process queued device events in arrival order, including world interception before
    /// window dispatch. A following host key sees the focus established by an earlier click.
    fn process_inputs<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        now: dereth_primitives::LocalTime,
        in_world: bool,
    ) {
        let map_allowed = self.map_allowed(cx);
        let armed = cx.target_mode() != dereth_client_runtime::interaction::TargetMode::None;
        let mut pointer_events = Vec::new();
        let mut paper_doll_clicks = Vec::new();
        {
            // A left click on a panel while a targeting cursor is armed acts with it (the item
            // clicked is the target) and then ends it, as a click in the world does; a right
            // click neither acts nor ends it.
            for input in std::mem::take(&mut self.inputs) {
                let view = cx.hud().view(cx.objects());
                let context = Context {
                    game: &view,
                    pregame: cx.pregame(),
                    keyboard: &self.keyboard,
                    settings: &self.settings,
                    map_teleport_allowed: map_allowed,
                    classic: &self.classic,
                };

                // Escape, with no dialog up and no text being typed, first ends a targeting
                // cursor, then clears the selection, and only then closes pages.
                if matches!(
                    input,
                    Input::Key {
                        key: crate::widgets::Key::Escape,
                        ..
                    }
                ) && in_world
                    && !self.desktop.modal_open()
                    && !self.desktop.editing()
                    && self.desktop.drag_payload.is_none()
                {
                    match escape_step(armed, context.game.selected_object().is_some()) {
                        EscapeStep::EndTargeting => {
                            self.leave_target_after_click = true;
                            continue;
                        }
                        EscapeStep::ClearSelection => {
                            self.desktop
                                .requests
                                .push(UiRequest::Select(dereth_primitives::ObjectId(0)));
                            continue;
                        }
                        EscapeStep::ClosePages => {}
                    }
                }
                if armed {
                    if let Input::PointerUp { x, y } = input {
                        if leaves_target(armed, self.desktop.pointer_over_panel(x, y)) {
                            self.leave_target_after_click = true;
                        }
                    }
                }
                let click = match input {
                    Input::PointerUp { x, y } => Some((x, y, false)),
                    Input::RightClick { x, y } => Some((x, y, true)),
                    _ => None,
                };
                // A click on the paper doll picks from the model, unless it is on an item slot
                // beside the doll: that slot's own press and release stay together.
                if !self.desktop.modal_open() && self.desktop.drag_payload.is_none() {
                    if let Some((x, y, right_click)) =
                        click.filter(|&(x, y, _)| self.desktop.item_at(x, y).is_none())
                    {
                        if let Some((index, preview)) = self
                            .desktop
                            .previews
                            .iter()
                            .enumerate()
                            .rev()
                            .find(|(_, p)| {
                                p.kind == PreviewKind::PaperDoll && p.rect.contains(x, y)
                            })
                        {
                            let origin = self.desktop.preview_owners[index];
                            let double_click = !right_click
                                && self.preview_click.is_some_and(|(token, px, py, t)| {
                                    token == origin
                                        && (x - px).abs() + (y - py).abs() < 4
                                        && t.elapsed().as_millis() < 500
                                });
                            self.preview_click = (!right_click && !double_click)
                                .then(|| (origin, x, y, std::time::Instant::now()));
                            paper_doll_clicks.push((
                                origin,
                                preview.clone(),
                                x,
                                y,
                                right_click,
                                double_click,
                            ));
                            // The release is the doll's, so the press's hold on its window ends
                            // here: the next press goes where it lands.
                            self.desktop.release_pointer(&context);
                            continue;
                        }
                    }
                }
                if in_world {
                    // A press on an object in the world and a move past the drag distance picks
                    // the object up on the pointer, to be dropped on the backpack or a pack.
                    match input {
                        Input::PointerDown { x, y }
                            if !self.desktop.pointer_over_panel(x, y)
                                && self.desktop.drag_payload.is_none() =>
                        {
                            self.world_held = Some((x, y));
                        }
                        Input::PointerUp { .. } => self.world_held = None,
                        Input::PointerMove { x, y } => {
                            if self.steering.active() {
                                let run = cx.model().player_system.options.toggle_run();
                                let view = self.desktop.game_rect();
                                let steer = self.steering.moved(view, x, y, run);
                                self.actions.extend(steer);
                            }
                            if let Some((px, py)) = self.world_held {
                                if (x - px).abs().max((y - py).abs()) >= WORLD_DRAG_DISTANCE {
                                    self.world_held = None;
                                    let pressed =
                                        (!cx.looking_for_object()).then(|| cx.found_object());
                                    if let Some(object) =
                                        pressed.and_then(|id| world_drag_object(cx.model(), id))
                                    {
                                        self.desktop.drag_payload =
                                            Some(crate::panels::DragPayload::Object(object));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    let mouse = match input {
                        // A second press close by and soon after is a double click, which uses.
                        Input::PointerDown { x, y } if !self.desktop.pointer_over_panel(x, y) => {
                            let double = world_double_click(self.world_press, x, y);
                            self.world_press = (!double).then(|| (x, y, std::time::Instant::now()));
                            Some((if double { 0x0A } else { 7 }, true, x, y))
                        }
                        Input::PointerDown { x, y } => Some((7, true, x, y)),
                        Input::PointerUp { x, y } => Some((7, false, x, y)),
                        Input::RightClick { x, y } => Some((8, true, x, y)),
                        _ => None,
                    };
                    if let Some((action, start, x, y)) = mouse.filter(|(_, _, x, y)| {
                        self.desktop.drag_payload.is_none()
                            && !self.desktop.pointer_over_panel(*x, *y)
                    }) {
                        pointer_events.push(dereth_client_runtime::interaction::UiMouseEvent {
                            action,
                            start,
                            x,
                            y,
                            over: None,
                        });
                        if start && action == 7 && self.pending_social.is_some() {
                            self.social_pick = true;
                        }
                        // A release in the world ends any press a window was holding on to, so
                        // no window keeps the pointer after the button is up.
                        if !start {
                            self.desktop.release_pointer(&context);
                        }
                        continue;
                    }
                }
                self.desktop.input(input, &context);
                self.desktop.finish_chat_focus(&context);
                self.carry_out_chat_entries(cx, now);
            }
        }
        for event in pointer_events {
            cx.pointer(event);
        }
        for (origin, preview, x, y, right_click, double_click) in paper_doll_clicks {
            let store = std::sync::Arc::clone(cx.store());
            match self.previews.pick(cx.present_mut(), &store, &preview, x, y) {
                Ok(hit) => {
                    let (object_index, part_index, equipment_mask) =
                        hit.map_or((u32::MAX, u32::MAX, 0), |h| {
                            (
                                u32_from(h.object_index),
                                h.part_index as u32,
                                h.equipment_mask,
                            )
                        });
                    self.callbacks.push((
                        origin,
                        ControlEvent::PreviewHit {
                            object_index,
                            part_index,
                            equipment_mask,
                            right_click,
                            double_click,
                        },
                    ));
                }
                Err(e) => self.errors.push(e),
            }
        }
    }
    pub fn apply_chat_entry<S: Host>(
        &mut self,
        cx: &Cx<'_, S>,
        update: dereth_client_contract::chat::entry::EntryUpdate,
    ) {
        if update.window != dereth_client_contract::chat::interface::window::MAIN {
            return;
        }
        if !self.desktop.is_open("hud") {
            self.pending_chat_entry = Some(update);
            return;
        }
        let view = cx.hud().view(cx.objects());
        let context = Context {
            game: &view,
            pregame: cx.pregame(),
            keyboard: &self.keyboard,
            settings: &self.settings,
            map_teleport_allowed: false,
            classic: &self.classic,
        };
        self.desktop.apply_chat_entry(update, &context);
    }

    /// Chat edits finish between input events; unrelated requests retain their existing queue.
    fn carry_out_chat_entries<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        now: dereth_primitives::LocalTime,
    ) {
        let mut retained = Vec::new();
        for request in std::mem::take(&mut self.desktop.requests) {
            if matches!(
                request,
                UiRequest::ChatEntry { .. }
                    | UiRequest::ChatLine { .. }
                    | UiRequest::StartTell { .. }
                    | UiRequest::SetTalkFocus { .. }
            ) {
                retained.extend(cx.run_request(request, now, &mut |_, _| false));
                for update in cx.take_chat_entry_updates() {
                    self.apply_chat_entry(cx, update);
                }
            } else {
                retained.push(request);
            }
        }
        self.desktop.requests = retained;
    }

    /// What the classic windows asked for this frame, carried out: the session's asks, then each
    /// request through the runtime's owners, then the host actions.
    fn carry_out_requests<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        now: dereth_primitives::LocalTime,
    ) {
        let mut requests = std::mem::take(&mut self.desktop.requests);
        // Whatever carried out a targeted use (a click, a number key, the radar, the paper
        // doll), the targeting cursor ends with it.
        let completed_target = requests
            .iter()
            .any(|r| matches!(r, UiRequest::ExecuteTargetItem(_)));
        if std::mem::take(&mut self.leave_target_after_click) || completed_target {
            requests.push(UiRequest::SetTargetMode(
                dereth_client_contract::view::TargetMode::None,
            ));
        }
        for request in &requests {
            if let UiRequest::StackSliderChanged { split, max } = request {
                self.classic.stack_split = Some((*split, *max));
            }
        }
        let mut unowned = Vec::new();
        for request in requests {
            // The character screens' session operations are the runtime's.
            let request = match request {
                UiRequest::CharacterAction(action) => {
                    cx.run_character_actions(vec![action]);
                    continue;
                }
                UiRequest::CharGenAction(action) => {
                    cx.run_chargen_actions(vec![action]);
                    continue;
                }
                // Leave World: the character session ends and the character list returns.
                UiRequest::EndCharacterSession { .. } => {
                    cx.log_off_character();
                    continue;
                }
                other => other,
            };
            // The interface choice is the option store's, as the retail Options page writes it;
            // the shell follows it on the next frame. The classic interface's own options are
            // the store's too.
            if let UiRequest::SetPreference(name, value) = &request {
                if *name == dereth_client_contract::options::interface::INTERFACE
                    || dereth_client_contract::options::classic::NAMES.contains(name)
                {
                    let _ = dereth_client_contract::options::store::set_value(name, value.clone());
                }
            }
            let used = match &request {
                UiRequest::Use(id) => Some(*id),
                _ => None,
            };
            let mut answer = |_, _| false;
            // What no owner here took (an option's preference for the scene, the camera or the
            // display, above all) goes on to the frame's own owners, as the other interface's
            // requests do.
            unowned.extend(cx.run_request(request, now, &mut answer));
            for update in cx.take_chat_entry_updates() {
                self.apply_chat_entry(cx, update);
            }
            cx.deliver_selection_notices(now);
            if let Some(automatic) =
                used.and_then(|id| crate::keyboard_runtime::auto_shortcut_after_use(cx.model(), id))
            {
                unowned.extend(cx.run_request(automatic, now, &mut |_, _| false));
            }
        }
        if !unowned.is_empty() {
            cx.queue(Vec::new(), unowned);
        }
        let origins = std::mem::take(&mut self.desktop.host_origins);
        for (origin, action) in origins
            .into_iter()
            .zip(std::mem::take(&mut self.desktop.host_actions))
        {
            // A request from the interface that cannot be carried out is reported, not fatal.
            if let Err(e) = self.host_action(cx, origin, action) {
                self.desktop
                    .show_dialog("host-action-error".into(), e, vec![], vec![]);
                self.desktop
                    .set_dialog_labels("host-action-error", "OK".into(), None);
            }
        }
    }
    /// The selection indicator (the "vivid targeting" character option, on by default): four
    /// corner marks around the selected object, or an arrow at the viewport's edge pointing to it
    /// when it is out of view. `project` is where the shell's drawn world puts an object.
    pub fn draw_world_target<S: Host>(
        &mut self,
        cx: &mut Cx<'_, S>,
        project: &dyn Fn(ObjectId) -> Option<dereth_client_contract::target::Projection>,
    ) {
        self.target_commands.clear();
        let vivid = cx.model().player_system.options.options & 0x8000 != 0;
        // Yourself, what you own and anything in a container have no place in the world to mark.
        let target = world_object(cx.model());
        if let (true, true, Some(selected)) = (self.last_in_world, vivid, target) {
            let r = self.desktop.game_rect();
            let projection = project(selected);
            let colour = cx
                .hud()
                .radar
                .iter()
                .find(|e| e.id == selected)
                .map_or(3, crate::panels::hud::blip_color);
            self.target_commands = crate::world_overlay::target_marks(projection, r, colour);
        }
        // The shared selection blink belongs to the other interface; this one flashes its own.
        drop(cx.take_selection_lighting());
        let now = cx.now();
        for selected in self.flash_notices.drain(..) {
            self.overlay.flash(selected, now);
        }
        if self.last_in_world {
            self.overlay.sync_selection(cx.model().selected, now);
            let found = cx
                .last_cursor()
                .filter(|(x, y)| {
                    let (x, y) = (to_i32_f64(*x), to_i32_f64(*y));
                    self.desktop.game_rect().contains(x, y)
                        && !self.desktop.pointer_over_panel(x, y)
                })
                .map(|_| cx.found_object())
                .filter(|id| id.0 != 0)
                // Display Tooltips off: no name or hint of what is under the pointer.
                .filter(|_| {
                    cx.model()
                        .player_system
                        .options
                        .get(dereth_client_model::player::option::SHOW_TOOLTIPS)
                });
            self.overlay.hover(cx.model(), found, now);
            self.overlay.tick(now);
        }
        for change in self.overlay.drain_lighting() {
            use crate::world_overlay::LightingChange;
            use dereth_animation::parts::LightingMode;
            let (object, mode) = match change {
                LightingChange::Restore(id) => (id, LightingMode::Restore),
                LightingChange::Set {
                    object, minimum, ..
                } => (
                    object,
                    if minimum > 0.0 {
                        LightingMode::High
                    } else {
                        LightingMode::Low
                    },
                ),
            };
            cx.light_object(object, mode);
        }
    }
    pub fn update_cursor<S: Host>(&mut self, cx: &mut Cx<'_, S>) {
        self.cursor_commands.clear();
        self.system_pointer = None;
        if cx.mouse_look() {
            return;
        }
        let Some((x, y)) = cx.last_cursor() else {
            return;
        };
        let (x, y) = (to_i32_f64(x), to_i32_f64(y));
        let found = self.desktop.item_at(x, y).unwrap_or_else(|| {
            if self.desktop.pointer_over_panel(x, y) {
                ObjectId(0)
            } else {
                cx.found_object()
            }
        });
        let view = cx.hud().view(cx.objects());
        if let Some(payload) = &self.desktop.drag_payload {
            match payload {
                DragPayload::Object(id) | DragPayload::Shortcut { object: id, .. } => {
                    if let Some(decoration) = view.slot_decoration(*id) {
                        self.cursor_commands.push(crate::Command::ItemIcon {
                            recipe: crate::item_art::recipe(
                                &decoration,
                                crate::item_art::Surface::Drag,
                            ),
                            x,
                            y,
                            width: 32,
                            height: 32,
                            clip: None,
                        });
                        return;
                    }
                }
                DragPayload::Spell(id) => {
                    if let Some(spell) = view.spell(*id) {
                        if let Some(icon) = spell.icon {
                            self.cursor_commands.push(crate::Command::SpellIcon {
                                icon: icon.0,
                                power: spell.icon_power,
                                bitfield: spell.bitfield,
                                x,
                                y,
                                width: 32,
                                height: 32,
                                clip: None,
                            });
                            return;
                        }
                    }
                }
                DragPayload::Text(_) => {}
            }
        }
        let world = cx.model();
        let art = if self.last_in_world {
            crate::cursor::resolve(crate::cursor::CursorInput {
                mode: self.classic.cursor_mode,
                combat_mode: world.combat.combat_mode as u32,
                alternate: cx.busy_count() > 0,
                hovered: found.0 != 0,
                target_valid: world.target_compatible_with_object(found, world.targeting_object),
            })
        } else {
            Some(crate::cursor::pregame())
        };
        // The pointer is the window system's, as the other interface's is: it follows the mouse
        // at the system's rate, not a frame behind it.
        self.system_pointer =
            if let Some(crate::Command::Image { did, .. }) = self.desktop.pointer_art(x, y) {
                u32::from_str_radix(&did, 16)
                    .ok()
                    .map(|did| crate::cursor::SystemPointer {
                        did,
                        hot_x: 0,
                        hot_y: 0,
                        keyed: false,
                    })
            } else {
                art.map(crate::cursor::SystemPointer::of)
            };
    }
    /// The pointer the window system should show, as [`Self::update_cursor`] chose it; `None`
    /// hides the system's pointer.
    #[must_use]
    pub fn system_pointer(&self) -> Option<crate::cursor::SystemPointer> {
        self.system_pointer
    }
    /// The pixels of `pointer` for the window system: its width, its height and its blue, green,
    /// red and alpha pixels. `None` when the early portal has no such image.
    #[must_use]
    pub fn system_pointer_pixels(
        &self,
        pointer: crate::cursor::SystemPointer,
    ) -> Option<(u32, u32, Vec<[u8; 4]>)> {
        let image = self.art.image(pointer.did)?;
        Some((
            image.width,
            image.height,
            crate::cursor::pointer_pixels(&image.rgba, pointer.keyed),
        ))
    }
    /// Build this frame's overlay, outside the frame bracket: the windows, the world overlay, the
    /// target marks and a dragged icon, with the previews and the portal swirl in their places.
    pub fn compose_ui<S: Host>(&mut self, cx: &mut Cx<'_, S>) {
        let sounds = std::mem::take(&mut self.desktop.sounds);
        if self.settings.interface && !sounds.is_empty() {
            cx.play_sounds(
                sounds
                    .into_iter()
                    .map(|sound| UiRequest::PlaySound {
                        file: DataId(0x2000004b),
                        sound_type: sound,
                    })
                    .collect(),
            );
        }
        let idle = cx.model().request_lock.is_idle() && !cx.model().combat.attack_in_progress;
        let cursor = cx
            .last_cursor()
            .map(|(x, y)| (to_i32_f64(x), to_i32_f64(y)));
        self.desktop
            .update_drag_preview(&cx.hud().view(cx.objects()), cursor, idle);
        self.screen = self.desktop.screen();
        if self.last_in_world {
            self.screen.commands.extend(self.overlay.commands_at(
                self.desktop.game_rect(),
                !self.desktop.active_regions().0.is_empty(),
                crate::keyboard_runtime::stretch_ui(),
                |text| {
                    crate::renderer::measure_text_width(crate::world_overlay::FONT, text)
                        .unwrap_or(0)
                },
            ));
        }
        self.screen.commands.extend(self.target_commands.clone());
        self.screen.commands.extend(self.cursor_commands.clone());
        let previews = self.desktop.previews.clone();
        if let Err(e) = self.previews.prepare(cx, &previews) {
            self.errors.push(e);
        }
        let portal = self.portal_rect.take();
        let size = cx.present().size();
        let store = std::sync::Arc::clone(cx.store());
        let shown = self.previews.shown().to_vec();
        if let Some(canvas) = &mut self.canvas {
            canvas.resize(size);
            if let Err(e) = canvas.load_runtime_images(&self.screen, &store) {
                self.errors.push(e.to_string());
                return;
            }
            let space_of = |index: usize| {
                let preview = previews.get(index)?;
                let (space, rect) = shown.iter().find(|(_, r)| *r == preview.rect)?;
                Some((*space, [rect.x, rect.y, rect.w, rect.h]))
            };
            if let Err(e) = canvas.compose(cx.present_mut(), &self.screen, &space_of) {
                self.errors.push(e.to_string());
            }
            if let Some(r) = portal {
                canvas.prepend_preview(
                    dereth_client_contract::overlay::PreviewSpace::Portal,
                    [r.x, r.y, r.w, r.h],
                );
            }
        }
    }
    /// Draw the overlay [`Self::compose_ui`] built, inside the frame bracket.
    ///
    /// # Errors
    /// Whatever the device answers.
    pub fn draw_ui(
        &mut self,
        present: &mut dyn Presentation,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        if let Some(canvas) = &self.canvas {
            present.draw_overlay(canvas.items())?;
        }
        Ok(())
    }
}

fn names_of_own(name: &str) -> bool {
    dereth_client_contract::actions::names::DERETH_ACTION_NAMES
        .iter()
        .any(|(_, n)| *n == name)
}

/// The classic interface's old settings folder, once everything in it has moved into the shared
/// store and key map: removed when nothing is left in it, and left with what is otherwise.
fn retire_folder(folder: &std::path::Path) {
    let Ok(mut entries) = std::fs::read_dir(folder) else {
        return;
    };
    if entries.next().is_none() {
        if std::fs::remove_dir(folder).is_ok() {
            tracing::info!("the classic interface's old settings folder is retired");
        }
    } else {
        tracing::info!(
            "the classic interface's old settings folder {} still holds files this client no longer reads",
            folder.display()
        );
    }
}

fn is_ui_action(name: &str) -> bool {
    // This client's own actions (the performance panel's) are the game's, not a window's.
    if names_of_own(name) {
        return false;
    }
    name.starts_with("Toggle") && name.ends_with("Panel")
        || name.starts_with("SelectQuickSlot_")
        || matches!(
            name,
            "EnterChatMode"
                | "ToggleChatEntry"
                | "START_COMMAND"
                | "Reply"
                | "PatronReply"
                | "MonarchReply"
                | "TellSelected"
                | "SelectionSplitStack"
                | "ToggleHelp"
                | "LOGOUT"
        )
}

/// The actions this interface answers itself and the game is not to see: the shortcut bar's
/// keys, the help key, and this interface's cancel, repeat-message, trade and research keys.
fn is_interface_action(id: u32) -> bool {
    use dereth_client_contract::actions::dereth as own;
    let name = dereth_client_contract::actions::names::enum_name_for_action(
        dereth_client_contract::actions::ActionId(id),
    );
    name.starts_with("UseQuickSlot_")
        || matches!(
            id,
            0x1000_010D
                | own::CANCEL
                | own::REPEAT_LAST_MESSAGE
                | own::TOGGLE_TRADE_PANEL
                | own::TOGGLE_SPELL_RESEARCH_PANEL
        )
}

/// The world view's field-of-view preference, in degrees, that gives the classic interface's
/// view: 90 degrees across the viewport's width. The shared scene turns the preference into a
/// vertical angle by dividing it by the viewport's aspect less a tenth, so the preference is the
/// wanted vertical angle times that divisor. `None` for an empty viewport.
fn classic_field_of_view(width: i32, height: i32) -> Option<f32> {
    if width <= 0 || height <= 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let aspect = width as f32 / height as f32;
    let vertical = 2.0 * dereth_primitives::num::math::atanf(1.0 / aspect);
    Some(vertical * (aspect - 0.1) / dereth_client_contract::camera::DEG_TO_RAD)
}

#[cfg(test)]
mod field_of_view_tests {
    //! Behaviour: none (the classic view's angle carried onto the shared preference).
    use super::classic_field_of_view;

    #[test]
    fn a_square_viewport_sees_ninety_degrees_across() {
        let fov = classic_field_of_view(400, 400).unwrap();
        // Vertical 90 degrees at aspect 1, times (1 - 0.1).
        assert!((fov - 81.0).abs() < 0.01, "{fov}");
        assert_eq!(classic_field_of_view(0, 10), None);
    }
}

/// Whether a left release while a targeting cursor is armed ends it once the panel under it has
/// acted: only over a panel (a click in the world ends it in the world's own handler).
fn leaves_target(armed: bool, over_panel: bool) -> bool {
    armed && over_panel
}

/// Whether a left press is the second of a double click: within four pixels of the previous press
/// and half a second after it.
fn world_double_click(previous: Option<(i32, i32, std::time::Instant)>, x: i32, y: i32) -> bool {
    previous.is_some_and(|(px, py, t)| {
        (x - px).abs() + (y - py).abs() < 4 && t.elapsed().as_millis() < 500
    })
}

/// A message as the chat window holds it: one trailing line break ends the last line rather than
/// starting an empty one (an empty line inside the message stays).
fn chat_text(text: &str) -> String {
    let line = dereth_client_contract::chat::interface::add_text_to_scroll_trim(text);
    // The game's lines name a speaker as a tag run (`<Tell:IIDString:id:name>name<\Tell>`), which
    // a chat window shows as the name: the markup is read as the retail chat window reads it.
    if line.contains('<') {
        dereth_text::tag::parse(line).text
    } else {
        line.to_owned()
    }
}

/// Stands for any classic window under the pointer: not the 3D view.
const CLASSIC_WINDOW: dereth_client_contract::ElementId = dereth_client_contract::ElementId(0);

/// The right-button release as the game's mouse handling sees it. A release over a window is
/// not a click in the world, so it starts no examine search there: a search started outside the
/// 3D view finds nothing and ends nothing, and would hold off every later single click in the
/// world. A release that ends steering is not one either: the press steered and examines nothing.
fn right_release(
    x: i32,
    y: i32,
    over_window: bool,
) -> dereth_client_runtime::interaction::UiMouseEvent {
    dereth_client_runtime::interaction::UiMouseEvent {
        action: 8,
        start: false,
        x,
        y,
        over: over_window.then_some(CLASSIC_WINDOW),
    }
}

#[cfg(test)]
mod click_and_chat_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn a_right_release_over_a_window_is_marked_as_over_one() {
        // A right click on a pack item in the side panel reaches the world's click handler as a
        // release over a window, which ends no search the next world click needs; one in the 3D
        // view is a plain release.
        let over = right_release(650, 300, true);
        assert_eq!((over.action, over.start), (8, false));
        assert_eq!(over.over, Some(CLASSIC_WINDOW));
        assert_eq!(right_release(250, 200, false).over, None);
    }
    #[test]
    fn a_trailing_line_break_adds_no_empty_chat_line() {
        assert_eq!(
            chat_text("You hit the drudge for 5 points of damage!\n"),
            "You hit the drudge for 5 points of damage!"
        );
        assert_eq!(chat_text("a\n\nb"), "a\n\nb");
        assert_eq!(chat_text("plain"), "plain");
    }
    #[test]
    fn a_tell_names_its_sender_without_the_name_markup() {
        assert_eq!(
            chat_text(
                "<Tell:IIDString:1342177281:+Infiltrater>+Infiltrater<\\Tell> tells you, \"yo\"\n"
            ),
            "+Infiltrater tells you, \"yo\""
        );
        assert_eq!(
            chat_text("[General] <Tell:IIDString:0:Bob>Bob<\\Tell> says, \"a < b\""),
            "[General] Bob says, \"a < b\""
        );
        assert_eq!(chat_text("2 < 3 and 4 > 1"), "2 < 3 and 4 > 1");
    }
    #[test]
    fn only_a_left_release_over_a_panel_ends_targeting_after_the_panel_acts() {
        assert!(leaves_target(true, true));
        assert!(!leaves_target(true, false));
        assert!(!leaves_target(false, true));
    }
    #[test]
    fn a_second_press_close_by_and_soon_is_a_double_click() {
        let now = std::time::Instant::now();
        assert!(world_double_click(Some((100, 100, now)), 101, 102));
        assert!(!world_double_click(Some((100, 100, now)), 110, 100));
        assert!(!world_double_click(None, 100, 100));
        let old = now - std::time::Duration::from_millis(600);
        assert!(!world_double_click(Some((100, 100, old)), 100, 100));
    }
}

fn open_house_profile(desktop: &mut Desktop, seen: &mut u64, context: &Context<'_>) {
    let notices = context.game.slumlord_notices();
    let visible = context.game.payment_lists().visible && context.game.slumlord().is_some();
    if visible && (notices != *seen || !desktop.is_open("maintenance")) {
        desktop.open("maintenance", context);
    } else if !visible && desktop.is_open("maintenance") {
        desktop.remove_visual("maintenance", context);
    }
    *seen = notices;
}

#[cfg(test)]
mod house_profile_tests {
    use super::*;
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::{hud::Hud, objects::ObjectStream};
    use dereth_protocol::{trade::HouseProfileMessage, Message};

    fn with_context(hud: &Hud, objects: &ObjectStream, run: impl FnOnce(&Context<'_>)) {
        run(&Context {
            game: &hud.view(objects),
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            map_teleport_allowed: false,
            classic: &Default::default(),
        });
    }

    /// Behaviour: panels.house-purchase.each-profile-opens-the-payment-window-once
    #[test]
    fn a_house_profile_opens_once_and_a_repeated_use_reopens_after_close() {
        let mut hud = Hud::new();
        let mut objects = ObjectStream::new();
        let mut desktop = Desktop::new(crate::panels::services::make, (800, 600));
        let mut seen = 0;
        with_context(&hud, &objects, |context| {
            open_house_profile(&mut desktop, &mut seen, context);
            assert!(!desktop.is_open("maintenance"));
        });
        let message = HouseProfileMessage {
            covenant_crystal: ObjectId(9),
            profile: dereth_protocol::trade::HouseProfile {
                house_type: 1,
                ..Default::default()
            },
        };
        let mut blob = HouseProfileMessage::OPCODE.0.to_le_bytes().to_vec();
        blob.extend(dereth_protocol::write_body(&message).unwrap());
        let event = SessionEvent::UiEvent {
            opcode: HouseProfileMessage::OPCODE,
            blob,
        };
        for receipt in 1..=2 {
            objects.world.recv_house_profile(&message);
            hud.apply_events(std::slice::from_ref(&event), &mut objects.world);
            with_context(&hud, &objects, |context| {
                assert_eq!(context.game.slumlord_notices(), receipt);
                open_house_profile(&mut desktop, &mut seen, context);
                assert!(desktop.is_open("maintenance"));
                let mut rebuilt = Desktop::new(crate::panels::services::make, (800, 600));
                open_house_profile(&mut rebuilt, &mut seen, context);
                assert!(
                    rebuilt.is_open("maintenance"),
                    "a rebuilt interface projects the already open session"
                );
                desktop.close("maintenance", context);
            });
            let requests = std::mem::take(&mut desktop.requests);
            assert_eq!(
                requests,
                vec![
                    UiRequest::PaymentList(
                        dereth_client_contract::panels::slumlord::PaymentAction::Close,
                    ),
                    UiRequest::UnregisterSlumlordRange,
                ],
            );
            for request in requests {
                match request {
                    UiRequest::PaymentList(action) => {
                        assert!(objects.world.payment_action(action, |_| None).is_empty());
                    }
                    UiRequest::UnregisterSlumlordRange => {
                        objects.world.unregister_slumlord_range_checks();
                    }
                    _ => unreachable!(),
                }
            }
            assert!(!objects.world.payment_lists_view().visible);
            with_context(&hud, &objects, |context| {
                for _ in 0..3 {
                    open_house_profile(&mut desktop, &mut seen, context);
                    assert!(
                        !desktop.is_open("maintenance"),
                        "an explicit close survives idle projection"
                    );
                }
            });
        }
    }
}

/// A face from the face data ids: the eyes, nose and mouth textures (9, 10, 11) and the eyes,
/// hair and skin palettes (16, 15, 17). A face needs all six.
fn portrait_from(did: impl Fn(u32) -> Option<u32>) -> Option<ClassicPortrait> {
    let textures = [9, 10, 11].map(|k| did(k).unwrap_or(0));
    let palettes = [16, 15, 17].map(|k| did(k).unwrap_or(0));
    textures
        .iter()
        .chain(&palettes)
        .all(|&v| v != 0)
        .then_some(ClassicPortrait { textures, palettes })
}

#[cfg(test)]
mod portrait_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn a_face_needs_all_six_face_data_ids() {
        let all = |k: u32| Some(0x0500_0000 + k);
        let p = portrait_from(all).unwrap();
        assert_eq!(p.textures, [0x0500_0009, 0x0500_000a, 0x0500_000b]);
        assert_eq!(p.palettes, [0x0500_0010, 0x0500_000f, 0x0500_0011]);
        assert!(portrait_from(|k| (k != 15).then_some(1)).is_none());
    }
}

/// The spell a spell-learned message added this frame: one in the book now that was not before.
/// Nothing on the first look at the book (the character's whole book arriving at log-in).
fn learned_spell(
    before: Option<&std::collections::BTreeSet<u32>>,
    now: &std::collections::BTreeSet<u32>,
    message: bool,
) -> Option<u32> {
    let before = before?;
    message
        .then(|| now.difference(before).next().copied())
        .flatten()
}

#[cfg(test)]
mod learned_spell_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn a_spell_learned_message_names_the_spell_new_to_the_book() {
        let book = |ids: &[u32]| {
            ids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<u32>>()
        };
        assert_eq!(
            learned_spell(Some(&book(&[1, 2])), &book(&[1, 2, 157]), true),
            Some(157)
        );
        // The log-in book, and a book that changed with no such message, open nothing.
        assert_eq!(learned_spell(None, &book(&[1, 2]), true), None);
        assert_eq!(
            learned_spell(Some(&book(&[1])), &book(&[1, 2]), false),
            None
        );
        assert_eq!(
            learned_spell(Some(&book(&[1, 2])), &book(&[1, 2]), true),
            None
        );
    }
}

/// What Escape does next in the world.
#[derive(Debug, PartialEq)]
enum EscapeStep {
    EndTargeting,
    ClearSelection,
    ClosePages,
}
fn escape_step(armed: bool, selected: bool) -> EscapeStep {
    if armed {
        EscapeStep::EndTargeting
    } else if selected {
        EscapeStep::ClearSelection
    } else {
        EscapeStep::ClosePages
    }
}
#[cfg(test)]
mod escape_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn escape_ends_targeting_then_clears_the_selection_then_closes_pages() {
        assert_eq!(escape_step(true, true), EscapeStep::EndTargeting);
        assert_eq!(escape_step(false, true), EscapeStep::ClearSelection);
        assert_eq!(escape_step(false, false), EscapeStep::ClosePages);
    }
}

/// How far the pointer moves from a press on the world before the pressed object is picked up.
const WORLD_DRAG_DISTANCE: i32 = 5;

/// What a press on the world and a move pick up: the object under the pointer (not the selection:
/// a drag over open ground picks up nothing), at peace, when it lies loose in the world and nobody
/// wields it. Anything fixed in place, such as a lifestone, and any creature stay where they are;
/// the player may pick up only themselves of the creatures, by dragging from their own body.
fn world_drag_object(
    world: &dereth_client_model::World,
    pressed: dereth_primitives::ObjectId,
) -> Option<dereth_primitives::ObjectId> {
    if pressed.0 == 0 {
        return None;
    }
    let w = world.weenie(pressed)?;
    world_drag_allowed(WorldDragFacts {
        at_peace: world.combat.combat_mode.raw() == 1,
        loose: w.current_state != dereth_client_model::weenie::PositionState::InContainer
            && !world.is_owned_by_player(pressed)
            && w.pwd.wielder_id.is_none_or(|id| id.0 == 0),
        fixed: w.pwd.bitfield & 0x4 != 0,
        creature: w.pwd.obj_type & 0x10 != 0,
        player: world.player == Some(pressed),
    })
    .then_some(pressed)
}

#[derive(Clone, Copy, Debug, Default)]
struct WorldDragFacts {
    at_peace: bool,
    loose: bool,
    fixed: bool,
    creature: bool,
    player: bool,
}
fn world_drag_allowed(f: WorldDragFacts) -> bool {
    f.at_peace && (f.player || (f.loose && !f.fixed && !f.creature))
}

#[cfg(test)]
mod world_drag_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn a_drag_where_no_object_lies_picks_up_nothing_whatever_is_selected() {
        let mut world = dereth_client_model::World::default();
        world.player = Some(dereth_primitives::ObjectId(0x5000_0001));
        world.selected = world.player;
        assert_eq!(
            world_drag_object(&world, dereth_primitives::ObjectId(0)),
            None
        );
    }
    #[test]
    fn only_loose_items_at_peace_can_be_dragged_out_of_the_world() {
        let item = WorldDragFacts {
            at_peace: true,
            loose: true,
            ..Default::default()
        };
        assert!(world_drag_allowed(item));
        assert!(!world_drag_allowed(WorldDragFacts {
            at_peace: false,
            ..item
        }));
        // A lifestone is fixed in place; an NPC is a creature.
        assert!(!world_drag_allowed(WorldDragFacts {
            fixed: true,
            ..item
        }));
        assert!(!world_drag_allowed(WorldDragFacts {
            creature: true,
            ..item
        }));
        assert!(!world_drag_allowed(WorldDragFacts {
            loose: false,
            ..item
        }));
        // The player is the one creature that can be picked up, to drop on the shortcut bar.
        let me = WorldDragFacts {
            at_peace: true,
            creature: true,
            player: true,
            ..Default::default()
        };
        assert!(world_drag_allowed(me));
    }
}

/// The selected object when it lies in the world: not the player, nothing the player owns, and
/// nothing inside a container.
fn world_object(world: &dereth_client_model::World) -> Option<dereth_primitives::ObjectId> {
    world.selected.filter(|&id| {
        world.weenie(id).is_some_and(|w| {
            world.player != Some(id)
                && !world.is_owned_by_player(id)
                && w.current_state != dereth_client_model::weenie::PositionState::InContainer
        })
    })
}

/// The named character options whose bit differs between the option words `old` and `new`, each
/// with its new value, in the options' own order.
fn changed_options(
    old: [u32; 2],
    new: [u32; 2],
) -> Vec<(dereth_client_contract::PlayerOption, bool)> {
    use dereth_client_model::player::options::{OptionWord, PLAYER_OPTIONS};
    dereth_client_contract::PlayerOption::ALL
        .into_iter()
        .filter_map(|option| {
            let (_, word, mask) =
                PLAYER_OPTIONS[dereth_client_runtime::hud::option_ordinal(option)];
            let i = usize::from(word == OptionWord::Two);
            let on = new[i] & mask != 0;
            ((old[i] & mask != 0) != on).then_some((option, on))
        })
        .collect()
}

#[cfg(test)]
mod option_change_tests {
    //! Behaviour: none (which options a whole-word change names; the wire is tested where the
    //! option change is sent).
    use super::changed_options;
    use dereth_client_contract::PlayerOption as P;

    #[test]
    fn a_word_change_names_each_option_it_moves_and_no_other() {
        // Allegiance chat (first word bit 30) on, General chat (second word bit 8) off, and the
        // unnamed first bit (automatic shortcuts) moved too.
        let old = [0, 0x100];
        let new = [0x4000_0001, 0];
        assert_eq!(
            changed_options(old, new),
            vec![(P::HearAllegianceChat, true), (P::HearGeneralChat, false)]
        );
        assert!(changed_options(new, new).is_empty());
    }
}

/// Whether a chat window with message filter `filter` shows a line of chat type `ty`: the
/// filter has one bit per type.
#[cfg(test)]
fn shows(filter: u64, ty: u32) -> bool {
    dereth_client_contract::chat::interface::type_is_active(filter, ty)
}

#[cfg(test)]
mod chat_filter_tests {
    //! Behaviour: none (the filter's bit per chat type; the page that sets it is tested where
    //! it is drawn).
    use super::shows;

    #[test]
    fn a_filter_shows_the_types_whose_bits_it_has() {
        let all_but_bubbles = dereth_client_contract::options::sheet::MAIN_WINDOW_DEFAULT_FILTER;
        assert!(shows(all_but_bubbles, 0));
        assert!(!shows(all_but_bubbles, 26));
        // The General channel's group off: its type no longer reaches the window.
        let general = 0x0800_0000;
        assert!(shows(all_but_bubbles, 27));
        assert!(!shows(all_but_bubbles & !general, 27));
    }
}

#[cfg(test)]
#[path = "../tests/chat_adapter.rs"]
mod chat_tests;

#[cfg(test)]
#[path = "../tests/spell_adapter.rs"]
mod spell_tests;
