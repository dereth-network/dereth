//! Character-sheet overflow and thumb dragging in the shipped layout.
//! Fixture: shared widget tree and a burdened character view.

use crate::common::widget_fixture::*;

/// **The instrument's calibration**: the sheet really does overflow the box it is drawn in, so
/// there is something for a bar to scroll. Without this an absent bar and a bar with nothing to
/// do are the same reading.
///
/// It is measured off the **composed glyphs** rather than off `scroll.height`, because
/// `scroll.height` is the quantity the defect leaves at zero — a calibration that read it would
/// fail for the reason it is calibrating against.
#[test]
fn the_burdened_character_sheet_overflows_the_pane_it_is_drawn_in() {
    let view = Burdened;
    let (mut ui, mut s, mut panels) = screen(&view);
    let pane = open_character_info(&mut ui, &mut s, &mut panels, &view);
    let box_ = ui.screen_box(pane);
    let low = ui
        .text_element_mut(pane)
        .expect("a text element")
        .compose(box_)
        .into_iter()
        .map(|g| g.y)
        .max()
        .expect("the sheet has glyphs on it");
    assert!(
        low >= box_.y1,
        "the six sections fit in the pane (last glyph at y {low}, box {box_:?}), so this file is \
         pointed at nothing -- a scrollbar would be right to stay hidden"
    );
}

/// *"Burden LAMP has no scrollbar."* `0x1000011E` ships `0x76 disabled = true` and `0x79
/// hide-when-disabled = true`, so it is on screen only when
/// has been told the content is bigger than the view.
#[test]
fn a_burdened_character_sheet_puts_the_scrollbar_on_the_screen() {
    let view = Burdened;
    let (mut ui, mut s, mut panels) = screen(&view);
    let pane = open_character_info(&mut ui, &mut s, &mut panels, &view);
    // The two numbers compares. Before the fix the first was
    // **0** however many lines the sheet held, which is the whole defect.
    let (content, box_h) = info_extent(&mut ui, pane);
    assert!(
        content > box_h,
        "the scrollable height is {content} against a {box_h} px view -- the pane was never \
         re-measured after it was filled"
    );
    assert!(
        visible(&ui, find(&ui, &s, INFO_SCROLLBAR)),
        "the sheet overflows, so scrollbar sizing must clear 0x76 and hide-when-disabled must \
         let the bar draw"
    );
}

/// Behaviour: character-sheet.scroll.a-burdened-sheet-overflows-and-its-bar-scrolls-the-bottom-into-view
/// **The gesture.** Press the thumb, drag it to the bottom of its track, release — and the bottom
/// of the sheet comes into view.
#[test]
fn a_real_drag_of_the_thumb_brings_the_bottom_of_the_character_sheet_into_view() {
    let view = Burdened;
    let (mut ui, mut s, mut panels) = screen(&view);
    let pane = open_character_info(&mut ui, &mut s, &mut panels, &view);

    // The final section composed by the panel update, and therefore the sheet's last line —
    // taken off the panel rather than spelled out here, so the assertion follows the string table.
    let marker = panels
        .character_info
        .sections
        .last()
        .expect("six sections")
        .trim()
        .lines()
        .next_back()
        .expect("the augmentation line")
        .trim()
        .to_owned();
    let before = readable(&mut ui, pane);
    assert!(
        !before.contains(&marker),
        "the instrument is pointed at nothing: {marker} is already readable before any \
         scroll:\n{before}"
    );

    let bar = find(&ui, &s, INFO_SCROLLBAR);
    let thumb = ui
        .get_child(bar, THUMB)
        .expect("the layout update takes child 1 as the thumb");
    let from = centre(&ui, thumb);
    let track = ui.screen_box(bar);
    drag(
        &mut ui,
        &mut s,
        &mut panels,
        &view,
        from,
        (from.0, track.y1 - 2),
    );

    assert!(
        info_offset(&mut ui, pane) > 0,
        "a drag of the thumb moved the scroll offset not at all"
    );
    let after = readable(&mut ui, pane);
    assert!(
        after.contains(&marker),
        "the bottom of the sheet is still below the fold after the thumb was dragged to the \
         bottom of its track:\n{after}"
    );
}
