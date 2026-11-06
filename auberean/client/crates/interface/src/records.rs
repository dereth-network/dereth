//! The records the scripts' notices name, as the panels read them: an examination profile and
//! request, the player's options, a quest and the quest list, a string reference or a string, a
//! shop and a catalog's shops, the story's scenes, a fellowship and a fellow, an allegiance's profile,
//! a friend's state and the friends list.

use std::collections::BTreeMap;

use auberean_client_sdk::runtime::examination::{kind, Profile, Request};
use auberean_client_sdk::runtime::text::from_script;
use auberean_client_sdk::script::native_object::{native_type, Field, NativeObject, QuestInfo};
use auberean_client_sdk::text::StringInfo;
use auberean_ui_game::examination::{ExamRequest, Line, Profile as Shown};
use auberean_ui_game::gameplay::Record;
use auberean_ui_game::quests::Quest;
use auberean_ui_game::social::{Fellow, Fellowship, Member, Profile as AllegianceProfile};

/// The records a notice's arguments name: an examination request whole when the scripts held one,
/// a profile with its effect lines' own lines, else as [`of`] reads it.
pub fn of_notice(n: &auberean_shell::interface::Notice) -> Vec<Option<Record>> {
    n.objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            if let Some(r) = n.requests.get(i).copied().flatten() {
                return Some(Record::Request(request(&r)));
            }
            let listed = n.listed.get(i).map_or(&[][..], Vec::as_slice);
            // D2-00898
            // The friends list: every entry a friend's state (an empty list too, for its notice).
            let friends: Vec<_> = listed.iter().flatten().filter_map(friend).collect();
            if friends.len() == listed.len()
                && (!friends.is_empty() || n.native == FRIENDS_LIST_NOTICE)
            {
                return Some(Record::Friends(friends));
            }
            if !listed.is_empty() {
                // D2-00925
                let shops: Vec<_> = listed.iter().flatten().filter_map(store).collect();
                if !shops.is_empty() {
                    return Some(Record::Stores(shops));
                }
            }
            let mut record = of(o);
            if let Some(Record::Profile(p)) = record.as_mut() {
                for (k, bytes) in n.effect_lines.get(i).into_iter().flatten() {
                    if let (Some(line), Ok(sub)) = (p.lines.get_mut(*k), Profile::unpack(bytes)) {
                        line.effect = lines(&sub);
                    }
                }
            }
            record
        })
        .collect()
}

/// The record a notice's argument names, when it is one a panel reads.
pub fn of(o: &Option<NativeObject>) -> Option<Record> {
    match o.as_ref()? {
        o @ (NativeObject::Packed { .. } | NativeObject::Profile { .. })
            if Profile::of(o).is_some() =>
        {
            Profile::of(o).map(|p| Record::Profile(profile(&p)))
        }
        NativeObject::Packed { ty, bytes } if *ty == native_type::EXAMINATION_REQUEST => {
            Some(Record::Request(request(&Request::unpack(bytes))))
        }
        NativeObject::Options(o) => Some(Record::OptionBits(o.bits)),
        NativeObject::TextInfo(t) => Some(Record::Text(from_script(t))),
        NativeObject::Text(t) => {
            let mut s = auberean_client_sdk::text::StringInfo::default();
            s.set_literal(t, false);
            Some(Record::Text(s))
        }
        o @ NativeObject::Adaptor { ty, .. } if *ty == SHOP => store(o).map(Record::Store),
        NativeObject::Scenes(v) => Some(Record::Scenes(scenes(v))),
        NativeObject::Packed { ty, bytes } if *ty == native_type::SCENE_INFO_LIST => {
            let mut r = auberean_client_sdk::script::wire::In::new(bytes);
            auberean_client_sdk::script::story::read_scenes(&mut r)
                .ok()
                .map(|v| Record::Scenes(scenes(&v)))
        }
        NativeObject::Quest(q) => Some(Record::Quest(quest(q))),
        NativeObject::DamageText(d) => {
            Some(Record::DamageText(auberean_ui_game::names::DamageBlob {
                text: String::from_utf16_lossy(&d.text),
                color: d.color.map(f32::from_bits),
                duration: f32::from_bits(d.duration),
                scale: f32::from_bits(d.scale),
            }))
        }
        NativeObject::QuestList(l) => Some(Record::Quests(l.iter().map(quest).collect())),
        o if friend(o).is_some() => friend(o).map(Record::Friend),
        NativeObject::Adaptor { ty, fields } => match *ty {
            adaptor::FELLOWSHIP => Some(Record::Fellowship(Box::new(fellowship(fields)))),
            adaptor::FELLOW => Some(Record::Fellow(Box::new(fellow(fields)))),
            adaptor::ALLEGIANCE_PROFILE => Some(Record::Allegiance(Box::new(allegiance(fields)))),
            _ => None,
        },
        _ => None,
    }
}

/// The native type of a shop's adaptor, and of a sale's.
const SHOP: u32 = 0x1AD6;

/// The scripts' notice of the friends list to the friends page.
const FRIENDS_LIST_NOTICE: u32 = 0x1AD2_0001;

/// A friend's state, as the friends page reads it.
fn friend(o: &NativeObject) -> Option<auberean_ui_game::social::UserStatus> {
    let s = auberean_client_sdk::runtime::friends::UserStatus::of(o)?;
    Some(auberean_ui_game::social::UserStatus {
        name: String::from_utf16_lossy(&s.name),
        location: String::from_utf16_lossy(&s.location),
        mode: s.mode,
    })
}

/// A field of an adaptor: a word, a 64-bit value, a string reference.
fn word(f: &std::collections::BTreeMap<String, Field>, k: &str) -> u32 {
    match f.get(k) {
        Some(Field::Word(w)) => *w,
        _ => 0,
    }
}

fn long(f: &std::collections::BTreeMap<String, Field>, k: &str) -> u64 {
    match f.get(k) {
        Some(Field::Long(l)) => *l,
        _ => 0,
    }
}

fn text(
    f: &std::collections::BTreeMap<String, Field>,
    k: &str,
) -> auberean_client_sdk::text::StringInfo {
    match f.get(k) {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::TextInfo(t) => from_script(t),
            _ => auberean_client_sdk::text::StringInfo::default(),
        },
        _ => auberean_client_sdk::text::StringInfo::default(),
    }
}

/// An icon description's layers, each a picture and its color, in order.
fn icon_layers(f: &std::collections::BTreeMap<String, Field>, k: &str) -> Vec<(u32, u32)> {
    let Some(Field::Value(v)) = f.get(k) else {
        return Vec::new();
    };
    let NativeObject::Packed { bytes, .. } = v.as_ref() else {
        return Vec::new();
    };
    let w: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let n = w.first().copied().unwrap_or(0) as usize;
    w.get(1..)
        .unwrap_or(&[])
        .as_chunks::<3>()
        .0
        .iter()
        .take(n)
        .map(|l| (l[1], l[2]))
        .collect()
}

/// A shop's adaptor as the shop windows read it: its sales' adaptors in order.
fn store(o: &NativeObject) -> Option<auberean_ui_game::shop::Store> {
    let NativeObject::Adaptor { ty, fields } = o else {
        return None;
    };
    if *ty != SHOP {
        return None;
    }
    let sales = match fields.get("Sale") {
        Some(Field::Many(v)) => v
            .iter()
            .filter_map(|s| match s {
                Field::Value(a) => match a.as_ref() {
                    NativeObject::Adaptor { fields: f, .. } => Some(auberean_ui_game::shop::Sale {
                        item: long(f, "ItemIID"),
                        item_did: word(f, "ItemDID"),
                        icon: word(f, "ItemIconID"),
                        icon_layers: icon_layers(f, "ItemIconDesc"),
                        description: text(f, "ItemDesc"),
                        cost_description: text(f, "CostDesc"),
                        cost_icon: word(f, "CostIcon"),
                        cost_quantity: f32::from_bits(word(f, "CostQuantity")),
                        stock: word(f, "StockQuantity").cast_signed(),
                        ordinal: word(f, "Ordinal"),
                        state: word(f, "State"),
                    }),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(auberean_ui_game::shop::Store {
        id: word(fields, "ID"),
        name: text(fields, "Name"),
        description: text(fields, "Desc"),
        sorter: text(fields, "Sorter"),
        portrait: word(fields, "Portrait"),
        sales,
        position: word(fields, "Position").cast_signed(),
        size: word(fields, "StoreSize").cast_signed(),
    })
}

/// Scenes as the story plays them, and their packed form to give back.
fn scenes(v: &[auberean_client_sdk::script::story::SceneInfo]) -> auberean_ui_game::story::Scenes {
    use auberean_client_sdk::script::story::track;
    let mut w = auberean_client_sdk::script::wire::Out::new();
    auberean_client_sdk::script::story::write_scenes(&mut w, v);
    auberean_ui_game::story::Scenes {
        scenes: v
            .iter()
            .map(|s| auberean_ui_game::story::Scene {
                id: s.id,
                playable: s.playable == 1,
                keyframes: s
                    .keyframes
                    .iter()
                    .map(|k| auberean_ui_game::story::Keyframe {
                        picture: k.has(track::VIDEO).then_some(k.video),
                        music: k.has(track::AUDIO).then_some(k.audio),
                        voice: k.has(track::VOICE).then_some(k.voice),
                        subtitle: k.has(track::SUBTITLE).then(|| from_script(&k.subtitle)),
                        duration: k.has_duration().then(|| f64::from_bits(k.duration)),
                    })
                    .collect(),
            })
            .collect(),
        packed: w.buf,
    }
}

/// The interface adaptors' native types.
mod adaptor {
    pub const ALLEGIANCE_PROFILE: u32 = 0x83;
    pub const FELLOW: u32 = 0x93;
    pub const FELLOWSHIP: u32 = 0x94;
}

type Fields = BTreeMap<String, Field>;

/// A string reference an adaptor holds (an empty one when never set).
fn text_of(f: Option<&Field>) -> StringInfo {
    match f {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::TextInfo(t) => from_script(t),
            _ => StringInfo::default(),
        },
        _ => StringInfo::default(),
    }
}

/// The fields of an adaptor an adaptor holds (none when never set).
fn inner(f: Option<&Field>) -> Fields {
    match f {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::Adaptor { fields, .. } => fields.clone(),
            _ => Fields::new(),
        },
        _ => Fields::new(),
    }
}

/// A fellow as its adaptor holds it.
fn fellow(f: &Fields) -> Fellow {
    Fellow {
        name: text_of(f.get("Name")),
        level: word(f, "Level"),
        health: word(f, "Health"),
        max_health: word(f, "MaxHealth"),
        vigor: word(f, "Vigor"),
        max_vigor: word(f, "MaxVigor"),
        pk_damage: word(f, "PKDamage"),
        pk_vigor_loss: word(f, "PKVigorloss"),
        cell: word(f, "Cell"),
    }
}

/// A fellowship as its adaptor holds it: its fellows in the order added.
#[allow(clippy::cast_possible_wrap)]
fn fellowship(f: &Fields) -> Fellowship {
    let fellows = match f.get("Fellow") {
        Some(Field::Many(all)) => all
            .iter()
            .filter_map(|e| match e {
                Field::Many(pair) => match pair.as_slice() {
                    [Field::Long(id), a] => Some((*id, fellow(&inner(Some(a))))),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let name = match f.get("Name") {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::Text(units) => String::from_utf16_lossy(units),
            _ => String::new(),
        },
        _ => String::new(),
    };
    Fellowship {
        fellows,
        name,
        leader: long(f, "Leader"),
        social: word(f, "Social") as i32,
        looting: word(f, "LootingMethod") as i32,
    }
}

/// An allegiance member as its adaptor holds one.
fn member(f: &Fields) -> Member {
    Member {
        id: long(f, "ID"),
        name: text_of(f.get("Name")),
        title: text_of(f.get("Title")),
        xp_passed: long(f, "XPPassed"),
        species: word(f, "Species"),
        level: word(f, "Level"),
        gender: word(f, "Gender"),
        officer: word(f, "Officer") != 0,
        faction: word(f, "Faction"),
        rank: word(f, "Rank"),
    }
}

/// A member and whether online, as a profile holds each (none when never set).
fn member_flag(f: Option<&Field>) -> (Member, bool) {
    match f {
        Some(Field::Many(pair)) => match pair.as_slice() {
            [a, Field::Word(on)] => (member(&inner(Some(a))), *on != 0),
            _ => (Member::default(), false),
        },
        _ => (Member::default(), false),
    }
}

/// An allegiance's profile as its adaptor holds it.
fn allegiance(f: &Fields) -> AllegianceProfile {
    let vassals = match f.get("Vassal") {
        Some(Field::Many(all)) => all.iter().map(|v| member_flag(Some(v))).collect(),
        _ => Vec::new(),
    };
    let officers = match f.get("Officer") {
        Some(Field::Many(all)) => all
            .iter()
            .filter_map(|o| match o {
                Field::Long(id) => Some(*id),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    AllegianceProfile {
        monarch: member_flag(f.get("Monarch")),
        patron: member_flag(f.get("Patron")),
        member: member_flag(f.get("Member")),
        vassals,
        total: word(f, "Total"),
        motd: text_of(f.get("MotD")),
        name: text_of(f.get("AllegianceName")),
        faction: word(f, "FactionType"),
        officers,
    }
}
fn request(r: &Request) -> ExamRequest {
    ExamRequest {
        kind: r.kind,
        object: r.object,
        tabs: r.current,
        data_enum: r.data_enum,
        data_id: r.data_id,
    }
}

/// A profile's lines as the examination window lays them out: a string reference written by
/// the window, an icon by its layers' pictures and colors.
fn profile(p: &Profile) -> Shown {
    Shown {
        request: request(&Request::unpack(&p.request)),
        tabs: p.tabs,
        lines: lines(p),
    }
}

fn lines(p: &Profile) -> Vec<Line> {
    p.nodes
        .iter()
        .map(|n| Line {
            kind: n.kind,
            tabs: n.tabs,
            appearance: n.appearance,
            value: n.value,
            time: n.time,
            text: (n.kind == kind::STRING).then(|| from_script(&n.text)),
            icon: n.icon.iter().map(|l| (l[1], l[2])).collect(),
            effect: Vec::new(),
        })
        .collect()
}

fn quest(q: &QuestInfo) -> Quest {
    Quest {
        id: q.id,
        name: from_script(&q.name),
        description: from_script(&q.description),
        journal: from_script(&q.journal),
        status: q.status,
        challenge: q.challenge,
        given: f64::from_bits(q.bestowed),
        maxed_out: q.maxed_out(),
        seconds_remaining: q.seconds_remaining(),
        cancellable: q.cancellable != 0,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how a notice's objects read for the panels, which their own tests hold)
    use super::*;
    use auberean_client_sdk::runtime::friends::UserStatus;

    fn status(name: &str, mode: u32) -> NativeObject {
        UserStatus {
            name: name.encode_utf16().collect(),
            location: Vec::new(),
            mode,
        }
        .native()
    }

    fn notice(native: u32, listed: Vec<Option<NativeObject>>) -> auberean_shell::interface::Notice {
        auberean_shell::interface::Notice {
            native,
            about: None,
            words: Vec::new(),
            objects: vec![None],
            requests: vec![None],
            effect_lines: vec![Vec::new()],
            listed: vec![listed],
        }
    }

    #[test]
    fn a_friends_state_reads_as_a_friend_and_the_friends_list_as_the_friends_even_empty() {
        let page = |name: &str, mode| auberean_ui_game::social::UserStatus {
            name: name.into(),
            location: String::new(),
            mode,
        };
        assert_eq!(
            of(&Some(status("Cara", 3))),
            Some(Record::Friend(page("Cara", 3)))
        );
        let n = notice(
            FRIENDS_LIST_NOTICE,
            vec![Some(status("Cara", 1)), Some(status("Bob@coldeve", 0))],
        );
        assert_eq!(
            of_notice(&n),
            [Some(Record::Friends(vec![
                page("Cara", 1),
                page("Bob@coldeve", 0)
            ]))]
        );
        assert_eq!(
            of_notice(&notice(FRIENDS_LIST_NOTICE, Vec::new())),
            [Some(Record::Friends(Vec::new()))]
        );
        assert_eq!(of_notice(&notice(0x1AD3_0001, Vec::new())), [None]);
    }
}
