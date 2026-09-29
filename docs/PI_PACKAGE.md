# Ultramarine for Pi

Ultramarine packages the Autoresearch Toolkit's Pi extension and two skills.
The executable, tools and skills retain their `autoresearch` names.

## Requirements

- Node.js 24 and Pi 0.85.1, the tested host version.
- Git and Linux or macOS. Native Windows is not qualified.
- Autoresearch engine **1.0.0**, installed separately, for the native tool.
- Python 3.10+ for portable skill mode and archive qualification.
- Your project's own check and benchmark dependencies.

Pi supplies the modules declared as `*` peers. This does not establish support
for every Pi version; the release is tested with 0.85.1. There is no automatic
binary download or install hook.

## 1. Install the Pi package

Run:

```sh
pi install npm:@metimer/ultramarine@1.0.0
```

Restart Pi. Both skills and `autoresearch_engine` are registered. Loading without
engine flags is supported; using the native tool requires step 3.

## 2. Install the engine

Open the [v1.0.0 release](https://github.com/Metimer/autoresearch-toolkit/releases/tag/v1.0.0)
and download `SHA256SUMS` and the engine/Pi archive for your machine:

| Machine | Target in archive filename |
| --- | --- |
| Apple Silicon Mac | `aarch64-apple-darwin` |
| Intel Mac | `x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` |

The filename begins with `autoresearch-toolkit-1.0.0-engine-pi-` and ends with
`-with-pi.tar.gz`. Follow [archive installation](INSTALL.md#install-an-engine-archive)
to compare its checksum, extract it into a new directory and run:

```sh
/absolute/path/to/autoresearch-toolkit/bin/autoresearch doctor --json
```

Use only the binary from this archive with the npm extension; registering its
extension or skills too would create duplicates. Linux platform requirements are
in the installation guide. Alternatively, build 1.0.0 from source using the
[engine guide](RUST_ENGINE.md). The npm package contains no Rust sources.

## 3. Prepare a session and connect Pi

Use `/skill:autoresearch-scout` to prepare a contract for your repository,
check/benchmark commands, scope, attempt ceiling and deadline. Supply the absolute
engine path and explicitly request Rust mode. The [shared workflow](HARNESS_WORKFLOW.md)
provides complete prompts. Initialize the reviewed contract if the scout has not
already done so:

```sh
/absolute/path/to/autoresearch init \
  --config /absolute/path/to/session.json \
  --root /absolute/path/to/pilot --operation-id init-001 --json
```

From your target project, start Pi with the contract's actual session ID:

```sh
pi \
  --autoresearch-engine /absolute/path/to/autoresearch \
  --autoresearch-root /absolute/path/to/pilot \
  --autoresearch-session SESSION_ID
```

Use the engine location from step 2 in both commands; quote paths with spaces.
An initial `status` call checks connectivity without running a benchmark. Invoke
`/skill:autoresearch-run` within your authorized scope and budget. Rust owns the
execution, budgets, cancellation and acceptance decisions.

## Stop, update and remove

`/autoresearch-stop` cancels the active engine call. Use the tool's `stop` action
to mark an idle session stopped. Restarting Pi never resumes experiments
automatically. See the [adapter reference](../adapters/pi/README.md) for recovery.

For an upgrade, install the explicitly chosen npm version and matching engine,
preserving a backup of session data. The install command pins version 1.0.0.

```sh
pi remove npm:@metimer/ultramarine
```

Remove engine flags from future launches. Session data and separately downloaded
engine files remain yours to retain or remove.

## Portable mode

Explicitly request portable mode to use the skills with Git and Python 3.10+
without the engine. The agent then enforces session limits. The native
`autoresearch_engine` tool still needs a configured Rust session.
