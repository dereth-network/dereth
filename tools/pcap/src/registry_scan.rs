//! Test support: list every `Message` type in `dereth-protocol`'s source, so the registry test can
//! require the registry to name them all.
//!
//! Two shapes define a message there: a direct `impl Message for Name` at the top level of a
//! module, and an invocation of a module-local `macro_rules!` whose body implements `Message` for
//! its first argument (for `quality_messages!`, for every `Name = OPCODE` line of the block).

use std::path::Path;

/// `module::Type` for every message, in file order.
pub fn message_types(src: &Path) -> Vec<String> {
    let mut files: Vec<_> = std::fs::read_dir(src)
        .expect("core/protocol/src is readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let module = f
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let text = std::fs::read_to_string(&f)
            .expect("readable")
            .replace("\r\n", "\n");
        // Test modules define helper messages of their own.
        let text = text
            .split("\n#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let mut macros = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find("macro_rules! ") {
            let after = &rest[i + "macro_rules! ".len()..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let end = after.find("\n}").unwrap_or(after.len());
            if after[..end].contains("impl Message for") {
                macros.push(name);
            }
            rest = &after[end..];
        }
        for line in text.split('\n') {
            if let Some(t) = line.strip_prefix("impl Message for ") {
                let name: String = t
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                out.push(format!("{module}::{name}"));
            }
        }
        for m in &macros {
            let pat = format!("\n{m}!");
            let mut rest = text;
            while let Some(i) = rest.find(&pat) {
                let after = &rest[i + pat.len()..];
                let end = after
                    .find(");")
                    .or_else(|| after.find("};"))
                    .unwrap_or(after.len());
                let body: String = after[..end]
                    .lines()
                    .filter(|l| {
                        !l.trim_start().starts_with("///") && !l.trim_start().starts_with("#[")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let body = body.trim_start().trim_start_matches(['(', '{']);
                if m == "quality_messages" {
                    for part in body.split(';') {
                        if let Some((name, _)) = part.split_once('=') {
                            let name = name.trim();
                            if !name.is_empty() {
                                out.push(format!("{module}::{name}"));
                            }
                        }
                    }
                } else {
                    let name = body.split(',').next().unwrap_or_default().trim();
                    out.push(format!("{module}::{name}"));
                }
                rest = &after[end..];
            }
        }
    }
    out
}
