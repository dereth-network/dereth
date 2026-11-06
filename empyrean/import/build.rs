//! What built this converter, for the provenance an ac1999 bundle records: the commit checked out,
//! the SHA-256 of the workspace's `Cargo.lock`, the compiler, and the pinned ZIP writer's crates.
//! Each is `unknown` when it cannot be read (a source tree with no Git checkout, say).

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}

/// `name`'s version in the lock file.
fn locked(lock: &str, name: &str) -> String {
    let key = format!("name = \"{name}\"\nversion = \"");
    lock.split(&key)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .map_or_else(|| "unknown".to_owned(), |v| format!("{name} {v}"))
}

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let lock_path = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock_path.display());
    let lock = std::fs::read(&lock_path).unwrap_or_default();
    let lock_sha = if lock.is_empty() {
        "unknown".to_owned()
    } else {
        format!("{:x}", Sha256::digest(&lock))
    };
    let commit = git(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    // Build again when the checkout moves to another commit.
    for what in ["HEAD", "packed-refs"] {
        if let Some(p) = git(&root, &["rev-parse", "--git-path", what]) {
            println!("cargo:rerun-if-changed={}", root.join(p).display());
        }
    }
    if let Some(head) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(&root, &["rev-parse", "--git-path", &head]) {
            println!("cargo:rerun-if-changed={}", root.join(p).display());
        }
    }
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let toolchain = Command::new(rustc)
        .arg("-V")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |v| v.trim().to_owned());
    let lock_text = String::from_utf8_lossy(&lock);
    let zip = ["zip", "flate2", "zlib-rs"]
        .map(|n| locked(&lock_text, n))
        .join(", ");
    println!("cargo:rustc-env=EMPYREAN_IMPORT_COMMIT={commit}");
    println!("cargo:rustc-env=EMPYREAN_IMPORT_CARGO_LOCK_SHA256={lock_sha}");
    println!("cargo:rustc-env=EMPYREAN_IMPORT_TOOLCHAIN={toolchain}");
    println!("cargo:rustc-env=EMPYREAN_IMPORT_ZIP={zip}");
}
