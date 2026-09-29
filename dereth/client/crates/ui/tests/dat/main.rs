//! DAT-tier tests for `dereth-ui`: each module reads the retail dats under `$DERETH_TEST_DAT_DIR`
//! (the shipped layouts, string tables and master property). Selected by `--features retail-dats`;
//! absent dats are a failure, never a skip.

mod border_resize;
mod common;
mod dialog_answers;
mod dialog_queue_and_menus;
mod drag_walk;
mod journal_page_list;
mod layout_conformance;
mod list_row_highlight;
mod listbox_click_select;
mod mouse_wheel;
mod scrollbar_hold_repeat;
