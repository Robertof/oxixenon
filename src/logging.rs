extern crate chrono;
extern crate fern;
extern crate log;
#[cfg(feature = "syslog-backend")]
extern crate syslog;

use std::{io, error, fmt};
use log::LevelFilter;
use config::{ValueExt, LogConfig, Error as ConfigError};

/// Initializes the global logger with the user-specified configuration.
pub fn init (config: &LogConfig) -> Result<(), Box<error::Error>> {
    let log_level: LevelFilter = config.level.parse().map_err (
        |_| ConfigError::InvalidOptionWithReason("logging.verbosity", "invalid log level"))?;
    let mut fern = fern::Dispatch::new().level (log_level);
    // Used to display data on "stdout". `file` uses a slightly different formatter which also
    // displays the date.
    let standard_formatter = |out: fern::FormatCallback, message: &fmt::Arguments, record: &log::Record| {
        // 12:34:56 INFO <module> message
        out.finish (format_args!(
            "{} {} <{}> {}",
            chrono::Local::now().format("%H:%M:%S"),
            record.level(),
            record.target().replace ("oxixenon::", ""),
            message
        ))
    };
    for backend in &config.backends {
        fern = match backend.name.as_str() {
            "stdout" => {
                fern
                    .chain (
                        // Log only errors to STDERR.
                        fern::Dispatch::new()
                            .format (standard_formatter)
                            .level (LevelFilter::Error)
                            .chain (io::stderr())
                    )
                    .chain (
                        // Log everything else to STDOUT.
                        fern::Dispatch::new()
                            .format (standard_formatter)
                            .filter (|metadata| metadata.level() != LevelFilter::Error)
                            .chain (io::stdout())
                    )
            },
            "file" => {
                fern.chain (
                    fern::Dispatch::new()
                        .format (|out, message, record| {
                            // 1970-01-01 12:34:56 INFO <module> message
                            out.finish (format_args!(
                                "{} {} <{}> {}",
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                                record.level(),
                                record.target().replace ("oxixenon::", ""),
                                message
                            ))
                        })
                        .chain (fern::log_file (
                            // Log to the specified path.
                            backend
                                .config
                                .as_ref()
                                .ok_or (ConfigError::InvalidOption("logging.file"))?
                                .get_as_str_or_invalid_key ("logging.file.path")?
                            )?
                        )
                )
            },
            #[cfg(feature = "syslog-backend")]
            "syslog" => {
                use std::process;
                let config = backend.config.as_ref();
                let formatter = syslog::Formatter3164 {
                    facility: syslog::Facility::LOG_DAEMON,
                    hostname: config
                        .and_then (|c| c.get_as_str ("logging.syslog.hostname"))
                        .map      (|h| h.to_string()),
                    pid: process::id() as i32,
                    process: "oxixenon".into()
                };
                // Process all the available syslog protocol options.
                fern.chain (if let Some(config) = config {
                    match config.get_as_str ("logging.syslog.protocol") {
                        Some("unix") => {
                            if let Some(socket_path) =
                                config.get_as_str ("logging.syslog.unix_socket_path")
                            {
                                syslog::unix_custom (formatter, socket_path)?
                            } else {
                                syslog::unix (formatter)?
                            }
                        },
                        Some("tcp") => {
                            syslog::tcp (
                                formatter,
                                config.get_as_str_or_invalid_key ("logging.syslog.server_addr")?
                            )?
                        },
                        Some("udp") => {
                            syslog::udp (
                                formatter,
                                config.get_as_str_or_invalid_key ("logging.syslog.local_addr")?,
                                config.get_as_str_or_invalid_key ("logging.syslog.server_addr")?
                            )?
                        },
                        Some(_) => Err(ConfigError::InvalidOptionWithReason(
                            "logging.syslog.protocol",
                            "must be one of 'unix', 'tcp', 'udp'"
                        ))?,
                        None => syslog::unix (formatter)?
                    }
                } else {
                    syslog::unix (formatter)?
                })
            },
            _ => Err(ConfigError::InvalidOptionWithReason(
                "logging.backends", "unknown backend - if it exists, ensure it is enabled"
            ))?
        }
    }
    fern.apply()?;
    Ok(())
}
