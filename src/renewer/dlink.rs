extern crate hmac;
extern crate sha2;

use super::{Error, Renewer as RenewerTrait};
use config;
use config::ValueExt;
use http_client;
use self::hmac::{Hmac, Mac};
use self::sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct Renewer {
    ip: String,
    username: String,
    password: String,
    interface: String,
    sid_cookie: Option<String>,
    try_count: u8
}

impl Renewer {
    fn login (&mut self) -> Result<(), Error> {
        info!(target: "renewer::dlink", "trying to login using specified credentials");
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
        trace!(target: "renewer::dlink", "extracted nonce = {}, csrf_tok = {}", nonce, csrf_tok);
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
        info!(target: "renewer::dlink", "login OK, redirected to {}",
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

impl RenewerTrait for Renewer {
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

        Ok(Self {
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
        let renewal_url = format!("http://{}/ui/dboard/settings/netif/{}&action=reset",
            self.ip, self.interface);

        let mut request = http_client::Request::builder();
        {
            let sid_cookie = match self.sid_cookie {
                Some(ref value) => {
                    debug!(target: "renewer::dlink", "trying to reuse existing sid to renew");
                    value
                },
                None => {
                    self.login()?;
                    self.sid_cookie.as_ref().expect ("sid must be present after login")
                }
            };
            request.uri (renewal_url).header ("Cookie", sid_cookie.as_str());
        }
        
        let request = http_client::make_request (request.body(None::<String>).unwrap())?;

        if !request.status().is_redirection() {
            debug!(target: "renewer::dlink", "renew failed: got status code {}", request.status());
            return Err (Error::Generic ("Expected redirect when renewing, got other status code"));
        }

        // get redirect path
        match request.headers()[http_client::header::LOCATION].to_str().unwrap() {
            "/ui/login" => {
                if self.try_count >= 3 {
                    return Err (Error::Generic (
                        "Too many retries - are you sure the credentials are correct?"));
                }
                debug!(target: "renewer::dlink", "sid expired. clearing and re-running");
                self.sid_cookie = None;
                self.try_count += 1;
                return self.renew_ip();
            },
            path @ _ => {
                self.try_count = 0;
                trace!(target: "renewer::dlink", "redirected to \"{}\", assuming success", path);
                info!(target: "renewer::dlink", "successfully asked for another IP");
            }
        }
        Ok(())
    }
}
