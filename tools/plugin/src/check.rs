//! `check`: every check a host makes of a package when it loads it, and each of its scripts
//! compiled as the hosts compile it, so a mistake fails here and not in a player's client.

use std::path::Path;

use dereth_plugin_api::{ApiVersion, Limits};
use dereth_plugin_package::{is_package_root, Package};

/// Check the package in `folder`, or each package in it when it is a folder of them.
pub fn check(folder: &Path) -> Result<String, String> {
    if is_package_root(|f| folder.join(f).is_file()) {
        return check_one(folder);
    }
    let packages = crate::release::packages_in(folder)?;
    if packages.is_empty() {
        return Err(format!("{} holds no plugin", folder.display()));
    }
    let (mut said, mut failed) = (Vec::new(), false);
    for p in packages {
        match check_one(&p) {
            Ok(line) => said.push(line),
            Err(e) => {
                failed = true;
                said.push(e);
            }
        }
    }
    let text = said.join("\n");
    if failed {
        Err(text)
    } else {
        Ok(text)
    }
}

fn check_one(folder: &Path) -> Result<String, String> {
    let at = folder.display();
    let p = crate::read_folder(folder)?;
    let m = p.manifest().map_err(|e| format!("{at}: {e}"))?;
    if !ApiVersion::CURRENT.runs(m.api) {
        return Err(format!(
            "{at}: it needs plugin API {}, and the Dereth this tool comes with has {}",
            m.api,
            ApiVersion::CURRENT
        ));
    }
    let problems = problems(&p, &m.entry);
    if !problems.is_empty() {
        return Err(format!("{at}:\n  {}", problems.join("\n  ")));
    }
    let scripts = p.files.keys().filter(|f| is_script(f)).count();
    Ok(format!(
        "{} {}: ok ({} files, {scripts} scripts compiled)",
        m.id,
        m.version,
        p.files.len()
    ))
}

fn is_script(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".lua") || lower.ends_with(".luau")
}

/// What fails in a package whose manifest reads: a script that does not compile, or a compiled
/// entry the hosts would refuse.
fn problems(p: &Package, entry: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (path, bytes) in &p.files {
        if is_script(path) {
            if let Err(e) = dereth_plugin_lua::compile_check(path, bytes) {
                out.push(e);
            }
        } else if path == entry && path.to_ascii_lowercase().ends_with(".wasm") {
            let pages = (Limits::default().memory_bytes as u64 / 65_536).max(1);
            let checked = dereth_plugin_wasm::module::core_module(bytes)
                .and_then(|core| dereth_plugin_wasm::module::prepare(&core, pages).map(|_| ()));
            if let Err(e) = checked {
                out.push(format!("{path}: {e}"));
            }
        }
    }
    out
}
