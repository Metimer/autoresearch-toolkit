#!/usr/bin/env bash
# .auto/checks.sh — correctness backpressure for pi-autoresearch.
# Runs after every passing benchmark. Exit non-zero to block "keep".
# Keep stdout small: only errors. The loop sees the last 80 lines on failure.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
export CI=1

fail=0
run() {  # run <label> <command...>: quiet on success, tail on failure
  local label=$1; shift
  if ! out=$("$@" 2>&1); then
    echo "== $label FAILED"; echo "$out" | tail -40; fail=1
  fi
}

# ---- pick the blocks matching the stack; delete the others --------------------

# Node / TypeScript
# run "typecheck" npx tsc --noEmit -p .
# run "lint"      npx eslint . --quiet
# run "tests"     npm test --silent -- --reporter=dot

# Python
# run "tests"     python -m pytest -q -x --no-header -p no:cacheprovider
# run "types"     python -m mypy --no-error-summary .
# run "lint"      ruff check --quiet .

# Rust
# run "tests"     cargo test --quiet
# run "clippy"    cargo clippy --quiet -- -D warnings

# Go
# run "tests"     go test ./... 2>&1
# run "vet"       go vet ./...

# ---- test-count guard (use when the primary metric is the test suite) ---------
# baseline_file=".auto/baseline_test_count"
# count=$(<test-count-command>)
# if [[ -f "$baseline_file" ]]; then
#   base=$(cat "$baseline_file")
#   if (( count < base )); then echo "== TEST COUNT DROPPED: $count < $base"; fail=1; fi
# else
#   echo "$count" > "$baseline_file"
# fi

exit $fail
