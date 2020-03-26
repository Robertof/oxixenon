use config;
use protocol::Event;
use std::net::SocketAddr;

error_chain! {
    links {
        Config(config::Error, config::ErrorKind);
    }
}

mod multicast;
mod noop;

pub trait Notifier {
    fn from_config (notifier: &config::NotifierConfig) -> Result<Self>
        where Self: Sized;
    fn notify (&mut self, event: Event) -> Result<()>;
    fn listen(&mut self, on_event: &dyn Fn(Event, Option<SocketAddr>) -> ()) -> Result<()>;
}

pub fn get_notifier (notifier: &config::NotifierConfig) -> Result<Box<dyn Notifier>> {
    macro_rules! notifier_from_config {
        ($name: path) => {
            <$name>::from_config (notifier).map (|v| Box::new(v) as Box<dyn Notifier>)
        }
    }
    match notifier.name.as_str() {
        "multicast"     => notifier_from_config!(multicast::Notifier),
        "none" | "noop" => notifier_from_config!(noop::Notifier),
        _ => bail!("invalid notifier name '{}', must be one of 'multicast', 'none'", notifier.name)
    }
}
