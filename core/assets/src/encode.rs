//! The write direction for the records a world builder makes: a landblock's terrain, its statics
//! and buildings, an indoor cell, a surface, an image texture and an image, a model (`GfxObj`), a
//! composite object (`Setup`) and an environment with its cell shapes.
//!
//! Each `encode` is the exact inverse of its type's decoder: decoding a record and encoding the
//! value gives back the record's bytes, for every record of the kind the retail files hold. The
//! two directions are separate functions (field order is the format), so a test of one against
//! the other is a real check.
//!
//! Padding is written as zero bytes, which is what every retail record holds there. A frame is
//! written as its value: a decoder that replaced an invalid rotation with the identity hands back
//! the identity, and that is what is written.
//!
//! **Each era's layout.** The record layouts changed over the life of the game, so each record a
//! world builder writes is also written in the layout of a given era's files (`encode_for`), the
//! inverse of the decoder for those files ([`crate::Decode::decode_payload_for`]). Each type
//! chooses its layout by the era itself: Infiltration's files (February 2005, before Throne of
//! Destiny) lay out indoor cells, models, environments, surfaces and pictures otherwise than the
//! end of retail's, and keep landblocks, their statics and composite objects as the later files
//! do. A record the era's files cannot hold (a later picture format in an older file, a model
//! naming a detail record the older files reach another way) is refused, not written another way.

use dereth_dat::ContainerEra;
use dereth_primitives::{DataId, EraId, Frame, Vec3};

use crate::common::{BspKind, BspTree, Plane, Polygon, Sphere, VertexArray};
use crate::error::AssetError;
use crate::geometry::{CellStruct, Environment, GfxObj, Setup};
use crate::hook::{AnimHook, HookData};
use crate::material::{pre_tod_image_format, PFID_CUSTOM_B8G8R8, PFID_INDEX16, PFID_P8};
use crate::motion::GfxObjDegradeInfo;
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
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    fn sphere(&mut self, s: &Sphere) {
        self.vec3(s.center);
        self.f32(s.radius);
    }
    fn plane(&mut self, p: &Plane) {
        self.vec3(p.normal);
        self.f32(p.d);
    }
    /// The compressed count: one byte under `0x80`, two under `0x4000` (the high byte first, its
    /// top bit set), else four (the high word big-endian with its top two bits set, then the low
    /// word as a native `u16`).
    fn compressed(&mut self, n: usize) {
        let n = u32::try_from(n).unwrap_or(0x3FFF_FFFF);
        if n < 0x80 {
            self.u8(u8::try_from(n).unwrap_or(0));
        } else if n < 0x4000 {
            self.u8(0x80 | u8::try_from(n >> 8).unwrap_or(0));
            self.u8(u8::try_from(n & 0xFF).unwrap_or(0));
        } else {
            self.u8(0xC0 | u8::try_from((n >> 24) & 0x3F).unwrap_or(0));
            self.u8(u8::try_from((n >> 16) & 0xFF).unwrap_or(0));
            self.u16(u16::try_from(n & 0xFFFF).unwrap_or(0));
        }
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

    /// The record in the layout of the files of `era`: the same in every era there is.
    ///
    /// # Errors
    /// None today; an era whose landblocks are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration | EraId::Eor => Ok(self.encode()),
        }
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

    /// The record in the layout of the files of `era`: the same in every era there is.
    ///
    /// # Errors
    /// None today; an era whose statics records are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration | EraId::Eor => Ok(self.encode()),
        }
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

    /// The record in the layout of the files of `era`. Infiltration's starts with the flags and
    /// then the id (it does not repeat the id), and pads to four bytes after the surfaces and
    /// after the visible cells; the end of retail's is [`Self::encode`].
    ///
    /// # Errors
    /// None today; an era whose cells are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => Ok(self.encode_older()),
            EraId::Eor => Ok(self.encode()),
        }
    }

    fn encode_older(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(
            44 + 2 * self.surfaces.len()
                + 8 * self.portals.len()
                + 2 * self.visible_cells.len()
                + 32 * self.static_objects.len(),
        ));
        let low = |id: DataId| u16::try_from(id.raw() & 0xFFFF).unwrap_or(0);
        o.u32(self.flags);
        o.id(self.id);
        o.u8(u8::try_from(self.surfaces.len()).unwrap_or(u8::MAX));
        o.u8(u8::try_from(self.portals.len()).unwrap_or(u8::MAX));
        o.u16(u16::try_from(self.visible_cells.len()).unwrap_or(u16::MAX));
        for s in &self.surfaces {
            o.u16(low(*s));
        }
        o.align();
        o.u16(low(self.environment));
        o.u16(self.cell_struct);
        o.frame(&self.frame);
        for p in &self.portals {
            cell_portal(&mut o, p);
        }
        for v in &self.visible_cells {
            o.u16(*v);
        }
        o.align();
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

    /// The record `id` in the layout of the files of `era`: Infiltration's starts with its own
    /// id, which the end of retail's ([`Self::encode`]) does not carry.
    ///
    /// # Errors
    /// None today; an era whose surfaces are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId, id: DataId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => {
                let mut o = Out(Vec::with_capacity(28));
                o.id(id);
                o.0.extend_from_slice(&self.encode());
                Ok(o.0)
            }
            EraId::Eor => Ok(self.encode()),
        }
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

    /// The record in the layout of the files of `era`. In Infiltration's files an image texture
    /// holds its own pixels rather than naming its images, so it is written from the image it
    /// holds ([`RenderSurface::encode_texture_for`]) and this refuses.
    ///
    /// # Errors
    /// [`AssetError::Unsupported`] for an era whose image textures carry their pixels.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => Err(AssetError::Unsupported {
                what: "image texture naming its images, in files whose textures hold pixels",
                value: u32::try_from(self.source_levels.len()).unwrap_or(u32::MAX),
            }),
            EraId::Eor => Ok(self.encode()),
        }
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

    /// The `0x06` record holding `pixels` in the layout of the files of `era`. Infiltration's
    /// is the id, the width, the height and three bytes a pixel ([`PFID_CUSTOM_B8G8R8`]), with
    /// nothing else; the end of retail's is [`Self::encode`].
    ///
    /// # Errors
    /// [`AssetError::Unsupported`] for a picture the era's images cannot hold: another pixel
    /// format, or pixels that are not the picture's size.
    pub fn encode_for(&self, era: EraId, pixels: &[u8]) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => {
                if self.format != PFID_CUSTOM_B8G8R8 {
                    return Err(AssetError::Unsupported {
                        what: "pixel format of an image in files before Throne of Destiny",
                        value: self.format,
                    });
                }
                self.check_size(pixels, 3)?;
                let mut o = Out(Vec::with_capacity(12 + pixels.len()));
                o.id(self.id);
                o.u32(self.width);
                o.u32(self.height);
                o.0.extend_from_slice(pixels);
                Ok(o.0)
            }
            EraId::Eor => Ok(self.encode(pixels)),
        }
    }

    /// The `0x05` image texture holding this picture's `pixels` itself, in the layout of the
    /// files of `era` whose image textures carry their pixels. Infiltration's is the id, the
    /// image type ([`pre_tod_image_format`]'s number for this picture's format), the width, the
    /// height, the pixels, the default palette for a paletted picture, then padding to four
    /// bytes.
    ///
    /// # Errors
    /// [`AssetError::Unsupported`] for an era whose image textures name their images instead
    /// ([`SurfaceTexture::encode`]), a format the era's textures have no type for, or pixels that
    /// are not the picture's size.
    pub fn encode_texture_for(&self, era: EraId, pixels: &[u8]) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => {
                let (image_type, bytes_per_pixel) = (0..=u32::from(u8::MAX))
                    .find_map(|t| {
                        pre_tod_image_format(t)
                            .filter(|(_, format)| *format == self.format)
                            .map(|(bpp, _)| (t, bpp))
                    })
                    .ok_or(AssetError::Unsupported {
                        what: "pixel format of an image texture before Throne of Destiny",
                        value: self.format,
                    })?;
                self.check_size(pixels, bytes_per_pixel)?;
                let mut o = Out(Vec::with_capacity(24 + pixels.len()));
                o.id(self.id);
                o.u32(image_type);
                o.u32(self.width);
                o.u32(self.height);
                o.0.extend_from_slice(pixels);
                if self.format == PFID_P8 {
                    o.id(self.default_palette_id.unwrap_or(DataId(0)));
                }
                o.align();
                Ok(o.0)
            }
            EraId::Eor => Err(AssetError::Unsupported {
                what: "image texture holding pixels, in files whose textures name images",
                value: self.format,
            }),
        }
    }

    /// Whether `pixels` is `bytes_per_pixel` bytes for each of the picture's pixels.
    fn check_size(&self, pixels: &[u8], bytes_per_pixel: u32) -> Result<(), AssetError> {
        let want = u64::from(self.width) * u64::from(self.height) * u64::from(bytes_per_pixel);
        if want == pixels.len() as u64 {
            Ok(())
        } else {
            Err(AssetError::Unsupported {
                what: "pixel count of a picture",
                value: u32::try_from(pixels.len()).unwrap_or(u32::MAX),
            })
        }
    }
}

/// A vertex array. Before Throne of Destiny each vertex pads to four bytes, which its fields
/// always fill already.
fn vertex_array(o: &mut Out, v: &VertexArray, layout: ContainerEra) {
    o.u32(v.vertex_type);
    o.count(v.vertices.len());
    for x in &v.vertices {
        o.i16(x.id);
        o.u16(u16::try_from(x.uvs.len()).unwrap_or(u16::MAX));
        o.vec3(x.position);
        o.vec3(x.normal);
        for (u, w) in &x.uvs {
            o.f32(*u);
            o.f32(*w);
        }
        if layout == ContainerEra::Classic {
            o.align();
        }
    }
}

/// One polygon. A one-sided polygon's record holds its positive surface as its negative one too,
/// which is what the decoder hands back. Before Throne of Destiny it pads to four bytes.
fn polygon(o: &mut Out, p: &Polygon, layout: ContainerEra) {
    o.i16(p.poly_id);
    o.u8(p.num_pts);
    o.u8(p.stippling);
    o.i32(p.sides_type);
    o.u16(p.pos_surface);
    o.u16(p.neg_surface);
    for v in &p.vertex_ids {
        o.u16(*v);
    }
    if p.stippling & 4 == 0 {
        for i in p.pos_uv_indices.iter().flatten() {
            o.u8(*i);
        }
    }
    if p.sides_type == 2 && p.stippling & 8 == 0 {
        for i in p.neg_uv_indices.iter().flatten() {
            o.u8(*i);
        }
    }
    if layout == ContainerEra::Classic {
        o.align();
    }
}

const TAG_PORT: u32 = 0x504F_5254;
const TAG_LEAF: u32 = 0x4C45_4146;

/// A BSP tree of `kind`, depth first from its root: each node's tag, then a leaf's index (and a
/// physics leaf's solid flag, sphere and polygons) or an interior node's plane and children, the
/// positive first; then what a node carries after its children (a drawing node's sphere and
/// polygons, a drawing portal node's portals, a physics node's sphere). Before Throne of Destiny
/// a node's polygon list (a drawing node's, with its portals, and a physics leaf's) pads to four
/// bytes.
fn bsp(o: &mut Out, tree: &BspTree, kind: BspKind, layout: ContainerEra) {
    let aligned = layout == ContainerEra::Classic;
    enum Task {
        Write(usize),
        After(usize),
    }
    let mut stack = vec![Task::Write(0)];
    while let Some(task) = stack.pop() {
        match task {
            Task::Write(i) => {
                let Some(n) = tree.nodes.get(i) else { continue };
                o.u32(n.tag);
                if n.tag == TAG_LEAF {
                    o.u32(n.leaf_index.unwrap_or(0));
                    if kind == BspKind::Physics {
                        o.i32(n.solid.unwrap_or(0));
                        o.sphere(&n.sphere.unwrap_or_default());
                        o.count(n.in_polys.len());
                        for p in &n.in_polys {
                            o.i16(*p);
                        }
                        if aligned {
                            o.align();
                        }
                    }
                    continue;
                }
                o.plane(&n.plane.unwrap_or_default());
                stack.push(Task::After(i));
                // Popped positive first.
                if let Some(c) = n.neg_child {
                    stack.push(Task::Write(c as usize));
                }
                if let Some(c) = n.pos_child {
                    stack.push(Task::Write(c as usize));
                }
            }
            Task::After(i) => {
                let n = &tree.nodes[i];
                match kind {
                    BspKind::Drawing => {
                        o.sphere(&n.sphere.unwrap_or_default());
                        o.count(n.in_polys.len());
                        if n.tag == TAG_PORT {
                            o.count(n.in_portals.len());
                        }
                        for p in &n.in_polys {
                            o.i16(*p);
                        }
                        if n.tag == TAG_PORT {
                            for (p, q) in &n.in_portals {
                                o.i16(*p);
                                o.i16(*q);
                            }
                        }
                        if aligned {
                            o.align();
                        }
                    }
                    BspKind::Physics if n.tag != TAG_PORT => {
                        o.sphere(&n.sphere.unwrap_or_default());
                    }
                    _ => {}
                }
            }
        }
    }
}

impl GfxObj {
    /// The `0x01` record: the id, the flags, the surfaces, the vertices, the physics polygons and
    /// tree (flags bit 0), the sort center, the drawing polygons and tree (bit 1), and the degrade
    /// record's id (bit 3). The counts are compressed.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.encode_in(ContainerEra::Modern, self.flags)
    }

    /// The record in the layout of the files of `era`. Infiltration's counts are plain dwords,
    /// its polygons and tree nodes pad to four bytes, it names no detail record (an object reaches
    /// its own by its id, [`crate::geometry::implicit_degrade_id`]) and the whole pads to four
    /// bytes; the end of retail's is [`Self::encode`].
    ///
    /// # Errors
    /// [`AssetError::Unsupported`] for an object Infiltration's files cannot hold: one with a
    /// triangle-strip block, or one naming a detail record other than the one its id reaches.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => {
                if self.flags & 4 != 0 {
                    return Err(AssetError::Unsupported {
                        what: "triangle-strip block",
                        value: self.flags,
                    });
                }
                let implied = crate::geometry::implicit_degrade_id(self.id);
                if let Some(d) = self.did_degrade.filter(|d| *d != implied) {
                    return Err(AssetError::Unsupported {
                        what: "detail record other than the one the object's id reaches",
                        value: d.raw(),
                    });
                }
                Ok(self.encode_in(ContainerEra::Classic, self.flags & !8))
            }
            EraId::Eor => Ok(self.encode()),
        }
    }

    fn encode_in(&self, layout: ContainerEra, flags: u32) -> Vec<u8> {
        let older = layout == ContainerEra::Classic;
        let mut o = Out(Vec::with_capacity(
            64 + self.vertex_array.vertices.len() * 40,
        ));
        let count = |o: &mut Out, n: usize| {
            if older {
                o.count(n);
            } else {
                o.compressed(n);
            }
        };
        o.id(self.id);
        o.u32(flags);
        count(&mut o, self.surfaces.len());
        for s in &self.surfaces {
            o.id(*s);
        }
        vertex_array(&mut o, &self.vertex_array, layout);
        if flags & 1 != 0 {
            count(&mut o, self.physics_polygons.len());
            for p in &self.physics_polygons {
                polygon(&mut o, p, layout);
            }
            bsp(
                &mut o,
                self.physics_bsp.as_ref().unwrap_or(&BspTree::default()),
                BspKind::Physics,
                layout,
            );
        }
        o.vec3(self.sort_center);
        if flags & 2 != 0 {
            count(&mut o, self.polygons.len());
            for p in &self.polygons {
                polygon(&mut o, p, layout);
            }
            bsp(
                &mut o,
                self.drawing_bsp.as_ref().unwrap_or(&BspTree::default()),
                BspKind::Drawing,
                layout,
            );
        }
        if older {
            o.align();
        } else if flags & 8 != 0 {
            o.id(self.did_degrade.unwrap_or(DataId(0)));
        }
        o.0
    }
}

fn hook(o: &mut Out, h: &AnimHook) {
    o.u32(h.hook_type);
    o.i32(h.direction);
    match &h.data {
        HookData::Sound { sound_id } => o.id(*sound_id),
        HookData::SoundTable { sound_type } => o.u32(*sound_type),
        HookData::Attack(a) => {
            o.u32(a.part_index);
            o.f32(a.left.0);
            o.f32(a.left.1);
            o.f32(a.right.0);
            o.f32(a.right.1);
            o.f32(a.radius);
            o.f32(a.height);
        }
        HookData::ReplaceObject {
            part_index,
            part_id,
        } => {
            o.u8(*part_index);
            // A data id of a known type, against the model's base.
            let delta = part_id.raw().wrapping_sub(0x0100_0000);
            if delta < 0x8000 {
                o.u16(u16::try_from(delta).unwrap_or(0));
            } else {
                o.u16(0x8000 | u16::try_from((delta >> 16) & 0x3FFF).unwrap_or(0));
                o.u16(u16::try_from(delta & 0xFFFF).unwrap_or(0));
            }
        }
        HookData::Ethereal { ethereal } => o.i32(*ethereal),
        HookData::PartRamp(r) => {
            o.u32(r.part);
            o.f32(r.start);
            o.f32(r.end);
            o.f32(r.time);
        }
        HookData::Ramp(r) => {
            o.f32(r.start);
            o.f32(r.end);
            o.f32(r.time);
        }
        HookData::Scale(s) => {
            o.f32(s.end);
            o.f32(s.time);
        }
        HookData::CreateParticle(p) => {
            o.id(p.emitter_info_id);
            o.u32(p.part_index);
            o.frame(&p.offset);
            o.u32(p.emitter_id);
        }
        HookData::Particle { emitter_id } => o.u32(*emitter_id),
        HookData::NoDraw { nodraw } => o.u32(*nodraw),
        HookData::DefaultScriptPart { part_index } => o.u32(*part_index),
        HookData::CallPes(p) => {
            o.id(p.pes);
            o.f32(p.pause);
        }
        HookData::SoundTweaked(s) => {
            o.id(s.sound_id);
            o.f32(s.probability);
            o.f32(s.priority);
            o.f32(s.volume);
        }
        HookData::SetOmega { axis } => o.vec3(*axis),
        HookData::TextureVelocity(t) => {
            o.f32(t.u_speed);
            o.f32(t.v_speed);
        }
        HookData::TextureVelocityPart(t) => {
            o.u32(t.part_index);
            o.f32(t.u_speed);
            o.f32(t.v_speed);
        }
        HookData::SetLight { lights_on } => o.i32(*lights_on),
        // `HookData::None` and any later payload-less kind.
        _ => {}
    }
    o.align();
}

impl Setup {
    /// The `0x02` record: the id, the flags, the parts (with their parents, flags bit 0, and
    /// scales, bit 1), the holding and connection points, the placement frames with their hooks,
    /// the cylinder-spheres and spheres, the height, radius and step heights (up before down),
    /// the sorting and selection spheres, the lights, and the five default ids; then padding to
    /// four bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(128 + self.parts.len() * 64));
        o.id(self.id);
        o.u32(self.flags);
        o.count(self.parts.len());
        for p in &self.parts {
            o.id(*p);
        }
        if self.flags & 1 != 0 {
            for p in self.parent_index.iter().flatten() {
                o.u32(*p);
            }
        }
        if self.flags & 2 != 0 {
            for s in self.default_scale.iter().flatten() {
                o.vec3(*s);
            }
        }
        let w = &self.written_order;
        for (table, order) in [
            (&self.holding_locations, &w.holding_locations),
            (&self.connection_points, &w.connection_points),
        ] {
            o.count(table.len());
            for key in in_order(table, order) {
                let e = &table[&key];
                o.u32(key);
                o.u32(e.part_id);
                o.frame(&e.frame);
            }
        }
        o.count(self.placement_frames.len());
        for key in in_order(&self.placement_frames, &w.placement_frames) {
            let p = &self.placement_frames[&key];
            o.u32(key);
            for f in &p.frames {
                o.frame(f);
            }
            o.count(p.hooks.len());
            for h in &p.hooks {
                hook(&mut o, h);
            }
        }
        o.count(self.cylspheres.len());
        for c in &self.cylspheres {
            o.vec3(c.low_pt);
            o.f32(c.radius);
            o.f32(c.height);
        }
        o.count(self.spheres.len());
        for s in &self.spheres {
            o.sphere(s);
        }
        o.f32(self.height);
        o.f32(self.radius);
        o.f32(self.step_up_height);
        o.f32(self.step_down_height);
        o.sphere(&self.sorting_sphere);
        o.sphere(&self.selection_sphere);
        o.count(self.lights.len());
        for key in in_order(&self.lights, &w.lights) {
            let l = &self.lights[&key];
            o.u32(key);
            o.frame(&l.frame);
            o.u32(l.color_argb);
            o.f32(l.intensity);
            o.f32(l.falloff);
            o.f32(l.cone_angle);
        }
        for id in [
            self.default_anim_id,
            self.default_script_id,
            self.default_mtable_id,
            self.default_stable_id,
            self.default_phstable_id,
        ] {
            o.id(id);
        }
        o.align();
        o.0
    }

    /// The record in the layout of the files of `era`: the same in every era there is.
    ///
    /// # Errors
    /// None today; an era whose composite objects are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration | EraId::Eor => Ok(self.encode()),
        }
    }
}

impl GfxObjDegradeInfo {
    /// The `0x11` record: the id, then each level's model, mode and three distances, nearest
    /// first.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(8 + self.degrades.len() * 20));
        o.id(self.id);
        o.count(self.degrades.len());
        for d in &self.degrades {
            o.id(d.gfxobj_id);
            o.i32(d.degrade_mode);
            o.f32(d.min_dist);
            o.f32(d.ideal_dist);
            o.f32(d.max_dist);
        }
        o.0
    }

    /// The record in the layout of the files of `era`: the same in every era there is.
    ///
    /// # Errors
    /// None today; an era whose detail records are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration | EraId::Eor => Ok(self.encode()),
        }
    }
}

/// A table's keys in `order` when it names exactly the table's keys, else in key order.
fn in_order<V>(table: &std::collections::BTreeMap<u32, V>, order: &[u32]) -> Vec<u32> {
    let same = order.len() == table.len() && order.iter().all(|k| table.contains_key(k));
    if same {
        order.to_vec()
    } else {
        table.keys().copied().collect()
    }
}

fn cell_struct(o: &mut Out, c: &CellStruct, layout: ContainerEra) {
    o.u32(c.cellstruct_id);
    o.count(c.polygons.len());
    o.count(c.physics_polygons.len());
    o.count(c.portals.len());
    vertex_array(o, &c.vertex_array, layout);
    for p in &c.polygons {
        polygon(o, p, layout);
    }
    for p in &c.portals {
        o.u16(*p);
    }
    o.align();
    bsp(o, &c.cell_bsp, BspKind::Cell, layout);
    for p in &c.physics_polygons {
        polygon(o, p, layout);
    }
    bsp(o, &c.physics_bsp, BspKind::Physics, layout);
    o.u32(u32::from(c.drawing_bsp.is_some()));
    if let Some(t) = &c.drawing_bsp {
        bsp(o, t, BspKind::Drawing, layout);
    }
    o.align();
}

impl Environment {
    /// The `0x0D` record: the id and the cell shapes, each with its vertices, its drawing
    /// polygons, its portal polygons' ids, its cell tree, its physics polygons and tree, and its
    /// drawing tree when it has one.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.encode_in(ContainerEra::Modern)
    }

    /// The record in the layout of the files of `era`. Infiltration's shapes are the same, with
    /// each polygon and tree node padded to four bytes; the end of retail's is [`Self::encode`].
    ///
    /// # Errors
    /// None today; an era whose environments are laid out otherwise may refuse one.
    pub fn encode_for(&self, era: EraId) -> Result<Vec<u8>, AssetError> {
        match era {
            EraId::Infiltration => Ok(self.encode_in(ContainerEra::Classic)),
            EraId::Eor => Ok(self.encode()),
        }
    }

    fn encode_in(&self, layout: ContainerEra) -> Vec<u8> {
        let mut o = Out(Vec::with_capacity(4096));
        o.id(self.id);
        o.count(self.cells.len());
        for c in &self.cells {
            cell_struct(&mut o, c, layout);
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

    const OLDER: EraId = EraId::Infiltration;

    /// An indoor cell in the older layout starts with its flags, then its id, and pads after an
    /// odd surface list and after the visible cells.
    #[test]
    fn an_older_indoor_cell_pads_after_its_surfaces_and_visible_cells() {
        let cell = EnvCell {
            id: DataId(0x0123_0100),
            flags: 2,
            cell_id_repeat: 0x0123_0100,
            surfaces: vec![
                DataId(0x0800_0010),
                DataId(0x0800_0011),
                DataId(0x0800_0012),
            ],
            environment: DataId(0x0D00_0170),
            cell_struct: 1,
            frame: frame(4.0),
            portals: vec![CellPortal {
                flags: 0,
                polygon_id: 3,
                other_cell_id: 0x0101,
                other_portal_id: 0,
            }],
            visible_cells: vec![0x0101],
            static_objects: vec![ObjectEntry {
                id: DataId(0x0200_0001),
                frame: frame(1.0),
            }],
            restriction_obj: None,
        };
        let b = cell.encode_for(OLDER).unwrap();
        assert_eq!(&b[..8], [2, 0, 0, 0, 0x00, 0x01, 0x23, 0x01]);
        // Three surfaces end at 18; the environment starts at 20.
        assert_eq!(&b[18..22], [0, 0, 0x70, 0x01]);
        assert_eq!(b.len() % 4, 0);
        let back = EnvCell::decode_payload_in(ContainerEra::Classic, cell.id, &b).unwrap();
        assert_eq!(back, cell);
        assert_eq!(cell.encode_for(EraId::Eor).unwrap(), cell.encode());
    }

    /// A surface in the older layout carries its id in front.
    #[test]
    fn an_older_surface_carries_its_id() {
        let s = Surface {
            surface_type: 2,
            orig_texture_id: Some(DataId(0x0500_1234)),
            orig_palette_id: Some(DataId(0)),
            color_value: None,
            translucency: 0.0,
            luminosity: 0.0,
            diffuse: 1.0,
        };
        let id = DataId(0x0800_0042);
        let b = s.encode_for(OLDER, id).unwrap();
        assert_eq!(&b[..4], &id.raw().to_le_bytes());
        assert_eq!(&b[4..], s.encode().as_slice());
        assert_eq!(
            Surface::decode_payload_in(ContainerEra::Classic, id, &b).unwrap(),
            s
        );
    }

    /// An older image is three bytes a pixel and nothing else; an older image texture holds its
    /// pixels, in one of its own types, padded to four bytes; a later image texture, which names
    /// its images, and a picture in a later format are refused there.
    #[test]
    fn older_pictures_hold_their_pixels_and_later_forms_are_refused() {
        let rgb: Vec<u8> = (0..27u8).collect();
        let image = RenderSurface {
            id: DataId(0x0600_2000),
            data_category: 0,
            width: 3,
            height: 3,
            format: PFID_CUSTOM_B8G8R8,
            image_size: 27,
            data_offset: 12,
            default_palette_id: None,
        };
        let b = image.encode_for(OLDER, &rgb).unwrap();
        assert_eq!(b.len(), 12 + 27);
        let back = RenderSurface::decode_payload_in(ContainerEra::Classic, image.id, &b).unwrap();
        assert_eq!(back, image);
        assert!(
            image.encode_for(OLDER, &rgb[1..]).is_err(),
            "a short picture"
        );

        // A 16-bit picture with alpha (type 4), three by three: 18 bytes, padded to 20.
        let texture = RenderSurface {
            id: DataId(0x0500_3000),
            format: 26,
            image_size: 18,
            data_offset: 16,
            ..image.clone()
        };
        let px: Vec<u8> = (0..18u8).collect();
        let b = texture.encode_texture_for(OLDER, &px).unwrap();
        assert_eq!(&b[4..8], &4u32.to_le_bytes(), "the type of A4R4G4B4");
        assert_eq!(b.len(), 16 + 20);
        let back = RenderSurface::from_classic_texture(texture.id, &b).unwrap();
        assert_eq!(back, texture);
        assert_eq!(back.payload(&b).unwrap(), px.as_slice());
        assert!(texture.encode_texture_for(EraId::Eor, &px).is_err());

        let later = SurfaceTexture {
            id: DataId(0x0500_3000),
            data_category: 0,
            unknown_byte: 2,
            source_levels: vec![DataId(0x0600_2000)],
        };
        assert!(later.encode_for(OLDER).is_err());
        assert_eq!(later.encode_for(EraId::Eor).unwrap(), later.encode());
        let argb = RenderSurface {
            format: 21,
            ..image
        };
        assert!(argb.encode_for(OLDER, &[0; 36]).is_err());
    }
}
