# Session memory and shareable results

The CLI can search completed native evaluations across sessions in one pilot and
export selected evidence. These operations never launch a project command or
change an acceptance decision. They may use isolated Git plumbing to construct
and verify patches. Use `doctor` capabilities `session_memory` and `result_bundles`
to detect support.

## Search and rebuild

```sh
autoresearch memory --root /path/to/pilot --query 'src/parser.rs' --json
autoresearch memory --root /path/to/pilot --query 'constraint_failed' --json
autoresearch index --root /path/to/pilot --operation-id index-001 --json
```

`memory` performs a case-insensitive substring search over session/candidate IDs,
hypotheses, changed paths and failure reasons. Omit `--query` to list all native
evaluations. Historical imports remain unverified and are inspected separately
through `report --legacy`.

Each query rebuilds its view from journal-bound reports and verified snapshots.
All selected sessions are locked in sorted order during the scan. A busy session,
corrupt report or invalid snapshot fails the complete query, instead of silently
omitting an evaluation. A session created after directory enumeration belongs to
the next scan. Session sequences identify the observed state.

`index` writes the same derived view atomically to `.auto/engine/memory.json`, with
an exclusive index-writer lock. This private file includes hypotheses and relative
source paths. It can be deleted and rebuilt; neither execution nor search trusts
its contents. The operation ID identifies an explicit cache refresh and does not
reserve a session operation: a later refresh may reflect new evidence. Rebuilding
does not modify session journals, budgets or verdicts. Limits are 256 sessions,
4,096 evaluations and 16 MiB of serialized rows per pilot; queries are intended
for bounded local history, not a remote search service.

## Exact duplicate information

A candidate row receives a `duplicate_key` from these three fingerprints:

1. The compared reference's complete file contents and executable modes.
2. The verified binary patch from that reference to the candidate.
3. The execution-time measurement protocol.

`duplicate_of` names matching preceding rows in deterministic session/key order.
A changed reference or protocol creates a different key. Text matches never
produce a duplicate key and never reject or schedule work automatically.

New evaluation reports record an optional `protocol_sha256`. It binds scope,
checks, benchmark, sampling, constraints, execution policy, resolved environment,
direct external executable hashes, OS/architecture, hook protocol and command/
output/artifact limits. It excludes session identity, goal, source location and
the overall attempt/time/deadline allowance. Those administrative budgets still
apply normally; they are not reset or combined by memory. Reports predating this
field remain readable and searchable but receive no exact duplicate key.

This is declared-input identity, not a claim of identical future measurements.
Hooks and commands can observe runtime locations and context; external/transitive
dependencies remain subject to the engine's documented trust limits. Even an
exact duplicate may be worth measuring again for a user-authorized reason.

## Preview and export

Select an explicit native evaluation key from `history`:

```sh
autoresearch result-preview --root /path/to/pilot --session my-session \
  --evaluation run-REPLACE_WITH_KEY --json
autoresearch export-result --root /path/to/pilot --session my-session \
  --evaluation run-REPLACE_WITH_KEY --output /path/to/new-result \
  --operation-id result-001 --json
```

Preview returns the exact selected inventory, file hashes, sizes, report and
manifest hash. It publishes no files and changes no session state. Export uses
the same preparation path, so an unchanged selection produces the same hash.

Default bundles contain `report.json`, `report.md`, `REPRODUCE.md` and
`manifest.json`. They include the recorded verdict, reason, metric definitions,
numeric observations grouped by measurement phase, acceptance summary, noise,
secondary constraints, selected limits and evidence/code fingerprints. They omit
source locations, session/operation/candidate names, hypothesis text, environment,
commands, input seeds and raw logs. Metric labels and measured values are intended
report content; no automatic secret detector is promised.

Additional content requires explicit selection on both preview and export:

| Option | Selected content |
| --- | --- |
| `--include-code` | `base/`, the actual compared `reference/`, cumulative `candidate.patch`, and `candidate-files.json` |
| `--include-protocol` | `protocol.template.json`, including command arguments, environment declarations/values, seeds and input fingerprints |
| `--log stages/0000.stdout` | That exact stage log, after checking its journal-bound SHA-256; repeat for another log |

For example:

```sh
autoresearch result-preview --root /path/to/pilot --session my-session \
  --evaluation run-REPLACE_WITH_KEY --include-code --include-protocol --json
autoresearch export-result --root /path/to/pilot --session my-session \
  --evaluation run-REPLACE_WITH_KEY --include-code --include-protocol \
  --output /path/to/new-reproducible-result --operation-id result-002 --json
```

Code selection includes every file in the captured source snapshots; snapshots
may themselves contain private files or embedded data. Protocol selection can
expose secrets in arguments or environment values. Its source location, commit,
session ID and goal are replaced with reproduction placeholders, but arbitrary
command strings are preserved faithfully. Selected logs may contain sensitive
output. Inspect the preview and selected material before sharing; no upload or
publication occurs as part of export.

The bundle keeps the original recorded decision, including rejection or failure.
An exported rejected trial never becomes an accepted optimization. The original
report's SHA-256 binds the local source evidence; the derived report is not a
copy of that private native report. Hashes provide integrity, not an independent
attestation or remote signature.

## Reproduction and persistence

The exporter applies the cumulative patch to the initial base in an independent
Git index and verifies the exact sealed tree, including binary content and modes.
`reference/` preserves the actual comparison reference, which may already include
earlier accepted improvements. The bundled guide explains reconstruction in a new
directory, then fresh qualification and evaluation using reviewed commands and an
explicit new budget. Toolchains and external inputs must be supplied separately.
Repeating measurements never inherits an old verdict.

Use a new output directory outside source, Git and engine metadata. Bundles are
limited to 128 MiB, 16,384 files and the session's artifact-byte limit. Publication
reserves the destination exclusively and synchronizes the prepared files. A local
receipt below `results/` binds the destination and selection to the session
journal. Retry the same operation ID and options after an interruption: a complete
bundle can be verified and its receipt reconciled without rewriting its content.
Different selections/destinations conflict. Missing or modified recorded bundles
are not regenerated automatically. Symlinks, hardlinks and nonregular result files
are rejected; an empty pre-existing destination is never overwritten.

A crash during directory reservation can leave an empty destination or `.result-*`
staging material for inspection. Exports remain allowed after a session stops or
expires and consume no experiment or active-process budget. They do not reset
budgets or edit source. The older `export-candidate` remains a code-only export
without an evaluation assertion.

These commands are available through the CLI and engine-mode skill guidance.
Pi's dedicated adapter keeps its existing action set; use Pi's ordinary CLI
access for memory and selective result exports.
