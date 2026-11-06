//! Tables of ids and the records no client install holds: the id maps (`.imp`), the file-id table
//! (`0x0E000003`), the iteration list, places, file formats, the content and performance tables,
//! categories, obstacle descriptions and path maps.

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};
use auberean_primitives::DataId;

use crate::common::Position;
use crate::{undecoded, Context, Decode, Encode};

/// The data versions a file holds (`0xFFFF0001`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IterationList {
    pub versions: Vec<i32>,
}

impl Decode for IterationList {
    fn decode(r: &mut Reader<'_>, _id: u32, _ctx: &Context) -> Result<Self> {
        let all = r.rest();
        match auberean_dat::iteration::decode_signed(all) {
            Ok(versions) => {
                r.skip(all.len())?;
                Ok(Self { versions })
            }
            Err(_) => r.fail("not a well-formed iteration list"),
        }
    }
}

/// One pair of a [`PairMap`] map.
// D2-00876
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PairEntry {
    pub key: u32,
    pub value: u32,
}

/// The client's own reading of the iteration list's type (`0xFFFF0000`-`0xFFFFFFFE`): a count,
/// then that many pairs of words, with no id first. The files' iteration lists are not in this
/// form (their count is of versions, not of pairs), so the client cannot read one this way.
// D2-12754, D2-05082, D2-12756, D2-12751
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PairMap {
    pub map: Vec<PairEntry>,
}

impl PairMap {
    /// The count, then the pairs. Fails when fewer than four bytes are left, or fewer than eight
    /// per pair the count claims (the claim taken modulo 2^32).
    // D2-12757, D2-12758
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        if r.remaining() < 4 {
            return r.fail("no count");
        }
        let count = r.u32()?;
        if r.remaining() < count.wrapping_mul(8) as usize {
            return r.fail("fewer pairs than the count");
        }
        let mut map = Vec::new();
        for _ in 0..count {
            map.push(PairEntry {
                key: r.u32()?,
                value: r.u32()?,
            });
        }
        Ok(Self { map })
    }

    /// Write the map as [`PairMap::read`] reads it.
    // D2-12752, D2-12753
    pub fn write(&self, w: &mut Writer) {
        w.u32(u32::try_from(self.map.len()).unwrap_or(u32::MAX));
        for e in &self.map {
            w.u32(e.key);
            w.u32(e.value);
        }
    }
}

impl Decode for PairMap {
    fn decode(r: &mut Reader<'_>, _id: u32, _ctx: &Context) -> Result<Self> {
        Self::read(r)
    }
}

impl Encode for PairMap {
    fn encode(&self, _id: u32, w: &mut Writer) {
        self.write(w);
    }
}

/// An id map (`.imp`, `0x28000000`+): numbers of an enumeration mapped to record ids and to names,
/// once for the game's own use and once more for the tools' use.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct IdMap {
    /// The size places the four tables were stored with.
    pub buckets: [u8; 4],
    pub ids: Vec<(u32, DataId)>,
    pub names: Vec<(u32, String)>,
    pub tool_ids: Vec<(u32, DataId)>,
    pub tool_names: Vec<(u32, String)>,
}

impl IdMap {
    /// The records the map names: every record of its game half, and of its tools' half unless
    /// the gathering is for the region (`0x4000000B`).
    // D2-12711
    pub fn sub_ids(&self, gathering: u32, out: &mut auberean_dat::QualifiedIds) {
        for (_, d) in &self.ids {
            out.add(d.0);
        }
        if gathering != auberean_dat::RecordType::REGION.0 {
            for (_, d) in &self.tool_ids {
                out.add(d.0);
            }
        }
    }
}

// D2-05049, D2-12718, D2-12721, D2-12717, D2-12719, D2-12715
impl Decode for IdMap {
    /// The id, then four hash tables: number to record id, number to name (a narrow string), and
    /// the same two again.
    // D2-12704, D2-00124, D2-07333
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let ids =
            |r: &mut Reader<'_>| r.hash_table_with_buckets(8, |r| Ok((r.u32()?, r.data_id()?)));
        let names = |r: &mut Reader<'_>| {
            r.hash_table_with_buckets(8, |r| Ok((r.u32()?, r.narrow_string()?)))
        };
        let (b0, ids0) = ids(r)?;
        let (b1, names0) = names(r)?;
        let (b2, ids1) = ids(r)?;
        let (b3, names1) = names(r)?;
        Ok(Self {
            buckets: [b0, b1, b2, b3],
            ids: ids0,
            names: names0,
            tool_ids: ids1,
            tool_names: names1,
        })
    }
}

impl Encode for IdMap {
    // D2-12704, D2-00124, D2-07333
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        let ids = |w: &mut Writer, b: u8, v: &[(u32, DataId)]| {
            w.hash_table(b, v, |w, (k, d)| {
                w.u32(*k);
                w.data_id(*d);
            });
        };
        let names = |w: &mut Writer, b: u8, v: &[(u32, String)]| {
            w.hash_table(b, v, |w, (k, n)| {
                w.u32(*k);
                w.narrow_string(n);
            });
        };
        ids(w, self.buckets[0], &self.ids);
        names(w, self.buckets[1], &self.names);
        ids(w, self.buckets[2], &self.tool_ids);
        names(w, self.buckets[3], &self.tool_names);
    }
}

/// The file-id table (`0x0E000003`): per record type, the source names of its records.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileIdTable {
    pub types: Vec<FileIdType>,
}

/// One record type's names in the file-id table.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileIdType {
    /// The record type number.
    pub record_type: u32,
    pub name: String,
    pub entries: Vec<FileIdEntry>,
}

/// One record's names: the client keeps the second, by id.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileIdEntry {
    pub id: DataId,
    /// A name the client reads and drops (empty in every retail entry).
    pub unused: String,
    pub name: String,
}

impl Encode for FileIdTable {
    // D2-14673, D2-14672
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.list(&self.types, |w, t| {
            w.u32(t.record_type);
            w.narrow_string(&t.name);
            w.list(&t.entries, |w, e| {
                w.data_id(e.id);
                w.narrow_string(&e.unused);
                w.narrow_string(&e.name);
            });
        });
    }
}

// D2-05023, D2-05024, D2-05025, D2-05026
impl Decode for FileIdTable {
    /// The id, a counted list of types: a type number, a name, a counted list of entries (a record
    /// id and two narrow strings).
    // D2-14674
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let types = r.list(12, |r| {
            Ok(FileIdType {
                record_type: r.u32()?,
                name: r.narrow_string()?,
                entries: r.list(12, |r| {
                    Ok(FileIdEntry {
                        id: r.data_id()?,
                        unused: r.narrow_string()?,
                        name: r.narrow_string()?,
                    })
                })?,
            })
        })?;
        Ok(Self { types })
    }
}

/// The places table (`.places`, `0x0E000004`): named positions in the world. No install holds
/// one.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Places {
    pub places: Vec<(String, Position)>,
}

impl Places {
    /// The place of this name: the name in lower case (ASCII letters only, as the C library's
    /// default locale has it) is looked up as it stands, so a table's name must be in lower case
    /// to be found.
    // D2-13678
    #[must_use]
    pub fn place(&self, name: &str) -> Option<&Position> {
        let name = name.to_ascii_lowercase();
        self.places.iter().find(|(n, _)| *n == name).map(|(_, p)| p)
    }
}

// D2-04525, D2-04585, D2-04586, D2-13681
impl Decode for Places {
    /// The id, then a hash table of a wide-string name to a position.
    // D2-13674, D2-04502
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            places: r.hash_table(36, |r| Ok((r.wide_string()?, Position::read(r)?)))?,
        })
    }
}

/// A file format (`.fmt`, `0x27000000`+). No install holds one, and the client loads none: its
/// reading of this type always fails, so a format holds nothing and letting it go has nothing to
/// clear.
// D2-14542, D2-14546, D2-14547
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileFormat {
    _private: (),
}

// D2-05048, D2-14543, D2-14544
impl Decode for FileFormat {
    fn decode(r: &mut Reader<'_>, _id: u32, _ctx: &Context) -> Result<Self> {
        r.fail("the client reads no record of this type")
    }
}

undecoded!(
    /// A content table (`.cdb`, `0x52000000`-`0x527FFFFF`): a server type the client has no reader
    /// for, and no install holds one.
    ContentDb
);

/// A performance table (`.prf`, `0x52800000`+): the counters a process reports. No install holds
/// one.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Performance {
    pub counters: Vec<Counter>,
}

/// One performance counter: two words and two names.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Counter {
    pub index: u32,
    pub kind: u32,
    pub name: String,
    pub help: String,
}

impl Decode for Performance {
    /// The id, then a counted list of counters: two words and two narrow strings.
    // D2-16921, D2-16927, D2-16896
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let counters = r.list(12, |r| {
            Ok(Counter {
                index: r.u32()?,
                kind: r.u32()?,
                name: r.narrow_string()?,
                help: r.narrow_string()?,
            })
        })?;
        Ok(Self { counters })
    }
}

/// Categories (`.dbcategories`, `0x62000000`+): the record is its id alone. No install holds one.
///
/// In memory it maps names (compared without case) to category numbers; the record stores none
/// of them.
// D2-12680, D2-12682, D2-05079, D2-12681, D2-12684
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Categories {
    pub mapping: Vec<(String, u32)>,
}

impl Categories {
    /// Forget every name.
    // D2-12671
    pub fn clear(&mut self) {
        self.mapping.clear();
    }

    /// Give another record this one's names.
    // D2-12683
    pub fn copy_into(&self, other: &mut Categories) {
        other.mapping.clone_from(&self.mapping);
    }

    /// The category number of a name, compared without case.
    #[must_use]
    pub fn category(&self, name: &str) -> Option<u32> {
        self.mapping
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, c)| *c)
    }
}

impl Decode for Categories {
    // D2-12666
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self::default())
    }
}

undecoded!(
    /// An obstacle description (`.obstacledesc`, `0x38000000`+): a footprint, an optional path map,
    /// what lies beneath, links out, names and a scale. No install holds one; not decoded yet.
    Obstacles
);
undecoded!(
    /// A path map (cell files, `0xXXYYFFFD`). No install holds one; not decoded yet.
    PathMap
);

#[cfg(test)]
mod tests {
    //! Behaviour: none (decoders of record types no install holds, on synthetic records)
    use super::*;
    use auberean_primitives::scramble;

    fn words(ws: &[u32]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn narrow(s: &str) -> Vec<u8> {
        let mut b = u16::try_from(s.len()).unwrap().to_le_bytes().to_vec();
        b.extend_from_slice(&scramble::conceal(s.as_bytes()));
        b
    }

    fn pad(b: &mut Vec<u8>) {
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
    }

    #[test]
    fn a_performance_table_reads_its_counters() {
        let mut b = words(&[0x5280_0001, 1, 3, 4]);
        b.extend(narrow("fps"));
        pad(&mut b);
        b.extend(narrow("frames a second"));
        pad(&mut b);
        let p: Performance = crate::decode(0x5280_0001, &b, &Context::empty()).unwrap();
        assert_eq!(
            p.counters,
            vec![Counter {
                index: 3,
                kind: 4,
                name: "fps".into(),
                help: "frames a second".into()
            }]
        );
    }

    #[test]
    fn a_places_table_maps_names_to_positions() {
        let mut b = words(&[0x0E00_0004, 1]);
        let units: Vec<u8> = "Home".encode_utf16().flat_map(u16::to_le_bytes).collect();
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend(scramble::conceal(&units));
        pad(&mut b);
        b.extend(words(&[0x5D3A_0101]));
        for f in [1.0f32, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        let p: Places = crate::decode(0x0E00_0004, &b, &Context::empty()).unwrap();
        assert_eq!(p.places.len(), 1);
        assert_eq!(p.places[0].0, "Home");
        assert_eq!(p.places[0].1.cell.0, 0x5D3A_0101);
        // Only the name asked goes to lower case: a table's name with a capital is never found.
        assert!(p.place("Home").is_none());
        let lower = Places {
            places: vec![("home".to_owned(), p.places[0].1)],
        };
        assert_eq!(lower.place("hOME").map(|h| h.cell.0), Some(0x5D3A_0101));
        assert!(lower.place("away").is_none());
    }

    #[test]
    fn a_category_record_is_its_id_and_a_file_format_never_loads() {
        assert!(crate::decode::<Categories>(
            0x6200_0001,
            &words(&[0x6200_0001]),
            &Context::empty()
        )
        .is_ok());
        assert!(crate::decode::<FileFormat>(
            0x2700_0001,
            &words(&[0x2700_0001]),
            &Context::empty()
        )
        .is_err());
    }
}
