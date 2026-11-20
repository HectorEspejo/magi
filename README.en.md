# MAGI

**English** · [Castellano](README.md)

A keyboard-driven terminal SSH manager with a technical-cockpit look: host
inventory, tabs that outlive the window, SFTP, tunnels, snippets with a
deliberation step, encrypted sync between machines, remote desktops (RDP, VNC),
services and containers, and persistent tmux sessions. Free software under
MIT OR Apache-2.0, with no account, no subscription and no telemetry.

![MAGI in a terminal: the fleet with three sample hosts, Files, Services and the command palette.](assets/demo/magi.gif)

*Animation recorded against the repository's test server, with made-up hosts.*

## Status

| Platform | Status |
|---|---|
| Linux (Omarchy, Arch and others) | Daily use |
| macOS | Under testing |
| Android app | Beta |
| Termux (MAGI inside Android) | Under testing |

"Under testing" and "beta" mean that automated tests and CI cover it, but it has
not been tried by hand in as much detail as Linux (on macOS and Termux, not yet). What has and has not been tested by hand, per platform
and per phase, is in [`docs/manual/estado.md`](docs/manual/estado.md) (in
Spanish).

## Installation

| Platform | How |
|---|---|
| Arch and Omarchy | `yay -S magi-ssh-bin` |
| macOS | `brew install hectorespejo/tap/magi-ssh` |
| Other Linux | tarball from [the latest release](https://github.com/HectorEspejo/magi/releases/latest) |
| Android | [Obtainium](https://github.com/ImranR98/Obtainium) with this repository's URL and the `android-v` tags |
| Termux | the release's `.deb` (see [Termux](docs/manual/termux.md)) |

Every release publishes a `.sha256` per file and GitHub provenance
attestations. Check both before installing a tarball:

```sh
sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz --repo HectorEspejo/magi
```

The app's APK is also checked with `apksigner verify --print-certs`. The
details, the requirements, building from source, updating and uninstalling are
in [`docs/manual/instalacion.md`](docs/manual/instalacion.md) (in Spanish).

## Getting started

1. Run `magi diagnostico`: it checks paths, socket, keyring and `~/.ssh/config`,
   and every warning comes with its fix.
2. Start `magi` and, in the Hosts view (`F2`), press `I` to import
   `~/.ssh/config`.
3. Connect with `↵`. Closing the window does not close the sessions.
4. Explore with `Ctrl+P` (palette) and `?` (help).

The full guide is in [`docs/manual/primeros-pasos.md`](docs/manual/primeros-pasos.md).

## Manual

The [manual](docs/manual/README.md) covers each part in detail. It is written in
Spanish, like the code, the comments and the interface:

- [Getting started and diagnostics](docs/manual/primeros-pasos.md) and the
  [session server](docs/manual/servidor-de-sesiones.md)
- [Tabs and terminal](docs/manual/pestanas-y-terminal.md) and
  [persistent sessions](docs/manual/sesiones-persistentes.md)
- [Files](docs/manual/archivos.md) and [tunnels](docs/manual/tuneles.md)
- [Snippets and deliberation](docs/manual/snippets-y-deliberacion.md) and
  [fleet, identities and log](docs/manual/flota-identidades-y-registro.md)
- [Inventory sync](docs/manual/sincronizacion.md)
- [Remote desktop](docs/manual/escritorio-remoto.md) and
  [services, containers and logs](docs/manual/servicios.md)
- [Android app](docs/manual/app-android.md) and [Termux](docs/manual/termux.md)
- [Shortcuts](docs/manual/atajos.md), [the `Include` in `~/.ssh/config`](docs/manual/ssh-config.md)
  and [security](docs/manual/seguridad.md)

## Contributing

Contributions are welcome: read [`CONTRIBUTING.en.md`](CONTRIBUTING.en.md)
(also [in Spanish](CONTRIBUTING.md)). Big changes start with a proposal (the
"Proposal" issue template). Code, comments and interface texts are in Spanish;
issues and pull requests in English are accepted. The design history of every
phase is in [`docs/`](docs/README.md).

## Security

To report a vulnerability, use GitHub's private advisory (Security → Report a
vulnerability) and not a public issue: [`SECURITY.md`](SECURITY.md).

## License

MAGI is free software under the dual license **MIT OR Apache-2.0**, at your
option: [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE). Unless
stated otherwise, any contribution submitted to the project is licensed the same
way, following the Rust convention.

Copyright (c) 2026 Cuatro de Tres S. Coop. And.
