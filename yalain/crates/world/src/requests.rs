//! The calls a client makes on the server in the world, each answered as the 2017 servers
//! answered it. A call the server does not answer yet is counted ([`crate::Unhandled`]).

use auberean_protocol::queue;
use auberean_protocol::script::ServerCall;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{self, Value};
use crate::examine::{self, Vitals};
use crate::{chat, profile, Clock, Server};

/// The client's calls the server answers, by their function ids.
pub mod function {
    /// Select an object (its id; 0 for none).
    pub const SET_SELECTION: u32 = 0x8212_0016;
    /// Say something aloud.
    pub const SAY: u32 = 0x8205_0006;
    /// Tell someone something.
    pub const TELL: u32 = 0x8205_0002;
    /// Show an emote; type one.
    pub const EMOTE: u32 = 0x8205_000B;
    pub const TYPED_EMOTE: u32 = 0x8205_0004;
    /// Examine an object.
    pub const EXAMINE: u32 = 0x820A_0007;
    /// Ask for an allegiance's profile.
    pub const QUERY_ALLEGIANCE: u32 = 0x8203_0005;
    /// Use an object.
    pub const USE: u32 = 0x8216_0016;
    /// Use a skill on a target (an attack; the lifestone recall among them).
    pub const ATTACK: u32 = 0x8204_000B;
}

/// A call from a player's client on its world connection.
pub fn call<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    call: &ServerCall,
    clock: Clock,
) {
    let f = call.function.0;
    tracing::debug!(?world, "call {f:#010x}");
    // Before anything is read of it.
    if crate::admin_calls::is_admin_call(f) {
        return crate::admin_calls::call(s, net, world, f, &call.args.0, clock);
    }
    if chat::carries_words(f) && call.args.0.len() > chat::MOST_TALK_BYTES {
        return chat::too_long(s, net, world);
    }
    let Ok(values) = calls::values(&call.args.0) else {
        s.unhandled
            .note(format!("script call {f:#010x} whose values do not read"));
        return;
    };
    match f {
        function::SET_SELECTION => select(s, net, world, &values),
        function::EXAMINE => examine_object(s, net, world, &values, clock),
        function::SAY => chat::say(s, net, world, call, clock),
        function::TELL => chat::tell(s, net, world, &values),
        function::EMOTE => chat::emote(s, net, world, &values),
        function::TYPED_EMOTE => chat::typed_emote(s, net, world, &values),
        // A member's profile is answered; the client's asking for nothing (id 0) is not.
        function::QUERY_ALLEGIANCE => crate::allegiance::query(s, net, world, &values),
        crate::allegiance::function::SWEAR => {
            crate::allegiance::swear(s, net, world, &values, clock);
        }
        crate::allegiance::function::BREAK => crate::allegiance::break_with(s, net, world, &values),
        crate::allegiance::function::ACCEPT_VASSAL => {
            crate::allegiance::accept(s, net, world, &values, clock);
        }
        crate::allegiance::function::DECLINE_VASSAL => {
            crate::allegiance::decline(s, net, world, &values);
        }
        crate::allegiance::function::TREE_EXPORT => {
            crate::allegiance::export(s, net, world, &values, clock.local);
        }
        crate::fellowship::function::CREATE => {
            crate::fellowship::create(s, net, world, &values, clock);
        }
        crate::fellowship::function::RECRUIT => {
            crate::fellowship::recruit(s, net, world, &values, clock);
        }
        crate::fellowship::function::ACCEPT => {
            crate::fellowship::accept(s, net, world, &values, clock);
        }
        crate::fellowship::function::DECLINE => crate::fellowship::decline(s, net, world, &values),
        crate::fellowship::function::LEAVE => crate::fellowship::leave_request(s, net, world),
        crate::fellowship::function::DISMISS => crate::fellowship::dismiss(s, net, world, &values),
        crate::fellowship::function::DISBAND => crate::fellowship::disband(s, net, world),
        crate::trade::function::OPEN => crate::trade::open(s, net, world, &values, clock),
        crate::trade::function::OFFER => crate::trade::offer(s, net, world, &values),
        crate::trade::function::REVOKE => crate::trade::revoke(s, net, world, &values),
        crate::trade::function::ACCEPT => crate::trade::accept(s, net, world, &values, clock),
        crate::trade::function::DECLINE => crate::trade::decline(s, net, world),
        crate::trade::function::CLOSE => crate::trade::close(s, net, world),
        crate::scenes::function::DONE => crate::scenes::done(s, world, &values, clock),
        crate::market::function::ENTER_STORE => {
            crate::market::enter_store(s, net, world, &values, clock.local);
        }
        crate::market::function::ENTER_CONSIGNMENT => {
            crate::market::enter_consignment(s, net, world, &values, clock.local);
        }
        crate::market::function::SELL => crate::market::sell(s, net, world, &values, clock),
        crate::market::function::COLLECT => crate::market::collect(s, net, world, &values),
        // Leaving a catalog or the consignments is not answered.
        crate::market::function::LEAVE_CATALOG | crate::market::function::LEAVE_CONSIGNMENT => {}
        crate::follow::FOLLOW => crate::follow::start(s, net, world, &values),
        crate::mounts::DISMOUNT => crate::mounts::dismount(s, net, world),
        crate::personal::function::SET_ALIAS => crate::personal::set_alias(s, world, &values),
        crate::personal::function::INSCRIBE => crate::personal::inscribe(s, net, world, &values),
        crate::personal::function::LIST_SKILLS => crate::personal::list_skills(s, net, world),
        crate::quests::CANCEL_QUEST => crate::quests::cancel(s, net, world, &values, clock),
        function::USE => {
            crate::follow::stop(s, net, world);
            crate::places::use_object(s, net, world, &values, clock);
        }
        function::ATTACK => {
            crate::follow::stop(s, net, world);
            crate::skill_use::request(s, net, world, &values, clock);
            fight_ends_trade(s, net, world);
        }
        crate::combat::function::START_ATTACK => {
            crate::follow::stop(s, net, world);
            crate::combat::start(s, net, world, &values, clock);
            fight_ends_trade(s, net, world);
        }
        crate::combat::function::STOP_ATTACK => {
            crate::skill_use::stopped(s, net, world, None, clock);
            crate::combat::end(s, net, world);
        }
        crate::combat::function::STOP_ATTACKING => {
            let target = values.iter().find_map(|x| match x {
                crate::calls::Value::Long(l) => Some(*l),
                _ => None,
            });
            if let Some(t) = target {
                crate::skill_use::stopped(s, net, world, Some(t), clock);
            }
            crate::combat::stop(s, net, world, &values);
        }
        crate::craft::function::ADD_INGREDIENT => {
            crate::craft::add_ingredient(s, net, world, &values);
        }
        crate::craft::function::EXECUTE => crate::craft::execute(s, net, world, &values, clock),
        crate::craft::function::RAISE => crate::craft::raise(s, net, world, &values),
        // The 2017 servers did not answer it (nor carry a run on to the next target).
        crate::combat::function::SET_NEXT_TARGET => {}
        crate::inventory::function::CLOSE => {
            // A chest closes as it plays its closing; a corpse at once.
            match values.iter().find_map(|v| match v {
                Value::Long(l) => Some(*l),
                _ => None,
            }) {
                Some(id) if crate::usable::chests::chest(s, id).is_some() => {
                    crate::usable::chests::close(s, net, world, id, clock);
                }
                _ => crate::inventory::close_request(s, net, world, &values),
            }
        }
        crate::drops::function::MOVE_ITEM => {
            crate::drops::move_item(s, net, world, &values, clock);
        }
        crate::equip::function::DIRECTIVE_EQUIP => {
            crate::equip::directive_equip(s, net, world, &values, clock);
        }
        crate::equip::function::DIRECTIVE_UNEQUIP => {
            crate::equip::directive_unequip(s, net, world, &values);
        }
        crate::pack::function::REORGANIZE => crate::pack::reorganize(s, net, world, &values),
        crate::pack::function::TO_MONEY_BAG => crate::pack::to_money_bag(s, net, world, &values),
        crate::pack::function::FROM_MONEY_BAG => {
            crate::pack::from_money_bag(s, net, world, &values);
        }
        crate::pack::function::TAKE_ALL => crate::pack::take_all(s, net, world, &values, clock),
        crate::inventory::function::TRANSMUTE_ALL => {
            crate::skill_use::cut_by_transmute(s, net, world, clock);
            crate::inventory::transmute_all(s, net, world, &values);
        }
        crate::store::function::NEXT_SALES => {
            crate::store::turn_page(s, net, world, true, clock.local);
        }
        crate::store::function::PREV_SALES => {
            crate::store::turn_page(s, net, world, false, clock.local);
        }
        crate::store::function::LEAVE_STORE => crate::store::leave(s, world),
        crate::store::function::PURCHASE => crate::store::purchase(s, net, world, &values, clock),
        crate::skills::function::TRAIN => crate::skills::train(s, net, world, &values),
        crate::skills::function::UNTRAIN => crate::skills::untrain(s, net, world, &values, clock),
        crate::skills::function::UNTRAIN_CANCEL => crate::skills::cancel_untraining(s, net, world),
        crate::death::player_fn::RESURRECT => crate::death::resurrect(s, net, world, clock),
        crate::skills::function::RAISE => crate::skills::raise(s, net, world, &values, clock),
        crate::pets::function::ATTACK
        | crate::pets::function::RETURN
        | crate::pets::function::MODE
        | crate::pets::function::DIE
        | crate::pets::function::NAME => crate::pets::request(s, net, world, f, &values, clock),
        _ => s.unhandled.note(format!("script call {f:#010x}")),
    }
}

/// A player who has begun a fight (an attack or a skill's use under way) ends its trade.
fn fight_ends_trade<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let fighting = s
        .players
        .get(&world)
        .is_some_and(|p| p.attacking.is_some() || p.using.is_some());
    if fighting {
        crate::trade::fighting(s, net, world);
    }
}

/// The vitals of an object the world knows, by id: a player's from its qualities, a creature a
/// camp keeps alive from its own, a placed creature's or a person's from its template. As a client
/// may be told of it.
#[must_use]
pub fn vitals_of<S: Store>(s: &Server<S>, viewer: ConnId, id: u64) -> Option<Vitals> {
    if let Some(p) = crate::presence::player_for(s, viewer, id).and_then(|w| s.players.get(&w)) {
        return Vitals::of_qualities(&p.qualities);
    }
    if let Some(t) = s.things.get(id) {
        // A creature, or a dye plant (2017: its health and vigor, 102 of 102 selections).
        return matches!(
            t.kind,
            crate::things::Kind::Creature | crate::things::Kind::Plant
        )
        .then(|| thing_vitals(t));
    }
    crate::presence::content_by_id(&s.content.objects, id)
        .and_then(|i| examine::content_vitals(&s.content.objects[i]))
}

/// A running creature's vitals, as they stand.
fn thing_vitals(t: &crate::things::Thing) -> Vitals {
    Vitals {
        health: t.health.max(0).cast_unsigned(),
        max_health: t.max_health.max(0).cast_unsigned(),
        vigor: t.vigor.max(0).cast_unsigned(),
        max_vigor: t.max_vigor.max(0).cast_unsigned(),
        pk_damage: 0,
        pk_vigor_loss: 0,
    }
}

/// What is held on a thing, what skills put on it and what auras hold on it in its holder's hands: each listed with its spellcraft, its vigor cost
/// lowered and a weapon's combat delay with it, rounded down, a weapon's damage raised (2017: 67
/// profiles of weapons and shields under Balance and Featherweight, 18 to 13, 18 to 11, 7 to 5; a
/// weapon's delay under Swift Killer 18 to 14, 12 to 9, 10 to 8, a shield's 8 stays 8; its damage
/// under Infected Caress 180-240 to 231-308, under Wound Twister 43-54 to 49-62).
fn enchanted_for_now<S: Store>(s: &Server<S>, t: &crate::things::Thing, v: &mut profile::ItemView) {
    let shield = crate::skill_use::implement_of(&s.catalog, t.template)
        == Some(crate::skill_use::implement::SHIELD);
    let timed = crate::effects::running_on_item(s, t);
    let held = crate::effects::on_thing(s, t);
    if held.is_empty() {
        return;
    }
    let by = |stat: u32, n: i32| {
        #[allow(clippy::cast_precision_loss)]
        let lowered = n as f32 * crate::effects::held_multiplier(s, &held, stat);
        dereth_primitives::num::to_i32(lowered.floor())
    };
    // What skills put on it says so: its damage and shield rating drawn as enchanted, with what
    // it gives a weapon of a rating as a line of its own (2017: a weapon's 67 at spellcraft 48,
    // 70 at 51; a shield's 36 to 76 at 30).
    let by_skills: Vec<(u32, f32)> = timed.iter().map(|e| (e.state, e.spellcraft)).collect();
    if let Some(w) = &mut v.weapon {
        v.damage_changed |= crate::effects::held_damage(s, &by_skills, w.0) != w.0;
        w.0 = crate::effects::held_damage(s, &held, w.0);
        w.3 = by(crate::made::stat::VIGOR, w.3);
        if !shield {
            w.2 = by(crate::made::stat::COMBAT_DELAY, w.2);
        }
        let rating = crate::effects::held_rating(s, &by_skills);
        if rating != 0 {
            match (&mut v.shield, &mut v.armor) {
                (Some(r), _) | (None, Some((r, _, _))) => {
                    *r += rating;
                    v.rating_changed = true;
                }
                (None, None) => v.rating_given = Some(rating),
            }
        }
    }
    let ends = |state: u32| timed.iter().find(|e| e.state == state).map(|e| e.until);
    for (state, spellcraft) in held.iter().copied() {
        let Some(shown) = s
            .effect_table
            .get(state)
            .and_then(profile::EffectShown::of)
            .filter(|x| x.visible)
        else {
            continue;
        };
        v.effects.push(profile::ItemEffect {
            when: profile::When::Enchantment,
            table: shown.table,
            spellcraft: shown
                .spellcraft_shown
                .then_some(dereth_primitives::num::to_i32(spellcraft)),
            amount: None,
            ends: ends(state),
        });
    }
}

/// What an item's profile shows: its class's facts for a starting weapon, else its template's;
/// its value, each piece's own when it has one (treasure) else its template's, times its count; its icon; the
/// effects its template lists; who made it and what is inscribed on it; whether its kind is
/// destroyed on use; the skills it takes.
pub fn item_view<S: Store>(s: &Server<S>, t: &crate::things::Thing) -> profile::ItemView {
    let own_value = (t.value > 0).then(|| t.value.saturating_mul(t.quantity.max(1)));
    let mut v = profile::starting_weapon(t.template).unwrap_or_else(|| {
        s.catalog
            .get(t.template)
            .map_or_else(profile::ItemView::default, |tp| {
                profile::ItemView::of(&tp.props, t.quantity, own_value)
            })
    });
    if s.catalog.icons.contains_key(&t.template) {
        v.icon = crate::inventory::icon_of(s, t);
    }
    if let Some(e) = s.item_effects.get(&t.template) {
        v.effects.clone_from(e);
    }
    v.crafter.clone_from(&t.crafter);
    v.inscription.clone_from(&t.inscription);
    // A worn item shows its durability now.
    if let (Some(now), Some((_, most))) = (t.durability, v.durability) {
        v.durability = Some((i32::try_from(now).unwrap_or(i32::MAX), most));
    }
    v.destroyed_on_use |= s.destroyed_on_use.contains(&t.description.package);
    v.jewelry = s.treasure.jewelry.contains(&t.description.package);
    // A treasure item's own least level and Arcane Lore, and a treasure potion's own effect and
    // name.
    if let Some(kind) = crate::craft::totem::noun(t.template) {
        // A totem's own least level and Arcane Lore are its target's.
        v.totem = Some(profile::TotemView {
            kind,
            least_level: t.own.min_level,
            least_lore: t.own.arcane_lore,
        });
        v.destroyed_on_use = true;
    } else {
        if let Some(l) = t.own.min_level {
            v.min_level = l;
        }
        if let Some(l) = t.own.arcane_lore {
            v.arcane_lore = l;
        }
    }
    if let Some(e) = t.own.effect {
        let shown = s
            .effect_table
            .get(e.state)
            .and_then(profile::EffectShown::of);
        v.effects.push(profile::ItemEffect {
            when: profile::When::Used,
            table: e.table,
            spellcraft: shown
                .is_none_or(|x| x.spellcraft_shown)
                .then_some(dereth_primitives::num::to_i32(e.spellcraft)),
            amount: None,
            ends: None,
        });
        if t.quantity <= 1 {
            v.name = Some(profile::potion_name(&t.facts, e.table));
        }
    }
    crate::made::show(s, t, &mut v);
    enchanted_for_now(s, t, &mut v);
    if let Some(table) = t.own.suffix {
        let word = t.own.made.map_or(0, |m| m.prefix);
        v.name = Some(if v.jewelry {
            profile::potion_name(&t.facts, table)
        } else if word != 0 {
            profile::treasure_word_name(&t.facts, table, word)
        } else {
            profile::treasure_name(&t.facts, table)
        });
    }
    // The skills it takes, by their names.
    if let Some(tp) = s.catalog.get(t.template) {
        v.skills = ["Usage_RequiredSkill1", "Usage_RequiredSkill2"]
            .iter()
            .filter_map(|n| tp.props.word(n).filter(|k| *k != 0))
            .filter_map(|k| s.skill_table.skills.get(&k))
            .map(|d| crate::progress::text(d.name))
            .collect();
    }
    v
}

/// A player examines an object, or stops examining.
fn examine_object<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(bytes) = v.iter().find_map(|x| match x.root() {
        Some(calls::Node::Native { ty, bytes }) if *ty == profile::REQUEST_TYPE => {
            Some(bytes.clone())
        }
        _ => None,
    }) else {
        s.unhandled.note("an examination that does not read");
        return;
    };
    let req = profile::read_request(&bytes);
    match req.kind {
        profile::request::STOP => {
            // A stop sent with the request it follows stops the examination before it; one sent
            // later stops this one.
            if s.examining
                .get(&world)
                .is_some_and(|e| e.asked_at < clock.local.0)
            {
                s.examining.remove(&world);
            }
            return;
        }
        profile::request::OBJECT | profile::request::TARGET => {}
        k => {
            s.unhandled.note(format!("an examination of kind {k:#x}"));
            return;
        }
    }
    let Some(sent) = profile_bytes(s, world, &req, clock.local.0) else {
        s.examining.remove(&world);
        return;
    };
    net.send(
        world,
        queue::EVENT,
        BlobKind::PRIVATE,
        profile::message_of(&sent).encode(),
    );
    let number = s.examining.get(&world).map_or(1, |e| e.number + 1);
    s.examining.insert(
        world,
        examine::Examining {
            request: bytes,
            number,
            asked_at: clock.local.0,
            sent,
        },
    );
    s.after(
        clock,
        examine::AGAIN_SECONDS,
        crate::Timer::Examine { world, number },
    );
}

/// Look again at what a player examines: its profile sent again if it changed, and another look
/// later; nothing more once the examination is over or replaced, or the object is gone.
pub fn examine_again<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    number: u32,
    clock: Clock,
) {
    let Some(e) = s.examining.get(&world).filter(|e| e.number == number) else {
        return;
    };
    let req = profile::read_request(&e.request);
    let Some(now) = profile_bytes(s, world, &req, clock.local.0) else {
        s.examining.remove(&world);
        return;
    };
    if let Some(e) = s.examining.get_mut(&world) {
        if e.sent != now {
            net.send(
                world,
                queue::EVENT,
                BlobKind::PRIVATE,
                profile::message_of(&now).encode(),
            );
            e.sent = now;
        }
    }
    s.after(
        clock,
        examine::AGAIN_SECONDS,
        crate::Timer::Examine { world, number },
    );
}

/// The packed profile answering an examination request (the request first), as the object is
/// now; none when the world has no such object.
fn profile_bytes<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    req: &profile::Request,
    now: f64,
) -> Option<Vec<u8>> {
    let me = s.players.get(&world)?;
    let my_level = examine::level_of(&me.qualities);
    let seen = crate::presence::player_for(s, world, req.object).and_then(|w| s.players.get(&w));
    let profile = if let Some(q) = seen {
        let v = Vitals::of_qualities(&q.qualities).unwrap_or_default();
        profile::player(
            req.asked,
            &crate::player::world_name(&q.name, q.sex),
            q.race,
            q.sex,
            examine::level_of(&q.qualities),
            crate::kingdoms::of(&q.qualities),
            v,
            q.play.quest_rating,
        )
    } else if let Some(t) = s.things.get(req.object) {
        // An item in another player's pack is seen by no one until it is offered.
        let another_holds = t.kind == crate::things::Kind::Item
            && t.container
                .is_some_and(|(c, _)| c != me.character && crate::social::world_of(s, c).is_some());
        if another_holds && !me.known.contains_key(&req.object) {
            s.unhandled.note("examining an item another player holds");
            return None;
        }
        // What the world runs: a creature a camp keeps alive, a corpse, an item, a resource node,
        // a dye plant.
        match t.kind {
            crate::things::Kind::Creature => profile::monster(
                req.asked,
                &t.facts,
                thing_vitals(t),
                profile::chance_to_hit(my_level, t.level()),
            ),
            crate::things::Kind::Item => {
                let mut v = item_view(s, t);
                v.bound = match t.own.bound_to {
                    0 => None,
                    who if who == me.character => Some(profile::BoundTo::You),
                    _ => Some(profile::BoundTo::Another),
                };
                profile::item(req.asked, &v)
            }
            crate::things::Kind::Plant => crate::plants::profile(s, req.asked, t, now),
            crate::things::Kind::Corpse
            | crate::things::Kind::Node
            | crate::things::Kind::Portal => {
                profile::npc(req.asked, profile::item_name(&t.facts), &t.facts)
            }
        }
    } else if let Some(i) = crate::presence::content_by_id(&s.content.objects, req.object) {
        let o = &s.content.objects[i];
        let f = &o.facts;
        let named = o
            .description
            .name_text
            .as_deref()
            .map(profile::literal)
            .or_else(|| o.description.name.map(|(sid, t)| profile::string(t, sid)));
        match o.kind {
            yalain_content::Kind::Creature => profile::monster(
                req.asked,
                f,
                examine::content_vitals(o).unwrap_or_default(),
                profile::chance_to_hit(my_level, f.level.unwrap_or(1)),
            ),
            yalain_content::Kind::Npc => profile::npc(
                req.asked,
                named.unwrap_or_else(|| {
                    f.name
                        .map_or_else(|| profile::literal(""), |(sid, t)| profile::string(t, sid))
                }),
                f,
            ),
            yalain_content::Kind::Static => {
                let lifestone = crate::places::LIFESTONES.contains(&o.template);
                let recall = if lifestone {
                    profile::line::LIFESTONE_RECALL
                } else {
                    profile::portal_line(f.portal)
                };
                if lifestone || o.destination.is_some() {
                    profile::place(
                        req.asked,
                        named.unwrap_or_else(|| profile::item_name(f)),
                        recall,
                        f,
                    )
                } else {
                    // An object for monarchs alone goes by its plain name (2017: 14 of 14).
                    let monarchs = s
                        .catalog
                        .get(o.template)
                        .is_some_and(|t| t.props.flag("Usage_MonarchOnly"));
                    let plain = || {
                        f.name.map_or_else(
                            || profile::literal(""),
                            |(sid, t)| profile::string(t, sid),
                        )
                    };
                    let name = named.unwrap_or_else(|| {
                        if monarchs {
                            plain()
                        } else {
                            profile::item_name(f)
                        }
                    });
                    let mut p = profile::npc(req.asked, name, f);
                    if monarchs {
                        profile::monarch_lines(&mut p);
                    }
                    p
                }
            }
        }
    } else {
        s.unhandled
            .note("examining an object the world does not have");
        return None;
    };
    let answered = req.answered(examine::PEACE_MODE, examine::NO_ACCURACY);
    Some(profile.packed(&answered))
}

fn select<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let id = v
        .iter()
        .find_map(|x| match x {
            Value::Long(l) => Some(*l),
            _ => None,
        })
        .unwrap_or(0);
    let vitals = vitals_of(s, world, id);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.selection = id;
    if id == 0 {
        return;
    }
    // Only an object with vitals (a player, a creature, a person) is answered.
    if let Some(v) = vitals {
        net.send(
            world,
            queue::EVENT,
            BlobKind::PRIVATE,
            examine::selection_info(id, Some(v)).encode(),
        );
    }
}
