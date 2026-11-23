//! World-profile map locations shared by map views, and a press on the map as every interface
//! reports it.

/// Which of the pointer's buttons pressed the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapButton {
    Left,
    Right,
    Middle,
}

impl MapButton {
    /// The button's name: `left`, `right` or `middle`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }
}

/// A press on the world map: the map coordinates under the pointer (north and east positive, as
/// the radar shows them), the button, and which of Ctrl, Shift and Alt were held.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapClick {
    pub ns: f64,
    pub ew: f64,
    pub button: MapButton,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// What a place on the world map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapPlaceKind {
    /// A town the game's map marks with a town's icon.
    Town,
    /// A smaller place the game's map names, or a region.
    Landmark,
    Cottage,
    Villa,
    Mansion,
    /// A lifestone the player has seen.
    Lifestone,
    /// A portal the player has seen.
    Portal,
    /// Where a portal the player used into a dungeon stands, named for the dungeon.
    Dungeon,
    /// A vendor the player has seen.
    Vendor,
    /// A person the player has seen who is no vendor and cannot be attacked.
    Npc,
}

impl MapPlaceKind {
    /// The kind as one word: `town`, `landmark`, `cottage`, `villa`, `mansion`, `lifestone`,
    /// `portal`, `dungeon`, `vendor` or `npc`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Town => "town",
            Self::Landmark => "landmark",
            Self::Cottage => "cottage",
            Self::Villa => "villa",
            Self::Mansion => "mansion",
            Self::Lifestone => "lifestone",
            Self::Portal => "portal",
            Self::Dungeon => "dungeon",
            Self::Vendor => "vendor",
            Self::Npc => "npc",
        }
    }
}

/// A place the world map marks: what it is, its name and where it stands, in map coordinates
/// (north and east positive).
#[derive(Debug, Clone, PartialEq)]
pub struct MapPlace {
    pub kind: MapPlaceKind,
    pub name: String,
    /// Map coordinates as the world map measures them: meters from the world's corner over 240,
    /// less 101.95.
    pub ns: f64,
    pub ew: f64,
}

/// A square of the world a map shows, in map coordinates (north and east positive): its west and
/// south edges and its side.
///
/// A world whose land lies in part of the world is shown framed to its land, the same square on
/// every map: the Horizon map frames it fully zoomed out, and the map pages, which show the whole
/// world on one picture, stretch it over the part of the picture that shows the world
/// ([`Self::stretched`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapFrame {
    pub west: f64,
    pub south: f64,
    pub side: f64,
}

// A frame is worked out from whole numbers of pixels and never holds a NaN.
impl Eq for MapFrame {}

impl MapFrame {
    /// Where `(ns, ew)` is drawn once this frame is stretched over `whole`, the square a map page's
    /// picture shows: the frame's corners on `whole`'s.
    #[must_use]
    pub fn stretched(self, whole: Self, (ns, ew): (f64, f64)) -> (f64, f64) {
        let k = whole.side / self.side;
        (
            whole.south + (ns - self.south) * k,
            whole.west + (ew - self.west) * k,
        )
    }

    /// The map coordinates drawn at `(ns, ew)` once this frame is stretched over `whole`: the
    /// inverse of [`Self::stretched`].
    #[must_use]
    pub fn unstretched(self, whole: Self, (ns, ew): (f64, f64)) -> (f64, f64) {
        let k = self.side / whole.side;
        (
            self.south + (ns - whole.south) * k,
            self.west + (ew - whole.west) * k,
        )
    }

    /// Whether `(ns, ew)` is inside the frame.
    #[must_use]
    pub fn contains(self, (ns, ew): (f64, f64)) -> bool {
        (self.south..=self.south + self.side).contains(&ns)
            && (self.west..=self.west + self.side).contains(&ew)
    }
}

/// The square a world's map frames on `view`'s world, where it shows only part of the world.
#[must_use]
pub fn map_frame(view: &dyn crate::GameView) -> Option<MapFrame> {
    view.era().and_then(|era| era.map_frame)
}

/// A named rectangle in the map image's own pixel space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapNote {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub name: &'static str,
}
impl MapNote {
    /// The right and bottom edges are outside the rollover rectangle.
    #[must_use]
    pub const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}
struct Region {
    note: MapNote,
    earlier_x: Option<i32>,
}
const fn n(x: i32, y: i32, w: i32, h: i32, name: &'static str) -> Region {
    Region {
        note: MapNote { x, y, w, h, name },
        earlier_x: Some(x),
    }
}
const fn later(x: i32, y: i32, w: i32, h: i32, name: &'static str) -> Region {
    Region {
        note: MapNote { x, y, w, h, name },
        earlier_x: None,
    }
}
const fn shifted(x: i32, earlier_x: i32, y: i32, w: i32, h: i32, name: &'static str) -> Region {
    Region {
        note: MapNote { x, y, w, h, name },
        earlier_x: Some(earlier_x),
    }
}

/// The connected world chooses content; a view without a world uses the default profile.
#[must_use]
pub fn profile(view: &dyn crate::GameView) -> dereth_primitives::EraId {
    view.era()
        .map_or(dereth_primitives::EraId::Eor, |era| era.era)
}

/// The towns and landmarks a world names itself, which its map names in place of the game's map's
/// notes. Two lists are equal when they are one list or hold the same places.
#[derive(Debug, Clone, Default)]
pub struct WorldPlaces(pub std::sync::Arc<[MapPlace]>);

impl PartialEq for WorldPlaces {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for WorldPlaces {}

/// What the map page names on a world: the game's map's notes of an era, or the world's own
/// places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageNames {
    Notes(dereth_primitives::EraId),
    Own(WorldPlaces),
}

/// What the map page names on `view`'s world: its own places where it names them, else the notes
/// of its [`profile`].
#[must_use]
pub fn page_names(view: &dyn crate::GameView) -> PageNames {
    match view.era().and_then(|era| era.own_places.clone()) {
        Some(own) => PageNames::Own(own),
        None => PageNames::Notes(profile(view)),
    }
}

/// A note for one of a world's own places on the map page: the rectangle of the map picture's own
/// pixels the pointer names it in, its name, and its kind, which the page marks it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnNote {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub name: String,
    pub kind: MapPlaceKind,
}

impl OwnNote {
    /// The right and bottom edges are outside the rollover rectangle, as a note's are.
    #[must_use]
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// The pixels a place of `kind` is marked with on the map page, a row a string: `Y` the rim, `N`
/// the inside, anything else left clear; the mark's size is its note's, and its edge is left clear
/// for the frame a note under the pointer draws there. A town is marked as the game's picture
/// marks its towns, in a note nine by eight as its largest towns' are; anything else with the
/// small cross the picture marks its lesser places with, in a note five by five as its smaller
/// places' are.
#[must_use]
#[rustfmt::skip]
pub const fn mark(kind: MapPlaceKind) -> &'static [&'static str] {
    match kind {
        MapPlaceKind::Town => &[
            ".........",
            "...YYY...",
            "..YYNYY..",
            ".YYNNNYY.",
            ".YNNNNNY.",
            ".YNYNYNY.",
            ".YYYYYYY.",
            ".........",
        ],
        _ => &[
            ".....",
            "..Y..",
            ".YYY.",
            "..Y..",
            ".....",
        ],
    }
}

/// The color of a mark's rim, as the game's picture draws its places' rims.
pub const MARK_RIM: u32 = 0xFFD7_C315;
/// The color of a mark's inside.
pub const MARK_INSIDE: u32 = 0xFF13_0F3C;

/// A mark's pixels as runs along its rows: `(x, y, length, color)` from its top-left corner.
#[must_use]
pub fn mark_runs(kind: MapPlaceKind) -> Vec<(i32, i32, i32, u32)> {
    let mut runs = Vec::new();
    for (y, row) in (0..).zip(mark(kind)) {
        let mut x = 0;
        for chunk in row.as_bytes().chunk_by(|a, b| a == b) {
            let len = i32::try_from(chunk.len()).unwrap_or(0);
            let color = match chunk[0] {
                b'Y' => Some(MARK_RIM),
                b'N' => Some(MARK_INSIDE),
                _ => None,
            };
            if let Some(color) = color {
                runs.push((x, y, len, color));
            }
            x += len;
        }
    }
    runs
}

/// The notes of a world's own `places`, each the size of its kind's [`mark`] and centered on the
/// pixel `at` puts its map coordinates `(ns, ew)` on; a place the pixel is `None` for has none.
/// The towns come last, so that they are drawn over the rest and found first under the pointer;
/// otherwise in `places`' order.
pub fn own_notes(places: &[MapPlace], at: impl Fn(f64, f64) -> Option<(i32, i32)>) -> Vec<OwnNote> {
    let (towns, rest): (Vec<&MapPlace>, Vec<&MapPlace>) =
        places.iter().partition(|p| p.kind == MapPlaceKind::Town);
    rest.into_iter()
        .chain(towns)
        .filter_map(|p| {
            let rows = mark(p.kind);
            let h = i32::try_from(rows.len()).unwrap_or(0);
            let w = i32::try_from(rows.first().map_or(0, |r| r.len())).unwrap_or(0);
            let (x, y) = at(p.ns, p.ew)?;
            Some(OwnNote {
                x: x - w / 2,
                y: y - h / 2,
                w,
                h,
                name: p.name.clone(),
                kind: p.kind,
            })
        })
        .collect()
}

/// Locations in name order, independent of interface artwork.
pub fn notes(profile: dereth_primitives::EraId) -> impl DoubleEndedIterator<Item = MapNote> {
    REGIONS.iter().filter_map(move |region| {
        let mut note = region.note;
        if profile == dereth_primitives::EraId::Infiltration {
            note.x = region.earlier_x?;
        }
        Some(note)
    })
}

const REGIONS: [Region; 53] = [
    n(178, 20, 11, 12, "Aerlinthe Island"),
    n(18, 74, 5, 5, "Ahurenga"),
    n(141, 166, 7, 6, "Al-Arqas"),
    n(129, 121, 7, 6, "Al-Jalima"),
    n(190, 88, 9, 8, "Arwic"),
    n(19, 201, 7, 6, "Ayan Baqur"),
    n(200, 190, 7, 6, "Baishi"),
    n(184, 53, 5, 5, "Bandit Castle"),
    n(34, 84, 5, 5, "Bluespire"),
    n(44, 235, 5, 5, "Candeth Keep"),
    n(180, 97, 9, 8, "Cragstone"),
    n(91, 102, 5, 5, "Danby's Outpost"),
    n(211, 138, 9, 8, "Dryreach"),
    n(199, 106, 9, 8, "Eastham"),
    later(56, 13, 5, 5, "Fiun Outpost"),
    n(37, 127, 9, 8, "Fort Tethana"),
    n(156, 94, 9, 8, "Glenden Wood"),
    n(43, 79, 5, 5, "Greenspire"),
    n(224, 177, 7, 6, "Hebian-to"),
    n(164, 77, 9, 8, "Holtburg"),
    n(182, 230, 7, 6, "Kara"),
    n(155, 185, 7, 6, "Khayyaban"),
    n(224, 218, 7, 6, "Kryst"),
    n(212, 195, 7, 6, "Lin"),
    n(159, 224, 5, 5, "Linvak Tukal"),
    n(185, 126, 9, 8, "Lytelthorpe"),
    n(235, 220, 7, 6, "MacNiall's Freehold"),
    n(223, 203, 7, 6, "Mayoi"),
    n(141, 53, 5, 5, "Mt Esper-Crater Village"),
    n(224, 191, 7, 6, "Nanto"),
    n(142, 46, 5, 5, "Neydisa"),
    n(240, 128, 5, 5, "Oolutanga's Refuge"),
    n(74, 79, 5, 5, "Plateau Village"),
    n(148, 218, 7, 6, "Qalaba'r"),
    n(26, 83, 5, 5, "Redspire"),
    n(193, 114, 9, 8, "Rithwic"),
    n(146, 133, 7, 6, "Samsur"),
    later(50, 42, 5, 5, "Sanamar"),
    n(195, 163, 7, 6, "Sawato"),
    n(213, 171, 7, 6, "Shoushi"),
    later(41, 25, 5, 5, "Silyun"),
    n(6, 239, 15, 16, "Singularity Caul Island"),
    n(100, 48, 5, 5, "Stonehold"),
    n(32, 76, 5, 5, "Timaru"),
    n(239, 163, 7, 6, "Tou-Tou"),
    n(131, 148, 7, 6, "Tufa"),
    n(112, 244, 5, 5, "Ulgrim's Island"),
    n(159, 160, 7, 6, "Uziz"),
    n(63, 203, 7, 6, "Wai Jhou"),
    n(144, 181, 7, 6, "Xarabydun"),
    shifted(175, 179, 145, 7, 6, "Yanshi"),
    n(121, 156, 7, 6, "Yaraq"),
    n(123, 112, 7, 6, "Zaikhal"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::num::to_i32_f64;
    use dereth_primitives::EraId;

    /// Behaviour: map.regions.follow-world-profile
    #[test]
    fn profiles_select_regions_and_yanshi_position() {
        let early: Vec<_> = notes(EraId::Infiltration).collect();
        let later: Vec<_> = notes(EraId::Eor).collect();
        assert_eq!((early.len(), later.len()), (50, 53));
        for (name, x, y) in [
            ("Fiun Outpost", 56, 13),
            ("Sanamar", 50, 42),
            ("Silyun", 41, 25),
        ] {
            assert!(!early.iter().any(|n| n.name == name));
            let n = later.iter().find(|n| n.name == name).unwrap();
            assert_eq!((n.x, n.y, n.w, n.h), (x, y, 5, 5));
            assert!(n.contains(x, y));
            assert!(!n.contains(x + 5, y));
            assert!(!n.contains(x, y + 5));
        }
        assert_eq!(early.iter().find(|n| n.name == "Yanshi").unwrap().x, 179);
        assert_eq!(later.iter().find(|n| n.name == "Yanshi").unwrap().x, 175);
        assert_eq!(profile(&crate::EmptyGameView), EraId::Eor);
    }

    #[derive(Debug)]
    struct World(crate::EraView);
    impl crate::GameView for World {
        fn era(&self) -> Option<&crate::EraView> {
            Some(&self.0)
        }
    }

    fn place(kind: MapPlaceKind, name: &str, ns: f64, ew: f64) -> MapPlace {
        MapPlace {
            kind,
            name: name.to_owned(),
            ns,
            ew,
        }
    }

    /// Behaviour: none (this client's own map pages, for a world that names its own places)
    #[test]
    fn a_world_naming_its_own_places_has_them_on_its_map_pages_in_place_of_the_notes() {
        let own = WorldPlaces(
            vec![
                place(MapPlaceKind::Town, "Kehan", 10.0, 20.0),
                place(MapPlaceKind::Landmark, "Drudge Windmill", -5.0, 3.0),
            ]
            .into(),
        );
        let world = World(crate::EraView {
            own_places: Some(own.clone()),
            ..Default::default()
        });
        assert_eq!(page_names(&world), PageNames::Own(own.clone()));
        // A world naming none of its own has its era's notes.
        let retail = World(crate::EraView {
            era: EraId::Infiltration,
            era_announced: true,
            ..Default::default()
        });
        assert_eq!(page_names(&retail), PageNames::Notes(EraId::Infiltration));
        assert_eq!(
            page_names(&crate::EmptyGameView),
            PageNames::Notes(EraId::Eor)
        );
        // Equal by their places, whichever list holds them.
        let copy = WorldPlaces(own.0.to_vec().into());
        assert_eq!(copy, own);
        assert_ne!(WorldPlaces(own.0[..1].into()), own);
    }

    /// Behaviour: none (this client's own map pages, for a world that names its own places)
    #[test]
    fn a_world_s_own_notes_are_centered_on_their_places_sized_by_kind_with_the_towns_last() {
        let places = [
            place(MapPlaceKind::Town, "Kehan", 10.0, 20.0),
            place(MapPlaceKind::Landmark, "Drudge Windmill", -5.0, 3.0),
            place(MapPlaceKind::Landmark, "Off the picture", 99.0, 99.0),
            place(MapPlaceKind::Landmark, "Wasp Hive", 1.0, 1.0),
        ];
        // A pixel a unit, the middle at (100, 100), north up; nothing past 50 units.
        let at = |ns: f64, ew: f64| {
            (ns.abs() < 50.0 && ew.abs() < 50.0)
                .then(|| (100 + to_i32_f64(ew), 100 - to_i32_f64(ns)))
        };
        let notes = own_notes(&places, at);
        let got: Vec<(&str, i32, i32, i32, i32)> = notes
            .iter()
            .map(|n| (n.name.as_str(), n.x, n.y, n.w, n.h))
            .collect();
        assert_eq!(
            got,
            [
                ("Drudge Windmill", 103 - 2, 105 - 2, 5, 5),
                ("Wasp Hive", 101 - 2, 99 - 2, 5, 5),
                ("Kehan", 120 - 4, 90 - 4, 9, 8),
            ]
        );
        assert_eq!(notes[2].kind, MapPlaceKind::Town);
        assert!(notes[2].contains(116, 86) && notes[2].contains(124, 93));
        assert!(!notes[2].contains(125, 86) && !notes[2].contains(116, 94));
    }

    /// Behaviour: none (this client's own map pages, for a world that names its own places)
    #[test]
    fn a_town_is_marked_rimmed_and_filled_and_anything_else_with_a_small_cross_inside_the_frame() {
        let pixels = |kind| {
            let rows = mark(kind);
            let mut grid = vec![vec!['.'; rows[0].len()]; rows.len()];
            for (x, y, len, color) in mark_runs(kind) {
                for i in x..x + len {
                    grid[y as usize][i as usize] = if color == MARK_RIM { 'Y' } else { 'N' };
                }
            }
            grid
        };
        let town = pixels(MapPlaceKind::Town);
        assert_eq!((town[0].len(), town.len()), (9, 8));
        for (row, want) in town.iter().zip(mark(MapPlaceKind::Town)) {
            assert_eq!(row.iter().collect::<String>(), *want);
        }
        assert!(town.iter().flatten().any(|c| *c == 'N'), "filled");
        let landmark = pixels(MapPlaceKind::Landmark);
        assert_eq!((landmark[0].len(), landmark.len()), (5, 5));
        let rims = landmark.iter().flatten().filter(|c| **c == 'Y').count();
        assert_eq!(rims, 5, "a cross of five");
        assert!(landmark.iter().flatten().all(|c| *c != 'N'));
        assert_eq!(mark(MapPlaceKind::Dungeon), mark(MapPlaceKind::Landmark));
        // The edge, where a hovered note's frame is drawn, is clear.
        for grid in [town, landmark] {
            let (w, h) = (grid[0].len(), grid.len());
            for (y, row) in grid.iter().enumerate() {
                for (x, c) in row.iter().enumerate() {
                    if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                        assert_eq!(*c, '.', "({x}, {y})");
                    }
                }
            }
        }
    }
}
