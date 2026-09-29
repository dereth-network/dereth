//! ACE: Source/ACE.Server/WorldObjects/Player_Use.cs::HandleActionUseWithTarget
//! UseWithTarget runs a recipe; risky tinker and rare gem confirmed through ConfirmationResponse;
//! craft tool special sources route before the RecipeManager; gem tailoring kit to tailoring.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use dereth_primitives::ObjectId;
use dereth_protocol::comms::{CharacterConfirmationRequest, CharacterConfirmationResponse};
use dereth_protocol::items::InventoryUseWithTargetEvent;
use dereth_protocol::objects::ItemUseDone;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    CharacterOption, ConfirmationType, ItemType, PropertyInt, Skill, WeenieError,
};
use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::handle_action_use_on_target::handle_action_use_on_target;
use empyrean_world::world_objects::container;

use super::recipes_tinkering_salvage::{
    chats, exchange, first, give, join, server, ALPHA, CHAT, CONFIRM, CONTAIN_ID, CREATE_OBJECT,
    DOUGH, FLOUR, IRON_BAG, MOTION, PET_DEVICE, PRIVATE_INT, RARE_GEM, REMOVE, SWORD,
    UPDATE_OBJECT, USE_DONE, WATER,
};

fn use_with_target(source: ObjectGuid, target: ObjectGuid) -> InventoryUseWithTargetEvent {
    InventoryUseWithTargetEvent {
        object: ObjectId(source.full()),
        target: ObjectId(target.full()),
    }
}

/// `TargetType`, which `HandleActionUseWithTarget` re-verifies against the target's `ItemType`
/// (the shared crafting weenies leave it unset).
fn set_target_type(ts: &mut empyrean_testkit::TestServer, source: ObjectGuid, t: ItemType) {
    ts.world
        .objects
        .get_mut(source)
        .unwrap()
        .set_property(PropertyInt::TargetType, i32::try_from(t.0).unwrap());
}

fn confirmation_open(
    ts: &empyrean_testkit::TestServer,
    player: ObjectGuid,
    t: ConfirmationType,
) -> bool {
    ts.world
        .objects
        .get(player)
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .confirmation_manager
        .contains(t)
}

/// Flour used on water from the pack: `UseWithTarget` -> `WorldObject.HandleActionUseOnTarget` ->
/// `RecipeManager.UseObjectOnTarget` (the cook-book recipe): the clap, both ingredients consumed,
/// the dough created, the recipe's message and `UseDone`.
#[test]
fn a_recipe_goes_through_use_with_target() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 100);
    let alpha = ObjectGuid::new(ALPHA);
    let flour = give(&mut ts, FLOUR);
    let water = give(&mut ts, WATER);
    set_target_type(&mut ts, flour, ItemType::Food);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.5, |ts| {
        ts.send_game_action(id, &use_with_target(flour, water))
    });
    assert_eq!(
        sent,
        [
            MOTION,
            REMOVE,
            PRIVATE_INT,
            REMOVE,
            PRIVATE_INT,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            CHAT,
            USE_DONE
        ],
        "{sent:04X?}"
    );
    assert_eq!(chats(&g), ["You make some dough."]);
    let done: ItemUseDone = first(&g, USE_DONE).decode();
    assert_eq!(done.failure_type, 0);
    let pack = container::inventory_values(&ts.world, alpha);
    assert!(
        !pack.contains(&flour) && !pack.contains(&water),
        "both ingredients are consumed"
    );
    assert!(
        pack.iter().any(|&o| ts
            .world
            .objects
            .get(o)
            .is_some_and(|o| o.biota.weenie_class_id == DOUGH)),
        "the dough is in the pack"
    );
}

/// Iron salvage used on a sword: `UseWithTarget` -> `CraftTool.HandleActionUseOnTarget` (not a
/// spirit, mana stone or plating device) -> the recipe manager, which asks first (clap,
/// ConfirmationRequest of type CraftInteraction, UseDone). The client answers yes with
/// `ConfirmationResponse`: `ConfirmationManager.HandleResponse` ->
/// `Confirmation_CraftInteration.ProcessConfirmation` -> `UseObjectOnTarget(confirmed)`, and the
/// seeded roll succeeds (the chance is 1.0). A second risky craft is answered no: YouChickenOut, and
/// nothing is consumed.
#[test]
fn a_tinkers_risky_craft_is_confirmed_through_confirmation_response() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 10_000);
    let alpha = ObjectGuid::new(ALPHA);
    let sword = give(&mut ts, SWORD);
    let bag = give(&mut ts, IRON_BAG);
    for (g, iw, n) in [(sword, 6, None), (bag, 100, Some(10))] {
        let o = ts.world.objects.get_mut(g).unwrap();
        o.set_item_workmanship(Some(iw));
        o.set_num_items_in_material(n);
    }
    ts.world
        .objects
        .get_mut(bag)
        .unwrap()
        .set_structure(Some(100));
    set_target_type(&mut ts, bag, ItemType::MeleeWeapon);
    ts.advance(0.1);

    // ---- the dialog
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &use_with_target(bag, sword))
    });
    assert_eq!(sent, [MOTION, CONFIRM, USE_DONE], "{sent:04X?}");
    let ask: CharacterConfirmationRequest = first(&g, CONFIRM).decode();
    assert_eq!(
        ask.confirmation_type,
        i32::try_from(ConfirmationType::CraftInteraction.0).unwrap()
    );
    assert_eq!(
        ask.text,
        "You determine that you have a 100 percent chance to succeed."
    );
    assert!(
        confirmation_open(&ts, alpha, ConfirmationType::CraftInteraction),
        "the one manager holds it"
    );

    // ---- yes, over the wire
    ThreadSafeRandom::seed(5901);
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(
            id,
            &CharacterConfirmationResponse {
                confirmation_type: ask.confirmation_type,
                context_id: ask.context_id,
                accepted: 1,
            },
        );
    });
    assert_eq!(
        sent,
        [REMOVE, PRIVATE_INT, CHAT, UPDATE_OBJECT],
        "{sent:04X?}"
    );
    assert_eq!(
        chats(&g),
        ["Alpha successfully applies the Iron Salvaged  (workmanship 10.00) to the Sword."]
    );
    let s = ts.world.objects.get(sword).expect("the sword is kept");
    assert_eq!((s.damage(), s.num_times_tinkered()), (Some(11), 1));
    assert!(ts.world.objects.get(bag).is_none(), "the bag is used up");
    assert!(
        !confirmation_open(&ts, alpha, ConfirmationType::CraftInteraction),
        "answered"
    );

    // ---- a second bag, answered no: YouChickenOut, nothing consumed
    ts.world
        .objects
        .get_mut(alpha)
        .unwrap()
        .biota
        .properties_skill
        .as_mut()
        .unwrap()
        .get_mut(&Skill::WeaponTinkering)
        .unwrap()
        .init_level = 0;
    let bag = give(&mut ts, IRON_BAG);
    {
        let o = ts.world.objects.get_mut(bag).unwrap();
        o.set_item_workmanship(Some(10));
        o.set_num_items_in_material(Some(10));
        o.set_structure(Some(100));
    }
    set_target_type(&mut ts, bag, ItemType::MeleeWeapon);
    ts.advance(0.1);
    let (_, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &use_with_target(bag, sword))
    });
    let ask: CharacterConfirmationRequest = first(&g, CONFIRM).decode();
    assert_eq!(
        ask.text,
        "You determine that you have a 0 percent chance to succeed."
    );
    let (sent, _) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(
            id,
            &CharacterConfirmationResponse {
                confirmation_type: ask.confirmation_type,
                context_id: ask.context_id,
                accepted: 0,
            },
        );
    });
    assert_eq!(sent, [0x028A], "YouChickenOut: {sent:04X?}");
    assert!(
        ts.world.objects.get(sword).is_some() && ts.world.objects.get(bag).is_some(),
        "nothing is consumed"
    );
    assert!(
        !confirmation_open(&ts, alpha, ConfirmationType::CraftInteraction),
        "answered"
    );
}

/// A craft tool routes its special sources before the recipe manager.
#[test]
fn a_craft_tool_routes_its_special_sources_before_the_recipe_manager() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 100);
    let alpha = ObjectGuid::new(ALPHA);
    let spirit = give(&mut ts, 49485);
    let pet_device = give(&mut ts, PET_DEVICE);
    let mana_stone = give(&mut ts, 42645);
    let aetheria = give(&mut ts, 42635);
    let integrator = give(&mut ts, 42979);
    let sword = give(&mut ts, SWORD);
    ts.advance(0.1);

    let routed = |ts: &mut empyrean_testkit::TestServer, source: ObjectGuid, target: ObjectGuid| {
        let _ = empyrean_common::not_ported::take_local();
        handle_action_use_on_target(&mut ts.world, source, alpha, target);
        empyrean_common::not_ported::take_local()
            .into_keys()
            .collect::<Vec<_>>()
    };
    let (_, g) = exchange(&mut ts, id, session, 3.0, |ts| {
        let _ = routed(ts, spirit, pet_device);
    });
    assert_eq!(chats(&g), ["This essence is already full."]);
    assert!(ts.world.objects.get(spirit).is_some(), "the spirit is kept");
    let _ = routed(&mut ts, mana_stone, aetheria);
    assert!(
        empyrean_world::world_objects::player_magic::is_busy(&ts.world, alpha),
        "the clap"
    );
    ts.advance(3.0);
    assert_eq!(
        ts.world
            .objects
            .get(aetheria)
            .unwrap()
            .get_property(empyrean_entity::enums::PropertyString::Name)
            .as_deref(),
        Some("Aetheria")
    );
    ts.world
        .objects
        .get_mut(sword)
        .unwrap()
        .set_valid_locations(Some(empyrean_entity::enums::EquipMask::MeleeWeapon));
    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        let _ = routed(ts, integrator, sword);
    });
    assert_eq!(sent.last(), Some(&USE_DONE), "{sent:04X?}");
    assert!(chats(&g).is_empty());
    assert!(
        ts.world.objects.get(integrator).is_some(),
        "nothing is consumed"
    );
    // a mana stone on something that is not aetheria falls through to the recipe manager
    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        assert!(routed(ts, mana_stone, sword).is_empty())
    });
    assert_eq!(sent, [CHAT, USE_DONE], "{sent:04X?}");
    assert_eq!(
        chats(&g),
        ["The Aetheria Mana Stone cannot be used on the Sword."]
    );

    // ActOnUse: "Do nothing"
    let (sent, _) = exchange(&mut ts, id, session, 0.2, |ts| {
        let _ = empyrean_common::not_ported::take_local();
        empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, spirit, alpha);
        assert!(empyrean_common::not_ported::take_local().is_empty());
    });
    assert!(sent.is_empty(), "{sent:04X?}");
}

/// `Gem.HandleActionUseOnTarget`: a tailoring kit goes to `Tailoring.UseObjectOnTarget`, whose
/// `VerifyUseRequirements` refuses the kit on itself with UseDone(YouDoNotPassCraftingRequirements)
/// (the recipe manager would have answered with its chat and transient lines instead).
#[test]
fn a_gem_tailoring_kit_goes_to_tailoring() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 100);
    let alpha = ObjectGuid::new(ALPHA);
    let kit = give(&mut ts, 51445);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        handle_action_use_on_target(&mut ts.world, kit, alpha, kit)
    });
    assert_eq!(sent, [USE_DONE], "{sent:04X?}");
    let done: ItemUseDone = first(&g, USE_DONE).decode();
    assert_eq!(
        done.failure_type,
        WeenieError::YouDoNotPassCraftingRequirements
            .0
            .cast_unsigned()
    );
}

/// A rare gem with ConfirmUseOfRareGems set: `Gem.ActOnUse` asks through
/// `ConfirmationManager.EnqueueSend(Confirmation_Custom)` (filed as Yes_No); the answer yes over the
/// wire runs the custom action, `ActOnUse(activator, confirmed: true)`: the rare's local broadcast.
/// A second use while the question is open answers ConfirmationInProgress.
#[test]
fn a_rare_gem_is_confirmed_through_confirmation_response() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 100);
    let alpha = ObjectGuid::new(ALPHA);
    let gem = give(&mut ts, RARE_GEM);
    empyrean_world::world_objects::player_character::set_character_option(
        &mut ts.world,
        alpha,
        CharacterOption::ConfirmUseOfRareGems,
        true,
    );
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, gem, alpha)
    });
    assert_eq!(sent, [CONFIRM], "{sent:04X?}");
    let ask: CharacterConfirmationRequest = first(&g, CONFIRM).decode();
    assert_eq!(
        ask.confirmation_type,
        i32::try_from(ConfirmationType::Yes_No.0).unwrap()
    );
    assert_eq!(ask.text, "Are you sure you want to use Rare Gem?");
    assert!(confirmation_open(&ts, alpha, ConfirmationType::Yes_No));

    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, gem, alpha)
    });
    assert_eq!(sent, [0x028A], "ConfirmationInProgress: {sent:04X?}");
    let _ = g;

    let (_, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        ts.send_game_action(
            id,
            &CharacterConfirmationResponse {
                confirmation_type: ask.confirmation_type,
                context_id: ask.context_id,
                accepted: 1,
            },
        );
    });
    assert!(
        chats(&g).contains(&"Alpha used the rare item Rare Gem".to_owned()),
        "{:?}",
        chats(&g)
    );
    assert!(
        !confirmation_open(&ts, alpha, ConfirmationType::Yes_No),
        "answered"
    );
}
