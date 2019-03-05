use crate::prelude::*;
use std::fs::File;
use std::path::Path;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use winapi::um::winbase::GetUserNameW;
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
use winapi::um::winnt::PROCESS_TERMINATE;

pub(crate) fn kill_process(id: u32) -> Fallible<()> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, id);
        if handle.is_null() {
            bail!("OpenProcess for process {} failed", id);
        }
        if TerminateProcess(handle, 101) == 0 {
            bail!("TerminateProcess for process {} failed", id);
        }
        if CloseHandle(handle) == 0 {
            bail!("CloseHandle for process {} failed", id);
        }
    }

    Ok(())
}

pub(crate) fn current_user() -> String {
    const CAPACITY: usize = winapi::lmcons::UNLEN + 1;
    let mut buf = [0; CAPACITY];
    let mut len: u32 = CAPACITY;
    let ret = unsafe {
        GetUserNameW(&mut buf, &mut len);
    };

    if ret == 0 {
        panic!("GetUserNameW failed");
    }

    Ok(OsString::from_wide(&buf[..len]).into_string().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_kill_process() {
        // Try to kill a sleep command
        let mut cmd = Command::new("timeout").args(&["2"]).spawn().unwrap();
        kill_process(cmd.id()).unwrap();

        // Ensure it was killed with SIGKILL
        assert_eq!(cmd.wait().unwrap().signal(), Some(9));
    }

    #[test]
    fn test_current_user() {
        let username = current_user().unwrap();
        assert!(!username.is_empty());
    }
}
