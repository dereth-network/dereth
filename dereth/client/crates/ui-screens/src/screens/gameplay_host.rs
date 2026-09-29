//! The gameplay screen's host calls: what the host delivers to [`GamePlayScreen`] and what it
//! gets back, through [`dereth_ui::framework::Screen::on_game`].
//!
//! The host (the application's frame, the HUD's per-frame drive, the request owners' tails and
//! the notice subscribers) does not reach the concrete screen. It builds one [`GameCall`] at the
//! point of the frame where it needs the screen and hands it over; the screen does the work
//! against `cx.view` (the game as the host sees it at that point) and writes its answers back
//! into the call. Nothing here reorders anything: each call is made at the point in the frame
//! where the screen's work belongs.
//!
//! The HUD's panel set ([`RemainingPanels`]) is owned by the host across screen rebuilds, exactly
//! as before; the calls that drive it borrow it through [`dereth_ui::framework::GameCx::lent`].

use std::rc::Rc;

use dereth_client_contract::panels::external_container::ExternalContainerNotice;
use dereth_client_contract::panels::salvage::SalvageNotice;
use dereth_primitives::ObjectId;
use dereth_ui::framework::{GameCx, ScreenCx};
use dereth_ui::persist::ScreenLayout;
use dereth_ui::{ElemHandle, UiSystem};

use crate::chat::interface::ChatMessage;
use crate::chat::mainchat::{AutoTargetWorld, TalkFocusChange};
use crate::chat::window::ReplyTargets;
use crate::panels::external_container;
use crate::panels::remaining::RemainingPanels;
use crate::panels::statmgmt::{self, child, Footer};
use crate::screens::chargen::{Cg3dView, CharGenTables};
use crate::screens::chargen_state::CharGenState;
use crate::screens::gameplay::{
    GamePlayScreen, PlayerSettingsView, SelectionQuery, ToolbarSelection,
};
use crate::view::GameView;

/// One host call. See the module documentation.
#[allow(clippy::large_enum_variant)] // one call at a time, handed straight to its handler
#[derive(Debug)]
pub enum GameCall {
    /// The HUD's per-frame drive, the screen half: every panel update, message delivery and drop
    /// delivery, in the order the drive has always made them. Lends the HUD's panels.
    Frame(Box<FrameCall>),
    /// A request owner's head: the panel messages and drops one input action raised, completed
    /// before the next action. Lends the HUD's panels.
    PanelInput(PanelInputCall),
    /// Selection and container notices' item-facing consumers: the toolbar selection, the
    /// inventory and the external container. Lends the HUD's panels.
    RefreshItemInputs,
    /// An object notice's inventory and toolbar subscribers, and the slumlord's move receiver.
    /// Lends the HUD's panels.
    ObjectNotice(ObjectNoticeCall),
    /// One talk-focus enable notice, delivered to the main chat's row. `talk_focus` is the
    /// communication system's current focus; `reset` comes back `true` when the host must put
    /// the focus back to *All*.
    ChatFocus {
        talk_focus: u32,
        focus: u32,
        enabled: bool,
        is_olthoi: bool,
        reset: bool,
    },
    /// Post-initialization of a freshly built gameplay screen's communication state; `change`
    /// is the focus write the host must queue, if any.
    InitCommunication {
        enabled: [bool; 14],
        is_olthoi: bool,
        change: Option<TalkFocusChange>,
    },
    /// The interaction mode and selection the item handlers read.
    InteractionContext {
        target_mode: bool,
        selected: Option<ObjectId>,
    },
    /// The UI-lock flag, pushed into the screen before the global `0x0D` broadcast.
    LockUi(bool),
    /// The start-tell notice for `name`, reaching the chat entry.
    StartTell(String),
    /// Place the screen's windows from a loaded layout; `placed` is how many were placed.
    LoadLayout { layout: ScreenLayout, placed: usize },
    /// The screen's windows as a layout.
    SaveLayout(Option<ScreenLayout>),
    /// The player's airborne state, which the log-off confirmation checks.
    PlayerAirborne(bool),
    /// The auto-target snapshot the chat entry's Tell-to-Selected reads.
    AutoTargetWorld(AutoTargetWorld),
    /// A chat window title notice.
    ChatWindowTitle {
        window_id: u32,
        title: crate::view::ChatWindowTitle,
    },
    /// The frame-rate display notice.
    FramerateDisplay {
        enabled: bool,
        values: Option<(f32, f32)>,
    },
    /// The frame-rate display's guarded update.
    FramerateUseTime { framerate: f32, degrade: f32 },
    /// A book went out of range.
    BookRangeExit(ObjectId),
    /// An item offered to the trade window from outside it.
    OfferTradeItem(ObjectId),
    /// The split-stack notice for the selected object; reads `cx.view`.
    SplitStack(ObjectId),
    /// The screen's root element.
    Root(Option<ElemHandle>),
    /// The paper doll's preview viewport element.
    PaperDollViewport(Option<ElemHandle>),
    /// The examination panel's creature preview: `(is the item pane up, the object shown, the
    /// viewport)`, each as the panel holds it.
    ExaminePreview(Option<(bool, Option<ObjectId>, Option<ElemHandle>)>),
    /// Whether the examination panel is open.
    ExaminationOpen(bool),
    /// Close the examination panel through its own visibility path; the answer is the panel's.
    CloseExamination(bool),
    /// Advance the barber panel's preview turntable by `dt` seconds.
    BarberTick(f64),
    /// The barber panel's preview: its view, state and tables.
    BarberPreview(Option<(Cg3dView, CharGenState, Option<Rc<CharGenTables>>)>),
    /// The key-bindings page. The input manager travels in [`GameCx::input`].
    KeyBindings(KeyBindingsCall),
}

/// [`GameCall::KeyBindings`]'s operations, one per call the host made on the page.
#[derive(Debug)]
pub enum KeyBindingsCall {
    /// Build the rows from the manager: `(rows, headers, failures)` comes back.
    InitOptions { out: (usize, usize, usize) },
    /// Re-read the rows from the manager after a keymap load.
    Reinit,
    /// Show the keymap file's name.
    RefreshFileName(Option<String>),
    /// Offer one diverted control to the page; `taken` comes back.
    KeyHit {
        control: dereth_input::ControlChord,
        taken: bool,
    },
    /// The per-frame drain of the page's element messages and key hits.
    Drive {
        events: Vec<(usize, crate::options::keybinding::RowEvent)>,
        verdicts: Vec<dereth_input::binding::Capture>,
    },
    /// The page's button events, taken.
    TakePageEvents(Vec<crate::options::keybinding::PageEvent>),
    /// Open the load dialog over `files`.
    OpenLoad {
        files: Vec<String>,
        current: Option<String>,
    },
    /// Open the save dialog.
    OpenSave,
    /// Open the overwrite confirmation.
    OpenOverwrite(String),
    /// Open the read-only notice.
    OpenReadOnly(String),
    /// Whether any row is capturing a key.
    Capturing(bool),
}

/// [`GameCall::Frame`]: what the HUD hands its drive, and what comes back.
#[derive(Debug, Default)]
pub struct FrameCall {
    /// The panels' bindings are for an older screen construction: bind them to this one's root.
    pub rebind: bool,
    /// External-container notices queued since the last drive.
    pub external_container: Vec<ExternalContainerNotice>,
    /// Salvage notices queued since the last drive.
    pub salvage: Vec<SalvageNotice>,
    /// The server answered a raise since the last drive.
    pub raise_answered: bool,
    /// The environment-page visibility edges, per page, as the host last delivered them. Updated
    /// in place.
    pub env_page_visibility: [Option<bool>; 4],
    /// The `+10` watch line last printed. Updated in place.
    pub trace_plus_ten_last: Option<String>,
    /// Whether the raise trace is on.
    pub trace_raise: bool,
    /// Whether the notice trace is on.
    pub trace_notice: bool,
    /// The player module's settings, when a new description has not been applied to this screen.
    pub player_settings: Option<PlayerSettingsView>,
    /// The three remembered names.
    pub reply_targets: ReplyTargets,
    /// The stay-in-chat-mode option bit.
    pub stay_in_chat_mode: bool,
    /// Whether the last speakable target is squelched.
    pub chat_target_squelched: bool,
    /// The auto-target snapshot.
    pub auto_target_world: AutoTargetWorld,
    /// Chat lines for this screen construction, in arrival order.
    pub chat: Vec<ChatMessage>,
    /// What the drive wrote. See [`FrameOut`].
    pub out: FrameOut,
}

/// The counters and answers [`GameCall::Frame`] writes back, one per HUD statistic the drive
/// reports.
#[derive(Debug, Default, Clone)]
pub struct FrameOut {
    pub vitals: bool,
    pub toolbar: ToolbarSelection,
    pub radar: bool,
    pub indicators: u32,
    pub examination: bool,
    pub book: bool,
    pub inventory: bool,
    pub shortcuts: bool,
    pub beats: usize,
    pub map_written: bool,
    /// The panels were bound to this construction's root.
    pub bound: bool,
    pub salvage_notices_delivered: u64,
    pub panel_messages: u64,
    pub panel_messages_consumed: u64,
    pub trade_drops: u64,
    pub house_drops: u64,
    pub vendor_sell_drops: u64,
    pub salvage_drops: u64,
    pub panels_written: u32,
    pub raises_answered: u64,
    pub skill_rows: u64,
    pub spell_slots: u64,
    pub selection_meters_written: u64,
    pub character_option_rows_reread: u64,
    pub character_option_edges: u64,
    pub character_option_rows_noticed: u64,
    pub chat_option_controls_reread: u64,
    pub chat_option_edges: u64,
    pub placements_applied: u64,
    /// The screen's lock flag after the settings pass, for the HUD's mirror.
    pub locked: bool,
    /// Per chat line: how many chat windows took it.
    pub chat_taken: Vec<usize>,
}

/// [`GameCall::PanelInput`].
#[derive(Debug, Default)]
pub struct PanelInputCall {
    pub target_mode_active: bool,
    pub trace_raise: bool,
    pub panel_messages: u64,
    pub panel_messages_consumed: u64,
    pub trade_drops: u64,
    pub house_drops: u64,
}

/// [`GameCall::ObjectNotice`].
#[derive(Debug, Default)]
pub struct ObjectNoticeCall {
    /// The notice moves or changes an item: the inventory re-reads.
    pub inventory: bool,
    /// The notice changes the selection or an item: the toolbar selection re-reads.
    pub toolbar: bool,
    /// An item moved: the slumlord's move receiver.
    pub moved: Option<ObjectId>,
    pub inventory_written: bool,
    pub toolbar_out: Option<ToolbarSelection>,
}

/// Count a toolbar selection pass's selection query into the four HUD counters, in the order
/// `(health, mana, declined, unanswerable)`.
#[must_use]
pub fn selection_query_counts(t: &ToolbarSelection) -> (u64, u64, u64, u64) {
    match t.query {
        Some(SelectionQuery::Health) => (1, 0, 0, 0),
        Some(SelectionQuery::ItemMana) => (0, 1, 0, 0),
        Some(SelectionQuery::Neither) => (0, 0, 1, 0),
        Some(SelectionQuery::NotAsked) => (0, 0, 0, 1),
        None => (0, 0, 0, 0),
    }
}

fn panels_of<'a>(g: &'a mut GameCx<'_>) -> Option<&'a mut RemainingPanels> {
    g.lent
        .as_deref_mut()
        .and_then(|l| l.downcast_mut::<RemainingPanels>())
}

/// [`GamePlayScreen`]'s [`dereth_ui::framework::Screen::on_game`].
pub(crate) fn on_game(s: &mut GamePlayScreen, cx: &mut ScreenCx<'_>, g: &mut GameCx<'_>) -> bool {
    // The call is taken out for the duration so the lent panels and the input manager can be
    // borrowed beside it; it goes back with its answers.
    let Some(call) = g.call.downcast_mut::<GameCall>() else {
        return false;
    };
    let mut call = std::mem::replace(call, GameCall::Root(None));
    let took = match &mut call {
        GameCall::Frame(f) => match panels_of(g) {
            Some(p) => {
                frame(s, cx, p, f);
                true
            }
            None => false,
        },
        GameCall::PanelInput(c) => match panels_of(g) {
            Some(p) => {
                panel_input(s, cx, p, c);
                true
            }
            None => false,
        },
        GameCall::RefreshItemInputs => match panels_of(g) {
            Some(p) => {
                s.update_toolbar_selection(cx.ui, cx.view);
                s.update_inventory(cx.ui, cx.view);
                p.external_container.update(cx.ui, cx.view);
                true
            }
            None => false,
        },
        GameCall::ObjectNotice(c) => match panels_of(g) {
            Some(p) => {
                if c.inventory {
                    c.inventory_written = s.update_inventory(cx.ui, cx.view);
                }
                if c.toolbar {
                    c.toolbar_out = Some(s.update_toolbar_selection(cx.ui, cx.view));
                }
                if let Some(object) = c.moved {
                    // Only an existing row whose authoritative owner is no longer the player is
                    // removed; a newly created split object never becomes an optimistic row.
                    p.slumlord
                        .recv_server_says_move_item(cx.ui, object, cx.view);
                }
                true
            }
            None => false,
        },
        GameCall::ChatFocus {
            talk_focus,
            focus,
            enabled,
            is_olthoi,
            reset,
        } => {
            s.main_chat.talk_focus = *talk_focus;
            s.main_chat.is_olthoi = *is_olthoi;
            *reset = s.main_chat.enable_selection(cx.ui, *focus, *enabled);
            true
        }
        GameCall::InitCommunication {
            enabled,
            is_olthoi,
            change,
        } => {
            *change = s
                .main_chat
                .initialize_communication_state(cx.ui, *enabled, *is_olthoi);
            true
        }
        GameCall::InteractionContext {
            target_mode,
            selected,
        } => {
            s.set_interaction_context(*target_mode, *selected);
            true
        }
        GameCall::LockUi(locked) => {
            s.set_lock_ui(*locked);
            true
        }
        GameCall::StartTell(name) => {
            s.chat_recv_notice_start_tell(cx.ui, name);
            true
        }
        GameCall::LoadLayout { layout, placed } => {
            *placed = s.load_screen_layout(cx.ui, layout);
            true
        }
        GameCall::SaveLayout(out) => {
            *out = Some(s.save_screen_layout(cx.ui));
            true
        }
        GameCall::PlayerAirborne(a) => {
            s.player_airborne = *a;
            true
        }
        GameCall::AutoTargetWorld(w) => {
            s.chat_auto_target_world = std::mem::take(w);
            true
        }
        GameCall::ChatWindowTitle { window_id, title } => {
            s.on_set_chat_window_title(cx.ui, *window_id, title.clone());
            true
        }
        GameCall::FramerateDisplay { enabled, values } => {
            s.on_set_framerate_display(cx.ui, *enabled, *values);
            true
        }
        GameCall::FramerateUseTime { framerate, degrade } => {
            s.framerate_use_time(cx.ui, *framerate, *degrade);
            true
        }
        GameCall::BookRangeExit(book) => {
            s.book.recv_object_range_exit(cx.ui, *book);
            true
        }
        GameCall::OfferTradeItem(item) => {
            s.offer_trade_item(*item);
            true
        }
        GameCall::SplitStack(selected) => {
            s.recv_split_stack(cx.ui, *selected, cx.view);
            true
        }
        GameCall::Root(out) => {
            *out = s.root();
            true
        }
        GameCall::PaperDollViewport(out) => {
            *out = s.inventory.paper_doll_viewport;
            true
        }
        GameCall::ExaminePreview(out) => {
            let p = &s.examination;
            *out = p.active.map(|a| {
                (
                    a == crate::panels::examination::ExamineSubUi::Item,
                    p.current,
                    p.creature_viewport,
                )
            });
            true
        }
        GameCall::ExaminationOpen(out) => {
            *out = s.examination.is_open(cx.ui);
            true
        }
        GameCall::CloseExamination(out) => {
            *out = s.examination.close_from_action(cx.ui);
            true
        }
        GameCall::BarberTick(dt) => {
            s.barber.tick_preview(*dt);
            true
        }
        GameCall::BarberPreview(out) => {
            *out = Some((
                s.barber.view3d.clone(),
                s.barber.state.clone(),
                s.barber.tables(),
            ));
            true
        }
        GameCall::KeyBindings(k) => key_bindings(s, cx, g.input.as_deref_mut(), k),
    };
    if let Some(slot) = g.call.downcast_mut::<GameCall>() {
        *slot = call;
    }
    took
}

fn key_bindings(
    s: &mut GamePlayScreen,
    cx: &mut ScreenCx<'_>,
    input: Option<&mut dereth_input::InputManager>,
    k: &mut KeyBindingsCall,
) -> bool {
    match k {
        KeyBindingsCall::InitOptions { out } => {
            let Some(m) = input else { return false };
            let rows = s.key_bindings_init_options(cx.ui, m);
            *out = (rows, s.key_bindings.headers, s.key_bindings.failures);
        }
        KeyBindingsCall::Reinit => {
            let Some(m) = input else { return false };
            s.key_bindings.init_options(cx.ui, m);
        }
        KeyBindingsCall::RefreshFileName(name) => {
            s.key_bindings
                .refresh_keymap_file_name(cx.ui, name.as_deref());
        }
        KeyBindingsCall::KeyHit { control, taken } => {
            *taken = s.key_bindings_key_hit(*control);
        }
        KeyBindingsCall::Drive { events, verdicts } => {
            let Some(m) = input else { return false };
            let (e, v) = s.drive_key_bindings(cx.ui, m);
            *events = e;
            *verdicts = v;
        }
        KeyBindingsCall::TakePageEvents(out) => {
            *out = s.take_key_binding_page_events();
        }
        KeyBindingsCall::OpenLoad { files, current } => {
            let _ = s.key_bindings.open_load_keymap_dialog(
                cx.ui,
                std::mem::take(files),
                current.as_deref(),
            );
        }
        KeyBindingsCall::OpenSave => {
            let _ = s.key_bindings.open_save_keymap_dialog(cx.ui);
        }
        KeyBindingsCall::OpenOverwrite(name) => {
            let _ = s
                .key_bindings
                .open_overwrite_keymap_dialog(cx.ui, std::mem::take(name));
        }
        KeyBindingsCall::OpenReadOnly(name) => {
            let _ = s
                .key_bindings
                .open_read_only_keymap_dialog(cx.ui, std::mem::take(name));
        }
        KeyBindingsCall::Capturing(out) => {
            *out = s
                .key_bindings
                .rows
                .iter()
                .any(crate::options::keybinding::ActionKeyMapRow::capturing);
        }
    }
    true
}

/// The request owner's head: see [`GameCall::PanelInput`].
fn panel_input(
    screen: &mut GamePlayScreen,
    cx: &mut ScreenCx<'_>,
    panels: &mut RemainingPanels,
    c: &mut PanelInputCall,
) {
    let ui = &mut *cx.ui;
    let view = cx.view;
    // The targeted-dialog service runs immediately before this boundary and
    // emits the slumlord's completed local questions. Take only those answers before the host's
    // generic request-owner loop; otherwise that loop correctly preserves/dispatches every
    // sibling but has no access to this HUD-owned panel and would report the answer unowned.
    // The retail dialog-close handler also reaches the panel before later input.
    for (rent, confirmed) in ui.requests.take_house_payment_confirmation_answers() {
        panels
            .slumlord
            .close_payment_confirmation(ui, view, rent, confirmed);
    }
    panels.external_container.target_mode_active = c.target_mode_active;
    panels.trade.target_mode_active = c.target_mode_active;
    let messages = screen.take_panel_messages();
    let split = screen.splitter;
    for message in &messages {
        // The paper doll needs the view for `auto_wield_is_legal`; the doll lives on the screen, so
        // the hop lands there.
        screen.on_paper_doll_drag_over(ui, message, view);
        let consumed = panels.on_element_message_with_split(
            ui,
            message,
            view,
            split.split_size,
            split.max_split_size,
        );
        c.panel_messages_consumed += u64::from(consumed);
        if c.trace_raise {
            // The live path's delivery (the headless tests take the drive's).
            use dereth_ui::msg::element::id as msg;
            if message.id == msg::MOUSE_PRESS || message.id == msg::BUTTON_CLICKED {
                tracing::debug!(
                    target: "dereth::trace::raise",
                    "raise-trace panel message {:?} from element {:#x} p1={} -> \
                     consumed={consumed} attributes.selected_index={} skills.selected_index={}",
                    message.id,
                    message.source_id.0,
                    message.p1,
                    panels.attributes.selected_index,
                    panels.skills.selected_index,
                );
            }
        }
    }
    if panels.vendor.take_split_reset() {
        screen.reset_stack_split_to_max(ui);
    }
    c.panel_messages += messages.len() as u64;
    for (item, split, max) in screen.take_trade_drops() {
        if panels
            .trade
            .drop_item_with_split(&mut ui.requests, item, split, max)
        {
            c.trade_drops += 1;
        }
    }
    for (item, split, max) in screen.take_slumlord_drops() {
        if panels
            .slumlord
            .accept_drag_object_with_split(ui, item, view, split, max)
        {
            c.house_drops += 1;
        }
    }
    for (item, position) in screen.take_trade_offers() {
        if panels.trade.offer_item_at(&mut ui.requests, item, position) {
            c.trade_drops += 1;
        }
    }
}

/// The HUD's per-frame drive, the screen half: see [`GameCall::Frame`].
#[allow(clippy::too_many_lines)]
fn frame(
    screen: &mut GamePlayScreen,
    cx: &mut ScreenCx<'_>,
    panels: &mut RemainingPanels,
    f: &mut FrameCall,
) {
    let ui = &mut *cx.ui;
    let view = cx.view;
    let out = &mut f.out;
    // Refresh vitals, toolbar selection and radar coordinates/compass. All guard on their own
    // value having changed, so the counters are what says they ran at all.
    out.vitals = screen.update_vitals(ui, view);
    // *The whole of* the toolbar's selection update — the name and the two
    // meters, and the stack splitter.
    out.toolbar = screen.update_toolbar_selection(ui, view);
    out.radar = screen.update_radar(ui, view);
    out.indicators = screen.update_indicators(ui, view);
    // Driven from the same place every other window on the screen is. It runs
    // **after** the frame's event application, which is what makes the panel open on the frame
    // the `0x00C9` lands rather than the frame after.
    out.examination = screen.update_examination(ui, view);
    // The same place, reason and frame as the examination panel: the `0x00B4` has
    // already been applied to the world.
    out.book = screen.update_book(ui, view);
    // The same notice-pull bridge for the barber panel.
    let _ = screen.update_barber(ui, view);
    // The inventory panel guards on its own snapshot, so this is a no-op on a frame
    // where nothing moved.
    out.inventory = screen.update_inventory(ui, view);
    // The shortcut-number pass of the combat-mode notice handler. Same guard.
    out.shortcuts = screen.update_shortcuts(ui, view);
    // Deliberately *not* inside either guarded pass above: the selection ring is
    // edge-driven off a notice every item list registers for, and the cooldown wedge is driven
    // off global message 3 once a second. Neither fact is in the pack's snapshot, so a wedge
    // would freeze and a ring would be left behind if this ran only when something moved.
    out.beats = screen.do_item_heartbeat(ui, view);
    // Global message 3, throttled to one update every five seconds by its
    // next-update time. The guard is the listener's own, not a frame-rate compensation.
    if screen.map_update_due(ui.now.0) {
        out.map_written = screen.update_map(ui, view);
    }

    // Bind off the screen root, so they re-run whenever the tree was rebuilt.
    if f.rebind {
        if let Some(root) = screen.root() {
            out.bound = true;
            panels.post_init(ui, root);
        }
    }
    // The skills and spellbook panels receive their own subtrees' messages in the
    // client; here the screen is the registered listener, so it records the two ids they switch
    // on and they are delivered in the same frame. Before `update`, because a filter click
    // rebuilds the spell list and a selection repaints the footer, and both must land in this
    // frame's draw list rather than the next one's.
    panels.trade.target_mode_active = screen.interaction_target_mode_active();
    let messages = screen.take_panel_messages();
    let split = screen.splitter;
    for notice in std::mem::take(&mut f.external_container) {
        panels.external_container.recv_notice(ui, notice, view);
    }
    // The same one-frame hop the line above takes, delivering `OpenSalvagePanel` to the salvage
    // panel.
    let salvage_notices = std::mem::take(&mut f.salvage);
    out.salvage_notices_delivered += salvage_notices.len() as u64;
    for notice in salvage_notices {
        panels.salvage.recv_notice(ui, notice, view);
    }
    let mut consumed = 0u64;
    for m in &messages {
        // The doll's `0x3E`, delivered with the view -- the same line as in the
        // request owner's head.
        screen.on_paper_doll_drag_over(ui, m, view);
        consumed += u64::from(panels.on_element_message_with_split(
            ui,
            m,
            view,
            split.split_size,
            split.max_split_size,
        ));
    }
    if panels.vendor.take_split_reset() {
        screen.reset_stack_split_to_max(ui);
    }
    out.panel_messages += messages.len() as u64;
    out.panel_messages_consumed += consumed;
    if f.trace_raise {
        // The presses offered, and the selection each sub-panel holds afterwards.
        use dereth_ui::msg::element::id as msg;
        for m in &messages {
            if m.id == msg::MOUSE_PRESS || m.id == msg::BUTTON_CLICKED {
                tracing::debug!(
                    target: "dereth::trace::raise",
                    "raise-trace panel message {:?} from element {:#x} p1={} -> \
                     attributes.selected_index={} skills.selected_index={} \
                     (consumed this frame: {consumed})",
                    m.id,
                    m.source_id.0,
                    m.p1,
                    panels.attributes.selected_index,
                    panels.skills.selected_index,
                );
            }
        }
    }
    // Deliver accepted trade drags to the trade panel's add-item. The screen recorded the drop
    // because the panel lives with the HUD; this is the delivery.
    for (item, split, max) in screen.take_trade_drops() {
        if panels
            .trade
            .drop_item_with_split(&mut ui.requests, item, split, max)
        {
            out.trade_drops += 1;
        }
    }
    for (item, position) in screen.take_trade_offers() {
        if panels.trade.offer_item_at(&mut ui.requests, item, position) {
            out.trade_drops += 1;
        }
    }
    // The slumlord's two payment lists are owned by the HUD just as the
    // secure-trade list above is. Preserve the splitter pair captured by the actual release.
    for (item, split, max) in screen.take_slumlord_drops() {
        if panels
            .slumlord
            .accept_drag_object_with_split(ui, item, view, split, max)
        {
            out.house_drops += 1;
        }
    }
    // Deliver accepted vendor-sell drags to the vendor's add-item-to-sell, the same hop.
    for (item, whole_stack) in screen.take_vendor_sell_drops() {
        if panels.vendor.drop_item(&mut ui.requests, item, whole_stack) {
            out.vendor_sell_drops += 1;
        }
    }
    // Deliver accepted salvage drags to `add_new_item`, the same hop.
    for item in screen.take_salvage_drops() {
        if panels.salvage.accept_drag_object(ui, item, view) {
            out.salvage_drops += 1;
        }
    }
    out.panels_written = panels.update(ui, view);
    // The page visibility callback and its parent environment stack are synchronous in the
    // client. Deliver the parent visibility here too, before this frame's draw list.
    //
    // Both environment pages this HUD drives go through here. The order is the
    // retail one: the corpse window first, so that when a `0x0062` arrives while a ground
    // container is open the close is delivered before the open. Delivered on an **edge**: two
    // pages that each re-assert themselves every frame displace each other without settling.
    for (slot, (page, root)) in [
        (external_container::PANEL, panels.external_container.root),
        (crate::panels::vendor::PANEL, panels.vendor.root),
        // The salvage window.
        (crate::panels::salvage::PANEL, panels.salvage.root),
        // The slumlord page.
        (crate::panels::slumlord::PANEL, panels.slumlord.root),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(root) = root else {
            // A destroyed or unbound panel forgets its edge, so a rebuilt one is delivered
            // again rather than silently matching a stale value from its predecessor.
            f.env_page_visibility[slot] = None;
            continue;
        };
        let visible = ui.node(root).is_some_and(|n| n.region.flags.visible);
        if f.env_page_visibility[slot] == Some(visible) {
            continue;
        }
        f.env_page_visibility[slot] = Some(visible);
        if let Some(crate::view::UiRequest::SetPanelVisibility { panel, .. }) =
            screen.env_panel.on_page_visibility_changed(page, visible)
        {
            screen.recv_set_panel_visibility(ui, panel, visible);
        }
    }
    // On the `0x10000004` answer the skills panel's awaiting-raise state clears
    // and the footer re-runs; the attributes panel has the same state.
    if std::mem::take(&mut f.raise_answered) {
        panels.skills.clear_awaiting_raise();
        panels.skills.update_selection(ui, view);
        panels.attributes.clear_awaiting_raise();
        panels.attributes.update_selection(ui, view);
        out.raises_answered += 1;
    }
    out.skill_rows = panels.skills.rows.len() as u64;
    out.spell_slots = panels.spellbook.shown.len() as u64;
    if f.trace_raise {
        // After the message delivery, the raise re-run and the panels' update:
        // the `+10` button as it will be drawn this frame, printed on change.
        let line = plus_ten_lines(ui, panels, view);
        if f.trace_plus_ten_last.as_deref() != Some(line.as_str()) {
            for l in line.lines() {
                tracing::debug!(target: "dereth::trace::raise","raise-trace +10 watch {l}");
            }
            f.trace_plus_ten_last = Some(line);
        }
    }
    // The object-health / item-mana update handlers — the answer to the two
    // selection queries, reaching the meter it is for.
    out.selection_meters_written += screen.update_selected_meters(ui, view) as u64;
    // The option-page visibility drain: `save_current_values` on show,
    // `restore_saved_values` on hide.
    for moved in screen.drive_character_options(ui, view) {
        out.character_option_rows_reread += moved as u64;
        out.character_option_edges += 1;
    }
    // Refresh the page's rows whose bit moved under them, after the frame's
    // requests have reached the option writer.
    out.character_option_rows_noticed +=
        screen.character_options.on_player_option_changed(ui, view) as u64;
    // The Chat Options page's Apply / Cancel / *Restore Defaults* and its
    // visibility edge.
    for moved in screen.drive_chat_options(ui, view) {
        out.chat_option_controls_reread += moved as u64;
        out.chat_option_edges += 1;
    }
    // A received player description feeds every window's `update_from_player_module`, once per
    // description.
    if let Some(pm) = f.player_settings.as_ref() {
        out.placements_applied += screen.update_from_player_module(ui, pm) as u64;
    }
    // The screen's lock flag for the HUD-side mirror, *after* the settings pass
    // above: that pass seeds a freshly built screen *from* the module, the mirror writes a
    // toggled screen *into* it.
    out.locked = screen.locked;
    // The three remembered names.
    screen.reply_targets = std::mem::take(&mut f.reply_targets);
    // The stay-in-chat-mode input.
    screen.stay_in_chat_mode = f.stay_in_chat_mode;
    // The auto-target producer.
    screen.chat_target_squelched = f.chat_target_squelched;
    screen.chat_auto_target_world = std::mem::take(&mut f.auto_target_world);

    for m in std::mem::take(&mut f.chat) {
        let took = screen.recv_display_final_string_info(ui, &m);
        if f.trace_notice {
            // The second receiver: which chat interfaces accepted the line, and
            // every window's live filter word.
            let filters: Vec<String> = screen
                .chat
                .iter()
                .map(|c| format!("{}:{:#x}", c.window_id, c.filter))
                .collect();
            tracing::debug!(
                target: "dereth::trace::notice",
                "notice-trace chat windows offered type={:#x} window={} text={:?} \
                 -> took={took:?} filters=[{}]",
                m.ty,
                m.window,
                m.body,
                filters.join(" "),
            );
        }
        out.chat_taken.push(took.len());
    }
}

/// What the live tree says about one footer button: the node's state, the instance and merged
/// attribute `0x0D` (the button-disabled attribute, written when the button state changes),
/// visibility, and screen box.
fn button_desc(ui: &UiSystem, footer: &Footer, id: u32) -> String {
    let Some(h) = footer.child(ui, id) else {
        return "absent under the resolved container".into();
    };
    let Some(n) = ui.node(h) else {
        return "dead handle".into();
    };
    format!(
        "state={} inst0x0D={:?} merged0x0D={:?} visible={} mouse_visible={} box={:?}",
        n.state.0,
        n.instance_properties.get_bool(statmgmt::ATTR_DISABLED),
        n.merged_properties().get_bool(statmgmt::ATTR_DISABLED),
        n.region.flags.visible,
        n.is_mouse_visible,
        ui.screen_box(h),
    )
}

/// One line per stat sub-panel: the selected row, the sub-panel's state and the container it
/// resolves to, the enable inputs (available experience, the cost to raise by one and by ten), the
/// decision the footer wrote, and the `+10` / `+1` nodes as they are on screen right now.
///
/// The HUD drive's raise watch prints this only when it changes.
#[must_use]
pub fn plus_ten_lines(ui: &UiSystem, panels: &RemainingPanels, view: &dyn GameView) -> String {
    let xp = view.available_experience();
    let mut out = String::new();

    let p = &panels.attributes;
    match p.footer {
        None => out.push_str("attributes: footer unbound"),
        Some(f) => {
            let row = p.selected();
            let sel = row.map_or_else(
                || "none".to_owned(),
                |r| {
                    format!(
                        "{:?} (stat {} secondary {})",
                        r.name,
                        r.wire_stat(),
                        r.secondary
                    )
                },
            );
            let costs = row
                .and_then(|r| view.attribute_advancement(r.wire_stat(), r.secondary))
                .map_or_else(
                    || "no advancement (no player state or no XP table)".to_owned(),
                    |a| {
                        format!(
                            "level_from_cp={} cp_spent={} cost_1={} cost_10={}",
                            a.level_from_cp, a.cp_spent, a.cost_to_raise, a.cost_to_raise_10
                        )
                    },
                );
            let c = &p.footer_content;
            out.push_str(&format!(
                "attributes: row={sel} footer_state={:#x} container={:#x} available_xp={xp} {costs} \
                 decided(button={:#x} button_10={:#x} button_10_visible={}) line2={:?} \
                 | +10 node [{}] | +1 node [{}]",
                f.state,
                Footer::container_for(f.state),
                c.button,
                c.button_10,
                c.button_10_visible,
                c.line_two_value,
                button_desc(ui, &f, child::BUTTON_10),
                button_desc(ui, &f, child::BUTTON),
            ));
        }
    }
    out.push('\n');

    let p = &panels.skills;
    match p.footer {
        None => out.push_str("skills: footer unbound"),
        Some(f) => {
            let row = usize::try_from(p.selected_index)
                .ok()
                .and_then(|i| p.rows.get(i));
            let sel = row.map_or_else(
                || "none".to_owned(),
                |r| format!("{:?} (skill {})", r.name, r.skill),
            );
            let costs = row
                .and_then(|r| view.skill_advancement(r.skill))
                .map_or_else(
                    || "no advancement (no player state or no XP table)".to_owned(),
                    |a| {
                        format!(
                            "sac={} level_from_pp={} pp={} cost_1={} cost_10={}",
                            a.sac, a.level_from_pp, a.pp, a.cost_to_raise, a.cost_to_raise_10
                        )
                    },
                );
            let c = &p.footer_content;
            out.push_str(&format!(
                "skills: row={sel} footer_state={:#x} container={:#x} available_xp={xp} {costs} \
                 decided(button={:#x} button_10={:#x} button_10_visible={}) line2={:?} \
                 | +10 node [{}] | +1 node [{}]",
                f.state,
                Footer::container_for(f.state),
                c.button,
                c.button_10,
                c.button_10_visible,
                c.line_two_value,
                button_desc(ui, &f, child::BUTTON_10),
                button_desc(ui, &f, child::BUTTON),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_ui::framework::Screen;

    fn screen() -> (UiSystem, GamePlayScreen) {
        (UiSystem::new((800, 600)), GamePlayScreen::default())
    }

    /// The host checks the answer of every call it makes, so a call of another vocabulary must
    /// be refused, not half-taken: the answer is `false` and the call is left as it was.
    #[test]
    fn a_call_of_a_foreign_type_is_refused_and_left_untouched() {
        let (mut ui, mut s) = screen();
        let mut foreign = 7u32;
        let took = s.on_game(&mut ScreenCx::new(&mut ui), &mut GameCx::call(&mut foreign));
        assert!(!took);
        assert_eq!(foreign, 7);
    }

    /// A panel call needs the HUD's panels lent to it. Without them, or with something else lent,
    /// the screen refuses it and hands the call back unchanged.
    #[test]
    fn a_panel_call_without_the_huds_panels_is_refused_and_handed_back() {
        let (mut ui, mut s) = screen();
        let mut call = GameCall::RefreshItemInputs;
        assert!(!s.on_game(&mut ScreenCx::new(&mut ui), &mut GameCx::call(&mut call)));
        assert!(matches!(call, GameCall::RefreshItemInputs));

        let mut wrong = 0u8;
        assert!(!s.on_game(
            &mut ScreenCx::new(&mut ui),
            &mut GameCx::lending(&mut call, &mut wrong)
        ));
        assert!(matches!(call, GameCall::RefreshItemInputs));
    }

    /// The key-binding page's calls that read the input manager are refused without one.
    #[test]
    fn a_key_binding_call_that_needs_the_input_manager_is_refused_without_it() {
        let (mut ui, mut s) = screen();
        let mut call = GameCall::KeyBindings(KeyBindingsCall::InitOptions { out: (9, 9, 9) });
        assert!(!s.on_game(&mut ScreenCx::new(&mut ui), &mut GameCx::call(&mut call)));
        assert!(matches!(
            call,
            GameCall::KeyBindings(KeyBindingsCall::InitOptions { out: (9, 9, 9) })
        ));
    }

    /// A call of the screen's own vocabulary that needs nothing lent is taken, and its effect and
    /// answers land.
    #[test]
    fn a_call_of_the_screens_own_vocabulary_is_taken() {
        let (mut ui, mut s) = screen();
        let mut call = GameCall::PlayerAirborne(true);
        assert!(s.on_game(&mut ScreenCx::new(&mut ui), &mut GameCx::call(&mut call)));
        assert!(s.player_airborne);

        let mut call = GameCall::PaperDollViewport(None);
        assert!(s.on_game(&mut ScreenCx::new(&mut ui), &mut GameCx::call(&mut call)));
        assert!(
            matches!(call, GameCall::PaperDollViewport(_)),
            "the answer comes back in the call"
        );
    }
}
