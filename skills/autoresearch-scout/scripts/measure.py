#!/usr/bin/env python3
"""Bounded POSIX wall-time sampling; stdlib only, no implicit shell or Git writes."""
from __future__ import annotations

import argparse
import math
import os
import re
import signal
import statistics
import subprocess
import sys
import time


def number(value: str) -> float:
    result = float(value)
    if not math.isfinite(result) or result <= 0:
        raise argparse.ArgumentTypeError("must be finite and positive")
    return result


def sample(command: list[str], timeout: float) -> float:
    start = time.perf_counter()
    process = subprocess.Popen(
        command, stdout=sys.stderr, stderr=sys.stderr, start_new_session=True
    )
    try:
        status = process.wait(timeout=timeout)
    except BaseException:
        # Kill only the group created for this workload, including ordinary children.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        raise
    elapsed = (time.perf_counter() - start) * 1000
    if status:
        raise subprocess.CalledProcessError(status, command)
    return elapsed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", default="bench_ms")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--timeout", type=number, default=60.0)
    parser.add_argument("--budget", type=number, default=360.0)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if os.name != "posix":
        parser.error("POSIX host required; on Windows use WSL")
    if not command:
        parser.error("provide a workload after --")
    if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", args.name):
        parser.error("invalid metric name")
    if args.name in {"run_min_ms", "run_max_ms"}:
        parser.error("metric name reserved for diagnostics")
    if not 1 <= args.runs <= 31 or not 0 <= args.warmup <= 31:
        parser.error("runs must be 1..31; warmup must be 0..31")
    if args.timeout > 600 or args.budget > 3600:
        parser.error("timeout max 600s; total budget max 3600s")
    deadline = time.monotonic() + args.budget
    values = []
    try:
        for index in range(args.warmup + args.runs):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("total measurement budget exhausted")
            value = sample(command, min(args.timeout, remaining))
            if index >= args.warmup:
                values.append(value)
    except (subprocess.TimeoutExpired, TimeoutError) as exc:
        print(f"MEASUREMENT TIMEOUT: {exc}", file=sys.stderr)
        return 124
    except (subprocess.CalledProcessError, OSError) as exc:
        print(f"MEASUREMENT FAILED: {exc}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("MEASUREMENT INTERRUPTED", file=sys.stderr)
        return 130
    print(f"METRIC {args.name}={statistics.median(values):.6f}")
    print(f"METRIC run_min_ms={min(values):.6f}")
    print(f"METRIC run_max_ms={max(values):.6f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
