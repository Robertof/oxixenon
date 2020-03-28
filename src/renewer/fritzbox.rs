use super::{Renewer as RenewerTrait, Result, ResultExt};
use crate::config;
use crate::config::ValueExt;
use crate::http_client;
use md5;

pub struct Renewer {
    ip: String,
    username: Option<String>,
    password: String,
    sid: Option<String>
}

impl Renewer {
    fn check_and_retrieve_sid(&mut self) -> Result<()> {
        info!(target: "renewer::fritzbox", "trying to login using specified credentials");

        let login_url = format!("http://{}/login_sid.lua", self.ip);

        let login_url_with_pre_existing_sid = format!("{}{}", login_url, match self.sid.as_ref() {
            None => "".into(),
            Some(sid) => format!("?sid={}", sid)
        });
        
        // This returns something like:
        // <SessionInfo>
        //   <SID>0000000000000000</SID>
        //   <Challenge>aabbccdd</Challenge>
        //   <BlockTime>0</BlockTime>
        //   <Rights/>
        // </SessionInfo>
        //
        // If SID is not all zeroes, the SID we have is already valid.
        // If BlockTime is different than 0, then a login attempt failed.
        // Challenge is used to actually perform the login.

        let res = http_client::get(&login_url_with_pre_existing_sid)
            .chain_err(|| format!("HTTP request to '{}' failed", login_url))?;
        ensure!(res.status().is_success(), "failed to request the login page");

        let body = res.body();

        // See if we already have a valid SID.
        if self.set_sid_if_valid(body).is_ok() {
            info!(target: "renewer::fritzbox", "login unnecessary - pre-existing SID is still valid");
            return Ok(());
        }

        // SID invalid, we need to login.
        let challenge = Self::extract_xml_tag(body, "Challenge")
            .chain_err(|| "failed to extract login challenge")?;

        debug!(target: "renewer::fritzbox", "challenge is {}", challenge);

        let response = {
            // Passwords needs to be encoded to UTF-16 and any codepoints above 255 needs to be
            // replaced with a dot.
            let password_bytes = format!("{}-{}", challenge, self.password)
                .chars()
                .map(|c| if c as u32 > 255 { '.' } else { c })
                .collect::<String>()
                .encode_utf16()
                .flat_map(|v| v.to_le_bytes().to_vec())
                .collect::<Vec<_>>();
            format!("{}-{:x}", challenge, md5::compute(password_bytes))
        };

        // Login is a POST request to the same url containing the parameters:
        // ["username": "...",  "response": "{challenge}-md5({challenge-pwd})"]
        let res = http_client::build_post(&login_url)
            .put("username", &self.username.as_ref().unwrap_or(&"".into()))
            .put("response", &response)
            .build_and_execute()
            .chain_err(|| format!("HTTP request to login at '{}' failed", login_url))?;

        let body = res.body();

        debug!(target: "renewer::fritzbox", "login attempt finished - blocktime is {}",
            Self::extract_xml_tag(body, "BlockTime").unwrap_or("N/A"));

        // Check SID and return if OK.
        self.set_sid_if_valid(body)
    }

    fn extract_xml_tag<'a>(source: &'a str, field: &'static str) -> Option<&'a str> {
        // This is a rough text processing function to extract content of XMl tags.
        // Find the tag itself first.
        let full_tag = format!("<{}>", field);
        let field_start = source.find(&full_tag)?;
        // Get the contents of the tag plus whatever is after.
        let field_content_unclamped = source.get((field_start + full_tag.len())..)?;
        // Find the closing bracket.
        let field_end = field_content_unclamped.find("<")?;
        // Clamp to the closing bracket to obtain just the value.
        field_content_unclamped.get(..field_end)
    }

    fn set_sid_if_valid<'a>(&mut self, document: &'a str) -> Result<()> {
        match Self::extract_xml_tag(document, "SID") {
            Some(sid) if sid.contains(|c| c != '0') => {
                self.sid = Some(sid.into());
                info!(target: "renewer::fritzbox", "login OK");
                Ok(())
            },
            _ => bail!("login failed, check your credentials!")
        }
    }
}

impl RenewerTrait for Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self> where Self: Sized {
        let config = renewer.config.as_ref()
            .chain_err(|| config::ErrorKind::MissingOption("server.renewer.fritzbox"))
            .chain_err(|| "the renewer 'fritzbox' requires to be configured")?;

        Ok(Self {
            ip:
                config.get_as_str_or_invalid_key("server.renewer.fritzbox.ip")
                    .chain_err(|| "failed to find the router's IP address in renewer 'fritzbox'")?
                    .into(),
            username: config.get_as_str("server.renewer.fritzbox.username").map(|s| s.into()),
            password:
                config.get_as_str_or_invalid_key("server.renewer.fritzbox.password")
                    .chain_err(|| "failed to find the router's password in renewer 'fritzbox'")?
                    .into(),
            sid: None
        })

    }

    fn init(&mut self) -> Result<()> {
        self.check_and_retrieve_sid()
    }

    fn renew_ip(&mut self) -> Result<()> {
        let sid = match self.sid.as_ref() {
            None => {
                self.check_and_retrieve_sid()?;
                self.sid.as_ref().expect("SID must have been correctly fetched")
            },
            Some(sid) => sid
        };

        // Try to use a pre-existing SID first.
        let disconnect_url = format!(
            "http://{}/internet/inetstat_monitor.lua?sid={}&action=disconnect&xhr=1&myXhr=1",
            self.ip, sid
        );
        let res = http_client::get(&disconnect_url)
            .chain_err(|| "HTTP request to renewal URL failed")?;

        if res.status().as_u16() == 403 {
            // Oops! Invalid SID. Invalidate it and login again.
            self.sid = None;
            return self.renew_ip();
        }

        ensure!(
            res.status().is_success(),
            "IP address renewal failed - server returned {}", res.status()
        );

        // Send a "connect" request too to speed things up. Ignore errors.
        {
            let connect_url = format!(
                "http://{}/internet/inetstat_monitor.lua?sid={}&action=connect&xhr=1&myXhr=1",
                self.ip, sid
            );
            let _ = http_client::get(&connect_url);
        }

        info!(target: "renewer::fritzbox", "successfully asked for another IP");

        Ok(())
    }
}
