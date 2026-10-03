//! Classic window lifetimes, modal callbacks and composition over the game viewport.
use crate::{control_host::ControlHost, panels::*, widgets::Input, Command, Screen};

type Factory = fn(&str) -> Option<Box<dyn Panel>>;
#[derive(Debug)]
struct Window {
    token: u64,
    key: String,
    panel: Box<dyn Panel>,
    controls: ControlHost,
    frame: PanelFrame,
    x: i32,
    y: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalPlacement {
    Hud,
    Centered,
    Fixed { x: i32, y: i32 },
}
impl ModalPlacement {
    fn for_panel(id: &str) -> Option<Self> {
        if id == "startup" {
            Some(Self::Fixed { x: 198, y: 174 })
        } else if id.starts_with("create-") {
            Some(Self::Fixed { x: 355, y: 174 })
        } else if matches!(id, "login" | "keyboard" | "key-edit") {
            Some(Self::Fixed { x: 199, y: 250 })
        } else {
            None
        }
    }
}
#[derive(Debug)]
struct Modal {
    placement: ModalPlacement,
    default_accept: bool,
    accept_label: String,
    reject_label: Option<String>,
    id: String,
    origin: u64,
    text: String,
    accept: Vec<PanelAction>,
    reject: Vec<PanelAction>,
    controls: ControlHost,
}
#[derive(Debug)]
pub struct Desktop {
    pub sounds: Vec<u32>,
    pub drag_payload: Option<DragPayload>,
    factory: Factory,
    windows: Vec<Window>,
    modal: Option<Modal>,
    dialog_queue: std::collections::VecDeque<Modal>,
    next: u64,
    capture: Option<u64>,
    focus: Option<u64>,
    size: (u32, u32),
    stretched: bool,
    pub requests: Vec<UiRequest>,
    pub host_actions: Vec<HostAction>,
    pub host_origins: Vec<u64>,
    pub previews: Vec<Preview>,
    pub preview_owners: Vec<u64>,
    pub errors: Vec<String>,
}
impl Desktop {
    pub fn update_drag_preview(
        &mut self,
        game: &dyn GameView,
        pointer: Option<(i32, i32)>,
        ready: bool,
    ) {
        let dragged = match self.drag_payload {
            Some(DragPayload::Object(id)) | Some(DragPayload::Shortcut { object: id, .. }) => {
                Some(id)
            }
            _ => None,
        };
        let spell = matches!(self.drag_payload, Some(DragPayload::Spell(_)));
        let visible = self.visible_tokens();
        for window in &mut self.windows {
            let pointer = pointer
                .filter(|_| self.modal.is_none() && visible.contains(&window.token))
                .map(|(x, y)| (x - window.x, y - window.y));
            window
                .controls
                .update_item_drop_preview(game, dragged, pointer, ready, 0);
            window.controls.update_slot_hint(pointer.filter(|_| spell));
        }
    }
    pub fn item_at(&self, x: i32, y: i32) -> Option<ObjectId> {
        if self.modal.is_some() {
            return None;
        }
        let visible = self.visible_tokens();
        self.windows
            .iter()
            .rev()
            .filter(|w| visible.contains(&w.token))
            .find_map(|w| w.controls.item_at(x - w.x, y - w.y))
    }
    fn visible_tokens(&self) -> std::collections::BTreeSet<u64> {
        let external = self
            .windows
            .iter()
            .any(|w| matches!(w.key.as_str(), "vendor" | "external-container"));
        let right = self
            .windows
            .iter()
            .rev()
            .find(|w| right_pane(&w.key))
            .map(|w| w.token);
        self.windows
            .iter()
            .filter(|w| !right_pane(&w.key) || Some(w.token) == right)
            .filter(|w| !external || !matches!(w.key.as_str(), "combat" | "spell-favorites"))
            .map(|w| w.token)
            .collect()
    }
    pub fn focused_control(&self) -> Option<&str> {
        self.windows
            .iter()
            .find(|w| Some(w.token) == self.focus)?
            .controls
            .focused_control()
    }
    pub fn modal_open(&self) -> bool {
        self.modal.is_some()
    }
    pub fn modal_button_rect(&self, accept: bool) -> Option<crate::widgets::Rect> {
        let frame = Self::modal_frame(self.size, self.stretched, self.modal.as_ref()?);
        frame
            .controls
            .into_iter()
            .find(|c| c.id == if accept { "accept" } else { "cancel" })
            .map(|c| c.rect)
    }
    pub fn game_rect(&self) -> crate::widgets::Rect {
        let mut r = panels_world(self.size);
        // With no side panel open the world runs to the window's right edge.
        if !self.windows.iter().any(|w| right_pane(&w.key)) {
            r.w = i32::try_from(self.size.0).unwrap_or(r.w).max(r.w);
        }
        let visible = self.visible_tokens();
        if let Some(w) = self.windows.iter().rev().find(|w| {
            visible.contains(&w.token)
                && matches!(
                    w.key.as_str(),
                    "trade"
                        | "vendor"
                        | "external-container"
                        | "salvage"
                        | "maintenance"
                        | "combat"
                        | "spell-favorites"
                )
        }) {
            r.h = (w.y - r.y + 1).max(0);
        }
        r
    }
    /// Whether a notice is showing over the side column.
    #[must_use]
    pub fn side_notice_showing(&self) -> bool {
        self.modal
            .as_ref()
            .is_some_and(|m| matches!(m.placement, ModalPlacement::Hud))
    }
    pub fn show_dialog(
        &mut self,
        id: String,
        text: String,
        accept: Vec<PanelAction>,
        reject: Vec<PanelAction>,
    ) {
        if let Some(existing) = self
            .modal
            .as_mut()
            .filter(|m| m.id == id)
            .or_else(|| self.dialog_queue.iter_mut().find(|m| m.id == id))
        {
            existing.text = text;
            existing.accept = accept;
            existing.reject = reject;
            return;
        }
        let modal = Modal {
            placement: if self.is_open("hud") {
                ModalPlacement::Hud
            } else {
                self.windows
                    .iter()
                    .rev()
                    .find_map(|w| ModalPlacement::for_panel(w.panel.id()))
                    .unwrap_or(ModalPlacement::Centered)
            },
            default_accept: self
                .windows
                .iter()
                .rev()
                .find_map(|w| Self::pregame_dialog_default(w.panel.id()))
                .unwrap_or(false),
            accept_label: "Yes".into(),
            reject_label: Some("No".into()),
            id,
            origin: 0,
            text,
            accept,
            reject,
            controls: ControlHost::default(),
        };
        if self.modal.is_none() {
            self.modal = Some(modal);
        } else {
            self.dialog_queue.push_back(modal);
        }
        self.capture = None;
    }
    fn pregame_dialog_default(id: &str) -> Option<bool> {
        if id.starts_with("create-") || matches!(id, "keyboard" | "key-edit") {
            Some(true)
        } else if id == "login" {
            Some(false)
        } else {
            None
        }
    }
    pub fn set_dialog_default(&mut self, id: &str, accept: bool) {
        for modal in self.modal.iter_mut().chain(self.dialog_queue.iter_mut()) {
            if modal.id == id {
                modal.default_accept = accept;
            }
        }
    }
    pub fn set_dialog_placement(&mut self, id: &str, placement: ModalPlacement) {
        for modal in self.modal.iter_mut().chain(self.dialog_queue.iter_mut()) {
            if modal.id == id {
                modal.placement = placement;
            }
        }
    }
    /// Whether the box `id` is up or waiting its turn.
    #[must_use]
    pub fn dialog_showing(&self, id: &str) -> bool {
        self.modal
            .iter()
            .chain(self.dialog_queue.iter())
            .any(|m| m.id == id)
    }
    pub fn dismiss_dialog(&mut self, id: &str) {
        self.dialog_queue.retain(|m| m.id != id);
        if self.modal.as_ref().is_some_and(|m| m.id == id) {
            self.modal = self.dialog_queue.pop_front();
        }
    }
    pub fn set_dialog_labels(&mut self, id: &str, accept: String, reject: Option<String>) {
        if let Some(m) = self
            .modal
            .as_mut()
            .filter(|m| m.id == id)
            .or_else(|| self.dialog_queue.iter_mut().find(|m| m.id == id))
        {
            m.accept_label = accept;
            m.reject_label = reject;
        }
    }
    fn answer_modal(&mut self, accepted: bool, context: &Context<'_>) {
        if let Some(modal) = self.modal.take() {
            self.apply(
                modal.origin,
                if accepted { modal.accept } else { modal.reject },
                context,
            );
        }
        if self.modal.is_none() {
            self.modal = self.dialog_queue.pop_front();
        }
    }
    /// Give control `id` the keyboard focus; an empty id takes the focus from every control.
    pub fn focus_control(&mut self, id: &str) -> bool {
        if id.is_empty() {
            for w in &mut self.windows {
                w.controls.focus_control("");
            }
            return true;
        }
        let visible = self.visible_tokens();
        for w in self.windows.iter_mut().rev() {
            if !visible.contains(&w.token) {
                continue;
            }
            if w.controls.focus_control(id) {
                self.focus = Some(w.token);
                return true;
            }
        }
        false
    }
    pub fn dispatch_panel(&mut self, id: &str, event: ControlEvent, context: &Context<'_>) {
        if let Some(token) = self.windows.iter().find(|w| w.key == id).map(|w| w.token) {
            self.dispatch(token, event, context);
        }
    }
    pub fn active_regions(&self) -> (String, String) {
        let mut right = String::new();
        let mut bottom = String::new();
        for w in &self.windows {
            if matches!(
                w.key.as_str(),
                "trade" | "vendor" | "external-container" | "salvage" | "maintenance"
            ) {
                bottom = w.key.clone();
            } else if !matches!(
                w.key.as_str(),
                "hud" | "chat" | "radar" | "vitals" | "shortcuts" | "spell-favorites" | "combat"
            ) {
                right = w.key.clone();
            }
        }
        (right, bottom)
    }
    /// What the copy key copies: the focused window's selection, else the selection of a
    /// window that does not take the focus, such as the chat log's.
    pub fn selected_text(&self) -> Option<String> {
        let of = |w: &Window| {
            w.controls
                .selected_text()
                .or_else(|| w.panel.selected_text())
        };
        self.windows
            .iter()
            .find(|w| Some(w.token) == self.focus)
            .and_then(of)
            .or_else(|| self.windows.iter().find_map(of))
    }
    pub fn editing(&self) -> bool {
        self.windows
            .iter()
            .find(|w| Some(w.token) == self.focus)
            .is_some_and(|w| w.controls.editing())
    }
    pub fn new(factory: Factory, size: (u32, u32)) -> Self {
        Self {
            sounds: vec![],
            drag_payload: None,
            factory,
            windows: vec![],
            modal: None,
            dialog_queue: Default::default(),
            next: 1,
            capture: None,
            focus: None,
            size,
            stretched: false,
            requests: vec![],
            host_actions: vec![],
            host_origins: vec![],
            previews: vec![],
            preview_owners: vec![],
            errors: vec![],
        }
    }
    /// Open panel `id` on `object` (or bring it forward and point it at `object`).
    pub fn open_object(&mut self, id: &str, object: ObjectId, context: &Context<'_>) {
        if let Some(token) = self.open(id, context) {
            if let Some(w) = self.windows.iter_mut().find(|w| w.token == token) {
                w.panel.set_object(object);
            }
        }
    }
    pub fn is_open(&self, id: &str) -> bool {
        self.windows
            .iter()
            .any(|w| w.key == id || w.panel.id() == id)
    }
    pub fn focused_panel(&self) -> Option<&str> {
        self.windows
            .iter()
            .find(|w| Some(w.token) == self.focus)
            .map(|w| w.key.as_str())
    }
    pub fn panel_at(&self, x: i32, y: i32) -> Option<&str> {
        let visible = self.visible_tokens();
        self.windows
            .iter()
            .rev()
            .find(|w| {
                visible.contains(&w.token)
                    && rect(
                        w.x,
                        w.y,
                        w.frame.screen.width as i32,
                        w.frame.screen.height as i32,
                    )
                    .contains(x, y)
            })
            .map(|w| w.key.as_str())
    }
    pub fn is_visible(&self, id: &str) -> bool {
        let visible = self.visible_tokens();
        self.windows
            .iter()
            .any(|w| (w.key == id || w.panel.id() == id) && visible.contains(&w.token))
    }
    fn deactivate_token(&mut self, token: u64, context: &Context<'_>) {
        let events = self
            .windows
            .iter_mut()
            .find(|w| w.token == token)
            .map(|w| w.controls.deactivate())
            .unwrap_or_default();
        if self.capture == Some(token) {
            self.capture = None;
        }
        for event in events {
            self.dispatch(token, event, context);
        }
    }
    fn release_inactive(&mut self, context: &Context<'_>) {
        let visible = self.visible_tokens();
        let inactive: Vec<_> = self
            .windows
            .iter()
            .filter(|w| {
                self.modal.is_some() || !visible.contains(&w.token) || Some(w.token) != self.focus
            })
            .map(|w| w.token)
            .collect();
        for token in inactive {
            self.deactivate_token(token, context);
        }
    }
    /// State-driven teardown must not invoke the user's close-button request.
    pub fn remove_visual(&mut self, id: &str, context: &Context<'_>) {
        let tokens: Vec<_> = self
            .windows
            .iter()
            .filter(|w| w.key == id || w.panel.id() == id)
            .map(|w| w.token)
            .collect();
        for token in tokens {
            self.deactivate_token(token, context);
            self.windows.retain(|w| w.token != token);
            if self.focus == Some(token) {
                self.focus = self.windows.last().map(|w| w.token);
            }
        }
        self.fit_bottom_windows(context);
    }
    pub fn close(&mut self, id: &str, context: &Context<'_>) {
        if let Some((token, key)) = self
            .windows
            .iter()
            .find(|w| w.key == id || w.panel.id() == id)
            .map(|w| (w.token, w.key.clone()))
        {
            let shown = self.shown_pane() == Some(token);
            self.deactivate_token(token, context);
            self.dispatch(token, ControlEvent::Activate("close".into()), context);
            self.windows.retain(|w| w.token != token);
            if self.focus == Some(token) {
                self.focus = self.windows.last().map(|w| w.token);
            }
            if shown && right_pane(&key) {
                self.close_retained_panes(context);
            }
            self.fit_bottom_windows(context);
        }
    }
    /// Whether `token` is a spell's examination shown over the spellbook: closing it brings the
    /// spellbook back rather than closing the side panel.
    fn spell_examination_over_spellbook(&self, token: u64) -> bool {
        let mut panes = self.windows.iter().rev().filter(|w| right_pane(&w.key));
        panes
            .next()
            .is_some_and(|w| w.token == token && w.key == "examine-spell")
            && panes
                .next()
                .is_some_and(|w| matches!(w.key.as_str(), "spellbook" | "components"))
    }
    /// The side panel's page on show, if any.
    fn shown_pane(&self) -> Option<u64> {
        self.windows
            .iter()
            .rev()
            .find(|w| right_pane(&w.key))
            .map(|w| w.token)
    }
    /// The side panel shows one page at a time, and closing the page on show closes the side
    /// panel: the pages it replaced earlier (kept so a page returns as it was left) close too,
    /// rather than coming back into view.
    fn close_retained_panes(&mut self, context: &Context<'_>) {
        let keys: Vec<String> = self
            .windows
            .iter()
            .filter(|w| right_pane(&w.key))
            .map(|w| w.key.clone())
            .collect();
        for key in keys {
            if let Some(token) = self.windows.iter().find(|w| w.key == key).map(|w| w.token) {
                self.deactivate_token(token, context);
                self.dispatch(token, ControlEvent::Activate("close".into()), context);
                self.windows.retain(|w| w.token != token);
            }
        }
        self.focus = self.windows.last().map(|w| w.token);
    }
    pub fn close_all(&mut self, context: &Context<'_>) {
        let tokens: Vec<_> = self.windows.iter().map(|w| w.token).collect();
        for token in tokens {
            self.deactivate_token(token, context);
            self.dispatch(token, ControlEvent::Activate("close".into()), context);
        }
        self.windows.clear();
        self.modal = None;
        self.dialog_queue.clear();
        self.capture = None;
        self.focus = None;
    }
    pub fn open(&mut self, id: &str, context: &Context<'_>) -> Option<u64> {
        self.stretched = context.classic.option_words[0] & 0x20_0000 != 0;
        let service = matches!(id, "trade" | "maintenance" | "salvage");
        if service
            || matches!(
                id,
                "vendor" | "external-container" | "combat" | "spell-favorites"
            )
        {
            for other in ["trade", "maintenance", "salvage"] {
                if other != id {
                    self.close(other, context);
                }
            }
        }
        if service {
            if self.is_open("external-container") {
                self.host_actions.push(HostAction::CloseGroundForced);
                self.host_origins.push(0);
            }
            self.host_actions.push(HostAction::CombatMode(1));
            self.host_origins.push(0);
            self.close("combat", context);
            self.close("spell-favorites", context);
        }
        if (service || id == "external-container") && self.is_open("vendor") {
            self.remove_visual("vendor", context);
            self.host_actions.push(HostAction::CloseVendorForced);
            self.host_origins.push(0);
        }
        if service || id == "vendor" {
            self.remove_visual("external-container", context);
        }
        if let Some(i) = self.windows.iter().position(|w| w.key == id) {
            let w = self.windows.remove(i);
            let token = w.token;
            self.windows.push(w);
            self.focus = Some(token);
            self.release_inactive(context);
            return Some(token);
        }
        let Some(mut panel) = (self.factory)(id) else {
            self.errors.push(format!("Unknown classic panel: {id}"));
            return None;
        };
        let wide = matches!(
            id,
            "trade"
                | "vendor"
                | "external-container"
                | "salvage"
                | "maintenance"
                | "combat"
                | "spell-favorites"
        );
        let full = id == "hud" || crate::panels::pregame::IDS.contains(&id);
        let regions = crate::panels::hud::regions_stretched(
            self.size.0,
            self.size.1,
            context.classic.option_words[0] & 0x20_0000 != 0,
        );
        crate::panels::set_side_height(regions.right.h as u32);
        panel.resize(
            if full {
                self.size.0
            } else if wide {
                self.bottom_width()
            } else {
                300
            },
            if full {
                self.size.1
            } else {
                regions.right.h as u32
            },
        );
        let frame = panel.frame(context);
        let (x, y) = if id == "help-chargen" {
            (2, 26)
        } else if id == "help-game" {
            (self.size.0 as i32 - 300, self.size.1 as i32 - 452)
        } else if wide {
            (0, regions.bottom.y - frame.screen.height as i32)
        } else if frame.screen.width >= self.size.0 {
            (0, 0)
        } else {
            (
                (self.size.0 - frame.screen.width.min(self.size.0)) as i32,
                regions.right.y,
            )
        };
        let token = self.next;
        self.next += 1;
        let mut controls = ControlHost::default();
        controls.sync(&frame);
        self.windows.push(Window {
            token,
            key: id.into(),
            panel,
            controls,
            frame,
            x,
            y,
        });
        self.focus = Some(token);
        self.release_inactive(context);
        self.dispatch(token, ControlEvent::Tick, context);
        self.fit_bottom_windows(context);
        Some(token)
    }
    /// The width of a window docked under the 3D view: the view's, which is the whole window
    /// without a side panel page and stops at the side column with one.
    fn bottom_width(&self) -> u32 {
        if self.windows.iter().any(|w| right_pane(&w.key)) {
            self.size.0.saturating_sub(309)
        } else {
            self.size.0
        }
    }
    /// Re-lay out the windows docked under the 3D view when the side panel opens or closes.
    fn fit_bottom_windows(&mut self, context: &Context<'_>) {
        let width = self.bottom_width();
        let regions =
            crate::panels::hud::regions_stretched(self.size.0, self.size.1, self.stretched);
        for w in &mut self.windows {
            if !bottom_window(&w.key) || w.frame.screen.width == width {
                continue;
            }
            w.panel.resize(width, regions.right.h as u32);
            w.frame = w.panel.frame(context);
            w.controls.sync(&w.frame);
            w.x = 0;
            w.y = regions.bottom.y - w.frame.screen.height as i32;
        }
    }
    pub fn set_position(&mut self, id: &str, x: i32, y: i32) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.key == id) {
            w.x = x;
            w.y = y;
        }
    }
    pub fn resize(&mut self, size: (u32, u32), context: &Context<'_>) {
        self.stretched = context.classic.option_words[0] & 0x20_0000 != 0;
        self.size = size;
        let bottom_width = self.bottom_width();
        crate::panels::set_side_height(
            crate::panels::hud::regions_stretched(size.0, size.1, self.stretched)
                .right
                .h as u32,
        );
        for w in &mut self.windows {
            let r = crate::panels::hud::regions_stretched(
                size.0,
                size.1,
                context.classic.option_words[0] & 0x20_0000 != 0,
            );
            let full = w.key == "hud" || crate::panels::pregame::IDS.contains(&w.key.as_str());
            let bottom = bottom_window(&w.key);
            w.panel.resize(
                if full {
                    size.0
                } else if bottom {
                    bottom_width
                } else {
                    300
                },
                if full { size.1 } else { r.right.h as u32 },
            );
            w.frame = w.panel.frame(context);
            (w.x, w.y) = if w.key == "help-chargen" {
                (2, 26)
            } else if w.key == "help-game" {
                (size.0 as i32 - 300, size.1 as i32 - 452)
            } else if full {
                (0, 0)
            } else if bottom {
                (0, r.bottom.y - w.frame.screen.height as i32)
            } else {
                (r.right.x, r.right.y)
            };
        }
        self.refresh(context);
    }
    /// The pointer's button came up outside every window: a window that was holding the press
    /// lets it go.
    pub fn release_pointer(&mut self, context: &Context<'_>) {
        if let Some(token) = self.capture.take() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.token == token) {
                w.controls.handle(Input::Cancel);
            }
            self.refresh(context);
        }
    }
    pub fn pointer_over_panel(&self, x: i32, y: i32) -> bool {
        let visible = self.visible_tokens();
        // A notice over the side column covers only its own rectangle; any other dialog covers
        // the whole window.
        let modal = self.modal.as_ref().is_some_and(|m| {
            m.placement != ModalPlacement::Hud || {
                let (w, h) = (self.size.0 as i32, self.size.1 as i32);
                let (top, height) = if self.stretched {
                    (53, h - 143)
                } else {
                    (h - 427, 337)
                };
                rect(w - 300, top, 300, height).contains(x, y)
            }
        });
        modal
            || self.windows.iter().rev().any(|w| {
                if !visible.contains(&w.token) {
                    return false;
                }
                if w.key == "hud" {
                    return w.controls.contains_control(x - w.x, y - w.y);
                }
                rect(
                    w.x,
                    w.y,
                    w.frame.screen.width as i32,
                    w.frame.screen.height as i32,
                )
                .contains(x, y)
            })
    }
    pub fn pointer_art(&self, x: i32, y: i32) -> Option<Command> {
        if self.modal.is_some() {
            return None;
        }
        let visible = self.visible_tokens();
        let w = self.windows.iter().rev().find(|w| {
            visible.contains(&w.token)
                && rect(
                    w.x,
                    w.y,
                    w.frame.screen.width as i32,
                    w.frame.screen.height as i32,
                )
                .contains(x, y)
        })?;
        let (did, width, height) = w.panel.pointer_art(x - w.x, y - w.y)?;
        Some(Command::Image {
            did,
            x,
            y,
            width,
            height,
            clip: None,
            color_key: None,
            key_bits: None,
            tile: false,
        })
    }
    pub fn tick(&mut self, context: &Context<'_>) {
        let tokens: Vec<_> = self.windows.iter().map(|w| w.token).collect();
        for token in tokens {
            self.dispatch(token, ControlEvent::Tick, context);
        }
        self.refresh(context);
    }
    pub fn tick_controls(&mut self, now: f64, context: &Context<'_>) {
        let visible = self.visible_tokens();
        let mut events = vec![];
        for window in &mut self.windows {
            if visible.contains(&window.token) {
                events.extend(
                    window
                        .controls
                        .tick(now)
                        .into_iter()
                        .map(|event| (window.token, event)),
                );
            }
        }
        if let Some(modal) = &mut self.modal {
            modal.controls.tick(now);
        }
        for (token, event) in events {
            self.dispatch(token, event, context);
        }
    }
    pub fn dispatch(&mut self, token: u64, event: ControlEvent, context: &Context<'_>) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.token == token) {
            let actions = w.panel.event(event, context);
            self.apply(token, actions, context);
        }
    }
    pub fn apply(&mut self, origin: u64, actions: Vec<PanelAction>, context: &Context<'_>) {
        for action in actions {
            match action {
                PanelAction::Toggle(id) => {
                    if self.is_visible(&id) {
                        self.close(&id, context);
                    } else {
                        self.open(&id, context);
                    }
                }
                PanelAction::BeginDrag(payload) => self.drag_payload = Some(payload),
                PanelAction::Game(request) => self.requests.push(request),
                PanelAction::Host(request) => {
                    self.host_actions.push(request);
                    self.host_origins.push(origin);
                }
                PanelAction::Control(event) => self.dispatch(origin, event, context),
                PanelAction::Open(id) => {
                    self.open(&id, context);
                }
                PanelAction::OpenObject { id, object } => {
                    // Examining an object asks the server for its appraisal; the panel shows
                    // "Receiving information..." until the answer arrives.
                    if id == "examine" && object.0 != 0 {
                        self.requests.push(UiRequest::Examine(object));
                    }
                    self.open_object(&id, object, context);
                }
                PanelAction::OpenSpell { id, spell } => {
                    if let Some(token) = self.open(&id, context) {
                        if let Some(w) = self.windows.iter_mut().find(|w| w.token == token) {
                            w.panel.set_spell(spell);
                        }
                    }
                }
                PanelAction::Close => {
                    let pane = self.shown_pane() == Some(origin);
                    let covered = self.spell_examination_over_spellbook(origin);
                    self.deactivate_token(origin, context);
                    self.windows.retain(|w| w.token != origin);
                    self.focus = self.windows.last().map(|w| w.token);
                    if pane && !covered {
                        self.close_retained_panes(context);
                    }
                    self.fit_bottom_windows(context);
                }
                PanelAction::Question {
                    id,
                    text,
                    accept,
                    reject,
                } => {
                    self.apply(
                        origin,
                        vec![PanelAction::Confirm {
                            id: id.clone(),
                            text,
                            accept,
                        }],
                        context,
                    );
                    if let Some(dialog) = self
                        .modal
                        .as_mut()
                        .filter(|m| m.id == id)
                        .or_else(|| self.dialog_queue.iter_mut().find(|m| m.id == id))
                    {
                        dialog.reject = reject;
                    }
                }
                PanelAction::Message { id, text, accept } => {
                    self.apply(
                        origin,
                        vec![PanelAction::Confirm {
                            id: id.clone(),
                            text,
                            accept,
                        }],
                        context,
                    );
                    self.set_dialog_labels(&id, "OK".into(), None);
                    self.set_dialog_default(&id, true);
                }
                PanelAction::Confirm { id, text, accept } => {
                    self.show_dialog(id.clone(), text, accept, vec![]);
                    if let Some(accept) = self
                        .windows
                        .iter()
                        .find(|w| w.token == origin)
                        .and_then(|w| Self::pregame_dialog_default(w.panel.id()))
                    {
                        self.set_dialog_default(&id, accept);
                    }
                    if let Some(placement) = self
                        .windows
                        .iter()
                        .find(|w| w.token == origin)
                        .and_then(|w| ModalPlacement::for_panel(w.panel.id()))
                    {
                        self.set_dialog_placement(&id, placement);
                    }
                    if let Some(modal) = self.modal.as_mut().filter(|m| m.id == id) {
                        modal.origin = origin;
                    }
                    for modal in self.dialog_queue.iter_mut().filter(|m| m.id == id) {
                        modal.origin = origin;
                    }
                }
            }
        }
    }
    fn refresh(&mut self, context: &Context<'_>) {
        self.release_inactive(context);
        for w in &mut self.windows {
            w.frame = w.panel.frame(context);
            w.controls.sync(&w.frame);
        }
    }
    fn modal_frame(size: (u32, u32), stretched: bool, modal: &Modal) -> PanelFrame {
        let mut f = PanelFrame::new(size.0, size.1);
        let (x, y, w, h) = match modal.placement {
            ModalPlacement::Hud => (
                size.0 as i32 - 300,
                if stretched { 53 } else { size.1 as i32 - 427 },
                300,
                if stretched { size.1 as i32 - 143 } else { 337 },
            ),
            ModalPlacement::Centered => (
                (size.0 as i32 - 403) / 2,
                (size.1 as i32 - 253) / 2,
                403,
                253,
            ),
            ModalPlacement::Fixed { x, y } => (x, y, 403, 253),
        };
        let hud = modal.placement == ModalPlacement::Hud;
        f.image(
            if hud { "0600128A" } else { "06000523" },
            rect(x, y, w, h),
            hud,
            false,
        );
        f.text_box(
            rect(x + 20, y + 20, w - 40, h - 100),
            &modal.text,
            "times-25-11",
            0xfffafaf0,
            TextAlign::Center,
            true,
            None,
        );
        f.button(
            "accept",
            rect(
                x + if modal.reject_label.is_some() {
                    30
                } else {
                    w / 2 - 50
                },
                y + h - 50,
                100,
                36,
            ),
            &modal.accept_label,
            true,
        );
        if let Some(label) = &modal.reject_label {
            f.button(
                "cancel",
                rect(x + w - 130, y + h - 50, 100, 36),
                label,
                true,
            );
        }
        f
    }
    pub fn input(&mut self, input: Input, context: &Context<'_>) {
        self.release_inactive(context);
        let visible = self.visible_tokens();
        if let Some(modal) = &mut self.modal {
            let answer = match &input {
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    ..
                } => Some(modal.default_accept || modal.reject_label.is_none()),
                Input::Key {
                    key: crate::widgets::Key::Escape,
                    ..
                } => Some(false),
                Input::Text(text) if matches!(text.as_str(), "y" | "Y" | "o" | "O") => Some(true),
                Input::Text(text) if matches!(text.as_str(), "c" | "C" | "n" | "N") => Some(false),
                _ => None,
            };
            if let Some(accept) = answer {
                if accept || modal.reject_label.is_some() {
                    self.answer_modal(accept, context);
                }
                return;
            }
            let frame = Self::modal_frame(self.size, self.stretched, modal);
            modal.controls.sync(&frame);
            if !modal.controls.has_focus() {
                modal.controls.focus_control(
                    if modal.default_accept || modal.reject_label.is_none() {
                        "accept"
                    } else {
                        "cancel"
                    },
                );
            }
            let events = modal.controls.handle(input);
            self.sounds.extend(modal.controls.take_sounds());
            if events
                .iter()
                .any(|e| matches!(e,ControlEvent::Activate(id) if id=="accept"))
            {
                self.answer_modal(true, context);
            } else if events
                .iter()
                .any(|e| matches!(e,ControlEvent::Activate(id) if id=="cancel"))
            {
                self.answer_modal(false, context);
            }
            self.refresh(context);
            return;
        }
        let pointer = match input {
            Input::RightClick { x, y } => Some((x, y)),
            Input::PointerMove { x, y }
            | Input::PointerDown { x, y }
            | Input::PointerUp { x, y }
            | Input::Wheel { x, y, .. } => Some((x, y)),
            _ => None,
        };
        if let Input::PointerUp { x, y } = input {
            if let Some(payload) = self.drag_payload.take() {
                let drop = self.windows.iter().rev().find_map(|w| {
                    if !visible.contains(&w.token) {
                        return None;
                    }
                    w.controls
                        .drop_event(x - w.x, y - w.y, payload.clone())
                        .map(|mut e| {
                            if let ControlEvent::Drop { id, payload, .. } = &mut e {
                                if w.key != "shortcuts"
                                    && !(w.key == "hud" && id.starts_with("shortcut:"))
                                {
                                    if let DragPayload::Shortcut { object, .. } = payload {
                                        *payload = DragPayload::Object(*object);
                                    }
                                }
                            }
                            if matches!(w.key.as_str(), "trade" | "maintenance") {
                                if let ControlEvent::Drop {
                                    id,
                                    payload: DragPayload::Object(object),
                                    slot,
                                } = &e
                                {
                                    if context.game.selected_object() == Some(*object) {
                                        if let Some((amount, max_amount)) =
                                            context.classic.stack_split
                                        {
                                            e = ControlEvent::DropStack {
                                                id: id.clone(),
                                                object: *object,
                                                amount,
                                                max_amount,
                                                slot: *slot,
                                            };
                                        }
                                    }
                                }
                            }
                            (w.token, e)
                        })
                });
                for w in &mut self.windows {
                    w.controls.handle(Input::Cancel);
                }
                self.capture = None;
                if let Some((token, event)) = drop {
                    self.dispatch(token, event, context);
                } else if let DragPayload::Object(item) = payload {
                    self.requests.push(UiRequest::DragDrop {
                        item,
                        target: dereth_client_contract::view::DropTarget::World,
                    });
                }
                self.refresh(context);
                return;
            }
        }
        if matches!(
            input,
            Input::Cancel
                | Input::Key {
                    key: crate::widgets::Key::Escape,
                    ..
                }
        ) {
            self.drag_payload = None;
        }
        let target = self
            .capture
            .or_else(|| {
                self.windows
                    .iter()
                    .rev()
                    .find(|w| visible.contains(&w.token) && w.controls.popup_open())
                    .map(|w| w.token)
            })
            .or_else(|| {
                pointer.and_then(|(x, y)| {
                    self.windows
                        .iter()
                        .rev()
                        .find(|w| {
                            if !visible.contains(&w.token) {
                                return false;
                            }
                            if w.key == "hud" {
                                return w.controls.contains_control(x - w.x, y - w.y)
                                    || w.panel.claims(x - w.x, y - w.y);
                            }
                            rect(
                                w.x,
                                w.y,
                                w.frame.screen.width as i32,
                                w.frame.screen.height as i32,
                            )
                            .contains(x, y)
                        })
                        .map(|w| w.token)
                })
            })
            .or(self.focus);
        if let Some(token) = target {
            if matches!(input, Input::PointerDown { .. }) {
                self.capture = Some(token);
                self.focus = Some(token);
                self.release_inactive(context);
            }
            if matches!(input, Input::PointerUp { .. } | Input::Cancel) {
                self.capture = None;
            }
            let panel_actions = self
                .windows
                .iter_mut()
                .find(|w| w.token == token)
                .and_then(|w| {
                    w.panel
                        .input(&translate_input(input.clone(), -w.x, -w.y), context)
                });
            if let Some(actions) = panel_actions {
                self.apply(token, actions, context);
                self.refresh(context);
                return;
            }
            if let Some(w) = self.windows.iter_mut().find(|w| w.token == token) {
                let local = translate_input(input, -w.x, -w.y);
                let mut events = w.controls.handle(local.clone());
                self.sounds.extend(w.controls.take_sounds());
                match local {
                    Input::PointerMove { x, y }
                    | Input::PointerDown { x, y }
                    | Input::PointerUp { x, y } => events.insert(
                        0,
                        ControlEvent::Pointer {
                            x,
                            y,
                            pressed: matches!(local, Input::PointerDown { .. }),
                        },
                    ),
                    Input::Key {
                        key: crate::widgets::Key::Escape,
                        ..
                    } => events.push(ControlEvent::Activate("close".into())),
                    Input::Key { .. } | Input::Text(_) => {
                        events.insert(0, ControlEvent::KeyPressed)
                    }
                    _ => {}
                }
                for event in events {
                    self.dispatch(token, event, context);
                }
            }
        }
        self.refresh(context);
    }
    pub fn screen(&mut self) -> Screen {
        let visible = self.visible_tokens();
        let mut screen = Screen {
            width: self.size.0,
            height: self.size.1,
            commands: vec![],
        };
        self.previews.clear();
        self.preview_owners.clear();
        for w in &self.windows {
            if !visible.contains(&w.token) {
                continue;
            }
            for mut command in w.controls.draw(&w.frame).commands {
                if w.key != "hud"
                    && !clip_command(
                        &mut command,
                        rect(
                            0,
                            0,
                            w.frame.screen.width as i32,
                            w.frame.screen.height as i32,
                        ),
                    )
                {
                    continue;
                }
                if let Command::Preview { index } = &mut command {
                    *index += self.previews.len();
                }
                screen.commands.push(translate_command(command, w.x, w.y));
            }
            for mut preview in w.frame.previews.clone() {
                preview.rect.x += w.x;
                preview.rect.y += w.y;
                self.previews.push(preview);
                self.preview_owners.push(w.token);
            }
        }
        if let Some(modal) = &mut self.modal {
            let f = Self::modal_frame(self.size, self.stretched, modal);
            modal.controls.sync(&f);
            screen.commands.extend(modal.controls.draw(&f).commands);
        }
        screen
    }
}
fn right_pane(id: &str) -> bool {
    !crate::panels::pregame::IDS.contains(&id)
        && !matches!(
            id,
            "hud"
                | "chat"
                | "radar"
                | "vitals"
                | "shortcuts"
                | "trade"
                | "vendor"
                | "external-container"
                | "salvage"
                | "maintenance"
                | "combat"
                | "spell-favorites"
        )
}
fn clip_command(command: &mut Command, bounds: crate::widgets::Rect) -> bool {
    match command {
        Command::Preview { .. } => {}
        Command::Fill {
            x,
            y,
            width,
            height,
            ..
        } => {
            let Some(r) = rect(*x, *y, *width as i32, *height as i32).intersect(bounds) else {
                return false;
            };
            *x = r.x;
            *y = r.y;
            *width = r.w as u32;
            *height = r.h as u32;
        }
        Command::TextBox { clip, .. }
        | Command::Invert { clip, .. }
        | Command::RichTextBox { clip, .. }
        | Command::Image { clip, .. }
        | Command::IndexedImage { clip, .. }
        | Command::Text { clip, .. }
        | Command::SpellIcon { clip, .. }
        | Command::ItemIcon { clip, .. } => {
            let prior = clip
                .map(|c| rect(c[0], c[1], c[2] - c[0], c[3] - c[1]))
                .unwrap_or(bounds);
            let Some(r) = prior.intersect(bounds) else {
                return false;
            };
            *clip = Some([r.x, r.y, r.x + r.w, r.y + r.h]);
        }
    }
    true
}
fn translate_input(input: Input, dx: i32, dy: i32) -> Input {
    match input {
        Input::RightClick { x, y } => Input::RightClick {
            x: x + dx,
            y: y + dy,
        },
        Input::PointerMove { x, y } => Input::PointerMove {
            x: x + dx,
            y: y + dy,
        },
        Input::PointerDown { x, y } => Input::PointerDown {
            x: x + dx,
            y: y + dy,
        },
        Input::PointerUp { x, y } => Input::PointerUp {
            x: x + dx,
            y: y + dy,
        },
        Input::Wheel { x, y, delta } => Input::Wheel {
            x: x + dx,
            y: y + dy,
            delta,
        },
        other => other,
    }
}
pub fn translate_command(mut c: Command, dx: i32, dy: i32) -> Command {
    let offset_clip = |clip: &mut Option<[i32; 4]>| {
        if let Some(c) = clip {
            c[0] += dx;
            c[1] += dy;
            c[2] += dx;
            c[3] += dy;
        }
    };
    match &mut c {
        Command::Preview { .. } => {}
        Command::TextBox { rect, clip, .. }
        | Command::RichTextBox { rect, clip, .. }
        | Command::Invert { rect, clip } => {
            rect[0] += dx;
            rect[1] += dy;
            offset_clip(clip);
        }
        Command::Image { x, y, clip, .. }
        | Command::IndexedImage { x, y, clip, .. }
        | Command::Text { x, y, clip, .. }
        | Command::SpellIcon { x, y, clip, .. }
        | Command::ItemIcon { x, y, clip, .. } => {
            *x += dx;
            *y += dy;
            offset_clip(clip);
        }
        Command::Fill { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
    }
    c
}

fn panels_world(size: (u32, u32)) -> crate::widgets::Rect {
    crate::panels::hud::regions(size.0, size.1).world
}

/// The windows docked under the 3D view, the width of the view.
fn bottom_window(key: &str) -> bool {
    matches!(
        key,
        "trade"
            | "vendor"
            | "external-container"
            | "salvage"
            | "maintenance"
            | "combat"
            | "spell-favorites"
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[derive(Debug)]
    struct StatefulPane(u32);
    impl Panel for StatefulPane {
        fn id(&self) -> &'static str {
            "test-pane"
        }
        fn frame(&self, _: &Context<'_>) -> PanelFrame {
            let mut frame = PanelFrame::new(300, 100);
            frame.button("increment", rect(0, 0, 100, 20), self.0.to_string(), true);
            frame
        }
        fn event(&mut self, event: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
            if matches!(event, ControlEvent::Activate(id) if id == "increment") {
                self.0 += 1;
            }
            vec![]
        }
    }
    #[test]
    fn switching_side_panels_retains_state_and_restores_the_existing_instance() {
        context_test(|c| {
            let mut desktop = Desktop::new(|_| Some(Box::new(StatefulPane(0))), (800, 600));
            let first = desktop.open("first", c).unwrap();
            desktop.dispatch(first, ControlEvent::Activate("increment".into()), c);
            desktop.open("second", c);
            assert!(desktop.is_open("first"));
            assert!(!desktop.is_visible("first"));
            assert!(desktop.is_visible("second"));
            assert_eq!(desktop.open("first", c), Some(first));
            assert_eq!(desktop.windows.len(), 2);
            desktop.refresh(c);
            assert!(
                matches!(&desktop.windows.last().unwrap().frame.controls[0].kind, ControlKind::Button { caption } if caption == "1")
            );
            assert!(!desktop.is_visible("second"));
        });
    }
    #[test]
    fn closing_the_shown_page_closes_the_side_panel_instead_of_uncovering_an_older_page() {
        context_test(|c| {
            let mut desktop = Desktop::new(|_| Some(Box::new(StatefulPane(0))), (800, 600));
            desktop.open("first", c);
            desktop.open("second", c);
            desktop.close("second", c);
            assert!(!desktop.is_visible("first"));
            assert!(!desktop.is_open("first"));
            assert!(desktop.active_regions().0.is_empty());
        });
    }
    #[test]
    fn closing_a_spells_examination_brings_the_spellbook_back() {
        context_test(|c| {
            let mut d = Desktop::new(crate::panels::factory, (800, 600));
            let book = d.open("spellbook", c).unwrap();
            d.apply(
                book,
                vec![PanelAction::OpenSpell {
                    id: "examine-spell".into(),
                    spell: 1,
                }],
                c,
            );
            assert!(d.is_visible("examine-spell") && !d.is_visible("spellbook"));
            let token = d
                .windows
                .iter()
                .find(|w| w.key == "examine-spell")
                .unwrap()
                .token;
            d.dispatch(token, ControlEvent::Activate("close".into()), c);
            assert!(d.is_visible("spellbook"));
            assert!(!d.is_open("examine-spell"));
            // Over any other page the examination's close still closes the side panel.
            d.open("inventory", c);
            let inv = d
                .windows
                .iter()
                .find(|w| w.key == "inventory")
                .unwrap()
                .token;
            d.apply(
                inv,
                vec![PanelAction::OpenSpell {
                    id: "examine-spell".into(),
                    spell: 1,
                }],
                c,
            );
            let token = d
                .windows
                .iter()
                .find(|w| w.key == "examine-spell")
                .unwrap()
                .token;
            d.dispatch(token, ControlEvent::Activate("close".into()), c);
            assert!(d.active_regions().0.is_empty());
        });
    }
    #[test]
    fn a_toolbar_page_button_closes_its_page_when_it_is_on_show() {
        context_test(|c| {
            for id in [
                "social",
                "spellbook",
                "character-stats",
                "map",
                "options",
                "inventory",
            ] {
                let mut d = Desktop::new(crate::panels::factory, (800, 600));
                d.apply(0, vec![PanelAction::Toggle(id.into())], c);
                assert!(d.is_visible(id), "{id} opens");
                d.apply(0, vec![PanelAction::Toggle(id.into())], c);
                assert!(!d.is_visible(id), "{id} closes");
                assert!(d.active_regions().0.is_empty(), "{id}");
            }
        });
    }
    #[test]
    fn clicking_the_toolbar_button_of_the_page_on_show_closes_it() {
        context_test(|c| {
            let mut d = Desktop::new(crate::panels::factory, (800, 600));
            d.open("hud", c);
            let r = crate::panels::hud::regions(800, 600);
            let (x, y) = (r.toolbar.x + 61 + 10, r.toolbar.y + 10);
            for open in [true, false] {
                d.input(Input::PointerDown { x, y }, c);
                d.input(Input::PointerUp { x, y }, c);
                assert_eq!(d.is_visible("social"), open);
            }
        });
    }
    #[test]
    fn the_combat_bar_spans_the_3d_view_with_or_without_a_side_page() {
        context_test(|c| {
            let mut d = Desktop::new(crate::panels::factory, (800, 600));
            d.open("combat", c);
            let bar = |d: &Desktop| {
                let w = d.windows.iter().find(|w| w.key == "combat").unwrap();
                (w.x, w.y, w.frame.screen.width, w.frame.screen.height)
            };
            assert_eq!(bar(&d), (0, 435, 800, 64));
            d.open("inventory", c);
            assert_eq!(bar(&d), (0, 435, 491, 64));
            d.close("inventory", c);
            assert_eq!(bar(&d).2, 800);
            // Closing the page with Escape (its own close) widens the bar again too.
            d.open("inventory", c);
            assert_eq!(bar(&d).2, 491);
            d.input(
                Input::Key {
                    key: crate::widgets::Key::Escape,
                    shift: false,
                },
                c,
            );
            assert!(!d.is_visible("inventory"));
            assert_eq!(bar(&d).2, 800);
        });
    }
    #[test]
    fn a_side_column_notice_leaves_the_world_clickable() {
        let mut d = Desktop::new(|_| None, (800, 600));
        d.show_dialog("notice".into(), "Notice".into(), vec![], vec![]);
        assert!(
            d.pointer_over_panel(100, 100),
            "a centred dialog covers everything"
        );
        d.set_dialog_placement("notice", ModalPlacement::Hud);
        assert!(!d.pointer_over_panel(100, 100));
        assert!(d.pointer_over_panel(700, 300));
    }
    #[derive(Debug)]
    struct EditingPane;
    impl Panel for EditingPane {
        fn id(&self) -> &'static str {
            "editing-pane"
        }
        fn frame(&self, _: &Context<'_>) -> PanelFrame {
            let mut frame = PanelFrame::new(300, 100);
            frame.edit("draft", rect(0, 0, 100, 20), "draft", 100, true, true);
            frame
        }
        fn event(&mut self, event: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
            match event {
                ControlEvent::Commit { .. } => {
                    vec![PanelAction::Host(HostAction::ConfirmBinding(true))]
                }
                ControlEvent::Activate(id) if id == "close" => {
                    vec![PanelAction::Host(HostAction::ConfirmBinding(false))]
                }
                _ => vec![],
            }
        }
    }
    #[test]
    fn hiding_retained_pane_commits_once_and_restoring_does_not_restore_edit_focus() {
        context_test(|c| {
            let mut d = Desktop::new(|_| Some(Box::new(EditingPane)), (800, 600));
            let first = d.open("first", c).unwrap();
            assert!(d
                .windows
                .iter_mut()
                .find(|w| w.token == first)
                .unwrap()
                .controls
                .focus_control("draft"));
            d.open("second", c);
            assert_eq!(d.host_actions, [HostAction::ConfirmBinding(true)]);
            assert!(d.is_open("first"));
            assert!(!d.is_visible("first"));
            d.refresh(c);
            assert_eq!(d.host_actions.len(), 1);
            d.open("first", c);
            assert!(!d.editing());
        });
    }
    #[test]
    fn visual_teardown_commits_before_removal_without_user_close_action() {
        context_test(|c| {
            let mut d = Desktop::new(|_| Some(Box::new(EditingPane)), (800, 600));
            let token = d.open("first", c).unwrap();
            assert!(d
                .windows
                .iter_mut()
                .find(|w| w.token == token)
                .unwrap()
                .controls
                .focus_control("draft"));
            d.remove_visual("first", c);
            assert_eq!(d.host_actions, [HostAction::ConfirmBinding(true)]);
            assert!(!d.is_open("first"));
            assert_eq!(d.focus, None);
        });
    }
    #[test]
    fn service_ground_teardown_precedes_peace_without_external_use() {
        context_test(|c| {
            let mut d = Desktop::new(|_| Some(Box::new(StatefulPane(0))), (800, 600));
            d.open("external-container", c);
            d.open("salvage", c);
            assert_eq!(
                d.host_actions,
                [HostAction::CloseGroundForced, HostAction::CombatMode(1)]
            );
            assert!(d.requests.is_empty());
            assert!(!d.is_open("external-container"));
        });
    }
    #[derive(Debug)]
    struct World;
    impl GameView for World {}
    fn context_test(f: impl FnOnce(&Context<'_>)) {
        let game = World;
        let pregame = PregameView::default();
        let keyboard = KeyboardState::default();
        let settings = ClassicSettings::default();
        let classic = ClassicState::default();
        f(&Context {
            game: &game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        });
    }
    #[test]
    fn replacing_a_visible_dialog_answers_only_the_latest_context_and_keeps_queue() {
        context_test(|c| {
            let mut d = Desktop::new(|_| None, (800, 600));
            let answer = |n| vec![PanelAction::Host(HostAction::CombatMode(n))];
            d.show_dialog("server".into(), "First".into(), answer(1), answer(2));
            d.show_dialog("next".into(), "Next".into(), answer(4), vec![]);
            d.show_dialog("server".into(), "Replaced".into(), answer(8), answer(9));
            assert_eq!(d.modal.as_ref().unwrap().text, "Replaced");
            d.answer_modal(false, c);
            assert_eq!(d.host_actions, vec![HostAction::CombatMode(9)]);
            assert_eq!(d.modal.as_ref().unwrap().id, "next");
            d.dismiss_dialog("next");
            assert!(!d.modal_open());
            assert_eq!(d.host_actions.len(), 1);
        });
    }
    #[test]
    fn enter_uses_dialog_default_and_single_button_popup_acknowledges() {
        context_test(|c| {
            let mut d = Desktop::new(|_| None, (800, 600));
            let answer = |yes| vec![PanelAction::Host(HostAction::ConfirmBinding(yes))];
            d.show_dialog(
                "confirm".into(),
                "Question".into(),
                answer(true),
                answer(false),
            );
            d.input(
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    shift: false,
                },
                c,
            );
            assert_eq!(d.host_actions, vec![HostAction::ConfirmBinding(false)]);
            d.show_dialog("popup".into(), "Information".into(), answer(true), vec![]);
            d.set_dialog_labels("popup", "OK".into(), None);
            d.input(
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    shift: false,
                },
                c,
            );
            assert_eq!(
                d.host_actions.last(),
                Some(&HostAction::ConfirmBinding(true))
            );
        });
    }
    #[test]
    fn window_teardown_cancels_unanswered_dialogs_without_sending_refusals() {
        context_test(|c| {
            let mut d = Desktop::new(|_| None, (800, 600));
            d.show_dialog(
                "pending".into(),
                "Question".into(),
                vec![],
                vec![PanelAction::Host(HostAction::ConfirmBinding(false))],
            );
            d.close_all(c);
            assert!(d.host_actions.is_empty());
            assert!(!d.modal_open());
        });
    }
    #[test]
    fn fixed_pregame_modal_placement_survives_window_resize_and_queueing() {
        let mut d = Desktop::new(|_| None, (1024, 768));
        d.show_dialog("login".into(), "Login".into(), vec![], vec![]);
        d.show_dialog("create".into(), "Create".into(), vec![], vec![]);
        d.set_dialog_placement("login", ModalPlacement::for_panel("login").unwrap());
        d.set_dialog_placement(
            "create",
            ModalPlacement::for_panel("create-skills").unwrap(),
        );
        let frame = Desktop::modal_frame(d.size, false, d.modal.as_ref().unwrap());
        assert!(
            matches!(&frame.screen.commands[0], Command::Image { did, x:199, y:250, width:403, height:253, tile:false, .. } if did == "06000523")
        );
        let r = d.modal_button_rect(true).unwrap();
        assert_eq!((r.x, r.y), (229, 453));
        d.dismiss_dialog("login");
        let r = d.modal_button_rect(false).unwrap();
        assert_eq!((r.x, r.y), (628, 377));
        assert_eq!(
            ModalPlacement::for_panel("keyboard"),
            Some(ModalPlacement::Fixed { x: 199, y: 250 })
        );
        assert_eq!(ModalPlacement::for_panel("skills"), None);
    }
    #[test]
    fn modal_hotkeys_use_button_presence_and_explicit_default_independent_of_focus() {
        context_test(|c| {
            let mut d = Desktop::new(|_| None, (800, 600));
            let choice = |yes| vec![PanelAction::Host(HostAction::ConfirmBinding(yes))];
            for (key, expected) in [("y", true), ("O", true), ("c", false), ("N", false)] {
                d.show_dialog(
                    "choice".into(),
                    "Question".into(),
                    choice(true),
                    choice(false),
                );
                d.input(Input::Text(key.into()), c);
                assert_eq!(
                    d.host_actions.pop(),
                    Some(HostAction::ConfirmBinding(expected))
                );
                assert!(!d.modal_open());
            }
            d.show_dialog(
                "creation".into(),
                "Continue?".into(),
                choice(true),
                choice(false),
            );
            d.set_dialog_default(
                "creation",
                Desktop::pregame_dialog_default("create-attributes").unwrap(),
            );
            d.input(
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    shift: false,
                },
                c,
            );
            assert_eq!(d.host_actions.pop(), Some(HostAction::ConfirmBinding(true)));
            d.show_dialog(
                "info".into(),
                "Information".into(),
                choice(true),
                choice(false),
            );
            d.set_dialog_labels("info", "OK".into(), None);
            d.input(
                Input::Key {
                    key: crate::widgets::Key::Escape,
                    shift: false,
                },
                c,
            );
            d.input(Input::Text("n".into()), c);
            assert!(d.modal_open());
            assert!(d.host_actions.is_empty());
            d.input(Input::Text("o".into()), c);
            assert_eq!(d.host_actions.pop(), Some(HostAction::ConfirmBinding(true)));
            assert_eq!(Desktop::pregame_dialog_default("keyboard"), Some(true));
            assert_eq!(Desktop::pregame_dialog_default("login"), Some(false));
        });
    }
}
