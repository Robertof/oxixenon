# v1.2.2

- FIXED: new FritzOS versions do not return _403_ anymore when the SID expires, but redirect.

# v1.2.1

- CHANGED: the `fritzbox` renewer no longer prints out the SID when logging in.
- CHANGED: the `syslog-backend` feature now only works on non-Windows platforms.

# v1.2

- NEW: added support for any FRITZ!Box routers with the new "fritzbox" renewer.
       This renewer works everywhere as it connects to a FRITZ!Box using its web interface.

# v1.1

- NEW: added support for FRITZ!Box routers with the new "fritzbox-local" renewer.
       NOTE: this renewer only works when Xenon is directly running on the FRITZ!Box router, as it
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
