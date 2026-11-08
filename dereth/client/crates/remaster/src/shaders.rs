//! The remaster's shaders, compiled ahead of time and embedded.
//!
//! The Slang sources are in the crate's `shaders/` folder, with `shaders.toml` naming each entry
//! point. `cargo xtask shaders` compiles them into `shaders/compiled/`, which is checked in, so
//! building the crate needs no shader compiler; `cargo xtask shaders --check` fails when a source
//! changed without its blobs. [`Entry`] names every entry point, and [`spirv`] hands out its
//! SPIR-V.
//!
//! The blobs' conventions: matrices are column-major in memory and read as `mul(m, v)`; each
//! entry point keeps its function's name in the blob ([`Entry::name`]); resources are read through
//! the shared bindless table (`shaders/bindless.slang`: set 0, sampled images at binding 0,
//! samplers at 1, storage buffers read at 2 and read-written at 3) by indices in the push
//! constants.

use std::sync::OnceLock;

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
    WORDS[at].get_or_init(|| words(entry.spirv_bytes()))
}

/// Little-endian bytes as 32-bit words; a trailing partial word is dropped (a blob never has
/// one).
fn words(bytes: &[u8]) -> Box<[u32]> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The SPIR-V magic number, the first word of every module.
    const MAGIC: u32 = 0x0723_0203;

    /// `OpEntryPoint`'s opcode.
    const ENTRY_POINT: u32 = 15;

    /// The execution model SPIR-V gives a stage.
    fn model(stage: Stage) -> u32 {
        match stage {
            Stage::Vertex => 0,
            Stage::Fragment => 4,
            Stage::Compute => 5,
            Stage::Task => 5364,
            Stage::Mesh => 5365,
        }
    }

    /// Every `OpEntryPoint` of a module: its execution model and its name.
    fn entry_points(words: &[u32]) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        let mut at = 5;
        while at < words.len() {
            let count = (words[at] >> 16) as usize;
            if count == 0 || at + count > words.len() {
                break;
            }
            if words[at] & 0xFFFF == ENTRY_POINT {
                let name: Vec<u8> = words[at + 3..at + count]
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .take_while(|b| *b != 0)
                    .collect();
                out.push((words[at + 1], String::from_utf8_lossy(&name).into_owned()));
            }
            at += count;
        }
        out
    }

    /// **Every embedded blob is a SPIR-V 1.6 module whose one entry point is the function and
    /// stage `shaders.toml` names**, so a pipeline made from it finds the entry point by
    /// [`Entry::name`].
    #[test]
    fn every_blob_is_a_spirv_module_with_its_named_entry_point() {
        assert!(!Entry::ALL.is_empty());
        for e in Entry::ALL {
            let w = spirv(e);
            assert_eq!(w.len() * 4, e.spirv_bytes().len(), "{e:?}: whole words");
            assert_eq!(w[0], MAGIC, "{e:?}");
            assert_eq!(w[1], 0x0001_0600, "{e:?}: SPIR-V 1.6, as Vulkan 1.3 takes");
            assert_eq!(
                entry_points(w),
                [(model(e.stage()), e.name().to_owned())],
                "{e:?}"
            );
            assert!(e.source().ends_with(".slang"));
            assert!(std::ptr::eq(spirv(e), w), "{e:?}: read once and kept");
        }
    }
}
