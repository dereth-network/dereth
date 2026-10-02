// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/StarterGearFactory.cs
// @generated from ClassicACE's `Source/ACE.Server/starterGear.infiltration.json`; do not edit by hand
//! ClassicACE's `Source/ACE.Server/starterGear.infiltration.json`,
//! as `StarterGearFactory` deserializes it, with System.Text.Json's rules. Entries are in file
//! order, which is the order `PlayerFactory.Create` walks them.

use super::{StarterGearSkill, StarterHeritage, StarterItem, StarterSpell};

/// `StarterGearConfiguration.Skills`.
pub static SKILLS: &[StarterGearSkill] = &[
    StarterGearSkill {
        skill_id: 6,
        name: Some("Melee Defense"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 7,
        name: Some("Missile Defense"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 14,
        name: Some("Arcane Lore"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 15,
        name: Some("Magic Defense"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 16,
        name: Some("Mana Conversion"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 18,
        name: Some("Item Tinkering"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 20,
        name: Some("Deception"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 21,
        name: Some("Healing"),
        gear: &[StarterItem {
            weenie_id: 628,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 22,
        name: Some("Jump"),
        gear: &[
            StarterItem {
                weenie_id: 273,
                stack_size: 500,
            },
            StarterItem {
                weenie_id: 1077,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 166,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 5084,
                stack_size: 1,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 259,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 261,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 258,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 23,
        name: Some("Lockpick"),
        gear: &[StarterItem {
            weenie_id: 511,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 24,
        name: Some("Run"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 27,
        name: Some("Assess Creature"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 28,
        name: Some("Weapon Tinkering"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 29,
        name: Some("Armor Tinkering"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 30,
        name: Some("Magic Item Tinkering"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 31,
        name: Some("Creature Enchantment"),
        gear: &[
            StarterItem {
                weenie_id: 691,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 774,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 760,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 789,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 749,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 626,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 750,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 754,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 782,
                stack_size: 10,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 4914,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 4916,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 4915,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[
            StarterSpell {
                spell_id: 1,
                name: Some("Strength Other I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 18,
                name: Some("Invulnerability Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 678,
                name: Some("Arcane Enlightment Self I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 32,
        name: Some("Item Enchantment"),
        gear: &[
            StarterItem {
                weenie_id: 691,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 774,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 740,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 759,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 792,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 756,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 790,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 761,
                stack_size: 10,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 4914,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 4916,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 4915,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[
            StarterSpell {
                spell_id: 35,
                name: Some("Blood Drinker I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 51,
                name: Some("Impenetrability I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 49,
                name: Some("Swift Killer I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 33,
        name: Some("Life Magic"),
        gear: &[
            StarterItem {
                weenie_id: 691,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 780,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 790,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 756,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 744,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 774,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 783,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 757,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 751,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 776,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 755,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 752,
                stack_size: 3,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 4914,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 4916,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 4915,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[
            StarterSpell {
                spell_id: 23,
                name: Some("Armor Other I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 6,
                name: Some("Heal Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 53,
                name: Some("Rejuvenation Other I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 34,
        name: Some("War Magic"),
        gear: &[
            StarterItem {
                weenie_id: 691,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 772,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 790,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 741,
                stack_size: 3,
            },
            StarterItem {
                weenie_id: 762,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 760,
                stack_size: 10,
            },
            StarterItem {
                weenie_id: 756,
                stack_size: 10,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 4914,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 4916,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 4915,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[
            StarterSpell {
                spell_id: 27,
                name: Some("Flame Bolt I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 86,
                name: Some("Force Bolt I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 75,
                name: Some("Lightning Bolt I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 35,
        name: Some("Leadership"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 36,
        name: Some("Loyalty"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 37,
        name: Some("Fletching"),
        gear: &[
            StarterItem {
                weenie_id: 4586,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 4585,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 5339,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 15296,
                stack_size: 30,
            },
        ],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 38,
        name: Some("Alchemy"),
        gear: &[
            StarterItem {
                weenie_id: 4751,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 2414,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 2414,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 2414,
                stack_size: 1,
            },
        ],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 39,
        name: Some("Cooking"),
        gear: &[
            StarterItem {
                weenie_id: 4761,
                stack_size: 6,
            },
            StarterItem {
                weenie_id: 4746,
                stack_size: 6,
            },
        ],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 40,
        name: Some("Salvaging"),
        gear: &[StarterItem {
            weenie_id: 20646,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 1,
        name: Some("Axe"),
        gear: &[StarterItem {
            weenie_id: 12740,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 4,
        name: Some("Dagger"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 527,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 523,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 527,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 5,
        name: Some("Mace"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 520,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 526,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 524,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 9,
        name: Some("Spear"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 534,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 519,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 539,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 10,
        name: Some("Staff"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 529,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 528,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 525,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 11,
        name: Some("Sword"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 535,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 533,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 538,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 13,
        name: Some("Unarmed Combat"),
        gear: &[StarterItem {
            weenie_id: 12742,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 2,
        name: Some("Bow"),
        gear: &[StarterItem {
            weenie_id: 300,
            stack_size: 250,
        }],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 518,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 537,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 531,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 3,
        name: Some("Crossbow"),
        gear: &[
            StarterItem {
                weenie_id: 12749,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 305,
                stack_size: 250,
            },
        ],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 12,
        name: Some("Thrown Weapons"),
        gear: &[
            StarterItem {
                weenie_id: 23109,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 12464,
                stack_size: 200,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 522,
                    stack_size: 50,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 522,
                    stack_size: 50,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 532,
                    stack_size: 50,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
];
