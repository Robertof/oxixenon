extern crate byteorder;
extern crate toml;
#[cfg(feature = "server")]
extern crate http;
#[cfg(feature = "server")]
extern crate hmac;
#[cfg(feature = "server")]
extern crate sha2;
extern crate clap;
#[cfg(all(windows, feature = "client-toasts"))]
extern crate winrt;

static mut DEBUG_ENABLED: bool = false;

pub fn debug() -> bool {
    unsafe { DEBUG_ENABLED }
}

pub unsafe fn set_debug (value: bool) {
    DEBUG_ENABLED = value
}

pub fn debug_print (args: std::fmt::Arguments) {
    if debug() {
        eprintln!("<debug> {}", args);
    }
}

macro_rules! debug {
    ( $ ( $x:tt )* ) => {
        ::debug_print(format_args!($($x)*));
    }
}

pub mod config;
#[macro_use]
pub mod protocol;
#[macro_use]
#[cfg(feature = "server")]
pub mod renewer;
#[macro_use]
#[cfg(feature = "server")]
pub mod http_client;
pub mod notifier;

#[cfg(feature = "client-toasts")]
pub mod notification_toasts;
