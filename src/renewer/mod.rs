use config;
#[cfg(feature = "http-client")]
use http_client;
use std::fmt;
use std::marker::Sized;
use std::error::Error as StdError;

#[cfg(feature = "renewer-dlink")]
mod dlink;
mod dummy;

/* <renewer::Error> */
#[derive(Debug)]
pub enum Error {
    Generic(&'static str), 
    #[cfg(feature = "http-client")]
    Http(http_client::Error),
    Config(config::Error)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            #[cfg(feature = "http-client")]
            Error::Http(ref s) => write!(f, "Renewer: HTTP error: {}", s),
            Error::Generic(ref s) => write!(f, "Renewer: Error: {}", s),
            Error::Config(ref s) => write!(f, "Renewer: Config error: {}", s)
        }
    }
}

impl StdError for Error {
    fn cause(&self) -> Option<&StdError> {
        match *self {
            #[cfg(feature = "http-client")]
            Error::Http(ref e) => Some(e),
            Error::Generic(_) => None,
            Error::Config(ref e) => Some(e)
        }
    }
}

impl From<config::Error> for Error {
    fn from(error: config::Error) -> Self {
        Error::Config(error)
    }
}

#[cfg(feature = "http-client")]
impl From<http_client::Error> for Error {
    fn from(error: http_client::Error) -> Self {
        Error::Http(error)
    }
}
/* </renewer::Error> */

pub trait Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized;
    fn init(&mut self) -> Result<(), Error> { Ok(()) }
    fn renew_ip(&mut self) -> Result<(), Error>;
}

pub fn get_renewer (renewer: &config::RenewerConfig) -> Result<Box<Renewer>, Error> {
    macro_rules! renewer_from_config {
        ($name: path) => {
            <$name>::from_config (renewer).map (|v| Box::new(v) as Box<Renewer>)
        }
    }
    match renewer.name.as_str() {
        #[cfg(feature = "renewer-dlink")]
        "dlink" => renewer_from_config!(dlink::Renewer),
        "dummy" => renewer_from_config!(dummy::Renewer),
        _ => Err(Error::Generic("invalid renewer name -- if applicable, ensure it's enabled"))
    }
}
