//! Headless character-creation presets seed heritage/era free baseline skills from the retail
//! data tables without training unrelated or retired weapon skills.
//! Fixture: the shipped retail DAT records.
//!
//! Behaviour: none (this client's built-in headless character presets over the retail tables)

use dereth_client_sdk::assets::{
    tables::{CharGen, SkillTable},
    Decode,
};
use dereth_client_sdk::dat::RetailDatStore;
use dereth_client_sdk::primitives::{AssetSource as _, DataId, EraId};
use dereth_client_sdk::rules::chargen::{
    default_creation_skill_classes, skill_credits_used, SkillAdvancementClass,
    RETIRED_WEAPON_SKILLS,
};
use dereth_headless::run::creation_request_for_store;
use dereth_headless::script::Role;

fn decode_sac(raw: i32) -> SkillAdvancementClass {
    match raw {
        1 => SkillAdvancementClass::Untrained,
        2 => SkillAdvancementClass::Trained,
        3 => SkillAdvancementClass::Specialized,
        _ => SkillAdvancementClass::Inactive,
    }
}

#[test]
fn soldier_and_caster_presets_satisfy_retail_free_skills_and_credit_budget() {
    dereth_client_sdk::dat::testing::require_dats();
    let store =
        RetailDatStore::open_dir(&dereth_client_sdk::dat::testing::dat_dir()).expect("open dats");
    let cg_id = DataId(0x0E00_0002);
    let sk_id = DataId(0x0E00_0004);
    let cg = CharGen::decode_payload_in(
        store.era_of(cg_id),
        cg_id,
        &store.read(cg_id).expect("char gen"),
    )
    .expect("decode char gen");
    let sk = SkillTable::decode_payload_in(
        store.era_of(sk_id),
        sk_id,
        &store.read(sk_id).expect("skill table"),
    )
    .expect("decode skill table");

    let expected_free = default_creation_skill_classes(&cg, &sk, 1, EraId::Eor);
    let budget = i32::try_from(cg.heritage_groups[&1].skill_credits).expect("skill credits");

    for (role, required_paid) in [
        (Role::Any, &[44usize, 6, 21][..]),
        (Role::Mage, &[34usize, 31, 33, 16][..]),
    ] {
        let req = creation_request_for_store(&store, Some(EraId::Eor), "HeadlessPreset", role);
        assert_eq!(req.heritage_group, 1);
        assert_eq!(req.skill_advancement_classes.len(), expected_free.len());

        let levels: Vec<SkillAdvancementClass> = req
            .skill_advancement_classes
            .iter()
            .copied()
            .map(decode_sac)
            .collect();

        for (i, (&got, &floor)) in levels.iter().zip(&expected_free).enumerate() {
            if floor >= SkillAdvancementClass::Trained {
                assert!(
                    got >= floor,
                    "role {role:?}: free skill {i} got {got:?}, expected at least {floor:?}"
                );
            }
        }
        for &id in required_paid {
            assert_eq!(
                levels[id],
                SkillAdvancementClass::Trained,
                "role {role:?}: expected skill {id} trained"
            );
        }
        for retired in RETIRED_WEAPON_SKILLS {
            let idx = usize::try_from(retired).expect("small");
            assert_eq!(
                levels[idx],
                SkillAdvancementClass::Inactive,
                "role {role:?}: retired weapon {retired} must stay Inactive"
            );
        }
        assert_eq!(
            levels[23],
            SkillAdvancementClass::Inactive,
            "role {role:?}: unrelated paid skill Lockpick must stay Inactive"
        );
        let used = skill_credits_used(&cg, &sk, req.heritage_group, &levels);
        assert!(
            used > 0 && used <= budget,
            "role {role:?}: used {used} credits against budget {budget}"
        );
    }
}
