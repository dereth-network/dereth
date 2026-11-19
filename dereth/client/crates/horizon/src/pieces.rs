//! The interface's own art, which names no game and so lives in its own crate
//! ([`dereth_horizon_art`]), shared with the second game's Horizon.

pub use dereth_horizon_art::*;

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    /// The art behind the interface is the files the host handed over: drawn from them when
    /// they are there, and plainly where they are not.
    #[test]
    fn the_art_draws_from_the_files_the_host_hands_over() {
        let built_in = Pieces::built_in().unwrap();
        let art = crate::art::Art::new(std::sync::Arc::new(built_in.clone()));
        assert!(art.has_piece("window.tl"));
        let sprite = art.piece("window.tl", 1.0).expect("the piece");
        assert!(art.image(sprite.tex).is_some(), "its atlas decoded");
        assert!(art.font(crate::art::Family::Body, 13.0).is_some());
        // Only the manifest: the piece is named, and its atlas is not there to draw it from.
        let mut manifest_only = Pieces::new();
        manifest_only.insert(MANIFEST, built_in.get(MANIFEST).unwrap().to_vec());
        let art = crate::art::Art::new(std::sync::Arc::new(manifest_only));
        assert!(art.has_piece("window.tl"));
        assert_eq!(art.piece("window.tl", 1.0), None);
        assert!(art.font(crate::art::Family::Body, 13.0).is_none());
        let empty = crate::art::Art::new(std::sync::Arc::new(Pieces::new()));
        assert!(!empty.has_piece("window.tl"));
    }

    #[test]
    fn the_built_in_fonts_cover_every_family_the_interface_draws_and_their_glyphs_fit_their_pages()
    {
        let pieces = Pieces::built_in().unwrap();
        let table = FontTable::parse(pieces.get(FONTS).unwrap()).unwrap();
        let mut pages = Vec::new();
        for page in 0..table.pages {
            let path = font_page_path(u16::try_from(page).unwrap());
            pages.push(decode_png(pieces.get(&path).unwrap()).unwrap());
        }
        for family in crate::art::Family::ALL {
            let fam = &table.families[family.file_stem()];
            assert!(!fam.sizes.is_empty(), "{family:?}");
            for size in &fam.sizes {
                let font = size.font(fam.factor);
                for ch in ['A', 'g', '0', '?'] {
                    assert!(font.glyph(ch).is_some(), "{family:?} {}: {ch}", size.size);
                }
                for g in font.glyphs.values() {
                    let page = &pages[usize::from(g.page)];
                    assert!(u32::from(g.x) + u32::from(g.w) <= page.width);
                    assert!(u32::from(g.y) + u32::from(g.h) <= page.height);
                }
            }
        }
        // Text asked for at 8.5 is drawn from the size nearest 8.5 times the factor, and that
        // size's table answers as 8.5 asks: drawn unscaled.
        let (size, factor) = table.nearest("Body", 8.53).unwrap();
        assert!((size.size - (8.53 * factor).round()).abs() < f32::EPSILON);
        let font = size.font(factor);
        assert!((font.point_size * factor - size.size).abs() < 1e-3);
    }
}
