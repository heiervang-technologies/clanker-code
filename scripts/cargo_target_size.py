#!/usr/bin/env python3
# Modified by Heiervang Technologies.

import os
from pathlib import Path


def directory_size(path: Path) -> int:
    total = 0
    for root, _dirs, files in os.walk(path):
        for filename in files:
            try:
                total += (Path(root) / filename).stat().st_size
            except FileNotFoundError:
                # Build processes can replace artifacts while this report runs.
                pass
    return total


def human_size(size: int) -> str:
    units = ("B", "KiB", "MiB", "GiB", "TiB")
    value = float(size)
    for unit in units:
        if value < 1024 or unit == units[-1]:
            return f"{value:.1f} {unit}"
        value /= 1024
    raise AssertionError("unreachable")


def main() -> None:
    repo_root = Path(__file__).resolve().parent.parent
    target_dir = Path(
        os.environ.get("CARGO_TARGET_DIR", repo_root / "codex-rs" / "target")
    ).resolve()
    if not target_dir.is_dir():
        print(f"Cargo target directory does not exist: {target_dir}")
        return

    entries = [
        (directory_size(path), path) for path in target_dir.iterdir() if path.is_dir()
    ]
    total = sum(size for size, _path in entries)

    print(f"Cargo target directory: {target_dir}")
    print(f"Total directory size: {human_size(total)}")
    for size, path in sorted(entries, reverse=True):
        print(f"{human_size(size):>10}  {path.name}")


if __name__ == "__main__":
    main()
