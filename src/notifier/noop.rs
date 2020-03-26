use super::{Notifier as NotifierTrait, Result};
use config;
use protocol::Event;
use std::net::SocketAddr;

pub struct Notifier;
impl NotifierTrait for Notifier {
    fn from_config (_notifier: &config::NotifierConfig) -> Result<Self>
        where Self: Sized
    {
        Ok(Notifier)
    }

    fn notify (&mut self, _event: Event) -> Result<()> { Ok(()) }

    fn listen(&mut self, _on_event: &dyn Fn(Event, Option<SocketAddr>) -> ()) -> Result<()> {
        bail!("Can't listen for notifications with this notifier. Try using a real one")
    }
}
