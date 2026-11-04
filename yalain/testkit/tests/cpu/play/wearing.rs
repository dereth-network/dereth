//! Wearing armor, clothing and jewelry: an item of the pack used is worn at the place its template
//! prefers (or another free one of its places), replacing what is there; the wearer's look gains
//! the appearance the item gives its race and sex, for everyone who sees it; used again it comes
//! off to the pack's front. An item whose race, skill, level or quest the wearer does not meet is
//! refused and stays in the pack. And what others see a player hold: the weapon in its hand made
//! for them hung from the hand. Fixture: players in the test hall, made-up garment templates naming
//! an appearance for the wearer's race and sex (one also naming requirements), a made-up hidden
//! quest, the human starting bow.

use auberean_protocol::types::VisualDesc;
use auberean_protocol::ServerMessage;
use yalain_content::{Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::equip::{self, flags};

const CHEST: u32 = 0x4700_7101;
const SHIRT: u32 = 0x4700_7102;
const ROBE: u32 = 0x4700_7103;
const RING: u32 = 0x4700_7104;
const CHEST_LOOK: u32 = 0x2000_7101;
const SHIRT_LOOK: u32 = 0x2000_7102;
const ROBE_LOOK: u32 = 0x2000_7103;
const CONTAINER: u32 = 258;
const WEARER: u32 = 261;

/// A garment worn on `valid` (preferring `preferred`, ruling out `precluded`) that gives a body of
/// this race and sex the appearance `look` (none: 0).
fn garment(
    id: u32,
    (valid, preferred, precluded): (u32, u32, u32),
    look: u32,
    race: u32,
    sex: u32,
) -> yalain_content::Template {
    let mut p = Props::default();
    p.set("Item_ValidInventoryLocations", Prop::Word(valid))
        .set("Item_PreferredInventoryLocation", Prop::Word(preferred))
        .set("Item_PrecludedInventoryLocations", Prop::Word(precluded));
    // Jewelry names no appearance; the others name one for another race first.
    if look != 0 {
        let other = if race == 1 { 2 } else { 1 };
        p.set("Item_WornAppearanceRace1", Prop::Word(other))
            .set("Item_WornAppearanceSex1", Prop::Word(sex))
            .set("Item_WornAppearanceFile1", Prop::Word(0x2000_7F00))
            .set("Item_WornAppearanceRace2", Prop::Word(race))
            .set("Item_WornAppearanceSex2", Prop::Word(sex))
            .set("Item_WornAppearanceFile2", Prop::Word(look));
    }
    yalain_testkit::test_template(id, 0x180, p)
}

/// Two players beside each other, the garments in the catalog for the first one's body; returns
/// the harness, the wearer's client and character, the watcher's client.
fn two_players() -> (Harness, usize, u64, usize) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, me) = h.player_in_world("one", "Elara");
    let (w, _) = h.player_in_world("two", "Bran");
    h.run(10);
    let (race, sex) = {
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == me)
            .unwrap();
        (p.race, p.sex)
    };
    for t in [
        garment(CHEST, (2, 2, 0), CHEST_LOOK, race, sex),
        garment(SHIRT, (2, 2, 0), SHIRT_LOOK, race, sex),
        garment(ROBE, (2, 2, 8), ROBE_LOOK, race, sex),
        garment(RING, (0x300, 0x100, 0), 0, race, sex),
    ] {
        h.server.catalog.insert(t);
    }
    (h, a, me, w)
}

/// The equipping answers since `from`: (wearing or taking off, the description's slots, the
/// item profile's slots).
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(bool, Vec<u32>, Vec<u32>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == equip::function::EQUIP_DONE
                    || c.function.0 == equip::function::UNEQUIP_DONE =>
            {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let Value::Graph(g) = v.first()? else {
                    return None;
                };
                let slots = |pkg: u16| {
                    g.iter().find_map(|(_, n)| match n {
                        Node::Script { package, slots } if *package == pkg => {
                            Some(slots.iter().map(|(_, x)| *x).collect::<Vec<u32>>())
                        }
                        _ => None,
                    })
                };
                Some((
                    c.function.0 == equip::function::EQUIP_DONE,
                    slots(equip::EQUIP_DESC.0)?,
                    slots(equip::EQUIP_PROFILE.0)?,
                ))
            }
            _ => None,
        })
        .collect()
}

/// The looks a client was told the player has since `from`, in order.
fn looks(h: &Harness, b: usize, from: usize, me: u64) -> Vec<VisualDesc> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateVisualDesc(e) if e.sender.id == me => Some(e.action.clone()),
            _ => None,
        })
        .collect()
}

/// The appearances a look's whole-object values name, in order.
fn tables(v: &VisualDesc) -> Vec<u32> {
    v.global
        .appearance
        .entries
        .iter()
        .map(|(k, _)| *k)
        .collect()
}

/// The instance-id qualities a client was told of an item since `from`.
fn holders(h: &Harness, b: usize, from: usize, item: u64) -> Vec<(u32, u64)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == item => {
                Some((q.quality, q.value))
            }
            _ => None,
        })
        .collect()
}

fn use_it(h: &mut Harness, b: usize, me: u64, item: u64) {
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(10);
}

fn held(h: &Harness, me: u64, item: u64) -> (u32, u32) {
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .unwrap();
    let x = p.play.items.iter().find(|x| x.id == item).unwrap();
    (x.location, x.slot)
}

/// A chest piece used from the pack is worn on the chest: the answer names the place, the places
/// it takes and rules out, its own places and preference, the slot it came from, worn by use; the
/// wearer's look gains the appearance it gives the wearer's body (the garment shown), first in its
/// bucket; the player beside it is told the new look, the wearer the item's holders. Used
/// again it comes off: answered as taken off by use, to the pack's front, the look without it, the
/// wearer cleared.
/// Behaviour: server.inventory.armor-used-is-worn-and-seen
#[test]
fn a_chest_piece_used_is_worn_and_its_look_seen_and_used_again_comes_off() {
    let (mut h, a, me, w) = two_players();
    // The body's own appearances: one in a lower bucket, one in the chest's, one in a higher.
    const BODY: [u32; 3] = [0x2000_0009, 0x2000_70D2, 0x2000_7105];
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.look.global.appearance.entries = BODY
            .iter()
            .map(|t| (*t, auberean_protocol::table::Table::default()))
            .collect();
    }
    let (fa, fw) = (h.bots[a].inbox.len(), h.bots[w].inbox.len());
    let item = h.arm(a, me, CHEST);
    let got = answers(&h, a, fa);
    assert_eq!(got.len(), 1, "one answer");
    let (wearing, d, profile) = &got[0];
    assert!(*wearing, "worn");
    assert_eq!(d[7], 2, "on the chest");
    assert_eq!((d[14], d[5], d[15]), (2, 0, 2), "taken, ruled out, where");
    assert_eq!(d[12], flags::WORN, "worn by use");
    assert_eq!(
        (d[6], d[0], d[16]),
        (0, 0, 0),
        "to slot 0, nothing replaced"
    );
    assert_eq!(
        (profile[3], profile[5]),
        (2, 2),
        "its places and its preference"
    );
    assert_eq!((profile[1], profile[2]), (0, 0), "hung from nothing");
    assert_eq!(held(&h, me, item).0, 2, "worn");
    for b in [a, w] {
        let seen = looks(&h, b, if b == a { fa } else { fw }, me);
        let last = seen.last().expect("the new look");
        assert!(
            tables(last).contains(&CHEST_LOOK),
            "the chest's appearance for the wearer's race"
        );
        let values = last.global.appearance.get(CHEST_LOOK).unwrap();
        assert_eq!(values.entries, [(0x10, 1.0)], "the garment shown");
    }
    assert_eq!(
        holders(&h, a, fa, item),
        [(CONTAINER, 0), (WEARER, me), (CONTAINER, me)],
        "the wearer told of the item's holders"
    );
    assert!(
        holders(&h, w, fw, item).is_empty(),
        "the look alone for others"
    );
    let look = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .unwrap()
        .look
        .clone();
    assert_eq!(
        tables(&look),
        [BODY[0], CHEST_LOOK, BODY[1], BODY[2]],
        "bucket by bucket, the newest first in its bucket"
    );

    let (fa, fw) = (h.bots[a].inbox.len(), h.bots[w].inbox.len());
    use_it(&mut h, a, me, item);
    let got = answers(&h, a, fa);
    assert_eq!(got.len(), 1, "one answer");
    let (wearing, d, profile) = &got[0];
    assert!(!*wearing, "taken off");
    assert_eq!(d[12], flags::TAKEN_OFF, "by its use");
    assert_eq!((d[6], d[18]), (0, 0), "to the pack's front");
    assert_eq!(profile.iter().sum::<u32>(), 0, "an empty profile");
    assert_eq!(held(&h, me, item), (0, 0), "at the front");
    let seen = looks(&h, w, fw, me);
    assert!(
        !tables(seen.last().expect("told")).contains(&CHEST_LOOK),
        "the look without it"
    );
    assert_eq!(holders(&h, a, fa, item), [(WEARER, 0)], "no wearer");
}

/// A second chest piece worn replaces the first: the first is taken off to make room (its answer
/// first), going where the second was in the pack; the second's answer names the one it replaced
/// and where that was; the look trades one appearance for the other. A robe that rules out the
/// legs takes both places. A second ring goes on the other hand; rings give no look.
/// Behaviour: server.inventory.worn-item-replaces-what-is-there
#[test]
fn a_second_chest_piece_replaces_the_first_which_goes_where_it_was() {
    let (mut h, a, me, _) = two_players();
    let first = h.arm(a, me, CHEST);
    let second = {
        let fa = h.bots[a].inbox.len();
        let id = h.arm(a, me, SHIRT);
        let got = answers(&h, a, fa);
        assert_eq!(got.len(), 2, "two answers: {got:?}");
        assert!(!got[0].0 && got[1].0, "taken off, then worn");
        assert_eq!(got[0].1[12], flags::MADE_ROOM, "taken off to make room");
        assert_eq!(got[0].1[3], u32::try_from(first & 0xFFFF_FFFF).unwrap());
        let came_from = got[1].1[18];
        assert_eq!(
            got[0].1[6],
            came_from - 1,
            "the first takes the second's place"
        );
        assert_eq!(
            (got[1].1[0], got[1].1[16]),
            (2, u32::try_from(first & 0xFFFF_FFFF).unwrap()),
            "the second names what it replaced and where"
        );
        id
    };
    assert_eq!(held(&h, me, first).0, 0, "back in the pack");
    assert_eq!(held(&h, me, second).0, 2, "worn");
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .unwrap();
    assert!(tables(&p.look).contains(&SHIRT_LOOK) && !tables(&p.look).contains(&CHEST_LOOK));

    let fa = h.bots[a].inbox.len();
    h.arm(a, me, ROBE);
    let got = answers(&h, a, fa);
    let worn = got.iter().find(|x| x.0).expect("worn");
    assert_eq!((worn.1[14], worn.1[5]), (10, 8), "the chest and the legs");

    let fa = h.bots[a].inbox.len();
    let one = h.arm(a, me, RING);
    let two = h.arm(a, me, RING);
    assert!(looks(&h, a, fa, me).is_empty(), "a ring gives no look");
    assert_eq!(
        (held(&h, me, one).0, held(&h, me, two).0),
        (0x100, 0x200),
        "one on each hand"
    );
}

/// A weapon another player holds is made hung from its hand: for a player already beside it when
/// it is wielded (no holder updates, no attaching) and for one that comes into view later; put
/// away, the holder lets go of it for those who see it (the item not unmade); wielded again, those
/// who know the item are told its holders and see it attached.
/// Behaviour: server.presence.others-held-weapons-are-made-in-hand
#[test]
fn a_weapon_another_player_holds_is_made_in_its_hand_for_those_who_see_it() {
    use auberean_protocol::types::PhysicsDesc;
    const BOW: u32 = 0x4700_051B;
    let (mut h, a, me, w) = two_players();
    let fw = h.bots[w].inbox.len();
    let bow = h.arm(a, me, BOW);
    let instance = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .unwrap()
        .instance;
    let hung = |h: &Harness, b: usize, from: usize| {
        h.world_messages(b, from).iter().find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == bow => Some(o.clone()),
            _ => None,
        })
    };
    let o = hung(&h, w, fw).expect("made for the player beside it");
    assert_eq!(o.weenie.objects[..2], [me, me], "contained and wielded");
    assert_ne!(o.physics.bits & PhysicsDesc::PARENT, 0, "hung");
    assert_eq!(
        (
            o.physics.parent.parent,
            o.physics.parent.location,
            o.physics.parent.stamp
        ),
        (me, 2, instance),
        "from the hand a bow is held in"
    );
    assert!(holders(&h, w, fw, bow).is_empty(), "no holder updates");
    assert!(
        !h.world_messages(w, fw)
            .iter()
            .any(|m| matches!(m, ServerMessage::Parent(e) if e.sender.id == bow)),
        "not attached: made so"
    );

    let fw = h.bots[w].inbox.len();
    use_it(&mut h, a, me, bow);
    let msgs = h.world_messages(w, fw);
    assert!(
        msgs.iter().any(|m| matches!(m, ServerMessage::DeParent(e)
            if e.sender.id == me && e.action.id == bow)),
        "the holder lets go of it"
    );
    assert!(holders(&h, w, fw, bow).is_empty(), "no holder updates");

    let fw = h.bots[w].inbox.len();
    use_it(&mut h, a, me, bow);
    assert_eq!(
        holders(&h, w, fw, bow),
        [(CONTAINER, 0), (WEARER, me), (CONTAINER, me)],
        "known: its holders"
    );
    assert!(
        h.world_messages(w, fw)
            .iter()
            .any(|m| matches!(m, ServerMessage::Parent(e) if e.sender.id == bow)),
        "attached"
    );

    let (c, _) = h.player_in_world("three", "Cora");
    h.run(10);
    let o = hung(&h, c, 0).expect("made for one coming into view");
    assert_eq!(
        (o.physics.parent.parent, o.physics.parent.location),
        (me, 2),
        "hung from the hand"
    );

    // The holder leaves and comes back with the bow in hand: made for the one waiting there.
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        auberean_protocol::queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: me,
        }),
    );
    assert!(h.run_until(400, |h| h.bots[a].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let fw = h.bots[w].inbox.len();
    h.enter_world(a, "one", me);
    h.run(10);
    assert!(hung(&h, w, fw).is_some(), "made for the one waiting");
}

/// The use answers a client was sent since `from`: (status, control flags, effective target).
fn use_answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    const USE_DONE: u32 = 0x8216_0021;
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == USE_DONE)
        .filter_map(|(_, v)| {
            let Value::Graph(g) = v.first()? else {
                return None;
            };
            g.iter().find_map(|(_, n)| match n {
                Node::Script { slots, .. } => Some((slots[11].1, slots[17].1, slots[12].1)),
                _ => None,
            })
        })
        .collect()
}

/// A chest piece that names a race other than the wearer's, a skill, a least level and a hidden
/// quest is refused, each in turn as the one before is met: for monarchs alone (10038), before all
/// of them, the race (10015), the skill (10013),
/// the level (10009), the quest as mysterious (10043); each answered alone, with the use's flags
/// and no effective target, the piece left in the pack. With all of them met it is worn.
/// Behaviour: server.equip.requirements-refuse
#[test]
fn a_chest_piece_whose_race_skill_level_or_quest_the_wearer_lacks_is_refused_in_that_order() {
    use yalain_content::quests::{QuestDef, QuestTable};
    use yalain_world::item_use::refusal as use_refusal;
    use yalain_world::quests::status;
    use yalain_world::state::{QuestState, SkillState};
    const GUARDED: u32 = 0x4700_7105;
    const SKILL: u32 = 0x163;
    const QUEST: u32 = 0x4000_0297;
    const LEVEL: u32 = 302;
    let (mut h, a, me, _) = two_players();
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .unwrap()
        .0;
    let (race, sex) = {
        let p = &h.server.players[&world];
        (p.race, p.sex)
    };
    let mut t = garment(GUARDED, (2, 2, 0), CHEST_LOOK, race, sex);
    t.props
        .set("Usage_RequiredRace", Prop::Word(!race & 0x7))
        .set("Usage_RequiredSkill1", Prop::Word(SKILL))
        .set("Usage_MinLevel", Prop::Int(30))
        .set("Usage_RequiredQuest", Prop::Word(QUEST))
        .set("Usage_RequiredQuestStatus", Prop::Word(status::COMPLETED));
    h.server.catalog.insert(t);
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            id: QUEST,
            name: (0xA860_7CEC, 0x2500_17EE),
            description: (0x6C45_9364, 0x2500_17EE),
            icon: 0,
            play_fx: false,
            journals: Vec::new(),
            challenge: -999,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: true,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
        }],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        LEVEL,
        10,
    );
    let fa = h.bots[a].inbox.len();
    let item = h.arm(a, me, GUARDED);
    let refused = |h: &mut Harness, fa: usize, why: u32, what: &str| {
        assert_eq!(use_answers(h, a, fa), [(why, 316, 0)], "{what}");
        assert!(answers(h, a, fa).is_empty(), "{what}: no equipping");
        assert_eq!(held(h, me, item).0, 0, "{what}: still in the pack");
    };
    refused(&mut h, fa, equip::refusal::WRONG_RACE, "the race first");
    // For monarchs alone, before all that, and a monarch goes on to the rest.
    let mut monarchs = h
        .server
        .catalog
        .get(GUARDED)
        .expect("in the catalog")
        .clone();
    monarchs.props.set("Usage_MonarchOnly", Prop::Bool(true));
    h.server.catalog.insert(monarchs.clone());
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    refused(&mut h, fa, use_refusal::MONARCH_ONLY, "for monarchs");
    h.server
        .allegiances
        .nodes
        .insert(me, yalain_world::allegiance::Node::default());
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    refused(
        &mut h,
        fa,
        equip::refusal::WRONG_RACE,
        "a monarch: the race",
    );
    let edit = |h: &mut Harness, f: &dyn Fn(&mut yalain_world::world::Player)| {
        f(h.server.players.get_mut(&world).unwrap());
    };
    edit(&mut h, &|p| {
        let want = !race & 0x7;
        p.race = want.isolate_lowest_one();
    });
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    refused(&mut h, fa, equip::refusal::MISSING_SKILL, "then the skill");
    edit(&mut h, &|p| {
        p.play.skills.push(SkillState {
            skill: SKILL,
            granted: 0,
            mask: 1,
            cost: 2,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    });
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    refused(&mut h, fa, use_refusal::LEVEL_TOO_LOW, "then the level");
    edit(&mut h, &|p| {
        yalain_world::quals::set_int(&mut p.qualities, LEVEL, 30);
    });
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    refused(
        &mut h,
        fa,
        use_refusal::MYSTERIOUS_USE,
        "then the hidden quest",
    );
    edit(&mut h, &|p| {
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: status::COMPLETED,
            phase: 0,
            started: 0,
            updated: 0,
        });
    });
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    assert!(use_answers(&h, a, fa).is_empty(), "met: no refusal");
    assert_eq!(answers(&h, a, fa).len(), 1, "worn");
    assert_eq!(held(&h, me, item).0, 2, "on the chest");
}

/// A treasure chest piece whose template names no requirement carries its own least level and
/// Arcane Lore: used below its level it is refused 10009; at its level, without the lore, 10050;
/// with an Arcane Lore skill rated at the item's lore it is worn.
/// Behaviour: server.equip.own-requirements-refuse
#[test]
fn a_treasure_chest_piece_is_refused_by_its_own_least_level_then_its_own_arcane_lore() {
    use yalain_world::item_use::refusal as use_refusal;
    use yalain_world::skills::{SkillDef, ARCANE_LORE_CATEGORY};
    use yalain_world::state::SkillState;
    const TREASURE: u32 = 0x4700_7106;
    const LORE_SKILL: u32 = 0x466;
    const LEVEL: u32 = 302;
    let (mut h, a, me, _) = two_players();
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .unwrap()
        .0;
    let (race, sex) = {
        let p = &h.server.players[&world];
        (p.race, p.sex)
    };
    h.server
        .catalog
        .insert(garment(TREASURE, (2, 2, 0), CHEST_LOOK, race, sex));
    h.server.skill_table.skills.insert(
        LORE_SKILL,
        SkillDef {
            id: LORE_SKILL,
            cap: 1000,
            category: ARCANE_LORE_CATEGORY,
            attribute_mod: 1.0,
            ..SkillDef::default()
        },
    );
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        LEVEL,
        49,
    );
    let fa = h.bots[a].inbox.len();
    let item = h.arm(a, me, TREASURE);
    assert_eq!(
        answers(&h, a, fa).len(),
        1,
        "with nothing of its own it is worn"
    );
    use_it(&mut h, a, me, item);
    assert_eq!(held(&h, me, item).0, 0, "taken off again");
    h.server.things.get_mut(item).unwrap().own = yalain_world::treasure::Own {
        min_level: Some(50),
        arcane_lore: Some(26),
        effect: None,
        ..yalain_world::treasure::Own::default()
    };
    let refused = |h: &mut Harness, why: u32, what: &str| {
        let fa = h.bots[a].inbox.len();
        use_it(h, a, me, item);
        assert_eq!(use_answers(h, a, fa), [(why, 316, 0)], "{what}");
        assert!(answers(h, a, fa).is_empty(), "{what}: no equipping");
        assert_eq!(held(h, me, item).0, 0, "{what}: still in the pack");
    };
    refused(&mut h, use_refusal::LEVEL_TOO_LOW, "below its own level");
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        LEVEL,
        50,
    );
    refused(
        &mut h,
        equip::refusal::ARCANE_LORE_TOO_LOW,
        "without its Arcane Lore",
    );
    let lore = |h: &mut Harness, rating: u32| {
        let p = h.server.players.get_mut(&world).unwrap();
        p.play.skills.retain(|k| k.skill != LORE_SKILL);
        p.play.skills.push(SkillState {
            skill: LORE_SKILL,
            granted: rating,
            mask: 1,
            cost: 75,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    };
    lore(&mut h, 25);
    refused(
        &mut h,
        equip::refusal::ARCANE_LORE_TOO_LOW,
        "an Arcane Lore one short",
    );
    lore(&mut h, 26);
    let fa = h.bots[a].inbox.len();
    use_it(&mut h, a, me, item);
    assert!(use_answers(&h, a, fa).is_empty(), "met: no refusal");
    assert_eq!(answers(&h, a, fa).len(), 1, "worn");
    assert_eq!(held(&h, me, item).0, 2, "on the chest");
}
