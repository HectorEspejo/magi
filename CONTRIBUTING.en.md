# Contributing to MAGI

**English** · [Castellano](CONTRIBUTING.md)

Thanks for your interest in MAGI. This document explains how to build it, how
to test it and how to propose changes. If you only want to use it, start with
the [README](README.en.md) and the [manual](docs/manual/README.md) (in Spanish).

## Before you start

- **A bug:** open an issue with the "Bug report" template. It asks for the
  version (`magi --version`), your system and terminal, the steps and the output
  of `magi diagnostico`. Do not paste real IPs, hostnames, keys or passwords.
- **An idea or a behavior change:** open an issue with the "Proposal" template
  first, and wait for an answer before writing code. Big changes are specified
  as a project phase (the design archive is in [`docs/`](docs/README.md)) and do
  not come in as a single pull request.
- **A vulnerability:** do not open a public issue. Use GitHub's private
  advisory, as [`SECURITY.md`](SECURITY.md) explains.
- Small changes (an obvious bug, a typo, a missing test) can go straight into a
  pull request.

## Building and testing

You need stable Rust (MSRV 1.86). The package is `magi-ssh`, but the command and
the library are called `magi`. The repository is a Cargo workspace: `magi-ssh`
(the root: TUI, server and CLI), `crates/magi-nucleo` (what is shared with the
app) and `crates/magi-movil` (the gateway to Kotlin); the Android app lives in
`android/`.

Before opening a pull request, all of these must pass:

```sh
TZ=UTC cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo deny check
```

Rendering tests need `TZ=UTC`; SFTP tests need `sftp-server` (or the
`MAGI_SFTP_SERVER` variable); persistent-session tests need `tmux` and
util-linux's `script`. If you touch workflows, also run `actionlint` and
`dist plan --tag vX.Y.Z`; if you touch scripts, `shellcheck`. For isolated
tests, set `HOME` and `XDG_*`: MAGI must not read or write your real files. The
tests never use the external network or your keyring.

The list of every command-line subcommand to try while developing, and a
description of what each test suite covers, are in the "Desarrollo" and "Tests"
sections of the Spanish [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Conventions

- **Language.** Code, comments, commit messages, interface texts and
  documentation are in Spanish; only standard technical terms (API, CRUD,
  endpoint, commit…) stay in English. The repository's front door is also in
  English, and nothing else: `README.en.md`, `CONTRIBUTING.en.md`, the English
  summary of each CHANGELOG version, the English paragraph of `SECURITY.md` and
  the English part of the `.github/` templates. Issues and pull requests in
  English are accepted.
- **Names.** Rust identifiers in Spanish (`snake_case`; types in `CamelCase`);
  SQL tables in `MAYUSCULAS_SNAKE_CASE` and fields in `minusculas_snake_case`.
- **Documentation.** Every user-visible change adds its line to the
  "Sin publicar" (unreleased) section of `CHANGELOG.md` and updates the right
  chapter of [`docs/manual/`](docs/manual/README.md). `README.md` and
  `README.en.md` say the same thing, and so do this file and `CONTRIBUTING.md`.
- **Sample data.** Nothing real in the repository: documentation addresses
  (`192.0.2.x`), the `ejemplo.org` domain, the `usuario` user and
  `/home/usuario`. The `repositorio_publico` test checks this on every pull
  request.
- **Pinned dependencies.** `russh`, `ratatui`, `crossterm`, `tui-term`, `vt100`,
  `rusqlite`, IronRDP, `age` and UniFFI are pinned and compatible with each
  other: they are not bumped one by one, and changes to only one of them are not
  accepted.
- **No telemetry.** MAGI opens no outgoing connection other than to the user's
  hosts and does not check for updates; changes that do are not accepted.

## Commits and pull requests

- Commit messages in Spanish, in the imperative and concise.
- One pull request per change, with its test. Use the template: tests, a line
  under "Sin publicar", the manual up to date and no real data.
- CI must be green: the `ci.yml` jobs (`formato`, `clippy`, `tests-linux`,
  `macos`, `visor`, `deny` and `aur-plantilla`) are required to merge into
  `main`. The maintainer reviews.
- The maintainer prepares versions and releases; you do not need to touch
  `version` or tag anything.

## License of what you contribute

There is no contributor license agreement. What you contribute comes in under
the project's same dual license, **MIT OR Apache-2.0**, as the
[README](README.en.md#license) says.

## Coding agents

The project is developed with coding agents. [`CLAUDE.md`](CLAUDE.md) and
[`AGENTS.md`](AGENTS.md) (the same content) hold the working rules and the design
decisions that matter when you touch the code (in Spanish): read them if you are
going to change more than a typo. If you use an agent, those rules apply equally
to what it produces, and you are responsible for the pull request.
