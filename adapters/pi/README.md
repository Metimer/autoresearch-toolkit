# Autoresearch Toolkit for Pi

An optional TypeScript adapter for the Rust engine. It attaches Pi to one explicit
engine session; Rust owns execution, budgets, cancellation, evidence and verdicts.
The adapter does not start an autonomous loop or resume work on session events.

Compatibility is pinned to **Pi `0.85.1`**, **Node.js 24**, and the Toolkit
**`1.0.0-rc.1`** CLI with JSON envelope version 1, Pi support and hook protocol 1.
The lockfile records the tested dependency graph. Local acceptance used macOS,
Node.js 24.20.0 and Rust 1.81; Linux/macOS CI is configured separately.

## Load from a source checkout

Build the engine and install the adapter dependencies explicitly:

```sh
cargo build --workspace --locked
npm --prefix adapters/pi ci --ignore-scripts --no-audit --no-fund
```

Prepare and authorize a real session contract using the
[engine guide](../../docs/RUST_ENGINE.md). Initialize it with the CLI; the adapter
attaches to an existing session and does not infer a configuration or budget:

```sh
/absolute/path/to/autoresearch init --config /absolute/path/to/session.json \
  --root /absolute/path/to/pilot --operation-id init-001 --json
```

From the checkout, load the adapter with the locally installed, pinned Pi:

```sh
./adapters/pi/node_modules/.bin/pi -e ./adapters/pi/index.ts \
  --autoresearch-engine /absolute/path/to/autoresearch \
  --autoresearch-root /absolute/path/to/pilot \
  --autoresearch-session my-session
```

Replace `my-session` with the initialized contract's ID. Paths with spaces are
supported when quoted. The extension checks engine compatibility before its first
operation. It never downloads a binary, modifies Pi configuration, or selects a
different session automatically. See the upstream
[extension loading documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md).

The root package and exported portable bundles continue to load skills only.
An engine archive built with `--agent pi --pi-adapter` includes this adapter and
registers it in the packaged manifest. See the
[candidate installation guide](../../docs/INSTALL.md). npm dependencies remain
separately installed; registry publication is not enabled.

## Tools and stopping

`autoresearch_engine` accepts these actions. Each mutation requires an explicit
`operation_id`; reuse it only to retry the same request.

| Action | Additional arguments |
| --- | --- |
| `status`, `history` | None |
| `report` | Optional `evaluation` key; defaults to the qualified reference |
| `workspace` | `local_changes`: `exclude` or `include` |
| `prepare-candidate` | `candidate`, `hypothesis` |
| `seal`, `evaluate` | `candidate` |
| `baseline`, `resume`, `stop` | None |
| `export-candidate` | `candidate`, `output` (a new directory) |

For example, after explicit authorization to measure the captured reference:

```json
{"action":"baseline","operation_id":"baseline-001"}
```

Only edit the directory returned by `prepare-candidate`. Seal it before
`evaluate`. The tool shows the recorded decision, reason and evidence hash;
its details retain the complete CLI envelope. A `discarded`, `failed` or
`cancelled` verdict is not a successful optimization, even when the CLI exits zero.
Read-only output is capped for display; the complete details remain available.

Pi's tool cancellation signal and `/autoresearch-stop` send SIGTERM only to the
currently owned Rust child and await its exit. Rust stops its ordinary process
group and records the charged budget. The slash command cancels the current call;
use the `stop` action to mark an idle session stopped. To stop an execution owned
by another process, use the engine CLI's `stop` command.

Session switch, fork, tree navigation and shutdown also cancel pending work.
Startup and reload do not launch an experiment or resume the engine session.
The adapter allows one engine operation at a time; Rust's locks also protect
against separate adapter instances. `resume` is explicit and retains the original
budget and deadline. Journal-tail repair and historical import remain CLI actions.

If cleanup has not finished within five seconds, the adapter reports the pending
cleanup and blocks new work while its child remains owned. It cancels a session
transition where Pi permits it. It does not kill a supervisor forcefully or signal
a PID read from disk. Forced host termination, escaped process groups and a
shutdown handler that the host cannot await require the engine's documented
inspection/recovery procedure; this adapter is not a process isolation sandbox.

## Checks

From a full source checkout:

```sh
cargo build --locked
npm --prefix adapters/pi run check
npm --prefix adapters/pi test
```

The integration tests use Pi's real extension loader and event runner with the
real Rust CLI, temporary Git repositories and an in-memory Pi session. They cover
qualification, promotion, idempotent retry, history, reports, export, preserved
source files, cancellation and process cleanup. No provider request or API key is
needed. Transport tests reject incompatible, oversized and inconsistent responses.

From an extracted engine/Pi archive, after explicitly installing its dependencies:

```sh
node /absolute/path/to/autoresearch-toolkit/scripts/qualify_pi.mjs \
  /absolute/path/to/autoresearch-toolkit
```

This package check loads the real Pi extension and negotiates with the bundled
engine without a provider request. The full integration suite above requires the
source checkout.

Hooks are configured in the engine contract, not in Pi. See the
[versioned hook contract](../../docs/RUST_ENGINE.md#hook-contract-version-1).
