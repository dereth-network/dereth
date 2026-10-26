//! Drawing the interface's pieces: the kit's painter ([`dereth_ui_kit::paint`]).

pub use dereth_ui_kit::paint::{Align, Painter, TextStyle};

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own text drawing)
    use super::*;
    use crate::art::{Art, Family};
    use crate::draw::{DrawList, Rect};

    /// Text placed in a box sits with its letters in the box's middle, not its whole line, whose
    /// room for descenders would leave the letters low: the box's middle falls between the
    /// middle of the capitals and the middle of the small letters.
    #[test]
    fn text_in_a_box_centres_its_letters() {
        let art = Art::new(std::sync::Arc::new(
            crate::pieces::Pieces::built_in().expect("the pieces built in"),
        ));
        let mut list = DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF);
        let row = Rect::new(0.0, 100.0, 200.0, 30.0);
        let middle = row.y + row.h / 2.0;
        p.text_in(&style, row, Align::Left, "H");
        let cap = list.quads.last().expect("the capital was drawn").dst;
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        p.text_in(&style, row, Align::Left, "x");
        let small = list.quads.last().expect("the small letter was drawn").dst;
        let (cap_middle, small_middle) = (cap.y + cap.h / 2.0, small.y + small.h / 2.0);
        assert!(
            cap_middle <= middle + 0.6 && small_middle >= middle - 0.6,
            "the capital's middle is at {cap_middle}, the small letter's at {small_middle}, the              row's at {middle}"
        );
        assert!(
            small_middle - middle < 2.0,
            "the small letters sit low: {small_middle} against {middle}"
        );
    }

    /// At 100%, 125% and 150%, and shrunk to fit a short window, text is drawn from the font's
    /// own glyphs unscaled, each texel on one pixel: every glyph as many pixels as it has texels,
    /// on whole pixels, wherever the line was asked to start. (The font holds every whole size
    /// these ask for.)
    #[test]
    fn text_is_drawn_from_its_own_glyphs_unscaled_on_whole_pixels() {
        let art = Art::new(std::sync::Arc::new(
            crate::pieces::Pieces::built_in().expect("the pieces built in"),
        ));
        let styles = [
            TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF).edge(0xFF00_0000),
            TextStyle::new(Family::Heading, crate::ui::kit::TITLE_POINTS, 0xFFFF_FFFF),
            TextStyle::new(Family::BarNumbers, 20.0, 0xFFFF_FFFF),
            TextStyle::new(Family::Numerals, 12.0, 0xFFFF_FFFF).tracking(4.0),
        ];
        for scale in [1.0, 1017.0 / 1080.0, 1.25, 1.5] {
            for style in &styles {
                let mut list = DrawList::default();
                let mut p = Painter {
                    list: &mut list,
                    art: &art,
                    scale,
                    screen: (1920.0, 1080.0),
                    fade: 1.0,
                };
                let w = p.text(style, 10.3, 20.6, "Aurelia Vale, 1,250");
                assert!(
                    (w - p.measure(style, "Aurelia Vale, 1,250")).abs() < 1e-3,
                    "{style:?} at {scale}: drawn as wide as measured"
                );
                assert!(list.quads.len() > 10);
                for q in &list.quads {
                    assert!(
                        (q.dst.w - q.src.w).abs() < 1e-3 && (q.dst.h - q.src.h).abs() < 1e-3,
                        "{style:?} at {scale}: a glyph drawn {:?} from {:?}",
                        q.dst,
                        q.src
                    );
                    assert!(
                        q.dst.x.fract() == 0.0 && q.dst.y.fract() == 0.0,
                        "{style:?} at {scale}: a glyph at ({}, {})",
                        q.dst.x,
                        q.dst.y
                    );
                }
            }
        }
    }
}
