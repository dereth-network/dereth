//! The game's state as the interface reads it, taken from the runtime once a frame.

use dereth_client_contract::pregame::GamePhase;
use dereth_client_contract::view::{GameView, Vital as ViewVital};
use dereth_client_runtime::shell::Shell;

use crate::ui::game::{
    Blip, BlipKind, CharacterEntry, ChatLine, Effect, GameState, Item, Relation, Shortcut, Skill,
    Spell, StatRow, Target, Vital,
};

/// The integer qualities read here.
mod int {
    pub const LEVEL: u32 = 25;
    pub const COIN_VALUE: u32 = 20;
}

fn vital(v: &impl GameView, id: dereth_primitives::ObjectId, which: ViewVital) -> Vital {
    v.vital(id, which)
        .map(|(current, max)| Vital { current, max })
        .unwrap_or_default()
}

/// The description bits of a radar object that decide how it is drawn.
mod bits {
    pub const ATTACKABLE: u32 = 0x10;
    /// Fixed where it stands: not to be picked up.
    pub const STUCK: u32 = 0x4;
    pub const LIFESTONE: u32 = 0x4000;
    pub const PORTAL: u32 = 0x0004_0000;
    /// Hidden from the interface: not something to select.
    pub const UI_HIDDEN: u32 = 0x80;
}

/// What the pad can select in the world: every object about the player that is in the world and
/// not hidden from the interface (doors, corpses, chests, portals and things on the ground among
/// them, whether or not the radar shows them), but the player.
pub fn targets_of(
    objects: &[dereth_client_contract::view::RadarEntry],
    kind: impl Fn(&dereth_client_contract::view::RadarEntry) -> BlipKind,
    name: impl Fn(dereth_primitives::ObjectId) -> String,
) -> Vec<Blip> {
    objects
        .iter()
        .filter(|r| !r.is_self && r.in_world && r.bitfield & bits::UI_HIDDEN == 0)
        .map(|r| Blip {
            id: r.id,
            dx: r.player_space.0,
            dy: r.player_space.1,
            kind: kind(r),
            colour: 0,
            shape: 0,
            name: name(r.id),
        })
        .filter(|b| !b.name.is_empty())
        .collect()
}

/// The creature bit of an object's item type, from its public description.
const ITEM_TYPE_CREATURE: u32 = 0x10;

/// What a radar object is, for its blip and its nameplate: a player, a creature that can be
/// fought, any other creature (the people of the world), a portal or lifestone, or a thing.
fn blip_kind(v: &impl GameView, r: &dereth_client_contract::view::RadarEntry) -> BlipKind {
    let creature = v
        .slot_decoration(r.id)
        .is_some_and(|d| d.obj_type & ITEM_TYPE_CREATURE != 0);
    if r.is_player {
        BlipKind::Player
    } else if creature && r.bitfield & bits::ATTACKABLE != 0 {
        BlipKind::Creature
    } else if creature {
        BlipKind::Npc
    } else if r.bitfield & (bits::PORTAL | bits::LIFESTONE) != 0 {
        BlipKind::Portal
    } else {
        BlipKind::Item
    }
}

/// The chat entry's state: the talk focus, the windows' filters, and the draft.
fn chat_entry_state<S: Shell>(cx: &crate::runtime::Cx<'_, S>, g: &mut GameState) {
    use dereth_client_contract::chat::interface::{default_filter, window};
    g.chat_focus = cx.hud().chat_focus_view(cx.model());
    let view = cx.hud().view(cx.objects());
    g.chat_windows = [
        window::MAIN,
        window::FLOATY_1,
        window::FLOATY_2,
        window::FLOATY_3,
        window::FLOATY_4,
    ]
    .into_iter()
    .map(|id| crate::ui::game::ChatWindow {
        id,
        filter: view
            .chat_window_filter(id)
            .unwrap_or_else(|| default_filter(id)),
        title: None,
    })
    .collect();
    g.chat_draft = cx
        .model()
        .chat
        .entries
        .get(&window::MAIN)
        .map(|e| e.text.clone())
        .unwrap_or_default();
}

/// A game phase in the words the interface shows.
/// Where getting into the world stands, from the game's phase: connecting, updating the data
/// files, or ready once character select is up (and from then on).
#[must_use]
pub fn connect_phase(phase: &GamePhase) -> crate::ui::game::ConnectPhase {
    use crate::ui::game::ConnectPhase as P;
    match phase {
        GamePhase::Connecting | GamePhase::Disconnected(_) => P::Connecting,
        GamePhase::Patching => P::Updating,
        _ => P::Ready,
    }
}

/// The data update's progress, followed through the game's update notices: `(received,
/// expected)`, begun by the server saying how much is coming and over when it says it is done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PatchProgress {
    received: u64,
    expected: u64,
}

impl PatchProgress {
    /// Follow this frame's notices.
    pub fn follow(&mut self, events: &[dereth_client_contract::pregame::DddEvent]) {
        use dereth_client_contract::pregame::DddEvent as E;
        for e in events {
            match *e {
                E::PatchtimeBegin { expected } => {
                    *self = Self {
                        received: 0,
                        expected,
                    };
                }
                E::DataDownloaded { bytes } => {
                    self.received = self.received.saturating_add(bytes).min(self.expected);
                }
                E::PatchtimeEnd => *self = Self::default(),
                E::PatchtimeInterrogation | E::PatchtimePending { .. } => {}
            }
        }
    }

    /// `(received, expected)` while an update with a known size is under way.
    #[must_use]
    pub fn shown(&self) -> Option<(u64, u64)> {
        (self.expected > 0).then_some((self.received, self.expected))
    }
}

fn phase_words(phase: &GamePhase) -> String {
    match phase {
        GamePhase::Connecting => "Connecting".into(),
        GamePhase::Patching => "Updating data".into(),
        GamePhase::CharacterSelect => "Character select".into(),
        GamePhase::CharacterCreation => "Creating a character".into(),
        GamePhase::EnteringWorld => "Entering the world".into(),
        GamePhase::InWorld => "In the world".into(),
        GamePhase::LoggingOff => "Logging off".into(),
        GamePhase::Disconnected(_) => "Disconnected".into(),
    }
}

/// Take this frame's snapshot, with the chat lines delivered to the interface this frame.
pub fn snapshot<S: Shell>(
    cx: &crate::runtime::Cx<'_, S>,
    chat: Vec<dereth_client_contract::chat::interface::ChatMessage>,
) -> GameState {
    let pregame = cx.pregame();
    let mut g = GameState {
        client_version: cx.client_version().to_owned(),
        host: cx.config().host.clone(),
        account: cx.config().account.clone(),
        world: pregame.world_name.clone(),
        sigil_slots: cx.hud().view(cx.objects()).aetheria_slots(),
        local_time: local_clock(cx.hud().view(cx.objects()).utc_offset_secs()),
        world_message: pregame.character_screen_message.clone(),
        targeting: cx.target_mode() != dereth_client_runtime::interaction::TargetMode::None,
        armed: (cx.target_mode() != dereth_client_runtime::interaction::TargetMode::None)
            .then(|| cx.objects().world.targeting_object)
            .filter(|id| id.0 != 0),
        connected: pregame.connected || pregame.has_packet_controller,
        session: phase_words(&pregame.phase),
        entering: pregame.phase == GamePhase::EnteringWorld,
        connect_phase: connect_phase(&pregame.phase),
        in_world: pregame.in_world,
        ..GameState::default()
    };
    if let Some(set) = &pregame.character_set {
        g.characters = set
            .set
            .iter()
            .map(|c| CharacterEntry {
                id: c.id,
                name: c.name.clone(),
                // A character held for deletion is listed with the seconds left to restore it.
                delete_seconds: c.seconds_grace_period,
            })
            .chain(set.del_set.iter().map(|c| CharacterEntry {
                id: c.id,
                name: c.name.clone(),
                delete_seconds: c.seconds_grace_period.max(1),
            }))
            .collect();
        g.character_slots = set.num_allowed_characters;
        g.account_has_tod = set.is_throne_of_destiny;
        g.free_slot = (0..set.num_allowed_characters as usize)
            .find(|&i| set.set.get(i).is_none_or(|c| c.id.0 == 0))
            .and_then(|i| i32::try_from(i).ok());
    }
    g.chargen_response = pregame.chargen_response;
    g.chargen_response_notices = pregame.chargen_response_notices;
    g.chargen_seeds = pregame.chargen_seeds;
    g.new_chat = chat.into_iter().map(ChatLine::from_message).collect();
    chat_entry_state(cx, &mut g);
    if !g.in_world {
        return g;
    }
    let v = cx.hud().view(cx.objects());
    g.world_services = crate::ui::panels::world::read(&v);
    let Some(me) = v.player() else {
        return g;
    };
    g.name = v.character_name().unwrap_or_default().to_owned();
    g.level = v
        .int_stat(me, int::LEVEL)
        .and_then(|l| u32::try_from(l).ok())
        .unwrap_or(0);
    g.health = vital(&v, me, ViewVital::Health);
    g.stamina = vital(&v, me, ViewVital::Stamina);
    g.mana = vital(&v, me, ViewVital::Mana);
    if let Some(xp) = v.experience_header() {
        g.total_xp = xp.total;
        g.xp_this = Some(xp.into_level);
        g.xp_next = Some(xp.level_span);
    }
    g.unassigned_xp = u64::try_from(v.available_experience()).unwrap_or(0);
    g.pyreals = v
        .int_stat(me, int::COIN_VALUE)
        .and_then(|c| u64::try_from(c).ok())
        .unwrap_or(0);
    g.burden = v.load();
    g.vitae = v.vitae();
    g.vitae_display = v.vitae_display();
    g.combat_mode = v.combat_mode();
    g.coords = v.player_coords();
    g.heading = v.player_heading();
    g.title = v.display_title().unwrap_or_default();
    g.heritage = v.gender_heritage_display().unwrap_or_default();
    g.effects = v
        .active_effects()
        .into_iter()
        .map(|e| Effect {
            spell: e.spell,
            name: e.name,
            ac_icon: e.icon.map_or(0, |d| d.0),
            harmful: !e.beneficial,
            remaining: (!e.permanent).then_some(e.remaining),
        })
        .collect();
    // The radar shows what the game's radar shows: only objects whose radar setting lets them
    // be seen, in the game's colours and shapes, within its reach.
    {
        use dereth_ui_screens::mapradar::radar as retail;
        let objects = v.radar_objects();
        let me = objects.iter().find(|r| r.is_self);
        g.radar_range = retail::radar_range(v.player_outside());
        g.blips = objects
            .iter()
            .filter(|r| !r.is_self && retail::inq_showable_on_radar(r))
            .map(|r| Blip {
                id: r.id,
                dx: r.player_space.0,
                dy: r.player_space.1,
                kind: if Some(r.id) == v.selection() {
                    BlipKind::Selected
                } else {
                    blip_kind(&v, r)
                },
                colour: 0xFF00_0000 | retail::get_blip_color(Some(r)).hex,
                shape: retail::get_blip_shape(Some(r), me) as u8,
                name: v.name(r.id).unwrap_or_default().to_owned(),
            })
            .collect();
        g.targets = targets_of(
            objects,
            |r| blip_kind(&v, r),
            |id| v.name(id).unwrap_or_default().to_owned(),
        );
    }
    g.player_module_strings = v.player_module_strings();
    // The shortcut bar's eighteen slots.
    for slot in 0..18u32 {
        if let Some(id) = v.shortcut(slot) {
            g.shortcuts.push(Shortcut {
                index: slot,
                name: v.name(id).unwrap_or_default().to_owned(),
                object: Some(id),
                spell: None,
                ac_icon: v.icon(id).map(|d| d.0),
                cooldown: v.slot_decoration(id).and_then(|d| {
                    let left = v.cooldown_remaining(d.cooldown_id, v.now())?;
                    (d.cooldown_id != 0 && left > 0.0).then_some((left, d.cooldown_duration))
                }),
                look: Some(item_of(&v, id)),
            });
        }
    }
    // The spellbook and the spell bar's tabs.
    g.spells = v
        .spellbook()
        .iter()
        .map(|s| Spell {
            id: s.id,
            name: s.name.clone(),
            school: s.school,
            level: s.level,
            ac_icon: s.icon.map_or(0, |d| d.0),
            icon_power: s.icon_power,
            bitfield: s.bitfield,
        })
        .collect();
    g.spell_tabs = (0..8).map(|t| v.spell_tab(t).to_vec()).collect();
    // The packs and what is worn.
    let item = |id: dereth_primitives::ObjectId, worn: u32, container: bool| Item {
        worn,
        container,
        ..item_of(&v, id)
    };
    g.player_id = Some(me);
    g.main_pack = Some({
        let mut pack = item_of(&v, me);
        // The game draws the player as the backpack whatever their own icon is.
        let mut d = pack.decoration.unwrap_or_default();
        d.is_player = true;
        pack.decoration = Some(d);
        pack
    });
    g.pack_capacity = v
        .slot_decoration(me)
        .and_then(|d| u32::try_from(d.items_capacity).ok())
        .filter(|c| *c > 0);
    g.pack = v
        .container_contents(me)
        .iter()
        .map(|id| item(*id, 0, false))
        .collect();
    g.side_packs = v
        .contained_containers(me)
        .iter()
        .map(|pack| {
            let contents = v
                .container_contents(*pack)
                .iter()
                .map(|id| item(*id, 0, false))
                .collect();
            (item(*pack, 0, true), contents)
        })
        .collect();
    g.equipped = v
        .equipment(me)
        .iter()
        .map(|(id, loc)| item(*id, *loc, false))
        .collect();
    // Attributes and skills.
    let attributes = [
        (1, "Strength"),
        (2, "Endurance"),
        (4, "Coordination"),
        (3, "Quickness"),
        (5, "Focus"),
        (6, "Self"),
    ];
    g.attributes = attributes
        .iter()
        .filter_map(|(id, name)| {
            v.attribute(*id)
                .and_then(|a| u32::try_from(a).ok())
                .map(|a| ((*name).to_owned(), a))
        })
        .collect();
    g.skills = v
        .skills()
        .iter()
        .filter(|s| s.sac >= 1)
        .map(|s| {
            let adv = v.skill_advancement(s.id).unwrap_or_default();
            Skill {
                name: s.name.clone(),
                value: u32::try_from(s.level).unwrap_or(0),
                training: s.sac,
                id: s.id,
                icon: s.icon.map(|d| d.0),
                cost: adv.cost_to_raise,
                cost_10: adv.cost_to_raise_10,
            }
        })
        .collect();
    g.skill_credits = v.skill_credits();
    // The attributes in the game's own order, then the three vitals by their maxima.
    let stat = |wire: u32, vital: bool, name: &str, shown: String| {
        let adv = v.attribute_advancement(wire, vital).unwrap_or_default();
        let colour = match adv.effective.cmp(&adv.value) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => 2,
            std::cmp::Ordering::Equal => 0,
        };
        StatRow {
            name: name.to_owned(),
            wire,
            vital,
            shown,
            colour,
            cost: adv.cost_to_raise,
            cost_10: adv.cost_to_raise_10,
        }
    };
    g.stats = [
        (1, "Strength"),
        (2, "Endurance"),
        (4, "Coordination"),
        (3, "Quickness"),
        (5, "Focus"),
        (6, "Self"),
    ]
    .iter()
    .map(|(id, name)| {
        let shown = v
            .attribute(*id)
            .map_or_else(|| "???".to_owned(), |a| a.to_string());
        stat(*id, false, name, shown)
    })
    .collect();
    for (wire, name, now) in [
        (1, "Health", g.health),
        (3, "Stamina", g.stamina),
        (5, "Mana", g.mana),
    ] {
        g.stats.push(stat(
            wire,
            true,
            name,
            format!("{} / {}", now.current, now.max),
        ));
    }
    // The fellowship, the allegiance, the friends and the contracts.
    if let Some(f) = v.fellowship() {
        g.fellowship = f
            .members
            .iter()
            .map(|m| {
                (
                    m.name.clone(),
                    Vital {
                        current: m.current_health,
                        max: m.max_health,
                    },
                    Vital {
                        current: m.current_stamina,
                        max: m.max_stamina,
                    },
                    Vital {
                        current: m.current_mana,
                        max: m.max_mana,
                    },
                    m.id == f.leader,
                )
            })
            .collect();
    }
    g.contracts = v.contracts();
    g.components = v.spell_components();
    g.titles = v.character_titles();
    g.character_info = v.character_info();
    g.packet_loss = v.packet_loss_percent();
    // How lately the server was heard, as the modern interface's lamp reads it each frame.
    g.link_quiet = dereth_client_runtime::net::link_status_holder::connection_status(cx.now());
    g.map_profile = dereth_client_contract::panels::map::profile(&v);
    g.fellowship_view = v.fellowship();
    g.squelches = v.squelch_list();
    g.journal = v.journal();
    g.notebook = v.era_features().journal;
    g.friend_list = v.friends();
    g.options = dereth_client_contract::view::PlayerOption::ALL
        .iter()
        .map(|o| (*o, v.player_option(*o)))
        .collect();
    let roster = v.allegiance_roster();
    g.roster = roster.clone();
    if let Some(m) = &roster.monarch {
        g.allegiance.push(("Monarch".into(), m.full_name.clone()));
    }
    if let Some(p) = &roster.patron {
        g.allegiance.push(("Patron".into(), p.full_name.clone()));
    }
    for vassal in &roster.vassals {
        g.allegiance
            .push(("Vassal".into(), vassal.full_name.clone()));
    }
    g.friends = v
        .friends()
        .into_iter()
        .map(|f| (f.name, f.online))
        .collect();
    g.duties = v
        .contracts()
        .into_iter()
        .map(|c| {
            let mut lines = Vec::new();
            if !c.status.is_empty() {
                lines.push(c.status);
            }
            if !c.timed.is_empty() {
                lines.push(c.timed);
            }
            (c.name, lines)
        })
        .collect();
    if let Some(sel) = v.selection() {
        let facts = v.selection_query_facts(sel);
        let relation = match facts {
            Some(f) if f.is_player => Relation::Player,
            Some(f) if f.attackable => Relation::Hostile,
            // A person: a creature that cannot be fought (a vendor, a guard), whether or not the
            // game sends a level for it, as the radar tells them apart.
            Some(_)
                if v.int_stat(sel, int::LEVEL).is_some()
                    || v.slot_decoration(sel)
                        .is_some_and(|d| d.obj_type & ITEM_TYPE_CREATURE != 0) =>
            {
                Relation::Npc
            }
            _ => Relation::Object,
        };
        g.target = Some(Target {
            look: Some(item_of(&v, sel)),
            id: sel,
            name: v.name(sel).unwrap_or_default().to_owned(),
            relation,
            health: v.selected_meters().0,
            level: v
                .int_stat(sel, int::LEVEL)
                .and_then(|l| u32::try_from(l).ok()),
            icon: v.icon(sel).map(|d| d.0),
        });
    }
    // A selection to pick up off the ground: an item of no one's. (One in the chest or corpse
    // open is the loot window's.)
    if let Some(sel) = v.selection() {
        let owned = v
            .selection_query_facts(sel)
            .is_some_and(|f| f.owned_by_player);
        let item_type = v.slot_decoration(sel).map_or(0, |d| d.obj_type);
        // A thing used where it stands (a door, a lever) is used, not picked up.
        g.target_usable_here = v
            .useability(sel)
            .is_some_and(|u| u & (USEABLE_REMOTE | USEABLE_VIEWED) != 0);
        g.target_pickable = !owned && !g.target_usable_here && is_loose_item(item_type);
        const CONTAINER: u32 = 0x200;
        g.target_container = !owned && item_type & CONTAINER != 0;
        g.target_stuck = v
            .radar_objects()
            .iter()
            .find(|r| r.id == sel)
            .is_some_and(|r| r.bitfield & bits::STUCK != 0);
    }
    // The options pages' state: the world's era, the chat windows' filters and opacities.
    g.era = Some(v.era_features());
    {
        use dereth_client_contract::options::sheet::window;
        g.chat_filters = [
            window::MAIN,
            window::FLOATY_1,
            window::FLOATY_2,
            window::FLOATY_3,
            window::FLOATY_4,
        ]
        .into_iter()
        .filter_map(|w| v.chat_window_filter(w).map(|f| (w, f)))
        .collect();
    }
    g.chat_opacity = [0x1000_0080, 0x1000_0081].map(|p| v.gameplay_option_float(p));
    g
}

/// The Preference string table's enum: the option labels and choices.
const PREFERENCE_TABLE: u32 = 0x1000_0003;

/// The game's own words for every option the Options window offers: the shared preference table,
/// each label and choice looked up in the Preference string table by its token.
#[must_use]
pub fn option_rows(
    store: &dereth_dat::RetailDatStore,
    strings: &mut crate::strings::Strings,
) -> Vec<crate::ui::panels::OptionRow> {
    use dereth_client_contract::options::{preferences::UI_PREFERENCES, store as prefs};
    UI_PREFERENCES
        .iter()
        .filter(|p| prefs::is_registered(p.name))
        .map(|p| {
            let values = prefs::inq_choice_values(p.name).unwrap_or(&[]);
            let mut choices: Vec<(i32, String)> = p
                .choices
                .iter()
                .enumerate()
                .map(|(i, token)| {
                    let value = values
                        .get(i)
                        .copied()
                        .unwrap_or_else(|| i32::try_from(i).unwrap_or(0));
                    (value, strings.text(store, PREFERENCE_TABLE, token))
                })
                .collect();
            // The resolution and the refresh rate: their choices are the display's own modes,
            // built at start-up, with the labels as the game writes them.
            if choices.is_empty() {
                choices = prefs::display_choices(p.name)
                    .into_iter()
                    .map(|c| (c.value, c.label))
                    .collect();
            }
            crate::ui::panels::OptionRow {
                name: p.name,
                label: strings.text(store, PREFERENCE_TABLE, p.label),
                kind: p.kind,
                range: p.range,
                choices,
            }
        })
        .collect()
}

/// How near, in metres, something must be for its name to show over it.
pub const NAMEPLATE_RANGE: f32 = 30.0;

/// The objects whose names may show over them this frame: each blip within range that is not a
/// thing.
#[must_use]
pub fn nameplate_candidates(state: &GameState) -> Vec<dereth_primitives::ObjectId> {
    // The selection is marked wherever it is and whatever it is: a door, a corpse or a thing on
    // the ground too, which the radar does not show.
    let mut ids: Vec<dereth_primitives::ObjectId> = state
        .blips
        .iter()
        .filter(|b| {
            b.kind == BlipKind::Selected
                || (b.dx * b.dx + b.dy * b.dy <= NAMEPLATE_RANGE * NAMEPLATE_RANGE
                    && b.kind != BlipKind::Item)
        })
        .map(|b| b.id)
        .collect();
    if let Some(t) = &state.target {
        if !ids.contains(&t.id) {
            ids.push(t.id);
        }
    }
    ids.retain(|id| !in_a_container(state, *id));
    ids
}

/// Whether `id` is in a pack, worn or in an open chest or corpse rather than in the world: a
/// thing just picked up keeps its last place in the world for a while, and is not marked there.
#[must_use]
pub fn in_a_container(state: &GameState, id: dereth_primitives::ObjectId) -> bool {
    state
        .pack
        .iter()
        .chain(state.equipped.iter())
        .chain(
            state
                .side_packs
                .iter()
                .flat_map(|(pack, items)| std::iter::once(pack).chain(items.iter())),
        )
        .chain(state.loot.iter().flat_map(|(_, _, items)| items.iter()))
        .any(|i| i.id == id)
}

/// Useability bits: usable at a distance, and while viewed.
const USEABLE_REMOTE: u32 = 0x20;
const USEABLE_VIEWED: u32 = 0x10;

/// Whether an object of `item_type` is a thing to pick up off the ground: an item, not a
/// creature, a container, a portal, a door or a lifestone.
#[must_use]
pub fn is_loose_item(item_type: u32) -> bool {
    const CREATURE: u32 = 0x10;
    const CONTAINER: u32 = 0x200;
    const PORTAL: u32 = 0x1_0000;
    const LOCKABLE: u32 = 0x2_0000;
    const LIFESTONE: u32 = 0x1000_0000;
    item_type != 0 && item_type & (CREATURE | CONTAINER | PORTAL | LOCKABLE | LIFESTONE) == 0
}

/// Where an object is drawn on screen this frame, as the shell's projection of it says.
pub type Projections = std::collections::BTreeMap<
    dereth_primitives::ObjectId,
    dereth_client_contract::target::Projection,
>;

/// Where the top of each object's body as drawn stands on screen, as the shell projected it.
pub type Tops = std::collections::BTreeMap<dereth_primitives::ObjectId, (f32, f32)>;

/// The names over the people and creatures near the player: each blip within range whose box is
/// on screen, standing on the top of its body as drawn (`tops`); on the top of its box where that
/// is not known.
pub fn nameplates(
    v: &impl GameView,
    state: &GameState,
    projections: &Projections,
    origins: &std::collections::BTreeMap<dereth_primitives::ObjectId, (i32, i32)>,
    tops: &Tops,
    places: &std::collections::BTreeMap<dereth_primitives::ObjectId, (bool, f32)>,
) -> Vec<crate::ui::game::Nameplate> {
    let selected = state.target.as_ref().map(|t| t.id);
    let outside = v.player_outside();
    nameplate_candidates(state)
        .into_iter()
        .filter_map(|id| {
            let kind = if Some(id) == selected {
                BlipKind::Selected
            } else {
                state.blips.iter().find(|b| b.id == id)?.kind
            };
            // A name indoors is not shown to a player outdoors, nor the reverse; the selection's
            // is shown wherever it is.
            let place = places.get(&id).copied();
            if !name_shown(outside, place, kind == BlipKind::Selected) {
                return None;
            }
            let distance = place.map(|(_, d)| d);
            let dereth_client_contract::target::Projection::OnScreen((x0, y0, x1, y1)) =
                *projections.get(&id)?
            else {
                return None;
            };
            // On the top of the body as drawn, over its own origin. Where that is not known, the
            // box's top, centred on the origin where that is known (the box swings with what it
            // holds and how it stands).
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = tops.get(&id).copied().unwrap_or_else(|| {
                (
                    origins
                        .get(&id)
                        .map_or((x0 + x1) as f32 / 2.0, |(ox, _)| *ox as f32),
                    y0 as f32,
                )
            });
            #[allow(clippy::cast_precision_loss)]
            let bounds = crate::draw::Rect::new(
                x0 as f32,
                y0 as f32,
                (x1 - x0).max(0) as f32,
                (y1 - y0).max(0) as f32,
            );
            Some(crate::ui::game::Nameplate {
                id,
                name: v.name(id).unwrap_or_default().to_owned(),
                x,
                y,
                kind,
                selected: kind == BlipKind::Selected,
                bounds,
                distance,
                nearness: distance.map_or(1.0, nearness),
            })
        })
        .filter(|n| !n.name.is_empty())
        .collect()
}

/// Whether a name is shown to a player `outside` (or not) for an object at `place` (outdoors or
/// not, and how far): not across the line between indoors and outdoors, unless it is the
/// selection's.
#[must_use]
pub fn name_shown(outside: bool, place: Option<(bool, f32)>, selected: bool) -> bool {
    selected || place.is_none_or(|(out, _)| out == outside)
}

/// How near a name stands, for fading it: 1 within [`NAMEPLATE_FADE`] of the edge of
/// [`NAMEPLATE_RANGE`], falling to 0 at the edge, `distance` in metres.
#[must_use]
pub fn nearness(distance: f32) -> f32 {
    ((NAMEPLATE_RANGE - distance) / NAMEPLATE_FADE).clamp(0.0, 1.0)
}

/// The distance over which a name fades out at the edge of [`NAMEPLATE_RANGE`], in metres.
pub const NAMEPLATE_FADE: f32 = 6.0;

/// The examine window's pane for `id`, from the game's appraisal of it, in the words the game's
/// own examine panel builds: the item's description blocks, or a creature's or a player's
/// attributes, vitals and extra rows.
pub fn examined(
    view: &dyn dereth_client_contract::view::GameView,
    id: dereth_primitives::ObjectId,
) -> Option<crate::ui::game::Examined> {
    use crate::ui::game::{ExaminePane, Examined};
    use dereth_ui_screens::panels::examination as ex;
    let p = view.appraisal(id)?;
    let name = view.name(id)?.to_owned();
    let stack = view.slot_decoration(id).map_or(1, |d| d.stack_size);
    let pane = if p.creature {
        if p.template || p.character_title {
            ExaminePane::Character
        } else {
            ExaminePane::Creature
        }
    } else {
        ExaminePane::Item
    };
    let display = p.gear_plating_name.clone().unwrap_or_else(|| name.clone());
    let mut e = Examined {
        id,
        pane,
        title: ex::title_text(&display, stack),
        icon: view.icon(id).map(|d| d.0),
        look: Some(item_of(view, id)),
        runs: Vec::new(),
        inscription: None,
        inscription_editable: false,
        inscription_value: String::new(),
        scribe: String::new(),
        level: ex::level_value(p.level),
        rows: Vec::new(),
        misc: Vec::new(),
        heritage: None,
        profession: None,
        pk: None,
        allegiance: None,
    };
    match pane {
        ExaminePane::Item => {
            e.scribe = p.scribe_name.clone().unwrap_or_default();
            if !e.scribe.is_empty() {
                e.inscription_value = p.inscription.clone().unwrap_or_default();
            }
            e.inscription_editable = dereth_client_contract::examination::inscription_editable(
                view.inscription_mouse_facts(id),
                p.owned_by_player,
                p.viewer_is_psr,
                &e.scribe,
                view.character_name().unwrap_or_default(),
            );
            e.runs = ex::item_description_runs(&p)
                .into_iter()
                .map(|r| (r.text, r.same_line, r.color))
                .collect();
            e.inscription = ex::inscription_text(
                p.inscribable,
                p.scribe_name.as_deref(),
                p.inscription.as_deref(),
            )
            .map(|text| {
                (
                    text,
                    ex::inscription_signature(
                        p.inscribable,
                        p.scribe_name.as_deref(),
                        p.inscription.as_deref(),
                    ),
                )
            });
        }
        ExaminePane::Creature | ExaminePane::Character => {
            let colours = ex::creature_row_colors(&p);
            e.rows = ex::creature_rows(&p)
                .into_iter()
                .enumerate()
                .map(|(i, (l, v))| (l, v, colours.get(i).copied().unwrap_or(0)))
                .collect();
            let misc = if pane == ExaminePane::Character {
                ex::char_misc_rows(&p)
            } else {
                ex::creature_misc_rows(&p)
            };
            e.misc = misc
                .into_iter()
                .map(|m| (m.label, m.value, m.color))
                .collect();
            if pane == ExaminePane::Character {
                e.heritage = p.gender_heritage_display.clone();
                e.profession = p.profession.clone();
                e.pk = Some(ex::pk_status_text(p.weenie_is_pk, p.weenie_is_pk_lite).to_owned());
                if let Some(t) = p.allegiance_title.as_deref() {
                    e.title = format!("{t} {name}");
                } else {
                    e.title = name;
                }
                if p.allegiance_rank.unwrap_or(0) >= 1 {
                    e.allegiance = p.allegiance_name.clone();
                }
            } else if let Some(d) = p.creature_display_name.clone() {
                e.title = d;
            }
        }
    }
    Some(e)
}

/// The chest or corpse open on the ground: its name and what is in it, as the pack's items are
/// read.
pub fn loot(
    view: &dyn dereth_client_contract::view::GameView,
    id: dereth_primitives::ObjectId,
) -> Option<(dereth_primitives::ObjectId, String, Vec<Item>)> {
    let name = view.name(id)?.to_owned();
    let items = view
        .container_contents(id)
        .iter()
        .map(|i| item_of(view, *i))
        .collect();
    Some((id, name, items))
}

/// The character's own options as the shared options sheet lists them for this interface: each
/// heading and its switches, labelled and explained from the game's string table where it has
/// the words, and in the sheet's own words where it does not.
pub fn character_options(
    store: &dereth_dat::RetailDatStore,
    strings: &mut crate::strings::Strings,
) -> Vec<(String, Vec<crate::ui::panels::CharacterOption>)> {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{headings_for, PageId, Value};
    use dereth_ui_screens::options::character as page;
    headings_for(PageId::Character, Interface::Horizon)
        .map(|(heading, rows)| {
            let title = heading
                .text
                .token
                .and_then(|t| strings.lookup(store, heading.text.table_enum, t))
                .unwrap_or_else(|| heading.title.to_owned());
            let options = rows
                .iter()
                .filter(|row| !crate::ui::panels::options::hidden_row(row))
                .filter_map(|row| {
                    let Value::Option(o) = row.value else {
                        return None;
                    };
                    Some(crate::ui::panels::CharacterOption {
                        option: o,
                        label: strings
                            .lookup(store, page::STRING_TABLE_ENUM, &page::label_token(o))
                            .unwrap_or_else(|| row.caption_for(Interface::Horizon).to_owned()),
                        help: strings
                            .lookup(store, page::STRING_TABLE_ENUM, &page::help_token(o))
                            .or_else(|| row.note.map(str::to_owned))
                            .unwrap_or_default(),
                        needs: row.needs,
                    })
                })
                .collect::<Vec<_>>();
            (title, options)
        })
        .filter(|(_, options)| !options.is_empty())
        .collect()
}

/// One object as an item tile shows it.
pub fn item_of(
    view: &dyn dereth_client_contract::view::GameView,
    id: dereth_primitives::ObjectId,
) -> Item {
    let deco = view.slot_decoration(id);
    Item {
        id,
        name: view.name(id).unwrap_or_default().to_owned(),
        ac_icon: view.icon(id).map(|d| d.0),
        underlay: deco.as_ref().and_then(|d| d.icon_underlay_id).map(|d| d.0),
        overlay: deco.as_ref().and_then(|d| d.icon_overlay_id).map(|d| d.0),
        stack: deco.as_ref().map(|d| d.stack_size),
        worn: 0,
        equip_locations: view.equip_locations(id),
        attuned: view.item_attuned(id),
        wcid: view.item_wcid(id),
        container: deco.as_ref().is_some_and(|d| d.is_container),
        cooldown: deco.as_ref().and_then(|d| {
            let left = view.cooldown_remaining(d.cooldown_id, view.now())?;
            (d.cooldown_id != 0 && left > 0.0).then_some((left, d.cooldown_duration))
        }),
        decoration: deco,
    }
}

/// The time on the player's own clock, "8:43 PM": now, moved by the zone's offset from UTC.
fn local_clock(offset_secs: i32) -> Option<String> {
    let unix = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let local = i64::try_from(unix).ok()? + i64::from(offset_secs);
    let of_day = local.rem_euclid(86_400);
    let (h, m) = (of_day / 3600, of_day % 3600 / 60);
    let (h12, half) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    Some(format!("{h12}:{m:02} {half}"))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn doors_and_corpses_are_targets_but_the_player_and_what_the_interface_hides_are_not() {
        use dereth_client_contract::view::RadarEntry;
        use dereth_primitives::ObjectId;
        const DOOR: u32 = 0x1000;
        const CORPSE: u32 = 0x2000;
        let entry = |id: u32, bitfield: u32| RadarEntry {
            id: ObjectId(id),
            bitfield,
            in_world: true,
            // Shown on no radar: a door or corpse is a target all the same.
            radar_enum: 0,
            ..RadarEntry::default()
        };
        let objects = [
            entry(0x7000_0001, DOOR),
            entry(0x7000_0002, CORPSE),
            entry(0x7000_0003, bits::UI_HIDDEN),
            RadarEntry {
                is_self: true,
                ..entry(0x5000_0001, 0x8)
            },
            RadarEntry {
                in_world: false,
                ..entry(0x7000_0004, 0)
            },
            entry(0x7000_0005, 0),
        ];
        let name = |id: ObjectId| {
            if id.0 == 0x7000_0005 {
                String::new()
            } else {
                format!("Thing {:x}", id.0)
            }
        };
        let ids: Vec<u32> = targets_of(&objects, |_| BlipKind::Item, name)
            .iter()
            .map(|b| b.id.0)
            .collect();
        assert_eq!(
            ids,
            [0x7000_0001, 0x7000_0002],
            "the door and the corpse only"
        );
    }

    #[test]
    fn a_selection_just_picked_up_is_not_marked_where_it_lay() {
        let coin = dereth_primitives::ObjectId(0x8000_0042);
        let mut state = GameState {
            target: Some(Target {
                id: coin,
                ..Target::default()
            }),
            ..GameState::default()
        };
        assert_eq!(nameplate_candidates(&state), [coin], "on the ground");
        state.pack.push(crate::ui::game::Item {
            id: coin,
            ..crate::ui::game::Item::default()
        });
        assert!(nameplate_candidates(&state).is_empty(), "in the pack");
    }

    #[test]
    fn character_select_is_ready_only_after_the_data_update_and_its_progress_is_followed() {
        use crate::ui::game::ConnectPhase as P;
        use dereth_client_contract::pregame::DddEvent as E;
        assert_eq!(connect_phase(&GamePhase::Connecting), P::Connecting);
        assert_eq!(connect_phase(&GamePhase::Patching), P::Updating);
        assert_eq!(connect_phase(&GamePhase::CharacterSelect), P::Ready);
        let mut p = PatchProgress::default();
        p.follow(&[E::PatchtimeInterrogation]);
        assert_eq!(p.shown(), None, "nothing said yet");
        p.follow(&[
            E::PatchtimeBegin { expected: 1000 },
            E::DataDownloaded { bytes: 300 },
        ]);
        p.follow(&[E::DataDownloaded { bytes: 200 }]);
        assert_eq!(p.shown(), Some((500, 1000)));
        p.follow(&[E::PatchtimeEnd]);
        assert_eq!(p.shown(), None, "done");
    }

    #[test]
    fn a_name_is_not_shown_across_the_line_between_indoors_and_outdoors_but_the_selection_s_is() {
        assert!(name_shown(true, Some((true, 5.0)), false));
        assert!(
            !name_shown(true, Some((false, 5.0)), false),
            "indoors, seen from outdoors"
        );
        assert!(
            !name_shown(false, Some((true, 5.0)), false),
            "outdoors, seen from indoors"
        );
        assert!(
            name_shown(false, Some((true, 5.0)), true),
            "the selection's"
        );
        assert!(name_shown(true, None, false), "no place known");
    }

    #[test]
    fn a_name_fades_over_the_last_metres_of_its_range() {
        assert!((nearness(1.0) - 1.0).abs() < f32::EPSILON);
        assert!((nearness(NAMEPLATE_RANGE - NAMEPLATE_FADE) - 1.0).abs() < 1e-6);
        assert!((nearness(NAMEPLATE_RANGE - NAMEPLATE_FADE / 2.0) - 0.5).abs() < 1e-6);
        assert!(nearness(NAMEPLATE_RANGE).abs() < f32::EPSILON);
        assert!(nearness(NAMEPLATE_RANGE + 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_selected_door_corpse_or_ground_item_is_marked_though_the_radar_does_not_show_it() {
        let door = dereth_primitives::ObjectId(0x7A9B_0001);
        let state = GameState {
            target: Some(Target {
                id: door,
                name: "Door".into(),
                relation: crate::ui::game::Relation::Object,
                health: None,
                level: None,
                icon: None,
                look: None,
            }),
            ..GameState::default()
        };
        assert_eq!(nameplate_candidates(&state), [door]);
    }
}
