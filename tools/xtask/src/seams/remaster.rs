//! Rule 14: **the remaster changes only pixels.** It draws the world it is handed and touches
//! nothing that decides what the world is.
//!
//! - `dereth-gfx`, its backend layer, holds no workspace crate and none of
//!   [`REMASTER_TREE_FORBIDDEN`] in its tree (with every feature, for every target), names none of
//!   [`REMASTER_NAMES_NOT`] and no identifier starting with one of [`REMASTER_PREFIXES_NOT`] (the
//!   simulation, the world, the network, the scene, the client past the device and the server),
//!   and its only dependent is `dereth-remaster`.
//! - `dereth-remaster`'s tree holds no workspace crate but [`REMASTER_TREE_DERE`] and none of
//!   [`REMASTER_TREE_FORBIDDEN`]; its code names the same nothing; its only dependent is
//!   `dereth-scene`; and only the scene's own remaster files ([`REMASTER_FILES`]) name it.
//! - The Vulkan device's seam to it ([`VULKAN_SEAM`]) writes none of the device's own state but
//!   its own ([`VULKAN_SEAM_FIELDS_NOT_WRITTEN`]); it may call only [`VULKAN_SEAM_FIELD_READS`] on
//!   one of them.
//! - The browser and headless clients ([`SHIPPED_CLIENTS`]), each built on its own, hold none of
//!   it.
//!
//! The rule covers the remaster's crate from the moment it is a workspace member; before then
//! its half reports nothing.

use std::path::{Path, PathBuf};

use super::{
    blank_comments, cargo_tree_args, crate_sources, dedup, feature_tree, is_workspace_crate,
    leading_ident, names_ident, names_prefix, tree_violations, workspace_members, TreeEntry,
};
use crate::util::Report;

/// The remaster.
pub const REMASTER: &str = "dereth-remaster";

/// Its backend layer.
pub const GFX: &str = "dereth-gfx";

/// The workspace crates the remaster's tree may hold, at any depth, with every feature on.
pub const REMASTER_TREE_DERE: &[&str] = &[
    "dereth-remaster",
    "dereth-gfx",
    "dereth-render",
    "dereth-render-cpu",
    "dereth-primitives",
    "dereth-client-contract",
];

/// The remaster's only dependent, and the layer's.
pub const REMASTER_DEPENDENTS: &[&str] = &["dereth-scene"];
/// The backend layer's only dependent.
pub const GFX_DEPENDENTS: &[&str] = &["dereth-remaster"];

/// Platform crates the remaster's tree and the layer's never hold.
pub const REMASTER_TREE_FORBIDDEN: &[&str] = &["winit", "cpal"];

/// What the remaster's code and the layer's (comments stripped, their tests included) never name:
/// the simulation, the world, the network, the scene and the game types a picture has no business
/// with.
pub const REMASTER_NAMES_NOT: &[&str] = &[
    "dereth_physics",
    "dereth_animation",
    "dereth_world_data",
    "dereth_protocol",
    "dereth_transport",
    "dereth_rules",
    "dereth_scene",
    "WorldState",
    "PhysicsWorld",
    "MotionDriver",
    "PickScene",
    "UiRequest",
];

/// Prefixes the remaster's code and the layer's never name an identifier with: every client crate
/// past the device (`dereth_client_runtime`, `dereth_client_model`, `dereth_client_net`, ...) and
/// every server crate.
pub const REMASTER_PREFIXES_NOT: &[&str] = &["dereth_client_", "empyrean_"];

/// The files that name the remaster's crate: the scene's own remaster files, `remaster_*.rs` in
/// this folder (its bridge, which hands the remaster the frame, its decoder of the remaster's
/// assets, the remaster's view of the scene, and their tests).
pub const REMASTER_FILES: &str = "dereth/client/crates/scene/src/world_scene/remaster_";

/// The Vulkan device's seam to the remaster.
pub const VULKAN_SEAM: &str = "dereth/client/crates/render/src/vulkan/world_hook.rs";

/// The device's own state the seam never writes: everything but its own (`remaster`) and the
/// raw device it records commands with (`device`).
pub const VULKAN_SEAM_FIELDS_NOT_WRITTEN: &[&str] = &[
    "pipelines",
    "textures",
    "texture_sets",
    "texture_book",
    "draw_calls",
    "frame_stamp",
    "frames",
    "frame_index",
    "frame_open",
    "target",
    "swapchain",
    "render_pass",
    "config",
    "gamma",
    "next_fence_value",
    "pending_uploads",
    "released_in_frame",
    "retired_textures",
    "samplers",
    "sampler_pair_sets",
    "bound_sampler",
    "digest",
];

/// The methods the Vulkan seam may call on one of [`VULKAN_SEAM_FIELDS_NOT_WRITTEN`]: each only
/// reads. Any other method called on one of them counts as a write.
pub const VULKAN_SEAM_FIELD_READS: &[&str] = &[
    "borrow",
    "len",
    "is_empty",
    "iter",
    "get",
    "as_ref",
    "as_slice",
    "contains",
    "contains_key",
    "keys",
    "values",
    "first",
    "last",
    "clone",
    "to_vec",
    "chunks",
    "windows",
];

/// The clients that never carry the remaster, each package built on its own, as a release builds
/// it: the browser client and the headless one. The desktop client carries it by design.
pub const SHIPPED_CLIENTS: &[&str] = &["dereth-web", "dereth-headless"];

/// What a shipped client's feature tree never holds: the remaster, its layer, or a `remaster`
/// feature.
pub const SHIPPED_NOT_REMASTER: &[&str] = &[
    "dereth-remaster",
    "dereth-gfx",
    "dereth-render feature \"remaster\"",
    "dereth-scene feature \"remaster\"",
    "dereth-client feature \"remaster\"",
];

/// A crate's tree (every feature, every target, normal and build edges): no workspace crate
/// outside `allowed`, and none of [`REMASTER_TREE_FORBIDDEN`].
pub fn tree_rule(
    krate: &str,
    tree: &[TreeEntry],
    members: &[String],
    allowed: &[&str],
) -> Vec<String> {
    let mut out = tree_violations(krate, tree, REMASTER_TREE_FORBIDDEN);
    for e in tree {
        if is_workspace_crate(&e.name, members)
            && e.name != krate
            && !allowed.contains(&e.name.as_str())
        {
            out.push(format!(
                "{krate}'s tree contains `{}` (depth {}); allowed: {}",
                e.name,
                e.depth,
                if allowed.is_empty() {
                    "no workspace crate".to_owned()
                } else {
                    allowed.join(", ")
                }
            ));
        }
    }
    dedup(out)
}

/// A crate's code (`files` relative to the workspace, comments blanked): none of
/// [`REMASTER_NAMES_NOT`] and no identifier starting with one of [`REMASTER_PREFIXES_NOT`].
pub fn code_rule(files: &[(PathBuf, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in files {
        let code_text = blank_comments(text);
        for (i, (code, line)) in code_text.lines().zip(text.lines()).enumerate() {
            for n in REMASTER_NAMES_NOT {
                if names_ident(code, n) {
                    out.push(format!(
                        "{}:{}: names `{n}`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
            for p in REMASTER_PREFIXES_NOT {
                if names_prefix(code, p) {
                    out.push(format!(
                        "{}:{}: names `{p}...`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

/// A crate's direct dependents (`cargo tree -i` at depth 1): none but `allowed`.
pub fn dependents_rule(krate: &str, dependents: &[TreeEntry], allowed: &[&str]) -> Vec<String> {
    let out = dependents
        .iter()
        .filter(|e| e.depth == 1 && !allowed.contains(&e.name.as_str()))
        .map(|e| {
            format!(
                "`{}` depends on {krate}; only {} may",
                e.name,
                allowed.join(", ")
            )
        })
        .collect();
    dedup(out)
}

/// The workspace's Rust files (relative to it): only [`REMASTER_FILES`] and the remaster's and
/// the layer's own name `dereth_remaster`.
pub fn naming_rule(files: &[(PathBuf, String)]) -> Vec<String> {
    let unix = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut out = Vec::new();
    for (file, text) in files {
        let name = unix(file);
        if name.starts_with("dereth/client/crates/remaster/")
            || name.starts_with("dereth/client/crates/gfx/")
            || name.starts_with("tools/xtask/src/seams")
            || (name.starts_with(REMASTER_FILES)
                && name.ends_with(".rs")
                && !name[REMASTER_FILES.len()..].contains('/'))
        {
            continue;
        }
        let code_text = blank_comments(text);
        for (i, (code, line)) in code_text.lines().zip(text.lines()).enumerate() {
            if names_ident(code, "dereth_remaster") {
                out.push(format!(
                    "{}:{}: names `dereth_remaster` outside {REMASTER_FILES}*.rs: {}",
                    file.display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
    }
    out
}

/// The Vulkan device's seam (`text`, [`VULKAN_SEAM`]'s contents): no write to any of
/// [`VULKAN_SEAM_FIELDS_NOT_WRITTEN`]: an assignment, a compound assignment, a method that is not
/// one of [`VULKAN_SEAM_FIELD_READS`], or `&mut` on one.
pub fn vulkan_seam_rule(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let code_text = blank_comments(text);
    for (i, (code, line)) in code_text.lines().zip(text.lines()).enumerate() {
        let squeezed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        for f in VULKAN_SEAM_FIELDS_NOT_WRITTEN {
            let field = format!("self.{f}");
            let mut from = 0;
            while let Some(at) = squeezed[from..].find(&field) {
                let start = from + at;
                let end = start + field.len();
                from = end;
                let rest = &squeezed[end..];
                if rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    continue;
                }
                if writes(rest) || squeezed[..start].ends_with("&mut") {
                    out.push(format!(
                        "{VULKAN_SEAM}:{}: writes the device's `{f}`: {}",
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

/// Whether the code after a protected field writes it: an assignment, a compound assignment, a
/// method that is not a read, or either after an index or a field of it.
fn writes(rest: &str) -> bool {
    let mut rest = rest;
    // Past an index, or into a field of the field: `self.target.color.image` reads.
    loop {
        if rest.starts_with('[') {
            let Some(close) = rest.find(']') else {
                return false;
            };
            rest = &rest[close + 1..];
        } else if let Some(after) = rest.strip_prefix('.') {
            let name = leading_ident(after);
            if !name.is_empty() && !after[name.len()..].starts_with('(') {
                rest = &after[name.len()..];
                continue;
            }
            break;
        } else {
            break;
        }
    }
    if rest.starts_with('=') && !rest.starts_with("==") {
        return true;
    }
    if ["+=", "-=", "*=", "/=", "|=", "&=", "^=", "<<=", ">>="]
        .iter()
        .any(|op| rest.starts_with(op))
    {
        return true;
    }
    if let Some(after) = rest.strip_prefix('.') {
        let m = leading_ident(after);
        if !m.is_empty() && after[m.len()..].starts_with('(') {
            return !VULKAN_SEAM_FIELD_READS.contains(&m);
        }
    }
    false
}

/// One shipped client's feature tree (`lines`): none of [`SHIPPED_NOT_REMASTER`].
pub fn shipped_rule(package: &str, lines: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines.lines() {
        let line = line.trim();
        for n in SHIPPED_NOT_REMASTER {
            let hit = if n.contains(' ') {
                line.starts_with(n)
            } else {
                line.split_whitespace().next() == Some(n)
            };
            if hit {
                out.push(format!(
                    "`{package}`, built on its own, holds the remaster: {line}"
                ));
            }
        }
    }
    dedup(out)
}

type Row<'a> = &'a dyn Fn(&str, Result<Vec<String>, String>, String) -> Report;

/// Rule 14's rows.
pub fn reports(ws: &Path, workspace_rs: &[(PathBuf, String)], row: Row<'_>) -> Vec<Report> {
    let tree = |krate: &str| {
        cargo_tree_args(
            ws,
            &[
                "-e",
                "normal,build",
                "--all-features",
                "--target",
                "all",
                "-p",
                krate,
            ],
        )
    };
    let dependents = |krate: &str| {
        cargo_tree_args(
            ws,
            &[
                "--workspace",
                "--all-features",
                "-e",
                "normal,build",
                "-i",
                krate,
                "--depth",
                "1",
            ],
        )
    };
    let members = workspace_members(ws).unwrap_or_default();
    let has_remaster = members.iter().any(|m| m == REMASTER);
    let gfx_tree = tree(GFX).map(|t| tree_rule(GFX, &t, &members, &[]));
    let gfx_code = Ok(code_rule(&crate_sources(ws, GFX)));
    let gfx_dependents = dependents(GFX).map(|t| dependents_rule(GFX, &t, GFX_DEPENDENTS));
    let (remaster_tree, remaster_code, remaster_dependents) = if has_remaster {
        (
            tree(REMASTER).map(|t| tree_rule(REMASTER, &t, &members, REMASTER_TREE_DERE)),
            Ok(code_rule(&crate_sources(ws, REMASTER))),
            dependents(REMASTER).map(|t| dependents_rule(REMASTER, &t, REMASTER_DEPENDENTS)),
        )
    } else {
        (Ok(Vec::new()), Ok(Vec::new()), Ok(Vec::new()))
    };
    let naming = Ok(naming_rule(workspace_rs));
    let seam = std::fs::read_to_string(ws.join(VULKAN_SEAM))
        .map(|t| vulkan_seam_rule(&t))
        .map_err(|e| format!("{VULKAN_SEAM}: {e}"));
    let shipped = SHIPPED_CLIENTS
        .iter()
        .map(|p| feature_tree(ws, p).map(|t| shipped_rule(p, &t)))
        .try_fold(Vec::new(), |mut all, v| {
            all.extend(v?);
            Ok::<_, String>(all)
        });
    vec![
        row(
            "seam: remaster layer deps",
            gfx_tree,
            format!("{GFX} reaches no workspace crate"),
        ),
        row(
            "seam: remaster layer code",
            gfx_code,
            "names no simulation, world, network, server or scene crate or type".to_owned(),
        ),
        row(
            "seam: remaster layer dependents",
            gfx_dependents,
            format!("only {} depends on {GFX}", GFX_DEPENDENTS.join(", ")),
        ),
        row(
            "seam: remaster deps",
            remaster_tree,
            format!(
                "{REMASTER} reaches no workspace crate but {}",
                REMASTER_TREE_DERE.join(", ")
            ),
        ),
        row(
            "seam: remaster code",
            remaster_code,
            "names no simulation, world, network, server or scene crate or type".to_owned(),
        ),
        row(
            "seam: remaster dependents",
            remaster_dependents,
            format!(
                "only {} depends on {REMASTER}",
                REMASTER_DEPENDENTS.join(", ")
            ),
        ),
        row(
            "seam: remaster naming",
            naming,
            format!("only the scene's {REMASTER_FILES}*.rs files name it"),
        ),
        row(
            "seam: remaster device seam",
            seam,
            "the Vulkan device's seam writes none of the device's state but its own".to_owned(),
        ),
        row(
            "seam: browser and headless clients without the remaster",
            shipped,
            format!(
                "{}, each built on its own for every target, hold no remaster",
                SHIPPED_CLIENTS.join(", ")
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ORACLE: none needed -- these calibrate the checker against planted violations, because a
    // seam check that silently stops firing is worse than none.

    fn tree(rows: &[(usize, &str)]) -> Vec<TreeEntry> {
        rows.iter()
            .map(|&(depth, name)| TreeEntry {
                depth,
                name: name.to_owned(),
            })
            .collect()
    }

    #[test]
    fn the_layer_reaches_no_workspace_crate_and_the_remaster_only_its_own() {
        let members = vec!["dereth-gfx".to_owned(), "dereth-scene".to_owned()];
        let clean = tree(&[(0, "dereth-gfx"), (1, "ash"), (1, "gpu-allocator")]);
        assert!(tree_rule(GFX, &clean, &members, &[]).is_empty());
        let reaching = tree(&[(0, "dereth-gfx"), (1, "dereth-primitives"), (1, "winit")]);
        assert_eq!(tree_rule(GFX, &reaching, &members, &[]).len(), 2);
        let remaster = tree(&[
            (0, "dereth-remaster"),
            (1, "dereth-gfx"),
            (1, "dereth-render"),
            (2, "dereth-render-cpu"),
        ]);
        assert!(tree_rule(REMASTER, &remaster, &members, REMASTER_TREE_DERE).is_empty());
        let into_scene = tree(&[(0, "dereth-remaster"), (1, "dereth-scene")]);
        assert_eq!(
            tree_rule(REMASTER, &into_scene, &members, REMASTER_TREE_DERE).len(),
            1
        );
    }

    #[test]
    fn the_code_names_no_simulation_world_or_scene() {
        let files = vec![
            (
                PathBuf::from("dereth/client/crates/remaster/src/a.rs"),
                "use dereth_physics::World;\n// dereth_scene in a comment is fine\nfn f(_: &WorldState) {}\nuse dereth_scene::X;\nuse dereth_client_runtime::Y;\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("dereth/client/crates/gfx/src/b.rs"),
                "use ash::vk;\n".to_owned(),
            ),
        ];
        let found = code_rule(&files);
        assert_eq!(found.len(), 4, "{found:#?}");
    }

    #[test]
    fn only_the_listed_dependents_and_files_reach_the_remaster() {
        let ok = tree(&[(0, "dereth-gfx"), (1, "dereth-remaster")]);
        assert!(dependents_rule(GFX, &ok, GFX_DEPENDENTS).is_empty());
        let other = tree(&[
            (0, "dereth-gfx"),
            (1, "dereth-remaster"),
            (1, "dereth-client"),
        ]);
        assert_eq!(dependents_rule(GFX, &other, GFX_DEPENDENTS).len(), 1);
        let files = vec![
            (
                PathBuf::from(format!("{REMASTER_FILES}bridge.rs")),
                "use dereth_remaster::scene;\n".to_owned(),
            ),
            (
                PathBuf::from("dereth/client/crates/scene/src/world_scene/imp.rs"),
                "use dereth_remaster::scene;\n".to_owned(),
            ),
        ];
        assert_eq!(naming_rule(&files).len(), 1);
    }

    #[test]
    fn the_device_seam_writes_only_its_own_state() {
        let clean = "fn f(&mut self) {\n    self.remaster.ended = true;\n    let c = self.frames[self.frame_index].cmd;\n    let i = self.target.color.image;\n    unsafe { self.device.cmd_end_render_pass(c) };\n    self.install_viewport(v);\n}\n";
        assert!(
            vulkan_seam_rule(clean).is_empty(),
            "{:#?}",
            vulkan_seam_rule(clean)
        );
        let writing = "fn f(&mut self) {\n    self.frame_stamp += 1;\n    self.draw_calls = 0;\n    self.textures.clear();\n    let t = &mut self.target;\n    self.frames[0].fence_value = 3;\n}\n";
        assert_eq!(
            vulkan_seam_rule(writing).len(),
            5,
            "{:#?}",
            vulkan_seam_rule(writing)
        );
    }

    #[test]
    fn a_shipped_client_holds_no_remaster() {
        let lines = "dereth-web v0.0.0\ndereth-render feature \"wgpu\"\ndereth-render feature \"remaster\"\ndereth-gfx v0.0.0\n";
        assert_eq!(shipped_rule("dereth-web", lines).len(), 2);
        assert!(shipped_rule("dereth-web", "dereth-web v0.0.0\n").is_empty());
    }
}
