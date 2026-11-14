use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::Path,
    process::Command,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        QueryFullProcessImageNameW, WaitForSingleObject,
    },
};

pub fn command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command.creation_flags(0x08000000);
    command
}
pub fn lock(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
}
pub fn wait_parent(pid: u32, expected: &Path, ready: &Path) -> io::Result<()> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut name = [0u16; 32768];
        let mut length = name.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, name.as_mut_ptr(), &mut length) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let actual = std::path::PathBuf::from(String::from_utf16_lossy(&name[..length as usize]));
        if actual.canonicalize()? != expected.canonicalize()? {
            return Err(io::Error::other("update parent is not this installation"));
        }
        std::fs::write(ready, b"ready")?;
        if unsafe { WaitForSingleObject(handle, 30000) } != WAIT_OBJECT_0 {
            return Err(io::Error::other(
                "application did not exit; installation preserved",
            ));
        }
        Ok(())
    })();
    unsafe {
        CloseHandle(handle);
    }
    result
}
