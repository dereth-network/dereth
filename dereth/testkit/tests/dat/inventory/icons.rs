use dereth_primitives::{DataId, ObjectId};
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const OWNER: ObjectId = ObjectId(0x5200_0001);
const AWARDED: ObjectId = ObjectId(0x5200_0010);
const AT_LOGIN: ObjectId = ObjectId(0x5200_0011);
/// Two pictures the shipped art really carries, and two different ones, so a slot drawing one
/// of them cannot have got it from the other.
const AWARD_ICON: DataId = DataId(0x0600_1F19);
const LOGIN_ICON: DataId = DataId(0x0600_2371);

fn a_client_with_a_pack() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        w.player = Some(OWNER);
        let mut me = dereth_client_model::Weenie::new(OWNER);
        me.valid = true;
        me.pwd.name = "Alba".into();
        me.pwd.items_capacity = Some(24);
        me.pwd.containers_capacity = Some(7);
        me.pwd.bitfield |=
            dereth_rules::weenie::bitfield::PLAYER | dereth_rules::weenie::bitfield::OPENABLE;
        w.tables.weenies.insert(OWNER, me);
        w.tables.inventories.insert(
            OWNER,
            dereth_client_model::objects::ObjectInventory::new(OWNER),
        );
        w.remake_character_inventory();
    }
    super::open_pack(&mut c);
    c.tick(2);
    c
}

/// The shard putting a thing in the player's pack before it has said what the thing is.
fn put_in_the_pack(item: ObjectId, slot: u32) -> Inbound {
    Inbound::message(&dereth_protocol::objects::ItemServerSaysContainId {
        item,
        container: OWNER,
        slot,
        container_properties: 0,
    })
}

/// The shard finally describing it.
fn described(
    item: ObjectId,
    name: &str,
    icon: DataId,
) -> dereth_protocol::objects::ItemCreateObject {
    dereth_protocol::objects::ItemCreateObject(dereth_protocol::objects::ObjectCreatePayload {
        id: item,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: dereth_protocol::types::PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(0x0200_0001),
            ..dereth_protocol::types::PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc {
            header: dereth_protocol::types::weeniedesc::header::CONTAINER_ID,
            name: name.into(),
            icon_id: icon.0,
            container_id: Some(OWNER),
            ..dereth_protocol::types::PublicWeenieDesc::default()
        },
    })
}

/// The picture the slot holding `item` is drawing, and whether any slot holds it.
fn drawn(c: &mut HeadlessClient, item: ObjectId) -> (bool, Option<DataId>) {
    let (ui, screen) = super::parts(c.app_mut());
    let p: &GamePlayScreen = &*screen;
    let Some(s) = p
        .inventory
        .top_container
        .iter()
        .chain(p.inventory.container_list.iter())
        .chain(p.inventory.item_list.iter())
        .flat_map(|w| w.slots.iter())
        .find(|s| s.item == Some(item))
    else {
        return (false, None);
    };
    let icon = match s.icon_recipe(&*ui) {
        Some(dereth_ui::region::IconRecipe::Object { icon, .. }) => icon,
        _ => None,
    };
    (true, icon)
}

fn pack_order(c: &HeadlessClient) -> Vec<ObjectId> {
    c.view()
        .world()
        .inventory(OWNER)
        .map(|i| i.items.clone())
        .unwrap_or_default()
}

/// A thing that arrives in the pack before its description draws its picture the moment the
/// description lands, and the description moves nothing in the pack when it does.
///
/// Two arms, because a comparison is satisfied just as well by its subject being absent from
/// both: the awarded thing and a thing that was already in the pack are carried side by side,
/// each drawing its own picture and neither drawing the other's.
pub fn an_awarded_thing_draws_its_picture_when_its_description_lands() {
    let mut c = a_client_with_a_pack();
    c.when(put_in_the_pack(AT_LOGIN, 0)).tick(2);
    c.when(put_in_the_pack(AWARDED, 1)).tick(2);

    let before = (drawn(&mut c, AT_LOGIN), drawn(&mut c, AWARDED));
    let waiting = before.0 == (true, None) && before.1 == (true, None);
    let order_before = pack_order(&c);
    let both_there = order_before.contains(&AT_LOGIN) && order_before.contains(&AWARDED);

    // The first description. One frame, and the slot must be right on it.
    c.when(Inbound::world_view(&described(
        AT_LOGIN,
        "Oil of Rendering",
        LOGIN_ICON,
    )))
    .tick(1);
    let login_arm = drawn(&mut c, AT_LOGIN) == (true, Some(LOGIN_ICON))
        && drawn(&mut c, AWARDED) == (true, None);
    let nothing_moved = pack_order(&c) == order_before;

    c.when(Inbound::world_view(&described(
        AWARDED,
        "Academy Coat",
        AWARD_ICON,
    )))
    .tick(1);
    let award_arm = drawn(&mut c, AWARDED) == (true, Some(AWARD_ICON))
        && drawn(&mut c, AT_LOGIN) == (true, Some(LOGIN_ICON));
    let still_nothing_moved = pack_order(&c) == order_before;

    // …and the pictures they name are art the game really ships.
    let real_art = {
        use dereth_primitives::AssetSource as _;
        let store = c
            .dat_store()
            .expect("a client over the retail data")
            .clone();
        [AWARD_ICON, LOGIN_ICON].into_iter().all(|id| {
            let bytes = store
                .read(id)
                .unwrap_or_else(|e| panic!("{id:?} is not in the data: {e}"));
            let sfc =
                <dereth_assets::RenderSurface as dereth_assets::Decode>::decode_payload(id, &bytes)
                    .unwrap_or_else(|e| panic!("{id:?} does not decode: {e}"));
            (sfc.width, sfc.height) == (32, 32) && sfc.image_size > 0
        })
    };

    c.assert_behaviour(
            "inventory.pack.a-thing-that-reaches-the-pack-before-it-is-described-draws-its-picture-the-moment-the-description-lands",
            move |_| {
                waiting
                    && both_there
                    && login_arm
                    && award_arm
                    && nothing_moved
                    && still_nothing_moved
                    && real_art
            },
        );
    c.shutdown();
}

/// The shard describing again what the client already has leaves the pack in the order it
/// was in, and the second description really did arrive.
pub fn describing_a_thing_again_leaves_the_pack_in_order() {
    let mut c = a_client_with_a_pack();
    c.when(put_in_the_pack(AT_LOGIN, 0)).tick(1);
    c.when(put_in_the_pack(AWARDED, 1)).tick(1);
    c.when(Inbound::world_view(&described(
        AT_LOGIN,
        "Oil of Rendering",
        LOGIN_ICON,
    )))
    .tick(1);
    c.when(Inbound::world_view(&described(
        AWARDED,
        "Academy Coat",
        AWARD_ICON,
    )))
    .tick(1);

    let before = pack_order(&c);
    let two_things = before.len() == 2;
    let merges_before = c.view().objects().stats.merges;

    // …and again, byte for byte, which is what asking the shard to resend everything is.
    c.when(Inbound::world_view(&described(
        AT_LOGIN,
        "Oil of Rendering",
        LOGIN_ICON,
    )))
    .tick(1);
    c.when(Inbound::world_view(&described(
        AWARDED,
        "Academy Coat",
        AWARD_ICON,
    )))
    .tick(1);

    // The premise, because "nothing changed" is satisfied just as well by nothing happening.
    let arrived = c.view().objects().stats.merges - merges_before == 2;
    let unmoved = pack_order(&c) == before;
    let still_drawn = drawn(&mut c, AWARDED) == (true, Some(AWARD_ICON))
        && drawn(&mut c, AT_LOGIN) == (true, Some(LOGIN_ICON));

    c.assert_behaviour(
            "inventory.pack.describing-a-thing-the-client-already-has-leaves-the-pack-in-the-order-it-was-in",
            move |_| two_things && arrived && unmoved && still_drawn,
        );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_an_awarded_thing_draws_its_picture_when_its_description_lands => an_awarded_thing_draws_its_picture_when_its_description_lands ["inventory.pack.a-thing-that-reaches-the-pack-before-it-is-described-draws-its-picture-the-moment-the-description-lands"],
    scenario_describing_a_thing_again_leaves_the_pack_in_order => describing_a_thing_again_leaves_the_pack_in_order ["inventory.pack.describing-a-thing-the-client-already-has-leaves-the-pack-in-the-order-it-was-in"],
}
