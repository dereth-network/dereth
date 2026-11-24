//! Where each era's public world database is published, and how a copy of it is named in a cache.
//!
//! | Era | Database | Release |
//! |---|---|---|
//! | end of retail | ACE's world database with its patches (`ACEmulator/ACE-World-16PY-Patches`) | one release, pinned with the SHA-256 of its zip |
//! | Infiltration | ACE's February 2005 world database (`ACEmulator/ACE-World-16PY`) | one release, pinned the same way |
//! | Launch | the ac1999 world converted for Empyrean (`dereth-network/empyrean-world-launch`) | its newest release, checked against the SHA-256 the release lists |
//! | New Dereth | its own world (`dereth-network/empyrean-world-newdereth`), built as Launch's is | none yet |
//!
//! The first two releases are a zipped SQL dump. The Launch release's zip holds a built world with
//! the SQL it was built from, `world.sql`, beside it. A copy is kept as one SQL file per release:
//! the dump's own name for the first two, `<asset prefix><tag>.sql` for the Launch world, so a
//! cache a server's import tool keeps holds the same files under the same names.

use dereth_primitives::EraId;

/// What a release carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A zip holding the dump, `<asset prefix><tag>.sql.zip`.
    SqlZip,
    /// A zip holding a built world and the SQL it was built from (`world.sql`), named
    /// `<asset prefix><anything>.zip` beside a `-source.zip`.
    BuiltWorld,
}

/// One era's public world database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraDatabase {
    pub era: EraId,
    /// The GitHub repository whose releases carry it.
    pub repository: &'static str,
    pub asset_prefix: &'static str,
    /// The release used; empty for the newest.
    pub tag: &'static str,
    /// The SHA-256 of the pinned release's zip, lowercase hex; empty for the newest.
    pub zip_sha256: &'static str,
    pub form: Form,
}

/// The end of retail.
pub const EOR: EraDatabase = EraDatabase {
    era: EraId::Eor,
    repository: "ACEmulator/ACE-World-16PY-Patches",
    asset_prefix: "ACE-World-Database-",
    tag: "v0.9.296",
    zip_sha256: "e56c594197b03a96963b14e3db61ad205d47f63a69d0c1c72e9bef78878f180c",
    form: Form::SqlZip,
};

/// February 2005, the Infiltration patch.
pub const INFILTRATION: EraDatabase = EraDatabase {
    era: EraId::Infiltration,
    repository: "ACEmulator/ACE-World-16PY",
    asset_prefix: "ACE-World-16PY-db-",
    tag: "v0.8.8",
    zip_sha256: "bbd0396e4382c555388d89e2f341fd79cf3647e78a9ef34e5f8717ddd3d9cb49",
    form: Form::SqlZip,
};

/// November 1999, the launch.
pub const LAUNCH: EraDatabase = EraDatabase {
    era: EraId::Launch,
    repository: "dereth-network/empyrean-world-launch",
    asset_prefix: "empyrean-world-launch-",
    tag: "",
    zip_sha256: "",
    form: Form::BuiltWorld,
};

/// New Dereth, Emergence: its world database (`dereth-network/empyrean-world-newdereth`), built as
/// the Launch world is; nothing is released yet.
pub const NEW_DERETH: EraDatabase = EraDatabase {
    era: EraId::NewDereth,
    repository: "dereth-network/empyrean-world-newdereth",
    asset_prefix: "empyrean-world-newdereth-",
    tag: "",
    zip_sha256: "",
    form: Form::BuiltWorld,
};

/// Every era's database that has a release.
pub const ALL: [EraDatabase; 3] = [LAUNCH, INFILTRATION, EOR];

impl EraDatabase {
    /// The database of `era`.
    #[must_use]
    pub const fn of(era: EraId) -> &'static Self {
        match era {
            EraId::Launch => &LAUNCH,
            EraId::Infiltration => &INFILTRATION,
            EraId::Eor => &EOR,
            EraId::NewDereth => &NEW_DERETH,
        }
    }

    /// Whether one release is pinned.
    #[must_use]
    pub const fn pinned(&self) -> bool {
        !self.tag.is_empty()
    }

    /// The pinned release's zip: `<prefix><tag>.sql.zip`.
    #[must_use]
    pub fn zip_name(&self, tag: &str) -> String {
        format!("{}{tag}.sql.zip", self.asset_prefix)
    }

    /// The download address of the pinned release's zip.
    #[must_use]
    pub fn download_url(&self, tag: &str) -> String {
        format!(
            "https://github.com/{}/releases/download/{tag}/{}",
            self.repository,
            self.zip_name(tag)
        )
    }

    /// The address of the release list's newest release.
    #[must_use]
    pub fn latest_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repository
        )
    }

    /// Whether the asset named `name` is this database's zip.
    #[must_use]
    pub fn is_world_asset(&self, name: &str, tag: &str) -> bool {
        match self.form {
            Form::SqlZip => name == self.zip_name(tag),
            Form::BuiltWorld => {
                name.starts_with(self.asset_prefix)
                    && name.ends_with(".zip")
                    && !name.ends_with("-source.zip")
            }
        }
    }

    /// The name of release `tag`'s SQL in a cache: `<prefix><tag>.sql`.
    #[must_use]
    pub fn sql_name(&self, tag: &str) -> String {
        format!("{}{tag}.sql", self.asset_prefix)
    }

    /// Whether `member`, a path inside the release's zip, is its SQL.
    #[must_use]
    pub fn is_sql_member(&self, member: &str, tag: &str) -> bool {
        let file = member.rsplit('/').next().unwrap_or(member);
        match self.form {
            Form::SqlZip => file == self.sql_name(tag),
            Form::BuiltWorld => file == "world.sql",
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (where each era's database is fetched from and how it is named).
    use super::*;

    #[test]
    fn each_era_names_its_database_and_how_a_copy_is_kept() {
        assert_eq!(EraDatabase::of(EraId::Eor), &EOR);
        assert_eq!(
            EOR.download_url(EOR.tag),
            "https://github.com/ACEmulator/ACE-World-16PY-Patches/releases/download/v0.9.296/ACE-World-Database-v0.9.296.sql.zip"
        );
        assert_eq!(EOR.sql_name(EOR.tag), "ACE-World-Database-v0.9.296.sql");
        assert!(EOR.is_sql_member("ACE-World-Database-v0.9.296.sql", EOR.tag));
        assert_eq!(
            INFILTRATION.sql_name("v0.8.8"),
            "ACE-World-16PY-db-v0.8.8.sql"
        );
        assert!(!LAUNCH.pinned());
        assert!(LAUNCH.is_world_asset("empyrean-world-launch-3c564fdf.zip", "v0.3.0"));
        assert!(!LAUNCH.is_world_asset("empyrean-world-launch-3c564fdf-source.zip", "v0.3.0"));
        assert!(LAUNCH.is_sql_member("empyrean-world-launch/world.sql", "v0.3.0"));
        assert_eq!(
            LAUNCH.sql_name("v0.3.0"),
            "empyrean-world-launch-v0.3.0.sql"
        );
    }
}
