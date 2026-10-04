//! The described player scale reaches the local animation and collision body.

use crate::common::sim_app::{app_with_recorded_body, body, frames};
use dereth_assets::{CharGen, Decode};
use dereth_client_net::client_session::{testing::Corpus, SessionEvent};
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_protocol::{
    objects::{ItemCreateObject, ItemUpdateObject},
    Message, Opcode,
};

/// Behaviour: objects.player.described-scale-reaches-body-and-paper-doll
#[test]
fn described_player_scale_reaches_animation_and_collision_and_returns_to_normal() {
    let mut app = app_with_recorded_body();
    let store = dereth_dat::testing::open_store_or_fail();
    let id = DataId(0x0e00_0002);
    let bytes = store.read_typed(CharGen::TYPE, id).unwrap();
    let tables = CharGen::decode_payload_in(store.era_of(id), id, &bytes).unwrap();
    let gear = &tables.heritage_groups[&6].sexes[&1];
    assert_eq!(gear.scale, 120, "the held Gear Knight scale is 120 percent");
    eprintln!("Gear Knight scale={} setup={:?}", gear.scale, gear.setup);
    let corpus = Corpus::load("early-inventory-and-casting")
        .unwrap()
        .unwrap();
    let player = app.objects().player().unwrap();
    let row = corpus
        .blobs
        .iter()
        .find(|row| {
            row.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && row.payload[4..8] == player.0.to_le_bytes()
        })
        .unwrap();
    let original =
        ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..])).unwrap();
    #[allow(clippy::cast_precision_loss)]
    let large = gear.scale as f32 / 100.0;
    for (index, scale) in [large, 1.0, large, 1.0].into_iter().enumerate() {
        let mut create = original.0.clone();
        create.physicsdesc.setup_id = Some(gear.setup.0);
        create.physicsdesc.mtable_id = Some(gear.motion_table.0);
        create.physicsdesc.object_scale = Some(scale);
        create.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::OBJSCALE;
        create.objdesc = Default::default();
        app.probe_mut().objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: if index == 0 {
                    Opcode::ITEM_UPDATE_OBJECT
                } else {
                    Opcode::ITEM_CREATE_OBJECT
                },
                body: dereth_protocol::write_body(&ItemUpdateObject(create)).unwrap(),
            },
            LocalTime(10.0 + f64::from(u32::try_from(index).unwrap())),
        );
        frames(&mut app, 1);
        let character = body(&app);
        assert_eq!(character.setup_id(), gear.setup);
        assert_eq!(character.scale, scale);
        assert_eq!(character.world.get(character.handle).unwrap().scale, scale);
        let driver = character.driver();
        assert_eq!(driver.scale, scale);
        assert_eq!(driver.part_array.scale, Vec3::new(scale, scale, scale));
        assert!(!driver.part_array.parts.is_empty());
        for (part_index, part) in driver.part_array.parts.iter().enumerate() {
            let base = driver
                .part_array
                .setup
                .as_ref()
                .unwrap()
                .default_scale
                .as_ref()
                .and_then(|scales| scales.get(part_index))
                .copied()
                .unwrap_or(Vec3::new(1.0, 1.0, 1.0));
            assert_eq!(
                part.gfxobj_scale,
                Vec3::new(base.x * scale, base.y * scale, base.z * scale)
            );
        }
    }
    app.shutdown();
}
