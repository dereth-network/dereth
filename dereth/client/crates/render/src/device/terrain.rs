//! Buffer words shared by terrain-composition shaders.

use super::{MergeSource, TerrainMergeJob};
use crate::RenderError;

/// The most overlays one composite may carry; the normal landscape uses at most five.
pub(crate) const MAX_OVERLAYS: usize = 8;
pub(crate) const JOB_WORDS: usize = 8 + 9 * MAX_OVERLAYS;

/// Header: size, output stride, base presence/offset/width/height/tiling, overlay count.
/// Each overlay: alpha offset/width/height/rotation, texture presence/offset/width/height/tiling.
/// Callers validate the overlay bound and retain their own output-row alignment arithmetic.
pub(crate) fn pack_job(
    job: &TerrainMergeJob,
    stride: u32,
    mut source: impl FnMut(MergeSource) -> Result<(u32, u32, u32), RenderError>,
) -> Result<Vec<u32>, RenderError> {
    let mut words = vec![0u32; JOB_WORDS];
    words[0] = job.size;
    words[1] = stride;
    if let Some(b) = job.base {
        let (off, w, h) = source(b)?;
        words[2..7].copy_from_slice(&[1, off, w, h, job.base_tiling]);
    }
    words[7] = u32::try_from(job.overlays.len()).expect("overlay count validated before packing");
    for (k, o) in job.overlays.iter().enumerate() {
        let (aoff, aw, ah) = source(o.alpha)?;
        let b = 8 + k * 9;
        words[b..b + 4].copy_from_slice(&[aoff, aw, ah, o.rotation]);
        if let Some(t) = o.tex {
            let (toff, tw, th) = source(t)?;
            words[b + 4..b + 9].copy_from_slice(&[1, toff, tw, th, o.tiling]);
        } else {
            words[b + 8] = o.tiling;
        }
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (buffer layout and source lookup order at the shader boundary).
    use super::*;
    use crate::device::TerrainMergeOverlay;

    #[test]
    fn explicit_stride_and_missing_sources_keep_the_shader_word_layout() {
        let mut job = TerrainMergeJob {
            size: 24,
            base: Some(MergeSource(1)),
            base_tiling: 3,
            overlays: vec![
                TerrainMergeOverlay {
                    alpha: MergeSource(2),
                    rotation: 1,
                    tex: Some(MergeSource(3)),
                    tiling: 4,
                },
                TerrainMergeOverlay {
                    alpha: MergeSource(4),
                    rotation: 3,
                    tex: None,
                    tiling: 7,
                },
            ],
        };
        let mut seen = Vec::new();
        let words = pack_job(&job, 64, |s| {
            seen.push(s.0);
            Ok((s.0 * 10, s.0 + 4, s.0 + 5))
        })
        .unwrap();
        assert_eq!(seen, [1, 2, 3, 4]);
        assert_eq!(
            &words[..26],
            &[24, 64, 1, 10, 5, 6, 3, 2, 20, 6, 7, 1, 1, 30, 7, 8, 4, 40, 8, 9, 3, 0, 0, 0, 0, 7]
        );
        assert!(words[26..].iter().all(|&w| w == 0));
        job.base = None;
        let words = pack_job(&job, 24, |s| Ok((s.0 * 10, s.0 + 4, s.0 + 5))).unwrap();
        assert_eq!(&words[..8], &[24, 24, 0, 0, 0, 0, 0, 2]);
        seen.clear();
        assert!(pack_job(&job, 24, |s| {
            seen.push(s.0);
            Err(RenderError::Unsupported("missing source"))
        })
        .is_err());
        assert_eq!(seen, [2]);
    }
}
