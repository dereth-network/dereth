//! Installations: a client on disk, and which build it is.
//!
//! Identification is two steps. The quick one hashes the executable alone (about 5 MB, a fraction
//! of a second) and looks it up in the table of builds the launcher knows. The slow one, on request,
//! checks every file in the folder against that build's manifest. Manifests are lists of hashes and
//! nothing else; the launcher carries no client files and never fetches any.
//!
//! An executable the table does not know is still an installation. Its version resource says what it
//! claims to be, and the interface says "not a build we know; it may still work".

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::datset::{scan_dir, Iterations};
use crate::world::END_OF_RETAIL_NET_VERSION;

/// Which family of client a folder holds. Decides the command-line form and how dats are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientKind {
    /// The rebuild. Takes its dats from any folder, runs any number of copies.
    Dereth,
    /// A retail `acclient.exe`.
    Retail,
}

/// How sure the launcher is about a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifiedBy {
    /// The executable's hash is in the known-build table.
    ExeSha256,
    /// And every file in the folder matched that build's manifest.
    ManifestFull,
    /// Unknown executable; what it says about itself is all there is.
    VersionResourceOnly,
}

/// A build the launcher recognises by its executable's hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownBuild {
    pub exe_sha256: &'static str,
    pub client_id: &'static str,
    pub version: &'static str,
    pub build_date: &'static str,
    /// The logon version string this build sends, where that is known. The version resource is not
    /// evidence of it: older builds carry a different number there, and whether their logon header
    /// agrees is an open question, so they are left unknown rather than guessed.
    pub net_version: Option<&'static str>,
    /// Post-link patches found in this exact file.
    pub modifications: &'static [&'static str],
    pub multi_instance: bool,
    /// The build's full-folder manifest, `sha256sum` format, dats and user-state files left out.
    pub manifest: Option<&'static str>,
}

/// Every build the launcher can name.
pub const KNOWN_BUILDS: &[KnownBuild] = &[
    KnownBuild {
        exe_sha256: "f512d6c494015d2afbff43d0cb98aa6b9d8c9d27e5deab73178eba35d88e6f6e",
        client_id: "acclient-6096",
        version: "00.00.11.6096",
        build_date: "2015-06-12",
        net_version: Some(END_OF_RETAIL_NET_VERSION),
        // The copy that circulates in the community, not Turbine's own: its checksum does not match.
        modifications: &[
            "single-instance check removed",
            "window resize limits raised",
        ],
        multi_instance: true,
        manifest: Some(include_str!("../manifests/acclient-6096.sha256")),
    },
    KnownBuild {
        exe_sha256: "3a437977074a0bed94c8cd03fd838492af62ef88bc28684f61e281556b1e634a",
        client_id: "acclient-6067",
        version: "00.00.11.6067",
        build_date: "2015-05-14",
        net_version: None,
        modifications: &[],
        multi_instance: false,
        manifest: None,
    },
    KnownBuild {
        exe_sha256: "006ffeadc5d679c871497112a5bd1f87714d0e273e2166bae5052dde369297b1",
        client_id: "acclient-4186",
        version: "00.00.11.4186",
        build_date: "2013-09-06",
        net_version: Some(END_OF_RETAIL_NET_VERSION),
        modifications: &[],
        multi_instance: false,
        manifest: Some(include_str!("../manifests/acclient-4186.sha256")),
    },
    KnownBuild {
        exe_sha256: "b66a6dc9f7b7844912917219b1d53894aeb3e78b75bc42c9f2fa905f630542db",
        client_id: "acclient-4079",
        version: "00.00.11.4079",
        build_date: "2012-01-12",
        net_version: None,
        modifications: &[],
        multi_instance: false,
        manifest: Some(include_str!("../manifests/acclient-4079.sha256")),
    },
    KnownBuild {
        exe_sha256: "c8264c45de6ff6c08225fbb770e3789929c92d99ed31f7bad59a93564d374166",
        client_id: "acclient-3986",
        version: "00.00.11.3986",
        build_date: "2010-06-09",
        net_version: None,
        modifications: &[],
        multi_instance: false,
        manifest: None,
    },
];

pub fn known_build(exe_sha256: &str) -> Option<&'static KnownBuild> {
    KNOWN_BUILDS
        .iter()
        .find(|b| b.exe_sha256.eq_ignore_ascii_case(exe_sha256))
}

pub fn known_build_by_id(client_id: &str) -> Option<&'static KnownBuild> {
    KNOWN_BUILDS.iter().find(|b| b.client_id == client_id)
}

/// The outcome of checking a folder against a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ManifestResult {
    pub total: usize,
    pub matched: usize,
    pub differing: Vec<String>,
    pub missing: Vec<String>,
}

impl ManifestResult {
    pub fn all_match(&self) -> bool {
        self.matched == self.total
    }

    /// "217 of 217 files match".
    pub fn summary(&self) -> String {
        format!("{} of {} files match", self.matched, self.total)
    }
}

/// An installation the launcher knows about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installation {
    pub id: String,
    pub path: PathBuf,
    /// The executable's file name within `path`.
    pub exe: String,
    pub kind: ClientKind,
    /// `acclient-6096`, `dereth-0.4.0`, or `unknown`.
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_version: Option<String>,
    pub identified_by: IdentifiedBy,
    #[serde(default)]
    pub modifications: Vec<String>,
    pub multi_instance: bool,
    /// What the dats in the client's own folder say, if it has any. A retail client plays with
    /// these.
    #[serde(default, skip_serializing_if = "Iterations::is_empty")]
    pub own_dats: Iterations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_result: Option<ManifestResult>,
}

impl Installation {
    pub fn exe_path(&self) -> PathBuf {
        self.path.join(&self.exe)
    }

    pub fn known(&self) -> Option<&'static KnownBuild> {
        known_build_by_id(&self.client_id)
    }

    /// "acclient 00.00.11.6096 (12 June 2015)", or the best that can be said.
    pub fn display_name(&self) -> String {
        let family = match self.kind {
            ClientKind::Dereth => "Dereth client",
            ClientKind::Retail => "acclient",
        };
        match (&self.version, &self.build_date) {
            (Some(v), Some(d)) => format!("{family} {v} ({})", long_date(d)),
            (Some(v), None) => format!("{family} {v}"),
            _ if self.kind == ClientKind::Dereth => family.to_owned(),
            _ => format!("{family} (unknown build)"),
        }
    }
}

/// `2015-06-12` as `12 June 2015`. Anything else is returned as it came.
pub fn long_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let parts: Vec<&str> = iso.split('-').collect();
    if let [y, m, d] = parts[..] {
        if let (Ok(m), Ok(d)) = (m.parse::<usize>(), d.parse::<u32>()) {
            if (1..=12).contains(&m) {
                return format!("{d} {} {y}", MONTHS[m - 1]);
            }
        }
    }
    iso.to_owned()
}

/// Hash a file, streaming.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug)]
pub enum IdentifyError {
    /// The folder has no `acclient.exe`, or the path is not a file.
    NoClient,
    Io(std::io::Error),
}

impl core::fmt::Display for IdentifyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IdentifyError::NoClient => write!(f, "no acclient.exe in this folder"),
            IdentifyError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for IdentifyError {}

/// The retail client's file name.
pub const RETAIL_EXE: &str = "acclient.exe";

/// Identify the retail client in `folder`, which must hold `acclient.exe`. The quick step only;
/// [`verify`] is the slow one.
pub fn identify_retail(
    folder: &Path,
    id: impl Into<String>,
) -> Result<Installation, IdentifyError> {
    if !folder.join(RETAIL_EXE).is_file() {
        return Err(IdentifyError::NoClient);
    }
    identify_exe(folder, RETAIL_EXE, ClientKind::Retail, id.into())
}

/// Identify the Dereth client at `exe`: the one that ships beside the launcher, or a build.
pub fn identify_dereth(exe: &Path) -> Result<Installation, IdentifyError> {
    let (Some(folder), Some(name)) = (exe.parent(), exe.file_name().and_then(|n| n.to_str()))
    else {
        return Err(IdentifyError::NoClient);
    };
    if !exe.is_file() {
        return Err(IdentifyError::NoClient);
    }
    identify_exe(folder, name, ClientKind::Dereth, "dereth".into())
}

fn identify_exe(
    folder: &Path,
    exe: &str,
    kind: ClientKind,
    id: String,
) -> Result<Installation, IdentifyError> {
    let path = folder.join(exe);
    let bytes = std::fs::read(&path).map_err(IdentifyError::Io)?;
    let sha = hex(&Sha256::digest(&bytes));
    let resource_version = pe::file_version(&bytes);
    let link_date = pe::link_date(&bytes);

    let mut inst = Installation {
        id,
        path: folder.to_path_buf(),
        exe: exe.to_owned(),
        kind,
        client_id: "unknown".into(),
        version: resource_version,
        build_date: link_date,
        net_version: None,
        identified_by: IdentifiedBy::VersionResourceOnly,
        modifications: Vec::new(),
        multi_instance: false,
        own_dats: Iterations::default(),
        verified_at: None,
        manifest_result: None,
    };
    let mut dats = Iterations::default();
    for f in scan_dir(folder) {
        dats.set(f.role, f.iterations);
    }
    inst.own_dats = dats;

    match kind {
        ClientKind::Dereth => {
            // The rebuild speaks the end-of-retail protocol and runs any number of copies. Its
            // version, when the executable carries one, names it.
            inst.client_id = inst
                .version
                .as_deref()
                .map_or("dereth".into(), |v| format!("dereth-{v}"));
            inst.net_version = Some(END_OF_RETAIL_NET_VERSION.into());
            inst.multi_instance = true;
            inst.identified_by = IdentifiedBy::ExeSha256;
        }
        ClientKind::Retail => {
            if let Some(b) = known_build(&sha) {
                inst.client_id = b.client_id.into();
                inst.version = Some(b.version.into());
                inst.build_date = Some(b.build_date.into());
                inst.net_version = b.net_version.map(Into::into);
                inst.modifications = b.modifications.iter().map(|m| (*m).to_owned()).collect();
                inst.multi_instance = b.multi_instance;
                inst.identified_by = IdentifiedBy::ExeSha256;
            }
        }
    }
    Ok(inst)
}

/// Check every file of a folder against a manifest in `sha256sum` format (`<hex> *<path>` or
/// `<hex>  <path>`, `#` comments allowed). Paths are relative to `folder`.
pub fn verify_manifest(folder: &Path, manifest: &str) -> ManifestResult {
    let mut r = ManifestResult::default();
    for line in manifest.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((hash, rest)) = line.split_once(' ') else {
            continue;
        };
        let rel = rest.trim_start_matches(' ').trim_start_matches('*');
        if rel.is_empty() {
            continue;
        }
        r.total += 1;
        match sha256_file(&folder.join(rel)) {
            Ok(h) if h.eq_ignore_ascii_case(hash) => r.matched += 1,
            Ok(_) => r.differing.push(rel.to_owned()),
            Err(_) => r.missing.push(rel.to_owned()),
        }
    }
    r
}

/// The slow step: check the whole folder against the identified build's manifest, and record it.
/// Returns `None` when there is no manifest to check against.
pub fn verify(inst: &mut Installation, now: u64) -> Option<&ManifestResult> {
    let manifest = inst.known()?.manifest?;
    let result = verify_manifest(&inst.path, manifest);
    if result.all_match() {
        inst.identified_by = IdentifiedBy::ManifestFull;
    }
    inst.verified_at = Some(now);
    inst.manifest_result = Some(result);
    inst.manifest_result.as_ref()
}

/// Reading what a Windows executable says about itself, from its bytes, on any platform.
pub mod pe {
    fn u16_at(b: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
    }

    fn u32_at(b: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes([
            *b.get(at)?,
            *b.get(at + 1)?,
            *b.get(at + 2)?,
            *b.get(at + 3)?,
        ]))
    }

    /// The file version from the fixed version block, as `00.00.11.6096`.
    ///
    /// Found by its signature rather than by walking the resource tree: the fixed block is the one
    /// structure in a version resource with a magic number, and a signature search cannot be sent
    /// astray by a malformed directory.
    pub fn file_version(b: &[u8]) -> Option<String> {
        const SIGNATURE: [u8; 4] = 0xFEEF_04BDu32.to_le_bytes();
        let at = b.windows(4).position(|w| w == SIGNATURE)?;
        let ms = u32_at(b, at + 8)?;
        let ls = u32_at(b, at + 12)?;
        Some(format!(
            "{:02}.{:02}.{:02}.{:04}",
            ms >> 16,
            ms & 0xFFFF,
            ls >> 16,
            ls & 0xFFFF
        ))
    }

    /// The link date from the file header, as `YYYY-MM-DD` (UTC).
    pub fn link_date(b: &[u8]) -> Option<String> {
        if b.get(..2)? != b"MZ" {
            return None;
        }
        let pe = u32_at(b, 0x3C)? as usize;
        if b.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let stamp = u32_at(b, pe + 8)?;
        // A zero or a reproducible-build hash is not a date.
        let _machine = u16_at(b, pe + 4)?;
        if stamp == 0 || stamp == u32::MAX {
            return None;
        }
        Some(civil_date(i64::from(stamp) / 86_400))
    }

    /// Days since 1970-01-01 to a calendar date.
    fn civil_date(days: i64) -> String {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// The smallest thing with a header and a version block.
    #[cfg(test)]
    pub fn fake_exe(stamp: u32, ms: u32, ls: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&0x14Cu16.to_le_bytes());
        b[0x88..0x8C].copy_from_slice(&stamp.to_le_bytes());
        b[0x100..0x104].copy_from_slice(&0xFEEF_04BDu32.to_le_bytes());
        b[0x108..0x10C].copy_from_slice(&ms.to_le_bytes());
        b[0x10C..0x110].copy_from_slice(&ls.to_le_bytes());
        b
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_6096_build_reads_as_its_version_and_date() {
            // 2015-06-12 08:16:44 UTC.
            let exe = fake_exe(1_434_097_004, 0x0000_0000, 0x000B_17D0);
            assert_eq!(file_version(&exe).as_deref(), Some("00.00.11.6096"));
            assert_eq!(link_date(&exe).as_deref(), Some("2015-06-12"));
        }

        #[test]
        fn calendar_edges() {
            assert_eq!(civil_date(0), "1970-01-01");
            assert_eq!(civil_date(11_016), "2000-02-29");
            assert_eq!(civil_date(20_723), "2026-09-27");
        }

        #[test]
        fn not_an_executable_says_nothing() {
            assert_eq!(link_date(b"hello"), None);
            assert_eq!(file_version(b"hello"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::tests::{fake_set, tmp};

    #[test]
    fn a_known_hash_names_its_build() {
        let b = known_build("F512D6C494015D2AFBFF43D0CB98AA6B9D8C9D27E5DEAB73178EBA35D88E6F6E")
            .unwrap();
        assert_eq!(b.client_id, "acclient-6096");
        assert!(
            b.multi_instance,
            "the circulating 6096 has the single-instance check removed"
        );
        assert!(!known_build("00").is_some());
    }

    #[test]
    fn every_build_id_is_unique_and_every_manifest_parses() {
        for b in KNOWN_BUILDS {
            assert_eq!(
                KNOWN_BUILDS
                    .iter()
                    .filter(|o| o.client_id == b.client_id)
                    .count(),
                1
            );
            assert_eq!(b.exe_sha256.len(), 64);
            if let Some(m) = b.manifest {
                let lines = m.lines().filter(|l| !l.trim().is_empty()).count();
                assert!(lines > 100, "{} manifest has {lines} lines", b.client_id);
                assert!(
                    !m.contains(".dat\n"),
                    "dats are identified by iteration, not hash"
                );
                assert!(m.contains("acclient.exe"));
            }
        }
    }

    #[test]
    fn an_unknown_retail_exe_is_an_installation_with_what_it_says_about_itself() {
        let d = tmp("unknown-exe");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("acclient.exe"),
            pe::fake_exe(1_434_097_004, 0, 0x000B_17D0),
        )
        .unwrap();
        fake_set(&d, Iterations::END_OF_RETAIL);
        let inst = identify_retail(&d, "i1").unwrap();
        assert_eq!(inst.kind, ClientKind::Retail);
        assert_eq!(inst.client_id, "unknown");
        assert_eq!(inst.identified_by, IdentifiedBy::VersionResourceOnly);
        assert_eq!(inst.version.as_deref(), Some("00.00.11.6096"));
        assert_eq!(
            inst.net_version, None,
            "an unknown build's logon version is not assumed"
        );
        assert_eq!(inst.own_dats, Iterations::END_OF_RETAIL);
        assert_eq!(inst.display_name(), "acclient 00.00.11.6096 (12 June 2015)");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_dereth_client_is_identified_by_its_path_and_speaks_the_final_protocol() {
        let d = tmp("dereth");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("dereth-client.exe"), b"MZ").unwrap();
        let inst = identify_dereth(&d.join("dereth-client.exe")).unwrap();
        assert_eq!(inst.kind, ClientKind::Dereth);
        assert_eq!(inst.client_id, "dereth");
        assert_eq!(inst.exe, "dereth-client.exe");
        assert!(inst.multi_instance);
        assert_eq!(inst.net_version.as_deref(), Some("1802"));
        assert!(matches!(
            identify_dereth(&d.join("absent.exe")),
            Err(IdentifyError::NoClient)
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_without_acclient_has_no_retail_client() {
        let d = tmp("empty");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("dereth-client.exe"), b"MZ").unwrap();
        assert!(matches!(
            identify_retail(&d, "i"),
            Err(IdentifyError::NoClient)
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_manifest_check_names_what_differs_and_what_is_missing() {
        let d = tmp("manifest");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        std::fs::write(d.join("sub/b.txt"), b"not b").unwrap();
        let a = hex(&Sha256::digest(b"a"));
        let b = hex(&Sha256::digest(b"b"));
        let manifest = format!("# comment\n{a} *a.txt\n{b}  sub/b.txt\n{a} *gone.txt\n");
        let r = verify_manifest(&d, &manifest);
        assert_eq!((r.total, r.matched), (3, 1));
        assert_eq!(r.differing, ["sub/b.txt"]);
        assert_eq!(r.missing, ["gone.txt"]);
        assert_eq!(r.summary(), "1 of 3 files match");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dates_read_the_way_people_say_them() {
        assert_eq!(long_date("2015-06-12"), "12 June 2015");
        assert_eq!(long_date("whenever"), "whenever");
    }
}
