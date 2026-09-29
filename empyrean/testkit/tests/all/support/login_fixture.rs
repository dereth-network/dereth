//! Character-generation dats and account login for isolated server tests.

use dereth_assets::tables::{
    CharGenTemplate, EyeStrip, GearItem, HairStyle, ObjDesc, SkillFormula,
};
use dereth_primitives::DataId;
use dereth_protocol::login::{CharGenResult, CharacterSendCharGenResult};
use empyrean_dat::fake::sample;
use empyrean_dat::file_types::{SecondaryAttributeTable, TabooTable};
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::{ClientId, ClientStatus, TestServer};
use std::sync::Arc;

pub(crate) fn objdesc(texture: (u32, u32), parts: &[u32]) -> ObjDesc {
    ObjDesc {
        version: 0x11,
        palette: None,
        subpalettes: Vec::new(),
        texture_changes: vec![(0, DataId(texture.0), DataId(texture.1))],
        anim_part_changes: parts.iter().map(|p| (16u8, DataId(*p))).collect(),
    }
}

pub(crate) fn dats(hair_parts: &[u32], taboo_names: bool) -> Arc<empyrean_dat::DatManager> {
    let mut cg = sample::char_gen();
    let heritage = cg.heritage_groups.get_mut(&1).expect("sample heritage");
    heritage.templates = vec![CharGenTemplate {
        name: "Custom".into(),
        icon: 0,
        title: 1,
        attributes: [10; 6],
        normal_skills: Vec::new(),
        primary_skills: Vec::new(),
    }];
    let sex = heritage.sexes.get_mut(&1).expect("sample sex");
    sex.hair_styles = vec![HairStyle {
        icon: 0,
        bald: 0,
        alternate_setup: DataId(0),
        objdesc: objdesc((1, 2), hair_parts),
    }];
    sex.eye_strips = vec![EyeStrip {
        icon: 0,
        icon_bald: 0,
        objdesc: objdesc((3, 4), &[]),
        objdesc_bald: objdesc((5, 6), &[]),
    }];
    sex.nose_strips = vec![(0, objdesc((7, 8), &[]))];
    sex.mouth_strips = vec![(0, objdesc((9, 10), &[]))];
    let none = GearItem {
        name: String::new(),
        clothing_table: DataId(0),
        weenie_default: 99_999,
    };
    sex.headgear = vec![none.clone()];
    sex.shirts = vec![none.clone()];
    sex.pants = vec![none.clone()];
    sex.footwear = vec![none];
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_xp_table(sample::xp_table())
        .with_char_gen(cg)
        .with_skill_table(sample::skill_table())
        .with_portal(
            file_id::SECONDARY_ATTRIBUTE_TABLE,
            SecondaryAttributeTable {
                id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
                health: f(2, 2),
                stamina: f(2, 1),
                mana: f(6, 1),
            },
        )
        .with_portal(
            file_id::TABOO_TABLE,
            TabooTable {
                id: DataId(file_id::TABOO_TABLE),
                audiences: if taboo_names {
                    vec![(1, vec![(1, vec!["*tabooword*".to_owned()])])]
                } else {
                    Vec::new()
                },
            },
        )
        .build()
        .expect("fake dats")
}

pub(crate) fn request(account: &str, name: &str) -> CharacterSendCharGenResult {
    let mut sacs = vec![0; 55];
    sacs[6] = 2; // Melee Defense trained (10 of 50 credits)
    let mut result = CharGenResult {
        version: 1,
        heritage_group: 1,
        gender: 1,
        headgear_style: -1,
        skin_shade: 0.5,
        hair_shade: 0.5,
        strength: 10,
        endurance: 100,
        coordination: 100,
        quickness: 100,
        focus: 10,
        self_: 10,
        class_id: 1,
        skill_advancement_classes: sacs,
        name: name.to_owned(),
        ..CharGenResult::default()
    };
    result.checksum_value = result.checksum();
    CharacterSendCharGenResult {
        account: account.to_owned(),
        result,
    }
}

pub(crate) fn connect_account(
    mut ts: TestServer,
    account: &str,
) -> (TestServer, ClientId, SessionId) {
    let account_id = ts
        .auth()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    assert_eq!(account_id, 1, "the first account gets id 1");
    let id = ts.connect(account, "pw");
    assert_eq!(ts.client(id).status(), ClientStatus::Connected);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(s, _)| s)
        .next()
        .expect("session");
    assert!(ts.run_until(1.0, |ts| ts
        .world
        .sessions
        .get(session)
        .is_some_and(|s| s.state == SessionState::AuthConnected)));
    (ts, id, session)
}
