use super::{Renewer as RenewerTrait, Result, ResultExt};
use crate::config;
use std::process::{Command, Stdio};

pub struct Renewer;

const CTLMGR_CTL_PATH: &str = "/usr/bin/ctlmgr_ctl";

impl RenewerTrait for Renewer {
    fn from_config (_renewer: &config::RenewerConfig) -> Result<Self>
        where Self: Sized {
        Ok(Self {})
    }

    fn init (&mut self) -> Result<()> {
        use std::path::Path;
        // Check if CTLMGR_CTL_PATH exists.
        if !Path::new (CTLMGR_CTL_PATH).is_file() {
            error!("oxixenon must be executed on your FritzBox! router for this renewer to work.");
            error!(
                "if this is the case and you are still getting this error message, please open an \
                issue: https://github.com/Robertof/oxixenon/issues"
            );
            bail!("FritzBox! renewer failed to initialize");
        }
        Ok(())
    }

    fn renew_ip (&mut self) -> Result<()> {
        macro_rules! exec_command {
            (param $arg:expr, error_msg $err:expr) => {
                Command::new (CTLMGR_CTL_PATH)
                        .args (&["w", "connection0", $arg, ""])
                        .stdout (Stdio::null())
                        .stderr (Stdio::null())
                        .status()
                        .chain_err (|| $err)
                        .and_then (|status| if status.success() {
                            Ok(())
                        } else {
                            bail!("{}: got status {}", $err, status)
                        })
            }
        }
        exec_command!(param "settings/cmd_disconnect", error_msg "failed to disconnect network")?;
        exec_command!(param "settings/cmd_connect",    error_msg "failed to reconnect network")
    }
}
