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

/// Behaviour: chargen.town.an-earlier-world-draws-a-dot-on-the-map-for-each-start
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn earlier_world_town_page_draws_a_dot_by_its_town_for_each_of_the_six_starts() {
    let (mut shell, mut screen, _) = fixture(true);
    screen.set_progress_state(&mut shell.ui, EcgProgress::Town);
    let root = screen.roots()[0];
    let map = shell.ui.screen_box(
        shell
            .ui
            .get_child_recursive(root, chargen::town_page::MAP)
            .expect("the map"),
    );
    // Holtburg, Shoushi, Yaraq: the pin each start's town has on the map.
    let starts = [
        ("Holtburg South", 0),
        ("Holtburg West", 0),
        ("Shoushi Southeast", 1),
        ("Shoushi West", 1),
        ("Yaraq North", 2),
        ("Yaraq East", 2),
    ];
    let mut boxes = Vec::new();
    for (index, (name, town)) in starts.iter().enumerate() {
        let id = dereth_ui::ElementId(0x7f01_0000 + u32::try_from(index).unwrap());
        let dot = shell
            .ui
            .get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{name}'s dot"));
        assert!(shell.ui.node(dot).unwrap().region.flags.visible, "{name}");
        let captions: Vec<String> = std::iter::once(dot)
            .chain(shell.ui.children(dot))
            .filter_map(|h| {
                shell
                    .ui
                    .text_element_mut(h)
                    .map(|t| t.glyphs.inq_text(false))
            })
            .filter(|t| !t.is_empty())
            .collect();
        assert!(
            captions.is_empty(),
            "{name}: a dot, not a captioned button: {captions:?}"
        );
        assert_eq!(
            shell.ui.node(dot).unwrap().tooltip_text.as_deref(),
            Some(format!("Start in {name}.").as_str())
        );
        let rect = shell.ui.screen_box(dot);
        assert!(
            rect.x0 >= map.x0 && rect.x1 <= map.x1 && rect.y0 >= map.y0 && rect.y1 <= map.y1,
            "{name} {rect:?} on the map {map:?}"
        );
        // Beside its own town's dot, within twice its width.
        let pin = shell
            .ui
            .get_child_recursive(root, chargen::TOWN_BUTTONS[*town].0)
            .unwrap();
        let town_dot = shell.ui.screen_box(
            shell
                .ui
                .get_child_recursive(pin, chargen::town_page::PIN_DOT)
                .unwrap(),
        );
        let centre = |b: dereth_ui::Box2D| ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let (a, b) = (centre(rect), centre(town_dot));
        assert!(
            (a.0 - b.0).abs() <= 2 * town_dot.width() && (a.1 - b.1).abs() <= 2 * town_dot.width(),
            "{name} {a:?} by its town {b:?}"
        );
        click(&mut shell.ui, &mut screen, dot);
        assert_eq!(
            screen.state.start_area,
            u32::try_from(index).unwrap(),
            "{name}"
        );
        assert!(screen.open_dialog.is_none());
        let title = shell
            .ui
            .get_child_recursive(root, chargen::town_page::TITLE)
            .unwrap();
        assert_eq!(
            shell
                .ui
                .text_element_mut(title)
                .unwrap()
                .glyphs
                .inq_text(false),
            name.split_whitespace().next().unwrap(),
            "the title holds the town's name"
        );
        let pane = shell
            .ui
            .get_child_recursive(root, chargen::town_page::TEXT)
            .unwrap();
        let text = shell
            .ui
            .text_element_mut(pane)
            .unwrap()
            .glyphs
            .inq_text(false);
        let town_name = name.split_whitespace().next().unwrap();
        assert!(
            text.starts_with(&format!("{name}\n\n")) && text[name.len()..].contains(town_name),
            "{name}: the start's name, then the town's own text: {text:?}"
        );
        // Clear of every town's name on the map.
        for (pin_id, _) in chargen::TOWN_BUTTONS {
            let pin = shell.ui.get_child_recursive(root, pin_id).unwrap();
            if !shell.ui.node(pin).unwrap().region.flags.visible {
                continue;
            }
            let label = shell.ui.screen_box(
                shell
                    .ui
                    .get_child_recursive(pin, chargen::town_page::PIN_NAME)
                    .unwrap(),
            );
            assert!(
                rect.x1 <= label.x0
                    || label.x1 <= rect.x0
                    || rect.y1 <= label.y0
                    || label.y1 <= rect.y0,
                "{name} {rect:?} covers a town's name {label:?}"
            );
        }
        boxes.push(rect);
    }
    // No two dots overlap.
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(
                a.x1 <= b.x0 || b.x1 <= a.x0 || a.y1 <= b.y0 || b.y1 <= a.y0,
                "{a:?} and {b:?} overlap"
            );
        }
    }
    let sanamar = shell
        .ui
        .get_child_recursive(root, chargen::TOWN_BUTTONS[3].0)
        .unwrap();
    assert!(!shell.ui.node(sanamar).unwrap().region.flags.visible);
}

/// Behaviour: chargen.text.an-earlier-world-shows-its-own-creation-texts
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn earlier_world_heritage_profession_and_naming_texts_are_the_worlds_own() {
    let pane = |shell: &mut UiShell, root, id| {
        let h = shell.ui.get_child_recursive(root, id).expect("a text pane");
        shell.ui.text_element_mut(h).unwrap().glyphs.inq_text(false)
    };
    let (mut shell, mut screen, _) = fixture(true);
    let root = screen.roots()[0];
    screen.set_progress_state(&mut shell.ui, EcgProgress::Hertage);
    screen.heritage_page_update(&mut shell.ui);
    let heritage = pane(&mut shell, root, chargen::heritage_page::TEXT);
    assert!(
        heritage.starts_with("ALUVIANS are a fiercely individualistic"),
        "{heritage:?}"
    );
    screen.set_progress_state(&mut shell.ui, EcgProgress::Profession);
    screen.update_profession(&mut shell.ui);
    let profession = pane(&mut shell, root, chargen::PROFESSION_DESC);
    assert!(
        profession.contains("Suggested skills"),
        "the February 2005 profession text: {profession:?}"
    );
    screen.set_progress_state(&mut shell.ui, EcgProgress::Summary);
    screen.set_how_to_text(&mut shell.ui);
    let naming = pane(&mut shell, root, chargen::summary_page::HOW_TO);
    assert!(
        naming.contains("Many Aluvian women have only a first name"),
        "{naming:?}"
    );
    screen.set_progress_state(&mut shell.ui, EcgProgress::Skills);
    let skills = pane(&mut shell, root, chargen::skills_page::DESCRIPTION);
    assert!(
        skills.starts_with("Use this screen to select and modify your character's skills.")
            && skills.contains("(from Untrained to Trained"),
        "the February 2005 skills help: {skills:?}"
    );
    screen.set_progress_state(&mut shell.ui, EcgProgress::Appearance);
    let looks = pane(&mut shell, root, chargen::appearance::HELP);
    assert!(
        looks.starts_with("Use the tools below to select your character's appearance")
            && looks.contains("customize the clothing"),
        "the February 2005 appearance and clothing help: {looks:?}"
    );

    // The end of retail names no texts of its own: the interface's strings stay.
    let (mut shell, mut screen, _) = fixture(false);
    let root = screen.roots()[0];
    screen.set_progress_state(&mut shell.ui, EcgProgress::Hertage);
    screen.heritage_page_update(&mut shell.ui);
    let heritage = pane(&mut shell, root, chargen::heritage_page::TEXT);
    assert!(!heritage.starts_with("ALUVIANS"), "{heritage:?}");
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
