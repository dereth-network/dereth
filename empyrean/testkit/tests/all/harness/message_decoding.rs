//! Vectors: synthetic plain-message and game-event byte fixtures defined in this module.
//! Message decoding through game messages.
//! Fixture: locally constructed message bytes and the shared decoder helpers.

mod messages {
    //! Vectors: synthetic plain-message and game-event byte fixtures defined in this module.
    use crate::support::real_content_bot::decode_table::*;

    #[test]
    fn a_plain_message_and_a_game_event_decode_and_are_found_by_type() {
        let teleport = incoming(
            &proto::write_blob(&EffectsPlayerTeleport {
                teleport_sequence: 7,
            })
            .expect("encodable"),
        );
        let wear = incoming(
            &pack_event(
                ObjectId(0x5000_0001),
                3,
                &ItemWearItem {
                    item: ObjectId(0x8000_0001),
                    slot: 0x10_0000,
                },
            )
            .expect("encodable"),
        );
        let d = decode::decode(&teleport);
        assert_eq!(
            (d.kind, d.event, d.result),
            (0xF751, false, Ok("EffectsPlayerTeleport"))
        );
        let d = decode::decode(&wear);
        assert_eq!(
            (d.kind, d.event, d.result.clone()),
            (0x0023, true, Ok("ItemWearItem"))
        );
        assert_eq!(d.name(), "Item_WearItem");
        let both = [teleport, wear];
        assert!(decode::undecoded(&both).is_empty());
        assert_eq!(
            decode::all_of::<ItemWearItem>(&both),
            [ItemWearItem {
                item: ObjectId(0x8000_0001),
                slot: 0x10_0000
            }]
        );
        assert_eq!(decode::all_of::<EffectsPlayerTeleport>(&both).len(), 1);
    }

    #[test]
    fn an_unknown_opcode_or_trailing_bytes_do_not_decode() {
        let unknown = incoming(&0x0000_7777u32.to_le_bytes());
        assert_eq!(
            decode::decode(&unknown).result,
            Err("no dereth-protocol decoder".to_owned())
        );

        let mut long = proto::write_blob(&EffectsPlayerTeleport {
            teleport_sequence: 7,
        })
        .expect("encodable");
        long.extend_from_slice(&[1, 2, 3, 4]);
        let d = decode::decode(&incoming(&long));
        assert!(
            d.result
                .as_ref()
                .is_err_and(|e| e.contains("EffectsPlayerTeleport")),
            "{d:?}"
        );
        assert_eq!(EffectsPlayerTeleport::OPCODE.0, d.kind);
    }
}
