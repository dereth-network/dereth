//! `SpellbookPanel`'s school × level filter.
//!
//! "Five school filters × eight level filters; the list shows the **intersection**."
//!
//! The class honours a `Level_9` filter bit but caches only eight level buttons. The layout
//! binding table in [`super::catalogue`] has eight, so eight buttons are bound; [`SpellFilter`] carries nine level bits so a spell of level 9 can still be admitted when
//! the bit is set from elsewhere.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

/// `Dialog` — layout enum 2, the one every root lives in.
pub const DIALOG_LAYOUT: dereth_ui::LayoutEnum = dereth_ui::LayoutEnum(2);

use crate::items::widget::ItemListWidget;
use crate::view::{GameView, SpellEntry};

/// The five magic schools, in the order the post-init binds their buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum School {
    Creature = 0,
    Item = 1,
    Life = 2,
    War = 3,
    Void = 4,
}

impl School {
    pub const ALL: [Self; 5] = [
        Self::Creature,
        Self::Item,
        Self::Life,
        Self::War,
        Self::Void,
    ];

    /// The filter button's element id.
    #[must_use]
    pub const fn button(self) -> ElementId {
        match self {
            Self::Creature => ElementId(0x1000_0298),
            Self::Item => ElementId(0x1000_0299),
            Self::Life => ElementId(0x1000_029A),
            Self::War => ElementId(0x1000_029B),
            // Void is the odd one out: its id is far from the other four because it was added later.
            Self::Void => ElementId(0x1000_05C0),
        }
    }
}

/// The eight level buttons the post-init caches, levels 1…8.
#[must_use]
pub const fn level_button(level: u32) -> Option<ElementId> {
    Some(match level {
        1 => ElementId(0x1000_029C),
        2 => ElementId(0x1000_029D),
        3 => ElementId(0x1000_029E),
        4 => ElementId(0x1000_029F),
        5 => ElementId(0x1000_02A0),
        6 => ElementId(0x1000_02A1),
        7 => ElementId(0x1000_02A2),
        // Level 8's id is far from the other seven, like Void's.
        8 => ElementId(0x1000_054E),
        _ => return None,
    })
}

/// How many level buttons the layout provides.
pub const LEVEL_BUTTON_COUNT: u32 = 8;
/// The highest level bit the filter carries, including the ninth (#173).
pub const MAX_LEVEL_BIT: u32 = 9;

/// One spell, reduced to what the filter tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellRow {
    pub school: School,
    /// 1…9.
    pub level: u32,
}

/// The spellbook's live filter: a school set and a level set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpellFilter {
    /// Bit *n* is [`School`] *n*.
    pub schools: u8,
    /// Bit *n* is level *n+1*, up to [`MAX_LEVEL_BIT`].
    pub levels: u16,
}

impl SpellFilter {
    /// Every school and every level.
    #[must_use]
    pub fn all() -> Self {
        Self {
            schools: 0b1_1111,
            levels: (1 << MAX_LEVEL_BIT) - 1,
        }
    }

    /// Toggle one school's button.
    pub fn toggle_school(&mut self, s: School) {
        self.schools ^= 1 << (s as u8);
    }

    /// Toggle one level's button.
    pub fn toggle_level(&mut self, level: u32) {
        if (1..=MAX_LEVEL_BIT).contains(&level) {
            self.levels ^= 1 << (level - 1);
        }
    }

    /// "the list shows the intersection" — a spell is visible when **both** its school and its
    /// level are selected.
    #[must_use]
    pub fn accepts(&self, s: SpellRow) -> bool {
        let school_on = self.schools & (1 << (s.school as u8)) != 0;
        let level_on =
            (1..=MAX_LEVEL_BIT).contains(&s.level) && self.levels & (1 << (s.level - 1)) != 0;
        school_on && level_on
    }

    /// Apply the filter to a spellbook.
    #[must_use]
    pub fn visible(&self, spells: &[SpellRow]) -> Vec<SpellRow> {
        spells
            .iter()
            .copied()
            .filter(|s| self.accepts(*s))
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// The live panel
// ---------------------------------------------------------------------------------------------

/// The spell list — element `0x10000295`, and it is a **`ItemListWidget`**
/// (type `0x10000031`), not a `ListBox`.
///
/// The spellbook's post-init looks up `0x10000295` and casts it to an item list, and every
/// write to it is an item-list call — flush, then insert a spell shortcut per spell. So the
/// spellbook reuses [`ItemListWidget`] exactly as the backpack does — the only difference is that
/// a slot holds a **spell id** instead of an object id.
/// [verified against the live element tree: `0x10000295` is type `0x10000031`, one
/// column (`0x5F` = 1), `FixedListSize` -1, slot id `0x10000343`]
pub const SPELL_LIST: ElementId = ElementId(0x1000_0295);

/// `SpellbookPanel` itself — element `0x100002AC` of the spell page `0x10000190`, type
/// `0x1000002E`.
///
/// Named because the search has to start here rather than at the screen root: the sibling
/// sub-panels of the same page carry their own lists, and starting higher is the trap that would
/// find `SpellComponentPanel`'s `0x10000464` subtree first.
pub const PANEL: ElementId = ElementId(0x1000_02AC);

/// The player-module unpack's default for the spell filters when section `0x0020` is absent —
/// every school and every level.
///
/// Defined in [`dereth_client_contract::spellbook`], because
/// `GameView::spell_filters`'s default body is this number and the contract crate may not depend
/// on this one.
pub use dereth_client_contract::spellbook::DEFAULT_SPELL_FILTERS;

impl School {
    /// The school value this button filters — the client's switch on the spell's school.
    ///
    /// The `MagicSchool` enum is 1 War, 2 Life, 3 Item, 4 Creature, 5 Void, and the function maps
    /// them to filter bits `0x8`, `0x4`, `0x2`, `0x1`, `0x2000` in that order — i.e. **bit 0 is
    /// school 4**. That inversion is why this mapping is a function and not `self as u32 + 1`.
    #[must_use]
    pub const fn magic_school(self) -> u32 {
        match self {
            Self::Creature => 4,
            Self::Item => 3,
            Self::Life => 2,
            Self::War => 1,
            Self::Void => 5,
        }
    }

    /// The `School` a school value names, or `None` for the switch's default arm, which
    /// filters the spell out entirely.
    #[must_use]
    pub const fn from_magic_school(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::War,
            2 => Self::Life,
            3 => Self::Item,
            4 => Self::Creature,
            5 => Self::Void,
            _ => return None,
        })
    }
}

impl SpellFilter {
    /// The player module's spell filters — the fourteen bits the filter tests.
    ///
    /// Bits `0`…`3` are the four elemental schools in [`School`] order and bit `13` is Void; bits
    /// `4`…`12` are levels 1…9. That is exactly [`Self::schools`] and [`Self::levels`], except
    /// that Void's bit sits at 13 rather than at 4 — the same "added later" discontinuity the
    /// button ids have.
    #[must_use]
    pub const fn from_player_module(filters: u32) -> Self {
        let elemental = (filters & 0b1111) as u8;
        let void = if filters & 0x2000 != 0 {
            1 << (School::Void as u8)
        } else {
            0
        };
        Self {
            schools: elemental | void,
            levels: ((filters >> 4) & ((1 << MAX_LEVEL_BIT) - 1)) as u16,
        }
    }

    /// The inverse, for a test and for a future write-back.
    #[must_use]
    pub const fn to_player_module(self) -> u32 {
        let elemental = (self.schools & 0b1111) as u32;
        let void = if self.schools & (1 << (School::Void as u8)) != 0 {
            0x2000
        } else {
            0
        };
        elemental | void | ((self.levels as u32) << 4)
    }
}

/// The **first** decrypted formula slot.
///
/// Defined in [`dereth_client_contract::spellbook`], beside `DEFAULT_SPELL_FILTERS`,
/// because `dereth_client::hud` is what reads `raw_comps[0]` and the key.
pub use dereth_client_contract::spellbook::power_component;

/// `SpellbookPanel` — the live spell list.
///
/// The update from the player description is the whole of it: flush the spell list; stop if
/// the player has no spellbook; otherwise add each spellbook entry (looking up its table entry,
/// skipping it if filtered out, inserting it at its sorted place); then reset the filter
/// buttons.
#[derive(Debug, Default)]
pub struct SpellbookPanel {
    /// The spell list.
    pub list: Option<ItemListWidget>,
    /// The spell ids currently on show, in list order.
    pub shown: Vec<u32>,
    /// The selected spell id, updated when a spell row is selected.
    pub selected_spell: u32,
    /// The last `(filters, spellbook)` the list was built for, so an unchanged frame rebuilds
    /// nothing.
    ///
    /// **The book is the whole `Vec<SpellEntry>`, not the id list.**
    ///
    /// An id list would be a **projection**: the rebuild below reads
    /// every field of a [`SpellEntry`] (`display_order` orders the list, `level` and `bitfield`
    /// pick the surfaces the icon blits, `name` and
    /// `icon` are written into the row), and the guard would see only the ids. That would be
    /// **safe**, but only transitively: on the production host `Hud::build_spells` derives every non-`id` field
    /// from the static spell table in `client_portal.dat`, which cannot change during a
    /// session, so the id list is a superset. That is containment through a **producer**, one
    /// accessor rewrite from breaking in silence. Comparing the whole value makes it containment by construction, and costs
    /// nothing on a still frame: the compare is a slice compare against `view.spellbook()` and the
    /// clone happens only on a frame that rebuilds.
    ///
    /// It is the whole value rather than a wider tuple because a tuple carrying
    /// `display_order` and `level` would still have omitted `name`, `icon` and `bitfield`, and
    /// would have **looked exactly as fixed**.
    last: Option<(u32, Vec<SpellEntry>)>,
    /// The add-spell-shortcut notice's spell id, waiting to be delivered.
    ///
    /// The client's `p1 == 10` arm raises the notice and the spell bar's receives it — two
    /// elements in the client and two plain structs here, so the notice is recorded and
    /// [`super::remaining::RemainingPanels`] hands it over in the same call. Same hop, and for the
    /// same borrow reason, as the screen's own `panel_messages`.
    add_shortcut_notice: Option<u32>,
    /// The dialog-factory contexts [`Self::delete_spell`] has open, each with the spell it
    /// stashed under [`DELETE_SPELL_PROPERTY`].
    ///
    /// A `Vec` because the delete has **no "one at a time" guard** — it builds a fresh
    /// `PropertyCollection` on every press and hands it to the dialog factory as a callback
    /// dialog, and it is the factory's queue that makes the second one wait behind the first.
    delete_dialogs: Vec<(u64, u32)>,
}

/// State 6 — a filter button that is *on*, as the client tests it.
///
/// Every one of the thirteen declares exactly the states `0x1, 0x3, 0x6, 0x8, 0xD` and ships in
/// `0x1`. [verified against the live tree]
pub const BUTTON_ON: u32 = 6;
/// The state a filter button is in when its bit is clear.
pub const BUTTON_OFF: u32 = 1;

/// The client's if-chain, as a table: `(element id, player-module filter bit)`.
///
/// This is the **third** independent statement of the same bit map — [`SpellFilter::
/// from_player_module`] has it, the "is filtered out" test has it, and the filter update has it — and
/// all three agree, including Void's discontinuity at `0x2000`. A test below asserts that this
/// table and `SpellFilter` cannot drift apart.
pub const FILTER_BUTTONS: [(u32, u32); 13] = [
    (0x1000_0298, 0x0001), // Creature
    (0x1000_0299, 0x0002), // Item
    (0x1000_029A, 0x0004), // Life
    (0x1000_029B, 0x0008), // War
    (0x1000_05C0, 0x2000), // Void
    (0x1000_029C, 0x0010), // level 1
    (0x1000_029D, 0x0020),
    (0x1000_029E, 0x0040),
    (0x1000_029F, 0x0080),
    (0x1000_02A0, 0x0100),
    (0x1000_02A1, 0x0200),
    (0x1000_02A2, 0x0400),
    (0x1000_054E, 0x0800), // level 8
];

/// The client's `0x100002A5` arm — the DELETE button.
pub const DELETE_BUTTON: ElementId = ElementId(0x1000_02A5);

/// The property the delete stashes the selected spell id in and the dialog callback reads it
/// back out of.
///
/// It is the dialog's whole memory of *which* spell it is asking about: the callback is a free
/// function with no `this`, so the id has to travel in the `PropertyCollection`.
pub const DELETE_SPELL_PROPERTY: u32 = 0x1000_003F;

/// The client's prompt, `sprintf`'d against the selected spell's name.
///
/// Retail's exact text. The trailing punctuation is an exclamation mark, not a full stop.
pub const DELETE_PROMPT: &str = "Are you sure you want to remove %s from your spellbook? You will no longer be able to cast this spell unless you learn it again!";

/// The bit one filter button owns, or `None` for any other element.
#[must_use]
pub fn filter_bit(id: ElementId) -> Option<u32> {
    FILTER_BUTTONS
        .iter()
        .find(|(e, _)| *e == id.0)
        .map(|(_, b)| *b)
}

impl SpellbookPanel {
    /// The spellbook panel's post-init's one load-bearing line — binding the spell list
    /// `0x10000295` — plus the item-list initialisation, which is what creates the slot
    /// elements. `page` is the spell page `0x10000190`.
    pub fn post_init(&mut self, ui: &mut UiSystem, page: ElemHandle) {
        let panel = ui.get_child_recursive(page, PANEL).unwrap_or(page);
        self.list = ui
            .get_child_recursive(panel, SPELL_LIST)
            .map(|h| ItemListWidget::init(ui, h));
        self.last = None;
        self.shown.clear();
        self.selected_spell = 0;
        self.add_shortcut_notice = None;
        // The tree these contexts' elements hung off has gone; the factory is reset with it.
        self.delete_dialogs.clear();
    }

    // ---- the double-click notice and the DELETE button --------------------------------------

    /// Take the pending add-spell-shortcut notice, if a double-click raised one. The receiver is
    /// [`super::spellcasting::SpellcastingPanel::on_add_spell_shortcut`].
    pub fn take_add_shortcut_notice(&mut self) -> Option<u32> {
        self.add_shortcut_notice.take()
    }

    /// The dialog-factory contexts this panel has open, and the spell each is asking about —
    /// `(context, spell id)`, in the order the delete raised them.
    #[must_use]
    pub fn delete_dialogs(&self) -> &[(u64, u32)] {
        &self.delete_dialogs
    }

    /// The spellbook panel's delete spell — **which removes nothing**.
    ///
    /// With no selected spell, or no magic system, it refuses. Otherwise it formats
    /// [`DELETE_PROMPT`] with the spell's name and builds a property collection: `0x8E` = 1
    /// (`DialogKind::Confirmation`), `0xC5` = the text, `0x1000003F` = the selected spell id. It
    /// raises that as a callback dialog and answers true.
    ///
    /// The whole function is the *question*. The dialog callback is the answer,
    /// and it sends the remove-spell request (`0x01A8`) and nothing else — no local removal,
    /// no list edit. The row goes when the shard echoes the removal back and
    /// the spell-removed notice handler rebuilds.
    ///
    /// **The dialog is reproduced rather than short-circuited to the request.** Skipping it would
    /// make a permanent, unrecoverable loss a single misclick, which is exactly what the client
    /// spends a whole function preventing. `panels::skills`'s "this build has no dialog" note is
    /// the opposite call on a different case: a spend, not a destruction.
    ///
    /// Returns true when a dialog was raised.
    pub fn delete_spell(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let spell = self.selected_spell;
        if spell == 0 {
            return false;
        }
        // The magic system's spell-name read. A spell the table cannot name takes the
        // `if (!magic) return false` shape here: the client would have formatted an empty string,
        // this refuses, because a prompt that names no spell is worse than no prompt at all.
        let Some(name) = view
            .spellbook()
            .iter()
            .find(|e| e.id == spell)
            .map(|e| e.name.clone())
        else {
            return false;
        };
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_assets::ui::PropertyValue::Integer(
                dereth_ui::dialog::DialogKind::Confirmation.property(),
            ),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_assets::ui::PropertyValue::String(DELETE_PROMPT.replacen("%s", &name, 1)),
        );
        // The id the callback reads back — the selected spell id, stored directly.
        data.set(
            DELETE_SPELL_PROPERTY,
            dereth_assets::ui::PropertyValue::InstanceId(spell),
        );
        // `make_dialog` plus "this context owes a
        // callback", which is what [`Self::service_delete_dialog`] then honours.
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        ui.dialogs.note_callback(context);
        self.delete_dialogs.push((context, spell));
        self.service_delete_dialog(ui);
        true
    }

    /// The dialog factory's create-dialog element half plus
    /// the spellbook's delete-spell dialog callback, run once a frame for every context
    /// [`Self::delete_spell`] opened.
    ///
    /// The callback returns without sending when the answer property `0x92` is absent, when it
    /// is "No", or when the spell property `0x1000003F` is absent or zero; otherwise it sends the
    /// remove-spell request (`0x01A8`) — the whole of it.
    ///
    /// The `0x92` test is presence **then** value, in that order, which is why a dialog dismissed
    /// without an answer sends nothing rather than sending a "no".
    ///
    /// Returns how many contexts sent their removal.
    pub fn service_delete_dialog(&mut self, ui: &mut UiSystem) -> usize {
        // The element half: whatever the factory has made current and not yet given an element.
        let owed: Vec<u64> = ui
            .dialogs
            .pending_create()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        for (context, _) in self.delete_dialogs.clone() {
            if !owed.contains(&context) {
                continue;
            }
            let Some(info) = ui.dialogs.info(context) else {
                continue;
            };
            let (data, kind) = (info.data.clone(), info.kind);
            let Ok(root) = ui.require_env().and_then(|e| {
                e.create_and_add_root_element(ui, DIALOG_LAYOUT, kind.root_element_id())
            }) else {
                continue;
            };
            // The dialog's text update: property `0xC5` goes on child `0x3E`.
            if let Some(dereth_assets::ui::PropertyValue::String(text)) =
                data.get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
            {
                if let Some(h) = ui.get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
                {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(text);
                    }
                }
            }
            dereth_ui::dialog::types::set_dialog_data(ui, root, &data);
            dereth_ui::dialog::base::update_popup_size_and_position(ui, root);
            ui.bind_dialog_element(context, root);
        }
        // The callback half.
        let mut done = 0;
        for (context, spell) in self.delete_dialogs.clone() {
            let Some(info) = ui.dialogs.info(context) else {
                // The factory dropped it ( or the framework switched screens):
                // the callback is not owed and nothing is sent.
                self.delete_dialogs.retain(|(c, _)| *c != context);
                continue;
            };
            let Some(root) = info.element else { continue };
            let answer = dereth_ui::dialog::types::dialog_element(ui, root)
                .and_then(|d| d.answer_property());
            let Some((key, value)) = answer else { continue };
            ui.dialogs.set_answer_property(context, key, value);
            let Some((queue, info)) = ui.dialogs.begin_close_dialog(context) else {
                continue;
            };
            // A "No" answer returns — the callback's only decision.
            if ui.dialogs.has_callback(context) && info.data.get_bool(0x92) == Some(true) {
                ui.requests
                    .emit(crate::view::UiRequest::RemoveSpell { spell_id: spell });
                done += 1;
            }
            ui.dialogs.take_callback(context);
            ui.send_notice(
                dereth_ui::NoticeId::DialogClosed,
                &dereth_ui::NoticePayload {
                    a: u32::try_from(context).unwrap_or(u32::MAX),
                    ..Default::default()
                },
            );
            ui.remove_and_delete_root(root);
            ui.dialogs.finish_close_dialog(queue, info, ui.now.0);
            ui.dialogs.completed.retain(|i| i.context != context);
            self.delete_dialogs.retain(|(c, _)| *c != context);
        }
        done
    }

    /// How many `UiItemWidget` slots the list has, which is zero when nothing was created.
    #[must_use]
    pub fn slots_created(&self) -> u32 {
        self.list.as_ref().map_or(0, |w| w.created)
    }

    /// The spellbook's update from the player description, run against the current world.
    ///
    /// Returns true on a frame that actually rewrote the list.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        self.update_filtered(ui, view, view.spell_filters())
    }

    fn update_filtered(&mut self, ui: &mut UiSystem, view: &dyn GameView, filters: u32) -> bool {
        // **Before** the snapshot guard. The dialog creation's element half and
        // the callback are not part of the list rebuild and must
        // not be skipped on a frame where the book has not changed — which is *every* frame the
        // dialog is up, since nothing has changed yet.
        self.service_delete_dialog(ui);
        let book = view.spellbook();
        // The whole book, field for field, not the id list — see [`Self::last`].
        // No allocation on a frame that does not rebuild: this is a slice compare, and the
        // `to_vec` at the tail runs only past the early-out.
        if self
            .last
            .as_ref()
            .is_some_and(|(f, b)| *f == filters && b.as_slice() == book)
        {
            return false;
        }
        let filter = SpellFilter::from_player_module(filters);
        let rows = Self::sorted(filter, book);
        self.shown = rows.iter().map(|s| s.id).collect();
        if let Some(w) = self.list.as_mut() {
            // The sorted rows go through whole rather than projected to `(id, icon, name)`, because
            // spell-icon composition needs the power level and spell bitfield to resolve
            // the `UISpellBackgrounds` / `UISpellOverlays` surfaces used in the icon.
            w.set_spells(ui, &rows);
        }
        // The client's last line. Without it the thirteen
        // buttons stay in their authored state and a panel that honours the capture's filters
        // shows every button lit, which is the "looks right, is wrong" case exactly.
        self.reset_filter_buttons(ui, filters);
        self.last = Some((filters, book.to_vec()));
        true
    }

    /// The client's filter plus the client's order, applied
    /// to the whole book at once.
    ///
    /// The client inserts one spell at a time and its sorted-insertion lookup scans the list it
    /// has built so far for the first row whose display order is on the far side of the new
    /// spell's — an insertion sort by display order. Doing it as one stable sort gives the same
    /// list, and two spells sharing a display order keep spellbook order either way.
    ///
    /// **The direction is inferred, not read.** Which side of the comparison is the new spell has
    /// not been established. Ascending display order is what puts the shipped
    /// table's own school-then-level grouping on screen in the order retail shows it. It remains
    /// an open question.
    #[must_use]
    pub fn sorted(filter: SpellFilter, book: &[SpellEntry]) -> Vec<SpellEntry> {
        let mut rows: Vec<SpellEntry> = book
            .iter()
            .filter(|s| !Self::is_filtered_out(filter, s))
            .cloned()
            .collect();
        rows.sort_by_key(|s| s.display_order);
        rows
    }

    // ---- the thirteen filter buttons -------------------------------------------------------

    /// The spellbook panel's element-message handler, in full:
    ///
    /// * **`0x1C` inside the spell list.** Take the UI-item row (type `0x10000032`) under the
    ///   mouse. On `p1 == 10` (double-click) raise the add-spell-shortcut notice for its spell and
    ///   stop — it never selects. On `p1` 7 or 8 select its spell.
    /// * **Message `1`.** A filter button (`0x10000298..=0x100002A2`, `0x1000054E`, `0x100005C0`)
    ///   updates the filter, rebuilds the list against the new mask, and scrolls to the top when
    ///   the list has rows. `0x100002A5` deletes the selected spell.
    ///
    /// Without the `10` and `0x100002A5` arms a double-click on a spell and a press of DELETE
    /// would both do nothing at all.
    ///
    /// Note that the double-click arm `break`s: a double-click puts the spell on the bar and does
    /// **not** move the spellbook's own selection, which is why DELETE after a double-click still
    /// asks about whatever was single-clicked last.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        // The element-message handler: only descendants of this spell list, and only
        // primary/secondary presses. p2 is not a row index: the item-under-mouse lookup uses the
        // pointer.
        if m.id == dereth_ui::msg::element::id::MOUSE_PRESS && matches!(m.p1, 7 | 8 | 10) {
            let Some(w) = self.list.as_ref() else {
                return false;
            };
            if !ui.is_ancestor_of(w.handle, m.source) {
                return false;
            }
            let (ox, oy) = ui.screen_origin(w.handle);
            let (x, y) = m.point.window;
            let Some(index) = w.item_index_at_point(ui, x - ox, y - oy) else {
                return false;
            };
            let Some(spell) = w
                .slots
                .get(index)
                .and_then(|s| s.spell)
                .filter(|id| *id != 0)
            else {
                return false;
            };
            // The magic system's add-spell-shortcut notice — a *local* notice,
            // not a message: nothing leaves the client here. The bar's receiver is what sends
            // `0x01E3`, and it appends to whichever tab is open.
            if m.p1 == 10 {
                self.add_shortcut_notice = Some(spell);
                return true;
            }
            // **Examining a spell is not this handler's arm.**
            //
            // This handler treats `7` and `8` alike and only selects. What examines a spell is the
            // **list element's own** handler, one level down: on `0x1C` with `p1 == 8` it takes the
            // UI-item row under the mouse and examines its object if it has an item id, or its
            // spell if it has a spell id.
            //
            // Two listeners, both reached by the same press, so a right-click **both** selects the
            // spell and opens the description — which is why the select above is not an `else`.
            // The spell list is an `ItemListWidget` (`0x10000295`, cast to `0x10000031` by
            // its post-init), so its rows carry a spell id and never an item id, and the
            // first branch of that pair can never be taken here.
            if m.p1 == 8 {
                ui.notice_inbox.emit_examine_spell(spell);
            }
            self.set_selected(ui, spell);
            return true;
        }
        // The UI-item element's element-message handler's `0x21` arm — find the
        // nearest ancestor of type `0x10000031` and, if there is one, begin its drag at (x, y).
        //
        // This is what **picks a spell up**: `GamePlayScreen::begin_item_drag` offers
        // `begin_drag_from_rejected` the four inventory lists and no spell list, so without this a
        // press-and-drag on a spellbook row finds no owner and returns `None`.
        // The client has no such list: the slot walks up to
        // *its own* ancestor, whichever list that is.
        if m.id == dereth_ui::msg::element::id::DRAG_REJECTED {
            let (x, y) = m.point.window;
            let Some(w) = self.list.as_mut() else {
                return false;
            };
            // **The `0x21` must name a slot.** It is the UI-item slot's
            // listener's arm and walks *up* to the list; the item list's own element-message
            // handler has no `0x21` arm, so a message naming the list
            // starts no drag in retail, so this test does not also accept `w.handle == m.source`.
            if w.slot_of(m.source).is_none() {
                return false;
            }
            return w.begin_drag(ui, x, y).is_some();
        }
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return false;
        }
        // The `0x100002A5` arm deletes the selected spell. It is tested before the filter table because it is
        // the `else` of the same `if`, and `filter_bit` answers `None` for it either way.
        if m.source_id == DELETE_BUTTON {
            self.delete_spell(ui, view);
            return true;
        }
        let Some(bit) = filter_bit(m.source_id) else {
            return false;
        };
        self.update_filter(ui, view, m.source_id, bit);
        // The handler's scroll follows the rebuild, even when the filter mask is equal.
        if let Some(w) = self.list.as_mut() {
            w.scroll_to_show(ui, 0);
        }
        true
    }

    /// The spellbook's selection: clear all nonmatching rings and reveal each
    /// matching nonzero spell row. A zero or absent id clears rings without moving the viewport.
    pub fn set_selected(&mut self, ui: &mut UiSystem, spell: u32) {
        if let Some(w) = self.list.as_mut() {
            for index in 0..w.slots.len() {
                let matches = spell != 0 && w.slots[index].spell == Some(spell);
                w.slots[index].set_selected_state(ui, matches);
                if matches {
                    w.scroll_to_view(ui, index);
                }
            }
        }
        self.selected_spell = spell;
    }

    /// The spellbook panel's filter update: find the button (return if absent), and set its bit
    /// in the player module's spell filters when the button is in state 6, clear it otherwise. If
    /// the mask changed, store it and send the spellbook-filter request with it.
    ///
    /// The button has already toggled itself. Redraw against the new mask immediately;
    /// the request writes the shared player state at the runtime boundary.
    pub fn update_filter(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        button: ElementId,
        bit: u32,
    ) {
        let old = view.spell_filters();
        let on = ui
            .get_element(button)
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.state.0 == BUTTON_ON);
        let new = if on { old | bit } else { old & !bit };
        if new == old {
            return;
        }
        ui.requests
            .emit(crate::view::UiRequest::SetSpellbookFilter { mask: new });
        self.update_filtered(ui, view, new);
    }

    /// Thirteen state writes, each the button's own bit tested against the mask.
    ///
    /// The update from the player description ends with this call, which is why the buttons come up
    /// matching the capture's filters rather than all lit.
    pub fn reset_filter_buttons(&self, ui: &mut UiSystem, mask: u32) -> u32 {
        let mut n = 0;
        for (id, bit) in FILTER_BUTTONS {
            let Some(h) = ui.get_element(ElementId(id)) else {
                continue;
            };
            // The button element's state write's toggle arm — see
            // [`super::statmgmt::set_toggle_button_state`] for why the attribute and not the state.
            super::statmgmt::set_toggle_button_state(ui, h, mask & bit != 0);
            n += 1;
        }
        n
    }

    /// True when the spell is **not** shown.
    ///
    /// Two gates, both of which must pass: the school bit, and then the level bit for the rough
    /// spell-level heuristic's answer. A school outside 1…5 or a level outside 1…9 falls
    /// into the switch default, which returns true — so a spell the client cannot classify
    /// is hidden rather than shown.
    #[must_use]
    pub fn is_filtered_out(filter: SpellFilter, s: &SpellEntry) -> bool {
        let Some(school) = School::from_magic_school(s.school) else {
            return true;
        };
        !filter.accepts(SpellRow {
            school,
            level: s.level,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered toolbar and panel behavior's child table — five school buttons and eight level
    /// buttons, and the two ids that sit apart from their runs.
    #[test]
    fn the_filter_buttons_are_the_documented_element_ids() {
        assert_eq!(School::Creature.button(), ElementId(0x1000_0298));
        assert_eq!(School::War.button(), ElementId(0x1000_029B));
        assert_eq!(School::Void.button(), ElementId(0x1000_05C0));
        for l in 1..=7 {
            assert_eq!(level_button(l), Some(ElementId(0x1000_029B + l)));
        }
        assert_eq!(level_button(8), Some(ElementId(0x1000_054E)));
        assert_eq!(level_button(9), None, "the layout provides eight");
        assert_eq!(LEVEL_BUTTON_COUNT, 8);
        // The catalogue's binding table must agree with this module.
        let spec = crate::panels::catalogue::spec("SpellbookPanel").unwrap();
        for s in School::ALL {
            assert!(spec.children.iter().any(|c| c.id == s.button()), "{s:?}");
        }
        for l in 1..=LEVEL_BUTTON_COUNT {
            assert!(
                spec.children
                    .iter()
                    .any(|c| c.id == level_button(l).unwrap()),
                "level {l}"
            );
        }
    }

    /// Oracle: §3.3 — "Five school filters × eight level filters; the list shows the
    /// **intersection**."
    #[test]
    fn the_filter_is_the_intersection_of_the_two_button_rows() {
        let book = [
            SpellRow {
                school: School::War,
                level: 1,
            },
            SpellRow {
                school: School::War,
                level: 7,
            },
            SpellRow {
                school: School::Life,
                level: 1,
            },
            SpellRow {
                school: School::Void,
                level: 8,
            },
        ];
        let mut f = SpellFilter::all();
        assert_eq!(f.visible(&book).len(), 4);

        // Turning off a school removes only that school's spells, at every level.
        f.toggle_school(School::War);
        assert_eq!(
            f.visible(&book),
            vec![
                SpellRow {
                    school: School::Life,
                    level: 1
                },
                SpellRow {
                    school: School::Void,
                    level: 8
                },
            ]
        );

        // Turning off a level removes it across every school.
        f.toggle_school(School::War);
        f.toggle_level(1);
        assert_eq!(
            f.visible(&book),
            vec![
                SpellRow {
                    school: School::War,
                    level: 7
                },
                SpellRow {
                    school: School::Void,
                    level: 8
                },
            ]
        );

        // With nothing selected the list is empty, not "everything".
        let empty = SpellFilter::default();
        assert!(empty.visible(&book).is_empty());
    }

    /// Oracle: the client's if-chain, read button by button —
    /// Creature `0x1`, Item `0x2`, Life `0x4`, War `0x8`, Void `0x2000`, levels 1…8
    /// `0x10`…`0x800`.
    ///
    /// This is the point of the test: the same bit map is stated **three** times in the client
    /// (the filter update, the is-filtered-out test and the player-module unpack) and twice in this
    /// crate ([`FILTER_BUTTONS`] and [`SpellFilter::to_player_module`]). Toggling a button and
    /// round-tripping through `SpellFilter` must land on exactly the bit the table names, or the
    /// two copies have drifted and the panel would filter on one map while the wire carried the
    /// other — which is invisible until a Void spell disappears.
    #[test]
    fn every_filter_buttons_bit_is_the_one_spell_filter_round_trips_to() {
        assert_eq!(FILTER_BUTTONS.len(), 13, "five schools and eight levels");
        for s in School::ALL {
            let bit = filter_bit(s.button()).unwrap_or_else(|| panic!("{s:?} has no bit"));
            let mut f = SpellFilter::default();
            f.toggle_school(s);
            assert_eq!(f.to_player_module(), bit, "{s:?}");
        }
        for l in 1..=LEVEL_BUTTON_COUNT {
            let b = level_button(l).expect("eight level buttons");
            let bit = filter_bit(b).unwrap_or_else(|| panic!("level {l} has no bit"));
            let mut f = SpellFilter::default();
            f.toggle_level(l);
            assert_eq!(f.to_player_module(), bit, "level {l}");
        }
        // Void's discontinuity, spelled out because it is the one that would be "fixed" by mistake.
        assert_eq!(filter_bit(School::Void.button()), Some(0x2000));
        assert_eq!(filter_bit(level_button(8).unwrap()), Some(0x800));
        // Every button appears once and no two share a bit.
        let mut bits: Vec<u32> = FILTER_BUTTONS.iter().map(|(_, b)| *b).collect();
        bits.sort_unstable();
        bits.dedup();
        assert_eq!(bits.len(), 13);
        // And the union is the default mask, which is what the player-module unpack falls back to.
        assert_eq!(
            FILTER_BUTTONS.iter().fold(0u32, |a, (_, b)| a | b),
            DEFAULT_SPELL_FILTERS & !(1 << 12),
            "every bit except the ninth level, which has no button"
        );
        // An element that is not a filter button has no bit — the handler's own guard.
        assert_eq!(filter_bit(SPELL_LIST), None);
        assert_eq!(
            filter_bit(ElementId(0x1000_02A5)),
            None,
            "the delete button is not a filter"
        );
    }

    /// The ninth level bit is carried even though no button sets it.
    #[test]
    fn the_ninth_level_bit_is_carried_even_though_no_button_sets_it() {
        let l9 = SpellRow {
            school: School::Life,
            level: 9,
        };
        let mut f = SpellFilter::all();
        assert!(f.accepts(l9));
        f.toggle_level(9);
        assert!(!f.accepts(l9));
        // A level outside 1..=9 is never accepted.
        assert!(!SpellFilter::all().accepts(SpellRow {
            school: School::Life,
            level: 10
        }));
        assert!(!SpellFilter::all().accepts(SpellRow {
            school: School::Life,
            level: 0
        }));
    }
}
