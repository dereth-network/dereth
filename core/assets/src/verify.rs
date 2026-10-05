//! The track's acceptance gate, as a library function so that the fixture generator and CI can
//! call it too.
//!
//! For every id in the dat set (all four files, or the two before Throne of Destiny, read in their
//! own record layouts), resolve the type, decode it, and require `expect_end()` — zero
//! shortfalls and zero overruns.

use std::collections::BTreeMap;

use dereth_dat::{
    classify_cell_id, divine_type_in, iteration, ContainerEra, DbType, RetailDatStore,
    ITERATION_LIST,
};
use dereth_primitives::DataId;

use crate::error::AssetError;
use crate::material::MaterialBlob;
use crate::ui::{LayoutDesc, PropertyAsset, PropertyTypes};
use crate::{decode_any_in, Decode, MasterProperty};

/// What one exhaustive pass found.
#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    /// Directory entries seen, including the four iteration lists.
    pub entries: usize,
    /// Objects decoded to completion with the cursor on the payload end.
    pub decoded: usize,
    /// Ids whose type resolved to something this crate does not decode.
    pub no_decoder: BTreeMap<DbType, usize>,
    /// Iteration lists read whole: each file of the later container has one; they have no type.
    pub iteration_lists: usize,
    /// Iteration lists that did not read, as `(id, message)`.
    pub iteration_failures: Vec<(DataId, String)>,
    /// Ids with no type at all.
    pub untyped: Vec<DataId>,
    /// Per-type decode counts.
    pub per_type: BTreeMap<DbType, usize>,
    /// Every failure, as `(id, type, message)`. The gate requires this to be empty.
    pub failures: Vec<(DataId, DbType, String)>,
}

impl VerifyReport {
    /// The gate: everything that has a decoder decoded, and nothing failed.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty() && self.no_decoder.is_empty() && self.iteration_failures.is_empty()
    }
}

/// The type an id resolves to for the exhaustive pass: portal and local ids by `DivineType` (with
/// the few ids only the older files type), cell ids by position, because the cell DAT carries no
/// type in the id.
fn resolve(era: ContainerEra, id: DataId, from_cell_dat: bool) -> Option<DbType> {
    if from_cell_dat {
        classify_cell_id(id)
    } else {
        divine_type_in(era, id)
    }
}

/// Decode every object in all four dats.
///
/// Two types need the `MasterProperty` type map to decode a property value at all — `UI_LAYOUT` and
/// `DBPROPERTIES` — exactly as the client does, so the map is read first and threaded through.
pub fn exhaustive_decode(s: &RetailDatStore) -> Result<VerifyReport, AssetError> {
    let mp_id = DataId(0x3900_0001);
    let types: PropertyTypes = if s.portal().contains(mp_id) {
        MasterProperty::decode_payload(mp_id, &s.read_portal(mp_id)?)?.property_types()
    } else {
        PropertyTypes::new()
    };

    let mut r = VerifyReport::default();
    let mut files: Vec<(&dereth_dat::DatFile, bool)> = vec![(s.portal(), false), (s.cell(), true)];
    // Before Throne of Destiny the language records are in the portal file, which the store also
    // answers language reads from: walked once.
    if s.era() == ContainerEra::Modern {
        files.push((s.local(), false));
    }
    if let Some(hi) = s.highres() {
        files.push((hi, false));
    }
    for (f, is_cell) in files {
        decode_file_into(&mut r, f, s.era(), is_cell, &types);
    }
    Ok(r)
}

/// Decode every object in one file, in the record layouts of `era`: the census of a single
/// file, for a file that is not part of a whole dat set. `is_cell` types ids by their position
/// in a landblock (the cell file's ids carry no type); `types` is the property-type map of the
/// portal file beside it, which the interface layouts and property records need.
#[must_use]
pub fn exhaustive_decode_file(
    f: &dereth_dat::DatFile,
    era: ContainerEra,
    is_cell: bool,
    types: &PropertyTypes,
) -> VerifyReport {
    let mut r = VerifyReport::default();
    decode_file_into(&mut r, f, era, is_cell, types);
    r
}

fn decode_file_into(
    r: &mut VerifyReport,
    f: &dereth_dat::DatFile,
    era: ContainerEra,
    is_cell: bool,
    types: &PropertyTypes,
) {
    for id in f.iter_ids() {
        r.entries += 1;
        if id == ITERATION_LIST {
            match f.read(id).map(|b| iteration::decode(&b)) {
                Ok(Ok(_)) => r.iteration_lists += 1,
                Ok(Err(e)) | Err(e) => r.iteration_failures.push((id, e.to_string())),
            }
            continue;
        }
        let Some(kind) = resolve(era, id, is_cell) else {
            r.untyped.push(id);
            continue;
        };
        let bytes = match f.read(id) {
            Ok(b) => b,
            Err(e) => {
                r.failures.push((id, kind, e.to_string()));
                continue;
            }
        };
        match decode_one(era, kind, id, &bytes, types) {
            Ok(true) => {
                r.decoded += 1;
                *r.per_type.entry(kind).or_insert(0) += 1;
            }
            Ok(false) => *r.no_decoder.entry(kind).or_insert(0) += 1,
            Err(e) => r.failures.push((id, kind, e.to_string())),
        }
    }
}

/// `Ok(true)` decoded, `Ok(false)` no decoder for that type, `Err` decode failure.
fn decode_one(
    era: ContainerEra,
    kind: DbType,
    id: DataId,
    bytes: &[u8],
    types: &PropertyTypes,
) -> Result<bool, AssetError> {
    match kind {
        DbType::UiLayout => {
            LayoutDesc::decode_payload(id, bytes, types)?;
            Ok(true)
        }
        DbType::DbProperties => {
            PropertyAsset::decode_payload(id, bytes, types)?;
            Ok(true)
        }
        // Open question #97: the unused engine-2 material system, three files, no layout worth
        // deriving. The id is checked and the body kept verbatim.
        DbType::RenderMaterial | DbType::MaterialModifier | DbType::MaterialInstance => {
            MaterialBlob::decode_payload(id, bytes)?;
            Ok(true)
        }
        _ => match decode_any_in(era, kind, id, bytes) {
            Ok(_) => Ok(true),
            Err(AssetError::NoDecoder(_)) => Ok(false),
            Err(e) => Err(e),
        },
    }
}
