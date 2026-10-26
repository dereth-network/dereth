use super::*;
use dereth_primitives::num::to_i32;

mod housing;
use dereth_client_contract::panels::map::{self as shared_map, MapNote};
use dereth_presentation::coordinates::format_coordinate as coord;
use dereth_primitives::EraId;
mod minigame;
pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    match id {
        "link-status" => Some(Box::new(Link::default())),
        "map" | "house" | "map-house" => Some(Box::new(MapHouse {
            house: id == "house",
            queried: false,
            hover: None,
        })),
        "maintenance" => Some(Box::new(housing::Maintenance::default())),
        "game-center" => Some(Box::new(minigame::GameCenter::default())),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct Link {
    last_refresh: Option<f64>,
    sent: Option<f64>,
    return_count: u64,
    roundtrip: Option<f64>,
    loss: f32,
}
impl Panel for Link {
    fn id(&self) -> &'static str {
        "link-status"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = tiled(300, c.layout.side_height(), "06001398");
        header(&mut f, "Link Status");
        label(&mut f,rect(5,40,290,240),"The Link Indicator shows the current status of your connection to the game servers.\n\nGREEN = your link is good.\n\nYELLOW = no packets for at least 5 sec.\n\nRED = no packets for at least 20 sec.\n\nIf approximately forty seconds pass without receiving a packet, you will be disconnected from the server.","16-7");
        label(
            &mut f,
            rect(5, 260, 290, 20),
            format!("Packet loss for the last 10 sec.: {:.2}%", self.loss),
            "16-7",
        );
        label(
            &mut f,
            rect(5, 300, 290, 20),
            format!(
                "Roundtrip Ping time to Server: {} ms.",
                self.roundtrip
                    .map(|n| format!("{n:.0}"))
                    .unwrap_or("????".into())
            ),
            "16-7",
        );
        label(
            &mut f,
            rect(5, 320, 290, 20),
            "(Includes Internet latency)",
            "16-7",
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if matches!(e,ControlEvent::Activate(ref id) if id=="close") {
            return vec![PanelAction::Close];
        }
        if !matches!(e, ControlEvent::Tick) {
            return vec![];
        }
        let now = c.game.now();
        let count = c.game.ping_returns();
        if count != self.return_count {
            self.return_count = count;
            if let Some(sent) = self.sent.take() {
                self.roundtrip = Some((now - sent) * 1000.0);
            }
        }
        if self.last_refresh.is_some_and(|t| now - t < 10.0) {
            return vec![];
        }
        self.last_refresh = Some(now);
        self.loss = c.game.packet_loss_percent();
        if self.sent.is_some_and(|sent| now - sent >= 120.0) {
            self.sent = None;
            self.roundtrip = None;
        }
        if self.sent.is_none() {
            self.sent = Some(now);
            return request(UiRequest::RequestPing);
        }
        vec![]
    }
}

#[derive(Debug)]
struct MapHouse {
    house: bool,
    queried: bool,
    hover: Option<(EraId, MapNote)>,
}
/// The map's picture on the map page, from the map's origin: `(x, y, w, h)`.
pub const MAP_AREA: (i32, i32, i32, i32) = (22, 33, 257, 267);

/// Whether a point from the map's origin falls on the map's picture.
#[must_use]
pub fn on_map((x, y): (i32, i32)) -> bool {
    let (ax, ay, w, h) = MAP_AREA;
    (ax..ax + w).contains(&x) && (ay..ay + h).contains(&y)
}

fn marker(lx: i32, ly: i32) -> (i32, i32) {
    (lx * 245 / 2048 + 28, (2047 - ly) * 245 / 2048 + 44)
}

/// Where coordinates north and east fall on the map page's map, from its origin.
#[must_use]
pub fn map_marker(north: f64, east: f64) -> (i32, i32) {
    let lx = dereth_primitives::num::to_i32_f64(((east - 0.5) * 10.0 + 1024.0).round());
    let ly = dereth_primitives::num::to_i32_f64(((north - 0.5) * 10.0 + 1024.0).round());
    marker(lx, ly)
}

/// The coordinates north and east under the pixel `(x, y)` of the map page, from the map's
/// origin: read at the pixel's middle, so [`map_marker`] puts them back on that pixel. `None` off
/// the part of the picture that shows the world.
#[must_use]
pub fn map_coords(x: i32, y: i32) -> Option<(f64, f64)> {
    if !(28..273).contains(&x) || !(44..289).contains(&y) {
        return None;
    }
    // The landscape cell under the pixel's middle, as `marker` scales it: 2048 across 245.
    let lx = (f64::from(x - 28) + 0.5) * 2048.0 / 245.0;
    let ly = 2047.0 - (f64::from(y - 44) + 0.5) * 2048.0 / 245.0;
    Some(((ly - 1024.0) / 10.0 + 0.5, (lx - 1024.0) / 10.0 + 0.5))
}

/// A press of `button` on the map page at `(x, y)`, from the map's origin, told to the plugins
/// where it falls on the world.
fn map_press(x: i32, y: i32, button: shared_map::MapButton) -> Option<PanelAction> {
    let (ns, ew) = map_coords(x, y)?;
    Some(PanelAction::Host(HostAction::MapClick { ns, ew, button }))
}
impl Panel for MapHouse {
    fn id(&self) -> &'static str {
        "map-house"
    }
    fn map_origin(&self) -> Option<(i32, i32)> {
        // Under the page's two tabs.
        (!self.house).then_some((0, 25))
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = if self.house {
            housing::house(c)
        } else {
            let mut f = tiled(300, c.layout.side_height() - 25, "06001398");
            for (did, r) in [
                ("06001273", rect(0, 0, 300, 33)),
                ("06001270", rect(0, 300, 300, 32)),
                ("06001271", rect(0, 33, 22, 267)),
                ("06001272", rect(279, 33, 21, 267)),
                (
                    "0600127D",
                    rect(MAP_AREA.0, MAP_AREA.1, MAP_AREA.2, MAP_AREA.3),
                ),
            ] {
                f.image(did, r, false, false);
            }
            let (date, time) = c.game.game_date_time().unwrap_or((" ".into(), " ".into()));
            label_color(
                &mut f,
                rect(35, 4, 245, 14),
                format!("Date: {date}"),
                "14-5",
                0xff080808,
            );
            label_color(
                &mut f,
                rect(35, 18, 245, 14),
                format!("Time: {time}"),
                "14-5",
                0xff080808,
            );
            if let Some((_, note)) = self
                .hover
                .filter(|(profile, _)| *profile == shared_map::profile(c.game))
            {
                label_color(
                    &mut f,
                    rect(120, 300, 140, 20),
                    note.name,
                    "15-6",
                    0xff080808,
                );
            }
            if c.game.player_outside() {
                if let Some((north, east)) = c.game.player_coords() {
                    let lx = to_i32(((east - 0.5) * 10.0 + 1024.0).round());
                    let ly = to_i32(((north - 0.5) * 10.0 + 1024.0).round());
                    let (x, y) = marker(lx, ly);
                    f.image("0600127C", rect(x - 8, y - 8, 17, 16), false, true);
                    label_color(
                        &mut f,
                        rect(30, 300, 90, 20),
                        format!("{}, {}", coord(north, 'N', 'S'), coord(east, 'E', 'W')),
                        "15-6",
                        0xff080808,
                    );
                }
            }
            if let Some((x, y)) = c.game.house_data().and_then(|h| h.location) {
                let (x, y) = marker(x, y);
                f.image("06002279", rect(x - 4, y - 4, 8, 8), false, true);
            }
            f
        };
        f = translated(f, 25, c.layout.side_height());
        for (id, text, x) in [("map", "Map", 0), ("house", "House", 138)] {
            let active = self.house == (id == "house");
            let c = f.button(id, rect(x, 0, 138, 25), text, true);
            c.images = Some(
                [
                    // The tab of the page on show is held down.
                    if active { "060022BC" } else { "060022BD" },
                    "060022BC",
                    "060022BD",
                ]
                .map(String::from),
            );
            c.keyed = false;
        }
        image_button(
            &mut f,
            "close",
            rect(276, 0, 24, 25),
            [0x06001283, 0x06001282, 0x06001283],
            true,
        );
        f
    }
    fn input(
        &mut self,
        input: &crate::widgets::Input,
        _context: &Context<'_>,
    ) -> Option<Vec<PanelAction>> {
        // A right press on the map is told to the plugins; the map does nothing else with it.
        match *input {
            crate::widgets::Input::RightClick { x, y } if !self.house => {
                map_press(x, y - 25, shared_map::MapButton::Right).map(|a| vec![a])
            }
            _ => None,
        }
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if let ControlEvent::Pointer { x, y, pressed } = e {
            let y = y - 25;
            let mut out = Vec::new();
            if !self.house {
                let profile = shared_map::profile(c.game);
                self.hover = map_region(x, y, profile).map(|note| (profile, note));
                if pressed && c.map_teleport_allowed {
                    if let Some((lx, ly)) = map_destination(x, y) {
                        out.push(PanelAction::Host(HostAction::MapTeleport { lx, ly }));
                    }
                }
                // Every press on the map is told to the plugins as well.
                if pressed {
                    out.extend(map_press(x, y, shared_map::MapButton::Left));
                }
            }
            return out;
        }
        if let ControlEvent::Activate(id) = e {
            match id.as_str() {
                "map" => self.house = false,
                "house" => self.house = true,
                "close" => return vec![PanelAction::Close],
                _ => {}
            }
        }
        if self.house && !self.queried && c.game.house_data().is_none() {
            self.queried = true;
            return vec![PanelAction::Host(HostAction::QueryHouse)];
        }
        vec![]
    }
}

fn map_region(x: i32, y: i32, profile: EraId) -> Option<MapNote> {
    shared_map::notes(profile)
        .rev()
        .find(|note| note.contains(x - 22, y - 33))
}
fn map_destination(x: i32, y: i32) -> Option<(u32, u32)> {
    if !(28..273).contains(&x) || !(44..289).contains(&y) {
        None
    } else {
        Some((
            ((x - 28) * 2047 / 244) as u32,
            (2047 - (y - 44) * 2047 / 244) as u32,
        ))
    }
}
#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn map_click_bounds_and_integer_inverse() {
        assert_eq!(map_destination(28, 44), Some((0, 2047)));
        assert_eq!(map_destination(272, 288), Some((2047, 0)));
        for (x, y) in [(27, 44), (273, 44), (28, 43), (28, 289)] {
            assert_eq!(map_destination(x, y), None);
        }
    }
    #[test]
    fn map_hover_has_profile_regions_and_excludes_right_bottom_edges() {
        assert_eq!(shared_map::notes(EraId::Infiltration).count(), 50);
        let note = map_region(200, 53, EraId::Infiltration).unwrap();
        assert_eq!(note.name, "Aerlinthe Island");
        assert_eq!(map_region(211, 65, EraId::Infiltration), None);
    }
    #[test]
    fn map_markers_follow_integer_projection_and_inverted_north() {
        assert_eq!(marker(0, 2047), (28, 44));
        assert_eq!(marker(2047, 0), (272, 288));
    }
    /// Every pixel of the map's world reads back the coordinates a marker puts on that pixel;
    /// Holtburg's reads back Holtburg to within a pixel, and the frame round the map reads nothing.
    #[test]
    fn a_pixel_of_the_map_reads_back_the_coordinates_its_marker_is_placed_from() {
        for y in 44..289 {
            for x in 28..273 {
                let (north, east) = map_coords(x, y).expect("on the map");
                assert_eq!(map_marker(north, east), (x, y), "{north}, {east}");
            }
        }
        let (x, y) = map_marker(42.2, 33.8);
        let (north, east) = map_coords(x, y).expect("Holtburg is on the map");
        // A pixel is 2048 / 245 landscape cells across, under 0.85 of a unit.
        assert!(
            (north - 42.2).abs() < 0.85 && (east - 33.8).abs() < 0.85,
            "{north}, {east}"
        );
        for (x, y) in [(27, 100), (273, 100), (100, 43), (100, 289)] {
            assert_eq!(map_coords(x, y), None, "({x}, {y})");
        }
    }
    #[test]
    fn zero_coordinate_has_no_hemisphere() {
        assert_eq!(coord(0.0, 'N', 'S'), "0.0");
        assert_eq!(coord(-12.25, 'N', 'S'), "12.2S");
    }
}
