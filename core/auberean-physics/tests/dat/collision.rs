//! The September 2013 property list and physics materials as a body's collisions read them.
//! Fixture: the target install's portal file.

use std::sync::OnceLock;

use auberean_assets::property::{PhysicsMaterial, PropertyList};
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::{DatFile, Reader};
use auberean_physics::collision::manager::{push, TERRAIN};
use auberean_physics::collision::Collisions;
use auberean_physics::ethereal::{flag_bit, CAMERA, ENVIRONMENT, WATER};
use auberean_physics::pack::{Pack, Writer};

/// The property list and every physics material of the portal file.
fn materials() -> &'static (Context, Vec<(u32, PhysicsMaterial)>) {
    static M: OnceLock<(Context, Vec<(u32, PhysicsMaterial)>)> = OnceLock::new();
    M.get_or_init(|| {
        let dir = require_target_install();
        let (_, path) = install_dats(&dir)
            .expect("list the install")
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)
            .expect("a portal file");
        let portal = DatFile::open(&path).expect("open the portal file");
        let ctx = Context::from_portal(&portal).expect("the property list");
        let mut all = Vec::new();
        for e in portal.entries_in(0x3700_0000, 0x37FF_FFFF) {
            let bytes = portal.read_entry(e).expect("read a physics material");
            let m: PhysicsMaterial =
                auberean_assets::decode(e.id, &bytes, &ctx).expect("decode a physics material");
            all.push((e.id, m));
        }
        (ctx, all)
    })
}

fn list(ctx: &Context) -> &PropertyList {
    ctx.properties().expect("a property list")
}

/// The terrain property is declared an enumeration, the push properties vectors and the
/// local-push property a flag; the environment, water and camera physics types are flags with bits
/// below 64.
///
/// Behaviour: collision.dat.the-collision-properties-have-the-kinds-read
#[test]
fn the_collision_properties_have_the_kinds_read() {
    let (ctx, _) = materials();
    let l = list(ctx);
    let kind = |id: u32| l.descriptor(id).map(|d| d.value_kind);
    assert_eq!(kind(TERRAIN), Some(0x4000_0007));
    for p in push::WALK_ON_AIR_VELOCITY..=push::WATER_ACCELERATION {
        assert_eq!(kind(p), Some(0x4000_0004), "{p:#x}");
    }
    assert_eq!(kind(push::LOCAL), Some(0x4000_0001));
    for t in [ENVIRONMENT, WATER, CAMERA] {
        let bit = flag_bit(l, t).unwrap_or_else(|| panic!("{t:#x} is a flag"));
        assert!(bit < 64, "{t:#x} bit {bit}");
    }
}

/// Every physics material's inquirable properties become the surroundings' contact properties,
/// newly begun, and a body holding them keeps every one through its binary form.
///
/// Behaviour: collision.dat.every-material-s-properties-survive-packing
#[test]
fn every_material_s_properties_survive_packing() {
    let (ctx, all) = materials();
    assert!(all.len() >= 60, "{} materials", all.len());
    let mut held = 0;
    for (id, m) in all {
        let mut c = Collisions::new();
        c.add_environmental_properties(Some(m), list(ctx));
        assert!(c.env_properties.iter().all(|p| p.is_begin()), "{id:#x}");
        held += c.env_properties.len();
        let mut w = Writer::new();
        c.pack(&mut w);
        let mut r = Reader::new(w.as_bytes());
        let back = Collisions::unpack(&mut r, ctx).unwrap_or_else(|e| panic!("{id:#x}: {e}"));
        assert!(r.at_end(), "{id:#x}");
        assert_eq!(back, c, "{id:#x}");
    }
    assert!(held > 0);
}
