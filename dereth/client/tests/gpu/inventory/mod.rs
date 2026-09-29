//! Inventory: the backpack and its grid, item icons, the paper doll and burden meter, loot
//! windows, targeted use, and dropping or giving items into the world.

mod backpack_grid_tiles;
mod backpack_panel_paper_doll;
mod burden_meter_pixels;
mod corpse_loot_window;
/// The icon bench is the dat tier's file, declared here by path rather than copied.
#[path = "../../dat/inventory/icon_bench.rs"]
mod icon_bench;
mod icon_composite;
mod inventory_request_lock;
mod targeted_use;
mod world_drop_draw;
mod world_drop_give;
