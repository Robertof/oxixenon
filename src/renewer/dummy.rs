use super::{Renewer as RenewerTrait, Result};
use config;

pub struct Renewer;
impl RenewerTrait for Renewer {
    fn from_config (_renewer: &config::RenewerConfig) -> Result<Self>
        where Self: Sized
    {
        Ok(Renewer)
    }
    fn renew_ip (&mut self) -> Result<()> {
        Ok(())
    }
}

