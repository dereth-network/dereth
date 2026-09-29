//! ACE: Source/ACE.Server/WorldObjects/Player_AllowedSpellID.cs::PlayerSpellTable
//! Every allowed player spell matches ACE's complete ordered table.
//! Fixture: the ordered ids from the pinned ACE source, stored in tests/fixtures/player_spell_table.json.

mod spells {
    #[test]
    fn the_player_spell_table_is_aces() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/player_spell_table.json"))
                .expect("the ACE spell-id fixture");
        let expected: Vec<u32> =
            serde_json::from_value(fixture["spell_ids"].clone()).expect("the ordered spell ids");
        let actual = &empyrean_world::world_objects::player_allowed_spell_id::PLAYER_SPELL_TABLE;
        assert_eq!(
            actual.as_slice(),
            expected.as_slice(),
            "the complete ACE spell-id order"
        );
    }
}
