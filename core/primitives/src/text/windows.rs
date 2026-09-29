//! The only unsafe boundary: bounded slices -> kernel32 NLS functions. No registry, file,
//! network, console, GUI, locale setter, loader, original-client DLL or callback is invoked.
use super::Error;

#[link(name = "kernel32")]
extern "system" {
    fn GetACP() -> u32;
    fn GetLastError() -> u32;
    fn WideCharToMultiByte(
        cp: u32,
        flags: u32,
        wide: *const u16,
        count: i32,
        bytes: *mut u8,
        capacity: i32,
        default: *const u8,
        used: *mut i32,
    ) -> i32;
    fn MultiByteToWideChar(
        cp: u32,
        flags: u32,
        bytes: *const u8,
        count: i32,
        wide: *mut u16,
        capacity: i32,
    ) -> i32;
}

pub fn acp() -> u32 {
    // SAFETY: no pointers, reads the current system ANSI code-page identifier only.
    unsafe { GetACP() }
}
fn error() -> Error {
    // SAFETY: no pointers; called immediately after a failed NLS function on this thread.
    Error::Windows(unsafe { GetLastError() })
}
pub fn query(cp: u32, units: &[u16], default: bool) -> Result<(usize, bool), Error> {
    let count = i32::try_from(units.len()).map_err(|_| Error::TooLong)?;
    let mut used = 0;
    // SAFETY: input points to count initialized units for the synchronous call. Null output
    // and capacity0 request size only. Default is a static terminated byte string; used is live.
    let size = unsafe {
        WideCharToMultiByte(
            cp,
            0,
            units.as_ptr(),
            count,
            std::ptr::null_mut(),
            0,
            if default {
                c"?".as_ptr().cast()
            } else {
                std::ptr::null()
            },
            &mut used,
        )
    };
    if size <= 0 {
        Err(error())
    } else {
        Ok((size as usize, used != 0))
    }
}
pub fn convert(
    cp: u32,
    units: &[u16],
    size: usize,
    default: bool,
) -> Result<(Vec<u8>, bool), Error> {
    let count = i32::try_from(units.len()).map_err(|_| Error::TooLong)?;
    let capacity = i32::try_from(size).map_err(|_| Error::TooLong)?;
    let mut bytes = vec![0; size];
    let mut used = 0;
    // SAFETY: immutable input and distinct initialized output spans match their checked signed
    // lengths and remain alive. API writes at most capacity bytes; default/used have valid storage.
    let written = unsafe {
        WideCharToMultiByte(
            cp,
            0,
            units.as_ptr(),
            count,
            bytes.as_mut_ptr(),
            capacity,
            if default {
                c"?".as_ptr().cast()
            } else {
                std::ptr::null()
            },
            &mut used,
        )
    };
    if written <= 0 {
        return Err(error());
    }
    bytes.truncate(written as usize);
    Ok((bytes, used != 0))
}
pub fn widen(cp: u32, bytes: &[u8]) -> Result<Vec<u16>, Error> {
    let count = i32::try_from(bytes.len()).map_err(|_| Error::TooLong)?;
    // SAFETY: reads count initialized bytes; null output with capacity0 is the documented query.
    let size =
        unsafe { MultiByteToWideChar(cp, 0, bytes.as_ptr(), count, std::ptr::null_mut(), 0) };
    if size <= 0 {
        // SAFETY: captures this failed query's thread-local code before another API can alter it.
        return Err(Error::WindowsSizeQuery(unsafe { GetLastError() }));
    }
    let mut units = vec![0; size as usize];
    // SAFETY: original byte slice and a distinct initialized output of exactly size units live
    // through the call. No pointer is retained and the API's length limits are checked above.
    let written =
        unsafe { MultiByteToWideChar(cp, 0, bytes.as_ptr(), count, units.as_mut_ptr(), size) };
    if written <= 0 {
        return Err(error());
    }
    units.truncate(written as usize);
    Ok(units)
}
