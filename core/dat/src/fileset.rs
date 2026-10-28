//! A set of data files by the role each plays: the portal file, the cell file, the language file
//! and the high-resolution portal partition ([`ModernDat`] names the four roles), with the name
//! each has on disk and the layout their containers are in.
//!
//! A game's files fill the same four roles under names of its own, and some sets have no file for
//! a role (the files from before Throne of Destiny kept the language file's records in the portal
//! file, and had no high-resolution one). A [`FileSet`] is that as a value: a tool that builds,
//! bakes or takes apart a set of files works by role and asks the set for each file's name, so a
//! set it has never seen is a different value, not different code.

use std::path::{Path, PathBuf};

use crate::layout::ContainerLayout;
use crate::locate::{ClassicDat, ModernDat};
use dereth_primitives::ContainerEra;

/// A set of data files: the name each role's file has on disk (`None` where the set has no file
/// for the role), and the layout every container of the set is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileSet {
    /// By role, in [`ModernDat::ALL`]'s order.
    names: [Option<&'static str>; 4],
    layout: ContainerLayout,
}

const fn slot(role: ModernDat) -> usize {
    match role {
        ModernDat::Portal => 0,
        ModernDat::Cell => 1,
        ModernDat::Local => 2,
        ModernDat::HighRes => 3,
    }
}

impl FileSet {
    /// The end of retail's files: `client_portal.dat`, `client_cell_1.dat`,
    /// `client_local_English.dat` and `client_highres.dat`.
    pub const MODERN: Self = Self {
        names: [
            Some(ModernDat::Portal.file_name()),
            Some(ModernDat::Cell.file_name()),
            Some(ModernDat::Local.file_name()),
            Some(ModernDat::HighRes.file_name()),
        ],
        layout: ContainerLayout::MODERN,
    };

    /// The files from before Throne of Destiny: `portal.dat` and `cell.dat`, the language file's
    /// records in the portal file and no high-resolution file.
    pub const CLASSIC: Self = Self {
        names: [
            Some(ClassicDat::Portal.file_name()),
            Some(ClassicDat::Cell.file_name()),
            None,
            None,
        ],
        layout: ContainerLayout::CLASSIC,
    };

    /// A set of files of its own: the name of each role's file, in [`ModernDat::ALL`]'s order
    /// (`None` where the set has none), every container in `layout`. A set has a portal and a
    /// cell file at least.
    #[must_use]
    pub const fn new(names: [Option<&'static str>; 4], layout: ContainerLayout) -> Self {
        Self { names, layout }
    }

    /// The set of files in the `era`'s layout that this crate knows: [`Self::MODERN`] or
    /// [`Self::CLASSIC`].
    #[must_use]
    pub const fn of_era(era: ContainerEra) -> Self {
        match era {
            ContainerEra::Modern => Self::MODERN,
            ContainerEra::Classic => Self::CLASSIC,
        }
    }

    /// The layout every container of the set is in.
    #[must_use]
    pub const fn layout(&self) -> &ContainerLayout {
        &self.layout
    }

    /// The name on disk of the file that plays `role`, when the set has one.
    #[must_use]
    pub const fn name(&self, role: ModernDat) -> Option<&'static str> {
        self.names[slot(role)]
    }

    /// The name on disk of the file that holds `role`'s records: its own, or the portal file's
    /// for a role the set keeps there.
    #[must_use]
    pub const fn holder_name(&self, role: ModernDat) -> &'static str {
        match self.name(role) {
            Some(name) => name,
            None => match self.name(ModernDat::Portal) {
                Some(name) => name,
                None => "",
            },
        }
    }

    /// The path in `dir` of the file that holds `role`'s records ([`Self::holder_name`]).
    #[must_use]
    pub fn holder_in_dir(&self, role: ModernDat, dir: &Path) -> PathBuf {
        dir.join(self.holder_name(role))
    }

    /// The path in `dir` of the file that plays `role`, when the set has one.
    #[must_use]
    pub fn in_dir(&self, role: ModernDat, dir: &Path) -> Option<PathBuf> {
        self.name(role).map(|n| dir.join(n))
    }

    /// The roles the set has a file for, in the order the client opens them.
    pub fn roles(&self) -> impl Iterator<Item = ModernDat> + '_ {
        ModernDat::ALL
            .into_iter()
            .filter(|r| self.name(*r).is_some())
    }

    /// The role `name` plays in the set, compared without case.
    #[must_use]
    pub fn role_of(&self, name: &str) -> Option<ModernDat> {
        self.roles()
            .find(|r| self.name(*r).is_some_and(|n| n.eq_ignore_ascii_case(name)))
    }

    /// Whether `dir` holds the set: its portal and cell files.
    #[must_use]
    pub fn holds(&self, dir: &Path) -> bool {
        [ModernDat::Portal, ModernDat::Cell]
            .into_iter()
            .all(|r| self.in_dir(r, dir).is_some_and(|p| p.is_file()))
    }
}
