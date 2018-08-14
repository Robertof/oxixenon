use super::{Notifier as NotifierTrait, Error};
use config;
use protocol::Event;
use std::net::SocketAddr;

pub struct Notifier;
impl NotifierTrait for Notifier {
    fn from_config (_notifier: &config::NotifierConfig) -> Result<Self, Error>
        where Self: Sized
    {
        Ok(Notifier)
    }

    fn notify (&mut self, _event: Event) -> Result<(), Error> { Ok(()) }

    fn listen(&mut self, _on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error> {
        Err(Error::Generic(
            "Can't listen for notifications with this notifier. Try using a real one"
        ))
    }
}
