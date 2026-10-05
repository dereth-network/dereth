//! `PlayerModule`'s window-placement blob.
//!
//! Window-placement rows read by floating windows from the retained `PlayerModule`. They live here
//! rather than in `dereth_ui_screens::hud::floaty` because `dereth_client_runtime::hud` *owns* the blob --
//! it is one of the three process globals the client owns -- and reading it should not make the
//! world half name the crate that draws the windows. Plain data: six `Option`s and a `BTreeMap`,
//! and [`ChatWindowTitle`](crate::view::ChatWindowTitle) is already this crate's.
//!
//! The chrome pieces, the window catalogue and everything else in `floaty.rs` stay in the UI: they
//! are element ids and layout, which is presentation.
//! `dereth_ui_screens::hud::floaty::WindowPlacement`
//! and `::WindowPlacements` resolve through a `pub use`.

/// One row of the server-side window blob: `Option_Placement`, property `0x1000008B`.
///
/// The chat-option structure includes position fields `0x10000086` and `0x10000087`.
/// The player module's chat-option accessor establishes the enclosing blob's shape:
/// Gameplay options hold a single **array**
/// property `0x1000008C` (`Option_PlacementArray`), indexed by `windowID − 1`, whose elements are
/// **structs** named `0x1000008B` (`Option_Placement`). A window-option query is
/// "index that array, then read `name` out of the struct", and the six names it is ever asked for
/// are these:
///
/// | property | name | read by |
/// |---|---|---|
/// | `0x10000086` / `0x10000087` | `Option_Placement_X` / `_Y` | every floating-window placement update, **only when layout is not taken from the file** |
/// | `0x10000088` / `0x10000089` | `Option_Placement_Width` / `_Height` | idem |
/// | `0x1000008A` | `Option_Placement_Visibility` | the five classes in `READS_PLACEMENT_VISIBILITY`, **unconditionally** |
/// | `0x1000008D` | `Option_Placement_Title` | floating chat windows only |
///
/// A field the server did not send is `None`, and a failed window-option lookup is what
/// leaves the layout's own value standing — which is why every field is an `Option` rather than a
/// value with a default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowPlacement {
    /// `0x10000086` `Option_Placement_X`.
    pub x: Option<i32>,
    /// `0x10000087` `Option_Placement_Y`.
    pub y: Option<i32>,
    /// `0x10000088` `Option_Placement_Width`.
    pub w: Option<i32>,
    /// `0x10000089` `Option_Placement_Height`.
    pub h: Option<i32>,
    /// `0x1000008A` `Option_Placement_Visibility`.
    pub visible: Option<bool>,
    /// `0x1000008D` `Option_Placement_Title`. Literal overrides and table references stay
    /// distinct, as they do in the PlayerModule's `StringInfo`.
    pub title: Option<crate::view::ChatWindowTitle>,
}

/// The gameplay option placement array, keyed by **window id**.
///
/// The wire form is a dense array indexed by `windowID − 1`; this is that array with the −1 undone,
/// because every consumer has a window id in hand and never an index.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowPlacements {
    pub rows: std::collections::BTreeMap<u32, WindowPlacement>,
}

impl WindowPlacements {
    /// The stored chat-window options for one window id.
    #[must_use]
    pub fn get(&self, window_id: u32) -> Option<&WindowPlacement> {
        self.rows.get(&window_id)
    }

    /// Store one row, by window id.
    pub fn set(&mut self, window_id: u32, p: WindowPlacement) {
        self.rows.insert(window_id, p);
    }
}
