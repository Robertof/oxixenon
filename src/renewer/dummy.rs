use super::{Error, Renewer as RenewerTrait};
use config;

pub struct Renewer;
impl RenewerTrait for Renewer {
    fn from_config (_renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized
    {
        Ok(Renewer)
    }
    fn renew_ip (&mut self) -> Result<(), Error> {
        Ok(())
    }
}

