//! The Horizon interface's art, which names no game: its pieces' manifest, their atlases at three
//! scales and its fonts, built into a desktop client and fetched by a browser's page.
//!
//! The pieces are packed into atlases at each scale, and the manifest names each piece, gives its
//! size in layout units and says where it sits in each scale's atlas. Where a piece is in the
//! manifest the interface draws it; where it is not, the interface draws plainly in its place.
//!
//! The art is a set of files ([`FILES`]): the manifest and the atlases, the interface's fonts
//! as glyph pages with their table ([`FontTable`]), and the two pictures a journey's way ahead is
//! drawn with ([`TRAIL`], [`MOTE`]), which stand alone because the wisp repeats its own along it. The host hands them to the interface as
//! [`Pieces`]. A desktop client carries them built in ([`Pieces::built_in`]); the browser's module
//! does not, and its page fetches them from beside the module instead.
//!
//! **Depends on** `dereth-ui-kit`, whose skins read their pieces through its file list. **Used
//! by** the Horizon interface (`dereth-horizon`) and the browser's module (`dereth-web`), which
//! hands over the files its page fetched.
//!
//! **Must never** name either game: each game's Horizon draws from the same art.

use std::borrow::Cow;
use std::collections::BTreeMap;

pub use dereth_ui_kit::pieces::{
    atlas_path, decode_png, font_page_path, FontFamily, FontSize, FontTable, Manifest, Piece,
    FONTS, MANIFEST,
};

/// The art's files, as one list: [`FILES`], and in a build that carries them, their bytes.
macro_rules! files {
    ($($name:literal),* $(,)?) => {
        /// Every file of the art, by its name: the manifest, the atlases, the fonts' table and
        /// the glyph pages. A host hands over each of them, and nothing else.
        pub const FILES: &[&str] = &[$($name),*];

        /// The files built in, by name.
        #[cfg(not(target_arch = "wasm32"))]
        const BUILT_IN: &[(&str, &[u8])] =
            &[$(($name, include_bytes!(concat!("../pieces/", $name)))),*];
    };
}

files!(
    "manifest.json",
    "atlas-1-0.png",
    "atlas-2-0.png",
    "atlas-3-0.png",
    "fonts.json",
    "font-0.png",
    "font-1.png",
    "font-2.png",
    "font-3.png",
    "font-4.png",
    "font-5.png",
    "trail-0.png",
    "mote-0.png",
);

/// The picture a journey's wisp is drawn with: a strip that repeats along it end to end, light in
/// its own colours with its brightness its alpha. Not in the atlases: it repeats.
pub const TRAIL: &str = "trail-0.png";

/// The picture a mote drifting off the wisp is drawn with, as [`TRAIL`] is.
pub const MOTE: &str = "mote-0.png";

/// The interface's art as a host hands it over: each of [`FILES`] by its name.
#[derive(Clone, Default)]
pub struct Pieces {
    files: BTreeMap<String, Cow<'static, [u8]>>,
}

impl std::fmt::Debug for Pieces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.files.iter().map(|(name, bytes)| (name, bytes.len())))
            .finish()
    }
}

impl Pieces {
    /// No files yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The files the client carries built in; `None` in a build that carries none (the
    /// browser's module, whose page hands the files over).
    #[must_use]
    pub fn built_in() -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut pieces = Self::new();
            for (name, bytes) in BUILT_IN {
                pieces.insert(name, *bytes);
            }
            Some(pieces)
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
    }

    /// File `name`'s bytes, in place of any it had.
    pub fn insert(&mut self, name: &str, bytes: impl Into<Cow<'static, [u8]>>) {
        self.files.insert(name.to_ascii_lowercase(), bytes.into());
    }

    /// File `name`, its case aside.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files
            .get(&name.to_ascii_lowercase())
            .map(std::convert::AsRef::as_ref)
    }

    /// Whether every file of the art is here and looks like what it should be: each picture a
    /// PNG and each table JSON text, so a page a server sends in a missing file's place is not
    /// taken for it.
    ///
    /// # Errors
    /// Each file that is missing, not one of [`FILES`], or not what its name says, named.
    pub fn check(&self) -> Result<(), String> {
        const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut findings = Vec::new();
        for name in FILES {
            match self.get(name) {
                None => findings.push(format!("{name} is missing")),
                Some(bytes) if name.ends_with(".png") && !bytes.starts_with(PNG) => {
                    findings.push(format!("{name} is not a PNG picture"));
                }
                Some(bytes)
                    if name.ends_with(".json")
                        && bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') =>
                {
                    findings.push(format!("{name} is not a JSON table"));
                }
                Some(_) => {}
            }
        }
        for name in self.files.keys() {
            if !FILES.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                findings.push(format!("{name} is not one of the art's files"));
            }
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(findings.join("; "))
        }
    }
}

impl dereth_ui_kit::art::ArtFiles for Pieces {
    fn get(&self, name: &str) -> Option<&[u8]> {
        Pieces::get(self, name)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn the_built_in_pieces_hold_every_atlas_their_manifest_names_and_each_decodes() {
        let pieces = Pieces::built_in().expect("a desktop build carries its pieces");
        let manifest = Manifest::parse(pieces.get(MANIFEST).unwrap()).unwrap();
        assert!(!manifest.pieces.is_empty());
        for (name, piece) in &manifest.pieces {
            for (scale, at) in &piece.at {
                let path = atlas_path(scale.parse().unwrap(), at[0]);
                assert!(pieces.get(&path).is_some(), "{name}: {path} is built in");
            }
        }
        for path in FILES.iter().filter(|p| p.starts_with("atlas-")) {
            let image =
                decode_png(pieces.get(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
            for piece in manifest.pieces.values() {
                for (scale, at) in &piece.at {
                    if atlas_path(scale.parse().unwrap(), at[0]) == *path {
                        assert!(at[1] + at[3] <= image.width && at[2] + at[4] <= image.height);
                    }
                }
            }
        }
    }

    /// The art's files are exactly the files of its folder, each built in and each passing the
    /// check a host's files pass: what a host is told to hand over is what there is.
    #[test]
    fn the_art_s_files_are_its_folder_s_files_and_the_built_in_ones_pass_the_check() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("pieces");
        let mut on_disk: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut listed: Vec<String> = FILES.iter().map(|f| (*f).to_owned()).collect();
        listed.sort();
        assert_eq!(listed, on_disk, "FILES lists {}", folder.display());
        let pieces = Pieces::built_in().unwrap();
        assert_eq!(pieces.check(), Ok(()));
        for name in FILES {
            assert_eq!(
                pieces.get(name).map(<[u8]>::len),
                Some(std::fs::read(folder.join(name)).unwrap().len()),
                "{name}"
            );
        }
    }

    /// Files a host hands over are the art once all of them are there and each is what its name
    /// says; a missing one, or a page sent in a file's place, is named.
    #[test]
    fn handed_over_files_are_checked_for_each_one_missing_or_not_what_its_name_says() {
        let mut pieces = Pieces::new();
        let check = pieces.check().unwrap_err();
        for name in FILES {
            assert!(check.contains(&format!("{name} is missing")), "{check}");
        }
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0];
        for name in FILES {
            if name.ends_with(".png") {
                pieces.insert(name, png.to_vec());
            } else {
                pieces.insert(name, b" {}".to_vec());
            }
        }
        assert_eq!(pieces.check(), Ok(()));
        assert_eq!(
            pieces.get("ATLAS-1-0.PNG"),
            Some(&png[..]),
            "its case aside"
        );
        pieces.insert("atlas-2-0.png", b"<!doctype html>".to_vec());
        pieces.insert(FONTS, b"<html>".to_vec());
        pieces.insert("notes.txt", b"x".to_vec());
        let check = pieces.check().unwrap_err();
        assert!(check.contains("atlas-2-0.png is not a PNG"), "{check}");
        assert!(check.contains("fonts.json is not a JSON"), "{check}");
        assert!(check.contains("notes.txt is not one of"), "{check}");
        assert!(!check.contains("missing"), "{check}");
    }
}
