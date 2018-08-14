//use errors::*;
use byteorder::{ReadBytesExt, WriteBytesExt, NetworkEndian};
use std::fmt;
use std::error;
use std::io::{Read, Write};

// Creates Error, ErrorKind & Result. They are linked to the main error type errors::Error.
error_chain! {}

trait WriteString {
    fn write_u16_string (&mut self, str: Option<&str>) -> Result<()>;
}

trait ReadString {
    fn read_u16_string (&mut self) -> Result<Option<String>>;
}

impl<'a> WriteString for Write + 'a {
    fn write_u16_string(&mut self, str: Option<&str>) -> Result<()> {
        let len = str.as_ref().map (|s| s.len()).unwrap_or (0);
        ensure!(
            len <= <u16>::max_value().into(),
            "invalid string length given to write_u16_string: {}", len
        );
        self.write_u16::<NetworkEndian>(len as u16).chain_err (|| "can't write string length")?;
        if let Some(msg) = str {
            write!(self, "{}", msg).chain_err (|| "can't write string contents")?;
        }
        Ok(())
    }
}

impl<'a> ReadString for Read + 'a {
    fn read_u16_string (&mut self) -> Result<Option<String>> {
        let msg_length = self.read_u16::<NetworkEndian>()
            .chain_err (|| "failed to read expected u16 string length")?;
        trace!("read_u16_string: received msg_length: {}", msg_length);
        let mut msg_buffer: Vec<u8> = Vec::with_capacity (msg_length.into());
        self.take (msg_length.into()).read_to_end (&mut msg_buffer)
            .chain_err (|| format!("failed to read string content of {} bytes", msg_length))?;
        trace!("read_u16_string: read buffer: {:?}", msg_buffer);
        Ok(if msg_buffer.len() > 0 { String::from_utf8(msg_buffer).ok() } else { None })
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Event {
    IPRenewed = 0
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Event::IPRenewed => write!(f, "ip renewed")
        }
    }
}

impl Event {
    pub fn extended_descr(&self) -> &'static str {
        match *self {
            Event::IPRenewed => "An IP renewal has been requested"
        }
    }
}

#[derive(Debug, Clone)]
pub enum RenewAvailability {
    Available,
    Unavailable(String)
}

impl fmt::Display for RenewAvailability {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            RenewAvailability::Available => write!(f, "available"),
            RenewAvailability::Unavailable(ref msg) => write!(f, "unavailable due to \"{}\"", msg)
        }
    }
}

// Representation (packet number not included):
// - Available: \x00
// - Unavailable: \x01 + serialization of the associated string
impl RenewAvailability {
    fn repr (&self) -> u8 {
        match *self {
            RenewAvailability::Available      => 0,
            RenewAvailability::Unavailable(_) => 1
        }
    }

    fn read (reader: &mut Read) -> Result<Self> {
        let variant = reader.read_u8().chain_err (|| "failed to read RenewAvailability variant")?;
        match variant {
            0 /* available */   => Ok(RenewAvailability::Available),
            1 /* unavailable */ => {
                let reason = reader.read_u16_string()
                    .chain_err (|| "failed to read RenewAvailability reason string")?  // Result<T>
                    .chain_err (|| "RenewAvailability reason string can't be empty")?; // Option<T>
                Ok(RenewAvailability::Unavailable(reason))
            },
            _ => bail!("unknown RenewAvailability variant: {}", variant)
        }
    }

    fn write (&self, writer: &mut Write) -> Result<()> {
        writer.write_u8 (self.repr())
            .chain_err (|| "failed to write RenewAvailability variant")?;
        match *self {
            RenewAvailability::Available => (),
            RenewAvailability::Unavailable(ref reason) => {
                writer.write_u16_string (Some (reason))
                    .chain_err (|| "failed to write RenewAvailability reason")?;
            }
        };
        Ok(())
    }
}

#[derive(Debug)]
pub enum Packet {
    // client -> server
    FreshIPRequest,
    SetRenewingAvailable(RenewAvailability),
    // server -> client
    Ok,
    Error(String),
    Event(Event)
}

use std::ops::Deref;

impl<T: Deref<Target = error::Error>> From<T> for Packet {
    fn from(error: T) -> Self {
        Packet::Error(error.to_string())
    }
}

// Packet numbers
const PACKET_FRESH_IP_REQUEST:  u8 = 0;
const PACKET_OK:                u8 = 1;
const PACKET_ERROR:             u8 = 2;
const PACKET_EVENT:             u8 = 3;
const PACKET_SET_RENEW_AVAIL:   u8 = 4;

impl Packet {
    pub fn packet_no(&self) -> u8 {
        match *self {
            Packet::FreshIPRequest          => PACKET_FRESH_IP_REQUEST,
            Packet::Ok                      => PACKET_OK,
            Packet::SetRenewingAvailable(_) => PACKET_SET_RENEW_AVAIL,
            Packet::Error(..)               => PACKET_ERROR,
            Packet::Event(..)               => PACKET_EVENT
        }
    }

    pub fn read(reader: &mut Read) -> Result<Self> {
        let packet_no = reader.read_u8().chain_err (|| "failed to read packet number")?;
        trace!("Packet::read: received packet number: {}", packet_no);

        let packet = match packet_no {
            PACKET_FRESH_IP_REQUEST => Packet::FreshIPRequest,
            PACKET_OK => Packet::Ok,
            PACKET_SET_RENEW_AVAIL => {
                Packet::SetRenewingAvailable(
                    RenewAvailability::read (reader)
                        .chain_err (|| "failed to read Packet::RenewAvailability")?
                )
            },
            PACKET_ERROR => Packet::Error(
                reader
                    .read_u16_string()
                    .chain_err (|| "failed to read Packet::Error reason")?
                    .unwrap_or ("Unknown error".into())
            ),
            PACKET_EVENT => {
                // read the event number
                let event_no = reader.read_u8()
                    .chain_err (|| "failed to read Packet::Event event number")?;
                // try to convert it back to an event
                let event = match event_no {
                    event_no if event_no == Event::IPRenewed as u8 => Event::IPRenewed,
                    _ => bail!("unknown event number: {}", event_no)
                };
                Packet::Event(event)
            },
            _ => bail!("unknown packet number: {}", packet_no)
        };

        trace!("Packet::read: finished parsing packet: {:#?}", packet);
        Ok(packet)
    }

    pub fn write(&self, writer: &mut Write) -> Result<()> {
        writer.write_u8 (self.packet_no()).chain_err (|| "failed to write packet number")?;
        match *self {
            Packet::FreshIPRequest | Packet::Ok => (),
            Packet::SetRenewingAvailable (ref availability) =>
                availability.write (writer).chain_err (|| "failed to write RenewAvailability")?,
            Packet::Error (ref msg) => {
                writer.write_u16_string (Some(msg))
                    .chain_err (|| format!("failed to write error message '{}'", msg))?
            },
            Packet::Event (ref evt) => {
                writer.write_u8 (*evt as u8)
                    .chain_err (|| format!("failed to write event number '{}'", evt))?;
            }
        }
        Ok(())
    }   
}
