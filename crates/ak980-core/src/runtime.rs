//! Process coordination: one background agent per user, one window per user.

use std::ffi::CString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::PathBuf;

pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(d) if d.is_dir() => d,
        _ => {
            let d = PathBuf::from(format!("/tmp/ak980-{}", unsafe { libc::getuid() }));
            let _ = std::fs::create_dir_all(&d);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
            }
            d
        }
    }
}

pub fn lock_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("ak980-{name}.lock"))
}

fn open_lock(name: &str) -> io::Result<OwnedFd> {
    let path = CString::new(lock_path(name).as_os_str().as_encoded_bytes()).map_err(io::Error::other)?;
    // SAFETY: valid C string; the fd is owned from here on
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC, 0o600) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Held for the lifetime of a process; the kernel releases it if the process dies.
pub struct InstanceLock {
    name: String,
    fd: Option<OwnedFd>,
}

impl InstanceLock {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), fd: None }
    }

    /// Takes the lock; with `wait` blocks until the current holder exits.
    pub fn acquire(&mut self, wait: bool) -> io::Result<bool> {
        let fd = open_lock(&self.name)?;
        let flags = libc::LOCK_EX | if wait { 0 } else { libc::LOCK_NB };
        // SAFETY: flock on our own fd
        if unsafe { libc::flock(fd.as_raw_fd(), flags) } < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
                return Ok(false);
            }
            return Err(e);
        }
        let mut f = std::fs::File::from(fd);
        f.set_len(0)?;
        write!(f, "{}", std::process::id())?;
        self.fd = Some(OwnedFd::from(f));
        Ok(true)
    }

    pub fn release(&mut self) {
        self.fd = None;
    }
}

pub fn is_held(name: &str) -> bool {
    if !lock_path(name).exists() {
        return false;
    }
    let Ok(fd) = open_lock(name) else { return false };
    // SAFETY: flock on our own fd
    if unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } < 0 {
        return true;
    }
    unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_UN) };
    false
}

pub fn agent_running() -> bool {
    is_held("agent")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_is_exclusive() {
        let name = format!("test-{}", std::process::id());
        let mut a = InstanceLock::new(&name);
        assert!(a.acquire(false).unwrap());
        assert!(is_held(&name));
        let mut b = InstanceLock::new(&name);
        assert!(!b.acquire(false).unwrap());
        a.release();
        assert!(!is_held(&name));
        let _ = std::fs::remove_file(lock_path(&name));
    }
}
