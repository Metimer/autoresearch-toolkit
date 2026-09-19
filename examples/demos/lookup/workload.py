"""Demonstrate replacing repeated scans with one lookup table."""
from pathlib import Path


def workload():
    rows = [(str(n), n * 3) for n in range(200)]
    queries = [str(n) for n in range(150)]
    optimized = Path("src/mode.txt").read_text().strip() == "indexed"
    operations = 0
    if optimized:
        index = dict(rows)
        operations += len(rows)
        values = [index[key] for key in queries]
        operations += len(queries)
    else:
        values = []
        for key in queries:
            for candidate, value in rows:
                operations += 1
                if candidate == key:
                    values.append(value)
                    break
    return values, operations
