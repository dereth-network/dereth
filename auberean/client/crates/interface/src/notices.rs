//! The scripts' notices as the panels take them: each notice the game reads ([`Notice`]) written
//! back as the words and records of the panel notice its panels hear ([`gameplay::Notice`]): its
//! native, an object's id as its high word and its low, a flag as 1 or 0, a record (a profile, a
//! fellowship, a shop, a string reference) at the word that names it.

use auberean_client_sdk::contract::notice::{
    self as n, AllegianceNotice, BookNotice, ChatNotice, CombatNotice, CraftNotice, DialogNotice,
    ExaminationNotice, FellowshipNotice, FriendsNotice, InterfaceNotice, InventoryNotice,
    MapNotice, Notice, PetNotice, PlayerNotice, QuestNotice, Said, SelectionNotice,
    ShortcutsNotice, SkillsNotice, StoreNotice, TradeNotice, WorldNotice,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::runtime::text::from_view;
use auberean_ui_game::examination::{ExamRequest, Line, Profile};
use auberean_ui_game::gameplay::{self, Record};
use auberean_ui_game::quests::Quest;
use auberean_ui_game::social::{Fellow, Fellowship, Member, Profile as AllegianceProfile};
use auberean_ui_game::{
    book, chat, craft, ingredients, inventory, map, names, options, pet, selection, shop,
    shortcuts, social, statusbar, trade,
};

/// The notices the panels hear that have no name among them: a craft skill changed, portal
/// space, the chat service's connection lost and its ticket taken again.
mod native {
    pub const CRAFT_SKILL: u32 = 0x0088 << 16 | 0x0006;
    pub const PORTAL_SPACE: u32 = 0x0091 << 16 | 0x0001;
    pub const CHAT_SERVICE_LOST: u32 = 0x46A;
    pub const CHAT_SERVICE_TICKET: u32 = 0x46C;
}

/// A panel notice being written: its words, and the record each names (none for a plain value).
struct Out {
    n: gameplay::Notice,
}

impl Out {
    fn new(native: u32, about: Option<u64>) -> Self {
        Self {
            n: gameplay::Notice {
                native,
                about,
                ..gameplay::Notice::default()
            },
        }
    }

    fn word(mut self, w: u32) -> Self {
        self.n.words.push(w);
        self.n.records.push(None);
        self
    }

    /// A 64-bit value: its high word, then its low.
    #[allow(clippy::cast_possible_truncation)]
    fn long(self, v: u64) -> Self {
        self.word((v >> 32) as u32).word(v as u32)
    }

    fn yes(self, b: bool) -> Self {
        self.word(u32::from(b))
    }

    fn float(self, f: f32) -> Self {
        self.word(f.to_bits())
    }

    /// A word naming a record.
    fn record(mut self, r: Option<Record>) -> Self {
        self.n.words.push(0);
        self.n.records.push(r);
        self
    }

    fn text(self, t: Option<&TextRef>) -> Self {
        self.record(t.map(|t| Record::Text(from_view(t))))
    }

    /// A record the client gives with the notice, after its words (the options read again, the
    /// whole list of quests).
    fn given(mut self, r: Option<Record>) -> Self {
        self.n.records.push(r);
        self
    }

    fn done(self) -> gameplay::Notice {
        self.n
    }
}

/// A notice as its panels hear it.
#[allow(clippy::too_many_lines)]
pub fn for_panels(notice: &Notice) -> gameplay::Notice {
    use auberean_ui_game::gameplay::notice as play;
    let about = notice.about;
    let o = |native| Out::new(native, about);
    match &notice.said {
        Said::Player(p) => match p {
            PlayerNotice::Vital {
                vital,
                current,
                max,
            } => o(statusbar::notice::UPDATE_VITAL)
                .word(*vital)
                .long(*current)
                .long(*max),
            PlayerNotice::Hero {
                hero,
                focus,
                max_focus,
            } => o(statusbar::notice::UPDATE_HERO_STATUS)
                .yes(*hero)
                .long(*focus)
                .long(*max_focus),
            PlayerNotice::Vitae {
                kept,
                regained,
                experience,
            } => o(statusbar::notice::UPDATE_VITAE)
                .float(*kept)
                .float(*regained)
                .float(*experience),
            PlayerNotice::Conflict { kind } => {
                o(statusbar::notice::UPDATE_FACTION_STATUS).word(*kind)
            }
            PlayerNotice::Death { dead } => o(play::UPDATE_DEATH_STATE).yes(*dead),
            PlayerNotice::Mounted { mounted } => o(play::UPDATE_MOUNT_STATE).yes(*mounted),
            PlayerNotice::EffectsChanged => o(statusbar::notice::UPDATE_EFFECT),
            PlayerNotice::PulseMissed { effect } => {
                o(statusbar::notice::PULSE_MISSED).word(*effect)
            }
            PlayerNotice::OptionsRead { options } => {
                o(play::RELOAD_OPTIONS).given(options.map(Record::OptionBits))
            }
            PlayerNotice::OptionsChanged { options } => {
                o(options::notice::OPTIONS_CHANGED).given(options.map(Record::OptionBits))
            }
            _ => unread(notice),
        },
        Said::Selection(s) => match s {
            SelectionNotice::Changed { selected } => {
                o(names::notice::SELECTION_CHANGED).long(*selected)
            }
            SelectionNotice::Selectable { selectable } => {
                o(selection::notice::UPDATE_SELECTABILITY).yes(*selectable)
            }
            // The selection's panel reads its kind, then its six numbers.
            SelectionNotice::Vitals(v) => {
                let out = o(names::notice::UPDATE_SELECTION_INFO);
                match v {
                    Some(v) => out
                        .word(v.kind)
                        .word(v.health)
                        .word(v.health_taken_by_players)
                        .word(v.max_health)
                        .word(v.vigor)
                        .word(v.vigor_taken_by_players)
                        .word(v.max_vigor),
                    None => out,
                }
            }
            SelectionNotice::Targeting { waiting } => {
                o(selection::notice::SET_TARGETING_POINTER).yes(*waiting)
            }
            _ => unread(notice),
        },
        Said::Examination(e) => match e {
            ExaminationNotice::Profile(p) => o(play::UPDATE_EXAMINATION_PROFILE)
                .record(p.as_ref().map(|p| Record::Profile(profile(p)))),
            ExaminationNotice::RequestChanged(r) => o(play::EXAMINATION_REQUEST_CHANGED)
                .record(r.as_ref().map(|r| Record::Request(request(r)))),
            ExaminationNotice::Open => o(play::OPEN_EXAMINATION_PANEL),
            _ => unread(notice),
        },
        Said::Chat(c) => match c {
            ChatNotice::StartTell { to } => o(chat::notice::DO_ASSISTED_TELL).text(to.as_ref()),
            ChatNotice::SettingsRead => o(chat::notice::RELOAD_CHAT_SETTINGS),
            _ => unread(notice),
        },
        Said::Dialog(d) => match d {
            DialogNotice::Message { text, at_once } => {
                o(play::DISPLAY_MESSAGE).text(text.as_ref()).yes(*at_once)
            }
            DialogNotice::ResurrectionOffered { by } => {
                o(play::RESURRECTION_REQUESTED_BY_OTHER).text(by.as_ref())
            }
            DialogNotice::EffectQuestion {
                target,
                source,
                effect,
                question,
            } => o(play::SHOULD_APPLY_TOTEM)
                .long(*target)
                .long(*source)
                .word(*effect)
                .text(question.as_ref()),
            _ => unread(notice),
        },
        Said::World(w) => match w {
            WorldNotice::LookChanged => o(names::notice::CHANGE_RADAR_LOOK),
            WorldNotice::ItemChanged => o(names::notice::ITEM_UPDATE),
            WorldNotice::Damage(d) => o(names::notice::DO_3D_DAMAGE).record(d.as_ref().map(|d| {
                Record::DamageText(auberean_ui_game::names::DamageBlob {
                    text: d.text.clone(),
                    color: d.color,
                    duration: d.seconds,
                    scale: d.scale,
                })
            })),
            _ => unread(notice),
        },
        Said::Combat(c) => match c {
            CombatNotice::Attacking { attacking } => o(play::UPDATE_ATTACK_STATE).yes(*attacking),
            CombatNotice::SpecialAsked { skill, special } => {
                o(shortcuts::notice::REQUEST_SPECIAL_ATTACK)
                    .word(*skill)
                    .word(*special)
            }
            CombatNotice::SpecialStarted { special } => {
                o(shortcuts::notice::START_SPECIAL_ATTACK).word(*special)
            }
            CombatNotice::SpecialEnded { special } => {
                o(shortcuts::notice::END_SPECIAL_ATTACK).word(*special)
            }
            CombatNotice::SpecialToggled { skill, on } => {
                o(shortcuts::notice::TOGGLE_SPECIAL_ATTACK)
                    .word(*skill)
                    .yes(*on)
            }
            _ => unread(notice),
        },
        Said::Skills(s) => match s {
            SkillsNotice::Changed { skill } => o(play::UPDATE_SKILL).word(*skill),
            SkillsNotice::Points {
                experience,
                credits,
                hero_credits,
                whole,
            } => o(if *whole {
                play::UPDATE_SKILL_PANEL_FULL
            } else {
                play::UPDATE_SKILL_PANEL
            })
            .long(*experience)
            .word(*credits)
            .word(*hero_credits),
            SkillsNotice::ResetTimes => o(shortcuts::notice::UPDATE_RESET_TIME),
            _ => unread(notice),
        },
        Said::Shortcuts(s) => match s {
            ShortcutsNotice::Reloaded => o(shortcuts::notice::RELOAD_SHORTCUTS),
            ShortcutsNotice::RecipeGone { recipe } => {
                o(shortcuts::notice::REMOVE_RECIPE).word(*recipe)
            }
            _ => unread(notice),
        },
        Said::Inventory(i) => match i {
            InventoryNotice::Worn {
                change,
                item,
                location,
            } => o(inventory::notice::UPDATE_INVENTORY)
                .word(*change)
                .long(*item)
                .word(*location),
            InventoryNotice::Contents {
                container,
                change,
                item,
                slot,
            } => o(inventory::notice::UPDATE_CONTAINER)
                .long(*container)
                .word(*change)
                .long(*item)
                .word(*slot),
            InventoryNotice::ContainerState { container, state } => {
                o(inventory::notice::CONTAINER_STATE_UPDATE)
                    .long(*container)
                    .word(*state)
            }
            InventoryNotice::PackChanged => o(inventory::notice::UPDATE_BACKPACK),
            InventoryNotice::Money { amount } => o(inventory::notice::UPDATE_MONEY).word(*amount),
            _ => unread(notice),
        },
        Said::Fellowship(f) => {
            use social::notice as s;
            match f {
                FellowshipNotice::Changed(f) => o(s::UPDATE_FELLOWSHIP).record(
                    f.as_ref()
                        .map(|f| Record::Fellowship(Box::new(fellowship(f)))),
                ),
                FellowshipNotice::Cleared => o(s::CLEAR_FELLOWSHIP),
                FellowshipNotice::Fellow { fellow, record } => {
                    o(s::UPDATE_FELLOW).long(*fellow).record(
                        record
                            .as_ref()
                            .map(|f| Record::Fellow(Box::new(fellow_of(f)))),
                    )
                }
                FellowshipNotice::FellowGone { fellow } => o(s::CLEAR_FELLOW).long(*fellow),
                FellowshipNotice::Level { fellow, level } => {
                    o(s::UPDATE_FELLOW_LEVEL).long(*fellow).word(*level)
                }
                FellowshipNotice::Health {
                    fellow,
                    health,
                    taken_by_players,
                } => o(s::UPDATE_FELLOW_HEALTH)
                    .long(*fellow)
                    .word(*health)
                    .word(*taken_by_players),
                FellowshipNotice::MaxHealth { fellow, max } => {
                    o(s::UPDATE_FELLOW_MAX_HEALTH).long(*fellow).word(*max)
                }
                FellowshipNotice::Vigor {
                    fellow,
                    vigor,
                    taken_by_players,
                } => o(s::UPDATE_FELLOW_VIGOR)
                    .long(*fellow)
                    .word(*vigor)
                    .word(*taken_by_players),
                FellowshipNotice::MaxVigor { fellow, max } => {
                    o(s::UPDATE_FELLOW_MAX_VIGOR).long(*fellow).word(*max)
                }
                FellowshipNotice::Leader { leader } => o(s::UPDATE_LEADER).long(*leader),
                FellowshipNotice::Cell { fellow, cell } => {
                    o(s::UPDATE_FELLOW_CELL).long(*fellow).word(*cell)
                }
                FellowshipNotice::Offered {
                    leader,
                    leader_name,
                    fellowship,
                } => o(s::REQUEST_RECRUITMENT)
                    .long(*leader)
                    .text(leader_name.as_ref())
                    .text(fellowship.as_ref()),
                FellowshipNotice::OfferExpired { leader } => o(s::EXPIRE_RECRUITMENT).long(*leader),
                _ => unread(notice),
            }
        }
        Said::Allegiance(a) => {
            use social::notice as s;
            match a {
                AllegianceNotice::Changed(a) => o(s::UPDATE_ALLEGIANCE_PROFILE).record(
                    a.as_ref()
                        .map(|a| Record::Allegiance(Box::new(allegiance(a)))),
                ),
                AllegianceNotice::OathOffered { vassal, name } => {
                    o(s::REQUEST_VASSAL).long(*vassal).text(name.as_ref())
                }
                AllegianceNotice::OathAnswered { patron, answer } => {
                    o(s::PATRON_SWEAR_DONE).long(*patron).word(*answer)
                }
                _ => unread(notice),
            }
        }
        Said::Friends(f) => {
            use social::notice as s;
            match f {
                FriendsNotice::List(all) => o(s::GET_FRIENDS_LIST)
                    .record(Some(Record::Friends(all.iter().map(friend).collect()))),
                FriendsNotice::Added { name } => o(s::ADD_FRIEND).text(name.as_ref()),
                FriendsNotice::Removed { name } => o(s::REMOVE_FRIEND).text(name.as_ref()),
                FriendsNotice::State(f) => {
                    o(s::FRIEND_STATUS).record(f.as_ref().map(|f| Record::Friend(friend(f))))
                }
                FriendsNotice::OwnState { mode } => o(s::LOCAL_STATUS).word(*mode),
                _ => unread(notice),
            }
        }
        Said::Trade(t) => {
            use trade::notice as s;
            match t {
                TradeNotice::Registered {
                    asker,
                    asked,
                    partner,
                } => o(s::REGISTERED)
                    .long(*asker)
                    .long(*asked)
                    .text(partner.as_ref()),
                TradeNotice::Opened => o(s::OPENED),
                TradeNotice::Closed { by, reason } => o(s::CLOSED).long(*by).word(*reason),
                TradeNotice::Accepted { by } => o(s::ACCEPTED).long(*by),
                TradeNotice::Declined { by } => o(s::DECLINED).long(*by),
                TradeNotice::Emptied => o(s::RESET),
                TradeNotice::AcceptanceCleared => o(s::ACCEPTANCE_CLEARED),
                TradeNotice::Offered { by, item, quantity } => {
                    o(s::OFFERED).long(*by).long(*item).word(*quantity)
                }
                TradeNotice::Withdrawn { by, item } => o(s::REVOKED).long(*by).long(*item),
                _ => unread(notice),
            }
        }
        Said::Pet(p) => {
            use pet::notice as s;
            match p {
                PetNotice::Added {
                    pet,
                    name,
                    icon,
                    mode,
                } => o(s::ADD_PET)
                    .long(*pet)
                    .text(name.as_ref())
                    .word(*icon)
                    .word(*mode),
                PetNotice::Removed { pet } => o(s::REMOVE_PET).long(*pet),
                PetNotice::Named { pet, name } => {
                    o(s::UPDATE_PET_NAME).long(*pet).text(name.as_ref())
                }
                PetNotice::Mode { pet, mode } => o(s::UPDATE_PET_MODE).long(*pet).word(*mode),
                PetNotice::Attacked { pet } => o(s::PET_IS_UNDER_ATTACK).long(*pet),
                _ => unread(notice),
            }
        }
        Said::Map(m) => {
            use map::notice as s;
            match m {
                MapNotice::MarkerAdded {
                    kind,
                    object,
                    cell,
                    picture,
                    label,
                } => o(s::ADD_MARKER)
                    .word(*kind)
                    .long(*object)
                    .word(*cell)
                    .word(*picture)
                    .text(label.as_ref()),
                MapNotice::MarkerRemoved { kind, object } => {
                    o(s::REMOVE_MARKER).word(*kind).long(*object)
                }
                MapNotice::MarkersCleared { kind } => o(s::CLEAR_MARKERS).word(*kind),
                _ => unread(notice),
            }
        }
        Said::Store(s) => {
            use shop::notice as k;
            let shop = |s: &Option<Box<n::Store>>| s.as_deref().map(|s| Record::Store(store(s)));
            match s {
                // A catalog with no shops names none.
                StoreNotice::Catalog { header, shops } => {
                    o(k::VIEW_CATALOG).text(header.as_ref()).record(
                        (!shops.is_empty())
                            .then(|| Record::Stores(shops.iter().map(store).collect())),
                    )
                }
                StoreNotice::CatalogLeft => o(k::LEAVE_CATALOG),
                StoreNotice::Shown(s) => o(k::VIEW_STORE).record(shop(s)),
                StoreNotice::Changed(s) => o(k::UPDATE_STORE).record(shop(s)),
                StoreNotice::Stock { item, stock } => {
                    o(k::UPDATE_SALE).long(*item).word(stock.cast_unsigned())
                }
                StoreNotice::Left => o(k::LEAVE_STORE),
                StoreNotice::Purchase {
                    product,
                    quantity,
                    cost,
                    cost_quantity,
                } => o(k::CONFIRM_PURCHASE)
                    .text(product.as_ref())
                    .word(*quantity)
                    .text(cost.as_ref())
                    .word(*cost_quantity),
                StoreNotice::Consignments(s) => o(k::VIEW_CONSIGNMENT).record(shop(s)),
                StoreNotice::ConsignmentsChanged(s) => o(k::UPDATE_CONSIGNMENT).record(shop(s)),
                StoreNotice::ConsignmentsLeft => o(k::LEAVE_CONSIGNMENT),
                StoreNotice::SaleWaiting { waiting } => o(play::SHOW_SALE_REMINDER).yes(*waiting),
                _ => unread(notice),
            }
        }
        Said::Craft(c) => match c {
            CraftNotice::PanelChanged => o(craft::UPDATE_CRAFT_PANEL),
            CraftNotice::IngredientAdded {
                recipe,
                slot,
                item,
                amount,
                error,
            } => o(ingredients::notice::ADD_INGREDIENT_DONE)
                .word(*recipe)
                .word(*slot)
                .long(*item)
                .word(*amount)
                .word(*error),
            CraftNotice::Made { recipe, error } => o(ingredients::notice::EXECUTE_CRAFT_DONE)
                .word(*recipe)
                .word(*error),
            CraftNotice::Open { recipe } => {
                o(ingredients::notice::OPEN_INGREDIENT_PANEL).word(*recipe)
            }
            CraftNotice::SkillChanged { skill } => o(native::CRAFT_SKILL).word(*skill),
            _ => unread(notice),
        },
        Said::Quest(q) => match q {
            QuestNotice::Journal { change, quest, all } => {
                let out = o(play::UPDATE_QUESTS)
                    .word(*change)
                    .record(quest.as_ref().map(|q| Record::Quest(quest_of(q))));
                match all {
                    Some(all) => {
                        out.given(Some(Record::Quests(all.iter().map(quest_of).collect())))
                    }
                    None => out,
                }
            }
            QuestNotice::Acts { change, act } => {
                o(play::UPDATE_STORY_QUESTS).word(*change).word(*act)
            }
            QuestNotice::Scenes(s) => {
                o(play::PLAY_SEQUENCE).record(s.as_ref().map(|s| Record::Scenes(scenes(s))))
            }
            _ => unread(notice),
        },
        Said::Book(BookNotice::Open {
            controls,
            look,
            title,
            text,
            picture,
        }) => o(book::USE_BOOK)
            .yes(*controls)
            .word(*look)
            .text(title.as_ref())
            .text(text.as_ref())
            .word(*picture),
        Said::Interface(i) => match i {
            InterfaceNotice::ThroughPortal { inside } => o(native::PORTAL_SPACE).yes(*inside),
            InterfaceNotice::PortalScene { playing, scenes: s } => o(play::PORTAL_SCENE)
                .yes(*playing)
                .record(s.as_ref().map(|s| Record::Scenes(scenes(s)))),
            InterfaceNotice::SaveLayout { name } => o(play::SAVE_UI).text(name.as_ref()),
            InterfaceNotice::LoadLayout { name } => o(play::LOAD_UI).text(name.as_ref()),
            InterfaceNotice::ChatServiceLost => o(native::CHAT_SERVICE_LOST),
            InterfaceNotice::ChatServiceTicket { taken } => {
                o(native::CHAT_SERVICE_TICKET).yes(*taken)
            }
            _ => unread(notice),
        },
        _ => unread(notice),
    }
    .done()
}

/// A notice the game does not read, as the scripts sent it.
fn unread(notice: &Notice) -> Out {
    let mut out = Out::new(0, notice.about);
    if let Said::Unread { native, words } = &notice.said {
        out.n.native = *native;
        out.n.words.clone_from(words);
    }
    out
}

fn request(r: &n::ExaminationRequest) -> ExamRequest {
    ExamRequest {
        kind: r.kind,
        object: r.object,
        tabs: r.tabs,
        data_enum: r.data_enum,
        data_id: r.data_id,
    }
}

fn profile(p: &n::ExaminationProfile) -> Profile {
    Profile {
        request: request(&p.request),
        tabs: p.tabs,
        lines: p.lines.iter().map(line).collect(),
    }
}

fn line(l: &n::ExaminationLine) -> Line {
    Line {
        kind: l.kind,
        tabs: l.tabs,
        appearance: l.appearance,
        value: l.value,
        time: l.time,
        text: l.text.as_ref().map(from_view),
        icon: l.icon.clone(),
        effect: l.effect.iter().map(line).collect(),
    }
}

fn fellow_of(f: &n::Fellow) -> Fellow {
    Fellow {
        name: from_view(&f.name),
        level: f.level,
        health: f.health,
        max_health: f.max_health,
        vigor: f.vigor,
        max_vigor: f.max_vigor,
        pk_damage: f.health_taken_by_players,
        pk_vigor_loss: f.vigor_taken_by_players,
        cell: f.cell,
    }
}

fn fellowship(f: &n::Fellowship) -> Fellowship {
    Fellowship {
        fellows: f
            .fellows
            .iter()
            .map(|(id, f)| (*id, fellow_of(f)))
            .collect(),
        name: f.name.clone(),
        leader: f.leader,
        social: f.social,
        looting: f.looting,
    }
}

fn member(m: &(n::Member, bool)) -> (Member, bool) {
    let (m, on) = m;
    (
        Member {
            id: m.id,
            name: from_view(&m.name),
            title: from_view(&m.title),
            xp_passed: m.experience_passed,
            species: m.species,
            level: m.level,
            gender: m.gender,
            officer: m.officer,
            faction: m.faction,
            rank: m.rank,
        },
        *on,
    )
}

fn allegiance(a: &n::Allegiance) -> AllegianceProfile {
    AllegianceProfile {
        monarch: member(&a.monarch),
        patron: member(&a.patron),
        member: member(&a.member),
        vassals: a.vassals.iter().map(member).collect(),
        total: a.total,
        motd: from_view(&a.message),
        name: from_view(&a.name),
        faction: a.faction,
        officers: a.officers.clone(),
    }
}

fn friend(f: &n::FriendState) -> auberean_ui_game::social::UserStatus {
    auberean_ui_game::social::UserStatus {
        name: f.name.clone(),
        location: f.location.clone(),
        mode: f.mode,
    }
}

fn store(s: &n::Store) -> auberean_ui_game::shop::Store {
    auberean_ui_game::shop::Store {
        id: s.id,
        name: from_view(&s.name),
        description: from_view(&s.description),
        sorter: from_view(&s.sorter),
        portrait: s.portrait,
        sales: s
            .sales
            .iter()
            .map(|s| auberean_ui_game::shop::Sale {
                item: s.item,
                item_did: s.item_data,
                icon: s.icon,
                icon_layers: s.icon_layers.clone(),
                description: from_view(&s.description),
                cost_description: from_view(&s.cost_description),
                cost_icon: s.cost_icon,
                cost_quantity: s.cost_quantity,
                stock: s.stock,
                ordinal: s.ordinal,
                state: s.state,
            })
            .collect(),
        position: s.position,
        size: s.size,
    }
}

fn scenes(s: &n::Scenes) -> auberean_ui_game::story::Scenes {
    auberean_ui_game::story::Scenes {
        scenes: s
            .scenes
            .iter()
            .map(|s| auberean_ui_game::story::Scene {
                id: s.id,
                playable: s.playable,
                keyframes: s
                    .keyframes
                    .iter()
                    .map(|k| auberean_ui_game::story::Keyframe {
                        picture: k.picture,
                        music: k.music,
                        voice: k.voice,
                        subtitle: k.subtitle.as_ref().map(from_view),
                        duration: k.seconds,
                    })
                    .collect(),
            })
            .collect(),
        packed: s.packed.clone(),
    }
}

fn quest_of(q: &n::Quest) -> Quest {
    Quest {
        id: q.id,
        name: from_view(&q.name),
        description: from_view(&q.description),
        journal: from_view(&q.journal),
        status: q.status,
        challenge: q.challenge,
        given: q.given,
        maxed_out: q.maxed_out,
        seconds_remaining: q.seconds_remaining,
        cancellable: q.cancellable,
    }
}

#[cfg(test)]
mod tests {
    use auberean_client_sdk::runtime::host::PanelNotice;
    use auberean_client_sdk::runtime::notices::{Hearers, NOTICES};
    use auberean_client_sdk::runtime::said::{native, read};
    use auberean_client_sdk::script::native_object::NativeObject;
    use auberean_ui_game::gameplay::Record;

    /// A notice of `native` as the scripts send it: about object 7, each of its words a number of
    /// its own (from 1) naming a literal string reference of that number.
    fn sent(native: u32, words: usize) -> PanelNotice {
        let words: Vec<u32> = (1..=u32::try_from(words).expect("a count")).collect();
        let objects = words
            .iter()
            .map(|w| Some(NativeObject::Text(w.to_string().encode_utf16().collect())))
            .collect();
        PanelNotice {
            native,
            about: Some(7),
            objects,
            requests: vec![None; words.len()],
            effect_lines: vec![Vec::new(); words.len()],
            listed: vec![Vec::new(); words.len()],
            selections: vec![None; words.len()],
            words,
        }
    }

    /// Every notice the client binds reaches its panels under its own native, about its object,
    /// with as many words as the scripts sent, each where the scripts put it: a plain word as
    /// it was (a flag as 1), a word naming a string reference as that reference at its place.
    ///
    /// Behaviour: ui.notices.the-panels-hear-each-notice-with-the-words-the-scripts-sent
    #[test]
    fn the_panels_hear_each_notice_with_the_words_the_scripts_sent() {
        for n in NOTICES {
            let words = usize::try_from(n.slots).expect("a count")
                - usize::from(n.hearers == Hearers::TheObjects);
            let sent = sent(n.native, words);
            let heard = super::for_panels(&read(&sent));
            let at = format!("{:#010x}", n.native);
            assert_eq!(heard.native, n.native, "{at}");
            assert_eq!(heard.about, Some(7), "{at}");
            if n.native == native::SELECTION_VITALS {
                // The selection's kind and six numbers, or none: this notice names no selection.
                assert!(heard.words.is_empty(), "{at}");
                continue;
            }
            assert_eq!(heard.words.len(), sent.words.len(), "{at}");
            for (i, (&w, &was)) in heard.words.iter().zip(&sent.words).enumerate() {
                match heard.records.get(i).cloned().flatten() {
                    Some(Record::Text(t)) => {
                        assert_eq!(t.literal, Some(was.to_string().encode_utf16().collect()));
                        assert_eq!(w, 0, "{at}: word {i} names its record");
                    }
                    Some(_) => assert_eq!(w, 0, "{at}: word {i} names its record"),
                    None => assert!(
                        w == was || w == 0 || w == 1,
                        "{at}: word {i} is {w}, sent {was}"
                    ),
                }
            }
        }
    }
}
