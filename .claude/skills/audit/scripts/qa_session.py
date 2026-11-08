#!/usr/bin/env python3
"""QA session lifecycle for /audit: one app:control session at a time, sandboxed, monitored.

  prebuild [--perf]          E2E build outside the sandbox with the same VITE_MEDIATAGGER_PERF.
  start --block S1 [--perf]  Start `bun run app:control start` in a new session (setsid) inside
                             bubblewrap + dbus-run-session; return at once. Also starts `monitor`.
  wait [--timeout 540]       Wait for READY; record the session ID and media path.
  stop                       chmod -R u+rwX the copy, `app:control stop <id>` with the run's
                             TMPDIR, verify cleanup, run the dataset guard, clear active_session.
  reap                       Stop or clear whatever active_session run.json still records.
  guard [--label L]          Dataset guard against the init manifest (dirty => halted).
  mark --block B --suite S begin|end   Suite boundaries for triage.
  maintenance --minutes N    Suspend the watchdog during a long backup export/import (<=120).
  triage --block B           Mechanical log triage into qa/runs/<B>/triage-raw.json.
  monitor                    (internal) 2 s sampler, write-fd guard, and 45 min watchdog.
TMPDIR always comes from run.json (`qa_tmpdir`). Never kill processes by name; the only
signals sent go to process groups this script started and recorded.
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402
import dataset_guard  # noqa: E402

SOCKET_SUFFIX = "/media-tagger-control-XXXXXX/control.sock"
SOCKET_LIMIT = 107
WATCHDOG_SECONDS = 45 * 60
STARTUP_GRACE_SECONDS = 60 * 60
SAMPLE_SECONDS = 2
RUNTIME_PREFIX = "mediatagger-qa-rt-"
# Home folders the in-sandbox build may write (cargo registry and locks, bun cache, tool caches).
BUILD_CACHES = (".cargo", ".bun", ".cache")
SCRUBBED_ENV = ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS",
                "XDG_RUNTIME_DIR", "VITE_MEDIATAGGER_PERF", "TMPDIR")


# ------------------------------------------------------------------ pure helpers (tested)


def socket_path_length(tmpdir: str) -> int:
    return len(os.path.realpath(tmpdir).encode()) + len(SOCKET_SUFFIX)


def sandbox_argv(repo: str, dataset: str, home: str, runtime_dir: str, tmpdir: str, perf: bool,
                 existing=os.path.isdir) -> list[str]:
    """bwrap outside, dbus-run-session inside. $HOME and the dataset are read-only; only the repo,
    for-claude-tests and the build caches are writable below them; production profile folders
    are replaced by empty tmpfs. /tmp and devices stay as on the host."""
    writable = os.path.join(dataset, "for-claude-tests")
    require_inside(tmpdir, writable, "QA TMPDIR must be below for-claude-tests")
    argv = ["bwrap", "--dev-bind", "/", "/", "--ro-bind", home, home, "--ro-bind", dataset, dataset]
    for cache in BUILD_CACHES:
        path = os.path.join(home, cache)
        if existing(path):
            argv += ["--bind", path, path]
    argv += ["--bind", repo, repo, "--bind", writable, writable]
    for profile in lib.production_profile_dirs(Path(home)):
        if existing(str(profile)):
            argv += ["--tmpfs", str(profile)]
    env = ["env", f"XDG_RUNTIME_DIR={runtime_dir}", "GSETTINGS_BACKEND=memory", "GTK_USE_PORTAL=0",
           "NO_AT_BRIDGE=1", "MEDIATAGGER_PERF=1", f"TMPDIR={tmpdir}"]
    if perf:
        env.append("VITE_MEDIATAGGER_PERF=1")
    return [*argv, "--", "dbus-run-session", "--", *env, "bun", "run", "app:control", "start"]


def require_inside(path: str, root: str, message: str) -> None:
    if not lib.within(path, root):
        raise lib.AuditError(message)


def session_env() -> dict:
    env = {k: v for k, v in os.environ.items() if k not in SCRUBBED_ENV}
    return env


def parse_ready(text: str) -> dict | None:
    for line in text.splitlines():
        if line.startswith("READY "):
            return json.loads(line[len("READY "):])
    return None


def descendants(root_pid: int, proc: str = "/proc") -> list[int]:
    parents: dict[int, list[int]] = {}
    for name in os.listdir(proc):
        if not name.isdigit():
            continue
        try:
            with open(f"{proc}/{name}/stat", encoding="utf-8") as handle:
                stat = handle.read()
        except OSError:
            continue
        fields = stat[stat.rfind(")") + 2:].split()
        parents.setdefault(int(fields[1]), []).append(int(name))
    found, queue = [], [root_pid]
    while queue:
        pid = queue.pop()
        if pid in found:
            continue
        found.append(pid)
        queue.extend(parents.get(pid, []))
    return found


def write_fd_violations(pids: list[int], dataset: str, profiles: list[str], proc: str = "/proc",
                        home: str | None = None, allowed: list[str] | None = None) -> list[dict]:
    """Open-for-write descriptors under the dataset (outside for-claude-tests), a production
    profile, or $HOME outside the allowed roots (repo, for-claude-tests, build caches)."""
    dataset = os.path.realpath(dataset)
    writable = os.path.join(dataset, "for-claude-tests")
    allowed = [os.path.realpath(path) for path in (allowed or [])] + [writable]
    violations = []
    for pid in pids:
        try:
            fds = os.listdir(f"{proc}/{pid}/fd")
        except OSError:
            continue
        for fd in fds:
            try:
                target = os.readlink(f"{proc}/{pid}/fd/{fd}")
                with open(f"{proc}/{pid}/fdinfo/{fd}", encoding="utf-8") as handle:
                    flags = next(int(line.split()[1], 8) for line in handle if line.startswith("flags:"))
            except (OSError, StopIteration, ValueError):
                continue
            if flags & 0o3 == 0 or not target.startswith("/"):
                continue
            target = target.removesuffix(" (deleted)")
            in_dataset = lib.within(target, dataset) and not lib.within(target, writable)
            in_profile = any(lib.within(target, profile) for profile in profiles)
            in_home = bool(home) and lib.within(target, home) and not any(lib.within(target, root) for root in allowed)
            if in_dataset or in_profile or in_home:
                violations.append({"pid": pid, "fd": fd, "target": target, "flags": oct(flags)})
    return violations


def sample(pid: int, proc: str = "/proc") -> dict | None:
    try:
        with open(f"{proc}/{pid}/stat", encoding="utf-8") as handle:
            stat = handle.read()
        with open(f"{proc}/{pid}/status", encoding="utf-8") as handle:
            rss = next((int(line.split()[1]) for line in handle if line.startswith("VmRSS:")), 0)
    except OSError:
        return None
    comm = stat[stat.find("(") + 1: stat.rfind(")")]
    fields = stat[stat.rfind(")") + 2:].split()
    return {"pid": pid, "comm": comm, "rss_kb": rss, "cpu_ticks": int(fields[11]) + int(fields[12])}


def alive(pid: int | None) -> bool:
    if not pid:
        return False
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8") as handle:
            return handle.read().rsplit(")", 1)[1].split()[0] != "Z"
    except OSError:
        return False


def signal_group(pgid: int | None, sig: int) -> None:
    if pgid and alive(pgid):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            pass


# ------------------------------------------------------------------ commands


def session_dir(run_dir: Path) -> Path:
    path = run_dir / "qa" / "sessions"
    path.mkdir(parents=True, exist_ok=True)
    return path


def cmd_prebuild(repo: Path, run_dir: Path, perf: bool) -> dict:
    lib.require_not_halted(lib.load_run(run_dir))
    if (repo / "artifacts" / "app-control" / "active.lock").exists():
        raise lib.AuditError("an app:control session owns the checkout (active.lock); do not build now")
    env = session_env()
    if perf:
        env["VITE_MEDIATAGGER_PERF"] = "1"
    log = session_dir(run_dir) / f"prebuild-{'perf' if perf else 'plain'}-{int(time.time())}.log"
    with open(log, "w", encoding="utf-8") as handle:
        code = subprocess.run(["bun", "run", "tauri:build:e2e"], cwd=repo, env=env, stdout=handle,
                              stderr=subprocess.STDOUT, timeout=3600).returncode
    lib.update_run(run_dir, lambda run: run.__setitem__("last_build", {"perf": perf, "code": code, "time": lib.now()}))
    if code != 0:
        raise lib.AuditError(f"E2E build failed ({code}); see {log.relative_to(repo)}")
    return {"ok": True, "perf": perf, "log": str(log.relative_to(repo))}


def cmd_start(repo: Path, run_dir: Path, block: str, perf: bool) -> dict:
    run = lib.load_run(run_dir)
    lib.require_not_halted(run)
    if run.get("active_session"):
        raise lib.AuditError("an active_session is already recorded; run `qa_session.py reap` first")
    if (repo / "artifacts" / "app-control" / "active.lock").exists():
        raise lib.AuditError("artifacts/app-control/active.lock exists; inspect it, never delete it blindly")
    last = run.get("last_build") or {}
    if last.get("perf") != perf or last.get("code") != 0:
        raise lib.AuditError(f"run `qa_session.py prebuild{' --perf' if perf else ''}` first (build flag must match)")
    tmpdir = run["qa_tmpdir"]
    os.makedirs(tmpdir, mode=0o700, exist_ok=True)
    if socket_path_length(tmpdir) > SOCKET_LIMIT:
        raise lib.AuditError(f"control socket path would be {socket_path_length(tmpdir)} bytes (> {SOCKET_LIMIT})")
    runtime_dir = tempfile.mkdtemp(prefix=RUNTIME_PREFIX, dir="/tmp")
    os.chmod(runtime_dir, 0o700)
    index = len(run.get("sessions", [])) + 1
    log = session_dir(run_dir) / f"{index:02d}-{block}{'-perf' if perf else ''}.log"
    argv = sandbox_argv(str(repo), run["dataset"], str(Path.home()), runtime_dir, tmpdir, perf)
    with open(log, "w", encoding="utf-8") as handle:
        handle.write(f"# started {lib.now()} block={block} perf={perf}\n")
        handle.flush()
        process = subprocess.Popen(argv, cwd=repo, env=session_env(), stdin=subprocess.DEVNULL,
                                   stdout=handle, stderr=subprocess.STDOUT, start_new_session=True)
    active = {"index": index, "block": block, "perf": perf, "pid": process.pid, "pgid": process.pid,
              "log": str(log.relative_to(repo)), "runtime_dir": runtime_dir, "status": "starting",
              "started": lib.now(), "started_epoch": time.time()}
    lib.update_run(run_dir, lambda r: r.__setitem__("active_session", active))
    monitor_log = open(session_dir(run_dir) / f"{index:02d}-{block}-monitor.log", "w", encoding="utf-8")
    monitor = subprocess.Popen([sys.executable, "-I", os.path.abspath(__file__), "--run", str(run_dir), "monitor"],
                               cwd=repo, stdin=subprocess.DEVNULL, stdout=monitor_log, stderr=subprocess.STDOUT,
                               start_new_session=True)
    lib.update_run(run_dir, lambda r: r["active_session"].__setitem__("monitor_pid", monitor.pid))
    return {"ok": True, "block": block, "pid": process.pid, "log": active["log"]}


def cmd_wait(repo: Path, run_dir: Path, timeout: int) -> dict:
    deadline = time.time() + max(1, min(timeout, 540))
    while True:
        run = lib.load_run(run_dir)
        active = run.get("active_session")
        if not active:
            raise lib.AuditError("no active session")
        if active.get("status") == "ready":
            return {"ok": True, "status": "ready", "id": active["id"], "media": active["media"]}
        text = (repo / active["log"]).read_text(encoding="utf-8", errors="replace")
        ready = parse_ready(text)
        if ready:
            manifest = lib.read_json(repo / "artifacts" / "app-control" / ready["id"] / "session.json")

            def mutate(r: dict) -> None:
                r["active_session"].update(status="ready", id=ready["id"], media=manifest["media"],
                                           scratch=manifest["scratch"], display=manifest.get("display"),
                                           evidence=f"artifacts/app-control/{ready['id']}")

            lib.update_run(run_dir, mutate)
            return {"ok": True, "status": "ready", "id": ready["id"], "media": manifest["media"]}
        if "Startup failed" in text or not alive(active["pid"]):
            lib.update_run(run_dir, lambda r: r["active_session"].__setitem__("status", "failed"))
            return {"ok": False, "status": "failed", "log": active["log"], "tail": text[-3000:]}
        if time.time() >= deadline:
            return {"ok": False, "status": "starting", "hint": "call wait again"}
        time.sleep(3)


def _cleanup_runtime(path: str | None) -> None:
    if not path:
        return
    real = os.path.realpath(path)
    if (os.path.dirname(real) == "/tmp" and os.path.basename(real).startswith(RUNTIME_PREFIX)
            and not os.path.islink(path) and os.stat(real).st_uid == os.getuid()):
        shutil.rmtree(real, ignore_errors=True)


def _finish(run_dir: Path, active: dict, outcome: dict) -> None:
    def mutate(run: dict) -> None:
        run.setdefault("sessions", []).append({**{k: v for k, v in active.items() if k != "started_epoch"},
                                               "stopped": lib.now(), "outcome": outcome})
        run["active_session"] = None

    lib.update_run(run_dir, mutate)
    signal_group(active.get("monitor_pid"), signal.SIGTERM)
    _cleanup_runtime(active.get("runtime_dir"))


def cmd_stop(repo: Path, run_dir: Path, reason: str = "requested") -> dict:
    run = lib.load_run(run_dir)
    active = run.get("active_session")
    if not active:
        return {"ok": True, "status": "none"}
    outcome: dict = {"reason": reason}
    session_id = active.get("id")
    if session_id:
        media = active.get("media", "")
        if lib.within(media, run["qa_tmpdir"]) and os.path.isdir(media) and not os.path.islink(media):
            subprocess.run(["chmod", "-R", "u+rwX", "--", media], check=False)
        env = {**session_env(), "TMPDIR": run["qa_tmpdir"]}
        stop = subprocess.run(["bun", "run", "app:control", "stop", session_id], cwd=repo, env=env,
                              capture_output=True, text=True, timeout=300)
        outcome["stop_code"] = stop.returncode
        outcome["stop_output"] = (stop.stdout + stop.stderr)[-2000:]
        manifest = lib.read_json(repo / "artifacts" / "app-control" / session_id / "session.json")
        outcome["manifest_status"] = manifest.get("status")
        outcome["scratch_removed"] = not os.path.exists(manifest["scratch"])
    else:
        signal_group(active.get("pgid"), signal.SIGTERM)
        for _ in range(60):
            if not alive(active.get("pid")):
                break
            time.sleep(1)
        outcome["startup_only"] = True
    for _ in range(60):
        if not alive(active.get("pid")):
            break
        time.sleep(1)
    outcome["controller_exited"] = not alive(active.get("pid"))
    lock_left = (repo / "artifacts" / "app-control" / "active.lock").exists()
    ok = outcome["controller_exited"] and not lock_left and (
        not session_id or (outcome.get("manifest_status") == "stopped" and outcome.get("scratch_removed")))
    guard = dataset_guard.check(Path(run["dataset"]), run_dir / "context" / "dataset-manifest.json", run_dir,
                                f"after-stop-{active.get('block')}")
    outcome["guard_clean"] = guard["clean"]
    if ok:
        _finish(run_dir, active, outcome)
    else:
        lib.halt(run_dir, f"session stop did not complete cleanly: {json.dumps(outcome)[:600]}", "qa_session.stop")
    return {"ok": ok and guard["clean"], **outcome}


def cmd_reap(repo: Path, run_dir: Path) -> dict:
    run = lib.load_run(run_dir)
    active = run.get("active_session")
    if not active:
        return {"ok": True, "status": "none"}
    if alive(active.get("pid")):
        return cmd_stop(repo, run_dir, "reap")
    session_id = active.get("id")
    lock_left = (repo / "artifacts" / "app-control" / "active.lock").exists()
    if session_id:
        manifest = lib.read_json(repo / "artifacts" / "app-control" / session_id / "session.json")
        if manifest.get("status") == "stopped" and not lock_left:
            _finish(run_dir, active, {"reason": "reap", "already_stopped": True})
            return {"ok": True, "status": "cleared"}
    elif not lock_left:
        _finish(run_dir, active, {"reason": "reap", "startup_never_ready": True})
        return {"ok": True, "status": "cleared"}
    lib.halt(run_dir, "controller exited without cleanup; recover manually as in verify-mediatagger "
             "Cleanup (inspect active.lock and process groups; never delete a lock blindly)", "qa_session.reap")
    return {"ok": False, "status": "halted"}


def cmd_mark(run_dir: Path, block: str, suite: str, edge: str) -> dict:
    if not re.fullmatch(r"QA-(\d{2}|T\d{1,3})", suite):
        raise lib.AuditError("suite must be QA-NN or QA-TN")
    path = run_dir / "qa" / "runs" / block / "marks.jsonl"
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps({"time": lib.now(), "epoch": time.time(), "suite": suite, "edge": edge}) + "\n")
    return {"ok": True}


def cmd_maintenance(run_dir: Path, minutes: int) -> dict:
    if not 1 <= minutes <= 120:
        raise lib.AuditError("--minutes must be 1..120")
    until = time.time() + minutes * 60

    def mutate(run: dict) -> None:
        if not run.get("active_session"):
            raise lib.AuditError("no active session")
        run["active_session"]["maintenance_until"] = until

    lib.update_run(run_dir, mutate)
    return {"ok": True, "maintenance_until": until}


def monitor(repo: Path, run_dir: Path) -> int:
    run = lib.load_run(run_dir)
    active = run.get("active_session") or {}
    root = active.get("pid")
    profiles = [str(p) for p in lib.production_profile_dirs()]
    samples = session_dir(run_dir) / f"{active.get('index', 0):02d}-{active.get('block')}-samples.csv"
    with open(samples, "a", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(["epoch", "pid", "comm", "rss_kb", "cpu_ticks"])
        while alive(root):
            current = (lib.load_run(run_dir).get("active_session") or {})
            if current.get("pid") != root:
                return 0
            pids = descendants(root)
            stamp = round(time.time(), 1)
            for pid in pids:
                row = sample(pid)
                if row:
                    writer.writerow([stamp, row["pid"], row["comm"], row["rss_kb"], row["cpu_ticks"]])
            handle.flush()
            home = str(Path.home())
            violations = write_fd_violations(pids, run["dataset"], profiles, home=home,
                                             allowed=[str(repo), *(os.path.join(home, c) for c in BUILD_CACHES)])
            if violations:
                lib.halt(run_dir, f"write descriptor outside the copy: {json.dumps(violations[:5])}", "qa_session.monitor")
                cmd_stop(repo, run_dir, "monitor: write fd outside the copy")
                return 3
            evidence = current.get("evidence")
            actions = repo / evidence / "actions.jsonl" if evidence else None
            last = actions.stat().st_mtime if actions and actions.exists() else current.get("started_epoch", stamp)
            limit = WATCHDOG_SECONDS if current.get("status") == "ready" else STARTUP_GRACE_SECONDS
            if stamp - last > limit and stamp > current.get("maintenance_until", 0):
                cmd_stop(repo, run_dir, f"watchdog: no actions for {int(stamp - last)} s")
                return 4
            time.sleep(SAMPLE_SECONDS)
    return 0


def triage(repo: Path, run_dir: Path, block: str) -> dict:
    run = lib.load_run(run_dir)
    sessions = [s for s in run.get("sessions", []) if s.get("block") == block and s.get("id")]
    marks_path = run_dir / "qa" / "runs" / block / "marks.jsonl"
    marks = [json.loads(line) for line in marks_path.read_text(encoding="utf-8").splitlines()] if marks_path.exists() else []
    intervals: list[tuple[str, str, str | None]] = []
    open_suites: dict[str, str] = {}
    for mark in marks:
        if mark["edge"] == "begin":
            open_suites[mark["suite"]] = mark["time"]
        elif mark["suite"] in open_suites:
            intervals.append((mark["suite"], open_suites.pop(mark["suite"]), mark["time"]))
    intervals += [(suite, start, None) for suite, start in open_suites.items()]

    def suite_at(stamp: str | None) -> str:
        if not stamp:
            return "unknown"
        for suite, start, end in intervals:
            if start <= stamp and (end is None or stamp <= end):
                return suite
        return "between-suites"

    entries = []
    pattern = re.compile(r"panic|panicked|\bERROR\b|\berror\b|\bWARN\b|CRITICAL|Gtk-WARNING|segfault|Traceback", re.I)
    for session in sessions:
        evidence = repo / "artifacts" / "app-control" / session["id"]
        console = evidence / "console.jsonl"
        if console.exists():
            for line in console.read_text(encoding="utf-8", errors="replace").splitlines():
                try:
                    entry = json.loads(line)
                except ValueError:
                    continue
                if entry.get("level") in ("error", "warn", "window.error", "unhandledrejection", "alert", "gap", "capture-error"):
                    entries.append({"suite": suite_at(entry.get("time")), "source": "console", "time": entry.get("time"),
                                    "level": entry.get("level"), "text": str(entry.get("message", entry.get("dropped", "")))[:2000],
                                    "session": session["id"]})
        process = evidence / "process.log"
        if process.exists():
            for number, line in enumerate(process.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
                if pattern.search(line):
                    entries.append({"suite": "unknown", "source": "process", "line": number, "text": line[:2000],
                                    "session": session["id"]})
        actions = evidence / "actions.jsonl"
        if actions.exists():
            for line in actions.read_text(encoding="utf-8", errors="replace").splitlines():
                try:
                    entry = json.loads(line)
                except ValueError:
                    continue
                if entry.get("error"):
                    entries.append({"suite": suite_at(entry.get("time")), "source": "actions", "time": entry.get("time"),
                                    "text": f"{entry.get('command')} {entry.get('args')}: {entry['error']}"[:2000],
                                    "session": session["id"]})
    rss: dict[str, dict] = {}
    for csv_path in sorted(session_dir(run_dir).glob(f"*-{block}-samples.csv")):
        with open(csv_path, encoding="utf-8") as handle:
            for row in csv.DictReader(handle):
                if row.get("epoch") == "epoch" or not row.get("rss_kb"):
                    continue
                item = rss.setdefault(row["comm"], {"first_kb": int(row["rss_kb"]), "max_kb": 0, "last_kb": 0})
                item["max_kb"] = max(item["max_kb"], int(row["rss_kb"]))
                item["last_kb"] = int(row["rss_kb"])
    anomalies = [{"source": "sampler", "suite": "block", "text": f"{comm}: RSS {v['first_kb']} -> {v['last_kb']} kB (max {v['max_kb']})"}
                 for comm, v in rss.items() if v["first_kb"] and v["last_kb"] > 2 * v["first_kb"] and v["last_kb"] > 200_000]
    result = {"block": block, "generated": lib.now(), "intervals": intervals, "entries": entries[:3000],
              "dropped_entries": max(0, len(entries) - 3000), "rss": rss, "anomalies": anomalies}
    lib.write_json(run_dir / "qa" / "runs" / block / "triage-raw.json", result)
    return {"ok": True, "entries": len(entries), "anomalies": len(anomalies),
            "path": f"qa/runs/{block}/triage-raw.json"}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    sub = parser.add_subparsers(dest="command", required=True)
    prebuild = sub.add_parser("prebuild")
    prebuild.add_argument("--perf", action="store_true")
    start = sub.add_parser("start")
    start.add_argument("--block", required=True, choices=["S1", "S2", "S3", "S4"])
    start.add_argument("--perf", action="store_true")
    wait = sub.add_parser("wait")
    wait.add_argument("--timeout", type=int, default=540)
    sub.add_parser("stop")
    sub.add_parser("reap")
    guard = sub.add_parser("guard")
    guard.add_argument("--label", default="manual")
    mark = sub.add_parser("mark")
    mark.add_argument("--block", required=True, choices=["S1", "S2", "S3", "S4"])
    mark.add_argument("--suite", required=True)
    mark.add_argument("edge", choices=["begin", "end"])
    maintenance = sub.add_parser("maintenance")
    maintenance.add_argument("--minutes", type=int, required=True)
    tri = sub.add_parser("triage")
    tri.add_argument("--block", required=True, choices=["S1", "S2", "S3", "S4"])
    sub.add_parser("monitor")
    args = parser.parse_args(argv)
    try:
        repo = lib.repo_root(Path(__file__).resolve().parent)
        run_dir = lib.resolve_run(args.run, repo)
        if args.command == "prebuild":
            result = cmd_prebuild(repo, run_dir, args.perf)
        elif args.command == "start":
            result = cmd_start(repo, run_dir, args.block, args.perf)
        elif args.command == "wait":
            result = cmd_wait(repo, run_dir, args.timeout)
        elif args.command == "stop":
            result = cmd_stop(repo, run_dir)
        elif args.command == "reap":
            result = cmd_reap(repo, run_dir)
        elif args.command == "guard":
            run = lib.load_run(run_dir)
            result = dataset_guard.check(Path(run["dataset"]), run_dir / "context" / "dataset-manifest.json",
                                         run_dir, args.label)
            result = {k: result[k] for k in ("clean", "totals", "label")}
        elif args.command == "mark":
            result = cmd_mark(run_dir, args.block, args.suite, args.edge)
        elif args.command == "maintenance":
            result = cmd_maintenance(run_dir, args.minutes)
        elif args.command == "triage":
            result = triage(repo, run_dir, args.block)
        else:
            return monitor(repo, run_dir)
    except (lib.AuditError, OSError, subprocess.SubprocessError, KeyError) as error:
        print(json.dumps({"ok": False, "error": f"{type(error).__name__}: {error}"}), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result.get("ok", True) and result.get("clean", True) else 3


if __name__ == "__main__":
    sys.exit(main())
