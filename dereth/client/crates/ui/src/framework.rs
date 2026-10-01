//! `UiFlow` and the mode machine.
//!
//! The client's entire life above the main loop is a **one-deep state machine**. A UI mode is an
//! id bound at startup to a no-argument screen factory; at most one screen
//! exists at a time. Anything anywhere queues a mode id ([`UiFlow::queue`]), which only records
//! the next mode;
//! the switch itself happens once per frame in the flow update, driven by **global message 3**
//! from the UI system's per-frame update.
//!
//! If you split the tick from the transition, the transition must still run **last**,
//! because `use_new_mode` destroys the current screen and then resets the dialog factory, so every
//! open dialog dies with a mode switch.

use std::collections::BTreeMap;

use dereth_primitives::{AssetSource, DataId, LocalTime};

use crate::persist::UiPersistentData;
use crate::{ElemHandle, ElementId, MessageId, NoticeId, NoticePayload, UiError, UiSystem};

/// A layout enum, the id a screen passes to `create_and_add_root_element`.
///
/// The documented range is `0x10000001`–`0x10000039`; the shipped mapper also carries
/// `0x10000092`–`0x100000A5` for the ToD-era panels and the small ids 1 and 2 for the `ContextMenu`
/// and `Dialog` template layouts.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutEnum(pub u32);

impl std::fmt::Debug for LayoutEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LayoutEnum(0x{:X})", self.0)
    }
}

/// Turn a layout enum into a `UI_LAYOUT` (dat type 0x23) DataID.
///
/// **Never hard-code DataIDs at the call site.** Hard-coding works against this dat build and
/// breaks on any other, which matters because the DDD patch path can replace layouts. Keeping the
/// indirection behind one trait keeps the lookup a one-file change.
pub trait LayoutEnumResolver: std::fmt::Debug {
    fn resolve(&self, e: LayoutEnum) -> Option<DataId>;
}

/// The `DidMapper` that answers the question, and the group id that selects it.
///
/// The framework's root-element creation asks for the layout's DataID by enum, which performs
/// **two** enum-to-DataID lookups: one
/// against the master `DID_MAPPER` selected by the cache's master-map ID and one against the mapper that
/// returned. The code alone does not show the first lookup's key argument.
///
/// **It is answered by the data.** The master mapper is `DidMapper 0x25000000`, whose entry **5** is named
/// `UILAYOUT` and points at [`UI_LAYOUT_MAPPER`]. Dumping that mapper gives 82 entries whose keys
/// are exactly the layout enums and whose values are exactly the `0x21xxxxxx` layouts — enum
/// `0x10000001` → `0x21000000` `patch`, which is the layout `DataPatchScreen` (the first mode) loads.
pub const UI_LAYOUT_GROUP: u32 = 5;

/// Group **4** of the same master mapper, `DidMapper 0x25000004`: the string tables.
///
/// Fifteen entries in the retail dat; enum `0x10000002` — the table
/// the data-patch screen builds its `StringInfo` in — is `0x23000002`.
pub const STRING_TABLE_GROUP: u32 = 4;

/// `DidMapper 0x2500000E`, named `UILAYOUT` by the master mapper.
pub const UI_LAYOUT_MAPPER: DataId = DataId(0x2500_000E);

/// Read the mapping out of the dat, which is what the client does and what survives a DDD patch.
#[derive(Debug)]
pub struct DidMapperResolver {
    map: BTreeMap<LayoutEnum, DataId>,
}

impl DidMapperResolver {
    /// Load `DidMapper 0x2500000E`.
    ///
    /// A caller that wants the full two-level lookup should first read the master mapper
    /// (`0x25000000`) and take entry [`UI_LAYOUT_GROUP`]; [`Self::load_via_master`] does that.
    pub fn load(assets: &dyn AssetSource) -> Result<Self, UiError> {
        Self::load_from(assets, UI_LAYOUT_MAPPER)
    }

    /// The full two-level lookup: master mapper → the UI-layout mapper → the layout DataID.
    pub fn load_via_master(assets: &dyn AssetSource) -> Result<Self, UiError> {
        Self::load_group(assets, UI_LAYOUT_GROUP)
    }

    /// The enum-to-DataID lookup for **any** group of the master mapper.
    ///
    /// The two-level lookup is not specific to layouts: group 4 (`STRINGTABLE`) is what turns
    /// the data-patch UI's table enum `0x10000002` into `0x23000002`, and group 9
    /// (`FONT`) is the font-name table read when setting a glyph's font. Dumped from the retail
    /// `DidMapper 0x25000000`: group 4 → `0x25000004` (15 string tables) and group 5 →
    /// `0x2500000E` (82 layouts).
    ///
    /// # Errors
    /// [`UiError`] when the master mapper is unreadable or has no such group.
    pub fn load_group(assets: &dyn AssetSource, group: u32) -> Result<Self, UiError> {
        let master = Self::read_mapper(assets, DataId(0x2500_0000))?;
        let second = master
            .enum_to_id
            .iter()
            .find(|(k, _)| *k == group)
            .map(|(_, v)| DataId(*v))
            .ok_or_else(|| {
                UiError::Persist(format!("master DidMapper 0x25000000 has no entry {group}"))
            })?;
        Self::load_from(assets, second)
    }

    fn load_from(assets: &dyn AssetSource, did: DataId) -> Result<Self, UiError> {
        let m = Self::read_mapper(assets, did)?;
        Ok(Self {
            map: m
                .enum_to_id
                .into_iter()
                .filter(|(_, v)| *v != 0)
                .map(|(k, v)| (LayoutEnum(k), DataId(v)))
                .collect(),
        })
    }

    fn read_mapper(
        assets: &dyn AssetSource,
        did: DataId,
    ) -> Result<dereth_assets::DidMapper, UiError> {
        use dereth_assets::Decode;
        let bytes = assets.read(did).map_err(|e| UiError::Layout {
            did,
            reason: format!("asset source: {e}"),
        })?;
        dereth_assets::DidMapper::decode_payload(did, &bytes).map_err(|e| UiError::Layout {
            did,
            reason: e.to_string(),
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl LayoutEnumResolver for DidMapperResolver {
    fn resolve(&self, e: LayoutEnum) -> Option<DataId> {
        self.map.get(&e).copied()
    }
}

/// A hand-built table, for tests and for a host with no dat.
#[derive(Debug, Default, Clone)]
pub struct TableResolver(pub BTreeMap<LayoutEnum, DataId>);

impl TableResolver {
    /// The mapping this dat build carries, transcribed from `DidMapper 0x2500000E` for the
    /// contiguous `0x10000001`–`0x1000003B` block of screen and dialog layout enums.
    ///
    /// It is a *convenience for tests*, not the production path: [`DidMapperResolver`] reads the
    /// same table out of the dat, and a DDD patch that moves a layout moves it there too.
    #[must_use]
    pub fn retail_prefix() -> Self {
        let mut m = BTreeMap::new();
        // 1 -> ContextMenu, 2 -> Dialog, then 0x10000001.. runs 0x21000000.. contiguously as far
        // as chargen_mainmenu.
        m.insert(LayoutEnum(1), DataId(0x2100_003B));
        m.insert(LayoutEnum(2), DataId(0x2100_003C));
        for i in 0..=0x3A_u32 {
            m.insert(LayoutEnum(0x1000_0001 + i), DataId(0x2100_0000 + i));
        }
        Self(m)
    }
}

impl LayoutEnumResolver for TableResolver {
    fn resolve(&self, e: LayoutEnum) -> Option<DataId> {
        self.0.get(&e).copied()
    }
}

/// A UI mode id.
///
/// The definition is [`dereth_client_contract::ids`]'s, because `UiRequest::QueueMode`
/// carries one and the contract crate may not depend on this one. The [`mode`] table below, which
/// is `UiFlow`'s registration order, is this crate's.
pub use dereth_client_contract::ids::UiMode;

/// The eight modes registers, in registration order.
///
/// `0x10000004`, `0x10000006` and `0x10000007` are **not registered** in this build; queueing one
/// leaves no current screen.
pub mod mode {
    use super::UiMode;
    /// `DataPatchScreen` — dat patching. **Queued first**, by the flow's own constructor.
    pub const DATA_PATCH: UiMode = UiMode(0x1000_0003);
    /// `IntroScreen` — the intro splash sequence.
    pub const INTRO: UiMode = UiMode(0x1000_0001);
    /// `CharacterManagementScreen` — character select / create / delete.
    pub const CHARACTER_MANAGEMENT: UiMode = UiMode(0x1000_000A);
    /// `GamePlayScreen` — the in-game HUD.
    pub const GAME_PLAY: UiMode = UiMode(0x1000_0008);
    /// `EpilogueScreen` — log off and quit.
    pub const EPILOGUE: UiMode = UiMode(0x1000_0009);
    /// `DisconnectedScreen` — "you have been disconnected".
    pub const DISCONNECTED: UiMode = UiMode(0x1000_0002);
    /// `CharGenScreen` — the character-generation wizard.
    pub const CHAR_GEN: UiMode = UiMode(0x1000_000B);
    /// `CreditsScreen` — the scrolling credits.
    pub const CREDITS: UiMode = UiMode(0x1000_0005);

    /// In `UiFlow`'s registration order.
    pub const REGISTRATION_ORDER: [UiMode; 8] = [
        DATA_PATCH,
        INTRO,
        CHARACTER_MANAGEMENT,
        GAME_PLAY,
        EPILOGUE,
        DISCONNECTED,
        CHAR_GEN,
        CREDITS,
    ];
}

/// What every [`Screen`] method is handed: the UI the screen lives in and the game it reads.
///
/// The UI carries the rest of what a screen needs from its host: the request queue
/// ([`UiSystem::requests`]), the inbound notices ([`UiSystem::notice_inbox`]) and the asset
/// environment ([`UiSystem::env`]). They are on the `UiSystem` rather than here because element
/// behaviours and the panels a screen calls reach them too, through the `&mut UiSystem` they are
/// handed, and one queue in emission order is the contract.
pub struct ScreenCx<'a> {
    /// The UI the screen's elements live in.
    pub ui: &'a mut UiSystem,
    /// The read-only view of the game.
    pub view: &'a dyn dereth_client_contract::view::GameView,
}

impl<'a> ScreenCx<'a> {
    /// A context over `ui` with no game behind it ([`dereth_client_contract::view::EmptyGameView`]): what
    /// a pre-game screen, and a test that drives a screen on its own, is handed.
    pub fn new(ui: &'a mut UiSystem) -> Self {
        Self {
            ui,
            view: &dereth_client_contract::view::EmptyGameView,
        }
    }

    /// A context over `ui` reading `view`.
    pub fn with_view(
        ui: &'a mut UiSystem,
        view: &'a dyn dereth_client_contract::view::GameView,
    ) -> Self {
        Self { ui, view }
    }

    /// The same context, reborrowed for a nested call.
    pub fn reborrow(&mut self) -> ScreenCx<'_> {
        ScreenCx {
            ui: &mut *self.ui,
            view: self.view,
        }
    }
}

impl std::fmt::Debug for ScreenCx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScreenCx").finish_non_exhaustive()
    }
}

/// What the host hands [`Screen::on_pregame`]: the pre-game view and the facts the flow and the
/// shell hold that a screen cannot reach itself.
pub struct PregameCx<'a> {
    /// This frame's pre-game view.
    pub view: &'a dereth_client_contract::pregame::PregameView,
    /// The persistent data's received-set flag.
    pub received_set: bool,
    /// The persistent data's character set.
    pub char_set: &'a dereth_client_contract::persist::CharacterSet,
    /// Whether the persistent character set differs from the one last delivered — the
    /// character-set notice's edge.
    pub char_set_changed: bool,
    /// The persistent data's selected avatar.
    pub selected_avatar: dereth_primitives::ObjectId,
    /// The persistent data's character-generation slot.
    pub chargen_slot: i32,
    /// Whether the char-gen verification response is a new notice this frame.
    pub chargen_response_changed: bool,
    /// String table enum `0x10000002` resolved to its table, where the pre-game screens' captions
    /// and the data-patch status lines live. `None` when the mapper had no entry.
    pub ui_strings: Option<DataId>,
    /// String table enum `0x10000001` resolved to its table, where the logout confirmations live.
    pub client_strings: Option<DataId>,
    /// Dat tables the host loaded once for the screens that share them (the character-generation
    /// data), as the screen crate's own type.
    pub tables: Option<std::rc::Rc<dyn std::any::Any>>,
}

impl std::fmt::Debug for PregameCx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PregameCx")
            .field("view", self.view)
            .finish_non_exhaustive()
    }
}

/// What a screen's action callback ([`Screen::on_mode_action`]) answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeAction {
    /// The callback consumed the action; `false` hands it on to the handler list.
    pub consumed: bool,
    /// The callback's body ran (the shell counts these per screen).
    pub handled: bool,
    /// A mode the callback asks to queue.
    pub queue: Option<UiMode>,
}

/// What the host hands [`Screen::on_game`].
///
/// The call vocabulary and the panels the host keeps across screen rebuilds belong to the screen
/// crate, which this crate sits below, so both travel type-erased and the screen accepts only its
/// own types. Every call carries owned data: its inputs, and the outputs the screen writes back.
pub struct GameCx<'a> {
    /// The call: a value of the screen crate's call type.
    pub call: &'a mut dyn std::any::Any,
    /// State the host owns and lends for the call (the HUD's panel set), as the screen crate's own
    /// type; `None` when the call needs none.
    pub lent: Option<&'a mut dyn std::any::Any>,
    /// The input manager, for the key-bindings page's calls.
    pub input: Option<&'a mut dereth_input::InputManager>,
}

impl<'a> GameCx<'a> {
    /// A call with nothing lent.
    pub fn call(call: &'a mut dyn std::any::Any) -> Self {
        Self {
            call,
            lent: None,
            input: None,
        }
    }

    /// A call with `lent` lent to it.
    pub fn lending(call: &'a mut dyn std::any::Any, lent: &'a mut dyn std::any::Any) -> Self {
        Self {
            call,
            lent: Some(lent),
            input: None,
        }
    }
}

impl std::fmt::Debug for GameCx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameCx").finish_non_exhaustive()
    }
}

/// What each screen implements, one per screen framework.
///
/// # Why `Any` is a supertrait
///
/// Several screens read **process globals** rather than anything the flow hands them:
/// the data-patch screen's per-frame update asks the shared network object whether the socket is
/// connected and
/// the persistent UI data's received-set flag whether the character list has arrived;
/// `IntroScreen` drives its list of states, which the client fills from the root element's
/// description.
/// A rebuild has no globals, so the host has to reach the *concrete* screen to supply them, and
/// Screen registration takes a bare `fn() -> Box<dyn Screen>`, mirroring the original no-argument
/// factory, so there is nowhere to pass a host handle in. Naming [`std::any::Any`] as a supertrait costs no implementor anything
/// (every screen is a `'static` concrete type) and lets the host upcast and downcast at the one
/// place that needs it; see `dereth_client::ui`.
pub trait Screen: std::fmt::Debug + std::any::Any {
    /// The screen's constructor body: build the root elements. `DataPatchScreen`,
    /// `CharacterManagementScreen` and `GamePlayScreen` all do this synchronously here rather than
    /// lazily, because there is no framework at all for the duration of the constructor.
    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError>;

    /// Undo `create`. The flow removes the recorded roots itself, so this is for anything else the
    /// screen owns.
    fn destroy(&mut self, _cx: &mut ScreenCx<'_>) {}

    /// The flow's interface update reaching the screen. Returning a mode queues it.
    fn update(&mut self, _cx: &mut ScreenCx<'_>, _now: LocalTime) -> Option<UiMode> {
        None
    }

    fn on_element_message(&mut self, _cx: &mut ScreenCx<'_>, _m: &crate::ElementMessage) {}
    fn on_global_message(&mut self, _cx: &mut ScreenCx<'_>, _id: MessageId, _param: u32) {}

    /// The screen's child-action leg. `true` means consumed.
    ///
    /// [`crate::UiSystem::dispatch_child_action`] walks the arena and asks every
    /// ancestor **behaviour**; the containers this build's child-action hooks belong to —
    /// `MainChat`, `FloatingChat`, `ExaminationPanel` — are not arena behaviours here, they
    /// are fields of a [`Screen`], exactly as their element-message handlers are
    /// [`Self::on_element_message`] arms rather than element behaviours. So the screen gets the
    /// same chance, in the same place in the order: **before** the focused element's own
    /// `on_action`, because dispatch chains to the base first.
    ///
    /// `child` is the element that raised the action — the focused one — never an intermediate.
    fn on_child_action(
        &mut self,
        _cx: &mut ScreenCx<'_>,
        _child: ElemHandle,
        _e: &crate::focus::InputEvent,
    ) -> bool {
        false
    }
    fn on_notice(&mut self, _cx: &mut ScreenCx<'_>, _id: NoticeId, _payload: &NoticePayload) {}

    /// Accept a queued error before the new screen is shown when the previous mode supplied one.
    fn set_error_msg(&mut self, _s: String) {}

    /// The host's pre-game state reaching the current screen, once per frame at the start of the
    /// UI frame — before the input update, the pointer dispatch and the queued deliveries.
    ///
    /// The client's pre-game screens read these facts out of process globals (the network
    /// object, the UI persistent data, the string tables); a rebuild has none, so the host hands
    /// them over here. Returning a mode queues it, ahead of anything the frame's input queues.
    fn on_pregame(&mut self, _cx: &mut ScreenCx<'_>, _p: &PregameCx<'_>) -> Option<UiMode> {
        None
    }

    /// Hand the host what the screen holds for it between deliveries — the character screens'
    /// player-session operations — as requests into the UI's queue. The shell calls this once a
    /// frame, after the deliveries and the flow update and immediately before it drains the
    /// queue, ahead of the mode switch.
    fn flush_to_host(&mut self, _cx: &mut ScreenCx<'_>) {}

    /// The screen's own action callback — the input maps a pre-game screen's constructor
    /// registers, offered each action before the handler list. `None` means this screen has no
    /// callback, and the action goes on the road it would have taken anyway.
    fn on_mode_action(
        &mut self,
        _cx: &mut ScreenCx<'_>,
        _e: &dereth_input::InputEvent,
    ) -> Option<ModeAction> {
        None
    }

    /// The screen's character handler (the intro registers one on the character list). `None`
    /// means the screen has none; `Some(mode)` is the mode it asks to queue, if any.
    fn on_mode_character(&mut self, _cx: &mut ScreenCx<'_>, _ch: u16) -> Option<Option<UiMode>> {
        None
    }

    /// A pre-game host call in the screen crate's own vocabulary (the development log-in driver,
    /// the creation wizard's preview). Returns whether the screen took it.
    fn on_host_call(&mut self, _cx: &mut ScreenCx<'_>, _call: &mut dyn std::any::Any) -> bool {
        false
    }

    /// Whether this is the in-game screen, the one [`Self::on_game`] reaches. The host's
    /// subscriber checks ("is a gameplay screen up to hear this notice?") ask this.
    fn is_game(&self) -> bool {
        false
    }

    /// The host's in-game state reaching the gameplay screen: one call of the screen crate's own
    /// call vocabulary, made at the point of the frame where the host's owner of that state runs
    /// (the HUD's per-frame drive, a request owner's tail, a notice's subscriber). `cx.view` is
    /// the game as the host sees it at that point. Returns whether the screen took the call.
    fn on_game(&mut self, _cx: &mut ScreenCx<'_>, _g: &mut GameCx<'_>) -> bool {
        false
    }

    /// The roots this screen created, so the flow can delete them.
    fn roots(&self) -> &[ElemHandle];
}

/// The two root-building helpers a screen calls from `create`.
///
/// One resolves a layout enum and the other accepts a data id directly; each builds the tree,
/// registers the screen as a root listener, and records the root.
pub fn create_and_add_root_element(
    ui: &mut UiSystem,
    assets: &dyn AssetSource,
    resolver: &dyn LayoutEnumResolver,
    layout: LayoutEnum,
    element: ElementId,
) -> Result<ElemHandle, UiError> {
    // Treat a missing entry as a loud error, not a silent empty screen: otherwise a wrong table
    // looks exactly like a layout parsing bug.
    let did = resolver
        .resolve(layout)
        .ok_or(UiError::UnresolvedLayoutEnum(layout))?;
    create_and_add_root_element_by_data_id(ui, assets, did, element)
}

/// Create and add a root element by data id.
pub fn create_and_add_root_element_by_data_id(
    ui: &mut UiSystem,
    assets: &dyn AssetSource,
    did: DataId,
    element: ElementId,
) -> Result<ElemHandle, UiError> {
    let h = ui.create_root_by_data_id(assets, did, element)?;
    ui.initialize_tree(h);
    Ok(h)
}

/// The UI flow mode machine.
#[derive(Debug, Default)]
pub struct UiFlow {
    /// The framework-constructor table. Global and never cleared, as in the client.
    factories: BTreeMap<UiMode, fn() -> Box<dyn Screen>>,
    /// The current mode (0 = none).
    cur_mode: Option<UiMode>,
    /// The next mode (0 = nothing queued).
    next_mode: Option<UiMode>,
    /// The current screen.
    cur: Option<Box<dyn Screen>>,
    /// The next error text — carried into the next mode's `set_error_msg`.
    next_text: Option<String>,
    /// The persistent UI data, allocated once and destroyed only with the flow.
    pub data: UiPersistentData,
    /// How many switches have happened, for tests.
    pub switches: u64,
}

impl UiFlow {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a framework class. Duplicate ids are **rejected**, matching the client's table.
    pub fn register(&mut self, m: UiMode, ctor: fn() -> Box<dyn Screen>) -> bool {
        if self.factories.contains_key(&m) {
            return false;
        }
        self.factories.insert(m, ctor);
        true
    }

    /// Queue a UI mode. Queueing twice in one frame keeps only the last request.
    pub fn queue(&mut self, m: UiMode) {
        self.next_mode = Some(m);
        self.next_text = None;
    }

    /// Queue a UI mode with an error message.
    pub fn queue_with_error(&mut self, m: UiMode, err: String) {
        self.next_mode = Some(m);
        self.next_text = Some(err);
    }

    #[must_use]
    pub fn current_mode(&self) -> Option<UiMode> {
        self.cur_mode
    }

    #[must_use]
    pub fn queued_mode(&self) -> Option<UiMode> {
        self.next_mode
    }

    #[must_use]
    pub fn current(&self) -> Option<&dyn Screen> {
        self.cur.as_deref()
    }

    pub fn current_mut(&mut self) -> Option<&mut Box<dyn Screen>> {
        self.cur.as_mut()
    }

    /// The flow's global-message listener — on message 3, `use_new_mode`.
    ///
    /// **Run this last**, after every other message-3 listener has ticked: `use_new_mode` destroys
    /// the current screen, and a listener that ticks afterwards ticks a screen that is gone.
    pub fn on_tick(&mut self, cx: &mut ScreenCx<'_>) {
        self.use_new_mode(cx);
    }

    /// Use the queued mode, in the documented order:
    ///
    /// ```text
    /// nothing queued                        -> return
    /// look up the queued mode's constructor; unknown mode -> return (silently dropped)
    /// hide the current screen, then destroy it (which resets the dialog factory)
    /// current mode = queued mode; no current screen; nothing queued
    /// take the pending error text
    /// construct the new screen
    /// if there was error text, hand it to the new screen
    /// show the new screen
    /// ```
    ///
    /// The old framework is **fully destroyed before the new one is constructed**, so a mode's
    /// constructor can assume the previous mode's roots, notice registrations and input maps are
    /// already gone.
    pub fn use_new_mode(&mut self, cx: &mut ScreenCx<'_>) {
        let Some(next) = self.next_mode else { return };
        // An unknown mode id: the request is silently dropped and the next mode is left set, exactly
        // as the client's early `return` does.
        let Some(ctor) = self.factories.get(&next).copied() else {
            return;
        };

        if let Some(mut old) = self.cur.take() {
            for h in old.roots().to_vec() {
                cx.ui.remove_and_delete_root(h);
            }
            old.destroy(cx);
            // Teardown resets the dialog factory, so every open dialog dies with a mode switch.
            // The factory's statics live on `UiSystem`
            // (`ui.dialogs`) rather than on the flow, because that is where a `Screen` can reach
            // them and because they are statics in the client, not framework members. This call
            // remains in its original position: after the old screen's roots are gone and before
            // the new screen is built.
            for h in cx.ui.dialogs.reset() {
                cx.ui.remove_and_delete_root(h);
            }

            cx.ui.reset_tooltip();
        }
        self.cur_mode = Some(next);
        self.next_mode = None;
        let pending = self.next_text.take();

        let mut screen = ctor();
        if let Some(t) = pending {
            screen.set_error_msg(t);
        }
        // A screen whose `create` fails leaves the flow with no framework, which is the same shape
        // as the client's "the factory returned nothing".
        if screen.create(cx).is_ok() {
            self.cur = Some(screen);
        }
        self.switches += 1;
    }

    /// The flow's interface update — poke the current mode.
    pub fn update(&mut self, cx: &mut ScreenCx<'_>, now: LocalTime) {
        if let Some(s) = self.cur.as_mut() {
            if let Some(m) = s.update(cx, now) {
                self.queue(m);
            }
        }
    }

    /// Route one queued delivery to the current screen.
    pub fn deliver(&mut self, cx: &mut ScreenCx<'_>, d: &crate::Delivery) {
        let Some(s) = self.cur.as_mut() else { return };
        match d {
            crate::Delivery::Element { msg, .. } => s.on_element_message(cx, msg),
            crate::Delivery::Global { id, param, .. } => s.on_global_message(cx, *id, *param),
            crate::Delivery::Notice { id, payload, .. } => s.on_notice(cx, *id, payload),
        }
    }

    /// One frame: the seven `use_time` steps, then the queued external deliveries, then — **last** —
    /// the mode switch.
    pub fn frame(
        &mut self,
        cx: &mut ScreenCx<'_>,
        now: LocalTime,
        input: &mut dyn crate::InputPump,
    ) {
        cx.ui.use_time(now, input);
        for d in cx.ui.drain_outbox() {
            self.deliver(cx, &d);
        }
        self.update(cx, now);
        self.on_tick(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the fixed UI mode table and its startup-mode transitions.
    #[test]
    fn the_eight_modes_are_the_registered_ones_and_three_ids_are_absent() {
        assert_eq!(mode::REGISTRATION_ORDER.len(), 8);
        assert_eq!(
            mode::REGISTRATION_ORDER[0],
            mode::DATA_PATCH,
            "queued first"
        );
        let ids: Vec<u32> = mode::REGISTRATION_ORDER.iter().map(|m| m.0).collect();
        for absent in [0x1000_0004_u32, 0x1000_0006, 0x1000_0007] {
            assert!(
                !ids.contains(&absent),
                "{absent:#X} is not registered in this build"
            );
        }
    }

    /// The layout enum table maps the documented screens.
    #[test]
    fn the_layout_enum_table_maps_the_documented_screens() {
        let r = TableResolver::retail_prefix();
        assert_eq!(
            r.resolve(LayoutEnum(0x1000_0001)),
            Some(DataId(0x2100_0000))
        ); // patch
        assert_eq!(
            r.resolve(LayoutEnum(0x1000_0002)),
            Some(DataId(0x2100_0001))
        ); // intro
        assert_eq!(
            r.resolve(LayoutEnum(0x1000_0005)),
            Some(DataId(0x2100_0004))
        ); // charactermanagement
        assert_eq!(
            r.resolve(LayoutEnum(0x1000_0037)),
            Some(DataId(0x2100_0036))
        ); // epilogue
        assert_eq!(
            r.resolve(LayoutEnum(0x1000_0038)),
            Some(DataId(0x2100_0037))
        ); // ItemSlot
        assert_eq!(r.resolve(LayoutEnum(2)), Some(DataId(0x2100_003C))); // Dialog
        assert_eq!(r.resolve(LayoutEnum(1)), Some(DataId(0x2100_003B))); // ContextMenu
        assert_eq!(r.resolve(LayoutEnum(0xDEAD)), None);
    }
}
