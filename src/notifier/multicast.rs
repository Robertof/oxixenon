use super::{Notifier as NotifierTrait, Result, ResultExt};
use config;
use config::ValueExt;
use protocol::{Packet, Event};
use std::net::{UdpSocket, IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

pub struct Notifier {
    bind_addr: SocketAddr,
    addr: SocketAddr
}

impl NotifierTrait for Notifier {
    fn from_config (notifier: &config::NotifierConfig) -> Result<Self>
        where Self: Sized
    {
        let config = notifier.config.as_ref()
            .chain_err (|| config::ErrorKind::MissingOption ("notifier.multicast"))
            .chain_err (|| "the notifier 'multicast' requires to be configured")?;
        // Get addr and bind_addr
        let addr = config
            .get_as_str_or_invalid_key ("notifier.multicast.addr")
            .chain_err (|| "failed to find an address for the notifier 'multicast'")?
            .to_socket_addrs()
            .chain_err (|| "failed to parse 'notifier.multicast.addr' as a socket address")?
            .find (|&addr| addr.is_ipv4() && addr.ip().is_multicast())
            .chain_err (||
                "failed to find an IPv4 multicast address for 'notifier.multicast.addr'")?;
        let bind_addr = config
            .get_as_str_or_invalid_key ("notifier.multicast.bind_addr")
            .chain_err (|| "failed to find a bind address for the notifier 'multicast'")?
            .to_socket_addrs()
            .chain_err (|| "failed to parse 'notifier.multicast.bind_addr' as a socket address")?
            .find (|&addr| addr.is_ipv4())
            .chain_err (|| "failed to find an IPv4 address for 'notifier.multicast.bind_addr'")?;
        trace!(target: "notifier::multicast", "initialized, addr = {}, bind_addr = {}",
            addr, bind_addr);
        Ok(Self {
            addr,
            bind_addr
        })
    }

    fn notify (&mut self, event: Event) -> Result<()> {
        let socket = UdpSocket::bind (self.bind_addr)
            .chain_err (|| format!("failed to bind to {}", self.bind_addr))?;
        let mut vec: Vec<u8> = Vec::new();
        Packet::Event(event).write (&mut vec)
            .chain_err (|| format!("failed to write event packet '{}' to a local buffer", event))?;
        socket.send_to (&vec, self.addr)
            .chain_err (|| format!("failed to send event packet '{}' to {}", event, self.addr))?;
        debug!(target: "notifier::multicast", "successfully notified event \"{}\"", event);
        Ok(())
    }

    fn listen(&mut self, on_event: &dyn Fn(Event, Option<SocketAddr>) -> ()) -> Result<()>
    {
        let any = Ipv4Addr::new (0, 0, 0, 0);
        let socket = UdpSocket::bind (self.bind_addr)
            .chain_err (|| format!("failed to bind to {}", self.bind_addr))?;
        socket
            .join_multicast_v4 (match self.addr.ip() {
                IpAddr::V4(ref ip) => ip,
                IpAddr::V6(..)     => panic!("Got IPv6 address when expecting IPv4")
            }, &any)
            .chain_err (|| format!("failed to join multicast group '{}'", self.addr))?;
        let mut buf = vec![0; 3]; // for now only support 2-byte packets
        loop {
            let (number_of_bytes, src_addr) = socket.recv_from (&mut buf)
                .chain_err (|| "failed to receive data from multicast socket")?;
            let mut slice = &buf[..number_of_bytes];

            match Packet::read (&mut slice) {
                Ok(packet) => {
                    if let Packet::Event(event) = packet {
                        debug!(target: "notifier::multicast", "received event \"{}\"", event);
                        on_event(event, Some(src_addr))
                    }
                },
                Err(error) =>
                    warn!(target: "notifier::multicast", "can't decode incoming packet: {}", error)
            }
        }
        
    }   
}
