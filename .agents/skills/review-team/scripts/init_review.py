#!/usr/bin/env python3
"""Create a new UTC-named review folder without reusing previous reports."""

import argparse
from datetime import datetime, timezone
from pathlib import Path
import re


def create_review(root: Path, name: str) -> Path:
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")[:80].rstrip("-")
    if not slug:
        raise ValueError("Use a descriptive name containing ASCII letters or digits")
    started = datetime.now(timezone.utc)
    base = f"{started:%Y-%m-%d-%H%M%S}-{slug}"
    parent = root / "docs" / "review"
    parent.mkdir(parents=True, exist_ok=True)
    folder = parent / base
    suffix = 2
    while True:
        try:
            folder.mkdir()
            break
        except FileExistsError:
            folder = parent / f"{base}-{suffix}"
            suffix += 1
    for filename in ("README", "scope", "frontend", "backend", "architecture", "verification"):
        (folder / f"{filename}.md").write_text(
            f"# {slug} / {filename}\n\n"
            f"Started: {started:%Y-%m-%dT%H:%M:%SZ}\n\n"
            "Review status: partial\n\n"
            "Pending. No review or verification results recorded yet.\n",
            encoding="utf-8",
        )
    return folder


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name", help="Descriptive review name, converted to a path-safe slug")
    args = parser.parse_args()
    try:
        print(create_review(Path.cwd(), args.name).relative_to(Path.cwd()))
    except ValueError as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
