#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use self::unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::*;

pub(crate) use is_executable::is_executable;