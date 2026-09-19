# Reproducible engine demonstrations

`lookup` replaces repeated linear scans with an index. `memoization` caches pure
calculations across repeated inputs. Each project has independent behavior checks
and a deterministic count of actual algorithm operations. These are functional
qualification examples, not claims about wall-clock speed or statistical power.

The editable scope is `src/mode.txt`. Tests, workload definitions and benchmarks
are protected. Both variants compute identical outputs. Python bytecode writes
are disabled so the frozen runtime remains unchanged.

From an extracted engine package, run:

```sh
python3 scripts/qualify.py --bundle .
```

This explicit command copies each example to a temporary Git repository, qualifies
the reference, evaluates an improvement, rejects a regression, exports a selected
result and checks that the source was preserved. It needs Git and Python 3.10+,
performs no dependency installation or network request, and leaves no source edits.
