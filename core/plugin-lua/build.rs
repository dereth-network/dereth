//! For `wasm32-unknown-unknown` only: builds the C and C++ runtime the Luau VM links against in a
//! browser, `libluaurt.a`. That target has no C library and no C++ runtime of its own, so the
//! archive carries the C++ exception runtime and the few out-of-line parts of the C++ standard
//! library Luau uses, compiled from the LLVM sources zig ships, and the small C library in `rt/`
//! (the rest of it, the allocator, `strtod` and number formatting, is Rust, in `src/wasm_rt.rs`).
//! The Luau build is told to link the archive through `CXXSTDLIB_wasm32_unknown_unknown=luaurt`,
//! which the web client's build sets together with the compiler (`dereth/web/README.md`).
//!
//! On every other target this does nothing: Luau is built with the platform's own compiler and
//! runtime.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=rt");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LUAU_CXXFLAGS");
    println!("cargo:rerun-if-env-changed=LUAU_INCLUDE_DIRS");
    println!("cargo:rerun-if-env-changed=ZIG");
    if std::env::var("TARGET").as_deref() != Ok("wasm32-unknown-unknown") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let z = zig_lib_dir();
    let base: Vec<String> = std::env::var("LUAU_CXXFLAGS")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    // In the platform's path-list form, as Luau's own build reads it.
    let incs: Vec<String> =
        std::env::split_paths(&std::env::var_os("LUAU_INCLUDE_DIRS").unwrap_or_default())
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| format!("-I{}", p.display()))
            .collect();

    let abi = z.join("libcxxabi/src");
    let cxx = z.join("libcxx/src");
    let unw = z.join("libunwind/src");
    let mut jobs: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let abi_flags = vec![
        "-D_LIBCXXABI_BUILDING_LIBRARY".into(),
        "-D_LIBCPP_BUILDING_LIBRARY".into(),
        "-D_LIBCXXABI_HAS_NO_THREADS".into(),
        "-D_LIBCXXABI_USE_LLVM_UNWINDER".into(),
        format!("-I{}", z.join("libunwind/include").display()),
        format!("-I{}", cxx.display()),
        "-std=c++23".into(),
    ];
    // The C++ exception runtime, whole: libc++abi's sources, by stem.
    for f in [
        "abort_message",
        "cxa_aux_runtime",
        "cxa_default_handlers",
        "cxa_exception",
        "cxa_exception_storage",
        "cxa_guard",
        "cxa_handlers",
        "cxa_personality",
        "cxa_vector",
        "cxa_virtual",
        "fallback_malloc",
        "private_typeinfo",
        "stdlib_exception",
        "stdlib_new_delete",
        "stdlib_stdexcept",
        "stdlib_typeinfo",
    ] {
        jobs.push((abi.join(cpp(f)), abi_flags.clone()));
    }
    let cxx_flags = vec![
        "-D_LIBCPP_BUILDING_LIBRARY".into(),
        "-DLIBCXX_BUILDING_LIBCXXABI".into(),
        format!("-I{}", cxx.display()),
        "-std=c++23".into(),
    ];
    // The out-of-line parts of the C++ standard library the VM uses: libc++'s sources, by stem.
    for f in ["string", "hash", "functional", "stdexcept", "verbose_abort"] {
        jobs.push((cxx.join(cpp(f)), cxx_flags.clone()));
    }
    jobs.push((
        unw.join("Unwind-wasm.c"),
        vec![
            "-D_LIBUNWIND_HIDE_SYMBOLS".into(),
            format!("-I{}", z.join("libunwind/include").display()),
            format!("-I{}", unw.display()),
        ],
    ));
    jobs.push((PathBuf::from("rt/libc.c"), vec![]));
    jobs.push((Path::new("rt").join(cpp("rt")), vec!["-std=c++17".into()]));

    let objs: Vec<PathBuf> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|(src, extra)| {
                let (base, incs, out) = (&base, &incs, &out);
                s.spawn(move || compile(src, extra, base, incs, out))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a compile thread finished"))
            .collect()
    });
    let lib = out.join("libluaurt.a");
    let _ = std::fs::remove_file(&lib);
    let st = Command::new(zig())
        .arg("ar")
        .arg("rcs")
        .arg(&lib)
        .args(&objs)
        .status()
        .expect("zig ar runs");
    assert!(st.success(), "zig ar failed");
    println!("cargo:rustc-link-search=native={}", out.display());
}

/// A C++ source file's name from its stem.
fn cpp(stem: &str) -> String {
    format!("{stem}.cpp")
}

/// The zig to run: `$ZIG`, or `zig` on the path.
fn zig() -> String {
    std::env::var("ZIG").unwrap_or_else(|_| "zig".to_owned())
}

/// zig's library folder, where its copies of the LLVM runtime sources and the C headers are.
fn zig_lib_dir() -> PathBuf {
    let out = Command::new(zig()).arg("env").output().unwrap_or_else(|e| {
        panic!(
            "the plugin engine's WebAssembly build needs zig (0.15 or later) on the path, or \
             named by ZIG: {e}"
        )
    });
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s
        .lines()
        .find(|l| l.contains(".lib_dir"))
        .expect("`zig env` names its lib_dir");
    let v = line
        .split('"')
        .nth(1)
        .expect("lib_dir is quoted")
        .replace("\\\\", "\\");
    PathBuf::from(v)
}

fn compile(src: &Path, extra: &[String], base: &[String], incs: &[String], out: &Path) -> PathBuf {
    let stem = src
        .file_name()
        .expect("a source file")
        .to_string_lossy()
        .replace('.', "_");
    let o = out.join(format!("{stem}.o"));
    let is_c = src.extension().and_then(|e| e.to_str()) == Some("c");
    let mut cmd = Command::new(zig());
    cmd.arg("clang");
    if is_c {
        cmd.arg("-xc");
    }
    cmd.args(base)
        .args(extra)
        .args(incs)
        .args([
            "-Os",
            "-w",
            "-ffunction-sections",
            "-fdata-sections",
            "-DNDEBUG",
            "-Irt",
        ])
        .arg("-c")
        .arg(src)
        .arg("-o")
        .arg(&o);
    let res = cmd.output().expect("zig clang runs");
    assert!(
        res.status.success(),
        "compile {}:\n{}",
        src.display(),
        String::from_utf8_lossy(&res.stderr)
    );
    o
}
