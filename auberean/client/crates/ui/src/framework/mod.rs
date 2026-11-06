//! Frameworks: what the game's panels are built on, and the flow from one screen to the next.
//!
//! - A **main framework** ([`Ui::new_main_frame`]) is one screen's top: it makes the screen's
//!   top-level elements from layouts ([`Ui::frame_create_root`]), keeps them, and keeps where the
//!   player put those whose place is kept ([`Ui::frame_inq_locations`],
//!   [`Ui::frame_apply_locations`]).
//! - A **child framework** ([`Ui::new_child_frame`]) is a panel inside a screen (the toolbar's
//!   windows, the status bar, a dialog): it makes its elements through its parent, and is shown
//!   only while it and every framework above it are ([`Ui::frame_shown`]). Showing a framework
//!   ([`Ui::frame_show`]) tells each child to show again by its own flag ([`Mail::Show`]).
//! - Every framework is a listener ([`Ui::frame_listener`]): it registers for its elements'
//!   messages and the manager's global messages and takes what it heard as [`Mail`].
//! - The **flow** ([`Flow`]) knows each screen by a mode token and the function that makes it;
//!   a screen asked for ([`Flow::queue_mode`]) replaces the current one at the next tick.
//! - **Persistent data** ([`Flow::data`]) lives across screens, of the type the game gives it.
//!
//! A panel written on these: hold the [`FrameId`] of its framework; make its roots with
//! [`Ui::frame_create_root`] and find its parts by name in them; register with
//! [`Ui::listen_for_message`] / [`Ui::listen_for_global_message`] on its listener; each frame,
//! take its mail and act on it; on [`Mail::Show`], call [`Ui::frame_show`] and show or hide its
//! roots by [`Ui::frame_shown`]; when it goes, [`Ui::drop_frame`].
//!
//! [`Mail`]: crate::listener::Mail
//! [`Ui::new_main_frame`]: crate::Ui::new_main_frame
//! [`Ui::frame_create_root`]: crate::Ui::frame_create_root
//! [`Ui::frame_inq_locations`]: crate::Ui::frame_inq_locations
//! [`Ui::frame_apply_locations`]: crate::Ui::frame_apply_locations
//! [`Ui::new_child_frame`]: crate::Ui::new_child_frame
//! [`Ui::frame_shown`]: crate::Ui::frame_shown
//! [`Ui::frame_show`]: crate::Ui::frame_show
//! [`Ui::frame_listener`]: crate::Ui::frame_listener
//! [`Ui::listen_for_message`]: crate::Ui::listen_for_message
//! [`Ui::listen_for_global_message`]: crate::Ui::listen_for_global_message
//! [`Ui::drop_frame`]: crate::Ui::drop_frame
//! [`Mail::Show`]: crate::listener::Mail::Show

pub mod flow;
pub mod frame;

pub use crate::location::Location;
pub use flow::Flow;
pub use frame::{FrameId, Locations};
