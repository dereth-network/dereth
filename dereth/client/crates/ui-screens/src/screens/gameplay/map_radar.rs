//! The map and radar windows: their set-up and their per-frame updates.

use super::*;

impl GamePlayScreen {
    /// The geometry and the eight attribute-named children.
    /// The five child lookups and the four
    /// marker-area attributes.
    ///
    /// The five ids are looked up from the gameplay root rather than from a `MapPanel` handle
    /// because the map's panel page is layout data (attribute `0x10000029`) and nothing in this
    /// crate pairs a page id with a class. The five ids are unique in the shipped layout, so the
    /// recursive walk finds the same elements retail's set-up would.
    ///
    /// **The note loop.** Location notes are created from the layout named by attribute
    /// `0x48` and the element enum at `0x47`; both attributes are in the shipped layout
    /// (`0x47 = 0x100001F0`, `0x48 = 0x21000026`), the loop is
    /// [`crate::mapradar::map::create_map_notes`], and the notes are not cosmetic: their tooltips
    /// are the only thing on the page a pointer can do anything with.
    pub(super) fn map_post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        use crate::mapradar::map::{attr, child};
        let find = |ui: &UiSystem, id| ui.get_child_recursive(root, id);
        self.map.page = find(ui, crate::mapradar::map::MAP_PAGE);
        self.map.date_time_text = find(ui, child::DATE_TIME_TEXT);
        self.map.coordinate_text = find(ui, child::COORDINATE_TEXT);
        self.map.player_icon = find(ui, child::PLAYER_LOCATION_ICON);
        self.map.house_icon = find(ui, child::HOUSE_LOCATION_ICON);
        self.map.map_image = find(ui, child::MAP_IMAGE);
        self.map_notes.clear();
        self.map_profile = Some(dereth_primitives::EraId::Eor);
        if let Some(m) = self.map.map_image {
            let a = |name| attr_int(ui, m, name).unwrap_or(0);
            self.map.marker_area = crate::mapradar::map::MarkerArea {
                x0: a(attr::MARKER_AREA_X0),
                x1: a(attr::MARKER_AREA_X1),
                y0: a(attr::MARKER_AREA_Y0),
                y1: a(attr::MARKER_AREA_Y1),
            };
            // The set-up's own next statement: read enum attribute `0x47` and data-id attribute
            // `0x48`, load that layout, then add the map notes. It reads both
            // attributes off the map image, which is why it is inside this block and not beside it.
            self.map_notes =
                crate::mapradar::map::create_map_notes(ui, m, dereth_primitives::EraId::Eor);
        }
        // Hiding the house icon is not in retail's set-up; the icon comes up however the layout
        // left it and the first update decides. A zeroed house position is not valid, so the first
        // pass hides it.
        self.last_map_date_time = None;
        self.last_map_coords = None;
    }

    /// Rebuild location rollovers when world content changes, independently of feature flags.
    pub fn refresh_map_profile(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let profile = dereth_client_contract::panels::map::profile(view);
        if self.map_profile == Some(profile) {
            return false;
        }
        self.map_profile = Some(profile);
        for note in self.map_notes.drain(..) {
            ui.clear_tooltip(note);
            ui.remove_and_delete_root(note);
        }
        if let Some(map) = self.map.map_image {
            self.map_notes = crate::mapradar::map::create_map_notes(ui, map, profile);
        }
        true
    }

    /// The map panel's update, throttled by the caller.
    ///
    /// Returns whether anything was written, the same contract [`Self::update_radar`] has.
    ///
    /// The three blocks, and their three *different* gates — not all three "outdoors only":
    ///
    /// 1. **Date/time**, gated on the date/time text existing and nothing else. Being outside is
    ///    not consulted; the date keeps ticking in a dungeon.
    /// 2. **Coordinates and the player icon**, gated on both the coordinate text and the player
    ///    icon existing, and then on being outside. Indoors the text is set to the empty string —
    ///    **set, not hidden**, which is where this differs from the radar's strip — and the icon
    ///    is hidden. Note the **and**: a layout missing either element suppresses
    ///    both.
    /// 3. **The house icon**, gated on the house position being valid, which is
    ///    not being outside either. Left inert here: `0x0225 HouseData` is never received by this
    ///    client, so there is no valid position to place and the hide arm is the whole of it.
    ///
    /// `place_marker_on_map` ends by making the element it moved visible.
    /// The map panel's global-message handler's whole body bar the call: on global message 3,
    /// update when the next-update time is at or before now.
    ///
    /// Separate from [`Self::update_map`] because that is where retail's split is: the guard reads
    /// the next-update time in the listener, and `Update`'s *first* line is what advances it.
    /// It starts at zero, so the first tick after construction always runs.
    pub fn map_update_due(&self, now: f64) -> bool {
        self.next_map_update <= now
    }

    /// See [`Self::map_update_due`] for the caller's guard.
    pub fn update_map(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        use crate::mapradar::map::{date_time_text, place_marker_on_map, UPDATE_INTERVAL_SECONDS};
        // Next update = now + 5.0 — the client, the first thing
        // `Update` does, before it has looked at a single element.
        self.next_map_update = ui.now.0 + f64::from(UPDATE_INTERVAL_SECONDS);
        let mut wrote = self.refresh_map_profile(ui, view);

        // 1. The date. Ungated.
        if let Some(h) = self.map.date_time_text {
            let strings = view.game_date_time();
            let text = date_time_text(strings.as_ref().map(|(d, t)| (d.as_str(), t.as_str())));
            if self.last_map_date_time.as_deref() != Some(text.as_str()) {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&text);
                    self.last_map_date_time = Some(text);
                    wrote = true;
                }
            }
        }

        // 2. The coordinates and the green circle. Both elements required, then being outside.
        if let (Some(text_h), Some(icon_h)) = (self.map.coordinate_text, self.map.player_icon) {
            // `player_outside` and `player_coords` are one question here: the cell the
            // second rejects is exactly the cell the first calls inside (`gid_to_lcoord` bails on
            // `(id & 0xFFFF) >= 0x100`, which is the outside test itself).
            let coords = view
                .player_outside()
                .then(|| view.player_coords())
                .flatten();
            let text = match coords {
                // `"%.1f%s, %.1f%s"` with `|ns|`, the NS suffix, `|ew|` and the EW suffix -- the
                // same string the radar's strip shows, and
                // the suffix is empty, not "N"/"E", when the component is exactly zero.
                Some((ns, ew)) => {
                    let c = crate::mapradar::radar::update_coordinates((ns, ew));
                    c.combined
                }
                // The empty string, then the text write.
                None => String::new(),
            };
            if self.last_map_coords.as_deref() != Some(text.as_str()) {
                if let Some(t) = ui.text_element_mut(text_h) {
                    t.set_text(&text);
                    self.last_map_coords = Some(text);
                    wrote = true;
                }
            }
            match coords {
                Some((ns, ew)) => {
                    let b = element_box(ui, icon_h);
                    // The width read and the height read on the Y pair are both **`x1 - x0 + 1`**,
                    // an inclusive box. Passing `(x1 - x0, y1 - y0)` would be one short in both
                    // axes, and `place_marker_on_map` halves them. On the shipped 17x16 icon
                    // `0x100001ED` the X error cancels (17/2 == 16/2 == 8) and the Y one does not
                    // (16/2 = 8, 15/2 = 7), so **the green circle would sit one pixel below where
                    // retail puts it** in every position on the map.
                    let size = (b.width(), b.height());
                    let (x, y) = place_marker_on_map(self.map.marker_area, ew, ns, size);
                    if (b.x0, b.y0) != (x, y) {
                        ui.move_to(icon_h, x, y);
                        wrote = true;
                    }
                    // The make-visible half of `place_marker_on_map`'s tail.
                    if !ui.node(icon_h).is_some_and(|n| n.region.flags.visible) {
                        ui.set_visible(icon_h, true);
                        wrote = true;
                    }
                }
                None => {
                    if ui.node(icon_h).is_some_and(|n| n.region.flags.visible) {
                        ui.set_visible(icon_h, false);
                        wrote = true;
                    }
                }
            }
        }

        // 3. The house icon. No `HouseData` reaches this client, so the house position is never
        // valid and
        // this is the hide arm every time.
        if let Some(h) = self.map.house_icon {
            if ui.node(h).is_some_and(|n| n.region.flags.visible) {
                ui.set_visible(h, false);
                wrote = true;
            }
        }
        wrote
    }

    pub(super) fn radar_post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        use crate::mapradar::radar::{token_magnitude, Compass};
        let Some(radar) = ui.get_child_recursive(root, window::RADAR) else {
            return;
        };

        // The radar radius, and the centre point.
        //
        // **The earlier description reads as three separate attributes, but it is one.**
        // `0x1000002E` is a **`Struct`** whose two integer members are named `0x1000002F` and
        // `0x10000030` — which is what "a `Vector2` assembled from three attribute reads" is
        // describing: an attribute read on the outer name and then one read per member. In the
        // shipped `classic_radar` the struct is `{0x1000002F: 60, 0x10000030: 60}` and the radius
        // is 50, so reading `0x1000002E` as a float yields nothing and the centre lands on `(0, 0)`
        // — which puts every compass token off the top-left corner of the screen. [read off the
        // live merged property set] The window id, enum attribute `0x1000007e` — the set-up's very
        // first read after the base set-up, and the key every per-window chat option row is filed
        // under. Without it the radar has no window id and cannot persist a drag.
        self.radar.window_id = attr_enum(ui, radar, 0x1000_007E)
            .or_else(|| attr_int(ui, radar, 0x1000_007E).map(|v| u32::try_from(v).unwrap_or(0)))
            .unwrap_or(0);
        self.radar.radius = attr_int(ui, radar, 0x1000_002D).unwrap_or(0);
        #[allow(clippy::cast_precision_loss)] // pixel coordinates inside an 800x600 display
        let m = |inner: u32| {
            crate::bind::attr_struct_int(ui, radar, 0x1000_002E, inner).unwrap_or(0) as f32
        };
        self.radar.center = (m(0x1000_002F), m(0x1000_0030));

        // Each attribute holds a **child element id**; the child is what gets cached.
        let by_attr = |a: u32| -> Option<ElemHandle> {
            let id = attr_enum(ui, radar, a)
                .or_else(|| attr_int(ui, radar, a).map(|v| u32::try_from(v).unwrap_or(0)))?;
            ui.get_child_recursive(radar, ElementId(id))
        };
        self.radar.north = by_attr(Compass::North.attribute());
        self.radar.south = by_attr(Compass::South.attribute());
        self.radar.east = by_attr(Compass::East.attribute());
        self.radar.west = by_attr(Compass::West.attribute());
        self.radar.coordinate_container = by_attr(0x1000_0035);
        self.radar.combined_coords = by_attr(0x1000_0036);
        self.radar.x_coord = by_attr(0x1000_0037);
        self.radar.y_coord = by_attr(0x1000_0038);

        // Each token's magnitude = |centre point - token centre|, recorded once.
        let center = self.radar.center;
        for (i, which) in Compass::ALL.into_iter().enumerate() {
            let b = self.radar_token(which).map(|h| element_box(ui, h));
            self.radar.magnitudes[i] = b.map_or(0.0, |b| token_magnitude(center, b));
        }

        // "`0x100006A3` -> the drag button (**hidden at init**)".
        if let Some(h) = ui.get_child_recursive(radar, crate::mapradar::radar::child::DRAG_BUTTON) {
            ui.set_visible(h, false);
        }

        // **The padlock.** The radar update ends by giving the lock-UI button its media for the
        // *current* lock-UI flag — `0x10000063` when locked, `0x10000064` when not. Doing that swap
        // in [`Self::cascade_lock`] only would mean that until the player toggled the lock the
        // button had never been told which of its two states to draw and the upper-left of the ring
        // came up empty. Retail shows it there from the first frame.
        //
        // This button looks like "a green G (range) button". It is neither green-G nor a range
        // control: it is the **lock-UI padlock**, element `0x10000619`, and its click
        // handler (the element-message handler, message `0x19`) flips
        // the lock-UI flag and broadcasts global `0x0D`. The radar's range is not a button
        // at all — the radar's per-frame step reads it from the player system's radar radius
        // every 25 ms, which answers 75 outdoors and 25 indoors with nothing for the player to
        // press. See [`crate::mapradar::radar::radar_range`].
        self.sync_lock_button(ui, radar);
    }

    /// Finish radar initialization or a lock-status update: give
    /// the lock-UI button the media of the lock state the player module is actually in.
    ///
    /// It is a **state change**, not an attribute write: the two ids are keys in the button's own
    /// state table, each holding the `MediaDesc` for one padlock image, and the element's
    /// default state is 0 with no base media — so a button that is never given a state has no
    /// picture. See [`crate::mapradar::radar::lock_state`].
    pub(super) fn sync_lock_button(&self, ui: &mut UiSystem, radar: ElemHandle) {
        let Some(h) = ui.get_child_recursive(radar, crate::mapradar::radar::child::LOCK_BUTTON)
        else {
            return;
        };
        let state = if self.locked {
            crate::mapradar::radar::lock_state::LOCKED
        } else {
            crate::mapradar::radar::lock_state::UNLOCKED
        };
        ui.set_state(h, dereth_ui::StateId(state));
    }

    fn radar_token(&self, which: crate::mapradar::radar::Compass) -> Option<ElemHandle> {
        use crate::mapradar::radar::Compass;
        match which {
            Compass::North => self.radar.north,
            Compass::South => self.radar.south,
            Compass::East => self.radar.east,
            Compass::West => self.radar.west,
        }
    }

    /// The radar panel's coordinates update, the compass tokens update and the object draw.
    ///
    /// **The blips.** `draw_objects` writes them pixel by pixel into the radar's own surface, and a
    /// draw command that could only name a dat image could not carry them. `dereth_ui::UiDrawCmd`
    /// carries [`dereth_ui::UiFill`] — the fill-area primitive both the blips and the background
    /// erase are made of — so the shapes go straight onto the radar element's region.
    pub fn update_radar(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        use crate::mapradar::radar::{compass_token_position, update_coordinates, Compass};
        let mut wrote = false;

        // The radar's object draw, once per frame. The list is rebuilt every time rather than
        // diffed: the radar's per-frame step re-renders the whole blip field on its own 25 ms
        // gate and never keeps a per-object rectangle.
        //
        // **The whole regeneration is gated on there being a player** — the per-frame update does
        // its work only when the world view has a player, and it is that branch which ends by
        // marking the root dirty. With no player the radar is never marked dirty, so the child draw
        // never runs and neither the blips nor the centre cross are ever generated. Without this
        // gate the cross would appear in a scene the retail client would have left blank, which is
        // a state retail cannot reach but ours can (an offline `App` sits in `GamePlayScreen` with
        // an empty object stream).
        let live_radar = self
            .roots
            .first()
            .filter(|_| view.player().is_some())
            .and_then(|r| ui.get_child_recursive(*r, window::RADAR));
        if let Some(radar) = live_radar {
            let objects = view.radar_objects();
            // The object draw fetches the player's own weenie for the two PK-threat comparisons, so
            // this is **our** character and not merely the first
            // player in the list.
            let player = objects.iter().find(|o| o.is_self);
            let geom = crate::mapradar::radar::RadarGeometry {
                radius: self.radar.radius,
                center: self.radar.center,
            };
            let blips = crate::mapradar::radar::draw_objects(
                objects,
                player,
                geom,
                crate::mapradar::radar::radar_range(view.player_outside()),
                view.selection(),
                view.radar_blank(),
            );
            let mut fills: Vec<dereth_ui::UiFill> = blips
                .iter()
                .flat_map(crate::mapradar::radar::blip_fills)
                .collect();
            // **The player's own marker.** The object draw runs `draw_objects` and then lays the
            // green cross over the centre point, unconditionally and outside the blip loop — the
            // player is not in the radar's blip list, because the blip loop skips the player's own
            // id. So it is appended here, after the blips, in the same order the client issues its
            // fills.
            fills.extend(crate::mapradar::radar::center_marker_fills(geom));
            if let Some(n) = ui.node_mut(radar) {
                if n.region.surface_fills != fills {
                    n.region.surface_fills = fills;
                    wrote = true;
                }
            }
            let hit = crate::mapradar::radar::object_under_mouse(&blips, cursor_in(ui, radar))
                .and_then(|i| objects.get(i))
                .map(|o| o.id);
            let name = hit.and_then(|id| view.name(id)).map(ToString::to_string);
            wrote |= self.set_object_under_mouse(ui, radar, hit, name);
        }

        // The client's tail, which is the *other* half of the drag handle.
        // See [`RadarChildren::last_origin`] for why it is observed here rather than overridden.
        let any_radar = self
            .roots
            .first()
            .and_then(|r| ui.get_child_recursive(*r, window::RADAR));
        if let Some(b) = any_radar.and_then(|h| ui.node(h)).map(|n| n.region.box_) {
            let at = (b.x0, b.y0);
            let was = self.radar.last_origin.replace(at);
            if was.is_some() && was != Some(at) {
                self.persist_window_position(&mut ui.requests, at);
            }
        }

        // The coordinate read-out has **two** gates: the `CoordinatesOnRadar` option (option-word
        // bit 22) and
        // `player_coords` (a body in a cell). If either fails the text writes are skipped. The
        // tail runs on every update either way: it sets the four coordinate elements' visibility
        // to whether both gates passed, then shows one more element (the radar's lock button)
        // unconditionally.
        //
        // So the four coordinate elements are **down**, not blank, when either gate fails — which
        // is what keeps an empty strip off the screen between entering the world and the body
        // existing, and what a player who unticks *Display Coordinates on Radar* expects to see.
        //
        // `HudView::player_option` reads the live option word. The option is **default-on** (the
        // default option value answers true for ordinal 20), so the shipped profile shows the
        // strip.
        //
        // Before a `0x0013` lands, `HudView::player_option` must answer the *constructed* default
        // `true`, as retail does (its player module is a member, not a pointer): this gate is a
        // consumer of one of the sixteen default-on options, and answering `false` there would take
        // the strip down for a body in the world with no `0x0013`. `HudView::player_option` reads
        // the option word directly, which is what the check-box option control's value read does;
        // see its note.
        let coords = view
            .player_coords()
            .filter(|_| view.player_option(crate::view::PlayerOption::CoordinatesOnRadar));
        let shown = coords.is_some();
        for h in [
            self.radar.coordinate_container,
            self.radar.combined_coords,
            self.radar.x_coord,
            self.radar.y_coord,
        ]
        .into_iter()
        .flatten()
        {
            if ui.node(h).is_some_and(|n| n.region.flags.visible) != shown {
                ui.set_visible(h, shown);
                wrote = true;
            }
        }
        if let Some(coords) = coords {
            if self.last_coords != Some(coords) {
                self.last_coords = Some(coords);
                wrote = true;
                let c = update_coordinates(coords);
                let fields = [
                    (self.radar.combined_coords, c.combined),
                    (self.radar.y_coord, c.y_field),
                    (self.radar.x_coord, c.x_field),
                ];
                for (h, text) in fields {
                    let Some(h) = h else { continue };
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(&text);
                    }
                }
            }
        }

        let heading = view.player_heading();
        if self.last_heading != Some(heading) {
            self.last_heading = Some(heading);
            wrote = true;
            let center = self.radar.center;
            for (i, which) in Compass::ALL.into_iter().enumerate() {
                let Some(h) = self.radar_token(which) else {
                    continue;
                };
                let (x, y) =
                    compass_token_position(center, heading, self.radar.magnitudes[i], which);
                // `MoveTo` centres the token on the orbit point: the client subtracts half the
                // token's own width and height before moving it.
                let b = element_box(ui, h);
                ui.move_to(
                    h,
                    dereth_primitives::num::to_i32(x) - b.width() / 2,
                    dereth_primitives::num::to_i32(y) - b.height() / 2,
                );
            }
        }
        wrote
    }
}
