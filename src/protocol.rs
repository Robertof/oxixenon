use byteorder::{ReadBytesExt, WriteBytesExt, NetworkEndian};
use std::fmt;
use std::error;
use std::io::{Error as IoError, ErrorKind, Result, Read, Write};

trait WriteString {
    fn write_u16_string (&mut self, str: Option<&str>) -> Result<()>;
}

trait ReadString {
    fn read_u16_string (&mut self) -> Result<Option<String>>;
}

impl<'a> WriteString for Write + 'a {
    fn write_u16_string(&mut self, str: Option<&str>) -> Result<()> {
        let len = str.as_ref().map (|s| s.len()).unwrap_or (0);
        if len > <u16>::max_value().into() {
            return Err (IoError::from (ErrorKind::InvalidInput));
        }
        self.write_u16::<NetworkEndian>(len as u16)?;
        if let Some(msg) = str {
            write!(self, "{}", msg)?;
        }
        Ok(())
    }
}

impl<'a> ReadString for Read + 'a {
    fn read_u16_string (&mut self) -> Result<Option<String>> {
        let msg_length = self.read_u16::<NetworkEndian>()?;
        trace!("read_u16_string: received msg_length: {}", msg_length);
        let mut msg_buffer: Vec<u8> = Vec::with_capacity (msg_length.into());
        let _ = self.take (msg_length.into()).read_to_end (&mut msg_buffer);
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
        let variant = reader.read_u8()?;
        match variant {
            0 /* available */   => Ok(RenewAvailability::Available),
            1 /* unavailable */ => {
                let reason = reader.read_u16_string()?.ok_or (
                    IoError::new (ErrorKind::InvalidData,
                        "error while parsing RenewAvailability: invalid unavailability string")
                )?;
                Ok(RenewAvailability::Unavailable(reason))
            },
            _ => Err (IoError::new (ErrorKind::InvalidData,
                "error while parsing RenewAvailability: unknown variant"))
        }
    }

    fn write (&self, writer: &mut Write) -> Result<()> {
        writer.write_u8 (self.repr())?;
        match *self {
            RenewAvailability::Available => (),
            RenewAvailability::Unavailable(ref reason) => {
                writer.write_u16_string (Some (reason))?;
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

impl From<Box<error::Error>> for Packet {
    fn from(error: Box<error::Error>) -> Self {
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
        let packet_no = reader.read_u8()?;
        trace!("Packet::read: received packet number: {}", packet_no);

        let packet = match packet_no {
            PACKET_FRESH_IP_REQUEST => Packet::FreshIPRequest,
            PACKET_OK => Packet::Ok,
            PACKET_SET_RENEW_AVAIL => {
                Packet::SetRenewingAvailable(RenewAvailability::read (reader)?)
            },
            PACKET_ERROR => Packet::Error(
                reader.read_u16_string()?.unwrap_or ("Unknown error".into())
            ),
            PACKET_EVENT => {
                // read the event number
                let event_no = reader.read_u8()?;
                // try to convert it back to an event
                let event = match event_no {
                    event_no if event_no == Event::IPRenewed as u8 => Event::IPRenewed,
                    _ => return Err (IoError::new (ErrorKind::InvalidData, "unknown event number"))
                };
                Packet::Event(event)
            },
            _ => return Err (IoError::new (ErrorKind::InvalidData, "unknown packet number"))
        };

        trace!("Packet::read: finished parsing packet: {:#?}", packet);
        Ok(packet)
    }

    pub fn write(&self, writer: &mut Write) -> Result<()> {
        writer.write_u8 (self.packet_no())?;
        match *self {
            Packet::FreshIPRequest | Packet::Ok => (),
            Packet::SetRenewingAvailable (ref availability) => availability.write (writer)?,
            Packet::Error (ref msg) => {
                writer.write_u16_string (Some(msg))?
            },
            Packet::Event (ref evt) => {
                writer.write_u8 (*evt as u8)?;
            }
        }
        Ok(())
    }   
}
