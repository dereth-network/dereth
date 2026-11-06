//! Vendors, fellowships and allegiances as the ac1999 server sends them, ported from it,
//! normalized to the later messages. What the 1999 wire does not carry is filled the way a Launch
//! world plays.

use dereth_primitives::ObjectId;
use dereth_protocol::{
    social::{
        AllegianceData, AllegianceHierarchy, AllegianceProfile, AllegianceUpdate, Fellow,
        Fellowship, FellowshipFullUpdate, FellowshipUpdateFellow,
    },
    trade::{ItemProfile, VendorInfo, VendorProfile},
    Message, MessageError, Opcode, Reader, Writer,
};

use super::{invalid, objects::read_public_desc, read_string, MAX_NAME, MAX_NEWS};

/// Most entries a list holds on the relay's side: a bound on a count from the wire.
const MAX_LIST: usize = 65_536;

fn count(r: &mut Reader<'_>) -> Result<usize, MessageError> {
    let n = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 list count"))?;
    if n > MAX_LIST {
        return Err(invalid("ac1999 list count"));
    }
    Ok(n)
}

/// `0x0062`: a vendor's profile and wares. The 1999 profile is six words, with no alternate currency.
#[derive(Debug, Clone, PartialEq)]
pub struct Vendor(pub VendorInfo);

impl Message for Vendor {
    const OPCODE: Opcode = Opcode(0x0062);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let merchant_id = ObjectId(r.u32()?);
        let profile = VendorProfile {
            item_types: r.u32()?,
            min_value: r.i32()?,
            max_value: r.i32()?,
            magic: r.i32()?,
            buy_price: r.f32()?,
            sell_price: r.f32()?,
            ..VendorProfile::default()
        };
        let n = count(r)?;
        let mut items = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            // The amount in 24 bits, its top byte all ones when a description follows.
            let v = r.u32()?;
            let amount = (v << 8).cast_signed() >> 8;
            let iid = ObjectId(r.u32()?);
            let pwd = if v.cast_signed() >> 24 == -1 {
                Some(read_public_desc(r)?.0)
            } else {
                None
            };
            items.push(ItemProfile { amount, iid, pwd });
        }
        Ok(Self(VendorInfo {
            merchant_id,
            profile,
            items,
        }))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}

/// One fellow: experience and level in sixteen bits each, then the name. The 1999 wire carries
/// no vitals here, and a Launch fellow shares no loot.
fn read_fellow(r: &mut Reader<'_>) -> Result<Fellow, MessageError> {
    let cp_cache = u32::from(r.u16()?);
    let level = u32::from(r.u16()?);
    Ok(Fellow {
        name: read_string(r, MAX_NAME)?,
        level,
        cp_cache,
        ..Fellow::default()
    })
}

/// `0x00AF`: the whole fellowship: the roster, the name and the leader. Launch shares experience.
#[derive(Debug, Clone, PartialEq)]
pub struct FullUpdate(pub FellowshipFullUpdate);

impl Message for FullUpdate {
    const OPCODE: Opcode = Opcode(0x00AF);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let members = r.packed_hash(|r| Ok((r.u32()?, read_fellow(r)?)))?;
        Ok(Self(FellowshipFullUpdate(Fellowship {
            members,
            name: read_string(r, MAX_NEWS)?,
            leader: ObjectId(r.u32()?),
            share_xp: 1,
            even_xp_split: 0,
            ..Fellowship::default()
        })))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}

/// `0x00B0`: one fellow.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateFellow(pub FellowshipUpdateFellow);

impl Message for UpdateFellow {
    const OPCODE: Opcode = Opcode(0x00B0);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(FellowshipUpdateFellow {
            fellow_id: ObjectId(r.u32()?),
            fellow: read_fellow(r)?,
            update_type: 1,
        }))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}

/// One member, as the ac1999 server writes it.
fn read_member(r: &mut Reader<'_>) -> Result<AllegianceData, MessageError> {
    let id = ObjectId(r.u32()?);
    let cp_cached = r.u32()?;
    let cp_tithed = r.u32()?;
    let bitfield = r.u32()?;
    let gender = u32::from(r.u8()?);
    let heritage_group = u32::from(r.u8()?);
    let rank = u32::from(r.u16()?);
    let loyalty = u32::from(r.u16()?);
    let leadership = u32::from(r.u16()?);
    #[allow(clippy::cast_possible_truncation)] // Seconds online, well inside an i32.
    let time_online = r.f64()? as i32;
    Ok(AllegianceData {
        id,
        name: read_string(r, MAX_NAME)?,
        gender,
        heritage_group,
        rank,
        bitfield,
        cp_tithed,
        cp_cached,
        loyalty,
        leadership,
        time_online,
        ..AllegianceData::default()
    })
}

/// `0x0020`: totals, a bare count, and each member after the monarch
/// with its patron first. The later hierarchy opens with a version: 0 reads none of its later fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Update(pub AllegianceUpdate);

impl Message for Update {
    const OPCODE: Opcode = Opcode(0x0020);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let total_members = r.u32()?;
        let total_vassals = r.u32()?;
        let n = count(r)?;
        let mut members = Vec::with_capacity(n.min(1024));
        for i in 0..n {
            let patron = if i == 0 {
                None
            } else {
                Some(ObjectId(r.u32()?))
            };
            members.push((patron, read_member(r)?));
        }
        // The 1999 message has no rank of its own; the client derives it from the members.
        Ok(Self(AllegianceUpdate {
            rank: 0,
            profile: AllegianceProfile {
                total_members,
                total_vassals,
                hierarchy: AllegianceHierarchy {
                    members,
                    ..AllegianceHierarchy::default()
                },
            },
        }))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}

#[cfg(test)]
mod tests;
