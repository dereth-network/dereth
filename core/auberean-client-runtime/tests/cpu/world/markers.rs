//! The map's markers kept for every front end: the map's rules for what the scripts put on it, and
//! the markers as the view carries them while the player is in the world.

use auberean_client_contract::text::TextRef;
use auberean_client_contract::view::{MapMarker, MapMarkers};
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::host::PanelNotice;
use auberean_client_runtime::said::native;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::ServerMessage;
use auberean_script::native_object::NativeObject;

use super::download::file;

const PLAYER: u64 = 0x2130_0000_0000_00A1;
/// A land of 255 blocks a side, in land cells.
const LAND: (i32, i32) = (2040, 2040);

/// The land cell at land coordinates `(x, y)`.
fn cell(x: i32, y: i32) -> u32 {
    auberean_primitives::land::cell_at(x, y, LAND)
}

fn marker(kind: u32, object: u64, cell: u32) -> MapMarker {
    MapMarker {
        kind,
        object,
        cell,
        picture: 0x4100_0001,
        label: None,
    }
}

fn kinds_and_objects(m: &MapMarkers) -> Vec<(u32, u64)> {
    m.markers.iter().map(|m| (m.kind, m.object)).collect()
}

/// A marker is not put where one of its kind stands within two and a half cells (another kind's
/// may stand there), nor of the map's own labels' kind; a kind keeps its latest ten, the oldest
/// going first; a marker for the same object takes the old one's place; one is taken off by its
/// object, and a kind is cleared at once.
///
/// Behaviour: ui.map.markers-come-and-go-by-kind-for-every-front-end
#[test]
fn the_map_keeps_the_markers_by_its_rules() {
    let mut m = MapMarkers::default();
    assert!(m.add(marker(1, 7, cell(100, 100)), LAND));
    assert!(
        !m.add(marker(1, 8, cell(101, 101)), LAND),
        "two cells' squares from one of its kind"
    );
    assert!(m.add(marker(2, 8, cell(101, 101)), LAND), "another kind");
    assert!(m.add(marker(1, 9, cell(102, 102)), LAND), "far enough");
    assert!(
        !m.add(marker(MapMarkers::LABELS, 10, cell(500, 500)), LAND),
        "the labels' kind is the map's own"
    );
    assert!(m.add(marker(1, 7, cell(300, 300)), LAND), "moved");
    assert_eq!(
        kinds_and_objects(&m),
        vec![(2, 8), (1, 9), (1, 7)],
        "the same object's marker replaced"
    );
    for (i, x) in (0..12).zip((400..).step_by(10)) {
        assert!(m.add(marker(1, 100 + i, cell(x, 400)), LAND));
    }
    let ones: Vec<u64> = m
        .markers
        .iter()
        .filter(|m| m.kind == 1)
        .map(|m| m.object)
        .collect();
    assert_eq!(ones, (102..112).collect::<Vec<u64>>(), "the latest ten");
    assert!(m.markers.iter().any(|m| m.kind == 2), "another kind's kept");
    let before = m.version;
    assert!(m.remove(1, 105));
    assert!(!m.remove(1, 105), "gone already");
    m.clear(1);
    assert_eq!(kinds_and_objects(&m), vec![(2, 8)]);
    assert!(m.version > before);
}

fn client() -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![file(0x1_0000_0000, &[]), file(0x2_0000_0001, &[])],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

/// A notice as the scripts send it to every panel: its words, and the text at a word's place.
fn sent(native: u32, words: Vec<u32>, text: Option<(usize, &str)>) -> PanelNotice {
    let n = words.len();
    let mut objects = vec![None; n];
    if let Some((at, text)) = text {
        objects[at] = Some(NativeObject::Text(text.encode_utf16().collect()));
    }
    PanelNotice {
        native,
        about: None,
        words,
        objects,
        requests: vec![None; n],
        effect_lines: vec![Vec::new(); n],
        listed: vec![Vec::new(); n],
        selections: vec![None; n],
    }
}

/// A marker put: its kind, the object, the cell, its picture and its words.
fn added(kind: u32, object: u64, cell: u32, words: &str) -> PanelNotice {
    #[allow(clippy::cast_possible_truncation)]
    let w = vec![
        kind,
        (object >> 32) as u32,
        object as u32,
        cell,
        0x4100_0002,
        0,
    ];
    sent(native::MARKER_ADDED, w, Some((5, words)))
}

fn refresh(c: &mut Client) {
    c.refresh_view(&|_| Vec::new());
}

/// While the player is in the world the view carries the markers the scripts' notices put on the
/// map, each heard once however often the view is refreshed before the notices are taken, with
/// its words; one taken off goes; and they all go as the player leaves the world. Before the
/// player is here, none is put.
///
/// Behaviour: ui.map.the-view-carries-the-markers-the-scripts-put
#[test]
fn the_view_carries_the_markers_while_the_player_is_in_the_world() {
    let mut c = client();
    c.notices.push(added(1, 5, 0x2998_0021, "too soon"));
    refresh(&mut c);
    assert!(c.view().markers.markers.is_empty(), "no player yet");
    let _ = c.take_notices();

    c.model.player = Some(PLAYER);
    c.take_message(
        0x0300_0000_0000_0000,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            ..CreateObject::default()
        }),
    );
    c.notices.push(added(1, 5, 0x2998_0021, "The mine"));
    c.notices.push(added(4, 6, 0x2A98_0021, "A friend"));
    refresh(&mut c);
    refresh(&mut c);
    let m = &c.view().markers;
    assert_eq!(
        kinds_and_objects(m),
        vec![(1, 5), (4, 6)],
        "each heard once"
    );
    assert_eq!(m.markers[0].cell, 0x2998_0021);
    assert_eq!(m.markers[0].picture, 0x4100_0002);
    assert_eq!(m.markers[0].label, Some(TextRef::literal("The mine")));
    let _ = c.take_notices();

    c.notices
        .push(sent(native::MARKER_REMOVED, vec![4, 0, 6], None));
    refresh(&mut c);
    assert_eq!(kinds_and_objects(&c.view().markers), vec![(1, 5)]);
    let _ = c.take_notices();

    c.model.player = None;
    refresh(&mut c);
    assert!(c.view().markers.markers.is_empty(), "left the world");
}
