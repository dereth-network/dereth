//! Every interface asks one question of a world: with one system's flag off and everything else
//! on, each interface that has that system's window, tab or slot refuses it, and with everything
//! on each offers it. A world whose era is not known has everything.
//! Fixture: the interfaces' own tables of what belongs to which system; no device, no dats.

use dereth_client_contract::era::EraSurface;
use dereth_primitives::EraFeatures;
use dereth_ui::ElementId;
use dereth_ui_screens::panels::{
    contracts, era, house, journal, minigame, research, salvage, titles, trade,
};

use dereth_horizon::ui::panels::{world::doll_slot_surface, WindowId};

/// Where one interface keeps a surface in Horizon, whose tabs and slots have no window of their
/// own.
#[derive(Debug, Clone, Copy)]
enum Horizon {
    Window(WindowId),
    Slot(u32),
    /// A key of its own, on a window that holds more than this system.
    Key(u32),
    /// A page of the Spellbook window, which reads the surface itself.
    Page,
}

/// One surface in each interface.
struct Row {
    /// The element of the Retail layout.
    retail: ElementId,
    /// Classic's panel, or the paper doll's accessory.
    classic: Option<Classic>,
    horizon: Horizon,
    /// Everything on but what this surface needs.
    lacking: EraFeatures,
}

#[derive(Debug, Clone, Copy)]
enum Classic {
    Panel(&'static str),
    Accessory(usize),
    /// The toolbar's button, which asks the shared quest fallback itself.
    Button,
}

fn without(f: fn(&mut EraFeatures)) -> EraFeatures {
    let mut features = EraFeatures::ALL;
    f(&mut features);
    features
}

/// The table. A new surface does not compile without a row here.
fn row(surface: EraSurface) -> Row {
    use dereth_rules::slots::loc;
    match surface {
        EraSurface::QuestWindow => Row {
            retail: journal::PAGE,
            classic: Some(Classic::Button),
            horizon: Horizon::Window(WindowId::Journal),
            lacking: without(|f| {
                f.journal = false;
                f.contracts = false;
            }),
        },
        EraSurface::Journal => Row {
            retail: journal::PANEL,
            classic: Some(Classic::Panel("journal")),
            horizon: Horizon::Key(0x1000_0128),
            lacking: without(|f| f.journal = false),
        },
        EraSurface::Contracts => Row {
            retail: contracts::PANEL,
            classic: Some(Classic::Panel("contracts")),
            horizon: Horizon::Key(0x1000_012E),
            lacking: without(|f| f.contracts = false),
        },
        EraSurface::Titles => Row {
            retail: titles::PANEL,
            classic: Some(Classic::Panel("titles")),
            horizon: Horizon::Key(0x1000_011A),
            lacking: without(|f| f.titles = false),
        },
        EraSurface::House => Row {
            retail: house::PANEL,
            classic: Some(Classic::Panel("house")),
            horizon: Horizon::Window(WindowId::House),
            lacking: without(|f| {
                f.housing = false;
                f.apartments = false;
            }),
        },
        EraSurface::Chess => Row {
            retail: minigame::PANEL,
            classic: Some(Classic::Panel("game-center")),
            horizon: Horizon::Window(WindowId::GameCenter),
            lacking: without(|f| f.chess = false),
        },
        EraSurface::Trade => Row {
            retail: trade::WINDOW,
            classic: Some(Classic::Panel("trade")),
            horizon: Horizon::Window(WindowId::Trade),
            lacking: without(|f| f.trade = false),
        },
        EraSurface::Salvage => Row {
            retail: salvage::PANEL,
            classic: Some(Classic::Panel("salvage")),
            horizon: Horizon::Window(WindowId::Salvage),
            lacking: without(|f| f.tinkering = false),
        },
        EraSurface::SpellResearch => Row {
            retail: research::PAGE,
            classic: Some(Classic::Panel("spell-research")),
            horizon: Horizon::Page,
            lacking: without(|f| f.spell_research = false),
        },
        EraSurface::CloakSlot => Row {
            retail: era::CLOAK_SLOT,
            classic: Some(Classic::Accessory(0)),
            horizon: Horizon::Slot(loc::CLOAK),
            lacking: without(|f| f.cloaks = false),
        },
        EraSurface::TrinketSlot => Row {
            retail: era::TRINKET_SLOT,
            classic: Some(Classic::Accessory(1)),
            horizon: Horizon::Slot(loc::TRINKET_ONE),
            lacking: without(|f| f.trinkets = false),
        },
    }
}

/// The surface Retail, Classic and Horizon each say the row's place belongs to; `None` where the
/// interface has no table of its own for it.
fn claimed(row: &Row) -> [Option<EraSurface>; 3] {
    [
        era::era_surface_of(row.retail),
        row.classic.and_then(|c| match c {
            Classic::Panel(id) => dereth_classic_ui::panels::era_surface_of(id),
            Classic::Accessory(i) => dereth_classic_ui::panels::accessory_surface(i),
            Classic::Button => None,
        }),
        match row.horizon {
            Horizon::Window(id) => id.surface(),
            Horizon::Slot(mask) => doll_slot_surface(mask),
            Horizon::Key(action) => dereth_horizon::ui::hud::key_surface(action),
            Horizon::Page => None,
        },
    ]
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
#[test]
fn every_interface_gives_a_surface_the_same_answer_for_every_world() {
    for surface in EraSurface::ALL {
        let row = row(surface);
        for (face, said) in ["retail", "classic", "horizon"].iter().zip(claimed(&row)) {
            if let Some(said) = said {
                assert_eq!(said, surface, "{face} files it under another system");
            }
        }
        assert_eq!(
            era::era_surface_of(row.retail),
            Some(surface),
            "the Retail layout has every surface"
        );
        for (features, offered) in [(EraFeatures::ALL, true), (row.lacking, false)] {
            assert_eq!(surface.offered(features), offered, "{surface:?}");
            // Retail refuses by element.
            assert_eq!(
                dereth_ui_screens::screens::gameplay::era_lacks_page(features, row.retail),
                !offered,
                "retail {surface:?}"
            );
            // Classic refuses by panel id, Horizon by window.
            if let Some(Classic::Panel(id)) = row.classic {
                assert_eq!(
                    dereth_classic_ui::panels::era_refuses(id, features),
                    !offered,
                    "classic {surface:?}"
                );
            }
            if let Horizon::Window(id) = row.horizon {
                assert_eq!(id.offered(&features), offered, "horizon {surface:?}");
            }
        }
        // A world whose era is not known has everything.
        assert!(!surface.refused(None), "{surface:?}");
    }
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
/// A window with no system of its own is on every world.
#[test]
fn a_window_of_no_system_is_offered_on_a_world_with_nothing() {
    for id in ["inventory", "options", "map", "social", "spellbook"] {
        assert!(!dereth_classic_ui::panels::era_refuses(
            id,
            EraFeatures::NONE
        ));
    }
    for id in [
        WindowId::Inventory,
        WindowId::Options,
        WindowId::Map,
        WindowId::Social,
        WindowId::Actions,
    ] {
        assert!(id.offered(&EraFeatures::NONE), "{id:?}");
    }
}
