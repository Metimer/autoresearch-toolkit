"""Demonstrate eliminating repeated pure calculations."""
from pathlib import Path


def workload():
    optimized = Path("src/mode.txt").read_text().strip() == "cached"
    cache = {}
    operations = 0
    values = []
    for n in [n % 20 for n in range(200)]:
        if optimized and n in cache:
            value = cache[n]
            operations += 1
        else:
            value = 0
            for i in range(n + 1):
                value += i * i
                operations += 1
            if optimized:
                cache[n] = value
        values.append(value)
    return values, operations
