//! What a plugin reads of the view's later parts: the place line, the fellowship, allegiance,
//! friends and kingdom, the journal, the map's markers, the pets and the mount, the shortcut bar
//! and crafting, the view made by hand here and read through Auberean's Lua bindings.

use std::collections::BTreeMap;

use auberean_client_contract::event::{EffectEvent, GameEvent};
use auberean_client_contract::notice::{
    Allegiance, Fellow, Fellowship, FriendState, Member, Quest,
};
use auberean_client_contract::session::Phase;
use auberean_client_contract::text::TextRef;
use auberean_client_contract::view::{
    kingdom, Craft, CraftSkill, CraftSkillFacts, Journal, MapMarker, MapMarkers, Pet, Player,
    Recipe, Shortcut, Social, Surroundings, Toolbar, ToolbarSlot, View,
};
use auberean_client_contract::{pet, shortcut_kind};

use crate::common::{frame_seeing, host, package, printed};

const PLAYER: u64 = 0x2130_0000_0000_0001;
const CARA: u64 = 0x2130_0000_0000_0002;
const WOLF: u64 = 0x2140_0000_0000_0011;
const SWORD: u64 = 0x8000_0000_0000_0010;

fn lit(s: &str) -> TextRef {
    TextRef::literal(s)
}

fn member(id: u64, name: &str) -> Member {
    Member {
        id,
        name: lit(name),
        level: 12,
        ..Member::default()
    }
}

fn view() -> View {
    let mut shortcuts = vec![Shortcut::default(); 100];
    shortcuts[2] = Shortcut {
        kind: shortcut_kind::SKILL,
        data_enum: 0x10,
        ..Shortcut::default()
    };
    shortcuts[11] = Shortcut {
        kind: shortcut_kind::ITEM,
        item: SWORD,
        ..Shortcut::default()
    };
    let mut slots = vec![ToolbarSlot::default(); 100];
    slots[2] = ToolbarSlot {
        answer: 1,
        tooltip: "Strike".into(),
        icon: 0x0600_0001,
    };
    let mut categories = BTreeMap::new();
    categories.insert(3, lit("Weapons"));
    View {
        player_id: Some(PLAYER),
        player: Some(Player {
            id: PLAYER,
            name: Some("Tester".into()),
            mounted: true,
            ..Player::default()
        }),
        world: Some(Surroundings {
            player: PLAYER,
            inside: false,
            map_coordinates: Some((12.5, -40.0)),
            radar_ranges: Some((50.0, 75.0)),
            ..Surroundings::default()
        }),
        social: Social {
            fellowship: Some(Fellowship {
                fellows: vec![
                    (
                        PLAYER,
                        Fellow {
                            name: lit("Tester"),
                            health: 80,
                            max_health: 100,
                            ..Fellow::default()
                        },
                    ),
                    (
                        CARA,
                        Fellow {
                            name: lit("Cara"),
                            health: 30,
                            max_health: 90,
                            ..Fellow::default()
                        },
                    ),
                ],
                name: "Wanderers".into(),
                leader: CARA,
                social: 1,
                looting: 0,
            }),
            allegiance: Some(Allegiance {
                monarch: (member(CARA, "Cara"), true),
                member: (member(PLAYER, "Tester"), true),
                name: lit("Order of Dawn"),
                total: 2,
                ..Allegiance::default()
            }),
            friends: vec![FriendState {
                name: "Ilsa".into(),
                location: "Lethe".into(),
                mode: 2,
            }],
            own_state: Some(1),
            kingdom: Some(kingdom::ORDER),
            conflict: Some(1),
        },
        journal: Some(Journal {
            version: 1,
            quests: vec![Quest {
                id: 7,
                name: lit("Lost Ring"),
                seconds_remaining: 600,
                cancellable: true,
                ..Quest::default()
            }],
        }),
        markers: MapMarkers {
            version: 1,
            markers: vec![MapMarker {
                kind: 1,
                object: 0,
                cell: 0x0298_0109,
                picture: 0x0600_0200,
                label: Some(lit("Home")),
            }],
        },
        pets: vec![Pet {
            id: WOLF,
            name: Some(lit("Wolf (Tester)")),
            icon: 0x0600_0101,
            mode: pet::mode::IDLE,
        }],
        toolbar: Some(Toolbar {
            shortcuts,
            slots,
            set: 1,
            locked: true,
            ..Toolbar::default()
        }),
        craft: Some(Craft {
            version: 1,
            experience: 500,
            skills: vec![CraftSkillFacts {
                listed: CraftSkill {
                    did: 0x3E00_0001,
                    name: lit("Smithing"),
                    ..CraftSkill::default()
                },
                own: Some(CraftSkill::default()),
                level: 5,
                can_raise: true,
                recipes: vec![Recipe {
                    did: 0x3F00_0002,
                    name: lit("Iron Sword"),
                    category: 3,
                    shown: true,
                    ..Recipe::default()
                }],
                ..CraftSkillFacts::default()
            }],
            categories,
            opened: None,
        }),
        ..View::default()
    }
}

/// A plugin reads where the player stands, its fellowship, allegiance, friends and kingdom, its
/// journal, the map's markers, its pets and its mount, its shortcut bar and its crafting from the
/// view the frame carries, ids as text and the game's kinds as words.
///
/// Behaviour: plugins.host.a-plugin-reads-the-views-later-parts
#[test]
fn a_plugin_reads_the_social_journal_markers_pets_shortcuts_crafting_and_place_from_the_view() {
    let mut h = host();
    h.request_load(package(
        "test.more",
        "auberean",
        "",
        r#"
        on("effects.added", function()
            local at = game.place()
            print(tostring(at.inside) .. " " .. at.coordinates.east .. " " .. at.coordinates.north .. " " .. at.radar_range)
            local s = game.social()
            local f = s.fellowship
            print(f.name .. " led by " .. f.leader .. ": " .. f.fellows[2].name .. " " .. f.fellows[2].health .. "/" .. f.fellows[2].max_health .. " " .. tostring(f.share_experience))
            print(s.allegiance.name .. " under " .. s.allegiance.monarch.name .. " " .. tostring(s.allegiance.monarch.online))
            print(s.friends[1].name .. " " .. s.friends[1].mode .. " " .. s.state .. " " .. s.kingdom .. " " .. s.conflict)
            local q = game.journal()[1]
            print(q.name .. " " .. q.seconds_remaining .. " " .. tostring(q.cancellable))
            local mk = game.markers()[1]
            print(mk.label .. " " .. mk.cell .. " " .. tostring(mk.object))
            local p = game.pets()[1]
            print(p.name .. " " .. p.id .. " " .. p.mode .. " " .. tostring(game.player().mounted))
            local t = game.toolbar()
            print(t.set .. " " .. tostring(t.locked) .. " " .. #t.slots .. ": " .. t.slots[1].slot .. " " .. t.slots[1].kind .. " " .. t.slots[1].skill .. " " .. t.slots[1].tooltip .. ", " .. t.slots[2].slot .. " " .. t.slots[2].item)
            local c = game.craft()
            local k = c.skills[1]
            print(c.experience .. " " .. k.name .. " " .. k.level .. " " .. tostring(k.trained) .. " " .. k.recipes[1].name .. " in " .. c.categories[1].name .. " " .. tostring(c.opened))
        end)
        "#,
    ));
    let v = view();
    frame_seeing(&mut h, 0.0, Phase::World, &[], &v);
    printed(&mut h);
    frame_seeing(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
        &v,
    );
    assert_eq!(
        printed(&mut h),
        [
            "test.more: false 12.5 -40 75",
            "test.more: Wanderers led by 0x2130000000000002: Cara 30/90 true",
            "test.more: Order of Dawn under Cara true",
            "test.more: Ilsa 2 1 order peace",
            "test.more: Lost Ring 600 true",
            "test.more: Home 43516169 nil",
            "test.more: Wolf  0x2140000000000011 idle true",
            "test.more: 1 true 2: 2 skill 16 Strike, 11 0x8000000000000010",
            "test.more: 500 Smithing 5 true Iron Sword in Weapons nil",
        ]
    );
}
