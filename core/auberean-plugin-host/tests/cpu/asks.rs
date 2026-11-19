//! What a plugin asks of the game beyond the first cut: the selection keys' selections, the
//! camera, the pets, the shortcut bar, the fellowship, allegiance and friends, and crafting, each
//! the action the game's own interface gives for the same gesture.

use auberean_client_contract::event::{EffectEvent, GameEvent};
use auberean_client_contract::orbit::{MovementMode, OrbitSettings};
use auberean_client_contract::session::Phase;
use auberean_client_contract::{
    pet, shortcut_kind, Action, CraftAction, PetAction, SelectKind, SocialAction, ToolbarShortcut,
};

use crate::common::{frame, host, package, printed};

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// A plugin selects as the selection keys do, commands its pets, sets its shortcut bar, asks of
/// its fellowship, allegiance and friends, and makes things, each as the contract's action in the
/// order asked; a wrong argument fails the call, saying why, and asks nothing.
///
/// Behaviour: plugins.host.a-plugins-later-requests-are-the-clients-actions
#[test]
fn a_plugins_selections_pets_shortcuts_social_and_crafting_requests_are_the_clients_actions() {
    let mut h = host();
    h.request_load(package(
        "test.asks",
        "auberean",
        "",
        r#"
        on("effects.added", function()
            act.select_next("foe")
            act.select_next("fellow", true)
            act.select_nearest("item")
            act.select_self()
            act.select_last()
            act.select_last_attacker()
            pets.attack("0x80000000000000ab")
            pets.come_back()
            pets.buff()
            pets.heal()
            pets.mode("0x2140000000000011", "idle")
            pets.rename("0x2140000000000011", "Fang")
            toolbar.show_set(2)
            toolbar.put(12, { kind = "skill", skill = 16 })
            toolbar.put(13, { kind = "item", item = "0x8000000000000010" })
            toolbar.put(14, nil)
            social.create_fellowship("Wanderers", false, 1)
            social.disband_fellowship()
            social.leave_fellowship()
            social.recruit("0x2130000000000002")
            social.dismiss("0x2130000000000002")
            social.ask_allegiance("0x2130000000000002")
            social.swear()
            social.break_with("0x2130000000000002")
            social.add_friend("Ilsa")
            social.remove_friend("Ilsa")
            social.ask_friends()
            social.set_state("away", "Lethe")
            craft.open(0x3F000002)
            craft.read(0x3F000002, 2)
            craft.raise(0x3E000001)
            craft.add(0x3F000002, 1, "0x8000000000000010", 2)
            craft.make(0x3F000002, 2, { { slot = 1, item = "0x8000000000000010" } })
            for _, bad in ipairs({
                function() act.select_next("dragon") end,
                function() pets.mode("0x2140000000000011", "sleepy") end,
                function() toolbar.put(100, nil) end,
                function() toolbar.show_set(11) end,
                function() social.recruit(nil) end,
            }) do
                local ok, err = pcall(bad)
                print(tostring(ok) .. " " .. tostring(err ~= nil))
            end
        end)
        "#,
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    printed(&mut h);
    let actions = frame(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
    );
    assert_eq!(printed(&mut h), vec!["test.asks: false true"; 5]);
    let item = |data_enum: u32, item: u64, kind: u32| ToolbarShortcut {
        kind,
        item,
        data_enum,
        ..ToolbarShortcut::default()
    };
    let cara = 0x2130_0000_0000_0002;
    let wolf = 0x2140_0000_0000_0011;
    let sword = 0x8000_0000_0000_0010;
    assert_eq!(
        actions,
        [
            Action::SelectCycle {
                kind: SelectKind::Foe,
                previous: false
            },
            Action::SelectCycle {
                kind: SelectKind::Fellow,
                previous: true
            },
            Action::SelectNearest {
                kind: SelectKind::Item
            },
            Action::SelectSelf,
            Action::SelectLast,
            Action::SelectLastAttacker,
            Action::Pet(PetAction::Attack(0x8000_0000_0000_00AB)),
            Action::Pet(PetAction::Return),
            Action::Pet(PetAction::Buff),
            Action::Pet(PetAction::Heal),
            Action::Pet(PetAction::Mode {
                pet: wolf,
                mode: pet::mode::IDLE
            }),
            Action::Pet(PetAction::Name {
                pet: wolf,
                name: "Fang".into()
            }),
            Action::ToolbarSet { set: 2 },
            Action::SetToolbarSlot {
                slot: 12,
                shortcut: item(16, 0, shortcut_kind::SKILL)
            },
            Action::SetToolbarSlot {
                slot: 13,
                shortcut: item(0, sword, shortcut_kind::ITEM)
            },
            Action::SetToolbarSlot {
                slot: 14,
                shortcut: ToolbarShortcut::default()
            },
            Action::Social(SocialAction::CreateFellowship {
                name: units("Wanderers"),
                social: false,
                looting: 1
            }),
            Action::Social(SocialAction::DisbandFellowship),
            Action::Social(SocialAction::LeaveFellowship),
            Action::Social(SocialAction::Recruit(cara)),
            Action::Social(SocialAction::Dismiss(cara)),
            Action::Social(SocialAction::QueryAllegiance(cara)),
            Action::Social(SocialAction::Swear),
            Action::Social(SocialAction::Break(cara)),
            Action::Social(SocialAction::AddFriend(units("Ilsa"))),
            Action::Social(SocialAction::RemoveFriend(units("Ilsa"))),
            Action::Social(SocialAction::GetFriends),
            Action::Social(SocialAction::SetState {
                mode: 2,
                location: units("Lethe")
            }),
            Action::Craft(CraftAction::Open(0x3F00_0002)),
            Action::Craft(CraftAction::Inquire {
                recipe: 0x3F00_0002,
                spinner: 2
            }),
            Action::Craft(CraftAction::RaiseCraftSkill(0x3E00_0001)),
            Action::Craft(CraftAction::AddIngredient {
                recipe: 0x3F00_0002,
                ordinal: 1,
                item: sword,
                spinner: 2
            }),
            Action::Craft(CraftAction::ExecuteCraft {
                recipe: 0x3F00_0002,
                spinner: 2,
                ingredients: vec![(1, sword)]
            }),
        ]
    );
}

/// A plugin chooses the orbit camera with its settings (the turning rates held to their ranges),
/// or the game's own; the program the plugins run in takes the last choice of the frame once, as
/// an interface's choice of camera.
///
/// Behaviour: plugins.host.a-plugin-chooses-the-camera
#[test]
fn a_plugin_chooses_the_orbit_camera_or_the_games_own_and_the_product_takes_the_last() {
    let mut h = host();
    h.request_load(package(
        "test.eye",
        "auberean",
        "",
        r#"
        on("effects.added", function(e)
            if e.effect == 1 then
                act.camera({ movement = "camera" })
                act.camera({ movement = "character", reverse_y = true, key_turn = 100 })
            else
                act.camera(nil)
                print(tostring(pcall(act.camera, { movement = "sideways" })))
            end
        end)
        "#,
    ));
    assert_eq!(h.take_camera(), None, "none chose");
    frame(&mut h, 0.0, Phase::World, &[]);
    printed(&mut h);
    frame(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
    );
    assert_eq!(
        h.take_camera(),
        Some(Some(OrbitSettings {
            movement: MovementMode::Character,
            reverse_y: true,
            key_turn: 6.0,
            ..OrbitSettings::default()
        })),
        "the last, its rates held to their ranges"
    );
    assert_eq!(h.take_camera(), None, "taken once");
    frame(
        &mut h,
        0.2,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 2 })],
    );
    assert_eq!(h.take_camera(), Some(None), "the game's own");
    assert_eq!(printed(&mut h), ["test.eye: false"]);
}
