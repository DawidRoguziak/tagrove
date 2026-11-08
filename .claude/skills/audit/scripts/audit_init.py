#!/usr/bin/env python3
"""Create an /audit run: docs/review/<run>/ with run.json, STATUS.md, context and smoke results.

  audit_init.py --dataset <path> [--name label] [--skip-smoke] [--models role=model,...]

Steps: run directory (collision-safe) and LATEST pointer; baseline HEAD, tree and dirty-state
hashes; tool inventory; dataset checks (for-claude-tests writable, socket length); dataset
manifest (dataset_guard snapshot); copies of batches.json/models.json/qa-blocks.json into
context/; hook installation check; smoke tests (bwrap + GTK file dialog on a private Xvfb,
reflink across the sandbox's bind mounts). The alert path cannot be smoke-tested without an
app session and is recorded as deferred to the first QA session.
The dataset path is stored only in the ignored run.json, never in tracked files.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402
import audit_state  # noqa: E402
import dataset_guard  # noqa: E402
import qa_session  # noqa: E402

REQUIRED_TOOLS = ["git", "bun", "node", "python3", "cargo", "bwrap", "Xvfb", "xvfb-run", "xauth", "import",
                  "ffmpeg", "ffprobe", "sqlite3", "setsid", "dbus-run-session", "cp"]
OPTIONAL_TOOLS = ["gitleaks", "cargo-deny", "cargo-audit", "knip", "xdotool"]
GTK_SMOKE = r"""
import gi, sys
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib
dialog = Gtk.FileChooserDialog(title="audit smoke", action=Gtk.FileChooserAction.SELECT_FOLDER)
dialog.add_button("Cancel", Gtk.ResponseType.CANCEL)
shown = []
def done():
    shown.append(dialog.get_visible())
    dialog.destroy()
    Gtk.main_quit()
    return False
dialog.show_all()
GLib.timeout_add(800, done)
Gtk.main()
print("gtk-dialog-ok" if shown == [True] else "gtk-dialog-not-visible")
"""


def run_name(root: Path, label: str | None, stamp: str) -> Path:
    base = stamp + (f"-{label}" if label else "")
    candidate, index = root / base, 1
    while candidate.exists():
        index += 1
        candidate = root / f"{base}-{index}"
    return candidate


def git(repo: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True, text=True).stdout.strip()


def baseline(repo: Path) -> dict:
    dirty = subprocess.run(["git", "diff", "HEAD", "--binary"], cwd=repo, check=True, capture_output=True).stdout
    untracked = git(repo, "ls-files", "--others", "--exclude-standard")
    return {
        "head": git(repo, "rev-parse", "HEAD"),
        "tree": git(repo, "rev-parse", "HEAD^{tree}"),
        "branch": git(repo, "rev-parse", "--abbrev-ref", "HEAD"),
        "dirty_sha256": hashlib.sha256(dirty + untracked.encode()).hexdigest(),
        "dirty": bool(dirty or untracked),
    }


def tool_inventory() -> dict:
    found = {}
    for name in REQUIRED_TOOLS + OPTIONAL_TOOLS:
        found[name] = shutil.which(name) is not None
    found["tauri-driver"] = bool(os.environ.get("TAURI_DRIVER_PATH") or shutil.which("tauri-driver")
                                 or (Path.home() / ".cargo" / "bin" / "tauri-driver").exists())
    found["WebKitWebDriver"] = Path(os.environ.get("WEBKIT_WEBDRIVER_PATH", "/usr/bin/WebKitWebDriver")).exists()
    missing = [name for name in REQUIRED_TOOLS + ["tauri-driver", "WebKitWebDriver"] if not found[name]]
    return {"found": found, "missing_required": missing,
            "missing_optional": [name for name in OPTIONAL_TOOLS if not found[name]]}


def check_dataset(dataset: Path) -> dict:
    real = Path(os.path.realpath(dataset))
    if not real.is_dir():
        raise lib.AuditError("--dataset is not a directory")
    writable = real / "for-claude-tests"
    if not writable.is_dir() or not os.access(writable, os.W_OK):
        raise lib.AuditError("the dataset needs a writable for-claude-tests/ folder")
    tmpdir = writable / "q"
    length = qa_session.socket_path_length(str(tmpdir if tmpdir.exists() else writable)) + (0 if tmpdir.exists() else 2)
    if length > qa_session.SOCKET_LIMIT:
        raise lib.AuditError(f"the control socket path below for-claude-tests/q would be {length} bytes "
                             f"(Linux allows {qa_session.SOCKET_LIMIT}); use a dataset with a shorter path")
    return {"dataset": str(real), "qa_tmpdir": str(tmpdir), "socket_path_bytes": length}


def smoke_reflink(dataset: str, tmpdir: str) -> dict:
    """Clone one small dataset file into for-claude-tests/q across the read-only/writable binds."""
    source = None
    for root, dirs, files in os.walk(dataset):
        dirs[:] = sorted(d for d in dirs if os.path.join(root, d) != os.path.join(dataset, "for-claude-tests"))
        for name in sorted(files):
            path = os.path.join(root, name)
            if not os.path.islink(path) and os.path.getsize(path) > 0:
                source = path
                break
        if source:
            break
    if not source:
        return {"status": "skipped", "detail": "no regular file in the dataset"}
    os.makedirs(tmpdir, mode=0o700, exist_ok=True)
    work = tempfile.mkdtemp(prefix="smoke-reflink-", dir=tmpdir)
    target = os.path.join(work, "clone")
    argv = ["bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--bind", work, work,
            "--unshare-net", "--die-with-parent", "cp", "--reflink=always", "--", source, target]
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=120)
        return {"status": "pass" if result.returncode == 0 else "fail", "detail": result.stderr[-500:]}
    finally:
        shutil.rmtree(work, ignore_errors=True)


def smoke_gtk(repo: Path, dataset: str, tmpdir: str) -> dict:
    """A GTK file dialog must open inside the same bwrap + dbus sandbox as a QA session."""
    runtime = tempfile.mkdtemp(prefix=qa_session.RUNTIME_PREFIX, dir="/tmp")
    os.chmod(runtime, 0o700)
    try:
        argv = qa_session.sandbox_argv(str(repo), dataset, str(Path.home()), runtime, tmpdir, False)
        split = argv.index("bun")
        argv = argv[:split] + ["python3", "-c", GTK_SMOKE]
        full = ["xvfb-run", "-a", "-s", "-screen 0 1024x768x24 -nolisten tcp", *argv]
        env = {**qa_session.session_env(), "GDK_BACKEND": "x11"}
        result = subprocess.run(full, cwd=repo, env=env, capture_output=True, text=True, timeout=180)
        ok = "gtk-dialog-ok" in result.stdout
        return {"status": "pass" if ok else "fail", "detail": (result.stdout + result.stderr)[-800:]}
    finally:
        shutil.rmtree(runtime, ignore_errors=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--name")
    parser.add_argument("--skip-smoke", action="store_true")
    parser.add_argument("--models")
    args = parser.parse_args(argv)
    try:
        repo = lib.repo_root()
        if args.name and not audit_state.SAFE_NAME.match(args.name):
            raise lib.AuditError("--name may use letters, digits, dot, dash and underscore")
        overrides = audit_state.parse_overrides(args.models)
        data = check_dataset(Path(args.dataset))
        if lib.within(data["dataset"], repo) or lib.within(repo, data["dataset"]):
            raise lib.AuditError("the dataset and the repository must not contain each other")
        root = lib.review_root(repo)
        root.mkdir(parents=True, exist_ok=True)
        label = (args.name or "").lower() or None
        run_dir = run_name(root, label, time.strftime("%Y%m%d-%H%M"))
        (run_dir / "context").mkdir(parents=True)
        for name in ("batches.json", "models.json", "qa-blocks.json", "recon-candidates.json"):
            shutil.copy2(lib.REFERENCES / name, run_dir / "context" / name)
        tools = tool_inventory()
        lib.write_json(run_dir / "context" / "tools.json", tools)
        run = {
            "schema": lib.SCHEMA_VERSION,
            "run": run_dir.name,
            "created": lib.now(),
            "baseline": baseline(repo),
            "dataset": data["dataset"],
            "qa_tmpdir": data["qa_tmpdir"],
            "socket_path_bytes": data["socket_path_bytes"],
            "model_overrides": overrides,
            "halted": None,
            "active_session": None,
            "sessions": [],
            "smoke": {},
            "hooks_missing": audit_state.hooks_missing(repo),
        }
        lib.save_run(run_dir, run)
        lib.atomic_write(root / "LATEST", run_dir.name + "\n")
        manifest = dataset_guard.snapshot(Path(data["dataset"]))
        lib.write_json(run_dir / "context" / "dataset-manifest.json", manifest)
        smoke = {"socket_length": {"status": "pass", "bytes": data["socket_path_bytes"]},
                 "alert": {"status": "deferred", "detail": "needs an app session; QA-10 proves alert handling"}}
        if args.skip_smoke:
            smoke["bwrap_gtk"] = smoke["reflink"] = {"status": "skipped"}
        else:
            smoke["reflink"] = smoke_reflink(data["dataset"], data["qa_tmpdir"])
            smoke["bwrap_gtk"] = smoke_gtk(repo, data["dataset"], data["qa_tmpdir"])
        lib.update_run(run_dir, lambda r: r.__setitem__("smoke", smoke))
        guard = dataset_guard.check(Path(data["dataset"]), run_dir / "context" / "dataset-manifest.json",
                                    run_dir, "after-init")
        with lib.run_lock(run_dir):
            audit_state.status(repo, run_dir)
    except (lib.AuditError, OSError, subprocess.SubprocessError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps({
        "ok": True,
        "run": run_dir.relative_to(repo).as_posix(),
        "files": manifest["counts"]["files"],
        "missing_required_tools": tools["missing_required"],
        "missing_optional_tools": tools["missing_optional"],
        "hooks_missing": run["hooks_missing"],
        "smoke": {k: v["status"] for k, v in smoke.items()},
        "guard_clean": guard["clean"],
    }, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
