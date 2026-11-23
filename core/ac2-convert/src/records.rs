//! What the converter makes, and with which ids: new ids handed out in order ([`Ids`]), the
//! records made ([`Records`]), and AC2's pictures as AC1 images and image textures, one each per
//! picture list and size ([`Pictures`]).
//!
//! **Pictures** keep AC2's own blocks: a block-compressed picture becomes an AC1 image in the
//! same format, so nothing is lost or re-encoded.

use std::collections::BTreeMap;

use dereth_assets::{RenderSurface, SurfaceTexture};
use dereth_primitives::DataId;

use crate::source::Ac2;

/// `A8R8G8B8`'s pixel format id.
pub const PFID_A8R8G8B8: u32 = 21;

/// New ids, handed out in order from the first free one of each kind the converter uses.
#[derive(Clone, Debug)]
pub struct Ids {
    next: BTreeMap<u32, u32>,
}

impl Default for Ids {
    fn default() -> Self {
        Self {
            next: BTreeMap::from([
                (0x01, 0x0100_C000),
                (0x02, 0x0200_C000),
                (0x03, 0x0300_C000),
                (0x05, 0x0500_C000),
                (0x06, 0x0600_C000),
                (0x08, 0x0800_C000),
                (0x0D, 0x0D00_C000),
                (0x11, 0x1100_C000),
                (0x12, 0x1200_C000),
                (0x32, 0x3200_C000),
                (0x33, 0x3300_C000),
            ]),
        }
    }
}

impl Ids {
    /// The next id of the kind whose ids start with byte `kind`.
    pub fn next(&mut self, kind: u32) -> DataId {
        let n = self
            .next
            .get_mut(&kind)
            .expect("a kind the converter makes");
        let id = *n;
        *n += 1;
        DataId(id)
    }
}

/// Where a new record's id comes from: the next of its kind, given the record's kind (the id's
/// first byte) and a name that says what it is, the same for the same thing every time it is made
/// (`picture 40100061 256`, `surface …`, `environment 0100004A`). A builder that numbers records
/// by what they are (a lockfile) numbers them by it; [`Ids`] hands them out in order.
pub trait NewIds {
    /// The id of the new record of kind `kind` named `name`.
    fn next_for(&mut self, kind: u32, name: &str) -> DataId;
}

impl NewIds for Ids {
    fn next_for(&mut self, kind: u32, _name: &str) -> DataId {
        self.next(kind)
    }
}

/// The records the converter adds or writes over, by id.
#[derive(Debug, Default)]
pub struct Records(pub BTreeMap<DataId, Vec<u8>>);

impl Records {
    pub fn add(&mut self, id: DataId, bytes: Vec<u8>) {
        self.0.insert(id, bytes);
    }
}

/// AC2's pictures as AC1 images and image textures, one each per picture list and size.
#[derive(Debug, Default)]
pub struct Pictures {
    made: BTreeMap<(u32, u32), (DataId, String)>,
}

impl Pictures {
    /// Picture list `list`'s largest level no more than `limit` a side as an AC1 image texture:
    /// its id, and a note of its size and format.
    pub fn texture(
        &mut self,
        ac2: &Ac2,
        list: u32,
        limit: u32,
        category: u32,
        ids: &mut dyn NewIds,
        out: &mut Records,
    ) -> Result<(DataId, String), String> {
        if let Some(m) = self.made.get(&(list, limit)) {
            return Ok(m.clone());
        }
        let (format, w, h, px) = ac2_image(ac2, list, limit)
            .ok_or_else(|| format!("AC2 picture {list:08X} will not read"))?;
        let name = format!("picture {list:08X} {limit}");
        let (rs_id, st_id) = (ids.next_for(0x06, &name), ids.next_for(0x05, &name));
        let rs = RenderSurface {
            id: rs_id,
            data_category: category,
            width: w,
            height: h,
            format,
            image_size: u32::try_from(px.len()).unwrap_or(u32::MAX),
            data_offset: 0x18,
            default_palette_id: None,
        };
        out.add(rs_id, rs.encode(&px));
        let st = SurfaceTexture {
            id: st_id,
            data_category: 0,
            unknown_byte: 2,
            source_levels: vec![rs_id],
        };
        out.add(st_id, st.encode());
        let note = format!("{w}x{h} {}", format_name(format));
        self.made.insert((list, limit), (st_id, note.clone()));
        Ok((st_id, note))
    }
}

fn format_name(format: u32) -> String {
    match format {
        0x3154_5844 => "DXT1".into(),
        0x3354_5844 => "DXT3".into(),
        0x3554_5844 => "DXT5".into(),
        PFID_A8R8G8B8 => "A8R8G8B8".into(),
        f => format!("{f:#X}"),
    }
}

/// An AC2 picture list's largest level no more than `limit` a side, as an AC1 image: its pixel
/// format, size and pixels. Block-compressed pictures keep their blocks; any other is expanded to
/// 32 bits a texel.
fn ac2_image(ac2: &Ac2, list: u32, limit: u32) -> Option<(u32, u32, u32, Vec<u8>)> {
    use auberean_assets::picture::{Picture, Texture};
    use dereth_render_cpu::pixel_format::PixelFormatId;
    let t: Texture = ac2.record(list)?;
    let p: Picture = t.levels.iter().find_map(|l| {
        let p: Picture = ac2.record(l.0)?;
        (p.width.max(p.height) <= limit).then_some(p)
    })?;
    let f = PixelFormatId::from_raw(p.format);
    match f {
        PixelFormatId::Dxt1 | PixelFormatId::Dxt3 | PixelFormatId::Dxt5 => {
            let blocks = (p.width.div_ceil(4) * p.height.div_ceil(4)) as usize;
            let per = if f == PixelFormatId::Dxt1 { 8 } else { 16 };
            let bytes = p.bytes.get(..blocks * per)?.to_vec();
            Some((p.format, p.width, p.height, bytes))
        }
        _ => {
            let rgba = ac2.picture(list, p.width.max(p.height))?;
            let bgra = rgba
                .texels
                .chunks(4)
                .flat_map(|t| [t[2], t[1], t[0], t[3]])
                .collect();
            Some((PFID_A8R8G8B8, rgba.width, rgba.height, bgra))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the converter's ids)
    use super::*;

    #[test]
    fn ids_are_handed_out_in_order_per_kind() {
        let mut ids = Ids::default();
        assert_eq!(ids.next(0x06), DataId(0x0600_C000));
        assert_eq!(ids.next(0x05), DataId(0x0500_C000));
        assert_eq!(ids.next(0x06), DataId(0x0600_C001));
        assert_eq!(ids.next(0x01), DataId(0x0100_C000));
        assert_eq!(ids.next(0x08), DataId(0x0800_C000));
    }
}
