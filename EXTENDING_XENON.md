# Extending oxixenon

## Table of contents

1. [Introduction](#introduction)
2. [Creating a new renewer](#creating-a-new-renewer)
3. [Creating a new notifier](#creating-a-new-notifier)

## Introduction

The core components of Xenon (the `renewers` and the `notifiers`) are designed to be generic and
expansible. This document details how to create a new renewer and a new notifier.

## Creating a new renewer

Renewers are defined in individual files inside the folder `src/renewer`.
The trait which defines a renewer is the the following:

```rust
trait Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self>
        where Self: Sized;
    fn init(&mut self) -> Result<()> { Ok(()) }
    fn renew_ip(&mut self) -> Result<()>;
}
```

Let's cover all the functions one by one.

### `from_config(renewer: &config::RenewerConfig) -> Result<Self>`

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
fn get_as_str_or_invalid_key (&self, key: &'static str) -> config::Result<&str>;
fn get_as_table_or_invalid_key (&self, key: &'static str) -> config::Result<&toml::Value>;
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

First, create a new file named `renewer/acme.rs`.
Here's how the definition and implementation of `from_config` may look:

```rust
use super::{Renewer as RenewerTrait, Result, ResultExt};
use config;
use config::ValueExt;

pub struct Renewer {
    url: String,
    username: String,
    password: String
}

impl RenewerTrait for Renewer {
    fn from_config(renewer: &config::RenewerConfig) -> Result<Self>
        where Self: Sized
    {
        let config = renewer.config.as_ref()
            .chain_err (|| config::ErrorKind::MissingOption ("server.renewer.acme"))
            .chain_err (|| "the renewer 'acme' requires to be configured")?;        
        Ok(Self {
            url:
                config.get_as_str_or_invalid_key ("server.renewer.acme.url")
                    .chain_err (|| "failed to find URL in renewer 'acme'")?
                    .into(),
            username:
                config.get_as_str_or_invalid_key ("server.renewer.acme.username")
                    .chain_err (|| "failed to find username in renewer 'acme'")?
                    .into(),
            password:
                config.get_as_str_or_invalid_key ("server.renewer.acme.password")
                    .chain_err (|| "failed to find password in renewer 'acme'")?
                    .into()
        })
    }
    // ...
}
```

### `init(&mut self) -> Result<()>`

This method is **optional** and is used if you want to perform any kind of initialization after
the configuration has been successfully loaded. For example, you might want to check if the
credentials supplied by the user are correct.

### `renew_ip(&mut self) -> Result<(), Error>`

This is the core function of the renewer which, as the name implies, performs an IP renewal.
In our imaginary Acme renewer, we would make an HTTP request (possibly using the built-in
`http_client` HTTP client) to the endpoint specified in the configuration.

### Wrapping up the renewer

Once all the required methods have been implemented, the last steps to perform are as follows:

1. **Include the renewer**  
   If your renewer needs additional dependencies, it needs to be an optional feature specified
   in `Cargo.toml`. [Check it out](Cargo.toml) to see how it's done.
   Then, to actually include the renewer, open `renewer/mod.rs`, and after the last `mod` line
   add something like the following:

   ```rust
   #[cfg(feature = "renewer-acme")] mod acme;
   ```

2. **Make it available inside the app**  
   To make it available, add the renewer to the `get_renewer` function in the same file as follows:

   ```rust
   pub fn get_renewer (renewer: &config::RenewerConfig) -> Result<Box<Renewer>> {
       ...
       match renewer.name.as_str() {
           #[cfg(feature = "renewer-dlink")] "dlink" => renewer_from_config!(dlink::Renewer),
           "dummy" => renewer_from_config!(dummy::Renewer),
           #[cfg(feature = "renewer-acme")] "acme" => renewer_from_config!(acme::Renewer)
           ...
       }
   }
   ```

3. **Test it**  
   You're done! Test your renewer as follows:

   ```
   cargo run --features "renewer-acme" -- server -r acme
   ```

## Creating a new notifier

The basic structure of a notifier is very similar to the one of a renewer. Notifiers are defined in
individual files inside the folder `src/notifiers`. Here's the `Notifier` trait:

```rust
trait Notifier {
    fn from_config (notifier: &config::NotifierConfig) -> Result<Self>
        where Self: Sized;
    fn notify (&mut self, event: Event) -> Result<()>;
    fn listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<()>;
}
```

### `from_config (notifier: &config::NotifierConfig) -> Result<Self>`

This function works and is implemented the same way as `Renewer::from_config` is implemented - the
objects even have the same fields.
Check out the docs above to see how it's done.

### `notify (&mut self, event: Event) -> Result<()>`

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

Here's an example of an `ImaginaryNotifier` which does as specified:

```rust
// src/notifier/imaginary.rs
use super::{Notifier as NotifierTrait, Result};
use config;
use protocol::Event;
use std::net::SocketAddr;

struct Notifier;

impl NotifierTrait for Notifier {
    // ...
    fn notify (&mut self, event: Event) -> Result<()> {
        // holds the raw bytes of the packet we're going to pack
        let mut vec: Vec<u8> = Vec::new();
        Packet::Event (event).write (&mut vec)
            .chain_err (|| "can't write specified event to a local buffer")?;
        // do anything with `vec`...
        Ok(())
    }
    // ...
}
```

### `listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<(), Error>`

This method is called by the client when it is told to listen to notifications. You can use
the macro `bail!("error message")` if your notifier doesn't support listening for notifications.

Example implementation:

```rust
// assuming the previous definition and partial implementation of ImaginaryNotifier

impl NotifierTrait for Notifier {
    // ...
    fn listen(&mut self, on_event: &Fn(Event, Option<SocketAddr>) -> ()) -> Result<()> {
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

Follow the instructions on how to wrap up a renewer above. In the end, `notifier/mod.rs` should
look like this:

```rust
...

mod multicast;
mod noop;
mod imaginary;

...

pub fn get_notifier (notifier: &config::NotifierConfig) -> Result<Box<Notifier>> {
    ...
    match notifier.name.as_str() {
        "multicast"     => notifier_from_config!(multicast::Notifier),
        "none" | "noop" => notifier_from_config!(noop::Notifier),
        "imaginary"     => notifier_from_config!(imaginary::Notifier)
        _ => bail!("invalid notifier name '{}', must be one of 'multicast', 'none'", notifier.name)
    }
}

```
