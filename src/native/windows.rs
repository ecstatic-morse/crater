use crate::prelude::*;
use failure::{Error, format_err};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::str::FromStr;
use winapi::um::winbase::GetUserNameW;
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
use winapi::um::winnt::PROCESS_TERMINATE;
use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

/// A 4-part version number which identifies a particular build of Windows.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WindowsBuild {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub revision: u32,
}

impl WindowsBuild {
    fn host() -> Fallible<Self> {
        let reg = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")?;

        let major: u32 = reg.get_value("CurrentMajorVersionNumber")?;
        let minor: u32 = reg.get_value("CurrentMinorVersionNumber")?;
        let revision: u32 = reg.get_value("UBR")?;
        let build: String = reg.get_value("CurrentBuildNumber")?;

        Ok(WindowsBuild {
            major,
            minor,
            build: build.parse()?,
            revision,
        })
    }
}

/// Parse a Windows version number like `10.0.17763.245`.
impl FromStr for WindowsBuild {
    type Err = Error;

    fn from_str(s: &str) -> Fallible<Self> {
        let parts: Vec<u32> = s.split(".")
            .map(|s| s.parse())
            .collect()?;

        let (major, minor, build, revision) = match &parts[..] {
            [a, b, c, d] => (*a, *b, *c, *d),

            _ => bail!("Version string must contain exactly 4 fields")
        };

        Ok(WindowsBuild {
            major,
            minor,
            build,
            revision,
        })
    }
}

/// Returns the Windows build number for the crater machine
pub(crate) fn build_number() -> WindowsBuild {
    lazy_static! {
        static ref HOST_BUILD: WindowsBuild = WindowsBuild::host()
            .expect("Failed to get Windows build of host");
    }

    *HOST_BUILD
}

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

pub(crate) fn current_user() -> Fallible<String> {
    use winapi::shared::lmcons::UNLEN;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::shared::winerror::ERROR_INSUFFICIENT_BUFFER;

    let mut buf = [0; UNLEN as usize + 1];
    let mut len = buf.len() as u32;

    let is_success = unsafe {
        GetUserNameW(buf.as_mut_ptr(), &mut len)
    };

    if is_success == 0 {
        assert_eq!(unsafe { GetLastError() }, ERROR_INSUFFICIENT_BUFFER);
        panic!("Buffer was too small for GetUserNameW");
    }

    let len = (len as usize) - 1; // Omit null terminator
    OsString::from_wide(&buf[..len])
        .into_string()
        .map_err(|_| format_err!("Username was not valid Unicode"))
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

        assert_eq!(cmd.wait().unwrap().code(), Some(101));
    }

    #[test]
    fn test_current_user() {
        assert!(!current_user().unwrap().is_empty());
    }

    #[test]
    fn test_build_number() {
        // This should fail unless run on Windows 10
        assert_eq!(build_number().major, 10);
    }
}
