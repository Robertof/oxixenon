use config;
use std::marker::Sized;

error_chain! {
    links {
        Config(config::Error, config::ErrorKind);
    }
}

// Available renewers. They also need to be specified in `get_renewer()`.
#[cfg(feature = "renewer-dlink")] mod dlink;
mod dummy;

pub trait Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self>
        where Self: Sized;
    fn init(&mut self) -> Result<()> { Ok(()) }
    fn renew_ip(&mut self) -> Result<()>;
}

pub fn get_renewer (renewer: &config::RenewerConfig) -> Result<Box<Renewer>> {
    macro_rules! renewer_from_config {
        ($name: path) => {
            <$name>::from_config (renewer).map (|v| Box::new(v) as Box<Renewer>)
        }
    }
    match renewer.name.as_str() {
        #[cfg(feature = "renewer-dlink")] "dlink" => renewer_from_config!(dlink::Renewer),
        "dummy" => renewer_from_config!(dummy::Renewer),
        _ => bail!(
            "invalid renewer name '{}' - if applicable, ensure this renewer is enabled",
            renewer.name
        )
    }
}
