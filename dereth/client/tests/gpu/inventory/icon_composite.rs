//! Object-icon composition, through the device: in a headless `App` in gameplay, the effects ring
//! and the icon overlay change pixels inside the inventory grid and nowhere else. The composition
//! arithmetic, the layer order and the recipes are the dat tier's `inventory::icon_composite`.
//! Fixture: the retail dats and a headless `App` pair run the same number of frames on a software
//! device.

#![cfg(gpu)]

use crate::common::client_dir;

use super::icon_bench::*;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_ui::framework::Screen;
use dereth_ui::region::IconRecipe;
use dereth_ui::{Box2D, ElementId, UiSystem};
use dereth_ui_screens::panels::inventory::ITEM_LIST;
use dereth_ui_screens::screens::gameplay::{window::INVENTORY_PAGE, GamePlayScreen};

// =================================================================================================
// 5. The pixels, through the GPU device
// =================================================================================================

fn app_in_gameplay(frames: u32) -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn app_gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen");
    (ui, screen)
}

fn open_the_backpack(app: &mut App) {
    let (ui, screen) = app_gameplay_screen(app);
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, true);
}

fn element_box(app: &mut App, id: ElementId) -> Box2D {
    let (ui, screen) = app_gameplay_screen(app);
    let root = *Screen::roots(&*screen).first().expect("root");
    let h = ui
        .get_child_recursive(root, id)
        .expect("in the shipped layout");
    ui.screen_clip_box(h)
}

/// Remove effects and custom overlay fields from inventory recipes and count changed slots.
/// The top/container strips and paper doll are stripped in both runs. The grid argument selects
/// whether to strip the item grid too, making it the only differing region between the pair.
/// Base icon, type background and custom underlay remain in both arms.
fn strip(app: &mut App, grid: bool) -> usize {
    let (ui, screen) = app_gameplay_screen(app);
    let p = &mut screen.inventory;
    let mut n = 0usize;
    let mut go = |w: &mut dereth_ui_screens::items::widget::ItemListWidget, ui: &mut UiSystem| {
        for s in &mut w.slots {
            let Some(IconRecipe::Object {
                background,
                icon,
                underlay,
                ..
            }) = s.icon_recipe(ui)
            else {
                continue;
            };
            let bare = IconRecipe::Object {
                background,
                effects: None,
                icon,
                overlay: None,
                underlay,
            };
            if s.set_icon_composite(ui, Some(bare)) {
                n += 1;
            }
        }
    };
    for w in p
        .top_container
        .iter_mut()
        .chain(p.container_list.iter_mut())
    {
        go(w, ui);
    }
    for (_, w) in &mut p.doll {
        go(w, ui);
    }
    if grid {
        for w in p.item_list.iter_mut() {
            go(w, ui);
        }
    }
    n
}

/// Behaviour: inventory.icons.the-effects-ring-replaces-the-icons-white-contour
///
/// Compare equal-frame applications with both effects and overlay fields retained or removed.
/// Successive frames of one advancing application differ everywhere, so the two-arm setup holds
/// frame count and all other recipe inputs fixed. Both keep the tile.
///
/// The declared rectangle is the inventory grid's screen clip box; other lists are stripped in
/// both arms. Count effects/overlay presence before removal and require both populations.
/// The joint removal must change pixels only inside the grid. It does not separately attribute
/// pixels to each layer; independent compositor tests and data joins supply those distinctions.
#[test]
fn the_effects_ring_and_the_icon_overlay_reach_the_pixels_inside_the_grid_and_nowhere_else() {
    // Rank recordings by layered grid cells using the no-GPU bench. Effects recipes include the
    // default contour even when object effects are zero, so this ranking counts recipe presence,
    // not just nonzero object effects.
    let mut best = (0usize, String::new());
    let mut ranked_sessions: Vec<(usize, String)> = Vec::new();
    for session in corpus_sessions() {
        let mut b = Bench::open(&session);
        let g = as_gameplay(&mut b.screen);
        let Some(w) = g.inventory.item_list.as_ref() else {
            continue;
        };
        let n = w
            .slots
            .iter()
            .filter(|s| {
                matches!(
                    s.icon_recipe(&b.ui),
                    Some(IconRecipe::Object {
                        effects: Some(_),
                        ..
                    }) | Some(IconRecipe::Object {
                        overlay: Some(_),
                        ..
                    })
                )
            })
            .count();
        // Measure grid overlay presence separately from all inventory-page cells.
        let (mut ov_grid, mut ov_elsewhere) = (0usize, 0usize);
        for (item, r) in b.cells() {
            if item.is_some() {
                if let Some(IconRecipe::Object {
                    overlay: Some(_), ..
                }) = r
                {
                    ov_elsewhere += 1;
                }
            }
        }
        {
            let g = as_gameplay(&mut b.screen);
            if let Some(w) = g.inventory.item_list.as_ref() {
                for s in &w.slots {
                    if let Some(IconRecipe::Object {
                        overlay: Some(_), ..
                    }) = s.icon_recipe(&b.ui)
                    {
                        ov_grid += 1;
                    }
                }
            }
        }
        println!(
            "{session} puts {n} layered cells in the inventory grid; {ov_grid} of its \
             {ov_elsewhere} overlay-carrying cells are in the grid"
        );
        ranked_sessions.push((n, session.clone()));
        if n > best.0 {
            best = (n, session);
        }
    }
    // Try candidates in descending layered-cell order, breaking ties by recording name.
    // `house-purchase-and-trade`'s grid has no valid screen box in this application bench although
    // it exists in the no-GPU bench, an unresolved client-side discrepancy, so a candidate whose
    // grid does not come up is passed over and named in the report. best.0 retains the largest
    // ranking count even if that candidate was unraisable; it is only a report value.
    let mut ranked: Vec<(usize, String)> = ranked_sessions;
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut skipped: Vec<String> = Vec::new();
    let mut chosen: Option<String> = None;
    for (n, name) in &ranked {
        if *n == 0 {
            continue;
        }
        let mut app = app_in_gameplay(4);
        *app.probe_mut().objects_mut() = replay_to(name, busiest_instant(name)).0;
        let (_, ev) = replay_to(name, busiest_instant(name));
        let _ = app.apply_hud_events(&ev);
        open_the_backpack(&mut app);
        for _ in 0..8 {
            app.frame();
        }
        let ok = element_box(&mut app, ITEM_LIST).is_valid();
        app.shutdown();
        if ok {
            chosen = Some(name.clone());
            break;
        }
        skipped.push(name.clone());
    }
    let session = chosen.unwrap_or_else(|| {
        panic!("no recording with layered cells raises its inventory grid; passed over {skipped:?}")
    });
    println!(
        "the differential runs on {session}; the largest ranked count was {} layered cells",
        best.0
    );
    let session: &str = &session;
    let (_, events) = replay_to(session, busiest_instant(session));

    let shot = |strip_grid: bool| -> (u32, u32, Vec<u8>, Box2D, usize, usize) {
        let mut app = app_in_gameplay(4);
        *app.probe_mut().objects_mut() = replay_to(session, busiest_instant(session)).0;
        let _ = app.apply_hud_events(&events);
        open_the_backpack(&mut app);
        for _ in 0..8 {
            app.frame();
        }
        let area = element_box(&mut app, ITEM_LIST);
        // Count what is there before anything is removed -- this is the denominator.
        let (n_effects, n_overlay) = {
            let (ui, screen) = app_gameplay_screen(&mut app);
            let p = &screen.inventory;
            let mut e = 0usize;
            let mut o = 0usize;
            for w in p.item_list.iter() {
                for s in &w.slots {
                    if let Some(IconRecipe::Object {
                        effects, overlay, ..
                    }) = s.icon_recipe(ui)
                    {
                        if effects.is_some() {
                            e += 1;
                        }
                        if overlay.is_some() {
                            o += 1;
                        }
                    }
                }
            }
            (e, o)
        };
        // Every list **except** the grid is stripped in both runs; the grid only in the second.
        let _ = strip(&mut app, strip_grid);
        app.frame();
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("the frame captures");
        app.shutdown();
        (w, h, bgra, area, n_effects, n_overlay)
    };

    let (w, h, with, area, n_effects, n_overlay) = shot(false);
    let (w2, h2, without, area2, _, _) = shot(true);
    assert_eq!((w, h), (w2, h2));
    assert_eq!(area, area2, "the grid moved between the two runs");
    assert!(
        area.is_valid(),
        "the inventory grid has a screen box: {area:?}"
    );

    // Calibrate the equality direction against the same frame; then require a nonzero
    // differential for this pair and zero changed pixels outside the declared rectangle.
    let count = |a: &[u8], b: &[u8]| -> (u32, u32) {
        let (mut changed, mut outside) = (0u32, 0u32);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if a[i..i + 3] == b[i..i + 3] {
                    continue;
                }
                changed += 1;
                let (px, py) = (i32::try_from(x).unwrap(), i32::try_from(y).unwrap());
                if px < area.x0 || px > area.x1 || py < area.y0 || py > area.y1 {
                    outside += 1;
                }
            }
        }
        (changed, outside)
    };
    let (self_changed, _) = count(&with, &with);
    assert_eq!(
        self_changed, 0,
        "the differ reports a difference between a frame and itself"
    );
    let (changed, outside) = count(&with, &without);
    println!(
        "{n_effects} grid cells carry an effects layer and {n_overlay} an overlay; \
         stripping both changes {changed} pixels, {outside} of them outside the grid {area:?} \
         (differ calibration: frame vs itself = {self_changed})"
    );
    assert!(
        n_effects > 0,
        "no grid cell in {session} carried an effects layer, so this differential is about nothing"
    );
    // The overlay is also established at the composite level and in the corpus join. The pick is
    // data-driven, so the differential must exercise both layers: a pick that moves to a
    // recording whose grid carries no overlay fails here rather than silently testing one layer.
    assert!(
        n_overlay > 0,
        "no grid cell in {session} carried an icon overlay, so this differential exercises only \
         the effects layer"
    );
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the inventory grid"
    );
    assert!(changed > 0, "removing both layers changed nothing at all");
}
