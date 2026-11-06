//! What the calls run against: the target install's library and saved states, linked with every
//! native, and the engine objects a session creates, read from its messages.
//! Behaviour: none (test support)

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_script::graph::SavedState;
use auberean_script::library::{Library, RECORD};
use auberean_script::native_object::{native_type, step_over};
use auberean_script::states::StateCache;
use auberean_script::wire::In;
use auberean_script::{Host, Program, Stop};

/// The linked program and the portal file it came from.
pub fn target() -> Option<&'static (Arc<DatFile>, Arc<Program>)> {
    static T: OnceLock<Option<(Arc<DatFile>, Arc<Program>)>> = OnceLock::new();
    T.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).expect("open the portal file");
        let lib = dat
            .read_checked(RECORD, Library::from_record)
            .expect("the library")
            .value;
        let natives = auberean_glue::natives(&lib);
        let prog = Program::link(lib, natives).expect("the library links against the natives");
        Some((Arc::new(dat), Arc::new(prog)))
    })
    .as_ref()
}

/// A host with the install's saved states, collecting what scripts send.
pub struct World {
    pub states: StateCache,
    pub program: Arc<Program>,
    pub sent: Vec<(u32, Vec<u8>)>,
    /// How many sends went through the encrypted carrier.
    pub encrypted: usize,
}

impl World {
    pub fn new(dat: &Arc<DatFile>, program: &Arc<Program>) -> Self {
        Self {
            states: StateCache::new(Arc::clone(dat)),
            program: Arc::clone(program),
            sent: Vec::new(),
            encrypted: 0,
        }
    }
}

impl Host for World {
    fn saved_state(&mut self, id: u32) -> Option<Arc<SavedState>> {
        self.states.get(id, &self.program.library)
    }

    fn send(&mut self, function: u32, args: &[u8], encrypted: bool) -> Result<(), Stop> {
        self.sent.push((function, args.to_vec()));
        self.encrypted += usize::from(encrypted);
        Ok(())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// Message types this harness reads besides the script calls.
pub const CREATE_OBJECT: u32 = 0x10058;
pub const CREATE_PLAYER: u32 = 0x10060;

/// An engine object as a session creates it: its package and entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Made {
    pub package: u32,
    pub entity: u32,
}

/// The player's id from a player creation: a 64-bit id after the type word.
pub fn player_id(raw: &[u8]) -> Option<u64> {
    let mut r = In::new(raw);
    r.u32().ok()?;
    r.u64().ok()
}

/// The id, package and entity an object creation names: the id, the visual description, the
/// physics description, then the weenie description's mask with the package (bit 0x1) and the
/// entity (bit 0x2000000).
pub fn created(raw: &[u8]) -> Option<(u64, Made)> {
    let mut r = In::new(raw);
    r.u32().ok()?;
    let id = r.u64().ok()?;
    r.align().ok()?;
    step_over(&mut r, native_type::VISUAL_DESC).ok()?;
    physics(&mut r)?;
    let mask = r.u32().ok()?;
    let package = if mask & 1 != 0 { r.u32().ok()? } else { 0 };
    let entity = if mask & 0x200_0000 != 0 {
        r.u32().ok()?
    } else {
        0
    };
    Some((id, Made { package, entity }))
}

fn words(r: &mut In<'_>, n: usize) -> Option<()> {
    r.bytes(4 * n).ok().map(|_| ())
}

/// Step over a physics description.
fn physics(r: &mut In<'_>) -> Option<()> {
    let bits = r.u32().ok()?;
    words(r, 1)?;
    let has = |b: u32| bits & b != 0;
    if has(0x80) {
        words(r, 1)?;
    }
    if has(0x1) {
        let n = r.u32().ok()?;
        for _ in 0..n {
            step_over(r, native_type::BEHAVIOR_PARAMS).ok()?;
        }
    }
    if has(0x2) {
        let n = r.u32().ok()? as usize;
        words(r, 3 * n)?;
    }
    if has(0x20000) {
        words(r, 1)?;
    }
    if has(0x8000) {
        words(r, 8)?;
    }
    if has(0x20) {
        words(r, 3)?;
        r.u16().ok()?;
        r.align().ok()?;
    }
    if has(0x100) {
        words(r, 1)?;
    }
    for (b, n) in [
        (0x4, 3),
        (0x40_0000, 3),
        (0x80_0000, 1),
        (0x100_0000, 1),
        (0x8, 3),
        (0x10, 3),
        (0x200_0000, 2),
    ] {
        if has(b) {
            words(r, n)?;
        }
    }
    for (b, n) in [
        (0x400_0000, 1),
        (0x800_0000, 1),
        (0x400, 2),
        (0x800, 8),
        (0x1000, 3),
    ] {
        if has(b) {
            words(r, n)?;
        }
    }
    if has(0x200) {
        let n = r.u32().ok()? as usize;
        words(r, 4 * n)?;
    }
    words(r, 2)?;
    r.align().ok()
}

/// The objects a session has created so far, by id, and its player's id.
#[derive(Default)]
pub struct Session {
    pub objects: HashMap<u64, Made>,
    pub player: Option<u64>,
}
