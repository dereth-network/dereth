//! Ambient sound by place on made-up places and a made-up ambient table, heard through a
//! stand-in for the sound manager that records what it is asked.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use auberean_assets::region::{AmbientSound, ContinuousSound, IntermittentSound, SoundDesc};
use auberean_audio::ambient::desc::Description;
use auberean_audio::ambient::objects::{bits, Gathered, Intermittent};
use auberean_audio::ambient::{
    is_emitter, Ambient, LandPoint, LandPoints, Place, Speaker, Surroundings,
};
use auberean_audio::manager::stop;
use auberean_audio::request::{Channel, PlayRequest, PlayType};
use auberean_primitives::random::Random;
use auberean_primitives::DataId;

const TABLE: u32 = 0x1000_0000;
const FOREST: u32 = 0x8100_0018;
const MEADOW: u32 = 0x8100_0019;
const FOREST_MUSIC: u32 = 0x2C00_0168;
const MEADOW_MUSIC: u32 = 0x2C00_0169;
const HALL_MUSIC: u32 = 0x2C00_016A;
const BIRDS: u32 = 0x2A00_0001;

/// The sound manager as the ambient sound sees it: every request kept, handles counted from 1,
/// the handles marked done, the emitters stopped.
#[derive(Debug, Default)]
struct Ear {
    plays: Vec<(u32, PlayRequest)>,
    stopped: Vec<u64>,
    masks: Vec<u32>,
    done: HashSet<u32>,
    frequencies: Vec<(u32, f32)>,
    placed: HashSet<u32>,
    table: Option<SoundDesc>,
}

impl Speaker for Ear {
    fn play(&mut self, req: &PlayRequest) -> u32 {
        let h = u32::try_from(self.plays.len()).expect("few") + 1;
        self.plays.push((h, *req));
        h
    }
    fn stop_for_emitter(&mut self, emitter: u64) {
        self.stopped.push(emitter);
    }
    fn stop_sounds(&mut self, mask: u32) {
        self.masks.push(mask);
    }
    fn has_finished(&self, handle: u32) -> bool {
        handle == 0 || self.done.contains(&handle)
    }
    fn set_frequency(&mut self, handle: u32, frequency: f32) {
        self.frequencies.push((handle, frequency));
    }
    fn placed(&self, id: u32) -> bool {
        self.placed.contains(&id)
    }
    fn ambient_table(&self, id: u32) -> Option<SoundDesc> {
        (id == TABLE).then(|| self.table.clone()).flatten()
    }
}

impl Ear {
    /// The ids played, in order.
    fn played(&self) -> Vec<u32> {
        self.plays.iter().map(|(_, r)| r.id).collect()
    }
}

/// Continuous place music, as the end-of-retail table holds it: a music setting that plays only
/// while its description dominates (and, `plays_on`, keeps playing after).
fn music(id: u32, plays_on: bool) -> ContinuousSound {
    ContinuousSound {
        flags: bits::MUSIC | bits::DOMINANT_ONLY | if plays_on { bits::PLAYS_ON } else { 0 },
        sound: DataId(id),
        volume: 1.0,
        min_pitch: 1.0,
        max_pitch: 1.0,
        min_height: 0.0,
        max_height: 0.0,
    }
}

fn kind(kind: u32, continuous: Vec<ContinuousSound>) -> AmbientSound {
    AmbientSound {
        kind,
        environment: DataId(0x2900_0001),
        intermittent: Vec::new(),
        continuous,
    }
}

/// The forest's music (which stops when the forest stops dominating), the meadow's (which plays
/// on), the halls' (interior cells).
fn table() -> SoundDesc {
    SoundDesc {
        version: 0,
        sounds: vec![
            kind(FOREST, vec![music(FOREST_MUSIC, false)]),
            kind(MEADOW, vec![music(MEADOW_MUSIC, true)]),
            kind(INTERIOR_KIND, vec![music(HALL_MUSIC, false)]),
        ],
    }
}

fn listening() -> Ear {
    Ear {
        table: Some(table()),
        ..Ear::default()
    }
}

/// The landscape's points of each block, as made-up.
#[derive(Default)]
struct Land(HashMap<(u8, u8), Arc<[LandPoint]>>);

impl LandPoints for Land {
    fn points(&mut self, block: (u8, u8)) -> Arc<[LandPoint]> {
        self.0
            .get(&block)
            .cloned()
            .unwrap_or_else(|| Arc::from(Vec::new()))
    }
}

/// Block (10, 10): forest points along its west edge (x 0), meadow points along x 100, every 10
/// meters north.
fn land() -> Land {
    let mut pts = Vec::new();
    for j in 0..16u8 {
        let y = f32::from(j) * 10.0;
        pts.push(LandPoint {
            kind: FOREST,
            origin: [0.0, y, 0.0],
        });
        pts.push(LandPoint {
            kind: MEADOW,
            origin: [100.0, y, 0.0],
        });
    }
    let mut l = Land::default();
    l.0.insert((10, 10), Arc::from(pts));
    l
}

const BLOCK: f32 = 160.0;
/// The kind of a plain interior cell.
const INTERIOR_KIND: u32 = 2;

/// A player outdoors in block (10, 10) at `(x, y)` of the block.
fn outdoors(x: f32, y: f32) -> (u32, Place) {
    (0x0A0A_0001, [10.0 * BLOCK + x, 10.0 * BLOCK + y, 0.0])
}

fn around<'a>(
    at: (u32, Place),
    cells: &'a [(u32, Place)],
    blocks: &'a [(u8, u8)],
) -> Surroundings<'a> {
    Surroundings {
        cell: at.0,
        player: at.1,
        blocking: false,
        cells,
        outside: true,
        blocks,
    }
}

fn ambient() -> Ambient {
    let mut a = Ambient::new(BLOCK);
    a.set_region(TABLE);
    a
}

/// Behaviour: audio.ambient.a-place-weighs-by-its-level-distance
#[test]
fn a_place_weighs_fully_within_twelve_meters_and_by_the_inverse_square_beyond() {
    let mut d = Description::new(&kind(FOREST, vec![music(FOREST_MUSIC, false)]), &mut 1);
    d.reset();
    // Within 12 meters: 1. Its height does not count.
    assert_eq!(d.add_sound([6.0, -1.0, 50.0], false), 1.0);
    // At 24 meters: 144 / 576.
    assert_eq!(d.add_sound([0.0, 24.0, 0.0], false), 0.25);
    // Muffled (the other side of the player's indoors or outdoors): halved.
    assert_eq!(d.add_sound([0.0, 24.0, 0.0], true), 0.125);
    // Past 100 meters the sound does not count it; the place still adds by the inverse square.
    let far = d.add_sound([0.0, 120.0, 0.0], false);
    assert_eq!(far, 144.0 / 14_400.0);
    assert_eq!(
        d.weight, 1.375,
        "the far place adds nothing to the description"
    );
    // The direction and distance the continuous sound gathered: north (0) took the 24 meter
    // places; east (2) the place 6 meters off.
    let g = &d.continuous[0].gathered;
    assert_eq!(g.directions[0].weight, 0.375);
    assert_eq!(
        (g.directions[0].min_distance, g.directions[0].max_distance),
        (24.0, 24.0)
    );
    assert_eq!(g.directions[2].weight, 1.0);
}

/// Behaviour: audio.ambient.a-direction-is-a-compass-eighth
#[test]
fn a_direction_is_its_compass_eighth_clockwise_from_north() {
    assert_eq!(Gathered::direction([1.0, 10.0, 0.0]), 0);
    assert_eq!(Gathered::direction([10.0, 9.0, 0.0]), 1);
    assert_eq!(Gathered::direction([10.0, -1.0, 0.0]), 2);
    assert_eq!(Gathered::direction([-1.0, -10.0, 0.0]), 4);
    assert_eq!(Gathered::direction([-10.0, 1.0, 0.0]), 6);
    assert_eq!(Gathered::direction([-9.0, 10.0, 0.0]), 7);
    // Too short to point: north.
    assert_eq!(Gathered::direction([0.0001, -0.0001, 0.0]), 0);
    // A place at the player weighs 1, spread over all eight.
    let mut g = Gathered::default();
    let mut sum = 0.0;
    assert!(g.add([0.0; 3], true, &mut sum));
    assert_eq!(sum, 1.0, "at the player even a muffled place weighs 1");
    assert!(g.directions.iter().all(|d| d.weight == 0.125));
}

/// Behaviour: audio.ambient.the-dominant-description-plays-its-music
#[test]
fn the_description_with_the_greatest_share_dominates_and_plays_its_place_music() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    // Near the west edge: the forest weighs most.
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    assert_eq!(a.dominant_kind(), Some(FOREST));
    assert_eq!(ear.played(), vec![FOREST_MUSIC]);
    let (_, req) = ear.plays[0];
    assert_eq!(req.channel, Channel::Music);
    assert_eq!(
        req.kind,
        PlayType::Inherit,
        "the music setting says how it plays"
    );
    let forest = &a.descriptions()[0];
    assert!(forest.share > 0.5 && forest.share < 1.0);
    assert_eq!(
        req.volume, forest.share,
        "its volume is its description's share"
    );
    assert!(is_emitter(req.emitter.expect("made by its object")));
    // Both heard, the meadow's music waits for its description to dominate.
    assert!(a.descriptions().iter().all(|d| d.active));
}

/// Behaviour: audio.ambient.recomputed-after-four-meters
#[test]
fn the_places_are_weighed_again_only_after_four_meters_and_never_while_cells_are_waited_for() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    assert_eq!(ear.plays.len(), 1);
    // Three meters on: nothing weighed, nothing asked.
    a.update(
        &around(outdoors(5.0, 83.0), &[], &blocks),
        &mut land,
        0.1,
        &mut ear,
        &r,
    );
    assert_eq!(ear.plays.len(), 1);
    // Waited-for cells: nothing, however far.
    let mut s = around(outdoors(5.0, 120.0), &[], &blocks);
    s.blocking = true;
    a.update(&s, &mut land, 0.2, &mut ear, &r);
    assert_eq!(ear.plays.len(), 1);
    // Four meters from where it was last weighed: weighed again, and the dominant description's
    // music asked for again.
    a.update(
        &around(outdoors(5.0, 84.0), &[], &blocks),
        &mut land,
        0.3,
        &mut ear,
        &r,
    );
    assert_eq!(ear.played(), vec![FOREST_MUSIC, FOREST_MUSIC]);
}

/// Behaviour: audio.ambient.dominance-moves-with-the-player
#[test]
fn when_dominance_moves_the_new_music_plays_and_the_old_stops_unless_it_plays_on() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    let forest_object = ear.plays[0].1.emitter.expect("an object");
    // Over by the meadow: it dominates. The forest, still heard and dominant as the shares are
    // worked out, asks for its music once more; then the meadow's plays and the forest's stops.
    a.update(
        &around(outdoors(95.0, 80.0), &[], &blocks),
        &mut land,
        1.0,
        &mut ear,
        &r,
    );
    assert_eq!(a.dominant_kind(), Some(MEADOW));
    assert_eq!(ear.played(), vec![FOREST_MUSIC, FOREST_MUSIC, MEADOW_MUSIC]);
    assert_eq!(ear.stopped, vec![forest_object]);
    let meadow_object = ear.plays[2].1.emitter.expect("an object");
    // Back west: the forest again; the meadow's music plays on.
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        2.0,
        &mut ear,
        &r,
    );
    assert_eq!(a.dominant_kind(), Some(FOREST));
    assert!(!ear.stopped.contains(&meadow_object));
}

/// Behaviour: audio.ambient.cells-and-the-landscape
#[test]
fn interior_cells_name_their_kind_and_the_landscape_is_heard_only_when_it_can_be() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    // Inside a hall, a cell of it at the player and two more 10 meters off; the landscape not
    // heard.
    let at = (0x0A0A_0105, [10.0 * BLOCK + 5.0, 10.0 * BLOCK + 80.0, 0.0]);
    let cells = [
        (INTERIOR_KIND, at.1),
        (INTERIOR_KIND, [at.1[0] + 10.0, at.1[1], 0.0]),
        (INTERIOR_KIND, [at.1[0], at.1[1] + 10.0, 0.0]),
    ];
    let mut s = around(at, &cells, &blocks);
    s.outside = false;
    a.update(&s, &mut land, 0.0, &mut ear, &r);
    assert_eq!(a.dominant_kind(), Some(INTERIOR_KIND));
    assert_eq!(a.descriptions().len(), 1, "no landscape point was weighed");
    assert_eq!(ear.played(), vec![HALL_MUSIC]);
    // Seeing out, from the same hall: the forest's points (outdoors, so muffled) count too.
    let mut b = ambient();
    let mut ear2 = listening();
    s.outside = true;
    b.update(&s, &mut land, 0.0, &mut ear2, &r);
    let kinds: Vec<u32> = b.descriptions().iter().map(|d| d.kind).collect();
    assert_eq!(kinds, vec![INTERIOR_KIND, FOREST, MEADOW]);
    // A cell that names a kind of its own is heard as that kind.
    let mut c = ambient();
    let mut ear3 = listening();
    let named = [(FOREST, at.1), (FOREST, [at.1[0] + 10.0, at.1[1], 0.0])];
    let mut s = around(at, &named, &blocks);
    s.outside = false;
    c.update(&s, &mut land, 0.0, &mut ear3, &r);
    assert_eq!(c.dominant_kind(), Some(FOREST));
    assert_eq!(c.descriptions().len(), 1);
}

/// Behaviour: audio.ambient.quiet-descriptions-go
#[test]
fn a_description_heard_below_a_hundredth_falls_quiet_and_goes_at_the_next_check() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    assert_eq!(a.descriptions().len(), 2);
    assert!(a.descriptions().iter().all(|d| d.active));
    // The quiet are let go of every half second: the next check after this one is at 1.4.
    a.use_time(0.9, &mut ear, &r);
    // Inside, out of the landscape: the forest and meadow fall quiet.
    let hall = (0x0A0A_0105, [10.0 * BLOCK + 5.0, 10.0 * BLOCK + 80.0, 0.0]);
    let cells = [(INTERIOR_KIND, hall.1)];
    let mut s = around(hall, &cells, &blocks);
    s.outside = false;
    // Move the player 4 meters so the places are weighed again.
    s.player[1] += 4.0;
    a.update(&s, &mut land, 1.0, &mut ear, &r);
    assert!(a
        .descriptions()
        .iter()
        .filter(|d| d.kind != INTERIOR_KIND)
        .all(|d| !d.active));
    // They go at the next check, and their objects' sounds stop.
    let before = ear.stopped.len();
    a.use_time(1.2, &mut ear, &r);
    assert_eq!(a.descriptions().len(), 3, "not before the check");
    a.use_time(1.5, &mut ear, &r);
    assert_eq!(
        a.descriptions().iter().map(|d| d.kind).collect::<Vec<_>>(),
        vec![INTERIOR_KIND]
    );
    assert!(ear.stopped.len() > before);
}

/// Behaviour: audio.ambient.heard-from-the-camera
#[test]
fn the_place_music_stands_where_the_camera_was_drawn_from() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    let object = ear.plays[0].1.emitter.expect("an object");
    let camera = [1605.0, 1675.0, 3.0];
    a.set_viewer(camera);
    a.use_time(0.1, &mut ear, &r);
    assert_eq!(a.emitter_place(object), Some(camera));
    assert_eq!(a.emitter_place(0x1234), None, "not an ambient object");
}

/// Behaviour: audio.ambient.no-table-no-ambient
#[test]
fn with_no_ambient_table_the_ambient_sound_turns_itself_off() {
    let (mut ear, mut land, r) = (listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    let mut a = Ambient::new(BLOCK);
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    assert!(!a.enabled());
    assert!(ear.plays.is_empty());
    // A region with a table turns it on again; the places are weighed once the player has moved
    // 4 meters from where it last tried.
    a.set_region(TABLE);
    a.update(
        &around(outdoors(5.0, 82.0), &[], &blocks),
        &mut land,
        1.0,
        &mut ear,
        &r,
    );
    assert!(ear.plays.is_empty());
    a.update(
        &around(outdoors(5.0, 84.0), &[], &blocks),
        &mut land,
        2.0,
        &mut ear,
        &r,
    );
    assert_eq!(ear.played(), vec![FOREST_MUSIC]);
}

/// Behaviour: audio.ambient.leaving-the-world-stops-it
#[test]
fn clearing_lets_every_description_go_and_stops_the_music_and_flat_sounds() {
    let (mut a, mut ear, mut land, r) = (ambient(), listening(), land(), Random::new());
    let blocks = [(10u8, 10u8)];
    a.update(
        &around(outdoors(5.0, 80.0), &[], &blocks),
        &mut land,
        0.0,
        &mut ear,
        &r,
    );
    a.clear(&mut ear);
    assert!(a.descriptions().is_empty());
    assert_eq!(a.dominant_kind(), None);
    assert_eq!(
        ear.masks,
        vec![stop::FLAT_MUSIC | stop::PLACED_MUSIC | stop::FLAT_SOUNDS]
    );
    // The next update weighs the places afresh, wherever the player is.
    a.update(
        &around(outdoors(5.0, 81.0), &[], &blocks),
        &mut land,
        1.0,
        &mut ear,
        &r,
    );
    assert_eq!(a.dominant_kind(), Some(FOREST));
}

/// Behaviour: audio.ambient.intermittent-sounds-play-now-and-then
#[test]
fn an_intermittent_sound_waits_its_time_and_plays_by_chance_one_a_turn() {
    let r = Random::new();
    let info = IntermittentSound {
        flags: bits::SOUND,
        sound: DataId(BIRDS),
        volume: 0.5,
        probability: 1.0,
        min_pitch: 1.5,
        max_pitch: 1.5,
        min_height: 2.0,
        max_height: 2.0,
        min_time: 10.0,
        max_time: 10.0,
    };
    let mut first = Intermittent::new(&info, 7).expect("a sound setting");
    let mut second = Intermittent::new(&info, 8).expect("a sound setting");
    let mut count = 0;
    // Due at once: the first plays (a roll within probability times density 1); its next time is
    // 10 seconds on.
    assert!(first.play_check(5.0, &mut count, 1.0, false, &r));
    assert_eq!(first.next, 15.0);
    // The second is due too, but one already played this turn: it waits, pulled in to a random
    // point of its wait.
    assert!(!second.play_check(5.0, &mut count, 1.0, false, &r));
    assert!(second.next > 5.0 && second.next < 15.0);
    // Not before its time.
    let mut count = 0;
    assert!(!first.play_check(14.0, &mut count, 1.0, false, &r));
    // A density of 0 never plays.
    let mut third = Intermittent::new(&info, 9).expect("a sound setting");
    assert!(!third.play_check(5.0, &mut count, 0.0, false, &r));
    // Played: from a place about the camera in its direction, 2 meters up, on the ambient
    // channel, one at a time and counted, at its volume, and at its pitch.
    let mut ear = Ear::default();
    first.gathered.directions[2].density = 1.0;
    first.gathered.directions[2].min_distance = 30.0;
    first.gathered.directions[2].max_distance = 30.0;
    first.play([100.0, 100.0, 0.0], 1.0, &mut ear, &r);
    let (h, req) = ear.plays[0];
    assert_eq!(
        (req.id, req.channel, req.kind, req.volume),
        (
            BIRDS,
            Channel::Ambient,
            PlayType::SelfExclusiveBudgeted,
            0.5
        )
    );
    assert_eq!(ear.frequencies, vec![(h, 1.5)]);
    let d = [first.place[0] - 100.0, first.place[1] - 100.0];
    assert!(
        ((d[0] * d[0] + d[1] * d[1]).sqrt() - 30.0).abs() < 1e-3,
        "30 meters off: {d:?}"
    );
    assert_eq!(
        Gathered::direction([d[0], d[1], 0.0]),
        2,
        "to the east-ish eighth"
    );
    assert_eq!(first.place[2], 2.0);
    // While it plays it is not played again.
    first.play([100.0, 100.0, 0.0], 1.0, &mut ear, &r);
    assert_eq!(ear.plays.len(), 1);
    ear.done.insert(h);
    first.play([100.0, 100.0, 0.0], 1.0, &mut ear, &r);
    assert_eq!(ear.plays.len(), 2);
}
