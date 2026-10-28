"""Process coordination: one background agent per user, one window per user."""

from __future__ import annotations

import errno
import fcntl
import os


def runtime_dir() -> str:
    base = os.environ.get("XDG_RUNTIME_DIR")
    if not base or not os.path.isdir(base):
        base = f"/tmp/ak980-{os.getuid()}"
        os.makedirs(base, mode=0o700, exist_ok=True)
    return base


def lock_path(name: str) -> str:
    return os.path.join(runtime_dir(), f"ak980-{name}.lock")


class InstanceLock:
    """Held for the lifetime of a process; the kernel releases it if the process dies."""

    def __init__(self, name: str):
        self.path = lock_path(name)
        self.fd = -1

    def acquire(self, wait: bool = False) -> bool:
        """Takes the lock; with ``wait`` blocks until the current holder exits."""
        self.fd = os.open(self.path, os.O_RDWR | os.O_CREAT | os.O_CLOEXEC, 0o600)
        try:
            fcntl.flock(self.fd, fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
        except OSError as e:
            os.close(self.fd)
            self.fd = -1
            if e.errno in (errno.EWOULDBLOCK, errno.EAGAIN):
                return False
            raise
        os.ftruncate(self.fd, 0)
        os.write(self.fd, str(os.getpid()).encode())
        return True

    def release(self) -> None:
        if self.fd >= 0:
            os.close(self.fd)
            self.fd = -1


def is_held(name: str) -> bool:
    path = lock_path(name)
    if not os.path.exists(path):
        return False
    try:
        fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC)
    except OSError:
        return False
    try:
        fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
    except OSError:
        return True
    else:
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    finally:
        os.close(fd)


def agent_running() -> bool:
    return is_held("agent")
