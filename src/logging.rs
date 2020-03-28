extern crate chrono;
extern crate fern;
extern crate log;
#[cfg(all(not(windows), feature = "syslog-backend"))]
extern crate syslog;

use crate::errors::*;
use std::{io, fmt};
use log::LevelFilter;
use crate::config::{ValueExt, LogConfig};

#[macro_export]
macro_rules! log_error_with_chain {
    (target: $target:expr, $level:expr, $error:ident, $($arg:tt)+) => {
        log!(target: $target, $level, $($arg)+);
        for err in $error.iter().skip(1) {
            log!(target: $target, $level, "- caused by: {}", err);
        }
    };
    ($level:expr, $error:ident, $($arg:tt)+) =>
        (log_error_with_chain!(target: module_path!(), $level, $error, $($arg)+));
    ($error:ident, $($arg:tt)+) =>
        (log_error_with_chain!(log::Level::Error, $error, $($arg)+));
}

/// Initializes the global logger with the user-specified configuration.
pub fn init (config: &LogConfig) -> Result<()> {
    let log_level: LevelFilter = config.level.parse()
        .chain_err (|| format!("invalid option 'logging.verbosity': {}", config.level))?;
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
                let log_path = backend.config.as_ref()
                    .chain_err (|| "the logging backend 'file' requires to be configured")?
                    .get_as_str_or_invalid_key ("logging.file.path")
                    .chain_err (|| "the logging backend 'file' requires a log path")?;
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
                        .chain (
                            fern::log_file (
                                // Log to the specified path.
                                log_path
                            ).chain_err (|| format!("can't open log file '{}'", log_path))?
                        )
                )
            },
            #[cfg(all(not(windows), feature = "syslog-backend"))]
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
                                syslog::unix_custom (formatter, socket_path)
                            } else {
                                syslog::unix (formatter)
                            }
                        },
                        Some("tcp") => {
                            syslog::tcp (
                                formatter,
                                config.get_as_str_or_invalid_key ("logging.syslog.server_addr")
                                    .chain_err (|| "syslog TCP protocol requires a server addr")?
                            )
                        },
                        Some("udp") => {
                            syslog::udp (
                                formatter,
                                config.get_as_str_or_invalid_key ("logging.syslog.local_addr")
                                    .chain_err (|| "syslog UDP protocol requires a local addr")?,
                                config.get_as_str_or_invalid_key ("logging.syslog.server_addr")
                                    .chain_err (|| "syslog UDP protocol requires a server addr")?
                            )
                        },
                        Some(val) => bail!(
                            "invalid value '{}' for option 'logging.syslog.protocol', \
                            must be one of 'unix', 'tcp', 'udp'",
                            val
                        ),
                        None => syslog::unix (formatter)
                    }
                } else {
                    syslog::unix (formatter)
                }.chain_err (|| "syslog initialization error")?)
            },
            _ => bail!(
                "unknown logging backend '{}', if it exists, make sure it is enabled",
                backend.name
            )
        }
    }
    fern.apply().chain_err (|| "can't initialize the main logger")?;
    Ok(())
}
