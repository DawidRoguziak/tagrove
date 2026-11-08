"""Shared helpers for the /audit scripts: run directory, locking, atomic write-once files.

Everything an audit stage produces lives below `docs/review/<run>/` (ignored by git).
`run.json` holds the private dataset path, so it must never be committed or copied into
tracked files. Scripts set `run.json.halted`; agents never do.
"""
from __future__ import annotations

import contextlib
import datetime as _dt
import fcntl
import json
import os
import re
import tempfile
from pathlib import Path
from typing import Any, Iterator

SCHEMA_VERSION = 1
SKILL_DIR = Path(__file__).resolve().parent.parent
REFERENCES = SKILL_DIR / "references"
RUN_NAME = re.compile(r"^[0-9]{8}-[0-9]{4}(-[a-z0-9][a-z0-9-]{0,40})?(-[0-9]+)?$")
PRODUCTION_IDENTIFIERS = ("com.example.mediatagger", "com.example.mediatagger.dev")
E2E_IDENTIFIER = "com.example.mediatagger.e2e"


class AuditError(Exception):
    """A refusal or validation failure that the caller reports and exits non-zero for."""


def now() -> str:
    return _dt.datetime.now(_dt.timezone.utc).isoformat(timespec="seconds")


def repo_root(start: Path | None = None) -> Path:
    path = (start or Path.cwd()).resolve()
    for candidate in (path, *path.parents):
        if (candidate / ".git").exists() and (candidate / "src-tauri").is_dir():
            return candidate
    raise AuditError("Run from inside the Tagrove repository.")


def review_root(repo: Path) -> Path:
    return repo / "docs" / "review"


def resolve_run(run: str | None, repo: Path | None = None) -> Path:
    """Resolve --run (a run directory or name); default to docs/review/LATEST."""
    repo = repo or repo_root()
    root = review_root(repo)
    if run is None:
        latest = root / "LATEST"
        if not latest.is_file():
            raise AuditError("No run selected and docs/review/LATEST is missing; run /audit init first.")
        run = latest.read_text(encoding="utf-8").strip()
    candidate = Path(run)
    if not candidate.is_absolute():
        candidate = (root / candidate.name) if len(candidate.parts) == 1 else (repo / candidate)
    candidate = candidate.resolve()
    if candidate.parent != root.resolve() or not RUN_NAME.match(candidate.name):
        raise AuditError(f"Not an audit run directory below docs/review/: {run}")
    if not (candidate / "run.json").is_file():
        raise AuditError(f"Missing run.json in {candidate}")
    return candidate


def read_json(path: Path) -> Any:
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def atomic_write(path: Path, data: str | bytes, *, once: bool = False) -> None:
    """Write via a temporary sibling and rename. With once=True refuse an existing target."""
    path.parent.mkdir(parents=True, exist_ok=True)
    if once and path.exists():
        raise AuditError(f"Refusing to overwrite write-once file {path.name}")
    payload = data.encode("utf-8") if isinstance(data, str) else data
    fd, temp = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".part", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        if once:
            # link() fails if the target appeared meanwhile, unlike rename().
            os.link(temp, path)
            os.unlink(temp)
        else:
            os.replace(temp, path)
    except BaseException:
        with contextlib.suppress(FileNotFoundError):
            os.unlink(temp)
        raise


def write_json(path: Path, value: Any, *, once: bool = False) -> None:
    atomic_write(path, json.dumps(value, indent=2, ensure_ascii=False, sort_keys=False) + "\n", once=once)


@contextlib.contextmanager
def run_lock(run_dir: Path) -> Iterator[None]:
    """Serialize every state change of one run (fcntl advisory lock on <run>/.lock)."""
    with open(run_dir / ".lock", "a+") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def load_run(run_dir: Path) -> dict:
    return read_json(run_dir / "run.json")


def save_run(run_dir: Path, run: dict) -> None:
    write_json(run_dir / "run.json", run)


def update_run(run_dir: Path, mutate) -> dict:
    with run_lock(run_dir):
        run = load_run(run_dir)
        mutate(run)
        save_run(run_dir, run)
        return run


def halt(run_dir: Path, reason: str, source: str) -> dict:
    """Stop all later stages until a human inspects the run and runs clear_halt.py."""

    def mutate(run: dict) -> None:
        if not run.get("halted"):
            run["halted"] = {"reason": reason, "source": source, "time": now()}
        run.setdefault("halt_history", []).append({"reason": reason, "source": source, "time": now()})

    return update_run(run_dir, mutate)


def require_not_halted(run: dict) -> None:
    if run.get("halted"):
        halted = run["halted"]
        raise AuditError(
            f"Run is halted ({halted.get('source')}: {halted.get('reason')}). "
            "A human inspects it, then runs `clear_halt.py --confirm-inspected` (never an agent)."
        )


def within(path: Path | str, root: Path | str) -> bool:
    """True when realpath(path) equals or lies below realpath(root)."""
    real = os.path.realpath(path)
    base = os.path.realpath(root)
    return real == base or real.startswith(base.rstrip(os.sep) + os.sep)


def production_profile_dirs(home: Path | None = None) -> list[Path]:
    home = home or Path.home()
    bases = [home / ".local" / "share", home / ".config", home / ".cache"]
    return [base / ident for base in bases for ident in PRODUCTION_IDENTIFIERS]


def load_reference(name: str) -> Any:
    return read_json(REFERENCES / name)
