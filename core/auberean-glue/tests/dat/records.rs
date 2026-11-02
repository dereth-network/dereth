//! The records each saved state names, asked as the client asks when it loads one: every state
//! of the install, through the library's own code and every native.

use std::any::Any;
use std::sync::Arc;

use auberean_assets::Database;
use auberean_glue::data_ids::state_records;
use auberean_script::graph::{Body, SavedState};
use auberean_script::native_object::NativeObject;
use auberean_script::{Host, Vm};

use crate::world::{target, World};

/// The record type the client asks about when it loads an object's state.
const ASKED: u32 = 0x4000_0009;
/// The character generator's data, whose state holds every race and sex's look.
const CHARACTER_GENERATION: u32 = 0x7000_0390;

/// The install's saved states, and its records for the enumerations scripts look records up by.
struct Data {
    world: World,
    db: Database,
}

impl Host for Data {
    fn saved_state(&mut self, id: u32) -> Option<Arc<SavedState>> {
        self.world.saved_state(id)
    }

    fn data_id_by_enum(&mut self, value: u32, mapping: u32) -> u32 {
        self.db.data_id_by_enum(value, mapping)
    }

    fn database(&mut self) -> Option<&Database> {
        Some(&self.db)
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// The native objects a state's graph packs, as the engine holds them.
fn natives(s: &SavedState) -> impl Iterator<Item = NativeObject> + '_ {
    s.graph.entries.iter().filter_map(|e| match &e.body {
        Body::Object(o) => o.native.as_ref().map(NativeObject::unpacked),
        _ => None,
    })
}

/// **Every saved state names the records it needs without stopping**: each of the install's
/// 41,098 states, made a process and asked as the client asks when it loads an object's state,
/// runs the scripts' answer to its end (40,937), or is no preprocessed package and is not asked
/// (161); the character generator's state names each of its 10 race and sex looks' body, pile look
/// and music, and every state that packs a scene (95 scenes) names each of its keyframes' tracks
/// and subtitle tables.
/// Fixture: the September 2013 install's library and saved states.
/// Behaviour: script.glue.every-saved-state-names-its-records
#[test]
fn every_saved_state_names_the_records_it_needs_without_stopping() {
    let (dat, prog) = target().expect("the September 2013 install opens");
    let mut data = Data {
        world: World::new(dat, prog),
        db: Database::new(vec![Arc::clone(dat)]),
    };
    let ids: Vec<u32> = dat
        .entries_in(0x6F00_0000, 0x7FFF_FFFF)
        .map(|e| e.id)
        .collect();
    let (mut asked, mut not_asked, mut scenes, mut looks) = (0, 0, 0, 0);
    let mut stopped = Vec::new();
    for &id in &ids {
        let mut vm = Vm::new(Arc::clone(prog));
        let set = match state_records(&mut vm, &mut data, id, ASKED) {
            Ok(Some(set)) => set,
            Ok(None) => {
                not_asked += 1;
                continue;
            }
            Err(e) => {
                stopped.push(format!("{id:#x}: {e}"));
                continue;
            }
        };
        asked += 1;
        let state = data.saved_state(id).expect("the state reads");
        for o in natives(&state) {
            match o {
                NativeObject::Scene(s) => {
                    scenes += 1;
                    for k in &s.keyframes {
                        for t in [k.video, k.audio, k.voice].into_iter().filter(|&t| t != 0) {
                            assert!(set.contains(t), "{id:#x}: a keyframe's track {t:#x}");
                        }
                        if k.subtitle.has_literal == 0 && k.subtitle.table != 0 {
                            assert!(set.contains(k.subtitle.table), "{id:#x}: a subtitle");
                        }
                    }
                }
                NativeObject::Packed { ty: 0xA2, bytes } if id == CHARACTER_GENERATION => {
                    looks += 1;
                    for w in bytes.chunks(4).take(3) {
                        let w = u32::from_le_bytes(w.try_into().expect("a word"));
                        assert!(set.contains(w), "a race and sex's record {w:#x}");
                    }
                }
                _ => {}
            }
        }
    }
    assert!(
        stopped.is_empty(),
        "{} stopped: {:?}",
        stopped.len(),
        &stopped[..stopped.len().min(5)]
    );
    assert_eq!((ids.len(), asked, not_asked), (41_098, 40_937, 161));
    assert_eq!(scenes, 95, "the scenes the states pack");
    assert_eq!(looks, 10, "the race and sex looks");
}
