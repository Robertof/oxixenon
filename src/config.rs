extern crate toml;
extern crate clap;

use protocol;
use clap::ArgMatches;
use std::fmt;
use std::fs::File;
use std::ops::FnOnce;
use std::io::prelude::*;

// config::Error type
error_chain! {
    errors {
        MissingOption (name: &'static str) {
            description("missing configuration option")
            display("missing configuration option: {}", name)
        }
        InvalidOption (name: &'static str) {
            description("invalid configuration option")
            display("invalid configuration option: {}", name)
        }
    }
}

// Configuration models
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
            ClientAction::SubscribeToNotifications => write!(f, "listen to notifications")
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
pub struct LogBackendConfig {
    pub name: String,
    pub config: Option<toml::Value>
}

#[derive(Debug)]
pub struct LogConfig {
    pub level: String,
    pub backends: Vec<LogBackendConfig>
}

#[derive(Debug)]
pub struct Config {
    pub mode: Mode,
    pub notifier: NotifierConfig,
    pub logging: LogConfig
}

// Extension to toml::Value
pub trait ValueExt {
    fn get_as<'a, T, F>(&'a self, key: &'static str, f: F) -> Result<T>
        where F: FnOnce(&'a Self) -> Option<T>;
    fn get_as_str (&self, key: &'static str) -> Option<&str>;
    fn get_as_str_or_invalid_key (&self, key: &'static str) -> Result<&str>;
    fn get_as_table_or_invalid_key (&self, key: &'static str) -> Result<&toml::Value>;
}

impl ValueExt for toml::Value {
    fn get_as<'a, T, F>(&'a self, key: &'static str, f: F) -> Result<T>
        where F: FnOnce(&'a Self) -> Option<T>
    {
        f(
            self.get (key.split ('.').collect::<Vec<&str>>().pop().unwrap())
                .chain_err (|| ErrorKind::MissingOption(key))?
        ).chain_err (|| ErrorKind::InvalidOption(key))
    }

    fn get_as_str (&self, key: &'static str) -> Option<&str> {
        Self::get_as (self, key, toml::Value::as_str).ok()
    }

    fn get_as_str_or_invalid_key (&self, key: &'static str) -> Result<&str> {
        Self::get_as (self, key, toml::Value::as_str)
    }
    fn get_as_table_or_invalid_key (&self, key: &'static str) -> Result<&toml::Value> {
        Self::get_as (self, key, |v|
             if v.is_table() { Some(v) } else { None })
    }
}

impl Config {
    pub fn parse_config(config_path: &str, args: &ArgMatches) -> Result<Config> {
        macro_rules! arg_or_cfg_option {
            (from [$args:expr] get $arg:expr, from [$config:expr] get $option:expr) => {
                $args.and_then (|a| a.value_of ($arg))
                     .or_else (|| $config.get_as_str ($option))
                     .chain_err (|| format!(
                        "can't retrieve option '{}' from either command line arguments or config",
                        $option
                     ))
            }
        }
        // slurp the config file and parse it
        let mut config_str = String::new();
        File::open (config_path)
            .chain_err (|| format!("can't open configuration file '{}'", config_path))?
            .read_to_string (&mut config_str)
            .chain_err (|| format!("can't read configuration file '{}'", config_path))?;
        let config = config_str.parse::<toml::Value>()
            .chain_err (|| format!("can't parse configuration file '{}'", config_path))?;

        // parse logging options
        let logging = {
            let logging_table = config.get_as_table_or_invalid_key ("logging")?;
            // Determine verbosity. It can be specified in three ways, in order of priority:
            // - configuration file option "verbosity"
            // - command line argument "level"
            // - command line argument "verbose" (sets verbosity to "debug")
            let verbosity = if args.is_present ("verbose") {
                "debug"
            } else {
                arg_or_cfg_option!(
                    from [Some(args)]    get "level",
                    from [logging_table] get "logging.verbosity"
                )?
            };
            // Parse backends and their configuration.
            let backends = logging_table
                .get_as ("logging.backends", toml::Value::as_array)?
                .iter()
                .map (|backend_name| {
                    backend_name
                        .as_str()
                        .chain_err (|| "each backend name in 'logging.backends' must be a string")
                        .map (|backend_name| LogBackendConfig {
                            name: backend_name.to_string(),
                            config: logging_table.get (backend_name).map (|v| v.clone())
                        })
                })
                .collect::<Result<Vec<LogBackendConfig>>>()?;
            LogConfig {
                level: verbosity.to_string(),
                backends
            }
        };

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
                .chain_err (||
                    "can't retrieve option 'mode' from either either arguments or config")?;

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
                        .chain_err (|| "can't retrieve option 'client.action.name' from \
                                        either arguments or config")?;
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
                                                .chain_err (|| "the availability reason \
                                                                'client.action.set_availability \
                                                                .reason' is mandatory")?
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
                                        None => bail!(
                                            "availability ('config.action.set_availability \
                                            .available') is required and must be a boolean")
                                    }
                                )
                            }
                        },
                        _ => bail!("unknown client action 'client.action.name': {}", action_name)
                    };
                    Mode::Client (ClientConfig {
                        connect_to: arg_or_cfg_option!(
                            from [subcommand_args] get "connect_to",
                            from [client_table]    get "client.connect_to"
                        )?.into(),
                        action
                    })
                }
                _ => bail!("unknown run mode: {}", mode_str)
            }
        };

        Ok(Config { mode, notifier, logging })
    }
}
