extern crate hmac;
extern crate sha2;

use super::{Renewer as RenewerTrait, Result, ResultExt};
use crate::config;
use crate::config::ValueExt;
use crate::http_client;
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
    fn login (&mut self) -> Result<()> {
        info!(target: "renewer::dlink", "trying to login using specified credentials");
        let login_url = format!("http://{}/ui/login", self.ip);
        let res = http_client::get (login_url.as_str())
            .chain_err (|| format!("HTTP request to '{}' failed", login_url))?;
        ensure!(res.status().is_success(), "failed to request the login page");
        let mut lines = res.body().lines();
        // TODO: regexps are much better for this purpose. But too many dependencies, argh!
        let nonce = lines.find (|l| l.contains ("\"nonce\" value=\""));
        let nonce = Self::_extract_field_value (nonce, '"')
            .chain_err (|| "failed to extract 'nonce' from the login page")?;
        let csrf_tok = lines.next();
        let csrf_tok = Self::_extract_field_value (csrf_tok, '\'')
            .chain_err (|| "failed to extract 'csrf token' from the login page")?;
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
            .build_and_execute()
            .chain_err (|| format!("HTTP request to login at '{}' failed", login_url))?;

        ensure!(
            res.status().is_redirection(),
            "failed to login, got status '{}' instead of redirection", res.status()
        );

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
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self>
        where Self: Sized {
        let config = renewer.config.as_ref()
            .chain_err (|| config::ErrorKind::MissingOption ("server.renewer.dlink"))
            .chain_err (|| "the renewer 'dlink' requires to be configured")?;        
        let interface = config
            .get_as_str_or_invalid_key ("server.renewer.dlink.interface")
            .chain_err (|| "failed to find the interface to renew in renewer 'dlink'")?
            .to_owned();
        // since interface is directly passed inside an URL, ensure it doesn't have any invalid
        // characters
        ensure!(
            !interface.contains(|c: char|
                !c.is_ascii_alphabetic() && !c.is_ascii_digit() && c != '?' && c != '='
            ),
            "option 'server.renewer.dlink.interface' contains invalid characters, allowed: {}",
            "a-z, 0-9, ?, ="
        );

        Ok(Self {
            ip:
                config.get_as_str_or_invalid_key ("server.renewer.dlink.ip")
                    .chain_err (|| "failed to find the router's IP address in renewer 'dlink'")?
                    .into(),
            username:
                config.get_as_str_or_invalid_key ("server.renewer.dlink.username")
                    .chain_err (|| "failed to find the router's username in renewer 'dlink'")?
                    .into(),
            password:
                config.get_as_str_or_invalid_key ("server.renewer.dlink.password")
                    .chain_err (|| "failed to find the router's password in renewer 'dlink'")?
                    .into(),
            interface,
            sid_cookie: None,
            try_count: 0
        })
    }

    fn init (&mut self) -> Result<()> {
        // Request the router's page and try to login using the specified credentials.
        self.login()
    }

    fn renew_ip(&mut self) -> Result<()> {
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
            request = request.uri (renewal_url.as_str()).header ("Cookie", sid_cookie.as_str());
        }
        
        let request = http_client::make_request (request.body(None::<String>).unwrap())
            .chain_err (|| format!("HTTP request to '{}' failed", renewal_url))?;

        ensure!(
            request.status().is_redirection(),
            "failed to renew the IP address, got status {}",
            request.status()
        );

        // get redirect path
        match request.headers()[http_client::header::LOCATION].to_str().unwrap() {
            "/ui/login" => {
                ensure!(
                    self.try_count < 3,
                    "failed to renew the IP address, too many retries - credentials are OK?"
                );
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
