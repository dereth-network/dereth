//! The target install's records, opened once per test binary, and a device to draw them on.

use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_gpu::Renderer;
use auberean_world::records::WorldRecords;

/// The records of the target install and the ids its portal file holds; the test fails when the
/// install is not found.
pub struct Install {
    pub records: WorldRecords,
    pub ids: Vec<u32>,
}

pub fn install() -> &'static Install {
    static INSTALL: OnceLock<Install> = OnceLock::new();
    INSTALL.get_or_init(|| {
        let dir = require_target_install();
        let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
            .expect("list the install")
            .into_iter()
            .map(|(n, p)| {
                (
                    n,
                    DatFile::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
                )
            })
            .collect();
        let mut take = |name: &DatName| {
            let k = files.iter().position(|(n, _)| n == name)?;
            Some(files.swap_remove(k).1)
        };
        let portal = take(&DatName::Portal).expect("a portal file");
        let cell = take(&DatName::Cell(1)).expect("a cell file");
        let highres = take(&DatName::HighRes);
        let ids = portal.entries().map(|e| e.id).collect();
        let ctx = Context::from_portal(&portal).expect("the property list");
        Install {
            records: WorldRecords::new(portal, cell, highres, ctx),
            ids,
        }
    })
}

/// The ids of one record kind (the top byte of its ids).
pub fn ids_of(kind: u8) -> Vec<u32> {
    install()
        .ids
        .iter()
        .copied()
        .filter(|id| id >> 24 == u32::from(kind))
        .collect()
}

/// A renderer on the machine's hardware adapter, as the client makes its own, held by one test at
/// a time: a device made while another test draws on its own has stalled the driver (both threads
/// waiting, the hardware idle), so a test has the machine's device to itself from making its
/// renderer until it lets it go. The tests' other work runs on the binary's threads meanwhile.
pub fn renderer() -> Held {
    let turn = turn();
    let renderer = Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
    eprintln!("adapter: {}", renderer.adapter);
    Held {
        renderer,
        _turn: turn,
    }
}

fn turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// [`renderer`], the edges smoothed or not.
pub fn renderer_antialiased(antialiasing: bool) -> Held {
    let turn = turn();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = Renderer::antialiased(&instance, None, antialiasing)
        .unwrap_or_else(|e| panic!("a hardware device: {e}"));
    Held {
        renderer,
        _turn: turn,
    }
}

/// A test's renderer and its turn on the device: the renderer is let go of before the turn.
pub struct Held {
    renderer: Renderer,
    _turn: MutexGuard<'static, ()>,
}

impl Deref for Held {
    type Target = Renderer;
    fn deref(&self) -> &Renderer {
        &self.renderer
    }
}

impl DerefMut for Held {
    fn deref_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }
}

/// Run `f` with the device's validation errors caught, and fail with the first one.
pub fn checked<T>(r: &mut Renderer, f: impl FnOnce(&mut Renderer) -> T) -> T {
    let scope = r.device().push_error_scope(wgpu::ErrorFilter::Validation);
    let out = f(r);
    if let Some(e) = pollster::block_on(scope.pop()) {
        panic!("the device refused it: {e}");
    }
    out
}
