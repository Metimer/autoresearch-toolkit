# Third-party notices

Autoresearch Toolkit's new contributions are maintained by Metimer under the
root MIT license. The following reference code retains its original attribution.

## pi-autoresearch

- Location: `originals/pi-autoresearch/`
- Declared upstream: https://github.com/davebcn87/pi-autoresearch
- Archived package version: 1.8.1
- Copyright (c) 2026 Tobi Lutke, David Cortés
- License: MIT; the full notice is preserved in `originals/pi-autoresearch/LICENSE`.

The Rust engine is being implemented separately. Characterization tests execute
the archived JSONL reader to document its behavior; they do not load the Pi engine.
Unknown outcomes and malformed historical events must not become verified Rust
results. Any code ported from the archive must retain its applicable notices.

## Rust dependencies

Direct runtime dependencies are Serde, serde_json, fs2, sha2, tempfile and libc
under MIT OR Apache-2.0, and schemars under MIT. Schema validation tests additionally
use jsonschema under MIT. `Cargo.lock` records the exact dependency graph, including
transitive and test dependencies; this paragraph is not a full release license
inventory. Dependency license texts and copyright notices must accompany binary
release bundles. No binary release is published by this workspace; release
packaging remains a separate plan item.
