use super::*;

// =============================================================================================
// house.*
//
// The House pane: what it draws from the shard's house message, the purchase text, number
// grouping, and the abandon gesture.
//
// # The payload is laid out by hand, and that is deliberate
//
// No recorded session carries the shard's house message: a shard only sends one to an account
// that owns a house and the recorded character owns none. So the bytes are written out by hand in
// the shard's own writer order -- as raw dwords rather than through this tree's own encoder, so
// that a decoder regression cannot hide by agreeing with the encoder that produced its input.
// That property is worth more than a shared builder would be, so [`house_data_body`] keeps the
// layout whole.
// =============================================================================================

/// The object this family's shard talks about, which is the player's own body.
pub(super) const HOUSE_PLAYER: ObjectId = ObjectId(0x5000_0001);

/// The quality that says when the player last bought a house.
const HOUSE_PURCHASE_TIMESTAMP: u32 = 0xC7;

/// Holtburg, the cell the client's own coordinate read-out is pinned over.
const HOUSE_CELL: u32 = 0xA9B4_0025;

/// The two house types the pane branches on, and their maintenance periods in seconds.
const VILLA: u32 = 2;
const APARTMENT: u32 = 4;
const THIRTY_DAYS: i64 = 2_592_000;
const NINETY_DAYS: i64 = 90 * 86_400;

/// One payment of a price list: how many, how many are paid, which thing, and its two names.
///
/// The three numbers are in the shard's **member** order and not its layout order -- writing the
/// thing's id first decodes a price list with the quantity in the wrong field.
fn payment(num: i32, paid: i32, wcid: u32, name: &str, plural: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&num.to_le_bytes());
    b.extend_from_slice(&paid.to_le_bytes());
    b.extend_from_slice(&wcid.to_le_bytes());
    for s in [name, plural] {
        b.extend_from_slice(&u16::try_from(s.len()).expect("a short name").to_le_bytes());
        b.extend_from_slice(s.as_bytes());
        while b.len() % 4 != 0 {
            b.push(0);
        }
    }
    b
}

/// A bare price list, which is the whole body of the maintenance-payment update.
fn payment_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut b = u32::try_from(items.len())
        .expect("a short list")
        .to_le_bytes()
        .to_vec();
    for p in items {
        b.extend_from_slice(p);
    }
    b
}

/// The whole house body: the two instants, the type, whether maintenance is waived, the two price
/// lists and the position -- in the shard's own order.
fn house_data_body(
    buy_time: i32,
    rent_time: i32,
    house_type: u32,
    maintenance_free: i32,
    buy: &[Vec<u8>],
    rent: &[Vec<u8>],
    cell: u32,
) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&buy_time.to_le_bytes());
    b.extend_from_slice(&rent_time.to_le_bytes());
    b.extend_from_slice(&house_type.to_le_bytes());
    b.extend_from_slice(&maintenance_free.to_le_bytes());
    for list in [buy, rent] {
        b.extend_from_slice(
            &u32::try_from(list.len())
                .expect("a short list")
                .to_le_bytes(),
        );
        for p in list {
            b.extend_from_slice(p);
        }
    }
    b.extend_from_slice(&cell.to_le_bytes());
    for f in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// A villa bought outright whose maintenance is two thirds paid -- so the pane takes its
/// *warning* arm, which is the one a happy path never reaches.
fn the_villa(buy_time: i32, rent_time: i32) -> Vec<u8> {
    house_data_body(
        buy_time,
        rent_time,
        VILLA,
        0,
        &[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")],
        &[payment(30_000, 20_000, 273, "Pyreal", "Pyreals")],
        HOUSE_CELL,
    )
}

/// A villa as retail screenshots show it: bought for two million with three things in the price,
/// and a maintenance period paid in full.
fn the_owners_villa() -> Vec<u8> {
    house_data_body(
        1_600_000_000,
        1_700_000_000,
        VILLA,
        0,
        &[
            payment(2_000_000, 2_000_000, 273, "Pyreal", "Pyreals"),
            payment(5, 5, 11_710, "Writ of Refuge", "Writs of Refuge"),
            payment(1, 1, 511, "Crude Lockpick", "Crude Lockpicks"),
        ],
        &[
            payment(100_000, 100_000, 273, "Pyreal", "Pyreals"),
            payment(2, 2, 21_073, "Writ of Refuge", "Writs of Refuge"),
        ],
        HOUSE_CELL,
    )
}

/// The wall clock, which is what the purchase-wait arithmetic reads.
fn house_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .expect("a clock after 1970")
}

/// An instant as the pane prints it, in whatever zone the host is in -- which is what the pane
/// asks for too, so this file stays green wherever it runs.
fn c_time(t: i64) -> String {
    dereth_ui_screens::ctime::strftime_c(t, dereth_client::platform::local_utc_offset_secs(t))
}

/// The frames the pane needs to redraw after something arrives.
fn house_settle(c: &mut HeadlessClient) {
    c.tick(6);
}

/// One ordered event with a **hand-laid body**, through the harness's own shard.
///
/// [`Peer::event`] takes a typed message and encodes it, which is exactly what these claims may
/// not do; [`Peer::replay_blob`] re-addresses and re-stamps a blob the caller framed, so the
/// envelope is the shard's and the body is the scenario's.
pub(super) fn house_event(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    opcode: dereth_protocol::Opcode,
    body: &[u8],
) {
    let mut blob = 0xF7B0_u32.to_le_bytes().to_vec();
    // The recipient and the stamp `replay_blob` overwrites.
    blob.extend_from_slice(&[0_u8; 8]);
    blob.extend_from_slice(&opcode.0.to_le_bytes());
    blob.extend_from_slice(body);
    peer.replay_blob(c, blob);
    house_settle(c);
}

/// Tell the client how long ago the player bought a house.
fn purchased_at(c: &mut HeadlessClient, peer: &mut Peer, sequence: u8, when: i64) {
    let m = dereth_protocol::qualities::QualitiesPrivateUpdateInt(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id: HOUSE_PURCHASE_TIMESTAMP,
            value: i32::try_from(when).expect("an instant that fits"),
        },
    );
    peer.event(c, &m);
    house_settle(c);
}

/// A client in the world with a shard attached, a player it has adopted, and the description the
/// last row of the pane is gated on.
///
/// **The description matters and its absence is silent**: the row about buying another house is
/// drawn only once the client has been told who the player is, so a scenario without it asserts
/// over a row that was never drawn.
pub(super) fn a_house_client() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = Peer::attach_creating(&mut c, HOUSE_PLAYER);
    {
        let w = c.world_mut();
        w.player = Some(HOUSE_PLAYER);
        w.weenie_mut(HOUSE_PLAYER)
            .expect("this shard created the player")
            .pwd
            .name = "Larktest".to_owned();
    }
    peer.event(
        &mut c,
        &dereth_protocol::login::LoginPlayerDescription::default(),
    );
    house_settle(&mut c);
    assert!(
        c.view().world().player_qualities().is_some(),
        "the last row of the pane is gated on the client knowing who the player is"
    );
    assert!(
        c.view().expect_app().hud().panels.house.fully_bound(),
        "the House pane is bound off the gameplay root"
    );
    (c, peer)
}

/// The pane's own record of what it wrote, with the colour index of each row.
fn house_lines(
    c: &HeadlessClient,
) -> Vec<(
    String,
    dereth_ui_screens::panels::house::HousePanelTextColor,
)> {
    c.view().expect_app().hud().panels.house.lines.clone()
}

/// Every row as its **element really holds it** -- the far side of the write, which is what a
/// draw would rasterise and what the player sees.
fn house_rows_drawn(c: &mut HeadlessClient) -> Vec<String> {
    let handles = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .row_elements()
        .to_vec();
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the shell is up");
    handles
        .into_iter()
        .map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// Every row as `(text, the colour its glyphs carry, the height of its box)`.
fn house_rows_drawn_full(c: &mut HeadlessClient) -> Vec<(String, u32, i32)> {
    let handles = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .row_elements()
        .to_vec();
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the shell is up");
    handles
        .into_iter()
        .map(|h| {
            let height = shell.ui.screen_box(h).height();
            let t = shell
                .ui
                .text_element_mut(h)
                .expect("a row is a text element");
            let text = t.glyphs.inq_text(false);
            let colour = t.glyphs.glyphs.first().map_or(0, |g| g.color);
            (text, colour, height)
        })
        .collect()
}

/// The one drawn row that begins with `prefix`.
fn house_row_starting(c: &mut HeadlessClient, prefix: &str) -> String {
    let rows = house_rows_drawn(c);
    rows.iter()
        .find(|r| r.starts_with(prefix))
        .unwrap_or_else(|| panic!("no row begins {prefix:?}; the pane holds {rows:#?}"))
        .clone()
}

// ---------------------------------------------------------------------------------------------

/// **The gate.** A character with no house sees two rows; the shard's description of one fills
/// all eight, in the pane's own order, each with its own colour.
pub(super) fn the_shards_house_message_fills_every_row_of_the_tab() {
    use dereth_ui_screens::panels::house::{self, HousePanelTextColor as Colour};

    let (mut c, mut peer) = a_house_client();
    let bought = house_now();
    purchased_at(&mut c, &mut peer, 1, bought);

    assert_eq!(c.view().expect_app().hud().stats.house_data_notices, 0);
    assert!(!c.view().expect_app().hud().panels.house.owns_house);
    let before = house_rows_drawn(&mut c);
    let wait_line = format!(
        "{}{}{}",
        house::BUY_LANDSCAPE_AT,
        c_time(bought + THIRTY_DAYS),
        house::APARTMENT_EXEMPTION
    );
    assert_eq!(
        before,
        vec![house::NO_HOUSE.to_owned(), wait_line.clone()],
        "with no house the pane is the line saying so and the line about buying one"
    );

    let buy_time = 1_500_000_000_i32;
    let rent_time = 1_600_000_000_i32;
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(buy_time, rent_time),
    );

    let notices = c.view().expect_app().hud().stats.house_data_notices;
    let owns = c.view().expect_app().hud().panels.house.owns_house;
    let period_end = i64::from(rent_time) + THIRTY_DAYS;
    let want: Vec<(String, Colour)> =
        vec![
        (format!("{}30,000 Pyreals", house::PURCHASE_PRICE), Colour::Normal),
        (format!("{}20,000/30,000 Pyreals", house::RENT), Colour::Normal),
        (format!("{}{}", house::BOUGHT, c_time(i64::from(buy_time))), Colour::Normal),
        (format!("{}{}", house::PERIOD_ENDS, c_time(period_end)), Colour::Normal),
        // Maintenance is still owed, so the next one is due at the end of *this* period and not
        // a period past it.
        (format!("{}{}", house::NEXT_DUE, c_time(period_end)), Colour::Normal),
        // The latitude is drawn before the longitude, and the cell is normalised against the
        // origin -- so this is the corner of Holtburg's own block.
        ("Location: 42.1N, 33.3E".to_owned(), Colour::Normal),
        (
            "Warning!  You have not paid your maintenance costs for the last 30 day maintenance \
             period.  Please pay these costs by this deadline or you will lose your house, and \
             all your items within it."
                .to_owned(),
            Colour::RentNotPaid,
        ),
        (wait_line, Colour::Normal),
    ];
    let got = house_lines(&c);
    // And the same eight really reached their elements, which is the side the player reads.
    let drawn = house_rows_drawn(&mut c);
    let drawn_matches: Vec<String> = want.iter().map(|(s, _)| s.clone()).collect();

    c.assert_behaviour(
        "house.data.the-shards-house-message-fills-every-row-of-the-tab",
        move |_| notices == 1 && owns && got == want && drawn == drawn_matches,
    );
    c.shutdown();
}

/// The line about buying another house, in all three of its arms and read off the **tree**.
///
/// A first arm that stops at the date is invisible to the pane's own record of what it meant to
/// write, because the near side of the write holds the whole sentence all along. Every reading
/// here is the glyph list a draw would rasterise.
pub(super) fn the_purchase_time_line_reads_what_retail_prints_in_each_arm() {
    use dereth_ui_screens::panels::house;

    let (mut c, mut peer) = a_house_client();

    // Arm one: bought just now, so the wait has not run out. Three terms, and the third is the
    // one a truncated line drops.
    let bought = house_now();
    purchased_at(&mut c, &mut peer, 1, bought);
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let first = house_rows_drawn(&mut c)
        .last()
        .expect("the pane has rows")
        .clone();
    let want_first = format!(
        "{}{}{}",
        house::BUY_LANDSCAPE_AT,
        c_time(bought + THIRTY_DAYS),
        house::APARTMENT_EXEMPTION
    );

    // Arm two: bought forty days ago, and the house is still theirs.
    purchased_at(&mut c, &mut peer, 2, bought - 40 * 86_400);
    let second = house_rows_drawn(&mut c)
        .last()
        .expect("the pane has rows")
        .clone();

    // Arm three: the same wait, and the house is gone.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_STATUS,
        &2_u32.to_le_bytes(),
    );
    let still_owns = c.view().expect_app().hud().panels.house.owns_house;
    let after = house_rows_drawn(&mut c);
    let third = after.last().expect("the pane has rows").clone();

    c.assert_behaviour(
        "house.purchase-time.the-line-reads-what-retail-prints-in-each-of-its-three-arms",
        move |_| {
            first == want_first
                && first.ends_with(house::APARTMENT_EXEMPTION)
                && second == house::BUY_AFTER_ABANDON
                // The third term belongs to the first arm alone and neither of the others may
                // grow one.
                && !second.contains(house::APARTMENT_EXEMPTION)
                && !still_owns
                && after.len() == 2
                && after[0] == house::NO_HOUSE
                && third == house::BUY_IMMEDIATELY
                && !third.contains(house::APARTMENT_EXEMPTION)
        },
    );
    c.shutdown();
}

/// An apartment takes the other side of every guard in the pane at once: no location row, a
/// ninety-day period, the next payment two whole periods out, and the paid sentence in its own
/// colour.
pub(super) fn a_paid_up_apartment_drops_the_location_row_and_doubles_the_period() {
    use dereth_ui_screens::panels::house::{self, HousePanelTextColor as Colour};

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());

    let rent_time = 1_600_000_000_i32;
    let body = house_data_body(
        1_500_000_000,
        rent_time,
        APARTMENT,
        0,
        // One of a thing takes its singular name and not its plural.
        &[payment(1, 1, 273, "Pyreal", "Pyreals")],
        &[payment(10_000, 10_000, 273, "Pyreal", "Pyreals")],
        HOUSE_CELL,
    );
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &body,
    );

    let text = house_rows_drawn(&mut c);
    let lines = house_lines(&c);
    let ends = format!(
        "{}{}",
        house::PERIOD_ENDS,
        c_time(i64::from(rent_time) + NINETY_DAYS)
    );
    let due = format!(
        "{}{}",
        house::NEXT_DUE,
        c_time(i64::from(rent_time) + 2 * NINETY_DAYS)
    );

    c.assert_behaviour(
        "house.data.a-paid-up-apartment-drops-the-location-row-and-doubles-the-period",
        move |_| {
            text.len() == 7
                && !text.iter().any(|l| l.starts_with("Location:"))
                && text[0] == format!("{}1 Pyreal", house::PURCHASE_PRICE)
                && text[1] == format!("{}10,000/10,000 Pyreals", house::RENT)
                && text[3] == ends
                && text[4] == due
                && text[5] == house::MAINTENANCE_ALREADY_PAID
                && lines[5].1 == Colour::RentPaid
        },
    );
    c.shutdown();
}

/// A zero instant is *not available* rather than the first moment of 1970 -- and the zero that
/// still has a period added to it is a real date, which is what makes the first a sentinel and
/// not a rule about zero.
pub(super) fn a_zero_instant_prints_the_sentinel_and_not_the_epoch() {
    use dereth_ui_screens::panels::house;

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, 0);
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(0, 0),
    );

    let text = house_rows_drawn(&mut c);
    let ends = format!("{}{}", house::PERIOD_ENDS, c_time(THIRTY_DAYS));

    c.assert_behaviour(
        "house.data.a-zero-instant-prints-the-sentinel-and-not-the-start-of-the-epoch",
        move |_| {
            text[2] == format!("{}{}", house::BOUGHT, house::TIME_NA)
                && text[3] == ends
                // A purchase instant of zero is a wait that ran out however the clock is set.
                && text.last().is_some_and(|l| l == house::BUY_AFTER_ABANDON)
        },
    );
    c.shutdown();
}

/// Every row is made as tall as its own text, so the sentence that wraps onto three lines is
/// drawn on three lines inside its row and inside the pane.
///
/// The assertion is the same arithmetic the scrollbar sizer uses, per row -- so a row that keeps
/// the template's single-line height while holding three lines fails here whichever row it is.
pub(super) fn a_house_row_is_as_tall_as_the_text_it_holds() {
    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_owners_villa(),
    );

    let handles = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .row_elements()
        .to_vec();
    let panel = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .panel
        .expect("the pane is in the tree");
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the shell is up");
    let pane_box = shell.ui.screen_box(panel);
    let mut tall_enough = true;
    let mut wrapped_at_all = false;
    let mut margin_holds = true;
    let mut wrap_width_is_the_templates = true;
    let mut fits_to_text = true;
    let mut last_bottom = 0;
    for h in &handles {
        let b = shell.ui.screen_box(*h);
        let max_width = shell.ui.node(*h).and_then(|n| {
            n.merged_properties()
                .get_int(dereth_ui::props::attr::MAX_WIDTH)
        });
        wrap_width_is_the_templates &= max_width == Some(280);
        let t = shell
            .ui
            .text_element_mut(*h)
            .expect("a row is a text element");
        fits_to_text &= t.bits.fit_to_text();
        let (_, needed) = t.scrollable_extent(b);
        tall_enough &= b.height() >= needed;
        wrapped_at_all |= needed > 27;
        // The height of a row is a whole number of lines plus the template's own bottom margin.
        let drawn_lines = b.height().saturating_sub(8) / 16;
        margin_holds &= b.height() == drawn_lines * 16 + 8 && drawn_lines >= 1;
        last_bottom = b.y1;
    }
    let rows = handles.len();
    let inside = last_bottom <= pane_box.y1;
    let ends_whole = house_rows_drawn(&mut c)
        .last()
        .is_some_and(|r| r.ends_with(dereth_ui_screens::panels::house::APARTMENT_EXEMPTION));

    c.assert_behaviour(
        "house.rows.a-row-is-as-tall-as-the-text-it-holds-and-fits-inside-the-pane",
        move |_| {
            rows == 8
            && wrap_width_is_the_templates
            && fits_to_text
            && tall_enough
            // A fixture with nothing that wraps would prove nothing at all.
            && wrapped_at_all
            && margin_holds
            && inside
            && ends_whole
        },
    );
    c.shutdown();
}

/// The paid-maintenance sentence is drawn in the template's second colour and every other row in
/// its first -- read off the composed glyphs, because the colour is a property of the glyph.
pub(super) fn the_paid_maintenance_line_is_drawn_in_the_templates_green() {
    use dereth_ui_screens::panels::house::{self, HousePanelTextColor as Colour};

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_owners_villa(),
    );

    // The array the colour indices resolve through, read off the shipped row template rather
    // than written down here: an index out of its range leaves the element's own colour, so a
    // template with fewer entries would show as an un-coloured row and not as a failure.
    let palette = {
        let h = *c
            .view()
            .expect_app()
            .hud()
            .panels
            .house
            .row_elements()
            .first()
            .expect("rows");
        let app = c.app_mut();
        app.ui_mut()
            .expect("the shell")
            .ui
            .text_element_mut(h)
            .expect("text")
            .font_colors
            .clone()
    };
    let lines = house_lines(&c);
    let rows = house_rows_drawn_full(&mut c);

    c.assert_behaviour(
        "house.rows.the-paid-maintenance-line-is-drawn-in-the-colour-the-template-ships",
        move |_| {
            palette == vec![0xFFFF_FFFF_u32, 0xFF00_FF00, 0xFFFF_FF00]
                && rows.len() == 8
                && lines[6].1 == Colour::RentPaid
                && rows[6].0 == house::MAINTENANCE_ALREADY_PAID
                && rows[6].1 == palette[1]
                && rows
                    .iter()
                    .enumerate()
                    .all(|(i, r)| i == 6 || r.1 == palette[0])
        },
    );
    c.shutdown();
}

/// The two price rows: several payments joined with a comma and a space, every count grouped the
/// way the shipped language data says to, and the row wrapping onto as many lines as it needs.
///
/// The second half is the part that is easy to lose: the composer the other windows share is
/// deliberately **left ungrouped**, because the grouping is applied at this pane's own projection
/// and no screenshot of those other windows was ever taken.
pub(super) fn the_house_price_rows_are_comma_joined_and_grouped() {
    use dereth_ui_screens::panels::house;
    use dereth_ui_screens::panels::numfmt;

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_owners_villa(),
    );

    let owns = c.view().expect_app().hud().panels.house.owns_house;
    let rows = house_rows_drawn(&mut c);

    // The rule itself, off the shipped language data.
    numfmt::clear_cache();
    let g = numfmt::shipped();
    let rule = (g.size, g.separator.clone());
    let grouped = (
        numfmt::group(2_000_000_i64, &g),
        numfmt::group(100_000_i64, &g),
    );

    // And the shared composer, which is not this pane's projection and stays plain.
    let shared = dereth_client_model::housing::HousePayment {
        num: 2_000_000,
        paid: 2_000_000,
        wcid: 273,
        name: "Pyreal".to_owned(),
        pname: "Pyreals".to_owned(),
    };
    let shared_text = (shared.compose_text(), shared.compose_text2());

    // The wrap the shipped font and the template's own content width really produce.
    let wrapped: Vec<String> = {
        let h = c.view().expect_app().hud().panels.house.row_elements()[0];
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the shell");
        let screen = shell.ui.screen_box(h);
        let t = shell
            .ui
            .text_element_mut(h)
            .expect("a row is a text element");
        let content = t.content_box(screen);
        dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line)
            .into_iter()
            .map(|line| {
                let utf16: Vec<u16> = t.glyphs.glyphs[line.start..line.end]
                    .iter()
                    .map(|g| g.data)
                    .collect();
                String::from_utf16_lossy(&utf16).trim().to_owned()
            })
            .collect()
    };

    c.assert_behaviour(
        "house.prices.the-price-rows-are-comma-joined-and-their-counts-are-grouped",
        move |_| {
            owns
            && rows.len() == 8
            && rows[0]
                == format!(
                    "{}2,000,000 Pyreals, 5 Writs of Refuge, 1 Crude Lockpick",
                    house::PURCHASE_PRICE
                )
            && rows[1]
                == format!("{}100,000/100,000 Pyreals, 2/2 Writs of Refuge", house::RENT)
            // There is no full stop between payments in either composer.
            && !rows[0].contains(". ")
            && rule == (3, ",".to_owned())
            && grouped == ("2,000,000".to_owned(), "100,000".to_owned())
            && shared_text == ("2000000 Pyreals".to_owned(), "2000000/2000000 Pyreals".to_owned())
            && wrapped.len() == 3
            && wrapped[2] == "Lockpick"
        },
    );
    c.shutdown();
}

/// Learning who the player is asks the shard about their house, once -- and a second telling asks
/// nothing, while re-arming the flag the way a log-off does asks again.
///
/// The last step is what makes the zero in the middle a guard rather than a send path that never
/// worked.
pub(super) fn the_client_asks_about_the_house_once_when_it_learns_who_it_is() {
    let (mut c, mut peer) = a_house_client();

    let initialized = c.view().world().player_initialized;
    // The requests are drained every frame, so the counters are the durable record and the
    // outbound list is what names the one that went.
    let sent = |c: &HeadlessClient| {
        let s = &c.view().interaction().stats;
        s.requests_sent + s.requests_undeliverable
    };
    let after_first = sent(&c);
    let named = c
        .view()
        .outbound()
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::QueryHouse(_)));

    let desc = dereth_protocol::login::LoginPlayerDescription::default();
    peer.event(&mut c, &desc);
    house_settle(&mut c);
    let after_second = sent(&c);

    c.world_mut().player_initialized = false;
    peer.event(&mut c, &desc);
    house_settle(&mut c);
    let after_third = sent(&c);

    c.assert_behaviour(
        "house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is",
        move |_| {
            initialized && named && after_second == after_first && after_third == after_first + 1
        },
    );
    c.shutdown();
}

/// A new maintenance period moves both dates and puts every payment back to nothing paid, so the
/// pane keeps warning rather than showing the last period's payment against the new one.
pub(super) fn a_new_maintenance_period_clears_every_payment() {
    use dereth_ui_screens::panels::house::HousePanelTextColor as Colour;

    let (mut c, mut peer) = a_house_client();
    let rent_time = 1_600_000_000_i32;
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, rent_time),
    );
    let before_rows = house_rows_drawn(&mut c).len();
    let before_ends = house_row_starting(&mut c, "This maintenance period ends: ");
    let before_rent = house_row_starting(&mut c, "Rent:");

    let next = rent_time + i32::try_from(THIRTY_DAYS).expect("a period that fits");
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_TIME,
        &next.to_le_bytes(),
    );

    let stats = c.view().expect_app().hud().stats;
    let after_ends = house_row_starting(&mut c, "This maintenance period ends: ");
    let after_rent = house_row_starting(&mut c, "Rent:");
    let after_rows = house_rows_drawn(&mut c).len();
    let warnings = house_lines(&c)
        .iter()
        .filter(|(_, col)| *col == Colour::RentNotPaid)
        .count();

    c.assert_behaviour(
        "house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid",
        move |_| {
            before_rows == 8
                && before_rent == "Rent:\n20,000/30,000 Pyreals"
                && stats.house_rent_time_updates == 1
                && stats.house_rent_time_applied == 1
                && after_rent == "Rent:\n0/30,000 Pyreals"
                && after_ends != before_ends
                && after_rows == 8
                // A fresh period is an unpaid one, which is the whole point of clearing them.
                && warnings == 1
        },
    );
    c.shutdown();
}

/// A maintenance-payment update replaces the list rather than merging into it, leaves the
/// purchase row alone, and -- when it pays in full -- changes the numbers, the warning sentence,
/// its colour and how far away the next payment is, all at once.
pub(super) fn a_payment_update_replaces_the_list_rather_than_merging() {
    use dereth_ui_screens::panels::house::HousePanelTextColor as Colour;

    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let buy_before = house_row_starting(&mut c, "The purchase price");
    let ends = house_row_starting(&mut c, "This maintenance period ends: ");
    let due_before = house_row_starting(&mut c, "Maintenance is next due: ");
    let same_while_owed = ends.trim_start_matches("This maintenance period ends: ")
        == due_before.trim_start_matches("Maintenance is next due: ");

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")]),
    );

    let stats = c.view().expect_app().hud().stats;
    let rent = house_row_starting(&mut c, "Rent:");
    let buy_after = house_row_starting(&mut c, "The purchase price");
    let ends_after = house_row_starting(&mut c, "This maintenance period ends: ");
    let due_after = house_row_starting(&mut c, "Maintenance is next due: ");
    let coloured: Vec<Colour> = house_lines(&c)
        .into_iter()
        .map(|(_, col)| col)
        .filter(|col| *col != Colour::Normal)
        .collect();
    let paid_sentence = house_rows_drawn(&mut c)
        .iter()
        .any(|t| t.starts_with("The maintenance has already been paid"));

    // ...and a shorter list really is shorter: a second update with one different payment in it
    // leaves nothing of the first behind.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(2, 2, 21_073, "Pyreal Scarab", "Pyreal Scarabs")]),
    );
    let replaced = house_row_starting(&mut c, "Rent:");

    c.assert_behaviour(
        "house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it",
        move |_| {
            same_while_owed
                && stats.house_rent_payment_updates == 1
                && stats.house_rent_payment_applied == 1
                && rent == "Rent:\n30,000/30,000 Pyreals"
                && buy_after == buy_before
                && ends_after == ends
                // Paid in full, so the next payment is a whole extra period away.
                && due_after != due_before
                && coloured == vec![Colour::RentPaid]
                && paid_sentence
                && replaced == "Rent:\n2/2 Pyreal Scarabs"
        },
    );
    c.shutdown();
}

/// Both maintenance updates arrive at a houseless character, and both are deliberately ignored:
/// no house is invented and nothing is redrawn.
///
/// The pair of counters is what makes *arrived* and *applied* separable; without the second there
/// would be no telling a working guard from a message that never reached a receiver at all.
pub(super) fn neither_maintenance_update_touches_a_houseless_character() {
    let (mut c, mut peer) = a_house_client();
    house_settle(&mut c);
    let before = house_rows_drawn(&mut c);

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_TIME,
        &1_600_000_000_i32.to_le_bytes(),
    );
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")]),
    );

    let stats = c.view().expect_app().hud().stats;
    let invented = c.view().world().house.is_some();
    let after = house_rows_drawn(&mut c);

    c.assert_behaviour(
        "house.rent.neither-update-touches-a-character-with-no-house",
        move |_| {
            before.len() == 2
                && stats.house_rent_time_updates == 1
                && stats.house_rent_payment_updates == 1
                && stats.house_rent_time_applied == 0
                && stats.house_rent_payment_applied == 0
                && !invented
                && after == before
        },
    );
    c.shutdown();
}

/// A list of who may enter is stored against the object the shard names it for, and one about the
/// player themselves is dropped rather than stored.
pub(super) fn a_restriction_update_reaches_the_object_it_names() {
    const OTHER: ObjectId = ObjectId(0x5000_0002);

    let (mut c, mut peer) = a_house_client();
    {
        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id: OTHER,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        let blob = dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("an object create encodes");
        peer.send(&mut c, 10, blob);
        house_settle(&mut c);
    }

    // The body is unaligned from its second byte, so it is laid out here: a sequence byte, the
    // object the list is about, the open-house flag, and an empty guest table -- whose header is
    // a single zero word, which is the third of the protocol's three hash-table headers and not
    // the one a first draft reaches for.
    let body = |seq: u8, who: ObjectId| -> Vec<u8> {
        let mut b = vec![seq];
        b.extend_from_slice(&who.0.to_le_bytes());
        b.extend_from_slice(&1_u32.to_le_bytes());
        b.extend_from_slice(&0_u32.to_le_bytes());
        b
    };

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RESTRICTIONS,
        &body(1, OTHER),
    );
    let first = c.view().expect_app().hud().stats;
    let other_has = c
        .view()
        .world()
        .weenie(OTHER)
        .expect("the other object")
        .pwd
        .restrictions
        .is_some();

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RESTRICTIONS,
        &body(1, HOUSE_PLAYER),
    );
    let second = c.view().expect_app().hud().stats;
    let player_has = c
        .view()
        .world()
        .weenie(HOUSE_PLAYER)
        .expect("the player")
        .pwd
        .restrictions
        .is_some();

    c.assert_behaviour(
        "house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player",
        move |_| {
            first.house_restriction_updates == 1
                && first.house_restrictions_applied == 1
                && other_has
                && second.house_restriction_updates == 2
                && second.house_restrictions_applied == 1
                && !player_has
        },
    );
    c.shutdown();
}

/// The other answer a shard can give about a house that is gone empties the pane exactly as the
/// first one does.
pub(super) fn a_transaction_answer_clears_the_pane_as_a_status_does() {
    use dereth_ui_screens::panels::house;

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now() - 40 * 86_400);
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let before = house_rows_drawn(&mut c).len();
    let notices = c.view().expect_app().hud().stats.house_status_notices;

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_TRANSACTION,
        &2_u32.to_le_bytes(),
    );

    let after_notices = c.view().expect_app().hud().stats.house_status_notices;
    let owns = c.view().world().house.is_some();
    let after = house_rows_drawn(&mut c);

    c.assert_behaviour(
        "house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does",
        move |_| {
            before == 8
                && after_notices == notices + 1
                && !owns
                && after.len() == 2
                && after[0] == house::NO_HOUSE
                && after[1] == house::BUY_IMMEDIATELY
        },
    );
    c.shutdown();
}

/// The player's gesture, end to end: asking to abandon a house asks twice, the first answer
/// sends nothing, the second sends the one request -- and the pane does not move until the shard
/// answers.
///
/// The middle claim is the one that needs saying out loud. A client that emptied the pane on the
/// send would look right in a happy-path screenshot and be wrong for every refusal a shard can
/// answer with, where the house is still the player's and the tab must still say so.
pub(super) fn two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane() {
    use dereth_ui_screens::panels::house;

    /// The shipped confirmation dialog's Yes.
    const DIALOG_YES: ElementId = ElementId(0x17);

    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let started_with = house_rows_drawn(&mut c).len();
    let had_house = c.view().world().house.is_some();

    // The command, typed into the chat entry a character at a time and committed with return --
    // which is the only way a player reaches this ladder.
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    hand.say(&mut c, "@house abandon");
    c.tick(1);

    let first_prompt = dialog_prompt(&mut c);
    answer_the_dialog(&mut c, &mut hand, DIALOG_YES);
    let nothing_yet = abandons(&c).is_empty();

    let second_prompt = dialog_prompt(&mut c);
    answer_the_dialog(&mut c, &mut hand, DIALOG_YES);
    let wire = abandons(&c);
    // ...and it was really framed into a datagram, which the outbox alone does not say.
    let framed = c.outbound_wire().contains(&0x0000_021F);

    // What that request encodes to, on the queue it goes out on: an empty body inside the game
    // action envelope, ordered, on the weenie queue.
    let bytes_and_queue = {
        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let framed = dereth_client_runtime::requests::send_request(&mut session, &wire[0]);
        let packet = session.transport.sent.last().expect("one datagram").clone();
        let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1_u32.to_le_bytes());
        want.extend_from_slice(&0x0000_021F_u32.to_le_bytes());
        (framed, packet.payload == want, packet.queue, packet.ordered)
    };

    let still_has_house = c.view().world().house.is_some();
    let still_eight = house_rows_drawn(&mut c).len();

    // The shard's answer, which is the only thing that empties it.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_STATUS,
        &2_u32.to_le_bytes(),
    );
    let gone = c.view().world().house.is_none();
    let after = house_rows_drawn(&mut c);

    c.assert_behaviour(
        "house.abandon.two-confirmations-send-it-and-only-the-shards-answer-empties-the-pane",
        move |_| {
            started_with == 8
                && had_house
                && first_prompt.as_deref()
                    == Some(dereth_client_model::chat_cmd::HOUSE_ABANDON_FIRST)
                && nothing_yet
                && second_prompt.as_deref()
                    == Some(dereth_client_model::chat_cmd::HOUSE_ABANDON_SECOND)
                && wire.len() == 1
                && matches!(wire[0], dereth_client_model::Request::AbandonHouse(_))
                && framed
                && bytes_and_queue == (true, true, dereth_primitives::NetQueue::Weenie, true)
                && still_has_house
                && still_eight == 8
                && gone
                && after.len() == 2
                && after[0] == house::NO_HOUSE
                && after[1] == house::BUY_IMMEDIATELY
        },
    );
    c.shutdown();
}

/// Issuing the same confirmed command twice asks twice: the first question is on screen with the
/// second waiting behind it, and answering the first brings up the second in the same words.
///
/// Both commands the client asks about on its own are typed twice before either is answered,
/// `@die` and then `@house abandon`, and each question is answered No, so the claim is the queue
/// alone: nothing reaches the wire, and after the second No nothing is left on screen or waiting.
pub(super) fn the_same_command_twice_asks_twice_one_question_after_the_other() {
    use dereth_client_model::chat_cmd::{DIE_CONFIRMATION, HOUSE_ABANDON_FIRST};
    use dereth_ui::dialog::factory::DEFAULT_QUEUE;

    /// The first prompt and the waiting count beside it, the second prompt and the waiting count
    /// beside that, whether a question was still open after the second answer, and how many were
    /// still waiting.
    type Asked = (
        (Option<String>, usize),
        (Option<String>, usize),
        bool,
        usize,
    );

    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    let waiting = |c: &mut HeadlessClient| {
        c.app_mut()
            .ui()
            .map_or(usize::MAX, |s| s.ui.dialogs.waiting_on(DEFAULT_QUEUE))
    };
    let open = |c: &mut HeadlessClient| {
        c.app_mut()
            .ui()
            .is_some_and(|s| s.ui.dialogs.is_dialog_open(DEFAULT_QUEUE))
    };

    // Asks the same question twice through `command`, answering No each time, and returns what
    // the player saw.
    let mut ask_twice = |c: &mut HeadlessClient, command: &str| -> Asked {
        hand.say(c, command);
        c.tick(1);
        hand.say(c, command);
        c.tick(1);
        let first = (dialog_prompt(c), waiting(c));
        answer_the_dialog(c, &mut hand, DIALOG_NO_BUTTON);
        let second = (dialog_prompt(c), waiting(c));
        answer_the_dialog(c, &mut hand, DIALOG_NO_BUTTON);
        (first, second, open(c), waiting(c))
    };

    let die = ask_twice(&mut c, "@die");
    let abandon = ask_twice(&mut c, "@house abandon");

    let sent_nothing = !c.view().outbound().iter().any(|r| {
        matches!(
            r,
            dereth_client_model::Request::Suicide(_)
                | dereth_client_model::Request::AbandonHouse(_)
        )
    });

    c.assert_behaviour(
        "dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other",
        move |_| {
            let asked_twice = |got: &Asked, words: &str| {
                let ((first, waiting_first), (second, waiting_second), still_open, left) = got;
                first.as_deref() == Some(words)
                    && *waiting_first == 1
                    && second.as_deref() == Some(words)
                    && *waiting_second == 0
                    && !still_open
                    && *left == 0
            };
            asked_twice(&die, DIE_CONFIRMATION)
                && asked_twice(&abandon, HOUSE_ABANDON_FIRST)
                && sent_nothing
        },
    );
    c.shutdown();
}

/// What the open confirmation dialog's prompt really says, trimmed.
///
/// A dialog's subtree is not under the current screen's roots, so
/// `Target::Element` cannot reach its buttons and `UiSnapshot` cannot see its text; the dialog
/// queue is where an open one is, and that is what this and [`answer_the_dialog`] walk.
pub(super) fn dialog_prompt(c: &mut HeadlessClient) -> Option<String> {
    let app = c.app_mut();
    let root = app
        .ui()?
        .ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)?
        .element?;
    let ui = &mut app.ui_mut()?.ui;
    let h = ui.get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)?;
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false).trim().to_owned())
}

/// Every request to abandon a house this scenario has put on the wire so far.
///
/// **It reads the accumulating outbox and not the layer's own `last_sent`**, which is a one-frame
/// window the send pass replaces.
fn abandons(c: &HeadlessClient) -> Vec<dereth_client_model::Request> {
    c.view()
        .outbound()
        .iter()
        .filter(|r| matches!(r, dereth_client_model::Request::AbandonHouse(_)))
        .cloned()
        .collect()
}

/// Press the open dialog's `button` with a real hit-tested click, and run the frames that carry
/// what it raised out to the wire.
pub(super) fn answer_the_dialog(
    c: &mut HeadlessClient,
    hand: &mut dereth_testkit::adapters_chat::Hand,
    button: ElementId,
) {
    let at = {
        let app = c.app_mut();
        let root = app
            .ui()
            .expect("the shell is up")
            .ui
            .dialogs
            .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
            .expect("a dialog is open to answer")
            .element
            .expect("an open dialog has a root");
        let ui = &app.ui().expect("the shell is up").ui;
        let h = ui
            .get_child_recursive(root, button)
            .expect("the shipped confirmation root carries both buttons");
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid(), "the button has a real visible clip");
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        assert!(
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
            "the button is the thing under the pointer"
        );
        at
    };
    hand.press_at(c, at.0, at.1);
    c.tick(6);
}
