//! The carried fonts: every request the classic interface makes is drawn, and where the Windows
//! font system exists they are measured against it.
//!
//! Behaviour: none (the classic interface's text drawn from carried fonts; the reference is this
//! machine's Windows font system, not the game).

mod measure;

use dereth_classic_dat::fonts::{FontSpec, REQUESTS};
use dereth_classic_fonts::rasterize;

fn spec(height: i32, width: i32, weight: i32, face: &str) -> FontSpec {
    FontSpec {
        height,
        width,
        weight,
        italic: false,
        face: face.into(),
    }
}

#[test]
fn every_request_of_the_classic_interface_draws_every_printable_western_character() {
    for (name, height, width, weight, face) in REQUESTS {
        let atlas = rasterize(&spec(height, width, weight, face)).unwrap();
        assert_eq!(atlas.glyphs.len(), 218, "{name}");
        assert!(
            atlas.line_height >= height - 1 && atlas.line_height <= height,
            "{name}"
        );
        assert!(
            atlas.baseline > 0 && atlas.baseline < atlas.line_height,
            "{name}"
        );
        for c in ('!'..='~').chain(['\u{e9}', '\u{20ac}', '\u{2019}']) {
            let g = atlas.glyphs[&u32::from(c)];
            assert!(g.width > 0 && g.height > 0 && g.advance > 0, "{name} {c:?}");
        }
        assert_eq!(atlas.glyphs[&32].width, 0, "{name}: the space has no ink");
    }
}

/// White pixels with coverage in alpha, at most the 65 levels an 8 by 8 sample grid gives.
#[test]
fn the_atlas_is_white_with_coverage_in_sixty_five_levels() {
    let atlas = rasterize(&spec(15, 6, 500, "Times New Roman")).unwrap();
    let mut levels = std::collections::BTreeSet::new();
    for px in atlas.rgba.as_chunks::<4>().0 {
        assert_eq!(&px[..3], &[255, 255, 255]);
        levels.insert(px[3]);
    }
    assert!(
        levels.len() <= 65 && levels.len() > 16,
        "{} levels",
        levels.len()
    );
    assert!(levels.iter().all(|a| *a == 0 || *a == 255 || a % 4 == 3));
}

#[test]
fn a_request_far_beyond_the_interface_s_sizes_is_refused_not_clipped() {
    assert!(rasterize(&spec(200, 0, 400, "Times New Roman")).is_err());
}

#[cfg(windows)]
mod windows {
    use super::measure::{compare, Tally};
    use super::{spec, REQUESTS};
    use dereth_classic_fonts::{rasterize, rasterize_face, Face};

    /// The advances, line heights and baselines are Windows' own for every request; the glyphs'
    /// pixels differ by the faces' outlines and hinting, within what the carried faces reach
    /// (50.5% of Windows' ink overall when measured, the worst request 68%).
    #[test]
    fn the_carried_fonts_lay_text_out_as_the_windows_font_system_does() {
        let mut all = Tally::default();
        for (name, height, width, weight, face) in REQUESTS {
            let s = spec(height, width, weight, face);
            let windows = dereth_classic_gdi::fonts::rasterize(&s).unwrap();
            let ours = rasterize(&s).unwrap();
            assert_eq!(ours.line_height, windows.line_height, "{name}");
            assert_eq!(ours.baseline, windows.baseline, "{name}");
            let t = compare(&windows, &ours);
            assert_eq!((t.glyphs, t.exact), (218, 218), "{name}: {t:?}");
            assert!(t.ink_error() < 72.0, "{name}: {:.1}%", t.ink_error());
            all.add(&t);
        }
        assert!(all.ink_error() < 53.0, "{:.1}%", all.ink_error());
    }

    /// Windows' own faces drawn by the carried fonts' path come out as Windows draws them: the
    /// request reading, hinting and sampling are Windows', and what differs in the carried
    /// fonts' pixels is their faces. A request Windows stretches sideways is hinted here at two
    /// square sizes, which is close but not exact.
    #[test]
    fn windows_own_faces_through_the_carried_path_match_the_windows_font_system() {
        let mut all = Tally::default();
        for (name, height, width, weight, face) in REQUESTS {
            let s = spec(height, width, weight, face);
            let file = match face {
                "Courier New" => "courbd.ttf",
                "Arial" => "arialbd.ttf",
                "Times New Roman Italic" => "timesi.ttf",
                _ if weight >= 600 => "timesbd.ttf",
                _ => "times.ttf",
            };
            let Ok(data) = std::fs::read(format!("C:/Windows/Fonts/{file}")) else {
                eprintln!("no {file}: skipped");
                return;
            };
            let windows = dereth_classic_gdi::fonts::rasterize(&s).unwrap();
            let ours = rasterize_face(
                &Face {
                    data: &data,
                    ..Face::for_request(&s)
                },
                &s,
            )
            .unwrap();
            let t = compare(&windows, &ours);
            assert!(t.ink_error() < 30.0, "{name}: {:.1}%", t.ink_error());
            all.add(&t);
        }
        assert!(all.ink_error() < 4.0, "{:.1}%", all.ink_error());
        assert!(all.identical * 4 > all.glyphs, "{all:?}");
    }
}
