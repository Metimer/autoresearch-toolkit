# Ultramarine

<p align="center">
  <img src="https://raw.githubusercontent.com/Metimer/autoresearch-toolkit/v1.0.0/ultramarine_wback.png" alt="Ultramarine — a blue snake forming an open loop" width="200" height="200">
</p>

Measured optimization for Pi: establish a baseline, test one hypothesis at a
time, and keep only confirmed improvements within an explicit budget.

Created and maintained by **Metimer**, built on the Autoresearch Toolkit engine.

## Install

With Pi installed, run:

```sh
pi install npm:@metimer/ultramarine@1.0.0
```

Requires **Node.js 24**. The tested host is **Pi 0.85.1**. The native extension
requires the separately installed **Autoresearch engine 1.0.0**, Git, and Linux
or macOS. Other Pi versions and Windows are not qualified by this release.

The package contains the Pi extension, two complete skills, documentation and
licenses. Download the engine for your OS/CPU from the
[GitHub release](https://github.com/Metimer/autoresearch-toolkit/releases/tag/v1.0.0)
or build it from source. There is no automatic binary
download or install hook.

## Start

Follow the [Pi installation guide](docs/PI_PACKAGE.md) to install the engine,
prepare a session and connect Pi. The extension exposes `autoresearch_engine`
and `/autoresearch-stop`. These existing names are retained for compatibility.

- `/skill:autoresearch-scout` discovers checks and prepares a measured baseline.
- `/skill:autoresearch-run` experiments within the session's scope and limits.

The Rust engine owns command execution, cancellation, measurements, budgets and
acceptance decisions. Loading the extension does not start an experiment or
resume a session. Portable skill mode is also available with Python 3.10+;
it uses agent-enforced session limits instead of Rust-owned execution.

Read the [workflow guide](docs/HARNESS_WORKFLOW.md),
[engine reference](docs/RUST_ENGINE.md), and [adapter reference](adapters/pi/README.md)
for setup, verdicts, cancellation and recovery.

## License

[MIT](LICENSE). See [third-party notices](THIRD_PARTY_NOTICES.md).
