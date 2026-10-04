//! Read-only desktop queries. Native pointers never escape these calls.

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[link(name = "user32")]
extern "system" {
    fn GetCaretBlinkTime() -> u32;
    fn GetMonitorInfoW(monitor: *mut core::ffi::c_void, info: *mut MonitorInfo) -> i32;
}

pub fn caret_blink_millis() -> u32 {
    // SAFETY: this read-only query takes no pointers and retains no resources.
    unsafe { GetCaretBlinkTime() }
}

pub fn monitor_work_area(handle: isize) -> Option<(i32, i32, i32, i32)> {
    if handle == 0 || handle == -1 {
        return None;
    }
    let mut info = MonitorInfo {
        size: u32::try_from(std::mem::size_of::<MonitorInfo>()).ok()?,
        ..MonitorInfo::default()
    };
    // SAFETY: the system validates the opaque monitor handle. `info` is a complete initialized
    // writable structure whose size matches the native layout; neither pointer is retained.
    if unsafe { GetMonitorInfoW(handle as *mut core::ffi::c_void, &mut info) } == 0 {
        return None;
    }
    Some((
        info.work.left,
        info.work.top,
        info.work.right,
        info.work.bottom,
    ))
}
