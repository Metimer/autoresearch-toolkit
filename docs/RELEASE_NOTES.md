# Autoresearch Toolkit 1.0.0

Prepared for stable release qualification; no tagged release or package has been
published yet. Created and maintained by Metimer.

The preceding `1.0.0-rc.1` revision
[`be10550`](https://github.com/Metimer/autoresearch-toolkit/commit/be10550390417f42b77268b8324ca5adbe732872)
passed all 20 jobs across
[Checks](https://github.com/Metimer/autoresearch-toolkit/actions/runs/35829790153)
and [Candidate packages](https://github.com/Metimer/autoresearch-toolkit/actions/runs/35829790064).
All nine archives were independently checked for source revision, clean-source
status, inventories, checksums, qualification reports and project credit.
Version 1.0.0 aligns the engine, Pi adapter and all five host manifests; the Pi
adapter requires the matching 1.0.0 engine. Commit
`581f0d372c8a7c990f4276fec01705cf965654db` subsequently passed
[Checks](https://github.com/Metimer/autoresearch-toolkit/actions/runs/35830541737)
and [Candidate packages](https://github.com/Metimer/autoresearch-toolkit/actions/runs/35830541749).
The Ultramarine packaging changes require fresh qualification on their final
commit before publication.

The public Pi package is named **Ultramarine**, distributed as
`@metimer/ultramarine`. It contains the extension and both complete skills; the
Rust engine remains a separate native download. Source and archive manifests
remain private. `scripts/package_pi.py` creates the public npm manifest,
inventories its contents and tests the actual tarball outside the checkout.
Existing commands, engine contracts and session paths are retained.

The Rust engine owns immutable contracts, independent source snapshots, scoped
candidates, supervised commands, shared budgets, measurement qualification and
confirmed acceptance. It preserves source repositories and records evidence in a
durable journal. The optional Pi 0.85.1 adapter relays cancellation and lifecycle
events without duplicating decisions or resuming experiments automatically.

This candidate includes historical import as unverified data, session search and
exact duplicate information, selective Markdown/JSON result bundles, versioned
hooks and portable agent skills. CLI-only operation requires neither Pi nor an
LLM. Default result exports omit code, environment and raw logs.

Distribution now has explicit skills/engine profiles, dependency notices,
versioned archives, checksums and a package inventory. Two packaged demonstrations
exercise qualification, a confirmed improvement, a rejected regression, source
preservation and result export. Their deterministic operation metrics are not
wall-clock performance claims.

Validation is recorded against archive hashes. Native macOS and local Linux
container checks do not replace the remote compatibility matrix. Promotion to
stable v1 requires all four announced targets, full failure/recovery tests,
packaged Pi loading and clean-source artifact qualification to pass on the final
revision. Local build and qualification commands do not push, tag, upload or
publish releases.

The qualification branch runs both general checks and native package tests.
See the [CI results and artifact instructions](INSTALL.md#native-target-matrix);
both workflows must pass on the same commit. CI uploads candidate artifacts after
successful package checks. This does not create a GitHub release or change `main`.

Known limits: trusted commands; no system sandbox or hard OS disk quota; external
and transitive inputs remain the user's responsibility; no native Windows
qualification; no macOS signing/notarization. Startup/import does not perform a
network request, install a dependency or run an experiment. Explicit dependency
installation and builds may use the network.

Use the [installation and compatibility guide](INSTALL.md) for setup, migration
and rollback. Original upstream attribution remains preserved separately from
Metimer's MIT-licensed contributions.
