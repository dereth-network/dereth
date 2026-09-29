//! `PFID_CUSTOM_RAW_JPEG` decoding.
//!
//! JPEG decoding preserves the legacy library's baseline and progressive behavior.
//!
//! The client's JPEG loader, "verified", reads the header, **fails unless**
//! `props.JPGChannels == 3`**, records the format as `PFID_R8G8B8`, and then decodes with
//! `DIBChannels = 3`, `DIBColor = IJL_BGR (2)`, `JPGColor = IJL_YCBCR (3)`. So the decoded pixels
//! land as **24-bit BGR**, which is exactly `PFID_R8G8B8`'s little-endian byte order
//! (`R = 0x00FF0000, G = 0x0000FF00, B = 0x000000FF`).
//!
//! # The corpus
//!
//! There are **79** raw-JPEG records in `client_portal.dat`, and **six of them are progressive**
//! (`060066AB`, `060066CE`, `06006701`, `060067CD`, `060067F2`, `0600681C`). A baseline-only decoder
//! passes 73 images and silently fails 6. `zune-jpeg` handles progressive; the corpus
//! test is what verifies it rather than assuming it.
//!
//! # Fidelity
//!
//! UNVERIFIED: IJL folds CPUID into its function-table selection, so the
//! *reference* decode of this corpus is CPU-dependent (which IDCT and colour converter ran). The
//! comparison tolerance is therefore a named constant, [`MAX_CHANNEL_DELTA`], and any fixture must
//! record the capture CPU in a sidecar. The mean-delta rule below is the part that is *not*
//! tolerance: a non-zero mean means a wrong YCbCr matrix, level shift or chroma upsample, which is a
//! bug rather than rounding.

use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::RenderError;

/// The per-channel tolerance the JPEG parity harness allows against the IJL reference.
///
/// The rule is "max per-channel Δ ≤ 1 **and mean Δ = 0** per channel". The `1` is
/// rounding slack for the IDCT; the mean is not slack at all.
/// UNVERIFIED: which IJL function table produced the reference.
pub const MAX_CHANNEL_DELTA: u8 = 1;

/// A decoded raw-JPEG surface: 24-bit BGR, tightly packed, top row first.
#[derive(Debug, Clone)]
pub struct DecodedJpeg {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` bytes, B then G then R per pixel — `PFID_R8G8B8`'s memory order.
    pub bgr: Vec<u8>,
}

/// Decode a `PFID_CUSTOM_RAW_JPEG` payload the way the retail texture loader does.
///
/// The three-channel check is the client's own: anything but three channels fails. A greyscale or
/// CMYK JPEG is rejected rather than converted, because the client rejects it.
pub fn decode(bytes: &[u8]) -> Result<DecodedJpeg, RenderError> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::BGR);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder
        .decode_headers()
        .map_err(|e| RenderError::Jpeg(format!("header: {e:?}")))?;

    // `if (props.JPGChannels != 3) fail` -- only 3-channel JPEG is accepted.
    let input = decoder
        .input_colorspace()
        .ok_or_else(|| RenderError::Jpeg("no colourspace after header decode".into()))?;
    if input.num_components() != 3 {
        return Err(RenderError::Jpeg(format!(
            "JPGChannels == {}; the client accepts only 3",
            input.num_components()
        )));
    }

    let bgr = decoder
        .decode()
        .map_err(|e| RenderError::Jpeg(format!("{e:?}")))?;
    let (w, h) = decoder
        .dimensions()
        .ok_or_else(|| RenderError::Jpeg("no dimensions after decode".into()))?;
    let width = u32::try_from(w).map_err(|_| RenderError::Jpeg("width overflow".into()))?;
    let height = u32::try_from(h).map_err(|_| RenderError::Jpeg("height overflow".into()))?;
    let expected = w * h * 3;
    if bgr.len() != expected {
        return Err(RenderError::Jpeg(format!(
            "decoder produced {} bytes; {expected} expected for {w}x{h} BGR",
            bgr.len()
        )));
    }
    Ok(DecodedJpeg { width, height, bgr })
}

/// Read the SOF marker to say whether a stream is progressive, without decoding it.
///
/// This is what makes the progressive case testable: the six progressive records must be *identified*, not
/// merely survive. `SOF0`/`SOF1` are baseline/extended-sequential; `SOF2` is progressive.
#[must_use]
pub fn is_progressive(bytes: &[u8]) -> Option<bool> {
    let mut i = 2usize; // skip SOI
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        // Padding fill bytes and the standalone markers carry no length.
        if marker == 0xFF {
            i += 1;
            continue;
        }
        match marker {
            0xC0 | 0xC1 => return Some(false),
            0xC2 => return Some(true),
            0xD8 | 0x01 | 0xD0..=0xD7 => i += 2,
            0xD9 | 0xDA => return None, // end of image, or entropy data before any SOF
            _ => {
                let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
                if len < 2 {
                    return None;
                }
                i += 2 + len;
            }
        }
    }
    None
}

/// Compare a decode against a reference under the parity rule: max per-channel Δ ≤
/// [`MAX_CHANNEL_DELTA`] **and mean Δ exactly 0** per channel.
///
/// Returns `Ok(())` or a description of the first rule broken. Both buffers are BGR.
///
/// # Errors
/// Returns a message naming the channel and the statistic that failed.
pub fn compare_to_reference(decoded: &[u8], reference: &[u8]) -> Result<(), String> {
    if decoded.len() != reference.len() {
        return Err(format!(
            "length {} != reference {}",
            decoded.len(),
            reference.len()
        ));
    }
    let mut sums = [0i64; 3];
    let mut maxes = [0u8; 3];
    for (i, (a, b)) in decoded.iter().zip(reference).enumerate() {
        let c = i % 3;
        let d = i32::from(*a) - i32::from(*b);
        sums[c] += i64::from(d);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: |d| of two u8 values is at most 255.
        let mag = d.unsigned_abs() as u8;
        maxes[c] = maxes[c].max(mag);
    }
    for c in 0..3 {
        if maxes[c] > MAX_CHANNEL_DELTA {
            return Err(format!(
                "channel {c}: max delta {} > {MAX_CHANNEL_DELTA}",
                maxes[c]
            ));
        }
        // A non-zero *mean* means a wrong YCbCr matrix, level shift or chroma upsample -- a bug,
        // not rounding. The sum is the mean times a positive constant, so testing the sum is the
        // same test without a division.
        if sums[c] != 0 {
            return Err(format!(
                "channel {c}: mean delta is not zero (sum {})",
                sums[c]
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A baseline 8x8 JPEG of solid pure red, 4:4:4 (632 bytes), produced once by a standard baseline
    /// encoder and pasted here so the test needs no build step. The *content* is what it
    /// proves: a known solid colour makes the decoded byte order observable.
    pub(crate) const RED_8X8_BASELINE: &str = concat!(
        "FFD8FFE000104A46494600010100000100010000FFDB00430002010101010102010101020202020204030202",
        "02020504040304060506060605060606070908060709070606080B08090A0A0A0A0A06080B0C0B0A0C090A0A",
        "0AFFDB004301020202020202050303050A0706070A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A",
        "0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0AFFC000110800080008030111000211010311",
        "01FFC4001F0000010501010101010100000000000000000102030405060708090A0BFFC400B5100002010303",
        "020403050504040000017D01020300041105122131410613516107227114328191A1082342B1C11552D1F024",
        "33627282090A161718191A25262728292A3435363738393A434445464748494A535455565758595A63646566",
        "6768696A737475767778797A838485868788898A92939495969798999AA2A3A4A5A6A7A8A9AAB2B3B4B5B6B7",
        "B8B9BAC2C3C4C5C6C7C8C9CAD2D3D4D5D6D7D8D9DAE1E2E3E4E5E6E7E8E9EAF1F2F3F4F5F6F7F8F9FAFFC400",
        "1F0100030101010101010101010000000000000102030405060708090A0BFFC400B511000201020404030407",
        "05040400010277000102031104052131061241510761711322328108144291A1B1C109233352F0156272D10A",
        "162434E125F11718191A262728292A35363738393A434445464748494A535455565758595A63646566676869",
        "6A737475767778797A82838485868788898A92939495969798999AA2A3A4A5A6A7A8A9AAB2B3B4B5B6B7B8B9",
        "BAC2C3C4C5C6C7C8C9CAD2D3D4D5D6D7D8D9DAE2E3E4E5E6E7E8E9EAF2F3F4F5F6F7F8F9FAFFDA000C030100",
        "02110311003F00F8BEBF94CFF7F0FFD9",
    );

    /// The same image encoded progressively (SOF2) (521 bytes), produced once by a standard baseline
    /// encoder and pasted here so the test needs no build step. The *content* is what it
    /// proves: a known solid colour makes the decoded byte order observable.
    const RED_8X8_PROGRESSIVE: &str = concat!(
        "FFD8FFE000104A46494600010100000100010000FFDB00430002010101010102010101020202020204030202",
        "02020504040304060506060605060606070908060709070606080B08090A0A0A0A0A06080B0C0B0A0C090A0A",
        "0AFFDB004301020202020202050303050A0706070A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A",
        "0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0AFFC200110800080008030111000211010311",
        "01FFC40014000100000000000000000000000000000007FFC400150101010000000000000000000000000000",
        "0708FFDA000C030100021003100000011729BFBFFFC40014100100000000000000000000000000000000FFDA",
        "00080101000105027FFFC40014110100000000000000000000000000000000FFDA0008010301013F017FFFC4",
        "0014110100000000000000000000000000000000FFDA0008010201013F017FFFC40014100100000000000000",
        "000000000000000000FFDA0008010100063F027FFFC40014100100000000000000000000000000000000FFDA",
        "0008010100013F217FFFDA000C030100020003000000101FFFC4001411010000000000000000000000000000",
        "0000FFDA0008010301013F107FFFC40014110100000000000000000000000000000000FFDA0008010201013F",
        "107FFFC40014100100000000000000000000000000000000FFDA0008010100013F107FFFD9",
    );

    /// A 16x8 solid pure blue at 4:2:0, so chroma upsampling runs (635 bytes), produced once by a standard baseline
    /// encoder and pasted here so the test needs no build step. The *content* is what it
    /// proves: a known solid colour makes the decoded byte order observable.
    const BLUE_16X8_420: &str = concat!(
        "FFD8FFE000104A46494600010100000100010000FFDB00430002010101010102010101020202020204030202",
        "02020504040304060506060605060606070908060709070606080B08090A0A0A0A0A06080B0C0B0A0C090A0A",
        "0AFFDB004301020202020202050303050A0706070A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A",
        "0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0AFFC000110800080010030122000211010311",
        "01FFC4001F0000010501010101010100000000000000000102030405060708090A0BFFC400B5100002010303",
        "020403050504040000017D01020300041105122131410613516107227114328191A1082342B1C11552D1F024",
        "33627282090A161718191A25262728292A3435363738393A434445464748494A535455565758595A63646566",
        "6768696A737475767778797A838485868788898A92939495969798999AA2A3A4A5A6A7A8A9AAB2B3B4B5B6B7",
        "B8B9BAC2C3C4C5C6C7C8C9CAD2D3D4D5D6D7D8D9DAE1E2E3E4E5E6E7E8E9EAF1F2F3F4F5F6F7F8F9FAFFC400",
        "1F0100030101010101010101010000000000000102030405060708090A0BFFC400B511000201020404030407",
        "05040400010277000102031104052131061241510761711322328108144291A1B1C109233352F0156272D10A",
        "162434E125F11718191A262728292A35363738393A434445464748494A535455565758595A63646566676869",
        "6A737475767778797A82838485868788898A92939495969798999AA2A3A4A5A6A7A8A9AAB2B3B4B5B6B7B8B9",
        "BAC2C3C4C5C6C7C8C9CAD2D3D4D5D6D7D8D9DAE2E3E4E5E6E7E8E9EAF2F3F4F5F6F7F8F9FAFFDA000C030100",
        "02110311003F00FC73A28A2BFDFC3F2B3FFFD9",
    );

    /// A single-channel (greyscale) 8x8 JPEG (331 bytes), produced once by a standard baseline
    /// encoder and pasted here so the test needs no build step. The *content* is what it
    /// proves: a known solid colour makes the decoded byte order observable.
    const GREY_8X8: &str = concat!(
        "FFD8FFE000104A46494600010100000100010000FFDB00430002010101010102010101020202020204030202",
        "02020504040304060506060605060606070908060709070606080B08090A0A0A0A0A06080B0C0B0A0C090A0A",
        "0AFFC0000B080008000801011100FFC4001F0000010501010101010100000000000000000102030405060708",
        "090A0BFFC400B5100002010303020403050504040000017D0102030004110512213141061351610722711432",
        "8191A1082342B1C11552D1F02433627282090A161718191A25262728292A3435363738393A43444546474849",
        "4A535455565758595A636465666768696A737475767778797A838485868788898A92939495969798999AA2A3",
        "A4A5A6A7A8A9AAB2B3B4B5B6B7B8B9BAC2C3C4C5C6C7C8C9CAD2D3D4D5D6D7D8D9DAE1E2E3E4E5E6E7E8E9EA",
        "F1F2F3F4F5F6F7F8F9FAFFDA0008010100003F002BFFD9",
    );

    pub(crate) fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    // Oracle: the JPEG standard's SOF markers classify the corpus as 73 baseline plus 6
    // progressive"). Spec trap 2 makes identifying the progressive records the point of the test:
    // "A baseline-only decoder passes 73 images and silently fails 6."
    #[test]
    fn the_sof_marker_distinguishes_baseline_from_progressive() {
        assert_eq!(is_progressive(&bytes(RED_8X8_BASELINE)), Some(false));
        assert_eq!(is_progressive(&bytes(RED_8X8_PROGRESSIVE)), Some(true));
        assert_eq!(is_progressive(&bytes(GREY_8X8)), Some(false));
        // Not a JPEG at all, or truncated before any SOF.
        assert_eq!(is_progressive(&[0u8; 16]), None);
        assert_eq!(is_progressive(&[]), None);
        assert_eq!(is_progressive(&bytes(RED_8X8_BASELINE)[..8]), None);
    }

    // Oracle: spec trap 2 -- "Both zune-jpeg and jpeg-decoder handle progressive; *verify with the
    // corpus test rather than assuming*." The retail corpus is not checked in (fixtures/C is
    // empty), so this verifies the decoder against a progressive stream of known content, which is
    // the half of the claim that is about the decoder rather than about the corpus.
    #[test]
    fn the_decoder_handles_progressive_streams() {
        let baseline = decode(&bytes(RED_8X8_BASELINE)).expect("baseline must decode");
        let progressive = decode(&bytes(RED_8X8_PROGRESSIVE)).expect("progressive must decode");
        assert_eq!((baseline.width, baseline.height), (8, 8));
        assert_eq!((progressive.width, progressive.height), (8, 8));
        assert_eq!(baseline.bgr.len(), 8 * 8 * 3);
        // Both encode the same solid colour, so both must decode to it.
        for img in [&baseline, &progressive] {
            for px in img.bgr.as_chunks::<3>().0 {
                assert!(px[2] > 200, "red channel, got {px:?}");
                assert!(px[0] < 60 && px[1] < 60, "blue/green channels, got {px:?}");
            }
        }
    }

    // The observed JPEG decode requests 24-bit BGR, matching PFID_R8G8B8's little-endian order
    // (R = 0x00FF0000, G = 0x0000FF00, B = 0x000000FF). A solid red source makes the order
    // observable: red must be the *third* byte of each triple, not the first.
    #[test]
    fn the_output_byte_order_is_bgr_not_rgb() {
        let img = decode(&bytes(RED_8X8_BASELINE)).unwrap();
        let (b, g, r) = (img.bgr[0], img.bgr[1], img.bgr[2]);
        assert!(r > 200, "red is the third byte, got r={r} b={b}");
        assert!(b < 60, "blue is the first byte, got b={b}");
        assert!(g < 60, "green is the second byte, got g={g}");

        // A 4:2:0 blue image also exercises the chroma upsample, which is the other half of what a
        // wrong colour path would break.
        let img = decode(&bytes(BLUE_16X8_420)).unwrap();
        assert_eq!((img.width, img.height), (16, 8));
        let (b, g, r) = (img.bgr[0], img.bgr[1], img.bgr[2]);
        assert!(b > 200, "blue is the first byte, got b={b}");
        assert!(r < 60 && g < 60, "got r={r} g={g}");
    }

    // Oracle: the client's JPEG loader, "verified" -- "if (props.JPGChannels
    // != 3) fail // only 3-channel JPEG is accepted". A greyscale JPEG must be rejected, not
    // silently expanded, because the client rejects it.
    #[test]
    fn a_non_three_channel_jpeg_is_rejected_as_the_client_rejects_it() {
        let err = decode(&bytes(GREY_8X8)).unwrap_err();
        match err {
            RenderError::Jpeg(m) => assert!(m.contains("JPGChannels"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    // Oracle: the brief -- "Parsers return Result; they do not panic on malformed input, because
    // they will meet malformed input." All 79 real records go through this path.
    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[0xFF, 0xD8]).is_err());
        assert!(decode(b"not a jpeg at all").is_err());
        // Truncated part way through a valid stream.
        let full = bytes(RED_8X8_BASELINE);
        assert!(decode(&full[..full.len() / 2]).is_err());
        // A stream whose header parses but whose entropy data is corrupt: either outcome is
        // acceptable, a panic is not.
        let mut broken = full.clone();
        let n = broken.len();
        broken[n - 8..n - 2].fill(0xFF);
        let _ = decode(&broken);
    }

    // Oracle: the decoder's parity rule -- max per-channel delta <= 1 and mean delta = 0 per
    // channel (a non-zero mean means a wrong YCbCr matrix, level shift or chroma upsample -- a
    // bug, not rounding).
    #[test]
    fn the_parity_rule_accepts_rounding_and_rejects_a_bias() {
        let r = vec![10u8, 20, 30, 40, 50, 60];
        assert!(compare_to_reference(&r, &r).is_ok());

        // Symmetric +/-1 rounding on one channel: max delta 1, mean 0. Accepted.
        let d = vec![11u8, 20, 30, 39, 50, 60];
        assert!(
            compare_to_reference(&d, &r).is_ok(),
            "symmetric rounding must pass"
        );

        // A uniform +1 on the blue channel: max delta 1 but a non-zero mean. Rejected.
        let d = vec![11u8, 20, 30, 41, 50, 60];
        let e = compare_to_reference(&d, &r).unwrap_err();
        assert!(e.contains("mean delta is not zero"), "{e}");

        // A delta of 2 anywhere is rejected on the max rule.
        let d = vec![12u8, 20, 30, 38, 50, 60];
        let e = compare_to_reference(&d, &r).unwrap_err();
        assert!(e.contains("max delta"), "{e}");

        // Mismatched lengths are a hard failure, not a partial comparison.
        assert!(compare_to_reference(&r[..3], &r).is_err());
        assert_eq!(MAX_CHANNEL_DELTA, 1, "see open question #190");
    }
}
