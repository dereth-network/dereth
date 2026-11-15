//! The remaster's shaders, compiled ahead of time and embedded.
//!
//! The Slang sources are in the crate's `shaders/` folder, with `shaders.toml` naming each entry
//! point; the shared passes' modules (`bindless`, `frame`) are imported from their folder.
//! `cargo xtask shaders` compiles them into `shaders/compiled/`, which is checked in, so building
//! the crate needs no shader compiler; `cargo xtask shaders --check` fails when a source changed
//! without its blobs. [`Entry`] names every entry point and [`shader`] hands out its blobs as the
//! layer takes them.

use std::sync::OnceLock;

use dereth_gfx::Shader;

/// The stage an entry point runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Vertex,
    Fragment,
    Compute,
    Mesh,
    Task,
}

include!("../shaders/compiled/shaders.rs");

/// The SPIR-V words of `entry`. The embedded bytes keep no alignment, so each blob is read into
/// words once, on first use, and kept.
#[must_use]
pub fn spirv(entry: Entry) -> &'static [u32] {
    static WORDS: [OnceLock<Box<[u32]>>; Entry::ALL.len()] =
        [const { OnceLock::new() }; Entry::ALL.len()];
    let at = Entry::ALL
        .iter()
        .position(|e| *e == entry)
        .expect("every entry point is in `Entry::ALL`");
    WORDS[at].get_or_init(|| {
        entry
            .spirv_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect()
    })
}

/// `entry` as the layer makes a pipeline stage of it: its SPIR-V, its DXIL and its name.
#[must_use]
pub fn shader(entry: Entry) -> Shader<'static> {
    Shader {
        spirv: spirv(entry),
        dxil: entry.dxil_bytes(),
        entry: entry.name(),
    }
}
