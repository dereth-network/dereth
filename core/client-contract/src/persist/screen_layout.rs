//! The screen-layout file: sixteen windows, one line each, in a fixed order.
//!
//! The floating in-game windows remember their position and size in a plain text file **per
//! character and per resolution** — which is why the resolution is baked into the file name: the
//! coordinates saved are the window's screen x and y, width and height, absolute
//! screen pixels.
//!
//! A separate generic location-save mechanism was observed in the original client.
//! Its storage was a nested map from layout ids to element locations, but only
//! static initialization survived and no runtime caller reached it.
//! That unused mechanism is deliberately not implemented in this Rust client.
//!
//! The table, the row format and the parse/serialise pair are the shape of the file and belong to
//! whoever describes it; the `fopen` pair and the path the caller resolves before it —
//! `load_layout_file`, `save_layout_file` and the three-way `layout_path` branch — are
//! `dereth_client_shell::persist`, because a contract crate that touches `std::fs` is not a contract.
//! The three path *formats* the branch chooses between ([`ScreenLayout::auto_path`],
//! [`ScreenLayout::named_path`], [`ScreenLayout::default_path`]) and the
//! [`ScreenLayout::AUTO_NAME`] literal that selects the first are here, because they are the
//! client's `sprintf` formats and carry no disk.

use super::PersistError;
use crate::ElementId;

/// One row of the fixed sixteen-window table, in the order used to write and read screen layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSlot {
    pub tag: &'static str,
    pub element: ElementId,
    pub what: &'static str,
}

/// The sixteen windows, in file order.
pub const WINDOWS: [WindowSlot; 16] = [
    WindowSlot {
        tag: "<SBOX>",
        element: ElementId(0x1000_049A),
        what: "the 3D world viewport",
    },
    WindowSlot {
        tag: "<CHAT>",
        element: ElementId(0x1000_0601),
        what: "the main chat window",
    },
    WindowSlot {
        tag: "<FCH1>",
        element: ElementId(0x1000_0505),
        what: "floating chat window 1",
    },
    WindowSlot {
        tag: "<FCH2>",
        element: ElementId(0x1000_050E),
        what: "floating chat window 2",
    },
    WindowSlot {
        tag: "<FCH3>",
        element: ElementId(0x1000_050F),
        what: "floating chat window 3",
    },
    WindowSlot {
        tag: "<FCH4>",
        element: ElementId(0x1000_0510),
        what: "floating chat window 4",
    },
    WindowSlot {
        tag: "<EXAM>",
        element: ElementId(0x1000_05F7),
        what: "the examination panel",
    },
    WindowSlot {
        tag: "<VITS>",
        element: ElementId(0x1000_05FA),
        what: "the vitals bars",
    },
    WindowSlot {
        tag: "<SVIT>",
        element: ElementId(0x1000_06D5),
        what: "the side vitals bars",
    },
    WindowSlot {
        tag: "<ENVP>",
        element: ElementId(0x1000_05FD),
        what: "the environment panel",
    },
    WindowSlot {
        tag: "<PANS>",
        element: ElementId(0x1000_05FF),
        what: "the panel container",
    },
    WindowSlot {
        tag: "<TBAR>",
        element: ElementId(0x1000_0603),
        what: "the toolbar",
    },
    WindowSlot {
        tag: "<INDI>",
        element: ElementId(0x1000_0611),
        what: "the indicators strip",
    },
    WindowSlot {
        tag: "<PBAR>",
        element: ElementId(0x1000_0613),
        what: "the power bar",
    },
    WindowSlot {
        tag: "<COMB>",
        element: ElementId(0x1000_06B5),
        what: "the combat panel",
    },
    WindowSlot {
        tag: "<RADA>",
        element: ElementId(0x1000_06D2),
        what: "the radar / compass",
    },
];

/// One window's saved rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SavedWindow {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The parsed file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenLayout {
    /// One entry per [`WINDOWS`] slot, in that order.
    pub windows: Vec<(&'static str, SavedWindow)>,
}

/// The `printf` format the layout writer uses, from the sixteen per-tag format strings the client
/// carries.
///
/// Note the irregular spacing, which has to be kept for existing players' layouts to load:
/// **no space after `X:`**, one space after `Y:`, `W:` and `H:`, and a **trailing space**.
pub const ROW_FORMAT: &str = "%s X:%d Y: %d W: %d H: %d ";

fn format_row(tag: &str, w: &SavedWindow) -> String {
    format!("{} X:{} Y: {} W: {} H: {} ", tag, w.x, w.y, w.w, w.h)
}

impl ScreenLayout {
    /// Render the file: sixteen `ROW_FORMAT` rows concatenated, **with no newline anywhere**.
    ///
    /// The written file is byte-compatible with the client's, since that is the part that cannot
    /// be retrofitted once players have files on disk:
    ///
    /// * The sixteen format strings are **per tag** (`"<SBOX> X:%d Y: %d
    ///   W: %d H: %d "` and fifteen siblings) and **none of them contains a `\n`**.
    /// * The writer `sprintf`s each row into a string and then
    ///   **`fwrite`s** it — not `fprintf`, and nothing is written between rows.
    ///
    /// So retail's `UI-*.txt` is **one line** of sixteen rows separated by the format's own
    /// trailing space, and a file this crate writes is the same bytes. [`Self::parse`] stays
    /// whitespace-insensitive exactly as `fscanf` is, so a newline-separated file still loads.
    ///
    /// Width is written to `W:` and height to `H:`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for (tag, w) in &self.windows {
            s.push_str(&format_row(tag, w));
        }
        s
    }

    /// Read the rows back and move and resize each element accordingly. Tolerant of the row
    /// separator, as `fscanf` is.
    pub fn parse(text: &str) -> Result<Self, PersistError> {
        let mut out = Self::default();
        let mut toks = text.split_whitespace();
        while let Some(tag) = toks.next() {
            let Some(slot) = WINDOWS.iter().find(|s| s.tag == tag) else {
                return Err(PersistError(format!("unknown window tag {tag:?}")));
            };
            let mut vals = [0_i32; 4];
            for (k, prefix) in ["X:", "Y:", "W:", "H:"].into_iter().enumerate() {
                let t = toks
                    .next()
                    .ok_or_else(|| PersistError(format!("{tag}: truncated")))?;
                // `X:12` is one token; `Y: 12` is two. `fscanf` sees both the same way.
                let Some(rest) = t.strip_prefix(prefix) else {
                    return Err(PersistError(format!("{tag}: expected {prefix}, got {t:?}")));
                };
                let v = if rest.is_empty() {
                    toks.next()
                        .ok_or_else(|| PersistError(format!("{tag}: truncated")))?
                } else {
                    rest
                };
                vals[k] = v
                    .parse()
                    .map_err(|_| PersistError(format!("{tag}: bad number {v:?}")))?;
            }
            out.windows.push((
                slot.tag,
                SavedWindow {
                    x: vals[0],
                    y: vals[1],
                    w: vals[2],
                    h: vals[3],
                },
            ));
        }
        Ok(out)
    }

    /// The path the client builds for the automatic per-character file.
    ///
    /// `"%sUI-%s-%s-%d-%d.txt"` = `<dir>UI-<character name>-<second string>-<height>-<width>.txt`.
    /// The second `%s` is presumed to be the world/server name; its producer is unknown, so that
    /// interpretation remains uncertain. **Note the argument order**: height before width, and the
    /// character before the world — not `UI-<world>-<char>-<w>-<h>.txt`.
    #[must_use]
    pub fn auto_path(dir: &str, character: &str, world: &str, height: i32, width: i32) -> String {
        format!("{dir}UI-{character}-{world}-{height}-{width}.txt")
    }

    /// A named layout: `"%s%s.txt"`.
    #[must_use]
    pub fn named_path(dir: &str, name: &str) -> String {
        format!("{dir}{name}.txt")
    }

    /// The built-in default file name.
    #[must_use]
    pub fn default_path(dir: &str) -> String {
        format!("{dir}UI-Default.txt")
    }

    /// The literal that selects the automatic file.
    pub const AUTO_NAME: &'static str = "#auto";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the fixed sixteen-window table's tags, element ids and order.
    #[test]
    fn the_sixteen_windows_are_the_documented_ones_in_the_documented_order() {
        assert_eq!(WINDOWS.len(), 16);
        let tags: Vec<&str> = WINDOWS.iter().map(|w| w.tag).collect();
        assert_eq!(
            tags,
            vec![
                "<SBOX>", "<CHAT>", "<FCH1>", "<FCH2>", "<FCH3>", "<FCH4>", "<EXAM>", "<VITS>",
                "<SVIT>", "<ENVP>", "<PANS>", "<TBAR>", "<INDI>", "<PBAR>", "<COMB>", "<RADA>",
            ]
        );
        assert_eq!(WINDOWS[0].element, ElementId(0x1000_049A));
        assert_eq!(WINDOWS[15].element, ElementId(0x1000_06D2));
        // Every element id is distinct: the table is a key, not a list.
        let mut ids: Vec<u32> = WINDOWS.iter().map(|w| w.element.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 16);
    }

    /// Oracle: the sixteen recovered format strings, transcribed in §2.2 as
    /// `"%s X:%d Y: %d W: %d H: %d "` — "note the irregular spacing".
    #[test]
    fn a_row_reproduces_the_irregular_spacing_exactly() {
        let row = format_row(
            "<TBAR>",
            &SavedWindow {
                x: 10,
                y: 20,
                w: 300,
                h: 40,
            },
        );
        assert_eq!(row, "<TBAR> X:10 Y: 20 W: 300 H: 40 ");
        assert!(!row.contains("X: "), "no space after X:");
        assert!(row.ends_with(' '), "trailing space");
    }

    /// Oracle: / — the file the writer
    /// produces is the file the reader consumes, unchanged.
    #[test]
    fn the_file_round_trips_byte_identically() {
        let l = ScreenLayout {
            windows: WINDOWS
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let i = i32::try_from(i).unwrap();
                    (
                        s.tag,
                        SavedWindow {
                            x: i * 7,
                            y: i * 11,
                            w: 100 + i,
                            h: 50 + i,
                        },
                    )
                })
                .collect(),
        };
        let text = l.to_text();
        let back = ScreenLayout::parse(&text).unwrap();
        assert_eq!(back, l);
        assert_eq!(back.to_text(), text, "byte-identical re-save");
        // **One line, not sixteen.** The sixteen recovered format strings carry no `\n` and
        // the writer `fwrite`s the sprintf result, so retail's file is a single
        // line. Pinned as a literal count rather than described, because a stray newline is
        // exactly the difference that would make a player's existing layout file unreadable by the
        // retail client and would never show up on screen.
        assert_eq!(text.lines().count(), 1, "retail writes no newline at all");
        assert!(!text.contains('\n'), "not one, anywhere");
        assert_eq!(
            text.matches("X:").count(),
            16,
            "and all sixteen rows are still there"
        );
        // The older newline-separated shape still parses, which is what makes the change safe for
        // a file an earlier build of this crate may already have written.
        let with_newlines: String = l
            .windows
            .iter()
            .map(|(t, w)| format!("{}\n", format_row(t, w)))
            .collect();
        assert_eq!(ScreenLayout::parse(&with_newlines).unwrap(), l);
    }

    /// Oracle: the same functions — `fscanf` is whitespace-insensitive, so a file written as one
    /// long line (which the recovered format string, carrying no `\n`, would produce) still loads.
    #[test]
    fn the_reader_is_whitespace_insensitive_like_fscanf() {
        let one_line = "<SBOX> X:1 Y: 2 W: 3 H: 4 <CHAT> X:5 Y: 6 W: 7 H: 8 ";
        let l = ScreenLayout::parse(one_line).unwrap();
        assert_eq!(l.windows.len(), 2);
        assert_eq!(
            l.windows[0].1,
            SavedWindow {
                x: 1,
                y: 2,
                w: 3,
                h: 4
            }
        );
        assert_eq!(
            l.windows[1].1,
            SavedWindow {
                x: 5,
                y: 6,
                w: 7,
                h: 8
            }
        );
    }

    /// Oracle: the client's automatic-path builder and its two literals, `#auto` and
    /// `UI-Default.txt`.
    #[test]
    fn the_path_shapes_match_the_recovered_format_strings() {
        assert_eq!(
            ScreenLayout::auto_path("C:\\ac\\", "Kupo", "Frostfell", 1080, 1920),
            "C:\\ac\\UI-Kupo-Frostfell-1080-1920.txt"
        );
        assert_eq!(
            ScreenLayout::named_path("C:\\ac\\", "mine"),
            "C:\\ac\\mine.txt"
        );
        assert_eq!(
            ScreenLayout::default_path("C:\\ac\\"),
            "C:\\ac\\UI-Default.txt"
        );
        assert_eq!(ScreenLayout::AUTO_NAME, "#auto");
    }

    /// A bad row is an error, not a panic: parsers must not panic on malformed
    /// input, and this file is user-editable.
    #[test]
    fn malformed_input_is_an_error() {
        assert!(ScreenLayout::parse("<NOPE> X:1 Y: 2 W: 3 H: 4 ").is_err());
        assert!(ScreenLayout::parse("<SBOX> X:1 Y: 2").is_err());
        assert!(ScreenLayout::parse("<SBOX> Q:1 Y: 2 W: 3 H: 4 ").is_err());
        assert!(ScreenLayout::parse("<SBOX> X:no Y: 2 W: 3 H: 4 ").is_err());
        assert_eq!(ScreenLayout::parse("").unwrap().windows.len(), 0);
    }
}
