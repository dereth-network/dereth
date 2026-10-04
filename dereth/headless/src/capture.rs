//! Read recording files supplied to this host and parse them through the shared network crate.

use std::path::Path;

pub use dereth_client_sdk::net::recording::{
    connection_sequence_number, enter_world_requests, peer, recorded_enter_world_requests,
    CaptureError, Datagram, RecordedEntry,
};

/// Load every datagram in a recording, in recorded order.
///
/// # Errors
/// Returns [`CaptureError`] when the file is unreadable, empty or malformed.
pub fn load(path: &Path) -> Result<Vec<Datagram>, CaptureError> {
    let name = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| CaptureError::Io {
        path: name.clone(),
        source,
    })?;
    dereth_client_sdk::net::recording::parse(&name, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (the recording host preserves file errors and delegates byte parsing).
    #[test]
    fn the_host_loader_distinguishes_file_errors_from_recording_errors() {
        let scratch = dereth_client_sdk::dat::testing::ScratchDir::new("headless-recording")
            .expect("scratch directory");
        let path = scratch.path().join("recording.jsonl");
        let name = path.display().to_string();
        assert!(matches!(load(&path), Err(CaptureError::Io { path, .. }) if path == name));
        std::fs::write(&path, [255]).expect("write invalid text");
        assert!(matches!(load(&path), Err(CaptureError::Io { path, .. }) if path == name));
        std::fs::write(&path, "\n").expect("write empty recording");
        assert!(matches!(load(&path), Err(CaptureError::Empty { path }) if path == name));
        std::fs::write(&path, "\n{}").expect("write malformed recording");
        assert!(
            matches!(load(&path), Err(CaptureError::Malformed { path, line: 2, what }) if path == name && what == "no t")
        );
        std::fs::write(&path, r#"{"t":1.5,"dir":"c2s","pair":1,"data":"00ff"}"#)
            .expect("write recording");
        assert_eq!(
            load(&path).expect("recording"),
            vec![Datagram {
                t: 1.5,
                c2s: true,
                pair: 1,
                raw: vec![0, 255]
            }]
        );
    }
}
