//! The converted world's region record: AC1's own, with its land definitions' grid, road width
//! and height table set in place (the same sizes, so nothing else in the record moves).

/// AC1's land definitions start with these five words in every retail region: 255 blocks by 255,
/// 24 m cells, 8 cells a block side, 1 vertex step a cell.
const RETAIL_HEAD: [u32; 5] = [255, 255, 0x41C0_0000, 8, 1];

/// Where the land definitions start: the one place the retail head is found in the record's
/// first 256 bytes.
///
/// # Errors
/// When the head is not found, or found more than once.
pub fn land_defs_offset(record: &[u8]) -> Result<usize, String> {
    let head: Vec<u8> = RETAIL_HEAD.iter().flat_map(|w| w.to_le_bytes()).collect();
    let window = &record[..record.len().min(256)];
    let hits: Vec<usize> = window
        .windows(head.len())
        .enumerate()
        .filter(|(_, w)| *w == head.as_slice())
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [one] => Ok(*one),
        [] => Err("the region record has no retail land definitions".into()),
        _ => Err("the region record's land definitions are ambiguous".into()),
    }
}

/// The region record with its land definitions' vertex steps, road width and height table
/// replaced. Offsets from the definitions' start: blocks long and wide, cell length, cells a
/// block, vertex steps (16), the greatest object height, the sky height, the road width (28),
/// then the 256 heights (32).
///
/// # Errors
/// As [`land_defs_offset`].
pub fn patch(
    record: &[u8],
    steps: u32,
    road_width: f32,
    table: &[f32; 256],
) -> Result<Vec<u8>, String> {
    let at = land_defs_offset(record)?;
    if record.len() < at + 32 + 1024 {
        return Err("the region record is too short for its land definitions".into());
    }
    let mut out = record.to_vec();
    out[at + 16..at + 20].copy_from_slice(&steps.to_le_bytes());
    out[at + 28..at + 32].copy_from_slice(&road_width.to_le_bytes());
    for (k, h) in table.iter().enumerate() {
        let o = at + 32 + 4 * k;
        out[o..o + 4].copy_from_slice(&h.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's region)
    use super::*;

    /// A record shaped as a region's start: id, number, version, a name, then land definitions,
    /// then more.
    fn record() -> Vec<u8> {
        let mut r = Vec::new();
        for w in [0x1300_0000u32, 1, 3] {
            r.extend_from_slice(&w.to_le_bytes());
        }
        r.extend_from_slice(&[6, 0]);
        r.extend_from_slice(b"Dereth");
        for w in RETAIL_HEAD {
            r.extend_from_slice(&w.to_le_bytes());
        }
        for f in [200.0f32, 1000.0, 5.0] {
            r.extend_from_slice(&f.to_le_bytes());
        }
        for k in 0..256 {
            r.extend_from_slice(&(2.0 * k as f32).to_le_bytes());
        }
        r.extend_from_slice(&[0xAB; 40]);
        r
    }

    #[test]
    fn patching_changes_only_the_steps_the_road_width_and_the_heights() {
        let before = record();
        let t: [f32; 256] = std::array::from_fn(|k| 0.5 * k as f32);
        let after = patch(&before, 3, 4.0, &t).unwrap();
        assert_eq!(after.len(), before.len());
        let at = land_defs_offset(&before).unwrap();
        assert_eq!(at, 20);
        for (i, (a, b)) in before.iter().zip(&after).enumerate() {
            let inside = (at + 16..at + 20).contains(&i) || (at + 28..at + 32 + 1024).contains(&i);
            if !inside {
                assert_eq!(a, b, "byte {i} moved");
            }
        }
        assert_eq!(after[at + 16..at + 20], 3u32.to_le_bytes());
        assert_eq!(after[at + 28..at + 32], 4.0f32.to_le_bytes());
        assert_eq!(
            after[at + 32 + 4 * 10..at + 36 + 4 * 10],
            5.0f32.to_le_bytes()
        );
    }

    #[test]
    fn a_record_without_retail_land_definitions_is_refused() {
        let mut r = record();
        r[20] = 254;
        assert!(patch(&r, 3, 4.0, &[0.0; 256]).is_err());
    }
}
