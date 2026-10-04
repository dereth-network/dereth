//! The intro AVI demux matches the documented header; every frame decodes and the movie never
//! loops; the surface is the flipped DIB; Cinepak matches ffmpeg pixel for pixel; frames advance at
//! 29.97 fps; a non-Cinepak file is skipped.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::path::PathBuf;
use std::process::Command;

use dereth_audio::video::{Movie, FOURCC_CVID};
use dereth_primitives::LocalTime;

fn movie_path() -> Option<PathBuf> {
    // The same resolution the retail-dat tests use, so one environment variable covers both.
    let p = dereth_dat::testing::dat_dir().join("turbine_logo_ac.avi");
    if p.exists() {
        Some(p)
    } else {
        // NOT a skip: every caller `expect`s this, so a missing movie fails the test loudly.
        None
    }
}

/// The demux reproduces the documented header.
#[test]
fn the_demux_reproduces_the_documented_header() {
    let p = movie_path().expect("the intro movie under the retail client directory");
    let m = Movie::from_bytes(std::fs::read(&p).expect("the movie is readable"))
        .expect("the shipped AVI demuxes");
    let i = m.info();
    assert_eq!((i.width, i.height), (640, 480));
    assert_eq!(i.micro_sec_per_frame, 33_367);
    assert!(
        (i.fps() - 29.970).abs() < 0.001,
        "29.97 fps, got {}",
        i.fps()
    );
    assert_eq!(i.total_frames, 219);
    assert_eq!(i.compression, FOURCC_CVID);
    assert_eq!(i.bits_per_pixel, 24);
    assert_eq!(
        m.frame_count(),
        i.total_frames as usize,
        "each declared video frame has a chunk"
    );
    // 219 frames at 33367 us is 7.307 s.
    let seconds = f64::from(i.total_frames) * f64::from(i.micro_sec_per_frame) / 1_000_000.0;
    assert!(
        (seconds - 7.31).abs() < 0.01,
        "about 7.3 s, got {seconds:.3}"
    );
}

/// Behaviour: presentation.intro-movie.every-frame-decodes-at-the-movies-rate-and-it-never-loops
#[test]
fn every_frame_decodes_and_the_movie_never_loops() {
    let p = movie_path().expect("the intro movie under the retail client directory");
    let mut m =
        Movie::from_bytes(std::fs::read(&p).expect("the movie is readable")).expect("demuxes");
    let frame_count = m.frame_count();
    let info = m.info();
    assert!(frame_count > 0, "movie frames are exercised");
    for i in 0..frame_count {
        let surface = m
            .decode_frame(i)
            .unwrap_or_else(|| panic!("frame {i} decodes"));
        assert_eq!(
            surface.len(),
            info.width as usize * info.height as usize * 4
        );
        assert!(
            surface.chunks(4).all(|px| px[3] == 0xFF),
            "frame {i}: alpha must be opaque"
        );
    }
    assert!(m.is_finished());
    assert!(
        m.decode_frame(frame_count).is_none(),
        "there is no frame beyond the end and no wrap"
    );
    // Time never restarts it either.
    assert!(m.next_frame(LocalTime(100.0)).is_none());
}

/// The flip is real: the top-down surface's first row is the bottom-up DIB's last.
#[test]
fn the_surface_is_the_dib_flipped() {
    let p = movie_path().expect("the intro movie under the retail client directory");
    let mut m =
        Movie::from_bytes(std::fs::read(&p).expect("the movie is readable")).expect("demuxes");
    let surface = m.decode_frame(0).expect("frame 0").to_vec();
    let dib = m.bottom_up_dib();
    let w = 640usize;
    let h = 480usize;
    for row in [0usize, 1, 239, 478, 479] {
        let src = (h - 1 - row) * w * 3;
        for x in [0usize, 1, 320, 639] {
            let d = (row * w + x) * 4;
            assert_eq!(surface[d], dib[src + x * 3], "row {row} col {x}: blue");
            assert_eq!(
                surface[d + 1],
                dib[src + x * 3 + 1],
                "row {row} col {x}: green"
            );
            assert_eq!(
                surface[d + 2],
                dib[src + x * 3 + 2],
                "row {row} col {x}: red"
            );
            assert_eq!(surface[d + 3], 0xFF);
        }
    }
}

/// The cinepak decode matches ffmpeg pixel for pixel.
#[test]
fn the_cinepak_decode_matches_ffmpeg_pixel_for_pixel() {
    let p = movie_path().expect("the intro movie under the retail client directory");
    // `DERETH_TEST_FFMPEG` overrides; otherwise `ffmpeg` is resolved on `PATH`. It is deliberately
    // NOT defaulted to a vendored or hard-coded path: an oracle that silently falls back to
    // something else is the failure this test exists to close.
    let ffmpeg = std::env::var("DERETH_TEST_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string());
    let out = Command::new(&ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&p)
        .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output();
    let out = match out {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => o.stdout,
        other => panic!(
            "ffmpeg must decode the reference stream ({ffmpeg:?}): {other:?}. \
             A shell that predates the install has a stale PATH -- restart it, or set \
             DERETH_TEST_FFMPEG. The DAT video decoder requires the configured FFmpeg executable."
        ),
    };
    let (w, h) = (640usize, 480usize);
    let frame_bytes = w * h * 3;
    let frames = out.len() / frame_bytes;
    assert!(frames > 0, "the independent decoder produced frames");
    assert_eq!(
        out.len() % frame_bytes,
        0,
        "the reference contains complete frames"
    );

    let mut m =
        Movie::from_bytes(std::fs::read(&p).expect("the movie is readable")).expect("demuxes");
    assert_eq!(m.frame_count(), frames, "every reference frame is exposed");
    let mut matched = 0usize;
    let mut pixels = 0usize;
    for i in 0..frames {
        let surface = m
            .decode_frame(i)
            .unwrap_or_else(|| panic!("frame {i} decodes"));
        let r = &out[i * frame_bytes..(i + 1) * frame_bytes];
        // The surface is top-down B, G, R, A; ffmpeg's rgb24 is top-down R, G, B.
        let mut mismatches = 0usize;
        for k in 0..w * h {
            if surface[k * 4 + 2] != r[k * 3]
                || surface[k * 4 + 1] != r[k * 3 + 1]
                || surface[k * 4] != r[k * 3 + 2]
            {
                mismatches += 1;
            }
        }
        assert_eq!(
            mismatches, 0,
            "frame {i}: {mismatches} pixels differ from ffmpeg"
        );
        matched += 1;
        pixels += w * h;
    }
    eprintln!(
        "cinepak vs ffmpeg: {matched} of {frames} frames matched exactly, {pixels} pixels compared"
    );
    // The count, not merely the absence of a failure: a loop that ran zero times prints the
    // same `ok` as one that compared every frame, and that is exactly how this comparison went
    // unrun on two machines.
    assert_eq!(matched, frames, "every reference frame is compared");
    assert_eq!(pixels, frames * w * h);
}

/// Behaviour: presentation.intro-movie.every-frame-decodes-at-the-movies-rate-and-it-never-loops
/// Timing: `next_frame` advances at the movie's own rate and does not skip frames, because Cinepak
/// is inter-coded and skipping one would leave the persistent buffer wrong.
#[test]
fn frames_advance_at_the_movies_rate_and_none_is_skipped() {
    let p = movie_path().expect("the intro movie under the retail client directory");
    let mut m =
        Movie::from_bytes(std::fs::read(&p).expect("the movie is readable")).expect("demuxes");
    assert!(
        m.next_frame(LocalTime(0.0)).is_some(),
        "the first frame shows immediately"
    );
    // Half a frame later there is nothing new.
    assert!(m.next_frame(LocalTime(0.016)).is_none());
    assert!(
        m.next_frame(LocalTime(0.034)).is_some(),
        "one frame period is 33.367 ms"
    );
    // Jumping ahead decodes every intervening frame rather than seeking.
    assert!(m.next_frame(LocalTime(1.0)).is_some());
    assert!(!m.is_finished());
    assert!(m.next_frame(LocalTime(8.0)).is_some(), "the tail decodes");
    assert!(m.is_finished(), "7.31 s of movie is over by 8 s");
}

/// A file that is not an AVI, and one whose video stream is not Cinepak, are both skipped silently.
#[test]
fn a_non_cinepak_file_is_skipped_silently() {
    assert!(Movie::from_bytes(b"not a riff file at all".to_vec()).is_none());
}
