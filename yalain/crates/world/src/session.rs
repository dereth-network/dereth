//! The session start: the one call that gives the client, as a character enters the world,
//! everything about the character that is not a quality: its faction standing, its inventory
//! and containers, its effects, its skills, its options, its quests, its scenes and its money.
//!
//! The call names the client's session-start handler and carries fourteen values, each packed
//! as the script machine packs it (last parameter first): plain integers, and object graphs of
//! the client's script objects and native containers. The server builds them from the
//! character's state; the layouts of the script objects are the client library's (their slot
//! counts must match it exactly, or the client drops the call).

use auberean_protocol::script::{Arg, ClientCall, FunctionId, ScriptArgs};
use auberean_protocol::ServerMessage;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject, TextInfo};

use crate::graph::{hash, list, Builder, Slot};

/// The client functions the server calls as a character enters and plays.
pub mod function {
    /// The session start.
    pub const SESSION_START: u32 = 0x8212_0027;
    /// Freeze or free the player's movement (one truth value).
    pub const MOVEMENT_FROZEN: u32 = 0x8212_0021;
    /// Show a text in the chat window (a kind of text, a string reference).
    pub const SHOW_TEXT: u32 = 0x8205_0011;
}

/// The client functions the client calls on the server, that the server answers or keeps.
pub mod request {
    /// The game options: damage text range, shortcut bar height, radar mask, option bits.
    pub const GAME_OPTIONS: u32 = 0x8212_000C;
    /// One shortcut place of the bar.
    pub const SHORTCUT: u32 = 0x8212_0018;
    /// The window places and opacities.
    pub const WINDOW_SETTINGS: u32 = 0x8212_002F;
    /// The chat windows' filters, font sizes and colors, devoted channels and pop-up flags.
    pub const CHAT_OPTIONS: u32 = 0x8212_0032;
}

/// Script packages of the objects the session start carries, by the ids the client's library
/// gives them, with their sizes in slots.
pub mod package {
    pub const SKILL_REPOSITORY: (u16, usize) = (0x233, 8);
    pub const SKILL: (u16, usize) = (0x231, 15);
    pub const EFFECT_REGISTRY: (u16, usize) = (0x14F, 14);
    pub const SCENE_REGISTRY: (u16, usize) = (0x1A4, 2);
    pub const CONTAINER_SEGMENT: (u16, usize) = (0x1AF6, 2);
    /// A worn or wielded item's entry: its look, the places it takes, where it is, its implement
    /// kind, the item.
    pub const INVENTORY_PROFILE: (u16, usize) = (0x1F9, 6);
    /// A look: its scale, its appearance, its visual description.
    pub const LOOK: (u16, usize) = (0x1A2, 3);

    /// Every package the server packs, for checking against a library.
    pub const ALL: [(u16, usize); 7] = [
        SKILL_REPOSITORY,
        SKILL,
        EFFECT_REGISTRY,
        SCENE_REGISTRY,
        CONTAINER_SEGMENT,
        INVENTORY_PROFILE,
        LOOK,
    ];
}

/// Native types of the values the session start carries.
pub mod native {
    /// A hash table of words to words.
    pub const WORD_WORD_HASH: u16 = 1;
    /// A hash table of words to words, several to a key.
    pub const WORD_WORD_MULTI_HASH: u16 = 3;
    /// A hash table of words to references.
    pub const WORD_REF_HASH: u16 = 17;
    /// An appearance (a table of appearance modifiers).
    pub const APPEARANCE: u16 = 15;
    /// Three floats.
    pub const VECTOR: u16 = 112;
    /// A visual description.
    pub const VISUAL_DESC: u16 = 113;
    /// A list of 64-bit values.
    pub const LONG_LIST: u16 = 59;
    /// A hash table of 64-bit values to references.
    pub const LONG_REF_HASH: u16 = 64;
    /// A list of references.
    pub const REF_LIST: u16 = 101;
    /// A list of words.
    pub const WORD_LIST: u16 = 0xA;
    /// A string reference.
    pub const TEXT: u16 = 106;
    /// The options record ([`crate::options`]).
    pub const OPTIONS: u16 = 155;
    /// A list of quest records.
    pub const QUEST_LIST: u16 = 161;
}

/// The size place (in the client's list of bucket counts) of the tables the server builds: 47
/// buckets, as every captured session start's script tables have.
pub const TABLE_SIZE: u8 = 2;

/// What the session start says about a character.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionStart {
    /// Its faction standing on this server and where it stands.
    pub server_faction: u32,
    pub local_faction: u32,
    /// The skills it holds: the skill and its granted level (0 for a skill to raise, 1 for one
    /// held as a gift).
    pub skills: Vec<crate::skills::Held>,
    /// The repository's perk and category tables.
    pub tables: crate::skills::RepositoryTables,
    /// Its options record, packed ([`crate::options::Options::to_packed`]).
    pub options: Vec<u8>,
    /// Its container segments: each segment's room and how much of it is taken.
    pub segments: Vec<(u32, u32)>,
    pub money: u32,
    /// The items in its pack, in slot order.
    pub contents: Vec<u64>,
    /// Its quests under way, packed as the quest list is.
    pub quests: Vec<u8>,
    /// Its unspent skill credits and hero skill credits.
    pub credits: u32,
    pub hero_credits: u32,
    /// Its effects.
    pub effects: Registry,
    /// The story's scenes it holds, by act.
    pub scenes: Vec<(u32, Vec<u32>)>,
    /// What it wears and wields.
    pub worn: Vec<Worn>,
    /// When it last used each skill it has used (game time).
    pub last_used: Vec<(u32, f64)>,
}

/// An item a character wears or wields, as its session start lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Worn {
    pub item: u64,
    /// Where it is (one place), and every place it takes (a two-handed weapon takes both hands).
    pub location: u32,
    pub taken: u32,
    /// What it is wielded as (a weapon's implement kind; 0 for anything else).
    pub implement: u32,
    /// The look it gives its wearer: an appearance and its values (none for a weapon).
    pub look: Option<(u32, auberean_protocol::table::Table<f32>)>,
}

/// The effects a session start carries: the id counter, the records, each class's top record,
/// the records by category, how many top records change each quality and show each appearance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Registry {
    pub counter: u32,
    pub records: Vec<crate::state::EffectState>,
    /// Each class and its top record's id.
    pub top: Vec<(u32, u32)>,
    /// Each record's category and id.
    pub categories: Vec<(u32, u32)>,
    /// Each quality (`type << 16 | quality`) and appearance, with how many top records apply it.
    pub qualities: Vec<(u32, u32)>,
    pub fx: Vec<(u32, u32)>,
}

/// The container segments of a character with nothing: a pack of 42 places and three of 12, as
/// the 2017 servers gave a new character.
pub const NEW_SEGMENTS: [(u32, u32); 4] = [(42, 0), (12, 0), (12, 0), (12, 0)];

const NONE: Slot = Slot::Ref(None);

fn skills_graph(
    skills: &[crate::skills::Held],
    tables: &crate::skills::RepositoryTables,
    (credits, hero_credits): (u32, u32),
    last_used: &[(u32, f64)],
) -> Arg {
    let mut b = Builder::new();
    let repo = b.reserve(package::SKILL_REPOSITORY.0);
    let perks = b.native(
        native::WORD_WORD_HASH,
        crate::skills::word_table(&tables.perks),
    );
    let categories = b.native(
        native::WORD_WORD_HASH,
        crate::skills::word_table(&tables.categories),
    );
    let table = b.next_id();
    let first = table + 1;
    let entries: Vec<(Item, Item)> = skills
        .iter()
        .zip(first..)
        .map(|(h, id)| (Item::Word(h.skill), Item::Ref(id)))
        .collect();
    b.native(
        native::WORD_REF_HASH,
        hash(Elem::Word, Elem::Ref, TABLE_SIZE, &entries),
    );
    for h in skills {
        // A skill used before keeps when it was last used, and says so in its mask.
        let mut h = *h;
        let used = last_used
            .iter()
            .find(|(k, _)| *k == h.skill)
            .map(|(_, t)| *t);
        if used.is_some() {
            h.mask |= crate::skill_use::HAS_LAST_USED;
        }
        let mut slots = crate::skills::info_slots(&h, None);
        if let Some(t) = used {
            slots[0] = Slot::Time(t);
        }
        b.object(package::SKILL.0, &slots);
    }
    b.fill(
        repo,
        &[
            Slot::Int(credits),
            Slot::Long(0),
            Slot::Int(hero_credits),
            Slot::Ref(Some(perks)),
            Slot::Int(0),
            Slot::Ref(Some(categories)),
            Slot::Ref(Some(table)),
        ],
    );
    Arg::Object(b.graph())
}

fn effects_graph(r: &Registry) -> Arg {
    let words = |b: &mut Builder, pairs: &[(u32, u32)]| {
        let entries: Vec<(Item, Item)> = pairs
            .iter()
            .map(|(k, v)| (Item::Word(*k), Item::Word(*v)))
            .collect();
        b.native(
            native::WORD_WORD_HASH,
            hash(Elem::Word, Elem::Word, TABLE_SIZE, &entries),
        )
    };
    let mut b = Builder::new();
    let reg = b.reserve(package::EFFECT_REGISTRY.0);
    let qualities = (!r.qualities.is_empty()).then(|| words(&mut b, &r.qualities));
    let applied = words(&mut b, &r.fx);
    let info = (!r.records.is_empty()).then(|| {
        let at = b.next_id();
        // The table, then each record, each followed by its effect's singleton.
        let entries: Vec<(Item, Item)> = r
            .records
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let id = at + 1 + 2 * u32::try_from(i).unwrap_or(0);
                (Item::Word(e.eid), Item::Ref(id))
            })
            .collect();
        b.native(
            native::WORD_REF_HASH,
            hash(Elem::Word, Elem::Ref, TABLE_SIZE, &entries),
        );
        for e in &r.records {
            let rec = b.reserve(crate::effects::RECORD);
            let state = b.singleton(e.state);
            b.fill(rec, &crate::effects::record_slots(e, state));
        }
        at
    });
    let top = (!r.top.is_empty()).then(|| words(&mut b, &r.top));
    let categories = (!r.categories.is_empty()).then(|| {
        let entries: Vec<(Item, Item)> = r
            .categories
            .iter()
            .map(|(k, v)| (Item::Word(*k), Item::Word(*v)))
            .collect();
        b.native(
            native::WORD_WORD_MULTI_HASH,
            NativeObject::MultiHash(auberean_script::multi_hash::MultiHash::unpacked(
                Elem::Word,
                Elem::Word,
                &entries,
            )),
        )
    });
    let looks = words(&mut b, &[]);
    b.fill(
        reg,
        &[
            Slot::Ref(qualities),
            Slot::Ref(Some(applied)),
            NONE,
            Slot::Int(r.counter.max(1)),
            Slot::Ref(info),
            Slot::Time(-1.0),
            NONE,
            NONE,
            Slot::Int(0x000C_0001),
            NONE,
            Slot::Ref(top),
            Slot::Ref(categories),
            Slot::Ref(Some(looks)),
        ],
    );
    Arg::Object(b.graph())
}

/// The act registry: the viewing protection's effect (none), and each act's scenes held.
/// The places what is worn takes, all together.
#[must_use]
pub fn filled_places(worn: &[Worn]) -> u32 {
    worn.iter().fold(0, |m, w| m | w.taken)
}

/// What is worn, as a table keyed by each item (`by_item`) or by where it is: each entry the
/// item's profile with its look: scale 1, and the appearance it gives its wearer both as a table
/// and as a visual description's whole-object values (empty for a weapon, as the 2017 servers
/// listed every wielded one).
fn worn_graph(worn: &[Worn], by_item: bool) -> Arg {
    let mut b = Builder::new();
    let first = b.next_id() + 1;
    // The table, then each profile followed by its look and the look's three natives.
    let entries: Vec<(Item, Item)> = worn
        .iter()
        .zip((first..).step_by(5))
        .map(|(w, id)| {
            let key = if by_item {
                Item::Long(w.item)
            } else {
                Item::Word(w.location)
            };
            (key, Item::Ref(id))
        })
        .collect();
    let key = if by_item { Elem::Long } else { Elem::Word };
    let ty = if by_item {
        native::LONG_REF_HASH
    } else {
        native::WORD_REF_HASH
    };
    b.native(ty, hash(key, Elem::Ref, 0, &entries));
    for w in worn {
        let profile = b.reserve(package::INVENTORY_PROFILE.0);
        let look = b.reserve(package::LOOK.0);
        let scale = b.native(native::VECTOR, NativeObject::Vector([1.0f32.to_bits(); 3]));
        let (app_bytes, visual_bytes) = look_natives(w.look.as_ref());
        let appearance = b.native(
            native::APPEARANCE,
            NativeObject::Packed {
                ty: native::APPEARANCE.into(),
                bytes: app_bytes,
            },
        );
        let visual = b.native(
            native::VISUAL_DESC,
            NativeObject::Packed {
                ty: native::VISUAL_DESC.into(),
                bytes: visual_bytes,
            },
        );
        b.fill(
            look,
            &[
                Slot::Ref(Some(scale)),
                Slot::Ref(Some(appearance)),
                Slot::Ref(Some(visual)),
            ],
        );
        b.fill(
            profile,
            &[
                Slot::Ref(Some(look)),
                Slot::Int(w.taken),
                Slot::Int(w.location),
                Slot::Int(w.implement),
                Slot::Long(w.item),
            ],
        );
    }
    Arg::Object(b.graph())
}

/// A worn item's look as the profile's two natives: the appearance table, and the visual
/// description carrying it as whole-object values.
fn look_natives(look: Option<&(u32, auberean_protocol::table::Table<f32>)>) -> (Vec<u8>, Vec<u8>) {
    use auberean_protocol::table::Table;
    use auberean_protocol::types::{PartGroupDesc, VisualDesc};
    use auberean_protocol::wire::{Field, Writer};
    let Some((table, values)) = look else {
        return (
            (u32::from(TABLE_SIZE) << 24).to_le_bytes().to_vec(),
            vec![0; 4],
        );
    };
    let appearance = Table {
        size_class: TABLE_SIZE,
        entries: vec![(*table, values.clone())],
    };
    let mut w = Writer::default();
    appearance.write(&mut w);
    let app = w.finish();
    let v = VisualDesc {
        bits: VisualDesc::GLOBAL,
        global: PartGroupDesc {
            bits: PartGroupDesc::KEY | PartGroupDesc::APPEARANCE,
            key: crate::chargen::AVATAR_KEY,
            appearance,
            ..PartGroupDesc::default()
        },
        ..VisualDesc::default()
    };
    let mut w = Writer::default();
    v.write(&mut w);
    (app, w.finish())
}

fn scenes_graph(acts: &[(u32, Vec<u32>)]) -> Arg {
    let mut b = Builder::new();
    let reg = b.reserve(package::SCENE_REGISTRY.0);
    let table = b.next_id();
    let first = table + 1;
    let entries: Vec<(Item, Item)> = acts
        .iter()
        .zip(first..)
        .map(|((act, _), r)| (Item::Word(*act), Item::Ref(r)))
        .collect();
    let scenes = b.native(
        native::WORD_REF_HASH,
        hash(Elem::Word, Elem::Ref, TABLE_SIZE, &entries),
    );
    for (_, held) in acts {
        b.native(
            native::WORD_LIST,
            list(Elem::Word, held.iter().map(|x| Item::Word(*x)).collect()),
        );
    }
    b.fill(reg, &[Slot::Int(0), Slot::Ref(Some(scenes))]);
    Arg::Object(b.graph())
}

/// The container segments as a call value: each segment's room and how much of it is taken.
#[must_use]
pub fn segments_arg(segments: &[(u32, u32)]) -> Arg {
    segments_graph(segments)
}

fn segments_graph(segments: &[(u32, u32)]) -> Arg {
    let mut b = Builder::new();
    let first = b.next_id() + 1;
    let refs = (first..).take(segments.len()).map(Item::Ref).collect();
    b.native(native::REF_LIST, list(Elem::Ref, refs));
    for (room, taken) in segments {
        b.object(
            package::CONTAINER_SEGMENT.0,
            &[Slot::Int(*room), Slot::Int(*taken)],
        );
    }
    Arg::Object(b.graph())
}

fn one_native(ty: u16, n: NativeObject) -> Arg {
    let mut b = Builder::new();
    b.native(ty, n);
    Arg::Object(b.graph())
}

impl SessionStart {
    /// The call's values, in packing order (the handler's last parameter first): money, scenes,
    /// quests, options, skills, effects, the places what is worn takes, what is worn by place
    /// and by item, the container segments, the containers, the contents, the faction standing
    /// here and on the server.
    #[must_use]
    pub fn args(&self) -> Vec<Arg> {
        vec![
            Arg::Int(self.money),
            scenes_graph(&self.scenes),
            one_native(
                native::QUEST_LIST,
                NativeObject::Packed {
                    ty: native::QUEST_LIST.into(),
                    bytes: if self.quests.is_empty() {
                        vec![0; 4]
                    } else {
                        self.quests.clone()
                    },
                },
            ),
            one_native(
                native::OPTIONS,
                NativeObject::Packed {
                    ty: native::OPTIONS.into(),
                    bytes: self.options.clone(),
                },
            ),
            skills_graph(
                &self.skills,
                &self.tables,
                (self.credits, self.hero_credits),
                &self.last_used,
            ),
            effects_graph(&self.effects),
            Arg::Int(filled_places(&self.worn)),
            worn_graph(&self.worn, false),
            worn_graph(&self.worn, true),
            segments_graph(&self.segments),
            one_native(native::LONG_LIST, list(Elem::Long, Vec::new())),
            one_native(
                native::LONG_LIST,
                list(
                    Elem::Long,
                    self.contents.iter().map(|i| Item::Long(*i)).collect(),
                ),
            ),
            Arg::Int(self.local_faction),
            Arg::Int(self.server_faction),
        ]
    }

    /// The call.
    ///
    /// # Errors
    /// A value that cannot be packed (none the server builds).
    pub fn message(&self) -> Result<ServerMessage, String> {
        call(function::SESSION_START, &self.args())
    }
}

/// A call of a client function for the player.
///
/// # Errors
/// A value that cannot be packed.
pub fn call(function: u32, args: &[Arg]) -> Result<ServerMessage, String> {
    Ok(ServerMessage::PlayerCall(ClientCall {
        function: FunctionId(function),
        args: Some(ScriptArgs::from_values(args)?),
    }))
}

/// Freeze or free the player's movement.
#[must_use]
pub fn movement_frozen(frozen: bool) -> ServerMessage {
    call(function::MOVEMENT_FROZEN, &[Arg::Int(u32::from(frozen))]).expect("an integer packs")
}

/// The line the 2017 servers showed in the chat window as a character entered, that the
/// character could not be logged into the authorization server and its friends list is out of
/// reach: a string of the client's tables, as text of kind 1. Sent only when the friends service
/// is off or the account has no stream on it.
#[must_use]
pub fn welcome() -> ServerMessage {
    let text = TextInfo {
        string_id: WELCOME.0,
        table: WELCOME.1,
        ..TextInfo::default()
    };
    call(
        function::SHOW_TEXT,
        &[
            one_native(native::TEXT, NativeObject::TextInfo(text)),
            Arg::Int(1),
        ],
    )
    .expect("a string reference packs")
}

/// The entry line's string: its id and its table.
pub const WELCOME: (u32, u32) = (0x7F91_D315, 0x2500_0884);
