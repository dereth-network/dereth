//! Vectors: synthetic gem-table expectations defined in this module.
//! Gem tables initialise once when the world first uses them.

mod table_initialization {
    //! Vectors: synthetic gem-table expectations defined in this module.
    use crate::harness::world_loop_basics::*;

    /// The gem count table is the worlds built on first use.
    #[test]
    fn the_gem_count_table_is_the_worlds_built_on_first_use() {
        use empyrean_content::models::world::TreasureGemCount;
        use empyrean_world::factories::loot_generation_factory::tables_logic::tables::gem_count_chance;
        let mut installed = TestServer::new();
        gem_count_chance::gem_count_chance(
            &mut installed.world,
            &[TreasureGemCount {
                id: 1,
                gem_code: 7,
                tier: 1,
                count: 4,
                chance: 1.0,
            }],
        );
        assert_eq!(gem_count_chance::roll(&installed.world, 7, 1), 4);

        let ts = TestServer::new();
        assert!(
            !gem_count_chance::is_initialized(&ts.world),
            "not built before a roll, nor shared"
        );
        // The empty synthetic world database has no rows: every gem count rolls 0, with no draw.
        assert_eq!(gem_count_chance::roll(&ts.world, 7, 1), 0);
        assert!(gem_count_chance::is_initialized(&ts.world));
    }
}
