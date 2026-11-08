//! What the last release kept, held as its bytes: a character's play state with every field filled, and
//! the consignments' record with an item's own numbers, made with that release's own code and made-up
//! numbers. A later build reads them whole, gives the fields added since their empty values, and keeps
//! them again whole.

use yalain_store::StoredPosition;
use yalain_world::made::Made;
use yalain_world::market::{Consignment, Market};
use yalain_world::state::{
    CraftSkillState, EffectState, Held, ItemState, PlayState, QuestState, RecipeState, SkillState,
};
use yalain_world::things::TimedEffect;
use yalain_world::treasure::{Look, Own, OwnEffect};

/// A play state as Yalain 0.2.0 wrote it, at the encoding's version 18.
const PLAY: &str = concat!(
    "1200000009000000000000001900000002000000010000000000318101000047000000000000000025000000",
    "02000000000031810200004701000000100000000100000001000000010000400000001002000000e8030000",
    "000000004c0400000000000001000000840000000100000003000000050000000100000002000000bc020000",
    "000000000100000001000000010000000400007000000000a3e1a14100007042070000000000000000003242",
    "010000000800000000000000090000000a000000000000000300000004000000000000000050534001000000",
    "0100006f0200000003000000000000000400000000000000010000000200006f000000000000294002000000",
    "ffffffff030000000100000001000010000000000020564001000000840000000000000000a0504001000000",
    "020000000000318107000000050000004d616b657229000000010000010000803f0000004000004040010000",
    "00010000700000384101000000ff07000005000000090000000200007000000642020000250000403f030000",
    "00f40100001f0000000000803e110000000300000008000000020000000100000004000000030000005a0000",
    "00020000000600000001000000030000700000b441341200000000003ff4010000090000004f6c6420776f72",
    "6473050000004d616b65720600000001000020010000000000803e020000000000003f000000000000000001",
    "000000010000250300000080510100000000000100000007000000746865204f6c6407000000",
);

/// The consignments' record as Yalain 0.2.0 wrote it: its one sale's item at version 18.
const MARKET: &str = concat!(
    "020200000001000000010000000100000000003021010000000000f0400300006f0400006f02000000000031",
    "81020000470000c64201000000000000000000000000448f40e7000000120000000200000000003181070000",
    "00050000004d616b657229000000010000010000803f00000040000040400100000001000070000038410100",
    "0000ff07000005000000090000000200007000000642020000250000403f03000000f40100001f0000000000",
    "803e110000000300000008000000020000000100000004000000030000005a00000002000000060000000100",
    "0000030000700000b441341200000000003ff4010000090000004f6c6420776f726473050000004d616b6572",
    "0600000001000020010000000000803e020000000000003f00000000000000000100000001000025",
);

/// A play state as Yalain kept it at the encoding's version 20, with what version 18 lacks filled.
const PLAY_20: &str = concat!(
    "1400000009000000000000001900000002000000010000000000318101000047000000000000000025000000",
    "02000000000031810200004701000000100000000100000001000000010000400000001002000000e8030000",
    "000000004c0400000000000001000000840000000100000003000000050000000100000002000000bc020000",
    "000000000100000001000000010000000400007000000000a3e1a14100007042070000000000000000003242",
    "010000000800000000000000090000000a000000000000000300000004000000000000000050534001000000",
    "0100006f0200000003000000000000000400000000000000010000000200006f000000000000294002000000",
    "ffffffff030000000100000001000010000000000020564001000000840000000000000000a0504001000000",
    "020000000000318107000000050000004d616b657229000000010000010000803f0000004000004040010000",
    "00010000700000384101000000ff3f000005000000090000000200007000000642020000250000403f030000",
    "00f40100001f0000000000803e110000000300000008000000020000000100000004000000030000005a0000",
    "00020000000600000001000000030000700000b441341200000000003ff4010000090000004f6c6420776f72",
    "6473050000004d616b65720600000001000020010000000000803e020000000000003f000000000000000001",
    "0000000100002501000000000030210100000005000070000048410000008093dca441030000008051010000",
    "0000000100000007000000746865204f6c6407000000",
);

/// The consignments' record with its one sale's item at the encoding's version 20.
const MARKET_20: &str = concat!(
    "020200000001000000010000000100000000003021010000000000f0400300006f0400006f02000000000031",
    "81020000470000c64201000000000000000000000000448f4003010000140000000200000000003181070000",
    "00050000004d616b657229000000010000010000803f00000040000040400100000001000070000038410100",
    "0000ff3f000005000000090000000200007000000642020000250000403f03000000f40100001f0000000000",
    "803e110000000300000008000000020000000100000004000000030000005a00000002000000060000000100",
    "0000030000700000b441341200000000003ff4010000090000004f6c6420776f726473050000004d616b6572",
    "0600000001000020010000000000803e020000000000003f0000000000000000010000000100002501000000",
    "000030210100000005000070000048410000008093dca441",
);

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

fn item() -> ItemState {
    ItemState {
        id: 0x8131_0000_0000_0002,
        instance: 7,
        crafter: Some("Maker".into()),
        durability: Some(41),
        destination: Some(StoredPosition {
            cell: 0x0100_0001,
            origin: [1.0, 2.0, 3.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
        }),
        bonuses: vec![(0x7000_0001, 11.5)],
        binds_on_use: true,
        own: Own {
            min_level: Some(5),
            arcane_lore: Some(9),
            effect: Some(OwnEffect {
                state: 0x7000_0002,
                spellcraft: 33.5,
                table: 0x2500_0002,
                color: 0.75,
            }),
            made: Some(Made {
                prefix: 3,
                value: 500,
                damage: Some((31, 0.25)),
                armor: Some(17),
                delay: Some(3),
                vigor: Some(8),
                slots: 2,
                harmonic: true,
                covers: 4,
                damage_changed: true,
                vigor_changed: true,
                durability: Some(90),
                tool_yield: Some(2),
                craft_needed: Some(6),
            }),
            dye: Some((0x1234, 0.5)),
            craft_flags: 6,
            look: Some(Look {
                table: 0x2000_0001,
                values: [(1, 0.25), (2, 0.5), (0, 0.0)],
            }),
            hand: 1,
            suffix: Some(0x2500_0001),
            // Added since that release: bound to no one, not attuned.
            bound_to: 0,
            attuned: false,
        },
        enchantments: vec![(0x7000_0003, 22.5)],
        // Added since that release: no effect of a skill's on it.
        timed: Vec::new(),
        value: 500,
        inscription: Some(("Old words".into(), "Maker".into())),
    }
}

fn play() -> PlayState {
    PlayState {
        items: vec![
            Held {
                id: 0x8131_0000_0000_0001,
                template: 0x4700_0001,
                slot: 0,
                location: 0,
                quantity: 37,
            },
            Held {
                id: 0x8131_0000_0000_0002,
                template: 0x4700_0002,
                slot: 1,
                location: 16,
                quantity: 1,
            },
        ],
        quests: vec![QuestState {
            quest: 0x4000_0001,
            status: 0x1000_0000,
            phase: 2,
            started: 1000,
            updated: 1100,
        }],
        skills: vec![SkillState {
            skill: 132,
            granted: 1,
            mask: 3,
            cost: 5,
            children: 1,
            dependents: 2,
            spent: 700,
        }],
        money: 9,
        credits: 25,
        effects: vec![EffectState {
            eid: 1,
            state: 0x7000_0004,
            cast: 1.5e8,
            timeout: 60.0,
            caster: 7,
            spellcraft: 44.5,
            pk: true,
            acting: 8,
            skill: 9,
            item: 10,
            flags: 3,
            categories: 4,
        }],
        effect_counter: 1,
        craft_skills: vec![CraftSkillState {
            skill: 0x6F00_0001,
            level: 2,
            today: 3,
            available: 4,
        }],
        recipes: vec![RecipeState {
            recipe: 0x6F00_0002,
            use_again: 12.5,
            successes: 2,
            charges: -1,
            last_spinner: 3,
        }],
        craft_reset: 77.25,
        scenes: vec![0x1000_0001],
        last_used: vec![(132, 66.5)],
        kept_at: 88.5,
        item_states: vec![item()],
        hero_credits: 3,
        played_seconds: 86_400,
        cloaked: true,
        suffix: "the Old".into(),
        quest_rating: 7,
    }
}

/// An item with what was added since that release, as a later one keeps it.
fn item_20() -> ItemState {
    let old = item();
    ItemState {
        own: Own {
            bound_to: 0x2130_0000_0000_0001,
            attuned: true,
            ..old.own
        },
        timed: vec![TimedEffect {
            state: 0x7000_0005,
            spellcraft: 12.5,
            until: 1.75e8,
        }],
        ..old
    }
}

fn play_20() -> PlayState {
    PlayState {
        item_states: vec![item_20()],
        ..play()
    }
}

fn market_20() -> Market {
    Market {
        list: vec![Consignment {
            sale: 1,
            owner: 0x2130_0000_0000_0001,
            keeper: 0x40F0_0000_0000_0001,
            catalog: 0x6F00_0003,
            store: 0x6F00_0004,
            item: 0x8131_0000_0000_0002,
            template: 0x4700_0002,
            price: 99.0,
            offered: 1,
            sold: 0,
            entered: 1000.5,
        }],
        next_sale: 2,
        item_states: vec![item_20()],
        ..Market::default()
    }
}

/// A play state the last release kept reads whole, every field as it was and those added since empty,
/// and what this build keeps of it reads the same.
/// Behaviour: server.operator.a-save-of-the-last-release-reads-whole
#[test]
fn a_play_state_the_last_release_kept_reads_whole_and_is_kept_again_whole() {
    let old = bytes(PLAY);
    assert_eq!(
        old[..4],
        18u32.to_le_bytes(),
        "the bytes are of that release"
    );
    let read = PlayState::from_bytes(&old).expect("reads");
    assert_eq!(read, play());
    let kept = read.to_bytes();
    assert_ne!(kept, old, "kept at this build's own version");
    assert_eq!(PlayState::from_bytes(&kept).expect("reads again"), read);
}

/// The consignments' record the last release kept reads whole, its item's own numbers with it, and what
/// this build keeps of it reads the same.
/// Behaviour: server.operator.a-save-of-the-last-release-reads-whole
#[test]
fn a_consignments_record_the_last_release_kept_reads_whole_and_is_kept_again_whole() {
    let check = |m: &Market| {
        assert_eq!(m.next_sale, 2);
        assert_eq!(m.list.len(), 1);
        let c = m.list[0];
        assert_eq!(
            (c.sale, c.owner, c.keeper, c.catalog, c.store),
            (
                1,
                0x2130_0000_0000_0001,
                0x40F0_0000_0000_0001,
                0x6F00_0003,
                0x6F00_0004
            )
        );
        assert_eq!(
            (c.item, c.template, c.offered, c.sold),
            (0x8131_0000_0000_0002, 0x4700_0002, 1, 0)
        );
        assert_eq!((c.price, c.entered), (99.0, 1000.5));
        assert_eq!(m.item_states, [item()]);
    };
    let read = Market::from_bytes(&bytes(MARKET)).expect("reads");
    check(&read);
    check(&Market::from_bytes(&read.to_bytes()).expect("reads again"));
}

/// A play state kept at the version before an item's permanent effects stood apart from its numbers
/// reads whole, and what this build keeps of it reads the same.
/// Behaviour: server.operator.a-save-of-the-last-release-reads-whole
#[test]
fn a_play_state_kept_at_version_twenty_reads_whole_and_is_kept_again_whole() {
    let old = bytes(PLAY_20);
    assert_eq!(
        old[..4],
        20u32.to_le_bytes(),
        "the bytes are of that version"
    );
    let read = PlayState::from_bytes(&old).expect("reads");
    assert_eq!(read, play_20());
    let kept = read.to_bytes();
    assert_ne!(kept, old, "kept at this build's own version");
    assert_eq!(PlayState::from_bytes(&kept).expect("reads again"), read);
}

/// The consignments' record kept at that version reads whole, and what this build keeps of it reads
/// the same.
/// Behaviour: server.operator.a-save-of-the-last-release-reads-whole
#[test]
fn a_consignments_record_kept_at_version_twenty_reads_whole_and_is_kept_again_whole() {
    let read = Market::from_bytes(&bytes(MARKET_20)).expect("reads");
    assert_eq!(read.list, market_20().list);
    assert_eq!(read.item_states, [item_20()]);
    let again = Market::from_bytes(&read.to_bytes()).expect("reads again");
    assert_eq!(again.list, read.list);
    assert_eq!(again.item_states, read.item_states);
}

/// An item with no numbers of its own keeps its permanent effects in a play state and in the
/// consignments' record, and one with none reads with none.
/// Behaviour: server.craft.made-item-no-level-keeps-effects
#[test]
fn an_item_with_no_numbers_of_its_own_keeps_its_permanent_effects_where_it_is_kept() {
    let enchanted = ItemState {
        id: 0x8131_0000_0000_0003,
        enchantments: vec![(0x7000_0006, 1.0), (0x7000_0007, 2.5)],
        ..ItemState::default()
    };
    let plain = ItemState {
        id: 0x8131_0000_0000_0004,
        ..ItemState::default()
    };
    assert_eq!(
        ItemState::from_bytes(&enchanted.to_bytes()).expect("reads"),
        enchanted
    );
    assert_eq!(
        ItemState::from_bytes(&plain.to_bytes()).expect("reads"),
        plain
    );
    let s = PlayState {
        item_states: vec![enchanted.clone(), plain.clone(), item_20()],
        ..PlayState::default()
    };
    assert_eq!(PlayState::from_bytes(&s.to_bytes()).expect("reads"), s);
    // The record keeps the state of an item on sale, by its id.
    let m = Market {
        item_states: vec![ItemState {
            id: 0x8131_0000_0000_0002,
            ..enchanted
        }],
        ..market_20()
    };
    assert_eq!(
        Market::from_bytes(&m.to_bytes())
            .expect("reads")
            .item_states,
        m.item_states
    );
}
