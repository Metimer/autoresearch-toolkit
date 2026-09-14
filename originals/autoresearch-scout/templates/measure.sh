#!/usr/bin/env bash
# .auto/measure.sh — benchmark script for pi-autoresearch.
# Prints "METRIC <name>=<number>" lines to stdout. Everything else goes to .auto/measure.log.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# ---- fill these in -----------------------------------------------------------
METRIC_NAME="test_time_ms"          # must match init_experiment's metric_name
CMD='npm test --silent'             # the workload to time (quoted, single line)
RUNS=5                              # timed runs; median is reported
WARMUP=1                            # untimed warm-up runs
PRECHECK='node -e "require(\"./package.json\")"'   # <1s sanity check, fails fast
# --------------------------------------------------------------------------------

LOG=".auto/measure.log"; : > "$LOG"

# Pre-check: cheap, fails fast on broken edits.
if ! bash -c "$PRECHECK" >>"$LOG" 2>&1; then
  echo "PRECHECK FAILED — see $LOG" >&2
  exit 1
fi

now_ms() { python3 -c 'import time; print(int(time.time()*1000))'; }

for _ in $(seq 1 "$WARMUP"); do
  bash -c "$CMD" >>"$LOG" 2>&1 || { echo "WARMUP RUN FAILED — see $LOG" >&2; exit 1; }
done

times=()
for i in $(seq 1 "$RUNS"); do
  t0=$(now_ms)
  bash -c "$CMD" >>"$LOG" 2>&1 || { echo "RUN $i FAILED — see $LOG" >&2; exit 1; }
  t1=$(now_ms)
  times+=( $((t1 - t0)) )
done

median=$(printf '%s\n' "${times[@]}" | sort -n | awk '{a[NR]=$1} END{print (NR%2)?a[(NR+1)/2]:int((a[NR/2]+a[NR/2+1])/2)}')
min=$(printf '%s\n' "${times[@]}" | sort -n | head -1)
max=$(printf '%s\n' "${times[@]}" | sort -n | tail -1)

echo "METRIC ${METRIC_NAME}=${median}"
echo "METRIC run_min_ms=${min}"
echo "METRIC run_max_ms=${max}"

# ---- optional secondary metrics (uncomment / adapt) ---------------------------
# Guard against "optimizing" by removing tests: report the test count.
# count=$(grep -cE "^(ok|✓|PASSED)" "$LOG" || true); echo "METRIC test_count=${count}"
# Bundle size:
# echo "METRIC bundle_bytes=$(stat -f%z dist/main.js 2>/dev/null || stat -c%s dist/main.js)"
# Peak RSS of one run (macOS):  /usr/bin/time -l bash -c "$CMD" 2>&1 | awk '/maximum resident/{print "METRIC peak_rss_kb=" int($1/1024)}'
