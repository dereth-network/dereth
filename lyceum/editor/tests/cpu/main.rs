//! Behaviour: none (tooling: the world editor)
//! The editor's plain modules are unit-tested where they are; this binary holds what needs more
//! than one of them.

use std::collections::BTreeMap;
use std::path::Path;

mod fly;
mod looks;

#[test]
fn the_usage_names_the_scripted_mode() {
    assert!(lyceum_editor::args::USAGE.contains("--script"));
}

/// Every character past ASCII in `code`'s strings and character literals (not its comments), each
/// with a line it is on.
fn literal_characters(code: &str, out: &mut BTreeMap<char, String>, file: &str) {
    let chars: Vec<char> = code.chars().collect();
    let mut line = 1;
    let mut i = 0;
    let mut note = |c: char, line: usize| {
        if !c.is_ascii() {
            out.entry(c).or_insert_with(|| format!("{file}:{line}"));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match (c, next) {
            ('\n', _) => line += 1,
            // A line comment, to its end.
            ('/', Some('/')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            // A block comment.
            ('/', Some('*')) => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
                continue;
            }
            // A raw string: r"...", r#"..."#.
            ('r', Some('"' | '#')) if i == 0 || !chars[i - 1].is_alphanumeric() => {
                let mut hashes = 0;
                i += 1;
                while chars.get(i) == Some(&'#') {
                    hashes += 1;
                    i += 1;
                }
                if chars.get(i) != Some(&'"') {
                    continue;
                }
                i += 1;
                while i < chars.len() {
                    if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
                        i += 1 + hashes;
                        break;
                    }
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    note(chars[i], line);
                    i += 1;
                }
                continue;
            }
            // A string.
            ('"', _) => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    } else if chars[i] == '\n' {
                        line += 1;
                    } else {
                        note(chars[i], line);
                    }
                    i += 1;
                }
            }
            // Past ASCII anywhere else in code: a character literal.
            (c, _) => note(c, line),
        }
        i += 1;
    }
}

#[test]
fn every_character_the_editor_writes_on_screen_has_a_glyph_in_its_fonts() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found: BTreeMap<char, String> = BTreeMap::new();
    let mut sources: Vec<_> = std::fs::read_dir(dir.join("src"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    sources.sort();
    for path in &sources {
        let code = std::fs::read_to_string(path).unwrap();
        // Tests' own words are never drawn.
        let code = code.split("#[cfg(test)]").next().unwrap_or_default();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        literal_characters(code, &mut found, &name);
    }
    // Every line of the text, the creator's words included, is drawn.
    let text = std::fs::read_to_string(dir.join("lyceum-text.toml")).unwrap();
    for (n, l) in text.lines().enumerate() {
        for c in l.chars().filter(|c| !c.is_ascii()) {
            found
                .entry(c)
                .or_insert_with(|| format!("lyceum-text.toml:{}", n + 1));
        }
    }
    // The ellipsis a label cut short ends in.
    found
        .entry('…')
        .or_insert_with(|| "a label cut short".into());
    assert!(
        found.len() > 3,
        "the scan found the editor's words: {found:?}"
    );
    // The fonts the panels are drawn with, as egui has them: a character is drawn when a font of
    // each family maps it to a glyph of its own (else egui draws its replacement box).
    use skrifa::MetadataProvider;
    let gui = lyceum_editor::gui::Gui::new();
    let mut out = gui.ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    let defs = gui.ctx.fonts(|f| f.definitions().clone());
    let maps = |family: &egui::FontFamily, c: char| {
        defs.families[family].iter().any(|name| {
            let data = &defs.font_data[name];
            skrifa::FontRef::from_index(&data.font, data.index)
                .is_ok_and(|font| font.charmap().map(c).is_some())
        })
    };
    assert!(maps(&egui::FontFamily::Proportional, 'A'), "the fonts read");
    let lacking: Vec<String> = found
        .iter()
        .filter(|(c, _)| {
            !maps(&egui::FontFamily::Proportional, **c) || !maps(&egui::FontFamily::Monospace, **c)
        })
        .map(|(c, at)| format!("U+{:04X} {c} ({at})", u32::from(*c)))
        .collect();
    assert!(lacking.is_empty(), "no glyph for {lacking:#?}");
}

#[test]
fn the_literal_scan_reads_strings_and_characters_and_skips_comments() {
    let mut found = BTreeMap::new();
    literal_characters(
        "// ☃ comment\nlet a = \"x→y\"; /* ✓ */ let b = '…';\nlet c = r#\"▾\"#;\n",
        &mut found,
        "f",
    );
    let got: Vec<char> = found.keys().copied().collect();
    assert_eq!(got, ['…', '→', '▾']);
    assert_eq!(found[&'→'], "f:2");
    assert_eq!(found[&'▾'], "f:3");
}
