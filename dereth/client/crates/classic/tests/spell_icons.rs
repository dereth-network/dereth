use super::*;
use crate::panels::Context;

struct Fonts;
impl dereth_classic_dat::fonts::FontSource for Fonts {
    fn rasterize(
        &self,
        _: &dereth_classic_dat::fonts::FontSpec,
    ) -> std::result::Result<dereth_classic_dat::fonts::FontAtlas, String> {
        Ok(Default::default())
    }
}
/// Behaviour: spellbar.icons.raw-power-background
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "requires retail DATs")]
fn runtime_spell_power_reaches_classic_rows_and_supported_background_pixels() {
    let store = dereth_dat::testing::open_store_or_fail();
    let path = std::path::PathBuf::from(
        std::env::var_os("DERETH_CLASSIC_PORTAL").expect("classic portal"),
    );
    let art = Arc::new(
        ClassicArt::new(
            dereth_classic_dat::ClassicPortal::open(&path).unwrap(),
            &Fonts,
        )
        .unwrap(),
    );
    let mut canvas = Canvas::new(art, (300, 362)).unwrap();
    let mut hud = dereth_client_runtime::hud::Hud::new();
    let objects = dereth_client_runtime::objects::ObjectStream::new();
    hud.load_tables(&store, &objects.world);
    let ids: Vec<_> = [0x6e, 0x70, 0xc0, 0xc1]
        .into_iter()
        .map(|component| {
            *hud.spell_table
                .as_ref()
                .unwrap()
                .spells
                .iter()
                .find(|(_, b)| {
                    dereth_client_contract::spellbook::power_component(b.raw_comps[0], b.comp_key)
                        == component
                        && b.icon != 0
                })
                .unwrap()
                .0
        })
        .collect();
    hud.spells = ids.iter().map(|id| hud.spell_entry(*id).unwrap()).collect();
    let view = hud.view(&objects);
    let ctx = Context {
        now: dereth_primitives::LocalTime(0.0),
        game: &view,
        pregame: &Default::default(),
        keyboard: &Default::default(),
        settings: &Default::default(),
        map_teleport_allowed: false,
        classic: &Default::default(),
    };
    let panel = crate::panels::factory("spellbook", dereth_primitives::LocalTime(0.0)).unwrap();
    let frame = panel.frame(&ctx);
    canvas.load_runtime_images(&frame.screen, &store).unwrap();
    let commands: Vec<_> = frame
        .screen
        .commands
        .iter()
        .filter_map(|command| match command {
            Command::SpellIcon {
                icon,
                power,
                bitfield,
                ..
            } => Some((*icon, *power, *bitfield)),
            _ => None,
        })
        .collect();
    assert_eq!(commands.len(), 4);
    for (entry, (power, level, background)) in hud.spells.iter().zip([
        (7, 6, Some(0x060013f6)),
        (8, 7, Some(0x06001f63)),
        (9, 7, None),
        (10, 8, None),
    ]) {
        assert_eq!((entry.icon_power, entry.level), (power, level));
        let &(icon, sent_power, _) = commands
            .iter()
            .find(|(icon, _, _)| Some(dereth_primitives::DataId(*icon)) == entry.icon)
            .unwrap();
        // Multiple spells may share their base icon, so check the power-bearing command too.
        assert!(
            commands.iter().any(|(id, p, _)| *id == icon && *p == power),
            "{sent_power} lost raw power {power}"
        );
        let raw = canvas
            .read_pixels(
                &canvas
                    .manifest
                    .image(&format!("{icon:08X}"))
                    .unwrap()
                    .rgba_file,
            )
            .unwrap();
        let composed = canvas.spell_pixels(icon, power, 0, false).unwrap();
        let expected = background
            .map(|id| canvas.read_pixels(&format!("{id:08X}")).unwrap())
            .unwrap_or_else(|| vec![0; 32 * 32 * 4]);
        let black: Vec<_> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[0] < 8 && p[1] < 4 && p[2] < 8)
            .map(|(i, _)| i)
            .collect();
        assert!(
            !black.is_empty(),
            "base icon has transparent surrounding pixels"
        );
        for i in black {
            assert_eq!(
                &composed[i * 4..i * 4 + 4],
                &[
                    expected[i * 4 + 2],
                    expected[i * 4 + 1],
                    expected[i * 4],
                    expected[i * 4 + 3]
                ]
            );
        }
    }
    let spell_screen = Screen {
        width: frame.screen.width,
        height: frame.screen.height,
        commands: frame
            .screen
            .commands
            .into_iter()
            .filter(|command| matches!(command, Command::SpellIcon { .. }))
            .collect(),
    };
    canvas.resize((spell_screen.width, spell_screen.height));
    let mut present = dereth_client_runtime::present::NullPresentation::new(
        spell_screen.width,
        spell_screen.height,
    );
    canvas
        .compose(&mut present, &spell_screen, &|_| None)
        .unwrap();
    assert!(present.counts().overlay_uploads >= 4);
}

/// Behaviour: classic.spell-drag.preserves-transparent-coverage
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "requires retail DATs")]
fn spell_drag_preserves_background_coverage_and_opaque_foreground() {
    let store = dereth_dat::testing::open_store_or_fail();
    let path = std::path::PathBuf::from(
        std::env::var_os("DERETH_CLASSIC_PORTAL").expect("classic portal"),
    );
    let art = Arc::new(
        ClassicArt::new(
            dereth_classic_dat::ClassicPortal::open(&path).unwrap(),
            &Fonts,
        )
        .unwrap(),
    );
    let mut canvas = Canvas::new(art, (32, 32)).unwrap();
    let mut hud = dereth_client_runtime::hud::Hud::new();
    let objects = dereth_client_runtime::objects::ObjectStream::new();
    hud.load_tables(&store, &objects.world);
    let icon = hud
        .spell_table
        .as_ref()
        .unwrap()
        .spells
        .values()
        .find(|spell| spell.icon != 0)
        .unwrap()
        .icon;
    let mut commands = Vec::new();
    for transparent in [false, true] {
        commands.push(Command::SpellIcon {
            icon,
            power: 1,
            bitfield: 0,
            transparent,
            x: 0,
            y: 0,
            width: 32,
            height: 32,
            clip: None,
        });
    }
    let screen = Screen {
        width: 32,
        height: 32,
        commands,
    };
    canvas.load_runtime_images(&screen, &store).unwrap();
    let row = canvas.spell_pixels(icon, 1, 0, false).unwrap();
    let drag = canvas.spell_pixels(icon, 1, 0, true).unwrap();
    let mut holes = 0;
    let mut opaque = 0;
    for (row, drag) in row.as_chunks::<4>().0.iter().zip(drag.as_chunks::<4>().0) {
        assert_eq!(&row[..3], &drag[..3], "drag retains foreground colors");
        if drag[3] == 0 {
            holes += 1;
            assert_eq!(row, &[0, 0, 0, 255]);
        } else {
            opaque += 1;
            assert_eq!(row, drag, "foreground stays fully opaque");
        }
    }
    assert!(holes > 0);
    assert!(opaque > 0);
    assert!(
        drag.as_chunks::<4>().0.contains(&[10, 10, 10, 255]),
        "dark foreground remains opaque"
    );
    let mut present = dereth_client_runtime::present::NullPresentation::new(32, 32);
    canvas.compose(&mut present, &screen, &|_| None).unwrap();
    assert_eq!(
        present.counts().overlay_uploads,
        2,
        "row and drag cache entries differ"
    );
}
