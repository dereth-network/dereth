//! The player system and `PlayerModule` model: options, the 480 s dirty flush, shortcuts, spell
//! tabs, squelch, friends, titles, vitae and the recall bindings.
//!
//! The wire form of `PlayerModule` belongs to [`dereth_protocol::login::PlayerModule`]; what is here
//! is the behaviour hung off it.

pub mod options;
pub mod placement;

pub use options::{
    default_option_value, is_auto_save_option, option, OptionWord, Options, PLAYER_OPTIONS,
};

use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::login::{PlayerModule, ShortCutData};
use std::collections::{BTreeMap, BTreeSet};

/// The batched-save interval: **480 seconds (8 minutes)** after the first un-saved change, the
/// entire `PlayerModule` is packed and sent.
pub const DIRTY_FLUSH_SECONDS: f64 = 480.0;

/// The shortcut manager's 18-slot shortcut array.
///
/// 18 slots are stored but the retail UI shows a single row; the remaining slots are the alternate
/// bars. Allocate 18 and let the UI decide what to draw.
pub const SHORTCUT_SLOTS: usize = 18;

/// The eight hotbar spell tabs, `favorite_spells_[0..7]`.
pub const SPELL_TABS: usize = 8;

/// The player's default: all 14 `SpellbookFilter` bits.
pub const DEFAULT_SPELL_FILTERS: u32 = 0x3FFF;

/// `Option_TextType` — the per-chat-window 64-bit text-type filter, a `Bitfield64` inside the
/// `0x1000008C` array indexes by the chat window's id minus 1.
/// The Chat Options page's five filter checkboxes write it.
pub const CHAT_TEXT_TYPE_FILTER: u32 = 0x1000_007F;
/// `Option_DefaultOpacity` — the **idle** chat opacity, a top-level `Float` of
/// the gameplay-options collection.
pub const CHAT_DEFAULT_OPACITY: u32 = 0x1000_0080;
/// `Option_ActiveOpacity` — the focused / moused-over chat opacity.
pub const CHAT_ACTIVE_OPACITY: u32 = 0x1000_0081;

/// Every change callback produced by one option write, folded together.
///
/// One write can run the change hook twice. The two fellowship exclusions go through the *other*
/// option's own setter, and those setters end in a tail call into the change hook for the other
/// option that runs its whole body, the auto-save check and the option-changed event included. See [`PlayerSystem::set_option`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptionChange {
    /// Send player-option-changed event `0x0005` once per
    /// change-hook run whose option is auto-saved, **in wire order**: an exclusion's inner
    /// run sends before the outer one reaches its own send.
    pub sends: Vec<(usize, bool)>,
    /// The dirty flag was raised: a bit moved whose option is not auto-saved, so the whole module
    /// goes out at the next flush.
    pub deferred: bool,
    /// The engine side effect of the option that was asked for. (The only options an exclusion
    /// re-enters the change hook with are the two fellowship ordinals, and neither has one.)
    pub effect: Option<OptionSideEffect>,
}

impl OptionChange {
    /// At least one bit moved — the write was not a no-op.
    #[must_use]
    pub fn moved(&self) -> bool {
        !self.sends.is_empty() || self.deferred
    }
}

/// The engine side effects applies immediately.
///
/// This crate does not own the weather, the landscape or the camera, so the effect is reported
/// rather than applied; the six arms are exactly the client's switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSideEffect {
    /// Enable weather when this option is clear.
    EnableWeather(bool),
    /// Set the landscape day-mode flag.
    SetDay(bool),
    /// Set combat target tracking.
    TrackTarget(bool),
    /// Enable landscape fog when this option is clear.
    FogEnabled(bool),
}

/// The player system plus the player module.
#[derive(Debug, Clone, Default)]
pub struct PlayerSystem {
    /// The module's own `PlayerModule` — the blob the server sent, **kept**.
    ///
    /// The client has exactly one of these: the module *is* a `PlayerModule` plus a dirty
    /// flag, and packs that same object. Keeping the blob is what lets `0x01A1` re-pack it: a
    /// module rebuilt from the decomposed fields below would drop the gameplay-options collection,
    /// the generic qualities bag and every bit of the second option word this build does not
    /// model — including bit 25, which is set in all five recorded blobs while the public option
    /// getter stops at bit 24. See [`Self::client_packed_module`].
    pub module: Option<PlayerModule>,
    /// The account name, as the **server** spelled it.
    ///
    /// The character-set handler unpacks `0xF658`'s `CharacterSet`
    /// and copies the set's account name straight into this field
    /// (a narrow-string assignment). The client
    /// never types it into this field, never lower-cases it and never touches it again: what the
    /// login server echoed back in `0xF658` is what every later reader gets.
    ///
    /// Retail reads it back to start Turbine chat, for the logon sends that carry it (the
    /// enter-world request with game and account ids, and the delete and character-generation
    /// requests beside it). The use this crate needs is the magic system's appropriate-formula
    /// lookup at two sites, which passes the narrow account string to the customized
    /// spell-formula query. That is the whole of what the account name
    /// does to *gameplay*: it chooses which **tapers** a spell's formula asks for. See
    /// [`crate::magic::randomize_for_name`].
    ///
    /// Empty until `0xF658` lands. That is not a substitute value: the original empty string's
    /// hash is 0, so the pre-login client derives exactly the
    /// tapers this build derives with an empty name.
    pub account: String,
    pub options: Options,
    /// 18 slots; `None` is an empty slot.
    pub shortcuts: Vec<Option<ShortCutData>>,
    /// `favorite_spells_[0..7]`.
    pub spell_tabs: Vec<Vec<u32>>,
    /// `spell_filters_`.
    pub spell_filters: u32,
    /// `desired_comps_` — component `DataID` → the quantity the player wants kept in stock.
    pub desired_comps: BTreeMap<u32, i32>,
    pub timestamp_format: Option<String>,
    /// The dirty flag and the time of the first un-saved change.
    dirty_since: Option<ServerTime>,
    /// The player's teleport-in-progress flag.
    pub teleport_in_progress: bool,
    /// Whether the login "everything has arrived" notification has been sent.
    pub login_complete_sent: bool,
    /// Client-side only, not persisted. It exists so the client
    /// can grey out corpses already looted this session.
    pub opened_corpses: BTreeSet<ObjectId>,
    pub social: Social,
}

impl PlayerSystem {
    #[must_use]
    pub fn new() -> Self {
        Self {
            module: None,
            account: String::new(),
            options: Options::default(),
            shortcuts: vec![None; SHORTCUT_SLOTS],
            spell_tabs: vec![Vec::new(); SPELL_TABS],
            spell_filters: DEFAULT_SPELL_FILTERS,
            desired_comps: BTreeMap::new(),
            timestamp_format: None,
            dirty_since: None,
            teleport_in_progress: false,
            login_complete_sent: false,
            opened_corpses: BTreeSet::new(),
            social: Social::default(),
        }
    }

    /// Fill from a decoded `PlayerModule` blob.
    ///
    /// Initialization runs right after: it clears the dirty flag and
    /// pushes four options into the engine. The four are returned so the caller can apply them.
    pub fn apply_player_module(&mut self, m: &PlayerModule) -> [OptionSideEffect; 4] {
        // Keep the blob itself, not only what this crate models of it.
        self.module = Some(m.clone());
        self.options = Options {
            options: m.options,
            options2: m.options2,
        };
        self.shortcuts = vec![None; SHORTCUT_SLOTS];
        for sc in m.shortcuts.iter().flatten() {
            if let Ok(i) = usize::try_from(sc.index) {
                if i < SHORTCUT_SLOTS {
                    self.shortcuts[i] = Some(*sc);
                }
            }
        }
        self.spell_tabs = vec![Vec::new(); SPELL_TABS];
        for (i, bar) in m.spell_bars.iter().enumerate().take(SPELL_TABS) {
            self.spell_tabs[i].clone_from(bar);
        }
        self.spell_filters = m.spell_filters;
        self.desired_comps = m
            .desired_comps
            .as_ref()
            .map(|h| h.entries.iter().copied().collect())
            .unwrap_or_default();
        self.timestamp_format.clone_from(&m.timestamp_format);
        // Clear the dirty flag and the first-dirtied time.
        self.dirty_since = None;
        [
            OptionSideEffect::SetDay(self.options.get(option::PERSISTENT_AT_DAY)),
            OptionSideEffect::FogEnabled(!self.options.get(option::DISABLE_DISTANCE_FOG)),
            OptionSideEffect::EnableWeather(
                !self.options.get(option::DISABLE_MOST_WEATHER_EFFECTS),
            ),
            OptionSideEffect::TrackTarget(self.options.get(option::VIEW_COMBAT_TARGET)),
        ]
    }

    /// The player module's option setter — the bit, and then the module's changed hook
    /// for that option if it moved.
    ///
    /// Returns every `0x0005` the write put on the wire, in order, plus any engine side effect.
    ///
    /// # The two fellowship exclusions run through the other option's own changed hook
    ///
    /// The changed hook's `IgnoreFellowshipRequests` arm clears
    /// `FellowshipAutoAcceptRequests` through its setter when ignore is now set,
    /// and its `FellowshipAutoAcceptRequests` arm is the mirror. Neither
    /// setter merely clears a bit:
    ///
    /// The auto-accept setter writes bit 29 and then reports ordinal `0x12` through the module's
    /// option-changed path. The ignore setter similarly writes bit 3 and reports ordinal `2`.
    /// Both paths run the complete notification behavior after the bit write; neither is a direct
    /// bit-table mutation. The ordering matters when setting one option clears the other:
    ///
    /// ```text
    /// clear the excluded option and send its change
    /// then write the requested option and send its change
    /// ```
    ///
    /// So the inner changed hook for `FellowshipAutoAcceptRequests` runs its **whole** body — the
    /// option-changed notice from which every bound checkbox redraws, the
    /// switch (a no-op: its arm tests the bit being *set*, and this is a clear), and
    /// the auto-save test → the `0x0005` option change `(0x12, 0)` — **before** the outer call
    /// sends `(2, 1)`. One click, two `0x0005`s, the cleared option's first.
    ///
    /// Clearing the other bit with a bare [`Options::set`] would tell the boxes and tell the shard
    /// nothing. ACE's `Player.SetCharacterOption` sets exactly the one bit it is told and applies
    /// no exclusion of its own, so after *Auto-Accept on, Ignore on, Ignore off* the shard would
    /// still hold `AutomaticallyAcceptFellowshipRequests` while the client's word — and the box
    /// drawn from it — said off: Auto-Accept shows OFF while auto-accept is ON, until ticking the
    /// box on and off again sends the `(0x12, 0)` the shard was missing.
    pub fn set_option(&mut self, ordinal: usize, value: bool, now: ServerTime) -> OptionChange {
        if !self.options.set(ordinal, value) {
            return OptionChange::default();
        }
        let mut change = match ordinal {
            // Clear auto-accept through its setter → changed hook for `0x12`: its sends come first.
            option::IGNORE_FELLOWSHIP_REQUESTS if value => {
                self.set_option(option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS, false, now)
            }
            // Clear ignore through its setter → changed hook for `2`.
            option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS if value => {
                self.set_option(option::IGNORE_FELLOWSHIP_REQUESTS, false, now)
            }
            option::DISABLE_MOST_WEATHER_EFFECTS => OptionChange {
                effect: Some(OptionSideEffect::EnableWeather(!value)),
                ..OptionChange::default()
            },
            option::PERSISTENT_AT_DAY => OptionChange {
                effect: Some(OptionSideEffect::SetDay(value)),
                ..OptionChange::default()
            },
            option::VIEW_COMBAT_TARGET => OptionChange {
                effect: Some(OptionSideEffect::TrackTarget(value)),
                ..OptionChange::default()
            },
            option::DISABLE_DISTANCE_FOG => OptionChange {
                effect: Some(OptionSideEffect::FogEnabled(!value)),
                ..OptionChange::default()
            },
            _ => OptionChange::default(),
        };
        // The setter writes the bit **in the module**, and the module's
        // save-to-server packs that same object. There is one option word in the
        // client and there is one here: [`Self::options`] is the model, [`Self::module`] is the
        // blob, and the mirror is one assignment rather than a second bit table. It runs **after**
        // the two mutual exclusions above, because those are option-setter calls of their own and the
        // module has to carry their result too.
        self.mirror_options_into_module();
        // The tail: an auto-save option → a `0x0005` option change carrying the option and its
        // new value, else the dirty stamp.
        if is_auto_save_option(ordinal) {
            change.sends.push((ordinal, value));
        } else {
            if self.dirty_since.is_none() {
                self.dirty_since = Some(now);
            }
            change.deferred = true;
        }
        change
    }

    /// A gameplay-option property change — the module's changed hook for a
    /// property. Gameplay options are **never** auto-saved individually.
    pub fn mark_dirty(&mut self, now: ServerTime) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(now);
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// Copy the two option words into the retained blob.
    ///
    /// A no-op before `0x0013` has landed, which is the only state in which the two can be out of
    /// step: [`Self::options`] then holds `DEFAULT_OPTIONS` / `DEFAULT_OPTIONS2`
    /// and there is no server word to read-modify-write. Writing those defaults into a module the
    /// server has not sent is exactly the "compose a fresh word from UI state" mistake, so it does
    /// not happen: with no module there is nothing to mirror into and nothing to send.
    fn mirror_options_into_module(&mut self) {
        if let Some(m) = self.module.as_mut() {
            m.options = self.options.options;
            m.options2 = self.options.options2;
        }
    }

    /// Return the module **as the client would put it on the wire**, or `None` when the server has
    /// not sent one.
    ///
    /// The unpacker reads ten gates and `0x0013` exercises them; the pack-header step decides what
    /// the client may emit and is narrower, so a module taken off a `0x0013` and written back
    /// verbatim is **not** what the client sends. Its four statements, with the compiler's common
    /// subexpressions unfolded:
    ///
    /// ```text
    /// if (shortcuts_)            flags |= 0x0001;
    ///                            flags |= 0x0400;
    /// if (desired_comps_)        flags |= 0x0008;
    ///                            flags |= 0x0060;
    /// if (player options data)   flags |= 0x0100;
    /// if (gameplay options non-empty) flags |= 0x0200;
    /// ```
    ///
    /// So `0x0400` and `0x0060` are unconditional — a client-packed module always carries eight
    /// spell bars, the spell filters and the second option word — and `0x0004`, `0x0010` and
    /// `0x0080` are never set. The packer has **no branch that writes the timestamp string**, so
    /// `timestamp_format` is unreachable client to server by construction and is dropped here
    /// rather than left to trip [`PlayerModule`]'s flag/field agreement check.
    ///
    /// Everything else is the module the server sent, untouched: the gameplay-options collection, the
    /// generic-qualities bag, the desired components, the shortcuts, and both option words
    /// **including the bits this build does not model**. That is the point of keeping the blob.
    ///
    /// All five recorded `0x01A1` bodies carry `option_flags = 0x0660`, which is what this
    /// produces for a module with no shortcuts, no desired comps and no generic qualities.
    #[must_use]
    pub fn client_packed_module(&self) -> Option<PlayerModule> {
        use dereth_protocol::login::player_module_flags as f;
        let mut m = self.module.clone()?;
        let mut flags = f::SPELL_LISTS_8 | f::SPELLBOOK_FILTERS | f::CHARACTER_OPTIONS_2;
        if m.shortcuts.is_some() {
            flags |= f::SHORTCUT;
        }
        if m.desired_comps.is_some() {
            flags |= f::DESIRED_COMPS;
        }
        if m.generic_qualities.is_some() {
            flags |= f::GENERIC_QUALITIES_DATA;
        }
        // A non-zero element count — an empty bag does not set the gate, which is not the same test as
        // `is_some()`.
        if m.gameplay_options
            .as_ref()
            .is_some_and(|o| !o.properties.entries.is_empty())
        {
            flags |= f::GAMEPLAY_OPTIONS;
        } else {
            m.gameplay_options = None;
        }
        // The packer never writes it, so the pack header never gates it.
        m.timestamp_format = None;
        // `0x0400` is unconditional, so the size and pack loops run exactly eight times.
        m.spell_bars.resize(SPELL_TABS, Vec::new());
        m.option_flags = flags;
        Some(m)
    }

    /// The module's save-to-server `(force)` — the `0x01A1` sender.
    ///
    /// ```text
    /// if (dirty || force) send the character-options event with the player module;
    /// dirty = false;
    /// ```
    ///
    /// The dirty flag is cleared **whether or not anything was sent**, which is the client's own
    /// code and not a simplification: a forced save with nothing dirty still sends, and an
    /// un-forced save with nothing dirty still clears. Returns whether a message went out.
    ///
    /// With no module retained nothing is sent and the flag is still cleared, so a client that
    /// somehow marked itself dirty before `0x0013` cannot spin.
    pub fn save_to_server(&mut self, req: &mut dyn crate::RequestSink, force: bool) -> bool {
        let send = self.dirty_since.is_some() || force;
        self.dirty_since = None;
        if !send {
            return false;
        }
        let Some(module) = self.client_packed_module() else {
            return false;
        };
        req.send(crate::Request::CharacterOptionsEvent(
            dereth_protocol::login::CharacterCharacterOptionsEvent { module },
        ));
        true
    }

    /// Behavior: the 480-second flush, **sending**.
    ///
    /// This is [`Self::use_time`]'s timer test with the character-options send attached, which is
    /// what the client's timer does; without the send, the deferred half of every option change
    /// would never reach the server.
    pub fn use_time_save(&mut self, req: &mut dyn crate::RequestSink, now: ServerTime) -> bool {
        if !self.use_time(now) {
            return false;
        }
        let Some(module) = self.client_packed_module() else {
            return false;
        };
        req.send(crate::Request::CharacterOptionsEvent(
            dereth_protocol::login::CharacterCharacterOptionsEvent { module },
        ));
        true
    }

    /// Behavior: **480 s** after the first un-saved change, the entire
    /// blob is sent. It fires **once**, not per change.
    ///
    /// The comparison is a strict `>` on the first-dirtied time plus 480.0.
    pub fn use_time(&mut self, now: ServerTime) -> bool {
        let Some(since) = self.dirty_since else {
            return false;
        };
        if now.0 > since.0 + DIRTY_FLUSH_SECONDS {
            self.dirty_since = None;
            return true;
        }
        false
    }

    /// The player module's add-shortcut.
    ///
    /// **Both mirrors move.** `shortcuts_` is one object in the client; this crate
    /// keeps the server's blob ([`Self::module`], what [`Self::client_packed_module`] re-packs) and
    /// the decomposed [`Self::shortcuts`] the toolbar reads, exactly as it does for the spell
    /// favourites — and for the same reason, spelled out in [`Self::add_spell_favorite`]'s doc:
    /// *"writing one and not the other would either lose the change at the next `0x01A1` or leave
    /// the bar showing the old list"*.
    ///
    /// Moving only the slots would be the first of those. The drop's own `0x019C` reaches the
    /// shard, so the shortcut persists — and then the next `0x01A1`, re-packed from a blob that
    /// never heard of it, **overwrites the shard's shortcut list with the one the last `0x0013`
    /// carried**. `App::log_off_character` sends a `0x01A1` on the way out, so every logoff
    /// would do it.
    pub fn add_shortcut(&mut self, sc: ShortCutData) -> bool {
        let Ok(i) = usize::try_from(sc.index) else {
            return false;
        };
        if i >= SHORTCUT_SLOTS {
            return false;
        }
        self.shortcuts[i] = Some(sc);
        self.mirror_shortcuts_into_module();
        true
    }

    /// The player module's remove-shortcut. Mirrors into the retained blob for
    /// [`Self::add_shortcut`]'s reason.
    pub fn remove_shortcut(&mut self, index: usize) -> bool {
        if index >= SHORTCUT_SLOTS {
            return false;
        }
        let removed = self.shortcuts[index].take().is_some();
        if removed {
            self.mirror_shortcuts_into_module();
        }
        removed
    }

    /// The decomposed slots back into the retained `PlayerModule`, in slot order.
    ///
    /// `None` when the bar is empty, because packing gates the section on
    /// `shortcuts_` being present at all and [`PlayerModule`]'s flag/field agreement check refuses
    /// a `Some(vec![])` with the bit clear.
    fn mirror_shortcuts_into_module(&mut self) {
        let list: Vec<ShortCutData> = self.shortcuts.iter().flatten().copied().collect();
        if let Some(m) = self.module.as_mut() {
            m.shortcuts = (!list.is_empty()).then_some(list);
        }
    }

    /// The client's sweep — which slot holds `item`, if any.
    ///
    /// The client asks the shortcut-bar elements whether the item is present. Those elements are
    /// filled from the shortcut slots every frame, so the array
    /// is the same question one seam earlier.
    #[must_use]
    pub fn shortcut_slot_of(&self, item: ObjectId) -> Option<usize> {
        self.shortcuts
            .iter()
            .position(|s| s.is_some_and(|s| s.object_id == item))
    }

    /// The object in shortcut slot `index`, if any. Toolbar removal reads it before removing it and
    /// returns it so the caller can re-home whatever the drop displaced.
    #[must_use]
    pub fn shortcut_at(&self, index: usize) -> Option<ObjectId> {
        let sc = (*self.shortcuts.get(index)?)?;
        (sc.object_id.0 != 0).then_some(sc.object_id)
    }

    /// The toolbar's add-shortcut out-of-range fallback — a plain scan
    /// from slot 0 for the first empty one, with no wrap and no "to the right of".
    #[must_use]
    pub fn first_empty_shortcut(&self) -> Option<usize> {
        self.shortcuts.iter().position(Option::is_none)
    }

    /// The toolbar's first-empty-shortcut-to-the-right-of.
    ///
    /// Two loops, and the wrap is **not** a full circle: the first runs `index + 1 ..= last`, the
    /// second runs `0 ..= index` and gives up the moment it passes `index`
    /// (it gives up once the counter exceeds `index`, at the bottom of the second loop, with the
    /// increment *before* the test — so slot `index` itself is the last one examined).
    #[must_use]
    pub fn first_empty_shortcut_to_the_right_of(&self, index: usize) -> Option<usize> {
        (index + 1..SHORTCUT_SLOTS)
            .chain(0..=index.min(SHORTCUT_SLOTS - 1))
            .find(|i| self.shortcuts.get(*i).is_some_and(Option::is_none))
    }

    /// The player module's favourite-spell list for one tab.
    #[must_use]
    pub fn favorite_spells(&self, tab: usize) -> Option<&Vec<u32>> {
        self.spell_tabs.get(tab)
    }

    /// Add `spell` as a favorite at `index` in `bank`.
    ///
    /// ```text
    /// if ((-1 < bank) && (bank < 8))
    ///     insert_at(&favorite_spells_[bank], index, &spell);
    /// ```
    ///
    /// List insertion walks `index` nodes from the head and links the new node **before**
    /// the one it lands on; if the walk runs off the end — which `-1` does immediately, and which
    /// the caller's own "append" index (the spell count *after* its increment, i.e. one past the
    /// last row) also does — it pushes at the tail. So an out-of-range index is an append and not
    /// a refusal, and **there is no capacity**: a favourites list can hold any number of spells.
    ///
    /// Both mirrors move. `favorite_spells_` is one object in the client; this crate keeps the
    /// server's blob (`module`, what [`Self::client_packed_module`] re-packs) *and* the decomposed
    /// [`Self::spell_tabs`] the UI reads, so writing one and not the other would either lose the
    /// change at the next `0x01A1` or leave the bar showing the old list.
    ///
    /// Returns false only for a bank outside `0..8`.
    pub fn add_spell_favorite(&mut self, spell: u32, index: i32, bank: usize) -> bool {
        if bank >= SPELL_TABS {
            return false;
        }
        let at = usize::try_from(index).unwrap_or(usize::MAX);
        let tab = &mut self.spell_tabs[bank];
        let at = at.min(tab.len());
        tab.insert(at, spell);
        let updated = tab.clone();
        if let Some(m) = self.module.as_mut() {
            m.spell_bars.resize(SPELL_TABS, Vec::new());
            m.spell_bars[bank] = updated;
        }
        true
    }

    /// Remove `spell` from favorites in `bank`, which
    /// unlinks the **first** node carrying that value and leaves a list that has none alone.
    pub fn remove_spell_favorite(&mut self, spell: u32, bank: usize) -> bool {
        if bank >= SPELL_TABS {
            return false;
        }
        let tab = &mut self.spell_tabs[bank];
        let Some(i) = tab.iter().position(|s| *s == spell) else {
            return false;
        };
        tab.remove(i);
        let updated = tab.clone();
        if let Some(m) = self.module.as_mut() {
            m.spell_bars.resize(SPELL_TABS, Vec::new());
            m.spell_bars[bank] = updated;
        }
        true
    }

    /// The player module's clear-desired-components.
    pub fn clear_desired_comps(&mut self) {
        self.desired_comps.clear();
    }

    /// Return the desired component level for `wcid`, or 0 when no entry exists.
    #[must_use]
    pub fn desired_comp_level(&self, wcid: u32) -> i32 {
        self.desired_comps.get(&wcid).copied().unwrap_or(0)
    }

    /// Set the desired component level for `wcid`.
    ///
    /// ```text
    /// if ((level < 0) || (5000 < level)) return false;
    /// if (!desired_comps_) desired_comps_ = new PackableHashTable(0x100);
    /// if (lookup(wcid)) { *slot = level; return true; }
    /// return add(wcid, level) != 0;
    /// ```
    ///
    /// The bound is `0 <= level <= 5000` and it is enforced **three** times in the client: twice in
    /// the spell-component panel as `(-1 < n) && (n < 0x1389)` and once here as `(n < 0) || (5000 < n)`.
    /// The two spellings agree — `0x1389` is 5001 — and both are reproduced, here and at the panel
    /// seam, because the panel's copy is what reverts the edit box.
    ///
    /// Note what this does **not** reject: `/fillcomps clear` sends `(INVALID_DID, -1)` on the wire
    /// and calls [`Self::clear_desired_comps`] instead of coming through here, so the negative
    /// sentinel never meets this guard.
    pub fn set_desired_comp_level(&mut self, wcid: u32, level: i32) -> bool {
        if !(0..=MAX_DESIRED_COMP_LEVEL).contains(&level) {
            return false;
        }
        self.desired_comps.insert(wcid, level);
        true
    }
}

/// The client's inclusive upper bound, expressed by the setter as
/// `5000 < level`. The spell-component panel writes the same bound as
/// `n < 0x1389`; see [`crate::magic::MAX_DESIRED_COMP_LEVEL`], which is that exclusive form.
pub const MAX_DESIRED_COMP_LEVEL: i32 = 5000;

/// Squelch, friends and titles — the model half; the windows belong to the UI.
#[derive(Debug, Clone, Default)]
pub struct Social {
    /// Squelched **accounts** by name.
    pub squelched_accounts: BTreeMap<String, u32>,
    /// Character-specific squelch table.
    pub squelched_characters: BTreeMap<ObjectId, SquelchInfo>,
    /// The "squelch this whole channel" entry.
    pub global_squelch: SquelchInfo,
    /// `Social_FriendsUpdate` (0x0021). Held by communication state, not the player system.
    ///
    /// `crate::friends` writes it and the friends panel draws it.
    pub friends: Vec<Friend>,
    /// Currently displayed character title.
    pub display_title: u32,
    /// Available character-title list.
    pub titles: Vec<u32>,
}

/// `SquelchInfo` (0x18).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SquelchInfo {
    /// A `vlong` bit vector of the chat-type message types suppressed for this
    /// target. Carried as a set of type numbers.
    pub squelch_msgs: BTreeSet<u32>,
    /// The zone-squelch flag. No client path reads it; it is stored opaquely so a server that
    /// sets it
    /// is not silently dropped.
    pub is_zone_squelch: i32,
    pub name: String,
}

/// One row of `Social_FriendsUpdate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friend {
    pub id: ObjectId,
    pub name: String,
    pub online: bool,
    /// The friend's own *Appear Offline* option, the
    /// `AppearOffline_PlayerOption` check box binds to
    /// `0x1000052C`. It is not *"whether this character has added you back"*, which belongs to
    /// the friend-of list.
    /// No friends-panel path reads it; it is carried so a server that sets it is not
    /// silently dropped, on the same terms as above.
    pub appear_offline: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flush fires once at four hundred and eighty seconds not per change.
    #[test]
    fn the_flush_fires_once_at_four_hundred_and_eighty_seconds_not_per_change() {
        let mut p = PlayerSystem::new();
        assert!(!p.is_dirty());
        // A non-auto-save option: batched.
        let change = p.set_option(option::ADVANCED_COMBAT_UI, true, ServerTime(100.0));
        assert!(change.deferred && change.sends.is_empty(), "{change:?}");
        assert!(p.is_dirty());

        // A second change does not restart the clock.
        let change = p.set_option(option::SALVAGE_MULTIPLE, true, ServerTime(400.0));
        assert!(change.deferred && change.sends.is_empty(), "{change:?}");

        assert!(!p.use_time(ServerTime(579.0)));
        assert!(
            !p.use_time(ServerTime(580.0)),
            "the comparison is a strict >"
        );
        assert!(
            p.use_time(ServerTime(580.001)),
            "480 s after the FIRST change"
        );
        assert!(!p.use_time(ServerTime(9999.0)), "and only once");
        assert!(!p.is_dirty());
    }

    /// Oracle: §4's auto-save split — 21 options go out immediately, everything else is
    /// batched and does not mark the module dirty twice.
    #[test]
    fn an_auto_save_option_is_sent_immediately_and_never_marks_dirty() {
        let mut p = PlayerSystem::new();
        let change = p.set_option(option::IGNORE_ALLEGIANCE_REQUESTS, true, ServerTime(0.0));
        assert_eq!(change.sends, vec![(1, true)]);
        assert!(!change.deferred);
        assert!(
            !p.is_dirty(),
            "an immediate save does not mark the module dirty"
        );
        assert!(!p
            .set_option(option::IGNORE_ALLEGIANCE_REQUESTS, true, ServerTime(0.0))
            .moved());
        assert!(!p.use_time(ServerTime(100_000.0)));
    }

    /// Oracle: §4's `OnChanged` switch — six arms, four of them engine side effects and two mutual
    /// exclusions.
    #[test]
    fn on_changed_applies_the_six_documented_arms() {
        let mut p = PlayerSystem::new();
        assert_eq!(
            p.set_option(option::DISABLE_MOST_WEATHER_EFFECTS, true, ServerTime(0.0))
                .effect,
            Some(OptionSideEffect::EnableWeather(false))
        );
        assert_eq!(
            p.set_option(option::PERSISTENT_AT_DAY, true, ServerTime(0.0))
                .effect,
            Some(OptionSideEffect::SetDay(true))
        );
        assert_eq!(
            p.set_option(option::VIEW_COMBAT_TARGET, true, ServerTime(0.0))
                .effect,
            Some(OptionSideEffect::TrackTarget(true))
        );
        assert_eq!(
            p.set_option(option::DISABLE_DISTANCE_FOG, true, ServerTime(0.0))
                .effect,
            Some(OptionSideEffect::FogEnabled(false))
        );

        // The two mutual exclusions. Note that they hang off `OnChanged`, so an option that is
        // already at the requested value does not trigger one: `IgnoreFellowshipRequests` is in
        // `DEFAULT_OPTIONS`, so it has to be cleared first for the write to be a change.
        p.options.set(option::IGNORE_FELLOWSHIP_REQUESTS, false);
        p.options.set(option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS, true);
        let change = p.set_option(option::IGNORE_FELLOWSHIP_REQUESTS, true, ServerTime(0.0));
        assert!(!p.options.get(option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS));
        assert_eq!(
            change.sends,
            vec![
                (option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS, false),
                (option::IGNORE_FELLOWSHIP_REQUESTS, true)
            ],
            "the cleared option's 0x0005 goes first"
        );
        let change = p.set_option(
            option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS,
            true,
            ServerTime(0.0),
        );
        assert!(!p.options.get(option::IGNORE_FELLOWSHIP_REQUESTS));
        assert_eq!(
            change.sends,
            vec![
                (option::IGNORE_FELLOWSHIP_REQUESTS, false),
                (option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS, true)
            ]
        );
        // And turning either **off** excludes nothing: `OnChanged`'s arms test the bit being set.
        assert_eq!(
            p.set_option(
                option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS,
                false,
                ServerTime(0.0)
            )
            .sends,
            vec![(option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS, false)]
        );
        assert!(
            !p.options.get(option::IGNORE_FELLOWSHIP_REQUESTS),
            "Auto off leaves Ignore alone"
        );
    }

    /// Oracle: §4's — it clears the dirty flag and pushes four options,
    /// in that order.
    #[test]
    fn applying_a_player_module_clears_dirty_and_reports_four_side_effects() {
        let mut p = PlayerSystem::new();
        p.mark_dirty(ServerTime(0.0));
        let m = PlayerModule {
            options: options::DEFAULT_OPTIONS,
            options2: options::DEFAULT_OPTIONS2,
            spell_bars: vec![vec![1, 2, 3], vec![4]],
            shortcuts: Some(vec![ShortCutData {
                index: 3,
                object_id: ObjectId(9),
                spell_id: 0,
            }]),
            spell_filters: DEFAULT_SPELL_FILTERS,
            ..PlayerModule::default()
        };
        let effects = p.apply_player_module(&m);
        assert!(!p.is_dirty());
        assert_eq!(
            effects,
            [
                // `PersistentAtDay` is *not* in `DEFAULT_OPTIONS2`; see the decomposition
                // test in `player::options`.
                OptionSideEffect::SetDay(false),
                OptionSideEffect::FogEnabled(true),
                OptionSideEffect::EnableWeather(true),
                OptionSideEffect::TrackTarget(false),
            ]
        );
        assert_eq!(p.shortcuts.len(), SHORTCUT_SLOTS);
        assert_eq!(p.shortcuts[3].unwrap().object_id, ObjectId(9));
        assert_eq!(p.spell_tabs.len(), SPELL_TABS);
        assert_eq!(p.spell_tabs[0], vec![1, 2, 3]);
        assert_eq!(p.spell_tabs[7], Vec::<u32>::new());
    }

    /// Oracle: 18 slots are allocated even though the retail UI shows one row.
    #[test]
    fn eighteen_shortcut_slots_are_allocated() {
        let mut p = PlayerSystem::new();
        assert_eq!(p.shortcuts.len(), 18);
        assert!(p.add_shortcut(ShortCutData {
            index: 17,
            object_id: ObjectId(1),
            spell_id: 0
        }));
        assert!(!p.add_shortcut(ShortCutData {
            index: 18,
            object_id: ObjectId(1),
            spell_id: 0
        }));
        assert!(p.remove_shortcut(17));
        assert!(!p.remove_shortcut(17));
    }

    /// Oracle: §6 — eight spell tabs, and the filter default is all 14 bits.
    #[test]
    fn there_are_eight_spell_tabs_and_the_filter_defaults_to_all_fourteen_bits() {
        let p = PlayerSystem::new();
        assert_eq!(p.spell_tabs.len(), 8);
        assert!(p.favorite_spells(7).is_some());
        assert!(p.favorite_spells(8).is_none());
        assert_eq!(p.spell_filters, 0x3FFF);
        assert_eq!(DEFAULT_SPELL_FILTERS.count_ones(), 14);
    }
}
