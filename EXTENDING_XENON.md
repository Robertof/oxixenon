# Extending oxixenon

## Table of contents

1. [Introduction](#introduction)
2. [Creating a new renewer](#creating-a-new-renewer)
3. [Creating a new notifier](#creating-a-new-notifier)

## Introduction

The core components of Xenon (the `renewers` and the `notifiers`) are designed to be generic and
expansible. This document details how to create a new renewer and a new notifier.

## Creating a new renewer

Renewers are defined in `renewers.rs`. The trait which defines a renewer is the the following:

```rust
trait Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized;
    fn init(&mut self) -> Result<(), Error> { Ok(()) }
    fn renew_ip(&mut self) -> Result<(), Error>;
}
```

Let's cover all the functions one by one.

### `from_config(renewer: &config::RenewerConfig) -> Result<Self, Error>`

First, let's examine `config::RenewerConfig`:

```rust
struct RenewerConfig {
    pub name: String,
    pub config: Option<toml::Value>
}
```

If your renewer requires to be configured (with a special `server.renewer.[renewer_name]` config
section), you can read any kind of parameter from the configuration using the methods from
[`toml::Value`](https://docs.rs/toml/latest/toml/value/enum.Value.html), which you can then store
inside your `struct`.

To make it easier to do that, `config.rs` defines an extension of `toml::Value` (which is already
imported in `renewer.rs`) which, among others, provides the following methods:

```rust
fn get_as_str_or_invalid_key (&self, key: &'static str) -> Result<&str, Error>;
fn get_as_table_or_invalid_key (&self, key: &'static str) -> Result<&toml::Value, Error>;
```

These methods accept configuration keys with fully qualified names
(e.g. `server.renewer.[renewer_name].option`), but doing so does not actually traverse the
configuration object -- the "extended" string is used to provide accurate error messages. An error
message specifying the full configuration key that's wrong/missing is better than one specifying
just the local key (with no context).

Now let's start building an imaginary renewer named `acme`, for routers manufactured by Acme Corp.
The expected configuration for it is:

```toml
[server.renewer.acme]
url = "http://super-private-acme-router-but-without-https.example.com/acme/renew_ip"
username = "some username"
password = "some password"
```

Here's how the definition and implementation of `parse_config` may look:

```rust
struct AcmeRenewer {
    url: String,
    username: String,
    password: String
}

impl Renewer for AcmeRenewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self, Error>
        where Self: Sized {
        // get the configuration or die with a configuration error
        let config = renewer.config.as_ref().ok_or (
            config::Error::InvalidOption ("server.renewer.acme"))?;
        // we can just use the `get_as_str_or_invalid_key` to handle errors for us
        Ok (AcmeRenewer {
            url: config.get_as_str_or_invalid_key ("server.renewer.acme.url")?.into(),
            username: config.get_as_str_or_invalid_key ("server.renewer.acme.username")?.into(),
            password: config.get_as_str_or_invalid_key ("server.renewer.acme.password")?.into(),
        })
    }
    // ...
}
```

### `init(&mut self) -> Result<(), Error>`

This method is **optional** and is used if you want to perform any kind of initialization after
the configuration has been successfully loaded. For example, you might want to check if the
credentials supplied by the user are correct.

### `renew_ip(&mut self) -> Result<(), Error>`

This is the core function of the renewer which, as the name implies, performs an IP renewal.
In our imaginary Acme renewer, we would make an HTTP request (possibly using the built-in
`http_client` HTTP client) to the endpoint specified in the configuration.

### Wrapping up the renewer

Once all the required methods have been implemented, the last step to perform is to add a new entry
to the `get_renewer` function. Thanks to macros(TM), this should be easy. If the `get_renewer`
function looks like this:

```rust
pub fn get_renewer (renewer: &config::RenewerConfig) -> Result<Box<Renewer>, Error> {
    [...]
        "dlink" => renewer_from_config!(DlinkRenewer),
        "dummy" => renewer_from_config!(DummyRenewer),
    [...]
}
```

Our imaginary Acme Corp renewer can be added by making it look like this:

```rust
pub fn get_renewer (renewer: &config::RenewerConfig) -> Result<Box<Renewer>, Error> {
    [...]
        "dlink" => renewer_from_config!(DlinkRenewer),
        "dummy" => renewer_from_config!(DummyRenewer),
        "acme"  => renewer_from_config!(AcmeRenewer)
    [...]
}
```

## Creating a new notifier

The basic structure of a notifier is very similar to the one of a renewer. Notifiers are defined
in `notifiers.rs`, and the trait which defines a notifier is the following:

```rust
pub trait Notifier {
    fn from_config (notifier: &config::NotifierConfig) -> Result<Self, Error>
        where Self: Sized;
    fn notify (&mut self, event: Event) -> Result<(), Error>;
    fn listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error>;
}
```

### `from_config (notifier: &config::NotifierConfig) -> Result<Self, Error>`

This function works and is implemented the same way as `Renewer::from_config` is implemented.
Check out the docs above to see how it's done.

### `notify (&mut self, event: Event) -> Result<(), Error>`

This method notifies an event. Let's take a look at the `Event` enum, defined in `protocol.rs`:

```rust
pub enum Event {
    IPRenewed = 0
}
```

The number associated to the enum members is the event number, which is used when wrapping the
raw event inside a packet of type `Packet::Event(_)`.

You can use the built-in packet serialization utilities to pack the event in an array of bytes, and
later decode the array of bytes (retrieved from your source) back to a `Packet::Event(_)`.

Example:

```rust
struct ImaginaryNotifier;

impl Notifier for ImaginaryNotifier {
    // ...
    fn notify (&mut self, event: Event) -> Result<(), Error> {
        // holds the raw bytes of the packet we're going to pack
        let mut vec: Vec<u8> = Vec::new();
        Packet::Event (event).write (&mut vec)?;
        // do anything with `vec`...
        Ok(())
    }
}
```

### `listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error>`

This method is called by the client when it is told to listen to notifications. You can return
an error of type `Error::Generic` if your notifier shouldn't support listening for notifications.

Example implementation:

```rust
// assuming the previous definition and partial implementation of ImaginaryNotifier

impl Notifier for ImaginaryNotifier {
    // ...
    fn listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error> {
        // loop to read data indefinitely
        // create a buffer to hold the data read from somewhere
        let mut buf = vec![0; 3]; // 3 bytes is OK
        // ...read data to buf...
        match Packet::read (&mut buf) {
            Ok(packet) => {
                if let Packet::Event(event) = packet {
                    // got event `event`! we don't know where it came from though
                    on_event(event, None)
                }
            },
            Err(error) => panic!() // not production ready!
        }
    }
}
```

### Wrapping up the notifier

Follow the instructions on how to wrap up a renewer above. In the end, `notifier::get_notifier`
should look like this:

```rust
pub fn get_notifier (notifier: &config::NotifierConfig) -> Result<Box<Notifier>, Error> {
    // ...
        "multicast"     => notifier_from_config!(MulticastNotifier),
        "none" | "noop" => notifier_from_config!(NoopNotifier),
        "imaginary"     => notifier_from_config!(ImaginaryNotifier)
    // ...
}
```
