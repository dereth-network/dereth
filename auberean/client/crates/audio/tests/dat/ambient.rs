//! Ambient sound by place over the end-of-retail data: standing in the world, the dominant
//! description's place music plays through the sound manager.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::Context;
use auberean_audio::ambient::{self, Ambient, LandPoint, LandPoints, Manager, Place, Surroundings};
use auberean_audio::manager::SoundPlayer;
use auberean_audio::space::Listener;
use auberean_audio::{records, Life};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_primitives::random::Random;
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::sound::block_sound_points;

use crate::records::dats;

/// The landscape's points of each block, worked out from the world's records.
struct Points<'a> {
    records: &'a WorldRecords,
    region: &'a RegionSet,
    kept: HashMap<(u8, u8), Arc<[LandPoint]>>,
}

impl LandPoints for Points<'_> {
    fn points(&mut self, block: (u8, u8)) -> Arc<[LandPoint]> {
        let (records, region) = (self.records, self.region);
        Arc::clone(self.kept.entry(block).or_insert_with(|| {
            let b = records
                .landblock(i32::from(block.0), i32::from(block.1))
                .expect("the block is in the world");
            let scenes = region.scenes.as_ref().expect("scenery described");
            block_sound_points(&region.land, &region.terrain, scenes, &b)
                .into_iter()
                .map(|p| LandPoint {
                    kind: p.kind,
                    origin: [p.origin.x, p.origin.y, p.origin.z],
                })
                .collect()
        }))
    }
}

fn world() -> WorldRecords {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("the install's folder lists");
    let open = |want: DatName| {
        let (_, p) = found
            .iter()
            .find(|(n, _)| *n == want)
            .expect("the file is in the set");
        DatFile::open(p).expect("opens")
    };
    let portal = open(DatName::Portal);
    let cell = open(DatName::Cell(1));
    let ctx = Context::from_portal(&portal).expect("the property list");
    WorldRecords::new(portal, cell, None, ctx)
}

/// Behaviour: audio.ambient.place-music-plays-in-the-world
#[test]
fn standing_outdoors_the_dominant_places_music_plays_on_the_primary_track() {
    let records = world();
    let region = records.region().expect("the region");
    let files = dats();
    let table = region
        .region
        .sounds
        .first()
        .expect("an ambient table")
        .record
        .0;
    let mut player = SoundPlayer::new(Some(22_050));
    let mut ambient = Ambient::new(160.0);
    ambient.set_region(table);
    let mut land = Points {
        records: &records,
        region: &region,
        kept: HashMap::new(),
    };
    let random = Random::new();
    // The middle of landblock (0xC1, 0x56), the camera 5 meters behind and 2 up.
    let (bx, by) = (0xC1u8, 0x56u8);
    let corner = [f32::from(bx) * 160.0, f32::from(by) * 160.0];
    let at: Place = [corner[0] + 80.0, corner[1] + 80.0, 10.0];
    let camera: Place = [at[0], at[1] - 5.0, at[2] + 2.0];
    let blocks: Vec<(u8, u8)> = (bx - 1..=bx + 1)
        .flat_map(|x| (by - 1..=by + 1).map(move |y| (x, y)))
        .collect();
    let around = Surroundings {
        cell: u32::from(bx) << 24 | u32::from(by) << 16 | 0x0021,
        player: at,
        blocking: false,
        cells: &[],
        outside: true,
        blocks: &blocks,
    };
    let listener = Listener {
        position: camera,
        right: [1.0, 0.0, 0.0],
        ahead: [0.0, 1.0, 0.0],
        up: [0.0, 0.0, 1.0],
    };
    {
        let mut speaker = Manager {
            player: &mut player,
            files: &files,
        };
        ambient.set_viewer(camera);
        ambient.update(&around, &mut land, 0.0, &mut speaker, &random);
        ambient.use_time(0.0, &mut speaker, &random);
    }
    let kind = ambient.dominant_kind().expect("a description dominates");
    let d = ambient
        .descriptions()
        .iter()
        .find(|d| d.kind == kind)
        .expect("the dominant one");
    let music = d.continuous[0].info.sound.0;
    let setting = records::music_info(&files, music).expect("a music setting");
    assert_eq!(
        setting.flags & 0x2,
        0x2,
        "a place piece plays on the primary track"
    );
    let emitters = |id: u64| ambient.emitter_place(id);
    player.use_time(0.0, listener, &emitters);
    player.use_time(0.05, listener, &emitters);
    let playing = player.playing();
    assert!(
        playing
            .iter()
            .any(|(_, id, life)| *id == music && *life == Life::Playing),
        "{music:08X} plays: {playing:X?}"
    );
    assert_eq!(
        player.stats.pieces, 1,
        "its piece started on the performance"
    );
    assert!(ambient::is_emitter(
        player
            .emitters()
            .first()
            .copied()
            .expect("made by its object")
    ));
}
