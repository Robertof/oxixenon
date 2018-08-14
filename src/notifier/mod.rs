use config;
use protocol::Event;
use std::{io, fmt, error};
use std::net::SocketAddr;

mod multicast;
mod noop;

/* <notifier::Error> */
#[derive(Debug)]
pub enum Error {
    Generic(&'static str),
    GenericWithCause(&'static str, Box<error::Error>),
    Config(config::Error),
    Io(io::Error),
    Other(Box<error::Error>)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Generic(ref e)                   => write!(f, "Notifier: Error: {}", e),
            Error::Config (ref e)                   => write!(f, "Notifier: Config error: {}", e),
            Error::Io     (ref e)                   => write!(f, "Notifier: I/O error: {}", e),
            Error::Other  (ref e)                   => write!(f, "Notifier: Error: {}", e),
            Error::GenericWithCause(ref msg, ref e) => write!(f, "Notifier: Error: {}: {}", msg, e)
        }
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Generic(_) => None,
            Error::Config(ref e) => Some(e),
            Error::Io(ref e) => Some(e),
            Error::Other(ref e) => Some(e.as_ref()),
            Error::GenericWithCause(_, ref e) => Some(e.as_ref())
        }
    }
}

impl From<config::Error> for Error {
    fn from(error: config::Error) -> Self {
        Error::Config(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<Box<error::Error>> for Error {
    fn from(error: Box<error::Error>) -> Self {
        Error::Other(error)
    }
}
/* </notifier::Error> */

pub trait Notifier {
    fn from_config (notifier: &config::NotifierConfig) -> Result<Self, Error>
        where Self: Sized;
    fn notify (&mut self, event: Event) -> Result<(), Error>;
    fn listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error>;
}

pub fn get_notifier (notifier: &config::NotifierConfig) -> Result<Box<Notifier>, Error> {
    macro_rules! notifier_from_config {
        ($name: path) => {
            <$name>::from_config (notifier).map (|v| Box::new(v) as Box<Notifier>)
        }
    }
    match notifier.name.as_str() {
        "multicast"     => notifier_from_config!(multicast::Notifier),
        "none" | "noop" => notifier_from_config!(noop::Notifier),
        _ => Err(Error::Generic ("invalid notifier name - must be one of 'multicast', 'none'"))
    }
}
