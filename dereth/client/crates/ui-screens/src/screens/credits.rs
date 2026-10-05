//! `CreditsScreen` — mode `0x10000005`, the scrolling credits.
//!
//! The shipped `credits` layout (`0x21000003`, 816 bytes, consumed exactly) has five roots and is
//! entirely data-driven:
//!
//! | root | what it is | authored |
//! |---|---|---|
//! | `0x10000410` | the text field, normal | (400, 0) 400×600, z 299; text area `0x10000411`, table `0x23000008`, speed **20.0** |
//! | `0x10000412` | the text field, Ctrl+Alt+Shift | the same with table `0x23000009` and speed **22.0** |
//! | `0x10000413` | the picture field, normal | (0, 0) 400×600, z 300; seven pictures `0x06005F14`…`0x06005F1A` |
//! | `0x10000414` | the picture field, Ctrl+Alt+Shift | the same seven |
//! | `0x10000415` | the picture template | (0, 0) 400×300, z 0 |
//!
//! The z level sorts **descending** (higher is further back), so the picture field at 300 is behind
//! the text field at 299 — which is why the credits read over the artwork rather than under it.
//!
//! # The please-wait, and which wire the action callback arrives on
//!
//! * **The dialog.** The please-wait is raised through `DialogController`, as `charmgmt.rs` and
//!   `chargen.rs` raise theirs; without it the roll ends on a frozen credit scroll instead of a
//!   wait box.
//! * **The producer.** In retail the constructor ends by registering input map 9 at priority
//!   3000, so **every control input map 9 carries reaches this screen's action callback before
//!   shared element input handling ever sees it**: the input manager calls the
//!   winning map's callback first. Only when that callback returns false does the input manager
//!   continue to the registered handler list.
//!
//! # Why the action arrives through map 9 and not global message 1
//!
//! Listening for the client's **global message 1** instead is not equivalent. Message 1 is
//! broadcast for **any** action the focused or active element declined, **from any input map**,
//! and the credits screen has no focusable element at all — so **every bound key in the build
//! would end the roll**: `W` (`MovementForward`, map 4), `F5` (`ToggleSpellbookPanel`,
//! `UICommands`), a quickbar digit. Retail ends the roll for what map 9 carries and nothing else,
//! because `CreditsScreen` is the callback of map 9 alone.

use dereth_primitives::DataId;
use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::{ElemHandle, ElementId, ListenerId, StateId, UiError, UiMode, UiSystem};

/// Both roots come from **layout enum `0x10000004`**; which element ids depends on whether
/// Ctrl+Alt+Shift are all held at construction time (an easter-egg alternate credit roll).
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0004);
/// The framework's own listener identity.
const ME: ListenerId = ListenerId::External(LAYOUT.0);

/// `(picture field, text field)` for the normal roll.
pub const NORMAL_ROOTS: (ElementId, ElementId) = (ElementId(0x1000_0413), ElementId(0x1000_0410));
/// `(picture field, text field)` for the Ctrl+Alt+Shift roll.
pub const EASTER_EGG_ROOTS: (ElementId, ElementId) =
    (ElementId(0x1000_0414), ElementId(0x1000_0412));

/// The element id the picture build creates each picture with, and the state it puts
/// it in.
pub const PICTURE_ELEMENT: ElementId = ElementId(0x1000_0415);
/// See [`PICTURE_ELEMENT`].
pub const PICTURE_STATE: u32 = 3;

/// The input map for which the original credits screen registers its action callback, and
/// which its action handler therefore catches **every** control of. `charmgmt.rs` has the same
/// constant, so both rows of `dereth_client_shell::ui::PREGAME_MODE_INPUT_MAPS` name the screen's own
/// map.
///
/// Retail registers map 9 at the focused-UI priority 3000. — the same registration the
/// original character-management and intro screens make.
pub const INPUT_MAP: u32 = 9;

/// The attribute naming the text-area child inside the text field.
///
/// The text area could be read as "the text field's first descendant with id 0", but the shipped
/// layout carries the child's id explicitly in attribute
/// `0x10000002` — `0x10000411` on both text fields — and that child is the field's only one, so the
/// two readings agree on this data. The attribute is used, with a recursive lookup of id 0 and
/// then "the only child" as fallbacks, because a description is a stronger source than an ordinal.
pub const ATTR_TEXT_AREA: u32 = 0x1000_0002;
/// The attribute the text field's string table comes from, read as a data id.
pub const ATTR_STRING_TABLE: u32 = 0x1000_0003;
/// The attribute the scroll speed comes from; **1.0 when absent**.
pub const ATTR_SPEED: u32 = 0x1000_0004;
/// The attribute holding the picture list: an `Array` whose members are `0x10000008` `DataFile`s.
///
/// This is not `0x8E`, which is the dialog kind property; the picture list is
/// `0x10000005` on the picture field, seven `0x06xxxxxx` surfaces in the shipped layout.
pub const ATTR_PICTURES: u32 = 0x1000_0005;
/// The property name each member of [`ATTR_PICTURES`] carries.
pub const ATTR_PICTURES_MEMBER: u32 = 0x1000_0008;
/// The default scroll speed.
pub const DEFAULT_SPEED: f32 = 1.0;

/// The `Dialog` layout enum, the same one `charmgmt.rs` builds its five from.
pub const DIALOG_LAYOUT: LayoutEnum = LayoutEnum(2);

/// The client's prompt token.
///
/// Retail hashes the literal "ID_Wait_PleaseWait" and uses string-table enum `0x10000001`.
///
/// **This is a different table and a different row from `CharacterManagementScreen`'s please-wait**,
/// which uses enum `0x10000002` and `ID_CharacterManagement_PleaseWait`. Two screens, two prompts.
pub const PLEASE_WAIT_STRING: &str = "ID_Wait_PleaseWait";

/// The string table enum the please-wait dialog build names, and the `StringTable` it resolves to —
/// `0x23000001`, the shipped `UI` table.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0001;
/// See [`STRING_TABLE_ENUM`].
pub const STRING_TABLE: DataId = DataId(0x2300_0001);

/// The original credit-row format.
///
/// It is `"ID_Credits%d"` (not `ID_Credits_%d`), and the counter starts at **1**. Confirmed against
/// the data: `str_hash("ID_Credits" + n)` for `n = 1…2345` reproduces **every one** of the 2,345
/// keys of table `0x23000008` and nothing else, and for `n = 1…1831` every one of the 1,831 keys of
/// the easter-egg table `0x23000009`.
pub const STRING_ID_FORMAT: &str = "ID_Credits%d";

/// `str_hash(sprintf("ID_Credits%d", n))`.
#[must_use]
pub fn credit_line_string_id(n: u32) -> u32 {
    dereth_primitives::num::hash::str_hash(format!("ID_Credits{n}").as_bytes())
}

/// The initialisation step 3:
///
/// ```text
/// duration = (fieldHeight + textHeight) / (((textHeight / lineCount) + fieldHeight) / speed)
/// ```
///
/// Evaluated rather than restated: a zero line count would divide by zero in the client, so the
/// caller must never pass one — the loop that produced it appended at least one line.
#[must_use]
pub fn scroll_duration(field_height: f32, text_height: f32, line_count: u32, speed: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let lines = line_count as f32;
    (field_height + text_height) / (((text_height / lines) + field_height) / speed)
}

/// The text area's Y for a given time.
///
/// ```text
/// t = 1 - (endTime - now) / duration, clamped above at 1 then below at 0
/// y = fieldHeight - truncate(t * (fieldHeight + scrollableHeight))
/// ```
///
/// Returns `(y, t)`; `t == 1.0` is the "unsubscribe from message 3, open a please-wait dialog and
/// queue mode `0x1000000A`" case.
///
/// The placement is not `round`: retail **truncates toward zero** — the same conversion
/// `dereth_primitives::num::to_i32` is, and the same one the whole engine uses. The half-pixel that
/// a `round` would add is observable: it moves the whole credit roll one pixel for half of every
/// scroll step.
#[must_use]
pub fn scroll_text(
    field_height: i32,
    scrollable_height: i32,
    end_time: f64,
    now: f64,
    duration: f32,
) -> (i32, f32) {
    #[allow(clippy::cast_possible_truncation)]
    let t = if duration == 0.0 {
        1.0
    } else {
        // The clamp order is the client's: clamp at 1 first, then at 0.
        let t = 1.0 - ((end_time - now) as f32 / duration);
        let t = if t >= 1.0 { 1.0 } else { t };
        if t <= 0.0 {
            0.0
        } else {
            t
        }
    };
    #[allow(clippy::cast_precision_loss)]
    let span = (field_height + scrollable_height) as f32;
    let y = field_height - dereth_primitives::num::to_i32(t * span);
    (y, t)
}

/// `CreditsScreen` — mode `0x10000005`.
#[derive(Debug, Default)]
pub struct CreditsScreen {
    roots: Vec<ElemHandle>,
    /// True when Ctrl+Alt+Shift were held at construction time.
    pub easter_egg: bool,
    /// The picture field.
    pub picture_field: Option<ElemHandle>,
    /// The text field.
    pub text_field: Option<ElemHandle>,
    /// The text area, the `TextElement` inside the text field.
    pub text_area: Option<ElemHandle>,
    /// The roll's duration.
    pub duration: f32,
    /// When the roll ends.
    pub end_time: f64,
    /// How many `ID_Credits<n>` rows the walk found before a lookup failed.
    pub line_count: u32,
    /// The picture list read off the picture field, rewound.
    pub pictures: Vec<DataId>,
    /// The last picture shown — the next picture id read cycles
    /// `last_picture = (last_picture + 1) % num_pictures`.
    pub last_picture: u32,
    /// The picture elements, in the client's own order — including the **swap-remove** the
    /// picture scroll performs when it retires the head.
    pub picture_elements: Vec<ElemHandle>,
    /// Set once the roll finished or an action short-circuited it.
    pub finished: bool,
    /// The please wait dialog build was reached.
    ///
    /// This is the record — the *"was this arm reached"* half — and [`Self::wait_context`] carries
    /// the dialog the arm raises.
    pub please_wait: bool,
    /// The `DialogController` context the please-wait dialog build stores and the credits
    /// screen closes during destruction.
    pub wait_context: Option<u64>,
    /// The element raised for [`Self::wait_context`], once the factory made that context current.
    pub wait_element: Option<ElemHandle>,
    /// Whether [`Self::initialize`] has run.
    initialized: bool,
}

impl CreditsScreen {
    /// The factory registered for this screen's mode.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self::default())
    }

    /// The number of pictures.
    #[must_use]
    pub fn num_pictures(&self) -> u32 {
        u32::try_from(self.pictures.len()).unwrap_or(0)
    }

    /// The two root element ids this instance will build.
    #[must_use]
    pub fn root_ids(&self) -> (ElementId, ElementId) {
        if self.easter_egg {
            EASTER_EGG_ROOTS
        } else {
            NORMAL_ROOTS
        }
    }

    /// The next picture id read.
    pub fn next_picture_id(&mut self) -> u32 {
        let n = self.num_pictures();
        if n == 0 {
            return 0;
        }
        self.last_picture = (self.last_picture + 1) % n;
        self.last_picture
    }

    /// **Any** input action takes the same "please wait dialog +
    /// mode `0x1000000A`" path the end of the roll takes.
    ///
    /// The original action callback first unregisters tick message 3, raises the please-wait
    /// dialog, queues mode `0x1000000A`, and reports the action consumed. Its input event is
    /// **read nowhere** — there is no start-edge test and no action comparison — so every action
    /// in map 9 skips the credits, and
    /// the release edge does it too. That is why this takes no action id.
    ///
    /// Retail reaches it through the input manager's "send action to listeners", which calls the
    /// action callback of the *winning* input map. The original credits setup registers map 9 at
    /// priority 3000, so this is reached from `UiShell::mode_on_action`, which *is* that dispatch's
    /// first-refusal step.
    ///
    /// It must not be driven from the client's **global message 1**: that is reached by any action
    /// the focused or active element declined, from **any** map, and the credits have no focusable
    /// element — so every bound key in the build would end the roll, where retail ends it only for
    /// what map 9 carries.
    ///
    /// The tick unregister is the action callback's **first** statement and belongs here rather
    /// than in the caller.
    pub fn on_action(&mut self, ui: &mut UiSystem) -> Option<UiMode> {
        // Unregister from global message 3, the tick the roll scrolls on.
        ui.unregister_for_global_message(dereth_ui::msg::global::TICK, ME);
        self.finish(ui)
    }

    /// The tail of the text scroll and the whole of the action callback: unregister from message 3, open the
    /// please-wait dialog, queue character management.
    fn finish(&mut self, ui: &mut UiSystem) -> Option<UiMode> {
        self.finished = true;
        self.make_please_wait_dialog(ui);
        Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
    }

    /// The credits screen's please wait dialog build.
    ///
    /// Two properties and no more: `0x8E` = 2 (`Wait`) and `0xC5`, the prompt.
    ///
    /// The property `0x8E` is 2. There are two differences from the
    /// character-management please-wait dialog:
    ///
    /// * **it sets no `0xAC`.** The character-management build sets `0xAC` to `1`; this one never mentions the
    ///   property, so the credits' wait dialog is **not modal**. A `Wait` has no buttons either
    ///   way, so what modality decides is only whether clicks reach the roll behind it.
    /// * **it has no existing-dialog-context guard.** The original path proceeds directly to
    ///   constructing the prompt. The guard here is [`Self::finished`], which the
    ///   two callers set — and which is why `update` stops scrolling.
    ///
    /// Returns whether a dialog was raised, so a caller can tell a host with no dat behind it from
    /// one that raised nothing.
    pub fn make_please_wait_dialog(&mut self, ui: &mut UiSystem) -> bool {
        self.please_wait = true;
        if self.wait_context.is_some() {
            return false;
        }
        let text = ui
            .resolve_string(
                STRING_TABLE,
                dereth_primitives::num::hash::str_hash(PLEASE_WAIT_STRING.as_bytes()),
            )
            .unwrap_or_else(|| PLEASE_WAIT_STRING.to_string());
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_ui::props::PropertyValue::Integer(
                dereth_ui::dialog::DialogKind::Wait.property(),
            ),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_ui::props::PropertyValue::String(text),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        self.wait_context = Some(context);
        self.service_dialog_queue(ui);
        true
    }

    /// The dialog queue service for this screen's one context, the same shape
    /// `charmgmt.rs` uses: build the root the kind names, copy `0xC5` onto child `0x3E`, size and
    /// centre it, and record it as one of the screen's roots so the mode switch deletes it.
    ///
    /// A `Wait` has **no answer children** (`DialogElement::answer_children` returns `(None, None)`
    /// for it), so unlike the character screen's five there is nothing to register a click on: the
    /// only way this dialog goes away is the mode switch, which is the client's behaviour and not
    /// an omission.
    fn service_dialog_queue(&mut self, ui: &mut UiSystem) {
        let Some(context) = self.wait_context else {
            return;
        };
        if self.wait_element.is_some() {
            return;
        }
        if !ui
            .dialogs
            .pending_create()
            .iter()
            .any(|(c, _)| *c == context)
        {
            return;
        }
        let Some(info) = ui.dialogs.info(context) else {
            return;
        };
        let data = info.data.clone();
        let root = info.kind.root_element_id();
        let Ok(h) = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, DIALOG_LAYOUT, root))
        else {
            return;
        };
        // The client's `0xAC` arm. This screen sets no `0xAC`, so this reads
        // `false` -- written through the same `get_bool(...).unwrap_or(false)` the character
        // screen uses so the two cannot drift on the default.
        ui.set_attribute_bool(
            h,
            dereth_ui::props::attr::DIALOG_MODAL,
            data.get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                .unwrap_or(false),
        );
        // The dialog's text update: `0xC5`'s string goes on child `0x3E`.
        if let Some(dereth_ui::props::PropertyValue::String(text)) = data
            .get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
            .cloned()
        {
            if let Some(t) = ui
                .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&text);
            }
        }
        dereth_ui::dialog::types::set_dialog_data(ui, h, &data);
        dereth_ui::dialog::base::update_popup_size_and_position(ui, h);
        self.roots.push(h);
        ui.bind_dialog_element(context, h);
        self.wait_element = Some(h);
    }

    /// The one original credits teardown operation beyond container cleanup closes the active
    /// please-wait dialog context.
    fn close_please_wait_dialog(&mut self, ui: &mut UiSystem) {
        let Some(context) = self.wait_context.take() else {
            return;
        };
        if let Some(h) = self.wait_element.take() {
            self.roots.retain(|r| *r != h);
            ui.remove_and_delete_root(h);
        }
        ui.dialogs.close_dialog(context, ui.now.0);
    }

    /// The credits screen's initialisation.
    ///
    /// Called from `create` once the roots exist. The credit lines are resolved through
    /// [`UiSystem::resolve_string`], which is the host's string service — a validity check
    /// against the dat cache in the client — so a host with no string tables gets a zero-line roll
    /// rather than a panic, and [`Self::line_count`] says so.
    pub fn initialize(&mut self, ui: &mut UiSystem, now: f64) {
        self.initialized = true;
        let (Some(field), Some(area)) = (self.text_field, self.text_area) else {
            return;
        };
        let Some(table) = crate::bind::attr_data_id(ui, field, ATTR_STRING_TABLE) else {
            return;
        };

        // "appends string ids 1, 2, 3, … from that table into the text area until
        // a lookup fails. The loop count becomes the line count."
        let mut n: u32 = 1;
        let mut text = String::new();
        while let Some(line) = ui.resolve_string(table, credit_line_string_id(n)) {
            text.push_str(&line);
            n += 1;
        }
        self.line_count = n - 1;
        if let Some(t) = ui.text_element_mut(area) {
            t.set_text(&text);
        }

        // Move to (0, field height), then resize to the field width, recalculate the glyph
        // list, and resize to the field width again.
        let fb = ui
            .node(field)
            .map_or((0, 0), |n| (n.region.box_.width(), n.region.box_.height()));
        ui.move_to(area, 0, fb.1);
        let text_height = Self::recalculate(ui, area, fb.0);

        // The duration and the end time.
        #[allow(clippy::cast_precision_loss)]
        let (fh, th) = (fb.1 as f32, text_height as f32);
        let speed = crate::bind::attr_float(ui, field, ATTR_SPEED).unwrap_or(DEFAULT_SPEED);
        self.duration = if self.line_count == 0 {
            0.0
        } else {
            scroll_duration(fh, th, self.line_count, speed)
        };
        self.end_time = now + f64::from(self.duration);

        // Step 4: the picture list, rewound.
        if let Some(pf) = self.picture_field {
            self.pictures = Self::read_picture_list(ui, pf);
            self.last_picture = 0;
        }
    }

    /// The glyph-list recalculation plus the resize around it: lay the glyphs out
    /// against the field width and give the area the height they need, which is what
    /// the text area's height then reads.
    ///
    /// Returns the scrollable height.
    fn recalculate(ui: &mut UiSystem, area: ElemHandle, width: i32) -> i32 {
        let height = match ui.text_element_mut(area) {
            Some(t) => {
                t.glyphs.recalculate(width);
                t.glyphs.extent().1
            }
            None => 0,
        };
        ui.resize_to(area, width, height);
        height
    }

    /// Step 4 of [`Self::initialize`]: the `Array` of picture `DataID`s.
    #[must_use]
    pub fn read_picture_list(ui: &UiSystem, picture_field: ElemHandle) -> Vec<DataId> {
        use dereth_assets::ui::PropertyValue;
        let Some(node) = ui.node(picture_field) else {
            return Vec::new();
        };
        let props = node.merged_properties();
        let Some(PropertyValue::Array(members)) = props.get(ATTR_PICTURES) else {
            return Vec::new();
        };
        members
            .iter()
            .filter_map(|m| match m.value {
                PropertyValue::DataFile(d) if d.0 != 0 => Some(d),
                _ => None,
            })
            .collect()
    }

    /// The update: scroll the text, then scroll the pictures by the text's delta.
    fn scroll(&mut self, ui: &mut UiSystem, now: f64) -> Option<UiMode> {
        let (Some(field), Some(area)) = (self.text_field, self.text_area) else {
            return None;
        };
        let fh = ui.node(field).map_or(0, |n| n.region.box_.height());
        let scrollable = Self::recalculate(
            ui,
            area,
            ui.node(field).map_or(0, |n| n.region.box_.width()),
        );
        let (y, t) = scroll_text(fh, scrollable, self.end_time, now, self.duration);
        // The client reads the *screen* y before the move and returns `oldScreenY - newY`.
        let old = ui.screen_box(area).y0;
        ui.move_to(area, 0, y);
        let delta = old - y;
        self.scroll_pictures(ui, delta);
        if t >= 1.0 {
            return self.finish(ui);
        }
        None
    }

    /// The scroll pictures.
    ///
    /// Two shipped details are reproduced rather than tidied: every picture is moved to
    /// `screenY0 - delta` — a **screen** coordinate written into a **parent-relative** one, which is
    /// only harmless because the picture field sits at x 0, y 0 — and retiring the head element
    /// **swaps the last element into index 0** rather than shifting, so the strip's order is
    /// scrambled after the first retirement.
    fn scroll_pictures(&mut self, ui: &mut UiSystem, delta: i32) {
        let Some(field) = self.picture_field else {
            return;
        };
        if self.picture_elements.is_empty() {
            self.create_and_add_picture(ui);
        } else {
            for h in self.picture_elements.clone() {
                let y = ui.screen_box(h).y0;
                ui.move_to(h, 0, y - delta);
            }
            let head = self.picture_elements[0];
            if ui.screen_box(head).y1 < 0 {
                let last = self.picture_elements.pop();
                if let Some(last) = last {
                    if !self.picture_elements.is_empty() {
                        self.picture_elements[0] = last;
                    }
                }
                ui.add_to_delete_queue(head);
            }
        }
        if let Some(last) = self.picture_elements.last().copied() {
            let field_height = ui.node(field).map_or(0, |n| n.region.box_.height());
            if ui.screen_box(last).y0 < field_height {
                self.create_and_add_picture(ui);
            }
        }
    }

    /// The create and add picture.
    ///
    /// The client builds a bare `ElementDesc` with element id `0x10000415`, state 3 and the picture
    /// field's own `LayoutDesc`. The shipped layout's own `0x10000415` root **is** that description
    /// Type 3, 400×300, no properties and no states — so it is created through the ordinary
    /// child-by-id path instead, which needs no synthetic description.
    ///
    /// The client then resizes the element to the picture's image size. That is not done here: the
    /// size lives in the `0x06xxxxxx` surface header and decoding one is the asset layer's work,
    /// not this crate's. The template's authored 400×300 is used, which is what every shipped
    /// credits picture is.
    fn create_and_add_picture(&mut self, ui: &mut UiSystem) {
        let Some(field) = self.picture_field else {
            return;
        };
        let index = self.next_picture_id();
        let Some(did) = self.pictures.get(index as usize).copied() else {
            return;
        };
        let Ok(h) = ui
            .require_env()
            .and_then(|e| e.create_child_element_by_enum(ui, field, LAYOUT, PICTURE_ELEMENT))
        else {
            return;
        };
        ui.set_state(h, StateId(PICTURE_STATE));
        let y = match self.picture_elements.last().copied() {
            Some(prev) => ui.screen_box(prev).y1,
            None => ui.node(field).map_or(0, |n| n.region.box_.height()),
        };
        ui.move_to(h, 0, y + 1);
        // The picture as the element's media image.
        if let Some(n) = ui.node_mut(h) {
            n.region.image = Some(dereth_ui::GraphicRef::opaque_surface(did, 0, 0));
        }
        self.picture_elements.push(h);
    }
}

impl Screen for CreditsScreen {
    /// No start-flag test and no action test: the body reads its event nowhere, so both edges of
    /// every control map 9 carries end the roll.
    fn on_mode_action(
        &mut self,
        cx: &mut ScreenCx<'_>,
        _e: &dereth_input::InputEvent,
    ) -> Option<dereth_ui::framework::ModeAction> {
        let queue = self.on_action(cx.ui);
        Some(dereth_ui::framework::ModeAction {
            consumed: true,
            handled: true,
            queue,
        })
    }

    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let (picture, text) = self.root_ids();
        // The picture field is created first; both come from the same layout enum.
        let pf = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, picture))?;
        self.roots.push(pf);
        self.picture_field = Some(pf);
        let tf = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, text))?;
        self.roots.push(tf);
        self.text_field = Some(tf);
        // The root-listener registration the original setup performs; the
        // credits have no buttons, but a screen that skips it hears nothing at all.
        ui.register_for_element_messages(pf, ME);
        ui.register_for_element_messages(tf, ME);
        // The text area: attribute `0x10000002`, then a recursive lookup of id 0, then the only child.
        self.text_area = crate::bind::attr_enum(ui, tf, ATTR_TEXT_AREA)
            .and_then(|id| ui.get_child_recursive(tf, ElementId(id)))
            .or_else(|| ui.get_child_recursive(tf, ElementId(0)))
            .or_else(|| ui.children(tf).first().copied());
        ui.register_for_global_message(dereth_ui::msg::global::TICK, ME);
        // No `KEY_DOWN_UNCONSUMED` registration here. The constructor's closing registration of
        // input map 9 at priority 3000 is made by `dereth_client_shell::ui::PREGAME_MODE_INPUT_MAPS` and
        // answered by `UiShell::mode_on_action`. The retail screen registers for exactly one global
        // message, 3; listening on 1 as well would be a deviation and a duplicate.
        Ok(())
    }

    fn destroy(&mut self, cx: &mut ScreenCx<'_>) {
        let ui = &mut *cx.ui;
        ui.unregister_for_global_message(dereth_ui::msg::global::TICK, ME);
        // The original credits teardown closes the active wait-dialog context. The mode switch is
        // what takes this dialog down -- a `Wait` has no buttons and the player cannot dismiss
        // it -- so the teardown has to close the *factory's* context too, not only delete the
        // element, or the queue keeps a context that will never drain.
        self.close_please_wait_dialog(ui);
        for h in std::mem::take(&mut self.picture_elements) {
            ui.add_to_delete_queue(h);
        }
        for r in std::mem::take(&mut self.roots) {
            ui.unregister_from_element(r, ME);
        }
    }

    /// Update — where [`Self::initialize`] is completed and the roll advances.
    ///
    /// The original client initializes during screen setup and advances scrolling on message 3.
    /// Initialization needs the current time
    /// for the end time and a `Screen::create` has no clock, so it is deferred by exactly one call to
    /// the first `update`, which is the same frame.
    fn update(
        &mut self,
        cx: &mut ScreenCx<'_>,
        now: dereth_primitives::LocalTime,
    ) -> Option<UiMode> {
        let ui = &mut *cx.ui;
        // The action callback is reached from `UiShell::mode_on_action`, which queues the return of
        // [`Self::on_action`] directly, as retail's credits action handler queues the mode itself;
        // no mode is carried over to this update.
        if self.finished {
            return None;
        }
        if !self.initialized {
            self.initialize(ui, now.0);
            return None;
        }
        self.scroll(ui, now.0)
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the screen catalogue's two-row root table and the shipped `credits` layout
    /// `0x21000003`, whose five roots are all four of these ids plus the picture template.
    #[test]
    fn the_two_credit_rolls_use_the_documented_element_pairs() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0004));
        let mut s = CreditsScreen::default();
        assert_eq!(
            s.root_ids(),
            (ElementId(0x1000_0413), ElementId(0x1000_0410))
        );
        s.easter_egg = true;
        assert_eq!(
            s.root_ids(),
            (ElementId(0x1000_0414), ElementId(0x1000_0412))
        );
        assert_eq!(PICTURE_ELEMENT, ElementId(0x1000_0415));
        assert_eq!(PICTURE_STATE, 3);
    }

    /// Oracle: the retail row format `ID_Credits%d` and the shipped credit tables. This pins
    /// the two starting hashes so a change to `str_hash` cannot pass unnoticed.
    #[test]
    fn the_credit_line_ids_are_the_hash_of_id_credits_n() {
        assert_eq!(STRING_ID_FORMAT, "ID_Credits%d");
        // The first row of both shipped tables.
        assert_eq!(credit_line_string_id(1), 0x08F8_A8C1);
        assert_eq!(credit_line_string_id(9), 0x08F8_A8C9);
        // …and the counter starts at 1, so "ID_Credits0" is not a row the client ever asks for.
        assert_ne!(
            credit_line_string_id(1),
            dereth_primitives::num::hash::str_hash(b"ID_Credits0")
        );
    }

    /// The scrolling formula is evaluated at fixed points.
    ///
    /// With field 600, text 3000, 100 lines and speed 1.0:
    /// `(600 + 3000) / (((3000/100) + 600) / 1) = 3600 / 630 = 5.714…`
    #[test]
    fn the_scroll_duration_formula_evaluates_to_the_documented_shape() {
        let d = scroll_duration(600.0, 3000.0, 100, DEFAULT_SPEED);
        assert!((d - 3600.0 / 630.0).abs() < 1e-4, "{d}");
        // `speed` divides the *denominator*, so doubling it **doubles** the duration — which reads
        // backwards for something called a speed, and is what the expression says:
        // `(fieldHeight + textHeight) / (((textHeight / lineCount) + fieldHeight) / speed)`.
        let fast = scroll_duration(600.0, 3000.0, 100, 2.0);
        assert!((fast - d * 2.0).abs() < 1e-4, "{fast} vs {d}");
        // The shipped normal roll: a 600-high field, speed 20, and 2,345 lines.
        let real = scroll_duration(600.0, 2345.0 * 16.0, 2345, 20.0);
        assert!(
            real > 1000.0,
            "the shipped roll is minutes long, not seconds: {real}"
        );
    }

    /// Oracle: the text starts at the field height and ends one full span
    /// above it, `t` clamps at both ends, and the conversion truncates.
    #[test]
    fn the_text_scrolls_from_the_field_height_to_one_span_above_it() {
        // At the start, `now` is `end_time - duration`, so t = 0 and y = fieldHeight.
        let (y, t) = scroll_text(400, 1000, 10.0, 4.0, 6.0);
        assert_eq!((y, t), (400, 0.0));
        // Half way.
        let (y, t) = scroll_text(400, 1000, 10.0, 7.0, 6.0);
        assert!((t - 0.5).abs() < 1e-6);
        assert_eq!(y, 400 - 700);
        // At and past the end, t clamps at 1 and y is fieldHeight - (field + scrollable).
        let (y, t) = scroll_text(400, 1000, 10.0, 10.0, 6.0);
        assert_eq!((y, t), (400 - 1400, 1.0));
        let (_, t) = scroll_text(400, 1000, 10.0, 99.0, 6.0);
        assert_eq!(t, 1.0);
        // …and before the start it clamps at 0 rather than going negative.
        let (y, t) = scroll_text(400, 1000, 10.0, 0.0, 6.0);
        assert_eq!((y, t), (400, 0.0));
        // The conversion truncates: a t that lands on 0.999 of one pixel does not round up.
        // t * span = 0.5 * 1401 = 700.5 -> 700, not 701.
        let (y, _) = scroll_text(400, 1001, 10.0, 7.0, 6.0);
        assert_eq!(y, 400 - 700);
    }

    /// Oracle: the next picture id read — `(last_picture + 1) % num_pictures`.
    #[test]
    fn the_picture_list_cycles() {
        let mut s = CreditsScreen {
            pictures: vec![DataId(1), DataId(2), DataId(3)],
            ..Default::default()
        };
        assert_eq!(s.next_picture_id(), 1);
        assert_eq!(s.next_picture_id(), 2);
        assert_eq!(s.next_picture_id(), 0);
        // A zero-length list must not divide by zero.
        let mut empty = CreditsScreen::default();
        assert_eq!(empty.next_picture_id(), 0);
    }

    /// Oracle: "any input action short-circuits to the same 'please wait
    /// dialog + mode `0x1000000A`' path".
    #[test]
    fn any_action_short_circuits_to_character_management() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CreditsScreen::default();
        assert_eq!(
            s.on_action(&mut ui),
            Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
        );
        assert!(s.finished);
        assert!(
            s.please_wait,
            "the please-wait dialog build is on the same path"
        );
        assert!(s.wait_context.is_some(), "dialog creation took the context");
    }

    /// Oracle: the two properties the retail build sets — and the one it does **not**: it never
    /// sets `0xAC`, where the character management screen's please wait dialog build does.
    ///
    /// The numbers are literals here on purpose (the stated testability rule: *"a test that reads a constant
    /// through the same symbol it writes it through cannot detect a wrong constant"*).
    #[test]
    fn the_wait_dialog_is_kind_two_with_a_prompt_and_no_modal_flag() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CreditsScreen::default();
        assert!(s.make_please_wait_dialog(&mut ui));
        let ctx = s.wait_context.expect("a context");
        let info = ui.dialogs.info(ctx).expect("the factory recorded it");
        assert_eq!(info.data.get_int(0x8E), Some(2), "0x8E = 2, the Wait kind");
        assert_eq!(info.kind, dereth_ui::dialog::DialogKind::Wait);
        assert_eq!(
            info.data.get_bool(0xAC),
            None,
            "the please-wait dialog build never mentions 0xAC"
        );
        assert_eq!(
            info.data.get(0xC5).cloned(),
            Some(dereth_ui::props::PropertyValue::String(
                PLEASE_WAIT_STRING.to_string()
            )),
            "with no string table installed the token itself is the prompt"
        );
        // The token and table, as literals.
        assert_eq!(PLEASE_WAIT_STRING, "ID_Wait_PleaseWait");
        assert_eq!(STRING_TABLE_ENUM, 0x1000_0001);
        assert_eq!(STRING_TABLE.0, 0x2300_0001);
        assert_eq!(DIALOG_LAYOUT.0, 2);
        // The client has no empty-context guard of its own, but it is only ever
        // reached once because both callers set `finished`; a second call here is refused so the
        // factory cannot accumulate contexts nothing will close.
        assert!(!s.make_please_wait_dialog(&mut ui), "one context, not two");
    }
}
