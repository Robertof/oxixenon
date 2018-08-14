extern crate error_chain;

use config;
use protocol;
use notifier;
#[cfg(feature = "server")]
use renewer;

error_chain! {
    links {
        Protocol(protocol::Error, protocol::ErrorKind);
        Config(config::Error, config::ErrorKind);
        Notifier(notifier::Error, notifier::ErrorKind);
        Renewer(renewer::Error, renewer::ErrorKind) #[cfg(feature = "server")];
    }
}
