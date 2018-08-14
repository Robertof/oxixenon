extern crate oxixenon;
#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
extern crate error_chain;

use std::process;
use error_chain::ChainedError;
use oxixenon::*;
use oxixenon::errors::*;
use oxixenon::notifier::Notifier;

#[cfg(all(feature = "client", feature = "client-toasts"))]
use oxixenon::notification_toasts::*;

fn main() {
    let args = clap_app!(oxixenon =>
        (@setting DeriveDisplayOrder)
        (@setting VersionlessSubcommands)
        (version: crate_version!())
        (about: "Fresh IPs for everyone.")
        (author: "Roberto Frenna [https://roberto.frenna.pro]")
        (@arg config: -c --config +takes_value "Sets a custom config file (default: config.toml)")
        (@arg level: -l +takes_value possible_value[off error warn info debug trace]
            "Sets logging level")
        (@arg verbose: -v --verbose "Sets logging level to 'debug'")
        (@arg notifier: -n --notifier +takes_value "Uses the specified notifier")
        (@subcommand client =>
            (about: "Client mode")
            (@arg connect_to: -a --addr +takes_value
                "Connects to the specified address + port (e.g. 1.2.3.4:1234)")
            (@subcommand renew =>
                (about: "Sends an IP renewal request")
            )
            (@subcommand set_availability =>
                (about: "Sets the availability of the renewal function")
                (@arg availability: * +takes_value possible_value[available unavailable]
                    "Availability")
                (@arg reason: +takes_value
                    "Reason of unavailability - only required when availability is 'unavailable'")
            )
            (@subcommand notifications =>
                (about: "Subscribe to remote notifications")
            )
        )
        (@subcommand server =>
            (about: "Server mode")
            (@arg renewer:
                -r --renewer +takes_value "Uses the specified renewer")
        )
    ).get_matches();
    // Parse the specified (or default) configuration file.
    let config_file = args.value_of ("config").unwrap_or ("config.toml");
    let config = match config::Config::parse_config(config_file, &args) {
        Err(error) => {
            eprintln!("Can't parse config file \"{}\" or command line arguments",
                config_file);
            eprintln!("{}", error.display_chain());
            process::exit(1)
        },
        Ok(result) => result
    };
    // Setup logging.
    if let Err(error) = logging::init (&config.logging) {
        eprintln!("Can't setup logging: {}", error.display_chain());
        process::exit(1)
    }
    // Get and initialize the chosen notifier.
    let notifier = match notifier::get_notifier (&config.notifier) {
        Err(error) => {
            error!("can't instantiate the requested notifier '{}'", config.notifier.name);
            log_error_with_chain!(error, "{}", error);
            process::exit(1)
        },
        Ok(result) => result
    };
    info!("running in {}", config.mode);
    let result = match config.mode {
        config::Mode::Server(ref config) => start_server (config, notifier),
        config::Mode::Client(ref config) => start_client (config, notifier)
    };
    if let Err(error) = result {
        log_error_with_chain!(error, "{}", error);
        process::exit(2);
    }
}

// Server
#[cfg(feature = "server")]
fn start_server (config: &config::ServerConfig, mut notifier: Box<Notifier>) -> Result<()> {
    use std::io::{BufWriter, BufReader};
    use std::time;
    use std::net::TcpListener;
    use oxixenon::protocol::{Packet, Event, RenewAvailability};
    // Local macro to make returning errors easy.
    macro_rules! error_packet {
        ($writer: ident, $($message: tt),+) => {{
            let msg = format!($($message),+);
            warn!(target: "server", "client produced error: {}", msg);
            Packet::Error (msg)
                .write (&mut $writer)
                .map_err (|e| e.into())
        }}
    }
    // Fetch an instance of the IP renewer
    let mut renewer = renewer::get_renewer (&config.renewer)?;
    renewer.init()?;
    // Store the current availability status.
    let mut availability = RenewAvailability::Available;
    info!(target: "server", "binding to {}", config.bind_to);
    let listener = TcpListener::bind (config.bind_to.as_str())
        .chain_err (|| format!("failed to bind to {}", config.bind_to))?;
    for stream in listener.incoming() {
        let mut stream = stream.chain_err (|| "failed to retrieve I/O stream")?;
        let peer_addr = stream.peer_addr().chain_err (|| "failed to retrieve peer address")?;
        let mut writer = BufWriter::new (&stream);
        let mut reader = BufReader::new (&stream);
        debug!(target: "server", "new client connected: {}", peer_addr);
        
        // poor man's try-catch block
        let result = (|| -> Result<()> {
            stream.set_read_timeout (Some (time::Duration::from_secs (5)))
                .chain_err (|| "failed to set stream read timeout to 5 seconds")?;
            let packet = Packet::read (&mut reader)
                .chain_err (|| "invalid packet")?;
            match packet {
                Packet::FreshIPRequest => {
                    info!(target: "server", "client {} requested a new IP address", peer_addr);
                    if let RenewAvailability::Unavailable(reason) = &availability {
                        return error_packet!(writer, "Renewal unavailable: {}", reason);
                    }
                    // Make sure that the outermost error is something safe to send to the client.
                    renewer.renew_ip()
                        .chain_err (|| "failed to renew the IP address")?;
                    notifier.notify (Event::IPRenewed)
                        .chain_err (|| "failed to notify the requested event")?;
                },
                Packet::SetRenewingAvailable (new_availability) => {
                    info!(target: "server", "client {} set availability to {}",
                        peer_addr, new_availability);
                    availability = new_availability;
                },
                _ => return error_packet!(writer, "Unsupported packet")
            };
            Packet::Ok.write (&mut writer)?;
            Ok(())
        })();

        if let Err(err) = result {
            log_error_with_chain!(
                target: "server",
                log::Level::Warn,
                err, "client {} produced external error: {}", peer_addr, err
            );

            // Retrieve a safe message to send to the client as an error message.
            let message = match err {
                // Protocol and chained errors can be safely sent (without the underlying cause)
                Error(ErrorKind::Protocol(err), _) => err.to_string(),
                Error(ErrorKind::Msg(err), _)      => err,
                Error(ErrorKind::Notifier(_), _)   => "failed to send notifications".into(),
                Error(ErrorKind::Renewer(_), _)    => "failed to renew the IP address".into(),
                _                                  => "unexpected error".into()
            };

            // ignore errors while writing errors
            let _ = Packet::Error(message).write (&mut writer);
        }
    }
    Ok(())
}

#[cfg(not(feature = "server"))]
fn start_server (_config: &config::ServerConfig, _notifier: Box<Notifier>) -> Result<()> {
    error!("server functionality is disabled");
    process::exit(255)
}

// Client
#[cfg(feature = "client-toasts")]
fn try_send_toast (toasts: &NotificationToasts, message: &str) {
    if let Err(e) = toasts.send_toast (message) {
        warn!(target: "client", "can't send notification toast: {}", e)
    }
}

#[cfg(feature = "client")]
fn start_client (config: &config::ClientConfig, mut notifier: Box<Notifier>) -> Result<()> {
    use std::io::prelude::*;
    use std::io::{BufReader, BufWriter};
    use std::net::TcpStream;
    use oxixenon::protocol::Packet;
    info!(target: "client", "running action '{}'", config.action);
    let packet = match config.action {
        config::ClientAction::RenewIP => Some (Packet::FreshIPRequest),
        config::ClientAction::SetRenewingAvailability (ref availability) =>
            Some (Packet::SetRenewingAvailable (availability.clone())),
        config::ClientAction::SubscribeToNotifications => {
            #[cfg(feature = "client-toasts")]
            let toasts = NotificationToasts::new();
            notifier.listen (&|event, from| {
                let from_str = from.map (|x| x.to_string()).unwrap_or ("unknown".into());
                info!(target: "client", "received event \"{}\" from {}", event, from_str);
                #[cfg(feature = "client-toasts")]
                try_send_toast (&toasts,
                    format!("{}\nRequest sent by {}", event.extended_descr(), from_str).as_str());
            })?;
            None
        }
    };

    if let Some(packet) = packet {
        info!(target: "client", "connecting to {}...", config.connect_to);
        let stream = TcpStream::connect (config.connect_to.as_str())
            .chain_err (|| format!("failed to connect to {}", config.connect_to))?;
        let mut reader = BufReader::new (&stream);
        let mut writer = BufWriter::new (&stream);
        packet.write (&mut writer)?;
        writer.flush()
            .chain_err (|| "failed to flush the I/O stream")?;

        let response = Packet::read (&mut reader)?;

        match response {
            Packet::Ok => info!(target: "client", "action completed successfully"),
            Packet::Error (ref msg) => error!(target: "client", "{}", msg),
            _ => error!(target: "client", "received unknown packet: {:?}", response)
        }
    }

    Ok(())
}

#[cfg(not(feature = "client"))]
fn start_client (_config: &config::ClientConfig, _notifier: Box<Notifier>) -> Result<()> {
    error!("client functionality is disabled");
    process::exit(255)
}
