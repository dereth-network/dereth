//! **The only unsafe code that touches a window's class.** One kernel32 call and three user32 ones.
//!
//! Scope: the executable's own icon resource, loaded as a shared icon at the system's two standard
//! icon sizes and put on the class of a window this process made. A shared icon belongs to the
//! system, which frees it with the module, so nothing is owned here or ever closed, and loading it
//! again hands back the same icon rather than a new one. The window handle is only passed through:
//! the system checks it, and refuses a window of another process.

use core::ffi::c_void;

const IMAGE_ICON: u32 = 1;
const LR_DEFAULTSIZE: u32 = 0x0040;
const LR_SHARED: u32 = 0x8000;
const SM_CXSMICON: i32 = 49;
const SM_CYSMICON: i32 = 50;
const GCLP_HICON: i32 = -14;
const GCLP_HICONSM: i32 = -34;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

#[link(name = "user32")]
extern "system" {
    fn LoadImageW(
        instance: *mut c_void,
        name: *const u16,
        kind: u32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> *mut c_void;
    fn GetSystemMetrics(index: i32) -> i32;
    fn SetClassLongPtrW(window: *mut c_void, index: i32, value: isize) -> usize;
}

/// See [`super::set_class_icon`].
pub fn set_class_icon(window: isize, resource: u16) -> bool {
    if window == 0 {
        return false;
    }
    // SAFETY: a null name asks for the executable's own module, which is not loaded or freed by
    // the call; its handle stays valid for the life of the process.
    let module = unsafe { GetModuleHandleW(core::ptr::null()) };
    if module.is_null() {
        return false;
    }
    // A numbered resource is named by its number in the pointer itself; the call reads it as a
    // number and never dereferences it.
    let name = core::ptr::without_provenance::<u16>(usize::from(resource));
    // SAFETY: plain reads of two system metrics.
    let (cx, cy) = unsafe { (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)) };
    // SAFETY: `module` is this process's own, `name` is a resource number (above), and
    // `LR_SHARED` leaves the icons the system's: nothing returned here is ours to destroy. The
    // big icon is the system's standard size (`LR_DEFAULTSIZE` with no size given), the small one
    // the standard small size, the only sizes a shared icon is meant to be loaded at.
    let (big, small) = unsafe {
        (
            LoadImageW(module, name, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE | LR_SHARED),
            LoadImageW(module, name, IMAGE_ICON, cx, cy, LR_SHARED),
        )
    };
    let window = window as *mut c_void;
    for (index, icon) in [(GCLP_HICON, big), (GCLP_HICONSM, small)] {
        if !icon.is_null() {
            // SAFETY: the system validates the window handle and refuses one of another process.
            // The icon is shared, so the class may hold it for as long as the process lives; the
            // value it replaces is not ours to free (none, or a shared icon set here before).
            unsafe { SetClassLongPtrW(window, index, icon as isize) };
        }
    }
    !big.is_null() && !small.is_null()
}
