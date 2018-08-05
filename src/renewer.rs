extern crate hmac;
extern crate sha2;

use ::config;
use ::http_client;
use config::ValueExt;
use std::fmt;
use std::marker::Sized;
use std::error::Error as StdError;
use sha2::Sha256;
use hmac::{Hmac, Mac};

type HmacSha256 = Hmac<Sha256>;

/* <renewer::Error> */
#[derive(Debug)]
pub enum Error {
    Generic(&'static str),
    Http(http_client::Error),
    Config(config::Error)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Http(ref s) => write!(f, "Renewer: HTTP error: {}", s),
            Error::Generic(ref s) => write!(f, "Renewer: Error: {}", s),
            Error::Config(ref s) => write!(f, "Renewer: Config error: {}", s)
        }
    }
}

impl StdError for Error {
    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::Http(ref e) => Some(e),
            Error::Generic(_) => None,
            Error::Config(ref e) => Some(e)
        }
    }
}

impl From<config::Error> for Error {
    fn from(error: config::Error) -> Self {
        Error::Config(error)
    }
}

impl From<http_client::Error> for Error {
    fn from(error: http_client::Error) -> Self {
        Error::Http(error)
    }
}
/* </renewer::Error> */

pub trait Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized;
    fn init(&mut self) -> Result<(), Error> { Ok(()) }
    fn renew_ip(&mut self) -> Result<(), Error>;
}

struct DlinkRenewer {
    ip: String,
    username: String,
    password: String,
    interface: String,
    sid_cookie: Option<String>,
    try_count: u8
}

impl DlinkRenewer {
    fn login (&mut self) -> Result<(), Error> {
        eprintln!("<renewer::dlink> trying to login using specified credentials");
        let login_url = format!("http://{}/ui/login", self.ip);
        let res = http_client::get (login_url.as_str())?;
        if !res.status().is_success() {
            return Err(Error::Generic("Login request was not successful!"));
        }
        let mut lines = res.body().lines();
        // TODO: regexps are much better for this purpose. But too many dependencies, argh!
        let nonce = lines.find (|l| l.contains ("\"nonce\" value=\""));
        let nonce = Self::_extract_field_value (nonce, '"').ok_or (
            Error::Generic("Can't find nonce")
        )?;
        let csrf_tok = lines.next();
        let csrf_tok = Self::_extract_field_value (csrf_tok, '\'').ok_or (
            Error::Generic ("Can't find CSRF token")
        )?;
        debug!("<renewer::dlink> extracted nonce = {}, csrf_tok = {}", nonce, csrf_tok);
        // Encrypt the password with the retrieved nonce
        let mut mac = HmacSha256::new_varkey (nonce.as_bytes()).expect ("Can't create HmacSha256");
        mac.input (self.password.as_bytes());

        let hashed_pwd: String = mac
            .result()
            .code()
            .into_iter()
            .map (|b| format!("{:02x}", b)) // convert bytes to lower-case hex nibbles
            .collect();

        // We're ready to try our login.
        let res = http_client::build_post (login_url.as_str())
            .put ("code1", csrf_tok)
            .put ("language", "IT")
            .put ("login", "Login")
            .put ("nonce", nonce)
            .put ("userName", self.username.as_str())
            .put ("userPwd", hashed_pwd.as_str())
            .build_and_execute()?;

        if !res.status().is_redirection() {
            return Err(Error::Generic("Login failed!"));
        }

        let headers = res.headers();
        eprintln!("<renewer::dlink> login OK, redirected to {}",
            headers[http_client::header::LOCATION].to_str().unwrap());

        self.sid_cookie = headers[http_client::header::SET_COOKIE]
            .to_str()
            .ok()
            .and_then (|s| s.split (";").next())
            .map (|s| s.to_owned());

        Ok(())
    }

    // given <input name="..." value="abc" /> and " returns abc
    // NOTE: does not work with escaped values. e.g. <... value="abc\"def" />
    fn _extract_field_value (input: Option<&str>, delimiter: char) -> Option<&str> {
        let pattern = format!("value={}", delimiter);
        let mut split = input?.split (pattern.as_str());
        split.nth (1)?.split (delimiter).nth(0)
    }
}

impl Renewer for DlinkRenewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized {
        let config = renewer.config.as_ref().ok_or (
            config::Error::InvalidOption ("server.renewer.dlink"))?;
        
        // since interface is directly passed inside an URL, ensure it doesn't have any invalid
        // characters
        let interface =
            config.get_as_str_or_invalid_key("server.renewer.dlink.interface")?.to_owned();

        if interface.contains(|c: char|
            !c.is_ascii_alphabetic() && !c.is_ascii_digit() && c != '?' && c != '=')
        {
            return Err (config::Error::InvalidOptionWithReason (
                "server.renewer.dlink.interface",
                "invalid characters (allowed: a-z, 0-9, ?, =)"
            ).into());
        }

        Ok(DlinkRenewer {
            ip:       config.get_as_str_or_invalid_key ("server.renewer.dlink.ip")?.into(),
            username: config.get_as_str_or_invalid_key ("server.renewer.dlink.username")?.into(),
            password: config.get_as_str_or_invalid_key ("server.renewer.dlink.password")?.into(),
            interface,
            sid_cookie: None,
            try_count: 0
        })
    }
    fn init (&mut self) -> Result<(), Error> {
        // Request the router's page and try to login using the specified credentials.
        self.login()
    }
    fn renew_ip(&mut self) -> Result<(), Error> {
        // try to request the ip renewal page. If we're redirected to the login page,
        // then we need to login again as the sid has expired.
        debug!("<renewer::dlink> trying to reuse existing sid to renew");
        let renewal_url = format!("http://{}/ui/dboard/settings/netif/{}&action=reset",
            self.ip, self.interface);

        let mut request = http_client::Request::builder();
        {
            let sid_cookie = match self.sid_cookie {
                Some(ref value) => value,
                None => {
                    self.login()?;
                    self.sid_cookie.as_ref().expect ("sid must be present after login")
                }
            };
            request.uri (renewal_url).header ("Cookie", sid_cookie.as_str());
        }
        
        let request = http_client::make_request (request.body(None::<String>).unwrap())?;

        if !request.status().is_redirection() {
            debug!("<renewer::dlink> renew failed: got status code {}", request.status());
            return Err (Error::Generic ("Expected redirect when renewing, got other status code"));
        }

        // get redirect path
        match request.headers()[http_client::header::LOCATION].to_str().unwrap() {
            "/ui/login" => {
                if self.try_count >= 3 {
                    eprintln!("<renewer::dlink> ran out of tries, aborting renewal");
                    return Err (Error::Generic (
                        "Too many retries - are you sure the credentials are correct?"));
                }
                debug!("<renewer::dlink> sid expired. clearing and re-running");
                self.sid_cookie = None;
                self.try_count += 1;
                return self.renew_ip();
            },
            path @ _ => {
                self.try_count = 0;
                debug!("<renewer::dlink> redirected to \"{}\", assuming success", path);
                eprintln!("<renewer::dlink> successfully asked for another IP");
            }
        }
        Ok(())
    }
}

struct DummyRenewer;
impl Renewer for DummyRenewer {
    fn from_config (_renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized
    {
        Ok(DummyRenewer)
    }
    fn renew_ip (&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub fn get_renewer (renewer: &config::RenewerConfig) -> Result<Box<Renewer>, Error> {
    macro_rules! renewer_from_config {
        ($name: ident) => {
            $name::from_config (renewer).map (|v| Box::new(v) as Box<Renewer>)
        }
    }
    match renewer.name.as_str() {
        "dlink" => renewer_from_config!(DlinkRenewer),
        "dummy" => renewer_from_config!(DummyRenewer),
        _ => Err(Error::Generic("invalid renewer name -- must be one of 'dlink', 'dummy'"))
    }
}
