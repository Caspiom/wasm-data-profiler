"""Time the pandas engine.

Reports the median of the runs, never the best one: the best run is the one
that happened to have the friendliest cache, and quoting it is how benchmarks
become marketing.

    cd api && uv run python ../benchmarks/bench_pandas.py ../benchmarks/data/*.csv
"""

from __future__ import annotations

import platform
import statistics
import sys
from pathlib import Path

import pandas as pd
import pyarrow

from app.profiler import profile_csv

RUNS = 5


def main() -> None:
    print(
        f"python {platform.python_version()} | pandas {pd.__version__} | "
        f"pyarrow {pyarrow.__version__} | {platform.machine()} {platform.system()}"
    )
    for name in sys.argv[1:]:
        path = Path(name)
        raw = path.read_bytes()
        # One warm-up run so imports and lazy initialisation are not measured.
        profile_csv(raw)
        runs = [profile_csv(raw)[1].profile_ms for _ in range(RUNS)]
        print(
            f"{path.name:14} {len(raw) / 1048576:7.1f} MB  "
            f"median {statistics.median(runs):9.0f} ms  "
            f"min {min(runs):8.0f}  max {max(runs):8.0f}  n={RUNS}"
        )


if __name__ == "__main__":
    main()
