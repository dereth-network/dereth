//! `HousePanel` — the **House tab** of the toolbar's map page.
//!
//! The housing, toolbar, and panel behavior gives element type `0x10000025`; the panel
//! instance is `0x100001F7`, the second sub-panel of
//! `<MAPS>` `0x1000018C` — `MapPanel` `0x100001F6` is the first and is the tab that opens by
//! default. Both are pages of that element's own `Panel` tab table, so the House tab is
//! the *text* element `0x100001F4` and nothing else in the client names it.
//!
//! A `panels::catalogue` row alone is not a panel; this file is the constructor, without which
//! the House tab is blank.
//!
//! The house-data display runs seven sections and six of them read the panel's house data; that
//! is written by exactly one thing, the update-house-data notice, whose notice comes from the
//! client housing system's house-data receiver, i.e. from `0x0225`. The map's house pin is bound
//! and hidden separately (`screens::gameplay`, "3. The house icon").
//!
//! # The one string
//!
//! The buy-payment section: with no house data it shows `"You do not currently own a house."`;
//! otherwise `"The purchase price for this dwelling is:\n"` followed by the composed buy list. Either
//! way the text goes into one new row of the text box, built from its template list, with the
//! fit-to-text attribute (`0x29`) set, font 0 and the `Normal` colour.
//!
//! [`NO_HOUSE`] is the retail string exactly, **not** paraphrased — it is set directly in one of
//! two arms whose other arm is a concatenation. It is 33 characters, a full stop and no newline.
//! (Its neighbours are `"Rent:\n"`, `"Bought: "` and `"Maintenance is next due: "`, which are the
//! sections this file does not draw.)
//!
//! # How the line gets on screen here, and where retail differs
//!
//! **This is the one place this file is not retail, and it is stated rather than hidden.** In
//! retail the panel is *pushed*: sends
//! `0x021E House_QueryHouse` at login, the server answers a houseless character with
//! `0x0226 House_HouseStatus` (*"no house owned"*), the client turns it into a failed-house-transaction
//! notice, and the house panel's failed house transaction notice handler
//! drops the house data, flushes the list box and runs the
//! seven sections. So the box is already populated **before** the player ever opens the tab.
//!
//! Retail's trigger is a login round trip. What makes a stand-in safe is
//! that the round trip carries **no information** for a houseless character — `0x0226`'s whole
//! payload is one `u32` and its handler stores nothing (it is *a transaction
//! outcome, not state*). The answer is therefore a constant, and [`HousePanel::update`] writes
//! it from the frame loop when no answer has arrived. The retail entry points are all here and
//! named — [`HousePanel::update_house_data`] is the house-data update and
//! [`HousePanel::house_data_cleared`] is the house-data clear.
//!
//! # The `0x0226` receiver
//!
//! `Hud::ui_event`'s arm counts `0x0226` and
//! [`crate::view::GameView::house_status_notices`] carries the count here, so
//! [`HousePanel::update`]'s first arm calls [`HousePanel::house_data_cleared`].
//!
//! Two load-bearing facts:
//!
//! * **the word is discarded.** The house panel's handler takes the `u32` and never reads it;
//!   so do the other two listeners (one clears a field, `HousingPanel`'s re-queries the lord).
//!   It is a weenie error — 2, `BadParam`, in all three recorded arrivals — and it is kept only as
//!   `HudStats::house_status_last_notice`, a measurement.
//! * **the answer is a constant.** So the pane draws the same thing either way, and the only
//!   observable difference between the stand-in and the receiver is *when* and *how often* the
//!   pane is redrawn. `HousePanel::displays` is that observable, and a test asserts on it.
//!
//! One piece of retail's update handler is **not** reproduced and is named here rather than left to be
//! discovered: its first statement closes the panel's pending dialog. `HousePanel` has no dialog —
//! the confirmation belongs to `HousingPanel`, which has no module in this build — so there is
//! nothing to close. When that panel lands, its failed-house-transaction handler queries the lord
//! and this one grows its dialog half.
//!
//! # The other six sections, and `0x0225`
//!
//! `0x0225 House_HouseData` has a receiver
//! (`recv_house_data`, `Hud::ui_event`'s arm), the panel's house data is
//! [`crate::view::GameView::house_data`], and all seven sections of
//! the house-data display are written here in the client's own order.
//!
//! **The census — every line `HousePanel` can draw, and what selects it.** Eight rows, because
//! `display_rent_times` draws two:
//!
//! | # | section | literal | source | colour | guard |
//! |---|---|---|---|---|---|
//! | 1 | `display_buy_payment` | `You do not currently own a house.` | — | Normal | no house data |
//! | 1 | `display_buy_payment` | `The purchase price for this dwelling is:\n` + composed buy list | buy list | Normal | otherwise — **the row is unconditional** |
//! | 2 | `display_rent_payment` | `Rent:\n` + composed rent list | rent list | Normal | house data present |
//! | 3 | `display_buy_time` | `Bought: ` + [`convert_time`]`(buy time)` | buy time | Normal | ditto |
//! | 4 | `display_rent_times` | `This maintenance period ends: ` + [`convert_time`]`(period + rent time)` | rent time, house type | Normal | ditto |
//! | 5 | `display_rent_times` | `Maintenance is next due: ` + [`convert_time`]`((2×?)period + rent time)` | ditto, and maintenance-free / rent pick the ×2 | Normal | ditto |
//! | 6 | `display_location` | `Location: %.1f%s, %.1f%s` | house position | Normal | both lcoords `!= -1`, i.e. a valid non-apartment position |
//! | 7 | `display_warning_text` | *(none)* | house type | **RentNotPaid** | not maintenance-free and rent not paid in full |
//! | 7 | `display_warning_text` | `The maintenance has already been paid…` | — | **RentPaid** | otherwise |
//! | 8 | `display_purchase_time_text` | `You may buy another landscape house at ` + `strftime("%c")` + `. This restriction does not apply to apartments.` | int quality `0xC7` + `0x278D00` | Normal | a player description, and the wait has **not** expired |
//! | 8 | `display_purchase_time_text` | `You may buy another house immediately.` | — | Normal | expired, no house data |
//! | 8 | `display_purchase_time_text` | `You may buy another house immediately after you abandon this one.` | — | Normal | expired, house data present |
//!
//! So retail draws **1** line with no house and a player description, **2** with no house and no
//! player description is impossible (it draws 1 — section 1 only), and **8** for an owned landscape
//! house whose position is valid; an apartment drops row 6 and shows 7.
//!
//! Every one of the eight has an input the shard sends: `0x0225` carries all seven house-data
//! fields (the buy and rent lists, what is paid, the position, type, buy time, rent time and
//! maintenance-free flag), and the int property `HousePurchaseTimestamp` (199 = `0xC7`) is what
//! row 8 reads off the player. **No line is left out for want of data.**
//!
//! The three literals of row 8 and the order they concatenate in are the client's own:
//! `("You may buy another landscape house at " + <time>) + ". This restriction…"`.
//!
//! `strftime(buf, 0x400, "%c", localtime(&t))` is rendered by [`crate::ctime::strftime_c`].
//! The retail client calls `setlocale(LC_ALL, "English")`, so `%c` is
//! `English_United States.1252`'s `M/d/yyyy h:mm:ss tt` — `Bought: 9/14/2026 9:42:58 AM`, which
//! is what a retail screenshot shows (not `asctime`'s shape). The `localtime` zone shift arrives
//! on [`crate::view::HouseDataView::utc_offset_secs`] and
//! [`crate::view::HousePurchaseView::utc_offset_secs`]; without it all four dates would render
//! in UTC.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::listbox::ListBoxWidget;
use crate::view::{GameView, HouseDataView, HousePurchaseView};

/// `<MAPS>` — the map page of `PanelStack`'s stack, and the `Panel` that owns both tabs.
pub const PAGE: ElementId = ElementId(0x1000_018C);
/// The `HousePanel` sub-panel itself.
pub const PANEL: ElementId = ElementId(0x1000_01F7);
/// The **House tab** — a `TextElement`, the second entry of `<MAPS>`'s `0x2E` table.
///
/// Nothing in the client names it: the pairing comes out of the layout.
/// It is here so a test can assert against the tab the shipped data actually pairs with [`PANEL`]
/// rather than driving `on_element_message` behind the tab's back.
pub const TAB: ElementId = ElementId(0x1000_01F4);
/// The panel's text box — the one child it binds, and a
/// **`ListBox`**, not a text element: every line is a row built from its template list.
pub const TEXT_BOX: ElementId = ElementId(0x1000_01E6);

/// `UICore_Text_fit_to_text` — the attribute the panel sets on every row it
/// creates, so a row is exactly as tall as its own text.
pub const ATTR_FIT_TO_TEXT: u32 = 0x29;

/// The client's no-house-data arm. See the module header.
pub const NO_HOUSE: &str = "You do not currently own a house.";

/// The purchase-price display's **other** arm — 41 stored bytes including the
/// trailing newline the composed buy list's first item then follows.
pub const PURCHASE_PRICE: &str = "The purchase price for this dwelling is:\n";
/// The rent-payment prefix, 6 stored bytes.
pub const RENT: &str = "Rent:\n";
/// The buy-time line — 8 bytes, trailing space and no newline.
pub const BOUGHT: &str = "Bought: ";
/// The rent-times display, first row — 30 bytes.
pub const PERIOD_ENDS: &str = "This maintenance period ends: ";
/// The rent-times display, second row — 25 bytes.
pub const NEXT_DUE: &str = "Maintenance is next due: ";
/// The location display's `sprintf` format — 24 bytes.
///
/// See [`location_line`] for the four arguments. Their logical order differs from their stack
/// order.
pub const LOCATION_FORMAT: &str = "Location: %.1f%s, %.1f%s";
/// The house time conversion's zero arm.
pub const TIME_NA: &str = "N/A";
/// The maintenance-warning display's paid arm, 100 stored bytes.
pub const MAINTENANCE_ALREADY_PAID: &str = "The maintenance has already been paid for this \
                                            period. You may not prepay next period's maintenance.";
/// The purchase-time text, the not-yet-expired arm — 39 bytes.
pub const BUY_LANDSCAPE_AT: &str = "You may buy another landscape house at ";
/// …and its suffix — 48 bytes.
pub const APARTMENT_EXEMPTION: &str = ". This restriction does not apply to apartments.";
/// The expired-with-no-house arm, 38 stored bytes.
pub const BUY_IMMEDIATELY: &str = "You may buy another house immediately.";
/// The expired-with-a-house arm, 65 stored bytes.
pub const BUY_AFTER_ABANDON: &str =
    "You may buy another house immediately after you abandon this one.";

/// The client's constant, and the offset
/// the purchase-time section adds to int quality `0xC7` — thirty days.
///
/// Defined in [`dereth_client_contract::panels::house`], because `dereth_client_runtime::hud` is
/// what adds it.
pub use dereth_client_contract::panels::house::PURCHASE_WAIT_SECONDS;

/// `"N/A"` for zero, otherwise `strftime("%c")`.
///
/// Zero gives `"N/A"` without calling `localtime`; otherwise `localtime` runs, and a `NULL`
/// result leaves the string untouched, else `strftime(buf, 0x200, "%c", tm)` fills it.
///
/// It takes the zone offset and renders through [`crate::ctime::strftime_c`] — see the module
/// header.
///
/// The client's third arm matters too: MSVC's
/// `localtime` returns `NULL` for a negative `time_t`, and the client then builds no string,
/// so retail draws **nothing at all** — not `"N/A"`. A negative buy time is not a state the
/// shard produces, but retail has the arm and it costs two lines.
#[must_use]
pub fn convert_time(unix_time: i64, utc_offset_secs: crate::ctime::UtcOffsetSecs) -> String {
    if unix_time == 0 {
        TIME_NA.to_owned()
    } else if unix_time < 0 {
        String::new()
    } else {
        crate::ctime::strftime_c(unix_time, utc_offset_secs)
    }
}

/// The client's location formatting, once the house's landblock coordinates are known.
///
/// Each lcoord at or above `0x400` gets `E` (east-west) or `N` (north-south), otherwise `W` or
/// `S`; its value is `|(lcoord - 1024) * 0.1 + 0.5|`, and the result is
/// `sprintf(buf, fmt, ns_value, ns_letter, ew_value, ew_letter)` — latitude first, which is how
/// retail reads out coordinates. The absolute value is taken *after* the `+ 0.5`, so a
/// coordinate west of 1024 prints its magnitude with a `W`.
///
/// This is the same transform as [`crate::mapradar::map::house_marker_coords`] and is not that
/// function: the map pin keeps `f32` because it feeds `place_marker_on_map`'s integer arithmetic,
/// and this one stays in the `double`s `sprintf`'s varargs require, plus the `fabs` the pin has no
/// use for.
#[must_use]
pub fn location_line(lx: i32, ly: i32) -> String {
    #[allow(clippy::suboptimal_flops)] // LINT-OK: multiply then add, as retail; two roundings.
    let axis = |v: i32| (f64::from(v - 1024) * 0.1 + 0.5).abs();
    let ew = if lx >= 0x400 { "E" } else { "W" };
    let ns = if ly >= 0x400 { "N" } else { "S" };
    format!("Location: {:.1}{}, {:.1}{}", axis(ly), ns, axis(lx), ew)
}

/// `HousePanelTextColor` — the colour each row is given, passed through to the row text's
/// colour argument.
///
/// The three values are **indices into the row template's own font-colour array**, element
/// property `0x1B`, and the shipped `0x100001E6` template carries exactly three entries
/// [measured on the shipped layout]:
///
/// | index | variant | colour | drawn by |
/// |---|---|---|---|
/// | 0 | [`Self::Normal`] | `0xFFFFFFFF` white | every row but the warning |
/// | 1 | [`Self::RentPaid`] | **`0xFF00FF00` green** | the already-paid line |
/// | 2 | [`Self::RentNotPaid`] | `0xFFFFFF00` yellow | the rent warning |
///
/// The index must reach the element, not only [`HousePanel::lines`]: otherwise *"The maintenance
/// has already been paid for this period"* draws in the element's default white where retail
/// draws it green. `as u8` on this enum is the index the client passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HousePanelTextColor {
    #[default]
    Normal = 0,
    RentPaid = 1,
    RentNotPaid = 2,
}

/// `HousePanel`.
#[derive(Debug, Default)]
pub struct HousePanel {
    /// The `HousePanel` element. `None` when the layout has no such sub-panel.
    pub panel: Option<ElemHandle>,
    /// The text box.
    text_box: Option<ListBoxWidget>,
    /// What the rows read, top to bottom, with the colour each was given — the pane read back
    /// without walking the tree, and the denominator that separates "nothing was written" from
    /// "something was written and drew empty".
    pub lines: Vec<(String, HousePanelTextColor)>,
    /// How many times [`Self::display_house_data`] has run.
    pub displays: u32,
    /// Whether the panel holds house data.
    pub owns_house: bool,
    /// The `0x0226` count this pane has already drawn for.
    ///
    /// Retail has no such field: `HousePanel` is *pushed* by a notice and redraws inside the
    /// handler. This build pulls, so the edge has to be reconstructed, and it is reconstructed
    /// from a count rather than a flag for the reason every other pane here uses a count: two
    /// notices in one frame are two redraws in retail and must not collapse into one here.
    status_notices_drawn: u64,
    /// The `0x0225` count this pane has already drawn for — the same edge for the *other*
    /// update notice.
    data_notices_drawn: u64,
    /// The `HouseData` the rows above were drawn from.
    ///
    /// Not a retail field: `HousePanel` never compares, it redraws when told. It is here for
    /// `0x0227`/`0x0228`, which mutate the house data and redraw without any
    /// update notice — see [`Self::update`]'s third arm.
    drawn_from: Option<HouseDataView>,
    /// The same, for section 8's inputs: int quality `0xC7` arrives as `0x02CD
    /// Qualities_PrivateUpdateInt` and moves no house notice at all.
    drawn_purchase: HousePurchaseView,
}

impl HousePanel {
    /// The house panel's post-init, minus the four notice-handler registrations: this build
    /// has no notice bus for the house notices, and the four handlers are the receivers'.
    ///
    /// Bound off the screen root, like [`super::book::BookPanel::post_init`], because `<MAPS>` is
    /// a page of `<PANS>` and the child lookup is recursive.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        self.text_box = ui
            .get_child_recursive(p, TEXT_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
    }

    /// Whether `post_init` found the `HousePanel` element at all.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// Whether the one child `post_init` binds is in the shipped layout too.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some() && self.text_box.is_some()
    }

    /// The text the panel currently shows, one entry per row.
    #[must_use]
    pub fn text(&self) -> Vec<String> {
        self.lines.iter().map(|(s, _)| s.clone()).collect()
    }

    /// The row elements [`Self::add_house_panel_text`] created, in the text box's order — **the tree**, not
    /// [`Self::lines`].
    ///
    /// `lines` is this panel's own shadow copy of what it meant to write, so a row that never
    /// reached a `TextElement` would still read right there. Tests read the glyph list through
    /// these handles instead, which is the only thing on the far side of the text setter and
    /// therefore the only thing a player can see.
    #[must_use]
    pub fn row_elements(&self) -> &[ElemHandle] {
        self.text_box.as_ref().map_or(&[], |l| l.items.as_slice())
    }

    /// One row from the text box's first template, as a
    /// `TextElement`, with fit-to-text (`0x29`) set, then the text.
    ///
    /// The colour is carried rather than rendered: the row text's colour argument is a chat
    /// colour *index* and the three `HousePanelTextColor` values are 0/1/2. Recorded in
    /// [`Self::lines`] as well, so the rent colouring is testable the day the receiver lands.
    pub fn add_house_panel_text(
        &mut self,
        ui: &mut UiSystem,
        text: &str,
        colour: HousePanelTextColor,
    ) {
        let Some(list) = self.text_box.as_mut() else {
            return;
        };
        let Some(h) = list.add_from_template(ui, 0, None) else {
            return;
        };
        ui.set_attribute_bool(h, ATTR_FIT_TO_TEXT, true);
        if let Some(t) = ui.text_element_mut(h) {
            // Retail sets the row's text as a literal: it clears all text, then appends the
            // string with the given font and colour.
            //
            // **The colour must reach the element.** A plain `set_text(text)` takes no colour, so
            // every row of the pane would draw in the element's own `font_color` whatever colour
            // the house-panel text insert had been given, while [`HousePanel::lines`] still
            // recorded the right index.
            //
            // The colours are **not** this file's: they are the row template's own font-colour
            // array, element property `0x1B`, which the colour index selects from. The shipped
            // `0x100001E6` template carries three entries and they are exactly the three
            // `HousePanelTextColor` values [measured on the shipped layout]:
            // `0xFFFFFFFF` white, **`0xFF00FF00` green** and `0xFFFFFF00` yellow.
            t.set_text("");
            t.append_text_with_font_and_color(text, 0, colour as u8);
        }
        // The client's tail — when the element's `0x400` flag is set it resizes the row to its
        // text, which the fit-to-text attribute above turns on.
        //
        // Without it the row keeps the template's 27-pixel height whatever it holds, and
        // `ListBoxWidget::update_layout` — which stacks rows by `region.box_.height()`, exactly as
        // the client does — gives every row one line. Row 8's
        // sentence is three lines at the template's `0x3D` of 280, so two of them would be drawn
        // outside the row's box and clipped after *"…landscape house at <date>"*. Rows 1b, 2 and
        // 7a have the same shape and would lose their tails in the same way.
        ui.resize_to_paper(h);
        self.lines.push((text.to_owned(), colour));
    }

    /// **The one section with no house-data
    /// guard**, which is why the houseless pane is not empty: [`NO_HOUSE`] with no data, else
    /// [`PURCHASE_PRICE`] and the composed buy list, added as one row.
    pub fn display_buy_payment(&mut self, ui: &mut UiSystem, data: Option<&HouseDataView>) {
        let line = match data {
            None => NO_HOUSE.to_owned(),
            Some(d) => format!("{PURCHASE_PRICE}{}", d.buy_text),
        };
        self.add_house_panel_text(ui, &line, HousePanelTextColor::Normal);
    }

    /// `"Rent:\n"` and the composed rent list.
    pub fn display_rent_payment(&mut self, ui: &mut UiSystem, data: &HouseDataView) {
        let line = format!("{RENT}{}", data.rent_text);
        self.add_house_panel_text(ui, &line, HousePanelTextColor::Normal);
    }

    /// `"Bought: "` and [`convert_time`] of the buy time.
    pub fn display_buy_time(&mut self, ui: &mut UiSystem, data: &HouseDataView) {
        let line = format!(
            "{BOUGHT}{}",
            convert_time(data.buy_time, data.utc_offset_secs[0])
        );
        self.add_house_panel_text(ui, &line, HousePanelTextColor::Normal);
    }

    /// **Two** rows, and the second one's period is
    /// doubled when nothing more is owed.
    pub fn display_rent_times(&mut self, ui: &mut UiSystem, data: &HouseDataView) {
        let ends = format!(
            "{PERIOD_ENDS}{}",
            convert_time(data.maintenance_period_end, data.utc_offset_secs[1])
        );
        self.add_house_panel_text(ui, &ends, HousePanelTextColor::Normal);
        let due = format!(
            "{NEXT_DUE}{}",
            convert_time(data.maintenance_next_due, data.utc_offset_secs[2])
        );
        self.add_house_panel_text(ui, &due, HousePanelTextColor::Normal);
    }

    /// See [`location_line`]. Draws **nothing** when
    /// the house-location lookup left either coordinate at `-1`, which is every apartment.
    pub fn display_location(&mut self, ui: &mut UiSystem, data: &HouseDataView) {
        let Some((lx, ly)) = data.location else {
            return;
        };
        let line = location_line(lx, ly);
        self.add_house_panel_text(ui, &line, HousePanelTextColor::Normal);
    }

    /// The only two coloured rows in the pane.
    ///
    /// Index 2 and index 1: `RentNotPaid` for the warning,
    /// `RentPaid` for the already-paid sentence.
    pub fn display_warning_text(&mut self, ui: &mut UiSystem, data: &HouseDataView) {
        if data.rent_owed {
            let line = data.rent_warning.clone();
            self.add_house_panel_text(ui, &line, HousePanelTextColor::RentNotPaid);
        } else {
            self.add_house_panel_text(ui, MAINTENANCE_ALREADY_PAID, HousePanelTextColor::RentPaid);
        }
    }

    /// The purchase-wait line.
    ///
    /// Three arms and one outer guard; see the module header's census. The
    /// guard is the player description, not the house: this section is the only one besides
    /// [`Self::display_buy_payment`] that draws with no house data.
    pub fn display_purchase_time_text(
        &mut self,
        ui: &mut UiSystem,
        data: Option<&HouseDataView>,
        purchase: HousePurchaseView,
    ) {
        if !purchase.have_player_desc {
            return;
        }
        let line = if purchase.wait_expired {
            if data.is_some() {
                BUY_AFTER_ABANDON
            } else {
                BUY_IMMEDIATELY
            }
            .to_owned()
        } else {
            // `0x278D00` — thirty days on to the quality, then `localtime` and
            // `strftime("%c")`. Not [`convert_time`]: this arm formats
            // inline and has no `"N/A"` and no NULL check.
            let when = crate::ctime::strftime_c(
                i64::from(purchase.purchase_timestamp) + PURCHASE_WAIT_SECONDS,
                purchase.utc_offset_secs,
            );
            format!("{BUY_LANDSCAPE_AT}{when}{APARTMENT_EXEMPTION}")
        };
        self.add_house_panel_text(ui, &line, HousePanelTextColor::Normal);
    }

    /// Flush the text box and then the seven sections
    /// in the client's order, which is the order below and is load-bearing: it is the order the
    /// rows appear in the list box.
    pub fn display_house_data(
        &mut self,
        ui: &mut UiSystem,
        data: Option<&HouseDataView>,
        purchase: HousePurchaseView,
    ) {
        if self.text_box.is_none() {
            return;
        }
        if let Some(list) = self.text_box.as_mut() {
            list.flush(ui);
        }
        self.lines.clear();
        self.owns_house = data.is_some();
        self.drawn_from = data.cloned();
        self.drawn_purchase = purchase;
        self.display_buy_payment(ui, data);
        if let Some(d) = data {
            self.display_rent_payment(ui, d);
            self.display_buy_time(ui, d);
            self.display_rent_times(ui, d);
            self.display_location(ui, d);
            self.display_warning_text(ui, d);
        }
        self.display_purchase_time_text(ui, data, purchase);
        if let Some(list) = self.text_box.as_mut() {
            list.update_layout(ui);
        }
        self.displays += 1;
    }

    /// The house panel's house-data update — copy the `HouseData` in, then redraw.
    ///
    /// The copy itself lives in
    /// `house`, because the receiver is `dereth_client_runtime::hud` and this pane is
    /// pulled; what arrives here is that copy projected through
    /// [`crate::view::GameView::house_data`].
    pub fn update_house_data(
        &mut self,
        ui: &mut UiSystem,
        data: Option<&HouseDataView>,
        purchase: HousePurchaseView,
    ) {
        self.display_house_data(ui, data, purchase);
    }

    /// The house panel's house-data clear — **delete** the house data, then redraw.
    ///
    /// This is the arm a houseless retail character actually takes at login, via `0x0226`. See
    /// the module header.
    pub fn house_data_cleared(&mut self, ui: &mut UiSystem, purchase: HousePurchaseView) {
        self.display_house_data(ui, None, purchase);
    }

    /// The frame pull. Returns true on the frame it wrote.
    ///
    /// `status_notices` is
    /// [`crate::view::GameView::house_status_notices`] — how many `0x0226 House_HouseStatus`
    /// answers the client has received — and the first arm below is
    ///  expressed as a pull: retail redraws on
    /// **every** notice, so this redraws every time the count moves.
    ///
    /// The order of the two arms is load-bearing and is the whole of how the stand-in retires
    /// itself. With a shard attached `0x0226` arrives during player initialisation, long before
    /// `<MAPS>` is ever opened, so by the first frame this pane is `fully_bound()` the count is
    /// already 1: the notice arm fires, the pane is drawn **from the real answer**, and
    /// [`Self::stand_in_for_the_login_query`] is never reached at all. With no shard attached —
    /// headless tests, and any build started without a network — the count stays 0 for ever and
    /// the stand-in still writes the constant, so the tab is still not blank.
    ///
    /// That is why the stand-in is kept: deleting it would make the pane blank in exactly the configuration every
    /// headless test and every offline run uses, and the divergence it names is now unreachable
    /// whenever the thing it stands in for actually happens.
    ///
    /// The two notice arms are `0x0225` first and `0x0226` second, which is the order that makes
    /// a shard answering the house query with data win over one answering with a status
    /// in the same frame — retail would draw twice and this can only draw once, and the last
    /// thing retail drew would be whatever arrived last. The `0x0225` arm carries the data, so
    /// preferring it loses less. **That is the one place this pane cannot be retail**, and it is
    /// reachable only if both opcodes land between two frames.
    ///
    /// The third arm is neither notice: the house data's *content* can change without a `0x0225`,
    /// because `0x0227 UpdateRentTime` and `0x0228 UpdateRentPayment` mutate it in place and then
    /// call the house-data display directly (the update-rent-time and update-rent-payment
    /// notices). The comparison is what makes them redraw the pane without a notice of their own.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if !self.fully_bound() {
            return false;
        }
        let purchase = view.house_purchase();
        let data = view.house_data();
        let data_notices = view.house_data_notices();
        let status_notices = view.house_status_notices();
        if self.data_notices_drawn != data_notices {
            self.data_notices_drawn = data_notices;
            self.status_notices_drawn = status_notices;
            self.update_house_data(ui, data.as_ref(), purchase);
            return true;
        }
        if self.status_notices_drawn != status_notices {
            self.status_notices_drawn = status_notices;
            self.house_data_cleared(ui, purchase);
            return true;
        }
        if self.displays == 0 {
            self.stand_in_for_the_login_query(ui, purchase);
            return true;
        }
        // `0x0227`/`0x0228`'s edge, and `0x02CD`'s, and the only pulls in this function that are
        // not counts. Section 8's quality moves no house notice at all: it arrives as a
        // `Qualities_PrivateUpdateInt` and retail simply reads it again on the next redraw, which
        // it gets for free because something else always redraws first. This build has to notice.
        if self.drawn_from.as_ref() != data.as_ref() || self.drawn_purchase != purchase {
            self.update_house_data(ui, data.as_ref(), purchase);
            return true;
        }
        false
    }

    /// The one divergence, isolated in a function whose name says so.
    ///
    /// Retail reaches the same state through the query-house event at player initialisation
    /// and the server's `0x0226` reply, and this is not the only road to the pane: see
    /// [`Self::update`], whose first arms
    /// are the real ones and whose ordering means this runs only when neither `0x0225` nor
    /// `0x0226` has ever arrived. It stays because a client with no shard attached must still
    /// draw the tab, which is the configuration every headless suite runs in.
    pub fn stand_in_for_the_login_query(&mut self, ui: &mut UiSystem, purchase: HousePurchaseView) {
        self.house_data_cleared(ui, purchase);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the retail string quoted in the module header. A paraphrase is the failure this
    /// asserts against — the rule that a formatted/concatenated string is reproduced exactly,
    /// never paraphrased.
    #[test]
    fn the_houseless_line_is_the_string_in_the_image() {
        assert_eq!(NO_HOUSE, "You do not currently own a house.");
        assert_eq!(NO_HOUSE.len(), 33, "33 bytes before the NUL ");
        assert!(
            !NO_HOUSE.ends_with('\n'),
            "the literal has no newline; `Rent:\\n` next door does"
        );
    }

    /// Oracle: `crate::panels::catalogue::PANEL_PAGES` and the `HousePanel` row — the page this
    /// module searches from is a toolbar-stack page and the panel is not.
    #[test]
    fn the_page_is_a_stack_page_and_the_panel_is_a_child_of_it() {
        assert!(crate::panels::catalogue::PANEL_PAGES.contains(&PAGE.0));
        assert!(!crate::panels::catalogue::PANEL_PAGES.contains(&PANEL.0));
        assert!(!crate::panels::catalogue::PANEL_PAGES.contains(&TAB.0));
        let spec = crate::panels::catalogue::spec("HousePanel").expect("HousePanel is catalogued");
        assert_eq!(spec.children.len(), 1);
        assert_eq!(spec.children[0].id, TEXT_BOX);
    }

    /// Every literal is the string in the image.
    #[test]
    fn every_literal_is_the_string_in_the_image() {
        for (s, n) in [
            (PURCHASE_PRICE, 41),
            (RENT, 6),
            (BOUGHT, 8),
            (NEXT_DUE, 25),
            (PERIOD_ENDS, 30),
            (LOCATION_FORMAT, 24),
            (MAINTENANCE_ALREADY_PAID, 100),
            (BUY_LANDSCAPE_AT, 39),
            (APARTMENT_EXEMPTION, 48),
            (BUY_IMMEDIATELY, 38),
            (BUY_AFTER_ABANDON, 65),
            (TIME_NA, 3),
        ] {
            assert_eq!(s.len(), n, "{s:?}");
        }
        assert!(
            PURCHASE_PRICE.ends_with('\n'),
            "the buy list follows straight after it"
        );
        assert!(BOUGHT.ends_with(' ') && !BOUGHT.ends_with('\n'));
        assert_eq!(PURCHASE_WAIT_SECONDS, 2_592_000);
    }

    /// The location line prints latitude first and takes the magnitude.
    #[test]
    fn the_location_line_prints_latitude_first_and_takes_the_magnitude() {
        // 1024 is the origin. `(1024-1024)*0.1 + 0.5 = 0.5`, and `>= 0x400` is N and E.
        assert_eq!(location_line(1024, 1024), "Location: 0.5N, 0.5E");
        // A cell south and west of it: both letters flip and both numbers come back positive.
        assert_eq!(location_line(0, 0), "Location: 101.9S, 101.9W");
        // Latitude is the FIRST number: this pair is unambiguous.
        assert_eq!(location_line(1224, 1524), "Location: 50.5N, 20.5E");
        assert_eq!(
            location_line(1023, 1023),
            "Location: 0.4S, 0.4W",
            "1023 is west, not east"
        );
    }

    /// Convert time has a sentinel for zero.
    #[test]
    fn convert_time_has_a_sentinel_for_zero() {
        assert_eq!(convert_time(0, 0), "N/A");
        // `strftime("%c")` under `English_United States.1252`, which is what `setlocale(LC_ALL,
        // "English")` leaves the CRT in. Measured in the retail install's `msvcr70.dll`.
        assert_eq!(convert_time(1_600_000_000, 0), "9/13/2020 12:26:40 PM");
        // The zone is a real parameter and not decoration: seven hours west moves the clock.
        assert_eq!(
            convert_time(1_600_000_000, -7 * 3600),
            "9/13/2020 5:26:40 AM"
        );
        // The client's NULL-`tm` arm: a negative instant draws nothing at all, not "N/A".
        assert_eq!(convert_time(-1, 0), "");
    }
}
