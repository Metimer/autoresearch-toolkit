# Stack detection cheat sheet

| Marker file | Stack | Test | Typecheck / lint | Build | Notes |
|---|---|---|---|---|---|
| `package.json` + `pnpm-lock.yaml` | Node (pnpm) | `pnpm test --silent` | `pnpm tsc --noEmit`, `pnpm eslint . --quiet` | `pnpm build` | check `scripts` first; vitest: `--run --reporter=dot`; jest: `--silent --ci` |
| `package.json` + `package-lock.json` | Node (npm) | `npm test --silent` | `npx tsc --noEmit` | `npm run build` | |
| `package.json` + `bun.lock*` | Bun | `bun test` | `bunx tsc --noEmit` | `bun run build` | |
| `pyproject.toml` / `setup.cfg` / `pytest.ini` | Python | `python -m pytest -q -x` | `mypy .`, `ruff check .` | — | prefer the project's venv; `uv run` if `uv.lock` exists |
| `Cargo.toml` | Rust | `cargo test --quiet` | `cargo clippy -- -D warnings` | `cargo build --release` | build time is a good metric; use `--timings` for phases |
| `go.mod` | Go | `go test ./...` | `go vet ./...` | `go build ./...` | `go test -count=1` to defeat the cache in measure.sh |
| `Makefile` | any | `make test` | `make lint` | `make` | read targets; CI usually mirrors them |
| `.github/workflows/*.yml` | any | see `run:` steps | idem | idem | the most trustworthy source of the real commands |

Metric pitfalls:
- Test runners cache (jest, vitest, go test, cargo): disable caches in `measure.sh` or the metric is meaningless.
- Bundlers cache in `node_modules/.cache`, `.next`, `.turbo`: delete before each timed run or measure a cold build explicitly.
- Wall time on a laptop drifts with thermal state: warm-up runs and medians are mandatory, and the loop's confidence score handles the rest.
