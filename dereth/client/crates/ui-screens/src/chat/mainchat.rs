//! `MainChat` and `FloatingChat` — what the two concrete windows add to `ChatInterface`.

use dereth_ui::ElementId;
pub use main_chat::CHAT_TARGET_BUTTON_TEXT;

/// One row of the talk-focus (chat target) menu.
///
/// The menu button is tagged with attribute `0x1000000B` = the talk-focus id; picking it calls
/// sets talk focus `n`, writes `caption` into the chat-target button text and
/// un-presses the others. All labels come from string table enum `0x10000001`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TalkFocus {
    /// The focus id, and the value of attribute `0x1000000B` on the menu button.
    pub id: u32,
    /// The caption written into the chat-target button once this focus is picked.
    pub caption: &'static str,
    /// The menu item's own label.
    pub item_label: &'static str,
}

const fn tf(id: u32, caption: &'static str, item_label: &'static str) -> TalkFocus {
    TalkFocus {
        id,
        caption,
        item_label,
    }
}

/// The thirteen talk-focus entries, in menu order.
///
/// The Olthoi entry (id 13) is gated on the player's int property `0xBC`, and
/// picking Allegiance (id 7) additionally sets the wants-to-be-in-allegiance-chat flag.
pub const TALK_FOCUS_MENU: [TalkFocus; 13] = [
    tf(1, "ID_Chat_ChatTargetMenu", "ID_Chat_TellToAll"),
    tf(
        2,
        "ID_Chat_ChatTargetMenuSelected",
        "ID_Chat_TellToSelectedNoSelection",
    ),
    tf(3, "ID_Chat_ChatTargetMenuFellows", "ID_Chat_TellToFellows"),
    tf(4, "ID_Chat_ChatTargetMenuPatron", "ID_Chat_TellToPatron"),
    tf(5, "ID_Chat_ChatTargetMenuMonarch", "ID_Chat_TellToMonarch"),
    tf(6, "ID_Chat_ChatTargetMenuVassals", "ID_Chat_TellToVassals"),
    tf(
        7,
        "ID_Chat_ChatTargetMenuAllegiance",
        "ID_Chat_TellToAllegiance",
    ),
    tf(8, "ID_Chat_ChatTargetMenuGeneral", "ID_Chat_TellToGeneral"),
    tf(9, "ID_Chat_ChatTargetMenuTrade", "ID_Chat_TellToTrade"),
    tf(10, "ID_Chat_ChatTargetMenuLFG", "ID_Chat_TellToLFG"),
    tf(
        0x0B,
        "ID_Chat_ChatTargetMenuRoleplay",
        "ID_Chat_TellToRoleplay",
    ),
    tf(
        0x0C,
        "ID_Chat_ChatTargetMenuSociety",
        "ID_Chat_TellToSociety",
    ),
    tf(0x0D, "ID_Chat_ChatTargetMenuOlthoi", "ID_Chat_TellToOlthoi"),
];

/// The talk-focus id that additionally sets the wants-to-be-in-allegiance-chat flag.
pub const ALLEGIANCE_FOCUS: u32 = 7;
/// The talk-focus id gated on.
pub const OLTHOI_FOCUS: u32 = 0x0D;

/// The reverse lookup, by attribute `0x1000000B`.
#[must_use]
pub fn talk_focus(id: u32) -> Option<TalkFocus> {
    TALK_FOCUS_MENU.iter().copied().find(|t| t.id == id)
}

/// The attribute a talk-focus menu button is tagged with.
pub const ATTR_TALK_FOCUS: u32 = 0x1000_000B;

/// `MainChat`'s named children beyond `ChatInterface`'s.
pub mod main_chat {
    use super::ElementId;
    /// The container [`super::MainChatPanel::init_talk_focus_menu`] fills.
    pub const TALK_FOCUS_MENU_CONTAINER: ElementId = ElementId(0x1000_0014);
    /// The chat-target button text — the caption the chosen focus's name is written into. The
    /// main chat panel's post-init finds it by recursive child search.
    pub const CHAT_TARGET_BUTTON_TEXT: ElementId = ElementId(0x1000_0015);
    /// The maximize button; message 1 is the button, message 7 the menu selection.
    pub const MAXIMIZE_BUTTON: ElementId = ElementId(0x1000_046F);
    /// The state the maximize handler puts [`MAXIMIZE_BUTTON`] into when it grows the
    /// window — `0x10000047`, set as the element state. A **layout**
    /// state id, like the talk-focus rows'.
    pub const MAXIMIZE_MAXIMIZED: dereth_ui::StateId = dereth_ui::StateId(0x1000_0047);
    /// Its partner, `0x10000048` — the restored state.
    pub const MAXIMIZE_RESTORED: dereth_ui::StateId = dereth_ui::StateId(0x1000_0048);
    /// The "send" button on the input line.
    pub const SEND_BUTTON: ElementId = ElementId(0x1000_0019);
    /// Clicking the log re-focuses the entry.
    pub const CHAT_LOG: ElementId = ElementId(0x1000_048C);
}

/// `FloatingChat`'s named children.
pub mod floaty_chat {
    use super::ElementId;
    /// The close/clear button, message 1.
    pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_052A);
    /// The title text element.
    pub const TITLE_TEXT: ElementId = ElementId(0x1000_04D9);
}

/// The four input actions bound to the reply keys.
pub mod action {
    /// Reply to the last tell.
    pub const REPLY: u32 = 0x1000_0020;
    /// Reply to the monarch.
    pub const REPLY_MONARCH: u32 = 0x1000_0021;
    /// Reply to the patron.
    pub const REPLY_PATRON: u32 = 0x1000_0022;
    /// Activate the chat entry and select all of it.
    pub const ACTIVATE_ENTRY: u32 = 0x1000_0023;
    /// The toggle chat entry handler.
    pub const TOGGLE_ENTRY: u32 = 0x1000_0024;
    /// The two actions, keyed into string table enum 6.
    pub const COMMAND_OR_ALIAS: [u32; 2] = [0x1000_0028, 0x1000_0119];
}

/// The object-id window a selected object must fall in for the tell action to start a tell:
/// "when that object id is a player (`0x50000000 < id < 0x70000000`)".
#[must_use]
pub fn selection_is_a_player(id: u32) -> bool {
    (0x5000_0000 < id) && (id < 0x7000_0000)
}

// ---------------------------------------------------------------------------------------------
// `MainChat`'s talk-focus menu, maximize button and panel lamps
// ---------------------------------------------------------------------------------------------

use dereth_ui::{ElemHandle, UiSystem};

/// The string table the talk-focus menu builds its caption in: table enum `0x10000001`.
///
/// The DataId is the same one [`crate::panels::statmgmt::STRING_TABLE`] resolves against — the
/// `DidMapper` group-4 entry for enum `0x10000001`. Keeping the id here rather than the enum is
/// the same exception `statmgmt` takes: a screen has no `LayoutEnumResolver`.
pub const CAPTION_STRING_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0001);

/// Resolve one `ID_*` token, or fall back to the token itself.
///
/// The fall-back is what makes a headless test — which has no string service — still able to see
/// **which** caption was written, rather than an empty box that is indistinguishable from a
/// caption that was never written at all.
#[must_use]
pub fn label(ui: &UiSystem, token: &str) -> String {
    ui.resolve_string(
        CAPTION_STRING_TABLE,
        dereth_primitives::num::hash::str_hash(token.as_bytes()),
    )
    .unwrap_or_else(|| token.to_owned())
}

/// Resolve a one-variable `StringInfo` whose variable is named `VALUE` in the native caller.
///
/// String tables store the literal pieces around a variable as separate variants. A literal
/// `%s` replacement therefore sees only variant zero and cannot insert the value. The fallback
/// preserves source-only tests whose `UiSystem` deliberately has no string service.
#[must_use]
fn label_value(ui: &UiSystem, token: &str, value: &str) -> String {
    ui.resolve_string_rendered(
        CAPTION_STRING_TABLE,
        dereth_primitives::num::hash::str_hash(token.as_bytes()),
        &[value.to_owned()],
    )
    .unwrap_or_else(|| label(ui, token).replace("%s", value))
}

/// The menu's **display order**, which is not the id order.
///
/// The menu fill adds the squelch toggle first (no `0x1000000B` attribute, so it
/// is not a focus row) and then these thirteen, each tagged with its focus id. The tags, in the
/// order the rows are added, are
/// 5, 2, 4, 1, 6, 3, 7, 8, 9, 10, 11, 12, 13 — monarch, selected, patron, all, vassals, fellows,
/// allegiance, then the six Turbine channels. \[verified\]
///
/// `dereth_client_model::chat::TALK_FOCUS_MENU_ORDER` is the same order for the first twelve; this one adds
/// Olthoi, which the client offers and gates on.
pub const TALK_FOCUS_MENU_ORDER: [u32; 13] = [5, 2, 4, 1, 6, 3, 7, 8, 9, 10, 11, 12, 13];

/// The focus the panel picks before anything else: its post-init fills the menu, sets no selected
/// target, then selects focus 1.
pub const DEFAULT_TALK_FOCUS: u32 = 1;

/// The element state a talk-focus menu item is put in when its focus is **not** available —
/// the selection handler's own guard returns early when the item's state is `0xD`.
pub const STATE_DISABLED: dereth_ui::StateId = dereth_ui::StateId(0x0D);

/// `0x0D`'s partner, and the state every available row is put in.
///
/// The whole vocabulary, at the four places the client sets a row state:
///
/// ```text
///   resetting every row: Olthoi, and this row is not 1 or 13 -> 0x0D
///   ...Olthoi, and it is 1 or 13                             -> 1
///   ...not Olthoi: focus n enabled ? 1 : 0x0D
///   the selection handler's tail: the CHOSEN row             -> 0x10000001
///   the panel-visibility notice, visible                     -> 6
///   ...not visible                                           -> 1
/// ```
/// \[verified\]
pub const STATE_ENABLED: dereth_ui::StateId = dereth_ui::StateId(1);

/// The state the selection handler puts the row a player just picked into — its last line,
/// after the talk-focus button reset has put every row back to `1` or `0x0D`. It is a
/// **layout** state id (`0x10000001`), not one of the generic `0`…`5`.
pub const STATE_SELECTED: dereth_ui::StateId = dereth_ui::StateId(0x1000_0001);

/// The state the panel-visibility notice puts a lamp button in when its floaty
/// window is visible. Its "off" partner is [`STATE_ENABLED`], not `NORMAL`.
pub const STATE_LAMP_LIT: dereth_ui::StateId = dereth_ui::StateId(6);

/// The four floaty chat windows' lamp
/// buttons on the main chat window's own frame.
///
/// ```text
/// panel 0x10000505 -> lamp 0x10000522    panel 0x1000050E -> lamp 0x10000523
/// panel 0x1000050F -> lamp 0x10000524    panel 0x10000510 -> lamp 0x10000525
/// anything else    -> no lamp
/// ```
#[must_use]
pub fn panel_lamp(panel: ElementId) -> Option<ElementId> {
    Some(match panel.0 {
        0x1000_0505 => ElementId(0x1000_0522),
        0x1000_050E => ElementId(0x1000_0523),
        0x1000_050F => ElementId(0x1000_0524),
        0x1000_0510 => ElementId(0x1000_0525),
        _ => return None,
    })
}

/// What `handle_selection` settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TalkFocusChange {
    /// The new talk focus.
    pub focus: u32,
    /// The caption string id written into the chat-target button text.
    pub caption: &'static str,
    /// Whether this selection also set the wants-to-be-in-allegiance-chat flag — focus 7 only.
    pub wants_alleg_chat: bool,
}

/// `MainChat`'s own state: the talk-focus menu, its enable mask and the chat-target caption.
///
/// Without this state the client could only ever be on focus 1, and every line a player typed would
/// go out as ordinary speech whatever the menu said.
///
/// The original client creates the menu **items** at runtime. This build models those rows rather
/// than instantiating menu elements, and `handle_selection` receives the focus id that the
/// original selection-message path read from property `0x1000000B`. What is
/// **not** modelled is the element half of the caption: that is written onto the real
/// `0x10000015`.
#[derive(Debug, Clone)]
pub struct MainChatPanel {
    /// The chat-target button text — `0x10000015`.
    pub chat_target_button: Option<ElemHandle>,
    /// The menu container [`Self::init_talk_focus_menu`] fills — `0x10000014`.
    pub menu: Option<ElemHandle>,
    /// The maximize button — `0x1000046F`.
    pub maximize: Option<ElemHandle>,
    /// The current talk focus.
    pub talk_focus: u32,
    /// The squelch toggle — the menu's **row 0**, added by [`Self::init_talk_focus_menu`]
    /// before the thirteen focus rows and deliberately given **no** `0x1000000B` attribute, which
    /// is why [`Self::menu_item`] cannot find it and why the focus rows start at index 1.
    pub squelch_toggle: Option<ElemHandle>,
    /// Whether talk focus n is enabled, for n in 1..=13, indexed by focus id.
    enabled: [bool; 14],
    /// Each menu row's element state, indexed by focus id — the thing the selection handler
    /// actually guards on (it returns early when the state is `0xD`) and the thing
    /// the client writes. It is a state, not a bool mask: a row has more than two states.
    ///
    /// [`Self::talk_focus_buttons`] holds the real elements, so this is the *mirror* and
    /// [`Self::row_state`] — the one the guards read — asks the element first. It is what a
    /// `MainChatPanel` with no live menu behind it still answers, which is every unit test in this
    /// file and nothing a player ever sees.
    row_state: [dereth_ui::StateId; 14],
    /// The thirteen talk-focus row **elements**
    /// [`Self::init_talk_focus_menu`] created and appended, in the client's own add order
    /// (5, 2, 4, 1, 6, 3, 7, 8, 9, 10, 11, 12, 13). The squelch toggle is **not** in here: it is
    /// added first and deliberately given no `0x1000000B`, which is why
    /// the row lookup cannot see it.
    pub talk_focus_buttons: Vec<ElemHandle>,
    /// The player's int property `0xBC` is `0xC` or `0xD`.
    pub is_olthoi: bool,
    /// The last speakable target, the object
    /// last named.
    pub last_speakable_target: u32,
    /// Maximized / old y / old height — the client's three fields.
    pub maximized: bool,
    pub old_y: i32,
    pub old_height: i32,
    /// The caption token currently on the chat-target button.
    pub caption: &'static str,
    /// The wants-to-be-in-allegiance-chat flag.
    pub wants_alleg_chat: bool,
}

impl Default for MainChatPanel {
    fn default() -> Self {
        // The communication system leaves 1 and 2 always on
        // and recomputes 8..13 from the Turbine-chat options; 3..7 follow allegiance/fellowship
        // membership. Before any of that arrives, the client's own talk-focus-enabled defaults
        // are the two `dereth_client_model::chat::ChatState::new` sets.
        let mut enabled = [false; 14];
        enabled[1] = true;
        enabled[2] = true;
        Self {
            chat_target_button: None,
            menu: None,
            maximize: None,
            squelch_toggle: None,
            talk_focus: DEFAULT_TALK_FOCUS,
            enabled,
            // Filling the menu adds every row in the layout's own default state; nothing is
            // disabled until the talk-focus button reset runs.
            row_state: [STATE_ENABLED; 14],
            talk_focus_buttons: Vec::new(),
            is_olthoi: false,
            last_speakable_target: 0,
            maximized: false,
            old_y: 0,
            old_height: 0,
            caption: "ID_Chat_ChatTargetMenu",
            wants_alleg_chat: false,
        }
    }
}

impl MainChatPanel {
    /// The main chat panel's post-init's tail: find the chat-target button text `0x10000015`,
    /// fill the talk-focus menu, set no selected target, and select focus 1.
    #[must_use]
    pub fn post_init(ui: &mut UiSystem, root: ElemHandle) -> Self {
        let mut s = Self {
            chat_target_button: ui.get_child_recursive(root, main_chat::CHAT_TARGET_BUTTON_TEXT),
            menu: ui.get_child_recursive(root, main_chat::TALK_FOCUS_MENU_CONTAINER),
            maximize: ui.get_child_recursive(root, main_chat::MAXIMIZE_BUTTON),
            // The squelch toggle is a menu text item — it is **created**,
            // not looked up, which is why it is filled in by [`Self::init_talk_focus_menu`]
            // below rather than bound from an element id.
            squelch_toggle: None,
            ..Self::default()
        };
        // Fill the menu — fourteen text items, in this exact position in the panel's post-init:
        // before `set_selected` and before `handle_selection`, both of which write onto rows that
        // have to exist first.
        s.init_talk_focus_menu(ui);
        // No speakable target yet, which disables focus 2 and puts the
        // "Tell to &lt;selected&gt;" row into `ID_Chat_TellToSelectedNoSelection`.
        s.set_selected(ui, None);
        s.handle_selection(ui, DEFAULT_TALK_FOCUS);
        s
    }

    /// Fill the talk-focus menu.
    ///
    /// The client finds `0x10000014` and requires it to be a menu (type 6), returning otherwise.
    /// It clears the row list and flushes the menu, adds the squelch row
    /// (`ID_Chat_SquelchSelectedNoSelection`, table `0x10000001`) as row 0 with **no**
    /// `0x1000000B`, then adds one text item per focus in the order 5 Monarch,
    /// 2 SelectedNoSelection, 4 Patron, 1 All, 6 Vassals, 3 Fellows, 7 Allegiance, 8 General,
    /// 9 Trade, 10 LFG, 11 Roleplay, 12 Society, 13 Olthoi; each item that was created is tagged
    /// with its focus id in `0x1000000B` and appended to the row list.
    ///
    /// **It does not walk a layout.** Fourteen rows are *created*, one per text item, and the
    /// thirteen focus rows are told which focus they are through attribute `0x1000000B` — which
    /// is the only thing that identifies a row afterwards, because all fourteen are built from the
    /// same description (`0x1000001E`, in layout `0x21000006`) and therefore share one element id.
    ///
    /// The add order is **not** id order; it is [`TALK_FOCUS_MENU_ORDER`]. The squelch row is
    /// first and carries no attribute, which is why the row lookup starts at what a player sees as
    /// the second row.
    ///
    /// Returns how many of the thirteen carry a real `0x1000000B` — 13 with the shipped data, 0
    /// with no asset environment installed, which is the number to report rather than a bool.
    pub fn init_talk_focus_menu(&mut self, ui: &mut UiSystem) -> usize {
        // The lookup of `0x10000014` — `post_init` already did it and kept the result.
        let Some(menu) = self.menu else { return 0 };
        if ui
            .node(menu)
            .is_none_or(|n| n.ty() != dereth_ui::ElementType(6))
        {
            // Not a menu: the client's cast fails and it returns.
            return 0;
        }
        // The menu's popup construction and the client's second half. In the client these run
        // from the menu's initialization, which has the element manager to hand; here the
        // asset source arrives through `env`, so they run once, lazily, on the first menu fill.
        // Everything after this is the client's function step for step.
        if dereth_ui::widgets::menu::list_box_handle(ui, menu).is_none() {
            let made = ui.env().cloned().map(|e| {
                e.with_assets(|assets| dereth_ui::widgets::menu::make_popup(ui, assets, menu))
            });
            if made.flatten().is_none() {
                return 0;
            }
            dereth_ui::widgets::menu::initialize_popup(ui, menu);
        }
        self.talk_focus_buttons.clear();
        self.squelch_toggle = None;
        dereth_ui::widgets::menu::flush(ui, menu);

        let squelch = label(ui, "ID_Chat_SquelchSelectedNoSelection");
        let mut rows: Vec<(u32, String)> = Vec::with_capacity(14);
        for id in TALK_FOCUS_MENU_ORDER {
            let Some(row) = talk_focus(id) else { continue };
            rows.push((id, label(ui, row.item_label)));
        }
        let made = ui.env().cloned().map(|e| {
            e.with_assets(|assets| {
                let mut out: Vec<(u32, ElemHandle)> = Vec::with_capacity(13);
                let toggle = dereth_ui::widgets::menu::add_text_item(ui, assets, menu, &squelch);
                for (id, caption) in &rows {
                    if let Some(h) =
                        dereth_ui::widgets::menu::add_text_item(ui, assets, menu, caption)
                    {
                        out.push((*id, h));
                    }
                }
                (toggle, out)
            })
        });
        let Some((toggle, made)) = made else { return 0 };
        self.squelch_toggle = toggle;
        for (id, h) in made {
            // Tag with the focus id in `0x1000000B`, then append. Both happen only for an item the
            // client actually created, so a row that failed to build is neither tagged nor recorded.
            ui.set_attribute_enum(h, ATTR_TALK_FOCUS, id);
            self.talk_focus_buttons.push(h);
        }
        // Push the states the model already holds onto the elements that now exist, which is what
        // post-init's following `set_selected`/`handle_selection` pair would do anyway and what
        // keeps a re-run of this function idempotent.
        for row in TALK_FOCUS_MENU {
            let s = self
                .row_state
                .get(row.id as usize)
                .copied()
                .unwrap_or(STATE_ENABLED);
            if let Some(h) = self.menu_item(ui, row.id) {
                ui.set_state(h, s);
            }
        }
        self.talk_focus_buttons.len()
    }

    /// Set whether talk focus `n` is enabled.
    ///
    /// The mask and the row's element state are two halves of one thing and the client never moves
    /// one without the other: the communication system sets the flag and broadcasts the
    /// enable-chat-target-selection notice, whose receiver is
    /// [`Self::enable_selection`]. This writes both, so a caller that has no `UiSystem` in hand
    /// still leaves the two consistent; [`Self::enable_selection`] is the faithful one and is what
    /// a notice should go through.
    pub fn set_talk_focus_enabled(&mut self, focus: u32, on: bool) {
        if let Some(slot) = self.enabled.get_mut(focus as usize) {
            *slot = on;
        }
        if talk_focus(focus).is_some() {
            if let Some(slot) = self.row_state.get_mut(focus as usize) {
                *slot = if on { STATE_ENABLED } else { STATE_DISABLED };
            }
        }
    }

    /// The main chat panel's enable chat target selection notice.
    ///
    /// With no row for the focus it does nothing. For an Olthoi character, any focus other than
    /// 1, 2 and 13 is set to `0xD` and it stops. Otherwise, only when the state really changes
    /// (enabling a row that is `0xD`, or disabling one that is not), the row is set to `1` or
    /// `0xD`; disabling the current focus then selects focus 1.
    ///
    /// **The Olthoi set here is `{1, 2, 13}`** — and the talk-focus button reset's is `{1, 13}`.
    /// Both are verified against retail and they genuinely differ: this function leaves the "Tell to
    /// &lt;selected&gt;" row alone for an Olthoi character and the reset disables it. Do not unify
    /// the two sets. \[verified\]
    ///
    /// Returns whether it fell back to focus 1, which is the player-visible half.
    pub fn enable_selection(&mut self, ui: &mut UiSystem, focus: u32, enable: bool) -> bool {
        if talk_focus(focus).is_none() {
            return false;
        }
        // The sender already changed the communication system's global mask before
        // this notice. Keep its local readback current even when the row/Olthoi guard returns.
        self.enabled[focus as usize] = enable;
        // A bound menu uses the real row lookup's missing-row guard. Preserve the existing
        // unbound model form for callers without a DAT tree, but never apply its virtual row
        // side effects when an actual subscribed menu has lost this row.
        if self.menu.is_some() && self.menu_item(ui, focus).is_none() {
            return false;
        }
        if self.is_olthoi && !matches!(focus, 1 | 2 | OLTHOI_FOCUS) {
            self.set_row_state(ui, focus, STATE_DISABLED);
            return false;
        }
        if enable != (self.row_state(ui, focus) == Some(STATE_DISABLED)) {
            return false;
        }
        self.set_talk_focus_enabled(focus, enable);
        self.set_row_state(
            ui,
            focus,
            if enable {
                STATE_ENABLED
            } else {
                STATE_DISABLED
            },
        );
        if !enable && self.talk_focus == focus {
            self.handle_selection(ui, DEFAULT_TALK_FOCUS);
            return true;
        }
        false
    }

    /// The host supplies the current communication globals once for a newly constructed
    /// subscriber. The panel's post-init ends by setting no selected target and selecting focus 1,
    /// whose talk-focus button reset reads those globals. This is initialization, never a replay of old subscriber notices.
    pub fn initialize_communication_state(
        &mut self,
        ui: &mut UiSystem,
        enabled: [bool; 14],
        is_olthoi: bool,
    ) -> Option<TalkFocusChange> {
        self.menu?;
        self.enabled = enabled;
        self.is_olthoi = is_olthoi;
        self.set_selected(ui, None);
        self.handle_selection(ui, DEFAULT_TALK_FOCUS)
    }

    /// Whether talk focus `n` is enabled — the plain predicate, and **nothing
    /// else**.
    ///
    /// **There is no Olthoi override here.** It belongs to the talk-focus button reset, which is
    /// where the client puts it — it writes a *state* onto the row and never consults this
    /// predicate at all on that path — and its set is **1 and 13**, not 1, 2 and 13:
    ///
    /// For an Olthoi character, focus 13 or 1 selects enabled state 1; every other
    /// focus selects disabled state `0x0D`. Other characters use the ordinary
    /// talk-focus-enabled predicate.
    ///
    /// So an Olthoi character cannot "Tell to &lt;selected&gt;" from the menu. \[verified\]
    #[must_use]
    pub fn is_talk_focus_enabled(&self, focus: u32) -> bool {
        self.enabled.get(focus as usize).copied().unwrap_or(false)
    }

    /// The row's own state, which is what the selection handler guards on. `None` when there is
    /// no such row.
    #[must_use]
    pub fn talk_focus_row_state(&self, focus: u32) -> Option<dereth_ui::StateId> {
        talk_focus(focus)?;
        self.row_state.get(focus as usize).copied()
    }

    /// The main chat panel's reset all talk focus menu buttons, whole:
    ///
    /// The client walks every row, skips a row with no focus attribute, and sets each remaining
    /// row's state as above; it then calls its set-selected with the last speakable target.
    ///
    /// That trailing set-selected is the client's own and is **not** run here: it needs the
    /// object's name, which lives on the far side of this crate's seam, and running it from inside
    /// [`Self::handle_selection`] would recurse. [`Self::set_selected`] is the caller's to run.
    pub fn reset_all_talk_focus_menu_buttons(&mut self, ui: &mut UiSystem) {
        for row in TALK_FOCUS_MENU {
            let on = if self.is_olthoi {
                row.id == OLTHOI_FOCUS || row.id == DEFAULT_TALK_FOCUS
            } else {
                self.is_talk_focus_enabled(row.id)
            };
            self.set_row_state(ui, row.id, if on { STATE_ENABLED } else { STATE_DISABLED });
        }
        // The client's tail: when the focus that is *current* has just been
        // disabled, the menu falls back to 1. It is what keeps a player out of a channel they
        // have left, and it is a different function from this one — kept here because this build
        // has no enable-selection caller yet.
        if self.row_state(ui, self.talk_focus) == Some(STATE_DISABLED) {
            self.talk_focus = DEFAULT_TALK_FOCUS;
            self.caption = "ID_Chat_ChatTargetMenu";
        }
    }

    /// Set the state of one focus row — the model half plus, when the row has been bound to
    /// a real element, the element half.
    fn set_row_state(&mut self, ui: &mut UiSystem, focus: u32, s: dereth_ui::StateId) {
        if let Some(slot) = self.row_state.get_mut(focus as usize) {
            *slot = s;
        }
        if let Some(h) = self.menu_item(ui, focus) {
            ui.set_state(h, s);
        }
    }

    /// The main chat panel's talk focus menu item lookup: the first row in the row list whose
    /// `0x1000000B` equals `focus`, or nothing.
    ///
    /// It walks the row list and asks each element for its own attribute. Walking the menu's
    /// *children* instead would answer `None` for all thirteen: the shipped layout has no tagged
    /// descendants at all.
    ///
    /// The array is what makes the lookup possible: all fourteen rows share element id
    /// `0x1000001E`, and the popup they live in is a **root** element rather than a child of the
    /// menu, so neither an id search nor a subtree walk from the menu can find them.
    #[must_use]
    pub fn menu_item(&self, ui: &UiSystem, focus: u32) -> Option<ElemHandle> {
        self.talk_focus_buttons
            .iter()
            .copied()
            .find(|h| crate::bind::attr_enum(ui, *h, ATTR_TALK_FOCUS) == Some(focus))
    }

    /// The state of the row [`Self::menu_item`] finds — **the element's own state**, which is the
    /// thing the client guards on (returning early at `0xD`) and the thing
    /// the talk-focus button reset writes.
    ///
    /// Falls back to [`Self::talk_focus_row_state`]'s mirror only when there is no element, which
    /// is a `MainChatPanel` that has not run [`Self::init_talk_focus_menu`].
    #[must_use]
    pub fn row_state(&self, ui: &UiSystem, focus: u32) -> Option<dereth_ui::StateId> {
        talk_focus(focus)?;
        match self.menu_item(ui, focus) {
            Some(h) => ui.node(h).map(|n| n.state),
            None => self.row_state.get(focus as usize).copied(),
        }
    }

    /// The client's message-**7** arm, by **element** —
    /// which is how the client does it.
    ///
    /// The original message-7 path clears the menu selection without broadcasting, rejects a
    /// missing chosen item, handles the squelch row specially, then reads focus property
    /// `0x1000000B` from every other row and applies that focus. The squelch row is matched **by
    /// pointer**, because it is the one row with no
    /// `0x1000000B`; every other row is matched by that attribute and never by position. Clearing
    /// the selected item first is what stops the menu keeping a highlight on the row a
    /// player just chose.
    pub fn on_menu_chosen_item(&mut self, ui: &mut UiSystem, item: ElemHandle) -> MenuRowChoice {
        if let Some(menu) = self.menu {
            // The selected item is cleared, and **not** broadcast.
            dereth_ui::widgets::menu::set_selected_item(ui, menu, None, false);
        }
        if Some(item) == self.squelch_toggle {
            return MenuRowChoice::Squelch;
        }
        let Some(focus) = crate::bind::attr_enum(ui, item, ATTR_TALK_FOCUS) else {
            return MenuRowChoice::None;
        };
        match self.handle_selection(ui, focus) {
            Some(c) => MenuRowChoice::Focus(c),
            None => MenuRowChoice::None,
        }
    }

    /// Select a talk focus — the thirteen-arm switch.
    ///
    /// With no row for the focus, or a row in state `0xD`, it returns. Otherwise it sets the talk
    /// focus and picks the matching `ID_Chat_ChatTargetMenu*` caption (table `0x10000001`), writes
    /// the caption to the chat-target button text when both exist, resets every row's state, and
    /// finally puts the chosen row in its selected state.
    ///
    /// Focus **7** is the one arm with an extra step: the wants-to-be-in-allegiance-chat flag is set
    /// *before* the talk focus.
    ///
    /// Returns `None` when the row does not exist or is disabled — which is exactly the client's
    /// two early returns, and is what stops a player selecting a channel they are not in.
    pub fn handle_selection(&mut self, ui: &mut UiSystem, focus: u32) -> Option<TalkFocusChange> {
        let row = talk_focus(focus)?;
        // The guard is the row's own state, not a flag. A row is unavailable because the
        // talk-focus button reset put it in state `0x0D`, and that is the only thing this function
        // reads. It is the *element's* state once the menu fill has made an item to have one.
        if self.row_state(ui, focus) == Some(STATE_DISABLED) {
            return None;
        }
        let wants_alleg_chat = focus == ALLEGIANCE_FOCUS;
        if wants_alleg_chat {
            self.wants_alleg_chat = true;
        }
        self.talk_focus = focus;
        self.caption = row.caption;
        self.set_chat_target_caption(ui);
        self.reset_all_talk_focus_menu_buttons(ui);
        // The last line: state `0x10000001` on the row that was picked — after the
        // reset has put every row back to 1 or 0x0D, so the selection survives it.
        self.set_row_state(ui, focus, STATE_SELECTED);
        Some(TalkFocusChange {
            focus,
            caption: row.caption,
            wants_alleg_chat,
        })
    }

    /// Write the caption to the chat-target button text — the caption a player reads
    /// on the chat-target button, resolved through the host's string service.
    pub fn set_chat_target_caption(&self, ui: &mut UiSystem) {
        let Some(h) = self.chat_target_button else {
            return;
        };
        let text = label(ui, self.caption);
        if let Some(t) = ui.text_element_mut(h) {
            t.set_text(&text);
        }
    }

    /// The client's message-**7** arm is the menu's "an item was chosen" event. It clears the
    /// selected row, treats the squelch row specially, and otherwise reads the focus from the
    /// chosen item's property `0x1000000B`. The chosen row is identified by that property, never by its
    /// position — which is why `TALK_FOCUS_MENU_ORDER` is display order and the ids are not
    /// contiguous with it.
    pub fn on_menu_chosen(&mut self, ui: &mut UiSystem, focus: u32) -> Option<TalkFocusChange> {
        self.handle_selection(ui, focus)
    }

    /// Apply the talk-focus visibility state on the live tree: light or unlight
    /// one of the four lamp buttons `0x10000522`…`0x10000525`.
    ///
    /// **The two state values are 6 and 1**, not the button `ACTIVE`/`NORMAL` pair; a lamp in the
    /// wrong state draws whatever that state draws:
    ///
    /// A visible talk-focus menu sets state 6; a hidden menu sets state 1.
    ///
    /// \[verified\]
    pub fn on_set_panel_visibility(
        ui: &mut UiSystem,
        root: ElemHandle,
        panel: ElementId,
        visible: bool,
    ) -> Option<ElementId> {
        let lamp = panel_lamp(panel)?;
        let h = ui.get_child_recursive(root, lamp)?;
        ui.set_state(
            h,
            if visible {
                STATE_LAMP_LIT
            } else {
                STATE_ENABLED
            },
        );
        Some(lamp)
    }

    // -----------------------------------------------------------------------------------------
    // The rest of `MainChat`
    // -----------------------------------------------------------------------------------------

    /// Set the selected object — the "Tell to &lt;selected&gt;" row and
    /// the squelch row both take their caption from the object the player has selected, and
    /// **focus 2 exists or does not exist** according to whether that object has a name.
    ///
    /// The name is the object's name, or empty when there is no such object.
    ///
    /// ```text
    /// empty name:  squelch row: ID_Chat_SquelchSelected(VALUE=name), state 0xD
    ///              row 2:       ID_Chat_TellToSelectedNoSelection,   state 0xD
    ///              focus 2 disabled; if it was the current focus, select focus 1
    /// with a name: squelch row: ID_Chat_SquelchSelected(VALUE=name), state 1
    ///              row 2:       ID_Chat_TellToSelected(VALUE=name),  state 1
    ///              focus 2 enabled; if the object is talkable and squelched,
    ///              the squelch row takes state 0x10000001
    /// then:        the id becomes the last speakable target
    /// ```
    ///
    /// **The state values split by receiver.** `0x0D` and `1` go to row 2 as well as the squelch
    /// row — row 2 is the row a player picks to send a tell, and without them it would stay in
    /// whatever state the reset left it — and only the talkable-and-squelched `0x10000001` goes to
    /// the squelch row alone.
    ///
    /// The **squelch** caption is `ID_Chat_SquelchSelected` in *both* arms, even the one where the
    /// name is empty — that is what the client does, not a transcription slip. \[verified\]
    ///
    /// `target` is `None` for "no selection or no name", which is the client's empty-string test.
    /// The object system is on the far side of this crate's seam, so the name, the talkable flag
    /// and the squelch flag arrive already answered.
    pub fn set_selected(&mut self, ui: &mut UiSystem, target: Option<&SpeakableTarget>) {
        let (id, name) = target.map_or((0, ""), |t| (t.id, t.name.as_str()));
        let squelch_caption = label_value(ui, "ID_Chat_SquelchSelected", name);
        if let Some(h) = self.squelch_toggle {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&squelch_caption);
            }
        }
        if name.is_empty() {
            self.set_menu_row_caption(ui, 2, "ID_Chat_TellToSelectedNoSelection", "");
            if let Some(h) = self.squelch_toggle {
                ui.set_state(h, STATE_DISABLED);
            }
            // The client — row 2's own state `0x0D`.
            self.set_row_state(ui, 2, STATE_DISABLED);
            self.set_talk_focus_enabled(2, false);
            self.set_last_speakable_target(ui, id);
            if self.talk_focus == 2 {
                self.handle_selection(ui, DEFAULT_TALK_FOCUS);
            }
            return;
        }
        self.set_menu_row_caption(ui, 2, "ID_Chat_TellToSelected", name);
        if let Some(h) = self.squelch_toggle {
            ui.set_state(h, STATE_ENABLED);
        }
        // The client — row 2's own state `1`, the partner of the one above.
        self.set_row_state(ui, 2, STATE_ENABLED);
        self.set_talk_focus_enabled(2, true);
        if let Some(t) = target {
            if t.id != 0 && t.talkable && t.squelched {
                if let Some(h) = self.squelch_toggle {
                    ui.set_state(h, STATE_SELECTED);
                }
            }
        }
        self.set_last_speakable_target(ui, id);
    }

    /// Write the last speakable target. It belongs to the communication system, not to this
    /// window: the line typed with talk focus 2 is told to it, so the write goes to the session's
    /// chat state as well as this copy.
    fn set_last_speakable_target(&mut self, ui: &mut UiSystem, id: u32) {
        self.last_speakable_target = id;
        ui.requests
            .emit(crate::view::UiRequest::SetLastSpeakableTarget {
                object: dereth_primitives::ObjectId(id),
            });
    }

    /// Set one focus row's caption, with the string's `VALUE` variable
    /// filled in. A no-op while the rows are modelled rather than instantiated.
    fn set_menu_row_caption(&mut self, ui: &mut UiSystem, focus: u32, token: &str, value: &str) {
        let text = label_value(ui, token, value);
        if let Some(h) = self.menu_item(ui, focus) {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&text);
            }
        }
    }

    /// The main chat panel's toggle squelch on current speakable target — the menu's **row 0**,
    /// which is the one row with no `0x1000000B` and therefore the one
    /// the client's message-7 arm matches by *identity* (the chosen item is the squelch
    /// toggle).
    ///
    /// With no last speakable target, or no such object, it returns. Otherwise it reads whether
    /// the target is squelched (empty account, message type 1), sets the squelch row's state, and
    /// sends the character-squelch change `(add = !squelched, object = id, account = "", 1)`.
    ///
    /// The flag sent is the **negation** of the current state, which is what makes the row a
    /// toggle, and the account/message-type arguments are the empty string and 1 — "this character,
    /// all message types". Returns the event, or `None` when there is nothing selected. The
    /// squelched/not-squelched pair of state sets share one tail in the client, so the row's own
    /// appearance is left to [`Self::set_selected`], which sets it from the same predicate.
    ///
    /// It returns a value rather than raising a [`crate::view::UiRequest`] because there is no
    /// squelch request in this build; the caller sends the event.
    #[must_use]
    pub fn toggle_squelch_on_current_speakable_target(
        &self,
        currently_squelched: bool,
    ) -> Option<ModifyCharacterSquelch> {
        if self.last_speakable_target == 0 {
            return None;
        }
        Some(ModifyCharacterSquelch {
            object: self.last_speakable_target,
            add: !currently_squelched,
            account: String::new(),
            message_type: 1,
        })
    }

    /// The window grows to attribute `0x3E`'s
    /// height and shrinks back to the one it had.
    ///
    /// It needs the maximize button `0x1000046F` and the window's integer attributes `0x3C` (the
    /// ceiling) and `0x3E`, and returns if any is missing. With `parentH` the parent's height and
    /// `y`/`h` the window's own, the grown y is `y` when `y + h < parentH` and `y < parentH/2`, and
    /// `y - parentH/2` otherwise.
    ///
    /// * **Restore** (currently maximized): the old height is clamped to `[0x3E, 0x3C]`, the old
    ///   y to `[0, parentH - 0x3E]`; the button takes its restored state and the window is resized
    ///   to the old height and moved to the old y.
    /// * **Maximize:** the old y and old height are remembered, the button takes its maximized
    ///   state, the grown y is floored at 0 and the grown height capped at `parentH - grownY`, and
    ///   the window is resized and moved there.
    ///
    /// Both attribute reads are hard returns: a window whose description carries
    /// neither `0x3C` nor `0x3E` has an inert maximize button.
    ///
    /// # Three details that are easy to get wrong, verified against retail
    ///
    /// A plausible misreading makes this button **move** the whole window up by exactly 300 px —
    /// top border client y 503 → 203, "Send" centre y 587 → 287 — with its height unchanged.
    /// The 300 itself is right (the client's own `y - parentH/2`); these three are what keep the
    /// height and the restore correct:
    ///
    /// 1. **The grown height is the window's height plus half the parent's, not attribute `0x3E`.**
    ///    That sum is what the client resizes to. The shipped `0x10000601` is 410 × **100** with
    ///    `0x3E = 100`, so resizing to `0x3E` would be a **no-op** and leave only the move. The
    ///    sum is 100 + 300 = **400**, and it pairs
    ///    with `grownY = y - parentH/2` to keep the bottom edge where it is while the top rises.
    ///    `0x3E` is what it is used for in the *restore* branch: the **floor** the old height is
    ///    clamped up to, with `0x3C` the ceiling.
    /// 2. **The old y is the window's own `y0`, not the grown y.** The client stores the window's
    ///    own y whichever way the grown-y choice goes; remembering the grown y would make restore
    ///    write the grown position back.
    /// 3. **The button's two states are `0x10000047` and `0x10000048`**, set on maximize and
    ///    restore respectively.
    ///
    /// Returns the new `(y, height)` if it moved.
    ///
    /// `window` is the main window's own [`crate::chat::window::ChatWindow`], because the client's
    /// resize goes through the window's own resize override and lands in
    /// `crate::chat::window::resize_to`, which keeps the log on its last line across
    /// the change.
    pub fn handle_maximize_button(
        &mut self,
        ui: &mut UiSystem,
        root: ElemHandle,
        window: &crate::chat::window::ChatWindow,
    ) -> Option<(i32, i32)> {
        let btn = self
            .maximize
            .or_else(|| ui.get_child_recursive(root, main_chat::MAXIMIZE_BUTTON))?;
        let max_h = crate::bind::attr_int(ui, root, attr::MAX_HEIGHT)?;
        let min_h = crate::bind::attr_int(ui, root, attr::MAXIMIZED_HEIGHT)?;
        let me = ui.screen_box(root);
        let parent_h = ui.parent(root).map_or(0, |p| ui.screen_box(p).height());
        let (x, y, w, h) = (me.x0, me.y0, me.width(), me.height());
        // The client: the grown height is this height plus half the parent's, computed before the
        // branch and clamped after it.
        let grown_h = h + parent_h / 2;
        let grown_y = if y + h < parent_h && y < parent_h / 2 {
            y
        } else {
            y - parent_h / 2
        };

        let (new_y, new_h, state) = if self.maximized {
            self.maximized = false;
            self.old_height = self.old_height.max(min_h).min(max_h);
            self.old_y = self.old_y.max(0).min(parent_h - min_h);
            (self.old_y, self.old_height, main_chat::MAXIMIZE_RESTORED)
        } else {
            self.maximized = true;
            self.old_y = y;
            self.old_height = h;
            let ny = grown_y.max(0);
            let nh = if parent_h < grown_h + ny {
                parent_h - ny
            } else {
                grown_h
            };
            (ny, nh, main_chat::MAXIMIZE_MAXIMIZED)
        };
        ui.set_state(btn, state);
        window.resize_to(ui, root, w, new_h);
        ui.move_to(root, x, new_y);
        Some((new_y, new_h))
    }

    /// The once-a-second auto-target sweep that keeps
    /// "Tell to &lt;selected&gt;" pointing at something a player can actually talk to.
    ///
    /// The original update uses one file-scope next-run time, so all five chat windows share a
    /// once-per-second throttle even though only the main window performs this sweep. With a current
    /// tell target, it keeps an item owned by the player, otherwise checks range to its containing
    /// object when one exists and clears an out-of-range target. Without a tell target, it adopts
    /// the selected object only when that object exists, is not the player, is talkable, and is in
    /// radar range. The containing-object hop is significant: an item inside a chest remains valid
    /// according to the *chest*'s distance. Every completed sweep advances the shared time by 1.0.
    ///
    /// This is the decision, separated from the object queries so it can be tested against values.
    #[must_use]
    pub fn use_time_auto_target(&self, w: &AutoTargetWorld) -> AutoTarget {
        if self.is_talk_focus_enabled(2) {
            if self.last_speakable_target == 0 {
                return AutoTarget::Unchanged;
            }
            if w.owned_by_player {
                return AutoTarget::Unchanged;
            }
            let ask = if w.container_id == 0 {
                self.last_speakable_target
            } else {
                w.container_id
            };
            if w.in_range_of_player.contains(&ask) {
                return AutoTarget::Unchanged;
            }
            return AutoTarget::Clear;
        }
        if w.selected_id == 0 || w.selected_id == w.player_id {
            return AutoTarget::Unchanged;
        }
        if !w.selected_talkable || !w.in_range_of_player.contains(&w.selected_id) {
            return AutoTarget::Unchanged;
        }
        AutoTarget::Adopt(w.selected_id)
    }
}

/// Which of the fourteen rows a click landed on — the three outcomes of
/// the client's message-7 arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuRowChoice {
    /// The item was the squelch toggle, matched by identity:
    /// [`MainChatPanel::toggle_squelch_on_current_speakable_target`].
    Squelch,
    /// The item carried `0x1000000B` and [`MainChatPanel::handle_selection`] accepted it.
    Focus(TalkFocusChange),
    /// No item, no attribute, or a row in state `0x0D` — the client's three `break`s.
    None,
}

/// The character-squelch change `(add, object, account, message_type)` — what
/// the squelch row asks the server for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifyCharacterSquelch {
    pub object: u32,
    /// Not currently squelched, which is what makes the row a toggle.
    pub add: bool,
    /// The account name — the empty string, i.e. "this character only".
    pub account: String,
    /// `1`, i.e. all message types.
    pub message_type: u32,
}

/// The object one call is about, with the three questions the client asks
/// the object system already answered.
///
/// Defined in [`dereth_client_contract::chat::mainchat`], beside [`AutoTargetWorld`].
pub use dereth_client_contract::chat::mainchat::SpeakableTarget;

/// What `use_time_auto_target` needs to know about the world.
///
/// Defined, with its one method, in [`dereth_client_contract::chat::mainchat`], because
/// `dereth_client::hud` is what fills it -- the selected id, the player id, the talkable and
/// ownership answers and the in-range sweep are all the object table's.
pub use dereth_client_contract::chat::mainchat::AutoTargetWorld;

/// What one per-frame auto-target sweep decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoTarget {
    /// No set-selected call this second.
    Unchanged,
    /// Set-selected with 0 — the target went out of range.
    Clear,
    /// Set-selected with the id — the selection was adopted.
    Adopt(u32),
}

/// The auto-target throttle: a shared "next time" set to the current time plus 1.0.
pub const AUTO_TARGET_INTERVAL_SECONDS: f64 = 1.0;

/// The two element attributes the resize path reads, returning early when either is absent.
pub mod attr {
    /// `0x3C` — the ceiling the old height is clamped to on restore.
    pub const MAX_HEIGHT: u32 = 0x3C;
    /// `0x3E` — the height the window grows to.
    pub const MAXIMIZED_HEIGHT: u32 = 0x3E;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered chat behavior's thirteen-row talk-focus table — ids 1…13 in menu order,
    /// with `0x0B`, `0x0C`, `0x0D` written in hex there exactly as here.
    #[test]
    fn the_talk_focus_menu_is_thirteen_rows_numbered_one_to_thirteen() {
        assert_eq!(TALK_FOCUS_MENU.len(), 13);
        let ids: Vec<u32> = TALK_FOCUS_MENU.iter().map(|t| t.id).collect();
        assert_eq!(ids, (1..=13).collect::<Vec<u32>>());
        assert_eq!(talk_focus(1).unwrap().item_label, "ID_Chat_TellToAll");
        assert_eq!(
            talk_focus(OLTHOI_FOCUS).unwrap().item_label,
            "ID_Chat_TellToOlthoi"
        );
        assert_eq!(
            talk_focus(ALLEGIANCE_FOCUS).unwrap().caption,
            "ID_Chat_ChatTargetMenuAllegiance"
        );
        assert_eq!(talk_focus(99), None);
        assert_eq!(ATTR_TALK_FOCUS, 0x1000_000B);
    }

    /// Oracle: §6.4's last paragraph — the player-id window the action handler tests before starting a
    /// tell to the selection.
    #[test]
    fn the_player_object_id_window_is_exclusive_at_both_ends() {
        assert!(!selection_is_a_player(0x5000_0000));
        assert!(selection_is_a_player(0x5000_0001));
        assert!(selection_is_a_player(0x6FFF_FFFF));
        assert!(!selection_is_a_player(0x7000_0000));
        assert!(!selection_is_a_player(0x8000_0000));
    }

    /// Oracle: §6.1 and §6.4's action tables.
    #[test]
    fn the_chat_input_actions_are_the_documented_ids() {
        assert_eq!(action::TOGGLE_ENTRY, 0x1000_0024);
        assert_eq!(action::ACTIVATE_ENTRY, 0x1000_0023);
        assert_eq!(action::REPLY, 0x1000_0020);
        assert_eq!(action::REPLY_MONARCH, 0x1000_0021);
        assert_eq!(action::REPLY_PATRON, 0x1000_0022);
        assert_eq!(action::COMMAND_OR_ALIAS, [0x1000_0028, 0x1000_0119]);
    }

    /// A `MainChatPanel` with no live tree behind it: every method here writes the model, and the
    /// element half is a no-op because the shipped layout carries no menu rows yet.
    fn detached() -> (UiSystem, MainChatPanel) {
        (UiSystem::new((800, 600)), MainChatPanel::default())
    }

    /// The menu row states are the four the binary pushes.
    #[test]
    fn the_menu_row_states_are_the_four_the_binary_pushes() {
        assert_eq!(STATE_ENABLED.0, 1);
        assert_eq!(STATE_DISABLED.0, 0x0D);
        assert_eq!(STATE_SELECTED.0, 0x1000_0001);
        assert_eq!(STATE_LAMP_LIT.0, 6);
        // ...and they are not the generic element states, which is what they were guessed as.
        assert_ne!(STATE_LAMP_LIT, dereth_ui::element::state::ACTIVE);
        assert_ne!(STATE_ENABLED, dereth_ui::element::state::NORMAL);
    }

    /// A row is refused because its state is thirteen.
    #[test]
    fn a_row_is_refused_because_its_state_is_thirteen() {
        let (mut ui, mut m) = detached();
        m.reset_all_talk_focus_menu_buttons(&mut ui);
        // 1 and 2 are the two `ChatState::new` enables; everything else is off.
        assert_eq!(m.talk_focus_row_state(1), Some(STATE_ENABLED));
        assert_eq!(m.talk_focus_row_state(8), Some(STATE_DISABLED));
        assert_eq!(m.talk_focus_row_state(99), None, "there is no row 99");

        assert!(
            m.handle_selection(&mut ui, 8).is_none(),
            "state 0x0D refuses"
        );
        m.set_talk_focus_enabled(8, true);
        assert!(m.handle_selection(&mut ui, 8).is_some(), "state 1 admits");
        assert_eq!(m.talk_focus, 8);
        // …and the chosen row ends in `0x10000001`, after the reset has been through it.
        assert_eq!(m.talk_focus_row_state(8), Some(STATE_SELECTED));

        // **The discriminating direction.** Make the mask and the state disagree — which is
        // exactly what `enable_selection`'s Olthoi arm does, and what `set_selected` does to row 2 —
        // and it is the *state* that refuses. A guard written against the mask would admit here.
        m.set_talk_focus_enabled(9, true);
        m.row_state[9] = STATE_DISABLED;
        assert!(m.is_talk_focus_enabled(9), "the flag says yes");
        assert!(
            m.handle_selection(&mut ui, 9).is_none(),
            "and the state says no, which wins"
        );
        assert_eq!(m.talk_focus, 8, "…so the focus did not move");
    }

    /// The two Olthoi guards are different sets and focus two is what separates them.
    #[test]
    fn the_two_olthoi_guards_are_different_sets_and_focus_two_is_what_separates_them() {
        let (mut ui, mut m) = detached();
        m.is_olthoi = true;
        for f in 1..=13 {
            m.set_talk_focus_enabled(f, true);
        }
        m.reset_all_talk_focus_menu_buttons(&mut ui);
        assert_eq!(m.talk_focus_row_state(1), Some(STATE_ENABLED));
        assert_eq!(m.talk_focus_row_state(13), Some(STATE_ENABLED));
        assert_eq!(
            m.talk_focus_row_state(2),
            Some(STATE_DISABLED),
            "the reset's set is {{1, 13}}"
        );
        assert_eq!(m.talk_focus_row_state(3), Some(STATE_DISABLED));

        // `enable_selection`'s own set includes 2, so it does *not* take the "force 0x0D" arm and
        // its ordinary state change goes through.
        m.enable_selection(&mut ui, 2, true);
        assert_eq!(
            m.talk_focus_row_state(2),
            Some(STATE_ENABLED),
            "enable_selection's is {{1,2,13}}"
        );
        // Focus 3 is outside both sets: the Olthoi arm forces it back down whatever is asked.
        m.enable_selection(&mut ui, 3, true);
        assert_eq!(m.talk_focus_row_state(3), Some(STATE_DISABLED));
    }

    /// Oracle: the client's tail — disabling the focus a player is *on* falls
    /// back to 1, and the guard that acts only when the row's state really changes makes a
    /// no-change call do nothing.
    #[test]
    fn losing_the_channel_you_are_talking_on_drops_you_back_to_say() {
        let (mut ui, mut m) = detached();
        m.set_talk_focus_enabled(6, true);
        m.handle_selection(&mut ui, 6).expect("Vassals");
        assert_eq!(m.talk_focus, 6);
        assert!(
            m.enable_selection(&mut ui, 6, false),
            "left the allegiance -> back to Say"
        );
        assert_eq!(m.talk_focus, DEFAULT_TALK_FOCUS);
        assert_eq!(m.caption, "ID_Chat_ChatTargetMenu");
        // A second disable is a no-change call and returns without touching anything.
        assert!(!m.enable_selection(&mut ui, 6, false));
    }

    /// Oracle: the client's two arms and the client's set-selected with 0.
    #[test]
    fn a_selection_with_a_name_is_what_turns_focus_two_on() {
        let (mut ui, mut m) = detached();
        m.set_selected(&mut ui, None);
        assert!(!m.is_talk_focus_enabled(2), "SetTalkFocusEnabled(2, false)");
        assert_eq!(m.talk_focus_row_state(2), Some(STATE_DISABLED));
        assert_eq!(m.last_speakable_target, 0);

        let t = SpeakableTarget {
            id: 0x5000_1234,
            name: "Alba".into(),
            talkable: true,
            squelched: false,
        };
        m.set_selected(&mut ui, Some(&t));
        assert!(m.is_talk_focus_enabled(2));
        assert_eq!(m.talk_focus_row_state(2), Some(STATE_ENABLED));
        assert_eq!(m.last_speakable_target, 0x5000_1234);

        // …and losing it while talking to it drops the focus back to 1.
        m.handle_selection(&mut ui, 2).expect("Tell to selected");
        assert_eq!(m.talk_focus, 2);
        m.set_selected(&mut ui, None);
        assert_eq!(
            m.talk_focus, DEFAULT_TALK_FOCUS,
            "talk focus 2 (the selection) falls back to focus 1"
        );
        assert_eq!(m.last_speakable_target, 0);
    }

    /// Oracle: the toggle squelch on current speakable target — the character-squelch change
    /// `(add = !squelched, object = id, account = "", 1)`, behind the early return when there is no
    /// last speakable target.
    #[test]
    fn the_squelch_row_sends_the_negation_of_the_current_state() {
        let (mut ui, mut m) = detached();
        assert_eq!(
            m.toggle_squelch_on_current_speakable_target(false),
            None,
            "nothing selected"
        );

        let t = SpeakableTarget {
            id: 0x5000_0009,
            name: "Alba".into(),
            talkable: true,
            squelched: false,
        };
        m.set_selected(&mut ui, Some(&t));
        let on = m
            .toggle_squelch_on_current_speakable_target(false)
            .expect("selected");
        assert_eq!(
            on,
            ModifyCharacterSquelch {
                object: 0x5000_0009,
                add: true,
                account: String::new(),
                message_type: 1,
            }
        );
        let off = m
            .toggle_squelch_on_current_speakable_target(true)
            .expect("selected");
        assert!(!off.add, "an already-squelched target is un-squelched");
    }

    /// Oracle: the client's two arms, quoted in the method's own documentation.
    ///
    /// The container hop is the branch nothing else in the row would have found: an item held in
    /// a chest is kept by the **chest**'s distance, not its own.
    #[test]
    fn the_auto_target_keeps_a_held_item_by_its_container_and_drops_what_walks_away() {
        let (mut ui, mut m) = detached();
        let t = SpeakableTarget {
            id: 40,
            name: "Alba".into(),
            talkable: true,
            squelched: false,
        };
        m.set_selected(&mut ui, Some(&t));
        assert!(
            m.is_talk_focus_enabled(2),
            "the first arm is the one under test"
        );

        // In range: nothing happens.
        let w = AutoTargetWorld {
            in_range_of_player: vec![40],
            ..AutoTargetWorld::default()
        };
        assert_eq!(m.use_time_auto_target(&w), AutoTarget::Unchanged);
        // Out of range: cleared.
        let w = AutoTargetWorld::default();
        assert_eq!(m.use_time_auto_target(&w), AutoTarget::Clear);
        // Out of range but inside a container that is in range: kept.
        let w = AutoTargetWorld {
            container_id: 77,
            in_range_of_player: vec![77],
            ..AutoTargetWorld::default()
        };
        assert_eq!(m.use_time_auto_target(&w), AutoTarget::Unchanged);
        // In our own pack: kept whatever the distance says.
        let w = AutoTargetWorld {
            owned_by_player: true,
            ..AutoTargetWorld::default()
        };
        assert_eq!(m.use_time_auto_target(&w), AutoTarget::Unchanged);

        // The second arm: no speakable target, so the selection is adopted if it can be talked to.
        m.set_selected(&mut ui, None);
        let mut w = AutoTargetWorld {
            selected_id: 12,
            player_id: 9,
            selected_talkable: true,
            in_range_of_player: vec![12],
            ..AutoTargetWorld::default()
        };
        assert_eq!(m.use_time_auto_target(&w), AutoTarget::Adopt(12));
        w.selected_talkable = false;
        assert_eq!(
            m.use_time_auto_target(&w),
            AutoTarget::Unchanged,
            "a rock is not talkable"
        );
        w.selected_talkable = true;
        w.selected_id = 9;
        assert_eq!(
            m.use_time_auto_target(&w),
            AutoTarget::Unchanged,
            "never ourselves"
        );
        assert_eq!(AUTO_TARGET_INTERVAL_SECONDS, 1.0);
    }

    /// Oracle: the client's two integer attribute reads, both of which
    /// return early when the attribute is absent.
    #[test]
    fn the_maximize_button_is_inert_without_its_two_attributes() {
        assert_eq!(attr::MAX_HEIGHT, 0x3C);
        assert_eq!(attr::MAXIMIZED_HEIGHT, 0x3E);
    }
}
