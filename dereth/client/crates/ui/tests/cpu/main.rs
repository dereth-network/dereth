//! CPU-tier tests for `dereth-ui`: no retail dats, no GPU device. Trees are built in the test,
//! most from a `LayoutDesc` written there.
//!
//! * `text`: text elements: selection by drag, keyboard and shift-click, per-run fonts and colours,
//!   the layout pass, the caret, string-table variables and the clipboard copy;
//! * `controls`: the list box, the scrollbar and the button;
//! * `elements`: what every element does: visibility and activation, the cursor override, the media
//!   image.

mod common;
mod controls;
mod elements;
mod text;
