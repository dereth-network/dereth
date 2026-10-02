//! Interface images the early-2005 portal does not have, composed when first asked for from pieces
//! it does have, so the classic interface can show what later worlds added in its own style without
//! reading any later data.
//!
//! Each one has an id of its own in a range no portal uses ([`BASE`] up), and [`crate::art`] answers
//! it like any portal image. A composed paper-doll slot is a bevelled slot frame with a grey
//! silhouette, as the classic slots are: the frame and its colours are taken from the helmet slot,
//! and the silhouette's shape from an item icon of the portal.
use crate::art::Image;

/// The first composed id. The portal's interface images stay well below it.
pub const BASE: u32 = 0x06F0_0000;
/// The cloak slot: a slot frame with a garment's outline.
pub const CLOAK_SLOT: u32 = BASE + 1;
/// The trinket slot: a slot frame with a flask's outline.
pub const TRINKET_SLOT: u32 = BASE + 2;
/// An aetheria sigil slot: a slot frame with a crystal's outline.
pub const SIGIL_SLOT: u32 = BASE + 3;

/// The classic helmet slot, whose bevel and colours every composed slot borrows.
const SLOT_FRAME: u32 = 0x0600_0f68;
/// The portal item icons whose outlines the composed slots show.
const GARMENT_ICON: u32 = 0x0600_32bf;
const FLASK_ICON: u32 = 0x0600_32c8;
const CRYSTAL_ICON: u32 = 0x0600_32d4;

/// Whether `id` is one this module composes.
#[must_use]
pub const fn is_composed(id: u32) -> bool {
    id >= BASE && id < BASE + 0x1_0000
}

/// Compose image `id` from the portal images `portal` returns; `None` for an id this module does
/// not know or a piece the portal lacks.
pub fn compose(id: u32, portal: &dyn Fn(u32) -> Option<Image>) -> Option<Image> {
    let icon = match id {
        CLOAK_SLOT => GARMENT_ICON,
        TRINKET_SLOT => FLASK_ICON,
        SIGIL_SLOT => CRYSTAL_ICON,
        _ => return None,
    };
    Some(slot(&portal(SLOT_FRAME)?, &portal(icon)?))
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * image.width + x) * 4) as usize;
    [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
}

fn luminance([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) * 3 + u32::from(g) * 6 + u32::from(b)) / 10
}

/// The frame's bevel kept, its inside cleared to its own ground colour, and the icon's outline
/// (its pixels that are not the icon's black ground) painted in the frame's silhouette grey, shaded
/// by the icon's own brightness.
fn slot(frame: &Image, icon: &Image) -> Image {
    let (w, h) = (frame.width, frame.height);
    const BEVEL: u32 = 3;
    let ground = pixel(frame, BEVEL + 1, BEVEL + 1);
    // The silhouette grey: the middle of the frame's lit pixels inside its bevel, the tone the
    // classic slots draw their outlines in.
    let mut lit: Vec<[u8; 3]> = (BEVEL..h.saturating_sub(BEVEL))
        .flat_map(|y| (BEVEL..w.saturating_sub(BEVEL)).map(move |x| (x, y)))
        .map(|(x, y)| pixel(frame, x, y))
        .filter(|p| luminance(*p) > 30)
        .collect();
    lit.sort_by_key(|p| luminance(*p));
    let silhouette = lit.get(lit.len() / 2).copied().unwrap_or([68, 68, 68]);
    let mut out = frame.clone();
    let inside = |x: u32, y: u32| x >= BEVEL && y >= BEVEL && x + BEVEL < w && y + BEVEL < h;
    // The outline at three quarters of the frame's inside, centred.
    let span = (w - 2 * BEVEL) * 3 / 4;
    let (ox, oy) = ((w - span) / 2, (h - span) / 2);
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let mut colour = ground;
            if (ox..ox + span).contains(&x) && (oy..oy + span).contains(&y) {
                let ix = (x - ox) * icon.width / span;
                let iy = (y - oy) * icon.height / span;
                let p = pixel(icon, ix.min(icon.width - 1), iy.min(icon.height - 1));
                let lum = luminance(p);
                if lum > 24 {
                    // Shaded from three quarters to a fifth over the silhouette grey by the icon's
                    // brightness, as the classic outlines are lit.
                    let shade = 192 + lum.min(255) / 2;
                    colour =
                        silhouette.map(|c| u8::try_from(u32::from(c) * shade / 255).unwrap_or(c));
                }
            }
            let i = ((y * w + x) * 4) as usize;
            out.rgba[i..i + 3].copy_from_slice(&colour);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (art composed for the classic interface from its own portal's pieces).
    use super::*;

    fn flat(w: u32, h: u32, rgb: [u8; 3]) -> Image {
        Image {
            width: w,
            height: h,
            rgba: (0..w * h)
                .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
                .collect(),
        }
    }

    #[test]
    fn a_composed_slot_keeps_the_frame_and_draws_the_icon_outline_in_the_silhouette_grey() {
        let mut frame = flat(32, 32, [60, 60, 60]);
        // A bevel pixel and a bright silhouette pixel inside.
        frame.rgba[0..3].copy_from_slice(&[200, 0, 0]);
        let at = ((10 * 32 + 10) * 4) as usize;
        frame.rgba[at..at + 3].copy_from_slice(&[150, 150, 150]);
        // An icon: black ground, a white block in its middle.
        let mut icon = flat(32, 32, [0, 0, 0]);
        for y in 12..20 {
            for x in 12..20 {
                let i = ((y * 32 + x) * 4) as usize;
                icon.rgba[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let out = slot(&frame, &icon);
        assert_eq!(pixel(&out, 0, 0), [200, 0, 0], "the bevel is the frame's");
        assert_eq!(
            pixel(&out, 5, 5),
            [60, 60, 60],
            "the inside is the frame's ground"
        );
        let centre = pixel(&out, 16, 16);
        assert!(
            centre[0] > 60,
            "the outline is drawn lighter than the ground, in the silhouette grey: {centre:?}"
        );
        assert!(is_composed(SIGIL_SLOT) && !is_composed(SLOT_FRAME));
        assert!(compose(BASE + 99, &|_| None).is_none());
    }
}
