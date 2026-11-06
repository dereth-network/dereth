//! The worlds whose servers want more than the end-of-retail client, which the launcher knows by
//! their community-list id.
//!
//! A few worlds run their own client: its own logon version, data files of their own over the
//! end-of-retail ones, and a few rules of their own. Their servers say none of it, and the
//! community list has no place for it, so the launcher keeps it here, keyed by the list's id
//! ([`World::list_id`]), which never changes. The player points the launcher at the world's files
//! (from the world's own download page, which [`KnownWorld::files_note`] names); the launcher
//! downloads nothing.

use dereth_primitives::{EraFeatureOverrides, EraFeatures, EraId};

use crate::datset::Iterations;
use crate::world::{CustomDats, Emulator, Told, World};

/// One world the launcher knows more about than its list row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownWorld {
    /// The community list's id, compared without regard to case.
    pub list_id: &'static str,
    /// The name the list gives it, for reading this table.
    pub name: &'static str,
    /// The server software, which the list may name as plain ACE.
    pub emulator: Emulator,
    /// The logon version string its server accepts.
    pub logon_version: &'static str,
    /// Whether that string is confirmed (the server's own source, or its client) or assumed.
    pub logon_confirmed: bool,
    /// The era its rules are.
    pub era: EraId,
    /// The systems it has. Its server does not announce them.
    pub features: EraFeatures,
    /// The data set its world is drawn from: `modern`, the later files, for every world here,
    /// whose own files are the later files with the world's changes over them.
    pub world_base: &'static str,
    /// What its own files report, file by file.
    pub expected: Iterations,
    /// The client rules it plays by (`--world-profile`).
    pub profile: &'static str,
    /// Where its own files come from, for the player.
    pub files_url: &'static str,
    pub files_note: &'static str,
}

/// The two ClassicACE CustomDM worlds' files.
const CUSTOMDM_FILES: Iterations = Iterations {
    portal: Some(20044),
    cell: Some(20011),
    local: Some(20011),
    highres: Some(497),
};

/// ClassicDereth's files: the end-of-retail portal count, and its own cell. Its servers compare
/// portal and cell only.
const CLASSICDERETH_FILES: Iterations = Iterations {
    portal: Some(2072),
    cell: Some(4),
    local: None,
    highres: None,
};

/// The systems a CustomDM world has: Infiltration's, with cloaks and trinkets, which its server
/// equips.
const CUSTOMDM_FEATURES: EraFeatures = EraFeatures {
    cloaks: true,
    trinkets: true,
    ..EraFeatures::INFILTRATION
};

/// Every world the launcher knows this way.
pub const KNOWN_WORLDS: &[KnownWorld] = &[
    KnownWorld {
        list_id: "4db379b0-587a-4775-9b7f-879e0f15ba4b",
        name: "Dekarutide",
        emulator: Emulator::ClassicAce,
        logon_version: "c118",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: CUSTOMDM_FEATURES,
        world_base: "modern",
        expected: CUSTOMDM_FILES,
        profile: "classicace-customdm",
        files_url: "https://www.dekarutide.com/",
        files_note: "Dekarutide plays with its own data files, the ClassicACE CustomDM set. Get \
                     them from Dekarutide's own download page, unpack them into a folder of their \
                     own, and choose that folder here.",
    },
    KnownWorld {
        list_id: "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1",
        name: "Unfamiliar Shores",
        emulator: Emulator::ClassicAce,
        logon_version: "c118",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: CUSTOMDM_FEATURES,
        world_base: "modern",
        expected: CUSTOMDM_FILES,
        profile: "classicace-customdm",
        files_url: "https://github.com/bDekaru/ClassicACE#required-data-files",
        files_note: "Unfamiliar Shores plays with the ClassicACE CustomDM data files. Get them \
                     from the CustomDM download the ClassicACE project's page links, unpack them \
                     into a folder of their own, and choose that folder here.",
    },
    KnownWorld {
        list_id: "394c58d0-885d-466b-b17f-d7e0b96fe3e2",
        name: "Seedsow",
        emulator: Emulator::Gdle,
        logon_version: "1802",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: EraFeatures::INFILTRATION,
        world_base: "modern",
        expected: CLASSICDERETH_FILES,
        profile: "classicdereth",
        files_url: "https://github.com/bDekaru/ClassicDereth",
        files_note: "Seedsow plays with the ClassicDereth portal and cell files. Get them from \
                     the download the ClassicDereth project's page links (or Seedsow's own \
                     setup post), put them beside a copy of the end-of-retail files in a folder \
                     of their own, and choose that folder here.",
    },
    KnownWorld {
        list_id: "eea962c0-cf9f-481e-9736-0eb058acc1d8",
        name: "Snowreap",
        emulator: Emulator::Gdle,
        logon_version: "1802",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: EraFeatures::INFILTRATION,
        world_base: "modern",
        expected: CLASSICDERETH_FILES,
        profile: "classicdereth",
        files_url: "https://github.com/bDekaru/ClassicDereth",
        files_note: "Snowreap plays with the ClassicDereth portal and cell files. Get them from \
                     the download the ClassicDereth project's page links (or Seedsow's own \
                     setup post), put them beside a copy of the end-of-retail files in a folder \
                     of their own, and choose that folder here.",
    },
];

/// The known world a list id names.
#[must_use]
pub fn known(list_id: &str) -> Option<&'static KnownWorld> {
    let id = list_id.trim();
    KNOWN_WORLDS
        .iter()
        .find(|k| k.list_id.eq_ignore_ascii_case(id))
}

/// Fill in what the table knows about `w`, if it knows `w`. The table speaks for the world: its
/// era and systems are the world's, not a choice the player is asked to make.
pub fn apply(w: &mut World) {
    let Some(k) = w.list_id.as_deref().and_then(known) else {
        return;
    };
    w.emulator = k.emulator;
    w.era = Some(k.era.name().to_owned());
    w.era_source = Some(Told::World);
    w.era_features = Some(EraFeatureOverrides::all_of(k.features).to_string());
    w.features_source = Some(Told::World);
    w.world_base = Some(k.world_base.to_owned());
    w.logon_version = Some(k.logon_version.to_owned());
    w.world_profile = Some(k.profile.to_owned());
    w.dats.expected = Some(k.expected);
    w.dats.custom = Some(CustomDats {
        url: k.files_url.to_owned(),
        sha256: None,
        size: None,
        iterations: k.expected,
        license_note: Some(k.files_note.to_owned()),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_world_has_a_profile_the_client_compiles_and_a_logon_it_can_send() {
        for k in KNOWN_WORLDS {
            assert!(
                dereth_rules::world::profile(k.profile).is_some(),
                "{}: {}",
                k.name,
                k.profile
            );
            assert!(!k.logon_version.is_empty() && k.logon_version.is_ascii());
            assert!(matches!(k.world_base, "modern" | "classic"), "{}", k.name);
            assert!(
                !k.expected.is_end_of_retail(),
                "{} has its own files",
                k.name
            );
            assert!(k.files_note.contains(k.name), "{}", k.name);
            assert_eq!(known(&k.list_id.to_ascii_uppercase()), Some(k), "any case");
        }
        let ids: std::collections::HashSet<_> = KNOWN_WORLDS
            .iter()
            .map(|k| k.list_id.to_ascii_lowercase())
            .collect();
        assert_eq!(ids.len(), KNOWN_WORLDS.len(), "one entry per world");
    }

    #[test]
    fn the_table_holds_the_four_worlds_with_their_servers_logon_files_and_rules() {
        let row = |name: &str| *KNOWN_WORLDS.iter().find(|k| k.name == name).unwrap();
        let d = row("Dekarutide");
        assert_eq!(
            (d.logon_version, d.profile, d.emulator),
            ("c118", "classicace-customdm", Emulator::ClassicAce)
        );
        assert_eq!(d.expected.label(), "20044/20011/20011/497");
        assert_eq!(
            row("Unfamiliar Shores").list_id,
            "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1"
        );
        assert_eq!(row("Unfamiliar Shores").expected, d.expected);
        for name in ["Seedsow", "Snowreap"] {
            let g = row(name);
            assert_eq!(
                (g.logon_version, g.profile, g.emulator),
                ("1802", "classicdereth", Emulator::Gdle)
            );
            assert!(
                g.logon_confirmed,
                "the server answered a 1802 logon with an account refusal, not a version one"
            );
            assert_eq!(g.expected.label(), "2072/4/-/-");
        }
        for k in KNOWN_WORLDS {
            assert_eq!((k.era, k.world_base), (EraId::Infiltration, "modern"));
        }
        assert!(d.features.trinkets && d.features.cloaks && !d.features.ratings);
    }

    #[test]
    fn a_known_world_is_filled_in_and_any_other_is_left_as_its_row_says() {
        let mut w = World::new("dekarutide", "Dekarutide");
        w.list_id = Some("4DB379B0-587A-4775-9B7F-879E0F15BA4B".into());
        w.emulator = Emulator::Ace;
        apply(&mut w);
        assert_eq!(w.emulator, Emulator::ClassicAce);
        assert_eq!(w.era.as_deref(), Some("infiltration"));
        assert_eq!(w.era_source, Some(Told::World));
        assert_eq!(w.logon_version.as_deref(), Some("c118"));
        assert_eq!(w.world_profile.as_deref(), Some("classicace-customdm"));
        assert_eq!(w.world_base.as_deref(), Some("modern"));
        assert_eq!(w.dats.custom.as_ref().unwrap().iterations, CUSTOMDM_FILES);
        assert!(w.needs_private_dats());
        let (o, unknown) = EraFeatureOverrides::parse(w.era_features.as_deref().unwrap()).unwrap();
        assert!(unknown.is_empty());
        assert_eq!(o.apply(EraId::Eor.features()), CUSTOMDM_FEATURES);

        let mut other = World::new("coldeve", "Coldeve");
        other.list_id = Some("9d17ce44-7db5-40c1-b7bb-a01c9de21d00".into());
        let before = other.clone();
        apply(&mut other);
        assert_eq!(other, before);
    }
}
