# v1.1

- NEW: added support for FritzBox! routers with the new "fritzbox-local" renewer.
       NOTE: this renewer only works when Xenon is directly running on the FritzBox! router, as it
       makes use of internal commands to restart the connection and obtain a new IP.

# v1.0

First stable release! Hooray!

- CHANGED: each renewer and notifier has its own file (7cc7461, b503aeb)
- CHANGED: error handling is now managed by crate `error-chain`. This is a substantial change,
  and improves errors A LOT.

# v0.3

- NEW: implemented proper logging support using `fern`. This requires a new mandatory configuration
  section - [the example configuration](config.example.toml) has been updated accordingly.

# v0.2

- NEW: added "notification toasts" on win32, which allow Xenon to display notification toasts
  when any event is received. See
  [the relevant section in the README](README.md#notification-toasts) for futher information.

# v0.1

- First release.
