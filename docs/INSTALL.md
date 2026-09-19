# Candidate installation and compatibility

This is **1.0.0-rc.1**, an unpublished candidate. Stable v1 qualification requires
the full target matrix to pass on the final revision. Downloading, uploading,
tagging and publishing releases are separate actions; none runs at import time.

## Select a profile

| Profile | Contents | Requirements |
| --- | --- | --- |
| `skills` | Skills and one chosen agent manifest; no binary or engine extension | Git, Python 3.10+ for portable measurements; an independent engine for Rust mode |
| `engine` | Skills, native CLI, license inventory, engine guides and demonstrations | Matching OS/CPU and Git; project commands supply their own dependencies |
| `engine` with Pi selected | Engine profile plus the TypeScript Pi adapter and lockfile | Node.js 24 and Pi 0.85.1, installed explicitly |

Archives are named with the version, profile, agent and native target. Each
contains one `autoresearch-toolkit/` directory, a `BUNDLE.json` file inventory,
and the original MIT license. Engine profiles add `DEPENDENCIES.json`, original
dependency license/notice files and Rust distribution notices. Pi dependencies
are installed separately and are not vendored in these archives.

## Install an engine archive

Check the archive against its accompanying `SHA256SUMS` before extracting. This
checks integrity, not publisher authenticity: obtain both from a trusted source.
Extract into a new directory. Keep the package intact, including its notices.
Invoke the binary directly or add its `bin/` directory to your shell PATH:

```sh
/absolute/path/to/autoresearch-toolkit/bin/autoresearch doctor --json
```

No global installation or shell configuration is changed automatically. macOS
candidates are not Developer ID signed or notarized; these locally built archives
are intended for testing. Project execution remains trusted local execution, not
a sandbox. The engine rejects `network=disabled` until an isolation backend exists.

To verify the extracted engine inventory and explicitly run its two demonstrations:

```sh
python3 /absolute/path/to/autoresearch-toolkit/scripts/qualify.py \
  --bundle /absolute/path/to/autoresearch-toolkit
```

This needs Python 3.10+ and Git. It uses temporary repositories outside the package
and performs no network request. Qualify before adding optional dependencies to
the package: additional files intentionally fail the strict package inventory.
Real experiments require a reviewed version 2 session contract and an explicit
scope, budget and deadline; the example contract contains placeholders.

## Load skills or the optional Pi adapter

Place the extracted `autoresearch-toolkit` folder in the location supported by
the chosen agent. Its selected manifest advertises the packaged skills. The
root package remains `private: true`. Existing agent configuration is not edited.

For an engine archive explicitly built with Pi, first install its pinned
dependencies from the extracted package, after inventory qualification:

```sh
npm --prefix /absolute/path/to/autoresearch-toolkit/adapters/pi ci \
  --ignore-scripts --no-audit --no-fund
```

This explicit installation may access the npm registry. Engine execution and
extension import never install dependencies. Launch the pinned Pi with the
extension path and an already initialized engine session:

```sh
/absolute/path/to/autoresearch-toolkit/adapters/pi/node_modules/.bin/pi \
  -e /absolute/path/to/autoresearch-toolkit/adapters/pi/index.ts \
  --autoresearch-engine /absolute/path/to/autoresearch-toolkit/bin/autoresearch \
  --autoresearch-root /absolute/path/to/pilot --autoresearch-session my-session
```

An engine archive for another agent has no Pi extension unless selected in the
Pi profile. Skills-only Pi archives contain skills only. Cancellation and recovery
follow the Rust session contract; there is no automatic experiment loop.

## Native target matrix

| Target | Native CI environment | Artifact name |
| --- | --- | --- |
| `aarch64-apple-darwin` | macOS 15 ARM64 | `candidate-aarch64-apple-darwin` |
| `aarch64-unknown-linux-gnu` | Ubuntu 24.04 ARM64 | `candidate-aarch64-unknown-linux-gnu` |
| `x86_64-unknown-linux-gnu` | Ubuntu 24.04 x64 | `candidate-x86_64-unknown-linux-gnu` |
| `x86_64-apple-darwin` | macOS 15 Intel | `candidate-x86_64-apple-darwin` |

Use the [candidate workflow runs](https://github.com/Metimer/autoresearch-toolkit/actions/workflows/candidate.yml?query=branch%3Aqualification%2F1.0.0-rc.1)
to inspect results and download the nine artifacts: four native engine/Pi packages
and five skills packages. Each contains an archive, checksums and qualification
reports; native packages additionally include the Pi loader report. Match the
run's commit to `BUNDLE.json.source_commit`, require `source_dirty: false`, and
check the archive and manifest hashes against the reports. The engine and Pi
reports must refer to the same manifest.

Qualification requires both that candidate run and the
[Checks workflow](https://github.com/Metimer/autoresearch-toolkit/actions/workflows/ci.yml?query=branch%3Aqualification%2F1.0.0-rc.1)
to succeed for the same source commit. A successful job on an older commit does
not qualify a newer artifact. CI artifacts are reviewable candidates, not a
tagged stable release; the workflow does not merge the qualification branch.

Builds run natively for each target, with Rust 1.81 and locked dependencies. Linux
packages depend on the builder's glibc; they are not advertised as universal
Linux binaries. CI uses Ubuntu 24.04, while local Linux candidate tests use Debian
12. Validate deployment against the exact artifact and OS in its qualification
report. Windows native, WSL and other CPU/OS combinations are not qualified by
this candidate. See the GitHub
[runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
for the native CI runner architectures.

## Build from a source checkout

```sh
python3 scripts/release.py --profile skills --agent codex --output dist/skills-codex
python3 scripts/release.py --profile engine --agent pi --pi-adapter --output dist/engine-pi
python3 scripts/qualify.py --archive dist/engine-pi/ARCHIVE_NAME.tar.gz \
  --output dist/engine-pi/qualification.json
```

`--target` may select the current native target; cross-built binaries are refused.
`--offline` uses only previously cached Cargo dependencies. Without it, an explicit
engine build may fetch locked Cargo dependencies. Every output directory must be
new, and all resources are copied from fixed inventories. Unlisted reports,
sessions, credentials, archives and local dependencies are excluded.

`scripts/export.py --profile ...` provides the same release builder. Omitting
`--profile` retains the existing skills-folder exporter. `BUNDLE.json` records
the source revision and whether it was dirty, plus hashes, sizes and executable
bits. Archive timestamps and owner fields are normalized; identical binary builds
across different toolchains/SDKs are not promised. No release is published by these
commands. A stable release requires clean-source artifacts and successful CI.

## Upgrade and rollback

Install into a separate versioned parent directory; select the new binary path
explicitly and retain the previous package. Back up engine sessions before trying
a candidate. The new engine reads existing version 2 configurations and earlier
native reports, including reports without `protocol_sha256`. Older binaries may
reject new report fields or event kinds; do not assume that switching the binary
back can read sessions advanced by this version. Keep the corresponding backup
for rollback. Historical Pi/portable import remains a separate, explicit action.
