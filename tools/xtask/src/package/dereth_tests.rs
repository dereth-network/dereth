//! Behaviour: none (tooling: Dereth's release files, their contents and deny scan, the header
//! checks of its programs, the launcher's update signatures and latest.json, version and tag).

use base64::Engine as _;

use super::archive::{self, Member};
use super::dereth::{self, Asset};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::sign::{self, Signer};
use super::targets::{self, Os, Target};
use super::tests::{elf, pe};
use super::version::{self, Product};
use crate::util::workspace_root;

fn target(triple: &str) -> Target {
    targets::find_dereth(triple).expect("a Dereth release target")
}

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";
const MAC_X64: &str = "x86_64-apple-darwin";
const V: &str = "0.2.0";

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

/// A fresh unencrypted signing key: its text as Tauri's signer takes it (base64 of the key file)
/// and its public half as the launcher's configuration carries it (base64 of the key file).
fn key_pair() -> (String, String) {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("a key pair");
    let secret = kp.sk.to_box(None).expect("secret box").to_string();
    let public = kp.pk.to_box().expect("public box").to_string();
    (b64(&secret), b64(&public))
}

fn signer(secret_b64: &str) -> Signer {
    Signer::from_key_text(&sign::key_text(secret_b64).expect("key text"), "").expect("signer")
}

// ---------------------------------------------------------------- signatures

/// A signature made here verifies the way the launcher's updater verifies one, and names the
/// file and the version in its trusted comment.
#[test]
fn a_signature_verifies_as_the_updater_verifies_it_and_names_file_and_version() {
    let (secret, public) = key_pair();
    let data = b"the release zip";
    let sig = signer(&secret)
        .sign(data, "dereth-0.2.0-windows-x86_64.zip", V, 1_700_000_000)
        .expect("signs");
    sign::verify(data, &sig, &public, V).expect("verifies");
    let decoded = String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(&sig)
            .expect("base64"),
    )
    .expect("text");
    let parsed = minisign_verify::Signature::decode(&decoded).expect("a minisign signature");
    assert_eq!(
        parsed.trusted_comment(),
        "timestamp:1700000000\tfile:dereth-0.2.0-windows-x86_64.zip\tversion:0.2.0"
    );
    assert_eq!(sign::signed_version(parsed.trusted_comment()), Some(V));
}

/// Changed bytes, another key, or a version other than the one signed are each refused.
#[test]
fn a_changed_file_another_key_or_another_version_is_refused() {
    let (secret, public) = key_pair();
    let (_, other_public) = key_pair();
    let data = b"the release zip";
    let sig = signer(&secret).sign(data, "f.zip", V, 0).expect("signs");
    assert!(sign::verify(b"the release zap", &sig, &public, V).is_err());
    assert!(sign::verify(data, &sig, &other_public, V).is_err());
    let e = sign::verify(data, &sig, &public, "0.3.0").expect_err("another version");
    assert!(e.contains("0.2.0"), "{e}");
    assert!(sign::verify(data, &sig, &public, "v0.2.0").is_ok());
}

/// The key is taken as Tauri takes it: the key file's base64, the key file's text, or a path to
/// either.
#[test]
fn the_signing_key_is_read_as_base64_as_text_or_from_a_file() {
    let (secret, _) = key_pair();
    let text = sign::key_text(&secret).expect("from base64");
    assert!(text.starts_with("untrusted comment:"), "{text}");
    assert_eq!(sign::key_text(&text).expect("from text"), text);
    let path = std::env::temp_dir().join(format!("dereth-key-{}.key", std::process::id()));
    std::fs::write(&path, &secret).expect("write");
    assert_eq!(
        sign::key_text(&path.display().to_string()).expect("from a file"),
        text
    );
    let _ = std::fs::remove_file(&path);
    assert!(sign::key_text("not a key").is_err());
}

/// A throwaway key pair from `cargo tauri signer generate -p throwaway` (encrypted, as the Tauri
/// CLI always makes them), and the CLI's own signature (`cargo tauri signer sign`) of
/// [`TAURI_SIGNED`]. Nothing is signed with it but these tests.
const TAURI_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5UzR2enRERnh4MWpSMk9KVVYvRDhvSFEzSXRDUUVoZTc0cVgyRytoRG9FSUFBQkFBQUFBQUFBQUFBQUlBQUFBQURQS2xacTZKaEY5MlVHbStza2NPMUpUeWpjck9DMGhTQS81TWZia0tzeG9mc1JWSEtYbmdhZHVWZHFOSzkwRlAzYXRrYmVaNnU0M0VnQVVaL25PemJ6YzNYN3ZqQzgzZjRpaXRWeU5BNklxQy9zZEZiVzVvd0JVa2lEZmh0WXN3ZVo1TTcvK1J5Z0k9Cg==";
const TAURI_PUB: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDMwQ0I0MDMzRTc1QjMxMzcKUldRM01Wdm5NMERMTUFwUmp2RmIwUU5sNGt3cVM0OUM4WGtLd2ZGSUhqUks2R1VoNVZmY3dBeUsK";
const TAURI_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRM01Wdm5NMERMTUJrekw5Tk1rMVR6ZUt0NytaT2tkdzJKdVJWc0VTRkpWUUs5dnU2M3RNYVZ6NTAvZUN6Zk9Pd3dPbDFQajJkV2IrSGxteVlpM1lWcWVpUm5xREFJYVEwPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwNjQ5NjkyCWZpbGU6c2FtcGxlLmJpbgp2ZXlxeTNQcjRpczRsbDkyTVJ2V3AwKy85UDBBSEIvN3c5TEFrbjU1WndNSU85R01MUVkvc3JaNGs0RXBONWQxekRvN2Q0R29PUVg4WDh3V2pqbElCUT09Cg==";
const TAURI_SIGNED: &[u8] = b"dereth release bytes";

/// A key the Tauri CLI made opens with its password and signs what the updater accepts, and the
/// CLI's own signature verifies here: the two signers are interchangeable.
#[test]
fn a_tauri_cli_key_signs_here_and_a_tauri_cli_signature_verifies_here() {
    sign::verify(TAURI_SIGNED, TAURI_SIG, TAURI_PUB, V).expect("the CLI's signature verifies");
    let text = sign::key_text(TAURI_KEY).expect("key text");
    let s = Signer::from_key_text(&text, "throwaway").expect("opens with its password");
    let sig = s.sign(b"a release", "a.zip", V, 0).expect("signs");
    sign::verify(b"a release", &sig, TAURI_PUB, V).expect("verifies");
    let e = Signer::from_key_text(&text, "wrong")
        .err()
        .expect("a wrong password");
    assert!(e.contains(sign::PASSWORD_VAR), "{e}");
}

/// The launcher's configured public key is a minisign public key, so a release signed with its
/// secret half is one every launcher accepts.
#[test]
fn the_launchers_configured_public_key_is_a_minisign_public_key() {
    let conf = std::fs::read_to_string(workspace_root().join("dereth/launcher/tauri.conf.json"))
        .expect("tauri.conf.json");
    let key = sign::launcher_public_key(&conf).expect("a pubkey");
    let text = String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(&key)
            .expect("base64"),
    )
    .expect("text");
    minisign_verify::PublicKey::decode(&text).expect("a minisign public key");
    assert!(sign::launcher_public_key("{}").is_err());
}

// ---------------------------------------------------------------- latest.json

/// The pieces merge into the updater's document: the version, the notes, the date, and one
/// entry per platform with its address and signature.
#[test]
fn latest_json_merges_one_piece_per_platform() {
    let pieces = [
        sign::platform_piece("windows-x86_64", "https://x/w.zip", "c2ln"),
        sign::platform_piece("linux-x86_64", "https://x/l.AppImage", "c2lnMg=="),
    ];
    let latest =
        sign::merge_latest(V, "Dereth 0.2.0", "2026-10-01T00:00:00Z", &pieces).expect("merges");
    assert_eq!(latest["version"], V);
    assert_eq!(latest["pub_date"], "2026-10-01T00:00:00Z");
    assert_eq!(
        latest["platforms"]["windows-x86_64"]["url"],
        "https://x/w.zip"
    );
    assert_eq!(latest["platforms"]["linux-x86_64"]["signature"], "c2lnMg==");
    let twice = [pieces[0].clone(), pieces[0].clone()];
    assert!(sign::merge_latest(V, "", "", &twice).is_err());
    let unsigned = [serde_json::json!({ "darwin-aarch64": { "url": "https://x/m" } })];
    assert!(sign::merge_latest(V, "", "", &unsigned).is_err());
    assert!(sign::merge_latest(V, "", "", &[]).is_err());
}

/// A platform's entry is keyed as the updater looks it up, and its file is downloaded from the
/// release's tag.
#[test]
fn platforms_are_keyed_as_the_updater_names_them_and_files_come_from_the_tag() {
    assert_eq!(target(WINDOWS).updater_platform(), "windows-x86_64");
    assert_eq!(target(LINUX).updater_platform(), "linux-x86_64");
    assert_eq!(target(MAC_ARM).updater_platform(), "darwin-aarch64");
    assert_eq!(target(MAC_X64).updater_platform(), "darwin-x86_64");
    assert_eq!(
        dereth::download_url("https://github.com/dereth-network/dereth/", V, "a.zip"),
        "https://github.com/dereth-network/dereth/releases/download/dereth-v0.2.0/a.zip"
    );
}

// ---------------------------------------------------------------- names and contents

/// Each release file is named for the version and the platform, and the folder's classification
/// reads every name back; anything else is refused, among it a client without the launcher and a
/// loose signature file (the signature is in `latest.json`).
#[test]
fn release_files_are_named_for_version_and_platform_and_read_back() {
    assert_eq!(
        dereth::release_file_name(target(WINDOWS), V),
        "dereth-0.2.0-windows-x86_64.zip"
    );
    assert_eq!(
        dereth::release_file_name(target(MAC_ARM), V),
        "Dereth-0.2.0-macos-aarch64.app.tar.gz"
    );
    assert_eq!(
        dereth::release_file_name(target(LINUX), V),
        "Dereth-0.2.0-linux-x86_64.AppImage"
    );
    for triple in targets::DERETH_TRIPLES {
        let t = target(triple);
        let name = dereth::release_file_name(t, V);
        assert_eq!(dereth::classify(&name, V), Ok(Asset::Launcher(t)));
        assert_eq!(
            dereth::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        );
        assert_eq!(guard::name_finding(&name), None, "{name}");
        assert!(dereth::classify(&format!("{name}.sig"), V).is_err());
        assert_eq!(
            dereth::classify(&dereth::piece_name(t), V),
            Ok(Asset::Piece(t))
        );
    }
    for index in ["SHA256SUMS", "MANIFEST.txt", "release.json", "latest.json"] {
        assert_eq!(dereth::classify(index, V), Ok(Asset::Index));
    }
    for stray in [
        "dereth-0.1.0-windows-x86_64.zip",
        "client_portal.dat",
        "dereth-client-0.2.0-windows-x86_64.zip",
        "dereth-client-0.2.0-linux-x86_64.tar.gz",
        "Dereth-Client-0.2.0-macos-aarch64.app.tar.gz",
        "notes.txt",
    ] {
        assert!(dereth::classify(stray, V).is_err(), "{stray}");
    }
}

/// The Windows zip is what the launcher's own update installs: one top-level folder holding
/// `dereth.exe` and the client beside it.
#[test]
fn the_windows_launcher_zip_holds_the_launcher_and_the_client_in_one_folder() {
    let w = target(WINDOWS);
    assert_eq!(dereth::archive_root(w, V).as_deref(), Some("dereth-0.2.0"));
    let (required, optional) = dereth::contents(w);
    assert_eq!(
        required,
        [
            "LICENSE",
            "NOTICE.txt",
            "THIRD-PARTY-LICENSES.html",
            "dereth-client.exe",
            "dereth.exe"
        ]
    );
    assert!(optional.is_empty());
}

/// The launcher's release configuration, which the launcher carries compiled in, names the
/// client and the notices relative to the app's folder on the bundled platforms, and names no
/// file at all on Windows, where nothing is bundled.
#[test]
fn the_launchers_release_configuration_names_no_machine_folder() {
    let ws = workspace_root();
    let app = ws.join("dereth").join("launcher");
    let stage = ws.join("target").join("package").join("stage").join("x");
    let client = stage.join("bin").join("dereth-client");
    let notices = stage.join("notices");
    let win = dereth::bundle_config(target(WINDOWS), &app, &client, &notices).unwrap();
    assert_eq!(win, serde_json::json!({ "createUpdaterArtifacts": false }));
    for t in [target(MAC_ARM), target(LINUX)] {
        let c = dereth::bundle_config(t, &app, &client, &notices).unwrap();
        assert_eq!(
            c["externalBin"],
            serde_json::json!(["../../target/package/stage/x/bin/dereth-client"])
        );
        assert_eq!(
            c["resources"]["../../target/package/stage/x/notices/NOTICE.txt"],
            "NOTICE.txt"
        );
        let text = c.to_string();
        assert!(
            guard::local_paths_in(text.as_bytes(), &[ws.display().to_string()]).is_empty(),
            "{text}"
        );
    }
}

/// A path is named relative to a folder by climbing to what they share; a folder on another
/// root has no such name.
#[test]
fn a_path_is_named_relative_to_a_folder_through_what_they_share() {
    let root = workspace_root();
    let a = root.join("x").join("y");
    assert_eq!(
        dereth::relative_to(&a, &root.join("z").join("f")).unwrap(),
        "../../z/f"
    );
    assert_eq!(dereth::relative_to(&root, &a).unwrap(), "x/y");
    assert!(dereth::relative_to(&a, std::path::Path::new("f")).is_err());
}

/// The macOS bundle carries the launcher and the client in `Contents/MacOS` and MoltenVK in
/// `Contents/Frameworks`, the places the client and the launcher look; the AppImage is no archive.
#[test]
fn the_macos_bundle_carries_the_client_and_moltenvk_where_they_are_looked_for() {
    let (required, optional) = dereth::contents(target(MAC_ARM));
    for need in [
        "Contents/MacOS/dereth",
        "Contents/MacOS/dereth-client",
        "Contents/Frameworks/libMoltenVK.dylib",
        "Contents/Info.plist",
        "Contents/Resources/NOTICE.txt",
    ] {
        assert!(required.contains(&need.to_owned()), "{need}");
    }
    for name in required.iter().chain(&optional) {
        assert_eq!(guard::name_finding(name), None, "{name}");
    }
    assert_eq!(
        dereth::archive_root(target(MAC_ARM), V).as_deref(),
        Some("Dereth.app")
    );
    assert_eq!(dereth::archive_root(target(LINUX), V), None);
    assert_eq!(
        dereth::programs(target(MAC_ARM)),
        ["Contents/MacOS/dereth", "Contents/MacOS/dereth-client"]
    );
}

fn file(path: &str, bytes: &[u8]) -> Entry {
    Entry::file(path, bytes)
}

fn dir(path: &str) -> Entry {
    Entry {
        path: path.to_owned(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }
}

fn mac_bundle_entries() -> Vec<Entry> {
    let (required, _) = dereth::contents(target(MAC_ARM));
    let mut entries = vec![
        dir(""),
        dir("Contents"),
        dir("Contents/MacOS"),
        dir("Contents/Frameworks"),
        dir("Contents/Resources"),
    ];
    entries.extend(required.iter().map(|p| file(p, b"x")));
    entries
}

/// A bundle passes with exactly its files; a stray file, a folder no file is in, a link, the data
/// header, an oversized file or a missing file is each refused.
#[test]
fn a_bundle_scan_passes_its_own_files_and_refuses_everything_else() {
    let (required, optional) = dereth::contents(target(MAC_ARM));
    let scan = |entries: &[Entry]| {
        guard::scan_tree(
            entries,
            &required,
            &optional,
            dereth::FILE_CAP,
            dereth::TOTAL_CAP,
        )
    };
    assert_eq!(scan(&mac_bundle_entries()), Vec::<String>::new());

    let mut stray = mac_bundle_entries();
    stray.push(file("Contents/Resources/extra.txt", b"x"));
    assert_eq!(scan(&stray).len(), 1, "{:?}", scan(&stray));

    let mut empty_dir = mac_bundle_entries();
    empty_dir.push(dir("Contents/Plugins"));
    assert!(scan(&empty_dir)[0].contains("folder"));

    let mut dats = mac_bundle_entries();
    let mut head = vec![0u8; 0x200];
    head[0x140..0x144].copy_from_slice(&0x5442u32.to_le_bytes());
    dats.push(file("Contents/MacOS/client_portal.dat", &head));
    let findings = scan(&dats);
    assert!(findings.iter().any(|f| f.contains(".dat")), "{findings:?}");
    assert!(
        findings.iter().any(|f| f.contains("header")),
        "{findings:?}"
    );

    let mut linked = mac_bundle_entries();
    linked.push(Entry {
        path: "Contents/MacOS/link".to_owned(),
        kind: Kind::Link("a symbolic link".to_owned()),
        size: 0,
        head: Vec::new(),
    });
    assert!(scan(&linked).iter().any(|f| f.contains("link")));

    let mut big = mac_bundle_entries();
    big.retain(|e| e.path != "Contents/MacOS/dereth-client");
    big.push(Entry {
        size: dereth::FILE_CAP + 1,
        ..file("Contents/MacOS/dereth-client", b"x")
    });
    assert!(scan(&big).iter().any(|f| f.contains("cap")));

    let mut missing = mac_bundle_entries();
    missing.retain(|e| e.path != "Contents/Frameworks/libMoltenVK.dylib");
    assert!(scan(&missing).iter().any(|f| f.contains("missing")));
}

/// The Windows zip, written as the package writes it, reads back and passes the same scan, with
/// its one top-level folder taken off.
#[test]
fn a_written_windows_launcher_zip_reads_back_and_passes_the_scan() {
    let w = target(WINDOWS);
    let (required, optional) = dereth::contents(w);
    let members: Vec<Member> = required
        .iter()
        .map(|n| Member {
            name: n.clone(),
            bytes: n.as_bytes().to_vec(),
            executable: n.ends_with(".exe"),
        })
        .collect();
    let root = dereth::archive_root(w, V).expect("a zip");
    let bytes = archive::zip_bytes(&root, &members, 1_700_000_000).expect("zip");
    let entries = archive::relative_to_root(archive::zip_entries(&bytes).expect("reads"), &root);
    assert_eq!(
        guard::scan_tree(
            &entries,
            &required,
            &optional,
            dereth::FILE_CAP,
            dereth::TOTAL_CAP
        ),
        Vec::<String>::new()
    );
}

/// An AppImage must be an executable with no data header, under the cap, and named cleanly.
#[test]
fn an_appimage_is_an_executable_with_no_data_header() {
    let name = "Dereth-0.2.0-linux-x86_64.AppImage";
    let mut good = b"\x7fELF".to_vec();
    good.resize(0x400, 0);
    assert!(dereth::check_appimage(name, &good).is_empty());
    assert!(!dereth::check_appimage(name, b"#!/bin/sh\n").is_empty());
    let mut dat = good.clone();
    dat[0x140..0x144].copy_from_slice(&0x5442u32.to_le_bytes());
    assert!(dereth::check_appimage(name, &dat)
        .iter()
        .any(|f| f.contains("header")));
    assert!(!dereth::check_appimage("client_cell_1.dat", &good).is_empty());
}

/// MoltenVK is taken from its release's unpacked folder, or the folder holding it, and a Mach-O
/// library is recognised whether thin or universal.
#[test]
fn moltenvk_is_found_in_its_unpacked_release() {
    let base = std::env::temp_dir().join(format!("dereth-moltenvk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let top = base.join("MoltenVK");
    let lib = top.join("MoltenVK/dynamic/dylib/macOS");
    std::fs::create_dir_all(&lib).expect("mkdir");
    std::fs::write(lib.join("libMoltenVK.dylib"), [0xCA, 0xFE, 0xBA, 0xBE]).expect("dylib");
    std::fs::write(top.join("LICENSE"), "Apache").expect("licence");
    let (dylib, licence) = dereth::moltenvk_files(&top).expect("from its folder");
    assert!(dylib.ends_with("libMoltenVK.dylib"));
    assert!(licence.ends_with("LICENSE"));
    assert_eq!(dereth::moltenvk_files(&base).expect("from above").0, dylib);
    assert!(dereth::moltenvk_files(&lib).is_err());
    let _ = std::fs::remove_dir_all(&base);
    assert!(dereth::is_macho_library(&[0xCF, 0xFA, 0xED, 0xFE, 0, 0]));
    assert!(dereth::is_macho_library(&[0xCA, 0xFE, 0xBA, 0xBE]));
    assert!(!dereth::is_macho_library(b"MZ\0\0"));
}

// ---------------------------------------------------------------- the programs' headers

/// Dereth's Windows programs are windowed, carry their icon and link the C runtime in; a console
/// program, one without its icon, or one importing the Visual C++ runtime is refused.
#[test]
fn a_windows_program_is_windowed_with_its_icon_and_a_static_runtime() {
    let w = target(WINDOWS);
    let good = pe(0x8664, 2, &["KERNEL32.dll", "USER32.dll"], &[], true);
    let d = headers::check_app_binary(&good, w, targets::DERETH_GLIBC_FLOOR).expect("passes");
    assert!(d.contains("windowed"), "{d}");
    let console = pe(0x8664, 3, &["KERNEL32.dll"], &[], true);
    let e =
        headers::check_app_binary(&console, w, targets::DERETH_GLIBC_FLOOR).expect_err("console");
    assert!(e.iter().any(|p| p.contains("windowed")), "{e:?}");
    let bare = pe(0x8664, 2, &["KERNEL32.dll"], &[], false);
    assert!(headers::check_app_binary(&bare, w, targets::DERETH_GLIBC_FLOOR).is_err());
    let runtime = pe(0x8664, 2, &["VCRUNTIME140.dll"], &[], true);
    assert!(headers::check_app_binary(&runtime, w, targets::DERETH_GLIBC_FLOOR).is_err());
}

/// A Linux program may link the desktop's libraries, but no glibc symbol newer than the Ubuntu
/// 22.04 floor.
#[test]
fn a_linux_program_links_what_the_desktop_has_but_no_newer_glibc() {
    let l = target(LINUX);
    let interp = headers::linux_interp(targets::Arch::X86_64);
    let good = elf(
        62,
        interp,
        &["libwebkit2gtk-4.1.so.0", "libc.so.6"],
        &["GLIBC_2.34", "GLIBC_2.35"],
    );
    assert!(headers::check_app_binary(&good, l, targets::DERETH_GLIBC_FLOOR).is_ok());
    let newer = elf(62, interp, &["libc.so.6"], &["GLIBC_2.39"]);
    let e = headers::check_app_binary(&newer, l, targets::DERETH_GLIBC_FLOOR).expect_err("too new");
    assert!(e.iter().any(|p| p.contains("2.35")), "{e:?}");
}

/// Each Dereth target builds on its own operating system only.
#[test]
fn dereth_builds_natively_and_names_what_is_missing() {
    let host = |os| targets::Host {
        os: Some(os),
        zigbuild: true,
        zig: true,
        installed: None,
    };
    assert!(targets::plan_dereth(target(WINDOWS), &host(Os::Windows)).is_ok());
    assert!(targets::plan_dereth(target(LINUX), &host(Os::Windows)).is_err());
    assert!(targets::plan_dereth(target(MAC_X64), &host(Os::Mac)).is_ok());
    let missing = targets::Host {
        installed: Some(vec![MAC_ARM.to_owned()]),
        ..host(Os::Mac)
    };
    let e = targets::plan_dereth(target(MAC_X64), &missing).expect_err("not installed");
    assert!(e.contains("rustup target add"), "{e}");
    assert!(targets::find_dereth("aarch64-unknown-linux-gnu").is_err());
}

// ---------------------------------------------------------------- version and tag

/// The client, the launcher and the web client carry one version, which the tag must name exactly.
#[test]
fn the_client_and_the_launcher_carry_one_version_which_the_tag_names() {
    let ws = workspace_root();
    let v = Product::Dereth.version(&ws).expect("the two agree");
    version::parse_release_version(&v).expect("a release version");
    let tag = Product::Dereth.tag_for(&v);
    assert!(tag.starts_with("dereth-v"), "{tag}");
    assert!(version::check_product_tag(Product::Dereth, &tag, &v).is_ok());
    assert!(version::check_product_tag(Product::Dereth, &format!("refs/tags/{tag}"), &v).is_ok());
    let e = version::check_product_tag(Product::Dereth, &format!("empyrean-v{v}"), &v)
        .expect_err("another product's tag");
    assert!(e.contains("Dereth"), "{e}");
    assert!(version::check_product_tag(Product::Dereth, "dereth-v9.9.9", &v).is_err());
    assert_eq!(version::dereth_manifests(&ws).len(), 3);
}

/// Every file the Dereth package reads from the tree is there.
#[test]
fn every_file_the_dereth_package_reads_from_the_tree_is_there() {
    let ws = workspace_root();
    for path in [
        "dereth/about.toml",
        "dereth/about.hbs",
        "dereth/launcher/Cargo.toml",
        "dereth/launcher/tauri.conf.json",
        "dereth/launcher/assets/fonts/OFL-Cinzel.txt",
        "dereth/launcher/assets/fonts/OFL-EBGaramond.txt",
        "LICENSE",
    ] {
        assert!(ws.join(path).is_file(), "{path}");
    }
    for (_, _, path) in super::dereth::FONTS {
        assert!(ws.join(path).is_file(), "{path}");
    }
    let hbs = std::fs::read_to_string(ws.join("dereth/about.hbs")).expect("template");
    assert!(hbs.contains(super::notice::SECTION_START));
    assert!(hbs.contains(super::notice::SECTION_END));
}

/// The notice names the source, says there is no game data, carries the typefaces' licences, and
/// carries MoltenVK's only where the package carries MoltenVK.
#[test]
fn the_notice_names_the_source_and_carries_the_licences_the_package_needs() {
    let crates = [super::notice::Crate {
        name: "serde".to_owned(),
        version: "1.0.0".to_owned(),
        licence: "MIT OR Apache-2.0".to_owned(),
    }];
    let fonts = [
        ("Cinzel", "The page is set in Cinzel", "OFL text".to_owned()),
        (
            "Liberation",
            "The classic interface's text is drawn in Liberation",
            "Liberation OFL".to_owned(),
        ),
    ];
    let facts = |moltenvk| super::notice::DerethFacts {
        version: V,
        target: MAC_ARM,
        commit: "abc123",
        source_url: "https://github.com/dereth-network/dereth",
        crates: &crates,
        mit_licence: "MIT License",
        fonts: &fonts,
        moltenvk_licence: moltenvk,
    };
    let text = super::notice::dereth_notice(&facts(Some("Apache License")));
    assert!(text.contains("https://github.com/dereth-network/dereth/tree/abc123"));
    assert!(text.contains("releases/tag/dereth-v0.2.0"));
    assert!(text.contains("NO GAME DATA"));
    assert!(text.contains("MOLTENVK") && text.contains("Apache License"));
    assert!(text.contains("CINZEL") && text.contains("OFL text"));
    assert!(text.contains("THE LIBERATION TYPEFACE"));
    assert!(text.contains("drawn in Liberation, under the SIL Open Font License"));
    assert!(text.contains("Liberation OFL"));
    assert!(text.contains("serde 1.0.0"));
    assert!(text.contains("the launcher, with the Dereth client"));
    let windows = super::notice::dereth_notice(&facts(None));
    assert!(!windows.contains("MOLTENVK"));
    assert!(windows.contains("CINZEL"));
}

/// A Developer ID signs the macOS bundle under the hardened runtime; an ad-hoc signature leaves it
/// off, so the bundled MoltenVK is not refused by library validation.
#[test]
fn only_a_developer_id_turns_on_the_hardened_runtime() {
    let signed = dereth::mac_signing("MoltenVK".into(), Some("Developer ID Application: X (T)"));
    assert_eq!(signed["hardenedRuntime"], true);
    assert_eq!(signed["signingIdentity"], "Developer ID Application: X (T)");
    for none in [None, Some(""), Some("-"), Some("  ")] {
        let adhoc = dereth::mac_signing("MoltenVK".into(), none);
        assert_eq!(adhoc["hardenedRuntime"], false, "{none:?}");
        assert_eq!(adhoc["signingIdentity"], "-", "{none:?}");
        assert_eq!(adhoc["frameworks"][0], "MoltenVK");
    }
}

/// The launcher is built knowing the repository the release is downloaded from, and looks for its
/// updates among that repository's releases.
#[test]
fn a_release_build_is_told_the_repository_it_updates_from() {
    let facts = super::BuildFacts {
        commit: "a".repeat(40),
        branch: "dereth-v0.2.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: "https://github.com/someone/fork".to_owned(),
    };
    for triple in [WINDOWS, LINUX, MAC_ARM] {
        let env = dereth::build_env(&workspace_root(), &facts, target(triple));
        assert!(
            env.iter().any(|(k, v)| k == dereth::SOURCE_URL_ENV
                && v == "https://github.com/someone/fork"),
            "{triple}: {env:?}"
        );
    }
}
