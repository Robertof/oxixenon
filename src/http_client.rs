//! A basic HTTP client.
//! 
//! You may ask: "why didn't you use Reqwest or Hyper?" The answer is that I didn't want to bundle
//! all the dependencies required by Hyper, so I implemented it by myself.
//! 
//! **Note:** no advanced HTTP features are implemented (such as chunking)!

extern crate http;

use std::{fmt, io, error, time};
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use http::Response;
use http::header::{HeaderValue};

pub use http::header;
pub use http::Request;

const FIVE_SECONDS: time::Duration = time::Duration::from_secs(5);

/// Errors that can occur when making an HTTP request.
#[derive(Debug)]
pub enum Error {
    /// An error which comes from the `http` crate.
    Http(http::Error),
    /// An HTTP protocol error.
    Protocol(&'static str),
    /// A generic I/O error.
    Io(io::Error),
    /// A generic error with an associated message.
    Other(String)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Http(ref s) => write!(f, "HTTP error: {}", s),
            Error::Protocol(ref s) => write!(f, "HTTP protocol error: {}", s),
            Error::Io(ref s) => write!(f, "I/O error: {}", s),
            Error::Other(ref s) => write!(f, "Error: {}", s)
        }
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Http(ref e) => Some(e),
            Error::Protocol(_) => None,
            Error::Io(ref e) => Some(e),
            Error::Other(_) => None
        }
    }
}

impl From<io::Error> for Error {
    fn from (error: io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<http::Error> for Error {
    fn from (error: http::Error) -> Self {
        Error::Http(error)
    }
}

type RequestBody = String;

/// A trait for objects which can be converted to `RequestBody` (`String`) values.
pub trait ToRequestBody {
    /// Converts this object to a `RequestBody`.
    fn to_request_body(self) -> RequestBody;
    /// The length that this object will have once converted.
    fn len(&self) -> usize;
}

impl ToRequestBody for String {
    fn to_request_body(self) -> RequestBody { return self }
    fn len(&self) -> usize { return self.len() }
}

impl<'a> ToRequestBody for HashMap<&'a str, &'a str>
{
    fn to_request_body(self) -> RequestBody {
        let mut output = String::new();
        for (key, value) in self.iter() {
            // TODO: perform proper urlencoding
            output += format!("{}={}&", key, value).as_str();
        }
        output.pop();
        output
    }
    fn len(&self) -> usize {
        self.len() * 2 + self.iter().map (|(k, v)| k.len() + v.len()).sum::<usize>() - 1
    }
}

/// Performs an HTTP request with a [`Request<Option<T>>`](struct.Request.html) object.
pub fn make_request<T>(mut request: Request<Option<T>>) -> Result<Response<String>, Error>
    where T: ToRequestBody
{
    let stream = each_addr (
        (request.uri().host().unwrap(), request.uri().port().unwrap_or (80)),
        |addr| TcpStream::connect_timeout (&addr, FIVE_SECONDS)
    )?;
    stream.set_read_timeout (Some (FIVE_SECONDS))?;
    let reader = io::BufReader::new (&stream);
    let mut writer = io::BufWriter::new (&stream);

    {
        let path = request.uri().path_and_query().map (|p| p.as_str()).unwrap_or ("/");
        debug!("<http_client> requesting {} {}", request.method(), path);
        // begin writing our HTTP request
        write!(writer, "{method} {path} {protocol}\r\n",
            method = request.method(),
            path = path,
            protocol = "HTTP/1.1")?;
    }

    // fixup headers
    if !request.headers().contains_key (header::HOST) {
        let host_header_port = request
            .uri()
            .port()
            .map (|p| format!(":{}", p))
            .unwrap_or ("".into());
        let host_header = HeaderValue::from_str (format!(
            "{}{}",
            request.uri().host().unwrap(),
            host_header_port
        ).as_str()).map_err (|e| http::Error::from(e))?;
        request.headers_mut().insert (header::HOST, host_header);
    }
    let is_post = http::Method::POST == *request.method();
    if is_post {
        let body_len = request.body()
            .as_ref()
            .expect ("Missing request body in POST request")
            .len();
        request.headers_mut().insert (
            header::CONTENT_LENGTH,
            body_len.into()
        );
        if !request.headers().contains_key (header::CONTENT_TYPE) {
            request.headers_mut().insert (
                header::CONTENT_TYPE,
                HeaderValue::from_static ("application/x-www-form-urlencoded")
            );
        }
    }
    request.headers_mut().insert (header::CONNECTION, HeaderValue::from_static ("close"));

    // write headers
    for (key, value) in request.headers().iter() {
        let value = value.to_str().map_err (|e| Error::Other (e.to_string()))?;
        debug!("<http_client> request header: {} => {}", key.as_str(), value);
        write!(writer, "{}: {}\r\n", key.as_str(), value)?;
    }
    
    write!(writer, "\r\n")?;

    if is_post {
        // write body
        let body = request
            .into_body()
            .unwrap()
            .to_request_body();
        write!(
            writer,
            "{}{newline}",
            body,
            newline = if body.ends_with ("\r\n") { "" } else { "\r\n" }
        )?;
    }

    writer.flush()?;

    // read the HTTP response
    let mut line_counter = 0;
    let mut response_builder = Response::builder();
    let mut expecting_headers = true;
    let mut body = String::new();
    debug!("<http_client> waiting for a response...");
    for line in reader.lines() {
        let line = line?;
        if line_counter == 0 && !line.starts_with ("HTTP/") {
            continue;
        }
        line_counter += 1;
        match line_counter {
            1 => {
                let status_code = line
                    .split_whitespace()
                    .nth (1)
                    .ok_or (Error::Protocol ("invalid status code"))?;
                debug!("<http_client> received status code: {}", status_code);
                response_builder.status (status_code);
            },
            _ if line.is_empty() && expecting_headers => {
                expecting_headers = false
            },
            _ if expecting_headers => {
                let mut iterator = line.splitn (2, ":");
                let (header_name, header_value) = (
                    iterator.next().ok_or (Error::Protocol ("expecting header name"))?.trim(),
                    iterator.next().ok_or (Error::Protocol ("expecting header value"))?.trim()
                );
                debug!("<http_client> response header: {} => {}", header_name, header_value);
                response_builder.header (
                    header_name,
                    header_value
                );
            },
            _ => {
                body += (line + "\n").as_str()
            }
        }
    }
    Ok(response_builder.body (body)?)
}

/// Performs a `GET` request to a given URI.
pub fn get (uri: &str) -> Result<Response<String>, Error> {
    let req: Request<Option<String>> = Request::builder().uri (uri).body (None)?;
    make_request (req)
}

/// Starts building a `POST` request to a given URI.
pub fn build_post<'a>(uri: &'a str) -> PostRequestBuilder<'a> {
    apply_to (PostRequestBuilder::new(), |b| b.uri(uri))
}

/// A builder for HTTP `POST` requests.
pub struct PostRequestBuilder<'a> {
    builder: http::request::Builder,
    data: Option<HashMap<&'a str, &'a str>>
}

impl<'a> PostRequestBuilder<'a> {
    /// Creates a new builder.
    pub fn new() -> PostRequestBuilder<'a> {
        PostRequestBuilder {
            builder: apply_to (Request::builder(), |b| b.method (http::Method::POST)),
            data: Some(HashMap::new())
        }
    }

    /// Returns a mutable reference to the associated `Builder` object.
    pub fn builder(&mut self) -> &mut http::request::Builder {
        &mut self.builder
    }

    /// Sets the URI of this request builder.
    pub fn uri (&mut self, uri: &'a str) -> &mut Self {
        self.builder.uri (uri);
        self
    }

    /// Adds an element to the `application/x-www-form-urlencoded` fields of this builder.
    pub fn put (&mut self, key: &'a str, value: &'a str) -> &mut Self {
        self.data.as_mut().expect ("PostRequestBuilder already used").insert (key, value);
        self
    }

    /// Consumes this builder and produces a `Request<T>` with a type suitable for use in
    /// `make_request`.
    pub fn build (&mut self) -> http::Result<Request<Option<HashMap<&'a str, &'a str>>>> {
        let map = self.data.take().expect ("PostRequestBuilder already used");
        self.builder.body (if map.is_empty() { None } else { Some (map) })
    }

    /// Consumes this builder and executes the built request.
    pub fn build_and_execute (&mut self) -> Result<Response<String>, Error> {
        let request = self.build()?;
        make_request (request)
    }
}

fn apply_to<T, F>(mut val: T, f: F) -> T
    where F: FnOnce(&mut T) -> &T
{
    f(&mut val);
    val
}

// taken from std/net/mod.rs
fn each_addr<A: ToSocketAddrs, F, T>(addr: A, mut f: F) -> io::Result<T>
    where F: FnMut(&SocketAddr) -> io::Result<T>
{
    let mut last_err = None;
    for addr in addr.to_socket_addrs()? {
        match f(&addr) {
            Ok(l) => return Ok(l),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput,
                   "could not resolve to any addresses")
    }))
}