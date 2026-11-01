//! Athenaeum's world machinery without an install: worlds over a base, their edits as pack text,
//! undo, redo and save, and their counts.
mod edit;
mod world;

use std::path::PathBuf;

use athenaeum_foundry::Session;
use dereth_pack::store::{MemoryStore, SourceStore};
use yalain_world_db::layer::Base;
use yalain_world_db::Files;

/// A small base database: a person, a camp and a portal in landblock 77B8, and the place the
/// portal leads to.
pub fn base_files() -> Files {
    let mut f = Files::new();
    f.insert(
        "world.toml".into(),
        "format_version = 1\nkey = \"yalain-world\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"seed\"]\n".into(),
    );
    f.insert(
        "seed/pack.toml".into(),
        "format_version = 1\nname = \"seed\"\nversion = \"0.1.0\"\ngame = \"ac2\"\nauthors = [\"tests\"]\nlicense = \"CC0-1.0\"\n".into(),
    );
    f.insert(
        "seed/server/landblocks/77B8.toml".into(),
        r#"# Written by the seed builder.

[[object]]
key = "n-334c"
kind = "npc"
template = 0x4700334C
cell = 0x77B80006
at = [1.3796, 100.7791, 182.505]
rotation = [0.249883, 0.0, 0.0, 0.968276]
prov = { from = "capture", evidence = "TEST-OBJECTS", ref = "c:d23a14c8", confidence = "high", sample = { sessions = 28 } }

[[object]]
key = "s-5c"
kind = "static"
id = 0x4077B8000000005C
template = 0x4700115B
at = [120.0, 110.0, 150.0]
heading = 90.0
to = "p-1"
prov = { from = "capture", evidence = "TEST-OBJECTS", ref = "c:00000001", confidence = "medium", sample = { sessions = 2 } }

[[camp]]
key = "c-4586"
cell = 0x77B80040
at = [144.8, 144.7, 133.0]
radius = 5.0
population = 2
regen = 192.0
spawns = [{ template = 0x47002224, weight = 2 }]
prov = { from = "capture", evidence = "TEST-CAMPS", ref = "camp 4586", confidence = "low", sample = { creatures = 2, sessions = 1 } }
"#
        .into(),
    );
    f.insert(
        "seed/server/places.toml".into(),
        r#"[[place]]
key = "p-1"
cell = 0x77B80003
at = [1.18, 53.42, 183.82]
heading = 320.5
prov = { from = "capture", evidence = "TEST-PLACES", ref = "p:1", confidence = "high" }
"#
        .into(),
    );
    f
}

/// The base, read.
pub fn base() -> Base {
    Base::new(&base_files()).expect("the test base reads")
}

/// A new world's files, as `athenaeum new` makes them.
pub fn world_files() -> Files {
    athenaeum_foundry::new::files("test").into_iter().collect()
}

/// The world's folder in the store.
pub fn dir() -> PathBuf {
    PathBuf::from("w")
}

/// A store holding `files` under the world's folder.
pub fn store(files: &Files) -> MemoryStore {
    let s = MemoryStore::default();
    for (rel, text) in files {
        s.write(&dir().join(rel), text.as_bytes()).unwrap();
    }
    s
}

/// A session over a new world.
pub fn session() -> Session {
    let files = world_files();
    Session::new(dir(), Box::new(store(&files)), files, base(), None).expect("a session")
}
