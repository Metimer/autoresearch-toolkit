# Ultramarine 1.0.0

Ultramarine brings bounded, measured optimization to Pi. Prepare a baseline,
test one hypothesis at a time, and retain only improvements confirmed by checks
and independent measurements.

The Pi package includes the native extension and two complete skills:

- `autoresearch-scout` discovers workloads and prepares a session.
- `autoresearch-run` experiments within the recorded scope and budget.

The Rust engine owns execution, cancellation, accounting and acceptance.
Loading the extension never starts an experiment or resumes a session.
Existing `autoresearch` executable, tool and skill names are preserved.

## Installation

Download the engine archive for your OS/CPU and compare it with `SHA256SUMS`.
Four native targets are supplied: Apple Silicon and Intel macOS, Linux ARM64
and Linux x64. See the
[installation guide](https://github.com/Metimer/autoresearch-toolkit/blob/v1.0.0/docs/PI_PACKAGE.md).

The npm package is published separately. Once available:

```sh
pi install npm:@metimer/ultramarine@1.0.0
```

Tested with Pi 0.85.1, Node.js 24 and engine 1.0.0. The npm package requires a
separate engine download; it has no binary download or lifecycle install script.

## Validation and limits

Attached qualification reports identify their source revision and artifact hashes.
Native archives exercise baseline qualification, accepted improvements, rejected
regressions and source preservation. The npm tarball is tested outside the source
checkout on Linux and macOS, including an offline npm install, skill discovery, real engine operations,
cancellation and process cleanup. These checks make no model/provider request.

The `metimer-ultramarine-1.0.0.tgz` file is the npm publication candidate. The
`macos-verified-` archive records the independently tested macOS build.
Skills-only archives are also supplied for Codex, Claude Code, Cursor, Pi and
generic hosts.

Commands run as trusted local code; no operating-system sandbox is provided.
Windows is not qualified. Linux binaries require a compatible glibc (native CI
uses Ubuntu 24.04); macOS binaries are not Developer ID signed or notarized.

Created and maintained by Metimer. MIT licensed; upstream attribution and
dependency notices are included in the corresponding archives.
