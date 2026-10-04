//! The world map, its profile-selected markers, and the marker placement rule.
//!
//! Marker rectangles and literal names come from the shared world-profile location table.
//! Artwork and marker placement remain local to the panel.
//!
//! **There is no dungeon map.** `MapPanel` renders one image and hides the player marker
//! whenever the player is not outside; indoors the only positional feedback is the
//! radar at 25-unit range and the `@loc` chat command. Nothing in this module has an indoor path.

use dereth_primitives::num::to_i32_f64;
use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

pub use dereth_client_contract::panels::map::{notes, MapNote};
use dereth_primitives::EraId;

/// A location row is `ulong X, Y, Width, Height; wchar_t* Name` = 20 bytes.
pub const LOCATION_ROLLOVER_INFO_SIZE: usize = 20;

/// The marker area, read off the map image element by the panel's post-init.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MarkerArea {
    /// Attribute `0x1000004E`.
    pub x0: i32,
    /// Attribute `0x1000004F`.
    pub x1: i32,
    /// Attribute `0x10000050`.
    pub y0: i32,
    /// Attribute `0x10000051`.
    pub y1: i32,
}

/// The client's five child lookups, in call order.
///
/// ```text
///   0x100001EB   -> the date/time text        (a TextElement)
///   0x100001EF   -> the coordinate text       (ditto)
///   0x100001ED   -> player-location icon      (a plain element)
///   0x100001EE   -> the house-location icon
///   0x100001EC   -> the map image             (then 0x1000004E..0x10000051 off it)
/// ```
///
/// The same five rows `panels::catalogue::MAP` carries.
pub mod child {
    use dereth_ui::ElementId;
    /// The date/time text.
    pub const DATE_TIME_TEXT: ElementId = ElementId(0x1000_01EB);
    /// The map image, and the owner of the marker area.
    pub const MAP_IMAGE: ElementId = ElementId(0x1000_01EC);
    /// The player-location icon — the green circle.
    pub const PLAYER_LOCATION_ICON: ElementId = ElementId(0x1000_01ED);
    /// The house-location icon.
    pub const HOUSE_LOCATION_ICON: ElementId = ElementId(0x1000_01EE);
    /// The coordinate text.
    pub const COORDINATE_TEXT: ElementId = ElementId(0x1000_01EF);
}

/// The map page itself — `MapPanel`'s own element in the shipped `classic_gameplay` layout.
///
/// `<MAPS>` `0x1000018C` is the toolbar page container and this is its **first tab**, the one that
/// opens by default (`panels::house` records the same pairing from the other side). It is the
/// element the panel compares the message's element against on
/// its `0x18` arm — "is this message about me" — so it is what this build has to bind in order to
/// see the page open. Measured off the shipped tree, not guessed: it is the
/// nearest ancestor of the map image whose element **type** is `MapPanel` `0x10000026`.
pub const MAP_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_01F6);

/// The four DAT graphics the shipped map page blits, measured off the live element tree built
/// from the retail `client_local_English.dat`.
///
/// Nothing in the client *names* these: they are `UICore_Region_background` values on the layout's
/// own elements, which is why they are recorded as a measurement of the shipped layout rather than
/// as a constant taken from the client itself. The map itself is one 257x267 picture; there is no
/// tiling, no zoom and no second level of detail (see [`MAP_INTERACTION`]).
pub mod graphic {
    use dereth_primitives::DataId;
    /// The map image `0x100001EC`'s background — **the world map picture**.
    pub const MAP_IMAGE: DataId = DataId(0x0600_127D);
    /// The player-location icon `0x100001ED`'s background — the green circle, 17x16.
    pub const PLAYER_ICON: DataId = DataId(0x0600_4D10);
    /// The house-location icon `0x100001EE`'s background — the house pin, 8x8.
    pub const HOUSE_ICON: DataId = DataId(0x0600_4D11);
    /// The four edge pieces the note layout `0x21000026` frames each rollover with.
    pub const NOTE_EDGE: DataId = DataId(0x0600_4CC9);
}

/// The two attribute values the shipped map image carries for its notes, and the marker box —
/// measured off the retail layout, and asserted in
/// [`crate::screens::gameplay::GamePlayScreen`]'s station rather than hard-coded into production.
///
/// ```text
/// 0x47 = Enum   0x100001F0     ; the note element id  -> attr::NOTE_ELEMENT_ENUM
/// 0x48 = DataID 0x21000026     ; the note LayoutDesc  -> attr::NOTE_LAYOUT_DID
/// 0x1000004E..51 = 6, 247, 8, 258   ; the marker area, in the map image's own pixel space
/// ```
///
/// The panel's post-init turns the `0x48` DataID into a `LayoutDesc` by loading it as DB type
/// `0x23`, i.e. DB type `0x23` is `LayoutDesc`; it is **not** routed through the `DidMapper` enum table the way
/// [`crate::env::create_child_element_by_enum`]'s callers are, which is why
/// [`create_map_notes`] resolves nothing and hands the DataID straight to the asset source.
pub const SHIPPED_NOTE_BINDING: (u32, u32, MarkerArea) = (
    0x1000_01F0,
    0x2100_0026,
    MarkerArea {
        x0: 6,
        x1: 247,
        y0: 8,
        y1: 258,
    },
);

/// The five attributes the map image element carries.
pub mod attr {
    /// The marker area's left edge.
    pub const MARKER_AREA_X0: u32 = 0x1000_004E;
    /// The marker area's right edge.
    pub const MARKER_AREA_X1: u32 = 0x1000_004F;
    /// The marker area's top edge.
    pub const MARKER_AREA_Y0: u32 = 0x1000_0050;
    /// The marker area's bottom edge.
    pub const MARKER_AREA_Y1: u32 = 0x1000_0051;
    /// The element enum each location note is created from.
    pub const NOTE_ELEMENT_ENUM: u32 = 0x47;
    /// The `LayoutDesc` DataID the notes are created from.
    pub const NOTE_LAYOUT_DID: u32 = 0x48;
}

/// One location note, created, moved, resized and tooltipped.
///
/// Create a child under the map with the supplied layout and id. A null result is
/// skipped without further work. Otherwise move and resize the child to the row's
/// rectangle, copy the row name into a wide literal string and install its tooltip.
///
/// `(X, Y)` and `(Width, Height)` are in the map image's **own** pixel space, because the note is
/// created as a child of the map image — the same space [`place_marker_on_map`] produces and the same
/// space [`MarkerArea`] is expressed in. Nothing here converts anything.
///
/// The tooltip is the single load-bearing effect: an element is mouse-visible when it has a
/// context menu or valid tooltip text, so **giving the note its name is what
/// makes it hoverable at all**. `None` is the client's own null-child arm.
///
/// The two lifecycle calls after the create are the client's own, in its order and with its
/// **bottom-up** initialisation, which is what [`dereth_ui::UiSystem::initialize_tree`] does; a
/// parent-first walk leaves every note framed.
pub fn add_map_note(
    ui: &mut UiSystem,
    map: ElemHandle,
    layout: DataId,
    element: ElementId,
    note: MapNote,
) -> Option<ElemHandle> {
    let created = ui.env().cloned().map(|e| {
        e.with_assets(|assets| ui.create_child_by_data_id(assets, map, layout, element))
    })?;
    let h = created.ok()?;
    // Initialize the subtree bottom-up, then run its root button's post-init. That order is what
    // [`dereth_ui::UiSystem::initialize_tree`] itself does, for every creator in the client, so
    // this site needs no private walk.
    ui.initialize_tree(h);
    ui.move_to(h, note.x, note.y);
    ui.resize_to(h, note.w, note.h);
    ui.set_tooltip(h, Some(note.name.to_owned()));
    Some(h)
}

/// The two states a map note can rest in, derived from the button state update. The note is a
/// `Button` and the whole of the state update's arithmetic is:
///
/// Disabled attribute `0x0D` selects state `0x0D` before any other checks. Otherwise
/// toggled attribute `0x0E` chooses base state 6 instead of 1. Rollover highlighting
/// (`0x13`) can add 1; pressed-and-over adds 2. The selected state is applied only
/// if its description exists.
///
/// A map note is never disabled, never toggled and never pressed, and it carries
/// `0x13 = true`, so it is **1 when the pointer is elsewhere and 2 when the pointer is on it**,
/// and nothing else. Both pass through to children, and the frame child `0x100001F1` answers them
/// with `0x3B UICore_Element_hide` `true` / `false` — so *state 1 is an unframed note*.
/// The button element's mouse-over handler records the mouse-over flag and then runs the state
/// update, and is called on both edges, which is what un-highlights a note the pointer leaves.
pub const NOTE_REST_STATE: u32 = 1;
/// See [`NOTE_REST_STATE`] — the hovered note.
pub const NOTE_ROLLOVER_STATE: u32 = 2;

/// The client's note loop — the selected [`add_map_note`] calls, in table order.
///
/// Read element enum `0x47` and layout data id `0x48` from the map. Load that id
/// as a layout (database type `0x23`); failure skips all notes. Otherwise create
/// each note in table order and release the layout after the loop.
///
/// Without this loop the map page has an image, a dot and no town on it; this is the caller of
/// the table and the placement rule.
///
/// Returns the notes it created, in [`notes`] order. An empty vector is one of the client's
/// two skip arms: the map image carries neither attribute, or the layout will not load.
pub fn create_map_notes(ui: &mut UiSystem, map: ElemHandle, profile: EraId) -> Vec<ElemHandle> {
    let Some(element) = crate::bind::attr_enum(ui, map, attr::NOTE_ELEMENT_ENUM) else {
        return Vec::new();
    };
    let Some(layout) = crate::bind::attr_data_id(ui, map, attr::NOTE_LAYOUT_DID) else {
        return Vec::new();
    };
    let element = ElementId(element);
    notes(profile)
        .filter_map(|n| add_map_note(ui, map, layout, element, n))
        .collect()
}

/// What the player can actually *do* on the map page. It is a short list.
///
/// The function has exactly two arms and no default:
///
/// ```text
/// visibility change (message 0x18) for the map element:
///     visible -> register for global message 3, then update the map
///     hidden  -> unregister from global message 3
/// mouse press (message 0x1C), when the local player passes the PSR privilege predicate
/// and the press lies inside the map-marker box:
///     Position p; p.objcell_id = lcoord_to_gid(
///         ((mx - x0) * 0x7FF) / (x1 - x0),
///         ((y0 - my) * 0x7FF) / (y1 - y0) + 0x7FF);
///     p.origin = (10, 10, 0); set_heading(0)
///     request privileged teleport with an empty argument and destination p
/// ```
///
/// So:
///
/// * **there is no zoom, no scroll and no pan.** The page has no such message arm, the map image has no
///   scroll attributes, and the image is one 257x267 blit ([`graphic::MAP_IMAGE`]).
/// * **there is no "place your own marker".** The only markers are the profile-selected notes and
///   the two icons; nothing writes a note at run time.
/// * **the hover behaviour is the note's own, not the page's**: the tooltip [`add_map_note`] sets,
///   and the rollover frame the button element's mouse-over handler raises through
///   its state update. `MapPanel` itself has no hover arm. See [`NOTE_REST_STATE`] —
///   a note the pointer is not on is **unframed**.
/// * **the only click behaviour is an advocate teleport, and it is gated on the
///   PSR privilege check.** It is deliberately *not* wired here: it is an admin command and this
///   build has no advocate-command producer. Note the inverse transform above divides by `x1 - x0`, **not** `x1 - x0 + 1` — it is
///   not the exact inverse of [`place_marker_on_map`], which is retail's own asymmetry and is
///   recorded here so nobody "fixes" one of them into the other.
pub const MAP_INTERACTION: [&str; 4] = [
    "0x18 visibility: register/unregister global message 3, and Update at once on show",
    "0x1C press: advocate teleport, PSR players only — not wired",
    "hover: the note's own tooltip and its rollover frame (state 1 -> 2), not the page's",
    "no zoom, no pan, no player-placed markers",
];

/// Place a marker element on the map at `(ew, ns)`, **as retail computes it**.
///
/// ```text
///   area_w = x1 - x0 + 1
///   x = x0 - w/2 + trunc((ew*10 + 1024) * area_w / 2048)
///   y = y0 - h/2 + trunc((2047 - (ns*10 + 1024)) * area_h / 2048)
/// ```
///
/// `w/2` is the element's width halved (signed), and the truncation is toward zero.
///
/// The `*10`, the `+1024` bias, the `/2048` scaling by the marker area's own width and the Y flip
/// are all load-bearing; `x0 - round(ew) - w/2` is a tempting and wrong reading. The landscape is 2048 tenths of a coordinate unit across (`lcoord_to_gid` rejects at
/// `0x7F8`), so the bias maps `[-102.4, +102.4]` onto the marker box, and `2047 -` is what makes
/// north up on a screen whose `+Y` is down.
#[must_use]
pub fn place_marker_on_map(area: MarkerArea, ew: f32, ns: f32, size: (i32, i32)) -> (i32, i32) {
    // The client does this in doubles: the coordinate arrives as a `double` from the player
    // coordinates query and all four constants are doubles. The operand order below is the
    // client's order, not an algebraic rearrangement of it -- `(2047 - biased) * extent`
    // and `2047 * extent - biased * extent` are not the same double.
    //
    // The client truncates toward zero after multiplying by `-1/2048` and then subtracts,
    // so `- trunc(-V)` is `+ trunc(V)` exactly; `to_i32_f64` is that truncation.
    // Deliberately **not** `mul_add`: the client multiplies then adds, which rounds twice.
    #[allow(clippy::suboptimal_flops)] // LINT-OK: two separate roundings, as retail does.
    let biased = |v: f32| f64::from(v) * 10.0 + 1024.0;
    let aw = f64::from(area.x1 - area.x0 + 1);
    let ah = f64::from(area.y1 - area.y0 + 1);
    let x = area.x0 - size.0 / 2 + to_i32_f64(biased(ew) * aw / 2048.0);
    let y = area.y0 - size.1 / 2 + to_i32_f64((2047.0 - biased(ns)) * ah / 2048.0);
    (x, y)
}

/// The house marker's coordinate transform: it is `− 1024`, not
/// `− 0x20`.
///
/// ```text
///   (l - 1024) * 0.1 + 0.5
/// ```
///
/// `− 0x20` would be the literal mis-scaled by 32 — the same mistake
/// `hud::player_coords` already records for the player-coordinates query, which performs the
/// identical transform on the same two lcoords.
#[must_use]
pub fn house_marker_coords(lx: i32, ly: i32) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)] // both are 0..0x7F8; exact in f32
    {
        (
            ((lx - 1024) as f32) * 0.1 + 0.5,
            ((ly - 1024) as f32) * 0.1 + 0.5,
        )
    }
}

/// The map panel's update is throttled: the next update is due at `now + 5.0`.
pub const UPDATE_INTERVAL_SECONDS: f32 = 5.0;

/// The two fixed prefixes the map update formats the game time with.
///
/// The client copies `"Date: "` and `"Time: "` into two buffers whose tails the game-time
/// string query then fills.
pub const DATE_PREFIX: &str = "Date: ";
/// See [`DATE_PREFIX`].
pub const TIME_PREFIX: &str = "Time: ";

/// The two buffer lengths and the date-and-time formatter, re-exported from
/// [`dereth_client_contract::panels::map`]: the world state's calendar clock formats through the
/// same function.
pub use dereth_client_contract::panels::map::{
    date_time_strings, DATE_BUFFER_LEN, TIME_BUFFER_LEN,
};

/// The client's first block: the two prefixed buffers joined by
/// `sprintf(out, "%s\n%s", dateBuf, timeBuf)`.
///
/// **This block is gated on the date/time text existing and nothing else.** It is *not* under
/// the is-outside test — that guards only the coordinate text
/// and the player icon, and the date keeps updating in a dungeon. See [`OUTDOORS_GATES`].
///
/// `None` is "no current game time", where the client writes a
/// single space into each buffer *after* the prefix rather than clearing the element.
#[must_use]
pub fn date_time_text(strings: Option<(&str, &str)>) -> String {
    let (date, time) = strings.unwrap_or((" ", " "));
    format!("{DATE_PREFIX}{date}\n{TIME_PREFIX}{time}")
}

/// Two of the three map-update blocks require the player to be outside.
///
/// ```text
///   no date/time text               -> skip the date (no outside test)
///   no coordinate text              -> skip
///   no player-location icon         -> skip (BOTH required)
///   player outside                  -> coords + marker
///           else: coordinate text set to "" and the player-location icon hidden
///   house-position validity         -> the house marker's own gate, not the outside test
/// ```
pub const OUTDOORS_GATES: [&str; 3] = [
    "date: never gated",
    "coordinates: outdoors only",
    "player marker: outdoors only",
];

#[cfg(test)]
mod tests {
    use super::*;
    fn n(x: i32, y: i32, w: i32, h: i32, name: &'static str) -> MapNote {
        MapNote { x, y, w, h, name }
    }

    /// Oracle: the 53-row location table dumped from the client. Row count, the first and last
    /// rows, and the alphabetical ordering are all checked.
    #[test]
    fn there_are_exactly_fifty_three_markers_in_the_dumped_order() {
        let map_notes: Vec<_> = notes(EraId::Eor).collect();
        assert_eq!(map_notes.len(), 53);
        assert_eq!(map_notes[0], n(178, 20, 11, 12, "Aerlinthe Island"));
        assert_eq!(map_notes[26], n(235, 220, 7, 6, "MacNiall's Freehold"));
        assert_eq!(map_notes[27], n(223, 203, 7, 6, "Mayoi"));
        assert_eq!(map_notes[52], n(123, 112, 7, 6, "Zaikhal"));
        // The table uses name order, and the two-column layout splits
        // 0..26 into the left column and 27..52 into the right.
        let names: Vec<&str> = map_notes.iter().map(|m| m.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "the location table is stored in name order");
        assert_eq!(LOCATION_ROLLOVER_INFO_SIZE, 20);
    }

    /// Oracle: the same table's W/H columns — the marker rectangles come in exactly four sizes,
    /// which is the tell that the table was read correctly rather than approximately.
    #[test]
    fn the_marker_rectangles_come_in_the_four_dumped_sizes() {
        use std::collections::BTreeMap;
        let mut hist: BTreeMap<(i32, i32), usize> = BTreeMap::new();
        for m in notes(EraId::Eor) {
            *hist.entry((m.w, m.h)).or_default() += 1;
        }
        assert_eq!(
            hist,
            BTreeMap::from([
                ((5, 5), 18),
                ((7, 6), 24),
                ((9, 8), 9),
                ((11, 12), 1), // Aerlinthe Island
                ((15, 16), 1), // Singularity Caul Island
            ])
        );
        // Every marker is inside the 256-pixel map image.
        for m in notes(EraId::Eor) {
            assert!(
                (0..256).contains(&m.x) && (0..256).contains(&m.y),
                "{}",
                m.name
            );
        }
    }

    /// Oracle: the retail formula quoted on
    /// [`place_marker_on_map`], evaluated here rather than restated.
    ///
    /// **This test replaces one that asserted `x0 - round(ew) - w/2`**, which is what the function
    /// used to compute: no `*10`, no `+1024` bias, no `/2048` scaling and no Y flip. That test was
    /// green for as long as the function has existed, because the oracle was the same mistake.
    #[test]
    fn a_marker_is_placed_by_scaling_the_biased_coordinate_across_the_marker_area() {
        // A 256-wide box over the full 2048-tenth landscape: one box pixel per eight tenths. Every
        // expected value below is worked by hand from the quoted formula -- one
        // truncation, applied to the whole product, which is *not* the same as truncating the
        // offset from the midpoint (that is what the first draft of this test got wrong).
        let area = MarkerArea {
            x0: 200,
            x1: 455,
            y0: 100,
            y1: 355,
        };

        // (0, 0): x = 200 + trunc(1024 * 256/2048) = 200 + 128.
        //         y = 100 + trunc((2047 - 1024) * 256/2048) = 100 + trunc(127.875).
        let (mid_x, mid_y) = (328, 227);
        assert_eq!(place_marker_on_map(area, 0.0, 0.0, (0, 0)), (mid_x, mid_y));

        // 33.8E / 42.2N -- the Holtburg reading `hud::player_coords` is calibrated against.
        // 33.8f32 is 33.7999992370605; *10 + 1024 = 1361.9999923706; /8 = 170.2499990 -> 170.
        // 42.2f32 is 42.2000007629395; 2047 - (422.0000076 + 1024) = 600.9999924; /8 -> 75.
        let (x, y) = place_marker_on_map(area, 33.8, 42.2, (0, 0));
        assert_eq!((x, y), (370, 175));
        assert!(
            x > mid_x && y < mid_y,
            "east is +X and north is -Y: ({x}, {y})"
        );

        // West and south go the other way. Truncation toward zero makes this deliberately
        // *asymmetric* about the midpoint: 85 and 180, not 128-42 and 127+52.
        assert_eq!(place_marker_on_map(area, -33.8, -42.2, (0, 0)), (285, 280));

        // The icon is centred on the point: half its width and half its height, both subtracted.
        assert_eq!(
            place_marker_on_map(area, 0.0, 0.0, (8, 6)),
            (mid_x - 4, mid_y - 3)
        );

        // The marker area's own extent is what scales it -- the half-width box halves the travel,
        // which is the part the old formula could not express at all because it never read x1/y1.
        let half = MarkerArea {
            x0: 200,
            x1: 327,
            y0: 100,
            y1: 227,
        };
        let (hx, _) = place_marker_on_map(half, 33.8, 0.0, (0, 0));
        assert_eq!(
            (hx - 200, x - 200),
            (85, 170),
            "x1 and y1 are read, not ignored"
        );
    }

    /// The client computes `(l - 1024) * 0.1 + 0.5`.
    ///
    /// **The subtrahend was `0x20` and it is `1024`** — a 992-unit error, i.e. the house marker was
    /// placed 99.2 coordinate units off in both axes. Unwired, so nobody could see it.
    #[test]
    fn the_house_marker_transform_subtracts_1024_not_0x20() {
        let (x, y) = house_marker_coords(1024, 1024);
        assert!(
            (x - 0.5).abs() < 1e-6 && (y - 0.5).abs() < 1e-6,
            "({x}, {y})"
        );
        let (x, _) = house_marker_coords(1024 + 10, 1024);
        assert!((x - 1.5).abs() < 1e-6, "{x}");
        // It is the same transform applies --
        // `hud::player_coords`' `(l - 1024) * 0.1 + 0.5`, repeated here because `dereth-ui-screens`
        // has no edge to `dereth-client`. Holtburg's block is lcoord (0xA9, 0xB4) and the retail
        // client reads 42.2N, 33.8E there (the live-run evidence).
        let (hx, hy) = house_marker_coords(0xA9, 0xB4);
        #[allow(clippy::cast_precision_loss)]
        let want = |l: i32| (l - 1024) as f32 * 0.1 + 0.5;
        assert!(
            (hx - want(0xA9)).abs() < 1e-6 && (hy - want(0xB4)).abs() < 1e-6,
            "({hx}, {hy})"
        );
        // The old `- 0x20` put it 99.2 units out, which is most of Dereth.
        #[allow(clippy::cast_precision_loss)]
        let old = (0xA9 - 0x20) as f32 * 0.1 + 0.5;
        assert!(
            (old - hx).abs() > 99.0,
            "the corrected constant is not the old one"
        );
    }

    /// Oracle: the date/time field ordering and its two guards.
    #[test]
    fn the_date_is_season_day_year_yearspec_and_the_time_is_the_named_time_of_day() {
        let (date, time) = date_time_strings("Morningthaw", 14, 10, "P.Y.", "Late Morning");
        assert_eq!(date, "Morningthaw 14, 10 P.Y.");
        assert_eq!(time, "Late Morning");
        assert_eq!(
            date_time_text(Some((&date, &time))),
            "Date: Morningthaw 14, 10 P.Y.\nTime: Late Morning"
        );

        // 29 characters fit, 30 do not, and the overflow is one space.
        let (_, t29) = date_time_strings("s", 1, 1, "y", &"x".repeat(29));
        assert_eq!(t29.len(), 29);
        let (_, t30) = date_time_strings("s", 1, 1, "y", &"x".repeat(30));
        assert_eq!(t30, " ");

        // The four lengths plus nine against 60.
        let (d_fit, _) = date_time_strings(&"s".repeat(47), 1, 1, "y", "t");
        assert_ne!(d_fit, " ", "47+1+1+1+9 = 59 fits");
        let (d_over, _) = date_time_strings(&"s".repeat(48), 1, 1, "y", "t");
        assert_eq!(d_over, " ", "48+1+1+1+9 = 60 does not");

        // `current_game_time == NULL` keeps the prefixes and writes one space after each.
        assert_eq!(date_time_text(None), "Date:  \nTime:  ");
    }

    /// The premise the owner flagged as memory, and retail's answer: **two of the three things he
    /// named are outdoors-only and the date is not.**
    #[test]
    fn only_the_coordinates_and_the_player_marker_are_gated_on_being_outside() {
        assert_eq!(OUTDOORS_GATES[0], "date: never gated");
        assert_eq!(OUTDOORS_GATES[1], "coordinates: outdoors only");
        assert_eq!(OUTDOORS_GATES[2], "player marker: outdoors only");
    }

    /// Map updates use a throttle and two fixed prefixes; interiors have no map.
    #[test]
    fn the_map_updates_every_five_seconds_and_has_no_indoor_mode() {
        assert_eq!(UPDATE_INTERVAL_SECONDS, 5.0);
        assert_eq!(DATE_PREFIX, "Date: ");
        assert_eq!(TIME_PREFIX, "Time: ");
        // The absence is the assertion: nothing in this module takes an "indoors" argument or a
        // cell id, because indoors the panel only hides the marker.
    }
}
