use std::{fmt, error};

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

#[cfg(windows)]
mod win32;

#[cfg(windows)]
pub use self::win32::*;

// This ensures that there's no possibility at all to compile oxixenon with notification_toasts
// enabled on an unsupported platform.
#[cfg(not(windows))]
pub use unsupported_platform;
