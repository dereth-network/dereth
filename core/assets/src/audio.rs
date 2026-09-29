//! `Wave` and `SoundTable`.
//!
//! Sound-table records are described in `docs/formats/21-sound-tables.md`. The reference wave and
//! sound-table parsers decode all 786 waves and all 190
//! sound tables with zero trailing bytes.
//!
//! No audio decoding happens here — the mixer and the wave decode belong to `dereth-audio`. This
//! produces the `WAVEFORMATEX` header fields and the sample payload as a byte range.

use dereth_dat::{packobj::read_n, Cursor, DbType};
use dereth_primitives::DataId;

use crate::error::AssetError;
use crate::Decode;

/// `WAVE_FORMAT_PCM`. 785 of the 786 shipped waves.
pub const WAVE_FORMAT_PCM: u16 = 1;
/// `WAVE_FORMAT_MPEGLAYER3`. Exactly one shipped wave uses it.
pub const WAVE_FORMAT_MPEGLAYER3: u16 = 0x0055;

/// The `WAVEFORMATEX` fields, when the header is long enough to hold them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    /// Present when the header is at least 18 bytes.
    pub cb_size: Option<u16>,
}

/// A decoded wave file: its `WAVEFORMATEX` header and its sample payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    pub id: DataId,
    pub header_size: u32,
    pub data_size: u32,
    /// The raw `WAVEFORMATEX` bytes, exactly as stored.
    pub header: Vec<u8>,
    /// Offset of the sample payload within the record.
    pub data_offset: usize,
    /// Parsed header, when `header_size >= 16`.
    pub format: Option<WaveFormat>,
}

impl Wave {
    /// The sample payload, as a slice of the record this was decoded from.
    #[must_use]
    pub fn payload<'a>(&self, record: &'a [u8]) -> Option<&'a [u8]> {
        record.get(self.data_offset..self.data_offset + self.data_size as usize)
    }
}

impl Decode for Wave {
    const TYPE: DbType = DbType::Wave;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let header_size = c.u32()?;
        let data_size = c.u32()?;
        let header = c.bytes(header_size as usize)?.to_vec();
        let format = if header.len() >= 16 {
            let w16 = |i: usize| u16::from_le_bytes([header[i], header[i + 1]]);
            let w32 = |i: usize| {
                u32::from_le_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]])
            };
            Some(WaveFormat {
                format_tag: w16(0),
                channels: w16(2),
                samples_per_sec: w32(4),
                avg_bytes_per_sec: w32(8),
                block_align: w16(12),
                bits_per_sample: w16(14),
                cb_size: if header.len() >= 18 {
                    Some(w16(16))
                } else {
                    None
                },
            })
        } else {
            None
        };
        let data_offset = c.position();
        c.skip(data_size as usize)?;
        Ok(Self {
            id,
            header_size,
            data_size,
            header,
            data_offset,
            format,
        })
    }
}

/// One node of a sound table, flattened into an arena so that a deep tree cannot exhaust
/// the stack. Node 0 is the root.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundTableNode {
    pub key: u32,
    pub data: Vec<SoundEntry>,
    /// Indices into [`SoundTable::nodes`].
    pub children: Vec<u32>,
}

/// One row of a sound-table node.
///
/// The audio crate owns the shipped bug that makes the **last row of any multi-row entry
/// unreachable**; this decoder keeps every row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundEntry {
    pub sound_id: DataId,
    pub priority: f32,
    pub probability: f32,
    pub volume: f32,
}

/// A decoded sound table: the node tree that picks a wave for a sound type.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundTable {
    pub id: DataId,
    pub nodes: Vec<SoundTableNode>,
}

impl Decode for SoundTable {
    const TYPE: DbType = DbType::STable;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let mut nodes: Vec<SoundTableNode> = Vec::new();
        // Work list rather than recursion; the parent slot to record the child index in travels
        // with the task.
        let mut stack: Vec<Option<usize>> = vec![None];
        while let Some(parent) = stack.pop() {
            let idx = nodes.len();
            let key = c.u32()?;
            let n = c.u32()? as usize;
            let data = read_n(c, n, |c| {
                Ok(SoundEntry {
                    sound_id: c.data_id()?,
                    priority: c.f32()?,
                    probability: c.f32()?,
                    volume: c.f32()?,
                })
            })?;
            let num_children = c.u32()? as usize;
            nodes.push(SoundTableNode {
                key,
                data,
                children: Vec::new(),
            });
            if let Some(p) = parent {
                nodes[p].children.push(u32::try_from(idx).unwrap_or(0));
            }
            // Depth-first pre-order: each pending slot is filled by whatever the cursor reads
            // next, and a node's own children are pushed above its parent's remaining siblings, so
            // the traversal follows the file exactly.
            for _ in 0..num_children {
                stack.push(Some(idx));
            }
        }
        c.align_ptr();
        Ok(Self { id, nodes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: an independent reader of the shipped dats, which reads this table depth-first and
    /// recursively.
    /// The arena version must produce the same traversal order.
    #[test]
    fn sound_table_children_are_read_depth_first_in_file_order() {
        // root(key 1) with two children, the first of which has one child.
        let mut b = Vec::new();
        b.extend_from_slice(&0x2000_0001u32.to_le_bytes()); // id
        let node = |key: u32, kids: u32, out: &mut Vec<u8>| {
            out.extend_from_slice(&key.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // no rows
            out.extend_from_slice(&kids.to_le_bytes());
        };
        node(1, 2, &mut b);
        node(2, 1, &mut b);
        node(3, 0, &mut b);
        node(4, 0, &mut b);
        let t = SoundTable::decode_payload(DataId(0x2000_0001), &b).unwrap();
        let keys: Vec<u32> = t.nodes.iter().map(|n| n.key).collect();
        assert_eq!(
            keys,
            vec![1, 2, 3, 4],
            "arena order must be the file's depth-first order"
        );
        assert_eq!(t.nodes[0].children, vec![1, 3]);
        assert_eq!(t.nodes[1].children, vec![2]);
        assert!(t.nodes[2].children.is_empty());
    }
}
