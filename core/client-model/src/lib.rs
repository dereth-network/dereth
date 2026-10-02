//! The client-side half of the game rules: the object model and state cache the server's messages
//! fill, with the display formulas and legality prechecks read from it.
//!
//! **Depends on** `dereth-primitives`, the decoded tables (`dereth-assets`), the codecs
//! (`dereth-protocol`), the shared rules (`dereth-rules`, whose modules it re-exports at their old
//! paths) and the contract (`dereth-client-contract`). **Used by** the client runtime and the SDK,
//! the client and its test kit, and, as a test-only cross-check, the server (`empyrean-world`).
//!
//! **Must never** predict or touch bytes. The client is a viewer with a state cache: it decodes
//! what the server tells it, formats it and sends requests, and every formula here is either a
//! display formula or a legality precheck, never a simulation (no predicted combat, stamina, damage
//! or inventory move). Wire encoding is `dereth-protocol`'s; the senders emit typed [`Request`]s
//! into a [`RequestSink`], and the session frames and stamps them, so the one global action stamp
//! lives in the session and not here.
//!
//! **Why every table here is a `BTreeMap`.** Several tables reach the player through a panel that
//! walks them with no sort (the quality tables behind the admin panel, the squelch list, the
//! component tracker, the allegiance tree, the fellowship roster, the guest list, the contract
//! tracker). The client's own order is a hash table's bucket order, which this crate does not
//! reproduce; it makes the order deterministic (ascending by key) instead, so a panel cannot
//! reorder between runs. The one place the client's exact order is reproduced is [`ObjMap`],
//! because its bucket walk is what the visible-object, destruction and radar sweeps enumerate.
//! `xtask lint` holds this crate to that order contract. The chat command interpreter is [`cmd`],
//! beside [`chat_cmd`].

#![doc(html_no_source)]

pub mod advancement;
pub mod allegiance;
pub mod allegiance_cmd;
pub mod appraisal;
pub mod appraisal_model;
pub mod attributes;
pub mod book;
pub mod chat;
pub mod chat_cmd;
/// The chat command interpreter: a line beginning `/` or `@` is normalised, tokenised and looked
/// up in the table of client-side handlers, and anything unrecognised is forwarded verbatim.
pub mod cmd;
pub mod turbine;
/// A container's slot counts: the capacity byte read signed. Lives in [`dereth_rules::capacity`].
pub use dereth_rules::capacity;
/// The chess rules. Lives in [`dereth_rules::chess`].
pub use dereth_rules::chess;
pub mod combat;
pub mod emotes;
pub mod enchant;
pub mod fellowship;
pub mod friends;
/// The host's text conversion, behind [`dereth_primitives::HostEncoding`].
pub mod host;
pub mod housing;
pub mod inventory;
pub mod magic;
/// Chess window and board grid.
pub mod minigame;
pub mod objects;
pub mod objmap;
pub mod player;
pub mod portal_storm;
pub mod qualities;
pub mod quests;
/// Object-range checks that close panels when the player walks away.
pub mod range;
/// the notice → screen chain.
pub mod scroll;
/// The language filter. Lives in [`dereth_rules::taboo`].
pub use dereth_rules::taboo;
/// the radar range as a selection gate.
pub mod selection;
pub mod skills;
pub mod trade;
pub mod vendor;
pub mod weenie;
pub mod world;

pub use host::HostText;
pub use objmap::ObjMap;
pub use qualities::{Qualities, StatKey, StatType, StatValue};
pub use weenie::Weenie;
pub use world::World;

use dereth_primitives::ObjectId;

/// Why a game operation could not be carried out.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GameError {
    #[error("no object {0}")]
    NoSuchObject(ObjectId),
    #[error("object {0} already exists; a duplicate create is silently dropped")]
    DuplicateCreate(ObjectId),
    /// The three-way instance-sequence gate rejected the descriptor before anything else ran.
    #[error("instance sequence {got} is not newer than {have} for object {id}")]
    StaleInstance { id: ObjectId, got: u16, have: u16 },
    #[error("the local player is not known yet")]
    NoPlayer,
}

/// The gameplay → UI channel. The presentation layer consumes these; this crate never draws.
///
/// This enum is the contract with the presentation layer; numeric notice ids are not needed
/// because nothing in this crate branches on them.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// The selected object changed.
    SelectionChanged {
        previous: Option<ObjectId>,
        current: Option<ObjectId>,
    },
    /// An object was created.
    ObjectCreated(ObjectId),
    /// The object left the tables.
    ObjectDeleted(ObjectId),
    /// An object's quality changed.
    StatUpdated(ObjectId, StatKey),
    /// An object's quality was removed.
    StatRemoved(ObjectId, StatKey),
    /// Refresh an item's attributes for the supplied change kind.
    ItemAttributesChanged { object: ObjectId, kind: u32 },
    /// The server moved an item between storage or wielding locations.
    ItemMoved {
        object: ObjectId,
        old_container: ObjectId,
        old_wielder: ObjectId,
        old_location: u32,
        container: ObjectId,
        place: u32,
        wielder: ObjectId,
        location: u32,
    },
    /// An attempted item operation failed with the supplied reason.
    AttemptFailed { object: ObjectId, reason: u32 },
    /// A container's contents list was replaced.
    InventoryChanged(ObjectId),
    /// Show an item operation pending in the player's inventory.
    ShowPendingInPlayer(ObjectId),
    /// End the pending-item display in the player's inventory.
    EndPendingInPlayer,
    /// An `AppraisalProfile` arrived and the assess panel may draw.
    AppraisalReady(ObjectId),
    /// Open the book panel.
    ///
    /// The book's pages are not carried here for the same reason `OpenVendor`'s stock is not:
    /// the client's notice passes a pointer to its page list into the panel's own storage, and
    /// here the panel reads the stored book state back. See [`crate::book`].
    OpenBook(ObjectId),
    /// The watched, unowned book left its use radius; hide the matching reader window.
    CloseBook(ObjectId),
    /// The local player's object description changed.
    PlayerObjDescChanged,
    /// `(channel, text)` — every user-visible refusal in this
    /// crate goes out this way. Channel `0x1A` is the inventory/system feedback channel.
    DisplayString { channel: u32, text: String },
    //
    // There is deliberately no chat-line notice: **retail has none**. An incoming line is
    // composed by the message handler and handed straight to the UI, and the hearing gate —
    // [`chat::ChatState::can_hear`] — is applied there, beside the `^`/`&` language split and the
    // self-echo ordering. A notice here would build a second, poorer chat path beside the one
    // that draws.
    //

    // ---------------------------------------------------------------------------------------
    // The four notices the client's use-item switch and its
    // tail raise. Each opens (or closes) a panel; the panel itself is the presentation layer's.
    // ---------------------------------------------------------------------------------------
    /// Open a container already held by the player.
    ///
    /// `using_item`'s tail, for a container the player already owns: a side pack in the inventory
    /// opens onto its own contents. It is raised **after** whatever the switch did, and for a pack
    /// the switch does nothing at all.
    OpenContainedContainer(ObjectId),
    /// `(id)` — notice. `ObjectId(0)` **closes**
    /// the ground container.
    ///
    /// This is how a corpse or a chest becomes the open ground object. Note the asymmetry, which
    /// is the client's and is the reason this notice arrives from two directions:
    /// the local use path raises only the zero (close) form; the open form follows the
    /// server's `0x0196` view-contents response.
    SetGroundObject(ObjectId),
    /// `(id)` — `using_item` arm 6, a tinkering tool.
    OpenSalvagePanel(ObjectId),
    /// `(id)` — `using_item` arm 7, a game board.
    BeginGame(ObjectId),
    /// Ask for one of the three use confirmations instead of immediately sending a use request.
    ///
    /// The Yes callback calls `confirm_usage`: send the use request, increment the busy count
    /// and run the local using-item path, in that order.
    UsageConfirmation {
        object: ObjectId,
        kind: inventory::use_object::UsageConfirmation,
    },
    /// A targeted-use dialog request. The accepted callback calls `confirm_targeted_usage`;
    /// source and target are snapshots, never the current selection.
    /// The App's generation-owned targeted-dialog subscriber builds the shipped Confirmation
    /// layout; only its actual Yes callback calls the accepted branch.
    TargetedUsageConfirmation {
        source: ObjectId,
        target: ObjectId,
        kind: inventory::targeted_use::TargetedUsageConfirmation,
    },

    // ---------------------------------------------------------------------------------------
    // Vendor notices.
    // ---------------------------------------------------------------------------------------
    /// Open the vendor panel with the received mode and surviving stock count.
    ///
    /// The profile and the stock are not carried in the notice because the client's are pointers
    /// into shared state and the panel reads them back; here it reads the stored shop state.
    /// `items` counts the stock rows that survived processing, so the panel can distinguish
    /// an empty shop from a shop it never received.
    OpenVendor {
        vendor: ObjectId,
        mode: vendor::ShopMode,
        items: usize,
    },
    /// Close the vendor panel. There is no close-vendor message on the wire.
    CloseVendor,
    /// The watched covenant crystal left the
    /// native nine-unit radius, so the matching purchase / maintenance window hides.
    CloseSlumlord(ObjectId),
    /// `(objId) ` — notice, raised when
    /// the shop was opened by dragging an item onto the vendor.
    AddItemToSell(ObjectId),

    // ---------------------------------------------------------------------------------------
    // Eleven secure-trade notices.
    // ---------------------------------------------------------------------------------------
    /// A negotiation has been registered; the window's
    /// partner-name field and its object-range handler both hang off this.
    TradeRegistered {
        initiator: ObjectId,
        partner: ObjectId,
    },
    /// **This notice does not raise the window.** Its inherited handler does not
    /// change visibility; registration is the event that shows the trade panel.
    OpenSecureTrade { source: ObjectId },
    /// Close the trade with the supplied reason.
    CloseSecureTrade { reason: u32 },
    /// An item was added. `position` is `0x0200`'s third
    /// dword, which is an **insert index**; see [`trade`]'s header.
    TradeItemAdded {
        item: ObjectId,
        side: u32,
        position: usize,
    },
    /// Remove an item from the indicated side of the trade.
    TradeItemRemoved { item: ObjectId, side: u32 },
    /// Acceptance changed. `source == 0` is the server clearing
    /// *your* acceptance, and is not a decline.
    TradeAccepted { source: ObjectId },
    /// The source declined the trade.
    TradeDeclined { source: ObjectId },
    /// Reset the trade for the supplied source.
    TradeReset { source: ObjectId },
    /// An item failed a trade operation with the supplied reason.
    TradeFailure { item: ObjectId, reason: u32 },
    /// The anti-scam message: the
    /// contents changed, so both acceptances are void. The handler raises **only** this; the flags
    /// are cleared by the `0x0202`s that follow.
    TradeAcceptanceCleared,
    /// The client's "the partner is already registered, so
    /// put this on the table now" arm, and the deferred replay of it once a `0x01FD` arrives.
    TradeAnItemForDummies(ObjectId),

    // ---------------------------------------------------------------------------------------
    // The two enchantment-registry notices. Verified against retail; see
    // [`crate::enchant`]'s module header.
    // ---------------------------------------------------------------------------------------
    /// **This is the notice that makes a live buff show up in the Skills and Attributes pages.**
    /// The stat-management panel's handler is inherited by the skills panel. It updates
    /// each row from the player description, recomputing raw and effective skill values
    /// and rewriting the displayed value and font.
    ///
    /// Raised at these points in message handling:
    ///
    /// | message | condition |
    /// |---|---|
    /// | `0x02C2` | only when the stat-mod type `& 0x800000 == 0`; the vitae arm raises [`Notice::VitaeChanged`] **instead** |
    /// | `0x02C3` / `0x02C7` | unconditional, after an optional [`Notice::VitaeChanged`] |
    /// | `0x02C4` | unconditional, **once** after the whole list |
    /// | `0x02C5` / `0x02C8` | unconditional |
    /// | `0x02C6` / `0x0312` | both purges, with [`Notice::VitaeChanged`] after it |
    EnchantmentsChanged,
    /// The stat-management panel does not register for this notice, so a vitae change alone does not redraw
    /// the skills list. That asymmetry is reproduced rather than smoothed: `0x02C2` raises this
    /// *instead of* [`Notice::EnchantmentsChanged`] when the enchantment is the vitae one.
    VitaeChanged,
}

/// Where notices go. The presentation layer implements it; the corpus gate records them.
pub trait NoticeSink {
    fn emit(&mut self, n: Notice);
}

/// A sink that keeps everything, for tests and for the corpus gate's notice comparison.
#[derive(Debug, Default)]
pub struct RecordingSink(pub Vec<Notice>);

impl NoticeSink for RecordingSink {
    fn emit(&mut self, n: Notice) {
        self.0.push(n);
    }
}

/// A sink that drops everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl NoticeSink for NullSink {
    fn emit(&mut self, _: Notice) {}
}

/// One outbound request, as a typed [`dereth_protocol`] message.
///
/// The variants are exactly the senders this crate implements. Framing — `OrderedActionHeader` and its global
/// stamp for a game action, a bare `[opcode][body]` blob otherwise — is
/// [`dereth_protocol::actions::outbound_kind`]'s answer and `dereth_client_net::client_session`'s job.
#[allow(clippy::large_enum_variant)] // built and consumed one at a time, never stored in bulk
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Request {
    /// Get and wield an item — 0x001A.
    GetAndWieldItem(dereth_protocol::items::InventoryGetAndWieldItem),
    /// Split a stack and wield the split — 0x019B.
    StackableSplitToWield(dereth_protocol::items::InventoryStackableSplitToWield),
    /// Put an item in a container — 0x0019.
    PutItemInContainer(dereth_protocol::items::InventoryPutItemInContainer),
    /// Drop an item — 0x001B.
    DropItem(dereth_protocol::items::InventoryDropItem),
    /// Merge one stack into another — 0x0054.
    StackableMerge(dereth_protocol::items::InventoryStackableMerge),
    /// Split a stack into a container — 0x0055.
    StackableSplitToContainer(dereth_protocol::items::InventoryStackableSplitToContainer),
    /// Split a stack onto the ground — 0x0056.
    StackableSplitTo3d(dereth_protocol::items::InventoryStackableSplitTo3d),
    /// Give an object to another creature — 0x00CD.
    GiveObjectRequest(dereth_protocol::items::InventoryGiveObjectRequest),
    /// Request an item appraisal — 0x00C8.
    Appraise(dereth_protocol::objects::ItemAppraise),
    /// Query item mana — 0x0263. The toolbar requests this when it needs the current value.
    QueryItemMana(dereth_protocol::items::ItemQueryItemMana),
    /// Set an inscription — 0x00BF, on the **Weenie** queue.
    ///
    /// Without this sender an inscription typed into the identify window never leaves the
    /// machine. The inscription box **losing keyboard focus** is the commit edge. The ordered
    /// body contains the object id followed by the packed text.
    SetInscription(dereth_protocol::trade::WritingSetInscription),
    /// Submit an abuse report — `0x0140`, an ordinary ordered
    /// Weenie action. The panel supplies target and complaint; native supplies status `1`.
    AbuseLog(dereth_protocol::admin::CharacterAbuseLogRequest),
    /// The five client-to-server messages used by the book panel.
    BookAddPage(dereth_protocol::trade::WritingBookAddPage),
    BookModifyPage(dereth_protocol::trade::WritingBookModifyPage),
    BookDeletePage(dereth_protocol::trade::WritingBookDeletePage),
    BookPageData(dereth_protocol::trade::WritingBookPageData),
    BookData(dereth_protocol::trade::WritingBookData),
    /// Finish changing appearance at the barber — `0x0311`.
    FinishBarber(dereth_protocol::trade::CharacterFinishBarber),
    /// Add an item shortcut — `0x019C`.
    /// The notifying insert updates local state and sends immediately, so the server learns
    /// about the drop without waiting for the 480-second options flush.
    AddShortCut(dereth_protocol::login::CharacterAddShortCut),
    /// Remove an item shortcut — `0x019D`. A move between slots removes the
    /// old shortcut before adding the new one, preventing the same object occupying both.
    RemoveShortCut(dereth_protocol::login::CharacterRemoveShortCut),
    /// Use an item — 0x0036.
    UseEvent(dereth_protocol::items::InventoryUseEvent),
    /// Use an item on a target — 0x0035.
    UseWithTargetEvent(dereth_protocol::items::InventoryUseWithTargetEvent),
    /// Create a tinkering tool — 0x027D.
    CreateTinkeringTool(dereth_protocol::items::InventoryCreateTinkeringTool),
    /// Request an object's description — 0xF7C8.
    ForceObjdesc(dereth_protocol::objects::ObjectSendForceObjdesc),
    /// Query the server version — 0xF7CC, on the Control queue.
    ///
    /// Player initialization emits this only for a `PlayerDesc`
    /// whose Boolean quality 0x2C, 0x2D, or 0x61 makes the player a PSR.
    AdminGetServerVersion(dereth_protocol::admin::AdminSendAdminGetServerVersion),
    /// Plain local speech — 0x0015.
    ///
    /// The leading command marker is what decides that a line is
    /// speech rather than a command; [`cmd::CommandInterp`] is its transcription.
    Talk(dereth_protocol::comms::CommunicationTalk),
    /// Room chat — bare F7DE on the Logon queue, without an ordered game-action header.
    TurbineChat(dereth_protocol::turbine::SendToRoomById),
    /// Tell the last speakable target — 0x0032.
    ///
    /// Talk focus 2 resolves the last speakable target and returns early if
    /// there is none. That early return drops the line instead of speaking it aloud.
    TalkDirect(dereth_protocol::comms::CommunicationTalkDirect),
    /// Tell a character by name — 0x005D.
    ///
    /// Two client commands produce this. The body is **message first,
    /// name second**; see [`chat::ChatState::do_tell`] for that order.
    TalkDirectByName(dereth_protocol::comms::CommunicationTalkDirectByName),
    /// Send an emote — `0x01DF`.
    ///
    /// The command table ([`cmd::table`]) routes `e`, `em`, `emote` and `me` here. The body is one
    /// packed byte string, length-prefixed and aligned to four bytes; the framing is the same `OrderedActionHeader`-stamped game action as
    /// [`Request::Talk`]. The two senders are identical except for the opcode.
    ///
    /// Produced by [`chat::do_emote`].
    Emote(dereth_protocol::comms::CommunicationEmote),
    /// Send a pose emote — `0x01E1`.
    ///
    /// The *pose* half: sends the emote table entry's
    /// text for other observers with `%p` substituted, reached through
    /// the client's `*…*` and `<…>` extraction rather than a command name —
    /// see [`emotes::public_chat`]. Same shape and same framing as [`Request::Emote`], and
    /// deliberately a separate variant rather than a flag, because the two opcodes take different
    /// paths into the client and one wired with the other dead would pass any test that only asks
    /// whether "an emote was sent".
    SoulEmote(dereth_protocol::comms::CommunicationSoulEmote),
    /// Broadcast a line to a channel — 0x0147.
    ///
    /// The chat-command handler's cases 3–6, whose channel constants are literals:
    /// 3 → `0x800`, 4 → `0x2000`, 5 → `0x4000`, 6 → `0x1000`. \[verified\]
    ChannelBroadcast(dereth_protocol::comms::CommunicationChannelBroadcast),
    /// `Combat_ChangeCombatMode` — 0x0053.
    ChangeCombatMode(dereth_protocol::combat::CombatChangeCombatMode),
    /// `Combat_TargetedMeleeAttack` — 0x0008.
    TargetedMeleeAttack(dereth_protocol::combat::CombatTargetedMeleeAttack),
    /// `Combat_TargetedMissileAttack` — 0x000A.
    TargetedMissileAttack(dereth_protocol::combat::CombatTargetedMissileAttack),
    /// `Combat_CancelAttack` — 0x01B7.
    CancelAttack(dereth_protocol::combat::CombatCancelAttack),
    /// `Combat_QueryHealth` — 0x01BF.
    QueryHealth(dereth_protocol::combat::CombatQueryHealth),
    /// Request a ping — 0x01E9.
    ///
    /// An empty body. Opening the link-status panel is the only sender in the client:
    /// opening the link-status panel pings, and so does every 120 s it stays open.
    RequestPing(dereth_protocol::admin::CharacterRequestPing),
    /// Change a player option — 0x0005.
    ///
    /// One `PlayerOption` ordinal and its new value, sent the instant an
    ///  option changes rather than waiting for the 480-second flush.
    /// Those twenty-one are the options with a server-visible effect, and five of them are chat
    /// channel subscriptions: ACE answers `ListenToAllegianceChat` turning on with
    /// `Player.JoinTurbineChatChannel("Allegiance")`, i.e. with a fresh `0x0295`, which is the
    /// only way a client can ask to join a Turbine chat room without logging out.
    PlayerOptionChanged(dereth_protocol::login::CharacterPlayerOptionChangedEvent),
    /// Request allegiance updates — 0x001F.
    ///
    /// The subscribe toggle sent when the allegiance tab comes up, and the only way a
    /// client ever sees a roster: the shard answers this with `0x0020
    /// Allegiance_AllegianceUpdate` and afterwards pushes on change. Without it the panel sees
    /// only the empty pushes a login produces.
    AllegianceUpdateRequest(dereth_protocol::social::AllegianceUpdateRequest),
    /// Swear allegiance to the target — `0x001D`, a bare
    /// object id.
    ///
    /// Raised from the allegiance panel's Swear button, but only through the confirmation
    /// dialog's accepted callback.
    SwearAllegiance(dereth_protocol::social::AllegianceSwearAllegiance),
    /// Break allegiance to the target — `0x001E`, a bare object
    /// id.
    ///
    /// **Both the Break button and the Kick button send this**: Break names the player's patron,
    /// while Kick names the selected vassal. `0x0277 Allegiance_BreakAllegianceBoot` is a different
    /// event entirely (a name string plus an
    /// account-boot flag) and no button on this panel raises it. The recorded captures carry
    /// five `0x001E`, each a single object id, and they are the oracle for **both** buttons.
    BreakAllegiance(dereth_protocol::social::AllegianceBreakAllegiance),

    // ---- twenty-four allegiance events sent from chat commands --------------------------------
    //
    // **None of them has a button in retail.** Each sender has exactly one caller, a
    // chat-command handler; the allegiance panel's only outbound edges are `0x001D`, `0x001E` and
    // `0x001F` above. So the whole production path is the chat entry, and it is
    // [`crate::allegiance_cmd`] — which is why these carry a command in their doc rather than an
    // element id.
    //
    // The capture corpus witnesses none of them: over 2,920 client game actions in the three
    // recorded fellowship sessions the only allegiance client-to-server opcodes are `0x001D` (7),
    // `0x001E` (5) and `0x001F` (31). The body oracle compares `dereth_protocol`'s writers against
    // each retail allegiance sender.
    /// Query the allegiance name — `0x0030`, `@allegiance name`.
    AllegianceQueryName(dereth_protocol::social::AllegianceQueryAllegianceName),
    /// Clear the allegiance name — `0x0031`,
    /// `@allegiance name clear`.
    AllegianceClearName(dereth_protocol::social::AllegianceClearAllegianceName),
    /// Set the allegiance name — `0x0033`,
    /// `@allegiance name set <text>`.
    AllegianceSetName(dereth_protocol::social::AllegianceSetAllegianceName),
    /// Set an allegiance officer — `0x003B`,
    /// `@allegiance officer add|set <1-3> <name>`.
    AllegianceSetOfficer(dereth_protocol::social::AllegianceSetAllegianceOfficer),
    /// Set an officer title — `0x003C`,
    /// `@allegiance title set <1-3> <title>`. The **level is first** on the wire
    /// (opcode, level dword, then title string).
    AllegianceSetOfficerTitle(dereth_protocol::social::AllegianceSetAllegianceOfficerTitle),
    /// List officer titles — `0x003D`,
    /// `@allegiance title` / `title list`.
    AllegianceListOfficerTitles(dereth_protocol::social::AllegianceListAllegianceOfficerTitles),
    /// Clear officer titles — `0x003E`,
    /// `@allegiance title clear`.
    AllegianceClearOfficerTitles(dereth_protocol::social::AllegianceClearAllegianceOfficerTitles),
    /// Change or query allegiance locking — `0x003F`, one dword from
    /// [`crate::allegiance_cmd::lock_action`]. `@allegiance lock [on|off|toggle|check|bypass]`.
    AllegianceDoLockAction(dereth_protocol::social::AllegianceDoAllegianceLockAction),
    /// Approve a vassal to bypass the lock — `0x0040`,
    /// `@allegiance lock bypass <name>`.
    AllegianceSetApprovedVassal(dereth_protocol::social::AllegianceSetAllegianceApprovedVassal),
    /// Change a member's allegiance-chat gag — `0x0041`,
    /// `@allegiance chat gag|ungag <name>`.
    AllegianceChatGag(dereth_protocol::social::AllegianceChatGag),
    /// Change allegiance housing access — `0x0042`, one dword from
    /// [`crate::allegiance_cmd::house_action`]. `@allegiance house [guest|storage] [open|close]`.
    AllegianceDoHouseAction(dereth_protocol::social::AllegianceDoAllegianceHouseAction),
    /// Set the message of the day — `0x0254`, `@motd set <text>`.
    AllegianceSetMotd(dereth_protocol::social::AllegianceSetMotd),
    /// Query the message of the day — `0x0255`, `@motd`.
    AllegianceQueryMotd(dereth_protocol::social::AllegianceQueryMotd),
    /// Clear the message of the day — `0x0256`, `@motd clear`.
    AllegianceClearMotd(dereth_protocol::social::AllegianceClearMotd),
    /// Boot an allegiance member — `0x0277`,
    /// `@allegiance boot [-account] <name>`. **Not** the Kick button, which is `0x001E`
    /// (`BreakAllegiance` above).
    AllegianceBreakAllegianceBoot(dereth_protocol::social::AllegianceBreakAllegianceBoot),
    /// Request allegiance information — `0x027B`,
    /// `@allegiance info <name>`. The shard answers with `0x027C`, whose chat block
    /// [`crate::allegiance::allegiance_info_block`] formats.
    AllegianceInfoRequest(dereth_protocol::social::AllegianceInfoRequest),
    /// Kick a member from allegiance chat — `0x02A0`,
    /// `@allegiance chat kick <name>[, <reason>]`.
    AllegianceChatBoot(dereth_protocol::social::AllegianceChatBoot),
    /// Add an allegiance ban — `0x02A1`, `@allegiance ban add`.
    AllegianceAddBan(dereth_protocol::social::AllegianceAddAllegianceBan),
    /// Remove an allegiance ban — `0x02A2`,
    /// `@allegiance ban remove`.
    AllegianceRemoveBan(dereth_protocol::social::AllegianceRemoveAllegianceBan),
    /// List allegiance bans — `0x02A3`, `@allegiance ban list`.
    AllegianceListBans(dereth_protocol::social::AllegianceListAllegianceBans),
    /// Remove an allegiance officer — `0x02A5`,
    /// `@allegiance officer remove <name>`.
    AllegianceRemoveOfficer(dereth_protocol::social::AllegianceRemoveAllegianceOfficer),
    /// List allegiance officers — `0x02A6`, `@allegiance officer`
    /// and `officer list`.
    AllegianceListOfficers(dereth_protocol::social::AllegianceListAllegianceOfficers),
    /// Clear allegiance officers — `0x02A7`,
    /// `@allegiance officer clear`.
    AllegianceClearOfficers(dereth_protocol::social::AllegianceClearAllegianceOfficers),
    /// Recall to the allegiance hometown — `0x02AB`, `@alh` / `@ah` and
    /// `@allegiance hometown`. The allegiance handler also accepts `ho` inline.
    AllegianceRecallHometown(dereth_protocol::social::AllegianceRecallAllegianceHometown),

    // ---- seven fellowship client-to-server events --------------------------------------------
    //
    // The fellowship panel is their only caller in retail: without them the Fellowship tab
    // cannot create, join, leave, promote, dismiss or open a fellowship at all.
    //
    // Recorded fellowship sessions carry **27 of them**, all sent by the original
    // client: 8 `0x00A6`, 9 `0x00A5`, 5 `0x00A3`, 2 `0x00A2`, 2 `0x0290`, 1 `0x00A4`, 1 `0x0291`.
    /// `0x00A6`, the live-vitals subscribe toggle.
    FellowshipUpdateRequest(dereth_protocol::social::FellowshipUpdateRequest),
    /// Create a fellowship — `0x00A2`.
    FellowshipCreate(dereth_protocol::social::FellowshipCreate),
    /// `0x00A3`. `disband` is `1` for the Disband button and `0`
    /// for Quit; the opcode is the same and the body is the only difference.
    FellowshipQuit(dereth_protocol::social::FellowshipQuitRequest),
    /// Dismiss a fellowship member — `0x00A4`.
    FellowshipDismiss(dereth_protocol::social::FellowshipDismiss),
    /// Recruit a fellowship member — `0x00A5`.
    FellowshipRecruit(dereth_protocol::social::FellowshipRecruit),
    /// Assign the new fellowship leader — `0x0290`.
    FellowshipAssignNewLeader(dereth_protocol::social::FellowshipAssignNewLeader),
    /// Change whether the fellowship is open — `0x0291`.
    FellowshipChangeFellowOpenness(dereth_protocol::social::FellowshipChangeFellowOpenness),

    // ---- four friends-list client-to-server events -------------------------------------------
    //
    // Without them the client cannot add or remove a friend at all, and
    // `0x0021 Social_FriendsUpdate` has nothing to answer.
    //
    // The asymmetry between the first two is retail's and is measured, not reasoned:
    // adding a friend carries a bare `PString` with **no id field at all**, while
    // removing one carries a real instance id. See `crate::friends` for the four capture
    // lines that show it.
    /// `(name) ` -- `0x0018`. You add by **name**.
    SocialAddFriend(dereth_protocol::social::SocialAddFriend),
    /// `(id) ` -- `0x0017`. You remove by **object id**.
    SocialRemoveFriend(dereth_protocol::social::SocialRemoveFriend),
    /// Clear the friends list — `0x0025`, an empty body.
    SocialClearFriends(dereth_protocol::social::SocialClearFriends),
    /// Legacy friends command with arguments `(0, "")` — `0xF7CD`, without a game-action header:
    /// no `OrderedActionHeader` and no stamp. `@friends old` is its only caller in retail.
    SocialSendFriendsCommand(dereth_protocol::social::SocialSendFriendsCommand),
    /// Abandon contract `(contractId)` -- `0x0316`, four body bytes.
    ///
    /// The Contracts tab's Abandon button sends it: the client's `0x100005DC` arm is its only
    /// caller.
    SocialAbandonContract(dereth_protocol::social::SocialAbandonContract),
    /// `Magic_CastUntargetedSpell` — 0x0048.
    CastUntargetedSpell(dereth_protocol::combat::MagicCastUntargetedSpell),
    /// `Magic_CastTargetedSpell` — 0x004A.
    CastTargetedSpell(dereth_protocol::combat::MagicCastTargetedSpell),
    /// `Magic_TestSpellFormula` — 0x004B, the early clients' spell research test.
    TestSpellFormula(dereth_protocol::combat::MagicTestSpellFormula),
    /// Set the displayed character title — `0x002C`.
    ///
    /// The only caller in retail is the client's *"Set as Display Title"*
    /// button; nothing local changes when it goes out, because the worn title is server state.
    SetDisplayCharacterTitle(dereth_protocol::social::SocialSetDisplayCharacterTitle),
    /// Spend experience on a skill — 0x0046.
    ///
    /// `advancement::skill_cost_to_raise` computes its only argument. Both raise buttons of
    /// the skills panel send this one message; they differ only in the amount.
    TrainSkill(dereth_protocol::admin::TrainSkill),
    /// Change a skill's advancement class — 0x0047.
    TrainSkillAdvancementClass(dereth_protocol::admin::TrainSkillAdvancementClass),
    /// Spend experience on an attribute — 0x0045. `advancement::attribute_cost_to_raise`
    /// computes the amount.
    TrainAttribute(dereth_protocol::admin::TrainAttribute),
    /// Spend experience on a vital — 0x0044.
    TrainAttribute2nd(dereth_protocol::admin::TrainAttribute2nd),
    /// Update the spellbook filter — 0x0286.
    ///
    /// Updating the spellbook filter sends this **only when the mask actually changed**,
    /// and updates the local player-module copy at the same time; the server is being
    /// told, not asked.
    SpellbookFilterEvent(dereth_protocol::combat::CharacterSpellbookFilterEvent),
    /// `(spell, index, bank) ` — `0x01E3`, sixteen
    /// bytes.
    ///
    /// Adding the favorite sends this alongside the local update,
    /// so the spell bar is a **client** decision the shard is
    /// told about. Without this message the bar looks right until the next login and is then
    /// empty again, which is exactly what a client that only moved the icon would look like.
    AddSpellFavorite(dereth_protocol::combat::CharacterAddSpellFavorite),
    /// `(spell, bank) ` — `0x01E4`, twelve bytes
    /// and **no index**.
    ///
    /// A drag that moves a spell already on the tab sends this *and then* [`Self::AddSpellFavorite`],
    /// because the drag removes the previous entry before inserting the new one.
    RemoveSpellFavorite(dereth_protocol::combat::CharacterRemoveSpellFavorite),
    /// `(spell) ` — `0x01A8`, eight bytes.
    ///
    /// The spellbook's DELETE button sends this through its dialog callback. The callback
    /// reads the confirmation's `0x92` property, and
    /// on `true` reads the spell it stashed under `0x1000003F` and sends this. Nothing is removed
    /// locally — the row goes when the shard echoes the removal back.
    RemoveSpell(dereth_protocol::qualities::MagicRemoveSpell),

    /// Open trade negotiations with a partner — `0x01F6`.
    ///
    /// The using-item path's arm 5 attempts this after a double-click on another player.
    OpenTradeNegotiations(dereth_protocol::trade::TradeOpenTradeNegotiations),
    /// Stop viewing a container's contents — `0x0195`.
    ///
    /// When the stop-viewing path's notify-server flag is set, leaving a ground container
    /// tells the server to stop streaming its contents.
    NoLongerViewingContents(dereth_protocol::objects::InventoryNoLongerViewingContents),

    /// Buy items from a vendor, including alternate currency spent — `0x005F`.
    VendorBuy(dereth_protocol::trade::VendorBuy),
    /// `(vendor, items) ` — `0x0060`, with **no** currency field.
    /// Otherwise the same shape as [`Request::VendorBuy`].
    VendorSell(dereth_protocol::trade::VendorSell),

    // ---------------------------------------------------------------------------------------
    // Five trade client-to-server events. Every opcode below is the one retail
    // actually sends, not taken from the catalogue:
    // adding an item sends `0x01F8`, accepting the trade
    // `0x01FA`, decline `0x01FB`,
    // closing negotiations `0x01F7`, and
    // resetting the trade **`0x0204`**, outside the
    // `0x01F6..0x01FE` block the rest of the family lives in.
    // ---------------------------------------------------------------------------------------
    /// `(item, position) ` — `0x01F8`. The second dword is the
    /// row's index in the secure-trade panel's own self list, which is
    /// why the panel supplies it.
    TradeAddToTrade(dereth_protocol::trade::TradeAddToTrade),
    /// `(trade) ` — `0x01FA`, carrying the whole packed
    /// `Trade`. Both the synchronized and out-of-sync accept paths send this opcode; the out-of-sync
    /// one sends a default-constructed `Trade` instead of the mirror, and that is the only
    /// difference on the wire.
    TradeAcceptTrade(dereth_protocol::trade::TradeAcceptTradeRequest),
    /// Decline the trade — `0x01FB`, empty body.
    TradeDeclineTrade(dereth_protocol::trade::TradeDeclineTradeRequest),
    /// Reset the trade — `0x0204`, empty body. The "Clear All Items"
    /// button, element `0x1000008A`.
    TradeResetTrade(dereth_protocol::trade::TradeResetTradeRequest),
    /// `() ` — `0x01F7`, empty body. Sent when the
    /// window is hidden or the partner leaves range
    /// (the range-exit handler).
    TradeCloseTradeNegotiations(dereth_protocol::trade::TradeCloseTradeNegotiations),

    /// Set the desired count for a spell-component class — `0x0224`.
    ///
    /// The spell-component panel's editable "keep this many" field, and the `/fillcomps clear`
    /// command, are its only two producers in the client.
    SetDesiredComponentLevel(dereth_protocol::combat::ComponentLevelRequest),

    /// `(add, id, name, type)` — `0x0058`.
    ///
    /// Sent by the chat-target menu's squelch row.
    ModifyCharacterSquelch(dereth_protocol::comms::CommunicationModifyCharacterSquelch),

    /// Add or remove an account squelch by name — `0x0059`.
    ///
    /// The sibling of the line above. `0x0059` has exactly two producer paths in retail — the
    /// squelch panel's Squelch Account and Remove actions, and the squelch/unsquelch commands.
    ModifyAccountSquelch(dereth_protocol::comms::CommunicationModifyAccountSquelch),

    /// Save the packed player options — `0x01A1`.
    ///
    /// The whole body is a codec-packed `PlayerModule`; all five recorded blobs round-trip
    /// byte-identical, including the `OrderedActionHeader` stamp.
    ///
    /// The two paths are an explicit save and the 480-second dirty flush, implemented by
    /// [`player::PlayerSystem::save_to_server`] and [`player::PlayerSystem::use_time_save`] here.
    /// The module is never composed — it is the blob the server sent, with the two option words
    /// read-modify-written and the client's narrower gate word
    /// recomputed.
    CharacterOptionsEvent(dereth_protocol::login::CharacterCharacterOptionsEvent),

    /// Answer a confirmation with type, context and acceptance — `0x0275`,
    /// an ordinary `OrderedActionHeader`-stamped game action on the Weenie queue.
    ///
    /// It answers `0x0274`. Without it every NPC yes/no question (`EmoteManager.cs:832`), every
    /// tinkering warning (`RecipeManager.cs:339`) and every skill, attribute and augmentation gem
    /// goes unanswered until ACE's thirty-second `ConfirmationManager` timeout aborts it, with
    /// nothing on screen at any point.
    ///
    /// The dialog-answer sender fires on
    /// **either** answer — the caller passes the byte it read out of dialog property `0x92`,
    /// not a constant `false`. A refusal is
    /// a message, not a silence.
    ///
    /// The body contains the type, context and acceptance in that order, each padded to
    /// four bytes. A refused send rolls back the action stamp.
    ConfirmationResponse(dereth_protocol::comms::CharacterConfirmationResponse),

    // ---------------------------------------------------------------------------------------
    // The twenty requests whose only producer in the retail client is a
    // `*` chat-command handler. See [`chat_cmd`] for the handler each one
    // comes from and its argument parsing.
    // ---------------------------------------------------------------------------------------
    /// `()` — `0x0063`, empty body. `@lifestone`/`@lif`/`@ls`.
    TeleToLifestone(dereth_protocol::combat::CharacterTeleToLifestone),
    /// Recall to the marketplace — `0x028D`. `@marketplace`/`@mar`/`@mp`.
    TeleToMarketplace(dereth_protocol::combat::CharacterTeleToMarketplace),
    /// `()` — `0x0027`. `@pkarena`/`@pka`, and only for a
    /// character whose player-killer flag is set.
    TeleToPkArena(dereth_protocol::combat::CharacterTeleToPkArena),
    /// `()` — `0x0026`. `@pklarena`/`@pla`, `IsPKLite` gated.
    TeleToPklArena(dereth_protocol::combat::CharacterTeleToPklArena),
    /// `()` — `0x028F`. `@pklite`/`@pkl`, and the gate is
    /// **inverted**: a character who is already a player killer is refused.
    EnterPkLite(dereth_protocol::combat::CharacterEnterPkLite),
    /// `()` — `0x0279`. `@die`, and only out of
    /// the suicide confirmation dialog's Yes arm.
    Suicide(dereth_protocol::combat::CharacterSuicide),
    /// `()` — `0x021E`. **It is not a command.**
    ///
    /// Player initialization ends by sending it, behind `if (!player_initialized)`. So
    /// retail asks the shard about the player's house **exactly once per session**, on `0x0013`,
    /// with no UI involved at all.
    QueryHouse(dereth_protocol::trade::HouseQueryHouse),
    /// Buy a house using the supplied payment items — `0x021C`.
    ///
    /// The housing payment path is its only caller. A recorded purchase confirms the body:
    /// the slumlord id followed by a packed list of the items dropped into the Buy tab.
    BuyHouse(dereth_protocol::trade::HouseBuyHouse),
    /// `(slumlord, items)` — `0x0221`. The same shape and the same
    /// caller; which of the two the payment sends is the panel's current house operation.
    RentHouse(dereth_protocol::trade::HouseRentHouse),
    /// Refresh the slumlord's profile — `0x0258`.
    ///
    /// The failed-transaction handler is its only caller, so this is a *retry* and
    /// never the thing that opens the window: a failed transaction re-asks the lord it is
    /// standing in front of so the panel's prices and paid counts come back in step with the
    /// shard's.
    QueryLord(dereth_protocol::trade::HouseQueryLord),
    /// Recall to the character's house — `0x0262`. `@hor`/`@hr`.
    TeleToHouse(dereth_protocol::trade::HouseTeleToHouse),
    /// Recall to the allegiance mansion — `0x0278`. `@hom`/`@hoa`.
    TeleToMansion(dereth_protocol::trade::HouseTeleToMansion),
    // ---- eleven house-command requests ------------------------------------------------------
    //
    // Each sender has exactly one caller
    // in retail: the house-command handler or
    // one of its four sub-handlers — there is no button, no panel and no other path. Eleven of
    // the thirteen (these eleven minus `0x025F`, plus `0x0262`/`0x0278` above) are in
    // a recorded house-command sweep.
    /// `(open)` — `0x0247`. `@house open` / `@house close`.
    SetOpenHouseStatus(dereth_protocol::trade::HouseSetOpenHouseStatus),
    /// `(visible)` — `0x0266`. `@house hooks on|off`, and
    /// **only** those two words: any third one falls to the help line without sending.
    SetHooksVisibility(dereth_protocol::trade::HouseSetHooksVisibility),
    /// `()` — `0x025F`. `@house boot_all`, `@house remove_all`
    /// and `@house boot -all`. The one command of the family the recorded sweep does **not**
    /// carry, so its bytes are composed longhand rather than compared against a recording.
    BootEveryone(dereth_protocol::trade::HouseBootEveryone),
    /// `(name)` — `0x024A`. `@house boot <name>`.
    BootSpecificHouseGuest(dereth_protocol::trade::HouseBootSpecificHouseGuest),
    /// `(name)` — `0x0245`. `@house guest add <name>`.
    AddPermanentGuest(dereth_protocol::trade::HouseAddPermanentGuest),
    /// Remove a permanent guest by name — `0x0246`.
    /// `@house guest remove <name>`.
    RemovePermanentGuest(dereth_protocol::trade::HouseRemovePermanentGuest),
    /// Remove all permanent guests — `0x025E`.
    /// `@house guest remove_all`.
    RemoveAllPermanentGuests(dereth_protocol::trade::HouseRemoveAllPermanentGuests),
    /// `()` — `0x024D`. `@house guest list|show` and
    /// `@house storage list|show`, which share the one message. **This is the request the
    /// `0x0257 House_UpdateHAR` receiver exists for**: in the recorded command sweep the shard
    /// answers it 153 ms later, twice.
    RequestFullGuestList(dereth_protocol::trade::HouseRequestFullGuestList),
    /// Change allegiance guest permission — `0x0267`.
    /// `@house guest add_allegiance` / `remove_allegiance`.
    ModifyAllegianceGuestPermission(dereth_protocol::trade::HouseModifyAllegianceGuestPermission),
    /// Change allegiance storage permission — `0x0268`.
    /// `@house storage add_allegiance` / `remove_allegiance`.
    ModifyAllegianceStoragePermission(
        dereth_protocol::trade::HouseModifyAllegianceStoragePermission,
    ),
    /// Change storage permission for a named guest — `0x0249`.
    /// `@house storage add|remove <name>`, one message with the flag inside it.
    ChangeStoragePermission(dereth_protocol::trade::HouseChangeStoragePermission),
    /// `()` — `0x025C`. `@house storage add -all`.
    AddAllStoragePermission(dereth_protocol::trade::HouseAddAllStoragePermission),
    /// `()` — `0x024C`. `@house storage remove_all`
    /// and `@house storage remove -all`.
    RemoveAllStoragePermission(dereth_protocol::trade::HouseRemoveAllStoragePermission),
    /// Query available houses by type — `0x0270`. `@hslist <type>` and
    /// `@house available <type>`. The answer is `0x0271 House_AvailableHouses`.
    ListAvailableHouses(dereth_protocol::trade::HouseListAvailableHouses),
    /// Abandon the house — `0x021F`, only after both confirmation dialogs accept.
    /// Two questions, two Yeses, one
    /// message; a No at either stage is silence.
    AbandonHouse(dereth_protocol::trade::HouseAbandonHouse),
    /// `(target)` — `0x01C2`. `@age`, always with target **0**: the
    /// selected-target query is restricted to privileged players. The answer arrives
    /// as `0x01C3`.
    QueryAge(dereth_protocol::admin::CharacterQueryAge),
    /// `(target)` — `0x01C4`. `@birth`, target **0**. There is no
    /// dedicated response: the shard answers with an ordinary `Communication_TextboxString`.
    QueryBirth(dereth_protocol::admin::CharacterQueryBirth),
    /// `(afk)` — `0x000F`. `@afk` / `@afk on` / `@afk off`,
    /// each guarded by the player's own `PropertyBool::Afk` so a repeat sends nothing.
    SetAfkMode(dereth_protocol::comms::CommunicationSetAfkMode),
    /// `(text)` — `0x0010`. `@afk msg <text>`, truncated to
    /// 191 characters and newline-terminated by the handler.
    SetAfkMessage(dereth_protocol::comms::CommunicationSetAfkMessage),
    /// `(add, msgType)` — `0x005B`. `@chat` (type 2)
    /// and `@notell` (type 3). The filter and unfilter commands use the same message.
    ModifyGlobalSquelch(dereth_protocol::comms::CommunicationModifyGlobalSquelch),
    /// List available channels — `0x0149`. `@index`.
    ChannelIndex(dereth_protocol::comms::CommunicationChannelIndexRequest),
    /// List channel members — `0x0148`. `@clist <name>`.
    ChannelList(dereth_protocol::comms::CommunicationChannelListRequest),
    /// Join a channel — `0x0145`. `@on <name>`.
    AddToChannel(dereth_protocol::comms::CommunicationAddToChannel),
    /// Leave a channel — `0x0146`. `@off <name>`.
    RemoveFromChannel(dereth_protocol::comms::CommunicationRemoveFromChannel),
    /// Display the consent list — `0x0217`. `@consent who` — **`who`**,
    /// not the `list` `dereth_protocol::admin`'s doc comment names.
    DisplayPlayerConsentList(dereth_protocol::admin::CharacterDisplayPlayerConsentList),
    /// Clear the consent list — `0x0216`. `@consent clear`.
    ClearPlayerConsentList(dereth_protocol::admin::CharacterClearPlayerConsentList),
    /// Remove consent for a named player — `0x0218`.
    /// `@consent remove <name>`.
    RemoveFromPlayerConsentList(dereth_protocol::admin::CharacterRemoveFromPlayerConsentList),
    /// `(name)` — `0x0219`. `@permit add <name>`.
    AddPlayerPermission(dereth_protocol::admin::CharacterAddPlayerPermission),
    /// `(name)` — **`0x021A`**, not the catalogue's
    /// `0x0220`. `@permit remove <name>`.
    RemovePlayerPermission(dereth_protocol::admin::CharacterRemovePlayerPermission),
    /// Join a board game with the supplied team — `0x0269`.
    ///
    /// The five requests below are the chess window's outbound operations. All allocate an
    /// ordered action stamp, encode the body and send, rolling the stamp back on failure.
    /// The join dialog's accepted arm is this request's only
    /// caller in retail and it always passes `-1` for the team.
    GameJoin(dereth_protocol::trade::GameJoin),
    /// Quit the board game — `0x026A`, empty body. Sent only after Yes in the resign dialog.
    GameQuit(dereth_protocol::trade::GameQuit),
    /// Move `(x0, y0)` to `(x1, y1)` — `0x026B`, **four raw ints** and not a
    /// `GameMoveData`. The board's move producer is its only caller.
    GameMove(dereth_protocol::trade::GameMove),
    /// `() ` — `0x026D`, empty body. The Pass button.
    GameMovePass(dereth_protocol::trade::GameMovePass),
    /// `(on) ` — `0x026E`. The Stalemate button, carrying the
    /// **toggled** value of the stalemate flag.
    GameStalemate(dereth_protocol::trade::GameStalemate),
}

/// Where requests go.
pub trait RequestSink {
    fn send(&mut self, r: Request);
}

/// A sink that keeps everything, for tests and for the corpus gate's request comparison.
#[derive(Debug, Default)]
pub struct RecordingRequests(pub Vec<Request>);

impl RequestSink for RecordingRequests {
    fn send(&mut self, r: Request) {
        self.0.push(r);
    }
}

/// A sink that drops everything — for the legality prechecks, which must run the same way whether
/// or not anything is listening.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullRequests;

impl RequestSink for NullRequests {
    fn send(&mut self, _: Request) {}
}
