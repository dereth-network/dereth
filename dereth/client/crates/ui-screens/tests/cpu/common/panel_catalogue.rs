//! Panel catalogue and production-source scanning fixtures.
#![allow(dead_code)]

pub(crate) use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::path::{Path, PathBuf};

pub(crate) use dereth_ui_screens::panels::catalogue::{PanelSpec, COMBAT_PANEL_SPEC, PANELS};

// ---------------------------------------------------------------------------------------------
// The allow-list
// ---------------------------------------------------------------------------------------------

/// Catalogue entries without a production consumer, with the reason for each exception.
/// Each exception occupies one row and becomes invalid when a consumer appears.
pub(crate) const NO_CONSUMER_YET: &[(&str, &str)] = &[
    (
        "AdminPropertiesPanel",
        "catalogued inert on purpose: no recovered bindings or handlers",
    ),
    (
        "PanelStack",
        "FALSE RED: panels/panel_stack.rs is its module and names neither signal",
    ),
];

/// Why a bound id is accounted for although no production file writes it as a literal.
///
/// Three different facts about the client, deliberately not collapsed into one. The census prints
/// the keyword beside the id, so the matrix says which of the three a given absence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Why {
    /// **The client binds the element and never reads the field.** The absence here matches the
    /// absence there: wiring it would invent behaviour retail does not have.
    Inert,
    Layout,
    /// **A navigation-only handle this build reaches past.** Retail holds the element only to
    /// dereference it to something inside; this build resolves that something from a common
    /// ancestor. Both arrive at the same elements, and a station asserts that they do.
    Route,
}

impl Why {
    /// The word the census prints.
    pub(crate) const fn keyword(self) -> &'static str {
        match self {
            Self::Inert => "INERT",
            Self::Layout => "LAYOUT",
            Self::Route => "ROUTE",
        }
    }
}

pub(crate) const ID_ACCOUNTED_FOR: &[(u32, Why, &str)] = &[
    (0x1000_059D, Why::Inert, "The barber's face-choice element is stored during initialization and never read elsewhere [barber_face_choices.rs]"),
    (0x1000_00BE, Why::Layout, "vendor stock list 0x100000BD's H_SCROLLBAR, attribute 0x71; retail never names the literal"),
    (0x1000_00C6, Why::Layout, "the same, one page over: buy list 0x100000C5's H_SCROLLBAR [vendor_stock_scrollbar.rs]"),
    (0x1000_00CF, Why::Layout, "and the third of the three: sell basket 0x100000CE's H_SCROLLBAR, on the selling page in the vendor layout; retail never names the literal [vendor_selling_page.rs]"),
    (0x1000_01CE, Why::Route, "The backpack sub-panel routes to the top container and container list resolved by InventoryPanels [inventory_sub_panels.rs]"),
    (0x1000_01CF, Why::Route, "The 3D-items sub-panel routes to the item list resolved by InventoryPanels [inventory_sub_panels.rs]"),
    (0x1000_00E6, Why::Layout, "The health meter is bound as element type 7 [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_00EB, Why::Layout, "The health label is bound as element type 0xC [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_00EC, Why::Layout, "The stamina meter is bound as element type 7 [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_00ED, Why::Layout, "The stamina label is bound as element type 0xC [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_00EE, Why::Layout, "The mana meter is bound as element type 7 [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_00EF, Why::Layout, "The mana label is bound as element type 0xC [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_019E, Why::Layout, "The selected-object field is bound from the root and supplies the search root for its three children [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_019F, Why::Layout, "The selected-object name is bound beneath that field as type 0xC [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_01A1, Why::Layout, "The selected-object health meter is bound beneath that field as type 7 [vitals_and_toolbar_selection_bindings.rs]"),
    (0x1000_01A2, Why::Layout, "The selected-object mana meter is bound beneath that field as type 7 [vitals_and_toolbar_selection_bindings.rs]"),
];

pub(crate) const FULLY_ACCOUNTED: &[&str] = &[
    "BarberPanel",
    "InventoryPanelStack",
    "Toolbar",
    "VendorPanel",
    "VitalsPanel",
];

// ---------------------------------------------------------------------------------------------
// Reading the workspace
// ---------------------------------------------------------------------------------------------

/// The workspace root, four levels above this crate's manifest directory
/// (`dereth/client/crates/ui-screens`). The scan covers its `core/` and `dereth/`: the shared and
/// the client crates.
pub(crate) fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("the workspace is four levels up")
        .to_path_buf()
}

/// The files that hold panel behavior labels **descriptively**, and so can never be evidence that
/// anything consumes a panel.
///
/// The first two hold every behavior label by construction. The third was found by this very check and
/// is the same defect one level down: `panels/rows.rs` is a **second descriptive table** — seven
/// `RowAttribute` rows naming their panel — and it has **no production consumer at all**.
/// `ROW_ATTRIBUTES` and `FELLOWSHIP_VITAL_ELEMENTS` are referenced nowhere in the workspace, and
/// `row_attribute()`'s only caller is a `#[cfg(test)]` assertion in `panels/spellcomponent.rs`.
/// Counting it as a consumer greened four panels that have no module — `TitlesPanel`,
/// `FellowshipPanel`, `FriendsPanel`, `SquelchPanel` — on the strength of a table that is itself
/// unwired. **If `rows.rs` gains a production caller, delete its line here, not before.**
pub(crate) const NOT_EVIDENCE: &[&str] = &[
    "dereth/client/crates/ui-screens/src/panels/catalogue.rs",
    "dereth/client/crates/ui-screens/src/element_types.rs",
    "dereth/client/crates/ui-screens/src/panels/rows.rs",
];

/// The crates whose `src/` may hold a panel's element ids. Narrowing to these is what keeps the
/// weak `id` signal from matching an unrelated `0x1000_0074` in `dereth-animation` or `dereth-input`.
pub(crate) const ID_CRATES: &[&str] =
    &["dereth/client/crates/ui-screens/src/", "dereth/client/src/"];

/// Files that are a **dictionary of some other `0x1000_xxxx` namespace**, and so are a source of
/// pure coincidence for the `id` signal.
pub(crate) const ID_NAMESPACE_TABLES: &[&str] = &[
    "dereth/client/crates/ui-screens/src/bind.rs",
    "dereth/client/src/interaction.rs",
];

/// One production source file: its workspace-relative path and its code, comments and
/// `#[cfg(test)]` tail removed.
#[derive(Debug)]
pub(crate) struct Source {
    /// Relative to the workspace root, always with `/` separators.
    pub(crate) path: String,
    /// Production lines only, in file order, `(line number, code)`.
    pub(crate) lines: Vec<(usize, String)>,
    /// Every hex literal on a production line, `_` removed, that fits in a `u32`.
    pub(crate) hex: BTreeSet<u32>,
}

pub(crate) fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            let name = p
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// Strip line and block comments, tracking double-quoted strings so that a `//` inside one is
/// kept. Raw strings and char literals holding a slash are not modelled; neither appears in a way
/// that matters here, and erring toward *keeping* code would only ever remove a false red.
pub(crate) fn strip_comments(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_block = false;
    for (n, raw) in text.lines().enumerate() {
        let b: Vec<char> = raw.chars().collect();
        let mut code = String::new();
        let mut i = 0;
        let mut in_str = false;
        while i < b.len() {
            if in_block {
                if b[i] == '*' && i + 1 < b.len() && b[i + 1] == '/' {
                    in_block = false;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            if in_str {
                if b[i] == '\\' {
                    code.push(b[i]);
                    if i + 1 < b.len() {
                        code.push(b[i + 1]);
                    }
                    i += 2;
                    continue;
                }
                if b[i] == '"' {
                    in_str = false;
                }
                code.push(b[i]);
                i += 1;
                continue;
            }
            if b[i] == '"' {
                in_str = true;
                code.push(b[i]);
                i += 1;
                continue;
            }
            if b[i] == '/' && i + 1 < b.len() && b[i + 1] == '/' {
                break;
            }
            if b[i] == '/' && i + 1 < b.len() && b[i + 1] == '*' {
                in_block = true;
                i += 2;
                continue;
            }
            code.push(b[i]);
            i += 1;
        }
        if !code.trim().is_empty() {
            out.push((n + 1, code));
        }
    }
    out
}

/// The brace delta of one comment-stripped line, and whether it opened a block, ignoring braces
/// inside `"…"` string and `'…'` char literals.
///
/// [`strip_comments`] leaves string literals in place, and a test module is full of `format!`-style
/// braces inside them. Counting those would end the skip in the wrong place.
pub(crate) fn brace_delta(code: &str) -> (i32, bool) {
    let b: Vec<char> = code.chars().collect();
    let (mut depth, mut opened, mut i) = (0i32, false, 0usize);
    let mut in_str = false;
    while i < b.len() {
        if in_str {
            if b[i] == '\\' {
                i += 2;
                continue;
            }
            if b[i] == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match b[i] {
            '"' => in_str = true,
            // A char literal: `'{'`, `'}'`, `'\''`. Lifetimes (`'a`) have no closing quote, so
            // only skip when the third character closes it.
            '\'' if i + 2 < b.len() && b[i + 2] == '\'' => i += 2,
            '\'' if i + 3 < b.len() && b[i + 1] == '\\' && b[i + 3] == '\'' => i += 3,
            '{' => {
                depth += 1;
                opened = true;
            }
            '}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    (depth, opened)
}

/// Drop cfg test items.
pub(crate) fn drop_cfg_test_items(lines: Vec<(usize, String)>) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].1.trim_start().starts_with("#[cfg(test)]") {
            out.push(lines[i].clone());
            i += 1;
            continue;
        }
        // Walk to the end of the governed item. Further attributes may sit between the two.
        let mut depth = 0i32;
        let mut opened = false;
        while i < lines.len() {
            let code = &lines[i].1;
            let (delta, o) = brace_delta(code);
            depth += delta;
            opened |= o;
            i += 1;
            if opened {
                if depth <= 0 {
                    break;
                }
                continue;
            }
            // No body yet: `#[cfg(test)] mod tests;` ends here, a bare attribute line does not.
            if code.trim_end().ends_with(';') {
                break;
            }
        }
    }
    out
}

/// Every `0x…` literal in `code`, `_` removed, that fits in a `u32`.
pub(crate) fn hex_literals(code: &str, out: &mut BTreeSet<u32>) {
    let b: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == '0' && (b[i + 1] == 'x' || b[i + 1] == 'X') {
            let mut j = i + 2;
            let mut digits = String::new();
            while j < b.len() && (b[j].is_ascii_hexdigit() || b[j] == '_') {
                if b[j] != '_' {
                    digits.push(b[j]);
                }
                j += 1;
            }
            if !digits.is_empty() {
                if let Ok(v) = u32::from_str_radix(&digits, 16) {
                    out.insert(v);
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

/// Every production source file in the workspace, minus the files that are not evidence.
pub(crate) fn production_sources() -> Vec<Source> {
    let root = crates_dir();
    assert!(
        root.is_dir(),
        "{} is not a directory - the scan has nothing to look at",
        root.display()
    );
    let mut paths = Vec::new();
    collect_rs(&root.join("core"), &mut paths);
    collect_rs(&root.join("dereth"), &mut paths);
    assert!(
        paths.len() > 200,
        "only {} .rs files found under {} - the instrument is not pointed at the workspace",
        paths.len(),
        root.display()
    );

    let mut out = Vec::new();
    for p in paths {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        // Not production: integration tests, benches, examples, build scripts.
        if rel.contains("/tests/")
            || rel.contains("/benches/")
            || rel.contains("/examples/")
            || rel.ends_with("/build.rs")
        {
            continue;
        }
        if NOT_EVIDENCE.contains(&rel.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        // Drop each `#[cfg(test)]` item, brace-matched, and keep everything that follows it.
        // Comments are stripped first so that a brace inside one cannot unbalance the match.
        let lines = drop_cfg_test_items(strip_comments(&text));
        let mut hex = BTreeSet::new();
        for (_, code) in &lines {
            hex_literals(code, &mut hex);
        }
        out.push(Source {
            path: rel,
            lines,
            hex,
        });
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The two signals
// ---------------------------------------------------------------------------------------------

/// Every element id a panel's post-init or row templates name.
pub(crate) fn binding_ids(spec: &PanelSpec) -> BTreeSet<u32> {
    spec.children
        .iter()
        .chain(spec.templates.iter())
        .map(|c| c.id.0)
        .collect()
}

/// `file:line` of the first production line naming `"ExamplePanel"` as a string literal.
pub(crate) fn class_witness(sources: &[Source], class: &str) -> Option<String> {
    let needle = format!("\"{class}\"");
    for s in sources {
        for (n, code) in &s.lines {
            if code.contains(&needle) {
                return Some(format!("{}:{n}", s.path));
            }
        }
    }
    None
}

/// Every production file in a UI crate that carries one of `ids` as a hex literal, with how many
/// distinct ids of the panel it carries. Sorted most ids first, so the strongest witness is head.
pub(crate) fn id_witnesses(sources: &[Source], ids: &BTreeSet<u32>) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    if ids.is_empty() {
        return out;
    }
    for s in sources {
        if !ID_CRATES.iter().any(|c| s.path.starts_with(c)) {
            continue;
        }
        if ID_NAMESPACE_TABLES.contains(&s.path.as_str()) {
            continue;
        }
        let hit: BTreeSet<u32> = s.hex.intersection(ids).copied().collect();
        if hit.is_empty() {
            continue;
        }
        let mut first = String::new();
        for (n, code) in &s.lines {
            let mut here = BTreeSet::new();
            hex_literals(code, &mut here);
            if here.intersection(ids).next().is_some() {
                first = format!("{p}:{n}", p = s.path);
                break;
            }
        }
        out.push((
            hit.len(),
            format!("{first} [{} of {} ids]", hit.len(), ids.len()),
        ));
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.0));
    out
}

/// The `id` signal: the best witness, or `None`.
pub(crate) fn id_witness(sources: &[Source], ids: &BTreeSet<u32>) -> Option<String> {
    id_witnesses(sources, ids)
        .into_iter()
        .next()
        .map(|(_, w)| w)
}

/// Ids named in production.
pub(crate) fn ids_named_in_production(sources: &[Source]) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for s in sources {
        if !ID_CRATES.iter().any(|c| s.path.starts_with(c)) {
            continue;
        }
        if ID_NAMESPACE_TABLES.contains(&s.path.as_str()) {
            continue;
        }
        out.extend(s.hex.iter().copied());
    }
    out
}

/// `file:line` of the first production line naming `id`, for the stale message.
pub(crate) fn id_site(sources: &[Source], id: u32) -> Option<String> {
    for s in sources {
        if !ID_CRATES.iter().any(|c| s.path.starts_with(c)) {
            continue;
        }
        if ID_NAMESPACE_TABLES.contains(&s.path.as_str()) {
            continue;
        }
        if !s.hex.contains(&id) {
            continue;
        }
        for (n, code) in &s.lines {
            let mut here = BTreeSet::new();
            hex_literals(code, &mut here);
            if here.contains(&id) {
                return Some(format!("{}:{n}", s.path));
            }
        }
    }
    None
}

/// Every spec in the catalogue, including the one deliberately kept out of `PANELS`.
pub(crate) fn all_specs() -> Vec<&'static PanelSpec> {
    PANELS
        .iter()
        .chain(std::iter::once(&COMBAT_PANEL_SPEC))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The census and the gate
// ---------------------------------------------------------------------------------------------
