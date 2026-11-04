//! Crafting: what a new character knows, putting ingredients into a recipe, making it, and the
//! recipes quests give and take back.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::Item;
use yalain_testkit::play::server_call;
use yalain_testkit::{
    Harness, BUTCHERY, GRANT_MINES_RECIPE, IRON, KNIFE_RECIPE, MINES_RECIPE, MINES_SWORD,
    QUEST_RECIPES, TOOLS,
};
use yalain_world::calls::{self, Node, Value};
use yalain_world::craft::{error, function};
use yalain_world::effects;

/// Run the world for so many seconds.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// The script objects of a call's graph values, as (package, slot count).
fn objects(v: &[Value]) -> Vec<(u16, usize)> {
    v.iter()
        .filter_map(|x| match x {
            Value::Graph(g) => Some(g),
            _ => None,
        })
        .flat_map(|g| g.iter())
        .filter_map(|(_, n)| match n {
            Node::Script { package, slots } => Some((*package, slots.len())),
            _ => None,
        })
        .collect()
}

/// A player in the world with the scene drawn; its world connection.
fn in_world() -> (Harness, usize, u64, yalain_net::ConnId) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    (h, b, me, world)
}

/// A recipe learned, and iron in the pack: `n` of it in one stack. Returns the stack.
fn with_recipe_and_iron(h: &mut Harness, world: yalain_net::ConnId, me: u64, n: u32) -> u64 {
    for t in [IRON, MINES_SWORD] {
        if h.server.catalog.get(t).is_none() {
            h.server.catalog.insert(yalain_testkit::test_template(
                t,
                0x200,
                yalain_content::Props::default(),
            ));
        }
    }
    let clock = h.clock();
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        GRANT_MINES_RECIPE,
        cast,
        clock,
    );
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, n);
    h.run(2);
    h.server
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().find(|i| i.template == IRON))
        .map(|i| i.id)
        .expect("the iron")
}

fn add_ingredient(h: &mut Harness, b: usize, recipe: u32, slot: u32, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::ADD_INGREDIENT,
            &[
                Arg::Int(0),
                Arg::LongInt(item),
                Arg::Int(slot),
                Arg::Int(recipe),
            ],
        ),
    );
    h.run(4);
    from
}

/// Ask to make a recipe with items by slot, as the client packs it (the target, the slots' items
/// as a table of slot to item, the spinner, the recipe).
fn execute(h: &mut Harness, b: usize, recipe: u32, slots: &[(u32, u64)]) -> usize {
    execute_at(h, b, recipe, slots, 0)
}

/// Ask to make a recipe with the spinner at a value.
fn execute_at(h: &mut Harness, b: usize, recipe: u32, slots: &[(u32, u64)], spinner: u32) -> usize {
    let mut g = yalain_world::graph::Builder::new();
    let entries: Vec<(Item, Item)> = slots
        .iter()
        .map(|(s, i)| (Item::Word(*s), Item::Long(*i)))
        .collect();
    g.native(
        8,
        yalain_world::graph::hash(Elem::Word, Elem::Long, 0, &entries),
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::EXECUTE,
            &[
                Arg::LongInt(0),
                Arg::Object(g.graph()),
                Arg::Int(spinner),
                Arg::Int(recipe),
            ],
        ),
    );
    from
}

/// A new character is told its craft skills (every one, at level 1), then its whole craft
/// registry: those craft skills and the starting recipes, each recipe never used.
/// Behaviour: server.craft.new-character-registry
#[test]
fn a_new_character_gets_every_craft_skill_and_the_starting_recipes() {
    let mut h = Harness::new();
    let b = h.at_character_list("one");
    let id = h.create_character(b, "one", "Elara");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", id);
    let got = calls_of(&h, b, from);
    let at = |f: u32| got.iter().position(|(x, _)| *x == f);
    assert!(
        at(function::SKILLS) < at(function::REFRESH),
        "skills, then the registry"
    );
    let skills = &got[at(function::SKILLS).expect("the skills")].1;
    assert_eq!(objects(skills), [(0x1B4F, 6); 3]);
    let registry = &got[at(function::REFRESH).expect("the registry")].1;
    let objs = objects(registry);
    assert_eq!(objs.first(), Some(&(0x1D2, 7)), "the registry first");
    assert_eq!(objs.iter().filter(|o| o.0 == 0x1B4F).count(), 3);
    assert_eq!(objs.iter().filter(|o| *o == &(0x1D9, 12)).count(), 1);
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(
        p.play
            .craft_skills
            .iter()
            .map(|c| (c.skill, c.level))
            .collect::<Vec<_>>(),
        [(QUEST_RECIPES, 1), (TOOLS, 1), (BUTCHERY, 1)]
    );
    assert_eq!(
        p.play.recipes.iter().map(|r| r.recipe).collect::<Vec<_>>(),
        [KNIFE_RECIPE]
    );
}

/// An effect that grants a recipe teaches it (the recipe learned), shows its appearance and says
/// so; an ingredient put into the recipe's slot is answered with the request's values and 0, a
/// stack too small with "too few", a recipe the player does not know with "bad recipe".
/// Behaviour: server.craft.ingredient-answered
#[test]
fn an_ingredient_is_answered_with_why_it_does_not_fit() {
    let (mut h, b, me, world) = in_world();
    let from = h.bots[b].inbox.len();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    let got = calls_of(&h, b, from);
    assert!(got.contains(&(function::ADD_RECIPE, vec![Value::Int(MINES_RECIPE)])));
    assert!(h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::DoFx(e) if e.action.fx == 0x4100_0499)));
    let from = add_ingredient(&mut h, b, MINES_RECIPE, 1, iron);
    assert_eq!(
        calls_of(&h, b, from),
        [(
            function::INGREDIENT_ADDED,
            vec![
                Value::Int(0),
                Value::Int(0),
                Value::Long(iron),
                Value::Int(1),
                Value::Int(MINES_RECIPE)
            ]
        )]
    );
    let from = add_ingredient(&mut h, b, KNIFE_RECIPE + 1, 1, iron);
    assert_eq!(calls_of(&h, b, from)[0].1[0], Value::Int(error::BAD_RECIPE));
    let small = {
        let p = h.server.players.get_mut(&world).expect("in the world");
        let held = p
            .play
            .items
            .iter_mut()
            .find(|i| i.id == iron)
            .expect("held");
        held.quantity = 5;
        iron
    };
    let from = add_ingredient(&mut h, b, MINES_RECIPE, 1, small);
    assert_eq!(calls_of(&h, b, from)[0].1[0], Value::Int(error::TOO_FEW));
}

/// Making the Arwic mines' sword: the player plays the recipe's animation for its two cycles, lets
/// it go, and about four seconds after the request the result: the sword made in the pack, the
/// recipe's text, the answer (shown, 0, the recipe), then the twenty iron used up; the quest the
/// recipe moves on is moved on.
/// Behaviour: server.craft.made
#[test]
fn making_a_recipe_plays_its_animation_then_makes_its_product_and_uses_up_its_ingredients() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    h.run(10);
    let msgs = h.world_messages(b, from);
    let anim = msgs.iter().find_map(|m| match m {
        ServerMessage::PlayBehavior(e) if e.sender.id == me => Some(e.action.clone()),
        _ => None,
    });
    let anim = anim.expect("the animation");
    assert_eq!((anim.behavior, anim.hold_cycles), (0x4100_00A4, 2));
    assert!(
        !calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == function::EXECUTED),
        "no answer yet"
    );
    wait(&mut h, 5.0);
    let msgs = h.world_messages(b, from);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::LetGoBehavior(e) if e.action.behavior == 0x4100_00A4)));
    let got = calls_of(&h, b, from);
    let at = |f: u32| got.iter().position(|(x, _)| *x == f);
    let done = at(function::EXECUTED).expect("the answer");
    assert_eq!(
        got[done].1,
        vec![Value::Int(1), Value::Int(0), Value::Int(MINES_RECIPE)]
    );
    let text = got
        .iter()
        .position(|(f, v)| *f == 0x8205_0011 && v.last() == Some(&Value::Int(8)));
    assert!(text < Some(done), "the recipe's text before the answer");
    let destroyed = at(0x820E_0067).or_else(|| {
        got.iter()
            .position(|(f, v)| v.contains(&Value::Long(iron)) && *f != function::INGREDIENT_ADDED)
    });
    assert!(
        destroyed > Some(done),
        "the iron used up after the answer: {got:x?}"
    );
    let p = h.server.players.get(&world).expect("in the world");
    assert!(p.play.items.iter().any(|i| i.template == MINES_SWORD));
    assert!(!p.play.items.iter().any(|i| i.template == IRON));
}

/// Making a recipe while attacking is refused at once with the answer alone: not at peace.
/// Behaviour: server.craft.refused
#[test]
fn making_a_recipe_while_attacking_is_refused_with_the_answer_alone() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.attacking = Some(yalain_world::combat::Run {
            target: 1,
            next_swing: f64::INFINITY,
            context: 0,
            mode_sent: true,
        });
    }
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    h.run(4);
    assert_eq!(
        calls_of(&h, b, from),
        [(
            function::EXECUTED,
            vec![
                Value::Int(1),
                Value::Int(error::PEACE_REQUIRED),
                Value::Int(MINES_RECIPE)
            ]
        )]
    );
}

/// Making a recipe ends a protection first: "You are no longer protected by the portal's magic.",
/// the protection taken away and its glow stopped, before the recipe's animation.
/// Behaviour: server.craft.ends-protection
#[test]
fn making_a_recipe_ends_a_protection_before_its_animation() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    let clock = h.clock();
    effects::protect_with(
        &mut h.server,
        &mut h.net,
        world,
        effects::RECALL_PROTECTION,
        (me, 0),
        clock,
    );
    h.run(2);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    h.run(4);
    let msgs = h.world_messages(b, from);
    let ended = msgs.iter().position(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == 0x8205_0011 => c
            .args
            .as_ref()
            .and_then(|a| calls::values(&a.0).ok())
            .unwrap_or_default()
            .iter()
            .any(|v| matches!(v.root(), Some(Node::TextInfo(t)) if t.string_id == effects::RECALL_PROTECTION.ended.0)),
        _ => false,
    });
    let removed = msgs.iter().position(
        |m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == effects::function::REMOVE),
    );
    let glow = msgs.iter().position(
        |m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == 0x4100_000F),
    );
    let anim = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == me));
    assert!(
        ended.is_some() && ended < removed && removed < glow && glow < anim,
        "{ended:?} {removed:?} {glow:?} {anim:?}"
    );
    assert!(h
        .server
        .players
        .get(&world)
        .is_some_and(|p| p.protected.is_none()));
}

/// The pool qualities a list of messages sets, in order: (quality, value).
fn pool_updates(msgs: &[ServerMessage]) -> Vec<(u32, u64)> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateLongInt(q) if q.quality == 1000 || q.quality == 1001 => {
                Some((q.quality, q.value))
            }
            _ => None,
        })
        .collect()
}

/// The craft skill records a list of calls carries one by one: (skill, today, available, level).
fn skill_records(got: &[(u32, Vec<Value>)]) -> Vec<(u32, u64, u64, u32)> {
    got.iter()
        .filter(|(f, _)| *f == function::SKILL)
        .filter_map(|(_, v)| {
            let Value::Graph(g) = v.first()? else {
                return None;
            };
            g.iter().find_map(|(_, n)| match n {
                Node::Script { package, slots } if *package == 0x1B4F => {
                    let long = |i: usize| u64::from(slots[i + 1].1) << 32 | u64::from(slots[i].1);
                    Some((slots[2].1, long(0), long(3), slots[5].1))
                }
                _ => None,
            })
        })
        .collect()
}

/// Craft experience is shared out as 2017's was: a fifth of a recipe skill's award to the
/// player's pool, a hundredth (at least one point) of a gathering skill's, the rest to the craft
/// skill, which rises a level as soon as it holds the next step; at its cap a craft skill takes
/// only the pool's share, all of it the player's.
/// Behaviour: server.craft.experience-shared-out
#[test]
fn craft_experience_goes_a_share_to_the_pool_and_the_rest_to_the_craft_skill() {
    use yalain_world::craft::share_out;
    use yalain_world::state::CraftSkillState;
    let c = yalain_testkit::test_crafting();
    let tools = c.skills.iter().find(|d| d.did == TOOLS).expect("tools");
    let butchery = yalain_testkit::butchery_skill();
    let at = |level: u32, available: u64| CraftSkillState {
        skill: 0,
        level,
        today: 100,
        available,
    };
    let mut r = at(5, 0);
    assert_eq!(share_out(tools, &mut r, 17), 3);
    assert_eq!((r.today, r.available, r.level), (117, 14, 5));
    let mut r = at(9, 0);
    assert_eq!(share_out(&butchery, &mut r, 37), 1, "at least one point");
    assert_eq!(
        (r.today, r.available, r.level),
        (137, 14, 11),
        "two steps of 11 taken"
    );
    let mut r = at(6, 9);
    assert_eq!(share_out(&butchery, &mut r, 2), 1);
    assert_eq!((r.level, r.available), (7, 0), "the next step reached");
    let mut r = at(150, 954);
    assert_eq!(share_out(&butchery, &mut r, 532), 5, "at the cap");
    assert_eq!((r.today, r.available, r.level), (105, 954, 150));
}

/// A gathering use pays its craft skill's amount for the level, less once the day's experience
/// passes the day's mark: three quarters, then half past twice it, then a quarter past three
/// times it.
/// Behaviour: server.craft.usage-pays-less-through-the-day
#[test]
fn a_gathering_use_pays_less_as_the_days_experience_passes_its_mark() {
    use yalain_world::craft::usage_xp;
    use yalain_world::state::CraftSkillState;
    let butchery = yalain_testkit::butchery_skill();
    let pay = |today: u64| {
        usage_xp(
            &butchery,
            &CraftSkillState {
                skill: BUTCHERY,
                level: 7,
                today,
                available: 0,
            },
        )
    };
    assert_eq!(
        [pay(0), pay(120), pay(121), pay(241), pay(361)],
        [2, 2, 1, 1, 0]
    );
}

/// A recipe made pays the craft experience its table gives the spinner chosen: the player's pool
/// is told (what is left, then the total), then the craft skill's record.
/// Behaviour: server.craft.recipe-pays-experience
#[test]
fn a_recipe_made_pays_the_experience_of_its_spinner_and_tells_the_pool_then_the_craft_skill() {
    let (mut h, b, _me, world) = in_world();
    if let Some(r) = h.server.crafting.recipes.get_mut(&KNIFE_RECIPE) {
        r.xp_map = Some(yalain_world::craft::LevelMap {
            first: 1,
            values: vec![5, 6, 8, 9, 11],
        });
    }
    let from = execute_at(&mut h, b, KNIFE_RECIPE, &[], 3);
    h.run(4);
    wait(&mut h, 9.0);
    let msgs = h.world_messages(b, from);
    assert_eq!(pool_updates(&msgs), [(1001, 1), (1000, 8)]);
    let got = calls_of(&h, b, from);
    assert_eq!(skill_records(&got), [(TOOLS, 8, 7, 1)]);
    let at = |f: u32| got.iter().position(|(x, _)| *x == f);
    assert!(at(function::SKILL) < at(function::EXECUTED));
    let p = h.server.players.get(&world).expect("in the world");
    assert_eq!(
        yalain_world::quals::long(&p.qualities, 1001),
        Some(1),
        "the pool kept with the player"
    );
}

/// Raising a craft skill by asking spends from the pool what the skill still lacks for its next
/// level: the pool told, the record (a level up, nothing held), "Your rating in %1 is now %2.";
/// a raise the pool cannot pay for is left unanswered and changes nothing.
/// Behaviour: server.craft.raised-from-the-pool
#[test]
fn raising_a_craft_skill_spends_from_the_pool_what_it_lacks() {
    let (mut h, b, _me, world) = in_world();
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_long(&mut p.qualities, 1001, 155);
        if let Some(c) = p.play.craft_skills.iter_mut().find(|c| c.skill == TOOLS) {
            c.available = 0;
        }
    }
    let raise = |h: &mut Harness| {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(function::RAISE, &[Arg::Int(TOOLS)]),
        );
        h.run(4);
        from
    };
    let from = raise(&mut h);
    let msgs = h.world_messages(b, from);
    assert_eq!(pool_updates(&msgs), [(1001, 115)]);
    let got = calls_of(&h, b, from);
    assert_eq!(skill_records(&got), [(TOOLS, 0, 0, 2)]);
    let text = got
        .iter()
        .find(|(f, _)| *f == 0x8205_0011)
        .expect("the text");
    assert_eq!(text.1.last(), Some(&Value::Int(8)));
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_long(&mut p.qualities, 1001, 30);
    }
    let from = raise(&mut h);
    assert!(
        calls_of(&h, b, from).is_empty(),
        "a raise it cannot pay for"
    );
    let p = h.server.players.get(&world).expect("in the world");
    let tools = p.play.craft_skills.iter().find(|c| c.skill == TOOLS);
    assert_eq!(tools.map(|c| c.level), Some(2));
}

/// When a player's craft day is over, every craft skill earns nothing today again, the client is
/// told them all, and the next day ends 22 hours later.
/// Behaviour: server.craft.day-starts-over
#[test]
fn a_craft_day_over_starts_every_craft_skills_day_again() {
    let (mut h, b, _me, world) = in_world();
    let clock = h.clock();
    let now = h.net.game_time(clock.local);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.play.craft_reset = now - 1.0;
        for c in &mut p.play.craft_skills {
            c.today = 50;
        }
    }
    let from = h.bots[b].inbox.len();
    yalain_world::craft::day_due(&mut h.server, &mut h.net, world, clock);
    h.run(2);
    let got = calls_of(&h, b, from);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, function::SKILLS);
    let p = h.server.players.get(&world).expect("in the world");
    assert!(p.play.craft_skills.iter().all(|c| c.today == 0));
    assert!((p.play.craft_reset - (now + 79_240.0)).abs() < 1.0);
}

/// A quest step whose recipes come after its update (the Esper Warder's bestowal) tells the quest
/// update first and then the recipe learned; a step without it tells the recipe first.
/// Behaviour: server.quests.step-effects-after-the-update
#[test]
fn a_step_with_its_effects_last_tells_the_update_before_the_recipe() {
    use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
    const QUEST: u32 = 0x4000_00D0;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    for last in [true, false] {
        let (mut h, b, _me, world) = in_world();
        h.server.quests = QuestTable {
            quests: vec![QuestDef {
                id: QUEST,
                name: (0xA860_7CEC, 0x2500_17EE),
                description: (0x6C45_9364, 0x2500_17EE),
                icon: 0x4100_034B,
                play_fx: true,
                journals: Vec::new(),
                challenge: -999,
                duration: 72000.0,
                retry: 0.0,
                xp: 0,
                cancellable: false,
                placed: "authored for a test".into(),
                hidden: false,
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
        let step = Step {
            quest: QUEST,
            needs: 0,
            trigger: Trigger::Use(0x4700_2FA9),
            tell: None,
            update: Update::Bestow,
            xp: 0,
            gives: Vec::new(),
            counts: Vec::new(),
            effects: vec![GRANT_MINES_RECIPE],
            text: None,
            placed: "authored for a test".into(),
            requires: Vec::new(),
            takes: Vec::new(),
            races: Vec::new(),
            effects_last: last,
            wants: None,
            fellows: false,
            to: None,
        };
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        yalain_world::quests::run(&mut h.server, &mut h.net, world, &step, None, clock);
        h.run(2);
        let order: Vec<u32> = calls_of(&h, b, from)
            .into_iter()
            .map(|(f, _)| f)
            .filter(|f| *f == UPDATE_QUEST || *f == function::ADD_RECIPE)
            .collect();
        let want = if last {
            [UPDATE_QUEST, function::ADD_RECIPE]
        } else {
            [function::ADD_RECIPE, UPDATE_QUEST]
        };
        assert_eq!(order, want, "effects last: {last}");
    }
}

/// Drawing a surveyor's map (a recipe whose outcome sets its region's wait) has all five of the
/// region's map recipes wait five minutes, told in one call after the recipe's text and before the
/// drawn recipe's own record and the answer.
/// Behaviour: server.craft.map-recipes-wait-together
#[test]
fn drawing_a_map_has_its_regions_five_map_recipes_wait_five_minutes_together() {
    use yalain_world::craft::{
        map_region_set_by, Action, Check, Outcome, RecipeDef, MAP_RECIPES, MAP_RECIPES_WAIT,
    };
    let (mut h, b, _me, world) = in_world();
    let (set, _, five) = MAP_RECIPES[0];
    assert_eq!(map_region_set_by(set), Some(0));
    let drawn = five[2];
    h.server.crafting.recipes.insert(
        drawn,
        RecipeDef {
            did: drawn,
            skill: QUEST_RECIPES,
            charges: -1,
            recovery: 300.0,
            animation: 0x4100_00A4,
            cycles: 1,
            checks: vec![Check {
                upto: 2.0,
                outcomes: vec![Outcome {
                    upto: 1000.0,
                    xp_mod: 1.0,
                    actions: vec![
                        Action::Text {
                            text: (0x53FE_2D25, 0x2500_1758),
                            kind: 8,
                        },
                        Action::Effect {
                            state: set,
                            spellcraft: 1.0,
                        },
                    ],
                }],
            }],
            ..RecipeDef::default()
        },
    );
    // The player knows two of the five; the list carries all five all the same.
    {
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.play.recipes.push(yalain_world::craft::new_recipe(drawn));
        p.play
            .recipes
            .push(yalain_world::craft::new_recipe(five[4]));
    }
    let from = execute(&mut h, b, drawn, &[]);
    wait(&mut h, 8.0);
    let now = h.net.game_time(h.now);
    // In the order sent: a larger message may arrive after smaller ones sent later.
    let mut sent: Vec<(u64, ServerMessage)> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world && r.queue == queue::EVENT)
        .filter_map(|r| Some((r.blob_id & 0xFFFF_FFFF, r.message()?)))
        .collect();
    sent.sort_by_key(|(n, _)| *n);
    let got: Vec<(u32, Vec<Value>)> = sent
        .iter()
        .filter_map(|(_, m)| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect();
    let at = |f: u32| got.iter().position(|(x, _)| *x == f);
    let text = got
        .iter()
        .position(|(f, v)| *f == 0x8205_0011 && v.last() == Some(&Value::Int(8)))
        .expect("the text");
    let list = at(function::UPDATE_RECIPES).expect("the five");
    let own = at(function::UPDATE_RECIPE).expect("the drawn recipe's record");
    let done = at(function::EXECUTED).expect("the answer");
    assert!(text < list && list < own && own < done, "{got:x?}");
    assert_eq!(
        objects(&got[list].1),
        vec![(0x1D9, 12); 5],
        "five recipe records"
    );
    let p = h.server.players.get(&world).expect("in the world");
    for r in five {
        let rec = p.play.recipes.iter().find(|x| x.recipe == r).expect("held");
        assert!(
            (rec.use_again - (now + MAP_RECIPES_WAIT)).abs() < 10.0,
            "{r:08X} waits five minutes"
        );
    }
}

/// A recipe with charges made is marked used at that moment, one charge fewer, and its record told
/// with the time of the telling.
/// Behaviour: server.craft.charged-recipe-marked-used
#[test]
fn a_recipe_with_charges_made_is_marked_used_now_with_a_charge_fewer() {
    use yalain_world::craft::{new_recipe, Action, Check, Outcome, RecipeDef};
    const CHARGED: u32 = 0x6F00_2001;
    let (mut h, b, _me, world) = in_world();
    h.server.crafting.recipes.insert(
        CHARGED,
        RecipeDef {
            did: CHARGED,
            skill: QUEST_RECIPES,
            charges: 3,
            animation: 0x4100_00A4,
            cycles: 1,
            checks: vec![Check {
                upto: 2.0,
                outcomes: vec![Outcome {
                    upto: 1000.0,
                    xp_mod: 1.0,
                    actions: vec![Action::Text {
                        text: (0x53FE_2D25, 0x2500_1758),
                        kind: 8,
                    }],
                }],
            }],
            ..RecipeDef::default()
        },
    );
    {
        let p = h.server.players.get_mut(&world).expect("in the world");
        let mut r = new_recipe(CHARGED);
        r.charges = 3;
        p.play.recipes.push(r);
    }
    let from = execute(&mut h, b, CHARGED, &[]);
    wait(&mut h, 8.0);
    let now = h.net.game_time(h.now);
    assert!(calls_of(&h, b, from)
        .iter()
        .any(|(f, _)| *f == function::UPDATE_RECIPE));
    let p = h.server.players.get(&world).expect("in the world");
    let r = p
        .play
        .recipes
        .iter()
        .find(|r| r.recipe == CHARGED)
        .expect("held");
    assert_eq!(r.charges, 2);
    assert!(
        r.use_again > now - 10.0 && r.use_again <= now,
        "{}",
        r.use_again
    );
}

/// A made item its recipe's outcome binds (on its ordinal, the spinner within the action's range)
/// binds when used in combat; outside the range it does not.
/// Behaviour: server.craft.made-item-binds-on-use
#[test]
fn a_made_item_its_recipe_binds_is_marked_to_bind_when_used_in_combat() {
    use yalain_world::craft::Action;
    for (spinners, marked) in [((1, 100), true), ((20, 100), false)] {
        let (mut h, b, _, world) = in_world();
        if h.server.catalog.get(yalain_testkit::KNIFE).is_none() {
            h.server.catalog.insert(yalain_testkit::test_template(
                yalain_testkit::KNIFE,
                0x200,
                yalain_content::Props::default(),
            ));
        }
        if let Some(r) = h.server.crafting.recipes.get_mut(&KNIFE_RECIPE) {
            for o in r.checks.iter_mut().flat_map(|c| c.outcomes.iter_mut()) {
                o.actions.push(Action::Bind {
                    ordinal: 10_001,
                    spinners,
                });
            }
        }
        execute_at(&mut h, b, KNIFE_RECIPE, &[], 17);
        h.run(4);
        wait(&mut h, 9.0);
        let knife = h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == yalain_testkit::KNIFE)
            .map(|i| i.id)
            .expect("the knife made");
        assert_eq!(
            h.server.things.get(knife).map(|t| t.binds_on_use),
            Some(marked),
            "spinners {spinners:?}"
        );
    }
}

/// A made item carries the effects its recipe's outcome gives it for whoever wears or wields it, at
/// the spinner the player chose (through the recipe's level table): wielded, the knife puts its
/// bonus on its holder at that spellcraft.
/// Behaviour: server.craft.made-item-bonus-at-the-spinner
#[test]
fn a_made_item_carries_its_outcomes_bonus_at_the_spinner_chosen_and_puts_it_on_when_wielded() {
    use yalain_world::craft::{item_effect, Action, LevelMap};
    const TABLE: u32 = 0x6F00_0EBA;
    let (mut h, b, me, world) = in_world();
    if h.server.catalog.get(yalain_testkit::KNIFE).is_none() {
        h.server.catalog.insert(yalain_testkit::test_template(
            yalain_testkit::KNIFE,
            0x200,
            yalain_content::Props::default(),
        ));
    }
    if let Some(r) = h.server.crafting.recipes.get_mut(&KNIFE_RECIPE) {
        for o in r.checks.iter_mut().flat_map(|c| c.outcomes.iter_mut()) {
            o.actions.push(Action::ItemEffect {
                ordinal: 10_001,
                state: yalain_testkit::WORN_HEALTH_RATE_EFFECT,
                kind: item_effect::WORN,
                spellcraft: 0.0,
                by_spinner: Some(TABLE),
            });
        }
    }
    // A table that maps each spinner to itself, as every such table of 2017's recipes does.
    h.server.crafting.spinner_tables.insert(
        TABLE,
        LevelMap {
            first: 1,
            values: (1..=150).collect(),
        },
    );
    let from = execute_at(&mut h, b, KNIFE_RECIPE, &[], 17);
    h.run(4);
    wait(&mut h, 9.0);
    assert!(calls_of(&h, b, from)
        .iter()
        .any(|(f, _)| *f == function::EXECUTED));
    let knife = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == yalain_testkit::KNIFE)
        .map(|i| i.id)
        .expect("the knife made");
    assert_eq!(
        h.server.things.get(knife).map(|t| t.bonuses.clone()),
        Some(vec![(yalain_testkit::WORN_HEALTH_RATE_EFFECT, 17.0)]),
        "its own bonus at the spinner"
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::requests::function::USE,
            &[yalain_testkit::play::use_of(knife, me)],
        ),
    );
    h.run(10);
    let on: Vec<(u32, f32, u64)> = h.server.players[&world]
        .play
        .effects
        .iter()
        .filter(|e| e.flags == effects::WORN)
        .map(|e| (e.state, e.spellcraft, e.item))
        .collect();
    assert_eq!(
        on,
        [(yalain_testkit::WORN_HEALTH_RATE_EFFECT, 17.0, knife)],
        "on its holder at that spellcraft"
    );
}

/// A recipe def for these tests: one text, the charges and targets given.
fn charged(did: u32, charges: i32, targets: Vec<u32>) -> yalain_world::craft::RecipeDef {
    use yalain_world::craft::{Action, Check, Outcome, RecipeDef};
    RecipeDef {
        did,
        skill: QUEST_RECIPES,
        charges,
        targets,
        animation: 0x4100_00A4,
        cycles: 1,
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![Action::Text {
                    text: (0x53FE_2D25, 0x2500_1758),
                    kind: 8,
                }],
            }],
        }],
        ..RecipeDef::default()
    }
}

/// The answer a craft got: its error, the last answer to the recipe since `from`.
fn answer(h: &Harness, b: usize, from: usize) -> Option<u32> {
    calls_of(h, b, from)
        .iter()
        .rev()
        .find(|(f, _)| *f == function::EXECUTED)
        .and_then(|(_, v)| match v.get(1) {
            Some(Value::Int(e)) => Some(*e),
            _ => None,
        })
}

/// Ask to make a recipe at a target.
fn execute_on(h: &mut Harness, b: usize, recipe: u32, target: u64) -> usize {
    let mut g = yalain_world::graph::Builder::new();
    g.native(8, yalain_world::graph::hash(Elem::Word, Elem::Long, 0, &[]));
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::EXECUTE,
            &[
                Arg::LongInt(target),
                Arg::Object(g.graph()),
                Arg::Int(0),
                Arg::Int(recipe),
            ],
        ),
    );
    from
}

/// A recipe with charges comes with them when it is learned, granted again has them all again, and
/// with none left is refused with the answer for that, not the one for a recipe still waiting.
/// Behaviour: server.craft.charges-come-with-a-recipe
#[test]
fn a_recipe_learned_has_its_charges_and_granted_again_has_them_all_again() {
    const CHARGED: u32 = 0x6F00_2002;
    let (mut h, b, _me, world) = in_world();
    h.server
        .crafting
        .recipes
        .insert(CHARGED, charged(CHARGED, 2, Vec::new()));
    let charges = |h: &Harness| {
        let p = h.server.players.get(&world).expect("in the world");
        p.play.recipes.iter().find(|r| r.recipe == CHARGED).map(|r| r.charges)
    };
    let clock = h.clock();
    yalain_world::craft::learn(&mut h.server, &mut h.net, world, CHARGED, clock);
    assert_eq!(charges(&h), Some(2));
    for left in [1, 0] {
        let from = execute(&mut h, b, CHARGED, &[]);
        wait(&mut h, 8.0);
        assert_eq!(answer(&h, b, from), Some(0));
        assert_eq!(charges(&h), Some(left));
    }
    let from = execute(&mut h, b, CHARGED, &[]);
    wait(&mut h, 1.0);
    assert_eq!(answer(&h, b, from), Some(error::NO_CHARGES));
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::craft::learn(&mut h.server, &mut h.net, world, CHARGED, clock);
    h.run(2);
    assert_eq!(charges(&h), Some(2));
    let got = calls_of(&h, b, from);
    assert!(got.iter().any(|(f, _)| *f == function::UPDATE_RECIPE), "told");
    assert!(!got.iter().any(|(f, _)| *f == function::ADD_RECIPE), "known");
}

/// A recipe that asks for a target is made only at an object of a template it names: at nothing, or
/// at anything else, it is refused with the answer for that and nothing is spent.
/// Behaviour: server.craft.target-asked-for
#[test]
fn a_recipe_asking_for_a_target_is_made_at_one_of_its_own_and_nowhere_else() {
    const AT_MINE: u32 = 0x6F00_2003;
    const MINE: u32 = 0x4700_0B3D;
    const THE_MINE: u64 = 0x4079_9500_0000_000C;
    const ELSE: u64 = 0x4079_9500_0000_000D;
    let (mut h, b, _me, world) = in_world();
    let like = |id, template| yalain_content::ContentObject {
        object: Some(id),
        template,
        description: yalain_content::Description {
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_1B44)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [61.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: yalain_content::Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    };
    h.server.content.objects.push(like(THE_MINE, MINE));
    h.server.content.objects.push(like(ELSE, MINE + 1));
    h.server
        .crafting
        .recipes
        .insert(AT_MINE, charged(AT_MINE, 3, vec![MINE]));
    let clock = h.clock();
    yalain_world::craft::learn(&mut h.server, &mut h.net, world, AT_MINE, clock);
    for target in [0, ELSE] {
        let from = execute_on(&mut h, b, AT_MINE, target);
        wait(&mut h, 1.0);
        assert_eq!(answer(&h, b, from), Some(error::WRONG_TARGET), "{target:X}");
    }
    let from = execute_on(&mut h, b, AT_MINE, THE_MINE);
    wait(&mut h, 8.0);
    assert_eq!(answer(&h, b, from), Some(0));
    let p = h.server.players.get(&world).expect("in the world");
    let r = p.play.recipes.iter().find(|r| r.recipe == AT_MINE).expect("held");
    assert_eq!(r.charges, 2, "the refusals spent nothing");
}
