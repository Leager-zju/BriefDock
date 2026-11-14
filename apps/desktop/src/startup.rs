//! The current user's explicit opt-in; development and isolated instances never write it.
use std::{io, os::windows::ffi::OsStrExt, ptr};
use windows_sys::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::*,
    },
    core::w,
};
const RUN: *const u16 = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const NAME: *const u16 = w!("LazyBuddy");
fn command() -> io::Result<Vec<u16>> {
    let path = std::env::current_exe()?;
    Ok(std::iter::once(34)
        .chain(path.as_os_str().encode_wide())
        .chain("\" --background".encode_utf16())
        .chain([0])
        .collect())
}
pub fn enabled() -> bool {
    let Ok(command) = command() else {
        return false;
    };
    let mut value = vec![0u16; 32768];
    let mut bytes = (value.len() * 2) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN,
            NAME,
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            value.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    result == ERROR_SUCCESS && command == value[..bytes as usize / 2]
}
pub fn set(enabled: bool) -> io::Result<()> {
    if lazybuddy_host_control::isolated() {
        return Err(io::Error::other("isolated instance"));
    }
    let result = if enabled {
        let command = command()?;
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN,
                NAME,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME) }
    };
    if result == ERROR_SUCCESS
        || (!enabled && matches!(result, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND))
    {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}
