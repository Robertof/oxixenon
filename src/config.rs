extern crate toml;
extern crate clap;

use protocol;
use clap::ArgMatches;
use std::{fmt, error, io};
use std::fs::File;
use std::ops::FnOnce;
use std::io::prelude::*;

#[derive(Debug)]
pub enum ClientAction {
    RenewIP,
    SetRenewingAvailability(protocol::RenewAvailability),
    SubscribeToNotifications
}

impl fmt::Display for ClientAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ClientAction::RenewIP => write!(f, "renew ip"),
            ClientAction::SetRenewingAvailability(ref availability) =>
                write!(f, "set renewal availability to {}", availability),
            ClientAction::SubscribeToNotifications => write!(f, "subscribe to notifications")
        }
    }
}

#[derive(Debug)]
pub struct ClientConfig {
    pub connect_to: String,
    pub action: ClientAction
}

#[derive(Debug)]
pub struct RenewerConfig {
    pub name: String,
    pub config: Option<toml::Value>
}

#[derive(Debug)]
pub struct ServerConfig {
    pub bind_to: String,
    pub renewer: RenewerConfig
}

#[derive(Debug)]
pub enum Mode {
    Client(ClientConfig),
    Server(ServerConfig)
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Mode::Client(..) => write!(f, "client mode"),
            Mode::Server(..) => write!(f, "server mode")
        }
    }
}

#[derive(Debug)]
pub struct NotifierConfig {
    pub name: String,
    pub config: Option<toml::Value>
}

#[derive(Debug)]
pub struct Config {
    pub mode: Mode,
    pub notifier: NotifierConfig
}

/* <config::Error> */
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    ParsingFailed(toml::de::Error),
    InvalidOption(&'static str),
    InvalidOptionWithReason(&'static str, &'static str)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref err) => write!(f, "I/O error: {}", err),
            Error::ParsingFailed(ref err) => write!(f, "Parsing failed: {}", err),
            Error::InvalidOption(ref str) => write!(f, "Invalid/missing option `{}`", str),
            Error::InvalidOptionWithReason(ref str, ref reason) =>
                write!(f, "Invalid option `{}` - {}", str, reason)
        }
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::ParsingFailed(ref err) => Some(err),
            Error::Io(ref err) => Some(err),
            _ => None
        }
    }
}

impl From<toml::de::Error> for Error {
    fn from(error: toml::de::Error) -> Self {
        Error::ParsingFailed(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        io::Error::new (io::ErrorKind::Other, error)
    }    
}
/* </config::Error> */

pub trait ValueExt {
    fn get_as<'a, T, F>(&'a self, key: &'static str, f: F) -> Option<T>
        where F: FnOnce(&'a Self) -> Option<T>;
    fn get_as_or_invalid_key<'a, T, F>(&'a self, key: &'static str, f: F) -> Result<T, Error>
        where F: FnOnce(&'a toml::Value) -> Option<T>;
    fn get_as_str (&self, key: &'static str) -> Option<&str>;
    fn get_as_str_or_invalid_key (&self, key: &'static str) -> Result<&str, Error>;
    fn get_as_table_or_invalid_key (&self, key: &'static str) -> Result<&toml::Value, Error>;
}

impl ValueExt for toml::Value {
    fn get_as<'a, T, F>(&'a self, key: &'static str, f: F) -> Option<T>
        where F: FnOnce(&'a Self) -> Option<T>
    {
        self.get (key.split ('.').collect::<Vec<&str>>().pop().unwrap())
            .and_then (f)
    }

    fn get_as_or_invalid_key<'a, T, F>(&'a self, key: &'static str, f: F) -> Result<T, Error>
        where F: FnOnce(&'a Self) -> Option<T>
    {
        self.get_as (key, f)
            .ok_or (Error::InvalidOption (key.into()))
    }

    fn get_as_str (&self, key: &'static str) -> Option<&str> {
        Self::get_as (self, key, toml::Value::as_str)
    }

    fn get_as_str_or_invalid_key (&self, key: &'static str) -> Result<&str, Error> {
        Self::get_as_or_invalid_key (self, key, toml::Value::as_str)
    }
    fn get_as_table_or_invalid_key (&self, key: &'static str) -> Result<&toml::Value, Error> {
        Self::get_as_or_invalid_key (self, key, |v|
             if v.is_table() { Some(v) } else { None })
    }
}

impl Config {
    pub fn parse_config(config_path: &str, args: &ArgMatches) -> Result<Config, Error> {
        macro_rules! arg_or_cfg_option {
            (from [$args:expr] get $arg:expr, from [$config:expr] get $option:expr) => {
                $args.and_then (|a| a.value_of ($arg))
                     .or_else (|| $config.get_as_str ($option))
                     .ok_or (Error::InvalidOption ($option))
            }
        }
        // slurp the config file and parse it
        let mut config_str = String::new();
        File::open (config_path)?.read_to_string (&mut config_str)?;
        let config = config_str.parse::<toml::Value>()?;

        // parse notifiers
        let notifier = {
            let chosen_notifier = arg_or_cfg_option!(
                from [Some(args)] get "notifier",
                from [config]     get "notifier_name"
            )?;
            let notifier_config = config.get ("notifier").and_then (|c| c.get (chosen_notifier));
            NotifierConfig {
                name: chosen_notifier.into(),
                config: notifier_config.map (|c| c.clone())
            }
        };

        let mode: Mode = {
            // get subcommand and related args
            let (subcommand_name, subcommand_args) = args.subcommand();
            // get run mode
            let mode_str = if subcommand_name.is_empty() { None } else { Some(subcommand_name) }
                .or_else (|| config.get_as_str("mode"))
                .ok_or (Error::InvalidOption ("mode"))?;

            match mode_str {
                "server" => {
                    // requested server mode, get server table
                    let server_table = config.get_as_table_or_invalid_key ("server")?;
                    // try to retrieve the chosen renewer first from command line arguments,
                    // then from the config file.
                    let chosen_renewer = arg_or_cfg_option!(
                        from [subcommand_args] get "renewer",
                        from [server_table]    get "server.renewer_name"
                    )?;
                    let renewer_config = server_table.get ("renewer")
                        .and_then (|v| v.get (chosen_renewer));

                    Mode::Server (ServerConfig {
                        bind_to: server_table.get_as_str_or_invalid_key ("server.bind_to")?.into(),
                        renewer: RenewerConfig {
                            name: chosen_renewer.into(),
                            config: renewer_config.map (|v| v.clone())
                        }
                    })
                },
                "client" => {
                    // requested client mode, get client table
                    let client_table = config.get_as_table_or_invalid_key ("client")?;
                    // parse CLI arguments
                    let action_name = subcommand_args
                        .and_then (|s| s.subcommand_name()) // try CLI first
                        .or_else (|| // otherwise get client_table.action.name
                            client_table.get ("action")
                                        .and_then (|a| a.get_as_str ("name")))
                        // or croak
                        .ok_or (Error::InvalidOption ("client.action.name"))?;
                    let action = match action_name {
                        "renew" => ClientAction::RenewIP,
                        "notifications" => ClientAction::SubscribeToNotifications,
                        "set_availability" => {
                            // get args of client-mode subcommand, that is
                            // ./bin client set_availability [args]
                            let args = subcommand_args.and_then (|s| s.subcommand().1);
                            if let Some(args) = args {
                                ClientAction::SetRenewingAvailability (
                                    match args.value_of ("availability").unwrap() {
                                        "available"   => protocol::RenewAvailability::Available,
                                        "unavailable" => protocol::RenewAvailability::Unavailable (
                                            args
                                                .value_of ("reason")
                                                .ok_or (Error::InvalidOptionWithReason (
                                                    "client.action.set_availability.reason",
                                                    "missing unavailability reason, see help"
                                                ))?
                                                .into()
                                        ),
                                        _ => unreachable!()
                                    }
                                )
                            } else {
                                let table = client_table
                                   .get_as_table_or_invalid_key("client.action")?
                                   .get_as_table_or_invalid_key("client.action.set_availability")?;
                                ClientAction::SetRenewingAvailability (
                                    match table.get ("available").and_then (|v| v.as_bool()) {
                                        Some(true)  => protocol::RenewAvailability::Available,
                                        Some(false) => protocol::RenewAvailability::Unavailable (
                                            table.get_as_str_or_invalid_key ("reason")?.into()
                                        ),
                                        None => return Err (Error::InvalidOption (
                                            "client.action.set_availability.available"))
                                    }
                                )
                            }
                        },
                        _ => return Err (Error::InvalidOption ("client.action.name"))
                    };
                    Mode::Client (ClientConfig {
                        connect_to: arg_or_cfg_option!(
                            from [subcommand_args] get "connect_to",
                            from [client_table]    get "client.connect_to"
                        )?.into(),
                        action
                    })
                }
                _ => return Err(Error::InvalidOption("mode"))
            }
        };

        Ok(Config { mode, notifier })
    }
}
