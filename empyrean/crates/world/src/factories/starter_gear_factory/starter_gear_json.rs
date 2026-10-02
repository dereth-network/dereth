// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/StarterGearFactory.cs
// @generated from ACE's `Source/ACE.Server/starterGear.json`; do not edit by hand
//! ACE's `Source/ACE.Server/starterGear.json`,
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
                stack_size: 10000,
            },
            StarterItem {
                weenie_id: 166,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 5084,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 33613,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 259,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 20646,
                stack_size: 1,
            },
        ],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 30988,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 30986,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 30985,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 4,
                name: Some("Viamontian"),
                gear: &[StarterItem {
                    weenie_id: 30987,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 5,
                name: Some("Umbrean"),
                gear: &[StarterItem {
                    weenie_id: 43019,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 6,
                name: Some("Gear"),
                gear: &[
                    StarterItem {
                        weenie_id: 43018,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 42979,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 43022,
                        stack_size: 1,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 10,
                name: Some("Penumbraen"),
                gear: &[StarterItem {
                    weenie_id: 43019,
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
                weenie_id: 12748,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 15268,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 691,
                stack_size: 5,
            },
            StarterItem {
                weenie_id: 20631,
                stack_size: 25,
            },
        ],
        heritage: &[],
        spells: &[
            StarterSpell {
                spell_id: 1421,
                name: Some("Focus Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 17,
                name: Some("Invulnerability Other I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 18,
                name: Some("Invulnerability Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 653,
                name: Some("Mana Conversion Mastery Self I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 1445,
                name: Some("Willpower Self I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 32,
        name: Some("Item Enchantment"),
        gear: &[
            StarterItem {
                weenie_id: 12748,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 15269,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 691,
                stack_size: 5,
            },
            StarterItem {
                weenie_id: 20631,
                stack_size: 25,
            },
        ],
        heritage: &[],
        spells: &[
            StarterSpell {
                spell_id: 35,
                name: Some("Aura of Blood Drinker Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 1511,
                name: Some("Bludgeon Bane I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 49,
                name: Some("Aura of Swift Killer Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 51,
                name: Some("Impenetrability I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 1599,
                name: Some("Aura of Defender Self I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 37,
                name: Some("Blade Bane I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 33,
        name: Some("Life Magic"),
        gear: &[
            StarterItem {
                weenie_id: 12748,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 15270,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 691,
                stack_size: 5,
            },
            StarterItem {
                weenie_id: 20631,
                stack_size: 25,
            },
        ],
        heritage: &[],
        spells: &[
            StarterSpell {
                spell_id: 23,
                name: Some("Armor Other I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 24,
                name: Some("Armor Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 5,
                name: Some("Heal Other I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 6,
                name: Some("Heal Self I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 25,
                name: Some("Imperil Other I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 1237,
                name: Some("Drain Health Other I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 7,
                name: Some("Harm Other I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 34,
        name: Some("War Magic"),
        gear: &[
            StarterItem {
                weenie_id: 12748,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 15271,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 691,
                stack_size: 5,
            },
            StarterItem {
                weenie_id: 20631,
                stack_size: 25,
            },
        ],
        heritage: &[],
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
                spell_id: 28,
                name: Some("Frost Bolt I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 64,
                name: Some("Shock Wave I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 58,
                name: Some("Acid Stream I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 75,
                name: Some("Lightning Bolt I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 92,
                name: Some("Whirling Blade I"),
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
                weenie_id: 4586,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 4586,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 4585,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 15296,
                stack_size: 30,
            },
            StarterItem {
                weenie_id: 5339,
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
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 41,
        name: Some("Two Handed Combat"),
        gear: &[StarterItem {
            weenie_id: 41512,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 43,
        name: Some("Void Magic"),
        gear: &[
            StarterItem {
                weenie_id: 12748,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 43173,
                stack_size: 1,
            },
            StarterItem {
                weenie_id: 691,
                stack_size: 5,
            },
            StarterItem {
                weenie_id: 20631,
                stack_size: 25,
            },
        ],
        heritage: &[],
        spells: &[
            StarterSpell {
                spell_id: 5339,
                name: Some("Destructive Curse I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 5387,
                name: Some("Corrosion I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 5395,
                name: Some("Corruption I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 5349,
                name: Some("Nether Bolt I"),
                specialized_only: false,
            },
            StarterSpell {
                spell_id: 5379,
                name: Some("Weakening Curse I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 5357,
                name: Some("Nether Streak I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 5369,
                name: Some("Nether Arc I"),
                specialized_only: true,
            },
            StarterSpell {
                spell_id: 5371,
                name: Some("Festering Curse I"),
                specialized_only: true,
            },
        ],
    },
    StarterGearSkill {
        skill_id: 44,
        name: Some("Heavy Weapons"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 12739,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 12743,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 12742,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 4,
                name: Some("Viamontian"),
                gear: &[StarterItem {
                    weenie_id: 12747,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 5,
                name: Some("Umbrean"),
                gear: &[StarterItem {
                    weenie_id: 12742,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 6,
                name: Some("Gear"),
                gear: &[StarterItem {
                    weenie_id: 12744,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 7,
                name: Some("Tumerok"),
                gear: &[StarterItem {
                    weenie_id: 12745,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 8,
                name: Some("Lugian"),
                gear: &[StarterItem {
                    weenie_id: 12740,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 9,
                name: Some("Empyrean"),
                gear: &[StarterItem {
                    weenie_id: 12747,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 10,
                name: Some("Penumbraen"),
                gear: &[StarterItem {
                    weenie_id: 12742,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 11,
                name: Some("Undead"),
                gear: &[StarterItem {
                    weenie_id: 12740,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 45,
        name: Some("Light Weapons"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 45538,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 45550,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 45558,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 4,
                name: Some("Viamontian"),
                gear: &[StarterItem {
                    weenie_id: 45554,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 5,
                name: Some("Umbrean"),
                gear: &[StarterItem {
                    weenie_id: 45558,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 6,
                name: Some("Gear"),
                gear: &[StarterItem {
                    weenie_id: 45542,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 7,
                name: Some("Tumerok"),
                gear: &[StarterItem {
                    weenie_id: 45546,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 8,
                name: Some("Lugian"),
                gear: &[StarterItem {
                    weenie_id: 45534,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 9,
                name: Some("Empyrean"),
                gear: &[StarterItem {
                    weenie_id: 45554,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 10,
                name: Some("Penumbraen"),
                gear: &[StarterItem {
                    weenie_id: 45558,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 11,
                name: Some("Undead"),
                gear: &[StarterItem {
                    weenie_id: 45534,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 46,
        name: Some("Finesse Weapons"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[StarterItem {
                    weenie_id: 45537,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[StarterItem {
                    weenie_id: 45549,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[StarterItem {
                    weenie_id: 45557,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 4,
                name: Some("Viamontian"),
                gear: &[StarterItem {
                    weenie_id: 45553,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 5,
                name: Some("Umbrean"),
                gear: &[StarterItem {
                    weenie_id: 45557,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 6,
                name: Some("Gear"),
                gear: &[StarterItem {
                    weenie_id: 45541,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 7,
                name: Some("Tumerok"),
                gear: &[StarterItem {
                    weenie_id: 45545,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 8,
                name: Some("Lugian"),
                gear: &[StarterItem {
                    weenie_id: 45533,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 9,
                name: Some("Empyrean"),
                gear: &[StarterItem {
                    weenie_id: 45553,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 10,
                name: Some("Penumbraen"),
                gear: &[StarterItem {
                    weenie_id: 45557,
                    stack_size: 1,
                }],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 11,
                name: Some("Undead"),
                gear: &[StarterItem {
                    weenie_id: 45533,
                    stack_size: 1,
                }],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 47,
        name: Some("Missile Weapons"),
        gear: &[],
        heritage: &[
            StarterHeritage {
                heritage_id: 1,
                name: Some("Aluvian"),
                gear: &[
                    StarterItem {
                        weenie_id: 12741,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31717,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 2,
                name: Some("Gharu'ndim"),
                gear: &[
                    StarterItem {
                        weenie_id: 12746,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31715,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 3,
                name: Some("Sho"),
                gear: &[
                    StarterItem {
                        weenie_id: 12741,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31717,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 4,
                name: Some("Viamontian"),
                gear: &[
                    StarterItem {
                        weenie_id: 12749,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31716,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 5,
                name: Some("Umbrean"),
                gear: &[
                    StarterItem {
                        weenie_id: 12749,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31716,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 6,
                name: Some("Gear"),
                gear: &[
                    StarterItem {
                        weenie_id: 12749,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31716,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 7,
                name: Some("Tumerok"),
                gear: &[
                    StarterItem {
                        weenie_id: 12746,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31715,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 8,
                name: Some("Lugian"),
                gear: &[
                    StarterItem {
                        weenie_id: 12746,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31715,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 9,
                name: Some("Empyrean"),
                gear: &[
                    StarterItem {
                        weenie_id: 12741,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31717,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 10,
                name: Some("Penumbraen"),
                gear: &[
                    StarterItem {
                        weenie_id: 12749,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31716,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
            StarterHeritage {
                heritage_id: 11,
                name: Some("Undead"),
                gear: &[
                    StarterItem {
                        weenie_id: 12746,
                        stack_size: 1,
                    },
                    StarterItem {
                        weenie_id: 31715,
                        stack_size: 250,
                    },
                ],
                spells: &[],
            },
        ],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 48,
        name: Some("Shield"),
        gear: &[StarterItem {
            weenie_id: 93,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 49,
        name: Some("Dual Wield"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 50,
        name: Some("Recklessness"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 51,
        name: Some("Sneak Attack"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 52,
        name: Some("Dirty Fighting"),
        gear: &[],
        heritage: &[],
        spells: &[],
    },
    StarterGearSkill {
        skill_id: 54,
        name: Some("Summoning"),
        gear: &[StarterItem {
            weenie_id: 48886,
            stack_size: 1,
        }],
        heritage: &[],
        spells: &[],
    },
];
