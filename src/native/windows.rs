use crate::prelude::*;
use std::fs::File;
use std::path::Path;
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
use winapi::um::winnt::PROCESS_TERMINATE;

pub(crate) fn kill_process(id: u32) -> Fallible<()> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, id);
        if TerminateProcess(handle, 101) == 0 {
            bail!("TerminateProcess for process {} failed", id);
        }
        if CloseHandle(handle) == 0 {
            bail!("CloseHandle for process {} failed", id);
        }
    }

    Ok(())
}

pub(crate) fn current_user() -> u32 {
    0
}

pub(crate) fn is_executable<P: AsRef<Path>>(path: P) -> Fallible<bool> {
    File::open(path)
        .map(|_| true)
        .map_err(Into::into)
}

pub(crate) fn make_executable<P: AsRef<Path>>(_path: P) -> Fallible<()> {
    Ok(())
}
