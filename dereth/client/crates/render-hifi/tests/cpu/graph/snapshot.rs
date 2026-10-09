//! The frame snapshot is plain owned data.

use dereth_render_hifi::HifiFrame;

fn crosses_threads<T: Send + 'static>(value: T) -> T {
    std::thread::spawn(move || value)
        .join()
        .expect("the thread returns the value")
}

/// Behaviour: hifi.snapshot.the-frame-snapshot-holds-no-borrow
#[test]
fn a_frame_snapshot_moves_to_another_thread_and_back_unchanged() {
    let mut frame = HifiFrame {
        stamp: 7,
        ..HifiFrame::default()
    };
    frame.sky.day_group = "Sunny".to_owned();
    let back = crosses_threads(frame.clone());
    assert_eq!(back, frame);
}
