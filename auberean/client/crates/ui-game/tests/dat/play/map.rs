//! The map window on the retail layouts and the region of the September 2013 install: the world
//! map, the closer map, the markers, the regions' overviews.

use std::sync::{Arc, OnceLock};

use auberean_assets::region::{BlockMap, DetailMap, MapNotes as NotesRecord, Region};
use auberean_assets::{decode, Context, Decode};
use auberean_dat::locate::DatName;
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, Notice, Record, Request, View};
use auberean_ui_game::map::{
    self, gid_to_lcoord, Atlas, Clock, DetailRow, DetailTable, MapNote, MapNotes, Navigation,
};
use auberean_ui_game::toolbar::Toolbar;
use auberean_ui_game::world::{Thing, World};

use crate::common::records;

const ME: u64 = 0x2000_0000_0000_0001;
/// An outdoor cell near the middle of the world.
const CELL: u32 = 0x7F7F_0021;

/// What the map draws on, from the install's region (as the client's interface makes it).
fn atlas() -> Arc<Atlas> {
    static ATLAS: OnceLock<Arc<Atlas>> = OnceLock::new();
    ATLAS
        .get_or_init(|| {
            let dir = require_target_install();
            let portal = DatFile::open(&DatName::Portal.in_dir(&dir)).expect("the portal");
            let ctx = Context::from_portal(&portal).expect("the portal's property list");
            fn get<T: Decode>(p: &DatFile, ctx: &Context, id: u32) -> Option<T> {
                decode::<T>(id, &p.read(id).ok()?, ctx).ok()
            }
            let region: Region = get(&portal, &ctx, 0x0F00_0000).expect("the region");
            let int = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
            let detail = region
                .detail_maps
                .first()
                .and_then(|t| get::<DetailMap>(&portal, &ctx, t.record.0))
                .map(|d| {
                    let grid: BlockMap = get(&portal, &ctx, d.block_map.0).unwrap_or_default();
                    let rows: Vec<DetailRow> = d
                        .rows
                        .iter()
                        .map(|r| DetailRow {
                            value: r.value,
                            picture: r.picture.0,
                            x: int(r.x),
                            y: int(r.y),
                            height: int(r.height),
                            width: int(r.width),
                            notes: r.notes.clone(),
                        })
                        .collect();
                    let mut notes = Vec::new();
                    for id in rows.iter().flat_map(|r| r.notes.iter().copied()) {
                        if notes.iter().any(|(i, _)| *i == id) {
                            continue;
                        }
                        if let Some(n) = get::<NotesRecord>(&portal, &ctx, id) {
                            let all = n
                                .notes
                                .iter()
                                .map(|m| MapNote {
                                    text: m.text,
                                    icon: m.icon.0,
                                    x: int(m.x),
                                    y: int(m.y),
                                })
                                .collect();
                            notes.push((
                                id,
                                MapNotes {
                                    strings: n.strings.0,
                                    notes: all,
                                },
                            ));
                        }
                    }
                    DetailTable {
                        height: grid.height,
                        width: grid.width,
                        cells: grid.cells,
                        rows,
                        notes,
                    }
                });
            let (wide, long) = (int(region.blocks_wide), int(region.blocks_long));
            Arc::new(Atlas {
                game_map: region.map.0,
                land: (wide << 3, long << 3),
                blocks: (wide, long),
                detail,
            })
        })
        .clone()
}

/// An instance id's high and low words, as a notice carries one.
#[allow(clippy::cast_possible_truncation)]
const fn hi(id: u64) -> u32 {
    (id >> 32) as u32
}

#[allow(clippy::cast_possible_truncation)]
const fn lo(id: u64) -> u32 {
    id as u32
}

fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

fn world(cell: u32) -> World {
    World {
        player: ME,
        cell,
        blocks: (255, 255),
        things: vec![Thing {
            id: ME,
            is_player: true,
            kind: 0x1F,
            name: "Me".to_owned(),
            ..Thing::default()
        }],
        ..World::default()
    }
}

/// The calendar's words for the test's date: the year's, a season's and a part of the day's
/// names are tokens of the calendar's table.
const CLOCK: Clock = Clock {
    year: 437,
    year_name: 0,
    day: 12,
    season: 0,
    part: 0,
};

struct Screen {
    ui: Ui,
    p: GamePlay,
    t: f64,
    w: World,
    atlas: Arc<Atlas>,
    shift: bool,
}

impl Screen {
    fn new() -> Self {
        let mut ui = Ui::new(records(), 1024, 768);
        let p = play(&mut ui);
        let mut s = Self {
            ui,
            p,
            t: 0.0,
            w: world(CELL),
            atlas: atlas(),
            shift: false,
        };
        s.tick(&[]);
        s
    }

    fn tick(&mut self, notices: &[Notice]) -> Vec<Request> {
        self.t += 0.05;
        self.ui.use_time(self.t);
        let mut out = Vec::new();
        let view = View {
            world: Some(&self.w),
            atlas: Some(&self.atlas),
            clock: Some(CLOCK),
            shift: self.shift,
            ..View::default()
        };
        self.p.tick(&mut self.ui, &view, notices, &mut out);
        out
    }

    fn el(&self, name: &str) -> Handle {
        self.ui
            .element(Token::of(name))
            .unwrap_or_else(|| panic!("{name} made"))
    }

    fn click_at(&mut self, x: i32, y: i32, button: u32) {
        self.ui.pointer_move(x, y);
        self.tick(&[]);
        for down in [true, false] {
            self.ui.pointer_button(button, down);
            self.tick(&[]);
        }
    }

    fn click(&mut self, h: Handle) {
        let r = self.ui.screen_rect(h).expect("placed");
        self.click_at((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2, button_id::LEFT);
    }

    fn nav(&self) -> &Navigation {
        self.p
            .child::<Toolbar>()
            .and_then(|t| t.window("NavigationField"))
            .and_then(|c| c.child.as_any().downcast_ref::<Navigation>())
            .expect("the map")
    }

    fn open(&mut self) {
        let b = self.el("NavigationButton");
        self.click(b);
        assert!(
            self.ui.shown(self.el("NavigationField")),
            "the button shows the map"
        );
    }

    fn center(&self, h: Handle) -> (i32, i32) {
        let r = self.ui.screen_rect(h).expect("placed");
        ((r.x0 + r.x1 + 1) / 2, (r.y0 + r.y1 + 1) / 2)
    }
}

/// The map opens on the world map: the region's picture fitted inside its frame (as tall as the
/// frame, or as wide), the player's mark where the player stands on it, the player's map
/// coordinates and the game's date and time written.
///
/// Behaviour: ui.map.the-world-map-fits-the-regions-picture-and-marks-the-player
#[test]
fn the_world_map_fits_the_regions_picture_and_marks_the_player() {
    let mut s = Screen::new();
    s.open();
    let nav = s.nav();
    assert!(nav.on_macro, "the world map first");
    let map = s.el("MapField");
    let parent = s.el("MapParentField");
    let (m, f) = (
        s.ui.screen_rect(map).expect("placed"),
        s.ui.screen_rect(parent).expect("placed"),
    );
    assert!(m.width() > 1 && m.height() > 1, "{m:?}");
    assert!(
        m.width() == f.width() || m.height() == f.height(),
        "fitted: {m:?} in {f:?}"
    );
    let (lx, ly) = gid_to_lcoord(CELL, s.atlas.land).expect("a land cell");
    let (px, py, on) = nav.convert_landscape_to_pixel(&s.ui, lx, ly);
    assert!(on, "the player is on the world map");
    let mark = s.el("PlayerMarker");
    let (cx, cy) = s.center(mark);
    let at = s.ui.screen_rect(map).expect("placed");
    #[allow(clippy::cast_possible_truncation)]
    let want = (at.x0 + px as i32, at.y0 + py as i32);
    assert!(
        (cx - want.0).abs() <= 1 && (cy - want.1).abs() <= 1,
        "the mark at {:?}, the player at {want:?}",
        (cx, cy)
    );
    let coords = s.ui.text(s.el("CoordinatesText")).unwrap_or_default();
    let blocks = (s.atlas.blocks.1 as u32, s.atlas.blocks.0 as u32);
    assert_eq!(
        coords,
        auberean_ui_game::radar::map_coordinates(&s.ui, CELL, blocks)
    );
    let date = s.ui.text(s.el("DateText")).unwrap_or_default();
    assert!(date.contains("437"), "the year: {date:?}");
}

/// A click on the world map zooms in to the land around the place clicked: the closer map's
/// four pictures, the edges' buttons shown; an edge's button moves it nine cells (three with
/// Shift) each time it hears the press (as an element's press, as a button's, as its first
/// repeat); a right click goes back to the world map at the next tick.
///
/// Behaviour: ui.map.a-click-zooms-in-and-a-right-click-goes-back-out
#[test]
fn a_click_zooms_in_and_a_right_click_goes_back_out() {
    let mut s = Screen::new();
    s.open();
    let map = s.el("MapField");
    let mark = s.el("PlayerMarker");
    let (x, y) = s.center(mark);
    let (x, y) = (x, y + 12);
    let r = s.ui.screen_rect(map).expect("placed");
    let (lx, ly) = s
        .nav()
        .convert_pixel_to_landscape(x - r.x0, y - r.y0)
        .expect("on the land");
    s.click_at(x, y, button_id::LEFT);
    s.tick(&[]);
    let nav = s.nav();
    assert!(!nav.on_macro, "the closer map");
    let (x0, y0, w, h) = nav.view;
    assert_eq!((w, h), (128, 128));
    let side = s.atlas.land.0 - 128;
    assert_eq!(
        (x0, y0),
        ((lx - 64).clamp(0, side), (ly - 64).clamp(0, side)),
        "around the click"
    );
    assert!(nav
        .panels
        .iter()
        .flatten()
        .all(|p| s.ui.node(*p).is_some_and(|n| n.visible)));
    let right = s.el("NavigateRightButton");
    assert!(s.ui.shown(right), "the edges' buttons on the closer map");
    s.click(right);
    // A press is heard three times: as any element's press, as a button's, and as its first
    // repeat (the button repeats while held).
    assert_eq!(
        s.nav().view.0,
        (x0 + 27).min(s.atlas.land.0 - 128),
        "nine cells east, thrice"
    );
    s.shift = true;
    let up = s.el("NavigateUpButton");
    s.click(up);
    s.shift = false;
    assert_eq!(
        s.nav().view.1,
        (y0 + 9).min(s.atlas.land.0 - 128),
        "three with Shift, thrice"
    );
    let parent = s.el("MapParentField");
    let r = s.ui.screen_rect(parent).expect("placed");
    s.click_at((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2, button_id::RIGHT);
    s.tick(&[]);
    assert!(s.nav().on_macro, "back to the world map");
}

fn marker(kind: u32, id: u64, cell: u32, words: &str) -> Notice {
    Notice {
        native: map::notice::ADD_MARKER,
        about: Some(ME),
        words: vec![kind, hi(id), lo(id), cell, 0, 7],
        records: vec![
            None,
            None,
            None,
            None,
            None,
            Some(Record::Text(StringInfo::literal(words))),
        ],
    }
}

fn kind_ids(s: &Screen, kind: u32) -> Vec<u64> {
    s.nav()
        .markers
        .iter()
        .filter(|(k, _)| *k == kind)
        .flat_map(|(_, l)| l.iter().map(|m| m.id))
        .collect()
}

/// The scripts' markers by kind: one put on the map with its words and map coordinates in its
/// tooltip; another of the kind too near it not put; the kind's oldest dropped beyond ten; one
/// taken off by its object; a kind cleared.
///
/// Behaviour: ui.map.markers-come-and-go-by-kind
#[test]
fn markers_come_and_go_by_kind() {
    let mut s = Screen::new();
    s.open();
    s.tick(&[marker(1, 0x10, CELL, "A camp")]);
    assert_eq!(kind_ids(&s, 1), [0x10]);
    let m = s.nav().markers[0].1[0].elem.expect("its element");
    let tip = s.ui.element_tooltip(m).unwrap_or_default().to_owned();
    assert!(tip.contains("A camp"), "{tip:?}");
    assert!(
        tip.contains('N') || tip.contains('S'),
        "its coordinates: {tip:?}"
    );
    s.tick(&[marker(1, 0x11, CELL, "Too near")]);
    assert_eq!(kind_ids(&s, 1), [0x10], "too near the first");
    for i in 0..10_u32 {
        let cell = (0x40 + i * 8) << 24 | 0x40 << 16 | 0x21;
        s.tick(&[marker(1, 0x20 + u64::from(i), cell, "Far")]);
    }
    let ids = kind_ids(&s, 1);
    assert_eq!(ids.len(), 10, "{ids:?}");
    assert!(!ids.contains(&0x10), "the oldest dropped: {ids:?}");
    let remove = Notice {
        native: map::notice::REMOVE_MARKER,
        about: Some(ME),
        words: vec![1, 0, 0x25],
        ..Notice::default()
    };
    s.tick(&[remove]);
    assert!(!kind_ids(&s, 1).contains(&0x25));
    let clear = Notice {
        native: map::notice::CLEAR_MARKERS,
        about: Some(ME),
        words: vec![1],
        ..Notice::default()
    };
    s.tick(&[clear]);
    assert!(kind_ids(&s, 1).is_empty());
}

/// A region's button opens its overview map in that region's state; its close button takes it
/// down.
///
/// Behaviour: ui.map.the-regions-overviews-open-from-their-buttons
#[test]
fn the_regions_overviews_open_from_their_buttons() {
    let mut s = Screen::new();
    s.open();
    let b = s.el("NavigationZoomToOverviewOstethButton");
    s.click(b);
    let o = s.el("OverviewMap");
    assert!(s.ui.shown(o), "the overview shows");
    let label =
        s.ui.child_recursive(o, Token::of("Panel_Label_OverviewMap"))
            .expect("its title");
    assert_eq!(
        s.ui.node(label).map(|n| n.state),
        Some(Token::of("OstethMap"))
    );
    let close =
        s.ui.child_recursive(o, Token::of("OverviewMapCloseButton"))
            .expect("its close button");
    s.click(close);
    assert!(
        s.ui.element(Token::of("OverviewMap")).is_none(),
        "taken down"
    );
}
