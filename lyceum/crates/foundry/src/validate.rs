//! The checks a world's records must pass before a client sees them.
//!
//! The retail client has no defenses against a record that decodes badly or names something that
//! is not there, so every build, every bake and anyone who asks ([`validate`]) runs these:
//!
//! 1. **Decode:** every record decodes with Dereth's own decoders in the layout of its era's
//!    files, the cursor on its end, and the kinds a world builder writes encode back to the same
//!    bytes in that layout.
//! 2. **References:** a landblock's terrain says it has a statics record exactly when it has one;
//!    every static and building names a model (a GfxObj or a Setup) the files hold; a statics
//!    record's cell count names cells its landblock holds, and each building portal and stab
//!    list names one of them; a surface's texture and palette are there; an image
//!    texture's images are pictures the files hold; a paletted picture's palette is there.
//!
//! And one warning, which stops nothing: a static the overlay places or moves that stands inside a
//! building ([`crate::footprint`]), drawn in its walls and in the way of whoever walks in. The
//! base's own statics are never warned of: retail stands 1,658 of them inside its buildings on
//! purpose.
//!
//! The checks are calibrated on retail: over the end-of-retail files and over Infiltration's
//! (February 2005) with no overlay every check passes, so a check that fails names a fault in the
//! records, not in the check.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{CellLandblock, LandblockInfo, ObjectEntry};
use dereth_assets::{decode_any_in, AssetError, Decode, RenderSurface, Surface, SurfaceTexture};
use dereth_dat::{classify_cell_id, divine_type, ContainerEra, DbType, ModernDat};
use dereth_primitives::DataId;

use crate::compile::Compiled;
use crate::dats::DatSet;
use crate::footprint::{statics_inside, Environments, Footprints};

/// Which records to check.
#[derive(Clone, Debug)]
pub enum Scope {
    /// Every record of every kind the checks know, in the portal and cell files.
    All,
    /// These records of each file. A deleted one is not checked itself, but the deletions put
    /// every record that could name it in scope.
    Only(BTreeMap<ModernDat, BTreeSet<u32>>),
}

impl Scope {
    /// The records a compiled overlay holds; everything when it deletes anything.
    #[must_use]
    pub fn of(c: &Compiled) -> Self {
        if c.tombstones.values().any(|d| !d.is_empty()) {
            return Self::All;
        }
        Self::Only(
            c.records
                .iter()
                .map(|(t, r)| (*t, r.keys().copied().collect()))
                .collect(),
        )
    }
}

/// What the checks found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// How many records were checked.
    pub checked: usize,
    /// One line for each fault.
    pub problems: Vec<String>,
    /// One line for each thing that is allowed but probably not meant.
    pub warnings: Vec<String>,
}

/// The kinds the reference checks read, in the portal file.
const PORTAL_KINDS: [DbType; 3] = [
    DbType::Surface,
    DbType::SurfaceTexture,
    DbType::RenderSurface,
];

fn model(id: DataId) -> bool {
    matches!(id.raw() >> 24, 0x01 | 0x02)
}

struct Checker<'a> {
    set: &'a DatSet,
    report: Report,
    environments: Environments,
}

impl Checker<'_> {
    fn problem(&mut self, id: u32, why: impl std::fmt::Display) {
        self.report.problems.push(format!("{id:#010X}: {why}"));
    }

    fn warning(&mut self, id: u32, why: impl std::fmt::Display) {
        self.report.warnings.push(format!("{id:#010X}: {why}"));
    }

    fn has_portal(&self, id: DataId) -> bool {
        self.set.has_portal(id)
    }

    fn landblock(&mut self, id: u32, bytes: &[u8]) {
        let era = self.set.era;
        let lb = match CellLandblock::decode_payload_for(era, DataId(id), bytes) {
            Ok(lb) => lb,
            Err(e) => return self.problem(id, format!("does not decode: {e}")),
        };
        if lb.encode_for(era).ok().as_deref() != Some(bytes) {
            self.problem(id, "does not encode back to its own bytes");
        }
        let statics = DataId((id & 0xFFFF_0000) | 0xFFFE);
        let has = self.set.cell.contains(statics);
        // The files from before Throne of Destiny ship a few landblocks with statics their
        // terrain does not name, which the client does not draw: the files' own, and no fault of
        // a world's.
        let shipped = self.set.layout() == ContainerEra::Classic
            && has
            && lb.lbi_exists == 0
            && !self
                .set
                .cell
                .layer()
                .is_some_and(|l| l.touches(DataId(id)) || l.touches(statics));
        if (lb.lbi_exists != 0) != has && !shipped {
            self.problem(
                id,
                if has {
                    "has a statics record, and its terrain says it has none"
                } else {
                    "says it has a statics record, and it has none"
                },
            );
        }
    }

    fn landblock_info(&mut self, id: u32, bytes: &[u8]) {
        let era = self.set.era;
        let info = match LandblockInfo::decode_payload_for(era, DataId(id), bytes) {
            Ok(i) => i,
            Err(e) => return self.problem(id, format!("does not decode: {e}")),
        };
        if info.encode_for(era).ok().as_deref() != Some(bytes) {
            self.problem(id, "does not encode back to its own bytes");
        }
        let terrain = DataId(id | 0xFFFF);
        if !self.set.cell.contains(terrain) {
            self.problem(id, "has no terrain record");
        }
        for (i, o) in info.objects.iter().enumerate() {
            if !model(o.id) || !self.has_portal(o.id) {
                self.problem(
                    id,
                    format!(
                        "static {i} names {:#010X}, not a model the files hold",
                        o.id.raw()
                    ),
                );
            }
        }
        // The cells the count names, from 0x0100 on, are all there. Retail holds cells past the
        // count in some landblocks, which nothing that walks the count reaches.
        let cells = info.num_cells;
        if let Some(missing) = (0..cells)
            .map(|k| (id & 0xFFFF_0000) | (0x0100 + k))
            .find(|c| !self.set.cell.contains(DataId(*c)))
        {
            self.problem(
                id,
                format!("says {cells} cells, and cell {missing:#010X} is not there"),
            );
        }
        let in_range = |c: u16| c >= 0x0100 && u32::from(c) < 0x0100 + cells;
        for (b, bld) in info.buildings.iter().enumerate() {
            if !model(bld.id) || !self.has_portal(bld.id) {
                self.problem(
                    id,
                    format!(
                        "building {b} names {:#010X}, not a model the files hold",
                        bld.id.raw()
                    ),
                );
            }
            for (p, portal) in bld.portals.iter().enumerate() {
                if !in_range(portal.other_cell_id) {
                    self.problem(
                        id,
                        format!(
                            "building {b} portal {p} leads to cell {:#06X}, not one of its {cells}",
                            portal.other_cell_id
                        ),
                    );
                }
                if let Some(s) = portal.stab_list.iter().find(|s| !in_range(**s)) {
                    self.problem(
                        id,
                        format!(
                            "building {b} portal {p} sees cell {s:#06X}, not one of its {cells}"
                        ),
                    );
                }
            }
        }
        self.footprints(id, &info);
    }

    /// Warn of every static that stands inside one of its landblock's buildings and that the
    /// files themselves do not put there: the base's own statics are the world's, and the base
    /// puts many inside its buildings on purpose (lights, furniture, a ring of stones in a
    /// courtyard).
    fn footprints(&mut self, id: u32, info: &LandblockInfo) {
        if info.buildings.is_empty() || info.objects.is_empty() {
            return;
        }
        let base: Vec<ObjectEntry> = self
            .set
            .cell
            .base()
            .read(DataId(id))
            .ok()
            .and_then(|b| self.set.decode::<LandblockInfo>(DataId(id), &b).ok())
            .map(|i| i.objects)
            .unwrap_or_default();
        let changed = |o: &ObjectEntry| !base.contains(o);
        if !info.objects.iter().any(changed) {
            return;
        }
        #[allow(clippy::cast_possible_truncation)] // the landblock is the id's top half
        let landblock = (id >> 16) as u16;
        let footprints = Footprints::of(self.set, landblock, info, &mut self.environments);
        for (s, b) in statics_inside(info, &footprints) {
            let o = info.objects[s];
            if !changed(&o) {
                continue;
            }
            self.warning(
                id,
                format!(
                    "static {s} ({:#010X} at {:.1}, {:.1}, {:.1}) stands inside building {b} ({:#010X})",
                    o.id.raw(),
                    o.frame.origin.x,
                    o.frame.origin.y,
                    o.frame.origin.z,
                    info.buildings[b].id.raw()
                ),
            );
        }
    }

    fn surface(&mut self, id: u32, bytes: &[u8]) {
        let era = self.set.era;
        let s = match Surface::decode_payload_for(era, DataId(id), bytes) {
            Ok(s) => s,
            Err(e) => return self.problem(id, format!("does not decode: {e}")),
        };
        if s.encode_for(era, DataId(id)).ok().as_deref() != Some(bytes) {
            self.problem(id, "does not encode back to its own bytes");
        }
        if let Some(t) = s.orig_texture_id {
            if !matches!(t.raw() >> 24, 0x05..=0x07) || !self.has_portal(t) {
                self.problem(
                    id,
                    format!(
                        "names texture {:#010X}, which the files do not hold",
                        t.raw()
                    ),
                );
            }
        }
        if let Some(p) = s.orig_palette_id.filter(|p| p.raw() != 0) {
            if !self.has_portal(p) {
                self.problem(
                    id,
                    format!(
                        "names palette {:#010X}, which the files do not hold",
                        p.raw()
                    ),
                );
            }
        }
    }

    fn surface_texture(&mut self, id: u32, bytes: &[u8]) {
        if self.set.layout() == ContainerEra::Classic {
            return self.texture_holding(id, bytes);
        }
        let st = match SurfaceTexture::decode_payload(DataId(id), bytes) {
            Ok(s) => s,
            Err(e) => return self.problem(id, format!("does not decode: {e}")),
        };
        if st.encode() != bytes {
            self.problem(id, "does not encode back to its own bytes");
        }
        for l in &st.source_levels {
            if !matches!(l.raw() >> 24, 0x06 | 0x07) || !self.has_portal(*l) {
                self.problem(
                    id,
                    format!("names image {:#010X}, which the files do not hold", l.raw()),
                );
            }
        }
    }

    /// An image texture of the files whose textures hold their pixels: it reads as the picture
    /// it is, and a paletted one's palette is there.
    fn texture_holding(&mut self, id: u32, bytes: &[u8]) {
        let era = self.set.era;
        let rs = match RenderSurface::from_classic_texture(DataId(id), bytes) {
            Ok(r) => r,
            Err(e) => return self.problem(id, format!("does not decode: {e}")),
        };
        self.picture(id, bytes, &rs, |rs, px| rs.encode_texture_for(era, px));
    }

    fn render_surface(&mut self, id: u32, bytes: &[u8]) {
        let era = self.set.era;
        let rs = match RenderSurface::decode_payload_for(era, DataId(id), bytes) {
            Ok(r) => r,
            Err(e) => return self.problem(id, format!("does not decode: {e}")),
        };
        self.picture(id, bytes, &rs, |rs, px| rs.encode_for(era, px));
    }

    /// A picture's pixels encode back to its record, and its palette is there.
    fn picture(
        &mut self,
        id: u32,
        bytes: &[u8],
        rs: &RenderSurface,
        encode: impl Fn(&RenderSurface, &[u8]) -> Result<Vec<u8>, AssetError>,
    ) {
        match rs.payload(bytes) {
            Some(px) if encode(rs, px).ok().as_deref() == Some(bytes) => {}
            _ => self.problem(id, "does not encode back to its own bytes"),
        }
        if let Some(p) = rs.default_palette_id {
            if !self.has_portal(p) {
                self.problem(
                    id,
                    format!(
                        "names palette {:#010X}, which the files do not hold",
                        p.raw()
                    ),
                );
            }
        }
    }

    /// Any other record: it decodes in its era's layout, when Dereth has a decoder for its kind.
    fn other(&mut self, id: u32, kind: Option<DbType>, bytes: &[u8]) {
        let Some(kind) = kind else { return };
        match decode_any_in(self.set.layout(), kind, DataId(id), bytes) {
            Ok(_) | Err(AssetError::NoDecoder(_)) => {}
            Err(e) => self.problem(id, format!("does not decode as {kind:?}: {e}")),
        }
    }

    fn check(&mut self, t: ModernDat, id: u32) {
        let file = match t {
            ModernDat::Portal | ModernDat::HighRes => self
                .set
                .portal_holder(DataId(id))
                .and_then(|h| self.set.file(h)),
            other => self.set.file(other),
        };
        let Some(bytes) = file.and_then(|f| f.read(DataId(id)).ok()) else {
            // A deleted record: nothing of its own to check.
            return;
        };
        self.report.checked += 1;
        let kind = match t {
            ModernDat::Cell => classify_cell_id(DataId(id)),
            _ => divine_type(DataId(id)),
        };
        match kind {
            Some(DbType::LandBlock) => self.landblock(id, &bytes),
            Some(DbType::Lbi) => self.landblock_info(id, &bytes),
            Some(DbType::Surface) => self.surface(id, &bytes),
            Some(DbType::SurfaceTexture) => self.surface_texture(id, &bytes),
            Some(DbType::RenderSurface) => self.render_surface(id, &bytes),
            other => self.other(id, other, &bytes),
        }
    }
}

/// Check the records `scope` names in `set` (the files as the world reads them, an overlay over
/// them or not).
#[must_use]
pub fn validate(set: &DatSet, scope: &Scope) -> Report {
    let mut c = Checker {
        set,
        report: Report::default(),
        environments: Environments::default(),
    };
    match scope {
        Scope::All => {
            let portal: Vec<u32> = [Some(&set.portal), set.highres.as_ref()]
                .into_iter()
                .flatten()
                .flat_map(|f| f.iter_ids().map(DataId::raw).collect::<Vec<_>>())
                .filter(|id| divine_type(DataId(*id)).is_some_and(|k| PORTAL_KINDS.contains(&k)))
                .collect();
            for id in portal {
                c.check(ModernDat::Portal, id);
            }
            let cell: Vec<u32> = set
                .cell
                .iter_ids()
                .filter(|id| matches!(classify_cell_id(*id), Some(DbType::LandBlock | DbType::Lbi)))
                .map(DataId::raw)
                .collect();
            for id in cell {
                c.check(ModernDat::Cell, id);
            }
        }
        Scope::Only(ids) => {
            for (t, ids) in ids {
                for id in ids {
                    c.check(*t, *id);
                }
            }
        }
    }
    c.report
}
