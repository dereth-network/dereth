//! In-memory [`AudioAssets`] and a small region, so that both the unit tests and
//! the conformance harness can drive the crate without a retail install.
//!
//! Nothing here asserts fidelity. It is scaffolding: the fidelity claims are made by tests that name
//! a retail oracle, and the ones that need the real dats live in `core/audio/tests/dat/audio/audio_conformance.rs`,
//! which **fails** when the install is absent rather than skipping.

use std::collections::BTreeMap;

use dereth_assets::audio::{
    SoundEntry, SoundTable, SoundTableNode, Wave, WaveFormat, WAVE_FORMAT_PCM,
};
use dereth_assets::region::{
    AmbientSound, GameTime, LandDefs, LandSurf, Region, SceneDesc, SoundDesc, TerrainType,
};
use dereth_primitives::DataId;

use crate::{AmbientSoundDescriptor, AudioAssets};

/// A wave record plus its decoded header, held together the way a dat read holds them.
#[derive(Debug, Clone)]
struct StubWave {
    wave: Wave,
    record: Vec<u8>,
}

/// An [`AudioAssets`] over in-memory data.
#[derive(Debug, Default)]
pub struct StubAssets {
    // ORDER-OK: keyed by DataID and only ever looked up, never iterated.
    waves: BTreeMap<DataId, StubWave>,
    tables: BTreeMap<DataId, SoundTable>,
    descs: Vec<SoundDesc>,
}

impl StubAssets {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mono 16-bit PCM wave at the mix rate, `frames` samples of a constant 1.0.
    pub fn add_tone(&mut self, id: DataId, frames: usize) {
        let data: Vec<u8> = (0..frames).flat_map(|_| 32_767i16.to_le_bytes()).collect();
        let fmt = WaveFormat {
            format_tag: WAVE_FORMAT_PCM,
            channels: 1,
            samples_per_sec: crate::MIX_RATE,
            avg_bytes_per_sec: crate::MIX_RATE * 2,
            block_align: 2,
            bits_per_sample: 16,
            cb_size: Some(0),
        };
        let mut header = Vec::new();
        header.extend_from_slice(&fmt.format_tag.to_le_bytes());
        header.extend_from_slice(&fmt.channels.to_le_bytes());
        header.extend_from_slice(&fmt.samples_per_sec.to_le_bytes());
        header.extend_from_slice(&fmt.avg_bytes_per_sec.to_le_bytes());
        header.extend_from_slice(&fmt.block_align.to_le_bytes());
        header.extend_from_slice(&fmt.bits_per_sample.to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        let mut record = Vec::new();
        record.extend_from_slice(&id.raw().to_le_bytes());
        record.extend_from_slice(&u32::try_from(header.len()).expect("small").to_le_bytes());
        record.extend_from_slice(&u32::try_from(data.len()).expect("small").to_le_bytes());
        let data_offset = record.len() + header.len();
        record.extend_from_slice(&header);
        record.extend_from_slice(&data);
        let wave = Wave {
            id,
            header_size: u32::try_from(header.len()).expect("small"),
            data_size: u32::try_from(data.len()).expect("small"),
            header,
            data_offset,
            format: Some(fmt),
        };
        self.waves.insert(id, StubWave { wave, record });
    }

    /// One wave, ready to play.
    #[must_use]
    pub fn with_tone(id: DataId, frames: usize) -> Self {
        let mut s = Self::new();
        s.add_tone(id, frames);
        s
    }

    /// A one-row table mapping `stype` to `wave`, plus the wave itself.
    #[must_use]
    pub fn with_table(table: DataId, stype: u32, wave: DataId) -> Self {
        let mut s = Self::with_tone(wave, 4);
        s.add_table(table, &[(stype, wave)]);
        s
    }

    /// Add a two-level sound table: a root with key 0 and one child per `(stype, wave)`.
    pub fn add_table(&mut self, id: DataId, rows: &[(u32, DataId)]) {
        let mut nodes = vec![SoundTableNode {
            key: 0,
            data: Vec::new(),
            children: Vec::new(),
        }];
        for (stype, wave) in rows {
            let idx = u32::try_from(nodes.len()).expect("small");
            nodes.push(SoundTableNode {
                key: *stype,
                data: vec![SoundEntry {
                    sound_id: *wave,
                    priority: 0.0,
                    probability: 1.0,
                    volume: 1.0,
                }],
                children: Vec::new(),
            });
            nodes[0].children.push(idx);
        }
        self.tables.insert(id, SoundTable { id, nodes });
    }

    /// Add a two-level sound table whose rows carry their **own** volume and probability.
    ///
    /// [`Self::add_table`] fixes both at 1.0, which is fine for "did a sound play" but cannot tell
    /// entry 3 from entry 4: their only difference is whose volume wins, and with the row at 1.0
    /// the two agree.
    pub fn add_table_rows(&mut self, id: DataId, rows: &[(u32, DataId, f32, f32)]) {
        let mut nodes = vec![SoundTableNode {
            key: 0,
            data: Vec::new(),
            children: Vec::new(),
        }];
        for (stype, wave, volume, probability) in rows {
            let idx = u32::try_from(nodes.len()).expect("small");
            nodes.push(SoundTableNode {
                key: *stype,
                data: vec![SoundEntry {
                    sound_id: *wave,
                    priority: 0.0,
                    probability: *probability,
                    volume: *volume,
                }],
                children: Vec::new(),
            });
            nodes[0].children.push(idx);
        }
        self.tables.insert(id, SoundTable { id, nodes });
    }

    /// Register the ambient descriptors this stub reports.
    pub fn set_descs(&mut self, descs: Vec<SoundDesc>) {
        self.descs = descs;
    }
}

impl AudioAssets for StubAssets {
    fn wave(&self, id: DataId) -> Option<(&Wave, &[u8])> {
        self.waves.get(&id).map(|w| (&w.wave, w.record.as_slice()))
    }

    fn sound_table(&self, id: DataId) -> Option<&SoundTable> {
        self.tables.get(&id)
    }

    fn ambient_desc(&self, index: usize) -> Option<&AmbientSoundDescriptor> {
        self.descs.get(index)
    }
}

/// A minimal [`Region`] with three terrain types:
///
/// * type 1, scene 0 -> descriptor 0, one **continuous** entry (`base_chance == 0`), volume 0.8,
///   `min_rate` 2.0;
/// * type 2, scene 0 -> descriptor 1, one continuous entry, volume 0.8;
/// * type 3, scene 0 -> descriptor 2, one **intermittent** entry, `base_chance` 1.0, `min_rate` 1.0,
///   `max_rate` 5.0.
///
/// The shapes are the decoders' view of the shipped region; the values are chosen to make the two
/// `AmbientSound` subclasses and the scene-share arithmetic visible in a test.
#[must_use]
pub fn test_region() -> Region {
    let sound_info = vec![
        SoundDesc {
            stb_id: DataId(0x2000_0010),
            ambient_sounds: vec![AmbientSound {
                stype: 70,
                volume: 0.8,
                base_chance: 0.0,
                min_rate: 2.0,
                max_rate: 4.0,
            }],
        },
        SoundDesc {
            stb_id: DataId(0x2000_0011),
            ambient_sounds: vec![AmbientSound {
                stype: 71,
                volume: 0.8,
                base_chance: 0.0,
                min_rate: 2.0,
                max_rate: 4.0,
            }],
        },
        SoundDesc {
            stb_id: DataId(0x2000_0012),
            ambient_sounds: vec![AmbientSound {
                stype: 72,
                volume: 0.6,
                base_chance: 1.0,
                min_rate: 1.0,
                max_rate: 5.0,
            }],
        },
    ];
    let scene_info = vec![
        SceneDesc {
            stb_index: 0,
            scenes: Vec::new(),
        },
        SceneDesc {
            stb_index: 1,
            scenes: Vec::new(),
        },
        SceneDesc {
            stb_index: 2,
            scenes: Vec::new(),
        },
    ];
    let terrain = |scenes: Vec<i32>| TerrainType {
        terrain_name: String::from("t"),
        terrain_color: 0,
        scene_types: scenes,
    };
    Region {
        id: DataId(0x1300_0000),
        region_number: 1,
        version: 0,
        region_name: String::from("test"),
        land_defs: LandDefs {
            num_block_length: 255,
            num_block_width: 255,
            square_length: 24.0,
            lblock_length: 8,
            vertex_per_cell: 1,
            max_obj_height: 200.0,
            sky_height: 1000.0,
            road_width: 5.0,
            land_height_table: vec![0.0; 256],
        },
        game_time: GameTime {
            zero_time_of_year: 0.0,
            zero_year: 0,
            day_length: 1.0,
            days_per_year: 1,
            year_spec: String::new(),
            times_of_day: Vec::new(),
            days_of_the_week: Vec::new(),
            seasons: Vec::new(),
        },
        parts_mask: 0x03,
        sky_info: None,
        sound_info: Some(sound_info),
        scene_info: Some(scene_info),
        // Index 0 is unused; types 1, 2 and 3 each carry one scene.
        terrain_types: vec![
            terrain(Vec::new()),
            terrain(vec![0]),
            terrain(vec![1]),
            terrain(vec![2]),
        ],
        land_surf: LandSurf {
            surf_type: 0,
            tex_merge: None,
            pal_shift: None,
        },
        region_misc: None,
    }
}
