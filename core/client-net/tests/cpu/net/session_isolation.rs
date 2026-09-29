//! Behaviour: none (checks data formats, fixture conformance or host contracts)
//! The client session sources name nothing of the transport (reads its own source files).
//! Fixture: recorded messages and synthetic state or packets.

use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the session's source directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// What the session may name from outside itself: its own module, the shared types, the numerics,
/// the message decoders and the standard library. The rest of `dereth-client-net` (socket, `Net`, wire,
/// crypto, the core it re-exports) is the transport, which the session reaches through `Transport`.
#[test]
fn the_session_names_nothing_of_the_transport() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.join("client_session.rs")];
    sources(&src.join("client_session"), &mut files);
    let forbidden = [
        "dereth_transport",
        "dereth_client_net::",
        "socket2",
        "serde_json",
    ];
    let mut bad = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a readable source file");
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let mut rest = code;
            while let Some(i) = rest.find("crate::") {
                let after = &rest[i + "crate::".len()..];
                let preceded = rest[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_');
                if !preceded && !after.starts_with("client_session") {
                    bad.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
                }
                rest = after;
            }
            for word in forbidden {
                if code.contains(word) {
                    bad.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "the client session names the transport:\n{}",
        bad.join("\n")
    );
}
