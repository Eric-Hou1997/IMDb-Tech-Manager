//! Windows file replacement keeps the original security descriptor and named
//! streams through ReplaceFileW. The same-directory native backup also covers
//! the documented partial-failure states; the independent journal stays intact.
#[cfg(windows)]
pub fn replace(
    source: &std::path::Path,
    candidate: &std::path::Path,
    backup: &std::path::Path,
    expected: &str,
) -> std::io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    fn wide(p: &std::path::Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let from = wide(source);
    let to = wide(candidate);
    let saved = wide(backup);
    // Flags intentionally zero: ignoring ACL/stream merge failures would silently
    // discard metadata, and REPLACEFILE_WRITE_THROUGH is not supported by Windows.
    if unsafe {
        ReplaceFileW(
            from.as_ptr(),
            to.as_ptr(),
            saved.as_ptr(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        let error = std::io::Error::last_os_error();
        // ERROR_UNABLE_TO_MOVE_REPLACEMENT_2 can leave the original at backup.
        // Restore only when no destination exists; never replace an external edit.
        if error.raw_os_error() == Some(1177)
            && !source.exists()
            && std::fs::read(backup).is_ok_and(|bytes| crate::hash(&bytes) == expected)
        {
            unsafe {
                MoveFileExW(saved.as_ptr(), from.as_ptr(), 8);
            }
        }
        return Err(error);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(source)?
        .sync_all()
}

/// Capture and restore the original DACL instead of keeping inherited entries
/// from a freshly created candidate. A descriptor is DWORD-aligned for Win32.
#[cfg(windows)]
pub struct Security(Vec<u32>);
#[cfg(windows)]
impl Security {
    pub fn read(path: &std::path::Path) -> std::io::Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn GetFileSecurityW(
                path: *const u16,
                info: u32,
                descriptor: *mut std::ffi::c_void,
                length: u32,
                needed: *mut u32,
            ) -> i32;
        }
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut needed = 0;
        unsafe {
            GetFileSecurityW(path.as_ptr(), 4, std::ptr::null_mut(), 0, &mut needed);
        }
        if needed == 0 || needed > 1024 * 1024 {
            return Err(std::io::Error::last_os_error());
        }
        let mut data = vec![0u32; (needed as usize).div_ceil(4)];
        if unsafe {
            GetFileSecurityW(
                path.as_ptr(),
                4,
                data.as_mut_ptr().cast(),
                (data.len() * 4) as u32,
                &mut needed,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(data))
    }
    pub fn apply(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn SetFileSecurityW(
                path: *const u16,
                info: u32,
                descriptor: *const std::ffi::c_void,
            ) -> i32;
            fn GetSecurityDescriptorControl(
                descriptor: *const std::ffi::c_void,
                control: *mut u16,
                revision: *mut u32,
            ) -> i32;
        }
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut control = 0;
        let mut revision = 0;
        if unsafe {
            GetSecurityDescriptorControl(self.0.as_ptr().cast(), &mut control, &mut revision)
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let protection = if control & 0x1000 != 0 {
            0x80000000
        } else {
            0x20000000
        };
        if unsafe { SetFileSecurityW(path.as_ptr(), 4 | protection, self.0.as_ptr().cast()) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}
