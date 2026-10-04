//! The persistence files, opened.
//!
//! UI persistence and keystone settings are serialized here.
//!
//! The *shape* of the screen-layout file — the sixteen-window table, the row format, the
//! parse/serialise pair — is [`dereth_client_contract::persist::screen_layout`], because a server, a tool
//! or a test can want it with no disk anywhere. Opening a path is the application's job, and this
//! is the application, so the three functions that touch one live here: the `fopen(path, "r")`
//! and `fopen(path, "w")` of retail's layout load and save, and the three-way branch that
//! decides which path they get.
//!
//! Their only caller is [`crate::ui::UiShell`].

use dereth_client_contract::persist::ScreenLayout;
use dereth_ui::UiError;

/// Choose the screen-layout path through the three original name cases.
///
/// The three path builders live in `dereth-client-contract`; this is the decision between them.
///
/// ```text
/// dir = parent directory of the preferences file
/// if (name == "#auto")   sprintf("%sUI-%s-%s-%d-%d.txt", dir, char, world, height, width)
/// else if (empty)        sprintf("%sUI-Default.txt", dir)
/// else                   sprintf("%s%s.txt", dir, name)
/// ```
///
/// The `#auto` name selects the character/world/height/width form, an empty name selects the
/// default file, and every other name selects the named form. The generated form orders `height`
/// before `width`; the tests pin them to unequal values so that ordering remains observable.
#[must_use]
pub fn layout_path(
    dir: &str,
    name: &str,
    character: &str,
    world: &str,
    height: i32,
    width: i32,
) -> String {
    if name == ScreenLayout::AUTO_NAME {
        ScreenLayout::auto_path(dir, character, world, height, width)
    } else if name.is_empty() {
        ScreenLayout::default_path(dir)
    } else {
        ScreenLayout::named_path(dir, name)
    }
}

/// Retail's layout load: `fopen(path, "r")`.
///
/// `Ok(None)` is "the file did not open", which callers treat as no layout loaded. A first run has
/// no file and that is not an
/// error. A file that opens and will not parse **is** an error, because the client's `fscanf`
/// loop would stop mid-file and leave a half-applied layout; refusing the whole file is the
/// safer end of that and is stated rather than silent.
///
/// # Errors
/// [`UiError::Persist`] when the file opens and does not parse, or cannot be read.
pub fn load_layout_file(path: &std::path::Path) -> Result<Option<ScreenLayout>, UiError> {
    let text = match dereth_client_runtime::platform::files::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(UiError::Persist(format!("{}: {e}", path.display()))),
    };
    let text = String::from_utf8(text)
        .map_err(|e| UiError::Persist(format!("{}: {e}", path.display())))?;
    Ok(Some(ScreenLayout::parse(&text)?))
}

/// Retail's layout save: `fopen(path, "w")` — falling back to `"w+"`, which is the
/// same truncate-or-create — then one `fwrite` per row with nothing between them.
///
/// Bytes, not lines: [`ScreenLayout::to_text`] is a single line and this writes it verbatim, so
/// the file is byte-identical to the client's. Writing it in text mode is what would insert a
/// `\r`, and there is nothing for it to insert one after.
///
/// # Errors
/// [`UiError::Persist`] when the file cannot be written.
pub fn save_layout_file(layout: &ScreenLayout, path: &std::path::Path) -> Result<(), UiError> {
    dereth_client_runtime::platform::files::write(path, layout.to_text().as_bytes())
        .map_err(|e| UiError::Persist(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_contract::persist::{SavedWindow, WINDOWS};

    /// The layout path branches the way create screen layout path does.
    #[test]
    fn the_layout_path_branches_the_way_create_screen_layout_path_does() {
        let d = "C:\\ac\\";
        assert_eq!(
            layout_path(d, "#auto", "Kupo", "Frostfell", 600, 800),
            "C:\\ac\\UI-Kupo-Frostfell-600-800.txt",
            "the #auto literal takes the character/world/height/width format"
        );
        assert_eq!(
            layout_path(d, "", "Kupo", "Frostfell", 600, 800),
            "C:\\ac\\UI-Default.txt",
            "an empty name is the packed-string length-1 branch: the default-layout literal"
        );
        assert_eq!(
            layout_path(d, "mine", "Kupo", "Frostfell", 600, 800),
            "C:\\ac\\mine.txt"
        );
        // Height before width, which is retail's own path order and is the reason
        // `to_text`'s `W:`/`H:` order is worth doubting. Pinned as unequal numbers on purpose: a
        // square window cannot tell the two apart.
        assert_ne!(
            layout_path(d, "#auto", "K", "F", 600, 800),
            layout_path(d, "#auto", "K", "F", 800, 600)
        );
    }

    /// The file on disk is the single line the client writes.
    #[test]
    fn the_file_on_disk_is_the_single_line_the_client_writes() {
        let dir = std::env::temp_dir().join("dereth-screen-layout");
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let path = dir.join("screen-layout.txt");
        let _ = std::fs::remove_file(&path);

        // "the file did not open" is `Ok(None)` and is what a first run sees.
        assert_eq!(
            load_layout_file(&path).expect("a missing file is not an error"),
            None
        );

        let mut l = ScreenLayout::default();
        for (i, s) in WINDOWS.iter().enumerate() {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let n = i as i32;
            l.windows.push((
                s.tag,
                SavedWindow {
                    x: n,
                    y: n + 1,
                    w: n + 2,
                    h: n + 3,
                },
            ));
        }
        save_layout_file(&l, &path).expect("the file is written");

        let bytes = std::fs::read(&path).expect("and read back");
        assert!(!bytes.contains(&b'\n'), "no newline reaches the disk");
        assert!(!bytes.contains(&b'\r'), "and no carriage return either");
        assert_eq!(
            bytes,
            l.to_text().as_bytes(),
            "the file is `to_text` verbatim"
        );
        assert_eq!(
            bytes.last(),
            Some(&b' '),
            "the format's own trailing space ends the file"
        );

        assert_eq!(
            load_layout_file(&path).expect("it parses").as_ref(),
            Some(&l)
        );
        std::fs::remove_file(&path).expect("cleanup");
    }
}
