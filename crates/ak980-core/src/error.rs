use std::fmt;
use std::io;

/// Errors carry a message that is already translated, so callers can show it as it is.
#[derive(Debug)]
pub enum Error {
    /// The keyboard (or the part of it a command needs) is not connected.
    NotConnected(String),
    /// The device exists but this user may not open it: the udev rule is missing.
    Permission(String),
    /// A value the user gave cannot be used.
    Invalid(String),
    Io(io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotConnected(m) | Error::Permission(m) | Error::Invalid(m) => f.write_str(m),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Error::Invalid(message.into())
    }

    pub fn io(kind: io::ErrorKind, message: impl Into<String>) -> Self {
        Error::Io(io::Error::new(kind, message.into()))
    }
}
