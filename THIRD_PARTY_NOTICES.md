# Third-party notices

Autoresearch Toolkit's new contributions are maintained by Metimer under the
root MIT license. The following reference code retains its original attribution.

## pi-autoresearch

- Location: `originals/pi-autoresearch/`
- Declared upstream: https://github.com/davebcn87/pi-autoresearch
- Archived package version: 1.8.1
- Copyright (c) 2026 Tobi Lutke, David Cortés
- License: MIT; the full notice is preserved in `originals/pi-autoresearch/LICENSE`.

The Rust engine is implemented separately. Characterization tests execute
the archived JSONL reader to document its behavior; they do not load the Pi engine.
Unknown outcomes and malformed historical events must not become verified Rust
results. Any code ported from the archive must retain its applicable notices.

## Rust dependencies

Direct runtime dependencies are Serde, serde_json, fs2, sha2, tempfile, libc,
rustix and signal-hook under MIT OR Apache-2.0, and schemars under MIT. Schema validation tests additionally
use jsonschema under MIT. `Cargo.lock` records the exact dependency graph, including
transitive and test dependencies; this paragraph is not a full release license
inventory. Engine archives produced by `scripts/release.py` include
`DEPENDENCIES.json`, original license and notice files for the resolved normal/build
dependency graph, and Rust distribution notices in `licenses/`. The builder refuses
missing license evidence. Test-only dependencies are not part of this binary
inventory. No binary release is published automatically by this workspace.

## Optional Pi adapter dependencies

`adapters/pi/` is new Toolkit code under the root MIT license. Its runtime schema
dependency TypeBox 1.3.7 is MIT (copyright Haydn Paterson); its host peer dependency
is `@earendil-works/pi-coding-agent` 0.85.1, declared MIT by its package metadata.
TypeScript and Node.js type declarations are development dependencies. The adapter
lockfile records direct and transitive packages; dependencies are installed
separately and are not vendored or included in portable skill exports. Any future
adapter release bundling dependencies must include their applicable notices and
license texts. The original archived extension is not loaded by this adapter.
