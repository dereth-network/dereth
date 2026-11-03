//! A world compiled against an install into the server's tables, as the server compiles the world
//! it serves: checked, merged over its base, every data id resolved against the install, and the
//! people and objects described from their templates.
//!
//! The tables are written as text for reading and comparing (`content.tsv`, `spawns.tsv`,
//! `loot.tsv`, `stores.tsv`, `quests.tsv`, `talk.tsv`); the server needs only the world's folder
//! and compiles it itself at start. Compiled tables copy values out of the install, so they stay
//! on the machine that made them.

use std::path::Path;
use std::sync::OnceLock;

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_script::library::{Library, RECORD};
use yalain_world_db::compile::{compile, Install, Tables};
use yalain_world_db::install::DatInstall;
use yalain_world_db::merge::Merged;
use yalain_world_db::{layer, report, rows};

use crate::world::Opened;

/// An install's data files, open.
pub struct InstallFiles {
    pub portal: DatFile,
    pub local: Option<DatFile>,
    pub ctx: Context,
    states: Vec<u32>,
    /// The script library, read at the first compile and shared by the ones after it.
    library: OnceLock<Library>,
}

impl std::fmt::Debug for InstallFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallFiles").finish_non_exhaustive()
    }
}

impl InstallFiles {
    /// The install in `dir`.
    ///
    /// # Errors
    /// The folder holds no portal file, or it does not read.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let found = install_dats(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
            found
                .iter()
                .find(|(n, _)| want(n))
                .and_then(|(_, p)| DatFile::open(p).ok())
        };
        let portal = open(&|n| *n == DatName::Portal)
            .ok_or_else(|| format!("no portal data file in {}", dir.display()))?;
        let local = open(&|n| matches!(n, DatName::Local(l) if l.eq_ignore_ascii_case("english")));
        Self::of(portal, local)
    }

    /// An install of these open files.
    ///
    /// # Errors
    /// The portal file does not read.
    pub fn of(portal: DatFile, local: Option<DatFile>) -> Result<Self, String> {
        let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
        let states = portal
            .entries_in(0x6F00_0000, 0x6FFF_FFFF)
            .map(|e| e.id)
            .collect();
        Ok(Self {
            portal,
            local,
            ctx,
            states,
            library: OnceLock::new(),
        })
    }

    /// The install's script library, read once. A read or decode that fails is checked against the
    /// file and read once more; a second read that succeeds is warned of.
    fn library(&self) -> Result<&Library, String> {
        if let Some(lib) = self.library.get() {
            return Ok(lib);
        }
        let lib = self
            .portal
            .read_checked(RECORD, Library::from_record)
            .map_err(|e| format!("the script library: {e}"))?;
        if let Some(r) = lib.retried {
            tracing::warn!("the script library: {r}");
        }
        Ok(self.library.get_or_init(|| lib.value))
    }

    /// What a compile reads of the install.
    ///
    /// # Errors
    /// The script library's field names do not read.
    pub fn install(&self) -> Result<DatInstall<'_>, String> {
        let mut install = DatInstall::new(
            Box::new(|id| self.portal.read(id).ok()),
            self.ctx.clone(),
            self.library()?,
            self.states.clone(),
        )?;
        if let Some(local) = &self.local {
            install = install.with_text(Box::new(move |id| local.read(id).ok()));
        }
        Ok(install)
    }
}

/// The world merged over its base.
///
/// # Errors
/// Every problem the world has: it must check before it builds.
pub fn merged(world: &Opened) -> Result<Merged, String> {
    let problems = world.check();
    if !problems.is_empty() {
        return Err(format!(
            "{}\n{} problems in {}",
            report(&problems),
            problems.len(),
            world.dir.display()
        ));
    }
    let db = rows::read(&world.files).map_err(|p| report(&p))?;
    layer::merge_over(&world.base, &db).map_err(|p| report(&p))
}

/// Compile the world against `install`.
///
/// # Errors
/// The world does not check or merge, or a row names what the install lacks.
pub fn compile_world(world: &Opened, install: &dyn Install) -> Result<Tables, String> {
    let merged = merged(world)?;
    compile(&merged, install).map_err(|p| {
        format!(
            "{}\n{} problems compiling {}",
            report(&p),
            p.len(),
            world.dir.display()
        )
    })
}

/// The tables as the text files a build writes, by name.
#[must_use]
pub fn texts(t: &Tables) -> [(&'static str, String); 6] {
    [
        ("content.tsv", t.content.to_text()),
        ("spawns.tsv", t.spawns.to_text()),
        ("loot.tsv", t.loot.to_text()),
        ("stores.tsv", t.stores.to_text()),
        ("quests.tsv", t.quests.to_text()),
        ("talk.tsv", t.talk.to_text()),
    ]
}

/// Write the tables into `out`.
///
/// # Errors
/// A file will not write.
pub fn write(t: &Tables, out: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in texts(t) {
        let p = out.join(name);
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}
