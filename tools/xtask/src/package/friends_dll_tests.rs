//! Behaviour: none (tooling: the retail client's friends library in the launcher's packages: its
//! header check, where each package carries it, which host can have it, and the release
//! workflows that build it once and hand it to every system's job).

use super::auberean;
use super::dereth;
use super::friends_dll::{self, FILE};
use super::targets::{self, Os, Target};
use super::tests::pe;
use crate::util::workspace_root;

fn target(triple: &str) -> Target {
    targets::find_dereth(triple).expect("a desktop target")
}

/// `pe` (a PE32+ image) made a 32-bit x86 image (PE32, its data directories where PE32 keeps
/// them), a DLL when `dll`.
fn x86(imports: &[&str], dll: bool) -> Vec<u8> {
    let mut b = pe(0x14C, 2, imports, &[], false);
    let opt = 0x98;
    b[opt..opt + 2].copy_from_slice(&0x10Bu16.to_le_bytes());
    b[opt + 92..opt + 96].copy_from_slice(&16u32.to_le_bytes());
    b.copy_within(opt + 112..opt + 112 + 128, opt + 96);
    if dll {
        b[0x96..0x98].copy_from_slice(&0x2102u16.to_le_bytes());
    }
    b
}

/// The library passes as a 32-bit x86 DLL with no Visual C++ runtime; a 64-bit one, a program, or
/// one that imports the runtime is refused, naming why.
#[test]
fn the_library_is_a_32_bit_dll_with_the_c_runtime_linked_in() {
    let good = x86(&["KERNEL32.dll", "WS2_32.dll"], true);
    let d = friends_dll::check(&good).expect("passes");
    assert!(
        d.starts_with("PE32 i386 DLL") && d.contains("WS2_32.dll"),
        "{d}"
    );

    let wide = pe(0x8664, 2, &["KERNEL32.dll"], &[], false);
    assert!(friends_dll::check(&wide).unwrap_err()[0].contains("32-bit x86"));
    let program = x86(&["KERNEL32.dll"], false);
    assert!(friends_dll::check(&program)
        .unwrap_err()
        .iter()
        .any(|p| p == "not a DLL"));
    let runtime = x86(&["KERNEL32.dll", "VCRUNTIME140.dll"], true);
    assert!(friends_dll::check(&runtime)
        .unwrap_err()
        .iter()
        .any(|p| p.contains("crt-static")));
    assert!(friends_dll::check(b"not a program").is_err());
}

/// Every launcher package carries the library beside the launcher: in the Windows folder, and in
/// the macOS bundle's resources, where the launcher looks for it; a release file without it is
/// refused.
#[test]
fn both_launcher_packages_carry_the_library_beside_the_launcher() {
    for t in targets::DESKTOP_TRIPLES.iter().map(|t| target(t)) {
        let want = match t.os {
            Os::Windows => Some(FILE.to_owned()),
            Os::Mac => Some(format!("Contents/Resources/{FILE}")),
            // The AppImage's contents are checked as built, not listed.
            Os::Linux => None,
        };
        for (required, package) in [
            (dereth::contents(t).0, "dereth"),
            (auberean::contents(t).0, "auberean"),
        ] {
            match &want {
                Some(name) => assert!(required.contains(name), "{package} {}", t.triple),
                None => assert!(required.is_empty(), "{package} {}", t.triple),
            }
        }
    }
    // Never in the retail install, which no package holds.
    assert_eq!(FILE, "auberean_friends.dll");
    assert_eq!(friends_dll::TRIPLE, "i686-pc-windows-msvc");
}

/// A Windows host with the 32-bit target builds the library itself; any other host, or one
/// without the target, must be given it, and is told how.
#[test]
fn a_host_that_cannot_build_the_library_is_told_to_pass_one() {
    if std::env::var_os(friends_dll::ENV).is_some() {
        return;
    }
    let host = |os, installed: Option<&[&str]>| targets::Host {
        os: Some(os),
        zigbuild: false,
        zig: false,
        installed: installed.map(|i| i.iter().map(|s| (*s).to_owned()).collect()),
    };
    assert_eq!(
        friends_dll::refusal(None, &host(Os::Windows, Some(&[friends_dll::TRIPLE]))),
        None
    );
    assert_eq!(friends_dll::refusal(None, &host(Os::Windows, None)), None);
    let missing = friends_dll::refusal(None, &host(Os::Windows, Some(&[]))).unwrap();
    assert!(
        missing.contains("rustup target add i686-pc-windows-msvc"),
        "{missing}"
    );
    for os in [Os::Linux, Os::Mac] {
        let why = friends_dll::refusal(None, &host(os, None)).unwrap();
        assert!(why.contains("--friends-dll <file>"), "{why}");
        let given = std::path::Path::new("auberean_friends.dll");
        assert_eq!(friends_dll::refusal(Some(given), &host(os, None)), None);
    }
}

/// Both release workflows build the library once, on Windows with its target installed, and give
/// that same file to every system's package job, whose smoke run finds it in the package.
#[test]
fn the_release_workflows_build_the_library_once_and_every_package_carries_it() {
    let ws = workspace_root();
    for (file, product) in [
        (".github/workflows/release-dereth.yml", "dereth"),
        (".github/workflows/release-auberean.yml", "auberean"),
    ] {
        let workflow = std::fs::read_to_string(ws.join(file)).expect(file);
        for needed in [
            "  friends-library:\n".to_owned(),
            "--target i686-pc-windows-msvc".to_owned(),
            "cargo xtask package friends-dll --out friends-library".to_owned(),
            "needs: [version, friends-library]".to_owned(),
            format!(
                "cargo xtask package {product} --target ${{{{ matrix.target }}}} --out dist --friends-dll friends-library/{FILE}"
            ),
            "cmp \"$library\" \"friends-library/auberean_friends.dll\"".to_owned(),
        ] {
            assert!(workflow.contains(&needed), "{file}: {needed}");
        }
    }
}
