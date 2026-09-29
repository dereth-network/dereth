//! **The only unsafe code in the client's error box.** One user32 call.
//!
//! Scope: `MessageBoxW`, with no owner window. The caption and the body are copied into two
//! NUL-terminated UTF-16 buffers that live on this function's stack frame for the length of the
//! call; the system copies what it draws and keeps neither pointer. Nothing is returned but the
//! button the player pressed, which the caller does not need: the box has one.

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        owner: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        style: u32,
    ) -> i32;
}

/// `text` as a NUL-terminated UTF-16 buffer. An embedded NUL would end the string early, so it
/// is dropped rather than passed.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16()
        .filter(|&u| u != 0)
        .chain(std::iter::once(0))
        .collect()
}

/// See [`super::error_box`].
pub fn error_box(caption: &str, text: &str, style: u32) {
    let text = wide(text);
    let caption = wide(caption);
    // SAFETY: both pointers are to NUL-terminated buffers that outlive the call, the owner is
    // null (the box belongs to no window), and the call reads the buffers without keeping them.
    // It blocks until the box is closed and returns the pressed button, which is ignored.
    let _ = unsafe {
        MessageBoxW(
            core::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            style,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buffers the call is handed end in exactly one NUL and carry no other.
    #[test]
    fn the_strings_are_terminated_once() {
        assert_eq!(wide("OK"), vec![u16::from(b'O'), u16::from(b'K'), 0]);
        assert_eq!(wide("a\0b"), vec![u16::from(b'a'), u16::from(b'b'), 0]);
        assert_eq!(wide(""), vec![0]);
    }
}
