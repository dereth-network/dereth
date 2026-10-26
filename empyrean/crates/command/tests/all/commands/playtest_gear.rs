//! Divergence: V453
//! A playtest character's gear, kit and packs, made against the retail data files and the world
//! database: each slot rolled on its own, better gear, chosen items, starter gear, ammunition worn
//! ready, what its packs and strength can carry, and a preview of it all that an apply makes.

#[cfg(feature = "real-content")]
mod real {
    use dereth_progression::gear::{Slot, AMMUNITION, ARMOR_LOCATIONS, ONE_HANDED};
    use dereth_progression::order::{Gear, Item, Made, MadeItem, Order, SlotOrder, ORDER_VERSION};
    use dereth_progression::Archetype;
    use empyrean_command::empyrean::playtest_character::{apply_recipe, preview, tier_for_level};
    use empyrean_content::PackContent;
    use empyrean_entity::enums::{PropertyInstanceId, PropertyInt, PropertyString, WeenieType};
    use empyrean_entity::Biota;
    use empyrean_testkit::{EmptyShard, TestServer};
    use empyrean_world::entity::i_player::IPlayer;
    use empyrean_world::entity::offline_player::OfflinePlayer;
    use empyrean_world::managers::guid_manager;
    use empyrean_world::managers::player_manager::OrdinalIgnoreCase;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    const ID: u32 = 0x5000_0042;

    /// A server over the retail dats and the world database's content pack.
    fn server() -> TestServer {
        let dir = dereth_dat::testing::dat_dir();
        let source = empyrean_dat::RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let path = empyrean_common::test_paths::world_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        server_of(source, pack)
    }

    /// A server over the February 2005 dats and the Infiltration world's content pack.
    fn infiltration_server() -> TestServer {
        if let Some(msg) = dereth_dat::testing::classic_shortfall() {
            panic!("{msg}");
        }
        let dir = dereth_dat::testing::classic_dat_dir().unwrap_or_default();
        let source = empyrean_dat::RealDats::open(&dir)
            .unwrap_or_else(|e| panic!("the February 2005 dats under {}: {e}", dir.display()));
        let path = empyrean_common::test_paths::infiltration_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "these tests need an Infiltration world.pack at {}                  (EMPYREAN_TEST_INFILTRATION_PACK; `empyrean-import fetch --world 16py --pack                  --out <it>`): {e}",
                path.display()
            )
        });
        server_of(source, pack)
    }

    fn server_of(source: empyrean_dat::RealDats, pack: PackContent) -> TestServer {
        let dats = empyrean_dat::DatManager::initialize(Arc::new(source)).expect("the dats");
        TestServer::with_setup(dats, |w| {
            w.content = Arc::new(pack);
            w.era = empyrean_common::era::EraExt::rules(w.content.era());
            guid_manager::initialize(w, &mut EmptyShard);
        })
    }

    /// An Aluvian of level 1, offline and saved, as the client's creation leaves one.
    fn character(ts: &mut TestServer, name: &str) {
        let mut biota = Biota {
            id: ID,
            weenie_class_id: 1,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        biota.set_property(PropertyString::Name, name.to_owned());
        biota.set_property(PropertyInt::HeritageGroup, 1);
        biota.set_property(PropertyInt::Level, 1);
        assert!(ts.shard().save_biota(&mut biota.clone(), false));
        let offline = {
            let mut shard = ts.shard();
            let mut auth = ts.auth();
            OfflinePlayer::new(biota, &mut **shard, &mut **auth)
        };
        let guid = offline.guid;
        ts.world.player_manager.offline_players.insert(ID, offline);
        ts.world.player_manager.player_names.insert(
            OrdinalIgnoreCase(name.trim_start_matches('+').to_owned()),
            IPlayer::Offline(guid),
        );
    }

    fn order(archetype: &str, level: u32, gear: Gear) -> Order {
        Order {
            version: ORDER_VERSION,
            hash: format!("{archetype}-{level}"),
            level,
            plan: Archetype::named(archetype).unwrap().plan,
            heritage: None,
            gender: None,
            gear,
            seed: 11,
            items: Vec::new(),
            place_tier: None,
            slots: BTreeMap::new(),
            extras: dereth_progression::Extras::default(),
        }
    }

    /// A scratch folder of this process's own.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let mut serial = 0;
        loop {
            let p = std::env::temp_dir().join(format!("{tag}-{}-{serial}", std::process::id()));
            match std::fs::create_dir(&p) {
                Ok(()) => return p,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => serial += 1,
                Err(e) => panic!("{e}"),
            }
        }
    }

    /// Apply `o` to the character, the `n`th time.
    fn apply(
        ts: &mut TestServer,
        o: &Order,
        dir: &std::path::Path,
        n: u32,
    ) -> Result<Made, String> {
        let input = dir.join(format!("recipe-{n}.json"));
        std::fs::write(&input, serde_json::to_vec(o).unwrap()).unwrap();
        apply_recipe(
            &mut ts.world,
            "+Geared",
            &input,
            Some(&dir.join(format!("backup-{n}.sqlite"))),
            &dir.join("made.json"),
            true,
        )
    }

    /// What `o` would make, asked the `n`th time.
    fn previewed(
        ts: &mut TestServer,
        o: &Order,
        dir: &std::path::Path,
        n: u32,
    ) -> Result<Made, String> {
        let input = dir.join(format!("preview-{n}.json"));
        std::fs::write(&input, serde_json::to_vec(o).unwrap()).unwrap();
        preview(
            &mut ts.world,
            &input,
            &dir.join(format!("previewed-{n}.json")),
        )
    }

    /// What a made character holds, as the server says it: each item's slot, class, name, place
    /// and count, in its order.
    fn held(m: &Made) -> Vec<(Option<Slot>, u32, String, u32, u32)> {
        m.items
            .iter()
            .map(|i| (i.gear, i.class, i.name.clone(), i.slot, i.count))
            .collect()
    }

    #[test]
    fn a_preview_rolls_and_makes_what_an_apply_of_the_recipe_makes_and_stores_nothing() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-preview");
        let mut o = order("melee-heavy", 80, Gear::Better);
        o.items = vec![Item {
            class: 258,
            count: 20,
        }];
        let shown = previewed(&mut ts, &o, &dir, 0).unwrap();
        assert_eq!(
            (shown.id, shown.name.as_str()),
            (0, ""),
            "no character named"
        );
        assert!(possessions(&mut ts).is_empty(), "nothing stored");
        let read: Made =
            serde_json::from_slice(&std::fs::read(dir.join("previewed-0.json")).unwrap()).unwrap();
        assert_eq!(read, shown);
        let made = apply(&mut ts, &o, &dir, 0).unwrap();
        println!("{made:#?}");
        assert_eq!(held(&shown), held(&made));
        assert_eq!(shown.tier, made.tier);
        // Its weapon rolled again: the preview shows the new one, and the apply makes it.
        o.slots.insert(
            Slot::Weapon,
            SlotOrder {
                roll: 1,
                class: None,
            },
        );
        let again = previewed(&mut ts, &o, &dir, 1).unwrap();
        assert_ne!(
            by_slot(&again)[&Slot::Weapon].class,
            by_slot(&shown)[&Slot::Weapon].class
        );
        let before: Vec<u32> = possessions(&mut ts).iter().map(|b| b.id).collect();
        assert!(!before.is_empty());
        previewed(&mut ts, &o, &dir, 2).unwrap();
        let after: Vec<u32> = possessions(&mut ts).iter().map(|b| b.id).collect();
        assert_eq!(after, before, "a preview leaves the character as it was");
        assert_eq!(held(&again), held(&apply(&mut ts, &o, &dir, 1).unwrap()));
        // What an apply refuses, a preview refuses too.
        let mut heavy = o.clone();
        heavy.items = vec![Item {
            class: 258,
            count: 10_000,
        }];
        assert!(previewed(&mut ts, &heavy, &dir, 3).is_err());
        assert!(apply(&mut ts, &heavy, &dir, 3).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// What the character wears and carries now, as stored.
    fn possessions(ts: &mut TestServer) -> Vec<Biota> {
        let mut db = ts.shard();
        let p = db.get_possessed_biotas_in_parallel(ID);
        p.inventory
            .iter()
            .chain(&p.wielded_items)
            .map(|b| empyrean_store::adapter::BiotaConverter::convert_to_entity_biota(b, false))
            .collect()
    }

    /// The worn items by the slot each fills.
    fn by_slot(m: &Made) -> BTreeMap<Slot, &MadeItem> {
        m.items
            .iter()
            .filter_map(|i| i.gear.map(|g| (g, i)))
            .collect()
    }

    /// No two worn items share a place.
    fn apart(m: &Made) {
        let mut worn = 0;
        for i in m.items.iter().filter(|i| i.slot != 0) {
            assert_eq!(worn & i.slot, 0, "{} shares a place: {m:?}", i.name);
            worn |= i.slot;
        }
    }

    #[test]
    fn a_level_80_melee_wields_a_weapon_and_a_shield_and_wears_a_suit_that_covers_each_slot_once() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-suit");
        let made = apply(&mut ts, &order("melee-heavy", 80, Gear::Level), &dir, 0).unwrap();
        println!("{made:#?}");
        apart(&made);
        let slots = by_slot(&made);
        assert_eq!(slots[&Slot::Weapon].slot, ONE_HANDED, "{made:?}");
        assert_eq!(
            slots[&Slot::Shield].slot,
            Slot::Shield.mask(),
            "it trains Shield: {made:?}"
        );
        let suit: u32 = made
            .items
            .iter()
            .filter(|i| i.slot & ARMOR_LOCATIONS != 0)
            .fold(0, |m, i| m | i.slot);
        assert!(suit.count_ones() >= 7, "{suit:#x}: {made:?}");
        assert_eq!(made.tier, tier_for_level(&ts.world, 80));
        // What the server stored is what it says it made: each wielded by the character.
        let stored = possessions(&mut ts);
        assert_eq!(
            stored
                .iter()
                .filter(|b| b.get_property(PropertyInstanceId::Wielder) == Some(ID))
                .count(),
            made.items.iter().filter(|i| i.slot != 0).count()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rolling_one_slot_again_changes_its_item_and_leaves_the_rest_as_they_were() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-reroll");
        let o = order("melee-heavy", 80, Gear::Level);
        let first = apply(&mut ts, &o, &dir, 0).unwrap();
        let again = apply(&mut ts, &o, &dir, 1).unwrap();
        let look = |m: &Made| -> BTreeMap<Slot, (u32, String)> {
            by_slot(m)
                .into_iter()
                .map(|(s, i)| (s, (i.class, i.name.clone())))
                .collect()
        };
        assert_eq!(
            look(&first),
            look(&again),
            "the same seed rolls the same gear"
        );
        let mut rolled = o.clone();
        rolled.slots.insert(
            Slot::Weapon,
            SlotOrder {
                roll: 1,
                class: None,
            },
        );
        let other = apply(&mut ts, &rolled, &dir, 2).unwrap();
        let (a, b) = (look(&first), look(&other));
        assert_ne!(a[&Slot::Weapon], b[&Slot::Weapon], "{a:?}\n{b:?}");
        for slot in a.keys().filter(|s| **s != Slot::Weapon) {
            assert_eq!(a.get(slot), b.get(slot), "{slot:?}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn gear_for_its_level_is_never_below_its_level_and_takes_the_places_tier_when_higher() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-tier");
        let own = tier_for_level(&ts.world, 20);
        let mut o = order("melee-heavy", 20, Gear::Level);
        o.place_tier = Some((own + 2).min(8));
        assert_eq!(apply(&mut ts, &o, &dir, 0).unwrap().tier, (own + 2).min(8));
        let high = tier_for_level(&ts.world, 200);
        let mut o = order("melee-heavy", 200, Gear::Level);
        o.place_tier = Some(1);
        assert_eq!(apply(&mut ts, &o, &dir, 1).unwrap().tier, high);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn better_gear_is_a_tier_up_and_adds_jewelry_and_a_cloak() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-better");
        let made = apply(&mut ts, &order("war-mage", 200, Gear::Better), &dir, 0).unwrap();
        println!("{made:#?}");
        apart(&made);
        assert_eq!(made.tier, (tier_for_level(&ts.world, 200) + 1).min(8));
        let slots = by_slot(&made);
        let jewels = Slot::JEWELRY
            .iter()
            .filter(|s| slots.contains_key(s))
            .count();
        assert!(jewels >= 4, "{made:?}");
        assert!(slots.contains_key(&Slot::Cloak), "{made:?}");
        assert_eq!(slots[&Slot::Weapon].slot, 0x0100_0000, "a caster: {made:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_chosen_item_fills_its_slot_and_one_that_cannot_go_there_or_be_worn_is_refused() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-chosen");
        const TACHI: u32 = 353;
        let mut o = order("melee-heavy", 80, Gear::Level);
        o.slots.insert(
            Slot::Weapon,
            SlotOrder {
                roll: 0,
                class: Some(TACHI),
            },
        );
        let made = apply(&mut ts, &o, &dir, 0).unwrap();
        let slots = by_slot(&made);
        assert_eq!(
            (slots[&Slot::Weapon].class, slots[&Slot::Weapon].slot),
            (TACHI, ONE_HANDED)
        );
        // A healing kit is not worn on the head.
        let mut kit = o.clone();
        kit.slots.insert(
            Slot::Head,
            SlotOrder {
                roll: 0,
                class: Some(629),
            },
        );
        let why = apply(&mut ts, &kit, &dir, 1).unwrap_err();
        assert!(why.contains("cannot be worn as the head"), "{why}");
        // A trinket for level 180 and up, on a level 80 character.
        let mut trinket = o.clone();
        trinket.slots.insert(
            Slot::Trinket,
            SlotOrder {
                roll: 0,
                class: Some(52036),
            },
        );
        let why = apply(&mut ts, &trinket, &dir, 2).unwrap_err();
        assert!(why.contains("level 180"), "{why}");
        assert!(!dir.join("backup-2.sqlite").exists(), "nothing was changed");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn starter_gear_is_what_creation_gives_the_skills_it_trains_carried_not_worn() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-starter");
        let made = apply(&mut ts, &order("melee-heavy", 1, Gear::Starter), &dir, 0).unwrap();
        assert_eq!(made.tier, 0);
        assert!(made.items.iter().all(|i| i.slot == 0), "{made:?}");
        let classes: Vec<u32> = made.items.iter().map(|i| i.class).collect();
        // The Aluvian's Heavy Weapons item, and the Healing kit.
        assert!(classes.contains(&12739), "{classes:?}");
        assert!(classes.contains(&628), "{classes:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_archers_arrows_are_worn_ready_for_its_bow_and_the_rest_are_packed() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-arrows");
        let mut o = order("archer", 50, Gear::Level);
        o.items = vec![
            Item {
                class: 300,
                count: 1500,
            },
            Item {
                class: 629,
                count: 5,
            },
        ];
        let made = apply(&mut ts, &o, &dir, 0).unwrap();
        println!("{made:#?}");
        let ammo: Vec<&MadeItem> = made.items.iter().filter(|i| i.slot == AMMUNITION).collect();
        assert_eq!(ammo.len(), 1, "{made:?}");
        assert_eq!((ammo[0].class, ammo[0].count), (300, 1000), "a whole stack");
        let packed: u32 = made
            .items
            .iter()
            .filter(|i| i.slot == 0 && i.class == 300)
            .map(|i| i.count)
            .sum();
        assert_eq!(packed, 500, "the rest of the arrows");
        assert_eq!(
            made.items.iter().filter(|i| i.class == 629).count(),
            5,
            "kits do not stack"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn what_its_packs_cannot_hold_or_it_could_not_lift_is_refused_and_side_packs_add_room() {
        let mut ts = server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-packs");
        const KIT: u32 = 629;
        const PACK: u32 = 136;
        // A sign weighs 9,000; a dinner table 70,000.
        const SIGN: u32 = 459;
        const TABLE: u32 = 277;
        let with = |items: Vec<Item>| Order {
            items,
            ..order("melee-heavy", 1, Gear::None)
        };
        // A level 1 character of strength 100 carries 15,000 at full speed, and could lift 45,000.
        let lift = with(vec![Item {
            class: TABLE,
            count: 1,
        }]);
        let why = apply(&mut ts, &lift, &dir, 0).unwrap_err();
        assert!(why.contains("more than three times"), "{why}");
        let slow = with(vec![Item {
            class: SIGN,
            count: 2,
        }]);
        let made = apply(&mut ts, &slow, &dir, 1).unwrap();
        assert!(made.burden > made.capacity, "slowed, not refused: {made:?}");
        // It keeps the two signs: 110 kits more do not fit the main pack, but do with a side pack.
        let full = with(vec![Item {
            class: KIT,
            count: 110,
        }]);
        let why = apply(&mut ts, &full, &dir, 2).unwrap_err();
        assert!(why.contains("more than its packs hold"), "{why}");
        let packs = with(vec![
            Item {
                class: PACK,
                count: 1,
            },
            Item {
                class: KIT,
                count: 110,
            },
        ]);
        apply(&mut ts, &packs, &dir, 3).unwrap();
        let stored = possessions(&mut ts);
        let pack = stored
            .iter()
            .find(|b| b.weenie_class_id == PACK)
            .expect("the side pack")
            .id;
        let in_pack = stored
            .iter()
            .filter(|b| b.get_property(PropertyInstanceId::Container) == Some(pack))
            .count();
        assert_eq!(in_pack, 2 + 110 - 102, "the main pack holds 102");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn in_february_2005_a_sword_and_board_melee_carries_a_shield_without_a_shield_skill_and_no_cloak(
    ) {
        let mut ts = infiltration_server();
        character(&mut ts, "+Geared");
        let dir = scratch("playtest-gear-infiltration");
        let made = apply(&mut ts, &order("melee-heavy", 50, Gear::Better), &dir, 0).unwrap();
        println!("{made:#?}");
        apart(&made);
        let slots = by_slot(&made);
        assert_eq!(slots[&Slot::Weapon].slot, ONE_HANDED, "{made:?}");
        assert!(
            slots[&Slot::Weapon].wield.is_empty() || slots[&Slot::Weapon].wield.contains("Sword"),
            "a sword: {made:?}"
        );
        assert!(
            slots.contains_key(&Slot::Shield),
            "anyone carried a shield: {made:?}"
        );
        assert!(!slots.contains_key(&Slot::Cloak), "no cloaks yet: {made:?}");
        let suit = made
            .items
            .iter()
            .filter(|i| i.slot & ARMOR_LOCATIONS != 0)
            .count();
        assert!(suit >= 3, "{made:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
