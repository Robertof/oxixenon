# oxixenon (aka Xenon)

## Introduction

Xenon is a software that can be used to obtain fresh IP addresses on demand on ISPs that give out
dynamic IP addresses. It uses a client-server architecture, where the server is supposed to run
on an always-on machine (e.g. a router) and performs the actual operations, and the client can be
distributed to many PCs in a LAN. It is supposed to be available on trusted, LAN environments, thus
there's no kind of authentication for incoming clients (yet).

Both the client and the server are written in Rust (they actually share the same crate).

## Compatibility

At the moment, the only renewer supported by Xenon is `dlink`, which works only on YAPS ("yet 
another purposeful solution") based routers, e.g. the D-Link DVA-5592 (or some ADB ones).

Check out [extending Xenon](EXTENDING_XENON.md) if you're interested in extending Xenon and adding
other renewers.

## Notifications

Xenon supports sending (and receiving) notifications using `notifiers`. The currently supported
notifiers are:

- `multicast`, which sends and receives notifications using UDP multicast packets. It requires
  a bind address and port along with a multicast address and port. To test if notifications work,
  run `./oxixenon client notifications` and run another client to send a renew request.
- `none`, which disables the functionality.

Check out [extending Xenon](EXTENDING_XENON.md) if you're interested in extending Xenon and adding
other notifiers.

## Quick start

To build Xenon, just run `cargo build --release` in the crate root. By default both client and
server modes are enabled when building. To change this, run:

```sh
# To build just the client
cargo build --release --no-default-features --features client
# To build just the server
cargo build --release --no-default-features --features server
```

The executable will be placed in `target/[architecture]/oxixenon` or `target/release/oxixenon`.

Xenon needs a valid configuration to run, please copy `config.example.toml` to `config.toml`
and edit it to suit your needs.

## Notification toasts

![notification toasts](https://robertof.ovh/sc/oxixenon_toasts.png)

Since version v0.2.0, Xenon supports showing notification toasts when any notification is received
from the specified `notifier`. At the moment, this feature is only supported on Windows, and
needs to be configured to work.

### Configuring notification toasts on Windows

1. On modern Windows platforms, notification toasts are only allowed by apps which have a shortcut
   with a specific property (`AppUserModelId`) set on the shortcut itself. Creating this shortcut
   by hand is not an easy task, but I created
   [a tool to make it easier](https://github.com/Robertof/make-shortcut-with-appusermodelid). After
   having downloaded the tool, run the following where the binary is stored:

   ```sh
   makelnk.exe RobertoFrenna.Xenon "PATH_TO_XENON_BINARY" Xenon.lnk
   ```

   Change `PATH_TO_XENON` accordingly. If everything went okay, `makelnk` should produce a positive
   output and you should now have a shortcut to Xenon in
   `%APPDATA%\Microsoft\Windows\Start Menu\Programs`.

2. Optional: place `oxixenon.png` where Xenon's binary is to receive toasts with icons. Otherwise,
   Xenon will gracefully degrade to use text-based toasts.

   _Note_: this is not needed when running Xenon with `cargo`.

3. Compile Xenon with the feature flag `client-toasts`:

   ```sh
   cargo build --release --no-default-features --features "client client-toasts"
   ```

4. Run Xenon in `notifications` mode:

   ```sh
   oxixenon client notifications
   ```

5. You're done!

## Renew availability

Xenon allows to temporarily disable the renewing functionality, and to specify a reason for the
unavailability. This is useful if, for example, you're performing some critical activity on your
network and you want to ensure that no downtime occurs.

## Dependencies

As I expected to run Xenon on my router, I decided to include as little dependencies as possible,
and I even went as far as implementing [my own (basic) HTTP client](src/http_client.rs) using the
objects specified in the crate `http`. The final list of dependencies is the following:

| Name | Client? | Server? | Purpose |
| ---- | ------- | ------- | ------- |
| byteorder | yes | yes | To properly send network-endian integers and so on |
| toml | yes | yes | To parse the configuration |
| http | no | yes | Basic HTTP objects used by `http_client` |
| sha2, hmac | no | yes | Required by the `dlink` renewer to hash passwords |
| clap | yes | yes | Used to parse command line arguments |

## Protocol

The custom protocol used by Xenon is pretty simple. A packet is composed of the following:

- a single byte which represents the packet number.
- other packet-specific fields.

A string is represented by a two-byte big-endian (`u16`) length field followed by individual
characters.

Here's a detailed view of existing packets and their composition:

| Packet # | Sent by | Name        | Description      | Fields |
| -------- | ------- | ----------- | ---------------- | ------ |
| `0`      | client  | `FreshIPRequest` | Requests a fresh IP address from the server | None |
| `1`      | server  | `Ok` | Sent when the requested operation has been successful | None |
| `2`      | server  | `Error` | Sent when the requested operation failed | reason (string) |
| `3`      | server  | `Event` | Represents an event | event_no (byte) |
| `4`      | client  | `SetRenewingAvailable` | Sets whether the renew functionality is enabled or not | is_unavailable (byte), if 1, then also unavailability_reason (string)  |

Available events:

| Event # | Name        | Description |
| ------- | ----------- | ----------- |
| `0`     | `IPRenewed` | A new IP has been requested |

Example protocol message (hexadecimal):

```
 ↙ Packet number (→ SetRenewingAvailable)
|   ↙ is_unavailable (true)
|  |   ___ ↙ unavailability reason length (0xC bytes → 12 bytes)
|  |  |   |  _________________________________ ↙ "hello world!"
|  |  |   | |                                 |
04 01 00 0C 68 65 6C 6C 6F 20 77 6F 72 6C 64 21
```

See also [protocol.rs](src/protocol.rs).

## TODOs

- [ ] Improve documentation
- [ ] Move `renewers` and `notifiers` specific implementations to their own directories
- [ ] Improve errors - right now only plain `enum`s are used, which are pretty restrictive and
      sometimes redundant

## History

Originally, Xenon was part of the "Noble Family", a collection of scripts written in Perl. It used
a textual protocol, and it had "siblings" (such as Neon, which was a web interface written using
Mojolicious). However, after performing a major refactoring and when the codebase started to become
of a relevant size (> 3k LOC), I decided to drop it and to eventually code it from scratch in
another language.

And now, years after I last touched the original Xenon, `oxixenon` (short of oxidized xenon,
which I know, doesn't make much sense chemically) was born in Rust. This is my first project in
Rust.

## License

This software is released under the simplified BSD license. See [LICENSE](LICENSE).
