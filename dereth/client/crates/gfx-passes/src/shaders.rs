//! The shared passes' shaders, compiled ahead of time and embedded.
//!
//! The Slang sources are in the crate's `shaders/` folder, with `shaders.toml` naming each entry
//! point. `cargo xtask shaders` compiles them into `shaders/compiled/`, which is checked in, so
//! building the crate needs no shader compiler; `cargo xtask shaders --check` fails when a source
//! changed without its blobs. [`Entry`] names every entry point, [`spirv`] hands out its SPIR-V
//! and [`shader`] the layer's view of all its blobs.
//!
//! The blobs' conventions are the remasters': matrices are column-major in memory and read as
//! `mul(m, v)`; each entry point keeps its function's name in the blob ([`Entry::name`]);
//! resources are read through the layer's bindless table (`shaders/bindless.slang`: set 0, sampled
//! images at binding 0, samplers at 1, storage buffers read at 2 and read-written at 3) by indices
//! in the push constants.

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

/// The vertex stage of a triangle that covers the whole target, for a pass that reads one picture
/// and writes another by pixel.
#[must_use]
pub fn fullscreen() -> Shader<'static> {
    shader(Entry::FullscreenVsMain)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// **Every entry point's SPIR-V is a module that names it, in its stage**, and every one has
    /// a DXIL blob beside it.
    #[test]
    fn every_entry_point_has_a_spirv_module_naming_it_and_a_dxil_blob() {
        const MAGIC: u32 = 0x0723_0203;
        const ENTRY_POINT: u32 = 15;
        for e in Entry::ALL {
            let words = spirv(e);
            assert_eq!(words.first(), Some(&MAGIC), "{e:?}");
            let model = match e.stage() {
                Stage::Vertex => 0,
                Stage::Fragment => 4,
                Stage::Compute => 5,
                Stage::Task => 5364,
                Stage::Mesh => 5365,
            };
            let mut at = 5;
            let mut found = false;
            while at < words.len() {
                let count = (words[at] >> 16) as usize;
                if words[at] & 0xFFFF == ENTRY_POINT && words[at + 1] == model {
                    let name: Vec<u8> = words[at + 3..at + count]
                        .iter()
                        .flat_map(|w| w.to_le_bytes())
                        .take_while(|b| *b != 0)
                        .collect();
                    found |= name == e.name().as_bytes();
                }
                at += count.max(1);
            }
            assert!(found, "{e:?} names {} in its stage", e.name());
            assert!(!e.dxil_bytes().is_empty(), "{e:?}");
        }
    }
}
