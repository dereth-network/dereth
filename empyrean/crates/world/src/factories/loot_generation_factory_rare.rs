// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Rare.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Rare.cs`.

use std::sync::LazyLock;

use empyrean_common::dotnet::math::round;
use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;

use crate::factories::loot_generation_factory::world_object_factory_create_new_world_object;
use crate::managers::property_manager;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// `RareChances`: tier -> the approximate "1 in n" chance of a rare of that tier.
/// (ACE: `LootGenerationFactory.RareChances`, an auto-property.)
pub static RARE_CHANCES: [(i32, i32); 6] = [
    (1, 2500),
    (2, 25000),
    (3, 250_000),
    (4, 3_120_000),
    (5, 7_542_500),
    (6, 8_750_000),
];

/// `RareWCIDs`: tier -> the tier's rare wcids, in the order ACE's `HashSet`s enumerate them
/// (insertion order: none is removed).
static RARE_WCIDS: LazyLock<Vec<(i32, &'static [i32])>> = LazyLock::new(|| {
    vec![
        (1, &TIER1_RARES[..]),
        (2, &TIER2_RARES[..]),
        (3, &TIER3_RARES[..]),
        (4, &TIER4_RARES[..]),
        (5, &TIER5_RARES[..]),
        (6, &TIER6_RARES[..]),
    ]
});

/// Builds `RareWCIDs` (run by the static constructor).
// ACE: LootGenerationFactory.InitRares
pub(crate) fn init_rares() {
    LazyLock::force(&RARE_WCIDS);
}

fn rare_wcids(tier: i32) -> &'static [i32] {
    RARE_WCIDS
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, wcids)| *wcids)
        .unwrap_or_else(|| panic!("KeyNotFoundException: rare tier {tier}"))
}

/// Rolls for a rare: `1 in (100 / rare_drop_rate_percent) - luck` for tier 1, then an extra
/// roll per higher tier, each of which can raise the tier; then an even pick of the tier's rares.
// ACE: LootGenerationFactory.TryCreateRare
pub fn try_create_rare(w: &mut World, luck: i32) -> Option<WorldObject> {
    //var t1_chance = 2500; // 1 in 2,500 chance // Old rate. Property default is 0.04 (which is 0.04%, or the same 1/2500)
    let rare_drop_rate_percent: f32 =
        property_manager::get_double(w, "rare_drop_rate_percent", 0.0, true)
            .item
            .cs_cast();
    let mut rare_drop_rate_percent = f64::from(rare_drop_rate_percent);

    // Check to make sure there *IS* a chance. Less than/equal to 0 would mean zero chance, so we can stop here
    if rare_drop_rate_percent <= 0.0 {
        return None;
    }

    rare_drop_rate_percent =
        empyrean_common::dotnet::math::min(rare_drop_rate_percent / 100.0, 1.0);
    let mut t1_chance: i32 = round(1.0 / rare_drop_rate_percent).cs_cast(); // Default PropertyManager value results in a 1 in 2,500 chance
    t1_chance = t1_chance.wrapping_sub(luck).max(1);

    let mut tier = 0;

    if ThreadSafeRandom::next(1, t1_chance) == 1 {
        // 1 in 2,500 chance
        tier = 1;
        if ThreadSafeRandom::next(1, 10) == 1 {
            // 1 in 25,000 chance
            tier = 2;
        }
        if ThreadSafeRandom::next(1, 100) == 1 {
            // 1 in 250,000 chance
            tier = 3;
        }
        if ThreadSafeRandom::next(1, 1250) == 1 {
            // 1 in 3,120,000 chance
            tier = 4;
        }
        if ThreadSafeRandom::next(1, 3017) == 1 {
            // 1 in 7,542,500 (wiki avg. 7,543,103)
            tier = 5;
        }
        if ThreadSafeRandom::next(1, 3500) == 1 {
            // 1 in 8,750,000 chance
            tier = 6;
        }
    }

    if tier == 0 {
        return None;
    }

    let tier_rares = rare_wcids(tier);

    let rng = ThreadSafeRandom::next(0, i32::try_from(tier_rares.len()).expect("small") - 1);

    let rare_wcid = tier_rares[usize::try_from(rng).expect("in range")];

    // DIVERGE: under the era's `LootTables::PackOnly` rule, a rare the world database lacks is not
    // dropped (ACE fails creating it and logs an error).
    if w.era.loot == empyrean_common::era::LootTables::PackOnly
        && !crate::factories::loot_generation_factory::world_has_weenie(
            w,
            rare_wcid.cast_unsigned(),
        )
    {
        return None;
    }

    let wo = world_object_factory_create_new_world_object(w, rare_wcid.cast_unsigned());

    if wo.is_none() {
        log::error!(
            "LootGenerationFactory_Rare.CreateRare(): failed to generate rare wcid {rare_wcid}"
        );
    }

    wo
}

/// Returns the tier for a rare wcid (0 if it is not a rare).
// ACE: LootGenerationFactory.GetRareTier
#[must_use]
pub fn get_rare_tier(rare_wcid: u32) -> i32 {
    let wcid: i32 = rare_wcid.cast_signed();

    for (tier, wcids) in RARE_WCIDS.iter() {
        if wcids.contains(&wcid) {
            return *tier;
        }
    }

    0
}

const TIER1_RARES: [i32; 42] = [
    30183, 30184, 30186, 30187, 30188, 30189, 30194, 30195, 30196, 30197, 30199, 30200, 30202,
    30205, 30206, 30209, 30214, 30215, 30216, 30217, 30218, 30221, 30222, 30224, 30225, 30226,
    30228, 30229, 30232, 30233, 30234, 30240, 30242, 30245, 30246, 41257, 43407, 45360, 45366,
    45367, 45368, 45369,
];

const TIER2_RARES: [i32; 28] = [
    30107, 30108, 30109, 30181, 30182, 30185, 30190, 30191, 30192, 30193, 30201, 30203, 30204,
    30207, 30208, 30210, 30211, 30212, 30213, 30219, 30220, 30227, 30230, 30231, 30235, 30237,
    30239, 30241,
];

const TIER3_RARES: [i32; 5] = [30250, 30251, 30252, 30258, 52034];

const TIER4_RARES: [i32; 47] = [
    30352, 30353, 30354, 30355, 30356, 30357, 30358, 30359, 30360, 30361, 30362, 30363, 30364,
    30365, 30366, 30367, 30368, 30369, 30370, 30371, 30372, 30373, 30510, 30511, 30512, 30513,
    30514, 30515, 30516, 30517, 30518, 30519, 30520, 30521, 30522, 30523, 30524, 30525, 30526,
    30527, 30528, 30529, 30530, 30531, 30532, 30533, 30534,
];

const TIER5_RARES: [i32; 112] = [
    30074, 30075, 30076, 30077, 30078, 30079, 30080, 30081, 30082, 30083, 30084, 30085, 30086,
    30087, 30088, 30089, 30090, 30091, 30092, 30093, 30094, 30095, 30096, 30097, 30098, 30099,
    30100, 30101, 30102, 30103, 30104, 30105, 30106, 30110, 30111, 30112, 30113, 30114, 30115,
    30116, 30117, 30118, 30119, 30120, 30121, 30122, 30123, 30124, 30125, 30126, 30127, 30128,
    30130, 30131, 30132, 30133, 30134, 30135, 30136, 30137, 30138, 30139, 30140, 30141, 30142,
    30143, 30144, 30145, 30146, 30147, 30148, 30149, 30150, 30151, 30152, 30153, 30154, 30155,
    30157, 30158, 30159, 30160, 30161, 30162, 30163, 30164, 30165, 30166, 30167, 30168, 30169,
    30171, 30173, 30174, 30175, 30176, 30179, 30180, 30247, 30248, 30249, 30253, 30254, 30936,
    45361, 45362, 45363, 45364, 45365, 70001, 70002, 70003,
];

const TIER6_RARES: [i32; 61] = [
    30302, 30303, 30304, 30305, 30306, 30307, 30308, 30309, 30345, 30346, 30347, 30348, 30349,
    30350, 30351, 30374, 30375, 30376, 30377, 30378, 42662, 42663, 42664, 42665, 42666, 43848,
    45436, 45437, 45438, 45439, 45440, 45441, 45442, 45443, 45444, 45445, 45446, 45447, 45448,
    45449, 45450, 45451, 45452, 45453, 45454, 45455, 45456, 45457, 45458, 45459, 45460, 45461,
    45462, 45463, 45464, 45465, 45466, 45467, 45468, 45469, 45470,
];
