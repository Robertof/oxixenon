extern crate error_chain;

use crate::config;
use crate::protocol;
use crate::notifier;
#[cfg(feature = "server")]
use crate::renewer;

error_chain! {
    links {
        Protocol(protocol::Error, protocol::ErrorKind);
        Config(config::Error, config::ErrorKind);
        Notifier(notifier::Error, notifier::ErrorKind);
        Renewer(renewer::Error, renewer::ErrorKind) #[cfg(feature = "server")];
    }
}
