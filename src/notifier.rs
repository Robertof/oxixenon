use config;
use config::ValueExt;
use protocol::{Packet, Event};
use std::{io, fmt, error};
use std::net::{UdpSocket, IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

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

struct MulticastNotifier {
    bind_addr: SocketAddr,
    addr: SocketAddr
}

impl Notifier for MulticastNotifier {
    fn from_config (notifier: &config::NotifierConfig) -> Result<Self, Error>
        where Self: Sized
    {
        let config = notifier.config.as_ref().ok_or (
            config::Error::InvalidOption ("notifier.multicast"))?;
        // Get addr and bind_addr
        let addr = config
            .get_as_str_or_invalid_key ("notifier.multicast.addr")?
            .to_socket_addrs().map_err (
                |e| Error::GenericWithCause ("Can't parse notifier.multicast.addr", Box::new(e))
            )?
            .find (|&addr| addr.is_ipv4() && addr.ip().is_multicast())
            .ok_or (Error::Generic (
                "Can't determine an IPv4 & multicast address for notifier.multicast.addr"
            ))?;
        let bind_addr = config
            .get_as_str_or_invalid_key ("notifier.multicast.bind_addr")?
            .to_socket_addrs().map_err (
                |e| Error::GenericWithCause (
                    "Can't parse notifier.multicast.bind_addr",
                    Box::new (e)
                )
            )?
            .find (|&addr| addr.is_ipv4())
            .ok_or (Error::Generic (
                "Can't determine an IPv4 address for notifier.multicast.bind_addr"
            ))?;
        debug!("<notifier::multicast> initialized, addr = {}, bind_addr = {}", addr, bind_addr);
        Ok(MulticastNotifier {
            addr,
            bind_addr
        })
    }

    fn notify (&mut self, event: Event) -> Result<(), Error> {
        let socket = UdpSocket::bind (self.bind_addr)?;
        let mut vec: Vec<u8> = Vec::new();
        Packet::Event(event).write (&mut vec)?;
        socket.send_to (&vec, self.addr)?;
        eprintln!("<notifier::multicast> successfully notified event \"{}\"", event);
        Ok(())
    }

    fn listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error>
    {
        let any = Ipv4Addr::new (0, 0, 0, 0);
        let socket = UdpSocket::bind (self.bind_addr)?;
        socket.join_multicast_v4 (match self.addr.ip() {
            IpAddr::V4(ref ip) => ip,
            IpAddr::V6(..)     => panic!("Got IPv6 address when expecting IPv4")
        }, &any)?;
        let mut buf = vec![0; 3]; // for now only support 2-byte packets
        loop {
            let (number_of_bytes, src_addr) = socket.recv_from (&mut buf)?;
            let mut slice = &buf[..number_of_bytes];

            match Packet::read (&mut slice) {
                Ok(packet) => {
                    if let Packet::Event(event) = packet {
                        eprintln!("<notifier::multicast> received event \"{}\"", event);
                        on_event(event, Some(src_addr))
                    }
                },
                Err(error) =>
                    eprintln!("<notifier::multicast> warning: can't decode incoming packet: {}", error)
            }
        }
        
    }   
}

struct NoopNotifier;
impl Notifier for NoopNotifier {
    fn from_config (_notifier: &config::NotifierConfig) -> Result<Self, Error>
        where Self: Sized
    {
        Ok(NoopNotifier)
    }

    fn notify (&mut self, _event: Event) -> Result<(), Error> { Ok(()) }

    fn listen(&mut self, _on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error> {
        Err(Error::Generic(
            "Can't listen for notifications with this notifier. Try using a real one"
        ))
    }
}

pub fn get_notifier (notifier: &config::NotifierConfig) -> Result<Box<Notifier>, Error> {
    macro_rules! notifier_from_config {
        ($name: ident) => {
            $name::from_config (notifier).map (|v| Box::new(v) as Box<Notifier>)
        }
    }
    match notifier.name.as_str() {
        "multicast"     => notifier_from_config!(MulticastNotifier),
        "none" | "noop" => notifier_from_config!(NoopNotifier),
        _ => Err(Error::Generic ("invalid notifier name - must be one of 'multicast', 'none'"))
    }
}
