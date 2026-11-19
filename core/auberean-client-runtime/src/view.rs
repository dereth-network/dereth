//! The game as a front end reads it ([`auberean_client_contract::view::View`]), kept current by
//! [`Client::refresh_view`] and read with [`Client::view`].
//!
//! A refresh reads the model, the session and the panels' caches, which are cheap to copy and
//! copied only when their version moved; and it asks the player's scripts what the panels read of
//! them, as the retail panels ask:
//! - the selection, the one before it and whether the player targets what attacks it, every
//!   refresh; the radar's ranges and the player's radar coordinates and names options, once, and
//!   again when the player's options come back or the player changed them;
//! - each object's name (once, and again when the scripts say its radar look or the item changed),
//!   its kind, groove, owner, container, whether it may be selected and whether it is dead, every
//!   refresh; how it shows on the radar, once, and again when the scripts say its look changed;
//! - which objects are fellows and which the player may attack, only when asked
//!   ([`Action::AskTargets`]); the fellows' shares of the experience, when the fellowship's notices
//!   come;
//! - the player's options, once the player is here, and again when the scripts say they or the
//!   chat's settings changed or the player acted;
//! - the player's quests, once the player is here, and again when the scripts say the journal
//!   changed; and the markers the scripts put on the map, kept by the map's rules while the
//!   player is in the world.
//!
//! What the scripts tell the panels at one refresh marks the parts it changes for the next: the
//! panels hear a notice in the frame it comes, and read what it changed a frame later.

use std::collections::{BTreeMap, BTreeSet};

use auberean_client_contract::notice::{MapNotice, QuestNotice, Said};
use auberean_client_contract::view::{
    self as v, ChatOptions, ChatServiceRefusal, Clock, CreationAnswer, GameOptions, IconLayers,
    Player, Pregame, Refusal, Surroundings, Thing, View,
};
use auberean_client_contract::{Action, UiLocation};
use auberean_client_model::Object;
use auberean_client_net::LocalTime;
use auberean_protocol::types::{VisualDesc, WeenieDesc};

use crate::interface::function;
use crate::text::{from_script, from_wire, to_view};
use crate::Client;
use crate::InterfaceArg;

/// What makes an object's icon from its look: the layers of its description's chain and its
/// appearance. A host without the world's records makes none.
pub type Icons<'a> = &'a dyn Fn(&VisualDesc) -> IconLayers;

/// Qualities the panels read: the kind bits, a pet's owner, what holds an item, the groove
/// level; whether it may be selected, whether it is dead; the player's race, whether it is a hero
/// and its focus.
pub(crate) mod quality {
    pub const KIND: u32 = 20;
    pub const GROOVE: u32 = 950;
    pub const OWNER: u32 = 505;
    pub const CONTAINER: u32 = 261;
    pub const SELECTABLE: u32 = 624;
    pub const DEAD: u32 = 279;
    pub const RACE: u32 = 0x122;
    /// The kingdom a player is in (none: in none).
    pub const KINGDOM: u32 = 2000;
    /// Health and its greatest, vigor and its greatest.
    pub const VITALS: [u32; 4] = [258, 263, 259, 264];
    /// The player's level (a whole number); its experience in all and to spend (64-bit).
    pub const LEVEL: u32 = 302;
    pub const TOTAL_EXPERIENCE: u32 = 300;
    pub const AVAILABLE_EXPERIENCE: u32 = 301;
    /// The vitae (a float) and the experience to earn for its next point (64-bit).
    pub const VITAE: u32 = 286;
    pub const VITAE_EXPERIENCE: u32 = 316;
    /// Whether the player is a hero (a truth); its focus, and what is added to the hundred its
    /// greatest focus starts from (whole numbers).
    pub const HERO: u32 = 4000;
    pub const FOCUS: u32 = 266;
    pub const MORE_FOCUS: u32 = 267;
    /// The greatest focus before what the qualities add.
    pub const BASE_FOCUS: u32 = 100;
}

/// The player's scripts' notices a refresh hears.
pub mod notice {
    /// An object's radar look changed (about the object; about the player, every object's): its
    /// name too.
    pub const CHANGE_RADAR_LOOK: u32 = 0x0085 << 16 | 0x000A;
    /// The player's options came back.
    pub const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
    /// The chat's settings changed.
    pub const RELOAD_CHAT_SETTINGS: u32 = 0x0087 << 16 | 0x0004;
    /// The options window changed the player's options.
    pub const OPTIONS_CHANGED: u32 = 0x46D;
    /// An item changed, its quantity among others.
    pub const ITEM_UPDATE: u32 = 0x008B << 16 | 0x0004;
    /// A fellowship or a fellow described, a fellow gone, a fellow's level.
    pub const FELLOWS: [u32; 4] = [
        0x008A << 16 | 0x0001,
        0x008A << 16 | 0x0003,
        0x008A << 16 | 0x0004,
        0x008A << 16 | 0x0005,
    ];
    /// A sequence of the story's scenes to play.
    pub const PLAY_SEQUENCE: u32 = 0x008E << 16 | 0x0003;
}

/// What the scripts said of the map's markers, kept as the map keeps them, on a land of `land`
/// cells (none known: no marker is refused for standing near another).
// D2-02583
fn markers_told(markers: &mut v::MapMarkers, said: MapNotice, land: Option<(i32, i32)>) {
    match said {
        MapNotice::MarkerAdded {
            kind,
            object,
            cell,
            picture,
            label,
        } => {
            markers.add(
                v::MapMarker {
                    kind,
                    object,
                    cell,
                    picture,
                    label,
                },
                land.unwrap_or((0, 0)),
            );
        }
        MapNotice::MarkerRemoved { kind, object } => {
            markers.remove(kind, object);
        }
        MapNotice::MarkersCleared { kind } => markers.clear(kind),
        _ => {}
    }
}

/// The radar shape of an object that does not show on the radar.
const NO_BLIP: u32 = 0x4000_000B;

/// The kinds of text the chat's options hold a font for: a bit each.
const TEXT_KINDS: u32 = 25;

/// What the scripts said of one object.
#[derive(Clone, Debug, Default, PartialEq)]
struct Facts {
    name: Option<String>,
    name_ref: Option<auberean_text::StringInfo>,
    kind: u32,
    groove: i8,
    owner: Option<u64>,
    container: Option<u64>,
    selectable: bool,
    dead: bool,
    /// How it shows on the radar (`None`: not on it); asked once.
    radar: Option<Option<([f32; 4], u32)>>,
    fellow: bool,
    attackable: bool,
    owner_fellow: bool,
}

/// The view and what keeping it current needs.
#[derive(Debug, Default)]
pub struct Views {
    view: View,
    /// The notices of the last refresh, read at this one: each by its native and the object it is
    /// about.
    heard: Vec<(u32, Option<u64>)>,
    /// The player acted since the last refresh.
    acted: bool,
    ask_targets: bool,
    ask_places: bool,
    facts: BTreeMap<u64, Facts>,
    ranges: Option<(f32, f32)>,
    world_options: Option<(bool, bool)>,
    fellow_xp: Vec<(u64, f32)>,
    /// The player whose name was read, and the name.
    named: Option<(u64, Option<String>)>,
    /// The game's questions open ([`crate::questions`]), and how many of the notices waiting to be
    /// taken they have heard.
    pub(crate) questions: crate::questions::Questions,
    /// The player's fellowship, allegiance, friends and the conflict the place allows
    /// ([`crate::social_view`]), kept from the same notices.
    pub(crate) social: crate::social_view::SocialRecords,
    pub(crate) notices_asked: usize,
    /// The player's pets and its mount ([`crate::pets`]), from the same notices.
    pub(crate) pets: crate::pets::Pets,
    /// The world's size in land cells, wide and long, from its region's record, once read.
    pub(crate) land: Option<(i32, i32)>,
}

impl Views {
    /// The player acted: what an action may change is read again at the next refresh, and what
    /// it asks for is answered there.
    pub(crate) fn acted(&mut self, action: &Action) {
        self.acted = true;
        self.questions.acted(action);
        match action {
            Action::AskTargets => self.ask_targets = true,
            Action::AskSavedPlaces => self.ask_places = true,
            Action::LogOff => self.pets.clear(),
            _ => {}
        }
    }
}

/// Whether an object stands in the world where it can be drawn.
fn placed(o: &Object) -> bool {
    o.in_world && o.place.is_some() && o.parent.is_none() && o.container.is_none()
}

/// Whether the panels see an object: it stands in the world, or hangs from a body (a wielded
/// weapon), where it is drawn and picked as itself.
pub(crate) fn seen_by_panels(o: &Object) -> bool {
    placed(o) || o.parent.is_some()
}

/// The player's health and vigor and their greatest, where the qualities have them.
/// The player's level, experience and vitae, from its own qualities.
fn progress(o: &Object) -> v::Progress {
    let q = &o.qualities;
    v::Progress {
        level: q
            .ints
            .get(&quality::LEVEL)
            .and_then(|l| u32::try_from(*l).ok()),
        total_experience: q.long_ints.get(&quality::TOTAL_EXPERIENCE).copied(),
        available_experience: q.long_ints.get(&quality::AVAILABLE_EXPERIENCE).copied(),
        vitae: q.floats.get(&quality::VITAE).copied(),
        vitae_experience: q.long_ints.get(&quality::VITAE_EXPERIENCE).copied(),
    }
}

/// The player's focus and its greatest, where the qualities have its focus.
fn focus(o: &Object) -> Option<[u32; 2]> {
    let q = &o.qualities;
    let n = |id: u32| q.ints.get(&id).map(|v| u32::try_from(*v).unwrap_or(0));
    let current = n(quality::FOCUS)?;
    let more = n(quality::MORE_FOCUS).unwrap_or(0);
    Some([current, quality::BASE_FOCUS.saturating_add(more)])
}

fn vitals(o: &Object) -> Option<[u32; 4]> {
    let q = &o.qualities;
    let v = |id: u32| {
        q.ints
            .get(&id)
            .map(|v| u32::try_from(*v).unwrap_or(0))
            .or_else(|| {
                q.floats.get(&id).map(|f| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let n = f.max(0.0) as u32;
                    n
                })
            })
    };
    let [a, b, c, d] = quality::VITALS;
    Some([v(a)?, v(b)?, v(c)?, v(d)?])
}

/// The number an object's description gives for how many there are of it (1 when it gives none).
fn quantity(w: &WeenieDesc) -> u32 {
    if w.mask & WeenieDesc::NUMBER_BITS[0] != 0 {
        u32::try_from(w.numbers[0]).unwrap_or(1)
    } else {
        1
    }
}

/// A table of kept places (its packed form), as the views carry them: none when it does not read.
#[must_use]
pub fn places(bytes: &[u8]) -> Option<Vec<UiLocation>> {
    let t = auberean_script::ui_locations::UiLocations::read(bytes)?;
    Some(
        t.iter()
            .map(|(layout, element, d)| UiLocation {
                layout,
                element,
                corners: [
                    f32::from_bits(d[0]),
                    f32::from_bits(d[1]),
                    f32::from_bits(d[2]),
                    f32::from_bits(d[3]),
                ],
                visible: d[4] != 0,
            })
            .collect(),
    )
}

/// The player's options, as the views carry them.
fn options(o: &auberean_script::options::GameplayOptions, version: u64) -> GameOptions {
    use crate::chat_options as c;
    let windows = [1, 2, 3, 4];
    GameOptions {
        version,
        bits: o.bits,
        radar_mask: o.radar_mask,
        shortcut_height: o.shortcut_height,
        damage_range: f32::from_bits(o.damage_text_range),
        ui_locations: places(&o.ui_locations),
        chat: ChatOptions {
            filters: windows.map(|w| c::filter(o, w)),
            devoted: windows.map(|w| c::devoted_channel(o, w)),
            fonts: (0..TEXT_KINDS)
                .map(|b| {
                    let kind = 1 << b;
                    (
                        kind,
                        (
                            c::font_size(o, kind),
                            c::font_color(o, kind),
                            c::popup(o, kind),
                        ),
                    )
                })
                .collect(),
            opacities: o
                .opacities
                .entries
                .iter()
                .map(|(k, v)| (*k, f32::from_bits(*v)))
                .collect(),
        },
    }
}

/// A new player's options, as the views carry them (the chat page's defaults among them).
#[must_use]
pub fn default_options() -> GameOptions {
    options(&auberean_script::options::GameplayOptions::default(), 0)
}

impl Client {
    /// The game as the last refresh left it.
    #[must_use]
    pub fn view(&self) -> &View {
        &self.views.view
    }

    /// Bring the view up to date (see the module's description); `icons` makes the items' icons
    /// from their looks.
    pub fn refresh_view(&mut self, icons: Icons<'_>) {
        let heard = std::mem::replace(
            &mut self.views.heard,
            self.notices.iter().map(|n| (n.native, n.about)).collect(),
        );
        let acted = std::mem::take(&mut self.views.acted);
        let mut view = std::mem::take(&mut self.views.view);
        view.refreshes += 1;
        view.pregame = self.pregame_view();
        // Written out apart from the rest: it reads the scripts' strings.
        view.pregame.world_name = self.world_name();
        view.link = self.heartbeat();
        view.since_heard = self.session.since_heard(LocalTime(self.local_time()));
        view.local_clock = self
            .scripts
            .as_ref()
            .and_then(|s| s.host.local.as_ref()?.local_time())
            .map(|t| (t.hour, t.minute));
        view.clock = Clock {
            world_time: self.world_time(),
            game_time: self.game_time(),
        };
        view.chat_service_refused = self.friends_refusal().map(|r| match r {
            crate::friends::Refusal::Banned => ChatServiceRefusal::Banned,
            crate::friends::Refusal::Failed => ChatServiceRefusal::Failed,
        });
        view.player_id = self.model.player;
        view.player = self.player_view();
        self.panels_view(&mut view, icons);
        let said = |ids: &[u32]| heard.iter().any(|(n, _)| ids.contains(n));
        let options_said = said(&[
            notice::RELOAD_OPTIONS,
            notice::RELOAD_CHAT_SETTINGS,
            notice::OPTIONS_CHANGED,
        ]);
        if self.model.player_object().is_none() {
            view.options = None;
        } else if options_said || acted || view.options.is_none() {
            let version = view.options.as_ref().map_or(0, |o| o.version) + 1;
            if let Some(o) = self.options() {
                view.options = Some(options(&o, version));
            }
        }
        view.saved_places = if std::mem::take(&mut self.views.ask_places) {
            self.ui_save_locations().as_deref().and_then(places)
        } else {
            None
        };
        view.world = self.surroundings_view(&heard, acted);
        // The game's questions the notices since the last refresh asked, and those they took down;
        // the pets and the mount they told of.
        let player = self.model.player;
        let asked = self.views.notices_asked.min(self.notices.len());
        let mut journal_said = false;
        let in_world = self.model.player_object().is_some();
        for n in &self.notices[asked..] {
            let said = crate::said::read(n);
            self.views.questions.hear(&said, player);
            self.views.social.hear(&said, player);
            self.views.pets.hear(&said);
            match said.said {
                Said::Quest(QuestNotice::Journal { .. }) => journal_said = true,
                Said::Map(m) if in_world => markers_told(&mut view.markers, m, self.views.land),
                _ => {}
            }
        }
        self.views.notices_asked = self.notices.len();
        view.questions = self.questions_view();
        view.social = self.social_view();
        // The journal read from the player's scripts once the player is here, and again whenever
        // the scripts say it changed; the markers go as the player leaves the world.
        if in_world {
            if journal_said || view.journal.is_none() {
                if let Some(list) = self.quests() {
                    let version = view.journal.as_ref().map_or(0, |j| j.version) + 1;
                    view.journal = Some(v::Journal {
                        version,
                        quests: list.iter().map(crate::said::quest).collect(),
                    });
                }
            }
        } else {
            view.journal = None;
            view.markers.clear_all();
        }
        view.pets = self.views.pets.pets().to_vec();
        if let Some(p) = &mut view.player {
            p.mounted = self.views.pets.mounted();
        }
        view.calls = v::ServerCalls {
            run: self.calls.calls,
            completed: self.calls.completed,
        };
        self.views.view = view;
    }

    /// The player's fellowship, allegiance, friends and the conflict the place allows, as kept, and
    /// the player's kingdom from its description.
    fn social_view(&self) -> v::Social {
        let mut social = self.views.social.social().clone();
        social.kingdom = self
            .model
            .player_object()
            .and_then(|o| {
                crate::description::int(&o.weenie, quality::KINGDOM)
                    .or_else(|| o.qualities.ints.get(&quality::KINGDOM).copied())
            })
            .map(i32::cast_unsigned);
        social
    }

    /// The screens before the world's part.
    fn pregame_view(&self) -> Pregame {
        let m = &self.model;
        Pregame {
            characters: m.characters.as_ref().map(crate::events::characters),
            creation: m
                .creation
                .map(|(response, character, count, error)| CreationAnswer {
                    response,
                    character,
                    count,
                    error,
                }),
            refused: m.refused.map(|(reason, count)| Refusal {
                reason,
                count,
                text: m.refused_with.as_ref().map(|t| to_view(&from_wire(t))),
            }),
            login: self.login_progress(),
            state: self.session_state(),
            download: self.data_download().events.clone(),
            world_name: String::new(),
        }
    }

    /// The player's part.
    fn player_view(&mut self) -> Option<Player> {
        let id = self.model.player?;
        let hero = self.bool_quality(id, quality::HERO).unwrap_or(false);
        let o = self.model.object(id)?;
        let cell = o.place.map(|p| p.cell);
        let vitals = vitals(o);
        let focus = focus(o);
        let progress = progress(o);
        let race = crate::description::int(&o.weenie, quality::RACE)
            .or_else(|| o.qualities.ints.get(&quality::RACE).copied());
        let name = match &self.views.named {
            Some((p, name)) if *p == id => name.clone(),
            _ => {
                let name = self.player_name();
                self.views.named = Some((id, name.clone()));
                name
            }
        };
        let body = v::Body {
            cell: self.body.cell,
            origin: [self.body.origin.x, self.body.origin.y, self.body.origin.z],
            heading: self.body.heading,
            moving: self.moving(),
        };
        Some(Player {
            id,
            cell,
            vitals,
            race,
            name,
            world_name: self.world_name(),
            body,
            mounted: self.views.pets.mounted(),
            progress,
            hero,
            focus,
        })
    }

    /// The panels' caches: copied when their versions moved.
    fn panels_view(&self, view: &mut View, icons: Icons<'_>) {
        let inv = self.inventory();
        if view
            .inventory
            .as_ref()
            .is_none_or(|i| i.version != inv.version || i.player != inv.player)
        {
            view.inventory = Some(self.inventory_view(icons));
        }
        let t = self.toolbar();
        if view.toolbar.as_ref().map(|t| t.version) != Some(t.version) {
            view.toolbar = Some(self.toolbar_view(icons));
        }
        let e = self.effects();
        if view.effects.as_ref().map(|e| e.version) != Some(e.version) {
            view.effects = Some(v::Effects {
                version: e.version,
                now: e.now,
                effects: e
                    .nodes
                    .iter()
                    .map(|n| v::Effect {
                        id: n.eid,
                        name: String::from_utf16_lossy(&n.name),
                        icon: n.icon,
                        time_cast: f64::from_bits(n.time_cast),
                        duration: f64::from_bits(n.duration),
                        pulsed: n.pulsed(),
                    })
                    .collect(),
            });
        }
        if view.skills.as_ref().map(|s| s.version) != Some(self.skills.version) {
            view.skills = Some(skills_view(&self.skills));
        }
        let k = self.craft();
        if view.craft.as_ref().map(|c| c.version) != Some(k.version) {
            view.craft = Some(craft_view(k));
        }
        let st = self.store();
        if view.store.as_ref().map(|s| s.version) != Some(st.version) {
            view.store = Some(v::Store {
                version: st.version,
                max_purchase: st.max_purchase.clone(),
                consignment_charge: st.consignment_charge,
            });
        }
        let a = self.acts();
        if a.version == 0 {
            view.acts = None;
        } else if view.acts.as_ref().map(|a| a.version) != Some(a.version) {
            view.acts = Some(v::Acts {
                version: a.version,
                acts: a.acts.iter().map(act_view).collect(),
            });
        }
    }

    /// An object's icon as its look makes it.
    fn icon_of(&self, id: u64, icons: Icons<'_>) -> IconLayers {
        self.model
            .object(id)
            .map(|o| icons(&o.visual))
            .unwrap_or_default()
    }

    /// The inventory, as the views carry it.
    fn inventory_view(&self, icons: Icons<'_>) -> v::Inventory {
        let inv = self.inventory();
        v::Inventory {
            version: inv.version,
            player: inv.player,
            containers: inv
                .containers
                .iter()
                .map(|(id, c)| {
                    let segments = c
                        .segments
                        .iter()
                        .map(|s| v::Contents {
                            items: s.items.clone(),
                            first: s.first,
                            count: s.count,
                        })
                        .collect();
                    (
                        *id,
                        v::Container {
                            size: c.size,
                            segments,
                            slots: c.slots.clone(),
                        },
                    )
                })
                .collect(),
            segments: inv.segments.clone(),
            worn: inv.worn.clone(),
            items: inv
                .items
                .iter()
                .map(|(id, i)| {
                    (
                        *id,
                        v::Item {
                            quantity: i.quantity,
                            icon: i.icon,
                            icon_layers: self.icon_of(*id, icons),
                            stackable: i.stackable,
                            stack_size: i.stack_size,
                            name: i.name.clone(),
                            wear: i.wear,
                            preferred_location: i.preferred_location,
                            had: i.had,
                            worn: i.worn,
                            melts_at_once: i.melts_at_once,
                            offered_name: i.offered_name.clone(),
                            data_id: self.model.object(*id).map_or(0, |o| o.weenie.entity_did),
                        },
                    )
                })
                .collect(),
            money: inv.money,
            coin_stack: inv.coin_stack,
            no_melt_question: inv.no_melt_question,
            selection: inv.selection,
            amount_name: inv.amount_name.clone(),
        }
    }

    /// The shortcut bar, as the views carry it: each item's icon as its look makes it.
    fn toolbar_view(&self, icons: Icons<'_>) -> v::Toolbar {
        let t = self.toolbar();
        v::Toolbar {
            version: t.version,
            shortcuts: t
                .shortcuts
                .iter()
                .map(|s| v::Shortcut {
                    kind: s.kind,
                    item: s.item,
                    data_enum: s.data_enum,
                    data_id: s.data_id,
                    text: String::from_utf16_lossy(&s.text),
                })
                .collect(),
            slots: t
                .slots
                .iter()
                .map(|s| v::ToolbarSlot {
                    answer: s.answer,
                    tooltip: s.tooltip.clone(),
                    icon: s.icon,
                })
                .collect(),
            set: t.set,
            show_numbers: t.show_numbers,
            height: t.height,
            locked: t.locked,
            insta_scroll: t.insta_scroll,
            skills: t
                .skills
                .iter()
                .map(|k| v::ToolbarSkill {
                    skill: k.skill,
                    trained: k.trained,
                    usable: k.usable,
                    waiting: k.waiting,
                })
                .collect(),
            items: t
                .items
                .iter()
                .map(|i| v::ToolbarItem {
                    item: i.item,
                    have: i.have,
                    here: i.here.clone(),
                    icon_layers: if i.here.is_some() {
                        self.icon_of(i.item, icons)
                    } else {
                        Vec::new()
                    },
                })
                .collect(),
        }
    }

    /// The objects around the player, asking the scripts what the panels read of them (see the
    /// module's description); none without the player's object.
    fn surroundings_view(
        &mut self,
        heard: &[(u32, Option<u64>)],
        acted: bool,
    ) -> Option<Surroundings> {
        let player = self
            .model
            .player
            .filter(|p| self.model.object(*p).is_some());
        let Some(player) = player else {
            let views = &mut self.views;
            views.facts.clear();
            views.ranges = None;
            views.world_options = None;
            views.fellow_xp.clear();
            views.ask_targets = false;
            return None;
        };
        for &(native, about) in heard {
            let facts = &mut self.views.facts;
            match native {
                // The scripts say so when an object's name quality changes too: its name is
                // asked for again with its look.
                // D2-02135
                notice::CHANGE_RADAR_LOOK => match about {
                    Some(id) if id != player => {
                        if let Some(f) = facts.get_mut(&id) {
                            f.radar = None;
                            f.name = None;
                        }
                    }
                    _ => facts.values_mut().for_each(|f| {
                        f.radar = None;
                        f.name = None;
                    }),
                },
                // D2-02061
                notice::ITEM_UPDATE => {
                    if let Some(f) = about.and_then(|id| facts.get_mut(&id)) {
                        f.name = None;
                    }
                }
                notice::RELOAD_OPTIONS | notice::OPTIONS_CHANGED => {
                    self.views.world_options = None;
                }
                _ => {}
            }
        }
        if acted {
            self.views.world_options = None;
        }
        // The fellows' shares, asked again after the notices on which the fellowship's rows read
        // them.
        if heard.iter().any(|(n, _)| notice::FELLOWS.contains(n)) {
            self.views.fellow_xp = self.fellow_xp();
        }
        let current = self.selection();
        let last = self.last_selection();
        // D2-00579, D2-01109
        let auto_target = self
            .interface(function::GET_AUTO_TARGET, &[])
            .is_some_and(|(_, v)| v != 0);
        let should_auto_target = self.should_auto_target().is_some_and(|v| v != 0);
        if self.views.ranges.is_none() {
            self.views.ranges = self.radar_ranges();
        }
        if self.views.world_options.is_none() {
            // D2-00566, D2-01144, D2-00568, D2-01141
            let coords = self
                .interface(function::GET_SHOW_RADAR_COORDS, &[])
                .is_none_or(|(_, v)| v != 0);
            let names = self
                .interface(function::GET_SHOW_OBJECT_NAMES, &[])
                .is_some_and(|(_, v)| v != 0);
            self.views.world_options = Some((coords, names));
        }
        let targets = std::mem::take(&mut self.views.ask_targets);
        let ids: Vec<u64> = self
            .model
            .objects()
            .filter(|o| seen_by_panels(o))
            .map(|o| o.id)
            .collect();
        let known: BTreeSet<u64> = ids.iter().copied().collect();
        self.views.facts.retain(|id, _| known.contains(id));
        let mut things = Vec::with_capacity(ids.len());
        for id in ids {
            let mut f = self.views.facts.get(&id).cloned().unwrap_or_default();
            if f.name.is_none() {
                // D2-00521
                let quantity = self.model.object(id).map_or(1, |o| quantity(&o.weenie));
                f.name_ref = self.object_name_info(id, quantity);
                f.name = f.name_ref.as_ref().and_then(|i| self.write_info(i));
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            {
                f.kind = self.int_quality(id, quality::KIND).unwrap_or(0) as u32;
                f.groove = self.int_quality(id, quality::GROOVE).unwrap_or(0) as i8;
            }
            f.owner = self
                .instance_quality(id, quality::OWNER)
                .filter(|o| *o != 0);
            f.container = self
                .instance_quality(id, quality::CONTAINER)
                .filter(|o| *o != 0);
            f.selectable = self.bool_quality(id, quality::SELECTABLE).unwrap_or(false);
            f.dead = self.bool_quality(id, quality::DEAD).unwrap_or(false);
            if f.radar.is_none() {
                // D2-02049
                let look = self.radar_look(id, [0.0; 4], 0);
                f.radar = Some(
                    look.filter(|(_, shows, _, shape)| *shows != 0 && *shape != NO_BLIP)
                        .map(|(_, _, c, s)| (c, s)),
                );
            }
            if targets {
                // D2-00604, D2-01184, D2-00427, D2-01073
                let ask = |c: &mut Self, function: u32, id: u64| {
                    c.interface(function, &[InterfaceArg::Long(id)])
                        .is_some_and(|(_, v)| v != 0)
                };
                f.fellow = ask(self, function::IS_FELLOWSHIP_MEMBER, id);
                f.attackable = ask(self, function::CAN_AUTO_ATTACK_TARGET, id);
                f.owner_fellow = f
                    .owner
                    .is_some_and(|o| ask(self, function::IS_FELLOWSHIP_MEMBER, o));
            }
            let Some(o) = self.model.object(id) else {
                continue;
            };
            things.push(Thing {
                id,
                data_id: o.weenie.entity_did,
                place: o
                    .place
                    .map(|p| (p.cell, [p.origin.x, p.origin.y, p.origin.z])),
                parent: o.parent.map(|(p, _)| p),
                name: f.name.clone(),
                name_ref: f.name_ref.as_ref().map(to_view),
                own_name: to_view(&from_wire(&o.weenie.name)),
                kind: f.kind,
                groove: f.groove,
                owner: f.owner,
                container: f.container,
                selectable: f.selectable,
                dead: f.dead,
                radar: f.radar.flatten(),
                fellow: f.fellow,
                attackable: f.attackable,
                owner_fellow: f.owner_fellow,
            });
            self.views.facts.insert(id, f);
        }
        let (show_coordinates, show_names) = self.views.world_options.unwrap_or((true, false));
        // The place line's: inside an interior cell, and outdoors the map coordinates.
        // D2-02045, D2-13199
        let cell = if self.body.placed() {
            Some(self.body.cell)
        } else {
            self.model
                .object(player)
                .and_then(|o| o.place)
                .map(|p| p.cell)
        };
        let inside = cell.is_some_and(|c| c & 0xFFFF >= 0x100);
        let map_coordinates = cell
            .zip(self.views.land)
            .and_then(|(c, land)| auberean_primitives::land::map_coords(c, land));
        Some(Surroundings {
            player,
            things,
            selection: v::Selection {
                current,
                last,
                auto_target,
                should_auto_target,
            },
            radar_ranges: self.views.ranges,
            inside,
            map_coordinates,
            show_coordinates,
            show_names,
            portal_space: self.in_portal_space(),
            last_attacker: self.last_attacker,
            fellow_xp: self.views.fellow_xp.clone(),
        })
    }

    /// A scene of the story's acts played again: its sequence told to the panels, as the
    /// scripts tell a sequence to play (when the act holds the scene and it may be played).
    pub(crate) fn play_story_scene(&mut self, act: u32, scene: u32) {
        let found = self
            .acts()
            .acts
            .iter()
            .filter(|a| a.id == act)
            .flat_map(|a| a.scenes.iter())
            .find(|s| s.id == scene && s.playable != 0)
            .cloned();
        if let Some(s) = found {
            self.notices.push(crate::host::PanelNotice {
                native: notice::PLAY_SEQUENCE,
                about: self.model.player,
                objects: vec![Some(auberean_script::native_object::NativeObject::Scenes(
                    vec![s],
                ))],
                requests: vec![None],
                effect_lines: vec![Vec::new()],
                listed: vec![Vec::new()],
                selections: vec![None],
                ..crate::host::PanelNotice::default()
            });
        }
    }
}

/// The skill window's part.
fn skills_view(s: &crate::skills::Skills) -> v::Skills {
    let text = |t: &auberean_script::native_object::TextInfo| to_view(&from_script(t));
    let (panel, nodes) = s.panel.as_ref().map_or((0, Vec::new()), |p| {
        let nodes = p
            .nodes
            .iter()
            .map(|n| v::SkillNode {
                skill: n.skill,
                kind: n.kind,
                name: text(&n.name),
                description: text(&n.description),
                icon: n.icon,
                x: n.x,
                y: n.y,
                parents: n.parents.clone(),
                dependencies: n.dependencies.clone(),
                passive: n.passive,
                untrainable: n.untrainable,
                perk: n.perk,
            })
            .collect();
        (p.id, nodes)
    });
    v::Skills {
        version: s.version,
        panel,
        nodes,
        panels: s
            .panels
            .iter()
            .map(|(k, (name, icon))| (*k, (text(name), *icon)))
            .collect(),
        states: s
            .states
            .iter()
            .map(|(k, s)| {
                let state = v::SkillState {
                    trained: s.trained,
                    being_untrained: s.being_untrained,
                    hero: s.hero,
                    can_train: s.can_train,
                    can_untrain: s.can_untrain,
                    can_raise: s.can_raise,
                    cost_to_train: s.cost_to_train,
                    experience_to_raise: s.experience_to_raise,
                    experience: s.experience,
                    base_level: s.base_level,
                    level: s.level,
                };
                (*k, state)
            })
            .collect(),
        roots: s.roots,
        experience: s.experience,
        credits: s.credits,
        hero_credits: s.hero_credits,
    }
}

/// The crafting page's part.
fn craft_view(k: &crate::craft::Craft) -> v::Craft {
    use crate::craft as rc;
    let text = |t: &auberean_script::native_object::TextInfo| to_view(&from_script(t));
    let skill = |s: &rc::CraftSkill| v::CraftSkill {
        did: s.did,
        name: text(&s.name),
        level: s.level,
        raw_level: s.raw_level,
        cost_to_raise: s.cost_to_raise,
        icon: s.icon,
        usage_xp_mod: s.usage_xp_mod,
    };
    v::Craft {
        version: k.version,
        skills: k
            .skills
            .iter()
            .map(|s| v::CraftSkillFacts {
                listed: skill(&s.listed),
                own: s.own.as_ref().map(skill),
                raw_level: s.raw_level,
                level: s.level,
                raiseable: s.raiseable,
                can_raise: s.can_raise,
                recipes: s
                    .recipes
                    .iter()
                    .map(|r| v::Recipe {
                        did: r.did,
                        name: text(&r.name),
                        icon: r.icon,
                        shown: r.shown,
                        craft_skill: r.craft_skill,
                        category: r.category,
                        time_use_again: r.time_use_again,
                    })
                    .collect(),
            })
            .collect(),
        experience: k.experience,
        categories: k.categories.iter().map(|(c, n)| (*c, text(n))).collect(),
        opened: k.opened.as_ref().map(recipe_at_view),
    }
}

/// A recipe at a spinner value, as the views carry it.
#[must_use]
pub fn recipe_at_view(a: &crate::craft::RecipeAt) -> v::RecipeAt {
    let text = |t: &auberean_script::native_object::TextInfo| to_view(&from_script(t));
    let r = &a.recipe;
    v::RecipeAt {
        recipe: r.did,
        spinner: a.spinner,
        name: text(&r.name),
        charges: r.charges,
        min_spinner: r.min_spinner,
        max_spinner: r.max_spinner,
        last_spinner: r.last_spinner,
        auto_populating: r.auto_populating,
        max_ingredients: r.max_ingredients,
        name_coloring: r.name_coloring,
        name_color: r.name_color,
        difficulty: r.difficulty,
        dynamic_difficulty: r.dynamic_difficulty,
        cutoff: r.difficulty_minus_cutoff,
        cost: r.cost,
        dynamic_cost: r.dynamic_cost,
        craft_skill: r.craft_skill,
        read_only_spinner: r.read_only_spinner,
        skill_level: a.skill_level,
        mask: a.mask,
        ingredients: a
            .ingredients
            .iter()
            .map(|i| v::Ingredient {
                id: i.id,
                description: text(&i.description),
            })
            .collect(),
        spinner_cost: a.cost,
        spinner_difficulty: a.difficulty,
        auto_populate: a.auto_populate.clone(),
        items: a.items.clone(),
        fits: a.fits.clone(),
    }
}

/// An act of the story, as the views carry it.
// D2-12307, D2-12316
fn act_view(a: &auberean_script::story::ActInfo) -> v::Act {
    let text = |t: &auberean_script::native_object::TextInfo| to_view(&from_script(t));
    v::Act {
        id: a.id,
        name: text(&a.name),
        visible: a.visible != 0,
        scenes: a
            .scenes
            .iter()
            .map(|s| v::Scene {
                id: s.id,
                name: text(&s.name),
                hidden: s.hidden != 0,
                playable: s.playable != 0,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the views' copies of the player's options and of a table of kept places)
    use super::*;

    #[test]
    fn a_kept_table_reads_back_as_the_places_kept_and_a_broken_one_as_none() {
        let kept = [
            UiLocation {
                layout: 0x2100_0004,
                element: 0x1234,
                corners: [-0.5, 0.25, 0.0, -0.75],
                visible: true,
            },
            UiLocation {
                layout: 0x2100_0004,
                element: 0x5678,
                corners: [0.1, 0.2, 0.3, 0.4],
                visible: false,
            },
        ];
        let bytes = crate::interface::ui_locations(&kept).write();
        let mut read = places(&bytes).expect("it reads");
        read.sort_by_key(|p| p.element);
        assert_eq!(read, kept);
        assert_eq!(places(&bytes[..3]), None);
    }

    #[test]
    fn a_new_players_options_give_every_kind_of_text_its_font() {
        let o = default_options();
        let fresh = auberean_script::options::GameplayOptions::default();
        assert_eq!(o.chat.fonts.len(), 25);
        for (kind, (size, color, popup)) in &o.chat.fonts {
            assert_eq!(
                *size,
                crate::chat_options::font_size(&fresh, *kind),
                "{kind:#x}"
            );
            assert_eq!(
                *color,
                crate::chat_options::font_color(&fresh, *kind),
                "{kind:#x}"
            );
            assert_eq!(
                *popup,
                crate::chat_options::popup(&fresh, *kind),
                "{kind:#x}"
            );
        }
        assert_eq!(o.chat.filters[0], crate::chat_options::filter(&fresh, 1));
        assert!(o.damage_range < 0.0, "others' damage shows everywhere");
    }
}
