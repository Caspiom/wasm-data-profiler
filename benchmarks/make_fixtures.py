"""Generate the benchmark datasets.

The seed is fixed, so the files are byte-identical on every machine and the
published numbers refer to a dataset anyone can reproduce.

    python3 benchmarks/make_fixtures.py benchmarks/data
"""

from __future__ import annotations

import random
import sys
from pathlib import Path

SEED = 7
CITIES = ["São Paulo", "Belém", "Curitiba", "Goiânia", "Maceió"]


def write_brazilian(path: Path, rows: int = 500) -> None:
    """Small file exercising the awkward parts: BOM, windows-1252, `;`, `1.234,56`."""
    rng = random.Random(SEED)
    lines = ["produto;preco;quantidade;disponivel;cidade;cadastro"]
    for i in range(rows):
        lines.append(
            f"Produto {i};{rng.randint(1, 9999)},{rng.randint(0, 99):02d};"
            f"{rng.randint(0, 500)};{rng.choice(['sim', 'não'])};"
            f"{rng.choice(CITIES)};2026-{rng.randint(1, 12):02d}-{rng.randint(1, 28):02d}"
        )
    path.write_bytes(b"\xef\xbb\xbf" + "\n".join(lines).encode("cp1252"))


def write_large(path: Path, rows: int = 1_500_000) -> None:
    """Plain UTF-8 file, one column of each type, ~51 MB."""
    rng = random.Random(SEED)
    with path.open("w", encoding="utf-8") as handle:
        handle.write("id,valor,categoria,ativo,data\n")
        for i in range(rows):
            handle.write(
                f"{i},{rng.gauss(100, 25):.2f},cat{i % 40},"
                f"{'true' if i % 3 else 'false'},2026-01-{i % 28 + 1:02d}\n"
            )


def main() -> None:
    target = Path(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/data")
    target.mkdir(parents=True, exist_ok=True)
    write_brazilian(target / "brasil.csv")
    write_large(target / "grande.csv")
    for file in sorted(target.iterdir()):
        print(f"{file}  {file.stat().st_size / 1048576:.1f} MB")


if __name__ == "__main__":
    main()
