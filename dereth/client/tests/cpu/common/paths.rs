//! The fixture-path helpers every test module of the client's binaries uses: the checkout root,
//! the retail dats, the packet captures and the message corpus.
//!
//! Where two call sites wanted a different shape (a different fallback, a different return type,
//! an `expect` rather than a default) each shape has its own name here; none of them resolves to
//! a different directory from another.
//!
//! This file belongs to the `cpu` binary; the `dat` and `gpu` binaries declare it with
//! `#[path = "../../cpu/common/paths.rs"]` so that there is exactly one copy of it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The workspace root: the directory holding `fixtures/`, two levels above this crate. (The
/// parent is tried as well, for a tree that keeps `fixtures/` beside the workspace.)
pub fn workspace_root() -> &'static Path {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let ws = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if ws.join("fixtures").is_dir() {
            ws
        } else {
            ws.join("..")
        }
    })
}

/// [`workspace_root`] owned, for the call sites whose copy returned a `PathBuf`.
pub fn workspace_root_buf() -> PathBuf {
    workspace_root().to_path_buf()
}

/// `fixtures/`, spelled relative to the crate.
pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Where the retail client install and its dats are: [`dereth_dat::testing::dat_dir`], the one
/// lookup every test in the workspace uses (`DERETH_TEST_DAT_DIR`).
pub fn client_dir() -> PathBuf {
    dereth_dat::testing::dat_dir()
}

/// [`client_dir`], for the call sites that **require** the dats rather than defaulting: it
/// panics, naming `DERETH_TEST_DAT_DIR`, when they are not found.
pub fn client_dir_required() -> PathBuf {
    dereth_dat::testing::require_dats();
    dereth_dat::testing::dat_dir()
}

/// [`client_dir`], for the call sites whose fallback is this checkout's retail install: the
/// shared lookup already falls back to it.
pub fn client_dir_or_workspace_client() -> PathBuf {
    dereth_dat::testing::dat_dir()
}

/// The golden corpus, `fixtures/packet-captures/` (one `<slug>.jsonl` per recording), resolved from THIS checkout.
///
/// **Not an `Option`, and an `expect` never a skip**: the capture files are committed to the
/// repository, so an absent directory is a broken checkout and not a reason to pass.
///
/// It is resolved from this checkout, never as an absolute path into another one: the corpus is
/// tracked, so a run in a git worktree must read that worktree's recordings.
pub fn captures_dir() -> PathBuf {
    let root = workspace_root().join("fixtures/packet-captures");
    assert!(
        root.is_dir(),
        "the capture corpus is this test's oracle and is absent at {root:?}"
    );
    root
}

/// [`captures_dir`] without the `is_dir` assertion, for call sites that do not want it. A call
/// site that reads the directory will fail on its own read; one that only joins a name will not.
pub fn captures_dir_unchecked() -> PathBuf {
    workspace_root().join("fixtures/packet-captures")
}

/// [`captures_dir_unchecked`] under the name the call sites that build it from the workspace root
/// use; both resolve to the same directory.
pub fn captures_dir_relative() -> PathBuf {
    workspace_root().join("fixtures/packet-captures")
}

/// [`captures_dir`] as an `Option`, for the call sites written around one. **This is the shape a
/// caller can turn into a silent skip**; prefer [`captures_dir`].
pub fn captures_dir_opt() -> Option<PathBuf> {
    let root = workspace_root().join("fixtures/packet-captures");
    root.is_dir().then_some(root)
}

/// `fixtures/message-corpus`, the decoded-blob corpus.
pub fn netblobs_dir() -> PathBuf {
    let d = workspace_root().join("fixtures/message-corpus");
    assert!(
        d.is_dir(),
        "the netblob corpus is this test's oracle and is absent at {d:?}"
    );
    d
}
