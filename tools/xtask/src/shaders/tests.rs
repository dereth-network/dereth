//! Behaviour: none (repository tool: the shader blobs, their manifest and the compiler pins)

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::*;

/// A compiler stand-in: a blob that is a pure function of the source folder's bytes, the entry
/// point and the target, counting its calls.
struct Fake {
    calls: Cell<usize>,
}

impl Fake {
    fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }
}

impl Compiler for Fake {
    fn compile(&self, dir: &Path, entry: &EntrySpec, target: Target) -> Result<Vec<u8>, String> {
        self.calls.set(self.calls.get() + 1);
        let source = std::fs::read(dir.join(&entry.source)).map_err(|e| e.to_string())?;
        let mut blob = format!("{}:{}:", entry.entry, target.name()).into_bytes();
        blob.extend_from_slice(sha256_hex(&lf(&source)).as_bytes());
        Ok(blob)
    }
}

/// A shader folder in a fresh temporary directory: a module, two sources and their spec.
fn folder(tag: &str) -> Root {
    let dir = std::env::temp_dir().join(format!("xtask-shaders-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("common.slang"), "module common;\n").unwrap();
    std::fs::write(
        dir.join("triangle.slang"),
        "import common;\nvoid vs_main() {}\nvoid fs_main() {}\n",
    )
    .unwrap();
    std::fs::write(dir.join("fill.slang"), "void cs_main() {}\n").unwrap();
    std::fs::write(
        dir.join(SPEC),
        "targets = [\"spirv\"]\n\n\
         [[entry]]\nsource = \"triangle.slang\"\nentry = \"vs_main\"\nstage = \"vertex\"\n\n\
         [[entry]]\nsource = \"triangle.slang\"\nentry = \"fs_main\"\nstage = \"fragment\"\n\n\
         [[entry]]\nsource = \"fill.slang\"\nentry = \"cs_main\"\nstage = \"compute\"\n",
    )
    .unwrap();
    Root {
        rel: format!("shaders-{tag}"),
        dir,
    }
}

fn compiled(root: &Root, rel: &str) -> PathBuf {
    root.dir.join(COMPILED).join(rel)
}

/// **A folder just compiled checks clean, and compiling it again writes nothing**: the manifest
/// and the index are what the sources make, so a second run changes no file.
#[test]
fn a_compiled_folder_checks_clean_and_recompiling_writes_nothing() {
    let root = folder("clean");
    let w = compile_root(&root, &Fake::new()).unwrap();
    assert_eq!(w.per_target.get(&Target::Spirv).map(|c| c.0), Some(3));
    assert_eq!(
        w.changed.len(),
        5,
        "three blobs, the manifest and the index"
    );
    assert!(
        compiled(&root, "spirv/triangle.vs_main.spv").is_file(),
        "blobs are named <source>.<entry>.<ext>"
    );
    assert_eq!(check_root(&root), Vec::<String>::new());
    let again = compile_root(&root, &Fake::new()).unwrap();
    assert!(again.changed.is_empty() && again.removed.is_empty());
    std::fs::remove_dir_all(&root.dir).ok();
}

/// **A source edited without regenerating fails `--check`, naming the source** -- the
/// calibration: a planted change to an imported module (which changes no blob's name) and to an
/// entry point's own source are both caught, and regenerating clears them.
#[test]
fn a_planted_source_change_fails_the_check_and_names_the_source() {
    let root = folder("planted");
    compile_root(&root, &Fake::new()).unwrap();
    std::fs::write(root.dir.join("common.slang"), "module common;\n// edited\n").unwrap();
    let found = check_root(&root);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("common.slang changed since its blobs were compiled"),
        "{found:?}"
    );
    std::fs::write(root.dir.join("fill.slang"), "void cs_main() { }\n").unwrap();
    assert_eq!(check_root(&root).len(), 2);
    let w = compile_root(&root, &Fake::new()).unwrap();
    assert_eq!(
        w.changed,
        ["spirv/fill.cs_main.spv", MANIFEST],
        "one edit rewrites only the blob its source makes, and the manifest"
    );
    assert_eq!(check_root(&root), Vec::<String>::new());
    std::fs::remove_dir_all(&root.dir).ok();
}

/// **A blob that is not the one recorded, one missing, one left over and an edited index each
/// fail the check**, and so does a manifest made with another compiler or other options.
#[test]
fn tampered_blobs_stray_files_an_edited_index_or_another_compiler_fail_the_check() {
    let root = folder("tampered");
    compile_root(&root, &Fake::new()).unwrap();

    std::fs::write(compiled(&root, "spirv/fill.cs_main.spv"), b"other").unwrap();
    let found = check_root(&root);
    assert!(
        found.len() == 1 && found[0].contains("spirv/fill.cs_main.spv is not the blob"),
        "{found:?}"
    );
    compile_root(&root, &Fake::new()).unwrap();

    std::fs::remove_file(compiled(&root, "spirv/triangle.fs_main.spv")).unwrap();
    assert!(check_root(&root)
        .iter()
        .any(|f| f.contains("spirv/triangle.fs_main.spv is missing")));
    compile_root(&root, &Fake::new()).unwrap();

    std::fs::write(compiled(&root, "spirv/old.vs_main.spv"), b"stale").unwrap();
    let found = check_root(&root);
    assert!(
        found.len() == 1 && found[0].contains("spirv/old.vs_main.spv is not a blob"),
        "{found:?}"
    );
    let w = compile_root(&root, &Fake::new()).unwrap();
    assert_eq!(
        w.removed,
        ["spirv/old.vs_main.spv"],
        "regenerating removes it"
    );

    let index = compiled(&root, INDEX);
    let text = std::fs::read_to_string(&index).unwrap();
    std::fs::write(&index, text.replace("vs_main", "vs_other")).unwrap();
    let found = check_root(&root);
    assert!(
        found.len() == 1 && found[0].contains("shaders.rs is not the index"),
        "{found:?}"
    );
    compile_root(&root, &Fake::new()).unwrap();

    let manifest = compiled(&root, MANIFEST);
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace(&compiler_line(), "slangc 0.0.1")).unwrap();
    let found = check_root(&root);
    assert!(
        found.len() == 1 && found[0].contains("the blobs were made with \"slangc 0.0.1\""),
        "{found:?}"
    );
    std::fs::write(
        &manifest,
        text.replace("-fvk-use-entrypoint-name", "-fvk-something-else"),
    )
    .unwrap();
    let found = check_root(&root);
    assert!(
        found.len() == 1 && found[0].contains("the spirv options are now"),
        "{found:?}"
    );
    std::fs::remove_dir_all(&root.dir).ok();
}

/// **An entry point added to `shaders.toml` without regenerating fails the check** (its blob is
/// missing and the spec's hash moved); a new source file does too, even before anything names it.
#[test]
fn a_new_entry_point_or_source_without_regenerating_fails_the_check() {
    let root = folder("added");
    compile_root(&root, &Fake::new()).unwrap();
    let spec = std::fs::read_to_string(root.dir.join(SPEC)).unwrap();
    std::fs::write(
        root.dir.join(SPEC),
        format!("{spec}\n[[entry]]\nsource = \"fill.slang\"\nentry = \"cs_other\"\nstage = \"compute\"\n"),
    )
    .unwrap();
    let found = check_root(&root);
    assert!(found
        .iter()
        .any(|f| f.contains("spirv/fill.cs_other.spv is missing")));
    assert!(found.iter().any(|f| f.contains("shaders.toml changed")));
    assert!(found
        .iter()
        .any(|f| f.contains("shaders.rs is not the index")));
    std::fs::write(root.dir.join(SPEC), spec).unwrap();
    std::fs::write(root.dir.join("later.slang"), "module later;\n").unwrap();
    let found = check_root(&root);
    assert!(
        found.len() == 1 && found[0].contains("later.slang is new"),
        "{found:?}"
    );
    std::fs::remove_dir_all(&root.dir).ok();
}

/// **A checkout that wrote CRLF line endings checks the same as the LF one** the blobs were made
/// from, for the sources, the manifest and the index.
#[test]
fn crlf_line_endings_in_a_checkout_do_not_fail_the_check() {
    let root = folder("crlf");
    compile_root(&root, &Fake::new()).unwrap();
    for p in [
        root.dir.join("triangle.slang"),
        root.dir.join(SPEC),
        compiled(&root, MANIFEST),
        compiled(&root, INDEX),
    ] {
        let text = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, text.replace('\n', "\r\n")).unwrap();
    }
    assert_eq!(check_root(&root), Vec::<String>::new());
    std::fs::remove_dir_all(&root.dir).ok();
}

/// **The index names every entry point once, embeds each target's blob and nothing else**, with
/// variants made from the source and the function.
#[test]
fn the_index_embeds_each_entry_points_blob_for_each_checked_in_target() {
    let spec = parse_spec(
        "targets = [\"msl\", \"spirv\"]\n[[entry]]\nsource = \"g-buffer.slang\"\n\
         entry = \"vs_main\"\nstage = \"vertex\"\n",
    )
    .unwrap();
    assert_eq!(spec.targets, [Target::Spirv, Target::Msl], "Target's order");
    let index = render_index(&spec);
    assert!(index.contains("    GBufferVsMain,\n"), "{index}");
    assert!(index.contains("pub const ALL: [Self; 1] = [Self::GBufferVsMain];"));
    assert!(index.contains("Self::GBufferVsMain => Stage::Vertex,"));
    assert!(index.contains("include_bytes!(\"spirv/g-buffer.vs_main.spv\")"));
    assert!(index.contains("include_str!(\"msl/g-buffer.vs_main.metal\")"));
    assert!(
        !index.contains("dxil"),
        "an unlisted target is not embedded"
    );
}

/// **`shaders.toml` is read strictly**: an unknown target, stage or key, a source that is not a
/// plain file name, and an entry point named twice are each refused.
#[test]
fn shaders_toml_refuses_unknown_targets_stages_keys_paths_and_repeats() {
    let entry = |source: &str, entry: &str, stage: &str| {
        format!("[[entry]]\nsource = \"{source}\"\nentry = \"{entry}\"\nstage = \"{stage}\"\n")
    };
    let ok = format!(
        "targets = [\"spirv\"]\n{}",
        entry("a.slang", "vs", "vertex")
    );
    assert!(parse_spec(&ok).is_ok());
    for (bad, why) in [
        (ok.replace("spirv", "hlsl"), "unknown target"),
        (ok.replace("vertex", "pixel"), "unknown stage"),
        (format!("{ok}extra = 1\n"), "unknown field"),
        (ok.replace("a.slang", "../a.slang"), "not a plain"),
        (ok.replace("a.slang", "a.hlsl"), "not a plain"),
        (ok.replace("\"vs\"", "\"vs-main\""), "not a function"),
        (
            format!("{ok}{}", entry("a.slang", "vs", "fragment")),
            "named twice",
        ),
        ("targets = []\n".to_owned(), "names none"),
    ] {
        let e = parse_spec(&bad).unwrap_err();
        assert!(e.contains(why), "{why}: {e}");
    }
}

/// **The checked-in shader folders are exactly what their sources make** -- `--check` over the
/// workspace, as `cargo xtask check` runs it -- and the folders are where the task looks.
#[test]
fn the_checked_in_shader_blobs_match_their_sources() {
    let ws = workspace_root();
    let found: Vec<String> = roots(&ws).iter().flat_map(check_root).collect();
    assert_eq!(found, Vec::<String>::new());
    for r in roots(&ws) {
        assert!(
            r.rel.ends_with("/shaders"),
            "{} is a crate's shaders folder",
            r.rel
        );
    }
}

/// **The arguments**: `--check` stands alone; `--dxil`, `--msl` and `--tools` go with a build.
#[test]
fn the_arguments_are_read_and_check_stands_alone() {
    let argv = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        parse_args(&argv(&["--dxil", "--msl", "--tools", "x"])).unwrap(),
        Args {
            check: false,
            extra: vec![Target::Dxil, Target::Msl],
            tools: Some(PathBuf::from("x")),
        }
    );
    assert!(parse_args(&argv(&["--check"])).unwrap().check);
    assert!(parse_args(&argv(&["--check", "--dxil"])).is_err());
    assert!(parse_args(&argv(&["--tools"])).is_err());
    assert!(parse_args(&argv(&["--spirv"])).is_err());
}

/// **Every host Slang publishes for is pinned to a SHA-256**, the URL is the release's own, and
/// DXC is pinned for Windows only.
#[test]
fn every_pinned_download_names_its_release_and_a_sha256() {
    for (os, arch, asset, sha) in fetch::SLANG_ASSETS {
        assert!(
            sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "{asset}"
        );
        assert!(asset.contains(fetch::SLANG_VERSION) && asset.ends_with(".zip"));
        let pin = fetch::slang_pin(os, arch).unwrap();
        assert_eq!(
            pin.url,
            format!(
                "https://github.com/shader-slang/slang/releases/download/v{}/{asset}",
                fetch::SLANG_VERSION
            )
        );
        assert_eq!(pin.folder, format!("slang-{}", fetch::SLANG_VERSION));
    }
    assert!(fetch::slang_pin("windows", "x86_64").is_some());
    assert!(fetch::slang_pin("freebsd", "x86_64").is_none());
    let dxc = fetch::dxc_pin("windows").unwrap();
    assert!(dxc.url.contains(fetch::DXC_TAG) && dxc.sha256.len() == 64);
    assert!(fetch::dxc_pin("linux").is_none());
}

/// **A download that does not match its pin is refused, naming both values**, whatever the case
/// of the hex.
#[test]
fn a_download_that_does_not_match_its_pin_is_refused() {
    let pin = fetch::slang_pin("windows", "x86_64").unwrap();
    assert!(fetch::verify(&pin, &pin.sha256.to_ascii_uppercase()).is_ok());
    let e = fetch::verify(&pin, &"0".repeat(64)).unwrap_err();
    assert!(
        e.contains(pin.sha256) && e.contains(&"0".repeat(64)) && e.contains("Nothing was cached")
    );
}

/// **The tools are cached in the per-user cache folder**, per host, and nowhere when the
/// variables it needs are absent or relative.
#[test]
fn the_tools_are_cached_in_the_per_user_cache_folder() {
    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> {
        let map: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).map(std::ffi::OsString::from)
    }
    let home = if cfg!(windows) {
        r"C:\Users\a"
    } else {
        "/home/a"
    };
    assert_eq!(
        fetch::cache_dir_for("windows", env(&[("LOCALAPPDATA", home)])),
        Some(PathBuf::from(home).join("Dereth").join("shader-tools"))
    );
    assert_eq!(
        fetch::cache_dir_for("linux", env(&[("HOME", home)])),
        Some(
            PathBuf::from(home)
                .join(".cache")
                .join("dereth/shader-tools")
        )
    );
    assert_eq!(
        fetch::cache_dir_for("macos", env(&[("HOME", home)])),
        Some(PathBuf::from(home).join("Library/Caches/Dereth/shader-tools"))
    );
    assert_eq!(
        fetch::cache_dir_for("windows", env(&[("LOCALAPPDATA", "relative")])),
        None
    );
    assert_eq!(fetch::cache_dir_for("linux", env(&[])), None);
}

/// **Only the parts of an archive a tool needs are unpacked**, under their own names, and an
/// entry that would leave the folder is refused before anything is written for it.
#[test]
fn unpacking_keeps_the_tools_parts_and_refuses_names_that_leave_the_folder() {
    use std::io::Write;
    let zip = |names: &[&str]| {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for n in names {
            w.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(n.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    };
    let dir = std::env::temp_dir().join(format!("xtask-shaders-unpack-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let kept = fetch::unpack(
        std::io::Cursor::new(zip(&[
            "bin/slangc.exe",
            "LICENSE",
            "LICENSES/Apache-2.0.txt",
            "share/doc/slang/README.md",
            "include/slang.h",
        ])),
        &dir,
        fetch::keep_slang,
    )
    .unwrap();
    assert_eq!(kept, 3);
    assert!(dir.join("bin/slangc.exe").is_file() && dir.join("LICENSE").is_file());
    assert!(!dir.join("share").exists() && !dir.join("include").exists());
    std::fs::remove_dir_all(&dir).ok();

    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        "x64"
    };
    assert_eq!(
        fetch::keep_dxc(&format!("bin/{arch}/dxcompiler.dll")).as_deref(),
        Some("dxcompiler.dll")
    );
    assert_eq!(fetch::keep_dxc("bin/x86/dxc.exe"), None);
    assert_eq!(
        fetch::keep_dxc("LICENSE-MS.txt").as_deref(),
        Some("LICENSE-MS.txt")
    );
    assert_eq!(
        fetch::safe_name("bin\\x64\\dxil.dll").as_deref(),
        Some("bin/x64/dxil.dll"),
        "DXC's zip names its entries with backslashes"
    );
    for bad in ["../x", "a/../../x", "/etc/x", "C:/x", ""] {
        assert_eq!(fetch::safe_name(bad), None, "{bad:?}");
    }
    let e = fetch::unpack(
        std::io::Cursor::new(zip(&["bin/../../x"])),
        &dir,
        fetch::keep_slang,
    )
    .unwrap_err();
    assert!(e.contains("leaves the folder"), "{e}");
    assert!(!dir.exists(), "nothing was written");
}

/// **An entry point's blob and variant come from its source and its function.**
#[test]
fn an_entry_points_blob_and_variant_come_from_its_source_and_function() {
    let e = EntrySpec {
        source: "g-buffer.slang".into(),
        entry: "fs_main".into(),
        stage: Stage::Fragment,
    };
    assert_eq!(e.blob(Target::Spirv), "spirv/g-buffer.fs_main.spv");
    assert_eq!(e.blob(Target::Dxil), "dxil/g-buffer.fs_main.dxil");
    assert_eq!(e.variant(), "GBufferFsMain");
}
