//! The natives scripts call: containers, strings, string references, the machine's own natives,
//! argument buffers, and a stand-in for every native not written yet.
//!
//! **Depends on** `auberean-script` (the machine and its native interface), `auberean-dat` (the
//! records natives read), `auberean-assets` (records decoded: entity properties, the property
//! list, the region), `auberean-physics` (a missile's parts, as the engine keeps them; a frame's
//! rotation), `auberean-primitives` (where a cell lies on the land),
//! `auberean-text` (string references written to text), `dereth-primitives` (numbers converted as the
//! client converts them) and `auberean-port` (the stand-ins' marker). **Used by** the client
//! runtime (`auberean-client-runtime`), a server and tests: each starts from [`natives`] and binds
//! its own engine natives over it.
//!
//! [`natives`] binds every native the September 2013 client binds, 2,145 ids: the ones written
//! here to their Rust functions, every other one to a stand-in that stops a run cleanly with the
//! native's id and the retail function it stands for ([`stubs`]). Containers are bound from
//! their packages' names: every hash, multi-hash, set, list and array package of the library gets
//! the same generic functions, which read element kinds from the object itself.
//!
//! - [`machine`]: the machine's own natives: singletons, packages, the current instance.
//! - [`adaptors`]: the objects scripts fill for the interface's panels.
//! - [`combat`]: what a hit shows: its effects and behaviors, and the floating text.
//! - [`missile`]: what a missile is: its flight, its aim and what it passes through.
//! - [`motion`]: how a body is asked to move, turn and be placed; headings and rays.
//! - [`story`]: the story's acts, scenes and keyframes, as the quest window's story page reads them.
//! - [`quests`]: a quest's record and the player's list of them, as the journal reads them.
//! - [`selection`]: a table of words picked at random by weight, each once until it starts over.
//! - [`entity`]: entity descriptions, filled from entity records and asked for properties.
//! - [`data_ids`]: the records a saved state needs, as its scripts gather them.
//! - [`behavior`]: the parameters of a behavior an object is asked to play.
//! - [`options`]: the player's options profile and its shortcuts.
//! - [`chargen`]: a character's creation choices, as the character generator's scripts fill them.
//! - [`chat_options`]: the chat's part of the options profile, as the chat windows read it.
//! - [`effects`]: an effect as the effects panel lists it.
//! - [`friends`]: a friend's state as the friends service gives it.
//! - [`position`]: positions (their parts, distances, land and map coordinates, text) and vectors.
//! - [`text`]: strings and the string-building natives.
//! - [`names`]: the client's table of file names: a record's id by its file's name, and back.
//! - [`containers`]: hashes, sets, lists and arrays, and their iterators.
//! - [`buffer`]: argument buffers, and sending a call to the other side.
//! - [`color`]: colors, set whole, by their alpha or as 32-bit colors, and attenuated.
//! - [`numbers`]: arithmetic (minimum, maximum, absolute value, floor, ceiling) and dice from the
//!   engine's random number generator.
//! - [`debug`]: the scripts' debug lines, for the host's console.
//!
//! **Must never** do I/O: what reaches beyond the script world goes through the host.

use auberean_script::library::Library;
use auberean_script::value::NativeId;
use auberean_script::{Binding, NativeFn, NativeTable};

pub mod adaptors;
pub mod behavior;
pub mod buffer;
pub mod chargen;
pub mod chat_options;
pub mod color;
pub mod combat;
pub mod containers;
pub mod data_ids;
pub mod debug;
pub mod effects;
pub mod entity;
pub mod enums;
pub mod examination;
pub mod friends;
pub mod machine;
pub mod missile;
pub mod motion;
pub mod names;
pub mod numbers;
pub mod options;
pub mod position;
pub mod properties;
pub mod quests;
pub mod selection;
pub mod skill_nodes;
pub mod story;
pub mod stubs;
pub mod text;

/// Every native the target client binds: the ported ones, and a stand-in for each of the rest.
#[must_use]
pub fn natives(lib: &Library) -> NativeTable {
    let mut t = NativeTable::new();
    stubs::bind_all(&mut t);
    machine::bind(&mut t);
    adaptors::bind(&mut t);
    chargen::bind(&mut t);
    combat::bind(&mut t);
    missile::bind(&mut t);
    motion::bind(&mut t);
    text::bind(&mut t);
    buffer::bind(&mut t);
    numbers::bind(&mut t);
    debug::bind(&mut t);
    story::bind(&mut t);
    quests::bind(&mut t);
    selection::bind(&mut t);
    entity::bind(&mut t);
    data_ids::bind(&mut t);
    enums::bind(&mut t);
    names::bind(&mut t);
    examination::bind(&mut t);
    behavior::bind(&mut t);
    color::bind(&mut t);
    options::bind(&mut t);
    effects::bind(&mut t);
    friends::bind(&mut t);
    position::bind(&mut t);
    properties::bind(&mut t);
    skill_nodes::bind(&mut t);
    containers::bind(&mut t, lib);
    t
}

/// Bind a module's table of natives: package, number, function.
pub(crate) fn bind_table(t: &mut NativeTable, natives: &[(u32, u32, NativeFn)]) {
    for (package, number, f) in natives {
        t.bind(NativeId::new(*package, *number), Binding::Ported(*f));
    }
}
