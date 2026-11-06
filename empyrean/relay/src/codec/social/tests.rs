//! Behaviour: none (vendors, fellowships and allegiances as the ac1999 server writes them).

use dereth_protocol::{read_body, read_checked, write_blob};

use super::super::write_string;
use super::*;

#[test]
fn a_vendor_keeps_its_prices_and_a_wares_amount_and_description() {
    let mut w = Writer::body();
    w.u32(5);
    w.u32(0xFF);
    w.i32(1);
    w.i32(1000);
    w.i32(1);
    w.f32(0.5);
    w.f32(1.5);
    w.u32(2);
    w.u32(0x00FF_FFFF & (-1i32).cast_unsigned());
    w.u32(70);
    w.u32(3);
    w.u32(71);
    let v = read_body::<Vendor>(&w.into_inner()).unwrap().0;
    assert_eq!((v.profile.buy_price, v.profile.sell_price), (0.5, 1.5));
    assert_eq!(v.items[0].amount, -1, "the 24-bit amount is signed");
    assert_eq!((v.items[1].amount, v.items[1].iid), (3, ObjectId(71)));
    let later = write_blob(&v).unwrap();
    assert_eq!(read_checked::<VendorInfo>(&later).unwrap(), v);
}

fn fellow(w: &mut Writer, cp: u16, level: u16, name: &str) {
    w.u16(cp);
    w.u16(level);
    write_string(w, name, MAX_NAME).unwrap();
}

#[test]
fn a_fellowship_keeps_its_roster_name_and_leader_and_shares_experience() {
    let mut w = Writer::body();
    w.u16(1);
    w.u16(16);
    w.u32(9);
    fellow(&mut w, 100, 7, "Ann");
    write_string(&mut w, "Band", MAX_NEWS).unwrap();
    w.u32(9);
    let f = read_body::<FullUpdate>(&w.into_inner()).unwrap().0;
    assert_eq!(f.0.name, "Band");
    assert_eq!(f.0.leader, ObjectId(9));
    assert_eq!(f.0.share_xp, 1);
    let (id, member) = &f.0.members.entries[0];
    assert_eq!(
        (*id, member.name.as_str(), member.level, member.cp_cache),
        (9, "Ann", 7, 100)
    );
    let later = write_blob(&f).unwrap();
    assert_eq!(read_checked::<FellowshipFullUpdate>(&later).unwrap(), f);
}

#[test]
fn one_fellows_update_names_who() {
    let mut w = Writer::body();
    w.u32(9);
    fellow(&mut w, 1, 2, "Ann");
    let f = read_body::<UpdateFellow>(&w.into_inner()).unwrap().0;
    assert_eq!((f.fellow_id, f.fellow.level), (ObjectId(9), 2));
    assert!(write_blob(&f).is_ok());
}

fn member(w: &mut Writer, id: u32, name: &str) {
    w.u32(id);
    w.u32(10);
    w.u32(20);
    w.u32(0);
    w.u8(1);
    w.u8(2);
    w.u16(3);
    w.u16(4);
    w.u16(5);
    w.f64(60.0);
    write_string(w, name, MAX_NAME).unwrap();
}

#[test]
fn an_allegiance_reads_the_monarch_without_a_patron_and_each_vassal_with_one() {
    let mut w = Writer::body();
    w.u32(2);
    w.u32(1);
    w.u32(2);
    member(&mut w, 1, "King");
    w.u32(1);
    member(&mut w, 2, "Page");
    let a = read_body::<Update>(&w.into_inner()).unwrap().0;
    let h = &a.profile.hierarchy;
    assert_eq!(a.profile.total_members, 2);
    assert_eq!(h.members[0].0, None);
    assert_eq!(h.members[1].0, Some(ObjectId(1)));
    assert_eq!(h.members[1].1.name, "Page");
    assert_eq!((h.members[0].1.rank, h.members[0].1.time_online), (3, 60));
    let later = write_blob(&a).unwrap();
    let back = read_checked::<AllegianceUpdate>(&later).unwrap();
    assert_eq!(back.profile.hierarchy.members[1].1.name, "Page");
    assert_eq!(back.profile.total_vassals, 1);
}
