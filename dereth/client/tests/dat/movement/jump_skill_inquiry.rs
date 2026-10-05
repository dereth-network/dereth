//! The Jump skill is read through the shipped quality filter: the recorded player descriptions give
//! their own Jump values, a true zero stamina stays zero, and a missing stamina or a missing Jump
//! skill row is an inquiry failure rather than a zero skill.
//! Fixture: the retail dats' skill table and quality filter, and three recorded player descriptions.

use dereth_assets::{
    tables::{QualityFilter, SkillTable},
    Decode,
};
use dereth_client_model::qualities::Qualities;
use dereth_primitives::{AssetSource, LocalTime};

/// Behaviour: movement.jump.the-jump-skill-is-read-through-the-shipped-quality-filter
#[test]
fn recorded_jump_quality_inputs_and_real_filter_are_explicit() {
    let store = dereth_dat::testing::open_store().expect("required DATs");
    let skills_id =
        dereth_client_runtime::assets::enum_did(&store, 2, 4).expect("retail SkillTable enum");
    let skills = SkillTable::decode_payload(skills_id, &store.read(skills_id).unwrap()).unwrap();
    let filter_id = dereth_client_runtime::assets::enum_did(&store, 3, 0x1000_0002)
        .expect("quality filter enum");
    let filter = QualityFilter::decode_payload(filter_id, &store.read(filter_id).unwrap()).unwrap();
    assert_eq!(skills_id, dereth_client_runtime::hud::SKILL_TABLE);
    assert_eq!(
        store.ids_of(dereth_dat::DbType::QualityFilter).first(),
        Some(&filter_id),
        "the existing Hud-owned filter must be the exact enum-selected object before reuse"
    );
    assert_eq!(filter_id, dereth_primitives::DataId(0x0e01_0001));
    assert_eq!(filter.attribute_lists[1], vec![1, 3, 5]);
    assert_eq!(
        skills.skills.get(&0x16).unwrap().formula,
        dereth_assets::tables::SkillFormula {
            w: 0,
            x: 1,
            y: 1,
            z: 2,
            attr1: 1,
            attr2: 4
        }
    );
    eprintln!(
        "quality filter={filter_id:?} attribute lists={:?} Jump formula={:?}",
        filter.attribute_lists,
        skills.skills.get(&0x16).unwrap().formula
    );
    for (name, expected) in [
        ("first-login-walk-jump", 75),
        ("early-inventory-and-casting", 35),
        ("long-solo-play", 50),
    ] {
        let corpus = dereth_client_net::client_session::testing::Corpus::load(name)
            .unwrap()
            .expect("recorded description");
        let row = corpus
            .blobs
            .iter()
            .find(|b| b.opcode == 0xf7b0 && b.payload[12..16] == 0x13_u32.to_le_bytes())
            .expect("0013 player description");
        let desc = dereth_protocol::read_body::<dereth_protocol::login::LoginPlayerDescription>(
            &row.payload[16..],
        )
        .unwrap();
        let mut q = Qualities::new();
        q.apply_ac_qualities(&desc.qualities, LocalTime(1.0));
        assert_eq!(
            dereth_rules::skills::inq_jump_skill(&q, &skills, Some(&filter)),
            Some(expected)
        );
        eprintln!(
            "{name} jump={:?} load={} stamina={:?} skillslot={:?} attributes={:?}",
            dereth_rules::skills::inq_skill(&q, &skills, 0x16, false),
            dereth_rules::burden::inq_load(&q),
            dereth_rules::attributes::inq_attribute_2nd_stored(&q, 4),
            q.skill(0x16),
            q.attributes
        );
        q.attributes
            .as_mut()
            .unwrap()
            .stamina
            .as_mut()
            .unwrap()
            .current_level = 0;
        assert_eq!(
            dereth_rules::skills::inq_jump_skill(&q, &skills, Some(&filter)),
            Some(0),
            "real filter excludes currentStamina4; a true zero stays zero"
        );
        let mut missing_jump = skills.clone();
        missing_jump.skills.remove(&0x16);
        assert_eq!(
            dereth_rules::skills::inq_jump_skill(&q, &missing_jump, Some(&filter)),
            None,
            "zero stamina does not bypass a required failed Jump skill inquiry"
        );
        q.attributes.as_mut().unwrap().stamina = None;
        assert_eq!(
            dereth_rules::skills::inq_jump_skill(&q, &skills, Some(&filter)),
            None,
            "missing stamina is inquiry failure, not skill zero"
        );
    }
}
