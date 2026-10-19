//! The write direction for the records a world builder makes: a landblock's terrain, its statics
//! and buildings, an indoor cell, a surface, an image texture and an image.
//!
//! Each `encode` is the exact inverse of its type's decoder: decoding a record and encoding the
//! value gives back the record's bytes, for every record of the kind the retail files hold. The
//! two directions are separate functions (field order is the format), so a test of one against
//! the other is a real check.
//!
//! Padding is written as zero bytes, which is what every retail record holds there. A frame is
//! written as its value: a decoder that replaced an invalid rotation with the identity hands back
//! the identity, and that is what is written.

use dereth_primitives::{DataId, Frame};

use crate::material::{PFID_INDEX16, PFID_P8};
use crate::world::{
    BuildInfo, BuildingPortal, CellLandblock, CellPortal, EnvCell, LandblockInfo, ObjectEntry,
    LANDBLOCK_BYTES,
};
use crate::{RenderSurface, Surface, SurfaceTexture};

/// A record being written, little-endian, as the files are.
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    fn id(&mut self, v: DataId) {
        self.u32(v.raw());
    }
    /// A count the format stores in 32 bits.
    fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }
    /// Origin x, y, z, then the rotation w, x, y, z: the frame record's order.
    fn frame(&mut self, f: &Frame) {
        for v in [
            f.origin.x,
            f.origin.y,
            f.origin.z,
            f.rotation.w,
            f.rotation.x,
            f.rotation.y,
            f.rotation.z,
        ] {
            self.f32(v);
        }
    }
    /// Zero bytes up to the next multiple of four from the record's start.
    fn align(&mut self) {
        while !self.0.len().is_multiple_of(4) {
            self.0.push(0);
        }
    }
}

impl CellLandblock {
    /// The `0xXXYYFFFF` record: the id, `lbi_exists`, 81 terrain words, 81 height indices and one
    /// pad byte, always [`LANDBLOCK_BYTES`].
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(LANDBLOCK_BYTES));
        o.id(self.id);
        o.u32(self.lbi_exists);
        for t in self.terrain {
            o.u16(t);
        }
        for h in self.height {
            o.u8(h);
        }
        o.align();
        o.0
    }
}

fn object(o: &mut Out, e: &ObjectEntry) {
    o.id(e.id);
    o.frame(&e.frame);
}

fn building_portal(o: &mut Out, p: &BuildingPortal) {
    o.u16(p.flags);
    o.u16(p.other_cell_id);
    o.i16(p.other_portal_id);
    o.u16(u16::try_from(p.stab_list.len()).unwrap_or(u16::MAX));
    for s in &p.stab_list {
        o.u16(*s);
    }
    o.align();
}

fn building(o: &mut Out, b: &BuildInfo) {
    o.id(b.id);
    o.frame(&b.frame);
    o.u32(b.num_leaves);
    o.count(b.portals.len());
    for p in &b.portals {
        building_portal(o, p);
    }
}

impl LandblockInfo {
    /// The `0xXXYYFFFE` record: the id, the cell count, the statics, the buildings (their count in
    /// the low half of a dword whose high half is the pack mask), and the restriction table when the
    /// pack mask's bit 0 says one follows.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(16 + self.objects.len() * 32));
        o.id(self.id);
        o.u32(self.num_cells);
        o.count(self.objects.len());
        for e in &self.objects {
            object(&mut o, e);
        }
        o.u16(u16::try_from(self.buildings.len()).unwrap_or(u16::MAX));
        o.u16(self.pack_mask);
        for b in &self.buildings {
            building(&mut o, b);
        }
        if self.pack_mask & 1 != 0 {
            let empty = crate::world::RestrictionTable {
                buckets: 0,
                entries: Vec::new(),
            };
            let t = self.restrictions.as_ref().unwrap_or(&empty);
            let count = u32::try_from(t.entries.len()).unwrap_or(0xFFFF) & 0xFFFF;
            o.u32((t.buckets << 16) | count);
            for (cell, owner) in &t.entries {
                o.u32(*cell);
                o.u32(*owner);
            }
        }
        o.0
    }
}

fn cell_portal(o: &mut Out, p: &CellPortal) {
    o.u16(p.flags);
    o.u16(p.polygon_id);
    // A portal to the outdoors keeps no cell: the files hold 0xFFFF there.
    #[allow(clippy::cast_possible_truncation)] // the low 16 bits are what the file stores
    o.u16(if p.flags & 4 != 0 {
        0xFFFF
    } else {
        p.other_cell_id as u16
    });
    o.i16(p.other_portal_id);
}

impl EnvCell {
    /// The `0xXXYY0100+` record: the id, the flags, the id again, the surface, portal and visible
    /// cell counts, the surfaces and the environment as their low 16 bits, the cell struct, the
    /// frame, the portals, the visible cells, then the statics when the flags' bit 1 says they
    /// follow and the restriction object when bit 3 does. Nothing pads.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(
            48 + 2 * self.surfaces.len()
                + 8 * self.portals.len()
                + 2 * self.visible_cells.len()
                + 32 * self.static_objects.len(),
        ));
        let low = |id: DataId| u16::try_from(id.raw() & 0xFFFF).unwrap_or(0);
        o.id(self.id);
        o.u32(self.flags);
        o.u32(self.cell_id_repeat);
        o.u8(u8::try_from(self.surfaces.len()).unwrap_or(u8::MAX));
        o.u8(u8::try_from(self.portals.len()).unwrap_or(u8::MAX));
        o.u16(u16::try_from(self.visible_cells.len()).unwrap_or(u16::MAX));
        for s in &self.surfaces {
            o.u16(low(*s));
        }
        o.u16(low(self.environment));
        o.u16(self.cell_struct);
        o.frame(&self.frame);
        for p in &self.portals {
            cell_portal(&mut o, p);
        }
        for v in &self.visible_cells {
            o.u16(*v);
        }
        if self.flags & 2 != 0 {
            o.count(self.static_objects.len());
            for e in &self.static_objects {
                object(&mut o, e);
            }
        }
        if self.flags & 8 != 0 {
            o.u32(self.restriction_obj.unwrap_or(0));
        }
        o.0
    }
}

impl Surface {
    /// The `0x08` record, which carries no id: the surface type, then the texture and palette when
    /// the type's bits 1 and 2 say it is textured (else the colour), then translucency,
    /// luminosity and diffuse.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(24));
        o.u32(self.surface_type);
        if self.surface_type & 6 != 0 {
            o.id(self.orig_texture_id.unwrap_or(DataId(0)));
            o.id(self.orig_palette_id.unwrap_or(DataId(0)));
        } else {
            o.u32(self.color_value.unwrap_or(0));
        }
        o.f32(self.translucency);
        o.f32(self.luminosity);
        o.f32(self.diffuse);
        o.0
    }
}

impl SurfaceTexture {
    /// The `0x05` record: the id, the category, the constant byte, then the images, largest first.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(13 + self.source_levels.len() * 4));
        o.id(self.id);
        o.u32(self.data_category);
        o.u8(self.unknown_byte);
        o.count(self.source_levels.len());
        for l in &self.source_levels {
            o.id(*l);
        }
        o.0
    }
}

impl RenderSurface {
    /// The `0x06` record holding `pixels`: the id, the category, the width, the height, the pixel
    /// format, the pixels' length and the pixels, then the default palette for the two paletted
    /// formats. The pixels' length is `pixels.len()`; the header's own `image_size` is not read.
    #[must_use]
    pub fn encode(&self, pixels: &[u8]) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(28 + pixels.len()));
        o.id(self.id);
        o.u32(self.data_category);
        o.u32(self.width);
        o.u32(self.height);
        o.u32(self.format);
        o.count(pixels.len());
        o.0.extend_from_slice(pixels);
        if self.format == PFID_P8 || self.format == PFID_INDEX16 {
            o.id(self.default_palette_id.unwrap_or(DataId(0)));
        }
        o.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::RestrictionTable;
    use crate::Decode;
    use dereth_primitives::{Quat, Vec3};

    fn frame(x: f32) -> Frame {
        Frame::new(Vec3::new(x, 2.5, -1.0), Quat::new(0.6, 0.0, 0.0, 0.8))
    }

    #[test]
    fn a_landblock_is_252_bytes_and_reads_back() {
        let mut lb = CellLandblock {
            id: DataId(0xA9B4_FFFF),
            lbi_exists: 1,
            terrain: [0; 81],
            height: [0; 81],
        };
        lb.terrain[9] = 0x7804;
        lb.height[80] = 200;
        let b = lb.encode();
        assert_eq!(b.len(), LANDBLOCK_BYTES);
        assert_eq!(CellLandblock::decode_payload(lb.id, &b).unwrap(), lb);
    }

    #[test]
    fn landblock_info_with_buildings_and_restrictions_reads_back() {
        let lbi = LandblockInfo {
            id: DataId(0xA9B4_FFFE),
            num_cells: 3,
            objects: vec![
                ObjectEntry {
                    id: DataId(0x0200_040B),
                    frame: frame(1.0),
                },
                ObjectEntry {
                    id: DataId(0x0100_0A2B),
                    frame: frame(2.0),
                },
            ],
            pack_mask: 1,
            buildings: vec![BuildInfo {
                id: DataId(0x0100_1234),
                frame: frame(3.0),
                num_leaves: 7,
                portals: vec![
                    // An odd stab count, so the portal pads to four.
                    BuildingPortal {
                        flags: 3,
                        other_cell_id: 0x0100,
                        other_portal_id: 2,
                        stab_list: vec![0x0100, 0x0101, 0x0102],
                    },
                    BuildingPortal {
                        flags: 0,
                        other_cell_id: 0x0102,
                        other_portal_id: -1,
                        stab_list: vec![0x0102, 0x0100],
                    },
                ],
            }],
            restrictions: Some(RestrictionTable {
                buckets: 8,
                entries: vec![(0xA9B4_0100, 0x7A9B_4001), (0xA9B4_0101, 0x7A9B_4001)],
            }),
        };
        let b = lbi.encode();
        assert_eq!(b.len() % 4, 0);
        assert_eq!(LandblockInfo::decode_payload(lbi.id, &b).unwrap(), lbi);
    }

    #[test]
    fn both_forms_of_a_surface_read_back() {
        let textured = Surface {
            surface_type: 2,
            orig_texture_id: Some(DataId(0x0500_1234)),
            orig_palette_id: Some(DataId(0)),
            color_value: None,
            translucency: 0.25,
            luminosity: 0.0,
            diffuse: 1.0,
        };
        let coloured = Surface {
            surface_type: 1,
            orig_texture_id: None,
            orig_palette_id: None,
            color_value: Some(0xFF80_4020),
            translucency: 0.0,
            luminosity: 0.5,
            diffuse: 0.75,
        };
        for s in [textured, coloured] {
            assert_eq!(Surface::decode_bytes(&s.encode()).unwrap(), s);
        }
    }

    #[test]
    fn a_surface_texture_and_a_paletted_image_read_back() {
        let st = SurfaceTexture {
            id: DataId(0x0500_3359),
            data_category: 0,
            unknown_byte: 2,
            source_levels: vec![DataId(0x0600_7577), DataId(0x0600_7578)],
        };
        assert_eq!(
            SurfaceTexture::decode_payload(st.id, &st.encode()).unwrap(),
            st
        );
        let pixels: Vec<u8> = (0..32u8).collect();
        let rs = RenderSurface {
            id: DataId(0x0600_7577),
            data_category: 4,
            width: 4,
            height: 4,
            format: PFID_INDEX16,
            image_size: 32,
            data_offset: 0x18,
            default_palette_id: Some(DataId(0x0400_0001)),
        };
        let b = rs.encode(&pixels);
        assert_eq!(b.len(), 24 + 32 + 4);
        let back = RenderSurface::decode_payload(rs.id, &b).unwrap();
        assert_eq!(back, rs);
        assert_eq!(back.payload(&b).unwrap(), pixels.as_slice());
    }
}
