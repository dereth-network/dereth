//! World-backed creation controls through the shell's real tables and interface tree.

use super::*;
use dereth_assets::{
    material::{Palette, PaletteSet},
    Decode,
};
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::framework::{Screen, ScreenCx};
use dereth_ui_screens::screens::chargen::{self, CharGenScreen, EParts, EcgProgress};

fn fixture(old: bool) -> (UiShell, CharGenScreen, Arc<dereth_dat::RetailDatStore>) {
    let store = Arc::new(if old {
        dereth_dat::RetailDatStore::open_pre_tod_with_later(
            &dereth_dat::testing::pre_tod_dat_dir().expect("earlier world directory"),
            &dereth_dat::testing::dat_dir(),
        )
        .expect("earlier world with later interface")
    } else {
        dereth_dat::testing::open_store_or_fail()
    });
    let mut shell = UiShell::new(&store, (800, 600)).expect("shell");
    let mut screen = CharGenScreen::default();
    screen.create(&mut ScreenCx::new(&mut shell.ui)).unwrap();
    screen.set_tables(shell.chargen_tables.clone().expect("creation tables"));
    screen.update(&mut ScreenCx::new(&mut shell.ui), LocalTime(0.0));
    screen.choose_heritage(1);
    screen.choose_gender(&mut shell.ui, 2);
    (shell, screen, store)
}

fn palette(store: &dereth_dat::RetailDatStore, id: DataId, index: usize) -> u32 {
    Palette::decode_payload(id, &store.read_portal(id).unwrap())
        .unwrap()
        .colors_argb[index]
}

fn averaged(store: &dereth_dat::RetailDatStore, id: DataId, index: usize) -> u32 {
    let set = PaletteSet::decode_payload(id, &store.read_portal(id).unwrap()).unwrap();
    let count = u32::try_from(set.palette_ids.len()).unwrap();
    let rgb = set.palette_ids.iter().fold([0; 3], |mut rgb, &id| {
        let color = palette(store, id, index);
        for (channel, shift) in rgb.iter_mut().zip([16, 8, 0]) {
            *channel += (color >> shift) & 255;
        }
        rgb
    });
    0xff00_0000 | ((rgb[0] / count) << 16) | ((rgb[1] / count) << 8) | (rgb[2] / count)
}

/// Behaviour: chargen.palette.samples-follow-entry-layout
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn modern_creation_wheels_sample_the_active_world_for_every_colored_part() {
    for old in [true, false] {
        let (mut shell, mut screen, store) = fixture(old);
        screen.set_progress_state(&mut shell.ui, EcgProgress::Appearance);
        let tables = screen.tables.clone().unwrap();
        let sex = &tables.chargen.heritage_groups[&1].sexes[&2];
        for (part, ids, index, direct) in [
            (
                EParts::Hair,
                sex.hair_colors
                    .iter()
                    .map(|&id| DataId(id))
                    .collect::<Vec<_>>(),
                if old { 30 } else { 208 },
                false,
            ),
            (
                EParts::Eyes,
                sex.eye_colors.iter().map(|&id| DataId(id)).collect(),
                if old { 35 } else { 259 },
                true,
            ),
            (
                EParts::Skin,
                vec![sex.skin_palset],
                if old { 12 } else { 176 },
                false,
            ),
        ] {
            screen.set_selection(&mut shell.ui, part);
            assert!(!ids.is_empty());
            for (actual, id) in screen.color_wheel.iter().zip(ids) {
                let expected = if direct {
                    palette(&store, id, index)
                } else {
                    averaged(&store, id, index)
                };
                assert_eq!(*actual, Some(expected), "{old} {part:?} {id:?}");
            }
        }
        screen.set_choice(&mut shell.ui, false);
        screen.state.set_headgear_style(&tables.chargen, 0);
        screen.state.set_shirt_style(&tables.chargen, 0);
        screen.state.set_trousers_style(&tables.chargen, 0);
        screen.state.set_footwear_style(&tables.chargen, 0);
        for (part, ids, compact_index) in [
            (
                EParts::Headgear,
                screen.state.headgear_pal_set_ids.clone(),
                253,
            ),
            (EParts::Shirt, screen.state.shirt_pal_set_ids.clone(), 49),
            (
                EParts::Trousers,
                screen.state.trousers_pal_set_ids.clone(),
                68,
            ),
            (
                EParts::Footwear,
                screen.state.footwear_pal_set_ids.clone(),
                164,
            ),
        ] {
            screen.set_selection(&mut shell.ui, part);
            assert!(!ids.is_empty(), "{old} {part:?}");
            for (actual, id) in screen.color_wheel.iter().zip(ids) {
                assert_eq!(
                    *actual,
                    Some(averaged(&store, id, if old { compact_index } else { 1312 })),
                    "{old} {part:?}"
                );
            }
        }
    }
}

fn drain(ui: &mut UiSystem, screen: &mut CharGenScreen) {
    for _ in 0..10 {
        let deliveries = ui.drain_outbox();
        if deliveries.is_empty() {
            return;
        }
        for delivery in deliveries {
            if let dereth_ui::msg::Delivery::Element { msg, .. } = delivery {
                screen.on_element_message(&mut ScreenCx::new(ui), &msg);
            }
        }
    }
    panic!("creation delivery did not settle");
}

fn click(ui: &mut UiSystem, screen: &mut CharGenScreen, handle: dereth_ui::ElemHandle) {
    let rect = ui.screen_box(handle);
    let (x, y) = ((rect.x0 + rect.x1) / 2, (rect.y0 + rect.y1) / 2);
    ui.mouse_move(LocalTime(1.0), x, y);
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    drain(ui, screen);
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn earlier_world_town_buttons_show_and_select_all_six_actual_start_areas() {
    let (mut shell, mut screen, _) = fixture(true);
    screen.set_progress_state(&mut shell.ui, EcgProgress::Town);
    let root = screen.roots()[0];
    let names = [
        "Holtburg South",
        "Holtburg West",
        "Shoushi Southeast",
        "Shoushi West",
        "Yaraq North",
        "Yaraq East",
    ];
    for (index, name) in names.iter().enumerate() {
        let id = dereth_ui::ElementId(0x7f01_0000 + u32::try_from(index).unwrap());
        let button = shell
            .ui
            .get_child_recursive(root, id)
            .expect("named town button");
        let label = shell
            .ui
            .text_element_mut(button)
            .expect("button caption")
            .glyphs
            .inq_text(false);
        assert_eq!(&label, name);
        let rect = shell.ui.screen_box(button);
        assert!(
            rect.x0 >= 0 && rect.x1 < 800 && rect.y0 >= 0 && rect.y1 < 600,
            "{rect:?}"
        );
        click(&mut shell.ui, &mut screen, button);
        assert_eq!(
            screen.state.start_area,
            u32::try_from(index).unwrap(),
            "{name}"
        );
        assert!(screen.open_dialog.is_none());
    }
    let sanamar = shell
        .ui
        .get_child_recursive(root, chargen::TOWN_BUTTONS[3].0)
        .unwrap();
    assert!(!shell.ui.node(sanamar).unwrap().region.flags.visible);
}

/// Behaviour: chargen.controls.wheel-over-scrollbars
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn creation_attribute_and_shade_bars_take_one_step_per_wheel_detent() {
    use dereth_ui::widgets::scrollbar::Scrollbar;
    for old in [true, false] {
        let (mut shell, mut screen, _) = fixture(old);
        for (page, id) in [
            (EcgProgress::Profession, chargen::slider::SCROLL),
            (EcgProgress::Appearance, chargen::appearance::SHADE_SCROLL),
        ] {
            screen.set_progress_state(&mut shell.ui, page);
            if page == EcgProgress::Appearance {
                screen.set_selection(&mut shell.ui, EParts::Hair);
                screen.set_shade(&mut shell.ui, 0.5);
            }
            let bar = shell.ui.get_child_recursive(screen.roots()[0], id).unwrap();
            let targets = std::iter::once(bar)
                .chain(shell.ui.children(bar))
                .collect::<Vec<_>>();
            for target in targets {
                let before = Scrollbar::position(&shell.ui, bar);
                let focus = shell.ui.focus_element();
                let rect = shell.ui.screen_box(target);
                let (x, y) = ((rect.x0 + rect.x1) / 2, (rect.y0 + rect.y1) / 2);
                shell.ui.mouse_move(LocalTime(2.0), x, y);
                shell.ui.mouse_down(5, x, y);
                drain(&mut shell.ui, &mut screen);
                let after = Scrollbar::position(&shell.ui, bar);
                assert!(
                    ((before - after).abs() - 0.01).abs() < 0.0002,
                    "{old} {page:?} {target:?}: one detent must move one percent: {before} -> {after}"
                );
                assert_eq!(
                    shell.ui.focus_element(),
                    focus,
                    "wheel preserves keyboard focus"
                );
                shell.ui.mouse_down(6, x, y);
                drain(&mut shell.ui, &mut screen);
                assert!((Scrollbar::position(&shell.ui, bar) - before).abs() < 0.0002);
            }
        }
    }
}
