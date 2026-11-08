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
/// A recipe whose product this world can't make uses up nothing: its ingredients stay and it is
/// answered as failed.
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn a_recipe_whose_product_cannot_be_made_uses_up_nothing() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    h.server.catalog.templates.remove(&MINES_SWORD);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    wait(&mut h, 5.0);
    let p = h.server.players.get(&world).expect("in the world");
    assert!(
        p.play
            .items
            .iter()
            .any(|i| i.id == iron && i.quantity == 20),
        "kept"
    );
    assert!(!p.play.items.iter().any(|i| i.template == MINES_SWORD));
    assert!(
        calls_of(&h, b, from)
            .iter()
            .any(|(f, v)| *f == function::EXECUTED && v.contains(&Value::Int(MINES_RECIPE))),
        "answered"
    );
}

/// Behaviour: server.craft.refused
#[test]
fn making_a_recipe_while_attacking_is_refused_with_the_answer_alone() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.attacking = Some(yalain_world::combat::Run {
            walking: false,
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
    for t in h.server.crafting.templates() {
        if h.server.catalog.get(t).is_none() {
            let props = yalain_content::Props::default();
            h.server
                .catalog
                .insert(yalain_testkit::test_template(t, 0x200, props));
        }
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
                timed_phase: 0,
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
            rooms: Vec::new(),
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
            levels: None,
            xp_share: 0.0,
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

/// A made item its recipe's outcome attunes (on its ordinal, the spinner within the action's range)
/// is attuned to whoever holds it; outside the range it is not, and it may be put down.
/// Behaviour: server.craft.made-item-attuned
#[test]
fn a_made_item_its_recipe_attunes_is_attuned_when_the_spinner_is_in_range() {
    use yalain_world::craft::Action;
    for (spinners, attuned) in [((1, 100), true), ((20, 100), false)] {
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
                o.actions.push(Action::Attune {
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
        let t = h.server.things.get(knife).expect("made");
        assert_eq!(t.own.attuned, attuned, "spinners {spinners:?}");
        assert_eq!(
            yalain_world::inventory::attuned(&h.server.catalog, t),
            attuned
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
    h.server.catalog.implements.insert(
        yalain_testkit::KNIFE,
        yalain_world::skill_use::implement::DAGGER,
    );
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
        p.play
            .recipes
            .iter()
            .find(|r| r.recipe == CHARGED)
            .map(|r| r.charges)
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
    assert!(
        got.iter().any(|(f, _)| *f == function::UPDATE_RECIPE),
        "told"
    );
    assert!(
        !got.iter().any(|(f, _)| *f == function::ADD_RECIPE),
        "known"
    );
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
            name_text: None,
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
    let r = p
        .play
        .recipes
        .iter()
        .find(|r| r.recipe == AT_MINE)
        .expect("held");
    assert_eq!(r.charges, 2, "the refusals spent nothing");
}

/// A recipe that asks for a target is made within reach of it: a mine across the world's no target.
///
/// Behaviour: server.craft.a-target-is-in-reach
#[test]
fn a_recipe_asking_for_a_target_is_refused_past_its_reach() {
    const AT_MINE: u32 = 0x6F00_0FF0;
    const MINE: u32 = 0x4700_0B3D;
    const THE_MINE: u64 = 0x4079_9500_0000_000C;
    let (mut h, b, _me, world) = in_world();
    let here = h.server.players[&world].position;
    h.server
        .content
        .objects
        .push(yalain_content::ContentObject {
            object: Some(THE_MINE),
            template: MINE,
            description: yalain_content::Description {
                name_text: None,
                package: 0x103E,
                look: 0x1F00_0100,
                enter_fx: 0,
                scale: None,
                name: Some((0xA860_7CEC, 0x2500_1B44)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: here.cell,
            origin: here.origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: yalain_content::Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: yalain_content::Facts::default(),
            walks: None,
            gate: None,
        });
    h.server
        .crafting
        .recipes
        .insert(AT_MINE, charged(AT_MINE, 9, vec![MINE]));
    let clock = h.clock();
    yalain_world::craft::learn(&mut h.server, &mut h.net, world, AT_MINE, clock);
    let stand = |h: &mut Harness, cell: u32, x: f32| {
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.position.cell = cell;
        p.position.origin[0] = here.origin[0] + x;
    };
    let reach = yalain_world::craft::TARGET_REACH;
    stand(&mut h, here.cell, reach + 0.5);
    let from = execute_on(&mut h, b, AT_MINE, THE_MINE);
    wait(&mut h, 1.0);
    assert_eq!(
        answer(&h, b, from),
        Some(error::WRONG_TARGET),
        "past its reach"
    );
    stand(&mut h, 0x0100_0001, 0.0);
    let from = execute_on(&mut h, b, AT_MINE, THE_MINE);
    wait(&mut h, 1.0);
    assert_eq!(
        answer(&h, b, from),
        Some(error::WRONG_TARGET),
        "another block"
    );
    stand(&mut h, here.cell, reach - 0.5);
    let from = execute_on(&mut h, b, AT_MINE, THE_MINE);
    wait(&mut h, 8.0);
    assert_eq!(answer(&h, b, from), Some(0), "within it");
}

/// A hidden quest given starts the step waiting on its giving, as a shown one's update and a recipe
/// after it; the hidden quest kept from a player without what it requires starts nothing.
/// Behaviour: server.quests.step-on-a-quest-given
#[test]
fn a_quest_given_runs_the_step_waiting_on_it_and_one_withheld_runs_none() {
    use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
    use yalain_world::quests::{moves, status, Move, QuestEffect};
    use yalain_world::state::QuestState;
    const TOLL: u32 = 0x4000_0183;
    const GUARD: u32 = 0x4000_0184;
    const MINE: u32 = 0x4000_0185;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    let def = |id, hidden, required| QuestDef {
        id,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: Vec::new(),
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required,
    };
    for paid in [false, true] {
        let (mut h, b, _me, world) = in_world();
        h.server.quests = QuestTable {
            quests: vec![
                def(TOLL, true, None),
                def(GUARD, true, Some((TOLL, status::UNDERWAY))),
                def(MINE, false, None),
            ],
            steps: vec![Step {
                quest: MINE,
                needs: 0,
                trigger: Trigger::Given(GUARD),
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
                effects_last: true,
                wants: None,
                fellows: false,
                levels: None,
                xp_share: 0.0,
                to: None,
            }],
            gives: Vec::new(),
            rooms: Vec::new(),
        };
        if paid {
            let p = h.server.players.get_mut(&world).expect("in the world");
            p.play.quests.push(QuestState {
                quest: TOLL,
                status: status::UNDERWAY,
                phase: 1,
                started: 0,
                updated: 0,
            });
        }
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        let e = QuestEffect {
            quest: GUARD,
            act: Move::Bestow,
        };
        moves(&mut h.server, &mut h.net, world, e, true, clock);
        h.run(2);
        let order: Vec<u32> = calls_of(&h, b, from)
            .into_iter()
            .map(|(f, _)| f)
            .filter(|f| *f == UPDATE_QUEST || *f == function::ADD_RECIPE)
            .collect();
        if paid {
            assert_eq!(
                order,
                [UPDATE_QUEST, function::ADD_RECIPE],
                "only the shown one told"
            );
        } else {
            assert!(order.is_empty(), "{order:?}");
        }
    }
}

/// A recipe with an item-level spinner, as the final data writes them: coal from 20, iron from 1,
/// each in the amount its own table gives at the value chosen, used up by the same tables.
const LEVEL_RECIPE: u32 = 0x6F00_7001;
const COAL: u32 = 0x4700_7001;
const LEVEL_SWORD: u32 = 0x4700_7002;
const COAL_TABLE: u32 = 0x6F00_7002;
const IRON_TABLE: u32 = 0x6F00_7003;
const FORGING: u32 = 0x6F00_7004;

fn level_recipe(needs_workshop: bool) -> yalain_world::craft::RecipeDef {
    use yalain_world::craft::{Action, Check, Ingredient, Outcome, RecipeDef};
    RecipeDef {
        did: LEVEL_RECIPE,
        skill: TOOLS,
        category: 10,
        charges: -1,
        animation: 0x4100_00A4,
        cycles: 2,
        spinner: Some((1, 100)),
        required_effect: needs_workshop.then_some(FORGING),
        ingredients: vec![
            Ingredient {
                ordinal: 1,
                quantity: 1,
                items: vec![COAL],
                spinners: Some((20, 100)),
                stack_table: Some(COAL_TABLE),
                ..Ingredient::default()
            },
            Ingredient {
                ordinal: 2,
                quantity: 1,
                items: vec![IRON],
                spinners: Some((1, 100)),
                stack_table: Some(IRON_TABLE),
                ..Ingredient::default()
            },
        ],
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![
                    Action::Produce {
                        template: LEVEL_SWORD,
                        quantity: 1,
                        ordinal: 10_001,
                    },
                    Action::Destroy {
                        ordinal: 1,
                        quantity: 1,
                        by_spinner: Some(COAL_TABLE),
                        spinners: Some((20, 100)),
                    },
                    Action::Destroy {
                        ordinal: 2,
                        quantity: 1,
                        by_spinner: Some(IRON_TABLE),
                        spinners: Some((1, 100)),
                    },
                ],
            }],
        }],
        ..RecipeDef::default()
    }
}

/// A player who knows the level recipe: `iron` iron and `coal` coal in the pack (their stacks,
/// iron first).
fn level_player(
    needs_workshop: bool,
    iron: u32,
    coal: u32,
) -> (Harness, usize, yalain_net::ConnId, u64, u64) {
    use yalain_world::craft::LevelMap;
    let (mut h, b, _, world) = in_world();
    h.server
        .crafting
        .recipes
        .insert(LEVEL_RECIPE, level_recipe(needs_workshop));
    let table = |first: u32, values: &[u32]| LevelMap {
        first,
        values: values.to_vec(),
    };
    h.server
        .crafting
        .spinner_tables
        .insert(COAL_TABLE, table(20, &[240, 400]));
    h.server
        .crafting
        .spinner_tables
        .insert(IRON_TABLE, table(1, &[26, 33, 41]));
    for t in [IRON, COAL, LEVEL_SWORD] {
        if h.server.catalog.get(t).is_none() {
            h.server.catalog.insert(yalain_testkit::test_template(
                t,
                0x200,
                yalain_content::Props::default(),
            ));
        }
    }
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .recipes
        .push(yalain_world::craft::new_recipe(LEVEL_RECIPE));
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, iron);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, COAL, coal);
    h.run(2);
    let stack = |h: &Harness, t: u32| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == t)
            .map_or(0, |i| i.id)
    };
    let (i, c) = (stack(&h, IRON), stack(&h, COAL));
    (h, b, world, i, c)
}

fn held(h: &Harness, world: yalain_net::ConnId, template: u32) -> u32 {
    h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == template)
        .map(|i| i.quantity)
        .sum()
}

/// The answers a craft's request came back with: (why, the recipe).
fn craft_answers(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::EXECUTED)
        .filter_map(|(_, v)| match v.get(1) {
            Some(Value::Int(e)) => Some(*e),
            _ => None,
        })
        .collect()
}

/// At item level 1 the recipe asks only for its iron, in the amount the table gives at 1; made,
/// the iron goes by that table and the coal in the pack stays.
/// Behaviour: server.craft.spinner-chooses-the-ingredients
#[test]
fn an_item_level_recipe_takes_only_the_ingredients_its_spinner_asks_for() {
    let (mut h, b, world, iron, _) = level_player(false, 100, 500);
    let from = execute_at(&mut h, b, LEVEL_RECIPE, &[(2, iron)], 1);
    wait(&mut h, 6.0);
    assert_eq!(craft_answers(&h, b, from), [0], "made");
    assert_eq!(held(&h, world, IRON), 100 - 26, "the table's 26 at level 1");
    assert_eq!(held(&h, world, COAL), 500, "not asked for at level 1");
    assert_eq!(held(&h, world, LEVEL_SWORD), 1);
}

/// A higher item level asks for what its range now holds, each in its table's amount at that level.
/// Behaviour: server.craft.spinner-sets-the-amounts-used
#[test]
fn a_higher_item_level_takes_more_ingredients_in_its_tables_amounts() {
    let (mut h, b, world, iron, coal) = level_player(false, 100, 500);
    let from = execute_at(&mut h, b, LEVEL_RECIPE, &[(1, coal), (2, iron)], 21);
    wait(&mut h, 6.0);
    assert_eq!(craft_answers(&h, b, from), [0], "made");
    assert_eq!(held(&h, world, COAL), 500 - 400, "the coal table at 21");
    assert_eq!(
        held(&h, world, IRON),
        100 - 41,
        "the iron table past its end"
    );
}

/// A slot the spinner asks for left empty, and a slot it does not ask for filled, are invalid
/// ingredients; nothing is used up.
/// Behaviour: server.craft.slots-must-match-the-spinner
#[test]
fn the_slots_filled_must_be_the_ones_the_spinner_asks_for() {
    let (mut h, b, world, iron, coal) = level_player(false, 100, 500);
    for (slots, spinner) in [
        (vec![(2, iron)], 20),
        (vec![(1, coal), (2, iron)], 1),
        (vec![(1, coal)], 20),
        // As many slots as it asks for, but not the one.
        (vec![(1, coal)], 1),
    ] {
        let from = execute_at(&mut h, b, LEVEL_RECIPE, &slots, spinner);
        h.run(4);
        assert_eq!(
            craft_answers(&h, b, from),
            [error::INVALID_INGREDIENTS],
            "{slots:?} at {spinner}"
        );
    }
    assert_eq!((held(&h, world, IRON), held(&h, world, COAL)), (100, 500));
}

/// Too few of the amount the spinner asks for is refused with why, both as the ingredient goes in
/// and at the making, though a stack of the recipe's fixed quantity would do.
/// Behaviour: server.craft.required-amount-by-the-spinner
#[test]
fn too_few_for_the_spinners_amount_is_refused_with_why() {
    let (mut h, b, world, iron, _) = level_player(false, 25, 0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::ADD_INGREDIENT,
            &[
                Arg::Int(1),
                Arg::LongInt(iron),
                Arg::Int(2),
                Arg::Int(LEVEL_RECIPE),
            ],
        ),
    );
    h.run(4);
    let answers: Vec<_> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::INGREDIENT_ADDED)
        .collect();
    assert_eq!(answers[0].1[0], Value::Int(error::TOO_FEW));
    let from = execute_at(&mut h, b, LEVEL_RECIPE, &[(2, iron)], 1);
    h.run(4);
    assert_eq!(craft_answers(&h, b, from), [error::TOO_FEW]);
    assert_eq!(held(&h, world, IRON), 25);
}

/// A spinner outside the recipe's range makes nothing, even though it asks for nothing.
/// Behaviour: server.craft.spinner-within-the-recipes-range
#[test]
fn a_spinner_outside_the_recipes_range_makes_nothing() {
    let (mut h, b, world, _, _) = level_player(false, 100, 500);
    let from = execute_at(&mut h, b, LEVEL_RECIPE, &[], 0);
    wait(&mut h, 6.0);
    assert_eq!(craft_answers(&h, b, from), [error::BAD_RECIPE]);
    assert_eq!(held(&h, world, LEVEL_SWORD), 0);
}

/// A recipe that needs a workshop is refused without its record, with the answer alone, and made
/// with it.
/// Behaviour: server.craft.needs-its-workshop
#[test]
fn a_recipe_that_needs_a_workshop_is_refused_away_from_one() {
    let (mut h, b, world, iron, _) = level_player(true, 100, 0);
    let from = execute_at(&mut h, b, LEVEL_RECIPE, &[(2, iron)], 1);
    wait(&mut h, 6.0);
    assert_eq!(craft_answers(&h, b, from), [error::MISSING_EFFECT]);
    assert_eq!(held(&h, world, IRON), 100);
    h.server
        .effect_table
        .effects
        .insert(FORGING, yalain_testkit::plain_effect(FORGING));
    let (cell, origin) = (0x5766_0032, [131.1, 36.0, 190.1]);
    h.server.workshops.place(
        0x4000_0000_0000_0001,
        yalain_world::presence::land_cell(cell, origin),
        FORGING,
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            cell,
            auberean_primitives::Vec3::new(origin[0], origin[1], origin[2]),
            0,
            yalain_world::world::IDLE_MOTION,
            0,
        ),
    );
    h.run(4);
    let from = execute_at(&mut h, b, LEVEL_RECIPE, &[(2, iron)], 1);
    wait(&mut h, 6.0);
    assert_eq!(craft_answers(&h, b, from), [0]);
    assert_eq!(held(&h, world, IRON), 100 - 26);
}

/// A slot that takes a class takes every template the world makes of a package deriving from it,
/// and no other.
/// Behaviour: server.craft.ingredient-classes
#[test]
fn a_slot_that_takes_a_class_takes_the_templates_deriving_from_it() {
    use yalain_world::craft::{Crafting, Ingredient, RecipeDef};
    let mut c = Crafting::default();
    c.recipes.insert(
        1,
        RecipeDef {
            did: 1,
            ingredients: vec![Ingredient {
                ordinal: 1,
                classes: vec![10],
                ..Ingredient::default()
            }],
            ..RecipeDef::default()
        },
    );
    c.resolve_classes(&[(100, 11), (101, 12), (102, 10)], &|package, class| {
        package == class || (package == 11 && class == 10)
    });
    assert_eq!(c.recipes[&1].ingredients[0].items, [100, 102]);
}

/// The world holds what its recipes make and ask for, and the nodes surveying conjures: without
/// them a slot holds nothing and a delving stone conjures nothing.
/// Behaviour: server.craft.world-holds-what-crafting-needs
#[test]
fn crafting_asks_the_world_for_its_products_ingredients_and_conjured_nodes() {
    let c = yalain_testkit::test_crafting();
    let wanted = c.templates();
    assert!(wanted.contains(&MINES_SWORD), "a product");
    assert!(wanted.contains(&IRON), "an ingredient");
    for node in yalain_world::mining::SUMMONED_NODES {
        assert!(wanted.contains(&node), "{node:08X}");
    }
    assert_eq!(
        yalain_world::mining::summoned_by("Eff_SummonMine_Silver_Delving"),
        Some(0x4700_0C8E)
    );
}

/// A recipe of the test's own, known to the player, its craft skill at a level; its ingredients'
/// templates and its product in the catalog.
fn with_own_recipe(
    h: &mut Harness,
    world: yalain_net::ConnId,
    def: yalain_world::craft::RecipeDef,
    level: u32,
    templates: &[u32],
) {
    use yalain_world::state::RecipeState;
    for t in templates {
        if h.server.catalog.get(*t).is_none() {
            h.server.catalog.insert(yalain_testkit::test_template(
                *t,
                0x200,
                yalain_content::Props::default(),
            ));
        }
    }
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.play.recipes.push(RecipeState {
        recipe: def.did,
        use_again: -1.0,
        successes: 0,
        charges: -1,
        last_spinner: 0,
    });
    if let Some(c) = p
        .play
        .craft_skills
        .iter_mut()
        .find(|c| c.skill == def.skill)
    {
        c.level = level;
    }
    h.server.crafting.recipes.insert(def.did, def);
}

const STEEL_RECIPE: u32 = 0x6F00_10AA;
const STEEL: u32 = 0x4700_0B40;
const WOOD: u32 = 0x4700_06C5;

/// Refining 100 of each of two materials, its craft skill paying `xp` a use.
fn steel_recipe() -> yalain_world::craft::RecipeDef {
    use yalain_world::craft::{Action, Check, Custom, Ingredient, Outcome, RecipeDef};
    let destroy = |ordinal| Action::Destroy {
        ordinal,
        quantity: 100,
        by_spinner: None,
        spinners: None,
    };
    let slot = |ordinal, item| Ingredient {
        ordinal,
        quantity: 100,
        items: vec![item],
        ..Ingredient::default()
    };
    RecipeDef {
        did: STEEL_RECIPE,
        skill: TOOLS,
        charges: -1,
        animation: 0x4100_014B,
        cycles: 3,
        ingredients: vec![slot(1, IRON), slot(2, WOOD)],
        custom: Custom::Refine { product: STEEL },
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![Action::Custom, destroy(1), destroy(2)],
            }],
        }],
        ..RecipeDef::default()
    }
}

/// Run a refining with the materials in the pack: what the answer and the lines said, by the stack
/// of the product it left.
fn refine(h: &mut Harness, b: usize, world: yalain_net::ConnId) -> (u32, usize) {
    let iron =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, 125).expect("iron");
    let wood =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, WOOD, 300).expect("wood");
    h.run(2);
    let from = execute(h, b, STEEL_RECIPE, &[(1, iron), (2, wood)]);
    h.run(4);
    wait(h, 9.0);
    let p = &h.server.players[&world];
    let held = |t: u32| {
        p.play
            .items
            .iter()
            .find(|i| i.template == t)
            .map(|i| i.quantity)
    };
    assert_eq!(held(IRON), Some(25), "the iron used up");
    assert_eq!(held(WOOD), Some(200), "the wood used up");
    (held(STEEL).unwrap_or(0), from)
}

/// Refining adds a third of what it takes to the player's stack of its product at the craft skill's
/// lowest levels, says how many, uses up what its outcome says and pays what the skill pays a use.
/// Behaviour: server.craft.refining-yields
#[test]
fn refining_adds_a_third_of_what_it_takes_to_the_players_stack_and_says_how_many() {
    let (mut h, b, _me, world) = in_world();
    for d in &mut h.server.crafting.skills {
        if d.did == TOOLS {
            d.usage_xp = Some(yalain_world::craft::LevelMap {
                first: 1,
                values: vec![2, 2, 2],
            });
        }
    }
    let mut steel_template =
        yalain_testkit::test_template(STEEL, 0x200, yalain_content::Props::default());
    steel_template.facts.name = Some((0xA860_7CEC, 0x2500_1B14));
    h.server.catalog.insert(steel_template);
    with_own_recipe(&mut h, world, steel_recipe(), 2, &[IRON, WOOD, STEEL]);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, STEEL, 4);
    let (steel, from) = refine(&mut h, b, world);
    assert_eq!(steel, 38, "4 and 34 made (2017: 4 to 38, 28 to 62)");
    let got = calls_of(&h, b, from);
    let said = got.iter().any(|(f, v)| {
        *f == 0x8205_0011
            && v.iter().any(|x| match x {
                Value::Graph(g) => g.iter().any(|(_, n)| match n {
                    Node::TextInfo(t) => {
                        t.string_id == yalain_world::craft::refined_text::TEXT.0
                            && t.variables.iter().any(|x| {
                                x.id == yalain_world::craft::refined_text::COUNT
                                    && x.value
                                        == auberean_script::native_object::VariableValue::Word(34)
                            })
                    }
                    _ => false,
                }),
                _ => false,
            })
    });
    assert!(said, "the line says how many: {got:x?}");
    let done = got
        .iter()
        .find(|(f, _)| *f == function::EXECUTED)
        .expect("the answer");
    assert_eq!(
        done.1,
        vec![Value::Int(1), Value::Int(0), Value::Int(STEEL_RECIPE)]
    );
    let c = h.server.players[&world]
        .play
        .craft_skills
        .iter()
        .find(|c| c.skill == TOOLS)
        .copied()
        .expect("its craft skill");
    assert_eq!(c.today, 2, "what the skill pays a use at its level");
}

/// What a refining yields grows with the craft skill to all it takes at the skill's cap, and what
/// the player's effects add to the skill counts.
/// Behaviour: server.craft.refining-grows-with-skill
#[test]
fn a_refining_yields_more_at_a_higher_craft_skill_and_with_what_effects_add_to_it() {
    use yalain_world::craft::refining_yield;
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    assert_eq!(refining_yield(100, 2), 34);
    assert_eq!(refining_yield(100, 1), 33);
    assert_eq!(refining_yield(100, 150), 100);
    assert_eq!(refining_yield(100, 500), 100, "no more past the cap");
    assert!(refining_yield(100, 75) > 34 && refining_yield(100, 75) < 100);
    assert_eq!(refining_yield(2500, 2), 845);

    const BONUS_STAT: u32 = 129 << 16 | 22;
    const CRUCIBLE: u32 = 0x6F00_17F3;
    let (mut h, b, me, world) = in_world();
    for d in &mut h.server.crafting.skills {
        if d.did == TOOLS {
            d.bonus_stat = BONUS_STAT;
        }
    }
    h.server.effect_table.effects.insert(
        CRUCIBLE,
        EffectDef {
            kind: Kind::Numeric {
                flags: yalain_world::effect_defs::numeric::ADD,
                stats: vec![BONUS_STAT],
                magnitude: Scale(vec![(1.0, 1.0), (50.0, 50.0)]),
            },
            ..yalain_testkit::plain_effect(CRUCIBLE)
        },
    );
    with_own_recipe(&mut h, world, steel_recipe(), 1, &[IRON, WOOD, STEEL]);
    let clock = h.clock();
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        CRUCIBLE,
        effects::Cast {
            caster: me,
            spellcraft: 75.0,
            ..effects::Cast::default()
        },
        clock,
    );
    let (steel, _) = refine(&mut h, b, world);
    assert!(
        steel > refining_yield(100, 40),
        "skill 1 and 50 from the effect yield more: {steel}"
    );
}

/// A recipe that works out its own product names none the data holds: it answers that it failed
/// with a line saying so, uses up nothing and pays nothing (2017: 9 of 9 card melds).
/// Behaviour: server.craft.own-product-makes-nothing
#[test]
fn a_recipe_working_out_its_own_product_fails_and_uses_up_nothing() {
    use yalain_world::craft::Custom;
    let (mut h, b, _me, world) = in_world();
    let def = yalain_world::craft::RecipeDef {
        custom: Custom::Unknown,
        ..steel_recipe()
    };
    with_own_recipe(&mut h, world, def, 2, &[IRON, WOOD, STEEL]);
    let iron =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, 100).expect("iron");
    let wood =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, WOOD, 100).expect("wood");
    h.run(2);
    let from = execute(&mut h, b, STEEL_RECIPE, &[(1, iron), (2, wood)]);
    h.run(4);
    wait(&mut h, 9.0);
    let got = calls_of(&h, b, from);
    let done = got
        .iter()
        .find(|(f, _)| *f == function::EXECUTED)
        .expect("the answer");
    assert_eq!(
        done.1,
        vec![
            Value::Int(1),
            Value::Int(error::FAILED),
            Value::Int(STEEL_RECIPE)
        ]
    );
    let p = &h.server.players[&world];
    let held = |t: u32| {
        p.play
            .items
            .iter()
            .find(|i| i.template == t)
            .map(|i| i.quantity)
    };
    assert_eq!(
        (held(IRON), held(WOOD), held(STEEL)),
        (Some(100), Some(100), None)
    );
    let c = p
        .play
        .craft_skills
        .iter()
        .find(|c| c.skill == TOOLS)
        .expect("skill");
    assert_eq!(c.today, 0, "it pays nothing");
}

/// Spellbinding makes a totem of the kind of the item taken apart.
/// Behaviour: server.craft.spellbinding-makes-a-totem
#[test]
fn spellbinding_makes_a_totem_of_the_kind_of_the_item_it_takes_apart() {
    use yalain_world::craft::{totem, Action, Custom};
    let cases = [
        (0x4700_207F_u32, true, false, totem::WEAPON),
        (0x4700_0DC0, false, true, totem::ARMOR),
        (0x4700_07E2, true, true, totem::SHIELD),
    ];
    for (source, weapon, armor, expected) in cases {
        let (mut h, b, _me, world) = in_world();
        let mut props = yalain_content::Props::default();
        if weapon {
            props.set("Weapon_Damage", yalain_content::Prop::Int(10));
        }
        if armor {
            props.set("Item_ArmorLevel", yalain_content::Prop::Int(10));
        }
        h.server
            .catalog
            .insert(yalain_testkit::test_template(source, 0x200, props));
        let def = yalain_world::craft::RecipeDef {
            custom: Custom::Spellbind,
            ingredients: vec![yalain_world::craft::Ingredient {
                ordinal: 1,
                quantity: 1,
                items: vec![source],
                ..Default::default()
            }],
            checks: vec![yalain_world::craft::Check {
                upto: 2.0,
                outcomes: vec![yalain_world::craft::Outcome {
                    upto: 1000.0,
                    xp_mod: 1.0,
                    actions: vec![
                        Action::Custom,
                        Action::Destroy {
                            ordinal: 1,
                            quantity: 1,
                            by_spinner: None,
                            spinners: None,
                        },
                    ],
                }],
            }],
            ..steel_recipe()
        };
        with_own_recipe(
            &mut h,
            world,
            def,
            2,
            &[totem::WEAPON, totem::ARMOR, totem::SHIELD],
        );
        let item = yalain_world::inventory::give(&mut h.server, &mut h.net, world, source, 1)
            .expect("the item");
        h.server.things.get_mut(item).expect("made").enchantments = vec![(ENCHANTMENT, 12.0)];
        h.server.things.get_mut(item).expect("made").own.min_level = Some(13);
        h.server.things.get_mut(item).expect("made").own.arcane_lore = Some(4);
        h.run(2);
        execute(&mut h, b, STEEL_RECIPE, &[(1, item)]);
        h.run(4);
        wait(&mut h, 9.0);
        let p = &h.server.players[&world];
        let made = p
            .play
            .items
            .iter()
            .find(|i| i.template == expected)
            .expect("a totem of its kind")
            .id;
        assert!(
            !p.play.items.iter().any(|i| i.template == source),
            "taken apart"
        );
        let t = h.server.things.get(made).expect("the totem");
        assert_eq!(t.enchantments, vec![(ENCHANTMENT, 12.0)], "what it took");
        assert_eq!((t.own.min_level, t.own.arcane_lore), (Some(13), Some(4)));
    }
}

/// An enchantment on an item some tests take apart.
const ENCHANTMENT: u32 = 0x6F00_0394;

/// A recipe taking an item apart into a totem, known to the player; returns the item.
fn spellbinding_ready(h: &mut Harness, world: yalain_net::ConnId, source: u32) -> u64 {
    use yalain_world::craft::{totem, Action, Custom};
    let mut props = yalain_content::Props::default();
    props.set("Weapon_Damage", yalain_content::Prop::Int(10));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(source, 0x200, props));
    let def = yalain_world::craft::RecipeDef {
        custom: Custom::Spellbind,
        ingredients: vec![yalain_world::craft::Ingredient {
            ordinal: 1,
            quantity: 1,
            items: vec![source],
            ..Default::default()
        }],
        checks: vec![yalain_world::craft::Check {
            upto: 2.0,
            outcomes: vec![yalain_world::craft::Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![
                    Action::Custom,
                    Action::Destroy {
                        ordinal: 1,
                        quantity: 1,
                        by_spinner: None,
                        spinners: None,
                    },
                ],
            }],
        }],
        ..steel_recipe()
    };
    with_own_recipe(
        h,
        world,
        def,
        2,
        &[totem::WEAPON, totem::ARMOR, totem::SHIELD],
    );
    let item = yalain_world::inventory::give(&mut h.server, &mut h.net, world, source, 1)
        .expect("the item");
    h.run(2);
    item
}

/// An item with no enchantment is not taken apart: nothing is lost and nothing made.
/// Behaviour: server.craft.spellbinding-takes-an-enchantment
#[test]
fn spellbinding_refuses_an_item_with_no_enchantment() {
    let (mut h, b, _me, world) = in_world();
    let item = spellbinding_ready(&mut h, world, 0x4700_207F);
    execute(&mut h, b, STEEL_RECIPE, &[(1, item)]);
    h.run(4);
    wait(&mut h, 9.0);
    let p = &h.server.players[&world];
    assert!(p.play.items.iter().any(|i| i.id == item), "still held");
    assert!(
        !p.play
            .items
            .iter()
            .any(|i| yalain_world::craft::totem::noun(i.template).is_some()),
        "no totem"
    );
}

/// A totem used on an item of its kind that has a free slot and what it asks puts its enchantment on
/// it and is gone; on anything else it is refused and nothing changes.
/// Behaviour: server.craft.totem-enchants-an-item
#[test]
fn a_totem_used_on_an_item_of_its_kind_enchants_it_and_is_gone() {
    use yalain_world::craft::totem;
    use yalain_world::made::Made;
    const SWORD_TEMPLATE: u32 = 0x4700_207F;
    const ROBE_TEMPLATE: u32 = 0x4700_0DC0;
    let (mut h, b, me, world) = in_world();
    let mut weapon = yalain_content::Props::default();
    weapon.set("Weapon_Damage", yalain_content::Prop::Int(10));
    let mut armor = yalain_content::Props::default();
    armor.set("Item_ArmorLevel", yalain_content::Prop::Int(10));
    for (t, p) in [(SWORD_TEMPLATE, weapon), (ROBE_TEMPLATE, armor)] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x200, p));
    }
    h.server.catalog.insert(yalain_testkit::test_template(
        totem::WEAPON,
        0x200,
        yalain_content::Props::default(),
    ));
    let give = |h: &mut Harness, t: u32| {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, 1).expect("given")
    };
    let totem_item = give(&mut h, totem::WEAPON);
    {
        let t = h.server.things.get_mut(totem_item).expect("made");
        t.enchantments = vec![(ENCHANTMENT, 12.0)];
        t.own.min_level = Some(10);
        t.own.made = Some(Made::default());
    }
    let sword = give(&mut h, SWORD_TEMPLATE);
    let low_sword = give(&mut h, SWORD_TEMPLATE);
    let full_sword = give(&mut h, SWORD_TEMPLATE);
    let robe = give(&mut h, ROBE_TEMPLATE);
    let set = |h: &mut Harness, id: u64, level: i32, slots: u32| {
        let t = h.server.things.get_mut(id).expect("made");
        t.own.min_level = Some(level);
        t.own.made = Some(Made {
            slots,
            ..Made::default()
        });
    };
    set(&mut h, sword, 12, 1);
    set(&mut h, low_sword, 9, 1);
    set(&mut h, full_sword, 12, 0);
    set(&mut h, robe, 12, 1);
    let try_it = |h: &mut Harness, target: u64| {
        use yalain_world::graph::{Builder, Slot};
        let mut g = Builder::new();
        g.object(
            0x245,
            &[
                Slot::Ref(None),
                Slot::Ref(None),
                Slot::Long(totem_item),
                Slot::Ref(None),
                Slot::Int(0),
                Slot::Long(me),
                Slot::Int((-1.0f32).to_bits()),
                Slot::Long(target),
                Slot::Int(0),
                Slot::Long(0),
                Slot::Int(0),
                Slot::Ref(None),
                Slot::Int(0),
                Slot::Int(316),
                Slot::Int(0),
                Slot::Int(0),
            ],
        );
        let call = yalain_testkit::play::server_call(
            yalain_world::requests::function::USE,
            &[auberean_script::args::Arg::Object(g.graph())],
        );
        h.bots[b].send(true, auberean_protocol::queue::WEENIE, &call);
        h.run(6);
    };
    let has_totem = |h: &Harness| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == totem_item)
    };
    for refused in [0, low_sword, full_sword, robe] {
        try_it(&mut h, refused);
        assert!(has_totem(&h), "refused: {refused:x}");
        if refused != 0 {
            assert!(h
                .server
                .things
                .get(refused)
                .expect("held")
                .enchantments
                .is_empty());
        }
    }
    try_it(&mut h, sword);
    assert!(!has_totem(&h), "the totem is gone");
    let t = h.server.things.get(sword).expect("held");
    assert_eq!(t.enchantments, vec![(ENCHANTMENT, 12.0)]);
    assert_eq!(t.own.made.expect("made").slots, 0, "it took a slot");
}

/// A totem's profile says what it goes on and the least its target must have, in place of the
/// least it asks of who wears it.
/// Behaviour: server.craft.totem-enchants-an-item
#[test]
fn a_totems_profile_says_what_it_goes_on() {
    use yalain_world::craft::totem;
    use yalain_world::profile::{item, item_line, TotemView};
    let view = yalain_world::profile::ItemView {
        totem: Some(TotemView {
            kind: totem::noun(totem::WEAPON).expect("a word"),
            least_level: Some(45),
            least_lore: None,
        }),
        ..yalain_world::profile::ItemView::default()
    };
    let bytes = item(0x3FF, &view).packed(&[]);
    let has = |id: u32| bytes.windows(4).any(|w| w == id.to_le_bytes());
    assert!(has(item_line::TOTEM_IMBUED) && has(item_line::TOTEM_LEVEL));
    assert!(!has(item_line::TOTEM_LORE));
}

const LEVEL_TABLE: u32 = yalain_world::made::ITEM_LEVEL_TABLE;
const PLAIN_QUALITY: u32 = 0x6F00_0EBC;
const SWORD: u32 = 0x4700_07B0;
const SMALL_SWORD_RECIPE: u32 = 0x6F00_0F71;
const HELM: u32 = 0x4700_07D8;

/// A recipe of a spinner making one template, with the mutations named, using `stack` of iron in
/// slot 1 and hide in slot 3 by a table of tens.
fn made_recipe(product: u32, tables: &[u32]) -> yalain_world::craft::RecipeDef {
    use yalain_world::craft::{Action, Check, Ingredient, Outcome, RecipeDef};
    const TENS: u32 = 0x6F00_0EC2;
    let slot = |ordinal, item| Ingredient {
        ordinal,
        quantity: 1,
        items: vec![item],
        spinners: Some((1, 100)),
        stack_table: Some(TENS),
        ..Ingredient::default()
    };
    let mut actions = vec![Action::Produce {
        template: product,
        quantity: 1,
        ordinal: 10_001,
    }];
    actions.extend(tables.iter().map(|t| Action::Mutate {
        ordinal: 10_001,
        table: Some(*t),
        spinners: Some((1, 100)),
    }));
    RecipeDef {
        did: SMALL_SWORD_RECIPE,
        skill: TOOLS,
        charges: -1,
        animation: 0x4100_014B,
        cycles: 3,
        spinner: Some((1, 150)),
        ingredients: vec![slot(1, IRON), slot(3, WOOD)],
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions,
            }],
        }],
        ..RecipeDef::default()
    }
}

/// An item a recipe makes without an item level, with a permanent effect, holds the effect after a
/// logout and is not given numbers by it.
/// Behaviour: server.craft.made-item-no-level-keeps-effects
#[test]
fn an_item_made_without_an_item_level_keeps_its_permanent_effect_across_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    use yalain_world::craft::{item_effect, Action, Custom};
    const EFFECT: u32 = 0x6F00_7001;
    let (mut h, b, me, world) = in_world();
    let mut def = made_recipe(SWORD, &[]);
    for o in def.checks.iter_mut().flat_map(|c| c.outcomes.iter_mut()) {
        o.actions.push(Action::ItemEffect {
            ordinal: 10_001,
            state: EFFECT,
            kind: item_effect::STATIC,
            spellcraft: 2.5,
            by_spinner: None,
        });
    }
    def.custom = Custom::None;
    with_level_tables(&mut h);
    with_own_recipe(&mut h, world, def, 50, &[SWORD, IRON, WOOD]);
    let iron =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, 40).expect("iron");
    let wood =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, WOOD, 40).expect("wood");
    h.run(2);
    execute_at(&mut h, b, SMALL_SWORD_RECIPE, &[(1, iron), (3, wood)], 20);
    h.run(4);
    wait(&mut h, 9.0);
    let item = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == SWORD)
        .map(|i| i.id)
        .expect("made");
    let t = h.server.things.get(item).expect("in the world");
    assert_eq!(t.own.made, None, "no item level, no numbers");
    assert_eq!(t.enchantments, vec![(EFFECT, 2.5)]);
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    h.enter_world(b, "one", me);
    let t = h.server.things.get(item).expect("back in the world");
    assert_eq!(t.own.made, None);
    assert_eq!(t.enchantments, vec![(EFFECT, 2.5)]);
}

fn with_level_tables(h: &mut Harness) {
    use yalain_world::craft::LevelMap;
    let tables = &mut h.server.crafting.spinner_tables;
    tables.insert(
        LEVEL_TABLE,
        LevelMap {
            first: 1,
            values: (1..=150).collect(),
        },
    );
    tables.insert(
        PLAIN_QUALITY,
        LevelMap {
            first: 1,
            values: (1..=150).map(|l| l * 10).collect(),
        },
    );
    tables.insert(
        0x6F00_0EC2,
        LevelMap {
            first: 1,
            values: vec![10; 150],
        },
    );
}

fn props_of(pairs: &[(&str, i64)]) -> yalain_content::Props {
    let mut p = yalain_content::Props::default();
    for (k, v) in pairs {
        p.set(k, yalain_content::Prop::Int(*v));
    }
    p
}

/// Make a template at a spinner: the item made.
fn make_at(
    h: &mut Harness,
    b: usize,
    world: yalain_net::ConnId,
    props: yalain_content::Props,
    product: u32,
    tables: &[u32],
    spinner: u32,
) -> u64 {
    with_level_tables(h);
    h.server
        .catalog
        .insert(yalain_testkit::test_template(product, 0x200, props));
    with_own_recipe(h, world, made_recipe(product, tables), 50, &[IRON, WOOD]);
    let iron =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, 40).expect("iron");
    let wood =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, WOOD, 40).expect("wood");
    h.run(2);
    execute_at(h, b, SMALL_SWORD_RECIPE, &[(1, iron), (3, wood)], spinner);
    h.run(4);
    wait(h, 9.0);
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == product)
        .map(|i| i.id)
        .expect("made")
}

/// A recipe's trade goods take the item level's name and are worth what went into them: 4 a unit of
/// the first material, 2 of the second; they need no level to use.
/// Behaviour: server.craft.trade-goods-worth-their-materials
#[test]
fn trade_goods_made_at_a_level_are_named_for_it_and_worth_their_materials() {
    let (mut h, b, _me, world) = in_world();
    let item = make_at(
        &mut h,
        b,
        world,
        props_of(&[("Item_Value", 150)]),
        0x4700_0FA5,
        &[LEVEL_TABLE],
        28,
    );
    let t = h.server.things.get(item).expect("in the world");
    let m = t.own.made.expect("what its level gave it");
    assert_eq!(m.prefix, yalain_world::made::prefix(28));
    assert_eq!(t.value, 4 * 10 + 2 * 10, "4 a unit and 2 a unit");
    assert_eq!(m.value, t.value);
    assert_eq!(t.own.min_level, None, "no level to use it");
    assert_eq!(m.damage, None);
    assert_eq!(m.armor, None);
}

/// A weapon made at a level takes its name, value and least level from it, and its greatest damage
/// from the outcome's quality: and that is the damage its hits are reckoned from.
/// Behaviour: server.craft.made-weapon-at-its-level
#[test]
fn a_weapon_made_at_a_level_hits_for_that_levels_damage_and_needs_that_level() {
    let (mut h, b, _me, world) = in_world();
    let props = {
        let mut p = props_of(&[("Weapon_Damage", 10), ("Item_Value", 10)]);
        p.set("Weapon_Variance", yalain_content::Prop::Float(0.25));
        p
    };
    let item = make_at(
        &mut h,
        b,
        world,
        props,
        SWORD,
        &[LEVEL_TABLE, PLAIN_QUALITY],
        20,
    );
    let t = h.server.things.get(item).expect("in the world");
    let m = t.own.made.expect("what its level gave it");
    assert_eq!(m.prefix, 0x2AF5_7660, "Sturdy");
    assert_eq!(t.own.min_level, Some(20));
    assert_eq!(m.damage, Some((105, 0.25)));
    assert_eq!(m.slots, 2);
    let (low, high) = yalain_world::made::equipment_value(20);
    assert!((low..=high).contains(&t.value), "worth {}", t.value);
    assert!(m.vigor == Some(4) || m.vigor == Some(5), "{:?}", m.vigor);
    let (least, most) = yalain_world::made::WEAPON_DELAY;
    assert!(m.delay.is_some_and(|d| (least..=most).contains(&d)));
    let d = yalain_world::combat::weapon_damage(&h.server, item).expect("a weapon");
    assert_eq!(d.most, 105, "its hits are reckoned from its own damage");
}

/// An outcome's quality decides a product's damage: a flawed making's reads two levels below the
/// spinner, an expert one's two above.
/// Behaviour: server.craft.outcome-quality-decides
#[test]
fn a_flawed_making_is_two_levels_worse_and_an_expert_one_two_better() {
    use yalain_world::craft::LevelMap;
    const FLAWED: u32 = 0x6F00_0EBE;
    const EXPERT: u32 = 0x6F00_0EB8;
    for (table, values, expected) in [
        (
            FLAWED,
            (1..=150)
                .map(|l: u32| l.saturating_sub(2).max(1) * 10)
                .collect::<Vec<_>>(),
            18,
        ),
        (
            EXPERT,
            (1..=150).map(|l: u32| (l + 2) * 10).collect::<Vec<_>>(),
            22,
        ),
    ] {
        let (mut h, b, _me, world) = in_world();
        h.server
            .crafting
            .spinner_tables
            .insert(table, LevelMap { first: 1, values });
        let props = props_of(&[("Weapon_Damage", 10)]);
        let item = make_at(&mut h, b, world, props, SWORD, &[LEVEL_TABLE, table], 20);
        let m = h
            .server
            .things
            .get(item)
            .and_then(|t| t.own.made)
            .expect("made");
        assert_eq!(
            m.damage.map(|d| d.0),
            Some(yalain_world::made::damage(expected)),
            "quality {expected}"
        );
        assert_eq!(
            m.prefix,
            yalain_world::made::prefix(20),
            "named for the spinner"
        );
    }
}

/// A piece of armor's rating follows the item level by where it is worn: a chest piece's whole, a
/// leg piece's half, a smaller piece's a quarter, a robe's more than a chest piece's.
/// Behaviour: server.craft.made-armor-at-its-level
#[test]
fn armor_made_at_a_level_is_rated_by_where_it_is_worn() {
    use yalain_world::made::{armor_rating, Piece};
    // Each as recorded of 2017's made pieces.
    for (level, rating) in [
        (5, 3),
        (10, 7),
        (12, 9),
        (13, 10),
        (15, 12),
        (20, 19),
        (28, 32),
    ] {
        assert_eq!(
            armor_rating(Piece::Chest, level),
            rating,
            "chest at {level}"
        );
    }
    for (level, rating) in [
        (5, 2),
        (10, 4),
        (12, 5),
        (13, 5),
        (25, 13),
        (28, 16),
        (29, 17),
    ] {
        assert_eq!(armor_rating(Piece::Legs, level), rating, "legs at {level}");
    }
    for (level, rating) in [
        (5, 2),
        (10, 3),
        (12, 3),
        (13, 3),
        (15, 3),
        (18, 4),
        (20, 5),
        (25, 7),
        (28, 8),
        (29, 8),
    ] {
        assert_eq!(
            armor_rating(Piece::Small, level),
            rating,
            "small at {level}"
        );
    }
    assert_eq!(armor_rating(Piece::Robe, 20), 24);
    assert_eq!(armor_rating(Piece::Robe, 17), 19);
    assert_eq!(Piece::of(0x2, 0x8), Piece::Robe);
    assert_eq!(Piece::of(0x2, 0), Piece::Chest);
    assert_eq!(Piece::of(0x8, 0), Piece::Legs);
    assert_eq!(Piece::of(0x1, 0), Piece::Small);

    let (mut h, b, _me, world) = in_world();
    let mut props = props_of(&[("Item_ArmorLevel", 10), ("Item_CombatDelay", 5)]);
    props.set(
        "Item_ValidInventoryLocations",
        yalain_content::Prop::Word(1),
    );
    let item = make_at(
        &mut h,
        b,
        world,
        props,
        HELM,
        &[LEVEL_TABLE, PLAIN_QUALITY],
        12,
    );
    let t = h.server.things.get(item).expect("in the world");
    let m = t.own.made.expect("what its level gave it");
    assert_eq!(m.armor, Some(3));
    assert_eq!(m.slots, 1);
    assert_eq!(t.own.min_level, Some(12));
    let (least, most) = yalain_world::made::ARMOR_DELAY;
    assert!(m.delay.is_some_and(|d| (least..=most).contains(&d)));
}

/// A product whose outcome names no mutation through the item level's table is its template (a tool
/// or a stone a spinner chooses): nothing of its own.
/// Behaviour: server.craft.no-level-mutation-no-level
#[test]
fn a_product_with_no_mutation_by_the_item_level_stays_its_template() {
    let (mut h, b, _me, world) = in_world();
    let item = make_at(
        &mut h,
        b,
        world,
        props_of(&[("Item_Value", 10)]),
        HELM,
        &[PLAIN_QUALITY],
        12,
    );
    let t = h.server.things.get(item).expect("in the world");
    assert_eq!(t.own.made, None);
    assert_eq!(t.value, 0);
    assert_eq!(t.own.min_level, None);
}

/// A made item's own effects change it as it is made: more damage, a smaller span, less vigor, the
/// changed lines marked; they are listed among its enchantments with their amounts.
/// Behaviour: server.craft.made-item-own-effects
#[test]
fn a_made_items_permanent_effects_change_its_numbers_and_show_with_their_amounts() {
    use yalain_world::craft::{item_effect, Action, Custom};
    use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
    use yalain_world::made::stat;
    use yalain_world::profile::{Amount, When};
    const WEIGHTED: u32 = 0x6F00_0F2B;
    const BALANCED: u32 = 0x6F00_0F23;
    const LIGHT: u32 = 0x6F00_0F1B;
    let (mut h, b, _me, world) = in_world();
    with_level_tables(&mut h);
    for (state, stat_, by) in [
        (WEIGHTED, stat::DAMAGE, 1.05),
        (BALANCED, stat::SPAN, 0.8),
        (LIGHT, stat::VIGOR, 0.5),
    ] {
        h.server.effect_table.effects.insert(
            state,
            EffectDef {
                kind: Kind::Numeric {
                    flags: numeric::MULTIPLY,
                    stats: vec![stat_],
                    magnitude: Scale(vec![(0.0, by)]),
                },
                internal_flags: 1,
                name: Some((0xA860_7CEC, 0x2500_179B)),
                ..yalain_testkit::plain_effect(state)
            },
        );
    }
    let mut def = made_recipe(SWORD, &[LEVEL_TABLE, PLAIN_QUALITY]);
    for o in def.checks.iter_mut().flat_map(|c| c.outcomes.iter_mut()) {
        for state in [WEIGHTED, BALANCED, LIGHT] {
            o.actions.push(Action::ItemEffect {
                ordinal: 10_001,
                state,
                kind: item_effect::STATIC,
                spellcraft: 0.0,
                by_spinner: Some(LEVEL_TABLE),
            });
        }
    }
    def.custom = Custom::None;
    let mut props = props_of(&[("Weapon_Damage", 10)]);
    props.set("Weapon_Variance", yalain_content::Prop::Float(0.25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SWORD, 0x200, props));
    with_own_recipe(&mut h, world, def, 50, &[IRON, WOOD]);
    let iron =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, 40).expect("iron");
    let wood =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, WOOD, 40).expect("wood");
    h.run(2);
    execute_at(&mut h, b, SMALL_SWORD_RECIPE, &[(1, iron), (3, wood)], 20);
    h.run(4);
    wait(&mut h, 9.0);
    let item = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == SWORD)
        .map(|i| i.id)
        .expect("made");
    let t = h.server.things.get(item).expect("in the world");
    let m = t.own.made.expect("its own");
    let (most, variance) = m.damage.expect("a weapon");
    assert_eq!(most, 110, "105 and a twentieth more");
    assert!(
        (variance - 0.2).abs() < 1e-5,
        "a fifth less span: {variance}"
    );
    assert!(m.damage_changed && m.vigor_changed);
    assert!(
        m.vigor.is_some_and(|v| v <= 3),
        "half the vigor: {:?}",
        m.vigor
    );
    assert_eq!(
        t.enchantments.iter().map(|e| e.0).collect::<Vec<_>>(),
        vec![WEIGHTED, BALANCED, LIGHT]
    );
    let d = h.server.effect_table.get(LIGHT).expect("light");
    let shown = yalain_world::made::effect_shown(d, When::Permanent, 20.0).expect("shown");
    assert_eq!(shown.amount, Some(Amount::Percent(50.0)));
    let d = h.server.effect_table.get(WEIGHTED).expect("weighted");
    let shown = yalain_world::made::effect_shown(d, When::Permanent, 20.0).expect("shown");
    assert_eq!(shown.amount, Some(Amount::Percent((1.05f32 - 1.0) * 100.0)));
}

/// Dyeing gives a piece of armor the plant's value in three of its appearance modifiers, in the pack
/// and on its wearer at once, its table for lying in a pack or, with none, its first for being worn;
/// the plant is used up, the armor stays.
/// Behaviour: server.craft.dyeing-colors-armor
#[test]
fn dyeing_gives_armor_the_plants_color_in_the_pack_and_on_its_wearer() {
    use yalain_world::craft::{Action, Check, Custom, Ingredient, Outcome, RecipeDef};
    use yalain_world::equip::DYE_KEYS;
    const ARMOR: u32 = 0x4700_0B06;
    const PLANT: u32 = 0x4700_1003;
    const PILE: u32 = 0x2000_036A;
    const WORN_FILE: u32 = 0x2000_036B;
    const RECIPE: u32 = 0x6F00_56D0;
    const LAPYAN: f32 = 0.573;
    for (worn, pile) in [(false, true), (true, true), (false, false)] {
        let (mut h, b, _me, world) = in_world();
        let mut armor = yalain_content::Props::default();
        if pile {
            armor.set("Item_PileAppearance", yalain_content::Prop::Word(PILE));
        }
        let table = if pile { PILE } else { WORN_FILE };
        armor
            .set(
                "Item_ValidInventoryLocations",
                yalain_content::Prop::Word(2),
            )
            .set("Item_WornAppearanceRace1", yalain_content::Prop::Word(0xFF))
            .set(
                "Item_WornAppearanceSex1",
                yalain_content::Prop::Word(0xFFFF),
            )
            .set(
                "Item_WornAppearanceFile1",
                yalain_content::Prop::Word(WORN_FILE),
            );
        h.server
            .catalog
            .insert(yalain_testkit::test_template(ARMOR, 0x200, armor));
        let mut plant = yalain_content::Props::default();
        plant.set("Craft_DyePlant_Mod", yalain_content::Prop::Float(LAPYAN));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(PLANT, 0x200, plant));
        let slot = |ordinal, item| Ingredient {
            ordinal,
            quantity: 1,
            items: vec![item],
            ..Ingredient::default()
        };
        let def = RecipeDef {
            did: RECIPE,
            skill: TOOLS,
            charges: -1,
            animation: 0x4100_014B,
            cycles: 3,
            custom: Custom::Dye,
            ingredients: vec![slot(1, ARMOR), slot(2, PLANT)],
            checks: vec![Check {
                upto: 2.0,
                outcomes: vec![Outcome {
                    upto: 1000.0,
                    xp_mod: 1.0,
                    actions: vec![
                        Action::Custom,
                        Action::Destroy {
                            ordinal: 2,
                            quantity: 1,
                            by_spinner: None,
                            spinners: None,
                        },
                    ],
                }],
            }],
            ..RecipeDef::default()
        };
        with_own_recipe(&mut h, world, def, 2, &[]);
        let item = yalain_world::inventory::give(&mut h.server, &mut h.net, world, ARMOR, 1)
            .expect("the armor");
        let dye = yalain_world::inventory::give(&mut h.server, &mut h.net, world, PLANT, 1)
            .expect("the plant");
        h.run(2);
        if worn {
            let me = h.server.players[&world].character;
            h.bots[b].send(
                true,
                queue::WEENIE,
                &server_call(
                    yalain_world::requests::function::USE,
                    &[yalain_testkit::play::use_of(item, me)],
                ),
            );
            h.run(4);
            wait(&mut h, 2.0);
        }
        execute(&mut h, b, RECIPE, &[(1, item), (2, dye)]);
        h.run(4);
        wait(&mut h, 9.0);
        let t = h.server.things.get(item).expect("the armor stays");
        assert_eq!(t.own.dye, Some((table, LAPYAN)));
        let p = &h.server.players[&world];
        assert!(
            !p.play.items.iter().any(|i| i.template == PLANT),
            "the plant used up"
        );
        let look = yalain_world::inventory::item_create(t);
        let values = look
            .visual
            .global
            .appearance
            .entries
            .iter()
            .find(|(k, _)| *k == table)
            .map(|(_, v)| v.clone())
            .expect("its color in the pack");
        for k in DYE_KEYS {
            assert_eq!(values.get(k), Some(&LAPYAN), "key {k:X}");
        }
        if worn {
            let on_me = p
                .look
                .global
                .appearance
                .entries
                .iter()
                .find(|(k, _)| *k == WORN_FILE)
                .map(|(_, v)| v.clone())
                .expect("worn");
            for k in DYE_KEYS {
                assert_eq!(on_me.get(k), Some(&LAPYAN), "worn key {k:X}");
            }
            assert_eq!(on_me.get(0x10), Some(&1.0), "shown");
        }
    }
}

/// A gathering tool's table gives the craft skill it asks and a yield of five above its level; its
/// durability is 100, an expert making's 200; it takes no name; and it says so.
/// Behaviour: server.craft.tool-at-its-level
#[test]
fn a_tool_made_at_a_level_asks_its_craft_skill_and_yields_five_above_it() {
    use yalain_world::craft::{Check, LevelMap, Outcome};
    use yalain_world::made::GATHERING_TOOL_TABLE;
    const HUNTING_KNIFE: u32 = 0x4700_0CBB;
    for (upto, durability) in [(1.99_f32, 100), (2.0, 200)] {
        let (mut h, b, _me, world) = in_world();
        with_level_tables(&mut h);
        h.server.crafting.spinner_tables.insert(
            GATHERING_TOOL_TABLE,
            LevelMap {
                first: 1,
                values: (1..=150).map(|l| l * 3).collect(),
            },
        );
        let mut def = made_recipe(HUNTING_KNIFE, &[GATHERING_TOOL_TABLE]);
        let outcome = def.checks[0].outcomes[0].clone();
        def.checks = vec![Check {
            upto,
            outcomes: vec![Outcome { ..outcome }],
        }];
        h.server.catalog.insert(yalain_testkit::test_template(
            HUNTING_KNIFE,
            0x200,
            props_of(&[("Item_Value", 1)]),
        ));
        with_own_recipe(&mut h, world, def, 50, &[IRON, WOOD]);
        let iron = yalain_world::inventory::give(&mut h.server, &mut h.net, world, IRON, 40)
            .expect("iron");
        let wood = yalain_world::inventory::give(&mut h.server, &mut h.net, world, WOOD, 40)
            .expect("wood");
        h.run(2);
        execute_at(&mut h, b, SMALL_SWORD_RECIPE, &[(1, iron), (3, wood)], 20);
        h.run(4);
        wait(&mut h, 9.0);
        let item = h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == HUNTING_KNIFE)
            .map(|i| i.id)
            .expect("made");
        let t = h.server.things.get(item).expect("in the world");
        let m = t.own.made.expect("its own");
        assert_eq!(m.tool_yield, Some(25));
        assert_eq!(m.craft_needed, Some(60));
        assert_eq!(m.durability, Some(durability));
        assert_eq!(m.prefix, 0, "no name of its level");
        assert_eq!(t.value, 0, "its template's value");
        let create = yalain_world::inventory::item_create(t);
        let bit = auberean_protocol::types::WeenieDesc::NUMBER_BITS
            .iter()
            .position(|x| *x == 0x1000_0000)
            .expect("the bit");
        assert_eq!(
            create.weenie.numbers[bit],
            i32::try_from(durability).unwrap()
        );
    }
}

/// How many of a template the player holds, its stacks counted whole.
fn units(h: &Harness, world: yalain_net::ConnId, template: u32) -> u32 {
    h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == template)
        .map(|i| i.quantity.max(1))
        .sum()
}

/// An ingredient put down while the recipe is being made: nothing comes of it, and the player is
/// told.
/// Behaviour: server.craft.nothing-made-of-an-ingredient-gone
#[test]
fn an_ingredient_put_down_during_the_making_makes_nothing() {
    use yalain_world::inventory::{move_graph, Move};
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    h.run(2);
    let m = Move {
        move_type: yalain_world::drops::DROP,
        item: iron,
        quantity: 20,
        ..Move::default()
    };
    h.ask(
        b,
        yalain_world::drops::function::MOVE_ITEM,
        &[move_graph(&m)],
        2,
    );
    wait(&mut h, 12.0);
    assert_eq!(units(&h, world, MINES_SWORD), 0, "no sword for no iron");
    let answers: Vec<u32> = calls_of(&h, b, from)
        .iter()
        .filter(|(f, _)| *f == function::EXECUTED)
        .filter_map(|(_, v)| match v.get(1) {
            Some(Value::Int(e)) => Some(*e),
            _ => None,
        })
        .collect();
    assert!(
        answers.last().is_some_and(|e| *e != 0),
        "told it did not go: {answers:?}"
    );
    assert!(h.server.players[&world].making.is_none());
}

/// One stack named for two slots of a recipe: it must hold enough for both.
/// Behaviour: server.craft.nothing-made-of-an-ingredient-gone
#[test]
fn one_stack_in_two_slots_must_hold_enough_for_both() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    let mut recipe = h.server.crafting.recipes[&MINES_RECIPE].clone();
    let mut second = recipe.ingredients[0].clone();
    second.ordinal = 2;
    recipe.ingredients.push(second);
    h.server.crafting.recipes.insert(MINES_RECIPE, recipe);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron), (2, iron)]);
    h.run(4);
    let answered = calls_of(&h, b, from)
        .iter()
        .find(|(f, _)| *f == function::EXECUTED)
        .cloned()
        .expect("answered");
    assert_eq!(answered.1.get(1), Some(&Value::Int(error::TOO_FEW)));
    assert!(h.server.players[&world].making.is_none());
    assert_eq!(units(&h, world, IRON), 20, "nothing used");
}

/// What an effect on the player adds to its craft skill counts in a making's check, up to the
/// skill's cap: a recipe far beyond the player never succeeds until its hammer's effect is on.
/// Behaviour: server.craft.check-counts-what-effects-add
#[test]
fn a_makings_check_counts_what_effects_add_to_the_craft_skill() {
    use yalain_world::craft::{craft_level, Action, Check, Outcome, RecipeDef};
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const BONUS_STAT: u32 = 129 << 16 | 22;
    const HAMMER: u32 = 0x6F00_17EE;
    const HARD: u32 = 0x6F00_7011;
    let (mut h, b, me, world) = in_world();
    for d in &mut h.server.crafting.skills {
        if d.did == TOOLS {
            d.bonus_stat = BONUS_STAT;
            d.cap = 150;
        }
    }
    h.server.effect_table.effects.insert(
        HAMMER,
        EffectDef {
            kind: Kind::Numeric {
                flags: yalain_world::effect_defs::numeric::ADD,
                stats: vec![BONUS_STAT],
                magnitude: Scale(vec![(1.0, 1.0), (500.0, 500.0)]),
            },
            ..yalain_testkit::plain_effect(HAMMER)
        },
    );
    let recipe = RecipeDef {
        did: HARD,
        skill: TOOLS,
        charges: -1,
        animation: 0x4100_014B,
        cycles: 3,
        difficulty: 100,
        checks: vec![
            Check {
                upto: 1.0,
                outcomes: vec![Outcome {
                    upto: 1000.0,
                    xp_mod: 1.0,
                    actions: Vec::new(),
                }],
            },
            Check {
                upto: 2.0,
                outcomes: vec![Outcome {
                    upto: 1000.0,
                    xp_mod: 1.0,
                    actions: vec![Action::Produce {
                        template: STEEL,
                        quantity: 1,
                        ordinal: 10_001,
                    }],
                }],
            },
        ],
        ..RecipeDef::default()
    };
    with_own_recipe(&mut h, world, recipe, 1, &[STEEL]);
    let made = |h: &mut Harness, tries: usize| {
        let before = held(h, world, STEEL);
        for _ in 0..tries {
            execute(h, b, HARD, &[]);
            h.run(4);
            wait(h, 9.0);
        }
        held(h, world, STEEL) - before
    };
    assert_eq!(craft_level(&h.server, world, TOOLS), 1);
    assert_eq!(made(&mut h, 20), 0, "99 under the recipe never succeeds");

    let clock = h.clock();
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        HAMMER,
        effects::Cast {
            caster: me,
            spellcraft: 99.0,
            ..effects::Cast::default()
        },
        clock,
    );
    assert_eq!(craft_level(&h.server, world, TOOLS), 100);
    assert!(made(&mut h, 20) > 0, "level with the recipe, it succeeds");

    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        HAMMER,
        effects::Cast {
            caster: me,
            spellcraft: 400.0,
            ..effects::Cast::default()
        },
        clock,
    );
    assert_eq!(
        craft_level(&h.server, world, TOOLS),
        150,
        "never past the skill's cap"
    );
}

/// A slot that asks a level or an Arcane Lore by the spinner: a level, the same number at each.
fn asking_slot(h: &mut Harness) -> yalain_world::craft::Ingredient {
    use yalain_world::craft::{Ingredient, LevelMap, Limit};
    const LEVELS: u32 = 0x6F00_0FF1;
    const LORES: u32 = 0x6F00_0FF2;
    let table = |values: Vec<u32>| LevelMap { first: 1, values };
    h.server
        .crafting
        .spinner_tables
        .insert(LEVELS, table((1..=60).collect()));
    let mut lore = vec![0; 50];
    lore.extend((1..=10).map(|n| n * 10));
    h.server.crafting.spinner_tables.insert(LORES, table(lore));
    Ingredient {
        ordinal: 1,
        quantity: 1,
        items: vec![yalain_testkit::TEST_ITEM],
        level: Limit::ByTable(LEVELS),
        lore: Limit::ByTable(LORES),
        ..Ingredient::default()
    }
}

/// What a slot asks by the spinner: below the lore table's start an item's least level, at most
/// the spinner; an item that takes Arcane Lore is refused there. Where the lore table starts, an
/// item's lore, at most the table's, and no level at all.
/// Behaviour: server.craft.slots-ask-level-and-lore
#[test]
fn a_slot_refuses_an_item_above_its_level_or_lore_at_the_spinner() {
    use yalain_world::craft::error;
    let mut h = Harness::new();
    let slot = asking_slot(&mut h);
    let c = &h.server.crafting;
    // Level 20: only a level is asked.
    assert_eq!(c.level_or_lore_refused(&slot, 20, 20, 0), 0);
    assert_eq!(c.level_or_lore_refused(&slot, 20, 6, 0), 0);
    assert_eq!(
        c.level_or_lore_refused(&slot, 20, 21, 0),
        error::ITEM_LEVEL_TOO_HIGH
    );
    assert_eq!(
        c.level_or_lore_refused(&slot, 20, 1, 5),
        error::ITEM_LORE_TOO_HIGH,
        "an item taking lore, where no lore is asked"
    );
    // Level 51: lore 10 is asked and level isn't.
    assert_eq!(c.level_or_lore_refused(&slot, 51, 1, 10), 0);
    assert_eq!(
        c.level_or_lore_refused(&slot, 51, 1, 11),
        error::ITEM_LORE_TOO_HIGH
    );
    assert_eq!(
        c.level_or_lore_refused(&slot, 51, 90, 0),
        0,
        "no level then"
    );
}

/// A slot asking nothing passes any item.
/// Behaviour: server.craft.slots-ask-level-and-lore
#[test]
fn a_slot_that_asks_no_level_or_lore_takes_any_item() {
    let h = Harness::new();
    let slot = yalain_world::craft::Ingredient::default();
    let c = &h.server.crafting;
    assert_eq!(c.level_or_lore_refused(&slot, 20, 90, 90), 0);
}

/// Putting an item in a slot and making a recipe both refuse one over the slot's level, its own
/// least level counting ahead of its template's.
/// Behaviour: server.craft.slots-ask-level-and-lore
#[test]
fn an_item_over_a_slots_level_is_refused_when_put_in_and_when_made() {
    use yalain_world::craft::{error, RecipeDef};
    const ASKS: u32 = 0x6F00_7012;
    let (mut h, b, _me, world) = in_world();
    let slot = asking_slot(&mut h);
    let recipe = RecipeDef {
        did: ASKS,
        skill: TOOLS,
        charges: -1,
        animation: 0x4100_014B,
        cycles: 3,
        spinner: Some((1, 60)),
        ingredients: vec![yalain_world::craft::Ingredient {
            spinners: Some((1, 60)),
            ..slot
        }],
        ..RecipeDef::default()
    };
    with_own_recipe(&mut h, world, recipe, 1, &[yalain_testkit::TEST_ITEM]);
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    h.server.things.get_mut(item).expect("made").own.min_level = Some(30);
    h.run(2);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::ADD_INGREDIENT,
            &[
                Arg::Int(20),
                Arg::LongInt(item),
                Arg::Int(1),
                Arg::Int(ASKS),
            ],
        ),
    );
    h.run(4);
    let added: Vec<_> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::INGREDIENT_ADDED)
        .collect();
    assert_eq!(added[0].1[0], Value::Int(error::ITEM_LEVEL_TOO_HIGH));
    let from = execute_at(&mut h, b, ASKS, &[(1, item)], 20);
    h.run(4);
    assert_eq!(craft_answers(&h, b, from), [error::ITEM_LEVEL_TOO_HIGH]);
}

/// A craft flag a slot refuses: a recipe that gives its ingredient that flag can't take it again,
/// and the flag stays across a logout.
/// Behaviour: server.craft.slots-ask-craft-flags
#[test]
fn an_item_a_recipe_flagged_is_refused_by_a_slot_that_bars_the_flag() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    use yalain_world::craft::{error, Action, Check, Ingredient, Outcome, RecipeDef};
    const ONCE: u32 = 0x6F00_7013;
    let (mut h, b, me, world) = in_world();
    let recipe = RecipeDef {
        did: ONCE,
        skill: TOOLS,
        charges: -1,
        animation: 0x4100_014B,
        cycles: 3,
        ingredients: vec![Ingredient {
            ordinal: 2,
            quantity: 1,
            items: vec![yalain_testkit::TEST_ITEM],
            restricted_flags: 8,
            ..Ingredient::default()
        }],
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![Action::Flag {
                    ordinal: 2,
                    flags: 8,
                    spinners: None,
                }],
            }],
        }],
        ..RecipeDef::default()
    };
    with_own_recipe(&mut h, world, recipe, 1, &[yalain_testkit::TEST_ITEM]);
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    h.run(2);
    let from = execute(&mut h, b, ONCE, &[(2, item)]);
    h.run(4);
    wait(&mut h, 9.0);
    assert_eq!(craft_answers(&h, b, from), [0], "the first time");
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    h.enter_world(b, "one", me);
    let from = execute(&mut h, b, ONCE, &[(2, item)]);
    h.run(4);
    assert_eq!(craft_answers(&h, b, from), [error::BARRED_BY_FLAGS]);
}

/// A recipe whose difficulty is further above the craft skill than its cutoff is refused with the
/// answer alone, and what effects add to the skill counts.
/// Behaviour: server.craft.recipe-too-far-above-the-skill-refused
#[test]
fn a_recipe_past_its_cutoff_is_refused_until_effects_lift_the_skill() {
    use yalain_world::craft::{error, Check, Outcome, RecipeDef};
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const BONUS_STAT: u32 = 129 << 16 | 22;
    const HAMMER: u32 = 0x6F00_17EE;
    const FAR: u32 = 0x6F00_7014;
    let (mut h, b, me, world) = in_world();
    for d in &mut h.server.crafting.skills {
        if d.did == TOOLS {
            d.bonus_stat = BONUS_STAT;
            d.cap = 150;
        }
    }
    h.server.effect_table.effects.insert(
        HAMMER,
        EffectDef {
            kind: Kind::Numeric {
                flags: yalain_world::effect_defs::numeric::ADD,
                stats: vec![BONUS_STAT],
                magnitude: Scale(vec![(1.0, 1.0), (50.0, 50.0)]),
            },
            ..yalain_testkit::plain_effect(HAMMER)
        },
    );
    let recipe = RecipeDef {
        did: FAR,
        skill: TOOLS,
        charges: -1,
        animation: 0x4100_014B,
        cycles: 3,
        difficulty: 60,
        cutoff: Some(50),
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: Vec::new(),
            }],
        }],
        ..RecipeDef::default()
    };
    with_own_recipe(&mut h, world, recipe, 1, &[]);
    let from = execute(&mut h, b, FAR, &[]);
    h.run(4);
    assert_eq!(craft_answers(&h, b, from), [error::BAD_RECIPE]);
    let clock = h.clock();
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        HAMMER,
        effects::Cast {
            caster: me,
            spellcraft: 9.0,
            ..effects::Cast::default()
        },
        clock,
    );
    let from = execute(&mut h, b, FAR, &[]);
    h.run(4);
    wait(&mut h, 9.0);
    assert_eq!(craft_answers(&h, b, from), [0], "exactly at the cutoff");
}

/// A request's table of ingredients is read as it is packed: its pairs, and none at all when its
/// count isn't what it holds.
/// Behaviour: server.lying-client.calls-hurt-nobody
#[test]
fn a_crafts_table_whose_count_is_not_what_it_holds_names_nothing() {
    use yalain_world::calls::Node;
    let table = |count: u32, pairs: usize| {
        let mut bytes = count.to_le_bytes().to_vec();
        for k in 0..pairs {
            let k = u32::try_from(k).expect("few");
            bytes.extend_from_slice(&k.to_le_bytes());
            bytes.extend_from_slice(&(k + 10).to_le_bytes());
            bytes.extend_from_slice(&(k + 20).to_le_bytes());
        }
        vec![(1u32, Node::Native { ty: 1, bytes })]
    };
    // The size place above the count.
    assert_eq!(
        yalain_world::craft::slots_in(&table(0x0200_0002, 2)),
        vec![(0, 20u64 << 32 | 10), (1, 21u64 << 32 | 11)]
    );
    assert_eq!(yalain_world::craft::slots_in(&table(0, 0)), vec![]);
    for (count, pairs) in [(0x00FF_FFFF, 0), (3, 2), (1, 2), (0, 1)] {
        assert_eq!(
            yalain_world::craft::slots_in(&table(count, pairs)),
            vec![],
            "{count} {pairs}"
        );
    }
    let mut cut = table(1, 1);
    if let Node::Native { bytes, .. } = &mut cut[0].1 {
        bytes.pop();
    }
    assert_eq!(yalain_world::craft::slots_in(&cut), vec![]);
}

/// A product with no place in the pack is refused at the craft's result: its answer says so, and
/// nothing is made or used up. With a place, the same request makes it.
/// Behaviour: server.craft.a-product-wants-a-place-in-the-pack
#[test]
fn a_product_with_no_place_in_the_pack_is_refused_at_the_result_and_uses_nothing() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    let room = yalain_world::inventory::PACK_ROOM;
    let items = &mut h.server.players.get_mut(&world).expect("there").play.items;
    for n in u32::try_from(items.len()).expect("few")..room {
        items.push(yalain_world::state::Held {
            id: 0x8130_0000_0FFF_0000 + u64::from(n),
            template: IRON + 1,
            slot: n,
            location: 0,
            quantity: 1,
        });
    }
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    wait(&mut h, 8.0);
    let answers: Vec<u32> = craft_answers(&h, b, from);
    assert_eq!(answers, [yalain_world::craft::error::NO_ROOM]);
    let of = |h: &Harness, t: u32| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == t)
            .map(|i| i.quantity)
    };
    assert_eq!(of(&h, MINES_SWORD), None, "nothing made");
    assert_eq!(of(&h, IRON), Some(20), "nothing used up");
    h.server
        .players
        .get_mut(&world)
        .expect("there")
        .play
        .items
        .retain(|i| i.template != IRON + 1);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    wait(&mut h, 8.0);
    assert_eq!(craft_answers(&h, b, from), [0]);
    assert!(of(&h, MINES_SWORD).is_some(), "made");
}

/// A recipe costs money: a maker without it is refused at once with the client's own word for it,
/// and nothing is made, used up or paid.
/// Behaviour: server.craft.a-recipe-wants-its-cost
#[test]
fn a_recipe_whose_cost_its_maker_lacks_is_refused_and_makes_nothing() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    if let Some(r) = h.server.crafting.recipes.get_mut(&MINES_RECIPE) {
        r.cost = 500;
    }
    let money = |h: &Harness| yalain_world::store::money(&h.server.players[&world].qualities);
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, 499);
    }
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    h.run(4);
    assert_eq!(answer(&h, b, from), Some(error::NO_MONEY), "at once");
    wait(&mut h, 12.0);
    assert_eq!(held(&h, world, MINES_SWORD), 0, "nothing made");
    assert_eq!(held(&h, world, IRON), 20, "nothing used up");
    assert_eq!(money(&h), 499);
    // With it, the recipe is made and paid for.
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, 501);
    }
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    wait(&mut h, 12.0);
    assert_eq!(answer(&h, b, from), Some(0));
    assert_eq!(held(&h, world, MINES_SWORD), 1);
    assert_eq!(money(&h), 1);
}

/// Money spent while a recipe is being made: its cost is asked for again as it comes due, so what
/// was spent can't pay for it too.
/// Behaviour: server.craft.a-recipe-wants-its-cost-at-its-end
#[test]
fn money_spent_during_a_making_leaves_it_refused_at_its_end() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    if let Some(r) = h.server.crafting.recipes.get_mut(&MINES_RECIPE) {
        r.cost = 500;
    }
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, 500);
    }
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    wait(&mut h, 1.0);
    assert_eq!(answer(&h, b, from), None, "under way");
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, 10);
    }
    wait(&mut h, 12.0);
    assert_eq!(answer(&h, b, from), Some(error::NO_MONEY));
    assert_eq!(held(&h, world, MINES_SWORD), 0);
    assert_eq!(held(&h, world, IRON), 20);
    assert_eq!(
        yalain_world::store::money(&h.server.players[&world].qualities),
        10
    );
}

/// A dead player, or one in portal space, is too busy to craft.
/// Behaviour: server.craft.none-from-death-or-portal-space
#[test]
fn a_recipe_is_refused_to_the_dead_and_in_portal_space() {
    let (mut h, b, me, world) = in_world();
    let iron = with_recipe_and_iron(&mut h, world, me, 20);
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, 258, 0);
    }
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    wait(&mut h, 12.0);
    assert_eq!(answer(&h, b, from), Some(error::TOO_BUSY));
    assert_eq!(held(&h, world, MINES_SWORD), 0);
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, 258, 100);
    }
    let clock = h.clock();
    let to = h.server.players[&world].position;
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, world, to, clock);
    let from = execute(&mut h, b, MINES_RECIPE, &[(1, iron)]);
    h.run(4);
    assert_eq!(answer(&h, b, from), Some(error::TOO_BUSY));
    assert_eq!(held(&h, world, IRON), 20);
}
